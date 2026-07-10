/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::any::Any;
use std::collections::HashMap;
use std::hash::Hash;
use std::marker::PhantomData;

use euclid::default::Vector2D;

use crate::camera::CanvasViewport;
use crate::scene::CanvasSceneInput;

use super::LayoutCapability;
use crate::{Layout, LayoutExtras};

// ── DynLayout — object-safe Layout ───────────────────────────────────────────

/// Erased state for layouts stored behind a trait object. Concrete
/// `Layout::State` types erase to `Box<dyn Any + Send>`; the blanket
/// [`DynLayout`] impl downcasts back on each call.
pub type ErasedState = Box<dyn Any + Send>;

/// Object-safe analogue of [`Layout`]. Every concrete `Layout<N>` whose
/// `State` is `Any + Default + Send` gets a blanket `DynLayout<N>` impl.
pub trait DynLayout<N: Clone + Eq + Hash + Send>: Send {
    fn step_dyn(
        &mut self,
        scene: &CanvasSceneInput<N>,
        state: &mut ErasedState,
        dt: f32,
        viewport: &CanvasViewport,
        extras: &LayoutExtras<N>,
    ) -> HashMap<N, Vector2D<f32>>;

    fn is_converged_dyn(&self, state: &ErasedState) -> bool;

    fn default_state_erased(&self) -> ErasedState;
}

impl<N, L> DynLayout<N> for L
where
    N: Clone + Eq + Hash + Send + 'static,
    L: Layout<N> + Send,
    L::State: Any + Default + Send,
{
    fn step_dyn(
        &mut self,
        scene: &CanvasSceneInput<N>,
        state: &mut ErasedState,
        dt: f32,
        viewport: &CanvasViewport,
        extras: &LayoutExtras<N>,
    ) -> HashMap<N, Vector2D<f32>> {
        let state_typed = state
            .downcast_mut::<L::State>()
            .expect("DynLayout: state type mismatch for this provider");
        self.step(scene, state_typed, dt, viewport, extras)
    }

    fn is_converged_dyn(&self, state: &ErasedState) -> bool {
        state
            .downcast_ref::<L::State>()
            .map(|s| self.is_converged(s))
            .unwrap_or(false)
    }

    fn default_state_erased(&self) -> ErasedState {
        Box::new(L::State::default())
    }
}

// ── Providers ────────────────────────────────────────────────────────────────

/// A producer of a particular layout. Hosts register providers; users
/// select a layout by id; the registry resolves to a provider; the
/// provider creates a fresh layout + state pair.
pub trait LayoutProvider<N: Clone + Eq + Hash + Send + 'static>: Send + Sync {
    fn capability(&self) -> LayoutCapability;
    /// Construct a fresh layout instance using the provider's default
    /// configuration. Hosts that want custom config construct concrete
    /// types directly and bypass the registry.
    fn create_default(&self) -> Box<dyn DynLayout<N>>;
}

/// A zero-sized built-in provider parameterized by the layout type `L`
/// and a capability-builder function. Used to register every built-in
/// layout with one line each.
pub struct BuiltinProvider<L, N>
where
    L: Default + Layout<N> + Send + 'static,
    L::State: Any + Default + Send,
    N: Clone + Eq + Hash + Send + 'static,
{
    capability_fn: fn() -> LayoutCapability,
    _layout: PhantomData<fn() -> L>,
    _node: PhantomData<fn() -> N>,
}

impl<L, N> BuiltinProvider<L, N>
where
    L: Default + Layout<N> + Send + 'static,
    L::State: Any + Default + Send,
    N: Clone + Eq + Hash + Send + 'static,
{
    pub const fn new(capability_fn: fn() -> LayoutCapability) -> Self {
        Self {
            capability_fn,
            _layout: PhantomData,
            _node: PhantomData,
        }
    }
}

impl<L, N> LayoutProvider<N> for BuiltinProvider<L, N>
where
    L: Default + Layout<N> + Send + 'static,
    L::State: Any + Default + Send,
    N: Clone + Eq + Hash + Send + 'static,
{
    fn capability(&self) -> LayoutCapability {
        (self.capability_fn)()
    }

    fn create_default(&self) -> Box<dyn DynLayout<N>> {
        Box::new(L::default())
    }
}
