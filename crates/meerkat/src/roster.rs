/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The roster pane: the graph's manifest as tabbed graph-element tables with
//! a detail-card region for the selected object.
//!
//! R1: title / URL / content-type chip / tag chips per row.
//! R2: edge rows (kind + direction + other node title) for the focused row.
//! R3: sort/filter by content type; content-type shapes in the orrery.

pub use ::mere::roster::*;
use register_theme::chrome::{ChromeTheme, Color32};

/// The roster's author CSS, themed from the chrome tokens.
pub fn roster_sheet(c: &ChromeTheme) -> Vec<String> {
    let rgb = |color: Color32| {
        let [r, g, b, _] = color.to_array();
        format!("rgb({r}, {g}, {b})")
    };
    vec![
        "div { display: block; }".to_string(),
        "span { display: inline-block; }".to_string(),
        format!(
            ".roster {{ position: relative; overflow: hidden; height: 100%; box-sizing: border-box; background-color: {}; padding: 6px; }}",
            rgb(c.panel_bg)
        ),
        ".roster-tabs { display: block; margin-bottom: 6px; }".to_string(),
        ".roster-scroll { overflow: scroll; height: 100%; box-sizing: border-box; padding-bottom: 8px; }".to_string(),
        format!(
            ".roster-tab {{ display: block; font-size: 12px; color: {}; background-color: {}; padding: 3px 8px; margin: 0 0 3px 0; border-radius: 3px; border: 0; }}",
            rgb(c.muted_text),
            rgb(c.control_bg)
        ),
        format!(
            ".roster-tab-active {{ display: block; font-size: 12px; color: {}; background-color: {}; padding: 3px 8px; margin: 0 0 3px 0; border-radius: 3px; border: 0; }}",
            rgb(c.control_text),
            rgb(c.active_bg)
        ),
        ".roster-table { display: block; }".to_string(),
        format!(
            ".roster-row {{ background-color: {}; padding: 8px 10px; margin: 2px 0; }}",
            rgb(c.surface_bg)
        ),
        format!(
            ".roster-row-selected {{ background-color: {}; padding: 8px 10px; margin: 2px 0; }}",
            rgb(c.active_bg)
        ),
        format!(
            ".roster-title {{ font-size: 15px; color: {}; }}",
            rgb(c.strong_text)
        ),
        format!(
            ".roster-title-small {{ font-size: 13px; color: {}; }}",
            rgb(c.strong_text)
        ),
        format!(
            ".roster-sub {{ font-size: 12px; color: {}; margin-top: 2px; }}",
            rgb(c.muted_text)
        ),
        ".roster-facets { display: flex; flex-wrap: wrap; gap: 3px; margin-top: 5px; }".to_string(),
        format!(
            ".roster-chip {{ font-size: 11px; color: {}; background-color: {}; padding: 1px 6px; border-radius: 3px; }}",
            rgb(c.muted_text),
            rgb(c.control_bg)
        ),
        format!(
            ".roster-tag {{ font-size: 11px; color: {}; background-color: {}; padding: 1px 6px; border-radius: 3px; }}",
            rgb(c.control_text),
            rgb(c.control_bg)
        ),
        format!(".roster-edges {{ margin-top: 6px; padding-top: 4px; }}",),
        format!(".roster-edge {{ display: flex; gap: 5px; padding: 2px 0; font-size: 12px; }}",),
        format!(
            ".roster-edge-dir {{ color: {}; min-width: 14px; }}",
            rgb(c.muted_text)
        ),
        format!(
            ".roster-edge-kind {{ font-size: 10px; color: {}; background-color: {}; padding: 0px 5px; border-radius: 2px; flex-shrink: 0; }}",
            rgb(c.muted_text),
            rgb(c.control_bg)
        ),
        format!(".roster-edge-target {{ color: {}; }}", rgb(c.body_text)),
        ".roster-link-grid { display: flex; gap: 6px; align-items: center; }".to_string(),
        format!(
            ".roster-link-cell {{ font-size: 12px; color: {}; }}",
            rgb(c.body_text)
        ),
        format!(
            ".roster-link-kind {{ font-size: 10px; color: {}; background-color: {}; padding: 0px 5px; border-radius: 2px; flex-shrink: 0; }}",
            rgb(c.muted_text),
            rgb(c.control_bg)
        ),
        format!(
            ".roster-link-arrow {{ font-size: 12px; color: {}; }}",
            rgb(c.muted_text)
        ),
        format!(
            ".roster-section {{ font-size: 10px; color: {}; padding: 10px 10px 2px; }}",
            rgb(c.muted_text)
        ),
        format!(
            ".roster-empty {{ font-size: 14px; color: {}; padding: 12px; }}",
            rgb(c.muted_text)
        ),
        format!(
            ".roster-detail {{ background-color: {}; margin-top: 8px; padding: 9px 10px; }}",
            rgb(c.surface_bg)
        ),
        format!(
            ".roster-card-title {{ font-size: 14px; color: {}; }}",
            rgb(c.strong_text)
        ),
        format!(
            ".roster-card-sub {{ font-size: 12px; color: {}; margin-top: 2px; }}",
            rgb(c.muted_text)
        ),
        format!(
            ".roster-card-row {{ font-size: 12px; color: {}; padding-top: 4px; }}",
            rgb(c.body_text)
        ),
        ".roster-card-actions { display: flex; flex-wrap: wrap; gap: 4px; margin-top: 7px; }"
            .to_string(),
        format!(
            ".roster-card-action {{ font-size: 11px; color: {}; background-color: {}; padding: 2px 6px; border-radius: 3px; }}",
            rgb(c.control_text),
            rgb(c.control_bg)
        ),
        ".roster-facet-row { display: flex; justify-content: space-between; gap: 8px; padding: 5px 0; }"
            .to_string(),
        format!(
            ".roster-facet-label {{ font-size: 12px; color: {}; }}",
            rgb(c.body_text)
        ),
        format!(
            ".roster-facet-value {{ font-size: 12px; color: {}; text-align: right; }}",
            rgb(c.muted_text)
        ),
        ".roster-card-group { margin-top: 6px; }".to_string(),
        format!(
            ".roster-card-group-title {{ font-size: 10px; color: {}; padding-top: 4px; }}",
            rgb(c.muted_text)
        ),
        format!(
            ".roster-card-group-title-selected {{ font-size: 10px; color: {}; background-color: {}; padding: 4px 5px 0px; }}",
            rgb(c.strong_text),
            rgb(c.active_bg)
        ),
        ".roster-relate-picker { margin-top: 2px; padding-left: 4px; }".to_string(),
        // Field rows: a compact table over rule, extent, visibility, and strength.
        format!(
            ".roster-field-header {{ display: block; padding: 2px 10px 4px; font-size: 10px; color: {}; }}",
            rgb(c.muted_text)
        ),
        format!(
            ".roster-field {{ display: block; background-color: {}; padding: 8px 10px; margin: 2px 0; }}",
            rgb(c.surface_bg)
        ),
        format!(
            ".roster-field-selected {{ background-color: {}; }}",
            rgb(c.active_bg)
        ),
        format!(".roster-field-hidden {{ opacity: 0.5; }}"),
        format!(
            ".roster-field-name {{ display: inline-block; width: 28%; font-size: 14px; color: {}; vertical-align: middle; }}",
            rgb(c.body_text)
        ),
        format!(
            ".roster-field-meta {{ font-size: 11px; color: {}; background-color: {}; padding: 1px 6px; border-radius: 3px; }}",
            rgb(c.muted_text),
            rgb(c.control_bg)
        ),
        ".roster-field-rule { display: inline-block; width: 14%; vertical-align: middle; }".to_string(),
        ".roster-field-extent { display: inline-block; width: 20%; vertical-align: middle; }".to_string(),
        ".roster-field-visibility { display: inline-block; width: 19%; vertical-align: middle; }".to_string(),
        ".roster-field-strength-cell { display: inline-block; width: 17%; vertical-align: middle; }".to_string(),
        format!(
            ".roster-field-toggle {{ font-size: 11px; color: {}; background-color: {}; padding: 1px 7px; border-radius: 3px; }}",
            rgb(c.muted_text),
            rgb(c.control_bg)
        ),
        // The − / + strength steppers and the value between them. (Field regions.)
        format!(
            ".roster-field-step {{ font-size: 13px; color: {}; background-color: {}; padding: 1px 7px; border-radius: 3px; }}",
            rgb(c.body_text),
            rgb(c.control_bg)
        ),
        format!(
            ".roster-field-strength {{ font-size: 12px; color: {}; min-width: 18px; padding: 0 4px; }}",
            rgb(c.muted_text)
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roster_sheet_marks_body_as_scroll_container() {
        let css = roster_sheet(&ChromeTheme::default()).join("\n");
        assert!(css.contains(".roster"));
        assert!(css.contains(".roster-scroll"));
        assert!(css.contains("overflow: scroll"));
        assert!(css.contains("height: 100%"));
    }
}
