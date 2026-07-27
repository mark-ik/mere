# Knot

Knot is Mere's files-in-place authoring port. It owns local document and vault
truth behind a Graphshell endpoint, so a host can mount projections without
owning the source files or vault keys.

The port now carries the full local ladder:

- `DirectorySource` discovers a real directory under a configurable ignore
  policy;
- chartulary containers keep `file:` references, titles, media types, and
  facets while file bytes remain on disk;
- filesystem identity preserves a container and its facets across rename;
- a recursive OS watcher collapses queued events into one attributed Servitor
  journal transition;
- revoking the watcher grant freezes accepted directory state without stopping
  the endpoint, and restoring it permits the next refresh;
- snapshot and resume requests expose accepted revisions;
- `knot.file` and `knot.note` are data-defined content classes backed by
  Eidetic-compatible facet schemas;
- a Personae-backed vault keeps keys and authored bodies endpoint-side;
- Sibylla search spans disk and vault under separate Servitor grants, with its
  vault index sealed;
- Stickleback sync carries sealed events between admitted device writers over
  real p2panda LogSync and refuses unresolved same-document writer conflicts;
- `.knot`, Markdown, Djot, and JSON codecs provide fixed-point writing and
  caller-selected Save As without rewriting untouched files;
- `KnotEditor` uses Cambium's source buffer and command path while
  `knot-editor-host` derives highlights, outline, folds, and preview;
- `knot_endpoint [directory]` serves the real folder, while no argument serves
  the deterministic K0 fixture.

The unresolved boundary is concurrent editing of one document by multiple
devices. The sync projection reports it instead of choosing a winner. K7's
clean-checkout receipt also waits on the completed Cambium primitive changes
in the Genet working tree being committed to `main`. See the
[Knot port plan](../../design_docs/mere_docs/implementation_strategy/2026-07-25_knot_port_plan.md).
