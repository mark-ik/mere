# Archive checkpoint — pivot-superseded (2026-06-09)

Docs obsoleted by architecture pivots, moved here per DOC_POLICY §2 ("move
superseded material to archive_docs rather than editing in place") during the
2026-06-09 audit. Each was verified dead against live code (`crates/graphshell/`
and `crates/mere/` are gone; the host is `meerkat`; `graph-canvas` dissolved into
the `orrery/` family; the Cable wire is deleted).

The pivots that retired these:
- **Host**: gpui / Xilem-Masonry-as-host / the `register-renderer` trait →
  `meerkat` on the genet-as-host path. (`modular_integration_plan` names the
  register-renderer stack dead.) Retires: scrying_web_tile, typed_action_bus,
  spatial_chrome_modular_adoption, host_architecture_roadmap, scrying_integration,
  verso_adoption, browser_taxonomy_translation, os_plumbing_reuse_audit,
  renderer_registry_contract, spatial_chrome_ir, xilem_embedding_spike,
  component_fit_map, between_tiles_layout_seam, netrender_for_engine_documents,
  app_architecture_rescaffold (the superseded idiomatic-Xilem + Masonry host re-scaffold).
- **graph-canvas dissolution** → `orrery/{aether,arrangements,gyre,cartography}`.
  Retires: graph_canvas_field_algebra (→ field_system_extraction),
  node_per_tile_lineage (→ node_navigation_lineage_wiring).
- **Substrate**: Cable → p2panda (Cable wire deleted). Retires:
  cable_migration_from_verso.
- **Donor archival / decomposition executed**: graphshell_supercrate_salvage_map,
  retired_host_stack_salvage_map, donor_graphshell_repo_salvage_map.
