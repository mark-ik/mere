# gloss

Gloss domain module for the [mere](https://crates.io/crates/mere) browser —
the peripheral content-commentary strip.

In scholarly terms, a *gloss* is a marginal annotation that explains or
contextualises the central text. mere's gloss panel is the same idea
applied to whatever's in the active workbench / orrery: an outline,
linked references, related-content surface, annotations.

## What it produces (v0)

[`project_outline`] takes an `inker::EngineDocument` and produces a
uxtree subtree whose children are the document's headings (preserving
level), suitable for use as a table-of-contents commentary panel.

```text
gloss → outline (Group "Outline")
  ├─ Heading L1 "Introduction"
  ├─ Heading L2 "Background"
  ├─ Heading L1 "Architecture"
  └─ ...
```

## Future shapes

- Annotations / highlights surfaced from local memory (eidetic) once
  that lane lands.
- Cross-document references (graph edges from the active node) — would
  consume orrery state.
- Citation apparatus (notes, footnotes, bibliographies) once an
  EngineDocument variant carries them.

## Status

Pre-1.0. Initial outline projection. Other commentary shapes plug in
as needed.
