/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Layer 2 — typed metadata about blobs.
//!
//! A [`BlobManifest`] describes a blob without reading it. It carries content
//! hash, size, schema reference, source list, timestamps, and the three-axis
//! classification (privacy / provenance / trust). Manifests are themselves
//! serialized (as JSON) to the Layer 1 [`Store`](crate::Store) under a known
//! prefix; the blob layer doesn't know they're manifests.
//!
//! ## Sources and the [`BlobFetcher`] split
//!
//! A manifest carries a *list* of sources where its blob can be obtained.
//! [`resolve_blob`] walks them in order:
//!
//! - [`BlobSource::Embedded`] — bytes are inline; return them directly.
//! - [`BlobSource::Local`] — bytes live in the [`Store`](crate::Store);
//!   `resolve_blob` loads them.
//! - Anything else (`Iroh`, `Https`, `LocalFile`, `LocalOnlyRef`) — eidetic
//!   delegates to a [`BlobFetcher`]. Concrete fetchers (an iroh-blobs
//!   fetcher, an HTTP fetcher) live in companion crates so eidetic stays
//!   wasm-friendly and dependency-light.
//!
//! Hash verification (BLAKE3) runs on every successful fetch before the
//! bytes are returned or cached.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::schema::{
    Hash, ManifestId, PrivacyClass, ProvenanceRecord, SchemaRef, Timestamp, TrustEnvelope,
};
use crate::{Error, Result, Store};

/// Where a blob can be obtained. The variant carries semantic meaning beyond
/// routing: [`BlobSource::LocalOnlyRef`] literally cannot leave the host
/// (safety rail for slicing — see `to_transferable`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlobSource {
    /// Bytes embedded directly in the manifest (small metadata blobs only).
    Embedded { bytes: Vec<u8>, media_type: String },
    /// Cached locally in the [`Store`](crate::Store) under this key.
    Local { key: String },
    /// Fetchable via an iroh blob ticket. iroh verifies BLAKE3 natively.
    Iroh { ticket: String },
    /// Fetchable via HTTPS (e.g. HuggingFace Hub).
    Https { url: String },
    /// User-provided file path on the host (not wasm-portable).
    LocalFile { path: String },
    /// Local-only reference; never serialized for transfer.
    LocalOnlyRef { local_ref: String },
}

impl BlobSource {
    /// True if this source is safe to include in a manifest serialized for
    /// transfer to another host. `LocalOnlyRef` is the only variant that
    /// returns false.
    pub fn is_transferable(&self) -> bool {
        !matches!(self, BlobSource::LocalOnlyRef { .. })
    }
}

/// Layer 2 typed metadata describing a blob in the [`Store`](crate::Store).
///
/// The manifest is serialized as JSON for inspectability and forward-compat
/// (per the design pass §11 resolutions). The [`manifest_version`] field
/// gates lazy migrations on read.
///
/// [`manifest_version`]: BlobManifest::manifest_version
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobManifest {
    /// Stable identifier — equals the blob's BLAKE3 content hash.
    pub id: ManifestId,
    /// Schema reference — content-addressed pointer to a schema engram.
    pub schema: SchemaRef,
    /// BLAKE3 hash of the blob bytes (matches `id.0` for `Local` /
    /// `Embedded` sources; held separately so out-of-band sources can
    /// declare integrity independent of the manifest id).
    pub content_hash: Hash,
    pub byte_size: u64,
    pub created_at: Timestamp,
    #[serde(default)]
    pub last_accessed: Option<Timestamp>,
    /// Sources tried in order by [`resolve_blob`].
    pub sources: Vec<BlobSource>,
    /// Privacy axis — who is allowed to see this.
    pub privacy: PrivacyClass,
    /// Provenance axis — where this came from.
    pub provenance: ProvenanceRecord,
    /// Trust axis — how confident the local node is.
    pub trust: TrustEnvelope,
    /// Free-form metadata interpreted per-schema.
    #[serde(default)]
    pub schema_metadata: serde_json::Value,
    /// Forward-compatibility version. New fields added in later versions
    /// default-on-read; semantic changes bump this and run migrations.
    #[serde(default = "BlobManifest::default_version")]
    pub manifest_version: u32,
}

impl BlobManifest {
    /// Current manifest schema version. New manifests are stamped with this.
    pub const CURRENT_VERSION: u32 = 1;

    fn default_version() -> u32 {
        Self::CURRENT_VERSION
    }

    /// True if every source in this manifest is safe to transfer (no
    /// `LocalOnlyRef`).
    pub fn is_transferable(&self) -> bool {
        self.sources.iter().all(BlobSource::is_transferable)
    }

    /// Return a clone of the manifest with all `LocalOnlyRef` sources
    /// stripped. Errors if no transferable sources remain (you'd be
    /// shipping a manifest no recipient could resolve).
    pub fn to_transferable(&self) -> Result<Self> {
        let sources: Vec<_> = self
            .sources
            .iter()
            .filter(|source| source.is_transferable())
            .cloned()
            .collect();
        if sources.is_empty() {
            return Err(Error::new(
                "manifest has no transferable sources after stripping LocalOnlyRef",
            ));
        }
        Ok(Self {
            sources,
            ..self.clone()
        })
    }
}

/// Plug-in fetcher for non-local blob sources.
///
/// Eidetic itself only knows how to load `Local` blobs from the Store and
/// return `Embedded` bytes inline. Anything else (`Iroh`, `Https`, `LocalFile`)
/// is delegated to a `BlobFetcher` provided by the host wiring. Companion
/// crates like a hypothetical `eidetic-iroh-fetcher` or `eidetic-https-fetcher`
/// implement this trait.
///
/// Return semantics:
/// - `Ok(Some(bytes))` — fetched successfully. Caller hash-verifies.
/// - `Ok(None)` — this fetcher does not handle the given source kind;
///   [`resolve_blob`] tries the next source.
/// - `Err(_)` — fetch attempted but failed.
#[async_trait(?Send)]
pub trait BlobFetcher {
    async fn fetch(&mut self, source: &BlobSource) -> Result<Option<Vec<u8>>>;
}

/// A fetcher that handles no remote sources. Useful for tests and for hosts
/// that only need local resolution. Always returns `Ok(None)`.
pub struct NoFetcher;

#[async_trait(?Send)]
impl BlobFetcher for NoFetcher {
    async fn fetch(&mut self, _source: &BlobSource) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }
}

/// Enumerate every manifest in the Store, optionally filtering by
/// schema reference.
///
/// Walks the `"manifest:"` key prefix via [`Store::iter_keys`], loads
/// each manifest, and (if `schema_filter` is `Some`) keeps only those
/// whose `schema` matches. Errors if the backing Store doesn't support
/// enumeration.
pub async fn list_manifests(
    store: &mut dyn Store,
    schema_filter: Option<SchemaRef>,
) -> Result<Vec<BlobManifest>> {
    let keys = store.iter_keys("manifest:").await?;
    let mut out = Vec::with_capacity(keys.len());
    for key in keys {
        let bytes = match store.load_blob(&key).await? {
            Some(b) => b,
            None => continue, // race: key disappeared between enumeration and load
        };
        let manifest: BlobManifest = match serde_json::from_slice(&bytes) {
            Ok(m) => m,
            // Tolerate non-manifest entries that happen to share the
            // prefix — surface as a load problem only for the specific
            // key, not as a list-wide error.
            Err(_) => continue,
        };
        if let Some(filter) = schema_filter {
            if manifest.schema != filter {
                continue;
            }
        }
        out.push(manifest);
    }
    Ok(out)
}

/// Save a manifest to the Layer 1 [`Store`] under the well-known
/// `"manifest:<id>"` key.
pub async fn save_manifest(store: &mut dyn Store, manifest: &BlobManifest) -> Result<()> {
    let bytes =
        serde_json::to_vec(manifest).map_err(|e| Error::new(format!("manifest serialize: {e}")))?;
    store.save_blob(&manifest.id.store_key(), &bytes).await
}

/// Load a manifest by id. Returns `Ok(None)` if no manifest is stored under
/// that id.
///
/// Forward-compatibility: if `manifest_version` exceeds `CURRENT_VERSION`,
/// load still proceeds (best-effort) but the manifest is treated as a future
/// shape. Future migrations will dispatch on the version field; for v1 this
/// is a stub that just records the observed version.
pub async fn load_manifest(store: &mut dyn Store, id: ManifestId) -> Result<Option<BlobManifest>> {
    let key = id.store_key();
    match store.load_blob(&key).await? {
        None => Ok(None),
        Some(bytes) => {
            let manifest: BlobManifest = serde_json::from_slice(&bytes)
                .map_err(|e| Error::new(format!("manifest deserialize: {e}")))?;
            Ok(Some(manifest))
        }
    }
}

/// Resolve a manifest into the actual blob bytes, trying sources in order.
///
/// Hash-verifies every successful fetch against `manifest.content_hash`. On
/// hash mismatch the bytes are rejected and resolution moves to the next
/// source.
///
/// On a successful non-Local fetch, the bytes are cached locally under
/// `"blob:<hash>"` and a `BlobSource::Local` entry is implicitly satisfied
/// for next time (callers may want to update the manifest's source list to
/// record this — that's not done automatically here, since we don't own the
/// manifest's mutability story).
pub async fn resolve_blob(
    store: &mut dyn Store,
    fetcher: &mut dyn BlobFetcher,
    manifest: &BlobManifest,
) -> Result<Vec<u8>> {
    let mut last_error: Option<Error> = None;
    for source in &manifest.sources {
        let candidate = match source {
            BlobSource::Embedded { bytes, .. } => Some(bytes.clone()),
            BlobSource::Local { key } => store.load_blob(key).await?,
            _ => match fetcher.fetch(source).await {
                Ok(value) => value,
                Err(error) => {
                    last_error = Some(error);
                    continue;
                }
            },
        };

        let Some(bytes) = candidate else {
            continue;
        };

        let actual = Hash::of(&bytes);
        if actual != manifest.content_hash {
            last_error = Some(Error::new(format!(
                "content hash mismatch from source ({:?}): manifest declared {}, blob has {}",
                source_kind(source),
                manifest.content_hash,
                actual
            )));
            continue;
        }

        // Cache non-local fetched bytes for next time.
        if !matches!(
            source,
            BlobSource::Local { .. } | BlobSource::Embedded { .. }
        ) {
            let cache_key = blob_cache_key(&actual);
            store.save_blob(&cache_key, &bytes).await?;
        }

        return Ok(bytes);
    }

    Err(last_error.unwrap_or_else(|| {
        Error::new(format!(
            "no source could resolve blob for manifest {}",
            manifest.id
        ))
    }))
}

/// Store key under which a freshly-fetched blob is cached locally.
fn blob_cache_key(hash: &Hash) -> String {
    format!("blob:{}", hash.to_hex())
}

fn source_kind(source: &BlobSource) -> &'static str {
    match source {
        BlobSource::Embedded { .. } => "Embedded",
        BlobSource::Local { .. } => "Local",
        BlobSource::Iroh { .. } => "Iroh",
        BlobSource::Https { .. } => "Https",
        BlobSource::LocalFile { .. } => "LocalFile",
        BlobSource::LocalOnlyRef { .. } => "LocalOnlyRef",
    }
}

#[cfg(test)]
mod tests;
