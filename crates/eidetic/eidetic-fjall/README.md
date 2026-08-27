# mere-eidetic-fjall

Package `mere-eidetic-fjall`; the library is `eidetic_fjall`.

`FjallStore` is a fjall LSM [`muniment::Backend`], which is what
`eidetic::Store` aliases. The production-default native store for the eidetic
lane, and usable by any muniment consumer.

| Item | Signature |
|---|---|
| `FjallStore::open` | `(path: impl AsRef<Path>) -> Result<Self, StoreError>`. Opens or creates a keyspace at `path` over `DEFAULT_PARTITION` |
| `FjallStore::open_partition` | `(path: impl AsRef<Path>, partition: &str) -> Result<Self, StoreError>`. One keyspace hosting several logical stores, for example one per identity |
| `DEFAULT_PARTITION` | `&str`, `"eidetic"` |

Implements every `Backend` method: `get`, `put`, `delete`, `list`, `scan`,
`apply`. `list` uses fjall's prefix iterator, `scan` its native key-ordered
range. `delete` is idempotent, per muniment's contract. `apply` writes the batch
in order; fjall has no cross-key transaction on a single partition.

Fjall is sync-blocking. `FjallStore` satisfies the async trait by performing the
blocking call inline and returning a ready future. Where blocking the executor
matters, wrap the call in `tokio::task::spawn_blocking` at the call site.

Native-only. Browser-side persistence uses muniment's `IndexedDbBackend`
(feature `indexeddb`).

Dependencies: `muniment`, `mere-eidetic`, `fjall` 2, `async-trait`.

## License

MPL-2.0 (see LICENSE).

[`muniment::Backend`]: https://docs.rs/muniment/latest/muniment/trait.Backend.html
