# SPEC-007 — Timeline Schema

**Status:** Draft — foundational contract

## Scope
Define the versioned, non-destructive edit representation used by the editor and renderer.

## Required concepts
- Timeline schema version.
- Source recording reference.
- Ordered steps.
- Annotations.
- Zoom segments.
- AI suggestion status.
- Manual override metadata.
- Creation/update metadata.

## Rules
- Original recording is immutable.
- Timestamps use milliseconds relative to the source recording.
- Coordinates use normalized values relative to source dimensions.
- Manual edits take precedence over AI suggestions.
- Unapproved AI zoom suggestions are not rendered by default.
- Timeline validation must complete before rendering.

## Acceptance criteria
- Timeline can be serialized and loaded.
- Invalid time ranges and coordinates are rejected.
- Schema version is explicit.
- Existing timeline can be reopened after application restart.
