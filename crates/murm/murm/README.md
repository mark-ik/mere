# murm

`murm` is the bilateral peer-to-peer comms supercrate for the
[mere](https://crates.io/crates/mere) browser. One-to-one (and small-group)
signed conversations between known peers: a *murmur* between them.

`murm` orchestrates three layers: identity from
[`identity`](https://crates.io/crates/identity), transport from
[`transport`](https://crates.io/crates/transport), and the Cable wire
protocol from [`murmuring`](https://crates.io/crates/murmuring). It exposes
a small high-level API for callers; the moving parts stay behind it.

## What's in the crate

- **`Murm<T: Transport>`** — the orchestrator, generic over the transport.
  - `Murm::new(identity, transport)` — construct.
  - `local_peer_id()` — the peer identity derived from the master pubkey.
  - `open_cabal(&CabalKey) -> CabalHandle` — open or rejoin a cabal by its
    32-byte secret key. The `CabalHandle` sends text, queries `history`, and
    `subscribe`s to posts.
  - `derive_cabal_keypair(&CabalKey)` — the per-cabal Ed25519 keypair
    derivation.
  - `subscribe_cabal(&CabalKey) -> SyncedCabal` (on `Murm<P2pandaTransport>`) —
    join a cabal's sync overlays. Authoring through the returned `SyncedCabal`
    broadcasts each post live over gossip; peers that were offline catch up
    via LogSync (RBSR) over the redb store. Both lanes ingest idempotently,
    so history converges across online members and catches up peers that
    were away.
- **`CabalHandle` / `CabalId` / `CabalKey`** — cabal addressing.
  - `CabalKey` is the 32-byte secret shared between members; `CabalId` is
    the public BLAKE3 derivation; `CabalHandle` is what callers send /
    query through.
- **`SyncedCabal`** — a `CabalHandle` plus the two live sync lanes (gossip +
  LogSync) over a `P2pandaTransport`.
- **`MurmError`** — unified error type.
- **Re-exports for ergonomic use**:
  - `Ed25519PublicKey`, `IdentityProvider` from identity.
  - `Alpn`, `PeerID`, `Transport` from transport.
  - `BilateralProtocol`, `ChannelName`, `InfoEntry`, `Post`, `PostId`,
    `PostKind` from murmuring.
  - Post primitives: `encode_post`, `decode_post`, `hash_post`, `sign_post`,
    `verify_post`.

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
   ┌──────────────────────┼──────────────────────────┐
   ▼                      ▼                          ▼
identity            transport                   murmuring
(derive_keypair   (P2pandaTransport:          (Cable over p2panda:
 per cabal)        QUIC + blobs + gossip)       signed Operations)
```

- [`identity`](https://crates.io/crates/identity) —
  `derive_keypair(cabal_key)` produces the per-cabal Ed25519 keypair. The
  master secret never leaves the identity provider; murm only sees the
  derived keypair.
- [`transport`](https://crates.io/crates/transport) — murm is
  `Murm<T: Transport>`; the same code runs against `MemoryTransport` in
  tests and `P2pandaTransport` in production. Cabal traffic claims its own
  ALPN.
- [`murmuring`](https://crates.io/crates/murmuring) — the protocol-core
  layer. murm dispatches conversation operations to whichever
  `BilateralProtocol` backs a given cabal; murm itself does not encode or
  decode posts.
- [`moothold`](https://crates.io/crates/moothold) — many-to-many federation
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
drain).

Ahead:

- **Host-led co-op sessions** (`host_coop` / `join_coop`) — ephemeral,
  host-admitted sessions over the bilateral transport.
- **Cross-author causal links** and **log pruning** — protocol-level work
  in `murmuring`.
- **A second `BilateralProtocol`** beyond Cable, dispatched transparently.

## License

MPL-2.0.
