/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

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
//! routing to the nematic + serval engines is S2.2b-ii.
//!
//! Smolweb: the same actor also speaks the small web. A page request is routed by
//! scheme, http(s) runs through netfetcher (WHATWG Fetch) and gemini / gopher /
//! finger / spartan / nex / guppy / titan run through the [`errand`] transport
//! crate. Either way the result is the same [`Fetched`] (decoded body +
//! content-type), so the render side (nematic engines) is unchanged.
//! Subresources stay http-only: smolweb documents reference links but do not
//! inline fetched media.

use std::sync::mpsc::Receiver;
use std::time::Duration;

use armillary::{spawn, ActorHandle, Emitter, Wake};
use tokio::runtime::Builder;

/// The most redirects a smolweb fetch will follow before giving up.
const MAX_REDIRECTS: usize = 5;

/// Per-hop timeout for smolweb fetches. Applied to each request in a redirect
/// chain independently so a chain of N slow hops can take up to N × this value.
const SMOLWEB_TIMEOUT: Duration = Duration::from_secs(30);

/// Successfully fetched content: the response content-type (if any) and the
/// decoded body as text.
#[derive(Clone, Debug)]
pub struct Fetched {
    pub content_type: Option<String>,
    pub body: String,
}

/// The result of one fetch, tagged with the requested URL so the host routes it
/// back to the right node's content slot.
pub struct FetchOutcome {
    pub url: String,
    pub result: Result<Fetched, String>,
}

/// A fetched subresource: raw bytes for an absolute URL (page CSS via
/// `<link>`, an `<img>`, ...). Carried as bytes, not text, since media is
/// binary. Only successful fetches are delivered; a failure simply never
/// arrives (the demand loader keeps treating the resource as absent).
pub struct SubresourceOutcome {
    pub url: String,
    pub bytes: Vec<u8>,
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
        },
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
    Page(String),
    /// Fetch the subresource at the (already absolute) `url` as raw bytes.
    Subresource(String),
}

/// An update from the fetch actor: one completed page or subresource fetch.
pub enum FetchUpdate {
    Page(FetchOutcome),
    Subresource(SubresourceOutcome),
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
                FetchCommand::Page(url) => {
                    let out = out.clone();
                    runtime.spawn(async move {
                        let result = fetch_page(&url).await;
                        out.emit(FetchUpdate::Page(FetchOutcome { url, result }));
                    });
                },
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
                },
            }
        }
    })
}

/// Fetch a page, routing by scheme: smolweb schemes go through [`errand`], every
/// other (http(s)) address through netfetcher's WHATWG Fetch.
async fn fetch_page(url: &str) -> Result<Fetched, String> {
    match scheme_of(url).and_then(errand::Scheme::parse) {
        Some(scheme) => {
            tracing::info!(%url, ?scheme, "smolweb fetch");
            let result = smolweb_fetch(url).await;
            match &result {
                Ok(fetched) => tracing::info!(
                    %url,
                    content_type = ?fetched.content_type,
                    bytes = fetched.body.len(),
                    "smolweb ok",
                ),
                Err(error) => tracing::warn!(%url, %error, "smolweb failed"),
            }
            result
        },
        None => do_fetch(url).await,
    }
}

/// Fetch a smolweb URL through [`errand`], following redirects up to
/// [`MAX_REDIRECTS`], and fold the response into a [`Fetched`] the nematic engines
/// render. Non-success statuses (input wanted, cert required, failure) surface as
/// an error string the card shows.
async fn smolweb_fetch(url: &str) -> Result<Fetched, String> {
    let mut current = url::Url::parse(url).map_err(|e| format!("bad URL: {e}"))?;
    for _ in 0..MAX_REDIRECTS {
        let response = errand::fetch_url_timeout(&current, SMOLWEB_TIMEOUT)
            .await
            .map_err(|e| e.to_string())?;
        match response.status {
            errand::Status::Success => {
                let content_type = smolweb_content_type(&current, &response);
                let body = String::from_utf8_lossy(&response.body).into_owned();
                return Ok(Fetched { content_type: Some(content_type), body });
            },
            errand::Status::Redirect => {
                current = current
                    .join(&response.meta)
                    .map_err(|e| format!("bad redirect target: {e}"))?;
            },
            errand::Status::Input => return Err(format!("input required: {}", response.meta)),
            errand::Status::CertRequired => return Err("client certificate required".to_string()),
            errand::Status::Failure => {
                return Err(if response.meta.is_empty() {
                    "request failed".to_string()
                } else {
                    response.meta
                });
            },
        }
    }
    Err("too many redirects".to_string())
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

/// Run one WHATWG-Fetch GET and collect the decoded body as text.
async fn do_fetch(url: &str) -> Result<Fetched, String> {
    let parsed = url::Url::parse(url).map_err(|e| format!("bad URL: {e}"))?;
    let cx = netfetcher::FetchContext::permissive();
    let response = netfetcher::fetch(netfetcher::Request::get(parsed), &cx).await;
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
    let bytes = response.bytes().await.map_err(|e| format!("read error: {e}"))?;
    let body = String::from_utf8_lossy(&bytes).into_owned();
    Ok(Fetched { content_type, body })
}

/// Run one WHATWG-Fetch GET and collect the raw response bytes, or `None` on any
/// network / HTTP / read error. The subresource counterpart to [`do_fetch`].
async fn fetch_bytes(url: &str) -> Option<Vec<u8>> {
    let parsed = url::Url::parse(url).ok()?;
    let cx = netfetcher::FetchContext::permissive();
    let response = netfetcher::fetch(netfetcher::Request::get(parsed), &cx).await;
    if response.is_network_error() || !(200..300).contains(&response.status) {
        return None;
    }
    response.bytes().await.ok().map(|b| b.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetchable_for_http_and_smolweb_schemes() {
        assert!(is_fetchable("http://example.com"));
        assert!(is_fetchable("https://example.com"));
        assert!(is_fetchable("gemini://capsule.example/"));
        assert!(is_fetchable("gopher://example.org/"));
        assert!(is_fetchable("finger://example.org/alice"));
        assert!(is_fetchable("spartan://example.org/"));
        assert!(is_fetchable("nex://nightfall.city/"));
        assert!(is_fetchable("guppy://mozz.us/"));
        assert!(is_fetchable("titan://capsule.example/page"));
        assert!(!is_fetchable("mere://welcome"));
        assert!(!is_fetchable("about:blank"));
    }

    #[test]
    fn smolweb_content_type_tags_fixed_schemes_and_passes_others_through() {
        fn resp_for(url: &url::Url, mime: &str) -> errand::Response {
            errand::Response {
                url: url.clone(),
                status: errand::Status::Success,
                raw_status: None,
                meta: mime.to_string(),
                body: Vec::new(),
            }
        }

        let finger = url::Url::parse("finger://example.org/alice").unwrap();
        assert_eq!(smolweb_content_type(&finger, &resp_for(&finger, "text/plain")), "text/x-finger");

        let nex = url::Url::parse("nex://nightfall.city/").unwrap();
        assert_eq!(smolweb_content_type(&nex, &resp_for(&nex, "")), "application/x-nex");

        let guppy = url::Url::parse("guppy://mozz.us/").unwrap();
        assert_eq!(smolweb_content_type(&guppy, &resp_for(&guppy, "text/gemini")), "application/x-guppy");

        let titan = url::Url::parse("titan://capsule.example/page").unwrap();
        assert_eq!(smolweb_content_type(&titan, &resp_for(&titan, "text/gemini")), "application/x-titan");

        let gem = url::Url::parse("gemini://capsule.example/").unwrap();
        let gem_resp = errand::Response {
            url: gem.clone(),
            status: errand::Status::Success,
            raw_status: Some(20),
            meta: "text/gemini; charset=utf-8".into(),
            body: Vec::new(),
        };
        assert_eq!(smolweb_content_type(&gem, &gem_resp), "text/gemini");
    }

    #[test]
    fn state_tag_distinguishes_transitions() {
        let ready = ContentState::Ready(Fetched { content_type: None, body: String::new() });
        assert_eq!(ContentState::tag(None), 0);
        assert_eq!(ContentState::tag(Some(&ContentState::Loading)), 1);
        assert_ne!(
            ContentState::tag(Some(&ContentState::Loading)),
            ContentState::tag(Some(&ready)),
            "Loading and Ready re-key the card",
        );
        assert_ne!(
            ContentState::tag(Some(&ready)),
            ContentState::tag(Some(&ContentState::Failed("x".into()))),
        );
    }
}
