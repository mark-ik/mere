/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Placeholder DOMs for D8 utility panes while their domain models are split out.

use frame::PaneContent;
use layout_dom_api::{LayoutDom, LayoutDomMut, LocalName, Namespace, QualName};
use register_theme::chrome::{ChromeTheme, Color32};
use serval_scripted_dom::ScriptedDom;

/// The utility pane CSS, themed from chrome tokens.
pub fn utility_pane_sheet(c: &ChromeTheme) -> Vec<String> {
    let rgb = |color: Color32| {
        let [r, g, b, _] = color.to_array();
        format!("rgb({r}, {g}, {b})")
    };
    vec![
        "div { display: block; }".to_string(),
        format!(
            ".utility-pane {{ background-color: {}; padding: 10px; }}",
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

pub fn build_utility_pane_dom(content: &PaneContent, rows: &[(String, String)]) -> ScriptedDom {
    let mut dom = ScriptedDom::new();
    let root = dom.document();
    let container = dom.create_element(qual("div"));
    dom.set_attribute(container, qual("class"), "utility-pane");
    dom.append_child(root, container);

    append_row(&mut dom, container, "utility-title", pane_title(content));
    for (label, value) in rows {
        append_row(
            &mut dom,
            container,
            "utility-row",
            &format!("{label}: {value}"),
        );
    }
    append_row(
        &mut dom,
        container,
        "utility-row-muted",
        pane_status(content),
    );
    dom
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
            "D8 seed: selected-object provenance, trust, parse diagnostics, document structure, cache state, and lineage land here."
        }
        PaneContent::Steward => {
            "D8 seed: live async operations, retries, background work, and user controls land here."
        }
        _ => "No utility pane data.",
    }
}

fn append_row(dom: &mut ScriptedDom, parent: serval_scripted_dom::NodeId, class: &str, text: &str) {
    let row = dom.create_element(qual("div"));
    dom.set_attribute(row, qual("class"), class);
    let label = dom.create_text(text);
    dom.append_child(row, label);
    dom.append_child(parent, row);
}

fn qual(name: &str) -> QualName {
    QualName::new(None, Namespace::from(""), LocalName::from(name))
}
