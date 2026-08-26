use std::path::{Path, PathBuf};
use tokio::process::Command;
use uuid::Uuid;

pub const DETECTION_INTERVAL_MS: u64 = 200;
pub const REPLAY_FPS: u32 = 10;

pub fn detection_interval_from(value: Option<&str>) -> u64 {
    value
        .and_then(|raw| raw.parse::<u64>().ok())
        .unwrap_or(DETECTION_INTERVAL_MS)
        .clamp(100, 5000)
}

pub fn detection_interval_ms() -> u64 {
    detection_interval_from(std::env::var("DETECTION_INTERVAL_MS").ok().as_deref())
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct VideoMetadata {
    pub duration_ms: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub frame_rate: Option<f64>,
    pub frame_count: Option<u64>,
    pub probe_source: String,
}

pub async fn probe(path: &Path) -> VideoMetadata {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(path)
        .output()
        .await;
    match output {
        Ok(result) if result.status.success() => parse_probe(&result.stdout),
        _ => VideoMetadata {
            probe_source: "unavailable".into(),
            ..Default::default()
        },
    }
}

pub fn frame_timestamp_ms(filename: &str) -> Option<u64> {
    let stem = Path::new(filename).file_stem()?.to_str()?;
    stem.strip_prefix("frame-")?.parse().ok()
}

pub fn parse_frame_rate(value: &str) -> Option<f64> {
    let (numerator, denominator) = value.split_once('/')?;
    let numerator = numerator.parse::<f64>().ok()?;
    let denominator = denominator.parse::<f64>().ok()?;
    (denominator > 0.0).then_some(numerator / denominator)
}

pub fn playback_duration_ms(source_duration_ms: u64, job_duration_ms: u64) -> u64 {
    if source_duration_ms > 0 {
        source_duration_ms
    } else {
        job_duration_ms
    }
}

pub async fn extract_frames(
    path: &Path,
    interval_ms: u64,
) -> Result<(PathBuf, Vec<PathBuf>), String> {
    let directory = std::env::temp_dir().join(format!("vision-frames-{}", Uuid::new_v4()));
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|error| error.to_string())?;
    let fps = 1000.0 / interval_ms.max(1) as f64;
    let pattern = directory.join("frame-%010d.jpg");
    let output = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(path)
        .args(["-vf", &format!("fps={fps}"), "-q:v", "3"])
        .arg(&pattern)
        .output()
        .await
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        let _ = tokio::fs::remove_dir_all(&directory).await;
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let mut entries = tokio::fs::read_dir(&directory)
        .await
        .map_err(|error| error.to_string())?;
    let mut frames = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| error.to_string())?
    {
        if entry.path().extension().and_then(|value| value.to_str()) == Some("jpg") {
            frames.push(entry.path());
        }
    }
    frames.sort();
    Ok((directory, frames))
}

pub async fn encode_frames(
    input_directory: &Path,
    output_path: &Path,
    duration_ms: u64,
    output_fps: f64,
    output_frame_count: Option<u64>,
) -> Result<(), String> {
    let pattern = input_directory.join("frame-%010d.jpg");
    let duration = format!("{:.3}", duration_ms as f64 / 1000.0);
    let fps = format!("{output_fps:.6}");
    let frame_count = output_frame_count.map(|value| value.to_string());
    let mut command = Command::new("ffmpeg");
    command
        .args(["-hide_banner", "-loglevel", "error", "-y", "-framerate", &fps, "-start_number", "1", "-i"])
        .arg(&pattern)
        .args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-movflags", "+faststart", "-t"])
        .arg(duration)
        ;
    if let Some(frame_count) = frame_count.as_deref() {
        command.args(["-frames:v", frame_count]);
    }
    let output = command
        .arg(output_path)
        .output()
        .await
        .map_err(|error| format!("无法启动 ffmpeg：{error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn parse_probe(bytes: &[u8]) -> VideoMetadata {
    let value: serde_json::Value = serde_json::from_slice(bytes).unwrap_or_default();
    let duration_ms = value
        .pointer("/format/duration")
        .and_then(|v| v.as_str())
        .and_then(|v| v.parse::<f64>().ok())
        .map(|v| (v * 1000.0) as u64)
        .unwrap_or_default();
    let stream = value
        .get("streams")
        .and_then(|v| v.as_array())
        .and_then(|streams| {
            streams
                .iter()
                .find(|stream| stream.get("codec_type").and_then(|v| v.as_str()) == Some("video"))
        });
    VideoMetadata {
        duration_ms,
        width: stream
            .and_then(|v| v.get("width"))
            .and_then(|v| v.as_u64())
            .map(|v| v as u32),
        height: stream
            .and_then(|v| v.get("height"))
            .and_then(|v| v.as_u64())
            .map(|v| v as u32),
        frame_rate: stream
            .and_then(|v| v.get("avg_frame_rate"))
            .and_then(|v| v.as_str())
            .and_then(parse_frame_rate),
        frame_count: stream
            .and_then(|v| v.get("nb_frames"))
            .and_then(|v| v.as_str())
            .and_then(|v| v.parse::<u64>().ok()),
        probe_source: "ffprobe".into(),
    }
}
