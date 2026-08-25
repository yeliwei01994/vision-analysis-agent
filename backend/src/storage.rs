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
            let filename = format!("frame-{index:04}-{timestamp_ms}-annotated.jpg");
            let destination = directory.join(&filename);
            if let Err(error) = annotate_frame(source, &destination, detections) {
                eprintln!("failed to annotate evidence frame {}: {error}", source.display());
                tokio::fs::copy(source, &destination).await?;
            }
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

    pub async fn delete_event_evidence(&self, event_id: Uuid) -> std::io::Result<()> {
        match tokio::fs::remove_dir_all(self.root.join("evidence").join(event_id.to_string())).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}

fn annotate_frame(source: &Path, destination: &Path, detections: &[Detection]) -> image::ImageResult<()> {
    let mut image = image::open(source)?.to_rgb8();
    let (width, height) = image.dimensions();
    for detection in detections {
        let legacy_pixels = detection.bbox.iter().any(|value| *value > 1.0);
        let scale_x = if legacy_pixels { 1.0 / width as f32 } else { 1.0 };
        let scale_y = if legacy_pixels { 1.0 / height as f32 } else { 1.0 };
        let x1 = (detection.bbox[0] * scale_x).clamp(0.0, 1.0) * width as f32;
        let y1 = (detection.bbox[1] * scale_y).clamp(0.0, 1.0) * height as f32;
        let x2 = (detection.bbox[2] * scale_x).clamp(0.0, 1.0) * width as f32;
        let y2 = (detection.bbox[3] * scale_y).clamp(0.0, 1.0) * height as f32;
        let (x1, x2) = (x1.min(x2) as u32, x1.max(x2) as u32);
        let (y1, y2) = (y1.min(y2) as u32, y1.max(y2) as u32);
        for thickness in 0..3 {
            for x in x1.saturating_add(thickness)..=x2.saturating_sub(thickness).min(width.saturating_sub(1)) {
                if y1.saturating_add(thickness) < height { image.put_pixel(x, y1 + thickness, image::Rgb([64, 232, 190])); }
                if y2.saturating_sub(thickness) < height { image.put_pixel(x, y2.saturating_sub(thickness), image::Rgb([64, 232, 190])); }
            }
            for y in y1.saturating_add(thickness)..=y2.saturating_sub(thickness).min(height.saturating_sub(1)) {
                if x1.saturating_add(thickness) < width { image.put_pixel(x1 + thickness, y, image::Rgb([64, 232, 190])); }
                if x2.saturating_sub(thickness) < width { image.put_pixel(x2.saturating_sub(thickness), y, image::Rgb([64, 232, 190])); }
            }
        }
    }
    image.save(destination)
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
