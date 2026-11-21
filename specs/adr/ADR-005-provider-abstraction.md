# ADR-005 — AI Provider Abstraction

**Status:** Approved

## Decision
Define provider-independent AI contracts. Implement Gemini first; defer Ollama implementation.

## Rationale
Avoid vendor lock-in and keep domain/timeline logic provider-independent.

## Consequences
A small abstraction layer is required. Provider-specific response formats must be normalized.
