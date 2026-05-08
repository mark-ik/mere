# Mere Documentation Index

This is the design-docs index for the [Mere](../README.md) workspace. All authoritative project documentation belongs under `design_docs/`.

## Required reading order

1. [`DOC_POLICY.md`](DOC_POLICY.md) — documentation governance rules
2. [`TERMINOLOGY.md`](TERMINOLOGY.md) — canonical project terminology (skeleton; defers to the lexicon brief)
3. [`2026-05-04_lexicon_brief.md`](2026-05-04_lexicon_brief.md) — current naming scheme + in-product vocabulary (authoritative for terms it covers)
4. Per-area roots (populated as work proceeds):
   - `mere_docs/` — product-level concerns (Navigator, scope spectrum, UX, cross-cutting protocol architecture)
     - [`mere_docs/implementation_strategy/2026-05-05_protocol_architecture_plan.md`](mere_docs/implementation_strategy/2026-05-05_protocol_architecture_plan.md) — cross-cutting plan for iroh-toolkit layering, identity vault, WebFinger self-host pattern, and primitive moot nodes
     - [`mere_docs/implementation_strategy/2026-05-07_event_dag_substrate_brief.md`](mere_docs/implementation_strategy/2026-05-07_event_dag_substrate_brief.md) — substrate pivot: drop Cable, BLAKE3 unification, Mere-native event DAG as protocol identity, sync layers as projections, schema-at-engram-boundary, Veilid as optional per-moot transport, multi-protocol moot hosting (revised), first-pass identity-system design
     - [`mere_docs/implementation_strategy/2026-05-07_moot_tiers_and_voluntary_hosting_brief.md`](mere_docs/implementation_strategy/2026-05-07_moot_tiers_and_voluntary_hosting_brief.md) — moot tier framework (t1 orrery → t2 moot → t3 moothold → t4 demesne), demesne→moothold→demesne lexicon shift, voluntary hosting + reputational stakes, cheesecloth pinning, ILL-shaped reciprocity at federation tiers, lapse-and-revive as normal life cycle, stake+agreement coordination protocol
   - `graphshell_docs/` — portable shell layer, host GUI integration, Navigator surface
     - [`graphshell_docs/implementation_strategy/2026-05-06_graphshell_migration_plan.md`](graphshell_docs/implementation_strategy/2026-05-06_graphshell_migration_plan.md) — migration plan for moving the existing Graphshell codebase into Mere and the associated crates
     - [`graphshell_docs/implementation_strategy/2026-05-07_graph_canvas_field_algebra_plan.md`](graphshell_docs/implementation_strategy/2026-05-07_graph_canvas_field_algebra_plan.md) — graph-canvas field algebra (custom AST → Burn lowering), Rhai per-canvas composition, ZSource → FieldProjection promotion, lossless dimension ladder with presets, `scene_physics.rs` split
   - `murm_docs/` — bilateral comms (Mere-native event DAG over iroh streams; co-op session lifecycle; identity derivation)
   - `moothold_docs/` — community/federation (moots, demesnes, social primitives, tessera validation)
     - [`moothold_docs/implementation_strategy/2026-05-05_irc_mod_plan.md`](moothold_docs/implementation_strategy/2026-05-05_irc_mod_plan.md) — IRC as Phase 3 T1 first cut; Pattern A + Pattern B (puppet mode)
   - `verso_docs/` — rendering-surface management (tile placement in GraphTree)
   - `graphshell_docs/` — shell layer (host GUI integration, Navigator surface)
   - `inker_docs/` — engine controller (engine selection, lifecycle, routing)
   - `platen_docs/` — composition surface (graph-aware layout)
   - `nematic_docs/` — smolweb engine (Gemini, Gopher, static HTML, Markdown, RSS/Atom)

## Working principles

- **Printing-press metaphor** organizes internal data flow: engines (Wry, Serval, Nematic) → **inker** (selects/orchestrates) → **platen** (graph-aware composition) → **verso-tile** (receives the impression). **eidetic** keeps impressions over time.
- **In-product vocabulary** (tier framework: *orrery* t1 → *moot* t2 → *moothold* t3 → *demesne* t4): *orrery* (a user's root graph view, t1, "your orrery is your moot"), *moot* (single themed federatable graph-view community, t2), *moothold* (federation of moots — *a holding of moots* in the Anglo-Saxon sense, t3), *demesne* (sovereign coalition of mootholds, t4), *suzerainty* (the relation between an outer tier and its inner members), *engram* (portable, durable, schematicized memory unit; canonical contribution payload), *flora* (a moot's accumulated engrams), *kith / kin* (contact tiers), *volvelle* (radial UI form factor of an expanded moot), *astroid* (UX vocab for graphlet hub-collapse), *tessera* (trust/contribution token, accrues against an identity's chain root), *eidetic* (private local memory; the substrate engrams are distilled from).
- **Avoid retired terms**: *Verse* (folded into Mere-at-network-scope), *Murmuration* (replaced by `moothold` + count-noun *moot*), *Gist* (replaced by *engram*), *Flock* (replaced by *kith / kin*), *Graphshell-as-product-name* (now a crate name only).
- **Cross-referencing**: relative links within `mere/design_docs/`; explicit absolute paths when referencing the inherited `graphshell/design_docs/` content.

## Inheritance from graphshell/design_docs/

The existing [`c:\Users\mark_\Code\repos\graphshell\design_docs\`](../../graphshell/design_docs/) directory remains intact and authoritative for its content. Migration into this workspace is incremental — pull docs over as they become relevant to active work, not as a big-bang reorg. Until a doc is migrated, the original location is canonical.

Specifically, until migrated, the following remain authoritative at the original graphshell paths:

- `TERMINOLOGY.md` — canonical terminology for terms not addressed in this workspace's `2026-05-04_lexicon_brief.md`
- `engram_spec.md` (1100+ lines) — full canonical engram spec
- `VERSO_AS_PEER.md` — pre-migration Verso role spec (will be split between `murm_docs/` and `verso_docs/` here)
- `COMMS_AS_APPLETS.md` — Comms surface family (consumes `moothold` here)
- `coop_session_spec.md` — co-op authority (relevant to `murm_docs/`)
- `cable_coop_minichat_spec.md` — Cable adoption plan (subject of the migration plan in this workspace)

## Status

Pre-1.0 development. Design-docs scaffolded 2026-05-04 alongside the 10-crate workspace reservation. Most directories are empty until populated; the lexicon brief and the Cable migration plan are the first inhabitants.
