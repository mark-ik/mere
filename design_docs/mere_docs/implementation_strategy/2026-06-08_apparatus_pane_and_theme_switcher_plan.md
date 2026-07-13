# Apparatus Pane + Runtime Theme Switcher Plan

**Date**: 2026-06-08
**Status**: Planning → building. Greenlit by Mark as the next arc after F1 (the
frame tree). The first **settings/system** consumer of the frame-tree substrate.
**Related**: [frame tree in meerkat](2026-06-08_frame_tree_in_meerkat_plan.md) (the pane substrate), [graph roster + frame taxonomy](../design/2026-06-07_graph_roster_and_frame_taxonomy.md) (apparatus = the system pane; settings folds into it), `register-theme` (the tokens), `apparatus` crate (uxtree skeleton).

Build **apparatus** as a frame pane (host diagnostics + settings) and land the
**light / dark / high-contrast theme switcher** whose chrome tokens the theming
pass already wired. The theme choice persists.

---

## Findings (from the code, 2026-06-08)

- **Chrome theming is wired, switching is not.** `register-theme` has a
  `ThemeRegistry` with Default / Light / Dark / High-Contrast, and meerkat builds
  its chrome CSS + the host-drawn surfaces (window controls) from a resolved
  `ChromeTheme`. But meerkat resolves the *default* once at startup; there is no
  runtime switch and the registry isn't kept.
- **Settings persistence exists.** `session-runtime::settings_store`
  (`PersistedSettings { tab_cap }`) is the sidecar; each field is serde-default,
  so adding `theme_id` reads old files cleanly.
- **The `apparatus` crate is a uxtree a11y skeleton**, not a render view-model.
  So the apparatus *pane* renders in meerkat like the `roster` pane (a genet DOM
  themed from the chrome tokens), not via the domain crate.
- **The orrery's colors are hardcoded** (`build.rs`: `surface_bg`,
  `dark_scene_style`, the `NODE_SHEET` gnode fills). They don't read theme tokens,
  so a theme switch re-themes the chrome + panes but **not** the graph backdrop /
  nodes. Orrery theming is its own task (A2).

---

## Phases

- **A1 — switcher + apparatus pane (this pass).**
  - Theme-switch infra: keep a `ThemeRegistry` + active theme id in App; a
    `set_theme(id)` rebuilds `chrome_theme` + `chrome_sheet`, invalidates the
    host-drawn caches (window controls, divider), redraws, and persists the id.
  - `PersistedSettings.theme_id: Option<String>`; restore on launch.
  - The apparatus pane: a frame leaf (`PaneContent::Apparatus`) summoned beside
    the graph (Ctrl+,), rendered as a genet DOM with a **Theme** section (the
    four themes as buttons, the active one highlighted) and a **System** section
    (diagnostics: node count, active actors, sync status). Theme buttons
    hit-tested like roster rows.
- **A2 — orrery theming (next pass).** Thread the resolved theme's backdrop +
  node/edge palette into the orrery (rebuild `NODE_SHEET` + `dark_scene_style` +
  `surface_bg` from tokens), so the graph re-themes with the chrome.
- **A3 — fold the chrome settings overlay (tab_cap) into apparatus**, retiring the
  command-palette settings overlay (taxonomy: settings lives in apparatus).

---

## Scope boundary for A1

In: runtime theme switching for the chrome + panes, theme persistence, the
apparatus pane (theme buttons + diagnostics), Ctrl+, summon. Out: orrery theming
(A2 — the graph stays on its dark palette, so the Light theme will read as a light
chrome over a dark graph until A2), the settings-overlay migration (A3), a
shellbar (F2; panes summon via keybind for now).

---

## Open decisions

1. **Light theme before A2.** A1 themes the chrome/panes only, so Light = light
   chrome over a dark orrery (a known intermediate). Acceptable to ship A1 and do
   A2 right after. (Lean: yes; flag it to Mark.)
2. **Summon key**: Ctrl+, for apparatus (settings convention), beside Ctrl+R
   (roster), Ctrl+M (maximize). (Lean: yes.)

---

## Progress

- 2026-06-08: Plan written.
- 2026-06-08: **A1 landed + confirmed.** `ThemeRegistry` + `active_theme_id` in
  App; `set_theme` rebuilds the chrome sheet/tokens, drops the host-drawn caches,
  persists; `PersistedSettings.theme_id` (restored on launch). Apparatus pane
  (`apparatus` module, genet DOM) with a Theme section (4 buttons, active
  highlighted) + a System section (nodes / active actors / tab cap / theme),
  Ctrl+, summon, theme-button hit-test. `toggle_roster` generalized to
  `toggle_pane(content)` anchored at the graph leaf (so a second pane nests).
- 2026-06-08: **A2 landed + confirmed.** `Orrery::set_palette(backdrop, edge)`
  themes the graph backdrop + edge stroke (node *state* colors stay semantic);
  the host pushes it at startup + on every switch from the resolved theme
  (`orrery_palette`: background_rgb → backdrop, default_stroke → edge @ 0.85).
  Refinements per Mark: Light backdrop softened (226,230,236) so edges read;
  Dark given its **own** deeper chrome (`ChromeTheme::mere_darker`) + a darker
  backdrop (8,9,13), distinct from Default's slate (Mark: "i do prefer that").
  Theme progression: Default slate → Dark deep → High Contrast pure black.
- A2 caveat retired for the common themes; per-theme **node fills** (HC wanting
  yellow/white nodes) remain a later refinement. A3 (fold the chrome settings
  overlay into apparatus) still pending.
