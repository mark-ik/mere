# Terminology

Canonical terminology for the Mere workspace. This file is the long-term authoritative reference for project vocabulary; until it's fully populated, [`2026-05-04_lexicon_brief.md`](2026-05-04_lexicon_brief.md) is the working source of truth.

For terms not addressed here, see the donor harvest indexes ([full harvest](mere_docs/research/2026-05-27_graphshell_docs_full_harvest.md), [concept brief](mere_docs/research/2026-05-17_graphshell_harvest_brief.md)). The donor `graphshell` repo is GitHub-archived (read-only) and its local clone was deleted 2026-05-27, so the old `../../graphshell/design_docs/` path no longer resolves.

## Top-level

- **Mere** — the product. The browser itself. Triple-meaning positioning: *merely* (humble) + *mere* (a small lake — still-water surface) + slant-rhyme with *mirror*.
- **Merely** — parent brand / company-name layer (adopted 2026-07-09, replacing *Strophos*). Takes the umbrella from the product rather than the reverse: Mere's own positioning already leads with *merely* ("merely a browser!"), so the parent is the adverb the product was named for. Humility as a house style. The prior *Strophos* (Greek στρόφος, "twist/turn," chosen to sit beside Verso's Latin "turned") is retired at every level; the twist/turn etymology no longer carries brand weight, though Verso keeps its own.

## Architectural roles (printing-press metaphor)

- **Engines** — content producers. Three flavors: **Wry** (system webview, third-party), **Genet** (Servo fork, wgpu-rendered), **Nematic** (portable smolweb engine for Gemini/Gopher/HTML/Markdown/RSS-Atom).
- **Inker** — engine controller. Selects which engine renders which content; manages engine lifecycle; routes URIs to engines.
- **Platen** — graph-aware composition surface. Knows graph semantics; presses node-data into renderable form for the verso-tile layer to receive.
- **Verso** — rendering-surface manager. Receives engine output (via inker) and places it into tile slots. *Verso* is the brand-level concept (the page that catches the impression); the crate family is `verso` (`verso-core`, `tile-state`).
- **Graphshell** — the Merely family's remote projection host: a wasm-first web/mobile client that connects to applications running on the user's devices, receives Scenograph scenes and diffs, and returns granted intents. It owns saved remote views and cross-application curation, while each application owns its source truth. This is a new product role, distinct from both the archived donor browser and Mere's internal `shell` crates (`mere-chrome`, `mere-comms`).
- **Eidetic** — private local memory crate (formerly *Mnem*). Persistence layer for graph snapshots, traversal logs, settings, browsing memory. Distinct from any moot's flora. Name evokes eidetic memory ("remembered with high fidelity"). The substrate engrams are distilled from.

## Engine layer (inker / nematic / document model)

- **Engine** — concrete content parser implementing `inker::Engine` (`engine_id() -> &str`, `render(&EngineInput) -> Result<EngineDocument, EngineError>`). Sixteen nematic engines ship today: `markdown`, `gemtext`, `gopher`, `feed`, `text`, `file`, `finger`, `knot`, `knot-djot`, `scroll`, `misfin`, `nex`, `guppy`, `spartan`, `titan`, `html-fragment`. Counted by shipping `ENGINE_ID`, per the rule that implemented engines are listed and unwired ones are marked planned. Two need a note rather than an exclusion: `knot-djot` is knot's default body handling outside the blocks rather than a peer format, and `html-fragment` sits behind a feature that is on by default (`default = ["html-fragment"]`). Plus `genet.web` (external) and `host.external-protocol` / `graphshell.internal` (host-side).
- **Protocol-faithfulness rule** — protocol engines (gemini, gopher, RSS/Atom, finger, scroll, misfin, nex, guppy) populate document blocks only with what the source spec actually says. They do not invent semantic structure the spec doesn't define. RSS `<item>` becomes `FeedEntry`; finger plain text stays plain text; gopher menu items use the `gopher://` URL synthesis from RFC 4266. The only Mere-defined format that's allowed to be richer is **knot**.
- **Semantic-block intent** — the four `Block` variants beyond structural shape that name *what content means*, not just *how it's laid out*: `FeedHeader`, `FeedEntry`, `MetadataRow`, `Badge`. Intelligence layers (search, summarise, recommend, recall) match on these intents. Adopting them in protocol engines is *more* spec-faithful (RSS / Atom literally have entry-typed items), not an invention.
- **Trust ladder** — the `DocumentTrustState` enum: **Trusted** (verified through a chain of trust — TLS root, signed envelope), **Tofu** (first-contact-accepted, "trust on first use"), **Insecure** (unauthenticated transport — plain HTTP, file://), **Broken** (verification attempted and failed — cert mismatch, sig invalid), **Unknown** (default; not yet evaluated).
- **Provenance** — `DocumentProvenance` carries `source_kind` (engine ID), `canonical_uri`, `fetched_at` (RFC 3339), `source_label`. Engines populate `source_kind` + `canonical_uri`; the host fills in `fetched_at` after transport.
- **Knot** — Mere's native note / clip format. Frontmatter (YAML subset) + polyglot CommonMark body where fenced code blocks with protocol language tags (`gemtext`, `gopher`, `nex`, `feed-entry`, `feed-header`, `metadata-row`, `badge`) expand into real semantic blocks. Wikilinks `[[name]]` rewrite to `mere://node/<slug>`; hashtags `#tag` extract to `Badge` siblings. The only Mere-defined content format. Engine ID `nematic.knot`; default content-type `text/x-knot`.
- **Three-head Hekate** — Genet's planned evolution into a smolweb-extract / middlenet / fullweb negotiator for the same HTML input. Not yet built; locks in that nematic does not own an HTML reader-mode engine — HTML in any rendering depth is Genet's job. Hekate = three-headed Greek goddess of crossroads.

## Memory naming retired

- **Mnem** — replaced by **Eidetic**. The prototype name `mnem` was unavailable on crates.io.

## Comms layers

- **Stickleback** — the shared replicated-space layer beneath every signed peer
  domain: joined spaces and their drain, policy-before-insert processing,
  muniment-backed replicated storage, checkpoints, retention mechanics, and
  native drop carriage. The package is `stickleback`. It was `murm-replication`
  until 2026-07-26, when the multi-consumer reality (Murm, Mesh, Moot, and
  transport) earned it a name for the boundary rather than one consumer. A
  domain supplies its own operation grammar, addressing, authorization, and
  materialization — Stickleback never infers authority from transport access or
  visible membership.
- **Murm** — the peer-exchange family, a domain over Stickleback. Its public
  conversation service owns invitation-scoped murmurs, mail, and co-op exchange.
  - **Murmuring** — retired inner crate. Its signed-operation grammar and
    conversation engine were folded into `murm` on 2026-07-14. Internal
    mechanics use `ConversationEngine` and `ConversationStore`.
- **Moot** — the governed-space domain over Stickleback. It owns community
  identity, membership, constitution, governed settings, moderation,
  recognition, tessera, and community projections.
  - **Mooting** — current home of recognition policy and, temporarily, the
    generic `MunimentStore`. The store moves to `stickleback`; the name is
    not a generic-plumbing law.
- **Moothold** — reserved for actual multi-moot holding or federation behavior.
  The current package also contains single-moot code for historical reasons;
  that code becomes the public Moot service during the peer-runtime reframe.
- **Gerund law (retired 2026-07-12)** — `murmuring`:`murm` ::
  `mooting`:`moothold` described the old workspace partition, not a durable
  semantic rule. The `murmuring` package was folded into `murm` on 2026-07-14.
  Use role-descriptive internal names and keep the product words for product
  concepts.
- **Gazetteer** — handle-resolution index: turns a name / handle / key into reachable, trust-stated endpoints (WebFinger today; NIP-05 / atproto-did / moot web-of-trust to come). An index / *directory*, not a broadcast *gazette*, so it sits on the persona / identity tier (`crates/persona/gazetteer`), promoted out of the murm supercrate 2026-07-08. Incubating — no consumer wired yet, and its blocking HTTP needs an async port first.

## In-product vocabulary

- **murmur** — the user-facing word for an invitation-scoped conversation
  between identified participants. A murmur is the container, and individual
  posts are utterances within it. Participant count does not select Murm versus
  Moot; a Moot is distinguished by durable governance. Product surfaces say
  murmur.
- **cabal** — the stable protocol and code word for an invitation-scoped
  shared conversation. `CabalId`, `CabalKey`, and `CabalHandle` are deliberate
  domain names, not UI copy.
- **Cable** — the semantic ancestry for cabals, channels, and signed posts.
  `mere/cable/v1` names Mere's Cable-shaped p2panda dialect. It does not claim
  wire interoperability with the cabal-club Cable protocol. Use `Conversation*`
  for storage/runtime mechanics and keep Cable terminology at this explicit
  protocol boundary.
- **moot** *(count noun)* — a persistent themed federatable graph-view community: what a mere becomes when shared. Ruled with Mark 2026-07-30, two faces: the genesis face (a moot *begins* when your mere is shared; the tiers are escalating socialization of the mere, the datastructure moot makes social) and the grown face (a mature moot may govern one or more meres, which is the region-grafting model; a shared world never stops being a mere). Substrate rule: solo meres are born share-ready (one-writer signed spaces), so becoming a moot is a membership change, never a format migration
- **fili** — reserved name for Moot lineage: community ancestry, forks, and
  genealogy across related moots. Do not use it for ordinary event history,
  retention, or storage mechanics.
- **tulpa** — the legend and memorial layer: what memory makes of history
  (legends, memorials, epithets, the manifestations of the dead), sustained
  by continued attention — presence scales with retelling, and a legend
  nobody tells fades. A *view* over a codicil log holding the retold subset,
  never the log itself. Do not use it for ordinary event history (that is
  **codicil**) or for descent (that is **fili**): where fili tracks who
  carried the line, tulpa holds what is remembered when nobody did. Chosen by
  Mark 2026-07-30 and published the same day (crates.io `tulpa` 0.0.1,
  `merely-made/tulpa`). Completes the thoughtform triad the stack already
  held: **servitor** (created and bounded) → **tulpa** (remembered and
  autonomous) → **egregore** (collective and emergent). From Tibetan
  *sprul-pa* by way of two westernizations; the living-office cousin *tulku*
  is deliberately not used.
- **gemot** *(count noun)* — a sovereign assembly of mootholds (t4; renamed from *coalition* 2026-07-30, which had renamed *demesne* 2026-06-04). OE *gemōt*, the collective form of *mōt* itself: the assembly of assemblies, rejoining the moot/moothold word-family where *coalition* was the Latinate outlier. crates.io `gemot` already held (0.1.0, claimed 2026-07-14 as the assembly-layer crate: Moot lifecycle, governance, replication, Tessera), so the t4 count noun and the governance crate share the name deliberately
- **suzerainty** *(relation)* — the outer-tier ↔ inner-member relationship (moothold ↔ moot, gemot ↔ moothold); overlordship without absorbing internal sovereignty
- **volvelle** — UI form factor: a moot expanded radially in the Navigator (medieval rotating-disc knowledge instrument)
- **astroid** — internal UX vocab for graphlet hub-collapse: collapsing a graphlet to its central node forms an astroid-shaped boundary curve
- **servitor** — the resident helper unit: an installed extension or local agent, living as a node bearing a nested graph, holding a personae identity and a capability grant, proposing changes through the participant gate (so it cannot exceed its grant, and every act is attributed and revertible). Chosen by Mark 2026-07-17 for the chaos-magic sense: created, named, task-scoped, dissolvable. Crate name reserved on crates.io the same day (0.0.1). A human peer is never a servitor; both are *denizens*. *Animula* (Hadrian's "guest and companion of the body") is banked as the companion-flavored runner-up. See the [participant gate and packs plan](mere_docs/implementation_strategy/2026-07-17_participant_gate_packs_plan.md)
- **denizen** — the umbrella word for anything admitted to act through the gate: a personae identity holding a grant and the right to submit petitions. Human moot peers, servitors, and scenario runners are all denizens; the trusted UI is not (it writes the journal directly). From English legal history: denization admitted an outsider by letters patent with a defined subset of rights, which is exactly the signed manifest plus grant. Ruled 2026-07-17 (replaces the working word *participant*)
- **petition** — a denizen's proposed change: a typed batch (graph edits lowering to captured deltas, app effects as Actions) validated against the grant and the journal revision before atomic, attributed apply. The journal records granted petitions. Ruled 2026-07-17 (replaces the working word *proposal*)
- **watch** — a denizen's standing subscription: the scope of the graph whose committed changes wake its body, with the containment law watch ⊆ read ⊆ grant (you cannot be woken by what you cannot read). Watches are declared in the pack manifest and reviewed at install beside the rings; a chain of wakes is a *cascade*, bounded by a budget that is a setting. Ruled by Mark 2026-08-13. See the [graph behaviors plan](mere_docs/implementation_strategy/2026-08-13_graph_behaviors_plan.md)
- **pack / mod** — the installable-bundle words, split by trust depth (ruled 2026-07-17): a **pack** is the plain user-facing word for a shallow-rung bundle (scenario/macro data, scripts; "campaign pack", "command pack"), a **mod** is a deeper-rung bundle (wasm components and beyond) whose grant reaches further. One envelope underneath, expected to be an engram profile (B4 confirms). Coheres with the existing `register-mod-loader` / `WasmModRuntime` naming
- **swatch** — a compact graph-canvas projection embedded in a pane: a scoped rendering of a graph or nested graph, either mirroring the main view (a minimap) or projecting through its own lens (independent layout, scope, or overlays; the gloss is a pane containing a swatch). A representation, never an identity: gnodes render in an orrery or swatch, while the graph itself lives in the kernel. A swatch over a servitor's nested graph is that servitor's inspection UI. Wording ruled 2026-07-17
- **nested graph** — a graph (a set of relations) contained *within* a node; the only containment sense of "subgraph" (avoid the bare word). Contrast: a *graphlet* is a forme scope over real kernel nodes (peer-scoping, never containment); a *swatch* is a canvas representation that may render a nested graph but never is one. Ruled 2026-07-17; realized by the chartulary containment capability per the [participant gate and packs plan](mere_docs/implementation_strategy/2026-07-17_participant_gate_packs_plan.md)
- **mere** *(lowercase, count noun)* — a configurable spatial dataspace: the unit an application integrates. Isometry's overmap, Woodshed's stage, Strophe's arrangement, and Turnstone's canvas are each a mere; a user has many. Capital **Mere** is the platform, lowercase **a mere** is one dataspace, and the platform is deliberately named for its unit. The word is the lake sense already carried in Mere's own positioning above (a small lake, still-water surface). **Amended 2026-08-13 (with Mark):** two borrowed terms of art apply on different axes, and a mere is genuinely both. *Dataspace* is the integration axis ([Franklin, Halevy & Maier, SIGMOD Record 34:4, 2005](https://dl.acm.org/doi/10.1145/1107499.1107502)): interrelated heterogeneous sources queried and navigated without full upfront integration, with relationships added *pay-as-you-go* — which is precisely cross-application linking between applications that never agree on a schema. *Datalake* is the storage axis (Dixon, 2010): raw native retention, schema-on-read, one accretive pool — of which this stack's *schema at the engram boundary* is the sharper statement. The lake term's usual "derivative copies beside someone else's system of record" reading is deployment practice accreted onto Dixon's actual contrast (natural versus cleansed-and-bottled), not part of the definition, so it does not contradict a mere holding source truth. Where that connotation would mislead, say **reservoir**: the engineered impoundment with a catalog and a drain valve, which is [IBM's own coinage](https://www.redbooks.ibm.com/Redbooks.nsf/RedpieceAbstracts/sg248274.html) for the governed lake, and which retention epochs, provenance, and native drop are what actually supply. Reservoir is a gloss for explaining the distinction, not a minted term. Ruled with Mark 2026-07-26: the concept had no name and was informally covered by *orrery* while the reference host was the only application, which is why the word stopped stretching once four products each integrated one. Tier 1 is *your root mere*, replacing *orrery* at that tier. Amended 2026-07-30: the tiers are escalating socialization of the mere itself, so solo is just a mere (not a moot-of-one), a moot is when your mere is shared, and sharing never stops a mere being a mere
- **orrery** *(form factor)* — the cosmos-style spatial form factor: a whole dataspace seen at once, force-directed and in-scene. A way a **mere** is *rendered*, exactly as **volvelle** names the radial-moot form factor. Narrowed 2026-07-26 from its former lexicon sense ("a user's root graph view", tier 1), which **mere** now carries. Not a tier and not a container; `Scope::Orrery` and "orrery root" in code already mean this form factor
- **tessera** — trust / contribution / reputation token; validated across gemots (Roman *tessera hospitalis* — guest-friendship token between communities)
- **engram** — canonical portable contribution payload; `TransferProfile` envelope plus typed `EngramMemory` items (see inherited `graphshell/design_docs/verse_docs/implementation_strategy/engram_spec.md`)
- **flora** — accumulated body of engrams that constitutes a moot's culture / geist
- **kith / kin** — contact tier distinction: *kith* = those known to you; *kin* = close. Orthogonal to moot membership.
- **gnode** — a node's rendered body on a graph canvas: the visible, spatially-placed object standing at the node's position in an orrery or swatch. A projection, never truth: rebuilt per frame from kernel truth plus seiche's live state, stores nothing. At most one per (node, pane instance); zero when off-scope/off-pane (the node demotes to an underlay dot, which is not a gnode). Anatomy: **body** (silhouette or custom hull, spatially coincident with the seiche collider: the face IS the collider), **face** (the body's texture: state color, favicon, or sprite), **caption** (label beside, LOD-driven); emphasis channels: selection = ring + lift, hover = wash, focus = focus ring, with color reserved for activation state. One primitive, two render tiers: a chrome-DOM `.gnode` element (focused pane) or an in-scene Scene layer (secondary panes; `render_gnodes_as_dom` picks per pane). Pointer-inert: seiche owns press/select/drag through the collider; a11y bounds are read off the gnode's painted rect. Distinct from the **node** (the graph object that references addressed things), from a **card** (summoned *about* a node or selection), and from non-spatial representations (roster row, tile tab, session chip). Etymology: g(raph)-node, coined 2026-06-02 as the orrery pool's CSS class; kept 2026-07-02 for the gnostic reading (the knowable body of the node). Full model: [node_card_summoning_design](mere_docs/design/2026-07-01_node_card_summoning_design.md).
- **strophalos** *(optional, lowercase)* — evocative term for an individual user's running Mere instance ("your strophalos has 47 moots")
- **link** — the user-facing word for a connection between nodes (adopted 2026-07-04; Mark: "if I started over today, I'd just call 'em links"). Plainer than *edge*, carries the right hyperlink lineage, and matches the statement-bucket data model (a link IS a statement: subject node, predicate, object node, with provenance and its own `StatementId`; the drawn connection between two nodes is the pair-local bucket that enumerates them — see the [petgraph-RDF plan](mere_docs/implementation_strategy/2026-06-18_petgraph_rdf_plan.md)). Scope: **product surfaces say link** — menu labels, omnibar verbs (`hide_link`, `show_all_links`, `relate`/`unrelate`), counters, roster tab (already "Links"), docs written from here on. **Code identifiers stay `edge`** (kernel types, petgraph vocabulary, CSS classes like `.roster-edge`) and migrate opportunistically when a file is touched for other reasons, never as a churn pass. *Edge* is not retired as an internal term — it is graph-theory vocabulary and correct at the petgraph layer; it is retired from user-visible copy. The shellbar's screen *edge* (Left/Right/Top/Bottom) is an unrelated sense and keeps its name.

## Retired terms (do not revive)

| Retired | Replacement | Reason |
|---------|-------------|--------|
| Graphshell *(the old browser product brand)* | **Mere** | The old product was absorbed into Mere. The name was reclaimed 2026-07-22 for the separate remote projection host. |
| Verse *(network layer)* | folded into Mere-at-network-scope | The Navigator handles networked-community as a form-factor of the same surface |
| Murmuration *(community layer)* | **Moothold** + count noun *moot* | TESS wall (Murmuration, Inc., civic-tech) |
| Gist *(contribution unit)* | **Engram** | Already canonical and richer |
| Flock *(contact grouping)* | **Kith / Kin** | More nuanced relational tiering |
| Mootcore | **Moothold** | Rename within this conversation |
| Verso *(as engine-controller)* | split: **verso-tile** (rendering surface) + **inker** (engine controller) | Two distinct concerns |
| Middlenet | **Nematic** | Better metaphor (aligned-but-flowing threads) |
| Mnem | **Eidetic** | Prototype name unavailable on crates.io; eidetic evokes "remembered with high fidelity" |
| `nematic.smolweb` *(umbrella engine ID)* | Per-protocol IDs (`nematic.gemtext`, `nematic.gopher`, `nematic.finger`) | Concrete engines now exist for each smolweb protocol |
| HTML reader-mode in nematic | Future Genet head (three-head Hekate negotiator) | HTML in any rendering depth is Genet's job, not nematic's |

## Status

Skeleton. As docs migrate from `graphshell/design_docs/` and as new specs are written here, this file should grow into the long-term canonical terminology surface.
