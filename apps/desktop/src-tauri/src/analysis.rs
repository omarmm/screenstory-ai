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
    io::{Read, Write},
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

const GEMINI_ENDPOINT: &str = "https://generativelanguage.googleapis.com/v1beta/interactions";
const GEMINI_MODEL_FLASH: &str = "gemini-3.6-flash";
const GEMINI_MODEL_FLASH_LITE: &str = "gemini-3.5-flash-lite";
const GEMINI_MODEL_PRO: &str = "gemini-3.1-pro-preview";
const LEGACY_GEMINI_MODEL_HYDROGEN: &str = "gemini-4-hydrogen";
const LEGACY_GEMINI_MODEL_FLASH: &str = "gemini-3.8-flash";
const MAX_FRAMES: usize = 30;
const MAX_SUPPORTED_FRAMES: usize = 60;
const MAX_INLINE_REQUEST_BYTES: usize = 12 * 1024 * 1024;
const MAX_EDGE: u32 = 1024;
const MAX_GEMINI_RETRY_AFTER_SECONDS: u64 = 60;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisStarted {
    run_id: String,
    recording_id: String,
    file_name: String,
    model: String,
    primary_model: String,
    fallback_model: Option<String>,
    automatic_fallback: bool,
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
    primary_model: String,
    fallback_model: Option<String>,
    automatic_fallback: bool,
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
    #[serde(alias = "schema_version")]
    pub schema_version: u32,
    pub summary: String,
    pub steps: Vec<TutorialStep>,
    #[serde(alias = "zoom_suggestions")]
    pub zoom_suggestions: Vec<ZoomSuggestion>,
    #[serde(default)]
    pub warnings: Vec<String>,
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
    #[serde(alias = "start_ms")]
    pub start_ms: u64,
    #[serde(alias = "end_ms")]
    pub end_ms: u64,
    #[serde(alias = "evidence_frame_ids")]
    pub evidence_frame_ids: Vec<String>,
    pub confidence: f64,
    #[serde(alias = "evidence_kind")]
    pub evidence_kind: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ZoomSuggestion {
    pub id: String,
    #[serde(alias = "start_ms")]
    pub start_ms: u64,
    #[serde(alias = "end_ms")]
    pub end_ms: u64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(alias = "target_scale")]
    pub target_scale: f64,
    pub rationale: String,
    #[serde(alias = "evidence_frame_ids")]
    pub evidence_frame_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiKeyStatus {
    configured: bool,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GeminiModelSettings {
    primary_model: String,
    fallback_model: Option<String>,
    automatic_fallback: bool,
}

fn is_supported_gemini_model(model: &str) -> bool {
    matches!(
        model,
        GEMINI_MODEL_FLASH | GEMINI_MODEL_FLASH_LITE | GEMINI_MODEL_PRO
    )
}

fn alternate_gemini_model(model: &str) -> Option<&'static str> {
    match model {
        GEMINI_MODEL_FLASH => Some(GEMINI_MODEL_FLASH_LITE),
        GEMINI_MODEL_FLASH_LITE | GEMINI_MODEL_PRO => Some(GEMINI_MODEL_FLASH),
        _ => None,
    }
}

fn migrate_legacy_gemini_model_settings(
    connection: &rusqlite::Connection,
) -> Result<(), AppError> {
    connection.execute(
        "UPDATE gemini_model_settings
         SET primary_model=?1
         WHERE primary_model IN (?2, ?3)",
        params![
            GEMINI_MODEL_FLASH,
            LEGACY_GEMINI_MODEL_HYDROGEN,
            LEGACY_GEMINI_MODEL_FLASH
        ],
    ).map_err(|_| {
        AppError::new(
            "GEMINI_SETTINGS_MIGRATION_FAILED",
            "The project's previous Gemini model preference could not be migrated.",
        )
    })?;
    Ok(())
}

fn settings_from_connection(connection: &rusqlite::Connection) -> Result<GeminiModelSettings, AppError> {
    let (primary_model, automatic_fallback): (String, i64) = connection.query_row(
        "SELECT primary_model, automatic_fallback FROM gemini_model_settings WHERE id=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).map_err(|_| AppError::new("GEMINI_SETTINGS_READ_FAILED", "Gemini model settings could not be read from this project."))?;

    if !is_supported_gemini_model(&primary_model) {
        return Err(AppError::new("GEMINI_MODEL_UNSUPPORTED", "This project stores an unsupported Gemini model. Choose a supported model in the Gemini settings."));
    }
    let automatic_fallback = automatic_fallback != 0;
    let fallback_model = if automatic_fallback {
        alternate_gemini_model(&primary_model).map(str::to_owned)
    } else {
        None
    };
    Ok(GeminiModelSettings { primary_model, fallback_model, automatic_fallback })
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
pub fn get_gemini_model_settings(state: State<'_, AppState>) -> Result<GeminiModelSettings, AppError> {
    let project_dir = state.active_project.lock()
        .map_err(|_| AppError::new("STATE_LOCK_ERROR", "Project state is temporarily unavailable."))?
        .as_ref().cloned()
        .ok_or_else(|| AppError::new("NO_ACTIVE_PROJECT", "Open a project before configuring its AI analysis settings."))?;
    ensure_analysis_table(&project_dir)?;
    let connection = db_connection(&project_dir)?;
    settings_from_connection(&connection)
}

#[tauri::command]
pub fn save_gemini_model_settings(
    primary_model: String,
    automatic_fallback: bool,
    state: State<'_, AppState>,
) -> Result<GeminiModelSettings, AppError> {
    if !is_supported_gemini_model(&primary_model) {
        return Err(AppError::new(
            "GEMINI_MODEL_UNSUPPORTED",
            "Choose Gemini 3.6 Flash, Gemini 3.5 Flash Lite, or Gemini 3.1 Pro Preview.",
        ));
    }
    let project_dir = state.active_project.lock()
        .map_err(|_| AppError::new("STATE_LOCK_ERROR", "Project state is temporarily unavailable."))?
        .as_ref().cloned()
        .ok_or_else(|| AppError::new("NO_ACTIVE_PROJECT", "Open a project before configuring its AI analysis settings."))?;
    ensure_analysis_table(&project_dir)?;
    let connection = db_connection(&project_dir)?;
    connection.execute(
        "UPDATE gemini_model_settings SET primary_model=?1, automatic_fallback=?2 WHERE id=1",
        params![primary_model, if automatic_fallback { 1_i64 } else { 0_i64 }],
    ).map_err(|_| AppError::new("GEMINI_SETTINGS_WRITE_FAILED", "Gemini model settings could not be saved in this project."))?;
    settings_from_connection(&connection)
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

pub(super) fn ensure_analysis_table(project_dir: &Path) -> Result<(), AppError> {
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
            primary_model TEXT NOT NULL DEFAULT 'gemini-3.6-flash',
            fallback_model TEXT,
            automatic_fallback INTEGER NOT NULL DEFAULT 1,
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

    let columns = {
        let mut statement = connection.prepare("PRAGMA table_info(analysis_runs)")
            .map_err(|_| AppError::new("ANALYSIS_DATABASE_ERROR", "The analysis run schema could not be inspected."))?;
        let rows = statement.query_map([], |row| row.get::<_, String>(1))
            .map_err(|_| AppError::new("ANALYSIS_DATABASE_ERROR", "The analysis run columns could not be listed."))?;
        rows.collect::<Result<HashSet<_>, _>>()
            .map_err(|_| AppError::new("ANALYSIS_DATABASE_ERROR", "The analysis run columns could not be read."))?
    };
    for (name, definition) in [
        ("primary_model", "TEXT NOT NULL DEFAULT 'gemini-3.6-flash'"),
        ("fallback_model", "TEXT"),
        ("automatic_fallback", "INTEGER NOT NULL DEFAULT 1"),
    ] {
        if !columns.contains(name) {
            connection.execute(
                &format!("ALTER TABLE analysis_runs ADD COLUMN {name} {definition}"),
                [],
            ).map_err(|_| AppError::new("ANALYSIS_DATABASE_ERROR", "The analysis model routing fields could not be migrated."))?;
        }
    }
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS gemini_model_settings (
            id INTEGER PRIMARY KEY CHECK(id=1),
            primary_model TEXT NOT NULL,
            automatic_fallback INTEGER NOT NULL DEFAULT 1
        );
        INSERT OR IGNORE INTO gemini_model_settings (id, primary_model, automatic_fallback)
        VALUES (1, 'gemini-3.6-flash', 1);"
    ).map_err(|_| AppError::new("ANALYSIS_DATABASE_ERROR", "The Gemini model settings table could not be initialized."))?;
    migrate_legacy_gemini_model_settings(&connection)?;
    Ok(())
}

pub(super) fn resolve_recording(project_dir: &Path, recording_id: &str) -> Result<(String, PathBuf), AppError> {
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
    let count = duration_seconds.ceil().clamp(1.0, MAX_FRAMES as f64) as usize;
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
            sampling_strategy: "periodic_uniform_max_1fps_30_frames".to_string(),
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

    let model_settings = settings_from_connection(&connection)?;
    let run_id = format!("a{}-{}", now_ms(), std::process::id());
    connection.execute(
        "INSERT INTO analysis_runs (
            id, recording_id, source_file_name, status, created_at, provider, model,
            primary_model, fallback_model, automatic_fallback, workflow_description, language
         )
         VALUES (?1, ?2, ?3, 'extracting_frames', ?4, 'gemini', ?5, ?5, ?6, ?7, ?8, ?9)",
        params![
            run_id,
            recording_id,
            file_name,
            now_ms().to_string(),
            model_settings.primary_model,
            model_settings.fallback_model,
            if model_settings.automatic_fallback { 1_i64 } else { 0_i64 },
            workflow_description,
            language
        ],
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

    Ok(AnalysisStarted {
        run_id,
        recording_id,
        file_name,
        model: model_settings.primary_model.clone(),
        primary_model: model_settings.primary_model,
        fallback_model: model_settings.fallback_model,
        automatic_fallback: model_settings.automatic_fallback,
    })
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
                duration_seconds, provider, model, primary_model, fallback_model, automatic_fallback,
                error_code, workflow_description, language
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
                primary_model: row.get(10)?,
                fallback_model: row.get(11)?,
                automatic_fallback: row.get::<_, i64>(12)? != 0,
                error_code: row.get(13)?,
                workflow_description: row.get(14)?,
                language: row.get(15)?,
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
        return Err(AppError::new(
            "ANALYSIS_SCHEMA_UNSUPPORTED",
            format!("Unsupported tutorial schema version {}.", draft.schema_version),
        ));
    }
    if draft.summary.trim().is_empty() {
        return Err(AppError::new("ANALYSIS_OUTPUT_INVALID", "Draft validation failed: empty_summary."));
    }
    if draft.summary.chars().count() > 4000 {
        return Err(AppError::new("ANALYSIS_OUTPUT_INVALID", "Draft validation failed: summary_exceeds_4000_characters."));
    }
    if draft.steps.is_empty() {
        return Err(AppError::new("ANALYSIS_OUTPUT_INVALID", "Draft validation failed: no_tutorial_steps."));
    }
    if draft.steps.len() > 60 {
        return Err(AppError::new("ANALYSIS_OUTPUT_INVALID", "Draft validation failed: tutorial_step_count_exceeds_60."));
    }
    if draft.zoom_suggestions.len() > 100 {
        return Err(AppError::new("ANALYSIS_OUTPUT_INVALID", "Draft validation failed: zoom_suggestion_count_exceeds_100."));
    }

    // Tutorial steps are core output and remain strict.
    let duration = manifest.source_duration_ms;
    let known_frames: HashSet<&str> = manifest.frames.iter().map(|f| f.id.as_str()).collect();
    let mut previous_end = 0_u64;
    let mut step_ids = HashSet::new();
    for (index, step) in draft.steps.iter().enumerate() {
        let issue = if step.id.trim().is_empty() {
            Some("empty_step_id")
        } else if !step_ids.insert(step.id.as_str()) {
            Some("duplicate_step_id")
        } else if step.title.trim().is_empty() {
            Some("empty_step_title")
        } else if step.title.chars().count() > 160 {
            Some("step_title_exceeds_160_characters")
        } else if step.description.trim().is_empty() {
            Some("empty_step_description")
        } else if step.description.chars().count() > 1200 {
            Some("step_description_exceeds_1200_characters")
        } else if step.start_ms >= step.end_ms {
            Some("step_start_must_precede_end")
        } else if step.end_ms > duration {
            Some("step_end_exceeds_source_duration")
        } else if index > 0 && step.start_ms < previous_end {
            Some("steps_are_not_time_ordered_or_overlap")
        } else if !step.confidence.is_finite() || !(0.0..=1.0).contains(&step.confidence) {
            Some("step_confidence_out_of_range")
        } else if !["observed", "inference"].contains(&step.evidence_kind.as_str()) {
            Some("invalid_step_evidence_kind")
        } else if step.evidence_frame_ids.is_empty() {
            Some("step_has_no_evidence_frames")
        } else if step.evidence_frame_ids.iter().any(|id| !known_frames.contains(id.as_str())) {
            Some("step_references_unknown_frame_id")
        } else {
            None
        };
        if let Some(issue) = issue {
            return Err(AppError::new(
                "ANALYSIS_OUTPUT_INVALID",
                format!("Draft validation failed: tutorial_step[{}] {issue}.", index + 1),
            ));
        }
        previous_end = step.end_ms;
    }
    Ok(())
}

#[derive(Debug)]
struct RejectedZoomSuggestion {
    index: usize,
    reason: &'static str,
    start_ms: u64,
    end_ms: u64,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    target_scale: f64,
    evidence_frame_count: usize,
}

fn filter_invalid_zoom_suggestions(
    draft: &mut AnalysisDraft,
    manifest: &FrameManifest,
) -> Vec<RejectedZoomSuggestion> {
    let duration = manifest.source_duration_ms;
    let known_frames: HashSet<&str> = manifest.frames.iter().map(|frame| frame.id.as_str()).collect();
    let mut seen_ids = HashSet::<String>::new();
    let mut valid = Vec::with_capacity(draft.zoom_suggestions.len());
    let mut rejected = Vec::new();

    for (index, zoom) in draft.zoom_suggestions.drain(..).enumerate() {
        let issue = if zoom.id.trim().is_empty() {
            Some("empty_zoom_id")
        } else if !seen_ids.insert(zoom.id.clone()) {
            Some("duplicate_zoom_id")
        } else if zoom.start_ms >= zoom.end_ms {
            Some("zoom_start_must_precede_end")
        } else if zoom.end_ms > duration {
            Some("zoom_end_exceeds_source_duration")
        } else if !zoom.x.is_finite() || !zoom.y.is_finite()
            || !zoom.width.is_finite() || !zoom.height.is_finite()
        {
            Some("zoom_geometry_contains_non_finite_values")
        } else if zoom.x < 0.0 || zoom.y < 0.0 || zoom.width <= 0.0 || zoom.height <= 0.0
            || zoom.x + zoom.width > 1.0 || zoom.y + zoom.height > 1.0
        {
            Some("zoom_geometry_out_of_bounds")
        } else if !zoom.target_scale.is_finite() || !(1.0..=3.0).contains(&zoom.target_scale) {
            Some("zoom_target_scale_out_of_range")
        } else if zoom.rationale.trim().is_empty() {
            Some("empty_zoom_rationale")
        } else if zoom.evidence_frame_ids.is_empty() {
            Some("zoom_has_no_evidence_frames")
        } else if zoom.evidence_frame_ids.iter().any(|id| !known_frames.contains(id.as_str())) {
            Some("zoom_references_unknown_frame_id")
        } else {
            None
        };

        if let Some(reason) = issue {
            draft.warnings.push(format!(
                "Zoom suggestion {} was omitted: {}.",
                index + 1,
                reason.replace('_', " ")
            ));
            rejected.push(RejectedZoomSuggestion {
                index: index + 1,
                reason,
                start_ms: zoom.start_ms,
                end_ms: zoom.end_ms,
                x: zoom.x,
                y: zoom.y,
                width: zoom.width,
                height: zoom.height,
                target_scale: zoom.target_scale,
                evidence_frame_count: zoom.evidence_frame_ids.len(),
            });
        } else {
            valid.push(zoom);
        }
    }

    draft.zoom_suggestions = valid;
    rejected
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
                        "x":{"type":"number","minimum":0,"maximum":1,"description":"Normalized horizontal position of the top-left corner relative to the full frame, from 0 to 1."},
                        "y":{"type":"number","minimum":0,"maximum":1,"description":"Normalized vertical position of the top-left corner relative to the full frame, from 0 to 1."},
                        "width":{"type":"number","exclusiveMinimum":0,"maximum":1,"description":"Normalized width as a fraction of the full frame; x plus width must not exceed 1."},
                        "height":{"type":"number","exclusiveMinimum":0,"maximum":1,"description":"Normalized height as a fraction of the full frame; y plus height must not exceed 1."},
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

#[derive(Debug, PartialEq, Eq)]
enum GeminiRetryDecision {
    Wait(Duration),
    Defer,
}

fn gemini_retry_decision(attempt: usize, retry_after: Option<&str>) -> GeminiRetryDecision {
    if let Some(value) = retry_after {
        let Ok(seconds) = value.trim().parse::<u128>() else {
            // Do not ignore a provider-supplied delay that cannot be interpreted safely.
            return GeminiRetryDecision::Defer;
        };
        if seconds > MAX_GEMINI_RETRY_AFTER_SECONDS as u128 {
            // Defer instead of retrying before Google's requested delay has elapsed.
            return GeminiRetryDecision::Defer;
        }
        return GeminiRetryDecision::Wait(Duration::from_secs((seconds as u64).max(1)));
    }

    GeminiRetryDecision::Wait(Duration::from_secs(1_u64 << attempt.min(3)))
}

fn wait_for_gemini_retry(
    attempt: usize,
    retry_after: Option<&str>,
    cancellation: &AtomicBool,
) -> Result<(), AppError> {
    let delay = match gemini_retry_decision(attempt, retry_after) {
        GeminiRetryDecision::Wait(delay) => delay,
        GeminiRetryDecision::Defer => {
            return Err(AppError::new(
                "GEMINI_RETRY_DEFERRED",
                "Google requested a retry delay longer than Screenstory's 60-second automatic retry window, or returned a Retry-After value that could not be interpreted safely. Screenstory stopped instead of retrying early. Please try again later.",
            ));
        }
    };

    let mut remaining = delay;
    while !remaining.is_zero() {
        if cancellation.load(Ordering::SeqCst) {
            return Err(AppError::new(
                "ANALYSIS_CANCELLED",
                "Analysis was cancelled while waiting to retry Gemini.",
            ));
        }
        let interval = remaining.min(Duration::from_millis(100));
        thread::sleep(interval);
        remaining = remaining.saturating_sub(interval);
    }

    if cancellation.load(Ordering::SeqCst) {
        return Err(AppError::new(
            "ANALYSIS_CANCELLED",
            "Analysis was cancelled while waiting to retry Gemini.",
        ));
    }

    Ok(())
}

fn redact_provider_error_body(body: &str, api_key: &str) -> String {
    let sanitized = if api_key.is_empty() {
        body.to_string()
    } else {
        body.replace(api_key, "[REDACTED]")
    };
    sanitized.chars().take(4096).collect()
}

struct GeminiHttpErrorContext<'a> {
    model: &'a str,
    status: u16,
    attempt: usize,
    request_id: Option<&'a str>,
    retry_after: Option<&'a str>,
    body: &'a str,
    api_key: &'a str,
}

fn append_gemini_http_error(run_dir: &Path, context: GeminiHttpErrorContext<'_>) {
    let diagnostic = json!({
        "timestamp_ms": now_ms(),
        "provider": "google-gemini",
        "model": context.model,
        "http_status": context.status,
        "attempt": context.attempt + 1,
        "request_id": context.request_id,
        "retry_after": context.retry_after,
        "response_body": redact_provider_error_body(context.body, context.api_key)
    });
    let path = run_dir.join("provider-errors.jsonl");
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        if let Ok(mut line) = serde_json::to_vec(&diagnostic) {
            line.push(b'\n');
            let _ = file.write_all(&line);
        }
    }
}

fn append_gemini_output_diagnostic(
    run_dir: &Path,
    model: &str,
    stage: &str,
    reason: &str,
    interaction_id: Option<&str>,
    response_bytes: usize,
    schema_version: Option<u32>,
    step_count: Option<usize>,
    zoom_count: Option<usize>,
) {
    // Deliberately store metadata and validation reasons only. Never log sampled
    // screenshots, workflow text, or the model-generated tutorial body.
    let diagnostic = json!({
        "timestamp_ms": now_ms(),
        "provider": "google-gemini",
        "event": "analysis_output_rejected",
        "model": model,
        "stage": stage,
        "reason": reason,
        "interaction_id": interaction_id,
        "response_bytes": response_bytes,
        "schema_version": schema_version,
        "step_count": step_count,
        "zoom_suggestion_count": zoom_count
    });
    let path = run_dir.join("provider-errors.jsonl");
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        if let Ok(mut line) = serde_json::to_vec(&diagnostic) {
            line.push(b'\n');
            let _ = file.write_all(&line);
        }
    }
}

fn append_gemini_zoom_omission_diagnostic(
    run_dir: &Path,
    model: &str,
    rejected: &[RejectedZoomSuggestion],
) {
    let diagnostic = json!({
        "timestamp_ms": now_ms(),
        "provider": "google-gemini",
        "event": "optional_zoom_suggestions_omitted",
        "model": model,
        "rejected": rejected.iter().map(|zoom| json!({
            "index": zoom.index,
            "reason": zoom.reason,
            "start_ms": zoom.start_ms,
            "end_ms": zoom.end_ms,
            "normalized_geometry": {
                "x": zoom.x,
                "y": zoom.y,
                "width": zoom.width,
                "height": zoom.height
            },
            "target_scale": zoom.target_scale,
            "evidence_frame_count": zoom.evidence_frame_count
        })).collect::<Vec<_>>()
    });
    let path = run_dir.join("provider-errors.jsonl");
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        if let Ok(mut line) = serde_json::to_vec(&diagnostic) {
            line.push(b'\n');
            let _ = file.write_all(&line);
        }
    }
}

fn gemini_request_body(manifest: &FrameManifest, run_dir: &Path, model: &str) -> Result<Value, AppError> {
    let mut content = vec![json!({
        "type":"text",
        "text": format!(
            "Analyze these timestamped screenshots from one desktop workflow recording. Produce a concise, evidence-grounded tutorial. Do not invent actions not visible in the frames. Distinguish observed evidence from inference. Every step and zoom suggestion must cite one or more supplied frame IDs. Zoom suggestions are optional; return an empty zoom_suggestions array when no clear zoom target is visible. For each zoom, x and y are normalized top-left coordinates relative to the entire frame, and width and height are normalized fractions of that frame (all values 0 to 1); x + width and y + height must each be at most 1. These are not pixel coordinates. Use milliseconds from the source video. Language: {}. Optional workflow context: {}. Source duration: {} ms. Source dimensions: {}x{}. Available frame IDs and timestamps: {}",
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
        "model": model,
        "input": content,
        "system_instruction": "You create clear software workflow tutorials from visual evidence. Never claim a click, typed value, or hidden action unless a frame proves it. Zoom suggestions are optional: omit a zoom if its rectangle cannot be confidently placed fully inside the frame. Zoom x/y are normalized top-left coordinates and width/height are normalized fractions of the full frame, all from 0 to 1; x + width and y + height must not exceed 1. Never use pixel coordinates. Do not include markdown fences.",
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

fn should_use_gemini_fallback(error_code: &str, automatic_fallback: bool) -> bool {
    automatic_fallback && error_code == "GEMINI_SERVER_ERROR"
}

fn request_gemini_model(
    client: &Client,
    api_key: &str,
    model: &str,
    body: &Value,
    run_dir: &Path,
    cancellation: &AtomicBool,
) -> Result<reqwest::blocking::Response, AppError> {
    let mut last_error_code = "GEMINI_NETWORK_ERROR";
    for attempt in 0..3 {
        if cancellation.load(Ordering::SeqCst) {
            return Err(AppError::new("ANALYSIS_CANCELLED", "Analysis was cancelled."));
        }
        match client.post(GEMINI_ENDPOINT)
            .header("x-goog-api-key", api_key)
            .header("api-revision", "2026-05-20")
            .header("content-type", "application/json")
            .json(body)
            .send()
        {
            Ok(result) if result.status().is_success() => return Ok(result),
            Ok(result) => {
                let status = result.status().as_u16();
                let request_id = result.headers()
                    .get("x-goog-request-id")
                    .or_else(|| result.headers().get("x-request-id"))
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned);
                let retry_after = result.headers()
                    .get("retry-after")
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned);
                let response = result;
                let mut response_bytes = Vec::with_capacity(4096);
                let _ = response.take(4096).read_to_end(&mut response_bytes);
                let response_body = String::from_utf8_lossy(&response_bytes).into_owned();
                append_gemini_http_error(
                    run_dir,
                    GeminiHttpErrorContext {
                        model,
                        status,
                        attempt,
                        request_id: request_id.as_deref(),
                        retry_after: retry_after.as_deref(),
                        body: &response_body,
                        api_key,
                    },
                );

                if status == 401 || status == 403 {
                    return Err(AppError::new("GEMINI_AUTH_FAILED", "Google rejected the Gemini API key. Check the key in Screenstory settings."));
                } else if status == 429 {
                    last_error_code = "GEMINI_RATE_LIMITED";
                } else if status >= 500 {
                    last_error_code = "GEMINI_SERVER_ERROR";
                } else {
                    return Err(AppError::new("GEMINI_REQUEST_REJECTED", format!("Gemini rejected the analysis request (HTTP {status}). Check the selected model and request settings.")));
                }
                if attempt < 2 {
                    wait_for_gemini_retry(attempt, retry_after.as_deref(), cancellation)?;
                }
            }
            Err(_) => {
                last_error_code = "GEMINI_NETWORK_ERROR";
                if attempt < 2 {
                    for _ in 0..5 {
                        if cancellation.load(Ordering::SeqCst) {
                            return Err(AppError::new("ANALYSIS_CANCELLED", "Analysis was cancelled while waiting to retry Gemini."));
                        }
                        thread::sleep(Duration::from_millis(100 * (attempt + 1) as u64));
                    }
                }
            }
        }
    }
    let message = match last_error_code {
        "GEMINI_SERVER_ERROR" => format!("Gemini model {model} remained unavailable after automatic retries."),
        "GEMINI_RATE_LIMITED" => format!("Gemini model {model} remained rate limited after automatic retries."),
        _ => format!("Gemini model {model} could not complete the request after bounded network retries."),
    };
    Err(AppError::new(last_error_code, message))
}

fn append_gemini_model_fallback(run_dir: &Path, primary_model: &str, fallback_model: &str) {
    let diagnostic = json!({
        "timestamp_ms": now_ms(),
        "provider": "google-gemini",
        "event": "automatic_model_fallback",
        "primary_model": primary_model,
        "fallback_model": fallback_model,
        "reason": "primary_model_server_error_after_retries"
    });
    let path = run_dir.join("provider-errors.jsonl");
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        if let Ok(mut line) = serde_json::to_vec(&diagnostic) {
            line.push(b'\n');
            let _ = file.write_all(&line);
        }
    }
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
        let (manifest_path, workflow, language, recording_id, primary_model, fallback_model, automatic_fallback):
            (String, String, String, String, String, Option<String>, i64) = connection.query_row(
                "SELECT manifest_path, workflow_description, language, recording_id, primary_model, fallback_model, automatic_fallback
                 FROM analysis_runs WHERE id=?1 AND status='sending_to_provider'",
                params![run_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?))
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
        if manifest.recording_id != recording_id || manifest.run_id != run_id || manifest.frames.is_empty() || manifest.frames.len() > MAX_SUPPORTED_FRAMES {
            return Err(AppError::new("ANALYSIS_MANIFEST_MISMATCH", "The analysis manifest does not match this run or contains an invalid frame count."));
        }
        let run_dir = project_dir.join("analysis").join(&run_id);
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .connect_timeout(Duration::from_secs(20))
            .build()
            .map_err(|_| AppError::new("GEMINI_CLIENT_ERROR", "The Gemini network client could not be initialized."))?;

        if !is_supported_gemini_model(&primary_model) {
            return Err(AppError::new("GEMINI_MODEL_UNSUPPORTED", "This analysis run references a model that is not supported by this Screenstory version."));
        }
        let valid_fallback = fallback_model.filter(|candidate| {
            is_supported_gemini_model(candidate) && candidate != &primary_model
        });
        let primary_body = gemini_request_body(&manifest, &run_dir, &primary_model)?;
        let (response, used_model) = match request_gemini_model(
            &client, &api_key, &primary_model, &primary_body, &run_dir, cancellation.as_ref(),
        ) {
            Ok(successful_response) => (successful_response, primary_model.clone()),
            Err(primary_error)
                if should_use_gemini_fallback(primary_error.code, automatic_fallback != 0)
                    && valid_fallback.is_some() =>
            {
                let fallback_name = valid_fallback.as_deref().expect("fallback was checked above");
                append_gemini_model_fallback(&run_dir, &primary_model, fallback_name);
                connection.execute(
                    "UPDATE analysis_runs SET model=?1 WHERE id=?2",
                    params![fallback_name, run_id],
                ).map_err(|_| AppError::new("ANALYSIS_DATABASE_ERROR", "The active fallback model could not be recorded."))?;
                if cancellation.load(Ordering::SeqCst) {
                    return Err(AppError::new("ANALYSIS_CANCELLED", "Analysis was cancelled before the fallback model request."));
                }
                let fallback_body = gemini_request_body(&manifest, &run_dir, fallback_name)?;
                match request_gemini_model(
                    &client, &api_key, fallback_name, &fallback_body, &run_dir, cancellation.as_ref(),
                ) {
                    Ok(successful_response) => (successful_response, fallback_name.to_string()),
                    Err(fallback_error) if fallback_error.code == "GEMINI_SERVER_ERROR" => {
                        return Err(AppError::new(
                            "GEMINI_MODELS_UNAVAILABLE",
                            format!("Both Gemini models ({primary_model} and {fallback_name}) remained unavailable after automatic retries. Please try again later."),
                        ));
                    }
                    Err(fallback_error) => return Err(fallback_error),
                }
            }
            Err(primary_error) => return Err(primary_error),
        };
        connection.execute(
            "UPDATE analysis_runs SET model=?1 WHERE id=?2",
            params![used_model, run_id],
        ).map_err(|_| AppError::new("ANALYSIS_DATABASE_ERROR", "The model used for this analysis could not be recorded."))?;
        if cancellation.load(Ordering::SeqCst) {
            return Err(AppError::new("ANALYSIS_CANCELLED", "Analysis was cancelled after the provider request."));
        }
        let response_bytes = response.content_length().unwrap_or_default() as usize;
        let payload: Value = match response.json() {
            Ok(payload) => payload,
            Err(_) => {
                append_gemini_output_diagnostic(
                    &run_dir,
                    &used_model,
                    "interaction_response_json",
                    "response_json_decode_failed",
                    None,
                    response_bytes,
                    None,
                    None,
                    None,
                );
                return Err(AppError::new(
                    "GEMINI_RESPONSE_INVALID",
                    "Gemini returned a response that could not be parsed. See the local provider diagnostics.",
                ));
            }
        };
        let interaction_id = payload.get("id").and_then(Value::as_str);
        let text = match response_text(&payload) {
            Some(text) => text,
            None => {
                append_gemini_output_diagnostic(
                    &run_dir,
                    &used_model,
                    "interaction_model_output",
                    "no_text_model_output_step",
                    interaction_id,
                    response_bytes,
                    None,
                    None,
                    None,
                );
                return Err(AppError::new(
                    "GEMINI_RESPONSE_EMPTY",
                    "Gemini returned no structured tutorial content. See the local provider diagnostics.",
                ));
            }
        };
        let mut draft: AnalysisDraft = match serde_json::from_str(&text) {
            Ok(draft) => draft,
            Err(error) => {
                let reason = error.to_string();
                append_gemini_output_diagnostic(
                    &run_dir,
                    &used_model,
                    "draft_json_deserialization",
                    &reason,
                    interaction_id,
                    text.len(),
                    None,
                    None,
                    None,
                );
                return Err(AppError::new(
                    "GEMINI_RESPONSE_INVALID",
                    "Gemini's structured response did not match the tutorial schema. See the local provider diagnostics.",
                ));
            }
        };
        if let Err(error) = validate_draft(&draft, &manifest) {
            append_gemini_output_diagnostic(
                &run_dir,
                &used_model,
                "draft_validation",
                &error.message,
                interaction_id,
                text.len(),
                Some(draft.schema_version),
                Some(draft.steps.len()),
                Some(draft.zoom_suggestions.len()),
            );
            return Err(error);
        }
        let rejected_zooms = filter_invalid_zoom_suggestions(&mut draft, &manifest);
        if !rejected_zooms.is_empty() {
            append_gemini_zoom_omission_diagnostic(&run_dir, &used_model, &rejected_zooms);
        }
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
        assert_eq!(sample_timestamps(2.0), vec![0, 1000]);
        assert_eq!(sample_timestamps(30.0).len(), 30);
        assert_eq!(sample_timestamps(900.0).len(), 30);
        assert_eq!(sample_timestamps(1.0), vec![0]);
    }

    #[test]
    fn scaled_dimensions_never_exceed_the_edge_limit() {
        assert_eq!(scaled_dimension(1920, 1080), (1024, 576));
        assert_eq!(scaled_dimension(1080, 1920), (576, 1024));
        assert_eq!(scaled_dimension(800, 600), (800, 600));
    }

    #[test]
    fn gemini_retry_decision_respects_provider_retry_after_without_retrying_early() {
        assert_eq!(gemini_retry_decision(0, None), GeminiRetryDecision::Wait(Duration::from_secs(1)));
        assert_eq!(gemini_retry_decision(1, None), GeminiRetryDecision::Wait(Duration::from_secs(2)));
        assert_eq!(gemini_retry_decision(2, None), GeminiRetryDecision::Wait(Duration::from_secs(4)));
        assert_eq!(gemini_retry_decision(0, Some("7")), GeminiRetryDecision::Wait(Duration::from_secs(7)));
        assert_eq!(gemini_retry_decision(0, Some("30")), GeminiRetryDecision::Wait(Duration::from_secs(30)));
        assert_eq!(gemini_retry_decision(0, Some("60")), GeminiRetryDecision::Wait(Duration::from_secs(60)));
        assert_eq!(gemini_retry_decision(0, Some("61")), GeminiRetryDecision::Defer);
        assert_eq!(gemini_retry_decision(0, Some("999")), GeminiRetryDecision::Defer);
        assert_eq!(gemini_retry_decision(0, Some("not-a-number")), GeminiRetryDecision::Defer);
    }

    #[test]
    fn gemini_retry_wait_stops_when_analysis_is_cancelled() {
        let cancellation = AtomicBool::new(true);
        let error = wait_for_gemini_retry(0, Some("30"), &cancellation).unwrap_err();
        assert_eq!(error.code, "ANALYSIS_CANCELLED");
    }

    #[test]
    fn gemini_retry_wait_defers_when_provider_delay_exceeds_safe_window() {
        let cancellation = AtomicBool::new(false);
        let error = wait_for_gemini_retry(0, Some("61"), &cancellation).unwrap_err();
        assert_eq!(error.code, "GEMINI_RETRY_DEFERRED");
    }

    #[test]
    fn walkmark_gemini_models_are_supported_with_safe_fallback_pairs() {
        assert!(is_supported_gemini_model(GEMINI_MODEL_FLASH));
        assert!(is_supported_gemini_model(GEMINI_MODEL_FLASH_LITE));
        assert!(is_supported_gemini_model(GEMINI_MODEL_PRO));
        assert!(!is_supported_gemini_model(LEGACY_GEMINI_MODEL_HYDROGEN));
        assert!(!is_supported_gemini_model(LEGACY_GEMINI_MODEL_FLASH));
        assert_eq!(
            alternate_gemini_model(GEMINI_MODEL_FLASH),
            Some(GEMINI_MODEL_FLASH_LITE)
        );
        assert_eq!(
            alternate_gemini_model(GEMINI_MODEL_FLASH_LITE),
            Some(GEMINI_MODEL_FLASH)
        );
        assert_eq!(
            alternate_gemini_model(GEMINI_MODEL_PRO),
            Some(GEMINI_MODEL_FLASH)
        );
        assert_eq!(alternate_gemini_model("unknown"), None);
    }

    #[test]
    fn automatic_fallback_is_only_for_server_errors() {
        assert!(should_use_gemini_fallback("GEMINI_SERVER_ERROR", true));
        assert!(!should_use_gemini_fallback("GEMINI_RATE_LIMITED", true));
        assert!(!should_use_gemini_fallback("GEMINI_NETWORK_ERROR", true));
        assert!(!should_use_gemini_fallback("GEMINI_SERVER_ERROR", false));
    }

    #[test]
    fn project_gemini_settings_default_to_flash_with_the_expected_fallback() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection.execute_batch(
            "CREATE TABLE gemini_model_settings (
                id INTEGER PRIMARY KEY CHECK(id=1),
                primary_model TEXT NOT NULL,
                automatic_fallback INTEGER NOT NULL DEFAULT 1
            );
            INSERT INTO gemini_model_settings (id, primary_model, automatic_fallback)
            VALUES (1, 'gemini-3.6-flash', 1);"
        ).unwrap();

        let defaults = settings_from_connection(&connection).unwrap();
        assert_eq!(defaults.primary_model, GEMINI_MODEL_FLASH);
        assert_eq!(defaults.fallback_model.as_deref(), Some(GEMINI_MODEL_FLASH_LITE));
        assert!(defaults.automatic_fallback);

        for (primary, fallback) in [
            (GEMINI_MODEL_FLASH_LITE, GEMINI_MODEL_FLASH),
            (GEMINI_MODEL_PRO, GEMINI_MODEL_FLASH),
        ] {
            connection.execute(
                "UPDATE gemini_model_settings SET primary_model=?1, automatic_fallback=1 WHERE id=1",
                params![primary],
            ).unwrap();
            let saved = settings_from_connection(&connection).unwrap();
            assert_eq!(saved.primary_model, primary);
            assert_eq!(saved.fallback_model.as_deref(), Some(fallback));
            assert!(saved.automatic_fallback);
        }

        connection.execute(
            "UPDATE gemini_model_settings SET primary_model=?1, automatic_fallback=0 WHERE id=1",
            params![GEMINI_MODEL_FLASH_LITE],
        ).unwrap();
        let saved = settings_from_connection(&connection).unwrap();
        assert_eq!(saved.primary_model, GEMINI_MODEL_FLASH_LITE);
        assert_eq!(saved.fallback_model, None);
        assert!(!saved.automatic_fallback);
    }

    #[test]
    fn legacy_project_gemini_preferences_migrate_to_gemini_36_flash() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection.execute_batch(
            "CREATE TABLE gemini_model_settings (
                id INTEGER PRIMARY KEY CHECK(id=1),
                primary_model TEXT NOT NULL,
                automatic_fallback INTEGER NOT NULL DEFAULT 1
            );"
        ).unwrap();

        for legacy_model in [
            LEGACY_GEMINI_MODEL_HYDROGEN,
            LEGACY_GEMINI_MODEL_FLASH,
        ] {
            connection.execute(
                "INSERT OR REPLACE INTO gemini_model_settings (id, primary_model, automatic_fallback)
                 VALUES (1, ?1, 1)",
                params![legacy_model],
            ).unwrap();
            migrate_legacy_gemini_model_settings(&connection).unwrap();
            let settings = settings_from_connection(&connection).unwrap();
            assert_eq!(settings.primary_model, GEMINI_MODEL_FLASH);
        }
    }

    #[test]
    fn provider_error_body_redacts_api_key_and_is_bounded() {
        let body = format!(r#"{{"message":"bad key: secret-value","detail":"{}"}}"#, "x".repeat(5000));
        let redacted = redact_provider_error_body(&body, "secret-value");
        assert!(!redacted.contains("secret-value"));
        assert!(redacted.contains("[REDACTED]"));
        assert!(redacted.chars().count() <= 4096);
    }

    #[test]
    fn output_diagnostics_record_validation_metadata_without_model_content_or_secrets() {
        let run_dir = std::env::temp_dir().join(format!(
            "screenstory-output-diagnostic-{}-{}",
            std::process::id(),
            now_ms()
        ));
        fs::create_dir_all(&run_dir).unwrap();

        append_gemini_output_diagnostic(
            &run_dir,
            GEMINI_MODEL_FLASH,
            "draft_validation",
            "Draft validation failed: tutorial_step[1] step_references_unknown_frame_id.",
            Some("interaction-test-id"),
            1234,
            Some(1),
            Some(2),
            Some(0),
        );

        let contents = fs::read_to_string(run_dir.join("provider-errors.jsonl")).unwrap();
        let value: Value = serde_json::from_str(contents.trim()).unwrap();
        assert_eq!(value["event"], "analysis_output_rejected");
        assert_eq!(value["model"], GEMINI_MODEL_FLASH);
        assert_eq!(value["stage"], "draft_validation");
        assert_eq!(value["step_count"], 2);
        assert_eq!(value["interaction_id"], "interaction-test-id");
        assert!(!contents.contains("sample screenshot text"));
        assert!(!contents.contains("AIza"));
        fs::remove_dir_all(run_dir).unwrap();
    }

    #[test]
    fn extracts_model_output_from_gemini_interactions_steps() {
        let payload = json!({
            "id": "v1_example",
            "status": "completed",
            "steps": [
                {"type":"thought","signature":"ignored"},
                {"type":"model_output","content":[
                    {"type":"text","text":r#"{"schema_version":1,"#},
                    {"type":"text","text":r#""summary":"Captured workflow"}"#}
                ]}
            ]
        });
        assert_eq!(
            response_text(&payload).as_deref(),
            Some("{\"schema_version\":1,\n\"summary\":\"Captured workflow\"}")
        );
    }


    fn test_frame_manifest() -> FrameManifest {
        FrameManifest {
            schema_version: 1,
            run_id: "a12345-1".to_string(),
            recording_id: "recording-1".to_string(),
            source_file_name: "recording.mp4".to_string(),
            source_duration_ms: 5_000,
            source_width: 1920,
            source_height: 1080,
            sampling_strategy: "test".to_string(),
            sample_count: 1,
            max_edge: 1024,
            total_bytes: 100,
            workflow_description: String::new(),
            language: "en".to_string(),
            frames: vec![FrameReference {
                id: "frame-000".to_string(),
                timestamp_ms: 0,
                relative_path: "frames/frame-000.jpg".to_string(),
                width: 1024,
                height: 576,
                mime_type: "image/jpeg".to_string(),
                byte_size: 100,
            }],
        }
    }

    #[test]
    fn invalid_optional_zoom_is_omitted_without_rejecting_valid_tutorial_steps() {
        let manifest = test_frame_manifest();
        let mut draft: AnalysisDraft = serde_json::from_value(json!({
            "schema_version": 1,
            "summary": "A valid tutorial summary",
            "steps": [{
                "id": "step-1",
                "title": "Open settings",
                "description": "Open the application settings.",
                "start_ms": 100,
                "end_ms": 900,
                "evidence_frame_ids": ["frame-000"],
                "confidence": 0.95,
                "evidence_kind": "observed"
            }],
            "zoom_suggestions": [
                {
                    "id": "zoom-out-of-bounds",
                    "start_ms": 200,
                    "end_ms": 700,
                    "x": 0.8,
                    "y": 0.1,
                    "width": 0.3,
                    "height": 0.3,
                    "target_scale": 1.5,
                    "rationale": "This rectangle exceeds the right frame edge.",
                    "evidence_frame_ids": ["frame-000"]
                },
                {
                    "id": "zoom-valid",
                    "start_ms": 1000,
                    "end_ms": 1500,
                    "x": 0.1,
                    "y": 0.2,
                    "width": 0.3,
                    "height": 0.4,
                    "target_scale": 1.5,
                    "rationale": "Emphasize the visible settings control.",
                    "evidence_frame_ids": ["frame-000"]
                }
            ]
        })).unwrap();

        validate_draft(&draft, &manifest).unwrap();
        let rejected = filter_invalid_zoom_suggestions(&mut draft, &manifest);

        assert_eq!(draft.steps.len(), 1);
        assert_eq!(draft.zoom_suggestions.len(), 1);
        assert_eq!(draft.zoom_suggestions[0].id, "zoom-valid");
        assert_eq!(draft.warnings.len(), 1);
        assert!(draft.warnings[0].contains("zoom geometry out of bounds"));
        assert_eq!(rejected.len(), 1);
        assert_eq!(rejected[0].index, 1);
        assert_eq!(rejected[0].reason, "zoom_geometry_out_of_bounds");
        assert!((rejected[0].x + rejected[0].width) > 1.0);
    }

    #[test]
    fn invalid_core_tutorial_step_still_rejects_the_draft() {
        let manifest = test_frame_manifest();
        let mut draft: AnalysisDraft = serde_json::from_value(json!({
            "schema_version": 1,
            "summary": "A tutorial summary",
            "steps": [{
                "id": "step-1",
                "title": "Open settings",
                "description": "Open the application settings.",
                "start_ms": 100,
                "end_ms": 900,
                "evidence_frame_ids": ["unknown-frame"],
                "confidence": 0.95,
                "evidence_kind": "observed"
            }],
            "zoom_suggestions": []
        })).unwrap();

        let error = validate_draft(&draft, &manifest).unwrap_err();
        assert_eq!(error.code, "ANALYSIS_OUTPUT_INVALID");
        assert!(error.message.contains("step_references_unknown_frame_id"));
        assert!(draft.warnings.is_empty());
    }

    #[test]
    fn deserializes_snake_case_gemini_schema_and_serializes_camel_case_app_contract() {
        let model_output = json!({
            "schema_version": 1,
            "summary": "Captured workflow",
            "steps": [{
                "id": "step-1",
                "title": "Open settings",
                "description": "Open the application settings.",
                "start_ms": 100,
                "end_ms": 900,
                "evidence_frame_ids": ["frame-000"],
                "confidence": 0.95,
                "evidence_kind": "observed"
            }],
            "zoom_suggestions": [{
                "id": "zoom-1",
                "start_ms": 200,
                "end_ms": 700,
                "x": 0.1,
                "y": 0.1,
                "width": 0.5,
                "height": 0.5,
                "target_scale": 1.5,
                "rationale": "Emphasize the settings control.",
                "evidence_frame_ids": ["frame-000"]
            }]
        });

        let draft: AnalysisDraft = serde_json::from_value(model_output).unwrap();
        assert_eq!(draft.schema_version, 1);
        assert_eq!(draft.steps[0].start_ms, 100);
        assert_eq!(draft.steps[0].evidence_frame_ids, vec!["frame-000"]);
        assert_eq!(draft.zoom_suggestions[0].target_scale, 1.5);
        assert!(draft.warnings.is_empty());

        let app_output = serde_json::to_value(&draft).unwrap();
        assert_eq!(app_output["warnings"], json!([]));
        assert!(app_output.get("schemaVersion").is_some());
        assert!(app_output.get("zoomSuggestions").is_some());
        assert!(app_output["steps"][0].get("startMs").is_some());
        assert!(app_output["steps"][0].get("evidenceFrameIds").is_some());
        assert!(app_output["zoomSuggestions"][0].get("targetScale").is_some());
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
