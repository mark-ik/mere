# Epoch carriage retention: leased slots

**Ratified 2026-08-14, by Mark.** Scopes ruling 4 of
[epoch carriage and replication](2026-08-14_epoch_carriage_replication.md),
which gated shipping on an unanswered retention question.

**Amended 2026-08-16**, folding in the four amendments from
[carriage against stickleback's epoch retention](../research/2026-08-16_carriage_against_stickleback_epochs.md),
a review of this doc against machinery that already exists. The central choice
survives the review unchanged; three surrounding claims did not.

Related: the
[listener-executive lease doctrine](../../../../retinue/design_docs/2026-08-10_listener_executive_and_protocol_leases.md),
the [sited device identity brief](../research/2026-08-10_sited_device_identity_brief.md),
`pandect::wallet_grant` (named `session_runtime` when this was written),
`muniment::SlotStore`, `stickleback::epoch_retention`,
`graphshell::personal_sync`, `graphshell::native::graph_keys`.

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

## Ruling

1. **Epoch carriage replicates as a leased slot, never as history.** The
   record is keyring state, not memory; it never enters the content-addressed
   engram lane.
2. **Every replicated record carries a mandatory, issuer-signed expiry**,
   bounded by three separate ceilings (amended 2026-08-14, below): the
   device's own carriage TTL, the grant it serves, and a stack-wide backstop.
   Asserted at issue and at install.
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

**Corrected 2026-08-18: `muniment::SlotStore` is the wrong seam for the
replicated half.** It is local only, `save`/`load`/`delete` over a backend,
with no replication in it, so it is right for the wallet file and cannot be
what a peer holds. What replicates is a p2panda log, append-only by
construction. The replicated slot already exists one layer down, in
stickleback: `HistoryAction::PruneBeforeCurrent` deletes every operation before
the admitted one, `Admission::prune_before_current` admits an operation "as the
surviving head of a pruned prefix", `erasing_payloads` clears named payloads in
the same backend batch, and `prune_proof.rs` proves the composition. `drop_io`
runs the pair live. So this section's conclusion holds and its mechanism does
not: **the lease is the authorization for a prune the protocol already
performs**, and ruling 1 becomes a `PruneFlag` the policy requires rather than a
convention it states. See the
[lane grammar](2026-08-18_epoch_carriage_lane_grammar.md).

### Lease, not durable state

The contract is already ruled twice in this ecosystem. Retinue's doctrine:
protocol modes are bounded leases, never durable states, and nothing can
prolong a lease past its declared window. Signalman's `SitedStationHead`:
mandatory expiry, renewal must carry a strictly later expiry and be accepted
before the old deadline, a late replacement cannot resurrect, local
revocation drops immediately. Carriage adopts the same contract rather than
inventing a third retention idiom.

**Amended 2026-08-14 (Mark): the TTL is per device, and the grant is not the
only ceiling.**

This section first read "sized to the worst link among the devices meant to
benefit, with the grant expiry as the hard ceiling". Both halves were wrong,
and in the same direction: they loosen the bound exactly where exposure is
worst.

*Worst-link sizing is backwards as a global rule.* It takes the device with
the most constrained link and imposes its window on everything else, so the
device that most needs a long lease sets the harvest window for devices that
never needed one. The doc's own example already argued per-device; the sizing
rule contradicted it.

Per-device is also the honest unit, because **the wrapping key is per-device**.
A compromised key opens only that device's carriage, so that device's TTL is
already the true exposure bound for it. Making the knob per-device just stops
one device's constraint from bounding another's exposure.

*The grant expiry is a correctness ceiling, not an exposure bound.* Grant
windows are sized by re-issue cost. Carriage windows are sized by harvest
exposure. Two different questions were sharing one bound, and the shared bound
was the loose one.

**Corrected 2026-08-16 after review: the worked example was of a case that
cannot arise.** This amendment and the sizing rule it replaced both argued from
the sited radio ("a carriage lease serving a LoRa-reachable radio is long; one
serving two desktops on a LAN is short"). **A sited radio has no carriage to
lease.** Verified along the whole chain rather than assumed:
`store_wrapped_epochs` iterates `set.personas` only, so no device-scoped
certificate ever gets a record; `requires_epoch_material` keys on
`ACTION_PRIVATE_READ`; `is_persona_scoped_action` puts `private.read` on the
persona side of the partition, so it can only land on a persona certificate;
and castellan's station policy refuses a set containing persona certificates
outright (`SitedStationGrantError::PersonaAuthority`). A transport-only station
holds no wrapped-epoch record at all.

The conclusion survives on its own terms, because the wrapping key really is
per-device and that really is the honest unit for an exposure budget. What
changes is the sizing argument. The devices that hold carriage are the ones
that act for a persona: ordinary personal devices on ordinary links, broadly
the same population that could hold a graph key-group seat. The "worst link"
framing was reaching for constrained radios that are out of scope, and TTLs
should be argued from persona-holding devices, where the recovery benefit is
real and the links are unremarkable.

So the lease ceiling becomes a conjunction of three bounds answering three
questions:

```
lease.expires_at_ms <= min(
    now_ms + device.carriage.max_ttl_ms,  // this device's exposure budget
    grant.expires_at_ms,                  // never outlive the authority
    now_ms + CARRIAGE_ABSOLUTE_CEILING_MS // backstop against misconfiguration
)
```

### The overlap with the graph key group

**Added 2026-08-16 after review.** `graphshell::native::graph_keys` is already
"this device's membership in one personal graph's key group": pre-key
publication, an explicit create-then-add ceremony, and `GroupSession` state
held through Personae's `SealedRecordStorage`. It distributes key material to a
person's own devices, over the same lane carriage would use. This doc proposed
a second mechanism for that shape without mentioning the first.

The dividing line is real, and `graph_keys` states it exactly.
`recipient_for_root` returns `None` for "a device that was paired but never
joined the key group", which it calls the ordinary state of such a device.
Pairing and group membership are separate acts, deliberately: creating a group
"is an explicit act, not something a device does because it noticed it could".
So a device can hold a persona certificate, and therefore carriage, while
holding no seat in any key group.

~~Where both exist, the group session is the better channel.~~ **Wrong, and
corrected below.** The group session is authenticated, live, and carries its own
membership ceremony including `remove_and_rotate` for withdrawal, all of which
is true and none of which makes it a channel for what carriage carries.

**Settled 2026-08-18: carriage cannot be retired for seated devices, because
the two mechanisms never carried the same thing.** The lifecycle comparison
turned out to be unnecessary, because the question dissolves one level down, at
what each key actually seals.

- The group session distributes one **graph's** `DataKeyring`, which seals
  `PersonalGraphEvent` operation bodies as `GroupCiphertext` on the sync lane.
  A device without it holds a sealed operation it cannot even admit, because
  admission reads the body.
- Carriage distributes one **persona's** private epoch secret, which
  `WalletEpochSealer` turns, under `mere.pandect.engram_seal.key.v1`, into the
  key sealing that persona's eidetic engram payloads at rest, with content
  hash, persona and epoch bound as associated data.

Different granularity, graph against persona. Different thing sealed, lane
operations against stored payloads. Different derivation, with no shared root:
the session's storage key comes from `derive_keypair(SESSION_IDENTITY_CONTEXT
|| graph)`, the persona epoch from the wallet's `PersonaEpochBridge`. And
neither path references the other, checked rather than assumed:
`personal_sync` and `graph_keys` never mention `WalletEpochSealer`.

So a device seated in a graph's key group can read that graph's lane traffic
and still cannot open the persona's sealed store. Retiring carriage for seated
devices would withdraw the only delivery of the material that does that. The
paragraph above about the group session being "the better channel" compared two
things that are not alternatives; it is corrected there.

**What the overlap actually is.** Both mechanisms deliver key material to one
person's own devices over one transport, so they want a shared lane discipline
and a shared retention idiom. They do not want a shared payload, and neither
subsumes the other. That makes the carriage lane's grammar the whole of the
remaining work, and it makes the population it serves the full one: every
device holding a persona certificate with `private.read`, seated or not.

~~**Noted while checking.** `WalletEpochSealer` is constructed nowhere outside
its own tests.~~ **Wired 2026-08-19.** It was true when written, and
`eidetic::seal` had said so deliberately: the seam "changes no runtime
behavior" until a host wires a sealer in. The host is now
`castellan::authority::PersonaeHost::payload_sealer`, which is the right place
because the keeper already holds the carry root and no other component then has
to learn where the wallet lives. Access records are the first consumer, through
`save_access_record_sealed` / `query_access_records_sealed`.

The recovery done-condition below therefore has something to demonstrate
against. What the wiring proved, in three tests rather than by assertion: a
`LocalOnly` record leaves no cleartext in the store's blob bytes, a reader
without the epoch **refuses** rather than reporting an empty history (the
failure mode worth foreclosing, since "no history" is indistinguishable from
having browsed nothing), and a `PublicPortable` record stays cleartext under
the same willing sealer, which is the lane asymmetry holding.

### Why not stickleback's epoch-retention engine

**Added 2026-08-16 after review.** `stickleback::epoch_retention` is a live
engine for a problem that reads like this one, in production in
`moot::commons::chat` and `knot::sync`, each with a propose/execute pair.
Carriage does not adopt it, and the reason is principled rather than
incidental.

**The two models pull opposite directions on the same fact.**
`EpochHoldReason::OfflineMember`, "the governed offline-member policy still
promises recovery to this member", makes an unreachable member a reason to
**keep** an epoch. A carriage lease makes an unreachable holder a reason to
**drop** one. Both are right in their domain, and the difference follows from
what the material is for: group epochs decrypt durable shared documents, so
premature forgetting destroys data, while carriage epochs are delivery copies
of material the local wallet already holds current, so late forgetting is the
entire risk.

**The engine's central gate is one carriage neither has nor needs.**
`propose_epoch_pruning` blocks on `MissingCheckpoint` unless the domain
supplies an `EpochCheckpointBasis` proving it can rebuild its projection
without the epochs it would forget. That gate exists because forgetting a
group epoch makes every operation sealed under it permanently unreadable.
Nothing is lost that way by forgetting a carriage copy, because the local
wallet is the authority and holds current material by construction. Carriage
would supply no checkpoint and be blocked forever.

That is not hypothetical. `graph_keys` carries a `retention_probe` test that
runs the engine against a personal-graph keyring and asserts the proposal is
blocked on exactly `MissingCheckpoint`, calling it "the correct answer rather
than a limitation to work around". The same probe measures the pressure and
finds it slight: 103 bytes per epoch, one epoch per revocation, so "a mesh that
retired a device every week for a decade would spend 54 KB". For the group
lane, retention is a shape worth naming and not a pressure worth acting on.
Carriage acts where that lane does not because its motive is exposure, not
size.

**Also not adopted: `retained_data_epochs`,** count-based retention. It needs a
prune authorization to arrive, which is the dependency ruling 5 refuses.
Time-based expiry converges with zero messages delivered, which is the whole
point against a peer that may never be reachable again.

**Adopted, in shape.** A propose/execute split, matching what both production
consumers do: `execute_epoch_pruning` revalidates a reviewed proposal before
anything is destroyed. The purge below becomes a pure pass returning a
reviewable artifact, a reason per retained lease and `blockers` when it
refuses, with execution separate, so a blocked purge is legible rather than
silent. And the fail-closed posture: `IncompleteEpochOrder` blocks the entire
proposal rather than pruning on partial information, and carriage refuses in
the same situation. `epochs_oldest_first` is guarded that way because "lexical
secret-id order is not a safe substitute for chronology", which is the same
insight as ruling 3's monotonic issue counter, reached independently.

### Where the knob lives

On the roster's `DeviceRecord`, beside `mode` and `exposure`, which are already
the per-device posture decisions:

```rust
pub enum CarriagePolicy {
    /// Never replicated. The default, so replication is opt-in per device
    /// rather than a global switch someone can flip once.
    None,
    /// Replicated, with a lease of at most this long.
    Leased { max_ttl_ms: u64 },
}
```

Defaulting to `None` matters as much as the granularity. It makes an
unconsidered device un-replicated by construction, and it makes the decision
visible at commissioning, where the operator already knows the link and the
siting. `#[serde(default)]` keeps existing roster records readable.

**Not adopted: a per-persona axis.** The record is already scoped to one
(device, persona-certificate) pair, so a per-device policy already selects at
record granularity. A persona-level override can be added if a persona ever
needs a tighter bound than its device, and nothing here forecloses it.

### Honest bounds, no placebo

What the design claims: cooperative replicas hold current material only,
within TTL of any rotation, and drop a revoked certificate's material within
TTL even if no revocation statement is ever delivered.

What it does not claim: erasure from an uncooperative or already-compromised
peer. Nothing can claim that; bytes copied are bytes kept. Against that peer
the guarantees are cryptographic only (per-device wrap, blinded-index
unlinkability), and they hold with or without retention. The doc states this
plainly so the feature is never described as remote erasure.

## Implementation shape (illustrative, gated on a carriage lane)

**Amended 2026-08-16 after review.** This first read "gated on a transport
existing", with the transport called "murm/moot shaped and undesigned". The
transport exists. `graphshell::personal_sync` is H7 personal-device
synchronization, built on stickleback and p2panda with causal ordering and
policy-before-insert storage, peers authenticated through
`personae::DerivedKeyAttestation`, and already enforcing `PrivacyClass` in that
`AppendAccess` refuses `LocalOnly | MootScoped`. Its peer set is
device-to-device across one person's own devices, which is exactly the
trusted-peer set carriage wants, and `src/bin/h7_sync_peer.rs` is a 411-line
serve/connect binary the tree calls a "Physical H7 offline-edit and convergence
receipt".

What does not exist is a **lane**, and the separation is load-bearing rather
than tidy. `PersonalGraphEvent` is a graph grammar, of nodes, tags, relations,
facets and scenes, and its payloads are engram truth. A `LeasedEpochRecord`
variant on it would put key material into graph history, contradicting ruling 1
in the one lane that is append-only by contract. Ruling 1 is not a preference
about where records live; it is the reason carriage may be destructive at all.

So carriage rides a **sibling topic on the same transport**, with its own
payload grammar and slot semantics. `TopicStore` and `Topic` are already
imported there. What remains is a topic and a grammar, not a transport:

- A `LeasedEpochRecord` wrapper over `WrappedEpochRecord`: `issued_at_ms`,
  `expires_at_ms`, `issue: u64` (monotonic per certificate), issuer signature
  over the encoded record. A wrapper rather than new fields, so the local
  wallet file is untouched and an un-leased record is unrepresentable in the
  carriage lane by construction.
- Install path: verify signature, check monotonicity against the held
  version, assert the grant-expiry ceiling, destructive-replace.
- Read path: refuse expired (`now >= expires_at_ms` refuses, matching the
  station grant check).
- Purge as propose then execute: a pure pass returning retained leases with a
  reason each, expired leases as candidates, and `blockers` when it refuses;
  execution separate, on the steady-heat schedule Athanor already runs for
  tombstone age-out.

## Done conditions

- A device that lost its record recovers from a peer replica while the lease
  is live, without re-pairing.
- A superseded version presented to a holder of a newer one is refused.
- A revoked certificate's leases vanish from a cooperative peer at expiry
  with zero messages delivered, and immediately when a revocation statement
  arrives.
- No lease exists whose expiry exceeds its grant's, its device's configured
  TTL, or the absolute ceiling.
- A device with no carriage policy set replicates nothing, and that is the
  state a freshly commissioned device is in.
- No carriage record is representable as a `PersonalGraphEvent`, so the two
  grammars cannot be crossed by accident.

## What this does not decide

- ~~The carriage lane's payload grammar.~~ **Decided 2026-08-18** in the
  [lane grammar](2026-08-18_epoch_carriage_lane_grammar.md): a sibling topic
  derived from the graph topic, an extension carrying everything checkable
  without the body, a blinded slot id in place of the certificate id, and
  admission ordered fail-closed onto the protocol's own prune.
- ~~Whether group-member devices need carriage at all.~~ **Settled
  2026-08-18**: they do. See the key-group section; the two mechanisms deliver
  different key material and neither substitutes for the other.
- **The trusted-peer roster.** Who the peers are and how membership is
  governed; shared with the participant gate's admission machinery.
- **Mesh-wide revocation flooding.** Sited brief question 3 stays open; this
  design converts it from a precondition into a latency optimization.
