// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Productive-selection probe — Confirmed dismissal of a configured
//! surface must be followed by a "productive" event.

use std::sync::Mutex;

use crate::ux_observability::{DismissReason, SurfaceId, UxEvent};

use super::{ProbeFailure, UxProbe};

/// One outcome that satisfies a [`ProductiveRule`]. A rule is satisfied
/// when the next event after the configured surface's Confirmed dismissal
/// matches any of its outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductiveOutcome {
    /// Any [`UxEvent::ActionDispatched`] satisfies the rule. Use for
    /// surfaces whose only effect is dispatching a `HostIntent::Action`
    /// (Command Palette, Confirm Dialog).
    AnyAction,
    /// A [`UxEvent::SurfaceOpened`] for `surface` satisfies the rule.
    /// Use for surfaces that route to a successor modal (Context Menu's
    /// destructive path opens ConfirmDialog; Command Palette host-routed
    /// actions open NodeCreate / FrameRename).
    Open(SurfaceId),
    /// Any [`UxEvent::OpenNodeDispatched`] satisfies the rule. Use for
    /// surfaces whose effect is opening a node (Node Finder).
    OpenNode,
}

/// Pairs a surface with the set of outcomes that count as productive
/// when that surface emits a Confirmed dismissal.
#[derive(Debug, Clone)]
pub struct ProductiveRule {
    pub surface: SurfaceId,
    pub outcomes: Vec<ProductiveOutcome>,
}

impl ProductiveRule {
    pub fn new(surface: SurfaceId, outcomes: Vec<ProductiveOutcome>) -> Self {
        Self { surface, outcomes }
    }
}

/// Asserts that every Confirmed dismissal of a configured surface is
/// followed (as the very next observable event) by a matching
/// [`ProductiveOutcome`]. The strictness of "very next event" relies
/// on the host emitting Dismissed → productive in the same update arm,
/// which the iced host satisfies for all five gs::Modal-backed surfaces.
///
/// Cancelled / Superseded / Programmatic dismissals are ignored —
/// only Confirmed dismissals carry the productive expectation.
pub struct ProductiveSelectionProbe {
    rules: Vec<ProductiveRule>,
    pending: Mutex<Option<Pending>>,
    failures: Mutex<Vec<ProbeFailure>>,
}

struct Pending {
    rule_idx: usize,
    triggering_event: UxEvent,
}

impl ProductiveSelectionProbe {
    pub fn new(rules: Vec<ProductiveRule>) -> Self {
        Self {
            rules,
            pending: Mutex::new(None),
            failures: Mutex::new(Vec::new()),
        }
    }

    /// Default rule set wiring the four selection-shaped surfaces the
    /// iced host emits today: Command Palette and Confirm Dialog
    /// confirm via `ActionDispatched`; Node Finder via
    /// `OpenNodeDispatched`; Context Menu via either `ActionDispatched`
    /// (immediate path) or `SurfaceOpened { ConfirmDialog }`
    /// (destructive gate path).
    pub fn iced_default() -> Self {
        Self::new(vec![
            ProductiveRule::new(
                SurfaceId::CommandPalette,
                vec![
                    ProductiveOutcome::AnyAction,
                    ProductiveOutcome::Open(SurfaceId::NodeCreate),
                    ProductiveOutcome::Open(SurfaceId::FrameRename),
                    ProductiveOutcome::Open(SurfaceId::CommandPalette),
                ],
            ),
            ProductiveRule::new(SurfaceId::NodeFinder, vec![ProductiveOutcome::OpenNode]),
            ProductiveRule::new(SurfaceId::ConfirmDialog, vec![ProductiveOutcome::AnyAction]),
            ProductiveRule::new(
                SurfaceId::ContextMenu,
                vec![
                    ProductiveOutcome::AnyAction,
                    ProductiveOutcome::Open(SurfaceId::ConfirmDialog),
                ],
            ),
        ])
    }

    fn outcome_matches(outcome: ProductiveOutcome, event: &UxEvent) -> bool {
        match (outcome, event) {
            (ProductiveOutcome::AnyAction, UxEvent::ActionDispatched { .. }) => true,
            (ProductiveOutcome::OpenNode, UxEvent::OpenNodeDispatched { .. }) => true,
            (ProductiveOutcome::Open(target), UxEvent::SurfaceOpened { surface }) => {
                target == *surface
            }
            _ => false,
        }
    }
}

impl UxProbe for ProductiveSelectionProbe {
    fn name(&self) -> &'static str {
        "productive_selection"
    }

    fn observe(&self, event: &UxEvent) {
        // First: if a productive expectation is pending, check whether
        // *this* event satisfies it. If so, clear the pending slot. If
        // not, record a failure (the dismissal was unproductive).
        let mut pending = self.pending.lock().unwrap();
        if let Some(p) = pending.as_ref() {
            let rule = &self.rules[p.rule_idx];
            let satisfied = rule
                .outcomes
                .iter()
                .any(|o| Self::outcome_matches(*o, event));
            if satisfied {
                *pending = None;
            } else {
                self.failures.lock().unwrap().push(ProbeFailure {
                    probe_name: self.name(),
                    description: format!(
                        "{:?} Confirmed dismissal not followed by a productive \
                         event (saw {:?} instead of {:?})",
                        rule.surface, event, rule.outcomes
                    ),
                    triggering_event: p.triggering_event.clone(),
                });
                *pending = None;
            }
        }

        // Second: if this event is a Confirmed dismissal of a configured
        // surface, arm a new expectation for the next event.
        if let UxEvent::SurfaceDismissed {
            surface,
            reason: DismissReason::Confirmed,
        } = event
        {
            if let Some(rule_idx) = self.rules.iter().position(|r| r.surface == *surface) {
                *pending = Some(Pending {
                    rule_idx,
                    triggering_event: event.clone(),
                });
            }
        }
    }

    fn drain_failures(&self) -> Vec<ProbeFailure> {
        std::mem::take(&mut *self.failures.lock().unwrap())
    }
}
