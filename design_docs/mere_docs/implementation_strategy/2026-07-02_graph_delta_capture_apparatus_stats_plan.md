# Graph Delta Capture + Apparatus Table Stats Plan

**Status:** in progress.
**Date:** 2026-07-02.
**Scope:** the mere half of the doctrine-shortfalls work from the
[data-oriented doctrine brief](../../2026-07-02_data_oriented_doctrine_brief.md)
(§6 items 2-3): make the orrery's `GraphDelta` stream recordable and replayable,
wire the first producer for the existing `Replay*` variants, and give the
apparatus panel real per-table instrumentation. The genet half (DomMutation
capture, engine arena stats) is `genet:docs/2026-07-02_dom_mutation_capture_replay_plan.md`.

**Out of scope:** log-authoritative persistence (rides the p2panda event-DAG
substrate work; today's snapshot-authoritative kernel persistence stands),
uxtree incremental projection, and frame `PaneNode` arena-ization (documented
deviations in the brief; revisit when scale demands).

---

## Findings (seams verified 2026-07-02)

- **Single write entry point exists:** `apply_graph_delta(graph, delta)`
  ([apply.rs:198](../../../crates/graph/graph-kernel/src/graph/apply.rs)). The
  capture hook has one home.
- **`GraphDelta` is not serializable** (`Debug, Clone` only) and its `Replay*`
  variants (`ReplayAddNodeWithIdIfMissing`, `ReplayAssertRelationByIds`,
  `ReplayRemoveNodeById`, `ReplayRetractRelationsByIds`) had **no producer
  anywhere in the workspace** — the replay lane was API-only. Payload types are
  largely rkyv-ready already
  (`EdgeAssertion` derives Archive/Serialize/Deserialize), but `GraphDelta`
  carries foreign types (`euclid::Point2D`), so the capture format should be a
  mirror enum in the existing `Persisted*` style rather than derives on
  `GraphDelta` itself.
- **The input form is not the replayable form.** `AddNode { id: Option<Uuid> }`
  mints a Uuid (and timestamps) at apply time. A faithful log must record the
  *resolved* outcome (the minted id), i.e. the recorder captures each delta in
  its `Replay*` form after `apply_graph_delta` returns, using the
  `GraphDeltaResult`. This is exactly what the `Replay*` variants were shaped
  for; the recorder is their missing producer.
- **Timestamps are the main equality hazard.** Nodes carry creation/visit
  times (`SystemTime`); replay-onto-empty cannot reproduce them. The oracle
  comparison (Phase C) needs either recorded timestamps in the log or a
  timestamp-insensitive snapshot comparison. Decide in Phase C; recording them
  is the doctrinally cleaner answer (the log is the record).

## Plan

### Phase A — the captured-delta mirror

- `CapturedDelta` enum beside the `Persisted*` family: plain fields
  (f32 pairs for positions, string/uuid ids), rkyv + serde derives, one variant
  per resolved `GraphDelta` outcome. `From` conversions in both directions:
  resolved apply outcome → `CapturedDelta`, and `CapturedDelta` → the matching
  `Replay*` `GraphDelta`.
- Done: round-trip unit test (resolved delta → captured → replay variant) plus
  postcard byte round-trip.

### Phase B — the recorder

- A `DeltaLog` writer owned by the host session (not by `Graph`): after each
  successful `apply_graph_delta`, append the `CapturedDelta`. Env-gated
  (`MERE_GRAPH_DELTA_LOG=<dir>`), postcard-framed, one file per session, in a
  gitignored logs location. Zero cost when unset.
- Done: a browsing session with the env var set yields a log whose entry count
  matches the session's applied deltas.

### Phase C — replay + oracle test

- `replay_delta_log(log) -> Graph`: fold `CapturedDelta`s into `Replay*`
  deltas over an empty graph via `apply_graph_delta`. The current landed scope
  now covers the sanctioned `GraphDelta` durable write lane end to end:
  structural mutations, traversal/media/navigation chronology, the light
  node-content and enrichment setters, classification/tag presentation
  maintenance, field/coupling lifecycle and membership writes, import-record
  truth, session fidelity (`form_draft`, `session_scroll`), and deterministic
  frame-layout/history state. `last_visited` now also replays through a
  resolved timestamp form and round-trips through `GraphSnapshot` session
  state, so the replay oracle is back to comparing canonicalized whole
  snapshots for that field too.
- Oracle test (the brief's §6 item 4 pattern): record a scripted session,
  replay the log, compare `GraphSnapshot`s. Resolve the timestamp question
  here. The current landed oracle normalizes the snapshot write-time
  `timestamp_secs` plus order-sensitive navigation bindings/owner visit lists
  before comparing full snapshots for the covered replay lane.
- Done condition: replay of a recorded session reproduces a snapshot-equal
  graph in a headless test, and a canned log replays green in-tree as a
  regression test.
- This also derisks murm sync: the sync lane will speak the same resolved
  `Replay*` vocabulary this phase exercises.

### Phase D — apparatus table stats

- A small stats convention (a struct, not a framework): per table, kind label,
  row count, estimated bytes, deltas this session, last dirty-set size. This
  belongs to Meerkat's apparatus surface now that apparatus has been extracted
  from `platen`; the cartography crate should stay out of diagnostics-surface
  ownership.
- Sources, in wiring order:
  1. kernel: node count, edge count by family, history owner/entry counts,
     delta-log length (from Phase B when active);
  2. genet engine arenas via the observables seam (DOM nodes/bytes, last
     restyle batch). The genet-side producer is landed; mere still needs to
     surface it per focused document/lane;
  3. Scene: per-frame op count and encoded size (numbers the transfer path
     already computes). The mere-side surfacing is now landed through the
     existing `ContentUpdate::Scene` path.
- Surface as an apparatus panel section rendering live values (real numbers,
  no placeholders, per the real-feedback rule). The target sentence: "this
  graph: N nodes / M edges, session log K deltas; this document: N nodes,
  ~M KiB, last batch dirtied K elements."
- Done: apparatus shows live table stats for kernel + one engine document; the
  panel goes empty-state (not fake) for sources that are not wired.

## Progress

- 2026-07-02: plan written; seams verified (`apply_graph_delta` single entry,
  unproduced `Replay*` variants, rkyv-ready payloads, foreign-type and
  timestamp hazards named).
- 2026-07-02: structural slice landed. Added kernel `CapturedDelta` for the
  existing stable-id replay lane (add/remove node, assert/retract relation), an
  `apply_graph_delta` capture hook, an env-gated postcard log writer
  (`MERE_GRAPH_DELTA_LOG=<dir>`), and live apparatus table rows for kernel
  tables plus session-log counts.
- 2026-07-02: traversal slice landed. Added stable-id replay/capture for
  `AppendTraversal`, with the recorder capturing the resolved timestamp from the
  edge payload so postcard logs replay the actual traversal event rather than a
  second `now`.
- 2026-07-02: light node-content slice landed. Added stable-id replay/capture
  for the lighter node setters (`SetNodeTitle`, `SetNodeUrl`,
  `SetNodeMimeHint`, `SetNodeViewerOverride`, `SetNodePinned`,
  `SetNodeCompatMode`, `InsertNodeTag`, `RemoveNodeTag`, `SetNodeBody`) and
  extended the log replay test to assert those fields survive round-trip.
- 2026-07-02: node-enrichment slice landed. Added stable-id replay/capture for
  `AppendNodeProperty`, `AddNodeClassification`, and `RecordNodeDerivation`,
  and extended replay tests to prove those enrichment records survive the log
  round-trip.
- 2026-07-02: frame/history state slice landed. Added stable-id replay/capture
  for `AppendFrameLayoutHint`, `RemoveFrameLayoutHint`, `MoveFrameLayoutHint`,
  `SetFrameSplitOfferSuppressed`, and `UpdateNodeHistory`, with replay tests
  asserting hint order, split-offer suppression, and clamped history cursor
  state round-trip correctly.
- 2026-07-02: media payload slice landed. Added stable-id replay/capture for
  `SetNodeThumbnail` and `SetNodeFavicon`, and extended replay tests to prove
  thumbnail PNG bytes plus favicon RGBA bytes and dimensions survive the log
  round-trip.
- 2026-07-02: navigation chronology slice landed. Added stable-id
  replay/capture for `NavigateNode`, `BranchHistory`, `NodeHistoryBack`, and
  `NodeHistoryForward`, carrying the resolved history timestamps plus
  `last_session_visited` so replay reproduces the shared navigation snapshot
  rather than only the visible current URL.
- 2026-07-02: field/coupling slice landed. Added stable-id replay/capture for
  `AddField`, `RetireField`, `AddCoupling`, and
  `SetFieldCouplingStrength`, reusing the existing `PersistedField` /
  `PersistedCoupling` DTO shape so replay preserves field definitions,
  selector/response payloads, lifecycle, and coupling strength.
- 2026-07-02: snapshot-oracle compare landed for the current replayable lane.
  The `graph_delta_log` replay test now compares canonicalized whole
  `GraphSnapshot`s rather than only spot-checking fields plus the navigation
  snapshot, normalizing the top-level snapshot clock and navigation ordering
  noise while the remaining uncovered write variants stay out of scope.
- 2026-07-02: semantic-predicate edge slice landed. Added stable-id
  replay/capture for `SetEdgeSemanticPredicate` and `AssertSemanticPredicate`,
  using stable endpoint ids for both the open-predicate edge assertion lane and
  predicate-set/clear on an existing edge payload; the kernel capture oracle
  and the `meerkat` graph-delta log replay test now both assert those
  predicate IRIs survive round-trip.
- 2026-07-03: import-record slice landed. Added a resolved
  `ReplaySetImportRecords` capture/replay form plus live delta coverage for
  `SetImportRecords`, `DeleteImportRecord`,
  `SetImportRecordMembershipSuppressed`, and `SetNodeImportProvenance`, so the
  recorder now persists the normalized post-mutation import-record table,
  including actual `imported_at_secs` values from provenance rebuilds instead
  of replaying a second `now`.
- 2026-07-03: classification/tag-presentation admin slice landed. Added
  stable-id replay/capture for `RemoveNodeClassification`,
  `SetNodeClassificationStatus`, `SetNodePrimaryClassification`, and
  `SetNodeTagIconOverride`, with kernel and `meerkat` replay tests asserting
  classification status/primary changes, classification removal, and tag-icon
  overrides survive the log round-trip.
- 2026-07-03: field admin slice landed. Added stable-id replay/capture for
  `ActivateField` and `RetractCoupling`, extending the existing field/coupling
  replay lane from add/retire/strength writes to the remaining lifecycle and
  membership operations.
- 2026-07-03: session-fidelity slice landed. Added stable-id replay/capture for
  `SetNodeFormDraft` and `SetNodeSessionScroll`, with kernel and `meerkat`
  replay tests asserting those per-node session restore fields survive the log
  round-trip.
- 2026-07-03: last-visited slice landed. Added a resolved timestamp replay form
  for `TouchNodeLastVisited`, so the recorder replays the actual observed
  `last_visited` instant instead of minting a second `now`.
- 2026-07-03: snapshot closure landed for `last_visited`. Added
  `last_visited_ms` to persisted per-node session state, wired the
  `Graph::to_snapshot` / `from_snapshot` round-trip, restored the dedicated
  snapshot test, and returned the `meerkat` log replay oracle to full
  canonicalized `GraphSnapshot` comparison for that field.
- 2026-07-02: apparatus engine-document stats slice landed. Content actors now
  ship focused-document Genet observables (`DomArenaStats` plus optional
  `LayoutBatchStats`) through the host update stream; the constellation caches
  them per activation; and apparatus now replaces the old "Document tables: not
  wired yet" placeholder with real focused-document DOM/layout rows when the
  current lane reports them, or an explicit lane/unavailable message when it
  does not.
- 2026-07-02: apparatus scene stats slice landed. `ContentUpdate::Scene` now
  carries per-frame scene stats (`ops.len()` plus stripped-postcard byte size),
  the constellation caches them beside the latest scene, and apparatus renders
  focused-document scene op-count / encoded-size rows or an explicit
  unavailable/awaiting message for lanes that do not currently have a scene.
- 2026-07-03: apparatus stats-model slice landed. Replaced the old ad hoc
  `(label, value)` table rows with a Meerkat-side typed apparatus stat model
  (kind label, count, optional bytes, optional session deltas, optional last
  dirty-set size, empty-state text), keeping the ownership boundary out of
  `platen` while rendering richer kernel / engine / scene diagnostics from the
  same apparatus pane.
