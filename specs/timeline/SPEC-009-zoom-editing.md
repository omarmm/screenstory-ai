# SPEC-009 — Hybrid Zoom Editing

**Status:** Draft

## Scope
Review AI zoom suggestions and create or edit manual zoom segments.

## Requirements
- Accept/reject AI suggestions.
- Edit region, start/end time, and scale.
- Create manual zoom segments.
- Manual edits take precedence.
- Smooth transitions are configured by the renderer.
- Invalid regions and times are rejected.

## Acceptance criteria
- Rejected suggestions are excluded from rendering.
- Accepted suggestions are rendered.
- Manual zooms are rendered.
- User edits persist and override AI suggestions.
