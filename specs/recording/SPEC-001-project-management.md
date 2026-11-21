# SPEC-001 — Project Management

**Status:** Draft

## Scope
Create, open, save, and reopen local Screenstory projects.

## Requirements
- Every project has a stable identifier and local workspace.
- Project metadata includes name and creation/update timestamps.
- Projects reference recordings, analysis runs, timelines, and exports.
- Missing assets must be reported without silently deleting project metadata.
- Project paths must be validated before file operations.

## Acceptance criteria
- A project can be created and reopened after application restart.
- Project metadata persists.
- Missing media produces a recoverable error.
