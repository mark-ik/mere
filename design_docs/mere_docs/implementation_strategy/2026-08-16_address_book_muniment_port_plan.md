# Address Book Muniment Port Plan

**Status: complete 2026-08-16.** All four phases landed and are verified.
Decisions taken by Mark before execution: the muniment impl lives in
stickleback (D1), and `sqlite` stays as an optional non-default feature of the
fork rather than being deleted (D2).

## Why

The p2panda-net address book was the place p2panda's bundled SQLite backend
leaked into mere's graph. Mere's own crates already drew the storage boundary
deliberately: every `p2panda_store` import in mere is a trait, and storage goes
through muniment via `stickleback::MunimentStore`. But `p2panda-net`'s
`address_book` feature enabled `p2panda-store/sqlite`, and every other
p2panda-net feature requires `address_book`, so any networking pulled sqlx in.
That row also pinned the CubeCL/Burn upgrade path through a `libsqlite3-sys`
`links` collision.

Naming note: this is the transport-layer node-information store (addresses,
topics, bootstrap/stale flags). It is unrelated to gazette, mere's
handle-resolution index, despite the everyday sense of "address book".

## Phases

- **P0 — second enabler closed (fork).** `p2panda-sync` and `p2panda-stream`
  both declared `p2panda-store` with `default-features = true`, which turned on
  `p2panda-store/default` -> `sqlite` independently of the address book. Neither
  uses anything gated behind it outside its own tests. Both now declare
  `default-features = false`; their dev-dependencies already enabled
  `test_utils` separately. **Done, both compile clean.**
- **P1 — store erased (fork).** `AddressBookStoreHandle` holds some
  `AddressBookStore` behind a trait object and implements `AddressBookStore`
  itself. `address_book` no longer enables `p2panda-store/sqlite`; `sqlite` is
  its own feature, still in `default`, so nothing changes for other consumers
  unless they opt out. **Done, 38 lib + 1 api + 1 e2e + 7 doc tests pass.**
- **P2 — muniment address book (mere).** `stickleback::MunimentAddressBook`,
  generic over a muniment `Backend` and `Codec`. **Done, 11 unit tests.**
- **P3 — consumers switched (mere).** The transport supplies the muniment store;
  all six mere crates naming p2panda-net opt out of `sqlite`. **Done: sqlx and
  libsqlite3-sys are absent from the workspace graph.**

## Findings

**The plan's headline claim was wrong, and cargo caught it.** The scoping doc
said the address book was "the single place" sqlite leaked in. `cargo tree -e
features -i p2panda-store` showed two enabler edges, not one: the address book,
and `p2panda-store/default` arriving through p2panda-sync and p2panda-stream.
Porting the address book alone would not have dropped sqlx. Both had to go.

**The patch table's rationale was also wrong.** mere's `[patch.crates-io]`
comment justified the vendored cubecl-wgpu backport partly by "p2panda-store's
`groups`/`encryption` features (which gemot needs) force its `sqlite` feature".
Neither feature was ever enabled in this graph. Corrected in place; the entry
now records that our half of the `libsqlite3-sys` conflict is gone and that
whether burn 0.22-pre now resolves is the burn migration's question.

**Upstream was mid-rewrite, and it did not collide.** `upstream/development` is
161 commits ahead of `main` (tip 2026-08-15) and two of them rewrite the address
book. They touch only `p2panda-store/src/address_book/sqlite.rs`: `traits.rs` and
the whole of `p2panda-net/src/address_book/` are byte-identical between the two
branches. Upstream is rewriting the SQLite *implementation*, not the trait and
not the net-side binding, so this work sits above their churn. Our fork's `main`
is current with `upstream/main` plus three own commits.

**Erasure beat genericization.** A generic `AddressBook<S>` would have rippled a
type parameter through the ~29 files holding an `AddressBook`. The handle keeps
every one of them non-generic. The discovery strategies needed nothing at all:
`RandomWalker` and `PsiHashDiscoveryProtocol` were already generic over
`S: AddressBookStore`, and p2panda-discovery has zero references to
`Transaction`.

**Transactions fold into the writes.** Every `tx!` site in the actor wrapped
exactly one store operation, and the one read-modify-write
(`InsertTransportInfo`) already performed its read outside the transaction. So
no permit ever spanned more than one operation. `with_transactions` reproduces
the old behaviour for a `Transaction` backend; `new` suits one whose operations
are already atomic. Muniment takes the second path, using `apply` where a write
genuinely spans keys (removing a node also removes its topics).

**The erased futures are `!Send`, and that is load-bearing.**
`AddressBookStore` does not declare its futures `Send`, so it cannot be required
of an arbitrary backend. Nothing needs it: the address book actor, the discovery
manager, its walker and its sessions are all `ThreadLocalActor`s. The handle
itself stays `Send + Sync`, because ractor moves actor arguments and messages to
the actor's thread even for thread-local actors. This is also what leaves room
for a browser backend awaiting JS promises.

**Behaviour is preserved, not merely compiled.** p2panda's default address book
was `SqliteStoreBuilder` at `:memory:`, so the transport's new
`MunimentAddressBook<MemoryBackend, JsonCodec>` has the same lifetime. A caller
wanting persistence hands in a durable backend instead. One semantic was checked
against the SQL rather than assumed: `remove_older_than` compares strictly
(`updated_at < UNIXEPOCH() - ?`), so an entry written in the same second
survives a zero-length window. The first muniment test asserted the opposite and
failed; the implementation was right and the test was wrong.

**Browser posture (the indexeddb/opfs question).** `IndexedDbBackend` ships in
muniment today: durable, transactional, origin-scoped, with `apply` as one
transaction. OPFS is designed for but not implemented, and the `Backend` trait
already relaxes to `?Send` on wasm for it. Because `MunimentAddressBook` is
generic over `Backend`, the address book gets IndexedDB now and OPFS later with
no change here. The SQLite binding could never reach wasm32 at all, so this
removes the storage-side block on a browser mesh. The remaining block is
p2panda-net's runtime shape (tokio, ractor, iroh), which this work did not
assess and does not claim.

**Queries are scans, deliberately.** The muniment impl has no secondary index:
topic lookup, the counts and both random pickers walk the `node/` key set. An
address book holds the nodes one peer has heard of, so the set is small and a
scan costs less than index writes on every insert. Recorded in the module docs
as the thing to revisit if a consumer grows the set.

## Verification

- p2panda-net with every feature except `sqlite`: compiles, and `cargo tree`
  reports zero sqlx / libsqlite3-sys nodes.
- p2panda-net suite: 38 lib, 1 api, 1 e2e (`gossip_and_sync_with_same_topic`),
  7 doc tests, all passing.
- stickleback: 11 new address book tests plus the existing 39, all passing.
- mere workspace graph: zero sqlx / libsqlite3-sys occurrences;
  `p2panda-store` remains, with no features enabled.
- Live two-peer networking over the muniment address book:
  `paired_p2panda_transports_round_trip_bytes` and
  `gossip_propagates_ops_between_subscribed_peers` pass.
- One caveat: `the_peer_directory_separates_a_known_address_from_a_live_path`
  failed once, on the first run after a cold compile, then passed on four
  consecutive runs including the identical parallel configuration and
  single-threaded. It carries 20s wall-clock timeouts over real discovery, so it
  is load-sensitive. No pre-change control was run, so this is inference from
  the isolation behaviour rather than a proof of no regression.

## Residue

- The `sqlite` feature remains in the fork's `default` (D2), so an outside
  consumer of `mark-ik/p2panda` is unaffected. Only mere opts out.
- The vendored `cubecl-wgpu` patch is retire-able, and this was **verified by
  probe on 2026-08-16**: with `burn = "0.22.0-pre.2"` and the patch disabled the
  workspace resolves, and with the burn wgpu features actually enabled the graph
  carries exactly one `wgpu` (30.0.0) and one `libsqlite3-sys` (0.38.2, from
  rusqlite via CubeCL's autotune cache). `cubecl-wgpu` then comes from the
  registry at 0.11.0-pre.2. The patch must retire *with* the burn migration, not
  before: dropping it on burn 0.21 puts wgpu 29 back. The migration itself is
  real API work (`burn::backend` and `burn::tensor::backend` moved, `Tensor`'s
  rank parameter changed) and was not attempted. Probe reverted. Recorded in
  [burn 0.22 migration](2026-08-09_burn_0_22_migration_plan.md).
- Adopting burn 0.22 reintroduces an embedded SQLite as `rusqlite`, behind
  CubeCL's autotune cache. It does not cross mere's storage boundary, but the
  workspace should not be called sqlite-free after that point.
- A wasm address book needs `MunimentAddressBook` over `IndexedDbBackend` and a
  p2panda-net runtime assessment. Not in scope.
