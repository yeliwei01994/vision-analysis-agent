use std::path::{Path, PathBuf};

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
