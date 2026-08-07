# Graphlet Wiring Plan — give graphlets a live home in the shell

**Date**: 2026-06-25
**Status**: **COMPLETE (v1), 2026-06-26.** Phases 1-3 done; open items #1 and #3 done + driven.
The graphlet-wiring subsystem is live end to end; the open tail (relational-browse consumer, the
full reconcile-proposal + family-picker UI, the Corridor / Loop / Frontier / Facet kinds) is
cross-plan or enhancement, tracked in the **Close-out** section. Branch (Shift+drag) mints a
persisted `Branched` graphlet
(round-trips a restart), reads as a distinct branch (an accent `⎇ <anchor>` chip), and
accumulates its own lineage as you navigate in it. Per-window focus/selection isolation now
makes two windows on one graph independent (driven), which resolved the Slice 2
cartography-focus nuance and unblocks Slice 3. **#3 (branch lifecycle on donor delete) is
done + driven** — deleting a session closes its windows and drops its pooled state, while
forks survive. **Phase 2 slice 3 (branch-scoped orrery) done + driven** — a branch window
renders only its graphlet's members. **Phase 3 slice 1 (derivation + Linked path + reconcile)
done + tested** — `Component` / `Ego` graphlets derive from the kernel graph and reconcile on
drift. **Phase 3 slice 2 (manual Linked consumer) done + driven** — an "Open component" node
action mints a `Linked { Component }` graphlet (12 derived members) + opens a scoped window.
**Slice 2+ done**: a Linked window tracks graph drift live; the persisted roster auto-reconciles
on save; derivation honours an **edge projection** (a spec's `selectors` filter which relation
families the walk follows, so the same nodes derive different shapes); and three consumers surface
it from the UI (**Open component**, **Open neighborhood** = `Ego { radius: 2 }`, **Open link web**
= `Component` under the Semantic projection) via one generalized `OpenLinkedGraphlet { kind,
selectors }` command. Remaining (cross-plan / enhancement, see Close-out): the relational-browse
consumer, the full reconcile-proposal + live family-picker UI, and the Corridor / Loop / Frontier
/ Facet kinds.
Spun out
of the [tear-out gestures plan](2026-06-24_tearout_gestures_plan.md) (G3 / OQ-7 deferral)
when scouting the forme graphlet API showed the whole layer is built but unwired.
**Lane / conflict posture**: meerkat session/state + a small forme surface (reusing its
`GraphletRef` type). No kernel or orrery changes in Phase 1. Touches per-session
persistence (a new sidecar) and the tear-out branch op.
**Consumers** (why this is worth building, beyond branch): the tear-out **branch**
operation (G3) is the immediate one, but the same per-session graphlet index is what
**document-groups** and the
[relational-browse front-end](../2026-08-06_completed_plans/2026-06-23_relational_browse_graphlet_plan.md) both want —
a named sub-structure over the graph that the workbench and orrery can scope to.

---

## Findings (the 2026-06-25 scouting)

What is actually in the tree, verified against the code:

- **The forme graphlet API is first-class and unit-tested.** At `crates/forme/forme`
  (not `crates/graph/forme` — the old plan note grepped the wrong path):
  `GraphletId` (a `u32`), `GraphletRef<N: MemberId>` (anchors + binding + kind),
  `GraphletBinding::{UnlinkedSession, Linked, Branched}` (graphlet.rs), 9 `GraphletKind`
  shapes, `GraphletSpec`, projection specs, and full reconciliation types
  (`GraphletMemberDelta`, `ReconciliationProposal`, `ReconciliationChoice`). 114 forme
  tests pass.
- **`GraphMemberId = Uuid`** (arrangement.rs) — the kernel node UUID, with a blanket
  `impl<T> MemberId for T`. So `GraphletRef<GraphMemberId>` compiles, and a torn node's
  uuid (which the tear gesture already has) is a valid graphlet **anchor** with no
  translation.
- **The layer is unwired** (2026-06-25 scouting snapshot; the wiring below changed this). At
  scouting, a workspace grep found **zero** live construction of a `GraphTree` or `GraphletRef`
  outside forme's own tests, and meerkat imported exactly one forme item — `GraphMemberId`. The
  wiring since added `GraphletId` / `GraphletSpec` / `GraphletKind` / `GraphletBinding` too.
- **forme's `GraphTree` is superseded, not just unused.** `GraphTree<N>` ("the core data
  structure, one per graph view") carries graphlets plus members, topology, a projection
  lens, and layout. But the **live** arrangement is the other model: platen's `Workbench`
  (its own recursive split-tree of tab-stacks, which explicitly "replaces the legacy
  `FrameState` / `PaneBinding` frame model") plus the orrery. The architecture comment
  in `platen/workbench.rs` frames the orrery and the workbench as "two **projections of
  one arrangement**" — and that arrangement is platen + the kernel graph, **not** forme's
  `GraphTree`. So resurrecting `GraphTree` would mean re-deriving members / topology /
  layout that the live model already owns.

**The consequence for branch:** branch's prerequisite is not "mint a `GraphletRef`" (the
type exists) and not "instantiate a `GraphTree`" (it would duplicate the live model). It
is the one genuinely-missing thing: **a per-session graphlet index** — a place graphlets
live, keyed by the same `GraphMemberId`s (kernel uuids) the workbench and orrery already
use. Without it a branch graphlet is an orphan and branch is indistinguishable from leaf.

---

## The central design choice (ratify before Phase 1 build)

Where do graphlets live?

- **(A) Graft onto platen's `Workbench`.** Add a graphlets field to the workbench model.
  Pro: graphlets sit with the live tiling. Con: pushes graph-truth concepts into the
  geometry-free tiling crate; the workbench is per-window, graphlets are per-session.
- **(B, recommended) A per-session graphlet index, reusing forme's `GraphletRef`.** Hold
  `Vec<GraphletRef<GraphMemberId>>` (wrapped as `SessionGraphlets`) in the session state,
  beside the graph / camera / frame. Reuse forme's tested `GraphletRef` type; skip the
  superseded `GraphTree` container. Anchors are kernel uuids, so it keys directly to the
  live graph. Pro: minimal, honest, branch-shaped, no duplication of members/topology.
  Con: forme's reconciliation functions take `&mut GraphTree`, so they are not reusable
  as-is — deferred to Phase 3, which decides whether to adapt them to the live model.
- **(C) Resurrect `GraphTree` wholesale as the arrangement authority.** The full
  "composition spine" vision — forme `GraphTree` becomes truth, platen + orrery project
  it. **CLOSED (2026-06-25).** See the derivation finding below: even in the richest
  scenario (Linked, auto-deriving graphlets), `GraphTree` adds nothing, because the
  valuable code that scenario needs (the per-kind derivation) does not exist in forme and
  would not live in `GraphTree` if it did. C is dominated.

**Decision: (B), and C is closed.** B unblocks branch with no duplication, and the
derivation finding makes C dominated rather than merely deferred. forme stays a **type
dependency only** (`GraphletRef` / `Spec` / `Kind` / `Binding`, plus the reconciliation
*data types* if/when needed); `GraphTree` stays unused.

### The derivation finding (2026-06-25) — why C is closed

A grep of forme for any kind dispatch, BFS / neighborhood walk, shortest-path, or selector
matching found **none**. The 9 `GraphletKind`s are enum tags; `selectors` are opaque
strings (forme "does not own the relation-selector vocabulary"); nothing matches on the
kind to compute members. The only `fn derive*` build `GraphTree`'s own topology, not graph
shapes. So "the full forme model" is a taxonomy + a binding enum + reconciliation *data
types* + a ~10-line set-diff (`compute_roster_delta`) + tree-bookkeeping over `GraphTree`'s
own state. The one algorithmically valuable thing for auto-derived graphlets — computing an
Ego / Component / Corridor from the graph — **is not in it.**

Consequence: a consumer wanting Linked graphlets needs (a) **derivation** (Ego = BFS to
radius N, Component = unbounded BFS from the seed = its connected component, selector =
edge-family filter — all kernel-graph work, where the primitives and the relation
vocabulary live), (b) **storage** (`GraphletRef`, already reused), (c) **drift
reconciliation** (the ~10-line diff + the plain `GraphletMemberDelta` / `ReconciliationProposal`
/ `ReconciliationChoice` types). None of (a)/(b)/(c) needs `GraphTree`. So the full
capability is reachable by harvesting forme's *types* and the diff logic and writing
derivation on the kernel graph. The second consumer no longer chooses B vs C; it only
chooses how much to harvest (open item OQ-2 below).

---

## Phases

### Phase 1 — Substrate: a live, persisted per-session graphlet index — DONE (2026-06-25), driven

Make graphlets real and durable; give branch a distinct identity.

- `SessionGraphlets` (the [stub](../../../crates/meerkat/src/graphlets.rs)) held per
  session, beside the graph. A fresh session seeds one default `Session`-kind graphlet.
- Persistence: a sidecar beside `graph.json` (`graphlets.json`), saved/loaded on the
  existing per-session save path. Survives restart.
- The window carries a `GraphletId`: a torn branch window records which graphlet it is.
  (Open: where the id rides — `WindowView` view-session bits vs the frame leaf.)
- The branch op (G3): on Shift+drag, `record_branch(anchor, parent_spec)` mints a
  `GraphletRef` with `GraphletBinding::Branched`, anchored on the torn node's uuid, and
  the torn window carries the new id. Replaces the current leaf-stub on the branch path.

**Done when:** Shift+drag mints a real `Branched` graphlet in the donor's session, the
torn window carries its `GraphletId`, and both survive a restart. Branch is now
identity-distinct from leaf even before it is visually distinct.

### Phase 2 — Projection: make the branch graphlet mean something

A graphlet that nothing reads is still ≈ leaf. Give it visible scope + lineage.

**Slice 1 — visible branch marker — DONE (2026-06-25), driven.** A `Chrome::branch_label`
shown as an accent `.branch-chip` pill in the branch window's toolbar, naming the anchor
(`⎇ <node label>`). Set when the branch window spawns (the `BranchNode` dispatch resolves
the anchor's `node_display_label`). `None` on leaves / the primary (hidden via a class,
like the crawl chip). Drove headed: the branch window shows the chip + shares the donor's
graph; a leaf shows none. Branch now reads as distinct from a leaf.

**Slice 2 — lineage accumulation — DONE (2026-06-25), driven.** Trigger (Mark's call):
**navigation**. The nav choke point `sync_orrery` calls `record_branch_nav(member)` for the
navigated node; on a branch window (`view.branch_graphlet.is_some()`) it pushes
`ShellCommand::RecordBranchMember { graph, graphlet, node }`, handled on `Shell`
(`SessionGraphlets::add_member` — dedup'd — + persist) via the same command seam `BranchNode`
uses (no ctx threading). Unit-tested (`add_member` grows + dedups) and **driven**: navigating
the branch window to a fresh URL grew its graphlet roster from `[anchor]` to `[anchor,
new-node]` in the sidecar, while the donor's default graphlet stayed empty — the lineage
diverges while the new node joins the shared graph.

**Note (focus model) — RESOLVED (2026-06-25) by open item #1.** Lineage records the
*navigated* node = the focus. That was the per-window `focused_tile` in workbench mode but
the *shared* `Orrery::focused_member` in cartography mode. Open item #1 (per-window
focus/selection isolation) made cartography focus per-window too, so a branch's lineage now
records *its own* focus in either mode.

**Slice 3 — orrery scope — DONE (2026-06-25), driven.** A branch window's orrery renders
only its graphlet's members (a filtered cartography view). Per-window via save/restore (only
a branch window touches the scope; its ctx saves the orrery's prior scope, sets
`scope_to_members(live roster)`, and `Drop` restores it, so the donor's own scope never
leaks): `WindowCtx` gained a read-only `graphlets` ref + a `branch_scope_restore` slot, with
`install_scope` / `restore_scope` in viewport.rs beside the camera/selection install/readback.
Driving surfaced that the host's gnode builder (render/orrery_scene.rs) iterated *all* nodes, ignoring
the orrery's scope — fixed with an `Orrery::node_in_scope` filter so the gnodes match the
scene's own scope filter. Drove headed: branched a node → its window showed **only** the
anchor node, while the donor showed the whole graph (scope did not leak). meerkat 89 lib /
172 bin + orrery 80 green.

**Done when:** a branch window shows a distinct grouping (Slice 1 ✓) that accumulates its
own lineage (Slice 2 ✓) while node edits still propagate to the donor (✓ — the navigated
node joins the shared graph). **Met** (modulo the cartography-focus note above).

### Phase 3 — Linked / auto-derived graphlets + reconciliation (the richer model)

**Decision (Mark, 2026-06-25): yes — we want selectors that track drift.** So Phase 3 is
in scope (not closed to "richer kinds as plain data"). Per the derivation finding, this is
built by **harvesting forme's types + the diff and writing derivation on the kernel graph**;
`GraphTree` is not used.

**Slice 1 — derivation + Linked path + reconcile — DONE (2026-06-25), kernel-tested.**
Kernel derivation primitives `Graph::component_members(seed, selectors)` (whole connected
component, BFS) and `Graph::ego_members(seed, radius, selectors)` (radius-bounded BFS), sharing
a `bfs_members` (the `selectors` edge-projection param was added in the selectors sub-slice)
helper — kernel-tested green (component: reaches the component, isolates stay singletons,
unknown seed empty; ego: radius 0/1/2 bounded). In meerkat `graphlets.rs`:
`derive_members(graph, spec)` dispatches the kind to the kernel primitive (`GraphTree`-free);
`SessionGraphlets::record_linked(graph, spec)` mints a `Linked { spec }` graphlet whose
`anchors` hold the live derived set and `spec.primary_anchor` the seed (the anchors-vs-members
split, OQ-2-a); `reconcile(graph, id)` re-derives, diffs against the live roster with the
harvested `compute_roster_delta` logic (reusing forme's plain `GraphletMemberDelta`), and
auto-applies, returning the delta. Integration tests `linked_component_graphlet_derives_and_
reconciles_on_drift` and `linked_ego_graphlet_is_radius_bounded` pass (kernel 255, meerkat 89
lib / 169 bin green; a concurrent steward/eidetic refactor briefly broke the meerkat build mid-
slice but settled). **Slice 2 — manual Linked consumer — DONE (2026-06-25), driven.** Mark's call: a manual
command. A `ContextAction::OpenComponentGraphlet` (node context menu "Open component" +
palette "Open component as graphlet") → `ShellCommand::OpenLinkedGraphlet` →
`Shell::linked_component_graphlet` mints a `Linked { Component }` graphlet derived on the
focused node, then opens a window scoped to it via the branch-window scope path (Slice 3),
with a distinct `◎` chip. **Drove it (re-drive 2026-06-25):** focused a node, ran the
palette command → `graphlets.json` got a `Linked { Component }` graphlet whose `spec.anchors`
is the seed and whose `GraphletRef.anchors` is the **12 derived members** (the seed's whole
connected component, from `component_members`) — the anchors-vs-members split working — and a
new window opened scoped to that component. (The first attempt was harness focus friction;
verifying the node stayed focused through the palette fixed it.) meerkat 89 lib / 172 bin +
kernel 255 + orrery 80 green.

**Slice 2+ — live drift for the Linked window — DONE (2026-06-25), tested.** `install_scope`
now re-derives a `Linked` graphlet's roster from the live graph each pass (a `Branched` one
keeps its accumulated roster), so the scoped window tracks graph drift — a node added
elsewhere joins the component and appears live. `derive_members` is read-only, so this needs
no `&mut`. Re-deriving every pass is fine at demo scale (a revision-cached derive is the
refinement). meerkat 89 lib / 172 bin green. (Not separately drift-driven — a multi-window
add-a-node drive is fiddly and the derivation is unit-tested; the visible-scope path is driven
in Slice 2.)

**Slice 2+ — auto-reconcile on drift (data level) — DONE (2026-06-25), tested.** `save_session`
queues a `ReconcileGraphlets { graph }` when the focused graph has a Linked graphlet;
`Shell::reconcile_linked_graphlets` runs `SessionGraphlets::reconcile_all` (re-derive + diff +
auto-apply each Linked graphlet) and persists on a real change (idempotent — a no-op when
nothing drifted). So a Linked graphlet's *persisted* roster tracks graph drift too, not just
its window. Unit-tested (`reconcile_all_updates_linked_rosters_on_drift`: connect a node →
`reconcile_all` reports the change and the roster grows). meerkat graphlets tests green (6/6);
the lib toolbar test was red on a concurrent chrome button refactor (not ours).

**Slice 2+ — selectors / edge projection — DONE (2026-06-26), tested.** `component_members`
and `ego_members` now take a `&[RelationSelector]` edge projection: the BFS only follows an
edge whose payload matches a selector (empty = all families), via the new `has_relation`-based
`edge_matches_selectors`. So the *same* nodes derive a different shape under a different
relation lens. `derive_members` maps a spec's opaque `selectors` strings (family names,
case-insensitive) to `RelationSelector::Family`. Kernel test `the_selector_projection_changes_
the_derived_shape` (A—Semantic→B—Containment→C: all = 3, Semantic-only = 2, Containment-only =
1). Highest-leverage gap closed: the kinds are now *relational*, not just topological. Still
seed-mapped: the **vocabulary control** that *sets* selectors (the projection toggle, see the
Controls note), and sub-kind selectors (only `Cites`, not all Semantic).

**Controls — where the projection lives (graphlet vs surface).** A graphlet is
surface-independent: a member set plus a derivation rule (kind + selectors). The gloss strip,
a cartography swatch, and the main canvas are *form factors* of a Navigator over a scope, so
the controls split on a WHAT/HOW seam. **WHAT** (scope + shape = kind + selectors) is the
*graphlet's*, shared by every surface showing it (change once → all re-derive) — this is the
design doc's projection toggle / derivation strip. **HOW** (arrangement = layout strategy +
which graphlet a surface binds) is the *surface's*: `gloss_strategy`, the swatch's cartography
`projection_id`, the canvas's gyre physics, already per-surface. So do **not** duplicate the
selector control per surface. forme's override precedence (`SelectionOverride` >
`GraphViewOverride` > `GraphDefault`) leaves room for a per-surface projection *override* later
(a swatch re-reading one graphlet under a different lens); the default is the baked projection.

**Slice 2+ (still remaining):** wiring the consumer to **relational-browse** neighborhoods
(the original Linked target); the **user-choice reconcile proposal** (keep-linked / unlink /
save-as-branch via forme's `ReconciliationChoice`) — auto-apply is the v0; the projection
vocabulary control (set selectors from the UI); and the remaining kinds (Corridor / Loop /
Frontier / Facet — currently fall back to the seed). The naming rename (OQ-2-b) only bites if
we call forme's `apply_fork` etc.; we harvest the diff ourselves, so it stays deferred.

- **Derivation (kernel graph) — built for Ego / Component + selectors.** A `Linked { spec }`
  graphlet derives its member set from the graph: Ego = BFS to radius N from the anchor;
  Component = unbounded BFS = the connected component; selector match = filter kernel edges by
  family (DONE; sub-kind filtering is a later refinement). The kernel owns the relation
  vocabulary forme leaves opaque. Still to build: Corridor = a path query; Loop / Frontier /
  Facet.
- **Reconciliation.** A ~10-line roster diff (port of `compute_roster_delta`'s body) on
  `SessionGraphlets`, fed graph-truth from the kernel; the plain `GraphletMemberDelta` /
  `ReconciliationProposal` / `ReconciliationChoice` types reused from forme (no `GraphTree`).
  On drift (the graph changed under a Linked graphlet) the host proposes add/remove and the
  user picks (keep-linked / unlink / save-as-branch / cancel).
- **Anchors vs members** (open item OQ-2-a): now live. Separate the seed `anchors` (what
  the derivation runs from) from the derived/accumulated member set (held in our
  `SessionGraphlets` wrapper, since `GraphletRef` has no members field).
- **Naming** (open item OQ-2-b): now in scope. Rename forme's `apply_fork` /
  `detect_fork_on_manual_override` / `ReconciliationChoice::SaveAsNewFork` to the branch
  vocabulary when this path goes live (the `Forked` → `Branched` variant rename already
  landed).
- **Consumers:** branch (a hand-built `Branched` roster, already live) plus the first
  *Linked* consumer — likely relational-browse, whose bird's-eye neighborhood is a derived
  Ego/Component graphlet that should update as the crawl grows. Confirm against that plan.

**Done when:** a `Linked` graphlet derives its members from the kernel graph, reconciles on
drift through the harvested diff + data types, and relational-browse (or another consumer)
reads the same index — all without `GraphTree`.

---

## Open items (scoped 2026-06-25)

**Resolved:**

- **OQ-A — structural choice.** RESOLVED: **(B)**, and **(C) closed** (the derivation
  finding above). forme is a type dependency only.
- **OQ-B — where the `GraphletId` rides.** RESOLVED (Phase 1): `WindowView::branch_graphlet:
  Option<GraphletId>`. `None` = whole-session default. Live-only (secondary windows are
  ephemeral); the graphlet persists in the sidecar.
- **OQ-C — persistence format.** RESOLVED: a `graphlets.json` sidecar (`GraphletRef` is
  `Serialize`). No migration.
- **OQ-2 — Linked / auto-derived graphlets?** RESOLVED (Mark): **yes**, selectors that
  track drift. Scoped under [Phase 3](#phase-3--linked--auto-derived-graphlets--reconciliation-the-richer-model);
  makes the anchors-vs-members split and the forme naming rename live (also Phase 3).

**Open, scoped:**

- **#1 — Per-window focus/selection isolation (the keystone) — DONE (2026-06-25), driven.**
  Mirrored the camera exactly: `WindowView.selections: HashMap<GraphId, Vec<uuid::Uuid>>`
  (member-keyed, so it survives an evict+reload), an `Orrery::set_selected_members` (the
  inverse of the existing `selected_members()`, guarded to skip the reconcile when
  unchanged since it runs every pass), and `install_selections` / `readback_selections`
  beside the viewport calls in `viewport.rs`, with the `Drop` reading both back. Because the
  install/readback brackets the ctx lifecycle, no selection read site changed and focus
  isolates for free (it derives from `selected` via `focused_key`). **Drove headed:** two
  windows on one graph — A selected "appearance" (`settings://pelt/scene`), B was spawned
  (inheriting A's selection on first install), then B selected "a.com" (`https://a.com`); A
  re-rendered on foreground and still showed appearance, B showed a.com. Independent
  selection + focus confirmed. meerkat 89 lib / 167 bin + orrery 80 green. **Deferred:** edge
  selection (`selected_edges`) and transient drags (marquee / orbit) stay shared for v0
  (node selection is the focus + lineage driver); a session-switch ↔ per-window-selection
  interaction is a smaller follow-on. **Payoff:** any two windows on one graph are now
  independent (not just branches); the Slice 2 cartography-focus nuance is resolved; Slice 3
  (a branch-scoped orrery) is unblocked.
- **#3 — Branch lifecycle on donor delete (G6) — DONE (2026-06-25), driven.**
  `close_session` now, after switching the primary to a survivor and trashing the session
  dir (which carries `graphlets.json` to `.trash`), tears down the dead graph: a new
  `Shell::close_windows_on_graph(graph)` closes every *secondary* window whose
  `focused_graph` is the dead graph (branches + leaves die with the donor, brief §4.2;
  forks live on their own `GraphId` so they survive), then `self.graphlets` / `self.orreries`
  / `orrery_lru` drop the dead entries. **Drove headed:** spawned a leaf on the active
  session's graph (window count 2), deleted that session via the switcher × → count dropped
  to **1** (the leaf closed) and the primary switched to the survivor with no crash. meerkat
  89 lib / 167 bin green. As scoped, `close_windows_on_graph` is the *general* multi-window
  session-delete fix (a window must not outlive its graph), not a graphlet one-off.
- **OQ-D — does a branch window get a thin orrery?** Brief says workbench-only (§4.2); G2
  (leaf content) settles the workbench-pane mechanics this rides on. Ties into #1 / Slice 3.

**Suggested order:** ~~#1 (keystone)~~ **done** → ~~#3 (donor-delete cascade)~~ **done** →
~~**Phase 3**~~ **done**. The tear-out-gesture trailing items (toast,
tile-tab leaf origin, G5 move, G2 leaf content, fork restore-into-switcher) live in the
[gestures plan](2026-06-24_tearout_gestures_plan.md), independent of this subsystem.

---

## Close-out (2026-06-26)

**Shipped (this plan is done):** a per-session graphlet index (`SessionGraphlets`, a
`graphlets.json` sidecar) reusing forme's `GraphletRef` over kernel uuids; the **branch** op
(persisted `Branched` graphlet, accent chip, lineage accumulation, scoped orrery, donor-delete
cascade); per-window focus/selection isolation; and the **Linked / auto-derived** path
(kernel-graph derivation for `Component` + `Ego`, an **edge-projection** via `selectors`,
live-drift re-derive in the window, data-level auto-reconcile on save, and three UI consumers:
component / neighborhood / link web). forme stayed a **type dependency only**; `GraphTree` was
never resurrected (the derivation finding closed that). Phase 3's done-condition is met: a Linked
graphlet derives from the kernel graph, reconciles on drift through the harvested diff + forme's
plain data types, and a consumer reads the same index — all without `GraphTree`.

**Handed off (not this plan's to finish):**

- **Relational-browse consumer (potential, not wired)** — the bird's-eye crawl neighborhood
  *could* be a Linked Ego/Component that grows with the crawl, but the
  [relational-browse plan](../2026-08-06_completed_plans/2026-06-23_relational_browse_graphlet_plan.md) as written
  materializes **real nodes** and does not use the graphlet mechanism (its "graphlet" is
  colloquial). Making a browse *mint* a Linked graphlet is an open decision (ruling 7 in the
  [scope model reconciliation](../design/2026-06-27_scope_model_reconciliation.md)); the index
  is live, so it is unblocked when chosen.
- **OQ-D (thin orrery for a branch window)** — workbench-pane mechanics, couples to G2 (leaf
  content) in the [gestures plan](2026-06-24_tearout_gestures_plan.md).

**Deferred enhancements (open when a consumer wants them):**

- **Live family-picker / derivation strip** — the full projection control (toggle Semantic /
  Traversal / Containment / … and watch the shape re-derive), vs today's three presets. A chrome
  design task; the engine + the `selectors` plumbing are done. See the **Controls** note.
- **User-choice reconcile proposal** — keep-linked / unlink / save-as-branch via forme's
  `ReconciliationChoice`; today auto-applies. Triggers the OQ-2-b forme naming rename.
- **Richer kinds** — Corridor (kernel path query, needs a two-anchor trigger), Loop, Frontier
  (the one-hop boundary / candidate ghosts), Facet (property slice). Each is kernel derivation +
  a consumer; low value until a consumer asks. Sub-kind selectors (only `Cites`, not all
  Semantic) sit here too.

---

## Progress

- **2026-06-25** — Plan spun out + stub landed. Scouting (above) established that the
  forme graphlet API is first-class but unwired and that `GraphTree` is superseded by the
  live platen + orrery arrangement, reframing the work from "instantiate a `GraphTree`"
  to "add a per-session graphlet index reusing `GraphletRef`" (recommendation B). The
  stub `crates/meerkat/src/graphlets.rs` (`SessionGraphlets` + `record_branch`) compiles
  and is module-registered but not yet wired into the session lifecycle — Phase 1 wires
  it. Build green.
- **2026-06-25** — **Phase 1 done + driven.** Wired `SessionGraphlets` end to end:
  serde + a `graphlets.json` sidecar (save/load); a `Shell::graphlets: HashMap<GraphId,
  SessionGraphlets>` pool populated on session load (`load_active_session`, load-or-
  default); `WindowView::branch_graphlet: Option<GraphletId>` (OQ-B); a
  `ShellCommand::BranchNode` → `Shell::branch_graphlet_from` (mint a `Branched` graphlet
  anchored on the torn node, persist, open a window on the donor's *same* graph scoped to
  it); and the Shift-tear release path rewired from the leaf-stub to `BranchNode`. meerkat
  85 lib / 158 bin green (incl. 2 graphlets unit tests). Drove headed: Shift+drag wrote
  `graphlets.json` with a `Branched` graphlet anchored on the torn node's uuid + a branch
  window on the donor graph; relaunched and branched again — the new graphlet got `id 2`
  (not overwriting `id 1`) with `next_id 3`, confirming the sidecar round-trips a restart.
  **Phase 1 caveat carried to Phase 2:** the branch window is visually identical to a leaf
  (it shares the donor orrery); the graphlet is real + persisted but does not yet *scope*
  anything. That visible scope + lineage is Phase 2.
- **2026-06-25** — **Phase 2 slice 1 (visible branch marker) done + driven.** `Chrome::
  branch_label` → an accent `.branch-chip` pill in the branch window's toolbar, set when
  the branch window spawns (the `BranchNode` dispatch resolves the anchor's
  `node_display_label`, prefixed with `⎇`). Drove headed: Shift+drag opened a branch
  window carrying `⎇ node:…/info` while sharing the donor's graph — branch now reads as
  distinct from a leaf (the half of the Phase 2 done-condition that is "shows a distinct
  grouping"). meerkat green. **Remaining for Phase 2:** slice 2, lineage accumulation —
  the branch graphlet grows as you work in the window; the trigger (selection vs
  navigation) is an open design choice recorded under Phase 2 above.
- **2026-06-25** — **Phase 2 slice 2 (lineage accumulation) done + driven; Phase 2 met.**
  Trigger = navigation (Mark's call). `sync_orrery` → `record_branch_nav` →
  `ShellCommand::RecordBranchMember` → `Shell::record_branch_member` →
  `SessionGraphlets::add_member` (dedup) + persist; factored a shared `graph_session_dir`
  helper. Unit-tested `add_member` (grow + dedup). Drove headed: branched a node (roster
  `[anchor]`), navigated the branch window's omnibar to a fresh URL, and the roster grew to
  `[anchor, new-node]` in the sidecar while the donor's default graphlet stayed empty —
  lineage diverges, the new node joins the shared graph. meerkat 87 lib / 159 bin green.
  Surfaced a focus-model nuance (cartography focus is shared, workbench focus is per-window)
  recorded in Slice 2; v0 is honest as-is. The fiddly multi-window drive needed a direct
  omnibar click (not the `/` focus shortcut) to land the in-branch navigation.
- **2026-06-25** — **Decisions + scoping pass (no code).** Read forme's derivation surface:
  no kind-dispatch / BFS / selector matching exists — the kinds are tags, selectors are
  opaque, derivation is unbuilt. So **(C) is closed** (the derivation finding above):
  `GraphTree` adds nothing even for Linked graphlets, since the missing derivation belongs
  on the kernel graph and forme's reusable value is types + a ~10-line diff. Mark decided
  **OQ-2 = yes** (selectors that track drift), so Phase 3 is in scope via the harvest path
  (derivation on the kernel graph + the diff + forme's plain reconciliation types), and the
  anchors-vs-members split + the forme naming rename go live with it. Scoped the open items:
  **#1 per-window focus/selection isolation** (the keystone — mirror the camera
  install/readback pattern; makes any two windows on a graph independent, fixes the Slice 2
  nuance, unblocks Slice 3) and **#3 branch lifecycle on donor delete** (drop pool entries +
  close windows on `close_session`; the dir already trashes the sidecar). Suggested order:
  #1 → #3 → Phase 3.
- **2026-06-25** — **Open item #1 (per-window focus/selection isolation) done + driven.**
  Mirrored the per-window camera: `WindowView.selections: HashMap<GraphId, Vec<uuid::Uuid>>`
  (member-keyed for evict+reload durability), `Orrery::set_selected_members` (inverse of
  `selected_members()`, guarded to skip the reconcile when unchanged — it runs every pass),
  and `install_selections` / `readback_selections` in `viewport.rs` bracketing the ctx
  lifecycle (the `Drop` reads both viewport + selection back). No selection read site
  changed; focus isolates for free (derives from `selected`). Drove headed: two windows on
  one graph, A on "appearance" (`settings://pelt/scene`) and B on "a.com" (`https://a.com`) —
  clicking in B left A unchanged (A re-rendered on foreground and still showed appearance).
  meerkat 89 lib / 167 bin + orrery 80 green. Resolved the Slice 2 cartography-focus nuance
  (cartography focus is now per-window) and unblocked Slice 3. Edge selection + transient
  drags stay shared for v0. Built against concurrent chrome edits (a `button()` refactor in
  views.rs / lib.rs) without conflict — my work was the orrery + window-view + viewport seam.
- **2026-06-25** — **Open item #3 (branch lifecycle on donor delete) done + driven.**
  `close_session` now tears down the dead graph after switching the primary to a survivor +
  trashing the dir: a new `Shell::close_windows_on_graph` closes every secondary window on
  the dead graph (branches + leaves die with the donor; forks on their own graph survive),
  then `graphlets` / `orreries` / `orrery_lru` drop the dead entries. Drove headed: a leaf on
  the active graph (2 windows), deleted that session via the switcher × → 1 window (leaf
  closed) + primary switched to the survivor, no crash. meerkat 89 lib / 167 bin green
  (after a transient red from a concurrent `unix_age` test edit that settled). Next: Phase 3.
- **2026-06-25** — **Phase 3 slice 1 (derivation + Linked path + reconcile) done + tested.**
  Kernel: `Graph::component_members` / `ego_members` (BFS, depth-bounded for ego) + a shared
  `bfs_members`, with `derivation_tests` (component reaches the component / isolates stay
  singletons / unknown seed empty; ego radius 0/1/2 bounded). meerkat `graphlets.rs`:
  `derive_members(graph, spec)` (kind→kernel primitive, `GraphTree`-free), `record_linked`
  (mint a `Linked` graphlet, live derived set in `anchors`, seed in `spec.primary_anchor`),
  `reconcile` (re-derive, then the harvested `compute_roster_delta` over forme's
  `GraphletMemberDelta`, then auto-apply). Tests `linked_component_*` (derive A+B, connect C,
  reconcile so delta.added is C and the roster grows) + `linked_ego_*` (radius 1 is A+B, not
  C). Added `euclid` as a meerkat
  dev-dep for the graph fixtures. kernel 255 + meerkat 89 lib / 169 bin green (a concurrent
  steward/eidetic refactor briefly broke the build mid-slice; verified not ours — `steward_rows`
  dup in pane_data.rs/steward.rs + `TraceEvent.candidates` — waited, it settled). Slice 2:
  a consumer (relational-browse Linked neighborhoods) + the user-choice reconcile proposal +
  the remaining kinds (Corridor/Loop/Frontier/selector).
- **2026-06-25** — **Phase 2 slice 3 (branch-scoped orrery) done + driven.** A branch window
  renders only its graphlet's members. Per-window via save/restore (only a branch window
  touches the scope; ctx saves the prior scope, sets `scope_to_members(roster)`, `Drop`
  restores): `WindowCtx` gained a read-only `graphlets` ref + `branch_scope_restore`;
  `install_scope` / `restore_scope` in viewport.rs. Driving caught that render/orrery_scene.rs's host
  card-builder ignored the orrery scope (iterated all nodes); fixed with `Orrery::node_in_scope`.
  Drove headed: a branch window showed only its anchor node; the donor showed the whole graph
  (no scope leak). meerkat 89 lib / 172 bin + orrery 80 green. The multi-window drive fought
  concurrent chrome churn (a tab refactor) + window-handle flakiness; a temporary
  `[scope]` stderr probe confirmed `install_scope` was scoping, isolating the bug to the host
  card path.
- **2026-06-25** — **Phase 3 slice 2 (manual Linked consumer) built + tested.**
  `ContextAction::OpenComponentGraphlet` (node menu + palette entry) →
  `ShellCommand::OpenLinkedGraphlet` → `Shell::linked_component_graphlet` (mint a
  `Linked { Component }` graphlet on the focused node) → open a window scoped to it (Slice 3
  path). meerkat 89 lib / 172 bin green. The Linked mechanism is unit-tested (Slice 1) and
  the action surfaced correctly in the palette ("Open component as graphlet"); the full UI
  click-through was not cleanly captured (the action keys off the focused node, and the drive
  harness lost focus across the palette interaction — harness friction, not a code gap).
  Slice 2+ remains: relational-browse wiring, the user-choice reconcile proposal,
  auto-reconcile on drift, and the richer kinds.
- **2026-06-25** — **Phase 3 slice 2 re-drive: confirmed.** With the node verified focused
  through the palette, running "Open component as graphlet" wrote a `Linked { Component }`
  graphlet (spec.anchors = the seed; GraphletRef.anchors = the 12-node derived component) and
  opened a scoped window with the `◎` chip. The earlier partial capture was harness focus
  friction, now closed. Slice 2 is driven.
- **2026-06-25** — **Phase 3 slice 2+ (live drift) done + tested.** `install_scope` re-derives
  a `Linked` graphlet's roster from the live graph each pass (read-only `derive_members`), so a
  Linked window tracks graph drift live; a `Branched` graphlet keeps its accumulated roster.
  meerkat 89 lib / 172 bin green. Remaining Slice 2+: relational-browse wiring, the user-choice
  reconcile proposal + persisting the reconciled roster, and the richer kinds.
- **2026-06-25** — **Phase 3 slice 2+ (data-level auto-reconcile) done + tested.** `save_session`
  queues `ReconcileGraphlets { graph }` when the graph has a Linked graphlet →
  `Shell::reconcile_linked_graphlets` → `SessionGraphlets::reconcile_all` (re-derive + diff +
  auto-apply) + persist on change. The persisted Linked roster now tracks drift, closing the
  stale-roster gap (the window already tracked it live). Unit-tested `reconcile_all_updates_
  linked_rosters_on_drift`; graphlets bin tests 6/6. The lib toolbar test was red on a
  concurrent chrome button refactor (verified not ours).
- **2026-06-26** — **Phase 3 slice 2+ (selectors / edge projection) done + tested.**
  `component_members` / `ego_members` take a `&[RelationSelector]` projection; the BFS follows
  only edges matching a selector (empty = all), via `edge_matches_selectors` (reads the edge
  payload between two nodes, `has_relation`). `derive_members` maps a spec's `selectors`
  strings → `RelationSelector::Family`. Kernel test `the_selector_projection_changes_the_
  derived_shape` (Semantic-only vs Containment-only vs all over A—Sem→B—Cont→C). Full suite
  green: forme 114, kernel 256, meerkat 89 lib / 173 bin, orrery 80. Closes the highest-leverage
  gap: the kinds derive *relationally*
  now. Added a **Controls** design note (graphlet owns kind+selectors; surface owns
  layout+binding — do not duplicate the selector control per gloss/swatch/canvas).
- **2026-06-26** — **Plan audit + doc fixes.** Re-read the whole plan and verified its claims
  against the code: all named functions / types / commands exist; test counts confirmed (forme
  114, kernel 256, meerkat 89 lib / 173 bin, orrery 80). Fixed eight staleness / accuracy
  issues: the headline "Next" (lagged four sub-slices), a contradictory duplicate "Slice 2+
  remaining" list, the inaccurate "Component = `weakly_connected_components`" claim (it is a
  fresh BFS; result-equivalent but not the fork primitive), stale `component_members` /
  `ego_members` signatures (now carry `selectors`), the stale "Derivation (kernel graph)"
  bullet, file paths moved by the settled 600-LOC split (`render.rs` → `render/orrery_scene.rs`),
  the dated "imports one forme item" snapshot, and a count drift (172 → 173 bin).
- **2026-06-26** — **Phase 3 slice 2+ (projection-vocabulary control v0 + second/third kinds)
  done + built.** Generalized `ShellCommand::OpenLinkedGraphlet` to carry `kind`, `selectors`,
  and a `chip` word, and `linked_component_graphlet` → `linked_graphlet(node, from, kind,
  selectors)`, so any Linked kind/projection is one command. Added two consumers beside "Open
  component": **Open neighborhood** (`Ego { radius: 2 }`, a radius-bounded *subset*) and **Open
  link web** (`Component` under `selectors = ["Semantic"]`, the link/citation projection —
  exercises the selector control end to end from the UI). Both are context-menu items + palette
  entries; the scoped window's chip reads `◎ {kind}: {anchor}`. meerkat 89 lib / 173 bin green.
  Not separately headed-driven: the new consumers ride the *same* mint→scope-window path the
  component consumer was driven on, over the kernel-tested Ego + selector engine. The remaining
  projection control (a live family-picker toggle / derivation strip, vs these presets) is a
  chrome design task; see the Controls note + Close-out.
