/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

/// A pointer button the host reports to the orrery (winit / serval / … map onto it).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerButton {
    Left,
    Middle,
    Right,
}

/// The orrery's camera as plain pan + zoom, for the host to persist and restore
/// (the host maps it to/from its own serialized form). `offset` is the screen-px
/// translation, `zoom` the uniform scale: `screen = world * zoom + offset`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraView {
    pub offset: (f32, f32),
    pub zoom: f32,
}

/// A node's coarse activation state, for the orrery to color its on-screen
/// nodes. The host computes these from the actor pool + content cache and pushes
/// them via [`Orrery::set_node_states`]; a node absent from the map colors as
/// [`Idle`](NodeState::Idle).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeState {
    /// A live actor is showing real (fetched) content — green.
    Open,
    /// Real fetched content, but no actor showing it right now — red.
    Closed,
    /// Idle: a local / settings page, or one that is synthesized, blank
    /// (loading), or errored — blue. The "kinda idle" nodes.
    Idle,
}
