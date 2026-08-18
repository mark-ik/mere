# Epoch carriage against stickleback's epoch retention

**Review, 2026-08-16, at Mark's request.** Checks the
[leased slots proposal](../technical_architecture/2026-08-14_epoch_carriage_retention_leases.md)
against machinery that already exists. Reads `stickleback::epoch_retention`,
`stickleback::group_crypto`, `graphshell::personal_sync`,
`graphshell::native::graph_keys`, `knot::sync`, `moot::commons::chat`,
`pandect::wallet_grant`.

**Framing corrected 2026-08-16.** This first said "before it is ratified". The
proposal was ratified 2026-08-14 (`8633bf5c`); this is a check after the fact,
so its findings are amendments to a ratified design rather than conditions on
ratifying it. **All four were folded in on 2026-08-16**, along with the
consumer correction in section 3 below.

The proposal says implementation is "gated on a transport existing" and calls
the transport "murm/moot shaped and undesigned". Both halves turned out to be
wrong, and a third thing turned out to be wrong underneath them.

## 1. The transport exists

`graphshell::personal_sync` is **"H7 personal-device synchronization"**:
device-to-device sync across one person's own devices, which is exactly the
trusted-peer set carriage wants. It is built on stickleback and p2panda with
causal ordering and policy-before-insert storage, peers authenticate through
`personae::DerivedKeyAttestation`, and it already enforces `PrivacyClass`:
`AppendAccess` refuses `LocalOnly | MootScoped`. `src/bin/h7_sync_peer.rs` is a
411-line serve/connect binary the tree calls a "Physical H7 offline-edit and
convergence receipt".

What does not exist is a *lane* for carriage. `PersonalGraphEvent` is a graph
grammar (nodes, tags, relations, facets, scenes), and the proposal's own ruling
1 says carriage is keyring state that never enters the engram lane. Adding a
`LeasedEpochRecord` variant would put key material into graph truth and
contradict the ruling the doc opens with.

So the gate is not "design a transport". It is "add a sibling topic with its
own payload grammar, and say why the separation is load-bearing".
`TopicStore` and `Topic` are already imported there.

## 2. A per-device key group already exists on that lane

`graphshell::native::graph_keys` is **"this device's membership in one personal
graph's key group"**: pre-key publication, an explicit create-then-add
ceremony, `GroupSession` state held through Personae's `SealedRecordStorage`.

So the personal-device lane already distributes key material to a person's own
devices, through a live authenticated group session. Carriage proposes to
distribute key material to a person's own devices through pairing-key-wrapped
slots. Those are two mechanisms for one shape, and the proposal does not
mention the first.

They are not simply redundant, and the dividing line is real: the group session
requires membership, and membership is an explicit act. A device can hold a
persona certificate without being in a graph key group.

**Corrected 2026-08-18.** This finding then said that "for any device that *is*
a member, the group session is already a better channel than a replicated
slot". That is wrong, and it was the load-bearing half. The two mechanisms
carry different key material: the group session distributes one graph's
`DataKeyring`, which seals `PersonalGraphEvent` bodies on the lane, while
carriage distributes one persona's private epoch secret, which
`WalletEpochSealer` turns into the key sealing that persona's eidetic payloads
at rest. Different granularity, different thing sealed, no shared root, and
neither module references the other. A seated device can read the lane and
still not open the persona's store, so the group session is not a better
channel for this, it is a channel for something else.

What survives is the weaker and still useful observation: both deliver key
material to one person's devices over one transport, so they should share a
lane discipline and a retention idiom, and the proposal should say so.

## 3. stickleback already has an epoch-retention engine, and it is live

`stickleback::epoch_retention` is 331 lines of exactly this problem:

- `GroupEncryptionProfile { mode, retained_data_epochs }`: a product-policy
  floor on how many epochs survive a prune.
- `EpochHoldReason`: `DecryptionReachability`, `AuthorityReevaluation`,
  `PendingCausality`, and **`OfflineMember`** — "the governed offline-member
  policy still promises recovery to this member".
- `propose_epoch_pruning`: a **pure, authority-neutral dry run** returning
  `retain` (with a reason per epoch), `forget`, and `blockers`. `forget` is
  populated only when every gate passes; execution and authorization stay
  domain-owned.
- `DataKeyring::epochs_oldest_first`, guarded so that a keyring whose
  chronology is unknown returns `None`, "because lexical secret-id order is not
  a safe substitute for chronology".

Live, not shelved: `moot::commons::chat` and `knot::sync` both run it in
production, each pairing `propose_epoch_pruning` with an `execute_*` that
"revalidate[s] and explicitly execute[s] a reviewed proposal". That pair is the
shape section 3's recommendation borrows.

**Corrected 2026-08-16.** This first read "consumed by `moot::commons::chat`
and by `graphshell::native::graph_keys`, that is, by the personal-device lane
itself", and missed `knot::sync` entirely. The graphshell half is wrong in a
way that matters, because it was doing work in the argument: graphshell's only
use is a `#[cfg(test)] mod retention_probe`, and what that probe found is that
pruning is **blocked**, on `EpochProposalBlocker::MissingCheckpoint`, which it
calls "the correct answer rather than a limitation to work around". The
personal-device lane did not adopt the engine. It measured it and declined.

That correction sharpens finding 3 rather than weakening it. The probe also
measured the pressure: 103 bytes per epoch, one epoch per revocation, so "a
mesh that retired a device every week for a decade would spend 54 KB". Group
retention is a shape worth naming and not a pressure worth acting on, which is
the opposite of carriage's motive.

### And carriage could not use the engine even if it wanted to

`propose_epoch_pruning` blocks on `MissingCheckpoint` unless the domain supplies
an `EpochCheckpointBasis` proving it can rebuild its projection without the
epochs it would forget. The gate exists because forgetting a group epoch makes
every operation sealed under it permanently unreadable. Carriage loses nothing
that way, because the local wallet is the authority and holds current material
by construction, so it has no checkpoint to supply and needs none. Handed the
engine, carriage would be blocked forever, exactly as graphshell is. The
borrowing in this section is therefore of shape, not of the function.

### It does not supersede the lease design, and the reason is principled

The two retention models pull **opposite directions on the same fact**.
`EpochHoldReason::OfflineMember` says an unreachable member is a reason to
**keep** an epoch, because losing it loses readability. A carriage lease says an
unreachable holder is a reason to **drop**, because keeping it is harvest
exposure.

Both are correct in their domain, and the difference follows from what the
material is for. Group epochs decrypt durable shared documents, so premature
forgetting destroys data. Carriage epochs are key delivery, so late forgetting
is the whole risk.

The delivery assumptions differ in the same way. Count-based retention needs a
prune authorization to arrive; the lease design deliberately refuses that
dependency ("revocation delivery is an optimization, not a dependency"), because
its threat model assumes unreachable peers. Time-based expiry converges with
zero messages. **The lease design's time-based choice is right, and this review
confirms it rather than replacing it.**

### Three things worth borrowing

1. **The proposal/execution split.** `propose_epoch_pruning` is pure, returns a
   reason for every retained epoch, and surfaces `blockers` so a blocked prune
   is legible rather than silent. The leases proposal's "scheduled purge" is a
   side-effecting pass with no dry-run artifact. Borrowing this shape would make
   carriage purges reviewable and testable without a runtime, which is also what
   athanor does (the proposal cites athanor's steady-heat scheduling but not its
   propose/apply separation).
2. **Fail closed on incomplete ordering.** `IncompleteEpochOrder` blocks the
   entire proposal rather than pruning on partial information. Carriage should
   refuse in the same situation.
3. **The chronology guard, as corroboration.** `epoch_order_version` exists
   because lexical id order is not chronology. The proposal's monotonic issue
   counter is the same insight reached independently; worth citing so the
   agreement is visible rather than looking invented.

What not to borrow: `retained_data_epochs`, count-based retention. It needs the
count communicated or an authorization delivered, which is the dependency the
lease design correctly refuses.

## 4. The sited radio has no carriage to lease

This is the finding that changes the proposal most, and it invalidates its
central TTL example.

Carriage only ever exists for a **persona** certificate. Verified along the
whole chain rather than assumed:

- `store_wrapped_epochs` iterates `set.personas` only, so no device-scoped
  certificate ever gets a record.
- `requires_epoch_material` keys on `ACTION_PRIVATE_READ`.
- `is_persona_scoped_action` puts `private.read` on the persona side of the
  partition, so it can only ever land on a persona certificate.
- Castellan's station policy refuses a set containing persona certificates
  outright (`SitedStationGrantError::PersonaAuthority`).

A transport-only sited radio therefore holds **no wrapped-epoch record at all**.
The proposal's TTL sizing is motivated by "a carriage lease serving a
LoRa-reachable radio is long", and that device cannot have one.

**The 2026-08-14 per-device amendment inherited the same error.** Its argument
for per-device TTLs used the radio as the exemplar. The conclusion still holds
on its own terms, because the wrapping key is per-device and so per-device is
the honest unit for an exposure budget, but the worked example was of a case
that does not arise and should be replaced.

What follows for TTL sizing: the devices that hold carriage are the ones that
act for a persona, which are broadly the ones that could be graph key-group
members. The "worst link" framing was reaching for constrained radios that are
out of scope. Sizing should be argued from persona-holding devices, where the
recovery benefit is real and the links are ordinary.

## Recommendation

Sound, with four amendments. **All four folded in 2026-08-16**, into the
proposal's own sections rather than as an appendix, so the doc argues correctly
on its own:

1. Replace "gated on a transport existing" with "gated on a carriage lane
   beside H7, kept out of the graph grammar", and state why the separation is
   load-bearing.
2. Address the graph key group: say when carriage should ride an existing group
   session instead of a leased slot, and confine leased slots to devices that
   hold persona authority without group membership.
3. Cite `stickleback::epoch_retention`, state the opposite-direction reason the
   idiom diverges, and adopt the propose/execute split and the fail-closed
   ordering gate.
4. Replace the sited-radio TTL example, and correct the 2026-08-14 amendment
   that repeated it.

**One was narrowed in the folding.** Amendment 2 as written above also said to
"confine leased slots to devices that hold persona authority without group
membership". That confinement is the same ruling this review says it does not
settle, one section below, so folding it in as written would have decided by
side effect what the review declined to decide on evidence. The folded section
states the boundary and the overlap, names the group session as the better
channel where a seat exists, and records the retirement question as open.

## What this review does not settle

~~Whether carriage for a group-member device should be retired entirely in
favour of the group session.~~ **Settled 2026-08-18, and not by the comparison
this section called for.** The membership and pairing-key lifecycles never
needed comparing: the question dissolves one level down, at what each key
seals. They seal different things, so retirement was never available. The
finding is written up in the proposal's key-group section, and finding 2 above
is corrected accordingly.

That this review posed the question as a lifecycle comparison is itself the
lesson. It had already read both modules and still framed two unrelated key
materials as competing channels, because both are "key material for a person's
own devices over one transport". The distinguishing fact was one level below
the one the review stopped at.
