/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The content actor build + spawn loop (relational-browse fetch/render driver).

use super::*;

/// Engine-painted find-highlight fills (overlay-roots P2), mirroring the retired
/// host overlay's amber tints: every match translucent, the active match stronger.
const FIND_HIGHLIGHT: serval_layout::HighlightStyle = serval_layout::HighlightStyle {
    color: paint_list_api::ColorF {
        r: 1.0,
        g: 0.82,
        b: 0.20,
        a: 0.38,
    },
};
const FIND_ACTIVE_HIGHLIGHT: serval_layout::HighlightStyle = serval_layout::HighlightStyle {
    color: paint_list_api::ColorF {
        r: 1.0,
        g: 0.55,
        b: 0.10,
        a: 0.55,
    },
};

/// Build the scripted-rung document for a scripted Serval node: parse the already-
/// fetched HTML body and run its scripts. With a `fetcher`, external `<script src>` is
/// fetched through it (`from_body`, no document re-fetch); without one, inline scripts
/// only (`parse`). `None` for any other engine or a non-`Ready` state. (Render ladder.)
#[cfg(feature = "scripted")]
pub(crate) fn build_scripted(
    engine: &str,
    url: &str,
    state: Option<&ContentState>,
    fetcher: Option<&dyn pelt_desktop::ScriptResourceFetcher>,
) -> Option<HostScriptedDocument> {
    let Some(ContentState::Ready(fetched)) = state else {
        return None;
    };
    // The node's origin cookies, so the page's JS shares the session HTTP uses.
    let cookies: Option<Box<dyn pelt_desktop::CookieProvider>> =
        url::Url::parse(url).ok().map(|parsed| {
            Box::new(JarCookieProvider { url: parsed }) as Box<dyn pelt_desktop::CookieProvider>
        });
    let result =
        match engine {
            inker::routing::ENGINE_SERVAL_SCRIPTED => match fetcher {
                Some(fetcher) => {
                    ScriptedDocument::<BoaEngine>::from_body(&fetched.body, fetcher, url, cookies)
                        .map(HostScriptedDocument::Boa)
                }
                // No fetcher (the blocking fetch could not be built): inline-only, no cookies.
                None => ScriptedDocument::<BoaEngine>::parse(&fetched.body)
                    .map(HostScriptedDocument::Boa),
            },
            #[cfg(feature = "scripted-nova")]
            inker::routing::ENGINE_SERVAL_SCRIPTED_NOVA => match fetcher {
                Some(fetcher) => {
                    ScriptedDocument::<NovaEngine>::from_body(&fetched.body, fetcher, url, cookies)
                        .map(HostScriptedDocument::Nova)
                }
                // No fetcher (the blocking fetch could not be built): inline-only, no cookies.
                None => ScriptedDocument::<NovaEngine>::parse(&fetched.body)
                    .map(HostScriptedDocument::Nova),
            },
            _ => return None,
        };
    match result {
        Ok(doc) => Some(doc),
        Err(err) => {
            tracing::warn!(%err, "scripted rung: document init failed");
            None
        }
    }
}

/// Spawn a content actor on its own thread (armillary harness). It builds the
/// nematic engine registry and an empty subresource cache on that thread (neither
/// crosses the boundary), then renders on each command. Returns the kernel's
/// command handle plus the receiver of [`ContentUpdate`]s to drain.
pub fn spawn_content(
    pool: &Pool,
    wake: Wake,
    disabled: std::collections::HashSet<String>,
    auto_ingest: bool,
) -> (
    ActorHandle<ContentCommand>,
    std::sync::mpsc::Receiver<ContentUpdate>,
) {
    spawn_on(pool, wake, move |commands, out: Emitter<ContentUpdate>| {
        run_content(commands, &out, disabled, auto_ingest);
    })
}

pub fn spawn_content_transfer(
    pool: &Pool,
    wake: Wake,
    disabled: std::collections::HashSet<String>,
    auto_ingest: bool,
) -> (
    ActorHandle<ContentCommand>,
    std::sync::mpsc::Receiver<TransferBuffer>,
) {
    spawn_on(pool, wake, move |commands, out: Emitter<TransferBuffer>| {
        let out = TransferContentUpdateSink::new(out);
        run_content(commands, &out, disabled, auto_ingest);
    })
}

#[allow(dead_code)]
pub fn spawn_content_with_transport(
    pool: &Pool,
    wake: Wake,
    disabled: std::collections::HashSet<String>,
    auto_ingest: bool,
    transport: ContentUpdateTransport,
) -> (ContentHandle, ContentUpdateStream) {
    match transport {
        ContentUpdateTransport::Native => {
            let (handle, updates) = spawn_content(pool, wake, disabled, auto_ingest);
            (
                ContentHandle::Native(handle),
                ContentUpdateStream::Native(updates),
            )
        }
        ContentUpdateTransport::Transfer => {
            #[cfg(target_arch = "wasm32")]
            {
                return spawn_content_worker(wake, disabled, auto_ingest);
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                let (handle, updates) = spawn_content_transfer(pool, wake, disabled, auto_ingest);
                (
                    ContentHandle::Native(handle),
                    ContentUpdateStream::Transfer {
                        updates,
                        decoder: SceneTransferDecoder::default(),
                    },
                )
            }
        }
    }
}

struct TransferContentUpdateSink {
    out: Emitter<TransferBuffer>,
    encoder: RefCell<SceneTransferEncoder>,
}

impl TransferContentUpdateSink {
    fn new(out: Emitter<TransferBuffer>) -> Self {
        Self {
            out,
            encoder: RefCell::new(SceneTransferEncoder::default()),
        }
    }
}

impl ContentUpdateSink for TransferContentUpdateSink {
    fn emit_update(&self, update: ContentUpdate) {
        match update.into_transfer_buffer(&mut self.encoder.borrow_mut()) {
            Ok(buffer) => self.out.emit(buffer),
            Err(err) => {
                let reason = err.to_string();
                tracing::warn!(%reason, "content update transfer encode failed");
                match TransferBuffer::from_transport_error(reason) {
                    Ok(buffer) => self.out.emit(buffer),
                    Err(fallback) => {
                        tracing::warn!(%fallback, "content transport error encode failed");
                    }
                }
            }
        }
    }
}

pub(crate) struct ContentRuntime {
    registry: EngineRegistry,
    policy: EngineRoutePolicy,
    store: RefCell<ResourceStore>,
    current: Option<Content>,
    net_fetcher: Option<Arc<dyn NetFetcher>>,
    auto_ingest: bool,
}

impl ContentRuntime {
    pub(crate) fn new(disabled: std::collections::HashSet<String>, auto_ingest: bool) -> Self {
        let mut registry = EngineRegistry::new();
        for engine in nematic::engines() {
            if !disabled.contains(engine.engine_id()) {
                registry.register(engine);
            }
        }
        Self {
            registry,
            policy: EngineRoutePolicy::default(),
            store: RefCell::new(ResourceStore::default()),
            current: None,
            net_fetcher: None,
            auto_ingest,
        }
    }
}

fn run_content<S: ContentUpdateSink>(
    commands: std::sync::mpsc::Receiver<ContentCommand>,
    out: &S,
    disabled: std::collections::HashSet<String>,
    auto_ingest: bool,
) {
    let mut runtime = ContentRuntime::new(disabled, auto_ingest);
    while let Ok(command) = commands.recv() {
        runtime.handle_command(command, out);
    }
}

impl ContentRuntime {
    pub(crate) fn handle_command<S: ContentUpdateSink>(
        &mut self,
        command: ContentCommand,
        out: &S,
    ) {
        match command {
            ContentCommand::Show {
                url,
                state,
                engine,
                viewport,
                nav,
                viewport_gen,
                sheet,
            } => {
                if self.auto_ingest {
                    if let Some(ContentState::Ready(fetched)) = &state {
                        let mut contributions = meerkat::ingest::harvest_contributions(
                            fetched.content_type.as_deref(),
                            &fetched.body,
                        );
                        #[cfg(feature = "scripted")]
                        let is_scripted_rung = inker::routing::serval_rung(&engine)
                            == Some(inker::routing::ServalRung::Scripted);
                        #[cfg(not(feature = "scripted"))]
                        let is_scripted_rung = false;
                        if !is_scripted_rung {
                            if let Some(extract) = meerkat::ingest::page_extract_contribution(
                                &url,
                                fetched.content_type.as_deref(),
                                &fetched.body,
                            ) {
                                contributions.push(extract);
                            }
                        }
                        if !contributions.is_empty() {
                            out.emit_update(ContentUpdate::Contribution { contributions });
                        }
                    }
                }
                #[cfg(feature = "scripted")]
                let scripted_doc = {
                    let fetcher = ScriptFetcher::new();
                    build_scripted(
                        &engine,
                        &url,
                        state.as_ref(),
                        fetcher
                            .as_ref()
                            .map(|f| f as &dyn pelt_desktop::ScriptResourceFetcher),
                    )
                };
                #[cfg(not(feature = "scripted"))]
                let _ = &engine;
                #[cfg(feature = "scripted")]
                if self.auto_ingest {
                    if let Some(doc) = scripted_doc.as_ref() {
                        if let Some(extract) =
                            meerkat::ingest::contribution_from_page_extract(&url, doc.extract())
                        {
                            out.emit_update(ContentUpdate::Contribution {
                                contributions: vec![extract],
                            });
                        }
                    }
                }
                let mut content = Content {
                    url,
                    state,
                    viewport,
                    nav,
                    viewport_gen,
                    band_y: 0,
                    band_h: viewport.1,
                    sheet,
                    html: None,
                    find_ranges: Vec::new(),
                    script: None,
                    #[cfg(feature = "scripted")]
                    scripted_doc,
                    #[cfg(feature = "smolweb")]
                    smolweb: None,
                    last_scene_sig: None,
                };
                if let Some(old) = self.current.as_mut().and_then(|c| c.script.take()) {
                    let _ = old.detach();
                }
                render(&mut content, &self.store, &self.registry, &self.policy, out);
                self.current = Some(content);
            }
            ContentCommand::Resize {
                viewport,
                viewport_gen,
            } => {
                if let Some(content) = self.current.as_mut() {
                    content.viewport = viewport;
                    content.viewport_gen = viewport_gen;
                    content.band_y = 0;
                    content.html = None;
                    content.find_ranges.clear();
                    relayout_script(content, &self.store, out, viewport.0, viewport.1);
                    render(content, &self.store, &self.registry, &self.policy, out);
                }
            }
            ContentCommand::Retheme {
                sheet,
                viewport_gen,
            } => {
                if let Some(content) = self.current.as_mut() {
                    content.sheet = sheet;
                    #[cfg(feature = "smolweb")]
                    {
                        content.smolweb = None;
                    }
                    content.viewport_gen = viewport_gen;
                    render(content, &self.store, &self.registry, &self.policy, out);
                }
            }
            ContentCommand::Resource { url, bytes } => {
                self.store.borrow_mut().insert(url, bytes);
                if let Some(content) = self.current.as_mut() {
                    content.html = None;
                    content.find_ranges.clear();
                    let (w, h) = content.viewport;
                    relayout_script(content, &self.store, out, w, h);
                    render(content, &self.store, &self.registry, &self.policy, out);
                }
            }
            ContentCommand::Scroll {
                band_y,
                band_h,
                viewport_gen,
            } => {
                if let Some(content) = self.current.as_mut() {
                    content.band_y = band_y;
                    content.band_h = band_h;
                    content.viewport_gen = viewport_gen;
                    render(content, &self.store, &self.registry, &self.policy, out);
                }
            }
            ContentCommand::Find {
                query,
                viewport_gen,
            } => {
                if let Some(content) = self.current.as_mut() {
                    let wanted = RefCell::new(Vec::new());
                    let matches = if ensure_html_layout(
                        content,
                        &self.store,
                        &self.registry,
                        &self.policy,
                        &wanted,
                    ) {
                        // Engine-painted find (overlay-roots P2): register the
                        // matches as the "find" custom highlight (the first as
                        // "find-active") on the retained layout, so the next band
                        // emit carries the fills in-band — scroll-locked, banded,
                        // and zoomed with the content. Rects still ship, but only
                        // as count/step/auto-scroll metadata. An empty query
                        // clears both names (empty ranges remove).
                        let (doc, layout) = content.html.as_mut().expect("ensured");
                        let ranges = layout.find_ranges(doc, &query);
                        let matches: Vec<Vec<[f32; 4]>> =
                            ranges.iter().map(|r| layout.range_rects(doc, r)).collect();
                        layout.set_highlight("find", ranges.clone(), FIND_HIGHLIGHT);
                        layout.set_highlight(
                            "find-active",
                            ranges.first().copied().into_iter().collect(),
                            FIND_ACTIVE_HIGHLIGHT,
                        );
                        content.find_ranges = ranges;
                        // Highlights aren't in the scene fingerprint; bust it so the
                        // re-render actually ships the freshly-highlighted band.
                        content.last_scene_sig = None;
                        render(content, &self.store, &self.registry, &self.policy, out);
                        matches
                    } else {
                        Vec::new()
                    };
                    out.emit_update(ContentUpdate::FindMatches {
                        nav: content.nav,
                        viewport_gen,
                        matches,
                    });
                }
            }
            ContentCommand::FindActive {
                index,
                viewport_gen,
            } => {
                if let Some(content) = self.current.as_mut() {
                    content.viewport_gen = viewport_gen;
                    let range = content.find_ranges.get(index).copied();
                    if let (Some(range), Some((_, layout))) = (range, content.html.as_mut()) {
                        layout.set_highlight("find-active", vec![range], FIND_ACTIVE_HIGHLIGHT);
                        content.last_scene_sig = None;
                        render(content, &self.store, &self.registry, &self.policy, out);
                    }
                }
            }
            ContentCommand::SelectText {
                anchor,
                focus,
                viewport_gen,
            } => {
                if let Some(content) = self.current.as_mut() {
                    let wanted = RefCell::new(Vec::new());
                    let selection = if ensure_html_layout(
                        content,
                        &self.store,
                        &self.registry,
                        &self.policy,
                        &wanted,
                    ) {
                        let (doc, layout) = content.html.as_ref().expect("ensured");
                        layout
                            .select_text(doc, anchor, focus, &ScrollOffsets::default())
                            .map(|selection| content_contract::TextSelectionMessage {
                                rects: selection
                                    .rects
                                    .into_iter()
                                    .map(|r| [r.x, r.y, r.x + r.width, r.y + r.height])
                                    .collect(),
                                text: selection.text,
                            })
                    } else {
                        None
                    };
                    out.emit_update(ContentUpdate::TextSelection {
                        nav: content.nav,
                        viewport_gen,
                        selection,
                    });
                }
            }
            ContentCommand::AttachScript {
                component_path,
                log,
                document,
                net,
                viewport_gen,
            } => {
                if let Some(content) = self.current.as_mut() {
                    content.viewport_gen = viewport_gen;
                    let grant = script::grant_from_resolved(log, document, net);
                    if self.net_fetcher.is_none()
                        && matches!(grant.net, document_host::CapPermission::Allow)
                    {
                        self.net_fetcher = script::ContentNetFetcher::new()
                            .map(|f| Arc::new(f) as Arc<dyn NetFetcher>)
                            .map_err(|e| tracing::warn!(%e, "content net fetcher unavailable"))
                            .ok();
                    }
                    let outcome = attach_script(
                        content,
                        &component_path,
                        &grant,
                        self.net_fetcher.clone(),
                        &self.store,
                        &self.registry,
                        &self.policy,
                        out,
                    );
                    out.emit_update(ContentUpdate::ScriptOutcome {
                        nav: content.nav,
                        outcome,
                    });
                    if content.script.is_some() {
                        render(content, &self.store, &self.registry, &self.policy, out);
                    }
                }
            }
            ContentCommand::DeliverEvent {
                kind,
                payload,
                viewport_gen,
            } => {
                if let Some(content) = self.current.as_mut() {
                    if content.script.is_some() {
                        content.viewport_gen = viewport_gen;
                        let outcome = deliver_event(content, &kind, &payload, &self.store, out);
                        out.emit_update(ContentUpdate::ScriptOutcome {
                            nav: content.nav,
                            outcome,
                        });
                        render(content, &self.store, &self.registry, &self.policy, out);
                    }
                }
            }
            ContentCommand::DetachScript { viewport_gen } => {
                if let Some(content) = self.current.as_mut() {
                    let outcome = match content.script.take() {
                        Some(inst) => match inst.detach() {
                            Ok(_) => "detached".to_string(),
                            Err(e) => format!("detach error: {e}"),
                        },
                        None => "no script attached".to_string(),
                    };
                    content.html = None;
                    content.find_ranges.clear();
                    content.viewport_gen = viewport_gen;
                    out.emit_update(ContentUpdate::ScriptOutcome {
                        nav: content.nav,
                        outcome,
                    });
                    render(content, &self.store, &self.registry, &self.policy, out);
                }
            }
            ContentCommand::MaterializeLinks { viewport_gen } => {
                if let Some(content) = self.current.as_mut() {
                    content.viewport_gen = viewport_gen;
                    if let Some(ContentState::Ready(fetched)) = &content.state {
                        if let Some(contribution) =
                            meerkat::ingest::harvest_links(&content.url, &fetched.body)
                        {
                            out.emit_update(ContentUpdate::Contribution {
                                contributions: vec![contribution],
                            });
                        }
                    }
                }
            }
            ContentCommand::SetLifecycle { hidden, frozen } => {
                // Scripted lane only: throttle/stop the page's task pump per
                // Page Visibility / Page Lifecycle; becoming visible re-renders
                // once so a band that went stale while hidden refreshes. The
                // other lanes have no tasks to gate.
                #[cfg(feature = "scripted")]
                if let Some(content) = self.current.as_mut() {
                    if let Some(doc) = content.scripted_doc.as_mut() {
                        let was_presented = !doc.is_hidden() && !doc.is_frozen();
                        if frozen {
                            doc.set_hidden(hidden);
                            doc.freeze();
                        } else {
                            doc.resume();
                            doc.set_hidden(hidden);
                        }
                        let presented = !hidden && !frozen;
                        if presented && !was_presented {
                            render(content, &self.store, &self.registry, &self.policy, out);
                        }
                    }
                }
                #[cfg(not(feature = "scripted"))]
                let _ = (hidden, frozen);
            }
            #[cfg(feature = "scripted")]
            ContentCommand::ScriptedClick { x, y, viewport_gen } => {
                if let Some(content) = self.current.as_mut() {
                    if let Some(doc) = content.scripted_doc.as_mut() {
                        doc.click_at(x, y);
                        content.viewport_gen = viewport_gen;
                        render(content, &self.store, &self.registry, &self.policy, out);
                    }
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            ContentCommand::SetOverlay {
                name,
                anchor,
                content: overlay,
                viewport_gen,
            } => {
                if let Some(content) = self.current.as_mut() {
                    content.viewport_gen = viewport_gen;
                    // The overlay slot lives on the retained HTML-lane layout, so
                    // ensure it exists (same lazy build the Find arm relies on),
                    // then register the host-laid-out satellite against the
                    // resolved anchor and re-emit. `set_overlay` touches no
                    // style/layout state (the satellite is a pre-emitted paint
                    // list composited after content), so this is repaint-only —
                    // the page never reflows. (Overlay-roots P1.)
                    let wanted = RefCell::new(Vec::new());
                    if ensure_html_layout(
                        content,
                        &self.store,
                        &self.registry,
                        &self.policy,
                        &wanted,
                    ) {
                        let (doc, layout) = content.html.as_mut().expect("ensured");
                        if let Some(anchor_node) = resolve_overlay_anchor(doc, anchor) {
                            layout.set_overlay(&name, anchor_node, overlay);
                            // Overlays aren't in the scene fingerprint; bust it so
                            // the re-render ships the freshly-composited band.
                            content.last_scene_sig = None;
                            render(content, &self.store, &self.registry, &self.policy, out);
                        }
                    }
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            ContentCommand::ClearOverlay { name, viewport_gen } => {
                if let Some(content) = self.current.as_mut() {
                    content.viewport_gen = viewport_gen;
                    if let Some((_, layout)) = content.html.as_mut() {
                        layout.clear_overlay(&name);
                        content.last_scene_sig = None;
                        render(content, &self.store, &self.registry, &self.policy, out);
                    }
                }
            }
        }
    }
}

/// Resolve an [`OverlayAnchor`] to a live page node id in the actor's HTML-lane
/// document. The host names the anchor by role (it cannot know the actor's node
/// ids); the actor maps that role to a concrete node with a box. v1 resolves
/// `Root` to the document element (`<html>`), whose fragment rect is the top of
/// the page. (Overlay-roots P1.)
#[cfg(not(target_arch = "wasm32"))]
fn resolve_overlay_anchor(doc: &StaticDocument, anchor: OverlayAnchor) -> Option<StaticNodeId> {
    match anchor {
        OverlayAnchor::Root => doc.document_element(),
    }
}
