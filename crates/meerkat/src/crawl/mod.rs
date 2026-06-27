/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The crawl frontier (and, slice 2, the dedicated crawl actor).
//!
//! Relational-browse V2: walk a seed's link neighborhood across pages, off the
//! render path. This module's pure half is the [`Frontier`] — the queue, the visited
//! set, and the depth / fan-out / host-scope policy that decides *which* URLs to
//! fetch and in what order, with no I/O. The crawl actor (slice 2) drives it: pop a
//! URL, fetch it politely through `fetch_page`, run the V1 link materializer +
//! reader-mode extract on the body, enqueue the new in-scope links, and emit the
//! contributions to the graph. Keeping the policy pure makes the crawl's bounds
//! unit-testable without the network — a runaway crawl is a logic bug caught here,
//! not a politeness incident caught in production.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use armillary::{ActorHandle, Emitter, Wake, spawn};
use frame::GraphId;
use linked_data::GraphContribution;
use tokio::runtime::Builder;

use crate::fetch::Fetched;

mod robots;
mod sitemap;
use robots::RobotsRules;


mod frontier;
pub use frontier::*;

// ---- the crawl actor (relational-browse V2) -----------------------------------

/// The minimum interval between fetches to the **same host** — basic politeness so a
/// crawl does not hammer one server. Different hosts are not throttled against each
/// other (the loop is sequential, so the real rate is already gentle). robots.txt
/// honoring and a configurable per-host rate are follow-ons.
const POLITE_DELAY: Duration = Duration::from_secs(1);

/// A command to the crawl actor.
pub enum CrawlCommand {
    /// Begin a bounded crawl from `seed` under `policy`. The policy's `max_pages`
    /// guarantees termination; a [`CrawlSession::stop`] cancels it mid-flight (the
    /// loop polls a shared flag between pages — the command channel is not used for
    /// cancellation, since the actor is busy in the crawl while it runs).
    Start { seed: String, policy: CrawlPolicy },
}

/// An update from the crawl actor to its owner (the kernel applies the contributions
/// to the graph, like the content actor's). All variants are `Send`.
pub enum CrawlUpdate {
    /// Graph contributions harvested from one crawled page: its outbound-link
    /// neighborhood (`Semantic:Hyperlink` edges) plus the page node's metadata.
    Contribution { contributions: Vec<GraphContribution> },
    /// Progress after a page: total pages fetched and the URL just processed.
    Progress { fetched: usize, last_url: String },
    /// The crawl finished (frontier drained or page cap reached) after `fetched` pages.
    Done { fetched: usize },
}

/// The dedicated crawl actor (relational-browse V2). It owns a tokio runtime on its
/// own thread; a `Start` runs a bounded, polite crawl to completion, emitting each
/// page's contributions and progress. Off the render path — it never blocks
/// compositing or the per-tile content actors (that separation is the whole reason it
/// is a distinct actor, not the sync per-tile one). Owned by the host through
/// [`CrawlSession`]. Returns the actor handle, its update channel, and the shared
/// **cancel flag** the loop polls between pages — [`CrawlSession::stop`] sets it to
/// stop a running crawl mid-flight.
pub fn spawn_crawl(
    wake: Wake,
) -> (ActorHandle<CrawlCommand>, Receiver<CrawlUpdate>, Arc<AtomicBool>) {
    let cancel = Arc::new(AtomicBool::new(false));
    let actor_cancel = cancel.clone();
    let (handle, rx) = spawn(wake, move |commands, out: Emitter<CrawlUpdate>| {
        // The runtime is built on the actor thread (never moved across the boundary),
        // current-thread because the crawl awaits one fetch at a time (sequential =
        // inherently polite). Only the `Send` handle and `Send` updates cross.
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build the crawl runtime");
        while let Ok(command) = commands.recv() {
            match command {
                CrawlCommand::Start { seed, policy } => {
                    // A fresh crawl starts un-cancelled, even if a prior `stop` set the
                    // flag while nothing was running. (Done here, not in `start`, so a
                    // stop racing the next start can't un-cancel the *current* crawl.)
                    actor_cancel.store(false, Ordering::Relaxed);
                    let cancel = actor_cancel.clone();
                    runtime.block_on(run_crawl(
                        &seed,
                        policy,
                        POLITE_DELAY,
                        |url| fetch_page_owned(url),
                        || cancel.load(Ordering::Relaxed),
                        |update| out.emit(update),
                    ));
                },
            }
        }
    });
    (handle, rx, cancel)
}

/// Own the URL so the returned future is `'static` (the actor's `fetch` thunk takes a
/// `String`); fetches as the crawler (descriptive bot User-Agent), routing by scheme.
async fn fetch_page_owned(url: String) -> Result<Fetched, String> {
    crate::fetch::fetch_page_crawler(&url).await
}

/// Drive a bounded crawl from `seed` under `policy`: pop the next URL from the
/// frontier, wait `polite_delay` since the last fetch of its host, fetch it through
/// `fetch`, harvest its links + metadata, enqueue the in-scope links, and report
/// through `emit`. Generic over the fetcher and reporter so the whole loop is testable
/// against a canned site with no network and no real delay. `cancelled` is polled
/// between pages. Returns the page count.
async fn run_crawl<F, Fut>(
    seed: &str,
    policy: CrawlPolicy,
    polite_delay: Duration,
    mut fetch: F,
    mut cancelled: impl FnMut() -> bool,
    mut emit: impl FnMut(CrawlUpdate),
) -> usize
where
    F: FnMut(String) -> Fut,
    Fut: std::future::Future<Output = Result<Fetched, String>>,
{
    let mut frontier = Frontier::new(seed, policy);
    // Seed from the site's sitemap.xml when asked, for a comprehensive site crawl
    // (its declared pages, not only what the seed links). Best-effort: a missing /
    // failed sitemap just leaves the link-following crawl. `max_pages` still bounds it.
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
        // Honor robots.txt: fetch + cache each host's rules once, then skip a
        // disallowed path. A missing / failed robots.txt allows everything (per spec).
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
                let contributions = crawl_page(&mut frontier, &url, depth, &fetched);
                if !contributions.is_empty() {
                    emit(CrawlUpdate::Contribution { contributions });
                }
                emit(CrawlUpdate::Progress { fetched: frontier.fetched(), last_url: url });
            },
            Err(error) => tracing::warn!(%url, %error, "crawl fetch failed; skipping"),
        }
    }
    let total = frontier.fetched();
    emit(CrawlUpdate::Done { fetched: total });
    total
}

/// Process one fetched page: harvest its outbound links (enqueuing the in-scope ones
/// into `frontier` at `depth + 1`) and extract the page node's declared metadata,
/// returning the graph contributions. Non-HTML bodies yield nothing. Pure: no I/O, so
/// the per-page crawl step is unit-testable on its own. (The reader-mode article body,
/// `main_text`, attaches here when V3 routes page text into the corpus.)
fn crawl_page(
    frontier: &mut Frontier,
    url: &str,
    depth: u32,
    fetched: &Fetched,
) -> Vec<GraphContribution> {
    let mut contributions = Vec::new();
    if let Some(links) = meerkat::ingest::harvest_links(url, &fetched.body) {
        // Feed the frontier the resolved target URLs (the edge objects), so the next
        // hop follows the same links just materialized into the graph.
        let targets: Vec<String> = links.edges.iter().map(|e| e.object.clone()).collect();
        frontier.enqueue(&targets, depth);
        contributions.push(links);
    }
    if let Some(meta) =
        meerkat::ingest::page_extract_contribution(url, fetched.content_type.as_deref(), &fetched.body)
    {
        contributions.push(meta);
    }
    contributions
}

/// Sleep so this host has not been fetched within `delay`, then stamp the fetch time.
/// A no-op for a zero delay (tests) or a host's first fetch.
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

// ---- the host's crawl owner ---------------------------------------------------

/// Progress of the active (or last) crawl, for a host status surface (a chip, a log).
#[derive(Clone, Debug, Default)]
pub struct CrawlProgress {
    /// Pages fetched so far.
    pub fetched: usize,
    /// The URL last processed, if any.
    pub last_url: Option<String>,
    /// Whether a crawl is still running (cleared once `Done` arrives).
    pub running: bool,
}

/// The host's owner of the crawl actor — the crawl counterpart to `Constellation`'s
/// ownership of content actors. It spawns the actor, starts a crawl into a target
/// graph, drains its updates each frame into `(GraphId, GraphContribution)` pairs the
/// host applies through the **same** `apply_contribution` path content harvests use,
/// and tracks progress. One crawl at a time (V2).
///
/// Host wiring (on the crawl wake): `for (gid, c) in crawl.drain() { orrery.ingest_graph(
/// |g| apply_contribution(g, &c)) }` — identical to how `Constellation::drain`'s
/// contributions are applied in `app_handler`. The host (`SharedState::content`) owns
/// one; `>crawl` on a focused page calls [`start`](Self::start), and `app_handler`
/// drains it each frame.
pub struct CrawlSession {
    handle: ActorHandle<CrawlCommand>,
    rx: Receiver<CrawlUpdate>,
    /// Shared with the actor: set by [`stop`](Self::stop) to cancel a running crawl;
    /// the actor's loop polls it between pages and clears it at each `start`.
    cancel: Arc<AtomicBool>,
    /// The graph crawled contributions land in (the seed page's graph).
    graph_id: Option<GraphId>,
    progress: CrawlProgress,
    /// The policy a `>crawl` starts under, configured by the settings lane's scope /
    /// depth picker (and restored from disk). `start` can still override it per call.
    default_policy: CrawlPolicy,
}

impl CrawlSession {
    /// Spawn the crawl actor; `wake` notifies the host loop when updates arrive, so it
    /// drains on the next frame — exactly like the content / fetch wakes.
    pub fn new(wake: Wake) -> Self {
        let (handle, rx, cancel) = spawn_crawl(wake);
        Self {
            handle,
            rx,
            cancel,
            graph_id: None,
            progress: CrawlProgress::default(),
            default_policy: CrawlPolicy::default(),
        }
    }

    /// The policy a `>crawl` starts under (scope / depth from the settings lane).
    pub fn policy(&self) -> CrawlPolicy {
        self.default_policy
    }

    /// The configured crawl scope (for the settings picker's current selection).
    pub fn scope(&self) -> HostScope {
        self.default_policy.scope
    }

    /// The configured crawl depth (for the settings picker's current selection).
    pub fn max_depth(&self) -> u32 {
        self.default_policy.max_depth
    }

    /// Set the scope a `>crawl` roams under (settings lane). Does not affect a crawl
    /// already running.
    pub fn set_scope(&mut self, scope: HostScope) {
        self.default_policy.scope = scope;
    }

    /// Set the depth a `>crawl` reaches (settings lane). Does not affect a crawl
    /// already running.
    pub fn set_max_depth(&mut self, depth: u32) {
        self.default_policy.max_depth = depth;
    }

    /// Whether a `>crawl` seeds from the site's `sitemap.xml` — its "crawl whole site"
    /// mode (vs. the focused neighborhood of the seed page's own links).
    pub fn seed_sitemap(&self) -> bool {
        self.default_policy.seed_sitemap
    }

    /// Set the "crawl whole site" mode (settings lane). Does not affect a crawl already
    /// running. Still bounded by `max_pages`.
    pub fn set_seed_sitemap(&mut self, on: bool) {
        self.default_policy.seed_sitemap = on;
    }

    /// The hard page cap a `>crawl` stops at — the runaway backstop (settings lane).
    pub fn max_pages(&self) -> usize {
        self.default_policy.max_pages
    }

    /// Set the page cap a `>crawl` stops at (settings lane). The bound that actually
    /// limits a wide crawl (`AnyHost` / whole-site), so it pairs with those. Does not
    /// affect a crawl already running.
    pub fn set_max_pages(&mut self, pages: usize) {
        self.default_policy.max_pages = pages;
    }

    /// Start a bounded crawl from `seed` under `policy`; its contributions route to
    /// `graph_id` (the seed page's graph). Supersedes any previous crawl's target.
    pub fn start(&mut self, seed: &str, policy: CrawlPolicy, graph_id: GraphId) {
        self.graph_id = Some(graph_id);
        self.progress = CrawlProgress { fetched: 0, last_url: None, running: true };
        self.handle.command(CrawlCommand::Start { seed: seed.to_string(), policy });
    }

    /// Cancel a running crawl. Sets the shared flag the actor's loop polls between
    /// pages, so the crawl stops at the next page boundary and emits `Done`. A no-op
    /// between crawls (the next `start` clears it). The host trigger (a stop-crawl
    /// command / button) drives it (`>crawl_stop`).
    pub fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Drain pending crawl updates into the `(graph, contribution)` pairs the host
    /// applies, folding progress in. Empty when nothing is pending.
    pub fn drain(&mut self) -> Vec<(GraphId, GraphContribution)> {
        let mut applied = Vec::new();
        while let Ok(update) = self.rx.try_recv() {
            fold_update(update, self.graph_id, &mut self.progress, &mut applied);
        }
        applied
    }

    /// The latest crawl progress, for a host status surface.
    pub fn progress(&self) -> &CrawlProgress {
        &self.progress
    }
}

/// Fold one [`CrawlUpdate`] into the host's view: route a `Contribution` to the
/// crawl's graph, track `Progress` / `Done`. A free function so the mapping is
/// unit-testable without spawning the actor.
fn fold_update(
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
        },
        CrawlUpdate::Progress { fetched, last_url } => {
            progress.fetched = fetched;
            progress.last_url = Some(last_url);
            progress.running = true;
        },
        CrawlUpdate::Done { fetched } => {
            progress.fetched = fetched;
            progress.running = false;
        },
    }
}

#[cfg(test)]
mod tests;
