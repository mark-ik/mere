# Mere Documentation Index

The design-docs index for the [Mere](../README.md) workspace. All authoritative
project documentation belongs under `design_docs/`. Entries are one line each;
the deep rationale lives in the doc itself.

## Required reading order

1. [`DOC_POLICY.md`](DOC_POLICY.md) — documentation governance rules.
2. [`TERMINOLOGY.md`](TERMINOLOGY.md) — canonical terminology (skeleton; defers to the lexicon).
3. [`2026-05-04_lexicon_brief.md`](2026-05-04_lexicon_brief.md) — naming scheme + in-product vocabulary (authoritative for terms it covers).
4. [`2026-05-24_external_deps_topology_brief.md`](2026-05-24_external_deps_topology_brief.md) — the `Code/` workspace-root `crates/` ↔ `repos/` split, path-dep convention, and cross-repo rename lineage.

---

## mere_docs/implementation_strategy/ — dated plans

Several older plans here carry a 2026-06-09 rename-key banner: their bodies use
pre-pivot crate names (e.g. `mere-host`, `graph-canvas`, Cable/BLAKE2b) as dated
receipts. The banner gives the current mapping.

- [protocol_architecture_plan](mere_docs/implementation_strategy/2026-05-05_protocol_architecture_plan.md) — iroh-toolkit layering, identity vault, WebFinger self-host, primitive moot nodes. *(Substrate sections superseded by p2panda; banner.)*
- [event_dag_substrate_brief](mere_docs/implementation_strategy/2026-05-07_event_dag_substrate_brief.md) — substrate pivot: event-DAG identity, BLAKE3, schema-at-engram-boundary, sync-as-projection. *(p2panda executed; banner.)*
- [moot_tiers_and_voluntary_hosting_brief](mere_docs/implementation_strategy/2026-05-07_moot_tiers_and_voluntary_hosting_brief.md) — tier framework (orrery → moot → moothold → coalition), voluntary hosting + reputational stakes, cheesecloth pinning.
- [post_engine_layer_priorities](mere_docs/implementation_strategy/2026-05-09_post_engine_layer_priorities.md) — forward plan after the engine layer landed. *(gpui-era snapshot; banner.)*
- [graph_cluster_namespaces_brief](mere_docs/implementation_strategy/2026-05-10_graph_cluster_namespaces_brief.md) — namespaces derived from the graph's community structure (Leiden/Louvain), not admin paths; capability-scope mapping.
- [engine_profile_boundary_plan](mere_docs/implementation_strategy/2026-05-14_engine_profile_boundary_plan.md) — graph-truth vs engine-profile bytes; per-persona/session/graph UDF path resolution.
- [session_service_runner_plan](mere_docs/implementation_strategy/2026-05-14_session_service_runner_plan.md) — sessions declare background workers behind a `SessionServiceRunner` capability; v0a landed.
- [short_term_memory_substrate_plan](mere_docs/implementation_strategy/2026-05-14_short_term_memory_substrate_plan.md) — substrate-per-short-term-consumer (JSON sidecars vs in-memory); branch/fork state.
- [net_media_plan](mere_docs/implementation_strategy/2026-05-26_net_media_plan.md) — the media organ (sibling to netfetcher/netrender): WebRTC + AV1 decode, three-tier asm-free decode policy.
- [serval_host_flip_plan](mere_docs/implementation_strategy/2026-06-01_serval_host_flip_plan.md) — execution plan for the serval-as-host flip (P0 perf spike → P5 cutover); IME + interactive gate clear (2026-06-10).
- [modular_integration_plan](mere_docs/implementation_strategy/2026-06-02_modular_integration_plan.md) — the unifying integration sequence + the graph-rooted projection model (graph is the root; orrery/workbench/gloss are projections). *(Draft; active.)*
- [actor_constellation_plan](mere_docs/implementation_strategy/2026-06-03_actor_constellation_plan.md) — single-threaded kernel + I/O/content/compute actors (Servo's constellation done in-process; scenes travel as messages).
- [host_p2p_wiring_plan](mere_docs/implementation_strategy/2026-06-03_host_p2p_wiring_plan.md) — wire the p2p substrate into the meerkat loop via the fetcher's async seam; S5.0–S5.2 shipped, S5.3 → comms shell.
- [comms_shell_plan](mere_docs/implementation_strategy/2026-06-05_comms_shell_plan.md) — docked comms pane (misfin mail + murm cabals) over a host-neutral comms domain. *(Largely implemented.)*
- [node_navigation_lineage_wiring_plan](mere_docs/implementation_strategy/2026-06-05_node_navigation_lineage_wiring_plan.md) — drive the built per-node nav-lineage substrate from the live navigation path (within-node history + across-node relations).
- [moot_constitution_brief](mere_docs/implementation_strategy/2026-06-06_moot_constitution_brief.md) — the constitution primitive: a per-moot ruleset + amendment rule (the §8.8 policy-authorization layer); home is `moothold` beside tessera.
- [apparatus_pane_and_theme_switcher_plan](mere_docs/implementation_strategy/2026-06-08_apparatus_pane_and_theme_switcher_plan.md) — apparatus pane + runtime theme switcher; A1 shipped, A2 orrery theming pending.
- [system_diagnostics_and_accessibility_plan](mere_docs/implementation_strategy/2026-06-08_system_diagnostics_and_accessibility_plan.md) — wire Apparatus to one meerkat observability spine (ux-events, register-diagnostics, tracing/probes, UxTree/AccessKit, agent harness).
- [accesskit_screen_reader_verification](mere_docs/implementation_strategy/2026-06-09_accesskit_screen_reader_verification.md) — AccessKit screen-reader verification notes.
- [shellbar_plan](mere_docs/implementation_strategy/2026-06-09_shellbar_plan.md) — F2 shellbar: a docked chrome strip (edge-configurable) with pane-toggle buttons, outside the frame tree.
- [multi_graph_activation_plan](mere_docs/implementation_strategy/2026-06-09_multi_graph_activation_plan.md) — wire meerkat to hold many graphs per window, switchable from the shellbar (Model B = window holds panes/graph is content, goal; Model A = session owns the content band, checkpoint at MG2). Session-manifest substrate (`ManifestStore`, `switcher_thumbnail`, `tearout`) is built and unconsumed; F2.3's switcher is MG4.
- [workbench_staging_plan](mere_docs/implementation_strategy/2026-06-09_workbench_staging_plan.md) — the #5 staging flow (stage nodes → commit to workbench → latent staging relation), spun out of the completed card-system plan.

## mere_docs/technical_architecture/

- [workspace_topology_status](mere_docs/technical_architecture/2026-05-19_workspace_topology_status.md) — supercrate-naming snapshot; §1–5 are pre-flip receipts, §7 (graphshell dissolution) + §8 (canvas-ir/graph-layout review) are current.
- [mere_composition_spine](mere_docs/technical_architecture/2026-05-21_mere_composition_spine.md) — **the spine**: truth → arrangement (forme) → projection (platen) → surface (verso) → engine (inker), with the three persistence scopes.
- [statements_over_schema_stance](mere_docs/technical_architecture/2026-05-22_statements_over_schema_stance.md) — the stance on statement/triple-shaped data over rigid schemas.
- [cartography_aether_layout_seam](mere_docs/technical_architecture/2026-05-29_cartography_aether_layout_seam.md) — how cartography + arrangements (projection) relate to gyre (physics); gyre is not a cartography strategy; the `Projection` bridge.
- [field_system_extraction](mere_docs/technical_architecture/2026-05-30_field_system_extraction.md) — the field system as a kernel primitive, and the decomposition of graph-canvas into the `orrery/*` family.
- [peripheral_panes_architecture](mere_docs/technical_architecture/2026-06-06_peripheral_panes_architecture.md) — the peripheral-pane slot (gloss/apparatus/roster) and the donor-harvest-per-pane shapes.
- [alembic_memory_and_engrams](mere_docs/technical_architecture/2026-06-09_alembic_memory_and_engrams.md) — the alembic memory model + engrams; the armillary distillation daemon.

## mere_docs/research/

Several older briefs here carry a 2026-06-09 rename-key banner (pre-pivot crate
names as receipts).

- [local_intelligence_integration_research](mere_docs/research/2026-05-08_local_intelligence_integration_research.md) — Burn-first statistical intelligence; tier ladder; embeddings shipped at `intel/embed`, generative/agentic deferred. *(banner.)*
- [cartography_layer_brief](mere_docs/research/2026-05-10_cartography_layer_brief.md) — cartography as the non-destructive projection layer (now `orrery/cartography`); strategy catalogue. *(banner.)*
- [geist_models_brief](mere_docs/research/2026-05-10_geist_models_brief.md) — personal + moot-trained LoRA adapters as engrams; LoRA stacking keystone; tessera-as-compute-credit (tier-3+).
- [browser_multiplexer_framing](mere_docs/research/2026-05-11_browser_multiplexer_framing.md) — the builder-facing "Mere multiplexes durable graph sessions" framing; identity matrix; session manifest. *(banner.)*
- [engine_peers_and_scrying_library_brief](mere_docs/research/2026-05-11_engine_peers_and_scrying_library_brief.md) — engine taxonomy: mere is the manager, engines are content functions, scrying is a library not a peer. *(banner.)*
- [memory_tiers_brief](mere_docs/research/2026-05-11_memory_tiers_brief.md) — short-term vs long-term memory partition; consolidation into engrams is an affirmative gesture.
- [tearout_operations_brief](mere_docs/research/2026-05-11_tearout_operations_brief.md) — the leaf / branch / fork tear-out trichotomy, gesture model, and identity semantics. *(banner.)*
- [capability_gate_catalogue_brief](mere_docs/research/2026-05-14_capability_gate_catalogue_brief.md) — the capability-gate catalogue the action bus + permission spine consume.
- [daemon_split_research_brief](mere_docs/research/2026-05-14_daemon_split_research_brief.md) — whether/when to split a separate daemon process from per-window clients (research; no v1 implementation).
- [persona_model_brief](mere_docs/research/2026-05-14_persona_model_brief.md) — one-human-many-personas; what a persona owns; persona switch as a session-boundary moment.
- [graphshell_harvest_brief](mere_docs/research/2026-05-17_graphshell_harvest_brief.md) — concept inventory pulled from the donor graphshell docs. *(Canonical donor concept-index per DOC_POLICY; banner.)*
- [graphshell_docs_full_harvest](mere_docs/research/2026-05-27_graphshell_docs_full_harvest.md) — the full sweep of the 633 donor docs (what to pull, where it lived). *(Canonical donor index per DOC_POLICY.)*
- [understory_orrery_graduation_brief](mere_docs/research/2026-05-27_understory_orrery_graduation_brief.md) — how/when to evaluate forest-rs/understory against the orrery element. *(banner.)*
- [nonstandard_browsing_profiles_brief](mere_docs/research/2026-05-30_nonstandard_browsing_profiles_brief.md) — Willow/Iroh-Willow/NextGraph/W3C survey; the derived-RDF projection boundary; optional host-level browsing profiles.
- [two_natured_kernel_brief](mere_docs/research/2026-05-30_two_natured_kernel_brief.md) — the kernel's two natures (data + space); the `aether` (field-algebra) / `gyre` (rapier) naming; Coupling as a force, not a 7th edge family.
- [murm_p2p_landscape_brief](mere_docs/research/2026-05-31_murm_p2p_landscape_brief.md) — orientation survey of the whole p2p program + external landscape verdicts. *(Pivot now executed; banner.)*
- [resource_coordination_brief](mere_docs/research/2026-06-04_resource_coordination_brief.md) — sharing storage + compute as one model (trust-graduated rings, bounty grammar, two ledgers, verifiable compute, durability); supersedes the 2026-06-03 banking/mesh briefs.

## mere_docs/design/

- [pane_ux_design_pass_brief](mere_docs/design/2026-05-11_pane_ux_design_pass_brief.md) — the five-gap pane-UX target (drag-rearrange, frame split, click hierarchy, context menus). *(gpui-era; largely realized; merge-then-archive into the frame taxonomy; banner.)*
- [gloss_navigator_design](mere_docs/design/2026-06-07_gloss_navigator_design.md) — gloss = the Navigator: one configurable summary surface across scope (document ↔ graph ↔ graphlet) and form factor (outline ↔ swatch).
- [graph_roster_and_frame_taxonomy](mere_docs/design/2026-06-07_graph_roster_and_frame_taxonomy.md) — the graph roster (graph manifest) + the surface/frame taxonomy (orrery/gloss/roster/apparatus/workbench/shellbar); §4 shellbar decision.

## Per-crate areas

- **eidetic_docs/** — [eidetic_design_pass](eidetic_docs/research/2026-05-09_eidetic_design_pass.md) (the four-layer memory stack design; mostly built; banner) · [eidetic_deferred_phases_plan](eidetic_docs/implementation_strategy/2026-06-09_eidetic_deferred_phases_plan.md) (the deferred Phases 7-9: OPFS store, browsing memory, search index — spun out of the completed layered-stack plan).
- **moothold_docs/** — [irc_mod_plan](moothold_docs/implementation_strategy/2026-05-05_irc_mod_plan.md) (IRC as the first T1 protocol mod). *(tessera_plan archived 2026-06-09.)*
- **murm_docs/** — [MURM_AS_BILATERAL](murm_docs/technical_architecture/MURM_AS_BILATERAL.md) (murm's bilateral-comms role + boundaries; banner: substrate pivoted to p2panda).
- **nematic_docs/** — [polyglot_knot_design](nematic_docs/implementation_strategy/2026-05-08_polyglot_knot_design.md) (protocol-faithful clip composition; knot bodies as polyglot CommonMark + fenced protocol blocks; implemented).

## archive_docs/ — superseded checkpoints (DOC_POLICY §4)

- [`2026-05-09_engine_layer_complete/`](archive_docs/2026-05-09_engine_layer_complete/) — the 2026-05-06 graphshell migration plan + donor inventory.
- [`2026-06-04_resource_coordination_merge/`](archive_docs/2026-06-04_resource_coordination_merge/) — the 2026-06-03 resource-banking + compute-mesh briefs (merged into the resource-coordination brief).
- [`2026-06-09_completed_plans/`](archive_docs/2026-06-09_completed_plans/) — 21 docs: 17 shipped plans swept on completion (DOC_POLICY §8) plus 4 executed decision-records/roadmaps from the held set (serval_as_host_evaluation, adoption_roadmap, p2panda_substrate_spike_plan, tessera_plan). See its README. Two plans had a live tail spun out first: eidetic Phases 7-9 and workbench staging.
- [`2026-06-09_pivot_superseded/`](archive_docs/2026-06-09_pivot_superseded/) — 21 docs obsoleted by the meerkat/serval-as-host, p2panda, and graph-canvas-dissolution pivots (incl. the superseded Xilem-host rescaffold); see its README.

## Current workspace topology

The workspace is organized into supercrate directories under `crates/`, each
owning a single concern. Crate leaf names dropped their `mere-*` / role prefixes
in the 2026-05-19 B1–B7 naming pass; the directory path disambiguates.

| Supercrate | Path | Owns |
|---|---|---|
| `meerkat` | `crates/meerkat/` | the Mere host binary: winit app, chrome + frame-tree + shellbar, panes, the constellation actors, content cards |
| `graph` | `crates/graph/` | graph truth: `graph-kernel`, `linked-data` (ingest/export), `node-lineage` (nav history) |
| `orrery` | `crates/orrery/` | the spatial graph view: `orrery` / `mere-orrery`, `arrangements` (layout strategies), `cartography` (non-destructive projection), `gyre` (rapier physics), `aether` (field algebra) |
| `shell` | `crates/shell/` | host-neutral domain: `chrome` view-models, `comms`, `frame` model |
| `system` | `crates/system/` | runtime services: `registry`, `session-runtime` (settings + manifest), `shell-state`, `ux-events` |
| `inker` | `crates/inker/` | engines: `document-canvas`, `engines` (`nematic` for smolweb, scrying for system WebViews) + the registry |
| `forme` | `crates/forme/` | per-graph-view workbench arrangement authority + `uxtree` |
| `platen` | `crates/platen/` | composition surface: `platen`, `domain` panels (apparatus, gloss, workbench), `view` |
| `verso` | `crates/verso/` | tile surface lifecycle: `verso-core`, `tile-state` |
| `eidetic` | `crates/eidetic/` | durable memory: `eidetic-core` + fjall / https / iroh fetchers |
| `intel` | `crates/intel/` | local intelligence: `embed` owns embeddings + vector index + semantic search |
| `murm` | `crates/murm/` | bilateral comms + transport: `murm`, `murmuring`, `transport` (p2panda), `misfin`, `webfinger` |
| `moot` | `crates/moot/` | community/federation: `mooting` (t2), `moothold` (t3, tessera + constitution) |
| `persona` | `crates/persona/` | persona `identity` + keypair vault |
| `armillary` | `crates/armillary/` | alembic memory + engrams (see the 2026-06-09 alembic doc) |
| `import` | `crates/import/` | linked-data ingest/export |
| `serval-winit-host` | `crates/serval-winit-host/` | the serval-as-host winit integration |
| `probes` | `crates/probes/` | spike/probe crates (p2panda, willow-cap, tessera-logsync, redb-logstore, …); not product crates |

Full per-sub-crate snapshot (with load-bearing vs aspirational status) at
[`mere_docs/technical_architecture/2026-05-19_workspace_topology_status.md`](mere_docs/technical_architecture/2026-05-19_workspace_topology_status.md)
(§7/§8 current). For the workspace-root `Code/crates/` (vendored libs) ↔
`Code/repos/` (own projects) split and the cross-repo rename lineage
(servo-wgpu→serval, webrender-wgpu→netrender), see
[`2026-05-24_external_deps_topology_brief.md`](2026-05-24_external_deps_topology_brief.md).

## Working principles

- **Printing-press metaphor** organizes the data flow in two threads:
  - *Per-node content production*: engines (Serval, Nematic, scrying) → **inker** (selects/orchestrates the engine + routing) → per-node engine output.
  - *Per-graph-view workbench arrangement*: graph truth (`graph/graph-kernel`) → **forme** (locks graph members + edges into the arrangement the view will print) → **platen** (presses the forme into surface/pane output) → **verso** (receptor of surface/tile lifecycle, communicating rearrangement back up to forme, and forme back to graph truth).
  - **eidetic** keeps impressions over time (private local memory; content-addressed engrams); **node-lineage** records per-owner navigation lineage.
- **In-product vocabulary** (tiers *orrery* t1 → *moot* t2 → *moothold* t3 → *coalition* t4): *orrery* (a user's root graph view), *moot* (a themed federatable graph-view community), *moothold* (a holding of moots), *coalition* (a sovereign cluster of mootholds), *suzerainty* (outer-tier ↔ inner-member relation), *engram* (portable durable schematicized memory unit), *flora* (a moot's accumulated engrams), *kith / kin* (contact tiers), *volvelle* (radial moot form factor), *astroid* (graphlet hub-collapse), *tessera* (trust/contribution token), *eidetic* (private local memory).
- **Avoid retired terms**: *Verse*, *Murmuration*, *Gist*, *Flock*, *Graphshell-as-product-name* (now a crate concept only) — see the lexicon brief §5.
- **Cross-referencing**: relative links within `design_docs/`; cite the donor graphshell content via the harvest indexes / GitHub archive (the local donor repo is gone).

## Inheritance from graphshell/design_docs/

The donor `graphshell` repo was **GitHub-archived (read-only) and its local clone deleted 2026-05-27**; its 633 design docs were swept into two curated indexes that are the entry points for any remaining pull: the [full docs harvest](mere_docs/research/2026-05-27_graphshell_docs_full_harvest.md) and the [concept brief](mere_docs/research/2026-05-17_graphshell_harvest_brief.md). Fetch detail from the GitHub archive when a slice needs it; the old `../../graphshell/design_docs/` local path no longer resolves.

Specifically, the following live in the GitHub archive (read-only), surfaced via the harvest indexes above: `TERMINOLOGY.md` (pre-Mere terms), `engram_spec.md` (the 1100+ line engram spec), `VERSO_AS_PEER.md`, `COMMS_AS_APPLETS.md`, `coop_session_spec.md`, and `cable_coop_minichat_spec.md`.

## Status

Pre-1.0 development. Index compacted in the 2026-06-09 design-docs audit (37 docs archived, entries trimmed to one line each). Older dated briefs may cite pre-topology crate paths as historical receipts; the rename-key banners and the topology table above give the current mapping.
