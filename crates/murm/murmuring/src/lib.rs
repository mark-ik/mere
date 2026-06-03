//! # Murmuring
//!
//! Protocol core for bilateral chat-protocol selection — the modular layer
//! within [`murm`](https://crates.io/crates/murm) that hosts concrete chat
//! protocols (Cable in Phase 2B; MLS, Tox, others later).
//!
//! ## Naming
//!
//! The gerund form (*murmuring* = the act of murmuring) names the protocol
//! plumbing. The singular noun (*murmur* = a single bilateral conversation)
//! is the user-facing term in `murm` and `mere`.
//!
//! ## What's in here
//!
//! - [`BilateralProtocol`] trait — the contract concrete protocols implement
//! - [`Post`] / [`PostKind`] / [`PostId`] / [`ChannelName`] — Cable-shaped
//!   post types from the inherited Cable spec §2.4
//! - [`MurmuringError`] — error type
//!
//! ## What's NOT in here yet (Phase 2B)
//!
//! - Cable wire-protocol encoding/decoding (`protocols/cable/wire.rs`)
//! - BLAKE3-256 post-hash computation
//! - Causal-DAG management (links resolution, head tracking)
//! - Channel time-range request sync
//! - Concrete `BilateralProtocol` implementations
//!
//! ## Why this lives in its own crate
//!
//! - Multiple protocols (Cable, MLS, future) share infrastructure; one
//!   crate avoids duplication.
//! - `murm` consumes the abstract `BilateralProtocol` trait without
//!   pulling in every concrete protocol; protocols can be feature-gated.
//! - A non-`murm` consumer (test harness, alternative bilateral layer)
//!   could in principle use `murmuring` directly.
//!
//! ## Status
//!
//! Pre-1.0. Phase 2A (foundation): trait + post types.
//! Phase 2B will add Cable wire protocol + concrete Cable adapter.

#![doc(html_root_url = "https://docs.rs/murmuring/0.0.1")]
#![warn(missing_docs)]

pub mod cable;

mod error;
mod post;
mod protocol;

pub use crate::cable::CableEngine;
pub use crate::error::MurmuringError;
pub use crate::post::{ChannelName, InfoEntry, Post, PostId, PostKind};
pub use crate::protocol::BilateralProtocol;

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Lifecycle stage marker.
pub const STAGE: &str = "pre-alpha";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_id_round_trips_through_bytes() {
        let bytes = [42u8; 32];
        let id = PostId::new(bytes);
        assert_eq!(id.as_bytes(), &bytes);
    }

    #[test]
    fn channel_name_constructs() {
        let c = ChannelName::new("session");
        assert_eq!(c.as_str(), "session");
    }

    #[test]
    fn channel_name_try_new_accepts_valid() {
        let c = ChannelName::try_new("session").unwrap();
        assert_eq!(c.as_str(), "session");
        assert!(ChannelName::try_new("links").is_ok());
        assert!(ChannelName::try_new("hello-world").is_ok());
        assert!(ChannelName::try_new("会議").is_ok()); // unicode is fine
    }

    #[test]
    fn channel_name_try_new_rejects_empty() {
        let result = ChannelName::try_new("");
        assert!(matches!(result, Err(MurmuringError::MissingField(_))));
    }

    #[test]
    fn channel_name_try_new_rejects_too_long() {
        // 256 bytes — one over the cap.
        let long = "a".repeat(256);
        let result = ChannelName::try_new(long);
        assert!(matches!(result, Err(MurmuringError::MissingField(_))));

        // Exactly at the cap is fine.
        let max_len = "a".repeat(255);
        assert!(ChannelName::try_new(max_len).is_ok());
    }

    #[test]
    fn channel_name_try_new_rejects_control_chars() {
        // ASCII NUL
        assert!(matches!(
            ChannelName::try_new("\0bad"),
            Err(MurmuringError::MissingField(_))
        ));
        // ASCII bell
        assert!(matches!(
            ChannelName::try_new("a\x07b"),
            Err(MurmuringError::MissingField(_))
        ));
        // DEL
        assert!(matches!(
            ChannelName::try_new("a\x7Fb"),
            Err(MurmuringError::MissingField(_))
        ));
        // Newline
        assert!(matches!(
            ChannelName::try_new("multi\nline"),
            Err(MurmuringError::MissingField(_))
        ));
    }

    #[test]
    fn channel_name_is_valid_name() {
        assert!(ChannelName::is_valid_name("session"));
        assert!(ChannelName::is_valid_name("会議"));
        assert!(!ChannelName::is_valid_name(""));
        assert!(!ChannelName::is_valid_name("a\nb"));
        assert!(!ChannelName::is_valid_name(&"a".repeat(256)));
    }

    #[test]
    fn post_kind_text_carries_channel() {
        let kind = PostKind::Text {
            channel: ChannelName::new("session"),
            text: "hello".to_string(),
            timestamp_ms: 1_700_000_000_000,
        };
        assert_eq!(kind.timestamp_ms(), 1_700_000_000_000);
        assert_eq!(kind.channel().unwrap().as_str(), "session");
        assert_eq!(kind.kind_name(), "text");
    }

    #[test]
    fn post_kind_info_has_no_channel() {
        let kind = PostKind::Info {
            entries: vec![InfoEntry::name("alice")],
            timestamp_ms: 0,
        };
        assert!(kind.channel().is_none());
        assert_eq!(kind.kind_name(), "info");
    }

    #[test]
    fn post_kind_delete_has_no_channel() {
        let kind = PostKind::Delete {
            posts: vec![PostId::new([0; 32])],
            timestamp_ms: 0,
        };
        assert!(kind.channel().is_none());
        assert_eq!(kind.kind_name(), "delete");
    }

    #[test]
    fn post_kind_all_six_variants_have_distinct_kind_names() {
        use std::collections::HashSet;
        let kinds = [
            PostKind::Text {
                channel: ChannelName::new("c"),
                text: String::new(),
                timestamp_ms: 0,
            },
            PostKind::Delete {
                posts: vec![PostId::new([0; 32])],
                timestamp_ms: 0,
            },
            PostKind::Info {
                entries: vec![],
                timestamp_ms: 0,
            },
            PostKind::Topic {
                channel: ChannelName::new("c"),
                topic: String::new(),
                timestamp_ms: 0,
            },
            PostKind::Join {
                channel: ChannelName::new("c"),
                timestamp_ms: 0,
            },
            PostKind::Leave {
                channel: ChannelName::new("c"),
                timestamp_ms: 0,
            },
        ];

        let names: HashSet<&str> = kinds.iter().map(|k| k.kind_name()).collect();
        assert_eq!(names.len(), 6);
        assert!(names.contains("text"));
        assert!(names.contains("delete"));
        assert!(names.contains("info"));
        assert!(names.contains("topic"));
        assert!(names.contains("join"));
        assert!(names.contains("leave"));
    }

    /// A trivial implementor of `BilateralProtocol` to verify the trait is
    /// usable as expected.
    struct TestProtocol;
    impl BilateralProtocol for TestProtocol {
        fn name(&self) -> &str {
            "test"
        }
    }

    #[test]
    fn bilateral_protocol_trait_works() {
        let p = TestProtocol;
        assert_eq!(p.name(), "test");

        // Object-safe usage check: no dynamic dispatch in the trait's current
        // shape (single sync `name()` method), so `&dyn BilateralProtocol`
        // works without ceremony.
        fn takes_dyn(p: &dyn BilateralProtocol) -> &str {
            p.name()
        }
        assert_eq!(takes_dyn(&p), "test");
    }
}
