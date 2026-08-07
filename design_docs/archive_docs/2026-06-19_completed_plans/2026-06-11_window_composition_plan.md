# Window Composition Plan — pooled orrery authorities, pane resolution, cross-graph composability

**Date**: 2026-06-11
**Status**: **Complete — archived 2026-06-19.** The enabling move shipped; the rest
shipped elsewhere or spun out. **Banked:** P1 (pooled orrery authorities) is done and
load-bearing (pool + `orrery_lru` + `MAX_POOLED_ORRERIES` + `reap_graph` + park/unload
+ Steward live-count, all code-verified); OQ2 resolved; the `frame_ops` split landed
(505 LOC across nine modules); the P2 load-bearing half shipped (the ctx borrows the
pool, render draws multiple orreries, `OpenGraphBeside` summons a second graph pane,
per-pane render + wheel/hover). **Migrated:** the P2 per-pane *input* tail (per-pane
focus / nav / save) into the
[unified_document_host_plan](../../mere_docs/implementation_strategy/2026-06-17_unified_document_host_plan.md)
(one shell document + shell hit-test). **Spun out:** P3-P5 + the deferred per-pane
camera + OQ3-OQ5 to
[tearout_composability_plan](../../mere_docs/implementation_strategy/2026-06-19_tearout_composability_plan.md).
Original planning text preserved below as the historical record.

**Historical status:** Planning. Supersedes the [multi-window plan](2026-06-10_multi_window_plan.md)'s
**MW4–MW6** (leaf tear-out / branch+fork / orrery-split). MW1–MW3 (the per-window
reshape: `WindowCtx` seam, `WindowId` registry, one-device/N-surfaces, spawn/close,
slim leaf chrome) are done and stand; this plan builds on them. The
[tear-out brief](../research/2026-05-11_tearout_operations_brief.md) is the design
source for the leaf/branch/fork gesture model; this plan is the architecture those
operations rest on once a window can own its own orrery.
**Code**: `crates/meerkat/`, `crates/orrery/`, `crates/forme/`, `crates/system/session-runtime/`.

**Spine**: per the [interaction model spine](../technical_architecture/2026-06-18_interaction_model_spine.md),
this plan owns the **interact** stage at the host level: the one input spine + one focus authority
(the P2-companion) and the external-texture-element-that-bears-input bridge, over the shell-root
document unified-document-host Phase 1 consolidates.

---

## The reframe (2026-06-11, with Mark)

The multi-window plan deferred the orrery split to MW6 as "the single hardest piece,"
and routed the second window through a workbench-only leaf to avoid it. That was an
over-deferral. The thing that looked hard — many orreries coexisting — is mostly already
supported one layer down, and the leaf-only interim throws away work.

The first correct framing was still wrong: it is **not** "orrery windows vs orrery-less
windows" (a window-level property). It is **orrery (authority) vs panes (views)**:

- An **orrery** is a graph's authority — its graph + physics + canonical state, the
  *source* of every pane's content for that graph. Orreries live in a pool keyed by
  `GraphId`/session, independent of any window. An orrery need not be rendered to exist.
- A **pane** is a view that **resolves to an orrery** by `graph_id`: the spatial
  orrery-view (with a camera), the **workbench, gloss, steward, inspector, apparatus,
  alembic, roster, comms**. A pane reads and acts on its orrery's graph; the orrery's own
  spatial view does **not** have to be present in the same window — the pane resolves to
  it regardless.
- A **window** is a frame-split of panes. The panes in one window may resolve to
  *different* orreries (graph A's workbench beside graph B's gloss). A window has no
  intrinsic orrery; it has panes, each pointing at its orrery.

**Linkage is emergent, not a second axis.** Panes resolving to the *same* orrery are
synced (they share the one authority); panes resolving to *different* orreries are
independent. The tear-out trichotomy is just which orrery a torn pane resolves to:

| Operation | Torn pane resolves to | Identity |
| --- | --- | --- |
| **Leaf** | the donor's orrery (same graph) → synced | none |
| **Branch** | the donor's orrery, new graphlet lineage → synced | new `GraphletId` |
| **Fork** | a fresh orrery (new graph, rekeyed copy) → independent | new `SessionId`+`GraphId` |
| **"New graph window"** | its own fresh orrery → independent | own session |
| **Primary** | the root orrery (with a spatial orrery-view pane) | the root session |

**Composability is re-pointing a pane across orreries.** Because a pane resolves to an
orrery by id, moving/copying a tile from graph A into graph B is re-pointing (move) or
duplicating (copy) the binding across orreries; copy mints a node in B's orrery and
records **provenance + lineage** back to A's node. Fork's cross-graph rekey is one
consumer of this general operation.

## Findings (code-verified, why the refactor is cheaper than it looked)

- **Node ids are global UUIDs.** `GraphMemberId = Uuid`
  ([forme/arrangement.rs](../../../crates/forme/forme/src/arrangement.rs)). The shared
  `constellation` (the content-actor pool that renders every node's tile/card) is keyed
  by that UUID and is held on `SharedState`, not on the orrery. So **the content layer
  is already graph-agnostic**: nodes from two different graphs coexist in it with no key
  collision. This is the fact that makes two orreries cheap — the part assumed to be a
  tangle isn't one.
- **The orrery bundles graph + physics + camera as one `Shell` field** (~58 access
  sites; the MW2 scope's "MW6 IOU"). Two orreries over two *different* graphs needs only
  to move that field per-window and give each its own physics actor (a second offload
  thread; the winit wake proxy is already shared). **No graph registry needed** for the
  different-graph case.
- **Frame panes already carry `graph_id`** (MG5 / `FrameLayout` leaf `graph_id`), so the
  per-window frame already distinguishes which graph a pane belongs to.
- **Provenance/lineage primitives exist.** The graph substrate has a per-node lineage
  facet (`graph/node-lineage`, not forme) and a provenance edge family (persisted as
  `PersistedProvenanceEdgeData`); fork already plans a cross-graph rekey (tear-out brief
  §7.5) and a weak `parent_session` ref. The composability operation records into these,
  it does not invent them.
- **Pooling whole orreries needs no graph extraction.** A `HashMap<GraphId, Orrery>` just
  holds N whole `Orrery` values (each its own graph + physics + camera); panes read the
  graph of the one they resolve to. The *only* thing that needs the camera pulled out of
  `Orrery` is *two spatial `Orrery` panes of the same graph* (one orrery, two cameras) —
  deferred, and orthogonal to the pool.
- **member→graph routing: decided (2026-06-11) — stamp the active `Activation`.** With N
  orreries pooled, the one thing that needs to know a member's graph is *routing a content
  actor's `GraphContribution`s back to the right orrery* (scenes apply by UUID inside the
  pool; subresources route by URL — both graph-agnostic; only contributions mutate a
  specific graph). `Constellation.active: HashMap<GraphMemberId, Activation>` is the
  per-member entry, and `drive(member, …)` already runs in a pane context that has the
  `graph_id`. So **stamp each `Activation` with its `graph_id` at `drive`-time**;
  `drain` pairs each contribution with its member's stamp (`contributions: Vec<(GraphId,
  GraphContribution)>`); `user_event` routes to `shared.orreries.get_mut(&graph_id)`. Not
  a global `member→graph` index (it would track dormant members + risk drift for a lookup
  only *active* members need) and not a per-contribution pool search. `graph→members` stays
  authoritative in each orrery's graph. The stamp is set once at activation and changes
  only on a P4 cross-graph **move** (branch is intra-graph; fork's copies get fresh UUIDs).
  Concrete P1 surface: `drive` gains a `graph_id` param, `Activation` a `graph_id` field,
  `Drained.contributions` becomes `Vec<(GraphId, GraphContribution)>`.
- **P1 site scout (2026-06-11): 69 `*.orrery` sites, no window-global-without-a-graph.**
  Inventoried across `frame_ops` (36), `agent_harness` (10 tests), `render` (9),
  `app_handler` (6), `input` (5), `main` (3). They fall into three resolution buckets:
  - **Pane-scoped** (resolve via the pane being rendered/hit → its `graph_id`, already in
    scope): the spatial Orrery-pane drive (`set_node_states/shapes/resize/recenter/frame`),
    the `Workbench`/`Gloss` pane graph reads, `a11y project_graph` for the Orrery pane,
    input's orrery `pointer_down/up`, orrery `cursor_moved`/`wheel`, roster/gloss
    `select_by_url`. Render already iterates leaves and input hit-tests to a pane, so each
    has its `graph_id`.
  - **Window-focused-graph** (omnibar/keyboard/command-triggered nav + focus + selection +
    edit): `visit`/`navigate_member`/`open_member_as_new_node`, `focused_member`/`_url`,
    history `back`/`forward` + `can_*`, omnibar `select_by_url`, `selected_members`,
    `remove_focused`, `hide`/`show` edges, the focused card. These want **one new
    per-window accessor `focused_graph_id()` = the active content pane's leaf `graph_id`**
    (the window already tracks `active_content`, and frame leaves carry `graph_id`), so
    `self.orrery` → `shared.orreries.get(self.focused_graph_id())`. Open Q1's anticipated
    "focused-graph fallback" is not a fallback — it is the primary resolution for ~half the
    sites, and it is cleanly derivable (no new tracking, just a helper).
  - **Special** (individual): contribution routing (`app_handler:224`, the stamp consumer);
    `set_graph` (session switch → "load/swap the graph in the pool"); `save_session` (a
    specific graph's snapshot + camera); switcher thumbnail (the active graph's live one);
    `Shell::new` (seed the first orrery into the pool); the test harness (`app.shared.orreries`).
  - **One P2 soft-spot, not a P1 blocker:** `set_ctrl`/`set_shift` and `cursor_moved`/`wheel`
    (`app_handler`) assume one orrery-pane-under-cursor. Fine at one Orrery pane per window
    (P1 pools but keeps one spatial view per window); once a window hosts *two* Orrery panes
    (different graphs side by side), these must hit-test which orrery pane the cursor is
    over. That is a P2 (multi-orrery-pane) concern.
  - **Verdict: P1 is a clean sweep, two helpers (`focused_graph_id()` + the in-scope pane
    `graph_id`) + the few special sites.** No soft spots that block the mechanical move.

## Architecture

Orreries are pooled authorities; panes resolve to them by `graph_id`. Nothing is a
"window kind"; a window is characterized by the panes in its split.

```rust
struct SharedState {
    // Orreries: the graph authorities, one per graph, pooled (lazily loaded). Each is a
    // whole `Orrery` = graph + physics + spatial state. Was the single `Shell.orrery`.
    orreries: HashMap<GraphId, Orrery>,
    // Content actors, keyed by the global node UUID — already graph-agnostic, so it
    // serves every orrery's nodes from one pool. Unchanged.
    constellation: Constellation,
    // actor handles, manifests, theming, observability ... (unchanged, MW2)
}

// A pane is a view that resolves to an orrery by graph_id (panes already carry it, MG5).
// PaneContent is the existing enum, the open set of view types (Alembic is aspirational —
// not in the enum yet; the rest exist today):
enum PaneContent { Orrery, Workbench, Gloss, Steward, Inspector, Apparatus, Alembic, Roster, Comms }
// FrameLayout leaf: { pane_id, content: PaneContent, graph_id }  ← graph_id picks the orrery

// A window is a frame-split of panes; per-window state stays in WindowView (MW2).
// Render/input resolve the operated pane's orrery: shared.orreries.get(&pane.graph_id).
```

The shift from MW3: render/input read `self.orrery` (one, the active graph) →
`self.shared.orreries.get(&graph_id)` for the pane being drawn / hit. The panes in a
window can resolve to different graphs; a pane resolves to its orrery whether or not that
orrery's spatial **Orrery pane** is shown anywhere. The reverted MW3 step-4-part-2
isolation gate becomes simply "this pane is not a `PaneContent::Orrery`, so it does not
drive a camera" — falling out of pane resolution, not a window-level slim flag.

**Camera note.** Pooling *whole* `Orrery` values means one camera per graph. That is
correct for every pane except *two spatial Orrery panes of the same graph* (one orrery,
two cameras) — which needs the camera pulled out of `Orrery` into the Orrery *pane*. That
extraction is the one piece deferred (see below); pooling itself needs no extraction.

## Phases (done-conditions, not dates)

### P1 — Orrery pool on `SharedState` (the enabling move)

**Status: done (2026-06-13)** — landed `16b02b9` (gating restructure) + `aba14ba`
(multi-graph payload) + OQ2 (`7a8304b` unload, `0f94f04` park, `a23fc7c` Steward count);
two graphs coexist, on-screen-verified (see the Progress log). The pool is a **`Shell`
field, not `SharedState`** (the foundation's refinement; the heading wording predates
it). The *save-to-dirs-on-exit* done-condition clause resolved through park rather than a
separate exit-save pass: park halts a switched-away graph's physics, so it stops changing
after its switch-away save, making that save final (only sub-pixel drift remains, which
re-settles on reload).

`self.orrery` (single, the active graph) → `SharedState.orreries: HashMap<GraphId,
Orrery>`, lazily loaded. The ~58 `self.orrery` sites resolve the operated pane's orrery by
`graph_id` (`self.shared.orreries.get(&graph_id)`). Per-graph physics: each pooled orrery
offloads to its own actor on the shared wake. At one graph this is behavior-identical;
**this is also far-B / MG6** (many graphs live at once), so the two converge here.

**The pool's blast radius includes lifecycle, not just resolution** (2026-06-11 review;
three single-active-graph mechanisms are still wired to "switching graphs means
teardown" and must be re-scoped here):

- **Constellation becomes graph-scoped.** The MG2 contract (`Constellation::clear()` on
  switch so actors "don't bleed into the new graph") is exactly wrong under the pool:
  switching the shown graph must not kill another live graph's actors. The constellation
  has *no graph dimension* today (members are bare UUIDs), so per-graph reaping doesn't
  exist — either activations learn their graph at spawn, or reap-by-member-set resolves
  through the owning orrery. `scrying.clear()` and the shared compat pins ride the same
  switch path and re-scope the same way. The tab cap is now shared across live graphs
  (a busy background graph competes with the active one's warm tabs); per-graph or
  per-window caps join the configurable settings.
- **Persistence goes per-orrery.** `session_dir` / `save_session` are
  active-session-shaped; each pooled orrery saves to *its own* manifest's storage dir,
  on switch, window close, exit, and eviction. Without this, the second live graph's
  mutations are lost on exit.
- **Physics wake fan-out.** A pooled orrery's physics tick should redraw only the
  windows with panes resolving to that graph (the MW2 multicast seam), not broadcast.

Done when the primary runs exactly as today with its orrery resolved from the pool, a
second graph's orrery can be live in the pool at the same time (a pane resolving to it
renders it), neither disturbs the other, **and a graph switch reaps only the switched
graph's actors while both graphs save to their own dirs on exit**. 44+/66+ green;
on-screen verify.

### P2 — Panes resolve everywhere (render / input / nav)

**Status (2026-06-19): load-bearing half shipped; the per-pane *input* tail migrated
to the unified-document-host plan (shell document + shell hit-test).**

Every render, hit-test, and navigation path operates on the *pane's* resolved orrery, not
a window-global one. A window's panes may resolve to different graphs (graph A's workbench
beside graph B's gloss). The MW3 `{Primary, Leaf}` marker + `is_slim` fold away: the
shellbar/switcher chrome shows when the window carries an `Orrery` pane (a graph surface),
which is read off the panes, not a window kind.

Two consequences of kindless windows land here:

- **Exit/save semantics.** `Shell.primary` + the kind-forked `CloseRequested` lose their
  rule when kinds fold away. Replacement: any window close saves its panes' view
  intents; the *last* window close saves every dirty pooled orrery and exits. (Adjust if
  a different rule is decided; the point is the rule must be restated, not inherited.)
- **One compat WebView per window (carried constraint).** Scrying X2 surfaced that a
  window has a single WebView2 composition target (`WINDOW_ALREADY_COMPOSED`;
  `reap_except` exists because of it), so two compat tiles in one window cannot both be
  live yet. P2's two-orreries-one-window done-condition holds for ordinary panes; compat
  tiles inherit the one-live-per-window limit until that constraint is lifted.

Done when one window can host panes resolving to two different orreries, each rendering
correctly, with input routed to the right orrery per pane.

### P2 companion — the xilem-serval input spine (the proper-use target)

Context (2026-06-12): the
[host wiring grabbag plan](../2026-08-06_completed_plans/2026-06-11_host_wiring_grabbag_plan.md) completed
its genet-side seams (on_wheel, transform-aware hit-testing, pointer
cancellation, keyboard escape hatches; 51/51 green) and correctly recorded
G1.1–G1.3 + G2.3 as *runway* — their meerkat callers cannot exist yet,
because the content surfaces aren't view nodes. The honest usage inventory:
meerkat runs **two** xilem-serval runners (chrome root: full consumer,
render + dispatch; workbench root: render-only, drags host-level), while the
roster / apparatus / utility panes are hand-built per-frame DOMs hit-tested
through the `WindowView` rect caches, and the orrery / cards / gloss are
scene composition. Three parallel input systems. That was deliberate staging
(the flip rebuilt chrome first; the orrery is host-composited by design;
cards are off-thread actor textures), but it is the most expensive place to
*stay*. The target, landed alongside / after this plan's P2:

- **List-shaped panes become view functions** (roster, apparatus, utility,
  steward, inspector): kills their rebuild-per-frame waste, their rect
  caches, and their hand-built-UxTree a11y drift in one move; gives them
  runner dispatch. Startable independently of P2; composes with the
  cheap-path plan's C5 sessions.
- **The external-texture element view is the bridge for content** (tracked
  in the scrying plan's Later + grabbag G1 notes): once a card / tile /
  scrying texture is an element *in* the pane DOM, `on_wheel` attaches,
  placement comes from layout (no hand-summed rects), and pointer
  cancellation gets its first consumer. This single primitive turns
  G1.1/G1.3 from runway into road.
- **The orrery stays composed but routes at its pane boundary**: never a
  diffed view tree (the node pool is the IncrementalLayout path by design),
  but its pane *element* takes `on_wheel`/`on_key` and forwards semantically
  to gyre. Transform-aware hit-testing (G1.2) goes live exactly when
  in-graph DOM becomes interactive under the camera.
- **One input spine**: chrome-root dispatch on top → unconsumed events fall
  to the pane tree → pane handlers forward into content (orrery semantic
  input, scrying `forward_*`, card scroll). The nine rect caches shrink
  toward zero as panes become views; `default_prevented()` finally gets
  read.
- **Keyboard order**: Tab-to-traverse is additive *now* (Tab is unhandled in
  meerkat). Enter/Space synthetic activation waits on key routing unifying
  through `dispatch_key` — the omnibar collision is an artifact of
  hand-interception in input.rs, not a design conflict; once keys dispatch,
  the omnibar's Enter is just its own `on_key` consuming first.

pelt V2 (genet's reference shell) demonstrates the same pattern mere-free
at 1/20th the size, and is the clean-room check that the spine works.
Charter note (2026-06-12): pelt's plan now carries V5/V6 — the surface grows
a tile tree (presentation-grade, over a genet-side plan-shaped contract
that platen's `tree_projection` maps forme onto) and then sheds its host
loop to become **this plan's workbench pane** (mixed content via genet
content-roots + the external-texture element). Design pane work here with
that destination in mind; the pane-module contract (standalone-or-hosted
surface) gets written down at pelt V6.

### P3 — Cross-window pane resolution (the leaf, done right)

**Status (2026-06-19): unstarted; spun out to the tear-out + composability plan (C3).**

A pane in window B that resolves to an orrery whose spatial view lives in window A. A
torn-out workbench tile is a `Workbench` pane in a new window resolving to the donor's
orrery (same graph), with no `Orrery` pane of its own. **Independently navigable** (its
own omnibar drives its own tile); edits propagate because it resolves to the *same* orrery
(leaf semantics, tear-out brief §4.1). This is MW3 step 4 part 2 reframed: a leaf has no
orrery because none of its panes is an `Orrery` pane — not because an orrery is "hidden."

Done when a torn `Workbench`-pane window shows a shared node's live tile, navigates on its
own, propagates edits to the donor, and instantiates no orrery of its own (it resolves to
the donor's in the pool).

### P4 — Cross-graph composability (re-point / copy a pane, with provenance)

**Status (2026-06-19): unstarted; spun out to the tear-out + composability plan (C4).**

Move or copy a tile/node across orreries. Copy mints a node in the destination orrery via
the cross-graph rekey (tear-out brief §7.5) and records a **provenance edge** (origin:
source node) + lineage. Move re-points the binding. Surfaced as a drag between two panes
resolving to *different* orreries (and a palette/command form).

Done when a tile dragged from a pane on graph A into a pane on graph B produces a node in
B with provenance + lineage back to A, and the source is left intact (copy) or releases
its binding (move).

### P5 — The tear-out gesture model (leaf / branch / fork + toast)

**Status (2026-06-19): unstarted; spun out to the tear-out + composability plan (C5).**

Implement the [tear-out brief](../research/2026-05-11_tearout_operations_brief.md) on top
of P1–P4: drag = leaf (a `Workbench` pane resolving to the donor's orrery), Shift+drag =
branch (donor's orrery, new `GraphletId`), Ctrl/Cmd+Shift+drag = fork (a fresh orrery via
the P4 rekey, + a thin `Orrery` pane), toast on ambiguous drag. Spawn-on-drop with an
in-donor drag ghost (the portable shape from MW4).

Done when all three operations run from the gesture model with the brief's identity
semantics, and the toast escalates a leaf in place.

### Deferred — per-pane camera (two spatial views of one graph)

**Status (2026-06-19): still deferred; carried to the tear-out + composability plan.**

Pull the camera out of `Orrery` into the `Orrery` *pane*, so two `Orrery` panes of the
same graph (in one split, or across windows) hold distinct cameras. Not needed by P1–P5
(those have at most one spatial view per graph). The orrery *pool* already shares the
graph; this is only the camera. Picked up when two cameras on one graph are actually
wanted.

## Open questions

1. **How a pane names its orrery** — `graph_id` is on the `FrameLayout` leaf today; P1
   threads it through the ~58 resolution sites. Confirm every `self.orrery` site has a
   pane (and thus a `graph_id`) in scope, or whether some are genuinely window-global
   (e.g. the active-nav target) and need a per-window "focused graph" fallback. Related:
   `follows_active_graph()` (frame lib.rs:203) changes meaning under the pool — today
   panes *follow* the window's active graph (the Model A residue); pooling makes
   follow-vs-pin a real per-pane choice (follow by default, pin deliberately, per the
   configurability rule), and the switcher becomes "re-point the following panes."
2. **Orrery pool lifecycle** — lazily load an orrery into the pool when a pane first
   resolves to its graph; evict when no pane (in any window) resolves to it. Eviction
   needs a named flavor: **park** (stop the physics actor, keep the graph in memory) vs
   **unload** (save to its dir + drop); park first, unload under memory pressure.
   N live orreries = N physics actor threads; fine for a handful — and surface the live
   count in Steward (real data, the no-placebo rule) as the tripwire for revisiting.
   **Park + unload landed (2026-06-13).** Unload (`7a8304b`): a `MAX_POOLED_ORRERIES`
   cap + an `orrery_lru` order drop the stalest non-focused orrery over the cap (the
   drop ends its physics actor thread; its content actors are reaped). No save on evict
   — the graph was persisted when last switched away from, so it reloads on switch-back.
   Park (`0f94f04`): `Orrery::park_physics()` halts a switched-away graph's settle so it
   stops ticking + waking the loop while warm (the actor then idles on its channel, no
   busy-spin); no explicit unpark, since the layout is left settled and interaction
   resumes it. Together: park first, unload under memory pressure. Eviction already skips
   every window's focused graph (P2-ready). The Steward live-count surface landed too
   (`a23fc7c`): a "Live graphs" row shows live / cap as the no-placebo tripwire. **OQ2 is
   resolved.** One minor edge remains: `close_session` leaves a trashed session's orrery
   pooled until LRU eviction rather than dropping it immediately.
3. **Provenance edge direction + family** — confirm the existing provenance edge family
   carries "copied-from across graphs" cleanly, or whether copy wants a distinct sub-kind
   (P4). Check before building, per the consumer-pull rule.
4. **Move vs copy default** — a cross-graph drag defaults to which? Likely copy (safer,
   provenance-tracked); move on a modifier. Decide in P4.
5. **Linked-tile lifecycle on donor delete** — the brief §8.3 cascade rules (leaves lose
   their node, branches die, forks survive) map onto the linkage axis; wire them in P3/P5.

## Progress

- 2026-06-14: **Scoping note — the focus/active-session decoupling (pane-as-unit), the
  prerequisite for per-pane *pointer* input.** Found while wiring the second graph-pane:
  per-pane *render* and *wheel/hover* fell out cleanly (a pane resolves its orrery by
  `graph_id`; landed `85f307c`, `21d9821`). Per-pane *pointer-select / nav* does **not**,
  and the blocker is architectural, not grind.

  **The mismatch.** mere ties an orrery to a *session* (graph + `session_dir` + manifest),
  and the shell holds one global `active_session_id`. That active session **re-points
  panes**: `load_active_session` calls `retag_graph_bound(active_graph)`, which retags
  *every* graph-bound leaf to the active graph. So a pane's graph is not pinned — it
  follows the global active session. Consequences: (1) a second pane pinned to graph B is
  clobbered the instant you switch sessions (all leaves retag); (2) "focused graph" (what
  input/render resolve) and "active session" (what save/nav/omnibar use) are coupled —
  clicking pane B to nav it either desyncs save (still writes A's `session_dir`) or
  triggers a switch that re-points the primary pane. P1/P2 are a halfway state: panes
  carry `graph_id` + an orrery pool, but the single-active-session model still lives under
  them.

  **The model (the plan's own).** Make the *pane* the unit: each graph-pane **pinned** to
  its `graph_id`; "focused" = *which pane has input focus* (a per-window `focused_pane`),
  not which session is globally active; every session op resolves through the focused
  pane's `graph_id` → its session. Then pointer input dispatches to the pane at the cursor,
  that pane owns its orrery + session context, and the focus/save coupling + the retag
  clobber both dissolve (no pane is ever retagged).

  **Enabler.** A `graph_id → session` resolution: `manifests` are keyed by `SessionId` and
  carry `root_graph_id`, so `session_for_graph(gid) = manifests.find(root_graph_id == gid)`
  yields the `SessionId` + `storage_path` (the `session_dir`). Session ops take the focused
  pane's `graph_id` and resolve their dir / manifest from it, instead of reading the global
  `active_session_id` / `session_dir`.

  **The coupled sites (code-verified inventory).**
  - `save_session` (`session_ops` 27/37/42/54/67) — **the critical one**: saves the focused
    orrery's graph to the *global* `session_dir` + active manifest + thumbnail. Re-scope to
    the focused pane's session (resolved from `focused_graph`).
  - `load_active_session` (`session_ops` 327/345) — `retag_graph_bound(target)` + sets
    `active_session_id`. Retag must re-point only the **focused** pane (or be removed once
    panes are pinned); `active_session_id` becomes the focused pane's session.
  - `retag_graph_bound` (`frame/layout.rs` 235; called `main.rs` 908, `session_ops` 327) —
    the "active re-points all leaves" mechanism; the change's center of gravity.
  - `active_graph_id` / `leaf_graph_id` (`frame_ops` 127/138) — a new leaf binds to the
    active graph; should bind to the focused pane's graph.
  - `refresh_session_thumbnails` (`session_ops` 144) — `id == active_session_id` picks the
    live orrery; generalize to "any session whose graph is pooled (any pane)" (the
    eviction was already made pane-aware this way, `21d9821`/`85f307c`).
  - `switch_session` / `cycle_session` / `close_session` (`session_ops` 197/222/242) and the
    switcher highlight (`render` 1134) + F2 rename (`input` 752) — all read
    `active_session_id`; re-key to "the focused pane's session."
  - Window-cache reset in `load_active_session` (clears `pages` / `scrying` / `live_previews`
    / textures) assumes a full session swap; with pinned panes a focus change must **not**
    clear them (both graphs' content coexists; `pages` is already URL-keyed, graph-agnostic).

  **Increments (deliberate, watched — not a blind sweep).** (1) `session_for_graph` +
  per-pane `save_session(graph_id)`; (2) `focused_pane` (which Orrery leaf has focus) +
  `focused_graph` derives from it; (3) retire `retag_graph_bound` from the focus path (panes
  pinned; switch re-points only the focused pane); (4) re-key the switcher / rename /
  thumbnails / `leaf_graph_id` off the focused pane; (5) per-pane pointer input (dispatch to
  the pane at the cursor; click sets `focused_pane`; nav/save resolve through it). Cost is
  real but contained to the session lane + the input dispatch; it is the clean foundation
  that makes input, save, and nav per-pane all *fall out* instead of each being bolted on.
- 2026-06-14: **P2 landed (the load-bearing half) + frame_ops carved under the ceiling.**
  Two milestones, both behaviour-preserving, all green (84 bin tests throughout):
  - **frame_ops split, 2739 → 375 LOC** across nine cohesive modules, each under the
    600 ceiling (the audit's "split it, don't grow it" risk, cleared before the sweep):
    `frame_a11y` + `frame_a11y_panes` (a11y projection), `pane_geom` (leaf rects /
    hit-tests / divider), `pane_data` (roster / steward / utility rows), `nav_sync`
    (location + history + physics-toggle), `menus` (context + shellbar), `command_drain`
    (omnibar / palette / comms dispatch), `session_ops` (save / rename / thumbnails +
    the Shell create/switch/cycle/close), `node_ops` (focused-node + content/node-state).
    Eight pure-move commits (`ea13654`..`200927b`).
  - **P2 sweep (`30d5570`): the ctx resolves the orrery from the pool.** `WindowCtx`
    no longer bundles one `orrery: &mut Orrery`; it borrows the whole pool
    (`orreries: &mut HashMap<GraphId, Orrery>`) and the ~110 `self.orrery.x` sites
    resolve through `orrery()` / `orrery_mut()` keyed on `view.focused_graph` (P1's
    exact bundling key, so resolution-identical). This is the mechanical move that
    makes per-pane resolution *expressible*.
  - **Render per-pane (`d124040`): render drives the Orrery pane's own graph.** A
    `LaidLeaf` now carries its `graph_id` (threaded through `leaf_rects` / `find_leaf`),
    so render reads the Orrery leaf's graph and drives that pooled orrery
    (`pane_orrery` / `pane_orrery_mut` by `graph_id`), not the window-focused one.
    One Orrery pane → identical; a second of another graph drives its own.
  - **Finding — `focused_graph_id()` is NOT yet a drop-in for `orrery()`.** The plan
    framed it as "cleanly derivable (no new tracking)". But `load_active_session`
    resolves + drives the re-keyed orrery (`set_camera`, `select_by_url`) **before**
    `retag_graph_bound` retags the leaves, so an active-content-leaf-derived key would
    resolve the *outgoing* graph mid-switch. The focused accessors must keep reading
    `view.focused_graph` until that retag is moved ahead of the orrery resolution.
    (Render is safe — it reads the leaf's stamped `graph_id` after layout, not a
    derivation.) The derived accessor was written then removed (premature, no safe
    consumer); reintroduce with the retag re-order, or for the cursor→pane hit-test.
  - **Remaining P2 tail (gated on a 2nd-graph-pane UI, ~P3):** iterate Orrery leaves
    in render to draw 2+ graphs at once; route the input pointer / wheel to the Orrery
    pane *under the cursor* (the P1-scout soft-spot, still on the focused orrery —
    correct for one pane). **Updated 2026-06-15:** the summon affordance now exists.
    `OpenGraphBeside` (Shift+click a switcher tile, `input.rs:370` →
    `session_ops.rs:470` → `app_handler.rs:670`) draws a second Orrery pane of another
    graph, with per-pane render + wheel/hover live (`85f307c`, `21d9821`, `d124040`).
    The remaining tail is per-pane *pointer-select / nav*, blocked on the
    focus/active-session decoupling (the 2026-06-14 scoping note above), not on a
    missing affordance.
- 2026-06-14: **Audit (code-verified against the tree).** P1 confirmed done (pool +
  `focused_graph` + `Activation.graph_id` stamp + `reap_graph` + park/unload/LRU +
  Steward live-count all present; OQ2 resolved). **The P2-companion list-pane
  view-ification has SHIPPED** (the entry below scoped it; it then landed and the
  Progress log missed it): roster / inspector / steward / apparatus are now
  `ViewPane`-driven (`view_pane.rs`, `roster_view.rs`/`RosterPane`,
  `list_pane.rs`/`ListPane`), and `build_roster_dom`, `roster_row_rects`,
  `build_utility_pane_dom`, `apparatus_button_rects`, `roster_row_at` are deleted —
  the rect caches + per-frame DOM rebuilds for these panes are gone, replaced by
  runner dispatch over a cached `PaneSession` (the C5 cheap-path). **Still open:**
  - **P2 (per-pane resolution) is unstarted** — render.rs / frame_ops still read the
    single ctx-bundled `self.orrery` (~68 sites); `leaf_graph_id()` exists but is not
    yet the per-pane render/input resolver. This is the next mere-lane phase, and it
    is mechanical (the P1 site-scout's three buckets are its spec). **Risk:**
    `frame_ops.rs` is ~2.7k LOC (4.5× the 600 ceiling) and P2 concentrates there —
    split it, don't grow it.
  - **The external-texture element view is the unstarted lynchpin** — no DOM
    element/view exists yet (only the host-compositor `ExternalTexturePlacement`).
    It gates the content-pane input spine (G1.1/G1.3), interactive in-graph DOM
    (G1.2), and **pelt V6** (workbench-pane → pelt surface). It lives in
    xilem-serval / genet-scripted-dom — the **genet/pelt agent's repo**, so the
    content half of the P2 companion is coordinated, not solo. The workbench pane's
    current `platen_view` internals are throwaway pending that convergence.
- 2026-06-13: **P2-companion scout — list-pane view-ification, the pelt-informed design.**
  Code-checked the input spine before building. The three systems, concretely: **chrome**
  is the good pattern (`chrome_view(&Chrome) -> ChromeView` declarative views with
  `on_click(el(...), handler)`, driven by a `GenetAppRunner` that diffs into a persistent
  DOM and dispatches input, no rect caches); **list panes** are the debt (per frame
  `build_roster_dom`/`build_utility_pane_dom` rebuild a fresh `ScriptedDom` →
  `scene_from_*` → rasterize → `compose_external_texture` into the pane rect, with the
  interactive ones bolting on hand-maintained `WindowView` rect caches —
  `roster_row_rects`, `apparatus_button_rects` — rebuilt in render + hit-tested in
  frame_ops, plus a separately hand-built UxTree); **orrery/cards/gloss** are scene
  composition (not a view target — that is the external-texture-element piece).
  - **The pelt lesson (`genet/ports/pelt-desktop/chrome.rs`):** a view-driven pane is one
    self-contained struct bundling its runner + sheets, exposing `frame(w,h) -> Scene`,
    `hit_test(x,y,w,h) -> Option<NodeId>` (lays out inline, no stored session field),
    `dispatch_click(node)` / `dispatch_key`, and `take_intents()` / `state()`. The shell
    just calls those. Meerkat already runs two runners (chrome, `workbench_runner`), so a
    third pane runner has a precedent; the cleanup is bundling it pelt-style rather than
    scattering `dom`/`session`/`runner` across `WindowView` + the rect caches.
  - **Roster slice (first, startable now):** a `RosterPane` bundle over `roster_view(&Roster
    State)` whose rows carry `on_click` queuing `Select`; `set_rows` from `roster_rows()`
    each frame; render calls `frame()`+compose; input calls `hit_test`+`dispatch_click`
    then drains the select intent; the runner DOM becomes the a11y tree. Deletes
    `roster_row_rects`, `build_roster_dom`, `roster_row_at`, and the hand-built roster
    UxTree. Confirm-on-build: the row-click action (member-select vs `SelectNodeByUrl`,
    [frame_ops a11y path]) and the `el` text-vs-children content API. Apparatus follows the
    same shape; Steward/Inspector are the display-only conversions; then extract a shared
    component (pelt kept `Chrome` concrete and would generalize on the second consumer).
- 2026-06-13: **Multi-graph increment landed — two graphs coexist. P1 is done.** Built
  from the scouted brief, in two commits, behavior-preserving at each step, 114 green.
  - **Gating restructure (`16b02b9`).** `create`/`switch`/`cycle`/`close_session` +
    `load_active_session` moved from `impl WindowCtx` to `impl Shell`, where the whole
    pool is mutable. **One refinement vs the scout:** they do *not* take a `window_id`.
    The harness boots with `primary: None` (the view is `pending_view`), and a
    `WindowCtx` carries no id, so the ops resolve the focused view primary-or-pending the
    way `ctx()` does, and re-enter `self.ctx()` for the WindowCtx-shaped sub-steps
    (save / cache-reset / thumbnails). Per-window input handlers can't reach `&mut Shell`,
    so they push new `ShellCommand::{CreateSession,SwitchSession,CycleSession,CloseSession}`
    and `Shell::apply` drains them after the ctx borrow ends — the existing spawn/close seam.
  - **Multi-graph payload (`aba14ba`).** `Activation` gained `graph_id`, stamped at spawn
    in `reconcile` (which now takes `(member, graph_id)` pairs; `render`/`frame_ops` pass
    `focused_graph`) — exactly the scout's "stamp at spawn, not `drive`." `drain` pairs
    each contribution with its node's graph; `constellation.clear()`-on-switch became
    `reap_graph(outgoing)` so another live graph's actors survive (the blast-radius item).
    `load_active_session` mints a *distinct* pool entry per graph (`Orrery::with_graph` +
    its own `offload_physics`, the wake now held on `Shell`) or reuses a still-pooled one,
    instead of re-keying one entry. Contribution routing applies the *focused* graph's
    here; a background graph's routes to its pooled orrery — but there are no live
    non-focused actors at P1 (a switched-away graph's are reaped), so that path is **P2**
    (two panes, two graphs).
  - **Verified three ways:** 114 unit tests; a clean runtime boot (graph restored from the
    pool, frames settling, zero panic/warn); and an **on-screen round-trip driven by
    injected keystrokes** — session A (node "4") → Ctrl+PageDown → empty "New" graph →
    Ctrl+PageUp → node "4" still there, served from the live pooled orrery, not reloaded.
  - **OQ2 resolved** (`7a8304b` unload, `0f94f04` park, `a23fc7c` Steward count). **Re-
    scoped by park:** per-orrery save on
    *exit* is now near-moot — park halts a switched-away graph's physics, so it stops
    changing after its switch-away save (only sub-pixel drift between save and halt, which
    re-settles on reload); physics-wake fan-out's P1 case is handled (parked graphs no
    longer wake), leaving only the multi-window "route a graph's wake to its own windows"
    routing, which is inherently **P2**. The P2 items (per-pane resolution, non-focused
    contribution routing, that wake routing) follow the P2 companion.
- 2026-06-12: **P2 companion added** (the xilem-serval input-spine target), prompted by
  the host-wiring grabbag plan completing its genet-side seams with their meerkat
  callers correctly recorded as blocked-on-composition. Records the honest usage
  inventory (two runners; chrome-only dispatch; three parallel input systems), why the
  current state was deliberate staging but a bad place to stay, and the landing order:
  pane view-ification (startable now) → external-texture element (the content bridge,
  unblocks G1.1/G1.3 callers) → orrery pane-boundary routing (G1.2 with interactive
  in-graph DOM) → Enter/Space after key routing unifies through `dispatch_key` (Tab is
  additive immediately).
- 2026-06-11: Plan written, superseding the multi-window plan's MW4–MW6. First reframed
  the second window from "workbench-only leaf to dodge the orrery split" to a window-level
  `(content, linkage)` model, then (with Mark) corrected that to the right model: **orrery
  (authority) vs panes (views that resolve to an orrery by `graph_id`)**. There is no
  window kind; a window is a split of panes, each resolving to its orrery, co-located or
  not, possibly to different graphs. Linkage is emergent (same orrery = synced). Grounded
  on the verified fact that the constellation is UUID-keyed and graph-agnostic, so orreries
  pool by `GraphId` with no graph extraction (only a same-graph two-camera case needs the
  camera pulled per-pane — deferred). MW3 step 4 part 2 (the workbench-only-leaf interim)
  is **dropped** for P3 (cross-window pane resolution); the half-built orrery-isolation
  gate was reverted, returning in P1/P2 as "this pane is not an `Orrery` pane," not a
  slim-flag. P1 (the orrery pool) **converges with far-B / MG6** (many graphs live at once).
- 2026-06-11: **member→graph mapping decided (with Mark): stamp the active `Activation`**
  (see Findings). The one P1 lifecycle item carrying a real design choice — where the
  member→graph mapping lives — is settled before the mechanical pool move: a `graph_id`
  on each active constellation entry, set at `drive`-time, routing contributions to their
  orrery. No global index, no pool search. P1 is unblocked.
- 2026-06-11: **P1 foundation landed (commit `4dddf0f`), behavior-preserving, 44+66
  green.** The single `Shell.orrery` is now `Shell.orreries: HashMap<GraphId, Orrery>`,
  seeded with the primary's orrery keyed by the active graph; `WindowView` gained
  `focused_graph: GraphId`. **Two implementation refinements vs the sketch:** (1) the pool
  is a **`Shell` field, not in `SharedState`** — it must borrow disjointly from `shared`
  / `view` exactly as the old single `orrery` did, and `SharedState`-nesting would alias
  every `self.shared.X` access; (2) resolution is **"the ctx bundles the window's focused
  orrery as `self.orrery`"** (resolved once in `ctx()`/`window_ctx()` from
  `view.focused_graph`), *not* a per-site `shared.orreries.get(graph_id)`. That kept the
  ~60 `WindowCtx` sites **unchanged** — the scout's clean-sweep verdict confirmed in
  practice (only the test harness's Shell-level `app.orrery` needed new `orrery()` /
  `orrery_mut()` resolvers). The pool is a single degenerate entry for now; session-switch
  still replaces the focused orrery's graph in place. **Remaining P1 (the multi-graph
  increment):** `switch_session` re-keys the pool into distinct live entries + updates
  `focused_graph`; the `Activation` `graph_id` stamp + contribution routing across the
  shared constellation; per-graph physics offload. Per-pane resolution (a window hosting
  panes on *different* graphs) stays P2 — P1 is one focused orrery per window, pooled.
- 2026-06-11: **Multi-graph increment scouted to the bottom — it is one coupled chunk,
  gated on a session-op → Shell restructure.** Confirmed by reading the code: (1) the
  pool is only mutable at the **Shell level**. The ctx bundles *one* `&mut Orrery`
  (Approach A), so it cannot re-key or insert pool entries; `load_active_session`
  (`frame_ops`) and its thin callers (`create`/`switch`/`cycle`/`close_session`) must
  move to `impl Shell` (taking a `window_id`) to give a switch its own distinct pool
  entry instead of re-pointing the shared one in place (`set_graph`). This is the gating
  piece — without it there is never a second pool entry. (2) The **stamp threads at
  spawn, not `drive`**: `Constellation::drive` returns early for an inactive member
  (`constellation.rs:237`), so the `graph_id` must be set when the `Activation` is
  created in the needed-set reconcile (where the requesting pane's graph is known), not
  in `drive`. (3) Per-graph **physics** offloads when an orrery enters the pool — which
  only happens at the new Shell-level load. So all three pieces are inert until the
  session-op restructure exists; there is **no cleanly-separable green sub-increment**,
  and building the stamp or physics first would be unexercised scaffolding. The increment
  is best done as a focused unit with on-screen verify (two graphs coexisting). The P1
  *foundation* (pool + ctx resolution, committed + proven) stands as the boundary; the
  multi-graph increment starts from this fully-scoped brief.
- 2026-06-11: **Review folded in** (code-verified pass). Verified: leaf `graph_id`
  (frame lib.rs:247, with the serde-default migration), `GraphMemberId = Uuid`,
  per-orrery physics is one `offload_physics(wake)` call each. Added the pool's
  lifecycle blast radius to P1 (constellation clear-on-switch must become per-graph
  reap — the constellation has no graph dimension yet; scrying pins ride the same path;
  the shared tab cap; per-orrery persistence to each manifest's dir; physics wake
  fan-out), the kindless exit/save rule + the one-compat-WebView-per-window constraint
  to P2, follow-vs-pin to OQ1, park-vs-unload + a Steward live-count tripwire to OQ2.
  Title corrected ("orrery-per-window" → pooled authorities; the retired two-axis
  framing scrubbed); lineage facet re-attributed to `graph/node-lineage`;
  `PaneContent::Alembic` marked aspirational (not in the enum yet; Steward is).
