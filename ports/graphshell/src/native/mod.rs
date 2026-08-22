//! Native authorities hosted by Graphshell.

pub mod app_admission;
pub mod app_broker;
#[cfg(test)]
mod app_broker_tests;
pub mod app_client;
pub mod browser_host;
#[cfg(feature = "personal-sync")]
pub mod carriage_host;
pub mod device_broker;
pub mod endpoint_catalog;
#[cfg(feature = "personal-sync")]
pub mod graph_keys;
pub mod identity_ui;
pub mod local_endpoint;
pub mod local_session;
pub mod personae_host;
#[cfg(feature = "personal-sync")]
pub mod personal_sync_host;
pub mod projection_host;
#[cfg(all(feature = "personal-sync", not(target_arch = "wasm32")))]
pub mod transfer_staging;
