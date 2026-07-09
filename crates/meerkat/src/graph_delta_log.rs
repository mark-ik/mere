/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Env-gated graph-delta session logging.

use std::fs::File;
use std::io::Write;
#[cfg(test)]
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use kernel::graph::GraphJournal;
use kernel::graph::capture::{CapturedDelta, set_captured_delta_hook};
#[cfg(test)]
use kernel::graph::Graph;

#[derive(Clone, Default)]
pub(crate) struct GraphDeltaLog {
    inner: Option<Arc<GraphDeltaLogInner>>,
}

struct GraphDeltaLogInner {
    file: Mutex<File>,
    path: PathBuf,
    entry_count: AtomicU64,
    byte_count: AtomicU64,
    /// The session's captured edits as the kernel's edit-spine type. The
    /// `.postcardlog` file is the crash-safe streaming transport; this is the
    /// ordered in-memory log a host replays, forks, or persists through the
    /// substrate (codicil / muniment). Streaming stays the durable path until
    /// codicil grows append-friendly, per-entry persistence (its own roadmap);
    /// today `Codicil::save` rewrites the whole log per call.
    journal: Mutex<GraphJournal>,
}

impl GraphDeltaLog {
    pub(crate) fn from_env() -> Self {
        let Some(dir) = std::env::var_os("MERE_GRAPH_DELTA_LOG").map(PathBuf::from) else {
            return Self::default();
        };
        Self::for_dir(dir)
    }

    pub(crate) fn install_hook(&self) {
        let hook = self.inner.as_ref().map(|inner| {
            let inner = inner.clone();
            Arc::new(move |delta: &CapturedDelta| inner.append(delta)) as _
        });
        set_captured_delta_hook(hook);
    }

    pub(crate) fn entry_count(&self) -> u64 {
        self.inner
            .as_ref()
            .map(|inner| inner.entry_count.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    pub(crate) fn byte_count(&self) -> u64 {
        self.inner
            .as_ref()
            .map(|inner| inner.byte_count.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    pub(crate) fn enabled(&self) -> bool {
        self.inner.is_some()
    }

    #[allow(dead_code)]
    pub(crate) fn path(&self) -> Option<&Path> {
        self.inner.as_ref().map(|inner| inner.path.as_path())
    }

    /// The session's captured edits as a [`GraphJournal`] (the kernel's edit-spine
    /// type): the ordered log this session recorded, replayable into a graph and
    /// forkable/persistable through the substrate. Empty when logging is disabled.
    #[allow(dead_code)]
    pub(crate) fn journal(&self) -> GraphJournal {
        self.inner
            .as_ref()
            .and_then(|inner| inner.journal.lock().ok().map(|journal| journal.clone()))
            .unwrap_or_default()
    }

    fn for_dir(dir: PathBuf) -> Self {
        if let Err(err) = std::fs::create_dir_all(&dir) {
            tracing::warn!(%err, dir = ?dir, "graph-delta log directory unavailable; capture disabled");
            return Self::default();
        }
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let pid = std::process::id();
        let path = dir.join(format!("mere-graph-deltas-{stamp}-{pid}.postcardlog"));
        let file = match File::create(&path) {
            Ok(file) => file,
            Err(err) => {
                tracing::warn!(%err, path = ?path, "graph-delta log file unavailable; capture disabled");
                return Self::default();
            }
        };
        Self {
            inner: Some(Arc::new(GraphDeltaLogInner {
                file: Mutex::new(file),
                path,
                entry_count: AtomicU64::new(0),
                byte_count: AtomicU64::new(0),
                journal: Mutex::new(GraphJournal::new()),
            })),
        }
    }
}

impl GraphDeltaLogInner {
    fn append(&self, delta: &CapturedDelta) {
        // The in-memory edit spine: the ordered log a host reconstructs the graph
        // from, alongside the crash-safe streaming file below.
        if let Ok(mut journal) = self.journal.lock() {
            journal.record(delta.clone());
        }
        let bytes = match postcard::to_allocvec(delta) {
            Ok(bytes) => bytes,
            Err(err) => {
                tracing::warn!(%err, "graph-delta capture encode failed");
                return;
            }
        };
        let len = bytes.len() as u32;
        let mut frame = Vec::with_capacity(4 + bytes.len());
        frame.extend_from_slice(&len.to_le_bytes());
        frame.extend_from_slice(&bytes);
        let mut file = match self.file.lock() {
            Ok(file) => file,
            Err(err) => {
                tracing::warn!("graph-delta log mutex poisoned: {err}");
                return;
            }
        };
        if let Err(err) = file.write_all(&frame).and_then(|_| file.flush()) {
            tracing::warn!(%err, path = ?self.path, "graph-delta capture write failed");
            return;
        }
        self.entry_count.fetch_add(1, Ordering::Relaxed);
        self.byte_count
            .fetch_add(frame.len() as u64, Ordering::Relaxed);
    }
}

#[cfg(test)]
pub(crate) fn read_delta_log(path: &Path) -> io::Result<GraphJournal> {
    let mut file = File::open(path)?;
    let mut journal = GraphJournal::new();
    loop {
        let mut len_bytes = [0u8; 4];
        match file.read_exact(&mut len_bytes) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(err) => return Err(err),
        }
        let len = u32::from_le_bytes(len_bytes) as usize;
        let mut payload = vec![0u8; len];
        file.read_exact(&mut payload)?;
        let delta: CapturedDelta = postcard::from_bytes(&payload).map_err(io::Error::other)?;
        journal.record(delta);
    }
    Ok(journal)
}

#[cfg(test)]
pub(crate) fn replay_delta_log(path: &Path) -> io::Result<Graph> {
    read_delta_log(path).map(|journal| journal.replay())
}

#[cfg(test)]
mod tests {
    use super::*;

    use euclid::default::Point2D;
    use kernel::graph::Graph;
    use kernel::graph::apply::{GraphDelta, add_node, apply_graph_delta, assert_relation};
    use kernel::graph::{
        Coupling, CouplingId, CouplingResponse, EdgeAssertion, Field, FieldDefinition, FieldExtent,
        FieldId, NavigationTrigger, NodeSelector, ProvenanceSubKind, ScalarField, SemanticSubKind,
    };
    use kernel::persistence::GraphSnapshot;
    use kernel::types::{
        BadgeIcon, ClassificationProvenance, ClassificationScheme, ClassificationStatus,
        FrameLayoutHint, NodeClassification, NodeDerivation, NodeImportProvenance, NodeProperty,
        SplitOrientation,
    };

    fn normalized_snapshot_json(graph: &Graph) -> serde_json::Value {
        let mut snapshot: GraphSnapshot = graph.to_snapshot();
        snapshot.timestamp_secs = 0;
        snapshot
            .nodes
            .sort_by(|left, right| left.node_id.cmp(&right.node_id));
        snapshot.edges.sort_by(|left, right| {
            left.from_node_id
                .cmp(&right.from_node_id)
                .then_with(|| left.to_node_id.cmp(&right.to_node_id))
        });
        snapshot
            .fields
            .sort_by(|left, right| left.id.cmp(&right.id));
        snapshot.couplings.sort_by(|left, right| {
            left.field_id
                .cmp(&right.field_id)
                .then_with(|| left.id.cmp(&right.id))
        });
        snapshot.import_records.sort_by(|left, right| {
            left.source_label
                .cmp(&right.source_label)
                .then_with(|| left.imported_at_secs.cmp(&right.imported_at_secs))
                .then_with(|| left.record_id.cmp(&right.record_id))
        });
        for record in &mut snapshot.import_records {
            record.memberships.sort_by(|left, right| {
                left.node_id
                    .cmp(&right.node_id)
                    .then_with(|| left.suppressed.cmp(&right.suppressed))
            });
        }
        let mut nav_snapshot = snapshot.navigation.snapshot().clone();
        for owner in &mut nav_snapshot.owners {
            owner.owned_visits.sort_unstable();
        }
        for visit in &mut nav_snapshot.visits {
            visit.bindings.sort_by_key(|binding| {
                (
                    binding.owner,
                    binding.forward_child.unwrap_or(usize::MAX),
                    binding.last_accessed_at_ms,
                )
            });
        }
        snapshot.navigation = kernel::graph::SharedNavigationMemory::from_snapshot(nav_snapshot);
        let mut value = serde_json::to_value(snapshot).expect("snapshot json");
        // Blank the wall-clock / minted fields that legitimately differ between the
        // original graph and one replayed from the delta log, so the comparison is
        // about the *logical* graph, not mint history (the same reason
        // `timestamp_secs` is zeroed above):
        //   - `statement_id`: minted per assertion (device + time + sequence), so a
        //     re-asserted statement gets a fresh id on replay.
        //   - `last_visited_ms`: `TouchNodeLastVisited` carries no timestamp, so it
        //     stamps apply-time; replay re-stamps at replay-time.
        // (petgraph-RDF statement-aware writes + delta round-trip.) See the report:
        // whether replay *should* preserve `last_visited_ms` for crash-recovery
        // fidelity is a delta-capture design question, flagged to that owner.
        normalize_nondeterministic_fields(&mut value);
        value
    }

    /// Recursively blank the field values that are non-deterministic across a
    /// delta-log round-trip, so two snapshots compare on logical content.
    fn normalize_nondeterministic_fields(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                for (key, child) in map.iter_mut() {
                    match key.as_str() {
                        "statement_id" => {
                            *child = serde_json::Value::String("<normalized>".to_string());
                        }
                        "last_visited_ms" => *child = serde_json::Value::Null,
                        _ => normalize_nondeterministic_fields(child),
                    }
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    normalize_nondeterministic_fields(item);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn graph_delta_log_round_trips_and_replays() {
        let temp = tempfile::tempdir().expect("temp dir");
        let log = GraphDeltaLog::for_dir(temp.path().to_path_buf());
        log.install_hook();

        let mut graph = Graph::new();
        graph.set_current_session(77);
        let field_id = FieldId::from_uuid(uuid::Uuid::from_u128(51));
        let coupling_id = CouplingId::from_uuid(uuid::Uuid::from_u128(52));
        let a = add_node(
            &mut graph,
            Some(uuid::Uuid::from_u128(41)),
            "https://a.test".into(),
            Point2D::new(0.0, 0.0),
        );
        let b = add_node(
            &mut graph,
            Some(uuid::Uuid::from_u128(42)),
            "https://b.test".into(),
            Point2D::new(1.0, 0.0),
        );
        let c = add_node(
            &mut graph,
            Some(uuid::Uuid::from_u128(43)),
            "https://c.test".into(),
            Point2D::new(2.0, 0.0),
        );
        let _ = assert_relation(
            &mut graph,
            a,
            b,
            EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::Hyperlink,
                label: Some("next".into()),
                decay_progress: None,
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::AppendTraversal {
                from: a,
                to: b,
                trigger: NavigationTrigger::LinkClick,
                timestamp_ms: Some(12_345),
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodeTitle {
                key: a,
                title: "Alpha".into(),
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodeUrl {
                key: a,
                new_url: "https://a.test/next".into(),
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodeThumbnail {
                key: a,
                png_bytes: vec![0x89, b'P', b'N', b'G'],
                width: 1,
                height: 1,
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodeFavicon {
                key: a,
                rgba: vec![255, 0, 0, 255],
                width: 1,
                height: 1,
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodeMimeHint {
                key: a,
                mime_hint: Some("text/html".into()),
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodeViewerOverride {
                key: a,
                viewer_override: Some("viewer:note".into()),
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodePinned {
                key: a,
                is_pinned: true,
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodeCompatMode {
                key: a,
                compat_mode: true,
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::InsertNodeTag {
                key: a,
                tag: "research".into(),
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::RemoveNodeTag {
                key: a,
                tag: "research".into(),
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodeBody {
                key: a,
                body: Some("body".into()),
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodeFormDraft {
                key: a,
                form_draft: Some("draft body".into()),
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodeSessionScroll {
                key: a,
                session_scroll: Some((20.0, 640.0)),
            },
        );
        let _ = apply_graph_delta(&mut graph, GraphDelta::TouchNodeLastVisited { key: a });
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::InsertNodeTag {
                key: a,
                tag: "paper".into(),
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodeTagIconOverride {
                key: a,
                tag: "paper".into(),
                icon: Some(BadgeIcon::Lucide("file-text".into())),
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::NavigateNode {
                key: b,
                url: "https://b.test/one".into(),
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::NavigateNode {
                key: b,
                url: "https://b.test/two".into(),
            },
        );
        let _ = apply_graph_delta(&mut graph, GraphDelta::NodeHistoryBack { key: b });
        let _ = apply_graph_delta(&mut graph, GraphDelta::NodeHistoryForward { key: b });
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::BranchHistory {
                child: c,
                parent: b,
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::NavigateNode {
                key: c,
                url: "https://c.test/branched".into(),
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::AddField {
                field: Field::new(field_id, FieldDefinition::Scalar(ScalarField::Const(1.0)))
                    .with_name("focus")
                    .with_extent(FieldExtent::Region {
                        min_x: -10.0,
                        min_y: -20.0,
                        max_x: 30.0,
                        max_y: 40.0,
                    }),
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::AddCoupling {
                coupling: Coupling::new(
                    coupling_id,
                    field_id,
                    NodeSelector::Kind("paper".into()),
                    CouplingResponse::DampenInside { factor: 0.3 },
                    1.5,
                ),
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::SetFieldCouplingStrength {
                field: field_id,
                strength: 2.0,
            },
        );
        let _ = apply_graph_delta(&mut graph, GraphDelta::RetireField { id: field_id });
        let _ = apply_graph_delta(&mut graph, GraphDelta::ActivateField { id: field_id });
        let _ = apply_graph_delta(&mut graph, GraphDelta::RetractCoupling { id: coupling_id });
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::AddCoupling {
                coupling: Coupling::new(
                    coupling_id,
                    field_id,
                    NodeSelector::Kind("paper".into()),
                    CouplingResponse::DampenInside { factor: 0.3 },
                    1.5,
                ),
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::SetFieldCouplingStrength {
                field: field_id,
                strength: 2.0,
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::AppendNodeProperty {
                key: a,
                property: NodeProperty::new(
                    "https://schema.org/datePublished".into(),
                    "2026-07-02".into(),
                ),
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::AddNodeClassification {
                key: a,
                classification: NodeClassification {
                    scheme: ClassificationScheme::ContentKind,
                    value: "article".into(),
                    label: Some("Article".into()),
                    confidence: 1.0,
                    provenance: ClassificationProvenance::UserAuthored,
                    status: ClassificationStatus::Accepted,
                    primary: true,
                },
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::AddNodeClassification {
                key: a,
                classification: NodeClassification {
                    scheme: ClassificationScheme::ContentKind,
                    value: "essay".into(),
                    label: Some("Essay".into()),
                    confidence: 0.6,
                    provenance: ClassificationProvenance::AgentSuggested,
                    status: ClassificationStatus::Suggested,
                    primary: false,
                },
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::AddNodeClassification {
                key: a,
                classification: NodeClassification {
                    scheme: ClassificationScheme::ContentKind,
                    value: "draft".into(),
                    label: Some("Draft".into()),
                    confidence: 0.3,
                    provenance: ClassificationProvenance::AgentSuggested,
                    status: ClassificationStatus::Suggested,
                    primary: false,
                },
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodeClassificationStatus {
                key: a,
                scheme: ClassificationScheme::ContentKind,
                value: "article".into(),
                status: ClassificationStatus::Verified,
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodePrimaryClassification {
                key: a,
                scheme: ClassificationScheme::ContentKind,
                value: "essay".into(),
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::RemoveNodeClassification {
                key: a,
                scheme: ClassificationScheme::ContentKind,
                value: "draft".into(),
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::RecordNodeDerivation {
                key: a,
                derivation: NodeDerivation {
                    sub_kind: ProvenanceSubKind::ExtractedFrom,
                    source_node: uuid::Uuid::from_u128(99).to_string(),
                    source_graph: Some("graph:test".into()),
                },
            },
        );
        let ab_edge = graph.find_edge_key(a, b).expect("a->b edge");
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::SetEdgeSemanticPredicate {
                edge: ab_edge,
                predicate: Some("https://schema.org/author".into()),
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::AssertSemanticPredicate {
                from: b,
                to: c,
                predicate: "https://schema.org/citation".into(),
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::AppendFrameLayoutHint {
                key: a,
                hint: FrameLayoutHint::SplitHalf {
                    first: uuid::Uuid::from_u128(42).to_string(),
                    second: uuid::Uuid::from_u128(43).to_string(),
                    orientation: SplitOrientation::Vertical,
                },
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::AppendFrameLayoutHint {
                key: a,
                hint: FrameLayoutHint::SplitHalf {
                    first: uuid::Uuid::from_u128(43).to_string(),
                    second: uuid::Uuid::from_u128(42).to_string(),
                    orientation: SplitOrientation::Horizontal,
                },
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::MoveFrameLayoutHint {
                key: a,
                from_index: 0,
                to_index: 1,
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::RemoveFrameLayoutHint {
                key: a,
                hint_index: 1,
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::SetFrameSplitOfferSuppressed {
                key: a,
                suppressed: true,
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::UpdateNodeHistory {
                key: a,
                entries: vec![
                    "https://a.test/one".into(),
                    "https://a.test/two".into(),
                    "https://a.test/three".into(),
                ],
                current_index: 9,
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodeImportProvenance {
                key: a,
                import_provenance: vec![NodeImportProvenance {
                    source_id: "import:seed".into(),
                    source_label: "Seed import".into(),
                }],
            },
        );
        let _ = apply_graph_delta(
            &mut graph,
            GraphDelta::SetImportRecordMembershipSuppressed {
                record_id: "import-record:import:seed".into(),
                key: a,
                suppressed: true,
            },
        );

        let path = log.path().expect("log path").to_path_buf();
        let entries = read_delta_log(&path).expect("read log");
        // One captured delta per `apply_graph_delta` call above (plus the 3
        // `add_node` + 1 `assert_relation` setup deltas). The two semantic-predicate
        // deltas each capture once now that statement writes fold into the predicate
        // write (petgraph-RDF statement-aware writes), so the total is 53; the replay
        // assertions below prove no data is lost.
        assert_eq!(entries.len(), 53);
        let replayed = replay_delta_log(&path).expect("replay log");
        assert_eq!(replayed.node_count(), 3);
        assert_eq!(replayed.edge_count(), 2);
        let replayed_a = replayed
            .get_node_key_by_id(uuid::Uuid::from_u128(41))
            .expect("replayed a");
        let replayed_b = replayed
            .get_node_key_by_id(uuid::Uuid::from_u128(42))
            .expect("replayed b");
        let _replayed_c = replayed
            .get_node_key_by_id(uuid::Uuid::from_u128(43))
            .expect("replayed c");
        let edge = replayed
            .find_edge_key(replayed_a, replayed_b)
            .expect("replayed edge");
        let payload = replayed.get_edge(edge).expect("replayed payload");
        let node = replayed.get_node(replayed_a).expect("replayed node");
        assert_eq!(payload.traversals().len(), 1);
        assert_eq!(payload.metrics().total_navigations, 1);
        assert_eq!(payload.metrics().last_navigated_at, Some(12_345));
        assert_eq!(node.title, "Alpha");
        assert_eq!(node.url(), "https://a.test/next");
        assert_eq!(
            node.thumbnail_png.as_deref(),
            Some(&[0x89, b'P', b'N', b'G'][..])
        );
        assert_eq!(node.thumbnail_width, 1);
        assert_eq!(node.thumbnail_height, 1);
        assert_eq!(node.favicon_rgba.as_deref(), Some(&[255, 0, 0, 255][..]));
        assert_eq!(node.favicon_width, 1);
        assert_eq!(node.favicon_height, 1);
        assert_eq!(node.mime_hint.as_deref(), Some("text/html"));
        assert_eq!(node.viewer_override.as_deref(), Some("viewer:note"));
        assert!(node.is_pinned);
        assert!(node.compat_mode);
        assert_eq!(node.body.as_deref(), Some("body"));
        assert_eq!(node.session_form_draft.as_deref(), Some("draft body"));
        assert_eq!(node.session_scroll, Some((20.0, 640.0)));
        assert!(!node.tags.contains("research"));
        assert!(node.tags.contains("paper"));
        assert_eq!(
            node.tag_presentation.icon_overrides.get("paper"),
            Some(&BadgeIcon::Lucide("file-text".into()))
        );
        assert_eq!(node.properties.len(), 1);
        assert_eq!(
            node.properties[0].predicate,
            "https://schema.org/datePublished"
        );
        assert_eq!(node.classifications.len(), 2);
        assert!(node.classifications.iter().any(|classification| {
            classification.value == "article"
                && classification.status == ClassificationStatus::Verified
                && !classification.primary
        }));
        assert!(node.classifications.iter().any(|classification| {
            classification.value == "essay"
                && classification.status == ClassificationStatus::Suggested
                && classification.primary
        }));
        assert!(
            node.classifications
                .iter()
                .all(|classification| classification.value != "draft")
        );
        assert_eq!(node.derivations.len(), 1);
        assert_eq!(
            node.derivations[0].sub_kind,
            ProvenanceSubKind::ExtractedFrom
        );
        assert_eq!(
            payload
                .semantic_data()
                .and_then(|data| data.predicate.as_deref()),
            Some("https://schema.org/author")
        );
        let replayed_b_node = replayed.get_node(replayed_b).expect("replayed b node");
        assert_eq!(replayed_b_node.url(), "https://b.test/two");
        assert_eq!(replayed_b_node.last_session_visited, 77);
        let replayed_b_history = replayed.node_history_projection(replayed_b);
        assert_eq!(
            replayed_b_history.entries,
            vec![
                "https://b.test/one".to_string(),
                "https://b.test/two".to_string(),
            ]
        );
        assert_eq!(replayed_b_history.current_index, 1);
        let replayed_c = replayed
            .get_node_key_by_id(uuid::Uuid::from_u128(43))
            .expect("replayed c");
        let replayed_c_node = replayed.get_node(replayed_c).expect("replayed c node");
        assert_eq!(replayed_c_node.url(), "https://c.test/branched");
        assert_eq!(replayed_c_node.last_session_visited, 77);
        let semantic_edge = replayed
            .find_edge_key(replayed_b, replayed_c)
            .expect("replayed semantic edge");
        let semantic_payload = replayed
            .get_edge(semantic_edge)
            .expect("replayed semantic payload");
        assert_eq!(
            semantic_payload
                .semantic_data()
                .and_then(|data| data.predicate.as_deref()),
            Some("https://schema.org/citation")
        );
        let field = replayed.field(field_id).expect("replayed field");
        assert_eq!(field.name.as_deref(), Some("focus"));
        assert!(field.is_active());
        assert_eq!(
            field.extent,
            FieldExtent::Region {
                min_x: -10.0,
                min_y: -20.0,
                max_x: 30.0,
                max_y: 40.0,
            }
        );
        let coupling = replayed
            .couplings_for_field(field_id)
            .next()
            .expect("replayed coupling");
        assert_eq!(coupling.id, coupling_id);
        assert_eq!(coupling.selector, NodeSelector::Kind("paper".into()));
        assert_eq!(
            coupling.response,
            CouplingResponse::DampenInside { factor: 0.3 }
        );
        assert_eq!(coupling.strength, 2.0);
        let hints = replayed
            .frame_layout_hints(replayed_a)
            .expect("replayed frame hints");
        assert_eq!(hints.len(), 1);
        assert_eq!(
            hints[0],
            FrameLayoutHint::SplitHalf {
                first: uuid::Uuid::from_u128(43).to_string(),
                second: uuid::Uuid::from_u128(42).to_string(),
                orientation: SplitOrientation::Horizontal,
            }
        );
        assert_eq!(
            replayed.frame_split_offer_suppressed(replayed_a),
            Some(true)
        );
        let history = replayed.node_history_projection(replayed_a);
        assert_eq!(
            history.entries,
            vec![
                "https://a.test/one".to_string(),
                "https://a.test/two".to_string(),
                "https://a.test/three".to_string(),
            ]
        );
        assert_eq!(history.current_index, 2);
        let import_records = replayed.import_records();
        assert_eq!(import_records.len(), 1);
        assert_eq!(import_records[0].record_id, "import-record:import:seed");
        assert!(import_records[0].memberships[0].suppressed);
        assert!(
            replayed
                .node_import_provenance(replayed_a)
                .expect("replayed import provenance")
                .is_empty()
        );
        assert_eq!(
            normalized_snapshot_json(&replayed),
            normalized_snapshot_json(&graph)
        );

        set_captured_delta_hook(None);
    }
}
