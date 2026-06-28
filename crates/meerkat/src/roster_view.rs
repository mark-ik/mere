/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The roster as a view-driven pane: a tabbed graph-element table plus an
//! in-roster detail-card region. Row handlers queue intents through the shell
//! runner; the host applies graph mutations and graphlet window commands.

use forme::{GraphMemberId, GraphletId};
use kernel::graph::{FieldId, RelationSelector, SemanticSubKind};
#[cfg(test)]
use layout_dom_api::LayoutDom;
#[cfg(test)]
use netrender::Scene;
#[cfg(test)]
use register_theme::chrome::ChromeTheme;
#[cfg(test)]
use serval_layout::ScrollOffsets;
#[cfg(test)]
use serval_scripted_dom::NodeId;
#[cfg(test)]
use xilem_serval::PointerClick;
use xilem_serval::{AnyView, ServalCtx, ServalElement, el};

use crate::roster::{
    FieldRow, GraphletRow, LinkRow, RosterDetail, RosterRow, RosterSubject, RosterTab,
};
#[cfg(test)]
use crate::roster::{RosterSnapshot, roster_sheet};
#[cfg(test)]
use crate::view_pane::ViewPane;

pub type RosterView = Box<dyn AnyView<RosterState, (), ServalCtx, ServalElement>>;

#[cfg(test)]
pub type RosterLogic = fn(&RosterState) -> RosterView;

#[derive(Clone, Debug, PartialEq)]
pub enum RosterIntent {
    SetTab(RosterTab),
    OpenDetail(RosterSubject),
    Select(GraphMemberId),
    SelectField(FieldId),
    ToggleFieldVisibility(FieldId),
    AdjustFieldStrength(FieldId, f32),
    RelateAs {
        from: GraphMemberId,
        to: GraphMemberId,
        kind: SemanticSubKind,
    },
    RetractRelation {
        from: GraphMemberId,
        to: GraphMemberId,
        selector: RelationSelector,
    },
    ReconcileGraphlet(GraphletId),
    KeepGraphletAsSession(GraphletId),
    BranchGraphlet(GraphletId),
    OpenGraphlet(GraphletId),
}

#[derive(Default)]
pub struct RosterState {
    pub active_tab: RosterTab,
    pub selected_subject: Option<RosterSubject>,
    pub node_rows: Vec<RosterRow>,
    pub link_rows: Vec<LinkRow>,
    pub graphlet_rows: Vec<GraphletRow>,
    pub field_rows: Vec<FieldRow>,
    pub detail: Option<RosterDetail>,
    pub pending: Vec<RosterIntent>,
}

pub fn roster_view(state: &RosterState) -> RosterView {
    let mut children: Vec<RosterView> = vec![
        crate::roster_view_parts::tab_strip(state),
        crate::roster_view_parts::active_table(state),
    ];
    if let Some(detail) = &state.detail {
        children.push(crate::roster_view_parts::detail_card(detail));
    }
    Box::new(el::<_, RosterState, ()>("div", children).attr("class", "roster"))
}

#[cfg(test)]
pub struct RosterPane {
    pane: ViewPane<RosterState, RosterLogic, RosterView>,
}

#[cfg(test)]
impl RosterPane {
    pub fn new() -> Self {
        Self {
            pane: ViewPane::new(roster_view as RosterLogic, RosterState::default()),
        }
    }

    pub fn set_snapshot(&mut self, theme: &ChromeTheme, snapshot: RosterSnapshot) {
        self.pane.set_sheets(roster_sheet(theme));
        self.pane.update(|s| {
            s.node_rows = snapshot.node_rows;
            s.link_rows = snapshot.link_rows;
            s.graphlet_rows = snapshot.graphlet_rows;
            s.field_rows = snapshot.field_rows;
            s.detail = snapshot.detail;
        });
    }

    pub fn set_active_tab(&mut self, tab: RosterTab) {
        self.pane.update(|s| s.active_tab = tab);
    }

    pub fn frame(&mut self, w: u32, h: u32, scroll: f32) -> Scene {
        let scrolls = self.scroll_offsets(scroll);
        self.pane.frame(w, h, &scrolls)
    }

    pub fn max_scroll(&self) -> f32 {
        let node = {
            let dom = self.pane.dom();
            let dom = dom.borrow();
            crate::first_with_class(&dom, dom.document(), "roster")
        };
        node.and_then(|n| self.pane.scroll_extent(n))
            .map_or(0.0, |(_, my)| my)
    }

    pub fn hit_test(&self, x: f32, y: f32, scroll: f32) -> Option<NodeId> {
        let scrolls = self.scroll_offsets(scroll);
        self.pane.hit_test(x, y, &scrolls)
    }

    pub fn dispatch_click(&mut self, node: NodeId, event: PointerClick) {
        self.pane.dispatch_click(node, event);
    }

    pub fn take_intents(&mut self) -> Vec<RosterIntent> {
        let mut out = Vec::new();
        self.pane.update(|s| out = std::mem::take(&mut s.pending));
        out
    }

    #[cfg(test)]
    pub(crate) fn dom(&self) -> std::rc::Rc<std::cell::RefCell<serval_scripted_dom::ScriptedDom>> {
        self.pane.dom()
    }

    fn scroll_offsets(&self, scroll: f32) -> ScrollOffsets<NodeId> {
        let mut offsets = ScrollOffsets::default();
        let dom = self.pane.dom();
        let dom = dom.borrow();
        if let Some(node) = crate::first_with_class(&dom, dom.document(), "roster") {
            offsets.insert(node, (0.0, scroll));
        }
        offsets
    }
}

#[cfg(test)]
impl Default for RosterPane {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use forme::GraphMemberId;
    use kernel::graph::{EdgeFamily, RelationSelector, SemanticSubKind};
    use layout_dom_api::LayoutDom;
    use register_theme::chrome::ChromeTheme;
    use serval_scripted_dom::{NodeId, ScriptedDom};
    use xilem_serval::PointerClick;

    use super::*;
    use crate::roster::{GraphletCard, LinkCard, LinkRelationRow, RosterSnapshot};

    fn node_row(n: u128, url: &str) -> RosterRow {
        RosterRow {
            member: GraphMemberId::from_u128(n),
            title: url.to_string(),
            url: url.to_string(),
            content_type: Some("text/html".to_string()),
            tags: Vec::new(),
            selected: false,
            open: false,
            section_header: None,
        }
    }

    fn link_row(kind: &str, family: EdgeFamily) -> LinkRow {
        LinkRow {
            from: GraphMemberId::from_u128(1),
            to: GraphMemberId::from_u128(2),
            source_title: "A".to_string(),
            source_url: "https://a.example".to_string(),
            target_title: "B".to_string(),
            target_url: "https://b.example".to_string(),
            direction_label: "->".to_string(),
            family,
            family_label: format!("{family:?}"),
            kind_label: kind.to_string(),
            source_label: None,
            selector: RelationSelector::Semantic(SemanticSubKind::Cites),
            selected: false,
            starts_bundle: true,
        }
    }

    fn with_rendered<R>(
        snapshot: RosterSnapshot,
        active: RosterTab,
        check: impl FnOnce(&ScriptedDom, NodeId) -> R,
    ) -> R {
        let mut pane = RosterPane::new();
        pane.set_snapshot(&ChromeTheme::default(), snapshot);
        pane.set_active_tab(active);
        let _ = pane.frame(280, 240, 0.0);
        let dom = pane.dom();
        let dom = dom.borrow();
        let root = dom.document();
        check(&dom, root)
    }

    fn first_by_class(dom: &ScriptedDom, id: NodeId, class: &str) -> Option<NodeId> {
        if dom.attributes(id).any(|attr| {
            attr.name.local.as_ref() == "class" && attr.value.split_whitespace().any(|c| c == class)
        }) {
            return Some(id);
        }
        dom.dom_children(id)
            .find_map(|child| first_by_class(dom, child, class))
    }

    fn count_by_class(dom: &ScriptedDom, id: NodeId, class: &str) -> usize {
        let here = usize::from(dom.attributes(id).any(|attr| {
            attr.name.local.as_ref() == "class" && attr.value.split_whitespace().any(|c| c == class)
        }));
        here + dom
            .dom_children(id)
            .map(|c| count_by_class(dom, c, class))
            .sum::<usize>()
    }

    #[test]
    fn roster_renders_all_four_tabs() {
        with_rendered(
            RosterSnapshot {
                node_rows: vec![node_row(1, "https://a.example")],
                ..Default::default()
            },
            RosterTab::Nodes,
            |dom, root| {
                assert_eq!(count_by_class(dom, root, "roster-tab"), 3);
                assert_eq!(count_by_class(dom, root, "roster-tab-active"), 1);
            },
        );
    }

    #[test]
    fn clicking_a_tab_queues_tab_intent() {
        let mut pane = RosterPane::new();
        pane.set_snapshot(
            &ChromeTheme::default(),
            RosterSnapshot {
                node_rows: vec![node_row(1, "https://a.example")],
                ..Default::default()
            },
        );
        let _ = pane.frame(280, 240, 0.0);
        let tab_node = {
            let dom = pane.dom();
            let dom = dom.borrow();
            first_by_class(&dom, dom.document(), "roster-tab").expect("an inactive tab")
        };
        pane.dispatch_click(tab_node, PointerClick::at((50.0, 10.0)));
        assert!(matches!(
            pane.take_intents().first(),
            Some(RosterIntent::SetTab(RosterTab::Links))
        ));
    }

    #[test]
    fn links_tab_lists_relation_families_distinctly() {
        with_rendered(
            RosterSnapshot {
                link_rows: vec![
                    link_row("Cites", EdgeFamily::Semantic),
                    link_row("Traversal", EdgeFamily::Traversal),
                    link_row("Copied", EdgeFamily::Provenance),
                ],
                ..Default::default()
            },
            RosterTab::Links,
            |dom, root| {
                assert_eq!(count_by_class(dom, root, "roster-link-kind"), 6);
                assert_eq!(count_by_class(dom, root, "roster-section"), 3);
            },
        );
    }

    #[test]
    fn clicking_link_row_opens_relation_cell_subject() {
        let mut pane = RosterPane::new();
        pane.set_snapshot(
            &ChromeTheme::default(),
            RosterSnapshot {
                link_rows: vec![link_row("Cites", EdgeFamily::Semantic)],
                ..Default::default()
            },
        );
        pane.set_active_tab(RosterTab::Links);
        let _ = pane.frame(280, 240, 0.0);
        let row_node = {
            let dom = pane.dom();
            let dom = dom.borrow();
            first_by_class(&dom, dom.document(), "roster-link-grid").expect("a link row")
        };
        pane.dispatch_click(row_node, PointerClick::at((40.0, 60.0)));
        assert!(matches!(
            pane.take_intents().first(),
            Some(RosterIntent::OpenDetail(RosterSubject::RelationCell { .. }))
        ));
    }

    #[test]
    fn link_card_groups_relations_by_family() {
        let card = LinkCard {
            from: GraphMemberId::from_u128(1),
            to: GraphMemberId::from_u128(2),
            source_title: "A".to_string(),
            source_url: "https://a.example".to_string(),
            target_title: "B".to_string(),
            target_url: "https://b.example".to_string(),
            relations: vec![
                LinkRelationRow {
                    from: GraphMemberId::from_u128(1),
                    to: GraphMemberId::from_u128(2),
                    family: EdgeFamily::Semantic,
                    family_label: "Semantic".to_string(),
                    kind_label: "Cites".to_string(),
                    label: None,
                    selector: RelationSelector::Semantic(SemanticSubKind::Cites),
                    editable: true,
                    selected: false,
                },
                LinkRelationRow {
                    from: GraphMemberId::from_u128(1),
                    to: GraphMemberId::from_u128(2),
                    family: EdgeFamily::Traversal,
                    family_label: "Traversal".to_string(),
                    kind_label: "Traversal".to_string(),
                    label: None,
                    selector: RelationSelector::Family(EdgeFamily::Traversal),
                    editable: false,
                    selected: false,
                },
            ],
        };
        with_rendered(
            RosterSnapshot {
                detail: Some(RosterDetail::Link(card)),
                ..Default::default()
            },
            RosterTab::Links,
            |dom, root| {
                assert_eq!(count_by_class(dom, root, "roster-card-group-title"), 2);
                assert!(first_by_class(dom, root, "roster-detail").is_some());
            },
        );
    }

    #[test]
    fn graphlet_card_shows_drift_preview_without_applying() {
        with_rendered(
            RosterSnapshot {
                detail: Some(RosterDetail::Graphlet(GraphletCard {
                    id: 7,
                    kind_label: "Component".to_string(),
                    binding_label: "Linked".to_string(),
                    members: vec!["A".to_string()],
                    selectors_label: "Semantic".to_string(),
                    drift_tracking: true,
                    added: vec!["B".to_string()],
                    removed: vec!["C".to_string()],
                })),
                ..Default::default()
            },
            RosterTab::Graphlets,
            |dom, root| {
                assert!(first_by_class(dom, root, "roster-detail").is_some());
                assert_eq!(count_by_class(dom, root, "roster-card-action"), 4);
            },
        );
    }
}
