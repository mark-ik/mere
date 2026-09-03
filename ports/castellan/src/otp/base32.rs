// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! RFC 4648 base32 decoding, which is how OTP secrets travel.
//!
//! Hand-rolled rather than pulled in: it is an alphabet mapping and a bit
//! accumulator, and RFC 4648 §10 publishes vectors, so it is checkable rather
//! than trusted. Decoding only. Castellan never mints secrets, so it never
//! needs to encode one.
//!
//! Two tolerances the Key Uri Format needs in practice: padding is optional
//! (most authenticator QR codes omit it) and case is ignored (many issuers
//! print secrets lowercase for readability).

use std::fmt;

/// Why a base32 string could not be decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Base32Error {
    /// A character outside the RFC 4648 alphabet.
    InvalidCharacter(char),
    /// The bit count does not correspond to any whole number of bytes.
    ///
    /// Base32 encodes 5 bits per character, so a group's leftover bits must be
    /// fewer than 8 and must all be zero. Lengths of 1, 3, or 6 characters
    /// past a group boundary cannot occur in a valid encoding.
    InvalidLength(usize),
    /// The final character carries bits that a decoder would have to discard,
    /// which means the encoder and decoder disagree about the input.
    NonCanonical,
}

impl fmt::Display for Base32Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Base32Error::InvalidCharacter(c) => {
                write!(f, "character {c:?} is not in the base32 alphabet")
            }
            Base32Error::InvalidLength(n) => {
                write!(f, "{n} base32 characters do not form whole bytes")
            }
            Base32Error::NonCanonical => {
                write!(
                    f,
                    "trailing bits are not zero; the encoding is not canonical"
                )
            }
        }
    }
}

impl std::error::Error for Base32Error {}

/// Decode an RFC 4648 base32 string. Padding optional, case insensitive.
pub fn decode(input: &str) -> Result<Vec<u8>, Base32Error> {
    let mut out = Vec::with_capacity(input.len() * 5 / 8);
    let mut buffer: u16 = 0;
    let mut bits: u32 = 0;
    let mut significant = 0usize;

    for c in input.chars() {
        // Padding only ever appears at the end and carries no data. Skipping
        // it also accepts the unpadded form the Key Uri Format uses.
        if c == '=' || c.is_whitespace() || c == '-' {
            continue;
        }
        let value = symbol_value(c).ok_or(Base32Error::InvalidCharacter(c))?;
        significant += 1;
        buffer = (buffer << 5) | u16::from(value);
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }

    // Whatever is left must be padding bits, and padding bits are zero.
    if bits >= 5 || (buffer & ((1 << bits) - 1)) != 0 {
        return if bits >= 5 {
            Err(Base32Error::InvalidLength(significant))
        } else {
            Err(Base32Error::NonCanonical)
        };
    }
    Ok(out)
}

fn symbol_value(c: char) -> Option<u8> {
    match c {
        'A'..='Z' => Some(c as u8 - b'A'),
        'a'..='z' => Some(c as u8 - b'a'),
        '2'..='7' => Some(c as u8 - b'2' + 26),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 4648 §10 publishes these, so the implementation is checked rather
    /// than believed.
    #[test]
    fn rfc4648_section_10_vectors() {
        let cases: &[(&str, &str)] = &[
            ("", ""),
            ("MY======", "f"),
            ("MZXQ====", "fo"),
            ("MZXW6===", "foo"),
            ("MZXW6YQ=", "foob"),
            ("MZXW6YTB", "fooba"),
            ("MZXW6YTBOI======", "foobar"),
        ];
        for (encoded, expected) in cases {
            assert_eq!(
                decode(encoded).unwrap(),
                expected.as_bytes(),
                "decoding {encoded:?}"
            );
        }
    }

    #[test]
    fn padding_is_optional_the_way_authenticator_uris_write_it() {
        assert_eq!(decode("MZXW6").unwrap(), b"foo");
        assert_eq!(decode("MZXW6===").unwrap(), b"foo");
    }

    #[test]
    fn case_and_grouping_whitespace_are_tolerated() {
        assert_eq!(decode("mzxw6ytboi").unwrap(), b"foobar");
        assert_eq!(decode("MZXW 6YTB OI").unwrap(), b"foobar");
        assert_eq!(decode("MZXW-6YTB-OI").unwrap(), b"foobar");
    }

    #[test]
    fn characters_outside_the_alphabet_are_rejected() {
        // 0, 1, and 8 are deliberately absent from the alphabet.
        assert_eq!(decode("MZXW0"), Err(Base32Error::InvalidCharacter('0')));
        assert_eq!(decode("MZX!"), Err(Base32Error::InvalidCharacter('!')));
    }

    #[test]
    fn a_lone_trailing_character_cannot_form_a_byte() {
        assert!(matches!(decode("M"), Err(Base32Error::InvalidLength(1))));
        assert!(matches!(decode("MZX"), Err(Base32Error::InvalidLength(3))));
    }

    #[test]
    fn non_zero_padding_bits_are_rejected() {
        // "MZXW6YTBOJ" differs from the valid "...OI" only in bits that a
        // decoder would have to throw away, so accepting it would mean two
        // strings decode to the same bytes.
        assert_eq!(decode("MZXW6YTBOJ"), Err(Base32Error::NonCanonical));
    }
}
