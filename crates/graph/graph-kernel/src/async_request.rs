// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Portable state machine for frame-bounded async requests.
//!
//! Shell-side session state often needs to track "a request is in
//! flight; when it completes, the result lands here" — the omnibar
//! provider mailbox, deferred intent resolution, clip-capture round
//! trips, etc. The pre-M4 pattern embedded `crossbeam_channel`
//! receivers inside the session state, which is fine on desktop but
//! doesn't compile to `wasm32-unknown-unknown` (crossbeam_channel
//! needs `std::thread`).
//!
//! `AsyncRequestState<T>` is the portable replacement: pure data, no
//! threading primitives inside. A host-side driver owns the actual
//! channel / future / fetch, and deposits results into this state
//! when ready. The shell-state side just reads and reacts.
//!
//! Concretely:
//!
//! - **Shell state** holds `AsyncRequestState<T>`. Calling code
//!   transitions via [`arm_pending`], [`resolve`], [`interrupt`],
//!   [`clear`]. Generation counter lets the state reject stale late
//!   results from requests that have since been superseded.
//! - **Host driver** (one per session or one per request kind) owns
//!   the concrete async machinery — `crossbeam_channel::Receiver<T>`
//!   / `tokio::sync::oneshot` / `futures::channel` / etc. Polls at
//!   frame boundaries and calls [`resolve`] / [`interrupt`] on the
//!   state it shepherds.
//!
//! The state type has no threading or time dependencies, so it is
//! safe to place inside types that belong in a portable shell-state
//! sub-crate targeting `wasm32-unknown-unknown` and `wasip2`.
//!
//! [`arm_pending`]: AsyncRequestState::arm_pending
//! [`resolve`]: AsyncRequestState::resolve
//! [`interrupt`]: AsyncRequestState::interrupt
//! [`clear`]: AsyncRequestState::clear

use serde::{Deserialize, Serialize};

/// Frame-bounded state of a host-driven async request.
///
/// Generic over the result payload `T`. The enum is always fully
/// deterministic — no internal threading, no hidden waiting. Hosts
/// bridge their concrete async primitives (crossbeam / tokio /
/// futures) into this state by calling the transition methods at
/// frame boundaries.
///
/// The `generation` counter inside `Pending` and `Ready` lets callers
/// distinguish a result for the current request from a late result
/// for a request that has been superseded. The pattern:
///
/// 1. Caller initiates request, calls [`arm_pending`]; state stores
///    `generation = N`.
/// 2. Caller initiates a newer request before the first completes;
///    state moves to `generation = N + 1`, the old request's result
///    is still en route on the host driver's side.
/// 3. The old result arrives at the host driver. Driver calls
///    [`resolve`] with `generation = N`; the state is at `N + 1`, so
///    the state rejects the stale value.
/// 4. The new result arrives. Driver calls [`resolve`] with
///    `generation = N + 1`; the state accepts and transitions to
///    `Ready { generation: N + 1, value }`.
///
/// [`arm_pending`]: Self::arm_pending
/// [`resolve`]: Self::resolve
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AsyncRequestState<T> {
    /// No request is in flight and no result is pending consumption.
    Idle,
    /// A request has been kicked off. `generation` uniquely identifies
    /// this request; a late result with a smaller generation is
    /// stale and should be dropped.
    Pending { generation: u64 },
    /// A result is ready and has not yet been consumed.
    Ready { generation: u64, value: T },
    /// The pending request was interrupted: the driver dropped its
    /// sender, the request was explicitly cancelled, or the state was
    /// re-armed before the result arrived. Treat as "the work happened
    /// or didn't, but don't wait for a value".
    Interrupted,
}

impl<T> Default for AsyncRequestState<T> {
    fn default() -> Self {
        Self::Idle
    }
}

impl<T> AsyncRequestState<T> {
    /// `true` when a request is in flight — the caller has armed the
    /// state and is awaiting the driver's resolution.
    pub fn is_pending(&self) -> bool {
        matches!(self, Self::Pending { .. })
    }

    /// `true` when a result is ready to be consumed via [`take`].
    ///
    /// [`take`]: Self::take
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }

    /// Current request generation, if any. Returns `None` for
    /// `Idle`/`Interrupted`.
    pub fn generation(&self) -> Option<u64> {
        match self {
            Self::Idle | Self::Interrupted => None,
            Self::Pending { generation } | Self::Ready { generation, .. } => Some(*generation),
        }
    }

    /// Transition to `Pending` with the supplied generation. Typically
    /// the caller tracks the next generation externally (e.g. an
    /// AtomicU64) and bumps on each new request so stale results from
    /// previous requests are rejected by [`resolve`].
    ///
    /// Returns the prior state so the caller can inspect whether a
    /// previous request was mid-flight (useful for logging /
    /// diagnostics).
    ///
    /// [`resolve`]: Self::resolve
    pub fn arm_pending(&mut self, generation: u64) -> Self
    where
        T: Clone,
    {
        let prior = self.clone();
        *self = Self::Pending { generation };
        prior
    }

    /// Deposit a result for a specific generation. If the state is
    /// `Pending { generation == requested }`, transitions to
    /// `Ready { generation, value }` and returns `true`. Otherwise
    /// (stale generation, idle, or already-ready), the value is
    /// discarded and returns `false` — the caller can use the return
    /// value for diagnostics.
    pub fn resolve(&mut self, generation: u64, value: T) -> bool {
        match self {
            Self::Pending {
                generation: pending,
            } if *pending == generation => {
                *self = Self::Ready { generation, value };
                true
            }
            _ => false,
        }
    }

    /// Mark the request as interrupted. Idempotent.
    pub fn interrupt(&mut self) {
        *self = Self::Interrupted;
    }

    /// Reset to `Idle`, discarding any in-flight request or pending
    /// result.
    pub fn clear(&mut self) {
        *self = Self::Idle;
    }

    /// Consume a ready result. Returns `Some(value)` and resets the
    /// state to `Idle` if a result was ready; `None` otherwise (the
    /// state is left unchanged in that case).
    pub fn take(&mut self) -> Option<T> {
        if matches!(self, Self::Ready { .. }) {
            match std::mem::replace(self, Self::Idle) {
                Self::Ready { value, .. } => Some(value),
                _ => unreachable!(),
            }
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_idle() {
        let state: AsyncRequestState<u32> = Default::default();
        assert!(matches!(state, AsyncRequestState::Idle));
    }

    #[test]
    fn arm_pending_transitions_from_idle_and_reports_prior() {
        let mut state: AsyncRequestState<u32> = AsyncRequestState::Idle;
        let prior = state.arm_pending(7);
        assert!(matches!(prior, AsyncRequestState::Idle));
        assert!(state.is_pending());
        assert_eq!(state.generation(), Some(7));
    }

    #[test]
    fn arm_pending_bumps_generation_and_reports_prior_pending() {
        let mut state: AsyncRequestState<u32> = AsyncRequestState::Idle;
        state.arm_pending(7);
        let prior = state.arm_pending(8);
        assert!(matches!(
            prior,
            AsyncRequestState::Pending { generation: 7 }
        ));
        assert_eq!(state.generation(), Some(8));
    }

    #[test]
    fn resolve_accepts_matching_generation() {
        let mut state: AsyncRequestState<u32> = AsyncRequestState::Idle;
        state.arm_pending(3);
        let accepted = state.resolve(3, 42);
        assert!(accepted);
        assert!(state.is_ready());
        assert_eq!(state.take(), Some(42));
        assert!(matches!(state, AsyncRequestState::Idle));
    }

    #[test]
    fn resolve_rejects_stale_generation() {
        // Classic stale-late-result case: request 1 goes out, caller
        // supersedes with request 2, then request 1's result arrives.
        // State should discard request 1's value.
        let mut state: AsyncRequestState<u32> = AsyncRequestState::Idle;
        state.arm_pending(1);
        state.arm_pending(2);
        let accepted = state.resolve(1, 42);
        assert!(!accepted);
        assert!(state.is_pending());
        assert_eq!(state.generation(), Some(2));
    }

    #[test]
    fn resolve_rejects_on_idle() {
        let mut state: AsyncRequestState<u32> = AsyncRequestState::Idle;
        let accepted = state.resolve(1, 42);
        assert!(!accepted);
        assert!(matches!(state, AsyncRequestState::Idle));
    }

    #[test]
    fn interrupt_moves_pending_to_interrupted() {
        let mut state: AsyncRequestState<u32> = AsyncRequestState::Idle;
        state.arm_pending(5);
        state.interrupt();
        assert!(matches!(state, AsyncRequestState::Interrupted));
        assert_eq!(state.generation(), None);
    }

    #[test]
    fn clear_from_any_state_returns_to_idle() {
        let mut state: AsyncRequestState<u32> = AsyncRequestState::Idle;
        state.arm_pending(1);
        state.resolve(1, 42);
        state.clear();
        assert!(matches!(state, AsyncRequestState::Idle));
    }

    #[test]
    fn take_without_ready_leaves_state_unchanged() {
        let mut state: AsyncRequestState<u32> = AsyncRequestState::Idle;
        state.arm_pending(1);
        assert_eq!(state.take(), None);
        assert!(state.is_pending());
    }

    #[test]
    fn serde_json_round_trips_each_variant() {
        let variants: Vec<AsyncRequestState<String>> = vec![
            AsyncRequestState::Idle,
            AsyncRequestState::Pending { generation: 42 },
            AsyncRequestState::Ready {
                generation: 42,
                value: "result".to_string(),
            },
            AsyncRequestState::Interrupted,
        ];
        for state in variants {
            let encoded = serde_json::to_string(&state).expect("serialize");
            let decoded: AsyncRequestState<String> =
                serde_json::from_str(&encoded).expect("deserialize");
            assert_eq!(decoded, state);
        }
    }

    #[test]
    fn stale_late_result_example_flow() {
        // End-to-end walk of the stale-generation scenario from the
        // doc comment, pinned as a test so the documented contract
        // stays honest.
        let mut state: AsyncRequestState<String> = AsyncRequestState::Idle;

        // 1. Caller arms for generation 10.
        state.arm_pending(10);
        assert_eq!(state.generation(), Some(10));

        // 2. Before 10's result arrives, a newer request supersedes
        //    it at generation 11.
        state.arm_pending(11);

        // 3. 10's result arrives late on the host driver. Rejected.
        let accepted_stale = state.resolve(10, "old".to_string());
        assert!(!accepted_stale);
        assert_eq!(state.generation(), Some(11));

        // 4. 11's result arrives. Accepted.
        let accepted_fresh = state.resolve(11, "new".to_string());
        assert!(accepted_fresh);
        assert_eq!(state.take().as_deref(), Some("new"));
    }
}
