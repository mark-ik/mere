/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Trail domain vocabulary — the browse-trail projection behind the Trail pane.
//!
//! Extracted from meerkat's `pane_data` per the pane-placement decision
//! (boundary pass plan, 2026-07-10): trail content is graph + persistence
//! truth (recent visits, the focused node's own url history, the eidetic
//! deleted-nodes log), so its vocabulary lives mere-side on the P8 pattern —
//! explicit inputs in, neutral items out, the host maps items to its pane
//! rows and owns the store/graph reads that gather the inputs.

/// One removed-node record (from the eidetic deleted-nodes log): enough for a
/// host to render a recovery affordance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrailRemoved {
    /// The removed node's stable id, as a string (the recovery key).
    pub node_id: String,
    pub url: String,
}

/// The gathered inputs for one Trail projection. The host reads its graph and
/// store to fill this; the builder never touches live state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrailInput {
    /// Graph-wide recently-visited urls, newest first.
    pub recent_urls: Vec<String>,
    /// The focused node's own url history (shown only when it has gone
    /// somewhere, i.e. more than one entry).
    pub history_urls: Vec<String>,
    /// The deleted-nodes log, newest first.
    pub removed: Vec<TrailRemoved>,
}

/// One projected Trail item. Neutral: the host maps these onto its pane-row
/// shapes (titles, inert rows, a recovery button).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrailItem {
    /// A titled section header ("Recent" / "This node" / "Removed").
    SectionTitle(&'static str),
    /// An inert row (a shortened url).
    Row(String),
    /// A muted empty-state hint.
    MutedRow(&'static str),
    /// A removed node's recovery row: shortened url text plus the node id the
    /// host keys its recover action by.
    Recover { text: String, node_id: String },
}

/// Shorten a url for row display: strip the web scheme and cap the length.
pub fn short_url(url: &str) -> String {
    url.strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url)
        .chars()
        .take(56)
        .collect()
}

/// Project a [`TrailInput`] into the Trail pane's item list: the graph-wide
/// recently-visited urls, the focused node's own url history, and the
/// removed-nodes log — three titled sections. Pure.
pub fn build_trail_items(input: &TrailInput) -> Vec<TrailItem> {
    let mut items = vec![TrailItem::SectionTitle("Recent")];
    if input.recent_urls.is_empty() {
        items.push(TrailItem::MutedRow("nothing visited yet"));
    } else {
        for url in &input.recent_urls {
            items.push(TrailItem::Row(short_url(url)));
        }
    }

    if input.history_urls.len() > 1 {
        items.push(TrailItem::SectionTitle("This node"));
        for url in &input.history_urls {
            items.push(TrailItem::Row(short_url(url)));
        }
    }

    if !input.removed.is_empty() {
        items.push(TrailItem::SectionTitle("Removed"));
        for tomb in &input.removed {
            items.push(TrailItem::Recover {
                text: short_url(&tomb.url),
                node_id: tomb.node_id.clone(),
            });
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_projects_the_recent_hint_only() {
        let items = build_trail_items(&TrailInput::default());
        assert_eq!(
            items,
            vec![
                TrailItem::SectionTitle("Recent"),
                TrailItem::MutedRow("nothing visited yet"),
            ]
        );
    }

    #[test]
    fn sections_appear_only_when_populated() {
        let items = build_trail_items(&TrailInput {
            recent_urls: vec!["https://example.com/a".into()],
            // A single-entry history is a node that has gone nowhere: no section.
            history_urls: vec!["https://example.com/a".into()],
            removed: Vec::new(),
        });
        assert!(items.contains(&TrailItem::SectionTitle("Recent")));
        assert!(!items.contains(&TrailItem::SectionTitle("This node")));
        assert!(!items.contains(&TrailItem::SectionTitle("Removed")));
    }

    #[test]
    fn removed_rows_carry_the_recovery_node_id() {
        let items = build_trail_items(&TrailInput {
            recent_urls: vec!["https://example.com/a".into()],
            history_urls: vec![
                "https://example.com/a".into(),
                "https://example.com/b".into(),
            ],
            removed: vec![TrailRemoved {
                node_id: "123".into(),
                url: "https://example.com/z".into(),
            }],
        });
        assert!(items.contains(&TrailItem::SectionTitle("This node")));
        assert!(items.iter().any(|item| matches!(
            item,
            TrailItem::Recover { node_id, text } if node_id == "123" && text == "example.com/z"
        )));
    }

    #[test]
    fn short_url_strips_web_schemes_and_caps_length() {
        assert_eq!(short_url("https://example.com/x"), "example.com/x");
        assert_eq!(short_url("gemini://long.example"), "gemini://long.example");
        assert_eq!(short_url(&format!("https://{}", "a".repeat(99))).len(), 56);
    }
}
