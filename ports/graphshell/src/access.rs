//! Address-access facts projected into Mere's unknown-forward facet store.

use chartulary::{FacetError, FacetId};
use eidetic::{
    BlobSource, Hash, ManifestId, MereNativeFieldSpec, MereNativeSchemaBuilder, ModerationState,
    NoFetcher, PayloadSealer, PrivacyClass, ProvenanceOrigin, ProvenanceRecord, SchemaDefinition,
    SchemaRef, Timestamp, TrustEnvelope, TrustLevel, TypedPayload, list_typed, load_typed_sealed,
    save_schema, save_typed_sealed,
};
use mere::kernel::graph::apply::{GraphDelta, GraphDeltaResult, apply_graph_delta};
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
    save_access_record_sealed(store, None, record).await
}

/// Save an access record, sealing it at rest when it belongs to the private
/// lane and a sealer is supplied.
///
/// The record's own `privacy` decides: `LocalOnly` and `TrustedPeersOnly` seal,
/// the public lane stays cleartext so it keeps its dedup and self-verification
/// properties. Passing `None` stores cleartext, which is what
/// [`save_access_record`] does and what every caller did before this existed.
///
/// The sealer comes from the resident keeper, which is the only component
/// holding the carry root: `castellan::authority::PersonaeHost::payload_sealer`.
/// Access records are the natural first consumer because they are the private
/// lane's densest writer, one record per visit, and they name a persona.
pub async fn save_access_record_sealed<B: Backend>(
    store: &mut B,
    sealer: Option<&dyn PayloadSealer>,
    record: &AccessRecord,
) -> Result<ManifestId, eidetic::Error> {
    save_typed_sealed(
        store,
        sealer,
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
    query_access_records_sealed(store, None, filter).await
}

/// Query the access authority, unsealing private-lane records with `sealer`.
///
/// A sealed record read with `sealer = None` is a hard error from eidetic
/// rather than a silent miss, which is the behaviour worth having: a reader
/// that has lost its epoch should say so instead of reporting an empty history.
pub async fn query_access_records_sealed<B: Backend>(
    store: &mut B,
    sealer: Option<&dyn PayloadSealer>,
    filter: &AccessRecordFilter,
) -> Result<Vec<AccessRecord>, eidetic::Error> {
    let mut records = Vec::new();
    let mut fetcher = NoFetcher;
    for manifest in list_typed::<AccessRecord>(store).await? {
        let Some(record) =
            load_typed_sealed::<AccessRecord>(store, &mut fetcher, sealer, manifest.id).await?
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
    let updated = apply_graph_delta(
        graph,
        GraphDelta::SetNodeFacet {
            key,
            facet: ACCESS_HISTORY_FACET.to_string(),
            value,
        },
    );
    debug_assert_eq!(updated, GraphDeltaResult::NodeMetadataUpdated(true));
    Ok((record, true))
}

/// Proof that the private lane actually seals, exercised against the real
/// wallet sealer rather than a stub.
///
/// `eidetic::seal` shipped inert on purpose: "nothing seals until a host wires
/// a `PayloadSealer` in". These tests are what makes that sentence stop being
/// true for access records, and they check the three properties the seam
/// promises rather than only that the call compiles.
#[cfg(all(test, feature = "native"))]
mod seal_wiring {
    use super::*;
    use muniment::MemoryBackend;
    use pandect::{KeyEpochId, PersonaId, WalletEpochSealer};

    const ADDRESS: &str = "https://example.invalid/private-lane-probe";

    fn record(privacy: PrivacyClass) -> AccessRecord {
        AccessRecord {
            record_id: Uuid::from_bytes([7; 16]),
            container_id: Uuid::from_bytes([8; 16]),
            address: ADDRESS.to_string(),
            action: AccessAction::Open,
            persona: "persona:probe".to_string(),
            device: "device:probe".to_string(),
            application: "graphshell.test".to_string(),
            at_ms: 1_700_000_000_000,
            handler: "system.default".to_string(),
            dwell_ms: None,
            referring_container_id: None,
            referring_address: None,
            transition: AccessTransition::UrlTyped,
            capture_source: "graphshell.test".to_string(),
            source_event_id: None,
            privacy,
        }
    }

    fn sealer() -> WalletEpochSealer {
        WalletEpochSealer::from_epoch(
            PersonaId::new(),
            KeyEpochId(Uuid::from_bytes([0xA1; 16])),
            b"probe-epoch-secret",
        )
    }

    /// Every blob byte held by the store, so a cleartext leak is caught by
    /// looking rather than by trusting the marker.
    async fn blob_bytes(store: &MemoryBackend) -> Vec<u8> {
        let mut all = Vec::new();
        for key in store.list("blob:").await.unwrap() {
            if let Some(bytes) = store.get(&key).await.unwrap() {
                all.extend(bytes);
            }
        }
        all
    }

    #[tokio::test]
    async fn a_private_record_is_unreadable_on_disk_and_reads_back_through_the_sealer() {
        let mut store = MemoryBackend::new();
        bootstrap_access_record_schema(&mut store).await.unwrap();
        let sealer = sealer();
        let record = record(PrivacyClass::LocalOnly);

        save_access_record_sealed(&mut store, Some(&sealer), &record)
            .await
            .unwrap();

        // The claim worth checking is about bytes, not about metadata: a seal
        // marker could be stamped on a manifest whose blob was never sealed.
        let raw = blob_bytes(&store).await;
        assert!(
            !raw.windows(ADDRESS.len()).any(|w| w == ADDRESS.as_bytes()),
            "the visited address survived in cleartext on disk"
        );

        let read =
            query_access_records_sealed(&mut store, Some(&sealer), &AccessRecordFilter::default())
                .await
                .unwrap();
        assert_eq!(read, vec![record], "the sealed record did not round-trip");
    }

    #[tokio::test]
    async fn a_reader_without_the_epoch_says_so_instead_of_reporting_no_history() {
        let mut store = MemoryBackend::new();
        bootstrap_access_record_schema(&mut store).await.unwrap();
        save_access_record_sealed(
            &mut store,
            Some(&sealer()),
            &record(PrivacyClass::LocalOnly),
        )
        .await
        .unwrap();

        // The failure mode this forecloses: a keyless read reporting an empty
        // history, which is indistinguishable from having browsed nothing.
        let blind = query_access_records(&mut store, &AccessRecordFilter::default()).await;
        assert!(
            blind.is_err(),
            "a keyless read returned {:?} rather than refusing",
            blind.map(|records| records.len())
        );
    }

    #[tokio::test]
    async fn the_public_lane_stays_cleartext_under_the_same_sealer() {
        let mut store = MemoryBackend::new();
        bootstrap_access_record_schema(&mut store).await.unwrap();
        let record = record(PrivacyClass::PublicPortable);

        save_access_record_sealed(&mut store, Some(&sealer()), &record)
            .await
            .unwrap();

        // The wallet plan's decisive asymmetry: the public lane keeps dedup,
        // pin-by-others and self-verification, so it must not seal even when a
        // sealer is present and willing.
        let raw = blob_bytes(&store).await;
        assert!(
            raw.windows(ADDRESS.len()).any(|w| w == ADDRESS.as_bytes()),
            "a public-lane record was sealed; the lane asymmetry is broken"
        );
        let read = query_access_records(&mut store, &AccessRecordFilter::default())
            .await
            .unwrap();
        assert_eq!(read, vec![record], "a cleartext record needed a sealer");
    }
}
