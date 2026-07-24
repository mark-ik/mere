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

- **C3, delegation** (servitor). `Delegation`, `DelegationId`, chain validity
  with D3's three invariants, user-as-root. Install becomes an attenuating
  delegation. **Done when** a denizen can delegate a strict subset onward, an
  attempt to delegate wider than held is refused, and a chain verifies from a
  cold store.

- **C4, revocation** (servitor + merecat). Sever plus D4's lazy cascade, and
  the user-visible Uninstall row beside Run. **Done when** severing a
  delegation stops its whole subtree, receipted headed: install, run, uninstall,
  run refused.

- **C5, the merecat migration** (merecat). Rings become `Power`s, `scenario/`
  stays a `Scope`, legacy read exercised against a pre-round session. **Done
  when** both denizen lanes still pass and the wasm receipt is green from a
  session written before this round.

## Open questions

1. **Attenuation on re-review.** When a pack upgrade asks for more, the trust
   model says "widening upgrades re-review". Does the new delegation replace
   the old one (losing any onward delegations the denizen made under it), or
   sit beside it? Proposed: replace, and cascade, since the old chain described
   a capability set the user has now explicitly reconsidered.
2. **Root identity.** Is the user's root subject a fixed well-known constant, or
   the active personae identity? The second is more correct and drags personae
   into servitor's dependency set; the first is a placeholder that will need
   migrating. Proposed: a distinguished root subject for C3, personae-backed
   when the identity vault lands.
3. **Moot alignment** (inherited as the previous plan's OQ6). Whether the
   peer-apply path adopts this same order, or gemot's provider stays a parallel
   implementation. Untouched by this round; the typed `Cap` makes the bridge
   cheaper, not free.

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
