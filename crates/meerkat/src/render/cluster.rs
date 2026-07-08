/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The chrome status cluster: a live chisel `Meter` (frame time) + a
//! recent-visited-trail `GraphGlyph`, rendered to a small overlay scene and
//! composited top-right. A **host overlay**, not a spliced `<chisel-leaf>` in
//! the chrome DOM, so the per-frame meter never dirties the cached chrome base
//! (a live leaf's data lives outside the DOM, which the base-staleness check
//! can't see — the overlay sidesteps that entirely). The glyph tints its nodes
//! from the same NODE_SHEET state palette every node representation uses, so the
//! trail carries node identity. (Chisel toolbar cluster; Representations carry
//! node identity.)

use chisel::{ColorF, GraphGlyph, GraphGlyphNode, Leaf, Meter, PaintCx, Size};
use orrery::NodeState;
use paint_list_api::specs::{TransformKind, TransformSpec};
use paint_list_api::{DeviceIntSize, LayoutPoint, LayoutTransform, PaintCmd};

const METER_KEY: u64 = 1;
const GLYPH_KEY: u64 = 2;
const METER_W: f32 = 60.0;
const METER_H: f32 = 8.0;
const GLYPH_W: f32 = 22.0;
const GLYPH_H: f32 = 22.0;
const GAP: f32 = 8.0;
const PAD: f32 = 6.0;
/// Full meter bar = two missed 60hz frames.
const FRAME_FULL_MS: f32 = 33.3;

/// The NODE_SHEET activation palette, as `ColorF` (mirrors `accent_rgb` in
/// `gloss_outline_view`): open green / closed red / idle blue.
fn state_color(state: NodeState) -> ColorF {
    match state {
        NodeState::Open => ColorF { r: 0.227, g: 0.549, b: 0.369, a: 1.0 },
        NodeState::Closed => ColorF { r: 0.651, g: 0.282, b: 0.282, a: 1.0 },
        NodeState::Idle => ColorF { r: 0.212, g: 0.361, b: 0.612, a: 1.0 },
    }
}

impl crate::WindowCtx<'_> {
    /// Build the status cluster's overlay scene and its top-right rect. Feeds the
    /// meter last frame's wall time and the glyph the recent-visited trail
    /// (re-fed, and `node_states` requeried, only when that trail changed).
    pub(super) fn status_cluster_scene(&mut self, win_w: u32) -> (netrender::Scene, [f32; 4]) {
        let cluster_w = PAD + METER_W + GAP + GLYPH_W + PAD;
        let cluster_h = PAD + GLYPH_H + PAD;

        // Ensure the two leaves exist (once).
        if self.view.chrome_cluster.get_mut_as::<Meter>(&METER_KEY).is_none() {
            let mut m = Meter::new(false, Size { width: METER_W, height: METER_H });
            m.fill_color = ColorF { r: 0.45, g: 0.75, b: 0.95, a: 1.0 };
            m.track_color = ColorF { r: 0.16, g: 0.16, b: 0.20, a: 1.0 };
            self.view.chrome_cluster.insert(METER_KEY, Box::new(m));
        }
        if self.view.chrome_cluster.get_mut_as::<GraphGlyph>(&GLYPH_KEY).is_none() {
            let g = GraphGlyph::new(Vec::new(), Vec::new(), Size { width: GLYPH_W, height: GLYPH_H });
            self.view.chrome_cluster.insert(GLYPH_KEY, Box::new(g));
        }

        // Meter: last frame's wall time, with a slowly-decaying rolling peak.
        let v = (self.view.last_frame_us / 1000.0 / FRAME_FULL_MS).clamp(0.0, 1.0);
        self.view.cluster_frame_peak = (self.view.cluster_frame_peak - 0.01).max(v).clamp(0.0, 1.0);
        let peak = self.view.cluster_frame_peak;
        if let Some(m) = self.view.chrome_cluster.get_mut_as::<Meter>(&METER_KEY) {
            m.set_level(v, Some(peak));
        }

        // Glyph: the recent-visited trail, re-fed only when it changed.
        let recent = self.orrery().graph().recent_visited(8);
        let sig = {
            use std::hash::{Hash, Hasher};
            let mut h = rustc_hash::FxHasher::default();
            for rv in &recent {
                rv.node.hash(&mut h);
            }
            h.finish()
        };
        if sig != self.view.cluster_recent_sig {
            let states = self.node_states();
            let n = recent.len();
            let nodes: Vec<GraphGlyphNode> = recent
                .iter()
                .enumerate()
                .map(|(i, rv)| {
                    let st = states.get(&rv.node).copied().unwrap_or(NodeState::Idle);
                    GraphGlyphNode {
                        x: if n > 1 { i as f32 / (n - 1) as f32 } else { 0.5 },
                        // A gentle zigzag so a straight trail still reads as a graph.
                        y: if i % 2 == 0 { 0.32 } else { 0.68 },
                        color: state_color(st),
                    }
                })
                .collect();
            let edges: Vec<(u16, u16)> =
                (0..n.saturating_sub(1)).map(|i| (i as u16, i as u16 + 1)).collect();
            if let Some(g) = self.view.chrome_cluster.get_mut_as::<GraphGlyph>(&GLYPH_KEY) {
                g.set_graph(nodes, edges);
            }
            self.view.cluster_recent_sig = sig;
        }

        // Render both leaves into one command buffer, each offset to its slot
        // and vertically centered in the cluster.
        let mut cmds: Vec<PaintCmd> = Vec::new();
        for (key, x, sz) in [
            (METER_KEY, PAD, Size { width: METER_W, height: METER_H }),
            (GLYPH_KEY, PAD + METER_W + GAP, Size { width: GLYPH_W, height: GLYPH_H }),
        ] {
            let oy = (cluster_h - sz.height) / 2.0;
            cmds.push(PaintCmd::PushTransform(TransformSpec {
                origin: LayoutPoint::new(x, oy),
                transform: LayoutTransform::identity(),
                kind: TransformKind::Standard,
            }));
            if let Some(leaf) = self.view.chrome_cluster.get_mut(&key) {
                let mut cx = PaintCx::new(&mut cmds, sz);
                leaf.paint(&mut cx);
            }
            cmds.push(PaintCmd::PopTransform);
        }

        let scene = paint_list_render::translate_paint_cmd_stream(
            DeviceIntSize::new(cluster_w.ceil() as i32, cluster_h.ceil() as i32),
            &cmds,
            &[],
            &[],
        )
        .scene;
        let x1 = win_w as f32 - 8.0;
        let x0 = x1 - cluster_w;
        (scene, [x0, 6.0, x1, 6.0 + cluster_h])
    }
}
