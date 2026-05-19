# uxtree

Portable accessibility / automation tree for the
[mere](https://crates.io/crates/mere) browser.

## What it does

Projects mere structural elements (`inker::EngineDocument` blocks,
eventually `platen` workbench frames + `verso-core` panes) into a tree of
[AccessKit](https://accesskit.dev) `Node`s with **stable, deterministic
IDs**. The same projection feeds:

- **a11y**: screen readers and OS accessibility APIs through AccessKit's
  platform adapters (Win32 UIA, AT-SPI, NSAccessibility, accesskit_winit
  for in-process).
- **automation**: tooling that wants to query / drive the UI by stable
  identity — integration tests, scripted scenarios, record/replay.
- **inspector / debug overlay**: any consumer that needs a structured
  view of the rendered surface without poking at gpui internals.

## What it does *not* do

- No bounds. Bounds come from rendering; the host fills them in after
  layout. uxtree produces logical structure, not screen coordinates.
- No event synthesis. The host owns input.
- No host-specific code. uxtree depends on `accesskit` and `inker`, full
  stop. gpui integration lives in the host crate.

## Stable IDs

Every projected node gets a deterministic `accesskit::NodeId` derived from
a domain path:

- An `EngineDocument` originates a subtree rooted at `engine:{address}`.
- Each block within the document gets a path like
  `engine:{address}#blocks/2/list/1/item/0/paragraph` — index-based,
  stable across renders of the same document.
- The path is hashed (FxHash) into a `u64` and wrapped as
  `accesskit::NodeId`.

The same document always produces the same IDs — automation can pin to
a specific node without relying on bounds or render order.

## Status

Pre-1.0. Initial projection covers `EngineDocument` blocks and inline
spans that map cleanly to AccessKit roles. Mere-specific structural
nodes (Tile, Pane, Workbench, View) project as `Role::Group` until the
projection layer learns their domain semantics.
