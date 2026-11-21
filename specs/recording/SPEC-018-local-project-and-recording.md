# SPEC-018 — Local Projects and X11 Desktop Recording

**Status:** Approved for implementation  
**Milestone:** M2 — First usable recording slice  
**Approval basis:** Product owner instruction to continue implementation through a usable application, 2026-11-21  
**Related:** SPEC-001, SPEC-002, SPEC-003, SPEC-017, ADR-002, ADR-006

## Purpose

Deliver the first useful Tauri-native Screenstory workflow: create/open a local project, record the full desktop in an X11 session, stop/finalize the MP4, and persist the recording in that project's library.

## In scope

- Create, list, and open local projects.
- Store project metadata and recording index in SQLite; store video and log files on disk.
- Store projects beneath the user's Videos/Screenstory/Projects directory.
- Initialize per-project folders for recordings, analysis, timeline, assets, and exports.
- Record the full X11 desktop at 30 FPS using the system FFmpeg executable.
- Display recording state and elapsed time; stop/finalize MP4 output.
- Register a finalized recording in the project's database.
- Keep the original captured MP4 immutable for future editing/export.
- Block project switching while a recording is active.
- Detect missing FFmpeg and unsupported capture sessions before starting.

## Explicitly out of scope

- Selected-window recording; the current FFmpeg window-ID prototype failed resize/minimize lifecycle criteria.
- Wayland; XDG Desktop Portal and PipeWire need a separately validated adapter.
- AI analysis, Gemini integration, frame extraction, timeline editing, zoom/annotations, preview, and final tutorial rendering.
- Audio capture, pause/resume, webcam capture, global input hooks, and background recording.
- Automatic FFmpeg installation in this slice. The existing consent-based installer and progress requirement remains governed by SPEC-017 and must be completed before release.

## Capture constraints

- Only X11 desktop sessions are supported by this slice.
- The X11 DISPLAY must be available to the app.
- Screen dimensions must be detected before capture.
- FFmpeg must support x11grab and H.264/libx264; startup failure must be shown.
- Screenstory starts capture only after an explicit Start action.
- Stop sends FFmpeg's graceful quit command and waits for output finalization.
- A recording is registered only after the MP4 exists, is non-empty, and FFmpeg exits successfully.
- The UI clearly identifies full-desktop capture.

## Project storage

- Project IDs are application-generated and cannot contain path separators.
- Project names are 1–80 characters after trimming and cannot contain control characters.
- Project root is application-managed; the UI does not supply arbitrary filesystem paths.
- SQLite stores project and recording metadata; media/log assets stay on disk.
- Failed initial project creation removes its partially initialized folder.
- Projects are listed from the managed root and opened by validated project ID.

## Acceptance criteria

1. User can create a project and see it in the project list.
2. Project metadata is persisted across application restarts.
3. User can reopen an existing project and see its recording library.
4. Recording cannot start without an active project.
5. Recording is accepted only in X11 sessions with FFmpeg and detected screen dimensions.
6. Start launches FFmpeg without exposing arbitrary process execution to the frontend.
7. The UI shows recording state and elapsed time.
8. Stop finalizes a non-empty MP4 and stores its metadata in SQLite.
9. Project switching is blocked during recording.
10. Unsupported Wayland/window-capture cases are not advertised as supported.
11. Frontend type checks/tests, Rust formatting/tests/lint, and .deb packaging pass in CI.
12. A Linux Mint X11 smoke test must still verify actual capture quality and MP4 playback before this feature is called device-validated.

## Verification boundaries

CI verifies compilation, automated tests, and packaging. It cannot prove capture output quality on the product owner's desktop. That requires a real MP4 smoke test on the target environment. No Wayland or selected-window support is claimed.
