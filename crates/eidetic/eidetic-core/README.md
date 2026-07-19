# eidetic

`eidetic` is your private, local, memory database for the
[mere](https://crates.io/crates/mere) browser. It owns the vocabulary for
owner-scoped local memory: graph snapshots, traversal logs, settings, caches,
typed payloads, schema engrams, and content-addressed model artifacts.

In the printing-press metaphor: eidetic is what remains after the impression,
but also the muscle memory of the process. Engines, inkers, and presses
produce; eidetic remembers and manages the big picture and the details.
Want a rolling 3 months of memory, or no memory at all? eidetic can handle
that too. Higher memory layers build on your eidetic memory to recall observations,
draw inferences, and distill engrams (portable, durable, schematicized memories).

## What's in the crate

- [`Request`] / [`Response`] — reducer-facing blob load/save vocabulary;
  serde-derived.
- [`Store`] — async trait that storage backends implement (fjall, redb, OPFS,
  in-memory, ...). Implementations live outside this crate.
- [`Error`] / [`Result`] — owned error vocabulary; downstream crates
  `From`-convert into their own service errors without taking a dependency on
  any particular error library.
- [`dispatch`] — routes a `Request` to a `Store` and produces the matching
  `Response`.
- Manifest layer — [`BlobManifest`], [`BlobSource`], [`BlobFetcher`], and
  source fallthrough for local, embedded, HTTPS, iroh, file, and local-only
  references.
- Typed payload layer — [`TypedPayload`], [`save_typed`], [`load_typed`],
  and [`list_typed`] bind Rust types to schema references.
- Schema layer — [`SchemaDefinition`], meta-schema bootstrap, Mere-native
  structural validation, JSON Schema validation, and JSON-LD parse/type checks.
- Engram layer — [`Engram`] + [`TimeBounds`] for immutable, schema-typed,
  content-hashed snapshots that can later cross murm / gemot boundaries.
- Bundle layer — [`Bundle`] and required-member verification for composite
  engram-style payloads.
- Model layer — [`ModelManifest`], [`ModelLibrary`], and [`ModelComponents`]
  for content-addressed model weights and tokenizers.

The crate is still storage-backend-agnostic and host-agnostic, but it is no
longer blob-only. Model/provider logic, vector-index behavior, search, training,
and inference stay in sibling crates; eidetic owns the durable typed memory
substrate those crates persist into.

## Naming

*Eidetic* memory is the rare capacity to recall mental images with vivid,
near-photographic clarity. The crate name borrows the connotation: a memory
lane that holds onto what passed through with high fidelity.

The prototype name `mnem` was unavailable on crates.io; lucky me.
Runners-up `idyl` and `eido` are kept on the bench,
'cause I couldn't believe this one was available!

## How it relates to other workspace crates

eidetic is a leaf crate: it depends on no other mere workspace crate. Consumers
reach for it; it doesn't reach for them.

```text
   graphshell::app_state
        │ WorkspaceEffect::RequestEidetic(eidetic::Request)
        ▼
   eidetic::dispatch
        │
        ▼
   host's eidetic::Store impl
        │ (fjall, redb, OPFS, in-memory, ...)
```

- [`graphshell`](https://crates.io/crates/graphshell) — `app_state` emits
  `eidetic::Request` effects via `WorkspaceEffect::RequestEidetic`; service
  glue calls `eidetic::dispatch` against a host-supplied `eidetic::Store`.
  `eidetic::Error` converts into `WorkspaceServiceError` through a `From`
  impl.
- [`mere`](https://crates.io/crates/mere) — composes eidetic into the
  product alongside a concrete `Store` backend.
- [`embed`](https://crates.io/crates/embed)
  — owns embedding providers, vector index, semantic search, and field bridges;
  persists model/index artifacts through eidetic rather than embedding those
  responsibilities here.
- [`eidetic-https-fetcher`](https://crates.io/crates/eidetic-https-fetcher)
  and [`eidetic-iroh-fetcher`](https://crates.io/crates/eidetic-iroh-fetcher)
  — companion [`BlobFetcher`] implementations for non-local sources.

### Peers it's easy to confuse with

eidetic occupies a different scope from these adjacent crates:

- [`transport`](https://crates.io/crates/transport) carries an
  `iroh-blobs` content-addressed blob store — networked / shared between
  peers, not owner-private.
- [`gemot`](https://crates.io/crates/gemot) owns community flora
  (engrams composing a moot's geist) — shared across community members, not
  owner-private.
- Host UI state (open dialogs, focus, transient overlays) — runtime-only,
  not durable.

## Status

Pre-1.0. The layered surface is useful but still settling. Two design gaps are
intentional and visible:

- `Hash` is currently raw BLAKE3 bytes. The substrate brief wants future
  multihash discipline, so this should be resolved before many more digest
  fields become public contracts.
- `Store` has no delete/GC policy yet. Higher layers can save and resolve
  typed artifacts; retention and garbage collection remain follow-up work.

## License

MIT OR Apache-2.0.

[`Request`]: https://docs.rs/eidetic/latest/eidetic/enum.Request.html
[`Response`]: https://docs.rs/eidetic/latest/eidetic/enum.Response.html
[`Store`]: https://docs.rs/eidetic/latest/eidetic/trait.Store.html
[`Error`]: https://docs.rs/eidetic/latest/eidetic/struct.Error.html
[`Result`]: https://docs.rs/eidetic/latest/eidetic/type.Result.html
[`dispatch`]: https://docs.rs/eidetic/latest/eidetic/fn.dispatch.html
[`BlobManifest`]: https://docs.rs/eidetic/latest/eidetic/struct.BlobManifest.html
[`BlobSource`]: https://docs.rs/eidetic/latest/eidetic/enum.BlobSource.html
[`BlobFetcher`]: https://docs.rs/eidetic/latest/eidetic/trait.BlobFetcher.html
[`TypedPayload`]: https://docs.rs/eidetic/latest/eidetic/trait.TypedPayload.html
[`save_typed`]: https://docs.rs/eidetic/latest/eidetic/fn.save_typed.html
[`load_typed`]: https://docs.rs/eidetic/latest/eidetic/fn.load_typed.html
[`list_typed`]: https://docs.rs/eidetic/latest/eidetic/fn.list_typed.html
[`SchemaDefinition`]: https://docs.rs/eidetic/latest/eidetic/struct.SchemaDefinition.html
[`Engram`]: https://docs.rs/eidetic/latest/eidetic/struct.Engram.html
[`TimeBounds`]: https://docs.rs/eidetic/latest/eidetic/struct.TimeBounds.html
[`Bundle`]: https://docs.rs/eidetic/latest/eidetic/struct.Bundle.html
[`ModelManifest`]: https://docs.rs/eidetic/latest/eidetic/struct.ModelManifest.html
[`ModelLibrary`]: https://docs.rs/eidetic/latest/eidetic/struct.ModelLibrary.html
[`ModelComponents`]: https://docs.rs/eidetic/latest/eidetic/struct.ModelComponents.html
