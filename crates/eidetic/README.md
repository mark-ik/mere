# eidetic

The durable-memory family: nine crates covering raw bytes, the append-only log,
the container graph, semantic projection, the typed memory lane, and its
backends.

## The substrate crates

Published under their own names. Formerly four sibling repos, merged into one
2026-07-21 and absorbed into this workspace 2026-07-23, histories preserved.

| Directory | Package | Contents |
|---|---|---|
| [`muniment`](muniment) | `muniment` | The persistence floor. `Backend` (host-supplied byte seam), `SlotStore` (typed mutable slots over a `Codec`), `BlobStore` (content-addressed blake3 blobs). Ships `MemoryBackend`, plus `RedbBackend`, `ZipBackend`, and `IndexedDbBackend` behind features. |
| [`codicil`](codicil) | `codicil` | The append-only log. `Codicil<T>` of immutable entries addressed by `Seq`, replayed to rebuild state, persisted through a muniment slot. Carries causal parent links and `fork`/`Provenance`. |
| [`chartulary`](chartulary) | `chartulary` | The content-addressed container graph. `Graph<N, E>` on petgraph with capability traits, default `Container`/`Relation` payloads, the `GraphLog` edit spine, runtime `facet` metadata, `content_class`, and the `stemma` lineage layer. |
| [`scholia`](scholia) | `scholia` | The RDF projection over a chartulary graph's semantic ring. `to_quads`, `to_jsonld`, `to_nquads`. |

## The eidetic lane

Mere's owner-scoped local memory, which carried the eidetic name first. Packages
are named `mere-*`; each sets a `[lib] name` so consumers still write
`use eidetic::…`.

| Directory | Package | Lib name | Contents |
|---|---|---|---|
| [`eidetic-core`](eidetic-core) | `mere-eidetic` | `eidetic` | Manifests, typed payloads, schemas, engrams, bundles, packs, sealing, browsing traces, model artifacts. `Store` is an alias for `muniment::Backend`. |
| [`eidetic-fjall`](eidetic-fjall) | `mere-eidetic-fjall` | `eidetic_fjall` | `FjallStore`, the production-default native backend. |
| [`eidetic-https-fetcher`](eidetic-https-fetcher) | `mere-eidetic-https-fetcher` | `eidetic_https_fetcher` | `HttpsFetcher`, a `BlobFetcher` for `BlobSource::Https`. |
| [`eidetic-iroh-fetcher`](eidetic-iroh-fetcher) | `mere-eidetic-iroh-fetcher` | `eidetic_iroh_fetcher` | `IrohFetcher`, a `BlobFetcher` for `BlobSource::Iroh`. |
| [`eidetic-search`](eidetic-search) | `mere-eidetic-search` | `eidetic_search` | `TrailIndex`, a tantivy index minted from `BrowsingTrace` engrams, plus the `fuse` hybrid-ranking seam. |

## Design docs

Per-crate design docs live beside `muniment`, `codicil`, `chartulary`, and
`scholia` (for example [`muniment/design_docs/`](muniment/design_docs)). The
eidetic lane's plans live in the repo-level `design_docs/mere_docs/`.

## License

MPL-2.0 (see LICENSE).
