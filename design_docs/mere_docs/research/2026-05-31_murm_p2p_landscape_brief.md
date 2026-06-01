# Murm / Mere P2P: state, trajectory, and the iroh-anchored landscape

**Date**: 2026-05-31
**Status**: Research brief / orientation survey. Grounds the p2p design corpus against the live code and against the current (May 2026) external p2p landscape. No code change proposed; this frames state, contradictions, and adopt-vs-build calls.
**Scope**: The whole p2p program centred on *murm* (bilateral comms) and reaching across `transport` (iroh), `moothold`/`mooting` (federation), the event-DAG substrate, and the identity vault. Treats iroh as the foundational piece and asks where the rest of the stack should come from.
**Related**:

- [`../implementation_strategy/2026-05-05_protocol_architecture_plan.md`](../implementation_strategy/2026-05-05_protocol_architecture_plan.md) — iroh-as-toolkit, identity vault, self-host-with-fallback, primitive moot nodes. Surviving spine.
- [`../implementation_strategy/2026-05-07_event_dag_substrate_brief.md`](../implementation_strategy/2026-05-07_event_dag_substrate_brief.md) — the substrate pivot (drop Cable wire, BLAKE3, MereEvent DAG, sync-as-projection). This brief refreshes its external facts and flags its unexecuted state.
- [`../implementation_strategy/2026-05-07_moot_tiers_and_voluntary_hosting_brief.md`](../implementation_strategy/2026-05-07_moot_tiers_and_voluntary_hosting_brief.md) — orrery/moot/moothold/demesne tiers, voluntary hosting, tessera, cheesecloth pinning.
- [`../implementation_strategy/2026-05-10_graph_cluster_namespaces_brief.md`](../implementation_strategy/2026-05-10_graph_cluster_namespaces_brief.md) — graph-cluster-derived namespaces (the novel direction).
- [`../../murm_docs/technical_architecture/MURM_AS_BILATERAL.md`](../../murm_docs/technical_architecture/MURM_AS_BILATERAL.md) — murm boundary doc (predates the substrate pivot; see §5 contradictions).
- [`../technical_architecture/2026-05-26_component_fit_map.md`](../technical_architecture/2026-05-26_component_fit_map.md) — confirms the p2p tier is dormant relative to the running app.
- [`../../2026-05-04_lexicon_brief.md`](../../2026-05-04_lexicon_brief.md) — *murm*/*murmur*/*moot*/*tessera*/*kith*/*kin* vocabulary.

---

## 0. What "murmur" denotes

In the lexicon, `murm` is the bilateral-comms supercrate, `murmuring` its protocol-core, and *murmur* the user-facing word for a one-to-one or small-group message. (The former community-layer name *Murmuration* was retired to `moothold`/*moot* over a trademark wall.) "Murmur" names the bilateral half, but the question this brief answers spans the whole p2p arc: `murm` plus `transport` (iroh), `moothold`/`mooting` (federation), the event-DAG substrate, and the identity vault.

## 1. Where we are (code reality, verified against the tree)

The governing fact: the design corpus describes a federation; the code ships bilateral chat over a real pipe. Per the [component fit-map](../technical_architecture/2026-05-26_component_fit_map.md), the entire p2p tier is dormant relative to the running app, which today exercises only the graph/forme/inker/nematic slice.

| Crate (path) | State | What is real |
|---|---|---|
| `transport` (`crates/murm/transport`) | **Built, tested** | `IrohTransport` on iroh 0.98: QUIC plus per-ALPN demux via iroh `Router`, with `iroh-blobs` and `iroh-gossip` served off the same endpoint. Byte round-trip, p2p blob fetch, and gossip-topic exchange all tested over loopback. Discovery is explicit `add_peer` only; n0-DNS deliberately unwired. Bridges the ed25519-dalek 2.x vs iroh-3.0-pre boundary via the raw seed. ~22 tests. |
| `murm` / `murmuring` (`crates/murm`) | **Built, tested. It is Cable.** | BLAKE2b cabal-id derivation, signed posts, encode/decode/sign/verify, snapshot push/pull working over both the memory transport and real iroh. Tamper-in-transit fails verification as intended. Single bi-stream snapshot only; live broadcast is a TODO. Persistent store (fjall+redb+rkyv) scaffolded. ~13 + ~81 tests. |
| `identity` (`crates/persona/identity`, 1,686 LOC) | **Built, tested, more than paper** | Master Ed25519 plus `derive_keypair` (BLAKE2b-256), `InMemoryProvider`. Vault skeleton (`vault.rs`: `IdentityVault<S>`, `Profile`, `IdentitySlot::{Direct,Bootstrap}`, `CredentialLineage`, `UnlockTier`, `IdentityStorage`). Production `PassphraseEncryptedStorage` (Argon2id KEK, ChaCha20-Poly1305). ~13 to 19 tests. |
| `misfin` (`crates/murm/misfin`) | **Built, standalone** | Gemini-style mail client: rustls plus rcgen self-signed client certs, trust-on-first-use, send/receive. No dependency on the rest of the comms stack. |
| `webfinger` (`crates/murm/webfinger`) | Present (de-nostr'd) | Discovery-endpoint scaffolding. |
| `eidetic-iroh-fetcher` (`crates/eidetic`) | Thin companion | Fetches `BlobSource::Iroh` tickets via transport's iroh-blobs. The second iroh consumer. |
| `moothold` / `mooting` (`crates/moot`) | **Stubs (61 LOC)** | Reservation only. The whole tier/federation/tessera/event-DAG/bridge edifice is unwritten. |

What is **not** in the tree (verified: `MereEvent` and `mere-namespace` appear only in README prose, never in a `.rs` file): the Mere-native event DAG, BLAKE3 unification, the moot/tier/tessera machinery, the capability stack, `mooting-*` adapters, `mere-bridge-*` crates, primitive moot nodes, discovery endpoints, back-claim proof. The 05-07 pivot onward is design.

## 2. Where we're going (trajectory, dated)

1. **05-04** Cable migration plus MURM_AS_BILATERAL: keep Cable, bilateral lives in murm, iroh stays in transport, identity is a consumed vault.
2. **05-05** [Protocol architecture plan](../implementation_strategy/2026-05-05_protocol_architecture_plan.md): iroh-as-toolkit (iroh / blobs / gossip / docs, each its own ALPN, all owned by transport); identity vault (per-protocol slots, lineage-aware recovery, unlock tiers); self-host-with-fallback (Mode 1 verso, Mode 1.5 kith/kin bridge, Mode 2 third-party plus JWS); WebFinger plus NIP-05 discovery with back-claim proof; primitive moot nodes (puppet vs portal bridge modes).
3. **05-07** [Event-DAG substrate brief](../implementation_strategy/2026-05-07_event_dag_substrate_brief.md), the pivot: drop the Cable wire, unify on BLAKE3, make a signed Mere-native event DAG the protocol identity, demote sync layers (iroh-docs / Willow / p2panda) to projections, crystallize schema at the engram boundary not at write time, add Veilid as optional per-moot privacy transport, split moot hosting into Pattern A native (`mooting-*`) vs Pattern B outbound bridges (`mere-bridge-*`).
4. **05-07** [Moot tiers brief](../implementation_strategy/2026-05-07_moot_tiers_and_voluntary_hosting_brief.md): orrery (t1) to moot (t2) to moothold (t3) to demesne (t4); voluntary hosting with reputational tessera stakes; cheesecloth pinning; ILL-shaped reciprocity; lapse-and-revive; moots link and store, they do not translate protocols.
5. **05-10/11/30** refinements: the capability *stack* (meadowcap structural / Biscuit policy / Keyhive group-key); [graph-cluster-derived namespaces](../implementation_strategy/2026-05-10_graph_cluster_namespaces_brief.md); hash-agility/multihash discipline; Loro as preferred CRDT projection; NextGraph narrowed to an RDF donor.
6. **05-30** present design energy is elsewhere (two-natured kernel, fields/aether/gyre, serval-as-host). P2P is parked, by deliberate fit-map discipline.

## 3. Best big ideas (worth keeping)

- **Iroh-as-toolkit, not iroh-as-monolith.** One endpoint, many ALPNs, blobs+gossip+streams sharing identity and connection. Already coded. Externally validated (§8).
- **Event-DAG-as-identity, sync-as-projection.** The right inversion: it lets the sync backend change without changing what makes Mere Mere.
- **Schema at the engram boundary.** Local browsing stays untyped and low-friction; typed schema appears only where a recipient must validate or merge. Dissolves the "RDF vs structured vs informal" tension.
- **Graph-cluster-derived namespaces.** Capability scopes follow the graph's own community structure (Louvain/Leiden), not admin folders. The most original idea in the corpus, and nobody in the landscape is doing it. Couples directly to the layout work the kernel already does.
- **Voluntary hosting plus reputational tessera plus cheesecloth pinning plus lapse-and-revive.** A humane federation model that dodges both Mastodon admin-burnout and crypto plutocracy. "Your orrery is your moot" (the moot-of-one) unifies personal and communal under one set of primitives.
- **Capability as a three-layer stack** (structural namespace / policy token / live group-key) rather than one token format. Most projects get this wrong by forcing one mechanism to do all three.
- **Lineage-aware recovery.** The four `CredentialLineage` classes refuse the "one phrase recovers everything" lie and state honestly what losing a device means per credential.

## 4. Pitfalls

- **iroh-docs is the weakest plank.** iroh 1.0 stabilizes the connection layer plus noq; blobs/docs/gossip were split into separate community crates ("net is the new iroh", v0.29), and iroh-docs is a "meta protocol" over blobs+gossip, not on the 1.0 stability train. The sync baseline rests on the least-committed part of the stack. The "sync-as-projection" design already hedges this; keep it, and be ready to own/fork or replace the sync layer.
- **Build-from-scratch MereEvent DAG vs adopt p2panda.** p2panda's modular rewrite (p2panda-core plus p2panda-net) is now, almost ingredient for ingredient, "signed event types over iroh with BLAKE3, Ed25519, CBOR (caps via header extensions / p2panda-auth)." The substrate brief files p2panda under "deferred to the engram boundary." That filing predates how Mere-shaped p2panda became. This is the highest-leverage re-evaluation in the program.
- **Scope versus shipped surface.** The spec keeps growing (vault, discovery, proof, four tiers, capability stack, bridges, personas, spam, recovery, privacy transport) while the built reality is Cable snapshot sync plus a vault skeleton. The fit-map's "correctly dormant" discipline is good; the design docs are still designing a federation before a chat ships.
- **CRDT-at-scale caution from Farcaster.** Snapchain dropped its CRDT hub network (Deltagraph) for Malachite BFT consensus (April 2025) because eventual consistency could not hold global consensus and spam control in one global namespace. Mere's bounded moots are a different topology and should be fine. The lesson to bank: never let a moot become a single global namespace, and treat tessera/spam-resistance as load-bearing, not bolt-on.
- **Three pre-1.0 external bets in the capability stack.** Keyhive is pre-alpha and unaudited, iroh-willow is unreleased, willow25 is 0.1. The "meadowcap-shaped native fallback" hedge is right; do not let these gate shipping.
- **Single-process trust.** The vault openly admits any in-process mod can read any slot. Acceptable for v1, but it must be surfaced to users; the WASM-sandbox path is distant.
- **Version churn.** Code is on iroh 0.98; the world is at 1.0-rc.1. The 1.0 API will move again, and the dalek 2.x / 3.0-pre seam needs a revisit at that bump.

## 5. Contradictions (live, unreconciled in the tree)

- **(A) Cable: drop it, keep it, or ship it?** The substrate brief says drop the Cable wire; the moot-tiers brief and protocol-plan §5.0/§7-Q9 say "Cable stays in murm"; the code still ships actual Cable (BLAKE2b, working). These reconcile conceptually (keep bilateral-in-murm, replace the *wire* with MereEvent), but nothing in code has executed it, and a fresh reader meets three answers.
- **(B) BLAKE2b vs BLAKE3.** The pivot mandates BLAKE3 everywhere (substrate brief §2, §9 item 1). The code is still BLAKE2b: `identity/keypair.rs` derives `BLAKE2b-256(seed || salt)`, murm computes cabal ids with BLAKE2b. `eidetic::schema::Hash` is a raw `[u8;32]` BLAKE3, not the multihash-aware type §13 itself requires. Three hash regimes under a doc titled "unification."
- **(C) The murm/moothold boundary axis flipped, but the crate tree did not follow.** MURM_AS_BILATERAL splits on "1:1 vs many-to-many" and keeps co-op in murm. Protocol-plan §5.0 calls that axis wrong, redraws it as "ad-hoc peer vs durable group," and moves Misfin *out* of murm to moothold+nematic. But `misfin` is a crate living under `crates/murm/`. **Resolved 2026-06-01 (Mark):** smolweb *exchange* protocols (Misfin and kin) stay in murm once developed to parity and fitting murm; the operative axis is *bilateral exchange with a known endpoint*, which includes store-and-forward mail, so `misfin` under `crates/murm/` is correct. Pure fetch/render protocols stay nematic engines. See the [MURM_AS_BILATERAL](../../murm_docs/technical_architecture/MURM_AS_BILATERAL.md) status note.
- **(D) Host-framework drift.** The daemon-split brief and the vault's "Element-style sub-instances" reference gpui `Entity<T>`; the host is now Xilem. The p2p docs need a Xilem re-read before they drive code.
- **(E) Stale "open questions."** The 05-04 plan still lists tokio-vs-async-std and sodiumoxide-vs-dalek as open; the code resolved both (tokio plus ed25519-dalek plus blake2, "no sodiumoxide" stated in `identity/lib.rs`).

## 6. Sidequests

- **Misfin** (Gemini-style mail, TOFU client certs). Real, standalone, shippable. Caveat: design says it belongs in moothold+nematic, code says it lives in murm (contradiction C).
- **eidetic-iroh-fetcher plus iroh-blobs.** The cleanest near-term p2p win: replicate engrams/snapshots/page-caches by BLAKE3 hash, independent of the whole moot edifice. This is cheesecloth-pinning v0.
- **WebFinger / NIP-05 self-host endpoints** (verso-as-server) with back-claim proof (verified vs unverified identity cards). Depends on a verso HTTP listener.
- **Previous-handles chain** (`mere:previous-handle` rel) for verifiable handle migration. Self-contained identity feature.
- **Persona id-chain plus tessera depreciation** as spam resistance. Novel; needs a design pass before code.
- **Veilid** per-moot privacy transport. Real upstream, opt-in, active in 2026.

## 7. Synergies

- **One iroh endpoint, three jobs.** Transport plus blobs plus gossip already share a connection and identity in code. Bilateral chat, presence, and content replication ride one pipe. The printing-press metaphor at the network layer.
- **eidetic plus iroh-blobs.** Content-addressed local memory plus BLAKE3 transfer means engram replication and cheesecloth pinning nearly fall out for free; eidetic-iroh-fetcher is the first wire.
- **Graph-cluster namespaces plus the graph kernel.** The same clustering that lays out the orrery derives the capability scopes. The graph *is* the namespace, reusing cartography/graph-layout rather than inventing a parallel hierarchy.
- **BLAKE3 everywhere plus multihash.** iroh-blobs addressing, engram CIDs, event hashes, and identity derivation become one hash family, with multihash discipline as the migration runway.
- **Tessera plus the two-natured kernel.** Tessera issuance is content-truth (federated, signed); reputation state is derived. It slots into the "content authoritative, experience derived" rule the kernel work just landed.
- **"Moot = graph view that links and stores" reuses the kernel.** A moot node tagged `matrix-room` / `gemini-capsule` / `irc-channel` is a typed node with a protocol tag. The federation layer reuses the existing graph model and nematic's protocol lanes instead of building a second world.

## 8. The current P2P landscape, with iroh as the anchor (verified May 2026)

**iroh itself: the bet is paying off.** 1.0.0-rc.1 shipped 2026-05-27 (rc.0 on 05-11), after four years and 50+ releases. The QUIC layer is now **noq** (iroh's Quinn fork, its own project as of 0.97), with QUIC multipath and QUIC-NAT-Traversal (an emerging IETF standard) replacing the old custom holepunch. Two independent heavyweight stacks converged onto iroh: **Holochain** switched from its WebRTC library (tx5) to iroh for "it just works" holepunching (Kitsune2 iroh transport ~58%), and **p2panda** rewrote its stack onto iroh plus BLAKE3 plus Ed25519 plus CBOR plus UCAN plus PlumTree/HyParView. That ingredient list is Mere's ingredient list. The foundational choice is confirmed.

| Layer | Verdict for Mere |
|---|---|
| **iroh plus noq** | Adopt (done). Foundational, 1.0, externally validated. Plan for the 0.98 to 1.0 API bump. |
| **iroh-blobs** | Adopt (done, wired). BLAKE3 CAS. The near-term win lives here. |
| **iroh-gossip** | Adopt (done, wired). HyParView plus PlumTree presence/churn. |
| **iroh-docs** | Caution. RBSR works (Meyer), but it is the least-committed crate, not on the 1.0 stability train. Keep it a projection; be ready to own or replace. |
| **p2panda (core/net/sync/blobs/stream)** | Strong adopt-candidate, re-evaluate now. iroh-based, BLAKE3/Ed25519/CBOR, modular (caps via header extensions / p2panda-auth). May give much of the MereEvent DAG for free. Note: the stack is not uniformly versioned (core/net/auth/encryption 0.6.1, blobs 0.5.2) and pins iroh 0.98, not 1.0. |
| **Willow / willow25 / Meadowcap / iroh-willow** | Evaluate. Meadowcap feature-complete; data model nearly done; iroh-willow unreleased. Adopt for the 3D data model plus meadowcap if graph-cluster namespaces land; the WILLIAM3/Bab payload-hash vs iroh-BLAKE3 dual-address boundary needs a measured spike. |
| **Keyhive / Beelay (Ink & Switch)** | Watch only. Pre-alpha, unaudited, co-designs Automerge's next sync (Beelay), E2EE-where-the-server-cannot-decrypt. The serious group-key/membership candidate, too early to depend on. |
| **Veilid** | Optional. Active (0.5.3, Mar 2026). Per-moot privacy transport, opt-in. |
| **Loro / Automerge 3** | Projection, not substrate. Loro is Rust-native with version-DAG/frontiers (preferred for event-DAG alignment); Automerge 3 cut memory ~10x and has full-time maintainers. For collaborative-edit views. |
| **NextGraph / oxigraph** | Narrow donor. RDF-CRDT, active. Derived-RDF projection only; do not adopt its repository/broker/identity stack. |
| **Biscuit** | Policy-token candidate. Datalog auth, offline attenuation, for tessera/quota/role/heartbeat gates. |
| **UCAN** | Interop envelope. Portable capability sharing and external agents. (p2panda references UCAN as an integratable external system, not a built-in; p2panda's own caps come via header extensions / p2panda-auth.) |
| **Farcaster Snapchain** | Negative signal, understood. CRDT to BFT because global single-namespace eventual consistency could not hold. Different topology from bounded moots; heed the lesson, do not adopt the conclusion. |
| **Federated social (ATProto / Nostr / ActivityPub / Matrix)** | ATProto is standardizing at IETF (working group chartered Jan 2026); three-way bridging via Bridgy Fed is becoming real. Validates Pattern B (outbound bridges) and suggests leaning on existing bridges rather than building all of them. |

## 9. Recommendations (concepts, crates, UI to adopt)

1. **Ship the cheap p2p win first.** Wire `eidetic-iroh-fetcher` plus iroh-blobs to replicate engrams/snapshots by hash. Cheesecloth-pinning v0, needing none of the moot machinery.
2. **Run the p2panda adopt-vs-build spike before writing the MereEvent DAG core.** The decision with the most leverage. If p2panda-core/net fit, Mere skips building and maintaining a signed-event-over-iroh layer.
3. **Reconcile the Cable contradiction in code.** Either finish the pivot (CBOR MereEvent over iroh streams, BLAKE3, drop the BLAKE2b Cable wire) or declare Cable a shipped protocol and stop documenting it as dropped. Pick one; stop shipping three answers.
4. **Do the one free structural task now:** upgrade `eidetic::schema::Hash` to CIDv1/multihash before event hashes, cap scopes, and engram receipts proliferate. p2panda's flag-day BLAKE2b to BLAKE3 migration is the precedent worth not repeating.
5. **Capability stack: prototype meadowcap-shaped-native plus Biscuit policy**, not gated on willow25 or Keyhive. Keep Keyhive watch-only.
6. **UI ideas worth aligning to:** Element-style live multi-account sub-instances (already in the vault plan); verified-vs-unverified identity cards driven by back-claim status; a per-slot "what does losing this device mean" recovery surface; the volvelle (radial moot expansion) and the "your strophalos has N moots" presence framing.
7. **Keep graph-cluster namespaces as the north star.** Prototype Louvain-to-path derivation against graph-layout/cartography once the kernel's field/clustering work settles. It is the most defensible original contribution.

On the framing of the request: iroh is the right foundation, it is now 1.0, and it is externally validated. The discipline is to depend on the connection layer (iroh plus noq plus blobs plus gossip) and treat iroh-docs, Willow, and Keyhive as swappable projections, which the architecture already does on paper. The gap is not the design; it is that the design has run several years ahead of a code layer that currently stops at Cable-over-QUIC plus a vault skeleton.

---

## Findings

### 2026-05-31 — landscape grounded against code

- **The built surface is larger than MURM_AS_BILATERAL implies, and smaller than the substrate brief implies.** Built and tested: iroh transport (blobs+gossip), Cable bilateral sync, identity-vault skeleton plus passphrase backend, standalone misfin. Unwritten: everything from the 05-07 pivot (MereEvent DAG, BLAKE3, moots/tiers/tessera, capability stack, bridges, primitive nodes, discovery endpoints, proof).
- **External validation of the iroh bet is strong.** Holochain and p2panda independently converged on iroh, with p2panda mirroring Mere's exact ingredient list. iroh 1.0 is landing now.
- **iroh-docs is the structural risk**, because iroh 1.0 commits to the connection layer, not the higher protocols. Sync-as-projection is the correct hedge and should be held firmly.
- **p2panda's modular rewrite reopens adopt-vs-build** for the event-DAG core. This was not true when the substrate brief deferred p2panda; it is true now.
- **Five live contradictions** (Cable drop/keep/ship; BLAKE2b vs BLAKE3; Misfin placement vs re-derived boundary; gpui vs Xilem in p2p docs; stale resolved-but-listed open questions) should be reconciled before any p2p code restarts.

## Progress

### 2026-05-31

- Brief created from a full read of the murm/transport/moothold code, the protocol and substrate and moot-tiers briefs, the fit-map, and the lexicon, plus a web survey of iroh 1.0, iroh-docs/gossip/blobs maintenance posture, willow25/iroh-willow, p2panda's rewrite, Keyhive/Beelay, Veilid, Loro/Automerge, Farcaster Snapchain, and the federated-social direction.
- Insights disseminated the same session into the event-DAG substrate brief (Progress + Findings refresh), the protocol architecture plan (iroh-docs caution + Progress note), MURM_AS_BILATERAL (status-update header), and the Cable migration plan (Findings + Progress). DOC_README index updated.
- No code change. Next concrete steps, if pursued, are recommendation 1 (eidetic-iroh-fetcher wire), recommendation 2 (p2panda spike), and recommendation 4 (multihash discipline).

## Sources (external, verified May 2026)

- iroh roadmap and 1.0 RC: <https://www.iroh.computer/roadmap>, <https://www.iroh.computer/blog/iroh-1-0-0-rc-0>, noq: <https://www.iroh.computer/blog/iroh-0-97-0-custom-transports-and-noq>, protocol split: <https://www.iroh.computer/blog/iroh-0-29-net-is-the-new-iroh>
- iroh-docs (RBSR): <https://github.com/n0-computer/iroh-docs>; iroh-gossip: <https://github.com/n0-computer/iroh-gossip>
- Willow / willow25 / iroh-willow: <https://willowprotocol.org/>, <https://github.com/n0-computer/iroh-willow>, <https://github.com/earthstar-project/willow-rs/releases>
- p2panda rewrite (iroh-based modular crates): <https://p2panda.org/2024/12/06/p2panda-release.html>, <https://docs.rs/p2panda-net/latest/p2panda_net/>
- Keyhive / Beelay (Ink & Switch): <https://www.inkandswitch.com/project/keyhive/>
- Veilid: <https://veilid.com/>, <https://gitlab.com/veilid/veilid/-/releases>
- Holochain on iroh: <https://blog.holochain.org/ongoing-delivery-demonstrating-whats-next/>
- Farcaster Snapchain (CRDT to BFT): <https://medium.com/@heimlabs/snapchain-how-farcaster-rewired-social-media-for-a-decentralized-future-e4c525754786>
- Loro / Automerge: <https://github.com/loro-dev/loro>, <https://automerge.org/blog/automerge-2/>
- Decentralized social 2026 (ATProto IETF, bridging): <https://en.wikipedia.org/wiki/AT_Protocol>, <https://fediview.com/articles/mastodon-vs-bluesky-vs-nostr-2026/>
