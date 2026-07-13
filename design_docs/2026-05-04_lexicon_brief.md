# Mere Lexicon Brief

**Status**: Active / authoritative for terms covered
**Date**: 2026-05-04
**Scope**: Establishes the post-rename naming scheme and in-product vocabulary for the Mere project. Authoritative for terms it covers; supersedes the prior [2026-05-03 lexicon brief](../../graphshell/design_docs/2026-05-03_lexicon_brief.md) (which had Strophos+Orrery as the product) and complements [`TERMINOLOGY.md`](TERMINOLOGY.md) until that file is fully populated. Terms not addressed here are covered by the donor harvest indexes; the donor `graphshell` repo is GitHub-archived (read-only) and its local clone was deleted 2026-05-27, so `../../graphshell/design_docs/` paths no longer resolve.

**Execution status (updated 2026-06-09)**: the workspace has since reorganized from the flat 2026-05-04 scaffolding into supercrate subtrees under `crates/`, with **meerkat** as the host on the genet-as-host path; see the topology table in [`DOC_README.md`](DOC_README.md) for the current layout. The donor `graphshell` repo was GitHub-archived and its local clone deleted 2026-05-27.

---

## 1. Top-level naming

| Name | What it labels | Notes |
|------|----------------|-------|
| **Mere** | Product / app — the browser itself | Triple-meaning positioning: *merely* (humble — "merely a browser!"), *mere* (a small lake — still-water surface where things accrue and reflect), slant-rhyme with *mirror*. Disambiguates from Māori weapon by intentional framing. Replaces *Graphshell-as-product-name* and *Verse-as-network-layer*. |
| **Merely** | Parent brand / company-name layer | Adopted 2026-07-09, replacing *Strophos*; confirmed 2026-07-10 after a full challenge round (history entry 4). The umbrella takes its name from the product's own positioning: Mere is *merely a browser*, so the parent is that adverb. Humility as a house style. GitHub org: **merely-made** (registered 2026-07-10; bare `merely` was taken). |
| **Verso** | Brand-level concept name for the rendering-surface layer | Crate family is `verso` (`verso-core`, `tile-state`). |

## 2. The printing-press metaphor

The architectural through-line. Engines (Wry, Genet, Nematic) produce content. The **inker** pairs each engine to its content. The **platen** composes the layout (graph-aware). The **verso** layer receives the impression (places it into tile slots). The user sees the printed result via **meerkat** (the host), chrome and content both rendered by genet on the genet-as-host path. **Eidetic** keeps the impressions over time. **Murm** carries bilateral comms; **moothold** carries federation across moots; **coalition** (t4) carries coalition across mootholds.

```
                          ┌─────────────┐
                          │   engines   │   Wry, Genet, Nematic
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

All scaffolded at `c:\Users\mark_\Code\repos\mere\crates\`. Reserved on crates.io as `0.0.1`. **(2026-05-04 snapshot; the workspace has since reorganized into supercrate subtrees with `meerkat` as host. See the [`DOC_README.md`](DOC_README.md) topology table for the current layout.)**

| Crate | Role |
|-------|------|
| **`mere`** | Product crate — entrypoint composing everything else |
| **`graphshell`** | Demoted: the chrome / shell-domain concept, now the `shell` crate family. The host is `meerkat`. |
| **`verso`** | Tile-rendering-surface management (`verso-core`, `tile-state`) |
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
| **Strophos** *(as product brand)* | **Mere** | Strophos retained at parent-brand level only — itself retired 2026-07-09, see below |
| **Strophos** *(as parent brand)* | **Merely** | Retired 2026-07-09. The umbrella now derives from the product ("merely a browser!") rather than from a separate Greek root. Strophos survives only in this brief's history sections and in `archive_docs/`; do not reintroduce it. |
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

## 6. Naming history

*Statements in this section are historical and are deliberately **not** rewritten
when a name changes; the retired-terms table above carries the current mapping.*

The product name went through several commits during the 2026-05-03 / 2026-05-04 sessions:

1. **Strophos** (parent) + **Orrery** (product) — initial commit, Orrery rejected when user found it weak as a product brand
2. **Mere** — final commit. Replaces Strophos+Orrery as product brand. Strophos retained at parent-brand level. Orrery preserved as internal term of art for the root graph view.

Then, on 2026-07-09:

3. **Merely** (parent). Strophos retired at the parent level too, and the umbrella
   is taken from the product instead of standing beside it: Mere is *merely a
   browser*, so the family is Merely. The parent no longer carries an independent
   etymology, which removes the Strophos/Verso twist-and-turn pairing as a brand
   device (Verso keeps its own meaning as the rendering-surface layer). Swept
   through every living doc, README, and crate description in one pass; archived
   docs and the history sections above were left as written.

   **Known trade-off, accepted:** *Merely* is a common English adverb. It is
   weakly distinctive as a mark (the trademark literature's own phrase for the
   failure mode is "merely descriptive"), it is hard to search for, and a short
   dictionary word is unlikely to be free as an org slug. That is the opposite
   problem from the collisions that killed Carta, Camino, Tela, and Lemni: not a
   conflict, a distinctiveness cost. Recorded here so it is a decision rather
   than an oversight.

4. **Merely confirmed; org slug registered** (2026-07-10). Before committing,
   the name was challenged against a full bench: the smallholding register
   (croft, steading, whittle), the gift register (lagniappe's family: handsel,
   fairing, benison, boon, windfall, whet), buoy, and the mark/marquis family
   (waymark, marque). Buoy fell to live companies (Buoy Health among others).
   **Croft**, the strongest challenger, fell to a genuine collision: an active
   `croft` dev-tool crate on crates.io (updated 2026-05, same audience), the
   bare GitHub slug taken, plus the Lara Croft (Square Enix, Class 9) and Croft
   port-house shadows. Merely's own objections dissolved on inspection: its
   weakness is not a collision (nothing found occupying it), and the
   Simply / Very / The Ordinary precedent shows understatement marks register
   and thrive, with "Merely Software" as the composite fallback if registration
   is ever wanted. GitHub org **merely-made** registered 2026-07-10 (bare
   `merely` taken; `-made` over `-hq` because it completes the adverb and reads
   as a colophon). Croft benched with affection as a possible future colloquial
   place-name inside the family.

Other rejected candidates (with reason): Carta (Carta, Inc. wall), Camino (Mozilla Camino® + camino crate), Tela (Schlumberger + tela.com), Waystone (Waystone Group financial), Almagest (Almagest Space Corp.), Holon (Holon Solutions/Platform), Duende (Duende Software/IdentityServer), Synoche (user reported conflict), Snicket (Snicket Labs Feb 2026 rebrand same metaphor), Holloway (Holloway.com publishing platform), Postern (dead Android proxifier ghost), Foundry (Foundry VTT + Foundry FX), Caster (`caster` crate taken), Motif (X11 Motif® + motif.io VC + crate), Syzygy (WPP-owned digital agency), Lemni (Sequoia-backed AI-agent), and others (see [`project_naming_state.md`](C:\Users\mark_\.claude\projects\c--Users-mark--Code\memory\project_naming_state.md) for the full list).

## 7. Pending mechanical work (updated 2026-06-09)

Most of the original list has executed:

1. **`cargo publish`** — names reserved; ongoing as needed.
2. **Donor `graphshell/design_docs/` harvest** — done; the donor repo was GitHub-archived and its local clone deleted 2026-05-27, its 633 docs swept into the [full harvest](mere_docs/research/2026-05-27_graphshell_docs_full_harvest.md) and [concept brief](mere_docs/research/2026-05-17_graphshell_harvest_brief.md) indexes.
3. **Cable migration** — superseded: the bilateral substrate pivoted to p2panda (Cable wire deleted; `P2pandaTransport` live).
4. **Donor `graphshell/` repo code salvage** — done (engines, `import`, the `register-*` cluster, `murm` misfin/webfinger pulled); repo archived.
5. **CLAUDE.md global instructions** still reference Graphshell; update when Mark wants the global config to track the rename.
6. **`servo-wgpu` → `genet` and `webrender-wgpu` → `netrender` renames** — done (`repos/genet`, `repos/netrender`).

## 8. References

- Memory: [`project_naming_state.md`](C:\Users\mark_\.claude\projects\c--Users-mark--Code\memory\project_naming_state.md) — full naming-decision history with rejected candidates and TESS findings
- Memory: [`project_tessera_trust_token.md`](C:\Users\mark_\.claude\projects\c--Users-mark--Code\memory\project_tessera_trust_token.md) — the tessera concept reservation
- Memory: [`user_aesthetic_word_list.md`](C:\Users\mark_\.claude\projects\c--Users-mark--Code\memory\user_aesthetic_word_list.md) — pool of evocative/niche words for future component naming
- Inherited: [`graphshell/design_docs/TERMINOLOGY.md`](../../graphshell/design_docs/TERMINOLOGY.md) — pre-Mere canonical terminology (still authoritative for terms not addressed here)
- Inherited: [`graphshell/design_docs/verse_docs/implementation_strategy/engram_spec.md`](../../graphshell/design_docs/verse_docs/implementation_strategy/engram_spec.md) — engram canonical spec (1100+ lines)
- Inherited: [`graphshell/design_docs/verso_docs/technical_architecture/VERSO_AS_PEER.md`](../../graphshell/design_docs/verso_docs/technical_architecture/VERSO_AS_PEER.md) — pre-migration Verso role spec
- Inherited: [`graphshell/design_docs/verso_docs/implementation_strategy/2026-03-28_cable_coop_minichat_spec.md`](../../graphshell/design_docs/verso_docs/implementation_strategy/2026-03-28_cable_coop_minichat_spec.md) — Cable adoption plan (subject of the migration plan in this workspace)
- Inherited: [`graphshell/design_docs/graphshell_docs/implementation_strategy/social/comms/COMMS_AS_APPLETS.md`](../../graphshell/design_docs/graphshell_docs/implementation_strategy/social/comms/COMMS_AS_APPLETS.md) — Comms surface family (consumes `moothold` here)
