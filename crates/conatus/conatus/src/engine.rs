use std::{collections::BTreeMap, error::Error, fmt};

use crate::{
    BodyCommand, BodyError, BodyId, BodyState, BodyWorld, ClockError, CommandId, CommandResult,
    FixedClock, InteractionEvent, Phase, Resources, StepUpdate, SystemContext, SystemError,
    VoxelChange,
    command::CommandQueue,
    schedule::{ScheduledSystem, context},
};

#[derive(Debug)]
pub enum EngineError {
    Body(BodyError),
    System {
        tick: u64,
        phase: Phase,
        name: String,
        source: SystemError,
    },
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Body(error) => error.fmt(f),
            Self::System {
                tick,
                phase,
                name,
                source,
            } => write!(
                f,
                "system {name:?} failed in {phase:?} at tick {tick}: {source}"
            ),
        }
    }
}

impl Error for EngineError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Body(error) => Some(error),
            Self::System { source, .. } => Some(source),
        }
    }
}

impl From<BodyError> for EngineError {
    fn from(value: BodyError) -> Self {
        Self::Body(value)
    }
}

#[derive(Debug)]
pub enum EngineConfigError {
    Clock(ClockError),
    Body(BodyError),
}

impl fmt::Display for EngineConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Clock(error) => error.fmt(f),
            Self::Body(error) => error.fmt(f),
        }
    }
}

impl Error for EngineConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Clock(error) => Some(error),
            Self::Body(error) => Some(error),
        }
    }
}

impl From<ClockError> for EngineConfigError {
    fn from(value: ClockError) -> Self {
        Self::Clock(value)
    }
}

impl From<BodyError> for EngineConfigError {
    fn from(value: BodyError) -> Self {
        Self::Body(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EngineConfig {
    pub ticks_per_second: u32,
    pub max_steps_per_advance: u64,
    pub gravity: [f32; 3],
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            ticks_per_second: 60,
            max_steps_per_advance: 8,
            gravity: [0.0, -9.81, 0.0],
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameUpdate {
    pub steps: u64,
    pub deferred_steps: u64,
    pub tick: u64,
    pub revision: u64,
    pub interpolation_alpha: f32,
    /// Latest state for every body changed during this host frame, sorted by
    /// stable body id. Multiple fixed steps collapse to the final transform.
    pub changed: Vec<BodyState>,
    /// Stable ids whose bodies were removed during this host frame.
    pub removed: Vec<BodyId>,
    /// Effective sparse voxel edits in revision order.
    pub voxel_changes: Vec<VoxelChange>,
    /// Ordered interaction edges. Entries and exits that occur in different
    /// fixed steps remain distinct rather than being collapsed.
    pub interactions: Vec<InteractionEvent>,
    /// Results from structural commands applied at tick/phase boundaries.
    /// A failed command does not prevent unrelated systems from advancing.
    pub commands: Vec<CommandResult>,
}

/// The shared fixed-step spatial runtime.
///
/// A product profile supplies elapsed microseconds or exact steps, admits
/// spatial changes through [`Self::bodies_mut`] or [`Self::queue`], and routes
/// [`FrameUpdate`] to its selected consumers. Rules, input, audio, rendering,
/// and durable source bindings remain outside this runtime.
pub struct Engine {
    bodies: BodyWorld,
    clock: FixedClock,
    max_steps_per_advance: u64,
    resources: Resources,
    commands: CommandQueue,
    systems: Vec<ScheduledSystem>,
}

impl Engine {
    pub fn new(config: EngineConfig) -> Result<Self, EngineConfigError> {
        Ok(Self {
            bodies: BodyWorld::try_new(config.gravity)?,
            clock: FixedClock::new(config.ticks_per_second)?,
            max_steps_per_advance: config.max_steps_per_advance,
            resources: Resources::default(),
            commands: CommandQueue::default(),
            systems: Vec::new(),
        })
    }

    pub fn bodies(&self) -> &BodyWorld {
        &self.bodies
    }

    pub fn bodies_mut(&mut self) -> &mut BodyWorld {
        &mut self.bodies
    }

    pub fn clock(&self) -> &FixedClock {
        &self.clock
    }

    pub fn resources(&self) -> &Resources {
        &self.resources
    }

    pub fn resources_mut(&mut self) -> &mut Resources {
        &mut self.resources
    }

    pub fn queue(&mut self, command: BodyCommand) -> CommandId {
        self.commands.push(command)
    }

    pub fn queued_commands(&self) -> usize {
        self.commands.len()
    }

    /// Register a system in deterministic registration order within its phase.
    pub fn add_system<F>(&mut self, phase: Phase, name: impl Into<String>, system: F)
    where
        F: for<'a> FnMut(&mut SystemContext<'a>) -> Result<(), SystemError> + Send + 'static,
    {
        self.systems
            .push(ScheduledSystem::new(name.into(), phase, system));
    }

    pub const fn max_steps_per_advance(&self) -> u64 {
        self.max_steps_per_advance
    }

    pub fn set_max_steps_per_advance(&mut self, max_steps: u64) {
        self.max_steps_per_advance = max_steps;
    }

    pub fn advance(&mut self, elapsed_us: u64) -> Result<FrameUpdate, EngineError> {
        let advance = self.clock.advance(elapsed_us, self.max_steps_per_advance);
        self.run_steps(advance.steps, advance.deferred)
    }

    /// Advance an exact number of simulation steps without changing the host
    /// clock. Useful for turn-based games, servers, and deliberate single-step
    /// tools.
    pub fn step(&mut self, steps: u64) -> Result<FrameUpdate, EngineError> {
        self.run_steps(steps, self.clock.pending_steps())
    }

    pub fn discard_time_backlog(&mut self) -> u64 {
        self.clock.discard_backlog()
    }

    fn run_steps(&mut self, steps: u64, deferred_steps: u64) -> Result<FrameUpdate, EngineError> {
        let mut latest = BTreeMap::<BodyId, BodyState>::new();
        let mut interactions = self.bodies.drain_events();
        let mut removed = self.bodies.drain_removed();
        let mut voxel_changes = self.bodies.drain_voxel_changes();
        let mut command_results = Vec::new();
        let dt = self.clock.step_seconds();

        for _ in 0..steps {
            command_results.extend(self.apply_commands(dt));
            let tick = self.bodies.tick().saturating_add(1);
            for phase in Phase::BEFORE_PHYSICS {
                self.run_phase(phase, tick, dt, None)?;
                command_results.extend(self.apply_commands(dt));
            }

            let update = self.bodies.step(dt)?;
            let tick = self.bodies.tick();
            for phase in Phase::AFTER_PHYSICS {
                self.run_phase(phase, tick, dt, Some(&update))?;
                command_results.extend(self.apply_commands(dt));
            }
            for state in update.changed {
                latest.insert(state.id, state);
            }
            removed.extend(update.removed);
            voxel_changes.extend(update.voxel_changes);
            interactions.extend(update.interactions);
            for state in self.bodies.drain_changes() {
                latest.insert(state.id, state);
            }
            removed.extend(self.bodies.drain_removed());
            voxel_changes.extend(self.bodies.drain_voxel_changes());
            interactions.extend(self.bodies.drain_events());
        }

        if steps == 0 {
            for state in self.bodies.drain_changes() {
                latest.insert(state.id, state);
            }
            voxel_changes.extend(self.bodies.drain_voxel_changes());
        }
        for id in &removed {
            latest.remove(id);
        }
        removed.sort_unstable();
        removed.dedup();

        Ok(FrameUpdate {
            steps,
            deferred_steps,
            tick: self.bodies.tick(),
            revision: self.bodies.revision(),
            interpolation_alpha: self.clock.interpolation_alpha(),
            changed: latest.into_values().collect(),
            removed,
            voxel_changes,
            interactions,
            commands: command_results,
        })
    }

    fn apply_commands(&mut self, dt: f32) -> Vec<CommandResult> {
        self.commands.drain_apply(&mut self.bodies, dt)
    }

    fn run_phase(
        &mut self,
        phase: Phase,
        tick: u64,
        dt: f32,
        step: Option<&StepUpdate>,
    ) -> Result<(), EngineError> {
        let Self {
            bodies,
            resources,
            commands,
            systems,
            ..
        } = self;
        for system in systems.iter_mut().filter(|system| system.phase == phase) {
            let mut context = context(tick, dt, phase, bodies, step, resources, commands);
            if let Err(source) = system.run(&mut context) {
                return Err(EngineError::System {
                    tick,
                    phase,
                    name: system.name.clone(),
                    source,
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BodyDesc, ColliderDesc, ColliderShape, CommandEffect, Transform};

    fn engine() -> Engine {
        Engine::new(EngineConfig::default()).unwrap()
    }

    #[test]
    fn uneven_host_frames_drive_whole_fixed_steps() {
        let mut steady = engine();
        let mut uneven = engine();
        for runtime in [&mut steady, &mut uneven] {
            runtime
                .bodies_mut()
                .spawn(
                    BodyDesc::dynamic()
                        .at(Transform::from_translation([0.0, 10.0, 0.0]))
                        .with_collider(ColliderDesc::new(ColliderShape::sphere(0.5))),
                )
                .unwrap();
            runtime.set_max_steps_per_advance(u64::MAX);
        }

        steady.advance(100_000).unwrap();
        for elapsed in [1_000, 40_000, 7, 31_000, 27_993] {
            uneven.advance(elapsed).unwrap();
        }

        assert_eq!(steady.bodies().tick(), 6);
        assert_eq!(steady.bodies().tick(), uneven.bodies().tick());
        assert_eq!(steady.bodies().states(), uneven.bodies().states());
    }

    #[test]
    fn several_steps_publish_only_latest_transform() {
        let mut engine = engine();
        let body = engine
            .bodies_mut()
            .spawn(
                BodyDesc::dynamic()
                    .at(Transform::from_translation([0.0, 10.0, 0.0]))
                    .with_collider(ColliderDesc::new(ColliderShape::sphere(0.5))),
            )
            .unwrap();
        let frame = engine.step(4).unwrap();
        assert_eq!(frame.changed.len(), 1);
        assert_eq!(frame.changed[0], engine.bodies().state(body).unwrap());
    }

    #[test]
    fn zero_step_frames_still_publish_spawned_bodies() {
        let mut engine = engine();
        let body = engine
            .bodies_mut()
            .spawn(
                BodyDesc::fixed().with_collider(ColliderDesc::new(ColliderShape::cuboid([1.0; 3]))),
            )
            .unwrap();
        let frame = engine.advance(0).unwrap();
        assert_eq!(frame.steps, 0);
        assert_eq!(frame.changed.len(), 1);
        assert_eq!(frame.changed[0].id, body);
    }

    #[test]
    fn zero_step_frames_publish_despawns() {
        let mut engine = engine();
        let body = engine
            .bodies_mut()
            .spawn(
                BodyDesc::fixed().with_collider(ColliderDesc::new(ColliderShape::cuboid([1.0; 3]))),
            )
            .unwrap();
        engine.advance(0).unwrap();
        engine.bodies_mut().despawn(body).unwrap();

        let frame = engine.advance(0).unwrap();
        assert!(frame.changed.is_empty());
        assert_eq!(frame.removed, vec![body]);
    }

    #[test]
    fn systems_share_resources_and_apply_commands_between_phases() {
        #[derive(Default)]
        struct Count(u32);

        let mut engine = engine();
        engine.resources_mut().insert(Count::default());
        engine.add_system(Phase::BeforePhysics, "spawn once", |context| {
            let spawn = {
                let count = context.resources_mut().get_mut::<Count>().unwrap();
                count.0 += 1;
                count.0 == 1
            };
            if spawn {
                context.queue(BodyCommand::Spawn {
                    body: BodyDesc::fixed()
                        .with_collider(ColliderDesc::new(ColliderShape::cuboid([1.0; 3]))),
                });
            }
            Ok(())
        });

        let frame = engine.step(2).unwrap();
        assert_eq!(engine.resources().get::<Count>().unwrap().0, 2);
        assert_eq!(engine.bodies().len(), 1);
        assert_eq!(frame.commands.len(), 1);
        assert!(matches!(
            frame.commands[0].result,
            Ok(CommandEffect::Spawned(_))
        ));
    }

    #[test]
    fn systems_run_in_phase_then_registration_order() {
        #[derive(Default)]
        struct Order(Vec<&'static str>);

        let mut engine = engine();
        engine.resources_mut().insert(Order::default());
        engine.add_system(Phase::Publish, "publish", |context| {
            context
                .resources_mut()
                .get_mut::<Order>()
                .unwrap()
                .0
                .push("publish");
            Ok(())
        });
        engine.add_system(Phase::BeforePhysics, "before physics one", |context| {
            context
                .resources_mut()
                .get_mut::<Order>()
                .unwrap()
                .0
                .push("before physics one");
            Ok(())
        });
        engine.add_system(Phase::BeforePhysics, "before physics two", |context| {
            context
                .resources_mut()
                .get_mut::<Order>()
                .unwrap()
                .0
                .push("before physics two");
            Ok(())
        });

        engine.step(1).unwrap();
        assert_eq!(
            engine.resources().get::<Order>().unwrap().0,
            ["before physics one", "before physics two", "publish"]
        );
    }

    #[test]
    fn after_physics_systems_receive_the_current_step_update() {
        #[derive(Default)]
        struct Seen {
            tick: u64,
            changed: usize,
        }

        let mut engine = engine();
        engine.resources_mut().insert(Seen::default());
        engine
            .bodies_mut()
            .spawn(
                BodyDesc::dynamic()
                    .at(Transform::from_translation([0.0, 3.0, 0.0]))
                    .with_collider(ColliderDesc::new(ColliderShape::sphere(0.5))),
            )
            .unwrap();
        engine.add_system(Phase::AfterPhysics, "observe physics", |context| {
            let update = context.step_update().expect("after physics has an update");
            let tick = update.tick;
            let changed = update.changed.len();
            let seen = context.resources_mut().get_mut::<Seen>().unwrap();
            seen.tick = tick;
            seen.changed = changed;
            Ok(())
        });

        engine.step(1).unwrap();
        let seen = engine.resources().get::<Seen>().unwrap();
        assert_eq!(seen.tick, 1);
        assert_eq!(seen.changed, 1);
    }

    #[test]
    fn zero_step_frames_publish_voxel_edits() {
        let mut engine = engine();
        let body = engine
            .bodies_mut()
            .spawn(
                BodyDesc::fixed().with_collider(ColliderDesc::new(ColliderShape::VoxelGrid {
                    cell_size: [1.0; 3],
                    occupied: vec![[0, 0, 0]],
                })),
            )
            .unwrap();
        engine.advance(0).unwrap();
        engine
            .bodies_mut()
            .edit_voxels(
                crate::ColliderId::new(body, 0),
                [crate::VoxelEdit {
                    cell: [1, 0, 0],
                    filled: true,
                }],
            )
            .unwrap();

        let frame = engine.advance(0).unwrap();
        assert_eq!(frame.voxel_changes.len(), 1);
        assert_eq!(frame.voxel_changes[0].edits[0].cell, [1, 0, 0]);
    }

    #[test]
    fn invalid_engine_configuration_is_refused() {
        assert!(matches!(
            Engine::new(EngineConfig {
                ticks_per_second: 0,
                ..EngineConfig::default()
            }),
            Err(EngineConfigError::Clock(_))
        ));
        assert!(matches!(
            Engine::new(EngineConfig {
                gravity: [f32::NAN, 0.0, 0.0],
                ..EngineConfig::default()
            }),
            Err(EngineConfigError::Body(_))
        ));
    }
}
