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

### Option B — not chosen, and much cheaper than first recorded

Keeps offline co-authoring, at the cost of a shared Knot space with
place-scoped admission.

**Cost corrected 2026-08-02 after reading `sync.rs`.** Two costs originally
recorded against this option do not exist, and the correction matters because
a wrong estimate in a plan is worse than none.

- **"It needs group keys, which are blocked on the DCGKA carrier."** Wrong.
  `KnotSyncCipher::CommonsData(&DataKeyring)` and
  `KnotEncryptionProfile::CommonsDataV1` are implemented, with a communal
  variant of every operation — `author_communal`, `communal_projection`,
  `resolve_communal_conflict`, `save_communal_checkpoint`,
  `communal_epoch_pruning_proposal`, `restore_communal_keyring`. Tested:
  `commons_documents_use_group_epochs_instead_of_personal_vault_keys`, plus
  two communal epoch-pruning receipts. Knot already speaks the Stickleback
  group keyring that Commons chat uses.

- **"Knot models multi-writer as a conflict, not a merge."** Wrong, and this
  was the claim that made the option look expensive.
  `independent_text_edits_merge_and_a_later_put_makes_them_durable` says
  independent edits **merge**. Only `overlapping_text_edits_remain_an_explicit_conflict`,
  and concurrent whole-document puts, become conflicts. Resolution is an
  authored, replicated `KnotSyncEvent::Resolve { id, supersedes, document }`,
  guarded by `a_resolution_does_not_erase_an_unseen_concurrent_version` and
  `a_resolution_cannot_name_a_version_outside_its_causal_history`.

  That is a proper collaborative document model: merge what is independent,
  surface what genuinely collides, and make the choice explicit and
  attributable rather than silently picking a winner. For prose that is
  arguably better than opaque CRDT convergence.

**What actually remains.** One thing: admission is a static allowlist.
`KnotSyncStore` takes `writers: BTreeSet<[u8; 32]>` at construction and
rejects anything else as `unrecognized-knot-writer`. A place's membership
changes as the Moot changes, so the work is replacing that allowlist with a
Gemot capability query — exactly what Moots already answer for Commons graph
and chat, over a Knot lane in the place's lane set, `lane_id` and per-lane
ALPN like every other.

**So the reopening bar is low, and the two compose.** Projection stays the
default for visiting a document; a Moot-scoped Knot lane is a small increment
whenever offline co-authoring in a place is actually wanted. Choosing A did
not close B; it chose which one is the default.

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
