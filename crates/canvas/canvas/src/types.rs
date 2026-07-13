/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use kernel::graph::{NodeKey, RelationSelector};

/// A selected addressable relation cell on the canvas.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct EdgeCell {
    pub from: NodeKey,
    pub to: NodeKey,
    pub selector: RelationSelector,
}

impl EdgeCell {
    /// The display/visibility endpoint bundle this cell belongs to.
    pub fn endpoint_pair(self) -> (NodeKey, NodeKey) {
        if self.from <= self.to {
            (self.from, self.to)
        } else {
            (self.to, self.from)
        }
    }
}

/// A pointer button the host reports to the canvas (winit / genet / … map onto it).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerButton {
    Left,
    Middle,
    Right,
}

/// The canvas's camera as plain pan + zoom, for the host to persist and restore
/// (the host maps it to/from its own serialized form). `offset` is the screen-px
/// translation, `zoom` the uniform scale: `screen = world * zoom + offset`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraView {
    pub offset: (f32, f32),
    pub zoom: f32,
}

/// The full per-pane *view* state over a shared canvas: the complete camera (pan +
/// zoom + isometric orbit/foreshorten) plus its pan inertia. The canvas
/// *authority* owns the graph, physics, and node positions; the **view** owns one
/// `Viewport` per (window, graph) pane. The host installs it before a render/input
/// pass with [`Canvas::set_viewport`] and reads back the mutated value with
/// [`Canvas::viewport`].
///
/// This is what lets two windows on one graph be independent views rather than a
/// mirror: they hold distinct `Viewport`s over the *shared* node positions. The
/// canvas's own camera/`pan_velocity` fields are then per-pass working state the
/// host drives, not durable per-graph truth. Seed a fresh pane's viewport from the
/// canvas's current one (`viewport()`) rather than constructing it, so the initial
/// framing is sensible.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    /// Screen-px camera translation (`CameraView::offset`).
    pub offset: (f32, f32),
    /// Uniform camera scale (`CameraView::zoom`).
    pub zoom: f32,
    /// Isometric orbit angle about the vertical, radians (`0.0` is top-down).
    pub yaw: f32,
    /// Isometric vertical foreshorten in `(0, 1]` (`1.0` is top-down 2D).
    pub tilt: f32,
    /// Inertial pan velocity (px/frame); per-view so one window's fling does not
    /// drift another window sharing the same canvas.
    pub pan_velocity: (f32, f32),
}

/// A node's coarse activation state, for the canvas to color its on-screen
/// nodes. The host computes these from the actor pool + content cache and pushes
/// them via [`Canvas::set_node_states`]; a node absent from the map colors as
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

/// A node's content-type silhouette, so the canvas can shape its on-screen nodes
/// by what kind of content they hold. The host resolves each node's content type
/// (inker's content-type → engine routing) and pushes these via
/// [`Canvas::set_node_shapes`], the same way it pushes [`NodeState`] colors. A
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

/// What is painted on a node's **face**: one of the two presentation axes (the other is the
/// **body** shape, the node's collider geometry). The face is independent of the body, so a
/// node with a custom hull body can still show its favicon, and a sprite face can sit on the
/// default silhouette body. Independent of the node's truth (content, identity, edges stay
/// authoritative in the kernel). The host pushes per-node overrides via
/// [`Canvas::set_node_face`]; a node without an override defaults to [`Favicon`](Face::Favicon).
/// The card preview is a *separate* layer over the node, not a face, so it is absent here.
/// (Node body & face model — the Face axis.)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Face {
    /// The fetched favicon textured on the face, with the caption beside it (the bare state
    /// color shows through until a favicon arrives). The default: a standard "tile" node wears
    /// this face over the content-type silhouette body.
    #[default]
    Favicon,
    /// A custom imported sprite image, cover-fit over the whole face — the "alive graph" form.
    /// The host stores the per-node image (a PNG data-URI) via [`Canvas::set_node_sprite`],
    /// which sets this face. The body (silhouette or a sprite-traced hull) is a separate axis.
    /// (Node body & face — sprite face.)
    Sprite,
    /// No texture: the bare content-typed state color alone, no favicon or caption. Minimal and
    /// cheap, for a dense graph or a node with nothing to texture. (Formerly `Shape`.)
    Bare,
}

impl Face {
    /// The persisted string code for the cartography sidecar (kept seiche/canvas-free downstream).
    /// (Node body & face — face persistence.)
    pub fn as_code(self) -> &'static str {
        match self {
            Face::Favicon => "favicon",
            Face::Sprite => "sprite",
            Face::Bare => "bare",
        }
    }

    /// Parse a persisted code, defaulting to [`Favicon`](Face::Favicon) for an unknown string.
    /// (Node body & face — face persistence.)
    pub fn from_code(code: &str) -> Self {
        match code {
            "sprite" => Face::Sprite,
            "bare" => Face::Bare,
            _ => Face::Favicon,
        }
    }
}
