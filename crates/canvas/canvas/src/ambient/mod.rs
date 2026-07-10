/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Ambient-sim backdrops: small standalone simulations painted behind the graph for liveliness the
//! rapier solver should not carry (the "ambient separate-sim backdrop" tier, physics scenes P5).
//! Non-rapier and host-side (cheap, no actor offload), advanced and painted each frame as the bottom
//! backdrop layer. They share the [`AmbientSim`] seam (advance + paint), so the canvas holds one
//! `Box<dyn AmbientSim>` and the catalog grows without touching the host. Each is painted in a
//! [`Tincture`] - a base colour it interprets as it likes.
//!
//! One sim per file: [`GameOfLife`] (a CA), [`NBody`] (a Keplerian galaxy drift), [`ParticleLife`]
//! (asymmetric-attraction species), [`SandFall`] (a gravity CA). The shared trait, the [`Tincture`]
//! type, and the tiny PRNG + HSV helpers live here; the sims reach them via `super::`.

use paint_list_api::{ColorF, PaintCmd};

mod game_of_life;
mod nbody;
mod particle_life;
mod sand;

pub use game_of_life::GameOfLife;
pub use nbody::NBody;
pub use particle_life::ParticleLife;
pub use sand::SandFall;

/// The base colour an ambient sim is painted in - a tint / wash. One colour the sim interprets as it
/// sees fit (Game of Life tints its cells; particle-life rotates the hue per species). Overridable
/// per backdrop via [`Canvas::set_ambient_tincture`](crate::Canvas::set_ambient_tincture). (P5.)
pub type Tincture = ColorF;

/// A non-rapier ambient backdrop simulation: advance it by `dt`, then paint it across the viewport in
/// a [`Tincture`]. The canvas holds one as `Box<dyn AmbientSim>`, advancing + painting it each frame
/// as the bottom layer and keep-redrawing while it is live. `Send` so a backdrop could later move to
/// an actor if one grows expensive. (Physics scenes P5.)
pub trait AmbientSim: Send {
    /// Advance the sim by `dt` seconds. Discrete sims (Game of Life) accumulate `dt` and step at
    /// their own cadence; continuous sims (n-body) integrate by `dt`.
    fn advance(&mut self, dt: f32);
    /// Paint the sim across a `w` x `h` px viewport, tinted by `tincture`, as backdrop paint commands.
    fn paint(&self, w: f32, h: f32, tincture: Tincture) -> Vec<PaintCmd>;
    /// The sim's natural tincture (its default base colour), used unless the host overrides it.
    fn default_tincture(&self) -> Tincture;
}

/// The hue (in `[0, 1)`) of an RGB colour; grey returns 0. (For rotating a tincture into a palette.)
fn rgb_to_hue(c: ColorF) -> f32 {
    let (r, g, b) = (c.r, c.g, c.b);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    if d <= 1e-6 {
        return 0.0;
    }
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0).rem_euclid(1.0)
}

/// An RGBA colour from hue `h` in `[0, 1)`, saturation `s`, value `v`, alpha `a`.
fn hsv_to_rgb(h: f32, s: f32, v: f32, a: f32) -> ColorF {
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let (r, g, b) = match (i as i32).rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    ColorF::new(r, g, b, a)
}

/// A deterministic `[0, 1)` sample from the xorshift PRNG.
fn unit(state: &mut u32) -> f32 {
    xorshift(state) as f32 / u32::MAX as f32
}

/// xorshift32 - a tiny deterministic PRNG (no `rand` dep, reproducible headless).
fn xorshift(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_round_trips_the_hue() {
        // A pure-green tincture should rotate cleanly: hue 1/3, recoverable from the built colour.
        let green = hsv_to_rgb(1.0 / 3.0, 0.8, 0.9, 1.0);
        assert!(
            (rgb_to_hue(green) - 1.0 / 3.0).abs() < 0.02,
            "hue round-trips"
        );
    }
}
