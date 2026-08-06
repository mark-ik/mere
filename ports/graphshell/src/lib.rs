//! Graphshell's presentation host.
//!
//! G1 adds a native semantic view over the portable client state. Networking,
//! product models, and source authority remain injected at the edge.

#[cfg(feature = "web")]
pub mod access;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod admission;
#[cfg(feature = "web")]
pub mod app;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod browser_carrier;
#[cfg(feature = "web")]
pub mod browser_storage;
pub mod canary;
#[cfg(feature = "web")]
pub mod capture;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod carrier;
#[cfg(feature = "web")]
pub mod handlers;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod identity;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod identity_endpoint;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod identity_projection;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod lifecycle;
#[cfg(feature = "web")]
pub mod mere_host;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod native;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod network_carrier;
#[cfg(all(feature = "personal-sync", not(target_arch = "wasm32")))]
pub mod personal_sync;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod policy_projection;
#[cfg(feature = "web")]
pub mod product;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod profile;
pub mod resume;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod session_loop;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod session_notices;
pub mod sessions;
#[cfg(feature = "web")]
pub mod transfer;
#[cfg(feature = "web")]
pub mod transfer_endpoint;
#[cfg(all(feature = "personal-sync", not(target_arch = "wasm32")))]
pub mod transfer_offer;
pub mod view;

pub use graphshell_client as client;
pub use graphshell_endpoint as endpoint;
pub use graphshell_protocol as protocol;
