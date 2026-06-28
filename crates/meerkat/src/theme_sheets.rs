/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Chrome + tile author-CSS builders and the UI-scale rewriter.

use super::*;

/// Build the chrome root's author CSS from a resolved [`ChromeTheme`] (theming
/// pass). The toolbar is a flex row (back / forward buttons + a growing omnibar)
/// that serval lays out via taffy's flexbox; the `.chrome` container itself has
/// no background, so the host composites it over the content root and only the
/// toolbar + the (opaque) dropdowns paint over the page. The toolbar band sits a
/// step above the graph backdrop, fields/buttons a step above that, dropdowns +
/// floated panels their own tiers; all colors come from the active theme so the
/// chrome theme-switches alongside the graph. Surfaces, not classes, are the
/// unit: the toolbar, palette, settings, and comms pane reuse the same dozen
/// tokens, so a theme reads as one coherent shell.
pub(crate) fn chrome_sheet(c: &ChromeTheme) -> Vec<String> {
    let rgb = |color: Color32| {
        let [r, g, b, _] = color.to_array();
        format!("rgb({r}, {g}, {b})")
    };
    vec![
        // `textarea` included so the knot-editor source field is a block box of its own — it
        // then holds its own inline text layout (keyed by its node), which the caret primitives
        // (`caret_rect` / `caret_byte_at_point` / soft-wrap `caret_byte_vertical`) look up. Left
        // as the CSS-initial `display: inline` it had no box (`fragments().rect_of` → `None`) and
        // its text flowed into the parent div's context, so no caret op resolved for it.
        "div, button, input, textarea { display: block; }".to_string(),
        // Shell z-stack: the chrome is the top layer. Making `.chrome` a stacking context
        // (position + z-index) above the orrery means every chrome surface — toolbar,
        // omnibar dropdown, context menu, palette, find, settings, shellbar — paints over
        // the orrery's node/content cards and wins their hit-test, whether it is in normal
        // flow or positioned. Without it, the node cards (each its own stacking context via
        // `transform`) paint above normal-flow chrome content (the omnibar dropdown was the
        // symptom). The orrery sits at the base layer (z-index:0, below). (Shell z-stack.)
        ".chrome { position: relative; z-index: 10; }".to_string(),
        // The toolbar reserves right padding the width of the window-control strip
        // (the borderless titlebar's min / max / close), so the omnibar + sync chip
        // stop short of it and the host composites the controls into that gap.
        // `align-items: center` so a tall child (e.g. a long omnibar value) can't
        // stretch the back/forward/add buttons to strange proportions; `flex-wrap:
        // nowrap` keeps the band a single row so nothing pushes the shellbar / session
        // chip out of view. (Chrome bar — toolbar robustness.)
        format!(
            ".toolbar {{ display: flex; flex-wrap: nowrap; align-items: center; \
                background-color: {}; padding: 8px {}px 8px 8px; }}",
            rgb(c.toolbar_bg),
            titlebar::CONTROLS_W as u32
        ),
        format!(
            "button {{ font-size: 22px; color: {}; background-color: {}; padding: 8px 14px; margin: 4px; }}",
            rgb(c.control_text),
            rgb(c.control_bg)
        ),
        // The knot editor's header lays out as a row — title on the left, close × on the right —
        // instead of the default stacked blocks, and the × shrinks to a small button rather than
        // the full-width bar `button { display: block }` would otherwise make it. (Flex, like the
        // toolbar, rather than absolute `right:` which the rest of the chrome doesn't rely on.)
        ".knot-editor-title { display: flex; justify-content: space-between; align-items: center; }"
            .to_string(),
        ".knot-editor-btn { padding: 1px 9px; margin: 0; font-size: 18px; line-height: 1.3; \
         border-radius: 4px; }"
            .to_string(),
        format!(
            ".disabled {{ color: {}; background-color: {}; }}",
            rgb(c.disabled_text),
            rgb(c.disabled_bg)
        ),
        format!(
            // The omnibar: a single line that clips (with `min-width: 0` so it actually
            // shrinks in the flex row) — never wrapping to a second line, which would
            // inflate the toolbar. (Chrome bar — omnibar single-line.)
            "input {{ font-size: 22px; color: {}; background-color: {}; padding: 8px; margin: 4px; \
                flex-grow: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }}",
            rgb(c.field_text),
            rgb(c.field_bg)
        ),
        // The crawl-progress chip (relational-browse V2), a small muted pill;
        // hidden when empty (no crawl has run) via `:empty`.
        format!(
            ".crawl-chip {{ font-size: 14px; color: {}; background-color: {}; padding: 8px 12px; margin: 4px; border-radius: 14px; }} .crawl-chip-hidden {{ display: none; }}",
            rgb(c.muted_text),
            rgb(c.menu_bg)
        ),
        // The branch chip (graphlet wiring Phase 2): an accent pill (the add-pill's
        // marked look) naming the anchor a tear-out branch window forked from, so the
        // branch reads as a distinct grouping. Hidden on leaves / the primary.
        format!(
            ".branch-chip {{ font-size: 13px; color: {}; background-color: {}; padding: 6px 12px; margin: 4px; border-radius: 14px; }} .branch-chip-hidden {{ display: none; }}",
            rgb(c.strong_text),
            rgb(c.active_bg)
        ),
        // The add pill — the primary create affordance, an accent pill (matching the
        // sync-chip's pill shape) with a "+" that opens the Add node/tile/session menu.
        format!(
            ".add-pill {{ font-size: 18px; color: {}; background-color: {}; padding: 6px 16px; margin: 4px; border-radius: 14px; }}",
            rgb(c.strong_text),
            rgb(c.active_bg)
        ),
        // The segmented create group (Chrome bar P5): `+node | +tile | +field` as one
        // pill, each cell a button; the split-button (`add-split`) is its crowded form.
        ".add-group { display: flex; align-items: center; margin: 4px 2px; }".to_string(),
        ".add-split { display: flex; align-items: center; margin: 4px 2px; }".to_string(),
        format!(
            ".add-seg {{ font-size: 13px; white-space: nowrap; color: {}; background-color: {}; padding: 7px 9px; }}",
            rgb(c.control_text),
            rgb(c.control_bg)
        ),
        // Rounded outer corners so the three cells read as one pill.
        ".add-seg-first { border-radius: 13px 0 0 13px; }".to_string(),
        ".add-seg-last { border-radius: 0 13px 13px 0; }".to_string(),
        format!(
            ".add-split-primary {{ font-size: 16px; color: {}; background-color: {}; \
                padding: 5px 11px; border-radius: 13px 0 0 13px; }}",
            rgb(c.control_text),
            rgb(c.control_bg)
        ),
        format!(
            ".add-split-caret {{ font-size: 13px; color: {}; background-color: {}; \
                padding: 7px 9px; border-radius: 0 13px 13px 0; }}",
            rgb(c.muted_text),
            rgb(c.control_bg)
        ),
        format!(
            ".suggestions {{ background-color: {}; padding-bottom: 6px; }}",
            rgb(c.panel_bg)
        ),
        format!(
            ".suggestion {{ font-size: 18px; color: {}; background-color: {}; padding: 8px 16px; }}",
            rgb(c.body_text),
            rgb(c.panel_bg)
        ),
        format!(
            ".suggestion-active {{ font-size: 18px; color: {}; background-color: {}; padding: 8px 16px; }}",
            rgb(c.strong_text),
            rgb(c.active_bg)
        ),
        // Command palette: a centered panel floated over the page (flex centering;
        // serval maps justify-content through stylo_taffy).
        ".palette-overlay { display: flex; justify-content: center; padding-top: 56px; }"
            .to_string(),
        format!(
            ".palette {{ width: 540px; background-color: {}; padding: 10px; }}",
            rgb(c.surface_bg)
        ),
        // The command list scrolls (the host bounds its height per-window inline
        // and offsets it to follow the selection, so a long list stays reachable in
        // a small window).
        format!(".cmd-list {{ overflow: scroll; background-color: {}; }}", rgb(c.surface_bg)),
        format!(
            ".cmd-row {{ font-size: 18px; color: {}; background-color: {}; padding: 8px 12px; }}",
            rgb(c.body_text),
            rgb(c.surface_bg)
        ),
        format!(
            ".cmd-row-active {{ font-size: 18px; color: {}; background-color: {}; padding: 8px 12px; }}",
            rgb(c.strong_text),
            rgb(c.active_bg)
        ),
        // Find-in-page bar: docked top-right under the toolbar (flex end), a small
        // panel with a label + the query field.
        ".find-overlay { display: flex; justify-content: flex-end; padding-top: 56px; padding-right: 12px; }"
            .to_string(),
        format!(
            ".find-bar {{ display: flex; align-items: center; background-color: {}; padding: 6px 10px; }}",
            rgb(c.surface_bg)
        ),
        format!(
            ".find-label {{ font-size: 16px; color: {}; padding: 6px 8px; }}",
            rgb(c.body_text)
        ),
        format!(
            ".find-count {{ font-size: 14px; color: {}; padding: 6px 8px; }}",
            rgb(c.muted_text)
        ),
        // Right-click context menu: a small panel of action rows floated at the cursor.
        format!(
            ".context-menu {{ background-color: {}; padding: 4px; }}",
            rgb(c.menu_bg)
        ),
        // The nested submenu panel: the same surface as the root menu, positioned beside its
        // parent row each frame by `render`. Child rows reuse `.context-item`. (Nested submenus.)
        format!(
            ".context-submenu {{ background-color: {}; padding: 4px; }}",
            rgb(c.menu_bg)
        ),
        // The wrapper holding the root menu + its submenu panel. It MUST be positioned (absolute at
        // the chrome origin), not in-flow: serval resolves an absolute child's inset against its
        // tree parent, so an in-flow wrapper would push both panels down by the toolbar height.
        // Zero-offset, so it contributes nothing to the panels' painted origin. (Nested submenus.)
        ".context-menu-layer { position: absolute; left: 0; top: 0; }".to_string(),
        // The keyboard-highlighted submenu child row — its own accent class (not
        // `.context-item-active`) so the root menu's scroll-into-view never latches onto a child
        // rect, and the submenu's own scroll can target it. (Nested submenus.)
        format!(
            ".context-subitem-active {{ font-size: 16px; color: {}; background-color: {}; padding: 8px 18px; }}",
            rgb(c.strong_text),
            rgb(c.active_bg)
        ),
        // The tear-out drag ghost: a small pill carrying the dragged node's title, floated
        // at the cursor (positioned each frame by `render`). Pointer-events off so it never
        // intercepts the drag. (Tear-out gestures, GA-5.)
        format!(
            ".tear-ghost {{ position: absolute; font-size: 13px; color: {}; background-color: {}; \
             padding: 4px 10px; border-radius: 12px; }}",
            rgb(c.body_text),
            rgb(c.menu_bg)
        ),
        format!(
            ".context-item {{ font-size: 16px; color: {}; background-color: {}; padding: 8px 18px; }}",
            rgb(c.body_text),
            rgb(c.menu_bg)
        ),
        // The keyboard-highlighted context-menu row (arrow nav), matching the palette's
        // `cmd-row-active` accent.
        format!(
            ".context-item-active {{ font-size: 16px; color: {}; background-color: {}; padding: 8px 18px; }}",
            rgb(c.strong_text),
            rgb(c.active_bg)
        ),
        // The context menu's search field (the cursor palette): an input-styled row at the top.
        // `-empty` dims the placeholder. Typing edits it (the menu owns the keyboard).
        format!(
            ".context-search {{ font-size: 16px; color: {}; background-color: {}; padding: 8px 18px; }}",
            rgb(c.field_text),
            rgb(c.field_bg)
        ),
        format!(
            ".context-search-empty {{ font-size: 16px; color: {}; background-color: {}; padding: 8px 18px; }}",
            rgb(c.muted_text),
            rgb(c.field_bg)
        ),
        // The pin toggle on a search-result row: "+" to pin (muted), "✓" once pinned (accent).
        format!(
            ".context-pin {{ font-size: 16px; color: {}; background-color: {}; padding: 8px 12px; }}",
            rgb(c.muted_text),
            rgb(c.menu_bg)
        ),
        format!(
            ".context-pin-on {{ font-size: 16px; color: {}; background-color: {}; padding: 8px 12px; }}",
            rgb(c.strong_text),
            rgb(c.active_bg)
        ),
        // Comms pane: an absolutely-positioned panel whose geometry the host sets
        // inline each frame from the Comms frame leaf's rect (so it splits beside
        // the orrery like the other panes, rather than floating docked).
        format!(
            ".comms-pane {{ position: absolute; background-color: {}; padding: 10px; }}",
            rgb(c.panel_bg)
        ),
        format!(
            ".comms-title {{ display: flex; background-color: {}; padding: 4px 4px 10px 4px; }}",
            rgb(c.panel_bg)
        ),
        format!(
            ".comms-title-text {{ font-size: 20px; color: {}; background-color: {}; flex-grow: 1; padding: 4px 8px; }}",
            rgb(c.strong_text),
            rgb(c.panel_bg)
        ),
        format!(
            ".comms-btn {{ font-size: 18px; color: {}; background-color: {}; padding: 4px 12px; }}",
            rgb(c.control_text),
            rgb(c.control_bg)
        ),
        format!(
            ".comms-failure {{ font-size: 14px; color: {}; background-color: {}; padding: 6px 10px; margin-bottom: 6px; }}",
            rgb(c.error_text),
            rgb(c.error_bg)
        ),
        format!(
            ".comms-row {{ font-size: 17px; color: {}; background-color: {}; padding: 10px 12px; margin: 3px 0; }}",
            rgb(c.body_text),
            rgb(c.surface_bg)
        ),
        format!(
            ".comms-empty {{ font-size: 15px; color: {}; background-color: {}; padding: 10px 12px; }}",
            rgb(c.muted_text),
            rgb(c.panel_bg)
        ),
        format!(
            ".comms-back {{ font-size: 15px; color: {}; background-color: {}; padding: 6px 12px; margin-bottom: 6px; }}",
            rgb(c.body_text),
            rgb(c.control_bg)
        ),
        format!(
            ".comms-thread-title {{ font-size: 18px; color: {}; background-color: {}; padding: 8px 4px; }}",
            rgb(c.strong_text),
            rgb(c.panel_bg)
        ),
        format!(
            ".comms-msg-in {{ font-size: 16px; color: {}; background-color: {}; padding: 8px 12px; margin: 4px 24px 4px 0; }}",
            rgb(c.body_text),
            rgb(c.menu_bg)
        ),
        format!(
            ".comms-msg-out {{ font-size: 16px; color: {}; background-color: {}; padding: 8px 12px; margin: 4px 0 4px 24px; }}",
            rgb(c.strong_text),
            rgb(c.active_bg)
        ),
        format!(
            ".comms-compose {{ display: flex; background-color: {}; padding-top: 8px; }}",
            rgb(c.panel_bg)
        ),
        format!(
            ".comms-send {{ font-size: 16px; color: {}; background-color: {}; padding: 8px 16px; margin: 4px; }}",
            rgb(c.control_text),
            rgb(c.active_bg)
        ),
        format!(
            ".comms-status {{ font-size: 14px; color: {}; background-color: {}; padding: 6px 12px; }}",
            rgb(c.muted_text),
            rgb(c.panel_bg)
        ),
        format!(
            ".comms-new-btn {{ font-size: 16px; color: {}; background-color: {}; padding: 8px 12px; margin: 4px 0; }}",
            rgb(c.control_text),
            rgb(c.active_bg)
        ),
        format!(".comms-new {{ background-color: {}; }}", rgb(c.panel_bg)),
        format!(
            ".comms-proto-row {{ display: flex; background-color: {}; padding: 4px 0; }}",
            rgb(c.panel_bg)
        ),
        format!(
            ".comms-proto {{ font-size: 15px; color: {}; background-color: {}; padding: 6px 14px; margin: 0 6px 0 0; }}",
            rgb(c.body_text),
            rgb(c.control_bg)
        ),
        format!(
            ".comms-proto-active {{ font-size: 15px; color: {}; background-color: {}; padding: 6px 14px; margin: 0 6px 0 0; }}",
            rgb(c.strong_text),
            rgb(c.active_bg)
        ),
        format!(
            ".comms-new-to {{ background-color: {}; padding-top: 2px; }}",
            rgb(c.panel_bg)
        ),
        format!(
            ".comms-new-body {{ display: flex; background-color: {}; padding-top: 2px; }}",
            rgb(c.panel_bg)
        ),
        format!(
            ".comms-field-label {{ font-size: 13px; color: {}; background-color: {}; padding: 8px 4px 2px 4px; }}",
            rgb(c.muted_text),
            rgb(c.panel_bg)
        ),
        // Shellbar: an absolutely-positioned chrome strip whose geometry the host
        // sets inline each frame from the shellbar_rect() helper. Contains toggle
        // buttons for each pane (F2.1).
        format!(
            ".shellbar {{ position: absolute; background-color: {}; display: flex; \
                align-items: center; justify-content: flex-start; }}",
            rgb(c.toolbar_bg)
        ),
        // Centred glyphs: serval's flex does not centre a bare text child via
        // `justify-content`, so a fixed-width button leaves the glyph hugging its left
        // edge. Instead the button is content-width with *symmetric* horizontal
        // padding — which centres the glyph inside it whatever its width — and the
        // container's `align-items: center` then centres the whole button in the strip.
        // The padding doubles as the ≥32px hit target. (Chrome bar P3.)
        // Inactive buttons carry **no** background: content-width grounds would otherwise
        // read as a ragged-width column (the glyphs differ in width). Only the active
        // button gets a fill — a single accent pill marking the open pane, no raggedness.
        // (Chrome bar — even shellbar.)
        format!(
            ".shellbar-btn {{ display: flex; align-items: center; justify-content: center; \
                height: 44px; padding: 0 13px; font-size: 17px; margin: 2px 0; \
                color: {}; background-color: transparent; }}",
            rgb(c.control_text)
        ),
        format!(
            ".shellbar-btn-active {{ display: flex; align-items: center; justify-content: center; \
                height: 44px; padding: 0 13px; font-size: 17px; margin: 2px 0; border-radius: 10px; \
                color: {}; background-color: {}; }}",
            rgb(c.strong_text),
            rgb(c.active_bg)
        ),
        // The toolbar session strip (Chrome bar P4): chips for the open sessions, the
        // overflow `+N ⌄`, and the add `+`. The active chip takes the selection fill so
        // the focused session reads at a glance (representation-identity).
        ".session-strip { display: flex; align-items: center; }".to_string(),
        format!(
            ".session-chip {{ display: flex; align-items: center; background-color: {}; \
                color: {}; border-radius: 13px; margin: 4px 2px; padding: 2px 2px 2px 4px; }}",
            rgb(c.control_bg),
            rgb(c.control_text)
        ),
        format!(
            ".session-chip-active {{ background-color: {}; color: {}; }}",
            rgb(c.active_bg),
            rgb(c.strong_text)
        ),
        ".session-chip-label { font-size: 13px; padding: 6px 6px; }".to_string(),
        format!(
            ".session-chip-close {{ font-size: 13px; padding: 6px 8px; color: {}; }}",
            rgb(c.muted_text)
        ),
        format!(
            ".session-add {{ font-size: 16px; color: {}; background-color: {}; \
                padding: 5px 13px; margin: 4px 2px; border-radius: 13px; }}",
            rgb(c.control_text),
            rgb(c.control_bg)
        ),
        format!(
            ".session-overflow-btn {{ font-size: 13px; color: {}; background-color: {}; \
                padding: 7px 11px; margin: 4px 2px; border-radius: 13px; }}",
            rgb(c.control_text),
            rgb(c.control_bg)
        ),
        format!(
            ".session-overflow-btn-open {{ color: {}; background-color: {}; }}",
            rgb(c.strong_text),
            rgb(c.active_bg)
        ),
        // The overflow dropdown panel; the host sets its top/right inline each frame
        // (it anchors under the strip), like the context menu. Floats over content.
        format!(
            ".session-overflow {{ position: absolute; background-color: {}; padding: 4px; \
                border-radius: 8px; z-index: 30; }}",
            rgb(c.menu_bg)
        ),
        format!(
            ".session-overflow-row {{ font-size: 14px; color: {}; background-color: {}; \
                padding: 8px 16px; margin: 1px 0; }}",
            rgb(c.body_text),
            rgb(c.surface_bg)
        ),
    ]
}

/// Scale every `<number>px` length in a built CSS sheet by `scale`, so one
/// `ui_scale` (display DPI × the user's zoom) resizes the whole chrome without
/// threading a factor through each of the ~37 rules. A no-op at 1.0. Only a digit
/// run immediately followed by `px` (and a non-alphanumeric boundary) is touched, so
/// unitless numbers (`z-index`, `line-height`, `flex-grow`, `rgb(...)`, `opacity`)
/// and `px` inside identifiers are left alone. (UI scale.)
pub(crate) fn scale_px(sheet: Vec<String>, scale: f32) -> Vec<String> {
    if (scale - 1.0).abs() < 1e-3 {
        return sheet;
    }
    sheet.into_iter().map(|rule| scale_px_in(&rule, scale)).collect()
}

pub(crate) fn scale_px_in(rule: &str, scale: f32) -> String {
    let b = rule.as_bytes();
    let mut out = String::with_capacity(rule.len() + 8);
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let starts_num =
            c.is_ascii_digit() || (c == b'.' && i + 1 < b.len() && b[i + 1].is_ascii_digit());
        if !starts_num {
            out.push(c as char);
            i += 1;
            continue;
        }
        let start = i;
        while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
            i += 1;
        }
        let is_px = i + 1 < b.len()
            && b[i] == b'p'
            && b[i + 1] == b'x'
            && (i + 2 >= b.len() || !b[i + 2].is_ascii_alphanumeric());
        match (is_px, rule[start..i].parse::<f32>()) {
            (true, Ok(n)) => {
                let s = n * scale;
                if (s.round() - s).abs() < 0.05 {
                    out.push_str(&(s.round() as i64).to_string());
                } else {
                    out.push_str(&format!("{s:.1}"));
                }
                out.push_str("px");
                i += 2;
            }
            _ => out.push_str(&rule[start..i]),
        }
    }
    out
}

#[cfg(test)]
mod ui_scale_tests {
    use super::scale_px;

    #[test]
    fn scales_px_lengths_but_not_unitless_or_colors() {
        let sheet = vec![
            ".a { font-size: 16px; padding: 8px 18px; z-index: 10; line-height: 1.3; }".to_string(),
            ".b { width: 44px; color: rgb(12, 34, 56); flex-grow: 1; opacity: 0.45; }".to_string(),
        ];
        let scaled = scale_px(sheet, 2.0);
        // px lengths double…
        assert!(scaled[0].contains("font-size: 32px"));
        assert!(scaled[0].contains("padding: 16px 36px"));
        assert!(scaled[1].contains("width: 88px"));
        // …while unitless numbers and rgb() channels are untouched.
        assert!(scaled[0].contains("z-index: 10"));
        assert!(scaled[0].contains("line-height: 1.3"));
        assert!(scaled[1].contains("rgb(12, 34, 56)"));
        assert!(scaled[1].contains("flex-grow: 1"));
        assert!(scaled[1].contains("opacity: 0.45"));
    }

    #[test]
    fn fractional_scale_keeps_one_decimal_and_is_noop_at_one() {
        let sheet = vec![".x { font-size: 15px; }".to_string()];
        assert!(scale_px(sheet.clone(), 1.1)[0].contains("font-size: 16.5px"));
        // 1.0 is an identity (and the cheap early return).
        assert_eq!(scale_px(sheet.clone(), 1.0), sheet);
    }
}

/// Build the pelt tile-surface theme sheet from the resolved [`ChromeTheme`], so the
/// workbench tiles read as the same shell as the chrome. The surface layers this over
/// its structural default CSS (`TileSurface::set_theme`), so it only restates colors,
/// not layout. Roles mirror [`chrome_sheet`] and the Strophos shared-palette model
/// (woodshed's `audio-widgets::theme` — surface ladder + selection fill + text
/// weights): the tab bar is the toolbar band, an inactive tab a control surface, the
/// active tab the selection fill, the content area the band tone (matching [`CARD_BG`]
/// so the gap below a short card is seamless, replacing pelt's white page default),
/// and the gutter a darkened seam.
pub(crate) fn tile_sheet(c: &ChromeTheme) -> String {
    let rgb = |color: Color32| {
        let [r, g, b, _] = color.to_array();
        format!("rgb({r}, {g}, {b})")
    };
    // A darker step off a token, for the slot gutter (no "darkest" theme token; the
    // chrome's frame dividers use a near-black seam, so halve the band tone here).
    let darken = |color: Color32, f: f32| {
        let [r, g, b, _] = color.to_array();
        let s = |v: u8| (v as f32 * f).round().clamp(0.0, 255.0) as u8;
        format!("rgb({}, {}, {})", s(r), s(g), s(b))
    };
    // A glyph/text resting on a fill picks near-white or near-black by WCAG
    // contrast, so it stays legible whatever hue the fill becomes. The active
    // tab's close × sits on the (theme-accented) active-tab background, so it is
    // contrast-picked rather than a fixed muted tone. (Readable-on-accent.)
    let on = |bg: Color32| {
        let [r, g, b, _] = bg.to_array();
        let o = tincture::best_on(tincture::Srgb::rgb(r, g, b));
        format!("rgb({}, {}, {})", o.r, o.g, o.b)
    };
    format!(
        ".tile-tabbar {{ background: {tabbar}; }} \
         .tile-tab {{ color: {tab_text}; background: {tab_bg}; }} \
         .tile-tab.active {{ color: {active_text}; background: {active_bg}; }} \
         .tile-close {{ color: {close}; }} \
         .tile-tab.active .tile-close {{ color: {active_close}; }} \
         .tile-content {{ background: {content}; }} \
         .tile-divider {{ background: {divider}; }} \
         .tile-ghost {{ color: {active_text}; background: {active_bg}; border: 1px solid {ghost_border}; }}",
        tabbar = rgb(c.toolbar_bg),
        tab_text = rgb(c.muted_text),
        tab_bg = rgb(c.control_bg),
        active_text = rgb(c.strong_text),
        active_bg = rgb(c.active_bg),
        close = rgb(c.muted_text),
        active_close = on(c.active_bg),
        content = rgb(c.toolbar_bg),
        divider = darken(c.toolbar_bg, 0.5),
        ghost_border = rgb(c.muted_text),
    )
}

/// Fallback chrome-band height (px) if the toolbar can't be measured.
pub(crate) const FALLBACK_TOOLBAR_H: u32 = 64;

/// Background of the floating content card — a panel a step above the orrery
/// backdrop, so the card reads as a raised surface over the dark orrery band.
pub(crate) const CARD_BG: wgpu::Color = wgpu::Color {
    r: 0.110,
    g: 0.122,
    b: 0.145,
    a: 1.0,
};
