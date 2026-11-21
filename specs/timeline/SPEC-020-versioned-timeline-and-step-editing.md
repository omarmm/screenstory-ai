# SPEC-020 — Versioned Timeline Persistence and Step Editing

**Status:** Approved for implementation  
**Milestone:** M4 — editable tutorial draft  
**Related:** SPEC-007, SPEC-008, SPEC-009, SPEC-010, SPEC-019

## 1. Purpose

Convert a completed, validated AI analysis draft into a separate editable timeline and allow the user to create, edit, remove, and reorder tutorial steps. The original MP4 and AI analysis draft remain immutable.

## 2. Timeline model v1

A timeline JSON document contains:
- `schema_version: 1`
- `timeline_id`, `recording_id`, source filename and `source_duration_ms`
- monotonic `revision`, `created_at_ms`, `updated_at_ms`
- `status` (`draft` or `ready_for_review`)
- ordered `steps`
- `zoom_segments` (AI suggestions are copied in `pending` state; manual/accepted/rejected status is persisted)
- `annotations` (empty in M4; introduced by M5)
- `source_analysis_run_id`

Each step has stable ID, title, description, start/end milliseconds, origin (`ai` or `manual`), evidence frame IDs (empty for manual steps), evidence kind and confidence (AI only).

The AI analysis draft remains stored in its original run folder and is not mutated by timeline editing.

## 3. Persistence and concurrency

- Store JSON snapshots in `timeline/{timeline_id}/revision-{revision}.json`.
- Store the current timeline pointer in SQLite (`timelines` table): timeline ID, recording ID (unique), source analysis run ID, schema version, current revision, current snapshot filename, creation/update timestamps.
- Save is optimistic: every save carries `expected_revision`; reject stale writes with `TIMELINE_REVISION_CONFLICT`.
- Write and fsync a new temporary snapshot, atomically rename to the revision filename, then update the SQLite pointer in a transaction. Never overwrite an earlier revision.
- On database failure, the previous revision remains the active pointer; orphaned unreferenced snapshots may be cleaned up later.
- Every path is derived from a validated timeline/recording ID and canonicalized within the project's managed timeline directory.
- A second timeline cannot be created for the same recording in M4. Later replacement requires an explicit user operation.
- Re-analysis creates another draft only; it cannot replace an existing timeline.

## 4. Step editing

- Create a timeline only from a completed analysis draft after explicit user action.
- Add a manual step, edit title/description/timing, delete a step, and move a step up/down.
- Title: 1–160 characters. Description: 1–1,200 characters.
- Start/end are integer milliseconds; require (0 \le start < end \le source\_duration\).
- M4 requires ordered, non-overlapping step ranges. Reordering is accepted only if the resulting steps remain chronologically valid; otherwise the user must adjust timing first.
- AI-origin steps preserve evidence IDs, confidence, and evidence kind. Manual steps use empty evidence IDs, null confidence, and origin `manual`.
- Manual fields and ordering are never overwritten by re-analysis.
- Empty timelines are invalid; at least one step must remain.

## 5. Validation

- Reject unknown schema versions, missing/oversized fields, duplicate/invalid IDs, missing source recording, invalid timestamps, out-of-range values, unsorted or overlapping steps, duplicate evidence IDs, or evidence references outside the source analysis manifest.
- Validate every snapshot in Rust before persistence and when reading it.
- Reject malformed JSON rather than silently repairing it.
- Keep the source recording and completed analysis draft unchanged if any validation or storage operation fails.

## 6. UI

- Completed analysis review offers `Create editable timeline`.
- If a timeline already exists for the recording, show `Open timeline` instead; do not make a second implicitly.
- Timeline editor shows source filename, duration, revision, ordered steps, time ranges, evidence references, and an explicit Save action.
- Add / remove / edit / move controls are functional, not decorative.
- Show save progress, saved revision, validation messages, and conflicts.
- If another revision has been saved, reload the latest revision and present the conflict without overwriting it.
- All step edits remain local; no cloud transfer occurs during timeline editing.

## 7. Acceptance criteria

1. A completed AI draft can be converted to an editable timeline without changing the original draft or source MP4.
2. Timeline JSON and its SQLite pointer survive application restart.
3. Add/edit/delete/reorder step operations are persisted as new revisions.
4. Invalid timing, duplicated IDs, unsupported evidence references, and overlaps are rejected.
5. A stale expected revision cannot overwrite a newer snapshot.
6. Re-analysis never mutates an existing timeline or manual edit.
7. Timeline reads validate schema and managed paths.
8. Automated tests cover serialization, migration/create, validation, revision conflict, atomic snapshot sequence, and step mutations.
