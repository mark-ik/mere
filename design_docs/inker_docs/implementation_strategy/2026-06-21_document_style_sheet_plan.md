# Document Style Sheet Plan

**Status:** **P0–P4 complete 2026-06-22.** The engine half (per-role sheet,
glyph-level colour, link adornment, per-role wrap, live theme sourcing) is built,
tested, and headed-verified. **P5 (customization surface) is reframed:** its
colour/theme half shipped as the
[seed_palette_theme_system_plan](../../mere_docs/implementation_strategy/2026-06-22_seed_palette_theme_system_plan.md)
(T0–T5 + harmony, complete); its remaining unique part — a **document-typography**
surface (font pickers, size/scale, line-height, link-adornment toggle, optional
per-engine override) — spun out and **built** as the
[document_typography_surface_plan](2026-06-22_document_typography_surface_plan.md)
(D1–D3 shipped 2026-06-22; D4 per-role/per-engine deferred). Codebase-grounded against
`crates/inker/document-canvas/` as it stands today.
**Scope:** promote `document-canvas`'s flat `StyleConfig` into a per-role
**`DocumentStyleSheet`** so every document-engine output (smolweb, markdown,
gopher, feed, knot) can be styled per block-kind, sourced from the live theme,
and exposed as a user-editable customization surface. This is the
"customizable like Geopard" capability, generalized past Gemtext and past one
hardcoded look.

External reference: [Geopard](https://github.com/ranfdev/Geopard) (GTK4 Gemini
browser). Its whole customization story is a small role table (`h1`/`h2`/`h3`/
`p`/`a`/`q`/`pre` TextTags) bound by a `Config` of four font buckets, with
colors inherited from the desktop theme (libadwaita `accent_color`). We already
have the two hard layers (a clean parse and a parley layout); the gap is the
role-table binding and color emission. See §2.

Related: [interaction_model_spine](../../mere_docs/technical_architecture/2026-06-18_interaction_model_spine.md)
(this is the *render* + *represent* stages of the pipeline, document side),
[retained_text_tiled_render_plan](../../mere_docs/implementation_strategy/2026-06-15_retained_text_tiled_render_plan.md)
(consumes the same `DocumentRenderPacket` this restyles; restyle-vs-relayout
question, §6), [engine_picker_and_pluggability_plan](2026-06-15_engine_picker_and_pluggability_plan.md)
(sibling inker plan; per-engine profile pattern this borrows),
[apparatus_pane_and_theme_switcher_plan](../../mere_docs/implementation_strategy/2026-06-08_apparatus_pane_and_theme_switcher_plan.md)
+ `register-theme` (the theme token source for §4),
[settings_lane_consolidation_plan](../../mere_docs/implementation_strategy/2026-06-21_settings_lane_consolidation_plan.md)
(the `pelt` provider appearance page where the customization surface lands, §5).

---

## 1. The decision in one line

Replace the narrow, hardcoded typography in
[layout.rs](../../../crates/inker/document-canvas/src/layout.rs) (one body
family, one mono family, six heading sizes, one line-height) with a
**role-keyed style sheet**: a table from semantic block/inline role to a full
style descriptor (family, weight, size-or-scale, color token, italic, wrap,
spacing, indent). The layout dispatch resolves each block against the sheet
instead of reading scattered `StyleConfig` fields. Colors resolve from the live
theme, not baked literals. The sheet serializes to config and is the
customization surface.

This is Geopard's TextTag-plus-`Config` model, generalized to all of Mere's
document engines and emitted through parley glyph layout instead of a GTK
`TextView`.

---

## 2. Findings (verified in source 2026-06-21)

### 2.1 What exists today

| Layer | Where | State |
|---|---|---|
| Gemtext parse → semantic blocks | `engines/nematic/src/gemtext.rs` | clean; no styling, same separation Geopard's `gemini/src/parser.rs` keeps |
| Document model | `inker/src/document.rs` | `DocumentBlock` (12 variants), `InlineSpan` (7 variants) |
| Layout dispatch | [layout.rs:101-176](../../../crates/inker/document-canvas/src/layout.rs) | per-variant `render_block`; styling hardcoded at each renderer |
| Parley wrapper | [text.rs:163-336](../../../crates/inker/document-canvas/src/text.rs) | ranged builder, glyph-run extraction, link hit-regions |
| Style knobs | [style.rs](../../../crates/inker/document-canvas/src/style.rs) | `StyleConfig` (flat), `ColorVocabulary`, `InlineStyle` (parley brush = flags only) |
| Theme tokens (UI chrome) | `system/registry/register-theme` | `ThemeRegistry` / `ThemeTokenSet`; separate from document style today |

### 2.2 The three gaps that block "customizable like Geopard"

1. **Coarse role binding.** `StyleConfig` exposes `body_font_family` +
   `mono_font_family` only ([style.rs:33-35](../../../crates/inker/document-canvas/src/style.rs)).
   Geopard lets a user set quote ≠ paragraph ≠ heading font. We cannot. The
   block renderers each hand-build a `TextBaseStyle` from fixed fields:
   heading at [layout.rs:192-199](../../../crates/inker/document-canvas/src/layout.rs)
   (bold + body family), code at [layout.rs:281-288](../../../crates/inker/document-canvas/src/layout.rs)
   (mono), paragraph at [layout.rs:113-123](../../../crates/inker/document-canvas/src/layout.rs),
   metadata-row at [513-520](../../../crates/inker/document-canvas/src/layout.rs),
   badge at [542-549](../../../crates/inker/document-canvas/src/layout.rs). These
   five sites are the resolution points to route through a sheet.

2. **Color is not emitted at the glyph level.** `GlyphRun`
   ([text.rs:120](../../../crates/inker/document-canvas/src/text.rs)) carries
   `font_face`/`size`/`weight`/`style` but **no brush or color**. The per-range
   styling loop pushes `InlineStyle` as parley's brush, but `InlineStyle` is
   just flags (italic/bold/monospace/link), and the link case explicitly punts:
   "*Links don't have an explicit visual style here ... A future slice could
   push an Underline + colored brush*" ([text.rs:214-216](../../../crates/inker/document-canvas/src/text.rs)).
   `ColorVocabulary`'s own doc comment confirms it is forward-looking:
   "*future glyph emission reads body_text / heading_text / link_text /
   code_text / badge_text for per-block-kind text color*"
   ([style.rs:36-40](../../../crates/inker/document-canvas/src/style.rs)). So
   per-role color is a build item, not a config rename.

3. **No theme coupling.** `ColorVocabulary::default()` is literal near-black
   ([style.rs:88-99](../../../crates/inker/document-canvas/src/style.rs)). It
   does not follow `register-theme`'s light/dark/high-contrast seeds, so
   documents do not re-theme with the rest of the shell. Geopard gets this for
   free from libadwaita; we have the token set already and just need to source
   from it.

### 2.3 What is already right (keep it)

The parse/layout split is correct and matches Geopard's. Do not introduce a CSS
engine for this: smolweb is word-processor-faithful, and CSS cascade is
Serval's job for HTML. A role-keyed sheet is the right altitude. The
`InlineStyle`-as-brush seam in parley is the right place to carry color once
the brush grows a color field (§3.3).

---

## 3. The shape

### 3.1 The style sheet (illustrative-signature-only; not compile-ready)

```rust
pub enum BlockRole {
    Body, Heading(u8), Link, ListItem, Quote, Code, Badge,
    MetadataLabel, MetadataValue, FeedTitle, // map the existing 12 variants
}

pub struct BlockStyle {
    pub family: FontChoice,      // Explicit(name) | InheritBody | InheritMono
    pub weight: u16,
    pub size: SizeSpec,          // Absolute(px) | ScaleOfBody(f32)
    pub italic: bool,
    pub color: ColorToken,       // resolved against the theme, not an RGBA literal
    pub wrap: WrapPolicy,        // Wrap | NoWrap  (NoWrap → renderer h-scrolls)
    pub spacing_above: f32,
    pub spacing_below: f32,
    pub indent: f32,
}

pub struct DocumentStyleSheet {
    roles: BTreeMap<BlockRole, BlockStyle>,   // a `default()` reproduces today's output
    // global metrics that are not per-role: viewport padding, indent_per_level,
    // line_height_ratio default. Fold the remainder of StyleConfig in here.
}
```

`ColorToken` is an enum of semantic names (`BodyText`, `HeadingText`,
`LinkText`, `CodeText`, `BadgeText`, `Rule`) resolved by a `ThemeResolver` at
layout time (§4), not a raw `[f32; 4]`. A single field change re-themes one
role, which is the property `ColorVocabulary`'s comment already aims at.

### 3.2 Resolution (the seam)

`layout_document` gains the sheet (replacing or wrapping `&StyleConfig`). Each
`render_*` builds its `TextBaseStyle` from `sheet.resolve(role)` instead of
fixed fields. `render_block`'s match
([layout.rs:107-175](../../../crates/inker/document-canvas/src/layout.rs)) is
the one dispatch point that maps `DocumentBlock` variant → `BlockRole`. No new
control flow; the five hand-built `TextBaseStyle` sites become one helper.

### 3.3 Color emission

`TextBaseStyle` + `InlineStyle` grow a resolved color, and `GlyphRun` grows a
`color: [f32; 4]` (premultiplied, the format
[`ColorVocabulary`](../../../crates/inker/document-canvas/src/style.rs) and
`netrender::Scene` already use). The per-range loop at
[text.rs:198-218](../../../crates/inker/document-canvas/src/text.rs) sets the
link range's brush color from `LinkText` and (optionally) an underline. This
closes the §2.2(2) gap and is where the link-color TODO lands.

### 3.4 Link adornment by destination (Geopard touch)

Geopard prefixes `⇒` for in-protocol links and `⇗` for external. That is a
render-time choice keyed on the link URL scheme, decided where `InlineSpan::Link`
is flattened ([text.rs:98-107](../../../crates/inker/document-canvas/src/text.rs)).
A sheet option (`link_adornment: None | SchemeArrow | custom`) makes it
configurable. Cheap; fits the same pass.

---

## 4. Theme sourcing

A `ThemeResolver` maps `ColorToken` → `[f32; 4]` from the active
`ThemeTokenSet` in `register-theme`. document-canvas takes the resolver (or a
resolved color table) as a layout input, so switching theme (the apparatus
theme switcher) re-themes documents in the same gesture. This is the richer
analog of Geopard's single `colors: bool` + accent inheritance: light / dark /
high-contrast seeds already exist, so documents join them rather than carrying a
parallel palette. Ties to
[apparatus_pane_and_theme_switcher_plan](../../mere_docs/implementation_strategy/2026-06-08_apparatus_pane_and_theme_switcher_plan.md)
(A3 + per-theme HC fills).

---

## 5. The customization surface

The sheet is `Serialize`/`Deserialize` (as `StyleConfig` already is), so it is
TOML-shaped out of the box. Expose it through the settings lane's `pelt`
(global appearance) provider per
[settings_lane_consolidation_plan](../../mere_docs/implementation_strategy/2026-06-21_settings_lane_consolidation_plan.md):
font-per-role pickers, size/scale, line-height, link adornment, plus the theme
selection that drives §4. This is exactly Geopard's `~/.config/geopard/`
config, given a UI.

Open: one global sheet vs a per-engine override map (a Gemtext sheet distinct
from a markdown sheet). Recommend a base sheet plus an optional per-engine
override keyed by `engine_id`, mirroring the engine-profile pattern in
[engine_picker_and_pluggability_plan](2026-06-15_engine_picker_and_pluggability_plan.md).
First pass: base sheet only. Track the full space (per-engine, per-persona,
per-moot) in this plan even while the first cut is narrow.

---

## 6. Phases

- **P0 — sheet type + default parity. (done 2026-06-21.)** Added
  `style_sheet.rs`: `DocumentStyleSheet` + `BlockStyle` / `HeadingStyle` /
  `RoleStyles` / `BlockRole` / `ColorToken` / `FontChoice` / `SizeSpec` /
  `WrapPolicy` / `ResolvedBlockStyle`, built as a projection of `StyleConfig`
  (`from_style_config`, and `default()` = projection of `StyleConfig::default()`).
  No call-site changes. 11 parity tests pin every role's resolved values to the
  current hardcodes; all 28 existing document-canvas tests still green
  (`cargo test -p document-canvas`, 39 passed). See the Progress log for the
  structural notes P1 inherits.
- **P1 — route layout through the sheet. (done 2026-06-22.)** `layout_document`
  now takes `&DocumentStyleSheet`; the five hand-built `TextBaseStyle` sites and
  the `render_block` dispatch resolve via `sheet.resolve(role)` through a
  `text_base_from` helper; image/rule spacing reads `block_spacing()`,
  `render_image` reads the sheet's `line_height`. `StyleConfig` is fully retired
  (struct + impls removed from `style.rs`, dropped from the `lib.rs` re-export;
  `ColorVocabulary` + `InlineStyle` stay). Callers moved: platen
  `build_document_scene`, meerkat `card.rs`, and every document-canvas test.
  Done in two compile-checked sub-steps (route, then retire). All green:
  document-canvas 39 (default) / 42 (netrender), platen 75, meerkat
  `cargo check` clean. Byte-identical confirmed by the unchanged layout/paint
  geometry assertions still passing.
- **P2 — color emission. (color half done 2026-06-22; adornment remaining.)**
  `GlyphRun` now carries `color`; `TextBaseStyle` carries the role base color;
  `layout_text_block` computes per-run color by brush role (link → `LinkText`,
  inline code → `CodeText`, else base) using parley's existing brush-boundary
  run segmentation; `paint_list` lowers `run.color` instead of a flat
  `body_text`. Closes §2.2(2). **Meerkat correctness:** text color now comes
  from the sheet, so meerkat builds `card_sheet()` (= built-in typography +
  `card_vocabulary()`); this lights up the per-role heading/link/code/badge
  colors Mark already defined but that were dead (paint_list only read
  `body_text` before) — a visible improvement (links go white → blue on the
  dark card), bulk body text unchanged. **P2b done 2026-06-22:** the `⇒`/`⇗`
  link adornment landed as a `LinkAdornment` sheet option (default
  `SchemeArrow`, matching the Geopard reference; configurable to `None`). `⇒`
  (U+21D2, plus a space) prefixes in-protocol / relative links, `⇗` (U+21D7)
  links that leave the document's protocol; classification derives the base
  scheme from the
  document's own address, so it is host-agnostic. The prefix is injected during
  inline flattening, styled + byte-ranged as part of the link (link-colored,
  inside the hit region). Only the document lane is affected (HTML keeps its CSS).
- **P3 — theme sourcing. (done 2026-06-22.)** Documents re-theme with the shell.
  Closes §2.2(3). **Approach (code-verified):** document-canvas stays
  theme-agnostic (it already takes a resolved `ColorVocabulary` via the sheet —
  P1/P2). The host owns the mapping. Crucial finding: document text color is
  **baked into the packet inside the content actor** (`content.rs`, which has no
  theme), and the card **background** (`CARD_BG`) is cleared in `render.rs` (which
  does) — and the two are coupled (theme the bg without the text and light-theme
  text goes invisible). So P3 spans ~6 files:
  1. `main.rs`: `document_palette(tokens) -> ColorVocabulary` +
     `card_background(tokens) -> wgpu::Color` from `ThemeTokenSet` (mirrors
     `orrery_palette`); map `chrome.body_text`→body, `chrome.strong_text`→heading,
     `theme_data.accent_rgb`→link, `chrome.muted_text`→badge/placeholder/rule,
     `chrome.surface_bg`→card bg. **Open: code_text** has no dedicated token —
     default to `chrome.body_text` (monospace font carries the distinction).
     Store both on `shared.presentation`; set at startup + on theme change.
  2. `content.rs`: `Content` gains `palette: ColorVocabulary`; `Show` carries it;
     new `Retheme { palette }` updates + re-renders (live theme switch).
  3. `card.rs`: `card_sheet` / `render_content` / `render_content_scene` /
     `recovering_card_scene` / `lower_window` take the palette; retire the
     hardcoded `card_vocabulary()`.
  4. `render.rs`: rasterize `Clear` uses the themed card bg; `lower_window` passes
     the palette.
  5. `frame_ops.rs`: theme change recomputes the palette + bg and broadcasts
     `Retheme` to content actors.
  6. command-dispatch site: `Show` stamps the current palette.
- **P4 — per-role wrap. (done 2026-06-22.)** Threaded `WrapPolicy` from the
  resolved role through `TextBaseStyle` into `layout_text_block`: `NoWrap` calls
  `break_all_lines(None)` so the block lays out on its natural width and
  overflows (for the host to scroll); `Wrap` constrains to the content width.
  Default sheet stays `Wrap` for every role (no behavior change; `NoWrap` becomes
  usable when the host adds horizontal scroll for document cards). Per-role
  spacing was already honored (P1). Test: `nowrap_role_overflows_instead_of_wrapping`
  (dc 47 / 50 netrender). Per-role *indent* (vs the single `indent_per_level`
  global) deferred — no consumer yet.
- **P5 — customization surface. (reframed; colour half shipped, typography half
  spun out 2026-06-22.)** The sheet is already `Serialize`/`Deserialize` (P0). The
  **colour/theme** half — sourcing document colours from a user-editable,
  mod-authorable palette — shipped in full as the
  [seed_palette_theme_system_plan](../../mere_docs/implementation_strategy/2026-06-22_seed_palette_theme_system_plan.md)
  (seeds → `ThemeTokenSet`, user themes, theme files, the `pelt` appearance
  editor with sliders + accent harmony); documents consume it via the P3
  `document_palette` seam unchanged, so "customizable like Geopard" is met for
  colour. The **typography** half remains: a `pelt` section exposing
  per-role font / size-scale / line-height pickers, the link-adornment toggle,
  and an optional per-engine override map, backed by a persisted
  `DocumentStyleSheet` (today meerkat uses a fixed `card_sheet()`). No consumer
  pressure yet; spun out as a focused follow-on rather than built at the tail of
  this plan. Track here until it gets its own plan.

---

## 7. Open questions

1. **Restyle vs relayout** (connects [retained_text_tiled_render_plan](../../mere_docs/implementation_strategy/2026-06-15_retained_text_tiled_render_plan.md)).
   A color-only change need not re-run parley if color lives on `GlyphRun` and
   the renderer can recolor a retained packet. A family/size change forces
   relayout. Decide whether the sheet split is "metrics (relayout) vs paint
   (recolor)" so the retained-render path can fast-path recolors.
2. **Color on `GlyphRun` vs role tag on `RenderedBlock`.** Carrying `[f32;4]`
   per run is simplest for the renderer; a role tag is smaller and defers color
   to paint time. P2 picks one; recommend color-on-run for renderer simplicity,
   revisit if the recolor fast-path (Q1) wants the tag.
3. **Sheet scope.** Global vs per-engine vs per-persona (§5). First cut global.
4. **Inline vs block roles.** `InlineStyle` is the brush; block roles set the
   base. Confirm the override order (inline link color wins over block body
   color within a paragraph) matches the parley range-stack merge already in use
   ([text.rs:198-218](../../../crates/inker/document-canvas/src/text.rs)).

---

## Progress

- **2026-06-21.** Plan created from a session question ("display smolweb text
  customizably like Geopard"). Geopard's model read from source
  (`src/widgets/pages/hypertext.rs`, `src/config.rs`): a `TextView` + named
  TextTags (`h1`/`h2`/`h3`/`p`/`a`/`q`/`pre`) bound by a four-bucket font
  `Config`, colors from libadwaita. Mere-side grounding verified in
  `document-canvas`: parse/layout split already matches; three gaps identified
  (coarse role binding, no glyph-level color, no theme coupling) with file:line
  resolution sites. No code written.
- **2026-06-21 (P0 — green).** Added `crates/inker/document-canvas/src/style_sheet.rs`
  (~310 LOC, under the 600 ceiling; `style.rs` left focused), wired into
  `lib.rs` + re-exported; `serde_json` added as a workspace dev-dep for the
  round-trip test. `cargo test -p document-canvas`: 39 passed (11 new parity +
  28 existing), warning-free. Structural notes P1 inherits:
  - **Projection-as-definition.** The sheet is `from_style_config(&StyleConfig)`
    and `default()` is the projection of `StyleConfig::default()`, so parity is
    true by construction and locked by tests. P1 swaps the five call sites to
    `sheet.resolve(role)` and the `render_block` dispatch to a role mapping,
    then retires `StyleConfig`'s now-duplicated fields (DOC_POLICY §3).
  - **Heading is its own shape.** Heading size is intrinsically per-level, so a
    distinct `HeadingStyle { sizes: [f32;6], … }` rather than a `BlockStyle`
    with one `SizeSpec`. `resolve(Heading(level))` clamps 1..=6 exactly like
    `StyleConfig::heading_size`.
  - **Containers + non-text blocks deferred.** Quote / List are not text roles
    (indent only, still via the `indent_per_level` global). Image / Rule
    spacing is bridged for P1 by `block_spacing()` (= body `spacing_below` =
    today's `paragraph_spacing`); promote to their own roles only if a use
    appears. `BlockRole` is the five text-bearing roles (Body / Heading / Code /
    Metadata / Badge); inline Link color is P2 (the `ColorToken::LinkText` slot
    exists).
  - **Byte-identical encodings.** `weight >= 700` reproduces the `bold: bool`
    flag; `monospace` derives from `FontChoice::InheritMono`; badge `0.85` and
    metadata/badge `* 0.5` spacing are the same f32 expressions the renderers
    use, so the resolved values match bit-for-bit.
- **2026-06-22 (P1 — green).** Routed `layout_document` through
  `&DocumentStyleSheet` and retired `StyleConfig`. Sub-step 1 (route): swapped
  the entry-point + `DocumentLayouter` signatures, added the `text_base_from`
  helper, mapped the heading / paragraph / code / preformatted / metadata /
  badge sites to `resolve(role)`, and pointed image/rule spacing at
  `block_spacing()` + the new `DocumentStyleSheet::line_height`; updated platen,
  meerkat, and all document-canvas tests; `cargo test -p document-canvas` +
  `-p platen` + `cargo check -p meerkat` green with `StyleConfig` still present
  as the projection source. Sub-step 2 (retire): removed the `StyleConfig`
  struct + impls from `style.rs`, dropped it from the `lib.rs` re-export,
  inlined `default()` with literals (dropping `from_style_config`), and rewrote
  the parity tests to assert the built-in defaults directly (against literals +
  `ColorVocabulary::default()`). Re-validated: document-canvas 39 / 42
  (netrender), platen 75, meerkat clean. Notes for P2: `GlyphRun` still carries
  no color, so the `ResolvedBlockStyle::color` + `wrap` fields are computed but
  unused at the call sites (the `text_base_from` doc records this); P2 grows
  `GlyphRun.color` and wires `ColorToken` → run color + the link-color path at
  [text.rs:214-216](../../../crates/inker/document-canvas/src/text.rs). The
  meerkat binary could not be relinked during validation (the running app holds
  a lock on `meerkat.exe`); `cargo check` is the clean substitute and the
  compile is proven.
- **2026-06-22 (P2 — color half, green).** Threaded per-run color end to end.
  `GlyphRun.color` + `TextBaseStyle.color` added; `text_base_from` carries the
  resolved role color; `layout_text_block` gained `link_color` / `code_color`
  params and picks per run from the brush (`brush.link` → link, else
  `brush.monospace` → code, else `base.color`) — parley already splits runs at
  brush boundaries, so a link / inline-code span is its own colored run with no
  extra geometry work; `render_flattened_with_spacing` sources the two inline
  colors via the now-public `DocumentStyleSheet::token_color`. `paint_list`'s
  `DrawText` lowers `run.color`. Two tests added: `glyph_runs_carry_per_role_colors`
  (layout — heading/body/link/code runs carry their token colors) and
  `draw_text_carries_per_run_role_color` (paint_list — the lowering preserves
  it and the roles differ). **Meerkat:** added `card_sheet()` (built-in
  typography + `card_vocabulary()`); the three `layout_document` sites
  (`layout_document_content` live, `render_card_scene` + `recovering_card_scene`
  test/decoration) use it; the per-band `lower_window` inherits the baked colors
  automatically and still passes `card_vocabulary()` for rule/image. Net
  behavior change to verify visually: card links/code/badges/headings now paint
  their distinct card-palette colors instead of all body_text (links white →
  blue); bulk body text unchanged. All green: document-canvas 41 / 44
  (netrender), platen 75, meerkat `cargo check` clean.
- **2026-06-22 (P2 — headed verify).** Built + drove meerkat (scry-shots
  harness, `drive-stylep2.ps1`); captured the `mere://welcome` document-lane
  card (`stylep2-11-recent.png`). Confirms: (1) no regression — card text is
  light-on-dark and readable, so sourcing text color from the sheet did not
  flip it to invisible black (the main risk of routing color through
  `card_sheet`); (2) per-role color is live — the "Mere" heading renders
  brighter (`heading_text`) than the grey body (`body_text`), where pre-P2 both
  were the same `body_text` grey. HTML-lane cards (iana / example.com) render
  via their own CSS, unaffected. The welcome card carries no links, so the
  white→blue link color is not in the shot; it is covered by the
  `glyph_runs_carry_per_role_colors` unit test.
- **2026-06-22 (P2b — link adornment, green).** Added the `LinkAdornment` sheet
  option (`None` | `SchemeArrow`; default `SchemeArrow`) +
  `DocumentStyleSheet.link_adornment`, re-exported from the crate root.
  `prefix_for(url, base_scheme)` returns the in-protocol (U+21D2) or external
  (U+21D7) arrow + space; `url_scheme` + `link_is_external` classify against the
  document's base scheme. `flatten_inline` / `flatten_into` gained `adornment` +
  `base_scheme` params; the prefix is pushed into the flattened text with the
  link's `InlineStyle` and included in the link byte range, so it inherits link
  color + the hit region. `layout_document` derives the base scheme from
  `document.address`; `DocumentLayouter` carries it. Five flatten tests added
  (`text.rs`): none / in-protocol / external / relative + prefix-carries-link-style.
  All green: document-canvas 46 / 49 (netrender), platen 75, meerkat
  `cargo check` clean. Visual check deferred: arrows show on document-lane
  links, but a link-bearing document card was finicky to reach in the headed run
  (welcome has none; reachable cards were the HTML lane); covered by the flatten
  unit tests. **P2 complete.**
- **2026-06-22 (P3 — theme sourcing, full incl. live retheme, green).** Wired
  the document palette + card background to the active theme across 6 files.
  `main.rs`: `document_palette(tokens)` (chrome `body`/`strong`/`muted` + accent
  → the `ColorVocabulary`; code = body, monospace carries it) + `vocab_color` +
  `chrome_to_wgpu`; `Presentation.document_palette` set at startup. `ColorVocabulary`
  gained `Copy` so it threads by value. `content.rs`: `Content.palette`, `Show`
  carries it, new `Retheme { palette, viewport_gen }` re-bakes live. `card.rs`:
  `card_sheet` / `render_content` / `render_content_scene` / `recovering_card_scene`
  / `lower_window` take the palette; `card_vocabulary()` kept only as the test
  default. `constellation.rs`: `drive` stamps the palette on `Show`; new
  `retheme(palette)` broadcasts to active actors with a bumped viewport gen (so
  the re-baked packet clears the generation gate → `scene_version` bumps →
  re-rasterize). `render.rs`: card-clear uses `chrome_to_wgpu(surface_bg)`;
  `lower_window` / `render_content_scene` / `recovering_card_scene` / `drive`
  pass the palette. `frame_ops.rs::set_theme`: recompute palette + broadcast
  `retheme`. **Gap found + fixed via the headed run:** the apparatus pane showed
  *Active actors: 0* — the focus-card **snapshots** (cached data-URIs) have no
  live actor to re-bake, so `set_theme` now also clears `snapshot_data_uris`
  (they rebuild themed on next focus). All green: document-canvas 46 / 49
  (netrender), platen 75, meerkat 164 (incl. the theme-switch agent-harness
  test). Headed verify: the `mere://welcome` card renders correctly with the
  theme-derived palette at startup (`p3-01-welcome-dark.png`), no regression.
  Not visually captured: the live dark→light flip on switch — the theme switcher
  lives in the pelt settings lane (a dedicated tile arm, not a URL-nav focus
  card) and was impractical to drive blind; the live-retheme chain is covered by
  the `retheme` broadcast + `Retheme` handler + version-bump logic + the snapshot
  clear. **P3 complete; document style sheet P0–P3 done.**
- **2026-06-22 (P4 — per-role wrap, green).** `WrapPolicy` now threads through
  `TextBaseStyle` into `layout_text_block` (`NoWrap` → `break_all_lines(None)`,
  natural width + overflow; `Wrap` constrains to content width). Default stays
  `Wrap` everywhere (no behavior change). Test added; dc 47 / 50 (netrender).
  **P4 complete.** P5 reframed by Mark into a bigger initiative: a
  primary/secondary/tertiary **seed-palette** extensible theme system
  (Woodshed/Zed-grade, mod-authorable) — spun to its own plan since it spans
  `register-theme` (chrome + orrery + documents), not just the document sheet.
  The document sheet consumes its output via the P3 `document_palette` seam
  unchanged.
- **2026-06-22 (plan closeout).** P0–P4 done, tested, and headed-verified; the
  engine is complete. P5 reframed: the colour/theme customization shipped in full
  as the seed-palette plan (T0–T5 + accent harmony + the `pelt` appearance editor
  + the readable-on-accent pass), which documents already consume via
  `document_palette`. The one genuine remainder is the **document-typography**
  customization surface (per-role font / size / line-height pickers, link-
  adornment toggle, per-engine override over a persisted `DocumentStyleSheet`);
  it has no consumer pressure and is spun out as a follow-on (own plan when
  pursued), not built here. **This plan is complete for its engine scope.**
