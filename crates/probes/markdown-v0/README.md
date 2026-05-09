# probe-markdown-v0

**Question:** can an `inker::EngineDocument` produced by
`nematic::markdown::MarkdownEngine` be rendered end-to-end through
[Glass-HQ/gpui](https://github.com/Glass-HQ/gpui) without engaging
`platen`, `verso-tile`, or `PlatformSurface`?

## What this probe does

1. Parses a hardcoded markdown sample with `nematic::markdown::MarkdownEngine`,
   producing an `inker::EngineDocument`.
2. Bootstraps a Glass-HQ/gpui application and opens a window.
3. Walks `EngineDocument.blocks` and renders each `DocumentBlock` as a gpui
   element using gpui's built-in text and layout primitives.

It deliberately **does not** use parley, `PlatformSurface`, or any
mere-workspace state machinery. Layout is gpui-native; the inline-span
styling is flattened to plain text.

## What this probe surfaces

- Whether the `EngineDocument` shape composes cleanly into gpui's
  element/`Render` model, or whether the bridge wants its own crate.
- What's awkward about `InlineSpan` rendering when flattened
  (emphasis/strong/code/link styling lost in v0) — flag for a v0.5 that
  preserves inline styling using gpui's text-style children.
- The first read on whether gpui's text system carries us, or whether
  parley needs to come in earlier than expected.

## Findings (run 2026-05-08)

- **Reflow + resize work cleanly.** Block-level layout via gpui's
  div/flex behaves as expected; the document re-wraps to window width.
- **Heading scale, code-block styling, blockquote rule, hr all render
  correctly** without parley.
- **List items rendered empty** — initially blamed on flex sizing, but
  the actual bug was upstream in `nematic::markdown`. pulldown-cmark
  v0.13 emits `Text` events directly under `Tag::Item` for tight lists
  (no enclosing `Tag::Paragraph`), and the converter's `Tag::Item`
  handler didn't open an inline frame, so spans were silently dropped
  by `push_inline`. Existing list tests only checked `items.len()`,
  never the items' contents. **Fixed in `nematic::markdown` (Item now
  opens an inline frame; on close, any captured spans become a
  `Paragraph` block in the item's children).** Two regression tests
  added (`tight_list_items_carry_paragraph_content`,
  `loose_list_items_preserve_paragraph_blocks`).
- The probe-side `flex_1()` on the list body div is still correct
  flex-layout hygiene and was kept.
- **No text selection / I-beam cursor.** `div().child(text)` produces
  non-interactable text. Selection + caret want gpui's `Text` element
  with interactability — v0.5.
- **Inline styling is flat plain text in v0.** Emphasis, strong, code,
  link don't differentiate visually. Per-span styled rendering wants
  gpui's text-style children — v0.5.
- **Window non-focus state shows a bright red title bar** (visible in
  the second screenshot when the window is unfocused). Not from this
  probe — that's gpui's default non-focus chrome on Windows.

## What this probe explicitly defers

- Parley layout (probe-markdown-v1).
- `PlatformSurface` registration / secondary surfaces (probe-surface-v0).
- `platen` projection consumption (probe-platen-v0).
- File-loading / I/O — sample is a `const &str`.
- Cross-platform anything — Glass-HQ's mac path is the only path with full
  surface support today; bring-up on Windows/Linux happens after the
  surface probe.

## Run

```pwsh
cd crates/probes/markdown-v0
cargo run
```

Glass-HQ/gpui must be checked out at `repos/glass-gpui/` (sibling to
`repos/mere/`).
