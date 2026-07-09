/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Session-chip thumbnail painter (ui_polish S1).
//!
//! `session_runtime::switcher_thumbnail` produces the typed mini-graph geometry
//! (fit-to-bounds node dots + edge segments); this module rasterizes it into a
//! small RGBA buffer and wraps it as a `data:image/png;base64,` URI for the
//! chrome-DOM chip `<img>` — the same texture-into-chrome pattern the snapshot
//! card and favicons use. Painted on session/graph change beside the label
//! refresh (`refresh_session_labels`), never per frame.

use session_runtime::SwitcherThumbnail;

/// Raster size, physical px. The chip CSS shows the image at half this in
/// logical px, so a 2x panel gets a crisp backing texture.
pub(crate) const THUMB_W: u32 = 60;
pub(crate) const THUMB_H: u32 = 36;

/// Paint `thumb` over `bg` with `edge`-coloured segments and `node`-coloured
/// dots, and wrap as a PNG data URI. Colors are straight-alpha RGBA8, sourced
/// from the chrome theme at the call site so thumbnails re-theme with it.
pub(crate) fn thumb_data_uri(
    thumb: &SwitcherThumbnail,
    bg: [u8; 4],
    edge: [u8; 4],
    node: [u8; 4],
) -> Option<String> {
    let (w, h) = (thumb.width, thumb.height);
    if w == 0 || h == 0 {
        return None;
    }
    let mut buf = vec![0u8; (w * h * 4) as usize];
    for px in buf.chunks_exact_mut(4) {
        px.copy_from_slice(&bg);
    }
    for e in &thumb.edges {
        draw_segment(&mut buf, w, h, (e.from.x, e.from.y), (e.to.x, e.to.y), edge);
    }
    for n in &thumb.nodes {
        draw_dot(&mut buf, w, h, (n.position.x, n.position.y), n.radius, node);
    }
    let png = crate::render::textures::png_bytes_from_rgba(&buf, w, h)?;
    crate::render::textures::png_data_uri(&png)
}

fn put(buf: &mut [u8], w: u32, h: u32, x: i32, y: i32, c: [u8; 4]) {
    if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
        return;
    }
    let i = ((y as u32 * w + x as u32) * 4) as usize;
    buf[i..i + 4].copy_from_slice(&c);
}

/// 1px segment by sampling along the longer axis (no AA; at thumbnail scale a
/// hard line reads fine and keeps this dependency-free).
fn draw_segment(buf: &mut [u8], w: u32, h: u32, from: (f32, f32), to: (f32, f32), c: [u8; 4]) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let steps = dx.abs().max(dy.abs()).ceil().max(1.0) as u32;
    for s in 0..=steps {
        let t = s as f32 / steps as f32;
        put(
            buf,
            w,
            h,
            (from.0 + dx * t).round() as i32,
            (from.1 + dy * t).round() as i32,
            c,
        );
    }
}

fn draw_dot(buf: &mut [u8], w: u32, h: u32, at: (f32, f32), radius: f32, c: [u8; 4]) {
    let r = radius.max(1.0);
    let (cx, cy) = (at.0, at.1);
    let (x0, x1) = ((cx - r).floor() as i32, (cx + r).ceil() as i32);
    let (y0, y1) = ((cy - r).floor() as i32, (cy + r).ceil() as i32);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let (fx, fy) = (x as f32 - cx, y as f32 - cy);
            if fx * fx + fy * fy <= r * r {
                put(buf, w, h, x, y, c);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use session_runtime::{ThumbnailEdge, ThumbnailNode};

    fn pt(x: f32, y: f32) -> mere::kernel::geometry::PortablePoint {
        mere::kernel::geometry::PortablePoint::new(x, y)
    }

    #[test]
    fn paints_nodes_and_edges_into_a_data_uri() {
        let thumb = SwitcherThumbnail {
            width: THUMB_W,
            height: THUMB_H,
            nodes: vec![
                ThumbnailNode {
                    position: pt(10.0, 10.0),
                    radius: 3.0,
                },
                ThumbnailNode {
                    position: pt(50.0, 26.0),
                    radius: 3.0,
                },
            ],
            edges: vec![ThumbnailEdge {
                from: pt(10.0, 10.0),
                to: pt(50.0, 26.0),
                family_tag: 0,
            }],
        };
        let uri = thumb_data_uri(
            &thumb,
            [20, 24, 20, 255],
            [90, 110, 90, 255],
            [230, 235, 230, 255],
        )
        .expect("paints");
        assert!(uri.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn empty_thumbnail_still_produces_a_uri() {
        let thumb = SwitcherThumbnail {
            width: THUMB_W,
            height: THUMB_H,
            nodes: vec![],
            edges: vec![],
        };
        assert!(
            thumb_data_uri(
                &thumb,
                [20, 24, 20, 255],
                [90, 110, 90, 255],
                [230, 235, 230, 255]
            )
            .is_some()
        );
    }
}
