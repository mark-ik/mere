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
//! - [`grant`] — a scoped structural capability ([`Grant`]) and the replaceable
//!   [`AuthorityProvider`] seam that answers "does this subject's capability
//!   cover this path?", mirroring `gemot::MootAuthorizationProvider`.
//! - [`gate`] — the one authority pipeline: refuse petitions that touch a
//!   grant projection, check authority, check scope, then commit attributed.
//!
//! A denizen's inner world (its grant projections, storage markers, registered
//! commands, journal cursors) is an ordinary [`chartulary::GraphLog`]: the
//! nested graph a graph-bearing node points at. The gate operates on that
//! nested graph; wiring a denizen node in a host graph to bear it is the host's
//! job (mere's `Node` implementing `chartulary::GraphBearing`).
//!
//! The capability model is deliberately minimal here: a `capability_path` is an
//! opaque string and coverage is prefix-shaped over the node-id namespace. The
//! full model (meadowcap-shaped structural caps over graph-cluster-derived
//! namespaces, binding to leaf node ids) lands when mere's namespace layer is
//! built; this crate consumes an [`AuthorityProvider`], so that richer provider
//! drops in without changing the gate.

pub mod gate;
pub mod grant;

pub use gate::{Gate, GateError, GRANT_PREFIX};
pub use grant::{AuthorityProvider, Grant, Mode, PrefixAuthority};

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
