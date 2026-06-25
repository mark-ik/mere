# Relational Browse Graphlet Plan — the bird's-eye neighborhood as corpus front-end

**Date**: 2026-06-23
**Status**: Planning. No code yet. Spun out of a design conversation (Mark) about
visual graph crawlers (the egui-based [raydroplet/crawler-rs](https://github.com/raydroplet/crawler-rs)
as a UX reference only; its egui_graphs / fdg-sim / petgraph stack does not
transfer, and it ships no license so it is read-for-ideas, not lift-able).
**Lane / conflict posture**: meerkat content actor + `graph-kernel` + `orrery`,
plus a small `serval-static-dom` consumer. This is the **front-end** counterpart
to the already-built eidetic capture/index back-end; it deliberately does *not*
re-derive that work.
**Relationship to existing plans** (read these; this plan composes them, it does
not restate them):
- [Eidetic Browsing Derivation](../../eidetic_docs/implementation_strategy/2026-06-12_eidetic_browsing_derivation_plan.md)
  — **built** (E1-E4 green): `BrowsingTrace` capture from `node-lineage`,
  real-history ingest, the `eidetic-search` tantivy index (BM25 + fast-field
  reports), hybrid lexical+vector recall. It explicitly parks **full page-text
  capture** out of scope with the named trigger *"a serval-side text-extraction
  seam."* The V1 materializer below is that trigger.
- [Eidetic Deferred Phases](../../eidetic_docs/implementation_strategy/2026-06-09_eidetic_deferred_phases_plan.md)
  — Phase 9 **consume half** (federated index merge: `EngramDirectory`,
  per-moot merge, defensive ingestion) is the index-federation lane, gated on a
  moot-side consumer.
- [Communal Compute Tiers](../research/2026-06-10_communal_compute_tiers_brief.md)
  — defines **tessera** (per-moot, reliability-shaped, unbuyable standing) and
  **moothold (t3) / coalition (t4)**. **flora = federated LoRA**: the federation
  of personal LoRA adapters across a moot/coalition (the geist is framed there as
  **LoRA + RAG**, the adapter shipping as a content-addressed engram). The flora
  lane is the model-weight counterpart to the index lane.
- Graph substrate: [node representation / arrangement](2026-06-18_node_representation_arrangement_plan.md)
  (LOD: dot → glyph → chip → card), [graph query layer](2026-06-18_graph_query_layer_plan.md),
  [petgraph/RDF](2026-06-18_petgraph_rdf_plan.md). DocumentScript driver, for the
  later sandboxed form: [substrate](2026-06-21_document_script_substrate_plan.md)
  + [follow-ons](2026-06-23_document_script_followons_plan.md).

---

## Thesis (why this is worth building)

A browser tab funnels you site to site, one page at a time, and discards every
candidate you did not click. The relational alternative places a page's link
*neighborhood* on the graph canvas at once, with its relations visible, so you
choose with peripheral vision instead of through a keyhole. Two consequences:

1. **Better directed browsing.** You see the candidate set and its shape before
   committing, and you can expand several branches at once. This is a Navigator
   capability (a graph-shaped session over a neighborhood), not a scraping tool.

2. **A richer corpus, as exhaust.** The eidetic back-end already turns browsing
   into a durable, searchable, locally-owned trail and a tantivy index. A
   *relational* browse makes that trail richer for free, because the structure
   it captures is exactly what a flat chronological trace throws away:
   - **Real negatives, in context.** Expanding two branches out of twelve labels
     the other ten as *seen and declined, against this candidate set.* Negatives
     in context are the hardest signal to get for link-relevance learning, and a
     neighborhood view emits them as a byproduct of ordinary browsing.
   - **The candidate set is the unit.** A relevance row is weak in isolation and
     strong when its context is the actual set of alternatives the human chose
     among. That is listwise-ranking data.
   - **Curation is the label, and it sidesteps the judge.** Synthetic agent data
     leans on an LLM-as-judge to label trajectories (typical reported judge
     accuracy is well under 90%, which quietly poisons the set). With a human
     curating the bird's-eye view, the human is the judge, at decision time,
     with causally-honest information.

The downstream of that corpus is **two lanes that already exist in the
architecture**, and they have different physics:

- **Index lane (built, leads).** A tantivy index composes by union and stays
  auditable: a peer's contribution is inspectable documents, shards merge,
  queries route with trust-weighting. `eidetic-search` is the local producer
  (shipped); Phase 9 consume is the federation step. Contribution value is
  measurable, which makes tessera accounting tractable.
- **Flora lane (federated LoRA; designed, follows).** A personal LoRA adapter
  distilled from the trail, shared as flora, specializes the community's models
  and agents. It composes by weight-merging (model-soup / TIES / DARE), which is
  lossy and not auditable: you cannot point at which browsing produced which
  weight, contribution value is hard to price in tessera, and a malicious peer
  can ship a backdoored adapter. So flora inherits the index lane's trust
  accounting and volume before it leads. This sequencing is not a proposal; it
  is where the repo already stands (index built, flora gated on the
  adapter-as-engram milestone in the compute-tiers brief).

The **division of labor** answers the LoRA-vs-RAG question cleanly: the index
carries *what is out there now* (retrieval, freshly queryable); the personal
adapter carries *how you navigate* (your relevance function and domains, which
are stable and personal, so genuinely adapter-shaped rather than RAG-replaceable
volatile facts). The same browse feeds both.

---

## Slices (each independently landable; done conditions, not dates)

### V1 — single-hop link-graph materializer (the one new primitive)

Materialize the current page's link neighborhood as real graph nodes, from a
single parse, with no fetch of the targets. Verified against the code this
session: the only genuinely new code is a rect-free anchor enumerator; the rest
is an existing pipe.

**Input**: the page already open in a tile (its URL is the seed; its body is
already fetched).

**Pipeline** (every stage but one already exists and is callable):
1. `serval-static-dom::StaticDocument::parse(body)` — paint-free, layout-free,
   html5ever into an arena tree (`crates/.../serval-static-dom/lib.rs:35`; it
   `impl LayoutDom` at `:178`).
2. **NEW**: a rect-free anchor walk over the `LayoutDom` trait
   (`shared/layout-dom/lib.rs:28`, using `walk` `:142` + `attribute(href)`
   `:121` + `text` `:135`). Today's anchor harvest is *layout-coupled*: every
   `<a href>` is lowered to a `LinkHit` **with a hit rect** by `serval_layout`
   (`crates/meerkat/src/card.rs:478`). A crawl wants only URLs and anchor text,
   so it needs the same enumeration *without* paying for layout. This helper is
   the single missing primitive (~30 LOC over the trait).
3. Resolve each href against the seed via `meerkat::nav::resolve_href`
   (`crates/meerkat/src/nav.rs:65`).
4. Build a `GraphContribution` (`linked-data/src/ingest.rs:85`):
   `nodes: Vec<NodeContribution>` (one per resolved target URL, `id` = URL,
   optional `title` = anchor text) and `edges: Vec<EdgeContribution>`
   (`{ subject: seed_url, predicate: hyperlink_iri, object: target_url }`). The
   predicate is `https://mere.computer/ns/rel#hyperlink`
   (`graph-kernel/src/graph/edge_data.rs:96`), i.e.
   `predicate_iri(SemanticSubKind::Hyperlink)`. **No new contribution type:**
   `apply_contribution` already maps a recognized predicate IRI to a typed
   `Semantic:Hyperlink` edge (`linked-data/src/lib.rs:24-25`), and
   `SemanticSubKind::Hyperlink` is the first semantic variant
   (`edge_taxonomy.rs:57-58`), already constructed elsewhere
   (`store.rs:91`, `facet_projection.rs:248`).
5. Emit `ContentUpdate::Contribution { contributions }` exactly as the existing
   JSON-LD harvest does (`crates/meerkat/src/content.rs:267-272`). The
   constellation already pairs `(GraphId, GraphContribution)`
   (`constellation.rs:188`) and applies it (`constellation.rs:772`).

The new producer is a sibling to `meerkat::ingest::harvest_contributions`
(`ingest.rs:43`). Illustrative signature only (not compile-ready):

```rust
// illustrative-signature-only
pub fn harvest_links(seed_url: &str, body: &str) -> linked_data::GraphContribution;
```

**Output**: the seed's outbound links as real graph nodes (deduped by URL via
the kernel's `url_to_nodes`), joined to the seed by `Semantic:Hyperlink` edges,
arranged by the `orrery` force sim into a branching cluster around the seed
(`orrery/orrery/src/physics.rs:38` reconciles a body per node;
`lib.rs:423` re-syncs bodies + edge topology; semantic edges drive the springs),
drawn at LOD (dots/glyphs zoomed out, cards on focus).

**Done when**: opening a static/SSR page and invoking the materializer places
its outbound-link neighborhood on the canvas as deduped nodes + Hyperlink edges,
auto-arranged around the seed, with no target fetch and no layout pass; the
anchor walker has unit tests over a `StaticDocument` fixture; every file under
the 600-LOC ceiling.

**Status: built 2026-06-24** (mere `6966e0b`, riding the render-ladder extraction
lane). The rect-free anchor enumerator landed as `serval-extract::extract_links`
(serval, render-free; its dep graph is the witness). `meerkat::ingest::harvest_links`
maps it to a `GraphContribution` (seed `—Semantic:Hyperlink→` each resolved target,
deduped by URL, non-navigable hrefs skipped), invoked via
`ContentCommand::MaterializeLinks` + `Constellation::materialize_links` and emitted on
the existing Contribution pipe. No target fetch, no new actor. The producer + actor
path are tested; the **host trigger** (a toolbar/menu action that calls
`materialize_links`) and the canvas placement are the thin remaining wire-up.

**Commits to none of the contested parts**: SERP scraping, the crawl frontier,
the LoRA, and DocumentScript wiring all stay out of V1.

### V2 — second-hop relational enrichment (the deliberate increment)

V1 gives a *star*: the candidates, but not the relations *among* them. The
relations get rich at the second hop, when the siblings are fetched and you can
see which cross-link, share targets, or act as hubs. This is where the fetch
cost and the politeness / anti-bot reality land, so it is a separate, gated
slice.

- A bounded fetch of selected siblings through `meerkat::fetch::fetch_page`
  (`fetch.rs:180`, already routes netfetcher for http(s) and errand for smolweb),
  each parsed by `StaticDocument::parse` and run through the V1 producer, with a
  **frontier**: depth/fan-out cap, visited-set (the kernel dedups by URL
  already), per-host serialization, robots.txt honored, descriptive UA. None of
  this scheduling exists yet.
- A **dedicated crawl actor**, not the per-tile content actor. The content actor
  is sync and a fan-out would serialize on its thread; fetch belongs off the
  render path.
- Sourcing seed URLs: get them from an independent-index API (Brave / Exa /
  Tavily) or sitemaps / feeds / Common Crawl, **not** by scraping a live SERP
  (see Findings).

**Done when**: from a seed, a depth-capped, politeness-bounded sibling fetch
materializes the second-hop neighborhood with cross-links visible, on a crawl
actor that never blocks compositing or input.

**Status: actor + frontier built 2026-06-24** (mere `c5e36c6`, `crates/meerkat/src/crawl.rs`).
The scheduling now exists: a pure `Frontier` (BFS queue, visited-dedup, depth /
fan-out caps, host scope SameHost/SameDomain/AnyHost) and a `spawn_crawl` actor that
owns a current-thread tokio runtime and drives `run_crawl` (pop → per-host
`POLITE_DELAY` → `fetch_page` → `crawl_page`: V1 `harvest_links` + page metadata,
enqueue in-scope links → emit `CrawlUpdate::Contribution`/`Progress`/`Done`). The loop
is generic over the fetcher, so it is tested against a canned site (depth + page caps),
14 tests, no network.

**Host wiring done 2026-06-24** (mere `11a9855`): the crawl is **drivable end to end**.
A `CrawlSession` (host-owned, in `SharedState::content`, sharing the content wake) wraps
the actor; the `>crawl` command (`Command::CrawlFocused`) seeds it from the focused
page's URL under `CrawlPolicy::default()` (same-host, shallow); `app_handler` drains the
session each frame and applies its `(GraphId, GraphContribution)` pairs through the same
`apply_contribution` path the content harvest uses. So a `>crawl` on an open page makes
its same-host neighborhood fill the graph, one polite fetch at a time, off the render
path. (Default-build clean; 15 crawl tests; standalone-compile verified.)

**robots.txt honored 2026-06-24** (mere `520d2de`): `crawl/robots.rs` is a
dependency-free robots.txt subset (`User-agent` groups, `Allow`/`Disallow` path-prefix
rules, longest-match with `Allow` breaking ties, our-UA over `*`); `run_crawl` fetches +
caches each host's robots.txt once and skips a disallowed path (a missing / failed
robots.txt allows all, per spec). 23 crawl tests. (crawl.rs was also split into a
`crawl/` module dir to hold the new code under the 600-LOC ceiling.)

**Crawl controls shipped 2026-06-25** (mere `a71878b`..`55c9ac2`): the deferred controls
landed and were headed-verified (driven in `target/debug/meerkat.exe`; shots in
`Code/scry-shots/crawl-*.png`). A **descriptive UA** rides crawl fetches
(`fetch::CRAWLER_USER_AGENT`, the `merebot` token shared with robots matching);
**mid-crawl cancellation** is wired to `>crawl_stop` (`Command::StopCrawl` flips the
actor's cancel flag); a **progress chip** in the toolbar reads `crawling/crawled: N
pages` (hidden when idle, via a class toggle since serval `:empty` does not match an
empty-string text view) and mirrors to leaf windows (MW3 fan-out); a **`pelt/crawl`
settings page** picks scope (same host / domain / any), depth, the **page cap**, and the
**whole-site (sitemap)** mode, each draining `crawl:<key>`, persisted to
`PersistedSettings` and restored at boot; and **sitemap seed sourcing**
(`crawl/sitemap.rs`, a dependency-free `<loc>` scan) seeds the frontier from
`sitemap.xml`. A mouse-clicked context-menu command now fires same-cycle (a
`drain_chrome_intents` ordering fix). 30 crawl tests. **Deferred** (named, not silently
dropped): wildcard (`*`/`$`) robots rules, a Public Suffix List for `SameDomain`
(suffix-approximated today), `sitemapindex` recursion into child sitemaps, bounded
cross-host concurrency / per-host rate limiting (sequential today), and seed sourcing
from an independent-index API (sitemaps only so far, never a live SERP). The second-hop
cross-link *relations* fall out of the graph as the frontier revisits shared targets.

### V3 — relational capture into eidetic (the LoRA-readiness lever)

Whether a future personal adapter is rich or RAG-replaceable is decided here, by
what the trace records. Enrich the eidetic capture so the relational decision is
preserved, not flattened to a chronological trail.

- Extend `BrowsingTrace` (or add a sibling engram) so a curation event carries
  the **candidate set** it was made against and the **decision** (expand /
  dismiss / pin / dwell). This answers the eidetic plan's open question on trace
  granularity and per-owner partitioning directly.
- Feed the V1/V2 extracted text into the eidetic content-capture seam that the
  derivation plan parks as *"full page-text capture, trigger: a serval-side
  text-extraction seam."* The rect-free walk (V1) plus a text extractor is that
  seam, so `eidetic-search` can index page text, not only titles/URLs.
- Keep it **LocalOnly by default**, per the eidetic privacy axis; sharing into a
  moot stays a separate, explicit, later act.

**Done when**: a relational browse session produces eidetic traces that carry
candidate sets and curation decisions; `eidetic-search` indexes extracted page
text from the same browse; classification defaults LocalOnly.

### Downstream (cross-linked, **not** built here)

- **Index federation**: eidetic Phase 9 consume (per-moot `EngramDirectory`
  merge). Trigger unchanged: a moot-side consumer.
- **Flora (federated LoRA)**: distill a personal LoRA adapter from the enriched
  trace (the defensible target is a link-relevance ranker plus navigation
  procedure, not page knowledge); contribute it up for tessera; draw on the
  moot/coalition flora to specialize local models and agents. Gated on the
  adapter-as-engram milestone in the compute-tiers brief and on the index lane's
  trust accounting existing first.

---

## Findings

**Seams verified against the code this session** (not doc-to-doc): the V1 pipe
is real end to end. `fetch_page` routes netfetcher/errand (`fetch.rs:180`);
`StaticDocument::parse` is paint-free and `impl LayoutDom`
(`serval-static-dom/lib.rs:35,178`); the `LayoutDom` trait exposes
`walk`/`attribute`/`text`/`element_name` (`shared/layout-dom/lib.rs:28+`);
`GraphContribution`/`NodeContribution`/`EdgeContribution` exist
(`linked-data/src/ingest.rs:85,46,75`) and `apply_contribution` maps a
recognized predicate IRI to a typed sub-kind (`linked-data/src/lib.rs:24-25`);
`SemanticSubKind::Hyperlink` is the first semantic variant with predicate IRI
`https://mere.computer/ns/rel#hyperlink` (`edge_taxonomy.rs:57-58`,
`edge_data.rs:96`); the content actor emits `ContentUpdate::Contribution`
(`content.rs:267-272`) and the constellation pairs+applies it
(`constellation.rs:188,772`); the orrery reconciles a body per node and lays out
semantic edges (`orrery/orrery/src/physics.rs:38`, `lib.rs:423`). The **one**
missing primitive is a rect-free anchor enumerator: existing anchor harvest is
layout-coupled because a `LinkHit` needs a hit rect (`card.rs:478`).

**SERP reality (drop "scrape a SERP, open all links").** Live SERP scraping is
both broken and hostile as of mid-2026: Google's SearchGuard (Jan 2025) broke
nearly every SERP scraper, Microsoft retired the Bing Search API (Aug 2025), and
Google sued SerpApi over SERP scraping (Dec 2025); it also violates the search
engines' ToS. Get URLs from an independent-index API (Brave Search API, Exa,
Tavily) or from sitemaps / RSS / Common Crawl, then fan out politely from there.

**Anti-bot / JS-required.** A large share of the modern web is JS-rendered, so a
raw fetch returns an empty shell, and Cloudflare-class challenges cannot be
passed by plain HTTP clients. The unrendered static path degrades gracefully on
static/SSR/standards pages. The natural heavy fallback is to route those URLs
through `scry` (the existing system-WebView lane in the multi-engine
multiplexer) rather than bundling a separate headless browser.

**Lightweight per-page analyses worth running at crawl time** (cheap, paint-free,
mostly Rust-native): HTTP/type triage and SPA-shell detection; near-dup
hashing (xxhash exact, SimHash/MinHash near) run *before* any NLP; structured
data + meta harvest (JSON-LD, OpenGraph, canonical/hreflang) for typed fields
for free; main-content extraction (`dom_smoothie` ~27ms or `rs-trafilatura`
~44ms, both on `dom_query`, no JS engine); language detect (CLD/fastText);
YAKE/RAKE keywords (corpus-free); outbound-link graph; relevance-to-seed score.

**LoRA vs RAG, settled by the two-lane split.** "Surface the right *current*
page" is retrieval (the index lane). "How *I* navigate" is stable and personal
(the flora lane). Do not bake volatile page knowledge into weights. The crawl
graph is a strong substrate for a link-relevance ranker (each edge is a
`{goal, anchor, context, url} -> follow?` row, the neural-prioritisation
framing) and a weak substrate for a classic action-policy web agent (which needs
real click/type traces a hyperlink graph lacks). Existing references for the
dataset side if/when the flora lane is built: Mind2Web, WebArena/WebVoyager,
NNetnav (explore-then-retroactively-label, the closest fit to a crawl graph),
ScribeAgent (a production LoRA proof point), and "Neural Prioritisation for Web
Crawling" for the ranker.

**Provenance is one mechanism, two payoffs.** Tessera needs to know whose
contribution was whose; training-data legitimacy needs to know where each row
came from; shared indices need source legibility. All three are the same
`Provenance` edge-family the graph already carries (`EdgeFamily`,
`edge_taxonomy.rs:31`). Honor robots/AI-crawler opt-outs at crawl time and record
per-source provenance; storing the navigation *procedure* is far safer than
storing page *text*.

**Borrowable prior art** for the federation composition (the hard part, harder
for flora than indices): YaCy for decentralized search, the model-merging
literature (TIES/DARE) for flora composition, federated-learning
poisoning-defense work for trust-gated merges. Harvest technique, do not derive
from scratch.

---

## Open questions

- **Trace granularity for LoRA-readiness.** Exactly which fields a curation event
  must carry (candidate set, decision, dwell, the relational edges) to be
  distillable later without an after-the-fact labeling pass. Re-deriving the
  signal later is the expensive part; capturing it live is free. Coordinate with
  the eidetic plan's open question on per-owner trace partitioning.
- **Where the materializer lives long-term.** V1 is host-side in the content
  actor's path; the V2 crawl actor is a new actor. The eventual sandboxed form
  is an origin-bound DocumentScript guest over `net.fetch` (auto-attach already
  works), but `net.fetch` is a stub today (echoes the URL; `net` defaults Deny;
  meerkat hardcodes `net: Deny`) and the per-actor sync fetch serializes I/O. So
  DocumentScript is the right long-term home and the wrong starting point.
- **Relational trace vs chronological trace.** The enriched relational capture
  must not drift into disagreeing with `node-lineage`'s edge views or the
  eidetic `co_occurrence` definition.
- **Legality/provenance for any shared content**, and whether shared artifacts
  carry text or only structure + procedure.

---

## Progress

- **2026-06-23** — Plan created from a design conversation. Scope deliberately
  narrowed after reading the existing eidetic plans: the corpus capture + tantivy
  index + hybrid recall back-end is already **built** (eidetic E1-E4), and the
  federation tiers (tessera, moothold/coalition, flora as LoRA+RAG geist) are
  already **designed** (compute-tiers brief). The genuinely new, unbuilt material
  is the relational-browse **front-end** (V1 single-hop materializer, V2
  second-hop enrichment) and the **capture-schema enrichment** (V3) that makes
  the trace LoRA-ready and supplies the serval-side text-extraction seam the
  eidetic plan parks as a named trigger. All V1 code seams verified against the
  actual sources this session (file:line refs in Findings). The one missing
  primitive is a rect-free anchor enumerator over `LayoutDom`. No code yet.
- **2026-06-23** — Terminology fixed (Mark): **flora = federated LoRA**, the
  federation of personal LoRA adapters that specialize the community's models and
  agents. Earlier "pooled geist" reading replaced throughout; the open question on
  the term is closed.
- **2026-06-24** — **V1 built** (mere `6966e0b`), on the back of the render-ladder
  extraction lane. The "one new primitive" (rect-free anchor enumerator) is
  `serval-extract::extract_links`; `meerkat::ingest::harvest_links` materializes the
  seed's outbound neighborhood as `Hyperlink`-edged nodes through the Contribution
  pipe, invoked via `MaterializeLinks` / `Constellation::materialize_links` — no fetch,
  no new actor, 3 tests. Two adjacent extraction-lane pieces also landed and serve this
  plan's later slices: the **serval-side text-extraction seam** the eidetic derivation
  parks (V3's trigger) is now `serval-extract`'s `extract_text` + reader-mode
  `main_text` (`extract()` over any `LayoutDom`, static or post-JS `ScriptedDom`), and a
  page's declared metadata already enriches its node via the same pipe. Remaining for
  V1: the host trigger (toolbar/menu action) + canvas placement. **V2 (the dedicated
  crawl actor)** and **V3 (relational capture into eidetic)** unchanged. Cross-ref:
  `2026-06-23_render_ladder_and_extraction_plan.md`.
- **2026-06-24** — **V2 actor + frontier built** (mere `c5e36c6`,
  `crates/meerkat/src/crawl.rs`). Mark confirmed the actor split (V1 actor-free on the
  content-actor path; V2 a dedicated crawl actor off the render path). The `Frontier`
  (pure BFS policy: dedup, depth/fan-out caps, host scope) + `spawn_crawl` (own tokio
  runtime, polite per-host pacing) + `run_crawl` (fetch → `crawl_page` → enqueue → emit
  contributions) are in, with the loop generic over the fetcher for network-free tests
  (14). Remaining: the host wiring (route `CrawlUpdate::Contribution` to the graph + a
  seed/scope UI). robots.txt, mid-crawl cancel, concurrency, and seed sourcing are
  named deferrals. The memory model from the design chat holds: crawled pages → graph
  nodes (short-term), V3 consolidates the article text into eidetic (long-term) for
  distillation.
