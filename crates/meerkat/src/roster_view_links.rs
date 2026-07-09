/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Rendering for the Links tab: the relation-cell table and the Link Card
//! (family grouping, relate/retract/hide/show). Split out of
//! `roster_view_parts.rs` per the 600-LOC ceiling.

use mere::forme::GraphMemberId;
use mere::kernel::graph::EdgeFamily;
use xilem_serval::{Keyed, PointerClick, clickable, el};

use crate::roster::{LinkCard, LinkRelationRow, LinkRow, RELATE_PICKER_KINDS, RosterSubject};
use crate::roster_view::{RosterIntent, RosterState, RosterView};
use crate::roster_view_parts::{action, action_bar, card_shell};

#[derive(Clone, PartialEq, Eq, Hash)]
enum LinkTableKey {
    Bundle {
        from: GraphMemberId,
        to: GraphMemberId,
    },
    Row {
        from: GraphMemberId,
        to: GraphMemberId,
        selector: mere::kernel::graph::RelationSelector,
    },
}

pub(crate) fn link_table(rows: &[LinkRow]) -> RosterView {
    let mut children: Vec<(LinkTableKey, RosterView)> = Vec::new();
    for row in rows {
        if row.starts_bundle {
            children.push((
                LinkTableKey::Bundle {
                    from: row.from,
                    to: row.to,
                },
                bundle_section(row),
            ));
        }
        let subject = RosterSubject::RelationCell {
            from: row.from,
            to: row.to,
            selector: row.selector,
        };
        let mut cells: Vec<RosterView> = vec![
            Box::new(
                el::<_, RosterState, ()>("span", row.source_title.clone())
                    .attr("class", "roster-link-cell")
                    .attr("title", row.source_url.clone()),
            ),
            Box::new(
                el::<_, RosterState, ()>("span", row.direction_label.clone())
                    .attr("class", "roster-link-arrow"),
            ),
            Box::new(
                el::<_, RosterState, ()>("span", row.target_title.clone())
                    .attr("class", "roster-link-cell")
                    .attr("title", row.target_url.clone()),
            ),
            Box::new(
                el::<_, RosterState, ()>("span", row.family_label.clone())
                    .attr("class", "roster-link-kind"),
            ),
            Box::new(
                el::<_, RosterState, ()>("span", row.kind_label.clone())
                    .attr("class", "roster-link-kind"),
            ),
        ];
        if let Some(label) = &row.source_label {
            cells.push(Box::new(
                el::<_, RosterState, ()>("span", label.clone()).attr("class", "roster-link-cell"),
            ));
        }
        let class = if row.selected {
            "roster-row-selected"
        } else {
            "roster-row"
        };
        children.push((
            LinkTableKey::Row {
                from: row.from,
                to: row.to,
                selector: row.selector,
            },
            Box::new(clickable(
                el::<_, RosterState, ()>("div", cells)
                    .attr("class", format!("{class} roster-link-grid")),
                move |st: &mut RosterState, _: PointerClick| {
                    st.open_subject(subject.clone());
                },
            )),
        ));
    }
    let children: Keyed<LinkTableKey, RosterView> = children.into();
    Box::new(el::<_, RosterState, ()>("div", children).attr("class", "roster-table"))
}

fn bundle_section(row: &LinkRow) -> RosterView {
    let subject = RosterSubject::LinkBundle {
        from: row.from,
        to: row.to,
    };
    Box::new(clickable(
        el::<_, RosterState, ()>(
            "div",
            format!("{} -> {}", row.source_title, row.target_title),
        )
        .attr("class", "roster-section roster-link-bundle")
        .attr("title", format!("{} -> {}", row.source_url, row.target_url)),
        move |st: &mut RosterState, _: PointerClick| {
            st.open_subject(subject.clone());
        },
    ))
}

pub(crate) fn link_card(state: &RosterState, card: &LinkCard) -> RosterView {
    let mut rows = card_shell(
        &format!("{} -> {}", card.source_title, card.target_title),
        &format!("{} -> {}", card.source_url, card.target_url),
    );
    rows.extend(crate::roster_facet_view::facet_links(&card.facets));
    let picker_open = state.relate_picker == Some((card.from, card.to));
    rows.push(action_bar(link_card_actions(card)));
    if picker_open {
        rows.push(relate_picker_choices(card));
    }
    rows.extend(link_relation_groups(card));
    Box::new(el::<_, RosterState, ()>("div", rows).attr("class", "roster-detail"))
}

fn link_card_actions(card: &LinkCard) -> Vec<RosterView> {
    let mut actions = Vec::new();
    let from = card.from;
    let to = card.to;
    actions.push(endpoint_action("select source", from));
    actions.push(endpoint_action("select target", to));
    actions.push(action("relate as...", move |st, ev| {
        ev.stop_propagation();
        st.toggle_relate_picker(from, to);
    }));
    if card.hidden {
        actions.push(action("show bundle", move |st, ev| {
            ev.stop_propagation();
            st.pending.push(RosterIntent::ShowLinkBundle { from, to });
        }));
    } else {
        actions.push(action("hide bundle", move |st, ev| {
            ev.stop_propagation();
            st.pending.push(RosterIntent::HideLinkBundle { from, to });
        }));
    }
    actions
}

fn relate_picker_choices(card: &LinkCard) -> RosterView {
    let from = card.from;
    let to = card.to;
    let mut choices = Vec::new();
    for &(kind, label) in RELATE_PICKER_KINDS {
        choices.push(action(label, move |st, ev| {
            ev.stop_propagation();
            st.relate_picker = None;
            st.pending.push(RosterIntent::RelateAs { from, to, kind });
        }));
    }
    Box::new(
        el::<_, RosterState, ()>("div", vec![action_bar(choices)])
            .attr("class", "roster-relate-picker"),
    )
}

fn link_relation_groups(card: &LinkCard) -> Vec<RosterView> {
    let mut out: Vec<RosterView> = Vec::new();
    let mut last: Option<EdgeFamily> = None;
    for rel in &card.relations {
        if last != Some(rel.family) {
            last = Some(rel.family);
            let class = if card
                .relations
                .iter()
                .any(|candidate| candidate.family == rel.family && candidate.selected)
            {
                "roster-card-group-title-selected"
            } else {
                "roster-card-group-title"
            };
            out.push(Box::new(
                el::<_, RosterState, ()>("div", rel.family_label.clone()).attr("class", class),
            ));
        }
        out.push(link_relation_row(rel));
    }
    out
}

fn link_relation_row(rel: &LinkRelationRow) -> RosterView {
    let subject = RosterSubject::RelationCell {
        from: rel.from,
        to: rel.to,
        selector: rel.selector,
    };
    let label = rel
        .label
        .as_ref()
        .map(|label| format!("{} · {label}", rel.kind_label))
        .unwrap_or_else(|| rel.kind_label.clone());
    let label = if rel.hidden {
        format!("{label} (hidden)")
    } else {
        label
    };
    let mut cells: Vec<RosterView> = vec![Box::new(
        el::<_, RosterState, ()>("span", label).attr("class", "roster-link-cell"),
    )];
    let from = rel.from;
    let to = rel.to;
    let selector = rel.selector;
    if rel.hidden {
        cells.push(action("show", move |st, ev| {
            ev.stop_propagation();
            st.pending
                .push(RosterIntent::ShowRelation { from, to, selector });
        }));
    } else {
        cells.push(action("hide", move |st, ev| {
            ev.stop_propagation();
            st.pending
                .push(RosterIntent::HideRelation { from, to, selector });
        }));
    }
    if rel.editable {
        cells.push(action("retract", move |st, ev| {
            ev.stop_propagation();
            st.pending
                .push(RosterIntent::RetractRelation { from, to, selector });
        }));
    }
    let class = if rel.selected {
        "roster-row-selected"
    } else {
        "roster-row"
    };
    Box::new(clickable(
        el::<_, RosterState, ()>("div", cells).attr("class", class),
        move |st: &mut RosterState, _: PointerClick| {
            st.open_subject(subject.clone());
        },
    ))
}

fn endpoint_action(label: &'static str, member: GraphMemberId) -> RosterView {
    action(label, move |st, ev| {
        ev.stop_propagation();
        st.pending.push(RosterIntent::Select(member));
    })
}
