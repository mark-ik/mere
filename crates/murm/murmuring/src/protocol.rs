//! [`BilateralProtocol`] trait — the contract every concrete bilateral chat
//! protocol implements.
//!
//! Murmuring's role is to host concrete protocol modules (Cable, MLS, Tox,
//! etc., Phase 2B+ work). The trait below is the abstraction `murm`
//! consumes when it wants to be agnostic about which protocol carries any
//! given conversation.

/// A pluggable bilateral chat protocol.
///
/// Concrete protocols (Cable in Phase 2B; MLS, Tox, etc., later) implement
/// this trait so `murm` can host them uniformly. The trait is intentionally
/// shallow at this stage — the concrete operation surface (open conversation,
/// subscribe to messages, send, history query) firms up in Phase 2B once
/// Cable's actual implementation lands.
///
/// ## Why so minimal right now?
///
/// Per the [`feedback_spec_code_samples_illustrative_vs_implementation_ready`
/// memory](C:\Users\mark_\.claude\projects\c--Users-mark--Code\memory\feedback_spec_code_samples_illustrative_vs_implementation_ready.md):
/// trait sketches in spec docs should be illustrative-signature-only when
/// the implementation hasn't surfaced the right associated types yet.
/// `BilateralProtocol` is in that state — we know it'll have associated
/// `Conversation`, `Message`, and `Error` types and methods for `open` /
/// `subscribe` / `send` / `history`, but the exact bounds and async-fn
/// shapes will fall out of writing Cable first. Locking the trait now would
/// force premature decisions.
///
/// What `BilateralProtocol` does pin down right now: a stable identity
/// (`name()`) so `murm` can route between protocol modules at runtime, and
/// `Send + Sync` so multiple protocol handlers can run concurrently across
/// `tokio::spawn`-ed tasks.
pub trait BilateralProtocol: Send + Sync {
    /// A short identifying name for this protocol — used for ALPN selection
    /// and for routing in `murm`.
    ///
    /// Conventionally lowercase, no spaces: `"cable"`, `"mls"`, `"tox"`.
    fn name(&self) -> &str;
}
