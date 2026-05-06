//! LEB128-style varint encoding/decoding.
//!
//! Cable wire encoding uses unsigned LEB128 (Little-Endian Base 128) varints
//! for length prefixes, post types, timestamps, and other variable-width
//! integer fields. This module provides the foundational encode/decode
//! primitives.
//!
//! ## Encoding
//!
//! Each byte stores 7 bits of the integer (least-significant first); the
//! high bit (`0x80`) signals "more bytes follow." The last byte has the high
//! bit clear.
//!
//! - `0` → `[0x00]`
//! - `127` → `[0x7F]`
//! - `128` → `[0x80, 0x01]`
//! - `300` → `[0xAC, 0x02]`
//! - `u64::MAX` → 10-byte sequence
//!
//! ## Reference
//!
//! Same encoding as [Protocol Buffers
//! varints](https://protobuf.dev/programming-guides/encoding/#varints) and
//! [Cable's wire protocol](https://github.com/cabal-club/cable).

use std::io::{Read, Write};

use crate::MurmuringError;

/// Maximum number of bytes a `u64` LEB128 varint can occupy.
///
/// `(64 + 6) / 7 = 10` — ten 7-bit groups plus terminator bits.
pub const MAX_VARINT_LEN_U64: usize = 10;

/// Write a `u64` as a LEB128 varint to the writer.
///
/// Returns the number of bytes written.
pub fn write_varint<W: Write>(writer: &mut W, mut value: u64) -> std::io::Result<usize> {
    let mut bytes_written = 0;
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        if value == 0 {
            writer.write_all(&[byte])?;
            bytes_written += 1;
            return Ok(bytes_written);
        }
        writer.write_all(&[byte | 0x80])?;
        bytes_written += 1;
    }
}

/// Read a LEB128 varint from the reader, returning the decoded `u64`.
///
/// Returns [`MurmuringError::MalformedPost`] if:
/// - The reader hits EOF mid-varint
/// - The varint exceeds 10 bytes (overflow protection: a `u64` LEB128 fits
///   in at most 10 bytes)
pub fn read_varint<R: Read>(reader: &mut R) -> Result<u64, MurmuringError> {
    let mut value: u64 = 0;
    let mut shift: u32 = 0;
    let mut buf = [0u8; 1];

    for _ in 0..MAX_VARINT_LEN_U64 {
        reader
            .read_exact(&mut buf)
            .map_err(|_| MurmuringError::MalformedPost)?;
        let byte = buf[0];

        // The low 7 bits contribute to the value. We cap at shift==63 below.
        let chunk = (byte & 0x7F) as u64;

        // Detect overflow: at shift 63, only the lowest bit can fit; anything
        // above it overflows.
        if shift == 63 && chunk > 1 {
            return Err(MurmuringError::MalformedPost);
        }

        value |= chunk << shift;

        if byte & 0x80 == 0 {
            return Ok(value);
        }

        shift += 7;
    }

    // 10 bytes consumed without terminator — malformed.
    Err(MurmuringError::MalformedPost)
}

/// Encode a `u64` as a LEB128 varint into a freshly-allocated `Vec<u8>`.
///
/// Convenience wrapper for non-streaming callers.
pub fn encode_varint(value: u64) -> Vec<u8> {
    let mut buf = Vec::with_capacity(MAX_VARINT_LEN_U64);
    write_varint(&mut buf, value).expect("Vec write never fails");
    buf
}

/// Decode a LEB128 varint from a byte slice.
///
/// Returns the decoded value and the number of bytes consumed, or
/// `MalformedPost` if the slice doesn't contain a complete varint or
/// the varint overflows `u64`.
pub fn decode_varint(bytes: &[u8]) -> Result<(u64, usize), MurmuringError> {
    let mut cursor = std::io::Cursor::new(bytes);
    let value = read_varint(&mut cursor)?;
    Ok((value, cursor.position() as usize))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each (decoded value, expected encoding) pair.
    const VECTORS: &[(u64, &[u8])] = &[
        (0, &[0x00]),
        (1, &[0x01]),
        (127, &[0x7F]),
        (128, &[0x80, 0x01]),
        (255, &[0xFF, 0x01]),
        (300, &[0xAC, 0x02]),
        (16_383, &[0xFF, 0x7F]),
        (16_384, &[0x80, 0x80, 0x01]),
        (
            u64::MAX,
            &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01],
        ),
    ];

    #[test]
    fn known_vectors_encode_correctly() {
        for &(value, expected_bytes) in VECTORS {
            let encoded = encode_varint(value);
            assert_eq!(
                encoded.as_slice(),
                expected_bytes,
                "encoding {value} did not match expected bytes"
            );
        }
    }

    #[test]
    fn known_vectors_decode_correctly() {
        for &(expected_value, bytes) in VECTORS {
            let (decoded, consumed) = decode_varint(bytes).unwrap();
            assert_eq!(decoded, expected_value, "decoded value mismatch");
            assert_eq!(consumed, bytes.len(), "consumed-byte count mismatch");
        }
    }

    #[test]
    fn round_trip_via_vec() {
        for value in [
            0u64,
            1,
            127,
            128,
            300,
            16_383,
            16_384,
            1_000_000,
            u32::MAX as u64,
            u64::MAX,
        ] {
            let encoded = encode_varint(value);
            let (decoded, consumed) = decode_varint(&encoded).unwrap();
            assert_eq!(decoded, value);
            assert_eq!(consumed, encoded.len());
        }
    }

    #[test]
    fn round_trip_via_streams() {
        for value in [0u64, 1, 100, 12345, 1_234_567_890, u64::MAX] {
            let mut buf = Vec::new();
            let written = write_varint(&mut buf, value).unwrap();
            assert_eq!(written, buf.len());

            let mut cursor = std::io::Cursor::new(&buf);
            let decoded = read_varint(&mut cursor).unwrap();
            assert_eq!(decoded, value);
        }
    }

    #[test]
    fn truncated_varint_is_malformed() {
        // 0x80 alone says "more bytes follow" but there are no more bytes.
        let bytes = &[0x80];
        let result = decode_varint(bytes);
        assert!(matches!(result, Err(MurmuringError::MalformedPost)));
    }

    #[test]
    fn empty_input_is_malformed() {
        let result = decode_varint(&[]);
        assert!(matches!(result, Err(MurmuringError::MalformedPost)));
    }

    #[test]
    fn overflowing_varint_is_malformed() {
        // 11 bytes (one too many for u64 LEB128) — even if the bytes are
        // formally well-structured, we reject them.
        let bytes = &[0xFF; 11];
        let result = decode_varint(bytes);
        assert!(matches!(result, Err(MurmuringError::MalformedPost)));
    }

    #[test]
    fn overflow_in_top_chunk_is_malformed() {
        // 10-byte varint where the final chunk has bits beyond the u64
        // boundary set. Specifically: nine 0xFF bytes followed by a
        // terminator with the bit-1 position set (0x02 instead of 0x01)
        // would overflow u64 by one bit.
        let bytes = &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x02];
        let result = decode_varint(bytes);
        assert!(matches!(result, Err(MurmuringError::MalformedPost)));
    }

    #[test]
    fn small_values_use_one_byte() {
        for value in 0u64..128 {
            let encoded = encode_varint(value);
            assert_eq!(encoded.len(), 1, "value {value} should use 1 byte");
        }
    }

    #[test]
    fn multi_varint_stream_round_trips() {
        // Encode several varints into one buffer; decode them back in
        // sequence. Verifies that decode correctly stops at each terminator.
        let values: &[u64] = &[0, 1, 127, 128, 300, 12345];
        let mut buf = Vec::new();
        for &v in values {
            write_varint(&mut buf, v).unwrap();
        }

        let mut cursor = std::io::Cursor::new(&buf);
        let mut decoded = Vec::new();
        while (cursor.position() as usize) < buf.len() {
            decoded.push(read_varint(&mut cursor).unwrap());
        }
        assert_eq!(decoded, values);
    }
}
