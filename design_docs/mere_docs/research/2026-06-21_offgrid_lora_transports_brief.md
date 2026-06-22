# Off-grid LoRa transports: Reticulum, Meshtastic, MeshCore

**Date**: 2026-06-21
**Status**: Research brief / evaluation. No code proposed. Frames how (and whether)
Mere's p2p layer could reach off-grid radio networks, and the one structural call
that decides the integration shape for all three. External facts verified against
primary sources 2026-06-21 at the confidence levels stated.
**Scope**: three off-grid LoRa stacks (Reticulum, Meshtastic, MeshCore) against
Mere's existing transport seam. Pairs with, and sits beside, the broader p2p survey.
**Related**:

- [`2026-05-31_murm_p2p_landscape_brief.md`](2026-05-31_murm_p2p_landscape_brief.md)
  — the iroh-anchored landscape; treats the wire as swappable and already reserves a
  Veilid-shaped opt-in transport slot. This brief adds the off-grid members of that family.
- [`../implementation_strategy/2026-05-07_event_dag_substrate_brief.md`](../implementation_strategy/2026-05-07_event_dag_substrate_brief.md)
  — sync-as-projection, and the Pattern A native (`mooting-*`) vs Pattern B outbound
  bridge (`mere-bridge-*`) split this brief leans on.
- [`../implementation_strategy/2026-06-03_host_p2p_wiring_plan.md`](../implementation_strategy/2026-06-03_host_p2p_wiring_plan.md)
  — the honest discovery/bootstrap gap in the live host (gossip + mDNS + tickets).
- [`2026-06-15_contact_identity_model_brief.md`](2026-06-15_contact_identity_model_brief.md)
  — key-rooted identity (petname → stable key → endpoints), which all three stacks align with.
- Code: [`transport/src/transport.rs`](../../../crates/murm/transport/src/transport.rs) (the
  `Transport` trait), [`transport/src/p2panda_transport.rs`](../../../crates/murm/transport/src/p2panda_transport.rs)
  (the iroh-concrete `sync_parts()` bypass).

---

## 1. The one distinction that decides everything

These three are not the same kind of thing, and the difference sets where each
plugs into Mere.

- **Reticulum is a networking stack you embed.** A library that supplies addressing,
  routing, links, channels, and crypto over arbitrary interfaces. Mere would *be* a
  Reticulum node. It can implement Mere's transport abstraction.
- **Meshtastic and MeshCore are firmware + device + app ecosystems.** You do not embed
  them to get a generic transport; you become a **client of a node**. Mere talks over
  BLE / serial / TCP to a radio running their firmware and sends messages on their
  terms: their routing, their channel/key model, their small payloads, their duty-cycle
  limits. They own the radio and the mesh; Mere is a tenant.

So Reticulum is a candidate **transport**; Meshtastic and MeshCore are candidate
**bridge targets** (Pattern B, the same shape as a Matrix or Nostr bridge). Conflating
the two is the main error to avoid.

## 2. The seam in code (verified 2026-06-21)

Two facts about the live `transport` crate set the ceiling for any of this.

1. **The bilateral lane is already transport-agnostic.**
   [`transport.rs:46-83`](../../../crates/murm/transport/src/transport.rs#L46-L83) defines
   `Transport` as `connect(peer, alpn) -> Stream` / `accept(alpn) -> Stream`, where
   `Stream` is just `AsyncRead + AsyncWrite`, plus `local_peer_id() -> PeerID`. There is
   already a non-iroh impl in the tree (`MemoryTransport`). This is the seam a transport
   adapter targets. (Confidence: high; read in source.)
2. **The sync lane bypasses the trait.** `SyncedMoot` / `SyncedCabal` / LogSync do not
   consume `Transport`. They take raw iroh types via
   [`sync_parts() -> Option<(Endpoint, Gossip)>`](../../../crates/murm/transport/src/p2panda_transport.rs#L464):
   iroh-gossip flood-fill plus RBSR set reconciliation. That is iroh-concrete and there
   is nothing transport-neutral to hand a different stack. (Confidence: high; read in source.)

Consequence: a new transport can ride the **bilateral** lane through the existing trait,
but **sync does not come along for free**. Lifting sync above a transport-neutral
interface is a separate, larger move.

## 3. Reticulum (the embeddable transport)

The fit case, covered in depth conceptually; this brief records the verified externals.

- **It maps onto `Transport`.** Reticulum destination hash → `PeerID`; destination
  aspect → ALPN; `Link` + `Channel`/`Buffer` → the `AsyncRead + AsyncWrite` stream. Both
  sides are Curve25519/Ed25519-keyed, so identity aligns with Mere's key-rooted model
  rather than fighting it.
- **There is a real Rust implementation.** Beechat's `reticulum-rs` (crate `reticulum`):
  modern async Rust, modular into core / transport / links / channels, the most mature
  non-Python build, with a FOSDEM 2026 talk. (Confidence: medium-high on maturity; young
  and pre-1.0, and upstream is in a leadership transition as the founder steps back.)
- **LoRa hardware path is RNode, not Meshtastic/MeshCore.** Choosing Reticulum gets you
  LoRa through its own RNode firmware. It does not require either appliance for radio.
- **The hard constraint is bandwidth.** Reticulum targets a 500-byte MTU and as little as
  500 bps half-duplex (≤465-byte payload per packet). The current sync design (gossip
  flood, blob-by-hash, CBOR event DAGs) assumes far more. Even after §2's seam work, you
  would want a lean pairwise RBSR tuned for tiny frames, which is a Reticulum-native sync
  *projection* nobody has built. (Confidence: high; primary-source manual.)

## 4. Meshtastic (a bridge target)

- **Model**: controlled-flood mesh, PSK/AES channels plus public-key DMs, protobuf
  messages, a couple hundred bytes per message over the air, optional MQTT internet gateway.
- **Rust client exists**: the official [`meshtastic`](https://crates.io/crates/meshtastic)
  crate (tokio; serial / TCP / BLE), so a bridge is buildable today.
- **What maps**: *murmurs* (short bilateral/group text) and presence/announce. **What does
  not**: moot RBSR sync, blob transfer. It is a narrow off-grid messaging lane.
- **Why bother**: reach. ~40k-star community and many deployed nodes to interoperate with.

## 5. MeshCore (a bridge target, younger, better routing)

- **Model**: hybrid routing (flood for path discovery, then directed unicast),
  public-key-addressed nodes, role-based topology (companion vs repeater vs room server).
  Architecturally closer in spirit to Reticulum than Meshtastic's flood-and-channels.
- **Maturity**: launched early 2025, firmware ~1.14 (2026), smaller community, tooling
  mostly the companion apps; a mature embeddable Rust client is not a given.
  (Confidence: medium; fast-moving young project.)
- **Same integration shape** as Meshtastic (client-of-node bridge), with a cleaner routing
  story and more risk.

## 6. The recommendation

Keep the layers straight; do not build Mere's transport on an appliance.

- **For an embedded off-grid transport, Reticulum is the candidate.** First probe (done
  when it holds): a `ReticulumTransport` implementing the existing `Transport` trait over
  `reticulum-rs` (Link behind `connect`/`accept`, destination-hash `PeerID`, aspect ALPN),
  proving the identity and stream mapping on the **bilateral lane only**. Explicitly not
  trying to run gossip/RBSR over it.
- **Sync over Reticulum is a separate, later problem.** A Reticulum-native lean pairwise
  RBSR projection plus announce-based discovery (Reticulum's announce mechanism is a decent
  fit for the off-grid discovery gap the host plan flagged). Prerequisite: §2's lift of sync
  above a transport-neutral interface.
- **Meshtastic and MeshCore are bridge targets, not transports.** A Pattern B `mere-bridge-*`
  relaying the murm/announce slice to and from a local node, accepting the message-oriented,
  small-payload, lossy reality. Priority: Meshtastic first if the goal is reach (mature Rust
  crate, large network); MeshCore if the goal is the routing model and you accept tracking a
  young project. Both rank behind the Reticulum transport probe.

Framing: Reticulum answers "can Mere itself speak an off-grid stack." Meshtastic and
MeshCore answer "can Mere talk to the off-grid meshes people already run." Different
questions, different seams.

## 7. Pitfalls

- **Do not generalize `sync_parts()` for this alone.** Sync is iroh-shaped below the trait;
  lifting it is justified by the broader sync-as-projection goal, with Reticulum as one
  beneficiary, not as a one-off.
- **Honest reach only.** A bridge to a small-payload mesh carries murmurs and announces, not
  the federation's sync. Surface that boundary to users (the real-feedback rule), never imply
  full sync over LoRa.
- **Pre-1.0 dependency posture.** Hold `reticulum-rs` and MeshCore at the corpus's
  watch/contained-experiment stance until they prove out; do not let either gate shipping.

## Findings

### 2026-06-21 — the transport-vs-bridge split is the whole answer

- The `Transport` trait is already abstract enough for a Reticulum adapter; sync is not,
  because it bypasses the trait to iroh's `(Endpoint, Gossip)`.
- Reticulum is an embeddable stack (fits the transport seam, RNode for radio); Meshtastic and
  MeshCore are client-of-node appliances (fit the Pattern B bridge seam).
- The shared hard limit is bandwidth: a ~200–465 byte payload world rules out running the
  existing gossip/RBSR/blob sync over any of them, independent of the seam work.

## Progress

### 2026-06-21

- Brief created from a session Q&A, grounded in a read of `transport/src/transport.rs` and
  `p2panda_transport.rs`, plus primary-source verification of `reticulum-rs` (Beechat /
  crates.io `reticulum`), the Reticulum manual (MTU/bandwidth), the `meshtastic` Rust crate,
  and the MeshCore project. No code change. Next concrete step, if pursued: the §6
  bilateral-lane `ReticulumTransport` probe.

## Sources (external, verified 2026-06-21)

- Reticulum-rs (Beechat): <https://github.com/BeechatNetworkSystemsLtd/Reticulum-rs>;
  crate: <https://crates.io/crates/reticulum>; FOSDEM 2026 talk:
  <https://fosdem.org/2026/schedule/event/KF7STF-reticulum-rs_porting_the_trustless_mesh_from_python_to_rust/>
- Reticulum (manual, MTU/bandwidth): <https://reticulum.network/manual/understanding.html>;
  reference stack: <https://github.com/markqvist/Reticulum>
- Meshtastic Rust client: <https://github.com/meshtastic/rust>;
  client API: <https://meshtastic.org/docs/development/device/client-api/>
- MeshCore: <https://github.com/meshcore-dev/MeshCore>;
  comparison: <https://www.seeedstudio.com/blog/2026/03/23/meshcore-vs-meshtastic/>
