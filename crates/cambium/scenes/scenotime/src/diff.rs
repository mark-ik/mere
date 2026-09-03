// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use sceno::{
    Backdrop, InstanceId, ProjectedItem, Rect, Region, RoutedRelation, SourceIx, SourceRef, Space,
    SpaceId,
};
use serde::{Deserialize, Serialize};

use crate::{BackdropId, RegionId, RelationId, Revision, SceneEpoch, SceneSnapshot, SnapshotError};

/// One idempotent transition within a scene epoch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneDiff {
    pub epoch: SceneEpoch,
    pub base: Revision,
    pub revision: Revision,
    pub operations: Vec<SceneOp>,
}

/// Stable-slot scene changes. Additions append at the named index; tombstones
/// remain allocated until a new epoch begins.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SceneOp {
    AddSource {
        index: SourceIx,
        value: SourceRef,
    },
    UpdateSource {
        index: SourceIx,
        value: SourceRef,
    },
    TombstoneSource {
        index: SourceIx,
    },
    AddSpace {
        index: SpaceId,
        value: Space,
    },
    UpdateSpace {
        index: SpaceId,
        value: Space,
    },
    TombstoneSpace {
        index: SpaceId,
    },
    AddBackdrop {
        index: BackdropId,
        value: Backdrop,
    },
    UpdateBackdrop {
        index: BackdropId,
        value: Backdrop,
    },
    TombstoneBackdrop {
        index: BackdropId,
    },
    AddItem {
        index: InstanceId,
        value: ProjectedItem,
        order: i32,
    },
    UpdateItem {
        index: InstanceId,
        value: ProjectedItem,
    },
    TombstoneItem {
        index: InstanceId,
    },
    SetItemLayer {
        index: InstanceId,
        layer: i16,
    },
    SetItemOrder {
        index: InstanceId,
        order: i32,
    },
    AddRelation {
        index: RelationId,
        value: RoutedRelation,
    },
    UpdateRelation {
        index: RelationId,
        value: RoutedRelation,
    },
    TombstoneRelation {
        index: RelationId,
    },
    AddRegion {
        index: RegionId,
        value: Region,
    },
    UpdateRegion {
        index: RegionId,
        value: Region,
    },
    TombstoneRegion {
        index: RegionId,
    },
    SetBounds {
        bounds: Rect,
    },
    SetGeneration {
        generation: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyOutcome {
    Applied,
    AlreadyApplied,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffError {
    WrongEpoch {
        current: SceneEpoch,
        received: SceneEpoch,
    },
    MissingBase {
        current: Revision,
        required: Revision,
    },
    InvalidRevision {
        base: Revision,
        revision: Revision,
    },
    InvalidOperation(String),
    InvalidSnapshot(SnapshotError),
}

impl SceneSnapshot {
    /// Apply a diff transactionally. A repeated or older target revision is a
    /// successful no-op; a future diff with a missing base requests resync.
    pub fn apply_diff(&mut self, diff: &SceneDiff) -> Result<ApplyOutcome, DiffError> {
        if self.epoch != diff.epoch {
            return Err(DiffError::WrongEpoch {
                current: self.epoch,
                received: diff.epoch,
            });
        }
        if diff.revision <= self.revision {
            return Ok(ApplyOutcome::AlreadyApplied);
        }
        if diff.base != self.revision {
            return Err(DiffError::MissingBase {
                current: self.revision,
                required: diff.base,
            });
        }
        if diff.revision <= diff.base {
            return Err(DiffError::InvalidRevision {
                base: diff.base,
                revision: diff.revision,
            });
        }

        let mut next = self.clone();
        for operation in &diff.operations {
            next.apply_operation(operation)?;
        }
        next.validate().map_err(DiffError::InvalidSnapshot)?;
        next.revision = diff.revision;
        *self = next;
        Ok(ApplyOutcome::Applied)
    }

    fn apply_operation(&mut self, operation: &SceneOp) -> Result<(), DiffError> {
        let tables = &mut self.tables;
        match operation {
            SceneOp::AddSource { index, value } => {
                append(&mut tables.sources, index.0, value.clone(), "source")
            }
            SceneOp::UpdateSource { index, value } => {
                update(&mut tables.sources, index.0, value.clone(), "source")
            }
            SceneOp::TombstoneSource { index } => tombstone(&mut tables.sources, index.0, "source"),
            SceneOp::AddSpace { index, value } => {
                append(&mut tables.spaces, index.0, value.clone(), "space")
            }
            SceneOp::UpdateSpace { index, value } => {
                update(&mut tables.spaces, index.0, value.clone(), "space")
            }
            SceneOp::TombstoneSpace { index } => tombstone(&mut tables.spaces, index.0, "space"),
            SceneOp::AddBackdrop { index, value } => {
                append(&mut tables.backdrops, index.0, value.clone(), "backdrop")
            }
            SceneOp::UpdateBackdrop { index, value } => {
                update(&mut tables.backdrops, index.0, value.clone(), "backdrop")
            }
            SceneOp::TombstoneBackdrop { index } => {
                tombstone(&mut tables.backdrops, index.0, "backdrop")
            }
            SceneOp::AddItem {
                index,
                value,
                order,
            } => {
                append(&mut tables.items, index.0, value.clone(), "item")?;
                append(&mut tables.item_order, index.0, *order, "item order")
            }
            SceneOp::UpdateItem { index, value } => {
                update(&mut tables.items, index.0, value.clone(), "item")
            }
            SceneOp::TombstoneItem { index } => {
                tombstone(&mut tables.items, index.0, "item")?;
                tombstone(&mut tables.item_order, index.0, "item order")
            }
            SceneOp::SetItemLayer { index, layer } => {
                active_mut(&mut tables.items, index.0, "item")?.layer = *layer;
                Ok(())
            }
            SceneOp::SetItemOrder { index, order } => {
                *active_mut(&mut tables.item_order, index.0, "item order")? = *order;
                Ok(())
            }
            SceneOp::AddRelation { index, value } => {
                append(&mut tables.relations, index.0, value.clone(), "relation")
            }
            SceneOp::UpdateRelation { index, value } => {
                update(&mut tables.relations, index.0, value.clone(), "relation")
            }
            SceneOp::TombstoneRelation { index } => {
                tombstone(&mut tables.relations, index.0, "relation")
            }
            SceneOp::AddRegion { index, value } => {
                append(&mut tables.regions, index.0, value.clone(), "region")
            }
            SceneOp::UpdateRegion { index, value } => {
                update(&mut tables.regions, index.0, value.clone(), "region")
            }
            SceneOp::TombstoneRegion { index } => tombstone(&mut tables.regions, index.0, "region"),
            SceneOp::SetBounds { bounds } => {
                tables.bounds = *bounds;
                Ok(())
            }
            SceneOp::SetGeneration { generation } => {
                tables.generation = *generation;
                Ok(())
            }
        }
    }
}

fn append<T>(
    slots: &mut Vec<Option<T>>,
    index: u32,
    value: T,
    table: &str,
) -> Result<(), DiffError> {
    if index as usize != slots.len() {
        return operation_error(format!(
            "{table} add index {index} must append at {}; tombstones are not reusable",
            slots.len()
        ));
    }
    slots.push(Some(value));
    Ok(())
}

fn update<T>(slots: &mut [Option<T>], index: u32, value: T, table: &str) -> Result<(), DiffError> {
    *active_mut(slots, index, table)? = value;
    Ok(())
}

fn tombstone<T>(slots: &mut [Option<T>], index: u32, table: &str) -> Result<(), DiffError> {
    let Some(slot) = slots.get_mut(index as usize) else {
        return operation_error(format!("{table} tombstone index {index} is out of range"));
    };
    *slot = None;
    Ok(())
}

fn active_mut<'a, T>(
    slots: &'a mut [Option<T>],
    index: u32,
    table: &str,
) -> Result<&'a mut T, DiffError> {
    slots
        .get_mut(index as usize)
        .and_then(Option::as_mut)
        .ok_or_else(|| {
            DiffError::InvalidOperation(format!("{table} index {index} is absent or tombstoned"))
        })
}

fn operation_error<T>(message: impl Into<String>) -> Result<T, DiffError> {
    Err(DiffError::InvalidOperation(message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sceno::{Backdrop, Footprint, Representation, Scene, Size2, Transform2};

    #[test]
    fn a_violation_survives_the_wire_a_remote_viewer_reads() {
        let mut scene = Scene::new();
        scene.unmet_holds.push(sceno::HeldPlacement::pinned(
            SourceRef::new("fixture", "ghost"),
            sceno::Vec2::new(7.0, 7.0),
        ));
        let snapshot =
            SceneSnapshot::from_dense(SceneEpoch(1), Revision(1), scene).expect("snapshot");
        assert_eq!(snapshot.tables.unmet_holds.len(), 1);

        // The done-condition names a remote viewer, so serialize it: the
        // distinction has to survive the hop, not merely exist in memory.
        let wire = serde_json::to_string(&snapshot).expect("serialize");
        let far_side: SceneSnapshot = serde_json::from_str(&wire).expect("deserialize");
        assert_eq!(far_side.tables.unmet_holds[0].source.id, "ghost");
        assert_eq!(
            far_side.tables.unmet_holds[0].at,
            sceno::Vec2::new(7.0, 7.0)
        );
    }

    #[test]
    fn a_remote_viewer_can_tell_placed_as_pinned_from_pin_unmet() {
        // A1's done-condition, stated whole. Carrying only the failures would
        // leave every unremarked item ambiguous: unpinned, or pinned and
        // honored, with nothing on the wire to separate them.
        let mut scene = Scene::new();
        let source = scene.intern_source(SourceRef::new("fixture", "kept"));
        scene.items.push(ProjectedItem {
            source,
            space: Scene::WORLD,
            transform: Transform2::translation(4.0, 4.0),
            footprint: Footprint::Point,
            representation: Representation::Glyph,
            layer: 0,
            visible: true,
            hit: None,
            channels: Vec::new(),
        });
        scene.honored_holds.push(sceno::HonoredHold {
            instance: InstanceId(0),
            placement: sceno::HeldPlacement::pinned(
                SourceRef::new("fixture", "kept"),
                sceno::Vec2::new(4.0, 4.0),
            ),
        });
        scene.unmet_holds.push(sceno::HeldPlacement::pinned(
            SourceRef::new("fixture", "lost"),
            sceno::Vec2::new(9.0, 9.0),
        ));

        let snapshot =
            SceneSnapshot::from_dense(SceneEpoch(1), Revision(1), scene).expect("snapshot");
        let wire = serde_json::to_string(&snapshot).expect("serialize");
        let far_side: SceneSnapshot = serde_json::from_str(&wire).expect("deserialize");

        assert_eq!(far_side.tables.honored_holds.len(), 1);
        assert_eq!(far_side.tables.honored_holds[0].placement.source.id, "kept");
        assert_eq!(far_side.tables.honored_holds[0].instance, InstanceId(0));
        assert_eq!(far_side.tables.unmet_holds.len(), 1);
        assert_eq!(far_side.tables.unmet_holds[0].source.id, "lost");
    }

    #[test]
    fn an_honored_pin_claim_must_match_the_live_instance() {
        let mut scene = Scene::new();
        let source = scene.intern_source(SourceRef::new("fixture", "kept"));
        scene.items.push(ProjectedItem {
            source,
            space: Scene::WORLD,
            transform: Transform2::translation(4.0, 4.0),
            footprint: Footprint::Point,
            representation: Representation::Glyph,
            layer: 0,
            visible: true,
            hit: None,
            channels: Vec::new(),
        });
        scene.honored_holds.push(sceno::HonoredHold {
            instance: InstanceId(0),
            placement: sceno::HeldPlacement::pinned(
                SourceRef::new("fixture", "kept"),
                sceno::Vec2::new(4.0, 4.0),
            ),
        });
        let snapshot =
            SceneSnapshot::from_dense(SceneEpoch(1), Revision(1), scene).expect("valid claim");

        let mut moved = snapshot.clone();
        moved.tables.items[0].as_mut().unwrap().transform.translate = sceno::Vec2::ZERO;
        assert!(matches!(moved.validate(), Err(SnapshotError::Invalid(_))));

        let mut wrong_source = snapshot.clone();
        wrong_source.tables.honored_holds[0].placement.source = SourceRef::new("fixture", "other");
        assert!(matches!(
            wrong_source.validate(),
            Err(SnapshotError::Invalid(_))
        ));

        let mut missing = snapshot;
        missing.tables.honored_holds[0].instance = InstanceId(9);
        assert!(matches!(missing.validate(), Err(SnapshotError::Invalid(_))));
    }

    #[test]
    fn a_snapshot_written_before_violations_existed_still_reads() {
        let snapshot =
            SceneSnapshot::from_dense(SceneEpoch(1), Revision(1), Scene::new()).expect("snapshot");
        let wire = serde_json::to_string(&snapshot).expect("serialize");
        let older = wire.replace(",\"unmet_holds\":[]", "");
        assert!(!older.contains("unmet_holds"), "older wire shape");
        let far_side: SceneSnapshot = serde_json::from_str(&older).expect("older wire loads");
        assert!(far_side.tables.unmet_holds.is_empty());
    }

    #[test]
    fn a_snapshot_written_before_backdrops_existed_still_reads() {
        let snapshot =
            SceneSnapshot::from_dense(SceneEpoch(1), Revision(1), Scene::new()).expect("snapshot");
        let wire = serde_json::to_string(&snapshot).expect("serialize");
        let older = wire.replace("\"backdrops\":[],", "");
        assert!(!older.contains("backdrops"), "older wire shape");
        let far_side: SceneSnapshot = serde_json::from_str(&older).expect("older wire loads");
        assert!(far_side.tables.backdrops.is_empty());
    }

    fn item(source: SourceIx, x: f32) -> ProjectedItem {
        ProjectedItem {
            source,
            space: Scene::WORLD,
            transform: Transform2::translation(x, 0.0),
            footprint: Footprint::Rect {
                size: Size2::new(40.0, 24.0),
            },
            representation: Representation::Card,
            layer: 0,
            visible: true,
            hit: None,
            channels: Vec::new(),
        }
    }

    fn snapshot() -> SceneSnapshot {
        let mut scene = Scene::new();
        let source = scene.intern_source(SourceRef::new("fixture", "one"));
        scene.items.push(item(source, 0.0));
        SceneSnapshot::from_dense(SceneEpoch(7), Revision(1), scene).unwrap()
    }

    fn backdrop(source: SourceIx, kind: &str) -> Backdrop {
        Backdrop {
            source,
            space: Scene::WORLD,
            transform: Transform2::IDENTITY,
            footprint: Footprint::Rect {
                size: Size2::new(80.0, 60.0),
            },
            kind: kind.into(),
            visible: true,
            collidable: false,
        }
    }

    #[test]
    fn backdrop_changes_cross_the_wire_in_stable_slots() {
        let mut state = snapshot();
        let added = backdrop(SourceIx(0), "fixture:floor");
        let diff = SceneDiff {
            epoch: SceneEpoch(7),
            base: Revision(1),
            revision: Revision(2),
            operations: vec![SceneOp::AddBackdrop {
                index: BackdropId(0),
                value: added.clone(),
            }],
        };
        let wire = serde_json::to_string(&diff).unwrap();
        let far_side: SceneDiff = serde_json::from_str(&wire).unwrap();
        state.apply_diff(&far_side).unwrap();
        assert_eq!(state.active_backdrop(BackdropId(0)), Some(&added));

        state
            .apply_diff(&SceneDiff {
                epoch: SceneEpoch(7),
                base: Revision(2),
                revision: Revision(3),
                operations: vec![SceneOp::TombstoneBackdrop {
                    index: BackdropId(0),
                }],
            })
            .unwrap();
        assert_eq!(state.active_backdrop(BackdropId(0)), None);
        assert_eq!(state.tables.backdrops.len(), 1, "slot remains allocated");
    }

    #[test]
    fn tombstoned_slots_are_not_reused() {
        let mut state = snapshot();
        state
            .apply_diff(&SceneDiff {
                epoch: SceneEpoch(7),
                base: Revision(1),
                revision: Revision(2),
                operations: vec![SceneOp::TombstoneItem {
                    index: InstanceId(0),
                }],
            })
            .unwrap();
        let before = state.clone();
        let error = state
            .apply_diff(&SceneDiff {
                epoch: SceneEpoch(7),
                base: Revision(2),
                revision: Revision(3),
                operations: vec![SceneOp::AddItem {
                    index: InstanceId(0),
                    value: item(SourceIx(0), 10.0),
                    order: 0,
                }],
            })
            .unwrap_err();
        assert!(matches!(error, DiffError::InvalidOperation(_)));
        assert_eq!(state, before, "a rejected diff is transactional");
    }

    #[test]
    fn duplicate_diff_is_an_idempotent_noop() {
        let mut state = snapshot();
        let diff = SceneDiff {
            epoch: SceneEpoch(7),
            base: Revision(1),
            revision: Revision(2),
            operations: vec![SceneOp::SetItemLayer {
                index: InstanceId(0),
                layer: 4,
            }],
        };
        assert_eq!(state.apply_diff(&diff), Ok(ApplyOutcome::Applied));
        let once = state.clone();
        assert_eq!(state.apply_diff(&diff), Ok(ApplyOutcome::AlreadyApplied));
        assert_eq!(state, once);
    }

    #[test]
    fn missing_base_and_wrong_epoch_request_resynchronization() {
        let mut state = snapshot();
        let missing = SceneDiff {
            epoch: SceneEpoch(7),
            base: Revision(9),
            revision: Revision(10),
            operations: Vec::new(),
        };
        assert_eq!(
            state.apply_diff(&missing),
            Err(DiffError::MissingBase {
                current: Revision(1),
                required: Revision(9)
            })
        );
        let other_epoch = SceneDiff {
            epoch: SceneEpoch(8),
            base: Revision(1),
            revision: Revision(2),
            operations: Vec::new(),
        };
        assert_eq!(
            state.apply_diff(&other_epoch),
            Err(DiffError::WrongEpoch {
                current: SceneEpoch(7),
                received: SceneEpoch(8)
            })
        );
    }

    #[test]
    fn dangling_relations_reject_the_whole_diff() {
        let mut state = snapshot();
        state.tables.relations.push(Some(RoutedRelation {
            from: InstanceId(0),
            to: InstanceId(0),
            space: Scene::WORLD,
            points: Vec::new(),
            kind: None,
            weight: None,
        }));
        let before = state.clone();
        assert!(matches!(
            state.apply_diff(&SceneDiff {
                epoch: SceneEpoch(7),
                base: Revision(1),
                revision: Revision(2),
                operations: vec![SceneOp::TombstoneItem {
                    index: InstanceId(0)
                }],
            }),
            Err(DiffError::InvalidSnapshot(_))
        ));
        assert_eq!(state, before);
    }

    #[test]
    fn explicit_order_is_independent_of_stable_slot() {
        let mut state = snapshot();
        state
            .apply_diff(&SceneDiff {
                epoch: SceneEpoch(7),
                base: Revision(1),
                revision: Revision(2),
                operations: vec![
                    SceneOp::AddItem {
                        index: InstanceId(1),
                        value: item(SourceIx(0), 20.0),
                        order: -1,
                    },
                    SceneOp::SetItemLayer {
                        index: InstanceId(1),
                        layer: 6,
                    },
                ],
            })
            .unwrap();
        let active = state.active_items_in_order();
        assert_eq!(active[0].0, InstanceId(1));
        assert_eq!(active[0].1.layer, 6);
        assert_eq!(active[1].0, InstanceId(0));
    }

    #[test]
    fn snapshot_and_diff_round_trip_with_tombstones() {
        let mut state = snapshot();
        let diff = SceneDiff {
            epoch: SceneEpoch(7),
            base: Revision(1),
            revision: Revision(2),
            operations: vec![SceneOp::TombstoneItem {
                index: InstanceId(0),
            }],
        };
        state.apply_diff(&diff).unwrap();
        let state_json = serde_json::to_string(&state).unwrap();
        let diff_json = serde_json::to_string(&diff).unwrap();
        assert_eq!(
            serde_json::from_str::<SceneSnapshot>(&state_json).unwrap(),
            state
        );
        assert_eq!(serde_json::from_str::<SceneDiff>(&diff_json).unwrap(), diff);
    }

    #[test]
    fn deterministic_random_diff_run_matches_full_rebuild_oracle() {
        let mut state = snapshot();
        let mut oracle_items = state.tables.items.clone();
        let mut oracle_order = state.tables.item_order.clone();
        let mut seed = 0x5eed_u64;
        for step in 0..96_u64 {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let active = oracle_items
                .iter()
                .enumerate()
                .filter_map(|(index, item)| item.as_ref().map(|_| index))
                .collect::<Vec<_>>();
            let mut operations = Vec::new();
            match seed % 4 {
                0 | 1 if !active.is_empty() => {
                    let index = active[(seed as usize >> 8) % active.len()];
                    let x = ((seed >> 16) % 500) as f32;
                    let updated = item(SourceIx(0), x);
                    operations.push(SceneOp::UpdateItem {
                        index: InstanceId(index as u32),
                        value: updated.clone(),
                    });
                    oracle_items[index] = Some(updated);
                }
                2 if active.len() > 1 => {
                    let index = active[(seed as usize >> 8) % active.len()];
                    operations.push(SceneOp::TombstoneItem {
                        index: InstanceId(index as u32),
                    });
                    oracle_items[index] = None;
                    oracle_order[index] = None;
                }
                _ => {
                    let index = oracle_items.len();
                    let value = item(SourceIx(0), (seed % 700) as f32);
                    let order = (seed >> 24) as i32;
                    operations.push(SceneOp::AddItem {
                        index: InstanceId(index as u32),
                        value: value.clone(),
                        order,
                    });
                    oracle_items.push(Some(value));
                    oracle_order.push(Some(order));
                }
            }
            let base = Revision(step + 1);
            state
                .apply_diff(&SceneDiff {
                    epoch: SceneEpoch(7),
                    base,
                    revision: Revision(step + 2),
                    operations,
                })
                .unwrap();
        }
        let mut rebuilt = snapshot();
        rebuilt.tables.items = oracle_items;
        rebuilt.tables.item_order = oracle_order;
        rebuilt.revision = Revision(97);
        assert_eq!(state, rebuilt);
    }
}
