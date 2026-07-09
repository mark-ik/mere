# Virtualized line-based editor buffer (the IDE-editor ladder, rung 3)

**Date**: 2026-07-09
**Status**: design, pre-build. The rung-3 *infrastructure* (arrangement leaf +
`VirtualWindow`) is already built in serval; this plan is the editor that consumes it,
so the knot editor can fold sections, show a gutter, and scale to large files. The one
gating decision (the text-editing layer) is called out below and wants a call before the
large build. Grew out of the [djot editor plan](2026-06-24_djot_editor_knot_nodes_plan.md)
Phase 3 (folds), which is blocked on this.

## Why (what the textarea cannot do)

The knot editor today is a single `<textarea>` (`xilem_serval::styled_textarea` over a
`TextInput`), laid out by serval-layout/parley as one element. That is why it edits so
cheaply: `TextInput` + parley give **caret, anchor selection, IME, word motion, soft-wrap
vertical nav, and click-to-place for free**. But a textarea is one element the engine lays
out whole, so it cannot:

- **Fold** — hide a heading's section while keeping the buffer intact (line-hiding needs a
  display that differs from the edit buffer, with caret mapping between them);
- **Gutter** — place line numbers / fold arrows / diagnostics aligned to each line as
  sibling content;
- **Scale** — a 10k-line file lays out entirely; only visible rows should materialize.

The chisel catalog's "djot-to-IDE editor ladder" names this: rung 1 highlighting (done,
serval `highlight`), **rung 2 gutter**, **rung 3 virtualized buffer**, rung 4 structure
decorations. Folds sit on rungs 2–3.

## What already exists (serval-side, built)

- `xilem_serval::arrangement(width, height, children)` — a `position: relative` container
  claiming an explicit content extent (keeps the scrollbar honest while only visible rows
  are DOM).
- `xilem_serval::placed(placement, child)` / `placed_with(..)` — a child absolutely
  positioned at a `Placement` (x/y/z); a re-placement is an attribute diff on a retained
  element, not a rebuild, so hit-test / focus / a11y ride through.
- `chisel::VirtualWindow` — fixed-height row virtualization: `range()` (the visible row
  band), `total_height()` (the honest scroll extent), from `(total_rows, row_height,
  viewport_height, scroll)`.

So the **virtualization mechanism is done**. No meerkat consumer uses it yet; the editor
would be the first.

## The architecture

A line-based editor over the arrangement:

1. **Buffer stays the source of truth.** `knot_source` (a `TextInput` today; a rope at
   scale, per the djot plan's deferred Phase 6) holds the full text. Lines are derived.
2. **Visual-line model.** Split the buffer into lines; apply the fold state (collapsed
   heading sections omit their lines) to get the **visible rows**. `illume::folds(src)`
   already yields the foldable heading regions.
3. **Virtualized render.** `arrangement(width, total_height)` inside an `overflow: scroll`
   box; for each row in `VirtualWindow::range()`, a `placed(y = row * row_height, ..)` line
   element carrying the line's highlighted spans (illume, per line) plus a gutter cell
   (line number, and a fold arrow on foldable headings).
4. **Fold controls.** A fold arrow is a disclosure control in the gutter (a Unicode ▸/▾ in
   a DOM span for v1; a chisel Path-A glyph later for crisp animated vectors). Clicking it
   is a real DOM click (routable, unlike a rasterized tile), toggling the heading's fold
   state; the model re-derives visible rows and the arrangement re-places them. Line-hiding
   falls out for free: folded lines are simply not placed.

## The gating decision: the text-editing layer

Everything above is the **display**. The hard, large part is **editing over a line model**,
because the textarea's free caret/selection/IME/motion do not carry over to N separate line
elements. Three ways, in rising cost and fidelity:

- **A — Hidden-textarea input (reuse `TextInput`).** Keep a real (offscreen / overlaid,
  1-line-tall or full) textarea that owns input: keystrokes, IME, clipboard, and the caret
  byte all stay `TextInput` + parley. The arrangement is a **display layer** synced to the
  buffer; the caret is drawn over the placed lines from the buffer caret. This is how
  CodeMirror/Monaco work (a hidden input + a custom render). Reuses the editing we have;
  the sync + caret-draw + fold/caret interaction (a caret entering a folded region
  auto-unfolds) is the new work. **Lowest cost, good fidelity.** Recommended starting point.
- **B — Full custom editing over lines.** Reimplement caret positioning, cross-line
  selection, IME, and motion natively over the placed line elements. Highest fidelity and
  control; it is essentially building a code editor's editing core from scratch (the bulk
  of Monaco/CodeMirror). **Highest cost; a multi-slice project on its own.**
- **C — Read-only virtualized view first.** Ship the virtualized line render + folding +
  gutter as a **read-only source view** (a foldable, navigable rendering of the source),
  keeping the textarea as the editor for now. Delivers folding + gutter + scale on the
  *display* without touching editing; editing over the line model (A or B) comes later.
  **Lowest cost, no editing risk; the display and the editor are two surfaces until merged.**

The recommendation is **C then A**: land the virtualized display (folding, gutter, scale)
read-only first to prove the mechanism and get folding into users' hands, then adopt the
hidden-textarea input so it becomes the live editor without a from-scratch editing rewrite.
B stays the door if pixel-exact editing control is ever wanted.

## Phased plan

- **P1 — read-only virtualized line render.** `arrangement` + `VirtualWindow` over the
  source lines: highlighted lines placed at row Ys, honest scroll extent, only visible rows
  materialized. No gutter, no folding yet. Proves the mechanism as the first meerkat
  arrangement consumer. Done-when: a long source renders line-by-line through the
  arrangement, scrolls correctly, and materializes only the visible band.
- **P2 — gutter + fold controls + line-hiding.** A gutter column (line numbers + a fold
  arrow on `illume::folds` headings); clicking folds/unfolds; the visible-row model omits
  collapsed sections. Done-when: clicking a heading's arrow collapses its section in the
  render and the scroll extent shrinks.
- **P3 — hidden-textarea input (option A).** Wire the buffer's `TextInput` input +
  drawn caret over the placed lines, so the virtualized view becomes the live editor; a
  caret entering a folded region auto-unfolds. Done-when: typing / caret / IME / selection
  work on the virtualized editor with folds.
- **P4 — migrate + retire.** Make the virtualized editor the knot editor's edit surface,
  retire the plain `styled_textarea` path (or keep it as a fallback). Promote the reusable
  pieces (the line-render view, the gutter, the fold-glyph) to serval so every host — an
  IDE pane, Isometry's log/console, a Strophe list — gets a virtualized code view.
- **Deferred:** the chisel Path-A fold glyph (polish over the Unicode arrow); a rope buffer
  for very large files (djot plan Phase 6); diagnostics dots in the gutter.

## Decisions

1. **The infra is serval's `arrangement` + `VirtualWindow`** — built; the editor consumes
   it, does not rebuild it.
2. **Display before editing (C then A).** Land folding/gutter/scale read-only, then adopt
   hidden-textarea input; avoid a from-scratch editing rewrite (B) unless proven necessary.
3. **Fold arrow is a DOM disclosure control** (Unicode v1), so its click routes through the
   chrome hit-test (a rasterized tile's would not). Chisel Path-A glyph is deferred polish.
4. **Fold state is view state**, not buffer state — the buffer stays full and editable; the
   render omits folded lines. Undo/save never see the fold state.

## Risks

- **The editing-layer rewrite is the 90%.** Options B is a code-editor-from-scratch; even A
  (hidden textarea) is real work (input sync, caret draw, fold/caret interaction). The
  display (P1–P2) is comparatively cheap; scope expectations to that split.
- **Soft-wrap vs fixed row height.** `VirtualWindow` assumes a fixed row height; a
  soft-wrapped line occupies multiple visual rows. v1 can disable soft-wrap in the
  virtualized view (a code editor convention) or measure per-line heights (loses the simple
  fixed-height window). Decide at P1.
- **Two surfaces during C.** Until P3/P4, the virtualized view and the textarea editor
  coexist; keep them from drifting (one buffer, the view is derived).

## Cross-references

- [djot editor plan](2026-06-24_djot_editor_knot_nodes_plan.md): Phase 3 folds/gutter,
  blocked on this; the illume `folds`/`outline`/container-tree the model reads.
- serval `docs/2026-07-08_chisel_widget_catalog.md`: the editor ladder + the arrangement /
  `VirtualWindow` tier-3 mechanism this consumes.
- [illume text lexer plan](2026-06-26_illume_text_lexer_plan.md): the per-line highlight +
  `folds` the render uses.

## Progress

- **2026-07-09, design written.** Confirmed the rung-3 infra (arrangement + `VirtualWindow`
  + `placed`) is built serval-side with no meerkat consumer yet; framed the editor over it;
  surfaced the editing-layer as the gating decision (A hidden-textarea / B full custom / C
  read-only-first) and recommended C→A. No code yet — the editing-layer call gates P3+.
