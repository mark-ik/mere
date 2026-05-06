/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Verso URI scheme parsing — `verso://`, `graphshell://`, `graph://`,
//! `node://`, `notes://`. Includes the address types that the rest of
//! Graphshell uses to address durable subsystem surfaces (settings
//! pages, frame-by-id, tile-group-by-id, view targets, tool panes,
//! clip captures, plus generic graph/node/note addresses).
//!
//! Promoted from `graphshell::util` per Slice 56 — these types were
//! pure URI-string parsing (no egui, no euclid) but lived next to the
//! `CoordBridge` egui-glue trait in `util.rs`, which forced every
//! portable consumer (the viewer registry, several services, action
//! routing, settings persistence) to reach into the binary root for a
//! primitive. The egui-flavored `CoordBridge` trait stayed behind in
//! `util.rs`.

use std::fmt;

pub const VERSO_SCHEME_PREFIX: &str = "verso://";
pub const GRAPHSHELL_SCHEME_PREFIX: &str = "graphshell://";
pub const GRAPH_SCHEME_PREFIX: &str = "graph://";
pub const NODE_SCHEME_PREFIX: &str = "node://";
pub const NOTES_SCHEME_PREFIX: &str = "notes://";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersoAddress {
    Settings(GraphshellSettingsPath),
    Frame(String),
    TileGroup(String),
    View(VersoViewTarget),
    Tool {
        name: String,
        instance: Option<u32>,
    },
    Clip(String),
    Other {
        category: String,
        segments: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersoViewTarget {
    Legacy(String),
    Graph(String),
    Node(String),
    Note(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphshellSettingsPath {
    General,
    Persistence,
    Physics,
    Sync,
    Appearance,
    Keybindings,
    Advanced,
    History,
    Other(String),
}

impl VersoAddress {
    pub fn parse(input: &str) -> Option<Self> {
        let normalized = normalize_internal_uri(input)?;
        let tail = normalized
            .strip_prefix(VERSO_SCHEME_PREFIX)
            .or_else(|| normalized.strip_prefix(GRAPHSHELL_SCHEME_PREFIX))?;
        let mut segments = tail
            .split('/')
            .filter(|segment| !segment.is_empty())
            .map(str::to_string);
        let category = segments.next()?;

        match category.as_str() {
            "settings" => {
                let settings_path = match segments.next() {
                    None => GraphshellSettingsPath::General,
                    Some(segment) => GraphshellSettingsPath::from_segment(&segment),
                };
                Some(Self::Settings(settings_path))
            }
            "frame" => Some(Self::Frame(segments.next()?)),
            "tile-group" => Some(Self::TileGroup(segments.next()?)),
            "view" => {
                let first = segments.next()?;
                match segments.next() {
                    None => Some(Self::View(VersoViewTarget::Legacy(first))),
                    Some(target_id) => {
                        let view_target = match first.as_str() {
                            "graph" => VersoViewTarget::Graph(target_id),
                            "node" => VersoViewTarget::Node(target_id),
                            "note" => VersoViewTarget::Note(target_id),
                            _ => {
                                let mut preserved_segments = vec![first, target_id];
                                preserved_segments.extend(segments);
                                return Some(Self::Other {
                                    category,
                                    segments: preserved_segments,
                                });
                            }
                        };
                        Some(Self::View(view_target))
                    }
                }
            }
            "tool" => {
                let name = segments.next()?;
                let instance = segments
                    .next()
                    .and_then(|segment| segment.parse::<u32>().ok());
                Some(Self::Tool { name, instance })
            }
            "clip" => Some(Self::Clip(segments.next()?)),
            _ => Some(Self::Other {
                category,
                segments: segments.collect(),
            }),
        }
    }

    pub fn is_settings(&self) -> bool {
        matches!(self, Self::Settings(_))
    }

    pub fn settings(path: GraphshellSettingsPath) -> Self {
        Self::Settings(path)
    }

    pub fn frame(frame_id: impl Into<String>) -> Self {
        Self::Frame(frame_id.into())
    }

    pub fn tile_group(group_id: impl Into<String>) -> Self {
        Self::TileGroup(group_id.into())
    }

    pub fn view(view_id: impl Into<String>) -> Self {
        Self::View(VersoViewTarget::Legacy(view_id.into()))
    }

    pub fn view_graph(graph_id: impl Into<String>) -> Self {
        Self::View(VersoViewTarget::Graph(graph_id.into()))
    }

    pub fn view_node(node_id: impl Into<String>) -> Self {
        Self::View(VersoViewTarget::Node(node_id.into()))
    }

    pub fn view_note(note_id: impl Into<String>) -> Self {
        Self::View(VersoViewTarget::Note(note_id.into()))
    }

    pub fn tool(name: impl Into<String>, instance: Option<u32>) -> Self {
        Self::Tool {
            name: name.into(),
            instance,
        }
    }

    pub fn clip(clip_id: impl Into<String>) -> Self {
        Self::Clip(clip_id.into())
    }

    pub fn inferred_mime_hint(&self) -> &'static str {
        match self {
            Self::Settings(_) => "application/x-graphshell-settings",
            Self::Frame(_)
            | Self::TileGroup(_)
            | Self::View(_)
            | Self::Tool { .. }
            | Self::Clip(_)
            | Self::Other { .. } => "application/x-graphshell-internal",
        }
    }
}

impl GraphshellSettingsPath {
    fn from_segment(segment: &str) -> Self {
        match segment {
            "general" => Self::General,
            "persistence" => Self::Persistence,
            "physics" => Self::Physics,
            "sync" => Self::Sync,
            "appearance" => Self::Appearance,
            "keybindings" => Self::Keybindings,
            "advanced" => Self::Advanced,
            "history" => Self::History,
            other => Self::Other(other.to_string()),
        }
    }

    fn as_segment(&self) -> Option<&str> {
        match self {
            Self::General => None,
            Self::Persistence => Some("persistence"),
            Self::Physics => Some("physics"),
            Self::Sync => Some("sync"),
            Self::Appearance => Some("appearance"),
            Self::Keybindings => Some("keybindings"),
            Self::Advanced => Some("advanced"),
            Self::History => Some("history"),
            Self::Other(other) => Some(other.as_str()),
        }
    }
}

impl fmt::Display for VersoAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Settings(path) => match path.as_segment() {
                Some(segment) => write!(f, "{VERSO_SCHEME_PREFIX}settings/{segment}"),
                None => write!(f, "{VERSO_SCHEME_PREFIX}settings"),
            },
            Self::Frame(frame_id) => write!(f, "{VERSO_SCHEME_PREFIX}frame/{frame_id}"),
            Self::TileGroup(group_id) => {
                write!(f, "{VERSO_SCHEME_PREFIX}tile-group/{group_id}")
            }
            Self::View(VersoViewTarget::Legacy(view_id)) => {
                write!(f, "{VERSO_SCHEME_PREFIX}view/{view_id}")
            }
            Self::View(VersoViewTarget::Graph(graph_id)) => {
                write!(f, "{VERSO_SCHEME_PREFIX}view/graph/{graph_id}")
            }
            Self::View(VersoViewTarget::Node(node_id)) => {
                write!(f, "{VERSO_SCHEME_PREFIX}view/node/{node_id}")
            }
            Self::View(VersoViewTarget::Note(note_id)) => {
                write!(f, "{VERSO_SCHEME_PREFIX}view/note/{note_id}")
            }
            Self::Tool { name, instance } => match instance {
                Some(instance) => write!(f, "{VERSO_SCHEME_PREFIX}tool/{name}/{instance}"),
                None => write!(f, "{VERSO_SCHEME_PREFIX}tool/{name}"),
            },
            Self::Clip(clip_id) => write!(f, "{VERSO_SCHEME_PREFIX}clip/{clip_id}"),
            Self::Other { category, segments } => {
                write!(f, "{VERSO_SCHEME_PREFIX}{category}")?;
                for segment in segments {
                    write!(f, "/{segment}")?;
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphAddress {
    pub graph_id: String,
}

impl GraphAddress {
    pub fn parse(input: &str) -> Option<Self> {
        let normalized = normalize_scheme_uri(input, GRAPH_SCHEME_PREFIX)?;
        let graph_id = normalized.strip_prefix(GRAPH_SCHEME_PREFIX)?;
        (!graph_id.is_empty()).then_some(Self {
            graph_id: graph_id.to_string(),
        })
    }

    pub fn graph(graph_id: impl Into<String>) -> Self {
        Self {
            graph_id: graph_id.into(),
        }
    }
}

impl fmt::Display for GraphAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{GRAPH_SCHEME_PREFIX}{}", self.graph_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeAddress {
    pub node_id: String,
}

impl NodeAddress {
    pub fn parse(input: &str) -> Option<Self> {
        let normalized = normalize_scheme_uri(input, NODE_SCHEME_PREFIX)?;
        let node_id = normalized.strip_prefix(NODE_SCHEME_PREFIX)?;
        (!node_id.is_empty()).then_some(Self {
            node_id: node_id.to_string(),
        })
    }

    pub fn node(node_id: impl Into<String>) -> Self {
        Self {
            node_id: node_id.into(),
        }
    }
}

impl fmt::Display for NodeAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{NODE_SCHEME_PREFIX}{}", self.node_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteAddress {
    pub note_id: String,
}

impl NoteAddress {
    pub fn parse(input: &str) -> Option<Self> {
        let normalized = normalize_scheme_uri(input, NOTES_SCHEME_PREFIX)?;
        let note_id = normalized.strip_prefix(NOTES_SCHEME_PREFIX)?;
        (!note_id.is_empty()).then_some(Self {
            note_id: note_id.to_string(),
        })
    }

    pub fn note(note_id: impl Into<String>) -> Self {
        Self {
            note_id: note_id.into(),
        }
    }
}

impl fmt::Display for NoteAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{NOTES_SCHEME_PREFIX}{}", self.note_id)
    }
}

fn normalize_internal_uri(input: &str) -> Option<String> {
    normalize_scheme_uri(input, VERSO_SCHEME_PREFIX)
        .or_else(|| normalize_scheme_uri(input, GRAPHSHELL_SCHEME_PREFIX))
}

fn normalize_scheme_uri(input: &str, scheme_prefix: &str) -> Option<String> {
    let trimmed = input.trim();
    let no_fragment = trimmed.split('#').next().unwrap_or(trimmed);
    let no_query = no_fragment.split('?').next().unwrap_or(no_fragment);
    let normalized = no_query.to_ascii_lowercase();
    normalized.starts_with(scheme_prefix).then_some(normalized)
}

/// Truncate a string to `max_chars` characters, appending an ellipsis if truncated.
/// Uses character counting (not byte length) so it's safe for multi-byte UTF-8.
pub fn truncate_with_ellipsis(input: &str, max_chars: usize) -> String {
    if input.chars().count() > max_chars {
        let truncated: String = input.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{truncated}\u{2026}")
    } else {
        input.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_graphshell_settings_history_route() {
        let parsed = VersoAddress::parse(" graphshell://settings/history ");
        assert_eq!(
            parsed,
            Some(VersoAddress::settings(GraphshellSettingsPath::History))
        );
    }

    #[test]
    fn parse_verso_settings_history_route() {
        let parsed = VersoAddress::parse(" verso://settings/history ");
        assert_eq!(
            parsed,
            Some(VersoAddress::settings(GraphshellSettingsPath::History))
        );
    }

    #[test]
    fn parse_graphshell_settings_root_as_general() {
        let parsed = VersoAddress::parse("graphshell://settings");
        assert_eq!(
            parsed,
            Some(VersoAddress::settings(GraphshellSettingsPath::General))
        );
    }

    #[test]
    fn parse_graphshell_unknown_route_preserves_segments() {
        let parsed = VersoAddress::parse("graphshell://frame/abc123");
        assert_eq!(parsed, Some(VersoAddress::frame("abc123")));
    }

    #[test]
    fn parse_graphshell_tile_group_route() {
        let parsed = VersoAddress::parse("graphshell://tile-group/group-123");
        assert_eq!(parsed, Some(VersoAddress::tile_group("group-123")));
    }

    #[test]
    fn parse_verso_view_note_route() {
        let parsed = VersoAddress::parse("verso://view/note/550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(
            parsed,
            Some(VersoAddress::view_note(
                "550e8400-e29b-41d4-a716-446655440000"
            ))
        );
    }

    #[test]
    fn parse_graphshell_strips_query_and_fragment() {
        let parsed = VersoAddress::parse("graphshell://settings/physics?tab=1#focus");
        assert_eq!(
            parsed,
            Some(VersoAddress::settings(GraphshellSettingsPath::Physics))
        );
    }

    #[test]
    fn graphshell_address_display_roundtrips_settings_route() {
        let address = VersoAddress::settings(GraphshellSettingsPath::Appearance);
        assert_eq!(address.to_string(), "verso://settings/appearance");
    }

    #[test]
    fn parse_graphshell_settings_advanced_route() {
        let parsed = VersoAddress::parse("graphshell://settings/advanced");
        assert_eq!(
            parsed,
            Some(VersoAddress::settings(GraphshellSettingsPath::Advanced))
        );
    }

    #[test]
    fn graphshell_address_display_roundtrips_other_route() {
        let address = VersoAddress::frame("abc123");
        assert_eq!(address.to_string(), "verso://frame/abc123");
    }

    #[test]
    fn graphshell_address_display_roundtrips_tile_group_route() {
        let address = VersoAddress::tile_group("group-123");
        assert_eq!(address.to_string(), "verso://tile-group/group-123");
    }

    #[test]
    fn parse_graphshell_tool_route_with_instance() {
        let parsed = VersoAddress::parse("graphshell://tool/history/2");
        assert_eq!(parsed, Some(VersoAddress::tool("history", Some(2))));
    }

    #[test]
    fn graphshell_address_display_roundtrips_view_route() {
        let address = VersoAddress::view("view-123");
        assert_eq!(address.to_string(), "verso://view/view-123");
    }

    #[test]
    fn graphshell_address_display_roundtrips_note_view_route() {
        let address = VersoAddress::view_note("550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(
            address.to_string(),
            "verso://view/note/550e8400-e29b-41d4-a716-446655440000"
        );
    }

    #[test]
    fn graphshell_address_display_roundtrips_tool_route_without_instance() {
        let address = VersoAddress::tool("history", None);
        assert_eq!(address.to_string(), "verso://tool/history");
    }

    #[test]
    fn graphshell_address_display_roundtrips_clip_route() {
        let address = VersoAddress::clip("clip-123");
        assert_eq!(address.to_string(), "verso://clip/clip-123");
    }

    #[test]
    fn parse_note_address_strips_query_and_fragment() {
        let parsed =
            NoteAddress::parse(" notes://550e8400-e29b-41d4-a716-446655440000?mode=edit#top ");
        assert_eq!(
            parsed,
            Some(NoteAddress::note("550e8400-e29b-41d4-a716-446655440000"))
        );
    }

    #[test]
    fn note_address_display_roundtrips() {
        let address = NoteAddress::note("550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(
            address.to_string(),
            "notes://550e8400-e29b-41d4-a716-446655440000"
        );
    }

    #[test]
    fn graph_address_display_roundtrips() {
        let address = GraphAddress::graph("550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(
            address.to_string(),
            "graph://550e8400-e29b-41d4-a716-446655440000"
        );
    }

    #[test]
    fn node_address_display_roundtrips() {
        let address = NodeAddress::node("550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(
            address.to_string(),
            "node://550e8400-e29b-41d4-a716-446655440000"
        );
    }

    #[test]
    fn test_short_string_unchanged() {
        assert_eq!(truncate_with_ellipsis("short", 20), "short");
    }

    #[test]
    fn test_long_string_truncated() {
        let result =
            truncate_with_ellipsis("this is a very long title that should be truncated", 20);
        assert_eq!(result.chars().count(), 20);
        assert!(result.ends_with('\u{2026}'));
    }

    #[test]
    fn test_exact_length_unchanged() {
        assert_eq!(
            truncate_with_ellipsis("exactly twenty chars", 20),
            "exactly twenty chars"
        );
    }

    #[test]
    fn test_emoji_safe() {
        // Emoji are multi-byte but single chars — should not panic
        let input = "Hello \u{1F600} World! This is long enough to truncate";
        let result = truncate_with_ellipsis(input, 15);
        assert!(result.chars().count() <= 15);
        assert!(result.ends_with('\u{2026}'));
    }

    #[test]
    fn test_cjk_safe() {
        // CJK characters are 3 bytes each — byte slicing would panic
        let input = "\u{4F60}\u{597D}\u{4E16}\u{754C}\u{4F60}\u{597D}\u{4E16}\u{754C}"; // 8 chars
        let result = truncate_with_ellipsis(input, 5);
        assert_eq!(result.chars().count(), 5);
        assert!(result.ends_with('\u{2026}'));
    }

    #[test]
    fn test_empty_string() {
        assert_eq!(truncate_with_ellipsis("", 20), "");
    }

    #[test]
    fn test_max_one() {
        // Edge case: max_chars = 1, saturating_sub(1) = 0, so just ellipsis
        let result = truncate_with_ellipsis("hello", 1);
        assert_eq!(result, "\u{2026}");
    }
}
