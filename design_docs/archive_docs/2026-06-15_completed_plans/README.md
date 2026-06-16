# 2026-06-15 Completed Plans

Three plans swept on completion (DOC_POLICY §8) during the doc-hygiene pass that
accompanied the
[in-the-wings + browser-bar audit](../../mere_docs/research/2026-06-15_in_the_wings_and_browser_bar_audit.md).
Each is done by its own done-conditions; the audit's §3 ("already shipped, docs
understate") is what surfaced them. Internal relative links inside these docs
resolve against their original `mere_docs/implementation_strategy/` location.

- **[2026-06-10_host_cheap_path_plan.md](2026-06-10_host_cheap_path_plan.md)** — the
  perf chain (move meerkat's DOM panes onto `IncrementalLayout` sessions + a
  laid-out-document query seam). C0–C5 + C4c shipped and proven (chrome
  cascade+layout 4.3×, whole frame −40%); the spun-out C6 grab-bag is its own plan
  (`host_wiring_grabbag_plan`, still active).

- **[2026-06-12_mesh_m1_plan.md](2026-06-12_mesh_m1_plan.md)** — the personal-space
  compute mesh, milestone 1. Complete: the two-machine run (Windows laptop ↔ Fedora
  ThinkPad) landed 2026-06-12, all done-conditions met. M2 (`MeshResource`), M3
  (heartbeat/reassign), M4+ (economy), and meerkat's P6 compute actor are future
  milestones tracked from the resource-coordination brief.

- **[2026-06-13_omnibar_command_shell_plan.md](2026-06-13_omnibar_command_shell_plan.md)** —
  the privileged omnibar command shell (`>`-expressions over the `Command` spine).
  S0–S4 shipped and on-screen verified. The *sandboxed knot-note `rhai eval` lane*
  (the other half of the two-tier trust model) is a different plan's scope and
  remains unwired: see `nematic_docs` `knot_evaluation_export_plan` and the audit's
  Tier B.
