# SPEC-012 — MP4 Export

**Status:** Draft

## Scope
Render the approved timeline into an MP4 tutorial.

## Requirements
- Validate source and timeline before rendering.
- Apply zoom and annotation overlays.
- Preserve source aspect ratio by default.
- Preserve source audio when available.
- Show progress and report errors.
- Never overwrite the original recording.
- Verify output file is playable.

## Acceptance criteria
- Exported MP4 plays successfully.
- Zoom and text are present at expected timestamps.
- Original recording remains unchanged.
- Failed export leaves project and timeline intact.
