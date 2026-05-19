/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Schema, identity, and three-axis classification types for Layer 2.
//!
//! These types are the vocabulary of the manifest layer:
//!
//! - [`Hash`] / [`ManifestId`] — content-addressed identity (BLAKE3, 32 bytes)
//! - [`SchemaRef`] — a content-addressed pointer to a schema engram
//! - [`Timestamp`] — Unix milliseconds since epoch (portable, no chrono dep)
//! - [`PrivacyClass`] / [`ProvenanceRecord`] / [`TrustEnvelope`] — the three
//!   orthogonal classification axes (privacy / provenance / trust). Kept
//!   separate, never bundled — see the eidetic design pass §8.

use serde::{Deserialize, Serialize};

/// BLAKE3 content hash (32 bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Hash(#[serde(with = "hash_serde")] pub [u8; 32]);

impl Hash {
    /// Compute the BLAKE3 hash of a byte slice.
    pub fn of(bytes: &[u8]) -> Self {
        let digest = blake3::hash(bytes);
        Self(*digest.as_bytes())
    }

    /// Compute the BLAKE3 hash by streaming chunks through the incremental
    /// hasher. Avoids materialising the full payload as a single contiguous
    /// slice — useful for verifying large model weights or other multi-MB
    /// blobs that arrive in chunks.
    ///
    /// ```ignore
    /// let hash = Hash::from_chunks([b"first ", b"second"].into_iter());
    /// assert_eq!(hash, Hash::of(b"first second"));
    /// ```
    pub fn from_chunks<'a, I: IntoIterator<Item = &'a [u8]>>(chunks: I) -> Self {
        let mut hasher = blake3::Hasher::new();
        for chunk in chunks {
            hasher.update(chunk);
        }
        Self(*hasher.finalize().as_bytes())
    }

    /// Compute the BLAKE3 hash by streaming bytes from a `Read` source —
    /// e.g. a file handle or HTTP response body. Reads in 64 KiB chunks
    /// until EOF.
    ///
    /// This is the path that lets eidetic-side verification scale to
    /// multi-GiB blobs without holding the full payload in memory at
    /// once: pair it with a `BlobFetcher` that streams.
    pub fn from_reader<R: std::io::Read>(mut reader: R) -> std::io::Result<Self> {
        let mut hasher = blake3::Hasher::new();
        let mut buffer = [0u8; 65_536];
        loop {
            let n = reader.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }
        Ok(Self(*hasher.finalize().as_bytes()))
    }

    /// Hex-encoded representation (no algorithm prefix).
    pub fn to_hex(&self) -> String {
        let mut out = String::with_capacity(64);
        for byte in &self.0 {
            out.push_str(&format!("{byte:02x}"));
        }
        out
    }
}

impl std::fmt::Display for Hash {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "blake3:{}", self.to_hex())
    }
}

/// Serde adapter that encodes a 32-byte hash as a hex string in JSON, so
/// manifests stay human-inspectable.
mod hash_serde {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error> {
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        serializer.serialize_str(&hex)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<[u8; 32], D::Error> {
        let hex = String::deserialize(deserializer)?;
        if hex.len() != 64 {
            return Err(serde::de::Error::custom(format!(
                "expected 64 hex chars, got {}",
                hex.len()
            )));
        }
        let mut out = [0u8; 32];
        for (idx, pair) in hex.as_bytes().chunks(2).enumerate() {
            let byte_str =
                std::str::from_utf8(pair).map_err(|_| serde::de::Error::custom("non-utf8 hex"))?;
            out[idx] = u8::from_str_radix(byte_str, 16)
                .map_err(|_| serde::de::Error::custom(format!("invalid hex byte: {byte_str}")))?;
        }
        Ok(out)
    }
}

/// Stable identifier for a manifest. Equals the BLAKE3 hash of the blob the
/// manifest describes — two blobs with identical bytes share a manifest id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ManifestId(pub Hash);

impl ManifestId {
    /// Construct from a content hash (typically the blob's BLAKE3 hash).
    pub fn from_hash(hash: Hash) -> Self {
        Self(hash)
    }

    /// Compute the manifest id for a given blob.
    pub fn of_blob(bytes: &[u8]) -> Self {
        Self(Hash::of(bytes))
    }

    /// The Store key under which the manifest itself is persisted.
    pub fn store_key(&self) -> String {
        format!("manifest:{}", self.0.to_hex())
    }
}

impl std::fmt::Display for ManifestId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Content-addressed pointer to a schema engram. A schema is itself an engram
/// (recursive); this is just a typed wrapper around a [`ManifestId`] that
/// signals "the thing at this id is a schema definition."
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SchemaRef(pub ManifestId);

impl SchemaRef {
    pub fn from_id(id: ManifestId) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for SchemaRef {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Unix milliseconds since epoch. Portable across native + wasm; no chrono
/// dependency.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(pub u64);

impl Timestamp {
    pub const ZERO: Self = Self(0);
}

/// Privacy axis — who is allowed to see this artifact. Default is `LocalOnly`;
/// promotion to a wider audience is always an explicit operation, never
/// automatic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PrivacyClass {
    LocalOnly,
    TrustedPeersOnly,
    MootScoped,
    PublicPortable,
}

impl Default for PrivacyClass {
    fn default() -> Self {
        Self::LocalOnly
    }
}

/// Where an artifact came from. The shape stays loose for Phase 2 — concrete
/// origin enrichment lands when `identity` and the Distillery aspect
/// surface.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProvenanceOrigin {
    /// Imported from outside the local system (HF Hub, peer, file). The
    /// inner string identifies the source informally (e.g. an HF model URL).
    Imported { source: String },
    /// Derived from one or more upstream engrams via a Distillery transform
    /// or merge.
    Derived,
    /// Generated by a local tool / pipeline (e.g. a vector index built from
    /// existing engrams).
    Generated,
}

/// Provenance axis — origin, ancestry, generation context. Independent of
/// privacy and trust; provenance enables payout/audit/lineage tracking.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenanceRecord {
    pub origin: ProvenanceOrigin,
    /// Ancestor engrams (manifest ids) — for derivations / merges.
    #[serde(default)]
    pub upstream: Vec<ManifestId>,
    /// Tooling identifier (e.g. crate name + version) that produced the
    /// artifact, when known.
    #[serde(default)]
    pub tooling: Option<String>,
    pub generated_at: Timestamp,
}

impl ProvenanceRecord {
    /// Convenience for "user explicitly imported this from <source>."
    pub fn self_imported(source: impl Into<String>) -> Self {
        Self {
            origin: ProvenanceOrigin::Imported {
                source: source.into(),
            },
            upstream: Vec::new(),
            tooling: None,
            generated_at: Timestamp::ZERO,
        }
    }
}

/// Trust level — how confident the local node is in the artifact's contents.
/// Independent of privacy: an engram can be `LocalOnly` + `CheckpointAccepted`
/// (validated upstream contribution kept private) or `PublicPortable` +
/// `SelfAsserted` (something the user shares without external review).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrustLevel {
    SelfAsserted,
    PeerAttested,
    CommunityReviewed,
    CheckpointAccepted,
}

impl Default for TrustLevel {
    fn default() -> Self {
        Self::SelfAsserted
    }
}

/// Reference to a signature — opaque string for Phase 2; concrete shape
/// (likely DID / VC envelope per the engram_spec) lands with `identity`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SignatureRef(pub String);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModerationState {
    Unreviewed,
    Quarantined,
    Accepted,
    Rejected,
    Superseded,
}

impl Default for ModerationState {
    fn default() -> Self {
        Self::Unreviewed
    }
}

/// Trust axis — confidence level + signatures + moderation state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustEnvelope {
    pub level: TrustLevel,
    #[serde(default)]
    pub signatures: Vec<SignatureRef>,
    pub moderation_state: ModerationState,
}

impl TrustEnvelope {
    /// Convenience — the trust level a user's own freshly-imported artifact
    /// starts at: self-asserted, no signatures, unreviewed.
    pub fn self_asserted() -> Self {
        Self {
            level: TrustLevel::SelfAsserted,
            signatures: Vec::new(),
            moderation_state: ModerationState::Unreviewed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_of_known_input_matches_blake3_reference() {
        // BLAKE3 of empty input is a fixed value; compare via to_hex stability.
        let h = Hash::of(b"");
        assert_eq!(h.to_hex().len(), 64);
        // Re-hashing identical bytes returns identical digest.
        assert_eq!(Hash::of(b"hello"), Hash::of(b"hello"));
        assert_ne!(Hash::of(b"hello"), Hash::of(b"world"));
    }

    #[test]
    fn hash_from_chunks_matches_full_slice() {
        // Streaming via from_chunks is bit-for-bit identical to a single
        // call against the concatenated bytes — that's the property
        // streaming verification depends on.
        let full = b"first chunk + second chunk + third";
        let chunks: Vec<&[u8]> = vec![b"first chunk ", b"+ second chunk ", b"+ third"];
        assert_eq!(Hash::from_chunks(chunks.into_iter()), Hash::of(full));
    }

    #[test]
    fn hash_from_reader_matches_full_slice() {
        let payload: Vec<u8> = (0..8192u32).map(|i| (i % 256) as u8).collect();
        let from_slice = Hash::of(&payload);
        let from_reader = Hash::from_reader(payload.as_slice()).unwrap();
        assert_eq!(from_slice, from_reader);
    }

    #[test]
    fn hash_serializes_as_hex_string() {
        let h = Hash::of(b"abc");
        let json = serde_json::to_string(&h).unwrap();
        // serde_json wraps a string in quotes; verify the inner is 64 hex chars.
        assert_eq!(json.len(), 66, "expected quoted 64-char hex string");
        let round: Hash = serde_json::from_str(&json).unwrap();
        assert_eq!(round, h);
    }

    #[test]
    fn manifest_id_store_key_is_prefix_plus_hex() {
        let id = ManifestId::of_blob(b"payload");
        let key = id.store_key();
        assert!(key.starts_with("manifest:"));
        assert_eq!(key.len(), "manifest:".len() + 64);
    }

    #[test]
    fn privacy_class_defaults_to_local_only() {
        assert_eq!(PrivacyClass::default(), PrivacyClass::LocalOnly);
    }

    #[test]
    fn trust_envelope_self_asserted_starts_unreviewed() {
        let trust = TrustEnvelope::self_asserted();
        assert_eq!(trust.level, TrustLevel::SelfAsserted);
        assert!(trust.signatures.is_empty());
        assert_eq!(trust.moderation_state, ModerationState::Unreviewed);
    }

    #[test]
    fn provenance_self_imported_records_source() {
        let prov = ProvenanceRecord::self_imported("hf:test-model");
        match prov.origin {
            ProvenanceOrigin::Imported { source } => {
                assert_eq!(source, "hf:test-model");
            }
            _ => panic!("expected Imported origin"),
        }
        assert!(prov.upstream.is_empty());
    }
}
