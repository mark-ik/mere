/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Transfer-ready content update wire forms.
//!
//! Native actors still move rich Rust values over `mpsc`. This module is the
//! browser-worker seam: encode an update into one flat byte buffer suitable for
//! an `ArrayBuffer` transfer, while splitting recurring scene asset bytes into a
//! sender/receiver cache keyed by stable ids.

#![allow(dead_code)] // called by the Web Worker actor backend when that backend lands.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;

use document_canvas::font_table::FontInterner;
use document_canvas::{DocumentRenderPacket, FontTable};
use linebender_resource_handle::Blob as ParleyBlob;
use linked_data::{EdgeContribution, GraphContribution, NodeContribution};
use netrender::{ImageKey, Scene, peniko};
use parley::FontData;
use serde::{Deserialize, Serialize};

use crate::card::LinkHit;

use super::ContentUpdate;

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
    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.bytes
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
    ScriptOutcome {
        nav: u64,
        outcome: String,
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
                content_height,
                band_y,
                band_h,
                links,
                masks,
            } => Self::Scene {
                nav: nav.0,
                viewport_gen: viewport_gen.0,
                scene: encoder.encode_scene(&scene),
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
            ContentUpdate::ScriptOutcome { nav, outcome } => Self::ScriptOutcome {
                nav: nav.0,
                outcome,
            },
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
                content_height,
                band_y,
                band_h,
                links,
                masks,
            } => ContentUpdate::Scene {
                nav: armillary::NavGeneration(nav),
                viewport_gen: armillary::ViewportGeneration(viewport_gen),
                scene: decoder.decode_scene(scene)?,
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
            Self::ScriptOutcome { nav, outcome } => ContentUpdate::ScriptOutcome {
                nav: armillary::NavGeneration(nav),
                outcome,
            },
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
        let mut stripped = scene.clone();
        let mut fonts = Vec::new();
        for font in &mut stripped.fonts {
            let id = font.data.id();
            let bytes = font.data.data();
            if !bytes.is_empty() && self.fonts.insert(id) {
                fonts.push(FontAsset {
                    id,
                    bytes: bytes.to_vec(),
                });
            }
            font.data = empty_blob(id);
        }

        let mut images = Vec::new();
        for (&key, image) in &mut stripped.image_sources {
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
            image.data = empty_blob(blob_id);
        }

        TransferScene {
            scene_postcard: stripped.snapshot_postcard(),
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
    fn repeated_scene_transfer_omits_cached_font_bytes() {
        let update = |scene| ContentUpdate::Scene {
            nav: NavGeneration(1),
            viewport_gen: ViewportGeneration(2),
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
        let ContentUpdate::Scene { scene, .. } = decoded_second else {
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
    }

    #[test]
    fn repeated_scene_transfer_omits_cached_image_bytes() {
        let update = |scene| ContentUpdate::Scene {
            nav: NavGeneration(1),
            viewport_gen: ViewportGeneration(2),
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
        let ContentUpdate::Scene { scene, links, .. } = decoded else {
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
    }
}
