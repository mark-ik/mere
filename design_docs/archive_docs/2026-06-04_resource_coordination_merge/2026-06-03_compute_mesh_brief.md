# Personal Compute Mesh (orrery as cluster)

**Date**: 2026-06-03
**Status**: Proposal (design probe)
**Scope**: Pool the power of all of one user's devices into a single p2p mesh so
any device can draw on all of them, then widen that mesh outward to kith, kin,
and moot as a *permissions* question rather than a *market* one. This is the
inner ring the [resource banking brief](2026-06-03_resource_banking_brief.md)
stepped past: inside a trust domain, sharing compute is scheduling + permissions
+ isolation, not verification + economy. Build inner-out; the banking economy is
the outer shell. Grounded in SHARY (NOMS 2025), exo, cake, and Mere's own
substrate.
**Related**:

- [`2026-06-03_resource_banking_brief.md`](2026-06-03_resource_banking_brief.md) — the outer shell. Its bounty + T0-T3 verification ladder + credit economy engage only at the low-trust rings this brief's mesh widens into. The two briefs are one model: mesh inner, banking outer.
- [`2026-05-10_geist_models_brief.md`](2026-05-10_geist_models_brief.md) — geist training/inference is the mesh's first heavy workload; its "Petals-style sharding for big models" is exactly §5 here. Tessera-as-compute-credit lives in the banking brief's outer ring.
- [`../implementation_strategy/2026-05-07_event_dag_substrate_brief.md`](../implementation_strategy/2026-05-07_event_dag_substrate_brief.md) — §8.6 "the user is a one-member moot" is the latent data mesh this brief extends to compute; §8.8 is the capability stack that becomes the sharing layer (§6).
- [`../implementation_strategy/2026-06-03_actor_constellation_plan.md`](../implementation_strategy/2026-06-03_actor_constellation_plan.md) — a mesh job is a compute actor (P6) whose recipient is a remote *owned* device; the armillary request-to-result message just travels over the personal p2panda space.
- [`../implementation_strategy/2026-06-01_p2panda_substrate_spike_plan.md`](../implementation_strategy/2026-06-01_p2panda_substrate_spike_plan.md) + [`2026-06-02_logsync_sync_as_projection_plan.md`](../implementation_strategy/2026-06-02_logsync_sync_as_projection_plan.md) — the live transport (`P2pandaTransport`, iroh underneath) + LogSync the mesh rides.

---

## 0. The reframe

The banking brief solved the hard case first, the stranger market, where an
asker pays an untrusted provider, which is why it needed the whole T0-T3
verification ladder and a credit economy. [SHARY](https://arxiv.org/abs/2501.18840)
(NOMS 2025), a deployed system for sharing scarce GPUs and programmable switches
across a research federation, shows the inner case needs none of that. It is a
reservation system where "permissions to login to the switch OS are dynamically
granted and removed based on the reservations," isolation is containers (Incus)
and time-slots, and the only token is a politeness nudge to release idle
reservations. It works because the participants already trust each other, so the
problem collapses to scheduling + permissions + isolation.

So the real shape is **trust-graduated rings**, and the verification economy is
the outer shell, not the core. Build inner-out.

---

## 1. Trust-graduated rings

| Ring | Trust | What it needs |
|---|---|---|
| Your own devices | total | scheduling + permissions + isolation only (SHARY-shaped, no economy, no verification) |
| Kith / kin | high | a capability grant + light sandbox; reputation optional |
| Moot members | medium | capability + tessera + T1 spot-check |
| Strangers | low / none | the full bounty + T2/T3 verification + banking ladder |

The trusted mesh is immediately useful with zero economy. Verification and
credits are added only at the rings where trust runs out, and each step out
relaxes a capability and tightens a check.

---

## 2. The personal mesh is already latent

"Use the power of all your devices from any of them" is the **orrery as a
cluster**, and the substrate is already here. The event-DAG brief §8.6 frames
multi-device as "the user is a one-member moot": every device subscribes to the
user's Personal space (`SpaceId::Personal(master_pubkey)`), events flow into one
DAG, replicate via LogSync, and materialize as projections. That is a synced
*data* mesh today.

The extension is data to compute. A device posts a compute request into the
personal space; any capable device claims it and returns the result. Because it
is all one user, there is nothing to verify. This is the same shape as the actor
constellation: a compute actor (P6) whose recipient happens to be your
workstation instead of a local thread, with the armillary request-to-result
message travelling over the personal p2panda space. "Your orrery is your moot"
becomes "your orrery is your cluster."

The **product** north-star is Syncthing: all your devices feel like one personal
substrate, with nothing to configure. But compute is more than sync. It adds
placement, cancellation, privacy, determinism, result verification, and
thermal/battery policy, so Syncthing is the *emotional* target, not the
architecture. For the architecture the right ancestor is **Plan 9**, whose deeper
lesson is not "remote CPU" but **per-process namespaces**: a job sees only what it
was granted, never the whole machine (see §6 and §8).

---

## 3. Build on what we have (p2panda + Burn)

An "exo in Rust" has two genuinely hard substrates, and exo and cake each built
both themselves. Mere already has both:

- **Networking + discovery + sync.** **p2panda** (iroh underneath, reached
  through it): `P2pandaTransport`, p2panda-net's QUIC + hole-punching, mDNS +
  random-walk discovery, RBSR + gossip, LogSync. exo wrote its own discovery;
  cake is essentially LAN-bound via mDNS. Mere's substrate already roams across
  networks, so a phone on cellular reaches a home workstation through NAT.
- **A cross-platform inference runtime.** **Burn-wgpu**, which runs everywhere
  wasm32 ships, including the browser via WebGPU, and runs "from embedded devices
  to large GPU clusters" ([Burn](https://burn.dev/blog/)). That is the per-node
  compute and the tensor primitives. Its distributed features are a *training*
  story, not an inference one (see §5).

So the mesh is **not a standalone project**. It is the compute-mesh layer of the
orrery, the compute sibling to cheesecloth storage. The networking and runtime
exist; only the orchestration on top is new (§5).

---

## 4. What the mesh does

- **An adapter per resource kind.** SHARY's load-bearing pattern is "an
  adaptation layer to drive the specific management tools per resource." Mere
  wants a `MeshResource`-style trait with per-kind adapters: GPU job
  (Burn-wgpu), inference (model serving), embeddings, storage pin (iroh-blobs,
  the cheesecloth path). Adding a resource kind is one adapter, not a core
  change.
- **Idle detection + dynamic reallocation.** Route work to whichever owned device
  is awake and free, the cheesecloth instinct applied to compute. SHARY's "smart
  monitoring ... can distinguish between development, batch workloads, and periods
  of inactivity" is the lesson.
- **A scheduler with owner priority, from day one.** Your own foreground use
  always wins: an owned device keeps **absolute reclamation priority** over its
  own GPU/CPU. Guest and kith jobs therefore run on **revocable leases** with real
  preemption, and each job declares a **checkpoint class**: interruptible (kill and
  re-dispatch), checkpointable (snapshot and resume elsewhere), or
  non-interruptible (must finish or fail). Owner reclamation is a *clean
  cancellation*, not a worker lapse, so a preempted guest takes no rep penalty (it
  maps to the bounty grammar's clean-handoff, never `CommitmentLapsed`). This is
  scheduler design, not a later UX toggle; the reservation layer itself stays light
  (queue + lease + heartbeat), lighter than SHARY's calendar because the personal
  case has one owner.

---

## 5. Inference strategy: route first, shard last

The mesh's value is not mainly cramming one model across devices. Pipeline
sharding pays a per-token network tax, since every generated token traverses the
whole pipeline and so carries one network hop per stage, which consumer links
punish into low-single-digit tokens/sec. So sharding is the last rung, not the
default. The ladder, in priority for Mere:

1. **Route the whole job (the backbone).** The model fits on *some* mesh device;
   send the whole inference there and stream tokens back. Embarrassingly parallel,
   zero per-token network tax. This is milestone 1, and it covers most real use.
2. **Throughput / data parallelism.** For many independent jobs (embedding a
   corpus, RAG over many docs, distillation-corpus prep, a moot serving many
   askers, a hundred members translating a hundred chapters), spread *distinct*
   jobs across devices rather than one job across devices. This is where all the
   devices' power actually pays, and where Mere's RAG-heavy geist workload lives.
   It is BOINC-shaped: an async work-list of independent tasks, pulled and
   committed, never a constant stream. At the trust edge this *is* a bounty queue
   (banking brief), which is why mesh sharing stays async and commitment-oriented
   rather than live-scheduled. The 2026-06-04 research confirms this is the right
   answer for the manga case: the coordinator stays minimal (an idempotent
   content-addressed work-list with claim/heartbeat/deliver, reassigning any bounty
   whose heartbeat lapses, validating by content address plus optional redundant or
   quorum re-run, the BOINC replication pattern), because there is zero cross-device
   tensor traffic to schedule. A hero device and a heavy moot scheduler are both
   unnecessary for an embarrassingly-parallel batch.
3. **Speculative decoding on a fast link.** A small draft model on the weak or
   local device proposes K tokens; the big model on a strong device verifies them
   in one batched pass, amortizing the round-trip over K tokens. The 2026-06-04
   research qualifies this sharply: cross-device speculative decoding is real
   ([DSSD](https://arxiv.org/abs/2507.12000), [FlowSpec](https://arxiv.org/abs/2507.02620),
   edge-cloud drafts), but the measured speedup is modest (~1.2x-1.75x, sometimes
   below 1x), and it holds only on co-located edge or datacenter links with top-K
   logit compression. It has not been shown to survive consumer WAN across
   strangers. So this is an **inner-ring** optimization (your own devices on a LAN,
   or kith on a good link), not a trust-edge technique; across the trust edge,
   rung 2 (data-parallel bounties) is the answer.
4. **Sharded pipeline inference (frontier escape hatch).** Only when a model
   genuinely exceeds every device, slow tokens are acceptable, and the link is
   fast. If the model is MoE (Mixtral, DeepSeek), shard by *expert* rather than
   layer, so only the active expert's device is hit per token, far more
   network-frugal than dense pipelining. This is the exo/cake/Petals pattern and
   the one genuinely new orchestration if built, but it is the rare lever, not the
   architecture. The 2026-06-04 research hardens why: inter-device KV-cache and
   activation transfer is a first-class cost, cheap only on datacenter InfiniBand
   (~5-8ms) and the chokepoint on WAN ([Splitwise](https://arxiv.org/abs/2311.18677),
   [HexGen-2](https://arxiv.org/abs/2502.07903)); strategic sharding needs a central
   placement planner (a constraint or max-flow solver, the opposite of an ad-hoc
   pull queue); and pipelined speculative sharding collapses at the low utilization
   typical of small personal meshes ([FlowSpec](https://arxiv.org/abs/2507.02620)).

The deeper reframe: geist's value is *context* (your flora), not parameter count,
so the strongest move is usually to *not need* a model too big to fit. Lean on
RAG + a personal LoRA + speculative decoding to make a fits-on-one-device model
excellent, and spend the mesh on the parallel rungs (1-3). Rung 4 is the "I want
the 671B and I will wait" lever.

**On composing models for quality (the swarm question).** A swarm of small models
does *not* reliably conjure capability beyond its strongest member (2026-06-04
research). Open-model swarms can beat a GPT-4-class model on general-chat
leaderboards (Together [Mixture-of-Agents](https://arxiv.org/abs/2406.04692): 65.1%
vs 57.5% on AlpacaEval 2.0), but that win is metric-fragile and never shown on
translation; aggregating samples from the single best model (Self-MoA,
[arXiv 2502.00674](https://arxiv.org/abs/2502.00674)) beats mixing weaker ones by
~6.6 points, so the gain is proposer *quality*, not diversity, and model fusion
still trails its best member on that member's strength
([FuseLLM](https://arxiv.org/abs/2401.10491)). On translation a single strong model
wins human quality 5 of 6 times at one fifth to one fifteenth the token cost of a
multi-agent workflow ([arXiv 2505.01560](https://arxiv.org/abs/2505.01560)), and
translation may even be genuinely scale-gated rather than a metric artifact. The
one composition with evidence of net quality gain is **draft-then-refine where the
refiner is strictly stronger** than the drafter (a larger model or a hero-frontier
node), and it pays off mostly when the drafter is weak
([TEaR](https://arxiv.org/abs/2402.16379)); equal-weak peers refining each other
just amplify self-bias. So data parallelism (rungs 1-2, each device runs the
strongest model it can hold) is the right default for quality and throughput, and a
hero-frontier refiner is reserved for a single hard chapter at that 5-15x cost.
The shape one might picture, every model referencing the others' takes and
converging on a single polished answer, *is* Mixture-of-Agents; its danger for
translation is that the consensus grows more *fluent* and more *preferred* while
drifting from the source (reference metrics fall, content is sometimes dropped),
so a polished consensus is not a faithful one. Collaboration helps only when one
participant is strictly stronger, supplying capability the others lack; equal-weak
peers converge on shared confident error.

**The swarm is heterogeneous, and that is the point.** A real 100-laptop swarm is
a mix of device classes, so it *has* a strongest member, which is exactly the
strict-refiner asymmetry collaboration needs. The productive shape is
capability-tiered: weak devices draft and bulk-translate in parallel (throughput),
and the strongest available model refines the hard parts and sets the quality
ceiling (draft-then-refine, the one net-positive composition). The hard constraint
holds: the swarm's ceiling is essentially its strongest member, since composition
does not reliably exceed it, so heterogeneity buys throughput plus a refiner, not
capability beyond your best node; a chapter needing frontier scale needs a
frontier-class node (a hero) in the swarm. The one documented way to beat the best
member is *orthogonal specialization* (members strong at different sub-skills), and
for manga that is plausible (honorifics, onomatopoeia, dialogue register), but the
measured gains elsewhere are marginal, so treat it as an experiment, not a given.

**Tasks carry a weight-set; the moot's geist is the shared quality lever.** A
node's capability is its base model *times* the LoRA adapters its owner has loaded
(geist brief), so the swarm is heterogeneous by adapter as well as by size, and a
bounty's result-spec can name a required weight-set: a base-model lineage plus a
stack of content-addressed adapters, the moot's geist among them. Adapters are
small (10-100MB) BLAKE3-addressed engrams, so a worker fetches a missing one via
iroh-blobs once and pins it thereafter (a cache-hit collapses to a pin, the
banking brief's property), and the per-job namespace (§8) carries the weight blobs
as scoped inputs. Eligibility falls out: a node may claim a weighted bounty only
if it has a *compatible* base (the geist brief's strict LoRA-stacking envelope:
base bytes, tokenizer, prompt template, target modules, format, quantization) and
can fetch the stack. The payoff is real and does not hit the composability
ceiling: a shared moot-trained adapter on every drafter gives consistent
terminology, honorifics, and house style across the whole swarm, which is exactly
where LoRA earns its keep (style and idiom). This is deliberate shared
*specialization*, not random ensemble orthogonality. The caveat persists though:
an adapter lifts style, not the base's reasoning, so hard chapters still want the
strongest base as the refiner. The clean shape is the whole swarm drafting with
the moot's geist loaded (consistent, parallel) and the strongest node, also
carrying the geist, refining the hard parts. A required adapter should be a vetted,
adopted moot artifact (the geist brief's adoption + canary/poisoning gates), and
granting one for a job is a capability in the namespace.

**Prior art for rung 4**, with licenses that decide how we may use each:

- **[exo](https://github.com/exo-explore/exo)** (Apache 2.0): pure-p2p, dynamic
  partitioning by topology, auto-discovery. Python + MLX-centric, so a port is a
  rewrite anyway, but the permissive license means we may read and borrow its
  algorithm and topology technique freely.
- **[cake](https://github.com/evilsocket/cake)** (FAIR license, *not*
  OSI-permissive): the closest existing Rust system, shards transformer blocks
  across iOS/Android/macOS/Linux/Windows on CUDA/Metal/Vulkan/CPU, master streams
  weight shards (zstd + CRC32). FAIR forbids business use without a signed
  agreement, so it is **reference-only**: study the shape, do not vendor or copy.
  It is also master-worker, not pure-p2p.
- **Petals** (geist brief): the sharded-big-model-inference pattern already named
  in Mere's docs.

Burn gives the per-node compute, but its distributed support is training-only,
and that holds through the latest release. `burn-collective` (Burn 0.19) added an
all-reduce for gradient synchronization, networked over WebSockets; Burn 0.21
([May 2026, latest](https://burn.dev/blog/release-0.21.0/)) reworked it into
*differentiable collectives* (faster `to_device` and `all_reduce`) plus the
`burn-dispatch` backend selector, but in Burn's own words "the foundation is in
place," and it remains a distributed-*training* story. No release through 0.21
ships inference distribution of any kind (pipeline, tensor, or sharded), so rung 4
is fully net-new on Burn-as-per-node-runtime, transported over p2panda rather than
Burn's own collective. Rungs 1-3 need only routing plus the existing LogSync
substrate. Rung 4 is real engineering: latency over WAN hops, KV-cache placement,
partition boundaries at layer edges, and recovery mid-token, which is why exo and
cake both flag themselves experimental.

---

## 6. Sharing is the permissions layer

Widening the mesh past your own devices is exactly the permissions discussion,
and Mere already has the language: the §8.8 capability stack (structural caps,
meadowcap-shaped BLAKE3 cluster-paths + Biscuit policy over tessera facts +
Keyhive/p2panda-encryption group/key state). "Let Alice's jobs run on my mesh
for 30 days, capped at N hours, only T0/T1 work" is a capability grant, not a
market transaction. Each ring out is a looser cap plus, where trust drops, a
verification tier from the banking brief. The economy never appears until the
caps run past people you personally trust.

**Grant state is shared; enforcement is local.** A grant, lease, revocation,
heartbeat, and result receipt are all signed facts on the p2p event substrate
(the same LogSync DAG tessera and murm ride), so a grant is just one more
projection of the event DAG, like the tessera ledger itself. Every device watches
that state and enforces it locally, so no broker has to be online to police a
worker and the system stays genuinely p2p. The one-line statement of the model:

> The mesh is Plan-9-shaped: every job runs inside a capability-scoped namespace
> assembled from replicated grant state. Owned devices enforce grants locally and
> retain absolute reclamation priority over their resources.

---

## 7. Where the banking economy engages

The handoff point is precise: as long as a job runs inside a trust ring (own
devices, granted kith), it is permission-gated and free. The moment a job crosses
to medium or low trust (a moot pool, a stranger), the banking brief's machinery
switches on, the bounty, the T1-T3 verification keyed to tessera, the escrowed
credits. The mesh is the substrate; banking is the toll booth at the trust
boundary. The same compute actor, the same p2panda message, gains a verification
tier and a settlement only when it leaves the ring of trust.

---

## 8. Isolation by ring

- **Own devices:** minimal. You trust your own hardware; the actor boundary's
  failure + memory isolation is enough.
- **Outer rings (running others' jobs):** real sandboxing. Per the actor
  constellation threat model, an in-process actor boundary is failure +
  memory-safety isolation, not a confidentiality boundary, so accepting a
  stranger's compute wants a stronger box (a wasm sandbox, an OS subprocess on
  native, or a container). On the browser target, a tab contributing to others is
  capped at light + self-verifying work for the same reason (banking brief §4).

So isolation strength tracks the ring, exactly as verification does.

**The Plan 9 namespace is the isolation primitive, not just the sandbox.** A job
never gets ambient access to "my machine." It receives a **per-job namespace**
assembled from capabilities: its input blobs, the model weights, scratch space, an
output sink, optionally a metrics channel, and nothing else. That maps cleanly
onto iroh-blobs for content-addressed inputs, weights, and outputs, and
iroh-docs plus caps for the scoped grant, so the namespace is built from the
substrate Mere already has. The sandbox (wasm, subprocess, container) contains the
*code*; the namespace bounds what the code can even *name*. Both tighten by ring.

---

## 9. Mere-original edges over exo and cake

- **A browser tab can be a node.** Burn-wgpu reaching WebGPU means a PWA tab
  contributes small cluster capacity. Neither exo nor cake runs in-browser.
- **Pure-p2p, like exo, not cake's master-worker**, which fits the
  no-single-controller ethos and the p2panda substrate.
- **Permissions and trust rings come for free** from the §8.8 cap stack. exo and
  cake assume a trusted LAN and stop there.

---

## 10. Configurable parameters (per user / per ring)

- Which of my devices participate, and for which resource kinds.
- Idle threshold at which a device offers itself.
- Per-ring capability defaults (own = full; kith = T0/T1; moot = tessera-gated).
- Sharding policy: shard a big model across devices, or route the whole job to
  the single most-capable device (the cheaper choice when one device can hold it).
- Scheduler policy: owner-reclamation priority (always on for owned devices),
  lease duration + preemption for guest/kith jobs, and the allowed checkpoint
  classes (interruptible / checkpointable / non-interruptible) per ring.

---

## 11. Open questions

- **Burn's sharded-inference maturity. RESOLVED (2026-06-03): there is none,
  checked through the latest release.** Across 0.19 (distributed training landed),
  0.20, and 0.21 (May 2026, latest: differentiable collectives + `burn-dispatch`,
  but Burn's own "the foundation is in place"), Burn ships distributed *training*
  only; it has no pipeline/tensor-parallel or sharded *inference*. So milestone 5
  is fully net-new orchestration on Burn-as-runtime over p2panda, not "turn on
  Burn distributed." The latency-budget prototype (next item) is the remaining
  empirical unknown.
- **When is sharding worth the latency?** Pipeline-parallel over WAN may lose to
  just routing the whole job to the best device. Needs a measured budget; sharding
  is likely a LAN-or-fast-link feature, routing the WAN default.
- **KV-cache placement + mid-inference recovery** when a device drops a token in.
- **Isolation choice for outer rings** (wasm sandbox vs subprocess vs container)
  and exactly what a browser node may safely run for others.
- **Where the mesh lives as a crate** in the workspace, and how its scheduler
  relates to the actor constellation's P6 compute actors (same machinery, wider
  recipient).
- **Can a swarm of small models match a frontier model? RESOLVED (2026-06-04):
  not for translation.** Open-model swarms beat GPT-4-class only on general-chat
  leaderboards and the win is metric-fragile; composition does not exceed its
  strongest member; on translation a single strong model wins at a fraction of the
  cost. Data parallelism (strongest model per device) is the default, with
  draft-then-refine by a strictly stronger refiner the only net-positive
  composition (see §5). Residual: no test exists on a *true heterogeneous* swarm
  for hard literary/manga translation, and translation may be genuinely scale-gated.
- **How light can the bounty coordinator be at ~100 nodes** while still validating
  against cheating and reassigning dropped workers? BOINC uses redundant
  computation + quorum; the overhead for a content-addressed p2panda pull-queue is
  unquantified (both passes deprioritized this; still open).
- **Do Petals/exo/cake fit an ad-hoc bounty model** or assume a persistent
  coordinated overlay, and what is real inference throughput over consumer internet?
  Unverified by either pass.
- **When does a single hard chapter justify the fallback?** The 2026-06-04 pass
  points to draft-then-refine with a hero-frontier refiner (at ~5-15x token cost)
  over sharding; the open part is the crossover where the quality gain on a hard
  chapter (idiom, honorifics, dense panels) is worth that cost, and whether a
  capability floor exists below which no local model suffices.
- **Does a base + domain-LoRA beat a larger generic base on hard translation**, or
  does the adapter only lift style/consistency while the reasoning ceiling stays at
  base scale? Where is the style-vs-reasoning line for manga (idiom and honorifics
  vs cultural-nuance reasoning)? Determines how far the moot geist closes the gap to
  a frontier node.

---

## 12. First milestones (done-conditions, not dates)

1. **Compute actor over the personal space.** One owned device posts a compute
   request into `SpaceId::Personal`, another claims it and returns the result via
   LogSync. *Done when* a job submitted on the laptop runs on the workstation and
   the result lands back, no economy involved.
2. **Resource adapter + first kind.** A `MeshResource` trait plus one adapter (a
   Burn-wgpu job or an embeddings batch). *Done when* a second resource kind is
   one adapter, not a core change.
3. **Idle routing.** Route a job to whichever owned device is awake and free.
   *Done when* a sleeping device is skipped and an idle one picks the work up.
4. **Capability-gated sharing to one kith peer.** Grant a cap, run a T0 job for a
   friend on your mesh. *Done when* the grant scopes what they may run and
   expires.
5. **Two-device sharded inference (frontier escape hatch, rung 4).** A model too
   big for one owned device runs across two, Burn-based, over p2panda. The rare
   lever, not the backbone (routing + throughput are milestones 1 and 3;
   speculative decoding is the interactive-big-model path, a research item).
   *Done when* the shard produces correct tokens and survives one device dropping.

Each is independently useful; the mesh is valuable at milestone 1, long before
sharding or sharing.

---

## Findings

### 2026-06-03

- **SHARY validates the inner-out build order.** A deployed scarce-compute sharing
  system uses reservation + permissions + isolation and no verification market,
  because it lives in a trust domain. The economy is the outer ring, not the core.
- **An "exo in Rust" is mostly already in Mere's hands.** The two hard substrates
  (cross-platform runtime, p2p networking) are Burn-wgpu and p2panda; only the
  sharded-inference orchestration is new. The mesh is the orrery's compute layer,
  not a standalone project.
- **License landscape is inverted.** The permissive prior art (exo, Apache 2.0)
  is Python; the Rust prior art (cake) is FAIR-licensed and commercially
  restricted, so cake is reference-only and exo is the one we may borrow technique
  from. Either way the clean path is a Burn + p2panda build.
- **The mesh and the banking brief are one model.** Mesh inner (permissions),
  banking outer (verification + economy), the handoff at the trust boundary.
- **Burn is the runtime, not the distributor (2026-06-03).** Verified across Burn
  0.19 through 0.21 (the May 2026 latest): the only distributed feature is a
  training-oriented collective (an all-reduce, reworked into differentiable
  collectives in 0.21, with `burn-dispatch` for backend selection). Burn's own
  framing is "the foundation is in place," and there is no sharded inference in
  any release. So the mesh's distributed-inference layer is fully Mere's to build
  over p2panda, with Burn supplying per-node compute plus quantization for fitting
  shards on small devices. Milestone 5 is bigger than "wire up Burn distributed."
- **The mesh is Plan-9-shaped (Mark, 2026-06-04).** The load-bearing borrow from
  Plan 9 is per-process namespaces, not remote-CPU: a job sees only its
  capability-scoped namespace (inputs, weights, scratch, output, metrics), never
  the whole machine. Grant/lease/revocation/heartbeat/receipt are signed facts on
  the substrate (one more event-DAG projection, like tessera), enforced locally by
  each device, so the system needs no online broker. Owner foreground use holds
  absolute reclamation priority, which puts leases, preemption, and checkpoint
  classes in the scheduler from day one; owner reclamation is a clean cancellation,
  not a guest lapse. Syncthing is the product feel, not the architecture.
- **Cross-device inference research validates the bounty model (2026-06-04).** A
  verified pass (18 of 25 claims confirmed) found: cross-device speculative
  decoding is real but modest (~1.2x-1.75x, sometimes below 1x) and fast-link-only,
  with no evidence it survives consumer WAN across strangers; sharded inference's
  KV-cache transfer is cheap only on InfiniBand and is the WAN chokepoint; and
  strategic sharding needs a central planner incompatible with ad-hoc pull. For
  embarrassingly-parallel batch (the manga case) the verified recommendation is
  BOINC-style data parallelism over a light bounty queue, which is exactly Mere's
  async-bounty-at-the-trust-edge model. The swarm-of-small-models-vs-frontier
  question is resolved in the next finding.
- **A swarm of small models is not a frontier substitute, especially for
  translation (2026-06-04).** Verified (19 of 25 claims): open-model swarms beat
  GPT-4-class only on general-chat leaderboards and the win is metric-fragile
  (Self-MoA beats heterogeneous MoA, so the gain is model quality, not diversity);
  fusion does not exceed its strongest member; on translation a single strong model
  wins human quality 5 of 6 times at one fifth to one fifteenth the cost of a
  multi-agent workflow, and translation may be genuinely scale-gated. The only
  composition with evidence of net quality gain is draft-then-refine with a
  strictly stronger refiner (Mixture-of-Agents is this pattern; equal-weak peers
  converging on a consensus grow fluent while drifting from the source). This
  confirms data parallelism (strongest model per device, one chapter per bounty) as
  the default, with a hero-frontier refiner reserved for a single hard chapter.
- **A node's capability is base times adapters; tasks can require a weight-set
  (2026-06-04).** Building on the geist brief: the swarm is heterogeneous by LoRA
  adapter as well as by base size, and a bounty can name a required weight-set
  (base lineage + content-addressed adapter stack, the moot's geist among them).
  Adapters are small BLAKE3 engrams fetched via iroh-blobs and pinned (cache-hit =
  pin); the per-job namespace carries them; eligibility is gated by the LoRA
  compatibility envelope. A shared moot adapter lifts terminology/style/idiom
  consistency across the swarm (deliberate specialization, no composability
  ceiling), but not the base's reasoning, so hard chapters still want the strongest
  base as refiner. Required adapters are vetted, adopted moot artifacts; granting
  one is a namespace capability.

---

## Pitfalls

- **Do not let the mesh assume a LAN.** cake's mDNS-bound master-worker is the
  shape to avoid; p2panda's roaming p2p is the point.
- **Sharding is not free.** Pipeline-parallel over a slow link can lose to routing
  the whole job; measure before defaulting to shard.
- **Outer-ring isolation is a real cost.** Running a stranger's code safely needs
  a sandbox the in-process actor boundary does not provide; do not blur the
  trusted-mesh ergonomics into the untrusted case.
- **Burn's distributed story is young.** Treat cross-peer sharded inference as a
  prototype-gated unknown, not a given.

---

## Progress

### 2026-06-03

- Brief drafted from the SHARY (NOMS 2025) reading and the exo/cake/Burn license +
  capability verification. Established the trust-graduated rings as the organizing
  model, the orrery-as-cluster framing on the existing p2panda data mesh, the
  build-on-Burn-plus-p2panda argument, the sharded-inference orchestration as the
  one new piece, and the §8.8 cap stack as the sharing layer. Positioned as the
  inner ring to the resource banking brief's outer shell. No code.
- Verified Burn's distributed surface (thread 2): training-only
  `burn-collective` all-reduce, no sharded inference, so milestone 5's
  orchestration is net-new over p2panda. Resolved the Burn open question
  accordingly; the latency-budget prototype remains the empirical next step.
- Correction (same day): the Burn check first cited only the 0.19 release; on
  review, verified through 0.21 (May 2026, latest: differentiable collectives +
  `burn-dispatch`). The training-only-no-sharded-inference conclusion holds across
  0.19, 0.20, and 0.21, but the citations now reference the current version, not a
  stale one.

### 2026-06-04

- Reframed §5 from sharding-centric to an inference-strategy ladder (route first,
  throughput, speculative decode, sharding last as a frontier escape hatch), per
  Mark: sharding's per-token network tax makes it the last rung, and mesh sharing
  at the trust edge is async bounty-shaped, not a constant stream. Relabelled
  milestone 5 accordingly. Launched research on speculative decoding + swarm
  computing to deepen rungs 3-4 and the 100-laptop manga-translation case (data
  parallelism vs sharding vs swarm-of-small-models; how light the coordinator can
  stay).
- Folded in Mark's Plan 9 framing (from an earlier discussion): grant state
  shared / enforcement local (no online broker; a grant is one more event-DAG
  projection like tessera) into §6 with the "mesh is Plan-9-shaped" key line; owner
  absolute-reclamation priority with revocable leases, preemption, and checkpoint
  classes into the §4 scheduler "from day one" (owner reclaim = clean cancellation,
  not a guest lapse); the per-job capability-scoped namespace (Plan 9's deeper
  lesson over remote-CPU, built from iroh-blobs + caps) into §8; the
  Syncthing-north-star framing into §2; plus the §10 scheduler config and a
  Finding. Ultracode is off; no new workflow.
- Folded in the verified Topic-1 research (speculative decoding + disaggregation,
  18/25 claims confirmed): qualified rung 3 (cross-device SD is real but modest and
  fast-link-only, so an inner-ring optimization, not trust-edge), hardened rung 4
  (KV/activation transfer is the WAN chokepoint, sharding needs a central planner,
  pipelined SD collapses at low utilization), and added the verified manga
  coordination answer to rung 2 (BOINC-style data-parallel bounty queue, light
  coordinator). The Topic-2 half (swarm-of-small-models vs frontier; BOINC
  coordinator overhead; scale to 2-4 devices) was left unverified by that pass, so
  launched a focused follow-up research workflow.
- Folded in the verified Topic-2 research (swarm composability, 19/25 confirmed): a
  swarm of small models is not a frontier substitute (composition does not exceed
  its strongest member; swarm-beats-frontier wins are general-chat-only and
  metric-fragile; on translation a single strong model wins at 5-15x less cost, and
  translation may be scale-gated). Added the composition treatment to §5, resolved
  the swarm open question, and reframed the single-hard-chapter fallback as
  draft-then-refine with a stronger refiner. Refined for Mark's points: the
  collaborative-consensus shape is Mixture-of-Agents (polish is not faithfulness),
  and a *heterogeneous* swarm is the working case, a capability-tiered draft (weak,
  parallel) then refine (strongest, sets the ceiling), still ceiling-bound by its
  strongest member. Secondary coordination questions (BOINC overhead,
  Petals/exo/cake topology) remain open.
- Extended for Mark's weight-set point: a node's capability is base times loaded
  adapters, so a bounty can require a weight-set (base lineage + content-addressed
  adapter stack, the moot's geist included), fetched via iroh-blobs and pinned,
  eligibility gated by the LoRA compatibility envelope. The moot adapter is the
  shared domain-quality lever that sidesteps the composability ceiling (style, not
  reasoning). Added to §5, a Finding, an open question, and the banking brief's
  `ResultSpec::Compute`.
- DOC_README index updated.
