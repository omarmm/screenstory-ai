use crate::timeline::{TimelineAnnotation, TimelineDocument, TimelineZoomSegment};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Media properties obtained from ffprobe before a render plan is accepted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceVideoInfo {
    pub width: u32,
    pub height: u32,
    pub frame_rate_num: u32,
    pub frame_rate_den: u32,
    pub duration_ms: u64,
    pub has_audio: bool,
}

/// A zoom segment converted to exact source-frame boundaries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlannedZoom {
    pub id: String,
    pub start_frame: u64,
    pub end_frame: u64,
    pub center_x: f64,
    pub center_y: f64,
    pub target_scale: f64,
    pub transition_frames: u64,
}

/// Immutable composition inputs. A plan is bound to one saved timeline revision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenderPlan {
    pub schema_version: u32,
    pub renderer_version: String,
    pub timeline_id: String,
    pub recording_id: String,
    pub timeline_revision: u64,
    pub source_width: u32,
    pub source_height: u32,
    pub frame_rate_num: u32,
    pub frame_rate_den: u32,
    pub duration_ms: u64,
    pub source_has_audio: bool,
    pub active_zooms: Vec<PlannedZoom>,
    pub annotations: Vec<TimelineAnnotation>,
}

pub fn build_render_plan(
    timeline: &TimelineDocument,
    source: &SourceVideoInfo,
    renderer_version: &str,
) -> Result<RenderPlan, String> {
    if source.width < 2
        || source.height < 2
        || source.width > 16_384
        || source.height > 16_384
        || source.frame_rate_num == 0
        || source.frame_rate_den == 0
        || source.duration_ms == 0
        || source.duration_ms > 24 * 60 * 60 * 1000
    {
        return Err("The source media properties are outside the supported rendering limits.".into());
    }
    if timeline.source_duration_ms != source.duration_ms
        || timeline.recording_id.is_empty()
        || timeline.revision == 0
        || renderer_version.trim().is_empty()
    {
        return Err("The saved timeline does not match the probed source media.".into());
    }

    let fps = source.frame_rate_num as f64 / source.frame_rate_den as f64;
    let to_frame = |milliseconds: u64| -> u64 {
        ((milliseconds as f64 * fps / 1000.0).round() as u64).min(
            (source.duration_ms as f64 * fps / 1000.0).ceil() as u64,
        )
    };

    let mut active_zooms = timeline
        .zoom_segments
        .iter()
        .filter(|zoom| {
            (zoom.origin == "manual" && zoom.approval_status != "rejected")
                || (zoom.origin == "ai" && zoom.approval_status == "accepted")
        })
        .map(|zoom| planned_zoom(zoom, to_frame))
        .collect::<Result<Vec<_>, _>>()?;
    active_zooms.sort_by_key(|zoom| zoom.start_frame);
    for pair in active_zooms.windows(2) {
        if pair[1].start_frame < pair[0].end_frame {
            return Err("Accepted zoom segments overlap after conversion to source frames.".into());
        }
    }

    if timeline.annotations.len() > 100 {
        return Err("A render may contain no more than 100 text annotations.".into());
    }
    for annotation in &timeline.annotations {
        if annotation.text.trim().is_empty()
            || annotation.text.chars().count() > 500
            || annotation.start_ms >= annotation.end_ms
            || annotation.end_ms > source.duration_ms
            || annotation.font_size < 12
            || annotation.font_size > 72
            || !valid_normalized(annotation.x)
            || !valid_normalized(annotation.y)
            || !valid_normalized(annotation.max_width)
            || annotation.x + annotation.max_width > 1.000001
            || !valid_color(&annotation.foreground)
            || !valid_color(&annotation.background)
        {
            return Err("A timeline annotation contains invalid text, timing, geometry, or color.".into());
        }
    }

    Ok(RenderPlan {
        schema_version: 1,
        renderer_version: renderer_version.to_string(),
        timeline_id: timeline.timeline_id.clone(),
        recording_id: timeline.recording_id.clone(),
        timeline_revision: timeline.revision,
        source_width: source.width,
        source_height: source.height,
        frame_rate_num: source.frame_rate_num,
        frame_rate_den: source.frame_rate_den,
        duration_ms: source.duration_ms,
        source_has_audio: source.has_audio,
        active_zooms,
        annotations: timeline.annotations.clone(),
    })
}

fn planned_zoom(
    zoom: &TimelineZoomSegment,
    to_frame: impl Fn(u64) -> u64,
) -> Result<PlannedZoom, String> {
    if zoom.start_ms >= zoom.end_ms
        || !valid_normalized(zoom.x)
        || !valid_normalized(zoom.y)
        || !valid_normalized(zoom.width)
        || !valid_normalized(zoom.height)
        || zoom.x + zoom.width > 1.000001
        || zoom.y + zoom.height > 1.000001
        || !(1.0..=3.0).contains(&zoom.target_scale)
    {
        return Err("A selected zoom segment has invalid geometry, scale, or timing.".into());
    }
    let start_frame = to_frame(zoom.start_ms);
    let end_frame = to_frame(zoom.end_ms);
    if start_frame >= end_frame {
        return Err("A selected zoom segment is shorter than one source frame.".into());
    }
    let available_transition = (end_frame - start_frame) / 2;
    let transition_frames = if zoom.transition_duration_ms == 0 {
        0
    } else {
        to_frame(zoom.transition_duration_ms).min(available_transition)
    };
    Ok(PlannedZoom {
        id: zoom.id.clone(),
        start_frame,
        end_frame,
        center_x: zoom.x + zoom.width / 2.0,
        center_y: zoom.y + zoom.height / 2.0,
        target_scale: zoom.target_scale,
        transition_frames,
    })
}

fn valid_normalized(value: f64) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

fn valid_color(value: &str) -> bool {
    let hex = value.strip_prefix('#').unwrap_or("");
    (hex.len() == 6 || hex.len() == 8) && hex.chars().all(|character| character.is_ascii_hexdigit())
}

/// Builds a deterministic zoompan filter. All commas are escaped for FFmpeg's
/// filter-option parser; user text is never interpolated into this expression.
pub fn build_zoompan_filter(plan: &RenderPlan) -> Option<String> {
    if plan.active_zooms.is_empty() {
        return None;
    }
    let mut expression = "1".to_string();
    for zoom in plan.active_zooms.iter().rev() {
        let scale = format!("{:.6}", zoom.target_scale);
        let value = if zoom.transition_frames == 0 {
            scale
        } else {
            let transition = zoom.transition_frames;
            format!(
                "1+({scale}-1)*min(1\\,min((on-{})/{transition}\\,({}-on)/{transition}))",
                zoom.start_frame, zoom.end_frame
            )
        };
        expression = format!(
            "if(between(on\\,{}\\,{})\\,{value}\\,{expression})",
            zoom.start_frame, zoom.end_frame
        );
    }
    let zoom = &plan.active_zooms[0];
    let fps = format!("{}/{}", plan.frame_rate_num, plan.frame_rate_den);
    Some(format!(
        "zoompan=z='{expression}':x='(iw-iw/zoom)*{:.6}':y='(ih-ih/zoom)*{:.6}':d=1:s={}x{}:fps={fps}",
        zoom.center_x, zoom.center_y, plan.source_width, plan.source_height
    ))
}

/// FFmpeg filter escaping for app-managed paths. Values remain quoted and are
/// passed as arguments, never through a shell.
pub fn escape_filter_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(character, '\\' | ':' | ',' | ';' | '[' | ']' | '\'') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

fn ffmpeg_color(value: &str) -> String {
    let hex = value.trim_start_matches('#');
    if hex.len() == 8 {
        let alpha = u8::from_str_radix(&hex[6..8], 16).unwrap_or(255) as f64 / 255.0;
        format!("0x{}@{alpha:.3}", &hex[..6])
    } else {
        format!("0x{hex}")
    }
}

/// Builds annotation filters using UTF-8 text files so text is not embedded in
/// filter syntax. Runtime must preflight drawtext text_shaping and Arabic font coverage.
pub fn build_annotation_filters(
    plan: &RenderPlan,
    text_files: &HashMap<String, PathBuf>,
    font_file: &Path,
) -> Result<Vec<String>, String> {
    if plan.annotations.is_empty() {
        return Ok(Vec::new());
    }
    if !font_file.is_file() {
        return Err("A supported font file is required before rendering text annotations.".into());
    }
    let font = escape_filter_path(font_file);
    let mut filters = Vec::with_capacity(plan.annotations.len());
    for annotation in &plan.annotations {
        let text_path = text_files
            .get(&annotation.id)
            .ok_or_else(|| "A managed UTF-8 caption file is missing from the render workspace.".to_string())?;
        if !text_path.is_file() {
            return Err("A managed UTF-8 caption file is missing from the render workspace.".into());
        }
        let text_file = escape_filter_path(text_path);
        let x = match annotation.alignment.as_str() {
            "center" => format!("w*{:.6}+(w*{:.6}-text_w)/2", annotation.x, annotation.max_width),
            "right" => format!("w*{:.6}+w*{:.6}-text_w", annotation.x, annotation.max_width),
            _ => format!("w*{:.6}", annotation.x),
        };
        let start = annotation.start_ms as f64 / 1000.0;
        let end = annotation.end_ms as f64 / 1000.0;
        filters.push(format!(
            "drawtext=fontfile='{font}':textfile='{text_file}':fontsize={}:fontcolor='{}':box=1:boxcolor='{}':boxborderw=10:x='{x}':y='h*{:.6}':enable='between(t\\,{start:.3}\\,{end:.3})':text_shaping=1",
            annotation.font_size,
            ffmpeg_color(&annotation.foreground),
            ffmpeg_color(&annotation.background),
            annotation.y
        ));
    }
    Ok(filters)
}

pub fn build_filter_chain(
    plan: &RenderPlan,
    text_files: &HashMap<String, PathBuf>,
    font_file: &Path,
) -> Result<String, String> {
    let mut filters = Vec::new();
    if let Some(zoom) = build_zoompan_filter(plan) {
        filters.push(zoom);
    }
    filters.extend(build_annotation_filters(plan, text_files, font_file)?);
    Ok(filters.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::{TimelineStep, TimelineZoomSegment};

    fn timeline() -> TimelineDocument {
        TimelineDocument {
            schema_version: 2,
            timeline_id: "t12345-77".into(),
            recording_id: "recording-1.mp4".into(),
            source_file_name: "recording-1.mp4".into(),
            source_duration_ms: 10_000,
            source_analysis_run_id: "a12345-77".into(),
            revision: 7,
            created_at_ms: 1,
            updated_at_ms: 1,
            status: "draft".into(),
            language: "en".into(),
            steps: vec![TimelineStep {
                id: "step-1".into(),
                title: "A step".into(),
                description: "A description".into(),
                start_ms: 0,
                end_ms: 10_000,
                origin: "manual".into(),
                evidence_frame_ids: vec![],
                confidence: None,
                evidence_kind: None,
            }],
            zoom_segments: vec![],
            annotations: vec![],
        }
    }

    fn source() -> SourceVideoInfo {
        SourceVideoInfo {
            width: 1920,
            height: 1080,
            frame_rate_num: 30,
            frame_rate_den: 1,
            duration_ms: 10_000,
            has_audio: true,
        }
    }

    #[test]
    fn plan_keeps_only_accepted_ai_and_manual_zoom_segments() {
        let mut timeline = timeline();
        let zoom = |id: &str, origin: &str, status: &str, start_ms: u64| TimelineZoomSegment {
            id: id.into(),
            start_ms,
            end_ms: start_ms + 2000,
            x: 0.2,
            y: 0.2,
            width: 0.3,
            height: 0.3,
            target_scale: 1.8,
            rationale: "Focus".into(),
            evidence_frame_ids: vec![],
            origin: origin.into(),
            approval_status: status.into(),
            transition_duration_ms: 250,
        };
        timeline.zoom_segments = vec![
            zoom("pending-ai", "ai", "pending", 0),
            zoom("rejected-ai", "ai", "rejected", 2500),
            zoom("accepted-ai", "ai", "accepted", 5000),
            zoom("manual", "manual", "accepted", 7500),
        ];
        let plan = build_render_plan(&timeline, &source(), "screenstory-renderer/1").unwrap();
        assert_eq!(plan.active_zooms.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(), vec!["accepted-ai", "manual"]);
    }

    #[test]
    fn render_plan_is_bound_to_the_saved_revision_and_source_duration() {
        let mut timeline = timeline();
        timeline.revision = 0;
        assert!(build_render_plan(&timeline, &source(), "v1").is_err());
        timeline.revision = 7;
        timeline.source_duration_ms = 9_000;
        assert!(build_render_plan(&timeline, &source(), "v1").is_err());
    }

    #[test]
    fn zoompan_filter_has_deterministic_frame_boundaries_and_eased_transition() {
        let mut timeline = timeline();
        timeline.zoom_segments.push(TimelineZoomSegment {
            id: "accepted".into(), start_ms: 1000, end_ms: 3000,
            x: 0.25, y: 0.25, width: 0.25, height: 0.25, target_scale: 2.0,
            rationale: "Focus".into(), evidence_frame_ids: vec![], origin: "ai".into(),
            approval_status: "accepted".into(), transition_duration_ms: 250,
        });
        let plan = build_render_plan(&timeline, &source(), "v1").unwrap();
        let filter = build_zoompan_filter(&plan).unwrap();
        assert!(filter.contains("between(on\\,30\\,90)"));
        assert!(filter.contains("(on-30)/8"));
        assert!(filter.contains("s=1920x1080"));
    }

    #[test]
    fn annotation_filter_uses_utf8_text_file_and_explicit_arabic_shaping() {
        let mut timeline = timeline();
        timeline.annotations.push(TimelineAnnotation {
            id: "caption-1".into(), text: "اضغط على الإعدادات".into(), language: "ar".into(),
            start_ms: 500, end_ms: 2500, x: 0.1, y: 0.75, max_width: 0.8,
            font_size: 28, foreground: "#FFFFFF".into(), background: "#000000CC".into(),
            alignment: "right".into(), direction: "rtl".into(), origin: "manual".into(),
        });
        let plan = build_render_plan(&timeline, &source(), "v1").unwrap();
        let mut files = HashMap::new();
        files.insert("caption-1".into(), PathBuf::from("/tmp/render/caption-1.txt"));
        // Path existence is enforced at runtime; this unit test exercises safe filter construction.
        let mut temp = std::env::temp_dir();
        temp.push(format!("screenstory-font-proof-{}", std::process::id()));
        std::fs::write(&temp, b"font").unwrap();
        let filters = build_annotation_filters(&plan, &files, &temp).unwrap();
        assert!(filters[0].contains("textfile='/tmp/render/caption-1.txt'"));
        assert!(filters[0].contains("text_shaping=1"));
        assert!(filters[0].contains("between(t\\,0.500\\,2.500)"));
        let _ = std::fs::remove_file(temp);
    }

    #[test]
    fn filter_paths_escape_ffmpeg_delimiters() {
        assert_eq!(
            escape_filter_path(Path::new("/tmp/user's, project:one[1]")),
            "/tmp/user\\'s\\, project\\:one\\[1\\]"
        );
    }

    #[test]
    fn rejects_invalid_annotation_color_and_geometry() {
        let mut timeline = timeline();
        timeline.annotations.push(TimelineAnnotation {
            id: "bad".into(), text: "Hello".into(), language: "en".into(),
            start_ms: 0, end_ms: 1000, x: 0.9, y: 0.9, max_width: 0.5,
            font_size: 20, foreground: "red".into(), background: "#000000".into(),
            alignment: "left".into(), direction: "ltr".into(), origin: "manual".into(),
        });
        assert!(build_render_plan(&timeline, &source(), "v1").is_err());
    }
}
