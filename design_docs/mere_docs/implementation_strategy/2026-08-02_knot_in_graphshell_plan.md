# Knot in Graphshell Plan

**Date:** 2026-08-02
**Status:** K0-K3 complete. K1 chose Option A (Mark): shared documents are
projected, personal documents replicate, and T4's done condition is replaced
accordingly, closing the shared-Knot authority question as dissolved rather
than answered. K2's three clauses are proven against the real resident host,
and its physical two-machine receipt passed on 2026-08-08. K3 kept the spawn
path deliberately.

**Physical receipt (2026-08-08):** `ports/knot/examples/k2_peer.rs` held a real
vault on Q-PC and visited it from Windows over an explicit endpoint ticket.
The visitor was admitted, mounted `Knot`, saved, heard the unseen revision,
remained live, read back exactly what it wrote from Q-PC's file, closed, and
exited zero. Both native runners used the same source snapshot and lockfile.
See the
[K2 physical two-machine receipt](../../../ports/knot/docs/2026-08-08_k2_physical_two_machine_receipt.md).

The reverse Windows-holder to Q-PC-visitor direction timed out before
admission and left the file unchanged. That direction remains a reachability
defect, but K2's physical two-machine condition is met by the passing direction.
The route was ticketed; direct-versus-relay selection was not instrumented.

The rehearsal remains an example rather than a bin because Knot depends on
graphshell only as a dev-dependency, and examples get dev-dependencies where
bins do not, so the rehearsal costs the crate graph nothing.

```
cargo run -p knot --example k2_peer -- hold  --root <vault-dir>
cargo run -p knot --example k2_peer -- visit --peer <ticket>
```

Both devices set `K2_OWNER` and `K2_NETWORK` to the same value and `K2_SEED` to
different ones. The Knot search section below is independent of K0-K3 and does
not block archiving of the hosting work.
**Related:** the
[carrier seam plan](../../archive_docs/2026-08-06_completed_plans/2026-08-01_graphshell_carrier_seam_plan.md), which this
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

**K0. Host `KnotEndpoint` in-process** — landed 2026-08-02 for the plain
directory mode. No new Knot code, exactly as predicted: `KnotEndpoint` already
implements every trait `LocalCarrier` needs, including
`ProjectionNoticeSource`.

`RetainedEndpointSession::over(Box<dyn Carrier>, profile)` is the general form
`spawn` is now one case of, and `KnotHub::host` is the in-process peer of
`KnotHub::connect`. Past construction the two are identical — `run_hub` never
learns which it got, which is the seam doing its job.

`from_env` hosts by default and spawns only when `TURNSTONE_KNOT_ENDPOINT`
explicitly names a program.

**Both directory modes host**, with and without effects. The effects grant
needed no change in Knot at all: `KnotEffectPolicy`, `KnotEffectAuthority`,
and `KnotWriteGrant` are already exported, so the host builds the policy
directly. Hosting also drops a round trip the spawned path pays — effect
settings travel as CLI strings the endpoint binary reparses, while a hosted
endpoint receives them typed on both sides of a call that no longer crosses a
process.

**Still spawning: the persona-vault modes.** Their endpoint unlocks through a
device identity that `KnotAuthoringEngine::from_env` never receives. Hosting
them needs an identity threaded into that constructor, which is a signature
change rather than more wiring at the branch — Turnstone has the identity, it
simply is not passed here. Unfinished, not blocked, and the shape of the fix
is known.

Note the root is still `TURNSTONE_KNOT_ROOT`, and correctly so — it names
*which directory is your vault*, which is real configuration rather than
deployment accident. What the env var no longer decides is whether a
subprocess exists.

**K1. ~~Decide A or B~~ DONE 2026-08-02: Option A.** T4's done condition is
replaced by the one stated above, and the shared-Knot authority question is
closed as dissolved. K2 is unblocked.

**K2. Project a place-held document.** Under A: the holder's mere serves the
document to place members over the endpoint, negotiating `EditableText`.
Authority is the holder's, so no new admission model appears.

**~~Blocked on the carrier plan's C3~~ UNBLOCKED 2026-08-06.** C3 landed: a
place member can now reach the holder's endpoint over the transport, and the
served session can ring when the document moves. What was blocked was reach,
and reach exists.

The block, as recorded on 2026-08-02: only two carriers existed, stdio and
local, and both only reached this machine. A place member reaching the
holder's endpoint is remote projection by definition.

One thing C3 changed about K2's shape rather than merely unblocking it. The
notice lane did not exist on an admitted session until C3 added it, so
"concurrent editing" was going to arrive as polling whether or not anyone
chose that. It now arrives as a bell, which is what makes the done condition
below reachable without a client asking on a timer.

Two things this does **not** mean, both easy to misread:

- **Not an authority problem.** Option A already settled who may write.
- **Not a connectivity problem.** Machines already reach each other: tickets
  are proven in the place port's T3a receipt, mDNS exists, and relays work —
  relay was the first path that connected the ThinkPad and the iMac. NAT
  traversal, discovery, and peer identity are done.

What is missing is only that the graphshell protocol has never been spoken
over one of those connections. C3 is an ALPN and a framing loop, both with
templates on either side. This is a queue-order fact, not a setback, and not
a networking project.

Done when two peers edit one document concurrently and the holder's projection
is what both see.

**The holder's serving half landed 2026-08-06.**
`native::projection_host::ResidentProjectionHost` accepts over the transport
and serves a catalog route, which is what `browser_host` already did for a
browser and no one did for a network peer. Before this, the only accept loops
in the tree were inside receipt binaries, exactly as the dial half was before
C3.

**Concurrency is the load-bearing part, not an optimisation.** A sequential
accept loop can serve a place member but not two: the second visitor waits at
the door until the first leaves, which is the one thing a shared document
cannot do. Each admitted session is spawned, and the host returns to accepting
immediately. Live sessions are counted so the policy's `max_sessions` is
checked against something real, and a slot is released however a session ends.

**Each session opens its own endpoint**, because the catalog is a factory
rather than a registry of live objects. Two visitors to one Knot vault hold
two `KnotEndpoint`s over the same files and converge through the holder's own
truth. That is Option A working as designed rather than a limitation to route
around.

**A finding that cost a red test, and is worth keeping.** `projection_session`
documents itself as making "two admissions never the same session", derived
from the transcript-bound `session_id`. That is only true because the client
mints a fresh nonce: `session_id` is `blake3(transcript)` and the nonce is in
the transcript, and the responder does not check nonce freshness. Two sessions
from one subject reusing a nonce receive the *same* projection session id.

Not an authority hole, because the transcript binds the subject and a peer can
only collide with itself. It does mean a client mounting two projections in
one `ClientState` must mint per-session randomness or the second silently
overwrites the first. The guarantee belongs to the client, and the doc comment
should say so rather than claiming the responder provides it.

**K2 IS DONE 2026-08-06.** All three clauses of the revised done condition
are proven in `ports/knot/tests/place_projection.rs`, against the real
`ResidentProjectionHost` and its catalog rather than a hand-rolled stand-in.
Two peers with distinct subjects and distinct grants are admitted over a
transport and each mounts a real `KnotEndpoint` over one holder's directory.
Ada saves; the holder writes the file; Bo, who asked for nothing, hears the
bell, resumes, and reads Ada's text; and the holder's own file on disk is what
both were reading. A second test takes the holder away mid-session and finds
the visitor told the document is unavailable, with the scene kept so a host can
still show what was there and no longer offer a save that cannot land.

**K2 PHYSICAL RECEIPT PASSED 2026-08-08.** Q-PC held the real vault and a
Windows visitor completed admission, mount, save, revision bell, live readback,
and close over the ticketed p2panda carrier. The holder's file hash matched the
captured Windows copy. See the
[physical receipt](../../../ports/knot/docs/2026-08-08_k2_physical_two_machine_receipt.md).

Three decisions were taken along the way, each forced by a compiler or a red
test rather than chosen in the abstract.

**`KnotEndpoint` is now `Send`, by enabling rhai's `sync` feature.** The
catalog requires `Send` because the host spawns each session, and the endpoint
was not, for exactly one reason: `KnotEffectAuthority` holds `BlockEvaluators`,
a map of `Box<dyn BlockEvaluator>`. The trait is genet's and has exactly one
implementor anywhere, mere's own `RhaiEvaluator` over a `rhai::Engine`.

The hidden cost turned out to be small and worth naming precisely. `sync`
makes rhai's `Shared` an `Arc` instead of an `Rc` and its `Locked` an `RwLock`
instead of a `RefCell`, so the cost is atomic refcounting inside script
evaluation. It also requires every registered function and custom type to be
`Send + Sync`, which costs us nothing: `base_engine` registers two non-capturing
closures and no custom types at all. Evaluation is an on-demand knot fence
under an operation budget, not a hot path. The other rhai consumer in the
workspace, `quint`, has its dependency behind a `field-rhai` feature that
nothing enables, so nothing else is dragged along today.

`BlockEvaluator: Send` is now stated in genet, where the constraint belongs: a
thread-hostile language runtime should say so by not implementing the trait,
rather than leaving every endpoint above it unschedulable for reasons invisible
from there. `ports/knot/tests/send_probe.rs` asserts the result, so breaking any
link in that two-repo chain names the type that changed instead of surfacing as
a confusing trait error at a registration in another crate.

**The carrier now distinguishes a refusal from a disconnection.**
`Carrier::request` returned `Result<_, String>`, and an endpoint that said no
was indistinguishable from a link that died. `CarrierError::Refused` against
`CarrierError::Disconnected` replaces it across the protocol crate and all
three carriers. `RetainedEndpointSession` keeps its `Result<_, String>` public
surface, so no consumer signature changed; internally it now observes every
carrier outcome and marks its mounted scenes disconnected when the session is
gone. The scene is kept rather than dropped, so a host can still show what was
there while no longer offering a save that cannot land.

**The catalog gained `register_resumable_notifying`.** Its typed registrations
answered resume with a refusal, so Knot, which is resumable, was told it could
not do the thing it demonstrably can, and a visitor recovering after a bell
failed. The escape hatch `register_erased` anticipated this in its doc comment;
what was missing was the ordinary case, since a product endpoint over durable
source is generally both notifying and resumable.


**K3. ~~Retire the spawn path or keep it deliberately~~ DECIDED 2026-08-06:
kept, deliberately, as a deployment option rather than the default.**

The evidence the step asked for, now that hosting and the network carrier both
exist:

- **Hosting is already the default and spawning is already the exception.**
  Turnstone hosts every Knot mode in-process, and `TURNSTONE_KNOT_ENDPOINT` is
  an explicit opt-out for the case where the endpoint genuinely is a separate
  program. Nothing has to change for that to be true; K0 made it so.
- **Spawning is no longer how anything reaches elsewhere.** That was the stdio
  carrier's accidental second job, and C3 took it back. A remote endpoint is
  now reached by dialling it.
- **It still has live consumers**, which is what distinguishes a decision from
  attrition: `ports/knot/tests/revision_bell.rs` drives a real endpoint
  process, and the G4 receipt harness in `ports/graphshell/src/sessions.rs`
  mounts endpoint programs through `spawn_endpoint_session`.

The reason to keep it is better than "it is still used", though, and worth
stating because it is the thing that would be lost:

**Stdio is the carrier that proves the protocol does not secretly depend on
shared memory.** It is the only one with a real process boundary, so it is the
only one that can catch a message type that stopped round-tripping, or an
endpoint that quietly began relying on state its client happened to share.
`graphshell-local` serializes deliberately for the same reason, but it
serializes *by choice* and could be changed by accident; stdio cannot cheat,
because there is a pipe in the way. Deleting it would leave that guarantee
resting entirely on a decision someone could reverse without noticing.

Second, it is the only deployment for an endpoint that *is* a separate program:
one shipped by someone else, or one a host wants isolated in its own process
rather than linked into itself. The participant gate's untrusted cases are the
obvious future consumer, and the cost of keeping the option open is 684 stable,
tested lines.

So: not the default, not deprecated, and not on a path to deletion. A
deployment option with two jobs it is uniquely good at.

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
