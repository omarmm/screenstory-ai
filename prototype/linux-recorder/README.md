# Screenstory — first-day recorder prototype

This is the first runnable product slice for the Linux Mint + Cinnamon + X11 development environment.

## What works in this prototype

- Desktop GUI with Start and Stop controls.
- Full-desktop X11 recording at 30 FPS.
- Local MP4 output using the installed FFmpeg and H.264 encoder.
- Elapsed recording timer and output-folder selection.
- Local-only operation; no AI or network upload.

## Deliberately not included yet

- Specific-window capture (XComposite follow-up).
- Wayland capture.
- AI analysis, generated steps, timeline editing, zooms, annotations, and final tutorial rendering.
- Project database and multi-project management.

This is a fast, disposable vertical slice to validate the product's first user journey. It does not replace the approved Tauri v2 + React + Rust architecture.

## Requirements

- Linux X11 desktop session.
- Python 3 with Tkinter.
- FFmpeg with `x11grab` input and `libx264` encoder.
- `xrandr` or `xdpyinfo` for screen-size detection.

On Debian/Ubuntu-based systems, if Tkinter is missing, install it explicitly with:

```bash
sudo apt install python3-tk
```

FFmpeg must be installed separately if it is missing. The app does not install packages or request elevated privileges.

## Run

From the repository root:

```bash
python3 prototype/linux-recorder/screenstory.py
```

Recordings are saved by default to `~/Videos/Screenstory`. Choose another folder with Browse.

## Verify an output

```bash
ffprobe -v error -show_entries format=duration:stream=codec_name,width,height,r_frame_rate -of default=noprint_wrappers=1 /path/to/Screenstory-YYYYMMDD-HHMMSS.mp4
```

## Known limitations

- X11 only; the app intentionally refuses non-X11 sessions.
- Full desktop only; window selection is not represented as available yet.
- Audio is disabled in this first slice.
- A screen-resolution change during recording is not handled.
- This prototype has not been executed on the target machine from this branch yet.
