/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Texture readback and data-URI helpers for render-owned preview imagery.

use image::ImageEncoder;
use std::path::PathBuf;
use std::sync::OnceLock;

/// The directory a headed-verify driver drops capture requests into, read once from
/// `MEERKAT_CAPTURE_DIR`. `None` (the default) makes [`maybe_dump_chrome_capture`] a
/// single cached-`None` check per frame — no per-frame env lookup, no cost when unset.
fn capture_dir() -> Option<&'static PathBuf> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| std::env::var_os("MEERKAT_CAPTURE_DIR").map(PathBuf::from))
        .as_ref()
}

/// Self-capture the chrome (shell document) texture straight off the GPU, bypassing the
/// OS compositor entirely — a driver script requesting a screenshot via
/// `SetForegroundWindow` + `CopyFromScreen` depends on this window actually being the
/// visually topmost surface, which a remote/virtual desktop session cannot guarantee (a
/// headed-verify session found the capture silently showing a *different* topmost window
/// instead, with no error). This reads the same already-rasterized texture the frame just
/// composited from (the pattern `read_texture_rgba` already proves out for the snapshot
/// card), so it needs no new render path and is exactly as correct as the shell document
/// itself. Polls for a `request.txt` (the target PNG path) in `MEERKAT_CAPTURE_DIR` each
/// frame — a single `Path::exists` stat when unset, so idle cost is negligible; only a
/// driver script that wrote a request pays the readback + encode cost. Captures the chrome
/// layer only (toolbar/shellbar/roster/outline/minimap-nodes/recent/list-panes/settings —
/// everything folded into the shell document); the orrery canvas and any
/// `<external-texture>` backdrop (the gloss minimap's edges/rings) composite separately
/// onto the swap-chain target and are not part of this texture.
pub(crate) fn maybe_dump_chrome_capture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    tex: &wgpu::Texture,
    w: u32,
    h: u32,
) {
    let Some(dir) = capture_dir() else {
        return;
    };
    let request = dir.join("request.txt");
    let Ok(target) = std::fs::read_to_string(&request) else {
        return;
    };
    let target = target.trim();
    if target.is_empty() {
        return;
    }
    let rgba = read_texture_rgba(device, queue, tex, w, h);
    if let Ok(file) = std::fs::File::create(target) {
        let _ = image::codecs::png::PngEncoder::new(file).write_image(
            &rgba,
            w,
            h,
            image::ExtendedColorType::Rgba8,
        );
    }
    let _ = std::fs::remove_file(&request);
}

/// The node's favicon RGBA (straight-alpha RGBA8) encoded as a `data:image/png;base64,`
/// URI, or `None` if it can't be encoded. The orrery card carries it as a leading
/// `<img>` that serval decodes. PNG (not BMP) keeps alpha and stays compact. (Phase 2.)
/// Also the sprite-import encoder (a downscaled dropped image to the face). (P2 sprite.)
pub(crate) fn favicon_data_uri(rgba: &[u8], w: u32, h: u32) -> Option<String> {
    if w == 0 || h == 0 || rgba.len() < (w as usize) * (h as usize) * 4 {
        return None;
    }
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(rgba, w, h, image::ExtendedColorType::Rgba8)
        .ok()?;
    Some(format!("data:image/png;base64,{}", base64_encode(&png)))
}

/// Read an `Rgba8Unorm` texture back to tightly-packed CPU RGBA bytes (width*height*4),
/// stripping wgpu's per-row alignment padding. Blocks until the copy and map complete,
/// so it is only used off the hot path.
pub(crate) fn read_texture_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let row_bytes = width * 4;
    let padded = row_bytes.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("snapshot readback"),
        size: padded as u64 * height as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("snapshot readback"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: target,
            mip_level: 0,
            origin: wgpu::Origin3d { x: 0, y: 0, z: 0 },
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    if device.poll(wgpu::PollType::wait_indefinitely()).is_err() {
        tracing::warn!("snapshot readback poll failed; skipping snapshot");
        return Vec::new();
    }
    if !matches!(rx.recv(), Ok(Ok(()))) {
        tracing::warn!("snapshot readback map failed; skipping snapshot");
        return Vec::new();
    }
    let mapped = slice.get_mapped_range();
    let mut out = Vec::with_capacity((row_bytes * height) as usize);
    for row in 0..height as usize {
        let src = row * padded as usize;
        out.extend_from_slice(&mapped[src..src + row_bytes as usize]);
    }
    out
}

/// Minimal standard base64 with no line breaks.
pub(crate) fn base64_encode(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = ((c[0] as u32) << 16)
            | ((c.get(1).copied().unwrap_or(0) as u32) << 8)
            | (c.get(2).copied().unwrap_or(0) as u32);
        out.push(A[((n >> 18) & 63) as usize] as char);
        out.push(A[((n >> 12) & 63) as usize] as char);
        out.push(if c.len() > 1 {
            A[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            A[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}
