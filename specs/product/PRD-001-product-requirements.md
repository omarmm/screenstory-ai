# PRD-001 — Product Requirements

**Version:** 1.0  
**Status:** Approved MVP scope  
**Platform:** Linux first

## 1. Vision

Screenstory transforms an ordinary desktop recording into an editable, AI-assisted instructional video.

A user records the desktop or a selected application window. After recording, AI analyzes representative frames and generates tutorial steps, explanations, and zoom suggestions. The user reviews and edits the result before exporting an MP4.

## 2. Problem

Producing a polished tutorial currently requires manual recording review, scene selection, step writing, zoom placement, annotation, and rendering. Screenstory aims to reduce this work while keeping the user in control.

## 3. Target users

- Software developers documenting workflows.
- QA engineers documenting test execution.
- Technical trainers.
- Product and support teams.
- Technical consultants.

## 4. Approved MVP decisions

| Decision | Value |
|---|---|
| OS | Linux first |
| Capture source | Full desktop + specific window |
| Description | Optional |
| AI timing | Post-recording |
| Initial provider | Gemini API |
| Future local provider | Ollama |
| Zoom | Hybrid |
| Editing | Non-destructive timeline |
| Output | MP4 |
| Storage | Local-first |

## 5. Functional requirements

### FR-001 Project management
Users can create, open, save, and reopen independent projects.

### FR-002 Capture source selection
Users can select an available display or a specific application window. Unsupported sources must produce a clear error.

### FR-003 Recording lifecycle
Users can start and stop recording, see elapsed time, and save the original recording. Pause/resume is optional and depends on capture backend support. Recording must not depend on AI or network availability.

### FR-004 Optional description
Users may provide an Arabic or English description of the intended workflow. The description is context only and must not be treated as proof that an action occurred.

### FR-005 Video analysis
After recording, users can request analysis. The pipeline inspects video metadata, detects meaningful visual changes, extracts representative frames with timestamps, sends selected frames and context to the configured AI provider, and validates the structured response.

### FR-006 Generated steps
Each step includes an ID, start/end timestamps, title, description, evidence references, and confidence when available. Observed facts must be distinguished from uncertain inferences.

### FR-007 Timeline editing
Users can edit step titles, descriptions, boundaries, order, and annotations; add or delete steps; and preview changes.

### FR-008 Hybrid zoom
AI creates zoom suggestions. Users can accept, reject, or modify them. Users can create manual zoom segments. Manual edits take precedence over AI suggestions.

### FR-009 Text annotations
Support titles and explanatory captions, positioning, duration, basic typography, Arabic/English text, and RTL/LTR layout.

### FR-010 Preview
Preview must reflect the approved timeline, including zooms and annotations.

### FR-011 Export
Export MP4 while preserving source aspect ratio by default and original audio when available. Export must not overwrite the original recording.

### FR-012 AI provider configuration
Gemini is the initial provider. Provider contracts must be independent of the domain and timeline. Ollama is architecturally planned but deferred.

### FR-013 Language
Support Arabic, English, and mixed-language content. PDF export is out of scope.

### FR-014 Markdown export
Markdown tutorial export is optional and secondary to MP4.

## 6. Non-functional requirements

- Recording remains operational if AI or network services fail.
- Analysis and rendering run asynchronously from UI interaction.
- Original recordings remain local by default.
- Cloud analysis requires explicit configuration and consent.
- Only selected frames and necessary context are transferred by default.
- API credentials are stored using OS secret storage.
- Projects and timeline edits survive application restart.
- AI responses are schema-validated.
- Rendering is reproducible for a given source, timeline version, and render configuration.

## 7. Out of scope

Specification/repository reading, autonomous agents, live AI monitoring, voice-over, speech transcription, collaboration, cloud storage, web editor, PDF export, Windows/macOS, and guaranteed automatic detection of all sensitive values.

## 8. MVP success criteria

A user can record desktop or window, optionally describe the workflow, analyze with Gemini, review and edit steps, accept/reject AI zooms, add manual zoom and annotations, and export a playable MP4.

## 9. Product constraints

AI analysis uses sampled visual evidence and may miss fast or subtle actions. The application must not claim every click or typed value was detected. The user remains responsible for reviewing the tutorial before export.
