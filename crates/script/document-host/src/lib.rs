// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! DocumentScript component host (P2.1b).
//!
//! The native side of the `document-core` WIT world. It owns a Wasmtime engine +
//! a per-instance `Store<ScriptHost>`, backs the `log` + `document-host.inspect`
//! imports over a **live genet `ScriptedDom`** (via [`dom_view`]), and drives the
//! per-turn `handle-event` contract (§10.2) with atomic, revision-checked `apply`
//! (§10.3).
//!
//! - Exports are invoked via `call_async` (`exports: { default: async }`): turns
//!   run on a fiber, so a future sync `fetch` import can suspend a turn without
//!   blocking (plan §11.7-7). No async import yet, so turns never suspend.
//! - The document is the real HTML DOM (P2.0's in-memory `Doc` is retired): each
//!   element is a view-node named by tag, each text node a `#text` view-node. Node
//!   identity is genet's `NodeId` round-tripped through the WIT `node-id` (`u64`);
//!   `is_live` guards every mutation. The revision counter is host-side.

use std::path::Path;

/// Project / mutate a live genet `ScriptedDom` behind the WIT imports.
pub mod dom_view;

/// The `register-mod-loader` `WasmModRuntime` bridge (P2.4).
pub mod runtime;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use genet_scripted_dom::ScriptedDom;
use layout_dom_api::{LayoutDom, LayoutDomMut, LocalName, Namespace, QualName};
use wasmtime::component::{Component, HasSelf, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store, StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

wasmtime::component::bindgen!({
    // The shared, runtime-neutral `mere:script` contract (one WIT, two runtimes:
    // this native Wasmtime host + the browser jco path). Lives at `crates/script/wit`,
    // not inside this native crate, so the browser consumes the same world.
    path: "../wit",
    world: "document-core",
    // Turns run on a fiber (`exports` async, invoked via `call_async`); host imports
    // are async too so the sync-signature `net.fetch` can be implemented as a host
    // `async fn` that suspends the turn's fiber during I/O without blocking the host
    // thread (plan §11.7-7). The other imports don't await — `async fn` is free there.
    exports: { default: async },
    imports: { default: async },
});

use crate::mere::script::document::DocumentView;

/// FNV-1a over kind + text. A deterministic stand-in for the change-detection
/// token; a real impl uses a subtree Merkle hash.
pub(crate) fn content_hash(kind: &str, text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in kind.bytes().chain(std::iter::once(0)).chain(text.bytes()) {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn qual(local: &str) -> QualName {
    QualName::new(None, Namespace::from(""), LocalName::from(local))
}

/// A small starting document: `<body><p>Intro paragraph.</p><p>Second paragraph.</p></body>`.
/// In the real integration (P2.5) this comes from the fetched page; here the host
/// seeds it so the contract can be exercised headless.
fn seed_dom() -> ScriptedDom {
    let mut dom = ScriptedDom::new();
    let root = dom.document();
    let body = dom.create_element(qual("body"));
    dom.append_child(root, body);
    for t in ["Intro paragraph.", "Second paragraph."] {
        let p = dom.create_element(qual("p"));
        dom.append_child(body, p);
        let text = dom.create_text(t);
        dom.append_child(p, text);
    }
    dom
}

mod capabilities;
mod host;
mod net;
mod script;

pub use capabilities::*;
pub use host::*;
pub use net::*;
pub use script::*;
