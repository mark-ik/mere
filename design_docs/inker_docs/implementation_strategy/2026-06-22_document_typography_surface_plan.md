# Document Typography Surface Plan

**Status:** **D1–D3 shipped + headed-verified 2026-06-22.** Document typography
(text size, line spacing, fonts, link adornment) is user-editable in the
`pelt/reading` page, applied live to document-lane cards, and persisted. D4
(per-role + per-engine overrides, full font enumeration) deferred. The spin-out
of the
[document_style_sheet_plan](2026-06-21_document_style_sheet_plan.md) P5's
*typography* half (its colour half shipped as the
[seed_palette_theme_system_plan](../../mere_docs/implementation_strategy/2026-06-22_seed_palette_theme_system_plan.md)).
**Scope:** make the document `DocumentStyleSheet`'s **typography** (base text
size, line spacing, body/mono font, link adornment, and — later — per-role
overrides) **user-editable + persisted**, exposed through a `pelt` settings page,
so reading is customizable the way the theme palette already is. Colours keep
coming from the live theme; this plan owns everything else on the sheet.

Related: the sheet type ([style_sheet.rs](../../../crates/inker/document-canvas/src/style_sheet.rs),
already `Serialize`/`Deserialize` and complete), the content-actor palette seam
(P3, [content.rs](../../../crates/meerkat/src/content.rs)),
[settings_lane_consolidation_plan](../../mere_docs/implementation_strategy/2026-06-21_settings_lane_consolidation_plan.md)
(the `pelt` provider this adds a page to).

---

## 1. The decision in one line

The content actor already carries the document **colours** (`ColorVocabulary`,
P3); widen that to carry the whole composed **`DocumentStyleSheet`** (the host
composes *user typography ⊕ themed colours* and sends it), persist the
typography in the settings sidecar, and add a `pelt/reading` page that edits it
live. Colours stay theme-owned; typography becomes user-owned.

---

## 2. Findings (verified in source 2026-06-22)

- `DocumentStyleSheet` is complete + serde-clean ([style_sheet.rs](../../../crates/inker/document-canvas/src/style_sheet.rs)):
  `body_font_size`, `body_font_family`, `mono_font_family`, `line_height_ratio`,
  `indent_per_level`, `horizontal/vertical_padding`, `link_adornment`, `colors`,
  and per-role `RoleStyles`. Nothing new needed in document-canvas.
- The content actor holds `palette: ColorVocabulary`, set on `Show`, swapped on
  `Retheme`, and read at `layout_document_content` (card.rs) which builds
  `card_sheet(colors) = { colors, ..default() }`. **This is the one seam**: swap
  `palette` for the composed sheet and the live path uses the user's typography.
- `Presentation` holds `document_palette: ColorVocabulary` (themed, recomputed on
  theme change). Add a `document_sheet: DocumentStyleSheet` beside it (the
  typography; its `colors` field is ignored — overwritten at compose time).
- `PersistedSettings` (settings_store.rs) is a flat struct
  (tab_cap / theme_id / shellbar_edge / physics_damping / disabled_engines). Add
  one optional field.
- Fonts resolve by **name** through fontique (the host font system); no
  enumeration API is wired. So v1 offers a **small curated family set** (generic
  + a couple of safe concretes) as a cycle, with fallback; full system-font
  enumeration is a follow-on.

---

## 3. Phases

- **D1 — actor carries the composed sheet.** content.rs: `Content.palette` →
  `Content.sheet: DocumentStyleSheet`; `Show.palette` / `Retheme.palette` →
  `… .sheet`. card.rs `layout_document_content` takes `&DocumentStyleSheet` and
  lays out with it directly (drop the inner `card_sheet` for the live path; keep
  it for the test helpers). constellation.rs `drive` + `retheme` take the sheet.
  No behaviour change yet (host composes `{ colors: document_palette, ..default()
  }`, identical to today).
- **D2 — host owns + persists the typography.** `Presentation.document_sheet`
  (default at startup, loaded from settings). A `composed_document_sheet()`
  helper = `{ colors: document_palette, ..document_sheet }`. `set_theme` composes
  via it; `drive`/`Show` send it. `PersistedSettings.document_typography:
  Option<DocumentStyleSheet>`; `persist_settings` writes it, startup loads it.
- **D3 — the `pelt/reading` page + live edits.** A new settings page (index
  spine entry + `pelt_settings_page` arm). Controls (reusing the slider / button
  widgets): **base text size** (slider → `body_font_size`), **line spacing**
  (slider → `line_height_ratio`), **link arrows** (toggle → `link_adornment`),
  **body font** + **mono font** (cycle over the curated set). Edits drain
  `doc:size` / `doc:linespacing` / `doc:arrows` / `doc:bodyfont` / `doc:monofont`
  → host methods that mutate `document_sheet`, recompose, broadcast `retheme`,
  and persist. A "Reset to defaults" button.
- **D4 — per-role overrides (advanced) + per-engine.** Deferred: an advanced
  section editing `RoleStyles` (heading scale, code wrap, badge italic) and an
  optional per-engine override map. Track here; build when wanted.

---

## 4. Open questions

1. **Font set.** v1 ships a curated list (e.g. system-ui / serif / sans-serif
   for body; monospace / ui-monospace for mono). Full enumeration (fontique
   family list → a real picker) is D4+.
2. **Heading sizes.** The default heading sizes are absolute px. "Base size"
   scales the body, not headings. A "heading scale" knob (multiply the heading
   array) is a D3 stretch or D4; v1 leaves headings fixed.
3. **Scope.** Global sheet for v1 (matches the theme model). Per-engine /
   per-persona is D4 (mirrors the seed-palette + persona threads).

---

## Progress

- **2026-06-22.** Plan created as the document_style_sheet P5 typography spin-out,
  grounded in source (sheet complete + serde; the content-actor palette seam is
  the single widen-point; PersistedSettings is flat; fonts resolve by name). No
  code written.
- **2026-06-22 (D1–D3 shipped + headed-verified).** Built end to end:
  - **D1** — the content actor carries the composed `DocumentStyleSheet` instead
    of a bare `ColorVocabulary`: `ContentCommand::Show`/`Retheme` + `Content` hold
    `sheet`; `card::render_content` / `layout_document_content` /
    `render_content_scene` take `&DocumentStyleSheet` and lay out with it
    directly; `constellation::drive`/`retheme` take the sheet. `lower_window` (the
    host band-lowering) still takes `ColorVocabulary` (the packet is already laid
    out by the actor; lowering only needs colours).
  - **D2** — `Presentation.document_sheet` (typography) beside `document_palette`
    (theme colours) + `document_sheet_composed()` = `{ colors: palette,
    ..document_sheet }`, the one compose point; `drive` / `set_theme` / the
    snapshot path all send it. Persisted as embedded JSON in
    `PersistedSettings.document_typography` (no session-runtime → document-canvas
    dep; the host owns (de)serialization; written only when it differs from the
    built-in look), loaded at startup.
  - **D3** — the `pelt/reading` page (`settings_lane`): **text size** (10–24px) +
    **line spacing** (100–200%) sliders, a **link-arrows** toggle, curated
    **body** / **code** font lists (`doc_style::{BODY_FONTS, MONO_FONTS}`), and a
    reset. Controls drain `doc:*` → `crate::doc_style` edit methods (mutate
    `document_sheet`, re-compose, broadcast `retheme`, drop themed snapshots,
    persist, redraw). `doc_style.rs` is the new module (mirrors `theme_edit`).
  - Tests green: meerkat 66 + 100, session-runtime 68. **Headed:** the Reading
    page renders all controls; pushing text size to 22px + switching body font to
    Georgia re-laid the `mere://welcome` document card to **serif at 22px** live,
    while the HTML lane (example.com) correctly ignored it. **D1–D3 complete.**
