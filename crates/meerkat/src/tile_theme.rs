/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Pelt tile-surface theme CSS.

use super::*;

/// Build the pelt tile-surface theme sheet from the resolved [`ChromeTheme`], so
/// the workbench tiles read as the same shell as the chrome.
pub(crate) fn tile_sheet(c: &ChromeTheme) -> String {
    let rgb = |color: Color32| {
        let [r, g, b, _] = color.to_array();
        format!("rgb({r}, {g}, {b})")
    };
    let darken = |color: Color32, f: f32| {
        let [r, g, b, _] = color.to_array();
        let s = |v: u8| (v as f32 * f).round().clamp(0.0, 255.0) as u8;
        format!("rgb({}, {}, {})", s(r), s(g), s(b))
    };
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
