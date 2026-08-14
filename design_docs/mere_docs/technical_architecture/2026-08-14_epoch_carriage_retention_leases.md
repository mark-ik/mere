# Epoch carriage retention: leased slots

**Proposed 2026-08-14, awaiting Mark's ratification.** Scopes ruling 4 of
[epoch carriage and replication](2026-08-14_epoch_carriage_replication.md),
which gated shipping on an unanswered retention question. Related: the
[listener-executive lease doctrine](../../../../retinue/design_docs/2026-08-10_listener_executive_and_protocol_leases.md),
the [sited device identity brief](../research/2026-08-10_sited_device_identity_brief.md),
`session_runtime::wallet_grant`, `muniment::SlotStore`.

## The threat, sharpened

The carriage doc stated the problem as "replication defeats `purge_deleted`".
Scoping it required asking what a lingering replica actually adds for each
attacker, and the answer narrows the problem usefully.

A **live-stolen device** already holds every epoch secret it was granted; it
unwraps them as part of normal operation. Replicas of its carriage record add
nothing. Rotation is the protection against this attacker, it already works,
and retention cannot improve on it.

The replica-specific attacker holds a **wrapping key without the device's live
state**: a cold-stolen locked device, a leaked pairing secret, an old device
backup. Against that attacker the replica set is a second keyring copy that
`purge_deleted` cannot reach. Locally this copy does not exist, because
rotation already *replaces* rather than accumulates:
`refresh_remote_auth_private_read_grant` clears the record and pushes the
re-wrap, and `save_wrapped_epoch_record` overwrites the wallet file. The local
wallet holds current material only.

(Corrected 2026-08-14: this read "retains-out the persona's old entry", which
was true until the blinding landed the same day. The record is keyed by one
persona's certificate, so every entry in it is that persona's and the clear is
unconditional. The premise is stronger than when it was written, not weaker.)

So retention's whole job is: **make cooperative replicas converge to the same
current-version-only state the local wallet already maintains, within a
bounded window, without assuming every peer is reachable.**

## Ruling (proposed)

1. **Epoch carriage replicates as a leased slot, never as history.** The
   record is keyring state, not memory; it never enters the content-addressed
   engram lane.
2. **Every replicated record carries a mandatory, issuer-signed expiry**, and
   never outlives the grant it serves: `lease.expires_at_ms <=
   grant.expires_at_ms`, asserted at issue and at install.
3. **Supersession is destructive.** A monotonic issue counter orders
   versions; installing version N+1 destroys N; a late-arriving N is refused.
   A late replacement cannot resurrect, same rule as signalman's live lease.
4. **The holder enforces expiry twice**: an expired lease is refused on read,
   and purged by the scheduled pass. Renewal is a re-publish with a strictly
   later expiry.
5. **Revocation delivery is an optimization, not a dependency.**
   `RemoteAuthRevocationOutcome.statements` already returns signed withdrawal
   statements because "these are what travels"; a peer that receives one
   destroys the certificate's leases immediately. A peer that never receives
   it converges at expiry anyway. Precondition 2 therefore does not wait on
   retinue's open mesh revocation-propagation question (sited brief, open
   question 3).

## Why this shape is the idiomatic one

### Slot, not history

Eidetic's R0 temporal-integrity contract (append-only, tombstoned, built not
to forget) is the right posture for memory and the wrong one for key
material; "content-addressed history is built not to forget" was the exact
sentence naming the problem. The stack already has the other seam:
`muniment::SlotStore`, typed mutable slots with overwrite semantics, and the
wallet file itself already behaves as a slot. Replicating the record as a
slot in a named, narrow **carriage lane** (destructive-replace, never
history) keeps both contracts clean: the engram lane never carries carriage
records, the carriage lane never accumulates versions.

### Lease, not durable state

The contract is already ruled twice in this ecosystem. Retinue's doctrine:
protocol modes are bounded leases, never durable states, and nothing can
prolong a lease past its declared window. Signalman's `SitedStationHead`:
mandatory expiry, renewal must carry a strictly later expiry and be accepted
before the old deadline, a late replacement cannot resurrect, local
revocation drops immediately. Carriage adopts the same contract rather than
inventing a third retention idiom.

TTL sizing follows the sited brief's rule for grant refresh: sized to the
worst link among the devices meant to benefit, with the grant expiry as the
hard ceiling. A carriage lease serving a LoRa-reachable radio is long; one
serving two desktops on a LAN is short. The exposure window after a
wrapping-key compromise is at most the TTL, so the same knob trades recovery
convenience against harvest exposure, legibly.

### Honest bounds, no placebo

What the design claims: cooperative replicas hold current material only,
within TTL of any rotation, and drop a revoked certificate's material within
TTL even if no revocation statement is ever delivered.

What it does not claim: erasure from an uncooperative or already-compromised
peer. Nothing can claim that; bytes copied are bytes kept. Against that peer
the guarantees are cryptographic only (per-device wrap, blinded-index
unlinkability), and they hold with or without retention. The doc states this
plainly so the feature is never described as remote erasure.

## Implementation shape (illustrative, gated on a transport existing)

No code moves now; there is no replication transport to gate. When one
exists (murm/moot shaped), the pieces are:

- A `LeasedEpochRecord` wrapper over `WrappedEpochRecord`: `issued_at_ms`,
  `expires_at_ms`, `issue: u64` (monotonic per certificate), issuer signature
  over the encoded record. A wrapper rather than new fields, so the local
  wallet file is untouched and an un-leased record is unrepresentable in the
  carriage lane by construction.
- Install path: verify signature, check monotonicity against the held
  version, assert the grant-expiry ceiling, destructive-replace.
- Read path: refuse expired (`now >= expires_at_ms` refuses, matching the
  station grant check).
- Scheduled purge: the steady-heat shape Athanor already has for tombstone
  age-out, applied to expired leases.

## Done conditions

- A device that lost its record recovers from a peer replica while the lease
  is live, without re-pairing.
- A superseded version presented to a holder of a newer one is refused.
- A revoked certificate's leases vanish from a cooperative peer at expiry
  with zero messages delivered, and immediately when a revocation statement
  arrives.
- No lease exists whose expiry exceeds its grant's.

## What this does not decide

- **The transport.** Which lane physically moves carriage slots between
  trusted peers is murm/moot shaped and undesigned.
- **The trusted-peer roster.** Who the peers are and how membership is
  governed; shared with the participant gate's admission machinery.
- **Mesh-wide revocation flooding.** Sited brief question 3 stays open; this
  design converts it from a precondition into a latency optimization.
