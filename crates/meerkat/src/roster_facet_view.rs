/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Facet rows and Facet Card rendering for the graph-object Roster.

use cambium::{PointerClick, clickable, el, on_click};

use crate::roster::{FacetAction, FacetActionIntent, FacetCard, FacetEntry, RosterSubject};
use crate::roster_view::{RosterIntent, RosterState, RosterView};

pub(crate) fn facet_links(facets: &[FacetEntry]) -> Vec<RosterView> {
    if facets.is_empty() {
        return Vec::new();
    }
    let mut out = vec![group_title("Facets")];
    out.extend(facets.iter().map(facet_link));
    out
}

pub(crate) fn facet_card(card: &FacetCard) -> RosterView {
    let mut rows = vec![card_title(&card.title), card_sub(&card.subtitle)];
    rows.extend(
        card.rows
            .iter()
            .map(|row| facet_info(&row.label, &row.value)),
    );
    if !card.actions.is_empty() {
        rows.push(action_bar(card.actions.iter().map(facet_action).collect()));
    }
    Box::new(el::<_, RosterState, ()>("div", rows).attr("class", "roster-detail"))
}

fn facet_link(facet: &FacetEntry) -> RosterView {
    let subject = facet.subject.clone();
    let cells: Vec<RosterView> = vec![
        Box::new(
            el::<_, RosterState, ()>("span", facet.label.clone())
                .attr("class", "roster-facet-label"),
        ),
        Box::new(
            el::<_, RosterState, ()>("span", facet.value.clone())
                .attr("class", "roster-facet-value"),
        ),
    ];
    Box::new(clickable(
        el::<_, RosterState, ()>("div", cells).attr("class", "roster-facet-row"),
        move |st: &mut RosterState, _: PointerClick| {
            st.open_subject(subject.clone());
        },
    ))
}

fn facet_info(label: &str, value: &str) -> RosterView {
    let cells: Vec<RosterView> = vec![
        Box::new(
            el::<_, RosterState, ()>("span", label.to_string()).attr("class", "roster-facet-label"),
        ),
        Box::new(
            el::<_, RosterState, ()>("span", value.to_string()).attr("class", "roster-facet-value"),
        ),
    ];
    Box::new(el::<_, RosterState, ()>("div", cells).attr("class", "roster-facet-row"))
}

fn facet_action(action: &FacetAction) -> RosterView {
    let label = action.label.clone();
    let intent = action.intent.clone();
    Box::new(on_click(
        el::<_, RosterState, ()>("span", label).attr("class", "roster-card-action"),
        move |st: &mut RosterState, ev: PointerClick| {
            ev.stop_propagation();
            match intent {
                FacetActionIntent::SelectNode(member) => {
                    st.pending.push(RosterIntent::Select(member))
                }
                FacetActionIntent::SelectField(id) => {
                    st.pending.push(RosterIntent::SelectField(id))
                }
                FacetActionIntent::ToggleFieldVisibility(id) => {
                    st.pending.push(RosterIntent::ToggleFieldVisibility(id));
                }
                FacetActionIntent::AdjustFieldStrength(id, delta) => {
                    st.pending
                        .push(RosterIntent::AdjustFieldStrength(id, delta));
                }
                FacetActionIntent::OpenLinkBundle { from, to } => {
                    let subject = RosterSubject::LinkBundle { from, to };
                    st.open_subject(subject);
                }
            }
        },
    ))
}

fn card_title(text: &str) -> RosterView {
    Box::new(el::<_, RosterState, ()>("div", text.to_string()).attr("class", "roster-card-title"))
}

fn card_sub(text: &str) -> RosterView {
    Box::new(el::<_, RosterState, ()>("div", text.to_string()).attr("class", "roster-card-sub"))
}

fn group_title(text: &str) -> RosterView {
    Box::new(
        el::<_, RosterState, ()>("div", text.to_string()).attr("class", "roster-card-group-title"),
    )
}

fn action_bar(actions: Vec<RosterView>) -> RosterView {
    Box::new(el::<_, RosterState, ()>("div", actions).attr("class", "roster-card-actions"))
}
