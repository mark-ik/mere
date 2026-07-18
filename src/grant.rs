//! Scoped structural capabilities and the authority seam.
//!
//! A [`Grant`] is a structural capability: a [`Subject`] may act under a
//! `capability_path` with a [`Mode`]. This is the layer-1 structural cap of the
//! designed three-layer stack (structural cap + policy facts + group-key),
//! kept minimal: the path is opaque and coverage is prefix-shaped. The
//! [`AuthorityProvider`] is the replaceable read boundary that answers coverage,
//! so a meadowcap-shaped provider over graph-cluster-derived namespaces can
//! replace [`PrefixAuthority`] without the gate changing.

use crate::Subject;

/// Access mode, in the meadowcap sense. Ordered by power: a `Write` grant
/// covers a `Read` need, and `Delegate` covers both.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mode {
    /// May observe the covered area.
    Read,
    /// May mutate the covered area (implies [`Read`](Mode::Read)).
    Write,
    /// May grant covered capabilities to others (implies [`Write`](Mode::Write)).
    Delegate,
}

impl Mode {
    /// Whether a grant of `self` satisfies a `needed` mode.
    pub fn covers(self, needed: Mode) -> bool {
        self >= needed
    }
}

/// A structural capability: `subject` may act under `path_prefix` with `mode`.
/// `path_prefix` is an opaque scope string; coverage is prefix-shaped over the
/// node-id namespace (the minimal stand-in for meadowcap path-prefix scope).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grant {
    /// The keyholder this capability is granted to.
    pub subject: Subject,
    /// The scope prefix this capability covers.
    pub path_prefix: String,
    /// The access mode granted.
    pub mode: Mode,
}

impl Grant {
    /// A grant of `mode` to `subject` over everything under `path_prefix`.
    pub fn new(subject: Subject, path_prefix: impl Into<String>, mode: Mode) -> Self {
        Self {
            subject,
            path_prefix: path_prefix.into(),
            mode,
        }
    }

    /// Whether this grant covers `path` at `needed` mode: same subject, the
    /// grant's prefix is a prefix of `path`, and the mode is sufficient.
    pub fn covers(&self, subject: Subject, path: &str, needed: Mode) -> bool {
        self.subject == subject && self.mode.covers(needed) && path.starts_with(&self.path_prefix)
    }
}

/// The replaceable authority read boundary: given a subject and the path a
/// petition claims to act under, does the subject hold a covering capability?
///
/// Mirrors `gemot::MootAuthorizationProvider`. The gate depends on this trait,
/// never on a concrete grant store, so authority can come from a local grant
/// table now and a meadowcap-shaped structural-cap provider (over
/// graph-cluster-derived namespaces, layered with tessera policy facts) later,
/// with no gate change.
pub trait AuthorityProvider {
    /// Whether `subject` may act under `path` at `needed` mode.
    fn covers(&self, subject: Subject, path: &str, needed: Mode) -> bool;
}

/// The minimal provider: a flat table of grants, coverage by prefix. The
/// stand-in until the structural-cap layer is built.
#[derive(Clone, Debug, Default)]
pub struct PrefixAuthority {
    grants: Vec<Grant>,
}

impl PrefixAuthority {
    /// An empty authority table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a grant to the table, returning `self` for chaining.
    pub fn with_grant(mut self, grant: Grant) -> Self {
        self.grants.push(grant);
        self
    }

    /// Add a grant to the table in place.
    pub fn grant(&mut self, grant: Grant) {
        self.grants.push(grant);
    }

    /// The grants held (for projection into a denizen's nested graph).
    pub fn grants(&self) -> &[Grant] {
        &self.grants
    }
}

impl AuthorityProvider for PrefixAuthority {
    fn covers(&self, subject: Subject, path: &str, needed: Mode) -> bool {
        self.grants
            .iter()
            .any(|grant| grant.covers(subject, path, needed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subject(tag: u8) -> Subject {
        Subject::new([tag; 32])
    }

    #[test]
    fn write_covers_read_but_not_the_reverse() {
        assert!(Mode::Write.covers(Mode::Read));
        assert!(Mode::Delegate.covers(Mode::Write));
        assert!(!Mode::Read.covers(Mode::Write));
    }

    #[test]
    fn a_grant_covers_only_its_subject_prefix_and_sufficient_mode() {
        let grant = Grant::new(subject(1), "trail/", Mode::Write);
        assert!(grant.covers(subject(1), "trail/step1", Mode::Write));
        assert!(grant.covers(subject(1), "trail/step1", Mode::Read));
        assert!(!grant.covers(subject(1), "other/x", Mode::Write), "outside prefix");
        assert!(!grant.covers(subject(2), "trail/step1", Mode::Write), "wrong subject");
        assert!(!grant.covers(subject(1), "trail/step1", Mode::Delegate), "insufficient mode");
    }

    #[test]
    fn prefix_authority_answers_over_its_table() {
        let authority = PrefixAuthority::new()
            .with_grant(Grant::new(subject(1), "trail/", Mode::Write))
            .with_grant(Grant::new(subject(2), "notes/", Mode::Read));
        assert!(authority.covers(subject(1), "trail/step1", Mode::Write));
        assert!(authority.covers(subject(2), "notes/a", Mode::Read));
        assert!(!authority.covers(subject(2), "notes/a", Mode::Write), "read-only");
        assert!(!authority.covers(subject(1), "notes/a", Mode::Read), "no grant there");
    }
}
