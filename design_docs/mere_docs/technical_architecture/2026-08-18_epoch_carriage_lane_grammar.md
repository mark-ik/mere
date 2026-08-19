# The epoch carriage lane grammar

**Decided 2026-08-18, with Mark.** Closes the last gate on
[epoch carriage retention: leased slots](2026-08-14_epoch_carriage_retention_leases.md),
whose implementation section was left "gated on a carriage lane". Follows
[epoch carriage and replication](2026-08-14_epoch_carriage_replication.md)
(precondition one, the blinded index) and the
[stickleback review](../research/2026-08-16_carriage_against_stickleback_epochs.md).
Reads `graphshell::personal_sync`, `stickleback::processor`,
`stickleback::prune_proof`, `murm::transport::p2panda_transport`,
`muniment::slot`, `pandect::wallet_grant::certificate`.

Two findings reshaped this before any grammar was written. Both were of the
same kind: a mechanism the proposal named turned out to be the wrong one, and
the right one already existed a layer down.

## 1. Slot semantics are a protocol operation, not a store choice

The leases proposal grounds "slot, never history" in `muniment::SlotStore`,
"typed mutable slots with overwrite semantics". That store is real and it is
**local only**: `save`, `load`, `delete` over a backend, with no replication in
it. It is the right seam for the wallet file. It cannot be the seam for a
replicated slot, because what replicates here is a p2panda log, and a log is
append-only by construction.

The replicated version already exists in stickleback:

- `HistoryAction::PruneBeforeCurrent`, "Delete every operation before the
  admitted operation".
- `Admission::prune_before_current(target)`, "Admit an operation as the
  surviving head of a pruned prefix".
- `.erasing_payloads(..)`, which erases named payloads "in the same backend
  batch as this insert", and ignores absent ones because "a peer may receive a
  checkpoint after the named prefix has already been collected locally".
- `PruneFlag` in the header extension, `validate_prunable_backlink` from
  `p2panda_core::prune`, and `prune_proof.rs`, a focused proof "that p2panda's
  prune law composes with the shared store".

`drop_io` already runs the pair live, in one receipted commit. So carriage
invents nothing for destructive supersession. **The lease is not a new
retention mechanism; it is the authorization for a prune the protocol already
performs.** Ruling 3's monotonic issue counter becomes the backlink discipline,
and ruling 1's "never as history" becomes a `PruneFlag` the policy requires
rather than merely permits.

## 2. The topic is a privacy decision

`sync_overlay_topic` records that a topic's gossip overlay is joined on a
derived value and that "in production, discovery announces it". Topic
membership is therefore observable before any payload is read, which makes
choosing what the carriage topic keys on a continuation of the blinding work
rather than a naming question.

A per-persona topic was the obvious fit for carriage's real granularity and is
rejected for exactly that reason: subscription to it would let an observer
reconstruct which devices serve which persona, which is the association graph
precondition one exists to prevent. Blinding the record and then announcing the
persona at the transport layer would be self-defeating.

**Ruling: a sibling topic derived from the graph topic.**

```
carriage_topic(graph) = BLAKE3(graph ++ CARRIAGE_TOPIC_MIX)
```

The same idiom `sync_overlay_topic` uses, with its own mix value. What an
observer learns is that a device on this graph also carries key material, and
graph peers already know it is on this graph. No new principal is named.

A separate topic, rather than a second log on the graph topic, keeps one
payload grammar per topic. `StoreTarget` is `(topic, log_id)`, so a second log
would also be a separate lane in storage, and it would be stronger still on
privacy since it adds no subscription at all. It was not taken because it puts
`Operation<PersonalGraphExt>` and `Operation<CarriageExt>` on one topic, and
nothing in the sync path discriminates payload grammars today. That is a real
decoder change for a marginal gain over a topic any graph peer can derive
anyway.

### The granularity mismatch, stated rather than hidden

Carriage is per `(device, persona-certificate)`. The topic is per graph. A
persona present in three graphs has three derivable carriage topics and needs
only one.

The rule: **the issuing wallet picks the graph at issue and records it on the
roster's `DeviceRecord`, beside `mode`, `exposure` and `CarriagePolicy`.** It is
chosen, not derived, because nothing in the certificate implies a graph. This
is the one place the mismatch surfaces, and making it an explicit
commissioning-time field keeps it visible where the other per-device posture
decisions already are.

## What travels

Everything an admitting peer must check lives in the header extension, so a
replica can accept, refuse, or prune **without decoding the body**. That is
`personal_sync`'s own discipline ("Everything checkable without the body,
first") and it matters more here, because a replica peer is not expected to
hold the key that opens the body at all.

```rust
pub struct CarriageExt {
    /// Which carriage lane this belongs to.
    pub graph: [u8; 32],
    /// Blinded slot identity. NOT the certificate id.
    pub slot: BlindedSlotId,
    /// Monotonic per slot. Ruling 3's ordering, as the backlink discipline.
    pub issue: u64,
    /// Mandatory. Ruling 2; no un-leased record is representable.
    pub expires_at_ms: u64,
    /// Ruling 1, as a protocol fact rather than a convention.
    pub prune_flag: PruneFlag,
    /// The issuing authority's signature over the header, payload hash
    /// included. Distinct from the transport writer's signature, because the
    /// writer may be a relay and the authority is the certificate's issuer.
    pub issuer_signature: Signature,
}
```

The body is the existing `WrappedEpochRecord`, unchanged, whose entries are
already blinded by precondition one.

### The slot id must be blinded too

This is the finding that the extension design forced. `WrappedEpochRecord` is
keyed by `certificate: DelegationId`, and the record needed addressing in the
clear so a holder could find its own. Putting that id in the extension would
publish a stable identifier for one `(device, persona)` pair on an announced
topic, which reintroduces the leak the blinded index closed one level up.

So the slot takes the same construction as the index, under its own context:

```
slot = keyed_hash(derive_key(BLIND_SLOT_CONTEXT, wrapping_key), certificate_id)
```

The holder can compute it, because it holds the wrapping key. The issuer can,
because it held the key at issue. A replica cannot, and does not need to: it
stores bytes at an opaque slot and enforces a lease over them. That a replica
can do its whole job while knowing nothing about what it holds is the property
worth having, and it falls out rather than being added.

## Admission, in order, fail closed

`OperationPolicy::admit` is called after the shared processor has verified
signature, header, and body hash, and before any storage mutation. The carriage
policy checks, refusing at the first failure:

1. **Topic** is the carriage topic of a graph this device syncs.
2. **Extension well-formed**, and `expires_at_ms` is present and in the future.
   Ruling 4's read-side refusal, applied at intake so an expired lease is never
   stored in the first place.
3. **Three ceilings** hold: the device's carriage TTL, the grant's own expiry,
   and `CARRIAGE_ABSOLUTE_CEILING_MS`.
4. **Issuer signature** verifies against a trusted root. Note the M5
   consequence inherited here: that is one root per persona, not one master
   key.
5. **Monotonicity**: `issue` is strictly greater than the held version for this
   slot. Equal or lower is refused, which is signalman's rule that a late
   replacement cannot resurrect.
6. Only then, `Admission::prune_before_current(target)
   .erasing_payloads([previous_payload])`.

**Fail closed where ordering is unknown.** If the held version for a slot
cannot be determined, admission refuses rather than accepting on partial
information. This is the stickleback review's borrowed posture, where
`IncompleteEpochOrder` blocks an entire proposal rather than pruning on what it
happens to know.

Refusal codes follow `personal_sync`'s convention of a stable code plus
diagnostic detail, so a refused carriage operation is legible in the same way a
refused graph operation is.

## Purge, as propose then execute

The scheduled pass is a pure function over held slots returning retained leases
with a reason each, expired leases as candidates, and `blockers` where it
refuses. Execution is separate. This is the split both production consumers of
`stickleback::epoch_retention` implement, and the reason for it here is the
same: a blocked purge should be reviewable rather than silent.

Carriage does **not** use `propose_epoch_pruning` itself. That function gates on
`MissingCheckpoint`, a checkpoint proving the domain can rebuild its projection
without the forgotten epochs, and carriage has no projection to rebuild. It
would be blocked forever, exactly as `graph_keys`' `retention_probe` is. The
shape is borrowed; the function is not.

## Done conditions

- A device that lost its record recovers it from a peer replica while the lease
  is live, without re-pairing.
- A replica peer that holds no wrapping key can accept, refuse, prune and expire
  a carriage slot correctly, and can name none of the personas it serves.
- A superseded version presented to a holder of a newer one is refused, and the
  holder's stored prefix is gone rather than retained.
- A revoked certificate's leases vanish from a cooperative peer at expiry with
  zero messages delivered, and immediately when a revocation statement arrives.
- No lease exists whose expiry exceeds its grant's, its device's configured TTL,
  or the absolute ceiling.
- No carriage record is representable as a `PersonalGraphEvent`, and no
  `PersonalGraphEvent` is admissible on the carriage topic.
- A device with no carriage policy set replicates nothing.

## What this does not decide

- **The trusted-peer roster.** Who replicates, and how membership is governed.
  Unchanged from the leases doc, and shared with the participant gate.
- **Whether a relay may carry another device's slot.** The grammar permits it,
  since the issuer signature is what authorizes and the writer is separate. Who
  is willing to relay is a roster question, above.
- **Wiring `WalletEpochSealer` into a host.** It is constructed nowhere outside
  its own tests, and `eidetic::seal` says the seam "changes no runtime
  behavior" until a host wires a sealer in. Carriage delivers material for a
  seal path that is defined and not yet live, so the recovery done-condition
  above has nothing to demonstrate against until that lands. This is a
  prerequisite for shipping, not for designing.
