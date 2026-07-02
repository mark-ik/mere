# Graph write-path migration (finish Phase 6.5)

**Date**: 2026-07-01
**Status**: In progress. Finishes the single-write-path boundary that `graph/mod.rs:328` declares but
does not enforce ("graph topology mutators are crate-internal... other runtime/shell code paths should
route through reducer intents"). Prerequisite hardening for
[event_log_timeline_plan](2026-07-01_event_log_timeline_plan.md): once `apply_graph_delta` is the real
funnel, slice E's recording hook instruments one function instead of ~45 mutator bodies.

## Audit findings (2026-07-01, code-verified)

- The kernel exposes ~45 `pub` mutating methods across `graph/{mod,edge_ops,edge_payload,node_props,
  field_ops,import_records,cross_graph,query,history}.rs`, plus two `&mut` escape hatches
  (`get_node_mut`, `get_edge_mut`). Nothing is `pub(crate)`; the Phase 6.5 comment is aspirational.
- **~44 production call sites** bypass `apply_graph_delta` (which has exactly 2 real callers, both in
  kernel-internal `facet_projection.rs`): orrery `input/selection/nodes/build/fields/lifecycle` (~26),
  meerkat `web_clip` + `command_drain` (body writes via `get_node_mut`), linked-data `ingest/apply`
  (title/tags/properties/classification/derivation/predicate via escape hatches), inker `statements`
  (predicate via `get_edge_mut`), aether `commit_to_graph`, platen `project_orrery_subgraph` (a
  *scratch* kernel graph — legitimate; deltas apply to any `&mut Graph`).
- **~120 test-fixture call sites** (orrery/tests, meerkat/graphlets_tests, platen, intel/signals,
  forme/topology, gyre, session-runtime test modules) — routing fixtures through deltas is churn with
  zero enforcement value; they get a feature-gated escape hatch instead.

## The four writer classes (the classification this migration enforces)

1. **Primitive durable mutations** → route through `GraphDelta`/`apply_graph_delta`, mutators become
   `pub(crate)`. Everything that changes persisted graph truth: topology, relations, navigation
   history, tags, titles, body, mime, pins, thumbnails/favicons, classifications, derivations,
   properties, predicates, fields/couplings, frame hints, import records.
2. **Compound kernel operations stay `pub`**: `from_snapshot`, `cross_graph::{copy_node_from*,
   copy_component_from}` — kernel-authored multi-step operations taking non-delta-able inputs
   (`&Graph` donors). They are the "trusted writers" the Phase 6.5 comment names; slice E records them
   as compound events or decomposes internally.
3. **Transient/runtime state stays `pub`, documented exempt**: `set_node_position` /
   `set_node_projected_position` (physics/view — "positions are no longer graph truth", the Position
   gut), `set_current_session` (per-launch host wiring, B5), `set_node_lifecycle` (webview runtime
   state). Not truth, not logged, not delta-routed.
4. **Test fixtures** → a `fixtures` cargo feature on kernel exposing a `GraphFixtures` extension trait
   that delegates to the `pub(crate)` mutators. Enabled only via `dev-dependencies`; call sites stay
   unchanged, each test module adds one `use`.

## Net-new kernel surface

- Methods: `set_node_body(key, Option<String>) -> bool` (today raw via `get_node_mut`),
  `append_node_property(key, NodeProperty) -> bool` (dedup-append, today raw in ingest),
  `set_edge_semantic_predicate(EdgeKey, Option<String>) -> bool` (today via `get_edge_mut`).
- `GraphDelta` variants (production-driven only; unused mutators go `pub(crate)` with no variant until
  a consumer appears): `NavigateNode`, `BranchHistory`, `NodeHistoryBack`, `NodeHistoryForward`,
  `InsertNodeTag`, `RemoveNodeTag`, `SetNodeBody`, `AppendNodeProperty`, `AddNodeClassification`,
  `RecordNodeDerivation`, `SetEdgeSemanticPredicate`, `AssertSemanticPredicate`, `AddField`,
  `RetireField`, `AddCoupling`, `SetFieldCouplingStrength`. Deltas are in-memory intents (Clone+Debug,
  not serialized), so variants may carry rich payloads (`Field`, `Coupling`, `NodeProperty`).
- `GraphDeltaResult` additions: `HistoryStepped(Option<String>)`, `FieldChanged(bool)`.
- A handful of ergonomic wrappers in `apply.rs` (e.g. `apply::add_node -> NodeKey`) that construct the
  delta and unwrap the result — they keep the funnel (they call `apply_graph_delta`) while sparing hot
  call sites the enum-unwrap noise.
- `EdgePayload`'s own `pub` methods stay as-is: gating `get_edge_mut` to `pub(crate)` removes the only
  external path to a `&mut EdgePayload`.

## Done conditions

- Every class-1 mutator and both escape hatches are `pub(crate)`; the workspace compiles; all suites
  pass. The Phase 6.5 comment in `graph/mod.rs` rewritten to describe the *enforced* state and the
  four-class taxonomy.
- `apply_graph_delta` is the only external write path for primitive mutations (grep for direct mutator
  calls outside kernel comes back empty outside `fixtures`-gated test code).
- `kernel/fixtures` feature exists; test crates enable it in dev-deps only.

## Gotchas

- `meerkat/src/agent_harness/tests.rs` is dirty in Mark's tree and needs one `use` line (fixtures
  trait for its `get_node_mut` fixture) — a single-line add should auto-merge; flag at merge time.
- `Orrery::ingest_graph(|&mut Graph| ...)` closures keep working: `apply_graph_delta` is `pub` and
  takes any `&mut Graph`; only raw mutator calls inside closures need rewriting (command_drain's body
  write).
- `apply.rs` will blow the 600-LOC ceiling with ~16 new arms — split into `delta.rs` (enum + results)
  and `apply.rs` (fn + wrappers) when it does.
- Feature unification: `cargo test --workspace` builds kernel with `fixtures` on everywhere; enforcement
  is "production code doesn't import the trait" + release builds (no dev-deps) lack the feature. Good
  enough; the boundary is review-visible, which is the point.

## Progress

- 2026-07-01: Audited (mutator inventory incl. multiline signatures; production vs test split via
  cfg(test)-cutoff line counting; escape-hatch reads). Classification + delta design above. Starting
  kernel-side implementation in a worktree.
- 2026-07-01: **Implemented.** Kernel: 16 new `GraphDelta` variants + `HistoryStepped`/`FieldChanged`/
  `Applied` results, three new sanctioned setters (`set_node_body`, `append_node_property`,
  `set_edge_semantic_predicate` — all born `pub(crate)`), four ergonomic wrappers in `apply`
  (`add_node`/`assert_relation`/`node_history_back`/`node_history_forward` — each routes through
  `apply_graph_delta`, so the funnel holds), ~52 mutators swept to `pub(crate)` across six files, the
  `fixtures` feature + `GraphFixtures` trait (24 methods incl. both `&mut` escape hatches), and the
  Phase 6.5 comment rewritten to the enforced four-class taxonomy. Call sites: orrery
  (input/selection/nodes/build/fields/lifecycle), aether `commit_to_graph`, platen
  `project_orrery_subgraph`, linked-data ingest (the biggest escape-hatch user — title/tags/properties/
  classifications/derivations/predicates all via deltas now), inker statements, meerkat web_clip +
  command_drain (the `node.body` raw writes). Fixtures: 10 crates gained the dev-dep, ~19 test modules
  gained the one-line import. All non-meerkat crates green (877 tests); meerkat test-compile clean.
  The compiler did the call-site enumeration once visibility flipped — grep-based auditing had missed
  gyre's five inline test modules and three orrery test modules; E0624 found every one.
