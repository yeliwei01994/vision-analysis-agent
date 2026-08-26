use crate::{domain::Detection, rules::FrameDetection};
use reqwest::{multipart, Client, Url};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const DEFAULT_BATCH_SIZE: usize = 4;
pub const DEFAULT_CONCURRENCY: usize = 2;

pub fn batch_size_from(value: Option<&str>) -> usize {
    value
        .and_then(|raw| raw.parse::<usize>().ok())
        .unwrap_or(DEFAULT_BATCH_SIZE)
        .clamp(1, 16)
}

pub fn concurrency_from(value: Option<&str>) -> usize {
    value
        .and_then(|raw| raw.parse::<usize>().ok())
        .unwrap_or(DEFAULT_CONCURRENCY)
        .clamp(1, 4)
}

pub fn batch_size() -> usize {
    batch_size_from(std::env::var("YOLO_BATCH_SIZE").ok().as_deref())
}

pub fn concurrency() -> usize {
    concurrency_from(std::env::var("YOLO_CONCURRENCY").ok().as_deref())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoloDetection {
    pub class_name: String,
    pub confidence: f32,
    pub bbox: [f32; 4],
    pub track_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoloResponse {
    pub model_version: String,
    pub timestamp_ms: u64,
    pub detections: Vec<YoloDetection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoloBatchItem {
    pub frame_id: String,
    pub timestamp_ms: u64,
    pub detections: Vec<YoloDetection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoloBatchResponse {
    pub model_version: String,
    pub items: Vec<YoloBatchItem>,
}

impl YoloBatchResponse {
    pub fn into_frame_results(
        self,
        paths: &[PathBuf],
    ) -> Result<Vec<(PathBuf, YoloResponse)>, String> {
        let mut results = Vec::with_capacity(paths.len());
        for path in paths {
            let frame_id = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| format!("frame path has no valid file name: {}", path.display()))?;
            let item = self
                .items
                .iter()
                .find(|item| item.frame_id == frame_id)
                .ok_or_else(|| format!("YOLO batch response is missing frame {frame_id}"))?;
            results.push((
                path.clone(),
                YoloResponse {
                    model_version: self.model_version.clone(),
                    timestamp_ms: item.timestamp_ms,
                    detections: item.detections.clone(),
                },
            ));
        }
        Ok(results)
    }
}

impl YoloResponse {
    pub fn into_frame_detections(self) -> Vec<FrameDetection> {
        self.detections
            .into_iter()
            .map(|item| FrameDetection {
                timestamp_ms: self.timestamp_ms,
                detection: Detection {
                    class_name: item.class_name,
                    confidence: item.confidence,
                    bbox: item.bbox,
                    track_id: item.track_id,
                },
                frame_path: None,
            })
            .collect()
    }
}

#[derive(Clone)]
pub struct YoloDetector {
    client: Client,
    endpoint: Url,
    batch_endpoint: Url,
    timeout: Duration,
}

impl YoloDetector {
    pub fn from_env() -> Result<Self, reqwest::Error> {
        Self::new(std::env::var("YOLO_URL").unwrap_or_else(|_| "http://localhost:9000".into()))
    }

    pub fn new(base_url: impl AsRef<str>) -> Result<Self, reqwest::Error> {
        let base_url = base_url.as_ref().trim_end_matches('/');
        let endpoint = Url::parse(&format!(
            "{}/v1/infer/frame",
            base_url
        ))
        .expect("YOLO_URL must be a valid URL");
        let batch_endpoint = Url::parse(&format!(
            "{}/v1/infer/batch",
            base_url
        ))
        .expect("YOLO_URL must be a valid URL");
        Ok(Self {
            client: Client::builder().build()?,
            endpoint,
            batch_endpoint,
            timeout: Duration::from_secs(60),
        })
    }

    pub async fn detect_frame(
        &self,
        path: &Path,
        timestamp_ms: u64,
    ) -> Result<YoloResponse, String> {
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|error| error.to_string())?;
        let file = multipart::Part::bytes(bytes)
            .file_name("frame.jpg")
            .mime_str("image/jpeg")
            .map_err(|error| error.to_string())?;
        let form = multipart::Form::new()
            .part("file", file)
            .text("timestamp_ms", timestamp_ms.to_string());
        let response = self
            .client
            .post(self.endpoint.clone())
            .timeout(self.timeout)
            .multipart(form)
            .send()
            .await
            .map_err(|error| error.to_string())?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!("YOLO returned {status}"));
        }
        response
            .json::<YoloResponse>()
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn detect_batch(
        &self,
        frames: &[(PathBuf, u64)],
    ) -> Result<Vec<(PathBuf, YoloResponse)>, String> {
        if frames.is_empty() {
            return Ok(Vec::new());
        }

        let mut form = multipart::Form::new();
        let mut metadata = Vec::with_capacity(frames.len());
        for (path, timestamp_ms) in frames {
            let frame_id = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| format!("frame path has no valid file name: {}", path.display()))?
                .to_string();
            let bytes = tokio::fs::read(path)
                .await
                .map_err(|error| error.to_string())?;
            let file = multipart::Part::bytes(bytes)
                .file_name(frame_id.clone())
                .mime_str("image/jpeg")
                .map_err(|error| error.to_string())?;
            form = form.part("files", file);
            metadata.push(serde_json::json!({
                "frame_id": frame_id,
                "timestamp_ms": timestamp_ms,
            }));
        }
        form = form.text("metadata", serde_json::to_string(&metadata).map_err(|error| error.to_string())?);

        let response = self
            .client
            .post(self.batch_endpoint.clone())
            .timeout(self.timeout)
            .multipart(form)
            .send()
            .await
            .map_err(|error| error.to_string())?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!("YOLO batch returned {status}"));
        }
        let response = response
            .json::<YoloBatchResponse>()
            .await
            .map_err(|error| error.to_string())?;
        response.into_frame_results(
            &frames
                .iter()
                .map(|(path, _)| path.clone())
                .collect::<Vec<_>>(),
        )
    }
}
