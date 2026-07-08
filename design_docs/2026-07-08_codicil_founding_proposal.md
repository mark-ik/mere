# codicil Founding Proposal

**Date:** 2026-07-08
**Status:** founding proposal. This repo's first doc. codicil is a **fresh build**,
founded alongside muniment from the same four-consumer survey. The append-only
log and its muniment persistence are ported to code and green in this commit.

## 1. What codicil is

codicil is an append-only, replayable log. A `Codicil<T>` is a linear sequence of
immutable entries:

- **Append, never edit.** `append` adds an entry and stamps it with a monotonic
  `Seq`. Entries are never mutated or removed. A change is a new entry.
- **Replay to a state.** `replay` folds the entries oldest-first into whatever
  state they describe; `replay_from` advances an existing state by only the new
  entries.
- **Durable cursor.** A `Seq` refers to the same entry for the life of the log,
  so a reader or peer holds one across sessions and calls `from(cursor)` to get
  only what is new.
- **Persisted through muniment.** `Codicil<T>` is `Serialize`/`Deserialize`, so it
  stores as one muniment slot. `save` / `load` are the convenience over that.

It is the event-source and nondestructive-history primitive the survey found in
three of the four consumers.

## 2. Why a standalone crate, and why separate from muniment

A store answers "what is the current value at this key." A log answers "what is
the ordered sequence of edits, replay them to rebuild state." Order plus replay
is real structure a plain store lacks, and it recurs across the apps:

- **isometry**: a session is an ordered event log replayed into board state.
- **strophe**: `History` is a nondestructive edit log; `Session` is its
  materialized state.
- **mere**: graph mutations are an append-only history with lineage.
- **woodshed**: practice history and versioned notes are append-shaped (its
  settings slot is the one flat exception).

codicil is that primitive, factored out. It is a **separate crate from muniment**
because the split is not symmetric: woodshed wants muniment's store without a log,
so muniment must stand alone, and codicil depends on muniment (not the reverse).
Two clean crates, one dependency direction.

## 3. Scope guards

Two deliberate limits keep codicil from ballooning:

- **Transport-neutral.** codicil produces a replayable sequence; it does not ship
  it anywhere. isometry replicating its log over iroh stays in isometry-net, the
  way armillary is host-neutral. This respects isometry's stated "DM-authority
  ordered log, no CRDTs, no rollback" rule: codicil is exactly that ordered log,
  nothing fancier.
- **Linear, not a tree.** This cut is a linear append-only log, the shared common
  case. strophe's undo/redo edit-*tree* is a richer structure a consumer layers
  on top, or a later extension here. The founding does not commit to a branching
  edit-DAG.

## 4. Roadmap

Done-conditions, not time estimates.

- **P0 (this commit): the linear log and whole-slot persistence.** `Codicil<T>`,
  `Seq`, append/get/from/replay/replay_from, and save/load through a muniment
  slot. **Done when** `cargo test` is green (it is, 8 tests including a muniment
  round-trip).
- **P1: content-addressed per-entry storage.** Instead of rewriting the whole log
  on each save, write each new entry as its own `muniment::BlobStore` blob and
  keep only an ordered index of hashes as the slot. An append becomes one small
  write, and entries become immutable by hash, which is exactly the strophe-media
  and eidetic-engram pattern applied to log entries. **Done when** appending to a
  large log is O(one entry), not O(log).
- **P2: incremental persistence and compaction.** Persist only entries past the
  last-saved `Seq`; optionally snapshot-and-truncate a long log to bound replay
  cost. **Done when** a long-running log persists in bounded time and space.
- **P3: consumer adoption.** isometry moves its session event log onto codicil;
  strophe's `History` re-bases onto it. **Done when** one app's live history runs
  through codicil.

The branching edit-tree (undo/redo with divergent history) is tracked as a
possible P4 shape, gated on a real consumer wanting divergence rather than linear
history.

## 5. Relationship to the stack

codicil is the versioning layer over muniment (the store), beneath the content
model (notes, tags, lists) that users author. The content model is trending
toward a content-addressed container graph, where a graph's mutations are exactly
a codicil of graph edits: an event-sourced graph is a codicil replayed into a
materialized graph, stored in muniment. codicil is founded now; that graph
substrate is a separate design pass.

## 6. Licensing

Fresh code, MPL-2.0 to match the sibling crates (muniment, vates, sibylla,
armillary). A family relicense to `MIT OR Apache-2.0` before publish is Mark's
call, decided together. The `muniment` dependency is a local path dep during
development and becomes a git dep on publish.

## Provenance

Founded from the same 2026-07-08 four-consumer persistence survey as muniment
(woodshed, strophe, isometry, mere). The two-crate decision (muniment for the
store, codicil for the log) and the append-only framing are recorded in the
workspace memory.
