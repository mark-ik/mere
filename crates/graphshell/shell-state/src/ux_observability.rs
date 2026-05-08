/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Host-neutral UX observability layer.
//!
//! Defines a portable `UxEvent` taxonomy and an `UxObserver` trait so
//! every Graphshell host (iced today, future Stage-G/Stage-H hosts
//! later) emits the same observable events when chrome surfaces open,
//! dismiss, and dispatch intents. Two built-in observers ship in this
//! module:
//!
//! - [`CountingObserver`] — atomic counters indexed by surface +
//!   event type. Cheap; safe to keep around for the lifetime of the
//!   runtime; readable from diagnostics panes / status bar /
//!   provenance traces without locking.
//! - [`RecordingObserver`] — appends every observed event to a
//!   bounded ring. Used by tests, by the diagnostics-pane "recent
//!   events" view, and by `UxProbe` adapters that want a replayable
//!   stream.
//!
//! The split between *observers* (this module — passive listeners)
//! and *probes* (assertion-shaped consumers, future module) maps onto
//! the [§4.10 graph coherence guarantee verification](
//! ../../../../../graphshell/design_docs/graphshell_docs/implementation_strategy/shell/2026-04-28_iced_jump_ship_plan.md):
//! observers record what happened; probes assert that what happened
//! satisfies a named invariant.
//!
//! ## Idiomatic iced wiring
//!
//! The iced host registers observers on `GraphshellRuntime::ux_observers`
//! at startup and emits events from `update()` arms whenever a chrome
//! surface transitions. Other hosts (egui today, future hosts later)
//! follow the same pattern — the trait and event taxonomy are the
//! portable seam. The iced host can additionally bridge events into
//! a `Subscription` to stream them into the StatusBar slot, but
//! that's host-local sugar, not part of the portable contract.
//!
//! ## Extensibility
//!
//! - **New surface**: add a variant to [`SurfaceId`].
//! - **New event shape**: add a variant to [`UxEvent`].
//! - **New observer behavior**: implement [`UxObserver`] in any
//!   crate; register it on the runtime's [`UxObservers`] collection.
//!
//! Observers are intentionally stateful and `Send + Sync` so future
//! parallel hosts can register thread-safe observers (e.g.,
//! background-task tracers). The interior mutability is each
//! observer's responsibility — the trait method is `&self`.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

pub use graphshell_core::accessibility::SurfaceId;

use graphshell_core::actions::ActionId;
use graphshell_core::graph::NodeKey;

/// Why a surface dismissed. Lets observers distinguish "user
/// confirmed" from "user cancelled" from "system superseded" without
/// reaching into surface-specific state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DismissReason {
    /// User clicked the surface's primary affirmative action
    /// (Confirm in a dialog, Enter / row-click in a list).
    Confirmed,
    /// User clicked Cancel / clicked outside / pressed Escape.
    Cancelled,
    /// Another surface preempted this one (mutual exclusion).
    Superseded,
    /// Surface dismissed itself programmatically (e.g., on a
    /// non-acting selection).
    Programmatic,
}

/// One observable UX event emitted by a host. The taxonomy stays
/// host-neutral — host-specific details (widget ids, frame timings)
/// belong on host-side decorators, not here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UxEvent {
    /// A chrome surface opened.
    SurfaceOpened { surface: SurfaceId },
    /// A chrome surface dismissed for the given reason.
    SurfaceDismissed {
        surface: SurfaceId,
        reason: DismissReason,
    },
    /// `HostIntent::Action` (or `ActionOnNode`) was dispatched. The
    /// optional `target` is the resolved NodeKey when the dispatch
    /// pre-positioned focus.
    ActionDispatched {
        action_id: ActionId,
        target: Option<NodeKey>,
    },
    /// `HostIntent::OpenNode` was dispatched.
    OpenNodeDispatched { node_key: NodeKey },
}

/// Trait every UX observer implements. Observers are `Send + Sync`
/// so future parallel hosts can register thread-safe consumers; the
/// `observe` method is `&self` and observers manage their own
/// interior mutability.
pub trait UxObserver: Send + Sync {
    fn observe(&self, event: &UxEvent);
}

/// Collection of observers attached to a runtime. Hosts emit events
/// by calling [`Self::emit`] — every registered observer sees the
/// event in registration order. Cloning an `UxObservers` clones the
/// observer list (each observer is `Arc`-shared internally where
/// shared state matters).
#[derive(Default)]
pub struct UxObservers {
    observers: Vec<Box<dyn UxObserver>>,
}

impl UxObservers {
    pub fn new() -> Self {
        Self {
            observers: Vec::new(),
        }
    }

    pub fn register(&mut self, observer: Box<dyn UxObserver>) {
        self.observers.push(observer);
    }

    pub fn emit(&self, event: UxEvent) {
        for observer in &self.observers {
            observer.observe(&event);
        }
    }

    pub fn observer_count(&self) -> usize {
        self.observers.len()
    }
}

impl std::fmt::Debug for UxObservers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UxObservers")
            .field("observer_count", &self.observers.len())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// CountingObserver — atomic counters indexed by event kind
// ---------------------------------------------------------------------------

/// Lock-free counter observer. Increments per-event-kind atomic
/// counters; readers can sample the counters at any time without
/// locking (each read is a relaxed atomic load).
///
/// Use this for diagnostics-pane summary rows, status-bar indicators,
/// and provenance hashes that need a stable "how many of X have
/// fired since runtime start" number.
#[derive(Debug, Default)]
pub struct CountingObserver {
    pub surfaces_opened: AtomicU64,
    pub surfaces_dismissed: AtomicU64,
    pub actions_dispatched: AtomicU64,
    pub open_nodes_dispatched: AtomicU64,
}

impl CountingObserver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn surfaces_opened(&self) -> u64 {
        self.surfaces_opened.load(Ordering::Relaxed)
    }

    pub fn surfaces_dismissed(&self) -> u64 {
        self.surfaces_dismissed.load(Ordering::Relaxed)
    }

    pub fn actions_dispatched(&self) -> u64 {
        self.actions_dispatched.load(Ordering::Relaxed)
    }

    pub fn open_nodes_dispatched(&self) -> u64 {
        self.open_nodes_dispatched.load(Ordering::Relaxed)
    }
}

impl UxObserver for CountingObserver {
    fn observe(&self, event: &UxEvent) {
        match event {
            UxEvent::SurfaceOpened { .. } => {
                self.surfaces_opened.fetch_add(1, Ordering::Relaxed);
            }
            UxEvent::SurfaceDismissed { .. } => {
                self.surfaces_dismissed.fetch_add(1, Ordering::Relaxed);
            }
            UxEvent::ActionDispatched { .. } => {
                self.actions_dispatched.fetch_add(1, Ordering::Relaxed);
            }
            UxEvent::OpenNodeDispatched { .. } => {
                self.open_nodes_dispatched.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// RecordingObserver — bounded ring of events
// ---------------------------------------------------------------------------

/// Append-only event recorder. Stores up to `capacity` events; older
/// events are dropped from the front when the buffer fills.
///
/// Use this for tests (assert event order / shape after a sequence
/// of host messages), for the diagnostics pane's "recent events"
/// view, and for `UxProbe` adapters that need replay.
pub struct RecordingObserver {
    capacity: usize,
    events: Mutex<std::collections::VecDeque<UxEvent>>,
}

impl RecordingObserver {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            events: Mutex::new(std::collections::VecDeque::with_capacity(capacity)),
        }
    }

    /// Snapshot the recorded events. Allocates a fresh `Vec`; the
    /// internal buffer is unchanged. Useful for assertions in tests.
    pub fn snapshot(&self) -> Vec<UxEvent> {
        self.events.lock().unwrap().iter().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.events.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.lock().unwrap().is_empty()
    }

    pub fn clear(&self) {
        self.events.lock().unwrap().clear();
    }
}

impl UxObserver for RecordingObserver {
    fn observe(&self, event: &UxEvent) {
        let mut buf = self.events.lock().unwrap();
        if buf.len() == self.capacity && self.capacity > 0 {
            buf.pop_front();
        }
        buf.push_back(event.clone());
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn counting_observer_increments_per_event_kind() {
        let counts = Arc::new(CountingObserver::new());
        let mut observers = UxObservers::new();
        observers.register(Box::new(CountingObserverProxy(Arc::clone(&counts))));

        observers.emit(UxEvent::SurfaceOpened {
            surface: SurfaceId::CommandPalette,
        });
        observers.emit(UxEvent::SurfaceDismissed {
            surface: SurfaceId::CommandPalette,
            reason: DismissReason::Cancelled,
        });
        observers.emit(UxEvent::ActionDispatched {
            action_id: ActionId::GraphTogglePhysics,
            target: None,
        });

        assert_eq!(counts.surfaces_opened(), 1);
        assert_eq!(counts.surfaces_dismissed(), 1);
        assert_eq!(counts.actions_dispatched(), 1);
        assert_eq!(counts.open_nodes_dispatched(), 0);
    }

    #[test]
    fn recording_observer_captures_in_order_with_bound() {
        let recorder = Arc::new(RecordingObserver::with_capacity(2));
        let mut observers = UxObservers::new();
        observers.register(Box::new(RecordingObserverProxy(Arc::clone(&recorder))));

        observers.emit(UxEvent::SurfaceOpened {
            surface: SurfaceId::CommandPalette,
        });
        observers.emit(UxEvent::SurfaceOpened {
            surface: SurfaceId::NodeFinder,
        });
        observers.emit(UxEvent::SurfaceOpened {
            surface: SurfaceId::ContextMenu,
        });

        // Capacity 2: oldest (CommandPalette) was evicted.
        let snap = recorder.snapshot();
        assert_eq!(snap.len(), 2);
        assert!(matches!(
            snap[0],
            UxEvent::SurfaceOpened {
                surface: SurfaceId::NodeFinder
            }
        ));
        assert!(matches!(
            snap[1],
            UxEvent::SurfaceOpened {
                surface: SurfaceId::ContextMenu
            }
        ));
    }

    #[test]
    fn observers_emit_to_all_registered() {
        let a = Arc::new(CountingObserver::new());
        let b = Arc::new(CountingObserver::new());
        let mut observers = UxObservers::new();
        observers.register(Box::new(CountingObserverProxy(Arc::clone(&a))));
        observers.register(Box::new(CountingObserverProxy(Arc::clone(&b))));

        observers.emit(UxEvent::SurfaceOpened {
            surface: SurfaceId::CommandPalette,
        });

        assert_eq!(a.surfaces_opened(), 1);
        assert_eq!(b.surfaces_opened(), 1);
    }

    // Proxy newtypes so `Arc<CountingObserver>` can implement
    // `UxObserver` via deref while keeping the shared handle for
    // post-emission inspection in tests.
    struct CountingObserverProxy(Arc<CountingObserver>);
    impl UxObserver for CountingObserverProxy {
        fn observe(&self, event: &UxEvent) {
            self.0.observe(event);
        }
    }

    struct RecordingObserverProxy(Arc<RecordingObserver>);
    impl UxObserver for RecordingObserverProxy {
        fn observe(&self, event: &UxEvent) {
            self.0.observe(event);
        }
    }
}
