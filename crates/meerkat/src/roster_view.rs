/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The roster as a **view-driven pane**: a `roster_view` over `RosterState`,
//! driven by a [`ServalAppRunner`] inside a self-contained [`RosterPane`] bundle
//! (the pelt `pelt-desktop::chrome::Chrome` pattern). The bundle owns the runner,
//! its cached cascade+layout [`PaneSession`], and the pane stylesheet, and exposes
//! `frame` (render to a scene), `hit_test`, `dispatch_click`, and `take_intents`.
//!
//! This replaces the per-frame `build_roster_dom` rebuild + the hand-maintained
//! `roster_row_rects` hit-test cache: a row carries an `on_click` that queues a
//! `Select`, so the runner dispatches input through the DOM and there is no rect
//! cache to drift. (Window composition P2 companion — list-pane view-ification.)

use forme::GraphMemberId;
use layout_dom_api::LayoutDom;
use netrender::Scene;
use register_theme::chrome::ChromeTheme;
use serval_layout::ScrollOffsets;
use serval_scripted_dom::NodeId;
use xilem_serval::{AnyView, PointerClick, ServalCtx, ServalElement, el, on_click};

use kernel::graph::FieldId;

use crate::roster::{roster_sheet, EdgeDir, FieldRow, RosterRow};
// `view_pane` is test-gated (its `ViewPane` base backs only the `RosterPane` test
// harness now), so this import rides the same gate. (Phase 1, step 2.)
#[cfg(test)]
use crate::view_pane::ViewPane;

/// The erased view the roster logic produces (mirrors `ChromeView`).
pub type RosterView = Box<dyn AnyView<RosterState, (), ServalCtx, ServalElement>>;

/// Logic alias for the runner: roster state → roster view tree. (Used by the test-only
/// `RosterPane` harness now that the roster is folded into the shell runner.)
#[cfg(test)]
pub type RosterLogic = fn(&RosterState) -> RosterView;

/// What a roster row-click queues for the shell to apply. The shell drains these
/// after dispatch and selects the node (looking its URL up off the member, exactly
/// as the old pointer path did).
pub enum RosterIntent {
    Select(GraphMemberId),
    /// Center the canvas on a field region (a field row's click). (Field regions.)
    SelectField(FieldId),
    /// Hide / show a field region on the canvas (a field row's toggle). (Field regions.)
    ToggleFieldVisibility(FieldId),
    /// Adjust a field's coupling strength by the given delta (a field row's − / +).
    /// (Field regions — strength tuning.)
    AdjustFieldStrength(FieldId, f32),
}

/// The roster's view state: the node rows + field rows to render, plus the intents
/// row handlers queue.
#[derive(Default)]
pub struct RosterState {
    pub rows: Vec<RosterRow>,
    /// Field-region rows, rendered in a "Fields" section after the node rows.
    pub field_rows: Vec<FieldRow>,
    pub pending: Vec<RosterIntent>,
}

/// Build the roster pane as a serval view tree — the declarative twin of
/// `build_roster_dom`, with each row carrying an `on_click` that queues a
/// `Select(member)` instead of relying on a row-rect hit-test.
pub fn roster_view(state: &RosterState) -> RosterView {
    if state.rows.is_empty() && state.field_rows.is_empty() {
        let empty: RosterView =
            Box::new(el::<_, RosterState, ()>("div", "No nodes yet").attr("class", "roster-empty"));
        return Box::new(el::<_, RosterState, ()>("div", vec![empty]).attr("class", "roster"));
    }

    let mut children: Vec<RosterView> = Vec::new();
    for row in &state.rows {
        if let Some(header) = &row.section_header {
            children.push(Box::new(
                el::<_, RosterState, ()>("div", header.clone()).attr("class", "roster-section"),
            ));
        }

        let mut entry: Vec<RosterView> = vec![
            Box::new(el::<_, RosterState, ()>("div", row.title.clone()).attr("class", "roster-title")),
            Box::new(el::<_, RosterState, ()>("div", row.url.clone()).attr("class", "roster-sub")),
        ];

        if row.content_type.is_some() || !row.tags.is_empty() {
            let mut facets: Vec<RosterView> = Vec::new();
            if let Some(ct) = &row.content_type {
                facets.push(Box::new(
                    el::<_, RosterState, ()>("span", ct.clone()).attr("class", "roster-chip"),
                ));
            }
            for tag in &row.tags {
                facets.push(Box::new(
                    el::<_, RosterState, ()>("span", tag.clone()).attr("class", "roster-tag"),
                ));
            }
            entry.push(Box::new(
                el::<_, RosterState, ()>("div", facets).attr("class", "roster-facets"),
            ));
        }

        if !row.edges.is_empty() {
            let mut edges: Vec<RosterView> = Vec::new();
            for edge in &row.edges {
                let arrow = match edge.direction {
                    EdgeDir::Out => "\u{2192}", // →
                    EdgeDir::In => "\u{2190}",  // ←
                };
                let edge_kids: Vec<RosterView> = vec![
                    Box::new(el::<_, RosterState, ()>("span", arrow).attr("class", "roster-edge-dir")),
                    Box::new(
                        el::<_, RosterState, ()>("span", edge.kind_label.clone())
                            .attr("class", "roster-edge-kind"),
                    ),
                    Box::new(
                        el::<_, RosterState, ()>("span", edge.other_title.clone())
                            .attr("class", "roster-edge-target"),
                    ),
                ];
                // Edge traversal: clicking a relation selects its *other* endpoint,
                // walking the graph along the edge. `stop_propagation` keeps the
                // parent row's own select from also firing.
                let target = edge.other_member;
                edges.push(Box::new(on_click(
                    el::<_, RosterState, ()>("div", edge_kids)
                        .attr("class", "roster-edge")
                        .attr("data-edge-target", target.to_string()),
                    move |st: &mut RosterState, ev: PointerClick| {
                        ev.stop_propagation();
                        st.pending.push(RosterIntent::Select(target));
                    },
                )));
            }
            entry.push(Box::new(
                el::<_, RosterState, ()>("div", edges).attr("class", "roster-edges"),
            ));
        }

        let class = if row.selected { "roster-row-selected" } else { "roster-row" };
        let member = row.member;
        children.push(Box::new(on_click(
            el::<_, RosterState, ()>("div", entry)
                .attr("class", class)
                // The a11y projection reads row bounds off the laid-out DOM by member.
                .attr("data-member", member.to_string()),
            move |st: &mut RosterState, _: PointerClick| st.pending.push(RosterIntent::Select(member)),
        )));
    }

    // The "Fields" section after the nodes: one row per field region — its name and
    // a hide/show toggle. The row click centers the field; the toggle (which stops
    // propagation so it doesn't also center) hides/shows it. (Field regions.)
    if !state.field_rows.is_empty() {
        children.push(Box::new(
            el::<_, RosterState, ()>("div", "Fields").attr("class", "roster-section"),
        ));
        for fr in &state.field_rows {
            let id = fr.id;
            let toggle_label = if fr.hidden { "show" } else { "hide" };
            let toggle = on_click(
                el::<_, RosterState, ()>("span", toggle_label).attr("class", "roster-field-toggle"),
                move |st: &mut RosterState, ev: PointerClick| {
                    ev.stop_propagation();
                    st.pending.push(RosterIntent::ToggleFieldVisibility(id));
                },
            );
            // − / value / + tune the field's coupling strength (the value is the raw
            // strength / 1000, a compact level). Each step stops propagation so it
            // doesn't also center the field. (Field regions — strength tuning.)
            let weaker = on_click(
                el::<_, RosterState, ()>("span", "\u{2212}").attr("class", "roster-field-step"),
                move |st: &mut RosterState, ev: PointerClick| {
                    ev.stop_propagation();
                    st.pending.push(RosterIntent::AdjustFieldStrength(id, -1000.0));
                },
            );
            let stronger = on_click(
                el::<_, RosterState, ()>("span", "+").attr("class", "roster-field-step"),
                move |st: &mut RosterState, ev: PointerClick| {
                    ev.stop_propagation();
                    st.pending.push(RosterIntent::AdjustFieldStrength(id, 1000.0));
                },
            );
            let entry: Vec<RosterView> = vec![
                Box::new(
                    el::<_, RosterState, ()>("span", fr.name.clone()).attr("class", "roster-field-name"),
                ),
                Box::new(weaker),
                Box::new(
                    el::<_, RosterState, ()>("span", format!("{:.0}", fr.strength / 1000.0))
                        .attr("class", "roster-field-strength"),
                ),
                Box::new(stronger),
                Box::new(toggle),
            ];
            let class = if fr.hidden { "roster-field roster-field-hidden" } else { "roster-field" };
            children.push(Box::new(on_click(
                el::<_, RosterState, ()>("div", entry).attr("class", class),
                move |st: &mut RosterState, _: PointerClick| {
                    st.pending.push(RosterIntent::SelectField(id));
                },
            )));
        }
    }

    Box::new(el::<_, RosterState, ()>("div", children).attr("class", "roster"))
}

/// The roster as a view-driven pane: a [`ViewPane`] over `RosterState`, plus the
/// roster-specific scroll (its `.roster` container), row-member a11y bounds, and
/// selection draining. The shell sets the rows, frames, hit-tests, dispatches
/// clicks, and drains the queued selections — no rect cache, no per-frame DOM.
///
/// The roster is folded into the shell runner now (the host drives `roster_view` through a
/// lens on `ShellState`); this bundle is retained only as the tests' render + dispatch
/// harness. (Unified document host Phase 1.)
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

    /// Refresh the rows (from `roster_rows()`) and the themed stylesheet. The runner
    /// diffs the new view into its DOM; the next `frame` lays the change out.
    pub fn set_rows(
        &mut self,
        theme: &ChromeTheme,
        rows: Vec<RosterRow>,
        field_rows: Vec<FieldRow>,
    ) {
        self.pane.set_sheets(roster_sheet(theme));
        self.pane.update(|s| {
            s.rows = rows;
            s.field_rows = field_rows;
        });
    }

    /// Render the pane to a scene at `w`×`h`, scrolled `scroll` px down its `.roster`
    /// container.
    pub fn frame(&mut self, w: u32, h: u32, scroll: f32) -> Scene {
        let scrolls = self.scroll_offsets(scroll);
        self.pane.frame(w, h, &scrolls)
    }

    /// The maximum scroll (content height beyond the visible pane) of the last laid-
    /// out frame, for the host to clamp its stored scroll. `0` before the first frame.
    pub fn max_scroll(&self) -> f32 {
        let dom = self.pane.dom();
        let dom = dom.borrow();
        let Some(node) = crate::first_with_class(&dom, dom.document(), "roster") else {
            return 0.0;
        };
        let Some(frags) = self.pane.fragments() else { return 0.0 };
        let Some(l) = frags.rect_of(node) else { return 0.0 };
        let inner = l.size.height - l.padding.top - l.padding.bottom - l.border.top - l.border.bottom;
        (l.content_size.height - inner).max(0.0)
    }

    /// Hit-test pane-local `(x, y)` (with the pane scrolled by `scroll`).
    pub fn hit_test(&self, x: f32, y: f32, scroll: f32) -> Option<NodeId> {
        let scrolls = self.scroll_offsets(scroll);
        self.pane.hit_test(x, y, &scrolls)
    }

    /// Dispatch a click that hit `node`, firing its `on_click` (which queues a
    /// `Select`); the shell then [`take_intents`](Self::take_intents).
    pub fn dispatch_click(&mut self, node: NodeId, event: PointerClick) {
        self.pane.dispatch_click(node, event);
    }

    /// Drain the selections queued by row handlers since the last call.
    pub fn take_intents(&mut self) -> Vec<RosterIntent> {
        let mut out = Vec::new();
        self.pane.update(|s| out = std::mem::take(&mut s.pending));
        out
    }

    /// The realized pane DOM (the runner has diffed the current rows into it), for
    /// tests asserting the emitted element structure.
    #[cfg(test)]
    pub(crate) fn dom(&self) -> std::rc::Rc<std::cell::RefCell<serval_scripted_dom::ScriptedDom>> {
        self.pane.dom()
    }

    /// Scroll offsets for the `.roster` container at `scroll` px (empty if absent).
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
    use layout_dom_api::LayoutDom;
    use register_theme::chrome::ChromeTheme;
    use serval_scripted_dom::{NodeId, ScriptedDom};
    use xilem_serval::PointerClick;

    use super::*;
    use crate::roster::{EdgeDir, EdgeRow, RosterRow};

    fn row(n: u128, url: &str) -> RosterRow {
        RosterRow {
            member: GraphMemberId::from_u128(n),
            title: url.to_string(),
            url: url.to_string(),
            content_type: Some("text/html".to_string()),
            tags: Vec::new(),
            edges: Vec::new(),
            selected: false,
            section_header: None,
        }
    }

    /// Build a roster pane over `rows`, lay it out (so the runner diffs them into the
    /// DOM), and hand the realized DOM + its root to `check`. The structural twin of
    /// the old `build_roster_dom` tests, now asserting against the view's output.
    fn with_rendered<R>(rows: Vec<RosterRow>, check: impl FnOnce(&ScriptedDom, NodeId) -> R) -> R {
        let mut pane = RosterPane::new();
        pane.set_rows(&ChromeTheme::default(), rows, Vec::new());
        let _ = pane.frame(240, 200, 0.0);
        let dom = pane.dom();
        let dom = dom.borrow();
        let root = dom.document();
        check(&dom, root)
    }

    /// The first node at/under `id` carrying `class`, by depth-first walk.
    fn first_by_class(dom: &ScriptedDom, id: NodeId, class: &str) -> Option<NodeId> {
        if dom.attributes(id).any(|attr| {
            attr.name.local.as_ref() == "class" && attr.value.split_whitespace().any(|c| c == class)
        }) {
            return Some(id);
        }
        dom.dom_children(id).find_map(|child| first_by_class(dom, child, class))
    }

    /// How many nodes at/under `id` carry `class`.
    fn count_by_class(dom: &ScriptedDom, id: NodeId, class: &str) -> usize {
        let here = usize::from(dom.attributes(id).any(|attr| {
            attr.name.local.as_ref() == "class"
                && attr.value.split_whitespace().any(|c| c == class)
        }));
        here + dom.dom_children(id).map(|c| count_by_class(dom, c, class)).sum::<usize>()
    }

    /// The input-spine proof: a row-click routes through the runner's DOM dispatch
    /// (hit-test → dispatch → the row's `on_click`) and queues that row's member —
    /// no rect cache. Replaces the old `roster_row_at` rect lookup.
    #[test]
    fn clicking_a_row_queues_its_member_select() {
        let mut pane = RosterPane::new();
        let first = GraphMemberId::from_u128(1);
        pane.set_rows(
            &ChromeTheme::default(),
            vec![row(1, "https://a.example"), row(2, "https://b.example")],
            Vec::new(),
        );
        // Lay the pane out so the hit-test has a cached layout to probe.
        let _ = pane.frame(300, 400, 0.0);
        // A point inside the first row (past the container + row padding, top of list).
        let node = pane.hit_test(40.0, 24.0, 0.0).expect("a node under the first row");
        pane.dispatch_click(node, PointerClick::at((40.0, 24.0)));
        let intents = pane.take_intents();
        assert_eq!(intents.len(), 1, "exactly one selection queued");
        assert!(
            matches!(intents.first(), Some(RosterIntent::Select(m)) if *m == first),
            "the first row's member is selected via runner dispatch, not a rect cache",
        );
    }

    /// A row's facets strip carries its content-type chip and one tag per tag.
    #[test]
    fn facets_render_content_type_and_tags() {
        let rows = vec![RosterRow {
            member: GraphMemberId::from_u128(1),
            title: "My Page".to_string(),
            url: "https://example.com/".to_string(),
            content_type: Some("text/html".to_string()),
            tags: vec!["gemini".to_string(), "research".to_string()],
            edges: Vec::new(),
            selected: false,
            section_header: None,
        }];
        with_rendered(rows, |dom, root| {
            assert!(first_by_class(dom, root, "roster-facets").is_some());
            assert!(first_by_class(dom, root, "roster-chip").is_some());
            assert_eq!(count_by_class(dom, root, "roster-tag"), 2);
        });
    }

    /// A row with no content type and no tags emits no facets strip.
    #[test]
    fn no_facets_strip_when_empty() {
        let rows = vec![RosterRow {
            member: GraphMemberId::from_u128(1),
            title: "Bare".to_string(),
            url: "mere://welcome".to_string(),
            content_type: None,
            tags: Vec::new(),
            edges: Vec::new(),
            selected: false,
            section_header: None,
        }];
        with_rendered(rows, |dom, root| {
            assert!(first_by_class(dom, root, "roster-facets").is_none());
        });
    }

    /// The focused row renders its edge detail: one `roster-edge` (with a kind
    /// label) per relation.
    #[test]
    fn edge_rows_render_for_focused_node() {
        let rows = vec![
            RosterRow {
                member: GraphMemberId::from_u128(1),
                title: "Focused".to_string(),
                url: "https://a.example/".to_string(),
                content_type: None,
                tags: Vec::new(),
                edges: vec![
                    EdgeRow {
                        direction: EdgeDir::Out,
                        kind_label: "Hyperlink".to_string(),
                        other_title: "Target".to_string(),
                        other_url: "https://b.example/".to_string(),
                        other_member: GraphMemberId::from_u128(2),
                    },
                    EdgeRow {
                        direction: EdgeDir::In,
                        kind_label: "Traversal".to_string(),
                        other_title: "Source".to_string(),
                        other_url: "https://c.example/".to_string(),
                        other_member: GraphMemberId::from_u128(3),
                    },
                ],
                selected: true,
                section_header: None,
            },
            RosterRow {
                member: GraphMemberId::from_u128(2),
                title: "Other".to_string(),
                url: "https://b.example/".to_string(),
                content_type: None,
                tags: Vec::new(),
                edges: Vec::new(),
                selected: false,
                section_header: None,
            },
        ];
        with_rendered(rows, |dom, root| {
            assert!(first_by_class(dom, root, "roster-edges").is_some());
            assert_eq!(count_by_class(dom, root, "roster-edge"), 2);
            assert_eq!(count_by_class(dom, root, "roster-edge-kind"), 2);
        });
    }

    /// Edge traversal: clicking a relation row selects its *other* endpoint (not
    /// the focused row), so the roster walks the graph along the edge.
    #[test]
    fn clicking_an_edge_selects_the_other_endpoint() {
        let mut pane = RosterPane::new();
        pane.set_rows(
            &ChromeTheme::default(),
            vec![RosterRow {
                member: GraphMemberId::from_u128(1),
                title: "Focused".to_string(),
                url: "https://a.example/".to_string(),
                content_type: None,
                tags: Vec::new(),
                edges: vec![EdgeRow {
                    direction: EdgeDir::Out,
                    kind_label: "Hyperlink".to_string(),
                    other_title: "Target".to_string(),
                    other_url: "https://b.example/".to_string(),
                    other_member: GraphMemberId::from_u128(2),
                }],
                selected: true,
                section_header: None,
            }],
            Vec::new(),
        );
        let _ = pane.frame(300, 400, 0.0);
        let edge_node = {
            let dom = pane.dom();
            let dom = dom.borrow();
            first_by_class(&dom, dom.document(), "roster-edge").expect("an edge row")
        };
        pane.dispatch_click(edge_node, PointerClick::at((40.0, 60.0)));
        let intents = pane.take_intents();
        assert!(
            matches!(intents.first(), Some(RosterIntent::Select(m)) if *m == GraphMemberId::from_u128(2)),
            "clicking the edge selects its other endpoint, not the focused row",
        );
        assert_eq!(intents.len(), 1, "stop_propagation kept the row's own select from firing");
    }

    /// Each row that opens a content-type section emits a section header.
    #[test]
    fn section_headers_render_for_grouped_rows() {
        let rows = vec![
            RosterRow {
                member: GraphMemberId::from_u128(1),
                title: "A Feed".to_string(),
                url: "https://example.com/feed".to_string(),
                content_type: Some("application/rss+xml".to_string()),
                tags: Vec::new(),
                edges: Vec::new(),
                selected: false,
                section_header: Some("Feeds".to_string()),
            },
            RosterRow {
                member: GraphMemberId::from_u128(2),
                title: "A Page".to_string(),
                url: "https://example.com/page".to_string(),
                content_type: Some("text/html".to_string()),
                tags: Vec::new(),
                edges: Vec::new(),
                selected: false,
                section_header: Some("Documents".to_string()),
            },
        ];
        with_rendered(rows, |dom, root| {
            assert_eq!(count_by_class(dom, root, "roster-section"), 2);
        });
    }

    /// Enough rows overflow a short pane, so the scroll-clamp path reports headroom.
    #[test]
    fn rows_overflow_small_pane() {
        let rows: Vec<RosterRow> = (0..12)
            .map(|i| RosterRow {
                member: GraphMemberId::from_u128(i + 1),
                title: format!("node-{i}"),
                url: format!("https://example.com/node-{i}"),
                content_type: Some("text/html".to_string()),
                tags: Vec::new(),
                edges: Vec::new(),
                selected: i == 0,
                section_header: None,
            })
            .collect();
        let mut pane = RosterPane::new();
        pane.set_rows(&ChromeTheme::default(), rows, Vec::new());
        let _ = pane.frame(220, 120, 0.0);
        assert!(
            pane.max_scroll() > 0.0,
            "roster rows should overflow the visible pane",
        );
    }
}
