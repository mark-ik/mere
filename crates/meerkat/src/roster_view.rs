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

use std::cell::RefCell;
use std::rc::Rc;

use forme::GraphMemberId;
use layout_dom_api::LayoutDom;
use netrender::Scene;
use register_theme::chrome::ChromeTheme;
use serval_layout::ScrollOffsets;
use serval_scripted_dom::{NodeId, ScriptedDom};
use xilem_serval::{AnyView, PointerClick, ServalAppRunner, ServalCtx, ServalElement, el, on_click};

use crate::pane_session::PaneSession;
use crate::roster::{roster_sheet, EdgeDir, RosterRow};

/// The erased view the roster logic produces (mirrors `ChromeView`).
pub type RosterView = Box<dyn AnyView<RosterState, (), ServalCtx, ServalElement>>;

/// Logic alias for the runner: roster state → roster view tree.
pub type RosterLogic = fn(&RosterState) -> RosterView;

/// What a roster row-click queues for the shell to apply. The shell drains these
/// after dispatch and selects the node (looking its URL up off the member, exactly
/// as the old pointer path did).
pub enum RosterIntent {
    Select(GraphMemberId),
}

/// The roster's view state: the rows to render plus the intents row handlers queue.
#[derive(Default)]
pub struct RosterState {
    pub rows: Vec<RosterRow>,
    pub pending: Vec<RosterIntent>,
}

/// Build the roster pane as a serval view tree — the declarative twin of
/// `build_roster_dom`, with each row carrying an `on_click` that queues a
/// `Select(member)` instead of relying on a row-rect hit-test.
pub fn roster_view(state: &RosterState) -> RosterView {
    if state.rows.is_empty() {
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
                edges.push(Box::new(
                    el::<_, RosterState, ()>("div", edge_kids).attr("class", "roster-edge"),
                ));
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

    Box::new(el::<_, RosterState, ()>("div", children).attr("class", "roster"))
}

/// A self-contained, view-driven roster pane: the runner over its own DOM, a
/// cached cascade+layout session, and the resolved stylesheet. The shell sets the
/// rows, asks for a frame, hit-tests, dispatches clicks, and drains the queued
/// selections — no rect cache, no per-frame hand-built DOM. (Pelt's `Chrome`
/// bundle, applied to a mere list pane.)
pub struct RosterPane {
    runner: ServalAppRunner<RosterState, RosterLogic, RosterView>,
    session: Option<PaneSession>,
    sheets: Vec<String>,
}

impl RosterPane {
    pub fn new() -> Self {
        let dom: Rc<RefCell<ScriptedDom>> = Rc::new(RefCell::new(ScriptedDom::new()));
        let runner = ServalAppRunner::new(dom, roster_view as RosterLogic, RosterState::default());
        Self {
            runner,
            session: None,
            sheets: Vec::new(),
        }
    }

    /// Refresh the rows (from `roster_rows()`) and the themed stylesheet. The runner
    /// diffs the new view into its DOM; the next `frame` lays the change out.
    pub fn set_rows(&mut self, theme: &ChromeTheme, rows: Vec<RosterRow>) {
        self.sheets = roster_sheet(theme);
        self.runner.update(|s| s.rows = rows);
    }

    /// Render the pane to a scene at `w`×`h`, scrolled `scroll` px down its `.roster`
    /// container, reusing the cached layout (rebuilt only on a structural / resize /
    /// theme change). The scroll node is resolved inside the pane.
    pub fn frame(&mut self, w: u32, h: u32, scroll: f32) -> Scene {
        let sheet: Vec<&str> = self.sheets.iter().map(String::as_str).collect();
        let dom = self.runner.dom();
        let scrolls = Self::scroll_offsets(&dom.borrow(), scroll);
        PaneSession::scene(&mut self.session, &dom, &sheet, w, h, None, &scrolls)
    }

    /// The maximum scroll (content height beyond the visible pane) of the last laid-
    /// out frame, for the host to clamp its stored scroll. `0` before the first frame.
    pub fn max_scroll(&self) -> f32 {
        let Some(session) = self.session.as_ref() else { return 0.0 };
        let dom = self.runner.dom();
        let dom = dom.borrow();
        let Some(node) = crate::first_with_class(&dom, dom.document(), "roster") else {
            return 0.0;
        };
        let Some(l) = session.fragments().rect_of(node) else { return 0.0 };
        let inner = l.size.height - l.padding.top - l.padding.bottom - l.border.top - l.border.bottom;
        (l.content_size.height - inner).max(0.0)
    }

    /// Hit-test pane-local `(x, y)` (with the pane scrolled by `scroll`) against the
    /// cached layout, returning the DOM node, or `None` if not laid out / nothing hit.
    pub fn hit_test(&self, x: f32, y: f32, scroll: f32) -> Option<NodeId> {
        let session = self.session.as_ref()?;
        let dom = self.runner.dom();
        let dom = dom.borrow();
        let scrolls = Self::scroll_offsets(&dom, scroll);
        session.hit_test(&dom, x, y, &scrolls)
    }

    /// Window-space bounds of each row, keyed by member, for the a11y projection —
    /// the rect cache's replacement, read straight off the cached layout. `origin` is
    /// the pane's window rect `[x0,y0,x1,y1]`; rows offscreen are dropped, partials
    /// clipped to the pane.
    pub fn row_bounds(&self, origin: [f32; 4], scroll: f32) -> Vec<(GraphMemberId, [f32; 4])> {
        let Some(session) = self.session.as_ref() else { return Vec::new() };
        let frags = session.fragments();
        let dom = self.runner.dom();
        let dom = dom.borrow();
        let root = dom.document();
        let mut nodes = crate::all_with_class(&dom, root, "roster-row");
        nodes.extend(crate::all_with_class(&dom, root, "roster-row-selected"));
        let mut out = Vec::new();
        for node in nodes {
            if let (Some(member), Some(l)) = (crate::member_attr(&dom, node), frags.rect_of(node)) {
                let x0 = origin[0] + l.location.x;
                let y0 = origin[1] + l.location.y - scroll;
                let x1 = x0 + l.size.width;
                let y1 = y0 + l.size.height;
                if x1 > origin[0] && x0 < origin[2] && y1 > origin[1] && y0 < origin[3] {
                    out.push((
                        member,
                        [x0.max(origin[0]), y0.max(origin[1]), x1.min(origin[2]), y1.min(origin[3])],
                    ));
                }
            }
        }
        out
    }

    /// Dispatch a click that hit `node`, firing its `on_click` (which queues a
    /// `Select`); the shell then [`take_intents`](Self::take_intents).
    pub fn dispatch_click(&mut self, node: NodeId, event: PointerClick) {
        self.runner.dispatch_click(node, event);
    }

    /// Drain the selections queued by row handlers since the last call.
    pub fn take_intents(&mut self) -> Vec<RosterIntent> {
        let mut out = Vec::new();
        self.runner.update(|s| out = std::mem::take(&mut s.pending));
        out
    }

    /// Scroll offsets for the `.roster` container at `scroll` px (empty if absent).
    fn scroll_offsets(dom: &ScriptedDom, scroll: f32) -> ScrollOffsets<NodeId> {
        let mut offsets = ScrollOffsets::default();
        if let Some(node) = crate::first_with_class(dom, dom.document(), "roster") {
            offsets.insert(node, (0.0, scroll));
        }
        offsets
    }
}

impl Default for RosterPane {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use forme::GraphMemberId;
    use register_theme::chrome::ChromeTheme;
    use xilem_serval::PointerClick;

    use super::*;
    use crate::roster::RosterRow;

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
}
