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
- **2026-06-26, point 3 done (illume born).** Renamed the `knot-editor` crate to
  **illume** in place (dir + package name + the host crate's dep/use; meerkat untouched,
  since its `knot_editor_*` identifiers are local names, not crate uses; mere `bcaf834`),
  and added the prose-entity pass: a `logos` lexer (`entity.rs`) emitting URL /
  `@mention` / `#tag` / email spans for *any* text, with the matching `SyntaxKind`s (mere
  `6d0a2ae`). illume now enriches the omnibar and comms, not just the editor; 32 tests
  green. Point 3(c), the `SyntaxKind` → `SyntaxRole` map, is the host's and lands with
  the point-5 bridge. Next: point 4 (unify serval's field body).
- **2026-06-26, point 4 done (one field body).** Unified serval's field rendering: a
  single `field_children(input, styles)` in `styled_field.rs` builds the field's
  children (unstyled runs as text nodes, styled runs as `<span class>`) with the
  caret / preedit / ghost splice; the plain `text_field` / `textarea` pass empty styles,
  so there is one body, not a styled fork. `TextField` is now the Vec-children view type
  and `field_body` shrank to a 3-line delegator (controls.rs lighter). Styled runs carry
  a **class** (not inline CSS), baking in the #1 themeable decision. `StyledField`
  dropped (it is `TextField` now). serval `3abaad8`; 75 tests pass, including the existing
  `text_field` behaviour tests over the new body. Next: point 5 (the bridge: illume spans
  → `SyntaxRole` → tinct colour → the styled field, editor first then omnibar).
- **2026-06-26, point 5a (bridge brain) + tincture pinned.** Resolved the tincture gate:
  github `main` already had the `syntax` module (`03661ce`), but mere's lock kept
  resolving the branch to the pre-syntax `df4c898`; a non-precise `cargo update` did not
  stick, so `cargo update -p tincture --precise 03661ce` pinned it. Added illume as a
  meerkat dep and built the bridge brain (`knot_highlight.rs`): `syntax_role` maps each
  `illume::SyntaxKind` to a tinct `SyntaxRole`, `role_class` names its `syntax-*` class,
  and `knot_styles(text)` runs illume's highlight + entity passes into a `Vec<StyleRange>`
  of (range, class). mere `39cec94`; 2 tests green (a sample yields heading / mention /
  url classes; every role maps to a distinct class). Email mapped to the Url role (an
  actionable address). Remaining for point 5, the visible wiring: **5b** the chrome
  stylesheet (the `syntax-*` classes coloured from `derive_syntax_palette`), **5c** the
  editor field (`styled_textarea(t, &knot_styles(t.text()))`), **5d** the omnibar.
- **2026-06-26, point 5 visible wiring (editor highlights end to end).** 5b: `syntax_css`
  in `knot_highlight` derives the syntax palette (tinct, perceptual, contrast-gated) and
  emits one `.syntax-* { color }` rule per role, appended to the chrome stylesheet at
  theme-resolve time (main.rs). 5c: `knot_editor_pane`'s field is now
  `styled_textarea(t, &knot_styles(t.text()))`, so the editor paints illume's highlight +
  entity spans as the themed classes. `cargo check -p meerkat` green; mere `81d0c86`,
  Mark's concurrent files untouched. **The whole pipeline is connected**: illume lexer →
  kind → role → class bridge → serval styled field → tinct colours. Honest caveats: the
  palette derives from fixed dark seeds for now (live-theme seeds are the follow-up,
  gated on a register-theme `ThemeTokenSet.syntax` field that ripples to every token-set
  constructor); and this is compile-verified, not yet headed-verified (running the app to
  see the colours is the next confirmation). Remaining: **5d** (omnibar entities live as
  you type), then point 6 (the deriver) and point 7 (extraction + publish).
- **2026-06-26, point 5d done (omnibar highlighting) — point 5 complete.** Added serval
  `styled_text_field` (the single-line styled sibling of `styled_textarea`; `edit` made
  `pub(crate)`), reachable to meerkat via the `.cargo/config.toml` `paths` override on
  `repos/serval/components` (so serval, unlike the git-dep tincture, needs no push or lock
  bump; that override is why earlier serval changes "just worked"). meerkat's
  `omnibar_styles` runs only illume's entity pass (no djot pass, the omnibar is not a
  note), and the omnibar field is now `styled_text_field(t, &omnibar_styles(...))`, so
  urls / mentions / tags colour as you type. serval `ea5fdf33`, mere `8e32f38`; meerkat
  compiles, Mark's concurrent files untouched. **Point 5 complete**: the editor and the
  omnibar both highlight through illume → tinct → the serval styled field, so the
  omnibar-as-legibility-layer vision is realized (the lexer's first non-editor consumer).
  Still compile-verified, not headed-verified. Remaining: point 6 (the `KnotEditor`
  stateless deriver) and point 7 (extraction + publish).
- **2026-06-26, point 5 headed-verified.** Built the meerkat exe and drove it
  (`scry-shots/drive-knot.ps1`): opened the knot editor and screenshotted the editor +
  omnibar. **Highlighting renders at runtime, in distinct perceptual colours.** The
  editor's seeded `# New note` heading colours (teal `#`, themed heading text) versus the
  plain body. The omnibar paints `visit https://example.com or @ada #web` with the url
  blue, `@ada` (mention) teal, and `#web` (tag) magenta, each distinct, plain words left
  white. So the illume → tinct → serval styled-field pipeline is confirmed end to end, and
  the omnibar-as-legibility-layer vision is visibly real. (The editor's *typed* note did
  not land, a click-focus timing miss in the driver, not a highlighting issue; the seed
  heading already proves the editor path.) Point 5 done and verified; remaining points 6
  (the deriver) and 7 (extraction + publish).
- **2026-06-26, point 6 + both tails.** **Point 6**: `KnotEditor` → `KnotReadout`, a
  stateless deriver (holds only the registry + engine; methods take text; the host owns
  the one buffer), resolving #3 (no second copy of the source). mere `e271adc`, 6 tests
  green; no external consumer, since meerkat highlights through illume directly. **Tail A**
  (theme-reactive seeds): `syntax_css` now takes the active theme's seeds, fetched via
  `theme.theme_def(id).seeds` (no register-theme change needed, the seeds live on
  `ThemeDef`), falling back to a dark triad. mere `3fdc64f`. **Visually confirmed**: a
  re-driven shot shows the editor's `# New note` heading now coloured by the active theme
  (orange, its brand) instead of the fixed-seed teal. **Tail B** (the driver's editor
  typing): the floated-panel textarea would not take focus from a *simulated* click across
  two attempts (a double-click on the seed text included), so the typed note never landed;
  the omnibar (a toolbar field) focuses fine. A harness / synthetic-input limitation, or a
  click-to-focus quirk for the floated panel worth a real-mouse check, not a highlighting
  gap, since the editor heading + the omnibar's distinct url / mention / tag colours
  already prove both paths. Remaining: point 7 (extraction + publish).
