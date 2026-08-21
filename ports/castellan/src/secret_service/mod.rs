// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! [Freedesktop Secret Service 0.2] over Castellan's resident sealed storage.
//!
//! The portable types and collection store live on every target so their
//! policy and persistence can be tested without a bus. [`serve`] is the Linux
//! session-bus adapter. It implements the standard `org.freedesktop.secrets`
//! object tree and the strongly recommended `plain` transfer session.
//!
//! [Freedesktop Secret Service 0.2]: https://specifications.freedesktop.org/secret-service/latest/

mod store;

#[cfg(target_os = "linux")]
mod dbus;

pub use store::{
    NewSecretItem, SecretCollection, SecretCollectionId, SecretItem, SecretItemId,
    SecretServiceError, SecretServiceLimits, SecretServiceStore,
};

#[cfg(target_os = "linux")]
pub use dbus::{
    SecretServiceAccessPolicy, SecretServiceCaller, SecretServiceOperation, SecretServiceServer,
    SecretServiceStartError, serve,
};
