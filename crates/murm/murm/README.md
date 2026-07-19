# murm

`murm` is the bilateral peer-to-peer comms supercrate for the
[mere](https://crates.io/crates/mere) browser. One-to-one (and small-group)
signed conversations between known peers: a *murmur* between them.

`murm` composes identity from
[`identity`](https://crates.io/crates/identity), transport from
[`transport`](https://crates.io/crates/transport), its signed operation
grammar, and the shared replication store behind one high-level conversation
runtime.

## What's in the crate

- **`Murm<T: Transport>`** — the orchestrator, generic over the transport.
  - `Murm::new(identity, transport)` — construct with an in-memory store.
  - `Murm::with_storage(identity, transport, ConversationStorage::redb(path))`
    — construct with durable per-cabal stores.
  - `local_peer_id()` — the peer identity derived from the master pubkey.
  - `open_cabal(&CabalKey) -> Future<CabalHandle>` — open or rejoin a cabal by
    its 32-byte secret key. A durable open rebuilds history and the local author
    head before returning. The `CabalHandle` sends text, queries `history`, and
    `subscribe`s to posts.
  - `derive_cabal_keypair(&CabalKey)` — the per-cabal Ed25519 keypair
    derivation.
  - `subscribe_cabal(&CabalKey) -> SyncedCabal` (on `Murm<P2pandaTransport>`) —
    join a cabal's sync overlays. Authoring through the returned `SyncedCabal`
    broadcasts each post live over gossip; peers that were offline catch up
    via LogSync (RBSR) over the shared muniment store. Both lanes ingest idempotently,
    so history converges across online members and catches up peers that
    were away.
- **`CabalHandle` / `CabalId` / `CabalKey`** — cabal addressing.
  - `CabalKey` is the 32-byte secret shared between members; `CabalId` is
    the public BLAKE3 derivation; `CabalHandle` is what callers send /
    query through.
  - `import_plain_drop` and `import_protected_drop` admit native drops through
    the shared processor, refresh materialized history, and publish newly
    visible posts once.
  - `CabalKeyring` protects native drops with epoch keys. Installing a new key
    preserves the stable `CabalId`; callers may retain or forget older epochs
    according to their recovery policy.
- **`SyncedCabal`** — a `CabalHandle` plus the two live sync lanes (gossip +
  LogSync) over a `P2pandaTransport`.
- **`MurmError`** — unified error type.
- **`ConversationDropSelector`** — settings-driven catch-up, archive, and
  radio selection for the shared native-drop exporter. Per-author frontiers,
  signed-header privacy, body privacy, and post-kind priority remain
  conversation policy.
- **`ConversationStore`** — the muniment-backed replacement substrate with
  shared structural checks, conversation admission, continuity, LogSync store
  traits, and native-drop export. The live `ConversationEngine` now uses it.
- **Re-exports for ergonomic use**:
  - `Ed25519PublicKey`, `IdentityProvider` from identity.
  - `Alpn`, `PeerID`, `Transport` from transport.
  - `ChannelName`, `InfoEntry`, `Post`, `PostId`, and `PostKind`.
  - Post primitives: `encode_post`, `decode_post`, `hash_post`, `sign_post`,
    `verify_post`.

## Vocabulary

Product surfaces call the conversation a **murmur**. **Cabal** remains the
protocol and code noun for the invitation-scoped shared conversation. **Cable**
names the inherited cabal/channel/post grammar and Mere's namespaced
`mere/cable/v1` dialect; it does not assert wire compatibility with cabal-club
Cable. Runtime and storage mechanics use plain `Conversation*` names.

## How it relates to other workspace crates

murm sits above the identity / transport / wire-protocol layers and below
the user-facing Comms UI.

```text
              Comms pane (UI; reads / writes via murm)
                          │
                          ▼
                        murm
              cabal lifecycle, live + catch-up sync,
              per-cabal keypair derivation, ALPN claim
                          │
   ┌──────────────────────┴──────────────────────┐
   ▼                                             ▼
identity                                      transport
(derive_keypair)                 (P2pandaTransport + gossip)
```

- [`identity`](https://crates.io/crates/identity) —
  `derive_keypair(cabal_key)` produces the per-cabal Ed25519 keypair. The
  master secret never leaves the identity provider; murm only sees the
  derived keypair.
- [`transport`](https://crates.io/crates/transport) — murm is
  `Murm<T: Transport>`; the same code runs against `MemoryTransport` in
  tests and `P2pandaTransport` in production. Cabal traffic claims its own
  ALPN.
- [`gemot`](https://crates.io/crates/gemot) — many-to-many federation
  lives there, not here. murm strictly handles bilateral / small-group
  flows.
- [`mere`](https://crates.io/crates/mere) — composes murm into the product.

## Status

Pre-1.0. The `Murm` orchestrator, cabal lifecycle, per-cabal keypair
derivation, and the `CabalHandle` send / history / subscribe surface are in
place. Over a `P2pandaTransport`, `subscribe_cabal` returns a `SyncedCabal`
that runs both live gossip and LogSync catch-up; two peers on the same cabal
key converge over the wire (tested). The host wires a networked cabal into
mere's Comms pane end to end (join ticket, connect-by-ticket, live post
drain) and selects redb storage under its per-user comms directory.

Ahead:

- **Host-led co-op sessions** (`host_coop` / `join_coop`) — ephemeral,
  host-admitted sessions over the bilateral transport.
- **Cross-author causal links** and **retention-aware pruning** — domain work
  over the shared store.
- **Group key distribution** — native drops have real epoch-aware XChaCha20
  protection and old-epoch recovery. Personae or a p2panda group-state adapter
  still owns authorization, epoch distribution, and persisted key history.

## License

MIT OR Apache-2.0.
