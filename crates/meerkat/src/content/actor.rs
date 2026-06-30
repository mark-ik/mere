/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The content actor build + spawn loop (relational-browse fetch/render driver).

use super::*;

/// Build the scripted-rung document for a `serval.scripted` node: parse the already-
/// fetched HTML body and run its scripts. With a `fetcher`, external `<script src>` is
/// fetched through it (`from_body`, no document re-fetch); without one, inline scripts
/// only (`parse`). `None` for any other engine or a non-`Ready` state. (Render ladder.)
#[cfg(feature = "scripted")]
pub(crate) fn build_scripted(
    engine: &str,
    url: &str,
    state: Option<&ContentState>,
    fetcher: Option<&dyn pelt_core::ResourceFetcher>,
) -> Option<ScriptedDocument<BoaEngine>> {
    if engine != inker::routing::ENGINE_SERVAL_SCRIPTED {
        return None;
    }
    let Some(ContentState::Ready(fetched)) = state else {
        return None;
    };
    // The node's origin cookies, so the page's JS shares the session HTTP uses.
    let cookies: Option<Box<dyn pelt_desktop::CookieProvider>> =
        url::Url::parse(url).ok().map(|parsed| {
            Box::new(JarCookieProvider { url: parsed }) as Box<dyn pelt_desktop::CookieProvider>
        });
    let result = match fetcher {
        Some(fetcher) => {
            ScriptedDocument::<BoaEngine>::from_body(&fetched.body, fetcher, url, cookies)
        }
        // No fetcher (the blocking fetch could not be built): inline-only, no cookies.
        None => ScriptedDocument::<BoaEngine>::parse(&fetched.body),
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
) -> (ActorHandle<ContentCommand>, ContentUpdateStream) {
    match transport {
        ContentUpdateTransport::Native => {
            let (handle, updates) = spawn_content(pool, wake, disabled, auto_ingest);
            (handle, ContentUpdateStream::Native(updates))
        }
        ContentUpdateTransport::Transfer => {
            let (handle, updates) = spawn_content_transfer(pool, wake, disabled, auto_ingest);
            (
                handle,
                ContentUpdateStream::Transfer {
                    updates,
                    decoder: SceneTransferDecoder::default(),
                },
            )
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

fn run_content<S: ContentUpdateSink>(
    commands: std::sync::mpsc::Receiver<ContentCommand>,
    out: &S,
    disabled: std::collections::HashSet<String>,
    auto_ingest: bool,
) {
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
    // The `net.fetch` backend, built lazily on first script attach (so unscripted
    // tiles never spin up a fetch runtime) and reused across attaches. (net loop.)
    let mut net_fetcher: Option<Arc<dyn NetFetcher>> = None;

    while let Ok(command) = commands.recv() {
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
                // Harvest the document's linked data once, on load (Ready only).
                // Off by default: auto-ingesting every page's embedded JSON-LD/RDFa
                // floods the graph with the page's own entities (e.g. a Wikipedia
                // article's structured data on each visit), so this is opt-in. The
                // host passes the flag; default false. (Linked-data ingest.)
                if auto_ingest {
                    if let Some(ContentState::Ready(fetched)) = &state {
                        let mut contributions = meerkat::ingest::harvest_contributions(
                            fetched.content_type.as_deref(),
                            &fetched.body,
                        );
                        // Render-free static-parse extraction (the extraction lane):
                        // enrich the page node with its own declared metadata
                        // (title / description / canonical / OpenGraph), through the
                        // same Contribution pipe. Fills the gap for the majority of
                        // pages with no JSON-LD. A scripted-rung node skips this: its
                        // post-JS extract (below, after the scripts run) supersedes
                        // the static shell. Only in a scripted build is there a post-JS
                        // path, so the base build always takes the static extract.
                        // (Render ladder phase 4.)
                        #[cfg(feature = "scripted")]
                        let is_scripted_rung = engine == inker::routing::ENGINE_SERVAL_SCRIPTED;
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
                // Build the scripted-rung document from the fetched body before the
                // state moves into `Content` (scripts run here). External `<script
                // src>` is fetched through a blocking `ScriptFetcher`; if that can't
                // be built, fall back to inline-only. No-op for a non-scripted
                // engine or in the base build.
                #[cfg(feature = "scripted")]
                let scripted_doc = {
                    let fetcher = ScriptFetcher::new();
                    build_scripted(
                        &engine,
                        &url,
                        state.as_ref(),
                        fetcher
                            .as_ref()
                            .map(|f| f as &dyn pelt_core::ResourceFetcher),
                    )
                };
                #[cfg(not(feature = "scripted"))]
                let _ = &engine;
                // Headless-scripted-DOM extract: the scripts have run, so extract the
                // post-JS DOM (an SPA's JS-rendered content) and contribute its
                // metadata through the same pipe — superseding the static shell the
                // harvest block skipped for this rung. (Render ladder phase 4.)
                #[cfg(feature = "scripted")]
                if auto_ingest {
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
                // A fresh navigation resets the band to the top, one viewport tall
                // (the host requests a taller scroll band once it has the content).
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
                    script: None,
                    #[cfg(feature = "scripted")]
                    scripted_doc,
                };
                // Run the outgoing scripted page's `deactivate` before it is
                // dropped: navigation is a teardown, and neither ScriptInstance
                // nor DocumentScript runs deactivate on a bare drop (lifecycle C1).
                if let Some(old) = current.as_mut().and_then(|c| c.script.take()) {
                    let _ = old.detach();
                }
                render(&mut content, &store, &registry, &policy, out);
                current = Some(content);
            }
            ContentCommand::Resize {
                viewport,
                viewport_gen,
            } => {
                if let Some(content) = current.as_mut() {
                    content.viewport = viewport;
                    content.viewport_gen = viewport_gen;
                    content.band_y = 0; // a resize relays out; re-anchor the band
                    content.html = None; // the viewport changed; rebuild the retained layout
                    // The scripted lane re-lays-out its own retained layout at the
                    // new viewport (the static lane rebuilds lazily in render). (#3.)
                    relayout_script(content, &store, out, viewport.0, viewport.1);
                    render(content, &store, &registry, &policy, out);
                }
            }
            ContentCommand::Retheme {
                sheet,
                viewport_gen,
            } => {
                if let Some(content) = current.as_mut() {
                    content.sheet = sheet;
                    // A bumped gen so the re-baked packet clears the generation gate.
                    content.viewport_gen = viewport_gen;
                    render(content, &store, &registry, &policy, out);
                }
            }
            ContentCommand::Resource { url, bytes } => {
                store.borrow_mut().insert(url, bytes);
                if let Some(content) = current.as_mut() {
                    content.html = None; // a subresource arrived; rebuild so images decode
                    // The scripted lane re-lays-out so the newly-arrived bytes decode
                    // into its retained layout too (same viewport). (#3.)
                    let (w, h) = content.viewport;
                    relayout_script(content, &store, out, w, h);
                    render(content, &store, &registry, &policy, out);
                }
            }
            ContentCommand::Scroll {
                band_y,
                band_h,
                viewport_gen,
            } => {
                if let Some(content) = current.as_mut() {
                    content.band_y = band_y;
                    content.band_h = band_h;
                    content.viewport_gen = viewport_gen;
                    render(content, &store, &registry, &policy, out);
                }
            }
            ContentCommand::Find {
                query,
                viewport_gen,
            } => {
                if let Some(content) = current.as_mut() {
                    // Find off the retained serval layout (no re-cascade per keystroke). A
                    // render runs before find in practice and builds the layout; if a find
                    // lands first, `ensure_html_layout` builds it. Non-HTML lanes find
                    // nothing here (their find rides the retained packet, a separate path).
                    let wanted = RefCell::new(Vec::new());
                    let matches =
                        if ensure_html_layout(content, &store, &registry, &policy, &wanted) {
                            let (doc, layout) = content.html.as_ref().expect("ensured");
                            layout.find(doc, &query)
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
            ContentCommand::AttachScript {
                component_path,
                log,
                document,
                net,
                viewport_gen,
            } => {
                if let Some(content) = current.as_mut() {
                    content.viewport_gen = viewport_gen;
                    // Map the host-resolved permissions to the link grant (§11.4).
                    let grant = script::grant_from_resolved(log, document, net);
                    // Build the `net.fetch` backend lazily on first attach (a
                    // current-thread tokio runtime), reused thereafter, and only
                    // when this grant actually allows net — a net-denied script
                    // never pays the runtime build. A build failure leaves `net`
                    // unbacked (a `net.fetch` call then errors). (net loop.)
                    if net_fetcher.is_none()
                        && matches!(grant.net, document_host::CapPermission::Allow)
                    {
                        net_fetcher = script::ContentNetFetcher::new()
                            .map(|f| Arc::new(f) as Arc<dyn NetFetcher>)
                            .map_err(|e| tracing::warn!(%e, "content net fetcher unavailable"))
                            .ok();
                    }
                    let outcome = attach_script(
                        content,
                        &component_path,
                        &grant,
                        net_fetcher.clone(),
                        &store,
                        &registry,
                        &policy,
                        out,
                    );
                    out.emit_update(ContentUpdate::ScriptOutcome {
                        nav: content.nav,
                        outcome,
                    });
                    // Render from the script's DOM once attached.
                    if content.script.is_some() {
                        render(content, &store, &registry, &policy, out);
                    }
                }
            }
            ContentCommand::DeliverEvent {
                kind,
                payload,
                viewport_gen,
            } => {
                if let Some(content) = current.as_mut() {
                    if content.script.is_some() {
                        content.viewport_gen = viewport_gen;
                        let outcome = deliver_event(content, &kind, &payload, &store, out);
                        out.emit_update(ContentUpdate::ScriptOutcome {
                            nav: content.nav,
                            outcome,
                        });
                        render(content, &store, &registry, &policy, out);
                    }
                }
            }
            ContentCommand::DetachScript { viewport_gen } => {
                if let Some(content) = current.as_mut() {
                    let outcome = match content.script.take() {
                        Some(inst) => match inst.detach() {
                            Ok(_) => "detached".to_string(),
                            Err(e) => format!("detach error: {e}"),
                        },
                        None => "no script attached".to_string(),
                    };
                    content.html = None; // revert to the static page path
                    content.viewport_gen = viewport_gen;
                    out.emit_update(ContentUpdate::ScriptOutcome {
                        nav: content.nav,
                        outcome,
                    });
                    render(content, &store, &registry, &policy, out);
                }
            }
            ContentCommand::MaterializeLinks { viewport_gen } => {
                if let Some(content) = current.as_mut() {
                    content.viewport_gen = viewport_gen;
                    // Render-free single-hop materialize: parse the already-fetched
                    // body for its outbound links and emit them as graph nodes +
                    // Hyperlink edges. (Scripted post-JS link materialization, off
                    // the live DOM, is a follow-on.)
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
            #[cfg(feature = "scripted")]
            ContentCommand::ScriptedClick { x, y, viewport_gen } => {
                if let Some(content) = current.as_mut() {
                    // Dispatch the click into the live document (listeners run, the
                    // DOM may mutate), then re-render off the mutated tree. Only the
                    // scripted rung holds a `scripted_doc`; other lanes no-op.
                    if let Some(doc) = content.scripted_doc.as_mut() {
                        doc.click_at(x, y);
                        content.viewport_gen = viewport_gen;
                        render(content, &store, &registry, &policy, out);
                    }
                }
            }
        }
    }
}
