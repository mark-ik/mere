# Settings / Config Consolidation via the pelt Settings lane

> **SUPERSEDED 2026-08-06** by the
> [configuration_ownership_settings_projection_plan](../../mere_docs/implementation_strategy/2026-08-06_configuration_ownership_settings_projection_plan.md).
> The P1-P3 work below landed in meerkat, which was deleted at the Turnstone
> founding; the settings-as-nodes model (P1) was ruled out by the pane-taxonomy
> revision (turnstone/src/apparatus_pane.rs header). The durable pieces (the
> pelt `SettingsRef` lane, the deep-config-vs-quick-gesture line, the
> diagnostics split) are carried forward in the successor's "Carried forward"
> section. Historical record only.

**Date**: 2026-06-21
**Status**: P1 landed (the host settings-lane render arm, 2026-06-21). P2 core landed (the
overlay + apparatus settings sections retired into `pelt/*`, tab cap on `pelt/appearance`,
2026-06-21); the `pelt/orrery` scene-toggles page is the remaining P2 piece. P3 next (the
`node:<id>` facets provider). Supersedes the loose "migrate settings to pelt" framing and
absorbs the node-facets / node-editor work as the `node:` provider.
**Code**: `crates/meerkat/` (apparatus, settings overlay, context menu, the tile-render arm),
`genet/ports/pelt-core` (the `ContentSource::Settings` contract, already present),
`crates/domain/apparatus`.
**Siblings**:
[node_editor_customization_probe](../research/2026-06-21_node_editor_customization_probe.md)
(the node editor + sprite + hitbox axis, hosted by the `node:` provider here),
[node_representation_arrangement_plan](2026-06-18_node_representation_arrangement_plan.md)
(the per-node form/size config that migrates into the `node:` facets pages).

---

## The problem (why consolidate)

"Configure something" is spread across redundant, overlapping surfaces that accreted one at a
time. The audit below confirms it is real redundancy, not seeming:

- The **apparatus** pane is two things wearing one coat: settings (Theme, Engines, Physics)
  **and** diagnostics (Overview, UX Events, Actors, Accessibility, Tracing, Registry, Probes).
  It is even self-labeled "Apparatus (diagnostics + settings)" (command.rs:196).
- The **settings overlay** is a *second* global settings surface holding exactly one knob
  (the active-tab cap), centered-modal, unrelated to the apparatus.
- The **context menu** keeps growing per-node + per-scene config (representation tile/shape,
  size-by-degree, engine pin, layout strategy, scope lens, mirror-tiles) with no page to own it.
- The **Inspector** utility pane ("selected object") is a third per-node surface.
- The **pelt Settings lane** (`ContentSource::Settings(SettingsRef)`, pelt-core/tile.rs:105)
  is designed, multi-provider, and **unused**.

So there are two global-settings surfaces, per-node config with no home, settings tangled with
diagnostics, and the one lane built to hold all of it sitting idle. This is the exact shape
DOC_POLICY §2/§3 calls out (eliminate redundancy; replace, do not half-migrate).

## The model (the lane is already designed)

pelt-core's `ContentSource` is the lane set a tile is driven by:
`Document` (html / genet), `ExternalTexture` (external surface / scry), `Settings`. The
**Settings lane is multi-provider**: a `SettingsRef` is a namespaced string
(`"pelt/appearance"`, `"node:<id>/engine"`, `"moot:<id>/permissions"`); the provider for that
namespace resolves it to a permission-gated page. The tile contract names the lane and carries
the opaque ref; the page schema, the index spine, and the permission model are the provider's
concern. **Consolidation = make this lane THE config surface and retire the others into it**,
not add a fourth.

Three providers cover everything in the audit:

- **`pelt`** — app/global config (the apparatus settings sections + the settings overlay).
- **`node:<id>`** — per-node facets (the context-menu per-node config + the Inspector). This
  is the node facets menu, and the home for the node-editor / sprite / hitbox-axis work.
- **`moot:<id>`** — community/federation config (future).

## Surface audit → destination

| today | kind | destination |
|---|---|---|
| Theme buttons (apparatus) | global config | `pelt/appearance` |
| Tab cap (settings overlay) | global config | `pelt/appearance` (or `pelt/tiles`) |
| Engine toggles (apparatus) | global config | `pelt/engines` |
| Physics damping (apparatus) | global config | `pelt/physics` |
| Overview / UX / Actors / a11y / Tracing / Registry / Probes (apparatus) | **diagnostics** | split out — `pelt/diagnostics` page **or** a debug surface (read-only, not settings) |
| Representation tile/shape (context menu) | per-node config | `node:<id>/appearance` |
| Size + size-by-degree (context menu / node-rep P0) | per-node + scene config | `node:<id>/appearance` (per-node) + `pelt/orrery` (the scene toggle) |
| Engine pin / Auto (context menu) | per-node config | `node:<id>/engine` |
| Inspector pane (selected object) | per-node inspection | `node:<id>/info` (subsume or coexist) |
| Layout strategy / scope lens / mirror-tiles (context menu) | per-scene config | `pelt/orrery` (scene settings) |
| Quick gestures (open tile, relate, add node/tile/field/tag, copy link) | gesture | **stay** in the context menu / commands |

The line held: **deep config moves to the lane; quick gestures stay fast.** Consolidate config,
do not gut the menu.

## Phases (done-conditions, not dates)

- **P0 — Audit.** Done (this doc's surface map).
- **P1 — Host settings-lane plumbing. Settings tiles are NODES** (Mark, 2026-06-21 — the cleaner
  model; the earlier "non-node workbench slot" framing is dropped). A settings page is a node with
  a settings-scheme url (`settings://pelt/appearance`), synthesized + ephemeral like `mere://welcome`
  (not written to the persisted graph; filtered from the orrery so it does not clutter the map).
  It opens via the **existing** `open_tile(member)` path — no workbench slot change. The pieces:
  (a) the `settings://` scheme routes to the settings lane and is non-fetchable/synthesized (like
  `mere://`); (b) the per-node content dispatch (render.rs `tile_for` / the workbench content loop,
  where scry-vs-genet is already decided per node) gains a third arm: a settings node →
  `ContentSource::Settings(ref)`, the ref being the node's url path; (c) the render arm resolves the
  ref through the **provider seam** (`settings_lane.rs`, landed: `settings_index` + `settings_page`)
  and paints the page's controls + the **index spine** at the tile body rect, reusing the list-pane
  rendering (a per-tile settings pane added to the shell document, like the fixed list-panes but
  keyed to the tile rect); (d) `>settings` just navigates to `settings://pelt/appearance`. The
  controls reuse the apparatus drain keys, so they drive the host unchanged. Done when a settings
  node opens as a tile, renders its page interactively, and the spine switches pages. **Status: done
  (2026-06-21).** All four pieces landed: (a) `settings://` is non-fetchable via the existing
  `fetch::is_fetchable` scheme gate (no change needed); (b) the content loop's third arm records
  `settings://` tiles (no actor, no card); (c) `settings_pane_view.rs` renders a dynamic, two-column
  per-tile pane (index spine + page body) folded into the shell document, resolved through the seam
  by `snapshot_settings_panes`; (d) `>settings` mints/reuses a `settings://pelt/appearance` node and
  opens it as a tile. Also landed: settings nodes are filtered off the orrery map, the tab shows a
  friendly title (`Settings: Appearance`), and `>settings` dedups by url. Deferred polish tracked in
  Progress (true ephemerality/persistence-exclusion, wheel-scroll in the body, theme-aware spine accent).
- **P2 — The `pelt` provider + retire the old surfaces.** Pages: appearance (theme + tab cap),
  engines, physics, orrery (the scene toggles). Migrate the apparatus settings sections + the
  settings overlay in, then **delete** the settings overlay and the apparatus settings sections
  (not keep them parallel). Done when global config lives only in `pelt/*` pages. **Status: core
  done (2026-06-21).** The settings overlay is deleted (view fn + `Chrome::{settings_open,
  open_settings, close_settings}` + the overlay CSS + the Escape handler + the overlay test), its
  one knob (the active-tab cap) migrated to `pelt/appearance` (theme + tab cap, drained via
  `tiles:cap:up/down` → `inc/dec_tab_cap` → the existing `sync_settings` persist path). The
  apparatus settings sections (Theme/Engines/Physics) are deleted from `apparatus_items`, so the
  apparatus is read-only diagnostics now (relabeled "Apparatus (diagnostics)"); theme/engines/
  physics live only in `pelt/*`. **P2b (the `pelt/orrery` page) — page landed + verified
  (2026-06-21).** Added a `pelt/orrery` page: the focused orrery's Layout picker (Force-directed +
  the registered `platen::ORRERY_LAYOUT_STRATEGIES`, active marked) plus a Map section (Size by
  degree, Mirror open tiles), driving the same three scene-toggle methods the context menu uses
  (extracted to `set_orrery_layout` / `toggle_orrery_size_by_degree` / `toggle_mirror_tiles`, so
  page + menu are one source of truth). Verified live: the page renders, the layout picker
  re-lays-out the orrery (Force-directed → Phyllotaxis), and the toggles reflect real persisted
  state. **Remaining for P2b:** the menu **de-duplication** — those toggles now live in both the
  context menu and the orrery page. Per the plan ("deep config moves to the lane; quick gestures
  stay"), the deep config should leave the menu, but which of {layout picker, size-by-degree,
  radial-on-selection, mirror} are "quick gestures" that stay is a graph-canvas UX call held for
  Mark. The selection-contextual scope lens (isolate / show-all) stays a menu gesture regardless.
- **P3 — The `node:<id>` facets provider (the node-editor home).** Pages: appearance
  (representation, size, color, and the sprite editor from the node-editor probe), engine (pin),
  info (the Inspector's selected-object view). Migrate the per-node context-menu config; the
  context menu keeps quick toggles + gestures. The hitbox axis (grow-the-collider, then
  sprite→collider) lands here. Done when a node's facets open as a settings tile and the
  per-node context config is gone from the menu (toggles excepted). **Status: first slice done +
  headed-verified (2026-06-21).** The provider plugs into the existing settings-lane render arm:
  `settings_index` / `settings_page` route the `node:<id>` namespace (prefix-matched, separate from
  the `pelt` arms) to a new `settings_node.rs` module (`node_settings_page` parses the member id,
  resolves the node, builds the page). The **info** page (read-only node facts) is built; the
  spine lists only it for now. A `Command::OpenNodeSettings` (palette: "Node settings (focused
  node)", verb `node_settings`) opens the focused node's facets tile via `open_settings_tile("node:
  <uuid>/info")`. Verified live: selecting the globe node + running the command opened a
  "Settings: Info" facets tile showing the node's Title/URL/Representation/Size. **Remaining:** the
  **engine** page (migrate the engine-pin picker) + **appearance** page (representation/size/sprite),
  which is where the command-registry plan's P2-rest engine/representation pickers land; then remove
  the migrated per-node config from the context menu.
- **P4 — Diagnostics split + `moot` provider.** The apparatus's observability sections move to a
  `pelt/diagnostics` page or a dedicated debug surface (read-only inspection is not config). The
  `moot:<id>` provider slots in for community/federation settings.

## Findings (audit, code-verified 2026-06-21)

1. The apparatus (apparatus.rs:80-247) conflates 3 settings sections (Theme/Engines/Physics,
   interactive) with 8 diagnostics sections (read-only). Self-labeled "diagnostics + settings".
2. The settings overlay (views.rs:212-235) is a second, single-knob settings surface (tab cap),
   opened by `Command::OpenSettings` ("settings"); independent of the apparatus.
3. Per-node + per-scene config lives in the context menu as `ContextAction` variants
   (`SetRepresentation`, `ToggleSizeByDegree`, `PinEngine`/`AutoEngine`, `SetLayoutStrategy`,
   `IsolateSelection`/`ShowAllNodes`, `MirrorTiles`) with no settings page to own it.
4. The pelt `ContentSource::Settings(SettingsRef)` lane is designed multi-provider and unused —
   the consolidation is host-side build-out, not a new contract.
5. The `node:<id>` provider unifies the node-editor probe's "facets menu" with the scattered
   per-node config: they are the same surface, this names it.

## Progress

- 2026-06-21: Plan created from the settings-redundancy thread with Mark. Audited the config
  surfaces inline (apparatus, settings overlay, context-menu config, the command verbs, the
  Inspector) and mapped each to a `pelt` / `node:` / `moot` settings-lane provider page, holding
  the line that deep config moves to the lane while quick gestures stay in the menu. Confirmed
  in code that the pelt Settings lane is already in the contract (pelt-core/tile.rs:105,
  multi-provider via `SettingsRef`), so the work is host-side: the tile-render arm, the provider
  protocol + index spine, the `pelt` provider, then the `node:` facets provider (which carries
  the node-editor / sprite / hitbox-axis work), then the diagnostics split + `moot`. Supersedes
  the loose "settings → pelt" framing.
- 2026-06-21: **P1 render arm landed.** Built the host-side render arm that turns a `settings://`
  node into an interactive settings tile, all of meerkat green (`cargo test -p meerkat`: lib 64,
  bin 98, doc 0; build clean, no new warnings). Pieces:
  - New module `crates/meerkat/src/settings_pane_view.rs` (`SettingsPane` / `SettingsPanesState` +
    `settings_panes_view`): one lensed shell-document subtree emitting one absolutely positioned
    **two-column** pane (index spine + page body) per open settings tile. Unlike the four fixed
    `ShellListPane` slots, this is dynamic (N tiles). Body controls reuse the apparatus author
    classes (`app-btn` etc.), so only the column geometry + theme `panel_bg` are host-supplied.
  - `window_view.rs`: `ShellState.settings: SettingsPanesState`, the lensed subtree in `shell_view`
    (present only when at least one tile is open), `WindowView::{set_settings_panes,
    settings_panes_open, take_settings_pane_keys, take_settings_pane_nav}`, and a `settings_rects`
    hit field.
  - `render.rs`: the content loop's third arm (record `settings://` tiles, skip actor/card); a
    per-frame `snapshot_settings_panes`; settings nodes filtered off the orrery card snapshot; a
    friendly tab title via `settings_tab_title`.
  - `settings_lane.rs` (now wired, `#![allow(dead_code)]` removed): `snapshot_settings_panes`
    (resolves each tile through `settings_page` + `settings_index`), `settings_tab_title`,
    `panel_bg_rgb`, and `open_settings_tile` (mint/reuse the node + open the tile).
  - `input.rs`: settings-body presses route to `chrome_click` (added `settings_pane_at` to the
    chrome-routed check); `drain_list_pane_activations` now also drains the settings panes —
    page-control keys via a shared `apply_pelt_activation` (extracted from the apparatus match) and
    spine navigations via `orrery_mut().navigate_member` (retargets the tile's node url, which the
    next frame re-resolves).
  - `lib.rs` + `command_drain.rs`: `>settings` (`Command::OpenSettings`) is now a host intent that
    calls `open_settings_tile("pelt/appearance")` instead of the chrome overlay. The overlay code
    stays (orphaned) until P2 deletes it; its direct-call test is unaffected.
  - Tests: three deterministic view tests in `settings_pane_view.rs` (spine click → page nav,
    page-control click → activation key, tab-title formatting), via the `ViewPane` harness and
    `accumulate_origins` for absolute hit coords.
  - **Deferred (tracked):** (1) true ephemerality — a settings node is a real graph node, so it can
    be persisted on snapshot; exclude `settings://` at the persistence/snapshot boundary for genuine
    ephemerality (it is already filtered off the orrery map). (2) wheel-scroll inside the settings
    body (the `pelt/*` pages are short, so nothing clips yet). (3) theme-aware spine accent (the
    active page uses `app-btn-active`; a dedicated spine style is polish).
- 2026-06-21: **Headed verification done + a render-order bug found and fixed.** Drove the live host
  (`scry-shots/drive-settings*.ps1`): `>settings` opens the tile (tab "Settings: Appearance", the
  two-column spine + theme body), the spine switches Appearance/Engines/Physics, a body control flips
  the theme app-wide (Dark re-themed the whole UI live, settings pane included), and the settings node
  stays off the orrery map. Shots in `scry-shots/set-*.png`.
  - **Bug found:** the settings body lagged the spine/tab by one click (click Physics → spine+tab show
    Physics, body still shows Engines). **Cause:** `snapshot_settings_panes` runs in the workbench
    block (it needs the tile rects the pelt surface reports), which is *after* the `chrome_scene`
    (shell document) render — so the settings-pane DOM mutations were only drained into the shell
    render on the *next* frame. The roster/list panes avoid this because their rects come from the
    early frame-leaf computation, so they are set before the shell render.
  - **Fix:** moved the `chrome_scene` build (and `chrome_sheet` + `external_texture_placements`) down
    to *after* the workbench block + `snapshot_settings_panes`, so the one shell render reflects this
    frame's settings panes. NLL ends `chrome_sheet`'s `self.shared` borrow at the scene build, so it
    coexists with the workbench block's `&mut self`. All meerkat tests still green (lib 64, bin 98).
  - **Structural learning:** any folded shell-document pane positioned at a *workbench tile* rect is
    inherently downstream of the tile-surface layout, so the shell render must run after it. Worth
    remembering for the `node:<id>` facets provider (P3), which will also open as workbench tiles.
- 2026-06-21: **P2 core landed + headed-verified — the two redundant global-settings surfaces
  retired into `pelt/*`.** All meerkat tests green (lib 64, bin 98).
  - **Settings overlay deleted** (it was already orphaned after P1 repointed `>settings` to the
    tile): removed `settings_overlay` (views.rs) + its render branch, `Chrome::{settings_open,
    open_settings, close_settings}` (lib.rs), the overlay CSS (`.settings*` / `.set-*` in main.rs),
    the Escape-closes-overlay handler (input.rs), and the overlay test. Its one knob, the active-tab
    cap, moved to the `pelt/appearance` page (theme + a "Tabs" section), drained via `tiles:cap:up` /
    `tiles:cap:down` through `apply_pelt_activation` → `inc/dec_tab_cap`, which the existing per-frame
    `sync_settings` applies to the actor pool + persists. The overlay test became a `tab_cap_edits_
    within_bounds` unit test (floors at 1, ceils at 64).
  - **Apparatus settings sections retired:** `apparatus_items` dropped Theme/Engines/Physics and is
    diagnostics-only now (signature `(system_rows, obs)`); the shared section builders stay (the lane
    uses them). Pane relabeled "Apparatus (diagnostics)". This also does part of P4 early (read-only
    inspection, not config).
  - **Verified live** (`scry-shots/set-1/7/8`): `pelt/appearance` shows theme + the tab-cap control;
    the cap edits live (12 → 14) and drives the host (the apparatus Overview reflects "Tab cap: 14");
    the apparatus pane shows only diagnostics (no settings sections); `>settings` opens the tile (no
    overlay anywhere).
  - **Deferred:** the `pelt/orrery` scene-toggles page (P2b) — net-new, needs a design pass (those
    toggles live in the context menu today). Plus the P1 deferrals (true ephemerality, body
    wheel-scroll, theme-aware spine accent).
- 2026-06-21: **P2b — `pelt/orrery` page landed + headed-verified.** Added the Orrery page (4th pelt
  page): a Layout picker (Force-directed + `platen::ORRERY_LAYOUT_STRATEGIES`, active marked) and a
  Map section (Size by degree, Mirror open tiles). Extracted the three scene-toggle bodies from the
  context-menu drain into shared `WindowCtx` methods (`set_orrery_layout`,
  `toggle_orrery_size_by_degree`, `toggle_mirror_tiles`); the menu arms and the page's drain
  (`orrery:layout:<id>` / `orrery:sizebydegree` / `orrery:mirror` in `apply_pelt_activation`) both
  call them, so page and menu stay one source of truth. All meerkat tests green (lib 64, bin 98).
  Verified live (`scry-shots/set-9/10/11`): the page renders, the toggles reflect real persisted
  state (Size by degree ✓), and picking Phyllotaxis re-laid-out the orrery. **Open decision (held
  for Mark):** the menu still carries these toggles too, so it is now parallel with the page. The
  de-dup (which toggles leave the menu vs stay as quick gestures) is a graph-canvas UX call.
- 2026-06-21: **P3 first slice — the `node:<id>` facets provider landed + headed-verified.** The
  provider reuses the existing settings-lane render arm: `settings_index` + `settings_page` route the
  `node:<id>` namespace (prefix-matched, additive arms separate from the `pelt` arms Mark is editing)
  to a new `settings_node.rs` (`node_settings_page` parses the member uuid from `node:<uuid>`, resolves
  the node in the focused graph, builds the page). Built the **info** page (read-only: Title / URL /
  Representation / Size); the spine lists only it for now. Added `Command::OpenNodeSettings` (palette
  "Node settings (focused node)", verb `node_settings`) → `open_settings_tile("node:<uuid>/info")` for
  the focused node. meerkat green (lib 69, bin 101). Verified live (`scry-shots/nf-4`): selecting the
  globe node + running the command opened a "Settings: Info" facets tile showing the node's facts
  (Representation: Tile, Size: 52 px). Chosen as the next step (over the command-registry plan's
  P2-rest pickers) because engine/representation are *per-node* facets whose home is exactly this
  provider.
- 2026-06-22: **P3 — engine + appearance facets pages landed + headed-verified.** Added the
  **appearance** page (the representation picker: Show as tile / Show as shape) and the **engine**
  page (the engine-pin picker: Auto + the pickable web engines, with a non-web note) to
  `settings_node.rs`; `node_settings_index` now lists Info / Appearance / Engine. The page controls
  drain `nodefacet:<id>:<action>` keys, applied by a new `apply_node_facet_key` directly to the
  subject node — `engine_pins.insert`/`remove` and `set_node_representation`, the *same underlying
  writes* the context-menu pickers use (no `&'static`/ContextAction round-trip). `input.rs` routes
  `nodefacet:` keys there, pelt keys to `apply_pelt_activation`. meerkat green (lib 69, bin 101).
  Verified live (`scry-shots/nf-13/15/16`): the facets spine shows Info / Appearance / Engine; the
  Appearance page renders Show-as-tile/shape and registers clicks; the Engine page shows
  "Auto (default routing)" active + the non-web note (correct for `mere://welcome`). The
  representation/engine *application* reuses the menu's tested writes. This is also the home for the
  command-registry plan's P2-rest engine/representation pickers. **Next:** the appearance page's
  size/sprite controls + the sprite→hitbox axis (node-editor probe); then strip the migrated
  per-node config (engine / representation pickers) from the context menu (toggles excepted).
  **Housekeeping:** the headed driving accidentally minted a couple of stray nodes (`info`,
  `appearance`) by typing into the omnibar before a field was focused — session cruft in the dev
  session graph, harmless, deletable via the menu/Trail if it bothers.
- 2026-06-22: **P3 — menu strip + appearance size control landed + headed-verified.** (1) **Menu
  strip (de-dup):** removed the inline engine + representation pickers from the single-selection
  context menu (deleted `engine_picker_items` / `representation_picker_items`) and replaced them with
  a "Node settings…" entry → `ContextAction::OpenNodeFacets` → `open_node_facets` →
  `open_settings_tile("node:<id>/info")`. The per-node config now lives only on the facets tile; the
  menu keeps gestures (Open tile / Resize / Add tag) + the selection scene-toggles (radial / size-by-
  degree / isolate). Verified live (`scry-shots/strip-2`): the selection menu shows "Node settings…"
  with no engine/representation pickers. (2) **Appearance size control:** added a Size section to
  `node:<id>/appearance` (readout + − / + tier steppers) draining `nodefacet:<id>:size:up|down` →
  `step_node_size_tier`. Verified live (`scry-shots/size-2`): the Size section renders and the stepper
  changes the node's size tier (readout updated to "24 px (tier 0)"). meerkat green (lib 69, bin 101).
  **Note:** `ContextAction::PinEngine`/`AutoEngine`/`SetRepresentation` are now orphaned (the facets
  apply directly via `engine_pins`/`set_node_representation`, and the `&'static` engine-id stops them
  being reconstructed from a runtime key) — harmless dead variants, a follow-up cleanup can remove
  them + their drain arms. **Remaining for P3:** the **sprite editor + sprite→hitbox axis** (the
  node-editor probe — a big, still-unsettled feature; its own phase).
