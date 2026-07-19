// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Placeholder for in-cartography adapter implementations.
//!
//! As of 2026-05-18, the `graph_canvas` adapter family (which wrapped
//! the `graph-canvas::layout` impls as `LayoutStrategy` /
//! `StreamingLayoutStrategy`) moved out to the new `graph-layout`
//! sibling crate, per the cartography layer brief §9 step 4 sibling-
//! crate move. The `graph-canvas-adapters` feature gate retired with
//! the move; consumers depend on `graph-layout` directly to pull in
//! strategy implementations.
//!
//! This module stays as a stub home for any future adapter family
//! that's narrow enough not to warrant its own crate (e.g.,
//! document-layout adapters that wrap document-canvas's layout).
