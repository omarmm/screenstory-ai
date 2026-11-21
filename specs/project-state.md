# Project State

**Project:** Screenstory  
**Baseline:** MVP v1.0  
**Status:** Specification baseline prepared; capture mechanism reassessment in progress.

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

## Capture architecture direction

- X11 full-display capture: FFmpeg `x11grab` passed the initial Linux Mint + Cinnamon + X11 validation.
- X11 window capture: FFmpeg `x11grab -window_id` is unsuitable for the required resize/minimize lifecycle. XComposite is the selected follow-up mechanism, based on its use in OBS Studio's Linux X11 window-capture implementation.
- Wayland: XDG Desktop Portal ScreenCast + PipeWire remains the selected architecture direction; validation is pending.
- No production capture implementation has started. XComposite dependency/license selection and spike evidence are still required.

## Next actions

1. Revise and review SPEC-002 with the XComposite-based X11 window-capture direction.
2. Complete the XComposite proof of capability on Linux Mint + Cinnamon + X11.
3. Validate Wayland capture through XDG Desktop Portal and PipeWire on a target compositor.
4. Finalize frame extraction and scene-detection strategy.
5. Approve the versioned timeline JSON schema.
6. Define the Gemini request/response contract and privacy controls.
7. Authorize M1 implementation tasks only after the relevant specifications are approved.

## Active implementation task

None. The active work is specification and technical validation.

## Blockers / open questions

- XComposite runtime capabilities and license-compatible integration approach must be selected and validated.
- Minimized/off-workspace X11 window behavior must be defined from evidence.
- Target Wayland portal implementation and its window-source capabilities remain unvalidated.
- FFmpeg availability and required filter/font capabilities must be validated.
- Gemini model selection, API limits, and current pricing must be verified at implementation time.
- Packaging strategy (AppImage, deb, or both) remains open.

## Decision policy

Do not start implementation merely because the repository exists. Start only after the relevant detailed specification is approved and a task is marked Ready.
