/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Transfer-ready content update wire forms.
//!
//! Native actors still move rich Rust values over `mpsc`. This module is the
//! browser-worker seam: encode an update into one flat byte buffer suitable for
//! an `ArrayBuffer` transfer, while splitting recurring scene asset bytes into a
//! sender/receiver cache keyed by stable ids.

#![allow(dead_code)] // exercised by the browser Web Worker content transport.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;

use document_canvas::font_table::FontInterner;
use document_canvas::{DocumentRenderPacket, FontTable};
use engine_observables_api::{DomArenaStats, LayoutBatchStats};
use linebender_resource_handle::Blob as ParleyBlob;
use linked_data::{EdgeContribution, GraphContribution, NodeContribution};
use netrender::{ImageKey, Scene, peniko};
use parley::FontData;
use serde::{Deserialize, Serialize};

use crate::card::LinkHit;
use crate::fetch::Fetched;

use super::{ContentCommand, ContentSceneStats, ContentState, ContentUpdate};

#[derive(Debug)]
pub(crate) enum TransferError {
    Encode(postcard::Error),
    Decode(postcard::Error),
    SceneDecode(String),
    MissingFont { id: u64 },
    MissingImage { key: ImageKey },
}

impl fmt::Display for TransferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransferError::Encode(err) => write!(f, "content update encode: {err}"),
            TransferError::Decode(err) => write!(f, "content update decode: {err}"),
            TransferError::SceneDecode(err) => write!(f, "scene decode: {err}"),
            TransferError::MissingFont { id } => write!(f, "missing cached font asset {id}"),
            TransferError::MissingImage { key } => write!(f, "missing cached image asset {key}"),
        }
    }
}

impl std::error::Error for TransferError {}

/// One encoded payload the web actor backend can post as an ArrayBuffer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TransferBuffer {
    bytes: Vec<u8>,
}

impl TransferBuffer {
    pub(crate) fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    pub(crate) fn from_transport_error(reason: impl Into<String>) -> Result<Self, TransferError> {
        ContentUpdateWire::TransportError {
            reason: reason.into(),
        }
        .into_transfer_buffer()
    }

    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Move the encoded envelope into a JavaScript-owned `ArrayBuffer`. This
    /// performs the required wasm-memory -> JS-buffer copy; posting with the
    /// transfer list below then moves that JS buffer between browser agents.
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn into_array_buffer(self) -> js_sys::ArrayBuffer {
        js_sys::Uint8Array::from(self.bytes.as_slice()).buffer()
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn from_array_buffer(buffer: js_sys::ArrayBuffer) -> Self {
        Self {
            bytes: js_sys::Uint8Array::new(&buffer).to_vec(),
        }
    }

    /// Worker-side update send: `postMessage(buffer, [buffer])`.
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn post_to_worker_scope(
        self,
        scope: &web_sys::DedicatedWorkerGlobalScope,
    ) -> Result<(), wasm_bindgen::JsValue> {
        let buffer = self.into_array_buffer();
        let transfer = js_sys::Array::new();
        transfer.push(&buffer);
        scope.post_message_with_transfer(&buffer, &transfer)
    }

    /// Main-thread command/send side for the same transferable envelope shape.
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn post_to_worker(
        self,
        worker: &web_sys::Worker,
    ) -> Result<(), wasm_bindgen::JsValue> {
        let buffer = self.into_array_buffer();
        let transfer = js_sys::Array::new();
        transfer.push(&buffer);
        worker.post_message_with_transfer(&buffer, &transfer)
    }
}

/// Sender-side cache. Hold one per content worker so repeated frames send only
/// scene structure plus asset ids.
#[derive(Default)]
pub(crate) struct SceneTransferEncoder {
    fonts: HashSet<u64>,
    images: HashSet<ImageKey>,
}

/// Receiver-side cache. Hold one per content worker / tile.
#[derive(Default)]
pub(crate) struct SceneTransferDecoder {
    fonts: HashMap<u64, Vec<u8>>,
    images: HashMap<ImageKey, CachedImage>,
}

#[derive(Clone)]
struct CachedImage {
    width: u32,
    height: u32,
    blob_id: u64,
    bytes: Vec<u8>,
}

impl ContentUpdate {
    /// Encode this native update into a single transferable byte buffer. The
    /// caller owns the encoder cache and should reuse it for one worker stream.
    pub(crate) fn into_transfer_buffer(
        self,
        encoder: &mut SceneTransferEncoder,
    ) -> Result<TransferBuffer, TransferError> {
        ContentUpdateWire::from_update(self, encoder).into_transfer_buffer()
    }

    /// Decode a transferred update. The caller owns the receiver cache and
    /// should reuse it for one worker stream.
    pub(crate) fn from_transfer_buffer(
        bytes: &[u8],
        decoder: &mut SceneTransferDecoder,
    ) -> Result<Self, TransferError> {
        ContentUpdateWire::from_transfer_buffer(bytes)?.into_update(decoder)
    }
}

impl ContentCommand {
    pub(crate) fn into_transfer_buffer(self) -> Result<TransferBuffer, TransferError> {
        ContentCommandWire::from_command(self).into_transfer_buffer()
    }

    pub(crate) fn from_transfer_buffer(bytes: &[u8]) -> Result<Self, TransferError> {
        ContentCommandWire::from_transfer_buffer(bytes)?.into_command()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
enum ContentCommandWire {
    Show {
        url: String,
        state: Option<ContentStateWire>,
        engine: String,
        viewport: (u32, u32),
        nav: u64,
        viewport_gen: u64,
        sheet: document_canvas::DocumentStyleSheet,
    },
    Resize {
        viewport: (u32, u32),
        viewport_gen: u64,
    },
    Retheme {
        sheet: document_canvas::DocumentStyleSheet,
        viewport_gen: u64,
    },
    Resource {
        url: String,
        bytes: Vec<u8>,
    },
    Scroll {
        band_y: u32,
        band_h: u32,
        viewport_gen: u64,
    },
    Find {
        query: String,
        viewport_gen: u64,
    },
    AttachScript {
        component_path: std::path::PathBuf,
        log: kernel::permissions::ResolvedPermission,
        document: kernel::permissions::ResolvedPermission,
        net: kernel::permissions::ResolvedPermission,
        viewport_gen: u64,
    },
    DeliverEvent {
        kind: String,
        payload: String,
        viewport_gen: u64,
    },
    DetachScript {
        viewport_gen: u64,
    },
    MaterializeLinks {
        viewport_gen: u64,
    },
    #[cfg(feature = "scripted")]
    ScriptedClick {
        x: f32,
        y: f32,
        viewport_gen: u64,
    },
}

impl ContentCommandWire {
    fn from_command(command: ContentCommand) -> Self {
        match command {
            ContentCommand::Show {
                url,
                state,
                engine,
                viewport,
                nav,
                viewport_gen,
                sheet,
            } => Self::Show {
                url,
                state: state.map(ContentStateWire::from),
                engine,
                viewport,
                nav: nav.0,
                viewport_gen: viewport_gen.0,
                sheet,
            },
            ContentCommand::Resize {
                viewport,
                viewport_gen,
            } => Self::Resize {
                viewport,
                viewport_gen: viewport_gen.0,
            },
            ContentCommand::Retheme {
                sheet,
                viewport_gen,
            } => Self::Retheme {
                sheet,
                viewport_gen: viewport_gen.0,
            },
            ContentCommand::Resource { url, bytes } => Self::Resource { url, bytes },
            ContentCommand::Scroll {
                band_y,
                band_h,
                viewport_gen,
            } => Self::Scroll {
                band_y,
                band_h,
                viewport_gen: viewport_gen.0,
            },
            ContentCommand::Find {
                query,
                viewport_gen,
            } => Self::Find {
                query,
                viewport_gen: viewport_gen.0,
            },
            ContentCommand::AttachScript {
                component_path,
                log,
                document,
                net,
                viewport_gen,
            } => Self::AttachScript {
                component_path,
                log,
                document,
                net,
                viewport_gen: viewport_gen.0,
            },
            ContentCommand::DeliverEvent {
                kind,
                payload,
                viewport_gen,
            } => Self::DeliverEvent {
                kind,
                payload,
                viewport_gen: viewport_gen.0,
            },
            ContentCommand::DetachScript { viewport_gen } => Self::DetachScript {
                viewport_gen: viewport_gen.0,
            },
            ContentCommand::MaterializeLinks { viewport_gen } => Self::MaterializeLinks {
                viewport_gen: viewport_gen.0,
            },
            #[cfg(feature = "scripted")]
            ContentCommand::ScriptedClick { x, y, viewport_gen } => Self::ScriptedClick {
                x,
                y,
                viewport_gen: viewport_gen.0,
            },
        }
    }

    fn into_command(self) -> Result<ContentCommand, TransferError> {
        Ok(match self {
            Self::Show {
                url,
                state,
                engine,
                viewport,
                nav,
                viewport_gen,
                sheet,
            } => ContentCommand::Show {
                url,
                state: state.map(ContentStateWire::into_state),
                engine,
                viewport,
                nav: armillary::NavGeneration(nav),
                viewport_gen: armillary::ViewportGeneration(viewport_gen),
                sheet,
            },
            Self::Resize {
                viewport,
                viewport_gen,
            } => ContentCommand::Resize {
                viewport,
                viewport_gen: armillary::ViewportGeneration(viewport_gen),
            },
            Self::Retheme {
                sheet,
                viewport_gen,
            } => ContentCommand::Retheme {
                sheet,
                viewport_gen: armillary::ViewportGeneration(viewport_gen),
            },
            Self::Resource { url, bytes } => ContentCommand::Resource { url, bytes },
            Self::Scroll {
                band_y,
                band_h,
                viewport_gen,
            } => ContentCommand::Scroll {
                band_y,
                band_h,
                viewport_gen: armillary::ViewportGeneration(viewport_gen),
            },
            Self::Find {
                query,
                viewport_gen,
            } => ContentCommand::Find {
                query,
                viewport_gen: armillary::ViewportGeneration(viewport_gen),
            },
            Self::AttachScript {
                component_path,
                log,
                document,
                net,
                viewport_gen,
            } => ContentCommand::AttachScript {
                component_path,
                log,
                document,
                net,
                viewport_gen: armillary::ViewportGeneration(viewport_gen),
            },
            Self::DeliverEvent {
                kind,
                payload,
                viewport_gen,
            } => ContentCommand::DeliverEvent {
                kind,
                payload,
                viewport_gen: armillary::ViewportGeneration(viewport_gen),
            },
            Self::DetachScript { viewport_gen } => ContentCommand::DetachScript {
                viewport_gen: armillary::ViewportGeneration(viewport_gen),
            },
            Self::MaterializeLinks { viewport_gen } => ContentCommand::MaterializeLinks {
                viewport_gen: armillary::ViewportGeneration(viewport_gen),
            },
            #[cfg(feature = "scripted")]
            Self::ScriptedClick { x, y, viewport_gen } => ContentCommand::ScriptedClick {
                x,
                y,
                viewport_gen: armillary::ViewportGeneration(viewport_gen),
            },
        })
    }

    fn into_transfer_buffer(self) -> Result<TransferBuffer, TransferError> {
        postcard::to_allocvec(&self)
            .map(TransferBuffer::from_bytes)
            .map_err(TransferError::Encode)
    }

    fn from_transfer_buffer(bytes: &[u8]) -> Result<Self, TransferError> {
        postcard::from_bytes(bytes).map_err(TransferError::Decode)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
enum ContentStateWire {
    Loading,
    Ready(FetchedWire),
    Failed(String),
}

impl From<ContentState> for ContentStateWire {
    fn from(state: ContentState) -> Self {
        match state {
            ContentState::Loading => Self::Loading,
            ContentState::Ready(fetched) => Self::Ready(FetchedWire::from(fetched)),
            ContentState::Failed(reason) => Self::Failed(reason),
        }
    }
}

impl ContentStateWire {
    fn into_state(self) -> ContentState {
        match self {
            Self::Loading => ContentState::Loading,
            Self::Ready(fetched) => ContentState::Ready(fetched.into_fetched()),
            Self::Failed(reason) => ContentState::Failed(reason),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct FetchedWire {
    content_type: Option<String>,
    body: String,
}

impl From<Fetched> for FetchedWire {
    fn from(fetched: Fetched) -> Self {
        Self {
            content_type: fetched.content_type,
            body: fetched.body,
        }
    }
}

impl FetchedWire {
    fn into_fetched(self) -> Fetched {
        Fetched {
            content_type: self.content_type,
            body: self.body,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
enum ContentUpdateWire {
    Document {
        nav: u64,
        viewport_gen: u64,
        packet: DocumentRenderPacket,
        fonts: FontTableWire,
        content_height: u32,
    },
    Scene {
        nav: u64,
        viewport_gen: u64,
        scene: TransferScene,
        stats: ContentSceneStats,
        content_height: u32,
        band_y: u32,
        band_h: u32,
        links: Vec<LinkHitWire>,
        masks: Vec<BoxShadowMaskRequestWire>,
    },
    Wanted {
        nav: u64,
        urls: Vec<String>,
    },
    Contribution {
        contributions: Vec<GraphContributionWire>,
    },
    FindMatches {
        nav: u64,
        viewport_gen: u64,
        matches: Vec<Vec<[f32; 4]>>,
    },
    EngineStats {
        nav: u64,
        viewport_gen: u64,
        dom: DomArenaStats,
        layout: Option<LayoutBatchStats>,
    },
    ScriptOutcome {
        nav: u64,
        outcome: String,
    },
    TransportError {
        reason: String,
    },
}

impl ContentUpdateWire {
    fn from_update(update: ContentUpdate, encoder: &mut SceneTransferEncoder) -> Self {
        match update {
            ContentUpdate::Document {
                nav,
                viewport_gen,
                packet,
                fonts,
                content_height,
            } => Self::Document {
                nav: nav.0,
                viewport_gen: viewport_gen.0,
                packet,
                fonts: FontTableWire::from(fonts),
                content_height,
            },
            ContentUpdate::Scene {
                nav,
                viewport_gen,
                scene,
                stats,
                content_height,
                band_y,
                band_h,
                links,
                masks,
            } => Self::Scene {
                nav: nav.0,
                viewport_gen: viewport_gen.0,
                scene: encoder.encode_scene(&scene),
                stats,
                content_height,
                band_y,
                band_h,
                links: links.into_iter().map(LinkHitWire::from).collect(),
                masks: masks
                    .into_iter()
                    .map(BoxShadowMaskRequestWire::from)
                    .collect(),
            },
            ContentUpdate::Wanted { nav, urls } => Self::Wanted { nav: nav.0, urls },
            ContentUpdate::Contribution { contributions } => Self::Contribution {
                contributions: contributions
                    .into_iter()
                    .map(GraphContributionWire::from)
                    .collect(),
            },
            ContentUpdate::FindMatches {
                nav,
                viewport_gen,
                matches,
            } => Self::FindMatches {
                nav: nav.0,
                viewport_gen: viewport_gen.0,
                matches,
            },
            ContentUpdate::EngineStats {
                nav,
                viewport_gen,
                dom,
                layout,
            } => Self::EngineStats {
                nav: nav.0,
                viewport_gen: viewport_gen.0,
                dom,
                layout,
            },
            ContentUpdate::ScriptOutcome { nav, outcome } => Self::ScriptOutcome {
                nav: nav.0,
                outcome,
            },
            ContentUpdate::TransportError { reason } => Self::TransportError { reason },
        }
    }

    fn into_update(
        self,
        decoder: &mut SceneTransferDecoder,
    ) -> Result<ContentUpdate, TransferError> {
        Ok(match self {
            Self::Document {
                nav,
                viewport_gen,
                packet,
                fonts,
                content_height,
            } => ContentUpdate::Document {
                nav: armillary::NavGeneration(nav),
                viewport_gen: armillary::ViewportGeneration(viewport_gen),
                packet,
                fonts: FontTable::from(fonts),
                content_height,
            },
            Self::Scene {
                nav,
                viewport_gen,
                scene,
                stats,
                content_height,
                band_y,
                band_h,
                links,
                masks,
            } => ContentUpdate::Scene {
                nav: armillary::NavGeneration(nav),
                viewport_gen: armillary::ViewportGeneration(viewport_gen),
                scene: decoder.decode_scene(scene)?,
                stats,
                content_height,
                band_y,
                band_h,
                links: links.into_iter().map(LinkHit::from).collect(),
                masks: masks
                    .into_iter()
                    .map(paint_list_render::BoxShadowMaskRequest::from)
                    .collect(),
            },
            Self::Wanted { nav, urls } => ContentUpdate::Wanted {
                nav: armillary::NavGeneration(nav),
                urls,
            },
            Self::Contribution { contributions } => ContentUpdate::Contribution {
                contributions: contributions
                    .into_iter()
                    .map(GraphContribution::from)
                    .collect(),
            },
            Self::FindMatches {
                nav,
                viewport_gen,
                matches,
            } => ContentUpdate::FindMatches {
                nav: armillary::NavGeneration(nav),
                viewport_gen: armillary::ViewportGeneration(viewport_gen),
                matches,
            },
            Self::EngineStats {
                nav,
                viewport_gen,
                dom,
                layout,
            } => ContentUpdate::EngineStats {
                nav: armillary::NavGeneration(nav),
                viewport_gen: armillary::ViewportGeneration(viewport_gen),
                dom,
                layout,
            },
            Self::ScriptOutcome { nav, outcome } => ContentUpdate::ScriptOutcome {
                nav: armillary::NavGeneration(nav),
                outcome,
            },
            Self::TransportError { reason } => ContentUpdate::TransportError { reason },
        })
    }

    fn into_transfer_buffer(self) -> Result<TransferBuffer, TransferError> {
        postcard::to_allocvec(&self)
            .map(|bytes| TransferBuffer { bytes })
            .map_err(TransferError::Encode)
    }

    fn from_transfer_buffer(bytes: &[u8]) -> Result<Self, TransferError> {
        postcard::from_bytes(bytes).map_err(TransferError::Decode)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TransferScene {
    scene_postcard: Vec<u8>,
    fonts: Vec<FontAsset>,
    images: Vec<ImageAsset>,
}

struct PreparedSceneSnapshot {
    scene_postcard: Vec<u8>,
    stats: ContentSceneStats,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct FontAsset {
    id: u64,
    bytes: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ImageAsset {
    key: ImageKey,
    width: u32,
    height: u32,
    blob_id: u64,
    bytes: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct FontTableWire {
    faces: Vec<DocumentFontFaceWire>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DocumentFontFaceWire {
    blob_id: u64,
    index: u32,
    bytes: Vec<u8>,
}

impl From<FontTable> for FontTableWire {
    fn from(table: FontTable) -> Self {
        Self {
            faces: table
                .iter()
                .map(|(_, font)| DocumentFontFaceWire {
                    blob_id: font.data.id(),
                    index: font.index,
                    bytes: font.data.data().to_vec(),
                })
                .collect(),
        }
    }
}

impl From<FontTableWire> for FontTable {
    fn from(table: FontTableWire) -> Self {
        let mut interner = FontInterner::new();
        for face in table.faces {
            let font = FontData::new(parley_blob_from_bytes(face.bytes, face.blob_id), face.index);
            interner.intern(&font);
        }
        interner.into_table()
    }
}

impl SceneTransferEncoder {
    fn encode_scene(&mut self, scene: &Scene) -> TransferScene {
        let prepared = prepare_scene_snapshot(scene);
        let mut fonts = Vec::new();
        for font in &scene.fonts {
            let id = font.data.id();
            let bytes = font.data.data();
            if !bytes.is_empty() && self.fonts.insert(id) {
                fonts.push(FontAsset {
                    id,
                    bytes: bytes.to_vec(),
                });
            }
        }

        let mut images = Vec::new();
        for (&key, image) in &scene.image_sources {
            let blob_id = image.data.id();
            let bytes = image.data.data();
            if !bytes.is_empty() && self.images.insert(key) {
                images.push(ImageAsset {
                    key,
                    width: image.width,
                    height: image.height,
                    blob_id,
                    bytes: bytes.to_vec(),
                });
            }
        }

        TransferScene {
            scene_postcard: prepared.scene_postcard,
            fonts,
            images,
        }
    }
}

impl SceneTransferDecoder {
    fn decode_scene(&mut self, wire: TransferScene) -> Result<Scene, TransferError> {
        for font in wire.fonts {
            self.fonts.insert(font.id, font.bytes);
        }
        for image in wire.images {
            self.images.insert(
                image.key,
                CachedImage {
                    width: image.width,
                    height: image.height,
                    blob_id: image.blob_id,
                    bytes: image.bytes,
                },
            );
        }

        let mut scene = Scene::replay_postcard(&wire.scene_postcard)
            .map_err(|err| TransferError::SceneDecode(err.to_string()))?;
        for font in &mut scene.fonts {
            if font.data.data().is_empty() {
                let id = font.data.id();
                if let Some(bytes) = self.fonts.get(&id) {
                    font.data = blob_from_bytes(bytes.clone(), id);
                } else if id != u64::MAX {
                    return Err(TransferError::MissingFont { id });
                }
            }
        }
        for (&key, image) in &mut scene.image_sources {
            if image.data.data().is_empty() {
                let Some(cached) = self.images.get(&key) else {
                    return Err(TransferError::MissingImage { key });
                };
                image.width = cached.width;
                image.height = cached.height;
                image.data = blob_from_bytes(cached.bytes.clone(), cached.blob_id);
            }
        }
        Ok(scene)
    }
}

fn empty_blob(id: u64) -> peniko::Blob<u8> {
    blob_from_bytes(Vec::new(), id)
}

fn blob_from_bytes(bytes: Vec<u8>, id: u64) -> peniko::Blob<u8> {
    let arc: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(bytes);
    peniko::Blob::from_raw_parts(arc, id)
}

fn prepare_scene_snapshot(scene: &Scene) -> PreparedSceneSnapshot {
    let mut stripped = scene.clone();
    for font in &mut stripped.fonts {
        let id = font.data.id();
        font.data = empty_blob(id);
    }
    for image in stripped.image_sources.values_mut() {
        let blob_id = image.data.id();
        image.data = empty_blob(blob_id);
    }
    let scene_postcard = stripped.snapshot_postcard();
    PreparedSceneSnapshot {
        stats: ContentSceneStats {
            op_count: scene.ops.len() as u64,
            encoded_bytes: scene_postcard.len() as u64,
        },
        scene_postcard,
    }
}

pub(crate) fn scene_stats(scene: &Scene) -> ContentSceneStats {
    prepare_scene_snapshot(scene).stats
}

fn parley_blob_from_bytes(bytes: Vec<u8>, id: u64) -> ParleyBlob<u8> {
    let arc: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(bytes);
    ParleyBlob::from_raw_parts(arc, id)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct LinkHitWire {
    rect: [f32; 4],
    url: String,
}

impl From<LinkHit> for LinkHitWire {
    fn from(hit: LinkHit) -> Self {
        Self {
            rect: hit.rect,
            url: hit.url,
        }
    }
}

impl From<LinkHitWire> for LinkHit {
    fn from(hit: LinkHitWire) -> Self {
        Self {
            rect: hit.rect,
            url: hit.url,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct BoxShadowMaskRequestWire {
    key: ImageKey,
    dim: u32,
    bounds: [f32; 4],
    corner_radius: f32,
    blur_radius_px: f32,
    invert: bool,
}

impl From<paint_list_render::BoxShadowMaskRequest> for BoxShadowMaskRequestWire {
    fn from(mask: paint_list_render::BoxShadowMaskRequest) -> Self {
        Self {
            key: mask.key,
            dim: mask.dim,
            bounds: mask.bounds,
            corner_radius: mask.corner_radius,
            blur_radius_px: mask.blur_radius_px,
            invert: mask.invert,
        }
    }
}

impl From<BoxShadowMaskRequestWire> for paint_list_render::BoxShadowMaskRequest {
    fn from(mask: BoxShadowMaskRequestWire) -> Self {
        Self {
            key: mask.key,
            dim: mask.dim,
            bounds: mask.bounds,
            corner_radius: mask.corner_radius,
            blur_radius_px: mask.blur_radius_px,
            invert: mask.invert,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct GraphContributionWire {
    nodes: Vec<NodeContributionWire>,
    edges: Vec<EdgeContributionWire>,
}

impl From<GraphContribution> for GraphContributionWire {
    fn from(contribution: GraphContribution) -> Self {
        Self {
            nodes: contribution
                .nodes
                .into_iter()
                .map(NodeContributionWire::from)
                .collect(),
            edges: contribution
                .edges
                .into_iter()
                .map(EdgeContributionWire::from)
                .collect(),
        }
    }
}

impl From<GraphContributionWire> for GraphContribution {
    fn from(contribution: GraphContributionWire) -> Self {
        Self {
            nodes: contribution
                .nodes
                .into_iter()
                .map(NodeContribution::from)
                .collect(),
            edges: contribution
                .edges
                .into_iter()
                .map(EdgeContribution::from)
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct NodeContributionWire {
    id: String,
    types: Vec<String>,
    title: Option<String>,
    tags: Vec<String>,
    properties: Vec<(String, String)>,
}

impl From<NodeContribution> for NodeContributionWire {
    fn from(node: NodeContribution) -> Self {
        Self {
            id: node.id,
            types: node.types,
            title: node.title,
            tags: node.tags,
            properties: node.properties,
        }
    }
}

impl From<NodeContributionWire> for NodeContribution {
    fn from(node: NodeContributionWire) -> Self {
        Self {
            id: node.id,
            types: node.types,
            title: node.title,
            tags: node.tags,
            properties: node.properties,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct EdgeContributionWire {
    subject: String,
    predicate: String,
    object: String,
}

impl From<EdgeContribution> for EdgeContributionWire {
    fn from(edge: EdgeContribution) -> Self {
        Self {
            subject: edge.subject,
            predicate: edge.predicate,
            object: edge.object,
        }
    }
}

impl From<EdgeContributionWire> for EdgeContribution {
    fn from(edge: EdgeContributionWire) -> Self {
        Self {
            subject: edge.subject,
            predicate: edge.predicate,
            object: edge.object,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use armillary::{NavGeneration, ViewportGeneration};
    use engine_observables_api::{DomArenaStats, LayoutApplyKind, LayoutBatchStats};
    use netrender::{FontBlob, Glyph, ImageData, Scene};

    fn font_scene(font_bytes: Vec<u8>) -> Scene {
        let mut scene = Scene::new(320, 200);
        let font = scene.push_font(FontBlob {
            data: peniko::Blob::new(Arc::new(font_bytes)),
            index: 0,
        });
        scene.push_glyph_run(
            font,
            16.0,
            vec![Glyph {
                id: 42,
                x: 12.0,
                y: 40.0,
            }],
            [1.0, 1.0, 1.0, 1.0],
        );
        scene
    }

    fn image_scene(image_bytes: Vec<u8>) -> Scene {
        let mut scene = Scene::new(64, 64);
        scene.push_image(
            0.0,
            0.0,
            2.0,
            2.0,
            7,
            ImageData::from_bytes(1, 1, image_bytes),
        );
        scene
    }

    #[test]
    fn show_command_transfer_round_trips() {
        let command = ContentCommand::Show {
            url: "https://example.test/".to_string(),
            state: Some(ContentState::Ready(Fetched {
                content_type: Some("text/html".to_string()),
                body: "<p>hello</p>".to_string(),
            })),
            engine: inker::routing::ENGINE_SERVAL_WEB.to_string(),
            viewport: (320, 200),
            nav: NavGeneration(7),
            viewport_gen: ViewportGeneration(9),
            sheet: document_canvas::DocumentStyleSheet::default(),
        };

        let buffer = command.into_transfer_buffer().expect("encode show command");
        let decoded =
            ContentCommand::from_transfer_buffer(buffer.as_bytes()).expect("decode show command");

        let ContentCommand::Show {
            url,
            state,
            engine,
            viewport,
            nav,
            viewport_gen,
            ..
        } = decoded
        else {
            panic!("expected show command");
        };
        assert_eq!(url, "https://example.test/");
        assert_eq!(engine, inker::routing::ENGINE_SERVAL_WEB);
        assert_eq!(viewport, (320, 200));
        assert_eq!(nav, NavGeneration(7));
        assert_eq!(viewport_gen, ViewportGeneration(9));
        assert!(matches!(
            state,
            Some(ContentState::Ready(Fetched {
                content_type: Some(content_type),
                body,
            })) if content_type == "text/html" && body == "<p>hello</p>"
        ));
    }

    #[test]
    fn attach_script_command_transfer_round_trips() {
        let command = ContentCommand::AttachScript {
            component_path: std::path::PathBuf::from("mods/weather.wasm"),
            log: kernel::permissions::ResolvedPermission {
                effective: kernel::permissions::Permission::Allow,
                decided_by: Some(kernel::permissions::SettingScope::Surface),
            },
            document: kernel::permissions::ResolvedPermission {
                effective: kernel::permissions::Permission::Prompt,
                decided_by: Some(kernel::permissions::SettingScope::Graph),
            },
            net: kernel::permissions::ResolvedPermission {
                effective: kernel::permissions::Permission::Deny,
                decided_by: Some(kernel::permissions::SettingScope::App),
            },
            viewport_gen: ViewportGeneration(11),
        };

        let buffer = command
            .into_transfer_buffer()
            .expect("encode attach-script command");
        let decoded = ContentCommand::from_transfer_buffer(buffer.as_bytes())
            .expect("decode attach-script command");

        let ContentCommand::AttachScript {
            component_path,
            log,
            document,
            net,
            viewport_gen,
        } = decoded
        else {
            panic!("expected attach-script command");
        };
        assert_eq!(
            component_path,
            std::path::PathBuf::from("mods/weather.wasm")
        );
        assert_eq!(log.effective, kernel::permissions::Permission::Allow);
        assert_eq!(document.effective, kernel::permissions::Permission::Prompt);
        assert_eq!(net.effective, kernel::permissions::Permission::Deny);
        assert_eq!(viewport_gen, ViewportGeneration(11));
    }

    #[test]
    fn transport_error_transfer_round_trips() {
        let buffer =
            TransferBuffer::from_transport_error("encode failed").expect("encode error update");
        let mut decoder = SceneTransferDecoder::default();
        let decoded = ContentUpdate::from_transfer_buffer(buffer.as_bytes(), &mut decoder)
            .expect("decode error update");

        let ContentUpdate::TransportError { reason } = decoded else {
            panic!("expected transport error");
        };
        assert_eq!(reason, "encode failed");
    }

    #[test]
    fn engine_stats_transfer_round_trips() {
        let update = ContentUpdate::EngineStats {
            nav: NavGeneration(3),
            viewport_gen: ViewportGeneration(4),
            dom: DomArenaStats {
                live_nodes: 9,
                attribute_count: 5,
                estimated_bytes: 2048,
                ..DomArenaStats::default()
            },
            layout: Some(LayoutBatchStats {
                applied: LayoutApplyKind::Restyled,
                fragment_count: 17,
                ..LayoutBatchStats::default()
            }),
        };
        let mut encoder = SceneTransferEncoder::default();
        let mut decoder = SceneTransferDecoder::default();
        let buffer = update
            .into_transfer_buffer(&mut encoder)
            .expect("encode engine stats");
        let decoded = ContentUpdate::from_transfer_buffer(buffer.as_bytes(), &mut decoder)
            .expect("decode engine stats");

        let ContentUpdate::EngineStats {
            nav,
            viewport_gen,
            dom,
            layout,
        } = decoded
        else {
            panic!("expected engine stats");
        };
        assert_eq!(nav, NavGeneration(3));
        assert_eq!(viewport_gen, ViewportGeneration(4));
        assert_eq!(dom.live_nodes, 9);
        assert_eq!(dom.attribute_count, 5);
        assert_eq!(dom.estimated_bytes, 2048);
        assert_eq!(
            layout.expect("layout stats").applied,
            LayoutApplyKind::Restyled
        );
    }

    #[test]
    fn repeated_scene_transfer_omits_cached_font_bytes() {
        let update = |scene: Scene| ContentUpdate::Scene {
            nav: NavGeneration(1),
            viewport_gen: ViewportGeneration(2),
            stats: scene_stats(&scene),
            scene,
            content_height: 200,
            band_y: 0,
            band_h: 200,
            links: Vec::new(),
            masks: Vec::new(),
        };
        let mut encoder = SceneTransferEncoder::default();
        let mut decoder = SceneTransferDecoder::default();
        let scene = font_scene(vec![0xAB; 4096]);

        let first = update(scene.clone())
            .into_transfer_buffer(&mut encoder)
            .expect("encode first");
        let decoded_first = ContentUpdate::from_transfer_buffer(first.as_bytes(), &mut decoder)
            .expect("decode first");
        assert!(matches!(decoded_first, ContentUpdate::Scene { .. }));

        let second = update(scene)
            .into_transfer_buffer(&mut encoder)
            .expect("encode second");
        let decoded_second = ContentUpdate::from_transfer_buffer(second.as_bytes(), &mut decoder)
            .expect("decode second");
        let ContentUpdate::Scene { scene, stats, .. } = decoded_second else {
            panic!("expected scene update");
        };
        assert!(
            scene
                .fonts
                .iter()
                .any(|font| font.data.data().len() == 4096),
            "receiver rehydrates cached font bytes",
        );
        assert!(
            second.as_bytes().len() < first.as_bytes().len(),
            "second transfer should reuse the cached font asset",
        );
        assert_eq!(stats.op_count, scene.ops.len() as u64);
    }

    #[test]
    fn repeated_scene_transfer_omits_cached_image_bytes() {
        let update = |scene: Scene| ContentUpdate::Scene {
            nav: NavGeneration(1),
            viewport_gen: ViewportGeneration(2),
            stats: scene_stats(&scene),
            scene,
            content_height: 64,
            band_y: 0,
            band_h: 64,
            links: vec![LinkHit {
                rect: [0.0, 0.0, 10.0, 10.0],
                url: "https://example.test/".to_string(),
            }],
            masks: Vec::new(),
        };
        let mut encoder = SceneTransferEncoder::default();
        let mut decoder = SceneTransferDecoder::default();
        let scene = image_scene(vec![0xCC; 4096]);

        let first = update(scene.clone())
            .into_transfer_buffer(&mut encoder)
            .expect("encode first");
        let _ = ContentUpdate::from_transfer_buffer(first.as_bytes(), &mut decoder)
            .expect("decode first");
        let second = update(scene)
            .into_transfer_buffer(&mut encoder)
            .expect("encode second");
        let decoded = ContentUpdate::from_transfer_buffer(second.as_bytes(), &mut decoder)
            .expect("decode second");
        let ContentUpdate::Scene {
            scene,
            stats,
            links,
            ..
        } = decoded
        else {
            panic!("expected scene update");
        };
        assert_eq!(links.len(), 1);
        assert_eq!(
            scene.image_sources.get(&7).unwrap().data.data().len(),
            4096,
            "receiver rehydrates cached image bytes",
        );
        assert!(
            second.as_bytes().len() < first.as_bytes().len(),
            "second transfer should reuse the cached image asset",
        );
        assert_eq!(stats.op_count, scene.ops.len() as u64);
        assert!(stats.encoded_bytes > 0);
    }
}
