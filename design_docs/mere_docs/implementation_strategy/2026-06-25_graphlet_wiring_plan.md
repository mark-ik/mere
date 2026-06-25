# Graphlet Wiring Plan — give graphlets a live home in the shell

**Date**: 2026-06-25
**Status**: Planning. Stub landed (`crates/meerkat/src/graphlets.rs`), not yet wired into
the session lifecycle. Spun out of the
[tear-out gestures plan](2026-06-24_tearout_gestures_plan.md) (G3 / OQ-7 deferral) when
scouting the forme graphlet API showed the whole layer is built but unwired.
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
  it. Pro: the architecturally "complete" model; reconciliation works out of the box.
  Con: a large reframe that re-homes the live arrangement; far beyond what branch needs;
  best justified by its own consumer wave, not by branch.

**Recommendation: (B).** It unblocks branch with the least new surface and no duplication,
and leaves (C) open as a Phase-3 decision if reconciliation / document-groups demand the
full container. The stub reflects (B).

---

## Phases

### Phase 1 — Substrate: a live, persisted per-session graphlet index

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

- The workbench (or a chrome surface) reflects graphlet membership — the branch window
  shows it is scoped to its graphlet, not the whole graph.
- In-window actions (navigation, node selection, tile additions) populate the branch
  graphlet's roster / lineage, diverging from the donor while sharing kernel nodes
  (brief §4.2 live behaviour).
- Decide how the orrery scopes to a graphlet (a filtered cartography projection), if at
  all in v0.

**Done when:** a branch window shows a distinct grouping that accumulates its own lineage
while node edits still propagate to the donor.

### Phase 3 — Reconciliation + the richer model (the broader payoff)

The features the forme model already specs, brought to the live index — and the point
where (C) gets re-decided.

- Reconciliation: deltas, proposals, the `SaveAsNewFork` choice — adapt forme's
  `GraphTree`-shaped functions to `SessionGraphlets`, or adopt `GraphTree` if that proves
  cleaner (the deferred (C) decision).
- Projection-binding specs + the non-`Session` `GraphletKind`s (Ego, Corridor, Component,
  Loop, Frontier, Facet, Bridge, WorkbenchCorrespondence).
- Consumers beyond branch: document-groups, the relational-browse front-end.
- Rename follow-on: forme's producing functions still carry the old word — `apply_fork`,
  `detect_fork_on_manual_override`, `ReconciliationChoice::SaveAsNewFork` — rename to the
  branch vocabulary once they are live (the `Forked` → `Branched` variant rename already
  landed 2026-06-25).

**Done when:** a diverged branch can be reconciled or consolidated, and a second consumer
(document-groups or relational-browse) reads the same index.

---

## Open questions

- **OQ-A — the (A)/(B)/(C) structural choice.** Recommend (B); ratify before Phase 1.
- **OQ-B — where the `GraphletId` rides on a window.** `WindowView` view-session bits, or
  the frame leaf, or the (deferred) workbench projection.
- **OQ-C — persistence format + migration.** A `graphlets.json` sidecar; `GraphletRef`
  is already `Serialize`, so this is mostly plumbing. No migration (nothing persists
  graphlets yet).
- **OQ-D — does a branch window get a thin orrery?** The brief says workbench-only
  (§4.2); G2 (leaf content) settles the workbench-pane mechanics this rides on.

---

## Progress

- **2026-06-25** — Plan spun out + stub landed. Scouting (above) established that the
  forme graphlet API is first-class but unwired and that `GraphTree` is superseded by the
  live platen + orrery arrangement, reframing the work from "instantiate a `GraphTree`"
  to "add a per-session graphlet index reusing `GraphletRef`" (recommendation B). The
  stub `crates/meerkat/src/graphlets.rs` (`SessionGraphlets` + `record_branch`) compiles
  and is module-registered but not yet wired into the session lifecycle — Phase 1 wires
  it. Build green.
