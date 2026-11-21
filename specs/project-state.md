# Project State

**Project:** Screenstory  
**Baseline:** MVP v1.0  
**Current milestone:** M4 — versioned timeline and step editor  
**Status:** M4 implementation passed CI and a Debian package has been produced. Target-device smoke testing and a real Gemini request are still pending. This is a verified development build, not a completed MVP release.

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
- Target-device test remains pending: install, capture a real MP4, verify playback, dimensions/frame rate, graceful stop and persistence.
- CI success is not evidence of a real capture on the target machine.

## M3 — Post-recording AI analysis

Approved specification: `specs/analysis/SPEC-019-post-recording-ai-analysis.md`.

Implemented:
- Per-project analysis-run persistence and cancellation state.
- Managed recording resolution and local ffprobe inspection.
- Deterministic periodic frame extraction (up to 60 images), resizing, timestamped manifest and local retention.
- Gemini Interactions API adapter with structured output, bounded retries and stateless `store=false`.
- Gemini API key save/read/delete through Linux Secret Service keyring; no plain-text fallback.
- Explicit per-run cloud-transfer confirmation after local extraction; request contains sampled images and optional description, never the source MP4.
- Validation of step timestamps, evidence frame references, confidence and zoom geometry.
- Separate, immutable `needs_review` draft; re-analysis does not update a timeline.
- UI for key setup, recording analysis, extraction progress, cloud consent, provider state and review of generated steps.
- Correct parsing of Gemini REST `steps[].content[]` model-output text, covered by a Rust unit test.

Verification:
- CI run `44941901960` passed all frontend and Linux Debian jobs, including Rust tests, Clippy, package build, metadata check and artifact upload.
- The response parser fix was included in run `44943033679`, which passed frontend checks, Rust tests, Clippy, Debian build, metadata verification and artifact upload.
- Model ID remains a default Rust constant, not user-selectable yet.
- Analysis-run history recovery after app restart, immediate cancellation of a blocking provider request, and rich usage/cost reporting remain outstanding.
- A real Gemini request using a user-owned API key and real source recording has not been verified on the target device.

## M4 — Versioned timeline and step editing

Approved specification: `specs/timeline/SPEC-020-versioned-timeline-and-step-editing.md`.

Implemented:
- Separate editable timeline created only by explicit user action from a completed AI draft.
- JSON snapshots under the managed project timeline folder and a SQLite current-revision pointer.
- Immutable prior revisions, atomic snapshot writing, optimistic expected-revision saves, and SQLite write serialization.
- Rust validation for schema, source identity, ordered/non-overlapping step ranges, source evidence, AI/manual origin fields and zoom geometry/status.
- Frontend can create/open a timeline, edit step titles/descriptions/timestamps, add/remove manual steps, sort by time, save new revisions and reload latest revision.
- AI zoom suggestions are copied as `pending`; none is automatically rendered.
- Original recording and source analysis draft remain unchanged.

Verification:
- Rust timeline validation tests passed in CI run `44942407655`.
- Latest CI run `44943033679` passed frontend typecheck, unit tests (including the timeline edit/save path), frontend build, Tauri asset validation, Rust tests, Rust formatting, Clippy, Debian build and package metadata verification.
- Artifact `screenstory-linux-deb`, ID `14722368387`, SHA-256 `3ca901e8a290aaeeba1a776642aa254004ff9fd320d50ccb351e6855769c3655`.
- Built package metadata: `screenstory 0.3.0 amd64`, dependencies include WebKitGTK 4.1, GTK 3 and D-Bus.
- Target-device persistence and real workflow testing remain pending.

## Approved next specifications

- M5: `specs/timeline/SPEC-021-hybrid-zoom-and-annotations.md` — accept/reject/edit zooms, manual zoom, bilingual RTL/LTR annotations and versioned timeline schema migration. Approved for implementation after M4 validation.
- M6: `specs/rendering/SPEC-022-video-preview-and-mp4-export.md` — draft; implementation gated by M5 approval and a renderer proof.

## Remaining milestones

1. Target-device verification of M2–M4 on supported Linux Mint: record, play MP4, restart app, inspect timeline, and run a real Gemini analysis with a user-provided key.
2. M5 — hybrid zoom editing, Arabic/English annotations, shaping and schema migration.
3. M6 — source/composited preview, deterministic MP4 rendering/export, audio preservation and output verification.
4. M7 — selected-window recording and Wayland XDG portal/PipeWire capture adapters.
5. M8 — complete user-consented FFmpeg dependency installer with real progress, Linux Mint compatibility matrix, recovery/security/licensing/packaging/release hardening.

## Deferred

Windows/macOS, live AI, audio transcription, voice-over, cloud project storage, full-source video uploads to providers, webcam capture and PDF export.

## Release verification policy

Each milestone requires an approved specification, implementation, automated tests, CI packaging, and explicit verification evidence. A CI-built package does not prove target-device behavior. Never label the whole MVP complete until all required release acceptance criteria pass.
