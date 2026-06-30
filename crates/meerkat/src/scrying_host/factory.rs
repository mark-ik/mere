/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Scrying surface factory and spawn glue.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use forme::GraphMemberId;
use inker::routing::{
    ENGINE_SCRYING_WEB, EngineRouteDecision, SurfaceContract, SurfaceContractMode, SurfaceTargetId,
};
use inker::{EngineProfileBinding, SurfaceEngineRegistry, SurfaceError, SurfaceSpawnRequest};
use scrying::{PlatformCompositionRoot, PlatformWebSurfaceConfig, PlatformWebSurfaceProducer};
use scrying_engine::{ProducerFactory, ScryingTileEngine};

use super::windows_pool::Tile;

pub(super) fn registry_for_root(root: Arc<PlatformCompositionRoot>) -> SurfaceEngineRegistry {
    let mut registry = SurfaceEngineRegistry::new();
    registry.register(Box::new(ScryingTileEngine::new(Arc::new(
        MeerkatScryFactory { root },
    ))));
    registry
}

/// Create the capture-only composition root on scrying's private off-screen
/// host window, not Meerkat's visible window.
pub(super) fn build_composition_root(
    window: &Arc<winit::window::Window>,
) -> Result<Arc<PlatformCompositionRoot>, String> {
    let inner = window.inner_size();
    PlatformCompositionRoot::new_offscreen(dpi::PhysicalSize::new(
        inner.width.max(1),
        inner.height.max(1),
    ))
    .map_err(|err| format!("composition root: {err}"))
}

pub(super) fn spawn(
    registry: &SurfaceEngineRegistry,
    member: GraphMemberId,
    url: &str,
    width: u32,
    height: u32,
    session_dir: &Path,
    fence_handle: Option<u64>,
    navigate_on_spawn: bool,
) -> Result<Tile, String> {
    let request = SurfaceSpawnRequest {
        url: url.to_string(),
        width,
        height,
        profile: EngineProfileBinding {
            user_data_dir: profile_dir(member, session_dir)
                .to_string_lossy()
                .into_owned(),
        },
        fence_handle,
    };
    let decision = EngineRouteDecision {
        engine_id: ENGINE_SCRYING_WEB.to_string(),
        surface_contract: SurfaceContract {
            target: SurfaceTargetId::new(format!("tile:{:032x}", member.as_u128())),
            mode: SurfaceContractMode::CompositedTexture,
        },
    };
    let mut tile = Tile {
        producer: registry
            .spawn(&decision, &request)
            .map_err(|err| err.to_string())?,
        texture_view: None,
        resource_epoch: None,
        shown_url: None,
        size: (width, height),
        last_error: None,
        flip: None,
    };
    if navigate_on_spawn {
        match tile.producer.as_web_surface() {
            Some(web) => match web.navigate_to_url(url) {
                Ok(()) => tile.shown_url = Some(url.to_string()),
                Err(err) => tile.last_error = Some(format!("navigate: {err}")),
            },
            None => tile.last_error = Some("navigate: surface has no web control".into()),
        }
    }
    Ok(tile)
}

fn profile_dir(member: GraphMemberId, session_dir: &Path) -> PathBuf {
    session_dir
        .join("scrying")
        .join(format!("pane-{:032x}", member.as_u128()))
}

struct MeerkatScryFactory {
    root: Arc<PlatformCompositionRoot>,
}

// The surface registry requires `Send + Sync` engines, but this factory is only
// driven by `Pool` on the winit UI thread. The `Arc` is retained here so the
// composition root outlives producer spawns; it is not used from worker threads.
#[allow(unsafe_code)]
unsafe impl Send for MeerkatScryFactory {}
#[allow(unsafe_code)]
unsafe impl Sync for MeerkatScryFactory {}

impl ProducerFactory for MeerkatScryFactory {
    fn build(
        &self,
        request: &SurfaceSpawnRequest,
    ) -> Result<Box<dyn scrying::WebSurfaceProducer>, SurfaceError> {
        let mut config = PlatformWebSurfaceConfig::new(
            dpi::PhysicalSize::new(request.width, request.height),
            PathBuf::from(&request.profile.user_data_dir),
        )
        .with_offset(0.0, 0.0);
        if let Some(handle) = request.fence_handle {
            config = config.with_fence_shared_handle(handle as *mut core::ffi::c_void);
        }
        #[allow(unsafe_code)]
        let producer = unsafe { PlatformWebSurfaceProducer::new_attached(&self.root, config) }
            .map_err(|err| SurfaceError::SpawnFailed(format!("WebView2 attach: {err}")))?;
        Ok(Box::new(producer))
    }
}
