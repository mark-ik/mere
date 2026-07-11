# Murm/Moot Sibling Posture Plan

**Status**: Active plan.
**Date**: 2026-07-08.
**Scope**: Reorganize the comms families so `murm` and `moot` sit as sibling service families (the netfetcher/netrender posture), consolidate the triplicated log-sync substrate by aligning on upstream p2panda, define the store substrate over muniment (desktop redb, wasm IndexedDB + OPFS), execute the naming payload (gazette, cabal, gerund law), and gate promotion to standalone repos on a purity check.
**Related**:

- [`../../murm_docs/technical_architecture/MURM_AS_BILATERAL.md`](../../murm_docs/technical_architecture/MURM_AS_BILATERAL.md) (role spec; the charter this plan narrows and sharpens)
- [`../research/2026-05-31_murm_p2p_landscape_brief.md`](../research/2026-05-31_murm_p2p_landscape_brief.md) (landscape; p2panda adopt call)
- [`2026-07-06_comms_gating_and_key_addressing_plan.md`](2026-07-06_comms_gating_and_key_addressing_plan.md) (comms G-phases; G1-G4 hold on the one-state migration)
- `repos/muniment` (the Backend seam this plan's store substrate rides)
- [`2026-07-08_generic_graph_substrate_plan.md`](2026-07-08_generic_graph_substrate_plan.md) (chartulary precedent: fresh core proven standalone, mere re-bases last)

---

## 1. Decision record

Settled in the 2026-07-08 session, in force for this plan:

1. **Sibling posture.** `murm` and `moot` are peer service families, shaped like netfetcher and netrender relative to serval: murm talks to remote endpoints (iroh, p2panda-net, the sync pump), moot materializes the durable artifact (log grammar, stores, folds, tiers). Neither depends on the other. The host composes them, exactly as serval composes fetch and render.
2. **The shared substrate is upstream p2panda.** No new neutral crate. The triplicated glue (murm `gossip_sync`, tessera `sync`, mesh sync) retires into upstream reliance plus one pump. Every crate takes `p2panda-store` with `default-features = false` (traits only); murmuring already does.
3. **One pump, folds stay home.** The LogSync/gossip drive loop lives once in the murm family, generic over the p2panda store traits, endpoint injected. Each domain keeps its own `Ext`, wire bridge, and fold (PostKind/channel_history, TesseraEvent/fold_moot, MeshEvent/JobBoard).
4. **Stores ride muniment.** The p2panda store traits get one shared implementation over muniment's Backend seam, living in the moot family (logs are press-side). murm never depends on moot: murm stays generic over the traits and the **host injects** the concrete store, same as it injects the endpoint.
5. **sqlx demotes.** The `sqlite` feature of p2panda-store pulls sqlx + libsqlite3-sys (a C dependency plus the sqlx driver family) into the tree. Target state removes it. This is a dependency removal and waits on Mark's sign-off (see open questions).
6. **wasm store splits by physics.** Records (operations, log index, topics) go to IndexedDB: transactions and ordered cursors are native, multi-tab isolation is built in. Blobs go to OPFS: iroh-blobs-shaped content needs ranged reads, which IndexedDB structurally cannot do. The worker requirement stays quarantined to the blob path.
7. **Naming payload.** gazette relocates to the persona tier as `gazetteer`; the `cabal` noun family migrates toward `murmur` by edge-to-link discipline (product copy first, identifiers opportunistically); the gerund law (murmuring:murm :: mooting:moothold) is codified in TERMINOLOGY.md.
8. **Promotion is gated, then follows the ecosystem playbook.** Standalone repos only after the purity check passes. MIT OR Apache-2.0, edition 2024, MPL per-file headers stripped on the way out (accretion, not Servo derivation). Consumption via git `branch=` deps, gitignored `.cargo/config.toml` local overrides, Cargo.lock gitignored, mere re-bases last.

## 2. Current state (receipts)

- The murm trio is live and tested: murmuring ~3,600 LOC / 76 tests (posts are signed BLAKE3/CBOR p2panda Operations), transport ~2,600 LOC / 31 tests (P2pandaTransport over iroh QUIC + blobs + gossip; reticulum feature default-off), murm ~1,350 LOC / 15 tests (SyncedCabal runs gossip + LogSync). `meerkat/src/comms_host.rs` wires a networked cabal into the docked pane: ticket, connect, live drain.
- The substrate is triplicated: `murm/murm/src/gossip_sync.rs`, `moothold/src/tessera/sync.rs`, and mesh carry near-identical pump loops (same `now_ms`, same 30x100ms quiet>=3 resync, same drain arms); murm and tessera duplicate a redb LogStore/TopicStore; tessera and mesh duplicate the Event/Operation bridge. tessera's module docs cite murm as the template. The moot-object lane would be copy four.
- mesh proved upstream sufficiency: p2panda-store's SqliteStore satisfies the LogSync bounds with zero custom code. The cost, visible in Cargo.lock: sqlx-core/macros/sqlite plus libsqlite3-sys.
- p2panda-store 0.6.1 traits are native `impl Future` with **no Send bounds** (verified in traits source). muniment's Backend is `#[async_trait(?Send)]` by design. The two are compatible as-is; Send pressure exists only at desktop spawn sites, and the pump does not run on wasm.
- gazette is an orphan: workspace member, absent from `[workspace.dependencies]`, zero `.rs` consumers, blocking reqwest, WebFinger only despite a multi-resolver charter.
- All three main READMEs are stale (BLAKE2b/varint/IrohTransport/snapshot-push against a BLAKE3/CBOR/P2pandaTransport/send-subscribe reality). Inline module docs are accurate.

## 3. Target architecture

| serval family | comms family | owns | never owns |
|---|---|---|---|
| netfetcher | **murm** | transport (iroh + p2panda-net), the sync pump, gossip lanes, tickets/blobs, chat domain (murmuring: PostKind, fold, MurmurExt), Murm facade | a store of record |
| netrender | **moot** | log grammar + the muniment-backed p2panda store adapter (mooting), tessera events + folds, tiers/federation (moothold) | a socket |
| serval (host) | meerkat comms host | composition: constructs backends, injects endpoint + store into the pump, stitches adapters | protocol logic |

Dependency rules (the purity contract):

- moot family builds with no `iroh` and no `p2panda-net` in its tree.
- murm family builds with no concrete store-of-record; `p2panda-store` traits only, in-memory for tests.
- both may depend on `p2panda-core`, `p2panda-store` (traits), `personae`, and (moot only, for the adapter) `muniment`.
- `mesh` stays mere-side glue, composed by the host like everything else.
- the only place murm's pump meets moot's store is a host construction site.

## 4. Phases

### Phase R: in-workspace re-sort

- **R0 doc hygiene.** Rewrite the murm/murmuring/transport READMEs from the accurate inline docs. Done when: no README claims BLAKE2b, varint wire, IrohTransport, or snapshot-push.
- **R1 pump consolidation (murm + mesh).** Extract the shared drain half into `transport::SyncedSpace` via a `drive(subscription, accept)` seam: the caller keeps its own `LogSync` session + `SyncHandle` (for liveness and any live publish) and hands `SyncedSpace` the subscribed stream plus an async `accept` closure (`FnMut(Operation<E>) -> impl Future<Output = bool>`) that absorbs the per-consumer verify + insert (sync redb or async sqlite). `SyncedSpace` owns the drain loop, the `SyncStatus`/`SyncRound` counters, `resync`, and the drop-aborts-the-task lifetime — the ~90 near-identical lines. Migrate murm's `gossip_sync` and mesh's `sync` onto it and delete their copies. **tessera is excluded here**: its pump cannot be deduped without removing p2panda-net from moot, which is R2's purity operation. Chose the `drive` seam over a store-generic `join<S>` because it needs only `E: Extensions` (no `LogStore`/`TopicStore`/`LogId` bounds, no `p2panda-store` dep in transport) while still deduping the meaty drain. Done when: `SyncedSpace` is the one drain impl, murm + mesh consume it, and the two-peer convergence tests pass in murm and mesh.
- **R2 purity gate (+ tessera pump move).** Move tessera off its in-crate `SyncedMoot`: moot provides `TesseraStore` + `verify` + `fold_moot`, and the **host** drives the pump over `transport::SyncedSpace` (the "only place murm's pump meets moot's store is a host construction site" rule). Then drop `p2panda-net` (and the iroh it pulls) from moothold. Enforce the §3 dependency rules. Done when: `cargo tree` for the moot family shows no iroh/p2panda-net, murm-family shows no redb/muniment, tessera's two-peer convergence test passes from its new (host-side) home, and the meerkat comms slice still converges two peers at runtime.

### Phase S: store substrate

The bar is three traits; murm's hand-rolled `PersistentCabalStore` is the living reference for the exact bounds LogSync needs. The `Transaction` trait and `_tx` variants are not required (murm ships without them); do not gold-plate.

| p2panda-store trait | needs | muniment primitive |
|---|---|---|
| `OperationStore` | insert/get/has/delete by hash; `delete_operation_payload` | BlobStore; store header and payload under separate keys so payload prune keeps the header |
| `LogStore` | latest entry, heights, size, `get_log_entries` range, `prune_entries`, per (author, log-id) | ordered scan over encoded keys (the seam gap) |
| `TopicStore` | associate/remove/resolve | SlotStore |

- **S1 muniment seam growth.** Add to `Backend`: `scan(prefix, range)` returning lexicographically ordered keys (default impl: `list` + sort; real backends override), and `apply(batch)` for multi-key atomic commit (default impl: sequential puts with the blob-then-index recovery discipline documented; transactional backends override). Key schema (illustrative only, not compile-ready): `op/<hash>` header, `op/<hash>/payload` body, `log/<pubkey>/<logid>/<seq as 016x>` index entries, `topic/<topic>` maps. Done when: both methods exist with defaults, MemoryBackend passes an ordered-scan and an atomic-batch test, and a batch-spanning-await regression test exists.
- **S2 shared adapter.** Implement the three traits once over muniment stores, in the moot family (mooting). Retire the hand-rolled redb LogStore/TopicStore in murm and tessera. Done when: murm's cable engine, tessera, and mesh two-peer LogSync tests pass on the muniment-backed store over MemoryBackend.
- **S3 desktop backend.** A redb `Backend` in muniment (feature-gated). Done when: the meerkat comms slice runs end to end on muniment-redb, two real peers converging.
- **S4 wasm backends.** `IdbBackend` (records: real IDB transactions for `apply`, cursors for `scan`) and `OpfsBackend` (blobs: worker-hosted sync access handles for ranged reads, `createWritable` fallback on the main thread). Done when: the batch-spanning-await test passes on IDB (transactions auto-commit on event-loop yield; all requests of a batch must issue in one tick), a two-tab test shows IDB isolation plus Web Locks guarding OPFS blob writes (content-addressed writes are idempotent; the losing tab skips), a ranged blob read is exercised, and `navigator.storage.persist()` is requested at init with its outcome surfaced honestly in the UI (no placebo status).

wasm scope note: S4 is store-only. Whether iroh/p2panda-net run in the browser is an unverified separate probe; nothing here assumes browser sync. A PWA reading its logs offline needs the store regardless. The higher-ceiling OPFS append-only log store remains available later as a backend swap under the same adapter, priced at hand-rolled crash-consistency testing.

### Phase N: naming payload

- **N1 gazette.** Relocate to `crates/persona/gazetteer`: rename (a gazetteer is an index; a gazette is a newspaper), add to `[workspace.dependencies]`, label incubating (WebFinger only today; blocking reqwest needs an async port before real consumers). Zero consumers, so the move breaks nothing. Done when: workspace builds and the crate is depend-able.
- **N2 cabal to murmur.** Product and UI copy adopt "murmur" now; code identifiers (open_cabal, CabalKey/Id/Handle, SyncedCabal, CabalExt) migrate opportunistically, never as a breaking sweep of the live slice. Done when: no user-facing surface says cabal, and TERMINOLOGY.md records the identifier migration as open.
- **N3 gerund law.** Codify murmuring:murm :: mooting:moothold (gerund names the plumbing, singular names the user artifact) in TERMINOLOGY.md, noting that the substrate consolidation is what keeps murmuring honestly chat-scoped.

### Phase P: promotion to standalone repos

Gate: R1 and S2 landed, R2 purity check green. Do not found repos around the pre-consolidation shape.

- **P1 names.** `murm`/`murmuring`/`moothold`/`mooting` are reserved on crates.io (2026-05-04). The bare `moot` crate name was taken; the repo can still be `repos/moot`. Confirm before founding.
- **P2 founding.** Fresh repos `repos/murm` and `repos/moot`; MIT OR Apache-2.0, edition 2024; strip MPL per-file headers (their MPL is workspace accretion, these are not Servo-derived; do not cargo-cult netrender's license); port cores with tests; proposal doc in each repo.
- **P3 consumption posture.** mere pulls both via git deps on the mark-ik remotes tracked by `branch=`; workspace pins at the root; machine-local path overrides via the gitignored `.cargo/config.toml`; Cargo.lock gitignored; no local paths in committed manifests.
- **P4 mere re-bases last**, per the chartulary discipline. Done when: a fresh clone of each repo builds and tests standalone, and mere builds green on the git deps.

## 5. Deferred, with owners named

- **Mail convergence** (one native sealed mail object, two delivery modes; misfin / LXMF-via-retinue / nostr as edge bridges): the stated north star for a slice gated on the one-state migration. Not this plan's cargo.
- **Presence/here-ness under murmur**: claim the ambient register only after the carillon boundary is written (murmur: ambient liveness; carillon: active coordinated notification).
- **Transport floor for Isometry/Strophe**: the old "stays unextracted" call is
  superseded. Isometry now consumes the shared p2panda store and needs the same
  endpoint/LogSync pump. `P2pandaTransport` therefore exposes a
  provider-neutral raw-seed constructor so Personae consumers do not depend on
  Mere's identity crate. Promotion to `repos/murm` remains Phase P work; do not
  duplicate the endpoint in Isometry while that move is pending.
- **Browser sync probe**: verify iroh/p2panda-net wasm viability before any browser peer design.

## 6. Open questions (Mark's calls)

1. Drop the p2panda-store `sqlite` feature from moothold and the full crate from mesh once S2 lands (removes sqlx + libsqlite3-sys). Dependency removal: needs explicit sign-off.
2. `InMemoryCabalStore`: retire, or document as a supported public test fixture.
3. Timing of the cabal-to-murmur identifier sweep (N2 keeps it opportunistic; a dedicated pass is available once the slice is quieter).
4. redb long-term: S3 keeps it as muniment's desktop backend; whether it remains the store of record after the muniment convergence proves out is open.
5. Final repo name for the moot family if `repos/moot` reads wrong against the taken crate name.

---

## Findings

### 2026-07-08 (plan creation)

- Multi-agent audit (9 facets/proposals/critique) plus direct verification established: murm trio live and wired (comms_host.rs), substrate triplicated with tessera citing murm as template, gazette orphaned, READMEs stale, mesh proving upstream store sufficiency.
- Cargo.lock shows the sqlite feature's real price: sqlx-core/macros/mysql/postgres/sqlite plus libsqlite3-sys in the tree of an otherwise pure-Rust workspace.
- p2panda-store 0.6.1 trait futures carry no Send bounds; muniment's `?Send` Backend implements them without friction. The wasm compatibility question is a consumer-side (spawn-site) concern only.
- The IndexedDB/OPFS split follows the physics: IDB cannot do partial reads of stored values (structured clone), which iroh-blobs-shaped ranged verification requires; OPFS sync access handles are worker-only and exclusive by default, which IDB's transaction model sidesteps for records. IDB transactions auto-commit on event-loop yield, so batches must issue in one tick; `apply(batch)` in the seam enforces the safe shape by construction.
- Sibling posture dissolves the earlier "neutral substrate crate" recommendation: with the pump consolidated in murm and stores adapted once over muniment in moot, upstream p2panda plus the host composition point covers what the neutral crate would have owned, and no new crate is minted.

### 2026-07-08 — R1 execution

- Reading the three pump copies side by side confirmed the shape: ~100 near-identical lines (`now_ms`, `SyncStatus`, `SyncRound`, the LogSync 3-stage build, the `SyncStarted`/`OperationReceived`/`SyncFinished` drain arms, `resync`'s 30×100ms/quiet≥3 loop, the drop-abort). The only per-consumer variation is the `OperationReceived` accept step (murm `ingest_operation`, tessera sync `verify+insert`, mesh async `verify+ext-guard+insert`). An async `accept` closure covers all three.
- **Seam decision (changes the plan's earlier R1 phrasing):** chose `SyncedSpace::drive(subscription, accept)` over a store-generic `join<S>`. `drive` needs only `E: Extensions` — no `LogStore`/`TopicStore`/`LogId` bounds and no `p2panda-store` dep in transport — while still deduping the meaty drain. The caller keeps its own `LogSync` + `SyncHandle` (liveness + live publish) and passes the subscribed stream + accept.
- **Scope correction (forced by the sibling posture):** R1 is **murm + mesh**, not all three. tessera's pump can't be deduped without removing p2panda-net from moot (purity), which means driving it host-side — that is R2's operation, not R1's. Moved tessera accordingly.
- **mesh-decoupling reversal (flagged for veto):** the pump's only sensible home under "no neutral crate" is `transport` (the p2panda-net owner), so every pump consumer must depend on `transport`. murm already does; mesh did not (it kept `transport` a dev-only dep, deliberately). R1's dedup only has value with a second consumer, so mesh reversing that decoupling is on the critical path. Judged fair and reversible under the sibling posture (mesh = host-composed glue consuming murm's p2panda-net home); mesh stays endpoint-decoupled (still takes injected `Endpoint`/`Gossip`), it just no longer re-rolls the drain. If vetoed, the pump needs a different home (revisits no-neutral-crate for this substrate).

## Progress

### 2026-07-08

- Plan created from the murm audit workflow (map/propose/critique), the sibling-posture discussion, and the store-substrate analysis. DOC_README indexed same session.
- **R0 landed.** Rewrote the murm / murmuring / transport READMEs from the accurate inline module docs. murmuring + murm rewritten wholesale; transport by targeted edits (its design/blobs/peer_id sections were already accurate). Removed every stale claim (BLAKE2b, varint wire, IrohTransport, snapshot-push, `local_node_id`, MereEvent, `dial`, the planned-Veilid section) and reframed each crate to code reality: posts are signed p2panda-core Operations (BLAKE3/CBOR), `P2pandaTransport` with gossip + LogSync, the `reticulum` off-grid feature (default-off, retinue as durable home). Verified by grep: zero forbidden terms (the sole `NodeId` match is the intentional PeerID-vs-NodeId disambiguation note). Docs-only, no compile. `cabal` identifiers left intact (rename is N2).
- **R1 landed (murm + mesh).** Wrote `transport::SyncedSpace` (the `drive(subscription, accept)` seam + `SyncStatus`/`SyncRound`/`resync`/drop-abort); `cargo check -p transport` green. Migrated both consumers onto it, deleting the two drain copies:
  - **mesh** `sync.rs`: dropped the drain loop, status structs, `now_ms`, `Drop`; kept the session build, `author`, `board`; re-exported `SyncStatus`/`SyncRound` from transport to keep `mesh::*` stable. Promoted `transport` dev→normal dep (the flagged decoupling reversal). `cargo test -p mesh` green: 17/17, incl. both two-peer convergence tests over real p2panda-net loopback peers.
  - **murm** `gossip_sync.rs` (dual-lane): kept the gossip live lane + its `posts_received` murm-side (new private `GossipCounters`), moved the LogSync drain to `SyncedSpace`, merged both in `sync_status`/`resync`, and held `LogSync`+`SyncHandle` as a type-erased `Box<dyn Any + Send>` keepalive (murm publishes on gossip, not the handle). `cargo test -p murm` green: 15/15, incl. `two_murm_peers_converge_over_gossip` (gossip lane) and `two_murm_peers_catch_up_over_logsync` (SyncedSpace drain + merged status + resync).
  - murm's public surface is byte-identical (same `SyncedCabal` methods, same `SyncStatus` fields), so the meerkat/comms_host consumer is expected-clean; a full `-p meerkat` check was not run this pass (heavy serval/netrender build) — R2 touches comms_host and will exercise it. Workspace was briefly blocked by a stale `illume` pin (`branch = "main"` vs remote `master`); resolved when Mark renamed illume's branch to `main`.
  - **Net:** three near-identical drain copies → one `SyncedSpace`; tessera's copy remains (R2). Next: **R2** (tessera host-move + moot purity) or **S** (store substrate).
- **R2 landed (moot purity).** Scope was bigger than the plan's line: moothold had **two** pumps (tessera `SyncedMoot`, receive-only redb; moot-object `SyncedMootSpace`, author+receive sqlite), and the tessera pump is **live** in meerkat's `SyncHost` (chrome sync chip). Authoring resolved via **option (a)** (Mark's call): moot signs+stores (`MootStore::author`), the host publishes on its own `SyncHandle` — moot names no p2panda-net type. Executed:
  - Dissolved both `SyncedMoot`/`SyncedMootSpace`; moothold now provides only stores + folds + `verify` + `MootStore::author`. The two lanes' `sync.rs` became `#[cfg(test)]`-only, holding the two-peer convergence tests as host-composed (build `LogSync` + `SyncedSpace::drive`, publish via handle). Rewired `moot-peer` example + the live `meerkat/src/sync.rs` (`SyncedMoot::join` → inline pump + a small `TesseraSync` holding the `SyncedSpace` + store + a `Send` keepalive; `SyncStatus` now from `transport`).
  - **Purity confirmed:** `cargo tree -p moothold -e normal` shows no p2panda-net / p2panda-sync / iroh — only p2panda-core + p2panda-store remain. `p2panda-net`/`p2panda-sync`/`tokio`/`tokio-stream` moved to moothold dev-deps; `p2panda-net`/`p2panda-core` added to meerkat (the host builds the pump now).
  - **Verified:** `cargo test -p moothold` green (72/72, incl. both tessera + both moot convergence tests, all host-composed) and the example builds; `cargo check -p meerkat` green (the live host compiles with the inline pump — the integration check the earlier R-phases deferred).
  - **Fixed an R1 regression the meerkat build surfaced:** murm's `_logsync_keepalive: Box<dyn Any + Send>` made `SyncedCabal` `!Sync`, breaking comms's `CabalSink: Send + Sync`. The earlier `-p comms` check had passed against a stale cached murm. Changed the box to `Send + Sync` (the LogSync session + handle are `Sync`); `-p murm -p comms` green. (meerkat's `TesseraSync` keeps the `Send`-only box — it's only ever moved into a spawn, never shared, so `Send` suffices.)
  - Not driven at runtime (compile-verified only): whether the chrome sync chip still updates live. R2's done-condition asks for two-peer runtime convergence in the app; the unit + moothold integration tests cover the convergence logic, but driving the actual app is a separate `/verify` step. The "murm-family shows no redb" half of the done-condition is **Phase S**, not R2 (murm still uses redb until the muniment migration).
  - **R-phase complete:** four drain copies (murm, mesh, tessera, moot-object) → one `transport::SyncedSpace`; moot is p2panda-net-free. Next: **Phase S** (store substrate: muniment `scan`/`apply`, the shared adapter, wasm IDB+OPFS) or **Phase N** (naming) / **P** (promotion).
- **Committed** the R-phase: `bfcfd05` (16 files, +783/−823 — net negative from the dedup). Whole tree per convention; included 5 pre-existing/concurrent meerkat files (chrome_comms, chrome_nav, ime, keyboard, lib) that rode along (the meerkat check that passed included them, so the checkpoint builds).
- **Phase N landed** (Mark sequenced N before S):
  - **N1 gazette → gazetteer.** `git mv crates/murm/gazette crates/persona/gazetteer`; renamed the crate `gazette` → `gazetteer` (an index/directory, not a broadcast gazette); updated the root workspace members + added `gazetteer` to `[workspace.dependencies]` with an incubating note; refreshed the lib.rs header. Zero `.rs` consumers, so nothing downstream broke. `cargo check -p gazetteer` green.
  - **N2 cabal → murmur (user-facing copy only).** Changed 8 user-facing strings in `comms_host.rs` (status lines, the demo "Project murmur" label + seed post) and `views/panels.rs` (Join / Share buttons, the compose "To" label). Left every code identifier (`CabalKey`/`CabalId`/`CabalHandle`, `open_cabal`, `CabalExt`, `with_cabal`), the internal error strings, the observability diagnostic, and the test fixtures for opportunistic migration — the same edge→link discipline. String-literal-only, no compile risk.
  - **N3 TERMINOLOGY.md.** Codified the gerund law (`murmuring`:`murm` :: `mooting`:`moothold`, with the substrate-extraction note), added a `gazetteer` entry, and a `murmur` in-product entry (container-not-post framing; product-says-murmur / code-stays-cabal, mirroring the `link` entry).
  - Next: **Phase S** (the store substrate — the big novel chunk).
- **S1 landed (muniment seam growth).** In `repos/muniment` (separate repo): grew the `Backend` trait with `scan(start, end)` (half-open lexicographic range, ascending — the ordered read a log needs; default filters+sorts `list`, overridable by redb ranges / IDB cursors) and `apply(&[WriteOp])` (atomic batch — the multi-key op-header + payload + log-index write; default sequential with the content-addressed-recovery discipline, overridable by a real transaction). Added the `WriteOp` enum (Put/Delete), exported it, and overrode both on `MemoryBackend` (scan filtered+sorted; apply atomic under the single mutex). `cargo test` (muniment) green: 13/13, incl. the ordered-scan test, the atomic-batch test, and the whole-batch-in-one-call guard (the seam-level batch-spanning-await mitigation: `apply` takes the full batch so a backend commits in one tick). Non-breaking (both methods have defaults, so codicil and other Backend users are unaffected). Uncommitted in the muniment repo. Next: **S2** (the shared `LogStore`/`OperationStore`/`TopicStore` adapter over muniment, in mere's moot family — the big one; retires the hand-rolled redb impls in murm + tessera).
- **S2a landed (the muniment store adapter).** Built `MunimentStore<B, E>` in `mooting` (was a name-reservation stub): one adapter implementing all three p2panda-store traits over a muniment `Backend`, so murm's cabal / tessera's reputation / mesh's job logs can share one store family (redb desktop, IDB+OPFS wasm) instead of a hand-rolled redb each. Facts pinned from the p2panda 0.6.1 source, not guessed:
  - `LogSync`'s store bound (net `log_sync/builder.rs`) is only `LogStore + TopicStore + Clone + Send + 'static`, not `OperationStore`, not `Transaction`. All three impl'd anyway so the adapter is a `SqliteStore` drop-in.
  - In `OperationStore<T, ID, C>` the collection id `C` **is the log id**, passed explicitly (`insert_operation(id, op, log_id)`), so there is no header-extension extraction to do; the consumer supplies it. `SeqNum = u64`, so the bound's `S = u64` matches.
  - The `?Send` question is settled in the plan's favour: LogSync drives the store from a ractor `ThreadLocalActor`, so the handle must be `Send` (it is, over a Send backend) but its method futures need not be.
  - Key schema: `log/<author>/<hex CBOR log_id>/<seq:016x>` -> CBOR `(Header, body)` (the ordered scan index; the trailing `/` isolates adjacent logs whose encodings share a prefix); `op/<hash>` -> pointer to the log key (id lookups skip the scan); `topic/<topic>/<author>/<log>` -> association. The log id segment is `hex(CBOR(log_id))`, uniform for any `L: LogId` (u64 for murm/mesh, `[u8;32]`, etc.).
  - p2panda's id-keyed methods (`get_operation`/`has_operation`/`resolve`) carry the `L` param but take no `L` argument, so a bare `store.get_operation(id)` can't infer `L` (the wart that forces p2panda's own doc example into fully-qualified syntax). Fixed with same-name inherent twins that shadow the trait methods for direct calls, so every consumer (not just the tests) calls them without pinning a log-id type.
  - Reuses `muniment::StoreError` as the adapter error (its `Codec` variant covers decode failures): zero new error type, and the only new dep is `hex`. muniment is a path dep (`../../../../muniment`), matching graph-kernel's existing pattern; the persistence family stays path-linked until published.
  - **Verified:** `cargo test -p mooting` green (3/3: log round-trip + seq ordering + range + heights + size; topic associate/resolve/remove; prune-below-seq), plus a `_log_sync_ready` compile assertion that `MunimentStore<MemoryBackend, ()>` satisfies LogSync's full store bound (the two sync traits and Clone + Send + 'static together).
  - Next: **S2b** (rewire consumers). mesh + tessera are clean (their stores are sync-only). murm is the real work: `PersistentCabalStore` couples the post/channel **domain** view with the `LogStore`/`TopicStore` **sync** index, so S2b decouples them (domain store for the cable engine's reads, `MunimentStore` for LogSync) rather than swapping one for the other. Done-condition: murm/tessera/mesh two-peer LogSync convergence tests pass on the muniment-backed store.
- **S3 landed (redb desktop backend), reordered ahead of S2b.** Scoping mesh's rewire surfaced the real dependency: a consumer's store carries a **file-persistence** path (mesh's `at_url`, its reopen test), and retiring `SqliteStore` wholesale needs muniment to actually persist on desktop. The convergence *tests* run in-memory, but a clean full rewire wants durability first, so S3 moved ahead of S2b. In `repos/muniment`: added `RedbBackend` behind a non-default `redb` feature (`redb = "2"`, matching mere's existing pin in murmuring/moothold). One string-keyed table; `scan` is a native redb range (ascending, half-open, exactly the seam's contract), `apply` a single write transaction (atomic batch), `list` a prefix-bounded range walk. redb's synchronous I/O runs inline behind the `?Send` methods. The API was pinned from murmuring's live `log_store.rs` redb usage, not guessed. **Verified:** `cargo test --features redb` green (5 new: round-trip, prefix list, ordered scan matching `MemoryBackend`'s, atomic batch, reopen-persistence), and the **default build stays redb-free** (gating correct, so graph-kernel and other Backend users pull nothing new). Committed `0d60e19` in the muniment repo. Now both backends exist (`MemoryBackend` for tests, `RedbBackend` for desktop; wasm OPFS/IDB is **S4**), so S2b can replace a consumer's store wholesale instead of leaving `at_url` stranded.
- **S2b-mesh landed (first consumer on the muniment store).** Rewired mesh from p2panda-store's `SqliteStore` to the muniment-backed `MunimentStore`, end to end. `MeshStore<B>` is now generic over the backend (`MemoryBackend` for tests, `RedbBackend` for a device via `at_path`); `SyncedMesh<B>` and the `mesh-peer` example thread `B` through (the example uses a generic `run<B>` since the backend is a runtime choice). `insert` associates the topic then inserts the op (p2panda's canonical order; a crash between leaves at worst a topic pointing at an empty log, never an op the topic can't reach). Two design consequences surfaced and were resolved:
  - **The ?Send/Send collision** (flagged in advance): the shared drain `SyncedSpace::drive` runs on `tokio::spawn` and needs the accept closure's `store.insert().await` future to be `Send`, but muniment's `?Send` Backend made it `!Send`. Resolved **on Mark's call** by making muniment's `Backend` **`Send` on native, `?Send` on wasm** (cfg'd `async_trait`): native backends return `Send` futures, the browser keeps `?Send` for OPFS. muniment `efc4e14`.
  - **A default-method `Sync` leak:** that native-`Send` change then leaked a `Self: Sync` bound to generic callers, but *only* through `scan`/`apply`, which were **default** methods (a defaulted async body borrows `&self` under the `Send` bound; required methods do not). Made them required (both backends override them anyway), which removed the leak, so mooting and mesh use `B: Backend` again. muniment `2ea41a0`. `SyncedMesh` still needs `B: Sync` at its impl (its `&self` `insert` future must be `Send` in the drain); native backends are `Sync`, so it holds.
  - **Verified:** `cargo test -p mesh` green (17/17), including both networked two-peer convergence tests (`a_job_posted_on_one_peer_is_worked_by_the_other` — the full M1 post → claim → execute → result round-trip over the live lane; `a_late_joiner_catches_up_on_an_existing_log` — RBSR offline catch-up) and `a_file_store_survives_reopen` (redb durability). muniment 19/19 and mooting 3/3 stay green. First real-loop proof of the whole store substrate (S2a adapter + S3 redb + the `Send` seam), over actual p2panda-net loopback peers.
  - Next: **S2b-tessera** (same shape, a sync-only redb store) then **S2b-murm** (the intricate one: decouple `PersistentCabalStore`'s post/channel domain view from its `LogStore`/`TopicStore` sync index).
- **S2b-tessera landed (second consumer; scope correction).** The line above called tessera "same shape, sync-only" — reading it showed the opposite. `TesseraStore` was **synchronous** (redb direct), so moving to the async `MunimentStore` turned its whole surface async and rippled through `fold_moot` and the **live pump in meerkat** (`build_sync_lane`), making moothold + meerkat one atomic change (the harder of the two consumers, not the easier). Done on Mark's go-ahead:
  - **moothold**: `TesseraStore<B>` wraps `MunimentStore<B, TesseraExt>` (log id `u64`); `insert`/`get`/`has`/`len`/`is_empty`/`fold_moot` are now async; `fold_moot` re-expressed over the adapter's `resolve` + `get_log_entries`; the hand-rolled `log_store.rs` (redb `LogStore`/`TopicStore`) **deleted** — the adapter provides them; `TesseraFileStore = TesseraStore<RedbBackend>` alias names the durable path; error slimmed to `Store(#[from] StoreError)` + `Malformed`. mooting gained `operation_count`/`is_empty` so tessera's `len`/`is_empty` don't reach into the adapter's key schema.
  - **meerkat** `sync.rs`: `TesseraFileStore`, `sync_store()` into the builder, async `insert`/`is_empty`/`author_starter_log`/`ledger`; the keepalive box became `Box<dyn Any + Send + Sync>` so the status-poll task can hold `&self` across the async ledger fold; the log-id type pinned (`LogSync<_, u64, TesseraExt>`) because the muniment store impls `LogStore` for every `L` and the type-erased keepalive leaves inference nothing to work back from.
  - **Verified:** `cargo test -p moothold` green (68/68, incl. both networked tessera convergence tests — `two_moots_converge_on_the_same_scores` and the ticket-bootstrap variant — plus the moot lane, untouched; the 4 fewer than before are the deleted `log_store.rs` tests, now covered by mooting's adapter tests + the convergence tests). meerkat's `sync.rs` compiles clean; a full `cargo check -p meerkat` is currently blocked **only** by a concurrent agent's in-progress `scenario` module (unrelated to tessera: missing `chord_label` / `scenario_navigate` / `scenario_key`), so its scenario WIP was left unstaged and this commit carries only the tessera files.
  - **Store convergence is 2 of 3** (mesh + tessera on muniment; the moot-object lane's `MootStore` stays p2panda-sqlite, out of S2's stated murm/tessera/mesh scope). **S2b-murm** remains: decouple `PersistentCabalStore`'s domain view from its sync index.
- **External-consumer boundary landed (2026-07-11).** Isometry's campaign
  collaboration lane now composes `mooting::MunimentStore` with Personae-signed
  p2panda operations. To remove the remaining identity-crate coupling from the
  endpoint, `P2pandaTransport` gained `builder_from_seed` and `bind_seed`; the
  existing identity-keypair constructors delegate to the same path. A focused
  identity-equivalence test and the full 29-test transport suite are green.
  Isometry also surfaced repeated topic/operation write ordering, so
  `MunimentStore::insert_indexed_operation` now centralizes the
  discoverability-safe order for every domain consumer. This is the first
  non-Mere consumer proving that the shared store and pump are library
  boundaries, and strengthens the Phase P promotion trigger.
