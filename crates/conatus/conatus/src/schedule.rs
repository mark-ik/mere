// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::{
    any::{Any, TypeId},
    collections::HashMap,
    error::Error,
    fmt,
};

use crate::{BodyCommand, BodyWorld, CommandId, StepUpdate, command::CommandQueue};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Phase {
    /// Receive already-admitted spatial work from the conducting profile.
    Ingest,
    /// Evaluate fields that may affect spatial state this step.
    Fields,
    /// Run spatial systems immediately before tactile physics advances.
    BeforePhysics,
    /// Observe the completed tactile step and enqueue derived spatial work.
    AfterPhysics,
    /// Materialize voxel, geometry, or resident-state changes.
    Materialize,
    /// Publish derived spatial changes to the conducting profile.
    Publish,
}

impl Phase {
    pub(crate) const BEFORE_PHYSICS: [Self; 3] = [Self::Ingest, Self::Fields, Self::BeforePhysics];

    pub(crate) const AFTER_PHYSICS: [Self; 3] =
        [Self::AfterPhysics, Self::Materialize, Self::Publish];
}

#[derive(Default)]
pub struct Resources {
    values: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl Resources {
    pub fn insert<T: Send + Sync + 'static>(&mut self, value: T) -> Option<T> {
        self.values
            .insert(TypeId::of::<T>(), Box::new(value))
            .and_then(|old| old.downcast::<T>().ok())
            .map(|old| *old)
    }

    pub fn contains<T: Send + Sync + 'static>(&self) -> bool {
        self.values.contains_key(&TypeId::of::<T>())
    }

    pub fn get<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.values.get(&TypeId::of::<T>())?.downcast_ref()
    }

    pub fn get_mut<T: Send + Sync + 'static>(&mut self) -> Option<&mut T> {
        self.values.get_mut(&TypeId::of::<T>())?.downcast_mut()
    }

    pub fn remove<T: Send + Sync + 'static>(&mut self) -> Option<T> {
        self.values
            .remove(&TypeId::of::<T>())?
            .downcast::<T>()
            .ok()
            .map(|value| *value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemError {
    message: String,
}

impl SystemError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for SystemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for SystemError {}

pub struct SystemContext<'a> {
    pub tick: u64,
    pub dt: f32,
    pub phase: Phase,
    bodies: &'a BodyWorld,
    step: Option<&'a StepUpdate>,
    resources: &'a mut Resources,
    commands: &'a mut CommandQueue,
}

impl SystemContext<'_> {
    pub fn bodies(&self) -> &BodyWorld {
        self.bodies
    }

    pub fn resources(&self) -> &Resources {
        self.resources
    }

    /// Physics output for after-physics phases. Before-physics phases receive
    /// `None` because the current tick has not advanced yet.
    pub fn step_update(&self) -> Option<&StepUpdate> {
        self.step
    }

    pub fn resources_mut(&mut self) -> &mut Resources {
        self.resources
    }

    pub fn queue(&mut self, command: BodyCommand) -> CommandId {
        self.commands.push(command)
    }
}

type SystemFn =
    dyn for<'a> FnMut(&mut SystemContext<'a>) -> Result<(), SystemError> + Send + 'static;

pub(crate) struct ScheduledSystem {
    pub(crate) name: String,
    pub(crate) phase: Phase,
    run: Box<SystemFn>,
}

impl ScheduledSystem {
    pub(crate) fn new<F>(name: String, phase: Phase, run: F) -> Self
    where
        F: for<'a> FnMut(&mut SystemContext<'a>) -> Result<(), SystemError> + Send + 'static,
    {
        Self {
            name,
            phase,
            run: Box::new(run),
        }
    }

    pub(crate) fn run(&mut self, context: &mut SystemContext<'_>) -> Result<(), SystemError> {
        (self.run)(context)
    }
}

pub(crate) fn context<'a>(
    tick: u64,
    dt: f32,
    phase: Phase,
    bodies: &'a BodyWorld,
    step: Option<&'a StepUpdate>,
    resources: &'a mut Resources,
    commands: &'a mut CommandQueue,
) -> SystemContext<'a> {
    SystemContext {
        tick,
        dt,
        phase,
        bodies,
        step,
        resources,
        commands,
    }
}
