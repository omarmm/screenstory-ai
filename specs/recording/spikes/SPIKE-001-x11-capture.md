# X11 Capture Technical Spike — SPIKE-001

**Status:** Initial FFmpeg path evaluated; XComposite follow-up required
**Related specification:** SPEC-002
**Initial environment:** Linux Mint + Cinnamon + X11
**Purpose:** Evaluate X11 display and application-window capture mechanisms for Screenstory.

## Safety and scope

- Run diagnostics inside the logged-in X11 desktop session.
- This is a local proof of capability. It does not add application code or upload recordings.
- Use a harmless test window; avoid capturing passwords or private content.
- Record actual versions, outputs, and observed behavior. Do not infer support from the distro name.

## 1. Environment and capability inventory

The initial environment was Linux Mint + Cinnamon + X11, with `DISPLAY=:0`, FFmpeg 6.1.1, and a 1920x1080 display. FFmpeg exposed `x11grab`; `xwininfo`, `wmctrl`, `xprop`, `xrandr`, and `ffprobe` were available. `xdotool` was not installed and was not required for the tests completed.

## 2. FFmpeg full-display capture

**Result: PASS**

A 15-second full-display recording was produced at 1920x1080, 30 fps, H.264. The output was playable and its duration/metadata were verified with ffprobe.

## 3. FFmpeg window capture using `-window_id`

**Result: CONDITIONAL / unsuitable as the production window-capture path**

- Initial capture produced a playable video at the target window's initial dimensions.
- Moving the target window appeared to be followed in playback, but FFmpeg emitted an X11 image-data/shared-memory warning.
- Resizing caused X11 image-data errors and demuxing to stop after approximately 1.53 seconds.
- Minimizing caused capture to stop after approximately 2.53 seconds with image-data/demuxer errors.
- Closing the target caused capture to stop when the window ceased to exist. This is expected termination for a destroyed target, not a capture defect by itself.
- `-use_shm 0` was not accepted by the installed FFmpeg 6.1.1 build.
- FFmpeg's documented `x11grab` `window_id` behavior states that enlarging a window does not add the newly exposed area and that closing/unmapping the window ends capture; shrinking below the captured size may also end capture.

Conclusion: keep FFmpeg `x11grab` for the validated full-display path, but do not use `x11grab -window_id` for production window capture.

## 4. Full-screen capture plus manual geometry/crop investigation

A full-screen 1920x1080 recording was produced successfully. Window geometry was inspected using `wmctrl`, `xwininfo`, `xprop`, and the X11 parent/child window tree.

The investigation found differing coordinate values between `wmctrl`, the client window, and its window-manager parent frame. A 10-second crop command produced a 1245x776 MP4, but the command was rerun without re-establishing the geometry variables in the same shell. Therefore the crop's alignment was not proven.

Conclusion: do not adopt manually tracked geometry plus FFmpeg crop as the production window-capture architecture. This approach adds avoidable coordinate/frame/lifecycle complexity and has not been validated end-to-end.

## 5. Selected follow-up: XComposite

OBS Studio documents a Linux X11 Window Capture source based on XComposite. Its Linux capture module uses X11/XCB Composite-related capabilities and includes window discovery, lifecycle observation, and reacquisition logic. This establishes XComposite as an existing, practical mechanism worth validating for Screenstory.

This is an architectural reference only. Do not copy OBS source code or add an OBS dependency without a separate license and dependency review.

### Required XComposite validation

1. Confirm the XComposite extension is available in the current X11 session.
2. Identify and enumerate top-level application windows.
3. Capture Firefox or another harmless test window.
4. Place another window over the target and verify the selected window remains the captured content.
5. Move and resize the target during capture; verify output content and dimensions.
6. Minimize, restore, close, and reopen the target; record exact lifecycle behavior.
7. Verify cursor handling, frame pacing, and playable output.
8. Identify the minimum runtime libraries and license-compatible Rust/native integration options.

## 6. Wayland follow-up

Wayland is a separate adapter. Validate source selection and permission flow through XDG Desktop Portal ScreenCast and frame delivery through PipeWire on at least one target compositor before claiming Wayland support. Do not infer Wayland behavior from the X11 results.

## Evidence and exit decision

- X11 full-display via FFmpeg `x11grab`: PASS.
- X11 initial-size `x11grab -window_id`: playable, but resize/minimize behavior unsuitable.
- Manual full-screen-plus-crop: NOT VALIDATED; not selected.
- XComposite window capture: NOT YET TESTED; next technical validation target.
- Wayland portal/PipeWire: NOT YET TESTED.

The X11 capture spike remains open until the XComposite validation is complete. The evidence above does not authorize production implementation by itself.
