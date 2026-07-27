//! Graphshell's presentation host.
//!
//! G1 adds a native semantic view over the portable client state. Networking,
//! product models, and source authority remain injected at the edge.

pub mod admission;
pub mod canary;
#[cfg(not(target_arch = "wasm32"))]
pub mod carrier;
pub mod profile;
pub mod resume;
#[cfg(not(target_arch = "wasm32"))]
pub mod sessions;
pub mod view;

pub use graphshell_client as client;
pub use graphshell_endpoint as endpoint;
pub use graphshell_protocol as protocol;
