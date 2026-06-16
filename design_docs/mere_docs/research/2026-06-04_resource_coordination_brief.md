# Resource Coordination (banking + compute mesh)

**Date**: 2026-06-04
**Status**: Proposal (design probe). Consolidates and supersedes the two
2026-06-03 briefs (unified resource banking + personal compute mesh), now archived
at [`../../archive_docs/2026-06-04_resource_coordination_merge/`](../../archive_docs/2026-06-04_resource_coordination_merge/).
**Scope**: One model for sharing storage and compute across Mere, from a user's
own devices out to strangers. Mere ships the coordination toolkit; the asker or
moot picks the strategy per job. Grounded in three adversarially-verified
deep-research passes (storage/compute economics; speculative decoding +
disaggregation; swarm composability), cited inline; verification provenance is in
Findings.
**Related**:

- [`../../moothold_docs/implementation_strategy/2026-06-02_tessera_plan.md`](../../moothold_docs/implementation_strategy/2026-06-02_tessera_plan.md) — the built reputation ledger (Phases 1-5, 60 tests, two-peer convergence) + the `reciprocity` sibling ledger. This brief extends that grammar, it does not replace it.
- [`../implementation_strategy/2026-05-07_moot_tiers_and_voluntary_hosting_brief.md`](../implementation_strategy/2026-05-07_moot_tiers_and_voluntary_hosting_brief.md) — tier framework, cheesecloth pinning, ILL reciprocity, voluntary hosting + reputational stakes. This is the economy and mechanics those tiers spend.
- [`2026-05-10_geist_models_brief.md`](2026-05-10_geist_models_brief.md) — compute-as-utility (LoRA train/inference), trainer/evaluator separation, canary/poisoning gates, LoRA stacking + the compatibility envelope. Reframed here as bounty-shaped, with weight-sets as task inputs.
- [`../implementation_strategy/2026-05-07_event_dag_substrate_brief.md`](../implementation_strategy/2026-05-07_event_dag_substrate_brief.md) — the signed event-DAG (BLAKE3 + Ed25519 + CBOR) all events ride; §8.6 "the user is a one-member moot"; §8.7/§8.9 persona-chain Sybil posture; §8.8 the capability stack the sharing layer uses.
- [`../implementation_strategy/2026-06-03_actor_constellation_plan.md`](../implementation_strategy/2026-06-03_actor_constellation_plan.md) — a mesh job is a compute actor (P6) whose recipient is a remote device; the armillary request-to-result message travels over the p2panda space.
- [`../implementation_strategy/2026-06-01_p2panda_substrate_spike_plan.md`](../implementation_strategy/2026-06-01_p2panda_substrate_spike_plan.md) + [`2026-06-02_logsync_sync_as_projection_plan.md`](../implementation_strategy/2026-06-02_logsync_sync_as_projection_plan.md) — the live transport (`P2pandaTransport`, iroh underneath) + LogSync the mesh rides.
- [`2026-06-10_communal_compute_tiers_brief.md`](2026-06-10_communal_compute_tiers_brief.md) — the social/tier layer over this brief's mechanics: volunteer-computing lessons, the moothold/coalition commons, the LCZero data-not-gradients loop for the moot geist, time-bank as a constitution preset.

---

> **2026-06-10 correction (scripting).** The four-language scripting map this
> brief leaned on was collapsed to **Rust + JS** (actor constellation plan,
> Progress 2026-06-10): Rune and Rhai are dropped. Where this brief says
> "sandboxed Rhai scripts" (§0, §8), read: untrusted strategy / brokering /
> acceptance policy is a **non-Turing-complete declarative format evaluated by
> Rust** (which also earns re-run-to-a-hash determinism host-side, resolving
> §8's determinism caveat), and trusted orchestration logic is JS on serval's
> DOM-neutral script-engine seam (Nova native / Boa wasm, both no-JIT and
> wasm32-clean, so the browser/PWA reach argument is unchanged). The Plan-9
> namespace isolation (iroh-blobs + caps) and the code-vs-data split stand.
> Reopen trigger per the actor plan: a Rune 1.0 with a sandbox warranty, and
> only if policy stays a script rather than data.

## 0. Thesis: a toolkit, not a prescription

Mere's job is to give people the capability to coordinate in the best way for each
job. They pick the strategy. Everything below is a **menu**, the strategy ladders,
verification tiers, weight-sets, durability and scheduler parameters, with sane
defaults and nothing hardcoded (the configurability rule). A moot or an asker
composes the strategy for a bounty; Mere supplies the rails and the defaults, not
the policy. Concretely, the picked strategies, brokering rules, acceptance tests,
and coordination logic are sandboxed **Rhai** scripts (§8), so a moot can author its
own without anyone trusting it with the machine.

The whole design is **trust-graduated**: inside a trust domain (your own devices)
sharing is scheduling + permissions + isolation with no economy at all; the
verification economy is an outer shell that engages only as trust runs out. Build
inner-out.

---

## 1. The trust-graduated rings

| Ring | Trust | What it needs |
| --- | --- | --- |
| Your own devices | total | scheduling + permissions + isolation only (no economy, no verification) |
| Kith / kin | high | a capability grant + light sandbox; reputation optional |
| Moot members | medium | capability + tessera + a T1 spot-check |
| Strangers | low / none | the full bounty + T2/T3 verification + the credit economy |

[SHARY](https://arxiv.org/abs/2501.18840) (NOMS 2025), a deployed system for
sharing scarce GPUs and programmable switches across a research federation,
validates the inner case: it is a reservation system ("permissions to login are
dynamically granted and removed based on the reservations"), isolation is
containers and time-slots, and the only token is a politeness nudge to release
idle reservations. It needs no verification market because the participants
already trust each other. So the economy is the outer ring, not the core. Each
step out relaxes a capability and tightens a check.

---

## 2. The bounty: one primitive for storage and compute

Sharing a resource is a **bounty / service-request**. An asker defines a *result*
and escrows credits; whoever produces something that satisfies the result spec is
paid in credits; reliable fulfilment also earns tessera. Storage and compute are
the same operation at two cadences:

- A **compute bounty** is one-shot: deliver an artifact that passes spec, paid
  once.
- A **storage bounty** is streaming: keep this available, audited continuously,
  paid as a stream.

Because results are content-addressed, a bounty whose answer already exists
collapses into a pin: a compute cache-hit degrades to a storage fetch for free.

The bounty orientation inverts verification in our favour. An offer marketplace
makes you trust a *provider* then audit their labour; a bounty makes you judge the
*artifact* against acceptance criteria. The bounty spec **is** the eval, so the
geist brief's `EvalReport` + canaries + poisoning probes become the contract.

Two settlement modes fall out: an **open bounty** (deterministic or cacheable;
first valid hash wins; zero wasted work) and a **claimed task** (expensive or
non-deterministic; a lease means one worker holds it; quality-critical work can
run as a tournament-within-a-window, accepting the waste deliberately). At the
trust edge this stays async and commitment-oriented, a pull queue, never a live
stream, which is why streaming activations across strangers (§6) is the wrong
model.

### Event grammar (illustrative signatures, not implementation-ready)

A bounty is a commitment with a result-spec attached, posted by the asker, riding
the same signed `Operation<...>` wire as tessera and murm:

```rust
enum BankingEvent {
    // Asker defines the result, escrows credits, declares the verification tier
    // and the minimum contributor capability (tab vs native).
    ServiceRequest { spec: ResultSpec, escrow: Credits, tier: VerifyTier,
                     min_contributor: ContributorClass, deadline: Hlc },
    // Optional lease: converts an open bounty into a claimed task so duplicate
    // speculation is bounded. This IS a CommitmentMade against the bounty.
    BountyClaim { bounty: EventHash },
    Submission { bounty: EventHash, artifact: BlobHash },  // a content-addressed answer
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
              required_weights: Vec<ManifestId> },            // base lineage + adapter stack (e.g. the moot geist); gates eligibility (§6)
    Storage { content: BlobHash, k: u16, n: u16,             // erasure params
              availability: Duration, audit_cadence: Duration },
}
```

Settlement reuses `CommitmentFulfilled` / `CommitmentLapsed` wholesale, so the
honey-and-stick and the per-moot ledger projection already work.

---

## 3. Two ledgers, one valve

Coordination needs a spendable unit; tessera must stay unbuyable. Keep them
separate.

- **Reciprocity credits** are the spendable bank, directed and per-counterparty
  (A's balance *with* B lives in the B-to-A relationship), the existing
  `tessera::reciprocity` shape generalized from moot-to-moot down to
  member-to-moot. There is **one credit unit** for both resources; the exchange
  rate between durable-bytes-over-time and compute-cycles is whatever askers
  escrow and providers accept, per moot, market-cleared, never a protocol-fixed
  price. (Naming open: "reciprocoin" was set aside for its money connotation,
  "favor" floated but reads like karma; unresolved.)
- **Tessera** is standing. It gates participation and carries governance weight.
  It accrues only through `CommitmentFulfilled` (you pledged, you delivered, on
  heartbeat), and it decays.

**The valve.** Credits buy resources; they never buy standing. This holds for the
reason [GNUnet's excess-based economy](https://grothoff.org/christian/ebe.pdf)
holds: no node owns its trust; the trust a node earns is held at the nodes it
helped, so it cannot be bought, donated, or moved by its owner. Mere's credits are
per-counterparty relationship balances for the same reason, and tessera is earned
only via follow-through. One load-bearing detail: tessera reward from a fulfilment
must be shaped by *reliability*, not *volume* (sub-linear in resource size,
capped, decaying), or raw throughput farms standing and the valve leaks.

**Credit you cannot back with standing is not much good.** Drawing on the commons
is bounded by `min(requested, trust)`: even a large credit balance buys service
only up to the standing the counterparty extends (via concord). Credits and rep
couple at spend time, which closes the last hoarding gap, a cash-rich actor can
amass credits but cannot consume past their reputation.

**The firewall is tessera, not cash.** Credits may be cash-permeable *at the
edges*: a community or an asker may make a bounty reward redeemable for money, so
an outside company can fund research bounties for sats and fulfillers can cash
out. This does not breach the valve, because cash still cannot buy *tessera*. It
splits spending into two lanes: **commons reciprocity** (lend me resources on my
standing; rep-gated, ILL-shaped) and **funded bounties** (anyone with escrowed
credits, including low-rep outsiders, may post; de-risked by escrow + brokering
rather than poster rep). Art commissions and research-for-hire live in the second
lane.

---

## 4. Verifiable compute: a tessera-keyed tier ladder

Compute is harder than storage because the usual way to know an answer is right is
to recompute it. The ladder is keyed to determinism, job stakes, and provider
standing; escalate only as far as the job warrants.

- **T0, deterministic and self-judging.** Embeddings with a fixed seed, graph
  clustering, transcoding. The result is a unique content hash, so N-of-M
  providers agreeing on the hash (or one cheap recompute) settles it. No economic
  machinery.
- **T1, low-stakes, reputation + sampling.** Trust a high-tessera provider,
  spot-check a random sample, backed by a
  [Proof-of-Sampling](https://arxiv.org/abs/2405.00295) economic check: PoSP has a
  pure-strategy Nash equilibrium making honesty the rational best response, at
  ~0.735% overhead under a 10% adversary, versus zkML measured in minutes (nanoGPT)
  to days (Llama2-70B). Conditional on rational, mostly-non-colluding (~<=10%)
  adversaries; its slashing term re-expressed as tessera loss, not coin loss.
- **T2, contested, replicate + bisect.** Replicated execution with interactive
  dispute resolution, after [Gensyn's Verde / refereed delegation](https://docs.gensyn.ai/litepaper):
  a multi-granular bisection narrows a disagreement to the single disputed
  operation, re-run for consistency and confirmed by a referee (for Mere, an
  independent evaluator or governance quorum). Gensyn's older probabilistic
  Proof-of-Learning is superseded and spoofable per their own
  [Verde paper](https://arxiv.org/abs/2502.19405); cite refereed delegation.
- **T3, high-value, layered.** For a moot adopting an adapter as default, the
  state of the art is TEE attestation + an optimistic fraud-proof window +
  stochastic ZK spot-checks, the [Optimistic TEE-Rollup](https://arxiv.org/abs/2512.20176)
  shape (an unreviewed Dec-2025 preprint, simulated figures, GPU-server +
  on-chain, no wasm32 coverage). A conceptual template, native-helper-only, never
  a browser tab. Pair with independent-evaluator signatures + quorum + a private
  held-out eval so the public eval cannot be overfit (Goodhart, the geist brief's
  poisoning concern).

Non-determinism is why training never verifies by recompute: GPU float drift means
two honest runs do not bit-match (the geist brief concedes adapters are "auditable,
not bit-reproducible"). Training is verified eval-based (T3), not by replication.

**Who brokers is a configurable role, defaulting to the community.** The deal
settles under a moot's reputational ledger, so the moot is the natural arbiter,
and the evaluator/referee above *is* the broker. A moot may delegate: to the
**asker/buyer** (the art-commission shape, where watermarked samples de-risk,
escrow holds the asset, and satisfaction beyond a mechanical spec is the buyer's
call; or large-volume open bounties self-judged against simple criteria), or to a
**paid broker role**, a member who blesses the transaction the way a priest
blesses a marriage, earning tessera + credits. Brokering is itself a bankable
service.

**Anti-collusion is commit-reveal.** The [Bittensor weight-copying](https://docs.learnbittensor.org/concepts/weight-copying-in-bittensor)
failure (copiers read public results and parasitize honest validators) is the
threat for any public verification market. Verdicts are committed as signed hashes
and revealed only after a delay (Bittensor uses DRAND time-lock), which composes
with the signed event-DAG. Pair with trainer/evaluator separation and the
already-built `Vouch` loop-back (endorsing a bad actor costs the voucher).

**Whether a reputation slash deters like a cash slash** is the load-bearing
substitution, and it resolves to a classic condition.
[Klein-Leffler](https://www.sfu.ca/~allen/KleinLeffler.pdf) (1981): reputation is
a bond made of future quasi-rents and deters cheating when the present value of
the earnings stream lost by cheating exceeds the one-shot gain. Tessera is exactly
such a stream (it gates future bounty access + credit honoured via
`min(requested, trust)`), so a tessera slash deters like a cash slash **for a
repeat player with a future**. It fails in three known cases, and the tier ladder
is the map of exactly those failures:

- **Endgame**: an actor about to exit has no future to lose, so high-value or
  one-shot jobs need T2/T3 (replication, escrow, proofs), not rep alone.
- **Cheap re-entry** ([Friedman-Resnick](https://www.researchgate.net/publication/2431661_The_Social_Cost_of_Cheap_Pseudonyms),
  2001): if cheaters reset rep for free, deterrence collapses; their prescribed
  fix, free-but-unreplaceable pseudonyms, is exactly Mere's persona-chain-root +
  debt-carries-on-fork.
- **Newcomers** with nothing to lose: they "pay dues" via the T0 / seeding on-ramp
  (§9).

The residual is **correlated multi-moot exit** (the restaking "forfeit stake only
once" attack: cheat every moot you hold standing in at once); per-moot rep +
one-hop concords + `min(requested, trust)` bound each moot's exposure but do not
eliminate it (§12).

---

## 5. Storage durability: erasure coding + audited repair

- **Erasure coding over naive replication.** Reed-Solomon-style coding cuts repair
  bandwidth under churn ([Storj](https://storj.dev/learn/concepts/file-redundancy),
  [Tahoe-LAFS](https://tahoe-lafs.readthedocs.io/en/latest/architecture.html)). A
  casual-friendly default is Tahoe's `k=3, N=10` (any 3 of 10 shares reconstruct,
  3.3x expansion); capacity moots pick higher-rate schemes. (Do not claim erasure
  coding gives better durability at lower expansion than replication; that framing
  was refuted. Claim only repair-bandwidth and expansion-factor.)
- **Audited repair, not one-shot redundancy.** Durability under churn needs a
  **checker/repairer**: periodically count shares, regenerate below a threshold
  (the Tahoe File Checker + File Repairer pattern). Storj's worked figure: a 30/80
  scheme at a 9-month node MTTF repairs ~35% of data monthly to hold eleven 9s.
  Cadence is a configurable setting derived from observed pinner MTTF. This is
  cheesecloth made quantitative.
- **Compact proof-of-retrievability for the audit.** To prove a casual pinner
  holds its pledge, use [Shacham-Waters compact PoR](https://eprint.iacr.org/2008/073.pdf):
  data is *extractable* from any prover that passes, BLS homomorphic aggregation
  makes a challenge 20 bytes / 40 bytes at 80-bit security, and PoR *requires* the
  content be erasure-encoded first (which the first point already does). A failed
  challenge or missed audit is the lapse trigger, derived, never self-reported.
- **Borrow Filecoin's shape, not its economics.** [Filecoin](https://docs.filecoin.io/basics/the-blockchain/proofs)
  pairs a one-time proof-of-replication at ingest with periodic WindowPoSt
  availability proofs on a deadline, and **graduates** the penalty (daily fault
  fees before full slash). Mere borrows the initial-plus-periodic proof structure
  and the graduated penalty, replacing collateral forfeiture with graduated tessera
  loss. A lapse is a slope, not a binary wipe.

So a storage bounty is: erasure-encode, escrow a credit stream, accept pinners,
challenge on a cadence, pay per passed audit, lapse on failure, regenerate shares
below threshold. **Redundancy is a demand dial**: an asker spends more credits to
attract more seeders, the BitTorrent swarm shape over cheesecloth. `(k, N)` sets
the floor; the escrowed stream sets how many pinners hold shares above it.

---

## 6. Inference strategy: route first, shard last

The mesh's value is not cramming one model across devices. Pipeline sharding pays
a per-token network tax (every generated token traverses the whole pipeline, one
hop per stage), which consumer links punish into low-single-digit tokens/sec. So
sharding is the last rung. The ladder, a menu the asker picks from:

1. **Route the whole job (the backbone).** The model fits on *some* mesh device;
   send the whole inference there and stream tokens back. Embarrassingly parallel,
   zero per-token network tax. Covers most real use.
2. **Throughput / data parallelism.** For many independent jobs (embedding a
   corpus, RAG over many docs, a moot serving many askers, a hundred members
   translating a hundred chapters), spread *distinct* jobs across devices rather
   than one job across devices. This is where all the devices' power pays, and
   where the RAG-heavy geist workload lives. It is BOINC-shaped: an async
   content-addressed work-list, pulled and committed, never a stream, and at the
   trust edge it *is* the bounty queue. For the manga case this is the verified
   answer: the coordinator stays minimal (work-list + claim/heartbeat/deliver,
   reassign on lapsed heartbeat, validate by content address plus optional
   redundant or quorum re-run, the BOINC replication pattern), because there is no
   cross-device tensor traffic to schedule. A hero and a heavy moot scheduler are
   both unnecessary for an embarrassingly-parallel batch.
3. **Speculative decoding on a fast link.** A small draft model proposes K tokens;
   a strong model verifies them in one batched pass, amortizing the round-trip over
   K tokens. Cross-device speculative decoding is real
   ([DSSD](https://arxiv.org/abs/2507.12000), [FlowSpec](https://arxiv.org/abs/2507.02620)),
   but the measured speedup is modest (~1.2x-1.75x, sometimes below 1x), holds only
   on co-located edge or datacenter links with top-K logit compression, and has
   not been shown to survive consumer WAN across strangers. So it is an
   **inner-ring** optimization (your own devices on a LAN, or kith on a good link),
   not a trust-edge technique.
4. **Sharded pipeline inference (frontier escape hatch).** Only when a model
   exceeds every device, slow tokens are acceptable, and the link is fast. For MoE
   models (Mixtral, DeepSeek), shard by *expert* not layer, so only the active
   expert's device is hit per token. The research hardens why this is the last
   resort: inter-device KV-cache and activation transfer is a first-class cost,
   cheap only on datacenter InfiniBand (~5-8ms) and the chokepoint on WAN
   ([Splitwise](https://arxiv.org/abs/2311.18677), [HexGen-2](https://arxiv.org/abs/2502.07903));
   strategic sharding needs a central placement planner (a constraint or max-flow
   solver, the opposite of an ad-hoc pull queue); and pipelined sharding collapses
   at the low utilization typical of small meshes ([FlowSpec](https://arxiv.org/abs/2507.02620)).
   All of this is the *interactive* case; for **async** work the per-token latency
   objection dissolves and sharded hosting becomes viable (see "Communal big models"
   below).

The deeper reframe: geist's value is *context* (your flora), not parameter count,
so the strongest move is usually to *not need* a model too big to fit. Lean on RAG +
a personal LoRA + speculative decoding to make a fits-on-one-device model
excellent, and spend the mesh on the parallel rungs (1-3). Rung 4 is the "I want
the 671B and I will wait" lever.

### Composing models for quality (the swarm question)

A swarm of small models does **not** reliably conjure capability beyond its
strongest member. Open-model swarms beat a GPT-4-class model on general-chat
leaderboards (Together [Mixture-of-Agents](https://arxiv.org/abs/2406.04692):
65.1% vs 57.5% on AlpacaEval 2.0), but that win is metric-fragile and never shown
on translation; aggregating samples from the single best model (Self-MoA,
[arXiv 2502.00674](https://arxiv.org/abs/2502.00674)) beats mixing weaker ones by
~6.6 points, so the gain is proposer *quality*, not diversity, and model fusion
still trails its best member on that member's strength
([FuseLLM](https://arxiv.org/abs/2401.10491)). On translation a single strong
model wins human quality 5 of 6 times at one fifth to one fifteenth the token cost
of a multi-agent workflow ([arXiv 2505.01560](https://arxiv.org/abs/2505.01560)),
and translation may be genuinely scale-gated. The one composition with evidence of
net gain is **draft-then-refine where the refiner is strictly stronger** than the
drafter, paying off mostly when the drafter is weak
([TEaR](https://arxiv.org/abs/2402.16379)). The "every model references the
others' takes and converges on a polished answer" shape *is* Mixture-of-Agents;
its danger for translation is that the consensus grows more *fluent* while drifting
from the source (reference metrics fall, content is dropped), so polish is not
faithfulness. Equal-weak peers converge on shared confident error.

**When a virtual model does work (exceptions to the ceiling).** The
ceiling-is-your-strongest-member rule applies to naive ensembling and consensus.
Two families escape it. First, **running one genuinely large model distributed**:
pipeline/tensor sharding (rung 4) and MoE expert-parallel give the real big model's
capability, and speculative decoding returns the large model's exact distribution
losslessly; these are virtual large models that pay network cost, not capability
cost, and still require the large model (or its experts) to exist on the mesh.
Second, **trading the swarm's abundant parallel compute for quality where the task
allows it**: when verification is cheap and reliable (code passes tests, math
checks, constraints validate), many weak attempts plus a verifier (best-of-N,
self-consistency, search) punch far above any single member, bounded only by
whether a correct answer is in the weak model's sample distribution at all; and
when a task decomposes into in-reach sub-steps, agentic decomposition solves what
one shot cannot. The unifying rule: a swarm converts parallel compute into
effective capability exactly when the task lets you trade compute for quality
(verifiable, decomposable, or search-amenable). Translation is the hard case, since
it is not cheaply verifiable and the hard part is reasoning, but it has one mild
lever: generate many candidate translations in parallel and rerank with a
quality-estimation model, which beats a single greedy decode up to the reranker's
own ceiling.

**Heterogeneity is the point.** A real swarm is a mix of device classes, so it
*has* a strongest member, which is exactly the strict-refiner asymmetry
collaboration needs. The productive shape is capability-tiered: weak devices draft
and bulk-translate in parallel (throughput), the strongest model refines the hard
parts and sets the ceiling. The constraint holds: the swarm's ceiling is
essentially its strongest member (composition does not reliably exceed it), so
heterogeneity buys throughput plus a refiner, not capability beyond your best
node; a chapter needing frontier scale needs a frontier-class node in the swarm.
The one documented way to beat the best member is *orthogonal specialization*
(members strong at different sub-skills); for manga that is plausible (honorifics,
onomatopoeia, dialogue register) but the measured gains elsewhere are marginal, so
treat it as an experiment.

**Tasks carry a weight-set; the moot's geist is the shared quality lever.** A
node's capability is its base model *times* the LoRA adapters its owner has loaded,
so the swarm is heterogeneous by adapter as well as by size, and a bounty's
`required_weights` names a base lineage plus a stack of content-addressed adapters,
the moot's geist among them. Adapters are small (10-100MB) BLAKE3 engrams, so a
worker fetches a missing one via iroh-blobs once and pins it (a cache-hit collapses
to a pin), and the per-job namespace (§8) carries the weight blobs as scoped
inputs. Eligibility falls out: a node may claim a weighted bounty only with a
*compatible* base (the geist brief's strict LoRA-stacking envelope: base bytes,
tokenizer, prompt template, target modules, format, quantization) and the
fetchable stack. The payoff does not hit the composability ceiling: a shared
moot-trained adapter on every drafter gives consistent terminology, honorifics, and
house style across the swarm, exactly where LoRA earns its keep (style and idiom,
not the base's reasoning). This is deliberate shared *specialization*, not random
ensemble orthogonality. The caveat persists: an adapter lifts style, not reasoning,
so hard chapters still want the strongest base as refiner. A required adapter
should be a vetted, adopted moot artifact (the geist brief's adoption +
canary/poisoning gates), and granting one for a job is a namespace capability.

**How far the geist closes the gap (resolved 2026-06-04).** The style-vs-reasoning
line is real and now quantified: restyling-only adaptation recovers ~100% of style
and safety behaviour but only ~53-62% of math-reasoning and ~67-94% of factuality
([decomposition study](https://arxiv.org/abs/2502.04602)), and fine-tuning teaches
a model to *use* its pretrained knowledge rather than injecting new facts (RAG
beats unsupervised fine-tuning decisively, 0.875 vs 0.504 on post-cutoff events:
[Ovadia](https://arxiv.org/abs/2312.05934), [Gekhman](https://arxiv.org/abs/2405.05904)).
Translation is unusually fine-tuning-friendly, so the geist closes a lot of the
surface gap: a fine-tuned 7B/13B MT model reaches frontier-level fluency and
adequacy on high-resource pairs ([ALMA-R](https://arxiv.org/abs/2401.08417)),
though that crossover is on reference-free QE metrics it optimizes, not
human-judged hard cases. The split for manga: the geist (LoRA) reliably carries the
**style side**, honorifics, register, recurring terminology and character voice,
formatting and bubble constraints; the **reasoning side** stays base-bound,
cultural-nuance inference, idiom needing world-knowledge, ambiguity and referent
disambiguation, document-level coherence, puns. So base + geist suffices for
style-and-terminology chapters; a stronger base (the hero refiner) is still
warranted for reasoning-dominated ones. The geist should be **LoRA + RAG**, rather
than a full fine-tune or fact-baking: LoRA gives small content-addressed fetchable
adapters that preserve base reasoning (it forgets less), and translation is a near
domain where the LoRA-to-full-FT gap is small; RAG and constrained decoding carry
the glossary and world-knowledge that fine-tuning injects poorly (~17% retention
for translation-as-mapping). Keep it one focused, all-layer translation adapter,
not a stack of many domains, which acquires "intruder dimensions" and forgets more
([Shuttleworth](https://arxiv.org/abs/2410.21228)).

### Prior art for rung 4 (licenses decide use)

- [exo](https://github.com/exo-explore/exo) (Apache 2.0): pure-p2p, dynamic
  partitioning by topology, auto-discovery; Python + MLX-centric, so a port is a
  rewrite, but the permissive license lets us borrow its technique freely.
- [cake](https://github.com/evilsocket/cake) (FAIR license, *not* OSI-permissive):
  the closest Rust system, shards transformer blocks across all platforms, master
  streams weight shards. FAIR forbids business use without a signed agreement, so
  it is **reference-only**, and it is master-worker, not pure-p2p.
- Petals (geist brief): the sharded-big-model-inference pattern already named.

Burn's distributed support is **training-only** through the latest release.
`burn-collective` (Burn 0.19) added an all-reduce for gradient synchronization over
WebSockets; Burn 0.21 ([May 2026, latest](https://burn.dev/blog/release-0.21.0/))
reworked it into differentiable collectives + `burn-dispatch`, but in Burn's own
words "the foundation is in place," and no release through 0.21 ships inference
distribution of any kind. So rung 4 is fully net-new on Burn-as-per-node-runtime,
transported over p2panda; rungs 1-3 need only routing plus LogSync.

### Communal big models: hosting is ready, training is the frontier

The dream of communally running a model nobody can individually hold is closer than
the inference rungs alone implied (2026-06-04 research).

**Hosting is reachable now, for async work.** Sharding makes the RAM constraint
aggregate-swarm, not per-node: each node holds a few layers, and Petals already
hosts 70B-176B models across volunteer nodes over the open internet. The per-token
WAN-latency objection (rung 4) is an *interactive* one; for async bounty work it
dissolves, so slow-but-steady big-model inference across the swarm is acceptable.
This is the concrete shape of a **Mere-native Petals on Burn + p2panda** (Petals is
MIT, so portable): Burn-wgpu as the per-node block executor, p2panda for the routing
and fault tolerance hivemind gave Petals, plus the trust rings, bounty economy, and
namespace Petals lacks. The efficiency win is in orchestration and portability
(browser/WebGPU block-holders, no GIL), not the matmuls or the network-bound
dominant cost.

**Training over WAN is demonstrated to 32B, with two asterisks.**
Low-communication training (the [DiLoCo](https://arxiv.org/abs/2311.08105) family:
many local steps between infrequent all-reduces) cuts communication 50x-500x and
trains over consumer-grade links; it has run over the real internet across
continents up to a 10B pretrain ([INTELLECT-1](https://arxiv.org/html/2412.01152v1):
400x comm reduction, 83-96% compute utilization) and a 32B RL fine-tune
([INTELLECT-2](https://arxiv.org/html/2505.07291v1)). The asterisks: (1) **DiLoCo
solves bandwidth, not RAM**, it is data-parallel, so every node must hold the full
model; "nobody has the RAM" training specifically needs *sharded* (pipeline or
tensor) training combined with low-comm, which no verified run has shown end-to-end.
(2) Every demonstrated run used **curated, vetted datacenter nodes** (named
providers, VPN, centralized orchestrator), not churning untrusted volunteers, and
**contribution verification is an open problem**: INTELLECT-2's claim that TOPLOC
verifies untrusted contributions and slashes bad nodes did not survive
verification, and the field treats poisoned-gradient and free-rider defence as
unsolved.

**The Mere fit is precise.** The curated-vs-untrusted gap is exactly the
trust-graduated rings: training within a trusted ring (a moot of vetted members) is
the curated case that already works; the untrusted churning-volunteer case is the
open frontier, and Mere's tessera, verification ladder, and bounty escrow are the
contribution-verification layer the field is missing. So Mere consumes hosting and
curated-ring training as available technology, while untrusted-pool training is a
research contribution it is unusually positioned to make, not a dependency. Caveat:
even the curated case carries a contested downstream-quality risk ("representation
drift" collapsing benchmark scores after fine-tuning despite matched pretraining
loss, [arXiv 2511.13761](https://arxiv.org/abs/2511.13761)), and the frontier 100B+
figures are simulations, not runs.

---

## 7. The browser/wasm32 reality

No surveyed compute network addresses contribution from a sandboxed browser tab.
This is the part Mere designs from scratch, and the constraint (Wasmtime out, Rhai +
Burn-wgpu in) forces a clean split by **contributor capability**:

- **A browser/PWA tab can offer:** light inference, embeddings, RAG retrieval, blob
  pinning + serving, running audits (PoR challenges, T0/T1 spot-checks), and
  governance. Burn-wgpu reaches real GPU through WebGPU, bounded and
  tab-lifecycle-fragile.
- **A tab cannot offer:** heavy LoRA training, big-model serving, TEE/ZK proofs.
- **Native helpers** (the moot-tiers "old laptop running a mooting server") carry
  T2/T3 compute and big-model serving.

Verification degrades with the contributor: **tab-provided compute is eligible
only for T0/T1**, never T3; native helpers unlock T2/T3. A bounty declares its
`min_contributor`, so the asker sets the trust floor. A browser tab being a real
cluster node at all is a Mere-original; neither exo nor cake runs in-browser.

---

## 8. Scheduler, grant state, and the Plan-9 namespace

**Owner priority is load-bearing, from day one.** Your own foreground use always
wins: an owned device keeps **absolute reclamation priority** over its GPU/CPU.
Guest and kith jobs run on **revocable leases** with real preemption, and each job
declares a **checkpoint class**: interruptible (kill and re-dispatch),
checkpointable (snapshot and resume elsewhere), or non-interruptible (must finish
or fail). Owner reclamation is a *clean cancellation*, not a worker lapse, so a
preempted guest takes no rep penalty (it maps to the clean-handoff event, never
`CommitmentLapsed`). The reservation layer stays light (queue + lease +
heartbeat).

**Grant state is shared; enforcement is local.** A grant, lease, revocation,
heartbeat, and result receipt are signed facts on the p2p event substrate (the
same LogSync DAG tessera and murm ride), so a grant is one more projection of the
event DAG, like the tessera ledger. Every device watches that state and enforces
it locally, so no broker has to be online to police a worker and the system stays
genuinely p2p.

> The mesh is Plan-9-shaped: every job runs inside a capability-scoped namespace
> assembled from replicated grant state. Owned devices enforce grants locally and
> retain absolute reclamation priority over their resources.

**The namespace is the isolation primitive, not just the sandbox.** A job never
gets ambient access to "my machine." It receives a **per-job namespace** assembled
from capabilities: its input blobs, the model weights, scratch space, an output
sink, optionally a metrics channel, and nothing else, built from iroh-blobs
(content-addressed inputs, weights, outputs) plus iroh-docs and caps (the scoped
grant). The sandbox (wasm, subprocess, container) contains the *code*; the
namespace bounds what the code can even *name*. Both tighten by ring: own devices
need minimal isolation (the actor boundary's failure + memory safety is enough),
outer rings running others' jobs need a real box (the actor constellation threat
model: an in-process boundary is not a confidentiality boundary).

**Rhai is the logic-layer sandbox, and it dissolves most of the browser worry.** A
compute job has two kinds of code, and only one is dangerous. The heavy numeric
kernel (matmul, inference, training) is the worker's *own* trusted Burn-wgpu code
run on the requester's content-addressed *data* (weights and inputs in the
namespace), so there is no untrusted code there to sandbox, only untrusted data the
namespace already bounds. The dangerous kind is untrusted *logic*: a moot's
brokering rule, a verification predicate, a bounty's acceptance test, a novel
coordination strategy. That is what **Rhai** is for, a Rust embedded scripting
interpreter that is sandboxed (a script reaches only the host functions you
register), deterministic, and runs everywhere wasm32 ships including the browser,
where Wasmtime cannot. Rhai's security model *is* the Plan-9 namespace at the logic
layer: the namespace bounds data access (iroh-blobs + caps), Rhai bounds logic
access (the granted host functions), and they compose. So a browser node safely
runs untrusted Rhai logic plus its own Burn kernels on untrusted data; the only
case still needing a subprocess or container is running a stranger's arbitrary
*native binary*, which is rare and simply not offered on a browser node. Determinism
must be earned, not assumed: neither Rhai nor Rune documents a determinism
guarantee (verified 2026-06-04), so a re-runnable acceptance predicate or T0 job
needs determinism enforced host-side (integer or fixed-point math, canonical
iteration order, no host nondeterminism); once constrained, N-of-M hash agreement
and re-runnable verdicts follow. Rhai is for
control and policy, never the numeric inner loop (that is Burn-wgpu), and you must
set its operation and time limits; like the actor boundary it is a memory-safety and
capability boundary, not a Spectre-grade confidentiality one.

Syncthing is the **product** north-star (all your devices feel like one substrate,
nothing to configure); Plan 9 is the **architecture** (per-process namespaces, the
deeper lesson over remote-CPU). Compute adds placement, cancellation, determinism,
verification, and thermal/battery policy, so Syncthing is the feel, not the design.

---

## 9. Sharing, permissions, and where the economy engages

Widening the mesh past your own devices is the permissions discussion, and Mere
has the language: the §8.8 capability stack (structural caps, meadowcap-shaped
BLAKE3 cluster-paths + Biscuit policy over tessera facts + Keyhive/p2panda-encryption
group/key state). "Let Alice's jobs run on my mesh for 30 days, capped at N hours,
only T0/T1 work" is a capability grant, not a market transaction.

The handoff to the economy is precise: as long as a job runs inside a trust ring
(own devices, granted kith) it is permission-gated and free. The moment it crosses
to medium or low trust (a moot pool, a stranger), the verification tier and the
escrowed credits switch on. The mesh is the substrate; banking is the toll booth
at the trust boundary, the same p2panda message gaining a tier and a settlement
only when it leaves the ring of trust.

**Cross-tier credit clearing** rides the existing **concord** graph (one-hop,
one-directional, weighted): a member's credit in moot M is honourable in moot N
only to the extent N concords M. This departs from GNUnet's strict
non-transitivity; the `CautiousImport` composition policy (a concorded moot's red
flags at full weight, its praise discounted) is the safe default, and the
anti-collusion behaviour across federated moots is unproven (§12).

**Anti-abuse, bootstrap.** The on-ramp for newcomers with zero of everything is
**contribute first**, not be-given. A newcomer has a computer, so they earn before
they spend: permissionless seeding and **T0 self-judging bounties** need no prior
standing because the work verifies itself, and fulfilling them accrues credits
*and* tessera together. Putting the newcomer on the supply side first dissolves
GNUnet's "Zero Priority Problem"; the excess rule (serve free under low load, still
crediting the provider) and kith/kin vouch-lent starter credits are secondary
smoothing.

---

## 10. Build on what we have

The two genuinely hard substrates an "exo in Rust" needs are already in Mere's
hands, so the mesh is not a standalone project, it is the compute-mesh layer of the
orrery:

- **Networking, discovery, sync:** p2panda (iroh underneath), `P2pandaTransport`,
  QUIC + hole-punching, mDNS + random-walk discovery, RBSR + gossip, LogSync. It
  already roams across networks, so a phone on cellular reaches a home workstation
  through NAT.
- **A cross-platform runtime:** Burn-wgpu, everywhere wasm32 ships including the
  browser via WebGPU, plus quantization for fitting models on small devices.

Only the orchestration on top is new (the inference rungs, the bounty queue, the
scheduler), and rungs 1-3 ride routing + LogSync directly.

---

## 11. Configurable parameters (per moot, per tier, per user)

Every number is a setting with a sane default, never a hardcode:

- Erasure `(k, N)` (default `k=3, N=10`); repair threshold + cadence (from observed
  pinner MTTF); audit cadence + PoR challenge frequency.
- Verification-tier escalation thresholds (tessera level, job-stake level for
  T0->T1->T2->T3); spot-check sample rate; commit-reveal delay.
- Lapse curve (graduated, Filecoin-shaped) and the tessera-from-fulfilment shaping
  curve (sub-linear in volume, capped, decaying) that protects the valve.
- Excess threshold (load below which newcomers are served free) + the
  newcomer-service tessera reward; credit exchange behaviour (market default; a
  moot may pin a rate).
- Per-ring capability defaults (own = full; kith = T0/T1; moot = tessera-gated);
  which devices participate and for which resource kinds; idle threshold to offer a
  device.
- Scheduler policy: owner-reclamation priority (always on for owned devices), lease
  duration + preemption for guest jobs, allowed checkpoint classes per ring.
- Sharding policy (shard vs route-whole); the manga-style batch coordinator's
  redundant-validation factor.

---

## 12. Open questions

- **Correlated multi-moot exit** (the tessera-slash residual): cheat every moot you
  hold standing in at once. Per-moot rep + one-hop concords bound each moot's
  exposure but do not eliminate it. A small agent-based sim could quantify the
  (audit-rate, future-value, fork-cost) -> cheating-rate surface.
- **One credit unit, cash-permeable at the edges:** stress-test that
  tessera-unbuyable + spend-bounded-by-rep holds under an adversary who buys up
  credits and floods funded bounties. Plus the unit's name.
- **Cross-tier clearing vs non-transitive trust:** the one-hop concord's Sybil and
  collusion properties across federated moots are unproven.
- **Tournament vs first-valid** for non-deterministic open bounties (quality vs
  wasted compute), and the lease/heartbeat cadence for claimed tasks.
- **When is sharding worth the latency** (the LAN-vs-WAN crossover), and KV-cache
  placement + mid-inference recovery when a device drops a token in.
- **Outer-ring isolation. Largely RESOLVED (2026-06-04, see §8):** untrusted *logic*
  runs in the Rhai sandbox (deterministic, browser-safe, capability-scoped), and ML
  jobs run the worker's own Burn kernels on namespace-bounded untrusted *data*, so no
  untrusted code runs there. The residual is only running a stranger's arbitrary
  *native binary* (subprocess or container, native-only), plus tuning Rhai's
  operation and time limits.
- **How light the bounty coordinator can be at ~100 nodes** while validating
  against cheating and reassigning dropped workers; and whether Petals/exo/cake fit
  an ad-hoc bounty model or assume a persistent overlay, with real throughput over
  consumer internet. (Both deprioritized across the research passes; still open.)
- **How far the moot geist closes the gap. RESOLVED (2026-06-04, see §6):** the
  style-vs-reasoning line is real and quantified (restyling recovers ~100% of style,
  ~53-62% of reasoning); a LoRA geist carries translation's style side and
  translation is fine-tuning-friendly, but the reasoning side stays base-bound, so
  the hero refiner remains for reasoning-dominated chapters and the geist is LoRA +
  RAG, not fact-baking. Residuals: no source tested LoRA-vs-scale *on translation*
  (literary/manga, low-resource, document-level) directly; ALMA-R's crossover is
  metric-circular and high-resource only; how stacking multiple domain LoRAs
  interacts with intruder-dimension forgetting is untested.
- **Where the mesh lives as a crate** — *resolved 2026-06-12*: `crates/mesh/`
  (the [mesh M1 plan](../../archive_docs/2026-06-15_completed_plans/2026-06-12_mesh_m1_plan.md));
  the scheduler-vs-P6 relation stands as designed (P6's compute actor drives
  this crate's wire/state; same machinery, wider recipient).
- **Communal training: bandwidth is solved, RAM and trust are not.** DiLoCo solves
  WAN bandwidth but is data-parallel (each node holds the full model); a sharded +
  low-comm scheme giving true aggregate-swarm RAM for *training* is unproven
  end-to-end. No run combines untrusted churning volunteers with working
  contribution-verification (read SWARM parallelism and SENTINEL,
  [arXiv 2603.03592](https://arxiv.org/abs/2603.03592), before designing Mere's
  training-verification layer), and DiLoCo's contested downstream "representation
  drift" caps usable quality.

---

## 13. First milestones (done-conditions, not dates)

Each builds on shipped substrate (tessera ledger + LogSync + iroh-blobs) and is
independently useful. The personal mesh is valuable at milestone 1, long before
any economy.

1. **Compute actor over the personal space.** One owned device posts a request into
   `SpaceId::Personal`, another claims it and returns the result via LogSync. *Done
   when* a laptop's job runs on the workstation and the result lands back, no
   economy involved. **DONE 2026-06-12**: `crates/mesh/mesh` + the `mesh-peer`
   bin ran the literal shape — the Windows laptop posted, the Fedora ThinkPad
   claimed and executed, the result landed back over LogSync (ticket bootstrap,
   both ways). See the
   [mesh M1 plan](../../archive_docs/2026-06-15_completed_plans/2026-06-12_mesh_m1_plan.md).
2. **Resource adapter + first kind.** A `MeshResource` trait plus one adapter (a
   Burn-wgpu job or an embeddings batch). *Done when* a second kind is one adapter,
   not a core change.
3. **Idle routing + data-parallel batch queue.** Route a job to whichever owned
   device is awake and free; extend to a pull-based content-addressed work-list
   (the manga case) with claim/heartbeat/deliver and redundant validation. *Done
   when* a dropped worker's task is reassigned and the honest result is accepted.
4. **Bounty grammar + escrow as a pure data model.** `BankingEvent` + `ResultSpec`
   folded into the per-moot ledger, escrow held against the reciprocity ledger,
   testable like tessera (event sequence -> expected credit + tessera moves). *Done
   when* a bounty posts, claims, settles, and lapses with the right deltas.
5. **Storage audit over cheesecloth.** Erasure-encode a blob across iroh-blobs, run
   a compact PoR challenge, treat a missed response as a graduated lapse, regenerate
   shares below threshold. *Done when* a dropped pinner is detected and its shares
   regenerate.
6. **Deterministic T0 compute bounty + verdict commit-reveal.** An embeddings or
   clustering job settled by N-of-M hash agreement; evaluator verdicts committed as
   signed hashes, revealed after a delay. *Done when* a wrong-hash provider is
   rejected and a verifier cannot copy another's verdict before reveal.
7. **Capability-gated sharing to one kith peer.** Grant a cap, run a T0 job for a
   friend on your mesh. *Done when* the grant scopes what they may run and expires.
8. **Two-device sharded inference (frontier escape hatch, rung 4).** A model too
   big for one owned device runs across two, Burn-based, over p2panda. The rare
   lever, not the backbone. *Done when* the shard produces correct tokens and
   survives one device dropping.
9. **Mere-native Petals (communal hosting) on Burn + p2panda.** Shard a big model
   across owned and kith nodes, route an async inference bounty through the chain of
   block-holders, recover when one drops. Petals (MIT) is the portable reference.
   *Done when* a model larger than any single node's RAM answers an async bounty
   across the swarm. (Communal *training* over untrusted volunteers stays a research
   frontier, not a milestone.)

---

## Findings

Consolidated from three adversarially-verified deep-research passes (2026-06-03 and
2026-06-04). Each surviving claim was confirmed by a 3-vote majority unless noted;
the killed claims below must not be asserted.

- **The one-way valve has a proven structural form** (GNUnet's excess-based
  economy): trust held by the counterparty, non-ownable, non-transitive, bounded by
  `min(requested, trust)`. It validates fresh-chain=0 directly. Mere's one-hop
  concord composition is a deliberate departure GNUnet does not have and no source
  stress-tests.
- **The tessera-slash equilibrium is grounded** (Klein-Leffler 1981
  reputation-as-quasi-rent-bond; Friedman-Resnick 2001 cheap pseudonyms + the
  unreplaceable-pseudonym fix Mere already implements). Rep-slash deters like
  cash-slash for repeat players; the tier ladder is the map of where it does not
  (endgame, newcomer, cheap re-entry), each mitigated; the residual is correlated
  multi-moot exit.
- **Verification is a tier ladder, not one mechanism:** PoSP sampling (Nash-honest,
  ~1% overhead, far below zkML), replicate + bisect (Gensyn Verde, not the
  superseded spoofable PoL), TEE+optimistic+ZK only at the heavy native tier (OTR,
  template not fit). All inherit security from cash slashing; re-mapping to tessera
  loss is the load-bearing adaptation.
- **Cheesecloth becomes quantitative** with erasure coding (Tahoe k=3/N=10) + a
  checker/repairer (Storj 30/80, ~35%/month for eleven 9s) + compact PoR audits
  (Shacham-Waters, 20B/40B) keyed to graduated lapse, borrowing Filecoin's proof
  structure without its economics.
- **Cross-device inference research validates the bounty model.** Cross-device
  speculative decoding is real but modest (~1.2x-1.75x, sometimes <1x) and
  fast-link-only, with no evidence it survives consumer WAN across strangers;
  sharded inference's KV-cache transfer is cheap only on InfiniBand (Splitwise
  ~5-8ms, NEVER cite for the WAN trust-edge case) and is the WAN chokepoint;
  strategic sharding needs a central planner incompatible with ad-hoc pull. For
  embarrassingly-parallel batch the answer is BOINC-style data parallelism over a
  light bounty queue.
- **A swarm of small models is not a frontier substitute, especially for
  translation.** Open-model swarms beat GPT-4-class only on general-chat
  leaderboards and metric-fragile (Self-MoA beats heterogeneous MoA, so the lift is
  model quality, not diversity); fusion does not exceed its strongest member; on
  translation a single strong model wins 5 of 6 at 5-15x less cost, and translation
  may be genuinely scale-gated. The one net-positive composition is draft-then-refine
  with a strictly stronger refiner. A *heterogeneous* swarm supplies that refiner
  for free (capability-tiered draft then refine), but the ceiling stays its
  strongest member; the moot geist (a shared adapter) lifts domain consistency
  without hitting the composability ceiling because it is specialization, not
  ensembling.
- **The browser is the genuine frontier** (no surveyed network solves wasm-tab
  contribution; the capability-tiered split is a Mere-original), and **the mesh is
  Plan-9-shaped** (Mark): per-process namespaces over remote-CPU, grant-state shared
  / enforcement local, owner reclamation priority making leases/preemption/checkpoint
  classes day-one scheduler concerns.
- **The moot geist closes the style half, not the reasoning ceiling (2026-06-04).**
  The line is mechanistically quantified: restyling-only adaptation recovers ~100%
  of style and safety but only ~53-62% of math-reasoning
  ([decomposition](https://arxiv.org/abs/2502.04602)); fine-tuning uses pretrained
  knowledge rather than injecting facts, and RAG beats unsupervised fine-tuning
  decisively ([Ovadia](https://arxiv.org/abs/2312.05934),
  [Gekhman](https://arxiv.org/abs/2405.05904)). Translation is fine-tuning-friendly
  enough that a small fine-tuned MT model reaches frontier-level automatic-QE on
  high-resource pairs ([ALMA-R](https://arxiv.org/abs/2401.08417); metric-circular,
  human parity unproven). Verdict: the geist is **LoRA + RAG** (small fetchable
  adapters preserving base reasoning, RAG for glossary/world-knowledge), carrying
  manga's style side; the reasoning side (cultural nuance, idiom, ambiguity,
  document coherence, puns) stays base-bound, so the hero refiner persists for
  reasoning-dominated chapters.
- **Communal big models: hosting is ready, training is the frontier (2026-06-04).**
  Sharding makes RAM aggregate-swarm not per-node (Petals hosts 70B-176B over the
  internet), and the WAN-latency objection dissolves for async work. Low-comm
  training ([DiLoCo](https://arxiv.org/abs/2311.08105)) cuts communication 50x-500x
  and has trained 10B-32B over the real internet across continents
  ([INTELLECT-1](https://arxiv.org/html/2412.01152v1),
  [INTELLECT-2](https://arxiv.org/html/2505.07291v1)). Two hard limits: DiLoCo is
  data-parallel so it solves bandwidth not RAM ("nobody has the RAM" training needs
  sharded + low-comm, unproven end-to-end); and every run used curated datacenter
  nodes with no working contribution-verification against poisoned gradients or
  free-riding (the TOPLOC-verifies-untrusted and INTELLECT-2-matches-QwQ-32B claims
  were both refuted). Mere's trust rings map onto the curated-vs-untrusted gap, and
  its tessera + verification ladder are the missing verification layer.
- **Killed claims, do not assert:** probabilistic proof-of-learning ~1350% more
  efficient than replication (refuted); the "Verifiability Trilemma" framing
  (refuted); erasure coding giving better durability at lower expansion than
  replication (refuted); TransAgents beating GPT-4/human references on literary
  translation (refuted, it wins only on subjective preference while losing on
  reference metrics); "diversity always hurts MoA" and "tokenizer mismatch blocks
  composition" (both refuted); the strong "emergence is purely a metric artifact"
  framing (contested, with translation a possible genuine-emergence counterexample);
  and from the LoRA pass, "reasoning is trainable via more fine-tuning data" and
  "fine-tuning injects world-knowledge as well as RAG" (both refuted).

---

## Pitfalls

- **The tessera-slash substitution holds only for repeat players.** Every imported
  economic guarantee assumes seizable cash stake; reputation substitutes only under
  the Klein-Leffler condition, so lean on the tier ladder, not rep alone, at the
  endgame, for newcomers, and under correlated multi-moot exit.
- **Buyability creep through credits.** A single floating credit unit is one
  careless feature from money. The per-counterparty directedness, the
  spend-bounded-by-rep rule, and non-convertibility to tessera are load-bearing.
- **Streaming across strangers.** Do not stream activations or shard a model across
  the trust edge; the WAN bandwidth/latency cost is the chokepoint and it needs a
  central planner. Async bounties only.
- **Polished consensus is not faithful.** Equal-weak peers converging amplify shared
  bias and drift from the source; collaboration helps only with a strictly stronger
  participant.
- **Tournament waste and silent truncation.** First-valid open bounties race;
  `log` what was dropped rather than implying full coverage.
- **Concord transitivity.** One-hop is the safety rule; honouring credits or
  reputation past one hop relaunders down a chain of agreements.

---

## Progress

### 2026-06-04

- Consolidated from the two 2026-06-03 briefs (unified resource banking + personal
  compute mesh), which had grown by accretion across a long design conversation and
  three deep-research passes. Merged into one model under the toolkit-not-prescription
  thesis (Mark): Mere supplies the rails and defaults, the asker/moot picks the
  strategy per bounty. De-duplicated the shared trust-ring, bounty, verification, and
  manga-case material; collapsed the per-turn progress log into this entry; preserved
  every verified finding and citation. The two originals were moved to
  `archive_docs/2026-06-04_resource_coordination_merge/`. No code.
- Resolved the last open item (the LoRA style-vs-reasoning / moot-geist gap) with a
  fourth verified pass (17/25 confirmed): the line is real and quantified, the geist
  closes translation's style half and translation is fine-tuning-friendly, but the
  reasoning ceiling stays base-bound, so the geist is LoRA + RAG (not full-FT, not
  fact-baking) and the hero refiner stays for reasoning-dominated chapters. Folded
  into §6, §12, Findings, and the killed-claims list. Added (Mark's follow-up) a
  "when a virtual model does work" treatment to §6: the ceiling is escaped by
  running one genuinely-large model distributed (sharding / MoE / speculative) or by
  trading the swarm's parallel compute for quality on verifiable, decomposable, or
  search-amenable tasks (best-of-N + verifier), with translation's mild lever being
  generate-many + QE-rerank. Fixed three markdown-lint nits (table separator, two
  wrapped lines starting with "+").
- Investigated the communal big-model dream (Mark): a fifth verified pass (19/25
  confirmed) reopened it. Hosting a model nobody can hold is reachable now for async
  work (Petals sharding = aggregate-swarm RAM, latency dissolves async); training
  over WAN is demonstrated to 32B (DiLoCo / INTELLECT-1/2) but DiLoCo solves
  bandwidth not RAM and every run used curated nodes with no contribution
  verification (an open problem the trust rings + tessera map onto). Revised rung 4
  (interactive vs async), added a "Communal big models" subsection to §6, a Finding,
  an open question, and milestone 9 (a Mere-native Petals on Burn + p2panda; Petals
  confirmed MIT, so a portable reference).
- Slotted Rhai in (Mark): it is the logic-layer sandbox the design needed,
  deterministic, browser-safe, and capability-scoped (a script reaches only granted
  host functions, which *is* the Plan-9 namespace at the logic layer). Sharpened §8
  with the code-vs-data split (ML jobs run the worker's own Burn kernels on
  namespace-bounded untrusted data, so untrusted *logic*, not arbitrary code, is what
  needs sandboxing, and Rhai handles it including in-browser), tied the toolkit
  thesis to Rhai-scripted strategies in §0, and largely resolved the outer-ring
  isolation open question.
