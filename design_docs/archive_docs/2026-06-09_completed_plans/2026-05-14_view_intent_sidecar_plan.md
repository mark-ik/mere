# ViewIntent sidecar — implementation plan

**Date**: 2026-05-14
**Status**: Implementation plan — v0a primitive landed; v0b host wiring pending
**Scope**: Make per-pane view-local state durable across restarts. Move `OrreryPaneState.hidden_relations` (currently in-memory, lost on app exit) into a per-pane JSON sidecar keyed `(session_id, frame_id, pane_id) → ViewIntent`. Land the persistence primitive with tests now; defer host integration until session/pane bootstrap order is settled.

**Related**:

- [`../research/2026-05-11_browser_multiplexer_framing.md`](../research/2026-05-11_browser_multiplexer_framing.md) §5.3 — the framing brief that defines `ViewIntent` and the per-pane sidecar location.
- [`2026-05-11_relation_taxonomy_and_edge_mutation_plan.md`](2026-05-11_relation_taxonomy_and_edge_mutation_plan.md) §6.2 — names the migration target for `OrreryPaneState.hidden_relations`.
- [`crates/mere-host-runtime/src/session_graph_store.rs`](../../../crates/mere-host-runtime/src/session_graph_store.rs) — sibling persistence primitive whose pattern this borrows (atomic tmp+rename, `Ok(None)` on missing file, `serde_json` pretty-print).

---

## 1. Goal + done conditions

**Goal:** `ViewIntent` is the persistable bundle of per-pane view-local state. v0 carries `hidden_relations` (UUID-keyed `(from, to, RelationKind)` tuples). Subsequent slices add fields as their producers wire up (form_factor, scale, focus, filter, strategy, overlays per the framing brief §5.3).

**v0a done when (this turn):**

- `view_intent_store` module in `mere-host-runtime` defines `ViewIntent`, `view_intent_path`, `save_view_intent`, `load_view_intent`, `view_intent_exists`.
- File layout: `<sessions_dir>/<session_id>/views/<frame_id>/<pane_id>.json`. Atomic writes via `.tmp` + rename.
- Round-trip tests + missing-file + malformed-JSON tests pass.
- No host wiring yet — the primitive is callable but inert.

**v0b done when (follow-up):**

- `HideRelation` execute calls `save_view_intent` after toggling the in-memory set.
- Bootstrap path: when an orrery pane is instantiated for the first time in a session (via `reconcile_panes` or fresh attach), call `load_view_intent` and populate `OrreryPaneState.hidden_relations`.
- A toggle made in one app run survives a restart of the same session.
- Test coverage: an integration-style test in `mere-host` that toggles a relation, "restarts" (drops + reloads the orrery state), and observes the relation is still hidden.

## 2. v0 ViewIntent shape

```rust
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ViewIntent {
    /// Per-orrery view-local hide set. Keyed by stable node UUIDs
    /// (not petgraph `NodeKey`, which isn't stable across save/load)
    /// + the `RelationKind::tag()` ordinal — so a Hyperlink line and a
    /// Cites line on the same node pair persist independently. The
    /// tag round-trip via [`mere_kernel::graph::RelationKind::from_tag`]
    /// rebuilds the kind on load.
    pub hidden_relations: BTreeSet<HiddenRelationRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub struct HiddenRelationRecord {
    pub from_id: Uuid,
    pub to_id: Uuid,
    pub relation_tag: u32,
}
```

Choosing `BTreeSet` over `HashSet` for stable JSON output (helps diffing manifests in dev).

Future fields (per framing brief §5.3) land as separate slices, each gated on a real consumer landing the field's producer:

- `form_factor: FormFactor` — once cartography drives the orrery render.
- `scale, focus, overlays` — once the cartography request pipeline replaces direct `mere-host-orrery::render`.
- `filter, strategy` — once user-facing strategy picker + filter UI ship.
- `binding_mode: ProjectionBindingMode` — once cartography ships the [linked/unlinked split](../research/2026-05-10_cartography_layer_brief.md#4-contract-surface) (per [graphshell harvest brief](../research/2026-05-17_graphshell_harvest_brief.md) Tier 1 / T1-5). The default for a live pane's view intent is `Linked` (re-render on graph mutation); for captured / snapshot view intents (switcher thumbnails, exports), it's `Unlinked`. Today every pane is implicitly linked — making the field explicit catches subtle bugs in scenarios like a switcher thumbnail re-rendering after the captured graph mutates.

Bundling them now risks the "half-finished implementation" the user's memory warns against. Each field arrives when its producer does.

## 3. File path layout

```text
<sessions_dir>/
└── <session_id>/
    ├── manifest.json            ← ManifestStore
    ├── graph.json               ← session_graph_store
    └── views/
        └── <frame_id>/
            └── <pane_id>.json   ← view_intent_store  (NEW)
```

`session_id` is a `Uuid` string (matches `ManifestStore`'s session-dir naming). `frame_id` is the `FrameId` newtype's underlying `Uuid` string. `pane_id` is the `PaneId(u64)` rendered as decimal.

Empty `ViewIntent` (no hidden relations and no other fields set) **is not persisted** — the loader returns `Ok(None)` for missing files, which means "use default."

## 4. Host integration (v0b — deferred)

Two open questions for the host wiring:

**Q1: How does an orrery pane resolve its `session_id`?**

`HostRoot.frame_layout` doesn't carry a session_id directly. Each leaf has a `graph_id`. The `manifests: Entity<ManifestStore>` knows the mapping `session_id → GraphSessionManifest` and the manifest references `primary_graph_id`. So `session_id = manifests.session_for_graph(graph_id)` is the resolver — but that helper doesn't exist yet. Adding it is part of v0b.

A simpler interim: key sidecars by `(graph_id, frame_id, pane_id)` directly. Loses the "two panes onto the same graph in different sessions" disambiguation, but no session yet uses that case. Filed as an open question for v0b.

**Q2: When does load happen?**

`reconcile_panes` lazily instantiates per-pane state via `PaneState::for_content(content)`. v0b extends this to look up the file at the resolved path and populate `OrreryPaneState.hidden_relations` if it exists. The `for_content` signature changes to take the session/frame/pane identity plus a borrowed store handle, or `reconcile_panes` does the load post-construction. Probably the latter to keep `for_content` callable from tests with no I/O.

**Q3: When does save happen?**

Synchronously inside the `HideRelation` execute arm, after toggling the in-memory set. The write is a small JSON file; debouncing isn't necessary for v0 (only one entry mutates per gesture). If hide gestures become bursty (e.g. "hide all relations on this pair"), introduce dirty-flag + flush-on-idle. For v0 the synchronous write is fine.

## 5. Test plan

**v0a (primitive — this turn):**

- `save_view_intent` then `load_view_intent` returns the same set.
- `load_view_intent` on a missing file returns `Ok(None)`.
- `save_view_intent` overwrites an existing file atomically (no `.tmp` left over).
- Malformed JSON returns `Err(InvalidData)`.
- `view_intent_exists` reflects save state.
- Empty `ViewIntent { hidden_relations: empty }` round-trips identically.
- Relation tags round-trip through the on-disk JSON via `RelationKind::from_tag` — a hidden Cites edge stays a Cites edge after load.

**v0b (host integration — follow-up):**

- Toggling `HideRelation` for a pair writes the sidecar at the expected path.
- Re-creating the orrery pane from a fresh `OrreryPaneState::default()` and feeding the loaded ViewIntent reconstructs the in-memory set.
- The bootstrap path produces the correct hide set when the sidecar exists, and an empty set when it doesn't.

## 6. Assumptions

- v0a's `(session_id, frame_id, pane_id)` keying matches the framing brief; the path string composition uses lowercase UUID-with-hyphens and decimal pane-id.
- JSON over rkyv: per the session_graph_store comment header, JSON is hand-inspectable and easy to evolve. ViewIntent is dev-tier state with tiny payloads; binary doesn't pay yet.
- `BTreeSet` over `HashSet` for deterministic JSON output.
- `RelationKind` tag stability is contractual: the [relation taxonomy plan §6.3 commit](2026-05-11_relation_taxonomy_and_edge_mutation_plan.md) freezes the ordinal scheme so on-disk tags decode unambiguously on load. Adding a `RelationKind` variant must extend the tag scheme additively (new ordinals at the end of their family) — covered by the existing kernel `tag_round_trips_for_every_relation_kind` test.

## 7. Open questions

1. **Session-vs-graph keying** (§4 Q1). Lean toward session-id keying once `manifests.session_for_graph(graph_id)` lands; until then v0a stores by stable string keys that the host computes.
2. **Eviction of stale sidecars.** When a pane is removed from the frame layout permanently, its sidecar lingers. Cheap: leave it, costs ~1KB per orphan. Tidy: add a sweep at session close. Defer the tidy pass.
3. **Schema versioning.** No version field in v0; pre-publication, breaking changes wipe local state per the relation-taxonomy plan §11 precedent. When ViewIntent leaves dev, add a `schema_version: u16` with explicit migration support.
