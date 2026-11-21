# Project State

**Project:** Screenstory  
**Baseline:** MVP v1.0  
**Current milestone:** M3 — post-recording AI analysis  
**Status:** M2 CI/package succeeded; target-device test pending. M3 backend and UI implementation are committed; latest CI rerun is testing Rust and packaging. M4 design is approved; implementation not started.

## Product scope

Linux-first desktop application that turns local screen recordings into editable AI-assisted tutorials. The original recording remains immutable. The product includes local project management, recording, optional workflow context, post-recording AI analysis, a versioned editable timeline, hybrid zoom, Arabic/English annotations, preview, and MP4 export.

## Architecture

Tauri v2 + React + TypeScript + Rust; FFmpeg for recording and video processing; SQLite for project/index metadata; filesystem for large media and versioned JSON. Python is not a mandatory desktop runtime.

## M1 — Desktop foundation

- Tauri v2 application shell, typed Rust bridge, frontend unit tests and Rust tests.
- Linux CI and Debian packaging.
- Relative Vite production asset paths for the Tauri local app URL.

## M2 — Local projects and X11 desktop recording

Specification: `specs/recording/SPEC-018-local-project-and-recording.md`.

Implemented:
- Create/list/open local projects, per-project SQLite index, managed local folders.
- Full-desktop FFmpeg x11grab, X11 only, 30 FPS; Start/Stop, elapsed state, MP4 finalization and recording library.
- Project switching blocked during recording; exit cleanup.
- The UI clearly states unsupported Wayland and selected-window capture.

Verification:
- Version 0.2.1, commit `153bbd4de27e7ee24955ab2d03d25a1f626cf54a`.
- GitHub Actions run `44939256173`: frontend typecheck/tests/build, Rust fmt/test/Clippy and Debian packaging succeeded.
- Artifact `screenstory-linux-deb`.
- Target-device test remains pending: install, capture a real MP4, verify playback, dimensions/frame rate, graceful stop and persistence.
- CI success is not evidence of a real capture on the target machine.

## M3 — Post-recording AI analysis

Approved specification: `specs/analysis/SPEC-019-post-recording-ai-analysis.md`.

Implemented on the current prototype branch:
- Per-project analysis-run persistence and cancellable run state.
- Managed recording resolution and local ffprobe inspection.
- Deterministic periodic frame extraction (up to 60 images), scaling, timestamped manifest and local retention.
- Gemini Interactions API request adapter, structured output schema, bounded retries and stateless `store=false`.
- Gemini API key save/read/delete through Linux Secret Service keyring; no plain-text fallback.
- Explicit cloud-transfer confirmation after local extraction; request contains sampled images and optional description, never the source MP4.
- Domain validation of step timestamps, evidence frame references, confidence and zoom geometry.
- Separate, immutable `needs_review` draft; re-analysis does not update a timeline.
- UI for key setup, recording analysis, extraction progress, cloud consent, provider state and review of generated steps.

**Verification status (latest run `44941901960`):**
- Frontend typecheck, unit tests, production build and Tauri asset check passed.
- Rust tests passed in the previous run `44941655377`.
- The only issue in that run was Clippy's `too_many_arguments` on two internal helpers; scoped, documented allowances were added in commit `2c6dc186ae54c47a933ca837f46548ccd077020f`.
- Latest CI is re-running native tests/lint and Debian packaging; do not call M3 released until the full run passes.
- A real Gemini request using a user-owned API key and a real recorded clip is not yet verified on the target device.
- In-flight provider cancellation is bounded by request timeout, not immediate.
- Model ID is currently a Rust default constant; per-user model configuration is not yet exposed.

## M4 — Versioned timeline and step editing

Approved specification: `specs/timeline/SPEC-020-versioned-timeline-and-step-editing.md`.

Decisions:
- User explicitly converts a completed AI draft into a separate editable timeline.
- Versioned JSON snapshots are stored under the project's managed timeline directory; SQLite tracks current revision.
- Each save uses an expected revision (optimistic concurrency) and writes a new atomic snapshot; prior revisions remain recoverable.
- User can add/edit/delete/reorder steps, subject to time/order/evidence validation.
- AI draft and original recording are immutable; re-analysis never overwrites timeline edits.
- M4 supports step editing; zoom acceptance/editing and annotations remain later milestones.

## Remaining milestones

1. Finish M3 CI and verify real analysis on target Linux Mint with a user-owned Gemini API key.
2. M4 — versioned timeline persistence and step editor.
3. M5 — accept/reject and edit zoom suggestions; manual zoom creation; bilingual RTL/LTR annotations and font/shaping verification.
4. M6 — source preview, composited timeline preview, deterministic MP4 rendering/export including audio preservation and output verification.
5. M7 — selected-window capture, followed by Wayland portal/PipeWire adapter.
6. M8 — dependency consent/progress installer, Linux Mint compatibility matrix, packaging/security/licensing/release hardening.

## Deferred

Windows/macOS, live AI, audio transcription, voice-over, cloud project storage, video upload to providers, webcam capture and PDF export.

## Release verification policy

Each milestone requires an approved specification, implementation, automated tests, CI packaging, and explicit verification evidence. A CI-built package does not prove target-device functionality. Never label the whole MVP complete until all required release acceptance criteria pass.
