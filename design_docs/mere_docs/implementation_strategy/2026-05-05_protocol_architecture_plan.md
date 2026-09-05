# Protocol Architecture Plan

**Date**: 2026-05-05
**Status**: Draft / canonical direction (architecture-level; per-protocol module plans branch off). **Partially superseded by the 2026-05-07 briefs:**

- [`2026-05-07_event_dag_substrate_brief.md`](2026-05-07_event_dag_substrate_brief.md) reframes wire-format / sync-layer / schema-locality / privacy-transport / persona-design decisions. §2 (iroh layering), §3 (identity vault), §4 (self-host-with-fallback) of *this* plan remain authoritative.
- [`2026-05-07_moot_tiers_and_voluntary_hosting_brief.md`](2026-05-07_moot_tiers_and_voluntary_hosting_brief.md) reframes §5 (Protocol Mods and Primitive Moot Nodes) through a four-tier scale (orrery → moot → moothold → coalition) with voluntary hosting, cheesecloth pinning, and ILL-shaped reciprocity. Notable lexicon shift: *moothold* now means t3 (federation of moots), *coalition* now means t4 (sovereign coalition of mootholds). The earlier "coalition = federation of moots" framing in this plan is superseded.
**Scope**: Cross-cutting plan governing how Mere composes peer-to-peer transports, identity, identity-discovery (WebFinger), and protocol mods (Cable, Matrix, Nostr, IRC, ATproto, ActivityPub, Misfin, …) into a coherent whole. Defines the layered relationship between Cable and the iroh toolkit, the multi-protocol identity vault, the self-host-with-fallback pattern for identity publication, and the "primitive node" pattern by which any protocol — whether the user's mod is installed or not — can appear in the orrery.

**Drives**: The Mere Phase 2C → Phase 3 work program, plus per-protocol mod specs that branch from this plan.

> **Crate-name + substrate note (2026-06-09 audit):** crate names below predate the 2026-05-19 supercrate naming pass and the `graphshell` dissolution: `mere-identity`→`persona/identity`, `mere-transport`→`murm/transport`, `mere-kernel`→`graph/graph-kernel`, `mere-host-runtime`→`system/session-runtime`. The bilateral substrate has since pivoted Cable→p2panda (the `mere-transport` iroh path retired, BLAKE2b→BLAKE3), so the Cable wire-format and iroh-transport sections are partially superseded by the [p2panda spike](../../archive_docs/2026-06-09_completed_plans/2026-06-01_p2panda_substrate_spike_plan.md). Dated "shipped"/progress receipts below are left as historical record.

**Related**:

- [`../../murm_docs/technical_architecture/MURM_AS_BILATERAL.md`](../../murm_docs/technical_architecture/MURM_AS_BILATERAL.md) — Murm's authority + boundaries (this plan extends Murm with sibling-protocol layers and identity-vault elaboration)
- [`../../murm_docs/implementation_strategy/2026-05-04_cable_migration_from_verso_plan.md`](../../archive_docs/2026-06-09_pivot_superseded/2026-05-04_cable_migration_from_verso_plan.md) — Cable's Phase 0–5 migration plan (this plan's iroh-layering work picks up after Cable Phase 2 ships)
- [`../../2026-05-04_lexicon_brief.md`](../../2026-05-04_lexicon_brief.md) — Mere lexicon (orrery, moot, coalition, kith/kin, tessera)
- [`../../TERMINOLOGY.md`](../../TERMINOLOGY.md) — canonical terms
- Inherited: [`protocol_modularity_and_host_capability_model.md`](../../../../graphshell/design_docs/graphshell_docs/technical_architecture/2026-03-30_protocol_modularity_and_host_capability_model.md) — packaging classes (`CoreBuiltins`, `DefaultPortableProtocolSet`, `OptionalPortableProtocolAdapters`, `NativeFeatureMods`, `NonEngineNetworkLayers`); this plan extends that taxonomy with "primitive moot node" semantics for `NonEngineNetworkLayers` protocols
- Inherited: [`SUBSYSTEM_MODS.md`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/subsystem_mods/SUBSYSTEM_MODS.md) — mod lifecycle authority
- Inherited: [`2026-03-08_unified_mods_architecture_plan.md`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/subsystem_mods/2026-03-08_unified_mods_architecture_plan.md) — mod packaging/loading
- Inherited: [`COMMS_AS_APPLETS.md`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/social/comms/COMMS_AS_APPLETS.md) — comms surface family (consumes Murm + Moothold)
- Inherited: [`coop_session_spec.md`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/system/coop_session_spec.md) — co-op authority

---

## 0. Premises

Four user-confirmed design choices ground this plan:

1. **Iroh-as-toolkit, not iroh-as-monolith.** Cable rides on `mere-transport`'s iroh streams (per Cable spec §2.1 Option A); on top of that, Mere consumes iroh's other primitives — `iroh-blobs`, `iroh-gossip`, `iroh-docs` — as complementary layers, each with its own ALPN, each chosen on its own merits per workload. There is no single "iroh integration"; there is a family of cooperating iroh-shaped surfaces.
2. **Identity is a vault, designed now.** `mere-identity` already owns the master Ed25519 keypair and per-protocol derivation. This plan promotes it to a multi-protocol credential vault: a `Profile` container with per-protocol identity slots (Cable cabal keys, Matrix MXIDs/access tokens, Nostr nsec, ATproto DIDs, IRC SASL, TLS client certs, etc.) over a pluggable storage trait (in-memory, OS keychain, passphrase-encrypted, hardware-token, host-bridge). It is not gated on any single consumer.
3. **Self-host with fallback.** Every protocol that can plausibly be self-hosted (WebFinger publication first, others where the same shape applies) gets two interoperable shipping modes: **Mode 1** — self-hosted via `verso` (the rendering-surface manager runs lightweight server endpoints when the host envelope permits); **Mode 2** — third-party hosting fallback for 24/7 availability when the user's machine isn't reachable. Eventually a decentralized storage time bank fills Mode 2 for users who don't want to lean on third parties. *"Give the people the servers, anywhere, everywhere."*
4. **Primitive moot nodes, accessible regardless of mod install.** Protocols that don't fit the bilateral / native moot patterns (IRC, Matrix, Nostr, …) are first-class **primitive moot nodes** in Moothold. A user can pin a protocol-client node directly into their orrery; that requires the mod. But when a moot *hosts* a protocol-client node (e.g. a Matrix room), that node renders for any moot visitor, regardless of whether they have the mod installed locally — the moot's own infrastructure carries the impression. *"Element matrix rooms on the web, but on IPFS."* Scripting (WebAssembly via Boa, Burn, or a purpose-built Rust DSL) is in-scope for elaborating these primitive nodes.

These four premises run concurrently; none gate the others. The phases below are sequenced by external dependency, not by chronological serialization.

---

## 1. Architecture Overview

### 1.1 Layer cake

```
                   ┌────────────────────────────────────────────────┐
                   │  graphshell + comms applets (UI surfaces)      │
                   └────────────────────────────────────────────────┘
                                          │
                ┌─────────────────────────┴─────────────────────────┐
                │                                                   │
        ┌───────▼─────────┐                              ┌──────────▼────────┐
        │  murm           │                              │  moothold         │
        │  bilateral      │                              │  community        │
        │  (Cable, MLS)   │                              │  (moots, coalitions,│
        └───────┬─────────┘                              │   primitive nodes)│
                │                                        └──────────┬────────┘
                │                                                   │
                └──────────────────┬────────────────────────────────┘
                                   │
                  ┌────────────────▼─────────────────┐
                  │  mere-transport                  │
                  │  Transport trait + iroh impl     │
                  │  (ALPN registration, NodeId,     │
                  │   AsyncRead+AsyncWrite streams)  │
                  └─┬─────────┬─────────┬──────────┬─┘
                    │         │         │          │
            ┌───────▼─┐ ┌─────▼────┐ ┌──▼───────┐ ┌▼──────────┐
            │  iroh   │ │iroh-blobs│ │iroh-gossip│ │iroh-docs │
            │ (QUIC)  │ │BLAKE3 cas│ │HyParView+ │ │CRDT meta │
            │         │ │transfer  │ │PlumTree   │ │protocol  │
            └─────────┘ └──────────┘ └───────────┘ └──────────┘
                                   │
                  ┌────────────────▼─────────────────┐
                  │  mere-identity                   │
                  │  Profile vault, derive_keypair,  │
                  │  pluggable storage backends      │
                  └──────────────────────────────────┘
```

### 1.2 Crate-level summary of new work

| Crate | Phase 2A status | New work in this plan |
|-------|-----------------|------------------------|
| `mere-identity` | Master keypair + Ed25519 derivation; in-memory provider | **Identity vault**: `Profile`, per-protocol slots, storage trait, OS-keychain backend, passphrase-encrypted backend |
| `mere-transport` | `Transport` trait + memory pair test fixture | **iroh transport impl**; ALPN registry (multi-ALPN node); blob/gossip/docs façades |
| `murm` / `murmuring` | Cable engine + persistent store + memory-transport sync | **MLS** module (sibling Bilateral protocol); `iroh-blobs` integration for Cable attachments |
| `moothold` / `mooting` | (Phase 0 reservation only) | **Mooting protocol-mod trait**; **primitive moot node** runtime; matrix/nostr/irc/atproto mods; **moot-hosted impression rendering**; scripting host (WASM) |
| `verso` (verso-tile) | Tile placement | **Verso-as-server**: WebFinger endpoint, primitive-node impression endpoint (self-host mode) |
| `mere` | Workspace root | Compose the above; manage user-data-dir layout for vault + moot stores |

---

## 2. Iroh-as-Toolkit Layering

### 2.1 The four iroh layers and what each carries

All four iroh primitives **live in `mere-transport`**. Murm and Moothold are *consumers*; they obtain blob / gossip / docs handles through the `Transport` API, not by depending on iroh-* crates directly. The "consumer" column below names workloads, not architectural participation.

| Layer | Crate | Lives in | Workloads (consumed via mere-transport API) |
|-------|-------|----------|---------------------------------------------|
| **iroh** (QUIC + NodeId discovery) | `iroh` | `mere-transport` | Authenticated streams to known peers |
| **iroh-blobs** | `iroh-blobs` | `mere-transport` | Cable attachments, engram payloads, fauna archives, large memory-class artifacts. BLAKE3 content-addressed transfer; large binary payloads off the chat-message wire |
| **iroh-gossip** | `iroh-gossip` | `mere-transport` | Cabal presence, moot-membership churn, kith/kin online-state. HyParView + PlumTree pub/sub for low-latency many-to-few state |
| **iroh-docs** | `iroh-docs` | `mere-transport` | Shared moot state (topic, member roster, moderation seeds), co-op session metadata longer-lived than chat. CRDT meta-protocol; small, contention-tolerant, shared-mutable state |

Each layer registers its own ALPN with `mere-transport`. The `mere-transport` `Transport` trait grows a `register_alpn` API that hands incoming streams off to the right consumer.

### 2.2 Why layered, not monolithic

Cable already exists, has its own wire format, and its own security model (cabal keys + per-cabal Ed25519). Replacing Cable with `iroh-docs` would re-implement Cable's existing post-DAG semantics inside a CRDT and lose the cable-spec interop story. Conversely, building all of moothold's shared-state needs *on Cable* would force CRDT-shaped problems through an append-only DAG. Each iroh primitive earns its place by workload fit, not by being "the iroh way."

This plan codifies the choice rule:

| Workload shape | Use |
|----------------|-----|
| Authoritative append-only social messages w/ causal links | Cable |
| Large binary blobs (images, files, engrams over a size threshold) | iroh-blobs |
| Ephemeral state churn (presence, typing, "who's here") | iroh-gossip |
| Shared mutable state with concurrent edits (topic, member roster, moot config) | iroh-docs |
| Strong forward-secrecy E2EE small-group chat | MLS (sibling Bilateral protocol; Phase 3) |
| Direct stream w/ custom protocol | raw iroh stream + new ALPN |

### 2.3 ALPN ledger (canonical)

This plan reserves ALPN strings; later per-protocol plans cite them. Pattern is `mere/<protocol>/v<major>`.

| ALPN | Used by | Status |
|------|---------|--------|
| `mere/cable/v1` | murm Cable | reserved (Phase 2B, in murm `lib.rs` per current code) |
| `mere/coop/v1` | murm co-op session presence/state | reserved |
| `mere/mls/v1` | murm MLS | reserved (Phase 3) |
| `mere/blobs/v1` | iroh-blobs (transitive) | reserved |
| `mere/gossip/v1` | iroh-gossip (transitive) | reserved |
| `mere/docs/v1` | iroh-docs (transitive) | reserved |
| `mere/moothold/primitive-node/v1` | moothold primitive node impression delivery | reserved (§5) |
| `mere/webfinger-bridge/v1` | peer-to-peer WebFinger relay (when verso isn't reachable) | reserved (§4.4) |

Per-protocol mods in `mooting/` register their own ALPNs as needed (`mere/mooting/<protocol>/v1`). Mod-registered ALPNs go through Moothold's mod registry, not directly into `mere-transport` — the registry mediates conflict resolution and capability gating.

### 2.4 Phase boundary

iroh-blobs / iroh-gossip / iroh-docs integrations land **after** Cable's iroh transport (Cable migration plan Phase 2C). They are independent of each other and can ship in any order; first concrete consumer drives priority.

**iroh-docs caution (2026-05-31).** iroh 1.0 (rc.1, 2026-05-27) stabilizes the connection layer plus noq, not the higher protocols; iroh-blobs / iroh-docs / iroh-gossip are separate community crates, and iroh-docs is a meta-protocol layered on the other two. Treat iroh-docs as a swappable projection (it already does RBSR, so adoption is not a sync-efficiency question), with p2panda-sync and willow-rs as fallbacks if its maintenance lags. See the [murm/p2p landscape brief](../research/2026-05-31_murm_p2p_landscape_brief.md).

---

## 3. Identity Vault (mere-identity Phase 2C)

### 3.1 The shape

Today `mere-identity` exposes:

```rust
pub trait IdentityProvider {
    fn master_public_key(&self) -> Ed25519PublicKey;
    fn derive_keypair(&self, salt: &[u8]) -> Result<Ed25519Keypair, IdentityError>;
}
```

This is correct for Cable (per-cabal Ed25519 derivation) but doesn't model the broader credential surface. Phase 2C extends it (additively, with the existing trait preserved) to a **profile vault** with two clearly distinguished slot categories: vault-modelable credentials (just bytes the vault stores) and SDK-delegated credentials (where the vault holds a bootstrap secret and an external SDK owns its own state directory).

This bifurcation matters because protocols differ on what "an identity" actually is. Nostr's `nsec` is bytes — the vault can store and load it. Matrix's identity surface is a `user_id` plus per-device Curve25519/Ed25519 keys generated fresh per login, plus cross-signing keys (master / self-signing / user-signing), plus pickled Olm/Megolm sessions that grow into megabytes per conversation, plus a recovery key for secret storage. `matrix-rust-sdk` already has a `StateStore` trait (sqlite-backed by default) for exactly this; the vault's job is to store the *bootstrap* and let the SDK own the rest behind an at-rest encryption boundary the vault provides.

> *Illustrative — signature-only, not implementation-ready (per [`feedback_spec_code_samples_illustrative_vs_implementation_ready`](<user-home>\.claude\projects\c--Users-mark--Code\memory\feedback_spec_code_samples_illustrative_vs_implementation_ready.md))*

```rust
/// Per-protocol identity slot. Two categories: vault-modelable (Direct)
/// vs SDK-delegated (Bootstrap + state-dir).
pub enum IdentitySlot {
    // ── Direct (vault-modelable; the slot IS the identity) ──

    /// Mere-native: derived from master via mere-identity::derive_keypair.
    /// Lineage: locally-derived (recoverable from master).
    Mere { protocol_salt: Vec<u8> },
    /// Nostr nsec/npub. Lineage: locally-rooted (the nsec is the identity).
    Nostr { secret_key: Secp256k1Secret, relays: Vec<Url> },
    /// Cable per-cabal key (cached; canonically derivable from Mere slot).
    /// Lineage: locally-derived.
    CableCabalKey { cabal_id: CabalId, derived: Ed25519Keypair },
    /// IRC NickServ password / SASL plain credentials. Lineage: locally-rooted.
    Irc { network_id: NetworkId, nick: String, sasl: SaslCredentials },
    /// X.509 client cert (Gemini, mTLS). Lineage: externally-rooted, locally-held.
    Tls { cert: Vec<u8>, key: SecretBytes },
    /// SSH key. Lineage: locally-rooted.
    Ssh { keypair: SshKeypair },
    /// Misfin Ed25519 keypair + handle. Lineage: locally-rooted.
    Misfin { keypair: Ed25519Keypair, handle: String },

    // ── Bootstrap (SDK-delegated; the slot is the *seed*; the SDK
    //    keeps its own encrypted state directory the vault wraps) ──

    /// Matrix bootstrap. matrix-rust-sdk owns its StateStore in
    /// `state_dir`; vault encrypts the directory at rest. Recovery key
    /// is the cross-device-verification root, NOT a way to reconstruct
    /// device keys after device loss.
    MatrixBootstrap {
        homeserver: Url,
        mxid: String,
        login_method: MatrixLoginMethod,    // Password, OAuth, RestoreSession, SSO
        recovery_key: Option<SecretString>,  // for cross-signing reset; not device-key recovery
        state_dir: PathBuf,                  // SDK-owned; vault wraps with encryption
    },
    /// ATproto bootstrap. atrium-rs owns session state; vault holds the
    /// rotation-capable seed. App passwords, refresh JWT, and rotation
    /// key live in the state_dir.
    AtprotoBootstrap {
        did: Did,
        pds: Url,
        rotation_key: Option<SecretBytes>,    // identity rotation key (separate from signing key)
        login_method: AtprotoLoginMethod,    // AppPassword, OAuth
        state_dir: PathBuf,
    },
    /// ActivityPub-with-server-state bootstrap (for Mastodon-shaped clients
    /// that maintain server-side session state).
    ActivityPubBootstrap {
        actor: Url,
        client_credentials: SecretString,
        state_dir: PathBuf,
    },

    // ── Mod-defined ──

    /// Mod-defined slot (typed by the mod). The mod declares its own
    /// category (Direct vs Bootstrap) via the slot manifest.
    Custom { mod_id: ModId, category: SlotCategory, payload: CustomPayload },
}

/// The lineage axis (orthogonal to Direct/Bootstrap). Affects how recovery
/// works — see §3.4.
pub enum CredentialLineage {
    /// Derived locally from master via deterministic salt.
    /// Recoverable from master alone.
    LocallyDerived,
    /// Generated locally (fresh randomness) and registered with an external
    /// authority. Lost = re-register; recovery phrase does NOT bring it back.
    LocallyGeneratedExternallyRegistered,
    /// Issued by external authority (access tokens, refresh JWTs, etc.).
    /// Rotates / expires; not meaningfully backupable.
    ExternallyIssued,
    /// Externally rooted (CA, identity provider) but locally held.
    /// Revocation is upstream's job.
    ExternallyRootedLocallyHeld,
}

/// User-level identity profile — one user can have multiple Profiles
/// (e.g. work, personal, alt) sharing or partitioning slots.
pub struct Profile {
    profile_id: ProfileId,
    display_name: String,
    master: Ed25519Keypair,      // mere-native master; never leaves storage backend
    slots: HashMap<ProtocolKey, IdentitySlot>,
}

/// Storage trait — pluggable backend, single point of credential touch.
/// Bootstrap-category slots get an encrypted `state_dir` managed here too;
/// the SDK reads/writes through a vault-provided wrapped FS.
pub trait IdentityStorage: Send + Sync {
    fn load_profile(&self, id: ProfileId) -> Result<Profile, StorageError>;
    fn save_slot(&self, id: ProfileId, key: ProtocolKey, slot: IdentitySlot) -> Result<(), StorageError>;
    fn delete_slot(&self, id: ProfileId, key: ProtocolKey) -> Result<(), StorageError>;
    fn list_profiles(&self) -> Result<Vec<ProfileSummary>, StorageError>;
    /// Open an encrypted state directory the SDK can use as its own
    /// storage root (Bootstrap-category slots only).
    fn open_state_dir(&self, id: ProfileId, key: ProtocolKey) -> Result<EncryptedDir, StorageError>;
}

/// Vault façade — the consumer-facing API.
pub struct IdentityVault<S: IdentityStorage> { /* opaque */ }

impl<S: IdentityStorage> IdentityVault<S> {
    pub fn open(storage: S, unlock: UnlockMethod) -> Result<Self, IdentityError>;
    pub fn current_profile(&self) -> &Profile;
    pub fn slot(&self, key: ProtocolKey) -> Option<&IdentitySlot>;
    pub fn add_slot(&mut self, key: ProtocolKey, slot: IdentitySlot) -> Result<(), IdentityError>;
    pub fn derive_mere_keypair(&self, salt: &[u8]) -> Result<Ed25519Keypair, IdentityError>;
    /// Hand the SDK its encrypted state directory.
    pub fn state_dir(&self, key: ProtocolKey) -> Result<EncryptedDir, IdentityError>;
}
```

### 3.2 Storage backends

Each backend implements `IdentityStorage`. Multiple may coexist; a vault is opened against exactly one.

| Backend | Use case | Host envelope |
|---------|----------|---------------|
| `InMemoryStorage` | Tests; anonymous sessions | All |
| `OsKeychainStorage` | Default desktop install (Windows Credential Manager / macOS Keychain / kwallet/SecretService) | Desktop |
| `PassphraseEncryptedStorage` | Portable single-file vault (encrypted at rest); for users who want backup-ability or who run Mere on systems w/o keychain | Desktop, mobile |
| `HardwareTokenStorage` | YubiKey / similar — root key never leaves device | Desktop |
| `HostBridgeStorage` | Browser-extension host: defers to extension's storage API | Extension/PWA |

Backend selection is a Mere-level deployment choice; the vault doesn't know which it's talking to. (Per [`feedback_configurability_over_opinionated_defaults`](<user-home>\.claude\projects\c--Users-mark--Code\memory\feedback_configurability_over_opinionated_defaults.md), expose this choice in user settings; the default is `OsKeychainStorage`.)

### 3.3 Multi-profile (resolved)

A single user may keep multiple profiles (work / personal / pseudonymous). Profiles are independent vault entries; per-profile master keys are independent (no derivation between profiles).

**Element-style sub-instances.** Each mod holds N profile-scoped sub-instances simultaneously; the UI shows one at a time but they are all live (resource-using). Switching profiles is a UI operation, not a process restart. The user can explicitly close a profile-scoped sub-instance to reclaim resources (memory, open sockets, sync state) — same shape as Element's "Sign out of this account" without removing the credentials.

This trades resource use (each loaded profile has its own SDK state-dir, gossip subscriptions, open Cable cabal connections) for fast switching. Per [`feedback_configurability_over_opinionated_defaults`](<user-home>\.claude\projects\c--Users-mark--Code\memory\feedback_configurability_over_opinionated_defaults.md): expose "auto-load profile on switch" vs "lazy-load on first use" as a setting; default to lazy-load (user opens a profile by name, mods spin up its sub-instances on demand).

Implication: mods (murm Cable engine; moothold Matrix mod; etc.) hold an `Arc<ProfileId>`-keyed map of sub-instances internally, and the per-mod `mount_personal` / `mount_in_moot` calls take a `ProfileId`. The vault's "current profile" pointer drives UI focus, not mod lifecycle.

### 3.4 Recovery semantics (lineage-dependent)

A single "recovery phrase recovers everything" mental model is wrong and will mislead users. Recovery means different things per lineage:

| Lineage | What "recovery" means |
|---------|----------------------|
| `LocallyDerived` | Recovery phrase + master reconstructs the slot deterministically. Cable cabal keys, Mere-native protocol keys. |
| `LocallyGeneratedExternallyRegistered` | Lost = **re-register from another trusted device** (Matrix cross-signing flow). Recovery phrase does NOT regenerate device keys; it unlocks the vault itself. |
| `ExternallyIssued` | Tokens rotate / expire by design. Recovery = **re-authenticate** to the issuer. |
| `ExternallyRootedLocallyHeld` | CA-issued certs: revoke + reissue by upstream authority. |

The vault UI must surface this distinction explicitly. Per [`feedback_configurability_over_opinionated_defaults`](<user-home>\.claude\projects\c--Users-mark--Code\memory\feedback_configurability_over_opinionated_defaults.md): expose per-slot "what does losing this device mean for this slot?" status to the user, not buried in docs. Phase 2C identity-vault sub-plan owns the UX shape.

### 3.5 Compatibility

The existing `IdentityProvider` trait stays. `IdentityVault<S>` will implement `IdentityProvider` against the current profile, so murm/Cable's existing dependency on the trait works unchanged.

### 3.6 Unlock tiers and unlock methods (resolved)

A flat "passphrase at launch" UX wastes friction on cheap reads and underpays on credential touches that should require a deliberate confirm. Resolution: **each slot declares an unlock tier at registration**, and the unlock-UX falls out as a consequence rather than being a top-level choice.

Three tiers:

| Tier | Behavior | Typical slots |
|------|----------|---------------|
| **Session** | Unlocked once at vault open; stays unlocked for the app session. | `Mere`, `CableCabalKey`, `Nostr`, `MatrixBootstrap` (the bootstrap secret; not the SDK state-dir, which has its own access pattern), `AtprotoBootstrap`, `Misfin`, `Tls`, `Ssh` |
| **Short-TTL** | Unlocked on first use; auto-relocks after a configurable idle window (default 15 min). | `Irc` SASL, slots flagged "sensitive" by the user |
| **Per-use** | Re-prompt on every credential read. | High-value slots the user explicitly tags (e.g., a financial-protocol slot, hardware-token-backed identities) |

Unlock methods (the *how* of any tier):

| Method | When | Notes |
|--------|------|-------|
| **Passkey** (WebAuthn / FIDO2) | Default on hosts that support it | Phishing-resistant; pairs with hardware token storage backend |
| **System keyring** | When `OsKeychainStorage` is the backend | Vault unlock = OS keychain auth (Hello / Touch ID / kwallet) |
| **Passphrase** | `PassphraseEncryptedStorage`, or fallback when no passkey/keyring | Argon2id-derived KEK; standard |
| **Hardware token** | `HardwareTokenStorage` | Per-tier UX: session unlock vs per-use challenge |

Combinations are valid (passphrase + passkey 2FA on `PassphraseEncryptedStorage`). Per [`feedback_configurability_over_opinionated_defaults`](<user-home>\.claude\projects\c--Users-mark--Code\memory\feedback_configurability_over_opinionated_defaults.md): tier and method are user-overridable per slot; defaults aim for "good enough for a credential of that lineage."

### 3.7 Isolation and capability scoping (stated threat model)

**This plan's vault assumes all mods are trusted within the Mere process.** A compromised Matrix mod can read the Nostr `nsec` slot in this single-process model. The vault's at-rest encryption + per-session unlock + audit log do NOT prevent in-process credential theft; they raise the cost of *passive* attack surfaces (disk theft, swap leakage), not active ones inside the process.

This is a deliberate Phase 2C scope choice, named here so it is not mistaken for "the vault makes mods safe." Future hardening paths, in increasing cost order:

1. **Per-mod capability tokens.** Vault checks a token before serving a slot to a mod; tokens granted at install time, scoped to declared protocol IDs. Defends against accidentally-broad slot reads but not malicious mods.
2. **Per-mod sandboxing via WASM.** Mods running under a WASM runtime (per inherited [`2026-03-08_unified_mods_architecture_plan.md`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/subsystem_mods/2026-03-08_unified_mods_architecture_plan.md)) get only the host imports they declare. Defends against in-process exfiltration.
3. **Per-mod processes (Servo content-process model).** Strongest. Vault speaks IPC to mods; slot bytes never enter mod address space without explicit grant. Multi-month work; not Phase 2C.

Phase 2C ships level 0 (single-process, all mods trusted). Phase 3 ships level 2 once WASM mods land. Level 3 is a long-term goal. The vault API is shaped so the threat-model upgrade is additive (capability tokens slot into `slot()` / `state_dir()` calls without changing slot enum shape).

A future doc under `mere_docs/technical_architecture/` will own the full threat model; this plan defines the surface and names the level-0 assumption explicitly.

### 3.7 Phase boundary

Identity vault lands **before** any new protocol mod that needs non-Mere credentials. That makes it the gate for Nostr / Matrix / ATproto / IRC mods. Cable continues to work pre-vault using the existing `InMemoryProvider`-style trait impl.

---

## 4. Identity Discovery, Proof, and the Self-Host-with-Fallback Pattern

### 4.1 Three problems, distinguished

Identity discovery is three problems that look similar and are not:

| Problem | What it answers | Substrate |
|---------|-----------------|-----------|
| **Storage** | Where do my own credentials live? | Identity vault (§3) |
| **Discovery** | Given a handle (`alice@mere.example`), find Alice's protocol endpoints. | WebFinger / NIP-05 / DID resolution |
| **Proof** | Does Alice actually control the Nostr `npub` that her WebFinger doc names? | **Back-claims** — the target re-asserts the link and signs it with the protocol's own key |

Discovery without proof is an unauthenticated UX hint. Anyone who controls `evil.example` can publish a WebFinger document claiming to be `@anthropic_ceo` on Matrix; a receiving Mere has no way to disprove that on the strength of the WebFinger doc alone. **This plan ships discovery and proof as two separate concerns**, not as one bundled "WebFinger" surface.

### 4.2 Why WebFinger (and what it does not do)

WebFinger (RFC 7033) maps a human-shaped handle to a JSON document linking to protocol-specific endpoints. Mere uses it for discovery only. **Proof requires reciprocal claims**:

- **Nostr** uses [NIP-39 (External Identity Claims)](https://github.com/nostr-protocol/nips/blob/master/39.md): the Nostr identity publishes a kind:0 metadata event listing external identities, signed by the `nsec`. A verifier checks both directions: WebFinger says "Alice's Nostr is `npub1…`" AND `npub1…`'s metadata event says "I am Alice on `mere.example`."
- **Matrix** can prove a bridge by a signed message in a verified room, or by an `m.room.canonical_alias`-linked control room, or via Matrix's identity-server `3PID` binding.
- **ATproto** binds handle ↔ DID at the protocol level: the DID document references the handle's host and the host serves a `.well-known/atproto-did` record naming the DID. ATproto's own resolution flow gives bidirectional proof; WebFinger is auxiliary.
- **PGP / Keyoxide** use the same back-claim pattern more generally — every claimed binding has a counterclaim.

This plan adopts **bidirectional verification as the default**: a Mere UI surfaces a slot as "verified" only when both sides claim each other. One-sided WebFinger entries are surfaced as **unverified hints**, not authoritative bindings, and tagged accordingly in any rendered identity card.

### 4.3 Nostr-specific note: NIP-05 is its own surface

Nostr's discovery does **not** ride WebFinger. NIP-05 specifies its own document at `/.well-known/nostr.json` mapping local-parts to hex pubkeys plus an optional `relays` map. Mere's verso must serve **both** `/.well-known/webfinger` (for ActivityPub / Matrix / Misfin / Cable / generic Fediverse discovery) and `/.well-known/nostr.json` (for Nostr discovery). They cohabit on the same host but are independent endpoints; populate from the same vault slots. There is no `http://nostr.com/protocol/*` rel in standard use; the earlier example's Nostr rel was wrong and is replaced in §4.6.

### 4.4 Two modes

Per Premise 3, every self-host-capable surface ships in two interoperable modes:

#### Mode 1 — self-hosted via verso

`verso-tile` already manages rendering surfaces; it can also run lightweight server endpoints (HTTP for WebFinger, Gemini for Misfin, etc.) when the host envelope permits raw sockets (Desktop; some Native mods). When the user's Mere is online, requests for `https://<their-host>/.well-known/webfinger?resource=acct:<them>@<host>` are served directly from their machine.

This mode requires:

- A reachable hostname or IP (DDNS, port forwarding, hole-punching via iroh, or LAN-only).
- Verso running an HTTP listener (already in scope per the smolweb engine + nematic work; this plan formalizes the shape).
- The user's credentials in the vault (TLS cert; Mere's master key signs the WebFinger doc).

#### Mode 2 — third-party hosting fallback

For 24/7 availability when the user's Mere isn't reachable. The user publishes a static WebFinger document to a third-party host (an existing Fediverse instance, a CDN'd bucket, a friend's Mere instance, …). Mere generates and uploads the doc; the third party serves it.

This mode is **degraded**: stale data updates require re-upload; revocations require coordination. But it provides reachability when self-host is offline.

#### Bridge mode (Mode 1.5)

Mere instances of *kith* / *kin* (per the Mere lexicon) can serve as cooperative WebFinger relays for each other over `mere/webfinger-bridge/v1`. When a user's primary host is unreachable, a friend's Mere can answer with the most recent signed doc it has cached. This is an iroh-gossip / iroh-blobs–shaped problem (gossip the doc-id, fetch by content-address) and it composes naturally with the layering in §2.

#### Future: decentralized storage time bank

User explicitly named this as the eventual filler for Mode 2: a peer-supplied storage marketplace where users trade availability time. Out of scope for this plan; named here so future work can slot in cleanly. Per [`feedback_targets_over_time_estimates`](<user-home>\.claude\projects\c--Users-mark--Code\memory\feedback_targets_over_time_estimates.md) — the done-condition is "Mode 2 has a Mere-native option," not a date.

### 4.5 The pattern is general

This plan generalizes Modes 1 / 1.5 / 2 to **any self-host-capable surface** in Mere:

- WebFinger publication
- Misfin server (already candidate per inherited `protocol_modularity_and_host_capability_model.md` §5.3)
- Gemini capsule hosting
- Matrix homeserver hosting (long-tail; advanced)
- Nostr relay (for users who want to run their own)
- Cable cabal seeding (a reachable Mere can act as a 24/7 cabal seeder for a community)

For each: define Mode 1 (verso-served), Mode 1.5 (kith/kin bridged), Mode 2 (third-party fallback). Each protocol's own plan elaborates its Mode 1/2 mechanics.

### 4.6 Endpoint shapes (WebFinger + NIP-05, populated from slots)

Verso serves both endpoints, populated from the same vault. Slots opt in per-profile (default: private).

**`/.well-known/webfinger`** — used for ActivityPub, Matrix, Misfin, Cable, generic Fediverse:

> *Illustrative example. Real-world `rel` values are protocol-specific; only well-known ones are listed. Bindings are unverified hints unless a back-claim has been collected — see §4.2.*

```json
{
  "subject": "acct:mark@mere.example",
  "aliases": ["https://mere.example/users/mark"],
  "links": [
    {"rel": "self", "type": "application/activity+json", "href": "https://mere.example/users/mark"},
    {"rel": "http://webfinger.net/rel/profile-page", "href": "https://mere.example/users/mark"},
    {"rel": "https://atproto.com/ns/handle", "href": "did:plc:..."},
    {"rel": "https://mere.dev/cable", "href": "cable://..."},
    {"rel": "https://misfin.org/ns/handle", "href": "misfin://mark@mere.example"}
  ]
}
```

Notes:

- Matrix and ATproto have their own canonical resolution flows (`/.well-known/matrix/client`, `/.well-known/atproto-did`); a WebFinger entry is auxiliary, not authoritative.
- Nostr is **not** in the WebFinger doc — it has its own `.well-known/nostr.json` (below).

**`/.well-known/nostr.json`** — Nostr discovery (NIP-05):

```json
{
  "names": {
    "mark": "0123abcd...hex-pubkey..."
  },
  "relays": {
    "0123abcd...hex-pubkey...": ["wss://relay.example", "wss://relay.mere.example"]
  }
}
```

Both endpoints draw from the same vault slots: WebFinger from `MatrixBootstrap`, `AtprotoBootstrap`, `Misfin`, `CableCabalKey`, etc.; NIP-05 from `Nostr`. Per [`feedback_configurability_over_opinionated_defaults`](<user-home>\.claude\projects\c--Users-mark--Code\memory\feedback_configurability_over_opinionated_defaults.md): expose per-slot publication toggle; default to "private until opted in."

**Back-claim collection.** When the user adds a slot, Mere offers to publish the matching back-claim (e.g., NIP-39 kind:0 metadata for Nostr; signed-room post for Matrix). The vault tracks back-claim status per slot so the UI can render *verified* vs *unverified* identity cards (§4.2).

### 4.7 Doc signing — what TLS, JWS, and SXG each buy you (resolved)

**TLS** authenticates the connection: the verifier knows the doc came from the host *named in the URL*, at fetch time. It does not authenticate the doc itself; if the host is compromised or replaces the doc, TLS happily serves the new bytes.

**JWS** ([RFC 7515](https://datatracker.ietf.org/doc/html/rfc7515)) signs the doc itself with a key the verifier can recognize. A JWS-wrapped WebFinger doc says "Mere-master-key X signed this content"; the verifier checks the signature against the user's published master key (recoverable via the doc's `links` self-reference, or out-of-band). This survives host substitution: a third-party Mode 2 host serving a JWS doc can't forge it without the master key.

**SXG** (Signed HTTP Exchanges) is a more elaborate scheme that signs an entire HTTP exchange (request + response headers + body) so a CDN-cached or otherwise-relayed exchange verifies as having come from origin. Browser-shaped use case (AMP-style preloading); overkill for a small JSON doc and not standardized in the Fediverse.

Resolution:

- **Mode 1 (verso self-host)**: TLS only. Direct connection from verifier to user's verso; the doc and the host are the same trust root.
- **Mode 2 (third-party host)**: TLS + **JWS-wrapped doc**. Third-party host serves bytes; JWS signature lets the verifier confirm the doc came from the user's master key, not the third party. This is what makes Mode 2 not a security regression.
- **Mode 1.5 (kith/kin bridge)**: TLS + JWS. Same reasoning as Mode 2 — the bridge is a relay, not an authority.
- **SXG**: out of scope. Revisit only if a concrete cache-shaped use case appears.

The vault holds the master key; the `IdentityVault` API grows a `sign_discovery_doc(bytes) -> JwsBytes` operation that uses the master key as the Mere identity root. Slot-level keys do not sign discovery docs — only the master.

### 4.8 Mode 2 churn: multi-host publication + previous-handles chain (resolved)

Q11 framing: depending on a single third-party host makes the user's handle fragile. Three mitigations, two of which ship:

1. **Default mitigation: push self-hosting.** Mode 1 (verso self-host) is the recommended path. Mode 2 is a *convenience* for users who can't self-host, with explicit "your handle is fragile if `<host>` goes away" warnings in the publication UI. (This is a UX guardrail, not architecture.)
2. **Multi-host publication.** When the user opts into Mode 2, they may publish to N third-party hosts; resolvers fall back through the list. Mere maintains the publication state per slot per host. A single-host failure still leaves the discovery doc reachable.
3. **Previous-handles chain (the resolution Mark prefers).** When a user migrates from `alice@old.example` to `alice@new.example`, the new WebFinger doc carries an explicit `previous` link (or a Mere-defined extension rel) referencing the prior handle. Resolvers walking the chain can prove continuity: the *old* doc was signed by master-key X; the *new* doc references the old and is also signed by X. Migration is verifiable without a live old host. This pattern composes with multi-host publication: a previous-handles chain can be served from whichever publication endpoint is currently live.

Phase 2D ships (1) and (3); (2) is a polish track. The doc schema reserves a `mere:previous-handle` rel for the chain; the slot's publication state grows a `previous_handles: Vec<HandleClaim>` field.

### 4.9 Per-identity publication policy (resolved)

Q12 framing: a "publish to NIP-05" toggle separate from "publish to WebFinger" leaks implementation detail (which `.well-known/` paths exist). Resolution: **per-identity publication policy.** When the user adds a Nostr identity to a profile and toggles "publish for discovery," that single act writes:

- `/.well-known/nostr.json` (NIP-05 entry)
- WebFinger doc's Nostr-link entry (if the user's profile also publishes WebFinger; if not, this is a no-op)

The toggle is keyed off the slot, not the endpoint. The endpoint set is derived from the slot's protocol (`Nostr` slot → NIP-05 + WebFinger Nostr-link; `MatrixBootstrap` slot → WebFinger Matrix-link only; etc.).

Advanced users who want to publish to one endpoint but not another edit the published-policy config directly (in-app config editor; not a UI checkbox). Per [`feedback_configurability_over_opinionated_defaults`](<user-home>\.claude\projects\c--Users-mark--Code\memory\feedback_configurability_over_opinionated_defaults.md): the simple toggle is the default; the granular config exists for users who want it; the UI doesn't drag everyone through the granular case.

### 4.10 Phase boundary

Discovery endpoints (WebFinger + NIP-05 publication) land **after** identity vault (they consume slots). Verso's HTTP listener is the prerequisite. **Back-claim collection / verification** is its own track and lands per-protocol (Nostr first, since NIP-39 is mature; Matrix and ATproto follow with their own flows). **JWS signing for Mode 2 / 1.5** lands with the third-party publication track. **Previous-handles chain** is a slot-schema and resolver feature; lands in Phase 2D alongside Mode 2.

---

## 5. Protocol Mods and Primitive Moot Nodes (Moothold Phase 2C → 3)

### 5.0 Re-derivation of the Murm / Moothold split

The MURM_AS_BILATERAL framing of the split as "1:1 vs many-to-many" is the wrong axis. Most modern protocols don't model a 1:1 vs N-party distinction at the protocol layer — Matrix DMs are 2-person rooms; Nostr DMs (NIP-17/44) are encrypted events relayed the same way as public events; IRC channels carry public conversation but `/msg` is the same network. Sorting by participant count forces single protocols to straddle the boundary.

**The right axis is durable group with membership vs ad-hoc peer interaction.**

| Murm — ad-hoc peer interaction | Moothold — durable group with membership |
|--------------------------------|-------------------------------------------|
| Direct co-browse session between two known users | Cabals (Cable named-cabals: durable, member-rostered, topic-bearing) |
| MLS small-group chat where membership is co-op-shaped (host + invited guests, single session) | Matrix rooms (2-person, 200-person, 200000-person — rooms have membership state) |
| Cable bilateral DMs where no cabal exists | Nostr communities / NIP-29 / NIP-72 + public-relay topical streams |
| Real-time presence-and-stream between two specific Mere instances | IRC channels (any size; `/msg` is also Moothold-shaped because user-pair identity is durable across sessions) |
| | ActivityPub timelines, ATproto feeds, Misfin mailboxes |

This is a real change from MURM_AS_BILATERAL §1's claim that "co-op stays in murm despite multi-party state." Co-op sessions remain bilateral *as long as* the host-guest relationship is the only binding state; once a co-op session promotes to a long-term shared cabal, it crosses into Moothold territory. This re-derivation makes the boundary crossing a first-class architectural event rather than a deferred open question.

**Cable stays in Murm (resolved).** The re-derived axis above suggests Cable cabals look Moothold-shaped (durable, member-rostered). User resolution: *"murm can keep Cable; the soul of it is bilateral."* The trust substrate (host-led, all participants known and admitted, Ed25519 per-cabal keypair derivation) is what the layer's name tracks, not the membership-set cardinality. Cable cabals stay in murm. Moothold's mods are the protocols whose *trust model* is "broadcast to a public/semi-public membership."

What changes immediately on the strength of this re-derivation:

- **Misfin moves out of Murm.** Misfin is mail-shaped (store-and-forward, asynchronous, recipient may be offline for days). It does not fit the "ad-hoc peer interaction" model. Misfin lives in **moothold + nematic**: nematic hosts the smolweb-shaped Misfin server endpoint; moothold owns the mailbox-as-durable-membership UX.

  > **Relaxed 2026-06-01 (Mark).** This "move Misfin out" call is reversed for the client/exchange side. Misfin and other smolweb *exchange* protocols stay in murm once developed to Misfin's level and jibing with murm. The operative murm axis is *bilateral exchange with a known endpoint* (sync or store-and-forward), not strictly "ad-hoc real-time peer," so `misfin` stays under `crates/murm/`. nematic still hosts any server-side smolweb endpoint; murm owns the client/exchange side. See the [MURM_AS_BILATERAL status note](../../murm_docs/technical_architecture/MURM_AS_BILATERAL.md) and the [murm/p2p landscape brief](../research/2026-05-31_murm_p2p_landscape_brief.md) contradiction (C).
- **Matrix is fully Moothold.** Including DMs. The mod does not split DMs into one layer and rooms into another.
- **Nostr is fully Moothold.** Including NIP-17 DMs. Same reasoning.
- **iroh-gossip is not a "Murm/Moothold concern."** It's transport that lives in `mere-transport`. §2.1 already corrected to say so.

### 5.1 The two pinning patterns

A protocol-client surface can appear in the user's experience in two places:

**Pattern A — Pinned in your orrery.** The user installs a mod (`mere-mod-matrix`, `mere-mod-nostr`, `mere-mod-irc`) and pins a node into their personal orrery (the root graph view). The node renders as a chat client; messages flow through the user's vault credentials. **Mod-install required.**

**Pattern B — Pinned inside a moot.** A moot operator places a primitive protocol-client node in their moot's graph. Moot members visit the moot, and the node renders for them — *regardless of whether they have the mod installed*. The moot's own infrastructure (the moot host's Mere, or a delegated relay, or hosted-third-party fallback) carries the protocol session on behalf of the visitor. **No mod-install required for visitors; the moot owns the relationship.**

User's framing: *"like Element matrix rooms on the web, but on IPFS."*

Pattern B requires:

- A protocol-client implementation (possibly the same mod that powers Pattern A) running at the moot host's side.
- A streaming transport from moot host → visitor that delivers the rendered impression. Plain QUIC (`mere/moothold/primitive-node/v1`) for live; iroh-blobs for caching.
- A bridge mode for visitor identity (resolved below).

#### 5.1.1 Puppet mode vs portal mode (Matrix as the canonical case)

A subtle constraint: federation-aware protocols see whatever joins their rooms, and that propagation is not undoable. In Matrix specifically: if a visitor's real `MXID` joins a room, every other homeserver participating in that room learns the MXID via federation. There is no "leave and unring it" — homeserver caches retain the membership event indefinitely. So "visitor joins as themselves" vs "moot relays on their behalf" isn't a per-visit UI toggle; they are **two architecturally different bridge modes** with different costs.

**Puppet mode** (matterbridge-style):

- The moot's bot owns the room membership.
- Visitors have *no* MXID in the room.
- All visitor messages enter Matrix attributed to the bot, with a display-name prefix (`<alice via mere-moot> hello`).
- Loses per-visitor reactions / threads / E2EE; preserves moot-side privacy and scales easily.

**Portal mode**:

- The bridge runs a homeserver (or virtualizes one); each visitor gets a "ghost" MXID on it.
- Visitors join the room as themselves (under the ghost MXID).
- Reactions, threads, E2EE all preserved.
- Operationally heavy: the bridge **is** a homeserver, and every visitor accumulates a permanent MXID on it.

**Phase 3 picks puppet mode.** Smaller surface; easier to run; the loss of per-visitor reactions/E2EE is an acceptable Phase 3 limitation, named explicitly in the moot UI ("you are visiting via puppet mode; Matrix does not see your real account"). Portal mode becomes a Phase 4+ research probe. The same shape applies to other federation-aware protocols (ActivityPub: puppet-mode federated post would attribute to the bridge actor; portal-mode would create per-visitor actors on the bridge instance).

For non-federation protocols the distinction collapses: IRC has no federation, so a "puppet" is just a bot, and "portal" doesn't apply. Nostr is relay-broadcast; visitor messages can be signed with a moot-bot's key (puppet) or with the visitor's `nsec` (portal-equivalent — visitor's own identity goes out). Per-protocol bridge-mode docs branch off this plan.

### 5.2 Primitive moot node trait

> *Illustrative — signature-only.*

```rust
/// A protocol mod that can run as a primitive moot node.
pub trait PrimitiveMootProtocol: Send + Sync {
    fn protocol_id(&self) -> &'static str;     // "matrix", "irc", "nostr", "atproto", ...
    fn capabilities(&self) -> ProtocolCaps;    // requires_mod_install_for_pin, supports_anon_visitor, ...

    /// Mount as a node in the user's orrery (Pattern A).
    fn mount_personal(
        &self,
        vault: &IdentityVault,
        config: &NodeConfig,
    ) -> Result<Box<dyn PrimitiveNode>, MootError>;

    /// Mount inside a moot — the moot host's instance becomes the relay
    /// (Pattern B). Visitors connect via the moot, not directly.
    fn mount_in_moot(
        &self,
        moot: &MootHandle,
        host_credentials: HostCredentials,    // moot owner's slot or a moot-bot slot
        config: &NodeConfig,
    ) -> Result<Box<dyn PrimitiveNodeRelay>, MootError>;
}

/// What a visitor sees when entering a moot that hosts a primitive node.
/// Implemented by moothold; the protocol mod doesn't see this directly.
pub trait PrimitiveNodeRelay {
    fn open_visitor_session(
        &self,
        visitor: VisitorIdentity,    // anonymous, kith, kin, moot-member, ...
    ) -> Result<Box<dyn PrimitiveNodeSession>, MootError>;
}
```

The moot's own `PrimitiveNodeRelay` instance is what carries the visitor's bytes. The visitor's Mere doesn't need the protocol mod compiled in; it speaks `mere/moothold/primitive-node/v1` and renders the impression.

### 5.3 Where the impression comes from

Pattern B's "render for any visitor" needs a content-shape decision. Three candidates:

1. **Server-side rendered HTML, streamed.** Moot host runs the protocol mod; renders a chat panel; ships HTML+CSS to the visitor; visitor's verso renders it as a tile. Cheap, works without per-protocol viewer code on the visitor. Trades latency + interactivity.
2. **Wire-protocol bytes proxied, visitor-side rendered by moot-supplied WASM viewer.** Moot host ships a WASM viewer along with the protocol-bytes stream; visitor's WASM runtime renders. Higher interactivity; requires WASM scripting host (per Premise 4).
3. **Generic Mere chat-impression schema.** Moot host translates protocol-specific messages into a Mere-native "chat impression" schema; visitor renders generically. Loses protocol-specific features but uniform.

Phase 3 picks one; current bias is **toward (2)** because it composes with the scripting host that Premise 4 calls for and leans on existing WASM infrastructure. (1) is a viable degraded mode for hosts that can't run WASM. (3) is a long-term unification target, not a Phase 3 deliverable.

### 5.4 Scripting host (deferred — capability-surface probe first)

Mark's resolution on Q6: *"if scripting becomes a hot path or really important, then we should think deeply about the options. As is, I find it hard to picture the capability surface of this, so let's see how Boa / Extism / Wasmtime do and what we'll need."*

Three candidate runtimes occupy different layers; the comparison is not apples-to-apples:

| Candidate | Layer | What it runs | Phase 3 fit |
|-----------|-------|--------------|--------------|
| **Boa** | JS engine in pure Rust | Plain JavaScript | Existing inherited plan; suits scripted mods authored as JS files. Already covers a generic surface. |
| **Extism** | WASM plugin framework with opinionated host-call ABI | WASM modules from any source language (Rust, Go, AssemblyScript, …) | Right shape for a *plugin* model — typed host calls, defined boundaries. Higher engineering up front for the host-call vocabulary. |
| **Wasmtime** | Raw WASM runtime | WASM modules; you build the host-call ABI yourself | Maximum control, maximum host-side work. Right answer if neither Boa's JS nor Extism's opinionated plugin shape fits. |

**This plan does not pick one.** Phase 3 starts with compiled-in Rust mods (IRC first cut); the scripting host is unblocked only when a concrete need surfaces (a community wants to ship a moot-scoped scripted node, or a third-party protocol mod wants WASM packaging). At that point, a dedicated **scripting host probe** evaluates Boa / Extism / Wasmtime against the actual capability surface — what host calls do mods need? Network access (constrained), vault access (capability-token scoped per §3.7), filesystem (none / encrypted-state-dir-via-host-bridge), rendering (impression-channel only)?

Until that probe runs, treat scripting as **future scope**, not Phase 3 deliverable. The inherited [`2026-03-11_boa_scripting_engine_plan.md`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/2026-03-11_boa_scripting_engine_plan.md) remains relevant as background; its scope (graphshell-side script extension) is broader than this plan's mod-scripting concern.

Wasm-runtime-as-mod-format remains the long-term packaging answer per inherited [`2026-03-08_unified_mods_architecture_plan.md`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/subsystem_mods/2026-03-08_unified_mods_architecture_plan.md). This plan does not re-litigate that.

### 5.5 Per-protocol mod table (with cost tiers)

Phase 3+ mods, packaging-class anchored to inherited [`protocol_modularity_and_host_capability_model.md`](../../../../graphshell/design_docs/graphshell_docs/technical_architecture/2026-03-30_protocol_modularity_and_host_capability_model.md) §5. **Cost tier** distinguishes wrap-maintained-SDK (T1, weeks) / wrap-thin-or-unmaintained (T2, weeks-to-months with maintenance burden) / implement-from-spec (T3, months and ongoing).

| Protocol | Primary home | Slot category | Packaging class | Pattern A | Pattern B | Cost tier | Notes |
|----------|--------------|---------------|-----------------|-----------|-----------|-----------|-------|
| Cable | murm (built-in; cabals likely moot-shaped — see §5.0) | Direct (`CableCabalKey`) | core (via murmuring) | yes | yes | **T3** (already in flight) | We own the impl; cable.rs upstream out of scope per memory |
| MLS | murm | Direct (`Mere` per-group) | optional portable | yes | possibly | **T3** | `openmls` exists & maintained but MLS spec is large; multi-month |
| Matrix | moothold mod | Bootstrap (`MatrixBootstrap` + state_dir) | non-engine layer | yes | yes | **T1** | Wrap [`matrix-rust-sdk`](https://crates.io/crates/matrix-sdk); SDK owns StateStore |
| Nostr | moothold mod | Direct (`Nostr`) | non-engine layer | yes | yes | **T1** | Wrap [`nostr-sdk`](https://crates.io/crates/nostr-sdk) (or `nostr` crate) |
| IRC | moothold mod | Direct (`Irc`) | native feature mod (raw sockets) | yes (host envelope permitting) | yes | **T1** | Wrap [`irc`](https://crates.io/crates/irc); long-lived TCP perfect for relayed Pattern B |
| Misfin | moothold + nematic | Direct (`Misfin`) | optional portable | yes (mailbox view) | yes | **T2** | Mail-shaped — store-and-forward; nematic serves, moothold owns mailbox UX |
| ATproto | moothold mod | Bootstrap (`AtprotoBootstrap` + state_dir) | non-engine layer | yes | yes | **T1–T2** | Wrap [`atrium-rs`](https://crates.io/crates/atrium-api); spec still moving so expect upstream churn |
| ActivityPub | moothold mod | Bootstrap (`ActivityPubBootstrap` + state_dir) | non-engine layer | yes | yes | **T2–T3** | No mature pure-Rust SDK; partial impls (e.g. `kitsune`) but largely hand-roll |
| Tox | (deferred) | — | non-engine layer | n/a | n/a | **T3 (revival)** | `rstox` essentially abandoned; revival or FFI to c-toxcore needed |
| Hypercore / Pears | (deferred) | — | non-engine layer | n/a | n/a | **T2–T3** | JS-first ecosystem; `hypercore-protocol-rs` lags upstream |
| Briar | (deferred) | — | non-engine layer | n/a | n/a | **T3** | Android-only reference impl; greenfield Rust port |

User explicitly named **IRC in moothold as the implementable primitive node** for Phase 3 first-cut. Reasoning aligns with cost-tier reality: IRC is shape-simple (line-oriented), well-specified, **T1 wrap of a maintained crate**, and an excellent stress test for the relay-from-moot-host pattern because IRC requires a long-lived TCP connection — exactly the workload that benefits from the moot hosting on the visitor's behalf.

The roadmap-as-flat-list anti-pattern: a "do MLS, Matrix, Nostr, IRC, ATproto, ActivityPub, Tox, Hypercore, Briar" bullet list hides ~100× cost variance between T1 and T3. **Phase 3 first batch is the T1 mods** (Matrix, Nostr, IRC, possibly ATproto). T2 mods follow once T1 lessons settle. T3 mods get their own dedicated plans (MLS already; Tox/Hypercore/Briar are research probes, not Phase 3 deliverables).

### 5.6 Phase boundary

Per-protocol mods are sequenced by mod-author + priority **within their cost tier**. Recommended first cut: **IRC** (T1, user named it). Then **Nostr** (T1, NIP-39 back-claim flow is a verifiable beachhead for §4 proof work). Then **Matrix** (T1 with the largest credential complexity, exercises Bootstrap-category vault path end-to-end).

---

## 6. Cross-Cutting Phase Sequencing

This plan sequences only the architectural prerequisites. Per-protocol plans branch off and proceed in parallel.

### Phase 2C — Foundations (architectural prerequisites)

| Track | Outcome | Gates |
|-------|---------|-------|
| **Iroh transport** in `mere-transport` | Real iroh-backed `Transport` impl alongside `MemoryTransport`; ALPN registry; multi-ALPN node | Cable migration plan §5 (post Phase 2 extraction) |
| **iroh-blobs** integration | `mere-transport` exposes blob get/put; murm wires Cable attachments | Iroh transport |
| **iroh-gossip** integration | `mere-transport` exposes presence subscribe/publish; murm wires Cable cabal presence | Iroh transport |
| **iroh-docs** integration | `mere-transport` exposes docs read/write; reserved for moothold consumption | Iroh transport |
| **Identity vault** | `IdentityVault<S>`, `Profile`, slots, `OsKeychainStorage`, `PassphraseEncryptedStorage` | None (additive over current `IdentityProvider`) |
| **PersistentCabalStore wired to CableEngine** | murm Cable engine uses persistent store by default; in-memory becomes test-only | None (already specified in Cable migration plan Phase 5) |

### Phase 2D — Verso-as-server + Discovery + Proof

| Track | Outcome | Gates |
|-------|---------|-------|
| **Verso HTTP listener** | verso-tile can run an HTTP server on configured ports; tile slots can be backed by served documents | None (slots into existing smolweb work) |
| **WebFinger Mode 1** | verso serves `/.well-known/webfinger`; populates from current profile slots | Identity vault |
| **NIP-05 Mode 1** | verso serves `/.well-known/nostr.json` from `Nostr` slot | Identity vault |
| **Mode 2 third-party publication** | publish-static-doc flow for both endpoints | Mode 1 |
| **Mode 1.5 kith/kin bridge** | discovery-doc relay over `mere/webfinger-bridge/v1`; iroh-gossip + iroh-blobs | iroh-gossip + iroh-blobs |
| **Back-claim collection (Nostr first)** | NIP-39 kind:0 metadata publication; vault tracks back-claim status per slot | Nostr mod (Phase 3 first cut) |
| **Back-claim verification** | UI surfaces verified vs unverified bindings | Back-claim collection |

### Phase 3 — Moothold protocol mods (T1 first; puppet bridge mode only)

| Track | Outcome | Gates |
|-------|---------|-------|
| **Mooting trait + primitive moot node runtime (single-host relay)** | `PrimitiveMootProtocol` trait, relay machinery, `mere/moothold/primitive-node/v1` ALPN handler. Single-moot-host relay only — distributed-relay scaling deferred. | Iroh transport, identity vault |
| **IRC mod (T1, first concrete)** | Pattern A pin + Pattern B puppet-mode relay; integrates with vault `Irc` slot | Mooting trait |
| **Nostr mod (T1)** | Pattern A pin; Pattern B with moot-bot signing (puppet) or visitor-`nsec` signing (per-moot config). Drives back-claim collection (NIP-39). | Mooting trait, vault `Nostr` slot |
| **Matrix mod (T1, first Bootstrap-category consumer)** | Wraps `matrix-rust-sdk`; SDK state-dir lives in vault-provided encrypted dir; Pattern B uses **puppet mode only** (matterbridge-style); portal mode deferred to Phase 4+ research. | Mooting trait, vault `MatrixBootstrap` |
| **ATproto mod (T1–T2)** | Wraps `atrium-rs`; same Bootstrap-category pattern as Matrix. | Mooting trait, vault `AtprotoBootstrap` |
| **iroh-docs first concrete consumer: moot membership roster** | Roster of who-belongs-to-this-moot as a CRDT; shared mutable state with concurrent additions/removals. | Iroh transport, iroh-docs integration |
| **Capability tokens (level-1 isolation upgrade)** | Triggered by Matrix mod landing; vault checks per-mod capability tokens before serving slots. | Matrix mod design |
| **Scripting host probe** | Evaluate Boa / Extism / Wasmtime against actual capability surface needed by *some* concrete scripted-mod use case. | First concrete scripted-mod use case surfacing |
| **MLS sibling Bilateral protocol (T3)** | murm-side, parallel to Cable. Multi-month spec implementation; own dedicated plan. | iroh transport |
| **Portal-mode bridge research** | Follow-up research probe for federation-aware protocols (Matrix portal homeserver, ActivityPub per-visitor actors). Not Phase 3 deliverable. | Puppet mode shipped |

### Phase 4 — Generalize self-host-with-fallback

| Track | Outcome | Gates |
|-------|---------|-------|
| **Misfin self-host via nematic + verso** | Pattern from §4.5 applied to Misfin server | Verso HTTP listener |
| **Cable cabal 24/7 seeder mode** | Reachable Mere acts as cabal seeder; iroh-blobs + iroh-gossip backed | Cable persistent store wired, iroh-blobs |
| **Decentralized storage time bank — design probe only** | Research doc, not implementation | None |

---

## 7. Question Resolution Log

The 12 open questions from the prior draft are now resolved or scoped. Listed here as decisions, not pending work.

### Resolved

1. **Vault unlock UX → §3.6 three tiers.** Slots declare a tier at registration: **Session** (unlock once at app open, default for most slots), **Short-TTL** (auto-relock after configurable idle, default 15 min, for sensitive-flagged), **Per-use** (re-prompt every read, for user-tagged high-value). Methods: passkey / system keyring / passphrase / hardware token, combinable. Unlock-UX is a consequence of the tier+method, not a top-level design choice.
2. **Multi-profile switch UX → §3.3 Element-style sub-instances.** Each mod holds N profile-scoped sub-instances simultaneously; UI shows one at a time. User can close a sub-instance to reclaim resources. Profile switch is UI, not process restart.
3. **Discovery doc signing → §4.7 TLS / JWS / SXG.** Mode 1 (verso self-host): TLS only — host and doc share a trust root. Mode 2 (third-party host) and Mode 1.5 (kith/kin bridge): TLS + **JWS-wrapped doc** so the verifier confirms master-key authorship regardless of who served the bytes. SXG: out of scope; revisit only if a cache-shaped use case appears.
4. **Pattern B credentials → §5.1.1 puppet mode for Phase 3.** Federation-aware protocols see what joins their rooms and federation propagation is unringable; visitor-joins-as-themselves vs moot-relays are *two architecturally different bridges*, not a per-visit toggle. Phase 3 ships **puppet mode** only (matterbridge-style: bot owns membership; visitor messages prefix-attributed to bot; loses per-visitor reactions/E2EE; scales). **Portal mode** (per-visitor ghost MXIDs, full Matrix semantics, bridge-runs-a-homeserver) deferred to Phase 4+ research.
5. **Pattern B scaling → start at single-moot-host.** None of the proposed scaling mechanisms (kith/kin volunteer, distributed relay, time bank) exist yet. Phase 3 ships single-moot-host relay; scaling cliff is documented; resolution returns when the first moot actually hits the cliff.
6. **Scripting host → §5.4 deferred.** Phase 3 starts with compiled-in Rust mods; scripting host probe runs only when a concrete scripted-mod need surfaces. Three candidates named for the probe (Boa for JS mods, Extism for opinionated WASM plugin, Wasmtime for raw WASM). Don't pre-commit before knowing the capability surface.
7. **iroh-docs first concrete consumer → moot membership roster.** Shared mutable state (concurrent additions/removals; multiple moot-admins editing) is a textbook CRDT fit. After roster lands: shared bookmark / pin lists. Co-op session state stays on Cable (durable) or gossip (live), not iroh-docs.
8. **Cable + iroh-gossip presence → split by question shape.** **Cable owns durable membership events** ("Alice joined this cabal at T") because they're the historical record. **Gossip owns live online/typing/cursor** because it's status, not history. Consumer chooses by the question being asked: "was Alice online from X to Y?" → walk the Cable log; "is Alice online now?" → check gossip. Don't fill the log with status-shaped data.
9. **Cable home → stays in Murm.** "The soul of it is bilateral." The trust substrate (host-led, all participants known and admitted, per-cabal Ed25519) is what the layer name tracks, not membership-set cardinality. §5.0 already records this; the Q9 in the prior draft is closed.
10. **Per-mod isolation upgrade trigger → first T1 Bootstrap-category mod (Matrix).** Capability tokens (level 1) ship with the Matrix mod. Matrix is the first mod that touches Bootstrap-category credentials end-to-end and the first that warrants enforcing per-mod scope on slot reads. WASM sandboxing (level 2) follows the scripting host probe.
11. **Mode 2 host churn → §4.8 push self-host + previous-handles chain.** Self-hosting is the recommended path; Mode 2 is a *convenience* with explicit "your handle is fragile" warnings. Migration uses a `mere:previous-handle` rel chained from the new doc to the old, both signed by the same master key, so peers can verify continuity without a live old host. Multi-host publication is a polish track on top.
12. **NIP-05 vs WebFinger toggle → §4.9 per-identity publication policy.** One slot = one toggle ("publish my Nostr identity for discovery") writes both `/.well-known/nostr.json` AND the WebFinger Nostr-link entry. Endpoint set is derived from slot type, not user-selected. Granular per-endpoint config exists in an in-app config editor for advanced users; the default UI doesn't drag everyone through it.

### Still open (deferred branching plans)

1. **Phase 4+ portal-mode bridge.** Per-protocol research probe: Matrix portal-mode requires running a homeserver (or virtualized one). Operationally heavy; cost tier and feasibility evaluated when a concrete community asks for E2EE / per-visitor reactions inside Pattern B.
2. **Pattern B scaling cliff path.** The named candidates (kith/kin volunteer relays, hosted fallback, time bank) need real prototypes. Returns to active design when first moot hits the cliff.
3. **Scripting host capability surface.** §5.4 lists candidates but the probe itself is the open work. Returns when first scripted-mod use case appears.
4. **Distributed Mode 2 publication policy.** §4.8 ships single-host + previous-handles chain; multi-host fan-out and resolver-fallback ordering is polish-tier (Phase 2D extension or Phase 4).

---

## 8. Acceptance Standard

This plan is acceptable when:

1. Every architectural slot named here has a clear owner crate + mod boundary.
2. Per-protocol future mods slot in without touching this plan's structure.
3. Identity vault evolution doesn't require breaking existing Cable / `IdentityProvider` consumers.
4. The "moot-hosted protocol-client renders for non-mod-installed visitors" pattern is implementable end-to-end without unplanned work.
5. Self-host-with-fallback is generalizable from WebFinger to Misfin, Cable seeders, and any future protocol where a 24/7 third-party fallback makes sense.
6. All cross-references to inherited graphshell docs resolve to existing files (audit before declaring complete).

---

## Findings

### 2026-05-05 — first-draft critique fold-in

Critique caught six concrete drift points the original draft glossed over. Recording the corrections so the lessons are durable:

1. **Identity slots are not all bytes.** Matrix's identity surface (per-device keys, cross-signing keys, pickled Olm/Megolm sessions, recovery key) cannot be modeled as a flat `IdentitySlot` variant. `matrix-rust-sdk` already owns a `StateStore` for exactly this. Same shape for ATproto and ActivityPub-with-server-state. Resolution: `IdentitySlot` now bifurcates into **Direct** (vault-modelable, just bytes) and **Bootstrap** (vault holds bootstrap secret + provides encrypted state-dir; SDK owns the rest). §3.1 rewritten.
2. **Recovery is lineage-dependent.** "My recovery phrase recovers everything" is wrong. Locally-derived (Cable cabal keys) is recoverable from master. Locally-generated-externally-registered (Matrix device keys) requires re-verification from another device — *not* recovery. Externally-issued tokens rotate by design. Externally-rooted certs need upstream reissue. §3.4 added.
3. **Discovery ≠ proof.** WebFinger is unauthenticated discovery. Anyone who runs `evil.example` can publish a WebFinger doc claiming to be `@anthropic_ceo` on Matrix. Real proof requires back-claims — the target signs a counterclaim with the protocol's own key. NIP-39 for Nostr; signed-room post for Matrix; ATproto's bidirectional handle ↔ DID flow. §4.1–§4.2 added; §4.6 reshaped.
4. **NIP-05 is its own surface.** Nostr discovery does not ride WebFinger; it has its own `/.well-known/nostr.json`. The earlier draft's `http://nostr.com/protocol/profile` rel was made up. Verso serves both endpoints, populated from the same vault. §4.3 added; §4.6 corrected.
5. **The Murm/Moothold split was wrong-axis.** "1:1 vs many-to-many" makes Matrix DMs / Nostr NIP-17 / IRC `/msg` straddle the boundary. The right axis is **durable group with membership vs ad-hoc peer interaction**. Misfin moves out of Murm (mail-shaped, store-and-forward — wrong layer). Matrix and Nostr go *fully* to Moothold including their DMs. Cable cabals are arguably also Moothold; flagged as open question rather than re-routed mid-Phase-2. §5.0 added.
6. **Roadmap-as-flat-list hides 100× cost variance.** T1 wrap-maintained-SDK (matrix-rust-sdk, nostr-sdk, irc, atrium-rs) ≠ T2 wrap-thin-or-unmaintained (Tox/rstox, Hypercore) ≠ T3 implement-from-spec (Cable, MLS, Briar). §5.5 now lists tiers explicitly; Phase 3 first batch is restricted to T1.
7. **Vault doesn't isolate mods.** A compromised Matrix mod can read the Nostr `nsec` slot in the single-process model. At-rest encryption + per-session unlock + audit log defend against passive attacks (disk theft, swap), not active in-process exfiltration. Resolution: §3.6 names the level-0 assumption explicitly and sketches the level 1 → level 3 upgrade path. The vault API is shaped so the upgrade is additive.
8. **Architecture diagrams must not blur transport ownership.** iroh-gossip is transport (lives in `mere-transport`); Murm and Moothold are *consumers*, not participants in the gossip layer. §2.1 corrected; iroh-* primitives now uniformly attributed to `mere-transport` with workloads listed.

These are not just doc edits; they reshape the Phase 2C / 2D / 3 work breakdown:

- The **Bootstrap slot category + encrypted state-dir** is new vault scope (Phase 2C).
- **Back-claim collection / verification** is new track (Phase 2D / Phase 3 per protocol).
- **Cost-tier-aware ordering** of Phase 3 mods (T1 first: IRC → Nostr → Matrix → ATproto).
- **NIP-05** is a distinct endpoint, not piggybacking on WebFinger (Phase 2D).
- **Cable's home (Murm vs Moothold)** is now an open question, with a deliberate "no churn until Phase 3" deferral.

### 2026-05-05 — open-question resolution pass

User answered all 12 open questions. Net effect: nine questions become decisions baked into the plan; three (portal-mode research, Pattern-B scaling, scripting-host probe) remain genuinely open and are listed as deferred branching plans rather than blockers. Decisions worth surfacing:

- **Unlock UX falls out of slot-declared tier.** §3.6 now ships Session / Short-TTL / Per-use as a slot-attribute, not a global setting. Methods (passkey / keyring / passphrase / hardware token) are orthogonal.
- **Multi-profile is Element-style.** Each mod holds N concurrent sub-instances; close to reclaim. Drives a `ProfileId`-keyed map inside every mod.
- **Mode 2 docs need JWS, not just TLS.** §4.7 makes signed bytes the third-party-host trust anchor; SXG ruled out.
- **Migration is verifiable: `mere:previous-handle` chain.** §4.8. Continuity proof without a live old host. The most distinctive design move in the plan.
- **One slot = one publish toggle.** §4.9. NIP-05 + WebFinger fire from the same user action; granular config exists for advanced users in-app, not as default UI.
- **Cable stays in Murm.** "The soul of it is bilateral." §5.0 closed.
- **Pattern B = puppet mode for Phase 3.** Portal mode is a research follow-up; puppet's per-visitor-attribution loss is a documented Phase 3 limitation, not a defect.
- **iroh-docs first consumer = moot membership roster.** Shared mutable state with concurrent admin edits is the textbook CRDT case.
- **Cable+gossip split by question shape.** Cable = durable history; gossip = live status. Don't fill the log with status.
- **Capability tokens (isolation level 1) ship with Matrix.** First Bootstrap-category mod is the trigger. Plan-level decision; Phase 3 mod lands with vault-side enforcement.
- **Scripting candidates: Boa / Extism / Wasmtime.** §5.4 names the trio; commits to *not picking yet* because the capability surface isn't visible.

---

## Progress

### 2026-05-05

- Plan drafted from architectural-research synthesis turn (Cable+iroh layering, identity-vault, WebFinger self-host pattern, primitive moot node pattern).
- Premises sourced from user directly: layering yes, vault now, self-host w/ fallback applicable to all self-hostable protocols, IRC as first primitive moot node concrete; "Element matrix rooms on the web, but on IPFS" framing.
- Reserves the ALPN ledger in §2.3.
- DOC_README index updated in same session.
- No code changes yet; this plan is design-only. Code work begins with Phase 2C iroh transport (Cable migration Phase 5 prerequisite).
- **Same-day critique fold-in**: §3.1 (Direct vs Bootstrap slots), §3.4 (lineage-dependent recovery), §3.6 (capability-scoping threat model), §4.1–§4.3 (discovery vs proof; NIP-05 separation), §4.6 (corrected endpoint shapes; back-claim collection), §5.0 (Murm/Moothold split re-derivation), §5.5 (cost tiers), §7 (4 new open questions) — see Findings entry above.
- **Same-day open-question resolution pass**: §3.3 (Element-style multi-profile sub-instances), §3.6 renumbered to §3.7 with new §3.6 (slot-declared unlock tiers + methods), §4.7 (JWS for Mode 2/1.5; SXG ruled out), §4.8 (multi-host + `mere:previous-handle` chain), §4.9 (one-toggle per-identity publication), §5.0 (Cable stays in murm — closed), §5.1.1 (puppet mode for Phase 3; portal deferred), §5.4 reshaped (Boa/Extism/Wasmtime probe deferred until capability surface visible), Phase 3 table reshaped (T1 ordering, single-host relay only, capability tokens with Matrix, iroh-docs roster as first concrete consumer), §7 rewritten as Question Resolution Log + four still-open branching plans. See second Findings entry for decision surfacing.

### 2026-05-05 — Phase 2C foundation work landed

Code-side execution against §6 Phase 2C foundations:

- **2a Cable migration plan supersession** — `2026-05-04_cable_migration_from_verso_plan.md` header now references this plan as authoritative for Phase 5+ scope (cabal store integration et al).
- **1a iroh transport** — `mere-transport::IrohTransport` (`crates/mere/mere-transport/src/iroh_transport.rs`) on iroh 0.98.2 / iroh-blobs 0.100. Uses `Builder::empty()` preset with explicit `iroh::tls::default_provider()` to keep tests off the n0 relay/DNS network. Cross-registers peers via in-process address book (`add_peer(EndpointAddr)`); production discovery is a follow-up. The `Connection` is held alongside the bi-stream pair to prevent connection-close-races. iroh API surprises: `NodeId` → `EndpointId` rename, `NodeAddr` → `EndpointAddr` rename, `accept_bi()` blocks until connecting side writes (Cable already does that). 2 new tests; mere-transport test suite at 17.
- **1b identity vault skeleton** — `mere-identity::vault` (`crates/mere/mere-identity/src/vault.rs`) ships `IdentityVault<S>`, `Profile`, `IdentitySlot::{Direct, Bootstrap}`, `CredentialLineage`, `UnlockTier`, `IdentityStorage` trait, and `InMemoryStorage` test fixture. Implements `IdentityProvider` against the current profile so existing Cable consumers keep working unchanged. 6 new tests.
- **murm-side iroh wiring** — `cable_snapshot_sync_via_iroh_transport` test in murm exercises Murm-over-IrohTransport end-to-end (3 Cable posts pushed and ingested over real loopback QUIC). Also caught and fixed a stream-truncation race in `push_cabal_to_peer`: connection-drop was racing the Bob-side stream-drain on multi-post pushes (2/3 posts arriving). Fix: half-close-then-wait-for-EOF as ACK before drop. Documented in `lib.rs`.
- **iroh-blobs integration v0** — `mere-transport::blobs::BlobStore` (`crates/mere/mere-transport/src/blobs.rs`) wraps `iroh_blobs::store::mem::MemStore` with a local-only put/get/has facade and a `BlobHash` newtype. **Network transfer (BlobsProtocol on the accept side, peer-to-peer fetch) is intentionally deferred** until the first concrete consumer (Cable attachments) lands — wiring `BlobsProtocol` into the existing per-ALPN queue accept loop wants iroh's `Router` pattern, and that refactor is risk if done speculatively. 7 new tests.

**Workspace test count**: 13 mere-identity (was 7, +6 vault) + 22 mere-transport (was 13, +2 iroh transport, +7 blobs) + 13 murm (was 12, +1 iroh integration) + 81 murmuring (unchanged) = **129 passing**.

### 2026-05-05 — Phase 2C+ continuation (Router refactor, gossip, passphrase storage, IRC plan)

Code-side and plan-side execution against the §6 phase tracks user requested in order a→b→c→d:

- **(a) Router refactor + iroh-blobs network exchange** — `IrohTransport`'s manual accept loop replaced with `iroh::protocol::Router`. Per-ALPN queues now run as `QueueProtocolHandler: ProtocolHandler` instances registered with the Router; iroh's `MemoryLookup` replaces our home-grown peer_book (one less abstraction). `BlobStore::fetch_from(transport, peer, hash)` does the multi-stream iroh-blobs fetch over a `connect_raw` connection. `IrohTransport::bind_with_blobs` registers `BlobsProtocol` against the same Router. New test `fetch_from_remote_peer_round_trips_blob` validates the end-to-end p2p blob flow. **23 mere-transport tests** (+1).
- **(b) iroh-gossip integration** — `IrohTransport` grows a builder pattern (`IrohTransport::builder(master).gossip().bind()`) and an `iroh_gossip::Gossip` handler registered with the same Router. `IrohTransport::gossip() -> Option<&Gossip>` exposes the iroh-gossip API directly (no wrapper layer in v0). New test `paired_iroh_transports_exchange_gossip_message` runs alice + bob as gossip peers on the same loopback endpoints (which also serve blobs and Cable), broadcasts and receives a topic message. **24 mere-transport tests** (+1).
- **(c) PassphraseEncryptedStorage backend** — `mere-identity::passphrase_storage::PassphraseEncryptedStorage` ships as a production-grade `IdentityStorage` backend. Argon2id-derived KEK, ChaCha20-Poly1305 authenticated encryption per profile, single-file JSON storage with atomic-rename writes. `serde::{Serialize, Deserialize}` derives added to `CredentialLineage` and `UnlockTier` to support the wire format. Wrong-passphrase rejection at `open()` time (decrypts an existing profile to verify). 6 new tests including a "ciphertext doesn't contain plaintext secrets" check. **19 mere-identity tests** (+6).
- **(d) IRC mod plan branch** — [`design_docs/moothold_docs/implementation_strategy/2026-05-05_irc_mod_plan.md`](../../archive_docs/2026-09-02_retired_plans/2026-05-05_irc_mod_plan.md) drafted. Crate scaffold (`mere-mod-irc`), vault slot shape (`Direct`, `kind="irc"`, `lineage=ExternallyIssued`, `unlock_tier=ShortTtl 15min` default), Pattern A (orrery-pinned) lifecycle, Pattern B (moot-relayed, puppet mode only per protocol architecture plan §5.1.1) flows, ALPN reuse (`mere/moothold/primitive-node/v1` — no new ALPN), IRCv3 capabilities scoped (sasl + server-time + away-notify + multi-prefix + extended-join + message-tags), three-phase delivery (3.0 mod scaffold + Pattern A → 3.1 Pattern B puppet relay → 3.2 comms applet UI), 5 still-open questions deferred to per-protocol research. DOC_README index updated.

### 2026-05-31 — landscape refresh + iroh-docs caution

- External p2p landscape grounded against current sources in the [murm/p2p landscape brief](../research/2026-05-31_murm_p2p_landscape_brief.md). §2 iroh layering validated: iroh 1.0.0-rc.1 (2026-05-27), noq stabilizing, and two external stacks (Holochain, p2panda) converged on iroh.
- **iroh-docs caution (added to §2.4).** iroh 1.0 stabilizes the connection layer plus noq, not the higher protocols; iroh-docs is a community meta-protocol over blobs+gossip. The "iroh-docs first concrete consumer = moot roster" track (§6 Phase 3) should treat iroh-docs as a swappable projection, with p2panda-sync and willow-rs as fallbacks.
- The §3 vault skeleton plus passphrase backend are built (`persona/identity`); the §5 moot/bridge/primitive-node layer remains a 61-LOC stub. No §5 code started. p2panda's modular rewrite is now a substrate adopt-candidate for the event-DAG core (the substrate brief's deferral predates it).
