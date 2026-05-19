# eidetic-https-fetcher

HTTPS [`BlobFetcher`] companion crate for [`eidetic`].

Implements `eidetic::BlobFetcher::fetch` for `BlobSource::Https { url }` by issuing a synchronous HTTPS GET via [`ureq`]. Returns `Ok(None)` for any other source kind so [`eidetic::resolve_blob`] can fall through to the next fetcher / source.

The fetcher does **not** verify the response hash — that's the responsibility of `eidetic::resolve_blob`, which BLAKE3-checks every successful fetch against the manifest's declared `content_hash`. This crate just retrieves bytes.

Native-only (no wasm). Browser-side HTTPS fetching uses the wasm `fetch` API; that lives in a separate companion crate (TBD).

[`BlobFetcher`]: https://docs.rs/eidetic/0.0.1/eidetic/manifest/trait.BlobFetcher.html
[`eidetic`]: https://crates.io/crates/eidetic
[`ureq`]: https://crates.io/crates/ureq
