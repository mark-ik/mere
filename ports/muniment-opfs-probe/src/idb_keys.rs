// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Key-range arithmetic for the IndexedDB range backend, and the contract it
//! is only correct under.
//!
//! **The seam finding this exists to record.** `muniment::Backend::scan` is
//! specified in Rust's string order, which is code-point order (UTF-8 byte
//! order). IndexedDB compares strings by **UTF-16 code unit**. Those two
//! orders are *not* the same: a supplementary character encodes as a
//! surrogate pair beginning in `0xD800..=0xDBFF`, which sorts **below**
//! `U+E000..=U+FFFF` in UTF-16, while its code point sorts above them. So
//!
//! ```text
//! Rust:      "\u{FFFF}"  <  "\u{10000}"
//! IndexedDB: "\u{FFFF}"  >  "\u{10000}"
//! ```
//!
//! A range query handed to IndexedDB therefore cannot reproduce muniment's
//! specified ordering for keys outside the BMP, no matter how the bounds are
//! computed. That is a property of the seam, not of this implementation, and
//! a production range adapter would have to either restrict keys, encode them
//! order-preservingly, or drop to a cursor and re-sort.
//!
//! This backend takes the narrow, honest option: it declares an **ASCII key
//! contract** and refuses anything else, so it can never silently return the
//! wrong set. The probe's workloads are all ASCII, so the benchmark is
//! unaffected — and a refusal is visible where a mis-selection would not be.
//!
//! (The first version had no contract and used `prefix + U+10FFFF` as the
//! exclusive upper bound for `list`. Under UTF-16 that bound sorts *below*
//! `prefix + U+FFFF`, so such a key was silently omitted.)

/// Why a key or bound was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NonAsciiKey {
    pub what: &'static str,
    pub key: String,
}

impl std::fmt::Display for NonAsciiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: {:?} is not ASCII. This range backend declares an ASCII key contract because \
             IndexedDB orders by UTF-16 code unit while muniment::Backend::scan is specified in \
             Rust's code-point order; the two disagree outside the BMP, so a range query cannot \
             honour the contract for such keys",
            self.what, self.key
        )
    }
}

/// Check a key or bound against the ASCII contract.
pub fn require_ascii(what: &'static str, key: &str) -> Result<(), NonAsciiKey> {
    if key.is_ascii() {
        Ok(())
    } else {
        Err(NonAsciiKey {
            what,
            key: key.to_string(),
        })
    }
}

/// The exclusive upper bound of every key beginning with `prefix`, in ASCII
/// order: the prefix with its last byte incremented, carrying past `0x7F`.
///
/// `None` means "no upper bound" — either the prefix is empty (every key
/// matches) or it is all `\x7F` (nothing sorts above it within ASCII), and
/// the caller should use a lower bound alone.
pub fn ascii_prefix_upper_bound(prefix: &str) -> Option<String> {
    let mut bytes = prefix.as_bytes().to_vec();
    while let Some(last) = bytes.pop() {
        if last < 0x7F {
            bytes.push(last + 1);
            // Every byte is ASCII, so this cannot fail.
            return Some(String::from_utf8(bytes).expect("ASCII in, ASCII out"));
        }
        // 0x7F carries: drop it and try to bump the byte before.
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_bound_is_the_immediate_successor() {
        assert_eq!(ascii_prefix_upper_bound("log/").as_deref(), Some("log0"));
        assert_eq!(ascii_prefix_upper_bound("a").as_deref(), Some("b"));
        assert_eq!(ascii_prefix_upper_bound("blob/").as_deref(), Some("blob0"));
    }

    /// Everything carrying the prefix must fall inside `[prefix, bound)`, and
    /// the first key that does not carry it must fall outside. This is the
    /// property the old `prefix + U+10FFFF` bound broke.
    #[test]
    fn bound_admits_every_ascii_continuation_and_excludes_the_next_prefix() {
        for prefix in ["log/", "a", "blob/", "op", "~"] {
            let bound = ascii_prefix_upper_bound(prefix).expect("has a successor");
            for suffix in ["", "\x00", "0", "z", "\x7F", "\x7F\x7F", "zzzzzzzz"] {
                let key = format!("{prefix}{suffix}");
                assert!(
                    key.as_str() >= prefix && key.as_str() < bound.as_str(),
                    "{key:?} must lie in [{prefix:?}, {bound:?})"
                );
            }
            // The bound itself does not carry the prefix, so excluding it is
            // correct rather than a lost key.
            assert!(!bound.starts_with(prefix));
        }
    }

    #[test]
    fn carrying_past_del_drops_to_the_previous_byte() {
        assert_eq!(ascii_prefix_upper_bound("a\x7F").as_deref(), Some("b"));
        assert_eq!(ascii_prefix_upper_bound("a\x7F\x7F").as_deref(), Some("b"));
        // No ASCII string sorts above these, so there is no upper bound.
        assert_eq!(ascii_prefix_upper_bound(""), None);
        assert_eq!(ascii_prefix_upper_bound("\x7F"), None);
        assert_eq!(ascii_prefix_upper_bound("\x7F\x7F"), None);
    }

    #[test]
    fn the_contract_refuses_what_it_cannot_order() {
        assert!(require_ascii("scan start", "log/0001").is_ok());
        assert!(require_ascii("scan start", "log/é").is_err());
        assert!(require_ascii("list prefix", "\u{FFFF}").is_err());
        assert!(require_ascii("list prefix", "\u{10000}").is_err());
    }

    /// The divergence itself, asserted so it stays a recorded fact rather
    /// than a remembered one. Rust orders by code point; UTF-16 code-unit
    /// order (what IndexedDB uses) inverts this pair.
    #[test]
    fn rust_and_utf16_orders_disagree_outside_the_bmp() {
        let bmp = "\u{FFFF}";
        let supplementary = "\u{10000}";
        assert!(bmp < supplementary, "Rust orders by code point");

        let utf16 = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
        assert!(
            utf16(bmp) > utf16(supplementary),
            "UTF-16 code-unit order, which IndexedDB uses, puts U+FFFF above a surrogate pair"
        );
        // Which is exactly why the ASCII contract exists.
        assert!(require_ascii("key", bmp).is_err());
        assert!(require_ascii("key", supplementary).is_err());
    }
}
