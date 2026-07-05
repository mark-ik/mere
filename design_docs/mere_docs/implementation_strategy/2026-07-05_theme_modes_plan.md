# Theme Modes Plan (light / dark / high-contrast / custom)

**Date**: 2026-07-05
**Status**: plan, from Mark's theme-model decision (2026-07-05), unblocking the W3C adoption
plan's P3 host half.
**Related**: `repos/serval/docs/2026-07-05_w3c_mechanism_adoption_plan.md` (P3 engine half landed:
`IncrementalLayout::set_prefers_color_scheme`), `repos/tincture` (tinct seed-to-palette
derivation), `crates/meerkat/src/theme_sheets.rs` + `theme_edit.rs` (current sheet baking +
switch path).

## The model (decision record)

- A THEME is a set of tinct-style seed key colors the user picks (`tinct::Seeds` shape: brand
  triad, neutral, optional text overrides, functional hues).
- A MODE is a derivation profile applied to the current theme's seeds. Canonical modes: light,
  dark, high-contrast light, high-contrast dark. The derivation differs per mode: light modes
  derive lighter surface/text ladders, dark modes darker, high-contrast modes wider lightness
  separation and stronger borders/focus tiers. (tinct's `Seeds.dark: bool` is the degenerate
  two-mode version of this; mode generalizes it.)
- Granular override: any theme may attach a CUSTOM STYLESHEET per mode, used instead of the
  derived palette for that mode. Theme + mode resolve to either (derived palette -> generated
  sheet) or (custom sheet as-is).
- Custom modes: users can create new modes, i.e. a custom palette calculator that receives the
  tinct seeds and produces a stylesheet. Candidate execution lanes per the scripting doctrine:
  rhai (host-automation lane) or a declarative mapping first; pick when the phase lands.

## Engine mapping (what rides which mechanism)

- The light/dark pair WITHIN the current contrast level bakes as one fixed sheet: base rules =
  light palette, `@media (prefers-color-scheme: dark)` block = dark palette. Flipping
  light/dark is then the landed engine path (`set_prefers_color_scheme`): media re-evaluation
  over the persistent Stylist, session survives, no rebuild.
- Contrast level (normal vs high) SHOULD ride `prefers-contrast` the same way; stylo's servo
  Device currently exposes only `PrefersColorScheme` in `Device::new`, so investigate whether
  0.18 evaluates `prefers-contrast` and can be fed the preference. If yes: bake all four
  palettes into the one sheet (2x2 media blocks) and both axes flip cheaply. If no: contrast
  switch takes the sheet-swap path (today's rebuild), which is acceptable at its frequency.
- Custom modes and per-mode custom stylesheets are sheet swaps by definition (different rule
  sets, not different media applicability).
- OS-follow: a setting maps the system scheme (and, where the platform reports it, the system
  contrast preference) onto the mode; manual pick overrides. Follows the configurability
  doctrine: expose, do not hardcode.

## Phases

### T1. Mode type + derivation profiles

- `Mode { Light, Dark, HcLight, HcDark, Custom(id) }` in the presentation layer; tinct grows
  per-mode derivation (replace `Seeds.dark: bool` at the call boundary with a profile:
  lightness ladder direction + contrast spread; tinct API change is upstream-first in
  repos/tincture since Woodshed/Strophe share it).
- **Done when** the four canonical modes derive distinct, sane palettes from one seed set in a
  tinct unit test (hc modes measurably wider text/surface contrast, e.g. an APCA/WCAG floor).

### T2. Bake the scheme pair + cheap flip (P3 host half)

- `theme_sheets::chrome_sheet` takes the active theme's LIGHT and DARK palettes (current
  contrast level) and emits one sheet: light values as base, dark values inside
  `@media (prefers-color-scheme: dark)`. Structural rules emitted once.
- `PaneSession::refresh` gains a scheme input: sheet unchanged + scheme changed calls
  `IncrementalLayout::set_prefers_color_scheme` instead of rebuilding; session creation seeds
  the scheme.
- The non-sheet theming (orrery palette, document palette, actor retheme, decoration caches)
  keys off the resolved mode palette exactly as today, on the same flip.
- **Done when** a light/dark mode flip re-themes the shell with `rebuild_us` absent from the
  capture (the `apply`-scale restyle only) and pixel output matches a from-scratch build of the
  same mode.

### T3. Contrast axis

- Resolve the stylo `prefers-contrast` question (see engine mapping). Either wire the second
  media axis (4-in-1 sheet) or route contrast switches through the sheet-swap path explicitly.
- **Done when** hc modes are pickable, derive via T1, and the chosen mechanism is recorded here
  with receipts.

### T4. Per-mode custom stylesheets

- Theme def gains optional per-mode sheet overrides; resolution order: custom sheet for
  (theme, mode) else derived palette sheet. Overrides participate in the media baking only when
  both scheme counterparts are derived; a custom sheet on either side of the pair forces the
  swap path for that theme (correctness first, optimize later).
- **Done when** a user-supplied dark sheet renders instead of the derived dark palette and
  survives restart (settings persistence).

### T5. Custom modes (calculator lane)

- A registered custom mode = name + calculator producing a stylesheet from `tinct::Seeds`.
  Start declarative if a mapping table covers the real cases; graduate to the rhai lane if
  authors need logic. Custom modes list alongside canonical ones in the mode picker.
- **Done when** a custom mode authored without rebuilding the app produces a working shell
  theme from the active seeds.

## Sequencing

T1 then T2 (T2 is the P3 host half and pays immediately). T3 after the stylo investigation.
T4 and T5 ride settings passes; T5 last.

## Progress

- 2026-07-05: decision recorded, plan written. Engine prerequisite (scheme flip) already landed
  serval-side.
