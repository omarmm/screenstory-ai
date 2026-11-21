# Project State

**Project:** Screenstory  
**Baseline:** MVP v1.0  
**Current milestone:** M2 — local projects and X11 desktop recording  
**Status:** Implemented in code; CI verification passed; target-device smoke test remains pending.

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

## Verification still required

- Install and run 0.2.1 on the target Linux Mint machine.
- Capture a real short MP4 and verify playback, dimensions, frame rate, and graceful stop.
- Verify relaunch persistence and project recording-library refresh on the target machine.
- Complete SPEC-017 consent-based FFmpeg installation with visible progress before release.
- Validate generated Debian dependencies against the supported Linux Mint baselines.

CI success does not prove actual desktop capture quality on the target machine. Do not claim device validation until a real MP4 is captured and played there.

## Not implemented yet

- Selected-window recording and Wayland portal/PipeWire capture.
- Post-recording analysis orchestration and cancellation.
- Deterministic frame extraction, scene/visual-change sampling, and extraction manifest.
- Gemini provider integration, explicit cloud-transfer consent, and provider abstraction.
- Persisted, versioned editable tutorial timeline.
- Step editing, zoom editing, Arabic/English annotations.
- Preview and final tutorial MP4 rendering/export.
- Audio capture, pause/resume, webcam capture, and background recording.

## Next milestone — M3: analysis foundation

Before implementation, revise and approve the currently draft specifications:
- `specs/analysis/SPEC-004-video-analysis.md`
- `specs/analysis/SPEC-005-frame-extraction.md`
- `specs/analysis/SPEC-006-ai-analysis-contract.md`
- `specs/ai/SPEC-013-gemini-provider.md`
- `specs/ai/SPEC-014-provider-abstraction.md`

Recommended sequence:
1. Finalize the analysis input/output contract and timeline handoff.
2. Implement deterministic metadata inspection and bounded frame extraction with source timestamps and a manifest.
3. Add cancellable analysis-run lifecycle and visible progress.
4. Implement provider-independent AI contracts and Gemini behind explicit cloud-transfer consent.
5. Validate all AI output before creating or replacing a timeline; preserve manual edits and the immutable original recording.

## Verification policy

Each milestone requires specification approval, implementation, automated tests, CI packaging, and explicit verification evidence. CI evidence is not a substitute for target-device behavior tests.
