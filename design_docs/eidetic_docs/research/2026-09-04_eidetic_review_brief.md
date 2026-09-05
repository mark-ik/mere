# Eidetic Review Brief — what the family holds, what search costs, what nothing uses

**Date**: 2026-09-04
**Status**: Review brief. Findings only; every recommendation ends in a
question for Mark rather than a change.
**Occasion**: Mark, 2026-09-04, under the [platform boundary
plan](../../mere_docs/implementation_strategy/2026-09-02_platform_boundary_and_repository_topology_plan.md)'s
P5: *"we should scrutinize the tantivy index and its utility; are there better
approaches to searching, what we're trying to do there? eidetic needs a review
pass and consideration for what we could use it for: probably deduplication,
and possible interesting enhancements/features to surface for the stack."*
**Companion change**: the P5 reshuffle landed the same day — `eidetic-search`
moved from `crates/eidetic/` to `crates/intel/eidetic-search`, beside `esp`.
Every path below is post-move.
**Method**: every count and every claim about who consumes what was taken by
reading the code and the manifests in this repository and in
`repos/turnstone` today. READMEs and prior plans were used only to find
things, never as evidence.

---

## 1. What eidetic holds today

Line counts are `src/**/*.rs` including in-file test modules.

| Crate (package) | Path | Lines | What it is |
|---|---|---:|---|
| `muniment` | `crates/eidetic/muniment` | 2,723 | The persistence floor. |
| `chartulary` | `crates/eidetic/chartulary` | 5,989 | The container graph. |
| `mere-eidetic` | `crates/eidetic/eidetic-core` | 6,881 | The typed memory lane. |
| `mere-eidetic-fjall` | `crates/eidetic/eidetic-fjall` | 294 | Native LSM backend. |
| `mere-eidetic-https-fetcher` | `crates/eidetic/eidetic-https-fetcher` | 226 | `BlobFetcher` for `BlobSource::Https`. |
| `mere-eidetic-iroh-fetcher` | `crates/eidetic/eidetic-iroh-fetcher` | 272 | `BlobFetcher` for `BlobSource::Iroh`. |
| `hagiograph` | `crates/eidetic/hagiograph` | 26 | Name reservation, no implementation. |
| `mere-eidetic-search` | `crates/intel/eidetic-search` | 952 (+709 example) | The tantivy trail index. |

**`muniment`** — four pieces over one `Backend` seam
(`muniment/src/lib.rs:33-50`). `SlotStore` (mutable named typed slots, 144
lines), `BlobStore` (content-addressed immutable blobs keyed by blake3
`Hash`, 156 lines), `Journal` (append-only with stable `Seq` cursors, causal
links and fork provenance; `journal/` is 868 lines, of which
`journal/causal.rs` is 376), and the backends: `MemoryBackend` in
`backend.rs`, plus `redb_backend.rs` (265), `zip_backend.rs` (502) and
`indexeddb_backend.rs` (286) behind features.

**`chartulary`** — `Graph<N, E>` over petgraph with capability traits
(`caps.rs`, 142). `commit.rs` (801) and `spine.rs` (753) are the `GraphLog`
edit spine: batches of `GraphEdit` committed against an expected revision,
the graph being the replay. `facet.rs` (492) is the runtime metadata tier,
`content_class.rs` (282) types a node by the facets it carries,
`taxonomy.rs` (167) is the two-ring edge model. `stemma/` (1,676) is the
lineage/visit layer with an rkyv snapshot. `rdf.rs` (335) projects the
semantic ring to expanded JSON-LD and n-quads.

**`mere-eidetic`** — the manifest/typed-payload lane.
`manifest.rs` + tests (669) define `BlobManifest` with sources, privacy
class, provenance and a trust envelope; `typed.rs` (552) is `save_typed` /
`load_typed`, where `ManifestId::from_hash(Hash::of(bytes))`
(`typed.rs:119-120`) makes every stored payload content-addressed.
`schema.rs` (560) and `schema_def.rs` + tests + validators (1,183) are the
schema layer; `seal.rs` (549) is the encrypt-at-rest seam (`PayloadSealer`,
`seal.rs:93`) whose production implementation lives in the host;
`pack.rs` (326) is signed distribution; `bundle.rs` (385); `codicil.rs`
(235); `deleted.rs` (236) is the removed-node recovery log; `models/` (961)
is the model-artifact and LoRA-adapter library; `browsing/` (906) is the
domain the crate is named for — `PageRef`, `TraceEvent`, `BrowsingTrace`,
`BrowsingMemory`, and the `lineage` bridge.

**The three adapters and `hagiograph`.** `FjallStore`, `HttpsFetcher` and
`IrohFetcher` are thin `Backend` / `BlobFetcher` implementations.
`hagiograph` (`hagiograph/src/lib.rs`) is 26 lines of doc comment ending
"No implementation yet." Its only appearance elsewhere in the workspace is a
prose mention in a doc comment at `ports/distillery/alembic/src/lib.rs:33`.

**`mere-eidetic-search`** — `index.rs` (546) is `TrailIndex`; `spec.rs`
(211) is the `SearchIndexSpec` sidecar and format-compatibility check;
`fusion.rs` (118) is reciprocal-rank fusion; `lib.rs` (77) is the error
type. `examples/eidetic-recall.rs` (709) is the ingest → index → recall →
report rehearsal bin, and it is the only thing in this repository that
exercises the reports.

### The one Mere edge left in the portable directory

`mere-eidetic` reaches mere's `identity` crate for Ed25519 pack signing: the
`use` at `eidetic-core/src/pack.rs:39`, `sign_pack` at 130-138, and the two
`Ed25519PublicKey` / `Ed25519Signature` uses inside `verify_pack` at
160-163 — 18 lines, all behind the default-on `pack-signing` feature
(`eidetic-core/Cargo.toml:41, 45`). Everything else under `crates/eidetic/`
is portable, which is what seven repositories pinning `chartulary` and
`muniment` already rely on.

---

## 2. The search lane

### What it indexes

[`TrailIndex::rebuild_with_text`](../../../crates/intel/eidetic-search/src/index.rs)
writes **one tantivy document per `TraceEvent`**, not per page. The schema
(`index.rs:50-74`) is eight fields: `url` (`STRING | STORED`), `url_text`
(`TEXT`, the same bytes through the default tokenizer), `title` (`TEXT |
STORED`), `text` (`TEXT`, not stored), `domain` (`STRING | STORED | FAST`),
`owner` (`STRING | FAST`), `at_ms` (`u64`, `INDEXED | STORED | FAST`), and
`transition` (`STRING | FAST`, written as `format!("{:?}", …)`).

The `text` field — page body, the only field an inverted index is really
needed for — is supplied by the `text_for` closure, and **`rebuild` passes
`|_| None`** (`index.rs:154`). No consumer anywhere calls
`rebuild_with_text`. So in production the index today holds URLs, titles and
a domain string, and nothing else.

### How it is queried, and by whom

The only production consumer is turnstone's trail-memory actor,
`repos/turnstone/src/trail_memory.rs` (784 lines). It uses exactly three
symbols: `TrailIndex`, `FusedHit` and `fuse` (`trail_memory.rs:44`).

- `RecallIndex::mint` (`trail_memory.rs:315-316`) calls `TrailIndex::rebuild`
  over every stored trace.
- `RecallIndex::fused_hits` (`trail_memory.rs:355-357`) calls `search`, and
  fuses the ranking with an `esp` phrase-vector ranking through `fuse`
  (`trail_memory.rs:385-391`).
- `recall` (`trail_memory.rs:446-468`) flushes the buffer, then **re-mints
  the whole index** whenever `index_stale` is set.

`index_stale` is set on **every recorded traversal** (`trail_memory.rs:512`),
on a source-set change (`trail_memory.rs:531`), and on the lifecycle edges
(564, 575). So the shipping pattern is: navigate once, then the next omnibar
query rebuilds a tantivy index from the entire corpus on disk at
`<session>/memory.index` (`trail_memory.rs:158-162`), searches it once, and
keeps it only until the next navigation. The index is never opened from
disk, never updated incrementally, and dies with the session.

**Consequences, all verified by grep across both repositories:**
`TrailIndex::open`, `top_domains`, `visits_histogram`, `doc_count`,
`rebuild_with_text`, and the whole of `spec.rs` — the `SearchIndexSpec`
sidecar, `matches_current`, `SearchError::FormatMismatch` — have **no
consumer outside this repository's own example and unit tests**. The
persistence-and-compatibility design that justifies a durable inverted index
is precisely the part nothing uses.

### What tantivy costs

`cargo tree -p mere-eidetic-search -e normal` is **207 lines / 117 unique
packages**. The same crate with `eidetic` and nothing else resolves **19**
(`cargo tree -p mere-eidetic --no-default-features -e normal`, unique). The
delta — roughly **98 crates** — is tantivy and its cone. Set-differencing
the two trees, everything outside tantivy's subtree is
`mere-eidetic-search`, `mere-eidetic`, `muniment`, `blake3`, `arrayvec`,
`constant_time_eq`, `cpufeatures`.

The heavy transitive families inside that 98: the tantivy sub-crates
themselves (`tantivy-columnar`, `-sstable`, `-stacker`, `-bitpacker`,
`-common`, `-fst`, `-query-grammar`, `-tokenizer-api`), compression
(`zstd`, `zstd-safe`, `zstd-sys` — a C build, `lz4_flex`), the whole `rayon`
+ `crossbeam` (4 crates) thread-pool stack, the full `regex` +
`aho-corasick` cone, `futures-util` and six sibling `futures-*` crates,
`memmap2`, `fs4`, `time` + `time-macros`, `uuid`, `rust-stemmers`,
`levenshtein_automata`, `sketches-ddsketch`, `htmlescape`, and the
`windows-sys` / `windows-targets` chain.

Two further facts about that cost:

- `zstd-sys` is a C dependency in a family whose selling point is being
  portable and wasm-friendly (`muniment`'s own package description).
- tantivy's native produce path uses the mmap directory
  (`eidetic-search/Cargo.toml:16-19` says so explicitly). The rest of the
  memory family has a proven browser lane — the [redb/OPFS feasibility
  plan](../implementation_strategy/2026-08-22_redb_opfs_feasibility_plan.md)
  established two-engine viability on 2026-08-22 — and `eidetic-search` is
  the one member that cannot follow it. It is absent from
  `ports/graphshell/web`.

### Alternatives, honestly

**(a) The graph itself as the index.** Eidetic is a graph store, and
graph-kernel already holds every visited page as a `Node` carrying url,
title and tags, with a single shared `chartulary::stemma::Stemma` visit tree
([`history.rs`](../../../crates/graph/graph-kernel/src/graph/history.rs)). Sorting the
queries by what they actually need:

- "recent", "this node's history", "removed" — `mere-trail`'s entire
  [`TrailInput`](../../../crates/domain/trail/src/lib.rs). Graph and store
  reads. **No text index needed.**
- `top_domains`, `visits_histogram` — aggregations over a domain string and
  a timestamp. Columnar or a `BTreeMap` fold. **No text index needed.**
- Exact URL recall — `TrailIndex::search` already special-cases this into a
  `TermQuery` and bypasses the query grammar entirely (`index.rs:89-101,
  234-238`). **A hash map.**
- Substring/ranked recall over titles — genuinely wants scoring, but over a
  corpus of short strings.
- BM25 over page **body** text — genuinely wants an inverted index, and is
  the one case that is not implemented (no caller supplies `text_for`).

So four of the five query shapes the crate serves do not need full text at
all, and the fifth is not wired.

**(b) SQLite FTS5 through the storage lane.** There is no SQLite in mere's
own lane. `rusqlite` reaches the lockfile only through the vendored
p2panda-store cone, and the root manifest documents at `Cargo.toml:663-677`
that `libsqlite3-sys` sets `links = "sqlite3"`, so exactly one may exist in
a graph — a conflict the 2026-08-16 note records as *resolved by removing
our half*. Adopting FTS5 would re-open it deliberately, add a second C
dependency, and still not reach the wasm/OPFS lane. Trade-off: a mature,
incremental, durable index, at the price of the portability the family is
built on. **Recommend against**, on the wasm lane alone.

**(c) A smaller pure-Rust engine.** The workspace already ships one and
turnstone already uses it: `esp::embed::LexicalEmbeddingProvider`
(`crates/intel/esp/src/embed/lexical.rs`, 313 lines, feature-hashed token
n-grams, zero dependencies) over `esp::embed::VectorIndex`
(`crates/intel/esp/src/embed/index.rs`, 331 lines, flat, O(N) per query,
"suitable for graphs up to ~10k nodes"). turnstone mints it over the *same*
corpus as the other half of the fusion (`trail_memory.rs:315-340`). A BM25
scorer over an in-memory postings map, ranking the same `Hit` shape, is on
the order of 200-300 lines and would sit beside it. Trade-off: we own the
ranking code, including tokenization and stemming decisions tantivy makes
for us; scale ceiling is memory.

**(d) Keep tantivy.** Defensible the moment the index becomes durable —
opened rather than rebuilt, updated incrementally, format-checked on open.
That is exactly what `spec.rs`, `open` and `FormatMismatch` were built for.
Trade-off today: we pay 98 crates and a C build for a machine we throw away
after one query.

### Recommendation

**Replace the lexical half with an in-tree BM25 over the trace corpus, and
retire tantivy — unless a durable index is actually wanted.** The public
surface turnstone consumes (`TrailIndex::rebuild`, `search` returning
`Hit`, `fuse`) can be preserved byte-for-byte, so the consumer does not
change. `top_domains` and `visits_histogram` become folds over the corpus.

**Decision criteria, stated so the call can be re-taken later.** Keep
tantivy if any one of these turns true:

1. A consumer **opens** a persisted index instead of rebuilding — i.e. the
   sidecar and `FormatMismatch` machinery become load-bearing.
2. A consumer supplies `text_for`, so page **bodies** enter the index (this
   is the capture plan's C5 and the derivation plan's parked text trigger).
3. The corpus outgrows what a session can hold in memory. **This is
   unmeasured** — see open question 1.
4. The browser lane is abandoned for browsing memory.

If none is true, the index is a 98-crate in-memory BM25 ranker, and we can
have that for 300 lines.

---

## 3. Deduplication

The family already dedupes **bytes**: `muniment::BlobStore::put` is
content-addressed on blake3 (`muniment/src/blob.rs:85`), and
`save_typed_sealed` derives `ManifestId::from_hash(Hash::of(&bytes))`
(`eidetic-core/src/typed.rs:118-120`), so two identical payloads are one
manifest and one blob. What is duplicated is one level up: **types, and the
same page recorded at several layers.**

### 3.1 The same visited page, stored five times

One visit to one URL materializes as:

1. A `Node` in graph-kernel carrying `addresses` and `title`
   ([`chart.rs`](../../../crates/graph/graph-kernel/src/graph/chart.rs) implements
   chartulary's `Addressed` / `Labeled` over it), plus a visit in the shared
   `Stemma` (`graph/history.rs`).
2. A `TraceEvent` whose `from` and `to` are each a full inline
   `PageRef { url, title }`, plus `candidates: Vec<PageRef>`
   (`eidetic-core/src/browsing/mod.rs:89-134`). Every event carries the
   destination's URL and title again, so N visits to one page are N copies.
3. A tantivy document per event, with `url` stored twice (`url` and
   `url_text`), `title` stored, and `domain` derived and stored
   (`eidetic-search/src/index.rs:176-188`).
4. turnstone's `recall_documents` `BTreeMap<String, RecallDocument>`, which
   is where the per-URL dedup finally happens
   (`turnstone/src/trail_memory.rs:273-300`).
5. `esp`'s `VectorIndex` `HashMap<K, Vec<f32>>` over the same document set
   (`trail_memory.rs:320-330`).

The structural point: **dedup happens at read time, in the consumer, at
layer 4.** The write path deduplicates nothing above the blob. Whether that
is wrong depends on whether traces are meant to be an event log (in which
case per-event `PageRef` is correct and a page table is the missing
projection) or a page history (in which case a `PageRef` interning table
belongs in `browsing`).

### 3.2 Three transition enums

- `chartulary::stemma::TransitionKind` — 10 variants
  (`chartulary/src/stemma/mod.rs:137-148`).
- `eidetic::browsing::TraceTransition` — the **same 10 variants in the same
  order** (`eidetic-core/src/browsing/mod.rs:96-109`), with a hand-written
  `From` at `browsing/lineage.rs:26-40`. The doc comment states the
  duplication is deliberate: "the schema must not depend on the lineage
  crate."
- `import::HistoryTransitionKind` — 8 variants, a different shape
  (`Link`/`Typed`/`AutoBookmark`/`AutoSubframe`/`Reload`/`Redirect`/
  `Generated`/`Other(String)`), `import/src/lib.rs:138-149`.

The first two are a deliberate, documented copy. The third is a genuine
third vocabulary, and there is **no mapping from it to either of the other
two anywhere in the tree** — so imported history cannot currently be given a
`TraceTransition` other than `Imported`.

### 3.3 Four url+title shapes

`eidetic::browsing::PageRef`, `import::ImportedPageSeed`
(`import/src/lib.rs:93-99`, url + title plus raw/favicon),
`import::ImportedNavigationEntry` (`import/src/lib.rs:185-191`, url + title
+ ordinal + time), and `eidetic_search::Hit`
(`eidetic-search/src/index.rs:104-111`), plus turnstone's `RecallHit`.
`chartulary::Container` (`chartulary/src/container.rs:57-80`) carries
`addresses` + `title` and is the fifth.

### 3.4 Two RDF projections

`chartulary::rdf` (335 lines) projects the semantic ring to expanded JSON-LD
and n-quads, mapping title → `https://schema.org/name` and tags →
`https://schema.org/keywords` (`chartulary/src/rdf.rs:32-36`).
`mere-linked-data` (`crates/graph/linked-data`, **3,739 lines**) does the
same job over the graph kernel with the same two constants
(`linked-data/src/lib.rs:77-79`), and additionally has ingest, bundled
contexts and optional SPARQL. **`mere-linked-data` does not depend on
`chartulary`** — its manifest names `kernel`, `oxrdf`, `oxjsonld`. So the
workspace carries two independent RDF layers over what is meant to be one
semantic ring, and `chartulary::rdf` has **zero consumers** outside its own
crate.

### 3.5 Dead or unreachable within the family

- `eidetic::browsing::lineage` (259 lines) sits behind the `lineage`
  feature. Grepping every manifest in the workspace: **nothing enables
  `lineage`**, so `project_lineage` is unreachable. turnstone builds
  `TraceEvent`s directly from shell events instead
  (`trail_memory.rs:494-514`). The DOC_README's 2026-06-26 note that
  "nothing in the running app writes a `BrowsingTrace`" is now half stale —
  turnstone does — but the *lineage projection* half is still true.
- `muniment::ZipBackend` (502 lines, the largest file in the crate) has
  **zero consumers**: no manifest enables `muniment/zip`.
- `muniment::PostcardCodec` has zero consumers; every consumer takes the
  `json` default.
- `hagiograph` (26 lines) has zero code consumers.
- `TraceEvent::dwell_ms` and `TraceEvent::candidates` are written as `None`
  and `Vec::new()` by the only producer (`trail_memory.rs:509-510`).

---

## 4. Features the stack could surface and does not

Five, each anchored in a type that exists today.

**F1. Trail reports.** `TrailIndex::top_domains` and
`visits_histogram` (`eidetic-search/src/index.rs:276, 299`) already answer
"where does my attention go" over fast-field columns with no re-index. The
consumer they would serve is the Trail pane: `mere-trail`'s `TrailInput`
([`TrailInput`](../../../crates/domain/trail/src/lib.rs)) is a three-section projection whose
`build_trail_items` **has no caller anywhere in the workspace**. A fourth
section — top domains, a visits histogram — is a report the data supports
and nothing renders. (If the tantivy recommendation is taken these become
corpus folds, which is simpler, not harder.)

**F2. Choice context as training data.** `TraceEvent::candidates`
(`eidetic-core/src/browsing/mod.rs:125-133`) is documented as "the
relational candidate set… the in-context 'seen but not followed' negatives
(the listwise relevance signal)". It is written empty everywhere. Filled, it
is free preference-pair data with no labelling step, and the consumer
already exists: `ports/distillery`'s LoRA trainer over
`eidetic::models::ModelAdapter` (`distillery/src/trainer.rs`). This is the
capture plan's C2 with the sink already built.

**F3. Durable browsing memory from the graph's own visit tree.**
`browsing::lineage::project_lineage` turns a `chartulary::stemma` snapshot
into per-owner `BrowsingTrace`s "by reading, never by keeping a second
visited-set" (`browsing/lineage.rs:7-15`). graphshell already holds that
snapshot — `graph/history.rs` keeps every node as an `Owner` in one shared
`Stemma`. Enabling `mere-eidetic/lineage` in graphshell would give it trail
memory with no capture tap at all, and would remove duplication 3.1(2)
rather than adding to it. The consumer is graphshell, which has the graph
and no trail memory today.

**F4. Portable memory export.** `muniment::ZipBackend` "a zip archive whose
entries are the store's keys, so a consumer that names keys after files
produces an archive anyone can unzip" (`muniment/Cargo.toml:22-27, 60-61`) — 502
lines, no consumer. Since `eidetic::Store` **is** `muniment::Backend`, a
whole eidetic memory can be written into an inspectable zip with no new
code. The consumer is graphshell / turnstone's settings surface: "export my
memory", and its inverse, a migration path between machines that does not
need fjall on both ends.

**F5. Causal and forked journals for graph history.**
`muniment::journal::causal` (376 lines) and `Provenance` fork records
already give the journal happens-before links and fork provenance;
stickleback and moot use them. graph-kernel's `GraphJournal`
(`graph/journal.rs`) uses the journal as a plain append-only log and takes
neither. The consumer is graphshell's personal-sync lane
(`ports/graphshell/src/personal_sync.rs`), which does its own causal
metadata validation over a journal that could carry it natively — branch and
merge of graph edits across devices, from a primitive already paid for.

---

## 5. Open questions for Mark

1. **How large is a real trail corpus?** Every judgement about tantivy turns
   on whether the trace corpus fits in memory. Nothing measures it. Should I
   instrument turnstone's `RecallIndex::mint` to report corpus size, event
   count and mint latency, and bring numbers back before any search change
   is made?
2. **Is the trail index meant to be durable?** If yes, the fix is the
   opposite of §2's recommendation: wire `TrailIndex::open` and incremental
   updates in turnstone and keep tantivy. If no, `spec.rs` (211 lines) and
   `open` are dead weight either way. Which is it?
3. **Does browsing memory have to reach the browser?** `eidetic-search` is
   the only family member that cannot follow muniment to wasm/OPFS. If the
   browser lane must eventually recall a trail, that alone settles the
   engine question.
4. **Traces: event log or page history?** Whether a `PageRef` interning
   table belongs in `browsing` (§3.1) depends on which of the two a
   `BrowsingTrace` is meant to be. I have not assumed.
5. **Two RDF layers, one ring (§3.4).** Is `chartulary::rdf` the portable
   projection that `mere-linked-data` should be built on, or is it a
   superseded 335-line stub that should be retired now the kernel-side layer
   exists? Both readings are defensible and they lead to opposite work.
6. **`import::HistoryTransitionKind` has no mapping** to `TraceTransition`
   (§3.2), so imported history flattens to `Imported`. Should it map, and if
   so who owns the mapping — `import`, or a bridge in the search/recall
   lane?
7. **`hagiograph`.** 26 lines, no implementation, and it now sits inside the
   directory this repo has just declared to be the portable core. Keep it
   there as a reservation, move it out until it has substance, or retire the
   name?
8. **Of F1-F5, which (if any) do you want turned into a plan?** They are
   independent; F3 and F4 need no new design work, F2 needs a capture change
   in turnstone.
