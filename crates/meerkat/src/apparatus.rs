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

use super::observability::{ObservabilitySnapshot, age, severity_label};

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
        format!(
            ".apparatus {{ background-color: {}; padding: 8px; }}",
            rgb(c.panel_bg)
        ),
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
        format!(
            ".app-row-muted {{ font-size: 13px; color: {}; background-color: {}; padding: 7px 10px; margin: 2px 0; }}",
            rgb(c.muted_text),
            rgb(c.surface_bg)
        ),
    ]
}

/// Build the apparatus DOM: a Theme section (one `data-theme` button per option)
/// plus host observability sections.
pub fn build_apparatus_dom(
    themes: &[ThemeOption],
    system_rows: &[(String, String)],
    obs: &ObservabilitySnapshot,
) -> ScriptedDom {
    let mut dom = ScriptedDom::new();
    let root = dom.document();
    let container = dom.create_element(qual("div"));
    dom.set_attribute(container, qual("class"), "apparatus");
    dom.append_child(root, container);

    append_title(&mut dom, container, "Theme");
    for theme in themes {
        let btn = dom.create_element(qual("div"));
        let class = if theme.active {
            "app-btn-active"
        } else {
            "app-btn"
        };
        dom.set_attribute(btn, qual("class"), class);
        dom.set_attribute(btn, qual("data-theme"), &theme.id);
        let label = dom.create_text(&theme.name);
        dom.append_child(btn, label);
        dom.append_child(container, btn);
    }

    append_title(&mut dom, container, "Overview");
    for (label, value) in system_rows {
        append_row(&mut dom, container, "app-row", &format!("{label}: {value}"));
    }

    append_title(&mut dom, container, "UX Events");
    if obs.ux.is_empty() {
        append_row(&mut dom, container, "app-row-muted", "No UX events yet");
    } else {
        for event in &obs.ux {
            let detail = event.detail.as_deref().unwrap_or("");
            append_row(
                &mut dom,
                container,
                "app-row",
                &format!(
                    "{} {} {} {}",
                    event.surface,
                    event.event,
                    detail,
                    age(event.at)
                ),
            );
        }
    }

    append_title(&mut dom, container, "Actors");
    if obs.actors.is_empty() {
        append_row(&mut dom, container, "app-row-muted", "No actor events yet");
    } else {
        for actor in &obs.actors {
            let detail = actor.detail.as_deref().unwrap_or("");
            append_row(
                &mut dom,
                container,
                "app-row",
                &format!(
                    "{} {} {} {}",
                    actor.actor,
                    actor.event,
                    detail,
                    age(actor.at)
                ),
            );
        }
    }

    append_title(&mut dom, container, "Accessibility");
    append_row(
        &mut dom,
        container,
        "app-row",
        &format!(
            "Surfaces: {}; nodes: {}; degraded: {}",
            obs.a11y.surfaces, obs.a11y.nodes, obs.a11y.degraded
        ),
    );
    append_row(
        &mut dom,
        container,
        "app-row",
        &format!("Root: {}; focus: {}", obs.a11y.root, obs.a11y.focus),
    );
    append_row(
        &mut dom,
        container,
        "app-row",
        &format!(
            "Missing labels: {}; missing bounds: {}; duplicate ids: {}",
            obs.a11y.missing_labels, obs.a11y.missing_bounds, obs.a11y.duplicate_ids
        ),
    );
    if obs.a11y.audit.is_empty() {
        append_row(
            &mut dom,
            container,
            "app-row-muted",
            "No a11y audit failures",
        );
    } else {
        for finding in &obs.a11y.audit {
            append_row(&mut dom, container, "app-row", finding);
        }
    }

    append_title(&mut dom, container, "Diagnostics");
    if obs.diagnostics.is_empty() {
        append_row(&mut dom, container, "app-row-muted", "No diagnostics yet");
    } else {
        for diagnostic in &obs.diagnostics {
            append_row(
                &mut dom,
                container,
                "app-row",
                &format!(
                    "{} {}: {} {}",
                    severity_label(diagnostic.severity),
                    diagnostic.channel,
                    diagnostic.message,
                    age(diagnostic.at)
                ),
            );
        }
    }

    append_title(&mut dom, container, "Tracing");
    if obs.traces.is_empty() {
        append_row(
            &mut dom,
            container,
            "app-row-muted",
            "No portable trace events yet",
        );
    } else {
        for trace in &obs.traces {
            let detail = trace.detail.as_deref().unwrap_or("");
            append_row(
                &mut dom,
                container,
                "app-row",
                &format!(
                    "{} {} {} {}",
                    trace.name,
                    trace.event,
                    detail,
                    age(trace.at)
                ),
            );
        }
    }

    append_title(&mut dom, container, "Registry");
    append_row(
        &mut dom,
        container,
        "app-row",
        &format!("Registered channels: {}", obs.registry.registered_channels),
    );
    if obs.registry.orphan_channels.is_empty() {
        append_row(&mut dom, container, "app-row-muted", "No orphan channels");
    } else {
        for (channel, count) in &obs.registry.orphan_channels {
            append_row(
                &mut dom,
                container,
                "app-row",
                &format!("orphan {channel}: {count}"),
            );
        }
    }
    for violation in &obs.registry.invariant_violations {
        append_row(
            &mut dom,
            container,
            "app-row",
            &format!("invariant: {violation}"),
        );
    }

    append_title(&mut dom, container, "Probes");
    if obs.probes.is_empty() {
        append_row(&mut dom, container, "app-row-muted", "No probe failures");
    } else {
        for probe in &obs.probes {
            append_row(
                &mut dom,
                container,
                "app-row",
                &format!(
                    "{} {}: {} {}",
                    probe.name,
                    probe.status,
                    probe.detail,
                    age(probe.at)
                ),
            );
        }
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

fn append_row(dom: &mut ScriptedDom, parent: serval_scripted_dom::NodeId, class: &str, text: &str) {
    let row = dom.create_element(qual("div"));
    dom.set_attribute(row, qual("class"), class);
    let text = dom.create_text(text);
    dom.append_child(row, text);
    dom.append_child(parent, row);
}

/// A `QualName` in the null namespace (the shape `ScriptedDom` builders take).
fn qual(local: &str) -> QualName {
    QualName::new(None, Namespace::from(""), LocalName::from(local))
}
