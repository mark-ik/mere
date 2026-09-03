// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The kernel / actor boundary, encoded in the type system.
//!
//! The host kernel owns all canonical state. That authority must never move onto
//! an actor thread. Rather than rely on review discipline, the kernel context is
//! `!Send` by construction, so the compiler refuses to send it across a thread
//! boundary. Actors, by contrast, carry only `Send` handles and exchange only
//! `Send` messages (see [`crate::actor`]). Modelled on GPUI's `ForegroundExecutor`
//! / `!Send` `Context`.

use std::marker::PhantomData;
use std::rc::Rc;

/// A zero-size `!Send` + `!Sync` marker that pins a value to the thread that
/// created it.
///
/// Embed it in the host's kernel context (the struct that owns the graph, the
/// wgpu device, and the chrome DOM) so the whole context cannot be moved onto an
/// actor thread: the attempt is a compile error, not a review catch. This is
/// GPUI's `ForegroundExecutor` discipline distilled to a marker.
///
/// Holding `PhantomData<Rc<()>>` opts the type out of `Send` / `Sync` at zero
/// runtime cost (an `Rc` is neither). The marker carries no data; constructing one
/// is the claim "this runs on the kernel thread."
///
/// The marker proves `!Send` to the compiler:
///
/// ```compile_fail
/// use armillary::KernelThread;
/// fn requires_send<T: Send>(_: T) {}
/// // Does not compile: KernelThread is !Send by construction.
/// requires_send(KernelThread::new());
/// ```
#[derive(Default)]
pub struct KernelThread {
    _pin: PhantomData<Rc<()>>,
}

impl KernelThread {
    /// Mint the marker on the current thread. The resulting value cannot leave it.
    pub fn new() -> Self {
        Self { _pin: PhantomData }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_thread_is_zero_sized() {
        assert_eq!(std::mem::size_of::<KernelThread>(), 0, "the marker is free");
    }

    // The !Send guarantee itself is checked by the `compile_fail` doctest above;
    // a runtime test cannot assert the absence of an auto-trait.
}
