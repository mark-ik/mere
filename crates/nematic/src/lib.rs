//! # Nematic
//!
//! Portable smolweb engine for the [`mere`](https://crates.io/crates/mere)
//! browser — renders Gemini, Gopher, static HTML, Markdown, RSS/Atom, and other
//! lightweight protocols where content can be rendered with minimal layout
//! cost.
//!
//! The name *nematic* is borrowed from liquid-crystal physics: a nematic phase
//! has *orientational* order without *positional* order — rod-shaped molecules
//! all point the same way but otherwise flow freely. Light passes through
//! aligned nematic crystals coherently; that's the basis of LCDs. The metaphor
//! for a smolweb rendering engine: organized-but-flowing, threads aligned, the
//! reader sees through to the meaning without layout-noise getting in the way.
//!
//! ## Status
//!
//! Pre-1.0. This 0.0.x release reserves the crate name and documents intent;
//! implementation is in progress within the
//! [Mere workspace](https://crates.io/crates/mere).

#![doc(html_root_url = "https://docs.rs/nematic/0.0.1")]

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";
