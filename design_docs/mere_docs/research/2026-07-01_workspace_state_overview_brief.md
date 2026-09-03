# Workspace State Overview Brief

**Date**: 2026-07-01
**Status**: cross-cutting state snapshot (research). Doc-derived: full
`DOC_README.md` index read plus deep reads of the most recent plans
([roster detail cards](../../archive_docs/2026-09-02_retired_plans/2026-06-29_graph_object_roster_detail_cards_plan.md),
[smolweb host integration](../implementation_strategy/2026-06-28_smolweb_host_integration_plan.md)),
then a second pass over the 2026-07-01-touched crop (§7). Statuses are as
their owning docs report them; code-verified only where noted. Successor
snapshot to the [in-the-wings audit](2026-06-15_in_the_wings_and_browser_bar_audit.md)
(which was code-verified; this one is doc-verified).

**Purpose**: one dated answer to "where are we, where are we headed,
sidequests, pitfalls, synergies, contradictions" across the workspace, as a
baseline for the next audit.

---

## 1. Where we're at

The stack is live end to end: meerkat runs Mere chrome on genet as one shell
document, themed from seed palettes ([theme system](../../archive_docs/2026-07-04_completed_plans/2026-06-22_seed_palette_theme_system_plan.md)
complete), user zoom shipped, auto-DPI planned.

- **Graph core.** Statement-shaped kernel on petgraph, six edge families,
  SPARQL read slice, JSON-LD in/out. The [Roster](../../archive_docs/2026-09-02_retired_plans/2026-06-29_graph_object_roster_detail_cards_plan.md)
  is now the durable graph-object control surface (nodes / links / graphlets /
  fields / facets, with in-roster cards). Relation cells
  `(source, target, kind)` are addressable across roster rows, Link Card,
  canvas picking, connections swatch, and session persistence; springs pull
  per visible cell and graphlet family selectors are editable from the card
  (both 2026-07-01).
- **Spatial shell.** Multi-graph, multi-window, and the tear-out trichotomy
  (leaf / branch / fork) are functionally done; the
  [gestures plan](../implementation_strategy/2026-06-24_tearout_gestures_plan.md)
  retains the interactive tail (toast view, drag-out gesture, dock-side
  setting).
- **Memory.** Alembic pane, Athanor idle daemon, engram save / open / compose
  shipped; the tail handoff archived 2026-06-30 with all items done.
- **Content.** Render ladder live for static HTML plus the native smolweb lane
  (P1-P3 landed 2026-06-28); document style sheets, typography surface, and
  the DocumentScript wasm-component substrate shipped, net hardening
  partially landed.
- **P2p.** p2panda substrate, mesh M1, moot M1, comms shell largely done;
  Reticulum transport probe active. Relational browse V1+V2 with crawl
  controls shipped.

## 2. Where we're headed

- **The keystone: [capture / provenance / consent](../implementation_strategy/2026-06-26_capture_provenance_consent_plan.md).**
  The eidetic sink is built but nothing live writes a `BrowsingTrace`, and the
  Provenance family has zero writers. C1 (the live recorder) unlocks the
  browsing-memory vision: eidetic corpus, tessera pricing, the flora lane.
- **Undo/redo, then the event log and Timeline**
  ([event_log_timeline](../implementation_strategy/2026-07-01_event_log_timeline_plan.md):
  undo ships first, with zero storage).
- **The write side of knots**: the [djot editor](../../archive_docs/2026-08-06_completed_plans/2026-06-24_djot_editor_knot_nodes_plan.md)
  (jotdown + logos injection, pure Rust, wasm-safe), with
  [illume / tinct](../implementation_strategy/2026-06-26_illume_text_lexer_plan.md)
  extraction as publishable siblings.
- **Swatch as the fourth primitive**: the P5 visibility stack
  (`GraphDefault < GraphViewOverride < SelectionOverride`), templates, and the
  [graph-signals producer](../../archive_docs/2026-08-20_completed_plans/2026-06-22_graph_signals_layer_plan.md)
  that arrangements, encodings, and the gloss lens all wait on.
- **The resource-coordination ring**: mesh M2, lease scheduler, kith
  capability sharing, bounty economy, commitment proofs (all scoped
  2026-06-30).
- **Engine consolidation**: the [surface-engine contract fold](../../archive_docs/2026-07-04_completed_plans/2026-06-28_surface_engine_contract_fold_plan.md)
  (scry / weld / graft onto one `SurfaceEngine`), the verso flip, the scripted
  rung integration, retained-text tiled render.
- **Longer arcs**: browser-extension delivery (capture-first), federation
  interop (Cambria lenses), MCP, the local-models harness, persona transport
  unlinkability plus the wallet carry layer.

## 3. Sidequests

- [lane0_sidequests](../../archive_docs/2026-09-02_retired_plans/2026-06-16_lane0_sidequests_plan.md):
  glue-only wins where the dep edge already exists (JSON-LD export,
  recover-node, relation-kind picker, tessera chip, Barnes-Hut, Trail button).
- Find-in-page host UI (backend committed, UI unbuilt); notification
  subsystem P1+ (foundation tested, no toast view yet); context-menu parity
  for relation-cell actions; gloss Scene-to-DOM migration; the
  [render-perf plan](../../archive_docs/2026-09-02_retired_plans/2026-06-24_meerkat_render_perf_plan.md)
  (split the ~1700-line `render()`, dirty-gate the redraw loop);
  [tracing reach](../../archive_docs/2026-09-02_retired_plans/2026-06-26_tracing_reach_and_quality_plan.md)
  (nine first-party crates at zero tracing calls); physics scenes and the
  isometric camera as the playful pair.

## 4. Pitfalls

- **Headed verification is blocked**: meerkat launches clean but the window
  sticks at a 13x13 px rect, so the 2026-07-01 roster slice is owed a
  physical-display drive. Nothing in that slice touches window creation; cause
  unexplained. (Recorded in the roster plan's R6.)
- **The one-texture render** ([in-the-wings audit](2026-06-15_in_the_wings_and_browser_bar_audit.md) §5):
  pages baked into a single GPU texture cap body size and scroll and block
  find-in-page and selection; retained-text tiled render is the single fix
  and much waits behind it.
- **`net.fetch` is prototype-only** until origin-scoped uncredentialed fetch
  lands; as built it carries the user's ambient cookies onto the open
  internet ([net hardening](../../archive_docs/2026-07-03_completed_plans/2026-06-23_documentscript_net_hardening_plan.md) §A1).
- **`cargo check -p meerkat --lib` false-cleans**: swatch / render /
  window_view / graphlets are bin modules; use targeted bin tests.
- **Semantics discipline**: hiding a cell is view curation, never graph
  truth; `EdgeCell` identity cannot represent two same-kind assertions, so
  never claim parallel edges.
- **Doc drift runs both directions**: headlines lag their own progress logs
  (swatch said "unstarted" after code landed) and also outrun the code (the
  edge audit's stale "no way to draw an edge"). The standing rule holds:
  trust the code.
- **Naming hazards**: the engram-ref catalog is now `fauna`, reserving `flora`
  for federated LoRA (resolved 2026-07-12). "Card" still names three surfaces
  with different lifetimes (roster detail card / canvas focus card / swatch
  card), and LoRa radio remains easy to confuse with LoRA adapters.

## 5. Synergies

- **Relation cells are the convergence point**: roster, Link Card, canvas
  picking, connections swatch, and petgraph-rdf's "multi-edge is truth,
  collapse is experience-LOD" all speak the same `(source, target, selector)`
  language. The Link Card is the cheap testbed for edge-family UI before the
  swatch strip gets interactive.
- **The command registry is one effort with five payoffs**: palette, context
  menu, keybindings, the rhai automation lane, and MCP tool exposure all ride
  the same `ActionRegistry`.
- **One capture record, three payoffs**: provenance edges serve tessera
  pricing, legality, and index legibility simultaneously.
- **The theming stack is cross-product**: Woodshed's OKLCH derivation shipped
  first and Mere consumes it; illume + tinct serve the editor, omnibar, and
  comms from one lexer.
- **The actor constellation maps onto BrowserEngineKit's mandated process
  split**, so an iOS sovereign-engine lane is process-portable if wanted.
- **The gloss outline doubles as an editable knot**, which is where
  notetaking arrives almost for free.

## 6. Contradictions

- **The dominant shape is built-but-unwired**: `IntelligenceSignals` defined
  but never computed, Provenance edges defined but never written, the eidetic
  sink with no live tap, the tracing spine forwarding only three targets. The
  machinery repeatedly exists before its producer does.
- **UI depth outruns the model in places**: display and hit-testing see
  relation cells, but sub-kind selectors, the P5 override stack, and true
  parallel edges remain unbuilt; visibility looks per-cell but behaves
  per-session.
- **Scope-model drift**: the wiring build instanced window-per-graphlet where
  the canonical model says scope the one Navigator;
  [scope_model_reconciliation](../design/2026-06-27_scope_model_reconciliation.md)
  ruled it a correction, still pending in code.
- **The privacy dial is named but not enforced**: `PrivacyClass` tags exist
  while engrams sit cleartext at rest; encrypt-LocalOnly was decided
  2026-06-25 and stays a gap until the wallet layer builds it.
- **Two delivery framings coexist**: the companion plan's orrery-in-a-tab DOM
  cards vs the browser-lane's capture-first "the browser browses". The lane
  plan superseded the delivery half; the companion doc still carries the
  forward p2p / smolweb vision, so readers must hold both.

## 7. Second-pass notes (2026-07-01 crop)

*(Pass pending; this section is filled in the same session once the
2026-07-01-touched docs are read: gloss scene-to-DOM, gloss outline lens,
djot editor, smolweb fidelity, event log timeline, reticulum transport,
alembic, surface-engine fold, notification subsystem, athanor, swatch
primitive, terminology.)*
