# X11 Capture Technical Spike — SPIKE-001

**Status:** Ready to execute — no production implementation
**Related specification:** SPEC-002
**Initial environment:** Linux Mint + Cinnamon + X11
**Purpose:** Verify whether FFmpeg `x11grab` is suitable for the MVP's X11 full-display and single-window capture paths.

## Safety and scope

- Run these commands in a terminal inside the logged-in X11 desktop session.
- This is a local proof of capability. It does not add application code or upload recordings.
- Record the actual FFmpeg version and results. Do not infer support from the distro name.
- Use a harmless test window; avoid capturing passwords or private content.

## 1. Environment and capability inventory

Run:

```bash
printf 'SESSION=%s\nDISPLAY=%s\n' "$XDG_SESSION_TYPE" "$DISPLAY"
uname -a
cat /etc/os-release | grep -E '^(PRETTY_NAME|VERSION_ID)='
ffmpeg -version | head -n 3
ffmpeg -hide_banner -devices 2>&1 | grep -E 'x11grab|xcb'
command -v xwininfo || true
command -v xdpyinfo || true
command -v ffprobe || true
```

Expected:
- `SESSION=x11`
- `DISPLAY` is non-empty.
- FFmpeg lists the `x11grab` input device.
- `xwininfo`, `xdpyinfo`, and `ffprobe` are available for the relevant tests.

If tools are missing, install the distro packages for FFmpeg, x11-utils, and ffmpeg/ffprobe as appropriate; record what was missing and installed. Do not treat a missing optional diagnostic utility as a capture failure.

## 2. Full-display capture

Run for 15 seconds:

```bash
mkdir -p "$HOME/Screenstory-spike"
ffmpeg -hide_banner -y -f x11grab -framerate 30 -draw_mouse 1 \
  -i "$DISPLAY" -t 15 -c:v libx264 -preset ultrafast \
  -pix_fmt yuv420p "$HOME/Screenstory-spike/x11-full-display.mp4"
```

During capture, move windows and the mouse, and scroll or type in a harmless test window.

Verify:

```bash
ffprobe -v error -show_entries format=duration:stream=codec_name,width,height,r_frame_rate \
  -of default=noprint_wrappers=1 "$HOME/Screenstory-spike/x11-full-display.mp4"
```

Pass criteria:
- FFmpeg exits successfully.
- The MP4 opens and plays.
- The expected desktop content and mouse pointer are visible.
- Duration is approximately 15 seconds and dimensions match the captured desktop.

## 3. Single-window capture

1. Open a harmless application window (for example, a text editor).
2. Run `xwininfo` and click the target window:

```bash
xwininfo
```

3. Copy the hexadecimal `Window id` from the output and run (replace the placeholder):

```bash
WINDOW_ID=0x0000000
ffmpeg -hide_banner -y -f x11grab -window_id "$WINDOW_ID" -framerate 30 \
  -draw_mouse 1 -i "$DISPLAY" -t 15 -c:v libx264 -preset ultrafast \
  -pix_fmt yuv420p "$HOME/Screenstory-spike/x11-window.mp4"
```

4. Verify the file with the same `ffprobe` command, replacing the filename.

Pass criteria:
- Only the selected window's contents are captured.
- The resulting MP4 opens and plays.
- The initial video dimensions match the window dimensions at capture start.

## 4. Window behavior matrix

Repeat a short window capture for each case and record the observed result:

| Scenario | Expected observation to verify |
|---|---|
| Move the selected window | Does capture follow the window or remain at its original location? |
| Enlarge the selected window | Does the output include the newly exposed area? |
| Shrink the selected window | Does capture continue, crop, or terminate? |
| Minimize the selected window | Does capture continue or terminate? |
| Close the selected window | Does FFmpeg terminate cleanly and leave a playable file? |

Important: FFmpeg's documented `window_id` behavior says enlarging the window does not add the newly exposed area; closing or unmapping it ends capture, and shrinking below the captured size can also end capture. Treat this as a known limitation to verify, not as a behavior the MVP can silently promise to solve.

## 5. Evidence to report

Return:
- Environment inventory output from section 1.
- Whether full-display capture passed.
- Whether window capture passed.
- Results for all five window behavior cases.
- `ffprobe` output for both MP4 files.
- Any FFmpeg errors.

Do not upload the MP4s unless specifically requested; command output and behavior notes are enough for the first review.

## Exit decision

- **Pass:** Full-display and initial-size window capture work and produce playable MP4s; limitations are documented.
- **Conditional pass:** Full-display works but window capture or one of its lifecycle behaviors is unsuitable. Reassess the X11 window-capture mechanism before implementation.
- **Fail:** `x11grab` is unavailable or cannot produce a playable full-display recording in the target environment.

This spike does not validate Wayland, multi-monitor selection, audio capture, hotplug behavior, or production performance. Those remain separate validation items.
