# mere-eidetic-iroh-fetcher

Package `mere-eidetic-iroh-fetcher`; the library is `eidetic_iroh_fetcher`.

`IrohFetcher` is an `eidetic::BlobFetcher` that resolves
`BlobSource::Iroh { ticket }` by parsing the ticket, pulling the blob from the
named peer through `mere-transport`'s `BlobStore` / `P2pandaTransport`, and
returning the bytes. Every other source kind returns `Ok(None)` so
`eidetic::manifest::resolve_blob` falls through.

| Item | Signature |
|---|---|
| `IrohFetcher::new` | `(blobs: Arc<BlobStore>, transport: Arc<P2pandaTransport>) -> Self` |
| `BlobFetcher::fetch` | `(&mut self, source: &BlobSource) -> Result<Option<Vec<u8>>>` |
| `build_ticket` | `(peer_id: PeerID, blob_hash: BlobHash) -> String` |

Construction takes references to an already-running iroh stack; it does not
start a node.

## Ticket format

`"<node-id-hex>/<blob-hash-hex>"`: 64 hex chars, a slash, 64 hex chars, 129
total. Same `(PeerID, BlobHash)` pair that iroh-blobs' base32 `BlobTicket`
encodes, in a form that reads plainly in a manifest.

## Hash verification

Two independent BLAKE3 checks happen on every successful fetch: iroh-blobs
verifies as part of its transfer protocol, and
`eidetic::manifest::resolve_blob` re-verifies against the manifest's
`content_hash`.

Native-only; pulls in iroh and iroh-blobs transitively.

Dependencies: `mere-eidetic`, `mere-transport` (as `transport`), `async-trait`.

## License

MIT OR Apache-2.0.
