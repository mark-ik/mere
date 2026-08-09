# Esp Consolidation Plan

**Date**: 2026-08-08
**Status**: E0-E3 implemented 2026-08-09. E4 is locally prepared: ESP 0.1.0
packages and verifies; the Vates and Sibylla compatibility shims compile, but
Cargo cannot package them until ESP exists in the registry. The irreversible
publish sequence remains unexecuted. The 2026-08-08 adversarial amendments
remain authoritative: corrected knot consumer graph, restored mesh/scheduler
boundaries, host-side device policy, narrowed servitor language, and separate
portability and repository-promotion gates. Supersedes the first draft written
in `repos/esp/design_docs/`; that file is now a pointer here.
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

`vates` ← `mere-infer` only. `sibylla` ← `mere-embed` **and `ports/knot`**
(`sibylla.workspace = true`, knot:51). `mere-embed` ← `eidetic-search` only.
**`mere-infer` ← nobody**: it exists to re-export vates under `infer::` paths,
nothing imports it, and its only original content is one integration test.
Nothing outside the mere repository consumes any of them; isometry, the named
flagship in both founding proposals, never wired either.

Method note, learned the hard way: the first draft said "sibylla ← mere-embed
only" because its dependency grep matched `name = {...}` but not the dotted
`name.workspace = true` form knot uses. Consumer graphs get verified with
`cargo metadata` (or a grep matching both forms), never a single-form grep.
The same bug undercounted servitor's consumers (six, not four: commons-spine,
document-host, gemot, knot, turnstone, cleromancy).

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

- **E0 — skeleton plus the feature/target matrix.** `crates/intel/esp` with
  the two namespaces, the merged dependency block, the union of the feature
  sets. An empty compile proves too little; E0's done-condition is a recorded
  matrix: empty/default on native and wasm; every feature individually;
  infer × embed combinations; the tokenizer question decided (both halves pin
  `onig`, native Oniguruma, so the wasm story needs tokenizers' wasm-compatible
  regex path or an honest wasm exclusion per feature); `decoder-wgpu` made to
  activate the wasm `getrandom` override it currently misses in the merged
  manifest; CPU-vs-real-wgpu parity smoke; and confirmation the default tree
  pulls neither burn nor tokenizers. Prior wasm receipts are treated as
  historical until re-run here.
- **E1 — move sibylla in.** `mere-embed` drops its path dep and rides
  `esp::embed`, keeping its genuine glue (persistence, quint field bridge,
  canvas search) mere-side per sibylla's founding split. **Repoint knot too**
  (its direct `sibylla` dep). Verify with `eidetic-search` under `bert` +
  `bert-wgpu`, and knot's own tests.
- **E2 — move vates in; delete `mere-infer`.** Not repointed, removed. Its
  `eidetic_corridor` test moves beside the other mere-side corridor tests
  first. One naming fix while everything is in motion: vates's `canned` and
  sibylla's `stub` are the same idea; `stub` wins (sibylla already renamed away
  from `hashed` once for honesty; keep that direction).
- **E3 — workspace and doc sweep.** Remove `infer`, add `esp`, and remove all
  active consumers of `vates` and `sibylla`; their workspace keys remain only
  to build the E4 compatibility shims. The intel cluster keeps its directory
  name. Sweep the record in the same pass: mere's root README, DOC_README
  (including the stale "scoped, not started" line on the index burn lift, §2),
  supersession banners on the two founding proposals, package metadata, and
  the `repos/esp` pointer.
- **E4 — publish esp, then compatibility shims.** Order matters because
  published versions are permanent: `esp` 0.1.0 first, then final `vates` and
  `sibylla` releases that depend on esp and re-export their old APIs with
  `#[deprecated]` markers (`CannedProvider` kept as a deprecated alias of the
  renamed stub). A notice-only tombstone is easy to miss and crates.io shows
  no maintenance badge; the shim keeps any stray consumer compiling and
  pointing at the door. Cost accepted: the shims pin an esp version and ride
  along on future bumps.

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
  done-condition, and D2 (the empirical wasm model-size ceiling, which sets
  the in-browser tier). D2 is a measurement list, not one number, per the
  WebLLM lesson: cold download, warm reopen, artifact integrity,
  persistent-storage status, first-token latency, steady throughput, GPU
  memory, cancellation, UI frame impact, worker restart. Model artifacts ride
  the existing muniment IndexedDB store and its headed-proven persistence
  status UX (graphshell's 2026-08-06 browser-storage receipt); esp does not
  grow its own cache.
- **The harness's missing trait**: `AdapterLoader` was specified in the harness
  brief §2 and never built. Load/stack LoRA-adapter engrams against a base,
  honouring the geist compatibility envelope; a mismatch is a rejection, never
  a guess. One tension to resolve on the way in: `GenerationRequest` says chat
  templating happens above the seam (provider.rs:52), while the envelope makes
  the template an adapter-compatibility fact. Resolve with a **`ModelSession`**
  (prepared-request) seam binding base, tokenizer, prompt template,
  quantization, and the *ordered* adapter set; the provider stays the streaming
  execution contract. Per the mistral.rs lesson, adapter selection is
  per-request against an immutable loaded base with the adapter-set identity
  recorded; provider-global mutable adapter state is a bug waiting for
  concurrent residents.
- **Lane 2, the personal mesh**: the split honors the standing
  resource-coordination boundaries rather than reinventing them.
  `crates/mesh` already owns the signed job grammar, `JobSpec`,
  `JobNamespace`, and the `MeshResource` adapter seam (M1 done: Echo/Blake3
  jobs over LogSync); the lease/scheduler plan owns owner priority, revocable
  leases, heartbeats, checkpoint classes, device policy, and owner reclaim.
  So: **esp** = model loading, tensor ops, the local/remote compute adapter,
  cooperative cancellation points; **mesh** = job grammar, namespace, resource
  registry, lease lifecycle, checkpoint/result facts; **host scheduler** =
  foreground priority, device selection, render-vs-compute budget, reclaim;
  **murm** = the shared iroh endpoint and Router; **servitor::Gate** = whether
  an admitted denizen may petition the graph at all. Burn's `PeerAuthorizer`
  is session admission, not job authorization; the opaque `RemoteTicket` may
  carry a mesh lease reference, but mesh enforces scope, expiry, and reclaim
  locally. **The first step needs no burn at all**: the M2 plan already names
  a deterministic hashed embedding batch as the first real `MeshResource`,
  and `esp::embed`'s stub provider is exactly that adapter. Burn-remote
  mounting on the Router stays gated on a stable release (D3; it exists today
  only as 0.22.0-pre.1, and 0.21 → 0.22 is a breaking backend/device
  migration that gets its own plan).
- **Lane 4, training**: Distillery-as-trainer as a native-only armillary job
  over the scoped corpus, emitting the geist §6 engram triple
  (`ModelAdapterManifest` + `TrainingCorpus` + `EvalReport`); the bunsen
  group-optimizer borrow; done when an adapter measurably beats the untuned
  baseline on a recall or ranking task. Lane 2 turns the personal fleet into
  the training pool.
- **D1, the device policy**: now a three-consumer decision (netrender render,
  infer generation, embed affinity) with measured contention on the shared
  queue (a 200-token answer at >40s, coalesce to ~18s, well under the 9.95
  tok/s standalone). **esp does not own this.** One consumer must not own
  policy shared by rendering, inference, embedding, and later training: esp
  accepts a host-selected device and exposes cancellation/yield hooks; the
  host scheduler owns foreground priority and the render-vs-compute budget.
  The M2 plan states the rule exactly: land device policy and owner reclaim
  first, "otherwise the first real adapter will smuggle scheduler policy into
  the resource layer." The decision itself stays open until the resident-data
  consumer forces it.
- **Scope guard**: esp is the inference and embedding seam home, not a
  taxonomy of every intelligent operation. Woodshed's structured audio
  analysis is a third, provenance-bearing provider seam that stays
  woodshed-side until a second consumer proves it, per its own plan.

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

**The consumer shape.** The servitor crate is a headless identity, capability,
and petition gate (six consumers: commons-spine, document-host, gemot, knot,
turnstone, cleromancy); it is not an inference runtime, and this plan gives it
no new role. The composition is the point: an admitted denizen may be hosted
by armillary, use an esp model session, retain artifacts through eidetic, and
petition through `servitor::Gate`. The application host composes those organs;
none of them owns the resident. That composition is what "harnessing models"
completes into: not a chat box, a bounded resident with a scoped faculty. The
thoughtform words (servitor, tulpa, egregore) stay product language, not a
runtime type hierarchy.

**Sequence (adopted from the 2026-08-08 review):** 1. consolidate and stop at
E4. 2. Land the deterministic embedding `MeshResource` and namespace receipt.
3. Burn 0.21 → 0.22 migration as its own plan (it carries burn-remote; new
runtime `Device` selects local or remote behind one model implementation; its
iroh 1.0 aligns with murm's 1.0.3). 4. Device policy and owner reclaim.
5. Mount burn-remote as another compute adapter. 6. wasm model
loading/storage receipts (D2's list). 7. Model sessions and adapter loading.
8. Only then training and communal compute.

## 5. Posture (does this necessitate going beyond mere?)

Not now, and the corpus is unambiguous about why: every unclaimed lane binds
into mere's substrate. Lane 2 rides murm's Router and tessera/kith; Lane 4
reads persona-scoped corpora and writes eidetic engrams; the communal lanes
ride gemot, personae, and the commons profile; the actors are armillary; the
resident is servitor. esp stays the portable burn boundary inside that weave,
publishing from mere.

Two valves, kept distinct because the consolidation ruling keeps them
distinct: portable crates may live in the platform repository and be consumed
from it, and the bar for a separate repository is coherent utility and
identity apart from mere and its products. So a **second consumer** (isometry
finally wiring the NPC lane, the graphshell remote lens needing client-side
inference) triggers *portability discipline*: portability tests, a documented
API and target support, semantic-version care. **Repository promotion**
requires independent ownership or release cadence, which no named consumer
implies. If the halves ever diverge, the intermediate is `esp-infer` +
`esp-embed` packages behind an `esp` facade, still inside mere.

## 6. Progress

- **2026-08-08**: first draft written standalone in `repos/esp`; Mark ruled esp
  lives in mere, one crate, `esp::infer` + `esp::embed` accepted, and asked for
  the plan to reconnect to the research corpus and for servitor's place in it.
  This version does both; the standalone draft is now a pointer. Consumer graph
  verified against manifests; corpus read (burn brief, harness brief, geist,
  communal compute, engram commons, commons profile). Nothing executed; E0 is
  the next act.
- **2026-08-08, review pass**: an adversarial review by a second agent landed
  seven corrections, all verified against the tree before adoption: knot's
  direct sibylla dep (the draft's grep missed dotted `workspace = true` deps;
  method note in §2), servitor at six consumers, the mesh/scheduler ownership
  boundaries restored to Lane 2 (with the M2 deterministic-embedding first
  step, which un-blocks Lane 2 from the burn release gate), device policy
  moved to the host scheduler, the servitor paragraph narrowed to composition
  language, portability separated from repository promotion, the E0 matrix,
  and the E4 esp-first publish order with compatibility shims. The review's
  burn 0.22.0-pre.1 status and prior-art lessons (WebLLM, mistral.rs,
  woodshed's scope guard) are folded into the ledger.
- **2026-08-09, E0-E3 implementation**: created `crates/intel/esp` with
  `esp::infer` and `esp::embed`; moved both implementation and test suites;
  renamed `CannedProvider` to `StubInferenceProvider` with a deprecated shim
  alias; removed `mere-infer`; repointed `mere-embed` and knot; retained
  Vates/Sibylla as deprecated compatibility crates; and swept the authority
  docs plus the standalone pointer repository. The recorded target matrix is
  `crates/intel/esp/design_docs/2026-08-09_feature_target_matrix.md`. Empty,
  every individual feature, combined CPU features, and combined browser-WGPU
  features compile on their claimed targets; the merged CPU suite passed 171
  tests with one ignored, the Eidetic corridor passed, and three real-device
  WGPU parity tests passed. ESP packages and verifies as 0.1.0. The shims pass
  all-feature workspace checks and package-file enumeration, then stop at the
  intended registry gate because ESP 0.1.0 has not been published. Knot's
  consumer compiled before concurrent publication-client work introduced an
  unrelated temporary-borrow error in its test module; that work was preserved
  untouched. E4 therefore waits only on explicit authorization for the
  irreversible crates.io sequence.
