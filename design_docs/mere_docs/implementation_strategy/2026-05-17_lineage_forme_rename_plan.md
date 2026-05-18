# Lineage + Forme rename plan

**Date**: 2026-05-17
**Status**: Mechanical rename + framing clarification. Two mere-workspace crates renamed to better names; four duplicate donor crates deleted from the graphshell repo.

## Goals

1. Rename `graph-memory` → `node-lineage` (Mere). "Memory" misnames it — that role belongs to eidetic. The crate is *navigation lineage*, with two natural granularities: url→url (within-tile, branchable internal lineage) and tile→tile / node→node (when a branch assumes its own identity, becoming a directed edge on the graph).
2. Rename `graph-tree` → `forme` (Mere). "Tree" undersells it — it's the per-graph-view *arrangement authority* that projects graph members + edges into a workbench arrangement (which may or may not be tree-shaped).
3. Sharpen the printing-press boundary in the working-principles index:
   - `forme` = semantic/workbench arrangement authority
   - `platen` = graph-aware print/composition stage that turns the forme into surface/pane output
   - `verso-tile` = receptor of surface/tile lifecycle, communicating rearrangement up the stack from tile → forme → graph truth
4. Add a credit to history-tree in `node-lineage/README.md` (Atlas Engineer, BSD-3-Clause). Attribution is not legally required — the Rust code is an independent reimplementation against the same abstract data model — but credit is the right move regardless.
5. Cascade-delete the four donor-superseded crates in the legacy graphshell repo: `graph-memory`, `graph-cartography`, `graphshell-core`, `graphshell-runtime`. Mere has fully absorbed each of their roles (mere-kernel, cartography, mere-host-runtime / mere-host).

## Done conditions

- `cargo check -p node-lineage` passes.
- `cargo check -p forme` passes.
- `cargo check -p mere-kernel` passes (downstream consumer of node-lineage).
- `repos/mere/crates/graph/graph-memory/` and `repos/mere/crates/graph/graph-tree/` no longer exist.
- `repos/graphshell/crates/{graph-memory,graph-cartography,graphshell-core,graphshell-runtime}/` no longer exist; the graphshell `Cargo.toml` workspace members list reflects this.
- All in-repo references to `graph-memory` / `graph_memory` / `graph-tree` / `graph_tree` are updated (Cargo deps, `use` statements, `pub use` re-exports, doc-comment headers, design docs).
- DOC_README's printing-press metaphor entry mentions `forme` between `inker` and `platen`.

## Non-goals

- Not renaming graphshell repo's `graph-tree` or `graph-canvas` — out of scope; the user opted into deleting exactly the four duplicates named above. Those graphshell-side crates stay until separately addressed.
- Not preserving graphshell-side build cohesion after the cascade delete. The graphshell binary depends on `graphshell-runtime` and `graphshell-core` (per [graphshell Cargo.toml:133, 218](../../../../graphshell/Cargo.toml#L133)) so it will not build after this work. Per user direction, "I am not gonna run graphshell again." Graphshell will be archived as a unit in a separate pass.
- Not rewriting `forme`'s internals. Module layout (`graphlet.rs`, `layout.rs`, `lens.rs`, `member.rs`, `memory_policy.rs`, `nav.rs`, `parity.rs`, `query.rs`, `reconciliation.rs`, `topology.rs`, `tree.rs`, `ux.rs`) stays as-is; only the crate name and lib.rs doc header change. Module rename (`tree.rs` → something else, `memory_policy.rs` → `lifecycle.rs`) is follow-up.
- Not implementing the layered node-lineage model (url→url + tile→tile granularities). That's a design clarification, not a code change in this pass — the node-per-tile lineage plan already covers the work.

## Sequence

1. **Write this plan** (you're reading it).
2. **Move directories**: `crates/graph/graph-memory/` → `crates/graph/node-lineage/`; `crates/graph/graph-tree/` → `crates/graph/forme/`.
3. **Update Cargo.tomls**:
   - The two crate manifests (new name, new description).
   - Mere workspace root `Cargo.toml` (workspace members paths).
   - All consumer Cargo.tomls (dep name + path).
4. **Update source references**: `use graph_memory::` → `use node_lineage::`; `use graph_tree::` → `use forme::`; `pub use graph_memory as memory` → `pub use node_lineage as lineage`; doc-comment headers in both renamed lib.rs files. Refresh `forme`'s lib.rs doc to describe its role as the per-graph-view arrangement authority (not "tile tree").
5. **Add node-lineage README** with history-tree credit + the layered-lineage framing.
6. **Targeted validation**: `cargo check -p node-lineage`, `cargo check -p forme`, `cargo check -p mere-kernel`.
7. **Cascade delete graphshell repo**: remove the four crate directories and prune `graphshell/Cargo.toml` workspace members.
8. **Update mere design docs**: DOC_README (printing-press metaphor + crate references), [tearout brief](../research/2026-05-11_tearout_operations_brief.md), [node-per-tile lineage plan](2026-05-11_node_per_tile_lineage_plan.md), [graphshell harvest brief](../research/2026-05-17_graphshell_harvest_brief.md).

## Findings

- `graph-memory`'s Rust code is an independent reimplementation against the same abstract data model as Atlas Engineer's [history-tree](https://github.com/atlas-engineer/history-tree) (BSD-3-Clause). It shares concepts (Entry, Visit, Owner, parent/children, deduplication-by-key) but no expression (Rust uses `slotmap::SlotMap` + generics; Lisp uses CLOS classes + hash-tables). Under US copyright law (Baker v. Selden 1880; Oracle v. Google 2021 on APIs), algorithms and abstract data models are not copyrightable — only expression is. BSD-3 attribution is therefore not legally binding. README credit is best practice anyway.
- Mere consumers of `graph-memory`: `mere-kernel` ([Cargo.toml:15](../../../crates/mere-kernel/Cargo.toml#L15), `src/graph/history.rs`, `src/graph/mod.rs`), `crates/graphshell` ([Cargo.toml:20](../../../crates/graphshell/Cargo.toml#L20), `src/lib.rs:33` re-exports as `memory`), `mere-host-contract` (comment in `src/frame_projection.rs:9`).
- Mere consumers of `graph-tree`: `crates/graphshell` ([Cargo.toml:21](../../../crates/graphshell/Cargo.toml#L21), `crates/graphshell/shell-state/Cargo.toml:19`, `src/app_state/intent_system.rs:12`, `src/app_state/persistence.rs` — multiple references), `graph-canvas` (lib.rs doc comment).
- Graphshell repo consumers of `graph-memory`: `graph-cartography` ([Cargo.toml:12](../../../../graphshell/crates/graph-cartography/Cargo.toml#L12), `src/lib.rs` — multiple `graph_memory::` imports), `graphshell-core` ([Cargo.toml:16](../../../../graphshell/crates/graphshell-core/Cargo.toml#L16), `src/graph/mod.rs:15`).
- Internal naming inside `forme`'s code (after the rename, `parity.rs` still uses identifiers like `graph_tree_parent`, `graph_tree_only`, `in_graph_tree`). These are local names for diagnostic-report fields comparing the forme's view of the graph to the graph's view of itself. They're not crate-name references and can stay until a separate cleanup pass — or be renamed in step 4 if cheap. Tag as a small follow-up.

## Risks

- **Graphshell crate's `pub use graph_memory as memory` re-export becomes `pub use node_lineage as lineage`** — any code reading `graphshell::memory::Foo` needs to switch to `graphshell::lineage::Foo`. Grep before renaming to avoid silent breakage.
- **Doc links across `design_docs/` will rot** if not updated this pass. The tearout brief and lineage plan reference `crates/graph/graph-tree/...` paths that will 404 in the IDE link follower after the move.
- **Targeted cargo check trust** — per workspace memory, sibling crates have in-flight work; broader `cargo check --workspace` may emit noise unrelated to this rename. Trust targeted `-p <crate>` results.

## Progress

**2026-05-17 — landed in one pass:**

- `graph-memory` → `node-lineage` via `git mv`. Cargo.toml name + description updated. `src/lib.rs` doc header updated to describe the layered-lineage framing (url→url + node→node granularities).
- `graph-tree` → `forme` via `git mv`. Cargo.toml name + description updated. `src/lib.rs` doc header rewritten to describe `forme` as the per-graph-view workbench arrangement authority and the corrected printing-press boundary (graph truth → forme → platen → verso-tile).
- Workspace `Cargo.toml` members updated.
- Mere-side consumers updated: `mere-kernel` (Cargo.toml + `src/graph/history.rs` + `src/graph/mod.rs`), `crates/graphshell` (Cargo.toml + `src/lib.rs` re-export + README), `crates/graphshell/shell-state` (Cargo.toml), `crates/graphshell/src/app_state.rs` + `app_state/intent_system.rs` + `app_state/workspace_routing.rs` (use statements), `mere-host-contract` (Cargo.toml + `frame_projection.rs`), `mere-host` (Cargo.toml workspace + dep + `host_navigation.rs` + `pane_state.rs` + `tearout.rs`), `mere-domain/graphshell` (Cargo.toml + `frame_model.rs`), `graph-canvas` (doc comment).
- `node-lineage/README.md` created with credit to Atlas Engineer's `history-tree` (BSD-3-Clause) — abstract data model adapted, no expression translated, attribution best-practice not legally binding.
- Targeted `cargo check -p node-lineage -p forme -p mere-kernel -p graphshell -p mere-host-contract` — all green.
- Cascade-deleted in graphshell repo: `crates/graph-memory`, `crates/graph-cartography`, `crates/graphshell-core`, `crates/graphshell-runtime`. Graphshell `Cargo.toml` workspace members pruned + commented-out explanation added.
- Design docs: DOC_README printing-press metaphor rewritten in two threads (per-node content production + per-graph-view workbench arrangement); rename plan added to index; tearout brief + node-per-tile lineage plan got top-of-doc naming notes pointing at this plan.

**2026-05-17 (same day) — second pass — landed:**

- `forme/src/parity.rs` internal field-name rename: `graph_tree_parent` → `forme_parent`, `graph_tree_only` → `forme_only`, `graph_tree_order` → `forme_order`, `in_graph_tree` → `in_forme`, `MissingFromGraphTree` enum variant → `MissingFromForme`, `ActiveMismatch.graph_tree` field → `ActiveMismatch.forme`. Local `gt_*` vars kept as-is (scoped, harmless).
- `graphshell/src/app_state/persistence.rs` + `services.rs` + `app_state.rs` downstream API rename: `load_workbench_graph_tree` → `load_workbench_forme`, `save_workbench_graph_tree` → `save_workbench_forme`, `WorkspaceRepository::load_graph_tree` → `load_forme`, `WorkspaceRepository::save_graph_tree` → `save_forme`, `GraphTreeDocument` type → `FormeDocument`, `graph_trees` HashMap field → `formes`, test `workbench_graph_tree_round_trips_*` → `workbench_forme_round_trips_*`.
- Module rename inside `forme`: `memory_policy.rs` → `pressure.rs`; type `MemoryPolicy` → `PressurePolicy`; module doc rewritten to explain naming (eidetic owns "memory"; this module is resource-pressure policy, not memory policy). `tree.rs` left as-is because the central type stays `GraphTree<N>` for now; renaming the type is a bigger surface change deferred to its own pass.
- Prose updates in `tearout brief` and `node-per-tile lineage plan` bodies: replace_all `graph-tree` → `forme` inside the body (top-of-doc naming notes kept as historical context).
- Targeted `cargo check -p forme -p graphshell` after second pass — green.

**Spatial-chrome plan amendments (also 2026-05-17) — Tier 1 of harvest brief absorbed**:

- Phase 2: named the `<op>.started` / `<op>.succeeded` / `<op>.failed` triple convention for long-running operations with timeout contracts; `ChannelRegistry` descriptors as declarative config separate from live analyzers (T1-3).
- Phase 3: added composition-pass ordering invariant on `SubstrateScene` to done-conditions — chrome → content → overlay layering, type-level invariant under a free-zoom camera (T1-2).
- Phase 5: introduced `EnvelopeCapabilityProfile` as the single host capability/degradation contract, with envelope filtering as the outer layer and the existing per-action capability-gate catalogue as the inner layer (T1-4).
- New "Cross-cutting prerequisite" section between §0 and Phase 1: single-write-path invariant on mere-kernel's `Graph` (`pub(crate)` mutators + `INV-1`-style runtime check + typed action bus as sole intent carrier) — sits above the spatial-chrome lane, blocks none of it, but converts conventional invariance to enforced (T1-1).

**ProjectionBindingMode (T1-5) absorbed across two adjacent briefs:**

- [`cartography brief §4`](../research/2026-05-10_cartography_layer_brief.md): added `binding_mode: ProjectionBindingMode` field on `Projection`, with `Linked` / `Unlinked` variants and explanation of which scenarios pick which default.
- [`view intent sidecar plan §2`](2026-05-14_view_intent_sidecar_plan.md): added `binding_mode` to the future-fields list, gated on cartography shipping the linked/unlinked split. Default for a live pane = `Linked`; default for switcher thumbnails / exports = `Unlinked`.

**Still deferred (separate-pass follow-ups)**:

- Type rename `GraphTree<N>` → `Forme<N>` (or similar) inside `forme/src/tree.rs` + all consumers — larger surface change; touches every site that names the type. Defer until naming around forme has fully bedded down (the user may want to revisit type-level naming once `arrangement` / `lattice` etc. patterns emerge in usage).
- Other design docs that mention `graph-tree` / `graph-memory` in passing (`local_intelligence_integration_research`, `browser_multiplexer_framing`, `memory_tiers_brief`, `relation_taxonomy_and_edge_mutation_plan`, the historical `graph_canvas_field_algebra_plan` in `graphshell_docs/`) — left as-is; readers cross-reference DOC_README's working-principles entry for current names.
- Graphshell repo will be archived as a unit in a separate pass; this rename pass deleted exactly the four crates that were donor-superseded duplicates.
