# SPEC-005 — Frame Extraction and Scene Detection

**Status:** Draft — calibration required

## Scope
Extract representative frames from the original recording and associate each frame with a source timestamp.

## Candidate strategy
- Periodic sampling as a baseline.
- Visual-change/scene detection to add informative frames.
- Deduplicate near-identical frames.
- Enforce configurable frame-count and image-size limits.
- Preserve exact source timestamps and extraction metadata.

## Requirements
- Frame extraction is reproducible for the same source and configuration.
- Frames are temporary analysis artifacts unless retained by project policy.
- The original video is never modified.
- Sampling configuration is recorded in an extraction manifest.

## Open questions
- Sampling interval and scene-change threshold.
- Maximum frames per analysis request/batch.
- Resize/compression policy.
- Handling of long recordings.

## Acceptance criteria
- Every extracted frame has a stable ID and source timestamp.
- Duplicate frames are reduced.
- Frame count and dimensions respect configured limits.
