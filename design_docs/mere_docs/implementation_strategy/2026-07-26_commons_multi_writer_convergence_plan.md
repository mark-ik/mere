# Commons Multi-Writer Convergence Plan

**Date:** 2026-07-26
**Status:** implemented decision and executable receipt. Answers **Decision 1** of the
[shared-engram commons brief](../research/2026-07-24_shared_engram_commons_brief.md),
which asked for "a written merge rule under which two offline members editing
the same container reconverge to one graph on sync, property-tested, before
any chat implementation slice". Decision 2 is answered by the
[follow-on plan](../../archive_docs/2026-08-06_completed_plans/2026-07-27_commons_authority_keys_consumers_plan.md) and
[Commons profile](../design/2026-07-27_commons_profile_v1.md), rather than by
this convergence plan. The tracked `commons-spine` receipt was promoted to
the workspace package at `crates/moot/commons` when Turnstone became its
intended place consumer. `commons-spine` remains a technical profile, not a
product name.

**Method, as the brief instructed:** check what `stickleback` already
inherits from p2panda before designing anything. That check is section 1. It
found both a useful substrate and two limits: p2panda gives stable per-author
order plus a deterministic cross-author tiebreak, but author-first sorting is
not causality; and chartulary's old edge counter collided across writers.
Both now have executable repairs.

## 1. What the substrate already gives (verified against the tree)

`stickleback`'s `process_batch_atomic` sorts every corpus by
`(header.verifying_key, admission.target.log_id, header.seq_num)` before
committing, then validates each operation against a virtual frontier that
includes earlier operations in the same batch. So the replication layer
already supplies:

- **a deterministic tiebreak across authors**, rooted in the author's
  cryptographic public key rather than a wall clock or arrival time;
- **per-author append-only logs** with backlink validation and a
  strictly-advancing retained frontier, so an author's own edits never
  reorder against each other;
- **idempotent, content-addressed insert**, so duplicate delivery over two
  lanes (gossip plus LogSync, or a native drop plus live sync) lands once;
- **prune and checkpoint law** already reasoned about for concurrent history
  truncation.

The tiebreak alone is not a merge order. Because author is the first sort key,
one writer would permanently outrank another: a lower-key writer could edit
after observing a higher-key writer's value and still lose to that older
value. The commons record therefore carries the exact per-author operation
frontier observed at authoring. Materialization topologically orders those
causal parents and uses the substrate tuple only among ready, concurrent
records. This remains smaller than a CRDT library or wall-clock protocol, but
it is application-level causality that p2panda does not supply on its own.

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

**Rule: edge identity becomes (writer, counter), unique by construction.**
The landed chartulary 0.2.0 shape:

```rust
pub struct EdgeId { pub writer: WriterId, pub counter: u64 }
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

**Rule: causally later wins; concurrent writes use the deterministic substrate
tiebreak.** Each signed commons record carries the exact operation heads the
writer observed. A deterministic topological fold preserves happens-before;
`(verifying_key, log_id, seq_num)` orders only incomparable records. This means
a member who edits a value after seeing it can actually replace it, regardless
of public-key rank. **And the granularity question is named rather than
absorbed:** the
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

**Rule: remove wins.** No tombstone is needed for remove-versus-connect:
remove-then-connect drops the dangling connect because its endpoint is absent;
connect-then-remove reaps the incident edge. Chosen to match the
destructive-operation reasoning already ruled in the murm/moot lane: a
deletion the user performed should not be undone by someone else's concurrent
edge. Add-wins is the defensible alternative and is rejected deliberately,
not overlooked.

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
- **M1. Writer-scoped `EdgeId`. DONE 2026-07-26, chartulary 0.2.0.**
  `EdgeId { writer: WriterId, counter: u64 }`, where `WriterId([u8; 32])` is
  opaque to chartulary and bound by the host to the replication identity.
  `GraphLog::for_writer` declares which replica this is; a log left at
  `WriterId::LOCAL` asserts it is the only writer, which every graph predating
  multi-writer containers was.
  - **The load-bearing half is the counter restore.** Replay advances
    `next_edge` only past ids **this** writer minted, so replaying a peer's
    journal cannot consume our range. Without that, the type change alone
    would still collide after a merge;
    `replaying_a_peers_edits_does_not_advance_our_counter` pins it.
    The post-M3 review caught the complementary restart case:
    `GraphLog::replay` began as `WriterId::LOCAL`, so setting the real writer
    only after replay lost that writer's consumed counter range.
    `replay_for_writer`, counter-rebuilding `for_writer`, and
    `load_full_for_writer` now restore it; the disconnected-edge case is
    pinned by `writer_scoped_counter_resumes_after_full_replay`.
  - **0.1.x data still loads.** A hand-written `Deserialize` reads both the
    bare integer 0.1.x wrote and the current struct, so an existing journal or
    snapshot returns as the single-writer graph it always was. Verified by
    `a_legacy_bare_counter_deserializes_as_a_local_id`. This needs a
    self-describing format; every chartulary store in the tree is JSON, and a
    postcard store would need its legacy data converted rather than sniffed.
  - Receipts: chartulary 44 -> 46 tests, plus scholia 88 and mere-eidetic 5
    green against the new type. The M0 receipts flipped from asserting the
    collision to asserting distinct ids and two addressable edges, the same
    tests now reading the fix.
- **M2. The two stated rules. DONE 2026-07-27, chartulary 50 tests.** Both
  hold already; the work was pinning them and finding where the phrasing did
  not reach.
  - **Remove wins over a concurrent connect, in either merge order.** Stronger
    than the plan claimed: it is commutative, so the result does not depend on
    which writer's key sorts first. Remove-then-connect drops the connect,
    since `Connect` only lands when both endpoints are present;
    connect-then-remove lands the edge and reaps it, since `RemoveNode` takes
    incident edges with it. No tombstone needed for this case.
  - **Whole-node LWW discards the loser's payload.** The test asserts the
    *losing* side is gone (a title erased by a concurrent tag write), not
    merely that both peers agree, so the coarseness is pinned rather than
    implied. M3 now makes "last" causal for observed writes and uses key order
    only for genuinely concurrent writes.
  - **Gap 3 is now settled at the Commons fold.** `InsertNode` remains an
    upsert in generic chartulary, where a concurrent insert sorted after a
    removal can resurrect the node. Commons uses the signed causal context to
    distinguish that case. A truly concurrent remove suppresses the insert;
    an insert causally after the remove recreates the node. A permanent
    tombstone would forbid that legitimate recreation. The local
    `commit_batch` scalar `expected` revision is not enough: two replicas can
    have the same journal length and different operation sets. M3's signed
    per-author operation frontier now records the exact causal context.
    Deliberate re-creation is therefore orderable after the observed delete.
    `concurrent_remove_wins_but_an_observing_insert_recreates` pins both sides
    of the rule.
  - **A test-model finding worth keeping.** The first draft merged two
    replicas by concatenating their full journals, which replays the shared
    history twice and lets the duplicate seed re-insert a node the other side
    had removed. The test failed and the code was right. A partition shares a
    common ancestor, so a merge is the shared prefix once plus each side's
    tail; `merge_divergent` models that and the naive `merge` is kept only
    for replicas with no shared history.
- **M3. Reconvergence over a real lane. DONE 2026-07-27**, as the tracked
  `commons-spine` package (`crates/moot/commons`, originally 17 test
  functions green, including one 48-case property test).

  The bullet originally claimed this "reuses the join ceremony, so this is a
  test rather than new machinery". **That was false**: no crate bridged
  chartulary to the replication layer, so a graph edit could not ride a lane
  at all. It is a probe rather than a founded crate because the spine gets
  named when a consumer pulls it, per the workspace's consumer-pull doctrine.
  - **Receipt**: two members edit one container **while partitioned**, each
    minting an edge before it can see the other, then reconverge over real
    p2panda LogSync on loopback. Both fold to four nodes and two edges, and
    **both edge ids stay separately addressable**, which is M1's fix proven
    over a lane rather than over a hand-built journal. Their exact graph
    fingerprints match. Bob then retracts Alice's synced edge, publishes the
    causally-later batch, and both reconverge with Bob's unrelated edge intact.
    Plus: reconstruction over one in-memory store and an actual Redb
    close/reopen both resume operation backlink/sequence and edge counter; a
    batch for another container is refused before mutation; signer/writer
    mismatch and counter reuse are refused; and arbitrary arrival permutations
    of one operation set property-test to one projection. A causal child
    arriving before its parent stores safely and is reported pending while
    unrelated causally complete state remains visible; it joins the projection
    when the parent arrives.
  - **The design finding: receipt must not apply edits.** The obvious `accept`
    closure folds each received batch into a live `GraphLog` on arrival. That
    does not converge, because a drain delivers in *arrival* order, which
    differs per peer. So receipt only **stores**, and the graph is a fold over
    the store. The first fold used author-first canonical order; review caught
    that this made public-key rank permanent write priority rather than LWW.
    Each `CommonsRecord` now signs the exact observed operation frontier.
    Materialization topologically orders that causal DAG, then applies
    `(verifying_key, log_id, seq_num)` only among concurrent ready records.
    This is the statement kernel brief's recorded-fact versus derived-state
    split landing in a second place: the log accumulates, the graph is
    recomputed.
  - **A bug this caught, worth keeping.** The first fold read the second
    element of `get_log_entries`' tuple as the payload. It is the **header**
    bytes, p2panda's LogSync convention; the batch rides in `op.body`. Because
    the fold used `.ok()` it swallowed every decode failure and produced an
    empty graph, which presented exactly like a sync failure and sent me
    looking at the lane. The fold now propagates decode errors, and the fix
    was found by asserting a replica could fold back *its own* edits before
    blaming the network. A fold that silently drops batches converges to a
    confidently wrong graph, which is worse than not converging.
  - The follow-on now adds authority without changing receipt: structural
    admission retains a valid fact, a Personae root/derived-key attestation
    binds its signer, and typed Servitor authority reclassifies it as
    effective, pending, or revoked during materialization.
- **M4. Write the limit down. DONE 2026-07-27**, in section 5 below. The
  [Commons profile](../design/2026-07-27_commons_profile_v1.md) now carries
  the user-visible merge, missing-history, limit, and durability contract.
  Section 5 remains the convergence decision's local rationale.

## 5. Stated behavior, for the commons profile

Written for the product surface, not the merge layer. Each of these is a
promise a member can rely on and a limit they can be surprised by, so they
belong in whatever documents a shared container before one ships.

- **A deletion holds against a concurrent edge.** If one member removes a node
  while another connects something to it, the node stays removed and the edge
  does not survive. This is true whichever member's change is seen first.
- **Two members editing one node do not merge; a causally later edit wins the
  whole node, and concurrent edits use a stable key tiebreak.** Concurrent
  edits to *unrelated* parts of the same node still cost one of them entirely.
  This is a known coarseness, not a bug, and it lifts when a facet write becomes
  its own edit (the one-node facets lane). Until then, a shared container wants
  either coarse ownership habits or content classes whose nodes are small.
- **A deletion holds against a concurrent edit to the same node.** A truly
  concurrent insert is suppressed. An insert that first observes the removal
  recreates the node.
- **Every member converges.** Whatever the rules resolve to, all members reach
  the same graph, because the order is computed from the operations themselves
  rather than from arrival time or a clock.

## 6. Non-goals

No CRDT library, wall clock, or operational transform. Cross-author causality
is the signed observed frontier; the existing deterministic tuple remains the
concurrent tiebreak. No per-facet merge until facet-grained edits exist, which
is the one-node facets lane's work and not this plan's. Group keys remain a
separate layer from convergence and are specified by the follow-on profile.
