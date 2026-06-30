# Graph Object Roster Detail Cards Plan

**Date**: 2026-06-29
**Status**: In progress. The roster table and in-roster card slice landed on
2026-06-29. Active-tab styling, high-zoom roster box-sizing, endpoint-bundle
visibility persistence, connections-swatch relation-cell routing, Graphlet Card
member wording, canvas relation-cell overlay/picking, and relation-cell visibility
persistence have landed. Deeper graphlet selector/family editing and true
parallel edge instances remain.
**Code**: `crates/meerkat/src/roster.rs`, `roster_data.rs`, `roster_view.rs`,
`roster_view_parts.rs`, `swatch.rs`, `render/connections.rs`, `input/panes.rs`,
`input/editing.rs`, `input/mouse_dispatch/press.rs`, `session_ops/view_intent.rs`;
`crates/orrery/orrery/src/edge_cells.rs`, `frame.rs`, `input.rs`, and
`selection.rs` for canvas relation-cell display, picking, and visibility helpers.

This plan turns the Roster into the first stateful control layer for graph
objects. The canvas still owns spatial selection, snapshots, object cards, and
connections swatches. The Roster owns graph-element tables and durable inspection:
nodes, links, graphlets, fields, facets, and the cards that edit those subjects.

Sibling / converging docs:

- [graph_roster_and_frame_taxonomy](../design/2026-06-07_graph_roster_and_frame_taxonomy.md)
  names the Roster as the graph-object list and frame taxonomic surface.
- [object_card_plan](2026-06-21_object_card_plan.md) owns the in-canvas
  focus-card slot. This plan deliberately keeps roster cards inside the Roster pane.
- [graphlet_wiring_plan](2026-06-25_graphlet_wiring_plan.md) owns graphlet
  derivation, drift tracking, reconciliation, and scoped windows.
- [context_submenus_plan](2026-06-25_context_submenus_plan.md) owns the quick
  context-menu grammar that roster cards reuse for relation actions.
- [swatch_primitive_plan](2026-06-27_swatch_primitive_plan.md) owns per-cell
  canvas hit-testing, cells-as-edges, and the eventual visibility stack.

---

## Plan

### R1 - Tabbed roster table

Replace the one-shape roster list with four element tabs:

- **Nodes**: title, URL/domain, content type, tags, selected/open state.
- **Links**: one row per relation cell, grouped visually by endpoints, with
  direction, family, kind, source/label, and target.
- **Graphlets**: id, kind, binding, member count, selectors, and drift status.
- **Fields**: field/facet rows with rule, extent, visibility, and strength columns.

Done when selecting a tab rebuilds the row collection from the live graph state,
without overloading node rows to mean every graph object.

**Status**: Done 2026-06-29.

### R2 - Roster subject and detail region

Add a subject model for the row that is selected:

`Node`, `LinkBundle`, `RelationCell`, `Graphlet`, `Field`, and `Facet`.

The Roster pane gets an in-roster detail/card region below the table. Selecting a
row sets the subject and renders the matching card. Inline-only expansion stops
being the whole model. The canvas focus-card slot remains unchanged for snapshot
preview, object-card, unavailable snapshot, and connections swatch.

**Status**: Done 2026-06-29.

### R3 - Link Card

The first payoff card is link management. A link card is addressed by endpoint
bundle first, then by relation cell when the caller has that specificity.

V0 contents:

- Header: source -> target, with both titles and URLs where available.
- Body: all relation cells between the endpoints, grouped by `EdgeFamily`.
- Actions: `Relate as...` using the existing `SemanticSubKind` vocabulary,
  retract selected editable semantic relation, hide/show the selected endpoint
  bundle when view visibility state exists.

Done when the Links tab can open a card for an endpoint bundle, show semantic,
traversal, and provenance cells distinctly, add a semantic relation, retract an
editable relation, and leave both endpoint nodes intact.

**Status**: Done 2026-06-29 for the roster/card path. Canvas relation-cell
overlay/picking landed 2026-06-30; gyre topology and physics springs remain
endpoint-pair scoped.

### R4 - Graphlet Card

The second payoff card surfaces graphlet state where users already inspect graph
substructure.

V0 contents:

- Kind, binding, members, selectors, and drift-tracking status.
- For `Linked` graphlets, a dry drift preview from re-derivation.
- Actions for the graphlet control grammar: reconcile/apply, keep as session,
  fork/branch, and open scoped window, using existing graphlet APIs where present.

Done when opening a graphlet row shows current binding and drift state, previews
added/removed members without mutating graph truth, and can open the scoped window
through the same command path as graphlet actions elsewhere.

**Status**: Done 2026-06-29 for the card/read/action surface. Selector/family
editing remains with graphlet/swatch follow-up work.

### R5 - Fields and facets

The Fields tab should not be only a dump of field records. It should become the
surface where graph-level rule objects are inspectable:

- Fields: rule surface, spatial extent, visibility state, strength.
- Facets: subject, tags/classes, source, and whether the facet is an object rule,
  styling rule, script hook, or template hook.

Done when fields/facets have their own table rows and cards rather than being
hidden behind node-only affordances.

**Status**: First pass done 2026-06-29.

### R6 - Runtime polish and future edge depth

The Roster is now the right place for link/graphlet/field inspection, but the
edge-depth story is not finished.

Open:

- Gyre topology and physics springs still collapse relations to endpoint pairs.
  Orrery now overlays and picks fanned relation cells without changing that
  topology.
- Relation-cell visibility is display/session scoped and keyed by current
  `(source, target, RelationKind)` cells. Bundle hide/show is still a wrapper
  over the current cells between two endpoints.
- Connections swatch P4 emits one fanned DOM relation cell per relation kind,
  filters hidden cells, and routes clicks to the same `RelationCell` card.
- Keep kernel storage untouched in this slice. True parallel edge instances stay
  out of scope.
- Deeper graphlet selector/family editing remains with graphlet/swatch follow-up
  work.

---

## Findings

- **Roster first was the right sequence.** `graph.relations()` already exposes
  relation cells well enough for management tables. Waiting for canvas edge
  hit-testing would have delayed useful edge management without changing the
  underlying control grammar.
- **Roster cards and focus cards are different surfaces.** The focus-card slot is
  transient and spatial. The Roster detail region is stateful and tabular. Keeping
  them separate avoids making snapshots, object-card, and the connections swatch
  fight with link/graphlet inspection.
- **Relation vocabulary should stay as-is.** The code should keep using
  `EdgeFamily`, `RelationKind`, `RelationSelector`, and `SemanticSubKind`. Product
  text can say "relation family" where it reads better, but this slice did not add
  a new "edge kind" model term.
- **Command drains are still the right interaction seam.** Roster cards can queue
  the same kind of actions context menus queue, and `input/panes.rs` can drain them
  into the shell/orrery/graphlet APIs. That kept link-card actions from inventing a
  second command bus.
- **The 600 LOC ceiling forced useful splits.** `roster_data.rs` owns row/card data
  building, `roster_view_parts.rs` owns repeated card/table view pieces, and
  `roster.rs` stays the state/action facade.
- **Runtime styling needs a safer tab state.** The table/card logic worked before
  the active-tab visual did. The current runtime build favors readable rows over a
  misleading selected-tab marker until that styling path is fixed.
- **The target path is not the app name.** Current headed builds land under
  `C:\t\graphshell-target` because the local environment sets `CARGO_TARGET_DIR`;
  the crate package, bin target, and native window title are still `meerkat`.

---

## Working Map - 2026-06-30

### Current state

- **Roster is now the durable graph-object control surface.** Nodes, links,
  graphlets, fields, and facets have table rows, selected subjects, and in-roster
  cards. Canvas focus cards remain transient spatial cards.
- **Link depth is display-real for current relation cells.** Roster rows, Link
  Card rows, canvas overlay/picking, selected-cell redraw, connections swatch
  routing, and session visibility all address `(source, target, RelationKind)`.
- **The underlying graph is still not a true multigraph at the UI boundary.** A
  relation cell cannot distinguish two independent `Cites` facts between the same
  pair. That remains a storage/model lane, not a roster-card lane.
- **Visibility is current-cell view state.** Hidden cells persist through the
  view-intent sidecar and filter display paths. There is not yet a graph-default
  visibility layer, per-instance override stack, or spring relaxation.
- **Graphlet Card is readable but not yet editable.** It shows binding, members,
  selectors, drift state, and actions. Selector/family editing still needs a
  shared graphlet/swatch control rather than a card-only picker.
- **The Swatch P4/P5 plan is now partly advanced by this work.** P4/P5 are still
  not complete by their own done conditions, but "unstarted" is stale.

### Next moves

1. **Keep the swatch P4/P5 status honest as future slices land.** The current
   code closes useful P4/P5 slices, not the full swatch primitive architecture.
2. **Add a small relation-cell visibility setting surface.** Show the effective
   hidden state in Link Card rows and one canvas/context action path, then keep
   the action routed through `RosterIntent` / command drains.
3. **Decide the next graphlet selector UI owner.** The likely home is the
   graphlet/swatch strip because changing selectors re-derives the graphlet for
   every surface that binds it. The Roster Card can display and invoke it.
4. **Split `roster_view_parts.rs` before adding richer controls.** It is close
   to the 600-line ceiling and now owns too many repeated row/card fragments.
5. **Headed-verify the combined route.** The test stack covers behavior. A short
   drive should confirm: select a relation cell, hide it from Link Card, see the
   canvas/swatch drop only that lane, restart, and confirm it stays hidden.

### Sidequests

- **Doc hygiene.** `swatch_primitive_plan` now has a dated note saying P4/P5 are
  partially advanced by roster/orrery work while the full done conditions remain
  open.
- **Terminology cleanup.** Product text should keep saying `members` for graphlet
  rosters. `anchors` remains a forme/internal field name unless the UI is
  specifically discussing seeds.
- **Session sidecar naming.** `HiddenRelationRecord` now records relation cells,
  not just endpoint bundles. The name still works, but the comments should keep
  the cell scope explicit.
- **Context menu parity.** Link Card row hide/show now exists. The context menu
  can reuse the same intent path once the selected-cell subject is present.
- **Target-path friction.** Headed checks still need the active
  `CARGO_TARGET_DIR` binary, commonly `C:\t\graphshell-target\debug\meerkat.exe`,
  not a stale repo-local binary.

### Synergies

- **Roster and swatch now meet on relation cells.** The roster table/card can
  describe the thing the canvas and connections swatch can pick.
- **Graphlet controls can reuse the same selector vocabulary.** `RelationKind`,
  `RelationSelector`, and `EdgeFamily` already bridge link cards, graphlet
  derivation, and connections rendering.
- **The command-drain boundary held.** Roster UI queues intents; host code mutates
  Orrery/graphlet/session state. That keeps view rendering free of graph writes.
- **View-intent persistence is the right temporary home.** It lets relation-cell
  visibility ship without touching kernel truth or forcing the full P5 override
  stack early.
- **The Link Card is now the testbed for edge-family UI.** It can prove hide,
  show, relate, retract, and selector vocabulary before the swatch strip becomes
  more interactive.

### Contradictions

- **Swatch P4/P5 wording lagged the code.** The swatch plan still described P4-P7
  as unstarted after roster work landed partial cells-as-edges and visibility
  behavior. Treat the swatch done conditions as authoritative, but not the old
  status line.
- **Relation-cell UI is deeper than the physics model.** Display and hit-testing
  now see cells; gyre topology and springs still see endpoint pairs.
- **Visibility looks per-cell but behaves per-session.** Users can hide one
  visible relation cell, but there is not yet a graph-level default or
  per-instance inheritance stack.
- **Graphlet selectors are in the data but not yet in the controls.** Linked
  graphlets can derive through selectors; the Roster Card mostly reports that
  state instead of editing it.
- **"Card" names three surfaces.** Roster detail cards, canvas focus cards, and
  swatch cards have different lifetimes and owners. Reviews need to name which
  card is being changed.

### Pitfalls

- **Do not turn hidden cells into graph truth.** Hide/show is view curation;
  assert/retract/delete are graph mutations.
- **Do not claim true parallel edges.** Current `EdgeCell` identity is
  `(from, to, selector)`. It cannot represent two separate same-kind assertions.
- **Do not add selector editing only to the Roster Card.** Selector changes are a
  graphlet derivation rule, so every bound surface needs to see the same result.
- **Do not rely on `cargo check -p meerkat --lib` here.** `swatch`, `render`,
  `window_view`, and `graphlets` are bin modules. Use targeted bin tests.
- **Do not grow `roster_view_parts.rs` much further.** It is near the local file
  ceiling and should be split before the next control-heavy pass.
- **Do not conflate swatch P5 with this visibility bridge.** P5 needs
  `GraphDefault < GraphViewOverride < SelectionOverride` plus spring effects.

---

## Progress

### 2026-06-29 - Roster tables and first detail cards

Landed:

- `RosterTab::{Nodes, Links, Graphlets, Fields}`.
- `RosterSubject::{Node, LinkBundle, RelationCell, Graphlet, Field, Facet}`.
- Tab-specific roster row collections instead of overloading the old node/field rows.
- Links table from relation cells, grouped by endpoints, with family/kind/source/target
  columns.
- In-roster detail region.
- Link Card with family grouping, semantic relation picker, editable relation
  retraction, and endpoint-bundle hide/show support.
- Graphlet Card with kind/binding/members/selectors/drift preview and graphlet actions.
- Field/Facet card first pass.
- Focus-card snapshots, object card, and connections swatch left on the canvas path.

Verification:

- `cargo test -q -p meerkat --bin meerkat roster` - passed, 29 tests.
- `cargo test -q -p meerkat --bin meerkat folded_roster_renders_visible_tab_buttons_in_shell_document`
  - passed, 1 test.
- `cargo build -q -p meerkat --bin meerkat` - passed.
- `git diff --check` over the touched roster/input/selection files - clean except
  existing LF-to-CRLF notices.
- File line counts stayed under the 600 LOC ceiling:
  `roster.rs` 525, `roster_data.rs` 534, `roster_view.rs` 540,
  `roster_view_parts.rs` 573, `input/panes.rs` 510,
  `roster_action_tests.rs` 512, `roster_view_action_tests.rs` 187,
  `selection.rs` 321.

Headed/runtime notes:

- The fresh app binary for headed verification was
  `C:\t\graphshell-target\debug\meerkat.exe`; using repo-local
  `target/debug/meerkat.exe` produced stale visual checks.
- Screenshots were saved under `C:\Users\mark_\Code\scry-shots\`:
  `meerkat-roster-tabs-visible-final-full.png`,
  `meerkat-roster-tabs-visible-final-crop.png`, and
  `meerkat-roster-tabs-visible-final-tabs-zoom.png`.
- Runtime verified that roster tabs are visible and switch state. The active-tab
  marker is deferred because the styled version made selected rows disappear.

Open after this pass:

- Persistent relation visibility stack.
- Deeper graphlet selector/family editing.

### 2026-06-29 - Open-tail pass

Landed:

- Active tab styling now emits `roster-tab roster-tab-active`, preserving the base
  tab class so tab hit-testing and tests still see all four tabs.
- Roster CSS now border-boxes the root and scroll body so padding does not steal
  pane height at high zoom.
- Connections swatch extraction no longer dedups relation cells to one endpoint
  line. It emits one fanned DOM edge per relation cell, carries endpoint ids and
  `RelationKind::tag()`, and skips hidden endpoint bundles.
- Clicking a relation-cell dot in the connections focus card sets
  `RosterSubject::RelationCell` on the Links tab, opening the same Link Card path
  as the Links table when the Roster is visible.
- Link Card hide/show now saves session state. The view-intent sidecar persists
  hidden endpoint bundles as `HiddenRelationRecord` rows and restores only records
  that still match live relations.
- Orrery exposes hidden endpoint bundles as stable member-id pairs for the host
  persistence bridge. Kernel storage remains unchanged.
- Graphlet Card now shows a compact drift proposal summary (`+N -M`, clean, or
  not tracked) alongside the existing dry add/remove detail rows.
- `DOC_README.md` was updated in the same session per `DOC_POLICY.md`.

Verification:

- `cargo fmt -p meerkat -p orrery` - passed.
- `cargo check -p meerkat --bin meerkat --message-format=short` - passed.
- `cargo test -p meerkat --bin meerkat graphlet_card -- --nocapture` - passed,
  3 tests.
- `cargo test -p meerkat --bin meerkat roster_view::tests::roster_marks_the_active_tab -- --nocapture`
  - passed, 1 test.
- `cargo test -p meerkat --bin meerkat swatch::tests::connections_spec_fans_multiple_relation_cells -- --nocapture`
  - passed, 1 test.
- `cargo test -p meerkat --bin meerkat session_ops::view_intent::tests::hidden_relation_records_round_trip_endpoint_visibility -- --nocapture`
  - passed, 1 test.
- `git diff --check` over the touched files - clean except existing
  LF-to-CRLF notices.
- File line counts stayed under the 600 LOC ceiling:
  `roster_view_parts.rs` 579, `roster_view.rs` 549, `roster_data.rs` 545,
  `roster.rs` 526, `roster_action_tests.rs` 512, `input/panes.rs` 512,
  `swatch.rs` 441, `input/mouse_dispatch/press.rs` 424,
  `input/editing.rs` 420, `selection.rs` 329,
  `roster_view_action_tests.rs` 188, `session_ops/view_intent.rs` 99,
  `render/connections.rs` 89.
- Headed run launched `C:\t\graphshell-target\debug\meerkat.exe`; window title
  was `Meerkat — Mere chrome on serval`. Screenshots saved under
  `C:\Users\mark_\Code\screenshots\`:
  `meerkat-roster-tail-2026-06-29.png`,
  `meerkat-roster-open-2026-06-29.png`, and
  `meerkat-roster-tab-switch-2026-06-29.png`.

Still open after the 2026-06-29 pass:

- Pair-level canvas underlay and gyre hit-testing are unchanged. The swatch can
  address relation cells; the orrery edge pass still draws/hits endpoint bundles.
- Visibility persistence was endpoint-bundle scoped at this point. The
  relation-cell visibility pass below closes the current display/session path.
- Graphlet Card controls still use the current action set; deeper
  selector/family editing remains deferred.

### 2026-06-30 - Canvas relation-cell pass

Landed:

- Orrery has a relation-cell geometry helper that fans parallel relation cells
  around the pair segment while leaving gyre topology pair-based.
- Canvas edge hit-testing now picks the nearest visible relation cell instead
  of falling back to the first relation on the endpoint pair.
- Marquee edge selection now collects visible relation cells, and selected-cell
  overlay redraws the exact selected cell lane.
- Graphlet Card wording now says `members` for the member roster rather than
  leaking forme's internal seed-field name.

Verification:

- `cargo test -p orrery --lib` - passed, 83 tests.
- `cargo test -p meerkat --bin meerkat graphlet_card -- --nocapture` - passed,
  3 tests.

Still open:

- Gyre topology and physics springs are still endpoint-pair scoped.
- Visibility persistence was still endpoint-bundle scoped at this point. The
  relation-cell visibility pass below closes the current display/session path.
- Graphlet selector/family editing remains deferred.

### 2026-06-30 - Relation-cell visibility pass

Landed:

- Orrery visibility now keys hidden edges by `EdgeCell` instead of endpoint pair.
- Link Card relation rows can hide/show one current relation cell while bundle
  hide/show still covers all live cells between the endpoints.
- Session `HiddenRelationRecord` save/restore now round-trips exact
  `(source, target, RelationKind)` hidden cells.
- Connections swatch and gloss/minimap filtering respect hidden relation cells
  while keeping the endpoint pair visible when another relation cell remains.
- `HideSelectedEdge` and `ShowAllEdges` now save visibility changes through the
  existing session path.

Verification:

- `cargo test -p orrery hide_selected_edge_cell_hides_only_that_relation -- --nocapture`
  - passed, 1 test.
- `cargo test -p meerkat --bin meerkat roster_action_hides_and_shows_one_relation_cell -- --nocapture`
  - passed, 1 test.
- `cargo test -p meerkat --bin meerkat link_card_actions_queue_endpoint_relate_and_retract_intents -- --nocapture`
  - passed, 1 test.
- `cargo test -p meerkat --bin meerkat hidden_relation_records_round_trip_relation_cell_visibility -- --nocapture`
  - passed, 1 test.
- `cargo test -p meerkat --bin meerkat roster_action -- --nocapture` - passed,
  11 tests.
- `cargo test -p orrery --lib` - passed, 84 tests.

Still open:

- Gyre topology and physics springs are still endpoint-pair scoped.
- Relation-cell visibility is keyed to current `(source, target, RelationKind)`
  cells. True parallel edge instances remain out of scope.
- Graphlet selector/family editing remains deferred.
