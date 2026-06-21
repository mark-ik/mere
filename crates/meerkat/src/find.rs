/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Host-side find-in-page over the HTML/serval lane, dispatched to the find worker.
//!
//! The match search is a full serval layout (~1-2s on a large page), so it runs on the
//! [find worker](crate::find_worker) thread, not the UI loop. The host ships the focused
//! page's body + query (stamped with a generation), the worker lays out and replies, and
//! [`apply_find_result`] folds the latest generation's rects into the window view for the
//! overlay. Because the worker searches the cached body directly, find works whatever the
//! node's activation state — a snapshot card with no actor searches the same as a live one.

use forme::GraphMemberId;

use super::WindowCtx;
use super::fetch::ContentState;
use crate::find_worker::{FindCommand, FindResult};

impl WindowCtx<'_> {
    /// Ship the focused page + `query` to the find worker (stamped with a fresh
    /// generation). The reply arrives on `inbox.find` and lands via [`apply_find_result`].
    /// An empty query (or no focused HTML page) clears the highlights immediately and
    /// bumps the generation so any in-flight reply is dropped. (Find-in-page.)
    pub(super) fn recompute_find(&mut self, query: &str) {
        self.view.find_gen = self.view.find_gen.wrapping_add(1);
        if query.is_empty() {
            self.clear_find();
            return;
        }
        let Some(member) = self.focused_member() else {
            self.clear_find();
            return;
        };
        let Some(url) = self.orrery().focused_url().map(str::to_string) else {
            self.clear_find();
            return;
        };
        // The focused page's Ready content: the live fetch if present, else the durable
        // cache (so a snapshot-only node still searches). Extract the worker's inputs.
        // Bind the live-fetch lookup first so its borrow ends before the cache fallback.
        let from_pages = self.shared.content.pages.get(&url).and_then(|s| match s {
            ContentState::Ready(f) => Some((f.content_type.clone(), f.body.clone())),
            _ => None,
        });
        let ready = from_pages.or_else(|| {
            self.load_cached(&url)
                .map(|s| (s.content_type, String::from_utf8_lossy(&s.body).into_owned()))
        });
        let Some((content_type, body)) = ready else {
            self.clear_find();
            return;
        };
        // Lay out at the rendered card's texture width (the overlay maps content->window
        // by dividing the dest by this width), and the visible card height.
        let dest = self
            .view
            .content_rects
            .iter()
            .chain(self.view.tile_rects.iter())
            .find(|(m, _)| *m == member)
            .map(|(_, r)| *r);
        let w = self
            .view
            .tile_textures
            .get(&member)
            .map(|c| c.size.0)
            .or_else(|| dest.map(|r| (r[2] - r[0]).round().max(1.0) as u32))
            .unwrap_or(300);
        let h = dest
            .map(|r| (r[3] - r[1]).round().max(1.0) as u32)
            .unwrap_or(800);

        self.shared.content.find_worker.command(FindCommand::Find {
            generation: self.view.find_gen,
            member,
            url,
            content_type,
            body,
            w,
            h,
            query: query.to_string(),
        });
    }

    /// Apply a find worker reply: keep the rects only if they are for the latest query
    /// (generation match), and feed any wanted subresources back from the durable cache
    /// so the next query lays out styled. Returns whether a redraw is warranted.
    /// (Find-in-page.)
    pub(super) fn apply_find_result(&mut self, result: FindResult) -> bool {
        // Always satisfy the worker's subresource wants (cheap; helps future queries),
        // even for a stale generation.
        for url in &result.wanted {
            if let Some(stored) = self.load_cached(url) {
                self.shared.content.find_worker.command(FindCommand::Resource {
                    url: url.clone(),
                    bytes: stored.body,
                });
            }
        }
        if result.generation != self.view.find_gen {
            return false; // a later query superseded this one
        }
        self.view.find_matches = result.matches;
        self.view.find_member = Some(result.member);
        true
    }

    /// Drop any find highlights (query cleared or the bar closed).
    pub(super) fn clear_find(&mut self) {
        self.view.find_matches.clear();
        self.view.find_member = None;
    }

    /// The focused member's find match rects, if they belong to `member`. The overlay
    /// draws only when the composited member owns the current matches.
    pub(super) fn find_matches_for(&self, member: GraphMemberId) -> &[Vec<[f32; 4]>] {
        if self.view.find_member == Some(member) {
            &self.view.find_matches
        } else {
            &[]
        }
    }
}
