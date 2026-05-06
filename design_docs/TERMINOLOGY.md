# Terminology

Canonical terminology for the Mere workspace. This file is the long-term authoritative reference for project vocabulary; until it's fully populated, [`2026-05-04_lexicon_brief.md`](2026-05-04_lexicon_brief.md) is the working source of truth.

For terms not addressed here, defer to the inherited [`graphshell/design_docs/TERMINOLOGY.md`](../../graphshell/design_docs/TERMINOLOGY.md).

## Top-level

- **Mere** — the product. The browser itself. Triple-meaning positioning: *merely* (humble) + *mere* (a small lake — still-water surface) + slant-rhyme with *mirror*.
- **Strophos** — parent brand / company-name layer. Greek στρόφος, "twist/turn." Sits next to Verso (Latin "turned") etymologically.

## Architectural roles (printing-press metaphor)

- **Engines** — content producers. Three flavors: **Wry** (system webview, third-party), **Serval** (servo-wgpu fork), **Nematic** (portable smolweb engine for Gemini/Gopher/HTML/Markdown/RSS-Atom).
- **Inker** — engine controller. Selects which engine renders which content; manages engine lifecycle; routes URIs to engines.
- **Platen** — graph-aware composition surface. Knows graph semantics; presses node-data into renderable form for the verso-tile layer to receive.
- **Verso-tile** — rendering-surface manager. Receives engine output (via inker) and places it into GraphTree tile slots. *Verso* is the brand-level concept (the page that catches the impression); *verso-tile* is the crate.
- **Graphshell** — portable shell layer + host GUI manager. Owns the workbench, tile tree, Navigator surface, and integration to whichever GUI framework hosts the app (iced / gpui / html-css / other).
- **Mnem** — private local accumulated browsing memory. Persistence layer (fjall/redb/rkyv). Distinct from any moot's flora.

## Comms layers

- **Murm** — bilateral peer-to-peer comms supercrate. One-to-one messaging across protocols. Cable + co-op session chat + bilateral identity derivation live here.
  - **Murmuring** — inner protocol-core for selecting bilateral chat protocols (Cable, MLS, Tox, etc.). Gerund form names the plumbing; singular *murmur* falls out as user-facing term.
- **Moothold** — community/federation supercrate. Manages moots, demesnes, social primitives. (Will switch to `moot` if that crate name frees up.)
  - **Mooting** — inner protocol-core for selecting community social protocols (Matrix, Nostr, IRC, ATproto, ActivityPub, etc.). Gerund form names the plumbing; singular *moot* falls out as user-facing term.

## In-product vocabulary

- **moot** *(count noun)* — a single persistent themed federatable graph-view community
- **demesne** *(count noun)* — a federation of moots; a sovereign cluster
- **suzerainty** *(relation)* — demesne ↔ moot relationship; overlordship without absorbing internal sovereignty
- **volvelle** — UI form factor: a moot expanded radially in the Navigator (medieval rotating-disc knowledge instrument)
- **astroid** — internal UX vocab for graphlet hub-collapse: collapsing a graphlet to its central node forms an astroid-shaped boundary curve
- **tessera** — trust / contribution / reputation token; validated across demesnes (Roman *tessera hospitalis* — guest-friendship token between communities)
- **engram** — canonical portable contribution payload; `TransferProfile` envelope plus typed `EngramMemory` items (see inherited `graphshell/design_docs/verse_docs/implementation_strategy/engram_spec.md`)
- **flora** — accumulated body of engrams that constitutes a moot's culture / geist
- **kith / kin** — contact tier distinction: *kith* = those known to you; *kin* = close. Orthogonal to moot membership.
- **orrery** — internal term of art for the root graph view (the whole knowledge-cosmos seen at once, mechanical-model-of-orbits style)
- **strophalos** *(optional, lowercase)* — evocative term for an individual user's running Mere instance ("your strophalos has 47 moots")

## Retired terms (do not revive)

| Retired | Replacement | Reason |
|---------|-------------|--------|
| Graphshell *(as product brand)* | **Mere** | Demoted to crate name (the shell layer) |
| Verse *(network layer)* | folded into Mere-at-network-scope | The Navigator handles networked-community as a form-factor of the same surface |
| Murmuration *(community layer)* | **Moothold** + count noun *moot* | TESS wall (Murmuration, Inc., civic-tech) |
| Gist *(contribution unit)* | **Engram** | Already canonical and richer |
| Flock *(contact grouping)* | **Kith / Kin** | More nuanced relational tiering |
| Mootcore | **Moothold** | Rename within this conversation |
| Verso *(as engine-controller)* | split: **verso-tile** (rendering surface) + **inker** (engine controller) | Two distinct concerns |
| Middlenet | **Nematic** | Better metaphor (aligned-but-flowing threads) |

## Status

Skeleton. As docs migrate from `graphshell/design_docs/` and as new specs are written here, this file should grow into the long-term canonical terminology surface.
