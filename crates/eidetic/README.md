# eidetic

The durable-memory family covering raw bytes, append-only journals, the
container graph, semantic projection, the typed memory lane, and its backends.

## The substrate crates

Published under their own names. Formerly four sibling repos, merged into one
2026-07-21 and absorbed into this workspace 2026-07-23, histories preserved.

| Directory | Package | Contents |
|---|---|---|
| [`muniment`](muniment) | `muniment` | The storage floor: host-supplied backends, mutable typed slots, immutable content-addressed blobs, and `Journal<T>` append-only histories addressed by `Seq`. Ships memory, redb, zip, and IndexedDB backends. |
| [`chartulary`](chartulary) | `chartulary` | The content-addressed container graph. `Graph<N, E>` on petgraph with capability traits, default `Container`/`Relation` payloads, the `GraphLog` edit spine, runtime `facet` metadata, `content_class`, the `stemma` lineage layer, and the semantic-ring RDF projection at `chartulary::rdf`. |

## The eidetic lane

Mere's owner-scoped local memory, which carried the eidetic name first. Packages
are named `mere-*`; each sets a `[lib] name` so consumers still write
`use eidetic::…`.

| Directory | Package | Lib name | Contents |
|---|---|---|---|
| [`eidetic-core`](eidetic-core) | `mere-eidetic` | `eidetic` | Manifests, typed payloads, schemas, codicils, bundles, packs, sealing, browsing traces, model artifacts. `Store` is an alias for `muniment::Backend`. |
| [`eidetic-fjall`](eidetic-fjall) | `mere-eidetic-fjall` | `eidetic_fjall` | `FjallStore`, the production-default native backend. |
| [`eidetic-https-fetcher`](eidetic-https-fetcher) | `mere-eidetic-https-fetcher` | `eidetic_https_fetcher` | `HttpsFetcher`, a `BlobFetcher` for `BlobSource::Https`. |
| [`eidetic-iroh-fetcher`](eidetic-iroh-fetcher) | `mere-eidetic-iroh-fetcher` | `eidetic_iroh_fetcher` | `IrohFetcher`, a `BlobFetcher` for `BlobSource::Iroh`. |
| [`eidetic-search`](eidetic-search) | `mere-eidetic-search` | `eidetic_search` | `TrailIndex`, a tantivy index minted from `BrowsingTrace` codicils, plus the `fuse` hybrid-ranking seam. |

## Design docs

Per-crate design docs live beside `muniment` and `chartulary` (for example
[`muniment/design_docs/`](muniment/design_docs)). The eidetic lane's plans live
in the repo-level `design_docs/mere_docs/`.

## License

MPL-2.0 (see LICENSE).
