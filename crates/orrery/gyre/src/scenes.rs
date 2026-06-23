/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The declarative scene library: ready-made [`SceneSpec`]s the orrery can load
//! behind the graph. Each is a plain `fn -> SceneSpec` (data, not engine code) in
//! the spirit of porting the open-source 2D-physics demo galleries (matter.js /
//! planck.js / box2d) to Mere's body vocabulary — we reproduce the *mechanic*, not
//! anyone's code. Everything here is expressible with today's [`SceneSpec`]: a flat
//! list of bodies (a [`NodeCollider`] shape, a world position, an initial velocity,
//! Fixed or Dynamic, a restitution), one global gravity vector, a default
//! tangibility, and a `perpetual` flag for backdrops that never settle.
//!
//! Two flavours: *settling* scenes (a pile, a topple, a pour) that come to rest so
//! the physics actor parks, and *perpetual* scenes (a drift) that keep moving — the
//! actor keeps ticking and their bodies take a near-zero damping so the motion
//! coasts. Slopes and curves are faked with staircases of fixed balls (there is no
//! rotated cuboid yet); tall props use an axis-aligned [`NodeCollider::Hull`] rect.
//! (Physics scenes P4a.)

use crate::{NodeCollider, SceneBodySpec, SceneBodyType, SceneSpec};

/// A transplanted demo scene: a bumpy fixed floor (a row of big fixed balls) with a
/// handful of dynamic balls falling onto it under gravity and piling up. The original
/// "interactive scene" proof — data-defined, and the graph can knock the balls around
/// once made tangible. (Physics scenes P3.)
pub fn drop_bowl_scene() -> SceneSpec {
    let mut bodies = Vec::new();
    // The bumpy fixed floor.
    for i in 0..5 {
        bodies.push(SceneBodySpec {
            collider: NodeCollider::Ball { radius: 60.0 },
            position: (-280.0 + i as f32 * 140.0, 340.0),
            velocity: (0.0, 0.0),
            body_type: SceneBodyType::Fixed,
            restitution: 0.4,
        });
    }
    // Dynamic balls dropped from above the graph.
    for i in 0..8 {
        bodies.push(SceneBodySpec {
            collider: NodeCollider::Ball { radius: 22.0 },
            position: (-210.0 + i as f32 * 60.0, -260.0 - (i % 3) as f32 * 40.0),
            velocity: (0.0, 0.0),
            body_type: SceneBodyType::Dynamic,
            restitution: 0.5,
        });
    }
    SceneSpec { bodies, gravity: (0.0, 520.0), default_tangible: false, perpetual: false }
}

/// A stacked pyramid of dynamic blocks resting on a fixed slab — stable and
/// architectural at rest, the headline knock-over once the graph is made tangible and
/// plows through. (matter.js `pyramid` / rapier examples2d `Pyramid`.)
pub fn pyramid_scene() -> SceneSpec {
    let mut bodies = vec![SceneBodySpec {
        collider: NodeCollider::Square { half: 300.0 },
        position: (0.0, 360.0),
        velocity: (0.0, 0.0),
        body_type: SceneBodyType::Fixed,
        restitution: 0.0,
    }];
    // 9 rows, row k has (9 - k) blocks, climbing upward (decreasing y).
    for k in 0..9 {
        let count = 9 - k;
        for i in 0..count {
            let x = (i as f32 - (count as f32 - 1.0) / 2.0) * 50.0;
            bodies.push(SceneBodySpec {
                collider: NodeCollider::Square { half: 24.0 },
                // Row 0 sits on the floor's top face (y=60: the half-300 slab spans [60,660]),
                // climbing upward. friction (0.3, set in load_scene) + zero restitution = it
                // holds until pushed.
                position: (x, 36.0 - k as f32 * 48.0),
                velocity: (0.0, 0.0),
                body_type: SceneBodyType::Dynamic,
                restitution: 0.0,
            });
        }
    }
    SceneSpec { bodies, gravity: (0.0, 600.0), default_tangible: false, perpetual: false }
}

/// A row of tall thin dynamic blocks on a fixed floor; the first is nudged sideways on
/// load to topple into its neighbour, cascading down the line. The most legible
/// cause-effect toy. (planck.js `Dominos` / box2d.) A clean *tipping* cascade really
/// wants per-body rotation (a P4b scene-spec extension); today the staircase tips by
/// sliding contact, which reads as a topple but is spacing-sensitive.
pub fn domino_scene() -> SceneSpec {
    let mut bodies = vec![SceneBodySpec {
        collider: NodeCollider::Square { half: 300.0 },
        position: (0.0, 360.0),
        velocity: (0.0, 0.0),
        body_type: SceneBodyType::Fixed,
        restitution: 0.0,
    }];
    // Tall thin domino via an axis-aligned Hull rect (Mere has no rotated cuboid).
    let tall = NodeCollider::Hull {
        points: vec![(-6.0, -44.0), (6.0, -44.0), (6.0, 44.0), (-6.0, 44.0)],
        fallback: 20.0,
    };
    for i in 0..14 {
        bodies.push(SceneBodySpec {
            collider: tall.clone(),
            position: (-260.0 + i as f32 * 40.0, 12.0),
            // The lead domino gets a shove to start the cascade on load.
            velocity: if i == 0 { (90.0, 0.0) } else { (0.0, 0.0) },
            body_type: SceneBodyType::Dynamic,
            restitution: 0.0,
        });
    }
    SceneSpec { bodies, gravity: (0.0, 520.0), default_tangible: false, perpetual: false }
}

/// A Galton board / Plinko: a triangular lattice of fixed pegs scatters a slow drip of
/// dynamic balls into bins below — a bell curve forming live behind the graph, and a
/// tangible graph deflects the cascade. (matter.js / classic box2d.)
pub fn galton_scene() -> SceneSpec {
    let mut bodies = Vec::new();
    // Triangular peg lattice (72 pegs).
    for row in 0..9 {
        let n = 4 + row;
        for i in 0..n {
            let x = (i as f32 - (n as f32 - 1.0) / 2.0) * 60.0;
            bodies.push(SceneBodySpec {
                collider: NodeCollider::Ball { radius: 8.0 },
                position: (x, -200.0 + row as f32 * 45.0),
                velocity: (0.0, 0.0),
                body_type: SceneBodyType::Fixed,
                restitution: 0.5,
            });
        }
    }
    // Catch floor below the pegs (the half-320 slab's top face sits at y=200, clear of the
    // lowest pegs at y=160). The scatter piles into a bell-shaped heap on it — the heap is
    // the live distribution (explicit bin dividers want a tall thin collider we don't have yet).
    bodies.push(SceneBodySpec {
        collider: NodeCollider::Square { half: 320.0 },
        position: (0.0, 520.0),
        velocity: (0.0, 0.0),
        body_type: SceneBodyType::Fixed,
        restitution: 0.2,
    });
    // A slow drip down the center (a true continuous stream wants the emitter extension).
    for i in 0..30 {
        bodies.push(SceneBodySpec {
            collider: NodeCollider::Ball { radius: 6.0 },
            position: ((i % 5) as f32 * 4.0 - 8.0, -320.0 - i as f32 * 12.0),
            velocity: (0.0, 30.0),
            body_type: SceneBodyType::Dynamic,
            restitution: 0.5,
        });
    }
    SceneSpec { bodies, gravity: (0.0, 450.0), default_tangible: false, perpetual: false }
}

/// A funnel / hourglass: two converging fixed-ball chutes (a staircase fakes the
/// slope, no rotation needed) feed a stream of dynamic grains through the throat to a
/// catch floor below — a slow, mesmeric pour behind the graph.
pub fn funnel_scene() -> SceneSpec {
    let mut bodies = Vec::new();
    // Two angled chutes as staircases of fixed balls converging toward the throat.
    for i in 0..8 {
        let inset = i as f32 * 30.0;
        let y = -200.0 + i as f32 * 28.0;
        bodies.push(SceneBodySpec {
            collider: NodeCollider::Ball { radius: 30.0 },
            position: (-360.0 + inset, y),
            velocity: (0.0, 0.0),
            body_type: SceneBodyType::Fixed,
            restitution: 0.1,
        });
        bodies.push(SceneBodySpec {
            collider: NodeCollider::Ball { radius: 30.0 },
            position: (360.0 - inset, y),
            velocity: (0.0, 0.0),
            body_type: SceneBodyType::Fixed,
            restitution: 0.1,
        });
    }
    // Catch floor below the throat.
    bodies.push(SceneBodySpec {
        collider: NodeCollider::Square { half: 200.0 },
        position: (0.0, 320.0),
        velocity: (0.0, 0.0),
        body_type: SceneBodyType::Fixed,
        restitution: 0.1,
    });
    // A column of grains trickling through.
    for i in 0..80 {
        bodies.push(SceneBodySpec {
            collider: NodeCollider::Ball { radius: 8.0 },
            position: (((i % 7) as f32 - 3.0) * 16.0, -340.0 - (i / 7) as f32 * 18.0),
            velocity: (0.0, 20.0),
            body_type: SceneBodyType::Dynamic,
            restitution: 0.2,
        });
    }
    SceneSpec { bodies, gravity: (0.0, 500.0), default_tangible: false, perpetual: false }
}

/// A gravity-free drift: a loose cluster of soft orbs coasting and bouncing off each
/// other in lazy lava-lamp motion — the calmest living backdrop, intangible by default
/// so the graph floats serenely over it. `perpetual` so the actor keeps ticking and
/// the near-zero perpetual damping lets the drift coast instead of bleeding to rest.
/// (Known P4a limitation: with no walls the orbs slowly disperse over a minute or two;
/// a gentle centering force is the proper fix and is a P4b force-field item.)
pub fn drift_scene() -> SceneSpec {
    // Spread wide with moderate radii so the orbs drift and bounce freely rather than
    // jamming into a static clump of sustained overlaps (which no restitution can undo).
    let seeds = [
        (-360.0, -200.0, 38.0, -22.0),
        (340.0, 170.0, -30.0, 26.0),
        (-300.0, 200.0, 28.0, -30.0),
        (320.0, -210.0, -34.0, 20.0),
        (-40.0, -40.0, 30.0, 34.0),
        (-200.0, 90.0, 36.0, -18.0),
        (220.0, 30.0, -28.0, -32.0),
        (60.0, 240.0, 22.0, -38.0),
    ];
    let bodies = seeds
        .iter()
        .enumerate()
        .map(|(i, &(x, y, vx, vy))| SceneBodySpec {
            collider: NodeCollider::Ball { radius: 26.0 + (i % 4) as f32 * 7.0 },
            position: (x, y),
            velocity: (vx, vy),
            body_type: SceneBodyType::Dynamic,
            // Elastic: mutual bounces conserve energy, so a near-zero-damping drift keeps
            // milling instead of bleeding to rest.
            restitution: 1.0,
        })
        .collect();
    SceneSpec { bodies, gravity: (0.0, 0.0), default_tangible: false, perpetual: true }
}

#[cfg(test)]
mod tests {
    use euclid::default::Point2D;
    use kernel::graph::NodeKey;

    use super::*;
    use crate::Simulation;

    /// Every catalog scene loads its bodies (under the cap) without disturbing a node,
    /// and reports the perpetual flag the spec declared.
    #[test]
    fn catalog_scenes_load_and_report_perpetual() {
        let cases: [(SceneSpec, bool); 6] = [
            (drop_bowl_scene(), false),
            (pyramid_scene(), false),
            (domino_scene(), false),
            (galton_scene(), false),
            (funnel_scene(), false),
            (drift_scene(), true),
        ];
        for (spec, perpetual) in cases {
            let want = spec.bodies.len();
            let mut sim = Simulation::new();
            let node = NodeKey::new(0);
            sim.sync_nodes([(node, Point2D::new(0.0, 0.0))]);
            sim.load_scene(&spec);
            assert!(want <= 200, "scene must stay under SCENE_BODY_CAP (was {want})");
            assert_eq!(sim.scene_body_count(), want, "every spec body became a scene body");
            assert_eq!(sim.body_count(), 1, "the node is not counted as a scene body");
            assert_eq!(sim.scene_perpetual(), perpetual, "scene reports its perpetual flag");
        }
    }

    /// A settling scene comes to rest (its dynamic bodies stop moving); a perpetual
    /// scene keeps drifting. Position deltas over a window stand in for "still moving".
    #[test]
    fn perpetual_drifts_while_settling_scene_comes_to_rest() {
        // Pyramid: settles. Sample positions, run, then confirm a later window is near-still.
        let mut settle = Simulation::new();
        settle.load_scene(&pyramid_scene());
        for _ in 0..600 {
            settle.tick(1.0 / 60.0);
        }
        let a: Vec<_> = settle.scene_bodies().map(|(_, p, _)| p).collect();
        for _ in 0..30 {
            settle.tick(1.0 / 60.0);
        }
        let b: Vec<_> = settle.scene_bodies().map(|(_, p, _)| p).collect();
        let settled_motion: f32 = a.iter().zip(&b).map(|(p, q)| (*p - *q).length()).sum();
        assert!(settled_motion < 5.0, "pyramid should be at rest (moved {settled_motion})");

        // Drift: still moving after the same long run (perpetual + near-zero damping).
        let mut drift = Simulation::new();
        drift.load_scene(&drift_scene());
        for _ in 0..600 {
            drift.tick(1.0 / 60.0);
        }
        let a: Vec<_> = drift.scene_bodies().map(|(_, p, _)| p).collect();
        for _ in 0..30 {
            drift.tick(1.0 / 60.0);
        }
        let b: Vec<_> = drift.scene_bodies().map(|(_, p, _)| p).collect();
        let drift_motion: f32 = a.iter().zip(&b).map(|(p, q)| (*p - *q).length()).sum();
        assert!(drift_motion > 5.0, "drift orbs should still be moving (moved {drift_motion})");
    }
}
