# SPEC-003 — Recording Lifecycle

**Status:** Draft

## Scope
Start and stop capture, track elapsed time, persist the original recording, and recover safely where possible.

## Requirements
- Recording is independent of AI availability.
- The original media is never overwritten by editing or export.
- Capture errors are surfaced to the user.
- Pause/resume is optional and backend-dependent.
- Finalized recordings are registered in the project.

## Acceptance criteria
- Start/stop creates a playable source recording.
- AI/network failure does not interrupt capture.
- Failed capture does not corrupt existing projects.
