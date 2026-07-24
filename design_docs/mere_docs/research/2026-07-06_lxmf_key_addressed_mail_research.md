# LXMF and Key-Addressed Store-and-Forward Mail — Research Brief

**Status (2026-07-06):** research complete; recommendation below. The front-loaded G6 of
the [comms gating plan](../implementation_strategy/2026-07-06_comms_gating_and_key_addressing_plan.md),
pulled ahead of G5 because the answer shapes the key-addressed misfin lane. Web-grounded
2026-07-06; local grounding is `crates/murm/transport/src/reticulum_transport.rs`.

## What LXMF is (verified against spec/README)

Lightweight Extensible Message Format: Mark Qvist's messaging protocol over Reticulum.
Message = 16-byte destination hash + 16-byte source hash + 64-byte Ed25519 signature +
msgpack payload `{timestamp, content, title, fields}`; total mandatory overhead 111
bytes. The message id is `SHA-256(destination + source + payload)`, derived rather than
transmitted; propagation storage uses a transient id over the encrypted form. So LXMF
mail is **content-addressed and signed**, structurally the same shape as a Mere engram
envelope.

Three delivery methods: **opportunistic** (single routed packet), **direct** (an
end-to-end link), **propagated** (hand the encrypted message to a propagation node).
Propagation nodes peer with each other and sync, forming a distributed encrypted
message store; a recipient retrieves from any node. Spam control is **stamps**:
configurable proof-of-work (0 to 32 bits) validated on reception and at propagation
ingestion, with recipient-issued tickets as exemptions.

Crypto rides Reticulum's layer entirely: X25519 ECDH with ephemeral keys, Ed25519
signatures, AES-256-CBC (RNS 1.x; older docs say AES-128), forward secrecy by default.
RNS hit 1.0 in 2025 and is at 1.3.x as of June 2026. **Not externally audited** (their
own statement).

## The Rust ecosystem finding (this is what changed the calculus)

Two independent Rust Reticulum stacks now exist:

- **Beechat `reticulum` 0.1.0** — what our probe pins. Last release 2025-10-14, so
  roughly nine months stale. Transport-level only, no LXMF.
- **FreeTAKTeam `LXMF-rs` monorepo** — `lxmf` 0.6.0 released **2026-06-30**, EPL-2.0,
  ~7k LOC, active release train. Umbrella over `lxmf-wire` (message primitives),
  `lxmf-sdk` (start/send/cancel/status/poll), and its own `reticulum-rs-core` /
  `-transport` / `-rpc` stack, plus `lxmd`/`reticulumd` daemons. Explicitly "not a
  complete drop-in replacement for every Python Reticulum/LXMF behavior"; maintains
  pinned Python interop evidence and mixed Rust/Python smoke runs; client compat with
  Sideband/MeshChat is "separate interop gate evidence", not guaranteed.

So "LXMF in Rust means implementing it ourselves" is no longer true, but adopting it
means choosing the FreeTAK stack, and running two Rust RNS stacks would be absurd, so
adoption implies migrating the probe off Beechat.

**License check:** EPL-2.0 is weak copyleft, module-scoped; consuming it as a
dependency from MPL-2.0 code is fine. No obligation leaks into Mere's own crates.

## What our probe already provides

`reticulum_transport.rs` (feature-gated, default off) already solves the parts an LXMF
lane would need from us: deterministic dual-key Reticulum identity stretched from the
master seed via HKDF-SHA256 (same seed, same destination — persona-keying is just a
salted derivation away), announce-based address book, ALPN-to-destination mapping,
bilateral streams. Scope stops there: sync and blobs stay iroh-only.

## The three options, weighed

**A — adopt LXMF-the-protocol (lxmf-rs).** Buys interop with the
Sideband/MeshChat/NomadNet world and a spec'd, implemented store-and-forward tier.
Costs: a second crypto suite (RNS: AES-256-CBC/X25519, unaudited) beside the mere lane
(XChaCha20-Poly1305/BLAKE3/Argon2id, p2panda-encryption planned); a stack migration
(Beechat → reticulum-rs); a sub-1.0 dependency whose Sideband interop is not yet
gated-proven; and mail addressing (RNS destination hash) that no non-Reticulum peer
can dial.

**B — adopt LXMF-the-blueprint over iroh.** Reimplement the pattern with our
primitives: sealed content-addressed mail objects (the engram envelope already has the
shape), wrapped to recipient persona keys, handed to store-and-forward nodes, synced
on reconnect. One crypto stack, persona keys native, no interop. The decisive fit: a
propagation node is **the mail instance of voluntary hosting**. Mere does not need
proof-of-work stamps for its core case, because ingestion can be gated by tessera
standing and capability (the moot gate is a better spam economics than CPU burn);
stamps only matter for open-world strangers, which is exactly the boundary the gate
already owns. TTL/quota/peering among store nodes is real design work, but it overlaps
the moot-tier hosting machinery (HostingCommitment, reciprocity) nearly one-to-one.

**C — layered (recommended).** Direct key-addressed mail = misfin-over-iroh (G5,
unchanged). Offline tier = option B, specified as a voluntary-hosting service inside
Mere's trust fabric. Protocol-level LXMF = an **optional interop bridge** on the
existing reticulum feature lane, built only on real demand, at which point the probe
migrates from Beechat 0.1.0 to the active reticulum-rs stack. Rationale: interop with
the Reticulum world is a bridge feature, not a foundation; letting LXMF's carrier and
crypto choices constrain Mere's core mail design would invert that. What we *should*
take from LXMF now is its design vocabulary: content-derived message ids, the
transient-id for encrypted-at-rest storage, retrieve-from-any-node, and the
stamp/ticket idea held in reserve for open-world ingestion.

## Recommendation

Take C. Concretely for the comms plan: G5 proceeds as specified; G6 graduates from
"research" to a design slice named **store-and-forward mail as voluntary hosting**,
blueprint-shaped per option B, sequenced after G5 and gated on the moot hosting
substrate.

**Addendum (2026-07-06, same day):** the bridge prerequisite changed after a
stewardship check. The Reticulum **protocol is public domain**; the reference
implementation's license changed in April 2025 (MIT plus an anti-AI clause), Mark
Qvist stepped back from RNS development in December 2025, and forks exist (RetiNet,
Reticulum_CE). Decided with Mark: if the reticulum lane gets real investment, Mere
stewards its **own endpoint-scoped Rust implementation** from the public-domain
spec (interop-tested against the Python reference as a black-box oracle) rather
than adopting Beechat or FreeTAK long-term. Details and reference-license
discipline live in the
[reticulum transport plan](../implementation_strategy/2026-06-29_reticulum_transport_plan.md)
Direction section.

**Addendum (2026-07-24):** the landscape shifted under option C's bridge
clause. Retinue now exists (R0-R7, oracle-verified against the Python
reference; reliable link and byte-exact Resource transfer over real RF,
2026-07-21/23), so the bridge prerequisite is no longer migrating to a
third-party stack: an LXMF codec would be a small spec-based sibling in the
radio workspace, the same clean-room posture as sennet. Two demand arguments
recorded in the 2026-07-24 application brainstorm: day-one interop for Merely
radios (a freshly flashed radio can message existing Sideband/MeshChat users,
answering the hardware cold-start problem), and LXMF propagation as an
offered role in the
[managed-network plan](../implementation_strategy/2026-07-24_low_power_managed_network_plan.md)'s
V9 offers, its propagation store matching V10's store-and-forward with
expiration. Posture otherwise unchanged: LXMF stays a boundary format and an
offered service; the internal spine is the shared engram (see the
[shared-engram commons brief](2026-07-24_shared_engram_commons_brief.md)),
and LXMF's message-shaped model must not leak inward, the same discipline
sennet holds against Meshtastic framing.

## Sources

- LXMF spec/README: github.com/markqvist/LXMF (message structure, delivery methods,
  propagation, crypto posture)
- unsigned.io/lxmf (protocol overview)
- Reticulum manual 1.3.5 (reticulum.network/manual): RNS 1.0 timeline, AES-256-CBC,
  X25519/Ed25519
- FreeTAKTeam/LXMF-rs (github): crate architecture, 0.6.0 train, parity caveats,
  interop evidence posture
- crates.io API: `reticulum` 0.1.0 (2025-10-14, BeechatNetworkSystemsLtd),
  `lxmf` 0.6.0 (2026-06-30, FreeTAKTeam)
- Reticulum community wiki: propagation nodes vs transport nodes; stamp cost
  validation (`validate_pn_stamps`)
