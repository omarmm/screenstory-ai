# ADR-010 — No Mandatory Python Backend

**Status:** Approved

## Decision
Keep the MVP in Tauri/Rust and React. Do not introduce a mandatory Python backend or worker.

## Rationale
Fewer processes and simpler packaging and lifecycle management.

## Consequences
Reconsider only if a concrete requirement cannot be met cleanly in the selected stack.
