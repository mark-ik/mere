// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Net-fetch host seam: response type, fetcher trait, origin helpers.

/// A response from [`NetFetcher::fetch`] (a clean public mirror of the WIT
/// `net::response`, so an embedder need not name the generated type).
pub struct NetResponse {
    pub status: u32,
    /// The response media type, if the backend reported one (mirrors WIT `content-type`).
    pub content_type: Option<String>,
    pub body: String,
}

/// The host's network backend for `net.fetch` (§11.7-7). document-host stays
/// dependency-light (no netfetcher/errand), so the embedder injects a concrete
/// fetcher at [`DocumentScript::attach`]. `fetch` is **blocking**: it runs inside
/// the guest's `net.fetch` host call on the turn's fiber, and the (sync) content
/// actor drives the fiber via `pollster`, so the impl blocks that actor thread for
/// the I/O (e.g. `block_on` over netfetcher/errand) and returns a ready result.
/// `Err` surfaces to the guest as `result::err`.
pub trait NetFetcher: Send + Sync {
    fn fetch(&self, url: &str) -> Result<NetResponse, String>;
}

/// The normalized host of a URL for the `net` allowlist check: the authority between
/// `://` and the next `/?#`, with userinfo (`user@`) and `:port` stripped, IPv6
/// brackets removed, lowercased. Mirrors the embedder's `host_of`; kept local so
/// document-host stays embedder-agnostic (a shared crate is the longer-term home).
pub(crate) fn net_host_of(url: &str) -> String {
    let after = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let authority = after.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority
        .rsplit_once('@')
        .map(|(_, h)| h)
        .unwrap_or(authority);
    let host = if let Some(rest) = host_port.strip_prefix('[') {
        rest.split_once(']')
            .map(|(inner, _)| inner)
            .unwrap_or(host_port)
    } else {
        host_port.split(':').next().unwrap_or(host_port)
    };
    host.to_ascii_lowercase()
}

/// Whether `url`'s host is in the granted `net` origin allowlist (§E1): an exact host
/// or a `*.suffix` glob. An empty allowlist denies everything (fail-closed).
pub(crate) fn host_in_origins(url: &str, origins: &[String]) -> bool {
    let host = net_host_of(url);
    origins.iter().any(|pattern| {
        let pattern = pattern.to_ascii_lowercase();
        match pattern.strip_prefix("*.") {
            Some(suffix) => host == suffix || host.ends_with(&format!(".{suffix}")),
            None => host == pattern,
        }
    })
}
