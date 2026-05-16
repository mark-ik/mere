# Spatial Chrome IR — framing brief

**Date**: 2026-05-15
**Status**: Framing probe (exploratory; decides nothing; flags follow-ups)
**Scope**: Names a chrome-architecture shape mismatch surfaced in conversation: every host framework Mere has considered (iced, gpui, blitz, serval-as-toolkit) is **document-tree-shaped**, but a browser multiplexer / spatial browser is **spatial-relational-graph-shaped**. Sketches what IR would actually fit, what existing traditions overlap, and how renderers (serval, netrender, vello, parley) plug in as painters of node types rather than as the host of the app.

**Related**:

- [`2026-05-11_browser_multiplexer_framing.md`](2026-05-11_browser_multiplexer_framing.md) — establishes Session / Window / Pane / Manifest as the durable structure this IR has to render. The chrome IR sits *under* `FrameLayout` leaves, not next to them.
- [`2026-05-09_netrender_for_engine_documents_brief.md`](2026-05-09_netrender_for_engine_documents_brief.md) — picked option (1) (keep netrender inside serval) for now; this brief is the long-form case for revisiting that, since the chrome IR question moves netrender from "document renderer for serval/nematic" to "compositor of all node types in the chrome graph."
- [`2026-05-11_engine_peers_and_scrying_library_brief.md`](2026-05-11_engine_peers_and_scrying_library_brief.md) — engines as content producers per node type. This brief generalises the same shape to *all* paint within the chrome, not only engine surfaces.
- [`2026-05-10_cartography_layer_brief.md`](2026-05-10_cartography_layer_brief.md) — `LayoutStrategy` / `Projection` / `MinimapDescriptor`. Cartography stays the projection layer *for graph nodes*; this brief is one layer below — the substrate cartography projects *into*.
- Memory: `project_host_framework_glass_gpui` (current pivot to gpui via PlatformSurface — what this brief reopens), `project_mere_domain_layer` (the UX-concept layer above data crates — already half-implements the shape proposed here), `project_multi_window_synced_panels` (graph-shaped session model — relies on the IR being graph-shaped), `project_blitz_serval_convergence` (where serval-as-toolkit was first floated — this brief disambiguates *toolkit-of-pages* from *toolkit-of-spatial-nodes*).

---

## Thesis

> **Mere's chrome is not a document. It is a spatial graph of placeable embeddable surfaces with first-class relations, LOD, and addressable identity. Every host framework we've evaluated is document-tree-shaped, which is why every option keeps feeling like a misfit even when it works.**

Two consequences fall out of that statement, both addressed below:

1. The host shrinks to *window + GPU surface + spatial scene graph runtime + input router*. Everything else is a renderer plugged into a node type.
2. serval / vello / netrender / parley / scrying / wry stop competing for "host" and become **co-resident renderers of different node types** in the same chrome graph.

This brief decides none of the above. It names the shape, the lineage, the slot diagram, and what would have to be true before adopting it.

---

## 1. The shape mismatch

Every framework Mere has been close to picking is structured as a **single rooted document tree**:

| Framework             | Root shape                                | What "the app" is                                          |
| --------------------- | ----------------------------------------- | ---------------------------------------------------------- |
| iced                  | `Element` tree, single window root        | A function `state -> Element`                              |
| gpui                  | View tree under a `Window` root           | Reactive `Entity<T>` rendered via `Element`s               |
| blitz                 | DOM (HTML document) → layout → paint      | A web page with native renderer                            |
| serval-as-toolkit     | DOM (HTML document) → layout → paint      | A web page with the full web engine                        |
| browser embed (wry)   | DOM (HTML document) inside an OS WebView  | A web page composed onto an OS surface                     |

What Mere's chrome wants to express *cannot* be said cleanly inside any of those:

- Multiple coexisting documents (web pages, graph views, panels, custom canvases) in a **shared spatial substrate**, where the *between* of those documents is first-class.
- **Placement** as a property of the surface, not flow-derived from the document.
- **LOD per surface** — Navigator scope (orrery / workbench / volvelle / minimap / palette-preview) is a render parameter of the *placement*, not of the inner content.
- **Stable addressable identity** for placements (`graph_id` / `pane_id` / `view_intent_id`) so the *handle* is portable — drag-tab-out spawns a new OS window that re-attaches the same handle, per [`project_multi_window_synced_panels`].
- **Typed relations** between surfaces (links, transclusions, sync-bindings between panels, navigation lineage, citation, persona-scoping membership) as graph objects, not painted overlays.

A document tree can simulate any one of these (iframes, transforms, CSS containment, custom renderers in a `<canvas>`). What it cannot do is express them all *uniformly*, so accessibility, hit-testing, layout, animation, and persistence all become per-feature wiring instead of substrate.

That is the shape mismatch. It is the same mismatch the [browser-multiplexer framing brief](2026-05-11_browser_multiplexer_framing.md) §1 surfaces at the *session* level — "session content is graph-typed, not an opaque byte stream." This brief surfaces it at the **chrome paint** level: the chrome itself is graph-typed, not a tree.

## 2. What the IR must express

The substrate the chrome compiles down to has five first-class properties. These are what no document tree gives natively:

### 2.1 Placement is first-class

A surface lives at a *transform* in a scene, not at the bottom of a flow. Translate / scale / rotate / 3-axis depth (for stacked / floating / sticky-note overlays) are properties of the placement, not of the inner content. The graph canvas's existing infinite-canvas model (per memory `feedback_graph_canvas_navigation_defaults`) is the right intuition — the chrome substrate has the same shape.

### 2.2 Surfaces are embeds, not subdocuments

A web page, a graph view, a panel, a custom canvas, a knot, a moot volvelle — all are **placed nodes** in the same scene. iframes are the document-world's awkward attempt at this; the IR makes embedding the default, not the special case.

### 2.3 Relations are first-class IR objects

Links between panels, transclusions, sync-bindings (the multi-window-synced-panels case), navigation lineage (per the [relation-taxonomy plan](../implementation_strategy/2026-05-11_relation_taxonomy_and_edge_mutation_plan.md)), citation, persona-scoping membership are *nodes / edges in the chrome graph*, not paint commands or overlays. They have identity, hit targets, persistence, accessibility presence.

### 2.4 LOD is per-node

The same surface renders as a thumbnail, a workbench swatch, a full pane, a deep-zoom — Navigator scope / form-factor as a property of the *placement*. This is exactly cartography's `FormFactor` ([cartography brief](2026-05-10_cartography_layer_brief.md)), generalised one level down so it applies to *every* node type in the chrome, not only graph projections.

### 2.5 Identity is addressable

`graph_id`, `pane_id`, `view_intent_id`, `session_id` are stable handles into the IR. The chrome can be queried, scripted from the action bus ([multiplexer framing §5.8](2026-05-11_browser_multiplexer_framing.md)), persisted, replayed, accessibility-walked, and synced across windows because the spatial substrate is *referenceable*, not opaque pixels.

These five properties are the load-bearing claims. Anything that can express all five is candidate IR; anything that can't is not.

## 3. Lineage — where this shape has been tried before

Worth naming so the substrate is judged against existing tradition, not invented blind. None of these has all five properties together; each has a subset:

- **Spatial hypertext** (Frank Shipman; **Tinderbox**, VKB, VIKI). Notes / cards as placeable, where *the spatial relationship between notes is the semantics*. Layout = meaning. Closest match for §2.1 + §2.3. Does not have embedded engines or LOD.
- **ZUI / Pad++ / Jef Raskin's Archy**. Zooming user interface as native substrate. Closest match for §2.4. Does not have first-class typed relations.
- **Scene graphs** (USD, OpenXR scene, every game engine). Hierarchical placement + LOD + addressable identity. Closest match for §2.1 + §2.4 + §2.5. Has no document model and no concept of typed semantic relations between scene nodes.
- **Morphic** (Self → Squeak → Newspeak). Composable spatial **morphs** that unify input / render / state in one object. Closest match for §2.2. Does not separate placement from content; everything is paint.
- **HyperCard / NoteCards / Project Xanadu**. Cards as first-class addressable units with transclusion as a primitive. Closest match for §2.5. Pre-spatial; layout was page-shaped not scene-shaped.
- **Outliner-graph hybrids (Tana, Logseq, Roam)**. The modern descendants of HyperCard's identity-and-transclusion intuition: every block has a stable id; block-refs render the block in place; the page graph is a first-class view alongside the outline. "Knowledge graph + projections (outline view / graph view / queries)" is structurally Mere's "session = graph, panes = projections" framing, one generation earlier. Closest match for §2.3 (relations as first-class — block refs, supertags, properties) + §2.5 (every block addressable). Tana's supertags + queries are the closest existing thing to typed `SceneEdge::kind` in a shipping product. Lacks §2.1 (layout is outline / page / grid, not free transform), §2.2 (no embedded engines — markdown only), §2.4 (no per-block form-factor / LOD). The substrate's identity + relations layer can borrow their vocabulary directly; the spatial + embed + LOD layers are where Mere goes further.
- **Whiteboard tools (tldraw, Figma, Miro)**. Scene graph + arbitrary embeds + drag/resize. Practical proof §2.1 + §2.2 ship in production today. Flat in identity (no stable per-surface handles outside the canvas), no first-class relations, no LOD-by-form-factor.

What Mere's IR is, in one line: **outliner-graph hybrid's identity-and-relations layer + spatial hypertext's layout-as-meaning + ZUI's per-node LOD + scene graph's transform stack + embeddable web/document/canvas surfaces**. That combination has no off-the-shelf name, which is part of why it keeps feeling like a missing layer when described from any one tradition's vocabulary.

## 4. Renderers as plug-ins per node type

If the substrate is a spatial graph of placed embeddable nodes, then the renderer question reframes. Today's competition (gpui-vs-vello-vs-blitz-vs-serval-as-toolkit) becomes **co-residence**: each renderer claims certain node kinds.

Illustrative-signature-only sketch (not implementation-ready; types renamed for clarity):

```text
SceneGraph {
    nodes: SlotMap<NodeId, SceneNode>,
    edges: SlotMap<EdgeId, SceneEdge>,
    transform_index: SpatialIndex<NodeId>,   // for hit-testing + view culling
    identity_index: HashMap<StableHandle, NodeId>,
}

SceneNode {
    placement:   Transform,                  // §2.1
    lod:         LodLevel,                   // §2.4
    identity:    StableHandle,               // §2.5  (graph_id | pane_id | view_intent_id | ...)
    content:     NodeContent,                // §2.2
    a11y:        AccessibilityNode,          // uxtree integration
}

NodeContent {
    WebPage(EngineProfileBinding, Url),      // serval | scrying | wry
    GraphView(GraphId, ViewIntent),          // cartography → vello
    Panel(PanelKind, ViewIntent),            // mere-domain panel descriptor → vello/parley
    Knot(EngramHandle),                      // nematic.knot → platen → vello
    DocumentTile(EngineDocument),            // platen → netrender Scene → vello
    CustomCanvas(CanvasHandle),              // direct vello
    Composite(SceneGraph),                   // recursive nesting (volvelle, nested orreries)
}

SceneEdge {
    kind:        RelationKind,               // reuse mere-kernel's taxonomy
    endpoints:   (NodeId, NodeId),
    rendering:   EdgeRendering,              // line | transclusion | sync-binding | hidden
}
```

**Renderer registry, illustrative-signature-only:**

```text
trait NodeRenderer {
    fn handles(&self) -> NodeContentKindSet;
    fn paint(&self, node: &SceneNode, ctx: &mut PaintCtx) -> PaintResult;
    fn input(&self, node: &SceneNode, event: &InputEvent) -> Option<Action>;
}
```

How current crates would map onto this:

| Node content kind     | Renderer                                                                   | Status                                     |
| --------------------- | -------------------------------------------------------------------------- | ------------------------------------------ |
| `WebPage` (full)      | **serval** (style + layout + paint into vello scene via netrender)          | netrender mainline shipped 2026-05-04      |
| `WebPage` (system)    | **scrying** (mere-managed system WebView; embedded-frame composition)       | per [scrying-web plan](../implementation_strategy/2026-05-11_scrying_web_tile_plan.md) |
| `WebPage` (overlay)   | **wry** (overlay composition; fallback)                                     | per [engine-peers brief](2026-05-11_engine_peers_and_scrying_library_brief.md) |
| `GraphView`           | **cartography → vello** directly                                            | cartography contracts in flight            |
| `Panel`               | **mere-domain panel descriptor → vello + parley**                           | needs reactive runtime — biggest new piece |
| `Knot` / `DocumentTile`| **platen → netrender Scene → vello**                                       | depends on platen growing layout (parley)  |
| `CustomCanvas`        | **direct vello** (paint hook with scene-builder access)                     | already the graph canvas's pattern         |
| `Composite`           | **recursive scene graph traversal**                                         | substrate-level                            |
| Edges                 | **parley** (labels) + **vello** (lines) — possibly own renderer crate       | small, isolated                            |

The substrate is the toolkit. serval/vello/netrender/parley/scrying/wry are tenants of the substrate.

## 5. ECS framing

The Mark intuition that surfaced this — *"why don't we just do our own ECS?"* — names the shape directly. The substrate is closer to a game-engine ECS than a UI toolkit's view tree:

- **Components**: `Placement`, `Lod`, `Identity`, `NodeContent`, `RelationEndpoint`, `AccessibilityNode`, `InputBindings`, `PersistenceKey`.
- **Systems**: `layout-by-relation` (relation-driven repositioning), `render-by-content-type` (dispatch to renderer registry), `input-by-spatial-hit` (spatial index → identity → owning renderer), `lod-promotion` (resolve form-factor at zoom thresholds), `accessibility-tree-emit` (ECS → uxtree → AccessKit).

This is how scene graphs / game engines have always worked — *because* a scene graph is structurally an ECS that happens to use spatial transforms as one component. Mere's chrome wants the same structural treatment.

The reactive layer (`Entity<T>` + cx, à la gpui) sits **above** the ECS as the data-binding mechanism for `NodeContent` mutation. It is not the substrate; it is one of the things the substrate can be driven by. This separation is what gpui collapses (its reactive model and its scene-tree-shaped renderer are the same thing); pulling them apart is what enables the renderer-registry shape.

## 6. What this reframes about the host question

Currently canonical (per `project_host_framework_glass_gpui`): **gpui as host, embed vello/netrender/serval surfaces via PlatformSurface (OS composition layers).** That decision was right *for the framing it answered*: "what hosts the iced-shaped Mere app?" The PlatformSurface route lets a tree-shaped host coexist with a foreign-shaped renderer at the OS layer.

This brief asks a different question: **what if the chrome itself is not tree-shaped?** Under that framing the host stops being "the framework that owns the view tree" and shrinks to:

```
mere-host = window manager + GPU surface + spatial scene graph runtime + input router
```

Everything above shrinks; everything below stays as it is. Critically:

- **mere-domain stays portable.** Already gpui-free per `project_mere_domain_layer`. *Vocabulary-half* the IR — `workbench` / `orrery` / `gloss` / `apparatus` / `system` / `graphshell` / `murm` / `moot` are the canonical *node content kinds* the substrate would dispatch on. **Structurally, today's output is `UxTree`** (`frame::project_frame_with`, `workbench::project_workbench`, `orrery::project_graph`, `gloss::project_outline`, `apparatus::project_skeleton` all emit a flat `Vec<(NodeId, accesskit::Node)>` whose `Node::children: Vec<NodeId>` builds a strict tree). The substrate needs either a new output mode per crate that emits `(NodeContentKind, StableHandle, Content)` triples directly, or a thin wrapper mapping `UxTree → substrate nodes` for projection only. Either way the crate boundaries are at the right seams; only the output shape changes. The wrapper is the cheaper migration; the new-output-mode is the cleaner one. See §10.7.
- **Two-stack GPU coexistence resolves into one stack.** vello via netrender is the single backend; serval / scrying / wry compose into it (netrender Scene for serval-painted content; OS-composed surfaces for system-WebView content). gpui's blade pipeline drops out.
- **Web-native chrome, without "chrome = web pages."** The substrate is graph-shaped (per §1 above); web pages are *one node content kind*, painted by serval. The toolkit is the spatial graph IR; serval is a tenant. This dissolves the question Mark balked at — the chrome doesn't have to model itself as HTML to be web-native.
- **Multi-window synced panels become substrate-native.** Per `project_multi_window_synced_panels`, drag-tab-out spawns a window that re-attaches the same handle. With identity (§2.5) as a substrate property, this is one ECS query per window, not custom plumbing.

What this brief **does not claim**: that the substrate-as-host should ship next, or even that it should ship at all. The case for reopening the host question rests on three preconditions, listed in §8.

## 7. Connection to existing architecture

The substrate slots into work already in flight without reshaping any of it. Roughly:

```
                    ┌─────────────────────────────────────────┐
                    │ Multiplexer concern                     │
                    │  SessionId, GraphSessionManifest        │
                    │  registry, FrameLayout, panes           │
                    │  action bus, capability gates           │
                    └────────────────┬────────────────────────┘
                                     │  per-pane (session_id, view_intent)
                                     ▼
                    ┌─────────────────────────────────────────┐
                    │ Cartography concern                     │
                    │  LayoutStrategy, Projection             │
                    │  ViewIntent, FormFactor, Overlays       │
                    │  MinimapDescriptor                      │
                    └────────────────┬────────────────────────┘
                                     │  projected scene contents
                                     ▼
                    ┌─────────────────────────────────────────┐
                    │ Spatial Chrome IR  (this brief)         │
                    │  SceneGraph: nodes, edges               │
                    │  Placement / LOD / Identity / Content   │
                    │  Renderer registry per NodeContentKind  │
                    │  Spatial input router → AccessKit       │
                    └────────────────┬────────────────────────┘
                                     │  paint dispatch
                                     ▼
                    ┌─────────────────────────────────────────┐
                    │ Renderers (co-resident)                 │
                    │  serval | scrying | wry | platen        │
                    │  vello | netrender | parley             │
                    └─────────────────────────────────────────┘
```

The multiplexer brief said *what is durable* (session manifest, view intent). Cartography said *how a graph projects into a pane*. This brief says *what the pane is rendered into*, and observes that the answer is itself graph-shaped one level down.

mere-domain already names the right node content kinds (workbench, orrery, gloss, apparatus, …). The substrate is what those become *placed instances* of, with identity, LOD, and relations. No mere-domain churn; the layer just gets a real backing IR underneath instead of being stitched together inside a gpui view tree.

## 8. What would have to be true before adopting this

This brief is a framing probe, not a decision. Three preconditions before reopening the host pivot:

### 8.1 Renderer maturity

netrender mainline shipped 2026-05-04 (per `project_netrender_three_paths`). parley wiring is established (per `project_parley_over_cosmic_text`). The "build our own host on netrender + parley" cost is *much lower than it was when gpui was picked* — but lower is not zero. The renderer stack must be load-bearing through Workbench-sized usage before the substrate-as-host is on the table.

### 8.2 OS-plumbing reuse strategy

The renderer is the cheap part. The expensive parts of any host framework are the OS-plumbing surfaces gpui has years of hardening on:

- IME (Windows/macOS/Linux input method editors)
- Accessibility (UIA / AT-SPI / AX integration via AccessKit)
- Focus rings + keyboard navigation
- Drag-and-drop (intra-app + OS-mediated)
- Window decoration handling, system menus
- Clipboard, color management, HiDPI

Per `feedback_dont_dismiss_borrowable_components`, the right move is to *layer-sketch reuse* before rebuild. Plausible: lift these subsystems out of gpui as standalone crates (license-permitting) under a `mere-os-plumbing` umbrella; let them serve a new substrate-native host. Implausible (and not proposed): rewrite them. The reuse strategy must be sketched concretely before adoption is on the table.

### 8.3 Substrate-as-host parity demonstrated

A prototype `mere-host/scene-graph` (parallel to `mere-host/gpui`) that hosts one Workbench-sized surface end-to-end — placement, LOD, render dispatch, input routing, accessibility — at parity with the gpui path. Until parity, the gpui host is the correct integration; the substrate exists as a research track only.

Until all three preconditions are met, the canonical host stays gpui via PlatformSurface. This brief flags the substrate as the *long-term shape* once preconditions clear, and recommends mere-domain continue holding the line on portability so the future migration is cheap.

## 9. What the substrate dissolves and what it doesn't

### Dissolves

- **gpui-vs-vello two-stack coexistence.** One GPU stack (vello via netrender) backs the substrate; gpui's blade pipeline drops out under the substrate-as-host scenario.
- **"chrome = web pages" objection.** The substrate is graph-shaped; web pages are one node content kind. Mere stays web-native without modelling itself as a web page.
- **Bespoke per-host rendering paths.** No more "the gpui host renders documents this way and the iced host rendered them another way." One scene IR; one render dispatch; renderers are dependency-injected.
- **iframe-shaped embedding awkwardness.** Embedded surfaces are ECS nodes, not nested document subtrees. Cross-surface relations (sync-bindings, transclusions, navigation lineage) are first-class.

### Doesn't dissolve

- **Engine profile / site data boundary** ([multiplexer framing §5.4](2026-05-11_browser_multiplexer_framing.md)). Engines still own their own state; the substrate references it via `EngineProfileBinding`. Same boundary, different substrate.
- **Capability gating** ([multiplexer framing §7](2026-05-11_browser_multiplexer_framing.md)). Cross-surface operations still route through the action bus; capability gates still apply. The substrate makes the *targets* of those gates more uniform (every cross-surface operation has a `(NodeId, NodeId)` shape), not weaker.
- **OS plumbing.** §8.2 above — this is where the work actually is.
- **Performance ceilings.** A spatial scene graph with thousands of placed nodes is a real engineering problem. Spatial indexing, view culling, LOD promotion, retained vs. immediate sub-trees are all live questions. Workbench-sized usage will probably surface them one at a time.

## 10. Open questions

### 10.1 Edges as content vs paint

§2.3 says relations are first-class IR objects. But edges-as-paint (lines between nodes drawn directly into the scene) is a viable alternative for cases where the edge has no semantic identity beyond "those two things relate." Cleanest answer: edges are first-class IR by default, edge-painting is an optimisation when the edge has no addressable handle. Decision deferred to substrate prototype.

### 10.2 Reactive runtime — own vs. borrow

Per §5, the reactive layer (`Entity<T>` + cx) sits above the ECS. Own implementation (small, well-known pattern, ~weeks) or borrow gpui's reactive runtime as a standalone crate (license-permitting, large surface area, well-tested)? Lean borrow if extractable; own if not. Resolved by §8.2's plumbing reuse strategy.

### 10.3 Scope of substrate adoption

Whole chrome (every pane, every overlay, every chrome element) or only the *between-panes* spatial layer (panes as opaque rectangles inside the substrate, gpui-style trees inside each pane)? Whole-chrome is the architecturally clean answer; pane-level is a faster migration. Likely answer: pane-level first, whole-chrome as renderers mature. Decided per migration plan if/when adoption advances.

### 10.4 Persistence shape

Substrate state is a graph; mere-kernel's graph is also a graph. Same substrate or layered separately? Likely separate — chrome substrate state is per-frame ephemera (placement, LOD, view intents), kernel graph state is durable knowledge. But the IR shape is identical, which raises the question of whether the substrate IR can borrow mere-kernel's serialization machinery wholesale. Tracked here for the migration plan.

### 10.5 Naming

"Spatial chrome IR" is descriptive. Per `user_aesthetic_word_list` and the project's naming sensibility, a more soulful name probably exists. Candidates noted for later naming pass: *strophalos* (already in lexicon for the running instance — overloaded), *scry*-family (taken by scrying engine), *atlas* / *arena* / *chora* (the philosophical "place / receptacle of becoming") / *metacosm*. Not deciding here.

### 10.6 Relationship to the donor graphshell repo

Per `project_graphshell_donor_not_authority`, the external graphshell repo is reference material — useful prior thinking on edges, relation families, history import. The substrate IR proposed here is a layer the donor repo never reached, but its edge / relation modelling is informative. Future migration plan should pass over the donor repo for borrowable substrate-shaped fragments before original-writing them.

### 10.7 mere-domain output shape — wrapper vs. new output mode

Per the §6 amendment, today's mere-domain crates emit `UxTree` (flat `Vec<(NodeId, accesskit::Node)>` building a strict accesskit tree). The substrate wants `(NodeContentKind, StableHandle, Content)` triples. Two paths:

- **Wrapper (cheaper).** A `mere-substrate-bridge` crate that maps each crate's existing `UxTree` output into substrate nodes by walking the tree and recovering structure from the path-keyed NodeIds. Mere-domain crates don't change.
- **New output mode (cleaner).** Each crate grows a second projection function that emits triples directly (`project_workbench_substrate`, etc.). The substrate consumes these natively; `UxTree` becomes an a11y-only emission downstream of substrate state.

The wrapper is the migration-cheap path but pretends mere-domain is graph-shaped when it isn't. The new-output-mode is the substrate-honest path but touches every mere-domain crate. Decision deferred to the modular adoption plan (§12) — it's likely a sequence: wrapper first to unblock substrate development, new-output-mode per crate as their reactive runtime hookup lands.

**Boundary constraint:** AccessKit forces tree shape at the OS edge regardless. Even with substrate-native graph state, the a11y emission has to flatten to a tree before AccessKit's `TreeUpdate` accepts it. "Graph internally, tree at OS boundary" is the same pattern engines already use (each engine paints freely; the OS composes); the substrate inherits the same discipline.

## 11. Crucial decisions made by this brief

Decisions are deliberately minimal — this is a framing probe.

1. **The chrome IR shape question is real and load-bearing.** Document-tree hosts can ship Mere; they cannot express its multiplexer / spatial-browser shape natively. Future host evaluations must include a substrate-native option in the comparison set.
2. **mere-domain stays portable.** No gpui leakage, ever. Per `project_mere_domain_layer`. This brief makes the long-term reason for that constraint explicit.
3. **Renderer registry shape is the right framing.** Whether or not the substrate-as-host ships, treating serval / vello / netrender / parley / scrying / wry as **renderers of node content kinds** (rather than as competing hosts) is the correct mental model and should drive how their integration is described in subsequent docs.
4. **Five first-class properties (§2) are the substrate test.** Any future IR proposal — whether a spatial substrate, a serval-as-toolkit revival, a return to iced, or anything else — is judged against whether it can express all five uniformly.
5. **Substrate adoption requires §8's three preconditions.** Renderer maturity + OS-plumbing reuse strategy + substrate-as-host parity demo. Until met, gpui via PlatformSurface remains canonical.

## 12. Follow-ups

Four of these are filed (same day as this brief — all useful even under the current gpui host); the rest are *natural* next briefs / plans if the substrate framing holds up under review, filed when picked up.

- **Browser taxonomy translation brief** ✅ filed — [`2026-05-15_browser_taxonomy_translation_brief.md`](2026-05-15_browser_taxonomy_translation_brief.md). Maps the spatial chrome / renderer-registry framing onto conventional browser subsystem language (Firefox-like parent/content/rendering/extension/process taxonomy), and spells out the impact on embeddable-browser, extension/mod, PWA/browser-hosted, p2p, and smolweb goals.
- **Renderer registry contract brief** ✅ filed — [`2026-05-15_renderer_registry_contract_brief.md`](2026-05-15_renderer_registry_contract_brief.md). The `NodeRenderer` trait, three composition modes (in-scene paint / embedded-frame / overlay), per-renderer mapping table, relationship to inker's existing traits.
- **OS-plumbing reuse audit** ✅ filed — [`2026-05-15_os_plumbing_reuse_audit_brief.md`](2026-05-15_os_plumbing_reuse_audit_brief.md). Per-subsystem extraction posture across 14 subsystems; **IME on macOS is the single substrate-as-host blocker; most others are ecosystem-covered** — a more reassuring finding than §8.2 anticipated.
- **Spatial chrome modular adoption plan** ✅ filed — [`../implementation_strategy/2026-05-15_spatial_chrome_modular_adoption_plan.md`](../implementation_strategy/2026-05-15_spatial_chrome_modular_adoption_plan.md). Sequences taxonomy cleanup, renderer-registry v0 under the current host, NetRender/Vello composition proof, native texture interop consolidation, PWA/browser-host envelope, session/p2p sync-state split, OS-plumbing proof gates, and substrate-as-host parity demo.
- **Substrate prototype plan** (`mere-host/scene-graph`). Folded into the modular adoption plan's Phase 8; split out only if/when the preceding proof gates clear.
- **Chrome IR persistence brief.** Per §10.4 — relationship between substrate state and mere-kernel graph state, serialization sharing.
- **Substrate naming brief.** Per §10.5.
- **Relation-shape comparison with the donor graphshell repo.** Per §10.6 — what edge / relation modelling fragments are borrowable into the substrate IR.
- **IME acceptance-criteria brief.** Per the OS-plumbing audit §8.2 — define what "polished IME" means in Mere's context so the IME work isn't open-ended.
- **Honest-broker review.** Per the OS-plumbing audit §8.5 — companion to the substrate prototype plan; explicitly articulate whether Mere-on-winit + ecosystem + substrate is materially better than gpui-as-host + spatial-substrate-as-overlay.

## 13. What this brief does and does not decide

**Does:**
- Names a chrome architecture shape mismatch and the IR shape that resolves it.
- Establishes five first-class IR properties (§2) as the substrate test.
- Frames serval / vello / netrender / parley / scrying / wry as **co-resident renderers of node content kinds**.
- Records three preconditions (§8) before substrate-as-host can be reopened.

**Does not:**
- Reverse the gpui host pivot. gpui via PlatformSurface remains canonical until §8 clears.
- Schedule any substrate work.
- Propose a name for the substrate (§10.5).
- Decide the reactive-runtime own-vs-borrow question (§10.2).
- Decide the substrate adoption scope (§10.3).

If the framing is wrong, this brief is the right place to argue against it. If the framing is right, the taxonomy brief, renderer-registry contract, OS-plumbing audit, and modular adoption plan are the natural child artefacts — all useful even under the current gpui host.
