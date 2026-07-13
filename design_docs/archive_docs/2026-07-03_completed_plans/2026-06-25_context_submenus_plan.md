# Context-menu nested submenus

**Date**: 2026-06-25
**Status**: Implemented (logic + DOM tested); visual/interaction feel needs a GUI verify pass.
**Scope**: A depth-1 submenu primitive for the meerkat context menu, plus refactoring the three
flat-list "pickers" (relation kinds, layout strategies, shellbar edges) onto it.

## Why, and at which layer

The relation-kind picker (audit A3) shipped as 11 flat "Relate as <kind>" rows because `ContextItem`
was a flat `Vec` with no nesting. Three such flat pickers existed (relate, layout, shellbar). The
question was where a real submenu belongs across the stack.

**Answer: the geometry primitive already exists in `xilem-serval`, and we now use it.**
`repos/genet/components/xilem-serval/src/overlay.rs` provides `overlay_at(x,y,content)`,
`anchor_point(trigger, popup, Placement)`, and `Placement` — with `Placement::RightOf` documented
verbatim as "(a submenu)". The mechanism (anchored floating panel geometry) is cross-surface and
engine-adjacent, so it lives in `xilem-serval`; meerkat owns the menu *content tree* + interaction;
genet (the engine) provides the substrate. `mere-domain` is not involved.

**Stacking is no longer a problem.** The `overlay.rs` module docs are stale: they say genet has no
z-index and an overlay must be "last among its siblings". Genet Stage 7 landed full CSS 2.1
Appendix E stacking (`genet-layout/paint_stacking.rs`): every `position: absolute` box auto-lifts
above in-flow content regardless of document order. So nested panels just work; no z-index juggling.

**Nested-submenu UI is greenfield on top of the primitive.** Nothing in genet or mere had a
submenu/flyout/popup-stack abstraction. `ContextItem` is flat; the menu is host-painted via the
genet DOM. So this builds the submenu layer in meerkat over `anchor_point`.

## What landed

Depth-1 only (all three substitutes are one level). Files in `crates/meerkat/src/`:

- **lib.rs** — `ContextItem.children: Vec<ContextItem>` (+ `with_children` / `has_submenu`);
  `ContextMenu.submenu: Option<SubmenuState { parent, selected }>`; `ContextAction::OpenSubmenu`
  sentinel. Chrome methods: `open_submenu` / `close_submenu` / `enter_submenu` / `escape_context_menu`,
  and submenu-aware `step_context_menu` + `run_context_selection` (decide-under-immutable-borrow,
  then act).
- **views.rs** — `menu_item_view` renders a parent row with a `›` glyph + `data-submenu=<i>` attr +
  (when open) a `context-submenu-anchor` class. `context_menu_view` emits the root panel plus, when
  a submenu is open, a `.context-submenu` panel of the parent's children, wrapped in a positioned
  `.context-menu-layer`. Child rows are leaves with their own `context-subitem-active` class and no
  `data-submenu` (keeps the depth-1 invariant and the scroll seams separate).
- **render.rs** — anchors the `.context-submenu` panel off the parent row's rect via
  `xilem_serval::anchor_point(trigger, popup, RightOf)`, flips to `LeftOf` (clamped >= 0) on right
  overflow, and scrolls the active child into view within the panel.
- **input.rs** — `submenu_parent_at(x,y)` (a deterministic hit-test to a `data-submenu` attr) lets
  the left-press gate open/toggle a parent's submenu instead of dismissing, independent of dispatch
  timing. Keyboard: ArrowRight = `enter_submenu`, ArrowLeft = `close_submenu`, Escape pops one level.
- **menus.rs** — the relate (2-node), layout (empty-canvas), and shellbar-move builders now emit a
  `with_children` parent; `rebuild_context_menu` resets `submenu`; `OpenSubmenu` drains as an early
  no-op.
- **main.rs** — `.context-submenu`, `.context-menu-layer` (positioned), `.context-subitem-active` CSS.

## Verification

- Unit-tested: the state machine (open/close/step/run/enter/escape, expand-navigate-pick,
  collapse-one-level) and the DOM rendering (a `.context-submenu` panel appears when open). 87 lib +
  158 bin tests green.
- Adversarially reviewed by a 4-agent workflow. It found 2 bugs (both fixed): the `.context-menu-layer`
  wrapper was unpositioned and shifted the whole menu down by the toolbar height (one reviewer
  confirmed it with a genet-layout repro) — fixed by making the wrapper `position: absolute; top:0;
  left:0`; and the root scroll-into-view latched onto a mouse-opened submenu's active child — fixed by
  the distinct `context-subitem-active` class + a dedicated submenu scroll block. Plus 5 nits/risks
  fixed (Enter/ArrowRight highlight parity, OpenSubmenu diagnostic noise, LeftOf left-edge clamp,
  click-to-toggle on an open parent, Escape-path divergence).
- **Not yet verified (needs a GUI run):** pixel placement of the flyout, the live mouse hover/keyboard
  feel, and the deferred risks below. No headless GUI harness was run.

## Deferred (known limitations)

- **No hover-open.** Submenus open on click or ArrowRight, not on hover-with-delay. A follow-up; the
  cursor-move handler + an `Instant`-timed `submenu_hover` field is the mechanism.
- **Submenu mis-anchors if the *root* menu is scrolled.** `render.rs`'s `row_y` sums raw fragment
  locations and does not subtract the root menu's paint-time scroll offset. Narrow: a root menu with a
  submenu parent is usually short. Fix: subtract the root's `chrome_scroll` target from `row_y`.
- **Mouse hit-test after keyboard-scroll.** `submenu_parent_at` / `chrome_click` use unscrolled
  offsets for the menu (pre-existing for the flat menu); a mouse press after keyboard-scrolling past
  the fold can mis-resolve. Mirror the menu's scroll into the hit-test offsets, or reset menu scroll
  on pointer move.
- **Depth-N.** The model is depth-1 by convention (child rows render as leaves). Deeper nesting would
  need the child render path + `submenu_parent_at` to carry a path, not a single index.

## Follow-on: genet capabilities the host underuses

Surfaced while doing this work (see the separate sweep): meerkat hand-rolls `position: absolute`
inline strings for the menu/palette/etc. instead of `xilem-serval::overlay_at`, and the overlay
module's own docs still describe the pre-z-index stacking model. These are not bugs, but they are the
host not yet using the primitive the genet-as-host track built for it.
