//! Source-preserving three-way merge at Djot block boundaries.
//!
//! Jotdown supplies byte spans for the authored source. We use those spans to
//! divide a document into stable, section-local blocks and splice exact source
//! slices from each branch. Unchanged Djot spelling and whitespace are never
//! rendered back from an AST.

use std::collections::BTreeMap;

use jotdown::{AttributeKind, Attributes, Container, Event, Parser};

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
            super::sync::merge_text_lines(base.source, left.source, right.source)?
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
