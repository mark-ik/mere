// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Content-addressed, producer-owned recipes for reopening a curated live view.
//!
//! A live-view record names source truth by an opaque producer identifier plus
//! its cursor. It carries only the existing per-pane [`ViewIntent`] curation;
//! it does not contain graph truth, Canvas physics, or a Scenotime revision.
//! The source owner resolves the reference when a recipient opens the record,
//! and reports a missing source, stale cursor, or denied disclosure explicitly.

use eidetic::{
    BlobSource, Error, ManifestId, NoFetcher, PayloadSealer, PrivacyClass, ProvenanceOrigin,
    ProvenanceRecord, Result, SchemaRef, Store, Timestamp, TrustEnvelope, TypedPayload,
    load_typed_sealed, save_typed_sealed,
};
use forme::FOLD_RECORD_VERSION;
use serde::{Deserialize, Serialize};

use crate::ViewIntent;

/// First durable wire shape for a producer-owned live-view record.
pub const LIVE_VIEW_RECORD_VERSION: u16 = 1;
/// Schema identity bytes for [`LiveViewRecord`].
pub const LIVE_VIEW_RECORD_SCHEMA_ID: &[u8] = b"mere.live-view/v1";

/// Content-addressed schema reference for [`LiveViewRecord`].
pub fn live_view_schema_ref() -> SchemaRef {
    SchemaRef::from_id(ManifestId::of_blob(LIVE_VIEW_RECORD_SCHEMA_ID))
}

/// One source-owned source-time position. The cursor is intentionally opaque:
/// a journal may use a sequence while a Git authority may use a commit id.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveViewCursor {
    /// Follow the source's current truth when the recipient is authorized to
    /// read it.
    #[default]
    Live,
    /// Reproduce one source-owned historical checkpoint exactly.
    Historical { cursor: String },
}

/// A source reference owned by the producer, never an authority grant.
///
/// `provider` selects the source adapter and `scope` selects its graph or
/// projection. The same record can be carried through a remote host, but only
/// that source owner decides whether its scope and cursor can be disclosed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveViewSource {
    pub provider: String,
    pub scope: String,
    #[serde(default)]
    pub cursor: LiveViewCursor,
}

/// A versioned recipe for reopening one live source with durable local
/// curation. The record itself is an Eidetic `LocalOnly` object; it is not a
/// URL encoding and may therefore carry a private source reference behind the
/// caller's disclosure boundary.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LiveViewRecord {
    pub version: u16,
    pub source: LiveViewSource,
    pub view: ViewIntent,
}

impl TypedPayload for LiveViewRecord {
    fn schema_ref() -> SchemaRef {
        live_view_schema_ref()
    }
}

/// A malformed or unsupported live-view recipe. These failures happen before
/// a source adapter receives the record, so they never invite a partial view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveViewRecordError {
    UnsupportedVersion {
        found: u16,
    },
    MissingProvider,
    MissingScope,
    EmptyHistoricalCursor,
    UnsupportedFoldVersion {
        found: u16,
    },
    FoldScopeMismatch {
        fold_scope: String,
        source_scope: String,
    },
    InvalidFoldMembers,
}

impl std::fmt::Display for LiveViewRecordError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedVersion { found } => {
                write!(formatter, "unsupported live-view record version {found}")
            }
            Self::MissingProvider => write!(formatter, "live-view source provider is missing"),
            Self::MissingScope => write!(formatter, "live-view source scope is missing"),
            Self::EmptyHistoricalCursor => {
                write!(formatter, "live-view historical cursor is missing")
            }
            Self::UnsupportedFoldVersion { found } => {
                write!(formatter, "unsupported fold record version {found}")
            }
            Self::FoldScopeMismatch {
                fold_scope,
                source_scope,
            } => write!(
                formatter,
                "fold source scope {fold_scope:?} does not match live-view source scope {source_scope:?}"
            ),
            Self::InvalidFoldMembers => {
                write!(
                    formatter,
                    "live-view fold must contain sorted, distinct members"
                )
            }
        }
    }
}

impl std::error::Error for LiveViewRecordError {}

impl LiveViewRecord {
    /// Reject a record that would be ambiguous or could reapply a fold to a
    /// different source. Graph membership remains the source adapter's check.
    pub fn validate(&self) -> std::result::Result<(), LiveViewRecordError> {
        if self.version != LIVE_VIEW_RECORD_VERSION {
            return Err(LiveViewRecordError::UnsupportedVersion {
                found: self.version,
            });
        }
        if self.source.provider.trim().is_empty() {
            return Err(LiveViewRecordError::MissingProvider);
        }
        if self.source.scope.trim().is_empty() {
            return Err(LiveViewRecordError::MissingScope);
        }
        if matches!(self.source.cursor, LiveViewCursor::Historical { ref cursor } if cursor.trim().is_empty())
        {
            return Err(LiveViewRecordError::EmptyHistoricalCursor);
        }
        for fold in &self.view.folds {
            if fold.version != FOLD_RECORD_VERSION {
                return Err(LiveViewRecordError::UnsupportedFoldVersion {
                    found: fold.version,
                });
            }
            if fold.source_scope != self.source.scope {
                return Err(LiveViewRecordError::FoldScopeMismatch {
                    fold_scope: fold.source_scope.clone(),
                    source_scope: self.source.scope.clone(),
                });
            }
            if fold.members.len() < 2
                || fold
                    .members
                    .windows(2)
                    .any(|members| members[0] >= members[1])
            {
                return Err(LiveViewRecordError::InvalidFoldMembers);
            }
        }
        Ok(())
    }
}

/// The only answers a source owner may give when opening a shared recipe.
/// None of them means "show a redacted empty graph".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveViewSourceError {
    MissingSource,
    StaleCursor,
    AccessDenied,
    UnsupportedArrangement,
}

impl std::fmt::Display for LiveViewSourceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingSource => {
                write!(formatter, "the requested live-view source is unavailable")
            }
            Self::StaleCursor => write!(formatter, "the requested live-view cursor is unavailable"),
            Self::AccessDenied => write!(
                formatter,
                "the recipient is not authorized to read this live-view source"
            ),
            Self::UnsupportedArrangement => {
                write!(
                    formatter,
                    "the requested live-view arrangement is unsupported"
                )
            }
        }
    }
}

impl std::error::Error for LiveViewSourceError {}

/// A producer's authority boundary for resolving a live-view source reference.
pub trait LiveViewSourceResolver {
    type Snapshot;

    fn resolve_live_view_source(
        &mut self,
        source: &LiveViewSource,
    ) -> std::result::Result<Self::Snapshot, LiveViewSourceError>;
}

/// Save a validated local live-view record, returning its content address.
pub async fn save_live_view_record(
    store: &mut dyn Store,
    record: &LiveViewRecord,
    created_at: Timestamp,
) -> Result<ManifestId> {
    save_live_view_record_sealed(store, None, record, created_at).await
}

/// As [`save_live_view_record`], but seal local curation at rest when the
/// caller has a persona payload sealer. Disclosure remains a separate choice.
pub async fn save_live_view_record_sealed(
    store: &mut dyn Store,
    sealer: Option<&dyn PayloadSealer>,
    record: &LiveViewRecord,
    created_at: Timestamp,
) -> Result<ManifestId> {
    record
        .validate()
        .map_err(|error| Error::new(error.to_string()))?;
    save_typed_sealed(
        store,
        sealer,
        record,
        Vec::<BlobSource>::new(),
        PrivacyClass::LocalOnly,
        ProvenanceRecord {
            origin: ProvenanceOrigin::Generated,
            upstream: Vec::new(),
            tooling: Some(
                concat!("session-runtime/live-view@", env!("CARGO_PKG_VERSION")).to_string(),
            ),
            generated_at: created_at,
        },
        TrustEnvelope::self_asserted(),
        created_at,
    )
    .await
}

/// Load a local live-view record and validate it before exposing it to a
/// source resolver. `Ok(None)` means the record itself is absent.
pub async fn load_live_view_record(
    store: &mut dyn Store,
    id: ManifestId,
) -> Result<Option<LiveViewRecord>> {
    load_live_view_record_sealed(store, None, id).await
}

/// As [`load_live_view_record`], but unseals local curation with `sealer`.
pub async fn load_live_view_record_sealed(
    store: &mut dyn Store,
    sealer: Option<&dyn PayloadSealer>,
    id: ManifestId,
) -> Result<Option<LiveViewRecord>> {
    let mut fetcher = NoFetcher;
    let record = load_typed_sealed::<LiveViewRecord>(store, &mut fetcher, sealer, id).await?;
    record
        .as_ref()
        .map(LiveViewRecord::validate)
        .transpose()
        .map_err(|error| Error::new(error.to_string()))?;
    Ok(record)
}

/// Load a record and ask its source owner for the selected truth. The caller
/// receives the source owner's explicit refusal unchanged.
pub async fn open_live_view_record<R: LiveViewSourceResolver>(
    store: &mut dyn Store,
    id: ManifestId,
    resolver: &mut R,
) -> Result<Option<(LiveViewRecord, R::Snapshot)>> {
    let Some(record) = load_live_view_record(store, id).await? else {
        return Ok(None);
    };
    let snapshot = resolver
        .resolve_live_view_source(&record.source)
        .map_err(|error| Error::new(error.to_string()))?;
    Ok(Some((record, snapshot)))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use forme::FoldRecord;
    use muniment::MemoryBackend as InMemoryStore;
    use uuid::Uuid;

    use super::*;
    use crate::{CameraSnapshot, HiddenRelationRecord};

    const CREATED_AT: Timestamp = Timestamp(1_723_000_000_000);
    const SOURCE_SCOPE: &str = "session:fixture/graph:root";

    fn record() -> LiveViewRecord {
        let mut view = ViewIntent::new();
        view.hidden_relations = BTreeSet::from([HiddenRelationRecord::new(
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            3,
        )]);
        view.folds = vec![
            FoldRecord::from_selection(SOURCE_SCOPE, [Uuid::from_u128(2), Uuid::from_u128(1)])
                .expect("two members form a fold"),
        ];
        view.camera = Some(CameraSnapshot {
            coefficients: [1.0, 0.0, 0.0, 1.0, 20.0, -10.0],
        });
        view.focus = Some("https://example.test/field-notes".to_string());
        view.strategy = Some("timeline.updated".to_string());
        LiveViewRecord {
            version: LIVE_VIEW_RECORD_VERSION,
            source: LiveViewSource {
                provider: "mere.graph-journal/v1".to_string(),
                scope: SOURCE_SCOPE.to_string(),
                cursor: LiveViewCursor::Historical {
                    cursor: "17".to_string(),
                },
            },
            view,
        }
    }

    struct FixtureResolver {
        result: std::result::Result<&'static str, LiveViewSourceError>,
    }

    impl LiveViewSourceResolver for FixtureResolver {
        type Snapshot = &'static str;

        fn resolve_live_view_source(
            &mut self,
            source: &LiveViewSource,
        ) -> std::result::Result<Self::Snapshot, LiveViewSourceError> {
            assert_eq!(source.scope, SOURCE_SCOPE);
            self.result.clone()
        }
    }

    #[test]
    fn content_addressed_record_round_trips_curation_and_source_cursor() {
        pollster::block_on(async {
            let original = record();
            let mut left = InMemoryStore::default();
            let mut right = InMemoryStore::default();
            let left_id = save_live_view_record(&mut left, &original, CREATED_AT)
                .await
                .expect("save local record");
            let right_id = save_live_view_record(&mut right, &original, CREATED_AT)
                .await
                .expect("same deterministic record addresses identically");
            assert_eq!(left_id, right_id, "record is content-addressed");

            let restored = load_live_view_record(&mut left, left_id)
                .await
                .expect("load local record")
                .expect("record is present");
            assert_eq!(restored, original);
            assert_eq!(
                restored.source.cursor,
                LiveViewCursor::Historical {
                    cursor: "17".to_string()
                }
            );
            assert_eq!(restored.view.hidden_relations.len(), 1);
            assert_eq!(restored.view.folds.len(), 1);
        });
    }

    #[test]
    fn source_refusals_are_explicit_and_never_open_an_empty_snapshot() {
        pollster::block_on(async {
            let mut store = InMemoryStore::default();
            let id = save_live_view_record(&mut store, &record(), CREATED_AT)
                .await
                .expect("save");

            for refusal in [
                LiveViewSourceError::MissingSource,
                LiveViewSourceError::StaleCursor,
                LiveViewSourceError::AccessDenied,
                LiveViewSourceError::UnsupportedArrangement,
            ] {
                let mut resolver = FixtureResolver {
                    result: Err(refusal.clone()),
                };
                let error = open_live_view_record(&mut store, id, &mut resolver)
                    .await
                    .expect_err("a source refusal must not become an empty view");
                assert!(
                    error.to_string().contains(&refusal.to_string()),
                    "the caller receives the source owner's exact refusal"
                );
            }
        });
    }

    #[test]
    fn malformed_source_and_stale_fold_are_refused_before_storage() {
        let mut malformed = record();
        malformed.source.cursor = LiveViewCursor::Historical {
            cursor: " ".to_string(),
        };
        assert_eq!(
            malformed.validate(),
            Err(LiveViewRecordError::EmptyHistoricalCursor)
        );

        let mut stale_fold = record();
        stale_fold.view.folds[0].source_scope = "session:other/graph:root".to_string();
        assert!(matches!(
            stale_fold.validate(),
            Err(LiveViewRecordError::FoldScopeMismatch { .. })
        ));
    }
}
