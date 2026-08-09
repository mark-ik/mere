//! Deprecated compatibility surface for [`esp::embed`].
//!
//! Existing Sibylla imports continue to compile. New code should depend on
//! `esp` and import the same items through `esp::embed`.

#![deprecated(since = "0.1.2", note = "use esp::embed instead")]

pub use esp::embed::*;
