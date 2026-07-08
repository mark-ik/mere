# murmuring

`murmuring` is the protocol-core layer for bilateral chat within
[`murm`](https://crates.io/crates/murm), in the
[mere](https://crates.io/crates/mere) browser. It carries Cable: a cabal
post is a signed [p2panda-core](https://crates.io/crates/p2panda-core)
`Operation` on the wire (BLAKE3-hashed, CBOR-encoded), reconciled by the
same log-sync machinery the rest of the workspace uses.

## Naming

The gerund form (*murmuring* = the act of murmuring) names the protocol
plumbing. The singular noun (*murmur* = a single bilateral conversation) is
the user-facing term in `murm` and `mere`.

## What's in the crate

- **`BilateralProtocol`** (trait) — the contract concrete protocols
  implement. Object-safe; `name()`-shaped today, with `CableEngine` the sole
  implementation.
- **`Post` family** — the cabal post model:
  - `Post` envelope (author pubkey, links, signature, kind).
  - `PostKind` — six variants: `Text`, `Delete`, `Info`, `Topic`, `Join`,
    `Leave`.
  - `PostId` — 32-byte BLAKE3 hash.
  - `ChannelName` — validated channel-name newtype (length / control-char
    rules).
  - `InfoEntry` — name / metadata claim within an `Info` post.
- **`cable` module** — the Cable concrete protocol over p2panda.
  - `CableEngine` — the per-`IdentityProvider` engine; opens cabals, derives
    per-cabal keypairs, ingests and queries posts.
  - `wire` — the `Post` ↔ p2panda-core `Operation` bridge (CBOR);
    `encode_post` / `decode_post`.
  - `sign` — Ed25519 sign / verify via the p2panda `Header` (`sign_post`,
    `verify_post`).
  - `hash` — BLAKE3 post hashing (`hash_post`).
  - `store` — in-memory cabal post store.
  - `persistent_store` — `PersistentCabalStore`, redb-backed (and also
    runnable in-memory); the store `CableEngine` posts through.
  - `log_store` — the [p2panda-store](https://crates.io/crates/p2panda-store)
    `LogStore` / `TopicStore` implementations over the persistent store, so
    `p2panda-net`'s LogSync (RBSR) reconciles a cabal's posts with peers.
- **`MurmuringError`** — unified error type.

## How it relates to other workspace crates

murmuring sits above [`identity`](https://crates.io/crates/identity)
(for per-cabal keypair derivation) and below
[`murm`](https://crates.io/crates/murm) (which wraps it with cabal lifecycle
and transport orchestration). It does not depend on transport directly:
posts become p2panda operations via `encode_post` / `decode_post`, and the
calling layer moves the bytes.

```text
                       murm
                         │ open_cabal, ingest_post, ingest_operation,
                         │ encode / decode / sign / verify / hash_post
                         ▼
                     murmuring
              ┌──────────┴──────────────────┐
              │                             │
              ▼                             ▼
       BilateralProtocol              cable module
       (trait — abstract           CableEngine, wire (post ↔
        over concrete              Operation), sign, hash, store,
        protocols)                 persistent_store, log_store
                                            │
                                            ▼
                                      identity
                              (derive_keypair for per-cabal Ed25519)
```

- [`identity`](https://crates.io/crates/identity) —
  `CableEngine::new(Arc<dyn IdentityProvider>)` consumes the identity trust
  root for per-cabal keypair derivation. The master secret never leaves the
  provider.
- [`murm`](https://crates.io/crates/murm) — wraps `CableEngine` with cabal
  lifecycle, transport orchestration, and the live gossip + LogSync lanes.
  murm holds the engine; murmuring provides the protocol logic.
- **Transport is *not* a dep.** murmuring doesn't know about iroh / streams
  / ALPNs. murm bridges the operations through
  [`transport`](https://crates.io/crates/transport).

## Why this lives in its own crate

- **Multiple bilateral protocols** (Cable today; MLS, Tox, others later)
  can share infrastructure (`Post` types, `BilateralProtocol` trait, store
  traits) without duplicating effort.
- **`murm` consumes the abstract trait** without pulling in every concrete
  protocol implementation; concrete protocols can be feature-gated.
- **A non-`murm` consumer** (test harness, alternative bilateral layer,
  future research crate) could in principle use `murmuring` directly
  without taking a dependency on cabal lifecycle or transport
  orchestration.

## Status

Pre-1.0. The p2panda substrate pivot has **landed**: posts are signed
p2panda-core Operations (BLAKE3, CBOR), stored through a redb-backed
`PersistentCabalStore` that implements p2panda-store's `LogStore` /
`TopicStore`, so a cabal reconciles over `p2panda-net` LogSync. `CableEngine`,
post sign / encode / decode / verify / hash, the in-memory and persistent
stores, the six-variant `Post` family, and the `BilateralProtocol` trait are
all in place and tested (end-to-end via [`murm`](https://crates.io/crates/murm)'s
roundtrip and two-peer LogSync suites).

Remaining work is protocol-level, not wire-format:

- **Cross-author causal links** — posts author with empty `links` today;
  DAG causality across authors is future work.
- **Log pruning** — `prune_entries` is currently a no-op.
- **A second `BilateralProtocol` impl** (MLS, Tox, …) — the trait stays
  single-impl until a second protocol forces its associated-type surface.

## License

MPL-2.0.
