/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Steward pane data: the live, actionable plane of the static-vs-live split (the
//! [peripheral panes architecture](../../../design_docs/mere_docs/technical_architecture/2026-06-06_peripheral_panes_architecture.md)).
//! Steward is the process / actor monitor — running content operations and the
//! live tessera/sync lane, each shown as status you can act on (stop / pin /
//! retry). The at-rest *record* half (session totals, traces) lives in Apparatus
//! (`pane_data::apparatus_sync_rows`, `apparatus::apparatus_items`).
//!
//! Factored out of `pane_data.rs` to keep files under the 600-LOC ceiling and to
//! co-locate the Steward pane (its status rows, its clickable verbs, and the live
//! sync readout) as one module. (Chrome bar P2.)

use forme::GraphMemberId;
use frame::PaneContent;

use super::{WindowCtx, fetch};

/// How many live operations the process list shows before collapsing the tail into
/// a muted "+N more" note (no silent cap — the dropped count is always stated).
const SHOWN_OPS: usize = 24;

impl WindowCtx<'_> {
    /// Steward's status rows: the live-ops counters, the focused-operation summary,
    /// and the live tessera/sync readout. The per-operation *list* (with its
    /// clickable verbs) is built in [`steward_items`](Self::steward_items), not here
    /// — these are the display-only `label: value` pairs above it.
    pub(super) fn steward_rows(&self) -> Vec<(String, String)> {
        let operations = self.shared.content.constellation.active_operations();
        let mut rows = vec![
            (
                "Active operations".to_string(),
                operations.len().to_string(),
            ),
            ("Tab cap".to_string(), self.shared.presentation.saved_tab_cap.to_string()),
            (
                "Live graphs".to_string(),
                format!("{} / {}", self.orrery_pool_count, super::MAX_POOLED_ORRERIES),
            ),
            (
                "Loading fetches".to_string(),
                self.fetch_state_count(1).to_string(),
            ),
            (
                "Failed fetches".to_string(),
                self.fetch_state_count(3).to_string(),
            ),
            (
                // Athanor's last forgetting pass, surfaced live here (not only in the
                // Apparatus diagnostics log). A real tracked op, no placebo. (Alembic B2.)
                "Last forgetting".to_string(),
                match self.shared.observability.last_forgetting() {
                    Some(p) => format!(
                        "dropped {} page(s) \u{b7} {}",
                        p.dropped,
                        crate::observability::age(p.at)
                    ),
                    None => "not run yet".to_string(),
                },
            ),
        ];
        // The live tessera/sync readout — the live, actionable plane of the
        // static-vs-live split (Steward owns live + actionable, Apparatus owns the
        // at-rest record). Lane status, whether a round is in flight, the earned
        // standing, and the dialable ticket to share. (Chrome bar P1; tessera out of
        // the omnibar.)
        rows.extend(self.steward_sync_rows());
        let focused = self.focused_member();
        rows.push((
            "Focused operation".to_string(),
            match focused {
                Some(member) if self.shared.content.constellation.is_active(member) => format!(
                    "active background={} recovering={}",
                    self.shared.content.constellation.is_background(member),
                    self.shared.content.constellation.is_recovering(member)
                ),
                Some(_) => "dormant".to_string(),
                None => "none".to_string(),
            },
        ));
        rows
    }

    /// The Steward pane as a clickable item list: the status rows (from
    /// [`steward_rows`](Self::steward_rows)), the focused-operation verbs (act on
    /// whatever is focused — useful when the focused node is *dormant* and so not in
    /// the live list), then the **live process list** — one row per active operation
    /// with its own stop / pin / retry buttons keyed by member, so each verb acts on
    /// that specific process, not only the focused one. The drain
    /// (`drain_list_pane_activations`) routes every `steward:*` key to a node-ops
    /// verb, so the actions are real, not a typed-verb hint. (Chrome bar P2; Audit A2.)
    pub(super) fn steward_items(&self) -> Vec<crate::list_pane::PaneItem> {
        use crate::list_pane::PaneItem;
        let mut items = vec![PaneItem::text(
            "utility-title",
            crate::utility_panes::pane_title(&PaneContent::Steward),
        )];
        for (label, value) in self.steward_rows() {
            items.push(PaneItem::text("utility-row", format!("{label}: {value}")));
        }
        // Focused-operation verbs (always present): act on the focused node, which may
        // be dormant and so absent from the live list below. The drain maps each key
        // to its node-ops method.
        items.push(PaneItem::button("utility-row", "\u{21bb} retry focused", "steward:retry"));
        items.push(PaneItem::button("utility-row", "\u{23f9} stop focused", "steward:stop"));
        items.push(PaneItem::button(
            "utility-row",
            "\u{2693} pin focused (background)",
            "steward:pin",
        ));

        // The live process list: one row per active operation, each with per-row verbs
        // keyed by member id so the click acts on *that* process. (Chrome bar P2.)
        items.push(PaneItem::text("utility-title", "Live operations"));
        let operations = self.shared.content.constellation.active_operations();
        if operations.is_empty() {
            items.push(PaneItem::text("utility-row-muted", "No live operations"));
        }
        for op in operations.iter().take(SHOWN_OPS) {
            let state = if op.recovering {
                "recovering"
            } else if op.background {
                "background"
            } else {
                "active"
            };
            let url = op.url.as_deref().unwrap_or("not shown yet");
            items.push(PaneItem::text(
                "utility-row",
                format!("{} \u{b7} {} \u{b7} {}", short_member(op.member), url, state),
            ));
            let member = op.member;
            items.push(PaneItem::button(
                "utility-row",
                "\u{23f9} stop",
                format!("steward:stop:{member}"),
            ));
            items.push(PaneItem::button(
                "utility-row",
                "\u{2693} pin",
                format!("steward:pin:{member}"),
            ));
            items.push(PaneItem::button(
                "utility-row",
                "\u{21bb} retry",
                format!("steward:retry:{member}"),
            ));
        }
        if operations.len() > SHOWN_OPS {
            items.push(PaneItem::text(
                "utility-row-muted",
                format!("+{} more (showing {SHOWN_OPS})", operations.len() - SHOWN_OPS),
            ));
        }

        items.push(PaneItem::text(
            "utility-row-muted",
            crate::utility_panes::pane_status(&PaneContent::Steward),
        ));
        items
    }

    fn fetch_state_count(&self, tag: u8) -> usize {
        self.shared.content.pages
            .values()
            .filter(|state| fetch::ContentState::tag(Some(*state)) == tag)
            .count()
    }

    /// The live tessera/sync rows for Steward (the live, actionable plane). The
    /// at-rest record (session op total, last-activity age) lives in Apparatus's Sync
    /// section instead — Steward shows only what is current and acted on: the lane,
    /// whether a round is in flight, the earned standing (the point of tessera), and
    /// the dialable ticket. (Chrome bar P1.)
    fn steward_sync_rows(&self) -> Vec<(String, String)> {
        let indicator = &self.view.chrome().sync;
        if !indicator.active {
            return vec![("Sync lane".to_string(), "off".to_string())];
        }
        let mut rows = vec![
            ("Sync lane".to_string(), indicator.label.clone()),
            (
                "Syncing now".to_string(),
                if indicator.syncing { "yes" } else { "no" }.to_string(),
            ),
        ];
        // The earned standing is the headline (the whole reason tessera exists);
        // negative reads as debt. Shown only once a ledger has folded.
        if let Some(standing) = indicator.standing {
            rows.push(("Standing".to_string(), format!("{standing:+}")));
        }
        // The dialable ticket, readable + shareable here instead of only in the logs.
        // (Tessera ticket surface.)
        rows.push((
            "Tessera ticket".to_string(),
            indicator
                .ticket()
                .map(str::to_string)
                .unwrap_or_else(|| "\u{2014}".to_string()),
        ));
        rows
    }
}

/// A short, stable prefix of a member's UUID, for the process-list row labels.
fn short_member(member: GraphMemberId) -> String {
    member.to_string().chars().take(8).collect()
}
