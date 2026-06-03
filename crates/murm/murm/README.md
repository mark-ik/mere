# murm

`murm` is the bilateral peer-to-peer comms supercrate for the
[mere](https://crates.io/crates/mere) browser. One-to-one (and small-group)
signed conversations between known peers: a *murmur* between them.

`murm` orchestrates three layers: identity from
[`identity`](https://crates.io/crates/identity), transport from
[`transport`](https://crates.io/crates/transport), and the wire
protocol from [`murmuring`](https://crates.io/crates/murmuring). It exposes
a small high-level API (`open_cabal`, snapshot push / accept) for callers;
the moving parts stay behind it.

## What's in the crate

- **`Murm<T: Transport>`** — the orchestrator, generic over the transport.
  - `Murm::new(identity, transport)` — construct.
  - `local_node_id()` — peer identity derived from the master pubkey.
  - `open_cabal(&CabalKey) -> CabalHandle` — open or rejoin a cabal by its
    32-byte secret key.
  - `derive_cabal_keypair(&CabalKey)` — the per-cabal Ed25519 keypair
    derivation.
  - `subscribe_cabal(&CabalKey) -> SyncedCabal` (on `Murm<P2pandaTransport>`) —
    join a cabal's sync overlays. Authoring through the returned `SyncedCabal`
    broadcasts each post live over gossip; offline peers catch up via LogSync
    (RBSR) over the redb store. Both lanes ingest idempotently, so history
    converges across online members and catches up peers that were away.
- **`CabalHandle` / `CabalId` / `CabalKey`** — cabal addressing.
  - `CabalKey` is the 32-byte secret shared between members; `CabalId` is
    the public derivation; `CabalHandle` is what callers send / query
    through.
- **`MurmError`** — unified error type.
- **Re-exports for ergonomic use**:
  - `Ed25519PublicKey`, `IdentityProvider` from identity.
  - `Alpn`, `NodeId`, `Transport` from transport.
  - `BilateralProtocol`, `ChannelName`, `InfoEntry`, `Post`, `PostId`,
    `PostKind` from murmuring.
  - Post primitives: `encode_post`, `decode_post`, `hash_post`, `sign_post`,
    `verify_post`.

## How it relates to other workspace crates

murm sits above the identity / transport / wire-protocol layers and below
the user-facing Comms UI in graphshell.

```text
              graphshell Comms applet
              (UI; reads / writes via murm)
                          │
                          ▼
                        murm
              Cabal lifecycle, snapshot push/accept,
              per-cabal keypair derivation, ALPN claim
                          │
   ┌──────────────────────┼──────────────────────────┐
   ▼                      ▼                          ▼
identity      transport                murmuring
(derive_keypair    (streams +              (wire protocol;
 per cabal)         ALPN multiplex)         today: Cable;
                                            direction: Mere
                                            event DAG)
```

- [`identity`](https://crates.io/crates/identity) —
  `derive_keypair(cabal_key)` produces the per-cabal Ed25519 keypair. The
  master secret never leaves the identity provider; murm only sees the
  derived keypair.
- [`transport`](https://crates.io/crates/transport) — murm is
  `Murm<T: Transport>`; the same code runs against `MemoryTransport` in
  tests and `IrohTransport` in production. Cabal traffic claims its own
  ALPN (`mere/cable/v1` today; will change as the wire layer migrates per
  the substrate brief).
- [`murmuring`](https://crates.io/crates/murmuring) — the protocol-core
  layer. murm dispatches conversation operations to whichever
  `BilateralProtocol` backs a given cabal; murm itself does not encode or
  decode posts.
- [`moothold`](https://crates.io/crates/moothold) — many-to-many federation
  lives there, not here. murm strictly handles bilateral / small-group
  flows.
- [`mere`](https://crates.io/crates/mere) — composes murm into the product.

## Status

Pre-1.0. **Phase 2B is in place**: `Murm` orchestrator, cabal lifecycle,
snapshot-push and accept over the bilateral ALPN, post sign / encode /
decode / verify (via murmuring), and end-to-end roundtrip tests against
both `MemoryTransport` and `IrohTransport`. The `Cabal::send` / `subscribe`
/ `history` surface on `CabalHandle` is functional for in-cabal posts.

Forward direction is tracked in the
[event-DAG substrate brief](https://github.com/mark-ik/mere/blob/main/design_docs/mere_docs/implementation_strategy/2026-05-07_event_dag_substrate_brief.md):

- **Drop Cable wire format.** murm's bilateral cabal *semantics* (per-cabal
  Ed25519 identity, signed-post DAG, channel-scoped history,
  ALPN-multiplexed transport) survive; the wire moves from Cable's binary
  records to CBOR-encoded MereEvents over iroh streams.
- **BLAKE3 unification.** Cabal-id derivation and post hashing migrate
  from BLAKE2b-256 to BLAKE3, in lockstep with the identity swap.
- **Live broadcast.** Today's snapshot-push is one-shot; a live
  "send-as-you-post" channel is a future chunk.
- **Additional `BilateralProtocol` impls** (MLS, Tox, …) can land in
  murmuring later; murm dispatches transparently regardless of which
  protocol backs a given cabal.

## License

MPL-2.0.
