# MCP-native graph (future)

**Date**: 2026-06-26
**Status**: Scoped, future, not yet. Captured from the
[borrowed-ideas brief](../research/2026-06-25_borrowed_ideas_brief.md) at Mark's
direction (scope both directions now, build later). Mere is agentic, so MCP (the Model
Context Protocol) is the natural agent boundary: it makes external agents first-class
citizens of the space and gives internal agents a standard way to reach out, both
without bespoke glue. No work scheduled.

## Two directions

**Expose: Mere as an MCP server.** The graph (nodes, edges, queries, crawl, clip)
offered as MCP tools and resources, so an external agent (Claude or any MCP client)
operates on the user's space. The clean substrate is the existing
[command registry](../../archive_docs/2026-09-02_retired_plans/2026-06-21_command_registry_configurable_menus_plan.md): the agent's
API is already the same `ActionRegistry` the user drives (the agent harness already runs
over `Command::ALL`), so the MCP tool set is that registry exposed over MCP, plus
read-only resources for nodes and subgraphs. Mutating tools route through the same
command path, so they inherit its gating.

**Consume: internal agents reach external MCP servers.** Agent nodes and the harness
gain an MCP client, so an agent node's policy can call out to external MCP tools. The
outbound half of the same boundary.

## What it must ride

- **Capability gating.** Exposed mutating tools sit behind the in-app permission spine
  (the [capability-gate catalogue](../research/2026-05-14_capability_gate_catalogue_brief.md)),
  with consent. Read scope can reuse the federation capability work
  ([federation_interop](2026-06-26_federation_interop_plan.md)) when an external agent's
  reach should be a scoped subgraph rather than the whole space.
- **Provenance.** Every external-agent mutation asserts a `ProvenanceSubKind` edge (the
  borrowed-ideas brief's assert-on-every-agent-mutation rule), so external action stays
  auditable and reversible.

## Decisions to settle (when picked up)

- Transport (stdio / SSE / streamable HTTP) and auth for the exposed server.
- Which commands are exposed, and the read-only vs mutating split (mutating off by
  default, behind consent plus capability).
- Whether expose or consume leads. Expose is the higher-value, more-contained half: it
  rides the command registry and needs no new agent runtime.

## Cross-references

- [borrowed-ideas brief](../research/2026-06-25_borrowed_ideas_brief.md): the source.
- [command registry plan](../../archive_docs/2026-09-02_retired_plans/2026-06-21_command_registry_configurable_menus_plan.md): the
  `ActionRegistry` that becomes the MCP tool set.
- [capability-gate catalogue brief](../research/2026-05-14_capability_gate_catalogue_brief.md):
  the gating the exposed tools sit behind.
- [federation_interop plan](2026-06-26_federation_interop_plan.md): capability-scoped read
  for external agents.
- [document_script_substrate plan](../../archive_docs/2026-07-03_completed_plans/2026-06-21_document_script_substrate_plan.md),
  [local_models_harness brief](../research/2026-06-24_local_models_harness_brief.md): the
  agentic lanes this complements.

## Progress

- **2026-06-26, scoped (future).** Captured from the borrowed-ideas brief at Mark's
  direction, both directions, marked future. The expose half rides the existing command
  registry as its tool surface and the capability spine as its gate; the consume half
  adds an MCP client to the agent path. No work scheduled, no code.
