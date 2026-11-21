# Project State

**Project:** Screenstory  
**Baseline:** MVP v1.0  
**Current milestone:** M3 — post-recording AI analysis  
**Status:** M2 implementation passed CI; target-device capture verification pending. M3 specification approved; implementation not yet started.

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

## Completed milestones

### M1 — desktop foundation

- Tauri v2 application shell and typed Rust bridge.
- React/TypeScript application, frontend tests, and Rust tests.
- Linux CI workflow and Debian packaging.
- Vite production asset paths use relative references for the Tauri local app URL.

### M2 — first usable local recording slice

Specification: `specs/recording/SPEC-018-local-project-and-recording.md`.

Implemented on `prototype/tauri-foundation`:
- Create, list, and open local projects.
- Per-project SQLite metadata and recording index.
- Filesystem project workspace beneath the managed Screenstory projects directory.
- Project folders for recordings, analysis, timeline, assets, and exports.
- Full-desktop FFmpeg x11grab recording in X11 sessions at 30 FPS.
- Recording status and elapsed timer.
- Graceful stop, MP4 finalization, and recording registration in the project library.
- Project switching is blocked while recording.
- UI explicitly identifies full-desktop X11 capture and does not claim Wayland or selected-window capture.
- Application-exit recording cleanup.

**Build evidence (2026-11-21):**
- App version: 0.2.1
- Commit: `153bbd4de27e7ee24955ab2d03d25a1f626cf54a`
- GitHub Actions run: `44939256173`
- Frontend typecheck, tests, and production build: passed.
- Rust format, tests, and Clippy: passed.
- Debian package build and package metadata verification: passed.
- Artifact: `screenstory-linux-deb`.

## Verification still required for M2

- Install and run 0.2.1 on the target Linux Mint machine.
- Capture a real short MP4 and verify playback, dimensions, frame rate, and graceful stop.
- Verify relaunch persistence and project recording-library refresh on the target machine.
- Complete SPEC-017 consent-based FFmpeg installation with visible progress before release.
- Validate generated Debian dependencies against the supported Linux Mint baselines.

CI success does not prove capture quality on the target device. Do not claim device validation until a real MP4 is captured and played there.

## M3 — post-recording AI analysis

Approved specification: `specs/analysis/SPEC-019-post-recording-ai-analysis.md`.

Implementation scope:
- User-initiated, persisted, cancellable analysis runs with visible progress.
- Local ffprobe metadata inspection and bounded, timestamped periodic frame extraction.
- Versioned frame manifest; original MP4 stays untouched and is never uploaded.
- Gemini Interactions API adapter using structured output and a provider-independent contract.
- Linux Secret Service/keyring storage for the user-provided API key; no plaintext fallback.
- Per-run cloud-transfer consent with disclosure of frames/text sent.
- Stateless Gemini request (`store=false`), bounded retries, normalized errors.
- Domain validation of timestamps, frame evidence, confidence, and normalized zoom geometry.
- Save AI output as an unapproved draft; re-analysis cannot overwrite earlier drafts or manual edits.

M3 intentionally does not include the timeline editor or MP4 tutorial renderer.

## Not implemented yet

- Selected-window recording and Wayland portal/PipeWire capture.
- M3 analysis implementation and Gemini integration.
- Persisted editable timeline editor, manual step editing, zoom editing, and Arabic/English annotations.
- Preview and final MP4 tutorial rendering/export.
- Audio capture, pause/resume, webcam capture, and background recording.
- Complete assisted FFmpeg installation with visible progress.

## Future milestones

1. M4 — versioned timeline persistence and step editing.
2. M5 — hybrid zoom editing and Arabic/English annotations.
3. M6 — video preview and deterministic MP4 tutorial export.
4. M7 — selected-window and Wayland capture adapters, after separate capability proofs.
5. M8 — release hardening, assisted runtime dependency installation, compatibility validation, and licensing/package review.

## Verification policy

Each milestone requires specification approval, implementation, automated tests, CI packaging, and explicit verification evidence. CI evidence is not a substitute for target-device behavior tests.
