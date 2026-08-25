//! Knot's three-way merge: source-preserving merge at Djot block boundaries,
//! the line merge it falls back to, and the entry point the sync projection
//! calls to settle a two-writer document without asking a person.
//!
//! Jotdown supplies byte spans for the authored source. We use those spans to
//! divide a document into stable, section-local blocks and splice exact source
//! slices from each branch. Unchanged Djot spelling and whitespace are never
//! rendered back from an AST.
//!
//! Only [`automatic_text_merge`] reads operation history. Everything below it
//! merges plain text and stays clear of p2panda and stickleback, so the merge
//! rules can be exercised without a store.

use std::collections::BTreeMap;

use jotdown::{AttributeKind, Attributes, Container, Event, Parser};
use p2panda_core::cbor::encode_cbor;
use similar::{Algorithm, TextDiff};
use stickleback::CausalIndex;

use crate::{KnotAutomaticTextMerge, KnotDocumentVersion, KnotSyncError, VaultDocument};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct BlockKey {
    section: Vec<String>,
    identity: String,
}

#[derive(Clone, Debug)]
struct Block<'a> {
    key: BlockKey,
    source: &'a str,
}

#[derive(Debug)]
struct PendingBlock {
    key: BlockKey,
    depth: usize,
    start: usize,
    end: usize,
}

/// Merge two Djot branches while preserving the exact source spelling chosen
/// for every structural block.
///
/// The first cut deliberately requires the same block identities and order in
/// all three versions. Insertions, deletions, moves, and edited headings fall
/// back to Knot's existing line merge or an explicit conflict. That is safer
/// than guessing identity when Djot has not supplied one.
pub(crate) fn merge_djot_sources(base: &str, left: &str, right: &str) -> Option<String> {
    if left == right {
        return Some(left.to_owned());
    }
    if left == base {
        return Some(right.to_owned());
    }
    if right == base {
        return Some(left.to_owned());
    }

    let base = blocks(base)?;
    let left = blocks(left)?;
    let right = blocks(right)?;
    if base.len() < 2
        || base
            .iter()
            .map(|block| &block.key)
            .ne(left.iter().map(|block| &block.key))
        || base
            .iter()
            .map(|block| &block.key)
            .ne(right.iter().map(|block| &block.key))
    {
        return None;
    }

    let mut output = String::new();
    for ((base, left), right) in base.iter().zip(&left).zip(&right) {
        let source = if left.source == right.source {
            left.source.to_owned()
        } else if left.source == base.source {
            right.source.to_owned()
        } else if right.source == base.source {
            left.source.to_owned()
        } else {
            merge_text_lines(base.source, left.source, right.source)?
        };
        output.push_str(&source);
    }
    Some(output)
}

fn blocks(source: &str) -> Option<Vec<Block<'_>>> {
    let mut sections = Vec::new();
    let mut stack = Vec::new();
    let mut ordinals: BTreeMap<(Vec<String>, String), usize> = BTreeMap::new();
    let mut pending: Option<PendingBlock> = None;
    let mut raw = Vec::new();

    for (event, range) in Parser::new(source).into_offset_iter() {
        if let Some(block) = &mut pending {
            block.start = block.start.min(range.start);
            block.end = block.end.max(range.end);
        }

        match event {
            Event::Start(container, attributes) => {
                let parent_is_document_or_section = stack
                    .last()
                    .is_some_and(|parent| is_document_or_section(parent));
                if let Container::Section { id } = &container {
                    sections.push(id.to_string());
                } else if parent_is_document_or_section && is_merge_block(&container) {
                    if pending.is_some() {
                        return None;
                    }
                    let kind = container_kind(&container);
                    let explicit = explicit_id(&attributes);
                    let ordinal = ordinals
                        .entry((sections.clone(), kind.clone()))
                        .and_modify(|ordinal| *ordinal += 1)
                        .or_insert(0);
                    let identity = explicit
                        .map(|id| format!("id:{id}"))
                        .unwrap_or_else(|| format!("{kind}:{ordinal}"));
                    pending = Some(PendingBlock {
                        key: BlockKey {
                            section: sections.clone(),
                            identity,
                        },
                        depth: stack.len() + 1,
                        start: range.start,
                        end: range.end,
                    });
                }
                stack.push(container);
            }
            Event::End(container) => {
                if pending
                    .as_ref()
                    .is_some_and(|block| block.depth == stack.len())
                {
                    raw.push(pending.take()?);
                }
                let opened = stack.pop()?;
                if opened != container {
                    return None;
                }
                if matches!(container, Container::Section { .. }) {
                    sections.pop()?;
                }
            }
            _ => {}
        }
    }
    if pending.is_some() || !stack.is_empty() || raw.is_empty() {
        return None;
    }

    // Make the block spans a complete partition. Whitespace and unattached
    // attributes between parser events stay attached to the preceding block.
    raw[0].start = 0;
    for index in 0..raw.len().saturating_sub(1) {
        raw[index].end = raw[index + 1].start;
    }
    raw.last_mut()?.end = source.len();
    if raw
        .windows(2)
        .any(|pair| pair[0].start > pair[0].end || pair[0].end > pair[1].start)
        || raw.last().is_some_and(|block| block.start > block.end)
    {
        return None;
    }

    Some(
        raw.into_iter()
            .map(|block| Block {
                key: block.key,
                source: &source[block.start..block.end],
            })
            .collect(),
    )
}

fn explicit_id(attributes: &Attributes<'_>) -> Option<String> {
    attributes
        .iter()
        .find_map(|(kind, value)| matches!(kind, AttributeKind::Id).then(|| value.to_string()))
}

fn is_document_or_section(container: &Container<'_>) -> bool {
    matches!(container, Container::Document | Container::Section { .. })
}

fn is_merge_block(container: &Container<'_>) -> bool {
    !matches!(container, Container::Document | Container::Section { .. })
}

fn container_kind(container: &Container<'_>) -> String {
    let debug = format!("{container:?}");
    debug
        .split([' ', '{', '('])
        .next()
        .unwrap_or(&debug)
        .to_owned()
}

/// Settle a two-writer document from its own causal history, or decline.
///
/// This is the only place in the merge subsystem that reads operation history:
/// the common ancestor has to be an operation for this document that both
/// current versions descend from, and `causal` answers that reachability
/// question. The caller owns the index so the walk below is not paying to
/// rebuild it per history entry.
pub(crate) fn automatic_text_merge(
    causal: &CausalIndex<'_, u64>,
    history: &[([u8; 32], String, Option<VaultDocument>)],
    id: &str,
    versions: &BTreeMap<[u8; 32], KnotDocumentVersion>,
) -> Option<KnotAutomaticTextMerge> {
    if versions.len() != 2 {
        return None;
    }
    let versions: Vec<_> = versions.values().collect();
    let left = versions[0].document.as_ref()?;
    let right = versions[1].document.as_ref()?;
    let (base_operation, base) =
        history
            .iter()
            .rev()
            .find_map(|(operation, event_id, document)| {
                let document = document.as_ref()?;
                (event_id == id
                    && causal.happens_before(*operation, versions[0].operation)
                    && causal.happens_before(*operation, versions[1].operation))
                .then_some((*operation, document))
            })?;
    let document = merge_text_document(base, left, right)?;
    let mut supersedes = vec![versions[0].operation, versions[1].operation];
    supersedes.sort_unstable();
    Some(KnotAutomaticTextMerge {
        id: id.into(),
        base: base_operation,
        supersedes,
        document,
    })
}

fn merge_text_document(
    base: &VaultDocument,
    left: &VaultDocument,
    right: &VaultDocument,
) -> Option<VaultDocument> {
    if base.id != left.id || base.id != right.id {
        return None;
    }
    let title = merge_scalar(&base.title, &left.title, &right.title)?;
    let media_type = merge_scalar(&base.media_type, &left.media_type, &right.media_type)?;
    if !media_type.starts_with("text/") {
        return None;
    }
    let base_body = std::str::from_utf8(&base.body).ok()?;
    let left_body = std::str::from_utf8(&left.body).ok()?;
    let right_body = std::str::from_utf8(&right.body).ok()?;
    let body = if matches!(media_type.as_str(), "text/djot" | "text/vnd.knot") {
        merge_djot_sources(base_body, left_body, right_body)
            .or_else(|| merge_text_lines(base_body, left_body, right_body))?
    } else {
        merge_text_lines(base_body, left_body, right_body)?
    }
    .into_bytes();
    Some(VaultDocument {
        id: base.id.clone(),
        title,
        body,
        media_type,
    })
}

fn merge_scalar<T: Clone + Eq>(base: &T, left: &T, right: &T) -> Option<T> {
    if left == right {
        Some(left.clone())
    } else if left == base {
        Some(right.clone())
    } else if right == base {
        Some(left.clone())
    } else {
        None
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LineEdit {
    start: usize,
    end: usize,
    replacement: Vec<String>,
}

fn line_edits(base: &[&str], branch: &[&str]) -> Vec<LineEdit> {
    TextDiff::configure()
        .algorithm(Algorithm::Myers)
        .diff_slices(base, branch)
        .ops()
        .iter()
        .filter_map(|operation| {
            let old = operation.old_range();
            let new = operation.new_range();
            (old.len() != new.len() || base[old.clone()] != branch[new.clone()]).then(|| LineEdit {
                start: old.start,
                end: old.end,
                replacement: branch[new].iter().map(|line| (*line).to_owned()).collect(),
            })
        })
        .collect()
}

fn merge_text_lines(base: &str, left: &str, right: &str) -> Option<String> {
    if left == right {
        return Some(left.into());
    }
    if left == base {
        return Some(right.into());
    }
    if right == base {
        return Some(left.into());
    }
    let base: Vec<_> = base.split_inclusive('\n').collect();
    let left: Vec<_> = left.split_inclusive('\n').collect();
    let right: Vec<_> = right.split_inclusive('\n').collect();
    let left_edits = line_edits(&base, &left);
    let right_edits = line_edits(&base, &right);
    for left in &left_edits {
        for right in &right_edits {
            if line_edits_conflict(left, right) {
                return None;
            }
        }
    }
    let mut edits = left_edits;
    edits.extend(right_edits);
    edits.sort_by(|left, right| {
        (left.start, left.end, &left.replacement).cmp(&(right.start, right.end, &right.replacement))
    });
    edits.dedup();

    let mut output = String::new();
    let mut cursor = 0;
    for edit in edits {
        if edit.start < cursor {
            return None;
        }
        output.extend(base[cursor..edit.start].iter().copied());
        output.extend(edit.replacement.iter().map(String::as_str));
        cursor = edit.end;
    }
    output.extend(base[cursor..].iter().copied());
    Some(output)
}

fn line_edits_conflict(left: &LineEdit, right: &LineEdit) -> bool {
    if left == right {
        return false;
    }
    let left_insert = left.start == left.end;
    let right_insert = right.start == right.end;
    match (left_insert, right_insert) {
        (true, true) => left.start == right.start,
        (true, false) => left.start > right.start && left.start < right.end,
        (false, true) => right.start > left.start && right.start < left.end,
        (false, false) => left.start.max(right.start) < left.end.min(right.end),
    }
}

pub(crate) fn automatic_text_merge_head(
    base: [u8; 32],
    supersedes: &[[u8; 32]],
    document: &VaultDocument,
) -> Result<[u8; 32], KnotSyncError> {
    let bytes = encode_cbor(&(base, supersedes, document))
        .map_err(|error| KnotSyncError::Payload(error.to_string()))?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"mere.knot.automatic-text-merge.v1");
    hasher.update(&bytes);
    Ok(*hasher.finalize().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_edits_to_adjacent_paragraphs_in_one_section() {
        let base = "# Field\n\nFirst paragraph.\n\nSecond paragraph.\n";
        let left = "# Field\n\nFirst paragraph, revised left.\n\nSecond paragraph.\n";
        let right = "# Field\n\nFirst paragraph.\n\nSecond paragraph, revised right.\n";
        assert_eq!(
            merge_djot_sources(base, left, right).as_deref(),
            Some("# Field\n\nFirst paragraph, revised left.\n\nSecond paragraph, revised right.\n")
        );
    }

    #[test]
    fn preserves_djot_attributes_and_authored_spacing() {
        let base = "# Field\n\n{#one}\nFirst.\n\n{#two}\nSecond.\n";
        let left = "# Field\n\n{#one}\n*First*, left.\n\n{#two}\nSecond.\n";
        let right = "# Field\n\n{#one}\nFirst.\n\n{#two}\nSecond,  right.\n";
        assert_eq!(
            merge_djot_sources(base, left, right).as_deref(),
            Some("# Field\n\n{#one}\n*First*, left.\n\n{#two}\nSecond,  right.\n")
        );
    }

    #[test]
    fn refuses_ambiguous_structure_changes() {
        let base = "# Field\n\nFirst.\n\nSecond.\n";
        let left = "# Field\n\nInserted.\n\nFirst.\n\nSecond.\n";
        let right = "# Field\n\nFirst.\n\nSecond revised.\n";
        assert_eq!(merge_djot_sources(base, left, right), None);
    }
}
