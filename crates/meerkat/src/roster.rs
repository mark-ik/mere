/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The roster pane: the graph's manifest as a node list with per-node facets
//! and edge detail for the focused node.
//!
//! R1: title / URL / content-type chip / tag chips per row.
//! R2: edge rows (kind + direction + other node title) for the focused row.
//! R3: sort/filter by content type; content-type shapes in the orrery.

use forme::GraphMemberId;
use layout_dom_api::{LayoutDom, LayoutDomMut, LocalName, Namespace, QualName};
use register_theme::chrome::{ChromeTheme, Color32};
use serval_scripted_dom::ScriptedDom;

/// Directionality of a relation from the perspective of the focused node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeDir {
    Out,
    In,
}

/// One relation row in the focused node's edge detail.
pub struct EdgeRow {
    pub direction: EdgeDir,
    /// Short display label for the relation kind (e.g. "Hyperlink", "Traversal").
    pub kind_label: String,
    /// Other-end node's display title (node.title → cached_host → URL fallback).
    pub other_title: String,
    pub other_url: String,
    pub other_member: GraphMemberId,
}

/// One roster row: node data for display in the roster pane.
pub struct RosterRow {
    pub member: GraphMemberId,
    /// Display title: `node.title` if non-empty, else cached hostname, else URL.
    pub title: String,
    /// Primary URL, shown beneath the title.
    pub url: String,
    /// Resolved content type (from fetch state) or MIME hint from node metadata.
    pub content_type: Option<String>,
    /// Semantic tags, sorted. Empty = not rendered.
    pub tags: Vec<String>,
    /// Relations to/from this node. Populated only for the focused row.
    pub edges: Vec<EdgeRow>,
    pub selected: bool,
    /// When `Some`, this row is the first in a new content-type section; render
    /// the header label before the row itself.
    pub section_header: Option<String>,
}

/// The roster's author CSS, themed from the chrome tokens.
pub fn roster_sheet(c: &ChromeTheme) -> Vec<String> {
    let rgb = |color: Color32| {
        let [r, g, b, _] = color.to_array();
        format!("rgb({r}, {g}, {b})")
    };
    vec![
        "div { display: block; }".to_string(),
        "span { display: inline-block; }".to_string(),
        format!(
            ".roster {{ overflow: scroll; height: 100%; background-color: {}; padding: 6px; }}",
            rgb(c.panel_bg)
        ),
        format!(
            ".roster-row {{ background-color: {}; padding: 8px 10px; margin: 2px 0; }}",
            rgb(c.surface_bg)
        ),
        format!(
            ".roster-row-selected {{ background-color: {}; padding: 8px 10px; margin: 2px 0; }}",
            rgb(c.active_bg)
        ),
        format!(".roster-title {{ font-size: 15px; color: {}; }}", rgb(c.strong_text)),
        format!(
            ".roster-sub {{ font-size: 12px; color: {}; margin-top: 2px; }}",
            rgb(c.muted_text)
        ),
        ".roster-facets { display: flex; flex-wrap: wrap; gap: 3px; margin-top: 5px; }".to_string(),
        format!(
            ".roster-chip {{ font-size: 11px; color: {}; background-color: {}; padding: 1px 6px; border-radius: 3px; }}",
            rgb(c.muted_text),
            rgb(c.control_bg)
        ),
        format!(
            ".roster-tag {{ font-size: 11px; color: {}; background-color: {}; padding: 1px 6px; border-radius: 3px; }}",
            rgb(c.control_text),
            rgb(c.control_bg)
        ),
        format!(
            ".roster-edges {{ margin-top: 6px; padding-top: 4px; }}",
        ),
        format!(
            ".roster-edge {{ display: flex; gap: 5px; padding: 2px 0; font-size: 12px; }}",
        ),
        format!(".roster-edge-dir {{ color: {}; min-width: 14px; }}", rgb(c.muted_text)),
        format!(
            ".roster-edge-kind {{ font-size: 10px; color: {}; background-color: {}; padding: 0px 5px; border-radius: 2px; flex-shrink: 0; }}",
            rgb(c.muted_text),
            rgb(c.control_bg)
        ),
        format!(".roster-edge-target {{ color: {}; }}", rgb(c.body_text)),
        format!(
            ".roster-section {{ font-size: 10px; color: {}; padding: 10px 10px 2px; }}",
            rgb(c.muted_text)
        ),
        format!(
            ".roster-empty {{ font-size: 14px; color: {}; padding: 12px; }}",
            rgb(c.muted_text)
        ),
    ]
}

/// Build the roster DOM from `rows`. Each row shows title, URL, and (when
/// non-empty) a facets strip. The focused row additionally renders its edge
/// detail beneath the facets.
pub fn build_roster_dom(rows: &[RosterRow]) -> ScriptedDom {
    let mut dom = ScriptedDom::new();
    let root = dom.document();
    let container = dom.create_element(qual("div"));
    dom.set_attribute(container, qual("class"), "roster");
    dom.append_child(root, container);

    if rows.is_empty() {
        let empty = dom.create_element(qual("div"));
        dom.set_attribute(empty, qual("class"), "roster-empty");
        let text = dom.create_text("No nodes yet");
        dom.append_child(empty, text);
        dom.append_child(container, empty);
        return dom;
    }

    for row in rows {
        if let Some(header) = &row.section_header {
            let hdr = dom.create_element(qual("div"));
            dom.set_attribute(hdr, qual("class"), "roster-section");
            let hdr_text = dom.create_text(header);
            dom.append_child(hdr, hdr_text);
            dom.append_child(container, hdr);
        }

        let entry = dom.create_element(qual("div"));
        let class = if row.selected { "roster-row-selected" } else { "roster-row" };
        dom.set_attribute(entry, qual("class"), class);
        dom.set_attribute(entry, qual("data-member"), &row.member.to_string());

        let title_el = dom.create_element(qual("div"));
        dom.set_attribute(title_el, qual("class"), "roster-title");
        let t = dom.create_text(&row.title);
        dom.append_child(title_el, t);
        dom.append_child(entry, title_el);

        let sub_el = dom.create_element(qual("div"));
        dom.set_attribute(sub_el, qual("class"), "roster-sub");
        let u = dom.create_text(&row.url);
        dom.append_child(sub_el, u);
        dom.append_child(entry, sub_el);

        let has_facets = row.content_type.is_some() || !row.tags.is_empty();
        if has_facets {
            let facets_el = dom.create_element(qual("div"));
            dom.set_attribute(facets_el, qual("class"), "roster-facets");
            if let Some(ct) = &row.content_type {
                let chip = dom.create_element(qual("span"));
                dom.set_attribute(chip, qual("class"), "roster-chip");
                let ct_text = dom.create_text(ct);
                dom.append_child(chip, ct_text);
                dom.append_child(facets_el, chip);
            }
            for tag in &row.tags {
                let tag_el = dom.create_element(qual("span"));
                dom.set_attribute(tag_el, qual("class"), "roster-tag");
                let tag_text = dom.create_text(tag);
                dom.append_child(tag_el, tag_text);
                dom.append_child(facets_el, tag_el);
            }
            dom.append_child(entry, facets_el);
        }

        if !row.edges.is_empty() {
            let edges_el = dom.create_element(qual("div"));
            dom.set_attribute(edges_el, qual("class"), "roster-edges");
            for edge in &row.edges {
                let edge_el = dom.create_element(qual("div"));
                dom.set_attribute(edge_el, qual("class"), "roster-edge");
                dom.set_attribute(
                    edge_el,
                    qual("data-member"),
                    &edge.other_member.to_string(),
                );

                let dir_el = dom.create_element(qual("span"));
                dom.set_attribute(dir_el, qual("class"), "roster-edge-dir");
                let arrow = match edge.direction {
                    EdgeDir::Out => "\u{2192}", // →
                    EdgeDir::In => "\u{2190}",  // ←
                };
                let arrow_text = dom.create_text(arrow);
                dom.append_child(dir_el, arrow_text);
                dom.append_child(edge_el, dir_el);

                let kind_el = dom.create_element(qual("span"));
                dom.set_attribute(kind_el, qual("class"), "roster-edge-kind");
                let kind_text = dom.create_text(&edge.kind_label);
                dom.append_child(kind_el, kind_text);
                dom.append_child(edge_el, kind_el);

                let target_el = dom.create_element(qual("span"));
                dom.set_attribute(target_el, qual("class"), "roster-edge-target");
                let target_text = dom.create_text(&edge.other_title);
                dom.append_child(target_el, target_text);
                dom.append_child(edge_el, target_el);

                dom.append_child(edges_el, edge_el);
            }
            dom.append_child(entry, edges_el);
        }

        dom.append_child(container, entry);
    }
    dom
}

fn qual(local: &str) -> QualName {
    QualName::new(None, Namespace::from(""), LocalName::from(local))
}

#[cfg(test)]
mod tests {
    use super::*;
    use layout_dom_api::LayoutDom;
    use pelt_live::fragments_from_scripted_dom;

    #[test]
    fn roster_sheet_marks_root_as_scroll_container() {
        let css = roster_sheet(&ChromeTheme::default()).join("\n");
        assert!(css.contains(".roster"));
        assert!(css.contains("overflow: scroll"));
        assert!(css.contains("height: 100%"));
    }

    #[test]
    fn roster_layout_overflows_small_pane() {
        let rows: Vec<RosterRow> = (0..12)
            .map(|i| RosterRow {
                member: GraphMemberId::from_u128(i + 1),
                title: format!("node-{i}"),
                url: format!("https://example.com/node-{i}"),
                content_type: Some("text/html".to_string()),
                tags: Vec::new(),
                edges: Vec::new(),
                selected: i == 0,
                section_header: None,
            })
            .collect();
        let dom = build_roster_dom(&rows);
        let sheet_strings = roster_sheet(&ChromeTheme::default());
        let sheet: Vec<&str> = sheet_strings.iter().map(String::as_str).collect();
        let frags = fragments_from_scripted_dom(&dom, &sheet, 220, 120);
        let root = first_by_class(&dom, dom.document(), "roster").expect("roster root");
        let layout = frags.rect_of(root).expect("roster layout");
        let inner_h = layout.size.height
            - layout.padding.top
            - layout.padding.bottom
            - layout.border.top
            - layout.border.bottom;
        assert!(
            layout.content_size.height > inner_h,
            "roster rows should overflow the visible pane"
        );
    }

    #[test]
    fn roster_facets_render_content_type_and_tags() {
        let rows = vec![RosterRow {
            member: GraphMemberId::from_u128(1),
            title: "My Page".to_string(),
            url: "https://example.com/".to_string(),
            content_type: Some("text/html".to_string()),
            tags: vec!["gemini".to_string(), "research".to_string()],
            edges: Vec::new(),
            selected: false,
            section_header: None,
        }];
        let dom = build_roster_dom(&rows);
        let root = dom.document();
        assert!(first_by_class(&dom, root, "roster-facets").is_some());
        assert!(first_by_class(&dom, root, "roster-chip").is_some());
        assert_eq!(count_by_class(&dom, root, "roster-tag"), 2);
    }

    #[test]
    fn roster_no_facets_strip_when_empty() {
        let rows = vec![RosterRow {
            member: GraphMemberId::from_u128(1),
            title: "Bare".to_string(),
            url: "mere://welcome".to_string(),
            content_type: None,
            tags: Vec::new(),
            edges: Vec::new(),
            selected: false,
            section_header: None,
        }];
        let dom = build_roster_dom(&rows);
        let root = dom.document();
        assert!(first_by_class(&dom, root, "roster-facets").is_none());
    }

    #[test]
    fn roster_edge_rows_render_for_focused_node() {
        let rows = vec![
            RosterRow {
                member: GraphMemberId::from_u128(1),
                title: "Focused".to_string(),
                url: "https://a.example/".to_string(),
                content_type: None,
                tags: Vec::new(),
                edges: vec![
                    EdgeRow {
                        direction: EdgeDir::Out,
                        kind_label: "Hyperlink".to_string(),
                        other_title: "Target".to_string(),
                        other_url: "https://b.example/".to_string(),
                        other_member: GraphMemberId::from_u128(2),
                    },
                    EdgeRow {
                        direction: EdgeDir::In,
                        kind_label: "Traversal".to_string(),
                        other_title: "Source".to_string(),
                        other_url: "https://c.example/".to_string(),
                        other_member: GraphMemberId::from_u128(3),
                    },
                ],
                selected: true,
                section_header: None,
            },
            RosterRow {
                member: GraphMemberId::from_u128(2),
                title: "Other".to_string(),
                url: "https://b.example/".to_string(),
                content_type: None,
                tags: Vec::new(),
                edges: Vec::new(),
                selected: false,
                section_header: None,
            },
        ];
        let dom = build_roster_dom(&rows);
        let root = dom.document();
        assert!(first_by_class(&dom, root, "roster-edges").is_some());
        assert_eq!(count_by_class(&dom, root, "roster-edge"), 2);
        assert_eq!(count_by_class(&dom, root, "roster-edge-kind"), 2);
    }

    #[test]
    fn roster_section_headers_render_for_grouped_rows() {
        let rows = vec![
            RosterRow {
                member: GraphMemberId::from_u128(1),
                title: "A Feed".to_string(),
                url: "https://example.com/feed".to_string(),
                content_type: Some("application/rss+xml".to_string()),
                tags: Vec::new(),
                edges: Vec::new(),
                selected: false,
                section_header: Some("Feeds".to_string()),
            },
            RosterRow {
                member: GraphMemberId::from_u128(2),
                title: "A Page".to_string(),
                url: "https://example.com/page".to_string(),
                content_type: Some("text/html".to_string()),
                tags: Vec::new(),
                edges: Vec::new(),
                selected: false,
                section_header: Some("Documents".to_string()),
            },
        ];
        let dom = build_roster_dom(&rows);
        let root = dom.document();
        assert_eq!(count_by_class(&dom, root, "roster-section"), 2);
    }

    fn first_by_class(
        dom: &ScriptedDom,
        id: serval_scripted_dom::NodeId,
        class: &str,
    ) -> Option<serval_scripted_dom::NodeId> {
        if dom.attributes(id).any(|attr| {
            attr.name.local.as_ref() == "class" && attr.value.split_whitespace().any(|c| c == class)
        }) {
            return Some(id);
        }
        dom.dom_children(id)
            .find_map(|child| first_by_class(dom, child, class))
    }

    fn count_by_class(
        dom: &ScriptedDom,
        id: serval_scripted_dom::NodeId,
        class: &str,
    ) -> usize {
        let here = usize::from(dom.attributes(id).any(|attr| {
            attr.name.local.as_ref() == "class"
                && attr.value.split_whitespace().any(|c| c == class)
        }));
        here + dom.dom_children(id).map(|c| count_by_class(dom, c, class)).sum::<usize>()
    }
}
