# muniment

A portable persistence store. One host-supplied byte `Backend` seam (filesystem,
OPFS, redb, fjall), typed mutable `SlotStore` slots over a pluggable codec, and
content-addressed immutable `BlobStore` blobs. Format-agnostic and
wasm-friendly. The storage floor a small app keeps its durable state in.

```rust
use muniment::{BlobStore, JsonSlots, MemoryBackend};

# pollster::block_on(async {
let backend = MemoryBackend::new();               // host swaps in fs / OPFS
let slots = JsonSlots::new(backend.clone());
let blobs = BlobStore::new(backend);

slots.save("session", &("Practice", 96.0)).await?; // mutable named slot
let session: Option<(String, f32)> = slots.load("session").await?;

let hash = blobs.put(b"media bytes").await?;        // content-addressed blob
assert_eq!(blobs.get(&hash).await?.unwrap(), b"media bytes");
# Ok::<_, muniment::StoreError>(()) }).unwrap();
```

## Modules

| Module | Contents |
|---|---|
| `backend` | `Backend` (`get`, `put`, `delete`, `list`, `scan`, `apply`), `WriteOp`, `MemoryBackend` |
| `slot` | `SlotStore<B, C>`: `save`, `load`, `delete`, `keys` |
| `blob` | `BlobStore<B>`: `put`, `get`, `has`; `Hash` (blake3, `of` / `as_bytes` / `to_hex` / `from_hex`) |
| `codec` | `Codec`, `JsonCodec`, `PostcardCodec` |
| `error` | `StoreError` |
| `redb_backend` | `RedbBackend` (feature `redb`) |
| `zip_backend` | `ZipBackend` (feature `zip`) |
| `indexeddb_backend` | `IndexedDbBackend` (feature `indexeddb`, wasm32 only) |

Type aliases `JsonSlots<B>` and `PostcardSlots<B>` pair `SlotStore` with the
matching codec.

## Features

| Feature | Default | Effect |
|---|---|---|
| `json` | yes | `JsonCodec` and `JsonSlots` via `serde_json` |
| `postcard` | no | `PostcardCodec` and `PostcardSlots` |
| `redb` | no | `RedbBackend`, the durable desktop store |
| `zip` | no | `ZipBackend`, a zip archive whose entries are the store's keys |
| `indexeddb` | no | `IndexedDbBackend`. Its `js-sys` / `wasm-bindgen` / `web-sys` / `futures-channel` deps are declared for wasm32 only, so enabling it in a shared feature set is a no-op on desktop |

The `Backend` is `async` and `?Send` so a browser main thread can await OPFS
promises, while desktop backends return ready futures. `scan` is an ordered
half-open range read; `apply` commits a batch of `WriteOp`s, atomically where the
backend has transactions. muniment moves bytes and does not model what they mean.

Built from a survey of four consumers (woodshed, hocket, isometry, mere), each
of which was hand-rolling this seam. Sibling to
[codicil](https://github.com/merely-made/mere), the append-only log that versions
what muniment stores. See [`design_docs/`](design_docs/).

The name: a muniment room is where a household keeps its deeds and records, the
documents preserved as evidence.

License: dual MIT OR Apache-2.0, at your option.
