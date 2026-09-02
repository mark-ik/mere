# Search Surface Wiring Plan

**Date**: 2026-08-12
**Status**: open. W4's lexical n-gram input probe, W5's deterministic fusion
probe, and the V3 tokenized-URL repair completed 2026-08-31. Turnstone's live
trail-fusion caller and private captured-trail evaluation harness are
implemented on paired feature branches as of 2026-09-02. W4 canvas host wiring
remains open. W5 has an executable promotion gate, but the active profile does
not yet admit a real training/held-out selection. Spun out of the
[leverage census](../../2026-08-10_leverage_census_brief.md) (step 2), and
carries the census's audit answer for `mere-embed` inside it.

**Related**: eidetic-search's own crate docs (Phase 9, producer half), the
esp consolidation plan (the split that left embed's glue behind), the
2026-05-08 local-intelligence integration research (the architectural
anchor embed's lib.rs cites).

## 1. Audit results (verified 2026-08-12)

- **`mere-embed` is not a husk; the census's "retire" branch is closed.**
  It is the re-export shim over `esp::embed` plus three genuinely
  mere-coupled modules, all built and all unwired: `persistence`
  (save/load a `VectorIndex` through eidetic's typed-payload API),
  `field_bridge` and `canvas_search` (project query similarity into
  quint's field algebra over the graph canvas). Zero importers means
  capability awaiting wiring, not deadness. Keep, and wire below.
- **`eidetic-search` is the lexical half, ready.** `TrailIndex` minted
  *from* `BrowsingTrace` engrams (derived state, re-mintable, format
  version carried with the index): BM25 recall over tokenized titles/page
  text/URL components plus exact URL/domain terms,
  fast-field reports (`top_domains`, `visits_histogram`), and `fuse()`,
  the engine-agnostic reciprocal-rank seam that deliberately takes both
  rankings from the caller.
- **The missing precondition is capture.** `BrowsingTrace` exists only
  inside eidetic-core (model, tests, example). Turnstone authors none.
  Recall without a corpus returns nothing, so the first slice is capture,
  not search UI.

## 2. Slices

- **W1 — capture.** Turnstone authors `BrowsingTrace` engrams at
  navigation commit points (the observe/session-lifecycle seam),
  persona-scoped, into the existing eidetic store. **Done when** a real
  browsing session yields traces that re-mint into a `TrailIndex` (run
  eidetic-search's `eidetic-recall` example against the real store).
- **W2 — recall in the omnibar.** A non-privileged omnibar lane queries
  `TrailIndex::search`; hits render as actionable rows (open the page /
  summon the node). Index staleness is surfaced honestly and
  `FormatMismatch` re-mints rather than erroring, per the crate's own
  doctrine. **Done when** "where did I read about X" answers from the
  user's own trail. *(Landed 2026-08-12; see §5.)*
- **W3 — reports.** The trail/steward surface renders `top_domains` and
  `visits_histogram` from the fast-field columns (no re-index needed).
  Small; may ride W2's session.
  **Fleece boundary audit (2026-08-26):** the live host capture path supplies
  extracted page text to the trace corpus. `mere-eidetic-search` consumes those
  traces rather than a DOM, so its direct Fleece dependency is unused and is
  now removed under `genet/design_docs/2026-08-26_fleece_followthrough_plan.md`.
- **W4 — canvas semantic search.** Wire `canvas::canvas_search` +
  `canvas::field_bridge` into the canvas's live surface: a query becomes a
  similarity field over the canvas through quint, with
  `esp::embed::persistence` (feature `persistence`) saving the
  `VectorIndex` via eidetic. Start on the lexical embedding provider
  (deterministic, no Burn), `bert` behind its existing feature per esp's
  target matrix. *(Paths updated 2026-08-12 by the
  [eidetic reorg](2026-08-12_eidetic_reorg_plan.md): the modules now live in
  the crates that will use them, not in the deleted `mere-embed`.)*
- **W5 — fusion.** `fuse()` merges W2's lexical ranking with W4's vector
  ranking in the omnibar. Gated on both.

## 3. Non-goals

- The moot consume-half of `SearchIndexSpec` (deferred by its own doc).
- A crawl-driven corpus (`mere-crawl` stays parked pending the gazette
  feed pipeline, per the census).
- New embedding backends beyond what esp already ships.

## 4. Sequence

W1 first; W2 and W3 follow it; W4 is independent of W2/W3 and may
interleave; W5 last. Each slice lands with its own receipt against a real
store, not fixtures only.

## 5. Progress

- **2026-08-12 — W1 landed** (turnstone `539dacc`). The trail-memory port
  mirrors the recycle bin's actor shape exactly: session-scoped
  `FjallStore` at `sessions/<id>/memory`, `BrowsingTrace` segments through
  `BrowsingMemory`, `from` chained per owner inside the actor, flush on
  segment fill and every lifecycle edge (switch, close, release — the
  release rides the same Windows rename handshake as the bin, since the
  memory store lives in the session dir). Capture rides the observation
  drain as designed: the shell's `drain_app_events` is the first
  production consumer of `App::take_events`, mapping
  AddressOpened/NavigatedBack/NavigatedForward/Reloaded onto
  UrlTyped/Back/Forward/Reload with the root identity's public key hex as
  the owner tag. Three unit tests green (round trip with origin chaining,
  self-flush on a full segment, event mapping); lib check clean. Still
  open in W1's done-condition: the headed receipt — a real browsing
  session's store re-minted through eidetic-search's `eidetic-recall`
  example. W2 is unblocked.
- **2026-08-12 — W1 done condition closed** (turnstone `f22f61f`). Building
  the receipt exposed one real gap: nothing flushed on a normal quit, so a
  short session would have left an empty store. `ApplicationHandler::exiting`
  now releases the trail store under a bounded ack (the scenario driver's
  Done exits through the same hook). The receipt itself
  (`scenarios/trail_capture.scn`, fresh profile): three `mere://`
  navigations through the real shell landed as **1 trace, 3 traversals, 3
  distinct pages**; `eidetic-recall index` minted a 3-document trail index
  from the session's store; `search alpha` answered `mere://alpha` at 0.98
  with the capture-time timestamp. "Where did I read about X" answers from
  a real session's store. W1 is complete.

- **2026-08-31: W4 lexical n-gram input probe complete.** ESP's cheap
  `LexicalEmbeddingProvider` now accepts explicit token n-gram orders while
  `new(dimensions)` remains byte-for-byte compatible unigram hashing. Orders
  are positive, non-empty, sorted, and deduplicated. Higher orders hash token
  windows without allocating joined strings. The decoder stack, Eidetic
  artifacts, `TrailIndex`, and reciprocal-rank fusion seam are unchanged.
  DeepSeek's [Engram paper](https://arxiv.org/abs/2601.07372) puts deterministic
  hashed n-gram lookup, learned memory tables, and contextual gating inside a
  Transformer. This probe borrows only the phrase-addressing idea at the
  application retrieval layer. Mere's existing `Engram` envelope concept is
  untouched.

  The fixed receipt uses the existing `SemanticSearch` → dense `VectorIndex`
  path over 28 derived records and 15 held-out queries: two each for browsing,
  title, URL, entity, and command phrases, plus five unigram/order-insensitive
  controls. Phrase targets compete with shorter records containing the same
  unigrams in another order. This is a forcing corpus for phrase sensitivity,
  not a claim about a production browsing corpus.

  | token orders | Ranking@1 | phrase Ranking@1 | controls Ranking@1 |
  |---|---:|---:|---:|
  | `1` | 7/15 | 2/10 | 5/5 |
  | `1+2` | 15/15 | 10/10 | 5/5 |
  | `1+2+3` | 15/15 | 10/10 | 5/5 |

  Optimized Windows x86-64 cost receipt, 4,096 dimensions, single test thread.
  The ranges below cover three harness runs; each run reports the median of 11
  order-rotated samples:

  | token orders | build ns/document range | query ns range | dense vector bytes | JSON index bytes | occupied slots |
  |---|---:|---:|---:|---:|---:|
  | `1` | 3,914–4,374 | 232,099–244,752 | 458,752 | 459,936 | 125 |
  | `1+2` | 3,987–4,340 | 226,749–248,814 | 458,752 | 460,924 | 222 |
  | `1+2+3` | 3,936–4,495 | 227,815–248,751 | 458,752 | 461,569 | 291 |

  The build and query ranges overlap, so this harness found no latency
  regression distinguishable from run-to-run noise at this corpus size. Dense
  storage is unchanged because the index remains fixed-width; JSON grew by 988
  bytes for bigrams and 1,633 bytes for bigrams plus trigrams. The result
  supports keeping `1+2` as an application setting for phrase-heavy surfaces
  while retaining `1` as compatibility default. Trigrams earned no additional
  ranking win here. Because orders are a vector-space input, changing the
  setting must re-mint the derived index; the index never becomes authority.
  Reproduce with:

  ```text
  cargo test -p esp --test lexical_ngram_recall --offline -j 1 -- --nocapture --test-threads=1
  cargo test -p esp --release --test lexical_ngram_recall --offline -j 1 -- --ignored --nocapture --test-threads=1
  ```

  Final focused gates passed from the shared checkout with isolated
  `CARGO_TARGET_DIR`: 65 ESP library tests plus two active held-out tests in the
  integration target,
  package-local Clippy over all targets with warnings denied, the default ESP
  library check for `wasm32-unknown-unknown`, formatting, and `git diff --check`.
  A broader Clippy invocation that linted dependencies stopped in seven
  pre-existing Personae `redundant_slicing` warnings before ESP; the
  `--no-deps` ESP gate passed. The checkout's unrelated carrier/Luggage work
  and four incoming Distillery-plan commits were left untouched.

- **2026-08-31: W5 deterministic fusion probe complete; live fusion remains
  open.** The same 28 records and 15 held-out queries now pass through an actual
  `BrowsingTrace` -> Tantivy `TrailIndex` BM25 ranking and the existing
  reciprocal-rank `fuse()` seam. URL cases occupy the URL field and have no
  title, so the projection does not hide field behavior. ESP names
  `eidetic-search` only as a dev dependency for this consumer-side receipt;
  neither production crate gains a dependency on the other.

  | ranking | deterministic Ranking@1 | Ranking@3 | phrase Ranking@1 | control Ranking@1 | unique fused Ranking@1 | expected target tied at top |
  |---|---:|---:|---:|---:|---:|---:|
  | BM25 | 7/15 | 15/15 | 2/10 | 5/5 | - | - |
  | unigram feature vector | 7/15 | 15/15 | 2/10 | 5/5 | - | - |
  | `1+2` feature vector | 15/15 | 15/15 | 10/10 | 5/5 | - | - |
  | BM25 + unigram RRF, weights `(1, 1)` | 7/15 | 15/15 | 2/10 | 5/5 | 7/15 | 0 |
  | BM25 + `1+2` RRF, weights `(1, 1)` | 15/15 | 15/15 | 10/10 | 5/5 | 7/15 | 8 |
  | BM25 + `1+2` RRF, weights `(1, 2)` | 15/15 | 15/15 | 10/10 | 5/5 | 15/15 | 0 |

  The main stop sign matters more than the headline numbers. Equal-weight RRF
  gives eight corrected phrase targets exactly the same score as their reversed
  decoys; the documented URL tie-break happens to put the target first. Its
  apparent 15/15 is therefore not a fusion win. Giving the vector input twice
  the weight breaks those ties, but `(1, 2)` is only a probe of the existing
  setting seam, not a recommended default. A real captured-trail partition must
  select weights without reusing its held-out queries.

  The titleless-URL gap found by the first fusion run is closed in field set V3.
  The canonical `url` remains an exact stored `STRING`; a separate, non-stored
  `url_text` field indexes the same canonical bytes with Tantivy's default
  tokenizer. `TrailIndex::search` queries both representations. A single
  absolute URL bypasses the query-string grammar and uses an exact `TermQuery`,
  preventing `https:` from being misread as an unknown field. A V2 sidecar is
  rejected before Tantivy opens its segments, and rebuilding from the trace
  corpus writes V3 and restores component recall. Both titleless URL targets now
  appear at rank 2 under BM25, raising its Ranking@3 from 13/15 to 15/15 while
  preserving the useful contrast: bag-of-words BM25 still prefers the shorter
  reversed-order decoy, and `1+2` supplies the ordering signal.

  Direct gates passed in the isolated target: all 14 `eidetic-search` library
  tests, including V2 rejection/re-mint, titleless URL-component recall, and
  exact full-URL recall; the two active
  ESP integration tests; package-local Clippy with warnings denied; formatting;
  and `git diff --check`. The first direct `eidetic-search` test build took
  34m14s on the loaded shared host because its existing dev dependencies enable
  the BERT/WGPU example graph; the tests themselves ran in 0.35s. Pre-existing
  warnings came from patched Burn, Genet Nematic, and `mere-kernel`, not these
  crates.

  This remains a lexical forcing receipt. It compares BM25, unigram feature
  hashing, `1+2` feature hashing, and their actual RRF combinations. The fixed
  learned-vector comparison below closes the model-baseline gap for this
  fixture. W5 is done only when the live caller supplies both rankings over a
  real held-out trail corpus and selects weights without reusing its evaluation
  partition.

- **2026-09-01: fixed MiniLM baseline complete; live W5 remains open.** The
  ignored `learned_minilm_baseline` receipt runs the same 28 records and 15
  queries through ESP's real Burn/NdArray CPU loader and `SemanticSearch`, then
  brings that ranking to the same Eidetic BM25/RRF seam. It requires
  `SIBYLLA_MINILM_DIR`; repository-ignored model data never becomes a silent CI
  dependency. The test verifies every artifact before loading:

  | artifact | bytes | SHA-256 |
  |---|---:|---|
  | `config.json` | 612 | `953f9c0d463486b10a6871cc2fd59f223b2c70184f49815e7efbcab5d8908b41` |
  | `tokenizer.json` | 466,247 | `be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037` |
  | `model.safetensors` | 90,868,376 | `53aa51172d142c89d9012cce15ae4d6cc0ca6895895114379cacb4fab128d9db` |

  This is `sentence-transformers/all-MiniLM-L6-v2`: 384 dimensions, mean
  pooling, L2 normalization. Total artifact size is 91,335,235 bytes. Its
  28-record vector index is 43,008 bytes, compared with 458,752 bytes for the
  4,096-dimensional feature-hashed index; the dense model also carries its
  weights and runtime.

  | ranking | deterministic Ranking@1 | Ranking@3 | phrase Ranking@1 | control Ranking@1 | unique fused Ranking@1 | expected target tied at top |
  |---|---:|---:|---:|---:|---:|---:|
  | MiniLM | 12/15 | 15/15 | 7/10 | 5/5 | - | - |
  | BM25 + MiniLM RRF, weights `(1, 1)` | 12/15 | 15/15 | 7/10 | 5/5 | 7/15 | 5 |
  | BM25 + MiniLM RRF, weights `(1, 2)` | 12/15 | 15/15 | 7/10 | 5/5 | 12/15 | 0 |
  | BM25 + `1+2` RRF, weights `(1, 2)` | 15/15 | 15/15 | 10/10 | 5/5 | 15/15 | 0 |

  MiniLM leaves `graph query nodes`, `washington post company`, and
  `open downloads folder` at rank 2. Equal weighting turns five of its wins
  into exact BM25 ties. A 2x dense weight restores MiniLM's own 12 unique wins
  but adds none. On this deliberately phrase-sensitive fixture, `1+2` feature
  hashing therefore beats the learned model rather than merely beating a weak
  unigram stand-in.

  Optimized Windows x86-64 CPU measurements across nine runs on the loaded
  shared host were 78-86 ms to load, 243-261 ms to embed all 28 records, and
  323-349 ms for the 15-query sweep (21.5-23.3 ms/query). Three direct process
  runs peaked at 194,674,688-194,699,264 bytes working set. The earlier lexical
  cost harness measured 226,749-248,814 ns/query for `1+2`, but used repeated
  rotated sweeps; the raw figures establish the cost separation without
  pretending the protocols are identical. The first isolated release build
  took 26m06s under concurrent host load; that is compile cost, not inference
  latency.

  Reproduce the learned baseline with:

  ```text
  SIBYLLA_MINILM_DIR=/path/to/all-MiniLM-L6-v2 cargo test -p esp --release --features bert --test lexical_ngram_recall --offline -j 1 learned_minilm_baseline -- --ignored --nocapture --test-threads=1
  ```

  The conclusion stays narrow: expose `1+2` as the phrase-sensitive derived
  lookup setting and take it to a real captured-trail partition. Do not promote
  a live fusion weight from this forcing fixture. A broader corpus with true
  paraphrases is still required to judge the semantic value of MiniLM, while
  live W5 still owns caller wiring and weight selection.

- **2026-09-01: live trail-fusion caller implemented; real W5 selection remains
  open.** Turnstone `7d6e348` on `codex/0901-ngram-recall` consumes Mere
  `7b6ced78` on `codex/0901-ngram-search`. The trail actor now owns a disposable
  `TrailIndex` plus an optional ESP lexical-vector index over one latest record
  per canonical URL. Current graph and recycle-bin titles are projected into
  cloned traces while minting; browsing traces remain unchanged authority.
  Corpus, title, or enabled token-order changes re-mint the derived projection.
  Weight-only changes reuse it.

  Two live application settings expose cumulative token orders `1`, `1+2`, and
  `1+2+3`, plus phrase influence relative to BM25 from `0` through `4`. The
  defaults are order `2` and influence `0`. At zero influence Turnstone issues
  the original BM25 query with the original limit and never mints the vector
  projection. Positive influence widens both candidate heads, applies the
  existing deterministic RRF seam, and reduces to the omnibar limit.

  The remote-pinned Turnstone graph passed a library check and all-test
  type-check. Executed gates passed five trail-actor tests, three settings-owner
  tests, four retained-settings-pane tests, and warning-denied Clippy over the
  focused actor module. The actor receipt proves that enabling `1+2` with weight
  `2` corrects a reversed-word-order BM25 tie and that changing current titles
  re-mints without a new traversal. Broader Turnstone Clippy still reports its
  existing warning set; none point into the new recall logic.

  This closes the caller mechanics only. W4's canvas similarity-field and
  persistence wiring remain separate. W5's done condition still requires a
  real captured-trail training/evaluation split, weight selection on the
  training side, held-out ranking metrics, and a stated RAM and latency budget.

- **2026-09-02: private captured-trail gate implemented; current corpus does
  not admit selection.** Turnstone `57faef4` on `codex/0901-ngram-recall`
  adds the local-only harness and records its protocol in
  `turnstone/design_docs/2026-09-02_trail_recall_evaluation_plan.md`. The
  ignored receipt takes one explicit session, copies it before opening Fjall,
  overlays current graph and recycle-bin titles through the live caller's
  projection, and emits only digests, aggregate ranking metrics, and costs.
  The judgment manifest remains outside both repositories.

  Admission requires twenty distinct documents and disjoint training and
  held-out targets: five distinct phrase targets and five controls on each
  side. Queries must be unique after lexical normalization; phrase queries and
  projected titles must contain multiple tokens. The manifest explicitly sets
  maximum token orders, positive fusion weights, `Ranking@K`, an exact dense
  vector-payload budget, and a p95 query budget. Selection sees training cases
  only. The held-out verdict requires a unique-top-one phrase gain while
  preserving overall unique top one, Recall@K, and control unique top one.
  Resource overruns or metric ties keep BM25.

  The copied active-profile corpus at BLAKE3
  `db4d9b34d4acbc57ee6a1accf84a7e03e3b3760e87b2c1076c1d56af11db6da6`
  contains four traces, eleven traversals, seven distinct pages, and one page
  with a current projected title. The receipt therefore returned
  `insufficient_corpus` before loading judgments or selecting a weight. This
  is a negative admission receipt, not a phrase-feature loss. BM25 remains the
  evidence-backed default.

  The clean remote-pinned Turnstone test graph compiled. The synthetic
  training-only selection test, all five trail-memory actor tests, and the
  ignored active-profile admission receipt passed. Package Clippy exited zero;
  its existing broad warning set contained nothing in the two changed Rust
  files. Targeted formatting and `git diff --check` passed. The earlier
  disposable profile copy was verified and deleted after the receipt.

  W5's next done condition is concrete: capture at least twenty distinct pages
  with enough current multi-token titles, author the private cases from
  remembered intent before consulting stored titles, and run the admitted
  receipt. A setting becomes a promotion candidate only if that held-out run
  says so. W4 remains an independent canvas-field lane.
