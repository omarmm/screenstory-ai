# SPEC-006 — AI Analysis Contract

**Status:** Draft

## Scope
Define provider-independent inputs and outputs for generating tutorial steps and zoom suggestions.

## Input
- Selected frame references and timestamps.
- Optional user workflow description.
- Desired explanation language.
- Structured output instructions.

## Output
- Schema version.
- Summary.
- Ordered tutorial steps.
- Start/end timestamps.
- Title and description.
- Evidence frame references.
- Confidence where available.
- Zoom suggestions with normalized coordinates and rationale.

## Requirements
- AI output is untrusted until schema-validated.
- Timestamps must be within source duration.
- Coordinates must be normalized and within valid bounds.
- User description is context, not proof.
- Distinguish observed evidence from uncertain inference.
- Malformed output is rejected or retried under a bounded policy.

## Acceptance criteria
- Valid response maps to domain objects.
- Invalid timestamps/coordinates are rejected.
- Unknown fields are handled according to schema version policy.
