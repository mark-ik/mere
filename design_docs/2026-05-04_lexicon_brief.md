# Mere Lexicon Brief

**Status**: Active / authoritative for terms covered
**Date**: 2026-05-04
**Scope**: Establishes the post-rename naming scheme and in-product vocabulary for the Mere project. Authoritative for terms it covers; supersedes the prior [2026-05-03 lexicon brief](../../graphshell/design_docs/2026-05-03_lexicon_brief.md) (which had Strophos+Orrery as the product) and complements [`TERMINOLOGY.md`](TERMINOLOGY.md) until that file is fully populated. Terms not addressed here defer to inherited [`graphshell/design_docs/TERMINOLOGY.md`](../../graphshell/design_docs/TERMINOLOGY.md).

**Execution status**: 10-crate workspace scaffolded at `c:\Users\mark_\Code\repos\mere\` 2026-05-04. Crates.io publication in progress (rate-limited; some crates published, some pending). The existing `c:\Users\mark_\Code\repos\graphshell\` directory remains intact and untouched.

---

## 1. Top-level naming

| Name | What it labels | Notes |
|------|----------------|-------|
| **Mere** | Product / app — the browser itself | Triple-meaning positioning: *merely* (humble — "merely a browser!"), *mere* (a small lake — still-water surface where things accrue and reflect), slant-rhyme with *mirror*. Disambiguates from Māori weapon by intentional framing. Replaces *Graphshell-as-product-name* and *Verse-as-network-layer*. |
| **Strophos** | Parent brand / company-name layer | Greek στρόφος, "twist/turn." Sits next to Verso (Latin "turned") etymologically. Strophalos (Hekate's Wheel) available as evocative long-form. |
| **Verso** | Brand-level concept name for the rendering-surface layer | Crate is `verso-tile` (bare `verso` is taken by a literate-programming tool). |

## 2. The printing-press metaphor

The architectural through-line. Engines (Wry, Serval, Nematic) produce content. The **inker** pairs each engine to its content. The **platen** composes the layout (graph-aware). The **verso-tile** receives the impression (places it into GraphTree tile slots). The user sees the printed result via `mere`-on-`graphshell`. **Eidetic** keeps the impressions over time. **Murm** carries bilateral comms; **moothold** carries federation across moots; **coalition** (t4) carries coalition across mootholds.

```
                          ┌─────────────┐
                          │   engines   │   Wry, Serval, Nematic
                          └──────┬──────┘
                                 │ produce content
                                 ▼
                          ┌─────────────┐
                          │   inker     │   selects + orchestrates
                          └──────┬──────┘
                                 │ inks content for
                                 ▼
                          ┌─────────────┐
                          │   platen    │   graph-aware composition
                          └──────┬──────┘
                                 │ presses onto
                                 ▼
                          ┌─────────────┐
                          │ verso-tile  │   rendering-surface in GraphTree
                          └──────┬──────┘
                                 │ user sees via
                                 ▼
                       graphshell + mere
```

## 3. Workspace crates

All scaffolded at `c:\Users\mark_\Code\repos\mere\crates\`. Reserved on crates.io as `0.0.1`.

| Crate | Role |
|-------|------|
| **`mere`** | Product crate — entrypoint composing everything else |
| **`graphshell`** | Portable shell layer + host GUI manager (iced / gpui / html-css / other) |
| **`verso-tile`** | Tile-rendering-surface management (Verso brand) |
| **`inker`** | Engine controller — selects/orchestrates engines |
| **`platen`** | Graph-aware composition surface (the press to verso's page) |
| **`nematic`** | Smolweb engine — Gemini, Gopher, HTML, Markdown, RSS/Atom |
| **`murm`** | Bilateral peer-to-peer comms supercrate |
| **`murmuring`** | Inner protocol-core for bilateral chat-protocol selection (within `murm`) |
| **`moothold`** | Federation of moots (Tier 3). The crate hosts t1–t3 logic; t4 coalition logic may end up in a sibling `coalition` crate. (Will switch to `moot` if that crate name frees up.) |
| **`mooting`** | Inner protocol-core for moot internal coordination + thin protocol-client orchestration (within `moothold`) |
| **`eidetic`** | Private local memory crate — owner-scoped blob storage substrate the orrery (Tier 1) lives on |

**Gerund-crate convention:** `murmuring` and `mooting` (gerunds) name the inner protocol-core layers. The singulars *murmur* and *moot* fall out as user-facing terms. Top-level supercrates (`murm`, `moothold`) avoid gerund forms.

## 4. In-product lexicon

### 4.1 The tier framework (revised 2026-05-07)

Mere's social-graph structures form a four-tier scale. Full design in
[`mere_docs/implementation_strategy/2026-05-07_moot_tiers_and_voluntary_hosting_brief.md`](mere_docs/implementation_strategy/2026-05-07_moot_tiers_and_voluntary_hosting_brief.md).

| Tier | Term | What it is |
| --- | --- | --- |
| **T1** | **orrery** | A single user's root graph view. *"Your orrery is your moot."* Backed by `eidetic`. |
| **T2** | **moot** | A persistent themed federatable graph-view community. Members pin and govern. Dissolves if nobody pins. |
| **T3** | **moothold** | A federation of moots — *a holding of moots*, in the Anglo-Saxon sense (cf. household, stronghold, freehold). ILL-shaped reciprocity between member moots. |
| **T4** | **coalition** | A sovereign coalition of mootholds. Provides organizing defaults; per-moothold override + clean fork-out always possible. |

All four tiers are **forkable**.

### 4.2 Terms

| Term | Role |
| --- | --- |
| **orrery** *(t1)* | A user's root graph view — the whole knowledge-cosmos seen at once. Their personal moot-of-one. |
| **moot** *(t2, count noun)* | A single persistent themed federatable graph-view community |
| **moothold** *(t3, count noun)* | A federation of moots — a sovereign holding (Anglo-Saxon *-hold* sense) |
| **coalition** *(t4, count noun)* | A sovereign cluster of mootholds (renamed from *demesne* 2026-06-04 for familiarity; *demesne* sounds like *domain*, which already names the domain layer) |
| **suzerainty** *(relation)* | The relation between an outer tier and its inner members — overlordship without absorbing internal sovereignty. Applies at moothold ↔ moot and coalition ↔ moothold. |
| **volvelle** | UI form factor — a moot expanded radially in the Navigator (medieval rotating-disc knowledge instrument) |
| **astroid** | Internal UX vocab for graphlet hub-collapse: collapsing a graphlet to its central node forms an astroid-shaped boundary curve |
| **tessera** | Trust / contribution / reputation token; validated across mootholds and coalitions (Roman *tessera hospitalis*). Accrues against an identity's chain root (per the substrate brief's persona-id-chain insight). |
| **engram** | Portable, durable, schematicized memory unit — the canonical contribution payload. `TransferProfile` envelope plus typed `EngramMemory` items (see inherited [`engram_spec.md`](../../graphshell/design_docs/verse_docs/implementation_strategy/engram_spec.md)) |
| **flora** | Accumulated body of engrams composing a moot's culture / geist |
| **eidetic** | Private local memory — owner-scoped, the substrate engrams are distilled from. *Distinct from any moot's flora.* |
| **kith / kin** | Contact tier distinction — *kith* = those known to you, *kin* = close. Orthogonal to moot membership. |
| **strophalos** *(optional)* | Evocative term for an individual user's running Mere instance ("your strophalos has 47 moots") |

## 5. Retired terms

Do not revive:

| Retired | Replacement | Why |
|---------|-------------|-----|
| **Graphshell** *(as product brand)* | **Mere** | Demoted to crate name (the shell layer within Mere) |
| **Strophos** *(as product brand)* | **Mere** | Strophos retained at parent-brand level only |
| **Lemni** / **Lemniscate** | — | Lemni Inc. (Sequoia-backed AI-agent SaaS, Class 9 mark) |
| **Verse** *(network layer)* | folded into Mere-at-network-scope | The Navigator handles networked-community as a form-factor |
| **Murmuration** *(community layer)* | **Moothold** + count noun *moot* | TESS wall (Murmuration, Inc., civic-tech) |
| **Gist** *(contribution unit)* | **Engram** | Already canonical and richer |
| **Flock** *(contact grouping)* | **Kith / Kin** | More nuanced relational tiering |
| **Mootcore** | **Moothold** | Renamed within naming conversation |
| **Middlenet** | **Nematic** | Better metaphor (aligned-but-flowing threads, like nematic liquid crystals) |
| **Verso** *(as engine-controller)* | split: **verso-tile** (rendering) + **inker** (engine control) | Two distinct architectural concerns |
| **Astroid** *(as product brand)* | preserved as internal UX vocab | Asteroid-confusion problem at brand level |
| **Orrery** *(as product brand)* | preserved as internal term of art | crates.io collision + namespace dilution at brand level |

## 6. Naming history (this session)

The product name went through several commits during the 2026-05-03 / 2026-05-04 sessions:

1. **Strophos** (parent) + **Orrery** (product) — initial commit, Orrery rejected when user found it weak as a product brand
2. **Mere** — final commit. Replaces Strophos+Orrery as product brand. Strophos retained at parent-brand level. Orrery preserved as internal term of art for the root graph view.

Other rejected candidates (with reason): Carta (Carta, Inc. wall), Camino (Mozilla Camino® + camino crate), Tela (Schlumberger + tela.com), Waystone (Waystone Group financial), Almagest (Almagest Space Corp.), Holon (Holon Solutions/Platform), Duende (Duende Software/IdentityServer), Synoche (user reported conflict), Snicket (Snicket Labs Feb 2026 rebrand same metaphor), Holloway (Holloway.com publishing platform), Postern (dead Android proxifier ghost), Foundry (Foundry VTT + Foundry FX), Caster (`caster` crate taken), Motif (X11 Motif® + motif.io VC + crate), Syzygy (WPP-owned digital agency), Lemni (Sequoia-backed AI-agent), and others (see [`project_naming_state.md`](C:\Users\mark_\.claude\projects\c--Users-mark--Code\memory\project_naming_state.md) for the full list).

## 7. Pending mechanical work

This brief is design-state. The following remain pending explicit triggers from Mark before any of them happen:

1. **`cargo publish`** for the second half of the workspace (rate-limit-paced; user is executing manually).
2. **Migrate inherited `graphshell/design_docs/` content** into per-area subdirectories here (`mere_docs/`, `murm_docs/`, etc.) — incremental, as docs become relevant to active work.
3. **Cable migration plan execution** — see [`murm_docs/implementation_strategy/2026-05-04_cable_migration_from_verso_plan.md`](murm_docs/implementation_strategy/2026-05-04_cable_migration_from_verso_plan.md). Code-level migration of Cable application logic from Verso to Murm.
4. **Existing `graphshell/` repo content migration** into Mere workspace — separate larger task. The repo at `c:\Users\mark_\Code\repos\graphshell\` remains intact in the meantime.
5. **CLAUDE.md global instructions** ([`~/.claude/CLAUDE.md`](~/.claude/CLAUDE.md)) reference Graphshell as the project; needs update once user wants the global config to track the rename.
6. **Sub-component renames** still gated on existing milestone ordering: `servo-wgpu/` → `serval/` and webrender-wgpu fork → `netrender/` after iced host migration M5a + webrender SPIR-V backend + servo-wgpuification land.

## 8. References

- Memory: [`project_naming_state.md`](C:\Users\mark_\.claude\projects\c--Users-mark--Code\memory\project_naming_state.md) — full naming-decision history with rejected candidates and TESS findings
- Memory: [`project_tessera_trust_token.md`](C:\Users\mark_\.claude\projects\c--Users-mark--Code\memory\project_tessera_trust_token.md) — the tessera concept reservation
- Memory: [`user_aesthetic_word_list.md`](C:\Users\mark_\.claude\projects\c--Users-mark--Code\memory\user_aesthetic_word_list.md) — pool of evocative/niche words for future component naming
- Inherited: [`graphshell/design_docs/TERMINOLOGY.md`](../../graphshell/design_docs/TERMINOLOGY.md) — pre-Mere canonical terminology (still authoritative for terms not addressed here)
- Inherited: [`graphshell/design_docs/verse_docs/implementation_strategy/engram_spec.md`](../../graphshell/design_docs/verse_docs/implementation_strategy/engram_spec.md) — engram canonical spec (1100+ lines)
- Inherited: [`graphshell/design_docs/verso_docs/technical_architecture/VERSO_AS_PEER.md`](../../graphshell/design_docs/verso_docs/technical_architecture/VERSO_AS_PEER.md) — pre-migration Verso role spec
- Inherited: [`graphshell/design_docs/verso_docs/implementation_strategy/2026-03-28_cable_coop_minichat_spec.md`](../../graphshell/design_docs/verso_docs/implementation_strategy/2026-03-28_cable_coop_minichat_spec.md) — Cable adoption plan (subject of the migration plan in this workspace)
- Inherited: [`graphshell/design_docs/graphshell_docs/implementation_strategy/social/comms/COMMS_AS_APPLETS.md`](../../graphshell/design_docs/graphshell_docs/implementation_strategy/social/comms/COMMS_AS_APPLETS.md) — Comms surface family (consumes `moothold` here)
