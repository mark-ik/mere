/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Destructive-action gate probe — every destructive `ActionDispatched`
//! must be preceded by a `ConfirmDialog` Confirmed dismissal.

use std::sync::Mutex;

use mere_kernel::actions::ActionId;

use crate::ux_observability::{DismissReason, SurfaceId, UxEvent};

use super::{ProbeFailure, UxProbe};

/// Asserts that every [`UxEvent::ActionDispatched`] for a destructive
/// `ActionId` is preceded (as the most recent ConfirmDialog event) by
/// a `ConfirmDialog` Confirmed dismissal. Covers the §4.10 guarantee
/// that destructive actions (Tombstone, Remove edge, ...) always carry
/// a confirmation step.
///
/// The probe is parameterised by the list of `ActionId`s the caller
/// considers destructive. Today the iced host marks `NodeMarkTombstone`
/// destructive in `items_for_target`; future destructive actions are
/// added by extending this list (and the corresponding
/// `ContextMenuEntry::destructive()` flag).
pub struct DestructiveActionGateProbe {
    destructive: Vec<ActionId>,
    /// True if the most recent ConfirmDialog event was a Confirmed
    /// dismissal. Cleared by Cancelled / Superseded dismissals or by
    /// any subsequent ActionDispatched (the grant is consumed).
    confirm_grant: Mutex<bool>,
    failures: Mutex<Vec<ProbeFailure>>,
}

impl DestructiveActionGateProbe {
    pub fn new(destructive: Vec<ActionId>) -> Self {
        Self {
            destructive,
            confirm_grant: Mutex::new(false),
            failures: Mutex::new(Vec::new()),
        }
    }

    /// Default wiring with the iced host's currently-known destructive
    /// actions. Extend this list as new destructive actions land.
    pub fn iced_default() -> Self {
        Self::new(vec![ActionId::NodeMarkTombstone])
    }
}

impl UxProbe for DestructiveActionGateProbe {
    fn name(&self) -> &'static str {
        "destructive_action_gate"
    }

    fn observe(&self, event: &UxEvent) {
        match event {
            UxEvent::SurfaceDismissed {
                surface: SurfaceId::ConfirmDialog,
                reason,
            } => {
                let mut grant = self.confirm_grant.lock().unwrap();
                *grant = matches!(reason, DismissReason::Confirmed);
            }
            UxEvent::ActionDispatched { action_id, .. } => {
                let mut grant = self.confirm_grant.lock().unwrap();
                if self.destructive.contains(action_id) && !*grant {
                    self.failures.lock().unwrap().push(ProbeFailure {
                        probe_name: self.name(),
                        description: format!(
                            "destructive action {:?} dispatched without a \
                             preceding ConfirmDialog Confirmed dismissal",
                            action_id
                        ),
                        triggering_event: event.clone(),
                    });
                }
                // Either way, consume the grant. A lingering grant must
                // not authorise a later, unrelated destructive dispatch.
                *grant = false;
            }
            _ => {}
        }
    }

    fn drain_failures(&self) -> Vec<ProbeFailure> {
        std::mem::take(&mut *self.failures.lock().unwrap())
    }
}
