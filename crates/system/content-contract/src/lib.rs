// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Transfer-ready content command/update messages plus the flat byte transport
//! used by browser workers.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use document_canvas::font_table::FontInterner;
use document_canvas::{DocumentRenderPacket, DocumentStyleSheet, FontTable};
use kernel::permissions::ResolvedPermission;
use kernel::types::{GraphScope, NodeProperty};
use linebender_resource_handle::Blob as ParleyBlob;
use linked_data::{EdgeContribution, GraphContribution, NodeContribution};
use netrender::{ImageKey, Scene, peniko};
use parley::FontData;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FetchedMessage {
    pub content_type: Option<String>,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentStateMessage {
    Loading,
    Ready(FetchedMessage),
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LinkHitMessage {
    pub rect: [f32; 4],
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextSelectionMessage {
    pub rects: Vec<[f32; 4]>,
    pub text: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentSceneStats {
    pub op_count: u64,
    pub encoded_bytes: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomNodeKindStatsMessage {
    pub documents: usize,
    pub document_fragments: usize,
    pub doctypes: usize,
    pub elements: usize,
    pub text: usize,
    pub comments: usize,
    pub processing_instructions: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomArenaStatsMessage {
    pub live_nodes: usize,
    pub node_kinds: DomNodeKindStatsMessage,
    pub attribute_count: usize,
    pub estimated_bytes: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayoutApplyKindMessage {
    #[default]
    Unchanged,
    RepaintOnly,
    Restyled,
    Spliced,
    FullRecompute,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayoutDamageClassMessage {
    #[default]
    None,
    PaintOnly,
    Relayout,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutBatchStatsMessage {
    pub applied: LayoutApplyKindMessage,
    pub damage: LayoutDamageClassMessage,
    pub mutations_in: usize,
    pub coalesced_invalidations: usize,
    pub restyled_elements: usize,
    pub boxes_rebuilt: usize,
    pub fragment_count: usize,
    pub box_tree_nodes: Option<usize>,
}

#[derive(Clone, Debug)]
pub enum ContentCommandMessage {
    Show {
        url: String,
        state: Option<ContentStateMessage>,
        engine: String,
        viewport: (u32, u32),
        nav: u64,
        viewport_gen: u64,
        sheet: DocumentStyleSheet,
    },
    Resize {
        viewport: (u32, u32),
        viewport_gen: u64,
    },
    Retheme {
        sheet: DocumentStyleSheet,
        viewport_gen: u64,
    },
    Resource {
        url: String,
        bytes: Vec<u8>,
    },
    /// Page Visibility / Page Lifecycle for the rendered card (W3C plan P1).
    SetLifecycle {
        hidden: bool,
        frozen: bool,
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
    FindActive {
        index: usize,
        viewport_gen: u64,
    },
    SelectText {
        anchor: (f32, f32),
        focus: (f32, f32),
        viewport_gen: u64,
    },
    AttachScript {
        component_path: PathBuf,
        log: ResolvedPermission,
        document: ResolvedPermission,
        net: ResolvedPermission,
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

#[derive(Clone, Debug)]
pub enum ContentUpdateMessage {
    Document {
        nav: u64,
        viewport_gen: u64,
        packet: DocumentRenderPacket,
        fonts: FontTable,
        content_height: u32,
    },
    Scene {
        nav: u64,
        viewport_gen: u64,
        scene: Scene,
        stats: ContentSceneStats,
        content_height: u32,
        band_y: u32,
        band_h: u32,
        links: Vec<LinkHitMessage>,
        masks: Vec<paint_list_render::BoxShadowMaskRequest>,
    },
    Wanted {
        nav: u64,
        urls: Vec<String>,
    },
    Contribution {
        contributions: Vec<GraphContribution>,
    },
    FindMatches {
        nav: u64,
        viewport_gen: u64,
        matches: Vec<Vec<[f32; 4]>>,
    },
    TextSelection {
        nav: u64,
        viewport_gen: u64,
        selection: Option<TextSelectionMessage>,
    },
    EngineStats {
        nav: u64,
        viewport_gen: u64,
        dom: DomArenaStatsMessage,
        layout: Option<LayoutBatchStatsMessage>,
    },
    ScriptOutcome {
        nav: u64,
        outcome: String,
    },
    TransportError {
        reason: String,
    },
}

#[derive(Debug)]
pub enum TransferError {
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
pub struct TransferBuffer {
    bytes: Vec<u8>,
}

impl TransferBuffer {
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    pub fn from_transport_error(reason: impl Into<String>) -> Result<Self, TransferError> {
        ContentUpdateWire::TransportError {
            reason: reason.into(),
        }
        .into_transfer_buffer()
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Move the encoded envelope into a JavaScript-owned `ArrayBuffer`. This
    /// performs the required wasm-memory -> JS-buffer copy; posting with the
    /// transfer list below then moves that JS buffer between browser agents.
    #[cfg(target_arch = "wasm32")]
    pub fn into_array_buffer(self) -> js_sys::ArrayBuffer {
        js_sys::Uint8Array::from(self.bytes.as_slice()).buffer()
    }

    #[cfg(target_arch = "wasm32")]
    pub fn from_array_buffer(buffer: js_sys::ArrayBuffer) -> Self {
        Self {
            bytes: js_sys::Uint8Array::new(&buffer).to_vec(),
        }
    }

    /// Worker-side update send: `postMessage(buffer, [buffer])`.
    #[cfg(target_arch = "wasm32")]
    pub fn post_to_worker_scope(
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
    pub fn post_to_worker(self, worker: &web_sys::Worker) -> Result<(), wasm_bindgen::JsValue> {
        let buffer = self.into_array_buffer();
        let transfer = js_sys::Array::new();
        transfer.push(&buffer);
        worker.post_message_with_transfer(&buffer, &transfer)
    }
}

/// Sender-side cache. Hold one per content worker so repeated frames send only
/// scene structure plus asset ids.
#[derive(Default)]
pub struct SceneTransferEncoder {
    fonts: HashSet<u64>,
    images: HashSet<ImageKey>,
}

/// Receiver-side cache. Hold one per content worker / tile.
#[derive(Default)]
pub struct SceneTransferDecoder {
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

impl ContentUpdateMessage {
    /// Encode this update into a single transferable byte buffer. The
    /// caller owns the encoder cache and should reuse it for one worker stream.
    pub fn into_transfer_buffer(
        self,
        encoder: &mut SceneTransferEncoder,
    ) -> Result<TransferBuffer, TransferError> {
        ContentUpdateWire::from_update(self, encoder).into_transfer_buffer()
    }

    /// Decode a transferred update. The caller owns the receiver cache and
    /// should reuse it for one worker stream.
    pub fn from_transfer_buffer(
        bytes: &[u8],
        decoder: &mut SceneTransferDecoder,
    ) -> Result<Self, TransferError> {
        ContentUpdateWire::from_transfer_buffer(bytes)?.into_update(decoder)
    }
}

impl ContentCommandMessage {
    pub fn into_transfer_buffer(self) -> Result<TransferBuffer, TransferError> {
        ContentCommandWire::from_command(self).into_transfer_buffer()
    }

    pub fn from_transfer_buffer(bytes: &[u8]) -> Result<Self, TransferError> {
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
    SetLifecycle {
        hidden: bool,
        frozen: bool,
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
    FindActive {
        index: usize,
        viewport_gen: u64,
    },
    SelectText {
        anchor: (f32, f32),
        focus: (f32, f32),
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
    fn from_command(command: ContentCommandMessage) -> Self {
        match command {
            ContentCommandMessage::Show {
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
                nav,
                viewport_gen,
                sheet,
            },
            ContentCommandMessage::Resize {
                viewport,
                viewport_gen,
            } => Self::Resize {
                viewport,
                viewport_gen,
            },
            ContentCommandMessage::Retheme {
                sheet,
                viewport_gen,
            } => Self::Retheme {
                sheet,
                viewport_gen,
            },
            ContentCommandMessage::Resource { url, bytes } => Self::Resource { url, bytes },
            ContentCommandMessage::SetLifecycle { hidden, frozen } => {
                Self::SetLifecycle { hidden, frozen }
            }
            ContentCommandMessage::Scroll {
                band_y,
                band_h,
                viewport_gen,
            } => Self::Scroll {
                band_y,
                band_h,
                viewport_gen,
            },
            ContentCommandMessage::Find {
                query,
                viewport_gen,
            } => Self::Find {
                query,
                viewport_gen,
            },
            ContentCommandMessage::FindActive {
                index,
                viewport_gen,
            } => Self::FindActive {
                index,
                viewport_gen,
            },
            ContentCommandMessage::SelectText {
                anchor,
                focus,
                viewport_gen,
            } => Self::SelectText {
                anchor,
                focus,
                viewport_gen,
            },
            ContentCommandMessage::AttachScript {
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
                viewport_gen,
            },
            ContentCommandMessage::DeliverEvent {
                kind,
                payload,
                viewport_gen,
            } => Self::DeliverEvent {
                kind,
                payload,
                viewport_gen,
            },
            ContentCommandMessage::DetachScript { viewport_gen } => {
                Self::DetachScript { viewport_gen }
            }
            ContentCommandMessage::MaterializeLinks { viewport_gen } => {
                Self::MaterializeLinks { viewport_gen }
            }
            #[cfg(feature = "scripted")]
            ContentCommandMessage::ScriptedClick { x, y, viewport_gen } => {
                Self::ScriptedClick { x, y, viewport_gen }
            }
        }
    }

    fn into_command(self) -> Result<ContentCommandMessage, TransferError> {
        Ok(match self {
            Self::Show {
                url,
                state,
                engine,
                viewport,
                nav,
                viewport_gen,
                sheet,
            } => ContentCommandMessage::Show {
                url,
                state: state.map(ContentStateWire::into_state),
                engine,
                viewport,
                nav,
                viewport_gen,
                sheet,
            },
            Self::Resize {
                viewport,
                viewport_gen,
            } => ContentCommandMessage::Resize {
                viewport,
                viewport_gen,
            },
            Self::Retheme {
                sheet,
                viewport_gen,
            } => ContentCommandMessage::Retheme {
                sheet,
                viewport_gen,
            },
            Self::Resource { url, bytes } => ContentCommandMessage::Resource { url, bytes },
            Self::SetLifecycle { hidden, frozen } => {
                ContentCommandMessage::SetLifecycle { hidden, frozen }
            }
            Self::Scroll {
                band_y,
                band_h,
                viewport_gen,
            } => ContentCommandMessage::Scroll {
                band_y,
                band_h,
                viewport_gen,
            },
            Self::Find {
                query,
                viewport_gen,
            } => ContentCommandMessage::Find {
                query,
                viewport_gen,
            },
            Self::FindActive {
                index,
                viewport_gen,
            } => ContentCommandMessage::FindActive {
                index,
                viewport_gen,
            },
            Self::SelectText {
                anchor,
                focus,
                viewport_gen,
            } => ContentCommandMessage::SelectText {
                anchor,
                focus,
                viewport_gen,
            },
            Self::AttachScript {
                component_path,
                log,
                document,
                net,
                viewport_gen,
            } => ContentCommandMessage::AttachScript {
                component_path,
                log,
                document,
                net,
                viewport_gen,
            },
            Self::DeliverEvent {
                kind,
                payload,
                viewport_gen,
            } => ContentCommandMessage::DeliverEvent {
                kind,
                payload,
                viewport_gen,
            },
            Self::DetachScript { viewport_gen } => {
                ContentCommandMessage::DetachScript { viewport_gen }
            }
            Self::MaterializeLinks { viewport_gen } => {
                ContentCommandMessage::MaterializeLinks { viewport_gen }
            }
            #[cfg(feature = "scripted")]
            Self::ScriptedClick { x, y, viewport_gen } => {
                ContentCommandMessage::ScriptedClick { x, y, viewport_gen }
            }
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

impl From<ContentStateMessage> for ContentStateWire {
    fn from(state: ContentStateMessage) -> Self {
        match state {
            ContentStateMessage::Loading => Self::Loading,
            ContentStateMessage::Ready(fetched) => Self::Ready(FetchedWire::from(fetched)),
            ContentStateMessage::Failed(reason) => Self::Failed(reason),
        }
    }
}

impl ContentStateWire {
    fn into_state(self) -> ContentStateMessage {
        match self {
            Self::Loading => ContentStateMessage::Loading,
            Self::Ready(fetched) => ContentStateMessage::Ready(fetched.into_fetched()),
            Self::Failed(reason) => ContentStateMessage::Failed(reason),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct FetchedWire {
    content_type: Option<String>,
    body: String,
}

impl From<FetchedMessage> for FetchedWire {
    fn from(fetched: FetchedMessage) -> Self {
        Self {
            content_type: fetched.content_type,
            body: fetched.body,
        }
    }
}

impl FetchedWire {
    fn into_fetched(self) -> FetchedMessage {
        FetchedMessage {
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
    TextSelection {
        nav: u64,
        viewport_gen: u64,
        selection: Option<TextSelectionMessage>,
    },
    EngineStats {
        nav: u64,
        viewport_gen: u64,
        dom: DomArenaStatsMessage,
        layout: Option<LayoutBatchStatsMessage>,
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
    fn from_update(update: ContentUpdateMessage, encoder: &mut SceneTransferEncoder) -> Self {
        match update {
            ContentUpdateMessage::Document {
                nav,
                viewport_gen,
                packet,
                fonts,
                content_height,
            } => Self::Document {
                nav,
                viewport_gen,
                packet,
                fonts: FontTableWire::from(fonts),
                content_height,
            },
            ContentUpdateMessage::Scene {
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
                nav,
                viewport_gen,
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
            ContentUpdateMessage::Wanted { nav, urls } => Self::Wanted { nav, urls },
            ContentUpdateMessage::Contribution { contributions } => Self::Contribution {
                contributions: contributions
                    .into_iter()
                    .map(GraphContributionWire::from)
                    .collect(),
            },
            ContentUpdateMessage::FindMatches {
                nav,
                viewport_gen,
                matches,
            } => Self::FindMatches {
                nav,
                viewport_gen,
                matches,
            },
            ContentUpdateMessage::TextSelection {
                nav,
                viewport_gen,
                selection,
            } => Self::TextSelection {
                nav,
                viewport_gen,
                selection,
            },
            ContentUpdateMessage::EngineStats {
                nav,
                viewport_gen,
                dom,
                layout,
            } => Self::EngineStats {
                nav,
                viewport_gen,
                dom,
                layout,
            },
            ContentUpdateMessage::ScriptOutcome { nav, outcome } => {
                Self::ScriptOutcome { nav, outcome }
            }
            ContentUpdateMessage::TransportError { reason } => Self::TransportError { reason },
        }
    }

    fn into_update(
        self,
        decoder: &mut SceneTransferDecoder,
    ) -> Result<ContentUpdateMessage, TransferError> {
        Ok(match self {
            Self::Document {
                nav,
                viewport_gen,
                packet,
                fonts,
                content_height,
            } => ContentUpdateMessage::Document {
                nav,
                viewport_gen,
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
            } => ContentUpdateMessage::Scene {
                nav,
                viewport_gen,
                scene: decoder.decode_scene(scene)?,
                stats,
                content_height,
                band_y,
                band_h,
                links: links.into_iter().map(LinkHitMessage::from).collect(),
                masks: masks
                    .into_iter()
                    .map(paint_list_render::BoxShadowMaskRequest::from)
                    .collect(),
            },
            Self::Wanted { nav, urls } => ContentUpdateMessage::Wanted { nav, urls },
            Self::Contribution { contributions } => ContentUpdateMessage::Contribution {
                contributions: contributions
                    .into_iter()
                    .map(GraphContribution::from)
                    .collect(),
            },
            Self::FindMatches {
                nav,
                viewport_gen,
                matches,
            } => ContentUpdateMessage::FindMatches {
                nav,
                viewport_gen,
                matches,
            },
            Self::TextSelection {
                nav,
                viewport_gen,
                selection,
            } => ContentUpdateMessage::TextSelection {
                nav,
                viewport_gen,
                selection,
            },
            Self::EngineStats {
                nav,
                viewport_gen,
                dom,
                layout,
            } => ContentUpdateMessage::EngineStats {
                nav,
                viewport_gen,
                dom,
                layout,
            },
            Self::ScriptOutcome { nav, outcome } => {
                ContentUpdateMessage::ScriptOutcome { nav, outcome }
            }
            Self::TransportError { reason } => ContentUpdateMessage::TransportError { reason },
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

pub fn scene_stats(scene: &Scene) -> ContentSceneStats {
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

impl From<LinkHitMessage> for LinkHitWire {
    fn from(hit: LinkHitMessage) -> Self {
        Self {
            rect: hit.rect,
            url: hit.url,
        }
    }
}

impl From<LinkHitWire> for LinkHitMessage {
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
    properties: Vec<NodePropertyWire>,
}

impl From<NodeContribution> for NodeContributionWire {
    fn from(node: NodeContribution) -> Self {
        Self {
            id: node.id,
            types: node.types,
            title: node.title,
            tags: node.tags,
            properties: node
                .properties
                .into_iter()
                .map(NodePropertyWire::from)
                .collect(),
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
            properties: node
                .properties
                .into_iter()
                .map(NodeProperty::from)
                .collect(),
        }
    }
}

/// A literal property's wire encoding (statement id / predicate / value / datatype /
/// lang / named-graph scope / provenance / assertion time), mirroring `NodeProperty`'s
/// full typed-literal + statement-metadata fidelity (petgraph-RDF plan, Phase 1). Kept
/// as its own struct rather than reusing `NodeProperty` directly, matching this file's
/// convention of decoupling the wire schema from the domain schema for every
/// contribution type it carries. `GraphScope` itself is reused as-is (already a plain
/// serde-friendly enum, like `ResolvedPermission` elsewhere in this file).
#[derive(Clone, Debug, Serialize, Deserialize)]
struct NodePropertyWire {
    statement_id: String,
    predicate: String,
    value: String,
    datatype: Option<String>,
    lang: Option<String>,
    graph_scope: GraphScope,
    provenance_iri: Option<String>,
    asserted_at_ms: Option<u64>,
}

impl From<NodeProperty> for NodePropertyWire {
    fn from(property: NodeProperty) -> Self {
        Self {
            statement_id: property.statement_id,
            predicate: property.predicate,
            value: property.value,
            datatype: property.datatype,
            lang: property.lang,
            graph_scope: property.graph_scope,
            provenance_iri: property.provenance_iri,
            asserted_at_ms: property.asserted_at_ms,
        }
    }
}

impl From<NodePropertyWire> for NodeProperty {
    fn from(property: NodePropertyWire) -> Self {
        Self {
            statement_id: property.statement_id,
            predicate: property.predicate,
            value: property.value,
            datatype: property.datatype,
            lang: property.lang,
            graph_scope: property.graph_scope,
            provenance_iri: property.provenance_iri,
            asserted_at_ms: property.asserted_at_ms,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct EdgeContributionWire {
    subject: String,
    predicate: String,
    object: String,
    graph_scope: GraphScope,
    #[serde(default)]
    statement_id: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    provenance_iri: Option<String>,
    #[serde(default)]
    asserted_at_ms: Option<u64>,
}

impl From<EdgeContribution> for EdgeContributionWire {
    fn from(edge: EdgeContribution) -> Self {
        Self {
            subject: edge.subject,
            predicate: edge.predicate,
            object: edge.object,
            graph_scope: edge.graph_scope,
            statement_id: edge.statement_id,
            label: edge.label,
            provenance_iri: edge.provenance_iri,
            asserted_at_ms: edge.asserted_at_ms,
        }
    }
}

impl From<EdgeContributionWire> for EdgeContribution {
    fn from(edge: EdgeContributionWire) -> Self {
        Self {
            subject: edge.subject,
            predicate: edge.predicate,
            object: edge.object,
            graph_scope: edge.graph_scope,
            statement_id: edge.statement_id,
            label: edge.label,
            provenance_iri: edge.provenance_iri,
            asserted_at_ms: edge.asserted_at_ms,
        }
    }
}
