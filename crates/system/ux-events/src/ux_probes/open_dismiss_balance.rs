// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Open/Dismiss balance probe — every Opened eventually gets Dismissed.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::ux_observability::{SurfaceId, UxEvent};

use super::{ProbeFailure, UxProbe};

/// Asserts that every `SurfaceOpened` for a given surface gets
/// matched by a `SurfaceDismissed` before the same surface is
/// re-opened. Catches leaks where a dismissal path is forgotten.
///
/// This probe *flags on re-open*, not on stream end (the stream
/// never explicitly ends). To check terminal balance, call
/// [`Self::pending_opens`] after running messages: any non-zero
/// count means a surface is still open.
pub struct OpenDismissBalanceProbe {
    open_counts: Mutex<HashMap<SurfaceId, u32>>,
    failures: Mutex<Vec<ProbeFailure>>,
}

impl OpenDismissBalanceProbe {
    pub fn new() -> Self {
        Self {
            open_counts: Mutex::new(HashMap::new()),
            failures: Mutex::new(Vec::new()),
        }
    }

    /// Snapshot the current per-surface open count (Opens minus
    /// Dismisses). A non-zero entry means that surface is currently
    /// open — useful for terminal-balance assertions in tests.
    pub fn pending_opens(&self) -> HashMap<SurfaceId, u32> {
        self.open_counts
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, c)| **c > 0)
            .map(|(s, c)| (*s, *c))
            .collect()
    }
}

impl Default for OpenDismissBalanceProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl UxProbe for OpenDismissBalanceProbe {
    fn name(&self) -> &'static str {
        "open_dismiss_balance"
    }

    fn observe(&self, event: &UxEvent) {
        match event {
            UxEvent::SurfaceOpened { surface } => {
                let mut counts = self.open_counts.lock().unwrap();
                let entry = counts.entry(*surface).or_insert(0);
                if *entry > 0 {
                    self.failures.lock().unwrap().push(ProbeFailure {
                        probe_name: self.name(),
                        description: format!(
                            "{:?} opened again while previous open is unmatched",
                            surface
                        ),
                        triggering_event: event.clone(),
                    });
                }
                *entry += 1;
            }
            UxEvent::SurfaceDismissed { surface, .. } => {
                let mut counts = self.open_counts.lock().unwrap();
                let entry = counts.entry(*surface).or_insert(0);
                if *entry == 0 {
                    self.failures.lock().unwrap().push(ProbeFailure {
                        probe_name: self.name(),
                        description: format!("{:?} dismissed without a matching open", surface),
                        triggering_event: event.clone(),
                    });
                } else {
                    *entry -= 1;
                }
            }
            _ => {}
        }
    }

    fn drain_failures(&self) -> Vec<ProbeFailure> {
        std::mem::take(&mut *self.failures.lock().unwrap())
    }
}
