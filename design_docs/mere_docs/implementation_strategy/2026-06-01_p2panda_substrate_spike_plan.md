# p2panda substrate spike (adopt-vs-build), and the iroh-transport refactor it decides

**Date**: 2026-06-01
**Status**: Decision made and executed (Mark greenlit). Adopt p2panda-net (transport/discovery) + p2panda-core (operations) for the wire; p2panda-encryption is the watch/adopt engine for E2EE (not the spaces bundle). Live: P2pandaTransport (IrohTransport retired), murm-on-operations with signed `cabal_id`, BLAKE3 unified, mDNS + random-walk discovery, the encryption-fit probe (caps + data + forward-secret schemes green). Follow-on builds spun out to dedicated plans: [sync-as-projection / LogSync](2026-06-02_logsync_sync_as_projection_plan.md) and [tessera](../../moothold_docs/implementation_strategy/2026-06-02_tessera_plan.md). This doc is the decision record + probe log; not yet archived because both follow-on plans reference it.
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

## 12. How Cable lives in this picture

Mark (2026-06-01): how does Cable live here? Resolved by separating three layers Cable conflated.

- **Trust model (the keeper).** A cabal is defined by a shared secret (`cabal_key`): anyone with it is in, symmetric, no roster, no capability issuance, no server; the per-cabal Ed25519 is derived from it (murm already does this). This is genuinely distinct from the moot's model (Meadowcap cluster-path caps, author-owned subspaces, rostered, asymmetric). So the sharpest murm-vs-moot axis is **shared-secret symmetric (cabal) vs capability-based asymmetric (moot)**, not "1:1 vs many" or "ad-hoc vs durable." Cable is the canonical shared-secret lane: bilateral DMs, co-op sessions, invite-by-secret small groups.
- **As a MereSpace.** A cabal is `SpaceId::Cabal(cabal_key)` carrying signed events. The Probe 1 causal-completeness constraint lands softly here: a cabal's members ARE everyone-with-the-key, so the space associates all member logs by construction and cross-member refs resolve automatically. Cable is the *easy* case for the substrate, not the hard one.
- **Wire (interop-only).** Cable's literal wire (BLAKE2b posts, cable.club spec, channel time-range sync) is structurally a cousin of MereEvent / p2panda-core (signed per-author posts, causal links, channel sync), so it is redundant with the substrate unless cabal.club client interop is wanted. If interop matters: keep Cable as a literal protocol module in murmuring (BLAKE2b preserved), one bilateral protocol among others, an interop bridge (same status as Matrix/Nostr/Misfin, consistent with "smolweb exchange protocols can live in murm"). If not: run MereEvents / p2panda-core operations over the cabal trust model (BLAKE3, unified), no separate wire.
- **Transport.** Either way Cable rides `p2panda-net::Endpoint` on `mere/cable/v1` (Probe S1 proved the exact snapshot framing round-trips), sharing one endpoint with blobs, gossip, and moot sync. It does not need the hand-rolled router.

**This reconciles the standing contradiction.** The substrate brief's "drop Cable" meant drop the *wire as the protocol identity*; the moot-tiers "keep Cable" meant keep the *bilateral lane*. Both hold: keep the trust model as the bilateral lane, the wire is interop-only, transport is p2panda-net. The murm/moothold boundary sharpens to the trust model: murm = shared-secret/bilateral (cabals, DMs, co-op, future MLS), moothold = capability/rostered/spatial (cluster-path caps, moots).

**Resolved 2026-06-01 (Mark): no cabal.club interop; model cabals natively, BLAKE3 only.** The open product question is answered: Mere does not pursue cabal.club client interop, so the literal Cable wire's reason-to-exist (BLAKE2b posts, Cable-spec salt/personalization) is gone. **BLAKE2b is removed from the code:** identity derivation is now `BLAKE3-keyed(master_seed, salt)`; murm post ids and cabal ids are plain BLAKE3-256 (the Cable salt/personalization params dropped). 19 identity + 79 murmuring/murm tests pass on BLAKE3. A cabal is a shared-secret-keyed BLAKE3 space. Also decided: **no WILLIAM3** — stay pure BLAKE3 everywhere, which means the spatial namespace layer is the Mere-native BLAKE3 cluster-path cap (Probe 2), not willow25's Bab/WILLIAM3 payload digests. This closes landscape-brief contradiction (B) and the substrate brief's hash-unification gap.

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

### 2026-06-01 — First build slice: P2pandaTransport adapter validated with real Murm/Cable

Probe: [`crates/probes/p2panda-transport-adapter`](../../../crates/probes/p2panda-transport-adapter), runs green.

- **`P2pandaTransport: transport::Transport` is implemented over `p2panda-net::Endpoint`** in one file: a `P2pandaStream` (AsyncRead+AsyncWrite over `Connection::open_bi`/`accept_bi`), a per-ALPN accept queue fed by an iroh `ProtocolHandler` (the `IrohTransport` pattern), `bind` from the Mere master key (bridged to p2panda's `SigningKey` via the raw ed25519 seed), `connect`, `accept`, and loopback `node_info`/`add_peer`.
- **The unchanged `murm` + Cable code runs over it.** The probe path-depends on the *real* `transport`, `murm`, `identity` crates and runs `Murm<P2pandaTransport>`: Alice opens a cabal, authors 3 posts, `push_cabal_to_peer`; Bob `accept_cable_connection` ingests all 3; signatures verify. No murm or Cable change.
- **Cross-workspace path-deps and iroh unification work in practice.** The probe (own workspace) path-deps the real workspace crates; iroh resolves to 0.98.x satisfying both `transport` (iroh 0.98 / iroh-blobs 0.100 / iroh-gossip 0.98) and p2panda-net (iroh 0.98.2). The S5 no-skew finding holds when actually compiled together.
- **Conclusion:** "replace the router, keep the behavior" is not just feasible (S1) but *mechanical*. P2pandaTransport is ~200 lines mirroring `IrohTransport` on `p2panda-net::Endpoint`; the `Transport` trait is the clean seam, and the murm side is untouched.

**The live-crate wiring is the gated next step** (needs Mark's sign-off): move `P2pandaTransport` into the `transport` crate (adding `p2panda-net` as a dep, with the heavy dep tree noted in S1), expose discovery + `node_info`/`add_peer`, and either keep `IrohTransport` as a fallback or retire it. That is the dependency swap the plan flagged as gated; the probe shows it is safe to make.

### 2026-06-01 — Live-crate wiring landed: P2pandaTransport is the transport, IrohTransport retired

Per Mark ("go for it ... prefer the best implementation over the inertial one"), the adapter moved from probe into the live `transport` crate, and the hand-rolled iroh Router retired.

- **`transport::P2pandaTransport` is now the production `Transport`** ([`crates/murm/transport/src/p2panda_transport.rs`](../../../crates/murm/transport/src/p2panda_transport.rs)): `bind` / `bind_with_blobs` / `builder`, `connect`, `accept` (per-ALPN queue via p2panda-net `Endpoint::accept`), `connect_raw`, `endpoint_addr` / `add_peer` (loopback), and it serves iroh-blobs off the same endpoint. p2panda-net is the endpoint authority, so discovery (`Discovery`/`MdnsDiscovery`), relay/hole-punching, NAT port-mapping, and actor supervision come for free.
- **`IrohTransport` (the hand-rolled iroh `Router` + `MemoryLookup` + per-ALPN queues + gossip wiring) is deleted.** `iroh-gossip` (only IrohTransport used it) is dropped from the transport deps; p2panda-net provides gossip when needed.
- **Consumers migrated, all green:** `blobs::BlobStore::fetch_from` takes `&P2pandaTransport`; the murm Cable-over-transport test (`cable_snapshot_sync_via_p2panda_transport`) and the `eidetic-iroh-fetcher` blob-fetch tests run over P2pandaTransport. transport 23 tests, plus the murm + eidetic suites, pass.
- **Dep-surface change (accepted):** the live `transport` crate now pulls p2panda-net's tree (noq, sqlx/SQLite, ractor, hickory DNS, reqwest, igd/portmapper). This affects crates that depend on `transport` (murm, eidetic-iroh-fetcher), not the dormant `mere-app` live path.
- **What remains:** wire p2panda-net discovery into a production path (today the transport uses explicit `add_peer` loopback, as IrohTransport did); plus the bilateral lane's move to a MereEvent space and the `p2panda-spaces` eval, both independent. The transport-layer router replacement is done.

### 2026-06-01 — Threads 1-3: discovery wired, cabal-as-space proven, p2panda-spaces evaluated

**Thread 1 (discovery wired, code landed).** `P2pandaTransport::builder().mdns(MdnsDiscoveryMode)` spawns p2panda-net's mDNS discovery off the same endpoint, so LAN peers auto-populate the address book (no explicit `add_peer`); the handle is held on the transport. transport at 24 tests (a smoke test confirms the service spawns; end-to-end LAN discovery is multi-host, not single-machine-deterministic). Random-walk `Discovery` (internet, needs bootstrap + relay config) is the follow-on.

**Thread 2 (cabal-as-BLAKE3-space, proven).** Probe [`crates/probes/cabal-space`](../../../crates/probes/cabal-space), green. A cabal is modeled as a BLAKE3 signed-event space on our stack, realizing §12: a shared secret `cabal_key` defines membership (symmetric, no roster); space id = `BLAKE3(cabal_key)`; per-member per-cabal signing key = `BLAKE3-keyed(member_master, cabal_key)` (the Cable §2.2 pattern, BLAKE3); events are p2panda-core Operations carrying a `CabalExt` extension (cabal_id, channel, event_type, parents). CBOR round-trips, Ed25519 signatures verify, multi-parent refs ride as app-level data (per Probe 1), and a different secret yields a different space (no impersonation). This is "Cable on our stack": the bilateral shared-secret trust model, BLAKE3 throughout, no literal Cable wire. Promoting it into murm (retiring the Cable wire) is the follow-on refactor.

**Thread 3 (p2panda-spaces eval, source-based).** `p2panda-spaces` is github-only (unpublished; 404 on crates.io and docs.rs), pre-v1, APIs unstable. It establishes encryption contexts for dynamic groups across devices, built on p2panda-auth (group coordination via a conflict-resistant structure) + p2panda-encryption (key agreement): forward secrecy, post-compromise security, and concurrent/offline membership conflict resolution; nested groups model multi-device profiles. Its **data-model precondition is exactly our model** — messages causally ordered + cryptographically signed, which p2panda-core Operations and the Thread 2 cabal-space already satisfy. So:

- p2panda-spaces fills §8.8's "live group/key state" slot (the **Keyhive** comparison) *and* the planned **MLS** E2EE module (consistent with S4).
- vs Keyhive: p2panda-spaces is the **lower-friction** candidate *given the adopt lean* — no translation layer (Keyhive is Automerge/Beelay-shaped and would need one), and it co-evolves with the net/core stack. Both are early (p2panda-spaces pre-v1; Keyhive pre-alpha/unaudited), so neither is adopt-now.
- **Stance:** p2panda-spaces becomes the preferred **watch/eval** candidate for the group/key + E2EE slot, over Keyhive, if Mere is on the p2panda stack. Revisit at its v1; a git-dep runnable eval is the next step when adoption is seriously considered.
- **Synthesis:** the Thread 2 cabal-space (and the moot's cluster-path-capability spaces) already meet p2panda-spaces' preconditions, so E2EE group encryption layers cleanly on top of the event model we built, when it matures.

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

### 2026-06-01 — BLAKE2b removed, BLAKE3 unified (code landed)

- Per Mark ("Blake2b can go ... stick to blake3, not blake2b or william3"): swapped identity derivation to `BLAKE3-keyed(master_seed, salt)` (`persona/identity/keypair.rs`, blake2 dep dropped) and murm post-id + cabal-id hashing to plain BLAKE3-256 (`murm/murmuring/cable/hash.rs`, Cable-spec salt/personalization + blake2/blake2b_simd deps dropped). 19 identity + 79 murmuring/murm tests green. No BLAKE2b remains in the code. Decisions recorded in §12: no cabal.club interop (cabals modeled natively, BLAKE3), and no WILLIAM3 (spatial layer = Mere-native BLAKE3 cluster-path cap, not willow25's Bab digests). Resolves landscape-brief contradiction (B).

### 2026-06-01 — P2pandaTransport adapter slice (green)

- `crates/probes/p2panda-transport-adapter` implements `P2pandaTransport: transport::Transport` over `p2panda-net::Endpoint` and runs the REAL `Murm` Cable snapshot sync over it (two nodes, loopback): 3 posts pushed + ingested, signatures verify. Cross-workspace path-deps to the real transport/murm/identity crates compile; iroh 0.98 unifies. Proves the router-replacement is mechanical via the `Transport` trait (murm side untouched). The live-crate dependency swap (p2panda-net into the `transport` crate, retire the iroh Router) remains gated on Mark.

### 2026-06-01 — IrohTransport retired; P2pandaTransport is the live transport

- Moved P2pandaTransport from probe into the live `transport` crate as the production `Transport` (bind/bind_with_blobs/connect/accept/connect_raw/endpoint_addr/add_peer + iroh-blobs serving). Deleted `iroh_transport.rs` and dropped the `iroh-gossip` dep. Migrated `blobs::fetch_from`, the murm Cable test, and `eidetic-iroh-fetcher` to P2pandaTransport. transport (23) + murm + eidetic tests green. The hand-rolled iroh Router is gone; p2panda-net's `Endpoint` is the endpoint authority. Remaining: production discovery wiring, the bilateral-lane-on-MereEvent move, and the p2panda-spaces eval.

### 2026-06-01 — Threads 1-3 done

- **Thread 1:** mDNS discovery wired into `P2pandaTransport` (`builder().mdns(..)` spawns + holds the service); transport at 24 tests. Random-walk Discovery (needs bootstrap+relay) is the follow-on.
- **Thread 2:** `crates/probes/cabal-space` (green) models a cabal as a BLAKE3 signed-event space (shared secret → cabal_id, per-member BLAKE3-keyed signing, events as p2panda-core Operations + CabalExt). "Cable on our stack," no literal wire; promoting into murm is the follow-on refactor.
- **Thread 3:** p2panda-spaces evaluated from source (github-only, pre-v1). It fills the §8.8 group/key-state slot (and the MLS E2EE module), is lower-friction than Keyhive given the adopt lean (its causal-order+signed precondition is exactly our event model, no translation layer), and the Thread 2 cabal-space already satisfies its preconditions. Stance: preferred watch/eval candidate for the slot; revisit at v1.

### 2026-06-01 — Cable wire retired in murm; random-walk discovery wired

Two follow-ons from Threads 1-2 landed in the live crates.

- **Cable wire retired; a murm post IS a p2panda operation.** Rewrote `murmuring`'s `cable::wire` and `cable::sign` so a `Post` encodes to / decodes from a p2panda-core `Operation` (a signed `Header<CabalExt>` + body): BLAKE3, CBOR, Ed25519 via `Header::{sign,verify}`. The `CabalExt` header extension carries `{channel, post_type, timestamp_ms, parents, info, deletes}`; text/topic ride in the body (bound into the signature via `payload_hash`). Identity bridges through `Ed25519Keypair::to_seed()` → `p2panda_core::SigningKey` (both ed25519-dalek). Deleted `cable/varint.rs` (the LEB128 framing) and dropped the byte-layout/varint tests. The `Post`/`PostKind`/`CableEngine`/store/`murm` APIs are unchanged — only the wire/crypto layer moved — so the swap is behavior-preserving. **murmuring 63 + murm 13 green**, including the cross-crate `cable_snapshot_sync_via_p2panda_transport` round-trip over the real transport. This is the §12 "Cable on our stack" promotion realized in the live path; the cabal-space probe was the proof, this is the production code. Follow-on still open: use p2panda's native per-author `seq_num`/`backlink` log instead of app-level `parents`.
- **`cabal_id` is now in the signed event (self-describing posts).** Added `cabal_id: [u8; 32]` to `CabalExt` (so it is signed) and to `Post`; `sign_post` takes the cabal id; `verify_post` reconstructs and checks it. Because the cabal id is signed, a post cannot be replayed into a different cabal: tampering it fails verification, and `CableEngine::ingest_post` now rejects a post whose `cabal_id` differs from the cabal it is ingested into (new `MurmuringError::CabalMismatch`). The cabal id was previously only the store key. **murmuring 66 + murm 13 green** (added tampered-cabal-id, different-cabals-differ-in-signature, and foreign-cabal-ingest-rejected tests).
- **Random-walk discovery wired into `P2pandaTransport`.** `builder().discovery()` / `.discovery_config(DiscoveryConfig)` spawns `p2panda_net::Discovery` off the same endpoint; walkers explore outward from the bootstrap nodes already in the address book (added via `add_peer` or surfaced by mDNS). The handle is held for the transport's life (dropping it stops the walkers). mDNS (LAN) and random-walk (internet) coexist on one endpoint. **transport at 26 tests** (added the random-walk smoke test and a combined mDNS+random-walk test). End-to-end internet discovery still needs a populated bootstrap set + reachable peers (a deployment concern); the tests verify the services spawn and stay alive.

### 2026-06-01 — p2panda-spaces: take the encryption engine, not the bundle

Verified (docs.rs/p2panda-encryption, issue #774): **p2panda-encryption is a standalone crate with pluggable authorization.** Its `traits` module exists to plug in "custom data- and messaging types, group management- and ordering strategies"; the caller supplies the member set and key bundles. It carries no built-in access-control model. `p2panda-spaces` is the (git-only, `spaces`-branch) bundle that layers `p2panda-auth` (an authorization CRDT) on top of it.

**Decision input:** if Mere stays on the p2panda stack for the encrypted-group slot, take **p2panda-encryption** (the crypto engine) and supply our own policy layers, rather than adopting the full `p2panda-spaces` bundle. `p2panda-auth` would be a second authorization model competing with the cluster-path capabilities we already build (Probe 2), so we skip it. This reframes the Thread 3 stance: the watch/eval target narrows from the bundle to the engine.

**Layer map (who owns what):**

- **Trust / admission** (who is allowed in): **tessera** (the trust receipt). p2panda-encryption has no opinion on who deserves a key bundle; this is the layer it deliberately leaves empty.
- **Capability / permission** (what a member may do; role changes): **our cluster-path capabilities** (meadowcap-shaped, BLAKE3, Probe 2; see the event-DAG brief §8.8 capability stack and the 2026-05-10 cluster-namespaces brief).
- **Membership → keys** (group key agreement, ratchets, encrypt/decrypt): **p2panda-encryption** (data scheme + message scheme), behind a Mere-side trait.
- **Ordering** (causal dependencies feeding the above): **our event-DAG** (the operations murm now rides on), through p2panda-encryption's ordering trait.
- **Key / pre-key distribution**: **identity vault + the address book / discovery** (mDNS + random-walk, now wired).
- **Metadata privacy**: **Veilid backend** (per-moot policy) + cluster-path scoping.

**Gaps that dissolve under this split** (they were `p2panda-auth`/spaces gaps, not ours):

- Role demote/promote (a spaces TODO) becomes an ordinary capability grant/revoke.
- "Strong removal" and concurrent-membership resolution become our caps' semantics.
- `AuthoredMessage` trait churn and serde-for-auth-state sit in the layer we skip; our seam is p2panda-encryption's `traits` module, pluggable by design.
- "Declare app-level message dependencies" (a spaces TODO) is our DAG's `parents`, fed through the ordering trait.
- Metadata-privacy-vs-sync (upstream undecided, Willow vs Beelay) has a Mere position already (Veilid + cluster-path scoping).
- Pre-key / PKI distribution (upstream "unexplored") has a plausible home in our identity + discovery substrate.

**Gaps that remain (crypto-intrinsic, not erased by owning the upper layers):**

- ~128-member ceiling (DCGKA, not TreeKEM). Mitigated by topology, not cryptography: federation shards a community into many smaller spaces (moot → moothold → demesne), so one huge encrypted group is rarely the shape. Mitigated, not fixed.
- Post-compromise healing delay offline (healed only once the last member hears the removal). Faster discovery helps at the margin.
- Strong forward secrecy vs local-first (use-a-pre-key-exactly-once). Fundamental; choose per lane (data scheme where late joiners should read history, message scheme where they must not).

**Other solutions considered for the crypto slot:**

- **Keyhive / Beehive** (Ink & Switch): TreeKEM, so `O(log n)` and better at scale, but Automerge/Beelay-shaped and ships its own capability model that competes with our caps; pre-alpha, unaudited. Scales better, couples harder.
- **OpenMLS / MLS proper**: mature TreeKEM, but assumes a Delivery Service for ordering; adapting it to a DAG is the work p2panda-encryption already did.
- **Server-centric** (Matrix megolm, Signal sender-keys): mature but not local-first.
- Net: for our constraints the real field is p2panda-encryption vs Keyhive, both early; p2panda-encryption is the lower-friction one precisely because its authorization is pluggable, so tessera and our caps slot in.

**Cost of this path:** we own the glue p2panda-spaces would have written (cap decision → member set, DAG → ordering strategy, key-bundle distribution). More integration work, but in our domain, and it avoids reconciling a second authorization model.

**Probe retarget:** the Thread 3 follow-on eval should target **p2panda-encryption's `traits` seam**, not the spaces bundle. A `crates/probes/p2panda-encryption-fit` that (1) feeds a murm/cabal operation as the authored + ordered message, (2) supplies a member set from a stub cluster-path capability, and (3) drives one data-scheme and one message-scheme round trip. Confirms the seam is real against current code, quarantined behind a Mere-side trait.

### 2026-06-01 — Probe result: p2panda-encryption-fit (green, data scheme)

Built [`crates/probes/p2panda-encryption-fit`](../../../crates/probes/p2panda-encryption-fit) against the published `p2panda-encryption` 0.6.1 (the engine **is** on crates.io; only the spaces bundle is git-only). Runs green. The "engine, not the bundle" decision is workable today:

- **Pluggable authorization is real.** Implemented `GroupMembership` (DGM) as `CapDgm`, fed by a stub `cap_members` standing in for cluster-path caps + tessera. p2panda-encryption decided nothing about membership; we did.
- **Our event is the payload.** Encrypted a real signed `Header<CabalExt>` operation (a murm post on the wire); after the DCGKA-established `GroupSecret` round trip Alice → Bob it decoded and `verify()`'d with `cabal_id` and author intact. A non-member (no secret) could not open it.
- **Quarantine boundary works.** The whole flow sits behind a Mere-side `EncryptedSpace` trait; the rest of Mere never touches p2panda-encryption directly.

**New findings (integration friction, not surfaced in the upstream docs):**

- **Data scheme is low-friction; the message scheme is not.** `encrypt_data`/`decrypt_data` take a `GroupSecret` you hold directly, so a thin DGM is the only trait you must supply (proven green). The message scheme (forward secrecy) has **no external shortcut**: its ratchet seed type `Secret<32>` has only `pub(crate)` constructors, so you cannot drive the low-level ratchet yourself — you must run the full `MessageGroup` with a `ForwardSecureOrdering` state machine (queue / dependency resolution / welcome handling) + `AckedGroupMembership`. That ordering layer is the heavy integration piece. **Data scheme is the adoption path; the message scheme's FS is deferred behind that ordering work.** This sharpens the earlier "gaps that remain" note: the FS-vs-local-first cost shows up concretely as "you must own the ordering state machine."
- **The published examples lean on `test_utils`.** `Rng::from_seed`, `KeyManager::init_and_generate_prekey`, `SecretKey::from_bytes`, and `Secret::from_bytes` are all test- or crate-internal-gated. The public setup path is `Rng::default()` + `KeyManager::init` + `PreKeyManager::rotate_prekey` + `prekey_bundle`. Minor, but a first integrator hits it immediately.
- **Two key kinds.** Cabal operations are Ed25519-signed (p2panda-core); the encryption identity is X25519 (p2panda-encryption X3DH). Both derive from the identity vault in Mere; distinct keys for distinct jobs.

**Net:** the data-scheme seam is proven against real code, and the residual cost is exactly what the decomposition predicted — we own the glue (DGM from caps, key-bundle distribution, and, for FS, the ordering state machine). Next step when adoption is greenlit: wire the DGM to real cluster-path caps (drop the stub), and implement `ForwardSecureOrdering` over our event-DAG to unlock the message scheme's forward secrecy.

#### 2026-06-01 — both follow-ups landed (caps + forward secrecy)

- **Real cluster-path caps (drop the stub).** The data-scheme bin now derives membership from signed cluster-path capabilities ported from Probe 2 (`Capability`/`Area` over p2panda-core's ed25519): the moot owner roots a Read cap per member over the space's cluster-path, `cap_members` admits the holders of valid caps covering that path, and an outsider holding no cap is rejected. p2panda still decides nothing about membership. Green.
- **Forward-secret message scheme (`src/bin/message_scheme.rs`).** Implemented the full trait stack ourselves: a minimal `AckedGroupMembership`, a `ForwardSecureOrdering` driven by our event-DAG's causal `previous`/parent links (ready when parents are processed; FIFO since the probe delivers in order), and a `ForwardSecureGroupMessage` envelope. Drove `MessageGroup` create → receive(welcome) → send → receive; Bob decrypted Alice's signed cabal operation under the message ratchet (269 bytes) and it still verifies. Green.
- **Confirmed adoption costs (now from working code, not inference):** the message scheme needs one-time pre-key bundles (`generate_onetime_bundle` after `rotate_prekey`), and the integrator must bring `AckedGroupMembership` + `ForwardSecureOrdering` + a message type — upstream ships these only as test-only/crate-private code. The data scheme remains the low-friction path; the FS path is real integration work, warranted for sensitive lanes (e.g. private moot threads). Our event-DAG slots cleanly into the ordering trait, which is the load-bearing confirmation.

## Synthesis: the spike vs murm + moothold goals (2026-06-02)

Read against the [moot tiers brief](2026-05-07_moot_tiers_and_voluntary_hosting_brief.md) §10 and the [substrate brief](2026-05-07_event_dag_substrate_brief.md) §8.8, the spike landed two-thirds of moothold's capability stack and reframed the third:

| §8.8 slot | Intent | Status |
|---|---|---|
| Structural namespace caps | meadowcap-shaped "which cluster/path you may touch" | **Proven** — cluster-path caps (Probe 2), now gating encryption membership (Follow-up A) |
| Group / key-state | was "Keyhive eval" | **Reframed + proven** — p2panda-encryption (engine, not the spaces bundle); caps supply auth, event-DAG supplies ordering |
| Moot policy authorization | "Biscuit candidate": tessera / quota / heartbeat / role facts | **Open** — this is where tessera lives |

**Can do (proven, green):** one signed-operation wire for all event kinds (murm posts are p2panda operations, self-describing via signed `cabal_id`); real QUIC transport + mDNS + random-walk discovery; membership from signed cluster-path caps; E2EE with our authorization + our event-DAG ordering plugged in (data scheme durable, message scheme forward-secret).

**Should do (gaps to the goals):** **sync-as-projection** (the event-DAG isn't replicated between peers yet — we have transport + the operation type, but p2panda-net Gossip/LogSync isn't wired; the spike used explicit push/accept); the **graph-view CRDT**; **tessera** + the policy layer; the pin/reciprocity **ledgers**; **tier transitions** + forking; **mooting** thin clients.

**Sidequests:** production `ForwardSecureOrdering` (ours is FIFO-over-DAG, correct for in-order only); misfin/smolweb in murm; Veilid privacy backend; p2panda-spaces v1 re-eval.

**Pitfalls:** the ~128-member encryption ceiling versus the 5K-10K moot ceiling (E2EE is for small private lanes, never the moot-wide view); forward secrecy fights durability (see contradictions); one-time-prekey distribution is ours to solve; tessera is load-bearing yet unbuilt.

**Contradictions (resolve deliberately):** FS drops keys after decrypt, which is incompatible with lapse-and-revive + durable archives, so FS is for ephemeral lanes only and the data scheme (durable group key) is for anything that must survive a lapse — the two encryption modes map onto the two content lifecycles. E2EE versus cheesecloth "any pinner serves any blob" resolves into a synergy: pinners serve ciphertext for availability, members decrypt; the data scheme's late-joiner-reads-history property lets a new member decrypt pinned history. Key-state maintenance (rekey on membership change) is itself a hosting commitment and needs heartbeats like pinning.

**Synergies:** the event-DAG is the spine and the ordering seam is the proof — murm posts, governance, hosting commitments, stake+agreement, cap grants are all signed operations on one substrate, and the spike proved our DAG plugs into the ordering trait that the graph-view CRDT and ledgers will reuse. One cap system gates read-access, crypto membership, and (with tessera facts) hosting scope. Discovery + LogSync + iroh-blobs/cheesecloth already align with the federation shape. One identity root derives Ed25519 (events/caps), X25519 (encryption), and the persona/tessera chain.

**Strategic read:** the substrate question is settled in shape — one signed-operation event-DAG, synced over p2panda-net, gated by cluster-path caps, optionally encrypted per lane, with tessera as the missing policy layer the whole federation design waits on. The two highest-leverage next builds are **sync-as-projection** and **tessera**, planned in [`2026-06-02_logsync_sync_as_projection_plan.md`](2026-06-02_logsync_sync_as_projection_plan.md) and [`../../moothold_docs/implementation_strategy/2026-06-02_tessera_plan.md`](../../moothold_docs/implementation_strategy/2026-06-02_tessera_plan.md).

## Sources (external, verified 2026-06-01)

- p2panda-core / p2panda-net license (dual MIT/Apache): <https://crates.io/crates/p2panda-core>, <https://crates.io/crates/p2panda-net>
- p2panda-core Operation/Header + Extension trait: <https://docs.rs/p2panda-core>, <https://docs.rs/p2panda-core/latest/p2panda_core/extensions/trait.Extension.html>
- iroh 1.0 migration (Endpoint takeover, relay/connection/0-RTT changes): <https://www.iroh.computer/blog/iroh-0-94-0-the-endpoint-takeover>, <https://www.iroh.computer/blog/iroh-0-95-0-new-relay>
- p2panda-spaces status / TODO (git-only, `spaces` branch): <https://github.com/p2panda/p2panda/issues/774>
- p2panda-encryption (standalone crate, pluggable authorization, data + message schemes): <https://docs.rs/p2panda-encryption>, <https://crates.io/crates/p2panda-encryption>
- p2panda group encryption design + limitations (DCGKA ~128 members, FS/PCS trade-offs, metadata-vs-sync): <https://p2panda.org/2025/02/24/group-encryption.html>
- p2panda convergent access-control CRDT (causal-DAG + signed preconditions): <https://p2panda.org/2025/08/27/notes-convergent-access-control-crdt.html>
