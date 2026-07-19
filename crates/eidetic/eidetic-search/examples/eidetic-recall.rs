// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! eidetic-recall — the browsing-derivation rehearsal bin (plan slices E2-E4).
//!
//! Ingests real browser data into `BrowsingTrace` engrams in a fjall store,
//! mints the lexical trail index from them, and answers from the terminal:
//! recall ("where did I read about X?"), the recent corridor, time windows,
//! co-occurrence, and the fast-column reports. The proof surface for
//! "derive useful information from your own browsing" until the shell
//! adopts it.
//!
//! ```text
//! eidetic-recall --db <dir> ingest-bookmarks <file> [--owner <tag>]
//! eidetic-recall --db <dir> ingest-history <jsonl> [--owner <tag>]
//! eidetic-recall --db <dir> index                  # (re-)mint the trail index
//! eidetic-recall --db <dir> search <query> [n]     # BM25 recall
//! eidetic-recall --db <dir> embed-index            # mint the vector index (BERT)
//! eidetic-recall --db <dir> recall <query> [n]     # fused BM25 + vector recall
//! eidetic-recall --db <dir> report                 # top domains + visits/day
//! eidetic-recall --db <dir> corridor [n]
//! eidetic-recall --db <dir> window <start_ms> <end_ms>
//! eidetic-recall --db <dir> co <url-a> <url-b>
//! eidetic-recall --db <dir> stats
//! ```
//!
//! `embed-index` and `recall` load the sentence-embedding model from
//! `--model-dir` (default `models/all-MiniLM-L6-v2`, the checkout the repo
//! carries) on burn's CPU backend, or the wgpu GPU backend with
//! `--backend wgpu`; the minted `VectorIndex` engram persists
//! through eidetic's typed layer, and `recall` fuses the lexical and vector
//! rankings by reciprocal rank (the E4 seam, both engines live).
//!
//! The index lives beside the store at `<db>.index` (override with
//! `--index <dir>`); it is derived state — `index` re-mints it from the
//! trace corpus, and `search` re-mints automatically when a tantivy format
//! mismatch refuses the on-disk one.
//!
//! **Bookmarks**: any Chrome/Chromium `Bookmarks` JSON or Netscape bookmark
//! HTML export (every browser's "export bookmarks" menu emits one of these).
//!
//! **History**: one JSON object per line, each an `ImportedHistoryVisitItem`
//! (the import crate's interchange type — no new vocabulary). Browsers keep
//! history in SQLite, so a one-liner exports it; Firefox example:
//!
//! ```text
//! sqlite3 places.sqlite "SELECT json_object(
//!   'page', json_object('canonical_url', p.url, 'normalized_title', p.title,
//!     'raw_url', NULL, 'raw_title', NULL, 'favicon_url', NULL),
//!   'visit_id', CAST(v.id AS TEXT),
//!   'visited_at_unix_secs', CAST(v.visit_date/1000000 AS INTEGER),
//!   'visit_count_hint', p.visit_count,
//!   'transition', NULL,
//!   'referring_url', (SELECT p2.url FROM moz_historyvisits v2
//!     JOIN moz_places p2 ON p2.id = v2.place_id WHERE v2.id = v.from_visit),
//!   'session_context', NULL)
//! FROM moz_historyvisits v JOIN moz_places p ON p.id = v.place_id
//! ORDER BY v.visit_date;" > history.jsonl
//! ```
//!
//! (Copy `places.sqlite` out of the profile first if Firefox is running.)

use eidetic::schema::{
    ModerationState, PrivacyClass, ProvenanceOrigin, ProvenanceRecord, Timestamp, TrustEnvelope,
    TrustLevel,
};
use eidetic::{
    BrowsingMemory, BrowsingTrace, NoFetcher, PageRef, TraceEvent, TraceTransition,
    bootstrap_browsing_schema, save_trace,
};
use eidetic_fjall::FjallStore;
use eidetic_search::{SearchError, TrailIndex, bootstrap_search_schema, fuse};
use embed::provider::EmbeddingProvider;
use embed::{BertEmbeddingProvider, VectorIndex};
use import::{
    HistoryTransitionKind, ImportedBookmarkItem, ImportedHistoryVisitItem, ImportedPageSeed,
    parse_bookmark_items,
};

/// The CPU backend the rehearsal bin embeds on by default.
type CpuBackend = burn::backend::NdArray<f32>;

/// The GPU backend behind `--backend wgpu` (burn brief Lane 1's first
/// consumer wiring; ~38x on batch embedding at MiniLM dims).
type GpuBackend = burn::backend::Wgpu<f32, i32>;

/// Texts per embedding batch (CPU-friendly).
const EMBED_BATCH: usize = 16;

/// Events per stored trace segment when ingesting (large imports become a
/// series of traces, not one giant engram).
const SEGMENT: usize = 512;

fn page_of(seed: &ImportedPageSeed) -> PageRef {
    PageRef {
        url: seed.canonical_url.clone(),
        title: seed.normalized_title.clone(),
    }
}

/// Bookmarks carry no traversal order — they land as `Imported` origin
/// events at their creation time, sorted.
fn events_from_bookmarks(items: &[ImportedBookmarkItem]) -> Vec<TraceEvent> {
    let mut events: Vec<TraceEvent> = items
        .iter()
        .map(|b| TraceEvent {
            from: None,
            to: page_of(&b.page),
            transition: TraceTransition::Imported,
            at_ms: b.created_at_unix_secs.unwrap_or(0).max(0) as u64 * 1000,
            dwell_ms: None,
            candidates: Vec::new(),
        })
        .collect();
    events.sort_by_key(|e| e.at_ms);
    events
}

/// History visits keep their real traversal kind where the source recorded
/// one (the trace-level provenance already says "imported"); referrers
/// become `from` pages.
fn transition_of(kind: Option<&HistoryTransitionKind>) -> TraceTransition {
    match kind {
        Some(HistoryTransitionKind::Link) => TraceTransition::LinkClick,
        Some(HistoryTransitionKind::Typed) => TraceTransition::UrlTyped,
        Some(HistoryTransitionKind::Reload) => TraceTransition::Reload,
        Some(HistoryTransitionKind::Redirect) => TraceTransition::Redirect,
        _ => TraceTransition::Imported,
    }
}

fn events_from_history(items: &[ImportedHistoryVisitItem]) -> Vec<TraceEvent> {
    let mut events: Vec<TraceEvent> = items
        .iter()
        .map(|v| TraceEvent {
            from: v.referring_url.as_ref().map(|url| PageRef {
                url: url.clone(),
                title: None,
            }),
            to: page_of(&v.page),
            transition: transition_of(v.transition.as_ref()),
            at_ms: v.visited_at_unix_secs.max(0) as u64 * 1000,
            dwell_ms: None,
            candidates: Vec::new(),
        })
        .collect();
    events.sort_by_key(|e| e.at_ms);
    events
}

/// Chunk chronological events into SEGMENT-sized traces.
fn traces_from_events(owner: &str, events: Vec<TraceEvent>) -> Vec<BrowsingTrace> {
    events
        .chunks(SEGMENT)
        .map(|chunk| BrowsingTrace::from_events(owner, chunk.to_vec()))
        .collect()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn print_event(e: &TraceEvent) {
    let title = e.to.title.as_deref().unwrap_or("");
    let from = e
        .from
        .as_ref()
        .map(|p| format!("  (from {})", p.url))
        .unwrap_or_default();
    println!(
        "  {:>13}  [{:?}] {} {}{}",
        e.at_ms, e.transition, e.to.url, title, from
    );
}

/// (Re-)mint the trail index from the stored trace corpus.
fn mint_index(memory: &BrowsingMemory, index_dir: &str) -> Result<TrailIndex, String> {
    let traces: Vec<&BrowsingTrace> = memory.traces().collect();
    let index = TrailIndex::rebuild(index_dir, traces.iter().copied())
        .map_err(|e| format!("rebuild index: {e}"))?;
    Ok(index)
}

/// Distinct pages across the corpus, url → text to embed (title when known,
/// the url itself otherwise; first-seen title wins, matching the window
/// read's rule).
fn distinct_pages(memory: &BrowsingMemory) -> Vec<(String, String)> {
    let mut by_url = std::collections::BTreeMap::new();
    for trace in memory.traces() {
        for event in &trace.events {
            let text = event
                .to
                .title
                .clone()
                .unwrap_or_else(|| event.to.url.clone());
            by_url.entry(event.to.url.clone()).or_insert(text);
        }
    }
    by_url.into_iter().collect()
}

/// Deduplicate a ranked hit list down to a URL ranking (first occurrence
/// keeps the rank — the trail index has one document per *visit*).
fn url_ranking(hits: &[eidetic_search::Hit]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    hits.iter()
        .filter(|h| seen.insert(h.url.clone()))
        .map(|h| h.url.clone())
        .collect()
}

fn load_provider(model_dir: &str, backend: &str) -> Result<Box<dyn EmbeddingProvider>, String> {
    let t = std::time::Instant::now();
    let provider: Box<dyn EmbeddingProvider> = match backend {
        "cpu" => Box::new(
            BertEmbeddingProvider::<CpuBackend>::load(model_dir, Default::default())
                .map_err(|e| format!("load embedding model from {model_dir}: {e}"))?,
        ),
        "wgpu" => Box::new(
            BertEmbeddingProvider::<GpuBackend>::load(model_dir, Default::default())
                .map_err(|e| format!("load embedding model from {model_dir}: {e}"))?,
        ),
        other => return Err(format!("--backend must be cpu or wgpu, got {other:?}")),
    };
    println!(
        "embedding backend: {backend} (model loaded in {} ms)",
        t.elapsed().as_millis()
    );
    Ok(provider)
}

/// The newest stored vector-index engram, if any.
async fn load_vector_index(store: &mut FjallStore) -> Result<Option<VectorIndex<String>>, String> {
    let manifests = embed::persistence::list_from_eidetic::<String>(store)
        .await
        .map_err(|e| format!("list vector indices: {e}"))?;
    let Some(newest) = manifests.iter().max_by_key(|m| m.created_at) else {
        return Ok(None);
    };
    embed::persistence::load_from_eidetic::<String>(store, &mut NoFetcher, newest.id)
        .await
        .map_err(|e| format!("load vector index: {e}"))
}

/// Open the index; on a tantivy format mismatch, re-mint from the corpus
/// (the index is derived state — refusal then re-mint is the contract).
async fn open_or_mint(store: &mut FjallStore, index_dir: &str) -> Result<TrailIndex, String> {
    match TrailIndex::open(index_dir) {
        Ok(index) => Ok(index),
        Err(SearchError::FormatMismatch { found, current }) => {
            println!(
                "index format mismatch (on disk {found}; this build {current}) — re-minting from traces"
            );
            let memory = load(store).await?;
            mint_index(&memory, index_dir)
        }
        Err(SearchError::Missing(_)) => {
            println!("no index yet — minting from traces");
            let memory = load(store).await?;
            mint_index(&memory, index_dir)
        }
        Err(e) => Err(format!("open index: {e}")),
    }
}

async fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut db = "eidetic-recall.db".to_string();
    let mut index_dir: Option<String> = None;
    let mut model_dir = "models/all-MiniLM-L6-v2".to_string();
    let mut backend = "cpu".to_string();
    let mut owner = String::new();
    let mut rest: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                i += 1;
                db = args.get(i).cloned().ok_or("--db needs a path")?;
            }
            "--index" => {
                i += 1;
                index_dir = Some(args.get(i).cloned().ok_or("--index needs a path")?);
            }
            "--model-dir" => {
                i += 1;
                model_dir = args.get(i).cloned().ok_or("--model-dir needs a path")?;
            }
            "--backend" => {
                i += 1;
                backend = args.get(i).cloned().ok_or("--backend needs cpu|wgpu")?;
            }
            "--owner" => {
                i += 1;
                owner = args.get(i).cloned().ok_or("--owner needs a tag")?;
            }
            other => rest.push(other.to_string()),
        }
        i += 1;
    }
    let index_dir = index_dir.unwrap_or_else(|| format!("{db}.index"));
    let usage = "usage: eidetic-recall [--db <dir>] [--index <dir>] [--model-dir <dir>] \
                 [--backend cpu|wgpu] [--owner <tag>] \
                 ingest-bookmarks <file> | ingest-history <jsonl> | index | \
                 search <query> [n] | embed-index | recall <query> [n] | report | \
                 corridor [n] | window <start_ms> <end_ms> | co <a> <b> | stats";

    let mut store = FjallStore::open(&db).map_err(|e| format!("open store {db}: {e}"))?;
    eidetic::bootstrap(&mut store)
        .await
        .map_err(|e| format!("bootstrap: {e}"))?;
    bootstrap_browsing_schema(&mut store)
        .await
        .map_err(|e| format!("bootstrap browsing: {e}"))?;
    bootstrap_search_schema(&mut store)
        .await
        .map_err(|e| format!("bootstrap search: {e}"))?;

    match rest.first().map(String::as_str) {
        Some("ingest-bookmarks") => {
            let path = rest.get(1).ok_or(usage)?;
            let contents =
                std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
            let items = parse_bookmark_items(&contents).map_err(|e| format!("parse: {e}"))?;
            let traces = traces_from_events(&owner, events_from_bookmarks(&items));
            ingest(&mut store, &traces).await?;
            println!(
                "ingested {} bookmarks as {} trace(s); run `index` to refresh recall",
                items.len(),
                traces.len()
            );
        }
        Some("ingest-history") => {
            let path = rest.get(1).ok_or(usage)?;
            let contents =
                std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
            let mut items = Vec::new();
            for (lineno, line) in contents.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let item: ImportedHistoryVisitItem = serde_json::from_str(line)
                    .map_err(|e| format!("{path}:{}: {e}", lineno + 1))?;
                items.push(item);
            }
            let traces = traces_from_events(&owner, events_from_history(&items));
            ingest(&mut store, &traces).await?;
            println!(
                "ingested {} history visits as {} trace(s); run `index` to refresh recall",
                items.len(),
                traces.len()
            );
        }
        Some("index") => {
            let memory = load(&mut store).await?;
            let index = mint_index(&memory, &index_dir)?;
            println!(
                "minted trail index at {index_dir}: {} traversal document(s)",
                index.doc_count().map_err(|e| format!("count: {e}"))?
            );
        }
        Some("search") => {
            let query = rest.get(1).ok_or(usage)?;
            let n = rest.get(2).and_then(|s| s.parse().ok()).unwrap_or(10);
            let index = open_or_mint(&mut store, &index_dir).await?;
            let hits = index.search(query, n).map_err(|e| format!("search: {e}"))?;
            println!("{} hit(s) for {query:?}:", hits.len());
            for h in &hits {
                println!(
                    "  {:>6.2}  {}  {}  (at {})",
                    h.score,
                    h.url,
                    h.title.as_deref().unwrap_or(""),
                    h.at_ms
                );
            }
        }
        Some("embed-index") => {
            let provider = load_provider(&model_dir, &backend)?;
            let memory = load(&mut store).await?;
            let pages = distinct_pages(&memory);
            if pages.is_empty() {
                return Err("nothing to embed — ingest some browsing first".to_string());
            }
            let mut vector_index =
                VectorIndex::<String>::new(provider.dimensions(), provider.metric());
            for chunk in pages.chunks(EMBED_BATCH) {
                let texts: Vec<&str> = chunk.iter().map(|(_, text)| text.as_str()).collect();
                let vectors = provider
                    .embed(&texts)
                    .map_err(|e| format!("embed batch: {e}"))?;
                for ((url, _), vector) in chunk.iter().zip(vectors) {
                    vector_index
                        .insert(url.clone(), vector)
                        .map_err(|e| format!("index insert: {e:?}"))?;
                }
            }
            let stamp = now_ms();
            let id = embed::persistence::save_to_eidetic(
                &mut store,
                &vector_index,
                PrivacyClass::LocalOnly,
                ProvenanceRecord {
                    origin: ProvenanceOrigin::Generated,
                    upstream: Vec::new(),
                    tooling: Some("eidetic-recall".to_string()),
                    generated_at: Timestamp(stamp),
                },
                TrustEnvelope {
                    level: TrustLevel::SelfAsserted,
                    signatures: Vec::new(),
                    moderation_state: ModerationState::Unreviewed,
                },
                Timestamp(stamp),
            )
            .await
            .map_err(|e| format!("save vector index: {e}"))?;
            println!(
                "embedded {} page(s) into a {}-dim vector index (engram {id})",
                pages.len(),
                provider.dimensions()
            );
        }
        Some("recall") => {
            let query = rest.get(1).ok_or(usage)?;
            let n = rest.get(2).and_then(|s| s.parse().ok()).unwrap_or(10);
            let pool = n.max(20);

            // Lexical half.
            let trail = open_or_mint(&mut store, &index_dir).await?;
            let hits = trail
                .search(query, pool)
                .map_err(|e| format!("search: {e}"))?;
            let lexical = url_ranking(&hits);
            let titles: std::collections::HashMap<String, String> = hits
                .iter()
                .filter_map(|h| h.title.clone().map(|t| (h.url.clone(), t)))
                .collect();

            // Vector half.
            let Some(vector_index) = load_vector_index(&mut store).await? else {
                return Err("no vector index yet — run `embed-index` first".to_string());
            };
            let provider = load_provider(&model_dir, &backend)?;
            let query_vector = provider
                .embed_one(query)
                .map_err(|e| format!("embed query: {e}"))?;
            let nearest = vector_index
                .nearest(&query_vector, pool)
                .map_err(|e| format!("nearest: {e:?}"))?;
            let vector: Vec<String> = nearest.iter().map(|(url, _)| url.clone()).collect();

            // The E4 seam, both engines live.
            let fused = fuse(&lexical, &vector, 60.0, (1.0, 1.0));
            println!(
                "{} fused hit(s) for {query:?} (lexical + vector):",
                fused.len().min(n)
            );
            for hit in fused.iter().take(n) {
                let ranks = format!(
                    "lex {} / vec {}",
                    hit.lexical_rank
                        .map(|r| (r + 1).to_string())
                        .unwrap_or_else(|| "—".into()),
                    hit.vector_rank
                        .map(|r| (r + 1).to_string())
                        .unwrap_or_else(|| "—".into()),
                );
                println!(
                    "  {:>6.4}  [{ranks}]  {}  {}",
                    hit.score,
                    hit.url,
                    titles.get(&hit.url).map(String::as_str).unwrap_or("")
                );
            }
        }
        Some("report") => {
            let index = open_or_mint(&mut store, &index_dir).await?;
            let domains = index
                .top_domains(10)
                .map_err(|e| format!("top domains: {e}"))?;
            println!("top domains:");
            for (domain, count) in &domains {
                println!("  {count:>6}  {domain}");
            }
            let histogram = index
                .visits_histogram(86_400_000)
                .map_err(|e| format!("histogram: {e}"))?;
            println!("visits per day (bucket start ms → count):");
            for (bucket, count) in &histogram {
                println!("  {bucket:>13}  {count}");
            }
        }
        Some("corridor") => {
            let n = rest.get(1).and_then(|s| s.parse().ok()).unwrap_or(10);
            let memory = load(&mut store).await?;
            let corridor = memory.recent_corridor(n);
            println!("the last {} traversal(s), oldest first:", corridor.len());
            for e in &corridor {
                print_event(e);
            }
        }
        Some("window") => {
            let start: u64 = rest.get(1).and_then(|s| s.parse().ok()).ok_or(usage)?;
            let end: u64 = rest.get(2).and_then(|s| s.parse().ok()).ok_or(usage)?;
            let memory = load(&mut store).await?;
            let pages = memory.nodes_visited_in_window(start, end);
            println!("{} distinct page(s) in [{start}, {end}]:", pages.len());
            for p in &pages {
                println!("  {} {}", p.url, p.title.as_deref().unwrap_or(""));
            }
        }
        Some("co") => {
            let a = rest.get(1).ok_or(usage)?;
            let b = rest.get(2).ok_or(usage)?;
            let memory = load(&mut store).await?;
            println!(
                "direct traversals between the two (either direction): {}",
                memory.co_occurrence(a, b)
            );
        }
        Some("stats") => {
            let memory = load(&mut store).await?;
            let traces: Vec<&BrowsingTrace> = memory.traces().collect();
            let events: usize = traces.iter().map(|t| t.events.len()).sum();
            let mut urls = std::collections::HashSet::new();
            for t in &traces {
                for e in &t.events {
                    urls.insert(e.to.url.as_str());
                }
            }
            let span = match (traces.first(), traces.last()) {
                (Some(first), Some(last)) => {
                    format!("{} → {}", first.started_at_ms, last.ended_at_ms)
                }
                _ => "empty".to_string(),
            };
            println!(
                "{} trace(s), {} traversal(s), {} distinct page(s), span {span}",
                traces.len(),
                events,
                urls.len()
            );
        }
        _ => return Err(usage.to_string()),
    }
    Ok(())
}

async fn ingest(store: &mut FjallStore, traces: &[BrowsingTrace]) -> Result<(), String> {
    let stamp = now_ms();
    for trace in traces {
        save_trace(store, trace, stamp)
            .await
            .map_err(|e| format!("save trace: {e}"))?;
    }
    Ok(())
}

async fn load(store: &mut FjallStore) -> Result<BrowsingMemory, String> {
    BrowsingMemory::load(store, SEGMENT)
        .await
        .map_err(|e| format!("load memory: {e}"))
}

fn main() {
    if let Err(message) = pollster::block_on(run()) {
        eprintln!("{message}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(url: &str, title: Option<&str>) -> ImportedPageSeed {
        ImportedPageSeed {
            canonical_url: url.to_string(),
            normalized_title: title.map(str::to_string),
            raw_url: None,
            raw_title: None,
            favicon_url: None,
        }
    }

    #[test]
    fn bookmarks_become_sorted_imported_origin_events() {
        let items = vec![
            import::ImportedBookmarkItem {
                page: seed("https://b.example", Some("B")),
                bookmark_id: None,
                folder_path: Vec::new(),
                location: import::BookmarkLocation::Toolbar,
                created_at_unix_secs: Some(200),
                modified_at_unix_secs: None,
                tags: Vec::new(),
            },
            import::ImportedBookmarkItem {
                page: seed("https://a.example", Some("A")),
                bookmark_id: None,
                folder_path: Vec::new(),
                location: import::BookmarkLocation::Menu,
                created_at_unix_secs: Some(100),
                modified_at_unix_secs: None,
                tags: Vec::new(),
            },
        ];
        let events = events_from_bookmarks(&items);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].to.url, "https://a.example", "sorted by time");
        assert_eq!(events[0].at_ms, 100_000);
        assert!(
            events
                .iter()
                .all(|e| e.transition == TraceTransition::Imported && e.from.is_none())
        );
    }

    #[test]
    fn history_visits_keep_real_transitions_and_referrers() {
        let jsonl = r#"{"page":{"canonical_url":"https://b.example","normalized_title":"B","raw_url":null,"raw_title":null,"favicon_url":null},"visit_id":"2","visited_at_unix_secs":200,"visit_count_hint":3,"transition":"Link","referring_url":"https://a.example","session_context":null}"#;
        let item: ImportedHistoryVisitItem = serde_json::from_str(jsonl).unwrap();
        let events = events_from_history(&[item]);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].transition, TraceTransition::LinkClick);
        assert_eq!(
            events[0].from.as_ref().map(|p| p.url.as_str()),
            Some("https://a.example")
        );
        assert_eq!(events[0].at_ms, 200_000);
    }

    #[test]
    fn large_imports_chunk_into_segment_sized_traces() {
        let events: Vec<TraceEvent> = (0..(SEGMENT + 3))
            .map(|i| TraceEvent {
                from: None,
                to: PageRef {
                    url: format!("https://p{i}.example"),
                    title: None,
                },
                transition: TraceTransition::Imported,
                at_ms: i as u64,
                dwell_ms: None,
                candidates: Vec::new(),
            })
            .collect();
        let traces = traces_from_events("mark", events);
        assert_eq!(traces.len(), 2);
        assert_eq!(traces[0].events.len(), SEGMENT);
        assert_eq!(traces[1].events.len(), 3);
        assert_eq!(traces[0].owner, "mark");
    }

    #[test]
    fn ingest_index_search_round_trips_through_real_stores() {
        pollster::block_on(async {
            let dir = tempfile::tempdir().unwrap();
            let mut store = FjallStore::open(dir.path().join("recall")).unwrap();
            eidetic::bootstrap(&mut store).await.unwrap();
            bootstrap_browsing_schema(&mut store).await.unwrap();

            let events = vec![
                TraceEvent {
                    from: None,
                    to: PageRef {
                        url: "https://a.example".into(),
                        title: Some("alpha notes".into()),
                    },
                    transition: TraceTransition::UrlTyped,
                    at_ms: 1_000,
                    dwell_ms: None,
                    candidates: Vec::new(),
                },
                TraceEvent {
                    from: Some(PageRef {
                        url: "https://a.example".into(),
                        title: None,
                    }),
                    to: PageRef {
                        url: "https://b.example".into(),
                        title: Some("beta manual".into()),
                    },
                    transition: TraceTransition::LinkClick,
                    at_ms: 2_000,
                    dwell_ms: None,
                    candidates: Vec::new(),
                },
            ];
            let traces = traces_from_events("mark", events);
            ingest(&mut store, &traces).await.unwrap();

            let memory = load(&mut store).await.unwrap();
            assert_eq!(memory.recent_corridor(10).len(), 2);
            assert_eq!(
                memory.co_occurrence("https://a.example", "https://b.example"),
                1
            );

            // Mint the index from the corpus and recall by title word.
            let index_dir = dir.path().join("recall.index");
            let index = mint_index(&memory, index_dir.to_str().unwrap()).unwrap();
            assert_eq!(index.doc_count().unwrap(), 2);
            let hits = index.search("beta", 5).unwrap();
            assert_eq!(hits[0].url, "https://b.example");
        });
    }
}
