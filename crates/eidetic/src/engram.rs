/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Engram envelope — schema-typed, immutable, time-bounded snapshot.
//!
//! An [`Engram`] wraps a schema-conformant payload with the integrity and
//! classification metadata required to ship it across the moothold/murm
//! federation tiers. Engrams are **immutable** by design: edits do not
//! exist; refreshing produces a new engram with a fresh content hash. See
//! the eidetic design pass §7.1 for the conceptual model.

use serde::{Deserialize, Serialize};

use crate::schema::{
    Hash, ManifestId, PrivacyClass, ProvenanceRecord, SchemaRef, Timestamp, TrustEnvelope,
};
use crate::{Error, Result};

/// Declared temporal range of source data captured in an engram. Inclusive
/// at both ends. Engrams are snapshots, not subscriptions — `to` is the
/// declared cutoff, not "now."
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeBounds {
    pub from: Timestamp,
    pub to: Timestamp,
}

impl TimeBounds {
    /// A snapshot covering a single instant — useful for engrams that are
    /// not derived from a temporal range (e.g. a model manifest).
    pub fn at(instant: Timestamp) -> Self {
        Self {
            from: instant,
            to: instant,
        }
    }

    /// `true` if `from <= to`.
    pub fn is_valid(&self) -> bool {
        self.from <= self.to
    }
}

/// Schema-typed, immutable, content-hashed snapshot. The portable payload
/// the moothold/murm federation tiers exchange.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Engram {
    /// Schema reference — content-addressed pointer to a schema engram
    /// describing the payload's shape. Recursive: a schema engram's
    /// `schema` field points to a meta-schema (Phase 4 work).
    pub schema: SchemaRef,
    /// Schematized data conforming to the declared schema.
    pub payload: Vec<u8>,
    /// BLAKE3 hash of `payload`. Verifies integrity on load.
    pub content_hash: Hash,
    /// Privacy axis — who is allowed to see this.
    pub privacy: PrivacyClass,
    /// Provenance axis — where this came from.
    pub provenance: ProvenanceRecord,
    /// Trust axis — how confident the local node is.
    pub trust: TrustEnvelope,
    /// Declared temporal range of source data.
    pub bounds: TimeBounds,
    /// Forward-compatibility version.
    #[serde(default = "Engram::default_version")]
    pub envelope_version: u32,
}

impl Engram {
    /// Current envelope schema version. New engrams are stamped with this.
    pub const CURRENT_VERSION: u32 = 1;

    fn default_version() -> u32 {
        Self::CURRENT_VERSION
    }

    /// Construct an engram, computing the content hash from the payload.
    pub fn new(
        schema: SchemaRef,
        payload: Vec<u8>,
        privacy: PrivacyClass,
        provenance: ProvenanceRecord,
        trust: TrustEnvelope,
        bounds: TimeBounds,
    ) -> Self {
        let content_hash = Hash::of(&payload);
        Self {
            schema,
            payload,
            content_hash,
            privacy,
            provenance,
            trust,
            bounds,
            envelope_version: Self::CURRENT_VERSION,
        }
    }

    /// Stable identifier for this engram — equals the BLAKE3 hash of the
    /// payload (same shape as [`ManifestId`]).
    pub fn id(&self) -> ManifestId {
        ManifestId::from_hash(self.content_hash)
    }

    /// Verify that `content_hash` matches `BLAKE3(payload)`. Use this on
    /// every load to reject tampered or corrupted bytes before the payload
    /// is interpreted.
    pub fn verify_integrity(&self) -> Result<()> {
        let actual = Hash::of(&self.payload);
        if actual != self.content_hash {
            return Err(Error::new(format!(
                "engram integrity check failed: declared {}, computed {}",
                self.content_hash, actual
            )));
        }
        if !self.bounds.is_valid() {
            return Err(Error::new(format!(
                "engram time bounds invalid: from={:?} > to={:?}",
                self.bounds.from, self.bounds.to
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{
        ManifestId, ModerationState, ProvenanceOrigin, ProvenanceRecord, SchemaRef, Timestamp,
        TrustLevel,
    };

    fn test_schema_ref() -> SchemaRef {
        SchemaRef::from_id(ManifestId::of_blob(b"test-schema"))
    }

    fn fresh_engram(payload: Vec<u8>) -> Engram {
        Engram::new(
            test_schema_ref(),
            payload,
            PrivacyClass::LocalOnly,
            ProvenanceRecord {
                origin: ProvenanceOrigin::Generated,
                upstream: Vec::new(),
                tooling: Some("eidetic-test/0.0.1".to_string()),
                generated_at: Timestamp(1_700_000_000_000),
            },
            TrustEnvelope {
                level: TrustLevel::SelfAsserted,
                signatures: Vec::new(),
                moderation_state: ModerationState::Unreviewed,
            },
            TimeBounds::at(Timestamp(1_700_000_000_000)),
        )
    }

    #[test]
    fn new_engram_passes_integrity_check() {
        let engram = fresh_engram(b"hello".to_vec());
        engram.verify_integrity().unwrap();
    }

    #[test]
    fn tampered_payload_fails_integrity_check() {
        let mut engram = fresh_engram(b"hello".to_vec());
        engram.payload = b"world".to_vec();
        let result = engram.verify_integrity();
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("integrity check failed"));
    }

    #[test]
    fn invalid_time_bounds_are_rejected() {
        let mut engram = fresh_engram(b"x".to_vec());
        engram.bounds = TimeBounds {
            from: Timestamp(2_000_000_000_000),
            to: Timestamp(1_000_000_000_000),
        };
        let result = engram.verify_integrity();
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("time bounds invalid"));
    }

    #[test]
    fn engram_id_equals_manifest_id_of_payload() {
        let payload = b"matching-id".to_vec();
        let engram = fresh_engram(payload.clone());
        assert_eq!(engram.id(), ManifestId::of_blob(&payload));
    }

    #[test]
    fn engram_round_trips_through_serde_and_keeps_integrity() {
        let engram = fresh_engram(b"round-trip".to_vec());
        let json = serde_json::to_string(&engram).unwrap();
        let round: Engram = serde_json::from_str(&json).unwrap();
        assert_eq!(round, engram);
        round.verify_integrity().unwrap();
    }

    #[test]
    fn engram_loads_with_default_version_when_field_missing() {
        // Older serialized engrams (pre-versioning) won't have envelope_version.
        let payload = b"x".to_vec();
        let hash = Hash::of(&payload);
        let json = format!(
            r#"{{
                "schema": "{schema}",
                "payload": [120],
                "content_hash": "{hash}",
                "privacy": "LocalOnly",
                "provenance": {{
                    "origin": "Generated",
                    "generated_at": 0
                }},
                "trust": {{
                    "level": "SelfAsserted",
                    "moderation_state": "Unreviewed"
                }},
                "bounds": {{
                    "from": 0,
                    "to": 0
                }}
            }}"#,
            schema = test_schema_ref().0.0.to_hex(),
            hash = hash.to_hex(),
        );
        let engram: Engram = serde_json::from_str(&json).unwrap();
        assert_eq!(engram.envelope_version, Engram::CURRENT_VERSION);
        engram.verify_integrity().unwrap();
    }
}
