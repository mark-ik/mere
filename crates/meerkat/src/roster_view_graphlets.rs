/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Rendering for the Graphlets tab: the graphlet table and the Graphlet Card
//! (binding, members, selectors, drift preview, control actions). Split out
//! of `roster_view_parts.rs` per the 600-LOC ceiling.

use xilem_serval::{PointerClick, clickable, el};

use crate::roster::{GraphletCard, GraphletRow, RosterSubject};
use crate::roster_view::{RosterIntent, RosterState, RosterView};
use crate::roster_view_parts::{action_bar, card_row, card_shell};

pub(crate) fn graphlet_table(rows: &[GraphletRow]) -> Vec<RosterView> {
    let mut children: Vec<RosterView> = Vec::new();
    for row in rows {
        let subject = RosterSubject::Graphlet(row.id);
        let entry: Vec<RosterView> = vec![
            Box::new(
                el::<_, RosterState, ()>("div", format!("{} #{}", row.kind_label, row.id))
                    .attr("class", "roster-title"),
            ),
            Box::new(
                el::<_, RosterState, ()>(
                    "div",
                    format!(
                        "{} · {} members · {} · {}",
                        row.binding_label, row.member_count, row.selectors_label, row.drift_label
                    ),
                )
                .attr("class", "roster-sub"),
            ),
        ];
        let class = if row.selected {
            "roster-row-selected"
        } else {
            "roster-row"
        };
        children.push(Box::new(clickable(
            el::<_, RosterState, ()>("div", entry).attr("class", class),
            move |st: &mut RosterState, _: PointerClick| {
                st.open_subject(subject.clone());
            },
        )));
    }
    children
}

pub(crate) fn graphlet_card(card: &GraphletCard) -> RosterView {
    let mut rows = card_shell(
        &format!("{} #{}", card.kind_label, card.id),
        &card.binding_label,
    );
    rows.push(card_row(format!("members: {}", card.members.len())));
    if !card.members.is_empty() {
        rows.push(card_row(format!("members: {}", card.members.join(", "))));
    }
    rows.push(card_row(format!("selectors: {}", card.selectors_label)));
    rows.push(card_row(if card.drift_tracking {
        "drift tracking: linked".to_string()
    } else {
        "drift tracking: off".to_string()
    }));
    rows.push(card_row(card.drift_summary.clone()));
    rows.extend(graphlet_drift_rows(card));
    let id = card.id;
    rows.push(action_bar(vec![
        graphlet_action("apply drift", id, RosterIntent::ReconcileGraphlet),
        graphlet_action("keep session", id, RosterIntent::KeepGraphletAsSession),
        graphlet_action("branch", id, RosterIntent::BranchGraphlet),
        graphlet_action("open window", id, RosterIntent::OpenGraphlet),
    ]));
    Box::new(el::<_, RosterState, ()>("div", rows).attr("class", "roster-detail"))
}

fn graphlet_drift_rows(card: &GraphletCard) -> Vec<RosterView> {
    if card.added.is_empty() && card.removed.is_empty() {
        return Vec::new();
    }
    let mut rows = Vec::new();
    if !card.added.is_empty() {
        rows.push(card_row(format!("would add: {}", card.added.join(", "))));
    }
    if !card.removed.is_empty() {
        rows.push(card_row(format!(
            "would remove: {}",
            card.removed.join(", ")
        )));
    }
    rows
}

fn graphlet_action(
    label: &'static str,
    id: forme::GraphletId,
    intent: fn(forme::GraphletId) -> RosterIntent,
) -> RosterView {
    crate::roster_view_parts::action(label, move |st, ev| {
        ev.stop_propagation();
        st.pending.push(intent(id));
    })
}
