# Mere Documentation Index

This is the design-docs index for the [Mere](../README.md) workspace. All authoritative project documentation belongs under `design_docs/`.

## Required reading order

1. [`DOC_POLICY.md`](DOC_POLICY.md) — documentation governance rules
2. [`TERMINOLOGY.md`](TERMINOLOGY.md) — canonical project terminology (skeleton; defers to the lexicon brief)
3. [`2026-05-04_lexicon_brief.md`](2026-05-04_lexicon_brief.md) — current naming scheme + in-product vocabulary (authoritative for terms it covers)
4. Per-area roots (populated as work proceeds):
   - `mere_docs/` — product-level concerns (Navigator, scope spectrum, UX, cross-cutting protocol architecture)
     - [`mere_docs/implementation_strategy/2026-05-05_protocol_architecture_plan.md`](mere_docs/implementation_strategy/2026-05-05_protocol_architecture_plan.md) — cross-cutting plan for iroh-toolkit layering, identity vault, WebFinger self-host pattern, and primitive moot nodes
   - `graphshell_docs/` — portable shell layer, host GUI integration, Navigator surface
     - [`graphshell_docs/implementation_strategy/2026-05-06_graphshell_migration_plan.md`](graphshell_docs/implementation_strategy/2026-05-06_graphshell_migration_plan.md) — migration plan for moving the existing Graphshell codebase into Mere and the associated crates
   - `murm_docs/` — bilateral comms (Cable, MLS, co-op session lifecycle, identity derivation)
   - `moothold_docs/` — community/federation (moots, demesnes, social primitives, tessera validation)
     - [`moothold_docs/implementation_strategy/2026-05-05_irc_mod_plan.md`](moothold_docs/implementation_strategy/2026-05-05_irc_mod_plan.md) — IRC as Phase 3 T1 first cut; Pattern A + Pattern B (puppet mode)
   - `verso_docs/` — rendering-surface management (tile placement in GraphTree)
   - `graphshell_docs/` — shell layer (host GUI integration, Navigator surface)
   - `inker_docs/` — engine controller (engine selection, lifecycle, routing)
   - `platen_docs/` — composition surface (graph-aware layout)
   - `nematic_docs/` — smolweb engine (Gemini, Gopher, static HTML, Markdown, RSS/Atom)

## Working principles

- **Printing-press metaphor** organizes internal data flow: engines (Wry, Serval, Nematic) → **inker** (selects/orchestrates) → **platen** (graph-aware composition) → **verso-tile** (receives the impression). **Mnem** keeps impressions over time.
- **In-product vocabulary**: use *moot* (count noun for community), *demesne* (federation cluster), *suzerainty* (their relation), *engram* (contribution payload), *flora* (a moot's accumulated engrams), *kith / kin* (contact tiers), *volvelle* (radial UI form factor of an expanded moot), *astroid* (UX vocab for graphlet hub-collapse), *tessera* (trust/contribution token), *mnem* (private local browsing memory), *orrery* (root graph view).
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
