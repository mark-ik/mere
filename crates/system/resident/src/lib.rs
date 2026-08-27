// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Narrow lifecycle mechanics shared by long-lived product residents.
//!
//! A resident's policies, resources, and receipts remain product-specific.
//! This crate holds only the proven common rule: shutdown is ordered, every
//! owner gets one close attempt, and later closes are attempted after an
//! earlier failure.

#![forbid(unsafe_code)]

use std::future::Future;
use std::pin::Pin;

/// One failed close attempt, named by its owning resident.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShutdownFailure {
    /// Stable resource name supplied by the composition root.
    pub resource: &'static str,
    /// Display form of the resource's error.
    pub error: String,
}

/// The complete result of a best-effort ordered shutdown.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShutdownReport {
    failures: Vec<ShutdownFailure>,
}

impl ShutdownReport {
    /// Whether every owned resource closed cleanly.
    pub fn is_clean(&self) -> bool {
        self.failures.is_empty()
    }

    /// Error reported by one named resource, when it failed to close.
    pub fn failure(&self, resource: &str) -> Option<&str> {
        self.failures
            .iter()
            .find(|failure| failure.resource == resource)
            .map(|failure| failure.error.as_str())
    }

    /// Consume the report into its complete list of failures.
    pub fn into_failures(self) -> Vec<ShutdownFailure> {
        self.failures
    }
}

/// Boxed asynchronous close operation, returned only when it is this
/// resource's turn to close.
pub type CloseFuture<'a> = Pin<Box<dyn Future<Output = Result<(), String>> + 'a>>;

/// One named close operation in shutdown order.
pub type CloseAction<'a> = Box<dyn FnOnce() -> CloseFuture<'a> + 'a>;

/// Attempt every close action in order and retain every refusal.
///
/// `CloseAction` delays construction of each future. Its captures therefore
/// drop before the next action begins, which makes resource order real rather
/// than merely the order in which futures were allocated.
pub async fn close_all<'a>(
    actions: impl IntoIterator<Item = (&'static str, CloseAction<'a>)>,
) -> ShutdownReport {
    let mut report = ShutdownReport::default();
    for (resource, close) in actions {
        if let Err(error) = close().await {
            report.failures.push(ShutdownFailure { resource, error });
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::ShutdownReport;

    #[test]
    fn empty_report_is_clean() {
        assert!(ShutdownReport::default().is_clean());
    }
}
