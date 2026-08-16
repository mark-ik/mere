# Projection Grammar Adoption Plan

**Date**: 2026-08-15
**Status**: design; no target started. Turns the projection grammar report's
findings (the claude.ai design artifact "Projection Grammar Report", two
passes, sources verified 2026-08-15) into gated feature targets across mere,
genet, and cambium. Sequenced against the
[scenograph expansion brief](../research/2026-08-10_scenograph_expansion_brief.md)
lanes L1-L5 and governed by the
[projection grammar catalog](../research/2026-08-15_projection_grammar_catalog.md)
promotion rules.
**Related**:
[scenograph_freeze_plan](2026-07-24_scenograph_freeze_plan.md) (0.0.3 frozen;
rulings D1-D4),
[projection_proofs_plan](2026-07-21_projection_proofs_plan.md) (P1-P5 landed),
scene contract note
(`crates/scenograph/design_docs/2026-07-22_scene_contract_note.md`),
[multi_window_plan](2026-06-10_multi_window_plan.md),
[graph_signals_layer_plan](2026-06-22_graph_signals_layer_plan.md),
[accesskit_screen_reader_verification](2026-06-09_accesskit_screen_reader_verification.md).

## Ruling context

The report reviewed the catalog against eleven external systems (Vega-Lite,
Draco, SetCoLa, Gemini, GoTree, ATOM; then Mosaic, Gosling, Penrose, Bluefish,
GoFish) and returned three results this plan acts on: the catalog's stack
holds and its authority layer is genuinely novel; every named contract gap has
a shipped anatomy to lift rather than invent; and the eleven systems are prior
art for the *compiler* (what a projection spec means), not for drawing scenes.

Two disciplines from the report govern every target here:

1. **Solver proposes, the score records.** Draco-style search and
   Penrose-style global optimization live above the score, never inside it.
   The score stays a deterministic record of what was chosen; determinism
   receipts stay meaningful. (Report caution 4.)
2. **A layer enters the portable contract only when a promotion proof fails
   without it.** GoTree's complexity cliff is the cost of adopting
   speculatively; the catalog's promotion rules are the defense. Nothing in
   this plan unfreezes 0.0.3; contract additions are 0.0.4+ material behind
   forcing consumers. (Report caution 2.)

## Findings

What transfers, from where, to where. Landing sites verified against the tree
2026-08-15 (score shape read from `crates/scenograph/sceno/src/score.rs`;
freeze rulings from the freeze plan; genet components listed from
`repos/genet/components/`).

| Transfer | Source system | Landing site |
| --- | --- | --- |
| Placement satisfaction state; pin = ensure (fails loudly), anchored home = encourage | WebCoLa silent-soft caution + Penrose `ensure`/`encourage` | sceno scene surface + scenomise solvers (A1), cambium chrome (C1) |
| Selection clauses with declared resolution (single / union / intersect / crossfilter) | Mosaic selections + Vega-Lite selections | chirograph intent plane / mere host coordination (A2) |
| LOD rungs as declarative conditions (measure, operation, threshold, hysteresis) | Gosling `visibility` | cartography representation profiles, then `ScoreItem.representation` selection (A3) |
| Transition specs between epochs, host-owned clock | Gemini | scenotime, expansion lane L5 (A4, C2) |
| Set-scoped constraints over predicate-defined member sets | SetCoLa | future score constraint records, gap 2 proof (A5) |
| Factored arrangement dimensions instead of new monolithic variants | GoTree + ATOM | `score::Arrangement` evolution at the hierarchy/tiling proofs (A5) |
| Rule-filled scales and guides; unit/aggregate distinction | Vega-Lite + ATOM | gap 1 proof (A5) |
| Per-channel shared/independent scale resolution | Vega-Lite `resolve` | gap 4 proof (A5) |
| Backdrop property classing (visible / collidable / hit / provenance; layout vs paint) | Mapbox Style Spec | expansion lane L2 backdrops (C3) |
| Accessible frozen realization as a standing receipt | none external; catalog's W3C citations stand | promotion rules + genet realization (B1) |
| Compound scenegraph shape (hierarchy + adjacency together) | Bluefish | sceno `Scene` already carries spaces, regions, layers; hold the shape deliberately, add nothing now |
| Effectiveness knowledge versions beside the grammar, never inside it | Draco 1 vs Draco 2 | wherever defaults/effectiveness land; `SCORE_VERSION` covers the wire, not the knowledge |

Current portable baseline the targets extend: `Score` v1 with
`Arrangement::{Spiral, Board, Geographic, Hulls}`,
`Placement::{Ordinal, Cell, Coordinate}`, per-item footprint, pre-selected
`Representation` rung, layer, visible. Intents live in chirograph (freeze
ruling D1); picking in scenotime (D4); emphasis channels open (D3).

## Plan

Three tracks. Every target names its forcing consumer; a target with no
consumer yet states its entrance gate and waits.

### Track A: mere (contract and compiler)

**A0. Catalog second shelf (docs only, open now).**
Context: the catalog's external-systems section reads as one shelf of
renderers and toolkits; the report's central structural finding is that
specification languages and design solvers are a different kind of prior art.
Tasks: split "What the external systems teach us" into two shelves; add the
eleven report systems with one-line transfers; add the Penrose name-collision
note (CMU's Penrose diagram language is unrelated to the
`graph_layout:penrose` tiling arrangement); link the report artifact.
Validation: catalog cites all eleven; DOC_README updated.
Done when: a reader of the catalog can find every system the report verified
without leaving the repo.

**A1. Placement satisfaction state (investigation first).**
Context: the catalog's free/anchored/pinned policies currently have no
satisfaction surface. Authored coordinates exist today (isometry adapts
authored pins to a geographic score via `Placement::Coordinate`), and two
mechanisms may displace placed items after the arrangement speaks
(`scenomise::relax`, viewport fits). WebCoLa's silent-soft failure is the
warned outcome; Penrose's ensure/encourage is the vocabulary: a pin is
ensure-class (satisfied or reported), an anchored home is encourage-class
(best effort by design).
Tasks: first, audit where a `Coordinate`-placed item can move after solving
and whether anything records it. Then, with the forcing consumer, give the
realized scene a satisfaction record for ensure-class placements (shape
decided by the consumer: a typed field or a recognized channel). An unmet pin
is reported, never silently repositioned.
Forcing consumer: the first host that surfaces pinned placement to a user
(isometry's VTT pins or the canvas pin/unpin intent, whichever ships).
Validation: a score with an unsatisfiable pin produces a scene that carries
the violation; a test asserts the violation is present rather than the pin
silently best-efforted; the record crosses the graphshell wire.
Done when: a remote viewer can distinguish "placed as pinned" from "pin
unmet" without source access.

**A2. Selection clauses: coordination as data.**
Context: freeze ruling D1 stands (sceno ships no intent vocabulary; the
protocol owns the triple). What the report adds is the *coordination* record
between views: Mosaic's clause (source, client set, predicate, value) with
declared resolution (single, union, intersect, crossfilter), where crossfilter
means a view is filtered by every brush but its own. Mere's one-app-state,
N-windows posture (multi_window_plan, expansion lane L3) is the shipped
precedent's exact use case; a Mere clause carries a reading-parameter delta
against graph authority, never SQL.
Tasks: with the forcing consumer, define the clause record and resolution
declaration; decide its home with evidence (chirograph beside the intent
triple, or mere host state that graphshell serializes); wire two views over
one authority through it.
Forcing consumer: the first two-view coordination ask: two windows brushing
one graph, a canvas and a swatch cross-filtering, or a graphshell remote
filter (entrance gate per L3: the first consumer that asks for the same
arrangement or filter from a second window, device, or peer).
Validation: brush in view one filters view two; crossfilter resolution
honored (the brushing view is unfiltered by its own clause); serialized round
trip is deterministic; clause removal restores the unfiltered reading.
Done when: brush, filter, and focus are named, serialized citizens rather
than host-only state.

**A3. LOD rungs as declarative conditions.**
Context: `ScoreItem.representation` is a pre-selected rung; the conditions
that select it live in host code, so a remote client cannot re-select on its
own zoom and a frozen realization cannot state why a rung was chosen.
Gosling ships the missing form: target, measure (screen-space width/height
vs data-space zoomLevel), operation, threshold, and hysteresis padding as
data. The measure split mirrors Mapbox's layout/paint classing: screen-space
conditions are realization-dependent, data-space conditions are
reading-dependent.
Tasks: stage one, conditions as data in cartography's representation
profiles (host-side registry; the P3b card-to-glyph traversal is the named
consumer). Stage two, portable only when a remote consumer needs client-side
re-selection: rung conditions travel beside the score, and a frozen
realization evaluates them at freeze zoom deterministically.
Forcing consumer: P3b (the recorded remaining half of P3: representation
degrades card to glyph with recency and zoom, focus stays live).
Validation: same score, two freeze zooms, different rungs, deterministic; a
hysteresis test shows a rung boundary does not flicker under small zoom
oscillation.
Done when: the representation ladder is data a second host can honor without
porting mere's selection code.

**A4. Transition specs between epochs.**
Context: expansion lane L5 named motion the deepest of the missing 90%;
`scenotime::diff` computes what changed and every consumer snaps. Gemini's
result: transitions specified relative to explicit start and end states, over
component classes, with sync/concat composition, and the authors rejected
extending the visualization spec itself. Scenotime epoch pairs are the
natural start/end states; the host owns the clock (the same seam discipline
`mere-mesh-host` uses for `Clock`).
Tasks: a transition spec over diff output (which item classes, what staging,
duration ratios); pure evaluation against host-supplied time in scenotime;
default staging so consumers get respectable motion for free; playback in the
first continuous consumer.
Forcing consumer: L5's entrance gate verbatim: the first continuous
re-projection a consumer ships (woodshed's rehearsal filmstrip or a canvas
projection switch, whichever lands first). C2 is the cambium half.
Validation: identical scene pair plus identical spec yields an identical
schedule; a frozen realization is unaffected by transition specs; motion
stays out of arrangement types (the catalog's motion taxonomy holds).
Done when: a projection switch reads as a staged transition, specified as
data, on at least one consumer, with snapping still the default elsewhere.

**A5. Gap proofs adopt named anatomies (gated on the promotion suite).**
Context: the catalog's first promotion suite (one heterogeneous fixture as
orrery, matrix, Cartesian chart, hierarchy, schematic) will force contract
material. The report's job was to make sure none of it is invented fresh.
Tasks, each strictly behind its proof:
- Chart proof (gap 1): scales, axes, legends filled by rule with derivation
  recoverable, per Vega-Lite; the unit/aggregate distinction per ATOM ("a bar
  over twenty nodes is not a twenty-first node" is already catalog law).
- Facet proof (gap 4): per-channel shared/independent resolution, lifted from
  Vega-Lite `resolve` with its default resolutions.
- Schematic proof (gap 2): ELK's port and label anatomy for endpoints;
  SetCoLa's set-scoped constraint form (constraints over predicate-defined
  sets, instance generation deferred to the runtime) so a saved layout
  reapplies to a second graph, which is the catalog's second-dataset receipt.
- Hierarchy proof: before adding tree/treemap as new monolithic
  `Arrangement` variants, evaluate GoTree's factoring (element x placement
  rule x coordinate transform) and ATOM's recursive partition operators so
  node-link and space-filling are parameter settings of one family.
- Every proof: an accessible frozen realization in the receipt (B1 defines
  the shape). Read GoFish in full before the facet and flow proofs; it is the
  chart-side proof of the catalog's central bet.
Validation: per the catalog's promotion checklist, unchanged.
Done when: each gap's contract addition cites the anatomy it lifted and the
proof that forced it.

### Track B: genet (realization receipts)

Genet is the realization layer of the projection stack: the accessible
frozen form and the drivable interactive form both land on its surfaces.
Pointer docs in `genet/docs/` are founded when the first slice opens, not
before.

**B1. Accessible frozen realization (open now; report's one immediate
recommendation).**
Context: no surveyed grammar treats the accessible form as a first-class
realization target; the catalog's W3C citations (WAI complex images, Graphics
ARIA, SVG structure) are the anatomy. Retrofitting accessibility contracts is
the expensive order, so the receipt shape should exist before the promotion
suite starts producing receipts.
Tasks: define the receipt shape (navigable structure, names, descriptions,
values, relations, and a tabular or long-form alternate where the visual
alone is insufficient); realize one existing scene (the P3 spiral score or
the P5 `coastal_map.json` fixture) as a frozen semantic form through genet's
DOM lane; verify with the AccessKit lane precedent
(accesskit_screen_reader_verification, 2026-06-09) and a genet-probe scenario
asserting the semantic tree (apps self-drive via genet-probe; never synthetic
OS input).
Forcing consumer: the promotion suite itself; every proof's receipt cites
this shape.
Validation: a screen-reader traversal of the frozen projection enumerates
instances and relations with names; a probe scenario asserts structure
deterministically; the same scene still produces its interactive realization.
Done when: "frozen with navigable semantics" is a checklist item the first
promotion proof can satisfy by following an existing worked example.

**B2. Probe-drivable projections.**
Context: genet-probe drives applications through DOM-carried identity, and
chirograph intents target an `InstanceId`. The two identities should meet:
a scenario should pick a projected instance and invoke an intent on it by
stable identity, not by coordinates.
Tasks: projected instances expose stable identity to the probe resolver at
whichever host surface renders them; a scenario verb resolves instance to
intent invocation.
Forcing consumer: the first headed receipt that asserts an intent against a
projected instance (the promotion suite's interaction receipts, or the A1
pin receipt, whichever runs first).
Validation: a scenario addresses an instance by identity, invokes an intent,
and asserts the `IntentResult`; renaming or moving the instance on screen
does not break the scenario.
Done when: projection receipts are driven by identity end to end.

**B3. Livery property-classing check (verify first, then close or record).**
Context: Mapbox's layout/paint split is prior art for classing every visual
property by what it invalidates. Livery's TOML property DB likely already
carries an invalidation class per property for its own engine needs.
Tasks: read livery's property DB schema; if classing exists, record the
rhyme in livery's docs and close this item; if it does not, weigh adding the
class with livery's own consumers, not on the report's authority.
Validation: one paragraph of recorded evidence either way.
Done when: the question is answered from the tree, not the report.

### Track C: cambium (host consumption)

Cambium is where scenes meet users: `cambium-genet-winit-host` is the
single-root host, woodshed then signalman its consumers, and swatches are the
agreed cross-product graph-view contract. Cambium doc updates land in
`genet/components/cambium/docs/` when a slice opens.

**C1. Satisfaction state in host chrome (consumer half of A1).**
Context: A1's scene-side record is only honest if a user can see it.
Tasks: with A1's consumer, surface pin state in the host's widget chrome
(a pinned badge; an unmet-pin state visibly distinct); keep the vocabulary
plain (pinned, held, unmet), not solver jargon.
Validation: the headed receipt shows a pinned item, a displaced anchored
item returning home, and an unmet pin visibly reported.
Done when: no best-effort placement is presented as satisfied truth in any
cambium-hosted view.

**C2. Transition playback with the host clock (consumer half of A4).**
Context: scenotime evaluates transitions purely; a cambium host owns time.
Tasks: the host drives A4's schedule with its frame clock; woodshed's
rehearsal filmstrip is the named first consumer (L5); the canvas projection
switch is the alternate.
Validation: pausing the host clock pauses the transition; identical inputs
replay identically; a consumer that never adopts transitions still snaps.
Done when: one shipping cambium consumer plays a staged epoch transition.

**C3. Backdrop realization (consumer half of expansion lane L2).**
Context: L2's two waiting consumers (isometry's map, woodshed's stage floor)
force the backdrop contract; the report adds the vocabulary: class backdrop
properties explicitly (visible, collidable, hit-transparent, provenance;
which properties are placement-class vs paint-class), per Mapbox's decade of
production answers, so "a backdrop may be visible, collidable, both, or
neither" is declared data rather than host convention.
Tasks: prototype against both consumers per L2's entrance gate; carry the
property classing into whatever shape both force; a remote graphshell viewer
renders the backdrop from scene data alone.
Validation: L2's own gate, plus: the same backdrop crosses the wire and
renders identically remote; a hit-transparent backdrop never picks; a
collidable one participates in placement.
Done when: environment is scene data with declared properties on two
consumers.

### Sequence

Open now, in order of unlock-per-effort: **A0** (one docs edit), **B1** (the
report's single act-now recommendation; additive, uses existing scenes),
**A1's audit half** (isometry pins are live today; the question is whether
displacement is currently recorded), **B3** (a read of livery's DB).

Opens with existing lanes, no new gates invented: **A2** with L3's entrance
gate, **A3** with P3b, **A4 + C2** with L5's gate, **C3** with L2's gate.
**C1** follows A1's consumer.

Gated on the promotion suite: **A5** entire.

Non-goals, restated from the governing docs and the report: no unfreeze of
0.0.3; no intent vocabulary in sceno (D1 stands); no global nonconvex solver
in scenomise (deterministic solving is the receipt currency); no speculative
adoption of the report's six-layer spec stack (each layer arrives only with
its proof); no new grammar DSL (the score is the spec; the report's
what-a-spec-means table describes meanings the score may adopt, not syntaxes
to build).

## Progress

- 2026-08-15: plan founded from the projection grammar report (pass one: six
  specification grammars and the five gap anatomies; pass two: the
  Mosaic/Gosling/Penrose/Bluefish/GoFish tail survey and the
  what-a-spec-means typology, the latter arriving from Mark's notes and
  verified against the papers). The report artifact was updated the same day
  with both passes. Landing sites verified against the tree: score shape read
  from `sceno/src/score.rs`, freeze rulings D1-D4 from the freeze plan,
  expansion lanes L1-L5 from the brief, isometry's authored pins and P3b's
  open LOD half from the proofs plan, genet component inventory
  (cambium family, genet-probe, livery) from `repos/genet/components/`. No
  code target started.
