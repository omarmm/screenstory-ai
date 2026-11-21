# ARCH-001 — System Architecture

**Version:** 1.1  
**Status:** Approved baseline — desktop stack decision recorded in ADR-001  
**Style:** Modular, local-first desktop application

## 1. Architecture goals

- Minimum operational complexity.
- Recording independent from AI.
- Local-first project data.
- Non-destructive editing.
- Provider-independent AI contracts.
- Replaceable Linux capture backends.
- Reproducible rendering.
- Clear security boundaries between the web UI and native capabilities.

## 2. Approved technology stack

| Layer | Technology | Responsibility |
|---|---|---|
| Desktop shell | Tauri v2 | Application window, lifecycle, packaging, capability permissions, native bridge |
| UI | React + TypeScript | Project dashboard, recording UI, timeline editor, preview, settings |
| Native application core | Rust | Domain orchestration, project lifecycle, capture coordination, validation, process management |
| Linux capture | Platform-specific Rust adapters | X11 and Wayland source discovery/permission/stream acquisition |
| Video analysis/rendering | FFmpeg | Metadata, frame extraction, video transforms, encoding and export |
| Persistence | SQLite + filesystem | Structured project state and large media/assets |
| Initial AI provider | Gemini | Post-recording tutorial analysis |
| Future local AI | Ollama | Deferred local inference provider |

Python is **not** part of the required desktop runtime or core architecture. It may be introduced later as an isolated worker for a concrete AI/computer-vision capability when a measured benefit justifies the additional runtime, packaging, and operational complexity. It must communicate through a versioned contract and must not become the source of truth for project state.

The working Tkinter/Python recorder is a prototype and behavioral reference, not the production application architecture. It remains available until the new implementation passes the agreed capture acceptance tests.

## 3. Technology decision

ADR-001 records the selection of Tauri v2 + React + TypeScript + Rust. The decision is based on a rich editor UI, native Linux integration, a compiled native core, and a narrow permission-controlled bridge. It does not imply that Tauri itself provides screen capture or video processing.

Tauri v2 uses the platform webview. On Linux, development/build prerequisites include the Tauri-supported WebKitGTK 4.1 stack and native build dependencies. The exact compatibility of the target Linux Mint installation must be checked before committing to a local build workflow. This is a validation gate, not an assumption.

## 4. Logical modules

### Desktop presentation
Project dashboard, recording controls, source selection, optional description, analysis status, timeline editor, preview, export, and settings.

### Native application core
Application commands, domain validation, project lifecycle, orchestration, process supervision, filesystem access, and controlled communication with platform adapters. The UI must not receive arbitrary shell or filesystem access.

### Recording
Enumerate sources, request permission, start/stop capture, receive stream metadata, save original media, and report errors. Platform-specific source acquisition stays behind a unified capture interface.

### Video analysis
Inspect metadata, detect visual changes, extract representative frames, associate timestamps, and produce an analysis manifest.

### AI integration
Provider configuration, request construction, schema validation, retries, rate-limit handling, and provider response normalization.

### Timeline domain
Steps, annotations, zoom segments, manual overrides, timeline validation, and persistence.

### Rendering
Validate timeline, apply zoom/crop, render text, preserve/configure audio, encode MP4, and verify output.

### Persistence
Project metadata, recording metadata, analysis runs, timeline versions, and export history.

## 5. Data flow

```text
User
  -> React UI
  -> Tauri command boundary
  -> Rust application/domain services
  -> Recording adapter
  -> Immutable Original Video
  -> Video Analysis (FFmpeg)
  -> Keyframes + timestamps
  -> AI Provider (after explicit consent)
  -> Validated Tutorial Analysis
  -> Timeline Editor
  -> Approved Timeline
  -> FFmpeg Renderer
  -> Final MP4
```

## 6. Native bridge and security

- Expose narrowly scoped Tauri commands for explicit application operations.
- Apply Tauri v2 capabilities/permissions per window and plugin; deny access by default unless required.
- Do not expose a general-purpose shell command, arbitrary filesystem API, or unrestricted process execution to the frontend.
- Validate all command inputs in Rust; treat frontend state and AI output as untrusted.
- Keep API credentials out of project exports, logs, and prompts.
- Cloud transfer of selected frames/context requires explicit user consent and must follow the configured privacy policy.
- Keep the original recording local and immutable by default.

## 7. Linux capture

The application exposes a unified capture interface independent of display server.

### X11
Use a dedicated X11 adapter. Full-display capture with FFmpeg x11grab has passed the current prototype test on Linux Mint + Cinnamon + X11. Window capture using FFmpeg x11grab -window_id has also produced a playable recording in the prototype, but it has a fixed-geometry limitation: resizing during capture is not supported by the current implementation. An XComposite experiment produced black frames in the same target environment and is not accepted as a working backend. Production window-capture mechanism remains subject to a revised SPEC-002 and proof of capability.

### Wayland
Use XDG Desktop Portal ScreenCast and PipeWire. Source selection and permission behavior are controlled by portal capabilities and desktop implementation. Validation remains pending.

Global keyboard and mouse monitoring is not required for MVP. The product must not assume arbitrary global input hooks are available.

## 8. Timeline

The timeline is the source of truth for final composition. It references the original recording and contains ordered steps, annotations, zoom segments, AI suggestion status, manual overrides, and schema version.

Priority:
1. User manual edits.
2. User-approved AI suggestions.
3. Unapproved AI suggestions.

Unapproved suggestions must not silently override manual edits.

## 9. Persistence

Each project has a local workspace. Large media files are stored on disk, not inside SQLite.

```text
project/
  project.json
  database.sqlite
  recordings/
    original.mkv
  analysis/
    frames/
    analysis.json
    extraction-manifest.json
  timeline/
    timeline.json
  assets/
  exports/
    tutorial.mp4
```

Exact file formats and transaction/recovery semantics are specified in component specifications.

## 10. Error boundaries

- Capture errors must not corrupt existing projects.
- AI errors must not delete recordings.
- Invalid timelines must block rendering.
- Rendering errors must preserve timeline and source assets.
- Network failures must not terminate the desktop application.
- A failed or unavailable native adapter must report a typed, user-visible error; it must not silently switch capture sources.

## 11. Open technical risks

- Tauri's Linux WebKitGTK 4.1 build prerequisites and target Linux Mint compatibility must be verified.
- Window capture behavior varies across Linux desktop environments.
- The current x11grab window prototype does not support resizing during capture.
- The XComposite experiment currently produces black frames in the target environment.
- PipeWire and portal availability and behavior must be verified.
- Scene detection and sampling thresholds need calibration.
- Arabic text rendering needs proper shaping and font support.
- FFmpeg build features and redistribution licensing need review.
- Gemini model capabilities, pricing, and quotas must be checked at implementation time.
- The low-memory target machine's build/runtime performance must be measured before finalizing development assumptions.
