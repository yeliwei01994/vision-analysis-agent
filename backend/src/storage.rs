use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::domain::{Detection, Evidence, EvidenceFrame};

#[derive(Clone)]
pub struct MediaStorage {
    root: PathBuf,
}

impl Default for MediaStorage {
    fn default() -> Self {
        Self::new(PathBuf::from("media"))
    }
}

impl MediaStorage {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub async fn save_upload(&self, filename: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
        tokio::fs::create_dir_all(&self.root).await?;
        let safe_name = sanitize_filename(filename);
        let path = self.root.join(safe_name);
        tokio::fs::write(&path, bytes).await?;
        Ok(path)
    }

    pub async fn save_event_evidence(
        &self,
        event_id: Uuid,
        frames: &[(u64, &Path, Vec<Detection>)],
    ) -> std::io::Result<Evidence> {
        let directory = self.root.join("evidence").join(event_id.to_string());
        tokio::fs::create_dir_all(&directory).await?;
        let mut evidence_frames = Vec::with_capacity(frames.len());
        for (index, (timestamp_ms, source, detections)) in frames.iter().enumerate() {
            let filename = format!("frame-{index:04}-{timestamp_ms}.jpg");
            tokio::fs::copy(source, directory.join(&filename)).await?;
            evidence_frames.push(EvidenceFrame {
                timestamp_ms: *timestamp_ms,
                image_url: format!("/media/evidence/{event_id}/{filename}"),
                detections: detections.clone(),
            });
        }
        let frame_urls = evidence_frames.iter().map(|frame| frame.image_url.clone()).collect();
        let thumbnail_url = evidence_frames.first().map(|frame| frame.image_url.clone());
        Ok(Evidence { thumbnail_url, clip_url: None, frame_urls, frames: evidence_frames })
    }
}

pub fn sanitize_filename(filename: &str) -> String {
    let basename = Path::new(filename)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("upload.bin");
    let cleaned: String = basename
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "upload.bin".into()
    } else {
        cleaned
    }
}
