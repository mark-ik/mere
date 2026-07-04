/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Web clip host wrapper: drive capture from the current node, load the best
//! available document body, and hand the host-neutral clip core off to `import`.

use forme::GraphMemberId;
use import::web_clip::{clip_title, fragment_from_body, write_clip_node};

pub(crate) use import::web_clip::{
    ClipFragment, attach_cropped_visual, parse_web_clip, web_clip_script,
};

use super::{WindowCtx, fetch};

impl WindowCtx<'_> {
    /// Run the clip command for the focused node. Scriptable surface nodes arm a click picker;
    /// other nodes clip the whole known document through the same fragment -> knot path.
    pub(super) fn start_clip_picker(&mut self) -> Option<String> {
        let Some(member) = self.focused_member() else {
            return Some("clip: select a node first".to_string());
        };
        let Some(source_url) = self
            .orrery()
            .graph()
            .get_node_by_id(member)
            .map(|(_, node)| node.url().to_string())
        else {
            return Some("clip: focused node is missing".to_string());
        };

        if self.is_surface_tier(member, &source_url) {
            self.open_workbench();
            self.view.workbench.open_tile(member);
            self.set_focused_tile(Some(member));
            self.view.clip_picker = Some(member);
            self.view.request_redraw();
            return Some("clip: click an element to capture".to_string());
        }

        Some(self.clip_whole_document(member, &source_url))
    }

    /// Complete an armed surface picker. Called from the mouse press path before the click is
    /// forwarded into the WebView, so the selected element remains under the cursor.
    pub(crate) fn finish_clip_pick(&mut self, member: GraphMemberId, x: i32, y: i32) -> String {
        if self.view.clip_picker.take() != Some(member) {
            return "clip: canceled".to_string();
        }
        let Some(source_url) = self
            .orrery()
            .graph()
            .get_node_by_id(member)
            .map(|(_, node)| node.url().to_string())
        else {
            return "clip: focused node is missing".to_string();
        };
        match self.view.scrying.capture_clip(member, x, y, &source_url) {
            Ok(fragment) => self.create_clip_node(member, fragment),
            Err(err) => format!("clip: {err}"),
        }
    }

    pub(crate) fn cancel_clip_picker(&mut self) -> Option<String> {
        self.view
            .clip_picker
            .take()
            .map(|_| "clip: canceled".to_string())
    }

    fn clip_whole_document(&mut self, member: GraphMemberId, source_url: &str) -> String {
        match self.document_fragment(member, source_url) {
            Some(fragment) => self.create_clip_node(member, fragment),
            None => "clip: no loaded or cached document for focused node".to_string(),
        }
    }

    fn document_fragment(
        &mut self,
        member: GraphMemberId,
        source_url: &str,
    ) -> Option<ClipFragment> {
        let (node_title, node_body, node_mime) =
            self.orrery()
                .graph()
                .get_node_by_id(member)
                .map(|(_, node)| {
                    (
                        Some(node.title.clone()),
                        node.body.clone(),
                        node.mime_hint.clone(),
                    )
                })?;
        if let Some(body) = node_body.filter(|body| !body.trim().is_empty()) {
            return Some(fragment_from_body(
                source_url,
                node_title,
                node_mime.as_deref(),
                &body,
                &self.shared.content.engine_registry,
                &self.shared.content.route_policy,
            ));
        }

        let live = self
            .shared
            .content
            .pages
            .get(source_url)
            .and_then(|state| match state {
                fetch::ContentState::Ready(fetched) => {
                    Some((fetched.content_type.clone(), fetched.body.clone()))
                }
                _ => None,
            });
        let (content_type, body) = match live {
            Some(ready) => ready,
            None => {
                let stored = self.load_cached(source_url)?;
                (
                    stored.content_type.clone(),
                    String::from_utf8_lossy(&stored.body).into_owned(),
                )
            }
        };
        Some(fragment_from_body(
            source_url,
            node_title,
            content_type.as_deref(),
            &body,
            &self.shared.content.engine_registry,
            &self.shared.content.route_policy,
        ))
    }

    fn create_clip_node(&mut self, source_member: GraphMemberId, fragment: ClipFragment) -> String {
        let Some(source_key) = self
            .orrery()
            .graph()
            .get_node_by_id(source_member)
            .map(|(key, _)| key)
        else {
            return "clip: source node is missing".to_string();
        };

        let title = clip_title(&fragment);
        let clip_url = format!("knot://clip/{}", uuid::Uuid::new_v4());
        let mut member = None;
        self.orrery_mut().ingest_graph(|graph| {
            member = write_clip_node(graph, source_key, &clip_url, &fragment);
            member.is_some()
        });

        let Some(member) = member else {
            return "clip: source node is missing".to_string();
        };
        if let Some(visual) = &fragment.visual {
            self.orrery_mut()
                .set_node_sprite(member, visual.data_uri.clone());
            self.orrery_mut().set_node_sprite_hull(
                member,
                vec![(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)],
            );
        }
        self.orrery_mut().set_selected_members(&[member]);
        self.ensure_content(&clip_url);
        self.open_workbench();
        self.view.workbench.open_tile(member);
        self.set_focused_tile(Some(member));
        self.view.scrying_input_focus = None;
        self.save_session();
        self.view.request_redraw();
        format!("clip: saved {title}")
    }
}
