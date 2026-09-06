/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Mere's document content lanes above Genet.
//!
//! The reader lane (held HTML through fleece into the portable document
//! canvas), the smolweb lane (protocol content through Nematic into the same
//! canvas, feature `smolweb`), their session engines, and the remote half of a
//! host's resource fetcher: http(s) over netfetcher (feature `netfetch`) and
//! the smolweb schemes over errand. None of it is web-observable engine
//! behaviour; all of it is application session and routing, which is why it
//! left `genet-documents` under the platform boundary plan (mere
//! `design_docs/mere_docs/implementation_strategy/`
//! `2026-09-02_platform_boundary_and_repository_topology_plan.md`, P1). It
//! lives in genet only until that plan moves it to Mere.

#[cfg(feature = "eidetic-bridge")]
pub mod eidetic_bridge;
pub mod reader;
mod remote;
#[cfg(feature = "smolweb")]
mod session;
pub mod smolweb;

#[cfg(feature = "fleece-json-ld")]
pub mod structured_data;

#[cfg(feature = "eidetic-bridge")]
pub use eidetic_bridge::{
    CaptureIdentity, ExternalWebResource, FLEECE_ANNOTATION_SCHEMA_REF, FleeceAnnotationRecord,
    FleeceExtractionRecord, WebAnnotationEnvelope, WebAnnotationTarget,
    bootstrap_fleece_annotation_schema, load_fleece_annotation, save_fleece_annotation,
};
#[cfg(feature = "fleece-json-ld")]
pub use structured_data::{
    JsonLdBlockProjection, JsonLdProjectionOutcome, json_ld_contributions, project_json_ld_blocks,
};

pub use genet_host_api::{ResourceFetchPolicy, ResourceFetcher};
pub use reader::{
    ReaderAccessibilityLink, ReaderAccessibilitySnapshot, ReaderDocumentSession,
    ReaderSessionEngine, lower_article,
};
pub use remote::RemoteFetcher;
#[cfg(feature = "smolweb")]
pub use session::{SmolwebDocumentSession, SmolwebSessionEngine};
pub use smolweb::{SmolwebDocument, SmolwebInlineMediaPolicy, SmolwebPalette, SmolwebTheme};
