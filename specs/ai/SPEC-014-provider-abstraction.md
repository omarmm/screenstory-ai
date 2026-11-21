# SPEC-014 — AI Provider Abstraction

**Status:** Draft

## Scope
Define the interface between application analysis orchestration and AI providers.

## Requirements
- Domain logic does not depend on Gemini SDK types.
- Provider input/output is normalized.
- Provider capabilities are explicit.
- Initial implementation is Gemini only.
- Ollama is a future adapter, not an MVP dependency.

## Acceptance criteria
- Analysis orchestration depends on the provider contract only.
- Provider-specific response objects do not enter timeline/domain state.
