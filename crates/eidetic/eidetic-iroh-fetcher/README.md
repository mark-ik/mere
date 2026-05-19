# eidetic-iroh-fetcher

Iroh [`BlobFetcher`] companion crate for [`eidetic`].

Implements `eidetic::BlobFetcher::fetch` for `BlobSource::Iroh { ticket }` by parsing the ticket as `"<node-id-hex>/<blob-hash-hex>"`, fetching the blob from the named peer through [`transport`]'s iroh-blobs integration, and returning the bytes.

The fetcher does **not** verify the response hash itself — `iroh-blobs` already does BLAKE3 verification natively as part of its transfer protocol, and `eidetic::resolve_blob` BLAKE3-checks again against the manifest's `content_hash` for defense-in-depth.

Returns `Ok(None)` for any other source kind so [`eidetic::resolve_blob`] can fall through to the next fetcher / source.

## Native-only

Pulls in iroh + iroh-blobs transitively. Native-only; browser-side iroh transfer would require an alternate transport layer.

[`BlobFetcher`]: https://docs.rs/eidetic/0.0.1/eidetic/manifest/trait.BlobFetcher.html
[`eidetic`]: https://crates.io/crates/eidetic
[`transport`]: https://crates.io/crates/transport
