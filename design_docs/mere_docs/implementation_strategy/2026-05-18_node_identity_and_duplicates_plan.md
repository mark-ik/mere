# Node identity + duplicates — implementation plan

**Date**: 2026-05-18
**Status**: Implementation plan. Phases the work decided in the [node identity + duplicates brief](../research/2026-05-18_node_identity_and_duplicates_brief.md). Greenfield (pre-1.0); no compat shims expected.

**Brief**: [`../research/2026-05-18_node_identity_and_duplicates_brief.md`](../research/2026-05-18_node_identity_and_duplicates_brief.md).

## Sequence

Four discrete phases. Each is one commit. Phases 1–3 must land in order (each depends on the previous); Phase 4 (PeerID rename) is independent and can land alongside any of the others or alone.

1. **mere-kernel**: `AddressRole` + `AddressClaim` types, `Node` field swap, `find_node_by_address` lookup.
2. **mere-host**: helper split (`find_node_by_address` + `create_node_for_address`), bootstrap/navigation/tearout call site updates.
3. **mere-host + action bus**: explicit `OpenAddressAsNewNode` gesture.
4. **mere-transport**: `NodeId` → `PeerID` rename. Independent.

## Phase 1 — mere-kernel: address claims + lookup

**Goal**: address representation on `Node` becomes structured + role-bearing; canonical lookup by address joins the existing UUID lookup as a first-class API.

**Work**:

1. New types in `mere-kernel::address`:
   ```rust
   pub enum AddressRole { Primary, Alias }
   pub struct AddressClaim { pub address: Address, pub role: AddressRole }
   ```
2. Replace `Node.address: Address` with `Node.addresses: Vec<AddressClaim>`. Keep invariant: exactly one `Primary` per node.
3. Add convenience accessor on `Node`:
   ```rust
   impl Node {
       pub fn primary_address(&self) -> &Address {
           self.addresses
               .iter()
               .find(|c| c.role == AddressRole::Primary)
               .map(|c| &c.address)
               .expect("Node invariant: exactly one Primary claim")
       }
   }
   ```
4. Add `Graph::find_node_by_address(address: &Address) -> Option<(NodeKey, &Node)>` — walks claims, matches any role. Replaces `get_node_by_url`. (Greenfield: remove `get_node_by_url`, not deprecate.)
5. Update `Graph::add_node` and `Graph::add_node_with_id` signatures to take the initial address (which becomes the `Primary` claim).
6. Snapshot migration: when an `rkyv` snapshot in the old shape loads, convert `address: Address` → `addresses: vec![AddressClaim { address, role: Primary }]`. Pre-1.0 — if the rkyv schema change makes old snapshots unreadable, document the break and let users start fresh.
7. Update internal callers in `mere-kernel` (`graph/query.rs`, `graph/history.rs`, etc.) that read `node.address` directly: switch to `node.primary_address()` if they want the canonical address, or walk `node.addresses` if they want all claims.

**Done when**:

- `AddressRole`, `AddressClaim` exist in `mere-kernel::address`.
- `Node.addresses: Vec<AddressClaim>` replaces `Node.address: Address`.
- `Node::primary_address()` exists and is used by every internal caller that previously read `node.address`.
- `Graph::find_node_by_address(&Address)` exists; `Graph::get_node_by_url` removed.
- `cargo check -p mere-kernel` — green.
- `cargo test -p mere-kernel` — green (tests may need updates for the new field shape; that's expected).

**Risks**:

- `rkyv` snapshot schema change. Pre-1.0, this is fine, but the failure mode on load is "panic on archived field count mismatch" rather than a graceful error. Add a clean error path *or* commit to wiping on-disk session graphs and reseeding. Pick one and document the choice in the commit message.
- Tests inside `mere-kernel/src/graph/tests/` reference `node.address` directly. Targeted test updates expected; do not let them become a separate refactor.

## Phase 2 — mere-host: helper split

**Goal**: address-as-identity dedup behaviour is removed; helpers express intent by name rather than smuggling dedup into a vague "ensure" verb.

**Work**:

1. Remove `host_helpers::ensure_node_for_address_near` and `ensure_node_for_address`.
2. Add two new helpers in `host_helpers`:
   ```rust
   pub fn find_node_by_address(graph: &Graph, address: &Address) -> Option<NodeKey>;
   pub fn create_node_for_address(
       graph: &mut Graph,
       address: Address,
       anchor: Option<NodeKey>,
   ) -> NodeKey;
   ```
   `create_node_for_address` keeps the existing positioning logic (`next_free_position` + anchor-edge creation when `anchor` is set). It does *not* check for an existing node at the address.
3. Update call sites by intent:
   - `bootstrap.rs` (intro-node creation paths) — these want "find or create" semantics for the intro node, since reopening should land on the same node. Express as `find_node_by_address(...).unwrap_or_else(|| create_node_for_address(...))`.
   - `host_navigation.rs::navigate_to` — within-tile navigation does not mint anchors; this path stays untouched. The new-tile path (when it exists) uses `find_node_by_address` first, falls back to `create_node_for_address` for default behaviour. Explicit duplication uses `create_node_for_address` unconditionally (wired in Phase 3).
   - `tearout.rs` — `tear_out_tile_as_branch` and related. `find_node_by_address` for the donor anchor, `create_node_for_address` only when minting new anchors.
   - `orrery_input.rs` — comment at line 779 says "node already exists in the graph, so `ensure_node_for_address_near` is a..." — re-read that path and split appropriately.

**Done when**:

- `find_node_by_address` and `create_node_for_address` exist in `host_helpers`.
- Every former call site of `ensure_node_for_address_near` / `ensure_node_for_address` is updated to express intent explicitly.
- `cargo check -p mere-host` — green.
- Manual smoke: open mere, navigate to a URL twice in default mode, observe one node (find-or-create default still works).

**Risks**:

- One call site may silently want different semantics than the others. Default to "find or create" composition at each updated site unless reading reveals the call site is "always create"-shaped (e.g. minting from a fresh tile-spawn gesture).

## Phase 3 — explicit duplication gesture

**Goal**: the user can deliberately mint a new graph node at an existing address.

**Work**:

1. New bus action: `OpenAddressAsNewNode { address: Address, position: Option<Point2D<f32>>, anchor: Option<NodeKey> }`. Lives alongside the existing `OpenInNewTile` action.
2. Reducer dispatch: action calls `host_helpers::create_node_for_address` unconditionally, creating a fresh graph node even when an existing node has the same address.
3. Omnibar binding: `Ctrl/Cmd-Enter` resolves to `OpenAddressAsNewNode` when the omnibar text matches an address that already exists in the graph; otherwise behaves like `OpenInNewTile`. (Or: always `OpenAddressAsNewNode` from this binding, and the new-vs-find distinction lives in the modifier. Pick the simpler one — probably always `OpenAddressAsNewNode` from `Ctrl/Cmd-Enter`; the existing find-or-create behaviour stays on plain `Enter`.)
4. Palette action: `OpenAddressAsNewNode` discoverable in the command palette by name + alias ("Open as new node," "Spawn duplicate node").
5. Diagnostics: `node.created { reason: "explicit_duplicate", url, source: "omnibar" | "palette" }` — distinguishes from `reason: "new_tile"` and `reason: "manual_add"`.

**Done when**:

- `OpenAddressAsNewNode` exists as a typed bus action.
- `Ctrl/Cmd-Enter` from the omnibar dispatches it; plain `Enter` dispatches existing find-or-create behaviour.
- Palette discoverability works.
- Manual smoke: navigate to a URL, then `Ctrl/Cmd-Enter` the same URL — observe two graph nodes with the same `Primary` claim, different UUIDs.

**Risks**:

- Conflict with existing `Ctrl/Cmd-Enter` semantics if any (the node-per-tile lineage plan reserved `Ctrl/Cmd-Enter` for "open in new tile"; revisit which binding owns which gesture). One of the two ("open in new tile" vs "open as new node") may need a different modifier.

## Phase 4 — `mere_transport::NodeId` → `PeerID`

**Goal**: eliminate the graph-node-vs-peer-node name collision.

**Work**:

1. Rename `mere_transport::NodeId` → `PeerID` (struct + all uses).
2. Update consumers across mere — find all `use mere_transport::NodeId` (or however it's currently imported) and rename.
3. Anywhere a comment or doc refers to the transport's "node id" by name, update to "peer id."

**Done when**:

- `mere_transport::PeerID` exists; `NodeId` does not.
- `cargo check -p mere-transport` and all dependents — green.
- No remaining references to `mere_transport::NodeId` anywhere in the workspace.

**Risks**:

- Low. Mechanical rename in a narrow crate; consumer list is short (mere-transport's downstream is mostly murm/moothold, both early-stage).

## Cross-phase concerns

- **Sequencing**: Phases 1 → 2 → 3 must be in order; Phase 4 is independent and can land first, last, or alongside any other. Bundling 4 with 1 in one commit is acceptable if both feel small.
- **Test logging**: per workspace memory `feedback_test_logging`, don't run tests concurrently; targeted `cargo test -p <crate>` per phase, logs to a gitignored folder if anything's flaky.
- **Targeted checks over broad**: per `feedback_targeted_tests_over_broad_check`, trust `cargo check -p <crate>` rather than full-workspace cargo check noise.
- **Commits**: one per phase. Subject line in the project's existing style (`crates: <imperative>` or similar — see prior commits for exact pattern). Body explains the intent and lists call-site updates touched.

## Deferred to follow-up plans

These are flagged in the brief as out of scope for this implementation pass; each gets its own dated plan when it lands:

- **Drag-merge gesture + confirmation dialog UX** (mere-host + mere-domain). Lineage primitives are ready; this is mostly UI wiring.
- **Passive sibling affordance** (orrery inspector + small visual hint).
- **Split-back gesture** — lineage-view branch-point picker.
- **Sibling-graphlet rendering policy in `forme`** — when same-address nodes should auto-form a graphlet vs. only on explicit user query.
- **Cross-protocol alias auto-suggestion** — passive hint when navigating to a URL whose alternate-protocol form is already in the graph.
- **Per-claim provenance metadata** on `AddressClaim` — defer until a consumer needs it.

## Done conditions (whole plan)

- All four phases landed, each as its own commit.
- DOC_README index entry for this plan + the originating brief.
- Targeted `cargo check -p mere-kernel -p mere-host -p mere-transport` — green at every commit boundary.
- Manual smoke covers the new gesture (`Ctrl/Cmd-Enter` duplicates), the default behaviour (plain `Enter` reuses), and the persistence boundary (relaunch loads the same graph, including duplicates, without merging them).

## Progress

(updated as phases land)
