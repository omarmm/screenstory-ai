# SPEC-013 — Gemini Provider

**Status:** Draft

## Scope
Implement the initial cloud AI adapter.

## Requirements
- Keep provider integration behind application contracts.
- Support selected image frames and text context.
- Request structured tutorial analysis.
- Handle authentication, quota, timeout, network, and malformed-response errors.
- Store credentials outside project files.
- Require explicit user consent before cloud transfer.
- Model selection and current API limits must be verified at implementation time.

## Acceptance criteria
- Successful request produces validated provider-independent output.
- API failures are normalized.
- Credentials are not written to logs or project exports.
