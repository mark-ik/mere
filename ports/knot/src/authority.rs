// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! One revisioned materialization of authority and routes for a Knot space.

use std::collections::{BTreeMap, BTreeSet};

use servitor::cap::{Cap, Mode};
use servitor::{AuthorityProvider, Subject};

use crate::{KnotSettingsError, KnotSyncSettings};

/// Durable source whose facts produced a space-authority snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnotAuthoritySource {
    /// Personae device pairing for a personal vault.
    PersonalPairing,
    /// Gemot constitution and delegation facts for a communal space.
    GemotCapabilities,
}

/// One immutable, revisioned authority view consumed by operation admission,
/// evidence serving, evidence fetching, and route refresh.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnotSpaceAuthoritySnapshot {
    source: KnotAuthoritySource,
    revision: [u8; 32],
    writers: BTreeSet<[u8; 32]>,
    evidence_readers: BTreeSet<[u8; 32]>,
    evidence_sources: BTreeSet<[u8; 32]>,
    route_hints: BTreeMap<[u8; 32], String>,
}

impl Default for KnotSpaceAuthoritySnapshot {
    fn default() -> Self {
        Self::new(KnotAuthoritySource::PersonalPairing, [], [], [], [])
    }
}

impl KnotSpaceAuthoritySnapshot {
    /// Build a canonical snapshot. Collection order does not affect revision.
    pub fn new(
        source: KnotAuthoritySource,
        writers: impl IntoIterator<Item = [u8; 32]>,
        evidence_readers: impl IntoIterator<Item = [u8; 32]>,
        evidence_sources: impl IntoIterator<Item = [u8; 32]>,
        route_hints: impl IntoIterator<Item = ([u8; 32], String)>,
    ) -> Self {
        let writers = writers.into_iter().collect::<BTreeSet<_>>();
        let evidence_readers = evidence_readers.into_iter().collect::<BTreeSet<_>>();
        let evidence_sources = evidence_sources.into_iter().collect::<BTreeSet<_>>();
        let route_hints = route_hints.into_iter().collect::<BTreeMap<_, _>>();
        let revision = authority_revision(
            source,
            &writers,
            &evidence_readers,
            &evidence_sources,
            &route_hints,
        );
        Self {
            source,
            revision,
            writers,
            evidence_readers,
            evidence_sources,
            route_hints,
        }
    }

    /// Materialize personal authority and cached routes from Personae pairing.
    pub fn from_personal_settings(settings: &KnotSyncSettings) -> Result<Self, KnotSettingsError> {
        let writers = settings.paired_writer_keys()?;
        let route_hints = writers
            .iter()
            .filter_map(|writer| {
                settings
                    .endpoint_for(writer)
                    .map(|ticket| (*writer, ticket.to_string()))
            })
            .collect::<Vec<_>>();
        Ok(Self::new(
            KnotAuthoritySource::PersonalPairing,
            writers.iter().copied(),
            writers.iter().copied(),
            writers.iter().copied(),
            route_hints,
        ))
    }

    /// Materialize independent communal rights from Gemot authority facts.
    pub fn from_gemot_authority(
        authority: &impl AuthorityProvider,
        space_id: [u8; 32],
        candidates: impl IntoIterator<Item = [u8; 32]>,
        route_hints: impl IntoIterator<Item = ([u8; 32], String)>,
    ) -> Result<Self, String> {
        let scope = format!("knot/{}", crate::hex32(&space_id));
        let document = Cap::scope(&format!("{scope}/document"))
            .map_err(|error| format!("invalid Knot document capability: {error}"))?;
        let evidence_read = Cap::scope(&format!("{scope}/evidence/read"))
            .map_err(|error| format!("invalid Knot evidence-read capability: {error}"))?;
        let evidence_source = Cap::scope(&format!("{scope}/evidence/source"))
            .map_err(|error| format!("invalid Knot evidence-source capability: {error}"))?;
        let candidates = candidates.into_iter().collect::<BTreeSet<_>>();
        let writers = candidates
            .iter()
            .copied()
            .filter(|peer| authority.covers(Subject(*peer), &document, Mode::Write));
        let evidence_readers = candidates
            .iter()
            .copied()
            .filter(|peer| authority.covers(Subject(*peer), &evidence_read, Mode::Read));
        let evidence_sources = candidates
            .iter()
            .copied()
            .filter(|peer| authority.covers(Subject(*peer), &evidence_source, Mode::Write));
        Ok(Self::new(
            KnotAuthoritySource::GemotCapabilities,
            writers,
            evidence_readers,
            evidence_sources,
            route_hints,
        ))
    }

    /// Authority source, used to reject cross-domain materializations.
    pub const fn source(&self) -> KnotAuthoritySource {
        self.source
    }

    /// Content-derived revision of every set and route in this view.
    pub const fn revision(&self) -> [u8; 32] {
        self.revision
    }

    /// Peers allowed to contribute document operations.
    pub fn writers(&self) -> impl Iterator<Item = [u8; 32]> + '_ {
        self.writers.iter().copied()
    }

    /// Peers allowed to read retained evidence from this space.
    pub fn evidence_readers(&self) -> impl Iterator<Item = [u8; 32]> + '_ {
        self.evidence_readers.iter().copied()
    }

    /// Peers this device may fetch evidence from.
    pub fn evidence_sources(&self) -> impl Iterator<Item = [u8; 32]> + '_ {
        self.evidence_sources.iter().copied()
    }

    /// Cached routes keyed by authenticated peer identity.
    pub fn route_hints(&self) -> impl Iterator<Item = (&[u8; 32], &str)> {
        self.route_hints
            .iter()
            .map(|(peer, ticket)| (peer, ticket.as_str()))
    }

    /// Cached route for one authenticated peer.
    pub fn route_hint(&self, peer: &[u8; 32]) -> Option<&str> {
        self.route_hints.get(peer).map(String::as_str)
    }
}

fn authority_revision(
    source: KnotAuthoritySource,
    writers: &BTreeSet<[u8; 32]>,
    evidence_readers: &BTreeSet<[u8; 32]>,
    evidence_sources: &BTreeSet<[u8; 32]>,
    route_hints: &BTreeMap<[u8; 32], String>,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"knot.space-authority/v1\0");
    hasher.update(&[match source {
        KnotAuthoritySource::PersonalPairing => 1,
        KnotAuthoritySource::GemotCapabilities => 2,
    }]);
    hash_peers(&mut hasher, b"writers", writers);
    hash_peers(&mut hasher, b"evidence-readers", evidence_readers);
    hash_peers(&mut hasher, b"evidence-sources", evidence_sources);
    hasher.update(b"route-hints");
    hasher.update(&(route_hints.len() as u64).to_be_bytes());
    for (peer, ticket) in route_hints {
        hasher.update(peer);
        hasher.update(&(ticket.len() as u64).to_be_bytes());
        hasher.update(ticket.as_bytes());
    }
    *hasher.finalize().as_bytes()
}

fn hash_peers(hasher: &mut blake3::Hasher, label: &[u8], peers: &BTreeSet<[u8; 32]>) {
    hasher.update(label);
    hasher.update(&(peers.len() as u64).to_be_bytes());
    for peer in peers {
        hasher.update(peer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_covers_each_independent_dimension_but_not_input_order() {
        let base = KnotSpaceAuthoritySnapshot::new(
            KnotAuthoritySource::GemotCapabilities,
            [[1; 32], [2; 32]],
            [[3; 32]],
            [[4; 32]],
            [([4; 32], "route-a".into())],
        );
        let reordered = KnotSpaceAuthoritySnapshot::new(
            KnotAuthoritySource::GemotCapabilities,
            [[2; 32], [1; 32]],
            [[3; 32]],
            [[4; 32]],
            [([4; 32], "route-a".into())],
        );
        assert_eq!(base.revision(), reordered.revision());

        let changed_reader = KnotSpaceAuthoritySnapshot::new(
            KnotAuthoritySource::GemotCapabilities,
            [[1; 32], [2; 32]],
            [[5; 32]],
            [[4; 32]],
            [([4; 32], "route-a".into())],
        );
        let changed_route = KnotSpaceAuthoritySnapshot::new(
            KnotAuthoritySource::GemotCapabilities,
            [[1; 32], [2; 32]],
            [[3; 32]],
            [[4; 32]],
            [([4; 32], "route-b".into())],
        );
        assert_ne!(base.revision(), changed_reader.revision());
        assert_ne!(base.revision(), changed_route.revision());
    }

    #[test]
    fn losing_a_route_changes_reachability_materialization_not_rights() {
        let routed = KnotSpaceAuthoritySnapshot::new(
            KnotAuthoritySource::GemotCapabilities,
            [[1; 32]],
            [[2; 32]],
            [[3; 32]],
            [([3; 32], "route".into())],
        );
        let route_lost = KnotSpaceAuthoritySnapshot::new(
            KnotAuthoritySource::GemotCapabilities,
            [[1; 32]],
            [[2; 32]],
            [[3; 32]],
            [],
        );

        assert_eq!(
            routed.writers().collect::<Vec<_>>(),
            route_lost.writers().collect::<Vec<_>>()
        );
        assert_eq!(
            routed.evidence_readers().collect::<Vec<_>>(),
            route_lost.evidence_readers().collect::<Vec<_>>()
        );
        assert_eq!(
            routed.evidence_sources().collect::<Vec<_>>(),
            route_lost.evidence_sources().collect::<Vec<_>>()
        );
        assert_ne!(routed.revision(), route_lost.revision());
    }
}
