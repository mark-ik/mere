//! Sceno: core contracts for the scenograph projection engine.
//!
//! The stem of the family. Sceno owns the vocabulary every other member
//! speaks: stable source references, coordinate spaces, footprints, scene
//! snapshots with per-instance identity, and (arriving with later proofs)
//! scores and action intents.
//!
//! The pipeline the family realizes:
//!
//! ```text
//! source data + relationships + signals + score
//!     → select and derive
//!     → map data to visual channels
//!     → solve placement          (scenomise — choreography)
//!     → produce an interactive scene
//!     → route gestures back as authorized intents   (scenotime — the
//!       inhabited scene)
//! ```
//!
//! Contract commitments, in force from this first slice:
//!
//! - **Sources keep their native truth behind adapters.** A [`SourceRef`]
//!   is opaque here; what is shared is the scene contract, not a data
//!   model.
//! - **Source and instance are different identities.** One source, many
//!   placed instances, structurally (`sources` interned, `items` point in).
//! - **Identity is an index.** Dense vectors; ids index them.
//! - **The representation measures; the projection places.** Scenes carry
//!   representation slots and extents ([`Footprint`]), never content;
//!   [`Measurements`] carry content needs the other way.
//!
//! No product, engine, or GPU dependencies belong here.

pub mod footprint;
pub mod geometry;
pub mod measure;
pub mod scene;
pub mod score;

pub use footprint::Footprint;
pub use geometry::{Rect, Size2, Transform2, Vec2};
pub use measure::{Measurement, Measurements};
pub use scene::{
    InstanceId, ProjectedItem, Region, Representation, RoutedRelation, Scene, SourceIx, SourceRef,
    Space, SpaceId,
};
pub use score::{
    Arrangement, Board, Geographic, Placement, SCORE_VERSION, Score, ScoreItem, Spiral, SpiralCurve,
};
