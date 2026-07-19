// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The crawl frontier and bounded crawl runtime.
//!
//! The pure half is the [`Frontier`] plus its host-scope / depth / fan-out
//! policy. The runtime half drives that frontier on a dedicated actor thread:
//! pop a URL, fetch it politely, hand the fetched page to a caller-supplied
//! page processor, enqueue the new in-scope links, and emit typed updates.
//!
//! The page processor is injected because the crawl crate owns no Mere-specific
//! page-extract or link-harvest policy. The host supplies the graph
//! contribution producer appropriate to its graph/model layer.

use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use armillary::{ActorHandle, Emitter, Wake, spawn};
use fetch::Fetched;
use incipit::GraphId;
use linked_data::GraphContribution;
use tokio::runtime::Builder;

mod frontier;
pub use frontier::*;
use frontier::{host_of, path_of, robots_url, sitemap_url};

mod robots;
pub use robots::CRAWLER_UA;
use robots::RobotsRules;

mod sitemap;

/// The minimum interval between fetches to the same host.
const POLITE_DELAY: Duration = Duration::from_secs(1);

/// One boxed async fetch operation for a crawled page.
pub type CrawlFetchFuture = Pin<Box<dyn Future<Output = Result<Fetched, String>> + Send>>;

/// The fetch seam the crawl actor runs through.
pub type CrawlFetch = Arc<dyn Fn(String) -> CrawlFetchFuture + Send + Sync>;

/// The per-page processing seam: enqueue discovered links into `frontier` and
/// return the graph contributions harvested from the fetched page.
pub type CrawlProcess =
    Arc<dyn Fn(&mut Frontier, &str, u32, &Fetched) -> Vec<GraphContribution> + Send + Sync>;

/// A command to the crawl actor.
pub enum CrawlCommand {
    /// Begin a bounded crawl from `seed` under `policy`.
    Start { seed: String, policy: CrawlPolicy },
}

/// An update from the crawl actor to its owner.
pub enum CrawlUpdate {
    Contribution {
        contributions: Vec<GraphContribution>,
    },
    Progress {
        fetched: usize,
        last_url: String,
    },
    Done {
        fetched: usize,
    },
}

/// Spawn the crawl actor on its own thread.
pub fn spawn_crawl(
    wake: Wake,
    fetch: CrawlFetch,
    process_page: CrawlProcess,
) -> (
    ActorHandle<CrawlCommand>,
    Receiver<CrawlUpdate>,
    Arc<AtomicBool>,
) {
    let cancel = Arc::new(AtomicBool::new(false));
    let actor_cancel = cancel.clone();
    let (handle, rx) = spawn(wake, move |commands, out: Emitter<CrawlUpdate>| {
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build the crawl runtime");
        while let Ok(command) = commands.recv() {
            match command {
                CrawlCommand::Start { seed, policy } => {
                    actor_cancel.store(false, Ordering::Relaxed);
                    let cancel = actor_cancel.clone();
                    let fetch = fetch.clone();
                    let process_page = process_page.clone();
                    runtime.block_on(run_crawl(
                        &seed,
                        policy,
                        POLITE_DELAY,
                        move |url| (fetch)(url),
                        || cancel.load(Ordering::Relaxed),
                        move |frontier, url, depth, fetched| {
                            (process_page)(frontier, url, depth, fetched)
                        },
                        |update| out.emit(update),
                    ));
                }
            }
        }
    });
    (handle, rx, cancel)
}

/// Drive a bounded crawl from `seed` under `policy`.
pub async fn run_crawl<F, Fut, P>(
    seed: &str,
    policy: CrawlPolicy,
    polite_delay: Duration,
    mut fetch: F,
    mut cancelled: impl FnMut() -> bool,
    mut process_page: P,
    mut emit: impl FnMut(CrawlUpdate),
) -> usize
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<Fetched, String>>,
    P: FnMut(&mut Frontier, &str, u32, &Fetched) -> Vec<GraphContribution>,
{
    let mut frontier = Frontier::new(seed, policy);
    if policy.seed_sitemap {
        if let Some(sitemap) = sitemap_url(seed) {
            if let Ok(fetched) = fetch(sitemap).await {
                let added = frontier.enqueue_seeds(&sitemap::parse_sitemap(&fetched.body));
                tracing::info!(seed, added, "sitemap-seeded the crawl");
            }
        }
    }
    let mut last_host_fetch: HashMap<String, Instant> = HashMap::new();
    let mut robots: HashMap<String, RobotsRules> = HashMap::new();
    while let Some((url, depth)) = frontier.next() {
        if cancelled() {
            break;
        }
        let host = host_of(&url).unwrap_or_default();
        if !robots.contains_key(&host) {
            let body = match robots_url(&url) {
                Some(ru) => fetch(ru).await.map(|f| f.body).unwrap_or_default(),
                None => String::new(),
            };
            robots.insert(host.clone(), RobotsRules::parse(&body));
        }
        if !robots[&host].allows(&path_of(&url)) {
            tracing::info!(%url, "robots.txt disallows; skipping");
            continue;
        }
        polite_wait(&url, polite_delay, &mut last_host_fetch).await;
        match fetch(url.clone()).await {
            Ok(fetched) => {
                let contributions = process_page(&mut frontier, &url, depth, &fetched);
                if !contributions.is_empty() {
                    emit(CrawlUpdate::Contribution { contributions });
                }
                emit(CrawlUpdate::Progress {
                    fetched: frontier.fetched(),
                    last_url: url,
                });
            }
            Err(error) => tracing::warn!(%url, %error, "crawl fetch failed; skipping"),
        }
    }
    let total = frontier.fetched();
    emit(CrawlUpdate::Done { fetched: total });
    total
}

/// Sleep so this host has not been fetched within `delay`, then stamp the fetch
/// time.
async fn polite_wait(url: &str, delay: Duration, last: &mut HashMap<String, Instant>) {
    if delay.is_zero() {
        return;
    }
    let host = host_of(url).unwrap_or_default();
    if let Some(prev) = last.get(&host) {
        let elapsed = prev.elapsed();
        if elapsed < delay {
            tokio::time::sleep(delay - elapsed).await;
        }
    }
    last.insert(host, Instant::now());
}

/// Progress of the active (or last) crawl.
#[derive(Clone, Debug, Default)]
pub struct CrawlProgress {
    pub fetched: usize,
    pub last_url: Option<String>,
    pub running: bool,
}

/// The host's owner of the crawl actor.
pub struct CrawlSession {
    handle: ActorHandle<CrawlCommand>,
    rx: Receiver<CrawlUpdate>,
    cancel: Arc<AtomicBool>,
    graph_id: Option<GraphId>,
    progress: CrawlProgress,
    default_policy: CrawlPolicy,
}

impl CrawlSession {
    /// Spawn the crawl actor using the supplied fetch and page-processing seams.
    pub fn new(wake: Wake, fetch: CrawlFetch, process_page: CrawlProcess) -> Self {
        let (handle, rx, cancel) = spawn_crawl(wake, fetch, process_page);
        Self {
            handle,
            rx,
            cancel,
            graph_id: None,
            progress: CrawlProgress::default(),
            default_policy: CrawlPolicy::default(),
        }
    }

    pub fn policy(&self) -> CrawlPolicy {
        self.default_policy
    }

    pub fn scope(&self) -> HostScope {
        self.default_policy.scope
    }

    pub fn max_depth(&self) -> u32 {
        self.default_policy.max_depth
    }

    pub fn set_scope(&mut self, scope: HostScope) {
        self.default_policy.scope = scope;
    }

    pub fn set_max_depth(&mut self, depth: u32) {
        self.default_policy.max_depth = depth;
    }

    pub fn seed_sitemap(&self) -> bool {
        self.default_policy.seed_sitemap
    }

    pub fn set_seed_sitemap(&mut self, on: bool) {
        self.default_policy.seed_sitemap = on;
    }

    pub fn max_pages(&self) -> usize {
        self.default_policy.max_pages
    }

    pub fn set_max_pages(&mut self, pages: usize) {
        self.default_policy.max_pages = pages;
    }

    pub fn start(&mut self, seed: &str, policy: CrawlPolicy, graph_id: GraphId) {
        self.graph_id = Some(graph_id);
        self.progress = CrawlProgress {
            fetched: 0,
            last_url: None,
            running: true,
        };
        self.handle.command(CrawlCommand::Start {
            seed: seed.to_string(),
            policy,
        });
    }

    pub fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn drain(&mut self) -> Vec<(GraphId, GraphContribution)> {
        let mut applied = Vec::new();
        while let Ok(update) = self.rx.try_recv() {
            fold_update(update, self.graph_id, &mut self.progress, &mut applied);
        }
        applied
    }

    pub fn progress(&self) -> &CrawlProgress {
        &self.progress
    }
}

/// Fold one [`CrawlUpdate`] into the host's view.
pub fn fold_update(
    update: CrawlUpdate,
    graph_id: Option<GraphId>,
    progress: &mut CrawlProgress,
    applied: &mut Vec<(GraphId, GraphContribution)>,
) {
    match update {
        CrawlUpdate::Contribution { contributions } => {
            if let Some(gid) = graph_id {
                applied.extend(contributions.into_iter().map(|c| (gid, c)));
            }
        }
        CrawlUpdate::Progress { fetched, last_url } => {
            progress.fetched = fetched;
            progress.last_url = Some(last_url);
            progress.running = true;
        }
        CrawlUpdate::Done { fetched } => {
            progress.fetched = fetched;
            progress.running = false;
        }
    }
}

#[cfg(test)]
mod tests;
