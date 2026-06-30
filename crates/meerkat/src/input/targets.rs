/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Pointer hit-testing: card / tile link resolution and scripted-click routing.

use super::*;

impl WindowCtx<'_> {
    /// Whether window point `(x, y)` is over a composited content card (its rect
    /// from the last frame). Clicks / scroll over the card route to the card, not
    /// the orrery beneath it.
    pub(crate) fn point_over_card(&self, x: f32, y: f32) -> bool {
        self.view
            .content_rects
            .iter()
            .any(|(_, r)| x >= r[0] && x <= r[2] && y >= r[1] && y <= r[3])
    }

    /// The `(card URL, link href)` under window point `(x, y)`, if it lands on a link
    /// in a composited content card. Maps the point into the card's content-local
    /// space (its rect origin + the card's scroll) and queries the actor's link map;
    /// the base is the card member's own URL, for resolving relative links. `None`
    /// when the point is over no card link (the caller keeps its normal click).
    /// (Inline-link nav.)
    pub(crate) fn card_link_at(&self, x: f32, y: f32) -> Option<(String, String)> {
        for (member, r) in &self.view.content_rects {
            if x >= r[0] && x <= r[2] && y >= r[1] && y <= r[3] {
                let scroll = self.view.scroll.get(member).copied().unwrap_or(0.0);
                let lx = x - r[0];
                let ly = (y - r[1]) + scroll;
                if let Some(href) = self.shared.content.constellation.link_at(*member, lx, ly) {
                    let href = href.to_string();
                    let base = self
                        .orrery()
                        .graph()
                        .get_node_by_id(*member)
                        .map(|(_, n)| n.url().to_string())
                        .unwrap_or_default();
                    return Some((base, href));
                }
            }
        }
        None
    }

    /// The `(tile member, member URL, link href)` under window `(x, y)`, if it lands on
    /// a link in a workbench tile's composited content. The tile counterpart to
    /// [`card_link_at`](Self::card_link_at): the tiles composite at `tile_rects` (window
    /// axis-aligned, no orrery camera), so the same content-local mapping (rect origin +
    /// the tile's scroll) and the same actor link map resolve it. Returns the member too,
    /// since a tile click navigates *that* tile rather than the focused card.
    pub(crate) fn tile_link_at(&self, x: f32, y: f32) -> Option<(GraphMemberId, String, String)> {
        for (member, r) in &self.view.tile_rects {
            if x >= r[0] && x <= r[2] && y >= r[1] && y <= r[3] {
                let scroll = self.view.scroll.get(member).copied().unwrap_or(0.0);
                let lx = x - r[0];
                let ly = (y - r[1]) + scroll;
                if let Some(href) = self.shared.content.constellation.link_at(*member, lx, ly) {
                    let href = href.to_string();
                    let base = self
                        .orrery()
                        .graph()
                        .get_node_by_id(*member)
                        .map(|(_, n)| n.url().to_string())
                        .unwrap_or_default();
                    return Some((*member, base, href));
                }
            }
        }
        None
    }

    /// If window point `(x, y)` is over a composited content **card** on the scripted
    /// rung, forward the click to its live document (its listeners run, the DOM may
    /// mutate, the tile re-renders) and report it consumed. The card's scroll is *not*
    /// added: the scripted document owns its scroll internally and the dispatch
    /// re-applies it, so the host passes the viewport-local point. `false` when the
    /// point is over no scripted card. (Render ladder phase 3.)
    #[cfg(feature = "scripted")]
    pub(crate) fn card_scripted_click(&self, x: f32, y: f32) -> bool {
        for (member, r) in &self.view.content_rects {
            if x >= r[0]
                && x <= r[2]
                && y >= r[1]
                && y <= r[3]
                && self.shared.content.constellation.is_scripted(*member)
            {
                self.shared
                    .content
                    .constellation
                    .click_scripted(*member, x - r[0], y - r[1]);
                return true;
            }
        }
        false
    }

    /// The tile counterpart to [`card_scripted_click`](Self::card_scripted_click): a
    /// click on a scripted workbench tile focuses that tile and forwards the click to
    /// its live document. `false` when the point is over no scripted tile. (Phase 3.)
    #[cfg(feature = "scripted")]
    pub(crate) fn tile_scripted_click(&mut self, x: f32, y: f32) -> bool {
        let hit = self.view.tile_rects.iter().find_map(|(member, r)| {
            (x >= r[0]
                && x <= r[2]
                && y >= r[1]
                && y <= r[3]
                && self.shared.content.constellation.is_scripted(*member))
            .then(|| (*member, x - r[0], y - r[1]))
        });
        let Some((member, lx, ly)) = hit else {
            return false;
        };
        self.view.workbench.activate(member);
        self.view.focused_tile = Some(member);
        self.shared
            .content
            .constellation
            .click_scripted(member, lx, ly);
        self.view.request_redraw();
        true
    }

    /// A right-click on a link in a tile or card opens the link context menu — open
    /// in new tab, copy link — instead of navigating in place. Resolves the link and
    /// its source member, stashes them in `context_link`, opens the menu, and returns
    /// whether a link was actually hit. (Browser link flow.)
    pub(crate) fn try_open_link_menu(&mut self, x: f32, y: f32) -> bool {
        let (origin, url) = if let Some((member, base, href)) = self.tile_link_at(x, y) {
            (member, nav::resolve_href(&base, &href))
        } else if let (Some(focused), Some((base, href))) =
            (self.focused_member(), self.card_link_at(x, y))
        {
            (focused, nav::resolve_href(&base, &href))
        } else {
            return false;
        };
        self.view.context_link = Some((origin, url));
        let items = vec![
            ContextItem::new("Open link in new tab", ContextAction::OpenLinkNewTab),
            ContextItem::new("Copy link address", ContextAction::CopyLink),
        ];
        self.view
            .chrome_update(move |c| c.open_context_menu(x, y, items));
        self.view.request_redraw();
        true
    }

    /// Whether `(x, y)` is over a clickable link in a tile or card — drives the hover
    /// (hand) cursor so links are discoverable. (Browser link flow.)
    pub(crate) fn over_link(&self, x: f32, y: f32) -> bool {
        self.tile_link_at(x, y).is_some() || self.card_link_at(x, y).is_some()
    }

    /// Middle / Ctrl-click on a tile link opens it in a new background tab directly
    /// (no menu). Tile-only: a card link uses the right-click menu, and this keeps the
    /// orrery's own middle-drag pan free. Returns whether a tile link was hit.
    /// (Browser link flow.)
    pub(crate) fn try_open_link_new_tab(&mut self, x: f32, y: f32) -> bool {
        if let Some((member, base, href)) = self.tile_link_at(x, y) {
            let url = nav::resolve_href(&base, &href);
            self.open_link_in_new_tab(member, url);
            true
        } else {
            false
        }
    }
}
