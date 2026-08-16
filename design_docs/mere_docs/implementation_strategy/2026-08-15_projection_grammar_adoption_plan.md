# Projection Grammar Adoption Plan

**Date**: 2026-08-15
**Status**: A0 landed; **A6 first slice landed 2026-08-16** (contract half:
holds in the score, honored by the solver and by relaxation). Mer3ly ruled
an authority-grade consumer 2026-08-16, which re-gates A1-A4, B1, and C3. Turns the projection grammar report's
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

### Consumer ruling (2026-08-16)

Mer3ly is authority-grade: its asks open gates, and it is not a donor in the
way graphshell is. It is live, public, pinned to a real rev, and consumes six
Mere crates plus published cambium and genet-scripted-dom. The
[mer3ly stack consumer survey](../research/2026-08-16_mer3ly_stack_consumer_survey.md)
is the evidence. The practical effect is that "the first consumer that asks"
has a real answer today, so the targets below name mer3ly where they used to
name a hypothetical host, and expansion lane L3's entrance gate is already met
in production.

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

**A0. Catalog second shelf (docs only) — landed 2026-08-15.**
Context: the catalog's external-systems section reads as one shelf of
renderers and toolkits; the report's central structural finding is that
specification languages and design solvers are a different kind of prior art.
Tasks: split "What the external systems teach us" into two shelves; add the
eleven report systems with one-line transfers; add the Penrose name-collision
note (CMU's Penrose diagram language is unrelated to the
`graph_layout:penrose` tiling arrangement); cite the report in-repo rather
than by artifact URL, since mere is a public repo.
Validation: catalog cites all eleven; DOC_README updated.
Done when: a reader of the catalog can find every system the report verified
without leaving the repo.

**A6. The placement seam (numbered late, sequenced first).**
Context: mer3ly consumes the stack along two disjoint paths. The portable path
builds a `Score` with `Placement::Ordinal` and solves it; the live path runs a
seiche simulation with `pin_node`, three-way mobility, and backdrops. `Score`
and `Placement` appear only in the first, pins and mobility only in the second,
and nothing joins them. A visitor who pins three repositories and shares the
result is sharing state the portable contract cannot express, which is why the
site invented its own wire.
The seam is the precondition for three targets that otherwise each wait on a
consumer that does not exist: A1 has no satisfaction to record while pins never
reach a score, A2 has no coordinated view to serialize, and C3 has no portable
backdrop.
Tasks: let a live arrangement's placement reach the score.
`Placement::Coordinate` carries a visitor-placed position; the mobility class
(free, anchored, frozen) is recorded rather than inferred from force
configuration; the solver's proposal and the recorded outcome stay
distinguishable, per the solver-proposes discipline. Decide with mer3ly whether
the seam is a write-back into the score or a sidecar the score references.
Ruling 2026-08-16 constrains the decision: whatever A6 produces is also the
shelfmark's `placement` delta section, one serialization, never a parallel
one (the shelfmark format note owns the envelope; A6 owns this record).
Forcing consumer: mer3ly, authority-grade.
Validation: a pinned arrangement in the sandbox survives a round trip as a
portable artifact; the reconstructed scene places pinned items where the
visitor put them; the determinism receipt still holds for the unpinned case.
Done when: the two paths meet, and a shared scene is a portable artifact rather
than a site-local JSON blob.

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
Forcing consumer: mer3ly, once A6 lands. Its sandbox already ships the
ensure/encourage split (an `anchored` mobility installs a soft `AnchorSpring`,
a `frozen` one hard-pins) with no satisfaction record anywhere, and it already
shares pins to a second device. Until A6, those pins never reach a score, so
there is nothing for a satisfaction field to attach to. Isometry's VTT pins and
the canvas pin/unpin intent remain alternates.
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
Forcing consumer: L3's entrance gate is met. Mer3ly shares a scene by URL
hash, and a second person opening that link asks for the same arrangement,
reading, and selection on a second device. What is still unforced is the clause
shape: the site carries one selected id with no resolution strategy, so union,
intersect, and crossfilter have no consumer yet. Treat the serialization
question as open now and the resolution question as still gated on a genuine
two-view ask.
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
degrades card to glyph with recency and zoom, focus stays live). Mer3ly does
not force this one: it exports `representation_registry()` to JavaScript for
the live path's own UI, while its portable path hardcodes
`Representation::Glyph`, and its fold/expand is a manual toggle over `visible`
plus an untyped fold channel rather than a measure and threshold.
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
projection switch, whichever lands first). C2 is the cambium half. Mer3ly
already ships most of the record this needs, a labeled revision-chained diff
sequence it steps through with a source-time control, so it is the cheapest
place to prove a spec once one exists; what it lacks is the authored half,
duration, easing, and staging.
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
Forcing consumer: the promotion suite itself; every proof's receipt cites this
shape. Mer3ly is the standing argument for it: its sandbox is JavaScript-only,
shipping `data-graph-interface` hidden with a runtime-built node list and a
no-script status reading "Graphshell sandbox not initialized", so the site
serves accessibility by rendering an entirely separate authority-derived static
index instead. That workaround is what B1 retires.
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
renders the backdrop from scene data alone. Mer3ly supplies a third,
already-shipping data point for the minimum property set: `set_backdrop(kind,
tangible)` in the live path, and a backdrop carrying kind and collidable on its
shared wire. Identity plus collidable is what a real consumer reached for
first, ahead of visible, hit-transparent, and provenance.
Validation: L2's own gate, plus: the same backdrop crosses the wire and
renders identically remote; a hit-transparent backdrop never picks; a
collidable one participates in placement.
Done when: environment is scene data with declared properties on two
consumers.

### Sequence

**A0** is landed. Open now, in order of unlock-per-effort: **A6** (the seam;
it is the precondition for A1, A2, and C3, and it has the only authority-grade
consumer in the plan), **B1** (the report's single act-now recommendation;
additive, uses existing scenes, and mer3ly is the standing argument for it),
**B3** (a read of livery's DB).

Opens with existing lanes, no new gates invented: **A2's serialization half**
now that L3's gate is met, with its resolution half still waiting on a two-view
ask; **A3** with P3b; **A4 + C2** with L5's gate; **C3** with L2's gate,
informed by mer3ly's shipped property pair. **A1** follows A6 rather than
preceding it, and **C1** follows A1.

Gated on the promotion suite: **A5** entire.

Non-goals, restated from the governing docs and the report: no unfreeze of
0.0.3; no intent vocabulary in sceno (D1 stands); no global nonconvex solver
in scenomise (deterministic solving is the receipt currency); no speculative
adoption of the report's six-layer spec stack (each layer arrives only with
its proof); no new grammar DSL (the score is the spec; the report's
what-a-spec-means table describes meanings the score may adopt, not syntaxes
to build).

## Open rulings

**The site's wire as an index.** Mark's framing, 2026-08-16: the wire has its
own utility as an index, and that may be a better way to think about it than
promotion. The argument in its favor is already in the code. `score.generation`
is derived from the authority's SHA-256, so a score is a pure function of its
authority. That makes the site's scene state a citation rather than a
transport: dataset, revision cursor, reading, arrangement, and a small delta of
pins, selection, motion, and backdrop. The scene is reconstituted
deterministically rather than shipped, which is why the wire is small. If the
framing holds, the five fields it carries beyond the contract do not all need
to enter the score, and the index becomes its own layer above the contract with
its own promotion question.
Ruled 2026-08-16, all five questions, recorded in the
[shelfmark format note](../technical_architecture/2026-08-16_shelfmark_format_note.md):
the citation is **shelfmark**, a Mere-level format held as a note until the second
shipping consumer; the citation delta and A6's sidecar are one record (A6's
`placement` section is the first standardized member); home is incipit, scoped,
with chirograph the fallback; checkability (`expects.generation`) is required in
v1. The [scene citation index brief](../research/2026-08-16_scene_citation_index_brief.md)
holds the reasoning.

**Apps embedded in the site.** Mark, 2026-08-16: if he could embed full
versions of all his apps in the site, he would. That is a larger scope question
than this plan carries, but it bears on the index ruling directly. An index is
exactly the addressing an embedded app needs for a deep link, and under that
reading authority-grade consumption stops being one site consuming one stack
and becomes the site hosting the family. Recorded here so the index question is
not settled without it.

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
- 2026-08-15: **A0 landed.** Catalog's external-systems section split into
  two shelves (renderers/toolkits/foundations, then specification languages and
  design solvers) with all eleven report systems cited and one-line transfers.
  Source URLs checked before writing: two guesses were 404s (`uwdata.github.io/draco2`,
  `uwdata.github.io/gemini`) and were replaced with the live repos; GoTree, ATOM, and
  GoFish resolved to an ACM DOI, the MSR publication page, and the MIT VIS page.
  The report artifact is cited by name and date, not linked: mere is a public repo and
  a claude.ai artifact URL is private, so A0's done-condition (findable without leaving
  the repo) is met by pointing at this plan instead.
  The collision note covers **Mosaic** as well as Penrose: the catalog's own tiling row
  names Mosaic as a packing variant, and it has no implementation in the tree.
  DOC_README: founded the missing catalog entry (the catalog had no index line at all)
  and recorded A0 on this plan's entry.
- 2026-08-16: correction. A0's first pass claimed `graph_layout:penrose` was absent
  from the tree and rewrote the note around `PenroseAdapter` / `penrose.default`. That
  was wrong: the claim came from a grep truncated by `head -20`. Both ids are real and
  distinct, `graph_layout:penrose` being the `LayoutCapability` registry id
  (`crates/canvas/arrangements/src/registry.rs:358`) and `penrose.default` the cartography
  adapter's projection id. Plan text and catalog note restored to the registry id.
- 2026-08-16: **mer3ly ruled authority-grade; A6 founded.** Survey first
  (`../research/2026-08-16_mer3ly_stack_consumer_survey.md`), then Mark's
  ruling: mer3ly's asks open gates. A6, the placement seam, is the new target
  and the new opener, because the site's two paths never meet and that single
  fact is what leaves A1, A2, and C3 each waiting on a consumer that does not
  exist. Re-gated A1 (follows A6; mer3ly ships ensure/encourage with no
  satisfaction record), A2 (L3's gate met by URL-hash scene sharing; resolution
  strategies still unforced), A3 (mer3ly does not force it; the plan's premise
  stands), A4 (mer3ly has the record, not the spec), B1 (mer3ly is the standing
  argument, not a counterexample), C3 (identity plus collidable is the shipped
  minimum). Two open rulings recorded: whether the site's wire is better
  understood as an index than a promotion candidate, and the app-embedding
  ambition that bears on it.
- 2026-08-16: **index ruled; shelfmark founded as a format note.** Mark ruled
  the brief's five questions: note now scoped Mere-level; one record shared
  between A6's sidecar and the citation delta, grounded in the stack; incipit
  as home (scoped: 127 lines, serde + uuid only, register pairs with the
  shelfmark; chirograph fallback if the opaque-sections indirection fights
  the one-record rule); checkability in v1; the name is shelfmark. The
  format note (`../technical_architecture/2026-08-16_shelfmark_format_note.md`)
  is the authority; the founding in code waits for the second shipping
  consumer. A6's task text now carries the one-record constraint.
- 2026-08-16: **A6 first slice landed: the score can hold a placement, and
  every solver honors it.** The audit found three silent displacement paths,
  not one. `Placement::Coordinate` was honored by `Geographic` and `Hulls` and
  silently discarded by `Spiral` (ordinal wins) and `Board` (rank wins), and
  `relax` moved every item with no notion of a pin at all. All three were the
  same bug wearing different clothes: the person said where, the layout
  answered somewhere else, and nothing said so.
  Decision, write-back versus sidecar: **sidecar, keyed by `SourceRef`**. The
  stack argued it, per the one-record ruling. A shelfmark cites by source
  identity, so a per-item field would mean shipping a whole score to cite one
  pin, defeating the index; item indices move when the authority changes and
  source ids do not; and pins are sparse, so absence is the free common case.
  `Score.holds: Vec<HeldPlacement>` is therefore the shelfmark's `placement`
  delta section verbatim, one serialization of a pin.
  Landed: `sceno` gains `Hold` (`Anchored` = encourage, `Pinned` = ensure),
  `HeldPlacement { source, at, hold }`, `Score.holds` (serde-default) and
  `Score::hold_for`; `SCORE_VERSION` 1 -> 2, because an adapter that only knows
  v1 must reject rather than silently drop the one field that carries what
  someone asked for. `scenomise::solve` consults `hold_for` ahead of all four
  arrangement families; `relax_holding` leaves held instances where they stand
  while they still push their neighbours (the asymmetry is the meaning of a
  hard hold), with `relax` preserved as the nothing-held wrapper;
  `pinned_instances` resolves holds to instances through interned sources, so
  a source appearing twice pins both.
  Receipts: 8 new tests, `sceno` 19 -> 21 and `scenomise` 15 -> 21, all green,
  plus `chirograph` (21) and `mere-cartography` (25) unaffected. A v1 score
  still loads and loads as "nothing held"; an unheld score places exactly as
  before, so no existing determinism receipt moves.
  Not done, and deliberately: satisfaction reporting is A1, so today an
  unsatisfiable pin is honored rather than reported. Crate versions stay at
  0.0.3; the bump to 0.0.4 and any republish is Mark's call, and the freeze
  plan's "0.0.4+ material behind forcing consumers" is the sanctioned path
  when he wants it. Mer3ly's own seam (its live path still never builds a
  score) is the next slice.
