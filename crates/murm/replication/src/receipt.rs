//! Portable completion receipts for native drops.
//!
//! A receipt says that one local import transaction for a semantic [`DropId`]
//! completed. It carries no peer identity or authority. A peer service must bind
//! the framed bytes to its authenticated session before using the statement for
//! resend coordination.

use std::io::{Read, Write};

use p2panda_core::cbor::{decode_cbor, encode_cbor};
use serde::{Deserialize, Serialize};

use crate::DropId;

const MAGIC: [u8; 8] = *b"MERERCP\0";
const VERSION: u16 = 1;
const COVER_LEN: usize = 8 + 2 + 8 + 32;

/// Caller-configured resource limit for a receipt control frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropReceiptLimits {
    pub max_body_bytes: u64,
}

impl Default for DropReceiptLimits {
    fn default() -> Self {
        Self {
            max_body_bytes: 1024,
        }
    }
}

/// Authority-neutral statement that a complete drop import committed locally.
///
/// The record counts describe the semantic manifest. They are useful for peer
/// coordination, but a remote receipt must never be installed as this device's
/// local import marker or used to bypass domain admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DropReceipt {
    pub drop_id: DropId,
    pub operation_records: u64,
    pub non_operation_records: u64,
}

impl DropReceipt {
    pub const fn new(drop_id: DropId, operation_records: u64, non_operation_records: u64) -> Self {
        Self {
            drop_id,
            operation_records,
            non_operation_records,
        }
    }

    /// Canonical semantic bytes used both in local storage and the wire frame.
    pub fn to_canonical_bytes(self) -> Result<Vec<u8>, DropReceiptError> {
        encode_cbor(&self).map_err(codec)
    }

    /// Decode canonical semantic bytes. Non-canonical encodings fail closed.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, DropReceiptError> {
        let receipt: Self = decode_cbor(bytes).map_err(codec)?;
        if receipt.to_canonical_bytes()? != bytes {
            return Err(DropReceiptError::NonCanonical);
        }
        Ok(receipt)
    }
}

/// Stable, transport-independent storage scope for one authenticated peer.
///
/// The peer service derives this from the identity authenticated by its current
/// carrier. The original identity bytes are not stored in muniment keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReceiptPeer(pub [u8; 32]);

impl ReceiptPeer {
    pub fn from_authenticated_identity(identity: &[u8]) -> Self {
        Self(blake3::derive_key(
            "mere murm drop receipt peer scope v1",
            identity,
        ))
    }
}

/// Framing, integrity, or resource-limit failure for a receipt control frame.
#[derive(Debug, thiserror::Error)]
pub enum DropReceiptError {
    #[error("drop receipt I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("drop receipt has the wrong magic")]
    WrongMagic,
    #[error("unsupported drop receipt version {0}")]
    UnsupportedVersion(u16),
    #[error("drop receipt is truncated")]
    Truncated,
    #[error("drop receipt body exceeds configured limit: {actual} > {maximum}")]
    Limit { actual: u64, maximum: u64 },
    #[error("drop receipt body digest is false")]
    FalseBodyDigest,
    #[error("drop receipt codec: {0}")]
    Codec(String),
    #[error("drop receipt body is not canonically encoded")]
    NonCanonical,
}

fn codec(error: impl std::fmt::Display) -> DropReceiptError {
    DropReceiptError::Codec(error.to_string())
}

fn read_exact<R: Read>(reader: &mut R, bytes: &mut [u8]) -> Result<(), DropReceiptError> {
    reader.read_exact(bytes).map_err(|error| {
        if error.kind() == std::io::ErrorKind::UnexpectedEof {
            DropReceiptError::Truncated
        } else {
            DropReceiptError::Io(error)
        }
    })
}

/// Write one integrity-framed receipt for exchange over any authenticated
/// carrier. The frame itself does not authenticate the peer that sent it.
pub fn write_drop_receipt<W: Write>(
    mut writer: W,
    receipt: DropReceipt,
    limits: DropReceiptLimits,
) -> Result<u64, DropReceiptError> {
    let body = receipt.to_canonical_bytes()?;
    let body_len = body.len() as u64;
    if body_len > limits.max_body_bytes {
        return Err(DropReceiptError::Limit {
            actual: body_len,
            maximum: limits.max_body_bytes,
        });
    }
    writer.write_all(&MAGIC)?;
    writer.write_all(&VERSION.to_le_bytes())?;
    writer.write_all(&body_len.to_le_bytes())?;
    writer.write_all(blake3::hash(&body).as_bytes())?;
    writer.write_all(&body)?;
    Ok(COVER_LEN as u64 + body_len)
}

/// Read one integrity-framed receipt. The caller supplies authenticated peer
/// context and decides how long remote coordination state is retained.
pub fn read_drop_receipt<R: Read>(
    mut reader: R,
    limits: DropReceiptLimits,
) -> Result<DropReceipt, DropReceiptError> {
    let mut cover = [0; COVER_LEN];
    read_exact(&mut reader, &mut cover)?;
    if cover[0..8] != MAGIC {
        return Err(DropReceiptError::WrongMagic);
    }
    let version = u16::from_le_bytes(cover[8..10].try_into().expect("fixed receipt cover"));
    if version != VERSION {
        return Err(DropReceiptError::UnsupportedVersion(version));
    }
    let body_len = u64::from_le_bytes(cover[10..18].try_into().expect("fixed receipt cover"));
    if body_len > limits.max_body_bytes {
        return Err(DropReceiptError::Limit {
            actual: body_len,
            maximum: limits.max_body_bytes,
        });
    }
    let mut body = vec![0; usize::try_from(body_len).map_err(codec)?];
    read_exact(&mut reader, &mut body)?;
    if blake3::hash(&body).as_bytes() != &cover[18..50] {
        return Err(DropReceiptError::FalseBodyDigest);
    }
    DropReceipt::from_canonical_bytes(&body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt() -> DropReceipt {
        DropReceipt::new(DropId([7; 32]), 3, 2)
    }

    #[test]
    fn receipt_frame_round_trips() {
        let mut bytes = Vec::new();
        let written =
            write_drop_receipt(&mut bytes, receipt(), DropReceiptLimits::default()).unwrap();
        assert_eq!(written as usize, bytes.len());
        assert_eq!(&bytes[0..8], b"MERERCP\0");
        assert_eq!(
            read_drop_receipt(&bytes[..], DropReceiptLimits::default()).unwrap(),
            receipt()
        );
    }

    #[test]
    fn receipt_frame_rejects_corruption_and_limits() {
        let mut bytes = Vec::new();
        write_drop_receipt(&mut bytes, receipt(), DropReceiptLimits::default()).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        assert!(matches!(
            read_drop_receipt(&bytes[..], DropReceiptLimits::default()),
            Err(DropReceiptError::FalseBodyDigest)
        ));
        assert!(matches!(
            write_drop_receipt(
                Vec::new(),
                receipt(),
                DropReceiptLimits { max_body_bytes: 1 }
            ),
            Err(DropReceiptError::Limit { .. })
        ));
    }
}
