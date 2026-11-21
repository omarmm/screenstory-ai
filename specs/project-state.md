# Project State

**Project:** Screenstory  
**Baseline:** MVP v1.0  
**Status:** M2 — first usable local recording slice implemented; CI verification pending.

## Product scope

- Linux-first desktop application.
- Full desktop and specific-window recording; specific-window backend remains unresolved.
- Optional workflow description.
- Post-recording AI analysis using Gemini initially.
- Provider-independent AI contracts.
- Hybrid zoom, editable timeline, Arabic/English annotations, and MP4 export.
- Local-first projects; original recording remains immutable.

## Approved architecture

Tauri v2 + React + TypeScript + Rust. FFmpeg handles video capture/processing. SQLite + filesystem store project metadata and large media. Python is not a mandatory desktop runtime.

## Completed foundation

- M1 Tauri shell and typed Rust bridge.
- Linux CI build and Debian packaging.
- Vite production asset paths use relative references for the Tauri local app URL.
- Initial 0.1.0 foundation .deb built successfully in CI.

## M2 — first usable local recording slice

SPEC-018 is approved for this implementation slice.

Implementation in progress on prototype/tauri-foundation:
- Local project create/list/open commands.
- Per-project SQLite metadata and recording index.
- Filesystem project workspace with recordings, analysis, timeline, assets, and exports folders.
- Full-desktop FFmpeg x11grab recording in X11 only.
- Recording status/timer, graceful stop, MP4 finalization, and project recording index.
- UI does not claim selected-window or Wayland capture.

Pending verification:
- CI Rust/frontend tests, lint, and Debian build.
- Linux Mint target-device smoke test for actual FFmpeg x11grab capture and MP4 playback.
- Application-exit cleanup while recording.
- FFmpeg dependency assisted installation with consent and progress (SPEC-017).
- Generated Debian dependency metadata validation on the supported Linux Mint baselines.

## Not implemented yet

- Selected-window recording and Wayland portal/PipeWire capture.
- AI video analysis, Gemini request flow, and frame extraction.
- Editable timeline, step editing, zoom editing, Arabic/English annotations.
- Preview and final MP4 tutorial rendering/export.
- Audio capture and pause/resume.
- Consent-based optional dependency installation with live progress.

## Verification policy

CI evidence is not equivalent to a local Linux Mint capture test. Do not claim recording is device-validated until a real MP4 is captured and played on the target environment.
