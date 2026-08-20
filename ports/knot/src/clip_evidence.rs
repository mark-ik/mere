//! Endpoint-owned retention for source artifacts attached to clips.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use chirograph::{KnotClipArtifactRoleV1, KnotClipArtifactV1};
use serde::Serialize;

static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

/// Portable content identity written into clip provenance.
///
/// Locations are intentionally absent. `urn:blake3:` remains valid if a local
/// file is later offered over iroh, HTTPS, removable media, or another carrier.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct KnotClipEvidenceRef {
    pub content_uri: String,
    pub digest: String,
    pub byte_size: u64,
    pub media_type: String,
    pub canonical_uri: String,
    pub role: KnotClipArtifactRoleV1,
}

/// Host-injected authority for retaining clip evidence.
pub trait KnotClipEvidenceStore: Send {
    /// Retain an artifact under its content identity. Implementations must be
    /// idempotent for identical bytes.
    fn retain(&mut self, artifact: &KnotClipArtifactV1) -> Result<KnotClipEvidenceRef, String>;
}

/// A content-addressed local store rooted at an explicitly configured path.
pub struct FileClipEvidenceStore {
    root: PathBuf,
    max_artifact_bytes: u64,
}

impl FileClipEvidenceStore {
    pub fn new(root: impl Into<PathBuf>, max_artifact_bytes: u64) -> Self {
        Self {
            root: root.into(),
            max_artifact_bytes,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn artifact_path(&self, digest: &str) -> PathBuf {
        self.root.join("blake3").join(digest)
    }
}

impl KnotClipEvidenceStore for FileClipEvidenceStore {
    fn retain(&mut self, artifact: &KnotClipArtifactV1) -> Result<KnotClipEvidenceRef, String> {
        let byte_size = u64::try_from(artifact.bytes.len())
            .map_err(|_| "clip artifact byte length does not fit u64".to_string())?;
        if byte_size > self.max_artifact_bytes {
            return Err(format!(
                "clip artifact is {byte_size} bytes; configured evidence limit is {}",
                self.max_artifact_bytes
            ));
        }
        let digest = blake3::hash(&artifact.bytes).to_hex().to_string();
        let path = self.artifact_path(&digest);
        if path.exists() {
            let existing = fs::read(&path)
                .map_err(|error| format!("could not verify retained clip evidence: {error}"))?;
            if existing != artifact.bytes {
                return Err("retained clip evidence does not match its BLAKE3 address".into());
            }
        } else {
            let parent = path
                .parent()
                .ok_or_else(|| "clip evidence path has no parent".to_string())?;
            fs::create_dir_all(parent)
                .map_err(|error| format!("could not create clip evidence directory: {error}"))?;
            let temporary = parent.join(format!(
                ".{digest}.{}.{}.tmp",
                std::process::id(),
                NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed)
            ));
            if let Err(error) = write_new(&temporary, &artifact.bytes) {
                let _ = fs::remove_file(&temporary);
                return Err(format!("could not stage clip evidence: {error}"));
            }
            match fs::rename(&temporary, &path) {
                Ok(()) => {}
                Err(error) if path.exists() => {
                    let _ = fs::remove_file(&temporary);
                    let existing = fs::read(&path).map_err(|read_error| {
                        format!(
                            "clip evidence raced with another writer ({error}) and could not be verified: {read_error}"
                        )
                    })?;
                    if existing != artifact.bytes {
                        return Err(
                            "retained clip evidence does not match its BLAKE3 address".into()
                        );
                    }
                }
                Err(error) => {
                    let _ = fs::remove_file(&temporary);
                    return Err(format!("could not install clip evidence: {error}"));
                }
            }
        }
        Ok(KnotClipEvidenceRef {
            content_uri: format!("urn:blake3:{digest}"),
            digest,
            byte_size,
            media_type: artifact.media_type.clone(),
            canonical_uri: artifact.canonical_uri.clone(),
            role: artifact.role,
        })
    }
}

fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn file_store_is_content_addressed_and_idempotent() {
        let temp = tempdir().unwrap();
        let mut store = FileClipEvidenceStore::new(temp.path(), 1024);
        let artifact = KnotClipArtifactV1 {
            role: KnotClipArtifactRoleV1::SourceResponse,
            media_type: "text/html".into(),
            canonical_uri: "https://example.test/post".into(),
            bytes: b"<p>evidence</p>".to_vec(),
        };
        let first = store.retain(&artifact).unwrap();
        let second = store.retain(&artifact).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            fs::read(store.artifact_path(&first.digest)).unwrap(),
            artifact.bytes
        );
        assert_eq!(first.content_uri, format!("urn:blake3:{}", first.digest));
    }
}
