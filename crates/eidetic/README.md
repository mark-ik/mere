# eidetic

`eidetic` is your private, local, memory database for the
[mere](https://crates.io/crates/mere) browser. It owns the vocabulary for
owner-scoped local blobs: graph snapshots, traversal logs, settings, caches.

In the printing-press metaphor: eidetic is what remains after the impression,
but also the muscle memory of the process. Engines, inkers, and presses
produce; eidetic remembers and manages the big picture and the details.
Want a rolling 3 months of memory, or no memory at all? eidetic can handle
that too. Higher memory layers build on your eidetic memory to recall observations,
draw inferences, and distill engrams (portable, durable, schematicized memories).

## What's in the crate

- [`Request`] / [`Response`] — typed blob load/save vocabulary; serde-derived.
- [`Store`] — trait that storage backends implement (fjall, redb, IndexedDB,
  in-memory, …). Implementations live outside this crate.
- [`Error`] / [`Result`] — owned error vocabulary; downstream crates
  `From`-convert into their own service errors without taking a dependency on
  any particular error library.
- [`dispatch`] — routes a `Request` to a `Store` and produces the matching
  `Response`.

The crate is intentionally minimal. Index, snapshot, journal, and other
higher-level seams build on top of `Store`; they don't live here.

## Naming

*Eidetic* memory is the rare capacity to recall mental images with vivid,
near-photographic clarity. The crate name borrows the connotation: a memory
lane that holds onto what passed through with high fidelity.

The prototype name `mnem` was unavailable on crates.io; lucky me.
Runners-up `idyl` and `eido` are kept on the bench,
'cause I couldn't believe this one was available!

## How it relates to other workspace crates

eidetic is a leaf crate: it depends on no other mere workspace crate (only
`serde`). Consumers reach for it; it doesn't reach for them.

```text
   graphshell::app_state
        │ WorkspaceEffect::RequestEidetic(eidetic::Request)
        ▼
   eidetic::dispatch
        │
        ▼
   host's eidetic::Store impl
        │ (fjall, redb, IndexedDB, in-memory, …)
```

- [`graphshell`](https://crates.io/crates/graphshell) — `app_state` emits
  `eidetic::Request` effects via `WorkspaceEffect::RequestEidetic`; service
  glue calls `eidetic::dispatch` against a host-supplied `eidetic::Store`.
  `eidetic::Error` converts into `WorkspaceServiceError` through a `From`
  impl.
- [`mere`](https://crates.io/crates/mere) — composes eidetic into the
  product alongside a concrete `Store` backend.

### Peers it's easy to confuse with

eidetic occupies a different scope from these adjacent crates:

- [`mere-transport`](https://crates.io/crates/mere-transport) carries an
  `iroh-blobs` content-addressed blob store — networked / shared between
  peers, not owner-private.
- [`moothold`](https://crates.io/crates/moothold) owns community flora
  (engrams composing a moot's geist) — shared across community members, not
  owner-private.
- Host UI state (open dialogs, focus, transient overlays) — runtime-only,
  not durable.

## Status

Pre-1.0. The public surface is small and intended to stabilize quickly; the
ecosystem of `Store` implementations is what's expected to evolve.

## License

[MPL-2.0](../../LICENSE).

[`Request`]: https://docs.rs/eidetic/latest/eidetic/enum.Request.html
[`Response`]: https://docs.rs/eidetic/latest/eidetic/enum.Response.html
[`Store`]: https://docs.rs/eidetic/latest/eidetic/trait.Store.html
[`Error`]: https://docs.rs/eidetic/latest/eidetic/struct.Error.html
[`Result`]: https://docs.rs/eidetic/latest/eidetic/type.Result.html
[`dispatch`]: https://docs.rs/eidetic/latest/eidetic/fn.dispatch.html
