# Gaz Founding Plan

**Date**: 2026-08-08
**Status**: M0 landed. M1 (persistence) next.
**Scope**: the contact layer, standalone. The record model, the persona-scoped
book, then storage over muniment, then the adapters that turn resolver output
into records, then mere reconciliation.

**Related**:

- Mere's [contact and remote-identity model brief](https://github.com/merely-made/mere)
  (`design_docs/mere_docs/research/2026-06-15_contact_identity_model_brief.md`)
  is the founding spec. It settles the *them* side of identity; this crate
  implements it.
- [`personae`](https://crates.io/crates/personae) owns the *me* side.
- [`muniment`](https://crates.io/crates/muniment) is the persistence seam M1
  rides.
- Mere's `ports/gazette` is the resolution sibling.

---

## 1. Why a crate and not a module

The record model is portable, has no mere dependency once it stops importing
`comms::Identity`, and has at least three prospective consumers: mere itself,
turnstone, and the radio companion. It also wants to be auditable on its own,
because it is the file that holds who a person knows. That clears the
crate bar. Publication is already done: `gaz` 0.0.1 was a reservation, and the
next publish is the first real one at 0.1.0.

## 2. Decisions

The brief's §9 left five shapes open. M0 settles four; the fifth is still
Mark's.

**Contacts are anchored, not merely key-rooted.** The brief says a contact is
rooted on a stable key, but a key that can rotate cannot also be the map key
without re-filing the record on every rotation. So a record is filed under its
**anchor**, the first key ever seen, and `current_key()` is the newest. The key
list only grows. Consequences worth having: a rotation never moves a record, a
message signed with a retired key still resolves to its owner, and the
key-rooted rule becomes enforceable rather than aspirational.

**The key-rooted invariant is enforced on both sides.** `Contact` cannot be
constructed without a key, the key list is private and append-only, and
deserialization rejects an empty list by name. State that has lost its root
fails to load instead of arriving as a contact rooted on nothing.

**Trust is one vocabulary, used per endpoint and per handle binding.**
`TrustState` is `Unverified | Pinned | Verified | Mismatched | Revoked`. The
brief named TOFU, signature, DID self-auth, and back-claim proof; those are
`ProofMethod` variants rather than separate states. `Mismatched` earns its
place because it is TOFU's whole point: a pin that stopped matching is either a
rotation nobody announced or someone in the middle, and gaz must not silently
choose the friendlier reading. `is_alarming()` marks the two states that have
to reach a person.

**Kith and kin are user-set, not trust-derived.** The brief asked. Deriving the
tier from verification would fold two axes into one and mean a peer becomes
close because their certificate checked out. Trust is a property of an address;
kith and kin describe a relationship. They stay separate.

**Still open, and Mark's call:** raw keys versus DIDs for the stable middle.
M0 takes raw 32-byte keys as the root and treats `did:` strings as handles,
which follows from "the pubkey IS the identity" and keeps the crate free of
resolution logic. If DIDs should be roots in their own right, the anchor type
becomes an enum and this is the cheapest moment to change it.

## 3. Shape

Six modules, all serde-only, no cryptography, no clock, no storage:

| Module | Holds |
|---|---|
| `key` | `ContactKey`, 32 bytes, hex in text formats and raw bytes in binary ones |
| `trust` | `TrustState`, `ProofMethod` |
| `handle` | `Handle`, `HandleKind`, normalization and matching |
| `endpoint` | `Endpoint`, `EndpointKind` |
| `contact` | `Contact`, `ContactTier`, the anchor rules |
| `book` | `ContactBook`, `PersonaScope`, `ScopeMismatch`, the queries |

Two dependency decisions, both deliberate:

- **No `personae` dependency.** gaz stores keys and compares them; it never
  verifies a signature. Depending on personae would pull ed25519-dalek, blake3,
  Argon2 and ChaCha20 into a crate that needs none of it. mere converts at the
  boundary, `Ed25519PublicKey::to_bytes()` to `ContactKey::from_bytes()`.
- **Not mere's `comms::ProtocolKind`.** That enum has two variants, Misfin and
  Murm, and is owned by the app. A contact book has to hold gemini capsules and
  ActivityPub actors it cannot message, so `EndpointKind` is its own open enum
  matching what the gazetteer already classifies WebFinger links into.

**Persona scoping is by construction**: one book per persona, and the book
carries its own scope label so a mis-filed load fails loudly through
`verify_scope` rather than quietly showing the work persona's people to a
burner. The label is an opaque string, so gaz never learns what a `PersonaId`
is.

**No clock.** Every timestamp is a caller-supplied `now_ms`. Deterministic
under test, fine on wasm, and it keeps the recency rules honest: every update
is monotonic, so a replayed or late event cannot rewind a record.

## 4. Phases

- **M0 — the model.** Done. Six modules, 41 tests plus a doctest, clippy clean
  under `-D warnings`, every file well under the 600-line ceiling.
- **M1 — persistence.** A `muniment` feature wiring `ContactBook` through
  `SlotStore` at `personas/<persona_id>/contacts`, with `verify_scope` on the
  load path. Core stays serde-only and default-featureless.
- **M2 — resolver intake.** Turn gazetteer output into `Endpoint`s with
  `TrustState::Unverified`, and give `Handle` bindings a way to be proven by
  back-claim. This is the seam where resolution meets storage, and it belongs
  here rather than in the resolver.
- **M3 — mere reconciliation.** mere consumes gaz as a git dep and grows the
  conversion glue for `personae` keys and `comms::Identity` endpoints. The
  `Contact` rollup the comms model lacks lands at this point.
- **M4 — recency and merge.** Duplicate detection when two records turn out to
  be one person, which needs a merge rule for conflicting petnames and tiers.

## 5. Findings

**The slot really was empty.** No `struct Contact` existed anywhere in mere
before this. The brief specified the record in June and nothing was built, so
M0 is the first implementation rather than a port.

**JSON forced the key encoding, and improved it.** A `BTreeMap<ContactKey, _>`
cannot serialize through JSON while the key is a byte array, because JSON map
keys must be strings. The round-trip test caught it. The fix, hex in
human-readable formats and raw bytes in binary ones, also makes a stored book
readable by a person, which is worth having for this particular file. Postcard
pays nothing for it.

**The brief's proposed struct could not be used verbatim.** Its
`endpoints: Vec<(Identity, TrustState)>` names mere's `comms::Identity`, which
would have made gaz mere-coupled and capped it at two protocols. Same intent,
independent type.

## 6. Progress

**2026-08-08.** Founded. Reservation stub replaced by the M0 model. `cargo test`
41 passed plus 1 doctest; `cargo clippy --all-targets -- -D warnings` clean.
File sizes: book 294, contact 271, key 204, trust 122, endpoint 100, handle 100,
lib 76.

Next session starts at M1, and should confirm the DID question in §2 before
M3 makes the anchor type expensive to change.
