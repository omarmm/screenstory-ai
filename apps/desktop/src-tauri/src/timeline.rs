use super::{analysis, db_connection, AppError, AppState};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::State;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TimelineDocument {
    pub schema_version: u32,
    pub timeline_id: String,
    pub recording_id: String,
    pub source_file_name: String,
    pub source_duration_ms: u64,
    pub source_analysis_run_id: String,
    pub revision: u64,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub status: String,
    pub language: String,
    pub steps: Vec<TimelineStep>,
    pub zoom_segments: Vec<TimelineZoomSegment>,
    #[serde(default)]
    pub annotations: Vec<TimelineAnnotation>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TimelineStep {
    pub id: String,
    pub title: String,
    pub description: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub origin: String,
    pub evidence_frame_ids: Vec<String>,
    pub confidence: Option<f64>,
    pub evidence_kind: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TimelineZoomSegment {
    pub id: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub target_scale: f64,
    pub rationale: String,
    pub evidence_frame_ids: Vec<String>,
    pub origin: String,
    pub approval_status: String,
    #[serde(default = "default_transition_duration_ms")]
    pub transition_duration_ms: u64,
}

fn default_transition_duration_ms() -> u64 { 250 }

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TimelineAnnotation {
    pub id: String,
    pub text: String,
    pub language: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub x: f64,
    pub y: f64,
    pub max_width: f64,
    pub font_size: u32,
    pub foreground: String,
    pub background: String,
    pub alignment: String,
    pub direction: String,
    pub origin: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SourceManifest {
    pub(super) run_id: String,
    pub(super) recording_id: String,
    pub(super) source_file_name: String,
    pub(super) source_duration_ms: u64,
    pub(super) frames: Vec<SourceFrame>,
}
#[derive(Debug, Deserialize)]
pub(super) struct SourceFrame {
    pub(super) id: String,
}

pub(super) fn ensure_timeline_table(project_dir: &Path) -> Result<(), AppError> {
    let connection = db_connection(project_dir)?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS timelines (
            timeline_id TEXT PRIMARY KEY,
            recording_id TEXT NOT NULL UNIQUE,
            source_analysis_run_id TEXT NOT NULL,
            schema_version INTEGER NOT NULL,
            current_revision INTEGER NOT NULL,
            current_snapshot TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );",
    )
    .map_err(|_| {
        AppError::new(
            "TIMELINE_DATABASE_ERROR",
            "The project's timeline index could not be initialized.",
        )
    })?;
    Ok(())
}

pub(super) fn valid_timeline_id(value: &str) -> bool {
    value.starts_with('t')
        && value.len() > 2
        && value
            .chars()
            .all(|character| character == 't' || character == '-' || character.is_ascii_digit())
}

fn validate_source_run_id(value: &str) -> bool {
    analysis::analysis_run_id_valid(value)
}

pub(super) fn source_manifest(project_dir: &Path, run_id: &str) -> Result<(PathBuf, SourceManifest), AppError> {
    if !validate_source_run_id(run_id) {
        return Err(AppError::new(
            "INVALID_ANALYSIS_RUN_ID",
            "The source analysis run identifier is invalid.",
        ));
    }
    let analysis_root = project_dir.join("analysis").canonicalize().map_err(|_| {
        AppError::new(
            "ANALYSIS_MANIFEST_MISSING",
            "The project analysis folder is unavailable.",
        )
    })?;
    let run_dir = analysis_root.join(run_id).canonicalize().map_err(|_| {
        AppError::new(
            "ANALYSIS_MANIFEST_MISSING",
            "The source analysis folder could not be found.",
        )
    })?;
    if !run_dir.starts_with(&analysis_root) {
        return Err(AppError::new(
            "ANALYSIS_PATH_INVALID",
            "The source analysis folder is outside the managed project directory.",
        ));
    }
    let manifest_path = run_dir.join("manifest.json").canonicalize().map_err(|_| {
        AppError::new(
            "ANALYSIS_MANIFEST_MISSING",
            "The source frame manifest could not be found.",
        )
    })?;
    if !manifest_path.starts_with(&run_dir) {
        return Err(AppError::new(
            "ANALYSIS_PATH_INVALID",
            "The source manifest is outside its managed analysis folder.",
        ));
    }
    let manifest: SourceManifest =
        serde_json::from_slice(&fs::read(&manifest_path).map_err(|_| {
            AppError::new(
                "ANALYSIS_MANIFEST_MISSING",
                "The source frame manifest could not be read.",
            )
        })?)
        .map_err(|_| {
            AppError::new(
                "ANALYSIS_MANIFEST_INVALID",
                "The source frame manifest is invalid.",
            )
        })?;
    if manifest.run_id != run_id || manifest.frames.is_empty() || manifest.frames.len() > 60 {
        return Err(AppError::new(
            "ANALYSIS_MANIFEST_MISMATCH",
            "The source frame manifest does not match the analysis run.",
        ));
    }
    Ok((run_dir, manifest))
}

fn known_frame_ids(manifest: &SourceManifest) -> HashSet<&str> {
    manifest.frames.iter().map(|frame| frame.id.as_str()).collect()
}

pub(super) fn validate_timeline(
    timeline: &TimelineDocument,
    manifest: &SourceManifest,
) -> Result<(), AppError> {
    if timeline.schema_version != 2 {
        return Err(AppError::new(
            "TIMELINE_SCHEMA_UNSUPPORTED",
            "This timeline uses an unsupported schema version.",
        ));
    }
    if !valid_timeline_id(&timeline.timeline_id)
        || !validate_source_run_id(&timeline.source_analysis_run_id)
        || timeline.recording_id.is_empty()
        || timeline.source_file_name.is_empty()
        || Path::new(&timeline.source_file_name).file_name().and_then(|v| v.to_str())
            != Some(timeline.source_file_name.as_str())
        || timeline.source_duration_ms == 0
        || timeline.revision == 0
        || timeline.created_at_ms == 0
        || timeline.updated_at_ms < timeline.created_at_ms
        || !["draft", "ready_for_review"].contains(&timeline.status.as_str())
        || !["auto", "en", "ar"].contains(&timeline.language.as_str())
        || timeline.source_analysis_run_id != manifest.run_id
        || timeline.recording_id != manifest.recording_id
        || timeline.source_file_name != manifest.source_file_name
        || timeline.source_duration_ms != manifest.source_duration_ms
    {
        return Err(AppError::new(
            "TIMELINE_METADATA_INVALID",
            "The timeline metadata does not match its source recording and analysis.",
        ));
    }
    if timeline.steps.is_empty() || timeline.steps.len() > 60 {
        return Err(AppError::new(
            "TIMELINE_STEPS_INVALID",
            "A timeline must contain between 1 and 60 tutorial steps.",
        ));
    }

    let known = known_frame_ids(manifest);
    let mut step_ids = HashSet::new();
    let mut previous_end = 0_u64;
    for (index, step) in timeline.steps.iter().enumerate() {
        if step.id.trim().is_empty()
            || step.id.len() > 128
            || !step_ids.insert(step.id.as_str())
            || step.title.trim().is_empty()
            || step.title.chars().count() > 160
            || step.description.trim().is_empty()
            || step.description.chars().count() > 1200
            || step.start_ms >= step.end_ms
            || step.end_ms > timeline.source_duration_ms
            || (index > 0 && step.start_ms < previous_end)
        {
            return Err(AppError::new(
                "TIMELINE_STEP_INVALID",
                "A tutorial step has invalid text, an overlapping time range, or an invalid order.",
            ));
        }
        previous_end = step.end_ms;
        match step.origin.as_str() {
            "ai" => {
                if step.evidence_frame_ids.is_empty()
                    || step
                        .evidence_frame_ids
                        .iter()
                        .any(|id| !known.contains(id.as_str()))
                    || step.confidence.is_none_or(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
                    || !["observed", "inference"].contains(&step.evidence_kind.as_deref().unwrap_or(""))
                {
                    return Err(AppError::new(
                        "TIMELINE_EVIDENCE_INVALID",
                        "An AI-generated step is missing valid source-frame evidence.",
                    ));
                }
            }
            "manual" => {
                if !step.evidence_frame_ids.is_empty()
                    || step.confidence.is_some()
                    || step.evidence_kind.is_some()
                {
                    return Err(AppError::new(
                        "TIMELINE_MANUAL_STEP_INVALID",
                        "A manual step cannot claim AI evidence or confidence.",
                    ));
                }
            }
            _ => {
                return Err(AppError::new(
                    "TIMELINE_STEP_ORIGIN_INVALID",
                    "The tutorial step origin is invalid.",
                ))
            }
        }
    }

    let mut zoom_ids = HashSet::new();
    for zoom in &timeline.zoom_segments {
        if zoom.id.trim().is_empty()
            || zoom.id.len() > 128
            || !zoom_ids.insert(zoom.id.as_str())
            || zoom.start_ms >= zoom.end_ms
            || zoom.end_ms > timeline.source_duration_ms
            || !zoom.x.is_finite()
            || !zoom.y.is_finite()
            || !zoom.width.is_finite()
            || !zoom.height.is_finite()
            || zoom.x < 0.0
            || zoom.y < 0.0
            || zoom.width <= 0.0
            || zoom.height <= 0.0
            || zoom.x + zoom.width > 1.0
            || zoom.y + zoom.height > 1.0
            || !zoom.target_scale.is_finite()
            || !(1.0..=3.0).contains(&zoom.target_scale)
            || zoom.rationale.trim().is_empty()
            || !["ai", "manual"].contains(&zoom.origin.as_str())
            || zoom.transition_duration_ms > 1500
            || !["pending", "accepted", "rejected"].contains(&zoom.approval_status.as_str())
            || (zoom.origin == "ai"
                && (zoom.evidence_frame_ids.is_empty()
                    || zoom
                        .evidence_frame_ids
                        .iter()
                        .any(|id| !known.contains(id.as_str()))))
            || (zoom.origin == "manual" && !zoom.evidence_frame_ids.is_empty())
        {
            return Err(AppError::new(
                "TIMELINE_ZOOM_INVALID",
                "A zoom segment contains invalid timing, region, scale, approval state, or evidence.",
            ));
        }
    }

    let mut active_zooms = timeline.zoom_segments.iter()
        .filter(|zoom| zoom.origin == "manual" || zoom.approval_status == "accepted")
        .collect::<Vec<_>>();
    active_zooms.sort_by_key(|zoom| zoom.start_ms);
    if active_zooms.windows(2).any(|pair| pair[0].end_ms > pair[1].start_ms) {
        return Err(AppError::new(
            "TIMELINE_ZOOM_OVERLAP",
            "Accepted zoom segments cannot overlap. Adjust the active zoom time ranges before saving.",
        ));
    }

    let mut annotation_ids = HashSet::new();
    for annotation in &timeline.annotations {
        let valid_color = |color: &str| {
            (color.len() == 7 || color.len() == 9)
                && color.starts_with('#')
                && color[1..].chars().all(|character| character.is_ascii_hexdigit())
        };
        if annotation.id.trim().is_empty()
            || annotation.id.len() > 128
            || !annotation_ids.insert(annotation.id.as_str())
            || annotation.text.trim().is_empty()
            || annotation.text.chars().count() > 500
            || annotation.text.chars().any(|character| character.is_control() && character != '\n' && character != '\t')
            || !["ar", "en", "mixed"].contains(&annotation.language.as_str())
            || annotation.start_ms >= annotation.end_ms
            || annotation.end_ms > timeline.source_duration_ms
            || !annotation.x.is_finite()
            || !annotation.y.is_finite()
            || !annotation.max_width.is_finite()
            || annotation.x < 0.0
            || annotation.y < 0.0
            || annotation.max_width <= 0.0
            || annotation.x + annotation.max_width > 1.0
            || annotation.y >= 1.0
            || !(12..=72).contains(&annotation.font_size)
            || !valid_color(&annotation.foreground)
            || !valid_color(&annotation.background)
            || !["left", "center", "right"].contains(&annotation.alignment.as_str())
            || !["rtl", "ltr", "auto"].contains(&annotation.direction.as_str())
            || annotation.origin != "manual"
        {
            return Err(AppError::new(
                "TIMELINE_ANNOTATION_INVALID",
                "A text annotation contains invalid content, language, timing, position, font, color, or direction.",
            ));
        }
    }
    Ok(())
}

fn migrate_v1_timeline(value: &mut Value) -> Result<bool, AppError> {
    let version = value.get("schemaVersion").and_then(Value::as_u64)
        .ok_or_else(|| AppError::new("TIMELINE_SCHEMA_UNSUPPORTED", "The timeline schema version is missing or invalid."))?;
    if version == 2 {
        return Ok(false);
    }
    if version != 1 {
        return Err(AppError::new("TIMELINE_SCHEMA_UNSUPPORTED", "This timeline uses an unsupported schema version."));
    }
    value["schemaVersion"] = Value::from(2);
    if let Some(zooms) = value.get_mut("zoomSegments").and_then(Value::as_array_mut) {
        for zoom in zooms {
            if zoom.get("transitionDurationMs").is_none() {
                zoom["transitionDurationMs"] = Value::from(250);
            }
        }
    }
    if value.get("annotations").is_none() {
        value["annotations"] = Value::Array(Vec::new());
    }
    Ok(true)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let parent = path.parent().ok_or_else(|| {
        AppError::new(
            "TIMELINE_STORAGE_FAILED",
            "The timeline snapshot folder is invalid.",
        )
    })?;
    fs::create_dir_all(parent).map_err(|_| {
        AppError::new(
            "TIMELINE_STORAGE_FAILED",
            "The timeline snapshot folder could not be created.",
        )
    })?;
    let file_name = path.file_name().and_then(|name| name.to_str()).ok_or_else(|| {
        AppError::new(
            "TIMELINE_STORAGE_FAILED",
            "The timeline snapshot filename is invalid.",
        )
    })?;
    let temporary = parent.join(format!("{file_name}.tmp"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|_| {
            AppError::new(
                "TIMELINE_STORAGE_FAILED",
                "A temporary timeline snapshot could not be created.",
            )
        })?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| {
            let _ = fs::remove_file(&temporary);
            AppError::new(
                "TIMELINE_STORAGE_FAILED",
                "The timeline snapshot could not be written to disk.",
            )
        })?;
    drop(file);
    fs::rename(&temporary, path).map_err(|_| {
        let _ = fs::remove_file(&temporary);
        AppError::new(
            "TIMELINE_STORAGE_FAILED",
            "The timeline snapshot could not be finalized.",
        )
    })?;
    Ok(())
}

#[tauri::command]
pub fn create_timeline_from_draft(
    run_id: String,
    state: State<'_, AppState>,
) -> Result<TimelineDocument, AppError> {
    if !validate_source_run_id(&run_id) {
        return Err(AppError::new(
            "INVALID_ANALYSIS_RUN_ID",
            "The source analysis run identifier is invalid.",
        ));
    }
    let project_dir = state
        .active_project
        .lock()
        .map_err(|_| AppError::new("STATE_LOCK_ERROR", "Project state is temporarily unavailable."))?
        .as_ref()
        .cloned()
        .ok_or_else(|| AppError::new("NO_ACTIVE_PROJECT", "Open a project before creating a timeline."))?;
    ensure_timeline_table(&project_dir)?;
    let connection = db_connection(&project_dir)?;
    let (recording_id, source_file_name, draft_path, status): (String, String, Option<String>, String) =
        connection.query_row(
            "SELECT recording_id, source_file_name, draft_path, status FROM analysis_runs WHERE id=?1",
            params![run_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        ).map_err(|_| AppError::new("ANALYSIS_RUN_NOT_FOUND", "The completed analysis run could not be found."))?;
    if status != "completed" {
        return Err(AppError::new("ANALYSIS_RUN_NOT_COMPLETED", "Only a successfully completed analysis draft can be converted into a timeline."));
    }
    let existing: Option<String> = connection.query_row(
        "SELECT timeline_id FROM timelines WHERE recording_id=?1",
        params![recording_id],
        |row| row.get(0),
    ).optional().map_err(|_| AppError::new("TIMELINE_DATABASE_ERROR", "The timeline index could not be read."))?;
    if existing.is_some() {
        return Err(AppError::new("TIMELINE_ALREADY_EXISTS", "This recording already has an editable timeline. Open the existing timeline instead."));
    }
    let (run_dir, manifest) = source_manifest(&project_dir, &run_id)?;
    if manifest.recording_id != recording_id || manifest.source_file_name != source_file_name {
        return Err(AppError::new("ANALYSIS_MANIFEST_MISMATCH", "The analysis manifest does not match the source recording."));
    }
    let draft_path = PathBuf::from(draft_path.ok_or_else(|| {
        AppError::new("ANALYSIS_DRAFT_NOT_FOUND", "The analysis run does not contain a completed tutorial draft.")
    })?);
    let canonical_draft = draft_path.canonicalize().map_err(|_| {
        AppError::new("ANALYSIS_DRAFT_NOT_FOUND", "The tutorial draft could not be read.")
    })?;
    if !canonical_draft.starts_with(&run_dir) {
        return Err(AppError::new("ANALYSIS_PATH_INVALID", "The tutorial draft is outside its managed analysis folder."));
    }
    let draft: analysis::AnalysisDraft = serde_json::from_slice(&fs::read(&canonical_draft).map_err(|_| {
        AppError::new("ANALYSIS_DRAFT_NOT_FOUND", "The tutorial draft could not be read.")
    })?).map_err(|_| AppError::new("ANALYSIS_DRAFT_INVALID", "The tutorial draft is invalid."))?;

    let created_at_ms = now_ms();
    let timeline_id = format!("t{}-{}", created_at_ms, std::process::id());
    let timeline = TimelineDocument {
        schema_version: 2,
        timeline_id: timeline_id.clone(),
        recording_id: recording_id.clone(),
        source_file_name: source_file_name.clone(),
        source_duration_ms: manifest.source_duration_ms,
        source_analysis_run_id: run_id.clone(),
        revision: 1,
        created_at_ms,
        updated_at_ms: created_at_ms,
        status: "draft".to_string(),
        language: draft.language.clone(),
        steps: draft.steps.into_iter().map(|step| TimelineStep {
            id: step.id,
            title: step.title,
            description: step.description,
            start_ms: step.start_ms,
            end_ms: step.end_ms,
            origin: "ai".to_string(),
            evidence_frame_ids: step.evidence_frame_ids,
            confidence: Some(step.confidence),
            evidence_kind: Some(step.evidence_kind),
        }).collect(),
        zoom_segments: draft.zoom_suggestions.into_iter().map(|zoom| TimelineZoomSegment {
            id: zoom.id,
            start_ms: zoom.start_ms,
            end_ms: zoom.end_ms,
            x: zoom.x,
            y: zoom.y,
            width: zoom.width,
            height: zoom.height,
            target_scale: zoom.target_scale,
            rationale: zoom.rationale,
            evidence_frame_ids: zoom.evidence_frame_ids,
            origin: "ai".to_string(),
            approval_status: "pending".to_string(),
            transition_duration_ms: 250,
        }).collect(),
        annotations: Vec::new(),
    };
    validate_timeline(&timeline, &manifest)?;
    let relative_snapshot = format!("{}/revision-1.json", timeline_id);
    let snapshot_dir = project_dir.join("timeline").join(&timeline_id);
    let root = project_dir.join("timeline");
    fs::create_dir_all(&root).map_err(|_| AppError::new("TIMELINE_STORAGE_FAILED", "The project's timeline folder could not be created."))?;
    let canonical_root = root.canonicalize().map_err(|_| AppError::new("TIMELINE_STORAGE_FAILED", "The project's timeline folder is unavailable."))?;
    fs::create_dir_all(&snapshot_dir).map_err(|_| AppError::new("TIMELINE_STORAGE_FAILED", "The timeline snapshot folder could not be created."))?;
    let canonical_dir = snapshot_dir.canonicalize().map_err(|_| AppError::new("TIMELINE_STORAGE_FAILED", "The timeline snapshot folder is unavailable."))?;
    if !canonical_dir.starts_with(&canonical_root) {
        return Err(AppError::new("TIMELINE_PATH_INVALID", "The timeline snapshot folder is outside this project."));
    }
    let snapshot_path = canonical_dir.join("revision-1.json");
    let bytes = serde_json::to_vec_pretty(&timeline).map_err(|_| AppError::new("TIMELINE_SERIALIZATION_FAILED", "The timeline could not be serialized."))?;
    atomic_write(&snapshot_path, &bytes)?;
    connection.execute(
        "INSERT INTO timelines (timeline_id, recording_id, source_analysis_run_id, schema_version, current_revision, current_snapshot, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, 2, 1, ?4, ?5, ?5)",
        params![timeline_id, recording_id, run_id, relative_snapshot, created_at_ms as i64],
    ).map_err(|_| AppError::new("TIMELINE_DATABASE_ERROR", "The timeline snapshot was saved, but the project index could not be updated."))?;
    Ok(timeline)
}

#[tauri::command]
pub fn get_timeline_for_recording(
    recording_id: String,
    state: State<'_, AppState>,
) -> Result<Option<TimelineDocument>, AppError> {
    let project_dir = state
        .active_project
        .lock()
        .map_err(|_| AppError::new("STATE_LOCK_ERROR", "Project state is temporarily unavailable."))?
        .as_ref()
        .cloned()
        .ok_or_else(|| AppError::new("NO_ACTIVE_PROJECT", "Open a project to view its timeline."))?;
    ensure_timeline_table(&project_dir)?;
    let connection = db_connection(&project_dir)?;
    let row: Option<(String, String, i64, String)> = connection.query_row(
        "SELECT timeline_id, source_analysis_run_id, current_revision, current_snapshot FROM timelines WHERE recording_id=?1",
        params![recording_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    ).optional().map_err(|_| AppError::new("TIMELINE_DATABASE_ERROR", "The timeline index could not be read."))?;
    let Some((timeline_id, run_id, revision, current_snapshot)) = row else {
        return Ok(None);
    };
    if !valid_timeline_id(&timeline_id) || !validate_source_run_id(&run_id) || revision < 1 {
        return Err(AppError::new("TIMELINE_INDEX_INVALID", "The project's timeline index contains invalid metadata."));
    }
    let root = project_dir.join("timeline").canonicalize().map_err(|_| AppError::new("TIMELINE_NOT_FOUND", "The timeline folder could not be found."))?;
    let timeline_dir = root.join(&timeline_id).canonicalize().map_err(|_| AppError::new("TIMELINE_NOT_FOUND", "The timeline snapshot folder could not be found."))?;
    if !timeline_dir.starts_with(&root) {
        return Err(AppError::new("TIMELINE_PATH_INVALID", "The timeline snapshot folder is outside the managed project directory."));
    }
    let expected_name = format!("revision-{revision}.json");
    if current_snapshot != format!("{timeline_id}/{expected_name}") {
        return Err(AppError::new("TIMELINE_INDEX_INVALID", "The active timeline snapshot pointer is invalid."));
    }
    let snapshot = timeline_dir.join(expected_name).canonicalize().map_err(|_| AppError::new("TIMELINE_NOT_FOUND", "The current timeline snapshot could not be found."))?;
    if !snapshot.starts_with(&timeline_dir) {
        return Err(AppError::new("TIMELINE_PATH_INVALID", "The current timeline snapshot is outside its managed folder."));
    }
    let raw = fs::read(&snapshot).map_err(|_| AppError::new("TIMELINE_NOT_FOUND", "The current timeline snapshot could not be read."))?;
    let mut value: Value = serde_json::from_slice(&raw)
        .map_err(|_| AppError::new("TIMELINE_SNAPSHOT_INVALID", "The current timeline snapshot is invalid."))?;
    let migrated = migrate_v1_timeline(&mut value)?;
    let mut timeline: TimelineDocument = serde_json::from_value(value)
        .map_err(|_| AppError::new("TIMELINE_SNAPSHOT_INVALID", "The current timeline snapshot is invalid."))?;
    let (_, manifest) = source_manifest(&project_dir, &run_id)?;
    if migrated {
        timeline.revision = revision as u64 + 1;
        timeline.updated_at_ms = now_ms();
        let next_name = format!("revision-{}.json", timeline.revision);
        let next_relative = format!("{}/{}", timeline_id, next_name);
        let next_path = timeline_dir.join(next_name);
        let bytes = serde_json::to_vec_pretty(&timeline)
            .map_err(|_| AppError::new("TIMELINE_SERIALIZATION_FAILED", "The migrated timeline could not be serialized."))?;
        validate_timeline(&timeline, &manifest)?;
        atomic_write(&next_path, &bytes)?;
        let changed = connection.execute(
            "UPDATE timelines SET schema_version=2, current_revision=?1, current_snapshot=?2, updated_at_ms=?3
             WHERE timeline_id=?4 AND current_revision=?5",
            params![timeline.revision as i64, next_relative, timeline.updated_at_ms as i64, timeline_id, revision],
        ).map_err(|_| AppError::new("TIMELINE_DATABASE_ERROR", "The migrated timeline index could not be updated."))?;
        if changed != 1 {
            let _ = fs::remove_file(next_path);
            return Err(AppError::new("TIMELINE_REVISION_CONFLICT", "The timeline changed while its schema was being upgraded. Reload it."));
        }
    }
    validate_timeline(&timeline, &manifest)?;
    if timeline.timeline_id != timeline_id || timeline.revision != (revision as u64 + u64::from(migrated)) || timeline.recording_id != recording_id {
        return Err(AppError::new("TIMELINE_INDEX_MISMATCH", "The current timeline snapshot does not match the project index."));
    }
    Ok(Some(timeline))
}

#[tauri::command]
pub fn save_timeline(
    mut timeline: TimelineDocument,
    expected_revision: u64,
    state: State<'_, AppState>,
) -> Result<TimelineDocument, AppError> {
    if !valid_timeline_id(&timeline.timeline_id) {
        return Err(AppError::new("INVALID_TIMELINE_ID", "The timeline identifier is invalid."));
    }
    let project_dir = state
        .active_project
        .lock()
        .map_err(|_| AppError::new("STATE_LOCK_ERROR", "Project state is temporarily unavailable."))?
        .as_ref()
        .cloned()
        .ok_or_else(|| AppError::new("NO_ACTIVE_PROJECT", "Open a project before saving a timeline."))?;
    ensure_timeline_table(&project_dir)?;
    let connection = db_connection(&project_dir)?;
    connection.execute_batch("BEGIN IMMEDIATE").map_err(|_| AppError::new("TIMELINE_DATABASE_ERROR", "The timeline could not acquire the project write lock."))?;
    let (recording_id, run_id, current_revision, snapshot): (String, String, i64, String) =
        connection.query_row(
            "SELECT recording_id, source_analysis_run_id, current_revision, current_snapshot FROM timelines WHERE timeline_id=?1",
            params![timeline.timeline_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        ).map_err(|_| AppError::new("TIMELINE_NOT_FOUND", "This editable timeline could not be found."))?;
    if current_revision < 1 || expected_revision != current_revision as u64 {
        return Err(AppError::new("TIMELINE_REVISION_CONFLICT", "A newer timeline revision already exists. Reload the latest revision before saving your changes."));
    }
    if timeline.revision != expected_revision
        || timeline.recording_id != recording_id
        || timeline.source_analysis_run_id != run_id
    {
        return Err(AppError::new("TIMELINE_METADATA_IMMUTABLE", "The timeline source or revision metadata cannot be changed by the editor."));
    }
    let (run_dir, manifest) = source_manifest(&project_dir, &run_id)?;
    let _ = run_dir;
    validate_timeline(&timeline, &manifest)?;
    let current_file = format!("{}/revision-{}.json", timeline.timeline_id, expected_revision);
    if snapshot != current_file {
        return Err(AppError::new("TIMELINE_INDEX_INVALID", "The current snapshot pointer does not match the expected revision."));
    }
    let current_path = project_dir.join("timeline").join(&current_file).canonicalize().map_err(|_| {
        AppError::new("TIMELINE_NOT_FOUND", "The previous timeline revision could not be read.")
    })?;
    let root = project_dir.join("timeline").canonicalize().map_err(|_| {
        AppError::new("TIMELINE_NOT_FOUND", "The project timeline folder is unavailable.")
    })?;
    if !current_path.starts_with(&root) {
        return Err(AppError::new("TIMELINE_PATH_INVALID", "The previous timeline revision is outside the managed project folder."));
    }
    let previous: TimelineDocument = serde_json::from_slice(&fs::read(current_path).map_err(|_| {
        AppError::new("TIMELINE_NOT_FOUND", "The previous timeline revision could not be read.")
    })?).map_err(|_| AppError::new("TIMELINE_SNAPSHOT_INVALID", "The previous timeline revision is invalid."))?;
    timeline.created_at_ms = previous.created_at_ms;
    timeline.updated_at_ms = now_ms();
    timeline.revision = expected_revision + 1;
    validate_timeline(&timeline, &manifest)?;
    let next_file_name = format!("revision-{}.json", timeline.revision);
    let next_relative = format!("{}/{next_file_name}", timeline.timeline_id);
    let timeline_dir = root.join(&timeline.timeline_id).canonicalize().map_err(|_| {
        AppError::new("TIMELINE_NOT_FOUND", "The timeline folder could not be found.")
    })?;
    if !timeline_dir.starts_with(&root) {
        return Err(AppError::new("TIMELINE_PATH_INVALID", "The timeline folder is outside the managed project directory."));
    }
    let next_path = timeline_dir.join(&next_file_name);
    let bytes = serde_json::to_vec_pretty(&timeline).map_err(|_| {
        AppError::new("TIMELINE_SERIALIZATION_FAILED", "The timeline could not be serialized.")
    })?;
    atomic_write(&next_path, &bytes)?;
    let updated = connection.execute(
        "UPDATE timelines SET schema_version=2, current_revision=?1, current_snapshot=?2, updated_at_ms=?3
         WHERE timeline_id=?4 AND current_revision=?5",
        params![timeline.revision as i64, next_relative, timeline.updated_at_ms as i64, timeline.timeline_id, expected_revision as i64],
    ).map_err(|_| AppError::new("TIMELINE_DATABASE_ERROR", "The new timeline revision was written, but the project index could not be updated."))?;
    if updated != 1 {
        let _ = connection.execute_batch("ROLLBACK");
        let _ = fs::remove_file(&next_path);
        return Err(AppError::new("TIMELINE_REVISION_CONFLICT", "A newer timeline revision already exists. Reload before saving."));
    }
    connection.execute_batch("COMMIT").map_err(|_| {
        let _ = fs::remove_file(&next_path);
        AppError::new("TIMELINE_DATABASE_ERROR", "The timeline index could not commit the new revision.")
    })?;
    Ok(timeline)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example_manifest() -> SourceManifest {
        SourceManifest {
            run_id: "a12345-77".to_string(),
            recording_id: "capture.mp4".to_string(),
            source_file_name: "capture.mp4".to_string(),
            source_duration_ms: 10_000,
            frames: vec![SourceFrame { id: "frame-000".to_string() }],
        }
    }

    fn example_timeline() -> TimelineDocument {
        TimelineDocument {
            schema_version: 2,
            timeline_id: "t12345-77".to_string(),
            recording_id: "capture.mp4".to_string(),
            source_file_name: "capture.mp4".to_string(),
            source_duration_ms: 10_000,
            source_analysis_run_id: "a12345-77".to_string(),
            revision: 1,
            created_at_ms: 100,
            updated_at_ms: 100,
            status: "draft".to_string(),
            language: "en".to_string(),
            steps: vec![TimelineStep {
                id: "step-1".to_string(),
                title: "Open settings".to_string(),
                description: "Open the settings page.".to_string(),
                start_ms: 0,
                end_ms: 1000,
                origin: "ai".to_string(),
                evidence_frame_ids: vec!["frame-000".to_string()],
                confidence: Some(0.9),
                evidence_kind: Some("observed".to_string()),
            }],
            zoom_segments: vec![],
            annotations: vec![],
        }
    }

    #[test]
    fn annotation_validation_accepts_arabic_rtl_text() {
        let mut timeline = example_timeline();
        timeline.annotations.push(TimelineAnnotation {
            id: "annotation-1".to_string(),
            text: "اضغط على الإعدادات".to_string(),
            language: "ar".to_string(),
            start_ms: 1000,
            end_ms: 3000,
            x: 0.08,
            y: 0.08,
            max_width: 0.7,
            font_size: 26,
            foreground: "#FFFFFF".to_string(),
            background: "#000000CC".to_string(),
            alignment: "right".to_string(),
            direction: "rtl".to_string(),
            origin: "manual".to_string(),
        });
        assert!(validate_timeline(&timeline, &example_manifest()).is_ok());
    }

    #[test]
    fn rejects_overlapping_accepted_zoom_segments_but_keeps_pending_suggestions() {
        let mut timeline = example_timeline();
        let zoom = TimelineZoomSegment {
            id: "zoom-1".to_string(),
            start_ms: 1000,
            end_ms: 3000,
            x: 0.2,
            y: 0.2,
            width: 0.4,
            height: 0.4,
            target_scale: 1.5,
            rationale: "Emphasize the control.".to_string(),
            evidence_frame_ids: vec!["frame-000".to_string()],
            origin: "ai".to_string(),
            approval_status: "accepted".to_string(),
            transition_duration_ms: 250,
        };
        timeline.zoom_segments.push(zoom.clone());
        let mut overlapping = zoom;
        overlapping.id = "zoom-2".to_string();
        overlapping.start_ms = 2500;
        overlapping.end_ms = 4000;
        timeline.zoom_segments.push(overlapping.clone());
        assert_eq!(
            validate_timeline(&timeline, &example_manifest()).unwrap_err().code,
            "TIMELINE_ZOOM_OVERLAP"
        );
        timeline.zoom_segments[0].approval_status = "pending".to_string();
        timeline.zoom_segments[1].approval_status = "pending".to_string();
        assert!(validate_timeline(&timeline, &example_manifest()).is_ok());
    }

    #[test]
    fn rejects_invalid_annotation_coordinates_and_control_characters() {
        let mut timeline = example_timeline();
        timeline.annotations.push(TimelineAnnotation {
            id: "annotation-1".to_string(),
            text: "invalid".to_string(),
            language: "en".to_string(),
            start_ms: 0,
            end_ms: 500,
            x: 0.9,
            y: 0.1,
            max_width: 0.3,
            font_size: 18,
            foreground: "#FFFFFF".to_string(),
            background: "#000000".to_string(),
            alignment: "left".to_string(),
            direction: "ltr".to_string(),
            origin: "manual".to_string(),
        });
        assert_eq!(
            validate_timeline(&timeline, &example_manifest()).unwrap_err().code,
            "TIMELINE_ANNOTATION_INVALID"
        );
    }

    #[test]
    fn migrate_v1_timeline_adds_transition_defaults_and_annotations() {
        let mut value = serde_json::json!({
            "schemaVersion": 1,
            "zoomSegments": [{
                "id":"zoom-1", "startMs":0, "endMs":1000, "x":0.2, "y":0.2,
                "width":0.4, "height":0.4, "targetScale":1.5, "rationale":"Focus",
                "evidenceFrameIds":["frame-000"], "origin":"ai", "approvalStatus":"pending"
            }]
        });
        let changed = migrate_v1_timeline(&mut value).unwrap();
        assert!(changed);
        assert_eq!(value["schemaVersion"], 2);
        assert_eq!(value["zoomSegments"][0]["transitionDurationMs"], 250);
        assert_eq!(value["annotations"], serde_json::json!([]));
    }

    #[test]
    fn accepts_valid_versioned_timeline() {
        assert!(validate_timeline(&example_timeline(), &example_manifest()).is_ok());
    }

    #[test]
    fn rejects_step_outside_source_duration() {
        let mut timeline = example_timeline();
        timeline.steps[0].end_ms = 10_001;
        assert_eq!(
            validate_timeline(&timeline, &example_manifest()).unwrap_err().code,
            "TIMELINE_STEP_INVALID"
        );
    }

    #[test]
    fn rejects_overlapping_or_unsorted_steps() {
        let mut timeline = example_timeline();
        let mut second = timeline.steps[0].clone();
        second.id = "step-2".to_string();
        second.start_ms = 500;
        second.end_ms = 1500;
        timeline.steps.push(second);
        assert_eq!(
            validate_timeline(&timeline, &example_manifest()).unwrap_err().code,
            "TIMELINE_STEP_INVALID"
        );
    }

    #[test]
    fn rejects_ai_steps_without_known_evidence() {
        let mut timeline = example_timeline();
        timeline.steps[0].evidence_frame_ids = vec!["not-a-real-frame".to_string()];
        assert_eq!(
            validate_timeline(&timeline, &example_manifest())
                .unwrap_err()
                .code,
            "TIMELINE_EVIDENCE_INVALID"
        );
    }
}
