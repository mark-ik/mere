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
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use armillary::{ActorHandle, Emitter, Wake, spawn};
use frame::GraphId;
use linked_data::GraphContribution;
use tokio::runtime::Builder;

use crate::fetch::Fetched;

/// Which hosts a crawl may follow links into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostScope {
    /// Only the seed's exact host (`docs.example.com` stays on `docs.example.com`).
    SameHost,
    /// The seed's registrable domain — its host and any subdomain
    /// (`example.com` admits `docs.example.com`). Approximated by domain suffix; a
    /// full Public Suffix List is a follow-on (so `*.co.uk` is over-broad today).
    SameDomain,
    /// Any host. The open web; only sane with a tight depth/page cap and politeness.
    AnyHost,
}

/// How far and wide a crawl may roam — the bound the [`Frontier`] enforces.
#[derive(Clone, Copy, Debug)]
pub struct CrawlPolicy {
    /// Max link-hops from the seed: `0` fetches the seed only, `1` the seed and its
    /// direct links, and so on.
    pub max_depth: u32,
    /// Hard cap on total pages fetched — the runaway backstop.
    pub max_pages: usize,
    /// Max links enqueued from a single page (fan-out cap), so one hub page cannot
    /// flood the frontier.
    pub max_fanout: usize,
    /// Which hosts links may lead into.
    pub scope: HostScope,
}

impl Default for CrawlPolicy {
    /// Conservative: shallow, bounded, same-host. Deliberately small so an accidental
    /// crawl is cheap and polite by default.
    fn default() -> Self {
        Self { max_depth: 2, max_pages: 50, max_fanout: 20, scope: HostScope::SameHost }
    }
}

/// The crawl frontier: the ordered set of URLs still to fetch, with dedup and the
/// [`CrawlPolicy`] bounds. Pure — no I/O; the crawl actor supplies fetched links and
/// asks what to fetch next. Breadth-first (a `VecDeque`), so a depth cap yields a
/// tidy expanding neighborhood rather than a deep tunnel.
pub struct Frontier {
    policy: CrawlPolicy,
    seed_host: String,
    queue: VecDeque<(String, u32)>,
    /// Every URL ever enqueued (normalized), so a target is fetched at most once even
    /// when many pages link to it.
    seen: HashSet<String>,
    fetched: usize,
}

impl Frontier {
    /// Seed a crawl at `seed_url`, enqueued at depth 0. The seed's host scopes the
    /// `SameHost` / `SameDomain` policies.
    pub fn new(seed_url: &str, policy: CrawlPolicy) -> Self {
        let seed = normalize(seed_url);
        let seed_host = host_of(&seed).unwrap_or_default();
        let mut seen = HashSet::new();
        let mut queue = VecDeque::new();
        seen.insert(seed.clone());
        queue.push_back((seed, 0));
        Self { policy, seed_host, queue, seen, fetched: 0 }
    }

    /// The next `(url, depth)` to fetch, or `None` when the frontier is empty or the
    /// page cap is reached. Each call counts toward [`CrawlPolicy::max_pages`].
    pub fn next(&mut self) -> Option<(String, u32)> {
        if self.fetched >= self.policy.max_pages {
            return None;
        }
        let item = self.queue.pop_front()?;
        self.fetched += 1;
        Some(item)
    }

    /// Enqueue the (already-resolved, absolute) outbound `links` found on a page at
    /// `depth`. Applies the depth cap (children at `depth + 1`), the per-page fan-out
    /// cap, the host scope, and visited-dedup; non-http(s) URLs are dropped. Returns
    /// how many were newly enqueued.
    pub fn enqueue(&mut self, links: &[String], depth: u32) -> usize {
        let child_depth = depth + 1;
        if child_depth > self.policy.max_depth {
            return 0;
        }
        let mut added = 0;
        for link in links {
            if added >= self.policy.max_fanout {
                break;
            }
            let url = normalize(link);
            if !is_http(&url) || !self.in_scope(&url) || self.seen.contains(&url) {
                continue;
            }
            self.seen.insert(url.clone());
            self.queue.push_back((url, child_depth));
            added += 1;
        }
        added
    }

    /// Pages handed out by [`next`](Self::next) so far (against `max_pages`).
    pub fn fetched(&self) -> usize {
        self.fetched
    }

    /// Whether `url`'s host is within the policy scope.
    fn in_scope(&self, url: &str) -> bool {
        match self.policy.scope {
            HostScope::AnyHost => true,
            HostScope::SameHost => host_of(url).as_deref() == Some(self.seed_host.as_str()),
            HostScope::SameDomain => {
                host_of(url).is_some_and(|h| same_registrable_domain(&h, &self.seed_host))
            }
        }
    }
}

/// `url`'s host, lowercased, or `None` if it does not parse / has no host.
fn host_of(url: &str) -> Option<String> {
    url::Url::parse(url).ok()?.host_str().map(|h| h.to_ascii_lowercase())
}

/// Normalize a URL for dedup: parse, drop the fragment, re-serialize. Falls back to
/// the trimmed input when it does not parse, so a malformed link still dedups against
/// itself.
fn normalize(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(mut u) => {
            u.set_fragment(None);
            u.to_string()
        },
        Err(_) => url.trim().to_string(),
    }
}

/// Whether `url` is http(s) — the schemes a web crawl follows.
fn is_http(url: &str) -> bool {
    url::Url::parse(url)
        .map(|u| matches!(u.scheme(), "http" | "https"))
        .unwrap_or(false)
}

/// Approximate registrable-domain match: `host` equals `seed_host`, or `host` ends
/// with `.seed_host` (so `docs.example.com` matches a seed host of `example.com`).
/// Without a Public Suffix List this is over-broad for multi-label public suffixes
/// (it cannot tell `a.co.uk` from `b.co.uk`), an acknowledged follow-on; for the
/// common case it scopes a crawl to one site.
fn same_registrable_domain(host: &str, seed_host: &str) -> bool {
    host == seed_host || host.ends_with(&format!(".{seed_host}"))
}

// ---- the crawl actor (relational-browse V2) -----------------------------------

/// The minimum interval between fetches to the **same host** — basic politeness so a
/// crawl does not hammer one server. Different hosts are not throttled against each
/// other (the loop is sequential, so the real rate is already gentle). robots.txt
/// honoring and a configurable per-host rate are follow-ons.
const POLITE_DELAY: Duration = Duration::from_secs(1);

/// A command to the crawl actor.
pub enum CrawlCommand {
    /// Begin a bounded crawl from `seed` under `policy`. The policy's `max_pages`
    /// guarantees termination.
    Start { seed: String, policy: CrawlPolicy },
    /// Stop between crawls. (Mid-crawl cancellation via a side-channel flag is a
    /// follow-on; today a running crawl is bounded by `max_pages` and runs to
    /// completion, and a `Stop` queued during it is a no-op handled when it ends.)
    Stop,
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
/// is a distinct actor, not the sync per-tile one).
#[allow(dead_code)] // host wiring (spawn it + route CrawlUpdate::Contribution to the graph) is the follow-on
pub fn spawn_crawl(wake: Wake) -> (ActorHandle<CrawlCommand>, Receiver<CrawlUpdate>) {
    spawn(wake, |commands, out: Emitter<CrawlUpdate>| {
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
                    runtime.block_on(run_crawl(
                        &seed,
                        policy,
                        POLITE_DELAY,
                        |url| fetch_page_owned(url),
                        || false, // mid-crawl cancellation is a follow-on; max_pages bounds it
                        |update| out.emit(update),
                    ));
                },
                CrawlCommand::Stop => {}, // no crawl in progress between Starts
            }
        }
    })
}

/// Own the URL so the returned future is `'static` (the actor's `fetch` thunk takes a
/// `String`); routes by scheme exactly as the fetch actor does.
async fn fetch_page_owned(url: String) -> Result<Fetched, String> {
    crate::fetch::fetch_page(&url).await
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
    let mut last_host_fetch: HashMap<String, Instant> = HashMap::new();
    while let Some((url, depth)) = frontier.next() {
        if cancelled() {
            break;
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
/// contributions are applied in `app_handler`.
#[allow(dead_code)] // until the host Shell owns one + drains it (the final wiring step)
pub struct CrawlSession {
    handle: ActorHandle<CrawlCommand>,
    rx: Receiver<CrawlUpdate>,
    /// The graph crawled contributions land in (the seed page's graph).
    graph_id: Option<GraphId>,
    progress: CrawlProgress,
}

#[allow(dead_code)]
impl CrawlSession {
    /// Spawn the crawl actor; `wake` notifies the host loop when updates arrive, so it
    /// drains on the next frame — exactly like the content / fetch wakes.
    pub fn new(wake: Wake) -> Self {
        let (handle, rx) = spawn_crawl(wake);
        Self { handle, rx, graph_id: None, progress: CrawlProgress::default() }
    }

    /// Start a bounded crawl from `seed` under `policy`; its contributions route to
    /// `graph_id` (the seed page's graph). Supersedes any previous crawl's target.
    pub fn start(&mut self, seed: &str, policy: CrawlPolicy, graph_id: GraphId) {
        self.graph_id = Some(graph_id);
        self.progress = CrawlProgress { fetched: 0, last_url: None, running: true };
        self.handle.command(CrawlCommand::Start { seed: seed.to_string(), policy });
    }

    /// Ask the crawl to stop. A no-op between crawls; mid-crawl cancellation is a
    /// follow-on, so today the bounded policy stops a running crawl on its own.
    pub fn stop(&self) {
        self.handle.command(CrawlCommand::Stop);
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
mod tests {
    use super::*;

    fn policy(max_depth: u32, max_pages: usize, max_fanout: usize, scope: HostScope) -> CrawlPolicy {
        CrawlPolicy { max_depth, max_pages, max_fanout, scope }
    }

    #[test]
    fn seeds_then_drains() {
        let mut f = Frontier::new("https://x.test/", CrawlPolicy::default());
        assert_eq!(f.next(), Some(("https://x.test/".to_string(), 0)));
        assert_eq!(f.next(), None, "drained after the seed");
    }

    #[test]
    fn enqueue_respects_the_depth_cap() {
        let mut f = Frontier::new("https://x.test/", policy(1, 100, 100, HostScope::SameHost));
        // Depth 0 -> children at depth 1 (<= max_depth): enqueued.
        assert_eq!(f.enqueue(&["https://x.test/a".into()], 0), 1);
        // Depth 1 -> children at depth 2 (> max_depth): rejected wholesale.
        assert_eq!(f.enqueue(&["https://x.test/b".into()], 1), 0);
    }

    #[test]
    fn enqueue_respects_the_fanout_cap() {
        let mut f = Frontier::new("https://x.test/", policy(3, 100, 2, HostScope::SameHost));
        let links: Vec<String> = (0..5).map(|i| format!("https://x.test/p{i}")).collect();
        assert_eq!(f.enqueue(&links, 0), 2, "fan-out capped at 2");
    }

    #[test]
    fn enqueue_dedups_across_calls_and_within() {
        let mut f = Frontier::new("https://x.test/", policy(3, 100, 100, HostScope::SameHost));
        assert_eq!(
            f.enqueue(&["https://x.test/a".into(), "https://x.test/a".into()], 0),
            1,
            "a repeat within one batch counts once",
        );
        assert_eq!(
            f.enqueue(&["https://x.test/a".into()], 0),
            0,
            "an already-seen URL is not re-enqueued",
        );
    }

    #[test]
    fn fragments_dedup_to_one() {
        let mut f = Frontier::new("https://x.test/", policy(3, 100, 100, HostScope::SameHost));
        assert_eq!(
            f.enqueue(&["https://x.test/p#a".into(), "https://x.test/p#b".into()], 0),
            1,
            "the fragment is stripped for dedup",
        );
    }

    #[test]
    fn same_host_scope_filters_other_hosts() {
        let mut f = Frontier::new("https://x.test/", policy(3, 100, 100, HostScope::SameHost));
        let added = f.enqueue(&["https://x.test/a".into(), "https://other.test/b".into()], 0);
        assert_eq!(added, 1, "only the same-host link is enqueued");
    }

    #[test]
    fn same_domain_scope_admits_subdomains() {
        let mut f = Frontier::new("https://example.com/", policy(3, 100, 100, HostScope::SameDomain));
        let added = f.enqueue(
            &[
                "https://docs.example.com/a".into(), // subdomain: admitted
                "https://example.com/b".into(),      // apex: admitted
                "https://evil.test/c".into(),        // other domain: rejected
            ],
            0,
        );
        assert_eq!(added, 2);
    }

    #[test]
    fn any_host_scope_admits_everything_in_scheme() {
        let mut f = Frontier::new("https://x.test/", policy(3, 100, 100, HostScope::AnyHost));
        assert_eq!(
            f.enqueue(&["https://other.test/a".into(), "https://third.test/b".into()], 0),
            2,
        );
    }

    #[test]
    fn non_http_schemes_are_dropped() {
        let mut f = Frontier::new("https://x.test/", policy(3, 100, 100, HostScope::AnyHost));
        let added = f.enqueue(
            &["ftp://x.test/a".into(), "https://x.test/b".into(), "file:///etc".into()],
            0,
        );
        assert_eq!(added, 1, "only the http(s) link survives");
    }

    #[test]
    fn max_pages_caps_total_fetches() {
        let mut f = Frontier::new("https://x.test/", policy(5, 2, 100, HostScope::SameHost));
        f.enqueue(&["https://x.test/a".into(), "https://x.test/b".into()], 0);
        assert!(f.next().is_some(), "1st (seed)");
        assert!(f.next().is_some(), "2nd");
        assert_eq!(f.next(), None, "page cap of 2 stops the 3rd even with a non-empty queue");
        assert_eq!(f.fetched(), 2);
    }

    #[test]
    fn breadth_first_order() {
        // Depth-0 seed, then two depth-1 children, then a depth-2 grandchild: the
        // grandchild comes out after both depth-1 nodes (BFS), not interleaved.
        let mut f = Frontier::new("https://x.test/", policy(5, 100, 100, HostScope::SameHost));
        let _ = f.next(); // seed @0
        f.enqueue(&["https://x.test/a".into(), "https://x.test/b".into()], 0);
        let a = f.next().unwrap();
        assert_eq!(a, ("https://x.test/a".into(), 1));
        f.enqueue(&["https://x.test/a/child".into()], 1); // grandchild @2
        let b = f.next().unwrap();
        assert_eq!(b, ("https://x.test/b".into(), 1), "b (depth 1) before the grandchild");
        let g = f.next().unwrap();
        assert_eq!(g, ("https://x.test/a/child".into(), 2));
    }

    #[test]
    fn crawl_page_harvests_links_and_enqueues_them() {
        let mut f = Frontier::new("https://s.test/", CrawlPolicy::default());
        let _ = f.next(); // consume the seed @0
        let fetched = Fetched {
            content_type: Some("text/html".to_string()),
            body: "<a href='/a'>A</a><a href='/b'>B</a>".to_string(),
        };
        let contributions = crawl_page(&mut f, "https://s.test/", 0, &fetched);
        assert!(!contributions.is_empty(), "a link contribution is produced");
        // The two same-host targets are now queued at depth 1.
        assert_eq!(f.next(), Some(("https://s.test/a".to_string(), 1)));
        assert_eq!(f.next(), Some(("https://s.test/b".to_string(), 1)));
    }

    #[test]
    fn run_crawl_visits_a_small_site_within_the_depth_cap() {
        // A same-host site: seed -> /a, /b ; /a -> /c. With max_depth 1, /c (depth 2)
        // is never fetched. A canned fetcher stands in for the network; zero delay.
        let site: HashMap<String, &str> = [
            ("https://s.test/".to_string(), "<a href='/a'>A</a><a href='/b'>B</a>"),
            ("https://s.test/a".to_string(), "<a href='/c'>C</a>"),
            ("https://s.test/b".to_string(), "<p>leaf</p>"),
            ("https://s.test/c".to_string(), "<p>deep</p>"),
        ]
        .into_iter()
        .collect();
        let fetch = move |url: String| {
            let body = site.get(&url).copied().unwrap_or("").to_string();
            async move {
                Ok::<_, String>(Fetched { content_type: Some("text/html".to_string()), body })
            }
        };

        let mut visited: Vec<String> = Vec::new();
        let mut contribution_count = 0usize;
        let rt = Builder::new_current_thread().enable_all().build().unwrap();
        let total = rt.block_on(run_crawl(
            "https://s.test/",
            CrawlPolicy { max_depth: 1, max_pages: 50, max_fanout: 20, scope: HostScope::SameHost },
            Duration::ZERO,
            fetch,
            || false,
            |update| match update {
                CrawlUpdate::Progress { last_url, .. } => visited.push(last_url),
                CrawlUpdate::Contribution { .. } => contribution_count += 1,
                CrawlUpdate::Done { .. } => {},
            },
        ));

        assert_eq!(total, 3, "seed + /a + /b fetched; /c is beyond the depth cap");
        assert!(visited.contains(&"https://s.test/".to_string()));
        assert!(visited.contains(&"https://s.test/a".to_string()));
        assert!(visited.contains(&"https://s.test/b".to_string()));
        assert!(!visited.iter().any(|u| u.ends_with("/c")), "/c not fetched: {visited:?}");
        assert!(contribution_count >= 1, "the seed's link neighborhood contributed");
    }

    #[test]
    fn run_crawl_stops_at_the_page_cap() {
        // A hub linking to many same-host pages, capped at 3 total fetches.
        let mut site: HashMap<String, String> = HashMap::new();
        let hub: String = (0..10).map(|i| format!("<a href='/p{i}'>{i}</a>")).collect();
        site.insert("https://s.test/".to_string(), hub);
        for i in 0..10 {
            site.insert(format!("https://s.test/p{i}"), "<p>leaf</p>".to_string());
        }
        let fetch = move |url: String| {
            let body = site.get(&url).cloned().unwrap_or_default();
            async move {
                Ok::<_, String>(Fetched { content_type: Some("text/html".to_string()), body })
            }
        };
        let rt = Builder::new_current_thread().enable_all().build().unwrap();
        let total = rt.block_on(run_crawl(
            "https://s.test/",
            CrawlPolicy { max_depth: 5, max_pages: 3, max_fanout: 20, scope: HostScope::SameHost },
            Duration::ZERO,
            fetch,
            || false,
            |_| {},
        ));
        assert_eq!(total, 3, "the page cap stops the crawl at 3 fetches");
    }

    #[test]
    fn fold_routes_contributions_to_the_graph_and_tracks_progress() {
        let gid = GraphId::nil();
        let mut progress = CrawlProgress::default();
        let mut applied: Vec<(GraphId, GraphContribution)> = Vec::new();

        fold_update(
            CrawlUpdate::Progress { fetched: 2, last_url: "https://x.test/a".to_string() },
            Some(gid),
            &mut progress,
            &mut applied,
        );
        assert_eq!(progress.fetched, 2);
        assert_eq!(progress.last_url.as_deref(), Some("https://x.test/a"));
        assert!(progress.running);

        fold_update(
            CrawlUpdate::Contribution { contributions: vec![GraphContribution::default()] },
            Some(gid),
            &mut progress,
            &mut applied,
        );
        assert_eq!(applied.len(), 1, "the contribution routed to the crawl's graph");
        assert_eq!(applied[0].0, gid);

        fold_update(CrawlUpdate::Done { fetched: 3 }, Some(gid), &mut progress, &mut applied);
        assert_eq!(progress.fetched, 3);
        assert!(!progress.running, "Done clears the running flag");

        // With no target graph set, a contribution has nowhere to route and is dropped.
        let mut orphaned = Vec::new();
        fold_update(
            CrawlUpdate::Contribution { contributions: vec![GraphContribution::default()] },
            None,
            &mut progress,
            &mut orphaned,
        );
        assert!(orphaned.is_empty());
    }
}
