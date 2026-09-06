/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The remote half of a host's resource fetcher, [`RemoteFetcher`]: http(s)
//! document loading over the netfetcher engine (the `netfetch` feature), and smolweb
//! (gemini/gopher/nex/finger/spartan/guppy) over the errand transport (the `smolweb`
//! feature). A host composes it under `genet_documents::LocalFetcher::with_fallback`.
//!
//! pelt is genet's reference *host*, so -- like meerkat in the product -- it owns
//! networking and drives the sibling engines ([`netfetcher`] for the web, errand for
//! smolweb); genet's engine components stay byte-consuming and never link them.
//! `ResourceFetcher::fetch` is synchronous, so the engines' async `fetch` is bridged
//! onto it through a small tokio runtime, block-on per request -- the document load is
//! a one-shot at open time, not a per-frame cost. The same wiring genet-wpt's
//! `fetch()` uses.

use std::sync::OnceLock;
#[cfg(feature = "netfetch")]
use std::sync::{Arc, Mutex};

#[cfg(feature = "netfetch")]
use bytes::BytesMut;
#[cfg(any(feature = "netfetch", feature = "smolweb"))]
use tokio::runtime::Runtime;

use genet_host_api::{ResourceFetchPolicy, ResourceFetcher, ResourceResponse};
#[cfg(feature = "netfetch")]
use script_runtime_api::{FetchEvent, FetchHandler, FetchOutcome, FetchRequest};

#[cfg(any(feature = "netfetch", feature = "smolweb"))]
/// The shared tokio runtime the blocking bridge drives. Built once on first use: a
/// multithread runtime lets the host policy admit several independent document
/// sessions without creating a private runtime per resource. `enable_all`
/// lights the IO + time drivers netfetcher's transport needs.
fn runtime() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("pelt netfetch tokio runtime")
    })
}

/// One shared remote-fetch host. Its context owns HTTP cache revalidation and
/// redirect policy; its permit pool bounds all simultaneous resource fetches.
/// It intentionally lives at the port boundary, not in a style engine.
#[cfg(feature = "netfetch")]
pub(crate) struct HttpResourceHost {
    context: netfetcher::FetchContext,
    policy: ResourceFetchPolicy,
    permits: Arc<tokio::sync::Semaphore>,
}

#[cfg(feature = "netfetch")]
impl HttpResourceHost {
    pub(crate) fn new(policy: ResourceFetchPolicy) -> Self {
        let mut context =
            netfetcher::FetchContext::permissive().with_redirect_limit(policy.max_redirects);
        context.cache = Arc::new(netfetcher::InMemoryHttpCache::new());
        Self {
            context,
            policy,
            permits: Arc::new(tokio::sync::Semaphore::new(
                policy.max_concurrent_fetches.max(1),
            )),
        }
    }

    fn acquire(&self) -> tokio::sync::OwnedSemaphorePermit {
        runtime()
            .block_on(Arc::clone(&self.permits).acquire_owned())
            .expect("HTTP fetch permit semaphore")
    }

    pub(crate) fn fetch_response(&self, url: &str) -> Option<ResourceResponse> {
        let _permit = self.acquire();
        let parsed = url::Url::parse(url).ok()?;
        let policy = self.policy;
        runtime().block_on(async move {
            tokio::time::timeout(policy.timeout, async move {
                let request = netfetcher::Request::get(parsed);
                let response = netfetcher::fetch(request, &self.context).await;
                if response.is_network_error() || response.status < 200 || response.status >= 300 {
                    return None;
                }
                let final_url = response
                    .url_list
                    .last()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| url.to_owned());
                let content_type = response
                    .headers
                    .iter()
                    .rev()
                    .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
                    .map(|(_, value)| value.clone());
                let mut body = response.body;
                let mut bytes = BytesMut::new();
                while let Some(chunk) = body.next_chunk().await {
                    let chunk = chunk.ok()?;
                    if bytes.len().saturating_add(chunk.len()) > policy.max_response_bytes {
                        return None;
                    }
                    bytes.extend_from_slice(&chunk);
                }
                Some(ResourceResponse {
                    final_url,
                    content_type,
                    bytes: bytes.to_vec(),
                })
            })
            .await
            .ok()
            .flatten()
        })
    }
}

/// The remote half of a host's resource fetcher: http(s) over netfetcher
/// (feature `netfetch`) and the smolweb schemes over errand (feature
/// `smolweb`). Every other scheme is `None`; a host composes it under
/// `genet_documents::LocalFetcher::with_fallback`. Clones share one HTTP
/// cache, redirect cap and concurrency budget.
#[derive(Clone)]
pub struct RemoteFetcher {
    #[cfg(feature = "netfetch")]
    http: Arc<HttpResourceHost>,
}

impl RemoteFetcher {
    /// A fetcher with its own HTTP cache, redirect cap and concurrency budget.
    pub fn new(policy: ResourceFetchPolicy) -> Self {
        #[cfg(not(feature = "netfetch"))]
        let _ = policy;
        Self {
            #[cfg(feature = "netfetch")]
            http: Arc::new(HttpResourceHost::new(policy)),
        }
    }

    /// One process-shared fetcher under the default policy, so document loads
    /// and every shared-resource resolver pass reuse the same cache, redirect
    /// cap and concurrency budget.
    pub fn shared() -> Self {
        static SHARED: OnceLock<RemoteFetcher> = OnceLock::new();
        SHARED
            .get_or_init(|| Self::new(ResourceFetchPolicy::default()))
            .clone()
    }

    /// Create a per-document script handler sharing this fetcher's HTTP cache,
    /// cookies, redirect policy, and concurrency budget.
    #[cfg(feature = "netfetch")]
    pub fn script_handler(&self, document_url: &str) -> ScriptFetchHandler {
        ScriptFetchHandler::from_http(document_url, Arc::clone(&self.http))
    }
}

impl ResourceFetcher for RemoteFetcher {
    fn fetch(&self, url: &str) -> Option<Vec<u8>> {
        self.fetch_response(url).map(|response| response.bytes)
    }

    fn fetch_response(&self, url: &str) -> Option<ResourceResponse> {
        #[cfg(feature = "netfetch")]
        if url.starts_with("http://") || url.starts_with("https://") {
            return self.http.fetch_response(url);
        }
        #[cfg(feature = "smolweb")]
        if url
            .split_once("://")
            .and_then(|(scheme, _)| errand::Scheme::parse(scheme))
            .is_some()
        {
            return smolweb_get_bytes(url).map(|bytes| ResourceResponse::new(url, bytes));
        }
        let _ = url;
        None
    }
}

/// Per-document, deferred `window.fetch()` adapter over Mere's shared
/// netfetcher host. Network work runs on the host runtime; the script thread
/// drains completions through [`FetchHandler::poll`].
#[cfg(feature = "netfetch")]
pub struct ScriptFetchHandler {
    http: Arc<HttpResourceHost>,
    origin: Option<url::Origin>,
    tx: std::sync::mpsc::Sender<FetchEvent>,
    rx: Mutex<std::sync::mpsc::Receiver<FetchEvent>>,
    active: Mutex<std::collections::HashMap<u64, tokio::task::AbortHandle>>,
}

#[cfg(feature = "netfetch")]
impl ScriptFetchHandler {
    /// Create one request namespace for one scripted document.
    pub fn new(document_url: &str, policy: ResourceFetchPolicy) -> Self {
        Self::from_http(document_url, Arc::new(HttpResourceHost::new(policy)))
    }

    fn from_http(document_url: &str, http: Arc<HttpResourceHost>) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        Self {
            http,
            origin: url::Url::parse(document_url).ok().map(|url| url.origin()),
            tx,
            rx: Mutex::new(rx),
            active: Mutex::new(std::collections::HashMap::new()),
        }
    }

    fn abort_all(&self) {
        let mut active = self.active.lock().expect("script fetch active lock");
        for (_, handle) in active.drain() {
            handle.abort();
        }
        let rx = self.rx.lock().expect("script fetch completion lock");
        while rx.try_recv().is_ok() {}
    }
}

#[cfg(feature = "netfetch")]
impl FetchHandler for ScriptFetchHandler {
    fn start(&self, id: u64, request: FetchRequest) -> Option<FetchOutcome> {
        let host = Arc::clone(&self.http);
        let origin = self.origin.clone();
        let tx = self.tx.clone();
        let task = runtime().spawn(async move {
            let event = match tokio::time::timeout(
                host.policy.timeout,
                script_fetch(request, origin, &host),
            )
            .await
            {
                Ok(Ok(outcome)) => FetchEvent::Complete { id, outcome },
                Ok(Err(message)) => FetchEvent::Failed { id, message },
                Err(_) => FetchEvent::Failed {
                    id,
                    message: "Fetch timed out".to_owned(),
                },
            };
            let _ = tx.send(event);
        });
        self.active
            .lock()
            .expect("script fetch active lock")
            .insert(id, task.abort_handle());
        None
    }

    fn cancel(&self, id: u64) {
        if let Some(handle) = self
            .active
            .lock()
            .expect("script fetch active lock")
            .remove(&id)
        {
            handle.abort();
        }
    }

    fn poll(&self, max_events: usize) -> Vec<FetchEvent> {
        let rx = self.rx.lock().expect("script fetch completion lock");
        let mut events = Vec::new();
        for _ in 0..max_events {
            let Ok(event) = rx.try_recv() else { break };
            let id = match &event {
                FetchEvent::Complete { id, .. } | FetchEvent::Failed { id, .. } => *id,
            };
            if self
                .active
                .lock()
                .expect("script fetch active lock")
                .remove(&id)
                .is_some()
            {
                events.push(event);
            }
        }
        events
    }

    fn has_pending(&self) -> bool {
        !self
            .active
            .lock()
            .expect("script fetch active lock")
            .is_empty()
    }

    fn cancel_all(&self) {
        self.abort_all();
    }
}

#[cfg(feature = "netfetch")]
impl Drop for ScriptFetchHandler {
    fn drop(&mut self) {
        self.abort_all();
    }
}

#[cfg(feature = "netfetch")]
async fn script_fetch(
    req: FetchRequest,
    origin: Option<url::Origin>,
    host: &HttpResourceHost,
) -> Result<FetchOutcome, String> {
    let _permit = Arc::clone(&host.permits)
        .acquire_owned()
        .await
        .map_err(|_| "Fetch host stopped".to_owned())?;
    let url = url::Url::parse(&req.url).map_err(|_| "Failed to fetch".to_owned())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Unsupported fetch scheme".to_owned());
    }
    let mut request = netfetcher::Request::get(url);
    request.method = match req.method.as_str() {
        "GET" => netfetcher::Method::Get,
        "HEAD" => netfetcher::Method::Head,
        "POST" => netfetcher::Method::Post,
        "PUT" => netfetcher::Method::Put,
        "DELETE" => netfetcher::Method::Delete,
        "PATCH" => netfetcher::Method::Patch,
        "OPTIONS" => netfetcher::Method::Options,
        method => netfetcher::Method::Other(method.to_owned()),
    };
    request.headers = req.headers;
    request.body = req.body.map(bytes::Bytes::from);
    request.cache = match req.cache.as_str() {
        "no-store" => netfetcher::CacheMode::NoStore,
        "reload" => netfetcher::CacheMode::Reload,
        "no-cache" => netfetcher::CacheMode::NoCache,
        "force-cache" => netfetcher::CacheMode::ForceCache,
        "only-if-cached" => netfetcher::CacheMode::OnlyIfCached,
        _ => netfetcher::CacheMode::Default,
    };
    request.redirect = match req.redirect.as_str() {
        "error" => netfetcher::RedirectMode::Error,
        "manual" => netfetcher::RedirectMode::Manual,
        _ => netfetcher::RedirectMode::Follow,
    };
    request.mode = match req.mode.as_str() {
        "no-cors" => netfetcher::RequestMode::NoCors,
        "same-origin" => netfetcher::RequestMode::SameOrigin,
        "navigate" => netfetcher::RequestMode::Navigate,
        _ => netfetcher::RequestMode::Cors,
    };
    request.referrer = (!req.referrer.is_empty())
        .then(|| url::Url::parse(&req.referrer).ok())
        .flatten();
    request.origin = origin;
    request.referrer_policy = match req.referrer_policy.as_str() {
        "no-referrer" => netfetcher::ReferrerPolicy::NoReferrer,
        "no-referrer-when-downgrade" => netfetcher::ReferrerPolicy::NoReferrerWhenDowngrade,
        "same-origin" => netfetcher::ReferrerPolicy::SameOrigin,
        "origin" => netfetcher::ReferrerPolicy::Origin,
        "strict-origin" => netfetcher::ReferrerPolicy::StrictOrigin,
        "origin-when-cross-origin" => netfetcher::ReferrerPolicy::OriginWhenCrossOrigin,
        "strict-origin-when-cross-origin" => {
            netfetcher::ReferrerPolicy::StrictOriginWhenCrossOrigin
        },
        "unsafe-url" => netfetcher::ReferrerPolicy::UnsafeUrl,
        _ => netfetcher::ReferrerPolicy::Empty,
    };
    request.credentials = match req.credentials.as_str() {
        "omit" => netfetcher::Credentials::Omit,
        "include" => netfetcher::Credentials::Include,
        _ => netfetcher::Credentials::SameOrigin,
    };
    request.integrity = req.integrity;

    let mut response = netfetcher::fetch(request, &host.context).await;
    if response.is_network_error() {
        return Err("Failed to fetch".to_owned());
    }
    let mut body = BytesMut::new();
    while let Some(chunk) = response.body.next_chunk().await {
        let chunk = chunk.map_err(|_| "Failed to read response body".to_owned())?;
        if body.len().saturating_add(chunk.len()) > host.policy.max_response_bytes {
            return Err("Fetch response exceeded host limit".to_owned());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(FetchOutcome {
        network_error: false,
        status: response.status,
        status_text: status_text(response.status).to_owned(),
        response_type: match response.response_type {
            netfetcher::ResponseType::Basic => "basic",
            netfetcher::ResponseType::Cors => "cors",
            netfetcher::ResponseType::Opaque => "opaque",
            netfetcher::ResponseType::OpaqueRedirect => "opaqueredirect",
            netfetcher::ResponseType::Error => "error",
        }
        .to_owned(),
        url: response
            .url_list
            .last()
            .map(ToString::to_string)
            .unwrap_or_default(),
        redirected: response.url_list.len() > 1,
        headers: response.headers,
        body: body.to_vec(),
    })
}

#[cfg(feature = "netfetch")]
fn status_text(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        304 => "Not Modified",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        409 => "Conflict",
        410 => "Gone",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "",
    }
}

/// Blocking smolweb GET of `url` over the errand transport, returning the response
/// body on a success status, or `None` on a non-success status (input / redirect /
/// failure / cert-required) or a transport error. The smolweb branch of
/// [`RemoteFetcher`]; mirrors `http_get_bytes`,
/// bridging errand's async `fetch` onto the sync `ResourceFetcher` through the shared
/// runtime. The caller surfaces the `None` as a clean load error rather than painting
/// a protocol error line as a document, matching the http path's non-2xx handling.
#[cfg(feature = "smolweb")]
fn smolweb_get_bytes(url: &str) -> Option<Vec<u8>> {
    install_tofu();
    runtime().block_on(async move {
        match errand::fetch(url).await {
            Ok(resp) if resp.status == errand::Status::Success => Some(resp.body),
            _ => None,
        }
    })
}

/// Install an [`errand::InMemoryTofu`] once for the process, so gemini certificate
/// pins persist across requests in a session: a first contact is trusted-on-first-use
/// and a later mismatch (a possible MITM or a key rotation) surfaces as a failed load
/// rather than a silent re-pin. Without this errand defaults to accept-any
/// (`PermissiveTofu`); the reference shell opts into real pinning. A durable on-disk
/// store is a later rung.
#[cfg(feature = "smolweb")]
fn install_tofu() {
    use std::sync::OnceLock;
    static INSTALLED: OnceLock<()> = OnceLock::new();
    INSTALLED.get_or_init(|| {
        errand::set_trust_store(std::sync::Arc::new(errand::InMemoryTofu::new()));
    });
}

#[cfg(all(test, feature = "netfetch"))]
mod tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use genet_host_api::{ResourceFetchPolicy, ResourceFetcher};
    use script_runtime_api::{FetchEvent, FetchHandler, FetchRequest};

    use super::HttpResourceHost;
    use crate::{RemoteFetcher, ScriptFetchHandler};

    struct GateTransport {
        started: std::sync::mpsc::Sender<()>,
        release: Arc<tokio::sync::Notify>,
    }

    impl netfetcher::Transport for GateTransport {
        fn send(&self, _request: netfetcher::WireRequest) -> netfetcher::TransportFuture<'_> {
            let started = self.started.clone();
            let release = Arc::clone(&self.release);
            Box::pin(async move {
                let _ = started.send(());
                release.notified().await;
                Some(netfetcher::RawResponse::once(
                    200,
                    Vec::new(),
                    bytes::Bytes::from_static(b"ok"),
                ))
            })
        }
    }

    fn request(url: String) -> FetchRequest {
        FetchRequest {
            method: "GET".to_owned(),
            url,
            headers: Vec::new(),
            body: None,
            cache: "default".to_owned(),
            redirect: "follow".to_owned(),
            mode: "cors".to_owned(),
            referrer: String::new(),
            referrer_policy: String::new(),
            credentials: "same-origin".to_owned(),
            integrity: String::new(),
        }
    }

    fn await_event(handler: &ScriptFetchHandler) -> FetchEvent {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(event) = handler.poll(1).pop() {
                return event;
            }
            assert!(Instant::now() < deadline, "deferred fetch did not complete");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn scripted_fetch_completes_off_thread_with_status_headers_and_body() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/script-data")
            .with_status(201)
            .with_header("content-type", "text/plain")
            .with_body("cedar")
            .create();
        let document_url = format!("{}/page", server.url());
        let handler = ScriptFetchHandler::new(&document_url, ResourceFetchPolicy::default());
        assert!(
            handler
                .start(7, request(format!("{}/script-data", server.url())))
                .is_none()
        );
        assert!(handler.has_pending());
        match await_event(&handler) {
            FetchEvent::Complete { id, outcome } => {
                assert_eq!(id, 7);
                assert_eq!(outcome.status, 201);
                assert_eq!(outcome.status_text, "Created");
                assert_eq!(outcome.body, b"cedar");
                assert!(
                    outcome
                        .headers
                        .iter()
                        .any(|(name, value)| name.eq_ignore_ascii_case("content-type")
                            && value == "text/plain")
                );
            },
            FetchEvent::Failed { message, .. } => panic!("fetch failed: {message}"),
        }
        assert!(!handler.has_pending());
        mock.assert();
    }

    #[test]
    fn scripted_fetch_keeps_document_origin_when_referrer_is_suppressed() {
        let document_server = mockito::Server::new();
        let mut foreign_server = mockito::Server::new();
        let foreign = foreign_server
            .mock("GET", "/private")
            .with_status(200)
            .with_body("must not be exposed")
            .create();
        let handler = ScriptFetchHandler::new(
            &format!("{}/page", document_server.url()),
            ResourceFetchPolicy::default(),
        );
        let mut req = request(format!("{}/private", foreign_server.url()));
        req.referrer_policy = "no-referrer".to_owned();
        assert!(req.referrer.is_empty());
        assert!(handler.start(11, req).is_none());
        assert!(matches!(
            await_event(&handler),
            FetchEvent::Failed { id: 11, .. }
        ));
        foreign.assert();
    }

    #[test]
    fn redirected_document_final_url_becomes_the_script_origin() {
        let mut initial_server = mockito::Server::new();
        let mut final_server = mockito::Server::new();
        let final_page_url = format!("{}/page", final_server.url());
        let redirect = initial_server
            .mock("GET", "/start")
            .with_status(302)
            .with_header("location", &final_page_url)
            .create();
        let page = final_server
            .mock("GET", "/page")
            .with_status(200)
            .with_body("<script>fetch('/data')</script>")
            .create();
        let data = final_server
            .mock("GET", "/data")
            .with_status(200)
            .with_body("same origin after redirect")
            .create();
        let remote = RemoteFetcher::new(ResourceFetchPolicy::default());
        let response = remote
            .fetch_response(&format!("{}/start", initial_server.url()))
            .expect("redirected top-level document");
        assert_eq!(response.final_url, final_page_url);

        let handler = remote.script_handler(&response.final_url);
        assert!(
            handler
                .start(12, request(format!("{}/data", final_server.url())))
                .is_none()
        );
        assert!(matches!(
            await_event(&handler),
            FetchEvent::Complete { id: 12, .. }
        ));
        redirect.assert();
        page.assert();
        data.assert();
    }

    #[test]
    fn resource_and_script_fetches_share_one_concurrency_budget() {
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let release = Arc::new(tokio::sync::Notify::new());
        let policy = ResourceFetchPolicy {
            max_concurrent_fetches: 1,
            ..ResourceFetchPolicy::default()
        };
        let mut context = netfetcher::FetchContext::permissive()
            .with_redirect_limit(policy.max_redirects)
            .with_transport(Arc::new(GateTransport {
                started: started_tx,
                release: Arc::clone(&release),
            }));
        context.cache = Arc::new(netfetcher::InMemoryHttpCache::new());
        let http = Arc::new(HttpResourceHost {
            context,
            policy,
            permits: Arc::new(tokio::sync::Semaphore::new(1)),
        });
        let resource_fetcher = RemoteFetcher {
            http: Arc::clone(&http),
        };
        let script_handler = ScriptFetchHandler::from_http("http://same.invalid/page", http);

        let resource_thread =
            std::thread::spawn(move || resource_fetcher.fetch("http://same.invalid/resource"));
        started_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("resource fetch entered transport");
        assert!(
            script_handler
                .start(21, request("http://same.invalid/script".to_owned()))
                .is_none()
        );
        assert!(
            started_rx.recv_timeout(Duration::from_millis(30)).is_err(),
            "script fetch must wait behind the resource permit"
        );

        release.notify_one();
        assert_eq!(
            resource_thread.join().expect("resource thread"),
            Some(b"ok".to_vec())
        );
        started_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("script fetch entered after resource released permit");
        release.notify_one();
        assert!(matches!(
            await_event(&script_handler),
            FetchEvent::Complete { id: 21, .. }
        ));
    }

    #[test]
    fn scripted_fetch_reports_errors_and_cancellation_without_stale_delivery() {
        let handler =
            ScriptFetchHandler::new("https://document.invalid/", ResourceFetchPolicy::default());
        assert!(
            handler
                .start(1, request("gemini://example.invalid/".to_owned()))
                .is_none()
        );
        assert!(matches!(
            await_event(&handler),
            FetchEvent::Failed { id: 1, .. }
        ));

        assert!(
            handler
                .start(2, request("http://192.0.2.1/slow".to_owned()))
                .is_none()
        );
        handler.cancel(2);
        assert!(!handler.has_pending());
        std::thread::sleep(Duration::from_millis(20));
        assert!(
            handler.poll(8).is_empty(),
            "cancelled completion was discarded"
        );

        assert!(
            handler
                .start(3, request("http://192.0.2.1/later".to_owned()))
                .is_none()
        );
        handler.cancel_all();
        assert!(!handler.has_pending());
        assert!(handler.poll(8).is_empty());
    }

    /// http(s) loading flows through the netfetcher engine end to end: an offline
    /// mock server serves a body, and `RemoteFetcher` (with the `netfetch` branch)
    /// fetches its bytes -- the same path `pelt --engine static https://…` takes,
    /// proven without a live network.
    #[test]
    fn local_fetcher_gets_http_bytes_via_netfetcher() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/page.html")
            .with_status(200)
            .with_header("content-type", "text/html; charset=utf-8")
            .with_body("<h1>From the network</h1>")
            .create();

        let url = format!("{}/page.html", server.url());
        let bytes = RemoteFetcher::shared()
            .fetch(&url)
            .expect("the http(s) document fetches over netfetcher");
        assert_eq!(
            bytes, b"<h1>From the network</h1>",
            "the fetched bytes are the served body"
        );
        mock.assert();
    }

    /// A non-2xx response is `None` (the caller surfaces a load error), not the error
    /// body painted as a document.
    #[test]
    fn http_not_found_is_none() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/missing")
            .with_status(404)
            .with_body("nope")
            .create();

        let url = format!("{}/missing", server.url());
        assert!(
            RemoteFetcher::shared().fetch(&url).is_none(),
            "a 404 is a failed load, not a document"
        );
        mock.assert();
    }

    #[test]
    fn configured_fetcher_revalidates_a_shared_cached_response() {
        let mut server = mockito::Server::new();
        let initial = server
            .mock("GET", "/revision.css")
            .match_header("if-none-match", mockito::Matcher::Missing)
            .with_status(200)
            .with_header("cache-control", "max-age=0")
            .with_header("etag", "\"v1\"")
            .with_body("body { color: red; }")
            .expect(1)
            .create();
        let revalidated = server
            .mock("GET", "/revision.css")
            .match_header("if-none-match", "\"v1\"")
            .with_status(304)
            .expect(1)
            .create();
        let fetcher = RemoteFetcher::new(ResourceFetchPolicy::default());
        let url = format!("{}/revision.css", server.url());
        let first = fetcher.fetch_response(&url).expect("initial response");
        let second = fetcher.fetch_response(&url).expect("revalidated response");
        assert_eq!(first.bytes, second.bytes);
        initial.assert();
        revalidated.assert();
    }

    #[test]
    fn configured_fetcher_enforces_redirect_and_body_limits() {
        let mut server = mockito::Server::new();
        let redirect = server
            .mock("GET", "/redirect")
            .with_status(302)
            .with_header("location", "/target")
            .expect(1)
            .create();
        let oversized = server
            .mock("GET", "/oversized")
            .with_status(200)
            .with_body("four")
            .expect(1)
            .create();
        let fetcher = RemoteFetcher::new(ResourceFetchPolicy {
            max_redirects: 0,
            max_response_bytes: 3,
            ..ResourceFetchPolicy::default()
        });
        assert!(
            fetcher
                .fetch(&format!("{}/redirect", server.url()))
                .is_none()
        );
        assert!(
            fetcher
                .fetch(&format!("{}/oversized", server.url()))
                .is_none()
        );
        redirect.assert();
        oversized.assert();
    }
}

#[cfg(all(test, feature = "smolweb"))]
mod smolweb_tests {
    use genet_host_api::ResourceFetcher;

    use crate::RemoteFetcher;

    /// A smolweb scheme is recognized and routed to the errand transport, and a host
    /// that cannot resolve fails to a clean `None` (a failed load, not a panic or an
    /// error document) -- the same contract the http path holds for a non-2xx. Uses a
    /// `.invalid` host (RFC 6761 guarantees NXDOMAIN, answered locally) so the test
    /// needs no live capsule, and exercises the one-time TOFU install on the way.
    #[test]
    fn smolweb_scheme_routes_and_unresolvable_host_is_none() {
        assert!(
            RemoteFetcher::shared()
                .fetch("gemini://capsule.invalid/")
                .is_none(),
            "an unresolvable gemini host is a failed load, not a document"
        );
    }

    /// A non-smolweb, non-http unknown scheme is not routed to errand; it falls
    /// through to the filesystem attempt and fails to `None`.
    #[test]
    fn unknown_scheme_is_not_routed_to_errand() {
        assert!(
            RemoteFetcher::shared().fetch("wat://nope/").is_none(),
            "a non-smolweb scheme is not an errand fetch nor a readable path"
        );
    }
}
