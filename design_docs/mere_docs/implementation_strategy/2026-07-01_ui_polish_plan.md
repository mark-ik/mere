# UI Polish Plan (scaling coherence, tile tabs, session chips)

**Date**: 2026-07-01
**Status**: planning. Findings verified headed 2026-07-01 (driven session, shots in `scry-shots/ui-01-baseline.png`, `ui-02-chrome-zoom.png`, `ui-03-canvas-zoom.png` + crops).
**Related**: [chrome_bar_refinement_plan](2026-06-26_chrome_bar_refinement_plan.md) (P4 moved sessions into toolbar chips, closeout retired the switcher thumbnails), [ui_dpi_scaling_plan](2026-06-26_ui_dpi_scaling_plan.md) (D1-D3 landed; this plan covers the surfaces it missed), [gloss_scene_to_dom_migration_plan](2026-07-01_gloss_scene_to_dom_migration_plan.md) (§Progress, the originating session for finding 5's paint-cost measurement + the screenshot-harness fix this plan's own driven verification reused), `crates/meerkat/` (`views.rs`, `window_view/views.rs`, `tile_theme.rs`, `render/workbench.rs`, `render/orrery_scene.rs`, `app_state.rs`), `repos/serval/ports/pelt-desktop/tile_surface.rs`.

Three of Mark's asks (2026-07-01): text that "doesn't want to scale", pelt tile tabs mis-proportioned and clipped at the toolbar, and the session indicator regressing from graph thumbnails to a big orange pill. Plus a toolbar-overflow defect found while verifying, and the paint-cost finding documented here so it lives in design_docs.

## Findings

### 1. Scaling is split-brain: only the chrome sheet scales

`ui_scale()` (dpi × user_zoom, [app_state.rs:232](../../../crates/meerkat/src/app_state.rs#L232)) is applied by rewriting px values in the chrome sheet (`scale_px` in `rebuild_chrome_sheet`, [app_state.rs:246](../../../crates/meerkat/src/app_state.rs#L246)). Everything styled outside that sheet never scales:

- **Pelt tile surface.** `DEFAULT_TILE_CSS` in [tile_surface.rs:183](../../../../serval/ports/pelt-desktop/tile_surface.rs#L183) is fixed px (tabbar 36px, tab padding 5/10, default font size), and meerkat's theme layer (`tile_sheet`, [tile_theme.rs:11](../../../crates/meerkat/src/tile_theme.rs#L11)) sets colors only. At dpi ~2 the tab bar renders at half the chrome's visual scale; Ctrl+zoom moves the chrome and leaves the tabs alone (verified: ui-01 vs ui-02).
- **Orrery node cards.** `node_card_view` uses inline styles: face size from `node_size` (world px, default 36) and a hardcoded `font-size:14px` label ([window_view/views.rs:292](../../../crates/meerkat/src/window_view/views.rs#L292)). Inline px bypass `scale_px`, so DPI and Ctrl+zoom never touch canvas text: 14 physical px on a 2x panel.
- **Canvas zoom scales positions only.** The card snapshot multiplies node *positions* by `cam.zoom` ([render/orrery_scene.rs:139](../../../crates/meerkat/src/render/orrery_scene.rs#L139)) but `node_size` and the label are zoom-independent, so Ctrl+wheel spreads or packs the constellation without magnifying anything (verified: ui-03, a zoomed-in node renders the same 24px face and tiny label).
- **Content ignores user_zoom.** Page text tracked DPI (D2a) but did not change under Ctrl+= (ui-02). Browser convention is that Ctrl+zoom also zooms content. Open question OQ-1 below.

### 2. Tile tab text clips at the tab's top edge

Verified at default zoom (full-res crops of ui-01/ui-03): the tab pill renders ~24px tall against the intended 36px tabbar, and the label's ascenders are cut at the pill's top edge. Because the tab bar sits flush under the toolbar with zero inset, the result reads as "the toolbar clipped my tab". Mechanism not yet pinned: fixed-height flex container + `align-items: center` + inline text in serval-layout, either a line-box vertical-alignment defect (fix in the engine, standards-correct preference) or CSS that needs an explicit `line-height`. The unscaled sheet (finding 1) compounds the bad proportions.

### 3. Session chip: a big wrapped pill where thumbnails used to be

The chip ([views.rs:367](../../../crates/meerkat/src/views.rs#L367)) renders the session label (often a raw URL like `settings://node:8ea45...`) in a filled accent-colored pill; long labels wrap to two lines, producing the tall orange blob that hangs below the toolbar row. The shellbar-era switcher rasterized a mini graph thumbnail per session (`build_switcher_thumbnail_with`, removed in the chrome-bar closeout 2026-06-26; label-only `refresh_session_labels` survives at [session_ops/windowctx.rs:221](../../../crates/meerkat/src/session_ops/windowctx.rs#L221)). Mark prefers the thumbnails and there is no strong reason the toolbar chip cannot carry one.

Also observed (ui-01 vs ui-03): right after a chip-list change the toolbar can composite a stale layout, the chip painting over the omnibar until a later relayout settles it.

### 4. Toolbar overflow at higher zoom

At user_zoom 1.4 on the 2x panel (ui-02) the window-controls strip (minimize/maximize/close) paints over `+tile` / `+field`. The toolbar's reserved right gap (`CONTROLS_W × ui_scale`, [render/paint.rs:336](../../../crates/meerkat/src/render/paint.rs#L336)) is not holding once session chips + the add group crowd the row.

### 5. Paint cost scales with shell-document size (documented here, fixed elsewhere)

From the 2026-07-02 diagnostics session: `chrome_us` (cascade+layout+paint of the shell document) runs 100-145ms/frame even on the RepaintOnly path, nearly independent of mutation count (203 vs 16 mutations, same cost). `RepaintOnly` skips layout but `emit_paint_list` re-walks and re-encodes the whole box tree into a fresh `netrender::Scene` every frame, so cost tracks total DOM size (roster + gloss + orrery cards + panes), not the changed delta. That lives in `serval-layout` (`repos/serval/components/serval-layout/`), out of scope for a meerkat-side fix. Needs its own serval-side plan: retained or fragment-keyed paint lists patched by mutation, or partitioning the shell document's paint emission. This plan only relieves pressure (P1 keeps thumbnails off the per-frame path; smaller chip DOM). Enable diagnostics with `RUST_LOG=meerkat=info,meerkat::profile=debug` (permanent probes in `pane_session.rs` + `render/paint.rs`).

## Phases

### P1: Session chips carry graph thumbnails

- Revive the retired thumbnail rasterization (git history: `build_switcher_thumbnail_with`, removed 2026-06-26) as small per-session textures, composited into the chip via the external-texture element path the gloss minimap already uses. Refresh on session/graph change only, never per frame.
- Chip layout: one toolbar row tall, never wraps. Thumbnail + a single-line ellipsized label, with a thumbnail-only variant; which one is a setting (configurability rule), tooltip carries the full label either way.
- Identity stays node-sheet-derived (representations carry node identity) but as a thin accent ring or underline on the chip, active session highlighted; drop the full-bleed filled pill.
- Fix the stale-layout overlap: chip-list changes force the same re-measure path a resize does.
- Done when: chips are one row tall at every zoom, thumbnails update on graph change, no overlap with the omnibar, and the "big orange button" is gone.

### P2: The pelt tile surface scales with the chrome

- Thread `ui_scale` into the tile shell. Preferred seam: `TileShell::set_scale(f32)` in pelt-desktop that applies `scale_px` to its own structural sheet, so standalone pelt stays correct and meerkat passes one number (it already calls `set_theme` on theme change, [render/workbench.rs:114](../../../crates/meerkat/src/render/workbench.rs#L114)). Alternative: meerkat's `tile_sheet` emits scaled overrides of the structural rules; rejected unless the pelt seam proves awkward, since it duplicates pelt's structural CSS in mere.
- Scale the drag ghost, divider thickness, and pelt's pointer thresholds (px constants) with the same factor.
- Done when: tab bar and tab text visually match the toolbar's scale at 1x and 2x DPI, and Ctrl+zoom moves both together (headed shots at 1.0 / 1.4 / 2.2).

### P3: Tab text clipping fixed at the root

- Reproduce minimally: fixed-height flex row + centered inline text through serval-layout, confirm whether the line box overflows the top (engine defect) or the sheet needs explicit `line-height`. Fix the engine if it is the engine (standards-correct over host hacks).
- Give the tab bar breathing room under the toolbar: a themed divider or small top inset on the workbench surface, so tabs never abut the toolbar's underside.
- Done when: full glyphs render at 1x and 2x and under chrome zoom, verified with full-res crops.

### P4: Toolbar overflow correctness

- The toolbar row must never paint over the window-controls strip: reserve the controls' actual scaled width, let the omnibar flex down first, collapse the add group (exists) and chips to `+N` overflow earlier under pressure.
- Done when: no overlapping glyphs at zoom 1.0 to 2.0 across 1280 to 2560 window widths (driven sweep).

### P5: Canvas text scaling policy

- Design space, exposed as a setting rather than one hardcoded pick:
  1. labels fixed-size, faces fixed (current: map-style, canvas zoom only repositions),
  2. labels + faces scale with `cam.zoom`, clamped (document-style),
  3. faces scale, labels fixed (hybrid).
- Whichever default lands, fold `ui_scale` into face + label px so DPI reaches the canvas (14px physical on a 2x panel is the current state), and add an LOD floor: below a zoom threshold hide labels instead of rendering unreadably small text.
- Done when: Ctrl+wheel visibly magnifies nodes and labels under the scaling setting, labels legible at 2x DPI, and the setting persists.

## Open questions

- **OQ-1**: should Ctrl+zoom also zoom content tiles (browser convention), or stay chrome-only with a separate per-tile zoom? Content follows DPI today but not user_zoom.
- **OQ-2**: thumbnail size/cadence for P1 chips (the old switcher rasterized on every session change; fine at small counts, revisit past ~10 sessions).
- **OQ-3**: whether P3's engine fix belongs in serval-layout's flex line-box handling or in explicit tile CSS; decided by the minimal repro.

## Progress

- **2026-07-01**: plan written from a driven verification session (launch, load `https://example.com` into a tile, Ctrl+= x3, Ctrl+0, Ctrl+wheel x5; ddagrab captures ui-01/02/03 + full-res tab crops) plus code trace across meerkat, orrery, and pelt-desktop. All four defects reproduced; findings 1-4 grounded to file/line above. Finding 5 absorbed from the 2026-07-02 diagnostics session's memory so it is documented in design_docs.
