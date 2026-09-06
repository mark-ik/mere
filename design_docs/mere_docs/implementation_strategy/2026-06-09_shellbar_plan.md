# Shellbar Plan (F2)

**Date**: 2026-06-09
**Status**: In progress.
**Related**: [graph roster + frame taxonomy §4](../design/2026-06-07_graph_roster_and_frame_taxonomy.md#4-shellbar-2026-06-09), [frame tree plan](../../archive_docs/2026-06-09_completed_plans/2026-06-08_frame_tree_in_meerkat_plan.md), `crates/meerkat/` *(historical citation)* <!-- doc-audit: historical-path -->, `crates/system/pandect/src/settings_store.rs`

Wire a docked chrome strip (the shellbar) that gives mouse users access to the
pane toggles currently keybind-only. F1 (frame tree in meerkat) is complete;
this is the first F2 slice.

---

## Design summary

See §4 of the taxonomy doc for the full rationale. The key decision: the
shellbar is **outside the frame tree**, not a leaf. Its edge (Left / Right / Top
/ Bottom) is a window preference; moving it is a preference change, not a frame
operation.

---

## Phases

### F2.1 — Strip + pane toggles (this slice)

1. **`ShellbarEdge` enum** in `session-runtime/settings_store.rs` (alongside
   `PersistedSettings`); add `shellbar_edge` field to `PersistedSettings`.
2. **New `Command` variants**: `ToggleRoster`, `ToggleGloss`, `ToggleApparatus`
   in `meerkat/src/command.rs`.
3. **`ShellbarPaneStates`** (small struct: `workbench/roster/gloss/apparatus/comms: bool`)
   + `shellbar_edge: ShellbarEdge` added to `Chrome` in `meerkat/src/lib.rs`.
   App sets both before each runner update so the view reflects live frame state.
4. **`meerkat/src/shellbar.rs`** — geometry helpers:
   + `SHELLBAR_THICKNESS: f32 = 48.0`
   + `shellbar_rect(edge, w, h, toolbar_h) -> [f32; 4]`
   + `band_after_shellbar(edge, w, h, toolbar_h) -> [f32; 4]`
5. **`meerkat/src/main.rs`** — add `shellbar_edge: ShellbarEdge` to `App`;
   load from `PersistedSettings`; save on change.
6. **`meerkat/src/render.rs`** — before the chrome scene: sync
   `ShellbarPaneStates` + `shellbar_edge` into Chrome; set the `.shellbar` div's
   inline style; replace the hard `band` with `band_after_shellbar(...)`.
7. **`meerkat/src/main.rs` / `chrome_sheet()`** — add `.shellbar` and
   `.shellbar-btn` / `.shellbar-btn-active` CSS.
8. **`meerkat/src/views.rs`** — add `.shellbar` div with five `on_click`
   buttons (Workbench, Roster, Gloss, Apparatus, Comms) in `chrome_view()`.
9. **`meerkat/src/frame_ops.rs`** — handle `ToggleRoster`, `ToggleGloss`,
   `ToggleApparatus` in `drain_pending_command`.

### F2.2 — Move shellbar

Right-click shellbar → context menu: "Move to left / right / top / bottom."
Sets `App::shellbar_edge`, persists it, triggers a redraw.

### F2.3 — Session/graph switcher + persona chip in shellbar (later)

Bottom anchor of the shellbar strip. The graph-switcher half is **MG4** of the
[multi-graph activation plan](2026-06-09_multi_graph_activation_plan.md) (it needs
the session registry + switch from MG1-MG3 first); the persona chip stays a
reserved slot gated on multi-persona.

---

## Risks

+ The chrome DOM's absolute-positioned elements are all currently within the
  toolbar band or floating overlays. The shellbar is the first element
  positioned in the content band. Ensure its `top` accounts for `toolbar_h`, not 0.
+ The content band (`band`) must be shrunk before being passed to
  `frame_view::leaf_rects`. Any path that reads `band` directly (orrery card
  anchoring, divider placement) must use the adjusted rect.

---

## Progress

+ 2026-06-09: Plan written. Design decision (outside frame tree, docked strip)
  confirmed by Mark. Taxonomy doc §4 added.
+ 2026-06-09: F2.1 implemented and confirmed green (67 meerkat + 4 shellbar geometry tests pass).
+ 2026-06-09: F2.2 implemented and confirmed green. Right-click the shellbar strip opens a
  context menu with all four edges (current edge marked ✓); selection redocks the shellbar,
  persists the preference, and redraws.
+ 2026-06-09: R1 (roster node-list facets) implemented. `RosterRow` now carries `title`
  (fixed: node.title → cached_host → url), `url`, `content_type`, and `tags`. The DOM renders
  a URL subtitle and a facets strip (content-type chip + tag chips) when non-empty. Tests: 94
  total meerkat tests green (4 new roster-facets tests added).
+ 2026-06-09: R2 (edge detail for focused row) implemented. `RosterRow` gains `edges:
  Vec<EdgeRow>` populated only for the focused row. Each `EdgeRow` carries direction (→/←),
  relation kind label ("Hyperlink", "Traversal", etc.), and the other node's title. A `.roster-edges`
  section renders beneath the facets strip when non-empty. `relation_kind_label()` covers all
  six `RelationKind` families. Tests: 95 total green.
+ 2026-06-09: R3 (sort/filter by content type) implemented. `roster_rows()` now sorts by
  `(content_bucket, title)` and stamps `section_header` on the first row of each bucket.
  `content_bucket()` maps MIME → shape → (order, label): Documents (0), Feeds (1), Menus (2),
  Unknown (3). Orrery shape wiring was already in place. `RosterRow` gains `section_header:
  Option<String>`; `build_roster_dom` renders `.roster-section` headers when set. Tests: 96 total green.
