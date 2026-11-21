# ADR-001 — Desktop Framework

**Status:** Accepted  
**Date:** 2026-11-21  
**Related:** ADR-010, ARCH-001, PRD-001, SPEC-002

## Context

Screenstory is a Linux-first desktop application for recording the desktop or a selected window, analyzing the resulting video, and producing an editable tutorial with a rich timeline, preview, annotations, zoom controls, and MP4 export.

The working Python/Tkinter prototype has validated important recording workflows on Linux Mint + Cinnamon + X11. It is useful evidence and a behavioral reference, but Tkinter is not the chosen foundation for the production editor UI. The production application needs a maintainable separation between presentation, domain logic, operating-system integration, and media processing.

## Decision

Adopt the following production stack:

| Concern | Decision |
|---|---|
| Desktop shell | Tauri v2 |
| Frontend | React + TypeScript |
| Native core | Rust |
| Video processing and encoding | FFmpeg |
| Persistence | SQLite + filesystem |
| Initial cloud AI provider | Gemini, behind provider-independent contracts |
| Future local AI provider | Ollama, deferred |

The application will use a narrow, explicitly permissioned Tauri command boundary. React is responsible for presentation and interaction; Rust owns domain validation, application orchestration, project lifecycle, process supervision, and native platform adapters.

FFmpeg remains responsible for media probing, frame extraction, transformations, and encoding. Rust coordinates these operations; the decision does not imply reimplementing codecs or video processing in Rust.

Python is not part of the required production desktop runtime or core, consistent with ADR-010. A separate worker may be reconsidered only for a concrete capability with a measurable benefit and a versioned contract. The current Python/Tkinter prototype remains a behavioral reference until the production application passes equivalent recording tests.

## Rationale

- React + TypeScript is suitable for a complex editor-style interface and interactive timeline.
- Tauri provides a desktop shell and controlled native bridge without requiring the application to bundle a complete Chromium browser.
- Rust provides a compiled native core with strong type and memory-safety guarantees for application and platform integration code.
- Separating UI, domain services, platform capture, and FFmpeg orchestration keeps capture replaceable and the application testable.
- Narrow Tauri capabilities support a security boundary between frontend presentation and native operations.

## Alternatives considered

### Python + Tkinter
Retain as the validated prototype, not the production UI. It is useful for proving capture behavior quickly, but is not selected for the timeline/editor experience.

### Python + PySide6 / Qt
A credible alternative with a mature desktop UI toolkit. The project chooses the Tauri/Rust boundary and frontend ecosystem instead.

### Electron + React + TypeScript
A viable desktop option, but it bundles Chromium and is not selected for this Linux-first product.

### Python backend plus Tauri frontend
Not selected as the default because it introduces a second long-lived runtime and IPC boundary before a concrete need has been demonstrated.

## Consequences and risks

- The team must learn and maintain Rust alongside TypeScript.
- Tauri v2 Linux development requires WebKitGTK 4.1 system dependencies and a native build toolchain.
- The actual Linux Mint target must pass a prerequisite/build compatibility check before the local development environment is considered ready.
- Tauri does not itself provide screen capture; X11 and Wayland adapters remain separate engineering work.
- Linux packaging and dependency distribution need explicit validation.
- The application must maintain a strict, tested Tauri capability and command policy.

## Validation gates

1. Confirm Tauri v2 prerequisites on the actual Linux Mint development machine, including WebKitGTK 4.1 availability.
2. Build and launch a minimal Tauri v2 + React + TypeScript application on the target machine.
3. Verify the Rust command bridge and deny-by-default capability configuration.
4. Validate packaging on the target Linux distribution before choosing the release format.
5. Keep the Python prototype as the capture behavior reference until parity tests pass.
6. Do not claim production X11 window capture or Wayland support until SPEC-002 acceptance evidence exists.

## Review triggers

Revisit this ADR if Tauri cannot be built or distributed on the supported Linux baseline without unreasonable dependency burden, the editor UI cannot meet performance/accessibility requirements, or a concrete workload justifies a different native boundary.
