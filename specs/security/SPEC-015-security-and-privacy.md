# SPEC-015 — Security and Privacy

**Status:** Draft

## Requirements
- Original recordings remain local by default.
- Cloud upload requires explicit consent and clear disclosure.
- Only selected frames and necessary context are uploaded by default.
- API keys use OS secret storage.
- Secrets must not be committed, logged, or included in project exports.
- Sensitive values should be masked before cloud upload when reliably detectable.
- The product must not promise perfect automatic sensitive-data detection.
- Temporary analysis frames follow a documented retention policy.

## Acceptance criteria
- No cloud request occurs before consent.
- API credentials are absent from logs and project files.
- User can inspect which project data is sent for analysis.
