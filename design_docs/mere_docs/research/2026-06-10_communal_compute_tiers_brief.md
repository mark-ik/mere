# Communal Compute Across the Tiers (volunteer computing, time banks, and the moot ladder)

**Date**: 2026-06-10
**Status**: Research brief. The social and tier-scale layer over the
[resource coordination brief](2026-06-04_resource_coordination_brief.md), which
owns the mechanics (trust rings, bounty grammar, two ledgers, verification
tiers, storage durability, inference rungs, namespace isolation; note its
2026-06-10 scripting correction: Rhai/Rune dropped for Rust + JS + declarative
policy). This
brief answers the question the mechanics left implicit: how do moots actually
harness people's personal devices, share that power outward through kith and
community, and scale to mootholds and coalitions with big ideas and big
computation budgets?
**Related**: [moot tiers + voluntary hosting](../implementation_strategy/2026-05-07_moot_tiers_and_voluntary_hosting_brief.md)
(the tier framework this maps onto),
[geist models](2026-05-10_geist_models_brief.md) (the communal artifact),
[moot constitution](../implementation_strategy/2026-06-06_moot_constitution_brief.md)
(where the policy presets below live).

---

## 1. How fleshed out is the existing model? (honest assessment)

More than remembered. The resource coordination brief carries five
adversarially-verified research passes and a complete mechanics design:
trust-graduated rings, the bounty as the one primitive, reciprocity credits
with the tessera valve, the T0-T3 verification ladder, erasure-coded audited
storage, the route-first-shard-last inference ladder, the communal-big-model
verdict (hosting reachable now for async work; WAN training demonstrated to
32B but only on curated nodes), the Plan-9 namespace + Rhai isolation story,
and nine staged milestones. On paper, the *mechanics* are largely designed.

What is thin, and what this brief supplies:

- The rings stop at "strangers." The **moothold (t3) and coalition (t4)**
  layers have only the one-hop concord clearing rule; nothing develops what a
  federation-scale or coalition-scale compute commons *is*.
- The **volunteer-computing tradition** (the
  [catalog](https://en.wikipedia.org/wiki/List_of_volunteer_computing_projects))
  is cited only as "BOINC-shaped queue." It is fifty years of evidence about
  why people lend machines, what retains them, and how projects die.
- The **time bank** framing was never connected to the credit design.

## 2. Fifty years of volunteer computing: the evidence

### The capacity is real, and purpose mobilizes it

The catalog's active projects alone: PrimeGrid ~89k hosts, Rosetta@home ~44
PFLOPS, GIMPS ~5.3 PFLOPS, World Community Grid ~57k hosts. The defining
event is [Folding@home in spring 2020](https://en.wikipedia.org/wiki/Folding@home):
~30k devices became **over one million in two months** when COVID gave people
a reason, reaching [2.43 exaFLOPS](https://www.techspot.com/news/84832-foldinghome-project-passes-24-exaflops-more-than-top.html)
— the world's first exascale computer, more than the top-500 supercomputers
combined. By [late 2025 it sat near 25 PFLOPS](https://en.wikipedia.org/wiki/Folding@home)
(x86-equivalent): roughly a **hundredfold decay** from peak.

Lesson: the latent supply in personal devices is staggering; a big idea
mobilizes it almost overnight; and **hub-and-spoke projects have no structure
that retains it**. The surge attached to a cause, not a community, so it
evaporated with the news cycle. Moots are precisely the retention structure:
you do not "run a screensaver for a university," you compute for *your*
community's flora and geist, among people who know you.

### Why people contribute, and what the credit systems teach

The participation research ([Anderson's incentive-system paper](https://boinc.berkeley.edu/boinc_papers/credit/text.php),
the [engagement studies](https://hcjournal.org/index.php/jhc/article/view/63))
converges on three motives: supporting the science, competition, and
community. SETI@home's [team system](https://www.researchgate.net/publication/230676386_SETIhome_BOINC_and_volunteer_distributed_computing)
(teams by nationality, institution, employer) drove people to recruit friends
and family and in documented cases to buy multi-GPU machines for the
leaderboard. The social layer is not decoration; it is the engine.

BOINC credit is proto-tessera: non-monetary, standing-shaped, motivating.
But it is global and **volume-shaped**, so it rewards raw throughput and
wealthy hardware. The mechanics brief's correction is already right: tessera
from fulfilment is shaped by *reliability* (sub-linear in volume, capped,
decaying), and standing is per-moot, not global. Leaderboards return as
per-moot contribution standings rendered from the real ledger (the
no-placebo rule: actual fulfilments, never a vanity counter).

### How projects die

The defunct half of the catalog has a clear pattern: **administrator
burnout** ("discontinued because the creator lacked time"), funding or
institutional shifts, mission completion, and low-activity mergers. A
volunteer-computing project is one server, one team, one grant; when the
center tires, the commons evaporates, taking the contributors' accumulated
identity with it. The timebanking literature found the same failure
independently: overhead-heavy, staff-run time banks
[are unsustainable](https://link.springer.com/article/10.1007/s11266-022-00467-6).

The mechanics brief already holds the answer, stated for a different reason:
**grant state is shared, enforcement is local** — the coordinator is a
projection of the event DAG, and brokering is a configurable, bankable,
rotatable role. A moot's "project server" is community property that survives
any one member's burnout. This deserves to be named as the anti-burnout
property, not just a p2p nicety.

### Validation and allocation precedents

- BOINC remains alive (client 8.2.11 released June 2026) and its
  [quorum validation](https://en.wikipedia.org/wiki/BOINC_client%E2%80%93server_technology)
  (N-of-M agreement, re-issue on deadline miss, adaptive replication for
  trusted hosts) is exactly the mechanics brief's T0/T1: independent
  validation of the design.
- [Science United](https://scienceunited.org/) is the t3/t4 precedent worth
  copying: contributors pledge compute to *areas* ("biomedicine," "physics"),
  and the system routes supply to vetted projects within the area. That is a
  coalition-shaped allocation layer: members pledge once to the big idea, and
  the federation routes idle supply to member moots' queues by policy.

### Leela Chess Zero: the communal-model precedent

The most actionable single precedent in the catalog.
[LCZero](https://en.wikipedia.org/wiki/Leela_Chess_Zero) trained a
Stockfish-class network from volunteers — over
[2.5 billion self-play games, ~1M/day as of mid-2025](https://www.chessprogramming.org/Leela_Chess_Zero)
— **without distributed training**. Volunteers generate self-play *data*
(embarrassingly parallel, the mechanics brief's rung 2); a central server
fits the network and redistributes it. The hard distributed-SGD problem the
DiLoCo literature is still solving was simply avoided.

The generalization for moots: **the communal part of communal model-making is
data generation, curation, and evaluation, not gradient descent.** A moot's
geist (LoRA + RAG per the mechanics brief §6) fits this shape today: members
contribute corpus curation, ratings, eval runs, and synthetic-data bounties
across the swarm; one strong member node (or a claimed T2/T3 bounty) trains
the adapter; the adapter ships as a content-addressed engram every member
benefits from. The LCZero loop is the moot-geist loop with the trust rings
and bounty grammar Mere already designed. Distributed *training* stays where
the mechanics brief put it: a research frontier for untrusted pools, and a
curated-ring option (DiLoCo-class) for vetted mootholds that genuinely need
it.

## 3. The tier ladder, developed

The mechanics brief's rings map onto the in-product tiers; t3/t4 are where
the new design lives.

- **Personal (orrery, t0/t1).** Your own devices as one substrate: scheduling
  + namespace, owner reclamation absolute, no economy (mechanics §1/§8,
  milestone 1). The supply unit of everything above is "a household's idle
  compute."
- **Kith/kin.** Capability grants, no economy: the family pool, the friend
  with the gaming GPU, the old laptop running a mooting server. This ring is
  Mere's genuine novelty against the entire volunteer-computing tradition,
  which jumps straight from *self* to *anonymous strangers* with nothing in
  between. Family-and-friends compute sharing has essentially no prior art
  with this trust shape; spec decoding and other fast-link techniques live
  here (mechanics §6 rung 3).
- **Moot (t2): the project-community fusion.** A moot is a volunteer-computing
  project whose server is community property. The bounty board is the work
  queue; per-moot tessera + reciprocity are the credit system with the
  volume-bug fixed; the geist is the communal artifact each contribution
  improves (the LCZero loop); contribution standings are the leaderboard,
  rendered from real fulfilments. Retention comes from membership and the
  artifact, not the news cycle: the thing F@h's surge lacked.
- **Moothold (t3): the federation commons.** Three shapes, all riding
  existing mechanics: (1) **durability underwriting** — the moothold runs the
  checker/repairer cadence and guarantees a baseline erasure-coded floor for
  member moots' flora (cheesecloth at federation scale, mechanics §5); (2)
  **bounty escalation** — a bounty a moot cannot fill escalates
  moothold-wide, cleared through the one-hop concord rule (mechanics §9);
  (3) **area pledges** — the Science United shape: members pledge idle supply
  to the moothold's areas once, and policy routes it to member moots' queues.
- **Coalition (t4): big ideas, big computation budgets.** The
  volunteer-computing catalog *is* the program list reborn: climate
  ensembles, conjecture sweeps, disease pipelines, communal hosting of a
  model nobody can individually hold (the mechanics brief's milestone 9,
  Mere-native Petals), and curated-ring communal training where genuinely
  warranted. The coalition is constitution-governed (the constitution brief's
  amendment rule decides program allocation), its budget is escrowed credit
  streams from member mootholds plus the **funded-bounty lane** (mechanics
  §3: cash buys credits at the edge, never tessera), so an institution can
  fund a coalition-scale program without buying standing. Suzerainty stays
  what it is elsewhere in Mere: the outer tier holds standing with inner
  members, never control. And the F@h arc is the design target stated
  plainly: a coalition should be able to *absorb a surge* (a big idea arrives,
  a million devices show up) and *retain a community* after the surge passes.

## 4. Time banks: a policy preset, not a substrate

[Timebanking](https://en.wikipedia.org/wiki/Time-based_currency) (Edgar Cahn,
1980): one hour of service equals one hour, whoever gives it; credits are
mutual, asset-framed, and deliberately egalitarian. The fit with Mere is
precise and pleasing: the mechanics brief already leaves the credit exchange
rate **market-cleared by default but per-moot pinnable** (§3, §11). A time
bank is exactly the pinned-rate special case: a moot whose constitution
declares one device-hour equals one device-hour (optionally normalized by a
device class) inside the ring, market-cleared at its edges. Ship it as a
named constitution preset ("time bank") rather than new machinery.

The timebanking literature's failure modes, read against compute:

- **Supply-heavy stagnation** ([~54% prefer offering over requesting](https://www.researchgate.net/publication/269519106_Unequal_Time_for_Unequal_Value_Implications_of_Differing_Motivations_for_Participation_in_Timebanking)):
  for *services* this stalls the exchange loop; for *compute* it is nearly
  harmless, since idle cycles hold no resentment and storage pledges want
  surplus. Compute is arguably the best-matched good timebanking never had.
- **Equal-hour tension** (instrumental vs altruistic members): the two-lane
  design already answers it — commons reciprocity inside the ring, funded
  bounties for instrumental outsiders, tessera unbuyable in both.
- **Overhead/staff burnout**: the same disease as volunteer-computing project
  death; answered by community-owned coordination (grant state shared,
  brokering a rotatable role).
- **Tax/regulatory edges**: Finland required timebank hours be reported at
  euro value, which chilled participation. The cash-permeable funded-bounty
  lane will eventually meet the same question; flag it for whenever the lane
  carries real money. No design change now.

## 5. What this adds to the plans (nothing replaced)

- **The moot-geist loop is the first communal-compute product story**: after
  the mechanics brief's milestone 3 (data-parallel batch queue), the LCZero
  shape is reachable — data-generation/eval bounties feeding a trainer node,
  adapter shipped as an engram. Worth promoting to a named milestone when the
  mesh work starts; it needs no distributed training.
- **Area pledges** (Science United shape) join the t3/t4 feature set,
  post-economy.
- **The "time bank" constitution preset** joins the configurable-parameters
  menu (mechanics §11) as a named bundle: pinned internal rate, market edges.
- **Contribution standings UI** renders from the tessera/reciprocity ledgers
  (real fulfilments only, per the no-placebo rule).
- Mechanics, milestones 1-9, and all verification/economics stand unchanged.

## Open questions

- **Retention without toxicity**: leaderboards drove SETI volunteers to buy
  GPUs, which is the volume bug in social form. Per-moot standings shaped by
  reliability (like tessera itself) probably avoid the arms race; watch for
  it when the standings UI lands.
- **Surge absorption**: what does a coalition need *in advance* to absorb an
  F@h-2020-scale influx (newcomer T0 on-ramp throughput, bootstrap seeding,
  moderation), and does the newcomer on-ramp (mechanics §9) scale to a
  million arrivals?
- **Petals swarm health in 2026 is unverified** from public sources; the
  public swarm's liveness is itself a data point about retention without
  community. The mechanics brief already treats Petals as a portable
  reference (MIT), not a dependency; keep it that way.
- **Device-class normalization** for the time-bank preset (is one
  phone-hour one workstation-hour? a constitution choice; name the default).

## Sources

Beyond those linked inline: the
[volunteer computing project list](https://en.wikipedia.org/wiki/List_of_volunteer_computing_projects)
(catalog + scale figures + defunct patterns),
[BOINC](https://boinc.berkeley.edu/) (client release cadence),
[Tom's Hardware](https://www.tomshardware.com/news/folding-at-home-breaks-exaflop-barrier-fight-coronavirus-covid-19)
/ [TechSpot](https://www.techspot.com/news/84561-foldinghome-exceeds-15-exaflops-battle-against-covid-19.html)
(F@h surge figures), [timebanks.org](https://www.timebanks.org/dr-cahn) and
the [UK voluntary-sector study](https://link.springer.com/article/10.1007/s11266-022-00467-6)
(timebanking practice + critiques).
