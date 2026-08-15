//! Daily graph-product operations composed from Mere's portable graph and canvas surfaces.

use std::collections::{BTreeSet, HashSet};

use chartulary::AcceptAll;
use chirograph::PortableCardV1;
use mere::canvas::CartographyGeometry;
use mere::kernel::geometry::PortablePoint;
use mere::kernel::graph::apply::{GraphDelta, add_node, apply_graph_delta, assert_relation};
use mere::kernel::graph::{
    ArrangementSubKind, ContainmentSubKind, EdgeAssertion, EdgeFamily, Graph, NodeFacetStore,
    RelationKind, SemanticSubKind,
};
use mere::kernel::persistence::GraphSnapshot;
use muniment::Backend;
use sceno::SourceRef;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::mere_host::{MereHost, MereHostError};

pub const LOCAL_FILE_FACET: &str = "graphshell.local-file/v1";
pub const CONTENT_FACET: &str = "graphshell.content/v1";
pub const SAVED_SCENE_FACET: &str = "graphshell.saved-scene/v2";
pub const PINNED_PROJECTION_FACET: &str = "graphshell.pinned-projection/v1";
pub const PRODUCT_ENGRAM_SCHEMA: &str = "graphshell.graph-engram/v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransferScope {
    ObjectOnly,
    DirectRelations,
    SelectedSubgraph,
    SavedScene,
}

impl TransferScope {
    pub fn code(self) -> &'static str {
        match self {
            Self::ObjectOnly => "object-only",
            Self::DirectRelations => "direct-relations",
            Self::SelectedSubgraph => "selected-subgraph",
            Self::SavedScene => "saved-scene",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "object-only" => Some(Self::ObjectOnly),
            "direct-relations" => Some(Self::DirectRelations),
            "selected-subgraph" => Some(Self::SelectedSubgraph),
            "saved-scene" => Some(Self::SavedScene),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelationFamilyFilter {
    All,
    Semantic,
    Traversal,
    Containment,
    Arrangement,
    Imported,
    Provenance,
}

impl RelationFamilyFilter {
    pub fn code(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Semantic => "semantic",
            Self::Traversal => "traversal",
            Self::Containment => "containment",
            Self::Arrangement => "arrangement",
            Self::Imported => "imported",
            Self::Provenance => "provenance",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "all" => Some(Self::All),
            "semantic" => Some(Self::Semantic),
            "traversal" => Some(Self::Traversal),
            "containment" => Some(Self::Containment),
            "arrangement" => Some(Self::Arrangement),
            "imported" => Some(Self::Imported),
            "provenance" => Some(Self::Provenance),
            _ => None,
        }
    }

    fn accepts(self, family: EdgeFamily) -> bool {
        self == Self::All
            || matches!(
                (self, family),
                (Self::Semantic, EdgeFamily::Semantic)
                    | (Self::Traversal, EdgeFamily::Traversal)
                    | (Self::Containment, EdgeFamily::Containment)
                    | (Self::Arrangement, EdgeFamily::Arrangement)
                    | (Self::Imported, EdgeFamily::Imported)
                    | (Self::Provenance, EdgeFamily::Provenance)
            )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EditableRelation {
    UserGrouped,
    Cites,
    Hyperlink,
    CollectionMember,
    FrameMember,
}

impl EditableRelation {
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "user-grouped" => Some(Self::UserGrouped),
            "cites" => Some(Self::Cites),
            "hyperlink" => Some(Self::Hyperlink),
            "collection-member" => Some(Self::CollectionMember),
            "frame-member" => Some(Self::FrameMember),
            _ => None,
        }
    }

    fn assertion(self) -> EdgeAssertion {
        match self {
            Self::UserGrouped => EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::UserGrouped,
                label: None,
                decay_progress: None,
            },
            Self::Cites => EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::Cites,
                label: None,
                decay_progress: None,
            },
            Self::Hyperlink => EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::Hyperlink,
                label: None,
                decay_progress: None,
            },
            Self::CollectionMember => EdgeAssertion::Containment {
                sub_kind: ContainmentSubKind::CollectionMember,
            },
            Self::FrameMember => EdgeAssertion::Arrangement {
                sub_kind: ArrangementSubKind::FrameMember,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalFileMetadata {
    pub content_hash: String,
    pub name: String,
    pub media_type: String,
    pub byte_len: u64,
    pub last_modified_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedSceneV1 {
    pub name: String,
    pub selected: Vec<Uuid>,
    pub layout_strategy: Option<String>,
    pub physics_paused: bool,
    pub physics_damping: f32,
    pub arrangement_pull: f32,
    pub camera_offset: (f32, f32),
    pub camera_zoom: f32,
    pub default_handler: String,
    pub cartography: CartographyGeometry,
}

/// User-selected public card copied from a mounted endpoint into local graph
/// truth. The source remains authoritative. Actions are deliberately absent,
/// because they are valid only in the live admitted session that advertised
/// them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PinnedProjectionAuthorityV1 {
    SourceOwned,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedProjectionCardV1 {
    pub source: SourceRef,
    pub observed_session: String,
    pub observed_epoch: u64,
    pub observed_revision: u64,
    pub authority: PinnedProjectionAuthorityV1,
    pub card: PortableCardV1,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProductEngramV1 {
    pub schema: String,
    pub scope: TransferScope,
    pub exported_at_ms: u64,
    pub graph: GraphSnapshot,
    pub facets: NodeFacetStore,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<SavedSceneV1>,
}

#[derive(Clone, Debug)]
pub struct ExportRequest {
    pub focused: Uuid,
    pub selected: Vec<Uuid>,
    pub scope: TransferScope,
    pub exported_at_ms: u64,
    pub include_local_file_locations: bool,
    pub scene: Option<SavedSceneV1>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImportReceipt {
    pub nodes: usize,
    pub relations: usize,
    pub facets: usize,
}

#[derive(Debug)]
pub enum ProductError {
    Host(MereHostError),
    UnknownNode(Uuid),
    UnknownAddress(String),
    InvalidFacetJson(String),
    EmptySelection,
    InvalidEngram(String),
}

impl std::fmt::Display for ProductError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Host(error) => write!(formatter, "{error}"),
            Self::UnknownNode(id) => write!(formatter, "unknown graph object {id}"),
            Self::UnknownAddress(address) => write!(formatter, "unknown address {address}"),
            Self::InvalidFacetJson(error) => write!(formatter, "invalid facet JSON: {error}"),
            Self::EmptySelection => write!(formatter, "the transfer scope contains no objects"),
            Self::InvalidEngram(error) => write!(formatter, "invalid graph engram: {error}"),
        }
    }
}

impl std::error::Error for ProductError {}

impl From<MereHostError> for ProductError {
    fn from(value: MereHostError) -> Self {
        Self::Host(value)
    }
}

impl<B: Backend> MereHost<B> {
    pub fn create_address(&mut self, address: &str, title: &str) -> Result<Uuid, ProductError> {
        if let Some((_, node)) = self.graph().get_node_by_url(address) {
            return Ok(node.id);
        }
        let id = Uuid::new_v4();
        self.mutate_product_graph(|graph| {
            let key = add_node(
                graph,
                Some(id),
                address.to_string(),
                PortablePoint::new(0.0, 0.0),
            );
            if !title.trim().is_empty() {
                apply_graph_delta(
                    graph,
                    GraphDelta::SetNodeTitle {
                        key,
                        title: title.trim().to_string(),
                    },
                );
            }
        });
        Ok(id)
    }

    pub fn create_file_metadata(
        &mut self,
        metadata: LocalFileMetadata,
    ) -> Result<Uuid, ProductError> {
        let address = format!("urn:sha256:{}", metadata.content_hash);
        let id = self.create_address(&address, &metadata.name)?;
        let key = self
            .graph()
            .get_node_key_by_id(id)
            .ok_or(ProductError::UnknownNode(id))?;
        self.mutate_product_graph(|graph| {
            apply_graph_delta(
                graph,
                GraphDelta::SetNodeMimeHint {
                    key,
                    mime_hint: (!metadata.media_type.is_empty())
                        .then_some(metadata.media_type.clone()),
                },
            );
        });
        self.set_facet(
            key,
            CONTENT_FACET,
            serde_json::json!({
                "sha256": metadata.content_hash,
                "byte_len": metadata.byte_len,
                "media_type": metadata.media_type,
            }),
        )?;
        self.set_facet(
            key,
            LOCAL_FILE_FACET,
            serde_json::json!({
                "name": metadata.name,
                "last_modified_ms": metadata.last_modified_ms,
                "source": "browser-file-picker",
            }),
        )?;
        Ok(id)
    }

    pub fn edit_node(
        &mut self,
        id: Uuid,
        title: &str,
        tags: impl IntoIterator<Item = String>,
    ) -> Result<(), ProductError> {
        let key = self
            .graph()
            .get_node_key_by_id(id)
            .ok_or(ProductError::UnknownNode(id))?;
        let wanted: BTreeSet<_> = tags
            .into_iter()
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
            .collect();
        let current: BTreeSet<_> = self
            .graph()
            .get_node(key)
            .expect("key resolved above")
            .tags
            .iter()
            .cloned()
            .collect();
        self.mutate_product_graph(|graph| {
            apply_graph_delta(
                graph,
                GraphDelta::SetNodeTitle {
                    key,
                    title: title.trim().to_string(),
                },
            );
            for tag in current.difference(&wanted) {
                apply_graph_delta(
                    graph,
                    GraphDelta::RemoveNodeTag {
                        key,
                        tag: tag.clone(),
                    },
                );
            }
            for tag in wanted.difference(&current) {
                apply_graph_delta(
                    graph,
                    GraphDelta::InsertNodeTag {
                        key,
                        tag: tag.clone(),
                    },
                );
            }
        });
        Ok(())
    }

    pub fn set_product_facet(
        &mut self,
        id: Uuid,
        facet: &str,
        json_value: &str,
    ) -> Result<(), ProductError> {
        let key = self
            .graph()
            .get_node_key_by_id(id)
            .ok_or(ProductError::UnknownNode(id))?;
        let value = serde_json::from_str(json_value)
            .map_err(|error| ProductError::InvalidFacetJson(error.to_string()))?;
        self.set_facet(key, facet.trim(), value)?;
        Ok(())
    }

    pub fn assert_product_relation(
        &mut self,
        from: Uuid,
        to: Uuid,
        relation: EditableRelation,
    ) -> Result<(), ProductError> {
        let from = self
            .graph()
            .get_node_key_by_id(from)
            .ok_or(ProductError::UnknownNode(from))?;
        let to = self
            .graph()
            .get_node_key_by_id(to)
            .ok_or(ProductError::UnknownNode(to))?;
        self.mutate_product_graph(|graph| {
            assert_relation(graph, from, to, relation.assertion());
        });
        Ok(())
    }

    pub fn matching_members(&self, query: &str, family: RelationFamilyFilter) -> Vec<Uuid> {
        let query = query.trim().to_lowercase();
        let related: HashSet<_> = self
            .graph()
            .relations()
            .filter(|relation| family.accepts(family_of(relation.kind)))
            .flat_map(|relation| [relation.from, relation.to])
            .collect();
        self.graph()
            .nodes()
            .filter(|(key, _)| family == RelationFamilyFilter::All || related.contains(key))
            .filter(|(_, node)| {
                if query.is_empty() {
                    return true;
                }
                node.title.to_lowercase().contains(&query)
                    || node.url().to_lowercase().contains(&query)
                    || node
                        .tags
                        .iter()
                        .any(|tag| tag.to_lowercase().contains(&query))
                    || self
                        .graph()
                        .facets()
                        .facets_of(&node.id)
                        .is_some_and(|facets| {
                            facets.iter().any(|(id, value)| {
                                id.as_str().to_lowercase().contains(&query)
                                    || value.to_string().to_lowercase().contains(&query)
                            })
                        })
            })
            .map(|(_, node)| node.id)
            .collect()
    }

    pub fn save_product_scene(
        &mut self,
        address: &str,
        scene: &SavedSceneV1,
    ) -> Result<Uuid, ProductError> {
        let id = self.create_address(address, &scene.name)?;
        let key = self
            .graph()
            .get_node_key_by_id(id)
            .ok_or(ProductError::UnknownNode(id))?;
        self.set_facet(key, SAVED_SCENE_FACET, serde_json::to_value(scene).unwrap())?;
        Ok(id)
    }

    pub fn product_scene(&self, address: &str) -> Result<SavedSceneV1, ProductError> {
        let value = self
            .facet_value(address, SAVED_SCENE_FACET)
            .ok_or_else(|| ProductError::UnknownAddress(address.to_string()))?;
        serde_json::from_value(value.clone())
            .map_err(|error| ProductError::InvalidEngram(error.to_string()))
    }

    pub fn export_product_engram(&self, request: ExportRequest) -> Result<Vec<u8>, ProductError> {
        let members = transfer_members(self.graph(), &request)?;
        if members.is_empty() {
            return Err(ProductError::EmptySelection);
        }
        let graph = filtered_snapshot(self.graph(), &members, request.exported_at_ms);
        let facets = filtered_facets(
            self.graph().facets(),
            &members,
            request.include_local_file_locations,
        );
        let scene = request.scene.map(|scene| filter_scene(scene, &members));
        serde_json::to_vec_pretty(&ProductEngramV1 {
            schema: PRODUCT_ENGRAM_SCHEMA.to_string(),
            scope: request.scope,
            exported_at_ms: request.exported_at_ms,
            graph,
            facets,
            scene,
        })
        .map_err(|error| ProductError::InvalidEngram(error.to_string()))
    }

    pub fn import_product_engram(&mut self, bytes: &[u8]) -> Result<ImportReceipt, ProductError> {
        let engram = decode_engram(bytes)?;
        let current_facets = self.graph().facets().clone();
        let mut merged = self.graph().to_snapshot();
        let mut known: HashSet<_> = merged
            .nodes
            .iter()
            .map(|node| node.node_id.clone())
            .collect();
        let mut imported_nodes = 0;
        for node in engram.graph.nodes {
            if known.insert(node.node_id.clone()) {
                merged.nodes.push(node);
                imported_nodes += 1;
            }
        }
        let imported_relations = engram.graph.edges.len();
        merged.edges.extend(engram.graph.edges);
        merged.timestamp_secs = engram.exported_at_ms / 1_000;
        let mut graph = Graph::from_snapshot(&merged);
        graph.overlay_facets(current_facets);
        let imported_facets = engram.facets.iter().map(|(_, facets)| facets.len()).sum();
        graph.overlay_facets(engram.facets);
        self.replace_product_graph(graph);
        Ok(ImportReceipt {
            nodes: imported_nodes,
            relations: imported_relations,
            facets: imported_facets,
        })
    }

    pub fn replace_with_product_engram(
        &mut self,
        bytes: &[u8],
    ) -> Result<(ImportReceipt, Option<SavedSceneV1>), ProductError> {
        let engram = decode_engram(bytes)?;
        let receipt = ImportReceipt {
            nodes: engram.graph.nodes.len(),
            relations: engram.graph.edges.len(),
            facets: engram.facets.iter().map(|(_, facets)| facets.len()).sum(),
        };
        let mut graph = Graph::from_snapshot(&engram.graph);
        graph.overlay_facets(engram.facets);
        self.replace_product_graph(graph);
        Ok((receipt, engram.scene))
    }
}

fn family_of(kind: RelationKind) -> EdgeFamily {
    match kind {
        RelationKind::Semantic(_) => EdgeFamily::Semantic,
        RelationKind::Traversal => EdgeFamily::Traversal,
        RelationKind::Containment(_) => EdgeFamily::Containment,
        RelationKind::Arrangement(_) => EdgeFamily::Arrangement,
        RelationKind::Imported(_) => EdgeFamily::Imported,
        RelationKind::Provenance(_) => EdgeFamily::Provenance,
    }
}

fn transfer_members(graph: &Graph, request: &ExportRequest) -> Result<HashSet<Uuid>, ProductError> {
    if graph.get_node_by_id(request.focused).is_none() {
        return Err(ProductError::UnknownNode(request.focused));
    }
    let mut members = HashSet::new();
    match request.scope {
        TransferScope::ObjectOnly => {
            members.insert(request.focused);
        }
        TransferScope::DirectRelations => {
            let focused = graph
                .get_node_key_by_id(request.focused)
                .expect("validated above");
            members.insert(request.focused);
            for relation in graph.relations() {
                if relation.from == focused || relation.to == focused {
                    if let Some(node) = graph.get_node(relation.from) {
                        members.insert(node.id);
                    }
                    if let Some(node) = graph.get_node(relation.to) {
                        members.insert(node.id);
                    }
                }
            }
        }
        TransferScope::SelectedSubgraph => {
            members.extend(
                request
                    .selected
                    .iter()
                    .copied()
                    .filter(|id| graph.get_node_by_id(*id).is_some()),
            );
        }
        TransferScope::SavedScene => {
            let scene = request.scene.as_ref().ok_or(ProductError::EmptySelection)?;
            members.extend(
                scene
                    .selected
                    .iter()
                    .copied()
                    .filter(|id| graph.get_node_by_id(*id).is_some()),
            );
        }
    }
    Ok(members)
}

fn filtered_snapshot(graph: &Graph, members: &HashSet<Uuid>, exported_at_ms: u64) -> GraphSnapshot {
    let mut snapshot = graph.to_snapshot();
    let ids: HashSet<_> = members.iter().map(Uuid::to_string).collect();
    snapshot.nodes.retain(|node| ids.contains(&node.node_id));
    snapshot
        .edges
        .retain(|edge| ids.contains(&edge.from_node_id) && ids.contains(&edge.to_node_id));
    snapshot.import_records.clear();
    snapshot.fields.clear();
    snapshot.couplings.clear();
    snapshot.navigation = Default::default();
    snapshot.timestamp_secs = exported_at_ms / 1_000;
    snapshot
}

fn filtered_facets(
    source: &NodeFacetStore,
    members: &HashSet<Uuid>,
    include_local_file_locations: bool,
) -> NodeFacetStore {
    let mut facets = NodeFacetStore::new();
    for (node, node_facets) in source.iter() {
        if !members.contains(node) {
            continue;
        }
        for (facet, value) in node_facets.iter() {
            if !include_local_file_locations && facet.as_str() == LOCAL_FILE_FACET {
                continue;
            }
            facets
                .set(node.to_owned(), facet.clone(), value.clone(), &AcceptAll)
                .expect("AcceptAll cannot reject an exported facet");
        }
    }
    facets
}

fn filter_scene(mut scene: SavedSceneV1, members: &HashSet<Uuid>) -> SavedSceneV1 {
    scene.selected.retain(|id| members.contains(id));
    scene.cartography = CartographyGeometry::from_positions(
        scene
            .cartography
            .iter()
            .filter(|(id, _)| members.contains(id)),
    )
    .with_sizes(
        scene
            .cartography
            .size_iter()
            .filter(|(id, _)| members.contains(id)),
    )
    .with_size_by_degree(scene.cartography.size_by_degree())
    .with_size_by_importance(scene.cartography.size_by_importance())
    .with_importance_metric(scene.cartography.importance_metric())
    .with_sprites(
        scene
            .cartography
            .sprite_iter()
            .filter(|(id, _)| members.contains(id))
            .map(|(id, uri)| (id, uri.to_string())),
    )
    .with_sprite_hulls(
        scene
            .cartography
            .sprite_hull_iter()
            .filter(|(id, _)| members.contains(id)),
    )
    .with_materials(
        scene
            .cartography
            .material_iter()
            .filter(|(id, _)| members.contains(id)),
    )
    .with_faces(
        scene
            .cartography
            .face_iter()
            .filter(|(id, _)| members.contains(id))
            .map(|(id, face)| (id, face.to_string())),
    );
    scene
}

pub(crate) fn decode_engram(bytes: &[u8]) -> Result<ProductEngramV1, ProductError> {
    let engram: ProductEngramV1 = serde_json::from_slice(bytes)
        .map_err(|error| ProductError::InvalidEngram(error.to_string()))?;
    if engram.schema != PRODUCT_ENGRAM_SCHEMA {
        return Err(ProductError::InvalidEngram(format!(
            "expected {PRODUCT_ENGRAM_SCHEMA}, found {}",
            engram.schema
        )));
    }
    let ids: HashSet<_> = engram
        .graph
        .nodes
        .iter()
        .map(|node| node.node_id.as_str())
        .collect();
    if engram.graph.edges.iter().any(|edge| {
        !ids.contains(edge.from_node_id.as_str()) || !ids.contains(edge.to_node_id.as_str())
    }) {
        return Err(ProductError::InvalidEngram(
            "a relation names an object outside the engram".to_string(),
        ));
    }
    Ok(engram)
}

#[cfg(test)]
mod tests {
    use mere::kernel::graph::{ProvenanceSubKind, RelationKind};
    use muniment::MemoryBackend;

    use super::*;
    use crate::access::AccessContext;
    use crate::mere_host::{
        FIXTURE_DEVICE_TWO_ADDRESS, FIXTURE_GRANT_ADDRESS, FIXTURE_PERSONA_ADDRESS,
        FIXTURE_RECEIPT_ADDRESS, FIXTURE_WEB_ADDRESS, SelectedPersonaRef, fixture_handlers,
    };

    fn selected_persona() -> SelectedPersonaRef {
        SelectedPersonaRef {
            persona: FIXTURE_PERSONA_ADDRESS.to_string(),
            profile: "profile:graphshell-h3".to_string(),
        }
    }

    #[test]
    fn h3_mixed_graph_scene_and_selected_engram_round_trip() {
        let mut host =
            MereHost::fixture(MemoryBackend::new(), selected_persona(), fixture_handlers())
                .expect("fixture");
        let file = host
            .create_file_metadata(LocalFileMetadata {
                content_hash: "ab".repeat(32),
                name: "radio-plan.unknown".to_string(),
                media_type: "application/x-unknown".to_string(),
                byte_len: 413,
                last_modified_ms: 42,
            })
            .expect("file");
        host.edit_node(
            file,
            "Radio plan",
            ["transport".to_string(), "unknown-file".to_string()],
        )
        .expect("edit");
        host.set_product_facet(
            file,
            "example.notes/v1",
            r#"{"status":"inspect-metadata-only"}"#,
        )
        .expect("facet");
        let web = host
            .graph()
            .get_node_by_url(FIXTURE_WEB_ADDRESS)
            .unwrap()
            .1
            .id;
        host.assert_product_relation(file, web, EditableRelation::Cites)
            .expect("relation");

        let receipt = host
            .graph()
            .get_node_by_url(FIXTURE_RECEIPT_ADDRESS)
            .unwrap()
            .1
            .id;
        let grant = host
            .graph()
            .get_node_by_url(FIXTURE_GRANT_ADDRESS)
            .unwrap()
            .1
            .id;
        let selected = vec![file, web, receipt, grant];
        let geometry = CartographyGeometry::from_positions([
            (file, (10.0, 20.0)),
            (web, (30.0, 40.0)),
            (receipt, (50.0, 60.0)),
            (grant, (70.0, 80.0)),
        ])
        .with_sprites([(file, "data:image/png;base64,AA==".to_string())])
        .with_faces([(file, "sprite".to_string()), (web, "bare".to_string())]);
        let scene = SavedSceneV1 {
            name: "Transport research".to_string(),
            selected: selected.clone(),
            layout_strategy: Some("grid.default".to_string()),
            physics_paused: true,
            physics_damping: 0.7,
            arrangement_pull: 0.4,
            camera_offset: (123.0, 234.0),
            camera_zoom: 1.2,
            default_handler: "system.default".to_string(),
            cartography: geometry,
        };
        host.save_product_scene("mere://scene/h3-test", &scene)
            .expect("save scene");
        assert_eq!(
            host.product_scene("mere://scene/h3-test")
                .expect("open scene"),
            scene
        );
        assert_eq!(
            host.matching_members("unknown-file", RelationFamilyFilter::Semantic),
            vec![file]
        );

        let bytes = host
            .export_product_engram(ExportRequest {
                focused: file,
                selected: selected.clone(),
                scope: TransferScope::SelectedSubgraph,
                exported_at_ms: 1_700_000_000_000,
                include_local_file_locations: false,
                scene: Some(scene.clone()),
            })
            .expect("export");
        let json = String::from_utf8(bytes.clone()).unwrap();
        assert!(!json.contains(LOCAL_FILE_FACET));
        assert!(json.contains(CONTENT_FACET));

        let mut reopened = MereHost::empty(
            MemoryBackend::new(),
            selected_persona(),
            fixture_handlers(),
            AccessContext {
                persona: FIXTURE_PERSONA_ADDRESS.to_string(),
                device: FIXTURE_DEVICE_TWO_ADDRESS.to_string(),
                at_ms: 100,
            },
        );
        let imported = reopened.import_product_engram(&bytes).expect("import");
        assert_eq!(imported.nodes, 4);
        assert_eq!(reopened.graph().node_count(), 4);
        for id in selected {
            assert!(
                reopened.graph().get_node_by_id(id).is_some(),
                "{id} survived"
            );
        }
        assert_eq!(
            reopened
                .facet_value(
                    "urn:sha256:abababababababababababababababababababababababababababababababab",
                    CONTENT_FACET
                )
                .unwrap()["byte_len"],
            413
        );
        assert!(
            reopened
                .graph()
                .relations()
                .any(|relation| relation.kind == RelationKind::Semantic(SemanticSubKind::Cites))
        );
        assert!(reopened.graph().relations().any(|relation| {
            relation.kind == RelationKind::Provenance(ProvenanceSubKind::GeneratedFrom)
        }));

        let (_, imported_scene) = reopened
            .replace_with_product_engram(&bytes)
            .expect("open as graph");
        let imported_scene = imported_scene.expect("scene");
        assert_eq!(imported_scene.selected.len(), 4);
        assert_eq!(imported_scene.cartography.sprite_iter().count(), 1);
        assert_eq!(imported_scene.cartography.face_iter().count(), 2);
    }
}
