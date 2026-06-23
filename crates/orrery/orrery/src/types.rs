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

/// A node's content-type silhouette, so the orrery can shape its on-screen nodes
/// by what kind of content they hold. The host resolves each node's content type
/// (inker's content-type → engine routing) and pushes these via
/// [`Orrery::set_node_shapes`], the same way it pushes [`NodeState`] colors. A
/// node absent from the map draws as [`Square`](NodeShape::Square), the neutral
/// default; square reads as a document, the others distinguish renderable
/// families at a glance. (The shape vocabulary is a first cut — a theme / lens
/// concern eventually, not a fixed set.)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum NodeShape {
    /// A document (HTML / markdown / gemtext / plain text / …) — a sharp square.
    /// Also the default for an unknown / not-yet-fetched node.
    #[default]
    Square,
    /// An interactive small-web menu / directory (gopher / nex / guppy / …) — a
    /// rounded square.
    Rounded,
    /// A feed (RSS / Atom / JSON feed) — a circle (a "stream").
    Circle,
}

/// A node's presentation form: the *sprite* the orrery draws for it, independent of the
/// node's truth (content, identity, edges stay authoritative in the kernel). The host
/// resolves a default per content type and pushes per-node overrides via
/// [`Orrery::set_node_representation`]; a node without an override takes the content-type
/// default ([`Orrery::node_representation`]). The card preview is a *separate* layer over
/// the node, not a representation form, so it is absent here. This is the initial set;
/// `TexturedBody` (P2-static) and `Scripted` (the field-regions hook) join as their
/// texture / script paths land. (Node representation P1.)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Representation {
    /// The gnode "physical chip": a content-typed colored face carrying the favicon,
    /// with the caption beside it. The default, and what every node showed before P1.
    #[default]
    Tile,
    /// Bare geometry: the colored, content-typed face alone, no favicon or caption.
    /// Minimal and cheap, for a dense graph or a node with nothing to texture.
    Shape,
    /// A custom sprite: an imported image textured on the face — the "alive graph" form
    /// (P2-static, the former `TexturedBody`). The host stores the per-node image (a PNG
    /// data-URI) and pushes it via [`Orrery::set_node_sprite`], which sets this
    /// representation; the collider stays the sized ball for now (the sprite-alpha hull
    /// collider is a later step). (Node representation P2 — sprite.)
    Sprite,
}
