# Theme Modes Plan (light / dark / high-contrast / custom)

**Date**: 2026-07-05
**Status**: T1–T5 implemented 2026-07-05 (see Progress; T5 shipped the declarative lane, rhai
graduation open). From Mark's theme-model decision (2026-07-05), unblocking the W3C adoption
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
- **RESOLVED 2026-07-05: sheet-swap path.** Stylo's servo-side media feature table at serval's
  pinned rev (8bde0e9, `style/servo/media_features.rs`) evaluates only `width / scan /
  resolution / device-pixel-ratio / -moz-device-pixel-ratio / prefers-color-scheme`;
  `prefers-contrast` exists gecko-side only. So the 2x2 4-in-1 sheet is not expressible on the
  servo Device today. Wired instead: `rebuild_chrome_sheet` bakes the pair AT the current
  contrast level (`(Light, Dark)` or `(HcLight, HcDark)`); picking a mode across contrast
  levels changes the sheet strings and takes the session-rebuild path (asserted in
  `chrome_sheet_bakes_the_scheme_pair_and_mode_flip_keeps_it_fixed`). Acceptable at contrast-
  switch frequency. Liftable later by adding `prefers-contrast` to the stylo servo table
  (fork territory) or an upstream stylo change.

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
- 2026-07-05: **T1 landed.** tinct (repos/tincture, v0.1.1 NOT YET PUBLISHED — Mark's call):
  `ModeProfile { dark, high_contrast }` with `LIGHT/DARK/HC_LIGHT/HC_DARK` consts +
  `derive_palette_with`; hc ladders push surfaces toward the extremes, text past them, and
  tighten the dim/disabled blend; `derive_palette(seeds)` unchanged as the degenerate form.
  Test `four_canonical_modes_derive_distinct_wider_hc_palettes` (7:1 floor + wider-than-normal
  assertions) green. Mere does NOT consume the new tinct API yet (crates.io pin at 0.1.0);
  register-theme derives per-mode through its existing `(dark, hc)` profile machinery:
  `theme::Mode { Light, Dark, HcLight, HcDark, Custom(id) }` (key/label/flags helpers),
  `seed::derive_from_def_for_mode`, `seed::default_mode_for_def`,
  `ThemeRegistry::mode_tokens`. Tests green (mode key roundtrip; four distinct palettes from
  one theme with the hc 7:1 gate; legacy-builtin default modes).
- 2026-07-05: **T2 landed.** `theme_sheets::bake_scheme_pair` (light rules base + dark-only
  rules in one `@media (prefers-color-scheme: dark)` block); `rebuild_chrome_sheet` bakes the
  pair at the current contrast level and refreshes a `chrome_theme_light/dark` token pair on
  `Presentation`; `gather_chrome_css` (roster/apparatus/utility/gloss pane CSS appended to the
  chrome sheet per frame) builds from the PAIR and pair-bakes too, so the chrome sheet identity
  is scheme-invariant. `PaneSession::refresh/scene` take `scheme_dark`: sheet unchanged +
  scheme changed rides `IncrementalLayout::set_prefers_color_scheme` (session + element scroll
  survive); a rebuild seeds the scheme after `new` (engine builds light-default — a
  `new`-with-scheme serval API would save that extra recascade, minor follow-up). Non-sheet
  lanes re-key off the mode tokens via `theme_edit::apply_resolved_tokens` (shared by
  `set_theme` / the new `set_mode`); the chrome base-raster cache folds the scheme into
  `chrome_base_sig`. Mode picker radios on the Appearance page (`mode:set:<key>`); persisted
  as `PersistedSettings::theme_mode`; boot restores it (unset re-seeds from the theme def, so
  the legacy four built-ins keep their meaning — `set_theme` re-seeds the mode the same way).
  Receipts: `chrome_sheet_bakes_the_scheme_pair_and_mode_flip_keeps_it_fixed` (sheet + token
  pair fixed across a scheme flip; hc pick changes them) and
  `scheme_flip_reuses_the_chrome_session_without_rebuild` (`rebuild == false` on the flip).
  meerkat bin suite 220 pass / 3 pre-existing fails (graph_delta_log, roster_view links_tab,
  wallet_pairing — fail at HEAD without these changes; meerkat LIB tests also red at HEAD in
  `ingest.rs`, concurrent-work skew).
  - CORRECTION (same day): the "list-pane pair-baking follow-up" is moot. The standalone
    `ViewPane` panes (RosterPane / ListPane / the settings-pane harness) are TEST harnesses
    only (`main.rs:134`); production roster / gloss / settings panes fold into the chrome
    document and are covered by the pair-baked `gather_chrome_css`. The remaining single-mode
    surfaces — the pelt tile CSS (`tile_sheet`, rebuilt when `pelt_theme != active tokens`) and
    the note-card band bake (`note_sheet`, stateless per re-raster) — are self-invalidating on
    a flip, same cost/behaviour as a theme switch. No further host work needed for the flip.
- 2026-07-05: **T3 resolved** — see the RESOLVED note in T3 (sheet-swap path; stylo servo
  Device has no `prefers-contrast` at the pinned rev).
- 2026-07-05: **T4 landed.** `ThemeDef.mode_sheets: BTreeMap<String, Vec<String>>` (keyed by
  `Mode::as_key`, `#[serde(default)]` so pre-T4 theme files parse; empty lists count as
  absent via `ThemeDef::mode_sheet`; forks carry the overrides). Resolution in
  `rebuild_chrome_sheet`: an override on either side of the scheme pair forces the swap path
  — the sheet is the ACTIVE mode's resolution (custom rules as-is, px-scaled, syntax rules
  appended; else the derived single-mode sheet); only a fully-derived pair bakes the cheap
  flip. Persistence is the theme file itself (`theme_store` serializes `ThemeDef`).
  Authoring surface today: hand-edit `<mere_root>/themes/<id>.json` (the mod-distribution
  path); a settings-lane editor can ride a later settings pass. Receipts:
  `mode_sheets_roundtrip_and_gate_on_non_empty` (register-theme),
  `per_mode_custom_sheet_overrides_the_derived_dark_sheet` (meerkat, the done-when),
  `theme_store::save_then_load_round_trips` extended with a mode-sheet entry (survives
  restart). Suite: 221 pass / same 3 pre-existing fails.
- 2026-07-05: **T5 landed, declarative lane** (the plan's own "start declarative" pick; the
  rhai graduation stays open for when authors need logic — the file shape below is the
  compatibility floor). A custom mode is `<mere_root>/modes/<id>.json`
  (`register_theme::mode_calc::CustomModeDef`): id + name + declared `(dark, high_contrast)`
  flags + a mapping table from every `ChromeTheme` role to a small OKLCH transform of one
  seed (`seed`, optional `l` / `c` / `rotate` degrees / `alpha`, or `on: true` for the
  contrast-picked text over the computed fill) — the same tinct maths as the built-in
  derivation, tiny and reviewable like theme files. Load: `mode_store::load_custom_modes` at
  boot (incomplete / malformed / duplicate files skipped + logged; completeness is proven by
  a seed-independent dry run). Resolution: custom modes are sheet swaps by definition —
  `rebuild_chrome_sheet` generates the sheet from the calculator's tokens (no baked pair);
  the non-sheet lanes (orrery / document palettes) derive canonically with the mode's
  declared flags, with the calculator's chrome overlaid (`set_mode` + boot mirror each
  other); `scheme_dark()` presents the declared scheme to the engine. Picker: customs list
  after the canonical four (`mode:set:custom:<id>`); persisted as `custom:<id>`, a missing
  file at boot falls back to the theme default. Failure posture: unknown id / failed eval is
  a logged no-op (pick) or canonical fallback (rebuild/boot) — a stale mode can't blank the
  shell. Receipts: `mode_calc` unit tests (transforms, `on`, rejection-by-name, JSON
  roundtrip), `mode_store` load test, and
  `custom_mode_file_produces_a_working_shell_theme` (the done-when: authored file → boots →
  listed → calculator palette renders → survives restart). Suite: 223 pass / same 3
  pre-existing fails; register-theme 21 pass.
- Remaining follow-ups: the rhai calculator lane (if declarative tables prove insufficient),
  an in-app editor surface for T4 per-mode sheets and T5 mode files (a later settings pass),
  the serval `new`-with-scheme micro-optimisation, and publishing tinct 0.1.1 (then
  optionally migrating register-theme's derivation onto `tinct::derive_palette_with`).
- 2026-07-05 (follow-ups pass): **tinct migration landed** — 0.1.1 published (Mark), pin bumped,
  `derive_token_set` bases on `derive_palette_with(seeds, ModeProfile)` (hc widens at the source;
  the stricter local hc branches stay). register-theme 22/22, meerkat theme tests green.
  **T4/T5 editors landed** (Appearance page): per-mode stylesheet rows for the active USER theme
  ("derived — copy to theme file" materializes the derived sheet into `mode_sheets` /
  "custom sheet — clear to derived"; built-ins show the fork hint); custom-mode management
  ("+ New custom mode (from current)" seeds a complete `CustomModeDef::template` file — valid for
  all four flag combos by test — plus per-mode Remove and "Reload modes from disk" for
  hand-edits; active removed mode falls back to the theme default). `mode_store` gained
  save/delete. The FILES stay the authoring surface (mod-distribution path); the editor
  materializes, removes, reloads. Receipts: `mode_sheet_editor_materializes_and_clears`,
  `custom_mode_editor_creates_removes_and_reloads`,
  `template_is_complete_and_valid_for_all_flag_combos`; suite 225 pass / same 3 pre-existing
  fails. Still open: serval `new`-with-scheme (deferred while moveBefore edits serval-layout)
  and the rhai calculator graduation.
