/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The roster pane (F1 first sibling pane): the graph's manifest as a node list.
//! A scrollable list of every graph node (title + url/content-type), rendered as
//! a serval DOM so it themes + lays out the same way the chrome does; the host
//! composites it into the roster leaf and hit-tests rows (by `data-member`) to
//! focus a node. This is R1's seed — edges + fields + drill-through come later.

use forme::GraphMemberId;
use layout_dom_api::{LayoutDom, LayoutDomMut, LocalName, Namespace, QualName};
use register_theme::chrome::{ChromeTheme, Color32};
use serval_scripted_dom::ScriptedDom;

/// One roster row: the node's member id (for hit-testing → focus), its title, a
/// subtitle (url + content type), and whether it is the focused node.
pub struct RosterRow {
    pub member: GraphMemberId,
    pub title: String,
    pub subtitle: String,
    pub selected: bool,
}

/// The roster's author CSS, themed from the chrome tokens so the pane reads as
/// part of the same shell. A row is a surface tile; the focused row takes the
/// selection fill.
pub fn roster_sheet(c: &ChromeTheme) -> Vec<String> {
    let rgb = |color: Color32| {
        let [r, g, b, _] = color.to_array();
        format!("rgb({r}, {g}, {b})")
    };
    vec![
        "div { display: block; }".to_string(),
        format!(".roster {{ background-color: {}; padding: 6px; }}", rgb(c.panel_bg)),
        format!(
            ".roster-row {{ background-color: {}; padding: 8px 10px; margin: 2px 0; }}",
            rgb(c.surface_bg)
        ),
        format!(
            ".roster-row-selected {{ background-color: {}; padding: 8px 10px; margin: 2px 0; }}",
            rgb(c.active_bg)
        ),
        format!(".roster-title {{ font-size: 16px; color: {}; }}", rgb(c.strong_text)),
        format!(".roster-sub {{ font-size: 13px; color: {}; }}", rgb(c.muted_text)),
        format!(".roster-empty {{ font-size: 14px; color: {}; padding: 12px; }}", rgb(c.muted_text)),
    ]
}

/// Build the roster DOM from `rows`: a `.roster` container of `.roster-row`
/// (`.roster-row-selected` for the focused node) entries, each carrying a
/// `data-member` attribute (the node UUID) for the host's row hit-test.
pub fn build_roster_dom(rows: &[RosterRow]) -> ScriptedDom {
    let mut dom = ScriptedDom::new();
    let root = dom.document();
    let container = dom.create_element(qual("div"));
    dom.set_attribute(container, qual("class"), "roster");
    dom.append_child(root, container);

    if rows.is_empty() {
        let empty = dom.create_element(qual("div"));
        dom.set_attribute(empty, qual("class"), "roster-empty");
        let text = dom.create_text("No nodes yet");
        dom.append_child(empty, text);
        dom.append_child(container, empty);
        return dom;
    }

    for row in rows {
        let entry = dom.create_element(qual("div"));
        let class = if row.selected { "roster-row-selected" } else { "roster-row" };
        dom.set_attribute(entry, qual("class"), class);
        dom.set_attribute(entry, qual("data-member"), &row.member.to_string());

        let title = dom.create_element(qual("div"));
        dom.set_attribute(title, qual("class"), "roster-title");
        let title_text = dom.create_text(&row.title);
        dom.append_child(title, title_text);
        dom.append_child(entry, title);

        let sub = dom.create_element(qual("div"));
        dom.set_attribute(sub, qual("class"), "roster-sub");
        let sub_text = dom.create_text(&row.subtitle);
        dom.append_child(sub, sub_text);
        dom.append_child(entry, sub);

        dom.append_child(container, entry);
    }
    dom
}

/// A `QualName` in the null namespace (the shape `ScriptedDom` builders take).
fn qual(local: &str) -> QualName {
    QualName::new(None, Namespace::from(""), LocalName::from(local))
}
