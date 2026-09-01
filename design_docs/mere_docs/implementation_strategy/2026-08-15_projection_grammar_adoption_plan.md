# Projection Grammar Adoption Plan

**Date**: 2026-08-15
**Status (reconciled 2026-08-29)**: A0, A6, A1, C1, B1, B2, B3, C3, A3 stage
one, A4+C2, and A2 are closed. Turnstone `648bf19` is B1's definitive close,
including routed screen-reader interaction. The Projection Receipts Plan's
coordinated spatial/Matrix views close A2's resolution half. A3 stage two and
A5 retain their entrance gates. A5 waits for a named product consumer to open
its first proof; the promotion suite begins with that proof and is an evidence
harness rather than a consumer. FT7 is closed through the local, admitted
remote, and frozen Matrix parity receipt. FT8 is closed through Retinue
Signalman's mixed-realization receipt at `8cea8f9`.
The Scenograph family is now on an unpublished 0.0.4 development line with
Score v4. Mer3ly ruled an authority-grade consumer 2026-08-16. This plan turns the projection grammar report's
findings (the claude.ai design artifact "Projection Grammar Report", two
passes, sources verified 2026-08-15) into gated feature targets across mere,
genet, and cambium. Sequenced against the
[scenograph expansion brief](../research/2026-08-10_scenograph_expansion_brief.md)
lanes L1-L5 and governed by the
[projection grammar catalog](../research/2026-08-15_projection_grammar_catalog.md)
promotion rules.
**Related**:
[scenograph_0_0_3_release_plan](2026-07-24_scenograph_0_0_3_release_plan.md)
(historical 0.0.3 release; rulings D1-D4),
[projection_proofs_plan](2026-07-21_projection_proofs_plan.md) (P1-P5 landed),
scene contract note
(`design_docs/scenograph_docs/technical_architecture/2026-07-22_scene_contract_note.md`),
[multi_window_plan](2026-06-10_multi_window_plan.md),
[graph_signals_layer_plan](../../archive_docs/2026-08-20_completed_plans/2026-06-22_graph_signals_layer_plan.md),
[accesskit_screen_reader_verification](2026-06-09_accesskit_screen_reader_verification.md).

## Ruling context

### Workbench component adoption (2026-08-31)

Platen now names its graph-specific split/tab cache `TileLayout`; `Workbench` is a compatibility
alias. Its structural AccessKit projection lives at
`platen::accessibility::project_tile_layout`, beside the layout it reads. The old Mere
`workbench` package is retired. Mere and Graphshell consume Genet's reusable `workbench` package
at immutable revision `eff0cb6df4834ecce9ac552a055c1c459befa7c3`. The component boundary and
remaining headed-host receipts are in
`genet/design_docs/2026-08-31_workbench_component_plan.md`.

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
    speculatively; the catalog's promotion rules are the defense. Published
    0.0.3 remains an historical artifact; `main` evolves through explicit
    score and crate versions behind forcing consumers. (Report caution 2.)

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
0.0.3 release rulings from the release plan; genet components listed from
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
| Accessible static realization as a standing receipt | none external; catalog's W3C citations stand | promotion rules + Graphshell realization targeting Genet surfaces (B1) |
| Compound scenegraph shape (hierarchy + adjacency together) | Bluefish | sceno `Scene` already carries spaces, regions, layers; hold the shape deliberately, add nothing now |
| Effectiveness knowledge versions beside the grammar, never inside it | Draco 1 vs Draco 2 | wherever defaults/effectiveness land; `SCORE_VERSION` covers the wire, not the knowledge |

Current portable baseline the targets extend: `Score` v3 with
`Arrangement::{Spiral, Grid, Geographic, Hulls}`,
`Placement::{Ordinal, Cell, Coordinate}`, per-item footprint, pre-selected
`Representation` rung, layer, visibility, authored holds, and honored/unmet
placement truth. Intents live in chirograph (release ruling D1); picking in
scenotime (D4); emphasis channels open (D3).

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

**A6. The placement seam (numbered late, sequenced first) - CLOSED 2026-08-16.**
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
(`anchored` and `free` since mer3ly `145eb41` retired `frozen` from the
interactive path) is recorded rather than inferred from force configuration; the solver's proposal and the recorded outcome stay
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

**A1. Placement satisfaction state - CLOSED 2026-08-16.**
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
a manual pin hard-pins) with no satisfaction record anywhere, and it already
shares pins to a second device. Until A6, those pins never reach a score, so
there is nothing for a satisfaction field to attach to. Isometry's VTT pins and
the canvas pin/unpin intent remain alternates.
Validation: a score with an unsatisfiable pin produces a scene that carries
the violation; a test asserts the violation is present rather than the pin
silently best-efforted; the record crosses the graphshell wire.
Done when: a remote viewer can distinguish "placed as pinned" from "pin
unmet" without source access.

**A2. Selection clauses: coordination as data — CLOSED 2026-08-23.**
Context: release ruling D1 stands (sceno ships no intent vocabulary; the
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
Forcing consumer: L3's entrance gate is met. Mer3ly shares a scene by URL hash,
and Wave 1 of the [projection receipts plan](2026-08-23_projection_receipts_plan.md)
supplies the genuine two-view ask: its spatial view and two-reading Matrix
contribute named clauses over the same source identities. The proof forced
crossfilter only; union and intersection remain absent until their behavior is
distinctly required.
Validation: brush in view one filters view two; crossfilter resolution
honored (the brushing view is unfiltered by its own clause); serialized round
trip is deterministic; clause removal restores the unfiltered reading.
Done when: brush, filter, and focus are named, serialized citizens rather
than host-only state.

**A3. LOD rungs as declarative conditions - STAGE ONE LANDED 2026-08-19 (via P3b); stage two stays gated.**
Context: `ScoreItem.representation` is a pre-selected rung; the conditions
that select it live in host code, so a remote client cannot re-select on its
own zoom and a static realization cannot state why a rung was chosen.
Gosling ships the missing form: target, measure (screen-space width/height
vs data-space zoomLevel), operation, threshold, and hysteresis padding as
data. The measure split mirrors Mapbox's layout/paint classing: screen-space
conditions are realization-dependent, data-space conditions are
reading-dependent.
Tasks: stage one, conditions as data in cartography's representation
profiles (host-side registry; the P3b card-to-glyph traversal is the named
consumer). Stage two, portable only when a remote consumer needs client-side
re-selection: rung conditions travel beside the score, and a static
realization evaluates them at its declared zoom deterministically.
Forcing consumer: P3b (the recorded remaining half of P3: representation
degrades card to glyph with recency and zoom, focus stays live). Mer3ly does
not force this one: it exports `representation_registry()` to JavaScript for
the live path's own UI, while its portable path hardcodes
`Representation::Glyph`, and its fold/expand is a manual toggle over `visible`
plus an untyped fold channel rather than a measure and threshold.
Validation: same graph and placement inputs, two declared zooms, different
selected rungs, deterministic; a hysteresis test shows a rung boundary does
not flicker under small zoom oscillation.
Stage-one done when: the representation ladder is registry data and one live
host selects from it using declared view facts rather than a hardcoded branch.
Stage-two done when: the ladder travels beside the score because a second host
needs to re-select locally, and that host honors it without porting Mere's
selection code.

**A4. Transition specs between epochs - CLOSED 2026-08-19.**
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
schedule; a static realization is unaffected by transition specs; motion
stays out of arrangement types (the catalog's motion taxonomy holds).
Done when: a projection switch reads as a staged transition, specified as
data, on at least one consumer, with snapping still the default elsewhere.

**A5. Gap proofs adopt named anatomies (gated on a named first proof consumer).**
Context: the catalog's promotion suite is an evidence harness, not a consumer.
It begins when a named product needs one of these proofs. Reusing one
heterogeneous fixture as orrery, matrix, Cartesian chart, hierarchy, and
schematic then tests whether the resulting contract material composes beyond
that first ask. The report's job was to make sure none of that material is
invented fresh.
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
- Every proof: an accessible static realization in the receipt (B1 defines
  the shape). Read GoFish in full before the facet and flow proofs; it is the
  chart-side proof of the catalog's central bet.
Forcing consumer: unassigned. Entrance gate: a named product consumer needs
one proof strongly enough to state its task, source facts, interaction, and
accessible output. Found the suite around that proof. Any portable addition
still needs the catalog's second heterogeneous consumer; the other suite views
do not count as substitutes merely because they share a fixture.
Validation: per the catalog's promotion checklist, unchanged.
Per-proof done when: the contract addition cites the anatomy it lifted, the
first proof that forced it, and the second heterogeneous consumer that proves
it portable. A5 stays open until every A5 proof family is either promoted
through that evidence or ruled unnecessary by a forcing proof.

### Track B: genet (realization receipts)

Genet is the realization layer of the projection stack: the accessible static
form and the drivable interactive form both *target* its surfaces. They do not
live in it. **Corrected 2026-08-16 while opening B1**: genet has no dependency
on scenograph and mere depends on genet, so a converter from `Scene` to a
semantic form cannot sit in genet without inverting the stack. The static
realization lands in mere and renders into genet's lane, which is the same
direction mer3ly already proves by serializing cambium views to static HTML.
Pointer docs in `genet/docs/` are founded when a slice genuinely lands there.

**B1. Accessible static realization - CLOSED 2026-08-20. All legs, including the manual pass.**
Context: no surveyed grammar treats the accessible form as a first-class
realization target; the catalog's W3C citations (WAI complex images, Graphics
ARIA, SVG structure) are the anatomy. Retrofitting accessibility contracts is
the expensive order, so the receipt shape should exist before the promotion
suite starts producing receipts.
Tasks: define the receipt shape (navigable structure, names, descriptions,
values, relations, and a tabular or long-form alternate where the visual
alone is insufficient); realize one existing scene (the P3 spiral score or
the P5 `coastal_map.json` fixture) as a static semantic form through genet's
DOM lane; verify with the AccessKit lane precedent
(accesskit_screen_reader_verification, 2026-06-09) and a genet-probe scenario
asserting the semantic tree (apps self-drive via genet-probe; never synthetic
OS input).
Forcing consumers: Mer3ly's shipping accessibility gap opened the static form,
and Turnstone `648bf19` supplied the definitive headed traversal and routed
interaction close. The promotion suite reuses this receipt shape; it is the
harness that made the need visible, not a substitute consumer. Mer3ly's sandbox
was JavaScript-only, shipping `data-graph-interface` hidden with a runtime-built
node list and a no-script status reading "Graphshell sandbox not initialized",
so the site served accessibility by rendering an entirely separate
authority-derived static index instead. That workaround is what B1 retires.
Validation: a screen-reader traversal of the static projection enumerates
instances and relations with names; a probe scenario asserts structure
deterministically; the same scene still produces its interactive realization.
Done when: "static with navigable semantics" is a checklist item the first
promotion proof can satisfy by following an existing worked example.

**B2. Probe-drivable projections - CLOSED 2026-08-20.**
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
**Landed.** Turnstone's retained Knot toolbar carries the projected
`InstanceId` and advertised intent in DOM data attributes on Save, Resolve,
and Run. The Knot session retains the endpoint's exact `IntentResult` as a
visible receipt carrying the same target and intent. `knot_authoring.scn`
selects Resolve as `.knot-resolve @data-projection-instance=1`, independent of
its label and screen position, then asserts
`Projection intent 1: knot.transclusion.resolve accepted` and the derived
content. A focused unit proof resolves a second fixture at `InstanceId(17)`
through genet-probe. The headed scenario completed with `RESULT ok`. The
boundary stayed narrow: Turnstone carries the host identity, genet-probe uses
its existing generic class-plus-attribute resolver, and chirograph continues
to own `InstanceId`, invocation, and result.

**B3. Livery property-classing check - CLOSED 2026-08-16, recorded.**
Context: Mapbox's layout/paint split is prior art for classing every visual
property by what it invalidates. Livery's TOML property DB likely already
carries an invalidation class per property for its own engine needs.
Tasks: read livery's property DB schema; if classing exists, record the
rhyme in livery's docs and close this item; if it does not, weigh adding the
class with livery's own consumers, not on the report's authority.
Validation: one paragraph of recorded evidence either way.
Done when: the question is answered from the tree, not the report.
**Answered.** Livery's DB carries no invalidation class: 123 property entries
with name, value type, inheritance, initial, grammar, seed values, animation
behaviour, and source, and nothing saying what a change invalidates. But the
rhyme is not missing from genet, it is *borrowed*: `LayoutDamageClass` resolves
None / PaintOnly / Relayout and a color swap is asserted repaint-only, with the
damage computed inside Stylo's `compute_style_difference` from the incumbent's
own property metadata. The adapter's `compute_layout_damage` hook is a stub.
So the classing leaves with the fork at retirement unless Livery's DB grows
somewhere to put it. Recorded in the cutover plan's deferral register as a
compounding item, on that plan's authority and with its retirement stage as the
forcing consumer, which is what this target asked for.

### Track C: cambium (host consumption)

Cambium is where scenes meet users: `cambium-genet-winit-host` is the
single-root host, woodshed then signalman its consumers, and swatches are the
agreed cross-product graph-view contract. Cambium doc updates land in
`genet/components/cambium/docs/` when a slice opens.

**C1. Satisfaction state in host chrome - LANDED 2026-08-16.**
Context: A1's scene-side record is only honest if a user can see it.
Tasks: with A1's consumer, surface pin state in the host's widget chrome
(a pinned badge; an unmet-pin state visibly distinct); keep the vocabulary
plain (pinned, held, unmet), not solver jargon.
Validation: the headed receipt shows a pinned item, a displaced anchored
item returning home, and an unmet pin visibly reported.
Done when: no best-effort placement is presented as satisfied truth in any
cambium-hosted view.

**C2. Transition playback with the host clock (consumer half of A4) - CLOSED
2026-08-19.**
Context: scenotime evaluates transitions purely; a cambium host owns time.
Tasks: the host drives A4's schedule with its frame clock; woodshed's
rehearsal filmstrip is the named first consumer (L5); the canvas projection
switch is the alternate.
Validation: pausing the host clock pauses the transition; identical inputs
replay identically; a consumer that never adopts transitions still snaps.
Done when: one shipping cambium consumer plays a staged epoch transition.

**C3. Backdrop realization (consumer half of expansion lane L2) - CLOSED
2026-08-20.**
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

Closed: **A0**, **A6**, **A1**, **C1**, **B1**, **B2**, **B3**, **C3**, **A2**,
**A3 stage one**, **A4 + C2**, and P3b's local selection-to-renderer proof.
Turnstone `648bf19` closes B1's readable and routed-interaction receipts.

The remaining implementation targets keep their entrance gates. **A3 stage
two** waits for a remote re-selection consumer. **A5** waits for a named first
proof consumer; the promotion suite is founded with that proof.

Non-goals, restated from the governing docs and the report: no intent
vocabulary in sceno (D1 stands); no global nonconvex solver
in scenomise (deterministic solving is the receipt currency); no speculative
adoption of the report's six-layer spec stack (each layer arrives only with
its proof); no new grammar DSL (the score is the spec; the report's
what-a-spec-means table describes meanings the score may adopt, not syntaxes
to build).

## Resolved ruling and open question

**The site's wire as an index - ruled 2026-08-16.** Mark's framing, 2026-08-16: the wire has its
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

**Apps embedded in the site - open.** Mark, 2026-08-16: if he could embed full
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
  from `sceno/src/score.rs`, release rulings D1-D4 from the 0.0.3 plan,
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
  At this point satisfaction reporting was still A1 and crate versions stayed
  at 0.0.3. Later entries close A1. The 2026-08-19 reconciliation opens the
  0.0.4 development line and Score v3; nothing was republished. Mer3ly's own
  seam (its live path still never builds a score) was the next slice.
- 2026-08-16: **A6 mer3ly half written and verified, parked on a push.** The
  seam now exists in code: `PlacementDelta` deserializes the sandbox's own
  scene state (extra fields ignored, so a caller hands over the whole shared
  record), and its `holds()` maps that placement onto `Score.holds`. A manual
  pin is hard in the live path, so it records as `Hold::Pinned`; under
  `anchored` motion the visitor has already said best-effort, so it records as
  `Hold::Anchored`. The class is recorded rather than left to be re-inferred
  from a spring stiffness, which was the point. `portable_projection_holding`
  and `portable_projection_with_placement_json` carry it, with a
  `portable_projection_with_placement` wasm export for the sandbox, and a pin
  naming a node the authority lacks is refused as a broken citation rather
  than solved into a scene that quietly omits it.
  The receipt half matters as much: `consume_portable_projection` now checks
  every hold against the scene **as solved** (not after the trace, which
  deliberately moves Turnstone) and reports `honored_holds`. A forged artifact
  claiming a pin the scene does not satisfy is rejected. That makes a cited
  pin checkable, which is the shelfmark's `expects` property arriving one
  layer down.
  Receipts: mer3ly-repo-graph 14 -> 19 tests, site workspace 49 green,
  including a round trip that pins a node under `Spiral`, the exact family
  that used to discard an authored coordinate in silence.
  The version bump proved itself immediately: `tests/m7_showcase.rs` asserted
  `score.version == 1` and failed loudly the moment the contract moved, which
  is a consumer noticing rather than silently dropping a field.
  **Blocker, and it is only this.** Mer3ly resolves mere by git rev, pinned at
  `8a7ede70`, and the contract half sits in unpushed `3716c601`. A local
  `[patch]` does not substitute for a rev-pinned git dep (it resolved the old
  rev with no warning), so verification ran against temporarily repointed path
  deps, now reverted; no local path is left in a manifest. Landing sequence:
  push mere, bump mer3ly's rev, reapply the parked diff, rebuild the committed
  wasm, then wire the sandbox's share control to the new export. The last two
  are deliberately not done, because a wasm rebuilt against a local path is
  not a thing to commit.
- 2026-08-16: **A6 closed; A1's first slice landed.** A6's mer3ly half shipped
  after the rev bump: `PlacementDelta` maps the sandbox's pins onto
  `Score.holds`, `consume_portable_projection` checks each hold against the
  scene as solved and reports `honored_holds`, and an `export projection`
  control sits beside `share scene` rather than replacing it, because a share
  is a citation and a score plus snapshot has no business in a URL. Verified in
  a browser, not only in tests: pin a node, press export, "1 pin honored"; and
  driving the export directly returns score version 2, a hold recording
  `Pinned` at (-137.5, 88.25), and that node placed at exactly (-137.5, 88.25)
  under `Spiral`. The Wasm ceiling was raised deliberately, with all three
  measurements recorded beside it (858,444 baseline / 932,204 stripped /
  958,016 full) and the finding that no variant fit the old bound.
  A1 then opened on a gap A6 itself created. A hold naming a source the score
  never placed was dropped in silence, which is the same failure as moving a
  pin and saying nothing; a probe test confirmed it before anything was built.
  `Scene.unmet_holds` now carries those violations as full `HeldPlacement`
  records, so a viewer without the score can say what was asked and where, and
  `SceneTables.unmet_holds` carries them through `from_dense` onto the wire.
  Encourage-class holds are excluded on purpose: an anchored home that goes
  unplaced is best effort working as designed.
  Receipts: sceno 21, scenomise 24, scenotime 19, with chirograph (21) and
  mere-cartography (25) unaffected. Two of the new tests serialize the snapshot
  and read it back, because A1's done-condition names a *remote* viewer, and
  one strips the field to prove an older wire still loads.
  Still open in A1: the only unsatisfiable case today is a hold on an absent
  source. Displacement by a consumer's own physics or a viewport fit is not yet
  reported, and mer3ly's `honored_holds` remains a site-local receipt rather
  than a promoted one. C1 (satisfaction in host chrome) now has something real
  to render.
- 2026-08-16: **B3 closed, and the answer was better than either branch the
  target anticipated.** The question was whether Livery's property DB classes
  properties by what they invalidate. It does not: 123 entries, eight keys,
  none of them an invalidation class, and Livery's own invalidation machinery
  is selector-scoped, a different axis. The target then expected either "close
  it" or "weigh adding it". Neither fit, because genet already has the
  layout/paint rhyme working: `LayoutDamageClass` resolves None / PaintOnly /
  Relayout, `RestyleOutcome::needs_relayout` reads `RestyleDamage::RELAYOUT`,
  and a test asserts a color swap is repaint-only. It is borrowed, not owned.
  Stylo computes the damage in `compute_style_difference` from its own
  per-property metadata, and genet's `compute_layout_damage` hook is a stub
  returning default. A first read of that stub suggested the classing was
  unimplemented; checking where damage actually comes from corrected it.
  So the finding is a dependency, not a gap: every incremental-layout receipt
  that skips layout for a paint-only change is currently a receipt for Stylo's
  property table, and the classing leaves with the fork unless the DB grows a
  home for it. Recorded in genet's cutover-plan deferral register as a
  compounding item with the retirement stage as its forcing consumer, on that
  plan's authority rather than the report's, which is exactly the bar this
  target set. Mapbox's layout/paint split is named there as the shape to
  borrow when it is built.
- 2026-08-16: **B1's first slice landed: the receipt shape exists, with two
  corrections it forced.** `graphshell_client::frozen` turns a `Scene` into
  navigable semantics: a document name, a generated summary, one entry per
  visible instance with a coarse role (symbol / object / live content, mapped
  down from `Representation` because a reader cares about kind, not rung),
  relations resolved to names at both ends, and a `rows()` tabular alternate.
  It produces a *structure*, not markup, so a host renders it into a genet DOM
  tree, an AccessKit node tree, or an HTML table, and the receipt is assertable
  in a test with no browser. That also keeps a DOM engine out of the client.
  **Correction one: B1's home is mere, not genet.** Genet has no dependency on
  scenograph and mere depends on genet, so a `Scene`-to-semantics converter
  cannot live in genet without inverting the stack. Track B's preamble is
  corrected: the realizations *target* genet's surfaces rather than living
  there, which is the direction mer3ly already proves.
  **Correction two: a scene carries no names.** `ProjectedItem` identifies
  itself with a `SourceRef`, an address rather than a label, so an accessible
  form cannot be derived from a scene alone. The names already exist one layer
  out, on `chirograph::PresentationSemantics.label`, so `freeze` takes a
  supplied lookup and falls back to the source id, and `unnamed` counts the
  fallbacks so a receipt can state how legible the scene actually was. This is
  the report's claim landing concretely: no surveyed grammar treats the
  accessible form as first-class, and the contract shows it.
  A1's violations are carried into the frozen form deliberately. A sighted
  reader can see a pin sitting in the wrong place; without this the frozen
  realization would be the one form that cannot say so.
  Receipts: six tests, run against the real P5 `coastal_map.json` fixture
  solved through `scenomise` (added test-only) rather than a hand-built
  stand-in, covering enumeration, fallback naming, summary-matches-listing,
  invisible-item omission, relation naming, unmet placements reaching the
  tabular alternate, and determinism. graphshell-client 13 -> 19.
  Still open in B1: rendering the structure into an actual DOM tree, the
  AccessKit screen-reader traversal, and the genet-probe scenario asserting the
  semantic tree. The shape they all need now exists and is stable.
- 2026-08-16: **B1 carried to its buildable edge; A1 closed; A2 half-open.**
  B1 gained a renderer: `FrozenScene::to_html` emits the graphics-document
  anatomy with WAI's long-form alternate as a real table, and the tests parse it
  back through genet-scripted-dom rather than string-matching, so a malformed
  tag fails as a missing tree node. One of them is a hostile name, because a
  projection whose accessible form can be broken by a source called `<script>`
  is not accessible and is a hole besides. It also gained an AccessKit
  projection into uxtree, behind an off-by-default feature so a client that only
  reads scenes carries no tree builder, and probe-resolution proof that every
  instance is reachable by carried id and by announced name.
  What B1 still lacks needs a host, not effort: no OS screen reader has been
  driven, and no probe scenario can run until something renders this realization
  inside a frame pump. The precedent this target named is a manual checklist
  written for meerkat, an app that no longer exists, so it could not be followed
  literally.
  A1 closed by merging its two remaining questions into one field.
  `Scene.honored_holds` records the positive half bound to instances, which both
  promotes the receipt mer3ly was recomputing and closes a real footgun: plain
  `relax` used to drag an ensure-class placement away in silence and now cannot,
  whatever entry point a caller reaches for. Anchored holds stay out, because
  recording a suggestion as honored would invite a later pass to treat it as
  binding.
  A2's serialization half landed as `chirograph::Selection`, carrying the
  producing view and its targets, with the resolution strategy deliberately
  absent until two coordinated views exist. The `source` field is mandatory now
  rather than later because crossfilter is defined as "every brush but this
  view's own", which is unanswerable without it, and a field added later would
  be missing from every link already in circulation. The shelfmark note's
  reserved `selection` section is now defined against that record.
- 2026-08-16: **C1 investigated and blocked, on the same wall B1 hit.** A1 has
  landed, so there is finally something real to render: `Scene.unmet_holds`
  crosses the wire, `Scene.honored_holds` names the satisfied pins, and the
  frozen realization already lists violations in its tabular alternate and
  announces them in its AccessKit tree. The target says surface the same
  distinction in cambium host chrome. Cambium cannot: it has no `sceno`
  dependency, so it cannot see a `Scene` at all, and nothing in mere consumes
  cambium either. Verified from the tree in both directions, not assumed.
  This is Track C meeting the correction Track B already took. A host renders
  *into* cambium's surfaces; the code that knows what a hold is lives mere-side.
  So C1's home is a mere-side host built on cambium, and no such host exists
  today, which is the same missing piece that blocks the driven probe scenario.
  One consumer could take it now without a new host: mer3ly already reports
  "1 pin honored" on its export control, and could report the unmet count
  beside it rather than only failing the whole artifact, since the snapshot now
  carries the violations. That is C1's substance in a web page rather than
  native chrome, and it costs another rev bump, wasm rebuild, and public deploy,
  so it is Mark's call rather than a quiet extension of this target.
  Not built: a speculative cambium surface with no consumer. That is exactly
  the work the five gated targets are being held back from, and C1 does not get
  an exemption for being adjacent to something that just landed.
- 2026-08-16: **C1 landed, after a wrong "blocked" call that Mark caught.**
  The earlier entry claimed C1 could not be built because nothing in mere
  consumes cambium. That was false. `ports/graphshell/web` depends on cambium,
  sceno, and graphshell together, and `ports/graphshell/src/web.rs` already
  renders a mounted `SceneSnapshot` in `remote_scene`. The claim came from a
  grep with relative paths run after the shell cwd had been reset, so it
  searched a directory with no `ports/` in it and returned nothing; an empty
  result was read as an answer. That is the wrong-cwd trap, hit one turn after
  citing it about a Cargo patch table.
  Chasing the real host then exposed a second, worse gap: `SceneTables` carried
  `unmet_holds` but not `honored_holds`, because the honored half was added to
  `Scene` after the unmet half had already been wired through `from_dense`. So
  A1's done-condition was only half met on the wire. A remote viewer could see
  that a pin failed but not that a pin succeeded, leaving every unremarked item
  ambiguous between unpinned and pinned-and-honored. Both halves now cross, with
  one test asserting them together rather than separately, since testing each
  alone is what let the gap open.
  C1 itself: `graphshell_client::frozen::Satisfaction` reads both halves off a
  snapshot and answers the two questions a host has, whether a given instance is
  holding an authored position and what the chrome line should say. It lives in
  the client rather than the host because `web.rs` is
  `#![cfg(target_arch = "wasm32")]` and no native test can reach it; a summary
  nobody can test is one that quietly goes wrong. The host draws a held edge
  outside a pinned card, so an item sitting where a person put it no longer
  looks identical to one the arrangement happened to place there, and the chrome
  carries the line. An unpinned scene says nothing at all, because zero of zero
  is noise.
  Receipts: three native tests on the summary, one wire test on the pair, and
  `cargo check -p graphshell-web --target wasm32-unknown-unknown` for the host,
  which is the only check that covers a cfg-gated file at all.
- 2026-08-19: **protocol and catalogs reconciled.** The Scenograph family
  moves to the unpublished 0.0.4 development line; Score v3 renames portable
  regular-cell Board to Grid, while the local categorical layout is Columns.
  Chirograph 0.0.2 renames the retained wire artifact to
  `ProjectionCaptureV1`. Snapshot validation now rejects an honored-pin claim
  whose instance is absent, names another source, is not ensure-class, or is
  not at the claimed position. The projection grammar governs primitives;
  the content catalog depends on it and collects complete scene recipes.
  Rosette is recorded as landed through Knot over poem and lyric datasets.
- 2026-08-19: **B1 probe scenario ran for real; the no-host premise died the
  way C1 death did.** Mark asked whether graphshell was built on cambium;
  re-checking from the tree found what the wrong-cwd grep had hidden, and the
  right host was turnstone: it implements genet-probe Automatable with real
  ProbeSurfaces, carries a .scn scenario lane whose assert-a11y reads a
  stitched AccessKit tree, and already depends on graphshell-client, sceno,
  and cambium.
  Landed in turnstone (49e179e): disclose_scene extracted from the endpoint
  snapshot so the frozen form freezes exactly what a remote peer is served,
  one recipe rather than two that drift; a Frozen Projection pane in cambium
  chrome with instances by name and carried data-source-id, relations named
  at both ends, the WAI summary, unmet placements, and C1 satisfaction line;
  and an a11y arm built by FrozenScene::to_ux_tree itself, so assert-a11y
  reads the realization actual tree rather than a summary of one.
  Receipt: scenarios/frozen_projection.scn, RESULT ok, self-driven, offline
  mere:// nodes, captures in Code/testing/turnstone/frozen_projection. The
  capture shows the pane beside the live canvas: 14 items, 17 relationships,
  harbor-notes and beacon-notes by name.
  All three automated verification legs are now done: renderer proven by
  parsing, AccessKit tree driven through a real host, probe scenario driven.
  At this checkpoint the manual OS screen-reader pass was still open. Turnstone
  `648bf19` later supplied the definitive routed receipt and supersedes this
  historical remainder.
- 2026-08-19: **A3 stage one landed; P3b selection is declarative.**
  `mere.graph-representation-registry/v2` puts ordered representation rungs
  on each class profile. A rung names its selected `sceno::Representation`
  and a conjunction of measure, operation, threshold, and hysteresis
  conditions. The initial policy is concrete: focus selects `LivePane`; a
  measured, recent item at zoom 1 or closer selects `Card`; `Glyph` is the
  fallback. Cartography resolves graph classes in registry order, normalizes
  durable visit recency, and consults the prior score only for hysteresis.
  Graphshell's live web host now supplies its camera zoom, measured node
  extents, focus, and prior score, and refreshes the score after local view
  commands. `sceno` is unchanged: it still records only the chosen rung.
  Deterministic receipts prove identical placement at two zooms with different
  rungs, focus overriding distance, and a card holding through the declared
  0.1 hysteresis band. This closes A3's host-side grammar, not P3b's visible
  rendering: the canvas still stores the selected score without painting
  distinct glyph/card/live-pane realizations from it. Portable conditions stay
  gated on a remote client that needs to re-select.
- 2026-08-19: **P3b's local renderer half landed.** Canvas rebinds the score's
  opaque `mere.graph` sources to node keys and its real frame assigns distinct
  `Glyph`, `Card`, and `LivePane` face classes. The glyph hides the caption,
  the card retains the established labelled node, and the live-capable rung
  receives a frame plus caption emphasis. The measured footprint still owns
  all geometry, and the live-capable treatment does not claim embedded browser
  content. Tests drive score → Canvas frame → DOM class and computed style,
  and prove a whole-graph swap clears the rebound score state. This closes the
  local forcing consumer for A3 stage one; A3 stage two remains gated on a
  remote client that must perform its own re-selection.
- 2026-08-19: **A4 + C2 landed through Graphshell's Canvas arrangement
  switch.** `scenotime::TransitionSpec` declares duration, easing, class
  windows, and stable stagger ratios over a validated `SceneDiff`.
  `TransitionSchedule::sample_at` is a pure function of that data and elapsed
  host time. The default stages exits, updates, and entrances without adding
  motion to `Arrangement` or changing direct diff application.
  Graphshell builds the arrangement change as a real scene diff, advances it
  from browser frame timestamps, and feeds sampled positions through Canvas's
  preview buffer. Completion returns to the ordinary strategy apply, which is
  still the authority that seeds physics. The host clock explicitly excludes
  paused intervals; identical timestamp sequences replay identically.
  Receipts: 27 scenotime tests, 179 Canvas tests, the focused Graphshell host
  clock test, and a wasm32 build. A headed browser drive reported
  `Arrangement changing to grid.default` before
  `Arrangement set to grid.default`; the start and final canvas captures were
  distinct. Consumers that do not construct a schedule continue to snap.
- 2026-08-19: **A3 stage one arrived under P3b's flag, landed by a sibling
  session and verified here against the tree.** The P3b check Mark ordered
  found the gate had fallen the same day: cartography's v2 representation
  profiles now declare `RepresentationCondition { measure, operation,
  threshold, hysteresis }`, which is Gosling's anatomy verbatim, serde-derived
  so the conditions are data, with the measure split A3 asked for
  (ScreenWidth/ScreenHeight are realization-space; ZoomLevel, Recency, Focused
  are view facts). Hysteresis relaxes the threshold only for the rung already
  selected, so a card holds to zoom 0.9 while a glyph still needs 1.0, and
  `profile_ladder_selects_from_data_and_retains_through_hysteresis` is the
  no-flicker receipt A3's validation named. The renderer half consumes the
  selected rung in the real canvas frame. What was the hint: the frozen-pane
  captures showing plain nodes flip object to symbol between runs, which was
  rung selection moving with recency and focus.
  Stage two, portable rung conditions traveling beside the score, remains
  gated exactly as written: the P3b entry itself says remote re-selection
  waits on a real remote consumer, and nothing about stage one changes that.
- 2026-08-20: **C3 and L2 landed through Isometry and Woodshed.** The two
  consumers forced a separate backdrop table rather than an item kind or a
  generalized Region. Each backdrop carries a source reference, coordinate
  space, transform, footprint, open appearance kind, visibility, and collision
  participation. Table order is back-to-front and the whole table paints before
  graph content. Pointer transparency is structural: selecting part of a map
  remains an ordinary item over the environment, so intent routing keeps one
  `InstanceId` path.
  Isometry's authored board now supplies `isometry:tile-board` behind its
  selectable tile items. Woodshed's Stage supplies `woodshed:stage-floor`
  around its arranged cards. Both cross `SceneSnapshot`; stable backdrop slots
  also cross `SceneDiff`. Graphshell's remote canvas derives transformed
  backdrop bounds and deterministic fallback paint from the snapshot before it
  draws items. Scenomise treats collidable backdrop footprints as static
  placement obstacles while visibility remains independent.
  Receipts: 22 sceno tests, 31 scenomise tests, 30 scenotime tests, 116
  Graphshell tests, Isometry's views and Graphshell endpoint tests, Woodshed's
  `stage_backdrop` wire test, and the Graphshell wasm32 build.
- 2026-08-20: **B2 landed through Turnstone Knot authoring.** Save, Resolve,
  and Run expose the projection instance and intent in the retained DOM. The
  session records the endpoint's typed intent result as visible scene text.
  The shared genet-probe scenario selects Resolve by `InstanceId(1)`, asserts
  the accepted result, and verifies the resulting transclusion before running
  the document. Receipts: the focused `InstanceId(17)` resolver unit test and
  the headed `knot_authoring.scn` run, `RESULT ok`.
- 2026-08-20: **B1 closed. Mark ran the manual screen-reader pass, and it
  earned its place twice before it passed.** Preflight found the OS bridge
  had never existed: accesskit was in-process only, so assert-a11y read a
  complete tree while Narrator saw a bare window; turnstone gained
  shell/a11y_bridge.rs (accesskit_winit installed before first show, shared
  slot for cross-thread activation, thirty-frame cadence, ActionRequests
  dropped honestly rather than routed nowhere). Attempt one then dead-ended
  at the omnibar: a live UIA dump showed every projected node boundless
  (rect -Infinity), which Narrator treats as off-screen and stops at, while
  the in-process integrity test written on the spot passed - a structurally
  sound tree that no reader could walk, the exact gap between iterating a
  node list and being a UIA client with geometry. Window-extent bounds fixed
  it. Attempt two: the full walk - window, chrome, canvas group, the
  Disclosed projection document with its summary spoken, twelve items by
  name, all twenty-seven relationships to the last entry, in the operator's
  words "made it to 27 of 27". Receipt at turnstone
  design_docs/2026-08-20_screen_reader_pass_receipt.md (05a2ec8), replacing
  the meerkat-era checklist as the worked example. Recorded follow-ups, none
  speculative: route ActionRequests when a consumer asks for operable rather
  than readable, per-node rects from the surface plan, and a label for the
  frisket root group that announces as blank.
- 2026-08-20: **The routed half landed: a screen reader's Enter reaches the
  app.** Mark asked for it, which is the forcing consumer B1's follow-up was
  written to wait for. Turnstone e117f27: a route table built beside every
  pushed tree (ids are one-way path hashes, so routing cannot be parsed back
  out of the tree), a frozen-projection instance selecting its member in the
  graph, the omnibar opening, both through the same update spine a keypress
  uses; unrouted requests land as interaction-missed a11y-action, the pointer
  miss's exact vocabulary. Eight tests. Linking them exposed two real defects:
  the livery cutover had shipped a second AccessKit platform stack beside the
  bridge's (two UIA providers in one process, deduped), and the grown graph
  crossed the preview MSVC linker's PDB limit. Both fixed and recorded in the
  turnstone receipt (`648bf19`). **Reconciled 2026-08-23:** that clean Turnstone
  state is the definitive routed screen-reader receipt; B1 has no remaining
  manual leg.
- 2026-08-23: **A2 resolution closed through the first projection-receipts
  wave.** `chirograph::CoordinatedSelection` carries named focus/filter/brush
  clauses and explicit `single` or `crossfilter` resolution. The mer3ly
  spatial and Matrix views prove both clauses in a headed browser: each view
  applies the foreign clause and excludes its own; deterministic Shelfmark
  serialization restores both clauses; removing the Matrix clause restores all
  18 spatial actors. Gazette's Ledger independently replays the same record.
- 2026-08-25: **FT7 closed.** The FT1 Matrix preserves its exact
  `InstanceId -> SourceRef` mapping, including repeated source instances,
  through `LocalCarrier`, an admitted Graphshell `MemoryTransport` session
  using Notochord/personae policy to a source-free viewer, and the
  `FrozenScene` semantic-table realization. The Mere platform implementation
  is `302bbe72d7597b7573e14199ce926bd3b03eea7f`; the Mer3ly consumer is
  `4c42847272489c41a20dc515884e83f3b413059a`.
- 2026-08-25: **FT8 closed.** Retinue Signalman's Network face derives its
  Sprigging marks and Cambium retained semantic targets from one
  `GraphCanvasSwatch`. Its focused receipt proves those marks and ordinary DOM
  controls share one focus order, pointer-capture route, AccessKit tree,
  genet-probe address space, and `DesktopState` action model. The full locked,
  offline Signalman suite passes 50 tests at Retinue `8cea8f9`. The receipt
  reused Cambium's existing `graph_canvas`; it did not force a generic
  scene-hosting abstraction or keyboard-navigable Cambium Matrix. A3 stage two
  and A5 remain gated by their own consumer rules.
- 2026-08-29: **governing documents reconciled against landed code and the
  promotion rule** at Mere `caf19a014766ae08e3231d9963d5edfaec945dd7`. The
  catalog now describes Score v4's actual arrangement surface, distinguishes
  C3's closed minimum backdrop contract from richer raster and scalar field
  work, and states that the promotion suite is an evidence harness rather than
  a forcing consumer. A5 now waits for a named first proof consumer and still
  requires a second heterogeneous consumer for every portable addition. The
  shelfmark/index ruling is labeled resolved; the embedded-app question
  remains open.
- 2026-08-31: **Graphshell Projection Editor component and first authoring
  loop landed through Mere `77369f6f0c03b301c398ad65107114080c4ba630`.** The
  host-neutral `ports/graphshell/src/projection_editor.rs` now models source
  and domain binding, reading, encoding, arrangement, interaction,
  appearance/realization, and provenance as an editable definition with
  field-level validation, panel taxonomy, reducer actions, deterministic JSON,
  and a sink-only save boundary. Its seven tools are `workbench::Tile`s in an
  open Graphshell content lane, so selection and typed tearout use the shared
  reducer without granting graph or endpoint authority. A focused standalone
  cross-repo harness passed eight tests, including Platen projection, editor
  activation, tearout custody, validation, provenance, serialization, and sink
  refusal. The standalone web manifest now restates its inherited immutable
  Genet patches, and the host `x86_64-pc-windows-msvc` cargo check passes
  offline with the immutable G1 Genet revision. G1's native-only `fontsan`
  policy closes the prior C++/WASI sysroot blocker for the wasm dependency
  cone. The standalone wasm build now passes with an isolated target and the
  matching `wasm-bindgen 0.2.126` package is generated under the ignored web
  package directory. The headed Graphshell browser receipt is now green:
  localhost reported title `GRAPHSHELL H3 READY` and `ready=true`; the editor
  opened the source panel, authored `local-workbench-receipt`,
  `woodshed.practice`, `fixture.projection/workbench`, `practice_title`,
  `tempo`/`difficulty`, `grid.default`/vertical/24, `canvas.points`, title
  `Workbench projection receipt`, and provenance
  `Projection Editor receipt` / `workbench-w4` / `Saved and reloaded in the Graphshell browser receipt`. The deterministic preview was `Workbench projection receipt · read nodes by practice_title · grid.default · x=tempo y=difficulty · canvas.points`. Save reported `saveCount=1`,
  `validation=valid`, `errors=0`, and `Saved · workbench-w4 · 1 save(s)`;
  after mutation, reload restored every authored field and reported
  `Reloaded · graphshell-reference · workbench-w4`. The bound stripped artifact
  is 34,679,440 bytes with SHA256
  `A6A43EA0D1FB510E9EEF897B6C9232BB66F5D1F5927DF54DADE3463F55D3DB85`.
  This closes the browser authoring receipt, while broader headed verification
  and A5 remain open. Woodshed is Workbench's second
  heterogeneous consumer, but it is not itself A5's named portable projection
  proof; A5 still needs a named proof consumer and its per-proof evidence. The
  component follows the component seam in
  `genet/design_docs/2026-08-31_workbench_component_plan.md`.
