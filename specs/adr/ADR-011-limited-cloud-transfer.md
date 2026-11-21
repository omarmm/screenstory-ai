# ADR-011 — Limited Cloud Data Transfer

**Status:** Approved

## Decision
Send selected analysis frames and necessary context to Gemini only after explicit user consent. Do not upload the complete original video by default.

## Rationale
Privacy, bandwidth, and cost control.

## Consequences
Sampling quality matters. Fast interactions may be missed. Sensitive-data handling must be specified and tested.
