# Edge System Audit

> **Current state (2026-06-15 reconciliation).** The opening symptom below is
> **resolved**: create / retract / traverse between two existing nodes all shipped
> (`command_drain.rs:97-112`, context menu `menus.rs:193`, `>relate` / `>unrelate`,
> roster edge-row click). The accurate state is the "Status (2026-06-13)" block in
> the Recommendation section, not this top framing, which is preserved as the
> original audit record. The **one remaining gap is the discoverable relation-kind
> picker** (Tier A in the
> [in-the-wings audit](2026-06-15_in_the_wings_and_browser_bar_audit.md)): the
> string→`SemanticSubKind` map exists at `command_drain.rs:25`, but the context-menu
> Relate handler hardcodes `UserGrouped`. Read top-down without this note, the doc
> mis-prioritizes done work.

A code-verified audit of edge generation and management, prompted by a concrete
symptom: there is no way to draw an edge between two existing nodes. The finding
is that the symptom is shallow. The kernel edge model is rich and complete, the
selection gesture exists, and the only missing piece is a thin host action that
asserts a relation from the current selection. The deeper gap is conceptual, not
mechanical: no UX for *which* relation and no edge retraction.

## What exists (and is solid)

### The kernel model is complete

[`graph-kernel`](../../../crates/graph/graph-kernel/src/graph/) carries a full
relation taxonomy, typed-sidecars-authoritative:

- Six [`EdgeFamily`](../../../crates/graph/graph-kernel/src/graph/edge_taxonomy.rs)
  values: `Semantic`, `Traversal`, `Containment`, `Arrangement`, `Imported`,
  `Provenance`. An edge's families are *derived* from the sidecars it carries, so
  one edge can be multi-family.
- A deep `SemanticSubKind` vocabulary: `Hyperlink`, **`UserGrouped`**,
  `AgentDerived`, `Cites`, `Quotes`, `Summarizes`, `Elaborates`, `ExampleOf`,
  `Supports`, `Contradicts`, `Questions`, `SameEntityAs`, `DuplicateOf`, and more.
  `UserGrouped` is a sub-kind that exists *only* to carry a user-asserted grouping
  relation — the taxonomy anticipated user-created edges that the UI never wired.
- A clean write API in
  [`edge_ops.rs`](../../../crates/graph/graph-kernel/src/graph/edge_ops.rs):
  `assert_relation(from, to, EdgeAssertion)` (the canonical typed path, idempotent
  per sub-kind, creates-or-merges), `assert_semantic_predicate` (open-IRI
  predicates), `append_traversal` (navigation), and `retract_relations(from, to,
  RelationSelector)` (true removal, garbage-collecting empty edges). The legacy
  `add_edge`/`remove_edges` paths were deleted in the 2026-05-11 taxonomy pass;
  everything routes through `EdgeAssertion` / `RelationSelector` now.

This layer is well-tested and not the problem.

### Edge generation today is (almost) all automatic

Five write paths reach `assert_relation` and friends; four are machine-driven:

1. **Traversal** — navigation appends `Traversal`-family events
   (`append_traversal`), the node-lineage spine.
2. **Linked-data ingest** —
   [`linked-data`](../../../crates/graph/linked-data/src/lib.rs) /
   `ingest/apply.rs` assert `Semantic` edges + open predicates from fetched
   JSON-LD.
3. **Inker link-statements** —
[`statements.rs`](../../../crates/inker/src/statements.rs) *(historical citation)* <!-- doc-audit: historical-link --> asserts relations
   from a document's typed links (knot `rel`s, etc.).
4. **New-node-from-address** —
[`open_member_as_new_node`](../../../crates/orrery/orrery/src/input.rs) *(historical citation)* <!-- doc-audit: historical-link --> (the
   Ctrl/Cmd+Enter omnibar path) creates a *new* node and asserts a `hyperlink`
   edge from the origin to it.

Path 4 is the only user-initiated edge creation, and it is welded to node
creation: it always makes a new node, always a `hyperlink`, and only from the
focused origin. There is no way to relate two nodes that already exist.

### Selection and display management already exist

The orrery is further along than the creation gap suggests:

- **Multi-node selection**: Shift-click toggles nodes into
  `selected: HashSet<NodeKey>`
([`orrery/input.rs`](../../../crates/orrery/orrery/src/input.rs) *(historical citation)* <!-- doc-audit: historical-link -->). Two-node
  selection — exactly what a relate gesture consumes — is already a thing.
- **Edge selection**: marquee rect-select collects crossed edges; a bare click
  runs `edge_hit_test` and picks the edge under the cursor, into
  `selected_edges: HashSet<(NodeKey, NodeKey)>`.
- **Display management**: `hide_selected_edges` / `show_all_edges` (wired to the
  `HideSelectedEdge` / `ShowAllEdges` commands) toggle edge visibility. These are
  display-only; the relations persist.

## The gaps

### 1. No creation between existing nodes (the reported symptom)

Nothing takes the current selection and calls `assert_relation`. The kernel API
is ready, the multi-select gesture is ready; the host action between them is
absent. There is no `Command` for it (the 20-verb set has `HideSelectedEdge` /
`ShowAllEdges`, no create), no context-menu item, no gesture.

### 2. No relation-kind UX

Every user-visible edge is a `Hyperlink`. The `SemanticSubKind` vocabulary
(`Cites`, `Supports`, `Contradicts`, `UserGrouped`, …) is unreachable from the
UI. A creation flow needs a way to pick the kind (default `UserGrouped` or
`Hyperlink`; a small picker for the rest), or the rich taxonomy stays
ingest-only.

### 3. No true edge retraction

`retract_relations` exists in the kernel but is wired to no user action. The only
edge command is `HideSelectedEdge`, which is *display*, not deletion. `DeleteNode`
removes a node (and its incident edges via `remove_node`), but a user cannot
delete a single mistaken relation while keeping both nodes.

### 4. No edge traversal (the roster's latent navigation hooks)

Edge management has three legs: create, retract, and *traverse*. Create and
retract are now wired (`assert_selected_relation` / `retract_selected_relation`,
the `AssertEdge` / `RetractEdge` commands, `>relate` / `>unrelate`). Traversal is
not: [`roster::EdgeRow`](../../../crates/meerkat/src/roster.rs) *(historical citation)* <!-- doc-audit: historical-link --> carries
`other_title`, `other_url`, and `other_member` per relation, but the live
`roster_view` renders only direction + kind + title — `other_url` /
`other_member` are unread (a dead-field warning). They are the hooks for "click a
relation in the focused node's edge list → select / navigate to the other
endpoint". The actuator already exists (`orrery.select_by_url`, the agent
harness's `SelectNodeByUrl`); the wiring is a roster edge-row `on_click`. Deferred
only because `roster_view` is under concurrent edit (window-composition work).

### 5. Edge selection identity is index-shaped

`selected_edges` is keyed by `(NodeKey, NodeKey)` (petgraph indices), not stable
UUIDs. Fine within a session; worth noting for any persisted edge-selection or
cross-session edge addressing, and for routing a retraction through the
UUID-keyed replay path (`replay_retract_relations_by_ids`).

## Recommendation

> **Status (2026-06-13): create / retract / traverse + kind choice + feedback all shipped.**
> - **Create / retract** (`d4f7787`): `orrery::assert_selected_relation` /
>   `retract_selected_relation`, the `Command::AssertEdge` / `RetractEdge` pair (→
>   palette, agent harness, accesskit, `>relate` / `>unrelate` via the `verb()`
>   unification). `>unrelate` is symmetric (two-node selection or a clicked edge).
> - **Relation-kind choice** (`a84fc94`, gap 2): the scriptable `relate("cites")`
>   form maps a kind word → `SemanticSubKind` (cites/quotes/supports/contradicts/
>   same/…; unknown → `UserGrouped`). A *discoverable kind-menu UI* is the one
>   remaining layer.
> - **Edge traversal** (`2b3ff83`, gap 4): a roster edge row's `on_click` selects
>   the other endpoint (the previously-unread `EdgeRow.other_member`).
> - **No-op feedback** (`fed6af2`): `>relate` / `>unrelate` echo "select two nodes…"
>   in the bar instead of silently no-opping.
>
> Still open: the discoverable relation-kind menu, drag-to-connect, edges
> rendering distinct sub-kinds (Cites looks like UserGrouped today), and the five
> spine threads below.

This is a wiring task on a finished substrate, not a new system. The shape:

- **A host action `assert_selected_relation(sub_kind)`** on the orrery / shell:
  when exactly two nodes are selected, call `graph.assert_relation(a, b,
  EdgeAssertion::Semantic { sub_kind, .. })`; refuse (with a status note) for
  selections that are not a clean pair. Mirror with `retract_selected_relation`
  over the selected edge.
- **Surface it four ways, reusing the spine we just built** (the omnibar command
  shell makes three of these nearly free):
  - a `Command::AssertEdge` / `RetractEdge` pair (palette + agent harness +
    accesskit, automatically, per the `verb()` unification);
  - omnibar verbs `>relate` / `>unrelate`, and — once the shell takes arguments —
    `relate(a, b, "cites")` scripting across the orrery (the graphshell premise);
  - a context-menu item on a two-node selection / a selected edge;
  - the default kind is `UserGrouped` (honest provenance: a human asserted it),
    with a small kind picker for the semantic vocabulary.
- **Relation-kind picker** as the one genuinely new piece of UX. Start with a
  short curated list (`UserGrouped`, `Hyperlink`, `Cites`, `Supports`,
  `Contradicts`, `SameEntityAs`), expandable later.

A drag-to-connect gesture (drag from node A's rim to node B) is the richer
direct-manipulation option and a natural follow-on, but the
selection-plus-command path lands the capability first with no new gesture code.

## Open questions

- **Default kind**: `UserGrouped` (truthful about provenance) vs `Hyperlink`
  (what every other path makes). Leaning `UserGrouped`.
- **Direction**: `assert_relation` is directed. For a two-node selection, which is
  `from`? Use the focus / selection order, or assert both directions for
  symmetric kinds (`SameEntityAs`, `UserGrouped`)?
- **Drag-to-connect**: worth the gesture code now, or after the command path
  proves the flow?

## The spine: settled and unsettled

A wider read of the model (2026-06-13), beyond the creation gap above.

**The shape, and why it is right.** One *directed* edge per node-pair,
multiplexed by typed family sidecars on `EdgePayload` — not a multigraph with
parallel edges. A single A→B edge can simultaneously carry up to six families,
each answering a different question: Semantic (what it means), Traversal (that you
went there — the only family with no sub-kinds, an event log + metrics +
`NavigationTrigger`), Containment (where it lives), Arrangement (how it sits on
screen), Imported (that it came from elsewhere), Provenance (what it was derived
from). Three decisions are load-bearing and right: **sidecars are authoritative**
(the family set is recomputed from populated sidecars — no parallel cache to
desync), **write/read are split** (`EdgeAssertion` carries construction payload,
`RelationKind` / `RelationSelector` are the read discriminants), and **semantic is
open-world** (it holds both recognized closed sub-kinds *and* a raw predicate IRI,
so a web `rel=schema:cites` lands as a predicate when no closed kind matches — the
same seam knot links ride).

**The five genuinely-unsettled threads**, roughly in priority:

1. **Merge under sync** (prioritize, given the mesh / moot LogSync work just
   landed). When two peers assert edges, `EdgePayload` needs to be a clean CRDT
   join. The sidecar sets look set-union-friendly, but `decay_progress` and labels
   are last-writer-ish and traversal logs are append-y. Edges crossing graphs (a
   moot member linking your node) are a different animal from `Imported`. How
   edges federate is undecided.
2. **Edge reification.** Edges aren't addressable today. The moment you want to
   annotate a relation (attach provenance to the link itself, let an agent cite
   *why* it asserted `SameEntityAs`, make statements about statements), an edge
   must become a node. RDF reification vs a lightweight "edge has a targetable id".
   The biggest fork; the open-predicate model already leans toward it.
3. **Symmetric relations on a directed store.** `SameEntityAs` / `DuplicateOf` are
   symmetric but stored directed. Write both directions, canonicalize to one, or
   rely on the read-time `UndirectedAdaptor`? Each has costs (double-write vs query
   asymmetry). Currently ambiguous — and it is exactly the direction question the
   creation flow above raises.
4. **Decay policy and strength.** Agent-derived semantic edges carry
   `decay_progress`, but who ticks it, on what cadence, and what reinforces it? And
   there is no unified *strength* the arrangements / gyre layer can read for force
   physics or ranking (traversal has counts, semantic has decay, arrangement has
   durability; layout wants one number). Per the configurability rule, the decay
   curve and the strength projection probably want to be settings, not constants.
   This is the same strength the [graphlet
   classifier](../design/2026-06-13_graphlet_derivation_from_selection.md) ranks
   by.
5. **Containment vs Arrangement overlap.** Both express grouping — Containment is
   "belongs to" (durable, semantic), Arrangement is "shown together" (often
   session). The line is principled, but `CollectionMember` (containment) and
   `TileGroup` (arrangement) can describe the same user gesture from two angles.
   Wants a sharp rule for which one a "group these" action writes — the graphlet
   crystallize step (session → Arrangement, linked → Containment) is one proposed
   answer.
