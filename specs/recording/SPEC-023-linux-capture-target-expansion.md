# SPEC-023 — Linux Capture Target Expansion

**Status:** Approved for implementation after M6 renderer validation  
**Milestone:** M7

## 1. Purpose

Expand recording beyond full-desktop X11 capture while preserving an explicit capability boundary between X11 and Wayland. Capture sources must be selected by the user and identified by a stable, session-scoped descriptor; the frontend never supplies raw FFmpeg arguments or arbitrary capture paths.

## 2. Capture source model

A capture source has a stable session-scoped ID, display name, kind (`display` or `window`), backend (`x11` or `portal_pipewire`), geometry when available, and capability flags. IDs expire when the enumeration session ends; stale IDs are rejected.

### X11

- Enumerate visible application windows and displays using X11-native APIs, not parsing localized command output.
- Include window title, application/process hint when safely available, geometry, and minimized/visible state.
- Refresh the list before recording and validate that the window still exists.
- Selected-window recording uses FFmpeg `x11grab` with a validated window ID and initial geometry. No shell command string is constructed.
- v1 limitation: selected-window geometry is fixed at recording start. If the target is destroyed or becomes unavailable, stop capture safely and finalize/report the segment. Resizing is not silently cropped or stretched.
- Full-desktop recording remains supported.

### Wayland / XDG Desktop Portal

- Use the ScreenCast portal (CreateSession → SelectSources → Start) with explicit user source-selection dialog.
- Obtain the PipeWire remote/node information through the portal session and connect to the selected stream using the PipeWire API.
- Do not use compositor-specific private APIs, screen scraping, XWayland fallback, or unauthorised capture.
- Portal cancellation, permission denial, missing portal service, PipeWire disconnect, and session revocation become typed, actionable errors.
- The app must release portal session and PipeWire resources on stop, cancellation, crash recovery and application exit.
- Only claim Wayland support after an end-to-end desktop session proves capture, stop and MP4 finalization on a supported compositor.

## 3. Lifecycle

- Recording still owns the single active capture session and prevents project switching.
- Start is acknowledged only after the selected backend has established a valid stream and FFmpeg accepts the input.
- Stop terminates the active backend, waits for process finalization and verifies the MP4 with ffprobe before indexing.
- On backend loss, keep already captured media, record the actual completed duration, and expose a recoverable status; never mark a zero-byte/invalid MP4 as a successful recording.
- Record capture backend and source type in recording metadata.
- Do not silently switch from selected window to full desktop.

## 4. Security and privacy

- Window/display enumeration is local.
- Portal capture remains OS-mediated and consent-based.
- No external service receives screen pixels during recording.
- Treat window titles and app names as untrusted display data.
- Pass FFmpeg inputs as fixed arguments; no shell interpolation.
- Keep source recording paths within the managed project recording folder.

## 5. Acceptance criteria

1. User can select full desktop or a specific visible X11 window.
2. Stale/destroyed window IDs are rejected before capture.
3. Selected-window capture records the intended window and labels the limitation on fixed geometry.
4. Window destruction or capture loss finalizes/reports safely without corrupting the project.
5. Existing full-desktop X11 recording remains regression-tested.
6. Wayland source selection uses the XDG ScreenCast portal and PipeWire, with no silent or unauthorised fallback.
7. Portal denial/cancel/disconnect and app exit release all capture resources.
8. MP4 finalization and recording persistence are verified on real X11 and Wayland desktop sessions separately.
9. Unsupported capture modes are disabled with a reason, not displayed as working controls.
