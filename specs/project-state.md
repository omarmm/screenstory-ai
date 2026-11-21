# Project State

**Project:** Screenstory  
**Baseline:** MVP v1.0  
**Status:** Specification baseline prepared; implementation not started.

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

## Current milestone

**Phase 0 — Specification finalization and technical validation.**

## Next actions

1. Review and approve the detailed Linux capture specification.
2. Validate Wayland and X11 capture behavior on the target Linux environment.
3. Finalize frame extraction and scene-detection strategy.
4. Approve the versioned timeline JSON schema.
5. Define the Gemini request/response contract and privacy controls.
6. Only then authorize M1 implementation tasks.

## Active implementation task

None.

## Blockers / open questions

- Target Linux distribution and desktop environment must be recorded.
- Window-capture behavior must be validated on the target Wayland portal implementation.
- FFmpeg availability and required filter/font capabilities must be validated.
- Gemini model selection, API limits, and current pricing must be verified at implementation time.
- Packaging strategy (AppImage, deb, or both) remains open.

## Decision policy

Do not start implementation merely because the repository exists. Start only after the relevant detailed specification is approved and a task is marked Ready.
