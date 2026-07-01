# Reticulum transport plan

Plan for adding an optional `ReticulumTransport` backend to the Mere `transport`
crate (`crates/murm/transport`).

## Goal

Implement a `Transport`-trait backend that runs Mere's bilateral peer-to-peer
lane over [Reticulum](https://reticulum.network/) packet links, using the Beechat
Rust port [`reticulum`](https://crates.io/crates/reticulum) v0.1.0. The probe is
limited to bilateral stream connectivity: one `connect(peer, alpn)` and one
`accept(alpn)` yielding an `AsyncRead + AsyncWrite` stream. Sync (gossip / RBSR
/ LogSync) and blob transfer remain iroh-only for now.

## What is in scope

- A new `ReticulumTransport` module implementing `Transport`.
- Deterministic Reticulum identity derived from the Mere master seed (same seed →
  same Reticulum destination across restarts).
- Authenticated binding between a Reticulum announce and a Mere `PeerID`.
- ALPN → Reticulum destination namespace mapping.
- Bidirectional byte stream over a Reticulum `Link`.
- In-process tests using two transports on a local TCP loopback.
- Optional feature-gating so the dependency is default-off.

## What is out of scope

- Gossip, live-sync, offline catch-up, or blob transfer over Reticulum.
- Serial / RNode / LoRa hardware interfaces for the first probe.
- Meshtastic / MeshCore bridges (a different integration shape).

## Findings

### From source reading (`reticulum` v0.1.0)

- `PrivateIdentity` is a dual-key identity: X25519 `StaticSecret` + Ed25519
  `SigningKey`. The public `Identity` contains both public keys, and the
  destination `address_hash` is computed over both. A Mere `PeerID`, which is a
  32-byte Ed25519 verifying key, does **not** contain the X25519 public key and
  therefore cannot be used to synthesize another peer's destination hash.
- Discovery is announce-based. A peer calls `send_announce(destination,
  app_data)` on its local destination; other peers receive the announce via
  `recv_announces()`. The announce carries the sender's public identity and an
  optional `app_data` blob.
- An outbound link is created with `transport.link(announce.destination.lock().await.desc)`,
  where the descriptor comes from a validated announce.
- Link data arrives as `LinkEvent::Data(payload)` from `out_link_events()`
  (outbound side) or `in_link_events()` (inbound side), and is sent with
  `link.data_packet(payload)` followed by `transport.send_packet(packet)`.
- The `buffer` module exposes only low-level primitives (`StaticBuffer`,
  `OutputBuffer`, `InputBuffer`). There is no `BufferReader`/`BufferWriter` stream
  abstraction in v0.1.0; the stream wrapper must be built directly on link data
  packets.
- `Transport::new(config)` requires a `PrivateIdentity`, a name, and a broadcast
  flag. Interfaces are attached through `iface_manager().lock().await.spawn(...)`.
- The crate uses `ed25519-dalek` 2.1.1 and `tokio` 1.x, matching Mere's existing
  stack. Its build script needs `protoc` for the Kaonic gRPC protobuf files.

### Identity mapping

Mere's master key is a single Ed25519 keypair. Reticulum needs both X25519 and
Ed25519 keys. We derive both deterministically from the 32-byte Ed25519 seed
using HKDF-SHA256 with a Mere-specific context string, producing a reproducible
`PrivateIdentity` for each Mere seed. The Mere `PeerID` is still computed from
only the Ed25519 verifying key, so it is stable with respect to the other Mere
transports.

### Authenticated PeerID binding

Because a destination hash cannot be computed from a `PeerID`, `connect(peer,
alpn)` must learn the peer's destination from an announce. To prevent a peer
from announcing someone else's `PeerID`, every announce's `app_data` carries:

```text
app_data = PeerID || signature
signature  = ed25519_sign(master_signing_key,
                           reticulum_identity_public_keys || PeerID || ALPN)
```

The receiver:

1. Validates the Reticulum announce signature.
2. Parses `PeerID` from the first 32 bytes of `app_data`.
3. Verifies the Mere master signature using the recovered `PeerID` public key.
4. Stores `PeerID → (ALPN, DestinationDesc)` in a local address book.

Only after this binding succeeds can `connect(peer, alpn)` resolve the peer.

### ALPN mapping

Each ALPN string maps to a Reticulum destination name. For example:

- `mere/cable/v1` → `DestinationName::new("mere", "cable.v1")`
- `mere/coop/v1` → `DestinationName::new("mere", "coop.v1")`

Per ALPN, the transport registers an incoming destination and periodically
announces it.

### Stream mapping

`ReticulumStream` implements `AsyncRead + AsyncWrite` over a `Link`:

- Write: buffer bytes, chunk into Reticulum-sized payloads, build data packets
  with `link.data_packet(chunk)`, and send them with `transport.send_packet`.
- Read: a background task drains the appropriate link event receiver and pushes
  payloads into an `mpsc` queue; `poll_read` pulls from the queue.
- Flush: drain the write buffer and await hand-off to the transport.
- Shutdown: close the Reticulum link.

## Progress

- 2026-06-21 — Research brief on off-grid / LoRa transports completed; decided
  p2panda-core can ride Reticulum but p2panda-net cannot run on packet-radio
  interfaces.
- 2026-06-22 — Initial implementation plan drafted and approved.
- 2026-06-29 — Plan corrected after source review:
  - Identity mapping changed from "derive destination from `PeerID`" to
    "deterministically derive full dual-key Reticulum identity from master
    seed".
  - Discovery model changed from "synthesize destination" to "announce-based
    discovery with authenticated `PeerID` binding".
  - Stream implementation acknowledged to use link data packets directly; the
    `buffer` module does not provide a stream abstraction in v0.1.0.
- 2026-06-29 — Dependency and feature flag landed:
  - `reticulum = "0.1"` added to workspace root `[workspace.dependencies]`.
  - Optional `reticulum` feature added to `crates/murm/transport/Cargo.toml`.
  - `cargo check -p transport --features reticulum` passes with `PROTOC` set to
    a local `protoc.exe`.
- 2026-07-01 — **P0/P1 implemented and verified (this is the real green).** A
  review found the 2026-06-29 state was a *false green*: `reticulum_transport.rs`
  was committed but never declared in `lib.rs`, so `--features reticulum` compiled
  the deps and skipped the (non-compiling, half-written) module. The `Transport`
  impl and five functions `bind_inner` called (`derive_identity`,
  `destination_name_for_alpn`, `announce_listener`, `announce_sender`,
  `build_app_data`) did not exist, and `send_announce_now` could never run
  (`master_keypair()` always errored). Fixed and completed against the real
  `reticulum` v0.1.0 source (read from the crate cache):
  - **Wired in.** `#[cfg(feature = "reticulum")] pub mod reticulum_transport;` +
    re-exports in `lib.rs`. Split into a module dir
    (`reticulum_transport/{keys,announce,stream}.rs` + mod root) to stay under the
    600-LOC ceiling.
  - **Transport impl.** `connect` resolves a peer from the announce-populated
    address book, opens an out-link, waits for activation, and bridges it;
    `accept` waits for an inbound-link activation on the ALPN's destination and
    bridges it. `ReticulumStream` is a tokio `DuplexStream` driven by two relay
    tasks (aborted on drop).
  - **`send_announce_now` fixed** by precomputing each ALPN's signed `app_data`
    once at bind and storing it, so no master secret is retained after
    construction.
  - **API corrections vs the Findings above** (real v0.1.0): `Transport::new` is
    sync; `add_destination` takes `&mut self` (so destinations are registered
    *before* the stack goes behind `Arc`); the Reticulum identity is built via
    `PrivateIdentity::new_from_hex_string` from the HKDF-derived 64 bytes (avoids
    adding an `ed25519-dalek` direct dep); sending uses
    `transport.send_to_out_links` / `send_to_in_links`, receiving uses
    `out_link_events()` / `in_link_events()` (`LinkEventData { id, address_hash,
    event }`); **announces carry only the 10-byte name hash**, so ALPN matching
    keys on `as_name_hash_slice()`, not the full 32-byte `Hash`;
    `Ed25519Signature::from_bytes` is infallible (returns `Self`).
  - **protoc is genuinely required.** `reticulum`'s `build.rs` unconditionally runs
    `tonic-build` on the Kaonic gRPC proto (no feature gate), so `protoc` must be
    present to build the feature. It was not on PATH; the build used a pinned
    prebuilt `protoc` via the `PROTOC` env var. This is a real build/CI
    prerequisite, recorded in the risk table.
  - **Verified.** `cargo check -p transport --features reticulum --tests` green
    (15s); `cargo clippy` clean; `cargo test` green — 3 tests, incl.
    `bilateral_round_trip_over_tcp_loopback` (two instances discover each other by
    authenticated announce, establish a link, and round-trip `hello`/`world`),
    finishing in 0.64s. Mere-side changes uncommitted (concurrent meerkat/orrery
    work in the tree).

## Phases and done-conditions

### Phase 0 — API and dependency verification (complete, for real 2026-07-01)

Done when:
- `cargo check -p transport --features reticulum` passes **with the module wired
  into `lib.rs`** (the 2026-06-29 pass did not meet this; the module was orphaned).
- Exact `reticulum` APIs for identity, destination, announce, link, and packet
  events are documented in Findings (corrected 2026-07-01).
- Identity derivation strategy is chosen (HKDF-SHA256 → `new_from_hex_string`).

### Phase 1 — Core transport skeleton (complete, verified 2026-07-01)

Done when:
- `ReticulumTransport` struct owns `reticulum::Transport`, local `PeerID`,
  per-ALPN destinations, and an authenticated peer→destination map. ✓
- `ReticulumStream` implements `AsyncRead + AsyncWrite` over link data packets. ✓
- `Transport` trait is implemented for `ReticulumTransport`. ✓ (`connect` / `accept`)
- Deterministic dual-key identity derivation and ALPN→destination mapping are
  implemented. ✓

### Phase 2 — Tests (mostly complete 2026-07-01: 3 tests green)

Done when:
- TCP loopback round-trip test passes between two transports. ✓
  (`bilateral_round_trip_over_tcp_loopback`)
- Unregistered-ALPN and deterministic-identity/peer-id tests pass. ✓
  (`accept_unregistered_alpn_errors`, `derived_identity_is_stable_and_peer_id_matches`)
- `cargo test -p transport --features reticulum` is green. ✓
- Remaining: a standalone binding-authentication test (a forged/mismatched
  `app_data` must be rejected — currently only exercised implicitly, since
  `connect` succeeds only when the binding verifies).

### Phase 3 — Documentation and decision gate

Done when:
- `crates/murm/transport/README.md` documents the optional backend, the ALPN
  mapping, announce discovery, authenticated binding, and sync limitations.
- This plan's Progress section is updated with implementation results.
- `design_docs/DOC_README.md` is updated.
- Decision recorded: keep feature-flagged, wire into host config, or pause on
  blockers.

## Risks and mitigations

| Risk | Mitigation |
|------|------------|
| `reticulum` v0.1.0 has minimal docs and is pre-1.0 | Source is the spec; keep feature optional and default-off. |
| `protoc` required at build time | Document requirement; set `PROTOC` in dev/CI. |
| No stream abstraction in the crate | Build directly on `LinkEvent::Data` / `link.data_packet()`. |
| `PeerID`-to-destination synthesis is impossible | Use authenticated announce cache instead of synthesis. |
| Identity derivation mismatch | Use deterministic HKDF from the Ed25519 seed; test reproducibility. |
| Lock-heavy `Arc<Mutex<_>>` API | Minimize critical sections; run event drains in background tasks. |
