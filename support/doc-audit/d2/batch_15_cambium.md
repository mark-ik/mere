# Batch 15 — Cambium active-tree additions

Audited read-only against Mere `724b613d078db74b389545c072a10662ba8c612e`
and live Genet `57929ac614802ff58adaf3ea39f5beab89507156`. Claims were
checked from status text, cited paths, named public symbols, manifests, and
committed receipts. Historical claims were held to their stated date; current
claims were checked against the two named trees.

| doc | disposition | status accurate | claims | holds | stale | unverifiable |
|---|---|---|---|---|---|---|
| cambium_docs/implementation_strategy/2026-05-27_serval_as_host_xilem_serval_plan.md | current | no | 10 | 8 | 1 | 1 |
| cambium_docs/implementation_strategy/2026-06-01_event_model_convergence_plan.md | current | yes | 8 | 7 | 0 | 1 |
| cambium_docs/implementation_strategy/2026-07-07_chisel_widget_leaf_design.md | historical-unmarked | no | 8 | 4 | 4 | 0 |
| cambium_docs/implementation_strategy/2026-07-08_chisel_widget_catalog.md | historical-unmarked | no | 8 | 3 | 5 | 0 |
| cambium_docs/implementation_strategy/2026-07-15_component_catalog_growth_plan.md | current | yes | 9 | 9 | 0 | 0 |
| cambium_docs/implementation_strategy/2026-08-31_workbench_component_plan.md | current | no | 9 | 8 | 1 | 0 |
| cambium_docs/implementation_strategy/2026-09-03_host_ui_zoom_plan.md | current | no | 9 | 8 | 1 | 0 |
| cambium_docs/implementation_strategy/2026-09-06_fact_visualization_leaves_plan.md | current | yes | 8 | 8 | 0 | 0 |
| cambium_docs/technical_architecture/2026-09-03_cambium_architecture.md | current | n/a | 7 | 7 | 0 | 0 |
| cambium_docs/technical_architecture/component-catalog.md | current | n/a | 9 | 9 | 0 | 0 |
| cambium_docs/technical_architecture/genet-compatibility.md | historical-marked | no | 8 | 4 | 3 | 1 |
| cambium_docs/technical_architecture/graph-canvas-swatch.md | current | n/a | 8 | 8 | 0 | 0 |
| cambium_docs/technical_architecture/namespace-claims.md | historical-marked | n/a | 6 | 5 | 0 | 1 |
| cambium_docs/technical_architecture/upstream-xilem.md | historical-marked | yes | 10 | 9 | 0 | 1 |
| cambium_docs/testing/local-genet-development.md | current | n/a | 7 | 7 | 0 | 0 |
| cambium_docs/testing/receipts/README.md | current | n/a | 6 | 6 | 0 | 0 |

**Totals: 16 docs, 130 claims checked (110 holds, 15 stale, 5 unverifiable), 3 contradictions.**

## cambium_docs/implementation_strategy/2026-05-27_serval_as_host_xilem_serval_plan.md
- disposition: current
- status line: "strong through Stages 0-7; the previously named host-backend blockers are now landed" — accurate: no
- claims checked: 10 — holds: 8, stale: 1, unverifiable: 1

### Stale claims
- The plan still names the live backend `xilem_serval`, `ServalAppRunner`, `ServalCtx`, and `ServalElement`. The current public names are `GenetAppRunner`, `GenetCtx`, and `GenetElement`; the Serval spellings survive only as deprecated aliases (`crates/cambium/cambium/src/lib.rs:186-195`).

### Contradictions
- none

### Recommended action
- update-status (name Cambium over Genet as the current implementation and identify the Serval vocabulary as extraction history)

### Notes
- The substantive loop still holds: the Genet runner, capture/bubble registries, focus, pointer capture, text controls, IME translation, scrolling, overlays, AccessKit projection, and headed host are present under `crates/cambium/`. The historical `pelt-live-counter` pixel claim was not independently replayed.

## cambium_docs/implementation_strategy/2026-06-01_event_model_convergence_plan.md
- disposition: current
- status line: "core dispatcher convergence landed; `window`/`document` targeting and shadow-tree `composedPath` remain explicitly deferred" — accurate: yes
- claims checked: 8 — holds: 7, stale: 0, unverifiable: 1

### Stale claims
- none

### Contradictions
- none

### Recommended action
- none

### Notes
- Cambium's current dispatcher records handlers by node and phase and walks ancestors for capture and bubble (`crates/cambium/cambium/src/context.rs:330-381`, `event.rs:206-276`). The document correctly leaves browser `window`/`document`, shadow retargeting, and native `currentTarget` outside that shared core. The dated WPT count was not replayed.

## cambium_docs/implementation_strategy/2026-07-07_chisel_widget_leaf_design.md
- disposition: historical-unmarked
- status line: "proposed (2026-07-07); first design pass" — accurate: no
- claims checked: 8 — holds: 4, stale: 4, unverifiable: 0

### Stale claims
- There is no current `components/chisel` or `chisel` package. Its `Leaf`, `PaintCx`, `LeafRegistry`, and `RenderedLeaves` contract now belongs to `sprigging` (`crates/cambium/sprigging/src/lib.rs:76,218,251,323`).
- The authored element is now `<custom-leaf>` through `custom_leaf`, while `<chisel-leaf>` is only a read-side compatibility spelling (`crates/cambium/cambium/src/tags.rs:87-108`; Genet `components/genet-livery/src/paint.rs:510`).
- The proposed standalone-repository and crates.io-name decision did not occur; Sprigging is a Mere workspace member and publishable package (`Cargo.toml:141,349`; `crates/cambium/sprigging/Cargo.toml:2-9`).
- The proposed Orrery first consumer is obsolete; the current reusable graph and angle leaves live in Sprigging and the host remains application-neutral.

### Contradictions
- The header says “proposed” while the same opening section says the scaffold, layout seam, author view, and render path landed.

### Recommended action
- mark-historical (point to Sprigging and the current component-catalog plan)

### Notes
- The central design judgment survived: retained custom paint plugs into Genet's layout and paint order rather than creating a second compositor.

## cambium_docs/implementation_strategy/2026-07-08_chisel_widget_catalog.md
- disposition: historical-unmarked
- status line: "proposed catalog + build order (2026-07-08)" — accurate: no
- claims checked: 8 — holds: 3, stale: 5, unverifiable: 0

### Stale claims
- The document assigns the catalog to `chisel`; the current split is Cambium view composition plus Sprigging paint leaves (`crates/cambium/cambium/examples/component_catalog.rs`; `crates/cambium/sprigging/src/lib.rs`).
- `chisel::grid`, `chisel::glyphs`, and `chisel::arrange` no longer exist. Their surviving contracts are `sprigging::{GridSpec, GraphGlyph, Knob, Meter}` and Cambium's arrangement views (`crates/cambium/sprigging/src/lib.rs:39-43`; `crates/cambium/cambium/src/arrangement.rs:27-70`).
- The catalog's `<chisel-leaf>` examples are not the write contract; Cambium emits `<custom-leaf>` (`crates/cambium/cambium/src/tags.rs:87-108`).
- The MeerKat/Orrery and `smoke_chisel.rs` routes named as consumers are absent from the current tree.
- The family-crate split forecast is overtaken by the `cambium`, `sprigging`, `meristem`, host, and `workbench` family now listed in the root workspace (`Cargo.toml:133-143`).

### Contradictions
- The header says “proposed,” but the build-order body repeatedly records grid, glyph, arrangement, Path-B, and inline-leaf work as landed.

### Recommended action
- mark-historical (superseded in practice by `2026-07-15_component_catalog_growth_plan.md` and `technical_architecture/component-catalog.md`)

### Notes
- Its durable sorting rule remains useful: ordinary semantic controls stay DOM/CSS; geometry-heavy leaves use the retained paint seam.

## cambium_docs/implementation_strategy/2026-07-15_component_catalog_growth_plan.md
- disposition: current
- status line: "active" — accurate: yes
- claims checked: 9 — holds: 9, stale: 0, unverifiable: 0

### Stale claims
- none

### Contradictions
- none

### Recommended action
- none

### Notes
- The executable catalog exists and covers overlays, command surfaces, selection bars, reorder, disclosure, summary bodies, component-local state, graph canvas, and Sprigging leaves. The named source modules are present under `crates/cambium/cambium/src/`, and the explicit family-level deferrals remain open.

## cambium_docs/implementation_strategy/2026-08-31_workbench_component_plan.md
- disposition: current
- status line: "W5 opened (2026-09-04) ... S1 and S2 landed ... S3 ... waits on a push of this repository and a pin bump" — accurate: no
- claims checked: 9 — holds: 8, stale: 1, unverifiable: 0

### Stale claims
- The repository-push half of the S3 gate is no longer current: S1/S2's `Workspace`, floating-tile reducer, strip/frame composition, and Workbench 0.1.0 are committed in this Mere snapshot (`crates/cambium/workbench/float.rs:140-228`; `Cargo.toml:143,373`). Whether Turnstone has consumed the new pin remains a separate consumer check.

### Contradictions
- none

### Recommended action
- update-status (replace “waits on a push” with the exact remaining Turnstone pin/adoption state)

### Notes
- W1-W4 and S1-S2 have concrete current owners. `TileTree`, `Workbench`, `Workspace`, `FloatingTile`, and their reducers remain in the named crate (`crates/cambium/workbench/lib.rs:48-285`; `float.rs:55-228`).

## cambium_docs/implementation_strategy/2026-09-03_host_ui_zoom_plan.md
- disposition: current
- status line: "in progress ... Z0 through Z4 landed ... Z5 landed in isometry ... nothing committed" — accurate: no
- claims checked: 9 — holds: 8, stale: 1, unverifiable: 0

### Stale claims
- “nothing committed” is false for Z0-Z4: `HostOptions::ui_zoom`, `fit_design`, `layout_scale`, zoom chords/wheel, scaled AccessKit, frame-inset scaling, and the host tests are committed under `crates/cambium/` (`cambium-rootstock/src/host.rs:188-205,947-1039`; `cambium-genet-winit-host/tests/ui_zoom.rs`).

### Contradictions
- The status says Z0-Z4 landed and “nothing committed”; the Progress section names the committed implementations and receipts for each gate.

### Recommended action
- update-status (scope the uncommitted qualifier to the dated Isometry Z5 state, or replace it with current consumer evidence)

### Notes
- The plan's core formula is exact in code: `layout_scale = scale_factor * ui_zoom` (`crates/cambium/cambium-rootstock/src/host.rs:947-951`).

## cambium_docs/implementation_strategy/2026-09-06_fact_visualization_leaves_plan.md
- disposition: current
- status line: "V0 landed; V1 and V2 planned" — accurate: yes
- claims checked: 8 — holds: 8, stale: 0, unverifiable: 0

### Stale claims
- none

### Contradictions
- none

### Recommended action
- none

### Notes
- `AngleStrip` and `AngleStripMark` are implemented and exported (`crates/cambium/sprigging/src/angle.rs:19-104`; `sprigging/src/lib.rs:39`), and the catalog contains the six-mark specimen (`crates/cambium/cambium/examples/component_catalog.rs:1417-1424`). No `DimensionLine` or range scrubber symbol exists, matching V1/V2's planned state.

## cambium_docs/technical_architecture/2026-09-03_cambium_architecture.md
- disposition: current
- status line: "(none)" — accurate: n/a
- claims checked: 7 — holds: 7, stale: 0, unverifiable: 0

### Stale claims
- none

### Contradictions
- none

### Recommended action
- none

### Notes
- Workspace membership and manifests preserve the stated direction: Cambium depends on Genet seam packages; the live Genet manifest contains no Cambium, Meristem, or Sprigging dependency. Deprecated `Serval*` aliases are visibly bounded in `crates/cambium/cambium/src/lib.rs:186-195`.

## cambium_docs/technical_architecture/component-catalog.md
- disposition: current
- status line: "(none)" — accurate: n/a
- claims checked: 9 — holds: 9, stale: 0, unverifiable: 0

### Stale claims
- none

### Contradictions
- none

### Recommended action
- none

### Notes
- The catalog source exists, packages as the `cambium` example, and its modules cover every family named in the table. Both headless semantics and retained-leaf assertions are present in the example; Genet-side layout/paint remains outside it as stated.

## cambium_docs/technical_architecture/genet-compatibility.md
- disposition: historical-marked
- status line: "Current verified set — Verified on 2026-07-22; stack updated 2026-08-20" — accurate: no
- claims checked: 8 — holds: 4, stale: 3, unverifiable: 1

### Stale claims
- The “current” Errand row says 0.1.3, while the current workspace package is 0.3.4 (`Cargo.toml:374`).
- The source-location narrative still places the Cambium workspace in Genet. The family is now in Mere at `crates/cambium/`, as its own current architecture and root manifest show (`Cargo.toml:133-143`).
- The document says every sibling consumes the family from `genet.git`; Mere now owns the family locally and pins only Genet seam packages to `genet.git` (`Cargo.toml:330,361-366`).

### Contradictions
- none

### Recommended action
- update-status (keep the dated release table as history, add a current Mere-owned compatibility row)

### Notes
- Commit `2e462fe8975` resolves in Genet. `cambium-winit-a11y` remains intentionally unpublished (`crates/cambium/cambium-winit-a11y/Cargo.toml:9-15`), and `<custom-leaf>` with a `<chisel-leaf>` read alias remains accurate. Live crates.io publication state was not queried.

## cambium_docs/technical_architecture/graph-canvas-swatch.md
- disposition: current
- status line: "(none)" — accurate: n/a
- claims checked: 8 — holds: 8, stale: 0, unverifiable: 0

### Stale claims
- none

### Contradictions
- none

### Recommended action
- none

### Notes
- `GraphCanvasSwatch`, `GraphCanvasSubgraph`, node/edge models, `GraphViewport`, callbacks, focus/hover/selection state, and the default 260x128 size are implemented in `crates/cambium/cambium/src/graph_canvas.rs:101-243`; Sprigging owns the leaf and projection geometry (`sprigging/src/glyphs.rs:40-152`).

## cambium_docs/technical_architecture/namespace-claims.md
- disposition: historical-marked
- status line: "claimed ... on 2026-07-13; `cambium-nematic` ... 2026-07-14" — accurate: n/a
- claims checked: 6 — holds: 5, stale: 0, unverifiable: 1

### Stale claims
- none

### Contradictions
- none

### Recommended action
- none

### Notes
- Current local packages confirm `cambium`, `meristem`, `sprigging`, and `cambium-nematic`; Genet remains the adjacent engine. The historical registry-claim operations, including `genet-stylo`, were not independently verified against crates.io.

## cambium_docs/technical_architecture/upstream-xilem.md
- disposition: historical-marked
- status line: "Historical note (2026-09-05): recorded revisions and replay describe extraction provenance ... not a current upstream alignment claim" — accurate: yes
- claims checked: 10 — holds: 9, stale: 0, unverifiable: 1

### Stale claims
- none

### Contradictions
- none

### Recommended action
- none

### Notes
- The retained Meristem surface and the three local `ElementSplice` operations exist (`crates/cambium/meristem/src/element_splice.rs:14-61`); removed modules and `hashbrown` are absent, and the manifest is version 0.2.0. The external fork-tip hashes were treated as dated provenance and not fetched.

## cambium_docs/testing/local-genet-development.md
- disposition: current
- status line: "(none)" — accurate: n/a
- claims checked: 7 — holds: 7, stale: 0, unverifiable: 0

### Stale claims
- none

### Contradictions
- none

### Recommended action
- none

### Notes
- Mere's root workspace pins the Genet source once at revision `221415af...`; Cambium manifests use `workspace = true`. `.cargo/config.toml.example` exists, the live override file is ignored, and workspace-member packages are absent from its Genet patch table, matching both cautions.

## cambium_docs/testing/receipts/README.md
- disposition: current
- status line: "(none)" — accurate: n/a
- claims checked: 6 — holds: 6, stale: 0, unverifiable: 0

### Stale claims
- none

### Contradictions
- none

### Recommended action
- none

### Notes
- Both named HTML receipts are committed beside the README at 54,895 and 54,898 bytes. The example accepts `--write-receipts`, compares generated output, and carries retained-leaf assertions in `crates/cambium/cambium/examples/component_catalog.rs`.
