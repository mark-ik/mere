# Knot in Graphshell Plan

**Date:** 2026-08-02
**Status:** scoped, not started. Supersedes nothing; it decides what
[T4](../../../../turnstone/design_docs/2026-07-28_turnstone_place_port_plan.md)
means and unblocks it.
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

### Option A (recommended): shared documents are projected

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

### Option B: shared documents replicate too

Keeps offline co-authoring. Requires the shared Knot space to have
place-scoped admission, which is the question T4 currently records as open,
and a second fold that must agree with Gemot about membership.

Strictly more capable and strictly more expensive. Choose it only if offline
co-authoring in a shared place is a requirement rather than a nicety.

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

**K1. Decide A or B**, and rewrite T4's done condition to match. This is the
gate; K2 cannot start without it.

**K2. Project a place-held document.** Under A: the holder's mere serves the
document to place members over the endpoint, negotiating `EditableText`.
Authority is the holder's, so no new admission model appears.

Done when two peers edit one document concurrently and the holder's projection
is what both see.

**K3. Retire the spawn path or keep it deliberately.** If the remote case has
no live consumer, the stdio carrier and `bin/knot_endpoint.rs` are a
deployment option rather than the default. Decide with evidence, not by
attrition.

## Not in scope

- **Search.** Knot's `search.rs` drives `sibylla::SemanticSearch` with a
  `LexicalEmbeddingProvider`, while `eidetic-search` is real tantivy BM25 with
  an explicit engine-agnostic hybrid-fusion seam. Knot approximating lexical
  search with an embedding stand-in, beside a fusion seam built for exactly
  this pairing, is worth its own pass. Unrelated to where Knot is hosted.
- **Wasm.** Same reasoning as the carrier plan: the argument is portability,
  not sandboxing, because Knot is first-party.
- **The DCGKA carrier**, still open under the place port's T3b and untouched
  by any of this.
