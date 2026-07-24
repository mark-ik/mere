//! The rigid **scene** tier on [`Simulation`]: non-graph scene-decoration bodies, declarative
//! [`SceneSpec`] loading (per-body gravity / rotation + joints), world gravity, and the per-node
//! tangibility lever (whether the graph collides with the scene). The scene *catalog* lives in
//! [`crate::scenes`] and its *format* in [`crate::scene_spec`]; this is the simulation side. Split
//! from `lib.rs` to keep the simulation core under the per-file size ceiling. (Physics scenes.)

use euclid::default::Point2D;
use crate::NodeKey;
use rapier2d::prelude::*;

use crate::{
    DEFAULT_ANGULAR_DAMPING, NODE_BODY_DENSITY, NODE_GROUP, SCENE_BODY_CAP, SCENE_GROUP,
    SceneBodyId, SceneBodyType, SceneBodyView, SceneJoint, SceneSpec, Simulation, scene_groups,
};

/// Linear damping for scene-decoration bodies — low, so a drifting backdrop body
/// coasts a long while rather than settling quickly. (Physics scenes P1.)
const SCENE_DAMPING: f32 = 0.3;

/// Linear damping for the dynamic bodies of a *perpetual* scene (a drifting backdrop):
/// near-zero so the motion coasts for minutes rather than bleeding to rest under the
/// heavier [`SCENE_DAMPING`] that lets a settling scene come to rest. (Physics scenes P4a.)
const PERPETUAL_SCENE_DAMPING: f32 = 0.01;

impl Simulation {
    /// Add a non-graph **scene body** to this world — a decoration / interactive-scene
    /// element that ticks and collides like any body but carries no [`NodeKey`], so the
    /// graph's layout forces ignore it and (by the default collision groups) nodes pass
    /// through it. `collider` is its shape (the node shape vocabulary, reused),
    /// `position` its world spawn, `velocity` an initial drift in px/s. Returns its
    /// [`SceneBodyId`]. (Physics scenes P1.)
    pub fn add_scene_body(
        &mut self,
        collider: crate::NodeCollider,
        position: Point2D<f32>,
        velocity: (f32, f32),
    ) -> SceneBodyId {
        let body = RigidBodyBuilder::dynamic()
            .translation(Vector::new(position.x, position.y))
            .linvel(Vector::new(velocity.0, velocity.1))
            .linear_damping(SCENE_DAMPING)
            .angular_damping(DEFAULT_ANGULAR_DAMPING)
            .build();
        let handle = self.bodies.insert(body);
        let shape = collider.to_shared_shape();
        let c = ColliderBuilder::new(shape)
            .density(NODE_BODY_DENSITY)
            .restitution(0.6)
            .friction(0.0)
            .collision_groups(scene_groups())
            .build();
        self.colliders
            .insert_with_parent(c, handle, &mut self.bodies);
        // Keep the collider shape alongside the handle for the host's shape-aware paint. (P4b.)
        let id = SceneBodyId(self.next_scene_id);
        self.next_scene_id += 1;
        self.scene_bodies.insert(id, (handle, collider));
        id
    }

    /// Remove a scene body. A no-op for an unknown id. (Physics scenes P1.)
    pub fn remove_scene_body(&mut self, id: SceneBodyId) {
        if let Some((handle, _)) = self.scene_bodies.remove(&id) {
            self.scene_sprites.remove(&id);
            self.bodies.remove(
                handle,
                &mut self.islands,
                &mut self.colliders,
                &mut self.impulse_joints,
                &mut self.multibody_joints,
                /* remove_attached_colliders */ true,
            );
        }
    }

    /// Remove every scene body, leaving the graph untouched. (Physics scenes P1.)
    pub fn clear_scene(&mut self) {
        let handles: Vec<RigidBodyHandle> = self.scene_bodies.values().map(|(h, _)| *h).collect();
        for handle in handles {
            self.bodies.remove(
                handle,
                &mut self.islands,
                &mut self.colliders,
                &mut self.impulse_joints,
                &mut self.multibody_joints,
                true,
            );
        }
        self.scene_bodies.clear();
        self.scene_sprites.clear();
        self.scene_perpetual = false;
        self.scene_field = None;
        self.emitters.clear();
    }

    /// Iterate every scene body as a paintable [`SceneBodyView`] (id, position, rotation, shape)
    /// — the host's read for the shape-aware backdrop paint. Reflects the last [`Self::tick`];
    /// order is unspecified. (Physics scenes P1 / P4b.)
    pub fn scene_bodies(&self) -> impl Iterator<Item = SceneBodyView> + '_ {
        self.scene_bodies
            .iter()
            .filter_map(|(&id, (handle, collider))| {
                self.bodies.get(*handle).map(|b| {
                    let t = b.translation();
                    SceneBodyView {
                        id,
                        position: Point2D::new(t.x, t.y),
                        rotation: b.rotation().angle(),
                        collider: collider.clone(),
                        sprite: self.scene_sprites.get(&id).cloned(),
                    }
                })
            })
    }

    /// Number of scene bodies in the world. (Physics scenes P1.)
    pub fn scene_body_count(&self) -> usize {
        self.scene_bodies.len()
    }

    /// Whether the loaded scene wants to keep moving forever (a perpetual backdrop) rather
    /// than settle. The physics actor reads this to keep ticking instead of parking at rest;
    /// `false` once the scene is cleared. (Physics scenes P4a.)
    pub fn scene_perpetual(&self) -> bool {
        self.scene_perpetual
    }

    /// Set the world gravity (px/s^2). Node bodies carry `gravity_scale(0)`, so only
    /// scene bodies fall; the graph layout is unaffected. (Physics scenes P3.)
    pub fn set_gravity(&mut self, gravity: (f32, f32)) {
        self.gravity = Vector::new(gravity.0, gravity.1);
    }

    /// Load a declarative [`SceneSpec`] into the world: clear any prior scene, set its
    /// gravity, spawn its bodies (capped at [`SCENE_BODY_CAP`]) with their per-body gravity
    /// scale + initial rotation, bind its joints, and apply its default tangibility. (Physics
    /// scenes P3 / P4b.)
    pub fn load_scene(&mut self, spec: &SceneSpec) {
        self.clear_scene();
        self.set_gravity(spec.gravity);
        // Perpetual backdrops coast (near-zero damping); settling scenes use the heavier
        // SCENE_DAMPING so they come to rest. (Physics scenes P4a.)
        let damping = if spec.perpetual {
            PERPETUAL_SCENE_DAMPING
        } else {
            SCENE_DAMPING
        };
        // Spawn bodies, keeping their handles in spec order so joints reference them by index.
        let mut handles: Vec<RigidBodyHandle> =
            Vec::with_capacity(spec.bodies.len().min(SCENE_BODY_CAP));
        for b in spec.bodies.iter().take(SCENE_BODY_CAP) {
            let builder = match b.body_type {
                SceneBodyType::Fixed => RigidBodyBuilder::fixed(),
                SceneBodyType::Dynamic => RigidBodyBuilder::dynamic(),
            };
            let body = builder
                .translation(Vector::new(b.position.0, b.position.1))
                .rotation(b.rotation)
                .linvel(Vector::new(b.velocity.0, b.velocity.1))
                .linear_damping(damping)
                .angular_damping(DEFAULT_ANGULAR_DAMPING)
                .gravity_scale(b.gravity_scale)
                .build();
            let handle = self.bodies.insert(body);
            let shape = b.collider.to_shared_shape();
            let c = ColliderBuilder::new(shape)
                .density(NODE_BODY_DENSITY)
                .restitution(b.restitution)
                .friction(0.3)
                .collision_groups(scene_groups())
                .build();
            self.colliders
                .insert_with_parent(c, handle, &mut self.bodies);
            let id = SceneBodyId(self.next_scene_id);
            self.next_scene_id += 1;
            self.scene_bodies.insert(id, (handle, b.collider.clone()));
            if let Some(sprite) = &b.sprite {
                self.scene_sprites.insert(id, sprite.clone());
            }
            handles.push(handle);
        }
        // Bind joints between the spawned bodies (by spec index). Out-of-range or capped-out
        // indices are skipped. rapier reaps these when the bodies are removed (clear_scene),
        // so no separate joint bookkeeping is needed.
        for j in &spec.joints {
            let (Some(&a), Some(&b)) = (handles.get(j.body_a), handles.get(j.body_b)) else {
                continue;
            };
            self.insert_scene_joint(a, b, &j.joint);
        }
        self.set_nodes_tangible(spec.default_tangible);
        self.scene_perpetual = spec.perpetual;
    }

    /// Build one [`SceneJoint`] and insert it into the impulse-joint set between two bodies.
    /// Anchors are body-local points (world units). (Physics scenes P4b.)
    fn insert_scene_joint(&mut self, a: RigidBodyHandle, b: RigidBodyHandle, joint: &SceneJoint) {
        let v = |p: (f32, f32)| Vector::new(p.0, p.1);
        match *joint {
            SceneJoint::Fixed { anchor_a, anchor_b } => {
                let jt = FixedJointBuilder::new()
                    .local_anchor1(v(anchor_a))
                    .local_anchor2(v(anchor_b))
                    .build();
                self.impulse_joints.insert(a, b, jt, true);
            }
            SceneJoint::Revolute {
                anchor_a,
                anchor_b,
                motor,
            } => {
                let mut builder = RevoluteJointBuilder::new()
                    .local_anchor1(v(anchor_a))
                    .local_anchor2(v(anchor_b));
                if let Some(m) = motor {
                    builder = builder.motor_velocity(m.target_vel, m.factor);
                }
                self.impulse_joints.insert(a, b, builder.build(), true);
            }
            SceneJoint::Rope {
                anchor_a,
                anchor_b,
                length,
            } => {
                let jt = RopeJointBuilder::new(length)
                    .local_anchor1(v(anchor_a))
                    .local_anchor2(v(anchor_b))
                    .build();
                self.impulse_joints.insert(a, b, jt, true);
            }
            SceneJoint::Spring {
                anchor_a,
                anchor_b,
                rest_length,
                stiffness,
                damping,
            } => {
                let jt = SpringJointBuilder::new(rest_length, stiffness, damping)
                    .local_anchor1(v(anchor_a))
                    .local_anchor2(v(anchor_b))
                    .build();
                self.impulse_joints.insert(a, b, jt, true);
            }
        }
    }

    /// Make a single node tangible (it collides with scene bodies) or intangible (it
    /// passes through), by re-masking its collider's filter. Node-node collision is
    /// unaffected. A no-op for an unknown node. (Physics scenes P2 — tangibility lever.)
    pub fn set_node_tangibility(&mut self, node: NodeKey, tangible: bool) {
        let Some(&body_handle) = self.bodies_by_node.get(&node) else {
            return;
        };
        self.remask_node(body_handle, tangible);
    }

    /// Set every node's tangibility at once (the scene-wide lever): `true` lets the graph
    /// collide with scene bodies, `false` (the default) passes through. Node-node collision
    /// is unaffected either way. (Physics scenes P2.)
    pub fn set_nodes_tangible(&mut self, tangible: bool) {
        self.nodes_tangible = tangible;
        let handles: Vec<RigidBodyHandle> = self.bodies_by_node.values().copied().collect();
        for handle in handles {
            self.remask_node(handle, tangible);
        }
    }

    /// Re-mask one node body's collider(s) to the intangible (`NODE`) or tangible
    /// (`NODE | SCENE`) filter. (Physics scenes P2.)
    fn remask_node(&mut self, body_handle: RigidBodyHandle, tangible: bool) {
        let groups = if tangible {
            InteractionGroups::new(
                NODE_GROUP,
                NODE_GROUP | SCENE_GROUP,
                InteractionTestMode::And,
            )
        } else {
            crate::node_groups()
        };
        let collider_handles: Vec<ColliderHandle> = self
            .bodies
            .get(body_handle)
            .map(|b| b.colliders().to_vec())
            .unwrap_or_default();
        for ch in collider_handles {
            if let Some(c) = self.colliders.get_mut(ch) {
                c.set_collision_groups(groups);
            }
        }
    }
}
