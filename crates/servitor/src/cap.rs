//! What a capability IS: a type with a partial order, not a string with a
//! prefix test.
//!
//! The capability-model round (2026-07-23) replaced `path_prefix: String` plus
//! `starts_with` because that one mechanism was serving two different kinds of
//! capability, and the closed kind inherited the open kind's failure mode:
//!
//! - **Powers** are a *closed set* (an app's rings: navigate, panes, dispatch,
//!   session). There is no such thing as "everything under navigate", so
//!   prefix openness bought nothing and cost everything: a grant on `app/nav`
//!   covered `app/navigate`, and adding a capability path that extended an
//!   existing one would silently widen every grant already issued. Coverage
//!   here is **equality**, so growth cannot widen an old grant.
//! - **Scopes** are an *unbounded hierarchy* (a graph's node-id namespace:
//!   `trail/`, `scenario/`) where prefix scoping is the entire point. Coverage
//!   here is **segment-prefix**, and [`ScopePath`] rejects `..` at parse, so
//!   traversal cannot be expressed, let alone matched.
//!
//! Cross-kind coverage is always false.
//!
//! The same order answers coverage, attenuation ("is B a narrowing of A") and
//! delegation, which is why [`Mode::Delegate`] was inert before it existed.

use std::fmt;

/// A capability with a coverage order.
///
/// Implementations must satisfy two laws, checked by
/// [`assert_capability_laws`] rather than trusted:
///
/// 1. **Reflexive**: `a.covers(a)`.
/// 2. **Transitive**: `a.covers(b) && b.covers(c)` implies `a.covers(c)`.
///
/// A break in either is a hole in the gate, not a style problem.
pub trait Capability: Clone + fmt::Debug + Eq {
    /// Whether holding `self` satisfies a need for `needed`.
    fn covers(&self, needed: &Self) -> bool;
}

/// Why a capability string failed to parse. Loud by design: an unparseable
/// capability must never degrade into a permissive one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapError {
    /// A scope segment was empty (`a//b`, a leading or trailing bare slash).
    EmptySegment,
    /// A scope contained a relative segment (`.` or `..`). Traversal is not
    /// expressible: a scope names a place, it never walks.
    RelativeSegment,
    /// A power name was empty.
    EmptyPower,
    /// The wire form named a kind this build does not know.
    UnknownKind(String),
}

impl fmt::Display for CapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CapError::EmptySegment => write!(f, "empty scope segment"),
            CapError::RelativeSegment => write!(f, "relative scope segment (`.` or `..`)"),
            CapError::EmptyPower => write!(f, "empty power name"),
            CapError::UnknownKind(kind) => write!(f, "unknown capability kind `{kind}`"),
        }
    }
}

impl std::error::Error for CapError {}

/// A hierarchical scope, parsed once into validated segments.
///
/// Comparison is per segment, so `app/nav` does not cover `app/navigate`.
/// `..` and `.` are rejected at parse, so a scope can name a place but never
/// walk out of one.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ScopePath(Vec<String>);

impl ScopePath {
    /// Parse a slash-separated scope. A trailing slash is accepted and
    /// ignored (`trail/` and `trail` are the same scope); an interior empty
    /// segment is not.
    pub fn parse(raw: &str) -> Result<Self, CapError> {
        let trimmed = raw.strip_suffix('/').unwrap_or(raw);
        if trimmed.is_empty() {
            // The root scope: covers everything. Written as "" or "/".
            return Ok(Self(Vec::new()));
        }
        let mut segments = Vec::new();
        for segment in trimmed.split('/') {
            if segment.is_empty() {
                return Err(CapError::EmptySegment);
            }
            if segment == "." || segment == ".." {
                return Err(CapError::RelativeSegment);
            }
            segments.push(segment.to_string());
        }
        Ok(Self(segments))
    }

    /// The root scope, covering every scope.
    pub fn root() -> Self {
        Self(Vec::new())
    }

    /// The validated segments.
    pub fn segments(&self) -> &[String] {
        &self.0
    }

    /// Whether this scope covers `other`: a segment-prefix relation, so
    /// `app/nav` does not cover `app/navigate`. The scope-only face of
    /// [`Capability::covers`], for callers (the gate's scope check) that
    /// already know both sides are scopes.
    pub fn covers_scope(&self, other: &Self) -> bool {
        other.0.len() >= self.0.len() && other.0[..self.0.len()] == self.0[..]
    }
}

impl fmt::Display for ScopePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.join("/"))
    }
}

/// One capability: a named power from a closed set, or a hierarchical scope.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Cap {
    /// A named power. Coverage is EQUALITY, so a power added later can never
    /// be covered by a grant issued earlier.
    Power(String),
    /// A hierarchical scope. Coverage is segment-prefix.
    Scope(ScopePath),
}

impl Cap {
    /// A named power. Empty names are refused: a nameless power would compare
    /// equal to nothing and silently deny, which reads as a bug at the call
    /// site rather than at the source.
    pub fn power(name: impl Into<String>) -> Result<Self, CapError> {
        let name = name.into();
        if name.is_empty() {
            return Err(CapError::EmptyPower);
        }
        Ok(Cap::Power(name))
    }

    /// A hierarchical scope from its slash-separated form.
    pub fn scope(raw: &str) -> Result<Self, CapError> {
        ScopePath::parse(raw).map(Cap::Scope)
    }

    /// The scope covering everything (the root subject's holding).
    pub fn root_scope() -> Self {
        Cap::Scope(ScopePath::root())
    }

    /// Parse the wire form: `power:<name>`, `scope:<path>`, or a bare string.
    ///
    /// A bare string parses as a **scope**, which is what every capability
    /// string meant before this round, so pre-round projections and manifests
    /// keep their meaning on replay. New writers always emit the tagged form.
    pub fn parse(raw: &str) -> Result<Self, CapError> {
        match raw.split_once(':') {
            Some(("power", name)) => Cap::power(name),
            Some(("scope", path)) => Cap::scope(path),
            Some((kind, _)) if !kind.contains('/') => Err(CapError::UnknownKind(kind.to_string())),
            // No recognized tag and the colon is inside a path segment: legacy
            // bare scope.
            _ => Cap::scope(raw),
        }
    }

    /// The wire form, round-tripping through [`Cap::parse`].
    pub fn to_wire(&self) -> String {
        match self {
            Cap::Power(name) => format!("power:{name}"),
            Cap::Scope(path) => format!("scope:{path}"),
        }
    }
}

impl Capability for Cap {
    fn covers(&self, needed: &Self) -> bool {
        match (self, needed) {
            // A closed set: equality only, so growth never widens.
            (Cap::Power(held), Cap::Power(want)) => held == want,
            // A hierarchy: segment-prefix.
            (Cap::Scope(held), Cap::Scope(want)) => held.covers_scope(want),
            // Different kinds never cover each other.
            _ => false,
        }
    }
}

impl fmt::Display for Cap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_wire())
    }
}

/// Assert the [`Capability`] laws over a sample of values: reflexivity on each,
/// and transitivity across every ordered triple. Call it from a test with a
/// representative sample whenever a new capability type appears.
///
/// Panics naming the offending pair or triple, so a broken order fails loudly
/// at the type that broke it rather than as a mysterious denial later.
pub fn assert_capability_laws<C: Capability>(sample: &[C]) {
    for a in sample {
        assert!(a.covers(a), "reflexivity: {a:?} must cover itself");
    }
    for a in sample {
        for b in sample {
            if !a.covers(b) {
                continue;
            }
            for c in sample {
                if b.covers(c) {
                    assert!(
                        a.covers(c),
                        "transitivity: {a:?} covers {b:?} covers {c:?}, so {a:?} must cover {c:?}"
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Cap> {
        vec![
            Cap::root_scope(),
            Cap::scope("trail").unwrap(),
            Cap::scope("trail/step").unwrap(),
            Cap::scope("trailer").unwrap(),
            Cap::power("navigate").unwrap(),
            Cap::power("session").unwrap(),
        ]
    }

    #[test]
    fn the_capability_laws_hold() {
        assert_capability_laws(&sample());
    }

    #[test]
    fn powers_cover_only_themselves_so_growth_never_widens() {
        let navigate = Cap::power("navigate").unwrap();
        assert!(navigate.covers(&Cap::power("navigate").unwrap()));
        assert!(!navigate.covers(&Cap::power("session").unwrap()));
        // The hazard this design exists to kill: a power added later is not
        // covered by a grant issued earlier, however its name is spelled.
        assert!(!navigate.covers(&Cap::power("navigate-secretly").unwrap()));
        assert!(!Cap::power("nav").unwrap().covers(&navigate));
    }

    #[test]
    fn scopes_cover_by_segment_not_by_string_prefix() {
        let trail = Cap::scope("trail").unwrap();
        assert!(trail.covers(&Cap::scope("trail/step1").unwrap()));
        assert!(trail.covers(&Cap::scope("trail").unwrap()));
        // The F1 table, inverted.
        assert!(
            !Cap::scope("app/nav").unwrap().covers(&Cap::scope("app/navigate").unwrap()),
            "a shorter sibling segment must not cover a longer one"
        );
        assert!(
            !Cap::scope("app/session")
                .unwrap()
                .covers(&Cap::scope("app/session-admin").unwrap())
        );
        assert!(!trail.covers(&Cap::scope("trailer").unwrap()));
        assert!(!Cap::scope("trail/step1").unwrap().covers(&trail), "narrower covers nothing wider");
    }

    #[test]
    fn traversal_is_unparseable_not_merely_unmatched() {
        assert_eq!(
            ScopePath::parse("scenario/../app/session"),
            Err(CapError::RelativeSegment),
            "the third F1 row: a `..` scope cannot be constructed at all"
        );
        assert_eq!(ScopePath::parse("a/./b"), Err(CapError::RelativeSegment));
        assert_eq!(ScopePath::parse("a//b"), Err(CapError::EmptySegment));
        assert_eq!(Cap::power(""), Err(CapError::EmptyPower));
    }

    #[test]
    fn kinds_never_cross() {
        // A scope literally spelled like a power name still is not one.
        assert!(!Cap::scope("navigate").unwrap().covers(&Cap::power("navigate").unwrap()));
        assert!(!Cap::power("navigate").unwrap().covers(&Cap::scope("navigate").unwrap()));
    }

    #[test]
    fn the_root_scope_covers_every_scope_and_no_power() {
        let root = Cap::root_scope();
        assert!(root.covers(&Cap::scope("anything/at/all").unwrap()));
        assert!(root.covers(&Cap::root_scope()));
        assert!(
            !root.covers(&Cap::power("navigate").unwrap()),
            "root is the root of the SCOPE hierarchy; powers are held explicitly"
        );
    }

    #[test]
    fn the_wire_form_round_trips_and_reads_legacy_bare_scopes() {
        for cap in sample() {
            assert_eq!(Cap::parse(&cap.to_wire()), Ok(cap.clone()), "{cap:?}");
        }
        // Pre-round strings meant scopes, and still do.
        assert_eq!(Cap::parse("trail/"), Ok(Cap::scope("trail").unwrap()));
        assert_eq!(Cap::parse("app/navigate"), Ok(Cap::scope("app/navigate").unwrap()));
        // A tagged form this build does not know is loud, never permissive.
        assert_eq!(
            Cap::parse("moot:something"),
            Err(CapError::UnknownKind("moot".to_string()))
        );
    }

    #[test]
    fn a_trailing_slash_is_the_same_scope() {
        assert_eq!(Cap::scope("trail/"), Cap::scope("trail"));
        assert_eq!(Cap::scope("/"), Ok(Cap::root_scope()));
        assert_eq!(Cap::scope(""), Ok(Cap::root_scope()));
    }
}
