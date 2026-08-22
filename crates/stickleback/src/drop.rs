//! Transport-independent native drop framing.
//!
//! A drop is a bounded carrier for the same operation and content bytes used by
//! live replication. This module freezes the plaintext/test framing and keeps
//! import policy elsewhere: callers must stage visited records, verify them
//! through their domain processor, then commit accepted effects.

use std::io::{Read, Write};

use p2panda_core::cbor::{decode_cbor, encode_cbor};
use serde::{Deserialize, Serialize};

const MAGIC: [u8; 8] = *b"MEREDRP\0";
const VERSION: u16 = 1;
const PLAIN_SUITE: u16 = 0;
const COVER_LEN: u64 = 8 + 2 + 2 + 8 + 32;
const FRAME_HEADER_LEN: u64 = 2 + 2 + 8;

fn protection_aad(suite: u16) -> [u8; 12] {
    let mut aad = [0; 12];
    aad[0..8].copy_from_slice(&MAGIC);
    aad[8..10].copy_from_slice(&VERSION.to_le_bytes());
    aad[10..12].copy_from_slice(&suite.to_le_bytes());
    aad
}

/// Injected authenticated protection suite for private drops.
///
/// The group/key layer supplies the implementation and keys. The codec does
/// not choose them and offers no implicit plaintext fallback.
pub trait DropProtector {
    /// Registered non-zero suite identifier written into the cover.
    fn suite_id(&self) -> u16;

    fn protect(&self, plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>, NativeDropError>;

    /// Recover the semantic body. A suite that decompresses must enforce
    /// `max_plaintext_bytes` before growing its output beyond that bound.
    fn unprotect(
        &self,
        protected: &[u8],
        aad: &[u8],
        max_plaintext_bytes: u64,
    ) -> Result<Vec<u8>, NativeDropError>;
}

/// Semantic identity of a drop, independent of its carrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DropId(pub [u8; 32]);

/// Registered evidence type carried beside operations or content.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum EvidenceKind {
    CommitmentProof = 1,
    CapabilityChain = 2,
    CheckpointAuthorization = 3,
    KeyEnvelope = 4,
    /// Versioned auxiliary operation corpora carried by a domain aggregate.
    DomainOperations = 5,
}

/// One semantic record in a native drop.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DropRecord {
    Operation {
        header: Vec<u8>,
        inline_body: Option<Vec<u8>>,
    },
    PayloadChunk {
        payload_hash: [u8; 32],
        offset: u64,
        bytes: Vec<u8>,
    },
    BlobChunk {
        blob_hash: [u8; 32],
        offset: u64,
        bytes: Vec<u8>,
    },
    Evidence {
        kind: EvidenceKind,
        subject: [u8; 32],
        bytes: Vec<u8>,
        critical: bool,
    },
}

impl DropRecord {
    fn kind(&self) -> u16 {
        match self {
            Self::Operation { .. } => 1,
            Self::PayloadChunk { .. } => 2,
            Self::BlobChunk { .. } => 3,
            Self::Evidence { .. } => 4,
        }
    }

    fn critical(&self) -> bool {
        matches!(self, Self::Evidence { critical: true, .. })
    }
}

/// Canonical description of one framed record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub kind: u16,
    pub critical: bool,
    pub digest: [u8; 32],
    pub byte_len: u64,
}

/// Canonical semantic manifest. Its BLAKE3 digest is the [`DropId`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DropManifest {
    pub version: u16,
    pub records: Vec<ManifestEntry>,
}

impl DropManifest {
    pub fn id(&self) -> Result<DropId, NativeDropError> {
        let bytes = encode_cbor(self).map_err(codec)?;
        Ok(DropId(*blake3::hash(&bytes).as_bytes()))
    }
}

/// Resource limits enforced before record allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropLimits {
    pub max_protected_bytes: u64,
    pub max_manifest_bytes: u64,
    pub max_records: u64,
    pub max_record_bytes: u64,
}

impl Default for DropLimits {
    fn default() -> Self {
        Self {
            max_protected_bytes: 64 * 1024 * 1024,
            max_manifest_bytes: 4 * 1024 * 1024,
            max_records: 100_000,
            max_record_bytes: 16 * 1024 * 1024,
        }
    }
}

/// Metadata returned after writing a drop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropWriteReceipt {
    pub id: DropId,
    pub protected_bytes: u64,
    pub total_bytes: u64,
    pub record_count: u64,
}

/// Integrity report returned after staged record visitation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropReadReport {
    pub id: DropId,
    pub record_count: u64,
    pub visited_records: u64,
    pub skipped_optional_records: u64,
}

/// A framing, integrity, or resource-limit failure.
#[derive(Debug, thiserror::Error)]
pub enum NativeDropError {
    #[error("native drop I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("native drop has the wrong magic")]
    WrongMagic,
    #[error("unsupported native drop version {0}")]
    UnsupportedVersion(u16),
    #[error("unsupported native drop protection suite {0}")]
    UnsupportedProtection(u16),
    #[error("native drop protector suite must be non-zero")]
    PlaintextProtector,
    #[error("native drop protector expected suite {expected}, found {actual}")]
    ProtectionSuiteMismatch { expected: u16, actual: u16 },
    #[error("native drop protection: {0}")]
    Protection(String),
    #[error("native drop is truncated")]
    Truncated,
    #[error("native drop exceeds {limit}: {actual} > {maximum}")]
    Limit {
        limit: &'static str,
        actual: u64,
        maximum: u64,
    },
    #[error("native drop codec: {0}")]
    Codec(String),
    #[error("native drop protected-body digest is false")]
    FalseBodyDigest,
    #[error("native drop record {index} does not match its manifest")]
    FalseRecord { index: u64 },
    #[error("unknown critical native drop record kind {kind}")]
    UnknownCriticalRecord { kind: u16 },
    #[error("native drop contains trailing or missing protected bytes")]
    BodyLengthMismatch,
}

fn codec(error: impl std::fmt::Display) -> NativeDropError {
    NativeDropError::Codec(error.to_string())
}

fn limit(name: &'static str, actual: u64, maximum: u64) -> Result<(), NativeDropError> {
    if actual > maximum {
        Err(NativeDropError::Limit {
            limit: name,
            actual,
            maximum,
        })
    } else {
        Ok(())
    }
}

fn manifest_for(records: &[DropRecord]) -> Result<DropManifest, NativeDropError> {
    Ok(DropManifest {
        version: VERSION,
        records: records
            .iter()
            .map(|record| {
                let bytes = encode_cbor(record).map_err(codec)?;
                Ok(ManifestEntry {
                    kind: record.kind(),
                    critical: record.critical(),
                    digest: *blake3::hash(&bytes).as_bytes(),
                    byte_len: bytes.len() as u64,
                })
            })
            .collect::<Result<_, NativeDropError>>()?,
    })
}

fn frame_header(entry: &ManifestEntry) -> [u8; FRAME_HEADER_LEN as usize] {
    let mut out = [0; FRAME_HEADER_LEN as usize];
    out[0..2].copy_from_slice(&entry.kind.to_le_bytes());
    out[2..4].copy_from_slice(&u16::from(entry.critical).to_le_bytes());
    out[4..12].copy_from_slice(&entry.byte_len.to_le_bytes());
    out
}

/// Write a plaintext public/test drop without buffering the full body.
///
/// Each record is encoded once to establish its canonical manifest, then the
/// manifest and record frames are written directly to `writer`.
pub fn write_plain_drop<W: Write>(
    mut writer: W,
    records: &[DropRecord],
    limits: DropLimits,
) -> Result<DropWriteReceipt, NativeDropError> {
    limit("record count", records.len() as u64, limits.max_records)?;
    let manifest = manifest_for(records)?;
    for entry in &manifest.records {
        limit("record bytes", entry.byte_len, limits.max_record_bytes)?;
    }
    let manifest_bytes = encode_cbor(&manifest).map_err(codec)?;
    limit(
        "manifest bytes",
        manifest_bytes.len() as u64,
        limits.max_manifest_bytes,
    )?;

    let protected_bytes = 8u64
        .checked_add(manifest_bytes.len() as u64)
        .and_then(|length| {
            manifest.records.iter().try_fold(length, |total, entry| {
                total.checked_add(FRAME_HEADER_LEN + entry.byte_len)
            })
        })
        .ok_or_else(|| codec("protected-body length overflow"))?;
    limit(
        "protected bytes",
        protected_bytes,
        limits.max_protected_bytes,
    )?;

    let mut body_hasher = blake3::Hasher::new();
    body_hasher.update(&(manifest_bytes.len() as u64).to_le_bytes());
    body_hasher.update(&manifest_bytes);
    for (entry, record) in manifest.records.iter().zip(records) {
        let bytes = encode_cbor(record).map_err(codec)?;
        body_hasher.update(&frame_header(entry));
        body_hasher.update(&bytes);
    }
    let body_digest = *body_hasher.finalize().as_bytes();

    writer.write_all(&MAGIC)?;
    writer.write_all(&VERSION.to_le_bytes())?;
    writer.write_all(&PLAIN_SUITE.to_le_bytes())?;
    writer.write_all(&protected_bytes.to_le_bytes())?;
    writer.write_all(&body_digest)?;
    writer.write_all(&(manifest_bytes.len() as u64).to_le_bytes())?;
    writer.write_all(&manifest_bytes)?;
    for (entry, record) in manifest.records.iter().zip(records) {
        let bytes = encode_cbor(record).map_err(codec)?;
        writer.write_all(&frame_header(entry))?;
        writer.write_all(&bytes)?;
    }

    Ok(DropWriteReceipt {
        id: manifest.id()?,
        protected_bytes,
        total_bytes: COVER_LEN + protected_bytes,
        record_count: records.len() as u64,
    })
}

/// Write a private drop through an explicitly injected protection suite.
pub fn write_protected_drop<W: Write, P: DropProtector>(
    mut writer: W,
    records: &[DropRecord],
    limits: DropLimits,
    protector: &P,
) -> Result<DropWriteReceipt, NativeDropError> {
    let suite = protector.suite_id();
    if suite == PLAIN_SUITE {
        return Err(NativeDropError::PlaintextProtector);
    }
    let mut plain = Vec::new();
    let plain_receipt = write_plain_drop(&mut plain, records, limits)?;
    let semantic_body = &plain[COVER_LEN as usize..];
    let protected = protector.protect(semantic_body, &protection_aad(suite))?;
    limit(
        "protected bytes",
        protected.len() as u64,
        limits.max_protected_bytes,
    )?;
    writer.write_all(&MAGIC)?;
    writer.write_all(&VERSION.to_le_bytes())?;
    writer.write_all(&suite.to_le_bytes())?;
    writer.write_all(&(protected.len() as u64).to_le_bytes())?;
    writer.write_all(blake3::hash(&protected).as_bytes())?;
    writer.write_all(&protected)?;
    Ok(DropWriteReceipt {
        id: plain_receipt.id,
        protected_bytes: protected.len() as u64,
        total_bytes: COVER_LEN + protected.len() as u64,
        record_count: plain_receipt.record_count,
    })
}

fn read_exact<R: Read>(reader: &mut R, bytes: &mut [u8]) -> Result<(), NativeDropError> {
    reader
        .read_exact(bytes)
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::UnexpectedEof => NativeDropError::Truncated,
            _ => NativeDropError::Io(error),
        })
}

fn read_hashed<R: Read>(
    reader: &mut R,
    bytes: &mut [u8],
    hasher: &mut blake3::Hasher,
    consumed: &mut u64,
) -> Result<(), NativeDropError> {
    read_exact(reader, bytes)?;
    hasher.update(bytes);
    *consumed = consumed
        .checked_add(bytes.len() as u64)
        .ok_or_else(|| codec("protected-body length overflow"))?;
    Ok(())
}

fn u16_le(bytes: &[u8]) -> u16 {
    u16::from_le_bytes(bytes.try_into().expect("two-byte field"))
}

fn u64_le(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes.try_into().expect("eight-byte field"))
}

/// Visit verified record frames one at a time.
///
/// The callback is a staging hook. It runs before the final body digest check,
/// so it must not mutate a live domain store. Known records are delivered;
/// unknown optional kinds are counted and skipped.
pub fn visit_plain_drop<R: Read, F: FnMut(DropRecord) -> Result<(), NativeDropError>>(
    mut reader: R,
    limits: DropLimits,
    mut visit: F,
) -> Result<DropReadReport, NativeDropError> {
    let mut cover = [0; COVER_LEN as usize];
    read_exact(&mut reader, &mut cover)?;
    if cover[0..8] != MAGIC {
        return Err(NativeDropError::WrongMagic);
    }
    let version = u16_le(&cover[8..10]);
    if version != VERSION {
        return Err(NativeDropError::UnsupportedVersion(version));
    }
    let suite = u16_le(&cover[10..12]);
    if suite != PLAIN_SUITE {
        return Err(NativeDropError::UnsupportedProtection(suite));
    }
    let protected_bytes = u64_le(&cover[12..20]);
    limit(
        "protected bytes",
        protected_bytes,
        limits.max_protected_bytes,
    )?;
    let expected_body_digest: [u8; 32] = cover[20..52].try_into().unwrap();

    let mut body_hasher = blake3::Hasher::new();
    let mut consumed = 0;
    let mut length = [0; 8];
    read_hashed(&mut reader, &mut length, &mut body_hasher, &mut consumed)?;
    let manifest_len = u64_le(&length);
    limit("manifest bytes", manifest_len, limits.max_manifest_bytes)?;
    if 8 + manifest_len > protected_bytes {
        return Err(NativeDropError::BodyLengthMismatch);
    }
    let mut manifest_bytes = vec![0; usize::try_from(manifest_len).map_err(codec)?];
    read_hashed(
        &mut reader,
        &mut manifest_bytes,
        &mut body_hasher,
        &mut consumed,
    )?;
    let manifest: DropManifest = decode_cbor(&manifest_bytes[..]).map_err(codec)?;
    if encode_cbor(&manifest).map_err(codec)? != manifest_bytes {
        return Err(NativeDropError::Codec(
            "manifest is not in canonical struct encoding".into(),
        ));
    }
    if manifest.version != VERSION {
        return Err(NativeDropError::UnsupportedVersion(manifest.version));
    }
    limit(
        "record count",
        manifest.records.len() as u64,
        limits.max_records,
    )?;
    let id = manifest.id()?;
    let mut visited_records = 0;
    let mut skipped_optional_records = 0;

    for (index, entry) in manifest.records.iter().enumerate() {
        limit("record bytes", entry.byte_len, limits.max_record_bytes)?;
        if consumed
            .checked_add(FRAME_HEADER_LEN + entry.byte_len)
            .is_none_or(|next| next > protected_bytes)
        {
            return Err(NativeDropError::BodyLengthMismatch);
        }
        let mut header = [0; FRAME_HEADER_LEN as usize];
        read_hashed(&mut reader, &mut header, &mut body_hasher, &mut consumed)?;
        if u16_le(&header[0..2]) != entry.kind
            || (u16_le(&header[2..4]) != 0) != entry.critical
            || u64_le(&header[4..12]) != entry.byte_len
        {
            return Err(NativeDropError::FalseRecord {
                index: index as u64,
            });
        }
        let mut bytes = vec![0; usize::try_from(entry.byte_len).map_err(codec)?];
        read_hashed(&mut reader, &mut bytes, &mut body_hasher, &mut consumed)?;
        if *blake3::hash(&bytes).as_bytes() != entry.digest {
            return Err(NativeDropError::FalseRecord {
                index: index as u64,
            });
        }
        if !(1..=4).contains(&entry.kind) {
            if entry.critical {
                return Err(NativeDropError::UnknownCriticalRecord { kind: entry.kind });
            }
            skipped_optional_records += 1;
            continue;
        }
        let record: DropRecord = decode_cbor(&bytes[..]).map_err(codec)?;
        if record.kind() != entry.kind
            || record.critical() != entry.critical
            || encode_cbor(&record).map_err(codec)? != bytes
        {
            return Err(NativeDropError::FalseRecord {
                index: index as u64,
            });
        }
        visit(record)?;
        visited_records += 1;
    }
    if consumed != protected_bytes {
        return Err(NativeDropError::BodyLengthMismatch);
    }
    if *body_hasher.finalize().as_bytes() != expected_body_digest {
        return Err(NativeDropError::FalseBodyDigest);
    }
    Ok(DropReadReport {
        id,
        record_count: manifest.records.len() as u64,
        visited_records,
        skipped_optional_records,
    })
}

/// Decode a small drop into memory. Importers should prefer
/// [`visit_plain_drop`] and a bounded staging store.
pub fn read_plain_drop<R: Read>(
    reader: R,
    limits: DropLimits,
) -> Result<(DropReadReport, Vec<DropRecord>), NativeDropError> {
    let mut records = Vec::new();
    let report = visit_plain_drop(reader, limits, |record| {
        records.push(record);
        Ok(())
    })?;
    Ok((report, records))
}

/// Authenticate, recover, and visit a protected drop through its configured
/// suite. The callback runs only after both protection and the outer digest
/// verify.
pub fn visit_protected_drop<
    R: Read,
    P: DropProtector,
    F: FnMut(DropRecord) -> Result<(), NativeDropError>,
>(
    mut reader: R,
    limits: DropLimits,
    protector: &P,
    visit: F,
) -> Result<DropReadReport, NativeDropError> {
    let mut cover = [0; COVER_LEN as usize];
    read_exact(&mut reader, &mut cover)?;
    if cover[0..8] != MAGIC {
        return Err(NativeDropError::WrongMagic);
    }
    let version = u16_le(&cover[8..10]);
    if version != VERSION {
        return Err(NativeDropError::UnsupportedVersion(version));
    }
    let suite = u16_le(&cover[10..12]);
    if suite != protector.suite_id() {
        return Err(NativeDropError::ProtectionSuiteMismatch {
            expected: protector.suite_id(),
            actual: suite,
        });
    }
    if suite == PLAIN_SUITE {
        return Err(NativeDropError::PlaintextProtector);
    }
    let protected_len = u64_le(&cover[12..20]);
    limit("protected bytes", protected_len, limits.max_protected_bytes)?;
    let mut protected = vec![0; usize::try_from(protected_len).map_err(codec)?];
    read_exact(&mut reader, &mut protected)?;
    if blake3::hash(&protected).as_bytes() != &cover[20..52] {
        return Err(NativeDropError::FalseBodyDigest);
    }
    let semantic_body = protector.unprotect(
        &protected,
        &protection_aad(suite),
        limits.max_protected_bytes,
    )?;
    limit(
        "protected bytes",
        semantic_body.len() as u64,
        limits.max_protected_bytes,
    )?;

    let mut plain = Vec::with_capacity(COVER_LEN as usize + semantic_body.len());
    plain.extend_from_slice(&MAGIC);
    plain.extend_from_slice(&VERSION.to_le_bytes());
    plain.extend_from_slice(&PLAIN_SUITE.to_le_bytes());
    plain.extend_from_slice(&(semantic_body.len() as u64).to_le_bytes());
    plain.extend_from_slice(blake3::hash(&semantic_body).as_bytes());
    plain.extend_from_slice(&semantic_body);
    visit_plain_drop(std::io::Cursor::new(plain), limits, visit)
}

/// Decode a protected drop into memory after authenticated recovery.
pub fn read_protected_drop<R: Read, P: DropProtector>(
    reader: R,
    limits: DropLimits,
    protector: &P,
) -> Result<(DropReadReport, Vec<DropRecord>), NativeDropError> {
    let mut records = Vec::new();
    let report = visit_protected_drop(reader, limits, protector, |record| {
        records.push(record);
        Ok(())
    })?;
    Ok((report, records))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    struct TestProtector;

    impl DropProtector for TestProtector {
        fn suite_id(&self) -> u16 {
            77
        }

        fn protect(&self, plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>, NativeDropError> {
            let mut out = blake3::hash(aad).as_bytes().to_vec();
            out.extend(plaintext.iter().map(|byte| byte ^ 0xa5));
            Ok(out)
        }

        fn unprotect(
            &self,
            protected: &[u8],
            aad: &[u8],
            max_plaintext_bytes: u64,
        ) -> Result<Vec<u8>, NativeDropError> {
            if protected.get(..32) != Some(blake3::hash(aad).as_bytes()) {
                return Err(NativeDropError::Protection(
                    "test authentication failed".into(),
                ));
            }
            limit(
                "protected bytes",
                (protected.len() - 32) as u64,
                max_plaintext_bytes,
            )?;
            Ok(protected[32..].iter().map(|byte| byte ^ 0xa5).collect())
        }
    }

    fn records() -> Vec<DropRecord> {
        vec![
            DropRecord::Operation {
                header: b"header".to_vec(),
                inline_body: Some(b"body".to_vec()),
            },
            DropRecord::BlobChunk {
                blob_hash: [7; 32],
                offset: 4,
                bytes: b"chunk".to_vec(),
            },
        ]
    }

    fn encoded() -> (Vec<u8>, DropWriteReceipt) {
        let mut bytes = Vec::new();
        let receipt = write_plain_drop(&mut bytes, &records(), DropLimits::default()).unwrap();
        (bytes, receipt)
    }

    fn raw_future_record(critical: bool) -> Vec<u8> {
        let payload = b"future record".to_vec();
        let entry = ManifestEntry {
            kind: 99,
            critical,
            digest: *blake3::hash(&payload).as_bytes(),
            byte_len: payload.len() as u64,
        };
        let manifest = DropManifest {
            version: VERSION,
            records: vec![entry.clone()],
        };
        let manifest_bytes = encode_cbor(&manifest).unwrap();
        let protected_bytes = 8 + manifest_bytes.len() as u64 + FRAME_HEADER_LEN + entry.byte_len;
        let mut hasher = blake3::Hasher::new();
        hasher.update(&(manifest_bytes.len() as u64).to_le_bytes());
        hasher.update(&manifest_bytes);
        hasher.update(&frame_header(&entry));
        hasher.update(&payload);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&MAGIC);
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&PLAIN_SUITE.to_le_bytes());
        bytes.extend_from_slice(&protected_bytes.to_le_bytes());
        bytes.extend_from_slice(hasher.finalize().as_bytes());
        bytes.extend_from_slice(&(manifest_bytes.len() as u64).to_le_bytes());
        bytes.extend_from_slice(&manifest_bytes);
        bytes.extend_from_slice(&frame_header(&entry));
        bytes.extend_from_slice(&payload);
        bytes
    }

    #[test]
    fn plaintext_drop_round_trips_and_visits_in_order() {
        let (bytes, written) = encoded();
        let mut visited = Vec::new();
        let read = visit_plain_drop(Cursor::new(bytes), DropLimits::default(), |record| {
            visited.push(record);
            Ok(())
        })
        .unwrap();
        assert_eq!(visited, records());
        assert_eq!(read.id, written.id);
        assert_eq!(read.visited_records, 2);
    }

    #[test]
    fn corruption_and_truncation_fail_closed() {
        let (mut corrupt, _) = encoded();
        *corrupt.last_mut().unwrap() ^= 1;
        assert!(matches!(
            read_plain_drop(Cursor::new(corrupt), DropLimits::default()),
            Err(NativeDropError::FalseRecord { .. })
                | Err(NativeDropError::FalseBodyDigest)
                | Err(NativeDropError::Codec(_))
        ));

        let (mut truncated, _) = encoded();
        truncated.pop();
        assert!(matches!(
            read_plain_drop(Cursor::new(truncated), DropLimits::default()),
            Err(NativeDropError::Truncated)
        ));
    }

    #[test]
    fn limits_are_checked_before_record_allocation() {
        let (bytes, _) = encoded();
        let limits = DropLimits {
            max_record_bytes: 1,
            ..DropLimits::default()
        };
        assert!(matches!(
            read_plain_drop(Cursor::new(bytes), limits),
            Err(NativeDropError::Limit {
                limit: "record bytes",
                ..
            })
        ));
    }

    #[test]
    fn unknown_optional_records_skip_and_unknown_critical_records_fail() {
        let optional = raw_future_record(false);
        let report = visit_plain_drop(Cursor::new(optional), DropLimits::default(), |_| {
            panic!("unknown records are not decoded")
        })
        .unwrap();
        assert_eq!(report.visited_records, 0);
        assert_eq!(report.skipped_optional_records, 1);

        let critical = raw_future_record(true);
        assert!(matches!(
            read_plain_drop(Cursor::new(critical), DropLimits::default()),
            Err(NativeDropError::UnknownCriticalRecord { kind: 99 })
        ));
    }

    #[test]
    fn unsupported_protection_and_false_body_length_fail_closed() {
        let (mut unsupported, _) = encoded();
        unsupported[10..12].copy_from_slice(&7u16.to_le_bytes());
        assert!(matches!(
            read_plain_drop(Cursor::new(unsupported), DropLimits::default()),
            Err(NativeDropError::UnsupportedProtection(7))
        ));

        let (mut false_length, _) = encoded();
        let stated = u64_le(&false_length[12..20]);
        false_length[12..20].copy_from_slice(&(stated - 1).to_le_bytes());
        assert!(matches!(
            read_plain_drop(Cursor::new(false_length), DropLimits::default()),
            Err(NativeDropError::BodyLengthMismatch)
        ));
    }

    #[test]
    fn operation_headers_can_arrive_before_their_payload_chunks() {
        let records = vec![
            DropRecord::Operation {
                header: b"signed header".to_vec(),
                inline_body: None,
            },
            DropRecord::PayloadChunk {
                payload_hash: [9; 32],
                offset: 0,
                bytes: b"later body".to_vec(),
            },
        ];
        let mut bytes = Vec::new();
        write_plain_drop(&mut bytes, &records, DropLimits::default()).unwrap();
        let (_, decoded) = read_plain_drop(Cursor::new(bytes), DropLimits::default()).unwrap();
        assert_eq!(decoded, records);
    }

    #[test]
    fn protected_drop_requires_and_round_trips_through_its_suite() {
        let records = records();
        let mut bytes = Vec::new();
        let written =
            write_protected_drop(&mut bytes, &records, DropLimits::default(), &TestProtector)
                .unwrap();
        assert_eq!(u16_le(&bytes[10..12]), 77);
        let (read, decoded) =
            read_protected_drop(Cursor::new(bytes), DropLimits::default(), &TestProtector).unwrap();
        assert_eq!(read.id, written.id);
        assert_eq!(decoded, records);
    }

    #[test]
    // Re-frozen for p2panda 0.7.1: `encode_cbor` moved from ciborium to
    // cbor-core, which emits canonical CBOR and therefore orders map keys
    // (shortest first, then bytewise) instead of writing struct fields in
    // declaration order. `ManifestEntry`'s bytes changed, so the manifest
    // digest — and with it the `DropId` — changed. The drop format itself is
    // unchanged; only its canonical byte encoding moved.
    fn golden_vector_freezes_manifest_identity_and_cover() {
        let (bytes, receipt) = encoded();
        assert_eq!(&bytes[0..8], b"MEREDRP\0");
        assert_eq!(receipt.total_bytes as usize, bytes.len());
        assert_eq!(
            hex::encode(receipt.id.0),
            "f92cf6cf4dc067f05f14454b9b1cbb618935a02002da9f15b2d37fd6f5452a55"
        );
        assert_eq!(
            hex::encode(&bytes[0..52]),
            "4d45524544525000010000007d010000000000005d67dd629e91bbcd8e04fd16ba72de402c27ca75bc74fb6f257f5b1f90f47d1b"
        );
    }
}
