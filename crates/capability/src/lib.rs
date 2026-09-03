// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The shared capability algebra used by Servitor and Gemot.
//!
//! A capability is a type with a coverage order. Strings exist at wire
//! boundaries; authority decisions compare parsed values. The concrete
//! [`Cap`] sum carries the three structural shapes currently used across Mere:
//!
//! - [`Cap::Power`] is closed and covers by equality.
//! - [`Cap::Scope`] is a slash-separated node hierarchy.
//! - [`Cap::Facet`] is a dot-separated facet-namespace hierarchy.
//!
//! Cross-kind coverage is always false. The same order answers coverage and
//! attenuation, so every implementation must remain reflexive and transitive.

use std::fmt;

/// A capability with a coverage order.
///
/// Implementations must be reflexive and transitive. Use
/// [`assert_capability_laws`] over representative values in each implementation's
/// tests.
pub trait Capability: Clone + fmt::Debug + Eq {
    /// Whether holding `self` satisfies a need for `needed`.
    fn covers(&self, needed: &Self) -> bool;
}

/// Why a capability string failed to parse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapError {
    /// A scope segment was empty (`a//b`, or a leading slash).
    EmptySegment,
    /// A scope contained a relative segment (`.` or `..`).
    RelativeSegment,
    /// A power name was empty.
    EmptyPower,
    /// A facet namespace contained an empty interior segment (`web..viewer`).
    EmptyFacetSegment,
    /// The wire form named a kind this build does not know.
    UnknownKind(String),
}

impl fmt::Display for CapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CapError::EmptySegment => write!(f, "empty scope segment"),
            CapError::RelativeSegment => write!(f, "relative scope segment (`.` or `..`)"),
            CapError::EmptyPower => write!(f, "empty power name"),
            CapError::EmptyFacetSegment => write!(f, "empty facet-namespace segment"),
            CapError::UnknownKind(kind) => write!(f, "unknown capability kind `{kind}`"),
        }
    }
}

impl std::error::Error for CapError {}

/// A hierarchical node scope, parsed once into validated slash-separated
/// segments.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ScopePath(Vec<String>);

impl ScopePath {
    /// Parse a slash-separated scope. One trailing slash is accepted and
    /// ignored; an interior empty segment is not.
    pub fn parse(raw: &str) -> Result<Self, CapError> {
        let trimmed = raw.strip_suffix('/').unwrap_or(raw);
        if trimmed.is_empty() {
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

    /// The root scope, covering every node scope.
    pub fn root() -> Self {
        Self(Vec::new())
    }

    /// The validated segments.
    pub fn segments(&self) -> &[String] {
        &self.0
    }

    /// Whether this scope covers `other` by segment prefix.
    pub fn covers_scope(&self, other: &Self) -> bool {
        other.0.len() >= self.0.len() && other.0[..self.0.len()] == self.0[..]
    }
}

impl fmt::Display for ScopePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.join("/"))
    }
}

/// A hierarchical facet namespace, parsed into dot-separated segments.
///
/// A trailing dot is the conventional namespace spelling, so `web` and
/// `web.` are the same holding. `web` covers `web.viewer`; it does not cover
/// `website.viewer` or any `denizen.*` facet.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FacetNamespace(Vec<String>);

impl FacetNamespace {
    /// Parse a dot-separated namespace or facet id. One trailing dot is
    /// accepted and ignored; an interior empty segment is refused.
    pub fn parse(raw: &str) -> Result<Self, CapError> {
        let trimmed = raw.strip_suffix('.').unwrap_or(raw);
        if trimmed.is_empty() {
            return Ok(Self(Vec::new()));
        }
        let mut segments = Vec::new();
        for segment in trimmed.split('.') {
            if segment.is_empty() {
                return Err(CapError::EmptyFacetSegment);
            }
            segments.push(segment.to_string());
        }
        Ok(Self(segments))
    }

    /// The root namespace, covering every facet namespace.
    pub fn root() -> Self {
        Self(Vec::new())
    }

    /// The validated segments.
    pub fn segments(&self) -> &[String] {
        &self.0
    }

    /// Whether this namespace covers `other` by dot-segment prefix.
    pub fn covers_namespace(&self, other: &Self) -> bool {
        other.0.len() >= self.0.len() && other.0[..self.0.len()] == self.0[..]
    }
}

impl fmt::Display for FacetNamespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.join("."))
    }
}

/// One structural capability.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Cap {
    /// A named power. Coverage is equality.
    Power(String),
    /// A hierarchical node scope. Coverage is slash-segment prefix.
    Scope(ScopePath),
    /// A hierarchical facet namespace. Coverage is dot-segment prefix.
    Facet(FacetNamespace),
}

impl Cap {
    /// Construct a named power.
    pub fn power(name: impl Into<String>) -> Result<Self, CapError> {
        let name = name.into();
        if name.is_empty() {
            return Err(CapError::EmptyPower);
        }
        Ok(Self::Power(name))
    }

    /// Construct a hierarchical node scope.
    pub fn scope(raw: &str) -> Result<Self, CapError> {
        ScopePath::parse(raw).map(Self::Scope)
    }

    /// Construct a hierarchical facet namespace.
    pub fn facet(raw: &str) -> Result<Self, CapError> {
        FacetNamespace::parse(raw).map(Self::Facet)
    }

    /// The root node scope.
    pub fn root_scope() -> Self {
        Self::Scope(ScopePath::root())
    }

    /// The root facet namespace.
    pub fn root_facet() -> Self {
        Self::Facet(FacetNamespace::root())
    }

    /// Parse `power:<name>`, `scope:<path>`, `facet:<namespace>`, or a legacy
    /// bare scope. New writers always emit a tagged form.
    pub fn parse(raw: &str) -> Result<Self, CapError> {
        match raw.split_once(':') {
            Some(("power", name)) => Self::power(name),
            Some(("scope", path)) => Self::scope(path),
            Some(("facet", namespace)) => Self::facet(namespace),
            Some((kind, _)) if !kind.contains('/') => Err(CapError::UnknownKind(kind.to_string())),
            _ => Self::scope(raw),
        }
    }

    /// The canonical wire form, round-tripping through [`Cap::parse`].
    pub fn to_wire(&self) -> String {
        match self {
            Self::Power(name) => format!("power:{name}"),
            Self::Scope(path) => format!("scope:{path}"),
            Self::Facet(namespace) => format!("facet:{namespace}"),
        }
    }
}

impl Capability for Cap {
    fn covers(&self, needed: &Self) -> bool {
        match (self, needed) {
            (Self::Power(held), Self::Power(want)) => held == want,
            (Self::Scope(held), Self::Scope(want)) => held.covers_scope(want),
            (Self::Facet(held), Self::Facet(want)) => held.covers_namespace(want),
            _ => false,
        }
    }
}

impl fmt::Display for Cap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_wire())
    }
}

/// Access mode, ordered by authority: `Write` covers `Read`, and `Delegate`
/// covers both.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mode {
    /// May observe the covered area.
    Read,
    /// May mutate the covered area.
    Write,
    /// May grant covered capabilities to others.
    Delegate,
}

impl Mode {
    /// Whether holding `self` satisfies `needed`.
    pub fn covers(self, needed: Self) -> bool {
        self >= needed
    }
}

/// Assert reflexivity and transitivity over a representative sample.
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
            Cap::power("navigate").unwrap(),
            Cap::root_facet(),
            Cap::facet("web").unwrap(),
            Cap::facet("web.viewer").unwrap(),
            Cap::facet("denizen.binding").unwrap(),
        ]
    }

    #[test]
    fn the_capability_laws_hold() {
        assert_capability_laws(&sample());
    }

    #[test]
    fn powers_cover_only_themselves() {
        let navigate = Cap::power("navigate").unwrap();
        assert!(navigate.covers(&Cap::power("navigate").unwrap()));
        assert!(!navigate.covers(&Cap::power("navigate-admin").unwrap()));
        assert!(!Cap::power("nav").unwrap().covers(&navigate));
    }

    #[test]
    fn scopes_cover_by_segment() {
        let trail = Cap::scope("trail").unwrap();
        assert!(trail.covers(&Cap::scope("trail/step1").unwrap()));
        assert!(!trail.covers(&Cap::scope("trailer").unwrap()));
        assert!(
            !Cap::scope("app/nav")
                .unwrap()
                .covers(&Cap::scope("app/navigate").unwrap())
        );
        assert!(
            !Cap::scope("app/session")
                .unwrap()
                .covers(&Cap::scope("app/session-admin").unwrap())
        );
    }

    #[test]
    fn facet_namespaces_cover_by_dot_segment() {
        let web = Cap::facet("web.").unwrap();
        assert_eq!(web, Cap::facet("web").unwrap());
        assert!(web.covers(&Cap::facet("web.viewer").unwrap()));
        assert!(web.covers(&Cap::facet("web.viewer.state").unwrap()));
        assert!(!web.covers(&Cap::facet("website.viewer").unwrap()));
        assert!(!web.covers(&Cap::facet("denizen.binding").unwrap()));
    }

    #[test]
    fn malformed_hierarchies_are_refused() {
        assert_eq!(
            ScopePath::parse("scenario/../app/session"),
            Err(CapError::RelativeSegment)
        );
        assert_eq!(ScopePath::parse("a//b"), Err(CapError::EmptySegment));
        assert_eq!(
            FacetNamespace::parse("web..viewer"),
            Err(CapError::EmptyFacetSegment)
        );
        assert_eq!(Cap::power(""), Err(CapError::EmptyPower));
    }

    #[test]
    fn kinds_never_cross() {
        assert!(
            !Cap::scope("web")
                .unwrap()
                .covers(&Cap::facet("web").unwrap())
        );
        assert!(
            !Cap::facet("navigate")
                .unwrap()
                .covers(&Cap::power("navigate").unwrap())
        );
        assert!(!Cap::root_scope().covers(&Cap::root_facet()));
    }

    #[test]
    fn wire_forms_round_trip_and_bare_strings_remain_scopes() {
        for cap in sample() {
            assert_eq!(Cap::parse(&cap.to_wire()), Ok(cap.clone()), "{cap:?}");
        }
        assert_eq!(Cap::parse("trail/"), Ok(Cap::scope("trail").unwrap()));
        assert_eq!(
            Cap::parse("moot:something"),
            Err(CapError::UnknownKind("moot".into()))
        );
    }

    #[test]
    fn modes_have_the_shared_order() {
        assert!(Mode::Write.covers(Mode::Read));
        assert!(Mode::Delegate.covers(Mode::Write));
        assert!(!Mode::Read.covers(Mode::Write));
    }
}
