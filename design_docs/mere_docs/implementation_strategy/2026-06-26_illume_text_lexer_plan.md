# Illume: the text lexer / highlighter, its tinct + serval pairing, and the omnibar legibility goal

**Date**: 2026-06-26
**Status**: Planning. Captures decisions made across the 2026-06-26 session: promote
the knot editor's highlight core to a standalone sibling crate (**illume**), pair it
with tincture (publishing as **tinct**) for themed colours and serval's styled field
for rendering, and aim the whole thing at making mere/meerkat fully operable and
legible from the omnibar. Two pieces already shipped (see Build sequence); the rest is
to address point by point.

## The idea

The highlight core we built for the knot editor (`crates/inker/knot-editor`: jotdown
djot structure + the pluggable `InjectionLexer` registry + the `logos` language pack)
is not editor-specific. It is a general capability: take text, run a lexer, get
`(range, kind)` spans. **illume** is that core promoted to its own name and scope: a
pure-Rust, wasm-safe lightweight text lexer and highlighter. It serves the knot editor,
and equally the omnibar, comms, labels, and any other host text surface.

To enrich all host text (not just code/djot), illume grows a **prose-entity pass**:
URL, `@mention`, `#tag`, email, alongside the djot and code passes, and `SyntaxKind`
gains those entity kinds. A small configurable front (the "mere lexer") picks which
passes run per surface: the omnibar lights up URL parts and command tokens, comms
lights up entities plus light djot, the editor runs the full thing.

## Why promote (and what stays home)

The test (Mark's, the one the wgpu siblings hold to): a crate promotes when it depends
only on crates.io plus a minimal portable patch, never on the personal mere workspace,
and fills a niche another host or forker would want.

- **illume promotes.** It depends only on jotdown + logos. Its niche is the lightweight
  slot that tree-sitter (C / wasmtime) and syntect (Oniguruma) do not fill cleanly for
  wasm: a small, pure-Rust, wasm-safe highlighter with a pluggable lexer registry.
- **`knot-editor-host` stays home.** It reuses the app's own parsers (cssparser,
  html5ever, boa_parser, pulldown-cmark) and deps inker + nematic, so it is
  host-coupled by design, the precision layer over illume's portable floor. (It may be
  renamed `illume-host` for clarity; mere-internal, so deferred.)

This is the pressure-vessel pattern (as run with Strophe): mere and meerkat incubate the
hard part, the host-agnostic core promotes to a crates.io-only sibling, the host-coupled
glue stays.

## Architecture: three separable pieces

The highlight path is three crates that each do one thing, plus a host bridge:

1. **illume (the lexer)** — text → `(range, SyntaxKind)` spans. Pure Rust, crates.io-only.
2. **tinct (the palette)** — `SyntaxRole` → a contrast-gated colour, derived from the
   theme seeds. Lives in tincture (publishing as tinct); the `syntax` module shipped
   this session. serde-only, toolkit-agnostic, lexer-agnostic.
3. **serval styled field (the renderer)** — `(range, css/class)` → styled `<span>` runs
   in the edit surface. `xilem_serval::styled_textarea`, shipped this session. Generic,
   no djot knowledge.

The **host (meerkat)** bridges them: it maps each illume `SyntaxKind` onto a tinct
`SyntaxRole`, looks up the colour in the derived `SyntaxPalette`, and hands serval the
styled ranges (as classes against a host stylesheet, so the colours stay themeable).

**The key seam: `SyntaxKind` → `SyntaxRole`.** illume's kinds are about *what was
lexed* (finer, lexer-specific); tinct's roles are a small canonical set about *what
colour*. The host owns the map between them, so tinct never depends on illume (it can
colour any highlighter's output) and illume never depends on tinct (it just emits
kinds). That decoupling is deliberate and is what keeps both promotable.

## The omnibar / TUI-comfort goal

The motivating north-star (Mark's): mere and meerkat should feel comfortable and
manipulable from the omnibar, the way a TUI power-user feels at home. That reframes the
lexer: it is not an editor feature, it is **the legibility layer for a keyboard-driven
interface**. When the omnibar is the command line, the query prompt, the script line,
and the note entry, illume is what makes all of it readable as you type (URL parts,
command and args, query, inline script, djot).

This sharpens the existing control rule. Today: "every control operable in the active
input mode." The TUI version: "the keyboard user can drive *and read* the whole thing
from the omnibar, expressively." Navigate, manage the graph, run commands, run scripts,
open and edit notes, all typed, all legible.

Gaps to close for that comfort (named, not yet built): a shared grammar across the
omnibar so commands, paths, queries, and scripts lex as one highlighted syntax;
completion everywhere, not just `[[` and `/`; and a typable graph query. The cheapest
first payoff is the lexer highlighting the omnibar input live, which makes the omnibar
illume's first non-editor consumer.

## Naming

- **illume**: the lexer/highlighter crate. Chosen 2026-06-26 (the poetic verb, to
  illuminate). `illume` is taken on crates.io only by an abandoned non-entry; reclaim
  or a near-name is a publish-prep concern. The `limn` + tincture "illuminated
  manuscript" pairing was the runner-up.
- **tinct**: tincture's published name. `tincture` is already taken on crates.io (a
  different OKLCH colour crate, 1.0.0), so tinct is necessary, not just shorter. It is a
  path dep today, so the rename (package name + Mere's `register-theme` + Woodshed's
  `audio_widgets`) is a small coordinated pass deferred to publish-prep.

## Build sequence (the point-by-point)

1. **tinct `syntax` module — DONE** (tincture `03661ce`). `SyntaxRole` (16 roles) +
   `derive_syntax_palette`: accents fanned off the brand primary in OKLCH, contrast-gated
   to WCAG 4.5 against the surface, muted roles on the dim-text tier. This is the #1 fix
   (colours derived, never hardcoded).
2. **serval `styled_textarea` — DONE** (serval `6a3ceace`). Per-range styled `<span>`
   runs, innermost-wins flatten, caret/preedit/ghost intact, fully generic.
3. **illume**: rename `knot-editor` → illume; add the prose-entity passes (URL /
   mention / tag / email) and their `SyntaxKind`s; expose the `SyntaxKind` → `SyntaxRole`
   map (or document it as the host's to own).
4. **#2**: fold serval's `styled_body` into one style-aware field body (plain text =
   the empty-styles case), so there are not two field bodies to keep in sync.
5. **The bridge** (meerkat): illume spans → `SyntaxRole` → tinct colour → serval styled
   field, as a themeable host stylesheet. Wire the **editor** first (the visible payoff),
   then the **omnibar** (illume's first non-editor consumer).
6. **#3**: fold `KnotEditor` into the stateless-deriver shape (registry + engine, methods
   over text; the host's `TextInput` is the single buffer), so the model is not an
   orphaned second copy of the text.
7. **Promotion (later)**: extract illume to a sibling repo, do the tinct rename + the
   consumer updates, publish. Pressure-vessel graduation, once the core is proven.

## Decisions

1. **Colours are derived, not hardcoded** (#1). The host maps `SyntaxKind` → `SyntaxRole`
   → `tinct::derive_syntax_palette`, wired through Mere's `register-theme` into the
   stylesheet serval applies. No parallel palette; tracks light/dark and any reseed.
2. **Highlight runs carry classes, not inline CSS.** Themeable through one sheet, the
   serval/stylo way, so user/mod themes override.
3. **One style-aware field body** (#2), not a styled fork of the plain one.
4. **`KnotEditor` is a stateless deriver** (#3), the host owns the buffer. The portable
   derivations (highlight / outline / folds / render) take text; the registry is built
   once and held, fixing the per-keystroke rebuild.
5. **The `SyntaxKind` → `SyntaxRole` seam is the host's**, keeping illume and tinct
   independent of each other.

## Cross-references

- [djot editor + knot nodes plan](2026-06-24_djot_editor_knot_nodes_plan.md): the editor
  that illume's highlight core grew out of; its Phase 2/3 highlight + injection work is
  illume's seed.
- [borrowed-ideas brief](../research/2026-06-25_borrowed_ideas_brief.md): the `=query`
  block and the broader graph-as-text directions the omnibar grammar feeds into.
- [command registry / configurable menus plan](2026-06-21_command_registry_configurable_menus_plan.md):
  the `>` shell + `ActionRegistry` that the omnibar legibility goal sits on top of.
- tincture (publishing as tinct): the `syntax` module, the colour contract.
- serval `xilem-serval`: `styled_textarea`, the renderer.

## Progress

- **2026-06-26, plan written + first two pieces shipped.** Captured the session's
  decisions: illume (the promoted lexer name), tinct (tincture's necessary published
  name), the three-piece architecture (illume lexer / tinct palette / serval renderer)
  with the host-owned `SyntaxKind` → `SyntaxRole` seam, the omnibar-as-legibility-layer
  goal, and the #1/#2/#3 resolutions. Shipped tinct's `syntax` module (perceptual
  contrast-gated highlight palette, tincture `03661ce`) and serval's `styled_textarea`
  (serval `6a3ceace`). Remaining points 3-7 to address in order.
