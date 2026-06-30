/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Native frame import for WebView-backed scrying surfaces.

use forme::GraphMemberId;
use grafting::{
    Dx12SharedTexture, EpochCachedImporter, EpochFrame, ImportOptions, NativeFrame, SyncMechanism,
};
use inker::{NativeTextureHandle, SurfaceFrame, SurfaceSyncHandle};

use super::windows_pool::Tile;

pub(super) fn drive_frame(
    member: GraphMemberId,
    tile: &mut Tile,
    importer: &mut EpochCachedImporter,
) {
    match tile.producer.acquire_frame() {
        Ok(Some(frame)) => {
            if let Err(err) = import_surface_frame(member, tile, importer, frame) {
                tile.last_error = Some(err);
            } else {
                tile.last_error = None;
            }
        }
        Ok(None) => {}
        Err(err) => tile.last_error = Some(format!("acquire: {err}")),
    }
}

fn import_surface_frame(
    member: GraphMemberId,
    tile: &mut Tile,
    importer: &mut EpochCachedImporter,
    frame: SurfaceFrame,
) -> Result<(), String> {
    let resource_epoch = frame.resource_epoch;
    let has_new_handle = d3d12_shared_handle(&frame).is_some_and(|handle| handle != 0);
    let should_refresh_view =
        tile.resource_epoch != Some(resource_epoch) || tile.texture_view.is_none();

    let imported = if has_new_handle {
        let close_handle = d3d12_shared_handle(&frame).unwrap_or_default();
        let native = surface_frame_to_native(frame)?;
        let result = importer.update(
            EpochFrame::NewResource {
                resource_epoch,
                frame: &native,
            },
            &ImportOptions::default(),
        );
        if close_handle != 0 {
            #[allow(unsafe_code)]
            if let Err(err) =
                unsafe { grafting::close_shared_handle(close_handle as *mut core::ffi::c_void) }
            {
                tracing::warn!(%member, %err, "close_shared_handle failed");
            }
        }
        result.map_err(|err| format!("frame import: {err}"))?
    } else {
        importer
            .update(
                EpochFrame::ReusedResource { resource_epoch },
                &ImportOptions::default(),
            )
            .map_err(|err| format!("frame reuse: {err}"))?
    };

    if should_refresh_view {
        tile.texture_view = Some(
            imported
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default()),
        );
        tile.resource_epoch = Some(resource_epoch);
    }
    Ok(())
}

fn d3d12_shared_handle(frame: &SurfaceFrame) -> Option<u64> {
    match &frame.texture {
        NativeTextureHandle::D3d12Shared(handle) => Some(*handle),
        _ => None,
    }
}

fn surface_frame_to_native(frame: SurfaceFrame) -> Result<NativeFrame, String> {
    let handle = d3d12_shared_handle(&frame)
        .filter(|handle| *handle != 0)
        .ok_or_else(|| "new D3D12 resource frame carried no shared handle".to_string())?;
    let (producer_sync, fence_value) = match frame.sync {
        SurfaceSyncHandle::D3d12Fence { value, .. } if value > 0 => {
            (SyncMechanism::ExplicitFence, value)
        }
        _ => (SyncMechanism::None, 0),
    };
    Ok(NativeFrame::Dx12SharedTexture(Dx12SharedTexture {
        size: dpi::PhysicalSize::new(frame.width, frame.height),
        format: wgpu::TextureFormat::Bgra8Unorm,
        generation: frame.resource_epoch,
        producer_sync,
        fence_value,
        handle: handle as *mut core::ffi::c_void,
    }))
}
