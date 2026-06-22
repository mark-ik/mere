/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The settings tiles as a **view-driven, variable-length pane**, folded into the one
//! shell document (the `roster_view` / `list_pane_view` pattern). Unlike the four fixed
//! list panes, a settings tile is dynamic — one per open `settings://` node — and
//! two-column: an **index spine** (the namespace's pages, the active one marked) beside
//! the **page body** (the provider's controls). A spine entry click queues a navigation
//! (retarget the tile's node to `settings://<ns>/<page>`); a body control click queues
//! the same activation key the apparatus drains (a theme id, `engine:toggle:<id>`,
//! `phys:damping:up`), so the host drives both unchanged.
//!
//! One lensed subtree over [`SettingsPanesState`] emits every open pane, each absolutely
//! positioned at its tile-body rect (resolved by `render.rs` from the workbench surface).
//! Body controls reuse the apparatus author classes (`app-title` / `app-btn` /
//! `app-btn-active` / `app-row`), already in the shell stylesheet, so only the two column
//! containers carry inline geometry + the host-resolved panel background. See
//! `2026-06-21_settings_lane_consolidation_plan` (P1 render arm).

use forme::GraphMemberId;
use xilem_serval::{AnyView, PointerClick, ServalCtx, ServalElement, el, focusable, on_click};

use crate::list_pane::{PaneItem, SliderSpec};

/// The erased view the settings-panes logic produces (mirrors `RosterView`).
pub type SettingsPanesView = Box<dyn AnyView<SettingsPanesState, (), ServalCtx, ServalElement>>;

/// One entry in a settings tile's index spine: the page id (suffixed onto the namespace
/// to form the nav target), its display title, and whether it is the active page.
pub struct SettingsSpineEntry {
    pub id: String,
    pub title: String,
    pub active: bool,
}

/// One open settings tile: the node it belongs to, its body rect (window px), the
/// namespace (e.g. `pelt`), the active page's title + controls, and the index spine.
pub struct SettingsPane {
    /// The settings node this tile renders, so a spine nav / control activation knows
    /// which tile to act on when drained.
    pub member: GraphMemberId,
    /// The tile body rect `[x0, y0, x1, y1]` in window px (below the surface's tab bar).
    pub rect: [f32; 4],
    /// The settings namespace (e.g. `pelt`), prefixed onto a spine id for the nav target.
    pub namespace: String,
    /// The active page's display title (the body's heading).
    pub page_title: String,
    pub spine: Vec<SettingsSpineEntry>,
    pub items: Vec<PaneItem>,
}

/// The shell document's settings panes plus the intents their handlers queued. One
/// state object lensed once (the dynamic list lives inside it, the `roster_view` shape).
#[derive(Default)]
pub struct SettingsPanesState {
    pub panes: Vec<SettingsPane>,
    /// The chrome theme's panel background as an `rgb(...)` string, resolved host-side so
    /// the column containers match the active theme without a sheet of their own.
    pub panel_bg: String,
    /// `(member, key)` a page control queued — the host applies it like an apparatus
    /// activation (theme / engine toggle / physics step).
    pub pending_keys: Vec<(GraphMemberId, String)>,
    /// `(member, "settings://ns/page")` a spine entry queued — the host navigates that
    /// settings tile's node to the page.
    pub pending_nav: Vec<(GraphMemberId, String)>,
}

/// Build the settings panes subtree: a passthrough container holding one absolutely
/// positioned two-column pane per open settings tile.
pub fn settings_panes_view(state: &SettingsPanesState) -> SettingsPanesView {
    let children: Vec<SettingsPanesView> =
        state.panes.iter().map(|pane| pane_view(pane, &state.panel_bg)).collect();
    Box::new(el::<_, SettingsPanesState, ()>("div", children).attr("class", "settings-panes"))
}

/// One settings tile: a flex row of the index spine and the page body, positioned at the
/// tile body rect (absolute against the shell root).
fn pane_view(pane: &SettingsPane, panel_bg: &str) -> SettingsPanesView {
    let [x0, y0, x1, y1] = pane.rect;
    let (w, h) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
    let member = pane.member;

    // Index spine: one navigator per page, the active page highlighted. A click retargets
    // the tile's node to `settings://<ns>/<page>` (drained host-side via `navigate_member`).
    let spine_children: Vec<SettingsPanesView> = pane
        .spine
        .iter()
        .map(|entry| {
            let class = if entry.active { "app-btn-active" } else { "app-btn" };
            let nav = format!("settings://{}/{}", pane.namespace, entry.id);
            let div = el::<_, SettingsPanesState, ()>("div", entry.title.clone()).attr("class", class);
            Box::new(focusable(on_click(div, move |s: &mut SettingsPanesState, _: PointerClick| {
                s.pending_nav.push((member, nav.clone()))
            }))) as SettingsPanesView
        })
        .collect();
    let spine = el::<_, SettingsPanesState, ()>("div", spine_children)
        .attr("class", "settings-spine")
        .attr(
            "style",
            format!(
                "width:184px;flex-shrink:0;height:100%;overflow:scroll;padding:8px;box-sizing:border-box;\
                 background-color:{panel_bg};border-right:1px solid rgba(255,255,255,0.08)"
            ),
        );

    // Page body: the active page's title heading plus the provider's controls (the same
    // `PaneItem`s the apparatus renders, so their classes are already themed).
    let mut body_children: Vec<SettingsPanesView> = Vec::with_capacity(pane.items.len() + 1);
    body_children.push(Box::new(
        el::<_, SettingsPanesState, ()>("div", pane.page_title.clone()).attr("class", "app-title"),
    ));
    for item in &pane.items {
        body_children.push(item_view(member, item));
    }
    let body = el::<_, SettingsPanesState, ()>("div", body_children)
        .attr("class", "settings-pane-body")
        .attr(
            "style",
            format!(
                "flex:1;height:100%;overflow:scroll;padding:8px;box-sizing:border-box;background-color:{panel_bg}"
            ),
        );

    Box::new(
        el::<_, SettingsPanesState, ()>(
            "div",
            vec![Box::new(spine) as SettingsPanesView, Box::new(body) as SettingsPanesView],
        )
        .attr("class", "settings-pane")
        .attr(
            "style",
            format!("position:absolute;left:{x0}px;top:{y0}px;width:{w}px;height:{h}px;display:flex;overflow:hidden"),
        ),
    )
}

/// One body control: a classed text row, or (when the `PaneItem` carries a key) a
/// clickable button that queues `(member, key)` for the host to apply — the settings
/// twin of `list_pane`'s item view.
fn item_view(member: GraphMemberId, item: &PaneItem) -> SettingsPanesView {
    if let Some(spec) = &item.slider {
        return slider_view(member, &item.text, spec);
    }
    let div = el::<_, SettingsPanesState, ()>("div", item.text.clone()).attr("class", item.class.clone());
    match &item.key {
        Some(key) => {
            let key = key.clone();
            Box::new(focusable(on_click(div, move |s: &mut SettingsPanesState, _: PointerClick| {
                s.pending_keys.push((member, key.clone()))
            })))
        }
        None => Box::new(div),
    }
}

/// A segmented slider: a label over a flex strip of `count` clickable cells.
/// Cell `i` queues `"<prefix>:<i>:<count>"` (the host sets the channel to
/// `i/count`). A hue track colours each cell by its hue (a rainbow picker) and
/// outlines the selected one; a magnitude track fills cells up to `value`.
fn slider_view(member: GraphMemberId, label: &str, spec: &SliderSpec) -> SettingsPanesView {
    let label_div = el::<_, SettingsPanesState, ()>("div", label.to_string()).attr("class", "app-slider-label");
    let count = spec.count.max(1);
    let selected = ((spec.value * count as f32).round() as usize).min(count - 1);
    let cells: Vec<SettingsPanesView> = (0..count)
        .map(|i| {
            let key = format!("{}:{}:{}", spec.key_prefix, i, count);
            let style = if spec.hue_track {
                let c = tincture::color_from_hsl(i as f64 / count as f64 * 360.0, 0.7, 0.5);
                let sel = if i == selected { "outline:2px solid #ffffff;outline-offset:-2px;" } else { "" };
                format!("flex:1;background-color:#{:02X}{:02X}{:02X};{sel}", c.r, c.g, c.b)
            } else {
                let fill = if i <= selected { "rgba(255,255,255,0.55)" } else { "rgba(255,255,255,0.12)" };
                let sel = if i == selected { "outline:2px solid #ffffff;outline-offset:-2px;" } else { "" };
                format!("flex:1;background-color:{fill};{sel}")
            };
            let cell = el::<_, SettingsPanesState, ()>("div", String::new())
                .attr("class", "app-seg")
                .attr("style", style);
            Box::new(on_click(cell, move |s: &mut SettingsPanesState, _: PointerClick| {
                s.pending_keys.push((member, key.clone()))
            })) as SettingsPanesView
        })
        .collect();
    let track = el::<_, SettingsPanesState, ()>("div", cells).attr("class", "app-slider-track");
    Box::new(
        el::<_, SettingsPanesState, ()>(
            "div",
            vec![Box::new(label_div) as SettingsPanesView, Box::new(track) as SettingsPanesView],
        )
        .attr("class", "app-slider-row"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_pane::ViewPane;
    use layout_dom_api::LayoutDom;
    use serval_layout::ScrollOffsets;
    use serval_scripted_dom::NodeId;
    use xilem_serval::PointerClick;

    type Pane =
        ViewPane<SettingsPanesState, fn(&SettingsPanesState) -> SettingsPanesView, SettingsPanesView>;

    /// A sheet sizing the reused apparatus classes so the headless pane has hit-testable
    /// boxes (the column geometry is inline on the view).
    fn sheet() -> Vec<String> {
        vec![
            "div { display: block; }".into(),
            ".app-title { font-size: 13px; padding: 6px; }".into(),
            ".app-btn { font-size: 15px; padding: 10px; }".into(),
            ".app-btn-active { font-size: 15px; padding: 10px; }".into(),
        ]
    }

    fn pane_over(state: SettingsPanesState) -> Pane {
        let mut pane: Pane = ViewPane::new(
            settings_panes_view as fn(&SettingsPanesState) -> SettingsPanesView,
            state,
        );
        pane.set_sheets(sheet());
        pane
    }

    /// The on-screen (viewport-absolute) center of the first element carrying `class`, for
    /// a robust hit. `rect_of` is parent-relative, so origins are accumulated down the tree
    /// the way the real render path does. The tests isolate so the class appears once.
    fn center_of(pane: &Pane, class: &str) -> (f32, f32) {
        let dom = pane.dom();
        let dom = dom.borrow();
        let frags = pane.fragments().expect("laid out");
        let node = crate::first_with_class(&dom, dom.document(), class).expect("class in document");
        let mut origins = std::collections::HashMap::new();
        crate::serval_render::accumulate_origins(&dom, frags, dom.document(), (0.0, 0.0), &mut origins);
        let &(ox, oy) = origins.get(&node).expect("origin for the node");
        let size = frags.rect_of(node).expect("node has a rect").size;
        (ox + size.width / 2.0, oy + size.height / 2.0)
    }

    /// A spine entry click queues a navigation to that page's `settings://` url (the host
    /// then retargets the tile's node). The only inactive `app-btn` is the "Engines" entry.
    #[test]
    fn clicking_a_spine_entry_queues_its_page_nav() {
        let member = GraphMemberId::nil();
        let mut pane = pane_over(SettingsPanesState {
            panes: vec![SettingsPane {
                member,
                rect: [0.0, 0.0, 420.0, 300.0],
                namespace: "pelt".into(),
                page_title: "Appearance".into(),
                spine: vec![
                    SettingsSpineEntry { id: "appearance".into(), title: "Appearance".into(), active: true },
                    SettingsSpineEntry { id: "engines".into(), title: "Engines".into(), active: false },
                ],
                items: Vec::new(),
            }],
            panel_bg: "rgb(20, 20, 20)".into(),
            pending_keys: Vec::new(),
            pending_nav: Vec::new(),
        });
        let scroll = ScrollOffsets::<NodeId>::default();
        let _ = pane.frame(420, 300, &scroll);
        let (x, y) = center_of(&pane, "app-btn");
        let node = pane.hit_test(x, y, &scroll).expect("a node under the entry");
        pane.dispatch_click(node, PointerClick::at((x, y)));
        let mut nav = Vec::new();
        pane.update(|s| nav = std::mem::take(&mut s.pending_nav));
        assert_eq!(nav, vec![(member, "settings://pelt/engines".to_string())]);
    }

    /// A page-control click queues that control's activation key — the same key the
    /// apparatus drains, so the host drives it unchanged.
    #[test]
    fn clicking_a_page_control_queues_its_key() {
        let member = GraphMemberId::nil();
        let mut pane = pane_over(SettingsPanesState {
            panes: vec![SettingsPane {
                member,
                rect: [0.0, 0.0, 420.0, 300.0],
                namespace: "pelt".into(),
                page_title: "Appearance".into(),
                spine: Vec::new(),
                items: vec![PaneItem::button("app-btn", "Dark", "theme.dark")],
            }],
            panel_bg: "rgb(20, 20, 20)".into(),
            pending_keys: Vec::new(),
            pending_nav: Vec::new(),
        });
        let scroll = ScrollOffsets::<NodeId>::default();
        let _ = pane.frame(420, 300, &scroll);
        let (x, y) = center_of(&pane, "app-btn");
        let node = pane.hit_test(x, y, &scroll).expect("a node under the control");
        pane.dispatch_click(node, PointerClick::at((x, y)));
        let mut keys = Vec::new();
        pane.update(|s| keys = std::mem::take(&mut s.pending_keys));
        assert_eq!(keys, vec![(member, "theme.dark".to_string())]);
    }

    /// A `settings://` url reads as a friendly page title; a non-settings url is left alone.
    #[test]
    fn tab_title_reads_settings_pages() {
        assert_eq!(crate::settings_lane::settings_tab_title("https://example.com"), None);
        assert_eq!(
            crate::settings_lane::settings_tab_title("settings://pelt/appearance").as_deref(),
            Some("Settings: Appearance"),
        );
    }
}
