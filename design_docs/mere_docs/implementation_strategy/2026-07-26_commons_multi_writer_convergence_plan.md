# Commons Multi-Writer Convergence Plan

**Date:** 2026-07-26
**Status:** decision doc, no code yet. Answers **Decision 1** of the
[shared-engram commons brief](../research/2026-07-24_shared_engram_commons_brief.md),
which asked for "a written merge rule under which two offline members editing
the same container reconverge to one graph on sync, property-tested, before
any chat implementation slice". Decision 2 (moot-level group keys) stays open
and is not addressed here.

**Method, as the brief instructed:** check what `murm-replication` already
inherits from p2panda before designing anything. That check is section 1, and
it changed the answer: most of the convergence machinery is already present
and correct, and the gap is narrower and sharper than "define a merge
strategy". One of the three findings is a structural identity collision that
no ordering rule can repair.

## 1. What the substrate already gives (verified against the tree)

`murm-replication`'s `process_batch_atomic` sorts every corpus by
`(header.verifying_key, admission.target.log_id, header.seq_num)` before
committing, then validates each operation against a virtual frontier that
includes earlier operations in the same batch. So the replication layer
already supplies:

- **a deterministic total order across authors**, rooted in the author's
  cryptographic public key rather than a wall clock or an arrival time. Every
  peer computes the same order from the same set of operations;
- **per-author append-only logs** with backlink validation and a
  strictly-advancing retained frontier, so an author's own edits never
  reorder against each other;
- **idempotent, content-addressed insert**, so duplicate delivery over two
  lanes (gossip plus LogSync, or a native drop plus live sync) lands once;
- **prune and checkpoint law** already reasoned about for concurrent history
  truncation.

This is more than the brief assumed. Convergence does not need a new ordering
mechanism, a vector clock, or a CRDT library. It needs chartulary's edit
vocabulary to be well-defined under an order that already exists.

## 2. What chartulary needs defined on top

`chartulary::GraphEdit` has five variants: `InsertNode`, `RemoveNode`,
`Connect`, `Disconnect`, `Derive`. Each was designed single-writer. Three
gaps, in descending severity.

### Gap 1: `EdgeId` collides across writers (structural, not a tiebreak)

`EdgeId(pub u64)` is minted from a per-log counter, `EdgeId(self.next_edge)`
then `self.next_edge += 1`, and restored on replay as `max(seen) + 1`. The
G1 design doc states this openly as a single-writer decision.

Two members editing offline both mint `EdgeId(7)` for different edges. On
merge the journal contains two `Connect` entries claiming the same identity,
`by_edge_key` and `edge_key(id)` collapse them, and `Disconnect(EdgeId(7))`
becomes ambiguous. **No ordering rule fixes this**, because the order is over
operations while the collision is in the addressing space the operations
refer to. The graph would converge to *a* state on every peer while retracting
the wrong edge.

**Rule: edge identity becomes (author, counter), unique by construction.**
Illustrative, not compile-ready:

```rust
pub struct EdgeId { pub author: PublicKey, pub counter: u64 }
```

Each writer keeps its cheap local monotonic counter, and uniqueness comes
from the key it is paired with. This is the same shape p2panda already uses
for operation identity (author plus sequence), so it introduces no new
concept. Content-hashing the endpoints was considered and rejected: it makes
two writers independently asserting the identical edge collapse into one
identity, which is defensible for a set but wrong for the multigraph
semantics chartulary chose at G0, where parallel edges between the same pair
are distinct edges.

**This is a wire change to `GraphEdit::Connect`, so it must land before any
shared container carries real data.** Single-writer graphs migrate by pairing
existing ids with the local identity.

### Gap 2: `InsertNode` is a whole-node upsert, so LWW is coarse

`InsertNode(N)` upserts by identity. Two writers editing one node converge
under the section-1 order, so there is no divergence, but the loser's whole
payload is discarded even when the two touched unrelated parts of the node.

**Rule: last-writer-wins under the existing total order**, which is
deterministic and needs no new mechanism. **And the granularity question is
named rather than absorbed:** the
[one-node facets ruling](../technical_architecture/2026-07-18_one_node_facets_layer_map.md)
makes facets atomic, so the right long-run answer is that a facet write is
its own edit and merges per facet. Until facet-grained edits exist, a
concurrent edit to one node is whole-node LWW, and that limit belongs in the
commons profile's documentation rather than in a surprise.

### Gap 3: remove versus connect has no stated winner

`RemoveNode(id)` removes the node and its incident edges. A concurrent
`Connect` naming that node lands on one side of the order or the other.
Replay already tolerates a dangling connect (the spine advances `next_edge`
even then), so every peer reaches the same state, but which state depends on
an ordering accident rather than a decision.

**Rule: remove wins.** A removed node identity is tombstoned for the
container, and a later-ordered `Connect` naming it is dropped rather than
resurrecting it. Chosen to match the destructive-operation reasoning already
ruled in the murm/moot lane: a deletion the user performed should not be
undone by someone else's concurrent edge. Add-wins is the defensible
alternative and is rejected deliberately, not overlooked.

### Not gaps

`Disconnect` becomes a set-remove on a unique key once Gap 1 lands, and
commutes. Duplicate delivery is already idempotent at the operation layer.
`Derive` appends provenance and wants a dedup-on-equal rule so two writers
recording the same derivation do not append it twice, which is a one-line
fold detail rather than a convergence question.

## 3. One more thing the check turned up

`chartulary::Author` is `Author(pub String)`, a caller-chosen label such as
`"ui"` or `"pre-gate"`. It is display and attribution, **not** identity, and
it must not be used as a merge tiebreak or as the author half of an
`EdgeId`. The identity for both is the replication layer's verifying key.
Recorded because the type is named `Author` and sits right beside the edit
spine, which makes it the obvious wrong choice to reach for.

## 4. Sequence

Done-conditions, not dates.

- **M0. Demonstrate the collision. DONE 2026-07-26.** Two receipts landed in
  `chartulary::commit::tests`, both green, both documenting present behavior:
  `two_offline_writers_mint_the_same_edge_id_for_different_edges` (two
  partitioned replicas each mint `EdgeId(0)` for unrelated edges) and
  `a_merged_journal_cannot_address_both_colliding_edges` (replaying both
  journals gives a graph with **two edges and one addressable id**, so the
  second edge is unreachable by any retraction). chartulary 42 -> 44 tests.
  The collision is now measured rather than inferred from reading the mint.
  When M1 lands, the first test flips to asserting the ids **differ** and the
  second to two addressable keys.
- **M1. Author-scoped `EdgeId`.** The type change, the mint, the snapshot
  field, and the single-writer migration. Done when M0's property test passes
  and chartulary's existing suite stays green.
- **M2. The two stated rules.** Remove-wins tombstones and whole-node LWW
  under the section-1 order, each with a property test that pins the losing
  side rather than only asserting equality.
- **M3. Reconvergence over a real lane.** Two `JoinedSpace` instances over
  Memory, then p2panda, editing one container while partitioned, converging
  on reconnect. Reuses the join ceremony landed 2026-07-25, so this is a test
  rather than new machinery.
- **M4. Write the limit down** in the commons profile: whole-node LWW until
  facet-grained edits exist, and remove-wins as a stated product behavior.

## 5. Non-goals

No CRDT library and no new ordering mechanism: the total order exists and is
deterministic. No operational transform. No per-facet merge until facet-
grained edits exist, which is the one-node facets lane's work and not this
plan's. Decision 2 (group keys) is untouched here; it gates encrypted
commons traffic, not convergence.
