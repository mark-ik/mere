/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The roster pane: the graph's manifest as a node list with per-node facets
//! and edge detail for the focused node.
//!
//! R1: title / URL / content-type chip / tag chips per row.
//! R2: edge rows (kind + direction + other node title) for the focused row.
//! R3: sort/filter by content type; content-type shapes in the orrery.

use forme::GraphMemberId;
use kernel::graph::FieldId;
use register_theme::chrome::{ChromeTheme, Color32};

/// Directionality of a relation from the perspective of the focused node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeDir {
    Out,
    In,
}

/// One relation row in the focused node's edge detail.
pub struct EdgeRow {
    pub direction: EdgeDir,
    /// Short display label for the relation kind (e.g. "Hyperlink", "Traversal").
    pub kind_label: String,
    /// Other-end node's display title (node.title → cached_host → URL fallback).
    pub other_title: String,
    pub other_url: String,
    pub other_member: GraphMemberId,
}

/// One roster row: node data for display in the roster pane.
pub struct RosterRow {
    pub member: GraphMemberId,
    /// Display title: `node.title` if non-empty, else cached hostname, else URL.
    pub title: String,
    /// Primary URL, shown beneath the title.
    pub url: String,
    /// Resolved content type (from fetch state) or MIME hint from node metadata.
    pub content_type: Option<String>,
    /// Semantic tags, sorted. Empty = not rendered.
    pub tags: Vec<String>,
    /// Relations to/from this node. Populated only for the focused row.
    pub edges: Vec<EdgeRow>,
    pub selected: bool,
    /// When `Some`, this row is the first in a new content-type section; render
    /// the header label before the row itself.
    pub section_header: Option<String>,
}

/// One field-region row — the roster's third member kind, beside node rows and
/// edge rows. The row click centers the field on the canvas; the hide toggle
/// controls its visibility (the field + its coupling persist regardless).
pub struct FieldRow {
    pub id: FieldId,
    /// Display name (the field's authoring name, else a short id).
    pub name: String,
    /// Whether the field is currently hidden from the canvas.
    pub hidden: bool,
}

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
            ".roster {{ overflow: scroll; height: 100%; background-color: {}; padding: 6px; }}",
            rgb(c.panel_bg)
        ),
        format!(
            ".roster-row {{ background-color: {}; padding: 8px 10px; margin: 2px 0; }}",
            rgb(c.surface_bg)
        ),
        format!(
            ".roster-row-selected {{ background-color: {}; padding: 8px 10px; margin: 2px 0; }}",
            rgb(c.active_bg)
        ),
        format!(".roster-title {{ font-size: 15px; color: {}; }}", rgb(c.strong_text)),
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
        format!(
            ".roster-edges {{ margin-top: 6px; padding-top: 4px; }}",
        ),
        format!(
            ".roster-edge {{ display: flex; gap: 5px; padding: 2px 0; font-size: 12px; }}",
        ),
        format!(".roster-edge-dir {{ color: {}; min-width: 14px; }}", rgb(c.muted_text)),
        format!(
            ".roster-edge-kind {{ font-size: 10px; color: {}; background-color: {}; padding: 0px 5px; border-radius: 2px; flex-shrink: 0; }}",
            rgb(c.muted_text),
            rgb(c.control_bg)
        ),
        format!(".roster-edge-target {{ color: {}; }}", rgb(c.body_text)),
        format!(
            ".roster-section {{ font-size: 10px; color: {}; padding: 10px 10px 2px; }}",
            rgb(c.muted_text)
        ),
        format!(
            ".roster-empty {{ font-size: 14px; color: {}; padding: 12px; }}",
            rgb(c.muted_text)
        ),
        // Field rows: a name + a hide/show toggle, muted when hidden.
        format!(
            ".roster-field {{ display: flex; justify-content: space-between; align-items: center; background-color: {}; padding: 8px 10px; margin: 2px 0; }}",
            rgb(c.surface_bg)
        ),
        format!(".roster-field-hidden {{ opacity: 0.5; }}"),
        format!(".roster-field-name {{ font-size: 14px; color: {}; }}", rgb(c.body_text)),
        format!(
            ".roster-field-toggle {{ font-size: 11px; color: {}; background-color: {}; padding: 1px 7px; border-radius: 3px; }}",
            rgb(c.muted_text),
            rgb(c.control_bg)
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roster_sheet_marks_root_as_scroll_container() {
        let css = roster_sheet(&ChromeTheme::default()).join("\n");
        assert!(css.contains(".roster"));
        assert!(css.contains("overflow: scroll"));
        assert!(css.contains("height: 100%"));
    }
}
