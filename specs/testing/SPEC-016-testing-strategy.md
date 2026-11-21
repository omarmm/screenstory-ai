# SPEC-016 — Testing Strategy

**Status:** Draft

## Unit tests
- Project domain rules.
- Timestamp and coordinate validation.
- Timeline serialization.
- AI response parsing.
- Provider error normalization.
- Render configuration generation.

## Integration tests
- SQLite persistence.
- FFmpeg metadata and frame extraction.
- Gemini adapter with mocked responses.
- Timeline-to-renderer integration.

## Linux capture tests
- Wayland full-desktop capture.
- Wayland window capture where supported.
- X11 capture.
- Permission denial.
- Stream interruption.

## End-to-end scenario
1. Record a browser form workflow.
2. Stop and save.
3. Add an optional description.
4. Analyze.
5. Review generated steps.
6. Accept one zoom and reject another.
7. Add a manual zoom.
8. Edit a caption.
9. Export MP4.
10. Verify playback and expected overlays.
