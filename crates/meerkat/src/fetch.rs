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

use std::sync::mpsc::{channel, Receiver, Sender};

use tokio::runtime::Runtime;
use winit::event_loop::EventLoopProxy;

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

/// Whether `url` is a network address meerkat fetches, vs a synthesized `mere://`
/// page or another non-network scheme.
pub fn is_fetchable(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

/// Owns the fetch runtime + the wake/deliver seam. [`spawn`](Fetcher::spawn) runs
/// a fetch off the UI thread; on completion it pushes a [`FetchOutcome`] and wakes
/// the event loop.
pub struct Fetcher {
    runtime: Runtime,
    proxy: EventLoopProxy<()>,
    tx: Sender<FetchOutcome>,
}

impl Fetcher {
    /// Build a fetcher over a multi-thread tokio runtime, returning it plus the
    /// receiver the host drains in `user_event`.
    pub fn new(proxy: EventLoopProxy<()>) -> (Self, Receiver<FetchOutcome>) {
        let (tx, rx) = channel();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("build the fetch runtime");
        (Self { runtime, proxy, tx }, rx)
    }

    /// Fetch `url` off the UI thread; push the outcome and wake the loop when done.
    pub fn spawn(&self, url: String) {
        let tx = self.tx.clone();
        let proxy = self.proxy.clone();
        self.runtime.spawn(async move {
            let result = do_fetch(&url).await;
            // Send first, then wake: the UI-thread drain reads the channel on wake.
            let _ = tx.send(FetchOutcome { url, result });
            let _ = proxy.send_event(());
        });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetchable_only_for_http_schemes() {
        assert!(is_fetchable("http://example.com"));
        assert!(is_fetchable("https://example.com"));
        assert!(!is_fetchable("mere://welcome"));
        assert!(!is_fetchable("about:blank"));
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
