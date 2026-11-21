# SPEC-004 — Video Analysis Orchestration

**Status:** Draft

## Scope
Coordinate metadata inspection, frame extraction, AI analysis, validation, and analysis status after recording.

## Requirements
- Analysis is initiated explicitly by the user.
- Analysis runs asynchronously from UI interaction.
- Analysis can be cancelled safely.
- Each run has a unique ID and status.
- Failures preserve the source recording and previous timeline.
- Re-analysis creates a new run and must not silently overwrite manual edits.

## Acceptance criteria
- User can start and cancel analysis.
- Progress and failures are visible.
- Source recording remains unchanged.
