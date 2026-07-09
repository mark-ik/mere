/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The declarative scene library: ready-made [`SceneSpec`]s the orrery can load behind the
//! graph. Each is a plain `fn -> SceneSpec` (data, not engine code) in the spirit of porting
//! the open-source 2D-physics demo galleries (matter.js / planck.js / box2d) to Mere's body
//! vocabulary — we reproduce the *mechanic*, not anyone's code. Bodies are built with the
//! [`SceneBodySpec::dynamic`] / [`SceneBodySpec::fixed`] constructors plus chaining setters.
//!
//! Two flavours: *settling* scenes (a pile, a topple, a pour, a swinging chain) that come to
//! rest so the physics actor parks, and *perpetual* scenes (a drift) that keep moving — the
//! actor keeps ticking and their bodies take a near-zero damping so the motion coasts. Slopes
//! and curves are faked with staircases of fixed balls (there is no rotated cuboid yet); tall
//! props use an axis-aligned [`NodeCollider::Hull`] rect. Joints (P4b) bind bodies into chains,
//! cradles, and bridges. (Physics scenes P4a / P4b.)

use crate::{JointMotorSpec, NodeCollider, SceneBodySpec, SceneJoint, SceneJointSpec, SceneSpec};

/// A transplanted demo scene: a bumpy fixed floor (a row of big fixed balls) with a handful of
/// dynamic balls falling onto it under gravity and piling up. The original "interactive scene"
/// proof — data-defined, and the graph can knock the balls around once made tangible. (P3.)
pub fn drop_bowl_scene() -> SceneSpec {
    let mut bodies = Vec::new();
    // The bumpy fixed floor.
    for i in 0..5 {
        bodies.push(
            SceneBodySpec::fixed(
                NodeCollider::Ball { radius: 60.0 },
                (-280.0 + i as f32 * 140.0, 340.0),
            )
            .restitution(0.4),
        );
    }
    // Dynamic balls dropped from above the graph.
    for i in 0..8 {
        bodies.push(
            SceneBodySpec::dynamic(
                NodeCollider::Ball { radius: 22.0 },
                (-210.0 + i as f32 * 60.0, -260.0 - (i % 3) as f32 * 40.0),
            )
            .restitution(0.5),
        );
    }
    SceneSpec {
        bodies,
        gravity: (0.0, 520.0),
        default_tangible: false,
        perpetual: false,
        joints: Vec::new(),
    }
}

/// A stacked pyramid of dynamic blocks resting on a fixed slab — stable and architectural at
/// rest, the headline knock-over once the graph is made tangible and plows through. (matter.js
/// `pyramid` / rapier examples2d `Pyramid`.)
pub fn pyramid_scene() -> SceneSpec {
    let mut bodies = vec![
        SceneBodySpec::fixed(NodeCollider::Square { half: 300.0 }, (0.0, 360.0)).restitution(0.0),
    ];
    // 9 rows, row k has (9 - k) blocks, climbing upward (decreasing y). Row 0 sits on the floor's
    // top face (y=60: the half-300 slab spans [60,660]). Friction (0.3) + zero restitution = it holds.
    for k in 0..9 {
        let count = 9 - k;
        for i in 0..count {
            let x = (i as f32 - (count as f32 - 1.0) / 2.0) * 50.0;
            bodies.push(
                SceneBodySpec::dynamic(
                    NodeCollider::Square { half: 24.0 },
                    (x, 36.0 - k as f32 * 48.0),
                )
                .restitution(0.0),
            );
        }
    }
    SceneSpec {
        bodies,
        gravity: (0.0, 600.0),
        default_tangible: false,
        perpetual: false,
        joints: Vec::new(),
    }
}

/// A row of tall thin dynamic blocks on a fixed floor; the first is nudged sideways on load to
/// topple into its neighbour, cascading down the line. (planck.js `Dominos` / box2d.) A clean
/// *tipping* cascade really wants per-body rotation tuning (P4b); today it tips by sliding contact.
pub fn domino_scene() -> SceneSpec {
    let mut bodies = vec![
        SceneBodySpec::fixed(NodeCollider::Square { half: 300.0 }, (0.0, 360.0)).restitution(0.0),
    ];
    // Tall thin domino via an axis-aligned Hull rect (Mere has no rotated cuboid).
    let tall = NodeCollider::Hull {
        points: vec![(-6.0, -44.0), (6.0, -44.0), (6.0, 44.0), (-6.0, 44.0)],
        fallback: 20.0,
    };
    for i in 0..14 {
        let vel = if i == 0 { (90.0, 0.0) } else { (0.0, 0.0) };
        bodies.push(
            SceneBodySpec::dynamic(tall.clone(), (-260.0 + i as f32 * 40.0, 12.0))
                .velocity(vel)
                .restitution(0.0),
        );
    }
    SceneSpec {
        bodies,
        gravity: (0.0, 520.0),
        default_tangible: false,
        perpetual: false,
        joints: Vec::new(),
    }
}

/// A Galton board / Plinko: a triangular lattice of fixed pegs scatters a slow drip of dynamic
/// balls into a bell-shaped heap on the floor below — a distribution forming live behind the
/// graph, and a tangible graph deflects the cascade. (matter.js / classic box2d.)
pub fn galton_scene() -> SceneSpec {
    let mut bodies = Vec::new();
    // Triangular peg lattice (72 pegs).
    for row in 0..9 {
        let n = 4 + row;
        for i in 0..n {
            let x = (i as f32 - (n as f32 - 1.0) / 2.0) * 60.0;
            bodies.push(
                SceneBodySpec::fixed(
                    NodeCollider::Ball { radius: 8.0 },
                    (x, -200.0 + row as f32 * 45.0),
                )
                .restitution(0.5),
            );
        }
    }
    // Catch floor below the pegs (the half-320 slab's top face at y=200 clears the lowest pegs at
    // y=160). The scatter piles into a bell-shaped heap (the heap is the live distribution).
    bodies.push(
        SceneBodySpec::fixed(NodeCollider::Square { half: 320.0 }, (0.0, 520.0)).restitution(0.2),
    );
    // A slow drip down the center (a true continuous stream wants the emitter extension).
    for i in 0..30 {
        bodies.push(
            SceneBodySpec::dynamic(
                NodeCollider::Ball { radius: 6.0 },
                ((i % 5) as f32 * 4.0 - 8.0, -320.0 - i as f32 * 12.0),
            )
            .velocity((0.0, 30.0))
            .restitution(0.5),
        );
    }
    SceneSpec {
        bodies,
        gravity: (0.0, 450.0),
        default_tangible: false,
        perpetual: false,
        joints: Vec::new(),
    }
}

/// A funnel / hourglass: two converging fixed-ball chutes (a staircase fakes the slope, no
/// rotation needed) feed a stream of dynamic grains through the throat to a catch floor below —
/// a slow, mesmeric pour behind the graph.
pub fn funnel_scene() -> SceneSpec {
    let mut bodies = Vec::new();
    // Two angled chutes as staircases of fixed balls converging toward the throat.
    for i in 0..8 {
        let inset = i as f32 * 30.0;
        let y = -200.0 + i as f32 * 28.0;
        bodies.push(
            SceneBodySpec::fixed(NodeCollider::Ball { radius: 30.0 }, (-360.0 + inset, y))
                .restitution(0.1),
        );
        bodies.push(
            SceneBodySpec::fixed(NodeCollider::Ball { radius: 30.0 }, (360.0 - inset, y))
                .restitution(0.1),
        );
    }
    // Catch floor below the throat.
    bodies.push(
        SceneBodySpec::fixed(NodeCollider::Square { half: 200.0 }, (0.0, 320.0)).restitution(0.1),
    );
    // A column of grains trickling through.
    for i in 0..80 {
        bodies.push(
            SceneBodySpec::dynamic(
                NodeCollider::Ball { radius: 8.0 },
                (
                    ((i % 7) as f32 - 3.0) * 16.0,
                    -340.0 - (i / 7) as f32 * 18.0,
                ),
            )
            .velocity((0.0, 20.0))
            .restitution(0.2),
        );
    }
    SceneSpec {
        bodies,
        gravity: (0.0, 500.0),
        default_tangible: false,
        perpetual: false,
        joints: Vec::new(),
    }
}

/// A gravity-free drift: a loose cluster of soft orbs coasting and bouncing off each other in lazy
/// lava-lamp motion — the calmest living backdrop, intangible by default so the graph floats over
/// it. `perpetual` so the actor keeps ticking and the near-zero perpetual damping lets the drift
/// coast. (Known P4a limitation: with no walls the orbs slowly disperse over a minute or two; a
/// gentle centering force is the proper fix and is a P4b force-field item.)
pub fn drift_scene() -> SceneSpec {
    // Spread wide with moderate radii so the orbs drift and bounce freely rather than jamming into
    // a static clump of sustained overlaps (which no restitution can undo).
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
        .map(|(i, &(x, y, vx, vy))| {
            // Elastic (restitution 1.0): mutual bounces conserve energy, so a near-zero-damping
            // drift keeps milling instead of bleeding to rest.
            SceneBodySpec::dynamic(
                NodeCollider::Ball {
                    radius: 26.0 + (i % 4) as f32 * 7.0,
                },
                (x, y),
            )
            .velocity((vx, vy))
            .restitution(1.0)
        })
        .collect();
    SceneSpec {
        bodies,
        gravity: (0.0, 0.0),
        default_tangible: false,
        perpetual: true,
        joints: Vec::new(),
    }
}

/// A rope chain hung from a fixed anchor: eight dynamic link-balls start stretched out horizontally
/// and fall under gravity, the rope joints holding the links together so the whole chain swings
/// down around the anchor like a pendulum. The joints proof (planck.js `Chain` / rapier rope_joints),
/// and a tangible graph node can bump the chain. (Physics scenes P4b.)
pub fn chain_scene() -> SceneSpec {
    let anchor = (-150.0, -120.0);
    let link = 48.0;
    // Body 0 is the fixed anchor; bodies 1..=8 are the dynamic links, laid out horizontally to the
    // right of the anchor (so gravity swings the whole chain down).
    let mut bodies = vec![SceneBodySpec::fixed(
        NodeCollider::Ball { radius: 10.0 },
        anchor,
    )];
    for i in 0..8 {
        bodies.push(
            SceneBodySpec::dynamic(
                NodeCollider::Ball { radius: 16.0 },
                (anchor.0 + (i as f32 + 1.0) * link, anchor.1),
            )
            .restitution(0.2),
        );
    }
    // Rope links: anchor(0) -> link(1) -> link(2) -> ... -> link(8), each capped at `link` apart.
    let joints = (0..8)
        .map(|i| SceneJointSpec {
            body_a: i,
            body_b: i + 1,
            joint: SceneJoint::Rope {
                anchor_a: (0.0, 0.0),
                anchor_b: (0.0, 0.0),
                length: link,
            },
        })
        .collect();
    SceneSpec {
        bodies,
        gravity: (0.0, 520.0),
        default_tangible: false,
        perpetual: false,
        joints,
    }
}

/// A whirlpool: two rings of loose gravity-free balls that a [`SceneField::Vortex`] swirls into an
/// orbiting, slowly-inspiralling seiche. The host pairs this scene (the bodies) with `set_scene_field`
/// (the environment); the field keeps the actor ticking. (Physics scenes P4 — force-field tier.)
///
/// [`SceneField::Vortex`]: crate::SceneField::Vortex
pub fn whirlpool_scene() -> SceneSpec {
    use std::f32::consts::TAU;
    let mut bodies = Vec::new();
    for i in 0..20 {
        let (ring, twist) = if i < 10 { (110.0, 0.0) } else { (190.0, 0.31) };
        let a = (i % 10) as f32 * TAU / 10.0 + twist;
        bodies.push(
            SceneBodySpec::dynamic(
                NodeCollider::Ball {
                    radius: 14.0 + (i % 3) as f32 * 6.0,
                },
                (ring * a.cos(), ring * a.sin()),
            )
            .restitution(0.6),
        );
    }
    SceneSpec {
        bodies,
        gravity: (0.0, 0.0),
        default_tangible: false,
        perpetual: false,
        joints: Vec::new(),
    }
}

/// A fountain catch-basin: a wide fixed floor under gravity. The host pairs this with an upward
/// `SceneEmitter` (added via `add_emitter`) whose droplets spray up, arc over, and rain back onto the
/// floor before ageing out — a perpetual jet. The scene carries the basin; the emitter is the jet.
/// (Physics scenes — emitters.)
pub fn fountain_scene() -> SceneSpec {
    let bodies = vec![
        // Top face at y=300 (the half-300 slab spans [300, 900]); droplets land here and pile.
        SceneBodySpec::fixed(NodeCollider::Square { half: 300.0 }, (0.0, 600.0)).restitution(0.1),
    ];
    SceneSpec {
        bodies,
        gravity: (0.0, 520.0),
        default_tangible: false,
        perpetual: false,
        joints: Vec::new(),
    }
}

/// A Newton's cradle: five elastic balls, each hung from its own fixed anchor by a rigid revolute
/// rod so they swing on arcs and rest just touching in a row. The leftmost is launched outward, so
/// it swings back and strikes the row, and the momentum carries through the still balls to kick the
/// far one out: the classic click-clack. `perpetual` (near-zero damping, restitution 0.95) so the
/// transfer keeps going as a backdrop rather than damping out in a few swings. (rapier `joints2`
/// family / the canonical demo; reproduced via revolute pendulums.)
pub fn cradle_scene() -> SceneSpec {
    let rod = 170.0;
    let anchor_y = -220.0;
    let r = 22.0;
    // Bodies 0..5 are the fixed anchors (a row, 2r apart so the hung balls just touch); bodies 5..10
    // are the dynamic balls hanging `rod` below each anchor. The leftmost ball gets an outward shove.
    let mut bodies = Vec::new();
    for i in 0..5 {
        let x = (i as f32 - 2.0) * (2.0 * r);
        bodies.push(SceneBodySpec::fixed(
            NodeCollider::Ball { radius: 8.0 },
            (x, anchor_y),
        ));
    }
    for i in 0..5 {
        let x = (i as f32 - 2.0) * (2.0 * r);
        let vel = if i == 0 { (-340.0, 0.0) } else { (0.0, 0.0) };
        bodies.push(
            SceneBodySpec::dynamic(NodeCollider::Ball { radius: r }, (x, anchor_y + rod))
                .velocity(vel)
                .restitution(0.95),
        );
    }
    // A rigid revolute rod from each anchor (i) to its ball (5 + i): the anchors coincide and the
    // ball's `(0, -rod)` point pins to the anchor, so the ball swings on a fixed-length arc.
    let joints = (0..5)
        .map(|i| SceneJointSpec {
            body_a: i,
            body_b: 5 + i,
            joint: SceneJoint::Revolute {
                anchor_a: (0.0, 0.0),
                anchor_b: (0.0, -rod),
                motor: None,
            },
        })
        .collect();
    SceneSpec {
        bodies,
        gravity: (0.0, 600.0),
        default_tangible: false,
        perpetual: true,
        joints,
    }
}

/// A suspended plank bridge: nine plank bodies hinged end to end by free revolute joints and pinned
/// at both ends to fixed posts, with a couple of heavy balls dropped on the span so it bows into a
/// deep catenary sag and rebounds. The hinge chain makes it a true rope bridge (floppy), the posts
/// hold the ends. (planck.js `Bridge` / rapier examples; reproduced via a revolute-hinge chain.)
pub fn bridge_scene() -> SceneSpec {
    let halfw = 33.0;
    // Body 0 is the left post, bodies 1..=9 the nine planks, body 10 the right post (all in a row at
    // y=0); bodies 11, 12 are the weights dropped on the deck.
    let mut bodies = vec![SceneBodySpec::fixed(
        NodeCollider::Ball { radius: 10.0 },
        (-297.0, 0.0),
    )];
    let plank = NodeCollider::Hull {
        points: vec![(-halfw, -6.0), (halfw, -6.0), (halfw, 6.0), (-halfw, 6.0)],
        fallback: 20.0,
    };
    for i in 0..9 {
        bodies.push(
            SceneBodySpec::dynamic(plank.clone(), (-264.0 + i as f32 * 66.0, 0.0))
                .restitution(0.05),
        );
    }
    bodies.push(SceneBodySpec::fixed(
        NodeCollider::Ball { radius: 10.0 },
        (297.0, 0.0),
    ));
    bodies.push(
        SceneBodySpec::dynamic(NodeCollider::Ball { radius: 28.0 }, (0.0, -170.0)).restitution(0.1),
    );
    bodies.push(
        SceneBodySpec::dynamic(NodeCollider::Ball { radius: 22.0 }, (90.0, -230.0))
            .restitution(0.1),
    );
    // Hinge the chain: left post -> plank0 -> ... -> plank8 -> right post. Interior hinges join a
    // plank's right edge (+halfw) to the next plank's left edge (-halfw); the post hinges use the
    // post centre.
    let mut joints = vec![SceneJointSpec {
        body_a: 0,
        body_b: 1,
        joint: SceneJoint::Revolute {
            anchor_a: (0.0, 0.0),
            anchor_b: (-halfw, 0.0),
            motor: None,
        },
    }];
    for i in 1..9 {
        joints.push(SceneJointSpec {
            body_a: i,
            body_b: i + 1,
            joint: SceneJoint::Revolute {
                anchor_a: (halfw, 0.0),
                anchor_b: (-halfw, 0.0),
                motor: None,
            },
        });
    }
    joints.push(SceneJointSpec {
        body_a: 9,
        body_b: 10,
        joint: SceneJoint::Revolute {
            anchor_a: (halfw, 0.0),
            anchor_b: (0.0, 0.0),
            motor: None,
        },
    });
    SceneSpec {
        bodies,
        gravity: (0.0, 500.0),
        default_tangible: false,
        perpetual: false,
        joints,
    }
}

/// A wrecking ball: a heavy ball on a five-link rope chain, anchored up and to the right and laid out
/// taut to the left, so on release it swings down through the bottom of its arc and ploughs a small
/// block tower scattering across the floor. The rope joints (slack-tolerant) make the swung-out start
/// trivially valid; the heavy ball's mass (a bigger radius is more mass at the shared density) carries
/// the wallop. (A heavy-ended variant of [`chain_scene`] with a target to demolish.)
pub fn ball_and_chain_scene() -> SceneSpec {
    let anchor = (140.0, -260.0);
    let link = 44.0;
    // Body 0 the fixed anchor; bodies 1..=4 the light links and body 5 the heavy ball, laid out
    // horizontally to the left of the anchor (taut, so gravity swings the whole thing down-right).
    let mut bodies = vec![SceneBodySpec::fixed(
        NodeCollider::Ball { radius: 10.0 },
        anchor,
    )];
    for i in 0..4 {
        bodies.push(SceneBodySpec::dynamic(
            NodeCollider::Ball { radius: 12.0 },
            (anchor.0 - (i as f32 + 1.0) * link, anchor.1),
        ));
    }
    bodies.push(
        SceneBodySpec::dynamic(
            NodeCollider::Ball { radius: 34.0 },
            (anchor.0 - 5.0 * link, anchor.1),
        )
        .restitution(0.2),
    );
    // The target: a fixed floor (top face y=0) and a five-block tower at the bottom of the ball's
    // arc (x=140), which the swinging ball smashes through.
    bodies.push(
        SceneBodySpec::fixed(NodeCollider::Square { half: 300.0 }, (0.0, 300.0)).restitution(0.1),
    );
    for k in 0..5 {
        bodies.push(
            SceneBodySpec::dynamic(
                NodeCollider::Square { half: 16.0 },
                (140.0, -16.0 - k as f32 * 32.0),
            )
            .restitution(0.0),
        );
    }
    // Rope links: anchor(0) -> link(1) -> ... -> link(4) -> heavy(5), each capped at `link` apart.
    let joints = (0..5)
        .map(|i| SceneJointSpec {
            body_a: i,
            body_b: i + 1,
            joint: SceneJoint::Rope {
                anchor_a: (0.0, 0.0),
                anchor_b: (0.0, 0.0),
                length: link,
            },
        })
        .collect();
    SceneSpec {
        bodies,
        gravity: (0.0, 540.0),
        default_tangible: false,
        perpetual: false,
        joints,
    }
}

/// A powered mixer: a long paddle bar pinned at its centre to a fixed hub by a motorised revolute
/// joint, spinning steadily inside a circular wall of fixed balls and flinging a scatter of loose
/// balls around the bowl. Gravity-free and `perpetual` so the motor stirs endlessly as a backdrop.
/// The one scene that exercises the revolute *motor* (a powered drum / wheel). (rapier `joints2`
/// motor demo, reproduced.)
pub fn mixer_scene() -> SceneSpec {
    use std::f32::consts::TAU;
    let arm = 150.0;
    // Body 0 the fixed hub (a small disc at the centre); body 1 the dynamic paddle bar centred on it.
    let mut bodies = vec![SceneBodySpec::fixed(
        NodeCollider::Ball { radius: 6.0 },
        (0.0, 0.0),
    )];
    bodies.push(
        SceneBodySpec::dynamic(
            NodeCollider::Hull {
                points: vec![(-arm, -8.0), (arm, -8.0), (arm, 8.0), (-arm, 8.0)],
                fallback: 20.0,
            },
            (0.0, 0.0),
        )
        .restitution(0.4),
    );
    // The containing wall: a ring of fixed balls the flung loose balls bounce off.
    for i in 0..18 {
        let a = i as f32 * TAU / 18.0;
        bodies.push(
            SceneBodySpec::fixed(
                NodeCollider::Ball { radius: 20.0 },
                (250.0 * a.cos(), 250.0 * a.sin()),
            )
            .restitution(0.5),
        );
    }
    // Loose balls in the annulus between the paddle's reach and the wall, for the paddle to stir.
    for i in 0..10 {
        let a = i as f32 * TAU / 10.0 + 0.3;
        bodies.push(
            SceneBodySpec::dynamic(
                NodeCollider::Ball {
                    radius: 14.0 + (i % 3) as f32 * 5.0,
                },
                (200.0 * a.cos(), 200.0 * a.sin()),
            )
            .restitution(0.6),
        );
    }
    // The motor spins the paddle (body 1) about the hub (body 0) at a steady rate.
    let joints = vec![SceneJointSpec {
        body_a: 0,
        body_b: 1,
        joint: SceneJoint::Revolute {
            anchor_a: (0.0, 0.0),
            anchor_b: (0.0, 0.0),
            motor: Some(JointMotorSpec {
                target_vel: 2.5,
                factor: 30.0,
            }),
        },
    }];
    SceneSpec {
        bodies,
        gravity: (0.0, 0.0),
        default_tangible: false,
        perpetual: true,
        joints,
    }
}

#[cfg(test)]
mod tests {
    use euclid::default::Point2D;
    use kernel::graph::NodeKey;

    use super::*;
    use crate::Simulation;

    /// Every catalog scene loads its bodies (under the cap) without disturbing a node, and reports
    /// the perpetual flag the spec declared.
    #[test]
    fn catalog_scenes_load_and_report_perpetual() {
        let cases: [(SceneSpec, bool); 11] = [
            (drop_bowl_scene(), false),
            (pyramid_scene(), false),
            (domino_scene(), false),
            (galton_scene(), false),
            (funnel_scene(), false),
            (drift_scene(), true),
            (chain_scene(), false),
            (cradle_scene(), true),
            (bridge_scene(), false),
            (ball_and_chain_scene(), false),
            (mixer_scene(), true),
        ];
        for (spec, perpetual) in cases {
            let want = spec.bodies.len();
            let mut sim = Simulation::new();
            let node = NodeKey::new(0);
            sim.sync_nodes([(node, Point2D::new(0.0, 0.0))]);
            sim.load_scene(&spec);
            assert!(
                want <= 200,
                "scene must stay under SCENE_BODY_CAP (was {want})"
            );
            assert_eq!(
                sim.scene_body_count(),
                want,
                "every spec body became a scene body"
            );
            assert_eq!(
                sim.body_count(),
                1,
                "the node is not counted as a scene body"
            );
            assert_eq!(
                sim.scene_perpetual(),
                perpetual,
                "scene reports its perpetual flag"
            );
        }
    }

    /// A settling scene comes to rest (its dynamic bodies stop moving); a perpetual scene keeps
    /// drifting. Position deltas over a window stand in for "still moving".
    #[test]
    fn perpetual_drifts_while_settling_scene_comes_to_rest() {
        // Pyramid: settles. Sample positions, run, then confirm a later window is near-still.
        let mut settle = Simulation::new();
        settle.load_scene(&pyramid_scene());
        for _ in 0..600 {
            settle.tick(1.0 / 60.0);
        }
        let a: Vec<_> = settle.scene_bodies().map(|b| b.position).collect();
        for _ in 0..30 {
            settle.tick(1.0 / 60.0);
        }
        let b: Vec<_> = settle.scene_bodies().map(|b| b.position).collect();
        let settled_motion: f32 = a.iter().zip(&b).map(|(p, q)| (*p - *q).length()).sum();
        assert!(
            settled_motion < 5.0,
            "pyramid should be at rest (moved {settled_motion})"
        );

        // Drift: still moving after the same long run (perpetual + near-zero damping).
        let mut drift = Simulation::new();
        drift.load_scene(&drift_scene());
        for _ in 0..600 {
            drift.tick(1.0 / 60.0);
        }
        let a: Vec<_> = drift.scene_bodies().map(|b| b.position).collect();
        for _ in 0..30 {
            drift.tick(1.0 / 60.0);
        }
        let b: Vec<_> = drift.scene_bodies().map(|b| b.position).collect();
        let drift_motion: f32 = a.iter().zip(&b).map(|(p, q)| (*p - *q).length()).sum();
        assert!(
            drift_motion > 5.0,
            "drift orbs should still be moving (moved {drift_motion})"
        );
    }

    /// The rope joints anchor the chain: instead of the eight links falling away forever under
    /// gravity, they hang ~chain-length below the fixed anchor. (Without joints the links would be
    /// thousands of px down after this run.)
    #[test]
    fn chain_joints_anchor_the_links() {
        let mut sim = Simulation::new();
        sim.load_scene(&chain_scene());
        for _ in 0..300 {
            sim.tick(1.0 / 60.0);
        }
        let max_y = sim
            .scene_bodies()
            .map(|b| b.position.y)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            max_y < 500.0,
            "rope joints should anchor the chain, not let it fall away (max_y {max_y})"
        );
        // ...and it hangs below the anchor (it swung down from the horizontal start).
        assert!(
            max_y > 0.0,
            "the chain should hang below the anchor (max_y {max_y})"
        );
    }

    /// The mixer's revolute motor actually turns the paddle: its rotation advances over a run
    /// (proving the motor drives it, not just gravity / contacts). `scene_bodies` walks a HashMap so
    /// it is unordered; the paddle is the scene's only Hull body, so pick it by shape.
    #[test]
    fn mixer_motor_spins_the_paddle() {
        let mut sim = Simulation::new();
        sim.load_scene(&mixer_scene());
        let paddle_rot = |sim: &Simulation| {
            sim.scene_bodies()
                .find(|b| matches!(b.collider, crate::NodeCollider::Hull { .. }))
                .map(|b| b.rotation)
                .expect("the mixer has a paddle")
        };
        let start = paddle_rot(&sim);
        for _ in 0..120 {
            sim.tick(1.0 / 60.0);
        }
        let after = paddle_rot(&sim);
        assert!(
            (after - start).abs() > 0.5,
            "the motor should have rotated the paddle (start {start}, after {after})"
        );
    }

    /// The bridge bows under load: the planks are the Hull bodies, and the loaded span sags so the
    /// lowest plank ends up well below the deck line (y=0) it started on, while the end posts keep it
    /// from dropping away. (A jointless plank row would just fall.)
    #[test]
    fn bridge_sags_under_the_dropped_weight() {
        let mut sim = Simulation::new();
        sim.load_scene(&bridge_scene());
        for _ in 0..240 {
            sim.tick(1.0 / 60.0);
        }
        let lowest_plank_y = sim
            .scene_bodies()
            .filter(|b| matches!(b.collider, crate::NodeCollider::Hull { .. }))
            .map(|b| b.position.y)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            lowest_plank_y > 20.0,
            "the loaded bridge should sag below the deck (was {lowest_plank_y})"
        );
        assert!(
            lowest_plank_y < 600.0,
            "the posts should still hold the span, not drop it (was {lowest_plank_y})"
        );
    }
}
