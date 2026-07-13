# Seed-Palette Theme System Plan

**Status:** §11 signed off (Mark, 2026-06-22); **complete — T0–T5 + accent
harmony shipped + verified 2026-06-22.** The theme system derives from seeds end
to end; user + mod themes work; the in-app seed editor edits live with rainbow
HSL sliders + an accent-harmony picker (Custom / Lock current / triadic /
analogous / complementary / mono), and on-accent text/icons are contrast-picked
(`best_on`). See the Progress log for the per-phase record and the (non-blocking)
follow-ons (granular per-token editor; the rest of the on-accent glyph sweep;
re-pointing Woodshed's `audio_widgets` onto `tincture`).
**Date:** 2026-06-22.
**Scope:** re-base Mere's theme system on a small **seed palette**
(primary / secondary / tertiary + neutral + mode) that *derives* the full
`register-theme` token set, so a theme is a handful of colors; add **user
themes** (CRUD, persisted) and **mod-authorable theme files**; expose a
seed-color **authoring UI**. Goal: Woodshed-grade derivation, Zed-grade
extensibility.

Adopts the **shared Strophos theming model** already shipped in Woodshed
(`audio_widgets::theme`), whose own design doc
([`repos/woodshed/design_docs/2026-05-20_theme_system_design.md`]) names Woodshed,
Strophe, **and Mere** as the intended consumers.

Related: [apparatus_pane_and_theme_switcher_plan](2026-06-08_apparatus_pane_and_theme_switcher_plan.md)
(A1/A2 shipped the runtime switcher + chrome/orrery theming this builds on),
[settings_lane_consolidation_plan](2026-06-21_settings_lane_consolidation_plan.md)
(the `pelt` appearance page is the authoring UI's home),
[document_style_sheet_plan](../../inker_docs/implementation_strategy/2026-06-21_document_style_sheet_plan.md)
(P3 already derives the document palette from `ThemeTokenSet.chrome`, so documents
re-theme for free; this plan **is** that plan's P5, generalized to the whole
shell), [persona_model_brief](../research/2026-05-14_persona_model_brief.md)
(user themes persist under `<persona_id>/settings/`).

---

## 1. The decision in one line

A theme becomes **seeds → derived `ThemeTokenSet`**, not a hand-authored wall of
100+ `Color32` literals. Built-ins are seed sets; users author themes by picking
seeds (or dropping in a theme file); everything (chrome, orrery, graph-node
chrome, **and documents** via the P3 seam) recolors from the same seeds.

---

## 2. Findings (code-verified 2026-06-22)

### 2.1 Woodshed's engine exists and is shared-by-design

`repos/woodshed/crates/audio-widgets/src/theme.rs` (~660 LOC, 8 tests) ships:
- `Seeds { primary, secondary, tertiary, neutral, text_header: Option, text_body:
  Option, success, danger, dark: bool }`.
- `derive_palette(&Seeds) -> Palette` in **OKLCH** (hand-rolled Ottosson oklab,
  no color-crate dep): surface ladder (perceptual L steps off `neutral`), text
  hierarchy (contrast-gated), `best_on` (near-white/near-black by WCAG contrast),
  `mix` (dim/disabled tiers), HSL/hex helpers for pickers.
- `ThemeMode` = 6 curated built-ins (Slate/Ember/Light/Dusk/Meadow/Parchment),
  each a `Seeds` set; `ThemeMode::palette()` = `derive_palette(seeds)`.
- The module doc states it is the **product-agnostic** base for the family; each
  app layers product colors on top of the derived base.

The color math depends only on `peniko::Color` (a u8 RGBA type) + std. The
spacing / type-scale / font helpers in that file are masonry-coupled and **not**
needed by Mere.

### 2.2 Mere's current theme system

`crates/system/registry/register-theme/`:
- `ThemeTokenSet` (theme.rs) — **~50 top-level `Color32` tokens** plus nested
  `ChromeTheme` (17 tokens) + `GraphNodeChromeTheme` (10) + `ThemeData`
  (background_rgb, accent_rgb, font_scale, stroke_width) + edge tokens. The
  gpui-ish "explicit named token" shape.
- 4 built-ins, each a **wall of literal `Color32`s**: `default_theme_tokens`
  (Slate-ish dark), `light`, `dark`, `high_contrast`. Adding a theme = authoring
  ~80 literals by hand.
- `validate_theme_tokens` enforces WCAG contrast (4.5 normal, **7.0 HC**) on a
  few text/bg token pairs + edge-token rules.
- `ThemeRegistry` keyed by `theme:*` id; `set_active_theme` / `resolve_theme` /
  fallback already exist. Host stores `active_theme_id: String` in settings.
- **No user themes, no derivation, no theme files.** Theme authoring is
  code-only.

### 2.3 The seam is already in place (P3)

`meerkat::document_palette(tokens)` derives the document `ColorVocabulary` from
`tokens.chrome` + `accent_rgb`; `orrery_palette(tokens)` derives the orrery
backdrop/edges; `chrome_sheet(chrome_theme)` builds the chrome CSS. So **all three
surfaces already read from `ThemeTokenSet`.** If seeds derive `ThemeTokenSet`,
every surface re-themes with no further wiring.

### 2.4 Zed's extensibility (the bar)

Zed themes are JSON files (a `themes/` dir + extensions), each a *family* of
named variants, with ~60 semantic UI tokens + a syntax map, validated, hot-loaded.
The lesson for Mere: **themes are data, discovered + loaded into the registry**,
not code. Mere's compact seed model makes a theme file ~6 colors, not 60.

---

## 3. The seed model (Mere)

Mirror Woodshed's `Seeds`, on `kernel::color::Color32`:

```rust
// Illustrative-signature-only.
pub struct ThemeSeeds {
    pub primary: Color32,     // brand anchor: active states, selection, primary actions
    pub secondary: Color32,   // chrome / nav surfaces tint (header, sidebars)
    pub tertiary: Color32,    // "you are here": selection, active tab, focus
    pub neutral: Color32,     // surface + text hue (tinted near-grey carries the theme)
    pub dark: bool,           // run the ladders dark or light
    pub text_header: Option<Color32>, // None → derive from neutral
    pub text_body: Option<Color32>,   // None → derive; drives dim/disabled
    pub success: Color32,     // fixed semantic, overridable
    pub danger: Color32,
}
```

A theme = these 6-ish colors + a mode + optional per-token overrides.

---

## 4. Derivation: seeds → `ThemeTokenSet`

Port Woodshed's OKLCH engine into a Mere module (see §9 home decision), operating
on `Color32`: `oklch` transform, `surface ladder` (L steps off `neutral`),
`text hierarchy` (contrast-gated), `best_on`, `mix`, `contrast`. Then a
**mapping layer** fills Mere's richer token set from the derived base:

- **Surfaces** → `chrome.toolbar_bg` / `panel_bg` / `surface_bg` / `field_bg` /
  `control_bg` / `menu_bg` from successive surface-ladder steps;
  `theme_data.background_rgb` = `bg`; `workbench_panel_background` = `surface`.
- **Text** → `chrome.body_text` / `strong_text` / `muted_text` from the text
  hierarchy; `disabled_text` from the disabled tier.
- **Triad** → `theme_data.accent_rgb` = `primary`; `graph_node_focus_ring` /
  `graph_node_selection` / `radial_*` actives from the triad; `on_*` via `best_on`.
- **Graph-node chrome** → badge bg/text + strokes from neutral steps + `best_on`.
- **Semantic** → `status_success`/`status_error`/`status_warning` from
  `success`/`danger` (+ a derived warning).
- **Documents** → already `document_palette(tokens.chrome)` (P3). No new work.

The long-tail tokens (every `radial_*`, hover, badge variant) derive from the
base with sensible relationships — the Radix "derived + refinement" model, not
200 hand-picks. **Overrides** are a per-token layer on top (the user/theme-file
can pin any token), so full control remains available.

**Contrast gate**: the derivation must keep clearing `validate_theme_tokens`
(4.5 / 7.0 HC). The text + `on_*` picks are contrast-driven already; HC is the
risk case (§11.4).

---

## 5. Built-in themes

Re-express built-ins as `ThemeSeeds`. Decision in §11.3: faithfully re-derive the
current 4 (accepting small color shifts) vs keep the current 4 explicit and add
new seed-derived themes (Woodshed's Slate/Ember/Dusk/Meadow/Parchment) alongside.
Either way the **derivation engine is proven against the built-ins** before users
touch it.

---

## 6. User themes + CRUD

Adopt Woodshed's management model:

```rust
pub struct ThemeDef { pub id: ThemeId, pub name: String, pub seeds: ThemeSeeds,
                      pub overrides: Vec<(TokenPath, Color32)> }
pub enum ThemeSource { BuiltIn, User }
```

- **Create**: pick seeds → `user:<uuid>` theme.
- **Edit/rename a built-in**: forks to a `user:` copy (built-in stays intact).
- **Edit/rename a user theme**: in place.
- **Remove**: user themes only.
- **Graceful fallback**: missing `active_theme_id` → default built-in, logged.
- **Persistence**: user themes under `<persona_id>/settings/themes/`
  (per-persona, per the persona brief); selection in the existing settings
  `active_theme_id`.

Runtime switching already works (P3 live retheme + chrome/orrery), so editing
seeds gives **live preview** for free.

---

## 7. Mod-authorable theme files (the Zed bar)

A theme file is a **serde `ThemeDef`** (seeds + name + optional overrides) in
TOML/JSON, discovered from a `themes/` dir (built-in bundled set + a user/mod
dir) and loaded into `ThemeRegistry` at startup (and on a rescan). Because a
Mere theme is ~6 seed colors, a community theme is a tiny, reviewable file — the
"prime mod candidate." Format + dir decided in §11.5. This is the extensibility
story: **themes ship as data, no recompile.**

---

## 8. Authoring UI (the document sheet's P5, generalized)

In the settings-lane `pelt` appearance page (per the settings-lane plan):
- A theme list (built-ins + user themes), select to activate.
- **+ New / fork**: a seed editor — per-seed swatch + HSL sliders (Woodshed's
  proven picker shape), live re-derive on drag.
- Rename / remove (user themes); export a theme file.
- The document-sheet per-role knobs (font family/size per role, link adornment,
  wrap) live here too as an **advanced** section — they are `DocumentStyleSheet`
  overrides layered on the theme's derived document palette.

---

## 9. Where the engine lives (decision §11.1)

- **Option A — port into Mere** (`register-theme`, a `seed`/`derive` module on
  `Color32`). Decoupled, Mere-native, ~150 LOC of standard OKLCH math borrowed
  from Woodshed's proven engine. Duplicates the algorithm with Woodshed.
- **Option B — extract a shared `strophos-palette` crate** (color-only, no
  masonry) consumed by both Woodshed's `audio_widgets` and Mere's `register-theme`.
  One source of truth (the doc's stated family intent), but a cross-repo refactor
  (carve the color half out of `audio_widgets::theme`, re-point Woodshed, add the
  dep to Mere).

Recommendation: **A now, B later** — port to unblock Mere without a cross-repo
lift; revisit extraction once both have settled, since the algorithm is standard
and stable (low duplication risk).

---

## 10. Phases

- **T0 — derivation engine. (done 2026-06-22.)** Built the shared crate
  **`tincture`** ([mark-ik/tincture](https://github.com/mark-ik/tincture), MPL-2.0,
  no toolkit deps — owns its `Srgb` type). Ports Woodshed's OKLCH derivation
  (`Seeds` → `derive_palette` → `Palette`: surface ladder, contrast-gated text,
  contrast-picked `on_*`, `mix`/`best_on`/`contrast`, hex/HSL helpers) and
  **exposes the `oklch` module publicly** (`Oklch::from_srgb`/`to_srgb`/
  `with_l`/`with_c`/`with_h`/`lighten`/`darken`/`rotate_hue`) so hosts derive
  richer token tiers on the same maths. 8 unit tests + a doctest, green; pushed
  to `main`. Woodshed can adopt it as a strict superset (its `derive_palette`
  logic is reproduced); Mere consumes it next (T1) via `git`/`branch=main`.
- **T1 — seeds → `ThemeTokenSet` mapping. (done 2026-06-22.)** New
  `register-theme/src/seed.rs`: `Color32 ↔ Srgb` conversions; `derive_token_set(id,
  name, seeds, profile)` maps the tincture base `Palette` + OKLCH long-tail onto
  Mere's ~80 tokens (chrome surface tiers, radial menu, graph-node chrome,
  status), with `ensure_contrast` clearing the four validator-gated text pairs;
  role intent followed (primary = accent/focus, secondary = badge tints, tertiary
  = selection / active-tab / hover). The four built-ins re-expressed as seeds +
  a `ThemeProfile` (edge tokens / font+stroke scale / high-contrast flag); HC uses
  a high-contrast mode that forces surfaces to the pure extreme + max-contrast
  text. The six hand-authored token-wall functions deleted from `theme.rs`
  (601 → 318 LOC). register-theme tests green incl. **every built-in passes
  `validate_theme_tokens`**; meerkat compiles. Headed-verified: Default (slate)
  renders coherent (slate surfaces, blue primary, amber tertiary tab marker,
  readable card) and HC renders correct (black/white/yellow, readable). The
  `tincture` git dep resolves from `mark-ik/tincture@main`.
- **T2 — registry on seeds. (registry half done 2026-06-22.)** `ThemeRegistry::default`
  now builds all built-ins via `seed::builtin_defs()` → `derive_from_def`; the
  missing-id fallback uses `seed::default_token_set`; selection unchanged
  (`active_theme_id`). The `ThemeDef`/`ThemeSource` user-theme types landed in T3.
- **2026-06-22 (T3 + T4 — user / mod themes, green + e2e verified).** register-theme:
  `ThemeDef` + `ThemeSource`; the registry stores authored defs + listing order
  beside the derived sets (`add_user_theme`/`remove_user_theme`/`rename_user_theme`/
  `fork`/`list`/`theme_def`); the built-in token-set derivation collapsed to one
  path (`builtin_defs` + `profile_for` + `derive_from_def`). meerkat `theme_store`
  loads/saves/deletes serde `ThemeDef` JSON in `<mere_root>/themes/`; startup
  loads user + mod theme files into the registry (bad files skipped + logged);
  `theme_options` now lists from `registry.list()` so user/mod themes appear in
  the picker. Tests: register-theme 11 (CRUD, listing, fallback, serde
  round-trip), meerkat theme_store 2 + 166 total. **E2e:** a hand-authored
  `Sunset` mod theme file loaded, derived, validated, and rendered as the active
  theme — the no-recompile mod path works. Deps added: meerkat `serde_json`;
  register-theme `serde_json` (dev). **The core ask is met: themes are real,
  authorable data now** (drop a `ThemeDef` JSON in `themes/`); T5 is the in-app
  GUI editor on top. Left an example `user_sunset.json` in the themes dir.
- **2026-06-22 (T5 — in-app theme editor, green + e2e verified).** Seed editor
  on the `pelt/appearance` page (`theme_editor_items`): fork / mode-toggle /
  per-seed HSL steppers / remove, draining `theme:*` keys through
  `apply_pelt_activation` to new `frame_ops` CRUD methods that mutate → re-derive
  → re-validate (drop a contrast-failing edit) → persist → `set_theme` live.
  meerkat deps `tincture` + `uuid`; 166 tests green. **Headed:** appearance page
  lists built-ins + the Sunset mod theme; "+ New custom" forked Dark into
  "Dark (custom)" and revealed the editor; Primary "Hue +"×7 shifted
  `#6EAAFF → #FF6ED0` and recolored the whole UI blue→pink live + persisted.
  **Seed-palette theme system T0–T5 complete.** Active theme is now the forked
  custom (from the verification); switch via the picker or "Remove this theme"
  to return to Dark. Follow-ons (not blocking): document-sheet per-role overrides
  as an advanced editor section; OKLCH sliders vs the HSL steppers; per-persona
  theme dir; TOML theme files; re-point Woodshed's `audio_widgets` onto
  `tincture` (the family-shared end state).
- **2026-06-22 (T5 — sliders, green + e2e verified).** Replaced the per-seed
  HSL steppers with **segmented sliders**: a `PaneItem::slider` (label + a flex
  strip of `count` clickable cells; click cell `i` → set the channel to
  `i/count`). The settings UI is click-dispatch, not pointer-drag, so a segmented
  click-to-position track is the feasible interactive slider with **no new input
  infra** (each cell is an ordinary `on_click`). The **Hue track is a rainbow**
  (24 cells colored by their hue, selected cell outlined); Saturation/Lightness
  are 16-cell fill tracks. `settings_pane_view::slider_view` renders them (inline
  per-cell colors); CSS in `apparatus_sheet`. `adjust_seed_from_key` parses
  `theme:seed:<seed>:<ch>:<i>:<count>` → `set_active_seed_channel` (absolute
  fraction). meerkat 166 tests green. **Headed:** clicking the green cell on the
  Primary hue rainbow set `#FF6ED0 → #92FF6E` and recolored the whole UI
  magenta→green live. True continuous **drag** sliders remain a follow-on (they'd
  need press-position + track-rect threaded through the shared chrome dispatch).
- **2026-06-22 (accent harmony + readable-on-accent, green + e2e verified).**
  Mark's ask: "key the difference between the red/green/blue as a palette formula
  so changing the base rotates the accents to suit, and text/buttons on those
  colours stay readable." Implemented:
  - `Harmony` on `ThemeDef` (register-theme): `Custom` (accents independent, the
    prior behaviour) or `Locked { secondary_deg, tertiary_deg }` (accent **hue**
    tied to the primary by a fixed OKLCH offset, each accent keeping its own
    chroma + lightness). Presets are just `Locked` with known offsets; "lock
    current" captures the triad's present hue gaps (measured in OKLCH so it is
    non-destructive). `harmonized_seeds(def)` is the single effective-triad path,
    shared by the derivation and the editor's display. The activity accents
    (node hover / selection / focus) already derive from the triad, so they
    follow the rotation for free.
  - Editor (settings_lane): a **Harmony** picker (Custom / Lock current / Triadic
    / Analogous / Complementary / Monochrome, active one highlighted); when
    locked, the accents' hue slider is replaced by a "Hue follows primary" hint
    (saturation + lightness stay per-accent); the hex label shows the *effective*
    colour, the sliders read the stored seed (what they edit). Drains
    `theme:harmony:<key>` → `frame_ops::set_active_harmony`.
  - Readable-on-accent (`best_on`): the tile-tab close `×` was a fixed
    `muted_text`; on the theme-accented **active** tab that can go illegible, so
    `.tile-tab.active .tile-close` is now `best_on(active_bg)` (near-white/black
    by WCAG contrast), computed in `tile_tabbar_sheet`. The same `best_on`
    primitive already powers every `on_*` token; routing the remaining on-accent
    glyphs/labels through it is the mechanical follow-on.
  - register-theme +1 test (`locked_harmony_rotates_accents…` asserts +120/+240
    and that the harmonized theme still clears contrast validation); 12 green.
    meerkat 166 green. **Headed:** set Triadic, then dragged the Primary hue from
    red `#C31C1C` to purple `#701CC3` — Secondary rotated `#8DD096 → #E0AC76` and
    the whole UI (chrome accents + node) recoloured as a coordinated triad.
  - Follow-ons: a **granular per-token style editor** (rides the same `ThemeDef`
    override seam); sweep the rest of the on-accent text/icons through `best_on`;
    an arbitrary-offset "Lock current" UI (today's presets + capture cover it).
- **2026-06-22 (settings-pane scroll, genet-native, e2e verified).** The
  appearance editor grew past the pane fold (Tertiary / Neutral unreachable). Fix
  uses genet's existing scroll primitive (per Mark's "is there a genet feature"
  question): `genet-render` already makes any `overflow: scroll` box a clip +
  scroll container and `push_scrollbars` draws the thumb for any container in the
  host `ScrollOffsets`. The `.settings-pane-body` had `overflow: scroll` but
  wasn't wired, so: a `settings_scroll` offset (window_view), wheel-decremented
  when the cursor is over a settings tile (app_handler), registered in the shell
  `ScrollOffsets` after the panes are folded (render) and mirrored in the
  hit-test (`chrome_click`) — mirroring the apparatus/roster pane pattern. The
  pane now wheel-scrolls with the genet thumb; no host-drawn scrollbar.
- **T3 — user themes + CRUD + persistence. (done 2026-06-22.)** register-theme:
  `ThemeDef` (id / name / source / seeds / high_contrast, serde) + `ThemeSource`
  (BuiltIn / User); the registry now stores authored defs + a listing order
  alongside the derived sets, with `add_user_theme` / `remove_user_theme` (user
  only) / `rename_user_theme` (in place) / `fork` (non-destructive built-in edit)
  / `list` / `theme_def`; missing-id resolution still falls back gracefully.
  meerkat: `theme_store` (load / save / delete user-theme files). The authoring
  UI that drives create/fork/edit is T5. Persistence is a global
  `<mere_root>/themes/` dir for now (per-persona `<persona>/settings/themes/` when
  persona dirs land). Legacy `theme_id` strings still resolve. register-theme 11
  tests + meerkat theme_store 2 tests green.
- **T4 — theme files. (done 2026-06-22.)** A user theme and a community "mod"
  theme are the same serde `ThemeDef` JSON in `<mere_root>/themes/`, discovered +
  loaded into the registry at startup (malformed files skipped + logged, never
  fatal); the picker lists them via `registry.list()`. Format is JSON today
  (consistent with `settings.json`, no new dep; `ThemeDef` serde is
  format-agnostic so TOML is a later swap). Rescan-on-demand deferred (startup +
  on-save covers the live cases). **End-to-end verified**: a hand-authored
  `Sunset` theme file (purple neutral + copper primary) loaded, derived,
  validated, and rendered as the active theme (`t4-01-sunset-mod.png`) — the
  no-recompile mod path works.
- **T5 — authoring UI. (done 2026-06-22.)** A theme editor under the picker on
  the `pelt/appearance` page (`theme_editor_items`): "+ New custom (fork
  current)" always; for the active **user** theme, a Mode toggle + per-seed HSL
  steppers (primary / secondary / tertiary / neutral × Hue/Sat/Light, each with
  a hex readout) + Remove. Built-ins show a "fork to edit" hint (read-only).
  The steppers drain `theme:fork` / `theme:mode` / `theme:remove` /
  `theme:seed:<seed>:<h|s|l>:<down|up>` through `apply_pelt_activation` to new
  `frame_ops` methods (`fork_active_theme` / `remove_active_user_theme` /
  `toggle_active_theme_mode` / `adjust_active_seed`), which mutate the def,
  re-derive + **re-validate** (a contrast-failing edit is dropped, prior theme
  kept), persist the file, and `set_theme` to re-apply live. Steps: 15° hue,
  0.05 sat/light. Deps added to meerkat: `tincture`, `uuid`. meerkat 166 tests
  green. **Headed-verified end to end**: open appearance page → the picker lists
  built-ins + the `Sunset` mod theme → "+ New custom" forks Dark + reveals the
  seed editor → "Hue +"×7 shifts Primary `#6EAAFF → #FF6ED0` and the whole UI
  recolors blue→pink live, persisted. Document-sheet per-role overrides as an
  advanced section deferred (the per-role document knobs exist in
  `DocumentStyleSheet`; surfacing them here is a follow-on).

---

## 11. Decisions

**Resolved (Mark, 2026-06-22):**
- **(1) Engine home — extract a shared crate.** A `strophos-palette` crate
  (color-only, no masonry), consumed by both Woodshed's `audio_widgets` and
  Mere's `register-theme`. **Homing: Mark stands up the repo first**, then I
  build the engine there, git-dep it from Mere, and re-point Woodshed. **T0 is
  blocked until the repo exists** (no committed cross-repo local paths; needs a
  git home). §12 specs the crate API so the build is fast on landing.
- **(2) Built-ins — re-express all as seeds**, including a high-contrast
  derivation mode, and bring over Woodshed's curated set
  (Slate/Ember/Dusk/Meadow/Parchment). One code path; engine proven against
  built-ins. Accept small color shifts from derivation; snapshot-compare to
  catch large drifts (§10 T1).
- **(3) Derivation depth — core derived + long-tail refined.** Derive
  surfaces/text/triad/status from seeds; the radial/badge/hover long-tail derives
  from that base with sensible relationships; per-token overrides for control.

Still defaulted (revisit if needed): HC via a high-contrast derivation mode
(§11.4, falls back to explicit if it can't clear the 7:1 gate); theme files in
**TOML**, bundled dir + `<persona>/settings/themes/` (§11.5); override
granularity a curated subset first (§11.6).

## 11′. Original open-decision notes (superseded by the resolutions above)

1. **Engine home** (§9): port into Mere now (recommended) vs extract a shared
   `strophos-palette` crate now.
2. **Derivation depth**: derive the core (surfaces/text/triad/status) + refine
   the long-tail radial/badge tokens from the base (recommended, Radix-ish), vs
   full-derive every token (most magic), vs keep the long-tail explicit per theme.
3. **Built-ins**: re-express the current 4 as seeds (accept small shifts) vs keep
   the 4 explicit + add new seed-derived themes alongside. (Recommend re-express
   + bring over Woodshed's curated set so the engine is the single path.)
4. **High-contrast**: HC needs 7:1 and pure-black/white. Derive HC with a
   high-contrast mode (forced extremes + contrast-max text), or keep HC explicit
   as the one non-derived built-in? (Recommend a HC derivation mode; fall back to
   explicit if it can't clear the gate.)
5. **Theme-file format + dir**: TOML vs JSON; bundled dir + `<persona>/settings/themes/`
   for user/mod themes. (Recommend TOML, both dirs.)
6. **Token override granularity**: which tokens are user-overridable (all, via a
   `TokenPath`, vs a curated subset). (Recommend a curated subset first, full
   later.)

---

## Progress

- **2026-06-22.** Plan created from Mark's directive ("primary/secondary/tertiary
  like Woodshed's, real authorable themes, as good as Zed"). Woodshed's engine
  read in full (`audio_widgets::theme`) — shared-family-by-design, OKLCH,
  port-friendly (color-only deps). Mere's `register-theme` token set + built-ins
  + contrast validator + the P3 document seam verified in source. No code written;
  gated on §11 sign-off.
- **2026-06-22 (decisions + T0 shipped).** §11 resolved (extract shared crate /
  re-express built-ins as seeds / core-derived + long-tail-refined). Crate named
  **tincture** (rejected `netpalette` — `net*` is Mere's networking prefix;
  `tincture` = the heraldic word for a colour, fits the orrery/gyre/aether
  component-naming pattern). Mark stood up
  [mark-ik/tincture](https://github.com/mark-ik/tincture); **T0 built + pushed**
  there: the OKLCH engine on a dependency-free `Srgb` type, Woodshed's
  `derive_palette` reproduced + the `oklch` primitives exposed, 8 tests + a
  doctest green. Next: **T1** — Mere `register-theme` git-deps tincture
  (`branch=main`) and builds `derive_token_set(seeds) -> ThemeTokenSet` (core
  from the base `Palette`, long-tail refined via `oklch`), re-expressing the 4
  built-ins as seeds and gating on `validate_theme_tokens`. Woodshed re-point is a
  separate follow-up (it keeps working on its in-repo engine until then).
