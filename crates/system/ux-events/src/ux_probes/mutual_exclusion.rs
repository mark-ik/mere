/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Mutual-exclusion probe — at most one modal-like surface open at a time.

use std::sync::Mutex;

use crate::ux_observability::{SurfaceId, UxEvent};

use super::{ProbeFailure, UxProbe};

/// The set of surfaces that must form a mutually-exclusive group:
/// at most one of these may be open at any given instant. Other
/// surfaces (StatusBar, NavigatorHost, panes) are always present and
/// not subject to this rule.
fn is_modal_like(surface: SurfaceId) -> bool {
    matches!(
        surface,
        SurfaceId::CommandPalette
            | SurfaceId::NodeFinder
            | SurfaceId::ContextMenu
            | SurfaceId::ConfirmDialog
            | SurfaceId::NodeCreate
            | SurfaceId::FrameRename
    )
}

/// Asserts that at most one modal-like surface is open at a time.
/// The iced host's "dismiss-before-open" supersession sequencing
/// satisfies this — opening a second modal must emit a dismissal of
/// the prior one *first*.
pub struct MutualExclusionProbe {
    open_modals: Mutex<Vec<SurfaceId>>,
    failures: Mutex<Vec<ProbeFailure>>,
}

impl MutualExclusionProbe {
    pub fn new() -> Self {
        Self {
            open_modals: Mutex::new(Vec::new()),
            failures: Mutex::new(Vec::new()),
        }
    }
}

impl Default for MutualExclusionProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl UxProbe for MutualExclusionProbe {
    fn name(&self) -> &'static str {
        "mutual_exclusion"
    }

    fn observe(&self, event: &UxEvent) {
        match event {
            UxEvent::SurfaceOpened { surface } if is_modal_like(*surface) => {
                let mut open = self.open_modals.lock().unwrap();
                if !open.is_empty() {
                    self.failures.lock().unwrap().push(ProbeFailure {
                        probe_name: self.name(),
                        description: format!("opened {:?} while {:?} still open", surface, open),
                        triggering_event: event.clone(),
                    });
                }
                open.push(*surface);
            }
            UxEvent::SurfaceDismissed { surface, .. } if is_modal_like(*surface) => {
                let mut open = self.open_modals.lock().unwrap();
                if let Some(pos) = open.iter().position(|s| s == surface) {
                    open.swap_remove(pos);
                }
            }
            _ => {}
        }
    }

    fn drain_failures(&self) -> Vec<ProbeFailure> {
        std::mem::take(&mut *self.failures.lock().unwrap())
    }
}
