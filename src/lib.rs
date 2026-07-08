/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # armillary
//!
//! A host-neutral **actor-kernel** runtime. A single-threaded *host kernel* owns
//! all canonical state (the document model, the GPU device, the window); a
//! constellation of *actors* runs off-thread and talks to it only by message. The
//! name is the instrument: an armillary sphere is a frame of rings around a
//! central point, which is the shape here, the kernel at the center and the actor
//! rings around it.
//!
//! This crate supplies the runtime spine and nothing host-specific:
//!
//! - **The typed boundary** ([`boundary`]). The kernel context is `!Send` by
//!   construction ([`KernelThread`]), so the compiler refuses to move kernel
//!   authority onto an actor thread. Actors carry only `Send` handles and exchange
//!   only `Send` messages. This is GPUI's discipline made structural: boundary
//!   drift is a compile error, not a code-review catch.
//! - **The actor harness** ([`actor`]). [`spawn`] runs a subsystem on its own
//!   thread, driven by `Command` messages and drained of `Update` messages. The
//!   actor's internals may be `!Send` (a JS engine, a DOM, a layout engine) because
//!   they are *built on the actor thread*, never moved across the boundary.
//! - **Generations** ([`message`]). Monotonic counters the kernel stamps work
//!   with, so a result from a superseded state (a scene for a page the surface
//!   already left, input from before a resize) is dropped rather than applied.
//!
//! The concrete message taxonomy (a host's own `Command` / `Update` enums) and the
//! kernel inbox live with the host, which builds its kernel on this crate. The
//! runtime here is deliberately generic over those types.

pub mod actor;
pub mod boundary;
pub mod message;
pub mod pool;

pub use actor::{ActorHandle, Emitter, Wake, spawn, spawn_on};
pub use boundary::KernelThread;
pub use message::{Generations, NavGeneration, ViewportGeneration};
pub use pool::Pool;
