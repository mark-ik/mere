// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The physics backend: where the seiche [`Simulation`] actually ticks.
//!
//! Two shapes behind one [`Physics`] interface (the dual-backend decision, P6):
//!
//! - [`Physics::Inline`] — the simulation ticks **in the frame loop**, on the UI
//!   thread. Deterministic (no thread race), so it is what the canvas's tests
//!   drive; it is also the path for the no-threads `wasm32` browser/PWA profile,
//!   where [`armillary`] cannot spawn an OS thread.
//! - [`Physics::Actor`] — the simulation runs on an [`armillary`] **actor
//!   thread** (always-offload). A heavy settle (`pipeline.step` over hundreds of
//!   bodies) never blocks compositing or input; the actor emits a
//!   [`LayoutSnapshot`] per step and the host folds it into its [`LayoutView`].
//!
//! Native builds construct [`Physics::Inline`] and immediately
//! [`offload`](Physics::offload) onto an actor (so native always offloads); the
//! host supplies the [`Wake`] that pokes its event loop when a snapshot lands.
//! Either way the canvas reads only its [`LayoutView`] — the backend just feeds
//! authoritative positions into it.

use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

use armillary::{ActorHandle, Emitter, Wake, spawn};
use euclid::default::Point2D;
use kernel::graph::NodeKey;
use seiche::{
    AffinitySpring, Basin, CouplingForce, FluidParams, LayoutSnapshot, LayoutView, NodeCollider,
    NodeMaterial, SceneEmitter, SceneField, SceneSpec, Simulation,
};

use super::TICK_DT;

/// A command the host sends to the physics actor. Mirrors the mutating surface
/// of [`Simulation`] plus the settle/drag drivers; all payloads are `Send`
/// (positions and node keys), so the graph itself never crosses the boundary.
pub(crate) enum PhysicsCommand {
    /// Reconcile the body set to exactly these `(node, position)` pairs (position
    /// used only for newly-spawned bodies). See [`Simulation::sync_nodes`].
    SyncNodes(Vec<(NodeKey, Point2D<f32>)>),
    /// Replace the spring edge topology.
    SyncEdges(Vec<(NodeKey, NodeKey)>),
    /// Override the positions of existing bodies (a seed / reseed).
    Seed(Vec<(NodeKey, Point2D<f32>)>),
    /// Pin a node to a world position (a drag in progress).
    Pin(NodeKey, Point2D<f32>),
    /// Release a pinned node back to dynamic.
    Unpin(NodeKey),
    /// Extend the settle budget to at least `n` ticks.
    Settle(u32),
    /// Stop settling now (a restored session that must not re-scramble).
    Halt,
    /// Whether a drag is in progress (keep ticking so neighbors react).
    SetDragging(bool),
    /// Replace the live field-coupling forces wholesale (the rebuild a field place /
    /// move / resize / new node triggers), without a position-losing sim rebuild.
    /// (Field regions — rebuild-on-mutation.)
    SetCouplingForces(Vec<CouplingForce>),
    /// Install (or clear) the pairwise affinity force — the rebuild a fresh affinity signal
    /// triggers ("cluster by affinity"). Position-preserving. (Graph signals — P4.)
    SetAffinityForce(Option<AffinitySpring>),
    SetAnchorForce(Option<seiche::AnchorSpring>),
    /// Set the linear damping (the "inertia" physics setting) on new + live bodies.
    SetLinearDamping(f32),
    /// Reshape node colliders to per-node face shapes (see [`Simulation::set_node_colliders`]).
    SetNodeColliders(Vec<(NodeKey, NodeCollider)>),
    /// Set per-node physical materials (restitution / friction / density; see
    /// [`Simulation::set_node_materials`]). (Node body & face — material.)
    SetNodeMaterials(Vec<(NodeKey, NodeMaterial)>),
    /// Add a non-graph scene-decoration body (shape, world position, drift velocity).
    /// (Physics scenes P1.)
    AddSceneBody(NodeCollider, Point2D<f32>, (f32, f32)),
    /// Set every node's tangibility (collide with scene bodies, or pass through).
    /// (Physics scenes P2.)
    SetNodesTangible(bool),
    /// Load a declarative scene (clears any prior scene). (Physics scenes P3.)
    LoadScene(SceneSpec),
    /// Remove every scene body. (Physics scenes P3.)
    ClearScene,
    /// Load a liquid pool: PBF params, basin, and a `cols × rows` spawn block at `spacing` from
    /// `origin`. (Physics scenes P4c.)
    LoadFluid {
        params: FluidParams,
        basin: Basin,
        origin: Point2D<f32>,
        cols: usize,
        rows: usize,
        spacing: f32,
    },
    /// Remove the liquid pool. (Physics scenes P4c.)
    ClearFluid,
    /// Set (or clear) the scene force-field (whirlpool / well). (Physics scenes P4 fields.)
    SetSceneField(Option<SceneField>),
    /// Add a continuous body emitter (fountain / stream). (Physics scenes — emitters.)
    AddEmitter(SceneEmitter),
    /// Remove every emitter + its bodies. (Physics scenes — emitters.)
    ClearEmitters,
}

/// One layout the actor produced: the positions plus whether it is still
/// settling (so the host knows to keep requesting frames).
pub(crate) struct PhysicsUpdate {
    pub snapshot: LayoutSnapshot,
    pub settling: bool,
}

/// The in-thread backend: the simulation plus the settle/drag state the frame
/// loop steps it with.
pub(crate) struct InlinePhysics {
    sim: Simulation,
    ticks_remaining: u32,
    dragging: bool,
    generation: u64,
}

/// The off-thread backend: the actor handle, its update channel, and the last
/// settling flag the host saw.
pub(crate) struct ActorPhysics {
    handle: ActorHandle<PhysicsCommand>,
    updates: Receiver<PhysicsUpdate>,
    settling: bool,
}

/// The physics backend the [`Canvas`](crate::Canvas) talks to. Inline by
/// default (tests + wasm); [`offload`](Physics::offload) swaps in the actor.
pub(crate) enum Physics {
    Inline(InlinePhysics),
    Actor(ActorPhysics),
}

impl Physics {
    /// A new in-thread backend over `sim`, with an initial settle budget.
    pub fn inline(sim: Simulation, initial_settle: u32) -> Self {
        Physics::Inline(InlinePhysics {
            sim,
            ticks_remaining: initial_settle,
            dragging: false,
            generation: 0,
        })
    }

    /// Move the simulation onto an [`armillary`] actor thread (always-offload).
    /// A no-op if already offloaded. `wake` pokes the host's event loop when a
    /// snapshot is ready. The actor inherits the current settle budget so an
    /// in-flight first settle continues uninterrupted across the move.
    pub fn offload(&mut self, wake: Wake) {
        let Physics::Inline(inline) = self else {
            return;
        };
        // Move the real simulation out (swap in a throwaway empty one) so it can
        // be built-into the actor thread; `Simulation: Send` makes this sound.
        let sim = std::mem::replace(&mut inline.sim, Simulation::new());
        let initial_settle = inline.ticks_remaining;
        let dragging = inline.dragging;
        let (handle, updates) = spawn(wake, move |commands, out| {
            run(sim, initial_settle, dragging, commands, out);
        });
        *self = Physics::Actor(ActorPhysics {
            handle,
            updates,
            settling: initial_settle > 0,
        });
    }

    /// Reconcile the body set (see [`PhysicsCommand::SyncNodes`]).
    pub fn sync_nodes(&mut self, nodes: Vec<(NodeKey, Point2D<f32>)>) {
        match self {
            Physics::Inline(p) => p.sim.sync_nodes(nodes),
            Physics::Actor(p) => {
                p.handle.command(PhysicsCommand::SyncNodes(nodes));
            }
        }
    }

    /// Replace the spring edge topology.
    pub fn sync_edges(&mut self, edges: Vec<(NodeKey, NodeKey)>) {
        match self {
            Physics::Inline(p) => p.sim.sync_edges(edges),
            Physics::Actor(p) => {
                p.handle.command(PhysicsCommand::SyncEdges(edges));
            }
        }
    }

    /// Override existing bodies' positions (a seed / reseed).
    pub fn seed(&mut self, positions: Vec<(NodeKey, Point2D<f32>)>) {
        match self {
            Physics::Inline(p) => p.sim.seed_positions(positions),
            Physics::Actor(p) => {
                p.handle.command(PhysicsCommand::Seed(positions));
            }
        }
    }

    /// Replace the live field-coupling forces wholesale — the rebuild a field place /
    /// move / resize / new node triggers (the host re-resolves every coupling against
    /// the current graph). Position-preserving (forces don't touch body state).
    /// (Field regions — rebuild-on-mutation.)
    pub fn set_coupling_forces(&mut self, forces: Vec<CouplingForce>) {
        match self {
            Physics::Inline(p) => p.sim.set_coupling_forces(forces),
            Physics::Actor(p) => {
                p.handle.command(PhysicsCommand::SetCouplingForces(forces));
            }
        }
    }

    /// Install (or clear, with `None`) the pairwise affinity force wholesale — the rebuild a fresh
    /// affinity signal triggers ("cluster by affinity"). Position-preserving (forces never touch
    /// body state). (Graph signals — P4.)
    pub fn set_affinity_force(&mut self, force: Option<AffinitySpring>) {
        match self {
            Physics::Inline(p) => p.sim.set_affinity_force(force),
            Physics::Actor(p) => {
                p.handle.command(PhysicsCommand::SetAffinityForce(force));
            }
        }
    }

    /// Install (or clear, with `None`) per-node **anchor** springs toward an
    /// arrangement's slots — the layout as a participant in the sim rather than
    /// an override of it. Position-preserving. (Arrangement as attractor.)
    pub fn set_anchor_force(&mut self, force: Option<seiche::AnchorSpring>) {
        match self {
            Physics::Inline(p) => p.sim.set_anchor_force(force),
            Physics::Actor(p) => {
                p.handle.command(PhysicsCommand::SetAnchorForce(force));
            }
        }
    }

    /// The number of anchored nodes (inline backend only). Test introspection.
    #[cfg(test)]
    pub(crate) fn anchor_count(&self) -> usize {
        match self {
            Physics::Inline(p) => p.sim.anchor_count(),
            Physics::Actor(_) => 0,
        }
    }

    /// The number of affinity pairs in the installed affinity force (inline backend only — the
    /// actor owns its sim on another thread). Test introspection for the P4 wiring. (Graph signals.)
    #[cfg(test)]
    pub(crate) fn affinity_pair_count(&self) -> usize {
        match self {
            Physics::Inline(p) => p.sim.affinity_pair_count(),
            Physics::Actor(_) => 0,
        }
    }

    /// Set the linear damping (the "inertia" physics setting) on new + live node
    /// bodies. (Physics settings.)
    pub fn set_linear_damping(&mut self, damping: f32) {
        match self {
            Physics::Inline(p) => p.sim.set_linear_damping(damping),
            Physics::Actor(p) => {
                p.handle.command(PhysicsCommand::SetLinearDamping(damping));
            }
        }
    }

    /// Reshape node colliders so each picks/collides at its true face shape + size (the
    /// host maps every node's silhouette / sprite hull to a [`NodeCollider`]). (P0/P5
    /// collider — Decision 5; node-rep — collider matches shape.)
    pub fn set_node_colliders(&mut self, colliders: Vec<(NodeKey, NodeCollider)>) {
        match self {
            Physics::Inline(p) => p.sim.set_node_colliders(colliders),
            Physics::Actor(p) => {
                p.handle
                    .command(PhysicsCommand::SetNodeColliders(colliders));
            }
        }
    }

    /// Apply per-node physical materials (restitution / friction / density) to the live bodies
    /// (the host maps each node's [`NodeMaterial`] override). (Node body & face — material.)
    pub fn set_node_materials(&mut self, materials: Vec<(NodeKey, NodeMaterial)>) {
        match self {
            Physics::Inline(p) => p.sim.set_node_materials(materials),
            Physics::Actor(p) => {
                p.handle
                    .command(PhysicsCommand::SetNodeMaterials(materials));
            }
        }
    }

    /// Add a non-graph scene-decoration body to the world — a drifting backdrop / scene
    /// element, intangible to the graph by default. Inline (pre-offload) adds it
    /// synchronously; once offloaded it rides a command. (Physics scenes P1.)
    pub fn add_scene_body(
        &mut self,
        collider: NodeCollider,
        position: Point2D<f32>,
        velocity: (f32, f32),
    ) {
        match self {
            Physics::Inline(p) => {
                p.sim.add_scene_body(collider, position, velocity);
            }
            Physics::Actor(p) => {
                p.handle
                    .command(PhysicsCommand::AddSceneBody(collider, position, velocity));
            }
        }
    }

    /// Set every node's tangibility to the scene (the scene-wide lever). (Physics scenes P2.)
    pub fn set_nodes_tangible(&mut self, tangible: bool) {
        match self {
            Physics::Inline(p) => p.sim.set_nodes_tangible(tangible),
            Physics::Actor(p) => {
                p.handle.command(PhysicsCommand::SetNodesTangible(tangible));
            }
        }
    }

    /// Load a declarative scene into the world (clears any prior scene). (Physics scenes P3.)
    pub fn load_scene(&mut self, spec: SceneSpec) {
        match self {
            Physics::Inline(p) => p.sim.load_scene(&spec),
            Physics::Actor(p) => {
                p.handle.command(PhysicsCommand::LoadScene(spec));
            }
        }
    }

    /// Remove every scene body. (Physics scenes P3.)
    pub fn clear_scene(&mut self) {
        match self {
            Physics::Inline(p) => p.sim.clear_scene(),
            Physics::Actor(p) => {
                p.handle.command(PhysicsCommand::ClearScene);
            }
        }
    }

    /// Load a liquid pool (replacing any prior fluid). (Physics scenes P4c.)
    pub fn load_fluid(
        &mut self,
        params: FluidParams,
        basin: Basin,
        origin: Point2D<f32>,
        cols: usize,
        rows: usize,
        spacing: f32,
    ) {
        match self {
            Physics::Inline(p) => p.sim.load_fluid(params, basin, origin, cols, rows, spacing),
            Physics::Actor(p) => {
                p.handle.command(PhysicsCommand::LoadFluid {
                    params,
                    basin,
                    origin,
                    cols,
                    rows,
                    spacing,
                });
            }
        }
    }

    /// Remove the liquid pool. (Physics scenes P4c.)
    pub fn clear_fluid(&mut self) {
        match self {
            Physics::Inline(p) => p.sim.clear_fluid(),
            Physics::Actor(p) => {
                p.handle.command(PhysicsCommand::ClearFluid);
            }
        }
    }

    /// Set (or clear) the scene force-field (whirlpool / well). (Physics scenes P4 fields.)
    pub fn set_scene_field(&mut self, field: Option<SceneField>) {
        match self {
            Physics::Inline(p) => p.sim.set_scene_field(field),
            Physics::Actor(p) => {
                p.handle.command(PhysicsCommand::SetSceneField(field));
            }
        }
    }

    /// Add a continuous body emitter (fountain / stream). (Physics scenes — emitters.)
    pub fn add_emitter(&mut self, spec: SceneEmitter) {
        match self {
            Physics::Inline(p) => p.sim.add_emitter(spec),
            Physics::Actor(p) => {
                p.handle.command(PhysicsCommand::AddEmitter(spec));
            }
        }
    }

    /// Remove every emitter + its bodies. (Physics scenes — emitters.)
    pub fn clear_emitters(&mut self) {
        match self {
            Physics::Inline(p) => p.sim.clear_emitters(),
            Physics::Actor(p) => {
                p.handle.command(PhysicsCommand::ClearEmitters);
            }
        }
    }

    /// Pin a node to a world position (a drag in progress).
    pub fn pin(&mut self, node: NodeKey, position: Point2D<f32>) {
        match self {
            Physics::Inline(p) => p.sim.pin(node, position),
            Physics::Actor(p) => {
                p.handle.command(PhysicsCommand::Pin(node, position));
            }
        }
    }

    /// Release a pinned node back to dynamic.
    pub fn unpin(&mut self, node: NodeKey) {
        match self {
            Physics::Inline(p) => p.sim.unpin(node),
            Physics::Actor(p) => {
                p.handle.command(PhysicsCommand::Unpin(node));
            }
        }
    }

    /// Extend the settle budget to at least `ticks` (start / prolong a settle).
    pub fn settle(&mut self, ticks: u32) {
        match self {
            Physics::Inline(p) => p.ticks_remaining = p.ticks_remaining.max(ticks),
            Physics::Actor(p) => {
                p.handle.command(PhysicsCommand::Settle(ticks));
                p.settling = true;
            }
        }
    }

    /// Stop settling immediately (a restored session that must not re-scramble).
    pub fn halt(&mut self) {
        match self {
            Physics::Inline(p) => p.ticks_remaining = 0,
            Physics::Actor(p) => {
                p.handle.command(PhysicsCommand::Halt);
                p.settling = false;
            }
        }
    }

    /// Tell the backend whether a drag is in progress (keep ticking so the
    /// pinned node's neighbors react through the springs).
    pub fn set_dragging(&mut self, dragging: bool) {
        match self {
            Physics::Inline(p) => p.dragging = dragging,
            Physics::Actor(p) => {
                p.handle.command(PhysicsCommand::SetDragging(dragging));
            }
        }
    }

    /// Whether the layout is still moving (settle in progress or a node dragged).
    pub fn is_settling(&self) -> bool {
        match self {
            Physics::Inline(p) => {
                p.ticks_remaining > 0 || p.dragging || p.sim.wants_continuous_tick()
            }
            Physics::Actor(p) => p.settling,
        }
    }

    /// Advance one frame, folding the latest positions into `view`, and return
    /// whether the layout is still settling.
    ///
    /// - Inline: step the simulation (while settling or dragging) and snapshot it.
    /// - Actor: drain the update channel, applying the most recent snapshot
    ///   (older queued ones are superseded — only the freshest layout matters).
    pub fn advance_frame(&mut self, view: &mut LayoutView) -> bool {
        match self {
            Physics::Inline(p) => {
                if p.ticks_remaining > 0 || p.dragging || p.sim.wants_continuous_tick() {
                    p.sim.tick(TICK_DT);
                    if p.ticks_remaining > 0 {
                        p.ticks_remaining -= 1;
                    }
                }
                p.generation = p.generation.wrapping_add(1);
                view.apply_snapshot(&p.sim.snapshot(p.generation));
                p.ticks_remaining > 0 || p.dragging || p.sim.wants_continuous_tick()
            }
            Physics::Actor(p) => {
                let mut latest = None;
                loop {
                    match p.updates.try_recv() {
                        Ok(update) => latest = Some(update),
                        Err(_) => break,
                    }
                }
                if let Some(update) = latest {
                    view.apply_snapshot(&update.snapshot);
                    p.settling = update.settling;
                }
                p.settling
            }
        }
    }
}

/// The actor thread body: own the simulation, apply commands, and tick at ~60Hz
/// while there is work, emitting a snapshot per step. Parks on `recv` when idle
/// (no busy-spin), and ends when the command channel closes (the handle drops).
fn run(
    mut sim: Simulation,
    initial_settle: u32,
    initial_dragging: bool,
    commands: Receiver<PhysicsCommand>,
    out: Emitter<PhysicsUpdate>,
) {
    let mut ticks_remaining = initial_settle;
    let mut dragging = initial_dragging;
    let mut generation: u64 = 0;
    // Pace the active settle at the simulation timestep so it does not run the
    // whole budget in microseconds and burn a core.
    let pacing = Duration::from_secs_f32(TICK_DT);

    loop {
        // Idle: block for the next command so the thread parks. A closed channel
        // (the host dropped the handle) ends the actor. A perpetual scene (a drifting
        // backdrop) is never idle, so the actor keeps ticking instead of parking.
        if ticks_remaining == 0 && !dragging && !sim.wants_continuous_tick() {
            match commands.recv() {
                Ok(cmd) => apply(&mut sim, cmd, &mut ticks_remaining, &mut dragging),
                Err(_) => return,
            }
        }
        // Drain any further pending commands without blocking. A disconnect means
        // the host is gone: stop accepting commands, but still run out whatever
        // settle is already queued before exiting (so a drop right after a
        // `Settle` does not throw away the layout work).
        let mut disconnected = false;
        loop {
            match commands.try_recv() {
                Ok(cmd) => apply(&mut sim, cmd, &mut ticks_remaining, &mut dragging),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }
        // Step while there is work (a settle, a drag, or a perpetual scene); emit the new
        // layout and pace the next step.
        if ticks_remaining > 0 || dragging || sim.wants_continuous_tick() {
            sim.tick(TICK_DT);
            if ticks_remaining > 0 {
                ticks_remaining -= 1;
            }
            generation = generation.wrapping_add(1);
            let settling = ticks_remaining > 0 || dragging || sim.wants_continuous_tick();
            out.emit(PhysicsUpdate {
                snapshot: sim.snapshot(generation),
                settling,
            });
            std::thread::sleep(pacing);
        }
        // Host gone and the settle budget spent: wind down.
        if disconnected && ticks_remaining == 0 {
            return;
        }
    }
}

/// Fold one command into the actor's simulation + settle/drag state.
fn apply(
    sim: &mut Simulation,
    cmd: PhysicsCommand,
    ticks_remaining: &mut u32,
    dragging: &mut bool,
) {
    match cmd {
        PhysicsCommand::SyncNodes(nodes) => sim.sync_nodes(nodes),
        PhysicsCommand::SyncEdges(edges) => sim.sync_edges(edges),
        PhysicsCommand::Seed(positions) => sim.seed_positions(positions),
        PhysicsCommand::Pin(node, position) => sim.pin(node, position),
        PhysicsCommand::Unpin(node) => sim.unpin(node),
        PhysicsCommand::Settle(n) => *ticks_remaining = (*ticks_remaining).max(n),
        PhysicsCommand::Halt => *ticks_remaining = 0,
        PhysicsCommand::SetDragging(d) => *dragging = d,
        PhysicsCommand::SetCouplingForces(forces) => sim.set_coupling_forces(forces),
        PhysicsCommand::SetAffinityForce(force) => sim.set_affinity_force(force),
        PhysicsCommand::SetAnchorForce(force) => sim.set_anchor_force(force),
        PhysicsCommand::SetLinearDamping(damping) => sim.set_linear_damping(damping),
        PhysicsCommand::SetNodeColliders(colliders) => sim.set_node_colliders(colliders),
        PhysicsCommand::SetNodeMaterials(materials) => sim.set_node_materials(materials),
        PhysicsCommand::AddSceneBody(collider, position, velocity) => {
            sim.add_scene_body(collider, position, velocity);
        }
        PhysicsCommand::SetNodesTangible(tangible) => sim.set_nodes_tangible(tangible),
        PhysicsCommand::LoadScene(spec) => sim.load_scene(&spec),
        PhysicsCommand::ClearScene => sim.clear_scene(),
        PhysicsCommand::LoadFluid {
            params,
            basin,
            origin,
            cols,
            rows,
            spacing,
        } => sim.load_fluid(params, basin, origin, cols, rows, spacing),
        PhysicsCommand::ClearFluid => sim.clear_fluid(),
        PhysicsCommand::SetSceneField(field) => sim.set_scene_field(field),
        PhysicsCommand::AddEmitter(spec) => sim.add_emitter(spec),
        PhysicsCommand::ClearEmitters => sim.clear_emitters(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use euclid::default::Point2D;
    use kernel::graph::Graph;

    use super::*;
    use kernel::graph::fixtures::GraphFixtures;

    /// The actor processes a sync + settle and emits layout snapshots, then ends
    /// cleanly when the handle drops. (The physics math itself is seiche's concern;
    /// this is a protocol smoke test of the run loop.)
    #[test]
    fn actor_syncs_settles_and_emits_snapshots() {
        let mut graph = Graph::new();
        let a = graph.add_node_with_id(
            uuid::Uuid::from_u128(1),
            "mere://a".into(),
            Default::default(),
        );
        let b = graph.add_node_with_id(
            uuid::Uuid::from_u128(2),
            "mere://b".into(),
            Default::default(),
        );

        let mut sim = Simulation::new();
        sim.add_force(seiche::NodeExclusion::default());
        let wake: Wake = Arc::new(|| {});
        let (handle, updates) = spawn(wake, move |commands, out| run(sim, 0, false, commands, out));

        handle.command(PhysicsCommand::SyncNodes(vec![
            (a, Point2D::new(0.0, 0.0)),
            (b, Point2D::new(1.0, 0.0)),
        ]));
        handle.command(PhysicsCommand::SyncEdges(vec![(a, b)]));
        handle.command(PhysicsCommand::Settle(4));
        // Dropping the handle closes the command channel; the actor finishes its
        // settle, then ends. `iter` collects every emitted update to completion.
        drop(handle);

        let emitted: Vec<PhysicsUpdate> = updates.iter().collect();
        assert!(
            !emitted.is_empty(),
            "the actor emitted at least one layout snapshot"
        );
        let last = emitted.last().unwrap();
        assert_eq!(
            last.snapshot.positions.len(),
            2,
            "both bodies are in the snapshot"
        );
    }
}
