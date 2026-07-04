/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The apparatus pane: the read-only diagnostics/system strip, rendered as a
//! view-driven [`ListPane`](crate::list_pane::ListPane) themed from the chrome
//! tokens.

use register_theme::chrome::{ChromeTheme, Color32};

use super::observability::{ObservabilitySnapshot, age, severity_label};
use crate::list_pane::PaneItem;

/// One live apparatus table stat row, owned by Meerkat's apparatus surface rather than
/// by `platen`.
pub struct ApparatusTableStat {
    pub label: String,
    pub kind: &'static str,
    pub count: Option<u64>,
    pub count_unit: &'static str,
    pub estimated_bytes: Option<u64>,
    pub session_deltas: Option<u64>,
    pub last_dirty_set_size: Option<u64>,
    pub detail: Option<String>,
    pub empty_state: Option<String>,
}

impl ApparatusTableStat {
    pub fn present(
        label: impl Into<String>,
        kind: &'static str,
        count: u64,
        count_unit: &'static str,
    ) -> Self {
        Self {
            label: label.into(),
            kind,
            count: Some(count),
            count_unit,
            estimated_bytes: None,
            session_deltas: None,
            last_dirty_set_size: None,
            detail: None,
            empty_state: None,
        }
    }

    pub fn unavailable(
        label: impl Into<String>,
        kind: &'static str,
        empty_state: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            kind,
            count: None,
            count_unit: "rows",
            estimated_bytes: None,
            session_deltas: None,
            last_dirty_set_size: None,
            detail: None,
            empty_state: Some(empty_state.into()),
        }
    }

    pub fn with_estimated_bytes(mut self, estimated_bytes: u64) -> Self {
        self.estimated_bytes = Some(estimated_bytes);
        self
    }

    pub fn with_session_deltas(mut self, session_deltas: u64) -> Self {
        self.session_deltas = Some(session_deltas);
        self
    }

    pub fn with_last_dirty_set_size(mut self, last_dirty_set_size: u64) -> Self {
        self.last_dirty_set_size = Some(last_dirty_set_size);
        self
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} bytes")
    } else {
        format!("{bytes} bytes (~{:.1} KiB)", bytes as f64 / 1024.0)
    }
}

fn format_table_stat(stat: &ApparatusTableStat) -> String {
    let mut metrics = Vec::new();
    if let Some(count) = stat.count {
        metrics.push(format!("{count} {}", stat.count_unit));
    }
    if let Some(bytes) = stat.estimated_bytes {
        metrics.push(format_bytes(bytes));
    }
    if let Some(session_deltas) = stat.session_deltas {
        metrics.push(format!("{session_deltas} session deltas"));
    }
    if let Some(last_dirty_set_size) = stat.last_dirty_set_size {
        metrics.push(format!("last dirty set {last_dirty_set_size}"));
    }
    if let Some(detail) = &stat.detail {
        metrics.push(detail.clone());
    }
    let body = if metrics.is_empty() {
        stat.empty_state
            .clone()
            .unwrap_or_else(|| "unwired".to_string())
    } else {
        metrics.join("; ")
    };
    format!("{} ({}): {body}", stat.label, stat.kind)
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
        // Drag-reorder (the configurable menu list, B2): a dimmed grip glyph; the dragged row
        // fades; the drop target draws an accent line at its top edge ("drop before here").
        format!(
            ".app-reorder-grip {{ font-size: 15px; color: {}; }}",
            rgb(c.muted_text)
        ),
        ".reorder-dragging { opacity: 0.45; }".to_string(),
        format!(
            ".reorder-drop {{ border-top: 2px solid {}; }}",
            rgb(c.active_bg)
        ),
    ]
}

/// Build the apparatus pane's item list: the host observability sections as display rows.
/// The interactive settings sections (Theme / Engines / Physics) live in the pelt
/// settings lane, so the apparatus is read-only diagnostics now. The
/// [`ListPane`](crate::list_pane::ListPane) renders these display rows.
pub fn apparatus_items(
    system_rows: &[(String, String)],
    table_stats: &[ApparatusTableStat],
    sync_rows: &[(String, String)],
    obs: &ObservabilitySnapshot,
    graph_metrics: &glossary::GraphMetrics,
) -> Vec<PaneItem> {
    let mut items = Vec::new();

    items.push(PaneItem::text("app-title", "Overview"));
    for (label, value) in system_rows {
        items.push(PaneItem::text("app-row", format!("{label}: {value}")));
    }

    // The graph's full diagnostic breakdown (settles the gloss-outline plan's metrics
    // surface split): gloss keeps only bare node/edge/component counts for a glance
    // beside the minimap; everything with an interpretive angle — the relation-family
    // histogram, orphan detail, largest-component sizing — lives here instead. The
    // first graph-content section in apparatus. (gloss-outline plan, settled 2026-07-01.)
    items.push(PaneItem::text("app-title", "Graph"));
    items.push(PaneItem::text(
        "app-row",
        format!(
            "Nodes: {}; links: {}",
            graph_metrics.node_count, graph_metrics.edge_count
        ),
    ));
    items.push(PaneItem::text(
        "app-row",
        format!(
            "Components: {} (largest {})",
            graph_metrics.component_count, graph_metrics.largest_component
        ),
    ));
    items.push(PaneItem::text(
        "app-row",
        format!("Orphans: {}", graph_metrics.orphan_count),
    ));
    if graph_metrics.relations_by_family.is_empty() {
        items.push(PaneItem::text("app-row-muted", "No relations yet"));
    } else {
        for (family, count) in &graph_metrics.relations_by_family {
            items.push(PaneItem::text(
                "app-row",
                format!("{}: {count}", crate::roster::edge_family_label(*family)),
            ));
        }
    }

    items.push(PaneItem::text("app-title", "Tables"));
    for stat in table_stats {
        let class = if stat.count.is_none() && stat.empty_state.is_some() {
            "app-row-muted"
        } else {
            "app-row"
        };
        items.push(PaneItem::text(class, format_table_stat(stat)));
    }

    // The at-rest sync record (the record half of the static-vs-live split; Steward
    // owns the live, actionable rows). (Chrome bar P1 — tessera out of the omnibar.)
    items.push(PaneItem::text("app-title", "Sync"));
    for (label, value) in sync_rows {
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
                format!(
                    "{} {} {} {}",
                    event.surface,
                    event.event,
                    detail,
                    age(event.at)
                ),
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
                format!(
                    "{} {} {} {}",
                    actor.actor,
                    actor.event,
                    detail,
                    age(actor.at)
                ),
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
        items.push(PaneItem::text(
            "app-row-muted",
            "No portable trace events yet",
        ));
    } else {
        for trace in &obs.traces {
            let detail = trace.detail.as_deref().unwrap_or("");
            items.push(PaneItem::text(
                "app-row",
                format!(
                    "{} {} {} {}",
                    trace.name,
                    trace.event,
                    detail,
                    age(trace.at)
                ),
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
            items.push(PaneItem::text(
                "app-row",
                format!("orphan {channel}: {count}"),
            ));
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
                format!(
                    "{} {}: {} {}",
                    probe.name,
                    probe.status,
                    probe.detail,
                    age(probe.at)
                ),
            ));
        }
    }

    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use kernel::graph::EdgeFamily;
    use std::collections::BTreeMap;

    #[test]
    fn graph_section_reports_the_full_metrics_breakdown() {
        let mut relations_by_family = BTreeMap::new();
        relations_by_family.insert(EdgeFamily::Semantic, 3);
        let metrics = glossary::GraphMetrics {
            node_count: 5,
            edge_count: 4,
            relation_count: 3,
            relations_by_family,
            orphan_count: 1,
            component_count: 2,
            largest_component: 4,
        };
        let items = apparatus_items(&[], &[], &[], &ObservabilitySnapshot::default(), &metrics);
        assert!(
            items
                .iter()
                .any(|i| i.class == "app-title" && i.text == "Graph")
        );
        assert!(
            items
                .iter()
                .any(|i| i.text.contains("Nodes: 5") && i.text.contains("links: 4"))
        );
        assert!(
            items
                .iter()
                .any(|i| i.text.contains("Components: 2") && i.text.contains("largest 4"))
        );
        assert!(items.iter().any(|i| i.text == "Orphans: 1"));
        assert!(items.iter().any(|i| i.text == "Semantic: 3"));
    }

    #[test]
    fn tables_section_renders_structured_stats_and_empty_state() {
        let metrics = glossary::GraphMetrics::default();
        let stats = vec![
            ApparatusTableStat::present("Node table", "kernel", 5, "rows")
                .with_session_deltas(12)
                .with_detail("graph nodes"),
            ApparatusTableStat::unavailable(
                "Document scene",
                "scene",
                "awaiting first scene on current lane (serval.web)",
            ),
        ];
        let items = apparatus_items(
            &[],
            &stats,
            &[],
            &ObservabilitySnapshot::default(),
            &metrics,
        );
        assert!(items.iter().any(|i| {
            i.text.contains("Node table (kernel): 5 rows")
                && i.text.contains("12 session deltas")
                && i.text.contains("graph nodes")
        }));
        assert!(items.iter().any(|i| {
            i.class == "app-row-muted"
                && i.text
                    == "Document scene (scene): awaiting first scene on current lane (serval.web)"
        }));
    }
}
