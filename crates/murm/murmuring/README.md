# murmuring

`murmuring` is the protocol-core layer for bilateral protocol selection
in the [mere](https://crates.io/crates/mere) browser, within [`murm`](https://crates.io/crates/murm).
Today: Cable. Forward: a Mere-native event DAG over CBOR (per
the substrate brief).

## Naming

The gerund form (*murmuring* = the act of murmuring) names the protocol
plumbing. The singular noun (*murmur* = a single bilateral conversation) is
the user-facing term in `murm` and `mere`.

## What's in the crate

- **`BilateralProtocol`** (trait) — the contract concrete protocols
  implement. Object-safe; `name()`-shaped today.
- **`Post` family** — Cable-shaped post types from the inherited Cable spec
  §2.4:
  - `Post` envelope (author pubkey, links, signature, kind).
  - `PostKind` — six variants: `Text`, `Delete`, `Info`, `Topic`, `Join`,
    `Leave`.
  - `PostId` — 32-byte BLAKE2b hash today; BLAKE3 per the in-flight
    migration.
  - `ChannelName` — validated channel-name newtype (length / control-char
    rules).
  - `InfoEntry` — name / metadata claim within an `Info` post.
- **`cable` module** — the Cable concrete protocol.
  - `CableEngine` — the per-IdentityProvider engine; opens cabals, derives
    per-cabal keypairs, ingests and queries posts.
  - `wire` — varint-prefixed binary post encoding (Cable §2.4).
  - `sign` — Ed25519 sign / verify over post bytes (`sign_post`,
    `verify_post`).
  - `hash` — BLAKE2b post hashing (`hash_post`).
  - `store` — in-memory cabal post store.
  - `persistent_store` — redb-backed cabal post store for durability.
  - `varint` — LEB128 codec for the wire framing.
- **`MurmuringError`** — unified error type.

## How it relates to other workspace crates

murmuring sits above [`identity`](https://crates.io/crates/identity)
(for per-cabal keypair derivation) and below
[`murm`](https://crates.io/crates/murm) (which wraps it with cabal lifecycle
and transport orchestration). It does not depend on transport directly:
wire bytes are produced and consumed via `encode_post` / `decode_post`, and
the calling layer moves them.

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
       (trait — abstract           CableEngine, wire, sign,
        over concrete              hash, store, persistent_store
        protocols)                          │
                                            ▼
                                      identity
                              (derive_keypair for per-cabal Ed25519)
```

- [`identity`](https://crates.io/crates/identity) —
  `CableEngine::new(Arc<dyn IdentityProvider>)` consumes the identity trust
  root for per-cabal keypair derivation. The master secret never leaves the
  provider.
- [`murm`](https://crates.io/crates/murm) — wraps `CableEngine` with cabal
  lifecycle, transport orchestration, and snapshot push / accept. murm
  holds the engine; murmuring provides the protocol logic.
- **Transport is *not* a dep.** murmuring doesn't know about iroh / streams
  / ALPNs. murm bridges wire bytes through
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

Pre-1.0. **Phase 2B is in place**: `CableEngine`, post sign / encode /
decode / verify / hash, in-memory and redb-backed cabal stores, `Post`
family (six `PostKind` variants per Cable §2.4), `BilateralProtocol` trait.
End-to-end tested via [`murm`](https://crates.io/crates/murm)'s roundtrip
suite over both `MemoryTransport` and `IrohTransport`.

Forward direction is tracked in the
[event-DAG substrate brief](https://github.com/mark-ik/mere/blob/main/design_docs/mere_docs/implementation_strategy/2026-05-07_event_dag_substrate_brief.md):

- **Drop Cable wire format.** The `cable` module's wire codec is replaced
  by a Mere-native event DAG with CBOR-encoded events. The `Post` *semantic*
  model (DAG of signed posts, channels, info posts, time-range sync)
  survives; the wire format does not.
- **BLAKE3 unification.** Post hashing (`hash_post`) and any internal hash
  uses migrate from BLAKE2b-256 to BLAKE3, in lockstep with the
  identity derivation swap.
- **Causal-DAG management** (links resolution, head tracking) and
  **channel time-range request sync** continue to land. These are
  protocol-level features, not wire-format, and survive the wire migration
  unchanged.
- **Additional `BilateralProtocol` impls** (MLS, Tox, …) may land as
  feature-gated submodules.

## License

MPL-2.0.
