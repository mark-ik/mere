/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `Coupling` — how nodes respond to a field. A coupling is
//! `field -> NodeSelector (a dynamic set) x response x strength`.
//!
//! This is the "seventh relation kind" of the
//! [field-system extraction](../../../../../design_docs/mere_docs/technical_architecture/2026-05-30_field_system_extraction.md),
//! realized as a **tracked first-class field-layer primitive** (it has identity,
//! lifecycle, and persistence) rather than an `EdgeFamily` variant: a coupling
//! targets a *selector* over nodes, not a single node, so it cannot be an
//! `EdgePayload` sidecar on a node-to-node edge, and `EdgeFamily` (derived from
//! node-edge sidecars) stays six.
//!
//! Ported from `aether::coupling`. The response vocabulary is a
//! recognized-core-plus-open-tail hybrid (plan Phase 4): the six force responses
//! are the recognized core `gyre` integrates, and [`CouplingResponse::Open`]
//! carries the open families (visual / navigational / selection / semantic /
//! trigger) by IRI until a consumer recognizes them. See [`CouplingResponse`].
//! Derives are serde only here; rkyv lands at the `Persisted*` DTO layer (plan
//! Phase 2). WASM-clean.

use serde::{Deserialize, Serialize};

use super::field::{CouplingId, FieldId};

/// Selects which nodes a coupling applies to. Tag and kind names are opaque
/// here — they resolve against the node's tags / classifications.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeSelector {
    /// Apply to every node.
    All,
    /// Apply to nodes carrying this tag.
    Tagged(String),
    /// Apply to nodes of this kind.
    Kind(String),
    /// Apply to nodes NOT carrying this tag.
    NotTagged(String),
}

/// Base IRI for Mere's coupling-response vocabulary (the
/// [statements-over-schema](../../../../../design_docs/mere_docs/technical_architecture/2026-05-22_statements_over_schema_stance.md)
/// substrate applied to responses). The recognized force core maps to
/// `{COUPLING_VOCAB}{slug}` via [`CouplingResponse::recognized_iri`]; the families
/// beyond force name themselves under sub-namespaces, e.g.
/// `…/coupling#visual/highlight`, `nav/focus`, `selection/member`,
/// `semantic/assert`, `trigger/fire`. Provisional base; mirrors
/// [`crate::graph::edge_taxonomy`]'s `REL_VOCAB`.
pub const COUPLING_VOCAB: &str = "https://mere.computer/ns/coupling#";

/// How a node responds to a field at its position.
///
/// **Recognized core (force).** The six force responses are the recognized core:
/// `gyre` dispatches on them to integrate motion. This is the statements-over-schema
/// "recognized core gets behavior" half; the responses are also the v1 surface.
///
/// **Open tail.** The coupling contract is deliberately open beyond force (the
/// [field-system extraction](../../../../../design_docs/mere_docs/technical_architecture/2026-05-30_field_system_extraction.md)
/// names visual, navigational, selection, semantic, and trigger families).
/// [`CouplingResponse::Open`] carries any such response by an open IRI
/// ([`COUPLING_VOCAB`]), stored faithfully and handled generically until a
/// consumer recognizes it. The force integrator ignores `Open`, so the open tail
/// never breaks dispatch. Recognized-core IRIs round-trip via
/// [`recognized_iri`](Self::recognized_iri) / [`from_iri`](Self::from_iri).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, strum::EnumIter)]
pub enum CouplingResponse {
    /// Move along `-grad(scalar)` (gradient descent on a potential).
    AttractToMin,
    /// Move along `+grad(scalar)` (gradient ascent).
    RepelFromMax,
    /// Set velocity equal to the vector field at this position.
    AlignVelocity,
    /// Advect: `pos += dt * field(pos)`.
    FlowAdvect,
    /// Multiplicative damping when inside a positive scalar region.
    DampenInside { factor: f32 },
    /// Hard pushout when the scalar field exceeds zero.
    ContainmentWall,
    /// Open tail: a non-force response identified by an open IRI (see
    /// [`COUPLING_VOCAB`]). Stored faithfully, ignored by the force integrator,
    /// recognized by whatever consumer owns its family (paint, camera, …).
    Open { predicate: String },
}

impl CouplingResponse {
    /// Build a response from an IRI, canonicalizing a recognized core IRI back to
    /// its typed variant and wrapping anything else as [`Self::Open`].
    pub fn open(predicate: impl Into<String>) -> Self {
        let predicate = predicate.into();
        Self::from_iri(&predicate).unwrap_or(Self::Open { predicate })
    }

    /// Whether this is a recognized force-core response (vs an open-tail one).
    pub fn is_force(&self) -> bool {
        !matches!(self, Self::Open { .. })
    }

    /// The open predicate IRI for an [`Open`](Self::Open) response; `None` for the
    /// recognized force core (whose IRI is [`recognized_iri`](Self::recognized_iri)).
    pub fn predicate(&self) -> Option<&str> {
        match self {
            Self::Open { predicate } => Some(predicate),
            _ => None,
        }
    }

    /// Canonical IRI for a recognized force-core response; `None` for `Open`.
    /// Variant params (e.g. `DampenInside.factor`) are statement-level data, not
    /// part of the IRI.
    pub fn recognized_iri(&self) -> Option<&'static str> {
        Some(match self {
            Self::AttractToMin => "https://mere.computer/ns/coupling#attract-to-min",
            Self::RepelFromMax => "https://mere.computer/ns/coupling#repel-from-max",
            Self::AlignVelocity => "https://mere.computer/ns/coupling#align-velocity",
            Self::FlowAdvect => "https://mere.computer/ns/coupling#flow-advect",
            Self::DampenInside { .. } => "https://mere.computer/ns/coupling#dampen-inside",
            Self::ContainmentWall => "https://mere.computer/ns/coupling#containment-wall",
            Self::Open { .. } => return None,
        })
    }

    /// The recognized force-core response for a canonical IRI, if any. Inverse of
    /// [`recognized_iri`](Self::recognized_iri). `DampenInside` comes back with
    /// `factor = 1.0` (the no-op identity); the real factor is carried separately,
    /// not in the IRI.
    pub fn from_iri(iri: &str) -> Option<Self> {
        Some(match iri {
            "https://mere.computer/ns/coupling#attract-to-min" => Self::AttractToMin,
            "https://mere.computer/ns/coupling#repel-from-max" => Self::RepelFromMax,
            "https://mere.computer/ns/coupling#align-velocity" => Self::AlignVelocity,
            "https://mere.computer/ns/coupling#flow-advect" => Self::FlowAdvect,
            "https://mere.computer/ns/coupling#dampen-inside" => Self::DampenInside { factor: 1.0 },
            "https://mere.computer/ns/coupling#containment-wall" => Self::ContainmentWall,
            _ => return None,
        })
    }
}

/// One coupling rule: identity + target field + selector + response + strength.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Coupling {
    pub id: CouplingId,
    pub field: FieldId,
    pub selector: NodeSelector,
    pub response: CouplingResponse,
    pub strength: f32,
}

impl Coupling {
    pub fn new(
        id: CouplingId,
        field: FieldId,
        selector: NodeSelector,
        response: CouplingResponse,
        strength: f32,
    ) -> Self {
        Self {
            id,
            field,
            selector,
            response,
            strength,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn uuid(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    #[test]
    fn coupling_serde_roundtrip() {
        let c = Coupling::new(
            CouplingId::from_uuid(uuid(1)),
            FieldId::from_uuid(uuid(2)),
            NodeSelector::Kind("paper".into()),
            CouplingResponse::AttractToMin,
            1.0,
        );
        let s = serde_json::to_string(&c).unwrap();
        assert_eq!(c, serde_json::from_str::<Coupling>(&s).unwrap());
    }

    #[test]
    fn selector_and_response_variants_roundtrip() {
        let selectors = [
            NodeSelector::All,
            NodeSelector::Tagged("important".into()),
            NodeSelector::Kind("paper".into()),
            NodeSelector::NotTagged("archived".into()),
        ];
        for s in &selectors {
            let j = serde_json::to_string(s).unwrap();
            assert_eq!(*s, serde_json::from_str::<NodeSelector>(&j).unwrap());
        }
        let responses = [
            CouplingResponse::AttractToMin,
            CouplingResponse::RepelFromMax,
            CouplingResponse::AlignVelocity,
            CouplingResponse::FlowAdvect,
            CouplingResponse::DampenInside { factor: 0.3 },
            CouplingResponse::ContainmentWall,
        ];
        for r in &responses {
            let j = serde_json::to_string(r).unwrap();
            assert_eq!(*r, serde_json::from_str::<CouplingResponse>(&j).unwrap());
        }
    }

    #[test]
    fn open_response_serde_roundtrip() {
        let r = CouplingResponse::Open {
            predicate: format!("{COUPLING_VOCAB}visual/highlight"),
        };
        assert!(!r.is_force());
        assert_eq!(r.predicate(), Some("https://mere.computer/ns/coupling#visual/highlight"));
        let j = serde_json::to_string(&r).unwrap();
        assert_eq!(r, serde_json::from_str::<CouplingResponse>(&j).unwrap());
    }

    #[test]
    fn recognized_response_iri_round_trips() {
        use strum::IntoEnumIterator;
        // Every recognized force-core response round-trips through its canonical
        // IRI (param-bearing variants compare by IRI, since the param is not in it).
        for r in CouplingResponse::iter().filter(|r| r.is_force()) {
            let iri = r.recognized_iri().expect("force core has an IRI");
            let back = CouplingResponse::from_iri(iri).expect("IRI is recognized");
            assert_eq!(back.recognized_iri(), Some(iri), "IRI round-trips for {r:?}");
        }
        // Open has no recognized IRI; an unknown IRI is not recognized.
        assert!(
            CouplingResponse::Open {
                predicate: "x".into()
            }
            .recognized_iri()
            .is_none()
        );
        assert!(CouplingResponse::from_iri("https://example.org/unknown").is_none());
    }

    #[test]
    fn open_canonicalizes_recognized_iri() {
        // `open` recognizes a canonical core IRI back to its typed variant, and
        // wraps anything else as Open.
        assert_eq!(
            CouplingResponse::open("https://mere.computer/ns/coupling#attract-to-min"),
            CouplingResponse::AttractToMin
        );
        assert!(matches!(
            CouplingResponse::open(format!("{COUPLING_VOCAB}trigger/fire")),
            CouplingResponse::Open { .. }
        ));
    }
}
