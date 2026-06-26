# Graphlet Wiring Plan — give graphlets a live home in the shell

**Date**: 2026-06-25
**Status**: **Phases 1 + 2 done + driven; open item #1 (per-window focus/selection) done +
driven (2026-06-25).** Branch (Shift+drag) mints a persisted `Branched` graphlet
(round-trips a restart), reads as a distinct branch (an accent `⎇ <anchor>` chip), and
accumulates its own lineage as you navigate in it. Per-window focus/selection isolation now
makes two windows on one graph independent (driven), which resolved the Slice 2
cartography-focus nuance and unblocks Slice 3. Next candidate: **Phase 3** (Linked /
auto-derived graphlets — OQ-2 is yes) or **#3** (branch lifecycle on donor delete). Spun out
of the [tear-out gestures plan](2026-06-24_tearout_gestures_plan.md) (G3 / OQ-7 deferral)
when scouting the forme graphlet API showed the whole layer is built but unwired.
**Lane / conflict posture**: meerkat session/state + a small forme surface (reusing its
`GraphletRef` type). No kernel or orrery changes in Phase 1. Touches per-session
persistence (a new sidecar) and the tear-out branch op.
**Consumers** (why this is worth building, beyond branch): the tear-out **branch**
operation (G3) is the immediate one, but the same per-session graphlet index is what
**document-groups** and the
[relational-browse front-end](2026-06-23_relational_browse_graphlet_plan.md) both want —
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
- **The layer is unwired.** A workspace grep finds **zero** live construction of a
  `GraphTree` or `GraphletRef` outside forme's own tests. meerkat imports exactly one
  forme item — `GraphMemberId` (29 files) — and nothing else.
- **forme's `GraphTree` is superseded, not just unused.** `GraphTree<N>` ("the core data
  structure, one per graph view") carries members + topology + projection lens + layout
  + graphlets. But the **live** arrangement is the other model: platen's `Workbench`
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
radius N, Component = the `weakly_connected_components` that already powers fork, selector =
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

**Slice 3 (optional v0) — orrery scope.** Decide whether the branch window's orrery shows
only the graphlet's members (a filtered cartography projection). Per-window focus is now
done (#1), so this is just the *filtered view*; still optional / deferrable.

**Done when:** a branch window shows a distinct grouping (Slice 1 ✓) that accumulates its
own lineage (Slice 2 ✓) while node edits still propagate to the donor (✓ — the navigated
node joins the shared graph). **Met** (modulo the cartography-focus note above).

### Phase 3 — Linked / auto-derived graphlets + reconciliation (the richer model)

**Decision (Mark, 2026-06-25): yes — we want selectors that track drift.** So Phase 3 is
in scope (not closed to "richer kinds as plain data"). Per the derivation finding, this is
built by **harvesting forme's types + the diff and writing derivation on the kernel graph**;
`GraphTree` is not used.

- **Derivation (kernel graph).** A `Linked { spec }` graphlet derives its member set from
  the graph: Ego = BFS to radius N from the anchor; Component = `weakly_connected_components`
  (the fork primitive); Corridor = a path query; selector match = filter kernel edges by
  family / sub-kind (the kernel owns the relation vocabulary forme leaves opaque). New code,
  but small and native to the kernel, which already has the primitives.
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
- **#3 — Branch lifecycle on donor delete (G6).** `close_session` already switches away and
  `move_to_trash`es the session dir, so `graphlets.json` trashes with it (no on-disk
  orphan). **Missing:** the in-memory `self.graphlets` + `self.orreries` pool entries for
  the deleted graph are not dropped, and open windows whose `focused_graph` is the deleted
  graph are not closed (brief §4.2: killing the donor kills the branch). **Seam:** in
  `close_session`, `self.graphlets.remove(graph)` + orrery eviction + close secondary
  windows on that graph. **Note:** closing windows on session-delete is a *general*
  multi-window gap (deleting any session with open windows orphans them), not
  graphlet-specific — do it with that general handling, not as a one-off.
- **OQ-D — does a branch window get a thin orrery?** Brief says workbench-only (§4.2); G2
  (leaf content) settles the workbench-pane mechanics this rides on. Ties into #1 / Slice 3.

**Suggested order:** ~~#1 (keystone)~~ **done** → #3 (with general multi-window
session-delete) → Phase 3 (now that OQ-2 is yes). The tear-out-gesture trailing items (toast,
tile-tab leaf origin, G5 move, G2 leaf content, fork restore-into-switcher) live in the
[gestures plan](2026-06-24_tearout_gestures_plan.md), independent of this subsystem.

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
