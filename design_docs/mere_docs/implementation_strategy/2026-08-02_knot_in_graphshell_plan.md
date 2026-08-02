# Knot in Graphshell Plan

**Date:** 2026-08-02
**Status:** scoped; **K1 decided 2026-08-02 in favour of Option A** (Mark).
Shared documents are projected, personal documents replicate. T4's done
condition is replaced accordingly, and the shared-Knot authority question it
recorded is closed as dissolved rather than answered.
**Related:** the
[carrier seam plan](./2026-08-01_graphshell_carrier_seam_plan.md), which this
converges with — see "Why these are one move".

## The finding

`ports/knot/src/endpoint.rs` opens with "Graphshell disclosure for Knot
directory state" and `KnotEndpoint` implements `ProjectionSource`,
`ResumableProjectionSource`, and `IntentSink`. **Knot is already a graphshell
endpoint.** It has been one for as long as that file has existed.

What is not graphshell-shaped is the *deployment*: `bin/knot_endpoint.rs` wraps
it as a process, Turnstone spawns that process, and the two talk over the
stdio carrier. So "put Knot in graphshell" is not a port, a rewrite, or a
re-architecture. It is removing a process boundary from an adapter that already
speaks the protocol on both sides of it.

## What moves, and what does not

Nothing moves crate-to-crate. The split is host versus domain, and only the
host half changes shape.

| Module | Lines | Disposition |
|---|---|---|
| `endpoint.rs` | 2968 | **Already the adapter.** Unchanged; gains an in-process host. |
| `bin/knot_endpoint.rs` | 570 | The process wrapper. Becomes one deployment of the adapter, not the only one. |
| `editor.rs` | 214 | Domain. Cambium-backed source editing; unchanged. |
| `sync.rs` | 2248 | Domain. Personal replication; unchanged. |
| `resident.rs` + `bin/knot_sync_host.rs` | 419 | Domain. Always-on personal sync; unchanged, and see "What projection does not replace". |
| `vault.rs`, `writer.rs`, `watcher.rs` | 956 | Domain. Unchanged. |
| `directory.rs`, `search.rs`, `settings.rs`, `startup.rs`, `content_classes.rs` | ~1100 | Domain. Unchanged. |

`ports/knot` keeps its bin and stays runnable alone. A port being a thin shell
over an embeddable adapter is the intended shape, not a compromise.

## Why these are one move with the carrier plan

Turnstone embedding Knot means Turnstone hosting a `KnotEndpoint` in-process
and talking to it without a subprocess. That is exactly the carrier plan's C1,
the in-memory carrier, reached from the other direction.

So the two plans have one implementation between them:

- Carrier C0 extracts the `Carrier` trait.
- Carrier C1 adds the in-memory carrier.
- **Knot in graphshell is C1's first consumer**, and the thing that proves it.

`graphshell-web` already demonstrates the endgame from a third direction: it
implements `IntentSink` and `ProjectionSource` directly in a wasm `cdylib`,
with no carrier at all. Three routes to the same place; the risk of leaving
them uncoordinated is three private ways of not having a carrier.

## What this decides for T4

T4's done condition currently reads: *both peers author offline revisions and
reopen the same derived document after convergence.*

**Projection cannot satisfy that, by definition.** A projected surface has no
local replica, so there is nothing to author into while disconnected. The
condition assumes replication.

The choice, stated plainly because it is a product decision and not a
technical one:

### Option A — CHOSEN 2026-08-02: shared documents are projected

A document belongs to the mere that holds it. Peers in a place edit it live
through projection, sending intents; the holder remains the single authority.
Disconnected, a visitor cannot edit it, because they never had it.

- **Dissolves the open authority question.** There is no shared Knot space to
  admit anyone into, so nothing has to agree with Gemot. The question recorded
  under T4 stops needing an answer rather than getting one.
- Matches how collaborative editors generally behave.
- Makes the personal/placed split load-bearing instead of incidental: your own
  documents replicate to your own devices and work offline; documents in a
  place you visit are live-edited while you are there.
- **Costs:** no offline co-authoring, and a document is unavailable when its
  holder is. Both are honest consequences of one authority, not bugs.

Revised done condition: *two peers edit the same document concurrently through
projection, the holder's revisions remain authoritative, and a disconnected
visitor is told the document is unavailable rather than shown a stale copy it
cannot save.*

### Option B — not chosen: shared documents replicate too

Kept offline co-authoring, and kept the authority question with it: a
place-scoped admission model for the shared Knot space, and a second fold that
must agree with Gemot about membership.

Recorded rather than deleted, because the thing that would reopen it is
specific and worth naming: **a requirement for offline co-authoring in a
shared place.** Wanting a document to stay readable while its holder is away
is not that requirement, and is better served by an explicit copy-into-my-vault
gesture than by making every shared document a replica.

## What projection does not replace

`KnotSyncHost` and `sync.rs` keep their job under either option. Personal
multi-device replication is a different problem from shared editing: you want
your own documents on your own laptop offline, and `paired_writers` is the
correct admission model for that, because the devices really are yours.

The earlier reading that `paired_writers` was "the wrong authority" was wrong.
It is the right authority for the question it answers. The mistake was
assuming it also had to answer the shared-place question.

## Steps

**K0. Host `KnotEndpoint` in-process.** Behind the carrier plan's C1. No new
Knot code; Turnstone stops spawning and starts hosting.

Done when a Knot document opens with no subprocess and no
`TURNSTONE_KNOT_ROOT`, through the same protocol messages the stdio carrier
sends today.

**K1. ~~Decide A or B~~ DONE 2026-08-02: Option A.** T4's done condition is
replaced by the one stated above, and the shared-Knot authority question is
closed as dissolved. K2 is unblocked.

**K2. Project a place-held document.** Under A: the holder's mere serves the
document to place members over the endpoint, negotiating `EditableText`.
Authority is the holder's, so no new admission model appears.

Done when two peers edit one document concurrently and the holder's projection
is what both see.

**K3. Retire the spawn path or keep it deliberately.** If the remote case has
no live consumer, the stdio carrier and `bin/knot_endpoint.rs` are a
deployment option rather than the default. Decide with evidence, not by
attrition.

## Knot search on the hybrid seam

**Approved 2026-08-02 (Mark).** Independent of hosting: it is about what Knot
recalls, not where Knot runs, and neither step below blocks or is blocked by
K0-K3.

### The actual defect

Knot's `search.rs` calls `sibylla::SemanticSearch` with a
`LexicalEmbeddingProvider`, which is hash-bucket embeddings: bag-of-words
hashed into a vector of a configured dimension. So Knot has **one** recall
engine wearing lexical clothing, not two.

That matters more than the quality of either. `eidetic_search::fuse` exists
precisely to combine two rankings, and with one engine there is nothing to
fuse. Knot is not off the seam by oversight; it has never had the second
input the seam requires.

### What is reusable, and what is not

- `fuse(lexical, vector, k, weights) -> Vec<FusedHit>` is a **pure function
  over two rankings of strings**, engine-agnostic by construction and reusable
  as-is. Reciprocal-rank fusion, so the two engines' incomparable score scales
  never meet, and the weights are a setting rather than a constant.
- `TrailIndex` is **not** reusable: it is tantivy over `BrowsingTrace` events
  and imports `eidetic::browsing::BrowsingTrace`. A different corpus, not a
  configurable one.

So the seam is free and the lexical input is the work.

### Steps

**S0. A real lexical index over Knot documents.** BM25, tantivy, keyed by
something stable enough to fuse on. `fuse` keys on `String`, and Knot results
already carry a `SearchLane` (`Disk` or `Vault`), so the key must distinguish a
file-in-place from a vault document rather than collapsing them.

**Decision inside S0:** does the tantivy machinery generalise out of
`TrailIndex` into a corpus-agnostic index that both use, or does Knot get its
own index type? Generalising is the better instinct if the schemas are close;
it is worse if it contorts the trail index to serve a second shape. Read both
schemas before choosing, and put the answer wherever it lands **in
`eidetic-search`, not in `ports/knot`** — search machinery belongs in the
search crate. Note that doing so widens that crate's stated charter beyond
"eidetic browsing memory", which is a rename or a charter edit, not a silent
drift.

**S1. Fuse.** Knot's search returns `fuse`'s output over its lexical and
semantic rankings, with `k` and the weights exposed as settings per the
configurability rule.

Done when a Knot query returns hits neither engine ranked first alone, and the
weights demonstrably move the ranking.

### Not part of this

Replacing `LexicalEmbeddingProvider` with a real embedding provider. Sibylla
has a BERT provider; whether Knot's semantic side uses it is a separate
quality question, and hash-bucket vectors remain a legitimate cheap default
for the vector input once a real lexical input exists beside them.

## Not in scope

- **Wasm.** Same reasoning as the carrier plan: the argument is portability,
  not sandboxing, because Knot is first-party.
- **Wasm.** Same reasoning as the carrier plan: the argument is portability,
  not sandboxing, because Knot is first-party.
- **The DCGKA carrier**, still open under the place port's T3b and untouched
  by any of this.
