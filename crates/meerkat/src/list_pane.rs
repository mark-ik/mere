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

use std::collections::HashMap;

use xilem_serval::{AnyView, Keyed, PointerClick, ServalCtx, ServalElement, clickable, el};

// The `ListPane` bundle is a #[cfg(test)] harness now that the four list panes fold into
// the shell document; its DOM / layout imports come along under the gate. (Phase 1, step 2.)
#[cfg(test)]
use crate::view_pane::ViewPane;
#[cfg(test)]
use layout_dom_api::LayoutDom;
#[cfg(test)]
use netrender::Scene;
#[cfg(test)]
use serval_layout::ScrollOffsets;
#[cfg(test)]
use serval_scripted_dom::NodeId;

/// A segmented slider control: a horizontal strip of `count` clickable cells.
/// Clicking cell `i` queues `"<key_prefix>:<i>:<count>"`, which the host maps to
/// the fraction `i/count`. `value` (0..1) marks the current position; a
/// `hue_track` strip colours each cell by its hue (a rainbow picker), else the
/// cells fill up to `value`. (Settings lane theme editor.)
pub struct SliderSpec {
    pub key_prefix: String,
    pub value: f32,
    pub count: usize,
    pub hue_track: bool,
}

/// A reorder control: a row's ▲ / ▼ buttons, each queuing a key that moves the row up or
/// down in its list, plus a drag grip for direct drag-reorder. Rendered inline beside the
/// row label (the label itself keeps its own `key`, e.g. remove-from-menu). Settings panes
/// render the buttons + grip; other list panes ignore them and show the label as a plain row.
/// (Command registry P4 — menu reorder; B2 — drag reorder.)
pub struct ReorderSpec {
    pub up_key: String,
    pub down_key: String,
    /// The row's stable id (a command id), emitted as the row's `data-reorder-id` so the
    /// host's pointer-driven drag-reorder gesture can grab it and resolve a drop. (B2.)
    pub id: String,
    /// Set while this is the row being dragged, so the view dims it. (B2.)
    pub dragging: bool,
    /// Set while a drag in flight would drop onto this row (it is under the cursor), so the
    /// view draws a drop marker above it. (B2.)
    pub drop_target: bool,
}

/// The ARIA semantics a selectable / toggleable [`PaneItem`] carries, so the
/// accessibility tree announces it as a control with a checked state (the row is
/// otherwise a styled `div` a screen reader reads as a neutral container). The
/// `bool` is the checked / selected state. The render paths emit `role` +
/// `aria-checked`, which serval-render's a11y bridge maps to the accesskit role +
/// toggled state.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaneAria {
    /// A member of a single-selection group (`role="radio"`); `true` = selected.
    Radio(bool),
    /// An independent on / off switch (`role="switch"`); `true` = on.
    Switch(bool),
}

impl PaneAria {
    /// The `(role, aria-checked)` attribute pair this control stamps.
    pub fn role_and_checked(self) -> (&'static str, &'static str) {
        let checked = |on: bool| if on { "true" } else { "false" };
        match self {
            PaneAria::Radio(on) => ("radio", checked(on)),
            PaneAria::Switch(on) => ("switch", checked(on)),
        }
    }
}

/// One row of a list pane: a div with `class` and `text`. When `key` is `Some`,
/// the div is clickable and a click queues that key as an activation (a theme id,
/// a row id, …) for the shell to act on; `None` is a plain display row / title.
/// When `slider` is `Some`, the row is a [`SliderSpec`] segmented track (the
/// `text` becomes its label); settings panes render it, other list panes show
/// the label. When `reorder` is `Some`, the row gains inline ▲ / ▼ move buttons.
/// When `aria` is `Some`, the row carries radio / switch a11y semantics.
pub struct PaneItem {
    pub class: String,
    pub text: String,
    pub key: Option<String>,
    pub slider: Option<SliderSpec>,
    pub reorder: Option<ReorderSpec>,
    pub aria: Option<PaneAria>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct PaneItemIdentity {
    class: String,
    text: String,
    key: Option<String>,
    slider_key_prefix: Option<String>,
    reorder_id: Option<String>,
    aria: Option<PaneAria>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct PaneItemKey {
    identity: PaneItemIdentity,
    occurrence: usize,
}

impl PaneItem {
    /// A non-interactive classed text row (a title or a display row).
    pub fn text(class: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            class: class.into(),
            text: text.into(),
            key: None,
            slider: None,
            reorder: None,
            aria: None,
        }
    }

    /// A clickable button row whose click queues `key`.
    pub fn button(
        class: impl Into<String>,
        text: impl Into<String>,
        key: impl Into<String>,
    ) -> Self {
        Self {
            class: class.into(),
            text: text.into(),
            key: Some(key.into()),
            slider: None,
            reorder: None,
            aria: None,
        }
    }

    /// A single-selection (radio-group) option row: a [`button`](Self::button)
    /// carrying `role="radio"` + `aria-checked` for the a11y tree, with the
    /// active / inactive `app-btn` class, both driven by `selected`. The host
    /// drains `key` on click, exactly as for a plain button.
    pub fn radio(selected: bool, text: impl Into<String>, key: impl Into<String>) -> Self {
        let class = if selected {
            "app-btn-active"
        } else {
            "app-btn"
        };
        Self {
            class: class.to_string(),
            text: text.into(),
            key: Some(key.into()),
            slider: None,
            reorder: None,
            aria: Some(PaneAria::Radio(selected)),
        }
    }

    /// An independent on / off switch row: like [`radio`](Self::radio) but
    /// `role="switch"`, for a boolean toggle (not a single-selection group).
    pub fn switch(on: bool, text: impl Into<String>, key: impl Into<String>) -> Self {
        let class = if on { "app-btn-active" } else { "app-btn" };
        Self {
            class: class.to_string(),
            text: text.into(),
            key: Some(key.into()),
            slider: None,
            reorder: None,
            aria: Some(PaneAria::Switch(on)),
        }
    }

    /// A reorderable button row: the label click queues `key` (e.g. remove), inline ▲ / ▼
    /// buttons queue `up_key` / `down_key` to move the row within its list, and `id` tags the
    /// row for the host's drag-reorder gesture. (Menu reorder; B2 — drag reorder.)
    pub fn reorder_row(
        class: impl Into<String>,
        text: impl Into<String>,
        key: impl Into<String>,
        id: impl Into<String>,
        up_key: impl Into<String>,
        down_key: impl Into<String>,
    ) -> Self {
        Self {
            class: class.into(),
            text: text.into(),
            key: Some(key.into()),
            slider: None,
            reorder: Some(ReorderSpec {
                up_key: up_key.into(),
                down_key: down_key.into(),
                id: id.into(),
                dragging: false,
                drop_target: false,
            }),
            aria: None,
        }
    }

    /// A segmented slider row: a `label` plus a `count`-cell track keyed by
    /// `key_prefix`, the current `value` (0..1) marked. `hue_track` renders a
    /// rainbow strip.
    pub fn slider(
        label: impl Into<String>,
        key_prefix: impl Into<String>,
        value: f32,
        count: usize,
        hue_track: bool,
    ) -> Self {
        Self {
            class: "app-slider-row".to_string(),
            text: label.into(),
            key: None,
            slider: Some(SliderSpec {
                key_prefix: key_prefix.into(),
                value,
                count,
                hue_track,
            }),
            reorder: None,
            aria: None,
        }
    }

    fn view_identity(&self) -> PaneItemIdentity {
        PaneItemIdentity {
            class: self.class.clone(),
            text: self.text.clone(),
            key: self.key.clone(),
            slider_key_prefix: self.slider.as_ref().map(|slider| slider.key_prefix.clone()),
            reorder_id: self.reorder.as_ref().map(|reorder| reorder.id.clone()),
            aria: self.aria,
        }
    }
}

/// Reposition `id` to where `target` sits in `order` — "drop before the target": remove `id`,
/// then insert it at `target`'s (post-removal) slot. A no-op if either id is absent or they're
/// the same. The pure core of the drag-reorder drop, shared by any reorderable list (the
/// configurable context menu is its first consumer). (Command registry B2.)
pub fn reorder_before(order: &mut Vec<String>, id: &str, target: &str) {
    let Some(from) = order.iter().position(|a| a == id) else {
        return;
    };
    let Some(mut to) = order.iter().position(|a| a == target) else {
        return;
    };
    if from == to {
        return;
    }
    let moved = order.remove(from);
    // Removing an earlier element shifts a later target index left by one.
    if from < to {
        to -= 1;
    }
    order.insert(to, moved);
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

/// Logic alias for the runner (the `ListPane` test harness only). (Phase 1, step 2.)
#[cfg(test)]
pub type ListLogic = fn(&ListPaneState) -> ListView;

/// Render a list pane: the root container with one classed div per item, a
/// button item carrying an `on_click` that queues its key.
pub fn list_pane_view(state: &ListPaneState) -> ListView {
    let mut counts: HashMap<PaneItemIdentity, usize> = HashMap::new();
    let children: Keyed<PaneItemKey, ListView> = state
        .items
        .iter()
        .map(|item| {
            let identity = item.view_identity();
            let occurrence = {
                let next = counts.entry(identity.clone()).or_insert(0);
                let occurrence = *next;
                *next += 1;
                occurrence
            };
            let mut div = el::<_, ListPaneState, ()>("div", item.text.clone())
                .attr("class", item.class.clone());
            if let Some(aria) = item.aria {
                let (role, checked) = aria.role_and_checked();
                div = div.attr("role", role).attr("aria-checked", checked);
            }
            match &item.key {
                Some(key) => {
                    let key = key.clone();
                    // `focusable` puts the button in the Tab order; the runner activates it on
                    // Enter/Space by synthesizing a click that fires this `on_click`, queuing
                    // the activation like a pointer click. (Phase 1, step 3c.)
                    (
                        PaneItemKey {
                            identity,
                            occurrence,
                        },
                        Box::new(clickable(
                            div,
                            move |s: &mut ListPaneState, _: PointerClick| {
                                s.pending.push(key.clone())
                            },
                        )) as ListView,
                    )
                }
                None => (
                    PaneItemKey {
                        identity,
                        occurrence,
                    },
                    Box::new(div) as ListView,
                ),
            }
        })
        .collect();
    Box::new(el::<_, ListPaneState, ()>("div", children).attr("class", state.root_class.clone()))
}

/// A view-driven display/button pane: a [`ViewPane`] over [`ListPaneState`]. It
/// scrolls its root container when the item list overflows (the same vertical
/// scroll as the roster) and carries no a11y row bounds (its a11y is a skeleton),
/// so the surface is set / frame / hit_test / dispatch / drain plus the scroll.
#[cfg(test)]
pub struct ListPane {
    pane: ViewPane<ListPaneState, ListLogic, ListView>,
    /// The root container's class (set each frame), so [`scroll_offsets`](Self::scroll_offsets)
    /// can find the scroll container to shift. Empty before the first `set`.
    root_class: String,
}

#[cfg(test)]
impl ListPane {
    pub fn new() -> Self {
        Self {
            pane: ViewPane::new(list_pane_view as ListLogic, ListPaneState::default()),
            root_class: String::new(),
        }
    }

    /// Replace the sheet, root class, and items for this frame.
    pub fn set(&mut self, sheets: Vec<String>, root_class: &str, items: Vec<PaneItem>) {
        self.pane.set_sheets(sheets);
        let root_class = root_class.to_string();
        self.root_class = root_class.clone();
        self.pane.update(|s| {
            s.root_class = root_class;
            s.items = items;
        });
    }

    /// Render the pane to a scene at `w`×`h`, scrolled `scroll` px down its root
    /// container (the list scrolls when it overflows the pane).
    pub fn frame(&mut self, w: u32, h: u32, scroll: f32) -> Scene {
        let scrolls = self.scroll_offsets(scroll);
        self.pane.frame(w, h, &scrolls)
    }

    /// Hit-test pane-local `(x, y)` (with the pane scrolled by `scroll`).
    pub fn hit_test(&self, x: f32, y: f32, scroll: f32) -> Option<NodeId> {
        let scrolls = self.scroll_offsets(scroll);
        self.pane.hit_test(x, y, &scrolls)
    }

    /// Scroll offsets for the root container at `scroll` px (empty if unset / absent).
    fn scroll_offsets(&self, scroll: f32) -> ScrollOffsets<NodeId> {
        let mut offsets = ScrollOffsets::default();
        if self.root_class.is_empty() {
            return offsets;
        }
        let dom = self.pane.dom();
        let dom = dom.borrow();
        if let Some(node) = crate::first_with_class(&dom, dom.document(), &self.root_class) {
            offsets.insert(node, (0.0, scroll));
        }
        offsets
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

#[cfg(test)]
impl Default for ListPane {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use xilem_serval::PointerClick;

    use super::*;

    fn ids(order: &[&str]) -> Vec<String> {
        order.iter().map(|s| s.to_string()).collect()
    }

    /// Dragging a row onto a later one drops it *before* that target (the row slides down to
    /// make room), and the indices account for the removed element. (Drag-reorder drop, B2.)
    #[test]
    fn reorder_before_moves_down_to_targets_slot() {
        let mut order = ids(&["a", "b", "c", "d"]);
        reorder_before(&mut order, "a", "c");
        assert_eq!(order, ids(&["b", "a", "c", "d"]));
    }

    /// Dragging a row onto an earlier one drops it directly before that target. (B2.)
    #[test]
    fn reorder_before_moves_up_before_target() {
        let mut order = ids(&["a", "b", "c", "d"]);
        reorder_before(&mut order, "d", "b");
        assert_eq!(order, ids(&["a", "d", "b", "c"]));
    }

    /// A drop onto self, or onto / from an absent id, leaves the order untouched. (B2.)
    #[test]
    fn reorder_before_is_a_noop_on_self_or_unknown() {
        let mut order = ids(&["a", "b", "c"]);
        reorder_before(&mut order, "b", "b");
        assert_eq!(order, ids(&["a", "b", "c"]));
        reorder_before(&mut order, "z", "a");
        assert_eq!(order, ids(&["a", "b", "c"]));
        reorder_before(&mut order, "a", "z");
        assert_eq!(order, ids(&["a", "b", "c"]));
    }

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
        let _ = pane.frame(240, 160, 0.0);
        // A point inside the button (below the inert title row, within its padding).
        let node = pane
            .hit_test(30.0, 40.0, 0.0)
            .expect("a node under the button");
        pane.dispatch_click(node, PointerClick::at((30.0, 40.0)));
        let keys = pane.take_activations();
        assert_eq!(
            keys,
            vec!["theme.dark".to_string()],
            "the button's key is queued via runner dispatch, not a rect cache",
        );
    }
}
