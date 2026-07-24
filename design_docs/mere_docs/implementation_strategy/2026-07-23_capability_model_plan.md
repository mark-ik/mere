# Capability Model Plan

Servitor's authority layer, from stringly prefixes to a typed capability with
a partial order, plus the three things that order makes possible: attenuation,
expiry, and revocation.

Successor round to the [participant gate + packs
plan](2026-07-17_participant_gate_packs_plan.md), whose build order B0-B5 is
complete. That plan's open question 3 (revocation design, "own round before
B5") was never taken and B5 landed without it; this is that round, widened by
what the B3 implementation pass exposed.

Servitor still lives at `repos/servitor`; it becomes a mere workspace member
at phase C3c of the [repo consolidation
plan](2026-07-23_repo_consolidation_plan.md). This work lands in the standalone
repo and travels with that move.

## The decision in one line

A capability is a **type with a partial order**, not a string with a prefix
test; that one order answers coverage, attenuation, and delegation alike, and
revocation is what happens when a link in a delegation chain dies.

## Findings

### F1. Coverage is string-prefix-shaped (verified 2026-07-23)

`Grant::covers` is `path.starts_with(&self.path_prefix)`. Probed directly:

| grant | queried path | covers? |
| --- | --- | --- |
| `app/nav` | `app/navigate` | **true** |
| `app/session` | `app/session-admin` | **true** |
| `scenario/` | `scenario/../app/session` | **true** |

No live bug: merecat queries only the five fixed ring paths
(`app/read|navigate|panes|dispatch|session`) and grants exactly those, and none
is a prefix of another. The hazard is in the shape, not today's data. **Adding
a capability path that extends an existing one silently widens every grant
already issued**, including durable grants replayed from nested worlds on
adopt. That is the authority-drift class the ring ruling exists to prevent,
sitting one `starts_with` deep.

The same test appears twice more: the gate's scope check
(`node.starts_with(claimed_path)`, gate.rs) and the projection guard
(`node.starts_with(GRANT_PREFIX)`). Three call sites sharing one unwritten
convention is the argument for a type that owns the relation.

### F2. One mechanism serves two different kinds of capability

This is the root, and the reason a better matcher is not the fix.

- **App rings** are a *closed set*: navigate, panes, dispatch, session, read.
  There is no such thing as "everything under navigate". Prefix openness buys
  nothing and costs F1 exactly.
- **Graph scopes** (`trail/`, `scenario/`) are an *unbounded hierarchy* where
  prefix scoping is the entire point, and meadowcap-shaped path scope is right.

Servitor models both as `path_prefix: String`, so the closed set inherits the
open set's failure mode. Split them and the ring hazard disappears by
construction rather than by careful naming.

### F3. The grant projection is lossy (verified 2026-07-23)

`Gate::project_grant` writes the durable record as a node id
`grant:<path_prefix>` and puts the **mode in the node's display title**, which
nothing parses. On adopt, merecat reconstructs authority as:

```rust
Grant::new(subject, path, Mode::Write)   // denizen.rs, rebuild()
```

The mode is hardcoded. No live bug (merecat only ever grants `Write`), but a
`Read` grant would come back as `Write` after a restart, and more to the point
**every richer field would be dropped on replay the same way**. An expiry that
does not survive adopt is not an expiry, so this blocks the revocation work
rather than sitting beside it.

### F5. personae ALREADY owns signed delegation (found 2026-07-24, the hard way)

`personae::delegation` is a mature (~750 LOC) **signed** capability delegation
grammar, and it predates this round:

- `DelegationCertificate` / `SignedDelegationCertificate` (Ed25519, via
  master-attested derived signing keys), content-addressed `id()`;
- `DelegationParent::{Root([u8;32]), Certificate(DelegationId)}` — parent
  chains;
- `CapabilityScope { domain, resource, path_prefix, actions }` with
  `attenuates()` (path containment + action subset) and
  `covers(path, action, at_ms)`;
- `not_before_ms` / `expires_at_ms`, `remaining_delegation_depth`, `nonce`;
- `DelegationRevocation`;
- and an `expiry_within(child, parent)` **byte-identical** to the one written
  for C3 before this was found.

**gemot already consumes it**: a 1,378-LOC subsystem
(`moot/delegation/{store,sync,wire}.rs`) with a muniment-backed
`MootDelegationStore`. So the moot tier is already built on this grammar.

Its `path_covers` requires a `/` boundary (`path == prefix || suffix starts
with '/'`), so personae **already avoids the F1 hazard** for paths by a
different route than F2's segment vectors. What it does not have is the
power/scope type distinction.

**This invalidates the C3 built above it.** C3 shipped a parallel unsigned
delegation algebra — parent chains, attenuation, expiry, revocation — that
duplicates personae's. It was written without grepping for an existing
implementation, against this repo's own standing rule (check existing crates
first; extend, don't duplicate). The correction is F5's ruling below.

### F4. `Mode::Delegate` is inert because the order does not exist

`Delegate` is defined, ordered above `Write`, and nothing delegates. It cannot:
delegation requires a well-defined "B is a narrowing of A", which is *the same
relation* coverage needs. The dead variant is a symptom of F2, not a missing
feature.

## Design

### D1. The capability type

```rust
pub trait Capability: Clone + Debug + Eq {
    /// Whether holding `self` satisfies a need for `needed`.
    fn covers(&self, needed: &Self) -> bool;
}
```

Laws, enforced by a reusable test harness rather than by prose: **reflexive**
(`a.covers(a)`) and **transitive** (`a.covers(b) && b.covers(c)` implies
`a.covers(c)`). Any implementation that breaks either is a hole in the gate.

Servitor ships one concrete sum, because the real consumers hold both kinds at
once (merecat grants app rings *and* a `scenario/` scope) and generics would
infect `Gate`'s signature and every call site for no gain:

```rust
pub enum Cap {
    /// A named power from a closed set. Coverage is EQUALITY, so a new power
    /// can never be covered by an old grant.
    Power(String),
    /// A hierarchical scope. Coverage is SEGMENT-prefix.
    Scope(ScopePath),
}
```

`ScopePath` parses once at construction into validated segments and **rejects
`..`, empty segments, and absolute forms**, so traversal cannot be expressed,
let alone matched. Segment comparison makes `app/nav` fail to cover
`app/navigate` (`["app","nav"]` is not a prefix of `["app","navigate"]`).

A `Power` never covers another `Power`, so growth is safe by construction.
Cross-variant coverage is always false.

### D2. Wire form

Strings survive only at boundaries (projection node ids, gemot's opaque
`capability_path`, pack manifests), parsed at the edge and never compared
inside a decision:

```text
power:navigate
scope:trail/step
```

Legacy read: an unprefixed string parses as `Scope`, which is what it meant
before this round. Merecat maps its four known `app/<ring>` paths to
`Power(<ring>)` on adopt, so existing installs keep working without a
re-install.

### D3. Delegation is what install already does

The user is currently an implicit, infinite authority and install "writes some
grants". Model the user as a root subject and the picture collapses into one
mechanism:

- the **visible review is an attenuating delegation** from user to denizen;
- **expiry** is a delegation with a bound;
- **revocation** is severing a delegation.

```rust
pub struct Delegation {
    pub id: DelegationId,
    pub parent: Option<DelegationId>,   // None = a root delegation
    pub from: Subject,
    pub to: Subject,
    pub cap: Cap,
    pub mode: Mode,
    pub expires_at_ms: Option<u64>,
}
```

Chain invariants, checked at issue **and** at verify (never trust a stored
chain):

1. **Attenuation**: `parent.cap.covers(child.cap)`.
2. **Mode**: `child.mode <= parent.mode`, and the delegator holds
   `Mode::Delegate` on that cap.
3. **Expiry**: `child.expires_at <= parent.expires_at`. A child cannot outlive
   its parent.

### D3b. One delegation system: servitor adapts to personae (RULED, Mark, 2026-07-24)

Given F5, the layering choice was put to Mark as three options (servitor adapts
to personae / two tiers with a promotion seam / personae adopts the typed Cap).
**Ruled: option 1 — servitor adapts.**

personae owns the delegation machinery; servitor contributes the one thing
personae lacks, the **typed capability**, as a view over personae's
two-dimensional `(path_prefix, actions)`:

| servitor | personae `path_prefix` | personae `actions` |
| --- | --- | --- |
| `Cap::Power("navigate")` at `Write` | `power/navigate` | `{read, write}` |
| `Cap::Scope("scenario/a")` at `Read` | `scope/scenario/a` | `{read}` |

Both halves of the order survive the encoding, which is why this is a view and
not a lossy mapping:

- **powers stay closed** — personae's slash-boundary match means `power/nav`
  does not cover `power/navigate`, and nothing sits beneath a power;
- **scopes stay hierarchical** — `scope/scenario` covers `scope/scenario/a`;
- **modes stay ordered** — a `Write` grant carries `{read, write}`, so subset
  attenuation reproduces `Write` covering `Read` without personae knowing the
  ordering.

What servitor therefore does NOT own any more: signatures, chain walking
semantics, attenuation rules, expiry containment, delegation depth,
revocation records. It keeps: `Cap`, the encoding, a rooted table that
verifies chains through personae, and the `AuthorityProvider` seam.

This also makes OQ3 (moot unification) nearly free rather than a project:
gemot already speaks these certificates, so denizen and moot authority are one
system with two consumers.

### D4. Revocation cascades, lazily

**Proposed ruling.** Validity is "an unbroken chain of valid links to a root",
evaluated on read. Severing a link therefore invalidates every descendant
immediately, by construction, with no marking pass and no partial state to get
stuck in.

The alternative (orphans stay valid until their own expiry) means revoking a
compromised pack leaves its sub-denizens running, which is precisely the case
revocation exists for. Lazy evaluation costs a walk per check; at this scale
that is free, and the walk result caches per rebuild.

### D5. Clock

Servitor must stay clock-free (portable, wasm-safe, deterministic in tests),
so it never calls `SystemTime`. The concrete authority holds a host-set
`now_ms` (`set_now`) that `covers` reads, leaving `AuthorityProvider`'s
signature untouched so gemot's mirror-shaped seam is unaffected. Expired means
not covered.

Known window: a grant expiring mid-session is not noticed until the next
`set_now`. Merecat sets it at every denizen run, which is the only moment
authority is consulted, so the window is not observable there. Any future
consumer that holds authority across long idles must tick it.

### D6. Lossless projection

The projection node carries the whole record in **explicit key:value tags**,
with `media_type` naming the record schema so a reader knows the parse.
`Container` has no arbitrary payload field, and the alternatives are worse: the
title is a display string (F3 is what that abuse costs), and content-addressing
the record as a muniment blob would make `rebuild` require a blob store to
answer "what may this denizen do".

```text
id:         grant:power:navigate
media_type: application/vnd.mere.grant+json
tags:       ["grant-projection", "cap:power:navigate", "mode:write",
             "expires:none", "delegation:<id>", "parent:<id>"]
```

Every field round-trips; nothing is reconstructed by guess. The record stays
browsable from the graph, which is the property that made projections the
authority record in the first place.

A denizen's own outgoing delegations are a *different* record kind
(`delegation:<id>`), so "what I hold" and "what I gave away" never share a
namespace.

## Build order (targets, not durations)

- **C0, the capability type** (servitor). `Capability` trait with law tests,
  `Cap::{Power, Scope}`, `ScopePath` parse + validation, wire form and legacy
  read. `Grant.path_prefix: String` becomes `Grant.cap: Cap`. All three
  `starts_with` sites route through the type. **Done when** the F1 table
  inverts (every row false except a genuine segment-prefix), the law tests
  pass, and servitor's 9 existing tests still pass.

- **C1, lossless projection** (servitor + merecat). D6's tag encoding, a
  reader that parses it back, `rebuild` consuming the reader instead of
  hardcoding `Mode::Write`. **Done when** a `Read` grant projects, replays, and
  is still `Read`, and a round-trip test covers every field.

- **C2, expiry** (servitor). `expires_at_ms`, `set_now`, expiry as
  non-coverage. **Done when** an expired grant stops covering without any
  mutation of the store, and the effective expiry of a chain is its minimum.

- **C3, delegation** (servitor, ADDITIVE). `Delegation`, `DelegationId`,
  `DelegationTable` implementing `AuthorityProvider` by chain validity (D3's
  three invariants), `sever` with D4's lazy cascade, and the delegation
  projection round-trip. Kept additive: `GrantTable` stays the working
  authority so merecat does not churn here; the swap is C4's. **Done when** a
  chain verifies from a cold store, an attempt to delegate wider than held is
  refused, `sever` invalidates a whole subtree, and delegation records
  round-trip through the projection.

- **C4, revocation + the merecat swap** (merecat). merecat moves from
  `GrantTable` to `DelegationTable`; the root subject comes from the active
  personae identity (OQ2); install issues attenuating root delegations instead
  of flat grants; re-review replaces-and-cascades (OQ1); the Uninstall row
  beside Run severs. **Done when** severing stops a denizen's whole subtree,
  receipted headed: install, run, uninstall, run-refused; and a pre-round
  session still heals.

- **C5 RETIRED.** The migration it named was forced into C0 (see Progress);
  C4 carries the remaining swap. No separate phase.

## Open questions

1. **RULED (Mark, 2026-07-24): replace and cascade.** A re-review that asks for
   more replaces the denizen's old root delegations rather than sitting beside
   them: the old set is severed (cascading to any onward delegations made under
   it) and the new set issued. The old chain described a capability set the user
   has now explicitly reconsidered, so keeping it live would be authority the
   user thinks they revoked.
2. **RULED (Mark, 2026-07-24): the root is the personae identity**, not a
   placeholder constant. Servitor stays identity-agnostic — it takes
   [`Subject`]s and never depends on personae — so "root is personae" is a HOST
   fact: merecat derives the root subject from the active personae identity's
   public key and hands it to the delegation table. (Mark: the SSH-key vault in
   personae is nearly functional, so the identity this roots on is real, not
   hypothetical.) Servitor's delegation algebra is the same whether the root is
   a test key or a personae key; only merecat knows which.
3. **RULED (Mark, 2026-07-24): unify — on the trait, when the caller exists.**
   gemot's `MootAuthorizationProvider` adopts this order. No fundamental
   drawback; the cost is a dependency edge, which decides the shape:
   - Unify on the **trait** ([`Capability`]), not the enum. `cap.rs` is already
     chartulary-free, so the algebra (`Capability`, `Cap`, `ScopePath`, `Mode`)
     extracts to a leaf crate both `gemot` and `servitor` depend on, and each
     keeps its own concrete capability if they ever diverge.
   - `Cap` fills the **L1 structural slot** of the three-layer stack; gemot's
     `TesseraFacts` return stays the L2 policy layer, untouched. Unifying does
     not collapse the layers.
   - The endgame meadowcap scope (graph-cluster namespaces, leaf-node-id
     binding) arrives as a new `Capability` impl for BOTH sides at once — a
     feature of unifying, not a cost.
   - **Sequencing**: there is no "moot refactor" — moot/murm stay inside mere
     (the consolidation plan withdrew their promotion). The real gate is that
     gemot's provider **has no caller today**: grep finds zero external
     invocations of `MootAuthorizationProvider`, and `DenizenKind::Peer` is an
     enum variant with no code path. So unifying now is a provider nothing
     calls. It lands when a **peer first petitions through the gate** — unbuilt
     product surface — and the leaf-crate extraction happens then.

## Follow-ons found in the closing audit (2026-07-24)

The round is complete and the workspace checks clean; these are named so they
are not lost, ordered by how much they matter.

1. **FIXED 2026-07-24 (merecat).** ~~The Graphshell projection endpoint
   self-issues its authority.~~ The endpoint now derives a **per-session
   keypair** from the profile identity (personae `derive_keypair`, salt
   `merecat/projection-endpoint/<session>`) and holds a **signed delegation**
   from the user's master key for `scope:projection/layout`, depth 0. Two
   things were wrong and both are gone: the subject was `blake3(session
   name)` — not a key, so nothing could ever prove it was this endpoint — and
   the endpoint granted itself the capability it then checked. The G3 golden
   receipt is unchanged byte-for-byte (the refusal reads the same), but it now
   MEANS something: a test adopts the endpoint's own certificates under a
   different root and they authorize nothing, which a self-issued grant could
   not fail. `GrantTable`'s remaining users are test doubles, which is the
   honest role for the unsigned provider.
   *Original finding:* the endpoint derived a subject from `blake3(session
   name)`, projected a grant to itself, and rebuilt a `GrantTable` from that
   projection — harmless for a loopback receipt, wrong the moment a remote
   peer projects.

   <details><summary>superseded wording</summary>
   `merecat::remote_projection` derives a subject from `blake3(session name)`,
   projects a grant *to itself*, and rebuilds a `GrantTable` from that
   projection. It is its own root. Harmless today — a loopback receipt whose
   "rejected" line is an audit trail, not a trust boundary — but it is exactly
   the shape this round removed everywhere else.</details>

2. **Sub-delegation: the consumer is named, and it is the endpoint above.**
   F4 called `Mode::Delegate` inert for want of an order; the order exists now,
   and personae carries `remaining_delegation_depth` — but every issuer in the
   product uses depth 0, so nothing delegates onward. Asked for a better
   consumer than "an installed helper", the answer is the one the fix just
   built: **the projection endpoint delegating to a remote viewer.** It is the
   only edge in the system where a second identity should hold *strictly less*
   than the delegator for a *specific* reason — the endpoint may present this
   session's layout; a connected viewer should hold read-only presentation of
   one scene, not the endpoint's whole capability. That makes the endpoint's
   certificate depth 1 and the viewer's 0, a one-line change at the point the
   viewer gains its own key.
   Rejected alternatives, both speculative (no code either side): a pack
   installing sub-packs, and an agent spawning sub-agents. A third is real but
   belongs elsewhere — a moot peer re-delegating to their own second device is
   the kith device tier's business, not this round's.
   **Gate:** lands with the Graphshell remote lane. Building it before a viewer
   has a key would be capability written against an imaginary consumer, which
   is how the self-issued grant happened in the first place.

3. **The re-root heal's safety is load-bearing on the projection guard.**
   `denizen::rebuild` re-issues authority from the grant projections in a
   denizen's own world. A denizen cannot forge one *because* `Gate::petition`
   refuses any spec touching the reserved `grant:` namespace, and the heal
   additionally filters projections by subject. Recorded as an invariant: if
   the projection guard ever weakens, the heal becomes an escalation path.

4. **Headed scenarios are not hermetic in identity.** `App::boot` opens the
   SHARED personae vault regardless of `MERECAT_ROOT`; only `App::isolated`
   (unit tests) gets a per-profile vault. That is correct for the product — the
   browser should use the user's real identity — but it means scenario runs
   bind to it, and two merecat profiles now share one root key. Profile
   isolation is by storage location (per-session certificate files, scoped by
   `resource: denizen:<subject>`), not by key. Verified during the audit:
   merecat LOADED the existing profile (one profile, mtime unchanged from the
   vault plan's own work) rather than minting a rival.

## Progress

### 2026-07-23

- Plan written. Findings F1 and F3 verified by direct probe against the
  current code (the probe was scaffolding and was removed; the coverage table
  above is its output). F2 and F4 are readings of the same evidence.
- Design D1-D6 proposed for Mark; D4's cascade rule and the three open
  questions are the parts wanted ruled rather than inferred.

- **C0, C1 and C2 LANDED.** servitor 9 tests → 22, clippy clean; merecat 119
  with both features, 108 without; headed `denizen_b1.scn` and
  `denizen_wasm.scn` both RESULT ok.
  - **C0**: `cap.rs` with the `Capability` trait, `assert_capability_laws`
    (reflexivity + transitivity over a sample, so a broken order fails at the
    type that broke it), `Cap::{Power, Scope}` and `ScopePath`. The F1 table
    is inverted and asserted: `app/nav` no longer covers `app/navigate`,
    `app/session` no longer covers `app/session-admin`, and the traversal row
    is now *unparseable* rather than merely unmatched. `Grant.path_prefix`
    became `Grant.cap`; `PrefixAuthority` became `GrantTable` (the old name
    described a matching rule that is no longer its business).
  - **C1**: projections carry `cap`/`mode`/`subject`/`expires` as explicit
    tags plus a schema media type, and `read_projection` parses them back.
    The round-trip test covers a `Read` grant, which is the exact case F3
    silently corrupted. Readers fail closed: a projection missing fields
    yields no grant rather than a guessed one.
  - **C2**: `Grant.expires_at_ms` with `GrantTable::set_now`. servitor reads
    no clock, so it stays portable and deterministic; expiry bites at the
    named instant and needs no mutation of the store.

- **Structural finding: the phases are not independently landable.** C0
  changed a type merecat consumes live, so merecat's migration (planned as
  C5) had to land in the same change or the tree would not compile. Rings
  became `Cap::Power`s, the world stayed a `Cap::Scope`, and `capabilities_from_grant`
  (the piccolo face) now asks the same questions `emit_allowed` (the wasm
  face) does. The plan's remaining phases inherit this: C3 and C4 will each
  drag their consumer half along. A separate C5 is therefore **retired**; its
  content landed here.
  - Two consumers found beyond the expected ones: `remote_projection.rs` (a
    concurrent lane had wired the projection endpoint into the gate) and the
    committed G3 golden receipt, whose only diff was `graph/open/` →
    `graph/open`, the intended scope canonicalization. Regenerated via
    `cargo run --bin g3_receipt` rather than hand-edited.
- **Legacy heal**: `Ring::from_legacy_path` maps a pre-round `app/<ring>`
  projection to that ring's power on adopt, and unrecognized paths fall back
  to the scope they always were, so an existing session keeps its grants
  without a re-install.
- Deferred here and still open: **C3 delegation** and **C4 revocation**,
  plus the three open questions above, which want ruling before the
  cascade is written.

### 2026-07-24

- OQ1/2/3 ruled by Mark: replace-and-cascade, personae as the root identity,
  and unify the moot path on the order.
- **C3 landed, then was SUPERSEDED the same session** (commit `04529a59`).
  It shipped a standalone unsigned delegation algebra; then F5 turned up
  `personae::delegation`, which already had all of it, signed, with gemot
  consuming it. My error: built before grepping. Recorded rather than quietly
  rewritten, because the commit is in the history and the lesson is the
  reusable part.
- **C3' landed: servitor's delegation is now an ADAPTER over personae's signed
  certificates** (D3b). `DelegationTable` holds `SignedDelegationCertificate`s,
  verifies each chain through personae (signature, derived-key attestation,
  issuer binding, attenuation, depth, expiry), tracks revocations, and answers
  `AuthorityProvider::covers` through the `Cap` encoding. servitor's own
  duplicated chain/expiry logic is deleted. The tests now exercise real
  cryptography: a certificate signed by the wrong authority is refused
  (`a_forged_root_is_refused`), a widening child never verifies however well
  signed, a zero-depth holder cannot delegate at all, revoking one link
  cascades to its subtree, and a cold store verifies in any adopt order.
  servitor 34 -> 32 tests (fewer, because personae now owns what several of
  them were testing), clippy clean.
- **Sequencing correction**: the earlier note that "gemot's provider has no
  caller" was true only of the `MootAuthorizationProvider` trait; the
  delegation machinery beneath it is fully wired. OQ3's unification is
  therefore much closer than that note implied.
- **C4 LANDED.** merecat's denizen authority is now signed delegation.
  - **The root identity**: `merecat::identity` loads-or-creates a persisted
    Ed25519 master seed in the profile (`<data_root>/identity/master.key`).
    Persistence is load-bearing, not a nicety: every install certificate names
    this key as its root, so a key that changed across restarts would fail
    every chain as `WrongRoot` and silently un-authorize every denizen. The
    seed sits **unsealed** for now; personae's `IdentityVault` (sealed, where
    the SSH key already lives) implements the same `IdentityProvider` trait,
    so the swap is a constructor change once merecat has an unlock path in the
    shell. Named rather than invented.
  - **Install is a delegation**: `issue_install_certificates` signs one root
    certificate per reviewed capability (world scope, read face, one per ring)
    at `depth = 0`, so an installed helper may act but never sub-delegate.
    Certificates persist beside the world
    (`denizens/<subject>.certs.json`); the browsable grant projections stay as
    the human-readable audit record of the same facts.
  - **Uninstall revokes**: `Action::UninstallDenizen` calls
    `revoke_root_grants` (cascading to anything the denizen delegated onward),
    removes the binding facet, drops the runtime entry, and deletes the
    certificate file so a later adopt cannot resurrect revoked authority. The
    node and its world are untouched — revoking authority destroys nothing.
  - **Pre-delegation sessions heal**: a session installed before C4 carries
    only projections, so adopt re-issues certificates from them under the
    profile root. The projection IS the record of what the user reviewed, so
    this preserves exactly the reviewed grant rather than re-asking.
  - The lanes went provider-generic (`impl AuthorityProvider`), so neither the
    ring gate nor the piccolo face names a concrete table.
  - Receipts: merecat 124 tests (both features); headed `denizen_revoke.scn`
    RESULT ok (install → run → uninstall, with the node surviving and the
    certificate file gone); `denizen_b1.scn` and `denizen_wasm.scn` still
    RESULT ok on the new authority, with the wasm guest's `caps.granted()`
    still reporting exactly its three rings — now derived from verified
    certificate chains rather than a flat table.

- **Both follow-ons LANDED, closing the round.**
  - **The vault swap needed no shell unlock path after all**: personae's
    bootstrap ceremony already carries `Unlock::AutoOs` (DPAPI on Windows,
    no prompt) with `PERSONAE_PASSPHRASE` as the portable alternative — the
    gap named at C4 was already closed upstream. `merecat::identity` is now
    `RootIdentity::{Vault, Unsealed}`: vault-first via the SAME ceremony the
    personae bins use (`open_storage` + `load_or_create_profile`), against
    the SHARED default vault and the `default` profile — the profile the SSH
    agent serves — so merecat's root identity IS the user's personae
    identity, not a browser-local key. The unsealed path remains as a LOUD
    fallback for platforms with no sealed backend, and once the vault opens,
    the legacy plaintext seed is retired from disk. The boot log prints
    personae's honest protection description; the headed receipt shows the
    real thing: `OS auto-unlock sealed records at
    %LOCALAPPDATA%\personae\vault (DPAPI-wrapped root)`.
  - **The re-root heal** makes the migration safe: a stored chain that fails
    to verify under the current root (pre-delegation session, or the vault
    identity superseding the stopgap seed) re-issues from the grant
    projections — the record of what the user reviewed — under the new root,
    and rewrites the certificate file so the next adopt verifies without
    healing. Preserves the review exactly; nothing widens, nothing re-asks.
    Found and fixed in the process: rebuild issued heal certificates at a
    fresh clock read that could land after the table's `set_now`, leaving
    `not_before` in the table's future — one clock read per rebuild now.
  - **OQ3 landed as `gemot::TypedMootAuthorization`**: the moot authorization
    seam parses the request's `capability_path` as a servitor `Cap` at the
    boundary (D2's rule) and answers it from the moot's own delegation
    certificates through the same `power/...`/`scope/...` encoding the
    denizen tier writes. `MootGroup`'s membership impl was path-blind (any
    Write member covered every capability); the typed provider composes
    membership facts from any inner provider with per-path delegated
    coverage. Typed means typed: no silent bridge between vocabularies — a
    bare-string request parses as the scope it always was, never accidentally
    a power. One capability question, two tiers, differing only in root
    (constitution grant vs profile identity).
  - Receipts: gemot 96 (3 new: chain-answered coverage with a path-blind
    membership stub, closed powers at the moot tier, certificate-side
    expiry), servitor 32, merecat 127 (re-root heal + vault fallback +
    retirement tests); headed `denizen_revoke.scn` and `denizen_wasm.scn`
    RESULT ok rooted on the real DPAPI-sealed vault identity.
  - **The peer lane landed too**, closing the round with nothing deferred.
    `TypedMootAuthorization` answers gemot's own seam; its sibling
    **`MootAuthority` implements `servitor::AuthorityProvider`**, presenting
    the same moot certificates to the denizen gate. So a moot peer petitions a
    shared graph through the SAME `servitor::Gate` a script or component uses
    — same projection guard, same scope check, same attributed
    revision-checked commit — differing only in where the chain roots. The
    participant-gate doctrine's "one gate for scripts, wasm, peers, agents"
    is now a receipt rather than a claim, and it needed no new gate: only the
    adapter. Mode mapping is honest: the moot vocabulary carries one action
    (`act`), which satisfies Read and Write, while `Mode::Delegate` FAILS
    CLOSED because a moot expresses delegability as certificate depth, a
    different axis — approximating it with `act` would have been the F1
    ambiguity in a new costume.
  - Receipts (gemot 99, +3 more): a delegated peer's in-scope petition
    commits attributed to the peer, an out-of-scope one hits the gate's own
    scope check, an undelegated identity is refused, revoking the moot
    certificate stops the peer AT THE GATE, and delegate-mode fails closed.
