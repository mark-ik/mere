/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # glossary
//!
//! Graph digest projections — pure `Graph -> human-facing view` for the Mere
//! browser. The textual / statistical sibling of `cartography` (spatial
//! projection) and `linked-data` (RDF / JSON-LD interchange): it turns the
//! kernel's graph into consumer-facing summaries the gloss Navigator, apparatus,
//! and export render. Host-free, DOM-free, `&Graph`-immutable.
//!
//! Planned surface (see the gloss-outline-lens plan): `outline_djot(&Graph) -> String`
//! (a djot outline nested by parsed URL structure) and
//! `graph_metrics(&Graph) -> GraphMetrics` (counts / degree / components).
//!
//! Renamed from `mere-orrery` 2026-06-23; its a11y `project_graph` moved host-side
//! into meerkat's `orrery_a11y_tree` (unified-document-host slice 4) and was retired here.

#![doc(html_root_url = "https://docs.rs/glossary/0.0.1")]

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";
