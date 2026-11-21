# ADR-002 — Production Desktop Technology Stack

**Status:** Accepted  
**Date:** 2026-11-21  
**Decision owners:** Product owner / project maintainer  
**Related:** ARCH-001, PRD-001, SPEC-002

## Context

Screenstory is a Linux-first desktop application for recording the desktop or a selected window, analyzing the resulting video, and producing an editable tutorial with a rich timeline, preview, annotations, zoom controls, and MP4 export.

The working Python/Tkinter prototype has validated important recording workflows on Linux Mint + Cinnamon + X11. It is useful evidence and a behavioral reference, but Tkinter is not the chosen foundation for the production editor UI. The production application needs a maintainable separation between presentation, domain logic, operating-system integration, and media processing.

The technology decision must support:
- A rich, responsive editing interface.
- Native Linux integration and explicit permission boundaries.
- A maintainable native core for project lifecycle and capture orchestration.
- Replaceable X11 and Wayland capture adapters.
- FFmpeg-based video processing without coupling it to the UI.
- Local-first storage and non-destructive editing.
- A practical path to package and distribute the Linux application.

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

Python is not part of the required production desktop runtime or core. It may be introduced later as an isolated worker for a specific AI/computer-vision capability only when a measurable technical benefit justifies its runtime, packaging, and maintenance cost. Such a worker must use a versioned contract and must not own canonical project state.

The current Python/Tkinter prototype will be retained as a behavioral reference until the production app passes equivalent recording tests. It will not be directly ported file-for-file.

## Rationale

- React + TypeScript is a suitable foundation for a complex editor-style interface and a large interactive component ecosystem.
- Tauri provides a desktop shell and a controlled native bridge without requiring the application to bundle a complete Chromium browser.
- Rust provides a compiled native core with strong type and memory-safety guarantees for long-lived application and platform integration code.
- Separating UI, domain services, platform capture, and FFmpeg orchestration keeps the capture backend replaceable and the application testable.
- The selected stack supports a clear security model: frontend capabilities can be narrowly granted, while privileged operations remain in native code.

## Alternatives considered

### Python + Tkinter
Retain as the validated prototype, not the production UI. It is useful for proving capture behavior quickly, but is not selected for the timeline/editor experience.

### Python + PySide6 / Qt
A credible alternative with a mature native desktop UI toolkit. It would keep more of the application in Python, but the project chooses the Tauri/Rust boundary for the production architecture and frontend ecosystem.

### Electron + React + TypeScript
A viable web-technology desktop option, but it bundles Chromium and generally carries a larger application footprint. It is not selected for this Linux-first product.

### Python backend plus Tauri frontend
Not selected as the default architecture because it would introduce a second long-lived runtime and IPC boundary before a concrete need has been demonstrated. Reconsider only for a specific AI/CV worker with a clear benefit.

## Consequences

### Positive
- Clear separation between UI, native core, media processing, and platform capture.
- Rich frontend ecosystem for the timeline editor.
- Native Rust boundary for sensitive OS operations.
- FFmpeg remains the proven media-processing workhorse.
- Python remains available as an optional specialist tool without being a mandatory runtime dependency.

### Costs and risks
- The team must learn and maintain Rust alongside TypeScript.
- Tauri v2 Linux development requires the supported WebKitGTK 4.1 system dependencies and native build toolchain.
- The actual Linux Mint target must pass a prerequisite/build compatibility check before the local development environment is considered ready.
- Native screen capture is not supplied automatically by Tauri; X11 and Wayland adapters remain separate engineering work.
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

Revisit this ADR if:
- Tauri cannot be built or distributed on the supported Linux baseline without unreasonable dependency burden.
- The rich editor UI cannot meet performance/accessibility requirements with the selected frontend.
- A concrete Python AI/CV workload demonstrates enough value to justify a separate worker.
- Linux capture integration requires a different native boundary or framework.
