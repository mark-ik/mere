/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! A generic item-model list pane over [`ViewPane`], for the simple display +
//! button panes (apparatus, steward, inspector, utility). Each is a flat list of
//! [`PaneItem`]s — a classed text div, or a clickable button that queues an
//! activation key — so they share one view function and one bundle. The shell
//! builds the items (reusing each pane's existing formatting), frames, hit-tests,
//! dispatches, and drains the activations. No per-frame hand-built DOM, no rect
//! cache. (Roster stays bespoke — its rows carry richer structure.)
//!
//! (Window composition P2 companion — list-pane view-ification.)

use netrender::Scene;
use serval_layout::ScrollOffsets;
use serval_scripted_dom::NodeId;
use xilem_serval::{el, on_click, AnyView, PointerClick, ServalCtx, ServalElement};

use crate::view_pane::ViewPane;

/// One row of a list pane: a div with `class` and `text`. When `key` is `Some`,
/// the div is clickable and a click queues that key as an activation (a theme id,
/// a row id, …) for the shell to act on; `None` is a plain display row / title.
pub struct PaneItem {
    pub class: String,
    pub text: String,
    pub key: Option<String>,
}

impl PaneItem {
    /// A non-interactive classed text row (a title or a display row).
    pub fn text(class: impl Into<String>, text: impl Into<String>) -> Self {
        Self { class: class.into(), text: text.into(), key: None }
    }

    /// A clickable button row whose click queues `key`.
    pub fn button(class: impl Into<String>, text: impl Into<String>, key: impl Into<String>) -> Self {
        Self { class: class.into(), text: text.into(), key: Some(key.into()) }
    }
}

/// The view state: the pane's root container class, its items, and the activation
/// keys button handlers have queued since the last drain.
#[derive(Default)]
pub struct ListPaneState {
    pub root_class: String,
    pub items: Vec<PaneItem>,
    pub pending: Vec<String>,
}

/// The erased view a list pane produces.
pub type ListView = Box<dyn AnyView<ListPaneState, (), ServalCtx, ServalElement>>;

/// Logic alias for the runner.
pub type ListLogic = fn(&ListPaneState) -> ListView;

/// Render a list pane: the root container with one classed div per item, a
/// button item carrying an `on_click` that queues its key.
pub fn list_pane_view(state: &ListPaneState) -> ListView {
    let children: Vec<ListView> = state
        .items
        .iter()
        .map(|item| {
            let div = el::<_, ListPaneState, ()>("div", item.text.clone()).attr("class", item.class.clone());
            match &item.key {
                Some(key) => {
                    let key = key.clone();
                    Box::new(on_click(div, move |s: &mut ListPaneState, _: PointerClick| {
                        s.pending.push(key.clone())
                    })) as ListView
                }
                None => Box::new(div) as ListView,
            }
        })
        .collect();
    Box::new(el::<_, ListPaneState, ()>("div", children).attr("class", state.root_class.clone()))
}

/// A view-driven display/button pane: a [`ViewPane`] over [`ListPaneState`]. The
/// panes that use it do not scroll and carry no a11y row bounds (their a11y is a
/// skeleton), so the surface is just set / frame / hit_test / dispatch / drain.
pub struct ListPane {
    pane: ViewPane<ListPaneState, ListLogic, ListView>,
}

impl ListPane {
    pub fn new() -> Self {
        Self {
            pane: ViewPane::new(list_pane_view as ListLogic, ListPaneState::default()),
        }
    }

    /// Replace the sheet, root class, and items for this frame.
    pub fn set(&mut self, sheets: Vec<String>, root_class: &str, items: Vec<PaneItem>) {
        self.pane.set_sheets(sheets);
        let root_class = root_class.to_string();
        self.pane.update(|s| {
            s.root_class = root_class;
            s.items = items;
        });
    }

    /// Render the pane to a scene at `w`×`h` (these panes do not scroll).
    pub fn frame(&mut self, w: u32, h: u32) -> Scene {
        self.pane.frame(w, h, &ScrollOffsets::default())
    }

    /// Hit-test pane-local `(x, y)`.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<NodeId> {
        self.pane.hit_test(x, y, &ScrollOffsets::default())
    }

    /// Dispatch a click that hit `node`, firing its `on_click` (a button queues its
    /// key); the shell then [`take_activations`](Self::take_activations).
    pub fn dispatch_click(&mut self, node: NodeId, event: PointerClick) {
        self.pane.dispatch_click(node, event);
    }

    /// Drain the activation keys queued by button handlers since the last call.
    pub fn take_activations(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        self.pane.update(|s| out = std::mem::take(&mut s.pending));
        out
    }
}

impl Default for ListPane {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use xilem_serval::PointerClick;

    use super::*;

    /// The input-spine proof for the generic list pane: a click on a button item
    /// routes through the runner's DOM dispatch (hit-test → dispatch → the button's
    /// `on_click`) and queues that button's activation key — no rect cache. This is
    /// the mechanism the apparatus theme buttons (and any future button pane) ride.
    #[test]
    fn clicking_a_button_item_queues_its_key() {
        let mut pane = ListPane::new();
        let sheet = vec![
            "div { display: block; }".to_string(),
            ".panel { padding: 4px; }".to_string(),
            ".title { font-size: 13px; padding: 6px; }".to_string(),
            ".btn { font-size: 15px; padding: 10px; }".to_string(),
        ];
        pane.set(
            sheet,
            "panel",
            vec![
                PaneItem::text("title", "Theme"),
                PaneItem::button("btn", "Dark", "theme.dark"),
            ],
        );
        // Lay the pane out so the hit-test has a cached layout to probe.
        let _ = pane.frame(240, 160);
        // A point inside the button (below the inert title row, within its padding).
        let node = pane.hit_test(30.0, 40.0).expect("a node under the button");
        pane.dispatch_click(node, PointerClick::at((30.0, 40.0)));
        let keys = pane.take_activations();
        assert_eq!(
            keys,
            vec!["theme.dark".to_string()],
            "the button's key is queued via runner dispatch, not a rect cache",
        );
    }
}
