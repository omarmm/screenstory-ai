# Project State

**Project:** Screenstory  
**Baseline:** MVP v1.0  
**Status:** Production desktop stack accepted; SPEC-017 drafted and awaiting review.

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

Draft specification: SPEC-017 — Tauri Desktop Foundation. It covers the app shell, Rust command boundary, least-privilege capabilities, project workspace boundary, build prerequisites, and quality gates. It is not yet approved; no implementation task is Ready.

Next gate:
1. Review and approve SPEC-017.
2. Verify Tauri v2 Linux prerequisites, especially WebKitGTK 4.1 compatibility on the actual Linux Mint target.
3. Resolve the remaining M1 open decisions and define task handoffs.
4. Scaffold Tauri v2 + React + TypeScript + Rust only after approval and environment readiness.

No production capture port is authorized merely by creating the shell. Capture remains governed by SPEC-002 and must be integrated behind the unified adapter contract.

## Capture evidence and current direction

- X11 full-display capture: FFmpeg x11grab produced a playable recording in the Linux Mint + Cinnamon + X11 prototype.
- X11 window capture: FFmpeg x11grab -window_id produced a playable recording in the prototype. The current approach uses fixed initial dimensions and does not support resizing the target window during capture.
- XComposite experiment: attempted in the same target environment, but output was black. It is not considered validated or selected as a working implementation.
- The prototype includes refresh/error handling for stale or closed window selections and a larger default UI. These changes are on the prototype branch and still require local confirmation after pull.
- Wayland: XDG Desktop Portal ScreenCast + PipeWire remains the intended direction; validation is pending.
- The prototype is not production-ready and does not prove cross-distribution compatibility.

## Active implementation task

None. No implementation task is currently Ready or In Progress.

## Blockers / open questions

- Tauri v2 WebKitGTK 4.1 build prerequisites and compatibility with the installed Linux Mint version are not yet verified.
- The production X11 window-capture backend is unresolved: current x11grab window capture has a resize limitation; the XComposite experiment produced black output.
- Minimized/off-workspace X11 window behavior must be defined from evidence.
- Wayland portal/PipeWire behavior remains unvalidated on a target compositor.
- FFmpeg availability and required filter/font capabilities must be validated.
- Gemini model selection, API limits, and current pricing must be verified at implementation time.
- Packaging strategy (AppImage, deb, or both) remains open.

## Decision policy

Do not start implementation merely because the repository exists. Start only after the relevant detailed specification is approved and a task is marked Ready.
