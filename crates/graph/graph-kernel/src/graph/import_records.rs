// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Import-record operations.
//!
//! Manages the durable `import_records` truth on `Graph`: queries,
//! membership suppression, replacement, and the two private helpers
//! that sync between `import_records` and per-node
//! `NodeImportProvenance` (`sync_node_import_provenance_from_records`
//! / `rebuild_import_records_from_node_provenance`).
//!
//! Also includes `set_node_import_provenance` because it triggers a
//! rebuild and naturally lives with its sibling import-record
//! plumbing.
//!
//! Extracted from `graph/mod.rs` per the 2026-05-11 kernel
//! decomposition pass.

use std::collections::{BTreeMap, HashMap};

use uuid::Uuid;

use super::identity::NodeKey;
use super::{Graph, current_unix_timestamp_secs, normalize_import_records};
use crate::types::{
    ImportRecord, ImportRecordMembership, NodeImportProvenance, NodeImportRecordSummary,
};

impl Graph {
    pub fn import_records(&self) -> &[ImportRecord] {
        &self.import_records
    }

    pub fn import_record_summaries_for_node(&self, key: NodeKey) -> Vec<NodeImportRecordSummary> {
        let Some(node) = self.get_node(key) else {
            return Vec::new();
        };
        let node_id = node.id.to_string();
        let mut summaries = self
            .import_records
            .iter()
            .filter(|record| {
                record
                    .memberships
                    .iter()
                    .any(|membership| membership.node_id == node_id && !membership.suppressed)
            })
            .map(|record| NodeImportRecordSummary {
                record_id: record.record_id.clone(),
                source_id: record.source_id.clone(),
                source_label: record.source_label.clone(),
                imported_at_secs: record.imported_at_secs,
            })
            .collect::<Vec<_>>();
        summaries.sort_by(|left, right| {
            right
                .imported_at_secs
                .cmp(&left.imported_at_secs)
                .then_with(|| left.source_label.cmp(&right.source_label))
                .then_with(|| left.record_id.cmp(&right.record_id))
        });
        summaries
    }

    pub fn import_record_member_keys(&self, record_id: &str) -> Vec<NodeKey> {
        let mut member_keys = self
            .import_records
            .iter()
            .find(|record| record.record_id == record_id)
            .into_iter()
            .flat_map(|record| record.memberships.iter())
            .filter(|membership| !membership.suppressed)
            .filter_map(|membership| Uuid::parse_str(&membership.node_id).ok())
            .filter_map(|node_id| self.inner.key_of(&node_id))
            .collect::<Vec<_>>();
        member_keys.sort_by_key(|key| key.index());
        member_keys.dedup();
        member_keys
    }

    pub(crate) fn delete_import_record(&mut self, record_id: &str) -> bool {
        let original_len = self.import_records.len();
        self.import_records
            .retain(|record| record.record_id != record_id);
        if self.import_records.len() == original_len {
            return false;
        }
        self.sync_node_import_provenance_from_records();
        true
    }

    pub(crate) fn set_import_record_membership_suppressed(
        &mut self,
        record_id: &str,
        key: NodeKey,
        suppressed: bool,
    ) -> bool {
        let Some(node_id) = self.get_node(key).map(|node| node.id.to_string()) else {
            return false;
        };
        let mut changed = false;
        for record in &mut self.import_records {
            if record.record_id != record_id {
                continue;
            }
            for membership in &mut record.memberships {
                if membership.node_id == node_id {
                    if membership.suppressed != suppressed {
                        membership.suppressed = suppressed;
                        changed = true;
                    }
                    break;
                }
            }
        }
        if changed {
            self.sync_node_import_provenance_from_records();
        }
        changed
    }

    pub(crate) fn set_import_records(&mut self, mut import_records: Vec<ImportRecord>) -> bool {
        normalize_import_records(&mut import_records);
        if self.import_records == import_records {
            return false;
        }
        self.import_records = import_records;
        self.sync_node_import_provenance_from_records();
        true
    }

    pub(crate) fn sync_node_import_provenance_from_records(&mut self) {
        let mut provenance_by_node = HashMap::<NodeKey, Vec<NodeImportProvenance>>::new();
        for record in &self.import_records {
            for membership in record
                .memberships
                .iter()
                .filter(|membership| !membership.suppressed)
            {
                let Ok(node_id) = Uuid::parse_str(&membership.node_id) else {
                    continue;
                };
                let Some(node_key) = self.inner.key_of(&node_id) else {
                    continue;
                };
                provenance_by_node
                    .entry(node_key)
                    .or_default()
                    .push(NodeImportProvenance {
                        source_id: record.source_id.clone(),
                        source_label: record.source_label.clone(),
                    });
            }
        }

        let node_keys = self.inner.inner().node_indices().collect::<Vec<_>>();
        for node_key in node_keys {
            let mut provenance = provenance_by_node.remove(&node_key).unwrap_or_default();
            provenance.sort();
            provenance.dedup();
            self.set_node_facet(node_key, super::node_facets::PROVENANCE_IMPORT, &provenance);
        }
    }

    pub(crate) fn rebuild_import_records_from_node_provenance(&mut self, imported_at_secs: u64) {
        let existing_record_meta = self
            .import_records
            .iter()
            .map(|record| {
                (
                    (record.source_id.clone(), record.source_label.clone()),
                    (record.record_id.clone(), record.imported_at_secs),
                )
            })
            .collect::<HashMap<_, _>>();

        let mut grouped = BTreeMap::<(String, String), Vec<ImportRecordMembership>>::new();
        let nodes = self
            .nodes()
            .map(|(node_key, node)| (node_key, node.id))
            .collect::<Vec<_>>();
        for (node_key, node_id) in nodes {
            let node_id = node_id.to_string();
            for provenance in self.node_import_provenance(node_key).unwrap_or_default() {
                grouped
                    .entry((provenance.source_id, provenance.source_label))
                    .or_default()
                    .push(ImportRecordMembership {
                        node_id: node_id.clone(),
                        suppressed: false,
                    });
            }
        }

        let mut import_records = grouped
            .into_iter()
            .map(|((source_id, source_label), memberships)| {
                let (record_id, imported_at_secs) = existing_record_meta
                    .get(&(source_id.clone(), source_label.clone()))
                    .cloned()
                    .unwrap_or_else(|| (format!("import-record:{}", source_id), imported_at_secs));
                ImportRecord {
                    record_id,
                    source_id,
                    source_label,
                    imported_at_secs,
                    memberships,
                }
            })
            .collect::<Vec<_>>();
        normalize_import_records(&mut import_records);
        self.import_records = import_records;
        self.sync_node_import_provenance_from_records();
    }

    pub(crate) fn set_node_import_provenance(
        &mut self,
        key: NodeKey,
        import_provenance: Vec<NodeImportProvenance>,
    ) -> bool {
        if self.inner.node(key).is_none() {
            return false;
        }
        let mut normalized = import_provenance;
        normalized.sort();
        normalized.dedup();
        if self.node_import_provenance(key).unwrap_or_default() == normalized {
            return false;
        }
        self.set_node_facet(key, super::node_facets::PROVENANCE_IMPORT, &normalized);
        self.rebuild_import_records_from_node_provenance(current_unix_timestamp_secs());
        true
    }
}
