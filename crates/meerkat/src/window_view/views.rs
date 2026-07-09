/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Shell + card view builders (pure `WindowLocal -> WindowLocalView` functions, lensed
//! per window into `AppState` by [`shell_view`]).
//!
//! **Node vs card.** A node is an object — a physics body with its own hull in seiche, a
//! DOM object here for tabbing/hit-test/a11y — that *references* an addressed thing (a
//! page, a file, a settings namespace); it is not that thing, and it is not a card. Its
//! rendered body is a **gnode** (the `.gnode` class), drawn either here as retained
//! chrome DOM or by the orrery crate itself as an in-scene Scene layer — one primitive,
//! two render tiers, never a summonable card. A **card** is
//! summoned *about* a node or selection: the focus slot's preview/unvisited/object/
//! connections family ([`FocusCard`]/[`FocusCardKind`], `focus_card_view`) and the
//! roster's detail cards. See `design_docs/mere_docs/design/2026-07-01_node_card_summoning_design.md`.

use super::*;

/// One window's shell view over the whole [`AppState`]: [`window_local_view`] built for
/// window `i` and lensed onto `windows[i]`, with the shared crawl chip cloned in. This is
/// each projection's logic — `Shell.multi` holds one per window (captured `i`). The lens's
/// `access` closure captures `i`, so a handler writes back to `app.windows[i]`; the crawl
/// chip reads `app.shared` directly (no cell — the multi-runner owns it). (One state, N
/// windows — Slice 3.)
pub(crate) fn shell_view(app: &AppState, i: usize) -> ShellView {
    let crawl = app.shared.crawl.clone();
    Box::new(lens(
        move |wl: &mut WindowLocal| window_local_view(wl, &crawl),
        move |app: &mut AppState| &mut app.windows[i],
    ))
}

/// One window's shell view, over its [`WindowLocal`]: a full-bleed container holding the
/// chrome (lifted via `lens`) and the orrery element as siblings, plus the folded panes.
/// The chrome stays in normal flow exactly as it laid out when it was the root; the orrery
/// element is absolutely positioned, so it does not disturb the chrome. `crawl` is the
/// shared chrome chip the caller cloned out of `AppState.shared`. (Orrery-as-element; one
/// state, N windows — Slice 3.)
pub(crate) fn window_local_view(s: &WindowLocal, crawl: &CrawlIndicator) -> WindowLocalView {
    // The crawl chip is a window-invariant truth from the shared chrome (cloned in by the
    // caller): own a clone so the lens's `make` closure can hold it, while event routing
    // still targets this window's `chrome`. (One state, N windows — Slice 0/3.)
    let crawl = crawl.clone();
    let make_chrome = move |c: &mut Chrome| chrome_view(c, &crawl);
    let to_chrome: fn(&mut WindowLocal) -> &mut Chrome = |s: &mut WindowLocal| &mut s.chrome;
    let chrome = lens(make_chrome, to_chrome);
    // The roster pane, when open, is a positioned subtree of the shell document: its view
    // is lensed onto `WindowLocal.roster`, so the one shell runner renders it, hit-tests it,
    // and dispatches its row clicks. `None` keeps the document identical to before the fold.
    // (Phase 1.)
    let roster = s.roster_rect.map(|[x0, y0, x1, y1]| {
        let make_roster: fn(&mut RosterState) -> RosterView = |r: &mut RosterState| roster_view(r);
        let to_roster: fn(&mut WindowLocal) -> &mut RosterState = |s: &mut WindowLocal| &mut s.roster;
        Box::new(
            el::<_, WindowLocal, ()>("div", lens(make_roster, to_roster))
                .attr("class", "roster-pane")
                .attr(
                    "style",
                    format!(
                        "position:absolute;left:{x0}px;top:{y0}px;width:{}px;height:{}px;overflow:hidden",
                        x1 - x0,
                        y1 - y0
                    ),
                ),
        ) as WindowLocalView
    });
    // The gloss outline lens, when open, is a positioned subtree of the shell document
    // like the roster: lensed onto `WindowLocal.gloss_outline`, sized to the gloss pane's
    // middle third ([`gloss::gloss_sections`]) so the one shell runner renders it,
    // hit-tests it, and dispatches its row clicks — the first DOM gloss section, the
    // minimap and recent list still Scene-rasterize the top/bottom thirds. `None` keeps
    // the document identical to before this section existed. (gloss-outline plan P1.)
    let gloss_outline = s.gloss_outline_rect.map(|[x0, y0, x1, y1]| {
        let make_outline: fn(&mut GlossOutlineState) -> GlossOutlineView =
            |g: &mut GlossOutlineState| gloss_outline_view(g);
        let to_outline: fn(&mut WindowLocal) -> &mut GlossOutlineState =
            |s: &mut WindowLocal| &mut s.gloss_outline;
        Box::new(
            el::<_, WindowLocal, ()>("div", lens(make_outline, to_outline))
                .attr("class", "gloss-outline-pane")
                .attr(
                    "style",
                    format!(
                        "position:absolute;left:{x0}px;top:{y0}px;width:{}px;height:{}px;overflow:hidden",
                        x1 - x0,
                        y1 - y0
                    ),
                ),
        ) as WindowLocalView
    });
    // The gloss recent-visited lens, when open, is a positioned subtree of the shell
    // document exactly like the outline above — the Scene-to-DOM migration's Phase 1.
    // Sized to the gloss pane's bottom third ([`gloss::gloss_sections`]).
    let gloss_recent = s.gloss_recent_rect.map(|[x0, y0, x1, y1]| {
        let make_recent: fn(&mut GlossRecentState) -> GlossRecentView =
            |g: &mut GlossRecentState| recent_view(g);
        let to_recent: fn(&mut WindowLocal) -> &mut GlossRecentState =
            |s: &mut WindowLocal| &mut s.gloss_recent;
        Box::new(
            el::<_, WindowLocal, ()>("div", lens(make_recent, to_recent))
                .attr("class", "gloss-recent-pane")
                .attr(
                    "style",
                    format!(
                        "position:absolute;left:{x0}px;top:{y0}px;width:{}px;height:{}px;overflow:hidden",
                        x1 - x0,
                        y1 - y0
                    ),
                ),
        ) as WindowLocalView
    });
    // The gloss minimap, when open, is TWO positioned elements at the same rect — the
    // Scene-to-DOM migration's Phase 2, split like this per a debugging finding: an
    // `<external-texture>` nested inside a *lensed* subtree broke the whole chrome
    // document (see the debugging note on `gloss_view::minimap_view`). So the backdrop
    // (edges/rings) is a top-level, non-lensed `<external-texture>` — exactly how
    // `orrery_element`'s own backdrop is a direct, non-lensed shell-tuple child, not a
    // lensed one — while only the interactive node squares (which need per-node click
    // state) stay lensed onto `WindowLocal.gloss_minimap`. Sized to the gloss pane's
    // top third.
    let gloss_minimap_backdrop = s.gloss_minimap_rect.map(|[x0, y0, x1, y1]| {
        Box::new(
            external_texture::<WindowLocal, ()>(
                crate::gloss_view::GLOSS_MINIMAP_SCENE_KEY,
                (x1 - x0).max(1.0) as u32,
                (y1 - y0).max(1.0) as u32,
            )
            .attr("class", "gloss-minimap-backdrop")
            .attr(
                "style",
                format!(
                    "position:absolute;left:{x0}px;top:{y0}px;width:{}px;height:{}px",
                    x1 - x0,
                    y1 - y0
                ),
            ),
        ) as WindowLocalView
    });
    let gloss_minimap = s.gloss_minimap_rect.map(|[x0, y0, x1, y1]| {
        let make_minimap: fn(&mut GlossMinimapState) -> GlossMinimapView =
            |g: &mut GlossMinimapState| minimap_view(g);
        let to_minimap: fn(&mut WindowLocal) -> &mut GlossMinimapState =
            |s: &mut WindowLocal| &mut s.gloss_minimap;
        Box::new(
            el::<_, WindowLocal, ()>("div", lens(make_minimap, to_minimap))
                .attr("class", "gloss-minimap-pane")
                .attr(
                    "style",
                    format!(
                        "position:absolute;left:{x0}px;top:{y0}px;width:{}px;height:{}px;overflow:hidden",
                        x1 - x0,
                        y1 - y0
                    ),
                ),
        ) as WindowLocalView
    });
    // The four list panes (apparatus / steward / inspector / trail), each a positioned
    // subtree of the shell document when open: its inner `list_pane_view` is lensed onto
    // the matching `panes` slot, so the one shell runner lays it out, scrolls it, and
    // dispatches its button clicks. `None` rects keep the document unchanged. (Phase 1.)
    let list_pane = |i: usize| -> Option<WindowLocalView> {
        s.pane_rects[i].map(|[x0, y0, x1, y1]| {
            let make: fn(&mut ListPaneState) -> ListView = |st| list_pane_view(st);
            Box::new(
                el::<_, WindowLocal, ()>("div", lens(make, PANE_TO[i]))
                    .attr("class", PANE_WRAPPER_CLASS[i])
                    .attr(
                        "style",
                        format!(
                            "position:absolute;left:{x0}px;top:{y0}px;width:{}px;height:{}px;overflow:hidden",
                            x1 - x0,
                            y1 - y0
                        ),
                    ),
            ) as WindowLocalView
        })
    };
    let list_panes = (
        list_pane(0),
        list_pane(1),
        list_pane(2),
        list_pane(3),
        list_pane(4),
    );
    // The settings tiles (variable count), folded in as one lensed subtree that emits one
    // absolutely positioned two-column pane (index spine + page body) per open `settings://`
    // tile. Absent when none are open, so the document is identical before any settings tile.
    // (Settings lane P1.)
    let settings = (!s.settings.panes.is_empty()).then(|| {
        let make: fn(&mut SettingsPanesState) -> SettingsPanesView =
            |s: &mut SettingsPanesState| settings_panes_view(s);
        let to: fn(&mut WindowLocal) -> &mut SettingsPanesState =
            |s: &mut WindowLocal| &mut s.settings;
        Box::new(
            el::<_, WindowLocal, ()>("div", lens(make, to)).attr("class", "settings-panes-host"),
        ) as WindowLocalView
    });
    Box::new(
        el::<_, WindowLocal, ()>(
            "shell",
            // Document order is paint + hit-test order in the one shell scene: the orrery
            // nodes and the folded panes come first (the content), then the chrome LAST so
            // its modal overlays (context menu, palette, find, settings) paint over the
            // gnodes and win the hit-test, instead of the gnode DOM (formerly later in the
            // document) occluding the menu and stealing its clicks. The toolbar is in normal
            // flow and the content roots are `position:absolute`, so their geometry is
            // unchanged by the reorder; only the z-order is. The settings panes sit with the
            // other folded content (before the chrome), painting over the workbench composite
            // at their tile rects while the chrome overlays still win above them.
            (
                orrery_element(&s.orrery),
                roster,
                gloss_outline,
                gloss_recent,
                gloss_minimap_backdrop,
                gloss_minimap,
                list_panes,
                settings,
                chrome,
            ),
        )
        .attr("style", "position:relative;width:100%;height:100%"),
    )
}

/// The external-texture key for the orrery scene underlay: a reserved high value, disjoint from the
/// workbench's per-member UUID-low-64 keys. The host rasterizes the seiche scene under it and
/// composites it at the element's laid-out rect, which `render.rs` enumerates from the document
/// (the external-texture-element compose). (cond 5.)
pub(crate) const ORRERY_SCENE_KEY: u64 = 0xF0F0_0000_0000_0001;

/// The orrery element: a positioned container whose gnodes are `position:absolute`
/// + `transform: translate(...)` DOM placed by seiche's world positions. The shell view
/// reserves a stable host-owned child pool for them; the render path reconciles the pool
/// directly against the DOM each frame. The underlay (edges + demoted dots) joins in (ii).
/// The rect is a placeholder until the frame tree drives the container layout (iii).
/// (Phase 2.)
pub(crate) fn orrery_element(render: &OrreryRender) -> WindowLocalView {
    let [x0, y0, x1, y1] = render.rect;
    let (pw, ph) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
    // The orrery scene (seiche edges / backdrop / demoted dots), which the host rasterizes to a
    // texture, sits as an `<external-texture>` underlay in the document so its placement comes from
    // layout and the gnodes stack over it via the DOM. First child = painted first = under the
    // gnodes. (cond 5: the scene becomes a document element, not a standalone host composite.)
    let scene: WindowLocalView = Box::new(
        external_texture::<WindowLocal, ()>(ORRERY_SCENE_KEY, pw as u32, ph as u32)
            .attr("class", "orrery-scene")
            .attr(
                "style",
                format!("position:absolute;left:0;top:0;width:{pw}px;height:{ph}px"),
            ),
    );
    let gnode_pool: WindowLocalView = Box::new(
        host_pool::<WindowLocal, ()>("div", ORRERY_GNODE_POOL_ID)
            .attr("class", ORRERY_GNODE_POOL_CLASS)
            .attr(
                "style",
                format!("position:absolute;left:0;top:0;width:{pw}px;height:{ph}px"),
            ),
    );
    // The focused node's content card paints LAST among the orrery's children, so it sits
    // over the gnodes (the spatial map's nodes are under the focused content). The
    // chrome (and its overlays) still paints over it, since the orrery element precedes the
    // chrome in the shell document. (Layering fix — card over nodes.)
    let focus_card = render.focus_card.as_ref().map(focus_card_view);
    // The orrery pane element bears the wheel: a wheel the host dispatches here queues its delta
    // for the host to drain into seiche's pan / Ctrl-zoom, routing the orrery wheel through the
    // document (the form wheel.rs intends). The gnodes / scene under it have no wheel handler, so
    // the runner's ancestor walk resolves any orrery wheel to this element. (cond 5 input bridge.)
    Box::new(on_wheel(
        el::<_, WindowLocal, ()>("div", (scene, gnode_pool, focus_card))
            .attr("class", "orrery")
            .attr(
                "style",
                // z-index:0 makes the orrery the base layer of the shell z-stack: a
                // stacking context that *contains* its gnodes and focus card, so they paint
                // within it and never hoist to compete with the chrome (z-index:10, above).
                // (Shell z-stack — card under chrome.)
                format!(
                    "position:absolute;left:{x0}px;top:{y0}px;width:{}px;height:{}px;overflow:hidden;z-index:0",
                    x1 - x0,
                    y1 - y0
                ),
            ),
        |s: &mut WindowLocal, w: WheelEvent| s.orrery_wheel = Some(w.delta),
    ))
}

/// The focused node's content card: a positioned element over the orrery's gnodes. A
/// `Snapshot` is a framed card holding a PNG data-URI `<img>` of the page's top peek (the
/// host builds + caches it per member while the URL matches); an `Unvisited` is a dashed
/// "double-click to load" placeholder (double-click is host-handled via `content_rects`,
/// so the element needs no click handler). The card is opaque chrome DOM after the gnodes,
/// so document order paints it over them and under the chrome overlays. (Layering fix.)
pub(crate) fn focus_card_view(fc: &FocusCard) -> WindowLocalView {
    let [x0, y0, x1, y1] = fc.rect;
    let (w, h) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
    match &fc.kind {
        FocusCardKind::Snapshot { data_uri } => {
            // The preview is a PNG data-URI <img> (like the favicons), so it is opaque chrome
            // DOM after the gnodes: document order paints it over them. Only the cached
            // image renders — there is no placeholder while it builds. (Layering fix.)
            let img: WindowLocalView = Box::new(
                el::<_, WindowLocal, ()>("img", ())
                    .attr("src", data_uri.clone())
                    .attr(
                        "style",
                        "width:100%;height:100%;border-radius:8px;display:block",
                    ),
            );
            // `overlay_rect` owns the geometry (and the hit-test class); the card's visuals
            // (clip, radius, shadow) ride the inner div, which fills the positioned box —
            // adding a `style` to the overlay element would clobber its geometry. (Overlay P2.)
            // A page-like light background so the card is a visible surface even if the
            // peek `<img>` has not painted (its own bg is transparent — without this the
            // card reads as just its shadow over the dark orrery). The img fills over it
            // when it renders. (Card legibility.)
            let card: WindowLocalView = Box::new(el::<_, WindowLocal, ()>("div", vec![img]).attr(
                "style",
                "width:100%;height:100%;box-sizing:border-box;overflow:hidden;\
                     background-color:rgb(250,250,252);\
                     border-radius:8px;box-shadow:0 6px 24px rgba(0,0,0,0.55)",
            ));
            Box::new(
                overlay_rect::<_, WindowLocal, ()>(x0, y0, w, h, vec![card])
                    .attr("class", "snapshot-card"),
            )
        }
        FocusCardKind::Unvisited => {
            let inner: WindowLocalView = Box::new(
                el::<_, WindowLocal, ()>("div", "Double-click to load".to_string()).attr(
                    "style",
                    "width:100%;height:100%;box-sizing:border-box;display:flex;align-items:center;\
                     justify-content:center;border:1px dashed #5a6478;border-radius:8px;\
                     color:#8b94a6;font-size:13px;background:rgba(28,32,40,0.88)",
                ),
            );
            Box::new(
                overlay_rect::<_, WindowLocal, ()>(x0, y0, w, h, vec![inner])
                    .attr("class", "unvisited-card"),
            )
        }
        // The per-object action card: the widgets' (caption, control) rows flattened as direct
        // children of `object-card`, block-stacked exactly like the P0 single widget did (no
        // flex container, no per-widget wrapper) so the controls' clicks route + fire. The
        // `object-card` class is the click-routing + double-click-suppress gate's key. (P1.)
        FocusCardKind::ObjectCard { widgets } => {
            let mut children: Vec<WindowLocalView> = Vec::with_capacity(widgets.len() * 2);
            for widget in widgets {
                let (caption, control) = object_card_widget_row(widget);
                children.push(caption);
                children.push(control);
            }
            let inner: WindowLocalView = Box::new(el::<_, WindowLocal, ()>("div", children).attr(
                "style",
                "width:100%;height:100%;box-sizing:border-box;padding:9px 12px;\
                     border-radius:8px;background:rgba(28,32,40,0.96);\
                     box-shadow:0 6px 24px rgba(0,0,0,0.55)",
            ));
            Box::new(
                overlay_rect::<_, WindowLocal, ()>(x0, y0, w, h, vec![inner])
                    .attr("class", "object-card"),
            )
        }
        FocusCardKind::Connections { spec } => {
            // The connections swatch is host-generic DOM (the P1 lift), mounted directly over
            // `WindowLocal`: no per-state callbacks, the hit-test routes on `data-element` (P4).
            let inner: WindowLocalView =
                crate::swatch::connections_swatch_view::<WindowLocal>(spec);
            Box::new(
                overlay_rect::<_, WindowLocal, ()>(x0, y0, w, h, vec![inner])
                    .attr("class", "connections-card"),
            )
        }
    }
}

/// Render one object-card widget as a `(caption, control)` pair the card stacks as direct
/// children. Each control queues an `object_card_keys` activation the host drains + dispatches.
/// (Object card — P1.)
pub(crate) fn object_card_widget_row(widget: &CardWidget) -> (WindowLocalView, WindowLocalView) {
    match widget {
        CardWidget::SizeTier { tier } => {
            let title: WindowLocalView = Box::new(
                el::<_, WindowLocal, ()>("div", "Size".to_string())
                    .attr("style", "color:#8b94a6;font-size:11px;margin-bottom:5px"),
            );
            let dots: String = (0..orrery::SIZE_TIERS.len())
                .map(|i| if i <= *tier { '\u{25CF}' } else { '\u{25CB}' })
                .collect();
            let btn = "width:30px;height:30px;display:flex;align-items:center;justify-content:center;\
                       border-radius:6px;background:#2a2f3a;color:#d8deea;font-size:20px;font-weight:600;\
                       cursor:pointer;user-select:none";
            let minus: WindowLocalView = Box::new(on_click(
                el::<_, WindowLocal, ()>("div", "\u{2212}".to_string()).attr("style", btn),
                move |s: &mut WindowLocal, _: PointerClick| {
                    s.object_card_keys.push("size:down".to_string())
                },
            ));
            let plus: WindowLocalView = Box::new(on_click(
                el::<_, WindowLocal, ()>("div", "+".to_string()).attr("style", btn),
                move |s: &mut WindowLocal, _: PointerClick| {
                    s.object_card_keys.push("size:up".to_string())
                },
            ));
            let notches: WindowLocalView = Box::new(
                el::<_, WindowLocal, ()>("span", dots)
                    .attr("style", "color:#9aa4b8;font-size:15px;letter-spacing:5px"),
            );
            let row: WindowLocalView = Box::new(
                el::<_, WindowLocal, ()>("div", vec![minus, notches, plus]).attr(
                    "style",
                    "display:flex;align-items:center;justify-content:space-between;margin-bottom:11px",
                ),
            );
            (title, row)
        }
        CardWidget::Face { is_favicon } => {
            let title: WindowLocalView = Box::new(
                el::<_, WindowLocal, ()>("div", "Face".to_string())
                    .attr("style", "color:#8b94a6;font-size:11px;margin-bottom:5px"),
            );
            let seg = |active: bool| -> String {
                let (bg, fg) = if active {
                    ("#3a4150", "#ffffff")
                } else {
                    ("#2a2f3a", "#9aa4b8")
                };
                format!(
                    "flex:1;height:30px;display:flex;align-items:center;justify-content:center;\
                     background:{bg};color:{fg};font-size:13px;cursor:pointer;user-select:none"
                )
            };
            let favicon_btn: WindowLocalView = Box::new(on_click(
                el::<_, WindowLocal, ()>("div", "Favicon".to_string()).attr(
                    "style",
                    format!("{};border-radius:6px 0 0 6px", seg(*is_favicon)),
                ),
                move |s: &mut WindowLocal, _: PointerClick| {
                    s.object_card_keys.push("face:favicon".to_string())
                },
            ));
            let shape_btn: WindowLocalView = Box::new(on_click(
                el::<_, WindowLocal, ()>("div", "Plain".to_string()).attr(
                    "style",
                    format!("{};border-radius:0 6px 6px 0", seg(!*is_favicon)),
                ),
                move |s: &mut WindowLocal, _: PointerClick| {
                    s.object_card_keys.push("face:bare".to_string())
                },
            ));
            let row: WindowLocalView = Box::new(
                el::<_, WindowLocal, ()>("div", vec![favicon_btn, shape_btn])
                    .attr("style", "display:flex;gap:2px"),
            );
            (title, row)
        }
    }
}
