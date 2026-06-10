# Mere Post-Engine-Layer Priorities

**Date**: 2026-05-09
**Status**: Active forward-looking plan

> **Crate-name note (2026-06-09 audit):** the §1 18-crate list and §2 host references are a 2026-05-09 **gpui-era** snapshot. The host is now `meerkat` (serval-as-host), not gpui: `mere-host`→`meerkat`, `mere-kernel`→`graph/graph-kernel`, `mere-host-contract`→`system/...`, `intelligence-embeddings`→`intel/embed`, `mere-transport`→`murm/transport`, `verso-tile`→`verso`. Dated status notes below are historical record.
**Scope**: What's still open after the Graphshell→Mere migration finished and the engine layer (inker + nematic + polyglot knot + uxtree projection) landed. Replaces the 2026-05-06 Graphshell migration plan, which is archived under [`../../archive_docs/2026-05-09_engine_layer_complete/`](../../archive_docs/2026-05-09_engine_layer_complete/) along with its companion donor inventory.

**Related**:

- [`2026-05-05_protocol_architecture_plan.md`](2026-05-05_protocol_architecture_plan.md) — protocol architecture
- [`2026-05-07_event_dag_substrate_brief.md`](2026-05-07_event_dag_substrate_brief.md) — event-DAG substrate pivot
- [`../../nematic_docs/implementation_strategy/2026-05-08_polyglot_knot_design.md`](../../nematic_docs/implementation_strategy/2026-05-08_polyglot_knot_design.md) — polyglot knot design
- [`../../eidetic_docs/implementation_strategy/2026-05-09_eidetic_layered_stack_plan.md`](../../eidetic_docs/implementation_strategy/2026-05-09_eidetic_layered_stack_plan.md) — eidetic layered stack
- Archived: [`../../archive_docs/2026-05-09_engine_layer_complete/2026-05-06_graphshell_migration_plan.md`](../../archive_docs/2026-05-09_engine_layer_complete/2026-05-06_graphshell_migration_plan.md) — full historical migration plan + progress log

---

## 1. State as of 2026-05-09

**Engine layer complete.** Don't carry these forward as future work:

- 18-crate Mere workspace including renamed `mere-kernel` (was `graphshell-core`), `mere-host-contract` (was `graphshell-runtime`), plus new `mere-host` (gpui), `mere-domain/{workbench,gloss,apparatus,frame,orrery}`, `eidetic`, `intelligence-embeddings`, `inker`, `nematic`, `platen`, `verso-tile`, `uxtree`.
- Twelve nematic engines: `markdown`, `gemtext`, `gopher`, `feed`, `text`, `file`, `finger`, `knot`, `scroll`, `misfin`, `nex`, `guppy`. Each spec-faithful in its protocol's grammar.
- Inker routing: scheme rules + content-type rules + per-host overrides + per-node pinned engine + availability filtering. Priority: pin > content-type > per-host > scheme > fallback.
- Document model with provenance, trust state, diagnostics, structural blocks, and four semantic block variants (`FeedHeader`, `FeedEntry`, `MetadataRow`, `Badge`). `to_markdown` / `to_gemini` / `to_knot` round-trip rendering.
- Polyglot knot format (`nematic.knot`): fenced protocol blocks (`gemtext` / `gopher` / `nex` / `feed-entry` / `feed-header` / `metadata-row` / `badge`) embed alongside markdown prose; wikilinks (`[[name]]` → `mere://node/<slug>`); hashtags extracted to Badge sibling blocks; `build_clip_knot` helper for the future host-side clip gesture.
- `uxtree` projection from `EngineDocument` → AccessKit `TreeUpdate` (with `stitch` for combining mere-domain subtrees under a host root).
- 1206 tests passing across the workspace, 0 failing. All source files under the 600 LOC ceiling. Mixed-form `mod tests;` pattern in use for crates whose tests would otherwise tip them over.

**Where we are now**: the host is the load-bearing remainder. `mere-host` has ~550 LOC of gpui orchestration (app bootstrap, window lifecycle, frame traversal, uxtree composition, demo fixtures). `mere-domain` crates are forming around per-domain rendering. Without the host, none of the engine work is user-visible.

---

## 2. Open Work

### 2.1 Host completion (active in `mere-host` + `mere-domain/*`)

`mere-host` is the gpui-first host per the [Glass-HQ / gpui pivot](memory). Pending:

- Per-domain renderers in `mere-domain/{workbench,gloss,apparatus,orrery,frame}` filling out beyond their fixtures.
- Real input → reducer intent translation (today's gpui mouse/move/down/up wiring is bootstrap-shaped).
- AccessKit OS adapter wiring. The gpui upstream doesn't currently expose window handles publicly, so `accesskit_windows::Adapter::with_hwnd` and equivalents can't be constructed without a gpui patch. `uxtree` produces the `TreeUpdate` correctly; pushing it to the OS is a separate piece of work.
- Real `SurfacePlacementPlan` consumption (pixels for clipped tiles, not just demo content).

**Fallbacks**: `graphshell-host-iced` (port-boundary scaffold exists) and HTML/CSS retained per the same memory entry.

### 2.2 Donor-area rebuilds still pending

Areas the donor `GraphBrowserApp` had that Mere has not yet rebuilt. Order by user-visible impact:

- **Runtime / webview lifecycle wiring** — route `host_open_request`, surface present/retire, URL/title/history/scroll/crash events through the reducer rather than direct mutation. Planning shape exists in `app_state::workspace_routing` + `verso-tile::surface`; needs concrete gpui or iced wiring.
- **History / undo / archive** — mutation-journal contract exists; needs reducer-side undo state, navigation preview cursor, Eidetic-backed archive queries.
- **Clip capture + clip nodes** — modal/action UX state exists in `app_state::app_ux`; needs the capture payload type, the host-side gesture, node creation through graph-runtime intents, storage through Eidetic. With `nematic::knot::build_clip_knot` ready, the missing piece is the gesture wiring.
- **Persistence health / snapshot timers / autosave** — service-glue concerns; live next to `WorkspaceServices` rather than inside the reducer.
- **Sync / storage interop** — rebuild around `mere-transport`, `murm`, `moothold`. The donor's `set_sync_command_tx` / `set_client_storage_manager` shape is largely obsolete residue; design fresh against the protocol crates.

### 2.3 Phase-1 loose ends — decide, don't defer

Two crates the original migration listed but never moved into the Mere workspace:

- **`graphshell-comms`** — never created. The donor crate's role overlaps `murm` (bilateral) and `moothold` (federation). **Recommendation: drop from the plan.** The protocol crates already own these concerns; a wrapper crate would be a layering tax with no payoff.
- ~~**`graph-cartography`**~~ — **resolved 2026-05-10: drop**. Read pass over `repos/graphshell/crates/graph-cartography/src/lib.rs` (3623 LOC) confirms the crate is not graph-traversal/path code — its public surface is `build_entry_edge_rollups`, `build_activation_freshness`, `build_traversal_centrality` (visit-frequency, not BFS/DFS), `build_repeated_path_priors`, `build_co_activation_pairs`, `build_frame_reformation_patterns`, plus a privacy/promotion model, a cartography-snapshot persistence layer, an invalidation-event vocabulary + planner, and contribution-assembly + agent-input/output envelopes. None of that is load-bearing graph traversal in `graph-tree` / `platen`'s sense. The same concerns are being designed fresh in:
  - **`eidetic`** — engram-based memory with three-axis classification (privacy / provenance / trust); the [2026-05-09 eidetic design pass](../../eidetic_docs/research/2026-05-09_eidetic_design_pass.md) supersedes graph-cartography's substrate-owned `EntryKey` with content-addressed engrams.
  - **`intelligence-embeddings`** — aggregate stability + clustering + learned-affinity surfaces (see the [2026-05-08 local-intelligence research](../../mere_docs/research/2026-05-08_local_intelligence_integration_research.md)). The `DEFAULT_CLUSTER_HYSTERESIS_MARGIN` / `hysteresis_decision_against` pattern is small enough to re-derive when needed.
  - **`moothold`** — privacy promotion + contribution assembly + tier-framed sharing (orrery t1 → moot t2 → moothold t3 → coalition t4). Graph-cartography's `PrivacyScope::{LocalOnly, DeviceSync, Shared}` enum predates the tier lexicon and would need a full rename anyway.

  Bringing the crate over wholesale would duplicate ~3600 LOC of work that's being redesigned against the post-pivot substrate (event-DAG, BLAKE3 engrams, tier framework) and lock in pre-pivot vocabulary that conflicts with the new lexicon. **Recommendation: do not migrate. Re-derive the patterns inside each new owner crate as those crates' designs mature.** The donor `graph-cartography` remains intact at its original location for reference.

### 2.4 Engine-layer follow-ups (small)

Identified during the 2026-05-09 review pass:

- **`to_knot()` round-trips frontmatter**. Currently emits only blocks; `EngineDocument.title` / `provenance` / `trust` don't make it into the frontmatter on re-render. `build_clip_knot` does this on construction, so re-rendering a parsed knot loses metadata. ~50 LOC fix.
- **Per-block provenance**. The slot exists on the document model (`FeedEntry.source_url`); a generic `block.provenance` field becomes useful when knots aggregate from multiple sources. Activate when `intelligence-embeddings` starts cross-source matching.
- **Content-type sniff helper in `inker`**. Small `sniff_content_type(bytes) -> Option<&'static str>` (file extension + magic bytes) lets the host populate `EngineRouteRequest.content_type` accurately without pre-parsing the body itself. ~80 LOC.
- ~~**`mere-kernel` / `graphshell-core` rename audit**~~ — **resolved 2026-05-09**: grep confirms `graphshell_core::*` no longer appears anywhere in `crates/` (only as a description string in `mere-kernel/Cargo.toml`). `inker::routing` uses `mere_kernel::graph::{GraphViewId, NodeKey}` directly; the rename is complete. The `graphshell` *name* survives as a separate concept — the shell layer crate (`crates/graphshell`, `crates/graphshell/shell-state`, `crates/mere-domain/graphshell`), not a kernel alias.
- **Pinned-engine surface mode**. `route_filtered` defaults to `CompositedTexture` for pinned routes. A user pinning `graphshell.internal` (a `Headless` engine) gets the wrong surface mode. Fix when a real consumer hits it.

### 2.5 Legitimate defers

- **Friction point #4 — split `graphshell` meta-crate from the reducer**. Splitting requires committing to a public API shape with no second consumer to validate against. Leave deferred until either (a) a real second consumer of the shell vocabulary appears, or (b) the reducer surface becomes painful enough inside the meta-crate to force the split on its own merits.
- **HTML reader-mode lane in nematic — don't build it**. Per the [Blitz/Serval convergence](memory) memory's three-head Hekate framing, HTML in any rendering depth is Serval's job. The reader-mode use case is a future Serval mode (smolweb extract / middlenet / fullweb negotiator), not a nematic engine. Resist the urge to add `nematic.html-reader`.

---

## 3. Pitfalls (architectural invariants)

- Do not split Navigator into multiple instances; maintain the single surface with configurable scope/form factor.
- Do not let host adapters mutate graph truth directly while implementing surface command outcomes.
- Do not import concrete desktop/webview/renderer dependencies into `graphshell::app_state`, `platen`, `inker`, or `verso-tile`.
- Do not treat `verso-tile` as old `verso`; engine routing belongs in `inker`, graph-aware composition belongs in `platen`.
- Do not copy donor `GraphBrowserApp` method bodies wholesale; classify against the (archived) donor inventory, take the contract or invariant, and rebuild against the new owner crate.
- Do not invent semantics in protocol engines that the spec doesn't say. The protocol-faithfulness rule binds future contributors. Knot is the only Mere-defined format and is allowed to be richer.
- Do not gate features on a hypothetical "consumer" when Mere is the only consumer. Build because the product needs it.

---

## 4. Cadence

- `cargo test --workspace` from `repos/mere` after every narrow change.
- File-size ceiling: 600 LOC per source file. Split before adding when a touched file is approaching the limit.
- Touched-only file decomposition — don't open file-size cleanup as its own slice unless a file is actively blocking comprehension.
- Design docs go in `design_docs/<area>/implementation_strategy/YYYY-MM-DD_<keyword>.md` per [`../../DOC_POLICY.md`](../../DOC_POLICY.md); update [`../../DOC_README.md`](../../DOC_README.md) when a new doc lands.
