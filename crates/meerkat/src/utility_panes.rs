/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Item lists for the D8 utility panes (inspector / steward) while their domain
//! models are split out. They render through a view-driven
//! [`ListPane`](crate::list_pane::ListPane): a title, the data rows, and a muted
//! status line, all inert text (these panes are display-only).

use frame::PaneContent;
use register_theme::chrome::{ChromeTheme, Color32};

use crate::list_pane::PaneItem;

/// The utility pane CSS, themed from chrome tokens.
pub fn utility_pane_sheet(c: &ChromeTheme) -> Vec<String> {
    let rgb = |color: Color32| {
        let [r, g, b, _] = color.to_array();
        format!("rgb({r}, {g}, {b})")
    };
    vec![
        "div { display: block; }".to_string(),
        format!(
            ".utility-pane {{ overflow: scroll; height: 100%; background-color: {}; padding: 10px; }}",
            rgb(c.panel_bg)
        ),
        format!(
            ".utility-title {{ font-size: 15px; color: {}; padding: 8px 4px 6px 4px; }}",
            rgb(c.strong_text)
        ),
        format!(
            ".utility-row {{ font-size: 14px; color: {}; background-color: {}; padding: 8px 10px; margin: 3px 0; }}",
            rgb(c.body_text),
            rgb(c.surface_bg)
        ),
        format!(
            ".utility-row-muted {{ font-size: 13px; color: {}; background-color: {}; padding: 8px 10px; margin: 3px 0; }}",
            rgb(c.muted_text),
            rgb(c.surface_bg)
        ),
    ]
}

/// A utility pane's item list: a title row, the data rows (`label: value`), then a
/// muted status line. The [`ListPane`](crate::list_pane::ListPane) renders these
/// under the `utility-pane` root; every item is inert text (display-only).
pub fn utility_pane_items(content: &PaneContent, rows: &[(String, String)]) -> Vec<PaneItem> {
    let mut items = vec![PaneItem::text("utility-title", pane_title(content))];
    for (label, value) in rows {
        items.push(PaneItem::text("utility-row", format!("{label}: {value}")));
    }
    items.push(PaneItem::text("utility-row-muted", pane_status(content)));
    items
}

pub fn pane_title(content: &PaneContent) -> &'static str {
    match content {
        PaneContent::Inspector => "Inspector",
        PaneContent::Steward => "Steward",
        _ => "Pane",
    }
}

pub fn pane_status(content: &PaneContent) -> &'static str {
    match content {
        PaneContent::Inspector => {
            "Selected-object identity, provenance, trust, parse state, document structure, cache state, and lineage."
        }
        PaneContent::Steward => {
            "Live async operations with retry, stop, and background pin hooks routed through host commands."
        }
        _ => "No utility pane data.",
    }
}
