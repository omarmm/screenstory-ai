use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
mod analysis;
mod timeline;
mod render;
mod capture;

use std::{
    collections::HashMap,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{Manager, State};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppError {
    code: &'static str,
    message: String,
}
impl AppError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Default)]
struct AppState {
    active_project: Mutex<Option<PathBuf>>,
    recording: Mutex<Option<RecordingSession>>,
    analysis_cancellations: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    render_cancellations: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    render_processes: Arc<Mutex<HashMap<String, u32>>>,
    capture_sources: Mutex<HashMap<String, CaptureTarget>>,
}
struct RecordingSession {
    child: Child,
    stdin: ChildStdin,
    project_dir: PathBuf,
    output_file: PathBuf,
    file_name: String,
    started_at: Instant,
    started_at_ms: u64,
    capture_backend: String,
    capture_source_type: String,
    capture_source_name: String,
    window_id: Option<u32>,
    window_width: Option<u32>,
    window_height: Option<u32>,
}

#[derive(Debug, Clone)]
struct CaptureTarget {
    window_id: Option<u32>,
    name: String,
    source_type: String,
    backend: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CaptureSource {
    id: String,
    name: String,
    source_type: String,
    backend: String,
    width: u32,
    height: u32,
    is_available: bool,
    is_minimized: bool,
    app_hint: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    name: &'static str,
    version: &'static str,
    runtime: &'static str,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DependencyStatus {
    id: &'static str,
    feature: &'static str,
    status: &'static str,
    version: Option<String>,
    impact: &'static str,
    can_install: bool,
    remediation: Option<&'static str>,
}
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DependencyReport {
    dependencies: Vec<DependencyStatus>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ProjectSummary {
    id: String,
    name: String,
    created_at: String,
    updated_at: String,
    recordings_count: u64,
}
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct RecordingSummary {
    id: String,
    file_name: String,
    created_at: String,
    duration_seconds: f64,
    capture_backend: String,
    capture_source_type: String,
    capture_source_name: String,
    has_timeline: bool,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordingStatus {
    is_recording: bool,
    file_name: Option<String>,
    started_at_ms: Option<u64>,
    warning: Option<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordingStarted {
    file_name: String,
    started_at_ms: u64,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn projects_root() -> Result<PathBuf, AppError> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| AppError::new("HOME_UNAVAILABLE", "Could not locate your home folder."))?;
    Ok(home.join("Videos").join("Screenstory").join("Projects"))
}
fn db_connection(project_dir: &Path) -> Result<Connection, AppError> {
    let connection = Connection::open(project_dir.join("project.sqlite")).map_err(|_| {
        AppError::new(
            "PROJECT_DATABASE_ERROR",
            "Could not open this project database.",
        )
    })?;
    connection.execute_batch(
        "PRAGMA foreign_keys = ON;
         CREATE TABLE IF NOT EXISTS project (
             id TEXT PRIMARY KEY, name TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS recordings (
             id TEXT PRIMARY KEY, file_name TEXT NOT NULL UNIQUE, created_at TEXT NOT NULL,
             duration_seconds REAL NOT NULL DEFAULT 0,
             capture_backend TEXT NOT NULL DEFAULT 'x11',
             capture_source_type TEXT NOT NULL DEFAULT 'display',
             capture_source_name TEXT NOT NULL DEFAULT 'Entire desktop'
         );",
    ).map_err(|_| AppError::new("PROJECT_DATABASE_ERROR", "Could not initialize this project's database."))?;

    let columns = {
        let mut statement = connection.prepare("PRAGMA table_info(recordings)")
            .map_err(|_| AppError::new("PROJECT_DATABASE_ERROR", "Could not inspect recording metadata."))?;
        let rows = statement.query_map([], |row| row.get::<_, String>(1))
            .map_err(|_| AppError::new("PROJECT_DATABASE_ERROR", "Could not inspect recording metadata."))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::new("PROJECT_DATABASE_ERROR", "Could not inspect recording metadata."))?
    };
    for (column, declaration) in [
        ("capture_backend", "capture_backend TEXT NOT NULL DEFAULT 'x11'"),
        ("capture_source_type", "capture_source_type TEXT NOT NULL DEFAULT 'display'"),
        ("capture_source_name", "capture_source_name TEXT NOT NULL DEFAULT 'Entire desktop'"),
    ] {
        if !columns.iter().any(|existing| existing == column) {
            connection.execute_batch(&format!("ALTER TABLE recordings ADD COLUMN {declaration};"))
                .map_err(|_| AppError::new("PROJECT_DATABASE_MIGRATION_FAILED", "Could not add capture-source details to this project."))?;
        }
    }
    Ok(connection)
}
fn project_summary(connection: &Connection) -> Result<ProjectSummary, AppError> {
    let (id, name, created_at, updated_at): (String, String, String, String) = connection
        .query_row(
            "SELECT id, name, created_at, updated_at FROM project LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(|_| {
            AppError::new(
                "PROJECT_METADATA_INVALID",
                "This project is missing valid metadata.",
            )
        })?;
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM recordings", [], |row| row.get(0))
        .map_err(|_| {
            AppError::new(
                "PROJECT_DATABASE_ERROR",
                "Could not read this project's recordings.",
            )
        })?;
    Ok(ProjectSummary {
        id,
        name,
        created_at,
        updated_at,
        recordings_count: count.max(0) as u64,
    })
}
fn ensure_no_active_render(state: &AppState) -> Result<(), AppError> {
    let active = state.render_cancellations.lock().map_err(|_| {
        AppError::new("STATE_LOCK_ERROR", "Render state is temporarily unavailable.")
    })?;
    if !active.is_empty() {
        return Err(AppError::new(
            "RENDER_IN_PROGRESS",
            "Wait for the current export to finish or cancel it before switching projects.",
        ));
    }
    Ok(())
}
fn validate_project_name(name: &str) -> Result<String, AppError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::new(
            "INVALID_PROJECT_NAME",
            "Enter a name for this project.",
        ));
    }
    if trimmed.chars().count() > 80 || trimmed.chars().any(char::is_control) {
        return Err(AppError::new(
            "INVALID_PROJECT_NAME",
            "Project names must be 1–80 characters and cannot contain control characters.",
        ));
    }
    Ok(trimmed.to_string())
}
fn valid_project_id(id: &str) -> bool {
    id.starts_with('p')
        && id.len() > 1
        && id
            .chars()
            .all(|c| c == 'p' || c == '-' || c.is_ascii_digit())
}
fn resolve_project_dir(id: &str) -> Result<PathBuf, AppError> {
    if !valid_project_id(id) {
        return Err(AppError::new(
            "INVALID_PROJECT_ID",
            "That project identifier is not valid.",
        ));
    }
    let directory = projects_root()?.join(id);
    if !directory.join("project.sqlite").is_file() {
        return Err(AppError::new(
            "PROJECT_NOT_FOUND",
            "This project could not be found.",
        ));
    }
    let connection = db_connection(&directory)?;
    if project_summary(&connection)?.id != id {
        return Err(AppError::new(
            "PROJECT_ID_MISMATCH",
            "The project identifier does not match its metadata.",
        ));
    }
    Ok(directory)
}

fn apt_installer_available() -> bool {
    Path::new("/usr/bin/pkexec").is_file() && Path::new("/usr/bin/apt-get").is_file()
}

fn has_font_family(pattern: &str, expected: &str) -> bool {
    Command::new("fc-match")
        .args(["-f", "%{family}", pattern])
        .output()
        .map(|output| {
            output.status.success()
                && String::from_utf8_lossy(&output.stdout)
                    .to_lowercase()
                    .contains(&expected.to_lowercase())
        })
        .unwrap_or(false)
}

fn ffmpeg_status() -> DependencyStatus {
    match Command::new("ffmpeg").arg("-version").output() {
        Ok(output) if output.status.success() => DependencyStatus {
            id: "ffmpeg",
            feature: "Desktop recording and video processing",
            status: "available",
            version: Some(
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .next()
                    .unwrap_or("FFmpeg available")
                    .to_string(),
            ),
            impact: "Desktop recording and MP4 export are available.",
            can_install: false,
            remediation: None,
        },
        _ => DependencyStatus {
            id: "ffmpeg",
            feature: "FFmpeg · recording and video export",
            status: "missing",
            version: None,
            impact: "Recording and video export are unavailable until FFmpeg is installed.",
            can_install: apt_installer_available(),
            remediation: Some(if apt_installer_available() {
                "Screenstory can install FFmpeg using your system's package manager. You will be asked to approve it."
            } else {
                "Install FFmpeg with your Linux distribution's package manager, then recheck."
            }),
        },
    }
}

fn caption_fonts_status() -> DependencyStatus {
    let fontconfig_available = Command::new("fc-match").arg("--version").output()
        .map(|output| output.status.success()).unwrap_or(false);
    let regular_font = fontconfig_available && has_font_family("Noto Sans", "noto sans");
    let arabic_font = fontconfig_available && has_font_family("Noto Sans Arabic:lang=ar", "noto sans arabic");
    if regular_font && arabic_font {
        DependencyStatus {
            id: "caption-fonts",
            feature: "Noto Sans · English and Arabic captions",
            status: "available",
            version: Some("Noto Sans and Noto Sans Arabic".to_string()),
            impact: "English and Arabic caption fonts are available.",
            can_install: false,
            remediation: None,
        }
    } else {
        DependencyStatus {
            id: "caption-fonts",
            feature: "Noto Sans · English and Arabic captions",
            status: "missing",
            version: None,
            impact: "Caption rendering needs Fontconfig, Noto Sans, and Noto Sans Arabic.",
            can_install: apt_installer_available(),
            remediation: Some(if apt_installer_available() {
                "Screenstory can install Fontconfig and Noto fonts using your system's package manager. You will be asked to approve it."
            } else {
                "Install fontconfig and fonts-noto-core with your Linux distribution's package manager, then recheck."
            }),
        }
    }
}

fn dependency_report() -> DependencyReport {
    DependencyReport {
        dependencies: vec![ffmpeg_status(), caption_fonts_status()],
    }
}

fn install_dependency_blocking(dependency_id: &str) -> Result<DependencyReport, AppError> {
    let packages: &[&str] = match dependency_id {
        "ffmpeg" => &["ffmpeg"],
        "caption-fonts" => &["fontconfig", "fonts-noto-core"],
        _ => return Err(AppError::new(
            "DEPENDENCY_NOT_SUPPORTED",
            "Screenstory cannot install that dependency automatically.",
        )),
    };
    let current = dependency_report();
    if current.dependencies.iter().find(|item| item.id == dependency_id)
        .is_some_and(|item| item.status == "available")
    {
        return Ok(current);
    }
    if !apt_installer_available() {
        return Err(AppError::new(
            "SYSTEM_INSTALLER_UNAVAILABLE",
            "Automatic installation requires APT and the PolicyKit authorization service. Use your Software Manager, then recheck dependencies.",
        ));
    }

    let output = Command::new("/usr/bin/pkexec")
        .arg("/usr/bin/apt-get")
        .arg("install")
        .arg("-y")
        .args(packages)
        .output()
        .map_err(|_| AppError::new(
            "DEPENDENCY_INSTALL_FAILED",
            "Could not start the system package installer. Check that a graphical authentication agent is running.",
        ))?;

    if !output.status.success() {
        if output.status.code() == Some(126) || output.status.code() == Some(127) {
            return Err(AppError::new(
                "DEPENDENCY_INSTALL_CANCELLED",
                "System installation was cancelled or not authorized. Approve the system authentication prompt to continue.",
            ));
        }
        let detail = String::from_utf8_lossy(&output.stderr);
        let detail = detail.lines().rev().find(|line| !line.trim().is_empty())
            .unwrap_or("The system package manager reported an error.");
        return Err(AppError::new(
            "DEPENDENCY_INSTALL_FAILED",
            format!("The system package manager could not install the required dependency: {detail}"),
        ));
    }

    let updated = dependency_report();
    if updated.dependencies.iter().find(|item| item.id == dependency_id)
        .is_some_and(|item| item.status == "available")
    {
        Ok(updated)
    } else {
        Err(AppError::new(
            "DEPENDENCY_INSTALL_UNVERIFIED",
            "The package manager finished, but Screenstory could not verify the dependency. Recheck the system packages and try again.",
        ))
    }
}

#[tauri::command]
async fn install_dependency(dependency_id: String) -> Result<DependencyReport, AppError> {
    tauri::async_runtime::spawn_blocking(move || install_dependency_blocking(&dependency_id))
        .await
        .map_err(|_| AppError::new(
            "DEPENDENCY_INSTALL_FAILED",
            "The system dependency installation task could not be completed.",
        ))?
}

#[cfg(test)]
fn parse_xrandr_size(output: &str) -> Option<(u32, u32)> {
    let dimensions = output.split("current ").nth(1)?.split(',').next()?;
    let mut parts = dimensions.split_whitespace();
    let width = parts.next()?.parse().ok()?;
    if parts.next()? != "x" {
        return None;
    }
    let height = parts.next()?.parse().ok()?;
    if width == 0 || height == 0 {
        return None;
    }
    Some((width, height))
}
#[tauri::command]
fn get_app_info() -> AppInfo {
    AppInfo {
        name: "Screenstory",
        version: env!("CARGO_PKG_VERSION"),
        runtime: "Tauri",
    }
}
#[tauri::command]
fn check_dependencies() -> DependencyReport {
    dependency_report()
}

#[tauri::command]
fn list_projects() -> Result<Vec<ProjectSummary>, AppError> {
    let root = projects_root()?;
    fs::create_dir_all(&root).map_err(|_| {
        AppError::new(
            "PROJECTS_FOLDER_ERROR",
            "Could not create the Screenstory projects folder.",
        )
    })?;
    let entries = fs::read_dir(root).map_err(|_| {
        AppError::new(
            "PROJECTS_FOLDER_ERROR",
            "Could not read the Screenstory projects folder.",
        )
    })?;
    let mut projects = Vec::new();
    for entry in entries.flatten() {
        if !entry.path().join("project.sqlite").is_file() {
            continue;
        }
        if let Ok(connection) = db_connection(&entry.path()) {
            if let Ok(summary) = project_summary(&connection) {
                projects.push(summary);
            }
        }
    }
    projects.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(projects)
}
#[tauri::command]
fn get_active_project(state: State<'_, AppState>) -> Result<Option<ProjectSummary>, AppError> {
    let active = state.active_project.lock().map_err(|_| {
        AppError::new(
            "STATE_LOCK_ERROR",
            "Project state is temporarily unavailable.",
        )
    })?;
    match active.as_ref() {
        Some(directory) => Ok(Some(project_summary(&db_connection(directory)?)?)),
        None => Ok(None),
    }
}
#[tauri::command]
fn create_project(name: String, state: State<'_, AppState>) -> Result<ProjectSummary, AppError> {
    let recording = state.recording.lock().map_err(|_| {
        AppError::new(
            "STATE_LOCK_ERROR",
            "Recording state is temporarily unavailable.",
        )
    })?;
    if recording.is_some() {
        return Err(AppError::new(
            "RECORDING_IN_PROGRESS",
            "Stop the current recording before creating another project.",
        ));
    }
    drop(recording);
    ensure_no_active_render(&state)?;
    let name = validate_project_name(&name)?;
    let root = projects_root()?;
    fs::create_dir_all(&root).map_err(|_| {
        AppError::new(
            "PROJECTS_FOLDER_ERROR",
            "Could not create the Screenstory projects folder.",
        )
    })?;
    let mut created = None;
    let mut project_id = String::new();
    for attempt in 0..5_u64 {
        project_id = format!("p{}-{}", now_ms() + attempt, std::process::id());
        let candidate = root.join(&project_id);
        match fs::create_dir(&candidate) {
            Ok(()) => {
                created = Some(candidate);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => {
                return Err(AppError::new(
                    "PROJECT_CREATE_FAILED",
                    "Could not create the project folder.",
                ))
            }
        }
    }
    let directory = created.ok_or_else(|| {
        AppError::new(
            "PROJECT_CREATE_FAILED",
            "Could not allocate a unique project folder. Try again.",
        )
    })?;
    let result = (|| {
        for folder in ["recordings", "analysis", "timeline", "assets", "exports"] {
            fs::create_dir(directory.join(folder)).map_err(|_| {
                AppError::new(
                    "PROJECT_CREATE_FAILED",
                    "Could not initialize the project folders.",
                )
            })?;
        }
        let connection = db_connection(&directory)?;
        let timestamp = now_ms().to_string();
        connection
            .execute(
                "INSERT INTO project (id, name, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
                params![project_id, name, timestamp],
            )
            .map_err(|_| {
                AppError::new(
                    "PROJECT_CREATE_FAILED",
                    "Could not save the new project's metadata.",
                )
            })?;
        project_summary(&connection)
    })();
    match result {
        Ok(summary) => {
            *state.active_project.lock().map_err(|_| {
                AppError::new(
                    "STATE_LOCK_ERROR",
                    "Project state is temporarily unavailable.",
                )
            })? = Some(directory);
            Ok(summary)
        }
        Err(error) => {
            let _ = fs::remove_dir_all(&directory);
            Err(error)
        }
    }
}
#[tauri::command]
fn open_project(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<ProjectSummary, AppError> {
    let recording = state.recording.lock().map_err(|_| {
        AppError::new(
            "STATE_LOCK_ERROR",
            "Recording state is temporarily unavailable.",
        )
    })?;
    if recording.is_some() {
        return Err(AppError::new(
            "RECORDING_IN_PROGRESS",
            "Stop the current recording before switching projects.",
        ));
    }
    drop(recording);
    ensure_no_active_render(&state)?;
    let directory = resolve_project_dir(&project_id)?;
    render::recover_interrupted_renders(&directory)?;
    let summary = project_summary(&db_connection(&directory)?)?;
    *state.active_project.lock().map_err(|_| {
        AppError::new(
            "STATE_LOCK_ERROR",
            "Project state is temporarily unavailable.",
        )
    })? = Some(directory);
    Ok(summary)
}
#[tauri::command]
fn list_capture_sources(state: State<'_, AppState>) -> Result<Vec<CaptureSource>, AppError> {
    let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default().to_ascii_lowercase();
    if session != "x11" {
        state.capture_sources.lock().map_err(|_| AppError::new("STATE_LOCK_ERROR", "Capture-source state is unavailable."))?.clear();
        return Ok(vec![CaptureSource {
            id: "unsupported-session".into(),
            name: if session == "wayland" { "Wayland desktop capture" } else { "Unsupported desktop session" }.into(),
            source_type: "display".into(),
            backend: if session == "wayland" { "portal-pipewire" } else { "unsupported" }.into(),
            width: 0,
            height: 0,
            is_available: false,
            is_minimized: false,
            app_hint: None,
        }]);
    }
    let display = capture::display_dimensions()
        .map_err(|message| AppError::new("X11_DISPLAY_UNAVAILABLE", message))?;
    let windows = capture::list_x11_windows()
        .map_err(|message| AppError::new("X11_WINDOW_ENUMERATION_FAILED", message))?;
    let session_id = format!("c{}-{}", now_ms(), std::process::id());
    let display_id = format!("{session_id}-display");
    let mut registry = HashMap::new();
    registry.insert(display_id.clone(), CaptureTarget {
        window_id: None,
        name: "Entire desktop".into(),
        source_type: "display".into(),
        backend: "x11".into(),
    });
    let mut result = vec![CaptureSource {
        id: display_id,
        name: format!("Entire desktop · {} × {}", display.width, display.height),
        source_type: "display".into(),
        backend: "x11".into(),
        width: display.width,
        height: display.height,
        is_available: true,
        is_minimized: false,
        app_hint: None,
    }];
    for window in windows {
        let id = format!("{session_id}-window-{:x}", window.window_id);
        let source_name = window.title.clone();
        registry.insert(id.clone(), CaptureTarget {
            window_id: Some(window.window_id),
            name: source_name.clone(),
            source_type: "window".into(),
            backend: "x11".into(),
        });
        result.push(CaptureSource {
            id,
            name: source_name,
            source_type: "window".into(),
            backend: "x11".into(),
            width: window.width,
            height: window.height,
            is_available: window.is_visible && !window.is_minimized,
            is_minimized: window.is_minimized,
            app_hint: window.app_hint,
        });
    }
    *state.capture_sources.lock().map_err(|_| AppError::new("STATE_LOCK_ERROR", "Capture-source state is unavailable."))? = registry;
    Ok(result)
}

#[tauri::command]
fn list_recordings(state: State<'_, AppState>) -> Result<Vec<RecordingSummary>, AppError> {
    let active = state.active_project.lock().map_err(|_| {
        AppError::new(
            "STATE_LOCK_ERROR",
            "Project state is temporarily unavailable.",
        )
    })?;
    let directory = active.as_ref().ok_or_else(|| {
        AppError::new(
            "NO_ACTIVE_PROJECT",
            "Create or open a project before viewing recordings.",
        )
    })?;
    timeline::ensure_timeline_table(directory)?;
    let connection = db_connection(directory)?;
    let mut statement = connection.prepare(
        "SELECT id, file_name, created_at, duration_seconds, capture_backend, capture_source_type, capture_source_name,
                EXISTS(SELECT 1 FROM timelines WHERE timelines.recording_id = recordings.id)
         FROM recordings ORDER BY created_at DESC"
    ).map_err(|_| AppError::new("PROJECT_DATABASE_ERROR", "Could not read this project's recordings."))?;
    let rows = statement
        .query_map([], |row| {
            Ok(RecordingSummary {
                id: row.get(0)?,
                file_name: row.get(1)?,
                created_at: row.get(2)?,
                duration_seconds: row.get(3)?,
                capture_backend: row.get(4)?,
                capture_source_type: row.get(5)?,
                capture_source_name: row.get(6)?,
                has_timeline: row.get(7)?,
            })
        })
        .map_err(|_| AppError::new(
            "PROJECT_DATABASE_ERROR",
            "Could not read this project's recordings.",
        ))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|_| AppError::new(
        "PROJECT_DATABASE_ERROR",
        "Could not read this project's recordings.",
    ))
}

fn valid_render_run_id(value: &str) -> bool {
    value.starts_with('r')
        && value.len() > 2
        && value.chars().all(|character| character == 'r' || character == '-' || character.is_ascii_digit())
}

fn remove_managed_directory(root: &Path, child_name: &str, code: &'static str, message: &'static str) -> Result<(), AppError> {
    if child_name.is_empty()
        || child_name == "."
        || child_name == ".."
        || Path::new(child_name).components().count() != 1
        || Path::new(child_name).file_name().and_then(|name| name.to_str()) != Some(child_name)
    {
        return Err(AppError::new("DELETE_PATH_INVALID", "A managed folder identifier is invalid."));
    }
    let parent = root.parent()
        .ok_or_else(|| AppError::new("DELETE_PATH_INVALID", "The managed folder has no project parent."))?
        .canonicalize()
        .map_err(|_| AppError::new(code, message))?;
    let root = match root.canonicalize() {
        Ok(root) => root,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(AppError::new(code, message)),
    };
    if !root.starts_with(&parent) || root == parent {
        return Err(AppError::new("DELETE_PATH_INVALID", "The managed folder resolved outside the project."));
    }
    let target = root.join(child_name);
    let metadata = match fs::symlink_metadata(&target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(AppError::new(code, message)),
    };
    if metadata.file_type().is_symlink() {
        return fs::remove_file(&target).map_err(|_| AppError::new(code, message));
    }
    let canonical = target.canonicalize().map_err(|_| AppError::new(code, message))?;
    if !canonical.starts_with(&root) || canonical == root {
        return Err(AppError::new("DELETE_PATH_INVALID", "A managed folder resolved outside its project."));
    }
    fs::remove_dir_all(canonical).map_err(|_| AppError::new(code, message))
}

fn remove_managed_export(exports_root: &Path, stored_path: &str) -> Result<(), AppError> {
    let parent = exports_root.parent()
        .ok_or_else(|| AppError::new("DELETE_PATH_INVALID", "The exports folder has no project parent."))?
        .canonicalize()
        .map_err(|_| AppError::new("RECORDING_DELETE_FAILED", "The recording's export folder could not be inspected."))?;
    let root = match exports_root.canonicalize() {
        Ok(root) => root,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(AppError::new("RECORDING_DELETE_FAILED", "The recording's export folder could not be inspected.")),
    };
    if !root.starts_with(&parent) || root == parent {
        return Err(AppError::new("DELETE_PATH_INVALID", "The exports folder resolved outside the project."));
    }
    let candidate = PathBuf::from(stored_path);
    let metadata = match fs::symlink_metadata(&candidate) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(AppError::new("RECORDING_DELETE_FAILED", "A generated export could not be inspected.")),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AppError::new("DELETE_PATH_INVALID", "A generated export is not a regular managed file."));
    }
    let canonical = candidate.canonicalize().map_err(|_| AppError::new(
        "RECORDING_DELETE_FAILED",
        "A generated export could not be inspected.",
    ))?;
    if !canonical.starts_with(&root) || canonical == root {
        return Err(AppError::new("DELETE_PATH_INVALID", "A generated export resolved outside this project's exports folder."));
    }
    fs::remove_file(canonical).map_err(|_| AppError::new(
        "RECORDING_DELETE_FAILED",
        "A generated export could not be removed. Check its file permissions and retry.",
    ))
}

#[tauri::command]
fn delete_recording(recording_id: String, state: State<'_, AppState>) -> Result<(), AppError> {
    let project_dir = state.active_project.lock()
        .map_err(|_| AppError::new("STATE_LOCK_ERROR", "Project state is temporarily unavailable."))?
        .as_ref().cloned()
        .ok_or_else(|| AppError::new("NO_ACTIVE_PROJECT", "Open a project before deleting a recording."))?;

    if let Some(session) = state.recording.lock()
        .map_err(|_| AppError::new("STATE_LOCK_ERROR", "Recording state is temporarily unavailable."))?
        .as_ref()
    {
        if session.project_dir == project_dir && session.file_name == recording_id {
            return Err(AppError::new("RECORDING_IN_PROGRESS", "Stop and save this recording before deleting it."));
        }
    }
    ensure_no_active_render(&state)?;
    analysis::ensure_analysis_table(&project_dir)?;
    timeline::ensure_timeline_table(&project_dir)?;
    render::ensure_render_table(&project_dir)?;

    let (_, recording_path) = analysis::resolve_recording(&project_dir, &recording_id)?;
    let connection = db_connection(&project_dir)?;
    let active_analyses: i64 = connection.query_row(
        "SELECT COUNT(*) FROM analysis_runs
         WHERE recording_id=?1 AND status IN ('extracting_frames','awaiting_cloud_consent','sending_to_provider','validating_output')",
        params![recording_id],
        |row| row.get(0),
    ).map_err(|_| AppError::new("ANALYSIS_DATABASE_ERROR", "The recording's analysis activity could not be checked."))?;
    if active_analyses > 0 {
        return Err(AppError::new("ANALYSIS_IN_PROGRESS", "Cancel or finish this recording's AI analysis before deleting it."));
    }
    let active_renders: i64 = connection.query_row(
        "SELECT COUNT(*) FROM render_runs
         WHERE recording_id=?1 AND status IN ('preparing','rendering','validating','cancelling')",
        params![recording_id],
        |row| row.get(0),
    ).map_err(|_| AppError::new("RENDER_DATABASE_ERROR", "The recording's export activity could not be checked."))?;
    if active_renders > 0 {
        return Err(AppError::new("RENDER_IN_PROGRESS", "Wait for this recording's export to finish before deleting it."));
    }

    let analysis_ids = {
        let mut statement = connection.prepare("SELECT id FROM analysis_runs WHERE recording_id=?1")
            .map_err(|_| AppError::new("ANALYSIS_DATABASE_ERROR", "The recording's analysis history could not be inspected."))?;
        let rows = statement.query_map(params![recording_id], |row| row.get::<_, String>(0))
            .map_err(|_| AppError::new("ANALYSIS_DATABASE_ERROR", "The recording's analysis history could not be inspected."))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::new("ANALYSIS_DATABASE_ERROR", "The recording's analysis history could not be read."))?
    };
    let timeline_id: Option<String> = connection.query_row(
        "SELECT timeline_id FROM timelines WHERE recording_id=?1",
        params![recording_id],
        |row| row.get(0),
    ).optional().map_err(|_| AppError::new("TIMELINE_DATABASE_ERROR", "The recording's timeline could not be inspected."))?;
    let render_rows = {
        let mut statement = connection.prepare("SELECT id, output_path FROM render_runs WHERE recording_id=?1")
            .map_err(|_| AppError::new("RENDER_DATABASE_ERROR", "The recording's export history could not be inspected."))?;
        let rows = statement.query_map(params![recording_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
            .map_err(|_| AppError::new("RENDER_DATABASE_ERROR", "The recording's export history could not be inspected."))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::new("RENDER_DATABASE_ERROR", "The recording's export history could not be read."))?
    };

    for analysis_id in &analysis_ids {
        if analysis::analysis_run_id_valid(analysis_id) {
            remove_managed_directory(&project_dir.join("analysis"), analysis_id,
                "RECORDING_DELETE_FAILED", "An analysis folder could not be removed.")?;
        }
    }
    if let Some(id) = timeline_id.as_deref() {
        if timeline::valid_timeline_id(id) {
            remove_managed_directory(&project_dir.join("timeline"), id,
                "RECORDING_DELETE_FAILED", "The editable timeline folder could not be removed.")?;
        }
    }
    let exports_root = project_dir.join("exports");
    for (render_id, output_path) in &render_rows {
        if valid_render_run_id(render_id) {
            remove_managed_export(&exports_root, output_path)?;
            let _ = fs::remove_file(exports_root.join(format!("{render_id}.partial.mp4")));
            if exports_root.join(".render-cache").is_dir() {
                remove_managed_directory(&exports_root.join(".render-cache"), render_id,
                    "RECORDING_DELETE_FAILED", "The render cache could not be removed.")?;
            }
        }
    }

    fs::remove_file(&recording_path).map_err(|_| AppError::new(
        "RECORDING_DELETE_FAILED",
        "The original recording could not be deleted. Check file permissions and retry.",
    ))?;

    let mut connection = db_connection(&project_dir)?;
    let transaction = connection.transaction()
        .map_err(|_| AppError::new("PROJECT_DATABASE_ERROR", "The project could not acquire the delete lock."))?;
    transaction.execute("DELETE FROM render_runs WHERE recording_id=?1", params![recording_id])
        .map_err(|_| AppError::new("RENDER_DATABASE_ERROR", "The recording's export history could not be removed."))?;
    transaction.execute("DELETE FROM timelines WHERE recording_id=?1", params![recording_id])
        .map_err(|_| AppError::new("TIMELINE_DATABASE_ERROR", "The recording's timeline index could not be removed."))?;
    transaction.execute("DELETE FROM analysis_runs WHERE recording_id=?1", params![recording_id])
        .map_err(|_| AppError::new("ANALYSIS_DATABASE_ERROR", "The recording's analysis history could not be removed."))?;
    transaction.execute("DELETE FROM recordings WHERE id=?1", params![recording_id])
        .map_err(|_| AppError::new("PROJECT_DATABASE_ERROR", "The recording could not be removed from the project library."))?;
    transaction.execute("UPDATE project SET updated_at=?1", params![now_ms().to_string()])
        .map_err(|_| AppError::new("PROJECT_DATABASE_ERROR", "The project timestamp could not be updated."))?;
    transaction.commit().map_err(|_| AppError::new("PROJECT_DATABASE_ERROR", "The recording deletion could not be committed."))?;
    if let Ok(mut active_analyses) = state.analysis_cancellations.lock() {
        for analysis_id in &analysis_ids {
            active_analyses.remove(analysis_id);
        }
    }

    Ok(())
}

#[tauri::command]
fn delete_project(project_id: String, state: State<'_, AppState>) -> Result<(), AppError> {
    if !valid_project_id(&project_id) {
        return Err(AppError::new("INVALID_PROJECT_ID", "That project identifier is not valid."));
    }
    if state.recording.lock()
        .map_err(|_| AppError::new("STATE_LOCK_ERROR", "Recording state is temporarily unavailable."))?
        .is_some()
    {
        return Err(AppError::new("RECORDING_IN_PROGRESS", "Stop and save the active recording before deleting a project."));
    }
    ensure_no_active_render(&state)?;

    let project_dir = resolve_project_dir(&project_id)?;
    analysis::ensure_analysis_table(&project_dir)?;
    let connection = db_connection(&project_dir)?;
    let active_analyses: i64 = connection.query_row(
        "SELECT COUNT(*) FROM analysis_runs
         WHERE status IN ('extracting_frames','awaiting_cloud_consent','sending_to_provider','validating_output')",
        [],
        |row| row.get(0),
    ).map_err(|_| AppError::new("ANALYSIS_DATABASE_ERROR", "The project's AI analysis activity could not be checked."))?;
    if active_analyses > 0 {
        return Err(AppError::new("ANALYSIS_IN_PROGRESS", "Cancel or finish AI analysis before deleting this project."));
    }

    let root = projects_root()?.canonicalize()
        .map_err(|_| AppError::new("PROJECTS_FOLDER_ERROR", "The local projects folder is unavailable."))?;
    let target = project_dir.canonicalize()
        .map_err(|_| AppError::new("PROJECT_NOT_FOUND", "This project folder could not be found."))?;
    if !target.starts_with(&root) || target == root {
        return Err(AppError::new("PROJECT_PATH_INVALID", "The project folder resolved outside the managed projects directory."));
    }
    fs::remove_dir_all(&target).map_err(|_| AppError::new(
        "PROJECT_DELETE_FAILED",
        "The project could not be deleted. Check folder permissions and retry.",
    ))?;

    let mut active = state.active_project.lock()
        .map_err(|_| AppError::new("STATE_LOCK_ERROR", "Project state is temporarily unavailable."))?;
    if active.as_ref().is_some_and(|directory| directory == &project_dir) {
        *active = None;
        if let Ok(mut sources) = state.capture_sources.lock() {
            sources.clear();
        }
        if let Ok(mut active_analyses) = state.analysis_cancellations.lock() {
            active_analyses.clear();
        }
    }
    Ok(())
}
#[tauri::command]
fn get_recording_status(state: State<'_, AppState>) -> Result<RecordingStatus, AppError> {
    let mut recording = state.recording.lock().map_err(|_| {
        AppError::new("STATE_LOCK_ERROR", "Recording state is temporarily unavailable.")
    })?;
    let warning = if let Some(session) = recording.as_ref() {
        match session.window_id {
            Some(window_id) => {
                let current = capture::get_window(window_id).ok().flatten();
                match current {
                    Some(window) if window.is_visible && !window.is_minimized
                        && Some(window.width) == session.window_width
                        && Some(window.height) == session.window_height => None,
                    _ => Some(format!(
                        "The selected window “{}” was closed, minimized, or resized. Screenstory stopped recording and saved the captured portion.",
                        session.capture_source_name
                    )),
                }
            }
            None => None,
        }
    } else {
        None
    };
    if let Some(message) = warning {
        let session = recording.take().ok_or_else(|| AppError::new("RECORDING_STATE_ERROR", "The recording session could not be finalized."))?;
        drop(recording);
        let _ = finalize_recording_session(session)?;
        return Ok(RecordingStatus {
            is_recording: false,
            file_name: None,
            started_at_ms: None,
            warning: Some(message),
        });
    }
    Ok(match recording.as_ref() {
        Some(session) => RecordingStatus {
            is_recording: true,
            file_name: Some(session.file_name.clone()),
            started_at_ms: Some(session.started_at_ms),
            warning: None,
        },
        None => RecordingStatus {
            is_recording: false,
            file_name: None,
            started_at_ms: None,
            warning: None,
        },
    })
}

#[tauri::command]
fn start_recording(capture_source_id: String, state: State<'_, AppState>) -> Result<RecordingStarted, AppError> {
    let active = state.active_project.lock().map_err(|_| AppError::new("STATE_LOCK_ERROR", "Project state is temporarily unavailable."))?;
    let project_dir = active.as_ref().cloned().ok_or_else(|| AppError::new("NO_ACTIVE_PROJECT", "Create or open a project before recording."))?;
    drop(active);

    let mut current = state.recording.lock().map_err(|_| AppError::new("STATE_LOCK_ERROR", "Recording state is temporarily unavailable."))?;
    if current.is_some() {
        return Err(AppError::new("RECORDING_ALREADY_ACTIVE", "A recording is already in progress."));
    }
    let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default().to_ascii_lowercase();
    if session != "x11" {
        return Err(AppError::new("UNSUPPORTED_CAPTURE_SESSION", "This build currently supports capture in X11 sessions. Wayland recording will be enabled after portal/PipeWire end-to-end verification."));
    }
    let display = std::env::var("DISPLAY").map_err(|_| AppError::new("DISPLAY_UNAVAILABLE", "The X11 display is unavailable. Launch Screenstory from your desktop session."))?;
    let target = state.capture_sources.lock().map_err(|_| AppError::new("STATE_LOCK_ERROR", "Capture-source state is temporarily unavailable."))?
        .get(&capture_source_id).cloned()
        .ok_or_else(|| AppError::new("STALE_CAPTURE_SOURCE", "The selected capture source expired. Refresh the source list and try again."))?;
    if target.backend != "x11" {
        return Err(AppError::new("CAPTURE_BACKEND_UNAVAILABLE", "The selected capture backend is not available in this desktop session."));
    }
    let (window_id, width, height) = match target.window_id {
        Some(window_id) => {
            let window = capture::get_window(window_id)
                .map_err(|message| AppError::new("X11_WINDOW_UNAVAILABLE", message))?
                .filter(|window| window.is_visible && !window.is_minimized)
                .ok_or_else(|| AppError::new("CAPTURE_WINDOW_NOT_VISIBLE", "The selected window is no longer visible. Restore it, refresh the source list, and try again."))?;
            (Some(window_id), window.width, window.height)
        }
        None => {
            let display = capture::display_dimensions().map_err(|message| AppError::new("X11_DISPLAY_UNAVAILABLE", message))?;
            (None, display.width, display.height)
        }
    };
    if width < 80 || height < 50 || width > 7680 || height > 4320 {
        return Err(AppError::new("CAPTURE_DIMENSIONS_UNSUPPORTED", "The selected capture source dimensions are outside the supported range."));
    }
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

    let mut command = Command::new("ffmpeg");
    command.args([
        "-hide_banner", "-loglevel", "warning",
        "-f", "x11grab", "-framerate", "30", "-draw_mouse", "1",
    ]);
    if let Some(window_id) = window_id {
        command.args(["-window_id", &format!("0x{window_id:x}")]);
    }
    command.args(["-video_size"]).arg(format!("{width}x{height}"))
        .arg("-i").arg(&display)
        .args([
            "-an", "-vf", "pad=ceil(iw/2)*2:ceil(ih/2)*2",
            "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p",
            "-movflags", "+faststart", "-y",
        ])
        .arg(&output_file)
        .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::from(log));
    let mut child = command.spawn().map_err(|_| AppError::new("RECORDING_START_FAILED", "FFmpeg could not be started. Check that FFmpeg is installed correctly."))?;
    let stdin = child.stdin.take().ok_or_else(|| AppError::new("RECORDING_START_FAILED", "Could not initialize the recording process."))?;
    if let Some(status) = child.try_wait().map_err(|_| AppError::new("RECORDING_START_FAILED", "Could not verify that recording started."))? {
        return Err(AppError::new("RECORDING_START_FAILED", format!("FFmpeg stopped immediately (exit status {}). Check the recording log in this project.", status)));
    }
    let started_at_ms = now_ms();
    let capture_source_name = target.name.clone();
    let capture_source_type = target.source_type.clone();
    let capture_backend = target.backend.clone();
    *current = Some(RecordingSession {
        child, stdin, project_dir, output_file, file_name: file_name.clone(),
        started_at: Instant::now(), started_at_ms,
        capture_backend, capture_source_type, capture_source_name,
        window_id, window_width: window_id.map(|_| width), window_height: window_id.map(|_| height),
    });
    Ok(RecordingStarted { file_name, started_at_ms })
}

fn finalize_recording_session(mut session: RecordingSession) -> Result<RecordingSummary, AppError> {
    let _ = session.stdin.write_all(b"q\n");
    let _ = session.stdin.flush();
    let status = session.child.wait().map_err(|_| AppError::new("RECORDING_FINALIZE_FAILED", "Could not finalize the recording. Check the recording log."))?;
    let duration_seconds = session.started_at.elapsed().as_secs_f64();
    if !status.success() || !session.output_file.is_file()
        || fs::metadata(&session.output_file).map(|meta| meta.len()).unwrap_or(0) == 0 {
        return Err(AppError::new("RECORDING_FAILED", format!("FFmpeg could not finalize {}. The project recording log may contain more details.", session.file_name)));
    }
    let created_at = session.started_at_ms.to_string();
    let connection = db_connection(&session.project_dir)?;
    connection.execute(
        "INSERT INTO recordings (id, file_name, created_at, duration_seconds, capture_backend, capture_source_type, capture_source_name) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![&session.file_name, &session.file_name, created_at, duration_seconds, &session.capture_backend, &session.capture_source_type, &session.capture_source_name],
    ).map_err(|_| AppError::new("RECORDING_METADATA_FAILED", "The video was saved, but its project record could not be updated."))?;
    connection.execute("UPDATE project SET updated_at = ?1", params![now_ms().to_string()])
        .map_err(|_| AppError::new("PROJECT_DATABASE_ERROR", "The video was saved, but project metadata could not be updated."))?;
    Ok(RecordingSummary {
        id: session.file_name.clone(),
        file_name: session.file_name,
        created_at,
        duration_seconds,
        capture_backend: session.capture_backend,
        capture_source_type: session.capture_source_type,
        capture_source_name: session.capture_source_name,
        has_timeline: false,
    })
}

#[tauri::command]
fn stop_recording(state: State<'_, AppState>) -> Result<RecordingSummary, AppError> {
    let session = state.recording.lock().map_err(|_| AppError::new("STATE_LOCK_ERROR", "Recording state is temporarily unavailable."))?
        .take().ok_or_else(|| AppError::new("NO_ACTIVE_RECORDING", "There is no active recording to stop."))?;
    finalize_recording_session(session)
}

pub fn run() {
    let app = tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            check_dependencies,
            install_dependency,
            list_projects,
            get_active_project,
            create_project,
            open_project,
            list_recordings,
            delete_recording,
            delete_project,
            list_capture_sources,
            get_recording_status,
            start_recording,
            stop_recording,
            analysis::get_gemini_key_status,
            analysis::save_gemini_api_key,
            analysis::delete_gemini_api_key,
            analysis::start_analysis,
            analysis::get_analysis_status,
            analysis::cancel_analysis,
            analysis::send_analysis_to_gemini,
            analysis::get_analysis_draft,
            timeline::create_timeline_from_draft,
            timeline::get_timeline_for_recording,
            timeline::save_timeline,
            render::start_render,
            render::get_render_status,
            render::list_render_history,
            render::cancel_render
        ])
        .build(tauri::generate_context!())
        .expect("error while building Screenstory");

    app.run(|handle, event| {
        if let tauri::RunEvent::Exit = event {
            let state = handle.state::<AppState>();
            render::prepare_for_app_exit(&state);
            if let Ok(mut recording) = state.recording.lock() {
                if let Some(mut session) = recording.take() {
                    let _ = session.stdin.write_all(b"q\n");
                    let _ = session.stdin.flush();
                    let _ = session.child.wait();
                }
            };
        }
    });
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
        assert_eq!(
            parse_xrandr_size(
                "Screen 0: minimum 320 x 200, current 1920 x 1080, maximum 8192 x 8192"
            ),
            Some((1920, 1080))
        );
        assert_eq!(parse_xrandr_size("Screen 0: current 0 x 0"), None);
        assert_eq!(parse_xrandr_size("unknown"), None);
    }
    #[test]
    fn dependency_report_is_feature_specific() {
        assert_eq!(
            dependency_report().dependencies.first().unwrap().id,
            "ffmpeg"
        );
    }
}
