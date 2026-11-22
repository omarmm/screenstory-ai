# Project State

**Project:** Screenstory  
**Baseline:** MVP v1.0  
**Current milestone:** M8 — user-consented runtime dependency installation and release hardening  
**Status:** The target Linux Mint device has installed and opened a locally built 0.5.3 Debian package (6.07 MiB). Clippy and local Debian packaging succeeded for the pre-patch 0.5.3 source. A formatting diff in `timeline.rs` was reported by `cargo fmt --check`; its correction is included in the current source patch. The latest Gemini retry-policy correction awaits local Rust tests, formatting, Clippy, a fresh package build, and target-device retesting. SPEC-024 remains partially implemented and its runtime dependency installer has not been exercised end to end. This branch is development source, not a verified release.

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
- Target-device Gemini analysis on 0.5.3 returned HTTP 503 (`service_unavailable`) on all three attempts, each with `Retry-After: 30`; no successful Gemini analysis has been verified.
- The previous retry helper capped provider-requested delays at 15 seconds. The follow-up patch waits the full numeric `Retry-After` value up to 60 seconds, defers instead of retrying early for longer or unparseable values, and checks cancellation during the wait. Local verification of this patch is pending.

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

Implemented on `prototype/tauri-foundation`:
- A saved-revision-bound render request resolved from the managed recording ID and the current validated timeline snapshot; no frontend file path is trusted.
- FFmpeg composition with frame-based zoompan, accepted AI/manual zooms only, UTF-8 ASS captions, Arabic bidi isolation and libass complex-shaping mode.
- Runtime font and FFmpeg capability checks; missing Arabic shaping/font support blocks captioned exports with an actionable error.
- Source audio mapping to AAC; MP4/H.264 output, ffprobe validation for video codec/dimensions/audio/duration, fsync and atomic finalization.
- Progress read from FFmpeg's machine-readable progress stream, cancellation, partial-file cleanup and persisted export history.
- Saved export preview embedded in the application through the narrowly-scoped Tauri asset protocol.
- Recovery of interrupted exports after application restart and retryable missing-output state.
- Free-space preflight, source dimension bounds, and FFmpeg process-group cleanup on application exit.

Automated verification added:
- Frontend test for the saved-revision export command/result flow.
- Rust tests for zoom expressions, accepted/rejected zoom selection, ASS timestamp/color/text escaping, Arabic caption generation, progress parsing and actual FFmpeg zoom + Arabic ASS composition when capabilities are available.
- CI capability proof installs FFmpeg, Fontconfig and Noto Core fonts, then verifies the ASS shaping option and Arabic font resolution.
- CI checks runs 44946417288 and 44946350274 are still in progress; do not treat earlier failed runs as release evidence.
- Real export playback and Arabic visual correctness on the target Linux Mint device remain unverified. The M6 artifact must be built from the latest passing commit, not the earlier 0.1.0 scaffold.

## M8 — User-consented runtime dependency installation

Approved specification: `specs/architecture/SPEC-024-user-consented-runtime-dependency-installation.md`.

Partially implemented on `prototype/tauri-foundation` (release source version 0.5.3):
- Debian package metadata declares mandatory Tauri Linux runtime libraries and PolicyKit; feature-specific FFmpeg and font packages remain optional.
- Runtime probes cover FFmpeg, FFprobe, H.264/AAC encoders, zoompan, ASS/libass, Noto Sans/Noto Sans Arabic, complex Arabic shaping, and the current X11/Wayland session capability.
- Recording, analysis, and export recheck the dependencies needed by that feature immediately before use.
- Missing APT-managed dependencies use a fixed package allowlist and an explicit user confirmation. Installation runs through PolicyKit, refreshes APT indexes, streams real package-manager output to the UI, and re-probes the requested capability.
- No shell interpolation, arbitrary package names, PPAs, repositories, or signing keys are added.

Not yet verified:
- The latest Gemini retry-policy patch has not yet passed local Rust tests, formatting, Clippy, or a fresh Debian build. GitHub Actions was intentionally not run.
- The installer has not yet been exercised on Linux Mint 22.3, including PolicyKit approval/cancellation, a package-manager lock, network failure, and partial install recovery.
- FFmpeg/libass/encoder/font capability probes and Arabic caption blocking need target-device verification.
- A 0.5.3 Debian package (6.07 MiB) was built locally, installed, and launched on the target device. It is not a release artifact; the retry-policy patch requires a fresh local build and device verification.

Do not mark SPEC-024 complete until the full acceptance criteria pass on supported Linux Mint versions. All implementation commits for this work include `[skip ci]`; no GitHub Actions build minutes have been used.

## Remaining milestones

1. M6 — complete the renderer proof, composited preview, deterministic MP4 export, audio preservation and output verification.
2. M7 — selected-window recording and Wayland XDG portal/PipeWire capture adapters.
3. M8 — verify and complete SPEC-024 on supported Linux Mint versions: user-consented runtime dependency installation, real package-manager progress, package-manager failure recovery, security, packaging, and release hardening.
4. Target-device validation on Linux Mint for capture, persisted projects/timelines, Gemini, RTL shaping, and export.

## Deferred

Windows/macOS, live AI, speech transcription, voice-over, cloud project storage, full-source video uploads to providers, webcam capture and PDF export.

## Release verification policy

Each milestone requires an approved specification, implementation, automated tests, CI packaging, and explicit verification evidence. A CI-built package does not prove target-device behavior. Never label the whole MVP complete until all required release acceptance criteria pass.
