# Address Book Muniment Port Plan

**Status: scoped 2026-08-16, awaiting go.** Scoping only; nothing here is
implemented.

## Why

The p2panda-net address book is the single place p2panda's bundled SQLite
backend leaks into mere's graph. Mere's own crates already draw the storage
boundary deliberately: `crates/mesh/mesh` and `crates/moot/gemot` both declare
`p2panda-store` with `default-features = false`, every `p2panda_store` import
in mere is the `LogStore` or `TopicStore` trait, and storage goes through
muniment via `stickleback::MunimentStore`. But the fork's `address_book`
feature enables `p2panda-store/sqlite`, and every other p2panda-net feature
(`iroh_endpoint`, `discovery`, `gossip`, `sync`) requires `address_book`, so
any networking at all pulls sqlx and the SQLite row back in. That row is also
what pins the CubeCL/Burn upgrade path (dependency check, session of
2026-08-16). Porting the address book to muniment settles the architectural
inconsistency and unblocks the prerelease row in one move.

Naming note: this is the transport-layer node-info store (addresses, topics,
bootstrap/stale flags). It is unrelated to gazette, mere's handle-resolution
index, despite the everyday sense of "address book".

## Plan

- **P1, fork: erase the store type in p2panda-net's address book.**
  Introduce an internal dyn-compatible wrapper trait over
  `AddressBookStore<NodeId, NodeInfo>` (boxed futures; the published trait
  uses RPIT methods and is not dyn-compatible as written), with a blanket
  impl. `AddressBookActor` state, `AddressBookActorArgs`, the
  `Store(RpcReplyPort<...>)` message, the `store()` accessor, and the builder
  hold the erased handle instead of `SqliteStore`. `AddressBookError::Store`
  carries an erased error instead of `SqliteError`. Discovery's manager hands
  the erased store to walker/session, which are already generic over
  `S: AddressBookStore`. Done when: `address_book` feature no longer lists
  `p2panda-store/sqlite`; SQLite becomes an optional convenience feature
  (default store in the builder when enabled, as today).
- **P2, mere: implement `AddressBookStore` over muniment.** Home is
  stickleback, beside `MunimentStore`'s existing `OperationStore` /
  `LogStore` / `TopicStore` impls over a muniment `Backend`. Same pattern:
  generic over `B: Backend`, so redb serves desktop and IndexedDB serves the
  browser with one impl. Done when: stickleback's impl passes p2panda-store's
  `address_book` test suite (test_utils exist upstream).
- **P3, mere: switch the hosts.** Whoever builds the `AddressBook` today
  (mesh host, murm transport) passes the muniment-backed store explicitly.
  Done when: `p2panda-store/sqlite` is absent from mere's lockfile and the
  CubeCL/Burn row check passes.
- **P4, browser posture (observation, not work in this plan).** With SQLite
  out, the storage blocker on a browser mesh is gone; what remains is
  p2panda-net's runtime shape (tokio, ractor actors, iroh's browser lane),
  which this plan does not assess.

## Findings

**Fork surface (crates/p2panda).** The store side already publishes the
traits: `AddressBookStore<ID, N>` and `NodeInfo<ID>` in
`p2panda-store/src/address_book/traits.rs` (125 lines, RPIT async methods).
The net side binds concrete in three files: `actor.rs` (533 lines: state
field, actor args tuple, `Store` reply port, `SqliteError` in helper
signatures), `api.rs` (307: `store()` accessor, `SqliteError` in the public
error enum), `builder.rs` (39: `SqliteStoreBuilder` default). Two discovery
actors name `SqliteStore` only to instantiate protocols that are already
generic (`RandomWalker`, `PsiHashDiscoveryProtocol`). The `AddressBook` api
handle is held across ~29 files (gossip, discovery, iroh_endpoint,
supervisor); erasure keeps it non-generic so none of those files change.
The rejected alternative, a generic `AddressBook<S>`, ripples a type
parameter through all of them for no capability gain.

**Transactions are lighter than they look.** Every `tx!` use in the actor
wraps a single logical store op, and the one read-modify-write
(`InsertTransportInfo`) performs its read outside the transaction. The permit
is sqlx concurrency control, not multi-op atomicity. The erased trait
therefore needs per-op atomicity only, which muniment's `Backend` contract
already guarantees (`apply` groups multi-key writes in one transaction if a
compound op ever appears).

**Muniment browser story (the indexeddb/opfs question).**
`IndexedDbBackend` exists today in `muniment/src/indexeddb_backend.rs`:
durable, transactional, origin-scoped; writes await the browser's commit;
`apply` is one transaction, so a tab closed mid-batch leaves the previous
state. It sits beside `RedbBackend` (desktop) and `ZipBackend` (portable
archive) as the third host realization. OPFS is designed for but not
implemented: the `Backend` trait relaxes to `?Send` on wasm specifically so a
browser main thread can await OPFS promises, and the redb/zip docs both name
"an OPFS-backed store" as the browser counterpart. So: a muniment-backed
address book runs in the browser today over IndexedDB, and an OPFS backend
slots in later with zero change to the address book, because P2 is generic
over `Backend`. By contrast the current SQLite address book can never reach
wasm32 (sqlx's SQLite lane does not build there), so today's binding
forecloses a browser mesh outright.

**Scope held.** p2panda-store's SQLite backend keeps its other trait impls
(`LogStore` etc.) untouched; only the address book leaks into mere, so only
it is ported. Upstream p2panda-net 0.7.0 (2026-07-07) is current and has no
newer release doing this.

## Open questions for Mark

- **D1: where the muniment impl lives.** Stickleback is recommended (the
  muniment-to-p2panda adapter layer already lives there); the alternative is
  a small module in the mesh crate if address-book policy turns out to be
  mesh-specific.
- **D2: fork posture on the sqlite feature.** Keep it as an optional
  non-default feature for fork hygiene against upstream, or delete the
  binding outright since mere is the only consumer.

## Progress

- 2026-08-16: scoped. Verified the trait already exists upstream, mapped all
  concrete bindings, confirmed discovery protocols are already generic,
  confirmed tx! wraps single ops, confirmed IndexedDbBackend is shipped and
  OPFS is a designed-for gap. No code changed.
