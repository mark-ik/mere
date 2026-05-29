# Donor `graphshell` Docs — Full Harvest Index (pre-archive)

**Date**: 2026-05-27
**Status**: Curated index of lasting design value across all 633 donor `design_docs/` markdown files, produced as the final sweep before the donor repo is GitHub-archived and the local clone deleted.
**Relationship to prior work**: companion to the [2026-05-17 concept-harvest brief](2026-05-17_graphshell_harvest_brief.md), which tiered ~70 *active design-bearing* docs. This sweep covers what that brief **missed or deliberately skipped**: concrete subsystem specs/invariants (vs. concepts), the `verse_docs` federation subsystem, the `verso_docs` surface-lifecycle subsystem, `matrix_docs`/`nostr_docs`, and the 212-file `archive_docs` decision/rejection record.

**How to use this**: the donor repo stays permanently browsable as the read-only archive at <https://github.com/mark-ik/graphshell>. This index is not a copy — it says *what is worth pulling, where it lives in the donor, which mere domain wants it, and why*. Pull detail from the cited donor doc when a slice lands (per [DOC_POLICY](../../DOC_POLICY.md) incremental-migration rule). Paths below are relative to `graphshell/design_docs/`.

---

## 1. Subsystem invariants & policies (adopt the contract language directly)

These are not aspirational; they define invariants that prevent silent corruption. Highest-value pulls.

| Donor doc | Item | mere home |
|---|---|---|
| `graphshell_docs/.../subsystem_history/SUBSYSTEM_HISTORY.md` | Temporal-integrity + replay-isolation + shared-projection policies (Navigator "Recent" is a read-only projection over history truth, not a second store) | `node-lineage` + `eidetic` |
| `graphshell_docs/.../SUBSYSTEM_ACCESSIBILITY.md` | Capability-declaration + non-silent-degradation + cross-surface-parity policies; every surface declares a11y capability in one place | `platen` + `inker` |
| `graphshell_docs/.../register_layer_spec.md` | Composition-over-semantics, explicit-bridge (no hidden cross-registry calls), diagnosable-routing | `register-*` cluster |
| `graphshell_docs/.../accessibility_baseline_checklist.md` + `accessibility_aa_waiver_register.md` | WCAG 2.2 A/AA conformance framework + a waiver register (owner/rationale/exit-criteria) so a11y debt is tracked, not aspirational | `platen` |

## 2. Architecture framing & graph models

| Donor doc | Item | mere home |
|---|---|---|
| `.../unified_view_model.md` | Five-domain authority model: Shell(host) → Graph(truth) → Navigator(projection) → Workbench(arrangement) → Viewer(realization); graph is primary, not workbench-embedded | spine / `kernel` |
| `.../graphlet_model.md`, `.../graphlet_projection_binding_spec.md` | Graphlet canonical definition (bounded subset + anchors + derivation rule + ranking); anchor semantics distinct from pin state; binding precedence (SelectionOverride → GraphViewOverride → GraphDefault) | `cartography` + `forme` |
| `.../2026-04-09_graph_object_classification_model.md` | Seven-class taxonomy (Address/Snapshot/Identity/Publication/Workspace/Signal/Session) with policy axes (durability/mutability/retention/authorship/provenance/shareability) | `kernel` |
| `.../node_object_query_model.md` | Node as queryable hub; *viewing* vs *examining*; examination surface as projection over node-adjacent families | `kernel` + `eidetic` |
| `.../2026-04-11_core_intent_inventory.md`, `.../command_semantics_matrix.md` | Intent classification (Direct/Translated/Host-only); **Undoable / SoftUndoable / NotUndoable** is structural (governs undo-stack + UI rollback), not just audit | `kernel` mutation bus |

## 3. Rendering, visual encoding, viewers

| Donor doc | Item | mere home |
|---|---|---|
| `.../frame_assembly_and_compositor_spec.md` | Three-pass composition (Chrome → Content → Overlay); four render modes (CompositedTexture/NativeOverlay/EmbeddedHost/Placeholder); GL-state-isolation invariant; visible-nav-geometry vs logical-rect | `inker` + `platen` |
| `.../canvas_behavior_contract.md` | Physics **scenario contracts** (KE convergence, overlap count, component separation) + readability metrics (`crossing_density`, `label_overlap_ratio`, `edge_len_cv`) as measurable acceptance criteria | `cartography` + `graph-canvas` |
| `.../edge_visual_encoding_spec.md` | Per-relation-family stroke/color/width/opacity/directionality; theme-token-bound, not hardcoded | `cartography` |
| `.../theme_and_tokens_spec.md` | Token theming with five-scope spine (default → persona → graph → view/tile → pane); libcosmic compatibility path | `inker` + `platen` |
| `.../viewer_presentation_and_fallback_spec.md` | Viewer classes + render-mode resolution from `ViewerRegistry` + overlay-affordance-by-mode + loading/partial/deferred state contracts | `inker` (+ `register-viewer`) |

## 4. Focus, events, interaction, UxTree

| Donor doc | Item | mere home |
|---|---|---|
| `.../focus_and_region_navigation_spec.md`, `.../focus_state_machine_spec.md` | Nine-region canonical focus model + `build_runtime_focus_state` priority resolver + capture-stack semantics (survives modal/pane lifecycle) | `verso` + `platen` |
| `.../semantic_event_pipeline_spec.md` | Pure-translation boundary: `GraphSemanticEvent → RuntimeEvent` with zero side effects; "responsive webview set" semantics. Load-bearing for scrying webview embedding | `verso` |
| `.../navigator_interaction_contract.md` | Row-type-specific click grammar (Node: select/navigate; Frame/Tile: expand/collapse); cold members always visible | `cartography` + `platen` |
| `.../ux_tree_and_probe_spec.md` | Three-layer `UxNode` (semantic / presentation / trace) + build-order invariant + per-frame snapshot rebuild (not incremental) + graph LOD point-to-compact cutoff | `verso` (AccessKit bridge) |

## 5. Layout, workbench, settings/permissions

| Donor doc | Item | mere home |
|---|---|---|
| `.../workbench_layout_policy_spec.md` | `WorkbenchLayoutConstraint` (anchored splits, cross-axis margins, overlay-occluded nav geometry) that survives frame navigation | `forme` |
| `.../settings_and_permissions_spine_spec.md` | Five-scope hierarchy (default → persona → graph → view/tile → pane) + **permission-narrowing rule** (narrower scope can only narrow inherited permission) | `kernel` + `verso` |
| `.../workbench_profile_and_workflow_composition_spec.md` | Composition domains as *separate* persistence scopes (interaction prefs / pane defaults / command-surface / navigator-host / workflow presets) with a resolution chain | `forme` |
| `.../layout_behaviors_and_physics_spec.md` | Degree-repulsion + domain-clustering + frame-affinity forces; `FamilyPhysicsPolicy` (semantic/traversal/containment/arrangement weights, composable not replaceable) | `cartography` (pairs with the just-pulled `register-lens` presets) |
| `.../projection_runtime_lifecycle_spec.md` | ProjectionDescriptor vs RuntimeInstance; invalidation families (SourceTruthChanged/ScopeChanged/SelectorChanged/AnchorSetChanged/…); linked-vs-unlinked refresh contract | `cartography` + `eidetic` |

## 6. Network, GPU, mods, intelligence

| Donor doc | Item | mere home |
|---|---|---|
| `.../graphshell_net_spec.md` | Single outbound-network policy layer (downloads/uploads/prefetch/DNS/push/provider/agent) with per-request scope-path tracking + rate limit/backpressure + audit surface | `murm` / netfetcher |
| `.../2026-04-20_graphshell_gpu_spec.md` (§5.1, §7.1) | Separate render + compute devices (avoid render starvation); renderer-fork texture-source integration (zero-copy when device shared, explicit sync otherwise) | scrying |
| `.../mod_lifecycle_integrity_spec.md` | Six-phase mod lifecycle (Discovery→Admission→Resolution→Activation→Operation→Unload/Rollback/Quarantine); rollback-first failure; quarantine on incomplete rollback | `register-mod-loader` |
| `.../intelligence_taxonomy.md` | Four axes (mechanism / output / scope / autonomy) to prevent "AI" bucket-collapse | `intel` / `moothold` |

## 7. Verse — federation/contribution subsystem (never harvested; ~70% transfers)

Verse treated federation as a **governance problem first**. The networking (iroh/libp2p/CIDv1) is sound but secondary; the durable value is the governance + artifact-standard model. Maps onto mere's `murm` (bilateral) + `moothold`/`mooting` (community) + `persona` (identity) + `eidetic` (memory).

| Donor doc | Concept | mere home |
|---|---|---|
| `verse_docs/.../2026-04-17_verse_graph_contribution_protocol_v0_1.md` | **VGCP**: Entry/Visit/Owner projection boundary; **structural (verifiable-by-fetch) edges, not behavioral**; canonicalization profiles per protocol; BLAKE3+CIDv1; **privacy-filter-before-sign** ordering | `kernel` + `murm` |
| same | Governance: Ed25519→did:key identity; three rule systems (**Genesis / Threshold / Delegated**); revocation-as-read-time-projection (sign immutable, filter on read) | `persona` + `moothold`/`mooting` |
| `verse_docs/.../community_governance_spec.md` | Role separation (Member/Contributor/Reviewer/Moderator/Curator/Treasurer/Operator); operational authority ≠ governance authority; append-only moderation logs | `mooting` |
| `verse_docs/.../engram_spec.md` | `TransferProfile` envelope: validation-class taxonomy (LocalPrivate→…→ArchiveOnly), redaction profile, memory-as-reference, lineage link | `eidetic` |
| `verse_docs/.../proof_of_access_ledger_spec.md` | Reputation ledger (not a blockchain): work-type receipts, epoch batching, receipts-as-evidence-not-money | `kernel` (trust graph) |
| `verse_docs/.../2026-03-28_decentralized_storage_bank_spec.md` | Community storage: 3-role model, availability challenges, usage-validates-storage credit | `kernel` + `murm` |
| `verse_docs/.../lineage_dag_spec.md` | Multi-parent ancestry; **DAG (what influenced me) + stream (what authority X published in order)** separation; trust filtering on traversal | `kernel` + `eidetic` |
| `verse_docs/.../2026-02-23_verse_tier2_architecture.md` | One Ed25519 secret → both iroh NodeId and libp2p PeerId (no re-pairing); VerseBlob size-class transport rules | `persona` + `murm` |
| `verse_docs/.../2026-02-28_verse_concrete_research_agenda.md` | Hardware-tier taxonomy **T-Min (CPU 8GB) / T-Mid (iGPU 16GB) / T-Full (dGPU 32GB)** for federation feasibility | federation feasibility |

Graphshell-specific, does **not** transfer: UDC as the mandated tag system, LoRA/Burn ML stack specifics, Nostr/DVM payment, report-token tokenomics, Matrix-as-comms-applet. (Trust-token naming note: [[project_tessera_trust_token]].)

## 8. Verso — surface-lifecycle subsystem (direct predecessor to mere `verso-core`/`tile-state`)

Living architectural ancestor, not obsolete. Travels with the codebase.

| Donor doc | Item | mere home |
|---|---|---|
| `verso_docs/VERSO_AS_PEER.md`, `VERSO_SERVO_ARCHITECTURE.md` | Surface-lifecycle identity (Ed25519 holder); ViewerRegistry/ProtocolRegistry contracts; "Servo dormant unless active" boundary | `verso-core` |
| `verso_docs/.../2026-02-23_verse_tier1_sync_plan.md` | Bilateral sync: VersionVector deltas, SyncUnit, SyncWorker, conflict resolution (LWW + ghost nodes), SyncLog persistence | `tile-state` sync engine |
| `verso_docs/.../2026-02-25_verse_presence_plan.md` | Presence as ephemeral overlay on a separate QUIC substream (ghost cursors, follow mode), rate-limited | `verso-core` presence |
| `verso_docs/.../2026-03-27_session_capsule_ledger_plan.md` | `SessionCapsule`: portable CID-addressed encrypted snapshot + `ArchivePrivacyClass` | `tile-state` archive |
| `verso_docs/.../2026-03-28_gemini_capsule_server_plan.md` | Publishing inversion: graph → SimpleDocument → protocol format; route privacy classes (reusable for Atom/RSS) | `verso-core` (+ `nematic`) |
| `verso_docs/.../2026-03-28_rss_feed_graph_model.md` | Feed as a living graphlet anchor: slow emission, user harvest, ghost-proxy chain | `nematic` + `cartography` |
| `verso_docs/.../2026-03-28_cable_coop_minichat_spec.md` | Cable protocol chat substrate; per-cabal keypair; ephemeral-vs-persistent modes | `murm` |
| `verso_docs/.../permacomputing_alignment.md`, `.../smolnet_follow_on_audit.md` | Protocol **admission bar** (user-felt benefit / stable family / clear trust / small surface / credible ecosystem); Gemini primary, Gopher secondary, Titan next; visible-seams design values | `nematic`/`verso` governance |

Caveat: `simple_document_engine_target_spec.md`'s `SimpleDocument` model is superseded by `inker::EngineDocument` / nematic; treat as historical.

## 9. Matrix (conditional) & Nostr (discard)

- **matrix_docs (5)** — archival unless `moothold` explicitly adopts Matrix room federation. Keep only the patterns: the M1–M7 phased-adoption template, the explicit non-goals ("Matrix is the room layer, not a replacement for bilateral/community/identity"), the three-identity binding model, and the observe-only / projectable / intent-capable event classification (only host-authored namespaced events may propose graph intents). Discard protocol-specific detail.
- **nostr_docs (5)** — **discard.** nostr is a dropped lane. The only reusable fragment is the generic mod-capability contract (deny-by-default, manifest-declared, runtime-enforced, no raw-secret export), which `persona`/mod-governance can adopt without the nostr specifics.

## 10. archive_docs — decisions & rejections worth keeping (the "don't re-litigate" record)

212 superseded checkpoint files (Jan–Apr 2026); ~25–30 carry durable value, the rest are status/progress noise. The arc: egui-dominant servoshell fork → authority inversion → arrangement/physics extraction → Xilem-host. Worth recording:

**Explicitly considered and rejected** (with rationale — keep so they aren't re-litigated): egui_graphs as the physics home (coupled mobile code to egui); large fixed webview pools (wasteful vs Servo origin-grouping + small LRU); WebRender-native MVP graph UI (complexity > benefit); **tile-tree as arrangement authority** (fragments graphlet membership — graph edges + lifecycle are the truth); "active webview" global state (conflates focus/chrome-source/render-target); fixed-schedule physics (couples dt to frame rate).

**Durable specs that are still load-bearing** (origin record): `Address` typed enum + `PersistedAddress` canonicalization; graphlet membership split (**durable** `UserGrouped`/`FrameMember` vs **circumstantial** `Hyperlink`/`History`); node lifecycle `Active`/`Warm`/`Cold`; `Layout<N>` + `LayoutExtras` trait abstraction (physics mobility); the **authority split** (graph = membership truth, workspace/FrameSnapshot = presentation/restore only); authority inversion (graph accepts, host reconciles). Primary sources: `archive_docs/checkpoint_2026-03-20/Arrangement_Graph_Projection_Plan.md`, `checkpoint_2026-03-22/Servoshell_Debtclear_Plan.md`, `checkpoint_2026-04-18/egui_graphs_Retirement_Plan.md`, `checkpoint_2026-02-01/ARCHITECTURE_DECISIONS.md`.

---

## Safely discardable (no pull)

Host-coupled to dropped tech (egui/iced/gpui/Wasmtime migration plans, egui_tiles decoupling, Servo/WebRender internals), completed-migration scaffolding (validation gates, coverage registers, lifecycle audits), pure research agendas (problem inventories, not decisions), planning artifacts (PLANNING_REGISTER, progress logs), and the bulk of `archive_docs` status logging. All remain in the GitHub archive if ever needed.
