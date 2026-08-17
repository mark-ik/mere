//! Sceno: core contracts for the scenograph projection engine.
//!
//! The stem of the family. Sceno owns the vocabulary every other member
//! speaks: stable source references, coordinate spaces, footprints, scene
//! snapshots with per-instance identity, and persisted scores.
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
//! - **The representation measures; the projection places.** A host measures
//!   its content and stamps the result on [`ScoreItem::footprint`]; solvers
//!   read that extent and clear it. Scenes carry representation slots and
//!   assigned extents, never rendered content.
//!
//! No product, engine, or GPU dependencies belong here.
//!
//! # What is deliberately absent
//!
//! **Action intents.** A gesture's reverse path is not modelled here, and
//! this is a decision rather than a gap. Sceno owns instance identity,
//! `scenotime` owns epoch and revision identity, the consuming protocol owns
//! the intent triple (advertise, invoke, result), and the product's own gate
//! owns authority. Graphshell's protocol layer demonstrates the split: it
//! binds an invocation to a [`InstanceId`] plus the epoch and revision it was
//! observed at, which is what lets it answer "stale" at all. An intent
//! vocabulary here could only restate identities the contract already
//! supplies, and would then have to agree with the one that exists.
//!
//! The engine-side story is a hit shape plus a product-owned action id. See
//! [`ProjectedItem::hit`] for the first half and `scenotime`'s pick for the
//! resolution that turns a point into the [`InstanceId`] an intent names.

pub mod footprint;
pub mod geometry;
pub mod scene;
pub mod score;

pub use footprint::Footprint;
pub use geometry::{Rect, Size2, Transform2, Vec2};
pub use scene::{
    InstanceId, ProjectedItem, Region, Representation, RoutedRelation, Scene, SourceIx, SourceRef,
    Space, SpaceId,
};
pub use score::{
    Arrangement, Board, Geographic, HeldPlacement, Hold, HonoredHold, Hulls, Placement,
    SCORE_VERSION, Score, ScoreItem, Spiral, SpiralCurve,
};
