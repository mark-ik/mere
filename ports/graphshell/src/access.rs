//! Address-access facts projected into Mere's unknown-forward facet store.

use chartulary::{AcceptAll, FacetError, FacetId};
use eidetic::{
    BlobSource, Hash, ManifestId, MereNativeFieldSpec, MereNativeSchemaBuilder, ModerationState,
    NoFetcher, PrivacyClass, ProvenanceOrigin, ProvenanceRecord, SchemaDefinition, SchemaRef,
    Timestamp, TrustEnvelope, TrustLevel, TypedPayload, list_typed, load_typed, save_schema,
    save_typed,
};
use mere::kernel::graph::{Graph, NodeKey};
use muniment::Backend;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stable facet id for Graphshell's portable access history.
pub const ACCESS_HISTORY_FACET: &str = "graphshell.access-history/v1";

pub static ACCESS_RECORD_SCHEMA_REF: std::sync::LazyLock<SchemaRef> =
    std::sync::LazyLock::new(|| {
        let bytes = serde_json::to_vec(&access_record_schema())
            .expect("AccessRecord schema definition always serializes");
        SchemaRef::from_id(ManifestId::from_hash(Hash::of(&bytes)))
    });

fn access_record_schema() -> SchemaDefinition {
    MereNativeSchemaBuilder::new("graphshell.AccessRecord/v1")
        .description("One append-only observation of an addressed Mere container.")
        .field("record_id", MereNativeFieldSpec::String, true)
        .field("container_id", MereNativeFieldSpec::String, true)
        .field("address", MereNativeFieldSpec::String, true)
        .field("action", MereNativeFieldSpec::String, true)
        .field("persona", MereNativeFieldSpec::String, true)
        .field("device", MereNativeFieldSpec::String, true)
        .field("application", MereNativeFieldSpec::String, true)
        .field("handler", MereNativeFieldSpec::String, true)
        .field("at_ms", MereNativeFieldSpec::U64, true)
        .field("dwell_ms", MereNativeFieldSpec::U64, false)
        .field("referring_container_id", MereNativeFieldSpec::String, false)
        .field("referring_address", MereNativeFieldSpec::String, false)
        .field("transition", MereNativeFieldSpec::String, true)
        .field("capture_source", MereNativeFieldSpec::String, true)
        .field("source_event_id", MereNativeFieldSpec::String, false)
        .field("privacy", MereNativeFieldSpec::String, true)
        .build()
}

pub async fn bootstrap_access_record_schema<B: Backend>(
    store: &mut B,
) -> Result<(), eidetic::Error> {
    if eidetic::manifest::load_manifest(store, ACCESS_RECORD_SCHEMA_REF.0)
        .await?
        .is_some()
    {
        return Ok(());
    }
    let id = save_schema(
        store,
        &access_record_schema(),
        PrivacyClass::PublicPortable,
        ProvenanceRecord {
            origin: ProvenanceOrigin::Generated,
            upstream: Vec::new(),
            tooling: Some(format!("graphshell/{}", env!("CARGO_PKG_VERSION"))),
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
    debug_assert_eq!(id, ACCESS_RECORD_SCHEMA_REF.0);
    Ok(())
}

/// The public identity context attached to one address access.
///
/// These are references only. Personae secrets and signing authority remain in
/// the native host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessContext {
    pub persona: String,
    pub device: String,
    pub at_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessAction {
    Examine,
    Open,
    Import,
    Preview,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessTransition {
    LinkClick,
    UrlTyped,
    Back,
    Forward,
    Reload,
    Redirect,
    TabSpawn,
    Restore,
    Imported,
    Unknown,
}

/// Inputs known at the observation edge. Container id and addressed value are
/// resolved from the graph so callers cannot claim another target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessObservation {
    pub record_id: Uuid,
    pub action: AccessAction,
    pub persona: String,
    pub device: String,
    pub application: String,
    pub handler: String,
    pub at_ms: u64,
    pub dwell_ms: Option<u64>,
    pub referring_container_id: Option<Uuid>,
    pub referring_address: Option<String>,
    pub transition: AccessTransition,
    pub capture_source: String,
    pub source_event_id: Option<String>,
    pub privacy: PrivacyClass,
}

/// One durable record of an address being handed to a selected handler.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessRecord {
    pub record_id: Uuid,
    pub container_id: Uuid,
    pub address: String,
    pub action: AccessAction,
    pub persona: String,
    pub device: String,
    pub application: String,
    pub at_ms: u64,
    pub handler: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dwell_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub referring_container_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub referring_address: Option<String>,
    pub transition: AccessTransition,
    pub capture_source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_event_id: Option<String>,
    pub privacy: PrivacyClass,
}

impl TypedPayload for AccessRecord {
    fn schema_ref() -> SchemaRef {
        *ACCESS_RECORD_SCHEMA_REF
    }
}

pub async fn save_access_record<B: Backend>(
    store: &mut B,
    record: &AccessRecord,
) -> Result<ManifestId, eidetic::Error> {
    save_typed(
        store,
        record,
        Vec::<BlobSource>::new(),
        record.privacy,
        ProvenanceRecord {
            origin: ProvenanceOrigin::Generated,
            upstream: Vec::new(),
            tooling: Some(format!("graphshell-access/{}", env!("CARGO_PKG_VERSION"))),
            generated_at: Timestamp(record.at_ms),
        },
        TrustEnvelope {
            level: TrustLevel::SelfAsserted,
            signatures: Vec::new(),
            moderation_state: ModerationState::Unreviewed,
        },
        Timestamp(record.at_ms),
    )
    .await
}

/// Filters over the append-only access authority. Time bounds are a half-open
/// interval: `start_ms` is inclusive and `end_ms` is exclusive.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AccessRecordFilter {
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
    pub persona: Option<String>,
    pub device: Option<String>,
}

impl AccessRecordFilter {
    fn matches(&self, record: &AccessRecord) -> bool {
        self.start_ms
            .is_none_or(|start_ms| record.at_ms >= start_ms)
            && self.end_ms.is_none_or(|end_ms| record.at_ms < end_ms)
            && self
                .persona
                .as_ref()
                .is_none_or(|persona| &record.persona == persona)
            && self
                .device
                .as_ref()
                .is_none_or(|device| &record.device == device)
    }
}

/// Query the Eidetic authority rather than the graph facet cache. Results are
/// stable by observation time and record id.
pub async fn query_access_records<B: Backend>(
    store: &mut B,
    filter: &AccessRecordFilter,
) -> Result<Vec<AccessRecord>, eidetic::Error> {
    let mut records = Vec::new();
    let mut fetcher = NoFetcher;
    for manifest in list_typed::<AccessRecord>(store).await? {
        let Some(record) = load_typed::<AccessRecord>(store, &mut fetcher, manifest.id).await?
        else {
            continue;
        };
        if filter.matches(&record) {
            records.push(record);
        }
    }
    records.sort_by_key(|record| (record.at_ms, record.record_id));
    Ok(records)
}

/// Ordered accesses for one addressed node.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessHistory {
    pub records: Vec<AccessRecord>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyAccessRecord {
    persona: String,
    device: String,
    at_ms: u64,
    handler: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyAccessHistory {
    records: Vec<LegacyAccessRecord>,
}

#[derive(Debug)]
pub enum AccessError {
    UnknownNode,
    InvalidFacet(serde_json::Error),
    RejectedFacet(FacetError),
}

impl std::fmt::Display for AccessError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownNode => write!(formatter, "access target is not in the Mere graph"),
            Self::InvalidFacet(error) => write!(formatter, "access history is invalid: {error}"),
            Self::RejectedFacet(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for AccessError {}

/// Read one node's access history. An absent facet is an empty history.
pub fn access_history(graph: &Graph, key: NodeKey) -> Result<AccessHistory, AccessError> {
    let node = graph.get_node(key).ok_or(AccessError::UnknownNode)?;
    let Some(value) = graph
        .facets()
        .get(&node.id, &FacetId::new(ACCESS_HISTORY_FACET))
    else {
        return Ok(AccessHistory::default());
    };
    if let Ok(history) = serde_json::from_value::<AccessHistory>(value.clone()) {
        return Ok(history);
    }
    let legacy: LegacyAccessHistory =
        serde_json::from_value(value.clone()).map_err(AccessError::InvalidFacet)?;
    Ok(AccessHistory {
        records: legacy
            .records
            .into_iter()
            .enumerate()
            .map(|(index, legacy)| {
                let identity = format!(
                    "{}\u{0}{}\u{0}{}\u{0}{}\u{0}{}",
                    node.id, legacy.persona, legacy.device, legacy.at_ms, index
                );
                AccessRecord {
                    record_id: Uuid::new_v5(&Uuid::NAMESPACE_URL, identity.as_bytes()),
                    container_id: node.id,
                    address: node.url().to_string(),
                    action: AccessAction::Open,
                    persona: legacy.persona,
                    device: legacy.device,
                    application: legacy.handler.clone(),
                    at_ms: legacy.at_ms,
                    handler: legacy.handler,
                    dwell_ms: None,
                    referring_container_id: None,
                    referring_address: None,
                    transition: AccessTransition::Unknown,
                    capture_source: "graphshell.legacy-facet".to_string(),
                    source_event_id: None,
                    privacy: PrivacyClass::LocalOnly,
                }
            })
            .collect(),
    })
}

/// Append an access through the host extension gate.
pub fn record_access(
    graph: &mut Graph,
    key: NodeKey,
    context: &AccessContext,
    handler: &str,
) -> Result<(), AccessError> {
    record_observation(
        graph,
        key,
        &AccessObservation {
            record_id: Uuid::new_v4(),
            action: AccessAction::Open,
            persona: context.persona.clone(),
            device: context.device.clone(),
            application: handler.to_string(),
            handler: handler.to_string(),
            at_ms: context.at_ms,
            dwell_ms: None,
            referring_container_id: None,
            referring_address: None,
            transition: AccessTransition::Unknown,
            capture_source: "graphshell.intent".to_string(),
            source_event_id: None,
            privacy: PrivacyClass::LocalOnly,
        },
    )
    .map(|_| ())
}

/// Append one complete observation to the derived node cache. Duplicate stable
/// record ids are idempotent. The caller separately saves the same record as
/// the append-only Eidetic authority.
pub fn record_observation(
    graph: &mut Graph,
    key: NodeKey,
    observation: &AccessObservation,
) -> Result<(AccessRecord, bool), AccessError> {
    let node_id = graph
        .get_node(key)
        .map(|node| node.id)
        .ok_or(AccessError::UnknownNode)?;
    let address = graph
        .get_node(key)
        .map(|node| node.url().to_string())
        .ok_or(AccessError::UnknownNode)?;
    let mut history = access_history(graph, key)?;
    let record = AccessRecord {
        record_id: observation.record_id,
        container_id: node_id,
        address,
        action: observation.action,
        persona: observation.persona.clone(),
        device: observation.device.clone(),
        application: observation.application.clone(),
        at_ms: observation.at_ms,
        handler: observation.handler.clone(),
        dwell_ms: observation.dwell_ms,
        referring_container_id: observation.referring_container_id,
        referring_address: observation.referring_address.clone(),
        transition: observation.transition,
        capture_source: observation.capture_source.clone(),
        source_event_id: observation.source_event_id.clone(),
        privacy: observation.privacy,
    };
    if history
        .records
        .iter()
        .any(|existing| existing.record_id == record.record_id)
    {
        return Ok((record, false));
    }
    history.records.push(record.clone());
    let value = serde_json::to_value(history).expect("AccessHistory always serializes");
    graph
        .facets_mut()
        .set(
            node_id,
            FacetId::new(ACCESS_HISTORY_FACET),
            value,
            &AcceptAll,
        )
        .map_err(AccessError::RejectedFacet)?;
    Ok((record, true))
}
