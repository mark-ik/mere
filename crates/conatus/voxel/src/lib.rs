//! Generic revisioned voxel chunks and occupancy edits.
//!
//! This package owns value mechanics only: chunk coordinates, revision-gated
//! edits, dirty regions, and lowering material changes into occupancy changes.
//! Products retain durable voxel identity and admission policy; spatial
//! runtimes decide how accepted occupancy edits affect colliders.

use std::{collections::BTreeMap, error::Error, fmt};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

/// One backend-neutral occupancy edit in collider-local cell coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoxelEdit {
    pub cell: [i32; 3],
    pub filled: bool,
}

/// A world cell split into a product-owned chunk coordinate and chunk-local cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoxelAddress {
    pub chunk: [i64; 3],
    pub local: [u32; 3],
}

/// Split a world-space cell with Euclidean division, including negative cells.
pub fn split_voxel_address(
    cell: [i64; 3],
    chunk_extent: [u32; 3],
) -> Result<VoxelAddress, VoxelChunkError> {
    validate_extent(chunk_extent)?;
    let mut chunk = [0; 3];
    let mut local = [0; 3];
    for axis in 0..3 {
        let extent = i64::from(chunk_extent[axis]);
        chunk[axis] = cell[axis].div_euclid(extent);
        local[axis] = cell[axis].rem_euclid(extent) as u32;
    }
    Ok(VoxelAddress { chunk, local })
}

/// A chunk-local dirty box. `extent` is exclusive from `origin`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoxelRegion {
    pub origin: [u32; 3],
    pub extent: [u32; 3],
}

impl VoxelRegion {
    pub fn contains(self, cell: [u32; 3]) -> bool {
        (0..3).all(|axis| {
            cell[axis] >= self.origin[axis]
                && cell[axis] < self.origin[axis].saturating_add(self.extent[axis])
        })
    }

    fn covering<T>(changes: &[VoxelCellChange<T>]) -> Option<Self> {
        let first = changes.first()?.cell;
        let mut min = first;
        let mut max = first;
        for change in &changes[1..] {
            for axis in 0..3 {
                min[axis] = min[axis].min(change.cell[axis]);
                max[axis] = max[axis].max(change.cell[axis]);
            }
        }
        Some(Self {
            origin: min,
            extent: std::array::from_fn(|axis| max[axis] - min[axis] + 1),
        })
    }
}

/// One requested replacement in a chunk-local cell.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoxelCellEdit<T> {
    pub cell: [u32; 3],
    pub value: T,
}

/// One effective replacement, including the prior value for derived consumers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoxelCellChange<T> {
    pub cell: [u32; 3],
    pub previous: T,
    pub value: T,
}

/// The effective result of one accepted, revision-gated chunk patch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoxelPatch<T> {
    pub previous_revision: u64,
    pub revision: u64,
    pub requested: usize,
    pub changes: Vec<VoxelCellChange<T>>,
    pub dirty_regions: Vec<VoxelRegion>,
}

impl<T> VoxelPatch<T> {
    /// Lower material changes into collision edits, omitting material-only changes.
    pub fn occupancy_edits(
        &self,
        occupied: impl Fn(&T) -> bool,
    ) -> Result<Vec<VoxelEdit>, VoxelChunkError> {
        self.changes
            .iter()
            .map(|change| {
                let previous = occupied(&change.previous);
                let current = occupied(&change.value);
                if previous == current {
                    return Ok(None);
                }
                let mut cell = [0; 3];
                for (target, coordinate) in cell.iter_mut().zip(change.cell) {
                    *target = i32::try_from(coordinate).map_err(|_| {
                        VoxelChunkError::CellCoordinateTooLarge { cell: change.cell }
                    })?;
                }
                Ok(Some(VoxelEdit {
                    cell,
                    filled: current,
                }))
            })
            .filter_map(Result::transpose)
            .collect()
    }
}

/// Dense, opaque chunk storage with record-friendly revisions and disposable dirt.
///
/// A product owns the chunk's durable identity and decides which values are occupied.
/// Conatus supplies storage and patch mechanics only. Dirty regions are projection work
/// and are deliberately omitted from serialization and equality.
/// Serialized cells are ordered by Y, then Z, then X, with X contiguous, matching the
/// incumbent Mesocosm brick order. This CPU record layout is not a resident GPU ABI.
#[derive(Clone, Debug)]
pub struct VoxelChunk<T> {
    extent: [u32; 3],
    cells: Vec<T>,
    revision: u64,
    dirty_regions: Vec<VoxelRegion>,
}

impl<T: PartialEq> PartialEq for VoxelChunk<T> {
    fn eq(&self, other: &Self) -> bool {
        self.extent == other.extent && self.cells == other.cells && self.revision == other.revision
    }
}

impl<T: Eq> Eq for VoxelChunk<T> {}

impl<T: Clone> VoxelChunk<T> {
    pub fn new(extent: [u32; 3], empty: T) -> Result<Self, VoxelChunkError> {
        let count = cell_count(extent)?;
        let mut cells = Vec::new();
        cells
            .try_reserve_exact(count)
            .map_err(|_| VoxelChunkError::AllocationFailed { cells: count })?;
        cells.resize(count, empty);
        Ok(Self {
            extent,
            cells,
            revision: 0,
            dirty_regions: Vec::new(),
        })
    }
}

impl<T> VoxelChunk<T> {
    pub fn from_cells(
        extent: [u32; 3],
        cells: Vec<T>,
        revision: u64,
    ) -> Result<Self, VoxelChunkError> {
        let expected = cell_count(extent)?;
        if cells.len() != expected {
            return Err(VoxelChunkError::CellCountMismatch {
                expected,
                actual: cells.len(),
            });
        }
        Ok(Self {
            extent,
            cells,
            revision,
            dirty_regions: Vec::new(),
        })
    }

    pub const fn extent(&self) -> [u32; 3] {
        self.extent
    }

    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub fn get(&self, cell: [u32; 3]) -> Option<&T> {
        self.index(cell).map(|index| &self.cells[index])
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = ([u32; 3], &T)> {
        let extent = self.extent;
        self.cells.iter().enumerate().map(move |(index, value)| {
            let x = index % extent[0] as usize;
            let yz = index / extent[0] as usize;
            let z = yz % extent[2] as usize;
            let y = yz / extent[2] as usize;
            ([x as u32, y as u32, z as u32], value)
        })
    }

    pub fn occupied_cells(&self, occupied: impl Fn(&T) -> bool) -> Vec<[i32; 3]> {
        self.iter()
            .filter(|&(_, value)| occupied(value))
            .map(|(cell, _)| cell.map(|value| value as i32))
            .collect()
    }

    pub fn dirty_regions(&self) -> &[VoxelRegion] {
        &self.dirty_regions
    }

    pub fn drain_dirty_regions(&mut self) -> Vec<VoxelRegion> {
        std::mem::take(&mut self.dirty_regions)
    }

    pub fn apply_edits(
        &mut self,
        expected_revision: u64,
        max_edits: usize,
        edits: impl IntoIterator<Item = VoxelCellEdit<T>>,
    ) -> Result<VoxelPatch<T>, VoxelChunkError>
    where
        T: Clone + PartialEq,
    {
        if expected_revision != self.revision {
            return Err(VoxelChunkError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }

        let edits: Vec<_> = edits.into_iter().collect();
        let requested = edits.len();
        if requested > max_edits {
            return Err(VoxelChunkError::BatchTooLarge {
                requested,
                limit: max_edits,
            });
        }

        let mut staged = BTreeMap::new();
        for edit in edits {
            let index = self
                .index(edit.cell)
                .ok_or(VoxelChunkError::CellOutOfBounds {
                    cell: edit.cell,
                    extent: self.extent,
                })?;
            staged.insert(index, edit);
        }

        let mut changes = Vec::new();
        for (index, edit) in staged {
            if self.cells[index] != edit.value {
                changes.push(VoxelCellChange {
                    cell: edit.cell,
                    previous: self.cells[index].clone(),
                    value: edit.value,
                });
            }
        }

        let previous_revision = self.revision;
        if changes.is_empty() {
            return Ok(VoxelPatch {
                previous_revision,
                revision: previous_revision,
                requested,
                changes,
                dirty_regions: Vec::new(),
            });
        }
        let revision = previous_revision
            .checked_add(1)
            .ok_or(VoxelChunkError::RevisionOverflow)?;
        let dirty = VoxelRegion::covering(&changes).expect("effective changes are non-empty");
        for change in &changes {
            let index = self.index(change.cell).expect("validated cell");
            self.cells[index] = change.value.clone();
        }
        self.revision = revision;
        self.dirty_regions.push(dirty);

        Ok(VoxelPatch {
            previous_revision,
            revision,
            requested,
            changes,
            dirty_regions: vec![dirty],
        })
    }

    fn index(&self, cell: [u32; 3]) -> Option<usize> {
        if (0..3).any(|axis| cell[axis] >= self.extent[axis]) {
            return None;
        }
        Some(
            ((cell[1] as usize * self.extent[2] as usize + cell[2] as usize)
                * self.extent[0] as usize)
                + cell[0] as usize,
        )
    }
}

#[derive(Serialize, Deserialize)]
struct VoxelChunkWire<T> {
    extent: [u32; 3],
    cells: Vec<T>,
    revision: u64,
}

#[derive(Serialize)]
struct VoxelChunkWireRef<'a, T> {
    extent: [u32; 3],
    cells: &'a [T],
    revision: u64,
}

impl<T: Serialize> Serialize for VoxelChunk<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        VoxelChunkWireRef {
            extent: self.extent,
            cells: &self.cells,
            revision: self.revision,
        }
        .serialize(serializer)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for VoxelChunk<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = VoxelChunkWire::<T>::deserialize(deserializer)?;
        Self::from_cells(wire.extent, wire.cells, wire.revision).map_err(D::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VoxelChunkError {
    EmptyExtent,
    ExtentTooLarge { extent: [u32; 3] },
    CellCountOverflow { extent: [u32; 3] },
    CellCountMismatch { expected: usize, actual: usize },
    AllocationFailed { cells: usize },
    StaleRevision { expected: u64, actual: u64 },
    BatchTooLarge { requested: usize, limit: usize },
    CellOutOfBounds { cell: [u32; 3], extent: [u32; 3] },
    CellCoordinateTooLarge { cell: [u32; 3] },
    RevisionOverflow,
}

impl fmt::Display for VoxelChunkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyExtent => formatter.write_str("voxel chunk extents must be non-zero"),
            Self::ExtentTooLarge { extent } => {
                write!(
                    formatter,
                    "voxel chunk extent exceeds collider coordinates: {extent:?}"
                )
            }
            Self::CellCountOverflow { extent } => {
                write!(
                    formatter,
                    "voxel chunk cell count overflows usize: {extent:?}"
                )
            }
            Self::CellCountMismatch { expected, actual } => write!(
                formatter,
                "voxel chunk expected {expected} cells but received {actual}"
            ),
            Self::AllocationFailed { cells } => {
                write!(formatter, "voxel chunk could not allocate {cells} cells")
            }
            Self::StaleRevision { expected, actual } => write!(
                formatter,
                "voxel patch expected revision {expected}, current revision is {actual}"
            ),
            Self::BatchTooLarge { requested, limit } => {
                write!(
                    formatter,
                    "voxel patch has {requested} edits, limit is {limit}"
                )
            }
            Self::CellOutOfBounds { cell, extent } => {
                write!(
                    formatter,
                    "voxel cell {cell:?} is outside extent {extent:?}"
                )
            }
            Self::CellCoordinateTooLarge { cell } => {
                write!(
                    formatter,
                    "voxel cell {cell:?} exceeds collider coordinates"
                )
            }
            Self::RevisionOverflow => formatter.write_str("voxel chunk revision overflow"),
        }
    }
}

impl Error for VoxelChunkError {}

fn validate_extent(extent: [u32; 3]) -> Result<(), VoxelChunkError> {
    if extent.contains(&0) {
        return Err(VoxelChunkError::EmptyExtent);
    }
    if extent.iter().any(|value| *value > i32::MAX as u32) {
        return Err(VoxelChunkError::ExtentTooLarge { extent });
    }
    Ok(())
}

fn cell_count(extent: [u32; 3]) -> Result<usize, VoxelChunkError> {
    validate_extent(extent)?;
    extent
        .into_iter()
        .try_fold(1usize, |count, axis| count.checked_mul(axis as usize))
        .ok_or(VoxelChunkError::CellCountOverflow { extent })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_world_cells_split_with_euclidean_coordinates() {
        let address = split_voxel_address([-1, -9, 8], [8, 8, 8]).unwrap();
        assert_eq!(address.chunk, [-1, -2, 1]);
        assert_eq!(address.local, [7, 7, 0]);
    }

    #[test]
    fn effective_patch_advances_once_and_publishes_one_dirty_box() {
        let mut chunk = VoxelChunk::new([4, 4, 4], 0u8).unwrap();
        let patch = chunk
            .apply_edits(
                0,
                4,
                [
                    VoxelCellEdit {
                        cell: [0, 0, 0],
                        value: 2,
                    },
                    VoxelCellEdit {
                        cell: [1, 1, 1],
                        value: 3,
                    },
                    VoxelCellEdit {
                        cell: [0, 0, 0],
                        value: 4,
                    },
                ],
            )
            .unwrap();

        assert_eq!(patch.previous_revision, 0);
        assert_eq!(patch.revision, 1);
        assert_eq!(patch.requested, 3);
        assert_eq!(patch.changes.len(), 2);
        assert_eq!(chunk.get([0, 0, 0]), Some(&4));
        assert_eq!(
            patch.dirty_regions,
            [VoxelRegion {
                origin: [0, 0, 0],
                extent: [2, 2, 2],
            }]
        );
        assert_eq!(chunk.dirty_regions(), patch.dirty_regions);
    }

    #[test]
    fn refused_and_noop_patches_leave_authoritative_state_unchanged() {
        let mut chunk = VoxelChunk::new([2, 2, 2], 0u8).unwrap();
        let before = chunk.clone();
        assert!(matches!(
            chunk.apply_edits(
                1,
                1,
                [VoxelCellEdit {
                    cell: [0, 0, 0],
                    value: 1,
                }]
            ),
            Err(VoxelChunkError::StaleRevision { .. })
        ));
        assert_eq!(chunk, before);
        assert!(matches!(
            chunk.apply_edits(
                0,
                2,
                [
                    VoxelCellEdit {
                        cell: [0, 0, 0],
                        value: 1,
                    },
                    VoxelCellEdit {
                        cell: [2, 0, 0],
                        value: 1,
                    },
                ]
            ),
            Err(VoxelChunkError::CellOutOfBounds { .. })
        ));
        assert_eq!(chunk, before);
        assert!(matches!(
            chunk.apply_edits(
                0,
                0,
                [VoxelCellEdit {
                    cell: [0, 0, 0],
                    value: 1,
                }]
            ),
            Err(VoxelChunkError::BatchTooLarge { .. })
        ));
        assert_eq!(chunk, before);

        let noop = chunk
            .apply_edits(
                0,
                1,
                [VoxelCellEdit {
                    cell: [0, 0, 0],
                    value: 0,
                }],
            )
            .unwrap();
        assert_eq!(noop.revision, 0);
        assert!(noop.changes.is_empty());
        assert!(chunk.dirty_regions().is_empty());
    }

    #[test]
    fn material_patch_lowers_only_occupancy_changes() {
        let mut chunk = VoxelChunk::new([4, 4, 4], 0u8).unwrap();
        chunk
            .apply_edits(
                0,
                1,
                [VoxelCellEdit {
                    cell: [0, 0, 0],
                    value: 1,
                }],
            )
            .unwrap();
        let patch = chunk
            .apply_edits(
                1,
                3,
                [
                    VoxelCellEdit {
                        cell: [0, 0, 0],
                        value: 2,
                    },
                    VoxelCellEdit {
                        cell: [1, 0, 0],
                        value: 3,
                    },
                ],
            )
            .unwrap();
        let collision_edits = patch.occupancy_edits(|material| *material != 0).unwrap();
        assert_eq!(collision_edits.len(), 1, "material-only edits stay out");
        assert_eq!(collision_edits[0].cell, [1, 0, 0]);
    }

    #[test]
    fn draining_dirt_does_not_change_record_equality() {
        let mut chunk = VoxelChunk::new([2, 2, 2], 0u8).unwrap();
        chunk
            .apply_edits(
                0,
                1,
                [VoxelCellEdit {
                    cell: [1, 1, 1],
                    value: 1,
                }],
            )
            .unwrap();
        let with_dirt = chunk.clone();
        assert_eq!(chunk.drain_dirty_regions().len(), 1);
        assert_eq!(chunk, with_dirt);
    }

    #[test]
    fn serialization_keeps_facts_and_drops_projection_dirt() {
        let mut chunk = VoxelChunk::new([2, 2, 2], 0u8).unwrap();
        chunk
            .apply_edits(
                0,
                1,
                [VoxelCellEdit {
                    cell: [1, 0, 1],
                    value: 7,
                }],
            )
            .unwrap();

        let encoded = serde_json::to_vec(&chunk).unwrap();
        let decoded: VoxelChunk<u8> = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, chunk);
        assert!(decoded.dirty_regions().is_empty());
        assert_eq!(decoded.get([1, 0, 1]), Some(&7));
    }
}
