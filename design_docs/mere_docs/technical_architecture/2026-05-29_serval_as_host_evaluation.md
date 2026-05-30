# Serval-as-host: evaluation, and what it does to the spine

**Date**: 2026-05-29
**Status**: Decision brief. Records the call on whether Mere's chrome should be
rendered by serval (one engine) rather than Masonry, the evidence, and the
worked consequences for the two subsystems it touches hardest (the orrery and
platen). Companion to the serval-side implementation doc
[`serval/docs/2026-05-27_serval_as_host_xilem_serval_plan.md`](../../../../serval/docs/2026-05-27_serval_as_host_xilem_serval_plan.md),
which owns the `xilem_serval` backend mechanics; this doc owns the Mere-side
architecture decision and adoption-plan relevance.
**Related**: [composition spine](2026-05-21_mere_composition_spine.md),
[between-tiles layout seam](2026-05-26_between_tiles_layout_seam.md),
[component fit-map](2026-05-26_component_fit_map.md),
[adoption roadmap](../implementation_strategy/2026-05-27_adoption_roadmap.md),
[understory orrery brief](../research/2026-05-27_understory_orrery_graduation_brief.md).

---

## 1. The decision

Serval-as-host (chrome and content rendered by one engine, chrome authored in
Rust through `xilem_serval`, painted through netrender) is **the right end-state
for Mere**, adopted as a *deliberate, gated flip*, not an immediate migration and
not a maybe. The current Xilem + Masonry setup stays the working host until the
flip gate clears.

The gate is narrow and already most-closed: serval's interactive completeness
reaching Masonry's current bar. As of the serval git log that means **IME plus
form-control breadth plus a decision on the orrery element**, not a from-scratch
engine build (form controls, keyboard, focus, capture-phase dispatch, and
caret-aware text editing already landed in `xilem-serval`).

The one near-term instruction that follows immediately: **stop deepening
Masonry-specific investment.** Treat the Masonry host as scaffolding and keep
every new host-coupled decision retargetable, so the flip is a rebuild of a thin
layer rather than an excavation.

## 2. The three architectures

From the serval doc, "use serval for the GUI" is three different things:

1. **serval-as-texture-in-a-host** (today): Xilem owns the chrome, serval renders
   content to a texture the host composites. The working setup.
2. **host-framework + serval-rendered chrome**: deliberately *excluded*. Runs two
   layout engines, two event models, two a11y trees, and buys only web-authoring
   ergonomics. The worst seat.
3. **serval-as-the-host**: serval owns window, layout, paint, input, script, and
   accessibility; chrome and content are both documents. This is the only
   architecture where "chrome as CSS" is coherent, because there is one engine.

This decision is about (3). Architecture (2) is never the path; whenever the
stack wobbles toward "render chrome through serval on top of Masonry," the answer
is "go to (3) instead."

## 3. State of the prototype (why this is a decision, not a question)

The scariest objection to (3) was "you have to build a reactive UI framework."
That risk is empirically retired. `xilem_serval` is a third `xilem_core` backend
(beside Masonry and `xilem_web`), `xilem_web` retargeted at serval's DOM, and the
full loop (diff → serval DOM → layout → paint → netrender → present, input via
serval hit-test + faithful message dispatch) is validated on screen
(`pelt-live-counter`, 2026-05-28).

The serval-as-host doc says "Stage 3 open." The serval git log already shows Stage
3 work landed since: keyboard + focus (`09f2bf1a720`), form controls
(`61135dec2b5`), capture-phase dispatch (`abde0a9ec1c`), caret-aware text editing
(`4ceac562e0c`), plus heavy script-runtime DOM/Element/TreeWalker progress. The
gaps the doc named as "the genuine engine-completeness cost" are being closed in
real time. So the honest framing is "this works and is advancing fast," which is
why the decision is *when to flip*, not *whether it can*.

Two seams the rest of Mere depends on are confirmed present:
- **netrender has `compose_external_texture` / `ExternalTextureComposite`**, so the
  scrying content tile re-homes onto netrender's external-texture path rather than
  Masonry's external layer. The GPU-interop core built in the scrying session is
  host-agnostic; only the compositor seam moves, and the destination exists.
- serval already depends on the `crates/xilem` fork (via pelt-viewer), so the
  dependency direction `xilem_serval` needs is established.

## 4. Pros and cons (architecture 3 vs the current architecture 1)

**Pros, and they are Mere-specific.**

1. **A browser is the one app class where two UI engines is genuinely wasteful.**
   Mere must ship a complete web engine regardless. Running Masonry *as well* for
   chrome pays for two layout engines (Morphorm/Masonry-flex + taffy), two
   hit-test systems, two focus models, two a11y trees, two text stacks. A note app
   or IDE should pick the lean toolkit; a browser should collapse onto the engine
   it cannot avoid shipping. This argument is structural, not aesthetic, and it
   does not generalize to non-browsers.
2. **"Chrome as CSS" becomes coherent.** Chrome and content are the same kind of
   object: register-theme becomes real CSS, devtools inspect chrome and content
   alike, one text stack, the printing-press metaphor goes literal (verso realizes
   actual DOM).
3. **Accessibility improves, and it reinforces the R0 a11y contract.** DOM →
   AccessKit from a semantic tree beats synthesizing it from a widget tree.
   Architecture 1 has to merge two AccessKit trees; architecture 3 has one. The
   `inker::a11y::A11yCapability` declaration adopted in R0 slots straight in:
   engines declare capability, serval-as-host emits the tree.
4. **The reactive layer is reuse, and proven** (section 3).
5. **One event/input/focus model.** Architecture 1 forced the reconciliation the
   understory brief wrestled with and forced scrying to forward input from Masonry
   into the WebView. Architecture 3 has one hit-test plus dispatch, already wired.

**Cons, stated honestly.**

1. **IME.** The one interactive gap not yet in the `xilem-serval` commits, and the
   same gap deferred on the scrying side. Mere needs text entry across chrome.
   Long-run this still favors (3): serval needs IME for content regardless, so it
   is paid once instead of twice. Near-term, Masonry has it and serval does not.
2. **The graph canvas is not a document.** Mere's defining surface is a free-form,
   physics-driven, infinite scene. Under (3) it is a custom-painted element the
   engine hosts. So the "everything is a document" story has its biggest exception
   at the center of the product. Section 6 argues this is a feature, not a wound,
   but it is real and worth naming.
3. **Chrome-update performance is unmeasured.** A full cascade + relayout per
   chrome update is heavier than targeted widget mutation. `IncrementalLayout` is
   the mitigation and is nascent. Section 8 turns this into a concrete gate.
4. **Coupling to two churning substrates** (`xilem_core` API + serval's DOM API),
   with serval itself under heavy active change. The fork pace is ours, but it is
   maintenance surface.
5. **Transition cost.** The clean move (per the zero-user-prototype jump-ship
   bias) is rebuilding the chrome in `xilem_serval` rather than running both paths,
   which is still rebuilding the chrome.

## 5. Relevance to the adoption plan

The portable core of the plan is unaffected, by design. kernel, forme, inker's
engine contracts, mere-domain, eidetic, node-lineage, and the aether physics are
host-agnostic. The "pulled not wired" discipline already hedged this: we have not
deep-wired Masonry specifics. What shifts:

- **platen is the most affected piece** (section 7).
- **R0 a11y is reinforced** (section 4, pro 3); R0 temporal-integrity is unchanged.
- **R1 orrery work is validated as host-agnostic.** The aether hit-test/cull
  surface (`3c12827`) and the future force-field/physics work are host-agnostic;
  only the canvas *host-binding* (Masonry widget vs serval element) is
  architecture-dependent. So the paused R1 fork resolves: the aether/live-layout
  work is safe to continue, the app-wiring stays thin and retargetable.
- **scrying re-homes cleanly** onto `netrender::compose_external_texture` (section
  3), which the counter demo already exercises.
- **verso** realizes into serval DOM instead of a Masonry scene; the TileId
  reshaping survives at the model level.

The single actionable change is the section-1 instruction: stop deepening
Masonry-specific investment; keep new host-coupling retargetable.

## 6. The orrery under serval-as-host (more than a document)

The orrery is **not** an opaque `<canvas>`. Because serval is ours, it becomes a
custom-layout element composing three first-class layers:

1. **Scene paint underlay (PaintCmds).** Edges, culled-node glyphs, selection
   halos, effects. The element contributes a `PaintCmd` sublist to the serval
   paint list: `PushTransform(camera)`, then `DrawPath`/`DrawStroke` per edge,
   `DrawRect` + glyph runs for distant nodes, `PushLayer` for effects. This is the
   existing graph-canvas scene-packet rendering expressed in netrender's
   engine-agnostic `paint_list_api` vocabulary (`DrawPath` is a filled/stroked
   Bezier, already in the enum). It scales because it is a flat command list, not
   DOM, and it renders identically whether driven from a Masonry widget or a serval
   element.
2. **Physics-positioned DOM children (real documents).** The visible nodes
   (selected by `aether::Simulation::cull_aabb`) materialize as real serval DOM
   subtrees: cascade-styled, hit-tested by serval's `FragmentQuery`, emitted to
   AccessKit. Their *position* comes from aether, not CSS flow: each is
   `position: absolute` with a per-frame `transform: translate(x, y)` from the sim.
   A node can therefore hold a label, a live document, or a WebView (via
   `ExternalTextureItem`), styled and accessible, while rapier arranges them. In
   architecture 1 a node is painted pixels with no tree presence; here a node *is*
   a document composed into a physics scene. That is the spatial-browser thesis
   made literal, and it is strictly more capable.
3. **The camera** is a single `PushTransform(TransformSpec)` over both layers, so
   pan/zoom moves scene and DOM children together. `understory_view2d`
   steal-the-shape governs the camera math; the navigation defaults (wheel=pan,
   ctrl+wheel=zoom, inertia, infinite canvas) live in the element's input handling.

**The two-hit-test worry resolves.** Node-*content* hit-testing is serval's (nodes
are DOM fragments); scene-*geometry* hit-testing (empty space, edge picking,
marquee) is aether's `QueryPipeline`. They are complementary, not duplicative,
with a clean boundary: a pointer landing on a materialized node dispatches through
serval; otherwise it is a scene-geometry query.

**Fidelity to the three goals.** petgraph stays the truth; the DOM children and the
scene are a projection of it (the R0 shared-projection invariant applies). rapier
composition is unchanged and host-agnostic. The canvas feel is the camera transform
plus the element's own input. All three survive, and the orrery gains live-content
nodes it could not have cheaply before.

**Virtualization** rides on `cull_aabb`: materialize DOM for visible nodes, demote
off-screen ones to PaintCmd glyphs in the underlay, preserve focus/state across
demotion.

## 7. The platen retarget

**Morphorm is obviated.** serval's layout engine is taffy with flexbox + grid +
block + float (`stylo_taffy`, `serval-layout/Cargo.toml`). Tiling panes map to
nested flex containers: a split is a flex row/col with children plus a draggable
divider adjusting flex-basis; a tab group is a strip plus a content slot. VS Code
is the existence proof that resizable tiling panes work in flex + DOM. So taffy
subsumes Morphorm's constraint-solving, and dropping it removes a layout engine
plus this session's Masonry-coupled `LaidOutPlan` to `place()` seam.

Honest caveat: flex gives proportional sizing and min/max, which is most of what
Morphorm did. The tiling *semantics* (the split-tree model, drag-to-resize,
drag-to-rearrange, tab grouping, serialization) are not in CSS and stay platen's
job regardless of engine. The Morphorm *dependency* goes; the tiling *logic* was
always platen's.

**platen survives, reshaped:**
- **Role unchanged**: own the tiling model + interaction + serialization, compose
  forme's tile-tree and canvas swatches, the press-on-paper layer between forme and
  verso. The forme TileId reshaping from the verso work carries over.
- **Output changes**: from `LaidOutPlan` (positions a Masonry host consumes) to
  serval DOM emitted via `xilem_serval` views. platen becomes a consumer of the
  authoring layer, diffing the forme tile-tree into a DOM subtree (flex containers,
  tile content-roots, dividers) and handling drag by updating flex sizes.
- **Within-tile boundary re-homes to serval**: "Masonry owns within-tile content"
  becomes "the tile content is a serval content-root," a separate document
  authority per the serval doc's separate-roots discipline. That root is either a
  serval-rendered document (an inker/nematic engine's output as DOM) or an
  `ExternalTextureItem` (texture_key + `content_generation`) for a WebView/scrying
  tile. `content_generation` is the frame-arrival hint that ties to the redraw
  straggler from the scrying session.
- **The canvas swatch** is the orrery element (section 6) placed as a tile/region.

**One sub-choice inside platen**: docked tiles want flex (taffy sizes them, platen
owns the split tree and drag), but floating/sticky-note tiles (the tear-out modes
in the multi-window plan) want absolute positioning like the orrery. So platen is
likely a hybrid: flex for the docked tree, absolute for floaters, which maps onto
the existing tile modes rather than fighting them.

**Net**: platen gets simpler (no Morphorm, no separate solver, no place-seam) but
serval-coupled (emits DOM via `xilem_serval`; within-tile content is serval roots
or external textures). The arch-1-specific part that retargets is this session's
Morphorm layout; the tiling model and interaction survive intact, and the retarget
cost is moderate because the Morphorm seam is days old.

## 8. The flip gate and the perf spike

The flip from architecture 1 to 3 is gated on serval reaching Masonry's
interactive bar. Concretely:

- **IME** across chrome text entry (the long pole).
- **Form-control breadth** beyond the demo text field.
- **The orrery element decision** (section 6), plus its one perf spike below.

**The perf spike (gates the orrery flip specifically):** confirm that
transform-only node motion lands on serval's `RepaintOnly` path, not
`full_relayout`, at orrery scale (hundreds to thousands of nodes, 60fps physics).
The mechanism exists (the `RepaintOnly` vs `full_relayout` split in
`serval-layout/incremental.rs`, and Stylo classifies `transform` as
composite-level damage rather than reflow). It needs measurement, not invention:
model node motion as `transform`, not `left/top`, and measure relayout incidence
on a moving N-node orrery against the `canvas_behavior_contract` scenarios. If
transform composites without relayout, physics-driven motion is cheap and the
orrery flip is unblocked; if it forces relayout, that is the one place a document
engine could feel worse than widgets, and it must be addressed before committing
the whole chrome.

## 9. Near-term implications

- **Do not deepen Masonry investment.** No elaborate Masonry chrome widgets; keep
  platen's layout emission abstract enough to retarget to DOM; keep the scrying
  seam re-homable to netrender external-texture.
- **Continue R1's aether work** (host-agnostic), keep the app-wiring thin.
- **Hold the separate-roots discipline** from day one: chrome-root and content-root
  are distinct document authorities. This is the invariant that goes wrong quietly.
- **Run the perf spike** (section 8) before any commitment to render the whole
  chrome through serval.
- The flip itself, when gated open, is a dedicated implementation-strategy plan,
  not part of this brief.
