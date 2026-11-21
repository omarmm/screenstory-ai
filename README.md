# Screenstory

**Turn a screen recording into an editable, AI-assisted tutorial.**

Screenstory is a Linux-first desktop application concept for recording a desktop or a specific application window, analyzing the recording after capture, generating tutorial steps and zoom suggestions, and exporting a polished MP4.

> **Project status:** Specification baseline — implementation has not started.

## Product goals

- Record the full desktop or a selected window.
- Keep the original recording unchanged.
- Optionally provide a description of the workflow.
- Analyze the recording after capture using Gemini initially.
- Generate ordered steps, titles, descriptions, and zoom suggestions.
- Review and edit the generated timeline.
- Add manual zooms and text annotations.
- Export an MP4 tutorial.

## MVP decisions

| Area | Decision |
|---|---|
| Platform | Linux first |
| Recording source | Full desktop and specific window |
| Workflow description | Optional |
| AI timing | Post-recording only |
| Initial AI provider | Gemini API |
| Local AI | Ollama planned after initial MVP |
| Zoom | Hybrid: AI suggestions + manual editing |
| Editing | Non-destructive timeline |
| Export | MP4 |
| Storage | Local-first |
| Specification/repository understanding | Deferred |
| Autonomous computer-use agent | Deferred |
| Voice-over | Deferred |

## Proposed technology

- **Desktop:** Tauri v2
- **UI:** React + TypeScript
- **Native core:** Rust
- **Linux capture:** XDG Desktop Portal + PipeWire (Wayland), separate X11 adapter
- **Video analysis and rendering:** FFmpeg
- **Persistence:** SQLite + project files
- **AI:** Gemini first; provider abstraction for future Ollama support

These are architectural decisions to validate during specification and technical spikes. They are not a claim that every Linux desktop environment has identical capture capabilities.

## Documentation

- [Product Requirements — PRD-001](specs/product/PRD-001-product-requirements.md)
- [System Architecture — ARCH-001](specs/architecture/ARCH-001-system-architecture.md)
- [SDD workflow](specs/SDD-WORKFLOW.md)
- [Project state](specs/project-state.md)
- [Architecture Decision Records](specs/adr/)

## Development approach

Screenstory follows Specification-Driven Development:

1. Define and approve the requirement.
2. Define component behavior and contracts.
3. Record architecture decisions.
4. Break approved specifications into implementation tasks.
5. Implement only the active task.
6. Verify behavior with tests and evidence.
7. Update project state before selecting the next task.

**No implementation task is authorized by this documentation baseline alone.** Detailed component specifications and the Linux capture technical spike must be completed before implementation begins.

## Initial milestones

1. **M1 — Desktop recording foundation**
2. **M2 — Video analysis and frame extraction**
3. **M3 — Gemini integration**
4. **M4 — Editable tutorial timeline**
5. **M5 — Rendering and MP4 export**

See [project-state.md](specs/project-state.md) for the current handoff and next actions.

## Privacy principles

- Original recordings remain local by default.
- The full original video is not uploaded by default.
- Only selected analysis frames and required context may be sent to the configured cloud provider, after user consent.
- API credentials must not be committed or stored in project exports.
- AI-generated steps and zooms are suggestions, not verified facts.

## License

No project license has been selected yet. Do not assume this repository is open source until a license is explicitly chosen and added.
