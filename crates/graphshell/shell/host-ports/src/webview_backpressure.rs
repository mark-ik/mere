/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Host-neutral metadata view and full state for webview/viewer creation
//! backpressure.
//!
//! 2026-04-26: retry/cooldown core [`WebviewAttachRetryState`] added.
//! 2026-04-27: portable probe state [`WebviewCreationProbeState`] and
//! full per-node state [`WebviewCreationBackpressureState`] added.
//! The probe uses [`ViewerSurfaceId`] for the viewer identity (shell adapter
//! converts from `servo::WebViewId` via the renderer-id registry) and
//! [`PortableInstant`] for time values, keeping this module free of
//! host-specific clocks and Servo types.

use kernel::graph::NodeKey;
use kernel::time::PortableInstant;

use crate::ports::ViewerSurfaceId;

/// Host-neutral per-node attach-attempt metadata.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NodePaneAttachAttemptMetadata {
    pub retry_count: u8,
    pub pending_attempt_age_ms: Option<u64>,
    pub cooldown_remaining_ms: Option<u64>,
}

impl NodePaneAttachAttemptMetadata {
    /// True when the metadata carries no retry, pending, or cooldown signal.
    pub const fn is_empty(self) -> bool {
        self.retry_count == 0
            && self.pending_attempt_age_ms.is_none()
            && self.cooldown_remaining_ms.is_none()
    }
}

/// Source of host-neutral attach-attempt metadata.
pub trait RuntimeWebviewBackpressureMetadataSource {
    /// Collect the current non-empty metadata records keyed by node.
    fn node_pane_attach_attempt_metadata(&self) -> Vec<(NodeKey, NodePaneAttachAttemptMetadata)>;
}

/// Host-neutral retry/cooldown core for webview attach attempts.
///
/// Tracks the count of retries observed and the current step in the
/// exponential backoff schedule. The shell composes this into its
/// per-node backpressure record alongside a Servo-typed pending probe
/// (`Option<WebviewCreationProbe>`) and a wall-clock deadline
/// (`Option<std::time::Instant>`). Those two members keep the shell-side
/// concerns (host windows, Servo IDs, deadline arithmetic) where they
/// belong.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WebviewAttachRetryState {
    pub retry_count: u8,
    pub cooldown_step: usize,
}

impl WebviewAttachRetryState {
    /// Maximum retries before the shell arms a cooldown.
    pub const MAX_RETRIES: u8 = 3;

    /// Lower bound of the cooldown delay schedule, in milliseconds.
    pub const COOLDOWN_MIN_MS: u64 = 1_000;

    /// Upper bound of the cooldown delay schedule, in milliseconds.
    pub const COOLDOWN_MAX_MS: u64 = 30_000;

    /// Cap on the exponential step that the schedule walks before saturating
    /// at [`Self::COOLDOWN_MAX_MS`].
    pub const COOLDOWN_MAX_STEP: usize = 8;

    /// Compute the cooldown delay in milliseconds for a given exponential
    /// step. Mirrors `min * 2^step`, clamped to the `[MIN, MAX]` band, with
    /// the step itself capped at [`Self::COOLDOWN_MAX_STEP`].
    pub fn cooldown_delay_ms_for_step(step: usize) -> u64 {
        let capped_step = step.min(Self::COOLDOWN_MAX_STEP);
        let scale = 1u64.checked_shl(capped_step as u32).unwrap_or(u64::MAX);
        Self::COOLDOWN_MIN_MS
            .saturating_mul(scale)
            .min(Self::COOLDOWN_MAX_MS)
            .max(Self::COOLDOWN_MIN_MS)
    }

    /// Cooldown delay in milliseconds for the current step (without
    /// advancing).
    pub fn cooldown_delay_ms(&self) -> u64 {
        Self::cooldown_delay_ms_for_step(self.cooldown_step)
    }

    /// Advance the cooldown step by one (saturating). Returns the delay
    /// in milliseconds that the *just-armed* cooldown corresponds to —
    /// i.e., the value computed from the pre-advance step.
    pub fn advance_cooldown_step(&mut self) -> u64 {
        let delay = self.cooldown_delay_ms();
        self.cooldown_step = self.cooldown_step.saturating_add(1);
        delay
    }

    /// Increment the retry counter (saturating).
    pub fn record_attempt(&mut self) {
        self.retry_count = self.retry_count.saturating_add(1);
    }

    /// True once the retry counter has reached [`Self::MAX_RETRIES`].
    pub const fn is_retry_exhausted(&self) -> bool {
        self.retry_count >= Self::MAX_RETRIES
    }

    /// Reset both retry counter and cooldown step to their initial values.
    pub fn reset(&mut self) {
        self.retry_count = 0;
        self.cooldown_step = 0;
    }

    /// Reset only the retry counter, leaving the cooldown step intact.
    pub fn reset_retry_count(&mut self) {
        self.retry_count = 0;
    }
}

/// Portable pending-probe record for a webview/viewer creation attempt.
///
/// The probe tracks which viewer surface is being waited on and when the
/// attempt started. The shell adapter converts `servo::WebViewId` to
/// [`ViewerSurfaceId`] via the renderer-id registry before storing it here,
/// and reverses the conversion when consuming the probe in the reconcile pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WebviewCreationProbeState {
    /// Host-neutral identity of the in-flight viewer surface. Packed as a
    /// `u64` renderer-id via [`ViewerSurfaceId::from_u64`] / [`ViewerSurfaceId::as_u64`].
    pub viewer_surface_id: ViewerSurfaceId,
    /// Monotonic start time for timeout / age calculations (ms from app start).
    pub started_at: PortableInstant,
}

/// Full portable per-node state for webview/viewer creation backpressure.
///
/// Composed of:
/// - [`WebviewAttachRetryState`] — retry counter + cooldown step (no time refs)
/// - An optional pending probe ([`WebviewCreationProbeState`])
/// - An optional cooldown deadline ([`PortableInstant`])
/// - A `cooldown_notified` flag that suppresses redundant `MarkRuntimeBlocked`
///   intents across frames while in the same cooldown window
///
/// The shell keeps Servo-coupled logic (webview creation, `window.contains_webview`,
/// `MarkRuntimeBlocked` intent construction with `std::time::Instant`) in its
/// adapter layer; this struct stores only portable data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WebviewCreationBackpressureState {
    pub retry: WebviewAttachRetryState,
    /// Pending probe waiting for confirmation or timeout. `None` when idle or
    /// in cooldown.
    pub pending: Option<WebviewCreationProbeState>,
    /// Earliest time (ms from app start) at which the next creation attempt
    /// is allowed. `None` when not in cooldown.
    pub cooldown_until: Option<PortableInstant>,
    /// True once a `MarkRuntimeBlocked` intent has been pushed for the current
    /// cooldown window. Reset to `false` whenever `cooldown_until` is armed or
    /// cleared so the shell pushes exactly one intent per cooldown period.
    pub cooldown_notified: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_metadata_reports_empty() {
        assert!(NodePaneAttachAttemptMetadata::default().is_empty());
    }

    #[test]
    fn populated_metadata_reports_non_empty() {
        assert!(
            !NodePaneAttachAttemptMetadata {
                retry_count: 0,
                pending_attempt_age_ms: Some(5),
                cooldown_remaining_ms: None,
            }
            .is_empty()
        );
    }

    #[test]
    fn cooldown_delay_is_bounded() {
        assert_eq!(
            WebviewAttachRetryState::cooldown_delay_ms_for_step(0),
            WebviewAttachRetryState::COOLDOWN_MIN_MS
        );
        let max_step_delay = WebviewAttachRetryState::cooldown_delay_ms_for_step(usize::MAX);
        assert!(max_step_delay >= WebviewAttachRetryState::COOLDOWN_MIN_MS);
        assert!(max_step_delay <= WebviewAttachRetryState::COOLDOWN_MAX_MS);
    }

    #[test]
    fn cooldown_delay_doubles_until_capped() {
        let s0 = WebviewAttachRetryState::cooldown_delay_ms_for_step(0);
        let s1 = WebviewAttachRetryState::cooldown_delay_ms_for_step(1);
        let s2 = WebviewAttachRetryState::cooldown_delay_ms_for_step(2);
        assert_eq!(s0, WebviewAttachRetryState::COOLDOWN_MIN_MS);
        assert_eq!(s1, 2 * WebviewAttachRetryState::COOLDOWN_MIN_MS);
        assert_eq!(s2, 4 * WebviewAttachRetryState::COOLDOWN_MIN_MS);
        let saturated = WebviewAttachRetryState::cooldown_delay_ms_for_step(
            WebviewAttachRetryState::COOLDOWN_MAX_STEP,
        );
        assert_eq!(saturated, WebviewAttachRetryState::COOLDOWN_MAX_MS);
    }

    #[test]
    fn advance_cooldown_step_returns_pre_advance_delay() {
        let mut state = WebviewAttachRetryState::default();
        let d0 = state.advance_cooldown_step();
        assert_eq!(d0, WebviewAttachRetryState::COOLDOWN_MIN_MS);
        assert_eq!(state.cooldown_step, 1);
        let d1 = state.advance_cooldown_step();
        assert_eq!(d1, 2 * WebviewAttachRetryState::COOLDOWN_MIN_MS);
        assert_eq!(state.cooldown_step, 2);
    }

    #[test]
    fn record_attempt_saturates_at_u8_max() {
        let mut state = WebviewAttachRetryState::default();
        for _ in 0..u16::from(u8::MAX) + 5 {
            state.record_attempt();
        }
        assert_eq!(state.retry_count, u8::MAX);
    }

    #[test]
    fn is_retry_exhausted_at_max_retries() {
        let mut state = WebviewAttachRetryState::default();
        for _ in 0..WebviewAttachRetryState::MAX_RETRIES {
            assert!(!state.is_retry_exhausted());
            state.record_attempt();
        }
        assert!(state.is_retry_exhausted());
    }

    #[test]
    fn reset_clears_both_counters() {
        let mut state = WebviewAttachRetryState {
            retry_count: 2,
            cooldown_step: 5,
        };
        state.reset();
        assert_eq!(state, WebviewAttachRetryState::default());
    }

    #[test]
    fn reset_retry_count_preserves_cooldown_step() {
        let mut state = WebviewAttachRetryState {
            retry_count: 2,
            cooldown_step: 5,
        };
        state.reset_retry_count();
        assert_eq!(state.retry_count, 0);
        assert_eq!(state.cooldown_step, 5);
    }

    #[test]
    fn backpressure_state_default_is_idle() {
        let state = WebviewCreationBackpressureState::default();
        assert!(state.pending.is_none());
        assert!(state.cooldown_until.is_none());
        assert!(!state.cooldown_notified);
        assert_eq!(state.retry, WebviewAttachRetryState::default());
    }

    #[test]
    fn probe_state_viewer_surface_id_roundtrip_via_u64() {
        let original = ViewerSurfaceId::new(7, 42);
        let probe = WebviewCreationProbeState {
            viewer_surface_id: original,
            started_at: PortableInstant(1_000),
        };
        // Verify the u64 round-trip the shell adapter uses.
        let packed = probe.viewer_surface_id.as_u64();
        let recovered = ViewerSurfaceId::from_u64(packed);
        assert_eq!(recovered, original);
    }

    #[test]
    fn cooldown_until_ordering_reflects_ms_comparison() {
        let earlier = PortableInstant(500);
        let later = PortableInstant(1_500);
        assert!(earlier < later);

        let mut state = WebviewCreationBackpressureState::default();
        state.cooldown_until = Some(later);
        // Simulated "is cooldown still active?" check.
        assert!(earlier < state.cooldown_until.unwrap());
        assert!(later >= state.cooldown_until.unwrap());
    }
}
