# transport

`transport` is the peer transport layer for the
[mere](https://crates.io/crates/mere) browser. It wraps
[iroh](https://www.iroh.computer) for authenticated, encrypted QUIC streams
between known peers, exposes content-addressed blob storage (BLAKE3 via
`iroh-blobs`), and presents a `Transport` trait the rest of the workspace
consumes generically.

## Design

- **Identity is provider-neutral at the boundary.** Existing Mere consumers can
  pass an `identity::Ed25519Keypair`; sibling applications can pass the raw
  32-byte Ed25519 signing seed from Personae or another provider. In both cases
  the peer ID is derived from the corresponding public key.
- **Streams are byte-oriented.** `Transport::Stream: AsyncRead + AsyncWrite`.
  Higher protocols layer their own framing on top — `murmuring` carries
  signed p2panda-core Operations; co-op sessions carry their own format.
- **ALPNs are explicit.** Each protocol registers its own ALPN string
  (`mere/cable/v1`, `mere/coop/v1`, …) so multiple protocols share one peer
  connection without ambiguity. ALPNs are versioned and tracked in the
  workspace's protocol architecture plan.
- **Generic over implementation.** Consumers take `T: Transport` rather than
  `Box<dyn Transport>`, so the same code runs against real iroh in production
  and against an in-memory transport in tests.

## What's in the crate

- **`transport`** — the public contract.
  - `Transport` trait — `connect(PeerID, Alpn) -> Stream`,
    `accept(Alpn) -> Stream`, `local_peer_id()`. Generic associated type for
    the stream.
- **`p2panda_transport`** — the production implementation.
  - `P2pandaTransport` — backed by `p2panda-net`'s `Endpoint` (the endpoint
    authority). Real QUIC, with p2panda-net discovery, relay/hole-punching,
    and actor supervision. Replaced a hand-rolled iroh `Router`.
  - `P2pandaStream` — the QUIC stream type implementing `AsyncRead + AsyncWrite`.
  - **Gossip + LogSync** — `builder().gossip()` enables a gossip overlay;
    `subscribe(topic) -> GossipHandle` joins a space topic to broadcast /
    receive operations (live convergence for online peers); `set_topics(peer,
    …)` bootstraps the overlay (discovery does this in production). RBSR
    offline catch-up over `p2panda-net`'s `LogSync` is wired: a consumer's
    `p2panda-store` `LogStore` / `TopicStore` reconciles the log with peers
    (murm's cabal store is the first consumer).
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
  derives the transport's peer identity from the identity trust root. External
  providers use `P2pandaTransport::builder_from_seed` or `bind_seed`; no
  separate transport key is generated.
- [`murm`](https://crates.io/crates/murm) — opens a stream per cabal
  conversation, carrying signed p2panda-core Operations over the byte
  stream, and reconciles the cabal log over gossip + LogSync. Each cabal
  claims an ALPN.
- [`moothold`](https://crates.io/crates/moothold) (planned) — uses transport
  streams + iroh-gossip topics for moot-scoped event sync.
- [`eidetic`](https://crates.io/crates/eidetic) — large local-memory
  artifacts (engram payloads, graph snapshots) are stored as content-addressed
  blobs through `BlobStore`; consumed via `iroh-blobs` for cross-device sync
  in multi-device deployments.
- **Bridges** (`mere-bridge-matrix`, `mere-bridge-nostr`, …, planned) — sit
  above transport when their foreign protocol can ride iroh; otherwise
  they bring their own transport.

## Off-grid transport (feature-gated)

A `reticulum` feature (default-off) adds a
[Reticulum](https://reticulum.network/) backend for the bilateral stream
lane, so a murmur can ride LoRa / packet-radio / serial links where there is
no IP network. It is the bilateral stream lane only: sync (gossip / LogSync)
and blob transfer stay iroh-only, since Reticulum's small MTU cannot carry
them. The feature contributes zero compile surface to the default build; the
durable Reticulum implementation is tracked in the external `retinue` repo.

## Status

Pre-1.0. The `Transport` trait, `MemoryTransport` (in-memory test fixture),
and `P2pandaTransport` (production, `p2panda-net`'s `Endpoint` as the
endpoint authority, with gossip + LogSync served off the same endpoint) are
in place. The `reticulum` off-grid backend is present behind its feature,
default-off. Production discovery wiring and a persistent `BlobStore` backend
continue to land.

transport stays the pipe: authenticated QUIC streams, ALPN multiplexing,
content-addressed blobs, and the p2panda-net endpoint the sync lanes ride.
Protocol semantics (the cabal log, folds, tiers) live above it, never here.

## License

MPL-2.0.
