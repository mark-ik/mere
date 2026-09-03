# Nodes and Cards: the summoning model (design)

**Date**: 2026-07-01
**Status**: Design statement from a Mark session, correcting a misreading and naming the model the sibling docs each hold a fragment of. **§6's terminology rename landed in code 2026-07-02** (meerkat + orrery + platen, compiles + all tests pass — see §6 for the full map). §4's snapshot-deposit fix is still open — not a build plan for that part.
**Related**: [swatch primitive design](2026-06-27_swatch_primitive_design.md) (§10 maps the focus-card slot + summon split), [object card plan](../../archive_docs/2026-09-02_retired_plans/2026-06-21_object_card_plan.md) (owns the slot and the object card), [graph_object_roster_detail_cards_plan](../../archive_docs/2026-09-02_retired_plans/2026-06-29_graph_object_roster_detail_cards_plan.md) (in-roster cards, the embedded variant), [node_representation_arrangement_plan](../implementation_strategy/2026-06-18_node_representation_arrangement_plan.md) (P4 retired the live card, left the snapshot gap; carries a rename banner), [orrery_browser_lane_plan](../implementation_strategy/2026-06-24_orrery_browser_lane_plan.md) (the node ontology, restated correctly below), [gloss_scene_to_dom_migration_plan](../../archive_docs/2026-07-04_completed_plans/2026-07-01_gloss_scene_to_dom_migration_plan.md) (the minimap's DOM node squares, built to the renamed vocabulary).

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

Fix direction, when a build is greenlit: deposit real pixels on tile close/blur, per lane (last rasterized band for genet lanes, last captured frame for scry tiles), persisted per node the way favicons and sprites already are. That makes the preview card honest for every lane and gives the unvisited card a crisp boundary (no deposit ever made).

## 5. Current app state (audited 2026-07-04)

Yes, cards can be fixed in the app, but the fix is split into two different problems that should not be lumped together.

### What is already live

- **The focus-card slot is real shell DOM now.** `render/setup.rs::compute_focus_card` projects the card into `OrreryRender.focus_card`, and `window_view/views.rs::focus_card_view` renders it after the `.gnode` pool. That fixes the old "card under nodes" class of bug: snapshot, unvisited, object, and connections cards now sit over gnodes and under chrome overlays by document order.
- **The card family split is live.** Multi-selection routes to the connections card, a context-summoned object card replaces the preview for the selected member, a visited single node gets a snapshot card, and a never-visited single node gets the unvisited placeholder. This is not just prose anymore.
- **The snapshot card no longer flashes an empty hit target while it builds.** The visible `Snapshot` kind only appears once a member-scoped `snapshot_data_uris[member]` entry exists for that member's current URL. The older hidden/phantom rect bug is explicitly guarded in `finalize_content_rects`.
- **Snapshot imagery is now a chrome `<img>` data URI, keyed by member.** `render/cards.rs::compute_focus_cards` first checks a member-scoped cache entry, then the node's persisted `thumbnail_png`, and only then falls back to a synthesized preview scene. `render/paint.rs` rasterizes/readbacks that scene to PNG, and `window_view::FocusCardKind::Snapshot` carries the data URI. This was a necessary layering fix because an external texture would still compose below opaque chrome-DOM gnodes.
- **Synthetic previews now persist to graph truth.** When the app synthesizes a preview from cache-renderable content, `render/paint.rs` now also writes it through `Orrery::set_node_thumbnail`, so the node carries a real `thumbnail_png` instead of the preview living only in a window-local cache.
- **Compatibility-view tiles now deposit on teardown.** The scrying host captures a WebView snapshot when a live surface is reaped, cleared, or dropped by the per-frame retain pass, and the app persists that PNG onto the node thumbnail. So there is now one real surface-tier deposit hook in the app, even though it is boundary-triggered rather than continuous.
- **Live tiles now deposit on navigation-away too.** When the focused workbench tile is retargeted in place by the omnibar, link-follow, history back/forward, or settings-page spine navigation, the app now deposits the old visual before mutating the node URL: scrying tiles capture a live WebView snapshot, and non-scry tiles persist the current cached visible band.
- **Focused-tile switches now deposit too.** When workbench focus moves from one tile to another, or leaves the workbench for the orrery, the previously focused tile deposits once before focus changes. That covers the common "I looked at this tile, then clicked somewhere else" path that used to wait for close or navigation.
- **Workbench close/session-switch boundaries now preserve the active live tile too.** When the workbench closes, a session switches, or a secondary window is closed, the currently active workbench tile now deposits through the live path first: scrying tiles capture a live WebView snapshot, while non-scry tiles persist their current visible band. If a non-scry tile has no cached band to read back at that boundary, the app now falls back to the same durable-cache synthesized producer the snapshot card uses (see the last-viewport bullet below for what that producer now means), instead of skipping the deposit outright.
- **Idle-cadence snapshot refresh is now wired (2026-07-04).** `app_handler/idle_snapshot_refresh.rs` mirrors the Alembic idle-forgetting pass exactly (host-side timer on `Shell`, ticked from `about_to_wait`, no actor): once the app has sat idle 120s, at most every 900s, it redeposits every open workbench tile's thumbnail across **every open window** (not just primary — tile textures/scroll live on each window's own `WindowView`, unlike the shared graph/store the forgetting pass reads). Gated by a new `snapshot_idle_refresh` setting (`settings.json`, defaults on) and a per-session thumbnail byte cap (`snapshot_byte_cap_mb`, default 16 MiB, resolved once at boot). The cap gates only this pass — boundary-triggered deposits and the on-demand snapshot-card render in `render/paint.rs` always proceed uncapped, since those are the correctness-critical paths this design's whole fix depends on; the idle pass is the optional extra. Both deposit funnels (`node_ops.rs::persist_node_thumbnail_png` and the `render/paint.rs` synthesized-preview path) tally into the same running `thumbnail_bytes_this_session` counter. No live settings-UI control exists yet — `snapshot_idle_refresh` and `snapshot_byte_cap_mb` are hand-edit-the-sidecar tunables today, the same shape `retention_keep_n` already has in this codebase.
- **The cache-renderable-lane fallback now windows from the tile's last scroll position (2026-07-04), not always the page top.** `card::render_content_scene` took a `band_y` parameter (was hardcoded to `0.0`); both fallback call sites — `node_ops.rs::persist_synthesized_tile_thumbnail` (the workbench-tile fallback) and `render/cards.rs`'s on-demand snapshot-card builder — now pass the member's `self.view.scroll` entry (defaulting to `0.0` when never scrolled this session, which is also the correct answer then). The genuine win is the case the design's own audit flagged: an open-but-not-currently-painted workbench tile (a background tile in a multi-tile layout whose `tile_textures` entry the render pass hasn't refreshed) now reproduces where the user actually scrolled to instead of resetting to page top on every fallback deposit.

### What is still false

- **The last-viewport fix is bounded honesty, not a promise.** `self.view.scroll` is in-session-only (never restored from `PersistedNodeSessionState.scroll_y` at boot, and cleared on navigation/session-switch), so a tile never opened this session, or opened only after a restart, still falls back to page top on the synthesized path — the best available answer, not a lie, since there is no recorded scroll to reproduce. Wiring `PersistedNodeSessionState.scroll_y` into `self.view.scroll` at boot (it is currently written by the kernel snapshot path but never read back into host view state) would close this gap; not done here.
- **The window cache is still only a cache.** `WindowView.snapshot_data_uris` is now member-scoped and URL-aware, which fixes URL aliasing between nodes, but it is still a per-window DOM-image cache cleared on theme/session transitions. The graph thumbnail is the authority.
- **The remaining live-lane coverage is still uneven.** Compatibility-view / scrying tiles now deposit on teardown, close, navigation-away, focus-switch, window blur, and app suspend; workbench content tiles can persist the current visible band on close/session switch/navigation-away/focus-switch/blur/suspend/idle, with a last-viewport synthesized fallback when a cached band is unavailable. But there is still no dedicated boundary hook for every document / genet-rendered / note-knot path, and the fallback is still a re-render from the durable cache (a reconstructed page image at the right scroll offset), not a captured pixel-exact frame.
- **Boundary coverage is still partial.** The current deposit path now explicitly covers surface reap/clear/retain, navigation-away, focus-switch, window blur, app suspend, and (2026-07-04) idle refresh. The cadence + byte-cap settings exist (`snapshot_idle_refresh`, `snapshot_byte_cap_mb` in `settings.json`), but there is still no live UI control for either — a sidecar hand-edit is the only way to change them today.

### App fix shape

1. **Read node thumbnails before synthesized cache previews.** For a visited focused member, prefer the node's persisted thumbnail PNG. Fall back to the current synthesized cache preview only when no thumbnail exists. A visited node with neither should behave like "no preview yet" rather than pretending the cache renderer saw the page.
2. **Keep writing deposits to the graph, not to `snapshot_data_uris`.** A deposit should produce PNG bytes plus dimensions and apply `GraphDelta::SetNodeThumbnail { key, png_bytes, width, height }`, then mark the graph/session dirty. `snapshot_data_uris` remains a window-local DOM-image cache derived from that thumbnail, not the authority.
3. **Finish the missing lane producers and boundaries.** On close/blur/backgrounding of a tile, deposit the last visual for that member:
   - document / genet-rendered lanes: use the latest rasterized visible band or an explicit top-peek render, depending on whether the card should mean "last seen viewport" or "page top". **Resolved 2026-07-04**: the fallback producer now means "last seen viewport" (windows from `self.view.scroll`), not "page top" — see the "What is already live" bullet above for the exact mechanism and its in-session-only limit.
   - scrying / system-WebView lanes: reap/clear/retain, navigation-away, focus-switch, window blur, and app suspend now deposit; background semantics and any cadence-based refresh still need an explicit decision and hook.
   - local notes / knot lanes: use the retained note/document render path, same as document lanes.
4. **Keep cadence and retention as settings.** Snapshot deposit should not be an every-frame default. The likely default is boundary-triggered deposit (tile close, blur, navigation away, app suspend), with optional "update while idle" and a per-session thumbnail byte cap. **Wired 2026-07-04** (see the "What is already live" bullet above): boundary-triggered deposit stays the unconditional, uncapped default; "update while idle" is a settings-file toggle defaulting on; the byte cap gates only the idle pass. Still open: a live settings-UI control, and whether the idle pass should skip a tile whose content hasn't changed since its last deposit rather than unconditionally re-rendering it every pass.

Done means: a scrying-backed web page opened in a pelt tile deposits a thumbnail, survives app restart, and later selecting that node in the orrery shows the real deposited thumbnail without spinning up the page actor; cache-renderable pages still show previews; unvisited nodes still show the dashed placeholder; object and connections cards keep replacing the preview slot by their current rules; and the remaining non-scry lanes have explicit deposit hooks for the intended boundaries instead of leaning on synthesized fallback alone.

## 6. Code rename executed (2026-07-02)

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

Design docs updated to match: this doc, [node_representation_arrangement_plan](../implementation_strategy/2026-06-18_node_representation_arrangement_plan.md) and [tearout_composability_plan](../../archive_docs/2026-07-04_completed_plans/2026-06-19_tearout_composability_plan.md) (rename banners, left the historical prose as-is), [unified_document_host_plan](../implementation_strategy/2026-06-17_unified_document_host_plan.md) (rename banner), [object_card_plan](../../archive_docs/2026-09-02_retired_plans/2026-06-21_object_card_plan.md), [tearout_gestures_plan](../implementation_strategy/2026-06-24_tearout_gestures_plan.md), [graphlet_wiring_plan](../../archive_docs/2026-07-04_completed_plans/2026-06-25_graphlet_wiring_plan.md), [node_body_face_model_plan](../implementation_strategy/2026-06-23_node_body_face_model_plan.md), [native_surface_compositing_plan](../../archive_docs/2026-07-03_completed_plans/2026-06-19_native_surface_compositing_plan.md), [gloss_scene_to_dom_migration_plan](../../archive_docs/2026-07-04_completed_plans/2026-07-01_gloss_scene_to_dom_migration_plan.md), and the canonical [interaction_model_spine](../technical_architecture/2026-06-18_interaction_model_spine.md) (inline fixes, current doc).

## 7. Open questions

- **OQ-1 Cycling gesture**: swipe / arrow keys / tabs on the card; and the deck order for a given selection.
- **OQ-2 Embeddability contract**: what a card needs from its host (anchor rect vs pane slot, drain path, dismissal rules) so canvas and roster render the same card.
- **OQ-3 Snapshot deposit**: cadence and storage cost of per-node pixel snapshots (they persist like sprites, but sprites are user-chosen and rare; snapshots accrue per visited node). **Partly resolved 2026-07-04**: a per-session byte cap now bounds the idle-refresh pass specifically (see §5 item 4); still open is any cross-session retention policy (the cap resets to 0 every launch, so it bounds one session's idle-refresh growth, not the graph's total accumulated thumbnail storage over time) and whether a live UI control is worth building before more settings pile up unexposed.
