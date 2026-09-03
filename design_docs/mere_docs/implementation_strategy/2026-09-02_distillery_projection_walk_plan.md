# Distillery Projection Walk Plan

**Date:** 2026-09-02
**Status:** W0 complete — landed 2026-09-02, all three tests verified in the
real crate 2026-09-03 once mere's surface imports followed the
`mere-surface-api` split; W1 held until the boundary migration's P2/P3 moves
settle (Progress, 2026-09-03). §2 was read at mere `77a3701f052` and corrected at `3ce750f5`
by the W0 implementation, which read the code rather than this plan.
**Scope:** the first end-to-end scene binding for a port — dataset → scene →
Scenograph → host — walked on Distillery. This plan owns the walk; the
[Distillery v0 plan](2026-08-12_distillery_v0_plan.md) owns the works
(resident authority, installed boundary, trainer, host-policy composition) and
nothing here changes what that plan rules.
**Companions:** the
[port GUI composition and comparable stacks brief](../research/2026-09-02_port_gui_composition_and_comparable_stacks_brief.md)
(why this port, and why a walk at all), the
[projection/scenes direction note](../../2026-08-23_projection_scenes_and_graph_native_platform.md)
(the nine questions this plan answers), the
[scenograph content catalog](../research/2026-08-18_scenograph_content_catalog.md)
(the Chronicle, Circuit, and Loom recipes), the
[projection grammar catalog](../research/2026-08-15_projection_grammar_catalog.md)
(promotion rules, should a recipe need a primitive the grammar lacks), the
[projection grammar adoption plan](2026-08-15_projection_grammar_adoption_plan.md)
(A3 and A5 gates this walk may supply evidence to, never close), and the
[suite census](../../2026-08-22_turnstone_suite_composition_and_capability_census.md)
(§6, the contribution seam Distillery already provides through; §3, the
Graphshell charter; §7.4, the Alembic re-ruling).

## 1. Why Distillery

Three reasons, each a fact rather than a preference:

- It is the one port with **models, work, and a Cambium surface** in one
  place: the model works by charter, Alembic's work-runs by the 2026-09-02
  re-ruling, and a read-only installed surface (`distillery.installed.v1`)
  that Turnstone already admits as the contribution seam's second provider.
- It has **no projection endpoint**. Nothing in `ports/distillery` implements
  `ProjectionCatalog` or `ProjectionSource`, so the walk forces the whole
  path rather than the half the receipts plan already proved.
- Its **second host already exists.** The family thesis's anti-shell test says
  the receipt for a platform capability is a second host that keeps its own
  identity. Distillery's own surface and Turnstone's admission of it are two
  such hosts, before Graphshell is counted.

## 2. What Distillery owns, as data

Read from `crates/mesh/mesh/src/board.rs` and `ports/distillery/src/`.

**The job board.** `Job { id: JobId([u8; 32]), kind | spec, payload, posted_by:
[u8; 32], state, lease: LeaseRecord, next_claimants: Vec<Claimant> }` with
`JobState::{Posted, Claimed { winner }, Done { winner, result }, Committed {
winner, output }}`. Two wire generations (M1 inline, V2 content-addressed)
share one state machine; the claim-race winner is deterministic; leases are
clock-free admissible facts with epochs. This is event-shaped data with
spans: a job is posted, claimed, and terminated, and a lease is a span with an
epoch.

**The resident receipt stream.** `ResidentReceipt::{Tick { steps },
MaintenanceCompleted(report), MaintenanceIdle, MaintenanceFailed { error },
SupervisorFailed { error }, StopRequested }` — one exact observation per
supervisor turn, the ticks the v0 plan's done-condition runs on `SystemClock`.
This is the clock the board's events hang from.

**Installed facts.** `DistilleryInstalledSnapshotV1 { profile, protection,
mesh_id, mesh_root, mesh_store_path, blob_store_root, resident:
Option<DistilleryResidentSnapshotV1> }` — the last a newtype over
`ResidentSettings`, which W4's prop shape will feel — plus the latest receipt:
what the surface renders today. These are status facts, not a scene (§3).

**Artifacts and provenance.** `TrainRequest { base_model_ref, tokenizer_ref,
corpus_ref, adapter_name, prompt_template, metric: EvalMetric, settings:
LoraTrainerSettings, created_at }` → `TrainReceipt { adapter_manifest_ref,
eval_report_ref, adapter_blob, adapter_config_blob, baseline: EvalTally,
adapter: EvalTally }` — both tallies, which is what makes §3.2's
compare-two-evals intent supportable from the data. `FloraContribution {
contribution_id, manifest_ref, manifest, adapter_config_json: Vec<u8>,
adapter_safetensors: Vec<u8>, weight }` → `FloraAggregate { manifest,
adapter_config_json, adapter_safetensors, receipt }`. Training provenance is
edge-by-reference: every reference is a content-addressed `ManifestId`, so it
is a DAG whose edges are references. The Flora records are not: they carry
the adapter bytes inline beside a manifest reference. So the Circuit's blocks
are of two kinds — reference-only and payload-bearing — and W3's entrance
check decides how a payload-bearing block is represented before any of it is
drawn.

**Retention and custody.** `RetentionSettings`, `ResidentStorage`, and
`MaintenanceReport { checkpoint, candidates, collected, effects }`, per tick:
`candidates` is the count of blob refs safe to release, `collected` the
custody claims actually removed, and the gap between them is what a custodian
refused. Nothing counts what was kept; a Chronicle span for a maintenance
window can show released-versus-refused, not retained.

## 3. The scene binding, by the nine questions

The direction note's rule: identify which projection layer owns a candidate's
novelty before naming a scene; then answer the nine questions; then prove it
on a second dataset. Applied to Distillery's data, two scenes survive, one
stays a candidate, and one thing is not a scene at all.

### 3.1 Chronicle over the board and the receipt stream (founding)

| Question | Answer |
| --- | --- |
| 1. Reads | `Job` (id, poster, state, lease epochs, claimants) and `ResidentReceipt` ticks and maintenance reports. |
| 2. Derivation | Order by tick; a job's span runs Posted → terminal; a lease's span runs its epoch; maintenance events group by tick. Temporal and grouping derivation only. |
| 3. Entity becomes | An **event card** (a job) or a **span meter** (a lease, a maintenance window). |
| 4. Relation becomes | An **arc over the axis**: poster → winner (the claim), job → committed output (the commit), maintenance → the jobs it retained or erased. |
| 5. Geometric law | A timeline spine; era bands are lease epochs; position is time, size is span. |
| 6. Guides | The time axis and era bands; a scrub transport over `scenotime` diffs (the recipe's native motion). |
| 7. Back-mapping | `JobId`, author keys, `ManifestId`, tick index — every mark names its source. |
| 8. Intents | v1 advertises none that change truth. Reclaim, cancel, and retention changes are the authority's petitions and stay with the works; the projection may advertise them later as typed intents the endpoint refuses or forwards, never performs. |
| 9. Second dataset | Djinn's resident job log (the same grammar, a different owner), then a heterogeneous one: the browsing trace or radio traffic, both named as Chronicle transfers in the catalog. |

Layer pre-filter: the novelty is the *reading* (events and spans from a
claim-race board) plus the arrangement (timeline with era bands); the
Chronicle recipe holds both. Nothing new enters the grammar. A3's stage-one
ladder applies unchanged — cards at close zoom, glyphs far out.

### 3.2 Circuit over training and Flora provenance (second, heterogeneous within the port)

| Question | Answer |
| --- | --- |
| 1. Reads | `TrainRequest`, `TrainReceipt`, `FloraContribution`, `FloraAggregate`, and the manifests they reference. |
| 2. Derivation | Traversal: follow `ManifestId` references into a DAG. |
| 3. Entity becomes | A **component block with ports**: corpus, base model, tokenizer, trainer run, adapter, eval report, contribution, aggregate. |
| 4. Relation becomes | An **orthogonally routed trace** from producer port to consumer port; direction always drawn. |
| 5. Geometric law | Grid-snapped placement by topological rank; bundling at density. |
| 6. Guides | A blueprint backdrop; a legend for block kinds. |
| 7. Back-mapping | `ManifestId` on every block and trace. |
| 8. Intents | Open a manifest; compare two eval reports (`EvalTally` baseline against adapter) — read-only intents the endpoint serves as resources. |
| 9. Second dataset | The workspace dependency graph, Circuit's own founding dataset — which proves the recipe is not Distillery's skin. |

Entrance check before W3: the grammar must already carry orthogonal routing
and port-anchored endpoints, or those are a promotion proof first (the
catalog's rule; the adoption plan's A5 names ELK's port and label anatomy for
exactly this). This plan does not decide that; it records the check.

### 3.3 Loom over device lanes (candidate, gated)

Lanes are claimants or devices; nodes are jobs in lane order; cross-lane edges
are leases handed off or reclaimed. The direction note lists Streams as
unresolved — it earns separation from Timeline only if lane membership sets
one axis and cross-stream crossings govern the view. Until the works runs
multi-device traffic where the crossings are the point, this is a Chronicle
variant, not a scene. Gate: real multi-device job traffic in the receipts.

### 3.4 Not a scene: the installed surface

Profile, protection, mesh id, roots, resident settings, latest receipt — this
is Steward-shaped status (census §8: status projections need exposure, not a
scene). It stays the retained surface it is, and becomes the frame the
Chronicle sits in (W4).

## 4. Phases and done-conditions

Each phase names its forcing consumer and its receipt. Nothing here closes a
gate in the adoption plan; evidence it produces is filed there.

**W0. Assemble — landed 2026-09-02, one leg unverified.** Fixtures for both
second datasets, readable without Distillery, plus a headless drive of the
installed surface through its own runner (the headed `.scn` belongs to W4,
because Turnstone admits the surface through a *pinned* mere revision in
another repository).
*Done when:* both fixtures exist under test, and a bare scenario drives the
installed surface end to end. Nothing rendered yet.
*State:* complete. All three tests pass in the real crate as of 2026-09-03
(Progress).

**W1. The endpoint.** `ports/distillery` implements `ProjectionCatalog`,
`ProjectionSource`, and `ResumableProjectionSource` over the board and the
receipt stream: one offer whose score uses the timeline arrangement with
per-item disclosed axis values (tick, epoch), a presentation manifest of
`PortableCardV1` per job, and no intents in v1. Served through the resident
projection host like every other endpoint; admitted by Notochord; resumable
by revision.
*Done when:* Graphshell mounts Distillery's offer and renders the job
Chronicle as a served scene; snapshot, diff, reconnect, and resume are in one
machine-readable receipt; the `FrozenScene` realization lists jobs by name
with their spans in its table; a second Distillery session over the same
mesh yields byte-identical scores (determinism receipt).

**W2. The binding, authored.** The Chronicle recipe is expressed as a
Scenograph definition — Source, Reading, Encoding, Arrangement, Interaction,
Appearance, Provenance — over Distillery's endpoint, cited by a shelfmark whose
`expects.generation` checks against the board's checkpoint hash. The same
definition is then pointed at Djinn's resident log.
*Done when:* one authored definition, two datasets, both read true in the
headed receipt; the shelfmark round-trips; changing one lever (era bands off)
yields a variant, not a new definition.

**W3. Circuit.** Training and Flora provenance as Circuit, subject to §3.2's
entrance check; the workspace dependency graph as its second dataset.
*Done when:* both render through one recipe; every block resolves to a
`ManifestId` or a crate; the eval-comparison intent is advertised and served
as a resource; the frozen realization is a navigable table of blocks and
traces.

**W4. Hosts.** Three, in order: Graphshell viewing (W1 gives it, and it is the
preeminent Scenograph host); Distillery's own Cambium surface embedding the
Chronicle beside its installed facts, through Cambium's scene-hosting rule
(props in, retained local interaction state, typed events out); Turnstone
through the contribution seam it already admits `distillery.installed.v1` by.
*Done when:* one scene realized in two hosts that keep their own identity,
workflow, and authority — the anti-shell receipt — each with a headed
genet-probe receipt, one Tab order, one AccessKit tree, and the table
alternate reachable in both. Turnstone is the third host, not the second.

**W5. Alembic's work-runs.** Once the Alembic move is ruled and executed, its
bounded-actor runs join the board Chronicle as a second job kind: same
recipe, one more entity kind, no new scene.
*Gate:* the move is executed (2026-09-02, `ports/distillery/{alembic,athanor}`
founded); what remains is Alembic's workshop implemented far enough to emit
runs onto the board. Not opened before that.

## 5. Boundaries

- The projection reads; it never performs. Reclaim, cancel, retention, and
  lending remain the works' petitions through the authority.
- No inference anywhere in the path. Derivations are ordering, grouping, and
  traversal; a model-produced derivation, if ever wanted, is a separate
  provider above the reading.
- No new grammar primitive without a promotion proof. §3.2's entrance check
  is the one place this plan expects to meet that rule.
- No change to what the v0 plan rules about the works, the installed
  boundary, or host-policy composition.
- Scenograph is built with Cambium and hosted; it does not acquire
  Distillery's authority or product policy.

## Findings

### 2026-09-02

- `ports/distillery` has no `ProjectionCatalog`/`ProjectionSource`
  implementation; its `chirograph` and `sceno` use is nil. Its surface is
  `distillery.installed.v1`, a `RetainedSurfaceSession` over
  `GenetAppRunner`, rendering installed facts and the latest receipt.
- The mesh board's `JobState` is a four-state machine with a deterministic
  claim-race winner and clock-free lease records with epochs — natively
  event-and-span shaped.
- Training and Flora records reference each other by `ManifestId` only, so
  provenance is a content-addressed DAG with no product identifiers to leak.
- `ScoreItem.axis` already exists for arrangements that place along an axis
  (the score's own doc names Timeline among them), so W1 needs no contract
  change to disclose tick and epoch.
- Turnstone admits the Distillery surface as the contribution seam's second
  provider (v0 plan status), which makes the anti-shell second host a
  composition receipt rather than new machinery. Concretely: turnstone's
  `src/shell/mod.rs` registers a built-in `DistilleryInstalledProvider`, and
  its `Cargo.toml` pins `distillery` to a mere git revision — so a headed
  scenario there sees the pinned Distillery, not this checkout.
- W0 (2026-09-02): `ResidentReceipt` does not derive `Serialize`, so the
  fixture's tick stream is a six-variant `TickRecord` mirror in the test file
  (the two failure variants included, to keep the mapping total).
  `mesh` re-exports `MeshExt` and `to_operation` but not p2panda's
  `Operation`, so a test that authors a log names `p2panda-core` as a
  dev-dependency. Tracked `.json` under the fixtures comes back CRLF unless
  `.gitattributes` says otherwise; it now does for
  `ports/distillery/tests/fixtures/**`.
- W0 (2026-09-02): in this checkout `cargo check -p distillery` fails inside
  `src/surface.rs` before any test is reached — the local genet working copy
  split `genet-host-api`'s application half out as `mere-surface-api`
  (genet `57bcc38fdae`), `.cargo/config.toml` patches cambium to that local
  genet, and mere's `surface.rs` still imports the surface contracts from
  `genet_host_api` at the pinned `eff0cb6`, so two `SurfaceDescriptor` types
  meet at `distillery_installed_surface`. Local-dev only: a clean checkout
  resolving genet from `eff0cb6` is unaffected. The migration of mere's
  side is the boundary plan's P1/P3 work, in flight in another session, and
  not this plan's to make.

## Progress

- 2026-09-02: plan founded from the assessment in the port GUI composition
  brief, on Mark's "walk distillery". No code opened.
- 2026-09-02: **W0 landed, one leg unverified.** Implemented by a subagent
  under a paths-only brief (new files under `ports/distillery/tests/`, one
  script, one dev-dependency line) while another session worked the tree.
  Landed: `tests/walk_fixtures.rs` (569 lines) with three tests;
  `tests/fixtures/chronicle/{distillery_board,djinn_board}.json`, two boards
  of the same grammar and different owners built by `JobBoard::fold` over
  seeded p2panda operations (3 and 4 jobs; Posted, Claimed with a lease,
  Committed V2; a second claimant so `next_claimants` is non-empty), each
  with a `TickRecord` stream; `tests/fixtures/circuit/workspace_graph.json`
  (100 packages, 230 edges, acyclic, `generated_from` the HEAD it was cut
  at) produced by `scripts/workspace_graph_fixture.py`, byte-identical on
  regeneration except for that label when HEAD moves. Verification: the two
  data tests were type-checked and run against the committed files in an
  isolated probe crate that path-depends on the same `mesh`, `personae`, and
  patched `p2panda-core`, three processes, byte-equality asserted across
  them — because `cargo test -p distillery` cannot link in this checkout
  (Findings). The headless surface drive
  `installed_surface_drives_headless_through_its_runner` is written against
  the surface's own unit-test pattern and every call checked against the
  pinned `RetainedSurfaceSession`, and it has not run. Re-run
  `cargo test -p distillery --test walk_fixtures` twice once mere's surface
  imports follow the `mere-surface-api` split. §2 corrected in five places
  by the same pass.
- 2026-09-03: **W0 verified in the real crate; W1 held.** Mere was repointed
  to genet head after P1 (`487e18a4`), `surface.rs` now imports from
  `mere_surface_api`, `cargo check -p distillery` is clean, and
  `cargo test -p distillery --test walk_fixtures` passes 3 of 3 — the
  headless surface drive included. A second run minutes later could not
  resolve at all: the local genet checkout was mid-edit and `cambium`'s
  manifest inherited a `mere-surface-api` entry its workspace no longer
  declared. That is the boundary plan's P2/P3 (Cambium and the surface
  contracts moving toward Mere) in flight, not a regression; the determinism
  receipt for the fixtures already stands from the probe (three processes,
  byte-equality). W1's endpoint would build against exactly the crates that
  are moving, so it is held until those moves settle rather than written
  twice; its brief is ready: `ProjectionCatalog::describe`,
  `ProjectionSource::snapshot` with `SceneSnapshot::from_dense`,
  `PresentationSource::resource`, `ResumableProjectionSource::resume`, the
  `LiveEndpoint` pattern in `ports/graphshell/src/live_endpoint.rs`,
  `Arrangement::Timeline` with `ScoreItem.axis: Option<AxisValue>`, and
  registration through `ResidentEndpointCatalog::register_resumable_notifying`.
