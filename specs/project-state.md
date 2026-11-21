# Project State

**Project:** Screenstory  
**Baseline:** MVP v1.0  
**Current milestone:** M4 — versioned timeline and step editor  
**Status:** M2 implementation packaged and CI verified; target-device smoke test pending. M3 analysis implementation packaged and CI verified on 0.3.0. M4 backend/frontend implementation committed; CI verification is running on the latest revision. No target-device or live Gemini-request verification has been performed yet.

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
- Project switching blocked while recording; exit cleanup.
- The UI clearly states unsupported Wayland and selected-window capture.

Verification:
- Version 0.2.1, commit `153bbd4de27e7ee24955ab2d03d25a1f626cf54a`.
- GitHub Actions run `44939256173`: frontend typecheck/tests/build, Rust fmt/test/Clippy and Debian packaging succeeded.
- Target-device test remains pending: install, capture a real MP4, verify playback, dimensions/frame rate, graceful stop and persistence.
- CI success is not evidence of a real capture on the target machine.

## M3 — Post-recording AI analysis

Approved specification: `specs/analysis/SPEC-019-post-recording-ai-analysis.md`.

Implemented on `prototype/tauri-foundation`:
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

**Verification:**
- CI run `44941901960` passed all frontend and Linux Debian jobs, including Rust tests, Clippy, package build, metadata check and artifact upload (0.3.0).
- Latest run `44943033679` includes the Gemini response parser fix and M4 code. Frontend typecheck/tests/build and Tauri asset validation passed; Rust tests and Clippy passed; Debian package build/metadata/upload is the remaining in-progress CI step at the time of this update.
- A real Gemini request using a user-owned API key and a real recorded clip is not yet verified on the target device.
- Provider request cancellation is bounded by the 120-second network timeout, not immediate.
- Model ID is currently a Rust default constant; user-selectable model configuration is not exposed yet.
- Analysis-run history recovery across app restarts and rich usage/cost reporting remain outstanding.

## M4 — Versioned timeline and step editing

Approved specification: `specs/timeline/SPEC-020-versioned-timeline-and-step-editing.md`.

Implemented in source:
- Separate editable timeline created only by explicit user action from a completed AI draft.
- JSON snapshots under the managed project timeline folder and a SQLite current-revision pointer.
- Immutable prior revisions, atomic snapshot writing, optimistic expected-revision saves, and SQLite write serialization.
- Rust validation for schema, source identity, ordered/non-overlapping step ranges, source evidence, AI/manual origin fields and zoom geometry/status.
- Frontend can create/open a timeline, edit step titles/descriptions/timestamps, add/remove manual steps, sort by time, save new revisions and reload the latest revision after conflicts.
- AI zoom suggestions are copied to the timeline as `pending`; no pending suggestion is automatically rendered.
- Original recording and source analysis draft remain unchanged.

**Verification:**
- Rust tests for timeline schema validation, duration bounds, ordering/overlap and evidence references passed in CI run `44942407655`.
- Latest run `44943033679` has frontend unit tests (including timeline edit/save), typecheck, frontend build, Rust tests and Clippy passed. Debian build and artifact upload are still running.
- Target-device persistence and real workflow testing remain pending.

## Approved next specifications

- M5: `specs/timeline/SPEC-021-hybrid-zoom-and-annotations.md` — accept/reject/edit zooms, manual zoom, bilingual RTL/LTR annotations and versioned timeline schema migration. Approved for implementation after M4 validation.
- M6: `specs/rendering/SPEC-022-video-preview-and-mp4-export.md` — draft; implementation gated by M5 approval and a renderer proof.

## Remaining milestones

1. Complete M4 CI packaging and verify a real project/recording/AI timeline flow on target Linux Mint.
2. M5 — hybrid zoom editing, Arabic/English annotations, shaping and schema migration.
3. M6 — source/composited preview, deterministic MP4 rendering/export, audio preservation and output verification.
4. M7 — selected-window recording and Wayland XDG portal/PipeWire capture adapters.
5. M8 — complete user-consented FFmpeg dependency installer with real progress, Linux Mint compatibility matrix, recovery/security/licensing/packaging/release hardening.

## Deferred

Windows/macOS, live AI, audio transcription, voice-over, cloud project storage, full-source video uploads to providers, webcam capture and PDF export.

## Release verification policy

Each milestone requires an approved specification, implementation, automated tests, CI packaging, and explicit verification evidence. A CI-built package does not prove target-device behavior. Never label the whole MVP complete until all required release acceptance criteria pass.
