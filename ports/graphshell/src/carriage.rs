// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The epoch carriage lane: leased slots for wrapped private-epoch material.
//!
//! Implements the lane grammar (design_docs, 2026-08-18). Carriage rides a
//! sibling topic beside a personal graph's own, so key delivery never enters
//! the codicil grammar; every fact a replica must check rides in the header
//! extension, so a peer can accept, refuse, and prune without decoding a body
//! it is not expected to be able to open; and supersession is the protocol's
//! own prune (`Admission::prune_before_current` plus payload erasure), so the
//! lease is the authorization for a deletion the store already knows how to
//! perform, not a second retention mechanism.
//!
//! The replica set is ruled elsewhere (wallet roster grants carriage, pairing
//! list routes it); this module is the grammar both sides speak.

use std::collections::HashMap;

use p2panda_core::prune::PruneFlag;
use p2panda_core::{Hash, Operation, Topic};
use pandect::BlindedSlotId;
use personae::{Ed25519Keypair, Ed25519PublicKey, Ed25519Signature};
use serde::{Deserialize, Serialize};
use stickleback::{Admission, OperationPolicy, Reject, StoreTarget};

/// Domain separating the carriage topic from the graph topic it shadows.
const CARRIAGE_TOPIC_CONTEXT: &str = "mere.graphshell.carriage.topic.v1";

/// Domain separating a lease signature from every other use of the issuer key.
const LEASE_SIGNING_CONTEXT: &[u8] = b"mere.graphshell.carriage.lease.v1";

/// The per-author log a slot's versions live on: the slot itself.
///
/// Not a single shared log, and the reason is the prune law:
/// `PruneBeforeCurrent` deletes every operation before the admitted one in
/// its log, so two slots sharing a log would let one slot's supersession
/// destroy the other's live version. One log per slot makes a prune unable
/// to reach past its own slot by construction.
pub fn carriage_log(slot: BlindedSlotId) -> [u8; 32] {
    slot.0
}

/// Backstop ceiling on any lease, against misconfiguration rather than as a
/// target: thirty days, sized generously above any sensible per-device TTL.
/// The real exposure bound is the device's own `CarriagePolicy` TTL; this
/// exists so a fat-fingered TTL cannot mint a year-long harvest window.
pub const CARRIAGE_ABSOLUTE_CEILING_MS: u64 = 30 * 24 * 60 * 60 * 1000;

/// The carriage topic shadowing one personal graph's topic.
///
/// Derived under its own BLAKE3 context, so it collides with neither the graph
/// topic nor p2panda's gossip-overlay derivation of it. Any graph peer can
/// compute it, which is the point: subscribing to it announces only that a
/// device on this graph also carries key material, and graph peers already
/// know the device is on the graph. No persona is named at the transport.
pub fn carriage_topic(graph: [u8; 32]) -> [u8; 32] {
    blake3::derive_key(CARRIAGE_TOPIC_CONTEXT, &graph)
}

/// Everything an admitting peer checks, in the header where it can be read
/// without the body.
///
/// The body is a `pandect` `WrappedEpochRecord`, unchanged and already blinded;
/// this extension never names a persona, a certificate, or a device. `slot` is
/// the blinded handle (`pandect::blinded_slot_id`), computable only by the
/// holder and the issuer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CarriageExt {
    /// Which graph's carriage topic this belongs to.
    pub graph: [u8; 32],
    /// Blinded slot identity. Never the certificate id.
    pub slot: BlindedSlotId,
    /// Monotonic per slot; the ordering destructive supersession runs on.
    pub issue: u64,
    /// Mandatory expiry. An un-leased record is unrepresentable here.
    pub expires_at_ms: u64,
    /// Required set: carriage is a slot, never history, as a protocol fact.
    pub prune_flag: PruneFlag,
    /// The issuing authority's signature over the lease facts and the payload
    /// hash. Distinct from the transport writer's header signature, because
    /// the writer may be a relay while the authority is always the persona
    /// that issued the certificate. Sixty-four bytes, checked at admission
    /// (a `[u8; 64]` has no derived serde impls, so the vec is the encoding).
    pub issuer_signature: Vec<u8>,
}

/// The canonical bytes a lease signature covers.
///
/// Domain-separated and fixed-width throughout, so no field can bleed into
/// its neighbor. Binding the payload hash means a signature authorizes one
/// exact record: replacing the body under a kept lease fails verification.
fn lease_message(
    graph: [u8; 32],
    slot: BlindedSlotId,
    issue: u64,
    expires_at_ms: u64,
    payload: Hash,
) -> Vec<u8> {
    let mut message = Vec::with_capacity(LEASE_SIGNING_CONTEXT.len() + 32 + 32 + 8 + 8 + 32);
    message.extend_from_slice(LEASE_SIGNING_CONTEXT);
    message.extend_from_slice(&graph);
    message.extend_from_slice(&slot.0);
    message.extend_from_slice(&issue.to_le_bytes());
    message.extend_from_slice(&expires_at_ms.to_le_bytes());
    message.extend_from_slice(payload.as_bytes());
    message
}

/// Sign a lease as its issuing authority.
///
/// The keypair is the persona's chain-root keypair, the same authority whose
/// public key the verifying side holds as a `TrustedRoot` (one per persona,
/// the M5 consequence).
pub fn sign_lease(
    issuer: &Ed25519Keypair,
    graph: [u8; 32],
    slot: BlindedSlotId,
    issue: u64,
    expires_at_ms: u64,
    payload: Hash,
) -> Vec<u8> {
    issuer
        .sign(&lease_message(graph, slot, issue, expires_at_ms, payload))
        .to_bytes()
        .to_vec()
}

/// Verify a lease against a set of trusted issuer roots.
///
/// Tries each root rather than reading an issuer key off the wire, on
/// purpose: publishing the issuer's key in the extension would be a stable
/// per-persona value on an announced topic, letting an observer group records
/// by persona across devices, which is the association graph the blinding
/// exists to prevent. The root set is small (one per persona of the owner),
/// so the trial loop costs nothing that matters.
pub fn verify_lease(ext: &CarriageExt, payload: Hash, trusted_roots: &[[u8; 32]]) -> bool {
    let Ok(signature_bytes) = <[u8; 64]>::try_from(ext.issuer_signature.as_slice()) else {
        return false;
    };
    let message = lease_message(ext.graph, ext.slot, ext.issue, ext.expires_at_ms, payload);
    let signature = Ed25519Signature::from_bytes(&signature_bytes);
    trusted_roots.iter().any(|root| {
        Ed25519PublicKey::from_bytes(root)
            .map(|key| key.verify(&message, &signature))
            .unwrap_or(false)
    })
}

/// What a holder knows about one slot it already stores.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeldLease {
    /// The stored version's issue counter.
    pub issue: u64,
    /// The stored version's payload hash, erased when superseded.
    pub payload: Hash,
    /// The stored version's expiry, which the purge pass acts on.
    pub expires_at_ms: u64,
}

/// Ceilings the admitting host can assert beyond the absolute backstop.
///
/// Only the issuing wallet knows a device's `CarriagePolicy` TTL, and only a
/// certificate holder knows the grant's expiry; a bare replica knows neither,
/// and refusing everything it cannot check would make replication pointless.
/// So the knowable ceilings are optional and the absolute one is not: every
/// host enforces the backstop, and each host additionally enforces what its
/// position lets it know. Issue-side code MUST pass both.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CarriageCeilings {
    /// The subject device's `CarriagePolicy::Leased` TTL, where known.
    pub device_max_ttl_ms: Option<u64>,
    /// The served certificate's own expiry, where known.
    pub grant_expires_at_ms: Option<u64>,
}

/// Fail-closed admission for the carriage topic, in the grammar's order.
#[derive(Clone, Debug)]
pub struct CarriageAdmissionPolicy {
    graph: [u8; 32],
    now_ms: u64,
    trusted_roots: Vec<[u8; 32]>,
    ceilings: CarriageCeilings,
    /// The holder's view of stored slots. `None` means the view could not be
    /// established, and admission refuses everything rather than pruning on
    /// partial information (the borrowed `IncompleteEpochOrder` posture).
    held: Option<HashMap<BlindedSlotId, HeldLease>>,
}

impl CarriageAdmissionPolicy {
    pub fn new(
        graph: [u8; 32],
        now_ms: u64,
        trusted_roots: Vec<[u8; 32]>,
        ceilings: CarriageCeilings,
        held: Option<HashMap<BlindedSlotId, HeldLease>>,
    ) -> Self {
        Self {
            graph,
            now_ms,
            trusted_roots,
            ceilings,
            held,
        }
    }
}

impl OperationPolicy<CarriageExt> for CarriageAdmissionPolicy {
    type LogId = [u8; 32];

    fn admit(&self, operation: &Operation<CarriageExt>) -> Result<Admission<[u8; 32]>, Reject> {
        let ext = &operation.header.extensions;

        // 1. The right lane. An operation naming another graph is not an
        //    attack by construction, but storing it would cross lanes.
        if ext.graph != self.graph {
            return Err(Reject::new(
                "carriage-wrong-graph",
                "operation addresses another graph's carriage topic",
            ));
        }

        // 2. Well-formed: a slot is always a full destructive replacement of
        //    a present body, and never travels without a live lease.
        if !ext.prune_flag.is_set() {
            return Err(Reject::new(
                "carriage-history-not-pruned",
                "carriage is a slot, never history; the prune flag is mandatory",
            ));
        }
        let Some(payload) = operation.header.payload_hash else {
            return Err(Reject::new(
                "carriage-missing-payload",
                "a carriage slot carries its record; a bodiless operation has nothing to lease",
            ));
        };
        if ext.expires_at_ms <= self.now_ms {
            return Err(Reject::new(
                "carriage-lease-expired",
                "an expired lease is refused at intake, not stored and purged later",
            ));
        }

        // 3. Ceilings: the backstop always, the knowable ones where known.
        if ext.expires_at_ms > self.now_ms.saturating_add(CARRIAGE_ABSOLUTE_CEILING_MS) {
            return Err(Reject::new(
                "carriage-exceeds-absolute-ceiling",
                "lease outlives the stack-wide backstop",
            ));
        }
        if let Some(ttl) = self.ceilings.device_max_ttl_ms {
            if ext.expires_at_ms > self.now_ms.saturating_add(ttl) {
                return Err(Reject::new(
                    "carriage-exceeds-device-ttl",
                    "lease outlives the device's carriage TTL",
                ));
            }
        }
        if let Some(grant_expiry) = self.ceilings.grant_expires_at_ms {
            if ext.expires_at_ms > grant_expiry {
                return Err(Reject::new(
                    "carriage-outlives-grant",
                    "lease outlives the authority it serves",
                ));
            }
        }

        // 4. The issuing authority vouches for exactly this lease and body.
        if !verify_lease(ext, payload, &self.trusted_roots) {
            return Err(Reject::new(
                "carriage-untrusted-issuer",
                "lease signature verifies against no trusted persona root",
            ));
        }

        // 5. Monotonicity, failing closed where the held view is unknown.
        let Some(held) = &self.held else {
            return Err(Reject::new(
                "carriage-order-unknown",
                "held-slot view unavailable; refusing rather than pruning on partial information",
            ));
        };
        let previous = held.get(&ext.slot);
        if let Some(previous) = previous {
            if ext.issue <= previous.issue {
                return Err(Reject::new(
                    "carriage-stale-issue",
                    "a late replacement cannot resurrect a superseded slot",
                ));
            }
        }

        // 6. Only now, the destructive replace: prune the slot's log prefix
        //    and erase the superseded payload in the same backend batch.
        let target = StoreTarget::new(
            Topic::from(carriage_topic(self.graph)),
            carriage_log(ext.slot),
        );
        let admission = Admission::prune_before_current(target);
        Ok(match previous {
            Some(previous) => admission.erasing_payloads([previous.payload]),
            None => admission,
        })
    }
}

/// Why the purge proposal keeps one lease.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarriageRetentionReason {
    /// The lease has not reached its expiry.
    Live,
    /// A global blocker keeps otherwise-expired leases untouched.
    ProposalBlocked,
}

/// One lease the purge proposal retains, with its reason.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetainedLease {
    pub slot: BlindedSlotId,
    pub expires_at_ms: u64,
    pub reason: CarriageRetentionReason,
}

/// Gate that prevents every destructive candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarriagePurgeBlocker {
    /// The held-slot view could not be established.
    HeldViewUnavailable,
}

/// Reviewable dry run: `expired` is populated only when no blocker stands,
/// and executing it stays the host's separate act. The propose/execute split
/// is borrowed from `stickleback::epoch_retention`'s production consumers;
/// the engine itself is not, because its checkpoint gate guards a projection
/// carriage does not have.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CarriagePurgeProposal {
    pub retain: Vec<RetainedLease>,
    pub expired: Vec<BlindedSlotId>,
    pub blockers: Vec<CarriagePurgeBlocker>,
}

impl CarriagePurgeProposal {
    pub fn is_executable(&self) -> bool {
        self.blockers.is_empty()
    }
}

/// Propose which held leases the scheduled pass may purge.
///
/// Pure over its inputs, so a blocked or surprising purge is reviewable and
/// testable without a runtime. Ruling 4's second enforcement point; the first
/// is the read-side refusal in admission.
pub fn propose_carriage_purge(
    now_ms: u64,
    held: Option<&HashMap<BlindedSlotId, HeldLease>>,
) -> CarriagePurgeProposal {
    let Some(held) = held else {
        return CarriagePurgeProposal {
            retain: Vec::new(),
            expired: Vec::new(),
            blockers: vec![CarriagePurgeBlocker::HeldViewUnavailable],
        };
    };
    let mut retain = Vec::new();
    let mut expired = Vec::new();
    let mut slots: Vec<_> = held.iter().collect();
    slots.sort_by_key(|(slot, _)| **slot);
    for (slot, lease) in slots {
        if lease.expires_at_ms <= now_ms {
            expired.push(*slot);
        } else {
            retain.push(RetainedLease {
                slot: *slot,
                expires_at_ms: lease.expires_at_ms,
                reason: CarriageRetentionReason::Live,
            });
        }
    }
    CarriagePurgeProposal {
        retain,
        expired,
        blockers: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use p2panda_core::cbor::{decode_cbor, encode_cbor};
    use p2panda_core::{Body, Header, SigningKey};

    const GRAPH: [u8; 32] = [3; 32];
    const NOW_MS: u64 = 1_700_000_000_000;

    fn issuer() -> Ed25519Keypair {
        Ed25519Keypair::from_seed([0x51; 32])
    }

    fn slot() -> BlindedSlotId {
        BlindedSlotId([0xAB; 32])
    }

    /// A signed carriage operation. The transport writer is deliberately a
    /// different key from the issuer: the grammar allows a relay to carry
    /// another device's slot, because the lease signature is what authorizes.
    fn operation(ext: CarriageExt) -> Operation<CarriageExt> {
        let body = Body::from_bytes(b"wrapped-epoch-record-stand-in");
        let writer = SigningKey::from_bytes(&[0x77; 32]);
        // p2panda 0.7.1 made the header's CBOR cache, size and digest private
        // and folded signing into the builder: `build` encodes, signs and
        // caches the digest in one step, so the struct-literal + `sign` pair
        // has no equivalent. `body` sets payload_size and payload_hash.
        let header = Header::builder()
            .body(body.as_bytes())
            .seq_num(0)
            .backlink(None)
            .build(&writer, ext);
        Operation {
            hash: header.hash(),
            header,
            body: Some(body),
        }
    }

    fn signed_ext(issue: u64, expires_at_ms: u64) -> CarriageExt {
        let payload = Body::from_bytes(b"wrapped-epoch-record-stand-in").hash();
        CarriageExt {
            graph: GRAPH,
            slot: slot(),
            issue,
            expires_at_ms,
            prune_flag: PruneFlag::new(true),
            issuer_signature: sign_lease(&issuer(), GRAPH, slot(), issue, expires_at_ms, payload),
        }
    }

    fn policy(held: Option<HashMap<BlindedSlotId, HeldLease>>) -> CarriageAdmissionPolicy {
        CarriageAdmissionPolicy::new(
            GRAPH,
            NOW_MS,
            vec![issuer().public_key().to_bytes()],
            CarriageCeilings::default(),
            held,
        )
    }

    fn holding(issue: u64) -> HashMap<BlindedSlotId, HeldLease> {
        HashMap::from([(
            slot(),
            HeldLease {
                issue,
                payload: Hash::digest(b"superseded-record"),
                expires_at_ms: NOW_MS + 1_000,
            },
        )])
    }

    #[test]
    fn a_fresh_lease_admits_as_a_destructive_replace_and_erases_the_predecessor() {
        let op = operation(signed_ext(2, NOW_MS + 60_000));
        let admission = policy(Some(holding(1))).admit(&op).unwrap();
        assert!(matches!(
            admission.history,
            stickleback::HistoryAction::PruneBeforeCurrent
        ));
        assert_eq!(
            admission.erase_payloads,
            vec![Hash::digest(b"superseded-record")]
        );
        assert_eq!(
            admission.target.topic,
            Topic::from(carriage_topic(GRAPH)),
            "the slot lands on the carriage topic, never the graph's"
        );
    }

    #[test]
    fn the_refusal_ladder_names_each_failure() {
        let cases: Vec<(&str, Operation<CarriageExt>)> = vec![
            ("carriage-wrong-graph", {
                let mut ext = signed_ext(2, NOW_MS + 60_000);
                ext.graph = [4; 32];
                operation(ext)
            }),
            ("carriage-history-not-pruned", {
                let mut ext = signed_ext(2, NOW_MS + 60_000);
                ext.prune_flag = PruneFlag::new(false);
                operation(ext)
            }),
            ("carriage-lease-expired", operation(signed_ext(2, NOW_MS))),
            (
                "carriage-exceeds-absolute-ceiling",
                operation(signed_ext(2, NOW_MS + CARRIAGE_ABSOLUTE_CEILING_MS + 1)),
            ),
            ("carriage-untrusted-issuer", {
                // Signed by a key nobody trusts. The lease facts are fine.
                let payload = Body::from_bytes(b"wrapped-epoch-record-stand-in").hash();
                let stranger = Ed25519Keypair::from_seed([0x99; 32]);
                let mut ext = signed_ext(2, NOW_MS + 60_000);
                ext.issuer_signature =
                    sign_lease(&stranger, GRAPH, slot(), 2, NOW_MS + 60_000, payload);
                operation(ext)
            }),
            (
                "carriage-stale-issue",
                operation(signed_ext(1, NOW_MS + 60_000)),
            ),
        ];
        for (code, op) in cases {
            let reject = policy(Some(holding(1))).admit(&op).unwrap_err();
            assert_eq!(reject.code, code);
        }
    }

    #[test]
    fn an_unknown_held_view_refuses_everything() {
        let op = operation(signed_ext(2, NOW_MS + 60_000));
        let reject = policy(None).admit(&op).unwrap_err();
        assert_eq!(reject.code, "carriage-order-unknown");
    }

    #[test]
    fn the_knowable_ceilings_bind_where_known() {
        let op = operation(signed_ext(2, NOW_MS + 60_000));
        let mut with_ttl = policy(Some(holding(1)));
        with_ttl.ceilings = CarriageCeilings {
            device_max_ttl_ms: Some(30_000),
            grant_expires_at_ms: None,
        };
        assert_eq!(
            with_ttl.admit(&op).unwrap_err().code,
            "carriage-exceeds-device-ttl"
        );
        let mut with_grant = policy(Some(holding(1)));
        with_grant.ceilings = CarriageCeilings {
            device_max_ttl_ms: None,
            grant_expires_at_ms: Some(NOW_MS + 30_000),
        };
        assert_eq!(
            with_grant.admit(&op).unwrap_err().code,
            "carriage-outlives-grant"
        );
    }

    #[test]
    fn the_lease_signature_binds_the_exact_record() {
        // Same lease facts, different body: a kept lease cannot vouch for a
        // swapped record.
        let ext = signed_ext(2, NOW_MS + 60_000);
        assert!(verify_lease(
            &ext,
            Body::from_bytes(b"wrapped-epoch-record-stand-in").hash(),
            &[issuer().public_key().to_bytes()],
        ));
        assert!(!verify_lease(
            &ext,
            Body::from_bytes(b"a-different-record").hash(),
            &[issuer().public_key().to_bytes()],
        ));
    }

    #[test]
    fn the_two_grammars_cannot_be_crossed_by_accident() {
        // A graph-lane extension does not decode as a carriage extension, so
        // no PersonalGraphEvent operation is admissible on the carriage topic
        // even before any policy check runs.
        #[derive(Serialize)]
        struct GraphExtStandIn {
            graph: [u8; 32],
        }
        let bytes = encode_cbor(&GraphExtStandIn { graph: GRAPH }).unwrap();
        let decoded: Result<CarriageExt, _> = decode_cbor(bytes.as_slice());
        assert!(decoded.is_err(), "a graph extension decoded as carriage");
    }

    #[test]
    fn the_purge_proposal_is_pure_reviewable_and_blocked_without_a_view() {
        let mut held = holding(1);
        held.insert(
            BlindedSlotId([0xCD; 32]),
            HeldLease {
                issue: 3,
                payload: Hash::digest(b"expired-record"),
                expires_at_ms: NOW_MS,
            },
        );
        let proposal = propose_carriage_purge(NOW_MS, Some(&held));
        assert!(proposal.is_executable());
        assert_eq!(proposal.expired, vec![BlindedSlotId([0xCD; 32])]);
        assert_eq!(proposal.retain.len(), 1);
        assert_eq!(proposal.retain[0].reason, CarriageRetentionReason::Live);

        let blocked = propose_carriage_purge(NOW_MS, None);
        assert!(!blocked.is_executable());
        assert!(
            blocked.expired.is_empty(),
            "a blocked purge proposes nothing"
        );
        assert_eq!(
            blocked.blockers,
            vec![CarriagePurgeBlocker::HeldViewUnavailable]
        );
    }
}
