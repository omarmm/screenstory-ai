# SPEC-022 — Video Preview and MP4 Tutorial Export

**Status:** Draft — implementation gated by M5 approval and renderer proof  
**Milestone:** M6

## 1. Scope

Compose an immutable source MP4 with the current validated timeline into a preview and a final MP4 tutorial. Preview and export must use one deterministic composition plan. The original recording is never overwritten.

## 2. Composition contract

- Input source recording is resolved from a managed project recording ID, never from an arbitrary frontend path.
- Input timeline must be the current saved revision and pass full schema validation.
- Only accepted AI zoom segments and manual zoom segments are active.
- Pending/rejected AI suggestions are excluded.
- Annotations are rendered through the same layout/shaping implementation used by preview.
- Output retains the original aspect ratio and source frame rate by default.
- Audio from the source is preserved when present; no transcription or generated voice-over.
- Unsupported codec or invalid media receives a typed error with the source preserved.

## 3. Render pipeline

- Build an immutable render plan with explicit timeline revision, source identity, output resolution, frame rate, active zooms, annotations, fonts and renderer version.
- Use FFmpeg filter graphs with arguments passed as fixed argument vectors; no shell interpolation.
- Apply crop/scale zoom transforms and text overlays with a verified shaping-capable renderer.
- Use safe intermediate files under the project export workspace.
- Write to a temporary output file, fsync, run ffprobe validation, and atomically rename on success.
- On failure/cancel, remove partial output but keep the source, timeline and prior exports intact.
- Store export history with source recording ID, timeline revision, renderer version, output properties, file size, and completion status.

## 4. Preview

- Preview must reflect the exact saved revision selected for rendering.
- Do not silently preview unsaved edits as if they were persisted.
- User can seek and play the composed output.
- Preview generation is cancellable and reuses the same render plan and filter composition as export.
- If an exact real-time composed preview cannot meet performance targets, use a low-resolution proxy from the same plan and clearly label it.

## 5. Progress and cancellation

- Render runs persist status and expose progress.
- Cancellation terminates the FFmpeg child process safely, waits for termination, and deletes temporary output.
- Application exit marks active render as interrupted and preserves the last successful export.
- Progress uses FFmpeg's machine-readable progress output, not estimated timers.

## 6. Resource constraints

- Bound output dimensions, duration, process count and temporary disk use.
- Never hold entire video data in memory.
- Check available disk space before render and show expected output/temporary storage estimate.
- Avoid CPU-intensive work on the UI thread.

## 7. Acceptance criteria

1. Saved valid timeline yields a deterministic render plan.
2. Pending/rejected AI zooms do not appear in preview/export.
3. Accepted and manual zooms appear at correct timestamps and regions.
4. Arabic/English annotations render with correct shaping and direction.
5. Source audio is preserved when present.
6. Export progress and cancellation work and survive app restarts as truthful states.
7. Failed/cancelled export leaves source, timeline and previous exports intact.
8. ffprobe verifies the final MP4 is decodable and matches expected properties.
9. Render history identifies the exact timeline revision used.
10. Automated tests cover render-plan validation, FFmpeg argument generation, atomic output finalization, cancellation, and export-history persistence.
