# Nodes and Cards: the summoning model (design)

**Date**: 2026-07-01
**Status**: Design statement from a Mark session, correcting a misreading and naming the model the sibling docs each hold a fragment of. **§6's terminology rename landed in code 2026-07-02** (meerkat + orrery + platen, compiles + all tests pass — see §6 for the full map). §4's snapshot-deposit fix is still open — not a build plan for that part.
**Related**: [swatch primitive design](2026-06-27_swatch_primitive_design.md) (§10 maps the focus-card slot + summon split), [object card plan](../implementation_strategy/2026-06-21_object_card_plan.md) (owns the slot and the object card), [graph_object_roster_detail_cards_plan](../implementation_strategy/2026-06-29_graph_object_roster_detail_cards_plan.md) (in-roster cards, the embedded variant), [node_representation_arrangement_plan](../implementation_strategy/2026-06-18_node_representation_arrangement_plan.md) (P4 retired the live card, left the snapshot gap; carries a rename banner), [orrery_browser_lane_plan](../implementation_strategy/2026-06-24_orrery_browser_lane_plan.md) (the node ontology, restated correctly below), [gloss_scene_to_dom_migration_plan](../implementation_strategy/2026-07-01_gloss_scene_to_dom_migration_plan.md) (the minimap's DOM node squares, built to the renamed vocabulary).

## 1. The ontology: a node is a node

A node is an **object**: a body in the orrery's physics system with its own hull, and a DOM object in the host (position, hit target, tab stop, accessibility node). It **references** addressed things: documents, files, media, settings pages, capsules, lots of stuff. It is not itself a document, and it is not a card. Its rendered visual form — the colored, faced, hulled square you see on the canvas — is a **gnode**: not a document, not a card, just the node's body.

**Gnode, precisely** (adopted into [TERMINOLOGY](../../TERMINOLOGY.md) 2026-07-02; etymology: g(raph)-node, coined 2026-06-02 as the orrery pool's CSS class, kept for the gnostic reading — the knowable body of the node):

- **A projection, never truth.** Rebuilt per frame from kernel truth (identity, URL, relations) plus gyre's live state (position, hull). It stores nothing; destroying it loses nothing. This is the statement-kernel dividing line applied to rendering: the gnode is derived/live state, on the recomputed side with positions and gyre bodies.
- **Per pane instance.** At most one gnode per (node, pane/swatch instance). The same node open in two panes has two gnodes, possibly at different render tiers. Off-scope or off-pane, the count is zero: the node demotes to an underlay dot, which is *not* a gnode (no per-node element, no identity, no hit rect — raster only).
- **Anatomy.** Three parts plus emphasis channels. **Body**: the silhouette (square/rounded/circle by content type) or custom hull; spatially coincident with the gyre collider — the face IS the collider, so picture and physics cannot disagree. **Face**: the texture on the body — activation-state color, favicon, or sprite. **Caption**: the label riding beside the body, LOD-driven. Emphasis: selection = ring + lift, hover = wash, focus = focus ring; the color channel is reserved for activation state (open/closed/idle), so selection never recolors.
- **One primitive, two render tiers.** A chrome-DOM `.gnode` element in the shell document (the focused pane; themeable, a11y-projectable) or an in-scene Scene layer drawn by the orrery crate itself (secondary panes; cheap). `render_gnodes_as_dom` selects per pane. Same anatomy either way.
- **Pointer-inert.** Gyre owns press/select/drag through the collider; the gnode carries no `on_click` and no focus handler. The a11y tree projects the *node* (as a link, with actions), reading its bounds off the gnode's painted rect.
- **Not to be confused with** the node's *non-spatial* representations — a roster row, a tile tab, a session chip. Those carry node identity (color, selection highlight, per the representation-identity rule) but are rows and handles in panes, not bodies at a position. "Gnode" is reserved for the spatial embodiment.

One boundary left open, not legislated here: the gloss minimap's DOM node squares (`.gloss-minimap-node` — body + state color, no face texture, no caption) are gnodes in spirit at a minimal LOD. Whether they take the term (and eventually the class) is a call for the swatch/LOD work, since the swatch element model already names its node-kind elements per instance.

Two conflations to refuse, separately:

- **Node as document.** The bad inference that the node should *be* or *embed* the page it references. The page is presented at a higher tier (a pelt tile, a card); the node stays a content-type-coded shape with a face.
- **Node as card.** Cards are summoned *about* nodes. They are not the node, and the browser-lane plan's thesis is this distinction, not an argument against cards existing.

(The 2026-07-01 session briefly misread the browser-lane plan as an anti-card thesis; it is an anti-conflation thesis. Recorded so it is not re-derived wrong.)

**Where the conflation came from: the code's own names.** The orrery's per-node DOM squares were built by `node_card_view` (`window_view/views.rs`), snapshotted as `OrreryCard`, classed `.node-card`. Calling the node's rendering a "card" is like calling a desktop shortcut an app (Mark, 2026-07-01); the name alone kept re-seeding the bad inference in every new session. **Renamed 2026-07-02** (§6): the node's rendered body is now a **gnode** everywhere it appears, in the chrome DOM and in the orrery crate's own in-scene Scene layer alike — one primitive, two render tiers, never a card. "Card" is reserved for the summonable family in §2, matching `focus_card_view`, which is a card and kept its name.

## 2. The card family

Cards are summonable surfaces scoped to a node or selection. The family so far:

- **Preview / snapshot card**: the "last visit" peek beside a single selected node.
- **Unvisited card**: the fallback member of the same family, when the node has no visit to peek at.
- **Connections swatch**: the multi-selection card (selected nodes + their edges as a live swatch).
- **Object card**: the per-object control surface (widgets bound to the object's settings).
- **Facet cards** and the roster's detail cards (Link, Graphlet, Field, Facet): the same family embedded in a pane instead of anchored on the canvas.

The aspiration (Mark, 2026-07-01): these converge on **embeddable node control surfaces**, one card system that renders anchored-on-canvas or embedded in the roster / a note / a menu, the way the roster detail cards already do in-pane.

## 3. Summoning

- **Selection summons the default card**: single select summons the preview snapshot card next to the relevant node (unvisited fallback when there is nothing to preview); multi-select summons the connections swatch. This split is live in `render/cards.rs` (the `len == 1` gate).
- **Right-click reaches the rest of the family**: context actions summon the other card kinds (object card today via "Resize"; facet and others as they land).
- **Cycling (open design idea)**: swiping or arrowing between all the cards relevant to a selection, so the summoned card is a position in a deck rather than a dead end. Gesture and ordering undecided.

## 4. The defect this model exposes

The snapshot card currently does no snapshotting. It re-renders from the durable content cache (`render_content_scene` over `load_cached(url)`), which only works for content meerkat itself fetched and can statically lay out (mere://, settings://, smolweb, static HTML). Surface-tier websites are fetched by the system WebView, so the cache has no body and the card renders empty; the node-rep plan P4 declared the deposit hook ("tile close/blur deposits the last scene as the node's snapshot") obviated, which it was only for the cacheable lanes. The WGC capture pipeline captures live frames every frame and nothing deposits one.

Fix direction, when a build is greenlit: deposit real pixels on tile close/blur, per lane (last rasterized band for serval lanes, last captured frame for scry tiles), persisted per node the way favicons and sprites already are. That makes the preview card honest for every lane and gives the unvisited card a crisp boundary (no deposit ever made).

## 5. Code rename executed (2026-07-02)

The §1 rename landed across `meerkat`, `orrery`, and `platen` the session after this doc was written — verified with `cargo check -p meerkat --bin meerkat` (clean) and `cargo test -p meerkat -p orrery -p platen` (344 meerkat tests + 85 orrery tests + 88 orrery-gyre tests, 0 failures). The map, for anyone grepping old names:

| Old (called the gnode a card) | New |
| --- | --- |
| `OrreryCard` (struct, `meerkat/window_view/mod.rs`) | `OrreryGnode` |
| `node_card_view` (fn, `meerkat/window_view/views.rs`) | `gnode_view` |
| `.node-card` (CSS/marker class) | `.gnode` |
| `OrreryRender.cards: Vec<OrreryCard>` | `OrreryRender.gnodes: Vec<OrreryGnode>` |
| `Orrery::render_as_cards` / `set_render_as_cards` (`orrery` crate) | `render_gnodes_as_dom` / `set_render_gnodes_as_dom` |
| `card_bounds` (local, `frame_a11y_panes.rs`) | `gnode_bounds` |
| `CARD_LABEL_CAP` (const, `render/orrery_scene.rs`) | `GNODE_LABEL_CAP` |

One adjacent, genuinely ambiguous name also got fixed (it named the *object card*'s widget queue with the same "node_card" prefix the gnode confusion used, even though it is correctly about a real card):

| Old | New |
| --- | --- |
| `ShellState.node_card_keys` / `take_node_card_keys` | `object_card_keys` / `take_object_card_keys` |

Left alone (already correct — the real card family): `FocusCard`, `FocusCardKind`, `focus_card_view`, `snapshot_card`, `unvisited_card`, `ObjectCard`, `object_card_widget_row`, `Connections`/`connections_swatch_view`, the `crate::card` module, and the roster's `node_card(card: &NodeDetail)` (a genuine roster detail card *about* a node — correctly named, not renamed).

A real staleness bug turned up mid-rename, fixed in the same pass: `gnode_view`'s docstring claimed the gnode "click-selects its node through the shell hit-test," but `input/mouse_dispatch/press.rs` shows presses route straight to gyre (the node-as-object model that superseded that DOM-select draft — gyre owns selection, the gnode is inert to pointer input). The docstring now says so.

Design docs updated to match: this doc, [node_representation_arrangement_plan](../implementation_strategy/2026-06-18_node_representation_arrangement_plan.md) and [tearout_composability_plan](../implementation_strategy/2026-06-19_tearout_composability_plan.md) (rename banners, left the historical prose as-is), [unified_document_host_plan](../implementation_strategy/2026-06-17_unified_document_host_plan.md) (rename banner), [object_card_plan](../implementation_strategy/2026-06-21_object_card_plan.md), [tearout_gestures_plan](../implementation_strategy/2026-06-24_tearout_gestures_plan.md), [graphlet_wiring_plan](../implementation_strategy/2026-06-25_graphlet_wiring_plan.md), [node_body_face_model_plan](../implementation_strategy/2026-06-23_node_body_face_model_plan.md), [native_surface_compositing_plan](../../archive_docs/2026-07-03_completed_plans/2026-06-19_native_surface_compositing_plan.md), [gloss_scene_to_dom_migration_plan](../implementation_strategy/2026-07-01_gloss_scene_to_dom_migration_plan.md), and the canonical [interaction_model_spine](../technical_architecture/2026-06-18_interaction_model_spine.md) (inline fixes, current doc).

## 6. Open questions

- **OQ-1 Cycling gesture**: swipe / arrow keys / tabs on the card; and the deck order for a given selection.
- **OQ-2 Embeddability contract**: what a card needs from its host (anchor rect vs pane slot, drain path, dismissal rules) so canvas and roster render the same card.
- **OQ-3 Snapshot deposit**: cadence and storage cost of per-node pixel snapshots (they persist like sprites, but sprites are user-chosen and rare; snapshots accrue per visited node).
