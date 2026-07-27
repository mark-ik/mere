# Stickleback Replication Promotion Plan

**Date:** 2026-07-26
**Status:** **complete 2026-07-27.** S0 froze the contract, S1 cut the path and
package over, S2 deleted the compatibility surface, and S3 passed the
publishable-boundary review. `stickleback` 0.1.0 is published on crates.io
(MIT OR Apache-2.0). S0's frozen contract, its four findings, and its boundary
fixtures are recorded in the
[S0 contract freeze receipt](./2026-07-26_stickleback_s0_contract_freeze_receipt.md);
S1 through S3 are in [Receipts](#receipts) below. A sibling repository remains
gated on a real external consumer and its own plan.
**Decision:** promote `murm-replication` to the Mere-owned `stickleback` crate.
The promotion stays inside the Mere repository. It is a domain-neutral
boundary and name cutover, not a new sibling repository or a new replication
implementation.
**Refines:** the
[Murm peer runtime and Moot domain plan](./2026-07-12_murm_peer_runtime_and_moot_domain_plan.md).

## Why the promotion is earned

The current package name says the implementation belongs to Murm. Its live
consumer graph says otherwise.

| Consumer | Shared machinery used |
|---|---|
| Murm | joined spaces, accepted-operation processing, storage, native drops |
| Mesh | joined spaces, sync status, storage, retention, drop export |
| Gemot/Tessera | joined spaces, storage, checkpoint authority, native drops |
| Moothold | operation processing and storage |
| `mere-transport` | compatibility re-exports for the shared sync drain |

This is a real multi-consumer subsystem. Its present public contract already
states the right ownership rule:

- Stickleback owns replicated-space joining and draining, policy-before-insert
  processing, operation storage, checkpoints, retention mechanics, and native
  drop carriage.
- Murm, Mesh, Moot, and future domains own operation grammar, addressing,
  authorization, and deterministic materialization.

The promotion names that boundary truthfully. It does not widen it.

## Target

```text
crates/stickleback
  src/
    authority.rs
    drop.rs
    drop_io.rs
    joined_space.rs
    processor.rs
    receipt.rs
    store.rs
    synced_space.rs

Murm --------\
Mesh ---------+--> Stickleback --> p2panda sync + Muniment storage
Moot ---------+         |
future domain/          +--> domain-supplied OperationPolicy
```

The Cargo package and Rust library are both `stickleback`. The path moves from
`crates/murm/replication` to `crates/stickleback` so the filesystem stops
restating the obsolete ownership.

The following public concepts keep their present behavior and names during the
cutover:

- `JoinedSpace`, `SyncedSpace`, `SyncRound`, and `SyncStatus`;
- `OperationProcessor`, `OperationPolicy`, and process outcomes;
- `MunimentStore`;
- `CheckpointAuthority`;
- native drop, selection, import, export, and receipt types.

Renaming those concepts is outside the promotion slice.

## Boundary rules

1. Stickleback accepts domain policy through traits and callbacks. It does not
   learn Murm messages, Moot constitutions, Tessera records, or Mesh jobs.
2. Signature validity is shared mechanics only where the operation envelope
   makes it generic. Authority to perform a domain action remains in the
   domain.
3. Storage and replication acceptance use the same `OperationProcessor`.
   Local authoring, gossip, LogSync, and drop import may not acquire separate
   insertion paths during the move.
4. Muniment remains the storage substrate. Stickleback does not grow a second
   database abstraction as part of a rename.
5. Existing retention, checkpoint, and native-drop receipts remain authority.
   Moving files does not relitigate them.

## Build order

### S0. Freeze the promoted contract

**Files:**

- `crates/murm/replication/src/lib.rs`
- all current `murm_replication` consumers
- `Cargo.toml`
- this plan

Check every public export against its live consumers. Classify it as:

- generic replication mechanics and promoted unchanged;
- domain-specific and left with its domain;
- compatibility-only and deleted at S2.

Add a focused boundary test or compile fixture for each generic extension
point:

- one policy accepts and rejects before storage;
- one store is shared by local and received operations;
- checkpoint authority gates destructive retention;
- native drop import passes through the same processor.

**Receipt:** a checked-in consumer/export matrix and the current package's
tests green before any path or name change.

**Stop rule:** if a public type carries Murm message grammar or conversation
authority, move that type back to Murm before continuing.

### S1. Atomic path and package cutover

**Moves and edits:**

- `crates/murm/replication` to `crates/stickleback`
- package name `murm-replication` to `stickleback`
- Rust imports `murm_replication` to `stickleback`
- root workspace member and dependency entries
- consumer manifests in Murm, Mesh, Gemot, Moothold, and transport
- crate docs, README links, and non-historical architecture docs
- `Cargo.lock`

Keep the source otherwise byte-equivalent where practical. Do not combine
module cleanup, trait redesign, or behavior changes with the move.

Before deleting the old package name, check whether a published
`murm-replication` release has a known clean-checkout consumer outside this
workspace. If it does, publish one deprecated forwarding release with an
explicit removal note. If it does not, delete the old package and path in the
same cutover. The pre-alpha workspace does not carry a compatibility crate for
its own sake.

**Receipt:**

```text
cargo test -p stickleback
cargo test -p murm
cargo test -p mere-mesh
cargo test -p gemot
cargo test -p moothold
cargo test -p mere-transport
cargo check --workspace --all-targets
cargo fmt --all -- --check
git diff --check
```

Use the actual package names from each manifest if a command differs. A scoped
search must find no build or source reference to `murm-replication` or
`murm_replication`.

### S2. Remove compatibility ownership

`mere-transport::synced_space` currently re-exports shared sync types for old
callers. Move every caller to Stickleback directly, then delete the
compatibility module. Transport should carry streams and authenticated facts;
it should not be an alternate replication facade.

Update current docs to call Stickleback the shared replication layer.
Historical plans retain their original package spelling with one promotion
note rather than being rewritten as if the old architecture never existed.

**Receipt:** the reverse dependency map has one shared replication package,
and no public module re-exports Stickleback merely to preserve the old Murm
path.

### S3. Publishable boundary review

Review the crate as an independently usable Mere component:

- crate-level docs state what the domain must provide;
- features do not drag Murm, Mesh, or Moot into a minimal build;
- public errors do not name one domain;
- examples use a tiny neutral test domain;
- licensing, repository, README, keywords, and docs.rs metadata are current.

This is packaging rigor, not a repository extraction. A sibling repository
requires a real external consumer and its own plan.

**Receipt:** a clean-checkout build of Stickleback and the neutral example,
plus the consumer suite from S1.

## Documentation cutover

Add a promotion note to the July 12 Murm/Moot plan. Update the live Mere crate
map and `design_docs/DOC_README.md`. Keep old receipts that name
`murm-replication`; the date and old package name are part of their evidence.

## Stop rules

- Stop if the move changes stored operation bytes, topic ids, drop formats, or
  receipt hashes.
- Stop if a shared helper bypasses a domain's authorization or fold.
- Stop if a compatibility facade creates two public ways to join or process a
  space.
- Stop a repository extraction until a second repository consumes
  Stickleback from a clean dependency edge.
- Keep unrelated replication improvements out of S1.

## Done condition

`stickleback` is the sole shared replication package; it lives at
`crates/stickleback`; Murm, Mesh, Moot, and transport consumers build against
it directly; the old package, path, and compatibility re-exports are gone
unless an evidenced external release requires one bounded forwarding version;
all stored and wire formats are unchanged; and domain authority remains in
the domain crates.

**Met 2026-07-27**, with one clause overshot: transport does not build against
Stickleback directly, because S2 removed its last reason to depend on it at all.
See the S2 receipt.

## Receipts

### S1 — atomic path and package cutover (complete 2026-07-27)

`crates/murm/replication` is now `crates/stickleback` and the package is
`stickleback`. Git recorded every source file as a rename, so history follows
the crate. Import paths, the root workspace member and dependency entry, all
five consumer manifests, `Cargo.lock`, the crate README, and the live docs moved
with it. A scoped search finds no build or source reference to the old package
or module name.

Tests, all green:

| Package | Result |
|---|---|
| `stickleback` | 45 passed (40 unit, 5 boundary) |
| `murm` | 57 passed |
| `mere-mesh` | 29 passed |
| `gemot` | 102 passed |
| `moothold` | 18 passed |
| `mere-transport` | 40 passed |

`cargo check --workspace --all-targets` exits 0, `cargo fmt` is clean across all
six packages, and `git diff --check` is clean.

Two notes on the receipt commands themselves:

- `mere-transport`'s `tests/session_policy.rs` needs its optional
  `session-policy` feature, so the bare `cargo test -p mere-transport` and
  `cargo check --workspace --all-targets` both fail on that target — before this
  cutover as much as after. Add `--features mere-transport/session-policy`. This
  is a pre-existing feature-gating gap, not promotion fallout, and it is the only
  red in the workspace.
- Renaming shortened every `use` line by five characters, so rustfmt rewrapped
  the import blocks in murm, mesh, gemot, and moothold. That is why those four
  packages carry formatting churn beyond the identifier change.

**No forwarding release was needed.** `murm-replication` 0.1.0 is published, and
it does have two registry reverse dependencies — but they are `gemot` 0.1.0 and
`moothold` 0.1.0, both published *from this workspace*. The plan's test is a
clean-checkout consumer *outside* the workspace, and there is none, so the old
package and path were deleted in the same cutover. Published gemot/moothold
0.1.0 keep resolving against the immutable published `murm-replication` 0.1.0;
their next release will name `stickleback`. The name `stickleback` was
unclaimed on crates.io.

The `ReceiptPeer` golden vector passes unchanged, which is the direct evidence
for the "no changed receipt hashes" stop rule: the KDF context string containing
"murm" survived the sweep.

**Documentation cutover.** Live docs now say Stickleback: the root README crate
map (a new `crates/stickleback` row, with the crate removed from the `crates/murm`
row), `design_docs/DOC_README.md` (crate map and index), `TERMINOLOGY.md` (the
term is promoted out of Murm's sub-list to a top-level comms layer), the crate
README and `lib.rs` header, `mooting`'s README, the commons convergence plan, and
the shared-engram commons brief. Promotion notes were added to the two July 12
plans that define the old ownership and carry open work. The other historical
plans that mention `murm-replication` in passing
(2026-06-03 host p2p wiring, 2026-07-09 merecat boundary, 2026-07-17 participant
gate, 2026-07-22 graphshell projection) and `MURM_AS_BILATERAL.md`, which already
declares itself superseded, keep their original spelling as evidence.

### S2 — remove compatibility ownership (complete 2026-07-27)

S0 established that `transport::synced_space` had no callers, so this was a
deletion, not a migration: the module, its `pub mod` line, and its `pub use`
re-export are gone. It was kept out of S1 deliberately, per the stop rule
against combining module cleanup with the move.

One consequence went past the plan's wording and is worth stating plainly.
`transport`'s *only* use of the shared crate was that facade, so removing it
left `stickleback` an unused dependency in transport's manifest, and it was
removed too. **Transport is therefore no longer a Stickleback consumer at all**
— a stronger outcome than the done condition's "Murm, Mesh, Moot, and transport
consumers build against it directly", and the right one: transport carries
streams and authenticated facts, and now depends on nothing replication-shaped.
The live consumer set is Murm, Mesh, Gemot/Tessera, and Moothold.

`mere-transport` still passes 40 tests with its `session-policy` feature.

### S3 — publishable boundary review (complete 2026-07-27)

`stickleback` 0.1.0 is published on crates.io under MIT OR Apache-2.0.

Reviewed against S3's list:

- **Crate docs state what the domain must provide.** The `lib.rs` header now
  names the two seams a domain supplies (`OperationPolicy`, and
  `CheckpointAuthority` where it prunes) and says outright that Stickleback
  supplies erasure mechanics while the domain holds the gate — S0's finding 3,
  written where a reader of the crate will find it.
- **Features drag no domain into a minimal build.** There is no `[features]`
  table and no domain dependency to gate.
- **No public error names one domain.** All five public error enums
  (`ProcessError`, `DropIoError`, `NativeDropError`, `JoinError`,
  `DropReceiptError`) and every `#[error]` string are domain-neutral.
- **A neutral example exists.** `examples/ledger.rs` builds a "ledger" domain
  that is deliberately none of the real consumers, and exercises the whole
  contract: policy refusal before storage, idempotent re-offer, and the
  domain-side retention gate. It runs green.
- **Metadata is current.** License, repository, readme, keywords, and categories
  were already right; the description was rewritten to lead with the boundary
  instead of listing Murm, Moot, and mesh, since a crate meant to serve a fourth
  domain should not read as three domains' plumbing.

The packaging fix S3 actually needed was a dependency version. `muniment` was a
bare path dependency in the workspace, which `cargo publish` refuses. Both path
deps are published (`muniment` 0.1.1, `mere-proofs` 0.1.0), so `muniment` gained
a version alongside its path, matching the convention already used for `proofs`
and `identity`. A `--dry-run` confirmed the packaged crate compiles against the
registry copies rather than the workspace paths. The published archive is 17
files: sources, README, the example, and the boundary fixtures.

**`murm-replication` 0.1.0 was deliberately not yanked.** Published `gemot`
0.1.0 and `moothold` 0.1.0 depend on it, and yanking would break fresh
resolution of those releases for no gain. It stays as a dead but resolvable
version; their next releases will name `stickleback`.

This was packaging rigor, not a repository extraction. A sibling repository
still requires a real external consumer and its own plan.
