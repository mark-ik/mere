# Window Composition Plan — pooled orrery authorities, pane resolution, cross-graph composability

**Date**: 2026-06-11
**Status**: Planning. Supersedes the [multi-window plan](2026-06-10_multi_window_plan.md)'s
**MW4–MW6** (leaf tear-out / branch+fork / orrery-split). MW1–MW3 (the per-window
reshape: `WindowCtx` seam, `WindowId` registry, one-device/N-surfaces, spawn/close,
slim leaf chrome) are done and stand; this plan builds on them. The
[tear-out brief](../research/2026-05-11_tearout_operations_brief.md) is the design
source for the leaf/branch/fork gesture model; this plan is the architecture those
operations rest on once a window can own its own orrery.
**Code**: `crates/meerkat/`, `crates/orrery/`, `crates/forme/`, `crates/system/session-runtime/`.

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

### P3 — Cross-window pane resolution (the leaf, done right)

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

Move or copy a tile/node across orreries. Copy mints a node in the destination orrery via
the cross-graph rekey (tear-out brief §7.5) and records a **provenance edge** (origin:
source node) + lineage. Move re-points the binding. Surfaced as a drag between two panes
resolving to *different* orreries (and a palette/command form).

Done when a tile dragged from a pane on graph A into a pane on graph B produces a node in
B with provenance + lineage back to A, and the source is left intact (copy) or releases
its binding (move).

### P5 — The tear-out gesture model (leaf / branch / fork + toast)

Implement the [tear-out brief](../research/2026-05-11_tearout_operations_brief.md) on top
of P1–P4: drag = leaf (a `Workbench` pane resolving to the donor's orrery), Shift+drag =
branch (donor's orrery, new `GraphletId`), Ctrl/Cmd+Shift+drag = fork (a fresh orrery via
the P4 rekey, + a thin `Orrery` pane), toast on ambiguous drag. Spawn-on-drop with an
in-donor drag ghost (the portable shape from MW4).

Done when all three operations run from the gesture model with the brief's identity
semantics, and the toast escalates a leaf in place.

### Deferred — per-pane camera (two spatial views of one graph)

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
3. **Provenance edge direction + family** — confirm the existing provenance edge family
   carries "copied-from across graphs" cleanly, or whether copy wants a distinct sub-kind
   (P4). Check before building, per the consumer-pull rule.
4. **Move vs copy default** — a cross-graph drag defaults to which? Likely copy (safer,
   provenance-tracked); move on a modifier. Decide in P4.
5. **Linked-tile lifecycle on donor delete** — the brief §8.3 cascade rules (leaves lose
   their node, branches die, forks survive) map onto the linkage axis; wire them in P3/P5.

## Progress

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
