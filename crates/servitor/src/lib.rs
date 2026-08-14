//! Capability-scoped resident helpers for graph applications.
//!
//! A **denizen** is anything admitted to act on a graph through the gate: a
//! resident helper (a servitor), a script, a scenario runner, a remote peer,
//! an agent. It holds an identity (a keyholder [`Subject`]) and a scoped
//! structural capability, and it proposes changes as **petitions** that the
//! [`gate`] validates against the capability and applies through chartulary's
//! attributed, revision-checked commit. Every applied change is attributed to
//! the denizen in the journal.
//!
//! This crate is the denizen-residency **core**, headless and app-agnostic:
//!
//! - [`Subject`] — a keyholder identity (a 32-byte public key), the same shape
//!   the moot authorization seam uses (`gemot::MootAuthorizationRequest.subject`).
//! - [`cap`] — what a capability IS: [`Cap`], a type with a coverage order
//!   (a closed set of named powers, or a hierarchical scope), never a string
//!   with a prefix test.
//! - [`grant`] — a scoped structural capability ([`Grant`]) and the replaceable
//!   [`AuthorityProvider`] seam that answers "does this subject's capability
//!   cover this one?", mirroring `gemot::MootAuthorizationProvider`.
//! - [`gate`] — the one authority pipeline: refuse petitions that touch a
//!   grant projection, check authority, check scope, then commit attributed.
//! - [`watch`] — when a denizen runs: a standing subscription to a scope,
//!   contained by what its subject may read, matched against a journal's
//!   committed entries. The gate says whether a body may write; a watch says
//!   what wakes it.
//!
//! A denizen's inner world (its grant projections, storage markers, registered
//! commands, journal cursors) is an ordinary [`chartulary::GraphLog`]: the
//! nested graph a graph-bearing node points at. The gate operates on that
//! nested graph; wiring a denizen node in a host graph to bear it is the host's
//! job (mere's `Node` implementing `chartulary::GraphBearing`).
//!
//! The capability model is typed but still small: [`Cap`] carries the two shapes
//! real consumers hold at once (an app's closed ring set, and a graph's
//! unbounded node-id namespace), and coverage is the partial order that same
//! type owns. The full model (meadowcap-shaped structural caps over
//! graph-cluster-derived namespaces, binding to leaf node ids) lands when
//! mere's namespace layer is built; this crate consumes an
//! [`AuthorityProvider`], so that richer provider drops in without changing
//! the gate. See the capability model plan (mere design_docs, 2026-07-23) for
//! the order's laws and the delegation/revocation rounds it enables.

pub mod cap;
pub mod delegation;
pub mod gate;
pub mod grant;
pub mod watch;

pub use cap::{Cap, CapError, Capability, ScopePath, assert_capability_laws};
pub use delegation::{ChainError, DelegationTable, cap_path, mode_action, mode_actions, scope_for};
pub use gate::{
    GRANT_PREFIX, Gate, GateError, PROJECTION_MEDIA_TYPE, PROJECTION_TAG, read_projection,
};
pub use grant::{AuthorityProvider, Grant, GrantTable, Mode};
pub use watch::{Wake, Watch, WatchError, WatchEvent, WatchTable};

/// A keyholder identity: the 32-byte public key of whoever acts. A device, a
/// servitor, a persona, a peer, an agent are all subjects; what *kind* of
/// holder it is, is metadata elsewhere, never a second identity axis. Matches
/// the `subject: [u8; 32]` the moot authorization seam already speaks.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Subject(pub [u8; 32]);

impl Subject {
    /// Wrap a raw public key.
    pub fn new(key: [u8; 32]) -> Self {
        Self(key)
    }

    /// Parse a 64-char lowercase-hex key. `None` on any malformed input, so a
    /// corrupt record yields no subject rather than a wrong one.
    pub fn from_hex(hex: &str) -> Option<Self> {
        if hex.len() != 64 {
            return None;
        }
        let mut key = [0u8; 32];
        for (byte, pair) in key.iter_mut().zip(hex.as_bytes().chunks(2)) {
            let text = std::str::from_utf8(pair).ok()?;
            *byte = u8::from_str_radix(text, 16).ok()?;
        }
        Some(Self(key))
    }

    /// Lowercase hex of the key.
    pub fn to_hex(&self) -> String {
        let mut s = String::with_capacity(64);
        for byte in self.0 {
            s.push(char::from_digit((byte >> 4) as u32, 16).unwrap());
            s.push(char::from_digit((byte & 0x0f) as u32, 16).unwrap());
        }
        s
    }

    /// The journal author label for changes this subject commits: `denizen:`
    /// plus the first 8 hex chars of the key. Attribution, not authentication;
    /// the gate has already checked authority before it commits.
    pub fn to_author(&self) -> chartulary::Author {
        let hex = self.to_hex();
        chartulary::Author::new(format!("denizen:{}", &hex[..8]))
    }
}

impl std::fmt::Debug for Subject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Subject({}…)", &self.to_hex()[..8])
    }
}
