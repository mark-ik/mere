# mere-eidetic-https-fetcher

Package `mere-eidetic-https-fetcher`; the library is `eidetic_https_fetcher`.

`HttpsFetcher` is an `eidetic::BlobFetcher` that resolves
`BlobSource::Https { url }` with a synchronous HTTPS GET via `ureq`. Every other
source kind returns `Ok(None)` so `eidetic::manifest::resolve_blob` falls through
to the next fetcher or source.

| Item | Signature |
|---|---|
| `HttpsFetcher::new` | `() -> Self`. A fresh `ureq::Agent` and the default 1 GiB response cap |
| `HttpsFetcher::with_agent` | `(agent: ureq::Agent) -> Self`. Bring your own agent for connection pooling, proxy, or TLS setup |
| `HttpsFetcher::with_max_response_bytes` | `(self, max: u64) -> Self`. Overrides the cap |
| `BlobFetcher::fetch` | `(&mut self, source: &BlobSource) -> Result<Option<Vec<u8>>>` |

The cap is checked twice: against `Content-Length` when the response declares
one, and again against the bytes actually read.

This crate retrieves bytes and does not verify the hash.
`eidetic::manifest::resolve_blob` BLAKE3-checks every successful fetch against
the manifest's declared `content_hash`.

Native-only; `ureq` is a native HTTP client. Browser-side fetching would use the
wasm `fetch` API in a separate crate.

Dependencies: `mere-eidetic`, `ureq` 2.10 (`default-features = false`, feature
`tls`), `async-trait`.

## License

MPL-2.0 (see LICENSE).
