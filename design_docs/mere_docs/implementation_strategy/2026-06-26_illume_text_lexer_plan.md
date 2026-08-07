# Illume: the text lexer / highlighter, its tinct + genet pairing, and the omnibar legibility goal

**Date**: 2026-06-26
**Status**: Points 1-6 done and headed-verified. Point 7 part-shipped 2026-06-27
(tinct 0.1.0 + illume 0.0.1 published). **Point 8 landed 2026-07-08: illume extracted
to its own repo and the bridge dissolved into genet.** illume is now a standalone
public repo (`github.com/mark-ik/illume`, MIT OR Apache-2.0, edition 2024), and the
`SyntaxKind → SyntaxRole → class` bridge that had lived host-side in
`meerkat/knot_highlight.rs` moved *into* xilem-serval behind a `highlight` feature
(optional illume + tinct deps). So the highlighter is no longer a per-host concern: any
genet host flips `xilem-serval/highlight` and gets `highlighted_textarea` /
`highlighted_text_field` / `syntax_css` for free (the omnibar, a note editor, a chat
line, and — when genet gains a TUI backend — terminal text too). meerkat's bridge is
deleted; Isometry (which already consumes xilem-serval) can adopt highlighting with a
single feature flag. See the 2026-07-08 Progress entry. Captures the decisions and the
build of the illume promotion across the 2026-06-26 / 27 / 07-08 sessions: the knot
editor's highlight core promoted to a standalone sibling crate (**illume**), paired with
tincture (published as **tinct**) for themed colours and genet's styled field for
rendering, aimed at making mere/meerkat operable and legible from the omnibar.

## The idea

The highlight core we built for the knot editor (`crates/inker/illume`, renamed from
`knot-editor`: jotdown djot structure + the pluggable `InjectionLexer` registry + the
`logos` language pack) is not editor-specific. It is a general capability: take text, run a lexer, get
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
3. **genet styled field (the renderer)** — `(range, css/class)` → styled `<span>` runs
   in the edit surface. `xilem_serval::styled_textarea`, shipped this session. Generic,
   no djot knowledge.

The **host (meerkat)** bridges them: it maps each illume `SyntaxKind` onto a tinct
`SyntaxRole`, looks up the colour in the derived `SyntaxPalette`, and hands genet the
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
  illuminate). **Published 0.0.1** on crates.io (2026-06-27, a name-reserve straight from
  the workspace; the 0.0.x signals the API is still moving). The `limn` and tincture
  "illuminated manuscript" pairing was the runner-up (`limn` is held by a dead 2017
  placeholder).
- **tinct**: tincture's published name. `tincture` is already taken on crates.io (a
  different OKLCH colour crate, 1.0.0), so tinct was necessary, not just shorter.
  **Published 0.1.0** on crates.io (2026-06-27). The rename turned out mere-only (Woodshed
  doesn't consume it after all); mere sources tinct from crates.io via a `package = "tinct"`
  alias, so every `use tincture::` and `tincture.workspace` stayed unchanged.

## Build sequence (the point-by-point)

1. **tinct `syntax` module — DONE** (tincture `03661ce`). `SyntaxRole` (16 roles) +
   `derive_syntax_palette`: accents fanned off the brand primary in OKLCH, contrast-gated
   to WCAG 4.5 against the surface, muted roles on the dim-text tier. This is the #1 fix
   (colours derived, never hardcoded).
2. **genet `styled_textarea` — DONE** (genet `6a3ceace`). Per-range styled `<span>`
   runs, innermost-wins flatten, caret/preedit/ghost intact, fully generic.
3. **illume — DONE** (mere `bcaf834` rename + `6d0a2ae` entity pass). Renamed
   `knot-editor` → illume; added the prose-entity passes (URL / mention / tag / email)
   and their `SyntaxKind`s. The `SyntaxKind` → `SyntaxRole` map is the host's (point 5).
4. **#2 — DONE** (genet `3abaad8`). Folded genet's `styled_body` into one style-aware
   field body (plain text = the empty-styles case); 75 genet tests pass.
5. **The bridge — DONE + headed-verified** (mere `81d0c86` editor + `8e32f38` omnibar,
   genet `ea5fdf33`). illume spans → `SyntaxRole` → tinct colour → the genet styled
   field, as a themeable host stylesheet; the editor and the omnibar both highlight, and a
   headed run confirmed distinct perceptual colours at runtime.
6. **#3 — DONE** (mere `e271adc`). `KnotEditor` → `KnotReadout`, a stateless deriver
   (registry + engine, methods over text; the host's buffer is the single source), so the
   model is not an orphaned second copy of the text.
7. **Promotion — PART-SHIPPED** (2026-06-27). tinct renamed + **published 0.1.0**; the
   github repo renamed tincture → tinct; mere sources tinct from crates.io via the
   `package = "tinct"` alias (no src churn — the Woodshed consumer turned out not to exist).
   illume **published 0.0.1** as a name-reserve straight from the workspace. Remaining:
   illume's extraction to its own sibling repo + a stable release once the API settles.
8. **Extraction + genet-owns-the-bridge — LANDED** (2026-07-08). illume extracted to its
   own public repo (`github.com/mark-ik/illume`, MIT OR Apache-2.0, edition 2024, 32
   tests); mere consumes it by git dep + local override, the in-workspace copy deleted.
   The `SyntaxKind → SyntaxRole → class` bridge moved from `meerkat/knot_highlight.rs`
   into **xilem-serval** behind a `highlight` feature (optional illume + tinct deps),
   exposing `highlighted_textarea` / `highlighted_text_field` / `syntax_css`. This
   **revises Decision 5**: the seam is no longer host-owned (that was a single-host
   premise), it is owned by the shared toolkit, so every genet host inherits highlighting
   for free and none re-writes the map. meerkat's bridge deleted; 230 bin tests green.

## Decisions

1. **Colours are derived, not hardcoded** (#1). The host maps `SyntaxKind` → `SyntaxRole`
   → `tinct::derive_syntax_palette` over the active theme's seeds (fetched directly via
   `theme.theme_def(id).seeds`, no register-theme change needed), emitting the `syntax-*`
   rules into the chrome stylesheet genet applies. No parallel palette; tracks the theme
   and any reseed.
2. **Highlight runs carry classes, not inline CSS.** Themeable through one sheet, the
   genet/stylo way, so user/mod themes override.
3. **One style-aware field body** (#2), not a styled fork of the plain one.
4. **`KnotReadout` is a stateless deriver** (#3, renamed from `KnotEditor`), the host owns
   the buffer. The portable derivations (highlight / outline / folds / render) take text;
   the registry is built once and held, fixing the per-keystroke rebuild.
5. **The `SyntaxKind` → `SyntaxRole` seam is the host's**, keeping illume and tinct
   independent of each other. *(Revised 2026-07-08 — see Decision 8: with two consumers
   (meerkat, Isometry) the map is shared infrastructure, not host glue, so it moved into
   xilem-serval's `highlight` feature. illume and tinct stay mutually independent — the
   toolkit is the one place that knows both.)*

## Cross-references

- [djot editor + knot nodes plan](../../archive_docs/2026-08-06_completed_plans/2026-06-24_djot_editor_knot_nodes_plan.md): the editor
  that illume's highlight core grew out of; its Phase 2/3 highlight + injection work is
  illume's seed.
- [borrowed-ideas brief](../research/2026-06-25_borrowed_ideas_brief.md): the `=query`
  block and the broader graph-as-text directions the omnibar grammar feeds into.
- [command registry / configurable menus plan](2026-06-21_command_registry_configurable_menus_plan.md):
  the `>` shell + `ActionRegistry` that the omnibar legibility goal sits on top of.
- tincture (publishing as tinct): the `syntax` module, the colour contract.
- genet `xilem-serval`: `styled_textarea`, the renderer.

## Progress

- **2026-06-26, plan written + first two pieces shipped.** Captured the session's
  decisions: illume (the promoted lexer name), tinct (tincture's necessary published
  name), the three-piece architecture (illume lexer / tinct palette / genet renderer)
  with the host-owned `SyntaxKind` → `SyntaxRole` seam, the omnibar-as-legibility-layer
  goal, and the #1/#2/#3 resolutions. Shipped tinct's `syntax` module (perceptual
  contrast-gated highlight palette, tincture `03661ce`) and genet's `styled_textarea`
  (genet `6a3ceace`). Remaining points 3-7 to address in order.
- **2026-06-26, point 3 done (illume born).** Renamed the `knot-editor` crate to
  **illume** in place (dir + package name + the host crate's dep/use; meerkat untouched,
  since its `knot_editor_*` identifiers are local names, not crate uses; mere `bcaf834`),
  and added the prose-entity pass: a `logos` lexer (`entity.rs`) emitting URL /
  `@mention` / `#tag` / email spans for *any* text, with the matching `SyntaxKind`s (mere
  `6d0a2ae`). illume now enriches the omnibar and comms, not just the editor; 32 tests
  green. Point 3(c), the `SyntaxKind` → `SyntaxRole` map, is the host's and lands with
  the point-5 bridge. Next: point 4 (unify genet's field body).
- **2026-06-26, point 4 done (one field body).** Unified genet's field rendering: a
  single `field_children(input, styles)` in `styled_field.rs` builds the field's
  children (unstyled runs as text nodes, styled runs as `<span class>`) with the
  caret / preedit / ghost splice; the plain `text_field` / `textarea` pass empty styles,
  so there is one body, not a styled fork. `TextField` is now the Vec-children view type
  and `field_body` shrank to a 3-line delegator (controls.rs lighter). Styled runs carry
  a **class** (not inline CSS), baking in the #1 themeable decision. `StyledField`
  dropped (it is `TextField` now). genet `3abaad8`; 75 tests pass, including the existing
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
  kind → role → class bridge → genet styled field → tinct colours. Honest caveats: the
  palette derives from fixed dark seeds for now (live-theme seeds are the follow-up,
  gated on a register-theme `ThemeTokenSet.syntax` field that ripples to every token-set
  constructor); and this is compile-verified, not yet headed-verified (running the app to
  see the colours is the next confirmation). Remaining: **5d** (omnibar entities live as
  you type), then point 6 (the deriver) and point 7 (extraction + publish).
- **2026-06-26, point 5d done (omnibar highlighting) — point 5 complete.** Added genet
  `styled_text_field` (the single-line styled sibling of `styled_textarea`; `edit` made
  `pub(crate)`), reachable to meerkat via the `.cargo/config.toml` `paths` override on
  `repos/genet/components` (so genet, unlike the git-dep tincture, needs no push or lock
  bump; that override is why earlier genet changes "just worked"). meerkat's
  `omnibar_styles` runs only illume's entity pass (no djot pass, the omnibar is not a
  note), and the omnibar field is now `styled_text_field(t, &omnibar_styles(...))`, so
  urls / mentions / tags colour as you type. genet `ea5fdf33`, mere `8e32f38`; meerkat
  compiles, Mark's concurrent files untouched. **Point 5 complete**: the editor and the
  omnibar both highlight through illume → tinct → the genet styled field, so the
  omnibar-as-legibility-layer vision is realized (the lexer's first non-editor consumer).
  Still compile-verified, not headed-verified. Remaining: point 6 (the `KnotEditor`
  stateless deriver) and point 7 (extraction + publish).
- **2026-06-26, point 5 headed-verified.** Built the meerkat exe and drove it
  (`scry-shots/drive-knot.ps1`): opened the knot editor and screenshotted the editor +
  omnibar. **Highlighting renders at runtime, in distinct perceptual colours.** The
  editor's seeded `# New note` heading colours (teal `#`, themed heading text) versus the
  plain body. The omnibar paints `visit https://example.com or @ada #web` with the url
  blue, `@ada` (mention) teal, and `#web` (tag) magenta, each distinct, plain words left
  white. So the illume → tinct → genet styled-field pipeline is confirmed end to end, and
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
- **2026-06-26, plan audit + corrections.** Re-read the plan against the codebase and
  corrected drift: the Status header and the Build sequence still read "Planning / two
  pieces shipped" though points 1-6 are done and headed-verified (each point now carries
  its DONE marker + commit); `illume` is **available** on crates.io (verified), not "taken
  by an abandoned non-entry" as written, so point 7 needs no reclaim; Decision #4's
  `KnotEditor` is now `KnotReadout`; the `crates/inker/knot-editor` path is now `illume`;
  and Decision #1's "wired through register-theme" was corrected to the path Tail A
  actually took (`theme.theme_def(id).seeds` directly, no register-theme change). Commit
  SHAs verify (the lone "missing" hit is a tincture commit, correctly scoped). Carried
  forward, Tail B: the code paints both a caret (tracks the field's text colour) and an
  accent-blue focus ring for any focused node, so the missing feedback in the harness
  shots points to the simulated click not focusing the field rather than to invisible
  feedback; a real-mouse check distinguishes a harness limit from a click-to-focus bug.
- **2026-06-27, tinct + illume published (point 7, partial).** Shipped the publish half of
  point 7. tinct: renamed the package (tinct repo `cb33adc`), fixed the doctest / README
  refs, **published tinct 0.1.0** to crates.io, renamed the github repo tincture → tinct,
  and switched mere's dep to the crates.io crate via
  `tincture = { package = "tinct", version = "0.1" }` (the alias keeps every `use tincture::`
  working — register-theme build-verified against it, and the published crate shipped
  `src/syntax.rs` so the `SyntaxRole` consumer still resolves). The rename was mere-only:
  Woodshed carries no tincture dep despite the README note. illume: **published illume
  0.0.1** as a name-reserve straight from the workspace (no extraction yet), claiming the
  scarce name with a 0.0.x that signals the API is still moving. Deferred: illume's
  extraction to its own sibling repo + a stable (0.1) release, both held until the API
  settles. Cosmetic loose end: the local checkout is still `repos/tincture` (crate + remote
  are tinct); rename the dir whenever convenient.
- **2026-07-08, point 8: illume extracted + the bridge dissolved into genet.** Prompted
  by Mark asking whether the editor should promote to a standalone that Isometry could
  consume. The dep-graph check reframed it: Isometry already consumes `xilem-serval` (git
  dep), and illume + tinct are already standalone, so the widget + lexer + palette are
  *already* shared; the one un-shared piece was the `SyntaxKind → SyntaxRole → class`
  bridge, duplicated per host. Rather than a third bridge crate (Mark: "right back at
  making two crates three... nah"), the bridge moved into genet's toolkit, where it can
  see both libs: **xilem-serval owns highlighted text, every genet host inherits it for
  free.** Landed across three repos: (1) **illume extracted** to its own public repo
  (`github.com/mark-ik/illume`, MIT OR Apache-2.0 relicense off mere's MPL, edition 2024,
  MPL per-file headers stripped, 32 tests, pushed to `main`); (2) **mere repointed** —
  git dep + local `.cargo/config` override in meerkat + knot-editor-host, illume dropped
  from workspace members, the in-workspace copy deleted; (3) **xilem-serval** gained a
  `highlight` feature (optional illume + tinct deps) with `src/highlight.rs`: the kind→role
  map, `role_class`, `note_styles` / `entity_styles`, the `Highlight` mode enum,
  `highlighted_textarea` / `highlighted_text_field`, and `syntax_css` — emitting genet's
  native `StyleRange`, 3 tests, 96 total green; (4) **meerkat** deleted `knot_highlight.rs`,
  repointed the editor field to `highlighted_textarea(t, Highlight::Note)` and the
  stylesheet to `xilem_serval::syntax_css` (with a small host-side `fallback_seeds`),
  dropped its direct illume dep; 230 bin tests green. Revised Decision 5 (the seam is the
  toolkit's now, not each host's), keeping illume and tinct mutually independent. Not yet
  committed across the three repos; the github push of illume was the one gated action.
  Follow-on: Isometry can adopt highlighting with a single `xilem-serval/highlight` flag;
  omnibar entity highlighting (lost in concurrent churn) re-wires as one
  `highlighted_text_field(omnibar, Highlight::Entities)` call.
