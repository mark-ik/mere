//! Endpoint-owned retention for source artifacts attached to clips.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use chirograph::{KnotClipArtifactRoleV1, KnotClipArtifactV1, PortableContentRefV1};
use serde::{Deserialize, Serialize};
use transport::{BlobHash, BlobStore};

static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

/// Portable content identity written into clip provenance.
///
/// New references serialize a shared [`PortableContentRefV1`]: RFC 6920
/// SHA-256 identity beside the BLAKE3 address iroh uses. The public normalized
/// fields preserve the established Knot API, while deserialization also
/// accepts the legacy `urn:blake3` record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotClipEvidenceRef {
    pub content_uri: String,
    pub digest: String,
    pub byte_size: u64,
    pub media_type: String,
    pub canonical_uri: String,
    pub role: KnotClipArtifactRoleV1,
    portable: Option<PortableContentRefV1>,
}

impl KnotClipEvidenceRef {
    fn portable(artifact: &KnotClipArtifactV1) -> Self {
        let content = PortableContentRefV1::of(&artifact.bytes);
        Self {
            content_uri: content.portable_id.to_string(),
            digest: content.transport.to_string(),
            byte_size: content.byte_size,
            media_type: artifact.media_type.clone(),
            canonical_uri: artifact.canonical_uri.clone(),
            role: artifact.role,
            portable: Some(content),
        }
    }

    /// The shared portable reference, absent only for a decoded legacy clip.
    pub fn portable_content(&self) -> Option<&PortableContentRefV1> {
        self.portable.as_ref()
    }

    /// Resolve the portable URI into the transport blob hash it names.
    pub fn blob_hash(&self) -> Result<BlobHash, String> {
        if let Some(content) = &self.portable {
            if self.content_uri != content.portable_id.to_string()
                || self.digest != content.transport.to_string()
                || self.byte_size != content.byte_size
            {
                return Err("clip evidence portable and normalized fields disagree".into());
            }
            return Ok(BlobHash::from_bytes(*content.transport.as_bytes()));
        }
        let named = self
            .content_uri
            .strip_prefix("urn:blake3:")
            .ok_or_else(|| "legacy clip evidence URI is not a urn:blake3 reference".to_string())?;
        if named != self.digest {
            return Err("legacy clip evidence URI and digest disagree".into());
        }
        parse_digest(&self.digest).map(BlobHash::from_bytes)
    }

    /// Check bytes before they are exposed as the retained source artifact.
    pub fn verify_bytes(&self, bytes: &[u8]) -> Result<(), String> {
        if u64::try_from(bytes.len()).ok() != Some(self.byte_size) {
            return Err("clip evidence byte length does not match its reference".into());
        }
        let actual = blake3::hash(bytes);
        if actual.as_bytes() != self.blob_hash()?.as_bytes() {
            return Err("clip evidence bytes do not match their BLAKE3 reference".into());
        }
        if let Some(content) = &self.portable {
            content
                .verify_bytes(bytes)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct PortableEvidenceWire<'a> {
    content: &'a PortableContentRefV1,
    media_type: &'a str,
    canonical_uri: &'a str,
    role: KnotClipArtifactRoleV1,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum EvidenceWire {
    Portable {
        content: PortableContentRefV1,
        media_type: String,
        canonical_uri: String,
        role: KnotClipArtifactRoleV1,
    },
    Legacy {
        content_uri: String,
        digest: String,
        byte_size: u64,
        media_type: String,
        canonical_uri: String,
        role: KnotClipArtifactRoleV1,
    },
}

impl Serialize for KnotClipEvidenceRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if let Some(content) = &self.portable {
            if self.content_uri != content.portable_id.to_string()
                || self.digest != content.transport.to_string()
                || self.byte_size != content.byte_size
            {
                return Err(serde::ser::Error::custom(
                    "clip evidence portable and normalized fields disagree",
                ));
            }
            PortableEvidenceWire {
                content,
                media_type: &self.media_type,
                canonical_uri: &self.canonical_uri,
                role: self.role,
            }
            .serialize(serializer)
        } else {
            serde_json::json!({
                "content_uri": self.content_uri,
                "digest": self.digest,
                "byte_size": self.byte_size,
                "media_type": self.media_type,
                "canonical_uri": self.canonical_uri,
                "role": self.role,
            })
            .serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for KnotClipEvidenceRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(match EvidenceWire::deserialize(deserializer)? {
            EvidenceWire::Portable {
                content,
                media_type,
                canonical_uri,
                role,
            } => Self {
                content_uri: content.portable_id.to_string(),
                digest: content.transport.to_string(),
                byte_size: content.byte_size,
                media_type,
                canonical_uri,
                role,
                portable: Some(content),
            },
            EvidenceWire::Legacy {
                content_uri,
                digest,
                byte_size,
                media_type,
                canonical_uri,
                role,
            } => Self {
                content_uri,
                digest,
                byte_size,
                media_type,
                canonical_uri,
                role,
                portable: None,
            },
        })
    }
}

/// Extract the portable evidence references authored by Knot clip v2 blocks.
///
/// These fenced JSON records are Knot-owned metadata inside an ordinary Djot
/// document. Unknown provenance fields remain forward-compatible; malformed or
/// internally inconsistent evidence references fail closed.
pub fn clip_evidence_references(source: &[u8]) -> Result<Vec<KnotClipEvidenceRef>, String> {
    const OPEN: &str = "```knot.clip.provenance\n";
    const CLOSE: &str = "\n```";

    let source = std::str::from_utf8(source)
        .map_err(|_| "evidence references require a UTF-8 Djot source".to_string())?;
    let mut rest = source;
    let mut references = BTreeMap::<String, KnotClipEvidenceRef>::new();
    while let Some(start) = rest.find(OPEN) {
        let body = &rest[start + OPEN.len()..];
        let end = body
            .find(CLOSE)
            .ok_or_else(|| "clip provenance fence is not closed".to_string())?;
        let value: serde_json::Value = serde_json::from_str(&body[..end])
            .map_err(|error| format!("clip provenance is malformed: {error}"))?;
        if let Some(evidence) = value.get("evidence") {
            let entries = evidence
                .as_array()
                .ok_or_else(|| "clip provenance evidence must be an array".to_string())?;
            for entry in entries {
                let reference: KnotClipEvidenceRef = serde_json::from_value(entry.clone())
                    .map_err(|error| format!("clip evidence reference is malformed: {error}"))?;
                reference.blob_hash()?;
                if let Some(previous) = references.get(&reference.digest) {
                    if previous != &reference {
                        return Err("one clip evidence digest carries conflicting metadata".into());
                    }
                } else {
                    references.insert(reference.digest.clone(), reference);
                }
            }
        }
        rest = &body[end + CLOSE.len()..];
    }
    Ok(references.into_values().collect())
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
        let reference = KnotClipEvidenceRef::portable(artifact);
        debug_assert_eq!(reference.digest, digest);
        debug_assert_eq!(reference.byte_size, byte_size);
        Ok(reference)
    }
}

/// Clip evidence retained in the Murm-owned iroh blob store.
///
/// The async-opened form is shareable with a [`crate::KnotSyncHost`], allowing
/// the same bytes retained by the authoring endpoint to be served over its
/// authenticated p2panda transport without copying them into a document
/// envelope. The synchronous form owns a private runtime for endpoint adapters.
pub struct BlobClipEvidenceStore {
    blobs: Arc<BlobStore>,
    runtime: Option<tokio::runtime::Runtime>,
    max_artifact_bytes: u64,
}

impl BlobClipEvidenceStore {
    /// Open a persistent transport blob store at `root`.
    pub fn open(root: impl AsRef<Path>, max_artifact_bytes: u64) -> Result<Self, String> {
        let runtime = evidence_runtime()?;
        let blobs = runtime
            .block_on(BlobStore::open(root))
            .map(Arc::new)
            .map_err(|error| format!("could not open clip evidence blob store: {error}"))?;
        Ok(Self {
            runtime: Some(runtime),
            blobs,
            max_artifact_bytes,
        })
    }

    /// Open a persistent store on the resident host's async runtime.
    pub async fn open_async(
        root: impl AsRef<Path>,
        max_artifact_bytes: u64,
    ) -> Result<Self, String> {
        let blobs = BlobStore::open(root)
            .await
            .map(Arc::new)
            .map_err(|error| format!("could not open clip evidence blob store: {error}"))?;
        Ok(Self {
            runtime: None,
            blobs,
            max_artifact_bytes,
        })
    }

    /// Shared store handle for a resident p2p transport.
    ///
    /// Only [`Self::open_async`] binds the backing actor to the resident
    /// runtime. The synchronous adapter deliberately cannot escape its private
    /// runtime into a longer-lived host.
    pub fn resident_blob_store(&self) -> Result<Arc<BlobStore>, String> {
        if self.runtime.is_some() {
            return Err("synchronous clip evidence store cannot escape its private runtime".into());
        }
        Ok(Arc::clone(&self.blobs))
    }

    /// Retain from an async resident host without nesting runtimes.
    pub async fn retain_async(
        &self,
        artifact: &KnotClipArtifactV1,
    ) -> Result<KnotClipEvidenceRef, String> {
        retain_blob_artifact(&self.blobs, self.max_artifact_bytes, artifact).await
    }
}

impl KnotClipEvidenceStore for BlobClipEvidenceStore {
    fn retain(&mut self, artifact: &KnotClipArtifactV1) -> Result<KnotClipEvidenceRef, String> {
        let runtime = self.runtime.as_ref().ok_or_else(|| {
            "async clip evidence store requires retain_async on its resident runtime".to_string()
        })?;
        runtime.block_on(retain_blob_artifact(
            &self.blobs,
            self.max_artifact_bytes,
            artifact,
        ))
    }
}

async fn retain_blob_artifact(
    blobs: &BlobStore,
    max_artifact_bytes: u64,
    artifact: &KnotClipArtifactV1,
) -> Result<KnotClipEvidenceRef, String> {
    let byte_size = u64::try_from(artifact.bytes.len())
        .map_err(|_| "clip artifact byte length does not fit u64".to_string())?;
    if byte_size > max_artifact_bytes {
        return Err(format!(
            "clip artifact is {byte_size} bytes; configured evidence limit is {max_artifact_bytes}"
        ));
    }
    let digest = blake3::hash(&artifact.bytes);
    let digest_hex = digest.to_hex().to_string();
    let tag = format!("knot/clip-evidence/{digest_hex}");
    let stored = blobs
        .put_bytes_named(artifact.bytes.clone(), tag.as_bytes())
        .await
        .map_err(|error| format!("could not retain clip evidence in blob store: {error}"))?;
    if stored.as_bytes() != digest.as_bytes() {
        return Err("transport blob store returned the wrong clip evidence digest".into());
    }
    blobs
        .flush()
        .await
        .map_err(|error| format!("could not flush retained clip evidence: {error}"))?;
    let reference = KnotClipEvidenceRef::portable(artifact);
    if reference.digest != digest_hex || reference.byte_size != byte_size {
        return Err("portable clip evidence disagrees with its retained transport bytes".into());
    }
    Ok(reference)
}

fn evidence_runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("could not create clip evidence runtime: {error}"))
}

fn parse_digest(value: &str) -> Result<[u8; 32], String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("clip evidence digest must be 64 lowercase hexadecimal characters".into());
    }
    let mut digest = [0u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let hex = std::str::from_utf8(pair).expect("ASCII hex was already checked");
        digest[index] = u8::from_str_radix(hex, 16)
            .map_err(|_| "clip evidence digest is not hexadecimal".to_string())?;
    }
    Ok(digest)
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
        assert!(
            first
                .content_uri
                .starts_with(chirograph::Sha256NamedInformation::PREFIX)
        );
        assert_eq!(
            first.portable_content().unwrap().transport.to_string(),
            first.digest
        );
    }

    #[test]
    fn transport_blob_store_retains_and_reopens_verified_evidence() {
        let temp = tempdir().unwrap();
        let artifact = KnotClipArtifactV1 {
            role: KnotClipArtifactRoleV1::SourceResponse,
            media_type: "text/html".into(),
            canonical_uri: "https://example.test/post".into(),
            bytes: b"<p>portable evidence</p>".to_vec(),
        };
        let reference = {
            let mut store = BlobClipEvidenceStore::open(temp.path(), 1024).unwrap();
            let reference = store.retain(&artifact).unwrap();
            let bytes = store
                .runtime
                .as_ref()
                .unwrap()
                .block_on(store.blobs.get_bytes(reference.blob_hash().unwrap()))
                .unwrap();
            reference.verify_bytes(&bytes).unwrap();
            reference
        };

        let reopened = BlobClipEvidenceStore::open(temp.path(), 1024).unwrap();
        let bytes = reopened
            .runtime
            .as_ref()
            .unwrap()
            .block_on(reopened.blobs.get_bytes(reference.blob_hash().unwrap()))
            .unwrap();
        reference.verify_bytes(&bytes).unwrap();
        let mut tampered = bytes.to_vec();
        tampered[0] ^= 1;
        assert!(reference.verify_bytes(&tampered).is_err());
    }

    #[test]
    fn djot_provenance_yields_deduplicated_validated_references() {
        let bytes = b"portable evidence";
        let digest = blake3::hash(bytes).to_hex().to_string();
        let reference = KnotClipEvidenceRef {
            content_uri: format!("urn:blake3:{digest}"),
            digest,
            byte_size: bytes.len() as u64,
            media_type: "text/plain".into(),
            canonical_uri: "https://example.test/evidence".into(),
            role: KnotClipArtifactRoleV1::SourceResponse,
            portable: None,
        };
        let provenance = serde_json::json!({
            "schema": "knot.clip.insert/v2",
            "evidence": [reference.clone(), reference.clone()]
        });
        let source = format!(
            "# Note\n\n```knot.clip.provenance\n{}\n```\n",
            serde_json::to_string(&provenance).unwrap()
        );
        let decoded = clip_evidence_references(source.as_bytes()).unwrap();
        assert_eq!(decoded, vec![reference]);
        assert!(decoded[0].portable_content().is_none());
        decoded[0].verify_bytes(bytes).unwrap();
    }

    #[test]
    fn portable_and_transport_hashes_must_both_match() {
        let artifact = KnotClipArtifactV1 {
            role: KnotClipArtifactRoleV1::SourceResponse,
            media_type: "text/plain".into(),
            canonical_uri: "https://example.test/evidence".into(),
            bytes: b"portable evidence".to_vec(),
        };
        let reference = KnotClipEvidenceRef::portable(&artifact);
        let mut value = serde_json::to_value(&reference).unwrap();
        value["content"]["portable_id"] =
            serde_json::to_value(chirograph::Sha256NamedInformation::of(b"different bytes"))
                .unwrap();
        let conflicting: KnotClipEvidenceRef = serde_json::from_value(value).unwrap();
        assert!(
            conflicting.blob_hash().is_ok(),
            "the transport hash is valid"
        );
        assert!(
            conflicting.verify_bytes(&artifact.bytes).is_err(),
            "the conflicting portable identity fails closed",
        );
    }
}
