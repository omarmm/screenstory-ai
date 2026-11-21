# SPEC-022 — Video Preview and MP4 Tutorial Export

**Status:** Approved for implementation
**Approval basis:** Product owner instruction to continue Screenstory implementation through MVP completion (2026-11-21)  
**Milestone:** M6

## Approved implementation decisions

- The render plan is derived only from the current persisted timeline revision and a managed recording ID. No frontend-supplied path is accepted.
- The plan records source identity, timeline revision, source dimensions/frame rate/audio presence, renderer version, active zoom segments and annotations. Its serialized representation is immutable for the duration of a run.
- FFmpeg is invoked directly with an argument vector. Filter expressions contain only validated numeric values and application-managed text-file paths; annotation text is never interpolated into an FFmpeg command string.
- Zoom motion uses FFmpeg's frame-evaluated zoom filter with a deterministic piecewise expression. Only accepted AI suggestions and manual zoom segments are included; pending and rejected AI suggestions are excluded.
- Annotation rendering uses FFmpeg drawtext with text loaded from UTF-8 files and text shaping enabled. The renderer must verify that the installed FFmpeg build exposes the shaping option and that fontconfig can resolve a font for Arabic before enabling annotation export. It must fail closed with a typed, actionable error when either capability is absent; it must not silently render broken Arabic.
- Source audio is mapped when present and encoded to AAC. The source video is never modified.
- Preview is a lower-resolution proxy produced from the same immutable plan and filter graph as the final export. The proxy is not treated as the final deliverable.
- Export output is written to a unique temporary file under the project export workspace, probed, fsynced and atomically renamed only after validation. Every run is tied to the source recording and saved timeline revision.

## 1. Scope

Compose an immutable source MP4 with the current validated timeline into a preview and a final MP4 tutorial. Preview and export must use one deterministic composition plan. The original recording is never overwritten.

## 2. Approved implementation strategy

- FFmpeg owns video decoding, zoom composition, subtitle burn-in, encoding, and output validation.
- Active zoom uses FFmpeg `zoompan` expressions keyed to input timestamps (`it`). Expressions include bounded in/out ramps using each segment's transition duration. Pending/rejected suggestions are excluded.
- Captions are emitted as UTF-8 Advanced SubStation Alpha (ASS) with per-event text, position, font, alignment, foreground and background. FFmpeg's libass-backed `ass` filter performs text layout and script shaping. Use the best available shaper; when the installed FFmpeg exposes the `shaping` option, request `complex` for Arabic/other complex scripts. When the option is unavailable, only enable Arabic caption rendering after the renderer self-test and target-device visual verification confirm the distribution's libass build uses a complex shaper. Do not silently fall back to visibly broken Arabic.
- Preview and export are the exact same rendered MP4 for the selected saved revision. A lower-resolution proxy may be added later, but it must be generated from the same render plan.
- Preview playback uses Tauri's asset protocol with a narrow allow-list limited to `$HOME/Videos/Screenstory/Projects/**/*`; do not widen it to arbitrary home files.

## 3. Composition contract

- Input source recording is resolved from a managed project recording ID, never from an arbitrary frontend path.
- Input timeline must be the current saved revision and pass full schema validation.
- Only accepted AI zoom segments and manual zoom segments are active.
- Pending/rejected AI suggestions are excluded.
- Annotations are rendered through the same layout/shaping implementation used by preview.
- Output retains the original aspect ratio and source frame rate by default.
- Audio from the source is preserved when present; no transcription or generated voice-over.
- Unsupported codec or invalid media receives a typed error with the source preserved.

## 4. Render pipeline

- Build an immutable render plan with explicit timeline revision, source identity, output resolution, source frame rate, active zooms, annotations, fonts, FFmpeg/libass capabilities and renderer version.
- Use FFmpeg filter graphs with arguments passed as fixed argument vectors; no shell interpolation.
- Apply timestamp-based zoom/pan transitions and ASS text overlays with a verified shaping-capable libass renderer.
- Use safe intermediate files under the project export workspace.
- Write to a temporary output file, fsync, run ffprobe validation, and atomically rename on success.
- On failure/cancel, remove partial output but keep the source, timeline and prior exports intact.
- Store export history with source recording ID, timeline revision, renderer version, output properties, file size, and completion status.

## 5. Preview

- Preview must reflect the exact saved revision selected for rendering.
- Do not silently preview unsaved edits as if they were persisted.
- User can seek and play the composed output.
- Preview generation is cancellable and reuses the same render plan and filter composition as export.
- If an exact real-time composed preview cannot meet performance targets, use a low-resolution proxy from the same plan and clearly label it.

## 6. Progress and cancellation

- Render runs persist status and expose progress.
- Cancellation terminates the FFmpeg child process safely, waits for termination, and deletes temporary output.
- Application exit marks active render as interrupted and preserves the last successful export.
- Progress uses FFmpeg's machine-readable progress output, not estimated timers.

## 7. Resource constraints

- Bound output dimensions, duration, process count and temporary disk use.
- Never hold entire video data in memory.
- Check available disk space before render and show expected output/temporary storage estimate.
- Avoid CPU-intensive work on the UI thread.

## 8. Acceptance criteria

1. Saved valid timeline yields a deterministic render plan.
2. Pending/rejected AI zooms do not appear in preview/export.
3. Accepted and manual zooms appear at correct timestamps and regions.
4. Arabic/English annotations render with correct shaping and direction.
5. Source audio is preserved when present.
6. Export progress and cancellation work and survive app restarts as truthful states.
7. Failed/cancelled export leaves source, timeline and previous exports intact.
8. ffprobe verifies the final MP4 is decodable and matches expected properties.
9. Render history identifies the exact timeline revision used.
10. Automated tests cover render-plan validation, zoom expression generation, ASS escaping/color/time generation, FFmpeg argument generation, atomic output finalization, cancellation, and export-history persistence.
11. Preview loads only the generated file through a narrowly scoped Tauri asset protocol and cannot expose arbitrary user files.
