/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The on-screen scripted document viewer (`pelt --engine scripted <url>`).
//!
//! The windowed half of the scripted profile: load a [`ScriptedDocument`] on the
//! chosen JS engine and present it through Pelt's public controller and shared
//! single-document shell. The controller retains document replacement and history;
//! the shell drives script timers and the GC tick at frame cadence. Gated on both
//! `present` (the present stack) and `scripted` (the runtime).

use script_engine_api::ScriptEngine;

use crate::scripted::ScriptedEngine;
use crate::scripted_graphics::WebGlTextureRegistry;
use crate::static_viewer::{
    ControllerViewerContent, ViewerClock, run_headed_with, validate_receipt_profile,
};
use crate::{StaticViewerConfig, StaticViewerOutcome, WindowingMode};
use genet_documents::{LocalFetcher, ResourceFetchPolicy};
use inker::{SessionRegistry, SurfaceEngineRegistry};
use netrender::Scene;
use pelt_core::{PeltController, PeltControllerConfig};

/// Run the scripted viewer for `config` on `engine`: headed opens a window and
/// presents the live, script-driven document; headless returns immediately with no
/// window (the CI smoke shape). The engine selects the monomorphization — Nova
/// requires the `scripted-nova` feature.
pub fn run_scripted_viewer(
    config: StaticViewerConfig,
    engine: ScriptedEngine,
) -> Result<StaticViewerOutcome, String> {
    validate_receipt_profile(&config, true)?;
    match config.profile.windowing {
        WindowingMode::Headless => Ok(StaticViewerOutcome {
            url: config.url,
            created_window: false,
            redraws: 0,
            size: (0, 0),
            product_receipt: None,
        }),
        WindowingMode::Headed => run_scripted_headed(config, engine),
    }
}

fn run_scripted_headed(
    config: StaticViewerConfig,
    engine: ScriptedEngine,
) -> Result<StaticViewerOutcome, String> {
    match engine {
        ScriptedEngine::Boa => {
            let content = ScriptedViewerContent::<script_engine_boa::BoaEngine>::new(
                &config,
                inker::routing::ENGINE_GENET_SCRIPTED,
                "Scripted · Boa",
            );
            run_headed_with(config, content)
        },
        #[cfg(feature = "scripted-nova")]
        ScriptedEngine::Nova => {
            let content = ScriptedViewerContent::<script_engine_nova::NovaEngine>::new(
                &config,
                inker::routing::ENGINE_GENET_SCRIPTED_NOVA,
                "Scripted · Nova",
            );
            run_headed_with(config, content)
        },
        #[cfg(not(feature = "scripted-nova"))]
        ScriptedEngine::Nova => Err(
            "the Nova engine needs `--features scripted-nova` (this build links Boa only)"
                .to_string(),
        ),
    }
}

struct ScriptedViewerContent<E> {
    address: String,
    size: (u32, u32),
    engine_id: &'static str,
    posture: String,
    controller: Option<ControllerViewerContent>,
    graphics: WebGlTextureRegistry,
    _engine: std::marker::PhantomData<fn() -> E>,
}

impl<E> ScriptedViewerContent<E> {
    fn new(config: &StaticViewerConfig, engine_id: &'static str, posture: &str) -> Self {
        Self {
            address: config.url.clone(),
            size: config.size.unwrap_or((800, 600)),
            engine_id,
            posture: posture.to_owned(),
            controller: None,
            graphics: WebGlTextureRegistry::default(),
            _engine: std::marker::PhantomData,
        }
    }

    fn controller(&self) -> &ControllerViewerContent {
        self.controller
            .as_ref()
            .expect("scripted viewer initialized before use")
    }
    fn controller_mut(&mut self) -> &mut ControllerViewerContent {
        self.controller
            .as_mut()
            .expect("scripted viewer initialized before use")
    }
}

impl<E: ScriptEngine + 'static> crate::static_viewer::windowed::ViewerContent
    for ScriptedViewerContent<E>
{
    fn initialize(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> Result<(), String> {
        #[cfg(feature = "netfetch")]
        let remote = mere_document_lanes::RemoteFetcher::new(ResourceFetchPolicy::default());
        #[cfg(feature = "netfetch")]
        let fetcher = LocalFetcher.with_fallback(remote.clone());
        #[cfg(not(feature = "netfetch"))]
        let fetcher = LocalFetcher;
        let graphics = self.graphics.clone();
        let mut registry: SessionRegistry<Scene> = SessionRegistry::new();
        registry.register(Box::new(
            genet_documents::ScriptedSessionEngine::<E, _>::new(self.engine_id, fetcher)
                .with_options_factory({
                    let device = device.clone();
                    let queue = queue.clone();
                    move |_address| {
                        Ok(genet_scripted::ScriptedDocumentOptions {
                            #[cfg(feature = "netfetch")]
                            fetch: Some(Box::new(remote.script_handler(_address))),
                            #[cfg(not(feature = "netfetch"))]
                            fetch: None,
                            webgl: Some(graphics.factory(device.clone(), queue.clone())),
                        })
                    }
                }),
        ));
        let controller = PeltController::new(
            registry,
            SurfaceEngineRegistry::new(),
            PeltControllerConfig::new(self.engine_id, &self.address, self.size),
            ViewerClock::new(),
        )?;
        self.controller = Some(ControllerViewerContent::new(
            controller,
            Some(self.posture.clone()),
        ));
        Ok(())
    }

    fn title(&self) -> Option<String> {
        self.controller
            .as_ref()
            .and_then(|v| crate::static_viewer::windowed::ViewerContent::title(v))
    }
    fn posture(&self) -> Option<&str> {
        Some(&self.posture)
    }
    fn address(&self) -> Option<&str> {
        self.controller
            .as_ref()
            .and_then(|v| crate::static_viewer::windowed::ViewerContent::address(v))
            .or(Some(&self.address))
    }
    fn drive_product_receipt(
        &mut self,
        receipt: crate::static_viewer::StaticProductReceipt,
    ) -> Result<String, String> {
        crate::static_viewer::windowed::ViewerContent::drive_product_receipt(
            self.controller_mut(),
            receipt,
        )
    }
    fn frame(&mut self, w: u32, h: u32) -> Scene {
        crate::static_viewer::windowed::ViewerContent::frame(self.controller_mut(), w, h)
    }
    fn external_texture(&self, key: u64) -> Option<wgpu::Texture> {
        self.graphics.texture(key)
    }
    fn external_texture_draws(&self) -> &[inker::SessionExternalTextureDraw] {
        self.controller().external_texture_draws()
    }
    fn scroll_by(&mut self, x: f32, y: f32) -> bool {
        crate::static_viewer::windowed::ViewerContent::scroll_by(self.controller_mut(), x, y)
    }
    fn scroll_at(&mut self, x: f32, y: f32, dx: f32, dy: f32) -> bool {
        crate::static_viewer::windowed::ViewerContent::scroll_at(
            self.controller_mut(),
            x,
            y,
            dx,
            dy,
        )
    }
    fn scroll_for_key(&mut self, k: crate::static_viewer::ViewerScrollKey) -> bool {
        crate::static_viewer::windowed::ViewerContent::scroll_for_key(self.controller_mut(), k)
    }
    fn input(&mut self, i: inker::SessionInput) -> crate::static_viewer::windowed::ViewerAction {
        crate::static_viewer::windowed::ViewerContent::input(self.controller_mut(), i)
    }
    fn navigation(
        &mut self,
        c: inker::SessionNavigationCommand,
    ) -> crate::static_viewer::windowed::ViewerAction {
        crate::static_viewer::windowed::ViewerContent::navigation(self.controller_mut(), c)
    }
    fn pump(&mut self, n: f64) -> bool {
        crate::static_viewer::windowed::ViewerContent::pump(self.controller_mut(), n)
    }
}

#[cfg(test)]
mod tests {
    use super::{ScriptedViewerContent, run_scripted_viewer};
    use crate::{ProductReceipt, ScriptedEngine, StaticViewerConfig, WindowingMode};
    use genet_host_api::EngineProfile;

    #[test]
    fn scripted_entrypoint_rejects_a_livery_receipt_before_windowing() {
        let config = StaticViewerConfig::new(
            EngineProfile::Scripted,
            WindowingMode::Headless,
            "about:blank",
        )
        .with_product_receipt(ProductReceipt::Article, "unused.png");
        assert_eq!(
            run_scripted_viewer(config, ScriptedEngine::Boa)
                .expect_err("livery receipt must not enter scripted"),
            "product receipt article is owned by the livery profile"
        );
    }

    #[cfg(feature = "netfetch")]
    #[test]
    fn deferred_viewer_fetches_and_mutates_through_its_real_host_services() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::thread;
        use std::time::{Duration, Instant};

        use crate::static_viewer::windowed::ViewerContent;

        const RESPONSE: &str = "Pelt deferred fetch receipt 6d7357";
        let core = genet_render_host::RenderCore::boot(netrender::NetrenderOptions {
            tile_cache_size: Some(16),
            enable_vello: true,
            ..Default::default()
        })
        .expect("receipt render core boots");

        let listener = TcpListener::bind("127.0.0.1:0").expect("local receipt server binds");
        listener
            .set_nonblocking(true)
            .expect("local receipt server becomes bounded");
        let address = listener.local_addr().expect("local receipt address");
        let server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut served = 0;
            while served < 2 && Instant::now() < deadline {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    },
                    Err(error) => panic!("local receipt accept failed: {error}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .expect("bounded request read");
                let mut request = [0; 2048];
                let count = stream.read(&mut request).expect("receipt request reads");
                let request = String::from_utf8_lossy(&request[..count]);
                let (content_type, body) = if request.starts_with("GET /data ") {
                    ("application/json", format!(r#"{{"value":"{RESPONSE}"}}"#))
                } else {
                    (
                        "text/html; charset=utf-8",
                        r#"<!doctype html><title>waiting</title><body>waiting<script>
                          fetch('/data').then(response => response.json()).then(data => {
                            document.title = data.value;
                            document.body.textContent = data.value;
                          });
                        </script>"#
                            .to_owned(),
                    )
                };
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .expect("receipt response writes");
                served += 1;
            }
            assert_eq!(
                served, 2,
                "document and authored fetch both reached the host"
            );
        });

        let url = format!("http://{address}/index.html");
        let config = StaticViewerConfig::new(EngineProfile::Scripted, WindowingMode::Headed, url)
            .with_size(160, 80);
        let mut content = ScriptedViewerContent::<script_engine_boa::BoaEngine>::new(
            &config,
            inker::routing::ENGINE_GENET_SCRIPTED,
            "Scripted · Boa",
        );
        content
            .initialize(core.device(), core.queue())
            .expect("deferred viewer initializes after device boot");

        let start = Instant::now();
        let deadline = start + Duration::from_secs(5);
        let mut pending = true;
        while (content.title().as_deref() != Some(RESPONSE) || pending) && Instant::now() < deadline
        {
            pending = content.pump(start.elapsed().as_secs_f64() * 1000.0);
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(content.title().as_deref(), Some(RESPONSE));
        assert!(!pending, "deferred fetch and its script jobs settle");
        assert!(!content.frame(160, 80).ops.is_empty());
        drop(content);
        server.join().expect("local receipt server exits cleanly");
    }
}
