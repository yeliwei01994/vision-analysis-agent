use std::path::{Path, PathBuf};
use tokio::process::Command;
use uuid::Uuid;

pub const DETECTION_INTERVAL_MS: u64 = 100;
pub const REPLAY_FPS: u32 = 10;

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct VideoMetadata {
    pub duration_ms: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
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
) -> Result<(), String> {
    let pattern = input_directory.join("frame-%010d.jpg");
    let duration = format!("{:.3}", duration_ms as f64 / 1000.0);
    let output = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y", "-framerate", &REPLAY_FPS.to_string(), "-start_number", "1", "-i"])
        .arg(&pattern)
        .args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-movflags", "+faststart", "-t"])
        .arg(duration)
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
        probe_source: "ffprobe".into(),
    }
}
