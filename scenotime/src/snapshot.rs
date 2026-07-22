use sceno::{
    InstanceId, ProjectedItem, Rect, Region, RoutedRelation, Scene, SourceRef, Space, SpaceId,
};
use serde::{Deserialize, Serialize};

use crate::{RegionId, RelationId, Revision, SceneEpoch};

/// Stable scene tables. `None` is a tombstone, never an invitation to reuse the
/// index during this epoch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneTables {
    pub sources: Vec<Option<SourceRef>>,
    pub spaces: Vec<Option<Space>>,
    pub items: Vec<Option<ProjectedItem>>,
    /// Explicit visual order, independent of the stable item slot.
    pub item_order: Vec<Option<i32>>,
    pub relations: Vec<Option<RoutedRelation>>,
    pub regions: Vec<Option<Region>>,
    pub bounds: Rect,
    pub generation: u64,
}

/// A complete resynchronization snapshot. Tombstones remain serialized so a
/// reconnect within the same epoch cannot reinterpret an old index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneSnapshot {
    pub epoch: SceneEpoch,
    pub revision: Revision,
    pub tables: SceneTables,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotError {
    Invalid(String),
}

impl SceneSnapshot {
    /// Begin an epoch from a dense one-shot scene.
    pub fn from_dense(
        epoch: SceneEpoch,
        revision: Revision,
        scene: Scene,
    ) -> Result<Self, SnapshotError> {
        let item_count = scene.items.len();
        let snapshot = Self {
            epoch,
            revision,
            tables: SceneTables {
                sources: scene.sources.into_iter().map(Some).collect(),
                spaces: scene.spaces.into_iter().map(Some).collect(),
                items: scene.items.into_iter().map(Some).collect(),
                item_order: (0..item_count).map(|index| Some(index as i32)).collect(),
                relations: scene.relations.into_iter().map(Some).collect(),
                regions: scene.regions.into_iter().map(Some).collect(),
                bounds: scene.bounds,
                generation: scene.generation,
            },
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub fn active_item(&self, id: InstanceId) -> Option<&ProjectedItem> {
        self.tables.items.get(id.0 as usize)?.as_ref()
    }

    pub fn active_relation(&self, id: RelationId) -> Option<&RoutedRelation> {
        self.tables.relations.get(id.0 as usize)?.as_ref()
    }

    pub fn active_region(&self, id: RegionId) -> Option<&Region> {
        self.tables.regions.get(id.0 as usize)?.as_ref()
    }

    pub fn active_item_count(&self) -> usize {
        self.tables.items.iter().flatten().count()
    }

    /// Active items sorted by explicit order, then stable slot.
    pub fn active_items_in_order(&self) -> Vec<(InstanceId, &ProjectedItem)> {
        let mut items = self
            .tables
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                let item = item.as_ref()?;
                let order = self.tables.item_order[index]?;
                Some((order, index as u32, item))
            })
            .collect::<Vec<_>>();
        items.sort_by_key(|(order, index, _)| (*order, *index));
        items
            .into_iter()
            .map(|(_, index, item)| (InstanceId(index), item))
            .collect()
    }

    /// Validate table shape, world-space invariants, live references, and
    /// space-chain acyclicity after deserialization or before disclosure.
    pub fn validate(&self) -> Result<(), SnapshotError> {
        let tables = &self.tables;
        if tables.spaces.first().and_then(Option::as_ref).is_none() {
            return invalid("world space slot 0 must remain active");
        }
        if tables.spaces[0].as_ref().unwrap().parent.is_some() {
            return invalid("world space cannot have a parent");
        }
        if tables.items.len() != tables.item_order.len() {
            return invalid("item and order tables must have equal length");
        }
        for (index, item) in tables.items.iter().enumerate() {
            match (item, tables.item_order[index]) {
                (Some(_), None) => return invalid(format!("active item {index} lacks order")),
                (None, Some(_)) => {
                    return invalid(format!("tombstoned item {index} retains order"));
                }
                _ => {}
            }
        }

        for (index, space) in tables.spaces.iter().enumerate() {
            let Some(space) = space else { continue };
            if let Some(parent) = space.parent {
                require_active(&tables.spaces, parent.0, "space parent")?;
            } else if index != 0 {
                return invalid(format!("non-world space {index} lacks a parent"));
            }
            validate_space_chain(&tables.spaces, SpaceId(index as u32))?;
        }

        for (index, item) in tables.items.iter().enumerate() {
            let Some(item) = item else { continue };
            require_active(&tables.sources, item.source.0, "item source")?;
            require_active(&tables.spaces, item.space.0, "item space")?;
            if item.source.0 as usize >= tables.sources.len() {
                return invalid(format!("item {index} has dangling source"));
            }
        }
        for relation in tables.relations.iter().flatten() {
            require_active(&tables.items, relation.from.0, "relation start")?;
            require_active(&tables.items, relation.to.0, "relation end")?;
            require_active(&tables.spaces, relation.space.0, "relation space")?;
        }
        for region in tables.regions.iter().flatten() {
            require_active(&tables.spaces, region.space.0, "region space")?;
            for member in &region.members {
                require_active(&tables.items, member.0, "region member")?;
            }
        }
        Ok(())
    }
}

fn validate_space_chain(spaces: &[Option<Space>], start: SpaceId) -> Result<(), SnapshotError> {
    let mut current = start;
    for _ in 0..=spaces.len() {
        let Some(space) = spaces.get(current.0 as usize).and_then(Option::as_ref) else {
            return invalid(format!("space {} has a dangling chain", start.0));
        };
        match space.parent {
            Some(parent) => current = parent,
            None => return Ok(()),
        }
    }
    invalid(format!("space {} participates in a cycle", start.0))
}

fn require_active<T>(slots: &[Option<T>], index: u32, label: &str) -> Result<(), SnapshotError> {
    if slots.get(index as usize).and_then(Option::as_ref).is_none() {
        return invalid(format!("{label} {index} is absent or tombstoned"));
    }
    Ok(())
}

fn invalid<T>(message: impl Into<String>) -> Result<T, SnapshotError> {
    Err(SnapshotError::Invalid(message.into()))
}
