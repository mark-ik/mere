# mere-frame

Frame domain module for the [mere](https://crates.io/crates/mere) browser.

A **frame** is the savable layout of resizable panes that the user
arranges. Each leaf pane shows one of the other mere-domain concepts —
workbench (tile area), orrery (graph), gloss (peripheral content
strip), apparatus (peripheral system inspector), system (settings), or
a custom content slot. Splits between panes are oriented horizontally
or vertically with a ratio.

This crate owns:

- The portable [`FrameLayout`] / [`PaneNode`] / [`PaneContent`] types
- A [`project_frame`] function that produces a uxtree subtree describing
  the layout
- Serde derives so frames are savable + restorable

## Relationship to other crates

- **NEW model**, distinct from `platen::FrameState` (flat list of
  `PaneBinding`s) which is the legacy graphshell-shell-state shape. They
  coexist while migration is in progress; eventually `platen::FrameState`
  is replaced by `mere_frame::FrameLayout` or `platen` consumes
  `FrameLayout` directly.
- The host (`host`) renders the layout: walks the tree, emits gpui
  split panes, and stitches each leaf pane's mere-domain subtree
  (workbench / orrery / etc.) into the application uxtree.
- Pane content is identified by [`PaneContent`] tags rather than typed
  references so this crate stays decoupled from workbench / orrery /
  gloss / apparatus.

## Status

Pre-1.0. Initial layout types, projection to uxtree, serde
serialization. Real layout-mutation operations (split, drag, resize)
land with the host's interaction layer.
