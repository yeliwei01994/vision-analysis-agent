use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::domain::{Detection, Evidence, EvidenceFrame};
use crate::video;

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

    pub async fn create_upload_temp(&self, _filename: &str) -> std::io::Result<PathBuf> {
        tokio::fs::create_dir_all(&self.root).await?;
        Ok(self.root.join(format!(".upload-{}.part", Uuid::new_v4())))
    }

    pub async fn finalize_upload(&self, temporary: PathBuf, filename: &str) -> std::io::Result<PathBuf> {
        let destination = self.root.join(sanitize_filename(filename));
        let _ = tokio::fs::remove_file(&destination).await;
        tokio::fs::rename(&temporary, &destination).await?;
        Ok(destination)
    }

    pub async fn discard_upload_temp(&self, temporary: &Path) {
        let _ = tokio::fs::remove_file(temporary).await;
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
            if let Err(error) = annotate_frame(source, &destination, detections, *timestamp_ms) {
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

    pub async fn save_annotated_video(
        &self,
        job_id: Uuid,
        frames: &[crate::rules::FrameDetection],
        duration_ms: u64,
    ) -> Result<String, String> {
        let output_directory = self.root.join("annotated");
        tokio::fs::create_dir_all(&output_directory)
            .await
            .map_err(|error| error.to_string())?;
        let temporary = std::env::temp_dir().join(format!("vision-annotated-{job_id}"));
        tokio::fs::create_dir_all(&temporary)
            .await
            .map_err(|error| error.to_string())?;
        let mut grouped: BTreeMap<PathBuf, (u64, Vec<Detection>)> = BTreeMap::new();
        for frame in frames {
            let Some(path) = frame.frame_path.as_ref() else { continue; };
            let entry = grouped.entry(path.clone()).or_insert_with(|| (frame.timestamp_ms, Vec::new()));
            entry.1.push(frame.detection.clone());
        }
        if grouped.is_empty() {
            let _ = tokio::fs::remove_dir_all(&temporary).await;
            return Err("没有可用于生成回放的视频帧".into());
        }
        for (index, (source, (timestamp_ms, detections))) in grouped.into_iter().enumerate() {
            let destination = temporary.join(format!("frame-{:010}.jpg", index + 1));
            annotate_frame(&source, &destination, &detections, timestamp_ms)
                .map_err(|error| format!("标注视频帧失败：{error}"))?;
        }
        let output = output_directory.join(format!("{job_id}.mp4"));
        let result = video::encode_frames(&temporary, &output, duration_ms).await;
        let _ = tokio::fs::remove_dir_all(&temporary).await;
        result.map(|_| format!("/media/annotated/{job_id}.mp4"))
    }

    pub async fn delete_annotated_video(&self, job_id: Uuid) -> std::io::Result<()> {
        match tokio::fs::remove_file(self.root.join("annotated").join(format!("{job_id}.mp4"))).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}

fn annotate_frame(source: &Path, destination: &Path, detections: &[Detection], timestamp_ms: u64) -> image::ImageResult<()> {
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
        draw_text(&mut image, x1, y1.saturating_sub(9), &format!("{} {:.0}%", detection.class_name, detection.confidence * 100.0), image::Rgb([255, 230, 90]));
    }
    draw_text(&mut image, 6, 6, &format!("{:02}:{:02}.{:03}", timestamp_ms / 60_000, timestamp_ms / 1_000 % 60, timestamp_ms % 1_000), image::Rgb([255, 255, 255]));
    image.save(destination)
}

fn draw_text(image: &mut image::RgbImage, x: u32, y: u32, text: &str, color: image::Rgb<u8>) {
    for (index, character) in text.to_ascii_lowercase().chars().enumerate() {
        let glyph = glyph(character);
        let origin_x = x + index as u32 * 6;
        for (row, bits) in glyph.iter().enumerate() {
            for column in 0..5 {
                if bits & (1 << (4 - column)) != 0 {
                    let px = origin_x + column;
                    let py = y + row as u32;
                    if px < image.width() && py < image.height() { image.put_pixel(px, py, color); }
                }
            }
        }
    }
}

fn glyph(character: char) -> [u8; 7] {
    match character {
        'a' => [0b01110, 0b00001, 0b01111, 0b10001, 0b01111, 0, 0],
        'b' => [0b10000, 0b10000, 0b11110, 0b10001, 0b11110, 0, 0],
        'c' => [0, 0, 0b01111, 0b10000, 0b01111, 0, 0],
        'd' => [0b00001, 0b00001, 0b01111, 0b10001, 0b01111, 0, 0],
        'e' => [0, 0, 0b01110, 0b11111, 0b01110, 0, 0],
        'n' => [0, 0, 0b11110, 0b10001, 0b10001, 0, 0],
        'o' => [0, 0, 0b01110, 0b10001, 0b01110, 0, 0],
        'p' => [0, 0, 0b11110, 0b10001, 0b11110, 0b10000, 0],
        'r' => [0, 0, 0b10110, 0b11000, 0b10000, 0, 0],
        's' => [0, 0, 0b01111, 0b11000, 0b11110, 0, 0],
        't' => [0b01000, 0b01000, 0b11111, 0b01000, 0b00111, 0, 0],
        'v' => [0, 0, 0b10001, 0b10001, 0b01010, 0b00100, 0],
        '0' => [0b01110, 0b10011, 0b10101, 0b11001, 0b01110, 0, 0],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b01110, 0, 0],
        '2' => [0b01110, 0b10001, 0b00110, 0b01000, 0b11111, 0, 0],
        '3' => [0b11110, 0b00001, 0b01110, 0b00001, 0b11110, 0, 0],
        '4' => [0b00010, 0b00110, 0b01010, 0b11111, 0b00010, 0, 0],
        '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b11110, 0, 0],
        '6' => [0b01110, 0b10000, 0b11110, 0b10001, 0b01110, 0, 0],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b00100, 0, 0],
        '8' => [0b01110, 0b10001, 0b01110, 0b10001, 0b01110, 0, 0],
        '9' => [0b01110, 0b10001, 0b01111, 0b00001, 0b01110, 0, 0],
        '%' => [0b11001, 0b11010, 0b00100, 0b01011, 0b10011, 0, 0],
        ':' => [0, 0b00100, 0, 0, 0b00100, 0, 0],
        '.' => [0, 0, 0, 0, 0, 0b00100, 0],
        _ => [0; 7],
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
