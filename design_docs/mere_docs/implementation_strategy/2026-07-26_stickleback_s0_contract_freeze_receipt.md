# Stickleback S0 Receipt: Frozen Contract

**Date:** 2026-07-26
**Status:** S0 complete. S1 (the path and package cutover) is unblocked.
**Receipt for:** S0 of the
[Stickleback replication promotion plan](./2026-07-26_stickleback_replication_promotion_plan.md).

This is the checked-in consumer/export matrix S0 requires, plus the boundary
fixtures and the constants that must survive the rename. S1 checks its result
against this document; S3 reviews the crate against it.

## Result

The contract is frozen and green. `murm-replication` exports 66 public items
across 8 modules, plus the `VERSION` and `STAGE` consts and the public `drop`
module. Every export was classified against its live consumers.

- **66 promoted unchanged** as generic replication mechanics.
- **0 domain-specific exports** found in the crate. No public type carries Murm
  message grammar or conversation authority, so the S0 stop rule did not fire
  and nothing moves back to Murm.
- **1 compatibility-only surface**, and it is not in this crate: the
  `mere-transport::synced_space` module.

Tests: `cargo test -p murm-replication` — **45 passed, 0 failed** (40 existing
unit tests, 5 new boundary fixtures).

## Findings that change later slices

**1. The compatibility module S2 targets already has zero callers.**
[`crates/murm/transport/src/synced_space.rs`](../../../crates/murm/transport/src/synced_space.rs)
is three lines re-exporting `SyncRound`, `SyncStatus`, and `SyncedSpace`. No
workspace file imports them through transport; Mesh re-exports from
`murm_replication` directly at [`sync.rs:41`](../../../crates/mesh/mesh/src/sync.rs).
S2 needs no caller migration — it is a deletion plus the `pub mod` line in
transport's `lib.rs`. S2 can fold into S1 if convenient.

**2. One derivation constant contains "murm" and must not be renamed.**
[`receipt.rs:79`](../../../crates/murm/replication/src/receipt.rs) derives peer
storage scope through
`blake3::derive_key("mere murm drop receipt peer scope v1", identity)`. The
context string is an input to stored muniment keys. A find-and-replace of
`murm` during S1 would silently repoint every stored peer receipt — exactly the
plan's stop rule on receipt hashes. A golden vector now pins it
(`receipt_peer_scope_survives_the_promotion`). **S1 must rename package and
import paths only, never string literals.**

The frame magics are already domain-neutral and need no special handling:
`MEREDRP\0` (drop.rs), `MERERCP\0` (receipt.rs), both version 1, extension
`.meredrop`.

**3. `CheckpointAuthority` is declared here but enforced entirely in domains.**
The trait has zero call sites inside the crate. Mesh
([`retention.rs:178`](../../../crates/mesh/mesh/src/retention.rs)) and Gemot
([`records/retention.rs:55`](../../../crates/moot/gemot/src/moot/records/retention.rs))
implement it; Gemot's governance consumes it generically at
[`governance.rs:231`](../../../crates/moot/gemot/src/moot/constitution/governance.rs).
It stays in Stickleback because it is the interop contract two independent
domains share, but the plan's phrase "Stickleback owns checkpoints" should be
read precisely: Stickleback owns the *erasure mechanics* and the *seam*; the
*gate* is domain-side. S3's crate docs must say this.

**4. Murm defines its own `SyncStatus` and `SyncRound`.**
[`gossip_sync.rs:50,67`](../../../crates/murm/murm/src/gossip_sync.rs) are
separate gossip-lane types that deliberately mirror the replication drain's
names and compose with them. They are not imports and must not be unified
during S1.

**5. No deletable orphans.** 28 exports have no direct consumer import, but all
28 appear in a public signature of a consumed function or trait — they are
reachable in return or parameter position, not dead. Nothing in the export list
is a rename artifact awaiting cleanup.

## Consumer/export matrix

Consumers: **M** = murm, **S** = mesh, **G** = gemot, **H** = moothold,
**T** = mere-transport. A blank row means no direct import; the "via" column
names the public signature that reaches it.

### Processing and storage — the shared insertion path

| Export | M | S | G | H | T | Class |
|---|---|---|---|---|---|---|
| `OperationProcessor` | ● | ● | ● | ● |  | generic |
| `OperationPolicy` | ● | ● | ● | ● |  | generic |
| `Admission` | ● | ● | ● | ● |  | generic |
| `StoreTarget` | ● | ● | ● | ● |  | generic |
| `Reject` | ● | ● | ● | ● |  | generic |
| `ProcessError` | ● | ● | ● | ● |  | generic |
| `ProcessOutcome` | ● | ● | ● |  |  | generic |
| `MunimentStore` | ● | ● | ● | ● |  | generic |
| `HistoryAction` |  |  |  |  |  | generic, via `Admission::history` |
| `BlobGcReport` |  |  |  |  |  | generic, via `collect_unreferenced_blobs` |

All four domains share one processor and one store. No consumer holds a second
insertion path.

### Join and drain

| Export | M | S | G | H | T | Class |
|---|---|---|---|---|---|---|
| `JoinedSpace` | ● | ● | ● |  |  | generic |
| `SyncedSpace` |  | ● | ● |  | ○ | generic |
| `SyncStatus` |  | ● |  |  | ○ | generic |
| `SyncRound` |  | ● |  |  | ○ | generic |
| `JoinError` |  |  |  |  |  | generic, via `JoinedSpace::join` |

○ = reached through the transport compatibility module deleted at S2. Murm's
same-named gossip types are its own (finding 4).

### Retention authority

| Export | M | S | G | H | T | Class |
|---|---|---|---|---|---|---|
| `CheckpointAuthority` |  | ● | ● |  |  | generic seam, domain-enforced (finding 3) |

### Native drop format

| Export | M | S | G | H | T | Class |
|---|---|---|---|---|---|---|
| `DropRecord` | ● | ● | ● |  |  | generic |
| `DropId` |  |  | ● |  |  | generic |
| `DropLimits` | ● |  | ● |  |  | generic |
| `DropProtector` | ● |  | ● |  |  | generic |
| `NativeDropError` | ● |  | ● |  |  | generic |
| `DropWriteReceipt` |  |  | ● |  |  | generic |
| `EvidenceKind` |  |  | ● |  |  | generic |
| `write_plain_drop` | ● |  | ● |  |  | generic |
| `write_protected_drop` | ● |  | ● |  |  | generic |
| `read_plain_drop` |  |  | ● |  |  | generic |
| `read_protected_drop` |  |  | ● |  |  | generic |
| `DropManifest` |  |  |  |  |  | generic, via `read_plain_drop` |
| `ManifestEntry` |  |  |  |  |  | generic, via `DropManifest` |
| `DropReadReport` |  |  |  |  |  | generic, via `read_plain_drop` |
| `visit_plain_drop` |  |  |  |  |  | generic, streaming variant |
| `visit_protected_drop` |  |  |  |  |  | generic, streaming variant |

### Drop carriage and selection

| Export | M | S | G | H | T | Class |
|---|---|---|---|---|---|---|
| `DropExportSelector` | ● | ● | ● |  |  | generic |
| `DropExportDecision` | ● | ● | ● |  |  | generic |
| `DropExportBudget` | ● | ● | ● |  |  | generic |
| `DropExportProfile` | ● |  | ● |  |  | generic |
| `DropExportStats` |  |  | ● |  |  | generic |
| `DropImportReport` | ● |  | ● |  |  | generic |
| `DropIoError` | ● |  | ● |  |  | generic |
| `export_topic_operations` | ● |  | ● |  |  | generic |
| `export_topic_operations_selected` | ● | ● | ● |  |  | generic |
| `import_plain_drop` | ● |  | ● |  |  | generic |
| `import_protected_drop` | ● |  | ● |  |  | generic |
| `import_drop_records` |  |  | ● |  |  | generic |
| `decode_operation_record` | ● |  | ● |  |  | generic |
| `operation_record` |  |  | ● |  |  | generic |
| `export_plain_topic_file` |  |  |  |  |  | generic, file variant |
| `export_protected_topic_file` |  |  |  |  |  | generic, file variant |
| `export_selected_plain_topic_file` |  |  |  |  |  | generic, file variant |
| `import_plain_drop_file` |  |  |  |  |  | generic, file variant |
| `import_protected_drop_file` |  |  |  |  |  | generic, file variant |
| `DropFileExportReport` |  |  |  |  |  | generic, via file variants |

### Staged drops and peer receipts

No consumer wires these yet. All are internally used and reachable through the
import path; they are a coherent unbuilt feature, not rename residue.

| Export | M | S | G | H | T | Class |
|---|---|---|---|---|---|---|
| `StagedDrop` |  |  |  |  |  | generic, via `list_staged_drops` |
| `list_staged_drops` |  |  |  |  |  | generic |
| `resume_staged_drop` |  |  |  |  |  | generic |
| `discard_staged_drop` |  |  |  |  |  | generic |
| `DropReceipt` |  |  |  |  |  | generic |
| `DropReceiptError` |  |  |  |  |  | generic |
| `DropReceiptLimits` |  |  |  |  |  | generic |
| `ReceiptPeer` |  |  |  |  |  | generic, **frozen derivation** (finding 2) |
| `read_drop_receipt` |  |  |  |  |  | generic |
| `write_drop_receipt` |  |  |  |  |  | generic |
| `local_drop_receipt` |  |  |  |  |  | generic |
| `peer_drop_receipt` |  |  |  |  |  | generic |
| `store_peer_drop_receipt` |  |  |  |  |  | generic |
| `discard_peer_drop_receipts` |  |  |  |  |  | generic |

### Crate metadata

`VERSION` and `STAGE` are consts; `STAGE` is `"pre-alpha"`.

## Boundary fixtures

[`crates/murm/replication/tests/boundary.rs`](../../../crates/murm/replication/tests/boundary.rs)
covers each generic extension point S0 names, through the public surface only,
against a neutral test domain called "ledger" that is not Murm, Mesh, or Moot.
Placing them in `tests/` rather than a `#[cfg(test)]` module is deliberate: an
integration test can only see the exported contract, so it fails if the
promotion narrows the public surface.

| Fixture | Extension point proven |
|---|---|
| `policy_accepts_and_rejects_before_storage` | a domain policy accepts and rejects *before* the store mutates |
| `local_and_received_operations_share_one_store` | locally authored and received operations share one processor and store; repeats are idempotent |
| `checkpoint_authority_gates_destructive_retention` | destructive retention runs only behind a domain authority decision; a stranger and a stale revision are both refused |
| `native_drop_import_passes_through_the_processor` | an offline drop imports through the same processor and cannot bypass policy |
| `receipt_peer_scope_survives_the_promotion` | the `ReceiptPeer` KDF context string is unchanged (finding 2) |

## S1 preconditions

1. Rename package name and import paths. Do not touch string literals,
   particularly `receipt.rs:79`.
2. Keep Murm's `gossip_sync` `SyncStatus`/`SyncRound` separate from the drain's.
3. `transport::synced_space` may be deleted outright; it has no callers.
4. Re-run the five boundary fixtures after the move. They are the tripwire for
   a narrowed public surface or a changed derivation.
5. `murm-replication` has no published release consuming it from outside this
   workspace, so no forwarding release is required — confirm against the
   registry at S1 rather than assuming.
