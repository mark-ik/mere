/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The apparatus pane (A1): the system pane — settings + host diagnostics — as a
//! frame leaf. v0 carries a **Theme** section (the registered themes as buttons,
//! the active one highlighted) and a **System** section (read-only diagnostics).
//! Rendered as a view-driven [`ListPane`](crate::list_pane::ListPane) themed from
//! the chrome tokens: each theme button is a clickable [`PaneItem`] whose activation
//! key is its theme id, so a click dispatches through the runner DOM and the host
//! drains the id to switch the theme — no `data-theme` rect cache.
//! Settings beyond the theme (the tab cap) fold in here later (plan A3).

use register_theme::chrome::{ChromeTheme, Color32};

use super::observability::{ObservabilitySnapshot, age, severity_label};
use crate::list_pane::PaneItem;

/// One theme option in the Theme section: its id (the hit-test key), display
/// name, and whether it is the active theme.
pub struct ThemeOption {
    pub id: String,
    pub name: String,
    pub active: bool,
}

/// One engine in the Engines section: its id (carried in the toggle key), display
/// name, and whether it is active (not deactivated) this session. (engine-picker
/// Phase 2.)
pub struct EngineRow {
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
            ".apparatus {{ overflow: scroll; height: 100%; background-color: {}; padding: 8px; }}",
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
        // Segmented sliders (theme editor): a label over a flex strip of cells.
        ".app-slider-row { display: block; margin: 4px 0 2px 0; }".to_string(),
        format!(
            ".app-slider-label {{ font-size: 12px; color: {}; padding: 2px 4px; }}",
            rgb(c.muted_text)
        ),
        ".app-slider-track { display: flex; height: 18px; gap: 1px; }".to_string(),
        ".app-seg { height: 18px; }".to_string(),
    ]
}

/// The Theme section's controls: one clickable button per option, its theme id the
/// activation key the host drains to switch the theme. Shared by the apparatus pane and the
/// settings lane's `pelt/appearance` page, so both stay one source of truth. (Settings lane.)
pub fn theme_section_items(themes: &[ThemeOption]) -> Vec<PaneItem> {
    themes
        .iter()
        .map(|theme| {
            let class = if theme.active { "app-btn-active" } else { "app-btn" };
            PaneItem::button(class, theme.name.clone(), theme.id.clone())
        })
        .collect()
}

/// The Engines section's controls: one toggle per present engine, the id riding the key as
/// `engine:toggle:<id>` (clicking flips its activation). Shared by the apparatus pane and the
/// settings lane's `pelt/engines` page. (Settings lane.)
pub fn engine_section_items(engines: &[EngineRow]) -> Vec<PaneItem> {
    engines
        .iter()
        .map(|engine| {
            let class = if engine.active { "app-btn-active" } else { "app-btn" };
            let label = format!("{}  —  {}", engine.name, if engine.active { "active" } else { "off" });
            PaneItem::button(class, label, format!("engine:toggle:{}", engine.id))
        })
        .collect()
}

/// The Physics section's controls: the node-damping readout plus the − / + step buttons
/// (`phys:damping:down` / `:up`). Shared by the apparatus pane and the settings lane's
/// `pelt/physics` page. (Settings lane.)
pub fn physics_section_items(physics_damping: f32) -> Vec<PaneItem> {
    vec![
        PaneItem::text("app-row", format!("Node damping (inertia): {physics_damping:.1}")),
        PaneItem::button(
            "app-btn",
            "− less damping (more drift)".to_string(),
            "phys:damping:down".to_string(),
        ),
        PaneItem::button(
            "app-btn",
            "+ more damping (settle sooner)".to_string(),
            "phys:damping:up".to_string(),
        ),
    ]
}

/// Build the apparatus pane's item list: the host observability sections as display rows.
/// The interactive settings sections (Theme / Engines / Physics) moved to the pelt settings
/// lane (`pelt/appearance` / `pelt/engines` / `pelt/physics`, via the shared section builders
/// above), so the apparatus is read-only diagnostics now. The [`ListPane`](crate::list_pane::ListPane)
/// renders these display rows. (Settings lane P2 — apparatus settings retired.)
pub fn apparatus_items(
    system_rows: &[(String, String)],
    obs: &ObservabilitySnapshot,
) -> Vec<PaneItem> {
    let mut items = Vec::new();

    items.push(PaneItem::text("app-title", "Overview"));
    for (label, value) in system_rows {
        items.push(PaneItem::text("app-row", format!("{label}: {value}")));
    }

    items.push(PaneItem::text("app-title", "UX Events"));
    if obs.ux.is_empty() {
        items.push(PaneItem::text("app-row-muted", "No UX events yet"));
    } else {
        for event in &obs.ux {
            let detail = event.detail.as_deref().unwrap_or("");
            items.push(PaneItem::text(
                "app-row",
                format!("{} {} {} {}", event.surface, event.event, detail, age(event.at)),
            ));
        }
    }

    items.push(PaneItem::text("app-title", "Actors"));
    if obs.actors.is_empty() {
        items.push(PaneItem::text("app-row-muted", "No actor events yet"));
    } else {
        for actor in &obs.actors {
            let detail = actor.detail.as_deref().unwrap_or("");
            items.push(PaneItem::text(
                "app-row",
                format!("{} {} {} {}", actor.actor, actor.event, detail, age(actor.at)),
            ));
        }
    }

    items.push(PaneItem::text("app-title", "Accessibility"));
    items.push(PaneItem::text(
        "app-row",
        format!(
            "Surfaces: {}; nodes: {}; degraded: {}",
            obs.a11y.surfaces, obs.a11y.nodes, obs.a11y.degraded
        ),
    ));
    items.push(PaneItem::text(
        "app-row",
        format!("Root: {}; focus: {}", obs.a11y.root, obs.a11y.focus),
    ));
    items.push(PaneItem::text(
        "app-row",
        format!(
            "Missing labels: {}; missing bounds: {}; duplicate ids: {}",
            obs.a11y.missing_labels, obs.a11y.missing_bounds, obs.a11y.duplicate_ids
        ),
    ));
    if obs.a11y.audit.is_empty() {
        items.push(PaneItem::text("app-row-muted", "No a11y audit failures"));
    } else {
        for finding in &obs.a11y.audit {
            items.push(PaneItem::text("app-row", finding.clone()));
        }
    }

    items.push(PaneItem::text("app-title", "Diagnostics"));
    if obs.diagnostics.is_empty() {
        items.push(PaneItem::text("app-row-muted", "No diagnostics yet"));
    } else {
        for diagnostic in &obs.diagnostics {
            items.push(PaneItem::text(
                "app-row",
                format!(
                    "{} {}: {} {}",
                    severity_label(diagnostic.severity),
                    diagnostic.channel,
                    diagnostic.message,
                    age(diagnostic.at)
                ),
            ));
        }
    }

    items.push(PaneItem::text("app-title", "Tracing"));
    if obs.traces.is_empty() {
        items.push(PaneItem::text("app-row-muted", "No portable trace events yet"));
    } else {
        for trace in &obs.traces {
            let detail = trace.detail.as_deref().unwrap_or("");
            items.push(PaneItem::text(
                "app-row",
                format!("{} {} {} {}", trace.name, trace.event, detail, age(trace.at)),
            ));
        }
    }

    items.push(PaneItem::text("app-title", "Registry"));
    items.push(PaneItem::text(
        "app-row",
        format!("Registered channels: {}", obs.registry.registered_channels),
    ));
    if obs.registry.orphan_channels.is_empty() {
        items.push(PaneItem::text("app-row-muted", "No orphan channels"));
    } else {
        for (channel, count) in &obs.registry.orphan_channels {
            items.push(PaneItem::text("app-row", format!("orphan {channel}: {count}")));
        }
    }
    for violation in &obs.registry.invariant_violations {
        items.push(PaneItem::text("app-row", format!("invariant: {violation}")));
    }

    items.push(PaneItem::text("app-title", "Probes"));
    if obs.probes.is_empty() {
        items.push(PaneItem::text("app-row-muted", "No probe failures"));
    } else {
        for probe in &obs.probes {
            items.push(PaneItem::text(
                "app-row",
                format!("{} {}: {} {}", probe.name, probe.status, probe.detail, age(probe.at)),
            ));
        }
    }

    items
}
