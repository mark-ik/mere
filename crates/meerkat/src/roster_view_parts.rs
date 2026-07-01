/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Rendering helpers for the tabbed Roster tables and in-roster detail cards:
//! tab dispatch, the Nodes and Fields tabs, and the shared card-building
//! primitives (`action`, `action_bar`, `card_shell`, ...) the Links
//! ([`crate::roster_view_links`]) and Graphlets ([`crate::roster_view_graphlets`])
//! tabs also build on. Split out of one file per the 600-LOC ceiling.

use xilem_serval::{PointerClick, ServalCtx, ServalElement, clickable, el, on_click};

use crate::roster::{
    FieldDetail, FieldRow, NodeDetail, RosterDetail, RosterRow, RosterSubject, RosterTab,
};
use crate::roster_view::{RosterIntent, RosterState, RosterView};

pub(crate) fn tab_strip(state: &RosterState) -> RosterView {
    let tabs: Vec<RosterView> = RosterTab::ALL
        .iter()
        .map(|&tab| {
            let class = if state.active_tab == tab {
                "roster-tab roster-tab-active"
            } else {
                "roster-tab"
            };
            Box::new(clickable(
                el::<_, RosterState, ()>("div", tab.label()).attr("class", class),
                move |st: &mut RosterState, _: PointerClick| {
                    st.active_tab = tab;
                    st.relate_picker = None;
                    st.pending.push(RosterIntent::SetTab(tab));
                },
            )) as RosterView
        })
        .collect();
    Box::new(el::<_, RosterState, ()>("div", tabs).attr("class", "roster-tabs"))
}

pub(crate) fn active_table(state: &RosterState) -> RosterView {
    let rows = match state.active_tab {
        RosterTab::Nodes => node_table(&state.node_rows),
        RosterTab::Links => crate::roster_view_links::link_table(&state.link_rows),
        RosterTab::Graphlets => crate::roster_view_graphlets::graphlet_table(&state.graphlet_rows),
        RosterTab::Fields => field_table(&state.field_rows),
    };
    if rows.is_empty() {
        return Box::new(
            el::<_, RosterState, ()>("div", state.active_tab.empty_label())
                .attr("class", "roster-empty"),
        );
    }
    Box::new(el::<_, RosterState, ()>("div", rows).attr("class", "roster-table"))
}

pub(crate) fn detail_card(state: &RosterState, detail: &RosterDetail) -> RosterView {
    match detail {
        RosterDetail::Node(card) => node_card(card),
        RosterDetail::Link(card) => crate::roster_view_links::link_card(state, card),
        RosterDetail::Graphlet(card) => crate::roster_view_graphlets::graphlet_card(card),
        RosterDetail::Field(card) => field_card(card),
        RosterDetail::Facet(card) => crate::roster_facet_view::facet_card(card),
    }
}

fn node_table(rows: &[RosterRow]) -> Vec<RosterView> {
    let mut children: Vec<RosterView> = Vec::new();
    for row in rows {
        if let Some(header) = &row.section_header {
            children.push(section(header));
        }
        let mut entry: Vec<RosterView> = vec![
            Box::new(
                el::<_, RosterState, ()>("div", row.title.clone()).attr("class", "roster-title"),
            ),
            Box::new(el::<_, RosterState, ()>("div", row.url.clone()).attr("class", "roster-sub")),
        ];
        let mut facets: Vec<RosterView> = Vec::new();
        if let Some(ct) = &row.content_type {
            facets.push(chip("roster-chip", ct.clone()));
        }
        if row.selected {
            facets.push(chip("roster-chip", "selected"));
        }
        if row.open {
            facets.push(chip("roster-chip", "open"));
        }
        for tag in &row.tags {
            facets.push(chip("roster-tag", tag.clone()));
        }
        if !facets.is_empty() {
            entry.push(Box::new(
                el::<_, RosterState, ()>("div", facets).attr("class", "roster-facets"),
            ));
        }
        let member = row.member;
        let subject = RosterSubject::Node(member);
        let class = if row.selected {
            "roster-row-selected"
        } else {
            "roster-row"
        };
        children.push(Box::new(clickable(
            el::<_, RosterState, ()>("div", entry)
                .attr("class", class)
                .attr("data-member", member.to_string()),
            move |st: &mut RosterState, _: PointerClick| {
                st.open_subject(subject.clone());
                st.pending.push(RosterIntent::Select(member));
            },
        )));
    }
    children
}

fn field_table(rows: &[FieldRow]) -> Vec<RosterView> {
    let mut children: Vec<RosterView> = vec![field_header()];
    for fr in rows {
        let id = fr.id;
        let subject = RosterSubject::Field(id);
        let entry: Vec<RosterView> = vec![
            Box::new(
                el::<_, RosterState, ()>("span", fr.name.clone())
                    .attr("class", "roster-field-name"),
            ),
            Box::new(
                el::<_, RosterState, ()>("span", fr.rule_label.clone())
                    .attr("class", "roster-field-meta roster-field-rule"),
            ),
            Box::new(
                el::<_, RosterState, ()>("span", fr.extent_label.clone())
                    .attr("class", "roster-field-meta roster-field-extent"),
            ),
            field_visibility_cell(id, fr.hidden),
            field_strength_cell(id, fr.strength),
        ];
        let class = if fr.selected {
            "roster-field roster-field-selected"
        } else if fr.hidden {
            "roster-field roster-field-hidden"
        } else {
            "roster-field"
        };
        children.push(Box::new(clickable(
            el::<_, RosterState, ()>("div", entry).attr("class", class),
            move |st: &mut RosterState, _: PointerClick| {
                st.open_subject(subject.clone());
                st.pending.push(RosterIntent::SelectField(id));
            },
        )));
    }
    children
}

fn field_header() -> RosterView {
    Box::new(
        el::<_, RosterState, ()>("div", "Field | Rule | Extent | Visibility | Strength")
            .attr("class", "roster-field-header"),
    )
}

fn field_visibility_cell(id: kernel::graph::FieldId, hidden: bool) -> RosterView {
    let status = if hidden { "hidden" } else { "visible" };
    let toggle_label = if hidden { "show" } else { "hide" };
    let cells: Vec<RosterView> = vec![
        Box::new(el::<_, RosterState, ()>("span", status).attr("class", "roster-field-meta")),
        Box::new(on_click(
            el::<_, RosterState, ()>("span", toggle_label).attr("class", "roster-field-toggle"),
            move |st: &mut RosterState, ev: PointerClick| {
                ev.stop_propagation();
                st.pending.push(RosterIntent::ToggleFieldVisibility(id));
            },
        )),
    ];
    Box::new(el::<_, RosterState, ()>("span", cells).attr("class", "roster-field-visibility"))
}

fn field_strength_cell(id: kernel::graph::FieldId, strength: f32) -> RosterView {
    let cells: Vec<RosterView> = vec![
        Box::new(field_step(id, "-", -1000.0)),
        Box::new(
            el::<_, RosterState, ()>("span", format!("{:.0}", strength / 1000.0))
                .attr("class", "roster-field-strength"),
        ),
        Box::new(field_step(id, "+", 1000.0)),
    ];
    Box::new(el::<_, RosterState, ()>("span", cells).attr("class", "roster-field-strength-cell"))
}

fn node_card(card: &NodeDetail) -> RosterView {
    let mut rows = card_shell(&card.title, &card.url);
    rows.push(card_row(format!("relations: {}", card.relation_count)));
    rows.push(card_row(format!(
        "state: {}",
        if card.open { "open" } else { "not open" }
    )));
    if let Some(ct) = &card.content_type {
        rows.push(card_row(format!("content: {ct}")));
    }
    if !card.tags.is_empty() {
        rows.push(card_row(format!("tags: {}", card.tags.join(", "))));
    }
    rows.extend(crate::roster_facet_view::facet_links(&card.facets));
    let member = card.member;
    rows.push(action_bar(vec![action("select node", move |st, ev| {
        ev.stop_propagation();
        st.pending.push(RosterIntent::Select(member));
    })]));
    Box::new(el::<_, RosterState, ()>("div", rows).attr("class", "roster-detail"))
}

fn field_card(card: &FieldDetail) -> RosterView {
    let mut rows = card_shell(&card.name, &format!("{}", card.id.as_uuid()));
    rows.push(card_row(format!("rule: {}", card.rule_label)));
    rows.push(card_row(format!("extent: {}", card.extent_label)));
    rows.push(card_row(format!(
        "visibility: {}",
        if card.hidden { "hidden" } else { "visible" }
    )));
    rows.push(card_row(format!("strength: {:.0}", card.strength / 1000.0)));
    rows.extend(crate::roster_facet_view::facet_links(&card.facets));
    let id = card.id;
    rows.push(action_bar(vec![
        action(if card.hidden { "show" } else { "hide" }, move |st, ev| {
            ev.stop_propagation();
            st.pending.push(RosterIntent::ToggleFieldVisibility(id));
        }),
        action("weaker", move |st, ev| {
            ev.stop_propagation();
            st.pending
                .push(RosterIntent::AdjustFieldStrength(id, -1000.0));
        }),
        action("stronger", move |st, ev| {
            ev.stop_propagation();
            st.pending
                .push(RosterIntent::AdjustFieldStrength(id, 1000.0));
        }),
    ]));
    Box::new(el::<_, RosterState, ()>("div", rows).attr("class", "roster-detail"))
}

fn field_step(
    id: kernel::graph::FieldId,
    label: &'static str,
    delta: f32,
) -> impl xilem_serval::View<RosterState, (), ServalCtx, Element = ServalElement> {
    on_click(
        el::<_, RosterState, ()>("span", label).attr("class", "roster-field-step"),
        move |st: &mut RosterState, ev: PointerClick| {
            ev.stop_propagation();
            st.pending
                .push(RosterIntent::AdjustFieldStrength(id, delta));
        },
    )
}

/// Shared card-building primitives — used by the Nodes/Fields tabs here and by
/// [`crate::roster_view_links`] / [`crate::roster_view_graphlets`].
pub(crate) fn card_shell(title: &str, sub: &str) -> Vec<RosterView> {
    vec![
        Box::new(
            el::<_, RosterState, ()>("div", title.to_string()).attr("class", "roster-card-title"),
        ),
        Box::new(el::<_, RosterState, ()>("div", sub.to_string()).attr("class", "roster-card-sub")),
    ]
}

pub(crate) fn card_row(text: impl Into<String>) -> RosterView {
    Box::new(el::<_, RosterState, ()>("div", text.into()).attr("class", "roster-card-row"))
}

pub(crate) fn action_bar(actions: Vec<RosterView>) -> RosterView {
    Box::new(el::<_, RosterState, ()>("div", actions).attr("class", "roster-card-actions"))
}

pub(crate) fn action(
    label: impl Into<String>,
    handler: impl Fn(&mut RosterState, PointerClick) + 'static,
) -> RosterView {
    Box::new(on_click(
        el::<_, RosterState, ()>("span", label.into()).attr("class", "roster-card-action"),
        handler,
    ))
}

fn section(text: impl Into<String>) -> RosterView {
    Box::new(el::<_, RosterState, ()>("div", text.into()).attr("class", "roster-section"))
}

fn chip(class: &'static str, text: impl Into<String>) -> RosterView {
    Box::new(el::<_, RosterState, ()>("span", text.into()).attr("class", class))
}
