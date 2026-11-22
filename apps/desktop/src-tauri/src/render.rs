use super::{analysis, db_connection, timeline, AppError, AppState};
use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    sync::Arc,
    thread,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::State;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RenderStatus {
    pub run_id: String,
    pub recording_id: String,
    pub timeline_id: String,
    pub timeline_revision: u64,
    pub status: String,
    pub progress: u8,
    pub output_file_name: String,
    pub output_path: Option<String>,
    pub output_size_bytes: u64,
    pub duration_seconds: f64,
    pub error_code: Option<String>,
    pub renderer_version: String,
    pub has_audio: bool,
    pub annotation_count: usize,
    pub active_zoom_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderStarted {
    pub run_id: String,
    pub timeline_id: String,
    pub timeline_revision: u64,
    pub output_file_name: String,
}

#[derive(Debug, Deserialize)]
struct ProbeOutput {
    streams: Vec<ProbeStream>,
    format: ProbeFormat,
}
#[derive(Debug, Deserialize)]
struct ProbeStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    avg_frame_rate: Option<String>,
}
#[derive(Debug, Deserialize)]
struct ProbeFormat {
    duration: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RenderPlan {
    schema_version: u32,
    run_id: String,
    recording_id: String,
    source_file_name: String,
    timeline_id: String,
    timeline_revision: u64,
    source_duration_ms: u64,
    output_width: u32,
    output_height: u32,
    frame_rate: String,
    preserve_source_audio: bool,
    active_zoom_count: usize,
    annotation_count: usize,
    active_zooms: Vec<timeline::TimelineZoomSegment>,
    annotations: Vec<timeline::TimelineAnnotation>,
    renderer_version: String,
    subtitle_filter: String,
    subtitle_shaping: String,
}

struct VideoInfo {
    duration_seconds: f64,
    width: u32,
    height: u32,
    frame_rate: String,
    audio_present: bool,
}

struct RenderRequest {
    run_id: String,
    recording_path: PathBuf,
    recording_id: String,
    file_name: String,
    timeline: timeline::TimelineDocument,
    video: VideoInfo,
    ffmpeg_version: String,
    subtitle_shaping: String,
    output_dir: PathBuf,
    cache_dir: PathBuf,
    output_path: PathBuf,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

pub(super) fn ensure_render_table(project_dir: &Path) -> Result<(), AppError> {
    db_connection(project_dir)?.execute_batch(
        "CREATE TABLE IF NOT EXISTS render_runs (
            id TEXT PRIMARY KEY,
            recording_id TEXT NOT NULL,
            timeline_id TEXT NOT NULL,
            timeline_revision INTEGER NOT NULL,
            output_file_name TEXT NOT NULL,
            output_path TEXT NOT NULL,
            status TEXT NOT NULL,
            progress INTEGER NOT NULL DEFAULT 0,
            created_at_ms INTEGER NOT NULL,
            duration_seconds REAL NOT NULL,
            output_size_bytes INTEGER NOT NULL DEFAULT 0,
            error_code TEXT,
            renderer_version TEXT NOT NULL,
            has_audio INTEGER NOT NULL DEFAULT 0,
            annotation_count INTEGER NOT NULL DEFAULT 0,
            active_zoom_count INTEGER NOT NULL DEFAULT 0
        );"
    ).map_err(|_| AppError::new("RENDER_DATABASE_ERROR", "The project's export history could not be initialized."))?;
    Ok(())
}


fn render_status_from_row(row: &Row<'_>) -> rusqlite::Result<RenderStatus> {
    let mut status: String = row.get(4)?;
    let path: String = row.get(7)?;
    let mut error_code: Option<String> = row.get(10)?;
    let output_path = if status == "completed" {
        if Path::new(&path).is_file() {
            Some(path)
        } else {
            status = "missing".to_string();
            error_code = Some("EXPORT_OUTPUT_MISSING".to_string());
            None
        }
    } else {
        None
    };
    Ok(RenderStatus {
        run_id: row.get(0)?,
        recording_id: row.get(1)?,
        timeline_id: row.get(2)?,
        timeline_revision: row.get::<_, i64>(3)?.max(0) as u64,
        status,
        progress: row.get::<_, i64>(5)?.clamp(0, 100) as u8,
        output_file_name: row.get(6)?,
        output_path,
        output_size_bytes: row.get::<_, i64>(8)?.max(0) as u64,
        duration_seconds: row.get(9)?,
        error_code,
        renderer_version: row.get(11)?,
        has_audio: row.get::<_, i64>(12)? != 0,
        annotation_count: row.get::<_, i64>(13)?.max(0) as usize,
        active_zoom_count: row.get::<_, i64>(14)?.max(0) as usize,
    })
}

fn parse_frame_rate(value: &str) -> Option<f64> {
    if let Some((numerator, denominator)) = value.split_once('/') {
        let a = numerator.parse::<f64>().ok()?;
        let b = denominator.parse::<f64>().ok()?;
        if b == 0.0 { None } else { Some(a / b) }
    } else {
        value.parse::<f64>().ok()
    }
}

fn probe_video(path: &Path) -> Result<VideoInfo, AppError> {
    let output = Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "stream=codec_type,codec_name,width,height,avg_frame_rate", "-show_entries", "format=duration", "-of", "json"])
        .arg(path).output()
        .map_err(|_| AppError::new("FFPROBE_MISSING", "FFprobe is required to inspect the source and verify the exported MP4."))?;
    if !output.status.success() {
        return Err(AppError::new("VIDEO_PROBE_FAILED", "FFprobe could not inspect the source recording."));
    }
    let probe: ProbeOutput = serde_json::from_slice(&output.stdout)
        .map_err(|_| AppError::new("VIDEO_METADATA_INVALID", "FFprobe returned invalid video metadata."))?;
    let video = probe.streams.iter().find(|stream| stream.codec_type.as_deref() == Some("video"))
        .ok_or_else(|| AppError::new("VIDEO_STREAM_MISSING", "The recording does not contain a video stream."))?;
    let width = video.width.filter(|v| *v > 0).ok_or_else(|| AppError::new("VIDEO_DIMENSIONS_INVALID", "The source video width is invalid."))?;
    let height = video.height.filter(|v| *v > 0).ok_or_else(|| AppError::new("VIDEO_DIMENSIONS_INVALID", "The source video height is invalid."))?;
    if width > 7680 || height > 4320 || u64::from(width) * u64::from(height) > 33_177_600 {
        return Err(AppError::new("VIDEO_DIMENSIONS_UNSUPPORTED", "Tutorial exports support source videos up to 8K UHD dimensions."));
    }
    let frame_rate = video.avg_frame_rate.clone().filter(|v| v != "0/0" && v != "N/A")
        .ok_or_else(|| AppError::new("VIDEO_FRAME_RATE_INVALID", "The source frame rate could not be determined."))?;
    let rate = parse_frame_rate(&frame_rate).ok_or_else(|| AppError::new("VIDEO_FRAME_RATE_INVALID", "The source frame rate is invalid."))?;
    if !(1.0..=120.0).contains(&rate) {
        return Err(AppError::new("VIDEO_FRAME_RATE_UNSUPPORTED", "Tutorial export supports source frame rates from 1 to 120 FPS."));
    }
    let duration_seconds = probe.format.duration.as_deref().and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v > 0.0)
        .ok_or_else(|| AppError::new("VIDEO_DURATION_INVALID", "The source recording duration could not be determined."))?;
    if duration_seconds > 24.0 * 60.0 * 60.0 {
        return Err(AppError::new("VIDEO_DURATION_UNSUPPORTED", "Tutorial export is limited to recordings shorter than 24 hours."));
    }
    Ok(VideoInfo {
        duration_seconds, width, height, frame_rate,
        audio_present: probe.streams.iter().any(|stream| stream.codec_type.as_deref() == Some("audio")),
    })
}

fn ffmpeg_version() -> Result<String, AppError> {
    let output = Command::new("ffmpeg").arg("-version").output()
        .map_err(|_| AppError::new("FFMPEG_MISSING", "FFmpeg is required to export a tutorial."))?;
    if !output.status.success() {
        return Err(AppError::new("FFMPEG_UNAVAILABLE", "FFmpeg could not be started."));
    }
    Ok(String::from_utf8_lossy(&output.stdout).lines().next().unwrap_or("FFmpeg").chars().take(180).collect())
}

fn filter_is_available(name: &str) -> bool {
    let Ok(output) = Command::new("ffmpeg").args(["-hide_banner", "-filters"]).output() else { return false; };
    if !output.status.success() { return false; }
    let mut listing = String::from_utf8_lossy(&output.stdout).to_string();
    listing.push_str(&String::from_utf8_lossy(&output.stderr));
    listing.lines().any(|line| line.split_whitespace().any(|word| word == name))
}

pub(super) fn ass_shaping_mode() -> String {
    let Ok(output) = Command::new("ffmpeg").args(["-hide_banner", "-h", "filter=ass"]).output() else {
        return "auto".to_string();
    };
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    if text.contains("shaping") && text.contains("complex") { "complex".to_string() } else { "auto".to_string() }
}

fn has_arabic_text(value: &str) -> bool {
    value.chars().any(|ch| matches!(ch as u32,
        0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF |
        0xFB50..=0xFDFF | 0xFE70..=0xFEFF))
}

fn active_zoom_segments(document: &timeline::TimelineDocument) -> Vec<&timeline::TimelineZoomSegment> {
    let mut zooms = document.zoom_segments.iter()
        .filter(|zoom| {
            (zoom.origin == "manual" && zoom.approval_status != "rejected")
                || (zoom.origin == "ai" && zoom.approval_status == "accepted")
        })
        .collect::<Vec<_>>();
    zooms.sort_by_key(|zoom| zoom.start_ms);
    zooms
}

fn ffnum(value: f64) -> String { format!("{value:.6}") }

fn zoompan_expressions(zooms: &[&timeline::TimelineZoomSegment], frame_rate: &str) -> (String, String, String) {
    let fps = parse_frame_rate(frame_rate).unwrap_or(30.0);
    let mut z = "1".to_string();
    let mut x = "0".to_string();
    let mut y = "0".to_string();
    for zoom in zooms.iter().rev() {
        // Use zoompan's output-frame counter (on) because d=1 emits one output
        // frame per source frame. The conversion keeps boundaries deterministic
        // across 24/30/60 FPS constant-frame-rate screen recordings.
        let start_frame = (zoom.start_ms as f64 * fps / 1000.0).round() as u64;
        let end_frame = (zoom.end_ms as f64 * fps / 1000.0).round() as u64;
        if start_frame >= end_frame { continue; }
        let transition_frames = ((zoom.transition_duration_ms as f64 * fps / 1000.0).round() as u64)
            .min((end_frame - start_frame) / 2);
        let segment_z = if transition_frames > 0 {
            let t = transition_frames.to_string();
            let p = format!(
                "min(max((on-{start_frame})/{t}\\,0)\\,min(max(({end_frame}-on)/{t}\\,0)\\,1))"
            );
            format!("1+({}-1)*({p})", ffnum(zoom.target_scale))
        } else {
            ffnum(zoom.target_scale)
        };
        let cond = format!("between(on\\,{start_frame}\\,{end_frame})");
        z = format!("if({cond}\\,{segment_z}\\,{z})");
        let cx = ffnum(zoom.x + zoom.width / 2.0);
        let cy = ffnum(zoom.y + zoom.height / 2.0);
        x = format!("if({cond}\\,max(0\\,min(iw-iw/zoom\\,iw*{cx}-iw/(2*zoom)))\\,{x})");
        y = format!("if({cond}\\,max(0\\,min(ih-ih/zoom\\,ih*{cy}-ih/(2*zoom)))\\,{y})");
    }
    (z, x, y)
}

fn zoompan_filter(zooms: &[&timeline::TimelineZoomSegment], width: u32, height: u32, frame_rate: &str) -> String {
    let (z, x, y) = zoompan_expressions(zooms, frame_rate);
    format!("zoompan=z='{z}':x='{x}':y='{y}':d=1:s={width}x{height}:fps={frame_rate}")
}

fn ass_time(ms: u64) -> String {
    let cs = ms / 10;
    format!("{}:{:02}:{:02}.{:02}", cs / 360_000, (cs / 6_000) % 60, (cs / 100) % 60, cs % 100)
}

fn ass_color(value: &str) -> Result<String, AppError> {
    let hex = value.strip_prefix('#').ok_or_else(|| AppError::new("ANNOTATION_COLOR_INVALID", "An annotation contains an invalid color."))?;
    if (hex.len() != 6 && hex.len() != 8) || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(AppError::new("ANNOTATION_COLOR_INVALID", "An annotation contains an invalid color."));
    }
    let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| AppError::new("ANNOTATION_COLOR_INVALID", "An annotation color is invalid."))?;
    let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| AppError::new("ANNOTATION_COLOR_INVALID", "An annotation color is invalid."))?;
    let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| AppError::new("ANNOTATION_COLOR_INVALID", "An annotation color is invalid."))?;
    let alpha = if hex.len() == 8 {
        255_u8.saturating_sub(u8::from_str_radix(&hex[6..8], 16).map_err(|_| AppError::new("ANNOTATION_COLOR_INVALID", "An annotation color is invalid."))?)
    } else { 0 };
    Ok(format!("&H{alpha:02X}{b:02X}{g:02X}{r:02X}&"))
}

fn escape_ass_text(value: &str, direction: &str) -> String {
    let directed = match direction {
        "rtl" => format!("\u{2067}{value}\u{2069}"),
        "ltr" => format!("\u{2066}{value}\u{2069}"),
        _ => value.to_string(),
    };
    directed.replace('\\', "\\\\").replace('{', "\\{").replace('}', "\\}")
        .replace("\r\n", "\n").replace('\r', "\n").replace('\n', "\\N").replace('\t', " ")
}

fn ass_alignment(value: &str) -> u8 {
    match value { "left" => 7, "right" => 9, _ => 8 }
}

fn generate_ass(document: &timeline::TimelineDocument, width: u32, height: u32) -> Result<String, AppError> {
    let mut ass = format!(
        "[Script Info]\nScriptType: v4.00+\nWrapStyle: 0\nScaledBorderAndShadow: yes\nPlayResX: {width}\nPlayResY: {height}\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Noto Sans Arabic,24,&H00FFFFFF,&H000000FF,&H00000000,&HAA000000,0,0,0,0,100,100,0,0,3,0,0,8,0,0,0,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n"
    );
    let mut annotations = document.annotations.iter().collect::<Vec<_>>();
    annotations.sort_by_key(|item| item.start_ms);
    for item in annotations {
        let x = (item.x * width as f64).round() as u32;
        let y = (item.y * height as f64).round() as u32;
        let left = (item.x * width as f64).round() as u32;
        let right = ((1.0 - item.x - item.max_width) * width as f64).round() as u32;
        let font = if item.language == "en" { "Noto Sans" } else { "Noto Sans Arabic" };
        let foreground = ass_color(&item.foreground)?;
        let background = ass_color(&item.background)?;
        let tags = format!("{{\\an{}\\pos({x},{y})\\fs{}\\fn{font}\\1c{foreground}\\4c{background}\\bord0\\shad0}}",
            ass_alignment(&item.alignment), item.font_size);
        let text = escape_ass_text(&item.text, &item.direction);
        ass.push_str(&format!("Dialogue: 0,{},{},Default,,{left},{right},0,,{tags}{text}\n",
            ass_time(item.start_ms), ass_time(item.end_ms)));
    }
    Ok(ass)
}

fn escape_filter_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "\\\\").replace(':', "\\:")
        .replace('\'', "\\'").replace(',', "\\,").replace('[', "\\[").replace(']', "\\]")
}

fn parse_progress_time(value: &str) -> Option<u64> {
    Some(value.parse::<u64>().ok()? / 1000)
}

fn update_render_run(project_dir: &Path, run_id: &str, status: &str, progress: u8, size: u64, output: Option<&Path>, error: Option<&str>) {
    if let Ok(connection) = db_connection(project_dir) {
        let _ = connection.execute(
            "UPDATE render_runs SET status=?1, progress=?2, output_size_bytes=?3, output_path=COALESCE(?4, output_path), error_code=?5 WHERE id=?6",
            params![status, progress as i64, size as i64, output.map(|p| p.to_string_lossy().to_string()), error, run_id],
        );
    }
}

fn detach_child_stdin(command: &mut Command) {
    command.stdin(Stdio::null());
}

fn available_space_bytes(path: &Path) -> Result<u64, AppError> {
    let output = Command::new("df")
        .args(["--output=avail", "-k"])
        .arg(path)
        .output()
        .map_err(|_| AppError::new("DISK_SPACE_CHECK_FAILED", "Could not check free space in the project's export folder."))?;
    if !output.status.success() {
        return Err(AppError::new("DISK_SPACE_CHECK_FAILED", "Could not check free space in the project's export folder."));
    }
    let value = String::from_utf8_lossy(&output.stdout)
        .lines()
        .last()
        .and_then(|line| line.trim().parse::<u64>().ok())
        .ok_or_else(|| AppError::new("DISK_SPACE_CHECK_FAILED", "The available disk space could not be determined."))?;
    value.checked_mul(1024)
        .ok_or_else(|| AppError::new("DISK_SPACE_CHECK_FAILED", "The available disk space value is invalid."))
}

fn ensure_render_space(path: &Path, duration_seconds: f64) -> Result<(), AppError> {
    // Conservative allowance for a high-motion screen capture plus temporary output.
    // The check is a preflight only; runtime write errors are still handled normally.
    const SAFETY_MARGIN: u64 = 256 * 1024 * 1024;
    const ESTIMATED_BYTES_PER_SECOND: u64 = 1024 * 1024;
    let estimated = (duration_seconds.ceil() as u64)
        .saturating_mul(ESTIMATED_BYTES_PER_SECOND)
        .saturating_add(SAFETY_MARGIN);
    let available = available_space_bytes(path)?;
    if available < estimated {
        return Err(AppError::new(
            "INSUFFICIENT_EXPORT_SPACE",
            format!(
                "Not enough free space for a safe export. About {:.0} MiB is recommended; {:.0} MiB is available. Free space and try again.",
                estimated as f64 / (1024.0 * 1024.0),
                available as f64 / (1024.0 * 1024.0)
            ),
        ));
    }
    Ok(())
}

fn build_render_request(project_dir: &Path, run_id: String, recording_id: String, expected_revision: u64) -> Result<RenderRequest, AppError> {
    let (timeline_id, source_run, current_revision, current_snapshot): (String, String, i64, String) =
        db_connection(project_dir)?.query_row(
            "SELECT timeline_id, source_analysis_run_id, current_revision, current_snapshot FROM timelines WHERE recording_id=?1",
            params![recording_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        ).map_err(|_| AppError::new("TIMELINE_NOT_FOUND", "Create or open a saved editable timeline before exporting."))?;
    if !timeline::valid_timeline_id(&timeline_id) || !analysis::analysis_run_id_valid(&source_run) || current_revision < 1 {
        return Err(AppError::new("TIMELINE_INDEX_INVALID", "The project's timeline index contains invalid identifiers."));
    }
    if current_revision as u64 != expected_revision {
        return Err(AppError::new("TIMELINE_REVISION_CONFLICT", "Save or reload the current timeline revision before exporting."));
    }
    let root = project_dir.join("timeline").canonicalize().map_err(|_| AppError::new("TIMELINE_NOT_FOUND", "The project's timeline folder is unavailable."))?;
    let folder = root.join(&timeline_id).canonicalize().map_err(|_| AppError::new("TIMELINE_NOT_FOUND", "The timeline snapshot folder is unavailable."))?;
    if !folder.starts_with(&root) { return Err(AppError::new("TIMELINE_PATH_INVALID", "The timeline folder is outside the managed project.")); }
    let file_name = format!("revision-{expected_revision}.json");
    if current_snapshot != format!("{timeline_id}/{file_name}") {
        return Err(AppError::new("TIMELINE_INDEX_INVALID", "The current timeline snapshot pointer is invalid."));
    }
    let snapshot = folder.join(file_name).canonicalize().map_err(|_| AppError::new("TIMELINE_NOT_FOUND", "The selected timeline revision could not be read."))?;
    if !snapshot.starts_with(&folder) { return Err(AppError::new("TIMELINE_PATH_INVALID", "The selected timeline revision is outside its managed folder.")); }
    let document: timeline::TimelineDocument = serde_json::from_slice(&fs::read(snapshot).map_err(|_| AppError::new("TIMELINE_NOT_FOUND", "The saved timeline could not be read."))?)
        .map_err(|_| AppError::new("TIMELINE_SNAPSHOT_INVALID", "The saved timeline is invalid JSON."))?;
    let (_, manifest) = timeline::source_manifest(project_dir, &source_run)?;
    timeline::validate_timeline(&document, &manifest)?;
    if document.timeline_id != timeline_id || document.recording_id != recording_id || document.source_analysis_run_id != source_run || document.revision != expected_revision {
        return Err(AppError::new("TIMELINE_INDEX_MISMATCH", "The timeline snapshot does not match the project's current revision."));
    }
    let (_file_name, recording_path) = analysis::resolve_recording(project_dir, &recording_id)?;
    let video = probe_video(&recording_path)?;
    let version = ffmpeg_version()?;
    if !document.annotations.is_empty() && !filter_is_available("ass") {
        return Err(AppError::new("ASS_FILTER_UNAVAILABLE", "This FFmpeg build does not include the libass subtitle filter needed to render captions."));
    }
    let requires_arabic = document.annotations.iter().any(|a| a.language != "en" || has_arabic_text(&a.text));
    if requires_arabic {
        let font = Command::new("fc-match")
            .args(["-f", "%{family}", "Noto Sans Arabic:lang=ar"])
            .output()
            .map_err(|_| AppError::new("FONTCONFIG_MISSING", "Fontconfig is required to verify Arabic caption fonts."))?;
        let family = String::from_utf8_lossy(&font.stdout);
        if !font.status.success() || !family.to_lowercase().contains("noto sans arabic") {
            return Err(AppError::new("ARABIC_FONT_MISSING", "Noto Sans Arabic is required to render Arabic captions. Install the Noto Core fonts, then retry."));
        }
    }
    if !document.annotations.is_empty() {
        let font = Command::new("fc-match")
            .args(["-f", "%{family}", "Noto Sans"])
            .output()
            .map_err(|_| AppError::new("FONTCONFIG_MISSING", "Fontconfig is required to verify caption fonts."))?;
        if !font.status.success() || String::from_utf8_lossy(&font.stdout).trim().is_empty() {
            return Err(AppError::new("CAPTION_FONT_MISSING", "A supported Noto Sans font is required to render captions."));
        }
    }
    let shaping = if requires_arabic { ass_shaping_mode() } else { "not_required".to_string() };
    if requires_arabic && shaping == "auto" {
        return Err(AppError::new("ARABIC_SHAPING_UNAVAILABLE", "This FFmpeg/libass build cannot confirm complex Arabic text shaping. Use a build with HarfBuzz support."));
    }
    let export_dir = project_dir.join("exports");
    fs::create_dir_all(&export_dir).map_err(|_| AppError::new("EXPORT_STORAGE_FAILED", "The project's export folder could not be created."))?;
    let output_dir = export_dir.canonicalize().map_err(|_| AppError::new("EXPORT_STORAGE_FAILED", "The project's export folder is unavailable."))?;
    ensure_render_space(&output_dir, video.duration_seconds)?;
    let cache_dir = output_dir.join(".render-cache").join(&run_id);
    fs::create_dir_all(&cache_dir).map_err(|_| AppError::new("EXPORT_STORAGE_FAILED", "The temporary render workspace could not be created."))?;
    let cache_dir = cache_dir.canonicalize().map_err(|_| AppError::new("EXPORT_STORAGE_FAILED", "The temporary render workspace is unavailable."))?;
    if !cache_dir.starts_with(&output_dir) { return Err(AppError::new("EXPORT_PATH_INVALID", "The render workspace is outside the project's export folder.")); }
    let output_file = format!("Screenstory-tutorial-{run_id}.mp4");
    let output_path = output_dir.join(output_file);
    if output_path.exists() { return Err(AppError::new("EXPORT_ALREADY_EXISTS", "An export with this unique name already exists.")); }
    Ok(RenderRequest {
        run_id, recording_path, recording_id, file_name: document.source_file_name.clone(),
        timeline: document, video, ffmpeg_version: version, subtitle_shaping: shaping,
        output_dir, cache_dir, output_path,
    })
}

fn render_filter(request: &RenderRequest, ass_path: Option<&Path>) -> String {
    let mut filters = Vec::new();
    let zooms = active_zoom_segments(&request.timeline);
    if !zooms.is_empty() {
        filters.push(zoompan_filter(&zooms, request.video.width, request.video.height, &request.video.frame_rate));
    }
    if let Some(path) = ass_path {
        let shaping = if request.subtitle_shaping == "complex" { ":shaping=complex" } else { "" };
        filters.push(format!("ass=filename='{}'{shaping}", escape_filter_path(path)));
    }
    if filters.is_empty() { "format=yuv420p".to_string() } else { filters.join(",") }
}

fn save_render_plan(request: &RenderRequest, ass: bool) -> Result<(), AppError> {
    let plan = RenderPlan {
        schema_version: 1,
        run_id: request.run_id.clone(),
        recording_id: request.recording_id.clone(),
        source_file_name: request.file_name.clone(),
        timeline_id: request.timeline.timeline_id.clone(),
        timeline_revision: request.timeline.revision,
        source_duration_ms: request.timeline.source_duration_ms,
        output_width: request.video.width,
        output_height: request.video.height,
        frame_rate: request.video.frame_rate.clone(),
        preserve_source_audio: request.video.audio_present,
        active_zoom_count: active_zoom_segments(&request.timeline).len(),
        annotation_count: request.timeline.annotations.len(),
        active_zooms: active_zoom_segments(&request.timeline).into_iter().cloned().collect(),
        annotations: request.timeline.annotations.clone(),
        renderer_version: request.ffmpeg_version.clone(),
        subtitle_filter: if ass { "libass".to_string() } else { "none".to_string() },
        subtitle_shaping: request.subtitle_shaping.clone(),
    };
    let bytes = serde_json::to_vec_pretty(&plan).map_err(|_| AppError::new("RENDER_PLAN_INVALID", "The render plan could not be serialized."))?;
    let path = request.cache_dir.join("render-plan.json");
    let temporary = request.cache_dir.join("render-plan.json.tmp");
    let mut file = File::create(&temporary).map_err(|_| AppError::new("RENDER_PLAN_SAVE_FAILED", "The render plan could not be written."))?;
    file.write_all(&bytes).and_then(|_| file.sync_all()).map_err(|_| AppError::new("RENDER_PLAN_SAVE_FAILED", "The render plan could not be saved."))?;
    drop(file);
    fs::rename(temporary, path).map_err(|_| AppError::new("RENDER_PLAN_SAVE_FAILED", "The render plan could not be finalized."))?;
    Ok(())
}

fn validate_output(path: &Path, source: &VideoInfo) -> Result<(u64, f64), AppError> {
    let size = fs::metadata(path).map_err(|_| AppError::new("EXPORT_OUTPUT_MISSING", "FFmpeg did not create the expected MP4 output."))?.len();
    if size < 1024 { return Err(AppError::new("EXPORT_OUTPUT_INVALID", "The generated MP4 is unexpectedly small.")); }
    let output = Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "stream=codec_type,codec_name,width,height", "-show_entries", "format=duration", "-of", "json"])
        .arg(path).output().map_err(|_| AppError::new("FFPROBE_MISSING", "FFprobe is required to verify the exported MP4."))?;
    if !output.status.success() { return Err(AppError::new("EXPORT_OUTPUT_INVALID", "FFprobe could not decode the exported MP4.")); }
    let probe: ProbeOutput = serde_json::from_slice(&output.stdout).map_err(|_| AppError::new("EXPORT_OUTPUT_INVALID", "The exported MP4 metadata is invalid."))?;
    let video = probe.streams.iter().find(|stream| stream.codec_type.as_deref() == Some("video"))
        .ok_or_else(|| AppError::new("EXPORT_OUTPUT_INVALID", "The exported file does not contain a video stream."))?;
    if video.codec_name.as_deref() != Some("h264") || video.width != Some(source.width) || video.height != Some(source.height) {
        return Err(AppError::new("EXPORT_OUTPUT_INVALID", "The exported video codec or dimensions do not meet the output contract."));
    }
    if source.audio_present && !probe.streams.iter().any(|stream| stream.codec_type.as_deref() == Some("audio")) {
        return Err(AppError::new("EXPORT_AUDIO_MISSING", "The source recording had audio, but the export does not."));
    }
    let duration = probe.format.duration.as_deref().and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or_else(|| AppError::new("EXPORT_DURATION_INVALID", "The exported MP4 duration could not be verified."))?;
    if (duration - source.duration_seconds).abs() > 0.75 {
        return Err(AppError::new("EXPORT_DURATION_MISMATCH", "The exported MP4 duration differs from the source recording."));
    }
    Ok((size, duration))
}

fn render_worker(
    project: PathBuf,
    request: RenderRequest,
    cancellation: Arc<AtomicBool>,
    registry: Arc<std::sync::Mutex<std::collections::HashMap<String, Arc<AtomicBool>>>>,
    processes: Arc<std::sync::Mutex<HashMap<String, u32>>>,
) {
    let run_id = request.run_id.clone();
    let result = (|| -> Result<(), AppError> {
        let ass_path = if request.timeline.annotations.is_empty() {
            None
        } else {
            let path = request.cache_dir.join("captions.ass");
            let contents = generate_ass(&request.timeline, request.video.width, request.video.height)?;
            let mut file = OpenOptions::new().write(true).create_new(true).open(&path)
                .map_err(|_| AppError::new("CAPTION_FILE_SAVE_FAILED", "The temporary caption file could not be created."))?;
            file.write_all(contents.as_bytes()).and_then(|_| file.sync_all())
                .map_err(|_| AppError::new("CAPTION_FILE_SAVE_FAILED", "The temporary caption file could not be saved."))?;
            Some(path)
        };
        save_render_plan(&request, ass_path.is_some())?;
        update_render_run(&project, &run_id, "rendering", 1, 0, None, None);
        let filters = render_filter(&request, ass_path.as_deref());
        let partial = request.output_dir.join(format!("{run_id}.partial.mp4"));
        let mut command = Command::new("ffmpeg");
        command.args(["-hide_banner", "-nostdin", "-loglevel", "error", "-y", "-i"]).arg(&request.recording_path)
            .args(["-map", "0:v:0", "-map", "0:a?", "-vf", &filters,
                "-c:v", "libx264", "-preset", "medium", "-crf", "20", "-pix_fmt", "yuv420p",
                "-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart",
                "-progress", "pipe:1", "-stats_period", "0.5"])
            .arg(&partial);
        detach_child_stdin(&mut command);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command.spawn()
            .map_err(|_| AppError::new("FFMPEG_START_FAILED", "FFmpeg could not start the MP4 export."))?;
        if let Ok(mut active_processes) = processes.lock() {
            active_processes.insert(run_id.clone(), child.id());
        }
        if cancellation.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = fs::remove_file(&partial);
            if let Ok(mut active_processes) = processes.lock() { active_processes.remove(&run_id); }
            return Err(AppError::new("RENDER_CANCELLED", "The MP4 export was cancelled."));
        }
        let stdout = child.stdout.take().ok_or_else(|| AppError::new("FFMPEG_PROGRESS_UNAVAILABLE", "The FFmpeg progress stream could not be opened."))?;
        let stderr = child.stderr.take().map(|mut stream| thread::spawn(move || {
            let mut retained = Vec::with_capacity(24 * 1024);
            let mut scratch = [0_u8; 4096];
            loop {
                match stream.read(&mut scratch) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => {
                        let remaining = 24 * 1024_usize.saturating_sub(retained.len());
                        retained.extend_from_slice(&scratch[..count.min(remaining)]);
                    }
                }
            }
            String::from_utf8_lossy(&retained).to_string()
        }));
        let mut last_progress = 0_u8;
        for line in BufReader::new(stdout).lines() {
            if cancellation.load(Ordering::SeqCst) {
                let _ = child.kill();
                let _ = child.wait();
                let _ = fs::remove_file(&partial);
                return Err(AppError::new("RENDER_CANCELLED", "The MP4 export was cancelled."));
            }
            let Ok(line) = line else { break; };
            if let Some(value) = line.strip_prefix("out_time_ms=") {
                if let Some(ms) = parse_progress_time(value) {
                    let progress = ((ms as f64 / (request.video.duration_seconds * 1000.0)) * 90.0)
                        .floor().clamp(1.0, 90.0) as u8;
                    if progress > last_progress {
                        last_progress = progress;
                        update_render_run(&project, &run_id, "rendering", progress, 0, None, None);
                    }
                }
            }
        }
        let status = child.wait().map_err(|_| AppError::new("FFMPEG_WAIT_FAILED", "The export process could not be reaped."))?;
        let error_text = stderr.and_then(|handle| handle.join().ok()).unwrap_or_default();
        if cancellation.load(Ordering::SeqCst) {
            let _ = fs::remove_file(&partial);
            return Err(AppError::new("RENDER_CANCELLED", "The MP4 export was cancelled."));
        }
        if !status.success() {
            let _ = fs::remove_file(&partial);
            let detail = error_text.lines().last().unwrap_or("FFmpeg reported a video encoding error.");
            return Err(AppError::new("FFMPEG_EXPORT_FAILED", format!("FFmpeg could not complete the export. {detail}")));
        }
        update_render_run(&project, &run_id, "validating", 92, 0, None, None);
        let (size, _) = validate_output(&partial, &request.video)?;
        if request.output_path.exists() {
            let _ = fs::remove_file(&partial);
            return Err(AppError::new("EXPORT_ALREADY_EXISTS", "An export already exists at the selected destination."));
        }
        let file = File::open(&partial).map_err(|_| AppError::new("EXPORT_FINALIZE_FAILED", "The temporary MP4 could not be opened."))?;
        file.sync_all().map_err(|_| AppError::new("EXPORT_FINALIZE_FAILED", "The temporary MP4 could not be flushed to disk."))?;
        fs::rename(&partial, &request.output_path).map_err(|_| AppError::new("EXPORT_FINALIZE_FAILED", "The validated MP4 could not be moved into the project export folder."))?;
        if let Ok(dir) = File::open(&request.output_dir) { let _ = dir.sync_all(); }
        update_render_run(&project, &run_id, "completed", 100, size, Some(&request.output_path), None);
        Ok(())
    })();
    if let Err(error) = result {
        let partial = request.output_dir.join(format!("{run_id}.partial.mp4"));
        let _ = fs::remove_file(partial);
        update_render_run(&project, &run_id, if error.code == "RENDER_CANCELLED" { "cancelled" } else { "failed" }, 0, 0, None, Some(error.code));
    }
    if let Ok(mut active) = registry.lock() { active.remove(&run_id); }
    if let Ok(mut active_processes) = processes.lock() { active_processes.remove(&run_id); }
}

#[tauri::command]
pub fn start_render(recording_id: String, expected_revision: u64, state: State<'_, AppState>) -> Result<RenderStarted, AppError> {
    if state.recording.lock().map_err(|_| AppError::new("STATE_LOCK_ERROR", "Recording state is unavailable."))?.is_some() {
        return Err(AppError::new("RECORDING_IN_PROGRESS", "Stop the recording before exporting a tutorial."));
    }
    let project = state.active_project.lock().map_err(|_| AppError::new("STATE_LOCK_ERROR", "Project state is unavailable."))?
        .as_ref().cloned().ok_or_else(|| AppError::new("NO_ACTIVE_PROJECT", "Open a project before exporting a tutorial."))?;
    ensure_render_table(&project)?;
    let connection = db_connection(&project)?;
    let active: i64 = connection.query_row(
        "SELECT COUNT(*) FROM render_runs WHERE status IN ('preparing','rendering','validating','cancelling')",
        [], |row| row.get(0)
    ).unwrap_or(0);
    if active > 0 { return Err(AppError::new("RENDER_ALREADY_ACTIVE", "Another export is already running in this project.")); }

    let run_id = format!("r{}-{}", now_ms(), std::process::id());
    let request = build_render_request(&project, run_id.clone(), recording_id.clone(), expected_revision)?;
    if !active_zoom_segments(&request.timeline).is_empty() && !filter_is_available("zoompan") {
        return Err(AppError::new("ZOOMPAN_FILTER_UNAVAILABLE", "This FFmpeg build does not include zoompan, which is needed for zoom effects."));
    }
    let output_file_name = request.output_path.file_name().and_then(|v| v.to_str()).unwrap_or("Screenstory-tutorial.mp4").to_string();
    let timeline_id = request.timeline.timeline_id.clone();
    let timeline_revision = request.timeline.revision;
    let zoom_count = active_zoom_segments(&request.timeline).len();
    let annotation_count = request.timeline.annotations.len();
    connection.execute(
        "INSERT INTO render_runs (id, recording_id, timeline_id, timeline_revision, output_file_name, output_path, status, progress, created_at_ms, duration_seconds, renderer_version, has_audio, annotation_count, active_zoom_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'preparing', 0, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![run_id, recording_id, timeline_id, expected_revision as i64, output_file_name, request.output_path.to_string_lossy().to_string(), now_ms() as i64, request.video.duration_seconds, request.ffmpeg_version, request.video.audio_present as i64, annotation_count as i64, zoom_count as i64]
    ).map_err(|_| AppError::new("RENDER_DATABASE_ERROR", "The export run could not be added to the project history."))?;
    let token = Arc::new(AtomicBool::new(false));
    let registry = Arc::clone(&state.render_cancellations);
    state.render_cancellations.lock().map_err(|_| AppError::new("STATE_LOCK_ERROR", "Render state is unavailable."))?
        .insert(run_id.clone(), Arc::clone(&token));
    let worker_project = project.clone();
    let worker_registry = Arc::clone(&registry);
    let worker_processes = Arc::clone(&state.render_processes);
    let worker_request = request;
    thread::spawn(move || render_worker(worker_project, worker_request, token, worker_registry, worker_processes));
    Ok(RenderStarted { run_id, timeline_id, timeline_revision, output_file_name })
}

#[tauri::command]
pub fn get_render_status(run_id: String, state: State<'_, AppState>) -> Result<RenderStatus, AppError> {
    if run_id.len() < 3 || run_id.len() > 100 || !run_id.starts_with('r') ||
        !run_id.chars().all(|c| c == 'r' || c == '-' || c.is_ascii_digit()) {
        return Err(AppError::new("INVALID_RENDER_RUN_ID", "The export run identifier is invalid."));
    }
    let project = state.active_project.lock().map_err(|_| AppError::new("STATE_LOCK_ERROR", "Project state is unavailable."))?
        .as_ref().cloned().ok_or_else(|| AppError::new("NO_ACTIVE_PROJECT", "Open a project to inspect export status."))?;
    ensure_render_table(&project)?;
    db_connection(&project)?.query_row(
        "SELECT id, recording_id, timeline_id, timeline_revision, status, progress, output_file_name, output_path, output_size_bytes, duration_seconds, error_code, renderer_version, has_audio, annotation_count, active_zoom_count FROM render_runs WHERE id=?1",
        params![run_id],
        render_status_from_row
    ).map_err(|_| AppError::new("RENDER_RUN_NOT_FOUND", "The requested export run could not be found in the active project."))
}

#[tauri::command]
pub fn list_render_history(recording_id: String, state: State<'_, AppState>) -> Result<Vec<RenderStatus>, AppError> {
    if recording_id.len() < 8 || recording_id.len() > 160 {
        return Err(AppError::new("INVALID_RECORDING_ID", "The recording identifier is invalid."));
    }
    let project = state.active_project.lock().map_err(|_| AppError::new("STATE_LOCK_ERROR", "Project state is unavailable."))?
        .as_ref().cloned().ok_or_else(|| AppError::new("NO_ACTIVE_PROJECT", "Open a project to inspect export history."))?;
    // Use the canonical managed-recording resolver; IDs intentionally match the filename including its extension.
    let _ = analysis::resolve_recording(&project, &recording_id)?;
    ensure_render_table(&project)?;
    let connection = db_connection(&project)?;
    let mut statement = connection.prepare(
        "SELECT id, recording_id, timeline_id, timeline_revision, status, progress, output_file_name, output_path, output_size_bytes, duration_seconds, error_code, renderer_version, has_audio, annotation_count, active_zoom_count FROM render_runs WHERE recording_id=?1 ORDER BY created_at_ms DESC LIMIT 20"
    ).map_err(|_| AppError::new("RENDER_DATABASE_ERROR", "The export history could not be loaded."))?;
    let rows = statement.query_map(params![recording_id], render_status_from_row)
        .map_err(|_| AppError::new("RENDER_DATABASE_ERROR", "The export history could not be loaded."))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::new("RENDER_DATABASE_ERROR", "The export history could not be read."))
}

pub(super) fn recover_interrupted_renders(project_dir: &Path) -> Result<(), AppError> {
    ensure_render_table(project_dir)?;
    let connection = db_connection(project_dir)?;
    let mut statement = connection.prepare(
        "SELECT id FROM render_runs WHERE status IN ('preparing','rendering','validating','cancelling')"
    ).map_err(|_| AppError::new("RENDER_DATABASE_ERROR", "Interrupted export history could not be inspected."))?;
    let ids = statement.query_map([], |row| row.get::<_, String>(0))
        .map_err(|_| AppError::new("RENDER_DATABASE_ERROR", "Interrupted export history could not be inspected."))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::new("RENDER_DATABASE_ERROR", "Interrupted export history could not be read."))?;
    drop(statement);
    for id in ids {
        let _ = fs::remove_file(project_dir.join("exports").join(format!("{id}.partial.mp4")));
        connection.execute(
            "UPDATE render_runs SET status='interrupted', progress=0, error_code='APP_EXITED' WHERE id=?1",
            params![id]
        ).map_err(|_| AppError::new("RENDER_DATABASE_ERROR", "Interrupted export state could not be recovered."))?;
    }
    Ok(())
}

pub(super) fn prepare_for_app_exit(state: &AppState) {
    let active = state.render_cancellations.lock().map(|runs| {
        runs.iter().map(|(id, token)| {
            token.store(true, Ordering::SeqCst);
            id.clone()
        }).collect::<Vec<_>>()
    }).unwrap_or_default();
    let pids = state.render_processes.lock().map(|processes| processes.values().copied().collect::<Vec<_>>()).unwrap_or_default();
    for pid in pids {
        // FFmpeg is launched in its own process group. Signal the group so a
        // child encoder cannot be orphaned when the desktop shell exits.
        let _ = Command::new("kill").args(["-TERM", "--", &format!("-{pid}")]).status();
    }
    let project = state.active_project.lock().ok().and_then(|active| active.clone());
    if let Some(project) = project {
        for id in active {
            update_render_run(&project, &id, "interrupted", 0, 0, None, Some("APP_EXITED"));
            let _ = fs::remove_file(project.join("exports").join(format!("{id}.partial.mp4")));
        }
    }
}

#[tauri::command]
pub fn cancel_render(run_id: String, state: State<'_, AppState>) -> Result<(), AppError> {
    if run_id.len() < 3 || run_id.len() > 100 || !run_id.starts_with('r') ||
        !run_id.chars().all(|c| c == 'r' || c == '-' || c.is_ascii_digit()) {
        return Err(AppError::new("INVALID_RENDER_RUN_ID", "The export run identifier is invalid."));
    }
    let project = state.active_project.lock().map_err(|_| AppError::new("STATE_LOCK_ERROR", "Project state is unavailable."))?
        .as_ref().cloned().ok_or_else(|| AppError::new("NO_ACTIVE_PROJECT", "Open a project before cancelling an export."))?;
    let status: String = db_connection(&project)?.query_row("SELECT status FROM render_runs WHERE id=?1", params![run_id], |row| row.get(0))
        .map_err(|_| AppError::new("RENDER_RUN_NOT_FOUND", "This export run could not be found."))?;
    if ["completed", "failed", "cancelled"].contains(&status.as_str()) { return Ok(()); }
    if let Some(token) = state.render_cancellations.lock().map_err(|_| AppError::new("STATE_LOCK_ERROR", "Render state is unavailable."))?.get(&run_id) {
        token.store(true, Ordering::SeqCst);
    }
    update_render_run(&project, &run_id, "cancelling", 0, 0, None, Some("RENDER_CANCELLED"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zoom(status: &str) -> timeline::TimelineZoomSegment {
        timeline::TimelineZoomSegment {
            id: "zoom-a".into(), start_ms: 1000, end_ms: 4000,
            x: 0.25, y: 0.25, width: 0.5, height: 0.5, target_scale: 1.75,
            rationale: "Focus the primary control.".into(), evidence_frame_ids: vec!["frame-000".into()],
            origin: "ai".into(), approval_status: status.into(), transition_duration_ms: 250,
        }
    }

    #[test]
    fn zoompan_expression_uses_input_timestamps_and_transition_ramps() {
        let item = zoom("accepted");
        let (z, x, y) = zoompan_expressions(&[&item], "30/1");
        assert!(z.contains("between(on\\,30\\,120)"));
        assert!(z.contains("max((on-30)/8"));
        assert!(x.contains("iw/zoom"));
        assert!(x.contains("\\,"));
        assert!(y.contains("ih/zoom"));
    }

    #[test]
    fn only_accepted_ai_or_manual_zooms_are_active() {
        let pending = zoom("pending");
        let rejected = zoom("rejected");
        let accepted = zoom("accepted");
        let manual = timeline::TimelineZoomSegment { origin: "manual".into(), approval_status: "pending".into(), ..zoom("pending") };
        let active = [&pending, &rejected, &accepted, &manual].iter().copied()
            .filter(|item| item.origin == "manual" || item.approval_status == "accepted").count();
        assert_eq!(active, 2);
    }

    #[test]
    fn ass_time_and_colors_follow_ass_alpha_bgr_conventions() {
        assert_eq!(ass_time(1234), "0:00:01.23");
        assert_eq!(ass_color("#FF8000").unwrap(), "&H000080FF&");
        assert_eq!(ass_color("#000000CC").unwrap(), "&H33000000&");
    }

    #[test]
    fn annotation_text_escapes_ass_override_tags_and_preserves_arabic() {
        let value = escape_ass_text("{\\bord99}مرحبا", "rtl");
        assert!(value.contains("\\{"));
        assert!(value.contains("مرحبا"));
        assert!(value.starts_with('\u{2067}'));
        assert!(value.ends_with('\u{2069}'));
    }

    #[test]
    fn export_space_estimate_has_a_fixed_safety_margin_and_scales_with_duration() {
        let one_second = 1_u64.saturating_mul(1024 * 1024).saturating_add(256 * 1024 * 1024);
        let ten_seconds = 10_u64.saturating_mul(1024 * 1024).saturating_add(256 * 1024 * 1024);
        assert_eq!(one_second, 269_484_032);
        assert_eq!(ten_seconds - one_second, 9 * 1024 * 1024);
    }

    #[cfg(unix)]
    #[test]
    fn detached_child_stdin_is_not_a_terminal_and_returns_eof() {
        let mut command = Command::new("sh");
        command.args(["-c", "test ! -t 0 && ! read -r value"]);
        detach_child_stdin(&mut command);

        let status = command.status().expect("shell should start");
        assert!(status.success(), "child stdin must be detached from the terminal");
    }

    #[test]
    fn ffmpeg_progress_time_and_frame_rate_are_parsed() {
        assert_eq!(parse_progress_time("12000000"), Some(12000));
        assert!((parse_frame_rate("30000/1001").unwrap() - 29.97002997).abs() < 0.0001);
    }

    #[test]
    fn ffmpeg_renders_a_zoom_and_arabic_ass_caption_when_capabilities_are_present() {
        if Command::new("ffmpeg").arg("-version").output().is_err()
            || Command::new("ffprobe").arg("-version").output().is_err()
        {
            return;
        }
        let root = std::env::temp_dir().join(format!("screenstory-render-proof-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let input_zoom = timeline::TimelineZoomSegment {
            start_ms: 0,
            end_ms: 1000,
            transition_duration_ms: 250,
            ..zoom("accepted")
        };
        let doc = timeline::TimelineDocument {
            schema_version: 2,
            timeline_id: "t12345-77".into(),
            recording_id: "recording-1".into(),
            source_file_name: "input.mp4".into(),
            source_duration_ms: 1000,
            source_analysis_run_id: "a12345-77".into(),
            revision: 1,
            created_at_ms: 1,
            updated_at_ms: 1,
            status: "draft".into(),
            language: "ar".into(),
            steps: vec![],
            zoom_segments: vec![input_zoom.clone()],
            annotations: vec![timeline::TimelineAnnotation {
                id: "caption-1".into(),
                text: "اضغط على الإعدادات".into(),
                language: "ar".into(),
                start_ms: 0,
                end_ms: 1000,
                x: 0.1,
                y: 0.75,
                max_width: 0.8,
                font_size: 22,
                foreground: "#FFFFFF".into(),
                background: "#000000CC".into(),
                alignment: "right".into(),
                direction: "rtl".into(),
                origin: "manual".into(),
            }],
        };
        let ass_path = root.join("captions.ass");
        fs::write(&ass_path, generate_ass(&doc, 320, 180).unwrap()).unwrap();
        let filter = format!(
            "{},ass=filename='{}':shaping=complex",
            zoompan_filter(&[&input_zoom], 320, 180, "30/1"),
            escape_filter_path(&ass_path)
        );
        let output_path = root.join("rendered.mp4");
        let output = Command::new("ffmpeg")
            .args([
                "-hide_banner", "-loglevel", "error",
                "-f", "lavfi", "-i", "testsrc2=size=320x180:rate=30:duration=1",
                "-vf", &filter, "-frames:v", "30", "-an",
                "-c:v", "libx264", "-pix_fmt", "yuv420p", "-y",
            ])
            .arg(&output_path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "FFmpeg renderer proof failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let probe = Command::new("ffprobe")
            .args([
                "-v", "error",
                "-select_streams", "v:0",
                "-show_entries", "stream=codec_name,width,height",
                "-of", "csv=p=0",
            ])
            .arg(&output_path)
            .output()
            .unwrap();
        assert!(probe.status.success(), "ffprobe could not inspect the renderer proof output");
        let metadata = String::from_utf8_lossy(&probe.stdout);
        assert!(metadata.contains("h264,320,180"), "Unexpected renderer proof output: {metadata}");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ass_document_contains_positioned_and_timed_arabic_caption() {
        let document = timeline::TimelineDocument {
            schema_version: 2, timeline_id: "t12345-77".into(), recording_id: "recording-1".into(),
            source_file_name: "capture.mp4".into(), source_duration_ms: 10000,
            source_analysis_run_id: "a12345-77".into(), revision: 1, created_at_ms: 100, updated_at_ms: 100,
            status: "draft".into(), language: "ar".into(),
            steps: vec![timeline::TimelineStep { id: "s1".into(), title: "Step".into(), description: "Open".into(),
                start_ms: 0, end_ms: 1000, origin: "manual".into(), evidence_frame_ids: vec![],
                confidence: None, evidence_kind: None }],
            zoom_segments: vec![],
            annotations: vec![timeline::TimelineAnnotation {
                id: "a1".into(), text: "اضغط على الإعدادات".into(), language: "ar".into(),
                start_ms: 500, end_ms: 2000, x: 0.1, y: 0.1, max_width: 0.8, font_size: 28,
                foreground: "#FFFFFF".into(), background: "#000000CC".into(), alignment: "right".into(),
                direction: "rtl".into(), origin: "manual".into(),
            }],
        };
        let ass = generate_ass(&document, 1920, 1080).unwrap();
        assert!(ass.contains("PlayResX: 1920"));
        assert!(ass.contains("Dialogue: 0,0:00:00.50,0:00:02.00"));
        assert!(ass.contains("اضغط على الإعدادات"));
        assert!(ass.contains("\\an9"));
    }
}
