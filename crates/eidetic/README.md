# eidetic

The durable-memory family. Eidetic memory holds what passed through it,
exactly; these four crates are that faculty for graph apps, from raw bytes up
to semantic projection:

| Crate | Role |
|---|---|
| [`muniment`](crates/muniment) | The persistence floor: a host-supplied byte-backend seam (filesystem, OPFS, redb, fjall), typed mutable slots over a pluggable codec, content-addressed immutable blobs. |
| [`codicil`](crates/codicil) | The append-only, replayable log: immutable ordered entries appended and replayed to rebuild state, persisted through muniment. |
| [`chartulary`](crates/chartulary) | The content-addressed container graph: `Graph<N, E>` with capability traits, binding muniment's blobs and codicil's log into an app-agnostic substrate. Carries the lineage layer (`chartulary::stemma`). |
| [`scholia`](crates/scholia) | The RDF projection over a chartulary graph's semantic ring: addressed, labeled nodes become RDF subjects. |

Floor, log, graph, projection. Each crate publishes individually under its
own name; this workspace is their shared home. Per-crate design docs live
beside each crate (for example `crates/muniment/design_docs/`).

Formerly four sibling repos (muniment, codicil, chartulary, scholia), merged
2026-07-21 with histories preserved. Mere's private local-memory lane carried
the eidetic name first and continues as `mere-eidetic`.

## License

MIT OR Apache-2.0.
