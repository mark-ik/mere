//! Coupling + edge-path types — re-exported from the kernel field layer.
//!
//! All of these are kernel truth now (field-system extraction Phases 3a + 4): a
//! coupling targets a selector over nodes, and an edge-path rule says how an
//! edge's curve is drawn — neither is a node→node `EdgePayload`, both are
//! field-layer definitions from the portable `numen` crate. This module re-exports
//! them so `crate::coupling::*` and `quint::*` keep resolving onto the canonical
//! types; their tests live beside the definitions in `numen`.

pub use numen::{Coupling, CouplingResponse, EdgePath, EdgePathRule, NodeSelector};
