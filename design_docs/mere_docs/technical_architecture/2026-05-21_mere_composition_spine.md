# Mere Composition Spine — graph-capable forme, projections, surfaces

**Date**: 2026-05-21
**Status**: Canonical for the arrangement ontology (§2-§8, §10-§15). The
host/realization rows (§1, §7, §9, §12) are pre-flip; read them through the
2026-06-10 correction banner below.

> **2026-06-10 post-flip correction.** The genet-as-host flip has executed
> (see the archived
> [flip plan](../../archive_docs/2026-06-10_completed_plans/2026-06-01_genet_host_flip_plan.md));
> the host rows below predate it. Where this doc says "Xilem app" /
> "Masonry" / "the `GraphCanvas` Masonry widget", the running stack is:
> **meerkat**, a genet-as-host shell whose chrome is authored as
> `xilem_serval` views (a `xilem_core` backend diffing into genet's
> ScriptedDom), laid out by genet (stylo + taffy), painted through netrender.
> Morphorm is gone from the workspace: between-tiles geometry is flex DOM
> emitted by `platen-view` and laid out by genet. The orrery shipped as a
> **host-side composition** (scene-paint underlay + physics-positioned DOM
> under one camera transform), not a Masonry widget and not a genet custom
> element. **Verso's chartered realization role has no code counterpart
> today**: verso-core survives as a thin ID/surface-types layer, platen
> carries an unused verso-core dep, and the realization work landed in
> constellation actors + platen-view + meerkat's compositing; §14.3's
> "TileManager survives as Verso runtime" never happened. Verso's disposition
> was decided and executed 2026-06-10: the crates are deleted (topology §9)
> and the name is designated for the compatibility-view / engine-flip charter
> ([verso_docs charter](../../verso_docs/technical_architecture/2026-06-10_compatibility_view_charter.md)).
> The model→plan layers
> (forme, platen) and the arrangement ontology are unaffected. A full
> §1/§7/§9/§12 refresh is still owed (topology doc, staleness flag).

Refines (does not
replace) [`2026-05-21_app_architecture_rescaffold.md`](2026-05-21_app_architecture_rescaffold.md)
— that doc fixed the *framework* question (chrome = idiomatic Xilem, retire
substrate-as-host, no action bus); this doc fixes the *arrangement ontology* it
left vague, and **corrects its "collapse forme/platen" claim** (§9).
**Provenance**: Two review passes (2026-05-21) + Woodshed lessons, against the
live code. The arrangement model here is not new — it is forme's own charter,
which the code drifted from (§3).

---

## 1. What Mere is

> **A graph-first browser/workbench: graph truth, projected into composable
> surfaces.**

Not a capability taxonomy, not a printing-press pipeline that replaces the UI
framework. The durable spine:

```text
Graph truth            kernel Graph, relations, provenance, session, lineage
  → Forme              per-workbench ARRANGEMENT (graph-capable, not tree-bound)
      → Platen         compiles an arrangement into a presentation PLAN
          → projection { split tree | tab stack | lattice | corridor | spatial bench | graph canvas }
              → Verso  composable tile/surface realization + lifecycle
                  → Inker   which engine backs a tile (Nematic | Genet | Scrying)
  → Host             Xilem app: realizes the plan as a view tree; window, input, GPU
```

Frame chrome (OS-window pane splits) is a *separate, deliberately tree-shaped*
authority — see §7.

## 2. The one move that organizes everything

**The first convenient layout projection (a tree) was masquerading as the
ontology.** A tree is a fine *realization* of a workbench — the default desktop
one. It is not the *shape of arrangement truth*. Once you separate the
arrangement (what this workbench *is*) from its projection (how it's laid out
right now), the rest falls out — including the realization that **the orrery and
a tiled workbench are the same kind of object** (§5).

## 3. Forme's charter (it was always graph-capable)

forme's own crate header:

> *"A forme is whatever shape the typesetter locks up — not forced into a tree…
> projects graph members + edges into the arrangement, which may be tree-shaped,
> lattice-shaped, or arbitrarily connected… the type name still says 'tree' —
> that is the* default output shape*, not the input contract."*

So graph-capable arrangement is forme's stated intent. The **code is behind**:
`topology.rs` is `TreeTopology`, `tree.rs` is tree-first, alongside heavy
graphshell-derived machinery (`graphlet.rs`, `lens.rs`, `reconciliation.rs`,
`pressure.rs`). The correction is therefore **advance forme to its charter**
(arrangement is a graph), **not** "park forme and build a simple TileTree" — that
would re-commit the tree-as-ontology mistake the rename explicitly rejected.

**Discipline (the counterweight):** advance the *shape* to graph; keep the v1
*vocabulary* small (§8); **park** the graphlet-reconciliation / pressure / lens
machinery until a real surface pulls on it. Graph shape, small vocabulary,
parked ambition.

## 4. The printing-press metaphor, at its true home

A **forme** (printing) is the locked-up type in the chase — the arrangement,
composed and ready, *before* anything is printed. The **platen** presses it into
an impression. So `forme = arrangement authority`, `platen = projection /
impression compiler` is etymologically exact and earns its name. The metaphor
only broke when stretched into "an executable forme→platen→verso pipeline that
replaces the framework." As **model → plan → render feeding Xilem**, it is the
authentic spine. (This is the correction to the re-scaffold doc's "collapse
forme/platen": they have sharp, rent-paying roles — §9.)

## 5. The unification: orrery and workbench are one authority

A pane kind selects a **projection**, not a different arrangement model:

- **Tiled workbench** = the *tree* projection of a forme arrangement.
- **Orrery (graph canvas)** = the *cartography* projection of a forme arrangement.
- **Compare-fan / corridor / lattice / spatial bench** = other projections.

So the `GraphCanvas` widget is not a special case beside the workbench — it is
*one of platen's projection targets*. Arrangement lives on a **spectrum** from
pure containment (split tree) to pure relation (the orrery), with the
interesting benches (compare, corridor, storyboard) in between. Mere is the
browser that treats that whole spectrum as one model.

**What a non-tree workbench is:** coherence comes from arrangement *relations*,
not from containment. A corridor bench (reading tile with predecessor/
successor/citation along a navigable, branching path); a compare-fan (one anchor,
sibling renderings around it: original / extracted text / script / graph / source
/ commentary); a lattice (rows = graph members, columns = original / transformed
/ summary / relation); a spatial stage (pinned tile islands, visible relations,
focus halos); a storyboard (ingest → inspect → compare → compose → publish, tiles
recurring across steps). A tree fights all of these; a forme arrangement holds
them and projects a tree *when a tree is what the host wants*.

## 6. Naming

The arrangement is **`forme`** / an **`Arrangement`** — **not** `TileTree` or
`TileGraph`. Not every arrangement node is a rendered tile: some are member
intents, groups, portals, collapsed graphlets, focus corridors, or placement
constraints. Tiles are what *some* nodes realize into, via Verso.

## 7. Who owns what

| Layer | Crate(s) | Owns | Shape |
|---|---|---|---|
| Truth | `kernel`, `cartography`, `arrangements`, `session-runtime` | graph, relations, provenance, session/manifest/view-intent, spatial-projection math | graph |
| **Arrangement** | `forme` | what a workbench *is*: members, groups, tiles-intents, focus paths, comparisons, portals | **graph-capable** |
| **Projection** | `platen` | compile an arrangement → a presentation plan for a host mode | tree / cartography / … |
| **Surface** | `verso` | composable tile/surface realization + lifecycle; the tile system Mere mounts into workbenches | — |
| **Engine** | `inker` (+ `nematic`/`scrying`/Genet) | which engine backs a tile's content; route by pin/type/scheme/override | — |
| **Frame chrome** | `frame` (FrameTree) | OS-window pane splits, pane placement, pane kinds, persistence | **tree** (deliberately) |
| **Platform** | `meerkat` (Host) | Xilem app: realize the plan as a view tree; window, input, frame loop, GPU *(pre-flip row; see the 2026-06-10 banner)* | — |

**FrameTree stays a tree on purpose.** Window/GPU split-hosting has hard
rectangular/realization constraints that semantic arrangement does not. Don't
graph-ify the frame; do graph the arrangement *inside* a workbench pane.

## 8. The v1 arrangement vocabulary (small, explicit)

forme advances from tree-only to a small arrangement graph. Start here; grow on
demand:

- **Nodes**: `WorkbenchRoot`, `MemberIntent` (a graph member to show),
  `TileIntent` (a curated tile), `Group`, `Portal`/`Mirror` (a tile referenced
  in another context), `CollapsedGraphlet`.
- **Edges**: `MemberOf`, `StackedWith` (tabs), `SplitWith`, `AdjacentTo`,
  `CompareWith`, `FocusPath`, `PinnedIn`, `MirrorOf`, `PresentsGraphMember`.

This is enough to express tabs/splits/groups (the conventional tiled workbench,
tree-projected) *and* compare/corridor relations (projected by their own
projections later). It is **not** the graphlet-reconciliation engine — that
stays parked.

## 9. Projection + realization

- **platen** projects a forme arrangement into a **presentation plan**. v1
  projections: **Tree** (the tiled workbench) and **Cartography** (the orrery).
  Others (lattice, corridor, spatial, storyboard) are added when a surface needs
  one — not built up front.
- **Three persistence scopes** (resolved 2026-05-22 — corrects an earlier
  conflation that put positions in the pane sidecar):
  - **forme** owns *semantic arrangement* (group / stack / split / compare /
    pin relations) — geometry-free.
  - **projection geometry** owns *semantic geometry* keyed `(FormeRef,
    ProjectionKind)` — split *ratios*/order (tree), canonical *world* positions
    (cartography). **Not** pixel rectangles, so panes of different sizes render
    responsively from one saved geometry. Shared across panes projecting the
    same forme the same way; two independent layouts are *forked formes*, not
    one bench in two views.
  - **pane view-intent** (`frame_id + pane_id`, already in `session-runtime`)
    owns *pane-local* state only: camera/pan-zoom, focus, selection, local
    relation-hide.
- **Host (Xilem)** renders the plan as a view tree: FrameTree → `split` views;
  a tree-projected workbench → tile content placed at platen-computed rects (the
  between-tiles geometry is platen's, via morphorm — see the
  [between-tiles layout seam](2026-05-26_between_tiles_layout_seam.md); Masonry
  owns only within-tile content); the orrery → the `GraphCanvas` Masonry widget
  (the cartography projection); Verso surfaces → each tile's content compositor
  (in-scene paint or embedded texture).

So forme/platen produce the *plan*; Xilem *renders* it. "Chrome = idiomatic
Xilem" (the re-scaffold doc) still holds — forme/platen are the model→plan layer
*above* the view functions, not a replacement for them.

**Host-architecture note (2026-05-29; 2026-06-10: executed — see the banner).**
The "Host (Xilem)" row above describes
architecture 1 (Xilem authoring, Masonry within-tile content). Genet-as-host
(architecture 3) is the evaluated destination: genet renders both between-tiles
and within-tile through one engine, chrome authored via `xilem_serval` and painted
through netrender. It changes the *realization substrate* at the bottom of the
spine (the Masonry scene becomes genet DOM + netrender) while leaving the
model→plan layers (forme, platen) and "chrome = idiomatic Xilem" (now an
`xilem_core` backend beside Masonry) intact. platen loses Morphorm to genet's
taffy; the orrery becomes a custom-paint element with physics-positioned DOM
children. See the
[genet-as-host evaluation](2026-05-29_genet_as_host_evaluation.md) for the
decision, pros/cons, and the worked orrery + platen consequences.

## 10. Costs we accept (and the guardrails)

Graph-capability costs what a tree gives for free:

- **Focus/traversal** isn't implicit: needs explicit `FocusPath`/ordering edges.
- **Projection determinism** needs the view-intent sidecar (same arrangement →
  same layout means storing positions/collapse per projection).
- It is **seductive enough to become a research project.** Guardrail (Woodshed:
  "keep the model richer than the first grammar," + "extract seams when pressure
  is real"): graph *shape*, **small v1 vocabulary** (§8), **two projections**
  (tree + cartography), a clean projection seam, graphlet-reconciliation/pressure/
  lens **parked**. Build a third projection only when a real bench pulls on it.

## 11. Supersedes / demotions

- **`PaneContent::Tile`** is demoted from "the multi-tile model" to a **lift-out /
  pinned-reference affordance** (tear a tile into its own frame pane). The
  multi-tile model is the forme arrangement, tree-projected inside a Workbench
  pane. (Supersedes the durable center of the 2026-05-11 pane-UX `PaneContent::Tile`
  shortcut.)
- **The re-scaffold doc's "collapse forme/platen"** is corrected: forme is the
  arrangement authority, platen is the projection compiler. They are not
  over-engineering under this ontology; they are the spine.
- **FrameTree** is affirmed as tree-shaped; **forme** is not.

## 12. Build order

*(Pre-flip sequencing; largely executed in genet-as-host form — see the
2026-06-10 banner. Step 5's verso surface contract never got a consumer.)*

On the `meerkat` Xilem skeleton (FrameTree = split views already landed):

1. **forme v1**: advance topology from tree-only to the small arrangement graph
   (§8); keep graphlet/reconciliation/pressure parked. Tree projection first.
2. **platen Tree projection**: arrangement → tiled-workbench presentation plan.
   Grow `TileManager` into the tile-realization layer Verso owns.
3. **Workbench pane** renders the tree-projected plan (nested split/tab views).
4. **platen Cartography projection** + the **`GraphCanvas`** widget for the
   orrery pane (ports the orrery/graph-node painting from `meerkat`).
5. **Verso surface** contract + first **engine tile** (`scrying.web`) via inker.
6. Retire the substrate-as-host crates from the product path once parity lands.

## 13. The test of authenticity

The design is authentic when it says what Mere *is* — graph truth projected into
composable surfaces — and follows the work already landed (`frame`,
`TileManager`, `inker` routing, surface contracts, cartography, forme's charter).
It is *clever in the middle* the moment a layer exists for a metaphor or a
capability taxonomy rather than for an arrangement, a projection, a surface, or
an engine choice a user can see.

## 14. Resolutions (2026-05-22)

The forks are settled (three review passes). The build follows these.

1. **Arrangement vs. graph truth** — a forme is an **independent curated
   arrangement** that references graph truth; it does **not** auto-reconcile.
   Binding modes: `Curated` (explicit, hand-placed) · `Identity` (read-through
   "all members," for the orrery — *not* a copied roster) · `LinkedDerived`
   (graphlet/reconciliation-backed; later). Promote arrangement facts into
   durable graph Arrangement-relations only on an explicit pin/save.

2. **Pane ↔ forme** — a forme is an **arrangement instance bound to a graph**,
   *not* the singleton for a graph-view. A graph carries many formes (identity
   orrery, curated workbenches, compare benches, later derived). A graph-bearing
   pane binds **`graph_id + FormeRef + projection_kind + view_state_ref`**, where
   `FormeRef = Stored(FormeId) | Identity(GraphId)`. Same authority *type*,
   different *instances + projections* — that is the precise orrery↔workbench
   unification.

3. **Tile ownership** — `forme` owns `TileIntent` (id, inclusion, relations,
   route hint); `inker` owns engine selection; `verso` owns live realization,
   surface lifecycle, cached tile state, within-tile history; **`TileManager`
   survives as Verso runtime keyed by a forme-assigned tile / surface id — no
   longer `NodeKey`** (NodeKey breaks on mirrors / multiple intents per member).
   *(2026-06-10: the verso half of this resolution has no code counterpart;
   realization landed in constellation actors + platen-view + meerkat
   compositing. Retire-or-revive verso is an open decision — see the banner.)*

4. **Position** — see §9's three scopes: geometry-free forme; semantic geometry
   (not pixels) in projection state keyed `(FormeRef, ProjectionKind)`, shared
   across panes; pane view-intent is pane-local only.

5. **v1 target** — **tree-projected workbench with one live engine-backed tile
   first**; orrery second. Done = forme arrangement → platen tree projection →
   Workbench pane renders the plan → Verso realizes a tile → Inker routes one
   real engine.

6. **Crate topology + graphlet fate** — `kernel` · `frame` · `forme` · `platen`
   · `verso` · `inker` · `cartography`/`arrangements` · `session-runtime` are the
   real authorities (crates). Re-home `kernel` + `cartography` out from under the
   `graphshell` supercrate (kernel-under-chrome is upside down); `arrangements`
   moves with cartography. `graph-canvas` survives **narrowed** to a portable
   scene/packet/geometry/hit IR + pure derivation, consumed by the Xilem
   `GraphCanvas` widget (the widget owns interaction + paint; interaction
   reducers stay in the widget until a second consumer pulls them out). Retire
   substrate-as-host crates from the product path. Graphlet / reconciliation /
   pressure / lens machinery leaves the active forme path (git-revivable; keep
   only a minimal `CollapsedGraphlet` hook for the future derived lane).

## 15. Persistence ownership (concrete)

| Store | Lives in | Keyed by | Holds |
|---|---|---|---|
| `FormeDocument` types | `forme` | `FormeId` | id, graph_id, label, arrangement (semantic, geometry-free) |
| `FormeStore` (disk I/O, lifecycle) | `forme` (`store` feature) | `FormeId` | per-session formes under `<session_dir>/formes/`; create/load/delete |
| projection geometry | `session-runtime` (store) / `platen` (types) | `(FormeRef, ProjectionKind)` | semantic geometry (ratios, world positions) |
| pane view-intent | `session-runtime` | `frame_id + pane_id` | camera, focus, selection, local relation-hide |

`forme` owns `FormeId` / `FormeRef` / `FormeDocument` / arrangement schema +
pure mutation **and** the native on-disk store (`forme::store`, gated by the
`store` feature so portable consumers stay `std::fs`-free). The *host* supplies
persistence policy — which session directory, and *when* to save (the clean
`meerkat` host enables `forme/store`, picks `<cwd>/mere-sessions/default`, and
re-saves the workbench forme on every edit).

> **Moved 2026-05-22.** The store originally landed in `session-runtime`
> (mirroring `session_graph_store`). It was relocated into `forme` so the clean
> `meerkat` host can persist formes without depending on `session-runtime` —
> which drags in the substrate control-plane crates (`identity`,
> `control-plane`, `tile-state`) the re-scaffold is retiring. Projection
> geometry + pane view-intent stores stay in `session-runtime` for now; they
> migrate when their substrate consumers are cut.

> **Pane-tier update, 2026-07-24.** The `frame` tier in the table above is now
> `frisket` (renamed 2026-07-14), and it left this repo: it relocated to merecat
> on 2026-07-18 because its `PaneContent` enumerates that app's panes. Ruled the
> same day with Mark: the *reusable* pane model belongs in genet, beside the
> tile contract, the Cambium split/tab furniture, and the surface currently
> trapped in `ports/pelt`. Pelt needs panes and cannot depend on mere, so the
> shared home has to sit at or below genet. **The name frisket travels to that
> genet component; merecat's crate retires into it, and its graph binding
> becomes content payload rather than tree structure.** forme and platen do not
> move: forme's compare/mirror/focus-path relations only mean something over
> graph members, and platen carries the document lane. Direction recorded in
> genet's
> [frisket pane component doc](../../../../genet/docs/2026-07-24_frisket_pane_component_direction.md).
