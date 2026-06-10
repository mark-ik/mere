# Relation taxonomy cleanup + edge mutation MVP — implementation plan

**Date**: 2026-05-11
**Status**: Stages 1–5 landed plus the §6.3 hit-test + per-family rendering follow-up. Outstanding deferrals: §5.3 `ViewIntent` sidecar (hide-state durability) and "Create node here" UX.
**Scope**: Make `mere-kernel`'s graph the source of truth for relation taxonomy before building edge create/delete UI. Remove the legacy `EdgeType` / `EdgeKind` path instead of extending it. Keep the six families (`Semantic`, `Traversal`, `Containment`, `Arrangement`, `Imported`, `Provenance`). Land an MVP of edge create / hide / retract on top, with derived/imported/provenance/document-hyperlink/traversal-history as hide-only (never retract). Treat "navigatory" as a projection over node navigation memory, semantic hyperlinks, and traversal events — **not** as a durable edge family.

**Related**:

- [`../research/2026-05-11_browser_multiplexer_framing.md`](../research/2026-05-11_browser_multiplexer_framing.md) §5.3 — `ViewIntent` sidecar (eventual home for per-orrery hide state).
- [`../research/2026-05-11_memory_tiers_brief.md`](../research/2026-05-11_memory_tiers_brief.md) — short-term vs long-term; hide is short-term view policy.
- [`2026-05-11_typed_action_bus_plan.md`](2026-05-11_typed_action_bus_plan.md) — new `AssertRelation` / `RetractRelation` / `HideRelation` bus kinds plug in here.
- [`2026-05-11_node_per_tile_lineage_plan.md`](2026-05-11_node_per_tile_lineage_plan.md) §5.4 — graph-tree's per-workbench `Provenance::Traversal` lineage facet coexists with kernel-level `Traversal` events (different scopes; see §8.5 below).
- [`crates/mere-kernel/src/graph/edge_taxonomy.rs`](../../../crates/mere-kernel/src/graph/edge_taxonomy.rs) — current state of the taxonomy.
- [`crates/mere-kernel/src/graph/edge_payload.rs`](../../../crates/mere-kernel/src/graph/edge_payload.rs) — `EdgePayload` to refactor.
- `c:/Users/mark_/Code/repos/graphshell/design_docs/graphshell_docs/implementation_strategy/graph/2026-03-14_graph_relation_families.md` — prior thinking on family vocabulary (informative, not authoritative; see [memory: graphshell-donor-not-authority](../../../.claude/memory/project_graphshell_donor_not_authority.md)).
- `c:/Users/mark_/Code/repos/graphshell/design_docs/graphshell_docs/implementation_strategy/graph/2026-03-21_edge_family_and_provenance_expansion_plan.md` — prior thinking on collapsing `EdgeType` into family + sub-kind (informative).

---

## 1. Goal + done conditions

**Goal:** `mere-kernel` exposes one canonical relation surface (`RelationKind` for reads, `EdgeAssertion` for writes, `RelationSelector` for filters/retractions). `EdgeType` and `EdgeKind` are gone from the public API. The Edge Mutation MVP (`AssertRelation` / `RetractRelation` / `HideRelation`) runs on top.

**Done when:**

- `Graph::add_edge` / `Graph::remove_edges` / `GraphDelta::{AddEdge, RemoveEdges, ReplayAddEdgeByIds, ReplayRemoveEdgesByIds}` no longer exist.
- New public API: `Graph::assert_relation` (extant), `Graph::retract_relations`, `Graph::append_traversal`, `Graph::relations()` → `Iterator<Item = RelationView>`.
- `RelationKind` enum lives in `mere-kernel` — `Semantic(SemanticSubKind)`, `Containment(ContainmentSubKind)`, `Arrangement(ArrangementSubKind)`, `Imported(ImportedSubKind)`, `Provenance(ProvenanceSubKind)`, `Traversal`.
- `RelationView { from, to, kind: RelationKind }` is the canonical read shape (replaces `EdgeView { edge_type: EdgeType }`).
- `EdgePayload` refactored: typed sidecars authoritative, `families` computed on write/read, `kinds` field gone.
- Semantic label + agent decay attach **per `SemanticSubKind`** (not per payload).
- `NavigationTrigger` expanded with `Redirect`, `ReopenSession`, `JumpAnchor`, `InPageSearchJump`, `ImportedHistory`.
- All workspace + excluded-`mere-host` callers migrated off `EdgeType::*`. `cargo test --workspace` + `cargo test --manifest-path crates/mere-host/Cargo.toml` both green.
- Three new bus actions: `AssertRelation { graph_id, from, to, assertion }`, `RetractRelation { graph_id, from, to, selector }`, `HideRelation { graph_id, view_id, from, to, selector }`. Hit-test surfaces carry `RelationKind` so multi-relation node pairs disambiguate.

## 2. Key API changes

| Removed                                  | Replaced by                                                                 |
| ---------------------------------------- | --------------------------------------------------------------------------- |
| `EdgeType` (public)                      | `EdgeAssertion` (write) + `RelationKind` (read) + `RelationSelector` (filter) |
| `EdgeKind` (public — internal index)     | Computed `EdgeFamily` discriminant from typed payload sidecars              |
| `Graph::add_edge(EdgeType, …)`           | `Graph::assert_relation(EdgeAssertion)`                                     |
| `Graph::remove_edges(EdgeType, …)`       | `Graph::retract_relations(RelationSelector)`                                |
| `Graph::edges()` → `EdgeView`            | `Graph::relations()` → `RelationView`                                       |
| `GraphDelta::{AddEdge, RemoveEdges, …}`  | `GraphDelta::{AssertRelation, RetractRelations, AppendTraversal}`           |

## 3. `RelationKind` vs `EdgeAssertion` — why both

These look shape-adjacent and serve distinct roles:

- **`EdgeAssertion`** is the **write contract**. Carries the full data needed to assert a relation: sub-kind plus per-sub-kind metadata (label, decay state, etc.). Construction-side rich.
- **`RelationKind`** is the **read classifier**. Just family + sub-kind, no metadata. Used by canvas hit-tests, render policy, filter UI, action targets. Think: `RelationKind ≈ discriminant(EdgeAssertion)`, dropping the construction payload.

Keeping them separate avoids forcing read-side code to handle write-side payload variants. A future `From<&EdgeAssertion> for RelationKind` keeps them in sync.

## 4. `RelationOrigin` — deferred

The kind-vs-origin separation (semantic meaning vs. "where the assertion came from": DocumentStructure / User / Agent / NavigationEvent / ImportRecord / DerivedProjection) is acknowledged but not landed in this pass. Reasons:

- The **hide-only vs retract** rule (see §6) handles the immediate authorship question without origin formalization: derived / imported / provenance / hyperlink / traversal-history edges are non-retractable regardless of origin; only user-authored semantic assertions can be retracted.
- Per-sub-kind metadata (§5) handles the AgentDerived decay case (decay attaches to the `Semantic(AgentDerived)` slot, not floating).
- The "agent-asserted `Cites` vs document-asserted `Cites`" case is real but rare and can be solved later — either via a `RelationOrigin` field on `EdgeAssertion` or via a parallel evidence stream on the edge (analogous to how `Traversal` events accumulate temporal evidence).

Filed as a follow-up: separate pass once we have a concrete UX need for distinguishing origins on otherwise-identical relations.

## 5. Per-sub-kind semantic metadata

Today `EdgeAssertion::Semantic { sub_kind, label, decay_progress }` carries label + decay at the assertion level. Refactor so metadata attaches to the sub-kind slot inside `EdgePayload` — not the whole payload. This solves:

- An `AgentDerived` decay no longer floats on the multi-slot edge as a whole.
- Multiple semantic sub-kinds on the same edge (e.g. Hyperlink + AgentDerived) carry independent labels / decay state.
- Retracting one sub-kind (e.g. agent's `Cites` assertion) leaves siblings intact.

`EdgePayload` shape becomes typed sidecars indexed by sub-kind, not a flat set of `EdgeKind`s.

## 6. Edge mutation MVP

Three bus actions, all routing through the typed action bus per [the bus plan](2026-05-11_typed_action_bus_plan.md):

```rust
ActionKind::AssertRelation { graph_id: GraphId, from: NodeKey, to: NodeKey, assertion: EdgeAssertion }
ActionKind::RetractRelation { graph_id: GraphId, from: NodeKey, to: NodeKey, selector: RelationSelector }
ActionKind::HideRelation    { graph_id: GraphId, view_id: ViewId, from: NodeKey, to: NodeKey, selector: RelationSelector }
```

### 6.1 Hide-only vs. retract — by authorship

**Retract** (mutates graph truth) is permitted only on **user-authored** Semantic assertions: the user added it, the user can remove it.

**Hide** (view-local policy; never mutates graph truth) is the default for everything else:

| Relation source                        | Hide | Retract |
| -------------------------------------- | :--: | :-----: |
| User-authored Semantic                 | ✓    | ✓       |
| Document hyperlink (Semantic::Hyperlink) | ✓  | ✗       |
| Derived Containment (UrlPath/Domain)   | ✓    | ✗       |
| Stored Containment (Folder/Notebook)   | ✓    | ✓       |
| Arrangement (workbench-produced)       | ✓    | ✗       |
| Imported (HistoryImport, Bookmark, …)  | ✓    | ✗       |
| Provenance (ClippedFrom, etc.)         | ✓    | ✗       |
| Traversal event                        | ✓    | ✗       |
| AgentDerived Semantic                  | ✓    | ✓ (counts as user-acceptable) |

The principle: deletability tracks authorship, not family.

### 6.2 Hide state storage (v0 + migration)

v0: per-orrery `hidden_relations: HashSet<(StableNodeId, StableNodeId, RelationKind)>` on `OrreryPaneState`. In-memory; lost on app exit.

v1 (when `ViewIntent` sidecar lands per the framing brief §5.3): hide state migrates to the per-pane `ViewIntent` record on disk. Same key shape.

Stable node IDs (not `NodeKey` / petgraph `NodeIndex`) because the latter aren't stable across save/load.

### 6.3 Canvas hit-test carries `RelationKind`

Today's hit-test surface returns `(from, to)`. With multi-relation edges between the same pair, that's ambiguous — right-clicking on a Hyperlink line vs. a Cites line on the same pair needs to identify *which* relation. Hit-test extends to `(from, to, RelationKind)`.

UI implication: one stored edge between (A, B) renders as multiple visual lines (one per family, family-coloured) — *not* a single line with mixed kinds. This is the cleanest mental model for the per-relation hide gesture.

## 7. `AppendTraversal` signature

```rust
GraphDelta::AppendTraversal {
    from: NodeKey,
    to: NodeKey,
    trigger: NavigationTrigger,
    timestamp_ms: Option<u64>, // None → kernel stamps now()
}

ActionKind::AppendTraversal {
    graph_id: GraphId,
    from: NodeKey,
    to: NodeKey,
    trigger: NavigationTrigger,
}
```

Replaces `EdgeType::History` usages. Traversal events accumulate as evidence on the edge's `Traversal` slot; the edge itself is `RelationKind::Traversal` for read views, with `EdgeMetrics` carrying aggregate counts.

## 8. Implementation changes per crate

### 8.1 `mere-kernel`

- Add `RelationKind`, `RelationView`. Expand `NavigationTrigger`.
- Replace `Graph::add_edge` / `Graph::remove_edges` with `Graph::assert_relation` / `Graph::retract_relations` / `Graph::append_traversal`.
- Refactor `EdgePayload`: typed sidecars authoritative; `families` computed; `kinds` removed.
- Migrate per-sub-kind metadata.
- `GraphDelta` replaces `AddEdge` / `RemoveEdges` / `ReplayAddEdgeByIds` / `ReplayRemoveEdgesByIds` with `AssertRelation`, `RetractRelations`, `AppendTraversal`.
- Snapshot reader/writer reads/writes typed relation data only.

### 8.2 `mere-host` (excluded from workspace)

- `host_helpers::ensure_node_for_address_near`: `add_edge(EdgeType::Hyperlink)` → `assert_relation(EdgeAssertion::Semantic { sub_kind: Hyperlink, label: None, … })`.
- `demo::render_demo_graph_state`: same migration.
- New bus actions wired into `host_action_bus` execute: `AssertRelation`, `RetractRelation`, `HideRelation`, `AppendTraversal`.
- `OrreryPaneState` gains `hidden_relations` field for v0 hide storage.

### 8.3 `mere-host-runtime`

- `session_graph_store` test fixtures migrate off `EdgeType::Hyperlink`.

### 8.4 `cartography`

- Edge weight policy: replace flat `1.0` weight per edge with family-aware policy (`shared.rs:83`). Defaults: Semantic visible + pulling; Traversal hidden by default; Imported / Provenance hidden by default; Containment / Arrangement lens-driven.
- `graph_canvas/radial.rs` test fixtures migrate.

### 8.5 `graph-tree` interaction

`graph-tree`'s per-workbench `Provenance::Traversal { source, edge_kind }` member-entries (from the node-per-tile v1 lineage facet) are **distinct** from kernel-level `Traversal` events on edges:

- **graph-tree** records *per-workbench projection*: "how the user moved through anchors *in this workbench*." Member entries on `GraphTree<NodeKey>`. Per-workbench scope.
- **Kernel `Traversal`** records *graph-truth-level navigation evidence*: "this navigation event happened, with this trigger, at this time." Lives on the edge between two anchor nodes. Graph-wide scope.

The two coexist with overlapping but non-identical semantics. graph-tree is the lineage projection layer; the kernel `Traversal` family is the durable evidence stream.

### 8.6 `platen` + arrangement edges

Per platen's existing role as workbench/frame arrangement owner: arrangement state stays workbench-local **until saved**. Only explicit "save frame" / "pin workspace" gestures mint durable `Arrangement::FrameMember` graph relations via `assert_relation`. Don't write arrangement edges on every splitter drag.

### 8.7 `graphshell` (external repo)

Out of scope. graphshell is a grab-bag of ideas, not Mere's authority (see memory: `project_graphshell_donor_not_authority`). Mere's relation model is canonical for Mere; graphshell may eventually adopt similar shape inside its `mere-domain/graphshell` + `mere-host/graphshell` crates as it shrinks, but Mere doesn't update graphshell docs to track this work.

## 9. Node navigation memory preserved

`NodeNavigationMemory` (per-node back/forward authority) remains separate from edge-level `Traversal` events. Updated via `GraphDelta::UpdateNodeHistory`, not by emitting traversal edges. Two distinct concerns:

- Per-node history: a property of the node — what the user navigated *while focused on this anchor*.
- Edge traversal events: evidence on edges — "the user moved from A to B at time T via trigger X."

Long-term, `NodeNavigationMemory`'s generic params upgrade from `String, String, ()` to an `EntryKey` + visit context once the Entry/Visit identity layer (from `graph-memory`) is wired. That's a separate pass; not in scope here.

## 10. Browser history import rule

Imported browser history:

- **Confidently creates**: nodes, visit metadata, import records, `Imported(HistoryImport)` relations, `Provenance(ImportedFromSource)` relations.
- **May create directed traversal events** when source data has referrer / opener / transition / redirect / session-or-window ordering — annotated with `NavigationTrigger::ImportedHistory`.
- **Must not synthesize semantic hyperlinks from chronological adjacency alone.** Plain "row N came before row N+1 in the export" is not evidence of a link. At most it becomes an *imported traversal hypothesis* with `ImportedHistory` trigger, and only when the importer can bound the session well enough.

Per-source strength of evidence:

| Signal                                     | Strength |
| ------------------------------------------ | -------- |
| URL, title, visit count, last visit time   | Strong   |
| Source browser / profile, bookmark folders | Strong   |
| Referrer / from-visit chains               | Strong if present |
| Transition type                            | Strong if present |
| Session / window / tab grouping            | Strong if present |
| Chronological adjacency without bounds     | Weak — not enough for traversal |

## 11. Snapshot version handling

Mere is pre-publication; the only snapshots that exist are local dev fixtures. Pre-cleanup snapshots with `EdgeType`-flavoured persisted data **will not load** after this pass. Recovery: delete `<data_dir>/mere/mere-host/sessions/` and let the bootstrap reseed.

The kernel snapshot format already keeps typed persisted fields (`PersistedSemanticEdgeData`, `PersistedContainmentEdgeData`, etc.) — only the in-memory `EdgeType` constructions need migrating. No version bump needed for typed persisted fields; they were always the canonical form. The legacy in-memory `EdgeType` does **not** get a compatibility wrapper.

## 12. Suggested sequencing — 5 green commits

Stage as five PRs/commits, each leaving the codebase green:

1. **Add new types.** ✅ Landed. `RelationKind`, expanded `NavigationTrigger`, per-sub-kind metadata slots on `EdgePayload`. Pure additions. No callers migrated.
2. **Add new write API.** ✅ Landed. `Graph::retract_relations`, `Graph::append_traversal`, `Graph::relations()` → `RelationView`. `Graph::assert_relation` already existed.
3. **Migrate Mere-owned callers off old API.** ✅ Landed. `mere-host` (`host_helpers`, `demo`), `mere-host-runtime` test fixture, `cartography` test fixtures, `graphshell` composition tests.
4. **Remove old API.** ✅ Landed. `EdgeType`, `EdgeKind`, `UserGroupedData`, `Graph::add_edge`, `Graph::remove_edges`, `Graph::edges()`, `EdgeView`, and the `GraphDelta::{AddEdge, RemoveEdges, ReplayAddEdgeByIds, ReplayRemoveEdgesByIds}` variants are gone. `EdgePayload` refactored: typed sidecars authoritative, `families()` computed on demand, `kinds` field removed. `apply.rs` / snapshot reader+writer / `cartography` / `platen` migrated. `cargo test --workspace --exclude mere-host` + `cargo test -p mere-kernel` + `cargo check --manifest-path crates/mere-host/Cargo.toml` all green.
5. **Edge mutation MVP.** ✅ Landed. `ActionKind::{AssertRelation, RetractRelation, HideRelation, AppendTraversal, RemoveNode}` bus actions + host execute wiring. `GraphDelta::AppendTraversal` signature is now `{ from, to, trigger, timestamp_ms }` per §7. `graph-canvas::derive_edges` emits `HitProxy::Edge` so the engine's `hovered_edge` populates. `OrreryPaneState.hidden_relations` (UUID-keyed) + `CanvasSceneOptions.hidden_relations` (NodeKey-keyed) filter; orrery `render` / `derive_scene_for_bounds` thread the set through. Orrery edge right-click → context menu listing every relation on the pair with "Hide …" (always) + "Delete …" (user-authored only, per §6.1). Node menu's "Delete node" is enabled → `RemoveNode`.

   **Deviations from the plan as written, all deliberate v0 simplifications:**
   - `HideRelation` is scoped via `ActionTarget::Pane(pane_id)` instead of a `view_id: ViewId` field — the orrery pane *is* the view in v0. The `ViewIntent` sidecar (§5.3) will reintroduce an explicit view id.
   - ~~Hit-test was **not** extended to carry `RelationKind`, and edges still render as one line per pair (not per-family-coloured lines).~~ **Landed as a follow-up commit.** Multi-relation pairs now render as parallel family-coloured lanes; each lane carries an opaque `tag: Option<u32>` (the `RelationKind::tag()` ordinal) through `CanvasEdge` → `HitProxy::Edge` → `EdgeRef`; the orrery edge menu reads the tag for precise relation targeting and falls back to "list all on pair" when absent.
   - "Create node here" is **deferred** — it needs an in-menu text-input affordance for the address, which the context-menu component doesn't have yet. `CreateNode` was not added as a dead `ActionKind`.
   - `ActionKind::AppendTraversal` exists with a correct execute arm but no UI caller yet; navigation-emits-traversal is a separate wiring slice.

## 13. Test plan

- `cargo test --workspace` in `repos/mere`.
- `cargo test --manifest-path crates/mere-host/Cargo.toml` (workspace-excluded crate; vendored gpui dep keeps it out of the main workspace).
- New focused tests:
  - Multi-relation payloads — assert + retrieve multiple sub-kinds on one edge.
  - Per-sub-kind semantic metadata — label + decay round-trip per slot.
  - Relation retraction preserves siblings — retract one sub-kind, others intact.
  - Traversal snapshot roundtrip — events + metrics survive save/load.
  - Typed import / provenance roundtrip — `ImportedSubKind` and `ProvenanceSubKind` payloads.
  - View-local hide doesn't mutate graph truth — `HideRelation` toggles state, `relations()` output unchanged.
  - Adapter tests proving canvas/cartography weights derive from `RelationKind` / `EdgeFamily`, **not** `EdgeType`.

## 14. Assumptions

- Kernel-first cleanup sequencing.
- Clean replacement — no public `EdgeType` deprecation shim.
- `Traversal` remains the one family without assertion sub-kinds; temporal nuance lives in `NavigationTrigger`.
- Browser-history import semantics specified here; a full Chrome / Firefox importer is a later slice.
- `RelationOrigin` deferred; hide-only-vs-retract handles authorship for v0.
- Pre-publication: no snapshot back-compat owed.

## 15. Open questions

1. **`StableNodeId` shape for hide-key**. The kernel has `get_node_key_by_id` for petgraph `NodeIndex → Uuid` round-trips. v0 of hide uses `(GraphId, Uuid, Uuid, RelationKind)`. When `ViewIntent` sidecar lands, this serialises naturally. Confirm no smaller / better-suited identifier already exists.
2. **`AgentDerived` retract policy.** Listed as retractable in §6.1 because the user explicitly accepting an agent suggestion makes them an authoring party. If the suggestion was never accepted (still decaying), is it retractable? Probably yes — "dismiss this agent suggestion" is a meaningful gesture. Worth pinning during execution.
3. **Edge-row UX in the node facet menu.** Delete-edge UI (when it lands later) lives in the node's facet menu or a future history-subsystem panel. Neither exists yet. Implementation of the deeper-retract surface is its own slice after the edge mutation MVP ships.
