# Cable Migration From Verso Plan

**Date**: 2026-05-04
**Status**: Active for Phases 0–4. **Phase 5+ scope (cabal store integration, retention, multi-channel) is superseded by [`../../mere_docs/implementation_strategy/2026-05-05_protocol_architecture_plan.md`](../../mere_docs/implementation_strategy/2026-05-05_protocol_architecture_plan.md) §6 (Phase 2C onward).** That plan owns the workspace-wide phase numbering for Cable's persistent-store wiring, iroh-toolkit layering, and downstream Cable + iroh-gossip / iroh-blobs integration.
**Scope**: Code-level migration of Cable application logic from Verso (in the inherited `graphshell/` repo) to **Murm** (in the new Mere workspace). Defines what moves, what stays, and the migration phases.
**Related**:
- Inherited: [`graphshell/design_docs/verso_docs/implementation_strategy/2026-03-28_cable_coop_minichat_spec.md`](../../../../graphshell/design_docs/verso_docs/implementation_strategy/2026-03-28_cable_coop_minichat_spec.md) — Cable adoption plan
- Inherited: [`graphshell/design_docs/verso_docs/technical_architecture/VERSO_AS_PEER.md`](../../../../graphshell/design_docs/verso_docs/technical_architecture/VERSO_AS_PEER.md) — pre-migration Verso role spec
- Workspace: [`mere/crates/murm/`](../../../crates/murm) — destination supercrate
- Workspace: [`mere/crates/murmuring/`](../../../crates/murmuring) — protocol-core inner layer
- Workspace lexicon: [`../../2026-05-04_lexicon_brief.md`](../../2026-05-04_lexicon_brief.md)

---

## Cable Migration Plan

### Phases and progress

#### Phase 0 — Reservation (DONE 2026-05-04)

- ✓ `murm` and `murmuring` crate names reserved on crates.io
- ✓ `murm/src/lib.rs` and `murmuring/src/lib.rs` document intent
- ✓ Workspace structure under `mere/` accommodates both crates

#### Phase 1 — Boundary doc (next)

Write `mere/crates/murm/MURM_AS_BILATERAL.md` (or under `murm_docs/technical_architecture/`) — the canonical authority/boundary doc establishing Murm's role. Parallel to the inherited `VERSO_AS_PEER.md`. Resolves:

- What does Murm own?
- What does Verso (or its successor `verso-tile` + `inker`) keep?
- What does the now-vacant peer-transport / iroh-owning role become — does it stay in `verso-tile`, move to a new `peer-transport` crate, or live in `inker`?
- How does Murm's identity model relate to graphshell-core's master-keypair concern (since identity isn't strictly a comms-only concern)?

This boundary doc is a prerequisite for migration — until it's signed off, the split between Murm and what-was-Verso is ambiguous.

#### Phase 2 — Code-level extraction

Extract Cable-specific logic from the inherited graphshell repo:

- **Cable wire-protocol encoding/decoding** → `murmuring/src/protocols/cable/` (it's a protocol; lives in the protocol-core crate)
- **Bilateral identity derivation** (`verso_master_secret` + `cabal_key` → per-cabal Ed25519 keypair, per `cable_coop_minichat_spec.md` §2.2) → `murm/src/identity/` (it's bilateral-identity, not protocol-specific)
- **Cabal key derivation from session ID** (per spec §2.3) → `murm/src/identity/`
- **Post type encoding/decoding** (per spec §2.4) → `murmuring/src/protocols/cable/`
- **Channel time-range request sync** (per spec §2.6) → `murmuring/src/protocols/cable/`
- **Moderation integration** (host-as-admin-seed model, per spec §3) → `murm/src/moderation/`
- **Storage model** (Mode A ephemeral, Mode B persistent named cabal, per spec §4) → `murm/src/storage/`
- **GraphIntent variants** (`SendCoopChatMessage`, etc., per spec §6) → handled at the `mere`-level intent layer, not in `murm` itself; `murm` exposes lower-level send/subscribe API

#### Phase 3 — Wiring

- Update `mere` crate to depend on `murm` for bilateral comms
- Update `verso-tile` to NOT own bilateral chat (it owns rendering surfaces only)
- Update `inker` to coordinate with `murm` for any rendering needs Cable produces (none expected — Cable is text/structured, not engine-rendered, but a chat panel UI may want to render Cable posts as rich content; that's a separate concern)
- Reproduce `--features cable` gating from the inherited spec, now keyed off `murmuring`'s feature for the Cable protocol adapter

#### Phase 4 — Delete from Verso

Once Murm is verified working (Phase 1 boundary doc + Phase 2 extraction + Phase 3 wiring all integration-tested):

- Remove Cable-specific code from `graphshell/ports/verso/` (or whatever path it lives at in the inherited tree)
- Update inherited `verso_docs/` to point Cable-related references to `mere/design_docs/murm_docs/`
- Add a `> Superseded by` header to `2026-03-28_cable_coop_minichat_spec.md` pointing here

#### Phase 5 — Cabal store integration

The persistent-cabal store described in spec §4.2 (Mode B) was deferred in the original plan. It now lives in `murm/src/storage/persistent_cabal/`:

- redb/fjall schema as specified (Posts table, ChannelTimeline table, ChannelHeads, ChannelState, PeerVectors)
- Full year of 100-user activity = ~146 MB, comfortable for local
- Per-cabal TTL policy as a configurable knob

### What moves vs. what stays

| Code area | Lived in (graphshell) | Moves to (mere) | Notes |
|-----------|----------------------|-----------------|-------|
| Cable wire protocol (encode/decode) | `verso/cable/` | `murmuring/src/protocols/cable/` | Pure protocol logic |
| Bilateral identity derivation | `verso/identity/` | `murm/src/identity/` | Independent of any specific protocol |
| Co-op session lifecycle | `verso/coop/` | `murm/src/coop/` | Bilateral-by-construction |
| Cable post types + sync | `verso/cable/sync/` | `murmuring/src/protocols/cable/sync/` | Protocol-specific |
| Moderation seed model | `verso/cable/moderation/` | `murm/src/moderation/` | Cross-protocol concept (Murm-level), not Cable-specific |
| Persistent cabal store | (deferred in original spec) | `murm/src/storage/persistent_cabal/` | New work |
| **iroh transport** | `verso/transport/iroh/` | **STAYS** — likely in a new `peer-transport` crate or remains under verso/inker | Transport is engine-adjacent, not comms-adjacent |
| **ALPN registration** | `verso/transport/alpn/` | **STAYS** — same as iroh transport | |
| **Master keypair (`verso_master_secret`)** | `verso/identity/master_key/` | **STAYS** in `mere`-core or `graphshell` — it's identity, not bilateral-comms | Murm consumes it via API, doesn't own it |
| **Engine management (Servo/Wry/Nematic)** | `verso/` | **STAYS** — moves to `inker` (engine controller) and `verso-tile` (rendering surfaces), NOT to `murm` | Different domain |
| **Browser viewer registry** | `verso/viewers/` | **STAYS** — `verso-tile` for tiles, `inker` for engine routing | Different domain |
| **Webview lifecycle** | `verso/webview/` | **STAYS** — `inker` (lifecycle) + `verso-tile` (placement) | Different domain |
| **Gemini/Gopher/Finger servers** | `verso/gemini/server/` etc. | **STAYS** — moves to `nematic` (smolweb engine) or its own server crate | Different domain (server, not comms) |

The rule of thumb: **if it's about engines, rendering, or browser-viewer concerns, it stays in verso/inker/nematic/verso-tile. If it's about person-to-person communication via a protocol, it moves to murm.** Identity straddles both — master keypair stays at the shared-core level, per-protocol derived keys are per-protocol.

### Open questions

1. **Master keypair location** — spec says `verso_master_secret`. After migration, where does the master keypair live? Options:
   - `mere`-core (top-level) — consumed by both `murm` (for per-cabal derivation) and any peer-transport crate (for iroh NodeId)
   - A dedicated `mere-identity` or `graphshell-core` crate that both `murm` and the transport crate depend on
   - Stays in `graphshell` (the shell crate) for now since it owns OS keychain integration

   Recommendation: `mere-identity` crate (new, not yet reserved), depends on OS keychain via `graphshell` adapter. `murm` depends on `mere-identity`. Defer reservation of this crate name until needed.

2. **iroh transport location** — does it become a separate `peer-transport` crate, or live inside an existing crate? Options:
   - New `peer-transport` crate (bare-bones iroh wrapper, owned by Mere workspace)
   - Stays under `inker` (since `inker` already owns engine-side transport)
   - Goes to `verso-tile` (since rendering surfaces consume content over transport)
   
   Recommendation: new `peer-transport` crate. Engines have their own transport (Servo's net layer); comms has its own transport (iroh + Cable wire). Keeping them separate is cleanest. Defer reservation until the boundary is clearer.

3. **Cabal store storage backend** — fjall (log) + redb (snapshots) + rkyv (serialization) is the existing pattern. Does Murm own its own store, or share with `mnem`? Options:
   - `murm` owns its store; cabal posts never enter mnem
   - `mnem` owns the unified personal store; Murm writes cabal posts as a memory class within mnem
   
   Recommendation: `murm` owns its own store under a `murm`-managed root directory. Mnem stays focused on private browsing memory. A user's Cable cabal posts are *social* artifacts that may someday be promoted to mnem (via explicit clip), but they live in the comms layer by default. Matches the "do not flatten cross-domain semantics" rule from the inherited [`COMMS_AS_APPLETS`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/social/comms/COMMS_AS_APPLETS.md).

4. **Co-op session ↔ moothold relation** — Co-op sessions (per `coop_session_spec.md`) are *bilateral* by definition (host + guests, all known). But a co-op session can have governance, multi-party state, and hand-off semantics that look like a small moot. Are co-op sessions in `murm` (bilateral), in `moothold` (community), or do they straddle?
   
   Recommendation: co-op sessions stay in `murm` because the *transport* and *trust model* are bilateral (each guest is in direct relation with the host). The session-state semantics overlap with moothold but the comms substrate is bilateral. If Mark/team want to unify them later, that's a future refactor, not a Phase 1 concern.

5. **Cable.rs upstream contributions** — the inherited spec notes cable.rs (the upstream Rust implementation) lacks a tokio-native peer manager and persistent storage. Murm will likely need both. Should we contribute these upstream?
   
   Recommendation: defer to Mark per the saved feedback memory (`feedback_upstream_contributions.md` says don't suggest Servo upstream PRs; cable.rs is different but the same caution applies — the user prefers to not be on the hook for upstream maintenance burden). For now, build inside Murm without upstream commitment; revisit later if the work stabilizes.

### Acceptance criteria

The migration is complete when:

1. ✓ `murm` and `murmuring` crates compile in the Mere workspace with Cable functionality available behind a feature flag
2. ✓ Bilateral chat works in a Mere build with the same UX as the inherited spec describes
3. ✓ Persistent cabal store works (Mode B) with redb+fjall+rkyv backing
4. ✓ Verso (in graphshell repo) no longer contains Cable-specific code
5. ✓ Inherited [`cable_coop_minichat_spec.md`](../../../../graphshell/design_docs/verso_docs/implementation_strategy/2026-03-28_cable_coop_minichat_spec.md) is marked superseded with a pointer to this plan
6. ✓ All `verso_docs/` references to Cable redirect to `murm_docs/` here
7. ✓ Tests cover: round-trip Cable encoding, keypair derivation determinism, persistent cabal store CRUD, sync convergence between two test peers, moderation propagation

## Findings

(None yet — populate as research/implementation surfaces relevant findings.)

## Progress

### 2026-05-04

- Plan drafted alongside Mere workspace scaffolding
- Murm + Murmuring crate scaffolds in place; Cable code not yet extracted
- Phase 0 (reservation) complete
- Phase 1 (boundary doc) is the next concrete step — pending Mark's decision on the open questions above (especially Q1 master keypair, Q2 transport, Q3 storage)
