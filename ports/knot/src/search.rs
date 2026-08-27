// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Capability-scoped search across files-in-place and sealed vault documents.

use std::fs;

use esp::embed::{LexicalEmbeddingProvider, SemanticSearch};
use serde::{Deserialize, Serialize};
use servitor::{AuthorityProvider, Cap, Mode, Subject};

use crate::{DirectorySource, KnotVault};

const DISK_SCOPE: &str = "knot/search/disk";
const VAULT_SCOPE: &str = "knot/search/vault";

/// Which source lane produced a search result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchLane {
    /// A file whose bytes remain authoritative on disk.
    Disk,
    /// A document held by the sealed vault.
    Vault,
}

/// One capability-filtered search result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    pub id: String,
    pub lane: SearchLane,
    pub score: f32,
}

/// Host-selected bounds for the local lexical index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchConfig {
    /// Number of feature-hash buckets.
    pub dimensions: usize,
    /// Largest disk document Knot will read for indexing.
    pub max_file_bytes: u64,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            dimensions: 512,
            max_file_bytes: 2 * 1024 * 1024,
        }
    }
}

/// Search state. The disk index is live in memory; the vault index is sealed
/// and opened only for an authorized query while the vault is unlocked.
pub struct KnotSearch {
    config: SearchConfig,
    disk: SemanticSearch<String, LexicalEmbeddingProvider>,
}

impl KnotSearch {
    /// Build both lanes. Disk decoding skips binary and over-limit files.
    /// Vault embeddings are immediately sealed through Personae.
    pub fn build(
        directory: Option<&DirectorySource>,
        vault: Option<&KnotVault>,
        config: SearchConfig,
    ) -> Result<Self, String> {
        let provider = || {
            LexicalEmbeddingProvider::new(config.dimensions)
                .map_err(|error| format!("invalid Knot search configuration: {error}"))
        };
        let mut disk = SemanticSearch::new(provider()?);
        if let Some(directory) = directory {
            for document in directory.documents() {
                if document.byte_size > config.max_file_bytes {
                    continue;
                }
                let Ok(body) = fs::read_to_string(&document.path) else {
                    continue;
                };
                let text = format!("{}\n{body}", document.container.title);
                disk.ingest(document.id.clone(), &text).map_err(|error| {
                    format!("could not index {}: {error}", document.path.display())
                })?;
            }
        }

        if let Some(vault) = vault {
            if vault.is_locked() {
                return Err("cannot build the Knot vault index while locked".into());
            }
            let mut sealed = SemanticSearch::new(provider()?);
            for document in vault.documents() {
                let Ok(body) = std::str::from_utf8(&document.body) else {
                    continue;
                };
                let text = format!("{}\n{body}", document.title);
                sealed.ingest(document.id.clone(), &text).map_err(|error| {
                    format!("could not index vault document {}: {error}", document.id)
                })?;
            }
            vault.store_search_index(sealed.index())?;
        }

        Ok(Self { config, disk })
    }

    /// Search only lanes covered by the caller's read grants.
    ///
    /// A locked vault contributes nothing even if the subject holds its grant.
    pub fn query(
        &self,
        vault: Option<&KnotVault>,
        query: &str,
        k: usize,
        subject: Subject,
        authority: &impl AuthorityProvider,
    ) -> Result<Vec<SearchHit>, String> {
        if k == 0 {
            return Err("Knot search result count must be positive".into());
        }
        let mut hits = Vec::new();
        if covers(authority, subject, DISK_SCOPE)? {
            hits.extend(
                self.disk
                    .search(query, k)
                    .map_err(|error| format!("could not search Knot disk index: {error}"))?
                    .into_iter()
                    .map(|(id, score)| SearchHit {
                        id,
                        lane: SearchLane::Disk,
                        score,
                    }),
            );
        }

        if covers(authority, subject, VAULT_SCOPE)?
            && let Some(vault) = vault.filter(|vault| !vault.is_locked())
            && let Some(index) = vault.load_search_index()?
        {
            let search = SemanticSearch::with_index(
                LexicalEmbeddingProvider::new(self.config.dimensions)
                    .map_err(|error| format!("invalid Knot search configuration: {error}"))?,
                index,
            )
            .map_err(|error| format!("could not open Knot vault index: {error}"))?;
            hits.extend(
                search
                    .search(query, k)
                    .map_err(|error| format!("could not search Knot vault index: {error}"))?
                    .into_iter()
                    .map(|(id, score)| SearchHit {
                        id,
                        lane: SearchLane::Vault,
                        score,
                    }),
            );
        }

        hits.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.id.cmp(&right.id))
        });
        hits.truncate(k);
        Ok(hits)
    }

    /// Capability a subject needs to read disk search results.
    pub fn disk_cap() -> Cap {
        Cap::scope(DISK_SCOPE).expect("static Knot disk scope is valid")
    }

    /// Capability a subject needs to read vault search results.
    pub fn vault_cap() -> Cap {
        Cap::scope(VAULT_SCOPE).expect("static Knot vault scope is valid")
    }
}

fn covers(
    authority: &impl AuthorityProvider,
    subject: Subject,
    scope: &str,
) -> Result<bool, String> {
    let cap = Cap::scope(scope).map_err(|error| format!("invalid Knot search scope: {error}"))?;
    Ok(authority.covers(subject, &cap, Mode::Read))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use servitor::{Grant, GrantTable};
    use tempfile::tempdir;

    use super::*;
    use crate::VaultDocument;
    use crate::vault::SEARCH_INDEX_PATH;

    fn subject() -> Subject {
        Subject::new([0x61; 32])
    }

    fn grant(cap: Cap) -> GrantTable {
        GrantTable::new().with_grant(Grant::new(subject(), cap, Mode::Read))
    }

    fn note(id: &str, title: &str, body: &str) -> VaultDocument {
        VaultDocument {
            id: id.into(),
            title: title.into(),
            body: body.as_bytes().to_vec(),
            media_type: "text/vnd.knot".into(),
        }
    }

    #[test]
    fn search_spans_disk_and_vault_but_respects_lane_grants() {
        let temp = tempdir().unwrap();
        let disk_root = temp.path().join("files");
        let vault_root = temp.path().join("vault");
        fs::create_dir(&disk_root).unwrap();
        fs::write(disk_root.join("runtime.md"), "rust async runtime internals").unwrap();
        let directory = DirectorySource::open(&disk_root).unwrap();
        let mut vault = KnotVault::open(&vault_root, [0x62; 32]).unwrap();
        vault
            .put(note(
                "orchard",
                "Private orchard",
                "orchard pruning observations",
            ))
            .unwrap();

        let search =
            KnotSearch::build(Some(&directory), Some(&vault), SearchConfig::default()).unwrap();
        let both = GrantTable::new()
            .with_grant(Grant::new(subject(), KnotSearch::disk_cap(), Mode::Read))
            .with_grant(Grant::new(subject(), KnotSearch::vault_cap(), Mode::Read));
        let orchard = search
            .query(Some(&vault), "orchard observations", 2, subject(), &both)
            .unwrap();
        assert_eq!(orchard[0].lane, SearchLane::Vault);

        let disk_only = search
            .query(
                Some(&vault),
                "orchard observations",
                2,
                subject(),
                &grant(KnotSearch::disk_cap()),
            )
            .unwrap();
        assert!(disk_only.iter().all(|hit| hit.lane == SearchLane::Disk));

        let vault_only = search
            .query(
                Some(&vault),
                "rust async",
                2,
                subject(),
                &grant(KnotSearch::vault_cap()),
            )
            .unwrap();
        assert!(vault_only.iter().all(|hit| hit.lane == SearchLane::Vault));
    }

    #[test]
    fn locked_vault_has_no_hits_and_its_derived_index_is_sealed() {
        let temp = tempdir().unwrap();
        let vault_root = temp.path().join("vault");
        let mut vault = KnotVault::open(&vault_root, [0x63; 32]).unwrap();
        vault
            .put(note(
                "private-orchard",
                "Private orchard",
                "confidential quince harvest",
            ))
            .unwrap();
        let search = KnotSearch::build(None, Some(&vault), SearchConfig::default()).unwrap();

        let sealed = fs::read(vault_root.join(SEARCH_INDEX_PATH)).unwrap();
        for plaintext in [b"private-orchard".as_slice(), b"confidential".as_slice()] {
            assert!(
                !sealed
                    .windows(plaintext.len())
                    .any(|window| window == plaintext)
            );
        }

        let authority = grant(KnotSearch::vault_cap());
        assert_eq!(
            search
                .query(Some(&vault), "quince harvest", 1, subject(), &authority,)
                .unwrap()[0]
                .id,
            "private-orchard"
        );
        vault.lock();
        assert!(
            search
                .query(Some(&vault), "quince harvest", 1, subject(), &authority,)
                .unwrap()
                .is_empty()
        );
    }
}
