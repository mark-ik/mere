# Node identity + duplicates — design brief

**Date**: 2026-05-18
**Status**: Design brief. Decides identity policy + duplicate semantics + the `AddressClaim` shape + the role of `node-lineage` as structured address history. Resolves T2-2 of the [graphshell harvest brief](2026-05-17_graphshell_harvest_brief.md) ("Universal Node Content Model") — but the realised scope is much smaller than that framing implied. The kernel infrastructure for UUID identity is already in place; the actual work is mostly host-side navigation policy + a few new gestures + one structural rename.

**Related**:

- [`2026-05-17_graphshell_harvest_brief.md`](2026-05-17_graphshell_harvest_brief.md) §Tier 2 — original T2-2 entry.
- [`2026-05-17_lineage_forme_rename_plan.md`](../implementation_strategy/2026-05-17_lineage_forme_rename_plan.md) — the `node-lineage` rename + layered-lineage framing (url→url within-tile + node→node external).
- [`2026-05-11_node_per_tile_lineage_plan.md`](../implementation_strategy/2026-05-11_node_per_tile_lineage_plan.md) — node-per-tile semantics and the lineage facet.
- [`2026-05-11_relation_taxonomy_and_edge_mutation_plan.md`](../implementation_strategy/2026-05-11_relation_taxonomy_and_edge_mutation_plan.md) — relation families consumed by sibling/merge graphlet projection.
- Current Node struct: [`crates/mere-kernel/src/graph/node.rs`](../../../crates/mere-kernel/src/graph/node.rs).
- Current dedup site: [`crates/mere-host/src/host_helpers.rs`](../../../crates/mere-host/src/host_helpers.rs).

---

## 1. What's already in place

Identity is not the problem. The kernel ships UUID identity today:

- [`Node.id: Uuid`](../../../crates/mere-kernel/src/graph/node.rs#L34) — stable, durable, per-node.
- [`Graph::get_node_by_id(id: Uuid)`](../../../crates/mere-kernel/src/graph/query.rs#L55) — UUID-keyed lookup, siblings with `get_node_by_url`.
- `Graph::add_node(url, position)` — creates a fresh `Uuid` per call. The kernel does not enforce "one node per address."

The address-as-identity behaviour lives at exactly one place: [`host_helpers::ensure_node_for_address_near`](../../../crates/mere-host/src/host_helpers.rs#L32). It checks `get_node_by_url(address)` first and returns the existing node's key if found, only creating a new one when the address is unseen. Every host-side bootstrap, navigation, and tearout path routes through this helper, so today, "navigate to URL X" implicitly collapses to "find or create the node for X." That collapse is the regression we want to remove.

## 2. Motivation

A browser is allowed to have two windows pointing at the same URL, each existing as its own instance. One Wikipedia page can legitimately be a source in an essay cluster, a TODO in a research trail, and a reference inside a moot discussion — three nodes with three lineages, three positions, three contexts. The graph history dimension Mere is reaching for is *richer* than browser history precisely because it lets the same content occupy multiple roles per the user's intent.

So: nodes are identified by UUID. Addresses are properties of a node. Multiple nodes pointing at the same address are allowed by default, surfaced passively, and may be merged or split through explicit gesture.

## 3. The identity stack

Six identity concepts coexist; each owns a distinct role.

| Concept | Crate | Role | Lifetime |
| --- | --- | --- | --- |
| `Node.id: Uuid` (alias: `GraphNodeId`) | `mere-kernel` | Graph-object identity. Stable across save/load. | Until node deleted. |
| `NodeKey` (`= petgraph::NodeIndex`) | `mere-kernel` | Runtime petgraph handle for fast traversal. **Not** durable identity. | One process lifetime. |
| `AddressClaim` (new) | `mere-kernel` | Retrieval/discovery target attached to a node. Has a role (primary/alias). | Per-claim. |
| `node-lineage::Entry` | `node-lineage` | Deduplicated navigation/content encounter. **Stays address/content keyed**, not graph-node keyed. | Per-entry. |
| Eidetic content ID / manifest hash | `eidetic` | Immutable stored content snapshot identity. | Forever, per content. |
| `PeerID` (renamed from `mere_transport::NodeId`) | `mere-transport` | Peer identity. | Per peer key. |

The renames flagged for this pass:

- **`mere_transport::NodeId` → `PeerID`** — eliminates the graph-node vs. peer-node name collision. Peer identity and graph-object identity are unrelated concepts; the name overlap is a landmine.
- `GraphNodeId` is the human-readable name for `Node.id: Uuid` in conversation and prose; the field stays as `id` on `Node` (no code rename needed).

## 4. `AddressClaim` shape

A node carries a `Vec<AddressClaim>` rather than a single `address: Address`. Each claim has a role:

```rust
// illustrative — final shape lives in crates/mere-kernel/src/address.rs

pub enum AddressRole {
    /// The canonical retrieval target. Exactly one Primary per node.
    Primary,
    /// Equivalent addresses pointing at the same content
    /// (mirrors, cross-protocol pairs like https://x + ipfs://x,
    /// shortlinks, user-declared aliases).
    Alias,
}

pub struct AddressClaim {
    pub address: Address,         // existing mere-kernel::address::Address
    pub role: AddressRole,
    // future: provenance (user-asserted, server-redirected,
    //   federation-imported), claim-time timestamp, verification
    //   status — added when consumers materialise.
}
```

On `Node`:

```rust
pub struct Node {
    pub id: Uuid,
    pub addresses: Vec<AddressClaim>,
    // ... existing fields
}
```

**Invariant**: exactly one `AddressRole::Primary` per node. Aliases are zero-or-more.

**Why a property, not a separate graph object**: addresses are how the system *finds* the node, not entities the user reasons about graph-topologically. Aliasing is a node-local concern (this node has these names). Cross-node equivalence is a separate semantic relation (the `Imported` family already covers "this node was sourced from that node in a peer's graph") and shouldn't be conflated with intra-node alias claims.

**Migration**: greenfield. `Node.address: Address` becomes `Node.addresses: Vec<AddressClaim>`, with existing data migrated as a single Primary claim at load time. Pre-1.0; no compat shim needed beyond the load-time conversion. If on-disk session formats break across this, that's acceptable.

## 5. Duplicate policy

- **No automatic dedup by address.** The kernel never collapses two nodes silently.
- **Explicit "open as new node" gesture.** Default behaviour for "navigate to URL X" stays *find existing or create*; the new gesture is a deliberate user choice to create a sibling. Triggers: `Ctrl/Cmd-Enter` in the omnibar (sibling of the existing "open in new tile"; this one explicitly mints a new graph node even if one already exists for the address), and a palette action `OpenAddressAsNewNode`.
- **Default in-tile navigation stays lineage-based.** Within-tile URL changes extend a tile's lineage (node-lineage::Entry chain); they do not mint anchor nodes regardless of dedup policy. This is unchanged from the [node-per-tile lineage plan](../implementation_strategy/2026-05-11_node_per_tile_lineage_plan.md).
- **Passive sibling affordance.** When a node has same-address siblings (other graph nodes whose Primary or Alias claims match), the orrery shows a small affordance — count badge or inspector list — so the user sees the relationship without it being intrusive. Not a separate visual treatment of the node itself; just a "you have N other nodes pointing here" hint accessible on focus / hover.
- **Merge and split are explicit gestures** (§7).

### Replace `ensure_node_for_address_near` with two intent-clear functions

```rust
// illustrative — final shapes in crates/mere-host/src/host_helpers.rs
//   (or wherever they end up)

/// Pure lookup. No side effects. Returns the first node whose
/// AddressClaims (any role) match the address. Caller decides what to
/// do with `None`.
pub fn find_node_by_address(graph: &Graph, address: &Address) -> Option<NodeKey>;

/// Always creates a fresh node with a new UUID. Caller knows it wants
/// a new node — used for explicit "open as new node" and for the
/// existing helpers that should mint anchors unconditionally.
pub fn create_node_for_address(
    graph: &mut Graph,
    address: Address,
    position: Point2D<f32>,
    anchor: Option<NodeKey>,
) -> NodeKey;
```

Callers pick by intent. The current "navigate to URL X — find or create" behaviour is expressed as a small composition (`find_node_by_address(...).unwrap_or_else(|| create_node_for_address(...))`) at the call sites that want it — and the call sites that *should* always create (explicit duplication, certain bootstrap paths) call `create_node_for_address` directly.

`ensure_node_for_address_near` is removed (or repurposed as a thin convenience that documents which composition it implements). Per the lineage/forme rename plan style, downstream API names that still reference the old pattern get cleaned up in the same pass.

## 6. Lineage as structured address history

`node-lineage`'s branchable visit tree is the structured address history of a node. The "url → url" granularity from the [lineage rename plan](../implementation_strategy/2026-05-17_lineage_forme_rename_plan.md) is exactly this: within a tile, the Owner cursor moves through Entry visits, with branches when the user backs-and-forwards-differently. The active address of a node is the URL at the current cursor position; aliases (declared) and prior addresses (navigated through) coexist:

- **Aliases** — declared via `AddressClaim::Alias`. Static. Doesn't move when the user navigates.
- **Address history (visits)** — `node-lineage::Entry` chain. Dynamic. Moves as the user navigates.
- **Active address** — the address at the Owner cursor's current visit position. Always one of the Primary claim or a recently-visited entry; the kernel records which one for rendering.

Promoting a within-tile branch into its own anchor (the "open as new node from this branch point" gesture) mints a new graph node whose lineage is the post-branch tail of the parent's tree, sharing the pre-branch prefix. This is the node→node external lineage layer.

`node-lineage::Entry` stays address/content-keyed — *not* graph-node keyed. Two graph nodes pointing at the same address share Entry records; their Owner cursors are independent.

## 7. Merge and split mechanics

Both are operations on the *Owner cursors* in `node-lineage` plus graph-object cleanup in `mere-kernel`. `node-lineage` is already branchable per the history-tree adaptation, so the underlying tree machinery is in place.

### Drag-merge

1. User holds a graph node (the "source") and drags it onto another node (the "target").
2. Target's presentation changes to indicate "merge target" — affordance similar to drop-zone highlight in standard UI.
3. On release, a confirmation dialog opens listing what will be merged and what conflicts exist:
   - **Lineage**: source's Owner-cursor branches absorb into target's visit tree as additional branches under the target's Owner. Both sets of visits are preserved.
   - **Annotations**: tags, classifications, title, position. Where source and target differ, the dialog presents either-or choice; where they agree, no choice needed.
   - **Edges**: edges incident to source are reattached to target. If both nodes were endpoints of the same edge, the edge collapses (one-side dedup, since now both endpoints are the same node).
   - **Aliases**: source's `AddressClaim::Alias`es are added to target's claims. Source's `Primary` claim becomes an Alias on target (or stays Primary if target's Primary was the same address).
   - **AddressClaim list**: ends with target having a union of both nodes' claims, exactly one Primary, the rest Aliases.
4. On confirm: target absorbs source. Source's UUID is retired (logged in lineage for split-back). Source's `NodeKey` is invalidated.
5. On cancel: nothing changes.

### Split-back

1. User opens the merged node's lineage view (a forme projection or dedicated inspector — UX TBD).
2. User picks a branch point in the visit tree.
3. "Split into separate node" gesture (palette action or context menu).
4. A new graph node is minted with a fresh UUID. The lineage tail from the branch point onward moves to the new node. The pre-branch prefix is shared with the original (read-only on the new node's side; both Owner cursors reference the same prefix entries).
5. Edges that were reattached during the original merge can be partially re-split if the user knows which side they belonged to — but this is a "best effort" reconstruction. The merge dialog should optionally record edge provenance to make this cleaner.

**Open**: how thoroughly to preserve "this came from a prior merge" provenance. Simplest is a single `MergeRecord` in `node-lineage` per merge event; richer is per-edge provenance carried through. Defer to consumer needs.

## 8. Sibling graphlets

When the user wants to see "all nodes pointing at example.com" or "all nodes with related contexts," render them as a graphlet (per `forme::GraphletRef`). Graphlets in `forme` are not required to be address-siblings — they're bounded subsets the projection picks up for any reason. Address-sibling rendering is one valid policy for *forming* a graphlet, alongside lineage-share, edge-connectivity, semantic-tag, etc.

This means the kernel does not need a new "sibling group" concept. The forme can project sibling-graphlets on demand when the user asks "show me everything pointing at this URL" by computing the set at render time from `AddressClaim` membership.

## 9. Code-change scope

### mere-kernel (in this pass)

- New types: `AddressRole`, `AddressClaim` in `mere-kernel::address`.
- `Node.address: Address` replaced with `Node.addresses: Vec<AddressClaim>` (invariant: exactly one Primary).
- `Graph::find_node_by_address(address) -> Option<(NodeKey, &Node)>` — replaces or wraps existing `get_node_by_url`. Internally walks `addresses` rather than checking a single field.
- `Graph::get_node_by_url(url)` stays as a thin convenience or is removed (greenfield — probably remove).
- Migration code at load time converts `address: Address` into `addresses: vec![AddressClaim { address, role: Primary }]` for any pre-existing snapshots that get touched during the change. Pre-1.0; lossy migration is acceptable.

### mere-host (in this pass)

- Replace `ensure_node_for_address_near` with the split: `find_node_by_address` + `create_node_for_address`. Update bootstrap, navigation, and tearout call sites to compose appropriately.
- Add `OpenAddressAsNewNode` bus action + omnibar `Ctrl/Cmd-Enter` binding for explicit duplication.

### mere-transport (in this pass)

- Rename `NodeId` → `PeerID`. Touch consumers.

### Deferred to follow-up passes

- **Drag-merge gesture + confirmation dialog UX**. Lineage primitives are ready; the gesture wiring + dialog are UI work. Land as a single follow-up plan once the host's drag system has the precision it needs.
- **Passive sibling affordance**. Defer until the orrery's hover/focus inspector has a place for it.
- **Split-back gesture**. Land alongside merge; defer the same way.
- **forme**: nothing required immediately. Future sibling-graphlet rendering picks up `AddressClaim` membership when consumers ask for it.
- **cartography**: nothing required immediately. May want a sibling-overlay strategy later.
- **node-lineage**: no kernel changes. The branchable visit-tree already supports merge-by-absorbing-branches and split-by-peeling-tail.

## 10. Done conditions

- `AddressRole` + `AddressClaim` exist in `mere-kernel::address`.
- `Node.addresses: Vec<AddressClaim>` replaces `Node.address: Address`. Invariant: exactly one `Primary` per node.
- `Graph::find_node_by_address` exists and is the canonical address lookup; the old `get_node_by_url` is removed or kept only as a thin convenience over `find_node_by_address` + Primary-role filter.
- `ensure_node_for_address_near` either removed or shrunk to a documented "find or create" composition; bootstrap / navigation / tearout call sites updated to pick `find_node_by_address` vs. `create_node_for_address` by intent.
- `OpenAddressAsNewNode` bus action wired into omnibar (`Ctrl/Cmd-Enter` binding) and palette.
- `mere_transport::NodeId` renamed to `PeerID`; all consumers updated.
- Targeted `cargo check -p mere-kernel -p mere-host -p mere-transport` — green.
- Manual smoke: open mere, navigate to a URL twice via different gestures, confirm two graph nodes exist with the same Primary claim.

## 11. Non-goals

- No drag-merge or split-back UI in this pass — primitives only.
- No auto-equivalence across protocols (https vs. ipfs vs. gemini for the same content) — that's a user-declared alias relation, not a kernel-inferred equivalence.
- No cross-graph identity reconciliation. When a federated peer's graph contains "the same" content, the `Imported` relation family handles it; this brief does not propose a federation-level identity model.
- No removal of `Node.address` field as a convenience accessor for "primary address" — if downstream code reads `node.address` heavily, a `Node::primary_address(&self) -> &Address` method preserves ergonomics without imposing single-address identity.

## 12. Risks

- **Bootstrap paths that currently rely on dedup** — anything that calls `ensure_node_for_address` expecting to *find* the intro node on second run, not create a duplicate. These all want `find_node_by_address`, not `create_node_for_address`. Easy to fix at the call site; flagged for explicit review during the change.
- **Snapshot serialization** — `rkyv` derive on `Node` changes shape. Old snapshots become unreadable. Pre-1.0; acceptable.
- **`Node::primary_address` ergonomics** — a lot of code reads `node.address` directly. Adding the accessor + replacing all such reads in a single pass is the smallest blast radius; missing one site causes a compile error (good) rather than silent breakage (also good — there's no ambiguity).
- **AddressClaim list reordering during merge** — if a node has three aliases and gets merged with a node that has two of those three as Primary/Alias, dedup within the resulting `Vec<AddressClaim>` needs to happen. The merge logic owns this; document the dedup rule (by address-equality, role-preferred-Primary).

## 13. Open questions

- **Conflict-resolution dialog shape on merge**: whose title wins by default? Tags union or pick-one-side? Position blend or snap to one side? Worth a separate UX brief once the gesture is wired.
- **Sibling affordance specifics**: small count badge on the node? Inspector list? Sidebar surface? Both? Defer to the orrery's hover/focus inspector pass.
- **Lineage promotion-to-anchor gesture details** — this is mentioned in [`node-per-tile lineage plan`](../implementation_strategy/2026-05-11_node_per_tile_lineage_plan.md) as "future; not blocking." It interacts with this brief's "open address as new node" gesture (both mint new graph nodes from existing context) but stays separately scoped.
- **Cross-protocol alias auto-suggestion**: when the user navigates to `https://x.com/page` and the graph contains a node with `gemini://x.com/page`, should the system *suggest* an alias relationship? Probably yes for UX richness, but as a passive hint, not an automatic claim. Defer.
- **Per-claim provenance**: `AddressClaim` may eventually need provenance metadata (user-asserted, server-redirected, federation-imported, verified via certificate, etc.). v0 leaves the struct intentionally lean; provenance fields land when a consumer needs them.
