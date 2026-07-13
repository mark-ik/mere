# MURM AS BILATERAL

**Purpose**: Specification for Murm — the Mere workspace's bilateral peer-to-peer comms supercrate. Murm represents the user in one-to-one and small-group communications with known peers, across pluggable protocols.

**Document Type**: Behavior and role specification (Phase 1 boundary doc per the Cable migration plan).

**Status**: Draft / canonical direction.

**Date**: 2026-05-04.

**Related**:

- [`../implementation_strategy/2026-05-04_cable_migration_from_verso_plan.md`](../implementation_strategy/2026-05-04_cable_migration_from_verso_plan.md) — phased migration from Verso (this doc resolves the Phase 1 deliverable)
- [`../../2026-05-04_lexicon_brief.md`](../../2026-05-04_lexicon_brief.md) — current naming + in-product lexicon
- [`../../TERMINOLOGY.md`](../../TERMINOLOGY.md) — canonical terminology
- Inherited: [`cable_coop_minichat_spec.md`](../../../../graphshell/design_docs/verso_docs/implementation_strategy/2026-03-28_cable_coop_minichat_spec.md) — Cable adoption plan (predecessor; Cable migrates from Verso to Murm per the migration plan)
- Inherited: [`VERSO_AS_PEER.md`](../../../../graphshell/design_docs/verso_docs/technical_architecture/VERSO_AS_PEER.md) — pre-migration Verso role spec (Murm inherits the bilateral half of Verso's prior responsibilities)
- Inherited: [`COMMS_AS_APPLETS.md`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/social/comms/COMMS_AS_APPLETS.md) — Comms applet surface family (consumes Murm + Moothold)

---

> **Status update (2026-05-31).** This doc predates two later shifts and now reads partly stale. Read it with these corrections, tracked in the [murm/p2p landscape brief](../../mere_docs/research/2026-05-31_murm_p2p_landscape_brief.md):
> - **Built reality.** `transport` (iroh QUIC + blobs + gossip), `murmuring`/`murm` (Cable bilateral sync), and a `persona/identity` vault skeleton plus passphrase backend are built and tested. The moot/federation tier is a 61-LOC stub.
> - **The substrate pivot is DONE (2026-06-01/02):** the Cable wire was replaced by p2panda operations (Cable varint wire deleted, `IrohTransport`→`P2pandaTransport`) and BLAKE2b→BLAKE3 unified, keeping bilateral semantics in murm. This supersedes the "executed neither" reading. **Crate renames since:** `mere-identity`→`persona/identity`, `mere-transport`→`murm/transport`, `graphshell`→the `shell` crate family / `meerkat` host; the crates.io links to those names below are stale (path-deps now, not published). The body uses the old names + BLAKE2b as dated receipts.
> - **The §1 boundary axis was re-derived, then partly relaxed.** Protocol-plan §5.0 replaced "1:1 vs many-to-many" with "ad-hoc peer vs durable group" and proposed moving Misfin out of murm. **Resolved 2026-06-01 (Mark):** Misfin and other smolweb *exchange* protocols stay in murm once they reach Misfin's implementation level and jibe with murm. The operative murm axis is therefore *bilateral exchange with a known endpoint* (sync or store-and-forward, which includes mail), not strictly "ad-hoc real-time peer." Pure fetch/render protocols stay nematic engines; murm owns the client/exchange side. So `misfin` under `crates/murm/` is correct, not a contradiction.

## 1. What Murm Is

Murm is the **bilateral peer-to-peer comms supercrate** in the Mere workspace. It owns:

1. **Bilateral identity derivation** — per-protocol Ed25519 keypairs derived from the master keypair (which lives in `mere-identity`).
2. **Bilateral chat application logic** — Cable-shaped append-only feeds, channel sync, post types, moderation seeds. Concrete protocols are implemented under [`murmuring`](https://crates.io/crates/murmuring) as pluggable modules.
3. **Co-op session orchestration** — host-led sessions with known guests, session lifecycle (join, leave, snapshot, archive), session-scoped chat lanes. (Co-op stays in murm despite multi-party state; the *transport* and *trust model* are bilateral.)
4. **Cabal storage** — per-cabal post stores using fjall + redb + rkyv (the same persistence stack as the rest of the workspace).
5. **Local moderation state** — per-user hide/mute/block, with per-protocol propagation rules.

Murm does NOT own:

| Concern | Owner |
|---------|-------|
| Master keypair, OS keychain integration | [`mere-identity`](https://crates.io/crates/mere-identity) |
| iroh transport, ALPN, QUIC connection management | [`mere-transport`](https://crates.io/crates/mere-transport) |
| Engine management (Wry, Genet, Nematic) | [`inker`](https://crates.io/crates/inker) |
| Rendering surfaces (GraphTree tiles) | [`verso-tile`](https://crates.io/crates/verso-tile) |
| Many-to-many community/federation comms (Matrix, Nostr, IRC, ATproto, ActivityPub, native moot infra) | [`moothold`](https://crates.io/crates/moothold) + [`mooting`](https://crates.io/crates/mooting) |
| Shell layer, host GUI, Navigator surface | [`graphshell`](https://crates.io/crates/graphshell) |
| Composition / graph-aware layout | [`platen`](https://crates.io/crates/platen) |
| Private local accumulated browsing memory | mnem (planned) |
| User-facing chat panel UI / Comms applet | [`COMMS_AS_APPLETS`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/social/comms/COMMS_AS_APPLETS.md) (Graphshell-side) |

**Architectural boundary** (the rule of thumb from the migration plan):

> If it's about *engines, rendering, or browser-viewer concerns*, it stays in inker / verso-tile / nematic.
> If it's about *person-to-person communication via a bilateral protocol*, it lives in Murm.
> Identity straddles both domains — it lives in `mere-identity` (foundational), and both Murm and `mere-transport` consume it.

---

## 2. Murm as Identity Consumer

### 2.1 Master keypair (consumed from `mere-identity`)

The user's master Ed25519 keypair lives in `mere-identity`. Murm never holds the master secret directly; it receives derived per-protocol keys via a derivation API.

### 2.2 Per-protocol derivation

Each bilateral protocol that needs its own Ed25519 keypair (Cable's per-cabal-key requirement is the canonical example) derives via the pattern from the inherited Cable spec §2.2:

```text
session_seed = BLAKE2b(master_secret || protocol_specific_salt)
session_keypair = Ed25519::from_seed(session_seed)
```

Protocol-specific salts:

- **Cable**: `cabal_key` (32 bytes, derived per Cable spec §2.3 from session id + host pubkey, or randomly generated for named cabals)
- **MLS** (future): MLS group identifier
- **Other** (future): defined per protocol module

### 2.3 Identity API surface (illustrative — signature-only, not implementation-ready)

```rust
// In mere-identity (illustrative, signature-only)
pub trait IdentityProvider {
    /// Returns the master public key (NodeId is derived from this).
    fn master_public_key(&self) -> Ed25519PublicKey;

    /// Derives a per-protocol keypair from a protocol-specific salt.
    /// The master secret never leaves the IdentityProvider.
    fn derive_keypair(&self, salt: &[u8]) -> Result<Ed25519Keypair, IdentityError>;
}
```

Murm depends on this trait, not on a concrete implementation. The graphshell-provided keychain backend implements it; tests use an in-memory fake.

---

## 3. Murm as Transport Consumer

### 3.1 iroh streams (consumed from `mere-transport`)

Murm uses `mere-transport` for authenticated, encrypted QUIC streams to known peers. Cable's wire protocol rides directly on these streams; Cable's own Noise handshake is **not** used (per Cable spec §2.1 Option A — skip Noise, use iroh's encryption, verify cabal key at application layer).

### 3.2 ALPN registration

Each Murm protocol that needs its own stream identification registers an ALPN with `mere-transport`:

- `mere/cable/v1` for Cable
- `mere/mls/v1` for MLS (future)
- `mere/coop/v1` for co-op session presence/state (distinct from chat ALPN)

ALPN registration is `mere-transport`'s responsibility; Murm provides the protocol-id strings.

### 3.3 Transport API surface (illustrative)

```rust
// In mere-transport (illustrative, signature-only)
pub trait Transport {
    async fn connect(&self, peer: NodeId, alpn: &[u8]) -> Result<Connection, TransportError>;
    async fn accept(&self, alpn: &[u8]) -> Result<Connection, TransportError>;
    fn register_alpn(&mut self, alpn: &[u8], handler: AlpnHandler);
}
```

Murm protocol modules consume `Transport`, never iroh directly.

---

## 4. Per-Protocol Module Pattern

### 4.1 Murmuring crate as protocol-core

[`murmuring`](https://crates.io/crates/murmuring) is the inner crate that hosts concrete protocol modules. Each protocol implements a common trait so Murm consumers see a unified API regardless of which protocol carries any given murmur.

### 4.2 Protocol-module trait (illustrative)

```rust
// In murmuring (illustrative, signature-only)
pub trait BilateralProtocol {
    type Conversation;
    type Message;

    /// Open or join a bilateral conversation with the given peer.
    async fn open(&self, peer: NodeId) -> Result<Self::Conversation, ProtocolError>;

    /// Subscribe to incoming messages on a conversation.
    fn subscribe(&self, conv: &Self::Conversation) -> impl Stream<Item = Self::Message>;

    /// Send a message on a conversation.
    async fn send(&self, conv: &Self::Conversation, body: MessageBody) -> Result<(), ProtocolError>;

    /// Native-order accessor (for protocols where order matters).
    fn native_order(&self, conv: &Self::Conversation) -> NativeOrdering;
}
```

Per [`feedback_spec_code_samples_illustrative_vs_implementation_ready`](C:\Users\mark_\.claude\projects\c--Users-mark--Code\memory\feedback_spec_code_samples_illustrative_vs_implementation_ready.md), this trait sketch is **illustrative-signature-only**. The concrete trait shape is a Phase 2 deliverable once Cable's actual concrete implementation surfaces the right associated types.

### 4.3 Cable as the first concrete module

Cable is the first concrete `BilateralProtocol` implementation in `murmuring`. Cable migrates from `graphshell/ports/verso/cable/` to `mere/crates/murmuring/src/protocols/cable/` per the migration plan Phase 2.

Cable-specific machinery in `murmuring/src/protocols/cable/`:

- Wire protocol encoding/decoding (per Cable spec §2.4–§2.6)
- Post types: `post/text`, `post/delete`, `post/info`, `post/topic`, `post/join`, `post/leave`
- `Channel Time Range Request` sync
- BLAKE2b post-hash computation
- Causal DAG handling (the `links` field)

Cable-specific machinery NOT in `murmuring` (lives at the Murm level instead, since it's not Cable-protocol-mechanics but bilateral-comms-orchestration):

- Per-cabal keypair derivation (uses `mere-identity`'s `derive_keypair`)
- Cabal-key generation/distribution (uses Murm's session-management)
- Cabal storage (Murm's persistent store under `mere-data-dir/murm/cabals/`)
- Moderation seed application (Murm's moderation layer)

### 4.4 Future protocols

Phase 2+ protocol modules expected under `murmuring/src/protocols/`:

- **MLS** (Messaging Layer Security) — for E2EE small-group chat with stronger forward secrecy than Cable
- Possibly **Tox** or other small-group p2p protocols

Each as a sibling module under `murmuring/src/protocols/<name>/`, each implementing `BilateralProtocol`.

---

## 5. Cabal Store

### 5.1 Storage backend

Murm owns its own persistent store, using the same fjall + redb + rkyv stack as the rest of the workspace. The store root is under a Murm-managed subdirectory of the user data dir:

```text
<user_data_dir>/mere/
├── mnem/        ← private browsing memory
└── murm/
    ├── cabals/<cabal_id>/    ← persistent named cabal stores
    └── sessions/<session_id>/ ← ephemeral co-op session stores (cleared on session end unless snapshot-archived)
```

**Murm does NOT write to mnem.** Cable cabal posts are *social* artifacts; if a user wants to promote a post to mnem, that's an explicit clip action (the same shape as web-clip-to-graph-node), not a default cross-domain write.

### 5.2 Schema

Per Cable spec §4.2, each persistent cabal store has these tables (described in fjall/redb terms):

| Table | Key | Value | Purpose |
|-------|-----|-------|---------|
| `posts` | `Blake2bHash` (32 bytes) | encoded `Post` | Primary post store; deduplication |
| `channel_timeline` | `(channel_name, timestamp_ms, Blake2bHash)` | `()` | Time-range request index |
| `channel_heads` | `channel_name` | `Vec<Blake2bHash>` | Current DAG heads per channel |
| `channel_state` | `channel_name` | `ChannelStateProjection` | Materialized topic + member info |
| `peer_vectors` | `PublicKey` (32 bytes) | `VersionVector` | Per-peer sync state |

### 5.3 Ephemeral vs persistent

- **Ephemeral mode** (Mode A in Cable spec §4.1): co-op session minichat. In-memory or session-temp storage, discarded at session end unless snapshot-archived.
- **Persistent mode** (Mode B in Cable spec §4.2): named cabals. fjall+redb backed, retained indefinitely subject to per-cabal TTL policy.

### 5.4 Garbage collection

- Default for ephemeral: discard at session end
- Default for persistent: indefinite retention
- User-configurable per-cabal TTL knob is a Phase 5 deliverable (see migration plan Phase 5)

---

## 6. Co-op Session Integration

Co-op sessions stay in Murm despite their multi-party state shape. The transport (iroh) and trust model (host-led, all participants known and admitted) are bilateral. The state-shape overlap with moothold (multi-party governance, role gating) does not override the substrate.

### 6.1 Session lifecycle

Inherited from `coop_session_spec.md`:

- Host creates a session, generates `CoopSessionId` (UUID)
- Guests admitted via invite (QR code, invite link, mDNS) — pairing flow lives in `mere-transport`
- Session-scoped chat lane uses Cable with a session-scoped cabal key (derived from `CoopSessionId` + host pubkey per Cable spec §2.3)
- Session ends → in-memory store discarded OR snapshot-archived as a `SessionCapsule`

### 6.2 Promotion to persistent cabal

A co-op session may opt into a *named cabal* backing rather than ephemeral mode (per Cable spec §4.3 Table). When promoted:

- The session uses an existing or newly-created named cabal's key instead of deriving a session-scoped one
- Posts are persisted into the named cabal store
- Past sessions with the same cabal show their history

### 6.3 Co-op vs moothold

The natural future question is whether a co-op session can promote to a moot when participants want long-term shared graph collaboration. This is **Phase 2+ research**, not Phase 1 spec. For now, co-op stays bilateral (Murm), moots stay community (Moothold), and any promotion path is undefined.

---

## 7. Moderation Model

Per Cable spec §3, applied at the Murm level (not specific to Cable):

### 7.1 Host as admin seed

For co-op sessions: the host's session public key is the sole admin seed. The host's moderation actions propagate by default; guests can override locally per Cable's local-user-supremacy rule.

For named cabals: admin seeds are configurable per cabal (founder-admin, multi-admin council, etc.). Phase 5 deliverable.

### 7.2 Local vs shared moderation

Two modes per moderation action:

- **Shared**: signed and published to other peers (e.g. host-issued bans propagate)
- **Local-only**: encrypted, never synced (e.g. a guest's personal mute of another guest)

Per Cable spec §3.4, both are first-class. Local-only is the default for guest-initiated actions.

### 7.3 Moderation actions

| Action | Effect |
|--------|--------|
| `hide_user` | Locally mute a participant's chat messages |
| `hide_post` | Locally hide a specific message |
| `drop_post` | Remove a message from local storage AND display |
| `block_user` | Stop syncing chat posts with/from a participant (chat-layer only; does not affect graph authority) |

---

## 8. Public API Surface (Murm consumer view)

Illustrative — signature-only, not implementation-ready:

```rust
// In murm (illustrative, signature-only)
pub struct Murm { /* opaque */ }

impl Murm {
    /// Construct with identity and transport providers from the workspace.
    pub fn new(
        identity: Arc<dyn IdentityProvider>,
        transport: Arc<dyn Transport>,
        store_root: PathBuf,
    ) -> Self;

    /// Open or join a named cabal.
    pub async fn open_cabal(&self, cabal_id: &CabalId) -> Result<Cabal, MurmError>;

    /// Start a new co-op session (host).
    pub async fn host_coop(&self) -> Result<CoopSession, MurmError>;

    /// Join an existing co-op session (guest).
    pub async fn join_coop(&self, invite: &CoopInvite) -> Result<CoopSession, MurmError>;
}

pub struct Cabal { /* opaque */ }

impl Cabal {
    /// Subscribe to incoming messages.
    pub fn subscribe(&self, channel: &str) -> impl Stream<Item = ChatMessage>;

    /// Send a message.
    pub async fn send(&self, channel: &str, body: MessageBody) -> Result<(), MurmError>;

    /// Get historical messages (local, plus pulled from peers).
    pub async fn history(&self, channel: &str, range: TimeRange) -> Result<Vec<ChatMessage>, MurmError>;
}
```

Comms-applet UI (Graphshell-side, [`COMMS_AS_APPLETS`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/social/comms/COMMS_AS_APPLETS.md)) consumes Murm via this public surface. Murm does not own UI; it provides streams + send + history.

---

## 9. Boundary Rules (what Murm does NOT do)

Restating for clarity:

1. **Murm does not own identity.** It calls into `mere-identity` for every keypair operation. The master secret never enters Murm code.
2. **Murm does not own transport.** It uses `mere-transport`'s `Transport` trait. iroh internals never appear in Murm.
3. **Murm does not own UI.** It produces message streams and accepts send commands. The chat panel rendering, focus, layout, etc. live in graphshell-side Comms applet code.
4. **Murm does not write to mnem.** Social posts stay in murm-owned cabal stores. Promotion to mnem is an explicit clip action, not a default cross-write.
5. **Murm does not handle community/federation.** Many-to-many comms (Matrix rooms, Nostr publication, IRC channels, federated moot infrastructure) live in `moothold` + `mooting`.
6. **Murm does not handle engines.** Engine selection, lifecycle, rendering all live in inker / verso-tile / nematic.
7. **Murm does not flatten cross-protocol semantics.** Each bilateral protocol keeps its native ordering, native concept of "conversation", native moderation rules. The trait surface is unified at the API level only.

---

## 10. Phase Boundaries

This boundary doc is the **Phase 1** deliverable from the Cable migration plan. It establishes Murm's authority and its boundaries with `mere-identity`, `mere-transport`, and the rest of the workspace.

Subsequent phases (per the migration plan):

- **Phase 2**: Code-level extraction of Cable from inherited graphshell repo into Murm + Murmuring
- **Phase 3**: Wiring (Mere depends on Murm; Verso-tile no longer owns bilateral chat)
- **Phase 4**: Delete Cable-specific code from inherited graphshell, supersede inherited cable spec
- **Phase 5**: Persistent cabal store schema realization, retention policy UX, multi-channel within cabal

---

## 11. Open Questions Deferred to Phase 2+

1. **Concrete `BilateralProtocol` trait shape** — settles when Cable's actual implementation surfaces the right associated types
2. **MLS adoption decision** — when/whether to add MLS as a sibling protocol module
3. **Co-op-to-moot promotion path** — if a co-op session wants to grow into a long-term shared moot, what's the boundary-crossing model?
4. **Cable.rs upstream contributions** — deferred per [`feedback_upstream_contributions`](C:\Users\mark_\.claude\projects\c--Users-mark--Code\memory\feedback_upstream_contributions.md). Build internal first; revisit later.
5. **Per-cabal data-dir layout details** — final form of subdirectory conventions, snapshot/checkpoint mechanics, atomicity guarantees during sync
6. **Sodiumoxide vs ed25519-dalek/blake2** — Cable spec §5.2 noted the cable.rs upstream uses sodiumoxide; mere-identity will likely use ed25519-dalek + blake2 (pure Rust) for consistency with rest of Mere. Phase 2 decision: fork Cable wire-protocol code to use pure-Rust crypto, or accept sodiumoxide as a transitive dep.
7. **async-std vs tokio** — Cable spec §5.2 noted cable.rs uses async-std; Mere uses tokio. Phase 2 decision: implement Cable's tokio-native peer manager in murmuring (recommended per migration plan), or use a tokio-compat shim.
