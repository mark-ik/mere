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

Either path lands on iroh 1.0; the difference is whether we hold the iroh handle or p2panda-net does.

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

## Findings

(None yet. Populate from S1 to S5 as the probes run.)

## Progress

### 2026-06-01

- Plan drafted from landscape-brief recommendation 2, with the iroh-transport refactor folded in as the same decision (Mark's question).
- Established facts recorded in §3 (license dual MIT/Apache; p2panda-core extension-driven; iroh 1.0 migration mechanical), so the spike does not re-litigate them.
- Hinge question isolated in §4 (per-author-log vs multi-parent DAG); S2 owns it.
- Four-position decision frame (P0..P3) with an explicit decision rule and a stated prior (P1 most likely), gate-driven, not pre-decided.
- DOC_README index updated same session.
- No code written. First action is recommendation 4 (multihash discipline, independent) and the S1/S2/S3 probes.

## Sources (external, verified 2026-06-01)

- p2panda-core / p2panda-net license (dual MIT/Apache): <https://crates.io/crates/p2panda-core>, <https://crates.io/crates/p2panda-net>
- p2panda-core Operation/Header + Extension trait: <https://docs.rs/p2panda-core>, <https://docs.rs/p2panda-core/latest/p2panda_core/extensions/trait.Extension.html>
- iroh 1.0 migration (Endpoint takeover, relay/connection/0-RTT changes): <https://www.iroh.computer/blog/iroh-0-94-0-the-endpoint-takeover>, <https://www.iroh.computer/blog/iroh-0-95-0-new-relay>
