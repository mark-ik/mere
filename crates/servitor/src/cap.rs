//! Compatibility re-exports for Servitor's original module path.
//!
//! The algebra itself lives in the dependency-free `mere-capability` leaf
//! crate so Servitor and Gemot consume one definition.

pub use capability::{
    Cap, CapError, Capability, FacetNamespace, Mode, ScopePath, assert_capability_laws,
};
