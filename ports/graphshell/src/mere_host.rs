//! Mere graph truth adapted to Graphshell's portable endpoint vocabulary.

use std::collections::{BTreeMap, HashMap};

use chartulary::{FacetError, FacetId};
use chirograph::{
    BoundsRelationship, CachePolicy, CardValueV1, ContentHash, EndpointDescriptor,
    IntentInvocation, IntentResult, PortableCardV1, PresentationBinding, PresentationCapability,
    PresentationCodec, PresentationKey, PresentationManifest, PresentationOffer,
    PresentationSemantics, ProjectionOffer, ProjectionRequest, ProjectionSession,
    ProjectionSnapshot, ProtocolVersion, ResourceRequest, ResourceResponse, SemanticRole,
};
use graphshell_endpoint::{IntentSink, PresentationSource, ProjectionCatalog, ProjectionSource};
use mere::kernel::graph::apply::{GraphDelta, apply_graph_delta};
use mere::kernel::graph::{Graph, NodeFacetStore, NodeKey, RelationKind};
use mere::kernel::persistence::GraphSnapshot;
use muniment::{Backend, JsonSlots, StoreError};
use sceno::{
    Arrangement, Footprint, InstanceId, ProjectedItem, Rect, Representation, RoutedRelation, Scene,
    Score, Size2, SourceRef, Transform2, Vec2,
};
use scenotime::{Revision, SceneEpoch, SceneSnapshot};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::access::{AccessContext, AccessError, AccessHistory, access_history, record_access};
use crate::handlers::{
    HandlerOffer, HandlerRegistry, OpenAddressV1, handler_from_intent, intent_id,
};

pub const HOST_SLOT: &str = "graphshell/mere-host/v1";
pub const LOCAL_SESSION: &str = "local:mere";

pub const FIXTURE_WEB_ADDRESS: &str = "https://example.test/i2p-port";
pub const FIXTURE_NON_WEB_ADDRESS: &str = "i2p://reference/service";
pub const FIXTURE_FILE_ADDRESS: &str = "file:///Graphshell/reference-notes.md";
pub const FIXTURE_SCENE_ADDRESS: &str = "mere://scene/reference-host";
pub const FIXTURE_REMOTE_ADDRESS: &str = "graphshell://projection/loopback-g1";
pub const FIXTURE_PERSONA_ADDRESS: &str = "personae://persona/alice";
pub const FIXTURE_DEVICE_ONE_ADDRESS: &str = "personae://device/laptop";
pub const FIXTURE_DEVICE_TWO_ADDRESS: &str = "personae://device/phone";
pub const FIXTURE_KEY_ADDRESS: &str = "personae://key/ssh-ed25519/test";
pub const FIXTURE_GRANT_ADDRESS: &str = "personae://grant/open-addresses";
pub const FIXTURE_RECEIPT_ADDRESS: &str = "personae://receipt/signing/test";
pub const UNKNOWN_FIXTURE_FACET: &str = "future.graphshell.transport-route/v7";

/// Public identity selection injected by the host.
///
/// It deliberately carries references only. Vault handles, private keys, and
/// signing authority remain in Personae on the native side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedPersonaRef {
    pub persona: String,
    pub profile: String,
}

#[derive(Debug)]
pub enum MereHostError {
    Store(StoreError),
    Access(AccessError),
    Facet(FacetError),
    InvalidSnapshot(String),
    WrongSession,
    MissingResource,
}

impl std::fmt::Display for MereHostError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(error) => write!(formatter, "Mere host storage: {error}"),
            Self::Access(error) => write!(formatter, "{error}"),
            Self::Facet(error) => write!(formatter, "{error}"),
            Self::InvalidSnapshot(error) => write!(formatter, "Mere projection: {error}"),
            Self::WrongSession => write!(formatter, "request names another projection session"),
            Self::MissingResource => {
                write!(formatter, "resource was not disclosed by this session")
            }
        }
    }
}

impl std::error::Error for MereHostError {}

impl From<StoreError> for MereHostError {
    fn from(value: StoreError) -> Self {
        Self::Store(value)
    }
}

impl From<AccessError> for MereHostError {
    fn from(value: AccessError) -> Self {
        Self::Access(value)
    }
}

impl From<FacetError> for MereHostError {
    fn from(value: FacetError) -> Self {
        Self::Facet(value)
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct PersistedMereHost {
    graph: GraphSnapshot,
    facets: NodeFacetStore,
    projection_epoch: u64,
    projection_revision: u64,
}

/// The H1 local host: Mere owns truth, Muniment owns bytes, and Graphshell
/// projects scenes and typed intents over both.
pub struct MereHost<B> {
    slots: JsonSlots<B>,
    // Reachable from `mere_host_fixture`, which builds the reference graph
    // node by node; nothing outside this crate touches either field.
    pub(crate) graph: Graph,
    selected_persona: SelectedPersonaRef,
    handlers: HandlerRegistry,
    access_context: AccessContext,
    projection_epoch: u64,
    pub(crate) projection_revision: u64,
    resources: BTreeMap<ContentHash, Vec<u8>>,
    instance_targets: Vec<NodeKey>,
    persisted_document: Option<PersistedMereHost>,
    dirty: bool,
}

impl<B: Backend> MereHost<B> {
    fn persistence_document(&self, saved_at_secs: u64) -> PersistedMereHost {
        match &self.persisted_document {
            Some(document) if !self.dirty && document.graph.timestamp_secs == saved_at_secs => {
                document.clone()
            }
            _ => {
                let mut graph = self.graph.to_snapshot();
                graph.timestamp_secs = saved_at_secs;
                PersistedMereHost {
                    graph,
                    facets: self.graph.facets().clone(),
                    projection_epoch: self.projection_epoch,
                    projection_revision: self.projection_revision,
                }
            }
        }
    }

    pub fn empty(
        backend: B,
        selected_persona: SelectedPersonaRef,
        handlers: HandlerRegistry,
        access_context: AccessContext,
    ) -> Self {
        Self {
            slots: JsonSlots::new(backend),
            graph: Graph::new(),
            selected_persona,
            handlers,
            access_context,
            projection_epoch: 1,
            projection_revision: 1,
            resources: BTreeMap::new(),
            instance_targets: Vec::new(),
            persisted_document: None,
            dirty: true,
        }
    }

    /// Reopen graph and facet truth from an injected Muniment backend.
    pub async fn open(
        backend: B,
        selected_persona: SelectedPersonaRef,
        handlers: HandlerRegistry,
        access_context: AccessContext,
    ) -> Result<Self, MereHostError> {
        let slots = JsonSlots::new(backend);
        let Some(saved): Option<PersistedMereHost> = slots.load(HOST_SLOT).await? else {
            return Ok(Self {
                slots,
                graph: Graph::new(),
                selected_persona,
                handlers,
                access_context,
                projection_epoch: 1,
                projection_revision: 1,
                resources: BTreeMap::new(),
                instance_targets: Vec::new(),
                persisted_document: None,
                dirty: true,
            });
        };
        let persisted_document = saved.clone();
        let mut graph = Graph::from_snapshot(&saved.graph);
        graph.overlay_facets(saved.facets);
        Ok(Self {
            slots,
            graph,
            selected_persona,
            handlers,
            access_context,
            projection_epoch: saved.projection_epoch,
            projection_revision: saved.projection_revision,
            resources: BTreeMap::new(),
            instance_targets: Vec::new(),
            persisted_document: Some(persisted_document),
            dirty: false,
        })
    }

    /// Persist graph and facets as one typed Muniment slot.
    ///
    /// The clock is injected so tests and importing hosts can produce stable
    /// bytes. Equal graph/facet truth plus an equal clock yields equal bytes.
    pub async fn persist(&mut self, saved_at_secs: u64) -> Result<(), MereHostError> {
        let document = self.persistence_document(saved_at_secs);
        self.slots.save(HOST_SLOT, &document).await?;
        self.persisted_document = Some(document);
        self.dirty = false;
        Ok(())
    }

    /// Stage the host document through another backend. Capture uses this with
    /// a write-buffer backend so the graph, facets, access records, and
    /// browsing traces land in one outer `Backend::apply`.
    pub(crate) async fn persist_through<T: Backend>(
        &mut self,
        backend: &T,
        saved_at_secs: u64,
    ) -> Result<(), MereHostError> {
        let document = self.persistence_document(saved_at_secs);
        let bytes =
            serde_json::to_vec(&document).map_err(|error| StoreError::Codec(error.to_string()))?;
        backend.put(HOST_SLOT, &bytes).await?;
        self.persisted_document = Some(document);
        self.dirty = false;
        Ok(())
    }

    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    /// Whether this host was reconstructed from a stored document rather than
    /// created empty or from the reference fixture in this process.
    pub fn was_reopened(&self) -> bool {
        self.persisted_document.is_some()
    }

    pub fn selected_persona(&self) -> &SelectedPersonaRef {
        &self.selected_persona
    }

    pub fn session(&self) -> ProjectionSession {
        ProjectionSession(LOCAL_SESSION.to_string())
    }

    pub fn projection_revision(&self) -> Revision {
        Revision(self.projection_revision)
    }

    pub fn set_access_context(&mut self, context: AccessContext) {
        self.access_context = context;
    }

    pub fn access_history_for(&self, address: &str) -> Result<AccessHistory, MereHostError> {
        let (key, _) = self
            .graph
            .get_node_by_url(address)
            .ok_or(AccessError::UnknownNode)?;
        Ok(access_history(&self.graph, key)?)
    }

    pub fn facet_value(&self, address: &str, facet: &str) -> Option<&Value> {
        let (_, node) = self.graph.get_node_by_url(address)?;
        self.graph.facets().get(&node.id, &FacetId::new(facet))
    }

    pub fn instance_for_address(&self, address: &str) -> Option<InstanceId> {
        self.instance_targets
            .iter()
            .position(|key| {
                self.graph
                    .get_node(*key)
                    .is_some_and(|node| node.url() == address)
            })
            .map(|index| InstanceId(index as u32))
    }

    pub fn local_request(&self) -> ProjectionRequest {
        ProjectionRequest {
            version: ProtocolVersion::V1,
            session: self.session(),
            score: self.score(),
        }
    }

    pub(crate) fn score(&self) -> Score {
        mere::canvas::project_canvas_strategy_with_score(
            "phyllotaxis.default",
            &self.graph,
            None,
            1280,
            720,
            None,
            None,
            true,
        )
        .score
        .unwrap_or_else(|| Score::new(Arrangement::Spiral(Default::default())))
    }

    pub(crate) fn set_facet(
        &mut self,
        key: NodeKey,
        facet: &str,
        value: Value,
    ) -> Result<(), MereHostError> {
        self.graph.get_node(key).ok_or(AccessError::UnknownNode)?;
        let updated = matches!(
            apply_graph_delta(
                &mut self.graph,
                GraphDelta::SetNodeFacet {
                    key,
                    facet: facet.to_string(),
                    value,
                },
            ),
            mere::kernel::graph::apply::GraphDeltaResult::NodeMetadataUpdated(true)
        );
        if updated {
            self.projection_revision = self.projection_revision.wrapping_add(1);
            self.dirty = true;
        }
        Ok(())
    }

    pub(crate) fn mutate_product_graph<R>(&mut self, mutate: impl FnOnce(&mut Graph) -> R) -> R {
        let result = mutate(&mut self.graph);
        self.projection_revision = self.projection_revision.wrapping_add(1);
        self.dirty = true;
        result
    }

    pub(crate) fn replace_product_graph(&mut self, graph: Graph) {
        self.graph = graph;
        self.projection_epoch = self.projection_epoch.wrapping_add(1);
        self.projection_revision = 1;
        self.resources.clear();
        self.instance_targets.clear();
        self.persisted_document = None;
        self.dirty = true;
    }

    fn build_snapshot(&mut self) -> Result<ProjectionSnapshot, MereHostError> {
        let layout = mere::canvas::project_canvas_strategy_with_score(
            "phyllotaxis.default",
            &self.graph,
            None,
            1280,
            720,
            None,
            None,
            true,
        );
        let mut scene = Scene::new();
        let mut presentation = PresentationManifest::default();
        let mut resources = BTreeMap::new();
        let mut instance_targets = Vec::with_capacity(layout.positions.len());
        let mut instance_of = HashMap::with_capacity(layout.positions.len());
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;

        for (index, (key, position)) in layout.positions.iter().copied().enumerate() {
            let node = self
                .graph
                .get_node(key)
                .expect("Mere canvas returned a key from this graph");
            let instance = InstanceId(index as u32);
            instance_targets.push(key);
            instance_of.insert(key, instance);
            min_x = min_x.min(position.x);
            min_y = min_y.min(position.y);
            max_x = max_x.max(position.x);
            max_y = max_y.max(position.y);

            let source = scene.intern_source(SourceRef::new("mere.graph", node.id.to_string()));
            scene.items.push(ProjectedItem {
                source,
                space: Scene::WORLD,
                transform: Transform2::translation(position.x, position.y),
                footprint: Footprint::Rect {
                    size: Size2::new(240.0, 112.0),
                },
                representation: Representation::Card,
                layer: 0,
                visible: true,
                hit: None,
                channels: Vec::new(),
            });

            let kind = node.primary_address().address_kind();
            let mut tags: Vec<String> = node.tags.iter().cloned().collect();
            tags.sort();
            let card = PortableCardV1 {
                title: node.title.clone(),
                values: vec![
                    CardValueV1 {
                        label: "Address".to_string(),
                        value: node.url().to_string(),
                    },
                    CardValueV1 {
                        label: "Kind".to_string(),
                        value: address_kind_label(kind).to_string(),
                    },
                    CardValueV1 {
                        label: "Accesses".to_string(),
                        value: access_history(&self.graph, key)?.records.len().to_string(),
                    },
                ],
                badges: tags,
                media: Vec::new(),
            };
            let bytes =
                serde_json::to_vec(&card).expect("PortableCardV1 fixture always serializes");
            let resource = ContentHash::of(&bytes);
            let key_ref = PresentationKey(format!("mere:{}", node.id));
            let mut semantics = PresentationSemantics {
                label: node.title.clone(),
                role: SemanticRole::Article,
                bounds: BoundsRelationship::FillFootprint,
                actions: Vec::new(),
            };
            self.handlers.attach_actions(&mut semantics, kind);
            presentation.bindings.push(PresentationBinding {
                instance,
                key: key_ref.clone(),
            });
            presentation.offers.insert(
                key_ref,
                vec![PresentationOffer {
                    codec: PresentationCodec::PortableCardV1,
                    resource,
                    byte_size: bytes.len() as u64,
                    requires: PresentationCapability::PortableCard,
                    semantics,
                }],
            );
            resources.insert(resource, bytes);
        }

        for relation in self.graph.relations() {
            let (Some(&from), Some(&to)) = (
                instance_of.get(&relation.from),
                instance_of.get(&relation.to),
            ) else {
                continue;
            };
            let from_position = layout.positions[from.0 as usize].1;
            let to_position = layout.positions[to.0 as usize].1;
            scene.relations.push(RoutedRelation {
                from,
                to,
                space: Scene::WORLD,
                points: vec![
                    Vec2::new(from_position.x, from_position.y),
                    Vec2::new(to_position.x, to_position.y),
                ],
                kind: Some(relation_kind_label(relation.kind).to_string()),
                weight: Some(1.0),
            });
        }

        scene.bounds = if layout.positions.is_empty() {
            Rect::new(Vec2::new(0.0, 0.0), Size2::new(0.0, 0.0))
        } else {
            Rect::new(
                Vec2::new(min_x - 120.0, min_y - 56.0),
                Size2::new(max_x - min_x + 240.0, max_y - min_y + 112.0),
            )
        };
        scene.generation = self.projection_revision;
        let scene = SceneSnapshot::from_dense(
            SceneEpoch(self.projection_epoch),
            Revision(self.projection_revision),
            scene,
        )
        .map_err(|error| MereHostError::InvalidSnapshot(format!("{error:?}")))?;

        self.resources = resources;
        self.instance_targets = instance_targets;
        Ok(ProjectionSnapshot {
            version: ProtocolVersion::V1,
            session: self.session(),
            scene,
            presentation,
            cache_policy: CachePolicy::default(),
        })
    }
}

impl<B: Backend> ProjectionCatalog for MereHost<B> {
    fn describe(&self) -> EndpointDescriptor {
        EndpointDescriptor {
            label: "Local Mere graph".to_string(),
            projections: vec![ProjectionOffer {
                label: "Current graph".to_string(),
                request: self.local_request(),
            }],
        }
    }
}

impl<B: Backend> ProjectionSource for MereHost<B> {
    type Error = MereHostError;

    fn snapshot(&mut self, request: ProjectionRequest) -> Result<ProjectionSnapshot, Self::Error> {
        if request.session != self.session() || request.version.major != ProtocolVersion::V1.major {
            return Err(MereHostError::WrongSession);
        }
        self.build_snapshot()
    }
}

impl<B: Backend> PresentationSource for MereHost<B> {
    type Error = MereHostError;

    fn resource(&mut self, request: ResourceRequest) -> Result<ResourceResponse, Self::Error> {
        if request.session != self.session() {
            return Err(MereHostError::WrongSession);
        }
        let bytes = self
            .resources
            .get(&request.resource)
            .cloned()
            .ok_or(MereHostError::MissingResource)?;
        Ok(ResourceResponse {
            session: request.session,
            resource: request.resource,
            bytes,
        })
    }
}

impl<B: Backend> IntentSink for MereHost<B> {
    type Error = MereHostError;

    fn invoke(&mut self, intent: IntentInvocation) -> Result<IntentResult, Self::Error> {
        if intent.session != self.session() {
            return Err(MereHostError::WrongSession);
        }
        if intent.observed_epoch != SceneEpoch(self.projection_epoch)
            || intent.observed_revision != Revision(self.projection_revision)
        {
            return Ok(IntentResult::Stale {
                current_epoch: SceneEpoch(self.projection_epoch),
                current_revision: Revision(self.projection_revision),
            });
        }
        let Some(&target) = self.instance_targets.get(intent.target.0 as usize) else {
            return Ok(IntentResult::Rejected {
                reason: "intent target is not in the disclosed scene".to_string(),
            });
        };
        let Some(handler_id) = handler_from_intent(&intent.intent) else {
            return Ok(IntentResult::Rejected {
                reason: "intent was not advertised by this endpoint".to_string(),
            });
        };
        let Some(handler) = self.handlers.get(handler_id) else {
            return Ok(IntentResult::Rejected {
                reason: "selected handler is unavailable".to_string(),
            });
        };
        let payload: OpenAddressV1 = match serde_json::from_slice(&intent.payload) {
            Ok(payload) => payload,
            Err(error) => {
                return Ok(IntentResult::Rejected {
                    reason: format!("open payload is invalid: {error}"),
                });
            }
        };
        let node = self
            .graph
            .get_node(target)
            .expect("disclosed target remains in the graph");
        if payload.handler != handler.id
            || intent.intent != intent_id(&payload.handler)
            || payload.address != node.url()
            || !handler.supports(node.primary_address().address_kind())
        {
            return Ok(IntentResult::Rejected {
                reason: "open payload does not match the advertised target".to_string(),
            });
        }
        record_access(
            &mut self.graph,
            target,
            &self.access_context,
            &payload.handler,
        )?;
        self.projection_revision = self.projection_revision.wrapping_add(1);
        self.dirty = true;
        Ok(IntentResult::Accepted)
    }
}

fn address_kind_label(kind: mere::kernel::address::AddressKind) -> &'static str {
    use mere::kernel::address::AddressKind;
    match kind {
        AddressKind::Http => "web",
        AddressKind::File => "file",
        AddressKind::Data => "data",
        AddressKind::GraphshellClip => "clip",
        AddressKind::Directory => "directory",
        AddressKind::Unknown => "custom",
    }
}

fn relation_kind_label(kind: RelationKind) -> &'static str {
    match kind {
        RelationKind::Semantic(_) => "semantic",
        RelationKind::Traversal => "traversal",
        RelationKind::Containment(_) => "containment",
        RelationKind::Arrangement(_) => "arrangement",
        RelationKind::Imported(_) => "imported",
        RelationKind::Provenance(_) => "provenance",
    }
}

/// The H1 fixture's two explicit handler choices.
pub fn fixture_handlers() -> HandlerRegistry {
    use mere::kernel::address::AddressKind;
    HandlerRegistry::new(vec![
        HandlerOffer {
            id: "graphshell.inspect".to_string(),
            label: "Inspect in Graphshell".to_string(),
            explanation: "Keep the address in the graph portal.".to_string(),
            address_kinds: vec![
                AddressKind::Http,
                AddressKind::File,
                AddressKind::Data,
                AddressKind::GraphshellClip,
                AddressKind::Directory,
                AddressKind::Unknown,
            ],
        },
        HandlerOffer {
            id: "system.default".to_string(),
            label: "Open in another application".to_string(),
            explanation: "Ask the native host to hand this address to a selected application."
                .to_string(),
            address_kinds: vec![
                AddressKind::Http,
                AddressKind::File,
                AddressKind::Directory,
                AddressKind::Unknown,
            ],
        },
    ])
}
