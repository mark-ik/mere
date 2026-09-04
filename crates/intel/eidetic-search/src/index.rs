// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The trail index — tantivy over `BrowsingTrace` events, native produce path.
//!
//! One document per traversal event. Tokenized title, page-text, and URL
//! component fields carry BM25 recall; the canonical URL and domain remain
//! exact string fields. The **reserved fast-field columns** (domain, owner,
//! at_ms, transition) carry the reports — columnar from day one so
//! aggregations never force a re-index (the derivation plan's E3 rule).
//!
//! The index is derived state. [`TrailIndex::rebuild`] re-mints it from the
//! trace corpus, and [`TrailIndex::open`] refuses an index whose spec
//! sidecar doesn't match this build ([`SearchError::FormatMismatch`]) so the
//! caller re-mints instead of reading segments tantivy may misparse.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use eidetic::browsing::BrowsingTrace;
use tantivy::collector::TopDocs;
use tantivy::query::{Query, QueryParser, TermQuery};
use tantivy::schema::{
    FAST, Field, INDEXED, IndexRecordOption, STORED, STRING, Schema, TEXT, Value,
};
use tantivy::{Index, TantivyDocument, Term, doc};

use crate::spec::SearchIndexSpec;
use crate::{Result, SearchError};

/// Writer memory budget — small; trail segments are tiny by tantivy
/// standards.
const WRITER_BUDGET_BYTES: usize = 15_000_000;

#[derive(Clone, Copy)]
struct Fields {
    url: Field,
    url_text: Field,
    title: Field,
    text: Field,
    domain: Field,
    owner: Field,
    at_ms: Field,
    transition: Field,
}

fn trail_schema() -> (Schema, Fields) {
    let mut builder = Schema::builder();
    let url = builder.add_text_field("url", STRING | STORED);
    // Preserve `url` as the exact stored identity while indexing the same
    // canonical bytes through the default tokenizer for host/path recall.
    let url_text = builder.add_text_field("url_text", TEXT);
    let title = builder.add_text_field("title", TEXT | STORED);
    // Page main text (reader-mode), tokenized for BM25 body recall; not stored —
    // hits return url/title, not the body. (C5.)
    let text = builder.add_text_field("text", TEXT);
    let domain = builder.add_text_field("domain", STRING | STORED | FAST);
    let owner = builder.add_text_field("owner", STRING | FAST);
    let at_ms = builder.add_u64_field("at_ms", INDEXED | STORED | FAST);
    let transition = builder.add_text_field("transition", STRING | FAST);
    (
        builder.build(),
        Fields {
            url,
            url_text,
            title,
            text,
            domain,
            owner,
            at_ms,
            transition,
        },
    )
}

/// The host of a URL, lowercased — good enough for report facets without a
/// URL-crate dependency (canonical URLs come in already normalized by the
/// import/capture paths).
fn domain_of(url: &str) -> String {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    rest.split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// A single absolute URL bypasses Tantivy's field-query grammar (`https:`
/// would otherwise be read as a field name) and takes the exact URL path.
fn is_absolute_url_query(query: &str) -> bool {
    let Some((scheme, rest)) = query.split_once("://") else {
        return false;
    };
    !scheme.is_empty()
        && !rest.is_empty()
        && !query.chars().any(char::is_whitespace)
        && scheme
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
}

/// One recall hit.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub url: String,
    pub title: Option<String>,
    pub at_ms: u64,
    pub score: f32,
}

/// The lexical index over a trail, at a directory.
pub struct TrailIndex {
    index: Index,
    fields: Fields,
    path: PathBuf,
}

impl TrailIndex {
    /// Open an existing index, refusing on spec mismatch (re-mint instead —
    /// the corpus is the source of truth) and `Missing` when there is none.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let on_disk = SearchIndexSpec::read_sidecar(&path)?;
        if !on_disk.matches_current() {
            return Err(SearchError::FormatMismatch {
                found: format!(
                    "{} (fields v{})",
                    on_disk.tantivy_version, on_disk.fields_version
                ),
                current: format!(
                    "{} (fields v{})",
                    SearchIndexSpec::current().tantivy_version,
                    SearchIndexSpec::current().fields_version
                ),
            });
        }
        let index = Index::open_in_dir(&path)?;
        let (_, fields) = trail_schema();
        Ok(Self {
            index,
            fields,
            path,
        })
    }

    /// Re-mint the index at `path` from the trace corpus: delete whatever is
    /// there, create fresh, index every event, write the spec sidecar.
    pub fn rebuild<'a>(
        path: impl AsRef<Path>,
        traces: impl IntoIterator<Item = &'a BrowsingTrace>,
    ) -> Result<Self> {
        Self::rebuild_with_text(path, traces, |_| None)
    }

    /// Re-mint like [`rebuild`](Self::rebuild) but enrich each event's document
    /// with the page's main text, so BM25 recall reaches the body, not just the
    /// title/URL. `text_for(url)` supplies the extracted text for a visited URL,
    /// or `None` when no body is cached. (Capture plan C5.)
    pub fn rebuild_with_text<'a>(
        path: impl AsRef<Path>,
        traces: impl IntoIterator<Item = &'a BrowsingTrace>,
        text_for: impl Fn(&str) -> Option<String>,
    ) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if path.exists() {
            std::fs::remove_dir_all(&path)?;
        }
        std::fs::create_dir_all(&path)?;
        let (schema, fields) = trail_schema();
        let index = Index::create_in_dir(&path, schema)?;

        let mut writer = index.writer::<TantivyDocument>(WRITER_BUDGET_BYTES)?;
        for trace in traces {
            for event in &trace.events {
                let mut document = doc!(
                    fields.url => event.to.url.clone(),
                    fields.url_text => event.to.url.clone(),
                    fields.domain => domain_of(&event.to.url),
                    fields.owner => trace.owner.clone(),
                    fields.at_ms => event.at_ms,
                    fields.transition => format!("{:?}", event.transition),
                );
                if let Some(title) = &event.to.title {
                    document.add_text(fields.title, title);
                }
                if let Some(text) = text_for(&event.to.url) {
                    document.add_text(fields.text, &text);
                }
                writer.add_document(document)?;
            }
        }
        writer.commit()?;
        SearchIndexSpec::current().write_sidecar(&path)?;
        Ok(Self {
            index,
            fields,
            path,
        })
    }

    /// Where the index lives.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// How many traversal documents the index holds.
    pub fn doc_count(&self) -> Result<u64> {
        let reader = self.index.reader()?;
        Ok(reader.searcher().num_docs())
    }

    /// BM25 recall over tokenized titles, page text, and URL components. A
    /// single absolute-URL query takes an exact term path on the canonical URL
    /// field so query syntax cannot mistake its scheme for a field name. Other
    /// input uses the existing parser over the tokenized fields plus exact URL
    /// and domain terms. Hits come back ranked, newest-irrelevant — relevance
    /// is the ranking; time is a column the caller can re-sort by.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<Hit>> {
        let reader = self.index.reader()?;
        let searcher = reader.searcher();
        let parser = QueryParser::for_index(
            &self.index,
            vec![
                self.fields.title,
                self.fields.url,
                self.fields.url_text,
                self.fields.domain,
                self.fields.text,
            ],
        );
        let query = query.trim();
        let parsed: Box<dyn Query> = if is_absolute_url_query(query) {
            Box::new(TermQuery::new(
                Term::from_field_text(self.fields.url, query),
                IndexRecordOption::Basic,
            ))
        } else {
            parser
                .parse_query(query)
                .map_err(|e| SearchError::Tantivy(format!("query: {e}")))?
        };
        let top = searcher.search(
            parsed.as_ref(),
            &TopDocs::with_limit(limit.max(1)).order_by_score(),
        )?;
        let mut hits = Vec::with_capacity(top.len());
        for (score, address) in top {
            let document: TantivyDocument = searcher.doc(address)?;
            let url = document
                .get_first(self.fields.url)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let title = document
                .get_first(self.fields.title)
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let at_ms = document
                .get_first(self.fields.at_ms)
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            hits.push(Hit {
                url,
                title,
                at_ms,
                score,
            });
        }
        Ok(hits)
    }

    /// Report: the most-visited domains, by traversal count, over the fast
    /// columns (no re-index, no text scan).
    pub fn top_domains(&self, n: usize) -> Result<Vec<(String, u64)>> {
        let request = serde_json::json!({
            "domains": { "terms": { "field": "domain", "size": n } }
        });
        let buckets = self.run_aggregation(request)?;
        let mut out = Vec::new();
        if let Some(entries) = buckets
            .get("domains")
            .and_then(|d| d.get("buckets"))
            .and_then(|b| b.as_array())
        {
            for entry in entries {
                let key = entry.get("key").and_then(|k| k.as_str()).unwrap_or("");
                let count = entry.get("doc_count").and_then(|c| c.as_u64()).unwrap_or(0);
                out.push((key.to_string(), count));
            }
        }
        Ok(out)
    }

    /// Report: traversals over time, bucketed at `interval_ms` (a day is
    /// 86_400_000). Returns `(bucket_start_ms, count)` for non-empty
    /// buckets, ascending.
    pub fn visits_histogram(&self, interval_ms: u64) -> Result<Vec<(u64, u64)>> {
        let request = serde_json::json!({
            "visits": { "histogram": { "field": "at_ms", "interval": interval_ms as f64 } }
        });
        let buckets = self.run_aggregation(request)?;
        let mut out = BTreeMap::new();
        if let Some(entries) = buckets
            .get("visits")
            .and_then(|d| d.get("buckets"))
            .and_then(|b| b.as_array())
        {
            for entry in entries {
                let key = entry.get("key").and_then(|k| k.as_f64()).unwrap_or(0.0) as u64;
                let count = entry.get("doc_count").and_then(|c| c.as_u64()).unwrap_or(0);
                if count > 0 {
                    out.insert(key, count);
                }
            }
        }
        Ok(out.into_iter().collect())
    }

    fn run_aggregation(&self, request: serde_json::Value) -> Result<serde_json::Value> {
        use tantivy::aggregation::AggregationCollector;
        use tantivy::aggregation::agg_req::Aggregations;
        use tantivy::query::AllQuery;

        let aggregations: Aggregations = serde_json::from_value(request)
            .map_err(|e| SearchError::Tantivy(format!("aggregation request: {e}")))?;
        let collector = AggregationCollector::from_aggs(aggregations, Default::default());
        let reader = self.index.reader()?;
        let result = reader.searcher().search(&AllQuery, &collector)?;
        serde_json::to_value(result)
            .map_err(|e| SearchError::Tantivy(format!("aggregation result: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eidetic::browsing::{PageRef, TraceEvent, TraceTransition};

    fn event(to: &str, title: &str, at_ms: u64) -> TraceEvent {
        TraceEvent {
            from: None,
            to: PageRef {
                url: to.to_string(),
                title: Some(title.to_string()),
            },
            transition: TraceTransition::LinkClick,
            at_ms,
            dwell_ms: None,
            candidates: Vec::new(),
        }
    }

    fn corpus() -> Vec<BrowsingTrace> {
        vec![BrowsingTrace::from_events(
            "mark",
            vec![
                event(
                    "https://docs.example/vello",
                    "vello scene encoding API",
                    1_000,
                ),
                event(
                    "https://docs.example/tantivy",
                    "tantivy index format notes",
                    90_000_000,
                ),
                event(
                    "https://news.example/wgpu",
                    "wgpu 29 release announcement",
                    90_500_000,
                ),
            ],
        )]
    }

    #[test]
    fn rebuild_then_search_ranks_the_matching_page() {
        let dir = tempfile::tempdir().unwrap();
        let index = TrailIndex::rebuild(dir.path().join("idx"), &corpus()).unwrap();
        assert_eq!(index.doc_count().unwrap(), 3);

        let hits = index.search("vello scene", 5).unwrap();
        assert!(!hits.is_empty());
        assert_eq!(hits[0].url, "https://docs.example/vello");
        assert_eq!(hits[0].at_ms, 1_000);
        assert!(hits[0].title.as_deref().unwrap_or("").contains("vello"));
    }

    #[test]
    fn body_text_is_recallable_via_rebuild_with_text() {
        let dir = tempfile::tempdir().unwrap();
        let traces = vec![BrowsingTrace::from_events(
            "m",
            vec![event("https://a.test/page", "Plain Title", 1_000)],
        )];
        let texts: std::collections::HashMap<String, String> = [(
            "https://a.test/page".to_string(),
            "quantum entanglement field notes".to_string(),
        )]
        .into_iter()
        .collect();
        let index = TrailIndex::rebuild_with_text(dir.path().join("idx"), &traces, |u| {
            texts.get(u).cloned()
        })
        .unwrap();
        // A term that appears only in the body — not the title or URL — still
        // recalls the page (the C5 payoff).
        let hits = index.search("entanglement", 5).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].url, "https://a.test/page");
    }

    #[test]
    fn titleless_url_components_are_recallable() {
        let dir = tempfile::tempdir().unwrap();
        let trace = BrowsingTrace::from_events(
            "m",
            vec![TraceEvent {
                from: None,
                to: PageRef {
                    url: "https://docs.example/rust/async/book/getting-started".to_string(),
                    title: None,
                },
                transition: TraceTransition::Imported,
                at_ms: 1_000,
                dwell_ms: None,
                candidates: Vec::new(),
            }],
        );
        let index = TrailIndex::rebuild(dir.path().join("idx"), [&trace]).unwrap();

        let hits = index.search("rust async book", 5).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(
            hits[0].url,
            "https://docs.example/rust/async/book/getting-started"
        );
        assert_eq!(hits[0].title, None);

        let exact = index
            .search("https://docs.example/rust/async/book/getting-started", 5)
            .unwrap();
        assert_eq!(exact[0].url, hits[0].url);
    }

    #[test]
    fn open_refuses_a_drifted_spec_and_rebuild_recovers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("idx");
        TrailIndex::rebuild(&path, &corpus()).unwrap();

        // Sabotage the sidecar to an older format.
        let drifted = SearchIndexSpec {
            tantivy_version: "tantivy 0.1.0".to_string(),
            ..SearchIndexSpec::current()
        };
        drifted.write_sidecar(&path).unwrap();

        match TrailIndex::open(&path) {
            Err(SearchError::FormatMismatch { found, current }) => {
                assert!(found.contains("0.1.0"));
                assert!(!current.is_empty());
            }
            other => panic!("expected FormatMismatch, got {:?}", other.err()),
        }

        // The re-mint path: rebuild from the corpus and search again.
        let index = TrailIndex::rebuild(&path, &corpus()).unwrap();
        let hits = index.search("wgpu", 5).unwrap();
        assert_eq!(hits[0].url, "https://news.example/wgpu");

        // And a normal reopen now succeeds.
        let reopened = TrailIndex::open(&path).unwrap();
        assert_eq!(reopened.doc_count().unwrap(), 3);
    }

    #[test]
    fn open_refuses_v2_fields_and_rebuild_recovers_url_recall() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("idx");
        let traces = corpus();
        TrailIndex::rebuild(&path, &traces).unwrap();

        SearchIndexSpec {
            fields_version: crate::spec::FIELDS_V2,
            ..SearchIndexSpec::current()
        }
        .write_sidecar(&path)
        .unwrap();

        match TrailIndex::open(&path) {
            Err(SearchError::FormatMismatch { found, current }) => {
                assert!(found.contains("fields v2"));
                assert!(current.contains("fields v3"));
            }
            other => panic!("expected field-version mismatch, got {:?}", other.err()),
        }

        let rebuilt = TrailIndex::rebuild(&path, &traces).unwrap();
        assert_eq!(
            rebuilt.search("docs example tantivy", 5).unwrap()[0].url,
            "https://docs.example/tantivy"
        );
    }

    #[test]
    fn opening_nothing_is_missing_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            TrailIndex::open(dir.path().join("nowhere")),
            Err(SearchError::Missing(_))
        ));
    }

    #[test]
    fn reports_run_over_the_fast_columns() {
        let dir = tempfile::tempdir().unwrap();
        let index = TrailIndex::rebuild(dir.path().join("idx"), &corpus()).unwrap();

        let domains = index.top_domains(5).unwrap();
        assert_eq!(
            domains.first().map(|(d, c)| (d.as_str(), *c)),
            Some(("docs.example", 2))
        );
        assert!(domains.iter().any(|(d, c)| d == "news.example" && *c == 1));

        // Day buckets: two events fall in day 1 (86.4M..172.8M), one in day 0.
        let histogram = index.visits_histogram(86_400_000).unwrap();
        assert_eq!(histogram.len(), 2);
        assert_eq!(histogram[0], (0, 1));
        assert_eq!(histogram[1], (86_400_000, 2));
    }

    #[test]
    fn domain_extraction_is_boring_and_correct() {
        assert_eq!(domain_of("https://Docs.Example/path?q=1"), "docs.example");
        assert_eq!(domain_of("gemini://smol.host"), "smol.host");
        assert_eq!(domain_of("no-scheme/path"), "no-scheme");
        assert!(is_absolute_url_query("https://docs.example/path?q=1"));
        assert!(is_absolute_url_query("gemini://smol.host"));
        assert!(!is_absolute_url_query("https://docs.example one more term"));
        assert!(!is_absolute_url_query("where did I read this"));
    }
}
