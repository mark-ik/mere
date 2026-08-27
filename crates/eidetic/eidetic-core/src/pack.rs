// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The pack schema (participant gate B4): a portable denizen as an eidetic
//! engram under `mere.pack/v1`.
//!
//! The envelope was never the work — an eidetic [`Engram`](crate::Engram) /
//! typed payload already carries content-hash identity and the orthogonal
//! privacy / provenance / trust axes. B4 defines only what rides it:
//!
//! - [`PackManifest`], the typed payload: the part inventory (each part an
//!   artifact by content hash — large parts live as muniment blobs; the
//!   manifest carries hashes, never bytes) plus the contribution manifest
//!   (author, requested capability scopes — what the install review shows).
//!   The donor TransferProfile v1's multi-part inventory imports as this
//!   vocabulary; its model-adaptation half stays out.
//! - The **personae signing binding**: an Ed25519 signature over the
//!   manifest's canonical bytes, carried in the engram's
//!   [`TrustEnvelope::signatures`] as a [`SignatureRef`] string
//!   (`personae:ed25519:<pubkey-hex>:<sig-hex>` — the "concrete shape lands
//!   with identity" note on `SignatureRef`, landed).
//! - [`verify_pack`]: `Trusted` when a personae signature verifies,
//!   `Unsigned` when none is claimed, **`Broken` when one is claimed and does
//!   not verify** — a tampered pack is rejected, never quietly downgraded.
//!
//! Trust semantics per the plan's review corrections: `Trusted` means *the
//! signature verifies* — nothing more. Install still mints a local grant only
//! after the visible review, and a widened re-ask re-reviews.
//!
//! Canonicalization (v1): the manifest's `serde_json` encoding — struct-order
//! stable and sufficient for a self-contained payload signed and verified by
//! this module. The canonical-CBOR upgrade (wallet_grant's discipline) is a
//! compatible follow-on: the binding string is versioned by its prefix.

#[cfg(feature = "pack-signing")]
use identity::{Ed25519Keypair, Ed25519PublicKey, Ed25519Signature};
use serde::{Deserialize, Serialize};

use crate::schema::{ManifestId, SchemaRef};
#[cfg(feature = "pack-signing")]
use crate::schema::{SignatureRef, TrustEnvelope};
use crate::typed::TypedPayload;

/// The pack schema id (`SchemaRef` derives from it by content hash).
pub const PACK_SCHEMA: &str = "mere.pack/v1";

/// The signing-binding prefix a personae signature rides under in
/// [`TrustEnvelope::signatures`].
pub const PACK_SIG_PREFIX: &str = "personae:ed25519:";

/// What one part of a pack is for — the donor inventory's role vocabulary,
/// trimmed to what turnstone consumes. Unknown-forward so a newer pack loads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackPartRole {
    /// A runnable scenario body (the piccolo lane).
    ScenarioSource,
    /// A wasm component (the document-host / mod lane).
    WasmComponent,
    /// A static asset (sprite, sheet, doc).
    Asset,
    /// A role this build does not recognize.
    #[serde(other)]
    Unknown,
}

/// One artifact in the pack, by content hash. The bytes live as muniment
/// blobs (fetched over retinue at B5); the inventory is hashes + roles, so a
/// pack manifest is small and the signature covers every part's identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackPart {
    /// The part's name within the pack (`main.lua`, `helper.wasm`).
    pub name: String,
    pub role: PackPartRole,
    /// The content-addressed blob id.
    pub blob: ManifestId,
    /// The blob's size, for the review + fetch planning.
    pub bytes: u64,
}

/// The pack manifest: the typed payload of a `mere.pack/v1` engram.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackManifest {
    /// The pack's display name.
    pub name: String,
    /// The pack's version string (display + upgrade review).
    pub version: String,
    /// The author's personae public key, 32-byte hex — the identity a
    /// signature must verify against.
    pub author: String,
    /// The capability scopes the pack ASKS for — what the install review
    /// shows; a grant only exists after the user confirms.
    pub requested_scopes: Vec<String>,
    /// The part inventory.
    pub parts: Vec<PackPart>,
}

impl TypedPayload for PackManifest {
    fn schema_ref() -> SchemaRef {
        SchemaRef::from_id(ManifestId::of_blob(b"schema:mere.pack/v1"))
    }
}

/// The verification verdict — the trust ladder's bottom three rungs, decided
/// purely from the manifest + envelope (no network, no store).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackVerdict {
    /// A personae signature by the manifest's author verifies over the
    /// canonical bytes. Signature-only: the review still gates any grant.
    Trusted,
    /// No personae signature is claimed.
    Unsigned,
    /// A signature is claimed and does NOT verify (tampered manifest, wrong
    /// author, or a malformed binding). A broken pack is rejected.
    Broken,
}

/// The canonical bytes a pack signature covers (v1: the serde_json encoding;
/// see the module doc for the upgrade path).
pub fn canonical_bytes(manifest: &PackManifest) -> Vec<u8> {
    serde_json::to_vec(manifest).unwrap_or_default()
}

/// Sign `manifest` with the author's keypair, returning the [`SignatureRef`]
/// to carry in the engram's [`TrustEnvelope::signatures`]. The manifest's
/// `author` field must be this keypair's public key (verification checks it).
#[cfg(feature = "pack-signing")]
pub fn sign_pack(manifest: &PackManifest, keypair: &Ed25519Keypair) -> SignatureRef {
    let signature = keypair.sign(&canonical_bytes(manifest));
    SignatureRef(format!(
        "{PACK_SIG_PREFIX}{}:{}",
        hex(&keypair.public_key().to_bytes()),
        hex(&signature.to_bytes()),
    ))
}

/// Verify a pack against its trust envelope. `Trusted` iff some personae
/// binding parses, names the manifest's author, and verifies over the
/// canonical bytes; a claimed-but-failing binding is `Broken`; no claim is
/// `Unsigned`.
#[cfg(feature = "pack-signing")]
pub fn verify_pack(manifest: &PackManifest, envelope: &TrustEnvelope) -> PackVerdict {
    for sig in &envelope.signatures {
        let Some(rest) = sig.0.strip_prefix(PACK_SIG_PREFIX) else {
            continue; // some other scheme's signature; not ours to judge
        };
        let Some((pub_hex, sig_hex)) = rest.split_once(':') else {
            return PackVerdict::Broken;
        };
        if pub_hex != manifest.author {
            return PackVerdict::Broken;
        }
        let (Some(pub_bytes), Some(sig_bytes)) = (unhex::<32>(pub_hex), unhex::<64>(sig_hex))
        else {
            return PackVerdict::Broken;
        };
        let Ok(public) = Ed25519PublicKey::from_bytes(&pub_bytes) else {
            return PackVerdict::Broken;
        };
        let signature = Ed25519Signature::from_bytes(&sig_bytes);
        if public.verify(&canonical_bytes(manifest), &signature) {
            return PackVerdict::Trusted;
        }
        return PackVerdict::Broken;
    }
    PackVerdict::Unsigned
}

#[cfg(feature = "pack-signing")]
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(feature = "pack-signing")]
fn unhex<const N: usize>(hex: &str) -> Option<[u8; N]> {
    if hex.len() != N * 2 {
        return None;
    }
    let mut out = [0u8; N];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let s = std::str::from_utf8(chunk).ok()?;
        out[i] = u8::from_str_radix(s, 16).ok()?;
    }
    Some(out)
}

#[cfg(all(test, feature = "pack-signing"))]
mod tests {
    use super::*;
    use crate::schema::{
        ModerationState, PrivacyClass, ProvenanceOrigin, ProvenanceRecord, Timestamp, TrustLevel,
    };
    use crate::typed::{load_typed, save_typed};
    use muniment::MemoryBackend;

    fn keypair() -> Ed25519Keypair {
        Ed25519Keypair::from_seed([11u8; 32])
    }

    fn manifest(author: &Ed25519Keypair) -> PackManifest {
        PackManifest {
            name: "trail-keeper".to_string(),
            version: "0.1.0".to_string(),
            author: hex(&author.public_key().to_bytes()),
            requested_scopes: vec!["scenario/".to_string(), "app/".to_string()],
            parts: vec![PackPart {
                name: "main.lua".to_string(),
                role: PackPartRole::ScenarioSource,
                blob: ManifestId::of_blob(b"mere.open('mere://kept/note')"),
                bytes: 29,
            }],
        }
    }

    fn envelope_with(sig: SignatureRef) -> TrustEnvelope {
        TrustEnvelope {
            level: TrustLevel::SelfAsserted,
            signatures: vec![sig],
            moderation_state: ModerationState::Unreviewed,
        }
    }

    /// The B4 bar, first half: a signed pack round-trips through the typed
    /// store and still verifies Trusted.
    #[test]
    fn a_signed_pack_round_trips_and_verifies() {
        pollster::block_on(async {
            let kp = keypair();
            let pack = manifest(&kp);
            let sig = sign_pack(&pack, &kp);
            assert_eq!(
                verify_pack(&pack, &envelope_with(sig.clone())),
                PackVerdict::Trusted
            );

            let mut store = MemoryBackend::default();
            let at = Timestamp(1);
            let id = save_typed(
                &mut store,
                &pack,
                Vec::new(),
                PrivacyClass::LocalOnly,
                ProvenanceRecord {
                    origin: ProvenanceOrigin::Generated,
                    upstream: Vec::new(),
                    tooling: Some(PACK_SCHEMA.to_string()),
                    generated_at: at,
                },
                envelope_with(sig.clone()),
                at,
            )
            .await
            .unwrap();
            let mut fetcher = crate::manifest::NoFetcher;
            let restored: PackManifest = load_typed(&mut store, &mut fetcher, id)
                .await
                .unwrap()
                .expect("the pack manifest resolves locally");
            assert_eq!(restored, pack, "the manifest round-trips bit-equal");
            assert_eq!(
                verify_pack(&restored, &envelope_with(sig)),
                PackVerdict::Trusted,
                "the restored pack still verifies"
            );
        });
    }

    /// The B4 bar, second half: a tampered pack is rejected with Broken —
    /// any field change breaks the signature, including the part inventory.
    #[test]
    fn a_tampered_pack_is_broken() {
        let kp = keypair();
        let pack = manifest(&kp);
        let envelope = envelope_with(sign_pack(&pack, &kp));

        let mut renamed = pack.clone();
        renamed.version = "9.9.9".to_string();
        assert_eq!(verify_pack(&renamed, &envelope), PackVerdict::Broken);

        let mut swapped_part = pack.clone();
        swapped_part.parts[0].blob = ManifestId::of_blob(b"os.execute('rm -rf /')");
        assert_eq!(verify_pack(&swapped_part, &envelope), PackVerdict::Broken);

        let mut widened = pack.clone();
        widened.requested_scopes.push("wallet/".to_string());
        assert_eq!(
            verify_pack(&widened, &envelope),
            PackVerdict::Broken,
            "a widened ask cannot ride an old signature"
        );
    }

    /// A wrong-author claim and a malformed binding are Broken; no claim at
    /// all is merely Unsigned.
    #[test]
    fn wrong_author_is_broken_and_no_claim_is_unsigned() {
        let kp = keypair();
        let intruder = Ed25519Keypair::from_seed([99u8; 32]);
        let pack = manifest(&kp);

        // Signed by an intruder claiming to be the author.
        let forged = sign_pack(&pack, &intruder);
        // The binding carries the INTRUDER's pubkey, which is not the
        // manifest's author.
        assert_eq!(
            verify_pack(&pack, &envelope_with(forged)),
            PackVerdict::Broken
        );

        let malformed = SignatureRef(format!("{PACK_SIG_PREFIX}nonsense"));
        assert_eq!(
            verify_pack(&pack, &envelope_with(malformed)),
            PackVerdict::Broken
        );

        let unsigned = TrustEnvelope::self_asserted();
        assert_eq!(verify_pack(&pack, &unsigned), PackVerdict::Unsigned);

        // A foreign scheme's signature is not ours to judge: still Unsigned.
        let foreign = envelope_with(SignatureRef("did:web:example#sig".to_string()));
        assert_eq!(verify_pack(&pack, &foreign), PackVerdict::Unsigned);
    }
}
