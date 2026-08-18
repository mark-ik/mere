// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Off-UI-thread fetching (S2.2b).
//!
//! netfetcher's WHATWG Fetch is async (tokio/hyper), so it runs on a worker
//! runtime, never the UI thread. Delivery is **model 2** (channel + bare wake):
//! a completed fetch pushes a [`FetchOutcome`] over an `mpsc` channel and wakes
//! the winit loop via an `EventLoopProxy<()>`; the host drains the channel in
//! `user_event` and updates its per-URL content cache. The winit user-event type
//! stays trivial, so persistence (S3) and sync (S5) can push their own typed
//! channels through the same wake without fighting over one event enum.
//!
//! S2.2b-i carries the decoded body as text and renders it plainly; content-type
//! routing to the nematic + genet engines is S2.2b-ii.
//!
//! Smolweb: the same actor also speaks the small web. A page request is routed by
//! scheme, http(s) runs through netfetcher (WHATWG Fetch) and gemini / gopher /
//! finger / spartan / nex / guppy / titan run through the [`errand`] transport
//! crate. Either way the result is the same [`Fetched`] (decoded body +
//! content-type), so the render side (nematic engines) is unchanged.
//! Subresources stay http-only: smolweb documents reference links but do not
//! inline fetched media.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use armillary::{ActorHandle, Emitter, Wake, spawn};
use eidetic::Store;
use netfetcher::{CookieRecord, CookieStore, InMemoryCookieJar, SameSite, SameSiteContext};
use pandect::PersonaId;
use serde::{Deserialize, Serialize};
use tokio::runtime::Builder;
use zeroize::Zeroizing;

/// Host-supplied durable trust storage for Gemini-style TLS.
pub use errand::TofuStore as SmolwebTofuStore;

/// The most redirects a smolweb fetch will follow before giving up.
const MAX_REDIRECTS: usize = 5;

/// Per-hop timeout for smolweb fetches. Applied to each request in a redirect
/// chain independently so a chain of N slow hops can take up to N × this value.
const SMOLWEB_TIMEOUT: Duration = Duration::from_secs(30);

/// Host-side cap on a fetched page body (§A5): a hard ceiling enforced *while*
/// streaming the body, so a malicious or runaway server cannot OOM the host by
/// sending an unbounded response. Generous for a real page; a script `net.fetch`
/// passes a tighter cap (the body is copied into the guest's mem-quota'd memory).
const PAGE_BODY_CAP: usize = 64 * 1024 * 1024;

/// The subresource counterpart (a page's images / CSS): generous but bounded.
const SUBRESOURCE_BODY_CAP: usize = 32 * 1024 * 1024;

/// Successfully fetched content: the response content-type (if any) and the
/// decoded body as text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fetched {
    pub content_type: Option<String>,
    pub body: String,
}

/// A page request that needs host participation rather than being reducible
/// to a terminal error string. The fetch actor preserves these arms so a UI
/// host can continue the protocol conversation without parsing prose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchFailure {
    /// A Gemini-style input response. `url` is the final request address after
    /// redirects and is therefore the address the submitted query belongs to.
    InputRequired {
        url: String,
        prompt: String,
        sensitive: bool,
    },
    /// The server requires a client certificate. Identity selection remains
    /// a host decision; carrying the target keeps that later conversation
    /// typed instead of collapsing it into an ordinary transport failure.
    ClientCertificateRequired {
        url: String,
        prompt: String,
        code: Option<u8>,
    },
    /// A Gemini capsule presented a certificate that differs from its durable
    /// pin. The request was not sent; the host must ask a human before replacing
    /// `pinned` with `seen` and retrying `url`.
    CertificateChanged {
        url: String,
        target: String,
        pinned: String,
        seen: String,
    },
    /// A terminal transport, protocol, HTTP, or size-limit failure.
    Failed(String),
}

impl std::fmt::Display for FetchFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InputRequired { prompt, .. } => write!(f, "input required: {prompt}"),
            Self::ClientCertificateRequired { .. } => f.write_str("client certificate required"),
            Self::CertificateChanged { target, .. } => {
                write!(f, "certificate for {target} changed")
            }
            Self::Failed(error) => f.write_str(error),
        }
    }
}

/// The result of one fetch, tagged with the requested URL so the host routes it
/// back to the right node's content slot.
pub struct FetchOutcome {
    pub url: String,
    pub result: Result<Fetched, FetchFailure>,
}

/// A fetched subresource: raw bytes for an absolute URL (page CSS via
/// `<link>`, an `<img>`, ...). Carried as bytes, not text, since media is
/// binary. Only successful fetches are delivered; a failure simply never
/// arrives (the demand loader keeps treating the resource as absent).
pub struct SubresourceOutcome {
    pub url: String,
    pub bytes: Vec<u8>,
}

/// One Gemini client certificate, assigned to exactly one capsule origin.
///
/// The host mints the material from its identity layer. The actor enforces the
/// host+effective-port scope on every redirect, so a certificate selected for
/// one capsule is never presented to another. Private bytes are shared rather
/// than copied between effects and commands, and zeroized when the last owner
/// drops.
#[derive(Clone)]
pub struct GeminiClientIdentity {
    host: String,
    port: u16,
    certificate_der: Arc<[u8]>,
    private_key_pkcs8_der: Arc<Zeroizing<Vec<u8>>>,
}

impl std::fmt::Debug for GeminiClientIdentity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GeminiClientIdentity")
            .field("origin", &self.origin())
            .field("certificate", &"[redacted]")
            .field("private_key", &"[redacted]")
            .finish()
    }
}

impl PartialEq for GeminiClientIdentity {
    fn eq(&self, other: &Self) -> bool {
        self.host == other.host
            && self.port == other.port
            && self.certificate_der.as_ref() == other.certificate_der.as_ref()
            && self.private_key_pkcs8_der.as_slice() == other.private_key_pkcs8_der.as_slice()
    }
}

impl Eq for GeminiClientIdentity {}

impl GeminiClientIdentity {
    pub fn new(
        capsule_url: &str,
        certificate_der: Vec<u8>,
        private_key_pkcs8_der: Vec<u8>,
    ) -> Result<Self, String> {
        let url = url::Url::parse(capsule_url).map_err(|error| error.to_string())?;
        if url.scheme() != "gemini" {
            return Err("Gemini client identity requires a gemini:// capsule".to_string());
        }
        let host = url
            .host_str()
            .ok_or_else(|| "Gemini capsule has no host".to_string())?
            .to_ascii_lowercase();
        if certificate_der.is_empty() || private_key_pkcs8_der.is_empty() {
            return Err("Gemini client identity material is empty".to_string());
        }
        Ok(Self {
            host,
            port: url.port().unwrap_or(1965),
            certificate_der: Arc::from(certificate_der),
            private_key_pkcs8_der: Arc::new(Zeroizing::new(private_key_pkcs8_der)),
        })
    }

    pub fn origin(&self) -> String {
        let host = if self.host.contains(':') && !self.host.starts_with('[') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        if self.port == 1965 {
            format!("gemini://{host}")
        } else {
            format!("gemini://{host}:{}", self.port)
        }
    }

    pub fn certificate_der(&self) -> &[u8] {
        self.certificate_der.as_ref()
    }

    fn applies_to(&self, url: &url::Url) -> bool {
        url.scheme() == "gemini"
            && url
                .host_str()
                .is_some_and(|host| host.eq_ignore_ascii_case(&self.host))
            && url.port().unwrap_or(1965) == self.port
    }

    fn errand_view(&self) -> errand::GeminiClientIdentity<'_> {
        errand::GeminiClientIdentity {
            certificate_der: self.certificate_der.as_ref(),
            private_key_pkcs8_der: self.private_key_pkcs8_der.as_slice(),
        }
    }
}

/// Per-URL content state behind the focused-node card.
#[derive(Clone, Debug)]
pub enum ContentState {
    /// A fetch is in flight.
    Loading,
    /// Fetched content, ready to render.
    Ready(Fetched),
    /// The fetch failed; the reason renders on the card.
    Failed(String),
}

impl ContentState {
    /// A small tag distinguishing the states, folded into the card's cache key so
    /// a `Loading → Ready/Failed` transition re-renders the card (terminal states
    /// don't change again, so the tag alone suffices — no body hash needed).
    pub fn tag(state: Option<&ContentState>) -> u8 {
        match state {
            None => 0,
            Some(ContentState::Loading) => 1,
            Some(ContentState::Ready(_)) => 2,
            Some(ContentState::Failed(_)) => 3,
        }
    }
}

/// Whether `url` is a network address meerkat fetches (http(s) or a smolweb
/// scheme), vs a synthesized `mere://` page or another non-network scheme.
pub fn is_fetchable(url: &str) -> bool {
    match scheme_of(url) {
        Some(scheme) => {
            scheme == "http" || scheme == "https" || errand::Scheme::parse(scheme).is_some()
        }
        None => false,
    }
}

/// The scheme of a `scheme://…` URL, if it has an authority component. Returns
/// `None` for schemeless or `scheme:opaque` forms (e.g. `about:blank`), which are
/// never network-fetched.
fn scheme_of(url: &str) -> Option<&str> {
    url.split_once("://").map(|(scheme, _)| scheme)
}

/// A command to the fetch actor.
pub enum FetchCommand {
    /// Fetch `url` as a page document (decoded body as text).
    Page {
        url: String,
        identity: Option<GeminiClientIdentity>,
    },
    /// Fetch the subresource at the (already absolute) `url` as raw bytes.
    Subresource(String),
    /// Fetch the favicon at `url` (already absolute) as raw bytes, remembering it
    /// belongs to the node currently at `owner_url` so the host applies the decoded
    /// icon to that node. Carried separately from `Subresource` so favicon bytes
    /// reach the graph, not the content actors' render stores. (Favicon-on-tile.)
    Favicon { owner_url: String, url: String },
}

/// An update from the fetch actor: one completed page, subresource, or favicon fetch.
pub enum FetchUpdate {
    Page(FetchOutcome),
    Subresource(SubresourceOutcome),
    /// Raw favicon bytes (only on success) plus the page they belong to; the host
    /// decodes them to RGBA and stamps them on that node. (Favicon-on-tile.)
    Favicon {
        owner_url: String,
        bytes: Vec<u8>,
    },
}

/// Spawn the fetch actor on its own thread (armillary harness). It owns a
/// multi-thread tokio runtime; each [`FetchCommand`] dispatches a concurrent async
/// fetch that emits a [`FetchUpdate`] on completion and wakes the loop. Returns the
/// kernel's command handle plus the receiver to drain in `user_event`. Dropping the
/// handle ends the actor (its runtime drops, aborting any in-flight fetches).
///
/// The internals stay `!Send` where they want to be: the runtime is built *on the
/// actor thread* inside the closure, never moved across the boundary; only the
/// `Send` handle and the `Send` `FetchUpdate`s cross.
pub fn spawn_fetcher(wake: Wake) -> (ActorHandle<FetchCommand>, Receiver<FetchUpdate>) {
    spawn(wake, |commands, out: Emitter<FetchUpdate>| {
        let runtime = Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("build the fetch runtime");
        while let Ok(command) = commands.recv() {
            match command {
                FetchCommand::Page { url, identity } => {
                    let out = out.clone();
                    runtime.spawn(async move {
                        let result =
                            fetch_page_interactive_capped(&url, PAGE_BODY_CAP, identity.as_ref())
                                .await;
                        out.emit(FetchUpdate::Page(FetchOutcome { url, result }));
                    });
                }
                FetchCommand::Subresource(url) => {
                    let out = out.clone();
                    runtime.spawn(async move {
                        // A failed / empty subresource fetch is dropped silently:
                        // the demand loader keeps the resource absent and the
                        // host's requested-set stops it being re-spawned.
                        if let Some(bytes) = fetch_bytes(&url).await {
                            out.emit(FetchUpdate::Subresource(SubresourceOutcome { url, bytes }));
                        }
                    });
                }
                FetchCommand::Favicon { owner_url, url } => {
                    let out = out.clone();
                    runtime.spawn(async move {
                        // Best-effort: a missing / undecodable favicon simply never
                        // arrives, and the node keeps its colored tile.
                        if let Some(bytes) = fetch_bytes(&url).await {
                            out.emit(FetchUpdate::Favicon { owner_url, bytes });
                        }
                    });
                }
            }
        }
    })
}

/// Fetch a page at the default page body cap ([`PAGE_BODY_CAP`]). The page-load and
/// crawl paths use this; the script `net.fetch` path calls [`fetch_page_capped`] with
/// a tighter ceiling. `pub(crate)` so the content actor reuses the routing.
pub async fn fetch_page(url: &str) -> Result<Fetched, String> {
    fetch_page_capped(url, PAGE_BODY_CAP).await
}

/// Fetch a page, routing by scheme: smolweb schemes go through [`errand`], every
/// other (http(s)) address through netfetcher's WHATWG Fetch. `max_bytes` caps the
/// body (§A5): the http path enforces it *while streaming* (no OOM); smolweb is
/// already buffered by errand, so it is checked post-hoc (errand bounds its own read).
pub async fn fetch_page_capped(url: &str, max_bytes: usize) -> Result<Fetched, String> {
    fetch_page_interactive_capped(url, max_bytes, None)
        .await
        .map_err(|error| error.to_string())
}

/// Fetch one page while preserving protocol responses that require host
/// participation. This is actor-facing: ordinary utility callers retain the
/// terminal `Result<Fetched, String>` contract above.
async fn fetch_page_interactive_capped(
    url: &str,
    max_bytes: usize,
    identity: Option<&GeminiClientIdentity>,
) -> Result<Fetched, FetchFailure> {
    match scheme_of(url).and_then(errand::Scheme::parse) {
        Some(scheme) => {
            let log_url = url_without_query(url);
            tracing::info!(url = %log_url, ?scheme, "smolweb fetch");
            let result = smolweb_fetch(url, identity).await.and_then(|fetched| {
                if fetched.body.len() > max_bytes {
                    Err(FetchFailure::Failed(format!(
                        "response exceeds the {max_bytes}-byte cap"
                    )))
                } else {
                    Ok(fetched)
                }
            });
            match &result {
                Ok(fetched) => tracing::info!(
                    url = %log_url,
                    content_type = ?fetched.content_type,
                    bytes = fetched.body.len(),
                    "smolweb ok",
                ),
                Err(error) => tracing::warn!(url = %log_url, %error, "smolweb failed"),
            }
            result
        }
        None => do_fetch(url, max_bytes).await.map_err(FetchFailure::Failed),
    }
}

fn url_without_query(raw: &str) -> String {
    url::Url::parse(raw)
        .map(|mut parsed| {
            parsed.set_query(None);
            parsed.set_fragment(None);
            parsed.to_string()
        })
        .unwrap_or_else(|_| "<invalid-url>".to_string())
}

/// Fetch a page without the browser session's cookie jar or other installed
/// authenticated HTTP state. Effect providers use this path so resolving a
/// transclusion cannot silently borrow the user's browsing authority.
///
/// Smolweb requests use errand's process-level trust store. A host that admits
/// Gemini should install one explicitly with
/// [`install_in_memory_smolweb_tofu`] or its own durable store.
pub async fn fetch_page_anonymous_capped(url: &str, max_bytes: usize) -> Result<Fetched, String> {
    match scheme_of(url).and_then(errand::Scheme::parse) {
        Some(_) => fetch_page_capped(url, max_bytes).await,
        None => {
            let cx = netfetcher::FetchContext::permissive();
            do_fetch_ua_with_context(url, max_bytes, None, &cx).await
        }
    }
}

/// Pin Gemini certificates for the lifetime of this process. This is stronger
/// than errand's permissive default, but it deliberately makes no restart
/// durability claim; a host with durable trust state should install its own
/// [`errand::TofuStore`] instead.
pub fn install_in_memory_smolweb_tofu() {
    install_smolweb_tofu(Arc::new(errand::InMemoryTofu::new()));
}

/// Install a host-owned Gemini trust store for every smolweb request in this
/// process. The host keeps the concrete store so certificate-change approval
/// can replace one pin before retrying the refused request.
pub fn install_smolweb_tofu(store: Arc<dyn SmolwebTofuStore>) {
    errand::set_trust_store(store);
}

/// Fetch a page as the **crawler**, identifying with [`CRAWLER_USER_AGENT`]. http(s)
/// sends the descriptive bot UA; smolweb routes through errand as usual (errand owns
/// its UA). The crawl actor fetches every page (and robots.txt) through this, so a
/// site sees an honest, contactable bot rather than a masquerading browser.
pub async fn fetch_page_crawler(url: &str) -> Result<Fetched, String> {
    match scheme_of(url).and_then(errand::Scheme::parse) {
        Some(_) => fetch_page_capped(url, PAGE_BODY_CAP).await,
        None => do_fetch_ua(url, PAGE_BODY_CAP, Some(CRAWLER_USER_AGENT)).await,
    }
}

/// Fetch a smolweb URL through [`errand`], following redirects up to
/// [`MAX_REDIRECTS`], and fold the response into a [`Fetched`] the nematic engines
/// render. Input and certificate statuses stay typed so the host can continue
/// the protocol conversation; terminal failures remain displayable prose.
async fn smolweb_fetch(
    url: &str,
    identity: Option<&GeminiClientIdentity>,
) -> Result<Fetched, FetchFailure> {
    let mut current =
        url::Url::parse(url).map_err(|error| FetchFailure::Failed(format!("bad URL: {error}")))?;
    for _ in 0..MAX_REDIRECTS {
        let response = match identity.filter(|identity| identity.applies_to(&current)) {
            Some(identity) => {
                errand::fetch_url_timeout_with_identity(
                    &current,
                    identity.errand_view(),
                    SMOLWEB_TIMEOUT,
                )
                .await
            }
            None => errand::fetch_url_timeout(&current, SMOLWEB_TIMEOUT).await,
        }
        .map_err(|error| smolweb_transport_failure(&current, error))?;
        match response.status {
            errand::Status::Success => {
                let content_type = smolweb_content_type(&current, &response);
                let body = String::from_utf8_lossy(&response.body).into_owned();
                return Ok(Fetched {
                    content_type: Some(content_type),
                    body,
                });
            }
            errand::Status::Redirect => {
                current = current.join(&response.meta).map_err(|error| {
                    FetchFailure::Failed(format!("bad redirect target: {error}"))
                })?;
            }
            errand::Status::Input => {
                return Err(smolweb_input_failure(&current, &response));
            }
            errand::Status::CertRequired => {
                return Err(FetchFailure::ClientCertificateRequired {
                    url: current.to_string(),
                    prompt: response.meta,
                    code: response.raw_status,
                });
            }
            errand::Status::Failure => {
                return Err(FetchFailure::Failed(if response.meta.is_empty() {
                    "request failed".to_string()
                } else {
                    response.meta
                }));
            }
        }
    }
    Err(FetchFailure::Failed("too many redirects".to_string()))
}

fn smolweb_transport_failure(current: &url::Url, error: errand::Error) -> FetchFailure {
    match error {
        errand::Error::CertificateChanged { host, pinned, seen } => {
            FetchFailure::CertificateChanged {
                url: current.to_string(),
                target: host,
                pinned,
                seen,
            }
        }
        error => FetchFailure::Failed(error.to_string()),
    }
}

fn smolweb_input_failure(current: &url::Url, response: &errand::Response) -> FetchFailure {
    FetchFailure::InputRequired {
        url: current.to_string(),
        prompt: response.meta.clone(),
        // Gemini status 11 is the sensitive-input form. Other protocols
        // currently expose only an ordinary input code.
        sensitive: response.raw_status == Some(11),
    }
}

/// The content-type to render a smolweb response under, in nematic's vocabulary.
/// Most schemes carry their own media type (gemini/spartan `text/gemini`, a gopher
/// menu `application/gopher-menu`, a gopher text file `text/plain`); finger has no
/// type of its own, so it is tagged `text/x-finger` to reach the finger engine.
fn smolweb_content_type(url: &url::Url, response: &errand::Response) -> String {
    match errand::Scheme::parse(url.scheme()) {
        // Protocols whose content type must be fixed regardless of the response
        // meta field, so the host routes to the correct nematic engine.
        Some(errand::Scheme::Finger) => "text/x-finger".to_string(),
        Some(errand::Scheme::Nex) => "application/x-nex".to_string(),
        Some(errand::Scheme::Guppy) => "application/x-guppy".to_string(),
        Some(errand::Scheme::Titan) => "application/x-titan".to_string(),
        _ => response.mime().unwrap_or("text/gemini").to_string(),
    }
}

mod cookies;
pub use cookies::*;

/// Drain a streaming [`netfetcher::ResponseBody`] into a buffer, aborting with an
/// error once the accumulated length would exceed `max_bytes` (§A5). Enforced *during*
/// the stream (not after `bytes()` buffers it all), so an unbounded / chunked body
/// cannot OOM the host: the read stops the moment the cap is crossed.
async fn read_capped(
    mut body: netfetcher::ResponseBody,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = body.next_chunk().await {
        let chunk = chunk.map_err(|e| format!("read error: {e}"))?;
        if buf.len() + chunk.len() > max_bytes {
            return Err(format!("response exceeds the {max_bytes}-byte cap"));
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

/// Run one WHATWG-Fetch GET and collect the decoded body as text, bounded by
/// `max_bytes` (§A5: a hard streamed cap so a huge response can't OOM the host).
async fn do_fetch(url: &str, max_bytes: usize) -> Result<Fetched, String> {
    do_fetch_ua(url, max_bytes, None).await
}

/// The crawl actor's descriptive User-Agent: a site can allow / block / reach the bot
/// from it (politeness, alongside robots.txt). The `merebot` token matches the
/// `User-agent:` groups `crawl::robots` honors; the `Mozilla/5.0 (compatible; …)`
/// shape is the conventional crawler form (cf. Googlebot).
pub const CRAWLER_USER_AGENT: &str =
    "Mozilla/5.0 (compatible; merebot/0.1; +https://mere.computer/bot)";

/// Like [`do_fetch`], but with an explicit `user_agent` request header when `Some`
/// (the crawler identifies itself); `None` lets netfetcher add its default browser UA.
async fn do_fetch_ua(
    url: &str,
    max_bytes: usize,
    user_agent: Option<&str>,
) -> Result<Fetched, String> {
    let cx = session_context();
    do_fetch_ua_with_context(url, max_bytes, user_agent, &cx).await
}

async fn do_fetch_ua_with_context(
    url: &str,
    max_bytes: usize,
    user_agent: Option<&str>,
    cx: &netfetcher::FetchContext,
) -> Result<Fetched, String> {
    let parsed = url::Url::parse(url).map_err(|e| format!("bad URL: {e}"))?;
    let mut request = netfetcher::Request::get(parsed);
    if let Some(ua) = user_agent {
        // netfetcher only injects its default UA when none is set, so this wins.
        request
            .headers
            .push(("user-agent".to_owned(), ua.to_owned()));
    }
    let response = netfetcher::fetch(request, cx).await;
    if response.is_network_error() {
        return Err("network error".to_string());
    }
    let status = response.status;
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}"));
    }
    let content_type = response
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        .map(|(_, v)| v.clone());
    let bytes = read_capped(response.body, max_bytes).await?;
    let body = String::from_utf8_lossy(&bytes).into_owned();
    Ok(Fetched { content_type, body })
}

/// Run one WHATWG-Fetch GET and collect the raw response bytes, or `None` on any
/// network / HTTP / read / over-cap error. The subresource counterpart to
/// [`do_fetch`], bounded by [`SUBRESOURCE_BODY_CAP`].
async fn fetch_bytes(url: &str) -> Option<Vec<u8>> {
    let parsed = url::Url::parse(url).ok()?;
    let cx = session_context();
    let response = netfetcher::fetch(netfetcher::Request::get(parsed), &cx).await;
    if response.is_network_error() || !(200..300).contains(&response.status) {
        return None;
    }
    read_capped(response.body, SUBRESOURCE_BODY_CAP).await.ok()
}

#[cfg(test)]
mod tests;
