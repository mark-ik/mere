# Event-DAG Substrate Brief

**Date**: 2026-05-07
**Status**: Proposal (architectural pivot under review)
**Scope**: Replaces specific wire-format, sync-layer, schema-locality, privacy-transport, and persona-design decisions in the 2026-05-05 protocol architecture plan. The bulk of that plan (iroh layering, identity vault, self-host-with-fallback, protocol-mod pattern) remains load-bearing.
**Related**:

- [`2026-05-05_protocol_architecture_plan.md`](2026-05-05_protocol_architecture_plan.md) — the prior plan; this brief refines specific sections (see §11).
- [`2026-05-10_graph_cluster_namespaces_brief.md`](2026-05-10_graph_cluster_namespaces_brief.md) — namespace-as-graph-cluster design direction emerging from §14.
- `2026-05-09_post_engine_layer_priorities.md` — current forward-looking plan; the original 2026-05-06 graphshell migration plan is archived at `../../archive_docs/2026-05-09_engine_layer_complete/2026-05-06_graphshell_migration_plan.md` (engine layer landed before today's BLAKE3 unification work).
- Inherited: `../../../../../graphshell/design_docs/verse_docs/implementation_strategy/engram_spec.md` — the engram envelope spec that anchors the schema-at-engram-boundary insight.
- Inherited: `../../../../../graphshell/design_docs/verse_docs/implementation_strategy/2026-02-26_intelligence_memory_architecture_stm_ltm_engrams_plan.md` — STM/LTM/Distillery framing.

---

## 0. Why this brief exists

The 2026-05-05 plan committed to:

- Cable as the bilateral-chat wire format (BLAKE2b-locked).
- Nostr as a first-class internal protocol (NIP-05 plumbing in mere-identity discovery).
- Matrix / Nostr / ATProto / ActivityPub / IRC as `mooting`-internal protocol mods dispatched via trait abstraction.
- An implicit assumption that "which sync layer" is the architectural choice.

Today's analysis (2026-05-07) shifted those decisions:

- **Drop Cable.** BLAKE3 unification. Replace with a Mere-native event DAG that keeps Cable's *semantic* model (signed-post DAG, channels, time-range sync) while shedding the wire format and BLAKE2b lock-in.
- **Mere-native event DAG as the protocol identity.** Sync layers (iroh-docs, p2panda, Willow, NextGraph) become projections of the event DAG, not the architectural identity itself.
- **Schema crystallizes at the engram boundary**, not at write time. eidetic stays untyped; engrams carry the schema discipline.
- **Privacy transport as an optional per-moot policy** (Veilid for sensitive communities; iroh for ergonomic default).
- **Multi-protocol moot hosting + outbound bridges as fallback.** Matrix / Nostr / ATProto / ActivityPub move from `mooting`-internal trait dispatch to **per-protocol sibling adapter crates** (`mooting-mere`, `mooting-matrix`, `mooting-nostr`, …) when the foreign protocol can host moot semantics natively (Pattern A); to **outbound bridge crates** (`mere-bridge-*`) when it cannot (Pattern B). The middlenet pattern, applied to p2p protocols. (Earlier in this brief I framed this as "bridges only"; §7 restores the Pattern A / Pattern B distinction after a later-day revision.)

This brief captures those shifts. The 5/5 plan's identity vault (§3), self-host-with-fallback (§4), and the *concept* of protocol mods (§5) survive; their *internal vs bridge* framing changes.

---

## 1. The architectural inversion

The 5/5 plan's framing was: pick a sync protocol; build moot/chat/graph semantics on top.

The revised framing is: **define Mere's event DAG; let sync layers be projections.**

```text
                  Mere-native event DAG
                  (signed events, BLAKE3 hashes,
                   capability fields, target refs)
                          ▲
                          │ event grammar
                          │
      ┌───────────────────┼───────────────────────────┐
      │                   │                           │
      ▼                   ▼                           ▼
  iroh-docs          p2panda             Willow / iroh-blobs
  (K/V replica)      (typed schemas      (capability-scoped
                      at engram layer)    namespaces, RBSR sync)
                          │
                          ▼
                  iroh transport
            (QUIC, peer auth via NodeId,
             gossip, blob content addressing)
```

The event DAG is what makes mere *mere*. Sync backends are implementation details that can be evaluated, swapped, and combined without changing the protocol's identity.

---

## 2. BLAKE3 unification

All hash uses across the workspace move to BLAKE3:

- `mere-identity`: keypair derivation = `BLAKE3-256(master_seed || salt)` in keyed-hash mode (replacing `BLAKE2b-256(...)`).
- `iroh-blobs`: already BLAKE3 (no change).
- Mere event hashes: BLAKE3-256 of the canonical CBOR encoding.
- Engram envelope content addressing: BLAKE3 (matches engram_spec adopted standard, IPFS CIDv1 with BLAKE3).
- Future tessera receipts, governance signatures: BLAKE3 over the canonical encoding.

**Note on Willow (revised 2026-05-10).** Willow'25 specifies WILLIAM3 as its eventual payload hash — verified as a Bab tree-mode modification of BLAKE3 ([Bab spec](https://worm-blossom.github.io/bab/), [Willow'25 spec](https://willowprotocol.org/specs/willow25/)) with three concrete differences from raw BLAKE3: domain-separated IV constants, zero chunk indices in leaf computation, and length-in-inner-nodes (parameter `t`). Differences #2 and #3 are load-bearing for one specific guarantee: **constant-size slice-verification proofs.** Difference #1 is pure domain separation.

However, **the willow-rs reference implementation is BLAKE3-default today**: `willow_25::PayloadDigest25` is a `blake3::Hash` newtype, and the doc comment marks WILLIAM3 as future work. Every willow-rs crate (`data-model`, `meadowcap`, `wgps`, `store_simple_sled`, `sideload`) is fully generic over `PayloadDigest`, with no WILLIAM3-specific assumptions in the trait surfaces. The chunk-vs-byte conversion that WILLIAM3 would eventually require is already abstracted as pluggable function pointers (`chunk_to_byte_fun` in `wgps/src/lib.rs`), defaulting to identity functions for BLAKE3.

**Implication:** if mere adopts Willow's data model + meadowcap + WGPS sync, BLAKE3 carries through unchanged across the entire willow-rs stack. There is no "WILLIAM3 cost" to calibrate — the cost was illusory under the willow-rs reality. The future WILLIAM3 swap only matters if mere ever needs Bab's constant-size slice-verification proofs, which it does not (iroh-blobs' BAO already provides logarithmic-proof verifiable streaming under BLAKE3, sufficient for all mere use cases).

---

## 3. Schema crystallizes at the engram boundary

The clearest architectural insight from the 2026-05-07 conversation: **mere does not need users to declare schemas when they save state.** Schema is imposed at distillation, not at write.

```text
   eidetic (untyped)        graph layer (weakly typed)        engram (schematicized)
   ───────────────          ──────────────────────────        ──────────────────────
   raw blobs by key,        nodes have URLs, titles,          TransferProfile envelope,
   AWAL, traversal logs,    positions, thumbnails, but        validation classes,
   session state.           no typed predicates / RDF.        EngramMemory items,
                                                              ranking-policy artifacts,
   user just scrolls.       implicit shape, not declared.     governance receipts.

                            ──────► Distillery ──────►
                            (the moment schema is imposed,
                             because the recipient needs to
                             validate / merge / interpret)
```

Why this matters for the protocol stack:

- **The Mere-native event DAG only needs a small typed event grammar** (ChatMessage, GraphAddNode, GraphAddEdge, Annotation, Vouch, ModerationAction, EngramSubmission, …). It does not need to type *content*; payloads are bytes addressed by hash.
- **p2panda's schema discipline maps naturally to the engram layer**, not the local-event layer. If p2panda earns a slot, it is at engram publishing / cross-moot exchange, not at local chat or local graph state.
- **RDF / NextGraph semantic typing** can attach to engrams as *optional* metadata for cross-moot semantic queries, without forcing it on local-memory writes.
- **Local browsing stays low-friction.** The schema never appears in the user's hot path.

The engram envelope is also the right place for ranking-policy artifacts, redaction rules, and trust modifiers per `engram_spec.md`. Distillation is therefore not just "compress local memory into a portable payload" — it is **the act of authoring an engram**, and authorship implies schema.

---

## 4. The Mere event grammar

Working draft of the event shape — to be refined as the prototype lands:

```rust
struct MereEvent {
    version: u16,
    space_id: SpaceId,           // moot, personal, shared session, …
    author_id: AuthorId,         // master pubkey or per-context derived
    device_id: DeviceId,         // for multi-device disambiguation
    event_type: EventType,
    parents: Vec<EventHash>,     // BLAKE3 refs into the DAG
    target_refs: Vec<TargetRef>, // semantic refs (node IDs, thread IDs, …)
    payload_hash: BlobHash,      // BLAKE3 of payload (stored as iroh-blob)
    timestamp: HybridTimestamp,  // wall-clock + Lamport, for ordering
    capabilities: Vec<CapabilityRef>,  // which cap chain authorized this
    signature: Ed25519Signature, // over canonical encoding above
}

enum EventType {
    ChatMessage,
    GraphAddNode,
    GraphAddEdge,
    GraphRenameNode,
    GraphRemove,
    Annotation,
    Vouch,
    ModerationAction,
    EngramSubmission,
    MembershipChange,
    PresenceHeartbeat,
    // … kept small; new types require deliberate design
}

enum SpaceId {
    Moot(MootId),
    Personal(UserMasterPubkey),
    SharedSession(SessionId),
    Demesne(DemesneId),
}
```

Key properties:

- **Append-only, signed, hash-linked.** Each event references its parents via BLAKE3 hashes. The DAG is the source of truth.
- **Payloads are content-addressed.** Large payloads (chat attachments, engrams, graph snapshots) live as iroh-blobs; the event carries the BLAKE3 hash. Small payloads inline as bytes.
- **Capabilities are explicit.** Every event carries the capability chain that authorized it. Verification: walk the cap chain against meadowcap-shaped (or simpler internal) capability scope.
- **Hybrid timestamps.** Wall-clock + Lamport handles "what time did this happen" and "what's the causal order" without forcing one to lie.
- **Event types stay small.** Adding a new `EventType` is a deliberate protocol-version event. Discipline is the point.

This shape is wire-format-agnostic: CBOR encoding is the obvious first choice, but a BARE or Protobuf encoding could substitute later. The signature canonicalization rule is what matters for interop.

---

## 5. Sync backends, evaluated as projections

Each candidate evaluated as: "can it carry MereEvents, and at what cost?"

**iroh-docs (the baseline; entry revised 2026-05-10).** Iroh's own key-value replicated docs. BLAKE3-native, already in the workspace, simplest possible substrate. **Already does Range-Based Set Reconciliation** per [Meyer 2022](https://arxiv.org/abs/2212.13567) — verified against `n0-computer/iroh-docs` v0.99.0 (2026-05-08), unchanged across recent commits. Two-week-behind peers converge in O(log n) round-trips proportional to divergence size, not set size. Hybridized with iroh-gossip for live broadcast of new entries. Pros: zero ecosystem risk, fewest deps, RBSR sync built-in. Cons: no native capability scoping (you build it on top), no hierarchical paths (you key the K/V yourself), flat K/V model.

→ Best fit for: **the baseline projection.** Start here. It's enough for personal-graph state and small moots. The RBSR sync efficiency assumption that previously motivated "migrate to Willow at scale" is wrong — RBSR is already in iroh-docs. Migrate to Willow only for the data model + meadowcap, not for sync.

**p2panda.** Append-only signed logs (Bamboo), schemas, document materialization. BLAKE3 + Ed25519 native (after their own BLAKE2b → BLAKE3 migration — useful precedent). Pros: typed schemas align with engram envelopes; precedent for migrating off BLAKE2b. Cons: schema-first commitment; smaller ecosystem; the operation grammar overlaps the MereEvent grammar in confusing ways if used as the substrate.

→ Best fit for: **engram publishing and cross-moot exchange**, where typed schemas earn their cost. Not for local event DAG storage.

**Willow (revised 2026-05-10/11).** 3D namespace/subspace/path data model, capability-based access (meadowcap), Range-Based Set Reconciliation, optional Confidential Sync handshake. Pros: meadowcap is the most thoughtful design for structural namespace delegation at scale; namespace/subspace/path maps cleanly to moot organization (and even more cleanly to graph-cluster-derived paths — see §14). willow-rs is BLAKE3-default today, fully PD-generic, and integrates over arbitrary byte transports. Cons: 3D data model is more machinery than flat K/V if we don't actually use the path dimension; iroh-willow (the existing iroh-bound integration) is dormant pre-alpha (last meaningful commit 2025-10-27, open correctness bug in capability delegation, [issue #5](https://github.com/n0-computer/iroh-willow/issues/5)) — we'd build our own willow-rs-on-iroh integration in `mere-namespace` rather than depend on it; meadowcap is design-stable but not an expressive moot-policy layer. §8.8 now treats Biscuit as the policy-token candidate and Keyhive as the live group/key-state eval, not as a direct Willow replacement.

→ Best fit for: **`mere-namespace`** (a new crate). Adopt for the data model + meadowcap + WGPS sync over iroh streams. The substrate-level RBSR efficiency is *not* a reason to adopt — iroh-docs already does RBSR (verified against `n0-computer/iroh-docs` v0.99.0, 2026-05-08). Adopt for the 3D data model, path-keyed range queries, and meadowcap-style capability scoping; or skip if those are not load-bearing. Per §14 (graph-cluster namespaces) they likely are.

**NextGraph / Lofire (revised 2026-05-10).** CRDT-based decentralized data + identity, RDF/SPARQL graph model. Pros: most architecturally aligned with mere's "graph as document" premise; cohesive stack; **as of FOSDEM 2026 has shipped Social Queries SPARQL and a reactive ORM TypeScript SDK with real users — no longer pre-production.** Cons: still betting on one project's roadmap; RDF-first model misaligned with mere's spatial/experiential graph; significant overlap with already-built mere-identity / eidetic.

→ Best fit for: **engram-layer semantic interop.** Take design lessons (RDF-as-engram-metadata, DID interop) and consider NextGraph compatibility for cross-stack engram exchange when mere ever ships engrams to RDF-shaped consumers. Do not adopt as substrate.

**Veilid.** Privacy-routed P2P substrate; DHT routing with built-in onion-routing privacy. BLAKE3 + Ed25519 native. Pros: closes the metadata-leakage gap iroh leaves open; live development; opinionated. Cons: less ergonomic than iroh; smaller ecosystem; replaces transport, not sync.

→ Best fit for: **optional per-moot privacy transport** (see §6).

**Hypercore / Pear.** Append-only signed feeds, mature ecosystem. BLAKE2b-legacy with BLAKE3 migration in Pear runtime. Cons: significant historical baggage; the model is more single-feed than DAG.

→ Best fit for: **not chosen.** iroh's own primitives cover the same ground without the legacy.

**Local-first CRDT stacks (Loro, Automerge, Yjs).** CRDT layers for collaborative editing. **[Loro](https://github.com/loro-dev/loro) is Rust-native with Replayable Event Graph semantics — version-DAG and frontiers as first-class primitives, Fugue text CRDT — and aligns most naturally with mere's event-DAG framing. Preferred candidate over Automerge/Yjs as of 2026-05-10.** Cons: not a substrate, not a sync protocol — they are document data structures.

→ Best fit for: **specific projections** (collaborative graph annotations, co-edited views) when those features land. Not the substrate.

**Practical decision (revised 2026-05-10):** start with iroh-docs (already RBSR). **Willow adoption is now a live evaluation, not a deferral** — the prior deferral was based on a now-disproven WILLIAM3 cost. See §14 (graph-cluster namespaces) for why Willow's data model is especially attractive under the emerging design direction. p2panda and NextGraph remain deferred until concrete need at the engram boundary. Treat all of these as candidate projections of the event DAG, not as identity-defining commitments.

---

## 6. Privacy transport: Veilid as a per-moot policy

Decision: **iroh is the default transport; Veilid is available as an optional per-moot transport policy** for privacy-sensitive communities.

The pattern:

- A moot's constitution declares a transport policy:
  - `transport: iroh` (default; ergonomic)
  - `transport: veilid` (privacy-required; members must run Veilid-capable clients)
  - `transport: any` (members choose; mixed)
- The mere-transport crate feature-gates the Veilid backend: `mere-transport[default] = ["iroh"]; mere-transport[full] = ["iroh", "veilid"]`.
- Joining a Veilid-required moot requires a Veilid-capable build; mere can detect and prompt the user to use a privacy-capable build, or refuse the moot.
- Bridge mod authors (Matrix, Nostr, etc.) need not implement Veilid; bridges are an inherent privacy compromise, and Veilid moots refuse bridge participation by default.

Why optional, not default: Veilid's privacy comes at an ergonomics cost (DHT lookup latency, smaller peer pool). Default-iroh keeps the typical mere experience snappy; opt-in Veilid serves the communities for whom membership-graph leakage is unacceptable.

This satisfies Mark's "privacy as an option, possibly enabled by a mod" framing. The "mod" is the moot's transport-policy declaration; member clients enforce it.

---

## 7. Multi-protocol moot hosting; bridges only for non-hostable systems

**Revised again 2026-05-07 (still later):** §7's framing — even after the Pattern A / Pattern B restoration below — has been **further reframed by the [moot tiers and voluntary hosting brief](2026-05-07_moot_tiers_and_voluntary_hosting_brief.md)**. The newer brief reframes moots as *graph views that link to and store, not translate*: foreign protocols stay themselves, and `mooting-*` adapters become **thin protocol clients** that help members reach foreign resources rather than bidirectional native moot hosts. The newer brief also introduces the four-tier scale (orrery → moot → moothold → demesne) and renames *moothold* to mean "federation of moots" specifically (t3), with *demesne* reserved for t4 (sovereign coalition of mootholds). Read the moot-tiers brief for the authoritative shape.

The Pattern A / Pattern B framing below remains useful as a coarse shape: thin-client adapters fit Pattern A's slot; outbound publishing fits Pattern B. The earlier "bridges-only" framing in this section collapsed the two patterns:

- **Pattern A — native hosting (`mooting-*` adapter crates).** The foreign p2p protocol can express moot semantics directly. A moot is hosted *on* that protocol; members of the moot speak the protocol under the hood; moothold's verbs (membership, governance, tessera, engram flora, capability scopes) map onto the protocol's primitives. **Bidirectional.** This is the right pattern for Matrix rooms, Nostr communities (NIP-72-shaped), ATProto feeds, ActivityPub Group actors, P2P Matrix, and other systems whose native primitives can carry moot semantics.

- **Pattern B — outbound bridge (`mere-bridge-*` crates).** The foreign system can't host moot semantics (or mere only wants to publish outward). One-way mapping from MereEvents into the foreign protocol's vocabulary. Useful for one-shot Mastodon posts, IRC channel echoes, ATProto firehose announcements. **Outbound only.**

Both patterns coexist. The choice is per-protocol, made when the adapter is designed.

**Why the multi-backend pattern is right:** it follows the same shape `nematic` uses for smolweb (one engine; many protocol-specific sibling crates after `middlenet-{gemini, gopher, finger, markdown, feed, spartan, nex, titan, scroll, guppy, misfin, …}`). One sibling adapter crate per protocol; no protocol leaks into the core; adding a new p2p protocol is one new sibling crate, not a modification.

The earlier "bridges-only" framing was reacting against a *bad* version of internal protocol mods — one with leaky abstraction and N×M coupling. The right shape is a clean trait surface in `mooting` plus per-protocol adapter siblings, the same shape `middlenet-adapters` uses to dispatch over `middlenet-{gemini, gopher, …}`.

**The patterns side-by-side:**

```text
   Pattern A (native hosting):

      moot                              moot operations
      operations  ──►  mooting  ──►  ──►   in foreign
                       (dispatcher)        protocol's
                            │              native form
       ┌────────────────────┼────────────────────────┐
       ▼                    ▼                        ▼
   mooting-mere       mooting-matrix            mooting-nostr  …
   (MereEvent DAG     (Matrix rooms              (NIP-72 events
    over iroh)         + state events)            over relays)


   Pattern B (outbound bridge):

      mere-native MereEvent stream
                  │
                  ▼
        mere-bridge-{matrix, nostr, ...}
                  │ map MereEvent → foreign event (one-way)
                  ▼
        foreign protocol (publish-only)
```

**What this changes about `mooting`:** the gerund-crate IS a protocol-selection layer, but the dispatch is over **multiple native moot-hosting protocols**, not over leaky bridges. The trait surface (`MootProtocol` or similar) is small and protocol-agnostic; per-protocol mappings live in sibling adapter crates. moothold orchestrates lifecycle and federation across whichever backends are configured.

**What this changes about `mere-bridge-*`:** outbound-only bridges still earn their separate-crate status. They are *not* the same as `mooting-*` adapters. A future `mooting-matrix` (Pattern A) is bidirectional Matrix-as-moot-host; a `mere-bridge-matrix` (Pattern B) is one-way "publish to a Matrix room from outside the moot model."

**Specific dispositions (revised):**

- **Matrix.** `mooting-matrix` (Pattern A) — Matrix rooms as moot hosts. Membership = room membership; governance = state events; rooms federate via Matrix's homeserver mesh. Element Call (WebRTC) is a distinct concern (calls layer, not moot hosting); a separate `mere-bridge-matrix-call` crate handles that.
- **Nostr.** `mooting-nostr` (Pattern A) — NIP-72-shaped community events. Membership = follow + invite; engrams = signed Nostr events. The 5/5 plan's NIP-05 discovery plumbing in mere-identity survives independently as part of the contact-discovery surface.
- **ATProto.** `mooting-atproto` (Pattern A) — bsky-shaped community feeds. May also have a `mere-bridge-atproto` for outbound publishing.
- **ActivityPub.** `mooting-activitypub` (Pattern A) — fediverse Group actors. May also have a `mere-bridge-activitypub`.
- **IRC.** `mere-bridge-irc` (Pattern B) — IRC's wire format can't fully express tessera or engrams; outbound bridge is the right shape.

---

## 8. Identity system: from cryptographic base to a real system

The 5/5 plan §3 specifies the identity vault. That work stands. But the identity *system* — device enrollment, revocation, recovery, contact discovery, key rotation, multi-device sync, pseudonymous personas, capability delegation, spam resistance — is still substantially open.

These are first-pass intuitions captured 2026-05-07. Each has trade-offs worth surfacing.

### 8.1 Device enrollment

**Intuition:** QR code or shared passphrase.

**Reaction:** standard pattern; both work.

- QR code encodes a one-time enrollment token (signed by the master). New device scans, presents the token, master device approves.
- Shared passphrase: master device generates a short passphrase shown on screen; new device types it in. Passphrase is one-time, expires quickly.

**Open sub-questions:** what if the master device is offline / lost? Need a fallback for "I'm setting up my second laptop while traveling." Either (a) share an enrollment token from a backup recovery shard, or (b) use the recovery code (see §8.3) to bootstrap a new master, then enroll the new device against the new master.

**Constraint:** the new device gets its own derived sub-keypair. Compromising one device should not compromise the master.

### 8.2 Device revocation

**Intuition:** a signed-in device can revoke another device's token with an additional credential.

**Reaction:** sound; the additional credential is the master passphrase or a hardware-backed authentication.

**Sub-question — capability tiering:** which devices can revoke which?

- *Master tier*: the device with the master passphrase can revoke any device.
- *High tier*: a long-lived primary device (laptop) can revoke peer-tier devices but not the master.
- *Peer tier*: short-lived devices (phone, tablet) can revoke only themselves.
- *Kiosk tier*: read-only access; cannot revoke anything.

This maps onto the existing `UnlockTier` enum in `mere-identity::vault`.

**Wire shape:** revocation is a signed `MereEvent` with `event_type: DeviceRevocation`, target_ref the revoked device's key, signed by an authorized device. Other devices observe the event and stop accepting signatures from the revoked key for events with timestamp > revocation timestamp.

### 8.3 Profile recovery

**Intuition:** backup code, or another source of truth, or just accept the loss.

**Reaction:** all three are valid product choices; pick one default and offer the others as opt-in.

- **Backup code (single secret).** User writes down a recovery phrase at first run. Loses passphrase → enters recovery phrase → re-derives master. Mainstream UX. Risk: phrase stored insecurely is a single point of failure.
- **Shamir's Secret Sharing (recovery shards).** Master split into N shards; recovery requires K. User stores shards across trusted locations / friends. Friends-and-family social recovery. More resilient; more complex.
- **Accept the loss.** Cypherpunk-aligned. No recovery; lose the passphrase, lose the identity. Some users prefer this.

**Recommendation:** default to backup code with optional shard upgrade. Explicitly document that no third party can recover for you.

### 8.4 Social proof / contact discovery

**Intuition:** associate with other credentials (phone number, Signal-style); not publicly presented.

**Reaction:** Signal-style works but inherits Signal's metadata trade-offs (PII in phone numbers, SIM-swap exposure, central directory of who-knows-whom).

**Layered approach:**

- **Out-of-band exchange (default).** Alice and Bob meet in person; phone NFC-tap or QR-scan exchanges master pubkeys. Most private; requires physical proximity or a trusted side channel.
- **WebFinger / NIP-05 discovery (existing 5/5 plan §4).** Alice publishes her master pubkey at `alice@example.com`. Bob looks her up via WebFinger. Trust derives from DNS / TLS proof. Already specified.
- **Phone-number directory (opt-in).** Mere operates a directory mapping phone numbers to master pubkeys. Phone numbers stay private (hashed; lookups via private set intersection like Signal's). Trade-off: requires running an infrastructure piece. Mere can bridge to Signal's existing directory if Signal-API access is available.

**Recommendation:** layer all three. Out-of-band is the trust ceiling; WebFinger is the convenience tier; phone-directory is the network-effect tier (opt-in only).

### 8.5 Key rotation

**Intuition:** regenerate but append prior key for history; or make a log of prior keys.

**Reaction:** this is "key history" / "key chain." Sound design.

**Mechanics:**

- Each rotation is a `MereEvent` with `event_type: MasterKeyRotation`, signed by the *prior* master. Carries the new master pubkey.
- Verification: walk the rotation chain from key K_n back to K_0. Each link is signed by the prior key.
- Old data signed by old keys remains verifiable (the chain proves continuity).
- New events use the new key.

**Edge case:** master compromise. Attacker has K_n; can sign rotation to K_n+1 (their key) and lock the legitimate user out. Defense: rotation events also require a recovery-shard quorum signature, or a hardware-key co-signature, depending on the user's chosen recovery setup. This couples rotation to recovery (§8.3) — the same trust path that recovers also rotates.

### 8.6 Multi-device sync

**Intuition:** "hell yeah."

**Reaction:** load-bearing, and tractable on the event-DAG model.

**Mechanics:**

- Each device has its own derived sub-key (`mere-identity::derive_keypair(device_id)`).
- All devices subscribe to the user's *Personal* MereSpace (`SpaceId::Personal(master_pubkey)`).
- Events authored on any device flow into the personal event DAG; other devices replicate via iroh-docs / iroh-gossip.
- Eidetic state, graph state, settings are all materialized projections of the personal event DAG.
- Conflict resolution: hybrid Lamport timestamps + per-event-type merge rules (LWW for settings; CRDT-merge for graph annotations; etc.).

This is essentially "the user is a one-member moot" — same protocol, scoped to the personal namespace.

### 8.7 Pseudonymous personas

**Intuition:** id chain — new persona links to a parent; user can present as a prior link (mask with historical id) or delete the chain to start fresh. Concern: cheap personas should be devalued to discourage flooding.

**Reaction:** the id-chain idea is novel and worth designing carefully. The chain alone does *not* solve cheap-persona spam. Reputation continuity does.

**Mechanics worth considering:**

- **Persona keypair derivation:** persona keys derived from master + persona-id. `derive_keypair(blake3("persona" || persona_id))`. Each persona has its own Ed25519 identity.
- **Public identity is the leaf** (current persona). Prior chain links remain in local memory but are not advertised externally.
- **Masking with a historical id:** subtle. If the historical id was *publicly used* before, observers can correlate. Masking only protects forward, not backward. The chain is best understood as "graceful persona evolution," not "anonymity rotation."
- **Chain deletion:** local-only. Past personas' posts on remote moots remain there; deletion only removes the user's local awareness of the chain.

**Cheap-persona spam:** the chain *helps* if reputation accumulates against the chain root, not the leaf:

- Tessera tokens are issued to the *master* identity (chain root) as the user participates in moots.
- A new persona on the same chain inherits a *fraction* of the master's tessera (depreciation curve).
- Brand-new personas on a brand-new chain start from zero tessera.
- Spammers can spin up fresh chains, but each persona has zero tessera at issuance and weak reputation thereafter; moot-level rate-limiting based on tessera throws the spam out.

**Trade-off:** strong privacy (delete the chain, start fresh) costs reputation continuity (zero tessera from the new chain). This is the right shape — reputation is the cost of continuity, anonymity is the cost of starting over.

### 8.8 Capability delegation at scale

**Intuition:** intrigued by Willow's meadowcap.

**Reaction (revised 2026-05-11):** this is no longer one flat "meadowcap vs Keyhive" choice. The capability surface splits into three layers:

1. **Structural namespace capabilities** — read/write/delegate authority over event areas: namespace, subspace/author, path/cluster, logical timestamp range.
2. **Moot policy authorization** — "is this action allowed under current moot facts?" Tessera thresholds, pin quotas, heartbeat freshness, roles, event types, bridge-publish windows.
3. **Live group/key state** — membership, device enrollment, key rotation, encrypted sync, pull access, and post-compromise recovery.

Treating those as one decision hides the real architecture. Meadowcap, Biscuit, UCAN, and Keyhive cover different parts of the stack.

**Option A — meadowcap via willow-rs (structural namespace layer).** If mere adopts Willow's data model (see §5 and §14), willow-rs's `meadowcap` crate provides the structural capability layer natively, fully PD-generic over BLAKE3. Zero design re-derivation; we get path-prefix delegation, logical time-interval bounds, recursive delegation, and signed credentials directly. Lowest implementation cost.

Pros: purpose-built for Willow entries; maps cleanly to graph-cluster-derived paths; simple deterministic verification for "does this cap authorize this event area?"; good fit for `mere-namespace`.

Cons: not an expressive policy language; does not naturally encode tessera thresholds, host quotas, moderation state, heartbeat freshness, or bridge-publish rules; revocation is expiry/replacement/external state rather than a rich live model; read access still depends on replication cooperation unless paired with encryption. Meadowcap timestamp bounds constrain the logical timestamp claimed by an Entry, not the wall-clock moment at which a hostile peer creates it.

**Option B — meadowcap-shaped, mere-native (structural fallback).** If we do not adopt the Willow data model, we can still implement caps in the meadowcap shape (signed credentials over `(subspace_set, path_prefix, time_interval, mode)` with recursive delegation). This keeps the storage-scope semantics without importing Willow's full 3D data model. It remains valid as a fallback, but it means re-deriving correctness that meadowcap already gives us.

**Option C — Biscuit / `biscuit-rust` (moot policy layer).** [Biscuit](https://doc.biscuitsec.org/reference/specifications) is a bearer authorization token with offline attenuation and Datalog-style checks. This is the missing candidate for moothold policy: tessera gates, pin/hosting quotas, heartbeat freshness, event-type constraints, scoped bridge publishing, moderator powers, "can this contributor vouch for this newcomer?", and "can this host pin this cluster under current moot rules?"

Pros: expressive enough for governance rules; verifiable by any peer with the root public key plus the current authorizer facts; holders can attenuate tokens offline; Rust implementation exists in [`biscuit-rust`](https://github.com/eclipse-biscuit/biscuit-rust).

Cons: not a storage model, sync model, encryption layer, or group-membership CRDT; revocation still needs external state; token theft matters unless Mere binds use to the request signer/device key in the authorizer facts; the predicate vocabulary becomes a protocol surface and must stay small, stable, and auditable. Biscuit can encode path/resource scopes, but it does not inherently understand Willow/Mere Entry semantics.

**Option D — Keyhive (live group/key-state layer).** [Keyhive](https://www.inkandswitch.com/project/keyhive/) is not just another capability-token format. It is a lower-level local-first access system: convergent capabilities, group-management CRDTs, device/person/document groups, encrypted-at-rest data, and BeeKEM/Beelay work for group keys and auth-enabled sync. This is the serious candidate for dynamic membership, device enrollment, key rotation, pull access, and encrypted local-first sharing.

Pros: targets exactly the class of local-first access problems that static certificate capabilities punt on; handles concurrent/offline membership updates as state, not as a token-only chain; gives Mere a plausible path for future encrypted group sync and post-compromise recovery.

Cons: pre-alpha, unaudited, and API-unstable as of the current upstream release; shaped around Automerge documents and Beelay, so Mere would need a real translation layer into event-DAG authors, graph-cluster namespaces, and iroh/Willow projections; adopting it too early risks letting its storage/sync assumptions leak into Mere's core. It also does not replace moot policy — it can answer who belongs to a group and who should receive keys, not whether current tessera/hosting/governance facts permit a specific moot action.

**UCAN disposition.** UCAN remains useful as an interoperability and delegation envelope, especially for portable session/capsule sharing and external agents. It is less compelling as the internal authorization brain: too coarse for moothold policy, not a namespace model, and not a live group/key-state system.

**Layered shape:**

- Structural caps are signed credentials over a scope (path-prefix / graph-cluster prefix), a mode (read / write / delegate), a logical time interval, and a delegation chain.
- Policy tokens evaluate a requested action against current moot facts: tessera, roles, quotas, heartbeats, event type, bridge target, and local revocation lists.
- Group/key state controls who is in which group/device set, who should receive encrypted material, and how future access changes propagate under offline concurrency.

**Apply to mere:**

- Tessera issuance is a governance event plus policy fact; vouching can be represented as delegated policy authority with abuse flowing back to the voucher's reputation.
- Moot moderator status is a capability over the moot's namespace.
- "Share this graph view with Alice for 30 days" is a structural read cap over `/personal/graphs/<view-id>/...` or a graph-cluster prefix under §14; if private bytes are involved, a group/key layer must also deliver the relevant decryption material.
- Bridge-publish authorization is better as a policy token constrained by `event_type`, destination, wall-clock expiry, and current governance facts.
- Pinning/hosting authority is structural scope plus policy facts: which cluster may be pinned, by whom, under which quota, with what heartbeat and handoff expectations.

**Live decision.** The v1 default should be meadowcap for structural namespace caps (Option A if Willow lands, Option B if Mere stays native), with Biscuit evaluated as the moothold policy-token layer. Keyhive should move to a focused lower-layer eval for dynamic groups, device/key management, encrypted sync, and pull-access defense-in-depth. The eval is therefore three small spikes, not one winner-take-all comparison: (1) meadowcap over graph-cluster paths, (2) Biscuit policy proof for tessera-gated hosting, (3) Keyhive translation proof against Mere event-DAG authors and group membership.

### 8.9 Spam resistance

**Intuition:** reputation matters.

**Reaction:** correct, and tessera is the existing lexicon term for this. The id-chain section above already couples it.

**Concrete spam mitigations to layer:**

- **Moot-level tessera thresholds.** A moot can set a minimum tessera level for posting; new chains can lurk but not post until they earn (or are vouched for).
- **Vouching as capability delegation.** A high-tessera member can vouch for a newcomer, delegating a fraction of their tessera as an entry boost. Vouches are signed events; abuse loops back to the voucher's reputation.
- **Per-moot rate limits.** Membership comes with a posting rate; cheap personas hit the limit fast.
- **Velocity-based detection.** Sudden surge of posts from new personas in the same chain → triggers moderator review.

None of this is unique to mere; the design is well-trodden in modern federated systems (Matrix's reputation pluggable rule sets, ActivityPub's relay policies). Mere benefits from designing this *into* the protocol rather than bolting it on.

---

## 9. What changes for the code

Concrete moves implied by this brief, in rough order:

1. **mere-identity: BLAKE3 derivation.** Add `derive_keypair_blake3(salt)` alongside existing `derive_keypair`. New code uses BLAKE3; the BLAKE2b path stays alive only as long as Cable wire-compat matters — and per this brief, it doesn't.
2. **murm / murmuring: drop Cable wire format.** Replace with the Mere event grammar (§4) over CBOR over iroh streams. Rename `murmurings` connotation: still bilateral chat semantics, but the wire is now MereEvent-shaped, not Cable.
3. **moothold / mooting: multi-protocol moot hosting via sibling adapter crates.** mooting becomes the protocol-core (a thin trait surface + dispatcher) for moot semantics; per-protocol concrete adapters live as sibling crates (`mooting-mere`, `mooting-matrix`, `mooting-nostr`, `mooting-atproto`, `mooting-activitypub`, …) following the `middlenet-{gemini, gopher, …}` precedent. Foreign systems that can't host the moot abstraction natively get separate outbound `mere-bridge-*` crates above moothold.
4. **New bridge crates** (separate workspace members, opt-in): `mere-bridge-matrix`, `mere-bridge-nostr`, `mere-bridge-activitypub`, `mere-bridge-atproto`, `mere-bridge-irc`. Each maps a subset of MereEvent types into the foreign vocabulary. None of these need to land for 0.0.x; the architecture clears space for them.
5. **mere-transport: Veilid feature flag.** Behind `mere-transport[veilid]`; default build is iroh-only. Moots can declare transport policy in their constitution.
6. **Event DAG core (new crate or murmuring-internal module):** `MereEvent`, `MereSpace`, signing / verification / canonical encoding, in-memory event store, hash-chained DAG walk, basic sync state machine over iroh-docs. This is the new load-bearing module.
7. **eidetic stays a leaf substrate.** No schema imposed at the storage layer. Higher-level subsystems (event store, AWAL, STM/LTM indexes) build on top.

---

## 10. What this brief does not decide

Open questions, deferred:

- **Concrete sync-backend choice for cross-moot federation.** iroh-docs is the baseline (and already does RBSR — this is no longer a discriminator). Whether Willow's data model + meadowcap (via `mere-namespace`) earn adoption, or p2panda earns a slot for engram publishing, is now a near-term decision rather than a deferred one — see §5 (revised) and §14.
- **Capability stack proof.** See §8.8. Live decision is now layered, not "meadowcap vs Keyhive": structural namespace caps (meadowcap / meadowcap-shaped), moothold policy tokens (Biscuit candidate), and live group/key state (Keyhive candidate) each need a focused proof before `mere-namespace` and `moothold` lock their surfaces.
- **Recovery default.** Backup-code vs Shamir-shards vs cypherpunk-loss. Probably product-driven, not architecturally forced.
- **WebRTC / calls strategy.** Element Call as a Matrix-bridge service, LiveKit over iroh, or something else. Independent of the substrate decisions here.
- **The Distillery's first concrete shape.** Where it lives (mere-side vs eidetic-side vs new crate), what its first transform pipelines are. Out of scope for this brief; covered by the inherited STM/LTM/Engrams plan.
- **Persona-id-chain wire shape.** The §8.7 mechanics are sketches; concrete chain encoding, depreciation curves, and tessera inheritance need a separate design pass before implementation.

---

## 11. What this updates in the 2026-05-05 plan

The 5/5 plan's surviving sections:

- §1 *Architecture Overview* — layer cake stands.
- §2 *Iroh-as-Toolkit Layering* — entirely.
- §3 *Identity Vault* — entirely. This brief extends it (§8) with the system-level concerns; the cryptographic base is unchanged.
- §4 *Identity Discovery, Proof, Self-Host-with-Fallback* — survives. NIP-05 plumbing in §4.3 / §4.6 stays as a *discovery* surface; Nostr internal participation is what drops.

The 5/5 plan's superseded sections (this brief takes precedence):

- §5 *Protocol Mods and Primitive Moot Nodes* — the *concept* survives, refined: mods become **sibling adapter crates** (`mooting-*`) for protocols that can host moots natively (Pattern A), plus separate **outbound bridge crates** (`mere-bridge-*`) for systems that cannot (Pattern B). §5.1.1's puppet/portal pinning patterns map onto Pattern B. §5.5's protocol-mod table becomes a per-protocol adapter table, with each row tagged Pattern A or Pattern B.
- §6 *Cross-Cutting Phase Sequencing* — Phase 3's "Moothold protocol mods" reframes as "Moothold core (Mere-native moot semantics) + first `mooting-*` adapter sibling (likely `mooting-mere` as the canonical reference) + first `mere-bridge-*` for non-hostable systems."

The 5/5 plan's open questions this brief closes:

- Cable wire-format vs replacement → **drop Cable, Mere-native event DAG.**
- Sync layer choice → **iroh-docs baseline, others as projections.**
- Schema locality → **engram boundary, not write boundary.**

The 5/5 plan's open questions still open:

- Per-protocol bridge rollout order.
- WebRTC / calls.
- Recovery default.

---

## 12. First milestone

The smallest prototype that demonstrates the new substrate end-to-end:

```text
1. Create a MereSpace (personal or moot).
2. Add peers by capability invite.
3. Send signed text events (ChatMessage event_type).
4. Add signed graph events (GraphAddNode, GraphAddEdge).
5. Store large payloads as BLAKE3-addressed iroh-blobs.
6. Sync events over iroh-docs / iroh-gossip.
7. Materialize the event log into a visible graph.
8. Verify on a second device joining the same MereSpace.
```

At that point: Cable is gone, BLAKE3 is central, the Mere-native event DAG is the protocol identity, sync is iroh-docs, and the architecture has cleared space for everything else (capability layer, engrams, bridges, Veilid privacy mode) without committing to any specific external protocol.

---

## 13. Hash agility for future migrations

**Added 2026-05-10.** CIDv1 multihash discipline is the runway for any future hash-function transition (BLAKE4, post-quantum, or otherwise). Every digest field in the system MUST use a multihash-aware type — never raw `[u8; 32]` — so that the hash function identifier travels with the digest. Eidetic's `schema::Hash` already does this; the rule generalizes across the workspace.

**What hash agility costs at adoption time:** ~3 bytes per digest (multihash function code + length prefix). Native BLAKE3 storage doesn't change.

**What hash agility buys at migration time:** the option to add a new hash function as a parallel namespace without flag-day rotation. New writes carry the new prefix; old reads still work; capability tokens issued under one hash remain valid for data addressed under that hash; the event DAG is naturally mixed-hash with no protocol-version bump required.

**Migration recipe** (for a hypothetical BLAKE4 day):

1. iroh-blobs adds the new hash function (or we fork). External gate.
2. Add the new variant to `eidetic::schema::Hash`. CIDv1 multihash handles the prefix.
3. Add a workspace flag `default_payload_hash = blake4` for new writes. Old writes stay readable forever.
4. Capability layer: new caps under the new hash for new data. Old caps remain valid for old data.
5. Event DAG: mixed-hash; no flag day required.
6. Identity derivation stays on whatever hash `mere-identity` bootstrapped with — independent decision from payload-hash agility, since identity keys are an immutable historical fact.

**Audit obligation.** Every digest field across `mere-identity`, `murmuring`, the future event-DAG core, the cap layer, `mere-namespace`, and any other crate that touches digests MUST use a multihash-aware digest type. `eidetic::schema::Hash` is the obvious shared type once it is upgraded; until then it should not be treated as satisfying this rule. This is the only structural work the migration story requires *today* — and it's free if done as discipline now. p2panda's BLAKE2b → BLAKE3 migration (which required a flag-day rotation because they didn't have multihash discipline) is the precedent worth not repeating.

**2026-05-11 implementation gap:** `eidetic::schema::Hash` currently serializes raw BLAKE3 `[u8; 32]` bytes and displays as `blake3:<hex>`. That is enough for current BLAKE3 integrity checks, but it is **not** the multihash-aware type this section requires. Before event hashes, adapter manifests, capability scopes, and engram receipts proliferate, either migrate `Hash` to a CIDv1/multihash-aware representation or explicitly introduce a second `ContentDigest` type and reserve raw `Hash` for legacy/local-only BLAKE3 values. Leaving this ambiguous will make the later hash-agility promise false.

---

## 14. Graph-cluster-derived namespaces (see separate brief)

**Added 2026-05-10.** The architecturally novel direction emerging from the §5 Willow re-evaluation: **namespaces should be derived from the graph's natural community structure, not admin-imposed paths.** A node's path in the meadowcap-style namespace hierarchy is its position in the graph's cluster decomposition (Louvain / Leiden / label-propagation / infomap), not a folder path chosen by anyone. Capability scopes follow semantically coherent sub-graphs; sharing a "cluster" gives a recipient a meaningful neighborhood rather than an arbitrary directory.

This is substantial enough to warrant its own design brief. See [`2026-05-10_graph_cluster_namespaces_brief.md`](2026-05-10_graph_cluster_namespaces_brief.md).

---

## Findings

### 2026-05-07 — substrate decisions consolidated

Conversation review: today's analysis converged on the event-DAG-as-core inversion (rather than picking a sync layer as architectural identity). The shift is structural, not cosmetic — it reframes Willow / p2panda / NextGraph as candidate projections rather than candidate substrates, and it isolates the BLAKE3 unification cost to a clean break with Cable.

Identity system: cryptographic base is solved by the 5/5 plan §3; the system-level concerns (device lifecycle, recovery, contact discovery, key rotation, multi-device sync, personas, spam) are largely open. First-pass intuitions captured in §8 with trade-offs surfaced.

Privacy transport: Veilid as a per-moot transport policy (rather than substrate replacement) preserves iroh's ergonomic default while opening privacy-required communities. Implementation cost: feature-gated `mere-transport[veilid]` build, transport-policy field in moot constitution.

Bridges-only framing was overcorrected; later-day revision (§7) restores the Pattern A / Pattern B distinction. The fix: native moot hosting on foreign p2p protocols (Pattern A) lives in `mooting-*` sibling adapter crates following the `middlenet-{gemini, gopher, …}` shape; outbound publishing to non-hostable systems (Pattern B) lives in `mere-bridge-*` crates. mooting is a multi-backend dispatcher with a thin trait surface, not a leaky abstraction. The original "one verb per crate" instinct still holds — the verb is "host moots over protocol X," and each protocol gets its own adapter crate.

Schema-at-engram-boundary: the architectural insight that resolves the "rdf vs structured vs informal" tension. Local memory stays untyped; engrams crystallize schema at distillation. p2panda's schema discipline finds its real home at the engram layer, not the event DAG.

### 2026-05-10 — Willow integration unblocked; substrate brief reframed

Research dispatch (four parallel agents on iroh-docs sync, iroh-willow status, willow-rs PD-genericity, and broader 2025-26 P2P landscape) inverted §2's premise.

- **WILLIAM3 was not a current cost.** willow-rs is BLAKE3-default (`willow_25::PayloadDigest25 = blake3::Hash` newtype), and every crate (`data-model`, `meadowcap`, `wgps`, `store_simple_sled`, `sideload`) is fully PD-generic. The eventual WILLIAM3 swap is for partial-payload verification — a guarantee mere does not need (iroh-blobs' BAO already provides logarithmic-proof verifiable streaming under BLAKE3).
- **iroh-docs already does RBSR.** The "no RBSR-style sync efficiency at large scale" concern in the original §5 iroh-docs entry was wrong — it has done RBSR per [Meyer 2022](https://arxiv.org/abs/2212.13567) all along, verified at v0.99.0 (2026-05-08). Willow adoption is therefore not justified by sync efficiency; it must be justified by the data model + meadowcap.
- **iroh-willow is dormant.** Pre-alpha (0.0.1, never released), last meaningful commit 2025-10-27, open correctness bug in capability delegation ([issue #5](https://github.com/n0-computer/iroh-willow/issues/5)). We'd build our own willow-rs-on-iroh integration in `mere-namespace` rather than depend on it.
- **Capability layer is now a capability stack.** §8.8 now separates structural namespace caps (meadowcap via willow-rs, or meadowcap-shaped Mere-native), moothold policy authorization (Biscuit / `biscuit-rust` candidate for tessera, quotas, roles, heartbeats, bridge-publish rules), and live group/key state (Keyhive for dynamic membership, devices, encrypted sync, pull access). The eval is three proof spikes, not one meadowcap-vs-Keyhive choice.
- **Loro added** as preferred CRDT projection over Yjs/Automerge — Rust-native with version-DAG-and-frontiers as first-class primitives.
- **NextGraph reframed** from "research branch" to "engram-layer interop candidate" — has shipped real users and SPARQL APIs as of FOSDEM 2026.
- **Snapchain (Farcaster) abandoned CRDT for BFT in April 2025** — negative signal worth examining before locking eventual-consistency assumptions for moot scale. Mere's moot-scale topology likely doesn't hit Farcaster's wall, but the failure mode deserves understanding.
- **iroh transport bet validated.** noq (n0's QUIC, March 2026) replacing quinn under iroh ≥0.96; iroh 1.0 RC in progress; Holochain 0.7 adopted iroh for hole-punching — first significant external adopter.
- **Multihash discipline (new §13)** codifies hash-function tagging across all digest fields. Runway for any future hash transition without flag-day rotation.
- **Graph-cluster-as-namespace direction (new §14)** emerges as the architecturally novel piece — split out into a separate design brief.

---

## Progress

### 2026-05-07

- Brief drafted to consolidate today's substrate decisions.
- Updates §11 of this brief enumerate which 5/5 plan sections survive vs supersede.
- Code-side moves enumerated in §9; not yet started. First implementation step is the BLAKE3 derivation in `mere-identity` (§9 item 1).
- Open questions enumerated in §10 for follow-up.
- DOC_README index updated.

### 2026-05-10

- §2 rewritten: WILLIAM3 cost framing removed; willow-rs is BLAKE3-default and PD-generic.
- §5 entries revised: iroh-docs (RBSR confirmed), Willow (full re-evaluation), NextGraph (matured), Loro (added). Practical-decision footer revised.
- §8.8 initially reframed as a three-option capability-layer decision (meadowcap-via-willow-rs, meadowcap-shaped-native, Keyhive); this intermediate framing is superseded by the 2026-05-11 stack split below.
- §10 open questions updated: WILLIAM3 disposition resolved; the initial meadowcap-vs-Keyhive question is superseded by the 2026-05-11 capability-stack proof.
- §13 added: hash agility / multihash discipline.
- §14 added: graph-cluster-derived namespaces (pointer to separate design brief).
- 2026-05-10 Findings entry consolidates research outcomes.
- DOC_README index updated alongside the new namespace brief.

### 2026-05-11

- §8.8 corrected from a flat meadowcap-vs-Keyhive comparison into a layered capability stack: structural namespace caps, moothold policy authorization, and live group/key state.
- Biscuit / `biscuit-rust` added as the missing moothold policy-token candidate for tessera-gated hosting, quotas, roles, heartbeat freshness, and event-type/bridge-publish constraints.
- §10 open question updated to require three proof spikes: meadowcap over graph-cluster paths, Biscuit policy proof for tessera-gated hosting, Keyhive translation proof for Mere event-DAG authors and group membership.
