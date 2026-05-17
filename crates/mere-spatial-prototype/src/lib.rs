// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! mere-spatial-prototype — Phase-3 proof harness for the spatial chrome
//! IR substrate.
//!
//! See [README](../README.md) for scope and framing. The TL;DR is: this
//! crate wires the renderer-registry contract end-to-end against a
//! minimal scene-graph data model so the substrate-as-host shape has a
//! running artifact, not just a brief.
//!
//! ## Quick example
//!
//! ```
//! use kurbo::Size;
//! use mere_renderer_registry::{
//!     NodeContentKind, NodeRenderer, RendererRegistry,
//! };
//! use mere_spatial_prototype::{
//!     RecordingRenderer, SubstrateHost, SubstrateNode, SubstrateScene,
//! };
//! use mere_renderer_registry::Placement;
//!
//! // Register a renderer that handles DocumentTile nodes.
//! let mut registry = RendererRegistry::with_default_selector();
//! registry
//!     .register(Box::new(RecordingRenderer::for_kind(
//!         "recording",
//!         NodeContentKind::DocumentTile,
//!     )))
//!     .unwrap();
//!
//! // Build a host and a scene with one placed node.
//! let mut host = SubstrateHost::new(registry);
//! let mut scene = SubstrateScene::new();
//! scene.insert(SubstrateNode::new(
//!     NodeContentKind::DocumentTile,
//!     Placement::translate(40.0, 60.0),
//!     Size::new(200.0, 120.0),
//! ));
//!
//! // Paint one frame.
//! let mut target = vello::Scene::new();
//! let report = host.paint_scene(&scene, &mut target, 1.0);
//! assert_eq!(report.painted, 1);
//! ```

#![warn(unused_crate_dependencies)]
#![warn(clippy::print_stdout, clippy::print_stderr)]

mod accessibility;
mod external_texture;
mod host;
mod lod;
mod recording_renderer;
mod scene;
mod solid_rect_renderer;
mod thumbnail;

pub use accessibility::{
    ROOT_NODE_ID, collect_accesskit_updates, project_scene, subtree_tree_id_for,
};
pub use external_texture::{CompositorError, ExternalTextureCompositor};
pub use host::{FrameReport, SubstrateHost};
pub use lod::{LodThresholds, compute_lod_for_node};
pub use recording_renderer::{PaintRecord, RecordingRenderer};
pub use scene::{
    DEFAULT_EDGE_HIT_TOLERANCE, EdgeIdentity, EdgeKind, EdgeStyle, RelationEdge, SceneHit,
    SubstrateNode, SubstrateScene,
};
pub use solid_rect_renderer::SolidRectRenderer;
pub use thumbnail::{ThumbnailFit, fit_camera};

// Re-export the registry so callers don't need a separate dep on it to
// name the trait types they implement.
pub use mere_renderer_registry;
