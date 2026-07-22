//! Sceno: core contracts for the scenograph projection engine.
//!
//! The stem of the family. Sceno owns the vocabulary every other member
//! speaks: stable source references, scores (serialized projection settings),
//! visual channels, coordinate spaces, footprints (point, rectangle, polygon,
//! path), scene snapshots with per-instance identity, and action intents that
//! route gestures back to the authority that owns the fact.
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
//! Sources keep their native truth behind adapters; what is shared is the
//! scene contract, not a data model. No product, engine, or GPU dependencies
//! belong here.
//!
//! Name reservation: the contract lands with its first consumers.
