/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The declarative scene **format**: the data types a [`SceneSpec`] is built from. A scene
//! is a list of [`SceneBodySpec`] (each a [`NodeCollider`] shape, a world spawn, an initial
//! velocity, Fixed-or-Dynamic, a restitution, a per-body gravity scale and rotation), the
//! world settings (gravity, default tangibility, whether it is perpetual), and a list of
//! [`SceneJointSpec`] binding pairs of bodies (hinges, ropes, springs, welds).
//!
//! Pure data, no engine: [`Simulation::load_scene`](crate::Simulation::load_scene) consumes a
//! `SceneSpec`, and [`crate::scenes`] builds ready-made ones. Split out of `lib.rs` because the
//! format grows independently of the simulation core (and `lib.rs` is already at the ceiling).

use crate::NodeCollider;

/// A stable, opaque handle to a scene-decoration body — a non-graph rapier body sharing the
/// orrery's world, distinct from a [`NodeKey`](kernel::graph::NodeKey). Minted by
/// [`Simulation::add_scene_body`](crate::Simulation::add_scene_body). (Physics scenes P1.)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SceneBodyId(pub(crate) u64);

/// Whether a scene body moves under forces / collision (`Dynamic`) or is immovable terrain
/// (`Fixed`) — a floor, a wall, a peg, a joint anchor. (Physics scenes P3.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneBodyType {
    Dynamic,
    Fixed,
}

/// One body in a declarative scene: its shape, world spawn, initial velocity, kind, bounciness,
/// per-body gravity scale, and initial rotation. Data, not code — a [`SceneSpec`] is a list of
/// these. Build with [`SceneBodySpec::dynamic`] / [`SceneBodySpec::fixed`] plus the chaining
/// setters. (Physics scenes P3 / P4b.)
#[derive(Clone, Debug)]
pub struct SceneBodySpec {
    pub collider: NodeCollider,
    pub position: (f32, f32),
    pub velocity: (f32, f32),
    pub body_type: SceneBodyType,
    pub restitution: f32,
    /// Per-body gravity multiplier (default `1.0`). `0.0` floats a body against a gravity scene
    /// (a peg, a buoyant prop); negative rises (an ember, a balloon). (Physics scenes P4b.)
    pub gravity_scale: f32,
    /// Initial orientation in radians (default `0.0`). Places a leaning card / angled ramp / tilted
    /// prop without needing a rotated-cuboid collider. (Physics scenes P4b.)
    pub rotation: f32,
    /// An optional **sprite handle**: an opaque key the host resolves to a texture to paint over
    /// this prop (a crate, a barrel) instead of the flat orb / polygon. `None` (the default) keeps
    /// the abstract paint. gyre never interprets it — it rides through to the
    /// [`SceneBodyView`](crate::SceneBodyView) for the host, exactly as the collider shape does (the
    /// gyre layer stays rendering-agnostic). (Physics scenes — scene-prop sprites.)
    pub sprite: Option<String>,
}

impl SceneBodySpec {
    /// A dynamic body at `position` with sensible defaults (still, restitution 0.3, full gravity,
    /// no rotation). Chain the setters to override.
    pub fn dynamic(collider: NodeCollider, position: (f32, f32)) -> Self {
        Self {
            collider,
            position,
            velocity: (0.0, 0.0),
            body_type: SceneBodyType::Dynamic,
            restitution: 0.3,
            gravity_scale: 1.0,
            rotation: 0.0,
            sprite: None,
        }
    }

    /// A fixed (immovable) body at `position` — a floor, wall, peg, or joint anchor.
    pub fn fixed(collider: NodeCollider, position: (f32, f32)) -> Self {
        Self {
            body_type: SceneBodyType::Fixed,
            ..Self::dynamic(collider, position)
        }
    }

    /// Set the initial velocity (px/s).
    pub fn velocity(mut self, velocity: (f32, f32)) -> Self {
        self.velocity = velocity;
        self
    }

    /// Set the restitution (bounciness, `0.0`-`1.0`).
    pub fn restitution(mut self, restitution: f32) -> Self {
        self.restitution = restitution;
        self
    }

    /// Set the per-body gravity scale (`1.0` full, `0.0` floats, negative rises).
    pub fn gravity_scale(mut self, gravity_scale: f32) -> Self {
        self.gravity_scale = gravity_scale;
        self
    }

    /// Set the initial rotation (radians).
    pub fn rotation(mut self, rotation: f32) -> Self {
        self.rotation = rotation;
        self
    }

    /// Set the sprite handle the host paints over this prop (an opaque texture key; `None` keeps
    /// the abstract orb / polygon). (Physics scenes — scene-prop sprites.)
    pub fn sprite(mut self, handle: impl Into<String>) -> Self {
        self.sprite = Some(handle.into());
        self
    }
}

/// The joint families [`Simulation::load_scene`](crate::Simulation::load_scene) can build between
/// two scene bodies, mirroring rapier's set. Anchors are body-local attach points in world units
/// (the body's own frame); use a [`SceneBodyType::Fixed`] body as a world anchor. (Physics scenes
/// P4b.)
#[derive(Clone, Copy, Debug)]
pub enum SceneJoint {
    /// Rigidly weld the two bodies' anchor points together (no relative motion).
    Fixed {
        anchor_a: (f32, f32),
        anchor_b: (f32, f32),
    },
    /// A hinge: the two anchor points coincide and the bodies rotate freely about that pivot. An
    /// optional motor drives the hinge (a powered drum, a wheel).
    Revolute {
        anchor_a: (f32, f32),
        anchor_b: (f32, f32),
        motor: Option<JointMotorSpec>,
    },
    /// A rope: the anchor points stay at most `length` apart (a chain link, a string, a tether).
    Rope {
        anchor_a: (f32, f32),
        anchor_b: (f32, f32),
        length: f32,
    },
    /// A spring along the line between the anchors, settling toward `rest_length`.
    Spring {
        anchor_a: (f32, f32),
        anchor_b: (f32, f32),
        rest_length: f32,
        stiffness: f32,
        damping: f32,
    },
}

/// A motor on a [`SceneJoint::Revolute`]: drive toward `target_vel` (rad/s) with the given motor
/// `factor` (a stiffness/strength). (Physics scenes P4b.)
#[derive(Clone, Copy, Debug)]
pub struct JointMotorSpec {
    pub target_vel: f32,
    pub factor: f32,
}

/// A constraint binding two scene bodies, referenced by their index in [`SceneSpec::bodies`]
/// (spawn order). (Physics scenes P4b.)
#[derive(Clone, Copy, Debug)]
pub struct SceneJointSpec {
    pub body_a: usize,
    pub body_b: usize,
    pub joint: SceneJoint,
}

/// A continuous force-field a scene applies to its dynamic bodies — the "environment has character
/// beyond ballistic gravity" tier (a whirlpool, a gravity well). Set on the
/// [`Simulation`](crate::Simulation) via `set_scene_field`; it keeps the physics actor ticking
/// while active. (Physics scenes P4 — force-field tier.)
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SceneField {
    /// A whirlpool centred at `center`: each dynamic scene body feels a tangential force of
    /// magnitude `strength` (the swirl, counter-clockwise) plus an inward pull `inward` toward the
    /// centre, so loose props orbit and slowly spiral in.
    Vortex {
        center: (f32, f32),
        strength: f32,
        inward: f32,
    },
}

/// A continuous body spawner: emits dynamic scene bodies over time (a fountain, rain, a sand
/// stream), reaping each after `lifetime_secs` so the live count stays bounded. The "scene is alive
/// over time" capability beyond one-shot placement; keeps the actor ticking while present. Added via
/// `add_emitter`. (Physics scenes — emitters.)
#[derive(Clone, Debug)]
pub struct SceneEmitter {
    pub collider: NodeCollider,
    pub position: (f32, f32),
    /// +/- deterministic random spread on the spawn position.
    pub position_jitter: (f32, f32),
    pub velocity: (f32, f32),
    /// +/- deterministic random spread on the spawn velocity.
    pub velocity_jitter: (f32, f32),
    /// Bodies emitted per second.
    pub rate_per_sec: f32,
    /// Seconds before an emitted body is reaped.
    pub lifetime_secs: f32,
    /// Cap on this emitter's live bodies (a per-emitter budget atop the global scene-body cap).
    pub max_alive: usize,
}

/// A declarative physics scene the orrery's world can load: a set of bodies, the world gravity,
/// whether the graph collides with it by default, whether it is a perpetual (never-settling)
/// backdrop, and the joints binding its bodies. Loading one
/// ([`Simulation::load_scene`](crate::Simulation::load_scene)) clears any prior scene. (Physics
/// scenes P3 / P4b.)
#[derive(Clone, Debug)]
pub struct SceneSpec {
    pub bodies: Vec<SceneBodySpec>,
    /// World gravity (px/s^2; `+y` is down at Mere's screen scale). `(0, 0)` for a gravity-free
    /// scene (a drifting backdrop, a bouncing box).
    pub gravity: (f32, f32),
    /// Whether the graph collides with the scene on load (the tangibility default).
    pub default_tangible: bool,
    /// Whether the scene is meant to stay in motion forever (a drifting / bouncing living backdrop)
    /// rather than settle to rest. When set, the physics actor keeps ticking instead of parking
    /// once at rest, and the scene's dynamic bodies take a near-zero damping so their motion coasts.
    pub perpetual: bool,
    /// Constraints binding pairs of bodies (by index into `bodies`): hinges, ropes, springs, welds.
    /// Empty for a jointless scene (a pile, a drift). (Physics scenes P4b.)
    pub joints: Vec<SceneJointSpec>,
}
