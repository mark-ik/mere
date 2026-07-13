# Graphshell harvest brief — spatial hypertext + multiplexer + Xilem

**Date**: 2026-05-17
**Status**: Research brief. Concept inventory pulled from the donor graphshell `design_docs/` for adoption into the active Mere spatial-chrome / multiplexer / Xilem-host lane. Decides nothing about adoption sequencing — that is the spatial-chrome adoption plan's job ([`../implementation_strategy/2026-05-15_spatial_chrome_modular_adoption_plan.md`](../implementation_strategy/2026-05-15_spatial_chrome_modular_adoption_plan.md)).
**Scope**: Concepts only, with source citations. Five concurrent surveys covered ~70 graphshell docs across `design/`, `implementation_strategy/` (aspects + subsystems + workspace), `research/`, and `technical_architecture/`. Items mentioned independently by ≥2 surveys are flagged Tier 1.

**Source repo**: [`../../../../graphshell/design_docs/`](../../../../graphshell/design_docs/) (donor; per project memory `project_graphshell_donor_not_authority`, treat as grab-bag, never as prescriptive).

> **Update note (2026-06-09 audit):** the donor source repo is now GitHub-archived (read-only; local clone deleted 2026-05-27), so the `../../../../graphshell/design_docs/` path no longer resolves. The "Xilem-host lane" framing is stale (the host is now `meerkat` on genet-as-host), and the spatial-chrome adoption plan this brief cites was archived under `archive_docs/2026-06-09_pivot_superseded/` (the register-renderer/Masonry pivot). This remains the canonical donor concept-index per DOC_POLICY.

---

## Framing

Graphshell predates three of Mere's load-bearing architectural decisions:

1. **Host framework**: Mere is on Xilem (per project memory `project_host_framework_glass_gpui`). Graphshell evaluated egui / iced / gpui / blitz and committed to none of these in production. Many of its renderer/host docs evaluate frameworks Mere has already moved past.
2. **Renderer registry**: Mere's `NodeRenderer` / composition-mode contract is canonical (see [`renderer registry contract brief`](2026-05-15_renderer_registry_contract_brief.md)). Graphshell's equivalent ideas appear as `ViewerRegistry`, `TileRenderMode`, presentation-provider scoring — useful as confirmation, not as a replacement contract.
3. **Multiplexer**: Window = graph-shaped session; panels carry `graph_id`; tearable per the [`tearout operations brief`](2026-05-11_tearout_operations_brief.md) and [`browser multiplexer framing`](2026-05-11_browser_multiplexer_framing.md). Graphshell's "shell layer" predates this and is more pane-tree-centric.

Concepts below are evaluated against this *current* Mere shape, not against where graphshell was when each doc was written. Citations are to the donor location; Mere's spatial-chrome plan and renderer-registry brief have already absorbed the bulk of the renderer/multiplexer framing.

---

## Tier 1 — cross-survey signal

These appeared independently in multiple surveys. Treat as architectural anchors.

### T1-1. Single durable write-path / Intent as sole mutation carrier

All durable graph mutations flow through one reducer entry (`apply_reducer_intents()`); direct mutations raise diagnostics. Enforced compile-time via `pub(crate)` boundary on the graph mutators; runtime via the `INV-1` invariant.

- Sources: [`graphshell core-interaction-model-plan.md:63`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/core-interaction-model-plan.md#L63), [`graphshell SUBSYSTEM_STORAGE.md:153`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/subsystem_storage/SUBSYSTEM_STORAGE.md#L153).
- Applies to: mere-kernel mutation API; the typed action bus ([`../implementation_strategy/2026-05-11_typed_action_bus_plan.md`](../implementation_strategy/2026-05-11_typed_action_bus_plan.md)) is the natural carrier for the "sole intent path" invariant. Edge-mutation work in the [relation taxonomy plan](../implementation_strategy/2026-05-11_relation_taxonomy_and_edge_mutation_plan.md) already routes through bus actions; making `pub(crate)` the type-level enforcement makes the convention non-optional.
- Suggested Mere term: keep as is — single-write-path invariant.

### T1-2. Three-tier composition pass order (Chrome → Content → Overlay)

Strict layering on the spatial canvas: chrome paints first, node content second, overlays (focus rings, lasso, tooltips, drag preview) last. Prevents Z-fighting and chrome occlusion under a free-zoom camera where overlay Z is otherwise camera-relative.

- Sources: [`graphshell ARCHITECTURAL_OVERVIEW.md:98`](../../../../graphshell/design_docs/graphshell_docs/technical_architecture/ARCHITECTURAL_OVERVIEW.md#L98), [`graphshell ASPECT_RENDER.md:35`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/aspect_render/ASPECT_RENDER.md#L35).
- Applies to: mere-spatial-prototype's `SubstrateScene::paint_scene` — currently flat (no enforced overlay-last ordering). Spatial-chrome plan Phase 3 done conditions don't cover overlay ordering yet.
- Suggested Mere term: `CompositionPassOrder` invariant on `SubstrateScene`.

### T1-3. Schema-first diagnostics with watchdog invariants

`ChannelRegistry` makes channel descriptors (schema, severity, retention) declarative; live analyzers consume the event stream separately. Long-running operations emit paired `*_started` / `*_succeeded | *_failed` events with timeout contracts — surfaces hangs vs. silent failures.

- Sources: [`graphshell SUBSYSTEM_DIAGNOSTICS.md:88`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/subsystem_diagnostics/SUBSYSTEM_DIAGNOSTICS.md#L88), [`graphshell SUBSYSTEM_DIAGNOSTICS.md:115`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/subsystem_diagnostics/SUBSYSTEM_DIAGNOSTICS.md#L115), [`graphshell core-interaction-model-plan.md:354`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/core-interaction-model-plan.md#L354).
- Applies to: spatial-chrome plan Phase 2 already lists `renderer.registered`, `engine.route_chosen`, etc. — but does not yet name a started/succeeded/failed convention. Engine profile boundary work and the worker runner have the same pattern need.
- Suggested Mere term: keep `started/succeeded/failed` naming; channel descriptors in `mere-host-runtime::diagnostics`.

### T1-4. Capability/envelope degradation as a single policy contract

One product model across hosts. Each envelope (desktop full / browser degraded / mobile constrained / headless) declares per-capability state: full / degraded / unavailable. Renderer selectors and feature surfaces consult the same contract — no per-feature bespoke "is this supported here" checks.

- Sources: [`graphshell browser_envelope_coop_and_degradation_policy.md:86`](../../../../graphshell/design_docs/graphshell_docs/technical_architecture/2026-04-09_browser_envelope_coop_and_degradation_policy.md#L86), [`graphshell SUBSYSTEM_SECURITY.md:94`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/subsystem_security/SUBSYSTEM_SECURITY.md#L94).
- Applies to: spatial-chrome plan Phase 5 (Browser/PWA envelope) — replaces hand-rolled supported/degraded/unavailable lists with `EnvelopeCapabilityProfile`. Composes with the existing [capability gate catalogue](2026-05-14_capability_gate_catalogue_brief.md): envelope is the *outer* layer (what is even physically available); capability gate is the *inner* layer (which gated actions an actor may perform within that envelope).
- Suggested Mere term: `EnvelopeCapabilityProfile`.

### T1-5. Projection descriptors with invalidation rules + linked/unlinked binding mode

Versioned, contracted carriers for cross-domain derived representations (graphlets, summaries, correspondences). Each descriptor declares its refresh rule and invalidation triggers. The linked-vs-unlinked distinction is the policy axis: a *linked* projection re-renders when its source mutates; an *unlinked* projection is a snapshot at capture time and does not.

- Source: [`graphshell ASPECT_PROJECTION.md:38`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/aspect_projection/ASPECT_PROJECTION.md#L38), [`graphshell ASPECT_PROJECTION.md:43`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/aspect_projection/ASPECT_PROJECTION.md#L43).
- Applies to: [cartography brief](2026-05-10_cartography_layer_brief.md)'s `Projection` carrier (currently lacks explicit invalidation contract) and [view-intent sidecar plan](../implementation_strategy/2026-05-14_view_intent_sidecar_plan.md) (currently treats every view as live-bound implicitly). Switcher thumbnails are de facto *unlinked* snapshots — making the binding mode explicit catches subtle bugs (e.g. should an open switcher refresh when its captured graph mutates? today: ambiguous).
- Suggested Mere term: `ProjectionBindingMode { Linked, Unlinked }` on `Projection`.

---

## Tier 2 — unique high-value pulls

### T2-1. Scene mode escalation (Browse / Arrange / Simulate)

Same graph supports three modes via lens-switching + physics profile, not three products. Browse is calm/discovery, Arrange is spatial authoring, Simulate runs nodes as physical objects.

- Source: [`graphshell scene_mode_ux_sketch.md:49`](../../../../graphshell/design_docs/graphshell_docs/research/2026-04-02_scene_mode_ux_sketch.md#L49).
- Applies to: cartography's `FormFactor` enum + `LayoutStrategy` trait can generalize the lens; physics profile is already a cartography concept (per the [graph canvas field algebra plan](../../graphshell_docs/implementation_strategy/2026-05-07_graph_canvas_field_algebra_plan.md)). View intent sidecar grows a `mode: SceneMode` field.

### T2-2. Universal Node Content Model — NodeId (UUID) decoupled from address

Identity (UUID) separate from address (URL / file path / Onion / IPFS / Nostr event ID / ...). One node can carry multiple addresses over time and across protocols; address class is a typed property, not a primary key.

- Source: [`graphshell universal_node_content_model.md:28`](../../../../graphshell/design_docs/graphshell_docs/technical_architecture/2026-02-18_universal_node_content_model.md#L28).
- Applies to: significant architectural decision — current `Node` model is address-keyed. Implications: file-system browsing as graph nodes; protocol-flexible content snapshots; cleaner person-node identity convergence (T2-7). Mere-kernel would need `NodeId` (UUID) as primary key separate from address. Not adopt-and-go; warrants its own design pass.
- Suggested Mere term: keep `NodeId` (already in use) — clarify that it is *not* the address.

### T2-3. Seven-way Graph Object Classification

Distinguish Address / Snapshot / Identity / Publication / Workspace / Signal / Session objects at the model level, parameterized by durability, mutability, retention, authorship, provenance, shareability.

- Source: [`graphshell graph_object_classification_model.md:59`](../../../../graphshell/design_docs/graphshell_docs/technical_architecture/2026-04-09_graph_object_classification_model.md#L59).
- Applies to: grounds edge-mutation policy (relation taxonomy plan §6.2 hide-only vs. retract: who can retract what?) and Moothold sync policy (what crosses the federation boundary?) and export rules. Likely consumes T2-2's identity/address split.

### T2-4. Signal Families — multi-lane graph enrichment

Keep ingestion signals stratified by lane (Discovery / Freshness / Clustering / Search / Neighborhood) rather than collapsing into one ranking score. Provenance per lane survives all the way to the renderer.

- Source: [`graphshell smolweb_discovery_and_aggregation_signal_model.md:50`](../../../../graphshell/design_docs/graphshell_docs/research/2026-04-09_smolweb_discovery_and_aggregation_signal_model.md#L50).
- Applies to: cartography's `IntelligenceSignals` carrier; future Mere ingestion (smolweb feeds, recommendation streams, Moothold flora).
- Suggested Mere term: `SignalFamily::{ Discovery, Freshness, Clustering, Search, NeighborhoodTraversal }`.

### T2-5. Hot/cold tiered traversal archive

Bounded hot in-memory traversal log + fjall cold tier per edge, with O(1) count for visual edge-weight overlay. Storage proof for the `Traversal` relation family.

- Source: [`graphshell edge_traversal_model_research.md:377`](../../../../graphshell/design_docs/graphshell_docs/research/2026-02-20_edge_traversal_model_research.md#L377).
- Applies to: relation taxonomy plan's `Traversal` family currently has no storage substrate. `EdgePayload` carries hot-tier `Vec<Traversal>` plus `archived_traversal_count: u64` metadata; cold tier under fjall keyspace per the [short-term memory substrate plan](../implementation_strategy/2026-05-14_short_term_memory_substrate_plan.md) precedent.

### T2-6. UxTree as canonical semantic tree with path-stable IDs

Machine-readable projection of the live UI with stable IDs derived from app identity (`NodeKey`, `GraphViewId`) — not pointer-based, not frame-index-based. Unblocks snapshot regression testing and AccessKit bridging.

- Sources: [`graphshell SUBSYSTEM_UX_SEMANTICS.md:192`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/subsystem_ux_semantics/SUBSYSTEM_UX_SEMANTICS.md#L192), [`graphshell SUBSYSTEM_UX_SEMANTICS.md:206`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/subsystem_ux_semantics/SUBSYSTEM_UX_SEMANTICS.md#L206).
- Applies to: spatial-chrome plan Phase 3 has UxTree/AccessKit projection ✅ at substrate level but not for renderer-contributed sub-trees (e.g. `MasonryTile::take_accesskit_update`). Graphshell's spec is the contract to fill that hole. Composes with the [xilem embedding spike](2026-05-15_xilem_embedding_spike.md) — masonry's `accesskit::TreeUpdate` output is already path-stable in spirit; the missing piece is the merge into substrate uxtree.

### T2-7. Six focus tracks (region / pane / graph-view / widget / embedded-content / capture)

Separate tracks rather than one monolithic focus state. Cross-track handoff rules + deterministic return-path policy (parent / sibling / root) when the focused element disappears.

- Source: [`graphshell SUBSYSTEM_FOCUS.md:56`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/subsystem_focus/SUBSYSTEM_FOCUS.md#L56).
- Applies to: Xilem-host with embedded webviews (`scrying.web`, `wry.web`) and panel widgets (mere-domain) — focus is unavoidably multi-track. Composes with [OS plumbing audit](2026-05-15_os_plumbing_reuse_audit_brief.md) §2.3 (focus + keyboard navigation): the audit notes substrate-shaped focus is more natural than tree-shaped; six tracks gives the substrate a structure to be natural *in*.

### T2-8. Shared GPU resource authority topology

`graphshell-gpu` spine: one authority owns device / queue / adapter; many consumers borrow. Texture bridge contract specifies zero-copy when sharing device, explicit sync otherwise. Separate render and compute devices to avoid render starvation under intelligence-signal compute.

- Source: [`graphshell graphshell_gpu_spec.md:60`](../../../../graphshell/design_docs/graphshell_docs/technical_architecture/2026-04-20_graphshell_gpu_spec.md#L60).
- Applies to: spatial-chrome plan Phase 4 ("designate one low-level interop crate"). The §5.1 (separate render/compute), §5.3 (texture-source opacity), §7.1 (renderer fork integration pattern) sections are direct input to whichever crate ends up canonical (`scrying::native_frame` is the current designated home per the spatial-chrome plan implementation status).

### T2-9. Frame scheduler with priority queue

User-input paint ≫ animation ≫ background work, not free-running per-tenant. Prevents subsystem starvation when NetRender composition is driving multiple embedded-frame renderers (Genet + scrying) concurrently with intelligence-signal compute.

- Source: [`graphshell graphshell_gpu_spec.md:299`](../../../../graphshell/design_docs/graphshell_docs/technical_architecture/2026-04-20_graphshell_gpu_spec.md#L299).
- Applies to: spatial-chrome plan Phase 3 done conditions don't yet specify scheduling; under single-tenant test data the prototype works fine, but Phase 4 (multi-renderer interop) needs this before stress testing.

### T2-10. Registry-as-extension-seam discipline (four-property test)

A registry has: keyed namespace, entry trait/value, lookup API, late-binding extension. Graphshell audited ~20 candidates and concluded ~12 were real registries; the rest were dispatchers, state, or integration code wearing registry costumes.

- Source: [`graphshell workspace_architecture_proposal.md:189`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/2026-05-01_workspace_architecture_proposal.md#L189).
- Applies to: Mere already has `mere-renderer-registry` and engine registry in inker; future extensions (mods, intelligence providers, capability gates) should pass the four-property test before getting their own crate. Cheap guardrail.

---

## Tier 3 — also valuable (one-liners with source)

- **Disabled-action visibility with precondition tooltips** ([`graphshell surface_behavior_spec.md:188`](../../../../graphshell/design_docs/graphshell_docs/design/surface_behavior_spec.md#L188)) — palette/context menu surfaces should expose disabled state with actionable help text, not just hide.
- **Click hierarchy with semantic intent tiers** ([`graphshell command_semantics_matrix.md:159`](../../../../graphshell/design_docs/graphshell_docs/design/command_semantics_matrix.md#L159)) — left/double/right/drag as distinct intent tiers; pane-UX brief already adopts this informally.
- **Context-aware action visibility per surface type** ([`graphshell command_semantics_matrix.md:126`](../../../../graphshell/design_docs/graphshell_docs/design/command_semantics_matrix.md#L126)) — node menu ≠ edge menu ≠ canvas menu ≠ pane-header menu; renderer registry can publish each.
- **Precondition-driven undo classification** ([`graphshell command_semantics_matrix.md:84`](../../../../graphshell/design_docs/graphshell_docs/design/command_semantics_matrix.md#L84)) — Undoable / SoftUndoable / NotUndoable on bus actions; separates graph-truth undo from presentation undo.
- **Empty/loading/error state contracts per surface class** ([`graphshell surface_behavior_spec.md:110`](../../../../graphshell/design_docs/graphshell_docs/design/surface_behavior_spec.md#L110)) — each NodeRenderer declares contract compliance.
- **Ambient liveness metrics (gravity wells, decay, pulse)** ([`graphshell ambient_graph_visual_effects.md:34`](../../../../graphshell/design_docs/graphshell_docs/research/2026-03-27_ambient_graph_visual_effects.md#L34), [`graphshell graph_interaction_brainstorm.md:20`](../../../../graphshell/design_docs/graphshell_docs/research/2026-03-29_graph_interaction_brainstorm.md#L20)) — node warmth from session-level activity; renderer registry consumes as aesthetic hint.
- **Semantic navigation grammar (Back follows traversal edges, Focus expands neighborhood)** ([`graphshell interaction_and_semantic_design_schemes.md:80`](../../../../graphshell/design_docs/graphshell_docs/research/2026-02-24_interaction_and_semantic_design_schemes.md#L80)) — "Back" is not tab history; it's the inverse of the last traversal.
- **Chord/sequence input primitives** ([`graphshell ASPECT_INPUT.md:37`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/aspect_input/ASPECT_INPUT.md#L37)) — first-class above flat key binding; useful for power-user gesture vocabulary.
- **Command-context-rank policy** ([`graphshell ASPECT_COMMAND.md:35`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/aspect_command/ASPECT_COMMAND.md#L35)) — category priority by summon context, shared across palette/menu/radial surfaces.
- **Tool-dismissal return-path contract** ([`graphshell ASPECT_CONTROL.md:49`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/aspect_control/ASPECT_CONTROL.md#L49)) — composes with T2-7 (focus tracks).
- **Identity convergence — Person nodes aggregate WebFinger / NIP-05 / MXID / ActivityPub / Misfin / Gemini endpoints as typed claims** ([`graphshell identity_convergence_and_person_node_model.md:64`](../../../../graphshell/design_docs/graphshell_docs/technical_architecture/2026-04-09_identity_convergence_and_person_node_model.md#L64)) — relevant once Murm/Moothold land.
- **`verso://` URI category formalization** ([`graphshell graphshell_verse_uri_scheme.md:20`](../../../../graphshell/design_docs/graphshell_docs/technical_architecture/2026-04-09_graphshell_verse_uri_scheme.md#L20)) — reserved categories (session/room/community/workspace/publication) align with Mere tier framework.
- **Edge dominance direction via traversal frequency** ([`graphshell edge_traversal_model_research.md:177`](../../../../graphshell/design_docs/graphshell_docs/research/2026-02-20_edge_traversal_model_research.md#L177)) — undirected-edge visual weight from accumulated traversals (>60% threshold); cartography overlay candidate.
- **Two-authority model (Graph Reducer + Workbench Authority)** ([`graphshell core-interaction-model-plan.md:320`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/core-interaction-model-plan.md#L320)) — Mere already separates these de facto; naming them prevents authority creep.
- **Lane architecture *concept* (one semantic doc → multiple render strategies)** ([`graphshell middlenet_lane_architecture_spec.md:99`](../../../../graphshell/design_docs/graphshell_docs/technical_architecture/2026-04-16_middlenet_lane_architecture_spec.md#L99)) — the concept stays; graphshell's HTML/Servo/Direct lane *split* doesn't (Mere's renderer registry composition modes have replaced it).
- **Single-write-path enforcement via `pub(crate)` boundary** ([`graphshell SUBSYSTEM_STORAGE.md:153`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/subsystem_storage/SUBSYSTEM_STORAGE.md#L153)) — type-level enforcement of T1-1.
- **WAL-first integrity with sequence-monotonicity gap detection** ([`graphshell SUBSYSTEM_STORAGE.md:128`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/subsystem_storage/SUBSYSTEM_STORAGE.md#L128)) — composes with event-DAG substrate.
- **Manifest-integrity pre-activation for mods** ([`graphshell SUBSYSTEM_MODS.md:59`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/subsystem_mods/SUBSYSTEM_MODS.md#L59)) — provides/requires validation before registry contribution accepted.
- **Default-deny capability matrix** ([`graphshell SUBSYSTEM_SECURITY.md:33`](../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/subsystem_security/SUBSYSTEM_SECURITY.md#L33)) — composes with [capability gate catalogue](2026-05-14_capability_gate_catalogue_brief.md).
- **Presentation-provider capability scoring (fidelity / interaction / geometry / subsystem_conformance)** ([`graphshell presentation_provider_and_ai_orchestration.md:38`](../../../../graphshell/design_docs/graphshell_docs/technical_architecture/2026-02-27_presentation_provider_and_ai_orchestration.md#L38)) — enriches `RendererCapabilities` beyond boolean flags.

---

## Already in Mere (no re-harvest needed)

The renderer/multiplexer/session work has already absorbed the bulk of graphshell's framing. Concepts already present in Mere docs:

- Renderer registry contract + composition modes — [renderer registry brief](2026-05-15_renderer_registry_contract_brief.md).
- Browser taxonomy mapping — [browser taxonomy translation brief](2026-05-15_browser_taxonomy_translation_brief.md).
- Native texture interop framing — spatial-chrome plan Phase 4.
- Browser/PWA envelope (concept; T1-4 above sharpens the contract) — spatial-chrome plan Phase 5.
- OS-plumbing audit — [OS plumbing reuse audit](2026-05-15_os_plumbing_reuse_audit_brief.md).
- Capability gates with persona / session / app layers — [capability gate catalogue](2026-05-14_capability_gate_catalogue_brief.md).
- Engine profile binding — [engine profile boundary plan](../implementation_strategy/2026-05-14_engine_profile_boundary_plan.md).
- Event-DAG substrate — [event DAG substrate brief](../implementation_strategy/2026-05-07_event_dag_substrate_brief.md).
- Session manifest + tear-out — [browser multiplexer framing](2026-05-11_browser_multiplexer_framing.md), [tearout operations brief](2026-05-11_tearout_operations_brief.md).
- View intent sidecar — [view intent sidecar plan](../implementation_strategy/2026-05-14_view_intent_sidecar_plan.md).
- Switcher thumbnails — [switcher thumbnails plan](../implementation_strategy/2026-05-14_switcher_thumbnails_plan.md).
- Short-term memory substrate — [short-term memory substrate plan](../implementation_strategy/2026-05-14_short_term_memory_substrate_plan.md).
- Persona model — [persona model brief](2026-05-14_persona_model_brief.md).
- Cartography projection layer — [cartography layer brief](2026-05-10_cartography_layer_brief.md).
- Relation taxonomy + edge mutation — [relation taxonomy and edge mutation plan](../implementation_strategy/2026-05-11_relation_taxonomy_and_edge_mutation_plan.md).
- Graph canvas field algebra — [graph canvas field algebra plan](../../graphshell_docs/implementation_strategy/2026-05-07_graph_canvas_field_algebra_plan.md) (already promoted into Mere's `graphshell_docs/`).
- Typed action bus — [typed action bus plan](../implementation_strategy/2026-05-11_typed_action_bus_plan.md).

---

## Skipped (explicitly out of scope)

Cluster-level reasons for non-adoption:

- **Egui-specific anything**: `egui_graphs` StableGraph coupling, MetadataFrame-based zoom, `egui_tiles` tree composition. Mere host is Xilem; chrome composition is substrate + masonry.
- **Iced renderer-boot options and Iced chrome migration vision**: superseded by the Xilem decision (per project memory `project_host_framework_glass_gpui`).
- **GPUI-coupled infrastructure**: `Entity<T>`-shaped reactive runtime, gpui platform integration. The [xilem embedding spike](2026-05-15_xilem_embedding_spike.md) resolves the reactive-runtime question via `xilem_core` + `masonry_core`.
- **Servoshell webview-id identity scheme and Servo internals coupling**: genet is now a peer renderer tenant (`genet.web`), not the host's child surface.
- **Graphshell's HTML / Servo / Direct lane *split***: replaced by renderer-registry composition modes. The *concept* (lane architecture) survives.
- **Verse P2P protocol internals**: identity model reusable (T2-7, Tier 3 person-node aggregation); protocol implementation not.
- **Wasmtime scripting**: Mere uses Rhai (per project memory `project_browser_pwa_shapes_scripting`).
- **Verso/verse retired terminology**: see [lexicon brief](../../2026-05-04_lexicon_brief.md) for the post-rename vocabulary. The donor docs still use these terms; translate on read.

---

## Recommended next moves

Three options, ordered by load-bearing impact on active work. None of these are blockers; this brief itself was the cheap-and-loss-tolerant preservation move.

1. **Amend the [spatial-chrome adoption plan](../implementation_strategy/2026-05-15_spatial_chrome_modular_adoption_plan.md)** to absorb Tier 1 items into the existing phases:
   - Phase 2: name the `started/succeeded/failed` diagnostic convention (T1-3).
   - Phase 3: add composition-pass ordering invariant to `SubstrateScene` done conditions (T1-2).
   - Phase 5: introduce `EnvelopeCapabilityProfile` as the explicit envelope contract (T1-4).
   - Cross-cutting prerequisite: single-write-path invariant on the mutation API (T1-1) — this is mere-kernel work, sits above the spatial-chrome lane but blocks none of it.
   - T1-5 (projection binding mode) folds into a small amendment to the cartography brief + view intent sidecar plan, not the spatial-chrome plan.

2. **File targeted small plans for Tier 2 items that don't fold into existing phases**:
   - UxTree renderer-contributed sub-tree merge contract (T2-6) — small follow-up to the spatial-chrome plan Phase 3.
   - Seven-way Graph Object Classification (T2-3) — intersects relation taxonomy and Moothold sync policy; warrants its own brief.
   - Scene mode escalation (T2-1) — cartography expansion brief.
   - Hot/cold traversal archive (T2-5) — small follow-up to relation taxonomy plan.
   - GPU authority topology (T2-8) + frame scheduler (T2-9) — fold into the eventual `scrying::native_frame` extraction plan (spatial-chrome plan Phase 4).
   - Universal Node Content Model — NodeId/address split (T2-2) — significant architectural decision; warrants its own design pass before any code lands.

3. **Treat Tier 3 as a checklist** for cross-cutting design reviews. Each item is small enough to fold into whichever plan touches it next; no standalone artifacts needed.

---

## Open questions

- T2-2 (NodeId/address split) implies a mere-kernel migration. The current address-keyed graph is shipped in the relation taxonomy plan landings. Is the cost of migrating to UUID-keyed nodes worth the flexibility? Defer until a concrete use case (file-system browsing as graph nodes? Nostr event aggregation across relays?) is on the critical path.
- T2-3 (graph object classification) reaches into Moothold sync policy. Premature without more concrete Moothold work in flight.
- The graphshell "Verse" docs (P2P federation, identity convergence at protocol level) were intentionally skipped beyond identity-claim aggregation. When Murm/Moothold work picks up, revisit.
