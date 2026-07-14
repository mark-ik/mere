/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Meerkat's crawl wiring over the host-neutral [`crawl`] crate.

use std::sync::Arc;

use fetch::Fetched;
use incipit::GraphId;
use mere::linked_data::GraphContribution;

pub use ::crawl::{CrawlPolicy, CrawlProgress, HostScope};

fn fetch_page_boxed(url: String) -> ::crawl::CrawlFetchFuture {
    Box::pin(async move { fetch::fetch_page_crawler(&url).await })
}

fn process_page(
    frontier: &mut ::crawl::Frontier,
    url: &str,
    depth: u32,
    fetched: &Fetched,
) -> Vec<GraphContribution> {
    let mut contributions = Vec::new();
    if let Some(links) = meerkat::ingest::harvest_links(url, &fetched.body) {
        let targets: Vec<String> = links.edges.iter().map(|e| e.object.clone()).collect();
        frontier.enqueue(&targets, depth);
        contributions.push(links);
    }
    if let Some(meta) = meerkat::ingest::page_extract_contribution(
        url,
        fetched.content_type.as_deref(),
        &fetched.body,
    ) {
        contributions.push(meta);
    }
    contributions
}

/// The host-facing crawl owner with the legacy `new(wake)` constructor shape.
pub struct CrawlSession {
    inner: ::crawl::CrawlSession,
}

impl CrawlSession {
    pub fn new(wake: armillary::Wake) -> Self {
        let fetch: ::crawl::CrawlFetch = Arc::new(fetch_page_boxed);
        let process: ::crawl::CrawlProcess = Arc::new(process_page);
        Self {
            inner: ::crawl::CrawlSession::new(wake, fetch, process),
        }
    }

    pub fn policy(&self) -> CrawlPolicy {
        self.inner.policy()
    }

    pub fn scope(&self) -> HostScope {
        self.inner.scope()
    }

    pub fn max_depth(&self) -> u32 {
        self.inner.max_depth()
    }

    pub fn set_scope(&mut self, scope: HostScope) {
        self.inner.set_scope(scope);
    }

    pub fn set_max_depth(&mut self, depth: u32) {
        self.inner.set_max_depth(depth);
    }

    pub fn seed_sitemap(&self) -> bool {
        self.inner.seed_sitemap()
    }

    pub fn set_seed_sitemap(&mut self, on: bool) {
        self.inner.set_seed_sitemap(on);
    }

    pub fn max_pages(&self) -> usize {
        self.inner.max_pages()
    }

    pub fn set_max_pages(&mut self, pages: usize) {
        self.inner.set_max_pages(pages);
    }

    pub fn start(&mut self, seed: &str, policy: CrawlPolicy, graph_id: GraphId) {
        self.inner.start(seed, policy, graph_id);
    }

    pub fn stop(&self) {
        self.inner.stop();
    }

    pub fn drain(&mut self) -> Vec<(GraphId, GraphContribution)> {
        self.inner.drain()
    }

    pub fn progress(&self) -> &CrawlProgress {
        self.inner.progress()
    }
}
