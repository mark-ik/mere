# Batch 16 — post-snapshot documents outside Cambium

| doc | disposition | status accurate | claims | holds | stale | unverifiable |
|---|---|---:|---:|---:|---:|---:|
| eidetic_docs/research/2026-09-04_eidetic_review_brief.md | current | yes | 12 | 12 | 0 | 0 |
| inker_docs/implementation_strategy/2026-06-15_engine_picker_and_pluggability_plan.md | current | yes | 10 | 10 | 0 | 0 |
| inker_docs/research/2026-09-03_web_surface_contract_assessment.md | current | no | 11 | 9 | 2 | 0 |
| mere_docs/implementation_strategy/2026-08-31_terminology_and_crate_folds_plan.md | historical-marked | yes | 10 | 10 | 0 | 0 |
| mere_docs/implementation_strategy/2026-09-02_autodiff_lora_trainer_plan.md | current | no | 13 | 11 | 1 | 1 |
| mere_docs/implementation_strategy/2026-09-02_distillery_projection_walk_plan.md | current | yes | 10 | 9 | 1 | 0 |
| mere_docs/implementation_strategy/2026-09-02_physics_catalog_plan.md | current | yes | 14 | 12 | 1 | 1 |
| mere_docs/implementation_strategy/2026-09-02_platform_boundary_and_repository_topology_plan.md | historical-marked | yes | 18 | 17 | 0 | 1 |
| mere_docs/research/2026-09-01_frontier_sync_over_reticulum_brief.md | current | yes | 10 | 9 | 0 | 1 |
| mere_docs/research/2026-09-02_port_gui_composition_and_comparable_stacks_brief.md | current | n/a | 9 | 9 | 0 | 0 |
| mere_docs/testing/2026-08-31_flora_tulpa_standing_receipt.md | historical-marked | yes | 10 | 9 | 0 | 1 |
| moothold_docs/implementation_strategy/2026-08-31_flora_tulpa_standing_plan.md | historical-marked | no | 10 | 9 | 1 | 0 |
| nematic_docs/implementation_strategy/2026-05-08_polyglot_knot_design.md | current | yes | 9 | 9 | 0 | 0 |
| nematic_docs/implementation_strategy/2026-06-13_polyglot_block_resolver_plan.md | current | yes | 8 | 8 | 0 | 0 |
| nematic_docs/implementation_strategy/2026-06-27_native_smolweb_rendering_plan.md | historical-unmarked | no | 13 | 11 | 1 | 1 |
| nematic_docs/implementation_strategy/2026-07-01_smolweb_fidelity_plan.md | current | yes | 9 | 8 | 1 | 0 |
| verso_docs/implementation_strategy/2026-06-23_genet_scrying_flipcarrier_plan.md | historical-marked | no | 10 | 9 | 1 | 0 |
| verso_docs/technical_architecture/2026-06-10_compatibility_view_charter.md | historical-marked | no | 9 | 8 | 1 | 0 |
| **Totals** |  |  | **195** | **179** | **10** | **6** |

**Totals: 18 docs, 195 claims checked (179 holds, 10 stale, 6 unverifiable), 8 contradictions.**

Audit base: Mere `724b613d` (2026-09-06), with adjacent sibling checkouts used
for cross-repository ownership and commit evidence. Snapshot base: Mere
`bcb222ce`. `archive_docs/` is excluded.

The earlier “16 additions” was a net-count inference, not a coverage manifest.
From the snapshot through `724b613d`, 34 active Markdown documents were added
and 17 previously audited documents were archived. The live active tree is 298
documents. This batch covers the 18 additions outside `cambium_docs/`.

## eidetic_docs/research/2026-09-04_eidetic_review_brief.md

- disposition: current
- status line: "Review brief. Findings only; every recommendation ends in a decision gate." — accurate: yes
- claims checked: 12 — holds: 12, stale: 0, unverifiable: 0

### Stale claims

- None. The current `crates/intel/eidetic-search` tree and index summary agree
  on location, consumers, index recreation, unused APIs, duplicate models, and
  dependency-cone findings.

### Contradictions

- None.

### Recommended action

- none

### Notes

- Exact line counts are snapshot measurements and should be refreshed before
  being reused as sizing evidence.

## inker_docs/implementation_strategy/2026-06-15_engine_picker_and_pluggability_plan.md

- disposition: current
- status line: "Phases 0–3 shipped + verified; Remaining: Phase 4." — accurate: yes
- claims checked: 10 — holds: 10, stale: 0, unverifiable: 0

### Stale claims

- None. `EngineRoutePolicy`, route requests/decisions, `engine_pins`, the
  registry/manager boundary, per-node selection, and Pelt/import consumers are
  present in the pinned tree.

### Contradictions

- None.

### Recommended action

- none

### Notes

- Old donor locations are already occurrence-marked as historical.

## inker_docs/research/2026-09-03_web_surface_contract_assessment.md

- disposition: current
- status line: "assessment complete; migration started separately from the triplet release gate" — accurate: no
- claims checked: 11 — holds: 9, stale: 2, unverifiable: 0

### Stale claims

- Extraction is no longer merely prospective: `mere-surface-api` exists. —
  evidence: `crates/system/surface-api/Cargo.toml:2` → package name.
- The boundary migration is complete through the platform plan's P2, rather
  than only “started.” — evidence:
  `mere_docs/implementation_strategy/2026-09-02_platform_boundary_and_repository_topology_plan.md:4`
  → P0-P7 complete.

### Contradictions

- The dated 2026-09-04 header is historically accurate but lacks a terminal
  follow-up for the completed migration.

### Recommended action

- update-status

### Notes

- The neutral-contract rationale remains current; package releases are dated
  publication facts.

## mere_docs/implementation_strategy/2026-08-31_terminology_and_crate_folds_plan.md

- disposition: historical-marked
- status line: "landed on main (2026-09-02)" — accurate: yes
- claims checked: 10 — holds: 10, stale: 0, unverifiable: 0

### Stale claims

- None. Current manifests and source names reflect the recorded folds, and the
  named commits resolve in the local repository family.

### Contradictions

- None.

### Recommended action

- none

### Notes

- Pre-fold names should remain confined to historical prose and compatibility
  readers.

## mere_docs/implementation_strategy/2026-09-02_autodiff_lora_trainer_plan.md

- disposition: current
- status line: "in progress (2026-09-02)" — accurate: no
- claims checked: 13 — holds: 11, stale: 1, unverifiable: 1

### Stale claims

- The status label says “in progress,” while the same paragraph and Progress
  ledger say every phase landed on 2026-09-03. — evidence:
  `mere_docs/implementation_strategy/2026-09-02_autodiff_lora_trainer_plan.md:4`
  → both statements occur in the header.

### Contradictions

- “In progress” contradicts “every phase landed” in the same header.

### Recommended action

- update-status

### Notes

- Source and tests prove `Device::autodiff`, detached base weights, the
  Distillery request arm, Djinn CPU/GPU lanes, adapter compatibility, and the
  local parity receipt. The historical Fedora execution was not repeated.

## mere_docs/implementation_strategy/2026-09-02_distillery_projection_walk_plan.md

- disposition: current
- status line: "W0 complete; W1 held" — accurate: yes
- claims checked: 10 — holds: 9, stale: 1, unverifiable: 0

### Stale claims

- `DOC_README.md` still calls implementation unstarted. — evidence:
  `DOC_README.md:116` versus
  `mere_docs/implementation_strategy/2026-09-02_distillery_projection_walk_plan.md:4`
  and `ports/distillery/tests/walk_fixtures.rs:487` → W0 plus two owners.

### Contradictions

- The plan and index disagree about whether implementation has started.

### Recommended action

- fix-refs (`DOC_README.md` status summary)

### Notes

- W0 fixtures do not establish the complete endpoint-to-host projection walk.

## mere_docs/implementation_strategy/2026-09-02_physics_catalog_plan.md

- disposition: current
- status line: "in progress; P1, P1b, P2, P3, and runtime extraction landed; P4 next" — accurate: yes
- claims checked: 14 — holds: 12, stale: 1, unverifiable: 1

### Stale claims

- `DOC_README.md` still says P1 is merely ready, behind the plan's ledger. —
  evidence: `DOC_README.md:140` versus
  `mere_docs/implementation_strategy/2026-09-02_physics_catalog_plan.md:4`.

### Contradictions

- The plan and index disagree on phase state.

### Recommended action

- fix-refs (`DOC_README.md` status summary)

### Notes

- The source tree supports the laws/overlays/profile split and landed local
  phases. The historical remote-board run was not repeated.

## mere_docs/implementation_strategy/2026-09-02_platform_boundary_and_repository_topology_plan.md

- disposition: historical-marked
- status line: "Complete 2026-09-06; P0-P7 landed." — accurate: yes
- claims checked: 18 — holds: 17, stale: 0, unverifiable: 1

### Stale claims

- None. Repository state and the P7 checker support the local migration and
  audit claims.

### Contradictions

- None in the terminal ledger.

### Recommended action

- none

### Notes

- Live DNS/HTTPS was not re-probed. P7 completion does not itself close D2.

## mere_docs/research/2026-09-01_frontier_sync_over_reticulum_brief.md

- disposition: current
- status line: "Research brief with rulings." — accurate: yes
- claims checked: 10 — holds: 9, stale: 0, unverifiable: 1

### Stale claims

- None. Current Mere and Retinue boundaries agree on application-owned frontier
  sync above opaque carriage.

### Contradictions

- None.

### Recommended action

- none

### Notes

- Physical on-air behavior remains unverified. Python RNS/LXMF stays a
  clean-room black-box reference.

## mere_docs/research/2026-09-02_port_gui_composition_and_comparable_stacks_brief.md

- disposition: current
- status line: "(none)" — accurate: n/a
- claims checked: 9 — holds: 9, stale: 0, unverifiable: 0

### Stale claims

- None. The port/runtime split, Distillery first-walk choice, contribution
  surface, recipe mapping, and second-host rule agree with current companion
  documents and W0 does not invalidate them.

### Contradictions

- None.

### Recommended action

- none

### Notes

- The comparisons are rationale, not portability receipts.

## mere_docs/testing/2026-08-31_flora_tulpa_standing_receipt.md

- disposition: historical-marked
- status line: "Passed again on the stack rebased onto origin/main f2924f08; landed on main." — accurate: yes
- claims checked: 10 — holds: 9, stale: 0, unverifiable: 1

### Stale claims

- None. Code and local commit objects support the structural and compatibility
  claims.

### Contradictions

- None.

### Recommended action

- none

### Notes

- The external Fedora/remote execution is historical evidence and was not
  repeated; the receipt is not a current full-workspace gate.

## moothold_docs/implementation_strategy/2026-08-31_flora_tulpa_standing_plan.md

- disposition: historical-marked
- status line: "complete on codex/0831-integration (2026-08-31)" — accurate: no
- claims checked: 10 — holds: 9, stale: 1, unverifiable: 0

### Stale claims

- The branch-only status omits the later rebase and landing on `main`. —
  evidence: `moothold_docs/implementation_strategy/2026-08-31_flora_tulpa_standing_plan.md:3`
  versus `mere_docs/testing/2026-08-31_flora_tulpa_standing_receipt.md:3`.

### Contradictions

- Terminal repository state is stronger than the frozen branch header.

### Recommended action

- update-status

### Notes

- `moothold_docs` is a historical directory name; current product ownership is
  Gemot.

## nematic_docs/implementation_strategy/2026-05-08_polyglot_knot_design.md

- disposition: current
- status line: "Implemented 2026-05-08 / 2026-05-09; retained as design rationale and format spec." — accurate: yes
- claims checked: 9 — holds: 9, stale: 0, unverifiable: 0

### Stale claims

- None. Live `nematic.knot`, `knot/expand.rs`, Inker rendering, and current
  cross-doc references resolve; donor paths are marked historical.

### Contradictions

- None.

### Recommended action

- none

### Notes

- Live behavior, not the archived `SemanticDocument`, is authoritative.

## nematic_docs/implementation_strategy/2026-06-13_polyglot_block_resolver_plan.md

- disposition: current
- status line: "Planned." — accurate: yes
- claims checked: 8 — holds: 8, stale: 0, unverifiable: 0

### Stale claims

- None. K1, K2, and K5 are landed prerequisites; the document's own Progress
  says the unified resolver spine has no code and P0 is next.

### Contradictions

- None after separating prerequisites from this plan's implementation.

### Recommended action

- none

### Notes

- The absent source-marker default remains consumer-gated.

## nematic_docs/implementation_strategy/2026-06-27_native_smolweb_rendering_plan.md

- disposition: historical-unmarked
- status line: "planning (with Mark); net-new architecture; greenfield" — accurate: no
- claims checked: 13 — holds: 11, stale: 1, unverifiable: 1

### Stale claims

- The header says planning, while Progress says the native rendering effort is
  complete and limits the tail to Mere host integration and eyeballing. —
  evidence: `nematic_docs/implementation_strategy/2026-06-27_native_smolweb_rendering_plan.md:4`
  versus the terminal Progress entry beginning at line 250.

### Contradictions

- The planning header contradicts the terminal Progress ledger. An earlier
  “uncommitted” note is superseded later in the same ledger.

### Recommended action

- mark-historical (add banner); update-status

### Notes

- Current `crates/system/document-lanes` and Pelt glue prove the source path.
  The old headed observation was not repeated.

## nematic_docs/implementation_strategy/2026-07-01_smolweb_fidelity_plan.md

- disposition: current
- status line: "planning (with Mark)" — accurate: yes
- claims checked: 9 — holds: 8, stale: 1, unverifiable: 0

### Stale claims

- Ownership prose still names the old `smolweb-views` component as current;
  implementation now lives in `crates/system/document-lanes`. — evidence:
  `crates/system/document-lanes/src/smolweb.rs:94` → live `SmolwebDocument`.

### Contradictions

- None in phase state.

### Recommended action

- fix-refs (`smolweb-views` ownership prose)

### Notes

- The Errand AST, trust-posture, gopher regime, and app-theme judgments remain
  useful after the move.

## verso_docs/implementation_strategy/2026-06-23_genet_scrying_flipcarrier_plan.md

- disposition: historical-marked
- status line: "Design resolved; verso-api and genet donor primitives shipped; carrier and adapters next." — accurate: no
- claims checked: 10 — holds: 9, stale: 1, unverifiable: 0

### Stale claims

- “Carrier + adapters next” predates the July consolidation into
  `components/verso-tile` and is not reconciled with the banner. — evidence:
  `verso_docs/implementation_strategy/2026-06-23_genet_scrying_flipcarrier_plan.md:3-14`.

### Contradictions

- The consolidation banner and original next-step line do not form one current
  verdict.

### Recommended action

- update-status

### Notes

- The banner correctly marks the four donor crate paths historical.

## verso_docs/technical_architecture/2026-06-10_compatibility_view_charter.md

- disposition: historical-marked
- status line: "Charter decision; pre-implementation; nothing consumes this yet." — accurate: no
- claims checked: 9 — holds: 8, stale: 1, unverifiable: 0

### Stale claims

- “Pre-implementation; nothing consumes this yet” predates the first-flip work
  and consolidated `verso-tile` implementation. — evidence:
  `verso_docs/technical_architecture/2026-06-10_compatibility_view_charter.md:3-13`.

### Contradictions

- The current consolidation banner conflicts with the unchanged 2026-06-10
  status.

### Recommended action

- update-status

### Notes

- Product UI vocabulary remains “compatibility view” and “flip”; Verso is the
  crate/seam name.
