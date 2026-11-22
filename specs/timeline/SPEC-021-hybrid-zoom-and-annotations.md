# SPEC-021 — Hybrid Zoom and Bilingual Annotation Editing

**Status:** Approved for implementation after M4 validation  
**Milestone:** M5

## 1. Scope

Add editable zoom segments and text annotations to the versioned timeline without changing the original recording or the immutable AI analysis draft. AI zoom suggestions are advisory until explicitly accepted by the user.

## 2. Zoom behavior

- Display each AI suggestion's evidence frame IDs, time range, normalized region, target scale, and rationale.
- Allow explicit accept/reject. Pending and rejected suggestions are not rendered.
- Accepted AI suggestions remain marked origin `ai`, status `accepted`.
- Allow manual creation, editing, and deletion. Manual zooms have origin `manual`, empty evidence, no AI confidence.
- Validate timestamps against source duration; require start < end.
- Normalized rectangle must be finite, positive, and contained in [0,1] x [0,1].
- Scale range 1.0–3.0.
- Prevent overlapping active zooms in v1. If two accepted segments overlap, user must resolve the conflict before saving.
- Store transition duration in milliseconds, default 250ms, bounded 0–1500ms.
- Keep rejected AI suggestions in the timeline for provenance but exclude them from composition.

## 3. Annotation model

- Annotation fields: stable ID, text, language (`ar`, `en`, `mixed`), start/end ms, normalized x/y anchor, max width fraction, font size, foreground/background, alignment, origin (`manual`), and direction (`rtl`/`ltr`/`auto`).
- Text limit 500 characters; no control characters except line breaks and tab.
- Position and width must be finite and fully contained in normalized frame bounds.
- Time range must be inside the source duration.
- Font size 12–72 logical pixels.
- Default Arabic direction RTL; default English LTR; mixed defaults auto. User can override direction and alignment.
- M5 persists annotation content, direction, alignment, geometry, colors, and font size only. It does not render annotation glyphs into video.
- Arabic/English shaping and font availability are a mandatory M6 renderer proof before preview/export can be enabled. M6 must use Pango/Cairo or another verified shaping-capable renderer, include explicit font fallback (Noto Naskh Arabic / Noto Sans Arabic where installed), and report missing glyph/font availability rather than silently replacing characters.
- Preview and export must use the same verified layout and font-shaping pipeline.

## 4. Persistence

- Extend timeline schema with version 2 and explicit migration from version 1.
- Preserve prior version-1 snapshots.
- Migration is deterministic, atomic, and idempotent; v1 has no annotations and zoom transition defaults to 250ms.
- Use the existing expected-revision save rule.
- Manual edits always take precedence over AI-generated defaults.
- Do not mutate analysis drafts.

## 5. UI

- Zoom track with pending/accepted/rejected state; accept/reject actions, editable time/region/scale/transition and manual zoom creation.
- Annotation editor with text, language, direction, alignment, position, width, font size and preview.
- Show Arabic/English text in the UI and verify RTL/LTR layout.
- Save as a new immutable revision; show validation/conflict messages.

## 6. Acceptance criteria

1. User can accept and reject each AI zoom suggestion.
2. Pending/rejected suggestions are never used by preview/export.
3. Manual zooms and accepted AI zooms persist across restart.
4. Invalid time, geometry, scale or overlapping active segments are rejected.
5. User can create/edit/delete annotations with Arabic and English content; direction and alignment metadata persist through timeline revisions.
6. Timeline schema v1 migrates to v2 without altering old snapshots.
7. Save conflicts cannot overwrite a newer revision.
8. Pending/rejected zoom suggestions remain distinguishable from active zooms in persisted timeline data.
9. M6 cannot enable annotation preview/export until Arabic/English shaping and font fallback have passed a Linux target proof; preview and export must then consume the same approved composition model.

## 7. Out of scope

Video rendering implementation, speech transcription, voice-over, remote project sync, and Wayland recording.


## Amendment — Automatic captions from AI tutorial steps

**Decision:** Captions are generated automatically when a completed AI analysis draft is converted into its first editable timeline. No user action or separate generation button is required.

- Create one editable AI-origin annotation from each non-empty AI tutorial-step title.
- Copy the step's start and end timestamps exactly so the caption follows the corresponding step.
- Detect Arabic and Latin script to choose the annotation language, writing direction, and alignment. Use the draft language only when the title contains no detectable letters.
- Use a readable default caption placement and styling; all generated caption fields remain editable or removable in the timeline editor.
- Preserve manual annotations as manual. AI-generated annotations are explicitly marked as AI-origin and are valid persisted timeline content.
- Do not modify the immutable analysis draft or the source recording. Existing timelines are not silently rewritten; this behavior applies when a new timeline is created from a completed draft.

This amendment makes the tutorial-step-to-on-screen-caption conversion part of the default timeline creation workflow. Export continues to render all valid saved annotations through the existing caption renderer.
