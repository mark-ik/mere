# eidetic-fjall

Fjall LSM [`Store`] implementation for [`eidetic`].

The production-default native backend. Persists blobs and manifests under a single keyspace partition; values up to a few MB are fine inline, larger values (model weights, etc.) too — fjall is fine with multi-MB values, though the [`eidetic`] design pass also offers content-hash-keyed sharding patterns when that matters.

Native-only. Browser-side persistence uses `eidetic-opfs` (separate companion crate, planned).

[`Store`]: https://docs.rs/eidetic/0.0.1/eidetic/trait.Store.html
[`eidetic`]: https://crates.io/crates/eidetic
