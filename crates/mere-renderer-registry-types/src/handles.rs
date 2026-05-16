// Copyright 2026 the Mere authors
// SPDX-License-Identifier: MPL-2.0

//! Opaque handle types for embedded-frame and overlay renderer lifecycles.

use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};

/// Identity token for an embedded-frame renderer's per-node producer.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ProducerHandle(NonZeroU64);

impl ProducerHandle {
    pub fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        Self(NonZeroU64::new(n).expect("handle counter never zero"))
    }

    pub fn as_u64(self) -> u64 {
        self.0.get()
    }
}

/// Identity token for an overlay renderer's per-node OS surface.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct OverlayHandle(NonZeroU64);

impl OverlayHandle {
    pub fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        Self(NonZeroU64::new(n).expect("handle counter never zero"))
    }

    pub fn as_u64(self) -> u64 {
        self.0.get()
    }
}
