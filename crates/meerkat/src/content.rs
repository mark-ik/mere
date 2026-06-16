/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The content actor: a focused tile's media rendered off the UI thread.
//!
//! P2 of the actor constellation plan. The kernel ships a tile's fetched document
//! to a content actor; the actor owns the serval cascade + nematic engines + a
//! per-tile subresource cache on its own thread (the cascade is confirmed
//! off-thread-safe, see the `cascade-offthread` probe), runs the existing
//! [`render_content_scene`](crate::card::render_content_scene) there, and ships a
//! `Send` [`Scene`] back. The kernel composites the latest scene and stays the sole
//! GPU owner.
//!
//! Three update kinds cross the boundary, all `Send`: the rendered [`Scene`], the
//! subresource URLs the render [`Wanted`](ContentUpdate::Wanted) (the kernel
//! fetches them through the I/O fetch actor and feeds the bytes back as
//! [`Resource`](ContentCommand::Resource)), and any linked-data
//! [`Contribution`](ContentUpdate::Contribution) harvested from the document (the
//! kernel applies it to the graph). The actor never touches the graph or the GPU.

use std::cell::RefCell;

use armillary::{ActorHandle, Emitter, NavGeneration, Pool, ViewportGeneration, Wake, spawn_on};
use document_canvas::{DocumentRenderPacket, FontTable};
use inker::{EngineRegistry, EngineRoutePolicy};
use linked_data::GraphContribution;
use netrender::Scene;

use crate::card::{render_content, LinkHit, RenderedContent};
use crate::fetch::ContentState;
use crate::resources::{ResourceLoader, ResourceStore};

/// A command from the kernel to a content actor.
pub enum ContentCommand {
    /// Show `fetched` at `url` (a fresh navigation): harvest its linked data once,
    /// then render at `viewport`. The generations tag the work the kernel gets
    /// back so it can drop a scene built for a page/size the tile has left.
    Show {
        url: String,
        /// The focused card's content state (`None` for a synthesized page such as
        /// `mere://welcome`); the actor renders any state, harvesting only `Ready`.
        state: Option<ContentState>,
        viewport: (u32, u32),
        nav: NavGeneration,
        viewport_gen: ViewportGeneration,
    },
    /// Re-render the current document at a new size.
    Resize {
        viewport: (u32, u32),
        viewport_gen: ViewportGeneration,
    },
    /// A subresource the kernel fetched on the actor's behalf has arrived: cache
    /// its bytes and re-render so the demand loader now hits.
    Resource { url: String, bytes: Vec<u8> },
}

/// An update from a content actor to the kernel. All variants are `Send`.
pub enum ContentUpdate {
    /// A document-lane render: the **retained packet** plus its font sidecar, the
    /// host windows + lowers a band of per scroll. `content_height` is the full
    /// laid-out height (px); the host scrolls the full extent and rasterizes one band
    /// at a time, so a tall page is never one giant texture. (Tiled render.)
    Document {
        nav: NavGeneration,
        viewport_gen: ViewportGeneration,
        packet: DocumentRenderPacket,
        fonts: FontTable,
        content_height: u32,
        // Link hit-testing reads the packet's own interactions
        // (`DocumentRenderPacket::link_at`), so the document lane ships no separate
        // link-rect table. (Phase 2 query API.)
    },
    /// An HTML/serval-lane render: one pre-lowered scene (a different pipeline that
    /// does not produce a document packet). `content_height` is the full laid-out
    /// height; the host rasterizes a texture this tall and scrolls a window of it on
    /// the GPU. Its true-height + windowing is the Phase 5 lane-parity work.
    Scene {
        nav: NavGeneration,
        viewport_gen: ViewportGeneration,
        scene: Scene,
        content_height: u32,
        /// Content-local clickable link regions harvested from the laid-out
        /// document; the host hit-tests a click against these and navigates.
        links: Vec<LinkHit>,
        /// Blurred box-shadow mask requests the host builds (GPU) and registers
        /// before rasterizing `scene`. Empty when the page has no blurred shadows.
        masks: Vec<paint_list_render::BoxShadowMaskRequest>,
    },
    /// Subresource URLs (absolute) the last render needs but did not have cached.
    /// The kernel fetches them and feeds the bytes back as [`ContentCommand::Resource`].
    Wanted {
        nav: NavGeneration,
        urls: Vec<String>,
    },
    /// Linked data harvested from the document, for the kernel to apply.
    Contribution {
        contributions: Vec<GraphContribution>,
    },
}

/// The actor-thread-local current document.
struct Content {
    url: String,
    state: Option<ContentState>,
    viewport: (u32, u32),
    nav: NavGeneration,
    viewport_gen: ViewportGeneration,
}

/// Spawn a content actor on its own thread (armillary harness). It builds the
/// nematic engine registry and an empty subresource cache on that thread (neither
/// crosses the boundary), then renders on each command. Returns the kernel's
/// command handle plus the receiver of [`ContentUpdate`]s to drain.
pub fn spawn_content(
    pool: &Pool,
    wake: Wake,
    disabled: std::collections::HashSet<String>,
) -> (
    ActorHandle<ContentCommand>,
    std::sync::mpsc::Receiver<ContentUpdate>,
) {
    spawn_on(pool, wake, move |commands, out: Emitter<ContentUpdate>| {
        let mut registry = EngineRegistry::new();
        for engine in nematic::engines() {
            // Skip engines deactivated this session: an unregistered engine is
            // routed past by the policy (`route_document_engine`), so its content
            // falls to the synthesized card. (engine-picker Phase 1b.)
            if !disabled.contains(engine.engine_id()) {
                registry.register(engine);
            }
        }
        // The actor's own copy of the default routing policy (it owns its registry
        // too); content-type routing for this node's document runs against it.
        let policy = EngineRoutePolicy::default();
        let store = RefCell::new(ResourceStore::default());
        let mut current: Option<Content> = None;

        while let Ok(command) = commands.recv() {
            match command {
                ContentCommand::Show {
                    url,
                    state,
                    viewport,
                    nav,
                    viewport_gen,
                } => {
                    // Harvest the document's linked data once, on load (Ready only).
                    if let Some(ContentState::Ready(fetched)) = &state {
                        let contributions = meerkat::ingest::harvest_contributions(
                            fetched.content_type.as_deref(),
                            &fetched.body,
                        );
                        if !contributions.is_empty() {
                            out.emit(ContentUpdate::Contribution { contributions });
                        }
                    }
                    let content = Content {
                        url,
                        state,
                        viewport,
                        nav,
                        viewport_gen,
                    };
                    render(&content, &store, &registry, &policy, &out);
                    current = Some(content);
                }
                ContentCommand::Resize {
                    viewport,
                    viewport_gen,
                } => {
                    if let Some(content) = current.as_mut() {
                        content.viewport = viewport;
                        content.viewport_gen = viewport_gen;
                        render(content, &store, &registry, &policy, &out);
                    }
                }
                ContentCommand::Resource { url, bytes } => {
                    store.borrow_mut().insert(url, bytes);
                    if let Some(content) = current.as_ref() {
                        render(content, &store, &registry, &policy, &out);
                    }
                }
            }
        }
    })
}

/// Render `content` against the cached subresources, emitting the scene and any
/// subresources the render newly wants.
fn render(
    content: &Content,
    store: &RefCell<ResourceStore>,
    registry: &EngineRegistry,
    policy: &EngineRoutePolicy,
    out: &Emitter<ContentUpdate>,
) {
    let wanted = RefCell::new(Vec::new());
    let (w, h) = content.viewport;
    let rendered = {
        let loader = ResourceLoader::new(store, &content.url, &wanted);
        render_content(
            &content.url,
            content.state.as_ref(),
            registry,
            policy,
            &loader,
            w,
            h,
        )
    };
    match rendered {
        RenderedContent::Document {
            packet,
            fonts,
            content_height,
        } => out.emit(ContentUpdate::Document {
            nav: content.nav,
            viewport_gen: content.viewport_gen,
            packet,
            fonts,
            content_height,
        }),
        RenderedContent::Html {
            scene,
            content_height,
            links,
            masks,
        } => out.emit(ContentUpdate::Scene {
            nav: content.nav,
            viewport_gen: content.viewport_gen,
            scene,
            content_height,
            masks,
            links,
        }),
    }
    // Ship only never-requested subresources, so a re-render before the bytes
    // arrive does not re-request them (the store dedups).
    let fresh: Vec<String> = wanted
        .into_inner()
        .into_iter()
        .filter(|url| store.borrow_mut().request(url.clone()))
        .collect();
    if !fresh.is_empty() {
        out.emit(ContentUpdate::Wanted {
            nav: content.nav,
            urls: fresh,
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::fetch::Fetched;

    fn noop_wake() -> Wake {
        Arc::new(|| {})
    }

    fn show(url: &str, content_type: &str, body: &str) -> ContentCommand {
        ContentCommand::Show {
            url: url.to_string(),
            state: Some(ContentState::Ready(Fetched {
                content_type: Some(content_type.to_string()),
                body: body.to_string(),
            })),
            viewport: (420, 360),
            nav: NavGeneration::default(),
            viewport_gen: ViewportGeneration::default(),
        }
    }

    fn glyph_runs(scene: &Scene) -> usize {
        scene
            .ops
            .iter()
            .filter(|op| matches!(op, netrender::SceneOp::GlyphRun(_)))
            .count()
    }

    #[test]
    fn show_renders_a_scene_off_thread() {
        let (handle, updates) =
            spawn_content(&Pool::new(), noop_wake(), std::collections::HashSet::new());
        handle.command(show(
            "https://example.com/",
            "text/html",
            "<h1>Hi</h1><p>There</p>",
        ));
        handle.join();

        let scene = updates
            .iter()
            .find_map(|u| match u {
                ContentUpdate::Scene { scene, .. } => Some(scene),
                _ => None,
            })
            .expect("a scene update");
        assert!(
            glyph_runs(&scene) >= 1,
            "the off-thread render lowered text to glyph runs"
        );
    }

    #[test]
    fn show_harvests_embedded_jsonld_into_a_contribution() {
        let (handle, updates) =
            spawn_content(&Pool::new(), noop_wake(), std::collections::HashSet::new());
        handle.command(show(
            "https://example.com/",
            "text/html",
            r#"<script type="application/ld+json">
               {"@context":{"name":"https://schema.org/name"},"@id":"mere://z","name":"Z"}
               </script><p>body</p>"#,
        ));
        handle.join();

        let harvested = updates.iter().any(|u| match u {
            ContentUpdate::Contribution { contributions } => contributions
                .iter()
                .any(|c| c.nodes.iter().any(|n| n.id == "mere://z")),
            _ => false,
        });
        assert!(harvested, "embedded JSON-LD harvested into a Contribution");
    }
}
