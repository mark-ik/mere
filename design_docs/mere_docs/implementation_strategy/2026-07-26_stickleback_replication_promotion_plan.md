# Stickleback Replication Promotion Plan

**Date:** 2026-07-26
**Status:** planned; S0 is the first slice.
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
