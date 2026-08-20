# The epoch carriage lane grammar

**Decided 2026-08-18, with Mark. Implemented 2026-08-19**: the grammar
(`graphshell::carriage`, `pandect::blinded_slot_id`, `CarriagePolicy` on the
roster's `DeviceRecord`) and the host (`graphshell::native::carriage_host`),
with the recovery done-condition demonstrated end to end over live sync.
Divergences from this text are noted inline where they occur. Closes the last
gate on
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
carriage_topic(graph) = blake3::derive_key("mere.graphshell.carriage.topic.v1", graph)
```

(As built 2026-08-19. This doc first wrote the formula as `BLAKE3(graph ++
CARRIAGE_TOPIC_MIX)`, mirroring `sync_overlay_topic`'s concat-a-mix-value
idiom; the implementation uses BLAKE3's own derive-key mode instead, which is
the same separation property without a loose constant to manage. Upstream's mix
value exists because their sync topic predates the derivation; ours does not.) What an
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

As built, the signature covers a domain-separated message over `(graph, slot,
issue, expires_at_ms, payload_hash)` rather than literal header bytes, since
the signature rides inside the header it would otherwise have to sign. Binding
the payload hash means a lease vouches for one exact record: swapping the body
under a kept lease fails verification, and the test asserts it. Verification
**tries each trusted root** instead of reading an issuer key off the wire,
because a published issuer key would be a stable per-persona value on an
announced topic, exactly the grouping the blinding prevents. The root set is
one per persona of the owner, so the trial loop is nothing.

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
   and `CARRIAGE_ABSOLUTE_CEILING_MS`. (Refined in implementation: only the
   issuing wallet knows a device's TTL and only a certificate holder knows the
   grant's expiry, so a bare replica can assert neither. `CarriageCeilings`
   makes the two knowable bounds optional and the backstop unconditional; each
   host enforces what its position lets it know, and issue-side code passes
   both. The backstop is thirty days.)
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

## The roster layering, ruled

**Ruled 2026-08-19 (Mark): the wallet roster governs carriage, the pairing
list governs reachability.**

Three device stores exist and the question was which is authoritative for the
replica set. The answer is that two of them are, for different halves, and the
third is derived:

- `personae::carry::DeviceRoster` decides **whether** a device holds carriage
  at all. It is where `CarriagePolicy` already lives, beside `mode` and
  `exposure`, and it is the wallet concept the certificate is keyed by. A
  device absent from it, or present with `CarriagePolicy::None`, replicates
  nothing regardless of how reachable it is.
- `owner_settings`' `paired_devices` + `roster_roots` decide **whether** that
  device is reachable: the Personae root that admits its writes, the
  `last_endpoint` dial hint, the receive-only tier. Pairing a device grants it
  no carriage, and unpairing it removes a route, not an authority.
- `SyncRoster` stays what it is, the runtime projection of the pairing list,
  and learns nothing about carriage.

The carriage replica set is therefore the **intersection**: devices the wallet
roster grants carriage that the pairing list can reach. Neither store takes on
the other's job, and the failure modes stay legible. A device granted carriage
but unpaired is an authority with no route, which is a commissioning gap the
operator can see. A device paired but ungranted is a route with no authority,
which is the default and the safe state.

The mapping this needs is the one seam: the wallet roster is keyed by
`DeviceId` / `DevicePublicKey`, the pairing list by Personae root. The
`DeviceRecord` already carries the pubkey and the pairing record already
carries the root, so the join is a lookup, not a schema change.

## One log per slot, which the prune law forced

**Found at implementation, 2026-08-19.** The first cut gave the lane a single
per-author log. That is wrong, and the prune law is why:
`PruneBeforeCurrent` deletes every operation before the admitted one *in its
log*, so two slots sharing a log would let one slot's supersession destroy the
other slot's live version. **The log id is the slot itself** (`[u8; 32]`
satisfies p2panda's blanket `LogId` impl), so a prune cannot reach past its
own slot by construction rather than by care.

## The host, as built

`graphshell::native::carriage_host::CarriageHost` joins the carriage topic,
mirrors `PersonalSyncHost`'s transport wiring, and holds the slot view the
admission policy's ordering check reads, rebuilt from the store at open so a
restart cannot desynchronize it. Its own endpoint for now: `set_topics`
replaces a peer's topic set, so folding both lanes onto one bound endpoint
waits on an append form; noted in the module header rather than hidden.

Two behaviours worth naming. **The issuer admits its own writes** through the
same policy peers apply, so a lease violating a knowable ceiling is refused
loudly at issue rather than silently dropped by every peer. And **recovery
refuses expired on read** even when bytes are still present, so a replica
never serves material its lease no longer covers.

## Done conditions

**Status 2026-08-19**, each against a named test in `carriage.rs` /
`carriage_host.rs`: recovery without re-pairing is **demonstrated**
(`a_peer_recovers_a_live_slot_and_supersession_replaces_it`, two hosts over
live sync, the replica learning the slot from sync alone); supersession
replaces and a stale issue is refused (same test, plus
`the_refusal_ladder_names_each_failure`); expiry converges with zero messages
(`an_expired_lease_is_refused_on_read_and_purged_on_schedule`), though the
revocation-statement fast path is not yet wired; the ceilings bind where
knowable (`the_knowable_ceilings_bind_where_known`,
`a_ceiling_violation_is_refused_at_the_issuer`); the grammars cannot be
crossed (`the_two_grammars_cannot_be_crossed_by_accident`); and replica
nescience is structural, since the host holds only blinded slots and trusted
roots. The contract as ruled:

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
- A device granted carriage but unpaired, and a device paired but ungranted,
  both replicate nothing, and each absence is visible in its own store.

## What this does not decide

- ~~The trusted-peer roster.~~ **Ruled 2026-08-19**, one section up: the
  wallet roster governs carriage, the pairing list governs reachability, and
  the replica set is their intersection. What remains open is only membership
  *governance* for peers beyond one's own devices, still shared with the
  participant gate.
- **Whether a relay may carry another device's slot.** The grammar permits it,
  since the issuer signature is what authorizes and the writer is separate. Who
  is willing to relay is a roster question, above.
- ~~Wiring `WalletEpochSealer` into a host.~~ **Done 2026-08-19.**
  `castellan::authority::PersonaeHost::payload_sealer` is the supply point, and
  access records are the first consumer. The recovery done-condition above now
  has a live seal path to demonstrate against.
- ~~Issue-path integration.~~ **Done 2026-08-19.**
  `CarriageHost::publish_grant_carriage` walks the roster, publishes a slot
  for every device leased onto this graph whose persona certificates carry
  epoch material, and reports what it honestly could not: a leased device
  whose wrapping key the wallet never retained (the direct issue path retains
  none; pairing does), a certificate with no record yet, an already-expired
  grant. The issuer signs with the persona's chain-root keypair and passes
  the full ceilings, so a bad lease fails at issue. The commissioning test
  runs the real machinery end to end: pairing issue with a private epoch,
  roster lease, roster-driven publish, live sync, recovery on a peer, and
  the recovered record opened with the pairing key
  (`a_paired_leased_device_recovers_its_epoch_through_a_peer`). One
  layering fact it surfaced: `publish_slot` now takes ceilings per call,
  because the issuer's knowledge is per grant, not per host.
- **The revocation fast path.** A received `SignedDelegationRevocation` should
  destroy the certificate's leases immediately; today only expiry converges.
- **One endpoint for both lanes.** Blocked on `set_topics` being
  replace-not-append; the carriage host runs its own endpoint meanwhile.
