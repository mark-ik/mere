// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Graph test suite — split by topic per the 2026-05-11 kernel
//! decomposition pass (1420-LOC monolithic `tests.rs` → 5 focused
//! sub-modules, each under the 600-LOC ceiling).

pub mod edge_taxonomy;
pub mod filter;
pub mod nodes_and_edges;
pub mod queries_and_address;
pub mod snapshot_basic;
pub mod snapshot_imports;
pub mod snapshot_navigation;
pub mod snapshot_size;
