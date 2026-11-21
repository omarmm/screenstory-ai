// Trigger rustfmt automation.
// Temporary rustfmt extraction trigger; removed after formatting.
use rusqlite::{params, Connection};
use serde::Serialize;
use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::Mutex,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tauri::State;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppError {
    code: &'static str,
    message: String,
}
impl AppError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

#[derive(Default)]
struct AppState {
    active_project: Mutex<Option<PathBuf>>,
    recording: Mutex<Option<RecordingSession>>,
}
struct RecordingSession {
    child: Child,
    stdin: ChildStdin,
    project_dir: PathBuf,
    output_file: PathBuf,
    file_name: String,
    started_at: Instant,
    started_at_ms: u64,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AppInfo { name: &'static str, version: &'static str, runtime: &'static str }

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DependencyStatus {
    id: &'static str, feature: &'static str, status: &'static str,
    version: Option<String>, impact: &'static str, can_install: bool,
    remediation: Option<&'static str>,
}
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DependencyReport { dependencies: Vec<DependencyStatus> }

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ProjectSummary {
    id: String, name: String, created_at: String, updated_at: String, recordings_count: u64,
}
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct RecordingSummary {
    id: String, file_name: String, created_at: String, duration_seconds: f64,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordingStatus {
    is_recording: bool, file_name: Option<String>, started_at_ms: Option<u64>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordingStarted { file_name: String, started_at_ms: u64 }

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}
fn projects_root() -> Result<PathBuf, AppError> {
    let home = std::env::var_os("HOME").map(PathBuf::from)
        .ok_or_else(|| AppError::new("HOME_UNAVAILABLE", "Could not locate your home folder."))?;
    Ok(home.join("Videos").join("Screenstory").join("Projects"))
}
fn db_connection(project_dir: &Path) -> Result<Connection, AppError> {
    let connection = Connection::open(project_dir.join("project.sqlite"))
        .map_err(|_| AppError::new("PROJECT_DATABASE_ERROR", "Could not open this project database."))?;
    connection.execute_batch(
        "PRAGMA foreign_keys = ON;
         CREATE TABLE IF NOT EXISTS project (
             id TEXT PRIMARY KEY, name TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS recordings (
             id TEXT PRIMARY KEY, file_name TEXT NOT NULL UNIQUE, created_at TEXT NOT NULL,
             duration_seconds REAL NOT NULL DEFAULT 0
         );",
    ).map_err(|_| AppError::new("PROJECT_DATABASE_ERROR", "Could not initialize this project's database."))?;
    Ok(connection)
}
fn project_summary(connection: &Connection) -> Result<ProjectSummary, AppError> {
    let (id, name, created_at, updated_at): (String, String, String, String) = connection.query_row(
        "SELECT id, name, created_at, updated_at FROM project LIMIT 1", [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    ).map_err(|_| AppError::new("PROJECT_METADATA_INVALID", "This project is missing valid metadata."))?;
    let count: i64 = connection.query_row("SELECT COUNT(*) FROM recordings", [], |row| row.get(0))
        .map_err(|_| AppError::new("PROJECT_DATABASE_ERROR", "Could not read this project's recordings."))?;
    Ok(ProjectSummary { id, name, created_at, updated_at, recordings_count: count.max(0) as u64 })
}
fn validate_project_name(name: &str) -> Result<String, AppError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::new("INVALID_PROJECT_NAME", "Enter a name for this project."));
    }
    if trimmed.chars().count() > 80 || trimmed.chars().any(char::is_control) {
        return Err(AppError::new("INVALID_PROJECT_NAME", "Project names must be 1–80 characters and cannot contain control characters."));
    }
    Ok(trimmed.to_string())
}
fn valid_project_id(id: &str) -> bool {
    id.starts_with('p') && id.len() > 1
        && id.chars().all(|c| c == 'p' || c == '-' || c.is_ascii_digit())
}
fn resolve_project_dir(id: &str) -> Result<PathBuf, AppError> {
    if !valid_project_id(id) {
        return Err(AppError::new("INVALID_PROJECT_ID", "That project identifier is not valid."));
    }
    let directory = projects_root()?.join(id);
    if !directory.join("project.sqlite").is_file() {
        return Err(AppError::new("PROJECT_NOT_FOUND", "This project could not be found."));
    }
    let connection = db_connection(&directory)?;
    if project_summary(&connection)?.id != id {
        return Err(AppError::new("PROJECT_ID_MISMATCH", "The project identifier does not match its metadata."));
    }
    Ok(directory)
}

fn ffmpeg_status() -> DependencyStatus {
    match Command::new("ffmpeg").arg("-version").output() {
        Ok(output) if output.status.success() => DependencyStatus {
            id: "ffmpeg", feature: "X11 desktop recording and video processing", status: "available",
            version: Some(String::from_utf8_lossy(&output.stdout).lines().next().unwrap_or("FFmpeg available").to_string()),
            impact: "Desktop recording is available.", can_install: false, remediation: None,
        },
        _ => DependencyStatus {
            id: "ffmpeg", feature: "X11 desktop recording and video processing", status: "missing",
            version: None, impact: "Recording is unavailable until FFmpeg is installed.",
            can_install: false,
            remediation: Some("Install FFmpeg from your Linux distribution's Software Manager, then recheck."),
        },
    }
}
fn dependency_report() -> DependencyReport { DependencyReport { dependencies: vec![ffmpeg_status()] } }

fn parse_xrandr_size(output: &str) -> Option<(u32, u32)> {
    let dimensions = output.split("current ").nth(1)?.split(',').next()?;
    let mut parts = dimensions.split_whitespace();
    let width = parts.next()?.parse().ok()?;
    if parts.next()? != "x" { return None; }
    let height = parts.next()?.parse().ok()?;
    if width == 0 || height == 0 { return None; }
    Some((width, height))
}
fn screen_dimensions() -> Result<(u32, u32), AppError> {
    if let Ok(output) = Command::new("xrandr").arg("--current").output() {
        if output.status.success() {
            if let Some(size) = parse_xrandr_size(&String::from_utf8_lossy(&output.stdout)) { return Ok(size); }
        }
    }
    if let Ok(output) = Command::new("xdpyinfo").output() {
        if output.status.success() {
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                if let Some(value) = line.trim().strip_prefix("dimensions:") {
                    if let Some(size) = value.split_whitespace().next() {
                        let mut values = size.split('x');
                        if let (Some(w), Some(h)) = (values.next(), values.next()) {
                            if let (Ok(w), Ok(h)) = (w.parse::<u32>(), h.parse::<u32>()) {
                                if w > 0 && h > 0 { return Ok((w, h)); }
                            }
                        }
                    }
                }
            }
        }
    }
    Err(AppError::new("SCREEN_SIZE_UNAVAILABLE", "Screen resolution could not be detected. Check that the X11 desktop session is available."))
}

#[tauri::command]
fn get_app_info() -> AppInfo {
    AppInfo { name: "Screenstory", version: env!("CARGO_PKG_VERSION"), runtime: "Tauri" }
}
#[tauri::command]
fn check_dependencies() -> DependencyReport { dependency_report() }

#[tauri::command]
fn list_projects() -> Result<Vec<ProjectSummary>, AppError> {
    let root = projects_root()?;
    fs::create_dir_all(&root).map_err(|_| AppError::new("PROJECTS_FOLDER_ERROR", "Could not create the Screenstory projects folder."))?;
    let entries = fs::read_dir(root).map_err(|_| AppError::new("PROJECTS_FOLDER_ERROR", "Could not read the Screenstory projects folder."))?;
    let mut projects = Vec::new();
    for entry in entries.flatten() {
        if !entry.path().join("project.sqlite").is_file() { continue; }
        if let Ok(connection) = db_connection(&entry.path()) {
            if let Ok(summary) = project_summary(&connection) { projects.push(summary); }
        }
    }
    projects.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(projects)
}
#[tauri::command]
fn get_active_project(state: State<'_, AppState>) -> Result<Option<ProjectSummary>, AppError> {
    let active = state.active_project.lock().map_err(|_| AppError::new("STATE_LOCK_ERROR", "Project state is temporarily unavailable."))?;
    match active.as_ref() {
        Some(directory) => Ok(Some(project_summary(&db_connection(directory)?)?)),
        None => Ok(None),
    }
}
#[tauri::command]
fn create_project(name: String, state: State<'_, AppState>) -> Result<ProjectSummary, AppError> {
    let name = validate_project_name(&name)?;
    let root = projects_root()?;
    fs::create_dir_all(&root).map_err(|_| AppError::new("PROJECTS_FOLDER_ERROR", "Could not create the Screenstory projects folder."))?;
    let mut created = None;
    let mut project_id = String::new();
    for attempt in 0..5_u64 {
        project_id = format!("p{}-{}", now_ms() + attempt, std::process::id());
        let candidate = root.join(&project_id);
        match fs::create_dir(&candidate) {
            Ok(()) => { created = Some(candidate); break; }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(AppError::new("PROJECT_CREATE_FAILED", "Could not create the project folder.")),
        }
    }
    let directory = created.ok_or_else(|| AppError::new("PROJECT_CREATE_FAILED", "Could not allocate a unique project folder. Try again."))?;
    let result = (|| {
        for folder in ["recordings", "analysis", "timeline", "assets", "exports"] {
            fs::create_dir(directory.join(folder)).map_err(|_| AppError::new("PROJECT_CREATE_FAILED", "Could not initialize the project folders."))?;
        }
        let connection = db_connection(&directory)?;
        let timestamp = now_ms().to_string();
        connection.execute(
            "INSERT INTO project (id, name, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
            params![project_id, name, timestamp],
        ).map_err(|_| AppError::new("PROJECT_CREATE_FAILED", "Could not save the new project's metadata."))?;
        project_summary(&connection)
    })();
    match result {
        Ok(summary) => {
            *state.active_project.lock().map_err(|_| AppError::new("STATE_LOCK_ERROR", "Project state is temporarily unavailable."))? = Some(directory);
            Ok(summary)
        }
        Err(error) => { let _ = fs::remove_dir_all(&directory); Err(error) }
    }
}
#[tauri::command]
fn open_project(project_id: String, state: State<'_, AppState>) -> Result<ProjectSummary, AppError> {
    let recording = state.recording.lock().map_err(|_| AppError::new("STATE_LOCK_ERROR", "Recording state is temporarily unavailable."))?;
    if recording.is_some() { return Err(AppError::new("RECORDING_IN_PROGRESS", "Stop the current recording before switching projects.")); }
    drop(recording);
    let directory = resolve_project_dir(&project_id)?;
    let summary = project_summary(&db_connection(&directory)?)?;
    *state.active_project.lock().map_err(|_| AppError::new("STATE_LOCK_ERROR", "Project state is temporarily unavailable."))? = Some(directory);
    Ok(summary)
}
#[tauri::command]
fn list_recordings(state: State<'_, AppState>) -> Result<Vec<RecordingSummary>, AppError> {
    let active = state.active_project.lock().map_err(|_| AppError::new("STATE_LOCK_ERROR", "Project state is temporarily unavailable."))?;
    let directory = active.as_ref().ok_or_else(|| AppError::new("NO_ACTIVE_PROJECT", "Create or open a project before viewing recordings."))?;
    let connection = db_connection(directory)?;
    let mut statement = connection.prepare(
        "SELECT id, file_name, created_at, duration_seconds FROM recordings ORDER BY created_at DESC"
    ).map_err(|_| AppError::new("PROJECT_DATABASE_ERROR", "Could not read this project's recordings."))?;
    let rows = statement.query_map([], |row| Ok(RecordingSummary {
        id: row.get(0)?, file_name: row.get(1)?, created_at: row.get(2)?, duration_seconds: row.get(3)?,
    })).map_err(|_| AppError::new("PROJECT_DATABASE_ERROR", "Could not read this project's recordings."))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|_| AppError::new("PROJECT_DATABASE_ERROR", "Could not read this project's recordings."))
}
#[tauri::command]
fn get_recording_status(state: State<'_, AppState>) -> Result<RecordingStatus, AppError> {
    let recording = state.recording.lock().map_err(|_| AppError::new("STATE_LOCK_ERROR", "Recording state is temporarily unavailable."))?;
    Ok(match recording.as_ref() {
        Some(session) => RecordingStatus {
            is_recording: true, file_name: Some(session.file_name.clone()), started_at_ms: Some(session.started_at_ms),
        },
        None => RecordingStatus { is_recording: false, file_name: None, started_at_ms: None },
    })
}
#[tauri::command]
fn start_recording(state: State<'_, AppState>) -> Result<RecordingStarted, AppError> {
    let active = state.active_project.lock().map_err(|_| AppError::new("STATE_LOCK_ERROR", "Project state is temporarily unavailable."))?;
    let project_dir = active.as_ref().cloned().ok_or_else(|| AppError::new("NO_ACTIVE_PROJECT", "Create or open a project before recording."))?;
    drop(active);

    let mut current = state.recording.lock().map_err(|_| AppError::new("STATE_LOCK_ERROR", "Recording state is temporarily unavailable."))?;
    if current.is_some() { return Err(AppError::new("RECORDING_ALREADY_ACTIVE", "A recording is already in progress.")); }

    let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default().to_ascii_lowercase();
    if session != "x11" {
        return Err(AppError::new("UNSUPPORTED_CAPTURE_SESSION", "This build currently records full desktops in X11 sessions only. Wayland capture is not enabled yet."));
    }
    let display = std::env::var("DISPLAY").map_err(|_| AppError::new("DISPLAY_UNAVAILABLE", "The X11 display is unavailable. Launch Screenstory from your desktop session."))?;
    let (width, height) = screen_dimensions()?;
    if !Command::new("ffmpeg").arg("-version").output().map(|output| output.status.success()).unwrap_or(false) {
        return Err(AppError::new("FFMPEG_MISSING", "FFmpeg is required for recording. Install it from your Linux Software Manager, then recheck dependencies."));
    }

    let recordings_dir = project_dir.join("recordings");
    fs::create_dir_all(&recordings_dir).map_err(|_| AppError::new("RECORDING_FOLDER_ERROR", "Could not prepare this project's recordings folder."))?;
    let id = now_ms().to_string();
    let file_name = format!("recording-{id}.mp4");
    let output_file = recordings_dir.join(&file_name);
    let log_file = recordings_dir.join(format!("recording-{id}.log"));
    let log = File::create(log_file).map_err(|_| AppError::new("RECORDING_LOG_ERROR", "Could not create the recording log."))?;

    let mut child = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "warning", "-f", "x11grab", "-framerate", "30", "-draw_mouse", "1", "-video_size"])
        .arg(format!("{width}x{height}")).arg("-i").arg(display)
        .args(["-an", "-vf", "pad=ceil(iw/2)*2:ceil(ih/2)*2", "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-movflags", "+faststart", "-y"])
        .arg(&output_file).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::from(log)).spawn()
        .map_err(|_| AppError::new("RECORDING_START_FAILED", "FFmpeg could not be started. Check that FFmpeg is installed correctly."))?;

    let stdin = child.stdin.take().ok_or_else(|| AppError::new("RECORDING_START_FAILED", "Could not initialize the recording process."))?;
    if let Some(status) = child.try_wait().map_err(|_| AppError::new("RECORDING_START_FAILED", "Could not verify that recording started."))? {
        return Err(AppError::new("RECORDING_START_FAILED", format!("FFmpeg stopped immediately (exit status {}). Check the recording log in this project.", status)));
    }
    let started_at_ms = now_ms();
    *current = Some(RecordingSession {
        child, stdin, project_dir, output_file, file_name: file_name.clone(),
        started_at: Instant::now(), started_at_ms,
    });
    Ok(RecordingStarted { file_name, started_at_ms })
}
#[tauri::command]
fn stop_recording(state: State<'_, AppState>) -> Result<RecordingSummary, AppError> {
    let mut session = state.recording.lock().map_err(|_| AppError::new("STATE_LOCK_ERROR", "Recording state is temporarily unavailable."))?
        .take().ok_or_else(|| AppError::new("NO_ACTIVE_RECORDING", "There is no active recording to stop."))?;
    let _ = session.stdin.write_all(b"q\n");
    let _ = session.stdin.flush();
    let status = session.child.wait().map_err(|_| AppError::new("RECORDING_FINALIZE_FAILED", "Could not finalize the recording. Check the recording log."))?;
    let duration_seconds = session.started_at.elapsed().as_secs_f64();
    if !status.success() || !session.output_file.is_file() || fs::metadata(&session.output_file).map(|meta| meta.len()).unwrap_or(0) == 0 {
        return Err(AppError::new("RECORDING_FAILED", format!("FFmpeg could not finalize {}. The project recording log may contain more details.", session.file_name)));
    }
    let created_at = session.started_at_ms.to_string();
    let connection = db_connection(&session.project_dir)?;
    connection.execute(
        "INSERT INTO recordings (id, file_name, created_at, duration_seconds) VALUES (?1, ?2, ?3, ?4)",
        params![&session.file_name, &session.file_name, created_at, duration_seconds],
    ).map_err(|_| AppError::new("RECORDING_METADATA_FAILED", "The video was saved, but its project record could not be updated."))?;
    connection.execute("UPDATE project SET updated_at = ?1", params![now_ms().to_string()])
        .map_err(|_| AppError::new("PROJECT_DATABASE_ERROR", "The video was saved, but project metadata could not be updated."))?;
    Ok(RecordingSummary {
        id: session.file_name.clone(), file_name: session.file_name, created_at, duration_seconds,
    })
}

pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            get_app_info, check_dependencies, list_projects, get_active_project, create_project,
            open_project, list_recordings, get_recording_status, start_recording, stop_recording
        ])
        .run(tauri::generate_context!())
        .expect("error while running Screenstory");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn project_name_validation_trims_and_rejects_invalid_names() {
        assert_eq!(validate_project_name("  Demo  ").unwrap(), "Demo");
        assert!(validate_project_name("   ").is_err());
        assert!(validate_project_name("a\nb").is_err());
        assert!(validate_project_name(&"x".repeat(81)).is_err());
    }
    #[test]
    fn project_ids_cannot_escape_the_projects_root() {
        assert!(valid_project_id("p12345-456"));
        assert!(!valid_project_id("../outside"));
        assert!(!valid_project_id("p/../../outside"));
    }
    #[test]
    fn xrandr_display_dimensions_are_parsed() {
        assert_eq!(parse_xrandr_size("Screen 0: minimum 320 x 200, current 1920 x 1080, maximum 8192 x 8192"), Some((1920, 1080)));
        assert_eq!(parse_xrandr_size("Screen 0: current 0 x 0"), None);
        assert_eq!(parse_xrandr_size("unknown"), None);
    }
    #[test]
    fn dependency_report_is_feature_specific() {
        assert_eq!(dependency_report().dependencies.first().unwrap().id, "ffmpeg");
    }
}
