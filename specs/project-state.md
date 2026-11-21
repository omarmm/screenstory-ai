# Project State

**Project:** Screenstory  
**Baseline:** MVP v1.0  
**Status:** M1 specification approved; Tauri foundation prototype authorized.

## Approved product scope

- Linux-first desktop application.
- Full desktop and specific-window recording.
- Optional workflow description.
- Post-recording AI analysis.
- Gemini as the initial AI provider.
- Provider-independent AI contracts.
- Hybrid zoom: AI suggestions plus manual editing.
- Editable timeline.
- Text annotations in Arabic and English.
- MP4 export.
- Local-first project storage.
- Original recording remains immutable.

## Deferred scope

- Ollama implementation.
- Reading project specifications or source repositories.
- Autonomous computer-use agents.
- Live AI monitoring.
- Automatic voice-over.
- Speech transcription.
- Windows/macOS support.
- Cloud project storage.
- PDF export.
- Mandatory Python backend/worker (ADR-010).

## Approved architecture

ADR-001 selects Tauri v2 + React + TypeScript + Rust for the production desktop application. ADR-010 confirms that a Python backend is not required for the MVP. FFmpeg remains the video processing/encoding tool. SQLite + filesystem remain the persistence direction.

The Python/Tkinter recorder remains a prototype and behavioral reference until the Tauri application passes equivalent capture acceptance tests. It is not the production architecture.

## Current milestone

**M1 — Desktop Foundation.**

SPEC-017 is Approved. It defines the Tauri shell, Rust command boundary, least-privilege capabilities, dependency readiness, explicit user-consented feature dependency installation, CI builds, and Debian packaging.

Approved implementation choices:
- pnpm with a committed lockfile for the frontend.
- Vitest + React Testing Library for frontend tests.
- Typed application error codes and safe messages; thiserror for internal Rust error composition.
- Project creation remains in SPEC-001 and is not faked in the M1 empty state.
- Initial x86_64 support baseline: Linux Mint 21.3+ (Ubuntu 22.04 base) and Linux Mint 22.x (Ubuntu 24.04 base), subject to CI/package verification.
- Primary package: .deb. Flatpak is deferred until the .deb path is proven.

## Prototype status

A separate Tauri foundation prototype branch is authorized. It will contain only the initial desktop shell and a minimal typed Rust bridge. It will not claim capture, project creation, AI, or export functionality. CI/build evidence must be reported separately from local-device testing.

The existing Python/Tkinter recorder remains the only demonstrated capture prototype. Its X11 window capture has a fixed-size limitation; the XComposite experiment produced black frames. No second capture prototype has been validated.

## Capture evidence and current direction

- X11 full-display capture: FFmpeg x11grab produced a playable recording in the Linux Mint + Cinnamon + X11 prototype.
- X11 window capture: FFmpeg x11grab -window_id produced a playable recording in the prototype. The current approach uses fixed initial dimensions and does not support resizing the target window during capture.
- XComposite experiment: attempted in the same target environment, but output was black. It is not considered validated or selected as a working implementation.
- The prototype includes refresh/error handling for stale or closed window selections and a larger default UI. These changes are on the prototype branch and still require local confirmation after pull.
- Wayland: XDG Desktop Portal ScreenCast + PipeWire remains the intended direction; validation is pending.
- The prototype is not production-ready and does not prove cross-distribution compatibility.

## Active tasks

- M1-T1 — CI/build baseline: Ready.
- M1-T2 — Tauri application scaffold: Blocked on M1-T1 build baseline.
- M1-T3 — Typed Rust bridge and least-privilege capabilities: Blocked on M1-T2.
- M1-T4 — Runtime dependency readiness and user-consented installation: Blocked on M1-T3.
- M1-T5 — Initial empty-state shell: Blocked on M1-T2.
- M1-T6 — Quality and .deb packaging gates: Blocked on M1-T1 through M1-T5.

## Dependency policy

- Build-time toolchains and development headers belong in CI, not on end-user machines.
- Mandatory libraries needed before the Tauri window can launch must be declared in package metadata and resolved by the OS package manager during installation.
- Optional/feature runtime dependencies are checked by the app. Missing dependencies trigger a clear user-consent prompt; no package operation or privilege escalation occurs without explicit approval.
- Linux Mint's primary distribution path is a .deb package. The generated package metadata must declare the Tauri runtime libraries it needs.

## Blockers / open questions

- The production X11 window-capture backend is unresolved: current x11grab window capture has a resize limitation; the XComposite experiment produced black output.
- Minimized/off-workspace X11 window behavior must be defined from evidence.
- Wayland portal/PipeWire behavior remains unvalidated on a target compositor.
- FFmpeg runtime package/version policy and required filter/font capabilities must be validated.
- Gemini model selection, API limits, and current pricing must be verified at implementation time.

## Decision policy

No implementation may exceed the approved scope of its parent specification. Each task requires a handoff, acceptance criteria, tests, and recorded verification evidence before closure.
