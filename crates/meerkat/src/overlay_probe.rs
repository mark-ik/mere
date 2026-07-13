/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Overlay-roots P1 host-half harness: a counter-chip satellite [`ViewPane`] and
//! the end-to-end proof that a host view over app state mounts as an
//! engine-composited overlay on a live page in the content actor and round-trips
//! a click.
//!
//! The plan's P1 done-condition is "a toy overlay view (counter chip anchored to
//! a page node) runs from host app state against a live page in the actor,
//! survives page mutations around it, and round-trips a click"
//! (`mere:design_docs/.../2026-07-05_overlay_roots_and_ua_widgets_plan.md`). This
//! module is the vehicle: `CounterChip` is a real [`ViewPane`] (the reusable
//! host runner) whose view renders its `count` state and increments on click; its
//! [`paint_list`](CounterChip::paint_list) is the composable satellite the host
//! ships via `Constellation::request_set_overlay`. The test wires the full
//! pipeline — host `ViewPane` → `ContentCommand::SetOverlay` → the content
//! actor's `ContentLayout::set_overlay` → the composited band — and round-trips a
//! click through it.
//!
//! Test-only for now (like `list_pane::ListPane`): the live placement of an
//! overlay satellite into a `WindowView` frame (which page node, on-screen click
//! hit-testing) lands with the first real overlay feature (P6 — link preview /
//! annotation pin), which supplies a real anchor and view instead of this toy.

use genet_layout::{ScrollOffsets, GenetPaintList};
use xilem_serval::{AnyView, PointerClick, GenetCtx, GenetElement, clickable, el};

use crate::view_pane::ViewPane;

/// The satellite's app state: a click counter.
#[derive(Default)]
struct CounterState {
    count: u32,
}

type CounterView = Box<dyn AnyView<CounterState, (), GenetCtx, GenetElement>>;
type CounterLogic = fn(&CounterState) -> CounterView;

/// The chip view: a classed `div` showing the live count, clickable to increment.
/// The click handler mutates app state; the runner re-renders + diffs it into the
/// satellite DOM, so the next `paint_list` reflects the new count. (Overlay-roots P1.)
fn counter_view(state: &CounterState) -> CounterView {
    let div =
        el::<_, CounterState, ()>("div", format!("count: {}", state.count)).attr("class", "chip");
    Box::new(clickable(div, |s: &mut CounterState, _: PointerClick| {
        s.count += 1;
    }))
}

/// A counter-chip overlay satellite: a [`ViewPane`] over [`CounterState`] that
/// emits a composable [`GenetPaintList`] for the overlay slot and round-trips
/// clicks host-side.
struct CounterChip {
    pane: ViewPane<CounterState, CounterLogic, CounterView>,
}

impl CounterChip {
    fn new() -> Self {
        let mut pane = ViewPane::new(counter_view as CounterLogic, CounterState::default());
        pane.set_sheets(vec![
            "div { display: block; }".to_string(),
            ".chip { padding: 6px; font-size: 14px; }".to_string(),
        ]);
        Self { pane }
    }

    /// The chip's satellite paint list at `w`×`h` — the overlay-slot content the
    /// host ships to the actor. Lays the chip out (so a subsequent `click` has a
    /// layout to hit-test).
    fn paint_list(&mut self, w: u32, h: u32) -> GenetPaintList {
        self.pane.paint_list(w, h, &ScrollOffsets::default())
    }

    /// Click at chip-local `(x, y)`: hit-test the retained layout and dispatch the
    /// click, firing the chip's `on_click` (which bumps the count). Returns whether
    /// a node was hit. Requires a prior [`paint_list`](Self::paint_list) so the
    /// layout exists to hit-test.
    fn click(&mut self, x: f32, y: f32) -> bool {
        match self.pane.hit_test(x, y, &ScrollOffsets::default()) {
            Some(node) => {
                self.pane.dispatch_click(node, PointerClick::at((x, y)));
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use armillary::{NavGeneration, Pool, ViewportGeneration, Wake};
    use document_canvas::DocumentStyleSheet;
    use netrender::Scene;
    use paint_list_api::{PaintCmd, PaintList};

    use super::*;
    use crate::content::{ContentCommand, ContentUpdate, OverlayAnchor, spawn_content};
    use crate::fetch::{ContentState, Fetched};

    fn noop_wake() -> Wake {
        Arc::new(|| {})
    }

    /// The glyph-index sequence a paint list draws — a precise, deterministic
    /// fingerprint of its rendered text. Two chips whose only difference is the
    /// counter digit produce different sequences (the digit glyph changes), so
    /// this catches the click's effect on the composed satellite.
    fn glyph_indices(pl: &GenetPaintList) -> Vec<String> {
        pl.commands()
            .iter()
            .flat_map(|cmd| match cmd {
                PaintCmd::DrawText(t) => {
                    t.glyphs.iter().map(|g| format!("{:?}", g.index)).collect()
                }
                _ => Vec::new(),
            })
            .collect()
    }

    fn show(url: &str, body: &str) -> ContentCommand {
        ContentCommand::Show {
            url: url.to_string(),
            state: Some(ContentState::Ready(Fetched {
                content_type: Some("text/html".to_string()),
                body: body.to_string(),
            })),
            engine: inker::routing::ENGINE_GENET_WEB.to_string(),
            viewport: (420, 360),
            nav: NavGeneration::default(),
            viewport_gen: ViewportGeneration::default(),
            sheet: DocumentStyleSheet::default(),
        }
    }

    fn set_overlay(content: GenetPaintList) -> ContentCommand {
        ContentCommand::SetOverlay {
            name: "counter".to_string(),
            anchor: OverlayAnchor::Root,
            content,
            viewport_gen: ViewportGeneration::default(),
        }
    }

    /// The overlay-roots P1 done-condition, end to end: a counter-chip satellite
    /// ViewPane over host app state mounts as an engine-composited overlay on a
    /// live page in the content actor, and a click round-trips through the full
    /// pipeline (host runner → SetOverlay → the actor's set_overlay → the band).
    ///
    /// The proof decomposes into the two halves it joins:
    /// - **Round-trip (host):** a click bumps the chip's `count` state, the runner
    ///   re-renders, and the re-emitted satellite paint list draws a *different*
    ///   glyph sequence (the digit changed) — the view responded to app state.
    /// - **Compositing (actor):** shipping the satellite adds paint ops to the
    ///   live page's band with no reflow, and clearing restores the exact baseline
    ///   band. The actor faithfully composites whatever paint list it is handed
    ///   (both the count-0 and count-1 chips), so the round-tripped change reaches
    ///   the composited output.
    #[test]
    fn counter_chip_overlay_round_trips_through_the_actor() {
        // Host half: a counter-chip satellite over app state, count 0.
        let mut chip = CounterChip::new();
        let pl0 = chip.paint_list(160, 32);
        let g0 = glyph_indices(&pl0);
        assert!(!g0.is_empty(), "the chip laid out its count text");

        // Actor half: a live page in a real off-thread content actor.
        let (handle, updates) = spawn_content(
            &Pool::new(),
            noop_wake(),
            std::collections::HashSet::new(),
            false,
        );
        handle.command(show("https://example.com/", "<h1>Hi</h1><p>There</p>"));
        handle.command(set_overlay(pl0));

        // Round-trip a click: bump host-side, re-lay, and re-ship the updated chip.
        assert!(chip.click(8.0, 8.0), "the click hit the chip");
        let pl1 = chip.paint_list(160, 32);
        let g1 = glyph_indices(&pl1);
        assert_ne!(
            g0, g1,
            "the click round-tripped: count 0 -> 1 changed the composed glyphs",
        );
        handle.command(set_overlay(pl1));
        handle.command(ContentCommand::ClearOverlay {
            name: "counter".to_string(),
            viewport_gen: ViewportGeneration::default(),
        });
        handle.join();

        let scenes: Vec<Scene> = updates
            .iter()
            .filter_map(|u| match u {
                ContentUpdate::Scene { scene, .. } => Some(scene),
                _ => None,
            })
            .collect();
        // Show, SetOverlay(0), SetOverlay(1), ClearOverlay each ship a scene.
        assert!(
            scenes.len() >= 4,
            "Show + two SetOverlay + ClearOverlay each emit a scene (got {})",
            scenes.len()
        );
        let base = &scenes[0];
        let with0 = &scenes[1];
        let with1 = &scenes[2];
        let cleared = scenes.last().unwrap();
        assert!(
            with0.ops.len() > base.ops.len(),
            "the count-0 chip composited engine-side over the live page ({} vs {})",
            with0.ops.len(),
            base.ops.len(),
        );
        assert!(
            with1.ops.len() > base.ops.len(),
            "the count-1 chip re-composited after the click ({} vs {})",
            with1.ops.len(),
            base.ops.len(),
        );
        assert_eq!(
            cleared.ops.len(),
            base.ops.len(),
            "clearing the overlay restored the exact baseline band (no residue)",
        );
    }
}
