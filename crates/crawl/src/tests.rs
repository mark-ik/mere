use super::*;

fn policy(max_depth: u32, max_pages: usize, max_fanout: usize, scope: HostScope) -> CrawlPolicy {
    CrawlPolicy {
        max_depth,
        max_pages,
        max_fanout,
        scope,
        seed_sitemap: false,
    }
}

fn test_process_page(
    frontier: &mut Frontier,
    url: &str,
    depth: u32,
    fetched: &Fetched,
) -> Vec<GraphContribution> {
    if enqueue_fetched_html_links(frontier, url, depth, fetched) == 0 {
        Vec::new()
    } else {
        vec![GraphContribution::default()]
    }
}

#[test]
fn host_scope_key_round_trips() {
    // The settings picker + on-disk pref round-trip scopes through their string keys.
    for scope in [
        HostScope::SameHost,
        HostScope::SameDomain,
        HostScope::AnyHost,
    ] {
        assert_eq!(
            HostScope::from_key(scope.as_key()),
            Some(scope),
            "{scope:?}"
        );
    }
    assert_eq!(HostScope::from_key("nonsense"), None);
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
fn enqueue_seeds_ignores_the_fanout_cap() {
    // Bulk seeds (a sitemap) are not one page's links; the fan-out cap does not apply.
    let mut f = Frontier::new("https://x.test/", policy(3, 100, 2, HostScope::SameHost));
    let urls: Vec<String> = (0..5).map(|i| format!("https://x.test/s{i}")).collect();
    assert_eq!(
        f.enqueue_seeds(&urls),
        5,
        "all 5 seeds enqueued despite a fan-out cap of 2"
    );
    // Host scope + dedup still apply to seeds.
    assert_eq!(
        f.enqueue_seeds(&["https://other.test/x".into(), "https://x.test/s0".into()]),
        0,
        "off-host rejected, already-seen rejected",
    );
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
        f.enqueue(
            &["https://x.test/p#a".into(), "https://x.test/p#b".into()],
            0
        ),
        1,
        "the fragment is stripped for dedup",
    );
}

#[test]
fn same_host_scope_filters_other_hosts() {
    let mut f = Frontier::new("https://x.test/", policy(3, 100, 100, HostScope::SameHost));
    let added = f.enqueue(
        &["https://x.test/a".into(), "https://other.test/b".into()],
        0,
    );
    assert_eq!(added, 1, "only the same-host link is enqueued");
}

#[test]
fn same_domain_scope_admits_subdomains() {
    let mut f = Frontier::new(
        "https://example.com/",
        policy(3, 100, 100, HostScope::SameDomain),
    );
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
        f.enqueue(
            &["https://other.test/a".into(), "https://third.test/b".into()],
            0
        ),
        2,
    );
}

#[test]
fn non_http_schemes_are_dropped() {
    let mut f = Frontier::new("https://x.test/", policy(3, 100, 100, HostScope::AnyHost));
    let added = f.enqueue(
        &[
            "ftp://x.test/a".into(),
            "https://x.test/b".into(),
            "file:///etc".into(),
        ],
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
    assert_eq!(
        f.next(),
        None,
        "page cap of 2 stops the 3rd even with a non-empty queue"
    );
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
    assert_eq!(
        b,
        ("https://x.test/b".into(), 1),
        "b (depth 1) before the grandchild"
    );
    let g = f.next().unwrap();
    assert_eq!(g, ("https://x.test/a/child".into(), 2));
}

#[test]
fn process_page_enqueues_links_and_emits_a_contribution() {
    let mut f = Frontier::new("https://s.test/", CrawlPolicy::default());
    let _ = f.next(); // consume the seed @0
    let fetched = Fetched::text(
        Some("text/html".to_string()),
        "<a href='/a'>A</a><a href='/b'>B</a>",
    );
    let contributions = test_process_page(&mut f, "https://s.test/", 0, &fetched);
    assert!(!contributions.is_empty(), "a link contribution is produced");
    // The two same-host targets are now queued at depth 1.
    assert_eq!(f.next(), Some(("https://s.test/a".to_string(), 1)));
    assert_eq!(f.next(), Some(("https://s.test/b".to_string(), 1)));
}

#[test]
fn fetched_html_links_resolve_then_apply_frontier_policy() {
    let mut f = Frontier::new(
        "https://example.test/guide/index.html",
        policy(1, 100, 10, HostScope::SameHost),
    );
    let _ = f.next();
    let fetched = Fetched::text(
        Some("text/html; charset=utf-8".to_string()),
        r#"<a href="chapter.html">chapter</a>
           <a href="/guide/index.html#again">duplicate seed</a>
           <a href="https://elsewhere.test/outside">outside</a>
           <a href="mailto:editor@example.test">mail</a>
           <a href="chapter.html#section">duplicate chapter</a>"#,
    );

    assert_eq!(
        enqueue_fetched_html_links(&mut f, "https://example.test/guide/index.html", 0, &fetched),
        1,
        "relative links resolve before Frontier rejects off-scope, non-http, and duplicate URLs",
    );
    assert_eq!(
        f.next(),
        Some(("https://example.test/guide/chapter.html".to_string(), 1)),
    );
    assert_eq!(f.next(), None);
}

#[test]
fn fetched_non_html_response_bypasses_fleece() {
    let mut f = Frontier::new("https://example.test/", CrawlPolicy::default());
    let _ = f.next();
    let fetched = Fetched::text(
        Some("application/json".to_string()),
        r#"{"href":"/not-a-document-link"}"#,
    );

    assert_eq!(
        enqueue_fetched_html_links(&mut f, "https://example.test/", 0, &fetched),
        0,
    );
    assert_eq!(f.next(), None);
}

#[test]
fn fetched_html_links_respect_the_depth_cap() {
    let mut f = Frontier::new(
        "https://example.test/",
        policy(0, 100, 10, HostScope::SameHost),
    );
    let _ = f.next();
    let fetched = Fetched::text(
        Some("text/html".to_string()),
        "<a href='/beyond-the-cap'>too deep</a>",
    );

    assert_eq!(
        enqueue_fetched_html_links(&mut f, "https://example.test/", 0, &fetched),
        0,
    );
    assert_eq!(f.next(), None);
}

#[test]
fn run_crawl_visits_a_small_site_within_the_depth_cap() {
    // A same-host site: seed -> /a, /b ; /a -> /c. With max_depth 1, /c (depth 2)
    // is never fetched. A canned fetcher stands in for the network; zero delay.
    let site: HashMap<String, &str> = [
        (
            "https://s.test/".to_string(),
            "<a href='/a'>A</a><a href='/b'>B</a>",
        ),
        ("https://s.test/a".to_string(), "<a href='/c'>C</a>"),
        ("https://s.test/b".to_string(), "<p>leaf</p>"),
        ("https://s.test/c".to_string(), "<p>deep</p>"),
    ]
    .into_iter()
    .collect();
    let fetch = move |url: String| {
        let body = site.get(&url).copied().unwrap_or("").to_string();
        async move { Ok::<_, String>(Fetched::text(Some("text/html".to_string()), body)) }
    };

    let mut visited: Vec<String> = Vec::new();
    let mut contribution_count = 0usize;
    let rt = Builder::new_current_thread().enable_all().build().unwrap();
    let total = rt.block_on(run_crawl(
        "https://s.test/",
        CrawlPolicy {
            max_depth: 1,
            max_pages: 50,
            max_fanout: 20,
            scope: HostScope::SameHost,
            seed_sitemap: false,
        },
        Duration::ZERO,
        fetch,
        || false,
        test_process_page,
        |update| match update {
            CrawlUpdate::Progress { last_url, .. } => visited.push(last_url),
            CrawlUpdate::Contribution { .. } => contribution_count += 1,
            CrawlUpdate::Done { .. } => {}
        },
    ));

    assert_eq!(
        total, 3,
        "seed + /a + /b fetched; /c is beyond the depth cap"
    );
    assert!(visited.contains(&"https://s.test/".to_string()));
    assert!(visited.contains(&"https://s.test/a".to_string()));
    assert!(visited.contains(&"https://s.test/b".to_string()));
    assert!(
        !visited.iter().any(|u| u.ends_with("/c")),
        "/c not fetched: {visited:?}"
    );
    assert!(
        contribution_count >= 1,
        "the seed's link neighborhood contributed"
    );
}

#[test]
fn run_crawl_stops_at_the_page_cap() {
    // A hub linking to many same-host pages, capped at 3 total fetches.
    let mut site: HashMap<String, String> = HashMap::new();
    let hub: String = (0..10)
        .map(|i| format!("<a href='/p{i}'>{i}</a>"))
        .collect();
    site.insert("https://s.test/".to_string(), hub);
    for i in 0..10 {
        site.insert(format!("https://s.test/p{i}"), "<p>leaf</p>".to_string());
    }
    let fetch = move |url: String| {
        let body = site.get(&url).cloned().unwrap_or_default();
        async move { Ok::<_, String>(Fetched::text(Some("text/html".to_string()), body)) }
    };
    let rt = Builder::new_current_thread().enable_all().build().unwrap();
    let total = rt.block_on(run_crawl(
        "https://s.test/",
        CrawlPolicy {
            max_depth: 5,
            max_pages: 3,
            max_fanout: 20,
            scope: HostScope::SameHost,
            seed_sitemap: false,
        },
        Duration::ZERO,
        fetch,
        || false,
        test_process_page,
        |_| {},
    ));
    assert_eq!(total, 3, "the page cap stops the crawl at 3 fetches");
}

#[test]
fn run_crawl_honors_robots_disallow() {
    // robots.txt disallows /private; the crawl fetches the seed and /ok but skips
    // the enqueued /private/secret. A canned fetcher serves robots.txt too.
    let site: HashMap<String, &str> = [
        (
            "https://s.test/robots.txt".to_string(),
            "User-agent: *\nDisallow: /private",
        ),
        (
            "https://s.test/".to_string(),
            "<a href='/ok'>ok</a><a href='/private/secret'>x</a>",
        ),
        ("https://s.test/ok".to_string(), "<p>ok</p>"),
        (
            "https://s.test/private/secret".to_string(),
            "<p>should be skipped</p>",
        ),
    ]
    .into_iter()
    .collect();
    let fetch = move |url: String| {
        let body = site.get(&url).copied().unwrap_or("").to_string();
        async move { Ok::<_, String>(Fetched::text(Some("text/html".to_string()), body)) }
    };
    let mut visited: Vec<String> = Vec::new();
    let rt = Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(run_crawl(
        "https://s.test/",
        CrawlPolicy {
            max_depth: 2,
            max_pages: 50,
            max_fanout: 20,
            scope: HostScope::SameHost,
            seed_sitemap: false,
        },
        Duration::ZERO,
        fetch,
        || false,
        test_process_page,
        |update| {
            if let CrawlUpdate::Progress { last_url, .. } = update {
                visited.push(last_url);
            }
        },
    ));
    assert!(
        visited.contains(&"https://s.test/".to_string()),
        "seed fetched"
    );
    assert!(
        visited.contains(&"https://s.test/ok".to_string()),
        "allowed path fetched"
    );
    assert!(
        !visited.iter().any(|u| u.contains("/private")),
        "robots-disallowed path skipped: {visited:?}",
    );
}

#[test]
fn run_crawl_cancels_mid_crawl() {
    // A hub linking to ten same-host pages; cancel after the first fetch, so only
    // the seed is crawled even though the page cap is far higher. This is the
    // mechanism `CrawlSession::stop` drives (it flips the polled flag to `true`).
    let mut site: HashMap<String, String> = HashMap::new();
    let hub: String = (0..10)
        .map(|i| format!("<a href='/p{i}'>{i}</a>"))
        .collect();
    site.insert("https://s.test/".to_string(), hub);
    for i in 0..10 {
        site.insert(format!("https://s.test/p{i}"), "<p>leaf</p>".to_string());
    }
    let fetch = move |url: String| {
        let body = site.get(&url).cloned().unwrap_or_default();
        async move { Ok::<_, String>(Fetched::text(Some("text/html".to_string()), body)) }
    };
    // Let the first page through, then cancel before the second.
    let mut allow_one = true;
    let cancelled = move || {
        if allow_one {
            allow_one = false;
            false
        } else {
            true
        }
    };
    let mut fetched_pages = 0;
    let rt = Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(run_crawl(
        "https://s.test/",
        CrawlPolicy {
            max_depth: 5,
            max_pages: 50,
            max_fanout: 20,
            scope: HostScope::SameHost,
            seed_sitemap: false,
        },
        Duration::ZERO,
        fetch,
        cancelled,
        test_process_page,
        |update| {
            if matches!(update, CrawlUpdate::Progress { .. }) {
                fetched_pages += 1;
            }
        },
    ));
    assert_eq!(
        fetched_pages, 1,
        "cancellation stopped the crawl after the first fetched page"
    );
}

#[test]
fn run_crawl_seeds_from_the_sitemap() {
    // The seed page links nothing, but the sitemap lists two pages; with
    // seed_sitemap on, the crawl discovers and fetches them anyway.
    let site: HashMap<String, &str> = [
        (
            "https://s.test/sitemap.xml".to_string(),
            "<urlset><url><loc>https://s.test/a</loc></url>\
                 <url><loc>https://s.test/b</loc></url></urlset>",
        ),
        ("https://s.test/".to_string(), "<p>no links here</p>"),
        ("https://s.test/a".to_string(), "<p>page a</p>"),
        ("https://s.test/b".to_string(), "<p>page b</p>"),
    ]
    .into_iter()
    .collect();
    let fetch = move |url: String| {
        let body = site.get(&url).copied().unwrap_or("").to_string();
        async move { Ok::<_, String>(Fetched::text(Some("text/html".to_string()), body)) }
    };
    let mut visited: Vec<String> = Vec::new();
    let rt = Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(run_crawl(
        "https://s.test/",
        CrawlPolicy {
            max_depth: 2,
            max_pages: 50,
            max_fanout: 20,
            scope: HostScope::SameHost,
            seed_sitemap: true,
        },
        Duration::ZERO,
        fetch,
        || false,
        test_process_page,
        |update| {
            if let CrawlUpdate::Progress { last_url, .. } = update {
                visited.push(last_url);
            }
        },
    ));
    assert!(
        visited.contains(&"https://s.test/a".to_string()),
        "sitemap page a crawled: {visited:?}"
    );
    assert!(
        visited.contains(&"https://s.test/b".to_string()),
        "sitemap page b crawled: {visited:?}"
    );
}

#[test]
fn fold_routes_contributions_to_the_graph_and_tracks_progress() {
    let gid = GraphId::nil();
    let mut progress = CrawlProgress::default();
    let mut applied: Vec<(GraphId, GraphContribution)> = Vec::new();

    fold_update(
        CrawlUpdate::Progress {
            fetched: 2,
            last_url: "https://x.test/a".to_string(),
        },
        Some(gid),
        &mut progress,
        &mut applied,
    );
    assert_eq!(progress.fetched, 2);
    assert_eq!(progress.last_url.as_deref(), Some("https://x.test/a"));
    assert!(progress.running);

    fold_update(
        CrawlUpdate::Contribution {
            contributions: vec![GraphContribution::default()],
        },
        Some(gid),
        &mut progress,
        &mut applied,
    );
    assert_eq!(
        applied.len(),
        1,
        "the contribution routed to the crawl's graph"
    );
    assert_eq!(applied[0].0, gid);

    fold_update(
        CrawlUpdate::Done { fetched: 3 },
        Some(gid),
        &mut progress,
        &mut applied,
    );
    assert_eq!(progress.fetched, 3);
    assert!(!progress.running, "Done clears the running flag");

    // With no target graph set, a contribution has nowhere to route and is dropped.
    let mut orphaned = Vec::new();
    fold_update(
        CrawlUpdate::Contribution {
            contributions: vec![GraphContribution::default()],
        },
        None,
        &mut progress,
        &mut orphaned,
    );
    assert!(orphaned.is_empty());
}
