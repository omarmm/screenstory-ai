# ARCH-001 — System Architecture

**Version:** 1.0  
**Status:** Proposed architecture baseline  
**Style:** Modular desktop application

## 1. Architecture goals

- Minimum operational complexity.
- Recording independent from AI.
- Local-first project data.
- Non-destructive editing.
- Provider-independent AI contracts.
- Replaceable Linux capture backends.
- Reproducible rendering.

## 2. Proposed stack

| Layer | Technology |
|---|---|
| Desktop shell | Tauri v2 |
| UI | React + TypeScript |
| Native core | Rust |
| Wayland capture | XDG Desktop Portal + PipeWire |
| X11 capture | Dedicated X11 adapter |
| Video analysis/rendering | FFmpeg |
| Persistence | SQLite + filesystem |
| AI | Gemini initially |
| Local AI | Ollama later |

These choices require technical validation on the target Linux distribution and desktop environment.

## 3. Logical modules

### Desktop presentation
Project dashboard, recording controls, source selection, optional description, analysis status, timeline editor, preview, export, and settings.

### Recording
Enumerate sources, request permission, start/stop capture, receive stream metadata, save original media, and report errors.

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

## 4. Data flow

```text
User
  -> Desktop Application
  -> Recording Module
  -> Immutable Original Video
  -> Video Analysis
  -> Keyframes + timestamps
  -> AI Provider
  -> Validated Tutorial Analysis
  -> Timeline Editor
  -> Approved Timeline
  -> FFmpeg Renderer
  -> Final MP4
```

## 5. Linux capture

The application exposes a unified capture interface independent of display server.

### Wayland
Use XDG Desktop Portal ScreenCast and PipeWire. Source selection and permission behavior are controlled by portal capabilities and desktop implementation.

### X11
Use a separate adapter for desktop/window capture where supported.

Global keyboard and mouse monitoring is not required for MVP. The product must not assume arbitrary global input hooks are available.

## 6. AI analysis

The original video remains local and immutable. The analysis module extracts representative frames using periodic sampling and visual-change detection. The initial provider receives selected frames, timestamps, optional user description, language preference, and structured output instructions.

The complete original video is not uploaded by default.

All AI responses must be validated before being accepted into the domain model.

## 7. Timeline

The timeline is the source of truth for final composition. It references the original recording and contains ordered steps, annotations, zoom segments, AI suggestion status, manual overrides, and schema version.

Priority:
1. User manual edits.
2. User-approved AI suggestions.
3. Unapproved AI suggestions.

Unapproved suggestions must not silently override manual edits.

## 8. Persistence

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

## 9. Error boundaries

- Capture errors must not corrupt existing projects.
- AI errors must not delete recordings.
- Invalid timelines must block rendering.
- Rendering errors must preserve timeline and source assets.
- Network failures must not terminate the desktop application.

## 10. Open technical risks

- Window capture behavior varies across Linux desktop environments.
- PipeWire and portal availability must be verified.
- Scene detection and sampling thresholds need calibration.
- Arabic text rendering needs proper shaping and font support.
- FFmpeg build features and redistribution licensing need review.
- Gemini model capabilities, pricing, and quotas must be checked at implementation time.
