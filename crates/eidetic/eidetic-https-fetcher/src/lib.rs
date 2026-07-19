// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! HTTPS [`BlobFetcher`] companion crate for [`eidetic`].
//!
//! Implements `eidetic::BlobFetcher` for `BlobSource::Https { url }` via
//! synchronous HTTPS GET (ureq). Returns `Ok(None)` for any other source
//! kind so [`eidetic::manifest::resolve_blob`] can fall through to the
//! next fetcher / source.
//!
//! The fetcher does **not** verify the response hash — that's the
//! responsibility of `resolve_blob`, which BLAKE3-checks every successful
//! fetch against the manifest's declared `content_hash`.
//!
//! ## Native-only
//!
//! `ureq` is a native HTTP client; this crate does not target wasm.
//! Browser-side HTTPS fetching uses the wasm `fetch` API and lives in a
//! separate companion crate (when needed).

#![doc(html_root_url = "https://docs.rs/eidetic-https-fetcher/0.0.1")]

use async_trait::async_trait;
use eidetic::{BlobFetcher, BlobSource, Error, Result};
use std::io::Read;

/// HTTPS-only [`BlobFetcher`].
///
/// Configurable via [`Self::with_max_response_bytes`] to refuse responses
/// over a size threshold (default: 1 GiB) — protects against runaway
/// downloads from misbehaving or compromised origins.
pub struct HttpsFetcher {
    agent: ureq::Agent,
    max_response_bytes: u64,
}

impl Default for HttpsFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpsFetcher {
    /// Construct with a fresh ureq agent and the default 1 GiB response
    /// cap.
    pub fn new() -> Self {
        Self {
            agent: ureq::AgentBuilder::new().build(),
            max_response_bytes: 1024 * 1024 * 1024,
        }
    }

    /// Provide a custom ureq agent (for connection-pool sharing,
    /// proxy/tls customization, etc.).
    pub fn with_agent(agent: ureq::Agent) -> Self {
        Self {
            agent,
            max_response_bytes: 1024 * 1024 * 1024,
        }
    }

    /// Override the maximum response size accepted before erroring.
    pub fn with_max_response_bytes(mut self, max: u64) -> Self {
        self.max_response_bytes = max;
        self
    }

    fn fetch_https(&self, url: &str) -> Result<Vec<u8>> {
        let response = self
            .agent
            .get(url)
            .call()
            .map_err(|e| Error::new(format!("https fetch {url}: {e}")))?;

        if let Some(len_str) = response.header("Content-Length") {
            if let Ok(len) = len_str.parse::<u64>() {
                if len > self.max_response_bytes {
                    return Err(Error::new(format!(
                        "https fetch {url}: Content-Length {len} exceeds max {}",
                        self.max_response_bytes
                    )));
                }
            }
        }

        let mut buffer = Vec::new();
        let reader = response.into_reader();
        let mut limited = reader.take(self.max_response_bytes.saturating_add(1));
        limited
            .read_to_end(&mut buffer)
            .map_err(|e| Error::new(format!("https read {url}: {e}")))?;

        if buffer.len() as u64 > self.max_response_bytes {
            return Err(Error::new(format!(
                "https fetch {url}: response exceeded max {} bytes",
                self.max_response_bytes
            )));
        }

        Ok(buffer)
    }
}

#[async_trait(?Send)]
impl BlobFetcher for HttpsFetcher {
    async fn fetch(&mut self, source: &BlobSource) -> Result<Option<Vec<u8>>> {
        match source {
            BlobSource::Https { url } => self.fetch_https(url).map(Some),
            // Every other source kind is somebody else's problem.
            _ => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_through_for_non_https_sources() {
        pollster::block_on(async {
            let mut fetcher = HttpsFetcher::new();
            assert!(matches!(
                fetcher
                    .fetch(&BlobSource::Local {
                        key: "blob:abc".to_string(),
                    })
                    .await,
                Ok(None)
            ));
            assert!(matches!(
                fetcher
                    .fetch(&BlobSource::Iroh {
                        ticket: "ticket".to_string(),
                    })
                    .await,
                Ok(None)
            ));
            assert!(matches!(
                fetcher
                    .fetch(&BlobSource::LocalFile {
                        path: "/tmp/x".to_string(),
                    })
                    .await,
                Ok(None)
            ));
            assert!(matches!(
                fetcher
                    .fetch(&BlobSource::LocalOnlyRef {
                        local_ref: "host:/x".to_string(),
                    })
                    .await,
                Ok(None)
            ));
            assert!(matches!(
                fetcher
                    .fetch(&BlobSource::Embedded {
                        bytes: vec![1, 2, 3],
                        media_type: "application/octet-stream".to_string(),
                    })
                    .await,
                Ok(None)
            ));
        });
    }

    #[test]
    fn fetches_https_response_body() {
        pollster::block_on(async {
            let mut server = mockito::Server::new();
            let body = b"hello-from-mock".to_vec();
            let _m = server
                .mock("GET", "/blob")
                .with_status(200)
                .with_header("content-type", "application/octet-stream")
                .with_body(&body)
                .create();

            let url = format!("{}/blob", server.url());
            let mut fetcher = HttpsFetcher::new();
            let result = fetcher.fetch(&BlobSource::Https { url }).await.unwrap();
            assert_eq!(result, Some(body));
        });
    }

    #[test]
    fn surfaces_error_on_404() {
        pollster::block_on(async {
            let mut server = mockito::Server::new();
            let _m = server
                .mock("GET", "/missing")
                .with_status(404)
                .with_body("not here")
                .create();

            let url = format!("{}/missing", server.url());
            let mut fetcher = HttpsFetcher::new();
            let result = fetcher.fetch(&BlobSource::Https { url }).await;
            assert!(result.is_err());
            assert!(format!("{}", result.unwrap_err()).contains("https fetch"));
        });
    }

    #[test]
    fn rejects_response_exceeding_max_bytes() {
        pollster::block_on(async {
            let mut server = mockito::Server::new();
            // Response is 100 bytes; cap is 50 -> should error.
            let body = vec![0u8; 100];
            let _m = server
                .mock("GET", "/big")
                .with_status(200)
                .with_body(&body)
                .create();

            let url = format!("{}/big", server.url());
            let mut fetcher = HttpsFetcher::new().with_max_response_bytes(50);
            let result = fetcher.fetch(&BlobSource::Https { url }).await;
            assert!(result.is_err());
            assert!(format!("{}", result.unwrap_err()).contains("max"));
        });
    }
}
