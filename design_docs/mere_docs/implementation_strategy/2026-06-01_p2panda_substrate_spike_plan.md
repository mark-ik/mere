# p2panda substrate spike (adopt-vs-build), and the iroh-transport refactor it decides

**Date**: 2026-06-01
**Status**: Spike plan. Phase 0 is probes in a throwaway crate (no live-crate edits); a decision gate follows; Phase 1+ is a conditional migration, sketched but gated on Mark's go-ahead after the gate.
**Scope**: Decide whether the Mere event-DAG substrate and its iroh transport are **built on our own iroh wrapper** or **adopted from the p2panda modular stack** (`p2panda-net` / `p2panda-core` / `p2panda-sync` / `p2panda-blobs` / `p2panda-auth` / `p2panda-encryption`). The iroh-transport refactor is not a separate task: its shape is the output of this decision.
**Related**:

- [`../research/2026-05-31_murm_p2p_landscape_brief.md`](../research/2026-05-31_murm_p2p_landscape_brief.md) — the survey this plan acts on (recommendation 2: run the spike before writing the MereEvent DAG core; recommendation 4: multihash discipline).
- [`2026-05-07_event_dag_substrate_brief.md`](2026-05-07_event_dag_substrate_brief.md) — §4 MereEvent grammar, §5 sync-as-projection (where p2panda was deferred), §8.8 capability stack, §13 multihash discipline. The build target this plan compares adoption against.
- [`2026-05-05_protocol_architecture_plan.md`](2026-05-05_protocol_architecture_plan.md) — §2 iroh-as-toolkit, §2.4 iroh-docs caution.
- [`2026-05-10_graph_cluster_namespaces_brief.md`](2026-05-10_graph_cluster_namespaces_brief.md) — `mere-namespace` / meadowcap direction the capability comparison touches.
- Code: [`crates/murm/transport/src/iroh_transport.rs`](../../../crates/murm/transport/src/iroh_transport.rs) (the `IrohTransport` under review), [`crates/murm/transport/src/transport.rs`](../../../crates/murm/transport/src/transport.rs) (the `Transport` trait that is the adapter seam), [`crates/murm/murm/src/lib.rs`](../../../crates/murm/murm/src/lib.rs) (`Murm<T: Transport>`).

---

## 1. Why these are one decision

p2panda's modular crates map onto Mere's p2p layers almost one to one:

| Mere layer (today) | p2panda crate | Today's state in Mere |
|---|---|---|
| `transport::IrohTransport` (iroh 0.98 wrapper) | `p2panda-net` (discovery + connect + byte streams over iroh) | built, tested, on iroh 0.98 |
| MereEvent DAG (substrate brief §4) | `p2panda-core` (signed append-only operations, custom extensions) | unwritten (README prose only) |
| sync projection (substrate brief §5; iroh-docs role) | `p2panda-sync` | unwritten |
| `transport::blobs` + `eidetic-iroh-fetcher` | `p2panda-blobs` | blobs built local-only; fetcher thin |
| discovery (unbuilt) | `p2panda-discovery` | not built (explicit `add_peer` only) |
| capability stack (substrate brief §8.8) | `p2panda-auth` (caps) + `p2panda-encryption` (Double Ratchet) | unwritten |

So the iroh-transport refactor has exactly two shapes, and which one we do **is** the adopt-vs-build outcome:

- **Build.** Keep `IrohTransport`, bump it from iroh 0.98 to iroh 1.0 / noq, fix the ed25519-dalek seam, add real discovery, and write the MereEvent DAG plus sync ourselves.
- **Adopt.** Implement the existing `Transport` trait over `p2panda-net` (so `Murm<T>` and Cable do not change), retire our hand-rolled iroh wrapper, and lean on `p2panda-core`/`-sync`/`-blobs` for the substrate Mere has not written yet.

Either way we refactor, because we are two releases behind iroh 1.0. The spike decides whether we refactor **forward on our own iroh** or **off our iroh onto p2panda-net**.

## 2. The seam that makes the swap cheap

The transport crate already defines a `Transport` trait, and `Murm<T: Transport>` is generic over it (not `Box<dyn Transport>`). The in-memory test fixture and `IrohTransport` are two impls of it today. A p2panda-net-backed `P2pandaTransport` would be a third impl. That means:

- Adopting p2panda-net at the transport layer touches **one new file**, not murm or Cable.
- Cable's snapshot push/pull is written against `AsyncRead + AsyncWrite` streams; the spike must confirm p2panda-net exposes a byte-stream surface that satisfies that bound (it advertises "exchange data in form of byte streams", so the prior is good).
- `eidetic-iroh-fetcher` is the other consumer; it moves to `p2panda-blobs` or stays on our blobs facade, decided in S1/S4.

This seam is why the spike is low-commitment: nothing in the live crates changes during Phase 0.

## 3. Facts already established (do not re-litigate in the spike)

- **License is fine.** `p2panda-core` and `p2panda-net` are dual MIT / Apache-2.0. Only the bootstrap-node binary is AGPL, and Mere does not link it. (Verified 2026-06-01 against crates.io.)
- **p2panda is iroh-based and BLAKE3-native**, using Ed25519, CBOR, UCAN, PlumTree/HyParView, Double Ratchet. This mirrors Mere's intended ingredient list, so adoption does not pull in a foreign stack.
- **p2panda-core is extension-driven.** Operation = Header + Body. Header carries version, verifying_key, signature, payload_size, payload_hash, timestamp, seq_num, backlink, and a first-class `extensions` field with an `Extension` trait for user-defined fields (their own examples: access-control tokens, expiry/ephemeral, encryption schemas).
- **iroh 1.0 migration is mostly mechanical** (Node→Endpoint renames, RelayNode→RelayConfig, dynamic relays, infallible Connection remote-id/ALPN, 0-RTT restructure, addr normalization). We already absorbed some of these renames at 0.98.

## 4. The one real open question: data-model fit

p2panda-core models a **per-author append-only log** (one signed linear chain per `verifying_key`, linked by `backlink` and `seq_num`); the global structure across authors is a DAG of logs. MereEvent (substrate brief §4) is drawn as a **multi-parent causal DAG** (`parents: Vec<EventHash>`). These are not the same shape. The spike must resolve one of:

- **Reframe MereEvent onto per-author logs plus semantic refs.** Drop multi-parent hash links; keep causal/semantic ordering via `target_refs` plus hybrid timestamps. This is what Bamboo/Bluesky/Nostr-shaped systems do, and it likely costs Mere nothing real because moot ordering is per-author-plus-merge, not a global content-hash DAG. If true, p2panda-core fits and `space_id`/`target_refs`/`capabilities`/`event_type` become extensions.
- **Keep a true multi-parent DAG.** Then p2panda-core's log model fights us, and the build path (own MereEvent grammar) wins on the substrate even if we still adopt p2panda-net for transport.

This single question, answered concretely in S2, is the hinge of the whole decision.

## 5. Phase 0 spikes (probes in `crates/probes/` or a throwaway sketch crate)

Each spike has a done-condition, not a duration. None touch the live `transport`/`murm`/`eidetic` crates.

| ID | Spike | Done when |
|---|---|---|
| **S1** | **p2panda-net transport probe.** Stand up two `p2panda-net` nodes, discover, open a stream, round-trip Cable snapshot bytes (the same payload `push_cabal_to_peer` sends). Sketch a `P2pandaTransport: Transport` impl against the real trait. | Bytes round-trip on p2panda-net, and a written note compares its LOC + ergonomics against `IrohTransport` and records whether the byte-stream surface satisfies Cable's `AsyncRead + AsyncWrite` bound. |
| **S2** | **p2panda-core data-model fit (the hinge).** Express two MereEvent types (ChatMessage, GraphAddNode) as p2panda Operations, with `space_id`/`target_refs`/`capabilities`/`event_type` as custom `Extension`s. Encode, sign, decode, verify. Decide the §4 question: per-author-log-plus-refs vs multi-parent DAG. | Two event types round-trip as Operations, and a written verdict states whether MereEvent reframes onto p2panda-core cleanly or needs a multi-parent model p2panda-core does not give. |
| **S3** | **Build-path baseline: iroh 0.98 to 1.0 / noq.** In a branch, port `IrohTransport` to iroh 1.0 / noq. Measure the diff; fix the ed25519-dalek 2.x vs iroh seam at the new version. | The transport test suite passes on iroh 1.0 (or a written blocker list exists). This is the cost number the adopt path (S1) is compared against. |
| **S4** | **Capability + encryption survey.** Read `p2panda-auth` and `p2panda-encryption` against substrate-brief §8.8 (meadowcap structural / Biscuit policy / Keyhive group-key). Decide whether they substitute for, complement, or are irrelevant to the meadowcap+Biscuit+Keyhive plan, and whether they help the `mere-namespace` / graph-cluster direction. | A fit/gap table maps p2panda-auth/-encryption onto the three capability layers, with a keep/borrow/ignore call per layer. |
| **S5** | **Pin-skew + multihash check.** Record p2panda-net's pinned iroh version against our iroh 1.0 target, and p2panda-core's BLAKE3/Ed25519 versions against ours. Confirm whether p2panda's BLAKE3 hash type satisfies the §13 multihash discipline or needs a CIDv1 wrapper. | A version table shows skew (if any) and whether the workspace can pin one iroh, plus a one-line multihash verdict feeding recommendation 4 of the landscape brief. |

## 6. Decision gate

After Phase 0, choose a position. The choice is not binary; p2panda can be adopted at four depths:

| Position | What we adopt | What we still own | Wins | Costs |
|---|---|---|---|---|
| **P0 Build** | nothing | iroh wrapper, event DAG, sync, caps | maximum control; no external coupling | we maintain the exact iroh/sync burden the landscape brief flagged; slowest to a working substrate |
| **P1 Adopt net** | `p2panda-net` (+ maybe `-blobs`, `-discovery`) | event grammar, sync, caps | sheds iroh/discovery maintenance; keeps our data model; smallest blast radius (one `Transport` impl) | one external dep on the transport hot path |
| **P2 Adopt net + core** | `+ p2panda-core` | moot/tessera semantics, sync policy | sheds the signed-log substrate too; only viable if S2 says the log model fits | event grammar shaped by p2panda's Operation model |
| **P3 Adopt the stack** | `+ p2panda-sync/-auth/-encryption` | moot governance + tessera only | maximum leverage; E2EE + caps + sync for free | deepest coupling to p2panda's evolution and pin set |

**Decision rule.** P1 wins if S1's adopt cost is below S3's build cost and the pin-skew (S5) is tolerable. P2/P3 additionally require S2 to say the data model fits and S4 to say the caps/encryption earn their coupling. If S2 says the log model fights MereEvent, stop at P1 (adopt net, build our own grammar) or P0.

**Prior (not a pre-decision).** Given the established facts (MIT/Apache, iroh-based, BLAKE3-native, extension-driven, mirrors our ingredient list) and the `Transport`-trait seam, **P1 is the most likely high-leverage outcome**: it removes the iroh-maintenance and discovery burden, keeps our data model under our control, and changes one file. P2/P3 are upside that the hinge question (S2) and the caps survey (S4) either unlock or rule out. The spike decides; this plan does not.

## 7. The iroh refactor, made concrete per outcome

Answering Mark's question directly: yes, the iroh implementation gets refactored either way. The shapes:

- **If P0 (build):** refactor `IrohTransport` 0.98 to 1.0 / noq (S3), then add the two things it lacks for production: real discovery (n0-DNS / pkarr / mDNS behind a toggle; today it is explicit `add_peer` only) and a graceful `Router` shutdown in `Drop` (today it aborts). The ed25519-dalek seam (raw-seed bridge) is revisited at 1.0. We keep owning blobs+gossip wiring.
- **If P1+ (adopt):** the refactor is a **subsume**, not a port. `P2pandaTransport: Transport` becomes the production impl; `IrohTransport` is demoted to a test/loopback fixture or deleted once parity is shown. Cable's raw-bi-stream push/pull moves onto p2panda-net's stream API. `eidetic-iroh-fetcher` retargets `p2panda-blobs`. Discovery and gossip come from p2panda-net, so our hand-rolled `MemoryLookup` + per-ALPN queue code retires. This is a **dependency swap** (drop direct `iroh`/`iroh-gossip`/`iroh-blobs` from transport in favour of `p2panda-net`), so per workspace memory it is gated on Mark's explicit go-ahead, not done inside the spike.

Both paths sit on the iroh 0.98 line today (p2panda-net 0.6.1 still pins iroh `0.98.2`); neither is on the 1.0 RC yet. The difference is who carries the eventual 1.0 move: us alone (build), or p2panda upstream (adopt).

## 8. Phase 1+ (conditional, gated on the decision gate)

Sketched so the gate has somewhere to land. Not started until Mark picks a position.

- **If P1:** (1) `P2pandaTransport: Transport` reaches parity with `IrohTransport` on the Cable round-trip and gossip tests; (2) wire it behind the same `Murm::new` path; (3) retire `IrohTransport` to test-only; (4) move `eidetic-iroh-fetcher` to `p2panda-blobs`; (5) single workspace iroh pin via p2panda-net.
- **If P2/P3:** above, plus (6) MereEvent types as p2panda-core Operations + extensions (closes substrate brief §4); (7) sync projection on `p2panda-sync` (closes §5, replaces the iroh-docs-roster track in the protocol plan §6); (8) capability layer evaluation lands on p2panda-auth or the meadowcap+Biscuit plan per S4.
- **If P0:** the build tracks already in the substrate brief §9 proceed, now with the iroh 1.0 bump (S3) as the transport baseline.

In all paths, recommendation 4 (multihash discipline on `eidetic::schema::Hash`) lands independently and first; it is free and is not gated on this decision.

## 9. The hash refactor (in scope; lands first, and mostly free)

Mark folded hashes into this refactor, and they pull the same direction as everything else. Three hash regimes coexist today against a substrate brief (§2, §13) that says "unify on BLAKE3, and make every digest field multihash-aware":

- identity derivation: BLAKE2b-256 (`persona/identity/keypair.rs`).
- murm cabal-id: BLAKE2b (Cable spec).
- `eidetic::schema::Hash`: raw BLAKE3 `[u8;32]`, not the CIDv1/multihash type §13 requires.

Two sub-decisions, different cadence:

1. **Content / payload / event hashes to BLAKE3 behind a CIDv1 multihash type.** This is landscape-brief recommendation 4: free, pre-1.0, and it lands first regardless of adopt-vs-build. Upgrade `eidetic::schema::Hash` to a multihash-aware representation before event hashes, cap scopes, and engram receipts proliferate. p2panda's flag-day BLAKE2b-to-BLAKE3 migration (which needed a flag day precisely because it lacked multihash discipline) is the precedent worth not repeating.
2. **Identity-derivation hash (BLAKE2b to BLAKE3-keyed).** Substrate brief §13 calls the identity-derivation hash an immutable historical fact, independent of payload-hash agility. But there are no production identities yet, so flipping derivation to BLAKE3 keyed-hash now is a free pre-1.0 change. Decide explicitly: keep BLAKE2b (matches the original Cable spec) or move to BLAKE3 (matches the unification and p2panda). The recommendation is to move it now while it is free, because the substrate pivot drops the Cable-wire interop that BLAKE2b was protecting.

**p2panda makes this easier, not harder.** p2panda-core is BLAKE3-native (payload_hash is BLAKE3, signatures Ed25519). Adopting it (P2/P3) makes BLAKE3 the event hash by default and removes the BLAKE2b-on-the-wire question entirely. So "and hashes" and "adopt p2panda" point the same way. S5 confirms whether p2panda's hash type already satisfies the multihash discipline or wants a thin CIDv1 wrapper at our boundary.

Sequencing: (1) `eidetic::schema::Hash` to CIDv1 multihash, free and first and ungated; (2) cabal-id plus identity derivation to BLAKE3, with the Cable-wire reconciliation and the p2panda gate; (3) S5 reconciles p2panda's hash type with our multihash boundary.

## 10. Guardrails

- **No live-crate edits in Phase 0.** Probes live in a throwaway crate; the `transport`/`murm`/`eidetic` crates are untouched until the gate.
- **Dependency swap needs explicit sign-off.** Adopting p2panda-net drops direct iroh deps from transport. Per workspace memory (ask before dropping dependencies), the swap is a gated Phase 1 step, not a spike side effect.
- **Pin discipline.** The probe crate adapts to whatever iroh version p2panda-net pins; the workspace does not churn its iroh pin to chase the probe. Reconcile to one pin only at Phase 1 (S5 feeds this).
- **Keep the `Transport` trait stable.** It is the seam that makes the whole swap cheap; do not let p2panda-net types leak through it.

## 11. Toolkit, not framework: keeping the spatial vision unbound

Mark's framing (2026-06-01): whether we keep Cable is not the crux. The crux is whether p2panda is the right architectural decision, taken as a *toolkit* (the way libp2p / iroh / Willow were intended) rather than a *framework* whose worldview binds Mere's spatial, moot-as-graph-view vision of decentralized browsing.

**p2panda is built to be used as a toolkit.** Its lib.rs says so directly: "Modular API allowing users to choose or replace the layers they want to use"; modules are feature-flagged (`address_book`, `discovery`, `gossip`, `iroh_endpoint`, `sync`, `supervisor`); it is "agnostic over the actual data of your application" ("CRDTs, messages, etc."); and it exposes "lower-level APIs and traits in p2panda-sync which allow you to implement your own sync protocol." The S1 probe confirmed the Endpoint hosts arbitrary custom protocols and returns the raw iroh endpoint. Nothing forces whole-framework adoption.

**Is it document-bound in a way that detracts from the spatial interface? Layered:**

- **Transport / discovery (`p2panda-net::Endpoint`, `Discovery`, `MdnsDiscovery`): no data model, no document-binding.** iroh + discovery + Erlang-style supervision. The pure-toolkit slot (the libp2p/iroh slot). Lowest-risk take.
- **Data / sync (`p2panda-core`, `LogSync`, `p2panda-spaces`): log/feed-shaped, not document-shaped, and it does not touch the spatial layer.** p2panda-core is per-author append-only signed-event logs; LogSync converges them. The moot's *truth* is already event-sourced in Mere's own design (the [two-natured kernel](../technical_architecture/2026-05-30_two_natured_kernel_brief.md): content is authoritative signed events; the spatial graph view is a *derived projection*). So the spatial interface is a projection regardless of substrate; p2panda would sit at the content-truth layer beneath it, never making the moot "a document."
- **The genuine vision-lock risks, named:**
  1. **Framing drift.** LogSync's "topic = log/feed" and `p2panda-spaces`' "space" vocabulary nudge toward "moot = feed/space of posts" rather than "moot = spatial graph of typed nodes." Adopting them wholesale risks importing that mental model.
  2. **Flat topics vs hierarchical graph-cluster namespaces.** p2panda topics are flat BLAKE3 hashes; Mere's [graph-cluster-derived namespaces](2026-05-10_graph_cluster_namespaces_brief.md) are hierarchical paths that mirror the graph's community structure. Willow / Meadowcap's namespace/subspace/path model is *more* aligned with the spatial vision here than p2panda's flat topics. This is a slot where p2panda is the weaker fit.
  3. **Per-author log vs multi-parent DAG** (the S2 hinge, still unproven-droppable).

**The guardrail that keeps us unbound: hold the substrate brief's inversion.** "Event-DAG-as-identity, sync-as-projection" is the architectural firewall. As long as Mere's event DAG is the identity and p2panda's LogSync (or Willow, or iroh-docs) is one projection behind that boundary, p2panda is a toolkit we consume, not a framework that defines us. The moment LogSync or `spaces` becomes the moot's identity, we are p2panda-shaped. So: keep the projection boundary; never let a sync backend or a "space" be the moot. The moot stays Mere's spatial graph-view projection, addressed by graph-cluster paths.

**Heterogeneous toolkit, not monoculture.** The likely shape: **p2panda-net (transport + discovery)** is the strong, low-risk take (the iroh+discovery toolkit we would otherwise build). **p2panda-core / -encryption** are candidate donors for the signed-event format and E2EE. But the **spatial namespace + capability** layer may want **Willow-shaped paths or Mere-native** addressing, not p2panda's flat topics + group model. Willow stays a live alternative for that slot; the graph-cluster-namespace direction points at it. Do not mono-commit to p2panda for layers where Willow or Mere-native is more spatially aligned.

**What this means for the gate.** The decision is not "adopt the p2panda framework." It is per-layer, behind the projection boundary:

- **Transport + discovery:** adopt p2panda-net (low risk, high value, S1-validated).
- **Signed-event format:** p2panda-core OR Mere-native MereEvent (near-identical; pick on integration ergonomics, keep it behind the DAG-as-identity boundary).
- **Sync:** LogSync as one projection, not the identity; keep iroh-docs / Willow / Mere-native swappable.
- **Spatial namespace + caps:** evaluate Willow / Meadowcap / Mere-native *against* p2panda-auth/spaces; lean toward whatever preserves graph-cluster paths.
- **Group/key + E2EE:** p2panda-encryption / -spaces as donors, evaluated, not assumed.

## Findings

### 2026-06-01 — Phase 0 spikes run

Probe artifact: [`crates/probes/p2panda-fit`](../../../crates/probes/p2panda-fit) (standalone crate, own `[workspace]`, does not touch the main workspace). p2panda-core / -net / -auth / -encryption are all at **0.6.1** (published 2026-05-22), dual MIT/Apache.

**S5 (pin-skew): no skew today.** p2panda-net 0.6.1 depends on iroh `^0.98.2`, iroh-gossip `^0.98`, iroh-base `^0.98`, the same iroh 0.98 line `transport` already pins. Also pulls tokio 1.52, serde 1.0.228, ractor 0.15 (actor supervision), rand 0.10, futures-util. iroh-blobs is not a p2panda-net dep (blobs is the separate `p2panda-blobs`, which **lags at 0.5.2** while core/net/auth/encryption are 0.6.1 — an intra-stack version skew to weigh). Our own `BlobStore::fetch_from` already does p2p blob transfer, so Mere blobs are not local-only and not a reason to adopt; keep the facade until `p2panda-blobs` is actually measured. Adopting p2panda-net needs no iroh-version churn now (both on iroh 0.98); the eventual 1.0 move is shared with p2panda rather than ours alone.

**S2 (the hinge): FITS with a reframe. Proven empirically, runs green.** p2panda-core's `Operation = Header + Body`. The `Header` is, by its own module docs, "a linked list of operations, where every subsequent operation points to the previous one" via a single `backlink` + `seq_num` (per-author, single-parent). The generic `extensions` field is the documented home for "capabilities ... and custom application-related features." The probe carried every MereEvent field (`space_id` / `event_type` / `target_refs` / `capabilities` / `parents`) as a `MereExt` extension, signed it (Ed25519), CBOR round-tripped it (a GraphAddNode header is 349 bytes), and the signature verified after the round-trip. The multi-parent `parents` (2 links in the probe) ride in `extensions`, opaque to p2panda's native ordering/sync. Verdict: p2panda-core fits **if MereEvent reframes onto per-author-log + semantic refs**; a native multi-parent content-hash DAG is not provided and would be application-level extension data the sync layer ignores. For moot ordering (per-author plus merge) that reframe costs nothing real. p2panda-core hashes with BLAKE3 natively, which also settles the hash question on the wire.

**S1 (transport fit): the adopt unit is net + core, not net-only.** p2panda-net 0.6.1 is modular and co-designed with core: `Endpoint` (QUIC over iroh), `AddressBook`, `Discovery` (confidential random-walk topic discovery) + `MdnsDiscovery`, `Gossip`, `LogSync` (eventual-consistency sync for append-only logs), `Supervisor`. It is abstracted around topic pub/sub plus log sync, not a low-level connect/accept-by-ALPN like our `Transport` trait. Its high-value pieces are exactly what we have not built: discovery (we are explicit-`add_peer`-only) and `LogSync` (the sync projection / iroh-docs role), and `LogSync` is append-log-shaped, matching p2panda-core operations and the reframed MereEvent. Consequence: the realistic adopt unit is **net + core together (P2)**, not net-only (P1), because LogSync's value is coupled to the core Operation model. In that path our Cable raw-bi-stream snapshot protocol *could* retire in favour of LogSync. **[Corrected 2026-06-01 — see the review-corrections entry below.]** This S1 conclusion was too strong. Source-level review of `p2panda-net/src/iroh_endpoint/api.rs` confirms the `Endpoint` *does* expose the low-level surface: `accept(protocol_id, handler)` (custom ALPN), `connect(node_id, protocol_id) -> iroh::endpoint::Connection` (raw streams by peer + ALPN), and `endpoint() -> iroh::Endpoint` (the raw iroh endpoint). So a low-level `P2pandaTransport` adapter preserving Cable and the one-endpoint/multi-protocol behavior is viable, Cable need not be retired to adopt, and there are two adopt layers to compare (net::Endpoint adapter vs the high-level `p2panda::Node`).

**S3 (iroh 1.0 port surface): small in our code, but the multi-crate tracking is the real cost.** `IrohTransport` already uses the renamed `Endpoint` / `EndpointId` / `EndpointAddr` API at 0.98, so the headline iroh 1.0 rename (Node→Endpoint) is mostly absorbed. We do not configure relays (`Builder::empty`) or use 0-RTT, so RelayNode→RelayConfig and the 0-RTT restructure do not touch us. The genuine burden is that we wire `iroh-blobs` (0.100) and `iroh-gossip` (0.98) ourselves, so the build path means tracking three community crates across the 1.0 transition in lockstep, the exact maintenance the landscape brief flagged (blobs/docs/gossip are off the 1.0 stability train). That is the burden p2panda-net absorbs.

**S4 (caps + encryption): p2panda retires the two riskiest §8.8 bets.** p2panda-auth 0.6.1 is decentralized group access control: four access levels (Pull/Read/Write/Manage), a signed-operation DAG with strong-removal conflict resolution, CRDT-adjacent, and sync-integrated (membership/access decides who to sync with and over which data). p2panda-encryption 0.6.1 ships two schemes: Data Encryption (symmetric group key, XChaCha20-Poly1305, post-compromise security via key rotation, optional forward secrecy, for late-joiner/historical access) and Message Encryption (Double-Ratchet-like, per-message forward-secure keys, for group chat). Mapped to substrate-brief §8.8: auth + encryption together cover the "live group/key state" layer (the **pre-alpha, unaudited Keyhive slot**) and the planned **MLS** bilateral-E2EE module, as shipping MIT/Apache co-designed crates. They do **not** provide meadowcap-style structural namespace/path caps (graph-cluster namespaces stay Mere-native) nor Biscuit-style datalog policy (tessera thresholds/quotas/heartbeats stay Mere-native or Biscuit).

### 2026-06-01 — decision-gate recommendation (spike-informed)

The plan's pre-spike prior was P1 (adopt net only, keep our data model). The spikes move the recommendation to **P2, leaning P3**:

- **Adopt p2panda-net + p2panda-core together.** Their value is coupled via LogSync (S1); the data model fits via extensions once MereEvent reframes onto per-author-logs + refs (S2); the pins already align on iroh 0.98 (S5).
- **Seriously adopt p2panda-auth + p2panda-encryption (P3)** to retire the pre-alpha Keyhive bet and the heavy MLS build (S4). This de-risks §8.8 more than it couples us.
- **Keep Mere-native:** meadowcap-shaped structural namespace caps (graph-cluster namespaces) and Biscuit/tessera policy. p2panda does not provide these and should not be expected to.
- **The iroh refactor, in this path, replaces our router, not the behavior.** [Revised 2026-06-01 per review.] We do not bump `IrohTransport`'s hand-rolled router to iroh 1.0 ourselves; we make `p2panda-net::Endpoint` the endpoint authority (it exposes custom ALPN, raw `connect`, and the iroh endpoint, so it can host Cable + blobs alongside p2panda sync) and keep the `Transport` trait plus the one-endpoint/multi-protocol behavior. Cable stays functional during migration and retires only later, if `LogSync` proves it redundant. Neither we nor p2panda-net is on iroh 1.0 today (both on 0.98); adopting shares the future 1.0 move with p2panda rather than getting us to 1.0 now.
- **Hashes:** adopting p2panda-core makes BLAKE3 the event hash natively (S2), removing the BLAKE2b-on-the-wire question; the multihash-discipline task on `eidetic::schema::Hash` still lands independently and first.

**P0 (build) remains the fallback** if any of these is unacceptable: the multi-parent reframe (S2), the deeper coupling to p2panda's release cadence and pin set, or a hard requirement for a protocol p2panda-net's Endpoint cannot carry.

**What must be true for the recommended path:** (1) the multi-parent reframe is acceptable for moot ordering; (2) we accept coupling to p2panda's release cadence (today aligned at iroh 0.98); (3) p2panda-net's Endpoint either exposes raw streams or LogSync fully replaces Cable's snapshot need; (4) meadowcap graph-cluster namespaces and tessera policy stay Mere-native regardless.

**Empirical follow-ups before committing (gated on Mark picking the gate position):** a real two-node p2panda-net `LogSync` round-trip of a MereEvent-as-Operation, and confirmation of `Endpoint` raw-stream access. Independently and first, regardless of the gate: multihash discipline on `eidetic::schema::Hash`.

### 2026-06-01 — review corrections (external review + Mark's framing)

An external review plus Mark's framing corrected several overreaches above. Corrections, verified against the p2panda source:

- **Phase 0 is source reconnaissance plus one green probe, not complete.** Only S2 has a runnable artifact. S1's two-node round-trip, S3's actual RC port (or a blocker list from a port attempt), and S4's fit/gap table were not run. The gate has not truly been reached; the recommendation above is a prior, not a settled outcome.
- **S1 was wrong: `p2panda-net::Endpoint` does expose the low-level surface.** Verified in `p2panda-net/src/iroh_endpoint/api.rs`: `accept(protocol_id, handler)` (custom ALPN), `connect(node_id, protocol_id) -> iroh::endpoint::Connection` and `connect_with_config(...)` (raw streams by peer + ALPN), and `endpoint() -> iroh::Endpoint` (the raw iroh endpoint). So there are **two adopt layers to compare**, not one: (a) low-level `p2panda-net::Endpoint` as the endpoint authority, keeping Cable + blobs + the one-endpoint/multi-protocol behavior and the `Transport` trait; (b) high-level `p2panda::Node` (bundles discovery + LogSync + SQLite persistence + causal ordering + replay + acks, constructs its own endpoint, uses fixed internal header extensions) replacing more of the substrate. The §2 `Transport`-trait seam is vindicated for layer (a).
- **Preserve behavior, not the router.** Worth keeping: one endpoint / one peer identity / many protocols (Cable, blobs, gossip share an endpoint), the raw-ALPN escape hatch (`connect_raw` already powers working p2p blob transfer), incremental migration (Cable stays live while LogSync proves itself), and the narrow `Transport` trait boundary with an in-memory test backend. Not worth protecting: the hand-rolled per-ALPN queues, `MemoryLookup`, router lifecycle, crude shutdown, and direct dep wiring. Deleting `IrohTransport` is premature until a probe shows `p2panda-net::Endpoint` can host Cable + blobs alongside p2panda sync cleanly.
- **S2 proves representability, not DAG-droppability.** The probe shows multi-parent `parents` survive as opaque extension data. It does not show that per-author-log + semantic refs + timestamps can *replace* the causal DAG, which the substrate brief calls the source of truth. Keep `parents` as application-level event data until a multi-author concurrency test demonstrates redundancy. The "costs nothing real" claim in S2 is withdrawn.
- **S4 / P3 is an evaluation, not a retirement.** p2panda-auth + p2panda-encryption are serious donors; the integration crate for live group/key state is **p2panda-spaces** (combines auth + encryption, nested groups, multi-device), which explicitly states its APIs are not production-stable before v1. It is the candidate to evaluate against the Keyhive watch slot. It does not automatically retire every MLS-shaped requirement. "Retire the Keyhive and MLS bets" above is softened to "evaluate as candidates."
- **Fact fixes.** Neither we nor p2panda-net is on iroh 1.0 (both pin iroh 0.98.2; iroh itself is on the 1.0 RC line). p2panda-blobs lags at 0.5.2 (intra-stack skew). Our blobs facade already does p2p (`fetch_from`), so it is not local-only and stays until `p2panda-blobs` is measured. p2panda does not "already use UCAN" (it is an integratable external system; p2panda's own caps come via header extensions / p2panda-auth). The landed eidetic change is multihash-aware (multicodec + uvarint-length + digest), not full CIDv1.

**Revised gate position: P2 evaluation candidate; no retirement approved.** The next spike runs both layers in parallel: (1) a two-node high-level `Node::stream<MereEvent>` test with persistence restart, replay, and concurrent authors, carrying explicit application-level `parents`; (2) a low-level `p2panda-net::Endpoint` Cable + blobs adapter sketched against the current router. Evaluate p2panda-spaces separately for live group/key state. The decisive architectural question: must one endpoint keep serving Cable, blobs, gossip, and moot sync together? Until that is answered empirically, `IrohTransport` is not deleted.

### 2026-06-01 — S1 low-level probe (green): p2panda-net::Endpoint hosts Cable

The decisive question, answered empirically. Probe: [`crates/probes/p2panda-cable`](../../../crates/probes/p2panda-cable), runs green.

- Two `p2panda-net::Endpoint`s on loopback (no relay, no discovery): Bob `accept`s the custom ALPN `mere/cable/v1`; Alice `connect`s, `open_bi`, and pushes the **exact murm Cable snapshot frame** (32-byte cabal_id + varint-length-prefixed posts) over a raw bi-stream; Bob receives all three posts. The wire is unchanged from `Murm::push_cabal_to_peer`.
- `Endpoint::endpoint()` returns the raw `iroh::Endpoint`, the same hook our `IrohTransport` Router uses to register `iroh-blobs`' `BlobsProtocol`. So **Cable + blobs coexist on one endpoint** while p2panda's Discovery / Gossip / LogSync run on the same node. The answer to "must one endpoint keep serving Cable, blobs, gossip, and moot sync together?" is **yes, it can.**
- Construction idiom (all public): `AddressBook::builder().spawn()`, `Endpoint::builder(ab).signing_key(sk).spawn()`, `accept(alpn, handler)` (handler is iroh's `ProtocolHandler`, so our `QueueProtocolHandler` ports directly), `connect(verifying_key, alpn) -> iroh::endpoint::Connection`. Loopback dialing without DNS/discovery via `NodeInfo::from(iroh::EndpointAddr)` + `insert_node_info`. `NodeId = p2panda_core::VerifyingKey`.
- **A `P2pandaTransport: Transport` adapter preserving Cable is viable and mechanical:** `connect` maps to `Endpoint::connect().open_bi()`; `accept` maps to a registered `ProtocolHandler` pushing accepted streams to a channel (our existing pattern). The endpoint authority moves from our hand-rolled Router + `MemoryLookup` to `p2panda-net::Endpoint`; we gain Discovery + LogSync; Cable stays live and retires only later if LogSync proves it redundant.
- **Cost to weigh:** p2panda-net pulls a much larger dependency tree than our current transport — noq (n0's QUIC), iroh-relay, reqwest, igd-next + portmapper (UPnP), netwatch, hickory DNS, sqlx/SQLite (p2panda-store), and ractor (Erlang-style actor supervision). Heavier build, binary, and dependency surface. Much of it (relay, holepunch, portmap, DNS, supervision) is machinery we would want for real discovery anyway, but it is a real surface-area increase over the current iroh + blobs + gossip set.

**Gate status after this probe:** layer (a) (low-level `p2panda-net::Endpoint` as endpoint authority, Cable preserved, blobs coexisting, raw iroh endpoint accessible) is empirically validated. The recommendation stays **P2 evaluation candidate; no retirement approved** — what remains before any commitment is the high-level `Node::stream<MereEvent>` substrate test (persistence restart, replay, concurrent authors, explicit `parents`) and the p2panda-spaces eval. The "replace the router, keep the behavior, migrate incrementally" path is now proven feasible.

### 2026-06-01 — Probe 1: concurrent-author convergence + the causal-completeness gap

Probe: [`crates/probes/p2panda-converge`](../../../crates/probes/p2panda-converge), runs green (the full public setup path).

- **Convergence is p2panda's tested guarantee.** Its own `e2e_log_sync` test (read from p2panda-net 0.6.1 source) drives two authors' associated logs to converge via LogSync (SyncStarted → OperationReceived → SyncFinished → LiveModeStarted, each receiving the other's operations). The probe reproduces the full *public* path (spawn, author MereEvents into a log, associate the log with a topic, subscribe) and it works.
- **The public sync API is discovery-driven, not manually initiated.** `SyncHandle::initiate_session` is `#[cfg(test)]`; out-of-crate, sessions start from discovery (mDNS / random-walk / relay bootstrap), and `TestNode` hardcodes *passive* mDNS, so a deterministic out-of-crate convergence binary is not cleanly supported. For a moot: members discover each other, then LogSync converges the topic's associated logs.
- **Decisive: causal completeness is association-policy dependent, not an automatic DAG property.** LogSync converges the author-logs ASSOCIATED with a topic (`associate(topic, {author: [log_ids]})`) and treats the operation body / extensions (where `MereEvent.parents` live) as opaque. A cross-author `parents` ref resolves only if that author's log was also associated and synced under the topic. The native multi-parent DAG (substrate brief §4) gives causal completeness for free because the sync layer walks parents; per-author-log + opaque-parents does not. So: **keep `parents` as application-level data**, the **moot must associate every member log** so cross-member refs resolve, and refs outside the associated set need **app-level backfill**.
- **The S2 "is dropping the native DAG safe?" question, answered:** safe *if and only if* the moot's association policy covers every author whose events can be referenced. Within a bounded moot that is automatic; across moots or for refs to non-members it is not, and needs backfill. So `parents` stays in the event and association policy becomes a first-class moot concern, not a freebie the substrate hands us.
- **Side-finding feeding Probe 2:** a moot maps to a flat topic plus per-author logs. p2panda topics are flat BLAKE3 hashes; hierarchical graph-cluster namespaces do not map onto them. This reinforces that the spatial namespace + capability layer wants Willow-shaped paths or Mere-native addressing, not p2panda topics + per-author-log scoping.

### 2026-06-01 — Probe 2: Mere-native cluster-path capability (the spatial namespace layer)

Probe: [`crates/probes/willow-cluster-cap`](../../../crates/probes/willow-cluster-cap), runs green. Grounded in the real Willow data model + Meadowcap (willowprotocol.org specs).

- **The mapping is clean.** Willow's 3D model (namespace × subspace × path) maps onto Mere's graph-cluster-namespace vision (§14 of the substrate brief): namespace = moot, subspace = member/author, **path = the node's position in the graph's community decomposition** (Louvain/Leiden), e.g. `[3,7,1]` = cluster 3 / sub 7 / node 1. Meadowcap's granted area = (subspace restriction, path-prefix restriction, mode), with delegation that monotonically narrows the area and is signed by the prior holder.
- **Path-prefix containment is the spatial benefit, demonstrated.** A READ cap over area `(alice, [3])` lets Bob read every node in Alice's cluster-3 neighborhood (`[3,7,1]`, `[3,9,5]`) but not cluster 4 (`[4,1,1]`); Bob delegating a narrower `(alice, [3,7])` to Carol gives Carol just that sub-neighborhood (loses `[3,9,5]`); a widening delegation is rejected by the monotonic rule. **Sharing a cluster = sharing a coherent sub-graph neighborhood,** with sub-scoping and signed, narrowing delegation.
- **p2panda flat topics cannot express this.** A p2panda topic is one 32-byte hash: you are in the topic or not, no sub-scoping, no neighborhood structure, no path-prefix delegation. Probe 1's side-finding (moot ↔ flat topic + per-author logs) is confirmed: the spatial namespace + capability layer is precisely where p2panda is the weaker fit.
- **Owning a willow-esque interpretation is cheap and ours.** The probe (one file, ed25519 only) is the working core of a Mere-native Meadowcap-shaped cap: `Area`, path-prefix containment, signed monotonic delegation, `grants(entry)`. We do not need to adopt `willow25` wholesale to get the spatial benefit; we own the interpretation (and could still adopt `willow25` for the data model + sync later, behind the projection boundary). Per Mark: owning the interpretation is fine.
- **Net shape across both probes:** transport + discovery + bilateral/event sync → p2panda-net (+ core, behind the projection boundary); **spatial namespace + capability → Mere-native Meadowcap-shaped cluster-paths** (this probe), or willow25 if its data model earns adoption later. The moot's content truth is event-sourced (p2panda or Mere-native), addressed by cluster-paths, projected into the spatial graph view. p2panda stays a toolkit beneath the projection boundary; the spatial identity stays ours.

## Progress

### 2026-06-01

- Plan drafted from landscape-brief recommendation 2, with the iroh-transport refactor folded in as the same decision (Mark's question).
- Established facts recorded in §3 (license dual MIT/Apache; p2panda-core extension-driven; iroh 1.0 migration mechanical), so the spike does not re-litigate them.
- Hinge question isolated in §4 (per-author-log vs multi-parent DAG); S2 owns it.
- Four-position decision frame (P0..P3) with an explicit decision rule and a stated prior (P1 most likely), gate-driven, not pre-decided.
- DOC_README index updated same session.
- No code written. First action is recommendation 4 (multihash discipline, independent) and the S1/S2/S3 probes.

### 2026-06-01 — Phase 0 spikes run (see Findings)

- All five spikes run. S2 is empirical (a green probe at `crates/probes/p2panda-fit`, p2panda-core 0.6.1, standalone workspace, untouched main tree); S5/S1/S3/S4 are grounded in the 0.6.1 source and dependency facts.
- Headline: p2panda-core/-net/-auth/-encryption are all 0.6.1, dual MIT/Apache, on the same iroh 0.98 line we pin. The data model fits via extensions with a per-author-log + refs reframe. The adopt unit is net + core (LogSync couples them), and auth + encryption retire the pre-alpha Keyhive bet and the MLS build.
- Recommendation moved from the pre-spike P1 to **P2 leaning P3**. In that path the iroh work is a retirement of `IrohTransport` behind p2panda-net, not a 1.0 port. Decision now sits with Mark at the gate.
- Open empirical follow-ups before commit: a two-node `LogSync` round-trip of a MereEvent-as-Operation, and confirmation of `Endpoint` raw-stream access. Multihash discipline on `eidetic::schema::Hash` lands first regardless.

### 2026-06-01 — multihash discipline landed (recommendation 4, gate-independent)

- `eidetic::schema::Hash` is now multihash-aware (§9): a `HashFn` multicodec vocabulary (BLAKE3 `0x1e`/32B), self-describing `<fn>:<hex>` serde with lenient legacy-bare-hex read, and a binary `to_multihash_bytes` / `from_multihash_bytes` form (`uvarint(code) ++ uvarint(len) ++ digest`) for wire / event-DAG / cap-scope use. The public surface (`of` / `to_hex` / `Display` / `ManifestId`) is preserved; only the serialized digit form gained the `blake3:` tag.
- Verified: 69 eidetic-core tests green (including the consumer round-trips that embed `Hash`); eidetic-fjall builds; intel/embed uses the preserved `of`/`to_hex`/`ManifestId` surface; eidetic-iroh-fetcher uses transport's `BlobHash`, not this type.
- Remaining for full §13 compliance: generalize the in-memory digest beyond `[u8;32]` when a second hash function lands, and audit the other digest fields (`persona/identity`, `murmuring`, future event-DAG core, cap layer, `mere-namespace`). This was the only ungated task; the rest waits on the gate decision.

### 2026-06-01 — S1 low-level probe run (green)

- `crates/probes/p2panda-cable` stands up two `p2panda-net::Endpoint`s and round-trips the exact murm Cable snapshot framing over a custom ALPN, with `endpoint()` exposing the raw iroh endpoint for blobs. Confirms one endpoint can host Cable + blobs + p2panda sync, and that a `P2pandaTransport: Transport` adapter preserving Cable is viable and mechanical (connect → open_bi; accept → ProtocolHandler → channel, our existing pattern). See the matching Findings entry. Heavier dep tree noted (noq / iroh-relay / reqwest / igd / portmapper / netwatch / hickory / sqlx / ractor). Gate unchanged: P2 evaluation candidate, no retirement; high-level Node::stream substrate test + p2panda-spaces eval still owed.

### 2026-06-01 — decision reframed as toolkit-not-framework (§11)

- Per Mark: the crux is not Cable but whether p2panda is the right architectural decision, taken as a toolkit (like libp2p / iroh / Willow) without binding Mere to a p2panda-shaped vision. Added §11: p2panda is explicitly toolkit-built (modular, data-agnostic, own-sync-protocol-able); it is log/feed-shaped not document-shaped and sits below the spatial layer (the moot stays a derived projection per the two-natured kernel); the guardrail is the substrate brief's event-DAG-as-identity / sync-as-projection inversion; and the take is heterogeneous (p2panda-net for transport+discovery, p2panda-core/-encryption as donors, Willow/Meadowcap or Mere-native for the spatial namespace+cap layer where flat topics are the weaker fit). The gate is now per-layer behind the projection boundary, not "adopt the framework."

### 2026-06-01 — Probe 1 run (green setup; convergence per p2panda's own test)

- `crates/probes/p2panda-converge` reproduces the public LogSync setup path (spawn, author MereEvents-in-bodies with `parents`, associate, subscribe) green. Convergence itself is p2panda's tested guarantee (`e2e_log_sync`); the public API gates session-init behind discovery (`initiate_session` is `#[cfg(test)]`). Decisive finding: causal completeness for cross-author `parents` is association-policy-dependent, not automatic — keep `parents` app-level, the moot associates all member logs, backfill for out-of-set refs. Side-finding: flat topics do not fit graph-cluster namespaces, which sets up Probe 2.

### 2026-06-01 — Probe 2 run (green): Mere-native cluster-path capability

- `crates/probes/willow-cluster-cap` implements a Meadowcap-shaped capability over graph-cluster paths (ed25519 only) and runs green: path-prefix containment shares a coherent sub-graph neighborhood (cap over `[3]` covers `[3,7,1]`/`[3,9,5]` not `[4,1,1]`), signed delegation narrows it (`[3,7]` drops `[3,9,5]`), widening is rejected. Confirms the spatial namespace + capability layer wants this Willow-esque shape, which p2panda's flat topics cannot express. Per Mark, owning the interpretation is fine; this probe is its core. Net shape: p2panda-net for transport/discovery (+ core behind the boundary), Mere-native cluster-path caps for the spatial layer.

## Sources (external, verified 2026-06-01)

- p2panda-core / p2panda-net license (dual MIT/Apache): <https://crates.io/crates/p2panda-core>, <https://crates.io/crates/p2panda-net>
- p2panda-core Operation/Header + Extension trait: <https://docs.rs/p2panda-core>, <https://docs.rs/p2panda-core/latest/p2panda_core/extensions/trait.Extension.html>
- iroh 1.0 migration (Endpoint takeover, relay/connection/0-RTT changes): <https://www.iroh.computer/blog/iroh-0-94-0-the-endpoint-takeover>, <https://www.iroh.computer/blog/iroh-0-95-0-new-relay>
