//! # Mere
//!
//! A spatial browser for the web — webpages are nodes in a force-directed graph
//! rendered on a tiling workbench. Browsing is *etching*: paths cut through
//! substrate by repeated traversal accrue meaning, the way holloways accrue in
//! landscape from centuries of walking.
//!
//! `mere` is the product crate: the entrypoint that composes
//! [`graphshell`](https://crates.io/crates/graphshell) (host shell + GUI),
//! [`verso-tile`](https://crates.io/crates/verso-tile) (tile rendering surfaces),
//! [`inker`](https://crates.io/crates/inker) (engine controller),
//! [`platen`](https://crates.io/crates/platen) (composition surface),
//! [`nematic`](https://crates.io/crates/nematic) (smolweb engine), and the
//! [`murm`](https://crates.io/crates/murm) and
//! [`moothold`](https://crates.io/crates/moothold) supercrates for bilateral
//! and community/federation comms respectively, into a working browser.
//!
//! ## Naming
//!
//! *Mere* draws on three meanings simultaneously: the adverb *merely* (humble,
//! "merely a browser!"), the noun *mere* (a small lake — the still-water surface
//! where things accrue and reflect), and a slant rhyme with *mirror*. Together
//! they frame the product as a quiet reflective surface for browsing.
//!
//! ## Status
//!
//! Pre-1.0. This 0.0.x release reserves the crate name and documents intent;
//! implementation is in progress.

#![doc(html_root_url = "https://docs.rs/mere/0.0.1")]

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";
