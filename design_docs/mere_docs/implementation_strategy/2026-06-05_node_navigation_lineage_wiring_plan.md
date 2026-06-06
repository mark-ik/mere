# Node navigation + lineage — wiring plan (live meerkat/orrery/kernel path)

**Date**: 2026-06-05
**Status**: Implementation plan — pre-build
**Scope**: Drive the already-built per-node navigation-lineage substrate from the live navigation path, so navigating the focused tile changes *that node* in place (within-node history), an explicit gesture mints a new node (a new browsing surface) with a typed lineage edge back to its origin, and the two histories (within-node back/forward, across-node previous/next) become real. This is a **wiring + finish** job: the model is decided and the substrate exists; the live path never drives it.

## Supersedes / builds on

- [`2026-05-11_node_per_tile_lineage_plan.md`](2026-05-11_node_per_tile_lineage_plan.md) — same intent, but its file/type references (`mere-host`, `TileManager`, host-side per-tile history) predate the meerkat/orrery/platen architecture **and** the substrate has since moved onto the kernel `Node` itself (`Node.navigation_memory`). Architecture-stale; this plan replaces its "where" while keeping its "what."
- [`2026-05-18_node_identity_and_duplicates_plan.md`](2026-05-18_node_identity_and_duplicates_plan.md) — decided UUID identity, no URL-dedup, `find` vs `create` split, the `OpenAddressAsNewNode` gesture, lineage-as-address-history, sibling graphlets. Same architecture-staleness caveat. This plan carries its gesture/identity decisions onto the live path.
- `node-lineage` crate ([`crates/graphshell/graph/node-lineage`](../../../crates/graphshell/graph/node-lineage)) — the Entry/Visit/Owner engine. Built, tested. Unchanged by this plan except possibly small additive incremental ops.
- [`history.rs`](../../../crates/graphshell/graph/graph-kernel/src/graph/history.rs) — `NodeNavigationMemory`, the per-node projection over node-lineage, already a field on every `Node`. Needs incremental nav ops added.

## 1. The model (as confirmed 2026-06-05)

- A **node is a browsing surface** (UUID identity; duplicate URLs welcome). It carries its own internal history: a forkable tree of visits. **Forward-fork**: going back then navigating to a new URL spawns a branch off the current visit, the prior forward branch is preserved (never truncated). This is exactly what `node-lineage`'s append-only `visit_entry` does and what `Node.navigation_memory` already stores.
- **Navigating in place** (omnibar Enter, plain link click) extends *the focused node's* history and changes the page it shows. It mints no node.
- **Within-node history** ↔ back/forward (in a tile, walks that node's visit tree).
- **Across-node relations, three buckets by strength, distinct edge styles:**
  1. **navigated-from** (strongest) — minted by "open in new tile/node" (context menu, middle-click, Ctrl/Cmd-Enter, Ctrl+left-click). A new node + an edge back to the origin, anchored at the origin's **current visit** (a distinct anchor each time, even on revisit).
  2. **containment** (medium, "parallel/proximate") — grouped nodes; a graphlet.
  3. **chronology** (weakest, accessible) — temporal proximity; the MRU thread.
- **Graphlets** emerge from containment + chronology when there is no navigated-from link.
- **Across-node history** ↔ previous/next = **most-recently-used** ordering of nodes. Belongs in **gloss** (the content strip; the graphshell-era "navigator"). Net-new; gloss does not exist in the live shell yet, so this is the heaviest, latest phase.
- **Diffable** (future, not this plan): comparing two nodes' histories for similarity to derive relations.

## 2. Findings — what exists vs what's wired

- **Built + persisted:** `Node.navigation_memory: NodeNavigationMemory` on every kernel node, with `history_projection()` / `current_history_url()` / `history_branch_projection()` / `replace_history_state()`, round-tripped through the snapshot. The forkable, append-only, branch-aware history is done.
- **Dormant:** the only callers of the history API are tests and snapshot I/O. The live navigation path (`Chrome` omnibar → `sync_orrery` → `orrery.visit`) never touches `navigation_memory`. `orrery.visit` ([orrery-host/src/lib.rs:547](../../../crates/orrery-host/src/lib.rs#L547)) still does the *old* model: dedup by URL, mint a node, add a hyperlink edge, move the single orrery cursor. That mismatch is the exact bug: navigating the focused tile mints a stray node instead of advancing the node.
- **Net-new:** the across-node MRU log (previous/next) has no substrate anywhere. The "activation" in the constellation is content-actor rendering, not a navigation log.
- **Edge taxonomy:** the kernel already has a relation taxonomy ([`edge_taxonomy.rs`](../../../crates/graphshell/graph/graph-kernel/src/graph/edge_taxonomy.rs)); Phase 3 maps the three buckets onto it (verify exact `EdgeFamily`/sub-kind variants at build time) rather than inventing new edge kinds.

## 3. Phases (done-conditions, not dates)

### Phase 1 — within-node navigation (kills the escape-to-graph bug)

- Add incremental ops to `NodeNavigationMemory`: `record_visit(url, transition, at_ms)` (drives `owner.visit_entry`, forward-fork falls out), `back()/forward()` (drive `owner.back/forward`), `can_back()/can_forward()`. Thin wrappers over node-lineage already-present methods.
- Kernel `Graph`/`Node` method to navigate a node in place: append a visit to its `navigation_memory` and update its current page; **no new graph node**.
- meerkat: when a tile is focused (Tree) or a node is focused (Cartography) and the omnibar submits, or a plain link is clicked, navigate **that node** in place. The tile shows `node.current_history_url()`. Stop routing in-place navigation through `orrery.visit`'s create-node path.
- Chrome back/forward operate on the focused node's `navigation_memory`; `can_back/can_forward` gate the buttons.
- Omnibar follows the focused node's current page (already does via `sync_location`; confirm it reads `current_history_url`).
- **Done when:** navigating the focused tile changes its page in place, stays in the workbench, builds the node's history, and back/forward walk that history (with forward-fork preserved). No stray nodes minted on navigation. Targeted kernel + meerkat tests green.

### Phase 2 — branch / new-node gestures

- Explicit "open in new node/tile": context-menu item, middle-click, Ctrl+left-click on a link, Ctrl/Cmd-Enter in the omnibar (per the 05-18 `OpenAddressAsNewNode` decision).
- Each mints a new node (`create_node_for_address`, never dedup) and a **navigated-from** edge = `Semantic::Hyperlink` (the strongest tier, P3; via `assert_relation`) from the source node. A weak `Traversal` event is also recorded for the chronology layer (P3 tier 4).
- **Anchor — resolved to (b):** history moves to a **shared `GraphMemory`** with each node an `Owner`, so a child node's `origin`/`pending_origin_parent` points natively at the parent's current visit (node-lineage's `creator` machinery). This restructures `history.rs` from one `GraphMemory`-per-node into one shared visit space with node-owners; the anchor becomes correct-by-construction and within-node + cross-node lineage unify in one tree. Bigger lift than carrying a visit id on the edge, chosen for faithfulness.
- New nodes **stack into the active slot by default** (per Mark): they join the focused tile's slot as a new tab, active.
- A "new node with no origin" path (empty omnibar / explicit new) mints a node with no navigated-from edge (graphlet candidate).
- **Done when:** the gestures mint distinct nodes with correct anchors; new nodes stack into the active slot; revisiting an origin page yields a fresh distinct anchor.

### Phase 3 — relation strength tiers + styles (reviewed + corrected 2026-06-05)

Not three buckets onto three families — the 6 `EdgeFamily` variants ordered by association strength, each a distinct style. Order **strong → weak** (corrected; the first audit had it inverted), against [`edge_taxonomy.rs`](../../../crates/graphshell/graph/graph-kernel/src/graph/edge_taxonomy.rs):

1. **Semantic (strongest)** — direct navigation links. `Semantic::Hyperlink` is the spine (the link followed A→B); other semantic sub-kinds (Cites/DependsOn/Contradicts/…) ride the same tier. **This is "navigated-from."** The current browse trail already uses `Semantic::Hyperlink` (`orrery build::hyperlink()`), so the strong family is correct — **keep it** (reverses the earlier "replace with Traversal").
2. **Containment** — hierarchical nesting (UrlPath/Domain/FileSystem/UserFolder/CollectionMember).
3. **Arrangement + Provenance (middle)** — `Arrangement` is the graphlet tier (neighbors-of-neighbors, "tiled together"); graphlets are *also* formed by user-grouping, tags, edge-family filtering/toggling, and search, not only spatial arrangement. `Provenance` (origin/derivation: ClippedFrom/GeneratedFrom/…) sits near here as "origin" (least-discussed; tentative).
4. **Traversal (weakest)** — the temporal / UI-UX / **chronology** layer (timestamped events + `EdgeMetrics`; recorded via `append_traversal`, no sub-kind, keyed by `NavigationTrigger`). This is the chronology home (reverses the earlier "no family"). Short-term traversal logs in-graph; **long-term browse history scopes to `eidetic`**, not the live graph.

`Imported` is the remaining family (external import provenance) — weak/external, its own style.

- **Done when:** edges render by tier (Semantic strongest → Traversal faintest), visually distinct in cartography (orrery and swatches).

**Implementation (2026-06-06):** harvest `register-theme/edge_style.rs` (`EdgeStyleToken`, non-color signatures, endpoint markers) — but **map forward to the live kernel's six families** (incl. `Provenance`); the donor has five, don't copy the enum. First insertion: `platen/platen/src/canvas_scene.rs` (~`family_color`), which already keeps `RelationKind`/tags. **Wrinkle:** `platen::orrery` (`orrery.rs`) collapses all relations between a node pair into one drawn line — so family-accurate styling needs either preserving `RelationKind` into the projection or accepting "one representative style per pair" for now. Start in `canvas_scene` (RelationKind preserved); fix the orrery collapse when layered relation edges matter visually. (See the [peripheral panes architecture](../technical_architecture/2026-06-06_peripheral_panes_architecture.md) harvest notes.)

### Phase 4 — across-node MRU (previous/next) + gloss + lineage swatch

- The MRU is a **projection over the Traversal layer** (`EdgeMetrics.last_navigated_at` / a node-activation log), not net-new substrate (corrects the earlier "no substrate"). previous/next step the MRU.
- Controls live in **gloss** (the navigator strip; the graphshell-era "navigator"). Build the minimal gloss surface in the live shell if absent.
- **Long-term browse history scopes to `eidetic`**, not the live graph; the in-graph Traversal layer is short-term.
- **Lineage swatch:** project a node's within-node history (its visit tree) as discrete nodes in a side **swatch** (a mini-cartography), arrangeable like the orrery. Cartography arrangement applies in the orrery *and* in swatches.
- **Done when:** previous/next walk the MRU from gloss (distinct from within-node back/forward); a node's lineage renders as a swatch.

### Phase 5 (future, not scheduled) — diffable node-history similarity

- Compare two nodes' visit trees for similarity, derive relations from the overlap.

## 4. Risks / watch-items

- **node.url() vs current_history_url():** the content actor renders `node.url()`. Phase 1 must make the node's shown page = its history cursor (either update the url field on each visit, or render `current_history_url()`). Pick one and keep it single-source.
- **orrery single cursor vs per-tile focus:** `orrery.visit` moves one selection cursor. Per-node navigation must target the focused tile's node, not the global cursor. In Cartography the focused node is the target; in Tree the focused tile's node.
- **Timestamps:** node-lineage needs `at_ms`. The live app supplies a real clock (fine; only workflow scripts lack one).
- **600-LOC ceiling:** new kernel nav ops may want their own module; meerkat nav routing already lives across main.rs — watch the split.
- **AddressClaim:** the 05-18 plan's `Node.addresses: Vec<AddressClaim>` may or may not have landed; Phase 1 works against `node.url()`/`current_history_url()` regardless. Reconcile in Phase 2 if claims exist.

## Findings

- **2026-06-05 — P3 audited + corrected against `edge_taxonomy.rs`.** `EdgeFamily` has 6; the write contract `EdgeAssertion` has 5 (no Traversal — recorded as timestamped events via `append_traversal`, the chronology layer). Strength order **corrected** (the first audit inverted it): **Semantic::Hyperlink strongest** (= navigated-from; the current browse-trail family is right, keep it) → Containment (hierarchical) → Arrangement (graphlets; also from user-grouping/tags/filter/search) + Provenance (origin) → **Traversal weakest** (temporal/chronology; long-term history → `eidetic`). Anchor fork **resolved to (b)**: shared `GraphMemory` with node-owners (`history.rs` restructures from per-node to one shared visit space). MRU = projection over Traversal, not net-new. Lineage projects as a side swatch (mini-cartography); cartography arrangement applies in orrery and swatches.

## Progress

- **2026-06-05** — Plan written. Investigation confirmed: `node-lineage` + `Node.navigation_memory` built and persisted but dormant; live nav (`orrery.visit`) still URL-dedup + mint-node; across-node MRU is net-new; two prior plans (05-11, 05-18) architecture-stale. Smolweb transport (`errand`) landed earlier same session and is unrelated except both touch the navigation/fetch path.
