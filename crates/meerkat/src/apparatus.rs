/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The apparatus pane (A1): the system pane — settings + host diagnostics — as a
//! frame leaf. v0 carries a **Theme** section (the registered themes as buttons,
//! the active one highlighted) and a **System** section (read-only diagnostics).
//! Rendered as a serval DOM themed from the chrome tokens, like the `roster`
//! pane; the host hit-tests theme buttons (`data-theme`) to switch the theme.
//! Settings beyond the theme (the tab cap) fold in here later (plan A3).

use layout_dom_api::{LayoutDom, LayoutDomMut, LocalName, Namespace, QualName};
use register_theme::chrome::{ChromeTheme, Color32};
use serval_scripted_dom::ScriptedDom;

/// One theme option in the Theme section: its id (the hit-test key), display
/// name, and whether it is the active theme.
pub struct ThemeOption {
    pub id: String,
    pub name: String,
    pub active: bool,
}

/// The apparatus pane's author CSS, themed from the chrome tokens.
pub fn apparatus_sheet(c: &ChromeTheme) -> Vec<String> {
    let rgb = |color: Color32| {
        let [r, g, b, _] = color.to_array();
        format!("rgb({r}, {g}, {b})")
    };
    vec![
        "div { display: block; }".to_string(),
        format!(".apparatus {{ background-color: {}; padding: 8px; }}", rgb(c.panel_bg)),
        format!(
            ".app-title {{ font-size: 13px; color: {}; padding: 10px 4px 4px 4px; }}",
            rgb(c.muted_text)
        ),
        format!(
            ".app-btn {{ font-size: 15px; color: {}; background-color: {}; padding: 8px 10px; margin: 2px 0; }}",
            rgb(c.body_text),
            rgb(c.surface_bg)
        ),
        format!(
            ".app-btn-active {{ font-size: 15px; color: {}; background-color: {}; padding: 8px 10px; margin: 2px 0; }}",
            rgb(c.strong_text),
            rgb(c.active_bg)
        ),
        format!(
            ".app-row {{ font-size: 14px; color: {}; background-color: {}; padding: 7px 10px; margin: 2px 0; }}",
            rgb(c.body_text),
            rgb(c.surface_bg)
        ),
    ]
}

/// Build the apparatus DOM: a Theme section (one `data-theme` button per option)
/// and a System section (read-only `label: value` rows).
pub fn build_apparatus_dom(themes: &[ThemeOption], diagnostics: &[(String, String)]) -> ScriptedDom {
    let mut dom = ScriptedDom::new();
    let root = dom.document();
    let container = dom.create_element(qual("div"));
    dom.set_attribute(container, qual("class"), "apparatus");
    dom.append_child(root, container);

    append_title(&mut dom, container, "Theme");
    for theme in themes {
        let btn = dom.create_element(qual("div"));
        let class = if theme.active { "app-btn-active" } else { "app-btn" };
        dom.set_attribute(btn, qual("class"), class);
        dom.set_attribute(btn, qual("data-theme"), &theme.id);
        let label = dom.create_text(&theme.name);
        dom.append_child(btn, label);
        dom.append_child(container, btn);
    }

    append_title(&mut dom, container, "System");
    for (label, value) in diagnostics {
        let row = dom.create_element(qual("div"));
        dom.set_attribute(row, qual("class"), "app-row");
        let text = dom.create_text(&format!("{label}: {value}"));
        dom.append_child(row, text);
        dom.append_child(container, row);
    }
    dom
}

fn append_title(dom: &mut ScriptedDom, parent: serval_scripted_dom::NodeId, text: &str) {
    let title = dom.create_element(qual("div"));
    dom.set_attribute(title, qual("class"), "app-title");
    let label = dom.create_text(text);
    dom.append_child(title, label);
    dom.append_child(parent, title);
}

/// A `QualName` in the null namespace (the shape `ScriptedDom` builders take).
fn qual(local: &str) -> QualName {
    QualName::new(None, Namespace::from(""), LocalName::from(local))
}
