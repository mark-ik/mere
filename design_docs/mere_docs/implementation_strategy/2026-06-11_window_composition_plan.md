# Window Composition Plan — orrery-per-window, window taxonomy, cross-graph composability

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
over-deferral. The thing that looked hard — two orreries coexisting — is mostly already
supported one layer down, and the leaf-only interim throws away work.

**Two windows want two axes, not one `WindowKind` ladder:**

- **Content axis** — what a window *renders*: an **orrery** over a graph, or an
  **orrery-less** surface (a workbench tile, a gloss/navigator pane, later others). Not
  every window should carry an orrery: tearing out a workbench tile should not force a
  graph to render behind it.
- **Linkage axis** — how a window *relates* to the others: **independent** (owns its
  own `SessionId`/`GraphId`; aware of siblings but not dependent — closing one does not
  break another) or **primary-secondary / linked** (shares a donor's backing state;
  edits sync both ways).

A window is a `(content, linkage)` pair. The tear-out trichotomy falls out of the two
axes rather than being three special cases:

| Operation | Content | Linkage | Identity |
| --- | --- | --- | --- |
| **Leaf** | workbench tile | linked (donor's node + graph) | none |
| **Branch** | workbench tile (or orrery) | linked (donor's graph, own graphlet) | new `GraphletId` |
| **Fork** | orrery (thin) | independent (own session + graph) | new `SessionId`+`GraphId` |
| **"New graph window"** (Mark's case) | orrery | independent | own session |
| **Primary** | orrery | independent root | the root session |

**Composability is the bridge between independent windows.** Because independent
windows own separate graphs, you can take a tile/node from graph A's window and **move
or copy** it into graph B's window. The resulting node in B records **provenance +
lineage** back to A's node (where it came from / what it was copied from). This is the
general operation; fork's cross-graph rekey is one consumer of it.

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
- **Provenance/lineage primitives exist.** forme has a per-node lineage facet; the graph
  has a provenance edge family (persisted as `PersistedProvenanceEdgeData`); fork already
  plans a cross-graph rekey (tear-out brief §7.5) and a weak `parent_session` ref. The
  composability operation records into these, it does not invent them.
- **The one case that still needs a registry** is *the same graph live in two windows*
  (two cameras, shared nodes updating in both) — because the graph is bundled *inside*
  `Orrery`, two instances are two graphs. Sharing one graph across two cameras means
  extracting `Graph` (+ physics) into a `GraphId`-keyed registry. Deferred until a real
  consumer wants it; the different-graph and linked-tile cases below do not.

## Architecture

```
struct WindowView {
    kind: WindowKind,            // the (content, linkage) pair (below)
    surface: WindowSurface,      // per-window (MW3, done)
    runner/dom, workbench, frame_layout, input/scrying caches ...   // per-window (MW2, done)
}

enum WindowContent {
    Orrery(Orrery),              // owns graph + camera + physics (moved off Shell)
    Tile(GraphMemberId),         // an orrery-less workbench-tile window (leaf/branch)
    Gloss(GraphId),              // an orrery-less navigator/minimap window
    // ... future orrery-less surfaces
}

enum WindowLink {
    Independent { session: SessionId },        // own graph; aware-not-dependent
    Linked { donor: WindowId, graph: GraphId },// shares a donor's backing state, synced
}
```

`WindowKind` becomes the `(WindowContent, WindowLink)` pair (exact shape decided in P2 —
it may be one enum or two fields; the `{Primary, Leaf}` marker from MW3 step 4 is the
seed). The shared `constellation`, actor handles, manifests, and theming stay on
`SharedState`; **only the orrery (graph+camera+physics) moves into `WindowContent`.**

Render/input already operate per-window (post-MW2), so they target the window's own
content: an `Orrery` window drives its orrery; a `Tile`/`Gloss` window renders that
surface from the shared constellation and never touches an orrery (the half-built
isolation gate from MW3 step 4 part 2 — reverted — becomes "this window has no
`Orrery`," cleaner than a `kind.is_slim()` check).

## Phases (done-conditions, not dates)

### P1 — Orrery off `Shell`, into the window (the enabling move)

Move `orrery: Orrery` from a single `Shell` field into per-window state (the ~58 sites),
behind `WindowContent::Orrery`. Per-window physics: each orrery offloads to its own actor
on the shared wake. At N=1 (one primary orrery) behavior is identical. Render/input
resolve the window's own orrery instead of `self.orrery`.

Done when the primary runs exactly as today with its orrery owned by its `WindowView`,
and a second `Orrery` window can be spawned over a *different* graph (different
`SessionId`) and panned/zoomed independently, both drawing content from the one shared
constellation, neither disturbing the other's camera. 44+/66+ green; on-screen verify.

### P2 — The `(content, linkage)` taxonomy

Generalize the MW3 `WindowKind {Primary, Leaf}` marker into the two-axis model. A window
records its content (`Orrery` / `Tile` / `Gloss` / …) and its link (`Independent` /
`Linked`). Close/save forks on linkage (independent windows save their own session;
linked windows just drop). The slim-vs-full chrome derives from content (orrery windows
get the shellbar/switcher; orrery-less windows get slim chrome — MW3 step 4 part 1 already
built the slim path).

Done when window role is one typed value driving chrome template, close behavior, and
render path, with the MW3 special-cases (`primary` field, `is_slim`) folded into it.

### P3 — Orrery-less linked windows (leaf + gloss), generalized

A `Tile(member)` window renders that one member's tile from the shared constellation,
linked to its donor's graph; a `Gloss(graph)` window renders the navigator. Both are
orrery-less and **independently navigable** (their own omnibar drives their own surface),
linked so edits to the underlying node propagate (the leaf semantics from the tear-out
brief §4.1). This is MW3 step 4 part 2, done right: not "a leaf is a slim orrery window
with the orrery hidden," but "a leaf is a window whose content is a `Tile`, with no orrery
at all."

Done when a spawned/torn `Tile` window shows a shared node's live tile, navigates on its
own, and propagates edits to the donor, with no orrery instantiated for it.

### P4 — Cross-graph composability (move / copy a node, with provenance)

A reusable `mere`-level operation: move or copy a tile/node from graph A into graph B's
workbench. Copy mints a new node in B via the cross-graph rekey (tear-out brief §7.5) and
records a **provenance edge** (origin: A's node) + lineage on the new node, so B's node
knows where it came from. Move re-parents the binding. Surfaced as a drag between two
independent orrery windows' workbenches (and a palette/command form).

Done when a tile dragged from one independent graph window into another's workbench
produces a node in the destination with correct provenance + lineage back to the source,
and the source is either left intact (copy) or releases its binding (move).

### P5 — The tear-out gesture model (leaf / branch / fork + toast)

Implement the [tear-out brief](../research/2026-05-11_tearout_operations_brief.md) on top
of P1–P4: drag = leaf (`Tile`, linked), Shift+drag = branch (new `GraphletId`, linked),
Ctrl/Cmd+Shift+drag = fork (`Orrery`, independent, new session via the P4 rekey), toast on
ambiguous drag. Spawn-on-drop with an in-donor drag ghost (the portable shape from MW4).

Done when all three operations run from the gesture model with the brief's identity
semantics, and the toast escalates a leaf in place.

### Deferred — same graph in two windows (the registry)

Extract `Graph` (+ physics) into a `GraphId`-keyed registry so two `Orrery` windows can
share *one* live graph (two cameras, shared nodes). Not needed by P1–P5 (those are
different-graph or linked-tile). Picked up when a real consumer wants the same graph open
twice; far-B (multi-graph coexistence in one window) lands on the same registry.

## Open questions

1. **`WindowKind` shape** — one `(content, linkage)` enum, or two fields? Decide in P2;
   the close/chrome/render consumers want cheap matches on both axes.
2. **Per-window physics cost** — N orreries = N physics actor threads. Fine for a handful
   of windows; revisit pooling only if it bites.
3. **Provenance edge direction + family** — confirm the existing provenance edge family
   carries "copied-from across graphs" cleanly, or whether copy wants a distinct sub-kind
   (P4). Check before building, per the consumer-pull rule.
4. **Move vs copy default** — a cross-graph drag defaults to which? Likely copy (safer,
   provenance-tracked); move on a modifier. Decide in P4.
5. **Linked-tile lifecycle on donor delete** — the brief §8.3 cascade rules (leaves lose
   their node, branches die, forks survive) map onto the linkage axis; wire them in P3/P5.

## Progress

- 2026-06-11: Plan written, superseding the multi-window plan's MW4–MW6. Reframed the
  second window from "workbench-only leaf to dodge the orrery split" to a two-axis
  `(content, linkage)` model after verifying the shared constellation is UUID-keyed and
  graph-agnostic (so two orreries over different graphs need only the per-window orrery
  move, no registry). MW3 step 4 part 2 (the workbench-only-leaf interim) is **dropped**
  in favor of P3 (orrery-less `Tile` windows). The half-built orrery-isolation render gate
  was reverted; it returns in P1 as "this window has no orrery," not a slim-flag check.
