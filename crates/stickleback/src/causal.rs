//! Deterministic causal bookkeeping shared by replicated domains.
//!
//! Stickleback stores and reconciles signed operations. Domains still own
//! their event grammar and fold, but they should not each reimplement the
//! mechanics for exact observed frontiers, topological ordering, missing-parent
//! diagnostics, and bounded causal metadata.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use p2panda_core::{Extensions, Operation};

/// Resource limits for one causally-addressed operation.
///
/// Profiles should choose tighter values where their carrier or grammar makes
/// that useful. These defaults are a safe desktop ceiling, not a radio budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CausalLimits {
    /// Maximum cross-author parents declared by one operation.
    pub max_parents: usize,
    /// Maximum signed operation-body size.
    pub max_payload_bytes: u64,
}

impl Default for CausalLimits {
    fn default() -> Self {
        Self {
            max_parents: 64,
            max_payload_bytes: 1024 * 1024,
        }
    }
}

/// One operation's domain-neutral causal metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CausalEntry<L> {
    /// Signed operation id.
    pub operation: [u8; 32],
    /// Operation signing key.
    pub author: [u8; 32],
    /// Domain log selected at admission.
    pub log_id: L,
    /// Position in the author's selected log.
    pub seq_num: u32,
    /// Previous operation in the same author/log, when present.
    pub backlink: Option<[u8; 32]>,
    /// Exact observed cross-author frontier.
    pub parents: Vec<[u8; 32]>,
}

impl<L> CausalEntry<L> {
    /// Build causal metadata from a retained operation and its decoded parents.
    pub fn from_operation<E: Extensions>(
        operation: &Operation<E>,
        log_id: L,
        parents: Vec<[u8; 32]>,
    ) -> Self {
        Self {
            operation: *operation.hash.as_bytes(),
            author: *operation.header.verifying_key.as_bytes(),
            log_id,
            seq_num: operation.header.seq_num,
            backlink: operation
                .header
                .backlink
                .as_ref()
                .map(|hash| *hash.as_bytes()),
            parents,
        }
    }

    fn dependencies(&self) -> BTreeSet<[u8; 32]> {
        self.backlink
            .into_iter()
            .chain(self.parents.iter().copied())
            .collect()
    }
}

/// One retained operation blocked on unavailable causal history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingCausalOperation {
    /// The blocked operation.
    pub operation: [u8; 32],
    /// Missing root dependencies. Descendants report the same roots rather than
    /// only naming an already-known but itself-blocked parent.
    pub missing: Vec<[u8; 32]>,
}

/// A causally closed projection prefix plus blocked-tail diagnostics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CausalProjection {
    /// Indices into the caller's entry slice, in deterministic causal order.
    pub order: Vec<usize>,
    /// Operations excluded because a dependency is unavailable.
    pub pending: Vec<PendingCausalOperation>,
}

/// Invalid or unprojectable causal metadata.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CausalError {
    /// Parent fan-out exceeds the configured profile limit.
    #[error("operation declares {actual} causal parents; maximum is {maximum}")]
    ParentLimit { actual: usize, maximum: usize },
    /// The signed body exceeds the configured profile limit.
    #[error("operation payload is {actual} bytes; maximum is {maximum}")]
    PayloadLimit { actual: u64, maximum: u64 },
    /// Repeated parents are rejected rather than spending wire and fold budget.
    #[error("operation repeats a causal parent")]
    DuplicateParent,
    /// An operation cannot depend on its own signed id.
    #[error("operation names itself as a causal parent")]
    SelfParent,
    /// One operation id appeared with two entries.
    #[error("operation id appears more than once")]
    DuplicateOperation,
    /// A per-author log fork reused one sequence number for different ids.
    #[error("author/log sequence {seq_num} contains a fork")]
    ForkedSequence { seq_num: u32 },
    /// The causally complete subset contains a dependency cycle.
    #[error("causal dependencies contain a cycle")]
    Cycle,
    /// The author has exhausted p2panda's sequence space.
    #[error("author log sequence is exhausted")]
    SequenceExhausted,
}

/// Enforce parent fan-out, duplicate/self-parent, and payload-size limits.
pub fn validate_causal_metadata<E: Extensions>(
    operation: &Operation<E>,
    parents: &[[u8; 32]],
    limits: CausalLimits,
) -> Result<(), CausalError> {
    if parents.len() > limits.max_parents {
        return Err(CausalError::ParentLimit {
            actual: parents.len(),
            maximum: limits.max_parents,
        });
    }
    let payload_size = u64::from(operation.header.payload_size);
    if payload_size > limits.max_payload_bytes {
        return Err(CausalError::PayloadLimit {
            actual: payload_size,
            maximum: limits.max_payload_bytes,
        });
    }
    let unique: BTreeSet<_> = parents.iter().copied().collect();
    if unique.len() != parents.len() {
        return Err(CausalError::DuplicateParent);
    }
    if unique.contains(operation.hash.as_bytes()) {
        return Err(CausalError::SelfParent);
    }
    Ok(())
}

/// Order the causally complete subset and report operations blocked by missing
/// history.
///
/// Missing parents do not hide unrelated facts. A cycle among the complete
/// subset remains an error because there is no deterministic legal fold.
pub fn causal_projection<L: Clone + Ord>(
    entries: &[CausalEntry<L>],
) -> Result<CausalProjection, CausalError> {
    validate_sequences(entries)?;

    let mut by_hash = BTreeMap::new();
    for (index, entry) in entries.iter().enumerate() {
        if by_hash.insert(entry.operation, index).is_some() {
            return Err(CausalError::DuplicateOperation);
        }
    }

    let mut dependencies = vec![BTreeSet::new(); entries.len()];
    let mut dependents = vec![Vec::new(); entries.len()];
    let mut missing = vec![BTreeSet::new(); entries.len()];
    for (index, entry) in entries.iter().enumerate() {
        dependencies[index] = entry.dependencies();
        for dependency in dependencies[index].iter().copied() {
            if let Some(parent) = by_hash.get(&dependency).copied() {
                dependents[parent].push(index);
            } else {
                missing[index].insert(dependency);
            }
        }
    }

    // A child of a blocked operation is blocked on the same missing roots.
    let mut blocked = VecDeque::new();
    for (index, roots) in missing.iter().enumerate() {
        if !roots.is_empty() {
            blocked.push_back(index);
        }
    }
    while let Some(index) = blocked.pop_front() {
        let roots = missing[index].clone();
        for dependent in dependents[index].iter().copied() {
            let before = missing[dependent].len();
            missing[dependent].extend(roots.iter().copied());
            if missing[dependent].len() != before {
                blocked.push_back(dependent);
            }
        }
    }

    let complete: Vec<bool> = missing.iter().map(BTreeSet::is_empty).collect();
    let mut indegree = vec![0usize; entries.len()];
    for (index, deps) in dependencies.iter().enumerate() {
        if !complete[index] {
            continue;
        }
        indegree[index] = deps
            .iter()
            .filter(|dependency| {
                by_hash
                    .get(*dependency)
                    .is_some_and(|parent| complete[*parent])
            })
            .count();
    }

    let mut ready = BTreeSet::new();
    for (index, entry) in entries.iter().enumerate() {
        if complete[index] && indegree[index] == 0 {
            ready.insert(order_key(entry, index));
        }
    }

    let mut order = Vec::new();
    while let Some(key) = ready.pop_first() {
        let index = key.4;
        order.push(index);
        for dependent in dependents[index].iter().copied() {
            if !complete[dependent] {
                continue;
            }
            indegree[dependent] -= 1;
            if indegree[dependent] == 0 {
                ready.insert(order_key(&entries[dependent], dependent));
            }
        }
    }
    let complete_count = complete.iter().filter(|is_complete| **is_complete).count();
    if order.len() != complete_count {
        return Err(CausalError::Cycle);
    }

    let mut pending: Vec<_> = entries
        .iter()
        .enumerate()
        .filter(|(index, _)| !complete[*index])
        .map(|(index, entry)| {
            (
                order_key(entry, index),
                PendingCausalOperation {
                    operation: entry.operation,
                    missing: missing[index].iter().copied().collect(),
                },
            )
        })
        .collect();
    pending.sort_by(|left, right| left.0.cmp(&right.0));

    Ok(CausalProjection {
        order,
        pending: pending.into_iter().map(|(_, item)| item).collect(),
    })
}

/// Return the latest operation for every observed author/log.
pub fn observed_frontier<L: Clone + Ord>(
    entries: &[CausalEntry<L>],
) -> Result<Vec<[u8; 32]>, CausalError> {
    validate_sequences(entries)?;
    let mut heads: BTreeMap<([u8; 32], L), (u32, [u8; 32])> = BTreeMap::new();
    for entry in entries {
        let key = (entry.author, entry.log_id.clone());
        let candidate = (entry.seq_num, entry.operation);
        if heads
            .get(&key)
            .is_none_or(|current| candidate.0 > current.0)
        {
            heads.insert(key, candidate);
        }
    }
    Ok(heads
        .into_values()
        .map(|(_, operation)| operation)
        .collect())
}

/// Recover the next sequence and backlink for one author/log.
pub fn author_head<L: Clone + Ord>(
    entries: &[CausalEntry<L>],
    author: [u8; 32],
    log_id: &L,
) -> Result<(u32, Option<[u8; 32]>), CausalError> {
    validate_sequences(entries)?;
    let latest = entries
        .iter()
        .filter(|entry| entry.author == author && &entry.log_id == log_id)
        .max_by_key(|entry| entry.seq_num);
    match latest {
        Some(entry) => Ok((
            entry
                .seq_num
                .checked_add(1)
                .ok_or(CausalError::SequenceExhausted)?,
            Some(entry.operation),
        )),
        None => Ok((0, None)),
    }
}

/// An operation-hash index over a causal entry set, reusable across queries.
///
/// [`happens_before`] answers one reachability question, and building the hash
/// index is the expensive half of answering it. A caller asking the question
/// once per operation over the same entries — a projection fold, a conflict
/// scan — would otherwise rebuild the whole index per call and pay O(n^2 log n)
/// to walk a graph that never changed. Build this once and ask it many times.
pub struct CausalIndex<'a, L: Clone + Ord> {
    by_hash: BTreeMap<[u8; 32], &'a CausalEntry<L>>,
}

impl<'a, L: Clone + Ord> CausalIndex<'a, L> {
    /// Index `entries` by operation hash. O(n log n), paid once.
    pub fn new(entries: &'a [CausalEntry<L>]) -> Self {
        Self {
            by_hash: entries
                .iter()
                .map(|entry| (entry.operation, entry))
                .collect(),
        }
    }

    /// Whether `earlier` is in `later`'s transitive dependency set.
    pub fn happens_before(&self, earlier: [u8; 32], later: [u8; 32]) -> bool {
        let Some(later) = self.by_hash.get(&later) else {
            return false;
        };
        let mut seen = BTreeSet::new();
        let mut pending: Vec<_> = later.dependencies().into_iter().collect();
        while let Some(operation) = pending.pop() {
            if operation == earlier {
                return true;
            }
            if seen.insert(operation)
                && let Some(entry) = self.by_hash.get(&operation)
            {
                pending.extend(entry.dependencies());
            }
        }
        false
    }
}

/// Whether `earlier` is in `later`'s transitive dependency set.
///
/// Builds a fresh [`CausalIndex`] per call. Asking this repeatedly over one
/// entry set is quadratic; hoist a `CausalIndex` out of the loop instead.
pub fn happens_before<L: Clone + Ord>(
    entries: &[CausalEntry<L>],
    earlier: [u8; 32],
    later: [u8; 32],
) -> bool {
    CausalIndex::new(entries).happens_before(earlier, later)
}

fn order_key<L: Clone + Ord>(
    entry: &CausalEntry<L>,
    index: usize,
) -> ([u8; 32], L, u32, [u8; 32], usize) {
    (
        entry.author,
        entry.log_id.clone(),
        entry.seq_num,
        entry.operation,
        index,
    )
}

fn validate_sequences<L: Clone + Ord>(entries: &[CausalEntry<L>]) -> Result<(), CausalError> {
    let mut positions = BTreeMap::new();
    for entry in entries {
        let key = (entry.author, entry.log_id.clone(), entry.seq_num);
        if positions
            .insert(key, entry.operation)
            .is_some_and(|existing| existing != entry.operation)
        {
            return Err(CausalError::ForkedSequence {
                seq_num: entry.seq_num,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(
        operation: u8,
        author: u8,
        seq_num: u32,
        backlink: Option<u8>,
        parents: &[u8],
    ) -> CausalEntry<u64> {
        CausalEntry {
            operation: [operation; 32],
            author: [author; 32],
            log_id: 0,
            seq_num,
            backlink: backlink.map(|value| [value; 32]),
            parents: parents.iter().map(|value| [*value; 32]).collect(),
        }
    }

    #[test]
    fn a_causal_child_follows_its_parent_despite_author_key_rank() {
        let entries = vec![entry(1, 9, 0, None, &[]), entry(2, 1, 0, None, &[1])];
        assert_eq!(causal_projection(&entries).unwrap().order, vec![0, 1]);
        assert!(happens_before(&entries, [1; 32], [2; 32]));
        assert!(!happens_before(&entries, [2; 32], [1; 32]));
    }

    #[test]
    fn concurrent_records_use_stable_author_order() {
        let entries = vec![entry(1, 9, 0, None, &[]), entry(2, 1, 0, None, &[])];
        assert_eq!(causal_projection(&entries).unwrap().order, vec![1, 0]);
    }

    #[test]
    fn missing_history_blocks_only_its_descendants() {
        let entries = vec![
            entry(1, 1, 0, None, &[99]),
            entry(2, 1, 1, Some(1), &[]),
            entry(3, 3, 0, None, &[]),
        ];
        let projection = causal_projection(&entries).unwrap();
        assert_eq!(projection.order, vec![2]);
        assert_eq!(projection.pending.len(), 2);
        assert!(
            projection
                .pending
                .iter()
                .all(|pending| pending.missing == vec![[99; 32]])
        );
    }

    #[test]
    fn a_complete_cycle_fails_closed() {
        let entries = vec![entry(1, 1, 0, None, &[2]), entry(2, 2, 0, None, &[1])];
        assert_eq!(causal_projection(&entries), Err(CausalError::Cycle));
    }

    #[test]
    fn frontiers_and_author_heads_are_recovered() {
        let entries = vec![
            entry(1, 1, 0, None, &[]),
            entry(2, 1, 1, Some(1), &[]),
            entry(3, 2, 0, None, &[]),
        ];
        assert_eq!(observed_frontier(&entries).unwrap(), vec![[2; 32], [3; 32]]);
        assert_eq!(
            author_head(&entries, [1; 32], &0).unwrap(),
            (2, Some([2; 32]))
        );
    }

    #[test]
    fn an_author_log_fork_is_rejected() {
        let entries = vec![entry(1, 1, 0, None, &[]), entry(2, 1, 0, None, &[])];
        assert_eq!(
            causal_projection(&entries),
            Err(CausalError::ForkedSequence { seq_num: 0 })
        );
    }
}
