# Terminology

Canonical terminology for the Mere workspace. This file is the long-term authoritative reference for project vocabulary; until it's fully populated, [`2026-05-04_lexicon_brief.md`](2026-05-04_lexicon_brief.md) is the working source of truth.

For terms not addressed here, see the donor harvest indexes ([full harvest](mere_docs/research/2026-05-27_graphshell_docs_full_harvest.md), [concept brief](mere_docs/research/2026-05-17_graphshell_harvest_brief.md)). The donor `graphshell` repo is GitHub-archived (read-only) and its local clone was deleted 2026-05-27, so the old `../../graphshell/design_docs/` path no longer resolves.

## Top-level

- **Mere** — the product. The browser itself. Triple-meaning positioning: *merely* (humble) + *mere* (a small lake — still-water surface) + slant-rhyme with *mirror*.
- **Strophos** — parent brand / company-name layer. Greek στρόφος, "twist/turn." Sits next to Verso (Latin "turned") etymologically.

## Architectural roles (printing-press metaphor)

- **Engines** — content producers. Three flavors: **Wry** (system webview, third-party), **Serval** (Servo fork, wgpu-rendered), **Nematic** (portable smolweb engine for Gemini/Gopher/HTML/Markdown/RSS-Atom).
- **Inker** — engine controller. Selects which engine renders which content; manages engine lifecycle; routes URIs to engines.
- **Platen** — graph-aware composition surface. Knows graph semantics; presses node-data into renderable form for the verso-tile layer to receive.
- **Verso** — rendering-surface manager. Receives engine output (via inker) and places it into tile slots. *Verso* is the brand-level concept (the page that catches the impression); the crate family is `verso` (`verso-core`, `tile-state`).
- **Graphshell** — the chrome / shell-domain concept, now the `shell` crate family (`chrome`, `comms`, `frame`); no longer a product or a standalone host. The application host is **meerkat** (`crates/meerkat`) on the serval-as-host path, where chrome and content are both rendered by serval via `xilem_serval`.
- **Eidetic** — private local memory crate (formerly *Mnem*). Persistence layer for graph snapshots, traversal logs, settings, browsing memory. Distinct from any moot's flora. Name evokes eidetic memory ("remembered with high fidelity"). The substrate engrams are distilled from.

## Engine layer (inker / nematic / document model)

- **Engine** — concrete content parser implementing `inker::Engine` (`engine_id() -> &str`, `render(&EngineInput) -> Result<EngineDocument, EngineError>`). Twelve nematic engines ship today: `markdown`, `gemtext`, `gopher`, `feed`, `text`, `file`, `finger`, `knot`, `scroll`, `misfin`, `nex`, `guppy`. Plus `serval.web` (external) and `host.external-protocol` / `graphshell.internal` (host-side).
- **Protocol-faithfulness rule** — protocol engines (gemini, gopher, RSS/Atom, finger, scroll, misfin, nex, guppy) populate document blocks only with what the source spec actually says. They do not invent semantic structure the spec doesn't define. RSS `<item>` becomes `FeedEntry`; finger plain text stays plain text; gopher menu items use the `gopher://` URL synthesis from RFC 4266. The only Mere-defined format that's allowed to be richer is **knot**.
- **Semantic-block intent** — the four `Block` variants beyond structural shape that name *what content means*, not just *how it's laid out*: `FeedHeader`, `FeedEntry`, `MetadataRow`, `Badge`. Intelligence layers (search, summarise, recommend, recall) match on these intents. Adopting them in protocol engines is *more* spec-faithful (RSS / Atom literally have entry-typed items), not an invention.
- **Trust ladder** — the `DocumentTrustState` enum: **Trusted** (verified through a chain of trust — TLS root, signed envelope), **Tofu** (first-contact-accepted, "trust on first use"), **Insecure** (unauthenticated transport — plain HTTP, file://), **Broken** (verification attempted and failed — cert mismatch, sig invalid), **Unknown** (default; not yet evaluated).
- **Provenance** — `DocumentProvenance` carries `source_kind` (engine ID), `canonical_uri`, `fetched_at` (RFC 3339), `source_label`. Engines populate `source_kind` + `canonical_uri`; the host fills in `fetched_at` after transport.
- **Knot** — Mere's native note / clip format. Frontmatter (YAML subset) + polyglot CommonMark body where fenced code blocks with protocol language tags (`gemtext`, `gopher`, `nex`, `feed-entry`, `feed-header`, `metadata-row`, `badge`) expand into real semantic blocks. Wikilinks `[[name]]` rewrite to `mere://node/<slug>`; hashtags `#tag` extract to `Badge` siblings. The only Mere-defined content format. Engine ID `nematic.knot`; default content-type `text/x-knot`.
- **Three-head Hekate** — Serval's planned evolution into a smolweb-extract / middlenet / fullweb negotiator for the same HTML input. Not yet built; locks in that nematic does not own an HTML reader-mode engine — HTML in any rendering depth is Serval's job. Hekate = three-headed Greek goddess of crossroads.

## Memory naming retired

- **Mnem** — replaced by **Eidetic**. The prototype name `mnem` was unavailable on crates.io.

## Comms layers

- **Murm** — bilateral peer-to-peer comms supercrate. One-to-one messaging across protocols. Cable + co-op session chat + bilateral identity derivation live here.
  - **Murmuring** — inner protocol-core for selecting bilateral chat protocols (Cable, MLS, Tox, etc.). Gerund form names the plumbing; singular *murmur* falls out as user-facing term.
- **Moothold** — community/federation supercrate. Manages moots, coalitions, social primitives. (Will switch to `moot` if that crate name frees up.)
  - **Mooting** — inner protocol-core for selecting community social protocols (Matrix, Nostr, IRC, ATproto, ActivityPub, etc.). Gerund form names the plumbing; singular *moot* falls out as user-facing term.
- **Gerund law** — `murmuring`:`murm` :: `mooting`:`moothold`. The gerund crate names the protocol *plumbing*; the singular is the user's *artifact* (a murmur, a moot). The generic signed-log / LogSync substrate was pulled out of `murmuring` into `transport::SyncedSpace` (2026-07-08 — the one drain shared by murm + mesh + host-composed moot sync) precisely so the gerund crate stays honestly chat-scoped rather than a stealth generic engine.
- **Gazetteer** — handle-resolution index: turns a name / handle / key into reachable, trust-stated endpoints (WebFinger today; NIP-05 / atproto-did / moot web-of-trust to come). An index / *directory*, not a broadcast *gazette*, so it sits on the persona / identity tier (`crates/persona/gazetteer`), promoted out of the murm supercrate 2026-07-08. Incubating — no consumer wired yet, and its blocking HTTP needs an async port first.

## In-product vocabulary

- **murmur** — the user-facing word for a bilateral conversation: a quiet 1:1 or small-group exchange between known peers (the intimate register, opposite *moot*'s public assembly). A murmur is the *container* — the room / confidence — and individual posts are utterances within it, so a two-person murmur and a small-group murmur are the same object at different cardinality. Scope: **product surfaces say murmur** (pane labels, status lines, the join / invite buttons — adopted 2026-07-08). **Code identifiers stay `cabal`** (`CabalKey` / `CabalId` / `CabalHandle`, `open_cabal`, `CabalExt` — dead residue of the Cable protocol whose varint wire was replaced by p2panda operations) and migrate opportunistically when a file is touched for other reasons, never as a churn pass — the same discipline as *edge* → *link*.
- **moot** *(count noun)* — a single persistent themed federatable graph-view community
- **coalition** *(count noun)* — a sovereign cluster of mootholds (t4; renamed from *demesne* 2026-06-04)
- **suzerainty** *(relation)* — the outer-tier ↔ inner-member relationship (moothold ↔ moot, coalition ↔ moothold); overlordship without absorbing internal sovereignty
- **volvelle** — UI form factor: a moot expanded radially in the Navigator (medieval rotating-disc knowledge instrument)
- **astroid** — internal UX vocab for graphlet hub-collapse: collapsing a graphlet to its central node forms an astroid-shaped boundary curve
- **tessera** — trust / contribution / reputation token; validated across coalitions (Roman *tessera hospitalis* — guest-friendship token between communities)
- **engram** — canonical portable contribution payload; `TransferProfile` envelope plus typed `EngramMemory` items (see inherited `graphshell/design_docs/verse_docs/implementation_strategy/engram_spec.md`)
- **flora** — accumulated body of engrams that constitutes a moot's culture / geist
- **kith / kin** — contact tier distinction: *kith* = those known to you; *kin* = close. Orthogonal to moot membership.
- **orrery** — internal term of art for the root graph view (the whole knowledge-cosmos seen at once, mechanical-model-of-orbits style)
- **gnode** — a node's rendered body on a graph canvas: the visible, spatially-placed object standing at the node's position in an orrery or swatch. A projection, never truth: rebuilt per frame from kernel truth plus gyre's live state, stores nothing. At most one per (node, pane instance); zero when off-scope/off-pane (the node demotes to an underlay dot, which is not a gnode). Anatomy: **body** (silhouette or custom hull, spatially coincident with the gyre collider: the face IS the collider), **face** (the body's texture: state color, favicon, or sprite), **caption** (label beside, LOD-driven); emphasis channels: selection = ring + lift, hover = wash, focus = focus ring, with color reserved for activation state. One primitive, two render tiers: a chrome-DOM `.gnode` element (focused pane) or an in-scene Scene layer (secondary panes; `render_gnodes_as_dom` picks per pane). Pointer-inert: gyre owns press/select/drag through the collider; a11y bounds are read off the gnode's painted rect. Distinct from the **node** (the graph object that references addressed things), from a **card** (summoned *about* a node or selection), and from non-spatial representations (roster row, tile tab, session chip). Etymology: g(raph)-node, coined 2026-06-02 as the orrery pool's CSS class; kept 2026-07-02 for the gnostic reading (the knowable body of the node). Full model: [node_card_summoning_design](mere_docs/design/2026-07-01_node_card_summoning_design.md).
- **strophalos** *(optional, lowercase)* — evocative term for an individual user's running Mere instance ("your strophalos has 47 moots")
- **link** — the user-facing word for a connection between nodes (adopted 2026-07-04; Mark: "if I started over today, I'd just call 'em links"). Plainer than *edge*, carries the right hyperlink lineage, and matches the statement-bucket data model (a link IS a statement: subject node, predicate, object node, with provenance and its own `StatementId`; the drawn connection between two nodes is the pair-local bucket that enumerates them — see the [petgraph-RDF plan](mere_docs/implementation_strategy/2026-06-18_petgraph_rdf_plan.md)). Scope: **product surfaces say link** — menu labels, omnibar verbs (`hide_link`, `show_all_links`, `relate`/`unrelate`), counters, roster tab (already "Links"), docs written from here on. **Code identifiers stay `edge`** (kernel types, petgraph vocabulary, CSS classes like `.roster-edge`) and migrate opportunistically when a file is touched for other reasons, never as a churn pass. *Edge* is not retired as an internal term — it is graph-theory vocabulary and correct at the petgraph layer; it is retired from user-visible copy. The shellbar's screen *edge* (Left/Right/Top/Bottom) is an unrelated sense and keeps its name.

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
| Mnem | **Eidetic** | Prototype name unavailable on crates.io; eidetic evokes "remembered with high fidelity" |
| `nematic.smolweb` *(umbrella engine ID)* | Per-protocol IDs (`nematic.gemtext`, `nematic.gopher`, `nematic.finger`) | Concrete engines now exist for each smolweb protocol |
| HTML reader-mode in nematic | Future Serval head (three-head Hekate negotiator) | HTML in any rendering depth is Serval's job, not nematic's |

## Status

Skeleton. As docs migrate from `graphshell/design_docs/` and as new specs are written here, this file should grow into the long-term canonical terminology surface.
