// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Portable emit scaffold for the registrar layer.
//!
//! Slice 59 ("Option A" per the diagnostics-emit DI design choice).
//! Registries extracted as their own crates need a way to emit
//! diagnostic events without depending on the shell-side runtime.
//! This module provides:
//!
//! - [`DiagnosticEvent`] — the *portable subset* of the diagnostic
//!   event taxonomy (Span + Message variants). Rich shell-side
//!   variants (CompositorFrame, IntentBatch) carry shell-only
//!   payload types and stay in tree.
//! - [`SpanPhase`], [`StructuredPayloadField`] — the value types
//!   the portable variants reference.
//! - [`install_global_sender`] / [`emit_event`] — the same
//!   thread_local-aware global sender pattern that ran in the
//!   shell-side runtime, lifted here so registry crates can
//!   `use register_diagnostics::emit_event` directly.
//!
//! ## Bridging to the shell-side runtime
//!
//! The shell-side runtime maintains its own richer DiagnosticEvent +
//! ring buffer for events it cares about (CompositorFrame batches,
//! IntentBatch records). Bridging is straightforward: at startup the
//! shell-side calls [`install_global_sender`] with a Sender whose
//! receiver translates each `register_diagnostics::DiagnosticEvent`
//! into the shell-side's own variant (mapping is field-for-field
//! since the variant shapes match). This bridging lands per-registry
//! as register-mod-loader, register-identity, etc. extract and start
//! emitting through this scaffold.

use std::sync::OnceLock;
use std::sync::mpsc::Sender;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanPhase {
    Enter,
    Exit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructuredPayloadField {
    pub name: &'static str,
    pub value: String,
}

/// Portable subset of the diagnostic event taxonomy. Rich shell-side
/// variants (CompositorFrame, IntentBatch) live in the shell-side
/// runtime's own DiagnosticEvent enum and don't appear here.
#[derive(Clone, Debug)]
pub enum DiagnosticEvent {
    Span {
        name: &'static str,
        phase: SpanPhase,
        duration_us: Option<u64>,
    },
    MessageSent {
        channel_id: &'static str,
        byte_len: usize,
    },
    MessageSentStructured {
        channel_id: &'static str,
        byte_len: usize,
        fields: Vec<StructuredPayloadField>,
    },
    MessageReceived {
        channel_id: &'static str,
        latency_us: u64,
    },
    MessageReceivedStructured {
        channel_id: &'static str,
        latency_us: u64,
        fields: Vec<StructuredPayloadField>,
    },
    /// A structured log / trace event (a `tracing` event mirrored into diagnostics), as
    /// opposed to channel messaging. `target` is the emitting component and `level` the
    /// tracing level name (`"ERROR"`/`"WARN"`/`"INFO"`/`"DEBUG"`/`"TRACE"`); the host maps
    /// `level` to its own severity and may group by `target`. `message` is the event's human
    /// text, `fields` the structured remainder. Distinct from the `Message*` variants so a
    /// log line is never framed as a message receipt.
    Event {
        target: &'static str,
        level: &'static str,
        message: String,
        fields: Vec<StructuredPayloadField>,
    },
}

static GLOBAL_DIAGNOSTICS_TX: OnceLock<Sender<DiagnosticEvent>> = OnceLock::new();

thread_local! {
    /// Per-thread sender override. Writes via `install_global_sender`;
    /// `emit_event` checks this first and falls back to the global
    /// `OnceLock` when absent. Lets each test inject its own sender
    /// (the OnceLock is one-shot, so multi-test isolation needs a
    /// per-thread channel). Production code that only calls
    /// `install_global_sender` once at startup writes both the
    /// thread-local (for the calling thread) and the global (for all
    /// other threads).
    ///
    /// Slice 68c-fix: was `#[cfg(test)]`-gated so it only activated
    /// for register-diagnostics' own tests. Cross-crate callers
    /// (register-mod-loader tests) bypassed it and saw OnceLock
    /// staleness across tests. The gate was removed; the thread-local
    /// state is a few bytes per thread and benign in production.
    static THREAD_LOCAL_DIAGNOSTICS_TX: std::cell::RefCell<Option<Sender<DiagnosticEvent>>> =
        const { std::cell::RefCell::new(None) };
}

/// Install the global sender. Hosts call this once at startup. Tests
/// call it per-test to set up isolated channels — the per-thread
/// override (see [`THREAD_LOCAL_DIAGNOSTICS_TX`]) makes per-test
/// isolation work despite the global being a one-shot `OnceLock`.
pub fn install_global_sender(sender: Sender<DiagnosticEvent>) {
    let _ = GLOBAL_DIAGNOSTICS_TX.set(sender.clone());
    THREAD_LOCAL_DIAGNOSTICS_TX.with(|slot| {
        *slot.borrow_mut() = Some(sender);
    });
}

/// Emit a diagnostic event. Routes to the per-thread sender override
/// if set, else to the global sender if installed, else drops
/// silently (matches the shell-side runtime's pre-startup tolerance).
pub fn emit_event(event: DiagnosticEvent) {
    let mut event = Some(event);
    let mut handled = false;
    THREAD_LOCAL_DIAGNOSTICS_TX.with(|slot| {
        if let Some(tx) = slot.borrow().as_ref() {
            if let Some(payload) = event.take() {
                let _ = tx.send(payload);
            }
            handled = true;
        }
    });
    if handled {
        return;
    }
    if let Some(tx) = GLOBAL_DIAGNOSTICS_TX.get() {
        if let Some(payload) = event.take() {
            let _ = tx.send(payload);
        }
    }
}

/// Convenience: emit a span exit with measured duration. Mirrors
/// the shell-side `emit_span_duration` helper that's the most
/// common emit pattern in instrumentation code.
pub fn emit_span_duration(name: &'static str, duration_us: u64) {
    emit_event(DiagnosticEvent::Span {
        name,
        phase: SpanPhase::Exit,
        duration_us: Some(duration_us),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn emit_with_no_sender_is_silent() {
        // No global sender installed — must not panic, must not block.
        emit_event(DiagnosticEvent::MessageSent {
            channel_id: "test.no_sender",
            byte_len: 42,
        });
    }

    #[test]
    fn emit_routes_to_thread_local_sender() {
        let (tx, rx) = mpsc::channel();
        THREAD_LOCAL_DIAGNOSTICS_TX.with(|slot| {
            *slot.borrow_mut() = Some(tx);
        });

        emit_event(DiagnosticEvent::MessageSent {
            channel_id: "test.routed",
            byte_len: 7,
        });

        let received = rx.try_recv().expect("event should arrive");
        match received {
            DiagnosticEvent::MessageSent {
                channel_id,
                byte_len,
            } => {
                assert_eq!(channel_id, "test.routed");
                assert_eq!(byte_len, 7);
            }
            _ => panic!("wrong variant"),
        }

        // Cleanup.
        THREAD_LOCAL_DIAGNOSTICS_TX.with(|slot| {
            *slot.borrow_mut() = None;
        });
    }

    #[test]
    fn emit_span_duration_helper_produces_exit_span() {
        let (tx, rx) = mpsc::channel();
        THREAD_LOCAL_DIAGNOSTICS_TX.with(|slot| {
            *slot.borrow_mut() = Some(tx);
        });

        emit_span_duration("test.span", 1234);

        let received = rx.try_recv().expect("span should arrive");
        match received {
            DiagnosticEvent::Span {
                name,
                phase,
                duration_us,
            } => {
                assert_eq!(name, "test.span");
                assert_eq!(phase, SpanPhase::Exit);
                assert_eq!(duration_us, Some(1234));
            }
            _ => panic!("wrong variant"),
        }

        THREAD_LOCAL_DIAGNOSTICS_TX.with(|slot| {
            *slot.borrow_mut() = None;
        });
    }
}
