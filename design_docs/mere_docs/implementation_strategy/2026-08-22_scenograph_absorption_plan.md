# Scenograph Absorption Plan

**Date:** 2026-08-22
**Status:** complete — S1–S7 landed; `arrangements` is deleted and `mer3ly`
names one engine. Committed and pushed as mere `cc40c24f`, with `mer3ly`
`54fd8e2` repinned to mere `330eee98`.
**Home / supersession note (2026-09-04):** the [platform boundary and
repository topology plan](2026-09-02_platform_boundary_and_repository_topology_plan.md)
supersedes this plan's generic `scenograph` facade destination. The facade is
gone, its solver registry belongs to `scenomise`, and the published
`scenograph` name is held for the scene editor. This remains the historical
receipt for absorbing `arrangements` into the scene family.
**Supersedes:** the half-finished `arrangements` → `scenomise` migration begun
2026-05-18 (see `crates/canvas/cartography/src/adapters.rs` header and the
`scenomise` crate doc: "The first consumer proof moves the generic Spiral,
board, and geographic solvers here").

## Why

Mere carries two placement engines with overlapping catalogs.

| | `crates/canvas/arrangements` | `crates/scenograph/scenomise` |
|---|---|---|
| lines | 6,785 (src) | ~600 (`solve.rs` + `relax.rs`) |
| deps | `cartography` + `kernel` — bound to graph truth | `sceno` only — portable |
| evaluation | `Layout<N>::step(scene, state, dt, viewport, extras) -> HashMap<N, Vector2D>` | `solve(&Score) -> Scene` |
| catalog | Grid, Phyllotaxis, Stack, Radial, Penrose, LSystem, Timeline, Kanban, SemanticEmbedding (+ Spectral, adapter-only) | Spiral, Grid, Geographic, Hulls |
| settling | delegated to `seiche` | `relax` / `relax_holding` |

Five families overlap by name. The split has already leaked downstream:
`mer3ly/crates/repo-graph` depends on **both** engines (pinned rev
`546991d`), `retinue/signalman-desktop` and `mesocosm` took scenomise only,
`turnstone` took the full scenograph trio, `woodshed-core` took `sceno` +
`scenotime`. Consumers are picking sides on a decision that was never
finished.

### The finding that makes this cheap

**Every `Layout` impl in `arrangements` is closed-form.** All nine use
`StaticLayoutState { damping: f32, step_count: u64 }` or `StatelessPassState
{ step_count: u64 }` — there is no accumulating displacement, no convergence
history, no iteration state anywhere in the crate. `adapters/mod.rs` says so
outright: "Analytic (closed-form, no iteration)", and "Live force physics …
is `seiche`'s domain, not an arrangement adapter."

So `Layout<N>` is a per-frame stepper interface wrapped around solvers that
do not need one. The `damping` field is **easing toward an already-computed
target** — a presentation concern, not a layout algorithm. Stripping that
wrapper is most of the migration.

## Decisions taken

1. **`Arrangement` becomes hybrid.** Named families stay typed and
   exhaustively matchable; an added `Custom { id: String, config: Value }`
   variant routes unknown ids to a solver registry. One wire bump
   (`SCORE_VERSION` 3 → 4) covers every future family, and downstream repos
   (isometry, mesocosm, mer3ly) can register their own arrangements without
   a PR to mere.
2. **Per-item inputs become typed optional fields on `ScoreItem`** —
   `axis: Option<AxisValue>` and `embedding: Option<Vec2>`. Portable,
   serializable, and consistent with the product-free score charter. The
   `LayoutExtras<N>` bag does not survive.
3. **`arrangements` is deleted.** Solvers → `scenomise`, easing →
   `scenotime`, registry → `scenograph`, graph-bound adapters →
   `cartography` (which already owns `LayoutStrategy`).

## Adjacency: Radial and Spectral

`Score` carries `items` (ordinal + footprint + representation + placement +
layer + visible) and `holds`. It carries **no relations** — `RoutedRelation`
exists only on the output `Scene`, routed by the adapter after solving.
Sceno's charter is that solvers "never learn a source's native truth."

Two families read graph topology:

- **Radial** — BFS rings from a focal node; needs edges and a focus node id.
  `RadialConfig<N>` is generic over `N` for exactly this reason: it is the
  only node-identity dependency in the crate.
- **Spectral** — the two smallest non-trivial eigenvectors of the graph
  Laplacian; needs the adjacency matrix. Adapter-only today; it has no
  `Layout` impl.

**Both move to scenograph anyway, by precomputing host-side.** The
graph traversal stays in the adapter, which already holds the `Graph`; the
disclosed result travels as a per-item field and scenograph does the
placing:

- Spectral's output *is* a 2-D embedding, so it lands in the
  `ScoreItem.embedding` field decision 2 already adds. No new contract.
- Radial decomposes into ring index (`axis: Numeric(ring)`, from BFS) and
  angular slot (the existing ordinal).

This is not a workaround — it is the pattern sceno already uses. `Hulls`,
whose doc calls it "adjacency-preserving tiling," takes **no adjacency**:
`hulls_position` reads a per-item `Placement::Coordinate` that the adapter
disclosed, and the adjacency-preservation is a property of the output
partition. The most topology-flavoured arrangement in sceno is already built
this way, and doing the BFS or the power iteration once in the adapter beats
serializing an adjacency list so the solver can re-derive it.

**This forecloses nothing.** If a solver that genuinely consumes topology
ever belongs inside scenograph — a force-directed solver, an adjacency-aware
tiling that is not `Hulls` — `Score` can grow an optional
`relations: Vec<ScoreRelation>` as an additive v5 field. Nothing in this
plan blocks that; it is deferred because no current family needs it, not
because the charter refuses it.

Net effect: scenograph's catalog reaches parity with arrangements' — all
eleven families — and `radial` means one thing across the whole tree.

## Family disposition

| family | destination | notes |
|---|---|---|
| Phyllotaxis | `sceno::Arrangement::Spiral` | already present; reconcile config (`SpiralCurve` vs arrangements' curve params) |
| Grid | `sceno::Arrangement::Grid` | already present; reconcile explicit-cell handling |
| Stack | new `Arrangement::Stack` | ordinal-only |
| Penrose | new `Arrangement::Penrose` | ordinal-only aperiodic tiling |
| LSystem | new `Arrangement::LSystem` | ordinal + grammar config |
| Timeline | new `Arrangement::Timeline` | needs `ScoreItem.axis` |
| Kanban | new `Arrangement::Kanban` | needs `ScoreItem.axis` |
| SemanticEmbedding | new `Arrangement::Embedded` | needs `ScoreItem.embedding` |
| Spectral | **same** `Arrangement::Embedded` | adapter runs power iteration → `ScoreItem.embedding` |
| Radial | new `Arrangement::Radial` | adapter runs BFS → `ScoreItem.axis` = ring index; ordinal gives the angular slot |
| Geographic, Hulls | stay in `scenomise` | no arrangements counterpart |

Net: six new typed variants, two reconciliations, one merge, plus the
`Custom` escape. Scenograph reaches full catalog parity.

### The Embedded merge

`SemanticEmbedding` and `Spectral` are the same solver. Both take
precomputed 2-D coordinates, apply origin + scale + rotation, auto-fit to a
bounded extent, and place. `SemanticEmbeddingConfig` is
`{ origin, scale, rotation, fallback }`; `SpectralConfig` is
`{ center, scale, iterations }`, and `iterations` is the power-iteration
count — graph work, not placement work.

Strip the producer and the placement halves are identical, so they collapse
into one `Arrangement::Embedded { origin, scale, rotation, fallback }` with
two upstream producers:

- an ML pipeline (UMAP / t-SNE / PCA) — today's `SemanticEmbedding`
- the graph Laplacian's two smallest non-trivial eigenvectors — today's
  `Spectral`

`EmbeddingFallback` (`LeaveInPlace` / `CollapseToOrigin` / `RingOutside`)
moves to sceno with the variant; it is the answer to "what about items with
no embedding", which both producers need.

Two adapters over 650 lines become one solver plus two producers. This is
the duplication the migration exists to remove, and it was invisible while
the two lived in different crates under different names.

## What landed, and where it diverged from this plan

**S1 and S2 are done.** `cargo test -p sceno` 26/26, `cargo test -p scenomise`
66/66. Five things turned out differently than written above; each is in the
code with its reasoning beside it.

1. **Three disclosure fields, not two.** `ScoreItem` gained
   `weight: Option<f32>` alongside `axis` and `embedding`. Radial's
   degree-weighted angular policy needs a per-item scalar that is not a
   coordinate, and `RoutedRelation::weight` already established both the name
   and the meaning on the scene side. Inventing a second vocabulary for the
   same idea would have been the worse cost.
2. **`NodeAssignmentStrategy` dissolved rather than moved.** Penrose's
   five-variant assignment enum had four variants documented as falling back
   to the fifth, and none implemented. All five describe an *ordering*, which
   `ScoreItem::ordinal` already is: a producer wanting graphlet-adjacent items
   on adjacent vertices sorts them into adjacent ordinals.
3. **`StackFlow` moved to the producer.** Whether sources precede targets is
   a question about ranks, and by the time a score exists the ranks are
   numbers. Reversing them is the producer's arithmetic.
4. **`LSystemGrammar::Custom(CustomGrammarHandle)` became `Named(String)`**,
   to keep it distinct from `Arrangement::Custom`. It names production rules,
   not an arrangement.
5. **One Penrose behaviour fixed rather than preserved.** `sort_center_out`
   ranked vertices by distance from `config.center` *before* translating the
   origin-generated tiling onto it — so away from the default centre of
   `(0, 0)`, "centre-out" ran outward from somewhere else. The two agree
   exactly at the default.

**"Leave it in place" needed a meaning.** Three fallbacks
(`TimelineFallback::LeaveInPlace`, `EmbeddingFallback::LeaveInPlace`,
`RadialUnreachablePolicy::LeaveInPlace`) worked in `arrangements` because a
stepper could emit no delta and let the live position stand. A solved scene
has no previous frame. They now resolve to the item's disclosed
`Placement::Coordinate` if it has one and the arrangement's origin otherwise —
the only thing a stateless score can mean by "where it already is".

**The S2 parity tests moved to S5.** Asserting placement parity against the
`arrangements` predecessors inside `scenomise` would require scenomise to
depend on `arrangements`, which inverts the layering the migration exists to
fix. S2 ships behavioural tests per family instead; the parity assertion
belongs at S5, where both the old adapter and the new one produce a
`cartography::Projection` and are directly comparable. **S5's done-condition
is now the only parity gate, which makes it load-bearing.**

Two tests are worth knowing about because they guard the whole absorption:
`a_hold_outranks_the_arrangement_in_every_family` now enumerates all eleven
families rather than the original four, so the seven new ones cannot become
seven new ways to quietly move a pin; and `every_family_places_every_item`
catches a family that returns an empty plan, which every hold test would
otherwise pass.

## Stages

### S1 — sceno contract

- `Arrangement` grows seven typed variants — `Stack`, `Penrose`, `LSystem`,
  `Timeline`, `Kanban`, `Embedded`, `Radial` — plus
  `Custom { id: String, config: serde_json::Value }`. With the four that
  exist (`Spiral`, `Grid`, `Geographic`, `Hulls`) that is eleven typed
  families and one escape.
- `ScoreItem` grows `axis: Option<AxisValue>` and `embedding: Option<Vec2>`,
  both `#[serde(default)]`.
- `AxisValue` (`Numeric(f64)` / `Categorical(String)`) and
  `EmbeddingFallback` (`LeaveInPlace` / `CollapseToOrigin` / `RingOutside`)
  move from `arrangements` into `sceno`.
- `SCORE_VERSION` → 4, with the rejection note in the doc comment extended
  to say what a v3 reader would drop.

**Done when:** sceno compiles with no new dependencies beyond the `Custom`
config value type; a v4 score round-trips through serde; a v3 score
deserializes with the new fields defaulted; `cargo test -p sceno` green.

### S2 — scenomise solvers

- Port the seven solvers from `arrangements` into `scenomise::solve`,
  dropping the `Layout<N>` wrapper and the damping field. `Embedded` is
  written once and serves both former `SemanticEmbedding` and `Spectral`
  callers; `Radial` reads its ring from `ScoreItem.axis` rather than
  walking a graph, which is what retires `RadialConfig<N>`'s generic.
- Reconcile Spiral against Phyllotaxis and Grid against arrangements' Grid;
  where behaviour differs, the scenomise form wins and the difference is
  noted in the solver doc comment.
- Existing hold semantics (`Hold::Pinned` / `Hold::Anchored`,
  `unmet_holds`) apply uniformly to the new variants. The bug called out at
  `solve.rs:37` — a Coordinate placement honored by Geographic and Hulls but
  silently discarded by Spiral and Grid — must not be reintroduced for any
  new family.

**Done when:** each ported solver has a test asserting placement parity with
its `arrangements` predecessor on a fixture score; `cargo test -p scenomise`
green.

### S3 — registry in scenograph

- De-genericize `arrangements::registry` (`LayoutRegistry<N>`,
  `LayoutProvider`, `LayoutCapability`, `LayoutCategory`,
  `LayoutProvenance`, `RegisterError`, `dyn_layout`) onto `SourceRef`
  identity and rehome it in the `scenograph` facade crate.
- `Arrangement::Custom` dispatches through it; built-in variants do not.
- `register_builtins` retires — built-ins are enum variants now.

**Done when:** a fixture custom solver registers, solves, and round-trips
its id through a persisted score.

**Landed**, 10/10 in `scenograph::registry`. Three parts of the original did
not survive, each because the absorption removed its reason to exist:

- **`DynLayout` is gone.** It erased `Layout::State` into
  `Box<dyn Any + Send>` so stateful and stateless layouts could share a trait
  object. Every solver here is closed-form and has no state to erase, so a
  plain object-safe `Solver` trait does the whole job.
- **`LayoutProvenance` is gone.** Its three variants distinguished built-ins
  from mods in a catalog that held both. The eleven named families are enum
  variants and are never registered, so everything in this catalog is external
  by construction and the field could only ever say one thing.
- **`LayoutCategory` is gone.** `Force` and `Projection` name iterative
  behaviour that has no solver here; `Positional` and `Extras` would then be
  the only inhabited variants of a four-variant enum. Tags carry the filtering
  that remains.

One thing was added rather than ported: `SolverCapability::requires`, naming
the disclosures (`Axis`, `Embedding`, `Weight`) a solver reads. The disclosure
mechanism means a solver handed a score that disclosed nothing places
everything as though every item disclosed nothing — a plausible-looking wrong
layout rather than a visible failure. Declaring the requirement lets
`scenograph::solve` fail by name before computing a single position, which is
the failure mode this design introduced and therefore owes a guard.

`scenomise::solve_with` is the seam: it takes a planner consulted only for
`Arrangement::Custom`, so the registry can supply a solver without `scenomise`
learning what a registry is, and without anyone re-implementing interning,
bounds, the hold report, or the Hulls partition. A planner returning the wrong
number of positions is refused (`SolveError::CountMismatch`) rather than
indexed into.

### S4 — easing in scenotime — **nothing to build**

This stage dissolved on contact with what already exists. `scenotime`'s
`transition` module supersedes `StaticLayoutState`'s damping outright:
`TransitionSpec { duration_ms, easing, stages }` with `TransitionSchedule`
already stages enter/update/exit, staggers in stable instance order, is
host-clocked, and is pure.

It is also the better mechanism, which is why nothing should be carried
across. `damping` was a fixed fraction of the delta-to-target applied per
frame — an exponential approach that asymptotes and never exactly arrives, on
a `step_count` that nothing read and nothing terminated. Scenotime's easing is
bounded in duration with exact endpoints (`TransitionEasing::EaseInOut` is
smoothstep), and `host_time_is_pure_and_exact_at_both_ends` already pins the
arrival that damping could not promise.

**The done-condition written above was wrong to want.** "The same per-frame
positions the old `damping: 0.2` path did" would mean reproducing an
asymptote inside a system built to land exactly, which is a regression dressed
as parity. The `damping` field is dropped with the rest of the stepper
wrapper, and callers wanting eased motion build a `TransitionSpec`.

**Done when:** nothing. Recorded so the field has a named successor rather
than appearing to have been lost.

### S5 — cartography adapters

- Move all nine adapters (`grid`, `kanban`, `lsystem`, `penrose`,
  `phyllotaxis`, `radial`, `semantic_embedding`, `spectral`, `timeline`)
  plus `shared` into `cartography`, rewritten to build a `Score` and call
  `scenomise::solve`.
- `radial` and `spectral` keep their graph work and become **producers**:
  radial's BFS fills `ScoreItem.axis`, spectral's power iteration fills
  `ScoreItem.embedding` (see "Adjacency: Radial and Spectral"). Both then
  hand off to a scenograph solver rather than placing anything themselves.
  `spectral` and `semantic_embedding` emit the same
  `Arrangement::Embedded` score and differ only in which producer ran.
- `crates/canvas/canvas/src/cartography_scene.rs:194` —
  `project_canvas_dispatch` — is the **only** non-test `arrangements::`
  reference in mere. Repoint it at the cartography adapters.

**Done when:** `CANVAS_LAYOUT_STRATEGIES` dispatch produces identical
`Projection`s for every strategy id on the existing fixtures, **except** for
the divergences named below, each of which carries its own test pinning the
new behaviour and a comment saying why it changed.

Byte-identical parity is not reachable, and pretending otherwise would mean
either a gate nobody can pass or a gate quietly weakened until it does. Three
behaviours changed on purpose:

| divergence | old | new |
|---|---|---|
| Penrose centre-out | ranked vertices by distance from `config.center` before translating the origin-generated tiling onto it | ranks from the tiling's own origin, then translates — identical at the default centre of `(0, 0)` |
| `LeaveInPlace` fallbacks | emitted no delta, letting the live position stand | resolves to the item's disclosed `Placement::Coordinate`, or the arrangement origin |
| Timeline zero span | divided by zero into NaN when every item disclosed the same coordinate | places at the axis origin and stacks |

Everything else must match position for position. This is the migration's only
parity gate, since S2's moved here.

**Landed**, 13/13 in `cartography::adapters::parity`. The goldens were
captured by running the old adapters on a fixture graph — a hub with three
spokes, a detached pair, one isolate — immediately before deleting the crate,
so the assertions outlive what produced them.

**The gate paid for itself three times.** Building the adapters against the
new solvers looked finished and was not; the goldens caught all three before
anything shipped:

1. **Grid spacing and columns.** The old `GridConfig` was `gap: 120` with
   `GridColumns::Auto` (`ceil(sqrt(n))`); `sceno::Grid::default()` is a
   64×64 cell with a 4-unit gap over 8 columns. Straight config reuse would
   have rescaled and rewrapped every grid. `GridAdapter` now owns a
   `GridColumns` resolved per request — it stays adapter-side because two of
   its three modes need the item count, and a persisted score should mean the
   same thing however many items it carries.
2. **Spectral reach.** The old adapter fit its coordinates into 320, not
   `Embedded`'s 400.
3. **Kanban columns.** The old board gave every unlisted tag its own column,
   sorted alphabetically, and reserved the trailing column for items with no
   tag at all. The first implementation folded every unlisted tag into one
   pile — which, with `column_order` empty as every current caller leaves it,
   would have collapsed the whole board into a single column. Fixed, and the
   sort is now pinned by `unlisted_tags_order_alphabetically_not_by_appearance`.

A fourth was caught while reading rather than by the gate: `Radial`'s uniform
policy puts the first slot exactly on `rotation_offset`, and the first
implementation centred each item in its slot, rotating every ring by half a
step. Only the weighted policy centres. Pinned by
`the_first_uniform_slot_sits_exactly_on_the_rotation_offset`.

The origin trick is gone. `run_static_layout_one_shot` built a scene with
every node at the origin and `damping = 1.0` so the returned *delta* equalled
the absolute target — a workaround for `Layout::step` returning deltas, with
nothing left to work around once `solve` returns positions.

### S6 — delete arrangements

- Remove `crates/canvas/arrangements` and its workspace member entry.
- `CanvasSceneInput` / `CanvasNode` / `CanvasEdge` / `CanvasViewport` move
  to `cartography` — they are graph-shaped, not portable.

**Done when:** `cargo check --workspace` green with no `arrangements`
reference remaining; `cargo test --workspace` green.

**Landed.** `crates/canvas/arrangements` is deleted (6,785 lines), along with
its workspace member entry, its workspace dependency, and `mere-canvas`'s
dependency on it.

`CanvasSceneInput` / `CanvasNode` / `CanvasEdge` / `CanvasViewport` did **not**
move to cartography as planned — they went away. They existed to feed
`Layout::step`, and nothing feeds it any more: cartography's adapters read the
graph directly into a score, and `mer3ly`, their only other consumer, does the
same. Moving them would have preserved a shape whose only purpose was the
interface being retired.

**The workspace gate is green: 3,347 library tests, zero failures.** Getting
there turned up two latent failures that had nothing to do with this
migration, recorded here because the sequence is worth knowing.

`cargo check --workspace` reported one error in
`crates/system/pandect/src/image_store.rs` — `missing field content_hash in
initializer of PersistedNode` — in a test fixture that predated the field.
It was pre-existing: the file was at committed state and nothing here touches
`pandect` or `graph-kernel`. Repairing it made pandect's tests compile for
the first time, and that **un-masked three failures that had been unrunnable
rather than passing**, all of them `#[cfg(windows)]` legacy-record
migrations.

Their cause was in `personae`, not `pandect`. `current_revision` reads a
record before saving it, to check freshness against rollback, and handled
exactly two cases: absent, or a parseable sealed envelope. A record written
before sealing existed is neither — plain JSON, or for a raw seed, not JSON
at all. The parse failed, so the save aborted, so the migration that would
have sealed it could never run: sealing a legacy record required it to
already be sealed. It now reconciles as an absent record does, which is what
`reconcile_freshness` already called the case, its parameter being named
`legacy_or_absent`. Deliberately narrow — a file *shaped* like an envelope
that still fails to parse keeps erroring, because treating a damaged sealed
record as absent would let a rollback overwrite it unnoticed.

A fourth surfaced the same way in `graphshell-client`:
`satisfaction_reads_both_halves_off_the_wire` invented a held position that
could never match its instance, so the snapshot was rejected and the test
never reached the counting it exists for.

Both were proven pre-existing by checking out `4bbbceeb` — the commit before
this migration — and running them there, not by inference. Both are fixed in
mere `330eee98`. The lesson worth carrying: a crate whose test binary does
not compile reports no failures, which reads exactly like passing.

### S7 — downstream

- `mer3ly/crates/repo-graph` is the only external consumer of
  `arrangements` (`camera::CanvasViewport`, `scene::{CanvasEdge, CanvasNode,
  CanvasSceneInput}`, `LayoutProvider<String>`), pinned at rev `546991d`.
  It already calls `scenomise::solve` as well. Migrate it to the scenograph
  path and repin.
- Repos on scenograph only (`retinue/signalman-desktop` rev `1609cb9`,
  `mesocosm`, `woodshed-core`, `turnstone`, `cleromancy`) need no source
  change, but `SCORE_VERSION` 4 means any persisted v3 score they hold must
  survive the default-field path — verify per repo before bumping pins.

**Done when:** every repo above builds against post-migration mere, and
`mer3ly` names one engine.

**Landed for `mer3ly`**, 26/26 green — including
`arrangement_catalog_preserves_graph_identity`,
`every_registered_arrangement_is_selectable_or_explained`,
`radial_focus_is_host_configurable`, and
`timeline_uses_one_proportional_axis_with_collision_free_strips`, which are
behavioural checks over the arrangement output rather than compile checks.

`crates/repo-graph/src/arrangement.rs` is new and holds the two things the
registry used to supply:

- **The catalog.** `LayoutRegistry::<String>::default()` provided both
  metadata and dispatch. Scenograph's families are enum variants, so what
  remains is a local table of the nine arrangements *this site* offers, with
  the names it shows. The descriptions carried over from the registry's
  capabilities, keeping the site's own overrides.
- **The producers.** `radial_rings` (breadth-first), `stack_layers` (Kahn's
  longest-path, cycles in one overflow layer), and `degree_weights`. These are
  the graph walks the old `Radial` and `Stack` did inside `step`.

Two things worth recording:

- `stack_layers` **de-duplicates parallel edges before counting indegree.**
  The original deduplicated its adjacency but kept the pre-dedup indegree, so
  a repeated edge left its target's count permanently above zero and stranded
  it in the cycle overflow. Pinned by `a_repeated_edge_is_one_dependency`.
- Radial's unreachable set is now read from the ring map rather than inferred
  from a missing delta. The old code found them via
  `!deltas.contains_key(id)`, which worked only because `LeaveInPlace` emitted
  nothing; the solver now places every item, so the absence has to be read
  where it actually lives.

`mer3ly` is unpinned from rev `546991d` only in the sense that it no longer
depends on `arrangements`; its remaining `mere` deps still name that rev and
must be repinned once mere is pushed. A gitignored
`mer3ly/.cargo/config.toml` resolves them to the local working copy in the
meantime, the same mechanism mere uses for its own git siblings.

The other consumers (`retinue/signalman-desktop`, `mesocosm`,
`woodshed-core`, `turnstone`, `cleromancy`) take `sceno`/`scenomise`/
`scenotime`/`seiche` and never touched `arrangements`, so none needs a source
change. **`SCORE_VERSION` 4 still applies to them**: a stored v3 score loads
with the three disclosure fields defaulted to absent, which
`v3_score_loads_with_disclosure_fields_absent` proves, but each repo's pin
should be bumped deliberately rather than drifting.

## Out of scope

- **seiche** keeps the live force physics. `scenomise::relax` and seiche are
  not merged by this plan; whether the relaxation pass and the force solver
  should converge is a separate question worth asking later.
- **cambium's `graph_canvas`** (`repos/genet/components/cambium`, 2,167
  lines) and sprigging's `GraphCanvas` leaf are a third node-link renderer
  at swatch tier, with their own pan/zoom projection. Mere does not consume
  it — only `AnyView`, the caret types, and persona-picker widgets —
  `woodshed-views` does. Untouched here.
