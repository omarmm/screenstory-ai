use super::{db_connection, AppError, AppState};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use keyring::Entry;
use reqwest::blocking::Client;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::State;

const GEMINI_MODEL: &str = "gemini-4-hydrogen";
const GEMINI_ENDPOINT: &str = "https://generativelanguage.googleapis.com/v1beta/interactions";
const MAX_FRAMES: usize = 60;
const MAX_INLINE_REQUEST_BYTES: usize = 12 * 1024 * 1024;
const MAX_EDGE: u32 = 1024;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisStarted {
    run_id: String,
    recording_id: String,
    file_name: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisStatus {
    run_id: String,
    recording_id: String,
    source_file_name: String,
    status: String,
    completed_frames: usize,
    total_frames: usize,
    total_bytes: u64,
    duration_seconds: Option<f64>,
    provider: String,
    model: String,
    error_code: Option<String>,
    workflow_description: String,
    language: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct FrameManifest {
    schema_version: u32,
    run_id: String,
    recording_id: String,
    source_file_name: String,
    source_duration_ms: u64,
    source_width: u32,
    source_height: u32,
    sampling_strategy: String,
    sample_count: usize,
    max_edge: u32,
    total_bytes: u64,
    workflow_description: String,
    language: String,
    frames: Vec<FrameReference>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct FrameReference {
    id: String,
    timestamp_ms: u64,
    relative_path: String,
    width: u32,
    height: u32,
    mime_type: String,
    byte_size: u64,
}

#[derive(Debug, Deserialize)]
struct ProbeOutput {
    streams: Vec<ProbeStream>,
    format: ProbeFormat,
}
#[derive(Debug, Deserialize)]
struct ProbeStream {
    width: Option<u32>,
    height: Option<u32>,
}
#[derive(Debug, Deserialize)]
struct ProbeFormat {
    duration: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisDraft {
    pub schema_version: u32,
    pub summary: String,
    pub steps: Vec<TutorialStep>,
    pub zoom_suggestions: Vec<ZoomSuggestion>,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TutorialStep {
    pub id: String,
    pub title: String,
    pub description: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub evidence_frame_ids: Vec<String>,
    pub confidence: f64,
    pub evidence_kind: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ZoomSuggestion {
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
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiKeyStatus {
    configured: bool,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn keyring_entry() -> Result<Entry, AppError> {
    Entry::new("com.screenstory.desktop", "gemini-api-key")
        .map_err(|_| AppError::new("CREDENTIAL_STORE_UNAVAILABLE", "The Linux Secret Service credential store is unavailable or locked. Unlock your desktop keyring and try again. Screenstory will not save the API key in a plain-text file."))
}

#[tauri::command]
pub fn get_gemini_key_status() -> Result<GeminiKeyStatus, AppError> {
    let entry = keyring_entry()?;
    match entry.get_password() {
        Ok(_) => Ok(GeminiKeyStatus { configured: true }),
        Err(error) if error.to_string().contains("NoEntry") => Ok(GeminiKeyStatus { configured: false }),
        Err(_) => Err(AppError::new("CREDENTIAL_STORE_UNAVAILABLE", "Screenstory could not read the Linux Secret Service credential store. Unlock your desktop keyring and try again.")),
    }
}

#[tauri::command]
pub fn save_gemini_api_key(api_key: String) -> Result<GeminiKeyStatus, AppError> {
    let key = api_key.trim();
    if key.len() < 20 || key.len() > 512 || key.chars().any(char::is_control) {
        return Err(AppError::new("INVALID_API_KEY", "Enter a valid Gemini API key. The key must be between 20 and 512 characters."));
    }
    let entry = keyring_entry()?;
    entry.set_password(key)
        .map_err(|_| AppError::new("CREDENTIAL_STORE_WRITE_FAILED", "The Gemini API key could not be saved in the Linux credential store."))?;
    Ok(GeminiKeyStatus { configured: true })
}

#[tauri::command]
pub fn delete_gemini_api_key() -> Result<GeminiKeyStatus, AppError> {
    let entry = keyring_entry()?;
    match entry.delete_credential() {
        Ok(()) => Ok(GeminiKeyStatus { configured: false }),
        Err(error) if error.to_string().contains("NoEntry") => Ok(GeminiKeyStatus { configured: false }),
        Err(_) => Err(AppError::new("CREDENTIAL_STORE_WRITE_FAILED", "The Gemini API key could not be removed from the Linux credential store.")),
    }
}

fn ensure_analysis_table(project_dir: &Path) -> Result<(), AppError> {
    let connection = db_connection(project_dir)?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS analysis_runs (
            id TEXT PRIMARY KEY,
            recording_id TEXT NOT NULL,
            source_file_name TEXT NOT NULL,
            status TEXT NOT NULL,
            created_at TEXT NOT NULL,
            provider TEXT NOT NULL,
            model TEXT NOT NULL,
            completed_frames INTEGER NOT NULL DEFAULT 0,
            total_frames INTEGER NOT NULL DEFAULT 0,
            total_bytes INTEGER NOT NULL DEFAULT 0,
            duration_seconds REAL,
            workflow_description TEXT NOT NULL DEFAULT '',
            language TEXT NOT NULL DEFAULT 'auto',
            manifest_path TEXT,
            draft_path TEXT,
            consent_at TEXT,
            error_code TEXT
        );"
    ).map_err(|_| AppError::new("ANALYSIS_DATABASE_ERROR", "The analysis history table could not be initialized."))?;
    Ok(())
}

fn resolve_recording(project_dir: &Path, recording_id: &str) -> Result<(String, PathBuf), AppError> {
    if recording_id.is_empty() || recording_id.len() > 128 || recording_id.chars().any(char::is_control) {
        return Err(AppError::new("INVALID_RECORDING_ID", "The selected recording identifier is invalid."));
    }
    let connection = db_connection(project_dir)?;
    let file_name: String = connection.query_row(
        "SELECT file_name FROM recordings WHERE id = ?1",
        params![recording_id],
        |row| row.get(0),
    ).map_err(|_| AppError::new("RECORDING_NOT_FOUND", "This recording is not in the active project library."))?;

    let root = project_dir.join("recordings").canonicalize()
        .map_err(|_| AppError::new("RECORDING_NOT_FOUND", "The project's recordings folder is unavailable."))?;
    let path = root.join(&file_name).canonicalize()
        .map_err(|_| AppError::new("RECORDING_NOT_FOUND", "The original recording file could not be found."))?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err(AppError::new("RECORDING_PATH_INVALID", "The recording path is outside the active project's managed recordings folder."));
    }
    Ok((file_name, path))
}

fn probe_video(path: &Path) -> Result<(f64, u32, u32), AppError> {
    let output = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height", "-show_entries", "format=duration", "-of", "json"])
        .arg(path)
        .output()
        .map_err(|_| AppError::new("FFPROBE_MISSING", "FFprobe is required for video analysis. It is normally included with FFmpeg."))?;
    if !output.status.success() {
        return Err(AppError::new("VIDEO_PROBE_FAILED", "FFprobe could not read this recording. The file may be incomplete or unsupported."));
    }
    let probe: ProbeOutput = serde_json::from_slice(&output.stdout)
        .map_err(|_| AppError::new("VIDEO_METADATA_INVALID", "FFprobe returned invalid video metadata."))?;
    let stream = probe.streams.first().ok_or_else(|| AppError::new("VIDEO_STREAM_MISSING", "No video stream was found in this recording."))?;
    let width = stream.width.filter(|v| *v > 0).ok_or_else(|| AppError::new("VIDEO_DIMENSIONS_INVALID", "The video width is invalid."))?;
    let height = stream.height.filter(|v| *v > 0).ok_or_else(|| AppError::new("VIDEO_DIMENSIONS_INVALID", "The video height is invalid."))?;
    let duration = probe.format.duration.as_deref().and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v > 0.0)
        .ok_or_else(|| AppError::new("VIDEO_DURATION_INVALID", "The recording duration could not be determined."))?;
    if duration > 24.0 * 60.0 * 60.0 {
        return Err(AppError::new("VIDEO_DURATION_UNSUPPORTED", "Recordings longer than 24 hours cannot be analyzed in this version."));
    }
    Ok((duration, width, height))
}

fn scaled_dimension(width: u32, height: u32) -> (u32, u32) {
    let largest = width.max(height);
    if largest <= MAX_EDGE {
        return (width.max(2), height.max(2));
    }
    let scale = MAX_EDGE as f64 / largest as f64;
    let w = (((width as f64 * scale).floor() as u32) / 2 * 2).max(2);
    let h = (((height as f64 * scale).floor() as u32) / 2 * 2).max(2);
    (w, h)
}

fn sample_timestamps(duration_seconds: f64) -> Vec<u64> {
    let count = ((duration_seconds * 2.0).ceil() as usize).clamp(1, MAX_FRAMES);
    (0..count)
        .map(|index| ((index as f64 * duration_seconds / count as f64) * 1000.0).floor() as u64)
        .collect()
}

#[allow(clippy::too_many_arguments)] // Internal persistence helper updates one complete analysis-run snapshot.
fn set_run_status(
    project_dir: &Path,
    run_id: &str,
    status: &str,
    completed: usize,
    total: usize,
    total_bytes: u64,
    duration: Option<f64>,
    manifest_path: Option<&Path>,
    draft_path: Option<&Path>,
    error_code: Option<&str>,
) {
    if let Ok(connection) = db_connection(project_dir) {
        let _ = connection.execute(
            "UPDATE analysis_runs SET status=?1, completed_frames=?2, total_frames=?3, total_bytes=?4,
             duration_seconds=COALESCE(?5, duration_seconds),
             manifest_path=COALESCE(?6, manifest_path), draft_path=COALESCE(?7, draft_path),
             error_code=?8 WHERE id=?9",
            params![status, completed as i64, total as i64, total_bytes as i64, duration,
                    manifest_path.map(|p| p.to_string_lossy().to_string()),
                    draft_path.map(|p| p.to_string_lossy().to_string()), error_code, run_id],
        );
    }
}

#[allow(clippy::too_many_arguments)] // Worker receives an immutable run context plus its cancellation token.
fn extract_worker(
    project_dir: PathBuf,
    run_id: String,
    recording_id: String,
    file_name: String,
    source: PathBuf,
    workflow_description: String,
    language: String,
    cancelled: Arc<AtomicBool>,
) {
    let result = (|| -> Result<(), AppError> {
        if cancelled.load(Ordering::SeqCst) {
            return Err(AppError::new("ANALYSIS_CANCELLED", "Analysis was cancelled."));
        }
        let (duration, width, height) = probe_video(&source)?;
        let timestamps = sample_timestamps(duration);
        let run_dir = project_dir.join("analysis").join(&run_id);
        let frames_dir = run_dir.join("frames");
        fs::create_dir_all(&frames_dir)
            .map_err(|_| AppError::new("ANALYSIS_STORAGE_FAILED", "The analysis workspace could not be created."))?;

        let (target_width, target_height) = scaled_dimension(width, height);
        let mut frames = Vec::with_capacity(timestamps.len());
        let mut total_bytes = 0_u64;
        for (index, timestamp_ms) in timestamps.iter().enumerate() {
            if cancelled.load(Ordering::SeqCst) {
                return Err(AppError::new("ANALYSIS_CANCELLED", "Analysis was cancelled."));
            }
            let id = format!("frame-{index:03}");
            let relative_path = format!("frames/{id}.jpg");
            let output = frames_dir.join(format!("{id}.jpg"));
            let seconds = format!("{:.3}", *timestamp_ms as f64 / 1000.0);
            let filter = format!("scale={target_width}:{target_height}");
            let command = Command::new("ffmpeg")
                .args(["-hide_banner", "-loglevel", "error", "-ss", &seconds, "-i"])
                .arg(&source)
                .args(["-frames:v", "1", "-vf", &filter, "-q:v", "8", "-y"])
                .arg(&output)
                .output()
                .map_err(|_| AppError::new("FFMPEG_MISSING", "FFmpeg is required to extract analysis frames."))?;
            if !command.status.success() {
                return Err(AppError::new("FRAME_EXTRACTION_FAILED", "FFmpeg could not extract a representative frame from the original recording."));
            }
            let bytes = fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
            if bytes == 0 {
                return Err(AppError::new("FRAME_EXTRACTION_FAILED", "A sampled frame was empty or unreadable."));
            }
            total_bytes = total_bytes.saturating_add(bytes);
            frames.push(FrameReference {
                id,
                timestamp_ms: *timestamp_ms,
                relative_path,
                width: target_width,
                height: target_height,
                mime_type: "image/jpeg".to_string(),
                byte_size: bytes,
            });
            set_run_status(&project_dir, &run_id, "extracting_frames", index + 1, timestamps.len(), total_bytes, Some(duration), None, None, None);
        }

        if cancelled.load(Ordering::SeqCst) {
            return Err(AppError::new("ANALYSIS_CANCELLED", "Analysis was cancelled."));
        }
        let manifest = FrameManifest {
            schema_version: 1,
            run_id: run_id.clone(),
            recording_id,
            source_file_name: file_name,
            source_duration_ms: (duration * 1000.0).round() as u64,
            source_width: width,
            source_height: height,
            sampling_strategy: "periodic_uniform_max_2fps_60_frames".to_string(),
            sample_count: frames.len(),
            max_edge: MAX_EDGE,
            total_bytes,
            workflow_description,
            language,
            frames,
        };
        let manifest_path = run_dir.join("manifest.json");
        let temporary = run_dir.join("manifest.json.tmp");
        let serialized = serde_json::to_vec_pretty(&manifest)
            .map_err(|_| AppError::new("ANALYSIS_MANIFEST_FAILED", "The frame manifest could not be serialized."))?;
        {
            let mut file = fs::File::create(&temporary)
                .map_err(|_| AppError::new("ANALYSIS_MANIFEST_FAILED", "The frame manifest could not be written."))?;
            file.write_all(&serialized)
                .and_then(|_| file.sync_all())
                .map_err(|_| AppError::new("ANALYSIS_MANIFEST_FAILED", "The frame manifest could not be saved."))?;
        }
        fs::rename(&temporary, &manifest_path)
            .map_err(|_| AppError::new("ANALYSIS_MANIFEST_FAILED", "The frame manifest could not be finalized."))?;
        let count = manifest.frames.len();
        set_run_status(&project_dir, &run_id, "awaiting_cloud_consent", count, count, total_bytes, Some(duration), Some(&manifest_path), None, None);
        Ok(())
    })();

    if let Err(error) = result {
        let status = if error.code == "ANALYSIS_CANCELLED" { "cancelled" } else { "failed" };
        set_run_status(&project_dir, &run_id, status, 0, 0, 0, None, None, None, Some(error.code));
        let _ = fs::remove_dir_all(project_dir.join("analysis").join(&run_id).join("frames"));
    }
}

#[tauri::command]
pub fn start_analysis(
    recording_id: String,
    workflow_description: String,
    language: String,
    state: State<'_, AppState>,
) -> Result<AnalysisStarted, AppError> {
    if workflow_description.chars().count() > 2000 || workflow_description.chars().any(char::is_control) {
        return Err(AppError::new("INVALID_WORKFLOW_DESCRIPTION", "The optional workflow description must be no longer than 2,000 characters."));
    }
    if !["auto", "en", "ar"].contains(&language.as_str()) {
        return Err(AppError::new("INVALID_ANALYSIS_LANGUAGE", "Choose automatic, English, or Arabic output language."));
    }
    let recording = state.recording.lock().map_err(|_| AppError::new("STATE_LOCK_ERROR", "Recording state is temporarily unavailable."))?;
    if recording.is_some() {
        return Err(AppError::new("RECORDING_IN_PROGRESS", "Stop the recording before starting analysis."));
    }
    drop(recording);

    let project_dir = state.active_project.lock()
        .map_err(|_| AppError::new("STATE_LOCK_ERROR", "Project state is temporarily unavailable."))?
        .as_ref().cloned()
        .ok_or_else(|| AppError::new("NO_ACTIVE_PROJECT", "Open a project before analyzing a recording."))?;

    ensure_analysis_table(&project_dir)?;
    let (file_name, source) = resolve_recording(&project_dir, &recording_id)?;
    if !Command::new("ffprobe").arg("-version").output().map(|o| o.status.success()).unwrap_or(false)
        || !Command::new("ffmpeg").arg("-version").output().map(|o| o.status.success()).unwrap_or(false) {
        return Err(AppError::new("VIDEO_TOOLS_MISSING", "FFmpeg and FFprobe are required for local video analysis. Install FFmpeg and retry."));
    }

    let connection = db_connection(&project_dir)?;
    let active_runs: i64 = connection.query_row(
        "SELECT COUNT(*) FROM analysis_runs WHERE status IN ('extracting_frames','awaiting_cloud_consent','sending_to_provider','validating_output')",
        [], |row| row.get(0)
    ).unwrap_or(0);
    if active_runs > 0 {
        return Err(AppError::new("ANALYSIS_ALREADY_ACTIVE", "Finish or cancel the current analysis run before starting another."));
    }

    let run_id = format!("a{}-{}", now_ms(), std::process::id());
    connection.execute(
        "INSERT INTO analysis_runs (id, recording_id, source_file_name, status, created_at, provider, model, workflow_description, language)
         VALUES (?1, ?2, ?3, 'extracting_frames', ?4, 'gemini', ?5, ?6, ?7)",
        params![run_id, recording_id, file_name, now_ms().to_string(), GEMINI_MODEL, workflow_description, language],
    ).map_err(|_| AppError::new("ANALYSIS_DATABASE_ERROR", "The analysis run could not be registered in the project."))?;

    let cancellation = Arc::new(AtomicBool::new(false));
    state.analysis_cancellations.lock()
        .map_err(|_| AppError::new("STATE_LOCK_ERROR", "Analysis state is temporarily unavailable."))?
        .insert(run_id.clone(), Arc::clone(&cancellation));

    let worker_project = project_dir.clone();
    let worker_run = run_id.clone();
    let worker_recording = recording_id.clone();
    let worker_file_name = file_name.clone();
    thread::spawn(move || {
        extract_worker(worker_project, worker_run, worker_recording, worker_file_name, source, workflow_description, language, cancellation);
    });

    Ok(AnalysisStarted { run_id, recording_id, file_name })
}

#[tauri::command]
pub fn get_analysis_status(run_id: String, state: State<'_, AppState>) -> Result<AnalysisStatus, AppError> {
    if !analysis_run_id_valid(&run_id) {
        return Err(AppError::new("INVALID_ANALYSIS_RUN_ID", "The analysis run identifier is invalid."));
    }
    let project_dir = state.active_project.lock()
        .map_err(|_| AppError::new("STATE_LOCK_ERROR", "Project state is temporarily unavailable."))?
        .as_ref().cloned()
        .ok_or_else(|| AppError::new("NO_ACTIVE_PROJECT", "Open a project to view its analysis."))?;
    ensure_analysis_table(&project_dir)?;
    let connection = db_connection(&project_dir)?;
    connection.query_row(
        "SELECT id, recording_id, source_file_name, status, completed_frames, total_frames, total_bytes,
                duration_seconds, provider, model, error_code, workflow_description, language
         FROM analysis_runs WHERE id=?1",
        params![run_id],
        |row| {
            Ok(AnalysisStatus {
                run_id: row.get(0)?,
                recording_id: row.get(1)?,
                source_file_name: row.get(2)?,
                status: row.get(3)?,
                completed_frames: row.get::<_, i64>(4)?.max(0) as usize,
                total_frames: row.get::<_, i64>(5)?.max(0) as usize,
                total_bytes: row.get::<_, i64>(6)?.max(0) as u64,
                duration_seconds: row.get(7)?,
                provider: row.get(8)?,
                model: row.get(9)?,
                error_code: row.get(10)?,
                workflow_description: row.get(11)?,
                language: row.get(12)?,
            })
        }
    ).map_err(|_| AppError::new("ANALYSIS_RUN_NOT_FOUND", "This analysis run could not be found in the active project."))
}

#[tauri::command]
pub fn cancel_analysis(run_id: String, state: State<'_, AppState>) -> Result<(), AppError> {
    if !analysis_run_id_valid(&run_id) {
        return Err(AppError::new("INVALID_ANALYSIS_RUN_ID", "The analysis run identifier is invalid."));
    }
    let project_dir = state.active_project.lock()
        .map_err(|_| AppError::new("STATE_LOCK_ERROR", "Project state is temporarily unavailable."))?
        .as_ref().cloned()
        .ok_or_else(|| AppError::new("NO_ACTIVE_PROJECT", "Open a project before cancelling analysis."))?;
    let connection = db_connection(&project_dir)?;
    let status: String = connection.query_row("SELECT status FROM analysis_runs WHERE id=?1", params![run_id], |row| row.get(0))
        .map_err(|_| AppError::new("ANALYSIS_RUN_NOT_FOUND", "This analysis run could not be found."))?;
    if ["completed", "failed", "cancelled"].contains(&status.as_str()) {
        return Ok(());
    }
    if let Some(token) = state.analysis_cancellations.lock()
        .map_err(|_| AppError::new("STATE_LOCK_ERROR", "Analysis state is temporarily unavailable."))?
        .get(&run_id) {
        token.store(true, Ordering::SeqCst);
    }
    set_run_status(&project_dir, &run_id, "cancelled", 0, 0, 0, None, None, None, Some("ANALYSIS_CANCELLED"));
    Ok(())
}

fn response_text(value: &Value) -> Option<String> {
    if let Some(text) = value.get("output_text").and_then(Value::as_str) {
        if !text.trim().is_empty() {
            return Some(text.to_string());
        }
    }

    // Gemini Interactions REST responses expose model text in
    // steps[].content[], not an "output" property.
    let steps = value.get("steps").and_then(Value::as_array)?;
    let mut found = Vec::new();
    for step in steps {
        if step.get("type").and_then(Value::as_str) != Some("model_output") {
            continue;
        }
        let Some(content) = step.get("content").and_then(Value::as_array) else {
            continue;
        };
        for item in content {
            if item.get("type").and_then(Value::as_str) == Some("text") {
                if let Some(text) = item.get("text").and_then(Value::as_str) {
                    if !text.trim().is_empty() {
                        found.push(text.to_string());
                    }
                }
            }
        }
    }
    if found.is_empty() { None } else { Some(found.join("\n")) }
}

fn validate_draft(draft: &AnalysisDraft, manifest: &FrameManifest) -> Result<(), AppError> {
    if draft.schema_version != 1 {
        return Err(AppError::new("ANALYSIS_SCHEMA_UNSUPPORTED", "Gemini returned an unsupported tutorial schema version."));
    }
    if draft.summary.trim().is_empty() || draft.summary.chars().count() > 4000 || draft.steps.is_empty() || draft.steps.len() > 60 || draft.zoom_suggestions.len() > 100 {
        return Err(AppError::new("ANALYSIS_OUTPUT_INVALID", "Gemini returned an empty or oversized tutorial draft."));
    }
    let duration = manifest.source_duration_ms;
    let known_frames: HashSet<&str> = manifest.frames.iter().map(|f| f.id.as_str()).collect();
    let mut previous_end = 0_u64;
    let mut step_ids = HashSet::new();
    for (index, step) in draft.steps.iter().enumerate() {
        if step.id.trim().is_empty() || !step_ids.insert(step.id.as_str()) || step.title.trim().is_empty() || step.title.chars().count() > 160
            || step.description.trim().is_empty() || step.description.chars().count() > 1200
            || step.start_ms >= step.end_ms || step.end_ms > duration
            || (index > 0 && step.start_ms < previous_end)
            || !step.confidence.is_finite() || !(0.0..=1.0).contains(&step.confidence)
            || !["observed", "inference"].contains(&step.evidence_kind.as_str())
            || step.evidence_frame_ids.is_empty()
            || step.evidence_frame_ids.iter().any(|id| !known_frames.contains(id.as_str())) {
            return Err(AppError::new("ANALYSIS_OUTPUT_INVALID", "Gemini returned a tutorial step with invalid timing, evidence, confidence, or text."));
        }
        previous_end = step.end_ms;
    }
    let mut zoom_ids = HashSet::new();
    for zoom in &draft.zoom_suggestions {
        if zoom.id.trim().is_empty() || !zoom_ids.insert(zoom.id.as_str())
            || zoom.start_ms >= zoom.end_ms || zoom.end_ms > duration
            || !zoom.x.is_finite() || !zoom.y.is_finite() || !zoom.width.is_finite() || !zoom.height.is_finite()
            || zoom.x < 0.0 || zoom.y < 0.0 || zoom.width <= 0.0 || zoom.height <= 0.0
            || zoom.x + zoom.width > 1.0 || zoom.y + zoom.height > 1.0
            || !zoom.target_scale.is_finite() || !(1.0..=3.0).contains(&zoom.target_scale)
            || zoom.rationale.trim().is_empty() || zoom.evidence_frame_ids.is_empty()
            || zoom.evidence_frame_ids.iter().any(|id| !known_frames.contains(id.as_str())) {
            return Err(AppError::new("ANALYSIS_OUTPUT_INVALID", "Gemini returned a zoom suggestion with invalid timing, evidence, or geometry."));
        }
    }
    Ok(())
}

fn response_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "schema_version": {"type":"integer"},
            "summary": {"type":"string"},
            "steps": {
                "type":"array",
                "items": {
                    "type":"object",
                    "properties": {
                        "id":{"type":"string"},
                        "title":{"type":"string"},
                        "description":{"type":"string"},
                        "start_ms":{"type":"integer"},
                        "end_ms":{"type":"integer"},
                        "evidence_frame_ids":{"type":"array","items":{"type":"string"}},
                        "confidence":{"type":"number"},
                        "evidence_kind":{"type":"string","enum":["observed","inference"]}
                    },
                    "required":["id","title","description","start_ms","end_ms","evidence_frame_ids","confidence","evidence_kind"]
                }
            },
            "zoom_suggestions": {
                "type":"array",
                "items": {
                    "type":"object",
                    "properties": {
                        "id":{"type":"string"},
                        "start_ms":{"type":"integer"},
                        "end_ms":{"type":"integer"},
                        "x":{"type":"number"},
                        "y":{"type":"number"},
                        "width":{"type":"number"},
                        "height":{"type":"number"},
                        "target_scale":{"type":"number"},
                        "rationale":{"type":"string"},
                        "evidence_frame_ids":{"type":"array","items":{"type":"string"}}
                    },
                    "required":["id","start_ms","end_ms","x","y","width","height","target_scale","rationale","evidence_frame_ids"]
                }
            }
        },
        "required":["schema_version","summary","steps","zoom_suggestions"]
    })
}

fn gemini_request_body(manifest: &FrameManifest, run_dir: &Path) -> Result<Value, AppError> {
    let mut content = vec![json!({
        "type":"text",
        "text": format!(
            "Analyze these timestamped screenshots from one desktop workflow recording. Produce a concise, evidence-grounded tutorial. Do not invent actions not visible in the frames. Distinguish observed evidence from inference. Every step and zoom suggestion must cite one or more supplied frame IDs. Use milliseconds from the source video. Language: {}. Optional workflow context: {}. Source duration: {} ms. Source dimensions: {}x{}. Available frame IDs and timestamps: {}",
            manifest.language,
            if manifest.workflow_description.trim().is_empty() { "(none)" } else { manifest.workflow_description.as_str() },
            manifest.source_duration_ms,
            manifest.source_width,
            manifest.source_height,
            manifest.frames.iter().map(|f| format!("{}@{}ms", f.id, f.timestamp_ms)).collect::<Vec<_>>().join(", ")
        )
    })];
    let canonical_run = run_dir.canonicalize()
        .map_err(|_| AppError::new("ANALYSIS_PATH_INVALID", "The analysis run folder is unavailable."))?;
    for frame in &manifest.frames {
        let path = run_dir.join(&frame.relative_path).canonicalize()
            .map_err(|_| AppError::new("ANALYSIS_FRAME_MISSING", "A sampled frame is no longer available. Prepare the analysis again."))?;
        if !path.starts_with(&canonical_run) || !path.is_file() {
            return Err(AppError::new("ANALYSIS_PATH_INVALID", "A sampled frame is outside the managed analysis folder."));
        }
        let bytes = fs::read(path).map_err(|_| AppError::new("ANALYSIS_FRAME_MISSING", "A sampled frame is no longer available. Prepare the analysis again."))?;
        content.push(json!({
            "type":"image",
            "data": BASE64.encode(bytes),
            "mime_type":"image/jpeg"
        }));
    }
    let body = json!({
        "model": GEMINI_MODEL,
        "input": content,
        "system_instruction": "You create clear software workflow tutorials from visual evidence. Never claim a click, typed value, or hidden action unless a frame proves it. Do not include markdown fences.",
        "response_format": {
            "type": "text",
            "mime_type": "application/json",
            "schema": response_schema()
        },
        "store": false
    });
    let size = serde_json::to_vec(&body).map(|v| v.len()).unwrap_or(usize::MAX);
    if size > MAX_INLINE_REQUEST_BYTES {
        return Err(AppError::new("ANALYSIS_PAYLOAD_TOO_LARGE", "The sampled frames exceed Gemini's safe inline request size. Use a shorter recording or reduce the recording's visual complexity."));
    }
    Ok(body)
}

fn analyze_worker(project_dir: PathBuf, run_id: String, cancellation: Arc<AtomicBool>) {
    let result = (|| -> Result<(), AppError> {
        if cancellation.load(Ordering::SeqCst) {
            return Err(AppError::new("ANALYSIS_CANCELLED", "Analysis was cancelled."));
        }
        let entry = keyring_entry()?;
        let api_key = entry.get_password()
            .map_err(|_| AppError::new("GEMINI_API_KEY_MISSING", "Add your Gemini API key in Screenstory settings before sending sampled frames."))?;
        let connection = db_connection(&project_dir)?;
        let (manifest_path, workflow, language, recording_id): (String, String, String, String) = connection.query_row(
            "SELECT manifest_path, workflow_description, language, recording_id FROM analysis_runs WHERE id=?1 AND status='sending_to_provider'",
            params![run_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        ).map_err(|_| AppError::new("ANALYSIS_RUN_NOT_READY", "This analysis run is not ready to send to Gemini."))?;
        let manifest_path = PathBuf::from(manifest_path);
        let canonical_run = project_dir.join("analysis").join(&run_id).canonicalize()
            .map_err(|_| AppError::new("ANALYSIS_PATH_INVALID", "The analysis run folder is unavailable."))?;
        let canonical_manifest = manifest_path.canonicalize()
            .map_err(|_| AppError::new("ANALYSIS_MANIFEST_MISSING", "The frame manifest could not be read."))?;
        if !canonical_manifest.starts_with(&canonical_run) || !analysis_run_id_valid(&run_id) {
            return Err(AppError::new("ANALYSIS_PATH_INVALID", "The analysis manifest path is outside its managed run folder."));
        }
        let mut manifest: FrameManifest = serde_json::from_slice(&fs::read(&canonical_manifest).map_err(|_| AppError::new("ANALYSIS_MANIFEST_MISSING", "The frame manifest could not be read."))?)
            .map_err(|_| AppError::new("ANALYSIS_MANIFEST_INVALID", "The frame manifest is invalid."))?;
        manifest.workflow_description = workflow;
        manifest.language = language;
        if manifest.recording_id != recording_id || manifest.run_id != run_id || manifest.frames.is_empty() || manifest.frames.len() > MAX_FRAMES {
            return Err(AppError::new("ANALYSIS_MANIFEST_MISMATCH", "The analysis manifest does not match this run or contains an invalid frame count."));
        }
        let run_dir = project_dir.join("analysis").join(&run_id);
        let body = gemini_request_body(&manifest, &run_dir)?;
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .connect_timeout(Duration::from_secs(20))
            .build()
            .map_err(|_| AppError::new("GEMINI_CLIENT_ERROR", "The Gemini network client could not be initialized."))?;

        let mut response = None;
        let mut last_error_code = "GEMINI_NETWORK_ERROR";
        for attempt in 0..3 {
            if cancellation.load(Ordering::SeqCst) {
                return Err(AppError::new("ANALYSIS_CANCELLED", "Analysis was cancelled."));
            }
            match client.post(GEMINI_ENDPOINT)
                .header("x-goog-api-key", &api_key)
                .header("api-revision", "2026-05-20")
                .header("content-type", "application/json")
                .json(&body)
                .send() {
                Ok(result) if result.status().is_success() => {
                    response = Some(result);
                    break;
                }
                Ok(result) => {
                    let status = result.status().as_u16();
                    if status == 401 || status == 403 {
                        return Err(AppError::new("GEMINI_AUTH_FAILED", "Google rejected the Gemini API key. Check the key in Screenstory settings."));
                    } else if status == 429 {
                        last_error_code = "GEMINI_RATE_LIMITED";
                        if attempt < 2 { thread::sleep(Duration::from_millis(500 * (attempt + 1) as u64)); continue; }
                    } else if status >= 500 {
                        last_error_code = "GEMINI_SERVER_ERROR";
                        if attempt < 2 { thread::sleep(Duration::from_millis(500 * (attempt + 1) as u64)); continue; }
                    } else {
                        return Err(AppError::new("GEMINI_REQUEST_REJECTED", format!("Gemini rejected the analysis request (HTTP {status}). Check the selected model and request settings.")));
                    }
                }
                Err(_) => {
                    last_error_code = "GEMINI_NETWORK_ERROR";
                    if attempt < 2 { thread::sleep(Duration::from_millis(500 * (attempt + 1) as u64)); continue; }
                }
            }
        }
        let response = response.ok_or_else(|| AppError::new(last_error_code, "Gemini could not complete the analysis after bounded retries."))?;
        if cancellation.load(Ordering::SeqCst) {
            return Err(AppError::new("ANALYSIS_CANCELLED", "Analysis was cancelled after the provider request."));
        }
        let payload: Value = response.json()
            .map_err(|_| AppError::new("GEMINI_RESPONSE_INVALID", "Gemini returned a response that could not be parsed."))?;
        let text = response_text(&payload).ok_or_else(|| AppError::new("GEMINI_RESPONSE_EMPTY", "Gemini returned no structured tutorial content."))?;
        let mut draft: AnalysisDraft = serde_json::from_str(&text)
            .map_err(|_| AppError::new("GEMINI_RESPONSE_INVALID", "Gemini's structured response did not match the tutorial schema."))?;
        validate_draft(&draft, &manifest)?;
        draft.status = "needs_review".to_string();
        draft.language = manifest.language.clone();
        let draft_path = run_dir.join("draft.json");
        let temporary = run_dir.join("draft.json.tmp");
        let bytes = serde_json::to_vec_pretty(&draft)
            .map_err(|_| AppError::new("ANALYSIS_DRAFT_SAVE_FAILED", "The tutorial draft could not be serialized."))?;
        {
            let mut file = fs::File::create(&temporary)
                .map_err(|_| AppError::new("ANALYSIS_DRAFT_SAVE_FAILED", "The tutorial draft could not be written."))?;
            file.write_all(&bytes).and_then(|_| file.sync_all())
                .map_err(|_| AppError::new("ANALYSIS_DRAFT_SAVE_FAILED", "The tutorial draft could not be saved."))?;
        }
        fs::rename(&temporary, &draft_path)
            .map_err(|_| AppError::new("ANALYSIS_DRAFT_SAVE_FAILED", "The tutorial draft could not be finalized."))?;
        set_run_status(&project_dir, &run_id, "completed", manifest.frames.len(), manifest.frames.len(), manifest.total_bytes, Some(manifest.source_duration_ms as f64 / 1000.0), None, Some(&draft_path), None);
        Ok(())
    })();

    if let Err(error) = result {
        let status = if error.code == "ANALYSIS_CANCELLED" { "cancelled" } else { "failed" };
        set_run_status(&project_dir, &run_id, status, 0, 0, 0, None, None, None, Some(error.code));
    }
}

#[tauri::command]
pub fn send_analysis_to_gemini(
    run_id: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    if !analysis_run_id_valid(&run_id) {
        return Err(AppError::new("INVALID_ANALYSIS_RUN_ID", "The analysis run identifier is invalid."));
    }
    let project_dir = state.active_project.lock()
        .map_err(|_| AppError::new("STATE_LOCK_ERROR", "Project state is temporarily unavailable."))?
        .as_ref().cloned()
        .ok_or_else(|| AppError::new("NO_ACTIVE_PROJECT", "Open a project before sending analysis."))?;
    ensure_analysis_table(&project_dir)?;
    let entry = keyring_entry()?;
    entry.get_password()
        .map_err(|_| AppError::new("GEMINI_API_KEY_MISSING", "Add a Gemini API key in Screenstory settings before sending sampled frames."))?;
    let connection = db_connection(&project_dir)?;
    let (status, manifest_path): (String, Option<String>) = connection.query_row(
        "SELECT status, manifest_path FROM analysis_runs WHERE id=?1",
        params![run_id],
        |row| Ok((row.get(0)?, row.get(1)?))
    ).map_err(|_| AppError::new("ANALYSIS_RUN_NOT_FOUND", "This analysis run could not be found."))?;
    if status != "awaiting_cloud_consent" || manifest_path.is_none() {
        return Err(AppError::new("ANALYSIS_RUN_NOT_READY", "Frame preparation must finish before cloud analysis can start."));
    }
    let cancellation = state.analysis_cancellations.lock()
        .map_err(|_| AppError::new("STATE_LOCK_ERROR", "Analysis state is temporarily unavailable."))?
        .get(&run_id).cloned().unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
    connection.execute(
        "UPDATE analysis_runs SET status='sending_to_provider', consent_at=?1 WHERE id=?2",
        params![now_ms().to_string(), run_id],
    ).map_err(|_| AppError::new("ANALYSIS_DATABASE_ERROR", "The cloud-consent decision could not be recorded."))?;
    let worker_project = project_dir.clone();
    let worker_run = run_id.clone();
    let registry = Arc::clone(&state.analysis_cancellations);
    thread::spawn(move || {
        analyze_worker(worker_project, worker_run.clone(), cancellation);
        if let Ok(mut active) = registry.lock() {
            active.remove(&worker_run);
        }
    });
    Ok(())
}

#[tauri::command]
pub fn get_analysis_draft(run_id: String, state: State<'_, AppState>) -> Result<AnalysisDraft, AppError> {
    if !analysis_run_id_valid(&run_id) {
        return Err(AppError::new("INVALID_ANALYSIS_RUN_ID", "The analysis run identifier is invalid."));
    }
    let project_dir = state.active_project.lock()
        .map_err(|_| AppError::new("STATE_LOCK_ERROR", "Project state is temporarily unavailable."))?
        .as_ref().cloned()
        .ok_or_else(|| AppError::new("NO_ACTIVE_PROJECT", "Open a project to view its analysis draft."))?;
    let run_dir = project_dir.join("analysis").join(&run_id);
    let canonical_run = run_dir.canonicalize()
        .map_err(|_| AppError::new("ANALYSIS_DRAFT_NOT_FOUND", "This analysis draft could not be found."))?;
    let canonical_root = project_dir.join("analysis").canonicalize()
        .map_err(|_| AppError::new("ANALYSIS_DRAFT_NOT_FOUND", "The project analysis folder is unavailable."))?;
    if !canonical_run.starts_with(canonical_root) {
        return Err(AppError::new("ANALYSIS_PATH_INVALID", "The requested analysis draft is outside this project."));
    }
    serde_json::from_slice(&fs::read(canonical_run.join("draft.json"))
        .map_err(|_| AppError::new("ANALYSIS_DRAFT_NOT_FOUND", "The draft has not completed yet."))?)
        .map_err(|_| AppError::new("ANALYSIS_DRAFT_INVALID", "The saved analysis draft is invalid."))
}

pub(super) fn analysis_run_id_valid(value: &str) -> bool {
    value.starts_with('a') && value.len() > 2 && value.chars().all(|c| c == 'a' || c == '-' || c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampling_is_deterministic_and_bounded() {
        assert_eq!(sample_timestamps(2.0), vec![0, 500, 1000, 1500]);
        assert_eq!(sample_timestamps(30.0).len(), 60);
        assert_eq!(sample_timestamps(900.0).len(), 60);
        assert_eq!(sample_timestamps(1.0), vec![0, 500]);
    }

    #[test]
    fn scaled_dimensions_never_exceed_the_edge_limit() {
        assert_eq!(scaled_dimension(1920, 1080), (1024, 576));
        assert_eq!(scaled_dimension(1080, 1920), (576, 1024));
        assert_eq!(scaled_dimension(800, 600), (800, 600));
    }

    #[test]
    fn extracts_model_output_from_gemini_interactions_steps() {
        let payload = json!({
            "id": "v1_example",
            "status": "completed",
            "steps": [
                {"type":"thought","signature":"ignored"},
                {"type":"model_output","content":[
                    {"type":"text","text":"{\\"schema_version\\":1,"},
                    {"type":"text","text":"\\"summary\\":\\"Captured workflow\\"}"}
                ]}
            ]
        });
        assert_eq!(
            response_text(&payload).as_deref(),
            Some("{\\"schema_version\\":1,\n\\"summary\\":\\"Captured workflow\\"}")
        );
    }

    #[test]
    fn response_schema_requires_evidence_for_each_step_and_zoom() {
        let schema = response_schema();
        let required = schema["properties"]["steps"]["items"]["required"].as_array().unwrap();
        assert!(required.iter().any(|value| value == "evidence_frame_ids"));
        let zoom_required = schema["properties"]["zoom_suggestions"]["items"]["required"].as_array().unwrap();
        assert!(zoom_required.iter().any(|value| value == "evidence_frame_ids"));
    }
}
