# nematic

`nematic` is the smolweb engine for the
[mere](https://crates.io/crates/mere) browser. It renders content from
protocols where layout cost is minimal and the document is mostly text:
Gemini, Gopher, Spartan, Finger, plain text, Markdown, static-HTML files,
RSS/Atom feeds, and local files.

For fullweb rendering (CSS, JS, embedded media), mere routes through a
serval, a system webview, or nematic can attempt to simplify well-structured
fullweb content into a reader mode presentation.

## Naming

*Nematic* is borrowed from liquid-crystal physics: a nematic phase has
*orientational* order without *positional* order; rod-shaped molecules all
point the same way but otherwise flow freely. Light passes through aligned
nematic crystals coherently, and that's the basis of LCDs.

If the web is a lenticular soup of pixels, then nematic is the engine that
tries to align the molecules and let the light through.

## What's in the crate

Pre-1.0 placeholder. Currently exposes only `VERSION` and `STAGE` constants.
Concrete protocol viewers (Gemini parser, Gopher menu renderer, Markdown
layout, feed reader, file viewer) land in subsequent slices.

## How it relates to other workspace crates

nematic is the engine that [`inker`](https://crates.io/crates/inker)
dispatches to for smolweb URI schemes; rendered output is presented through
[`verso-tile`](https://crates.io/crates/verso-tile)'s surface contracts.

```text
   inker.routing
      │ EngineRouteDecision
      │ engine_id ∈ { nematic.smolweb, nematic.file }
      ▼
   nematic
      │ rendered content
      ▼
   verso-tile (CompositedTexture surface)
```

- [`inker`](https://crates.io/crates/inker) — references nematic by engine
  ID. The default policy routes `gemini`, `gopher`, `finger`, `spartan` →
  `nematic.smolweb`, and `file` → `nematic.file`.
- [`verso-tile`](https://crates.io/crates/verso-tile) — nematic's output is
  presented as a `CompositedTexture` surface; verso-tile owns the surface
  lifecycle.
- [`mere`](https://crates.io/crates/mere) — composes nematic into the
  product.

## Status

Pre-1.0. The crate name is reserved and the engine-ID slots are wired into
inker's default policy. Implementation is in progress within the
[mere workspace](https://github.com/mark-ik/mere).

## Fun Fact

My first idea for the crate's name was middlenet, intended to encapsulate
the smolweb and well-structured web content. This notion of a browser
that could manage whatever protocol it was offered calls to mind a quote
from the game Elden Ring:

"Heresy is not native to the world; it is but a contrivance.
All things can be conjoined."

Accordingly, another possible name was "miriel," and a fourth, "turtlepope."
All protocols *can* be conjoined?

## License

MPL-2.0.
