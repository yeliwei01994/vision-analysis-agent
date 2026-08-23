use std::path::Path;
use tokio::process::Command;

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
