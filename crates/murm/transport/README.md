# transport

`transport` is the peer transport layer for the
[mere](https://crates.io/crates/mere) browser. It wraps
[iroh](https://www.iroh.computer) for authenticated, encrypted QUIC streams
between known peers, exposes content-addressed blob storage (BLAKE3 via
`iroh-blobs`), and presents a `Transport` trait the rest of the workspace
consumes generically.

## Design

- **Identity comes from [`identity`](https://crates.io/crates/identity).**
  A peer's `PeerID` is derived from its master Ed25519 public key. Transport
  never holds the master secret; it consumes the public key for addressing.
- **Streams are byte-oriented.** `Transport::Stream: AsyncRead + AsyncWrite`.
  Higher protocols layer their own framing on top — `murmuring` carries a
  CBOR-encoded MereEvent stream; co-op sessions carry their own format.
- **ALPNs are explicit.** Each protocol registers its own ALPN string
  (`mere/coop/v1`, `mere/moot/v1`, …) so multiple protocols share one peer
  connection without ambiguity. ALPNs are versioned and tracked in the
  workspace's protocol architecture plan.
- **Generic over implementation.** Consumers take `T: Transport` rather than
  `Box<dyn Transport>`, so the same code runs against real iroh in production
  and against an in-memory transport in tests.

## What's in the crate

- **`transport`** — the public contract.
  - `Transport` trait — `dial(PeerID, Alpn) -> Stream`, `accept() -> Stream`,
    PeerID / capability surface. Generic associated types for the stream
    type.
- **`iroh_transport`** — the real implementation.
  - `IrohTransport` — backed by `iroh` (Phase 2C v0). Real QUIC, real
    discovery, real gossip topics.
  - `IrohStream` — the QUIC stream type implementing `AsyncRead + AsyncWrite`.
- **`memory`** — the in-memory test fixture.
  - `MemoryTransport` — paired channels, no network. Used for unit tests of
    higher-level protocols without booting iroh.
- **`peer_id`** — `PeerID` derived from `Ed25519PublicKey`. Named `PeerID` rather than `NodeId` to disambiguate from `kernel`'s graph-node identity (graph-object identity and peer identity are distinct concepts that previously shared the name).
- **`alpn`** — `Alpn` newtype with hash-friendly equality, for the ALPN
  registry pattern.
- **`blobs`** — content-addressed blob storage.
  - `BlobStore`, `BlobHash` (32-byte BLAKE3), `BlobError`. Backed by
    `iroh-blobs::store::mem::MemStore` today; persistent backends land
    behind the same trait.
- **`error`** — `TransportError` (unified error type).
- **Re-exports**: `Ed25519PublicKey`, `IdentityProvider` from `identity`
  so consumers don't need a direct identity dep for basic flows.

## How it relates to other workspace crates

transport sits between [`identity`](https://crates.io/crates/identity)
(for addressing) and the higher-level protocols above; iroh sits below as
the substrate.

```text
                murm     moothold (planned)    eidetic-sync (planned)
                  ▲              ▲                       ▲
                  │              │                       │
                  └──────────────┼───────────────────────┘
                                 │ Transport::dial / accept,
                                 │ ALPN-multiplexed streams,
                                 │ BlobStore for content addressing
                                 ▼
                          transport
                  ┌─────────────────┴──────────────────┐
                  │                                    │
                  ▼                                    ▼
            identity                       iroh + iroh-blobs
            (PeerID from                        (QUIC, content addressing,
             master pubkey)                      gossip topics)
```

- [`identity`](https://crates.io/crates/identity) — `PeerID::from_public_key(master_pubkey)`
  derives the transport's peer identity from the identity trust root. No
  separate transport key is generated.
- [`murm`](https://crates.io/crates/murm) — opens a stream per cabal
  conversation, layered with the Mere-native event DAG (CBOR-encoded
  MereEvents) on top of the byte stream. Each cabal claims an ALPN.
- [`moothold`](https://crates.io/crates/moothold) (planned) — uses transport
  streams + iroh-gossip topics for moot-scoped event sync.
- [`eidetic`](https://crates.io/crates/eidetic) — large local-memory
  artifacts (engram payloads, graph snapshots) are stored as content-addressed
  blobs through `BlobStore`; consumed via `iroh-blobs` for cross-device sync
  in multi-device deployments.
- **Bridges** (`mere-bridge-matrix`, `mere-bridge-nostr`, …, planned) — sit
  above transport when their foreign protocol can ride iroh; otherwise
  they bring their own transport.

## Privacy transport (planned)

Per the [event-DAG substrate brief](../../../design_docs/mere_docs/implementation_strategy/2026-05-07_event_dag_substrate_brief.md),
transport will gain an optional [Veilid](https://veilid.com/) backend
behind a `veilid` feature flag, for moots that declare a
privacy-required transport policy. iroh stays the default; Veilid is opt-in
for communities where membership-graph leakage is unacceptable.

## Status

Pre-1.0. The `Transport` trait, `MemoryTransport` (in-memory test fixture),
and `IrohTransport` (real iroh-backed implementation, Phase 2C v0) are in
place. Gossip-topic surface and persistent `BlobStore` backend continue to
land. Veilid backend is planned but not yet wired.

Forward direction is tracked in the
[event-DAG substrate brief](https://github.com/mark-ik/mere/blob/main/design_docs/mere_docs/implementation_strategy/2026-05-07_event_dag_substrate_brief.md):
transport stays the iroh wrapper; the substrate-level decisions
(Mere-native event DAG over iroh streams, BLAKE3 unification, Veilid as a
per-moot privacy policy, bridges-only for foreign protocols) drive what
lands here.

## License

MPL-2.0.
