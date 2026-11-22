# SPEC-019 — Post-Recording AI Analysis Pipeline

**Status:** Approved for implementation  
**Milestone:** M3 — Evidence-based AI tutorial draft  
**Approval basis:** Product owner instruction to continue Screenstory implementation through the complete MVP, 2026-11-21  
**Related:** PRD-001, SPEC-004, SPEC-005, SPEC-006, SPEC-013, SPEC-014, SPEC-015, SPEC-017, SPEC-018, SPEC-007, ADR-003, ADR-004, ADR-005, ADR-006, ADR-007

## 1. Purpose

Turn a saved local recording into a reviewable, evidence-linked AI tutorial draft. The source video remains local and immutable. Analysis is explicitly started by the user, prepares a bounded set of representative frames locally, requests structured analysis from Gemini only after a separate cloud-transfer consent, validates the response, and saves a new draft without overwriting existing edits.

This milestone creates AI-generated content. It does not yet deliver the full timeline editor or final MP4 rendering.

## 2. Architecture decisions

### 2.1 Analysis lifecycle

- Analysis begins only after the original recording has been finalized and registered in the active project.
- One active analysis run per project is allowed in the first implementation. Completed/failed/cancelled runs remain in the project history.
- Each run gets an immutable run ID, source recording ID, provider/model identity, extraction configuration, creation time, status, and error code where applicable.
- Statuses: `queued`, `extracting_frames`, `awaiting_cloud_consent`, `sending_to_provider`, `validating_output`, `completed`, `failed`, `cancelled`.
- The application emits progress events and the UI must never block while a run is processing.
- Cancellation is supported during extraction and while a provider request is in flight where the HTTP client supports cancellation. A cancellation after a provider has already processed a request cannot recall data already transferred.
- A cancelled or failed run never changes the source recording, an earlier run, or an existing timeline.

### 2.2 Local metadata inspection and frame extraction

- Use the installed FFmpeg toolchain; `ffprobe` inspects the registered source recording.
- Do not accept an arbitrary path from the frontend. Resolve the recording ID against the active project's recording database and constrain the resolved path to that project's managed recordings directory.
- Supported video input for this milestone is the H.264 MP4 produced by SPEC-018. Unsupported/corrupt media receives an actionable error.
- Extract frames locally before cloud consent. Do not upload the source video.
- Baseline extraction is deterministic periodic sampling:
  - Maximum 60 frames per run.
  - Target 2 frames per second for recordings up to 30 seconds; for longer recordings, choose an interval that distributes no more than 60 samples across the full duration.
  - Include the first decodable frame and preserve the actual source timestamp in milliseconds for every frame.
  - Resize proportionally so the longest edge is at most 1024 pixels.
  - Encode JPEG at a quality target that keeps the complete inline request payload under 12 MiB; reduce quality/resolution deterministically when necessary.
  - Avoid duplicate timestamps and reject an empty extraction result.
- Scene-change sampling and OCR-assisted de-duplication are deferred. The manifest records that periodic sampling was used; it must not imply semantic scene detection.
- Store frames under `analysis/{run_id}/frames/` and write a versioned JSON manifest containing source recording identity, duration, source dimensions, sampling interval/configuration, frame IDs, timestamps, relative paths, dimensions, MIME type, and byte size.
- The manifest is written atomically only after extraction has been validated.
- Retain completed-run frames with the run. Cancelled/failed temporary files are cleaned up safely; preserve a failure manifest/log without retaining partial image payloads unnecessarily.

### 2.3 Provider and request contract

- The initial provider is Gemini. Domain, extraction, validation, and timeline code depend on a provider-independent contract; Gemini request/response types remain inside the adapter.
- Use the Gemini Interactions API for the initial integration. The default primary model is `gemini-4-hydrogen`; `gemini-3.8-flash` is the supported stable multimodal alternative. Each project stores its primary-model selection and whether automatic server-error fallback is enabled.
- If the selected primary model exhausts retries with HTTP 5xx responses, Screenstory may automatically try the alternate approved Gemini model. Do not switch models for HTTP 429 quota/rate limits, authentication/permission failures, invalid requests, or network failures. Record each model attempt and the final model used for the analysis.
- Use structured output with a JSON schema and validate the returned JSON again in the Rust domain layer.
- Use inline JPEG frame data; do not use the Gemini Files API for the first slice.
- Set `store=false` for every analysis request so the interaction is not retained as server-side conversation state. Do not use server-side previous-interaction chaining.
- Set explicit request timeouts and bounded retry rules. Retry only transient transport, rate-limit, and server errors; do not retry authentication, permission, invalid-request, safety, or schema failures automatically.
- Normalize provider failures into stable application error codes. Never show raw API keys or complete authorization headers in error messages/logs.
- Record provider, model, request ID when available, duration, and usage metadata when returned. Do not persist request bodies containing image bytes or credentials.
- Model availability is checked through configuration/connection diagnostics. Changing model IDs must not require a frontend rebuild.

### 2.4 Credential storage

- The Gemini API key is entered by the user in application settings and stored in the operating system credential store (Linux Secret Service/keyring).
- The project-specific Gemini primary model and automatic-fallback preference are non-secret settings stored with the project's SQLite metadata; they must never contain credentials.
- The key must never be stored in SQLite, project files, ordinary configuration files, logs, exports, source control, or frontend state beyond the transient entry form.
- If no supported OS credential store is available or unlocked, explain the blocker and do not silently fall back to plaintext storage.
- The UI may show only whether a credential is configured and a masked suffix; it must not retrieve the full key for display.
- Provide a delete/replace operation and a connection test.

### 2.5 Cloud-transfer consent and privacy

- Extract frames locally before asking the user to approve cloud transfer.
- Before each analysis request, show a consent review containing:
  - Provider, primary model, and the alternate model that may receive the same approved sample if automatic fallback is enabled.
  - Source project and recording filename/duration.
  - Number of frames and estimated payload size.
  - Optional workflow description and output language.
  - A clear statement that the sampled images and supplied text will be sent to Google Gemini; the original video file is not sent.
  - A warning that sampled frames may contain private information and automatic redaction is not guaranteed.
- The user must explicitly approve this specific transfer. Dismissing/declining the review sends no frames or workflow text.
- Allow the user to cancel the review and discard the run.
- Do not enable Google Search, URL context, tools, or other external grounding for this workflow.
- Keep a local record of the consent decision and the exact frame-manifest version approved for transfer.

### 2.6 Structured tutorial draft

The provider-independent result contains:

- Schema version.
- A concise recording summary.
- Ordered tutorial steps:
  - Stable step ID.
  - Title and explanatory description.
  - Start and end timestamps in milliseconds relative to the original video.
  - One or more evidence frame IDs.
  - Confidence in the range 0–1 where returned.
  - A distinction between directly observed evidence and uncertain inference.
- Zoom suggestions:
  - Stable suggestion ID.
  - Start/end timestamps.
  - Normalized region `x`, `y`, `width`, `height` in the 0–1 coordinate space.
  - Target scale and concise rationale.
  - Evidence frame IDs.
- The output language is chosen by the user: `auto`, `en`, or `ar`. Mixed Arabic/English content is allowed.

Domain validation rules:
- Reject unknown schema versions.
- Reject empty titles/descriptions, invalid IDs, duplicate IDs, and excessive collection sizes.
- Every timestamp must be finite, non-negative, and within the source duration; each step/suggestion must have start < end.
- Steps must be ordered and non-overlapping unless a later approved schema explicitly permits overlap.
- Every evidence frame ID must exist in the approved manifest.
- Confidence must be within 0–1.
- Zoom coordinates must be normalized, width/height positive, and the rectangle must remain inside the source frame.
- Target zoom scale must remain within the approved safe range of 1.0–3.0.
- Invalid output is rejected; it is never silently coerced into a valid timeline.

### 2.7 Persistence and re-analysis

- Persist analysis runs and their status in the project's SQLite database.
- Persist the approved frame manifest and validated AI draft as versioned JSON files beneath the run's managed analysis directory.
- The AI draft is marked `needs_review`; no AI step or zoom suggestion is implicitly approved for rendering.
- Re-analysis creates a separate run and draft. It must not overwrite the source video, an existing timeline, or user-authored edits.
- Existing draft/timeline selection is unchanged until the user explicitly chooses to review or apply the new draft.

## 3. UI requirements

- A saved recording exposes an `Analyze recording` action only when required local dependencies and provider configuration are available.
- Show preparation/extraction progress and current status.
- Show the cloud-transfer consent review after extraction and before any network request.
- Show provider progress, cancellation, and actionable errors.
- On success, show a review summary with generated step/zoom counts and indicate that the draft requires human review.
- Do not render non-functional buttons for deferred editor/export capabilities.

## 4. Security and failure handling

- All process execution uses fixed executable names and argument arrays; no shell interpolation.
- All file paths are derived from validated project/recording IDs and canonicalized under the managed project directory.
- File size, frame count, response size, duration, and output collection sizes are bounded.
- Original recording checksums/metadata are not changed by analysis.
- Provider calls are HTTPS only and use the key in an authorization header.
- Do not log sampled images, prompt payloads, secrets, or raw provider error bodies that may echo sensitive request data.
- Network failure, quota exhaustion, invalid credentials, unsupported model, malformed output, cancellation, and disk-full conditions are distinct states.
- A failure never destroys a prior successful analysis or project timeline.

## 5. Acceptance criteria

1. User can start analysis for a saved recording in the active project.
2. The run is persisted and progress is visible without blocking the UI.
3. ffprobe metadata is validated and persisted.
4. Deterministic extraction produces a bounded, timestamped manifest and playable/decodable JPEG frames.
5. No network request occurs before the per-run consent action.
6. Consent decline sends no frame or workflow-description data.
7. Approved requests send sampled frames and necessary text only, never the source MP4.
8. Gemini is invoked through the provider adapter with structured output, `store=false`, and the configured model.
9. Missing credentials, credential-store lock, HTTP errors, timeouts, rate limits, and malformed responses are surfaced as normalized errors.
10. Provider output passes all domain validations before a draft is saved.
11. Every generated step/zoom has valid source timestamps and evidence references.
12. AI drafts remain unapproved and cannot be rendered automatically.
13. A second analysis run does not overwrite the first run or an existing timeline.
14. Cancellation and process shutdown leave no falsely completed run.
15. Automated tests cover extraction configuration, manifest validation, provider request/response normalization, consent gating, output validation, cancellation, persistence, and re-analysis isolation.

## 6. Explicitly out of scope

- Scene-change detection, OCR, automatic sensitive-data redaction, and semantic de-duplication.
- Uploading the source video or using the Gemini Files API.
- Live AI analysis while recording.
- Local Ollama analysis.
- Timeline editor, manual step editing, zoom editing, annotations, preview, and final MP4 rendering/export.
- Audio transcription, speech recognition, and voice-over.
- OpenCode Free model integration: OpenCode's documented free Console inference models are restricted to OpenCode itself; external clients are rejected with HTTP 403. Screenstory will not bypass this restriction. A separately authorized provider integration may be considered later.
- Cross-platform provider support beyond the Linux-first architecture.

## 7. Upstream API reference snapshot

Implementation must re-verify the live API contract during integration. The initial design is based on Google's documentation accessed 2026-11-21:

- Interactions API: https://ai.google.dev/gemini-api/docs/interactions — documented as the recommended interface for new projects; stateless requests support `store=false`.
- Image understanding: https://ai.google.dev/gemini-api/docs/image-understanding — documents inline base64 image input and a 20 MB total inline request limit.
- Structured output: https://ai.google.dev/gemini-api/docs/structured-output — documents JSON-schema constrained structured responses.
- Models: https://ai.google.dev/gemini-api/docs/models — lists `gemini-4-hydrogen` as a supported multimodal model.
- API keys: https://ai.google.dev/gemini-api/docs/api-key — documents Gemini API key handling and the transition to authorization keys.

This snapshot is a design reference, not a promise that external model availability, limits, or billing policies remain unchanged.
