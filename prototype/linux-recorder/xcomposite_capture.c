/*
 * Screenstory X11 window capture helper.
 * Captures a redirected window's off-screen Composite pixmap as RGB24 frames.
 * Original implementation; uses Xlib/XComposite APIs, no OBS source copied.
 * SPDX-License-Identifier: MIT
 */
#define _POSIX_C_SOURCE 200809L
#include <X11/Xlib.h>
#include <X11/Xutil.h>
#include <X11/extensions/Xcomposite.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <signal.h>

static volatile sig_atomic_t running = 1;
static volatile int x_error = 0;
static int on_xerror(Display *d, XErrorEvent *e) { (void)d; x_error = e->error_code; return 0; }
static void stop_signal(int sig) { (void)sig; running = 0; }

static unsigned char channel(unsigned long pixel, unsigned long mask) {
    if (!mask) return 0;
    unsigned int shift = 0;
    while (((mask >> shift) & 1UL) == 0UL) shift++;
    unsigned long value = (pixel & mask) >> shift;
    unsigned long max = mask >> shift;
    return (unsigned char)((value * 255UL) / max);
}

static void sleep_frame(void) {
    struct timespec ts = {0, 33333333};
    nanosleep(&ts, NULL);
}

static void black_frame(unsigned char *out, size_t bytes) {
    memset(out, 0, bytes);
}

int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: %s WINDOW_ID\n", argv[0]);
        return 2;
    }
    Window win = (Window)strtoul(argv[1], NULL, 0);
    if (!win) { fprintf(stderr, "invalid window id\n"); return 2; }

    Display *d = XOpenDisplay(NULL);
    if (!d) { fprintf(stderr, "cannot open X display\n"); return 3; }
    XSetErrorHandler(on_xerror);

    int ev, err, major, minor;
    if (!XCompositeQueryExtension(d, &ev, &err) ||
        !XCompositeQueryVersion(d, &major, &minor) ||
        (major == 0 && minor < 2)) {
        fprintf(stderr, "XComposite 0.2+ is unavailable\n");
        XCloseDisplay(d); return 4;
    }

    XWindowAttributes attr;
    x_error = 0;
    if (!XGetWindowAttributes(d, win, &attr) || x_error || attr.width < 1 || attr.height < 1) {
        fprintf(stderr, "window is unavailable\n"); XCloseDisplay(d); return 5;
    }
    const int out_w = attr.width, out_h = attr.height;
    const size_t out_bytes = (size_t)out_w * (size_t)out_h * 3U;
    unsigned char *out = malloc(out_bytes);
    if (!out) { fprintf(stderr, "out of memory\n"); XCloseDisplay(d); return 6; }

    fprintf(stderr, "XComposite capture started: window=0x%lx source=%dx%d output=%dx%d\\n",\n            win, attr.width, attr.height, out_w, out_h);\n    XSelectInput(d, win, StructureNotifyMask);
    x_error = 0;
    XCompositeRedirectWindow(d, win, CompositeRedirectAutomatic);
    XSync(d, False);
    if (x_error) {
        fprintf(stderr, "could not redirect window (X error %d)\n", x_error);
        free(out); XCloseDisplay(d); return 7;
    }

    Pixmap pix = None;
    int pix_w = 0, pix_h = 0;
    signal(SIGINT, stop_signal); signal(SIGTERM, stop_signal);

    while (running) {
        x_error = 0;
        if (!XGetWindowAttributes(d, win, &attr) || x_error) {
            fprintf(stderr, "target window was closed or destroyed\n");
            break;
        }
        if (attr.map_state != IsViewable) {
            /* Minimized/unmapped: keep the stream valid and wait for restore. */
            black_frame(out, out_bytes);
            if (fwrite(out, 1, out_bytes, stdout) != out_bytes) break;
            fflush(stdout); sleep_frame(); continue;
        }
        if (pix == None || attr.width != pix_w || attr.height != pix_h) {
            if (pix != None) XFreePixmap(d, pix);
            x_error = 0;
            pix = XCompositeNameWindowPixmap(d, win);
            XSync(d, False);
            if (x_error || pix == None) {
                fprintf(stderr, "could not acquire window pixmap (X error %d)\n", x_error);
                break;
            }
            pix_w = attr.width; pix_h = attr.height;
        }

        x_error = 0;
        XImage *img = XGetImage(d, pix, 0, 0, (unsigned int)pix_w,
                                (unsigned int)pix_h, AllPlanes, ZPixmap);
        XSync(d, False);
        if (!img || x_error) {
            if (img) XDestroyImage(img);
            fprintf(stderr, "could not read window frame (X error %d)\n", x_error);
            break;
        }

        /* Fit the changing source into the initial stream dimensions. */
        double scale_x = (double)out_w / (double)pix_w;
        double scale_y = (double)out_h / (double)pix_h;
        double scale = scale_x < scale_y ? scale_x : scale_y;
        int draw_w = (int)(pix_w * scale), draw_h = (int)(pix_h * scale);
        int off_x = (out_w - draw_w) / 2, off_y = (out_h - draw_h) / 2;
        black_frame(out, out_bytes);
        for (int y = 0; y < draw_h; y++) {
            int sy = (int)((double)y / scale);
            for (int x = 0; x < draw_w; x++) {
                int sx = (int)((double)x / scale);
                unsigned long p = XGetPixel(img, sx, sy);
                size_t i = ((size_t)(y + off_y) * (size_t)out_w + (size_t)(x + off_x)) * 3U;
                out[i] = channel(p, img->red_mask);
                out[i + 1] = channel(p, img->green_mask);
                out[i + 2] = channel(p, img->blue_mask);
            }
        }
        XDestroyImage(img);
        if (fwrite(out, 1, out_bytes, stdout) != out_bytes) break;
        fflush(stdout);
        sleep_frame();
        while (XPending(d)) {
            XEvent event; XNextEvent(d, &event);
            if (event.type == DestroyNotify) running = 0;
        }
    }

    if (pix != None) XFreePixmap(d, pix);
    XCompositeUnredirectWindow(d, win, CompositeRedirectAutomatic);
    XSync(d, False);
    free(out); XCloseDisplay(d);
    return running ? 8 : 0;
}
