# Unified Resource Banking

**Date**: 2026-06-03
**Status**: Proposal (design probe)
**Scope**: One model for decentralized storage *and* compute in Mere, banked
through the existing tessera economy. Lands a concrete mechanism per open
problem (verifiable compute, storage durability/proofs, the unified credit
ledger, anti-abuse) rather than a survey. The spine is a **bounty /
service-request** primitive that makes storage and compute the same operation
at different cadences. Grounded in an adversarially-verified research pass
(2026-06-03, 25 sources, 22 of 25 claims confirmed, 3 killed); external claims
are cited inline with the surviving confidence.
**Related**:

- [`2026-06-03_compute_mesh_brief.md`](2026-06-03_compute_mesh_brief.md) — the inner ring. The personal compute mesh (orrery-as-cluster) + the trust-graduated rings; this banking brief is its outer shell, engaging only where trust runs out. Read them as one model.
- [`../implementation_strategy/2026-05-07_moot_tiers_and_voluntary_hosting_brief.md`](../implementation_strategy/2026-05-07_moot_tiers_and_voluntary_hosting_brief.md) — tier framework, cheesecloth pinning, ILL reciprocity, voluntary hosting + reputational stakes, stake+agreement. This brief is the resource-economy layer those tiers spend.
- [`../../moothold_docs/implementation_strategy/2026-06-02_tessera_plan.md`](../../moothold_docs/implementation_strategy/2026-06-02_tessera_plan.md) — the built reputation ledger (Phases 1-5, 60 tests, two-peer convergence) + the `reciprocity` sibling ledger. Resource banking extends this grammar; it does not replace it.
- [`2026-05-10_geist_models_brief.md`](2026-05-10_geist_models_brief.md) — compute-as-utility (LoRA train/inference), compute-offer/ask/receipt, trainer/evaluator separation, canary/poisoning gates. This brief reframes that marketplace as bounty-shaped and supplies the missing verification + valve mechanics.
- [`../implementation_strategy/2026-05-07_event_dag_substrate_brief.md`](../implementation_strategy/2026-05-07_event_dag_substrate_brief.md) — the signed event-DAG (BLAKE3 + Ed25519 + CBOR) all banking events ride; §8.7/§8.9 persona-chain Sybil posture; the §8.8 cap stack the verifier gate sits beside.
- [`../implementation_strategy/2026-06-03_actor_constellation_plan.md`](../implementation_strategy/2026-06-03_actor_constellation_plan.md) — a decentralized compute bounty is the network-scale form of a compute-actor request; the armillary message taxonomy is the same request-to-result shape with a wider recipient.

---

## 0. The one idea

Resource banking is a **bounty**. An asker defines a *result* and escrows
credits; whoever produces something that satisfies the result spec is paid in
credits; reliable fulfilment also earns tessera. Storage and compute are the
same operation at two cadences: a **compute bounty** is one-shot (deliver an
artifact that passes spec, paid once), a **storage bounty** is streaming (keep
this available, audited continuously, paid as a stream). Because results are
content-addressed, a bounty whose answer already exists collapses into a pin: a
compute cache-hit degrades to a storage fetch for free.

This inverts the verification problem in our favour. An offer marketplace makes
you trust a *provider* and then audit their labour. A bounty makes you judge the
*artifact* against acceptance criteria. The bounty spec **is** the eval, so the
geist brief's `EvalReport` + canaries + poisoning probes stop being an
afterthought and become the contract.

---

## 1. Two ledgers, one valve

Resource banking needs a spendable unit; tessera must stay unbuyable. Keep them
separate.

- **Reciprocity credits** are the spendable bank. Directed and per-counterparty
  (A's balance *with* B lives in the B-to-A relationship), which is the existing
  `tessera::reciprocity` shape generalized down from moot-to-moot to
  member-to-moot. There is **one credit unit** for both resources; the exchange
  rate between durable-bytes-over-time and compute-cycles is whatever askers
  escrow and providers accept, per moot, never a protocol-fixed price. A
  market-cleared rate avoids reintroducing a central valuation.
- **Tessera** is standing. It gates participation and carries governance weight.
  It accrues only through `CommitmentFulfilled` (you pledged, you delivered, on
  heartbeat), and it decays.

**The valve.** Credits buy resources; they never buy standing. This holds
structurally for the reason [GNUnet's excess-based economy](https://grothoff.org/christian/ebe.pdf)
holds (confirmed 3-0): *no node owns its trust; the trust a node earns is stored
at the nodes it helped, so it cannot be bought, donated, or moved by its owner.*
Mere's credits are per-counterparty relationship balances for the same reason,
and tessera is earned only via the follow-through path. A GPU-rich or cash-rich
actor can rent capacity to a moot, win bounties, and run up a healthy credit
balance, but cannot convert that into trust or votes.

**Credit you cannot back with standing is not much good.** Drawing on the
commons is bounded by `min(requested, trust)`: even a large credit balance buys
service only up to the standing the counterparty extends you (via concord). So
credits and rep are coupled at spend time, which closes the last hoarding gap.
A cash-rich actor can amass credits but cannot consume past their reputation.

**The firewall is tessera, not cash.** Credits may be cash-permeable *at the
edges*: a community or an asker may make a bounty's reward redeemable for money,
so an outside company can fund research bounties for sats and fulfillers can
cash out. That does not breach the valve, because cash still cannot buy
*tessera*. This splits spending into two lanes: **commons reciprocity** (lend me
resources on my standing; rep-gated, ILL-shaped) and **funded bounties** (anyone
with escrowed credits, including low-rep outsiders, may post; de-risked by
escrow + community brokering rather than poster rep). The art-commission and
research-for-hire cases live in the second lane.

One load-bearing detail: tessera reward from a fulfilment must be shaped by
*reliability*, not *volume* (sub-linear in resource size, capped, decaying).
Otherwise raw throughput farms standing and the valve leaks. This shaping is a
configurable curve (§8), and whether reputation-loss slashing preserves the
honesty equilibria that the cited compute protocols derive from *cash* slashing
is the dominant open risk (§9, Q1).

---

## 2. Verifiable compute: a tessera-keyed tier ladder

Compute is harder than storage because the usual way to know an answer is right
is to recompute it. The research supports a ladder keyed to determinism, job
stakes, and provider standing. Escalate only as far as the job warrants.

- **T0 — deterministic, self-judging.** Embeddings with a fixed seed, graph
  clustering, transcoding. The result is a unique content hash, so N-of-M
  providers agreeing on the hash (or one cheap recompute) settles it. Cheapest;
  no economic machinery needed.
- **T1 — low-stakes, reputation + sampling.** Trust a high-tessera provider,
  spot-check a random sample. Back it with a
  [Proof-of-Sampling](https://arxiv.org/abs/2405.00295) economic check (confirmed
  3-0): PoSP has a pure-strategy Nash equilibrium making honest execution the
  rational best response, at ~0.735% overhead under a 10% adversary, versus zkML
  measured in minutes (nanoGPT) to days (Llama2-70B). The guarantee is
  conditional on rational, mostly-non-colluding (~<=10%) adversaries, and its
  slashing term must be re-expressed as tessera loss (`CommitmentLapsed` /
  `Censure`), not coin loss.
- **T2 — contested, replicate + bisect.** Replicated execution with interactive
  dispute resolution, after [Gensyn's Verde / refereed delegation](https://docs.gensyn.ai/litepaper)
  (confirmed 3-0): a multi-granular bisection narrows a disagreement to the
  single disputed operation, re-run for consistency and confirmed by a referee
  (for Mere, an independent evaluator or governance quorum). Note Gensyn's older
  probabilistic Proof-of-Learning is **superseded and spoofable** per their own
  [Verde paper](https://arxiv.org/abs/2502.19405); cite refereed delegation, not
  PoL.
- **T3 — high-value, layered.** For a moot adopting an adapter as a default, the
  state of the art is TEE attestation + optimistic fraud-proof window + stochastic
  ZK spot-checks, the [Optimistic TEE-Rollup](https://arxiv.org/abs/2512.20176)
  shape (confirmed 3-0, but an unreviewed Dec-2025 preprint with simulated-only
  figures, GPU-server + on-chain, **no wasm32 coverage**). Treat it as a
  conceptual template reachable only on native helpers, never on a browser tab.
  Pair it with independent-evaluator signatures + quorum + a private held-out eval
  (so the public eval cannot be overfit; Goodhart is the geist brief's poisoning
  concern restated).

**Who brokers is a configurable role, defaulting to the community.** The deal
settles under a moot's reputational ledger, so the moot is the natural arbiter,
and the evaluator/referee above *is* that broker. A moot may delegate: to the
**asker/buyer** (the art-commission shape, where watermarked samples de-risk,
escrow holds the asset, and satisfaction beyond any mechanical spec is the
buyer's call; or large-volume open bounties the asker self-judges against simple
criteria), or to a **paid broker role**, a community member who blesses the
transaction the way a priest blesses a marriage and earns tessera + credits for
the service. Brokering is itself a bankable service.

Non-determinism is why training never verifies by recompute: GPU float drift
means two honest runs do not bit-match (the geist brief already concedes
adapters are "auditable, not bit-reproducible"). Training is verified
**eval-based** (T3), not by replication.

---

## 3. Storage durability: erasure coding + audited repair

- **Erasure coding over naive replication.** Reed-Solomon-style coding cuts
  repair bandwidth under churn ([Storj](https://storj.dev/learn/concepts/file-redundancy),
  [Tahoe-LAFS](https://tahoe-lafs.readthedocs.io/en/latest/architecture.html),
  confirmed 3-0). A casual-friendly default is Tahoe's `k=3, N=10` (any 3 of 10
  shares reconstruct, 3.3x expansion); capacity moots can pick higher-rate
  schemes. (Do **not** claim erasure coding gives better durability at lower
  expansion than replication; that stronger framing was refuted 0-3. Claim only
  the repair-bandwidth and expansion-factor facts.)
- **Audited repair, not one-shot redundancy.** Durability under churn needs a
  **checker/repairer**: periodically count available shares, regenerate below a
  threshold (the Tahoe File Checker + File Repairer pattern, confirmed 3-0).
  Storj's worked figure: a 30/80 scheme at a 9-month node MTTF repairs ~35% of
  data monthly to hold eleven 9s. Cadence is a configurable setting derived from
  observed pinner MTTF, not a constant. This is cheesecloth made quantitative:
  overlapping unreliable pinners, share-counted and regenerated.
- **Compact proof-of-retrievability for the audit.** To prove a casual pinner
  actually holds its pledge, use [Shacham-Waters compact PoR](https://eprint.iacr.org/2008/073.pdf)
  (confirmed 3-0): data is *extractable* from any prover that passes, BLS
  homomorphic aggregation makes a challenge 20 bytes / 40 bytes at 80-bit
  security, and PoR *requires* the content be erasure-encoded first, which the
  point above already does. A failed challenge or a missed audit is the lapse
  trigger (derived, never self-reported, exactly like tessera heartbeats).
- **Borrow Filecoin's shape, not its economics.**
  [Filecoin](https://docs.filecoin.io/basics/the-blockchain/proofs) (confirmed
  3-0) pairs a one-time proof-of-replication at ingest with periodic WindowPoSt
  availability proofs on a deadline, and **graduates** the penalty (daily fault
  fees before full slash). Mere borrows the initial-proof-plus-periodic-proof
  structure and the graduated penalty, replacing collateral forfeiture with
  graduated tessera loss. A lapse is a slope, not a binary wipe.

A storage bounty is therefore: erasure-encode, escrow a credit stream, accept
pinners, challenge them on a cadence, pay per passed audit, lapse on failure,
regenerate shares below threshold.

**Redundancy is a demand dial.** Durability is not a fixed factor; an asker
spends more credits to attract more seeders, the seeder/peer shape of a
BitTorrent swarm laid over cheesecloth. `(k, N)` sets the floor; the escrowed
stream sets how many pinners hold shares above it. Popular content draws many
casual seeders cheaply; an asker who needs more assurance pays for it.

---

## 4. The browser/wasm32 reality

No surveyed compute network addresses contribution from a sandboxed browser tab;
OTR is explicitly GPU-server only. This is the part Mere designs from scratch,
and the constraint (Wasmtime out, Rhai + Burn-wgpu in) forces a clean split by
**contributor capability**:

- **A browser/PWA tab can offer:** light inference, embeddings, RAG retrieval,
  blob pinning + serving, *running* audits (PoR challenges, T0/T1 spot-checks),
  and governance. Burn-wgpu reaches real GPU through WebGPU, bounded and
  tab-lifecycle-fragile.
- **A tab cannot offer:** heavy LoRA training, big-model serving, TEE/ZK proofs.
- **Native helpers** (the moot-tiers brief's "old laptop running a mooting
  server") carry T2/T3 compute and big-model serving. They are the escape hatch.

Verification degrades with the contributor: **tab-provided compute is eligible
only for T0/T1** (deterministic or reputation+sampling), never T3; native helpers
unlock T2/T3. A bounty declares the minimum contributor capability it accepts, so
the asker controls the trust floor.

---

## 5. Anti-abuse

- **Sybil.** Fresh-chain=0 is validated by GNUnet's identical posture (confirmed
  2-1): new identities start at zero, effective request priority is bounded by
  `min(requested, trust)`, and "there is no difference between one large node and
  many small colluding nodes," so spinning up identities yields nothing on the
  spending side. Mere already has this via persona-chain-root + fork
  depreciation.
- **Free-riding.** The existing `reciprocity::may_request` already cuts off a
  counterparty whose unreciprocated debt exceeds tolerance. The bounty escrow
  reinforces it: you cannot post what you cannot pay.
- **Collusion (providers vouching for fakes; verifiers copying each other's
  verdicts).** The [Bittensor weight-copying](https://docs.learnbittensor.org/concepts/weight-copying-in-bittensor)
  failure (confirmed 3-0) is exactly this: copiers read public results and
  parasitize honest validators. The defense is **commit-reveal**: verdicts are
  committed as signed hashes and revealed only after a delay (Bittensor uses
  DRAND time-lock), which composes directly with Mere's signed event-DAG. Pair
  with independent trainer/evaluator separation and the already-built `Vouch`
  loop-back (endorsing a bad actor costs the voucher).
- **Bootstrap (newcomers with zero of everything).** The on-ramp is **contribute
  first**, not be-given. A newcomer has a computer, so they earn before they
  spend: permissionless seeding (hold + serve shares, paid per passed audit) and
  **T0 self-judging bounties** (a deterministic result the hash settles) need no
  prior standing, because the work verifies itself. Fulfilling them accrues
  credits *and* tessera together, so the newcomer climbs both ladders at once and
  graduates to higher-trust work. Putting the newcomer on the supply side first
  dissolves GNUnet's "Zero Priority Problem" (why serve a zero-credit stranger).
  The GNUnet **excess rule** (serve free under low load, still crediting the
  provider) and kith/kin **vouch**-lent starter credits remain as secondary
  smoothing, not the primary path.

---

## 6. Event-grammar extensions to tessera

Resource banking adds a thin layer over the existing event-DAG grammar; a bounty
is a commitment with a result spec attached, posted by the asker. Illustrative
signatures only (not implementation-ready):

```rust
// New events, riding the same signed Operation<...> wire as tessera + murm.
enum BankingEvent {
    // Asker defines the result, escrows credits, declares the verification tier
    // and the minimum contributor capability (tab vs native).
    ServiceRequest { spec: ResultSpec, escrow: Credits, tier: VerifyTier,
                     min_contributor: ContributorClass, deadline: Hlc },
    // Optional lease: converts an open bounty into a claimed task so duplicate
    // speculation is bounded. This IS a CommitmentMade against the bounty.
    BountyClaim { bounty: EventHash },
    // A content-addressed answer.
    Submission { bounty: EventHash, artifact: BlobHash },
    // Evaluator's signed judgement, committed then revealed (anti weight-copy).
    Verdict { submission: EventHash, pass: bool, evaluator: ChainRoot,
              commit: Hash, reveal: Option<VerdictReveal> },
    // Credits move asker -> winner; fires CommitmentFulfilled (tessera).
    Settlement { bounty: EventHash, winner: ChainRoot },
    // Streaming-bounty audit (storage). Failed/absent response = lapse.
    StorageAudit { pledge: EventHash, challenge: PorQuery, resp: Option<PorResp> },
}

enum ResultSpec {
    Compute { eval_ref: ManifestId, public_bar: Threshold,   // overfittable
              holdout_ref: Option<ManifestId>,                // private, evaluator-run
              required_weights: Vec<ManifestId> },            // base lineage + adapter stack (e.g. the moot geist); gates eligibility
    Storage { content: BlobHash, k: u16, n: u16,             // erasure params
              availability: Duration, audit_cadence: Duration },
}
```

Settlement reuses `CommitmentFulfilled` / `CommitmentLapsed` wholesale, so the
honey-and-stick and the per-moot ledger projection already work. Two settlement
**modes** fall out: an **open bounty** (deterministic or cacheable; first valid
hash wins; zero wasted work) and a **claimed task** (expensive/non-deterministic;
the lease means one worker holds it; quality-critical jobs can run as a
tournament-within-a-window, accepting the waste deliberately).

---

## 7. Where credits clear across tiers

Member-to-moot credits clear locally. Cross-moot clearing rides the existing
**concord** graph (one-hop, one-directional, weighted), so a member's credit in
moot M is honorable in moot N only to the extent N concords M. This is a
deliberate departure from GNUnet (whose trust is strictly non-transitive), and
its anti-collusion behaviour across federated moots is not stress-tested by any
source (§9, Q4). The `CautiousImport` composition policy (a concorded moot's red
flags at full weight, its praise discounted) is the safe default for credit
honouring across a concord.

---

## 8. Configurable parameters (per moot / per tier)

Per the configurability rule, every number is a per-moot/per-tier setting with a
sane default, never a hardcode:

- Erasure `(k, N)` (default `k=3, N=10`); repair threshold + cadence (from
  observed pinner MTTF).
- Verification tier thresholds: the tessera level and job-stake level at which
  T0->T1->T2->T3 escalate; the spot-check sample rate.
- Audit cadence + PoR challenge frequency.
- Excess threshold (local load below which newcomers are served free) + the
  newcomer-service tessera reward.
- Lapse curve (graduated, Filecoin-shaped) and the **tessera-from-fulfilment
  shaping curve** (sub-linear in volume, capped, decaying) that protects the
  valve.
- Commit-reveal delay for verdicts.
- Credit exchange behaviour (market-cleared default; a moot may pin a rate).

---

## 9. Open questions

1. **Tessera-slash equilibrium. RESOLVED to a Klein-Leffler condition, with one
   residual (2026-06-03).** The economics literature answers this. Reputation
   substitutes for a posted cash bond when the
   [Klein-Leffler](https://www.sfu.ca/~allen/KleinLeffler.pdf) (1981) condition
   holds: the present value of the future earnings stream lost by cheating exceeds
   the one-shot gain. Tessera is exactly such a stream (it gates future bounty
   access + credit honoured via `min(requested, trust)`), so a tessera slash
   deters like a cash slash **for a repeat player with a future**. It fails in
   three known cases, each already mitigated by the tier ladder: the **endgame**
   (an actor about to exit has no future to lose, so high-value or one-shot jobs
   need T2/T3 replication / escrow / proofs, not rep alone, which is what the
   ladder does); **cheap re-entry**
   ([Friedman-Resnick](https://www.researchgate.net/publication/2431661_The_Social_Cost_of_Cheap_Pseudonyms)
   2001, whose prescribed fix, free-but-unreplaceable pseudonyms, is exactly
   Mere's persona-chain-root + debt-carries-on-fork); and **newcomers** with
   nothing to lose (who "pay dues" via the T0 / seeding on-ramp). The residual is
   **correlated multi-moot exit** (the restaking "forfeit stake only once" attack:
   cheat every moot you hold standing in at once); per-moot rep + one-hop concords +
   `min(requested, trust)` bound each moot's exposure but do not eliminate it.
   Two configurable knobs strengthen rep-slash: audit probability (spot-check
   rate) and how much future value rep gates. A small agent-based sim could
   quantify the (audit-rate, future-value, fork-cost) -> cheating-rate surface,
   but the qualitative answer is settled.
2. **What can a sandboxed tab actually offer as *verifiable* compute** beyond the
   §4 sketch, and exactly how does the tier ladder degrade for tab work that
   cannot run TEEs or heavy ZK? The central wasm32 gap; nobody has precedent.
3. **One credit unit, cash-permeable at the edges.** This brief proposes one unit
   with a market-cleared rate, redeemable for money only where an asker chooses
   (research-for-hire, commissions). The firewall is that cash still cannot buy
   tessera, and drawing on the commons is bounded by `min(requested, trust)`, so
   credit hoarding cannot outrun standing. Stress-test that this pair
   (tessera-unbuyable + spend-bounded-by-rep) holds under an adversary who buys up
   credits and floods funded bounties. Naming of the unit is open ("reciprocoin"
   set aside for its money connotation).
4. **Cross-tier clearing vs non-transitive trust.** The one-hop concord is the
   proposed bridge over GNUnet's deliberate non-transitivity; its Sybil and
   collusion properties across federated moots are unproven.
5. **Tournament vs first-valid for non-deterministic open bounties** (quality vs
   wasted compute), and the exact lease/heartbeat cadence for claimed tasks.

---

## 10. First milestones (done-conditions, not dates)

Each builds on shipped substrate (tessera ledger + LogSync + iroh-blobs) and is
independently useful.

1. **Bounty grammar + escrow as a pure data model.** `BankingEvent` +
   `ResultSpec` folded into the per-moot ledger, escrow held against the
   reciprocity ledger. Testable the way tessera is (deterministic event sequence
   -> expected credit + tessera moves), no network. *Done when* a bounty
   posts, claims, settles, and lapses with the right ledger deltas.
2. **Storage audit over cheesecloth.** Erasure-encode a blob across iroh-blobs,
   run a compact PoR challenge, treat a missed/failed response as a graduated
   lapse, regenerate shares below threshold. Extends the eidetic-iroh cheesecloth
   pin. *Done when* a dropped pinner is detected by audit and its shares
   regenerate.
3. **Deterministic compute bounty (T0).** An embeddings or clustering job settled
   by N-of-M hash agreement, self-judging. *Done when* a divergent (wrong-hash)
   provider is rejected and the honest majority is paid.
4. **Verdict commit-reveal.** Evaluator verdicts committed as signed hashes,
   revealed after a delay, on the event-DAG. *Done when* a verifier cannot copy
   another's verdict before reveal.
5. **Contributor-capability tiering.** A bounty declares `min_contributor`; a tab
   contributor is offered only T0/T1 work, native helpers unlock T2/T3. *Done
   when* the same bounty routes differently by contributor class.

---

## Findings

### 2026-06-03 — research pass

- **The one-way valve has a proven structural form.** GNUnet's excess-based
  economy (trust held by the counterparty, non-ownable, non-transitive, bounded
  by `min(requested, trust)`) is the exact shape Mere's no-buy constraint needs,
  and it validates fresh-chain=0 directly (confirmed 3-0 / 2-1). Mere's
  departure is the one-hop concord composition, which GNUnet does not have and no
  source stress-tests.
- **Verification should be a tier ladder, not one mechanism.** Reputation +
  PoSP-style sampling at the low tier (Nash-equilibrium honesty, ~1% overhead,
  far below zkML), replicate + bisect at the contested tier (Gensyn Verde, not
  the superseded PoL), TEE+optimistic+ZK only at the heavy native tier (OTR, as a
  template not a fit). All of it inherits its security from cash slashing in the
  literature; re-mapping to tessera loss is the load-bearing, unvalidated
  adaptation.
- **Cheesecloth becomes quantitative** with erasure coding + a checker/repairer +
  compact PoR audits keyed to lapse, borrowing Filecoin's proof structure and
  graduated penalty without its economics.
- **The browser is the genuine frontier.** No surveyed network solves wasm-tab
  contribution; the capability-tiered split (tabs do light + pinning + audits;
  native helpers do heavy + proofs) is a Mere-original.
- **Design refinements (Mark, 2026-06-03).** Four that tightened the model: (1)
  redundancy is a demand dial on a seeder swarm, not a fixed factor; (2) credit
  you cannot back with standing is not much good (commons draws bounded by
  `min(requested, trust)`), which lets credits be cash-permeable at the edges
  while tessera stays the firewall, splitting spend into commons-reciprocity vs
  funded-bounty lanes; (3) brokering is a configurable, bankable role
  (community-default, delegable to buyer or to a "priest"); (4) the newcomer
  on-ramp is contribute-first via self-verifying work, which dissolves the Zero
  Priority Problem.
- **The tessera-slash equilibrium is grounded, not just asserted (2026-06-03).**
  [Klein-Leffler](https://www.sfu.ca/~allen/KleinLeffler.pdf) (1981) shows
  reputation is a bond made of future quasi-rents that deters like a posted bond
  when their present value exceeds the one-shot cheat gain;
  [Friedman-Resnick](https://www.researchgate.net/publication/2431661_The_Social_Cost_of_Cheap_Pseudonyms)
  (2001) name cheap pseudonyms as the failure mode and free-but-unreplaceable
  pseudonyms as the fix, which Mere already implements as the persona chain. The
  tier ladder turns out to be the failure-mode map: rep alone where the
  Klein-Leffler condition holds (T1, repeat players), bonded / replicated / proven
  verification exactly at the endgame, newcomer, and high-value cases where it
  does not. See §9 Q1.
- **Three claims were killed and must not be asserted:** probabilistic
  proof-of-learning being ~1350% more efficient than replication (refuted 0-3),
  the "Verifiability Trilemma" framing (1-2), and erasure coding giving better
  durability at lower expansion than replication (0-3).

---

## Pitfalls

- **The tessera-slash substitution holds only for repeat players.** Every
  imported economic guarantee assumes seizable cash stake; reputation substitutes
  only under the Klein-Leffler condition (§9 Q1), so it fails at the endgame, for
  newcomers, and under correlated multi-moot exit. Lean on the tier ladder, not
  rep alone, exactly there.
- **Buyability creep through credits.** A single floating credit unit is one
  careless feature away from money. The per-counterparty + non-convertible guards
  are load-bearing, not decorative.
- **Tournament waste.** First-valid open bounties race; claimed-task leases and
  tournaments trade waste for quality. Pick per job, and `log` what was dropped
  rather than silently truncating.
- **Concord transitivity.** One-hop is the safety rule; honouring credits past
  one hop relaunders reputation down a chain of agreements.

---

## Progress

### 2026-06-03

- Brief drafted from the deep-research pass (5 angles, 25 sources, 22/25 claims
  confirmed). Bounty/service-request adopted as the unifying primitive (storage =
  streaming bounty, compute = one-shot). Two-ledger valve specified (credits
  spendable + market-cleared; tessera unbuyable + reliability-shaped). Verifiable
  compute resolved as a tessera-keyed T0-T3 ladder; storage durability resolved as
  erasure coding + audited repair + compact PoR. Browser split (tabs vs native
  helpers) named as the wasm32-original. Event-grammar extensions sketched over the
  built tessera wire. Five open questions (tessera-slash equilibrium the dominant
  risk) and five milestones recorded. No code.
- DOC_README index updated.
- Refined with Mark same day (his notes predate the report but map cleanly): the
  seeder-swarm redundancy dial, the cash-edge firewall + two spend lanes +
  credit-bounded-by-standing, brokering-as-bankable-role (the "priest"), and the
  contribute-first newcomer on-ramp. Credit-unit naming left open ("reciprocoin"
  set aside for its money connotation; "favor" floated as a plain candidate).
- Resolved the dominant open risk (Q1) against the economics literature
  (Klein-Leffler 1981 reputation-as-quasi-rent-bond; Friedman-Resnick 2001 cheap
  pseudonyms + the unreplaceable-pseudonym fix; the restaking correlated-exit
  attack). Tessera-slash deters like cash-slash for repeat players; the tier
  ladder is the map of where it does not. One residual (correlated multi-moot
  exit) tracked; a confirmatory sim noted as optional.
