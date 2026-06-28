/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The roster pane: the graph's manifest as tabbed graph-element tables with
//! a detail-card region for the selected object.
//!
//! R1: title / URL / content-type chip / tag chips per row.
//! R2: edge rows (kind + direction + other node title) for the focused row.
//! R3: sort/filter by content type; content-type shapes in the orrery.

use forme::{GraphMemberId, GraphletId};
use kernel::graph::{EdgeFamily, FieldId, RelationSelector, SemanticSubKind};
use register_theme::chrome::{ChromeTheme, Color32};

/// The roster's top-level element tables.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RosterTab {
    Nodes,
    Links,
    Graphlets,
    Fields,
}

impl RosterTab {
    pub const ALL: [RosterTab; 4] = [
        RosterTab::Nodes,
        RosterTab::Links,
        RosterTab::Graphlets,
        RosterTab::Fields,
    ];

    pub fn label(self) -> &'static str {
        match self {
            RosterTab::Nodes => "Nodes",
            RosterTab::Links => "Links",
            RosterTab::Graphlets => "Graphlets",
            RosterTab::Fields => "Fields",
        }
    }

    pub fn empty_label(self) -> &'static str {
        match self {
            RosterTab::Nodes => "No nodes yet",
            RosterTab::Links => "No relations yet",
            RosterTab::Graphlets => "No graphlets yet",
            RosterTab::Fields => "No fields yet",
        }
    }
}

impl Default for RosterTab {
    fn default() -> Self {
        Self::Nodes
    }
}

/// The selected roster object. The host keeps this stable while fresh row/card
/// data is rebuilt from graph truth each frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RosterSubject {
    Node(GraphMemberId),
    LinkBundle {
        from: GraphMemberId,
        to: GraphMemberId,
    },
    RelationCell {
        from: GraphMemberId,
        to: GraphMemberId,
        selector: RelationSelector,
    },
    Graphlet(GraphletId),
    Field(FieldId),
}

/// The curated semantic relation picker shared by context menus and the Link Card.
pub const RELATE_PICKER_KINDS: &[(SemanticSubKind, &str)] = &[
    (SemanticSubKind::Cites, "Cites"),
    (SemanticSubKind::Quotes, "Quotes"),
    (SemanticSubKind::Summarizes, "Summarizes"),
    (SemanticSubKind::Elaborates, "Elaborates"),
    (SemanticSubKind::ExampleOf, "Example of"),
    (SemanticSubKind::Supports, "Supports"),
    (SemanticSubKind::Contradicts, "Contradicts"),
    (SemanticSubKind::Questions, "Questions"),
    (SemanticSubKind::SameEntityAs, "Same entity as"),
    (SemanticSubKind::DuplicateOf, "Duplicate of"),
    (SemanticSubKind::Hyperlink, "Hyperlink"),
];

/// One roster row: node data for display in the roster pane.
#[derive(Clone)]
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
    pub selected: bool,
    /// Whether the node is currently open in the workbench/tree projection.
    pub open: bool,
    /// When `Some`, this row is the first in a new content-type section; render
    /// the header label before the row itself.
    pub section_header: Option<String>,
}

/// One row in the Links table. A directed edge carrying multiple relation cells
/// contributes multiple rows; `starts_bundle` marks the first row for an
/// endpoint pair so the view can visually group them.
#[derive(Clone)]
pub struct LinkRow {
    pub from: GraphMemberId,
    pub to: GraphMemberId,
    pub source_title: String,
    pub source_url: String,
    pub target_title: String,
    pub target_url: String,
    pub direction_label: String,
    pub family: EdgeFamily,
    pub family_label: String,
    pub kind_label: String,
    pub source_label: Option<String>,
    pub selector: RelationSelector,
    pub selected: bool,
    pub starts_bundle: bool,
}

#[derive(Clone)]
pub struct GraphletRow {
    pub id: GraphletId,
    pub kind_label: String,
    pub binding_label: String,
    pub member_count: usize,
    pub selectors_label: String,
    pub drift_label: String,
    pub selected: bool,
}

/// One field-region row — the roster's third member kind, beside node rows and
/// edge rows. The row click centers the field on the canvas; the hide toggle
/// controls its visibility (the field + its coupling persist regardless).
#[derive(Clone)]
pub struct FieldRow {
    pub id: FieldId,
    /// Display name (the field's authoring name, else a short id).
    pub name: String,
    pub rule_label: String,
    pub extent_label: String,
    /// Whether the field is currently hidden from the canvas.
    pub hidden: bool,
    /// The field's coupling strength (the per-field force-well), shown + tuned in
    /// the row. (Field regions — strength tuning.)
    pub strength: f32,
}

#[derive(Clone)]
pub struct NodeDetail {
    pub member: GraphMemberId,
    pub title: String,
    pub url: String,
    pub content_type: Option<String>,
    pub tags: Vec<String>,
    pub relation_count: usize,
    pub open: bool,
}

#[derive(Clone)]
pub struct LinkRelationRow {
    pub from: GraphMemberId,
    pub to: GraphMemberId,
    pub family: EdgeFamily,
    pub family_label: String,
    pub kind_label: String,
    pub label: Option<String>,
    pub selector: RelationSelector,
    pub editable: bool,
    pub selected: bool,
}

#[derive(Clone)]
pub struct LinkCard {
    pub from: GraphMemberId,
    pub to: GraphMemberId,
    pub source_title: String,
    pub source_url: String,
    pub target_title: String,
    pub target_url: String,
    pub relations: Vec<LinkRelationRow>,
}

#[derive(Clone)]
pub struct GraphletCard {
    pub id: GraphletId,
    pub kind_label: String,
    pub binding_label: String,
    pub members: Vec<String>,
    pub selectors_label: String,
    pub drift_tracking: bool,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

#[derive(Clone)]
pub struct FieldDetail {
    pub id: FieldId,
    pub name: String,
    pub rule_label: String,
    pub extent_label: String,
    pub hidden: bool,
    pub strength: f32,
}

#[derive(Clone)]
pub enum RosterDetail {
    Node(NodeDetail),
    Link(LinkCard),
    Graphlet(GraphletCard),
    Field(FieldDetail),
}

/// The host-built roster projection for one frame. View state owns the active
/// tab and selected subject; this snapshot owns current table/card content.
#[derive(Clone, Default)]
pub struct RosterSnapshot {
    pub node_rows: Vec<RosterRow>,
    pub link_rows: Vec<LinkRow>,
    pub graphlet_rows: Vec<GraphletRow>,
    pub field_rows: Vec<FieldRow>,
    pub detail: Option<RosterDetail>,
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
        ".roster-tabs { display: flex; gap: 4px; margin-bottom: 6px; }".to_string(),
        format!(
            ".roster-tab {{ font-size: 12px; color: {}; background-color: {}; padding: 3px 8px; border-radius: 3px; }}",
            rgb(c.muted_text),
            rgb(c.control_bg)
        ),
        format!(
            ".roster-tab-active {{ font-size: 12px; color: {}; background-color: {}; padding: 3px 8px; border-radius: 3px; }}",
            rgb(c.strong_text),
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
        ".roster-card-group { margin-top: 6px; }".to_string(),
        format!(
            ".roster-card-group-title {{ font-size: 10px; color: {}; padding-top: 4px; }}",
            rgb(c.muted_text)
        ),
        // Field rows: a name + a hide/show toggle, muted when hidden.
        format!(
            ".roster-field {{ display: flex; justify-content: space-between; align-items: center; background-color: {}; padding: 8px 10px; margin: 2px 0; }}",
            rgb(c.surface_bg)
        ),
        format!(".roster-field-hidden {{ opacity: 0.5; }}"),
        format!(
            ".roster-field-name {{ font-size: 14px; color: {}; flex: 1 1 auto; }}",
            rgb(c.body_text)
        ),
        format!(
            ".roster-field-toggle {{ font-size: 11px; color: {}; background-color: {}; padding: 1px 7px; border-radius: 3px; margin-left: 6px; }}",
            rgb(c.muted_text),
            rgb(c.control_bg)
        ),
        // The − / + strength steppers and the value between them. (Field regions.)
        format!(
            ".roster-field-step {{ font-size: 13px; color: {}; background-color: {}; padding: 1px 7px; border-radius: 3px; margin-left: 4px; }}",
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
    fn roster_sheet_marks_root_as_scroll_container() {
        let css = roster_sheet(&ChromeTheme::default()).join("\n");
        assert!(css.contains(".roster"));
        assert!(css.contains("overflow: scroll"));
        assert!(css.contains("height: 100%"));
    }
}
