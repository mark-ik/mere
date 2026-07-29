//! Portable graph-selection transfer between independent Graphshell stores.
//!
//! The graph selection is an immutable Eidetic engram. File bytes remain
//! separate Muniment blobs, addressed and verified by BLAKE3. Applying a
//! package is explicit about identity: replicate preserves container ids;
//! copy mints stable-per-transfer ids and records `CopiedFrom` provenance.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::LazyLock;

use chartulary::{AcceptAll, FacetId};
use eidetic::{
    BlobSource, Engram, Hash, ManifestId, MereNativeFieldSpec, MereNativeSchemaBuilder,
    ModerationState, PrivacyClass, ProvenanceOrigin, ProvenanceRecord, SchemaDefinition, SchemaRef,
    TimeBounds, Timestamp, TrustEnvelope, TrustLevel, TypedPayload, save_schema, save_typed,
    validate_payload,
};
use mere::kernel::geometry::PortablePoint;
use mere::kernel::graph::node_facets::{
    ARRANGEMENT_FRAME_LAYOUT, ARRANGEMENT_PIN, ARRANGEMENT_SPLIT_OFFER_SUPPRESSED,
    PROVENANCE_DERIVATIONS, PROVENANCE_IMPORT, VISIT_HISTORY,
};
use mere::kernel::graph::{Graph, NodeFacetStore};
use muniment::{Backend, BlobStore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::access::{
    ACCESS_HISTORY_FACET, AccessAction, AccessObservation, AccessRecord, AccessRecordFilter,
    AccessTransition, access_history, bootstrap_access_record_schema, query_access_records,
    record_observation, save_access_record,
};
use crate::mere_host::MereHost;
use crate::product::{
    CONTENT_FACET, ExportRequest, ProductEngramV1, ProductError, SavedSceneV1, decode_engram,
};

pub const TRANSFER_MANIFEST_SCHEMA: &str = "graphshell.transfer-manifest/v1";
pub const TRANSFER_CONTENT_FACET: &str = "graphshell.transfer-content/v1";

static PRODUCT_ENGRAM_SCHEMA_REF: LazyLock<SchemaRef> =
    LazyLock::new(|| schema_ref(&product_engram_schema()));
static TRANSFER_RECEIPT_SCHEMA_REF: LazyLock<SchemaRef> =
    LazyLock::new(|| schema_ref(&transfer_receipt_schema()));

fn schema_ref(definition: &SchemaDefinition) -> SchemaRef {
    let bytes = serde_json::to_vec(definition).expect("Graphshell schema always serializes");
    SchemaRef::from_id(ManifestId::from_hash(Hash::of(&bytes)))
}

pub(crate) fn product_engram_schema() -> SchemaDefinition {
    MereNativeSchemaBuilder::new("graphshell.GraphEngram/v1")
        .description("A closed graph or scene selection with portable facets.")
        .field("schema", MereNativeFieldSpec::String, true)
        .field(
            "scope",
            MereNativeFieldSpec::Enum {
                values: vec![
                    "object-only".to_string(),
                    "direct-relations".to_string(),
                    "selected-subgraph".to_string(),
                    "saved-scene".to_string(),
                ],
            },
            true,
        )
        .field("exported_at_ms", MereNativeFieldSpec::U64, true)
        .field("graph", MereNativeFieldSpec::Object, true)
        .field("facets", MereNativeFieldSpec::Object, true)
        .field("scene", MereNativeFieldSpec::Object, false)
        .build()
}

fn transfer_receipt_schema() -> SchemaDefinition {
    MereNativeSchemaBuilder::new("graphshell.TransferReceipt/v1")
        .description("The durable result of applying one graph selection transfer.")
        .field("transfer_id", MereNativeFieldSpec::String, true)
        .field(
            "operation",
            MereNativeFieldSpec::Enum {
                values: vec!["replicate".to_string(), "copy".to_string()],
            },
            true,
        )
        .field("source", MereNativeFieldSpec::Object, true)
        .field("destination", MereNativeFieldSpec::Object, true)
        .field("route", MereNativeFieldSpec::Object, true)
        .field("authorization_grant", MereNativeFieldSpec::String, true)
        .field("manifest_hash", MereNativeFieldSpec::String, true)
        .field("selection_hash", MereNativeFieldSpec::String, true)
        .field("blob_hashes", MereNativeFieldSpec::Array, true)
        .field("id_map", MereNativeFieldSpec::Array, true)
        .field("nodes", MereNativeFieldSpec::U64, true)
        .field("relations", MereNativeFieldSpec::U64, true)
        .field(
            "destination_access_records",
            MereNativeFieldSpec::Array,
            true,
        )
        .field("completed_at_ms", MereNativeFieldSpec::U64, true)
        .field(
            "result",
            MereNativeFieldSpec::Enum {
                values: vec!["completed".to_string()],
            },
            true,
        )
        .build()
}

impl TypedPayload for ProductEngramV1 {
    fn schema_ref() -> SchemaRef {
        *PRODUCT_ENGRAM_SCHEMA_REF
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransferOperation {
    Replicate,
    Copy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AccessTransferPolicy {
    ExcludeSourceHistory,
    IncludeSourceHistory,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferEndpointV1 {
    pub graph: String,
    pub persona: String,
    pub device: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferRouteV1 {
    pub carrier: String,
    pub peer: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferAuthorization {
    pub grant_id: String,
    pub revoked: bool,
}

#[derive(Clone, Debug)]
pub struct TransferRequest {
    pub transfer_id: Uuid,
    pub operation: TransferOperation,
    pub source: TransferEndpointV1,
    pub destination: TransferEndpointV1,
    pub route: TransferRouteV1,
    pub selection: ExportRequest,
    pub access_policy: AccessTransferPolicy,
    pub privacy: PrivacyClass,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferBlobInput {
    pub node_id: Uuid,
    pub role: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferBlobV1 {
    pub node_id: Uuid,
    pub role: String,
    pub media_type: String,
    pub content_hash: Hash,
    pub byte_len: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferManifestV1 {
    pub schema: String,
    pub transfer_id: Uuid,
    pub operation: TransferOperation,
    pub source: TransferEndpointV1,
    pub destination: TransferEndpointV1,
    pub route: TransferRouteV1,
    pub selection_schema: SchemaDefinition,
    pub selection: Engram,
    pub blobs: Vec<TransferBlobV1>,
    pub access_policy: AccessTransferPolicy,
    pub access_records: Vec<AccessRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferredIdV1 {
    pub source: Uuid,
    pub destination: Uuid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransferResult {
    Completed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferReceiptV1 {
    pub transfer_id: Uuid,
    pub operation: TransferOperation,
    pub source: TransferEndpointV1,
    pub destination: TransferEndpointV1,
    pub route: TransferRouteV1,
    pub authorization_grant: String,
    pub manifest_hash: Hash,
    pub selection_hash: Hash,
    pub blob_hashes: Vec<Hash>,
    pub id_map: Vec<TransferredIdV1>,
    pub nodes: u64,
    pub relations: u64,
    pub destination_access_records: Vec<Uuid>,
    pub completed_at_ms: u64,
    pub result: TransferResult,
}

impl TypedPayload for TransferReceiptV1 {
    fn schema_ref() -> SchemaRef {
        *TRANSFER_RECEIPT_SCHEMA_REF
    }
}

#[derive(Clone, Debug)]
pub struct ApplyTransferContext {
    pub authorization: TransferAuthorization,
    pub application: String,
    pub handler: String,
    pub completed_at_ms: u64,
    pub access_privacy: PrivacyClass,
}

#[derive(Debug)]
pub enum TransferError {
    Product(ProductError),
    Store(muniment::StoreError),
    Eidetic(eidetic::Error),
    InvalidManifest(String),
    MissingBlob(Hash),
    BlobHashMismatch(Hash),
    ContentMismatch { node_id: Uuid, reason: String },
    Revoked(String),
    PartialCopy,
}

impl std::fmt::Display for TransferError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Product(error) => write!(formatter, "{error}"),
            Self::Store(error) => write!(formatter, "{error}"),
            Self::Eidetic(error) => write!(formatter, "{error}"),
            Self::InvalidManifest(error) => write!(formatter, "invalid transfer manifest: {error}"),
            Self::MissingBlob(hash) => write!(formatter, "transfer blob {hash} is unavailable"),
            Self::BlobHashMismatch(hash) => {
                write!(formatter, "transfer blob {hash} failed its BLAKE3 check")
            }
            Self::ContentMismatch { node_id, reason } => {
                write!(formatter, "content for {node_id} is invalid: {reason}")
            }
            Self::Revoked(grant) => write!(formatter, "transfer grant {grant} is revoked"),
            Self::PartialCopy => formatter.write_str(
                "copy destination contains only part of this transfer's deterministic id set",
            ),
        }
    }
}

impl std::error::Error for TransferError {}

impl From<ProductError> for TransferError {
    fn from(value: ProductError) -> Self {
        Self::Product(value)
    }
}

impl From<muniment::StoreError> for TransferError {
    fn from(value: muniment::StoreError) -> Self {
        Self::Store(value)
    }
}

impl From<eidetic::Error> for TransferError {
    fn from(value: eidetic::Error) -> Self {
        Self::Eidetic(value)
    }
}

/// Build a carrier-ready manifest and stage its external blobs in the source
/// blob store. Local file locations are always removed from the selection.
pub async fn prepare_transfer<HB: Backend, AB: Backend, BB: Backend>(
    host: &MereHost<HB>,
    source_authority: &mut AB,
    source_blobs: &BlobStore<BB>,
    mut request: TransferRequest,
    blob_inputs: Vec<TransferBlobInput>,
) -> Result<TransferManifestV1, TransferError> {
    if host.selected_persona().persona != request.source.persona {
        return Err(TransferError::InvalidManifest(format!(
            "source persona {} is not the host's selected persona {}",
            request.source.persona,
            host.selected_persona().persona
        )));
    }
    if request.operation == TransferOperation::Replicate
        && request.source.persona != request.destination.persona
    {
        return Err(TransferError::InvalidManifest(
            "replicate requires one persona on both endpoints".to_string(),
        ));
    }
    request.selection.include_local_file_locations = false;
    let selection_bytes = host.export_product_engram(request.selection)?;
    let mut product = decode_engram(&selection_bytes)?;
    let selected_ids = selected_ids(&product)?;

    for node_id in &selected_ids {
        product
            .facets
            .remove(node_id, &FacetId::new(ACCESS_HISTORY_FACET));
    }

    let mut blobs = Vec::with_capacity(blob_inputs.len());
    let mut seen = HashSet::new();
    for input in blob_inputs {
        if !selected_ids.contains(&input.node_id) {
            return Err(TransferError::ContentMismatch {
                node_id: input.node_id,
                reason: "blob is outside the selected graph closure".to_string(),
            });
        }
        if !seen.insert((input.node_id, input.role.clone())) {
            return Err(TransferError::ContentMismatch {
                node_id: input.node_id,
                reason: format!("duplicate blob role {}", input.role),
            });
        }
        verify_content_facet(&product.facets, &input)?;
        let muniment_hash = source_blobs.put(&input.bytes).await?;
        let content_hash = Hash::of(&input.bytes);
        debug_assert_eq!(muniment_hash.to_hex(), content_hash.to_hex());
        blobs.push(TransferBlobV1 {
            node_id: input.node_id,
            role: input.role,
            media_type: input.media_type,
            content_hash,
            byte_len: input.bytes.len() as u64,
        });
    }
    require_selected_content_blobs(&product.facets, &selected_ids, &blobs)?;
    blobs.sort_by(|left, right| {
        (left.node_id, left.role.as_str()).cmp(&(right.node_id, right.role.as_str()))
    });

    let access_records = match request.access_policy {
        AccessTransferPolicy::ExcludeSourceHistory => Vec::new(),
        AccessTransferPolicy::IncludeSourceHistory => {
            query_access_records(source_authority, &AccessRecordFilter::default())
                .await?
                .into_iter()
                .filter(|record| selected_ids.contains(&record.container_id))
                .collect()
        }
    };

    let selection_payload = product.serialize_to_bytes()?;
    let definition = product_engram_schema();
    validate_payload(&definition, &selection_payload)?;
    let selection = Engram::new(
        ProductEngramV1::schema_ref(),
        selection_payload,
        request.privacy,
        ProvenanceRecord {
            origin: ProvenanceOrigin::Generated,
            upstream: Vec::new(),
            tooling: Some(format!("graphshell-transfer/{}", env!("CARGO_PKG_VERSION"))),
            generated_at: Timestamp(product.exported_at_ms),
        },
        TrustEnvelope {
            level: TrustLevel::SelfAsserted,
            signatures: Vec::new(),
            moderation_state: ModerationState::Unreviewed,
        },
        TimeBounds::at(Timestamp(product.exported_at_ms)),
    );

    Ok(TransferManifestV1 {
        schema: TRANSFER_MANIFEST_SCHEMA.to_string(),
        transfer_id: request.transfer_id,
        operation: request.operation,
        source: request.source,
        destination: request.destination,
        route: request.route,
        selection_schema: definition,
        selection,
        blobs,
        access_policy: request.access_policy,
        access_records,
    })
}

/// Apply a verified package to another host and store. The authorization check
/// is deliberately first, so a revoked transfer cannot fetch bytes or mutate
/// destination truth.
pub async fn apply_transfer<HB, SB, DB, AB>(
    host: &mut MereHost<HB>,
    source_blobs: &BlobStore<SB>,
    destination_blobs: &BlobStore<DB>,
    destination_authority: &mut AB,
    manifest: &TransferManifestV1,
    context: &ApplyTransferContext,
) -> Result<TransferReceiptV1, TransferError>
where
    HB: Backend,
    SB: Backend,
    DB: Backend,
    AB: Backend,
{
    if context.authorization.revoked {
        return Err(TransferError::Revoked(
            context.authorization.grant_id.clone(),
        ));
    }
    if host.selected_persona().persona != manifest.destination.persona {
        return Err(TransferError::InvalidManifest(format!(
            "destination persona {} is not the host's selected persona {}",
            manifest.destination.persona,
            host.selected_persona().persona
        )));
    }
    let product = verify_manifest(manifest)?;
    let source_addresses: HashMap<Uuid, String> = product
        .graph
        .nodes
        .iter()
        .filter_map(|node| {
            let id = Uuid::parse_str(&node.node_id).ok()?;
            let from_address = node.address.as_url_str();
            Some((
                id,
                if from_address.is_empty() {
                    node.url.clone()
                } else {
                    from_address.to_string()
                },
            ))
        })
        .collect();

    for descriptor in &manifest.blobs {
        let hash = muniment_hash(descriptor.content_hash)?;
        if let Some(existing) = destination_blobs.get(&hash).await? {
            if Hash::of(&existing) != descriptor.content_hash
                || existing.len() as u64 != descriptor.byte_len
            {
                return Err(TransferError::BlobHashMismatch(descriptor.content_hash));
            }
            continue;
        }
        let bytes = source_blobs
            .get(&hash)
            .await?
            .ok_or(TransferError::MissingBlob(descriptor.content_hash))?;
        if Hash::of(&bytes) != descriptor.content_hash || bytes.len() as u64 != descriptor.byte_len
        {
            return Err(TransferError::BlobHashMismatch(descriptor.content_hash));
        }
        let stored = destination_blobs.put(&bytes).await?;
        if stored.to_hex() != descriptor.content_hash.to_hex() {
            return Err(TransferError::BlobHashMismatch(descriptor.content_hash));
        }
    }

    let (target_product, id_map) = match manifest.operation {
        TransferOperation::Replicate => {
            let mut source_ids: Vec<_> = selected_ids(&product)?.into_iter().collect();
            source_ids.sort_unstable();
            let ids = source_ids
                .into_iter()
                .map(|id| TransferredIdV1 {
                    source: id,
                    destination: id,
                })
                .collect();
            (product, ids)
        }
        TransferOperation::Copy => copied_product(&product, manifest)?,
    };

    let existing = id_map
        .iter()
        .filter(|mapping| host.graph().get_node_by_id(mapping.destination).is_some())
        .count();
    if manifest.operation == TransferOperation::Copy && existing > 0 && existing < id_map.len() {
        return Err(TransferError::PartialCopy);
    }
    if existing != id_map.len() {
        let bytes = target_product.serialize_to_bytes()?;
        host.import_product_engram(&bytes)?;
    }

    attach_transfer_content(host, manifest, &id_map)?;

    bootstrap_access_record_schema(destination_authority).await?;
    for record in &manifest.access_records {
        save_access_record(destination_authority, record).await?;
        if manifest.operation == TransferOperation::Replicate {
            attach_existing_access_record(host, record)?;
        }
    }

    let mut destination_access_records = Vec::with_capacity(id_map.len());
    for mapping in &id_map {
        let key = host
            .graph()
            .get_node_key_by_id(mapping.destination)
            .ok_or_else(|| {
                TransferError::InvalidManifest(format!(
                    "destination object {} was not imported",
                    mapping.destination
                ))
            })?;
        let source_address = source_addresses.get(&mapping.source).cloned();
        let record_id = Uuid::new_v5(
            &manifest.transfer_id,
            format!("import:{}:{}", mapping.source, mapping.destination).as_bytes(),
        );
        let observation = AccessObservation {
            record_id,
            action: AccessAction::Import,
            persona: manifest.destination.persona.clone(),
            device: manifest.destination.device.clone(),
            application: context.application.clone(),
            handler: context.handler.clone(),
            at_ms: context.completed_at_ms,
            dwell_ms: None,
            referring_container_id: Some(mapping.source),
            referring_address: source_address,
            transition: AccessTransition::Imported,
            capture_source: "graphshell.transfer".to_string(),
            source_event_id: Some(manifest.transfer_id.to_string()),
            privacy: context.access_privacy,
        };
        let record = host
            .mutate_product_graph(|graph| {
                let (candidate, inserted) = record_observation(graph, key, &observation)?;
                if inserted {
                    return Ok(candidate);
                }
                access_history(graph, key)?
                    .records
                    .into_iter()
                    .find(|existing| existing.record_id == record_id)
                    .ok_or(crate::access::AccessError::UnknownNode)
            })
            .map_err(|error| TransferError::InvalidManifest(error.to_string()))?;
        save_access_record(destination_authority, &record).await?;
        destination_access_records.push(record.record_id);
    }

    host.persist(context.completed_at_ms / 1_000)
        .await
        .map_err(|error| TransferError::InvalidManifest(error.to_string()))?;

    let selection_schema_id = save_schema(
        destination_authority,
        &manifest.selection_schema,
        PrivacyClass::PublicPortable,
        ProvenanceRecord {
            origin: ProvenanceOrigin::Imported {
                source: manifest.source.graph.clone(),
            },
            upstream: Vec::new(),
            tooling: Some(format!("graphshell-transfer/{}", env!("CARGO_PKG_VERSION"))),
            generated_at: Timestamp(context.completed_at_ms),
        },
        TrustEnvelope {
            level: TrustLevel::SelfAsserted,
            signatures: Vec::new(),
            moderation_state: ModerationState::Unreviewed,
        },
        Timestamp(context.completed_at_ms),
    )
    .await?;
    if selection_schema_id != manifest.selection.schema.0 {
        return Err(TransferError::InvalidManifest(
            "selection schema id changed while saving".to_string(),
        ));
    }
    bootstrap_transfer_receipt_schema(destination_authority).await?;

    let manifest_bytes = serde_json::to_vec(manifest)
        .map_err(|error| TransferError::InvalidManifest(error.to_string()))?;
    let receipt = TransferReceiptV1 {
        transfer_id: manifest.transfer_id,
        operation: manifest.operation,
        source: manifest.source.clone(),
        destination: manifest.destination.clone(),
        route: manifest.route.clone(),
        authorization_grant: context.authorization.grant_id.clone(),
        manifest_hash: Hash::of(&manifest_bytes),
        selection_hash: manifest.selection.content_hash,
        blob_hashes: manifest
            .blobs
            .iter()
            .map(|blob| blob.content_hash)
            .collect(),
        id_map,
        nodes: target_product.graph.nodes.len() as u64,
        relations: target_product.graph.edges.len() as u64,
        destination_access_records,
        completed_at_ms: context.completed_at_ms,
        result: TransferResult::Completed,
    };
    save_transfer_receipt(destination_authority, &receipt).await?;
    Ok(receipt)
}

async fn bootstrap_transfer_receipt_schema<B: Backend>(
    store: &mut B,
) -> Result<(), eidetic::Error> {
    if eidetic::manifest::load_manifest(store, TRANSFER_RECEIPT_SCHEMA_REF.0)
        .await?
        .is_some()
    {
        return Ok(());
    }
    let id = save_schema(
        store,
        &transfer_receipt_schema(),
        PrivacyClass::PublicPortable,
        ProvenanceRecord {
            origin: ProvenanceOrigin::Generated,
            upstream: Vec::new(),
            tooling: Some(format!("graphshell-transfer/{}", env!("CARGO_PKG_VERSION"))),
            generated_at: Timestamp::ZERO,
        },
        TrustEnvelope {
            level: TrustLevel::CheckpointAccepted,
            signatures: Vec::new(),
            moderation_state: ModerationState::Accepted,
        },
        Timestamp::ZERO,
    )
    .await?;
    debug_assert_eq!(id, TRANSFER_RECEIPT_SCHEMA_REF.0);
    Ok(())
}

async fn save_transfer_receipt<B: Backend>(
    store: &mut B,
    receipt: &TransferReceiptV1,
) -> Result<ManifestId, eidetic::Error> {
    save_typed(
        store,
        receipt,
        Vec::<BlobSource>::new(),
        PrivacyClass::LocalOnly,
        ProvenanceRecord {
            origin: ProvenanceOrigin::Generated,
            upstream: vec![ManifestId::from_hash(receipt.selection_hash)],
            tooling: Some(format!("graphshell-transfer/{}", env!("CARGO_PKG_VERSION"))),
            generated_at: Timestamp(receipt.completed_at_ms),
        },
        TrustEnvelope {
            level: TrustLevel::SelfAsserted,
            signatures: Vec::new(),
            moderation_state: ModerationState::Unreviewed,
        },
        Timestamp(receipt.completed_at_ms),
    )
    .await
}

pub(crate) fn verify_manifest(
    manifest: &TransferManifestV1,
) -> Result<ProductEngramV1, TransferError> {
    if manifest.schema != TRANSFER_MANIFEST_SCHEMA {
        return Err(TransferError::InvalidManifest(format!(
            "expected {TRANSFER_MANIFEST_SCHEMA}, found {}",
            manifest.schema
        )));
    }
    manifest.selection.verify_integrity()?;
    let actual_schema_ref = schema_ref(&manifest.selection_schema);
    if actual_schema_ref != manifest.selection.schema
        || manifest.selection.schema != ProductEngramV1::schema_ref()
    {
        return Err(TransferError::InvalidManifest(
            "selection schema reference does not match its definition".to_string(),
        ));
    }
    validate_payload(&manifest.selection_schema, &manifest.selection.payload)?;
    let product = decode_engram(&manifest.selection.payload)?;
    let selected = selected_ids(&product)?;
    if manifest.operation == TransferOperation::Replicate
        && manifest.source.persona != manifest.destination.persona
    {
        return Err(TransferError::InvalidManifest(
            "replicate requires one persona on both endpoints".to_string(),
        ));
    }
    let mut seen = HashSet::new();
    for blob in &manifest.blobs {
        if !selected.contains(&blob.node_id) {
            return Err(TransferError::InvalidManifest(format!(
                "blob {} names an object outside the selection",
                blob.content_hash
            )));
        }
        if !seen.insert((blob.node_id, blob.role.as_str())) {
            return Err(TransferError::InvalidManifest(format!(
                "duplicate blob role {} for {}",
                blob.role, blob.node_id
            )));
        }
    }
    let mut access_ids = HashSet::new();
    if manifest.access_policy == AccessTransferPolicy::ExcludeSourceHistory
        && !manifest.access_records.is_empty()
    {
        return Err(TransferError::InvalidManifest(
            "source access records are present under the exclusion policy".to_string(),
        ));
    }
    for record in &manifest.access_records {
        if !selected.contains(&record.container_id) {
            return Err(TransferError::InvalidManifest(format!(
                "access record {} names an object outside the selection",
                record.record_id
            )));
        }
        if !access_ids.insert(record.record_id) {
            return Err(TransferError::InvalidManifest(format!(
                "duplicate access record id {}",
                record.record_id
            )));
        }
    }
    require_selected_content_blobs(&product.facets, &selected, &manifest.blobs)?;
    Ok(product)
}

fn selected_ids(product: &ProductEngramV1) -> Result<HashSet<Uuid>, TransferError> {
    product
        .graph
        .nodes
        .iter()
        .map(|node| {
            Uuid::parse_str(&node.node_id).map_err(|error| {
                TransferError::InvalidManifest(format!(
                    "object id {} is not a UUID: {error}",
                    node.node_id
                ))
            })
        })
        .collect()
}

fn verify_content_facet(
    facets: &NodeFacetStore,
    input: &TransferBlobInput,
) -> Result<(), TransferError> {
    let Some(content) = facets.get(&input.node_id, &FacetId::new(CONTENT_FACET)) else {
        return Ok(());
    };
    if let Some(expected) = content.get("byte_len").and_then(serde_json::Value::as_u64)
        && expected != input.bytes.len() as u64
    {
        return Err(TransferError::ContentMismatch {
            node_id: input.node_id,
            reason: format!(
                "declared {expected} bytes but selected {}",
                input.bytes.len()
            ),
        });
    }
    if let Some(expected) = content
        .get("media_type")
        .and_then(serde_json::Value::as_str)
        && !expected.is_empty()
        && expected != input.media_type
    {
        return Err(TransferError::ContentMismatch {
            node_id: input.node_id,
            reason: format!(
                "declared media type {expected} but selected {}",
                input.media_type
            ),
        });
    }
    if let Some(expected) = content.get("sha256").and_then(serde_json::Value::as_str) {
        let actual = format!("{:x}", Sha256::digest(&input.bytes));
        if !expected.eq_ignore_ascii_case(&actual) {
            return Err(TransferError::ContentMismatch {
                node_id: input.node_id,
                reason: "SHA-256 does not match the graph content facet".to_string(),
            });
        }
    }
    Ok(())
}

fn require_selected_content_blobs(
    facets: &NodeFacetStore,
    selected: &HashSet<Uuid>,
    blobs: &[TransferBlobV1],
) -> Result<(), TransferError> {
    for node_id in selected {
        if facets.get(node_id, &FacetId::new(CONTENT_FACET)).is_some()
            && !blobs.iter().any(|blob| blob.node_id == *node_id)
        {
            return Err(TransferError::ContentMismatch {
                node_id: *node_id,
                reason: "portable content metadata has no transferred blob".to_string(),
            });
        }
    }
    Ok(())
}

fn copied_product(
    source: &ProductEngramV1,
    manifest: &TransferManifestV1,
) -> Result<(ProductEngramV1, Vec<TransferredIdV1>), TransferError> {
    let mut donor = Graph::from_snapshot(&source.graph);
    donor.overlay_facets(source.facets.clone());
    let mut copy = Graph::new();
    let mut id_map = Vec::with_capacity(source.graph.nodes.len());
    let mut id_by_source = HashMap::with_capacity(source.graph.nodes.len());

    for persisted in &source.graph.nodes {
        let source_id = Uuid::parse_str(&persisted.node_id)
            .map_err(|error| TransferError::InvalidManifest(error.to_string()))?;
        let (_, node) = donor.get_node_by_id(source_id).ok_or_else(|| {
            TransferError::InvalidManifest(format!("source object {source_id} did not load"))
        })?;
        let destination_id = Uuid::new_v5(&manifest.transfer_id, source_id.as_bytes());
        copy.copy_node_from_with_id(
            destination_id,
            node,
            Some(manifest.source.graph.clone()),
            PortablePoint::new(0.0, 0.0),
        );
        id_by_source.insert(source_id, destination_id);
        id_map.push(TransferredIdV1 {
            source: source_id,
            destination: destination_id,
        });
    }

    let mut copied_facets = copy.facets().clone();
    for (source_id, source_facets) in source.facets.iter() {
        let Some(destination_id) = id_by_source.get(source_id).copied() else {
            continue;
        };
        for (facet, value) in source_facets.iter() {
            if copy_excludes_facet(facet.as_str()) {
                continue;
            }
            copied_facets
                .set(destination_id, facet.clone(), value.clone(), &AcceptAll)
                .expect("AcceptAll cannot reject a copied facet");
        }
    }

    let mut snapshot = copy.to_snapshot();
    snapshot.edges = source
        .graph
        .edges
        .iter()
        .cloned()
        .map(|mut edge| {
            edge.from_node_id = remapped_id(&edge.from_node_id, &id_by_source)?;
            edge.to_node_id = remapped_id(&edge.to_node_id, &id_by_source)?;
            Ok(edge)
        })
        .collect::<Result<_, TransferError>>()?;
    snapshot.timestamp_secs = source.graph.timestamp_secs;

    Ok((
        ProductEngramV1 {
            schema: source.schema.clone(),
            scope: source.scope,
            exported_at_ms: source.exported_at_ms,
            graph: snapshot,
            facets: copied_facets,
            scene: source
                .scene
                .as_ref()
                .map(|scene| remap_scene(scene, &id_by_source)),
        },
        id_map,
    ))
}

fn copy_excludes_facet(facet: &str) -> bool {
    matches!(
        facet,
        ACCESS_HISTORY_FACET
            | PROVENANCE_IMPORT
            | PROVENANCE_DERIVATIONS
            | VISIT_HISTORY
            | ARRANGEMENT_PIN
            | ARRANGEMENT_FRAME_LAYOUT
            | ARRANGEMENT_SPLIT_OFFER_SUPPRESSED
    )
}

fn remapped_id(source: &str, id_by_source: &HashMap<Uuid, Uuid>) -> Result<String, TransferError> {
    let source = Uuid::parse_str(source)
        .map_err(|error| TransferError::InvalidManifest(error.to_string()))?;
    id_by_source
        .get(&source)
        .map(Uuid::to_string)
        .ok_or_else(|| {
            TransferError::InvalidManifest(format!("relation leaves selected closure at {source}"))
        })
}

fn remap_scene(scene: &SavedSceneV1, ids: &HashMap<Uuid, Uuid>) -> SavedSceneV1 {
    let remap = |id: Uuid| ids.get(&id).copied();
    SavedSceneV1 {
        name: scene.name.clone(),
        selected: scene.selected.iter().filter_map(|id| remap(*id)).collect(),
        layout_strategy: scene.layout_strategy.clone(),
        physics_paused: scene.physics_paused,
        physics_damping: scene.physics_damping,
        arrangement_pull: scene.arrangement_pull,
        camera_offset: scene.camera_offset,
        camera_zoom: scene.camera_zoom,
        default_handler: scene.default_handler.clone(),
        cartography: mere::canvas::CartographyGeometry::from_positions(
            scene
                .cartography
                .iter()
                .filter_map(|(id, position)| remap(id).map(|new_id| (new_id, position))),
        )
        .with_sizes(
            scene
                .cartography
                .size_iter()
                .filter_map(|(id, size)| remap(id).map(|new_id| (new_id, size))),
        )
        .with_size_by_degree(scene.cartography.size_by_degree())
        .with_size_by_importance(scene.cartography.size_by_importance())
        .with_importance_metric(scene.cartography.importance_metric())
        .with_sprites(
            scene
                .cartography
                .sprite_iter()
                .filter_map(|(id, uri)| remap(id).map(|new_id| (new_id, uri.to_string()))),
        )
        .with_sprite_hulls(
            scene
                .cartography
                .sprite_hull_iter()
                .filter_map(|(id, hull)| remap(id).map(|new_id| (new_id, hull))),
        )
        .with_materials(
            scene
                .cartography
                .material_iter()
                .filter_map(|(id, material)| remap(id).map(|new_id| (new_id, material))),
        )
        .with_faces(
            scene
                .cartography
                .face_iter()
                .filter_map(|(id, face)| remap(id).map(|new_id| (new_id, face.to_string()))),
        ),
    }
}

fn attach_transfer_content<B: Backend>(
    host: &mut MereHost<B>,
    manifest: &TransferManifestV1,
    id_map: &[TransferredIdV1],
) -> Result<(), TransferError> {
    let destinations: HashMap<_, _> = id_map
        .iter()
        .map(|mapping| (mapping.source, mapping.destination))
        .collect();
    let mut by_node = BTreeMap::<Uuid, Vec<&TransferBlobV1>>::new();
    for blob in &manifest.blobs {
        by_node.entry(blob.node_id).or_default().push(blob);
    }
    for (source, descriptors) in by_node {
        let destination = destinations[&source];
        let key = host
            .graph()
            .get_node_key_by_id(destination)
            .ok_or_else(|| {
                TransferError::InvalidManifest(format!(
                    "destination object {destination} was not imported"
                ))
            })?;
        host.set_facet(
            key,
            TRANSFER_CONTENT_FACET,
            serde_json::to_value(descriptors)
                .map_err(|error| TransferError::InvalidManifest(error.to_string()))?,
        )
        .map_err(|error| TransferError::InvalidManifest(error.to_string()))?;
    }
    Ok(())
}

fn attach_existing_access_record<B: Backend>(
    host: &mut MereHost<B>,
    record: &AccessRecord,
) -> Result<(), TransferError> {
    let Some(key) = host.graph().get_node_key_by_id(record.container_id) else {
        return Ok(());
    };
    let observation = AccessObservation {
        record_id: record.record_id,
        action: record.action,
        persona: record.persona.clone(),
        device: record.device.clone(),
        application: record.application.clone(),
        handler: record.handler.clone(),
        at_ms: record.at_ms,
        dwell_ms: record.dwell_ms,
        referring_container_id: record.referring_container_id,
        referring_address: record.referring_address.clone(),
        transition: record.transition,
        capture_source: record.capture_source.clone(),
        source_event_id: record.source_event_id.clone(),
        privacy: record.privacy,
    };
    let (projected, _) = host
        .mutate_product_graph(|graph| record_observation(graph, key, &observation))
        .map_err(|error| TransferError::InvalidManifest(error.to_string()))?;
    if &projected != record {
        return Err(TransferError::InvalidManifest(format!(
            "access record {} does not name its selected container",
            record.record_id
        )));
    }
    Ok(())
}

fn muniment_hash(hash: Hash) -> Result<muniment::Hash, TransferError> {
    muniment::Hash::from_hex(&hash.to_hex()).ok_or_else(|| {
        TransferError::InvalidManifest(format!("{} is not a Muniment BLAKE3 hash", hash))
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use eidetic::{NoFetcher, load_typed};
    use mere::kernel::graph::{ProvenanceSubKind, RelationKind, SemanticSubKind};
    use muniment::MemoryBackend;

    use super::*;
    use crate::access::{
        AccessContext, bootstrap_access_record_schema, query_access_records, save_access_record,
    };
    use crate::mere_host::{
        FIXTURE_DEVICE_ONE_ADDRESS, FIXTURE_DEVICE_TWO_ADDRESS, FIXTURE_PERSONA_ADDRESS,
        SelectedPersonaRef, fixture_handlers,
    };
    use crate::product::{EditableRelation, LocalFileMetadata, TransferScope};

    fn selected(persona: &str) -> SelectedPersonaRef {
        SelectedPersonaRef {
            persona: persona.to_string(),
            profile: "profile:graphshell-h6".to_string(),
        }
    }

    fn endpoint(graph: &str, persona: &str, device: &str) -> TransferEndpointV1 {
        TransferEndpointV1 {
            graph: graph.to_string(),
            persona: persona.to_string(),
            device: device.to_string(),
        }
    }

    fn apply_context(revoked: bool) -> ApplyTransferContext {
        ApplyTransferContext {
            authorization: TransferAuthorization {
                grant_id: "grant:h6-transfer".to_string(),
                revoked,
            },
            application: "graphshell".to_string(),
            handler: "graphshell.transfer/v1".to_string(),
            completed_at_ms: 1_700_000_001_000,
            access_privacy: PrivacyClass::LocalOnly,
        }
    }

    struct Fixture {
        host: MereHost<MemoryBackend>,
        backend: MemoryBackend,
        url: Uuid,
        file: Uuid,
        file_bytes: Vec<u8>,
    }

    async fn source_fixture() -> Fixture {
        let backend = MemoryBackend::new();
        let mut host = MereHost::empty(
            backend.clone(),
            selected(FIXTURE_PERSONA_ADDRESS),
            fixture_handlers(),
            AccessContext {
                persona: FIXTURE_PERSONA_ADDRESS.to_string(),
                device: FIXTURE_DEVICE_ONE_ADDRESS.to_string(),
                at_ms: 1_700_000_000_000,
            },
        );
        let url = host
            .create_address("https://example.test/h6", "H6 transfer notes")
            .unwrap();
        host.edit_node(url, "H6 transfer notes", ["transport".to_string()])
            .unwrap();

        let path =
            std::env::temp_dir().join(format!("graphshell-h6-{}-reference.txt", Uuid::new_v4()));
        fs::write(&path, b"real file bytes for Graphshell H6\n").unwrap();
        let file_bytes = fs::read(&path).unwrap();
        fs::remove_file(&path).unwrap();
        let sha256 = format!("{:x}", Sha256::digest(&file_bytes));
        let file = host
            .create_file_metadata(LocalFileMetadata {
                content_hash: sha256,
                name: "h6-reference.txt".to_string(),
                media_type: "text/plain".to_string(),
                byte_len: file_bytes.len() as u64,
                last_modified_ms: 1_700_000_000_000,
            })
            .unwrap();
        host.edit_node(
            file,
            "H6 real file",
            ["file".to_string(), "transport".to_string()],
        )
        .unwrap();
        host.assert_product_relation(file, url, EditableRelation::Cites)
            .unwrap();

        let key = host.graph().get_node_key_by_id(url).unwrap();
        let observation = AccessObservation {
            record_id: Uuid::new_v4(),
            action: AccessAction::Examine,
            persona: FIXTURE_PERSONA_ADDRESS.to_string(),
            device: FIXTURE_DEVICE_ONE_ADDRESS.to_string(),
            application: "graphshell".to_string(),
            handler: "system.default".to_string(),
            at_ms: 1_700_000_000_100,
            dwell_ms: Some(500),
            referring_container_id: None,
            referring_address: None,
            transition: AccessTransition::UrlTyped,
            capture_source: "graphshell.test".to_string(),
            source_event_id: Some("h6-source-access".to_string()),
            privacy: PrivacyClass::LocalOnly,
        };
        let (record, _) = host
            .mutate_product_graph(|graph| record_observation(graph, key, &observation))
            .unwrap();
        let mut authority = backend.clone();
        bootstrap_access_record_schema(&mut authority)
            .await
            .unwrap();
        save_access_record(&mut authority, &record).await.unwrap();
        host.persist(1_700_000_000).await.unwrap();

        Fixture {
            host,
            backend,
            url,
            file,
            file_bytes,
        }
    }

    async fn package(
        fixture: &Fixture,
        operation: TransferOperation,
        destination_persona: &str,
    ) -> TransferManifestV1 {
        let mut authority = fixture.backend.clone();
        let blobs = BlobStore::new(fixture.backend.clone());
        prepare_transfer(
            &fixture.host,
            &mut authority,
            &blobs,
            TransferRequest {
                transfer_id: Uuid::new_v4(),
                operation,
                source: endpoint(
                    "graph:source",
                    FIXTURE_PERSONA_ADDRESS,
                    FIXTURE_DEVICE_ONE_ADDRESS,
                ),
                destination: endpoint(
                    "graph:destination",
                    destination_persona,
                    FIXTURE_DEVICE_TWO_ADDRESS,
                ),
                route: TransferRouteV1 {
                    carrier: "local.two-store".to_string(),
                    peer: "device:destination".to_string(),
                },
                selection: ExportRequest {
                    focused: fixture.file,
                    selected: vec![fixture.file, fixture.url],
                    scope: TransferScope::SelectedSubgraph,
                    exported_at_ms: 1_700_000_000_500,
                    include_local_file_locations: true,
                    scene: None,
                },
                access_policy: AccessTransferPolicy::IncludeSourceHistory,
                privacy: PrivacyClass::TrustedPeersOnly,
            },
            vec![TransferBlobInput {
                node_id: fixture.file,
                role: "primary".to_string(),
                media_type: "text/plain".to_string(),
                bytes: fixture.file_bytes.clone(),
            }],
        )
        .await
        .unwrap()
    }

    #[test]
    fn h6_replicate_preserves_ids_relations_tags_blobs_and_access_authority() {
        pollster::block_on(async {
            let source = source_fixture().await;
            let manifest = package(
                &source,
                TransferOperation::Replicate,
                FIXTURE_PERSONA_ADDRESS,
            )
            .await;
            assert_eq!(manifest.access_records.len(), 1);
            assert!(
                !String::from_utf8(manifest.selection.payload.clone())
                    .unwrap()
                    .contains(crate::product::LOCAL_FILE_FACET)
            );

            let destination_backend = MemoryBackend::new();
            let mut destination = MereHost::empty(
                destination_backend.clone(),
                selected(FIXTURE_PERSONA_ADDRESS),
                fixture_handlers(),
                AccessContext {
                    persona: FIXTURE_PERSONA_ADDRESS.to_string(),
                    device: FIXTURE_DEVICE_TWO_ADDRESS.to_string(),
                    at_ms: 1_700_000_001_000,
                },
            );
            let source_blobs = BlobStore::new(source.backend.clone());
            let destination_blobs = BlobStore::new(destination_backend.clone());
            let mut destination_authority = destination_backend.clone();
            let receipt = apply_transfer(
                &mut destination,
                &source_blobs,
                &destination_blobs,
                &mut destination_authority,
                &manifest,
                &apply_context(false),
            )
            .await
            .unwrap();

            assert_eq!(receipt.nodes, 2);
            assert_eq!(receipt.relations, 1);
            assert!(
                receipt
                    .id_map
                    .iter()
                    .all(|ids| ids.source == ids.destination)
            );
            assert!(destination.graph().get_node_by_id(source.url).is_some());
            assert!(destination.graph().get_node_by_id(source.file).is_some());
            assert!(
                destination
                    .graph()
                    .get_node_by_id(source.file)
                    .unwrap()
                    .1
                    .tags
                    .contains("file")
            );
            assert!(destination.graph().relations().any(|relation| {
                relation.kind == RelationKind::Semantic(SemanticSubKind::Cites)
            }));
            let descriptor = &manifest.blobs[0];
            let stored = destination_blobs
                .get(&muniment_hash(descriptor.content_hash).unwrap())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(stored, source.file_bytes);
            assert_eq!(
                destination
                    .facet_value(
                        &format!(
                            "urn:sha256:{}",
                            Sha256::digest(&source.file_bytes)
                                .iter()
                                .map(|byte| format!("{byte:02x}"))
                                .collect::<String>()
                        ),
                        TRANSFER_CONTENT_FACET
                    )
                    .unwrap()[0]["content_hash"],
                serde_json::to_value(descriptor.content_hash).unwrap()
            );

            let records =
                query_access_records(&mut destination_authority, &AccessRecordFilter::default())
                    .await
                    .unwrap();
            assert_eq!(records.len(), 3, "source, URL import, and file import");
            assert_eq!(
                records
                    .iter()
                    .filter(|record| record.action == AccessAction::Import)
                    .count(),
                2
            );
            assert_eq!(
                destination
                    .access_history_for("https://example.test/h6")
                    .unwrap()
                    .records
                    .len(),
                2,
                "source examination and destination import"
            );

            let receipt_id =
                ManifestId::from_hash(Hash::of(&receipt.serialize_to_bytes().unwrap()));
            let stored_receipt = load_typed::<TransferReceiptV1>(
                &mut destination_authority,
                &mut NoFetcher,
                receipt_id,
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(stored_receipt, receipt);
        });
    }

    #[test]
    fn h6_copy_mints_ids_and_keeps_copied_from_provenance() {
        pollster::block_on(async {
            let source = source_fixture().await;
            let destination_persona = "personae://persona/bob";
            let manifest = package(&source, TransferOperation::Copy, destination_persona).await;
            let destination_backend = MemoryBackend::new();
            let mut destination = MereHost::empty(
                destination_backend.clone(),
                selected(destination_persona),
                fixture_handlers(),
                AccessContext {
                    persona: destination_persona.to_string(),
                    device: FIXTURE_DEVICE_TWO_ADDRESS.to_string(),
                    at_ms: 1_700_000_001_000,
                },
            );
            let mut authority = destination_backend.clone();
            let receipt = apply_transfer(
                &mut destination,
                &BlobStore::new(source.backend.clone()),
                &BlobStore::new(destination_backend.clone()),
                &mut authority,
                &manifest,
                &apply_context(false),
            )
            .await
            .unwrap();

            assert!(
                receipt
                    .id_map
                    .iter()
                    .all(|ids| ids.source != ids.destination)
            );
            for mapping in &receipt.id_map {
                let key = destination
                    .graph()
                    .get_node_key_by_id(mapping.destination)
                    .unwrap();
                let derivations = destination.graph().node_derivations(key).unwrap();
                assert_eq!(derivations.len(), 1);
                assert_eq!(derivations[0].sub_kind, ProvenanceSubKind::CopiedFrom);
                assert_eq!(derivations[0].source_node, mapping.source.to_string());
                assert_eq!(derivations[0].source_graph.as_deref(), Some("graph:source"));
            }
            assert_eq!(destination.graph().relations().count(), 1);
            let copied_file = receipt
                .id_map
                .iter()
                .find(|ids| ids.source == source.file)
                .unwrap()
                .destination;
            assert!(
                destination
                    .graph()
                    .get_node_by_id(copied_file)
                    .unwrap()
                    .1
                    .tags
                    .contains("transport")
            );
            assert_eq!(
                destination
                    .graph()
                    .facets()
                    .get(&copied_file, &FacetId::new(TRANSFER_CONTENT_FACET))
                    .unwrap()[0]["byte_len"],
                source.file_bytes.len() as u64
            );
        });
    }

    #[test]
    fn h6_revoked_grant_refuses_before_blob_or_graph_mutation() {
        pollster::block_on(async {
            let source = source_fixture().await;
            let manifest = package(
                &source,
                TransferOperation::Replicate,
                FIXTURE_PERSONA_ADDRESS,
            )
            .await;
            let destination_backend = MemoryBackend::new();
            let mut destination = MereHost::empty(
                destination_backend.clone(),
                selected(FIXTURE_PERSONA_ADDRESS),
                fixture_handlers(),
                AccessContext {
                    persona: FIXTURE_PERSONA_ADDRESS.to_string(),
                    device: FIXTURE_DEVICE_TWO_ADDRESS.to_string(),
                    at_ms: 1_700_000_001_000,
                },
            );
            let mut authority = destination_backend.clone();
            let error = apply_transfer(
                &mut destination,
                &BlobStore::new(source.backend.clone()),
                &BlobStore::new(destination_backend.clone()),
                &mut authority,
                &manifest,
                &apply_context(true),
            )
            .await
            .unwrap_err();
            assert!(matches!(error, TransferError::Revoked(_)));
            assert_eq!(destination.graph().node_count(), 0);
            assert!(destination_backend.is_empty());
        });
    }

    #[test]
    fn h6_retry_reuses_verified_blobs_and_deterministic_copy_ids() {
        pollster::block_on(async {
            let source = source_fixture().await;
            let manifest =
                package(&source, TransferOperation::Copy, "personae://persona/bob").await;
            let destination_backend = MemoryBackend::new();
            let mut destination = MereHost::empty(
                destination_backend.clone(),
                selected("personae://persona/bob"),
                fixture_handlers(),
                AccessContext {
                    persona: "personae://persona/bob".to_string(),
                    device: FIXTURE_DEVICE_TWO_ADDRESS.to_string(),
                    at_ms: 1_700_000_001_000,
                },
            );
            let source_blobs = BlobStore::new(source.backend.clone());
            let destination_blobs = BlobStore::new(destination_backend.clone());
            let mut authority = destination_backend.clone();
            let first = apply_transfer(
                &mut destination,
                &source_blobs,
                &destination_blobs,
                &mut authority,
                &manifest,
                &apply_context(false),
            )
            .await
            .unwrap();
            let node_count = destination.graph().node_count();
            let relation_count = destination.graph().relations().count();
            let record_count = query_access_records(&mut authority, &AccessRecordFilter::default())
                .await
                .unwrap()
                .len();
            let mut retry_context = apply_context(false);
            retry_context.completed_at_ms += 9_000;
            let second = apply_transfer(
                &mut destination,
                &source_blobs,
                &destination_blobs,
                &mut authority,
                &manifest,
                &retry_context,
            )
            .await
            .unwrap();
            assert_eq!(second.id_map, first.id_map);
            assert_eq!(destination.graph().node_count(), node_count);
            assert_eq!(destination.graph().relations().count(), relation_count);
            assert_eq!(
                query_access_records(&mut authority, &AccessRecordFilter::default())
                    .await
                    .unwrap()
                    .len(),
                record_count,
                "retry reuses the original destination access records"
            );
        });
    }

    #[test]
    fn h6_resume_uses_an_already_verified_destination_blob() {
        pollster::block_on(async {
            let source = source_fixture().await;
            let manifest = package(
                &source,
                TransferOperation::Replicate,
                FIXTURE_PERSONA_ADDRESS,
            )
            .await;
            let destination_backend = MemoryBackend::new();
            let mut destination = MereHost::empty(
                destination_backend.clone(),
                selected(FIXTURE_PERSONA_ADDRESS),
                fixture_handlers(),
                AccessContext {
                    persona: FIXTURE_PERSONA_ADDRESS.to_string(),
                    device: FIXTURE_DEVICE_TWO_ADDRESS.to_string(),
                    at_ms: 1_700_000_001_000,
                },
            );
            let source_blobs = BlobStore::new(source.backend.clone());
            let destination_blobs = BlobStore::new(destination_backend.clone());
            let descriptor = &manifest.blobs[0];
            let hash = muniment_hash(descriptor.content_hash).unwrap();

            destination_blobs.put(&source.file_bytes).await.unwrap();
            source_blobs
                .backend()
                .delete(&format!("blob/{}", hash.to_hex()))
                .await
                .unwrap();
            assert!(source_blobs.get(&hash).await.unwrap().is_none());

            let mut authority = destination_backend.clone();
            let receipt = apply_transfer(
                &mut destination,
                &source_blobs,
                &destination_blobs,
                &mut authority,
                &manifest,
                &apply_context(false),
            )
            .await
            .unwrap();
            assert_eq!(receipt.result, TransferResult::Completed);
            assert_eq!(
                destination_blobs.get(&hash).await.unwrap(),
                Some(source.file_bytes)
            );
        });
    }
}
