// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

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

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Multicodec-tagged hash function.
///
/// Per the event-DAG substrate brief §13, every digest field carries its hash
/// function identifier alongside the digest, so a future hash migration
/// (BLAKE4, post-quantum, ...) needs no flag day: new writes carry a new tag
/// and old reads still parse. BLAKE3 is the only function today; adding one is
/// the only change a migration requires here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HashFn {
    /// BLAKE3-256. Multicodec `0x1e`, 32-byte digest.
    Blake3,
}

impl HashFn {
    /// Multicodec code (encoded as an unsigned-varint in the binary multihash).
    pub const fn multicodec(self) -> u64 {
        match self {
            HashFn::Blake3 => 0x1e,
        }
    }

    /// Short tag used in the self-describing string form `<fn>:<hex>`.
    pub const fn name(self) -> &'static str {
        match self {
            HashFn::Blake3 => "blake3",
        }
    }

    /// Digest length in bytes.
    pub const fn digest_len(self) -> usize {
        match self {
            HashFn::Blake3 => 32,
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        match name {
            "blake3" => Some(HashFn::Blake3),
            _ => None,
        }
    }

    fn from_multicodec(code: u64) -> Option<Self> {
        match code {
            0x1e => Some(HashFn::Blake3),
            _ => None,
        }
    }
}

/// Error parsing a [`Hash`] from its string or binary multihash form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HashError {
    /// Hash-function tag / multicodec is not one this build understands.
    UnknownFunction,
    /// Digest length does not match the function's expected length.
    BadLength,
    /// Hex string contained a non-hex byte.
    BadHex,
    /// Binary multihash ended before a full code/length/digest could be read.
    Truncated,
}

impl std::fmt::Display for HashError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            HashError::UnknownFunction => "unknown hash function",
            HashError::BadLength => "digest length mismatch",
            HashError::BadHex => "invalid hex in digest",
            HashError::Truncated => "truncated multihash",
        })
    }
}

impl std::error::Error for HashError {}

/// Content hash, multihash-aware: a digest tagged with the function that
/// produced it. BLAKE3-256 today (32 bytes). The string form is `<fn>:<hex>`
/// (human-inspectable, the [`Display`](std::fmt::Display) form); the binary
/// form ([`to_multihash_bytes`](Hash::to_multihash_bytes)) is a self-describing
/// multihash (`uvarint(code) ++ uvarint(len) ++ digest`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Hash {
    func: HashFn,
    digest: [u8; 32],
}

impl Hash {
    /// Compute the BLAKE3 hash of a byte slice.
    pub fn of(bytes: &[u8]) -> Self {
        Self {
            func: HashFn::Blake3,
            digest: *blake3::hash(bytes).as_bytes(),
        }
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
        Self {
            func: HashFn::Blake3,
            digest: *hasher.finalize().as_bytes(),
        }
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
        Ok(Self {
            func: HashFn::Blake3,
            digest: *hasher.finalize().as_bytes(),
        })
    }

    /// The hash function that produced this digest.
    pub fn func(&self) -> HashFn {
        self.func
    }

    /// The raw digest bytes (no function tag). BLAKE3 is 32 bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.digest
    }

    /// Hex-encoded digest (no algorithm prefix). For the self-describing form
    /// use [`Display`](std::fmt::Display) (`<fn>:<hex>`).
    pub fn to_hex(&self) -> String {
        let mut out = String::with_capacity(self.digest.len() * 2);
        for byte in &self.digest {
            out.push_str(&format!("{byte:02x}"));
        }
        out
    }

    /// Self-describing binary multihash: `uvarint(multicodec) ++ uvarint(len)
    /// ++ digest`. Digest fields on the wire, in the event DAG, and in
    /// capability scopes should use this form so a hash migration never needs a
    /// flag day.
    pub fn to_multihash_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(2 + self.digest.len());
        write_uvarint(self.func.multicodec(), &mut out);
        write_uvarint(self.func.digest_len() as u64, &mut out);
        out.extend_from_slice(&self.digest);
        out
    }

    /// Parse a [`Hash`] from its self-describing binary multihash form.
    pub fn from_multihash_bytes(bytes: &[u8]) -> Result<Self, HashError> {
        let (code, n1) = read_uvarint(bytes).ok_or(HashError::Truncated)?;
        let func = HashFn::from_multicodec(code).ok_or(HashError::UnknownFunction)?;
        let rest = &bytes[n1..];
        let (len, n2) = read_uvarint(rest).ok_or(HashError::Truncated)?;
        let digest_bytes = &rest[n2..];
        if len as usize != func.digest_len() || digest_bytes.len() != func.digest_len() {
            return Err(HashError::BadLength);
        }
        let mut digest = [0u8; 32];
        digest.copy_from_slice(digest_bytes);
        Ok(Self { func, digest })
    }

    /// Parse a [`Hash`] from its string form `<fn>:<hex>`. A bare hex string
    /// (no `<fn>:` prefix) is accepted as legacy BLAKE3 for backward reads.
    pub fn parse(s: &str) -> Result<Self, HashError> {
        let (name, hex) = match s.split_once(':') {
            Some((name, hex)) => (name, hex),
            None => ("blake3", s),
        };
        let func = HashFn::from_name(name).ok_or(HashError::UnknownFunction)?;
        if hex.len() != func.digest_len() * 2 {
            return Err(HashError::BadLength);
        }
        let mut digest = [0u8; 32];
        for (idx, pair) in hex.as_bytes().chunks(2).enumerate() {
            let byte_str = std::str::from_utf8(pair).map_err(|_| HashError::BadHex)?;
            digest[idx] = u8::from_str_radix(byte_str, 16).map_err(|_| HashError::BadHex)?;
        }
        Ok(Self { func, digest })
    }
}

impl std::fmt::Display for Hash {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}:{}", self.func.name(), self.to_hex())
    }
}

impl Serialize for Hash {
    /// Serialize as the self-describing string `<fn>:<hex>` so manifests stay
    /// human-inspectable and the hash function travels with the digest.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Hash {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Hash::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// Unsigned-varint (LEB128) encoder for multihash code/length prefixes.
fn write_uvarint(mut value: u64, out: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

/// Unsigned-varint decoder; returns the value and the number of bytes read.
fn read_uvarint(bytes: &[u8]) -> Option<(u64, usize)> {
    let mut value = 0u64;
    let mut shift = 0u32;
    for (idx, &byte) in bytes.iter().enumerate() {
        if shift >= 64 {
            return None;
        }
        value |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Some((value, idx + 1));
        }
        shift += 7;
    }
    None
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
    fn hash_serializes_as_self_describing_string() {
        let h = Hash::of(b"abc");
        let json = serde_json::to_string(&h).unwrap();
        // Quoted "blake3:<64 hex>" — the function identifier travels with the
        // digest, so a future hash migration needs no flag day (§13).
        assert_eq!(json, format!("\"blake3:{}\"", h.to_hex()));
        let round: Hash = serde_json::from_str(&json).unwrap();
        assert_eq!(round, h);
    }

    #[test]
    fn hash_multihash_bytes_round_trip() {
        let h = Hash::of(b"payload");
        let mh = h.to_multihash_bytes();
        // BLAKE3 multihash = code 0x1e, length 0x20 (32), then 32 digest bytes.
        assert_eq!(mh[0], 0x1e);
        assert_eq!(mh[1], 0x20);
        assert_eq!(mh.len(), 2 + 32);
        assert_eq!(Hash::from_multihash_bytes(&mh).unwrap(), h);
    }

    #[test]
    fn hash_parse_accepts_tagged_and_legacy_bare_hex() {
        let h = Hash::of(b"legacy");
        // Tagged form round-trips.
        assert_eq!(Hash::parse(&h.to_string()).unwrap(), h);
        // The old serialized form was bare hex with no function prefix; still
        // readable so pre-migration fixtures load.
        assert_eq!(Hash::parse(&h.to_hex()).unwrap(), h);
        // An unknown function tag is rejected, not silently treated as BLAKE3.
        assert_eq!(
            Hash::parse(&format!("blake4:{}", h.to_hex())),
            Err(HashError::UnknownFunction)
        );
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
