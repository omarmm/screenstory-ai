# Project State

**Project:** Screenstory  
**Baseline:** MVP v1.0  
**Current milestone:** M6 — video preview and MP4 tutorial export  
**Status:** M1–M5 implementation is committed. The 0.4.0 package build is in CI. M6 SPEC-022 is approved. M6-T301 render-plan/filter-generation foundation and the Linux renderer capability proof are being implemented; preview/export execution is not yet complete. Target-device smoke testing and a real Gemini request remain pending. This is not a completed MVP release.

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
- CI run `44943033679` passed frontend typecheck/unit tests/build, Rust tests/format/Clippy, Debian build and metadata verification.
- Artifact ID `14722368387`, package `screenstory 0.3.0 amd64`.
- Target-device persistence and real workflow testing remain pending.

## M5 — Hybrid zoom and bilingual annotation editing

Approved specification: `specs/timeline/SPEC-021-hybrid-zoom-and-annotations.md`.

Implemented:
- Timeline schema v2 with atomic migration from v1 snapshots to new immutable revisions; prior snapshots remain intact.
- AI zoom suggestions remain pending until explicit acceptance; users can accept/reject/reset and edit timing, normalized region, scale and transition duration.
- Users can add/remove manual zooms.
- Active zoom overlaps are rejected; pending/rejected AI suggestions are not considered active.
- Users can add/edit/remove Arabic, English, or mixed-language captions; set timing, direction, alignment, normalized position/width, font size, foreground and background.
- Rust validates caption text, allowed control characters, colors, language, timing, geometry, direction and alignment.
- UI saves edits as versioned timeline revisions. No original recording or analysis draft is mutated.
- M5 deliberately covers editing/persistence only. It does not yet composite zoom or caption overlays into video. Arabic shaping/glyph/font validation remains a mandatory renderer proof in M6, not claimed complete here.

Verification:
- CI run `44943760023` passed frontend and Linux Debian jobs, including Rust format, tests, Clippy, package build, metadata verification and upload.
- Artifact ID `14723146076`; SHA-256 `ba09e5fe1ced791c61c13de1f67d22005d8a9583872b94ecb69c9c5641ab6dde`.
- Artifact was downloaded and inspected: package `screenstory 0.3.0 amd64`, size 6,014,624 bytes.
- A further UI test covering accept-zoom and Arabic caption save was added after that run and is included in the 0.4.0 build workflow.
- Target-device RTL shaping and persisted edit behavior remain pending.

## M6 — Video preview and MP4 tutorial export

Specification: `specs/rendering/SPEC-022-video-preview-and-mp4-export.md` (Approved).

Approved rendering decisions:
- Immutable render plan derived from the saved timeline revision and managed recording ID.
- One deterministic filter graph shared by preview and export.
- Accepted AI/manual zooms only; pending/rejected AI suggestions are excluded.
- FFmpeg drawtext UTF-8 text files with required Arabic text-shaping and font capability checks. Fail closed if shaping is unavailable.
- Preserve source audio when present.
- Fixed FFmpeg argument vector, temporary output, ffprobe validation, fsync and atomic finalization.
- Export history tied to source recording and timeline revision.

Implementation:
- M6-T301 — render-plan and filter-generation foundation plus Linux capability proof: in progress.
- Preview/export process execution, progress, cancellation, persistent history, UI integration and full renderer verification remain open.

## Remaining milestones

1. M6 — complete the renderer proof, composited preview, deterministic MP4 export, audio preservation and output verification.
2. M7 — selected-window recording and Wayland XDG portal/PipeWire capture adapters.
3. M8 — complete the user-consented FFmpeg dependency installer with real progress, Linux Mint compatibility matrix, recovery/security/licensing/packaging/release hardening.
4. Target-device validation on Linux Mint for capture, persisted projects/timelines, Gemini, RTL shaping, and export.

## Deferred

Windows/macOS, live AI, speech transcription, voice-over, cloud project storage, full-source video uploads to providers, webcam capture and PDF export.

## Release verification policy

Each milestone requires an approved specification, implementation, automated tests, CI packaging, and explicit verification evidence. A CI-built package does not prove target-device behavior. Never label the whole MVP complete until all required release acceptance criteria pass.
