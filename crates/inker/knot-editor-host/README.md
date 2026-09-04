# knot-editor-host

Host-side reuse-lexers for the knot editor: precise inner-language injection
over parsers the workspace already ships, layered as overrides on the
portable [illume](https://crates.io/crates/illume) pack. The editor model
ties source text to the rendered preview through the same engine path the
rest of the family uses (inker's `EngineDocument` + nematic's
`DjotKnotEngine`).

> **Home:** [`merely-made/mere`](https://github.com/merely-made/mere), at
> `crates/inker/knot-editor-host`. It went to genet with the engine-management
> family on 2026-07-10, came back on 2026-09-03 with the platform boundary
> move, and moved beside the rest of the inker family on 2026-09-04 when Knot's
> own sources left this repository. It is integration code for the inker
> family, not Knot product source: Knot Editor is the independent
> [`merely-made/knot-editor`](https://github.com/merely-made/knot-editor)
> repository, and consumes this crate. Markdown via pulldown-cmark today;
> CSS/HTML/JS/Turtle to follow.
