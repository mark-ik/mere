# Epoch carriage against stickleback's epoch retention

**Review, 2026-08-16, at Mark's request.** Checks the
[leased slots proposal](../technical_architecture/2026-08-14_epoch_carriage_retention_leases.md)
against machinery that already exists, before it is ratified. Reads
`stickleback::epoch_retention`, `stickleback::group_crypto`,
`graphshell::personal_sync`, `graphshell::native::graph_keys`,
`pandect::wallet_grant`.

The proposal says implementation is "gated on a transport existing" and calls
the transport "murm/moot shaped and undesigned". Both halves turned out to be
wrong, and a third thing turned out to be wrong underneath them.

## 1. The transport exists

`graphshell::personal_sync` is **"H7 personal-device synchronization"**:
device-to-device sync across one person's own devices, which is exactly the
trusted-peer set carriage wants. It runs (`h7_sync_peer`), it is built on
stickleback and p2panda with causal ordering and policy-before-insert storage,
peers authenticate through `personae::DerivedKeyAttestation`, and it already
enforces `PrivacyClass`: `AppendAccess` refuses `LocalOnly | MootScoped`.

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
persona certificate without being in a graph key group. But for any device that
*is* a member, the group session is already a better channel than a replicated
slot, and it comes with the retention engine in §3 for free.

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

Live, not shelved: consumed by `moot::commons::chat` and by
`graphshell::native::graph_keys` — that is, by the personal-device lane itself.

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

Ratifiable, with four amendments:

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

## What this review does not settle

Whether carriage for a group-member device should be retired entirely in favour
of the group session. That is a real simplification and it needs the membership
and pairing-key lifecycles compared properly, which is more than this review
read.
