# Esp Consolidation Plan

**Date**: 2026-08-08
**Status**: Decisions registered (Mark, 2026-08-08); consolidation phases planned,
nothing executed. Supersedes the first draft written in `repos/esp/design_docs/`
the same day; that file is now a pointer here.
**Scope**: fold `vates` and `sibylla` into one crate named `esp` inside mere,
retire the two names, and connect the crate to the intention corpus it serves.
The lanes themselves keep their own plans; this doc consolidates the code and
holds the ledger, it does not absorb the lanes.

**Related (the corpus this reconnects):**

- [burn_utilization_brief](../research/2026-07-04_burn_utilization_brief.md) —
  the five endorsed lanes; the spine of §4.
- [local_models_harness_brief](../research/2026-06-24_local_models_harness_brief.md)
  — the runtime + harness layer; its §7 "where the seam crate lives" question is
  answered by this plan: the seam crate is esp.
- [geist_models_brief](../research/2026-05-10_geist_models_brief.md) — geist =
  open base + LoRA-adapter engrams, composable at inference across tiers;
  Distillery-as-trainer; the adapter compatibility envelope.
- [communal_compute_tiers_brief](../research/2026-06-10_communal_compute_tiers_brief.md)
  — the authority for the communal lane; rings, tessera-shaped credit, and the
  communal-big-model verdict.
- [shared_engram_commons_brief](../research/2026-07-24_shared_engram_commons_brief.md)
  + [commons_profile_v1](../design/2026-07-27_commons_profile_v1.md) — the
  commons is a profile over the substrate; multi-writer convergence answered.
- [mesh_lease_scheduler_plan](2026-06-30_mesh_lease_scheduler_plan.md) — Lane 2
  prior art: burn-remote as an ALPN on murm's iroh Router, leases ours.
- [inference_provider_plan](2026-07-05_inference_provider_plan.md) (Lane 3),
  [burn_wgpu_flip_plan](2026-07-04_burn_wgpu_flip_plan.md) (Lane 1),
  [orrery_graph_intelligence_plan](2026-07-06_orrery_graph_intelligence_plan.md)
  (Lane 5) — the landed lanes esp inherits.
- [participant_gate_packs_plan](2026-07-17_participant_gate_packs_plan.md) —
  servitor, the consumer shape (§6).

---

## 1. Decisions (registered 2026-08-08)

1. **esp lives in mere**, at `crates/intel/esp`, publishing from mere exactly as
   vates and sibylla do today. This follows the 2026-07-23 repo consolidation
   rather than reversing it; `repos/esp` stays as the name-reservation repo and
   a pointer. Promote out later if and only if a consumer outside mere
   materializes (§5 names the triggers).
2. **One crate, two namespaces**: `esp::infer` (vates, 14 files) and
   `esp::embed` (sibylla, 24 files). Both crates already default to serde-only
   and wasm-clean with every heavy backend behind features, so the wall the two
   crates enforced is one features already enforce. Feature names carry over
   unchanged (`actor`, `decoder`, `decoder-wgpu`; `index-burn`,
   `index-burn-wgpu`, `bert`, `bert-wgpu`, `bert-validation`); the near-duplicate
   dependency blocks (burn 0.21, tokenizers, safetensors, serde_json, the wasm
   getrandom override) merge into one.
3. Accepted cost: independent versioning between the halves goes away. Fine at
   0.1.x with one owner; revisit only for a real external consumer on a
   different cadence.

## 2. The consumer graph (verified, and why this is small)

`vates` ← `mere-infer` only. `sibylla` ← `mere-embed` only. `mere-embed` ←
`eidetic-search` only. **`mere-infer` ← nobody**: it exists to re-export vates
under `infer::` paths, nothing imports it, and its only original content is one
integration test. Nothing outside mere consumes any of them; isometry, the
named flagship in both founding proposals, never wired either.

Two side-findings the consolidation collects:

- Retiring `mere-infer` resolves a live name collision: `register-viewer` and
  `graph-kernel` depend on the crates.io `infer` crate (file-type detection)
  while the workspace aliases `infer` to `mere-infer`. Afterward the bare word
  means one thing.
- The DOC_README entry for the
  [intel_vector_index_burn_lift_plan](2026-07-06_intel_vector_index_burn_lift_plan.md)
  still says "scoped, not started"; the lift re-homed into sibylla at its
  founding and landed there (sibylla's `index_burn`, keyed accelerators,
  measured crossover). Worth a line-item correction when this plan executes.

## 3. Consolidation phases

- **E0 — skeleton.** `crates/intel/esp` with the two namespaces, the merged
  dependency block, the union of the feature sets. Compiles empty.
- **E1 — move sibylla in.** `mere-embed` drops its path dep and rides
  `esp::embed`, keeping its genuine glue (persistence, quint field bridge,
  canvas search) mere-side per sibylla's founding split. Verify with
  `eidetic-search` under `bert` + `bert-wgpu`.
- **E2 — move vates in; delete `mere-infer`.** Not repointed, removed. Its
  `eidetic_corridor` test moves beside the other mere-side corridor tests
  first. One naming fix while everything is in motion: vates's `canned` and
  sibylla's `stub` are the same idea; `stub` wins (sibylla already renamed away
  from `hashed` once for honesty; keep that direction).
- **E3 — workspace sweep.** Drop the `vates` / `sibylla` / `infer` workspace
  keys, add `esp`. The intel cluster keeps its directory name.
- **E4 — publish and tombstone.** `esp` 0.1.0 from mere. Final `vates` and
  `sibylla` publishes carry a deprecation notice naming esp; notice-only is
  enough (downloads are double-digit, no external consumers).

Risks carry from the draft: mere's workspace manifest is the likeliest
concurrent-work conflict, so E1 and E2 land as separate commits with targeted
per-crate tests; the wgpu features need a real device to verify and
`eidetic-search` turns both on.

## 4. The intention ledger (what esp is the seam FOR)

The consolidation above covers only what lanes 1, 3, and 5 already landed. The
corpus records considerably more, endorsed and unclaimed. esp is the named home
for the seams all of it rides; each lane spins out to its own plan when picked
up, per the burn brief's own rule.

**Landed, inherited by esp:**

- Lane 1: burn-wgpu on, measured (BERT 3.2-38x GPU; field eval CPU-default;
  the burn existing-device init seam verified).
- Lane 3 core: the `InferenceProvider` seam, the streaming actor with
  mid-stream cancellation, the own llama-family decoder validated on TinyLlama
  (9.95 tok/s wgpu vs 0.09 CPU), eidetic `ManifestId` loading.
- Lane 5: the burn-free solver and affinity seams into gyre, blended affinity,
  lexical embedding, all with the honest in-context numbers.
- The vector-index burn lift (in sibylla, exact, measured crossover).

**Unfinished, with esp as the seam home:**

- **Lane 3's tail**: the `endpoint` backend (external OpenAI-compatible
  endpoints; named in vates's manifest as roadmap), the wasm half of the
  done-condition, and D2 (the empirical wasm model-size ceiling, which sets the
  in-browser tier).
- **The harness's missing trait**: `AdapterLoader` was specified in the harness
  brief §2 and never built. Load/stack LoRA-adapter engrams against a base,
  honouring the geist compatibility envelope; a mismatch is a rejection, never
  a guess. This lands in `esp::infer` beside the provider.
- **Lane 2, the personal mesh**: burn-remote mounted as an ALPN on murm's
  existing iroh Router, `RemoteTicket`/`PeerAuthorizer` as the tessera/kith
  authorization seam, lease semantics ours. Done when a second machine executes
  tensor ops for the first under a lease. Still gated on the post-0.21
  burn-remote release (D3); the split when it lands: esp owns the compute
  worker/client seam, murm owns transport, the lease scheduler stays mere.
- **Lane 4, training**: Distillery-as-trainer as a native-only armillary job
  over the scoped corpus, emitting the geist §6 engram triple
  (`ModelAdapterManifest` + `TrainingCorpus` + `EvalReport`); the bunsen
  group-optimizer borrow; done when an adapter measurably beats the untuned
  baseline on a recall or ranking task. Lane 2 turns the personal fleet into
  the training pool.
- **D1, the device policy**: now a three-consumer decision (netrender render,
  infer generation, embed affinity) with measured contention on the shared
  queue (a 200-token answer at >40s, coalesce to ~18s, well under the 9.95
  tok/s standalone). esp inherits the burn boundary, so the compute-device
  handle and any yield/priority seam live in esp; the decision itself stays
  open until the resident-data consumer forces it.

**The communal horizon (moots get the capability):**

- **Geist**: a moot's accumulated flora plus an inference primitive that thinks
  in its idiom; adapters are engrams, composable at inference across tiers
  (yours + this moot's + that co-membership's). esp provides the faculty;
  eidetic stores the weights; the geist brief owns architecture and governance,
  and its §10.2 privacy question (training data vs the right to be forgotten)
  is still the hardest open item in the whole lane.
- **Sharing dataweights and open models**: adapter engrams travel under the
  same three-axis envelope (privacy / provenance / trust) as any engram, and
  the commons profile already proves the replication substrate (multi-writer
  convergence answered 2026-07-26/27, property-tested; membership by gemot,
  authority by personae, capability checks by servitor). Flora training data
  and trained adapters ride the same rails.
- **Group-scale compute, honestly bounded**: the communal-compute brief's
  verdict stands — hosting a big model communally is reachable now for async
  work; WAN *training* is demonstrated to 32B but only on curated nodes; the
  inference ladder is route-first, shard-last. "Run a bigger model than one
  machine can, as a group, asynchronously" is therefore a real target with a
  researched shape, and it composes from Lane 2 + the marketplace mechanics
  (tessera-shaped credit, T0-T3 verification) that the resource-coordination
  briefs own.

**The consumer shape: servitor.** The created-and-bounded resident of the
thoughtform triad (servitor → tulpa → egregore), live at `crates/servitor` with
four consumers (commons, document-host, turnstone, cleromancy). A servitor
perceives through esp's seams, remembers through eidetic, runs on armillary,
and acts only under participant-gate grants; the commons profile already routes
its capability checks. This is what "harnessing models" completes into: not a
chat box, a bounded resident with a scoped faculty. The moot-scale end of the
same triad is the moot's geist wearing the egregore vocabulary, which the stack
already holds.

## 5. Posture (does this necessitate going beyond mere?)

Not now, and the corpus is unambiguous about why: every unclaimed lane binds
into mere's substrate. Lane 2 rides murm's Router and tessera/kith; Lane 4
reads persona-scoped corpora and writes eidetic engrams; the communal lanes
ride gemot, personae, and the commons profile; the actors are armillary; the
resident is servitor. esp stays the portable burn boundary inside that weave,
publishing from mere.

The promote-as-necessary triggers, named so the valve is real: isometry finally
wiring the NPC lane (the flagship consumer both founding proposals promised),
the graphshell remote lens needing client-side inference, or any non-mere host
adopting the seams. Until one exists, promotion would be a wall with nothing
behind it.

## 6. Progress

- **2026-08-08**: first draft written standalone in `repos/esp`; Mark ruled esp
  lives in mere, one crate, `esp::infer` + `esp::embed` accepted, and asked for
  the plan to reconnect to the research corpus and for servitor's place in it.
  This version does both; the standalone draft is now a pointer. Consumer graph
  verified against manifests; corpus read (burn brief, harness brief, geist,
  communal compute, engram commons, commons profile). Nothing executed; E0 is
  the next act.
