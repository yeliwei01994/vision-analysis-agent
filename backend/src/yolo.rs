use crate::{domain::Detection, rules::FrameDetection};
use reqwest::{multipart, Client, Url};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

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
    timeout: Duration,
}

impl YoloDetector {
    pub fn from_env() -> Result<Self, reqwest::Error> {
        Self::new(std::env::var("YOLO_URL").unwrap_or_else(|_| "http://localhost:9000".into()))
    }

    pub fn new(base_url: impl AsRef<str>) -> Result<Self, reqwest::Error> {
        let endpoint = Url::parse(&format!(
            "{}/v1/infer/frame",
            base_url.as_ref().trim_end_matches('/')
        ))
        .expect("YOLO_URL must be a valid URL");
        Ok(Self {
            client: Client::builder().build()?,
            endpoint,
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
}
