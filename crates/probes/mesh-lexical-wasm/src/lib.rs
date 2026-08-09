// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Cross-target determinism receipt for `esp.embed.lexical/v1`.
//!
//! The mesh declares `VerificationClass::ExactBytes` for the lexical embedding
//! resource — a claim that *any* conforming device reproduces the same output
//! bytes. That is only a receipt if a second target actually runs it, so this
//! probe compiles mesh's own `lexical_codec.rs` (by path, not by copy) plus the
//! real `esp` provider to wasm32 and reports the digest of the canonical
//! output for the same fixture the native test pins.

#[path = "../../../mesh/mesh/src/resources/lexical_codec.rs"]
mod lexical_codec;

use lexical_codec::{LexicalBatch, run_canonical};

/// The fixture the native `lexical_determinism_receipt` test uses, verbatim.
fn fixture() -> Vec<u8> {
    LexicalBatch::new(
        64,
        vec![
            "async rust programming".to_string(),
            "rust runtime internals".to_string(),
            "italian dinner recipes".to_string(),
        ],
    )
    .encode()
}

/// Canonical output length, so the loader can check the record shape too.
#[unsafe(no_mangle)]
pub extern "C" fn canonical_len() -> u32 {
    run_canonical(&fixture()).expect("fixture embeds").len() as u32
}

/// A pointer to 32 digest bytes in linear memory. Leaked on purpose: the probe
/// runs once and exits.
#[unsafe(no_mangle)]
pub extern "C" fn receipt_digest() -> *const u8 {
    let canonical = run_canonical(&fixture()).expect("fixture embeds");
    let digest = blake3::hash(&canonical).as_bytes().to_vec();
    Box::leak(digest.into_boxed_slice()).as_ptr()
}
