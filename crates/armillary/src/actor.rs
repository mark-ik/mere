/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The actor harness: run a subsystem on its own thread, talk to it by message.
//!
//! An actor owns possibly-`!Send` internals (a JS engine, a DOM, Stylo) pinned to
//! its thread. The kernel holds only a `Send` [`ActorHandle`] to send commands and
//! drains a `Receiver` of `Send` updates. The boundary is the GPUI lesson made a
//! function signature: everything that crosses is `Send`; everything pinned stays
//! on its thread.
//!
//! The actor's internals never cross the boundary because [`spawn`] builds them
//! *on the actor thread*: the `run` closure that constructs the engine is the only
//! thing moved, and it captures `Send` build arguments, not `!Send` state.
//!
//! `fetch::Fetcher` and `sync::SyncHost` in meerkat are the existing instances of
//! this shape (own a runtime, push typed updates over a channel + wake); the host
//! migrates them onto this harness in P1.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use crate::pool::Pool;

/// A thread-safe wake callback the actor calls after emitting an update, so the
/// kernel's event loop knows to drain. The host supplies it (meerkat pokes the
/// winit `EventLoopProxy`); armillary stays host-neutral by taking it as a value.
pub type Wake = Arc<dyn Fn() + Send + Sync>;

/// The actor's side of the channel: emit a `Send` update to the kernel and wake
/// it. Lives on the actor thread; cheap to clone for fan-out within the actor.
pub struct Emitter<U> {
    updates: Sender<U>,
    wake: Wake,
}

// Manual `Clone` so it does not require `U: Clone` (only the `Sender` and the
// `Arc` are cloned, never an `U`).
impl<U> Clone for Emitter<U> {
    fn clone(&self) -> Self {
        Self { updates: self.updates.clone(), wake: Arc::clone(&self.wake) }
    }
}

impl<U> Emitter<U> {
    /// Send `update` to the kernel and wake the loop. A closed channel (the kernel
    /// dropped the receiver) drops the update silently; the actor learns of
    /// shutdown when its *command* channel closes, not here.
    pub fn emit(&self, update: U) {
        if self.updates.send(update).is_ok() {
            (self.wake)();
        }
    }
}

/// The kernel's `Send` handle to a running actor. Send it commands; drop it to
/// signal shutdown (the actor's command loop ends when its channel closes).
pub struct ActorHandle<C> {
    commands: Sender<C>,
    join: Option<JoinHandle<()>>,
}

impl<C> ActorHandle<C> {
    /// Send a command to the actor. Returns whether it was delivered; a dead actor
    /// returns `false`, and the kernel reaps it on the update-channel disconnect it
    /// observes.
    pub fn command(&self, command: C) -> bool {
        self.commands.send(command).is_ok()
    }

    /// Signal shutdown and wait for the actor thread to finish. Mostly for tests
    /// and orderly teardown; the kernel usually just drops the handle.
    pub fn join(mut self) {
        // Take the join handle out, then drop `self` so its command `Sender` closes
        // and the actor loop ends, *then* wait. Dropping before joining is what
        // avoids a deadlock (the loop only ends once the channel closes).
        let handle = self.join.take();
        drop(self);
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }
}

impl<C> Drop for ActorHandle<C> {
    fn drop(&mut self) {
        // The command `Sender` (a field) drops with the struct, closing the channel
        // and ending the actor loop. We do not join here, to avoid blocking on
        // drop; the thread winds down on its own. Detach any remaining handle.
        let _ = self.join.take();
    }
}

/// Spawn an actor on its own thread.
///
/// `run` executes *on the new thread*: it builds the actor's (possibly `!Send`)
/// state there, then loops on the `Receiver<Command>`, emitting through the
/// `Emitter<Update>`. Because the state is constructed on the thread it never
/// crosses the boundary, so `!Send` engines (Nova, Stylo, a DOM) are fine. Only
/// `run` itself moves to the thread, and it is `Send`.
///
/// Returns the kernel's `Send` [`ActorHandle`] plus the `Receiver` of updates the
/// kernel drains. The actor loop should end when its `Receiver<Command>` closes
/// (`recv` returns `Err`), which happens when the handle is dropped.
pub fn spawn<C, U, F>(wake: Wake, run: F) -> (ActorHandle<C>, Receiver<U>)
where
    C: Send + 'static,
    U: Send + 'static,
    F: FnOnce(Receiver<C>, Emitter<U>) + Send + 'static,
{
    let (command_tx, command_rx) = mpsc::channel::<C>();
    let (update_tx, update_rx) = mpsc::channel::<U>();
    let emitter = Emitter { updates: update_tx, wake };
    let join = thread::spawn(move || run(command_rx, emitter));
    (ActorHandle { commands: command_tx, join: Some(join) }, update_rx)
}

/// Spawn an actor on a pooled worker thread instead of a fresh one.
///
/// Identical to [`spawn`] except the actor's `run` loop occupies a worker from
/// `pool` for its lifetime; when it ends (the handle drops, the command channel
/// closes), the worker is reused for the next actor. This bounds the OS-thread
/// count — and any leaked per-thread state, such as Stylo's leaked sharing cache —
/// to *peak concurrent* actors rather than the total ever spawned, which matters
/// for the churny, long-lived content actors. The returned [`ActorHandle`] carries
/// no `JoinHandle` (the worker is the pool's, not the handle's); dropping it still
/// ends the actor by closing the command channel.
pub fn spawn_on<C, U, F>(pool: &Pool, wake: Wake, run: F) -> (ActorHandle<C>, Receiver<U>)
where
    C: Send + 'static,
    U: Send + 'static,
    F: FnOnce(Receiver<C>, Emitter<U>) + Send + 'static,
{
    let (command_tx, command_rx) = mpsc::channel::<C>();
    let (update_tx, update_rx) = mpsc::channel::<U>();
    let emitter = Emitter { updates: update_tx, wake };
    pool.submit(Box::new(move || run(command_rx, emitter)));
    (ActorHandle { commands: command_tx, join: None }, update_rx)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    // The handles that cross to the kernel thread must be `Send` by type. (The
    // kernel context, by contrast, is `!Send`; see `boundary`.)
    const _: fn() = || {
        fn assert_send<T: Send>() {}
        assert_send::<ActorHandle<u32>>();
        assert_send::<Emitter<u32>>();
        assert_send::<Receiver<u32>>();
    };

    fn counting_wake() -> (Wake, Arc<AtomicUsize>) {
        let count = Arc::new(AtomicUsize::new(0));
        let inner = Arc::clone(&count);
        let wake: Wake = Arc::new(move || {
            inner.fetch_add(1, Ordering::SeqCst);
        });
        (wake, count)
    }

    #[test]
    fn actor_round_trips_commands_and_wakes_per_update() {
        let (wake, wakes) = counting_wake();

        // A trivial accumulator: its running total is actor-thread-local state,
        // built inside the closure, so it never crosses the boundary.
        let (handle, updates) = spawn::<u32, u32, _>(wake, |commands, out| {
            let mut total: u32 = 0;
            while let Ok(n) = commands.recv() {
                total += n;
                out.emit(total);
            }
        });

        assert!(handle.command(2));
        assert!(handle.command(3));
        handle.join(); // closes the command channel; the loop ends; the thread joins

        let got: Vec<u32> = updates.iter().collect();
        assert_eq!(got, vec![2, 5], "updates reflect the running total, in order");
        assert_eq!(wakes.load(Ordering::SeqCst), 2, "each emit woke the kernel once");
    }

    #[test]
    fn dropping_the_handle_stops_the_actor() {
        let (wake, _wakes) = counting_wake();
        let (handle, updates) = spawn::<(), u8, _>(wake, |commands, out| {
            // Emit one update on construction, then idle until the channel closes.
            out.emit(1);
            while commands.recv().is_ok() {}
            out.emit(2); // emitted as the loop exits
        });

        drop(handle); // no join; the actor must still wind down on its own
        // The update channel closes once the actor thread (holding the Emitter)
        // ends, so collecting terminates rather than hanging.
        let got: Vec<u8> = updates.iter().collect();
        assert_eq!(got, vec![1, 2], "the actor ran to completion after the handle dropped");
    }
}
