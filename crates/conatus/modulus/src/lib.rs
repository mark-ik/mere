// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Product-neutral sparse-brick carriage and voxel traversal.
//!
//! This crate owns one presentation ABI: a three-dimensional pointer volume
//! whose zero value means air and whose other values select dense 8-cubed
//! material slots in an R8 atlas. [`BRICK_DDA_WGSL`] traverses that ABI from a
//! caller-supplied ray. Products still own working-set selection, source
//! revision, camera construction, material appearance, and final composition.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

use bytemuck::{Pod, Zeroable};

pub const BRICK_EDGE: u32 = 8;
pub const ATLAS_SLOTS_X: u32 = 16;
pub const ATLAS_SLOTS_Z: u32 = 16;
pub const MAX_ATLAS_SLOTS_Y: u32 = 8;
pub const MAX_BRICKS: usize = (ATLAS_SLOTS_X * ATLAS_SLOTS_Z * MAX_ATLAS_SLOTS_Y - 1) as usize;

/// The shared WGSL module for pointer lookup, ray-box clipping, and voxel DDA.
///
/// It declares pointer and atlas textures at group 0 bindings 0 and 1. A
/// product shader supplies rays and passes its [`BrickTraceSpace`] plus its own
/// far cut to `brick_dda`. The module contains no camera, lighting, material,
/// body, or composition policy.
pub const BRICK_DDA_WGSL: &str = include_str!("brick_dda.wgsl");

pub type BrickKey = [i16; 3];

/// Host-issued revision of one selected key-to-slot projection.
///
/// This is disposable presentation identity, separate from a product's source
/// revision. The working-set owner advances it when selection or slot
/// assignment changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct BrickProjectionRevision(pub u64);

/// The exact map fields consumed by [`BRICK_DDA_WGSL`].
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct BrickTraceSpace {
    pub world_min: [f32; 4],
    pub pointer_extent: [u32; 4],
    pub atlas_slots: [u32; 4],
}

impl BrickTraceSpace {
    pub fn from_map(map: &BrickMap) -> Self {
        Self {
            world_min: [
                f32::from(map.origin[0]) * BRICK_EDGE as f32,
                f32::from(map.origin[1]) * BRICK_EDGE as f32,
                f32::from(map.origin[2]) * BRICK_EDGE as f32,
                0.0,
            ],
            pointer_extent: [
                map.pointer_extent[0],
                map.pointer_extent[1],
                map.pointer_extent[2],
                0,
            ],
            atlas_slots: [map.slots[0], map.slots[1], map.slots[2], 0],
        }
    }
}

/// Dense material data arranged for a `texture_3d<u32>` pointer volume and
/// `texture_3d<u32>` material atlas.
#[derive(Clone, Debug)]
pub struct BrickMap {
    projection_revision: BrickProjectionRevision,
    origin: BrickKey,
    pointer_extent: [u32; 3],
    slots: [u32; 3],
    pointers: Vec<u32>,
    atlas: Vec<u8>,
    key_slots: BTreeMap<BrickKey, u32>,
}

impl BrickMap {
    /// Builds a deterministic bounded map from product-selected brick keys.
    ///
    /// `brick` returns one dense material brick in Y-Z-X order, with X
    /// contiguous. Missing keys and payloads other than exactly 8 cubed bytes
    /// are refused. Selection policy and source authority remain with the
    /// caller.
    pub fn from_keys<'a>(
        projection_revision: BrickProjectionRevision,
        keys: impl IntoIterator<Item = BrickKey>,
        mut brick: impl FnMut(BrickKey) -> Option<&'a [u8]>,
    ) -> Result<Self, BrickMapError> {
        let keys: Vec<_> = keys
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if keys.len() > MAX_BRICKS {
            return Err(BrickMapError::TooManyBricks {
                actual: keys.len(),
                maximum: MAX_BRICKS,
            });
        }

        let (origin, pointer_extent) = bounds(&keys);
        let pointer_len = pointer_extent
            .into_iter()
            .try_fold(1usize, |total, axis| total.checked_mul(axis as usize))
            .ok_or(BrickMapError::PointerVolumeOverflow { pointer_extent })?;
        let mut pointers = Vec::new();
        pointers
            .try_reserve_exact(pointer_len)
            .map_err(|_| BrickMapError::AllocationFailed {
                entries: pointer_len,
            })?;
        pointers.resize(pointer_len, 0);

        let slot_count = keys.len() as u32 + 1;
        let slots = [
            ATLAS_SLOTS_X,
            slot_count.div_ceil(ATLAS_SLOTS_X * ATLAS_SLOTS_Z).max(1),
            ATLAS_SLOTS_Z,
        ];
        let atlas_extent = atlas_extent(slots);
        let atlas_len = atlas_extent
            .into_iter()
            .try_fold(1usize, |total, axis| total.checked_mul(axis as usize))
            .expect("the fixed atlas bounds fit usize");
        let key_slots = keys
            .iter()
            .enumerate()
            .map(|(index, key)| (*key, index as u32 + 1))
            .collect();
        let mut map = Self {
            projection_revision,
            origin,
            pointer_extent,
            slots,
            pointers,
            atlas: vec![0; atlas_len],
            key_slots,
        };
        map.refresh(keys, &mut brick)?;
        Ok(map)
    }

    /// Copies changed source bricks and returns their stable atlas slots.
    pub fn refresh<'a>(
        &mut self,
        changed: impl IntoIterator<Item = BrickKey>,
        mut brick: impl FnMut(BrickKey) -> Option<&'a [u8]>,
    ) -> Result<Vec<u32>, BrickMapError> {
        let mut staged = BTreeMap::new();
        for key in changed {
            let Some(&slot) = self.key_slots.get(&key) else {
                return Err(BrickMapError::UnknownKey { key });
            };
            let raw = brick(key).ok_or(BrickMapError::MissingBrick { key })?;
            if raw.len() != BRICK_EDGE.pow(3) as usize {
                return Err(BrickMapError::InvalidBrickLength {
                    key,
                    actual: raw.len(),
                    expected: BRICK_EDGE.pow(3) as usize,
                });
            }
            staged.insert(slot, (key, raw));
        }

        for (&slot, &(key, raw)) in &staged {
            let pointer = self.pointer_index(key);
            self.pointers[pointer] = u32::from(raw.iter().any(|material| *material != 0)) * slot;
            self.write_slot(slot, raw);
        }
        Ok(staged.into_keys().collect())
    }

    /// Builds an empty capacity-fixed map whose pointer and atlas extents
    /// never change across [`Self::retarget`] calls.
    ///
    /// `capacity_rows` fixes the atlas at `16 x capacity_rows x 16` slot
    /// spots (one of which is the reserved air slot), and `pointer_extent`
    /// fixes the pointer volume; a retarget moves the volume's origin but
    /// never its extent. A consumer keying texture identity to these
    /// extents therefore never reallocates while the map lives.
    pub fn with_capacity(
        projection_revision: BrickProjectionRevision,
        capacity_rows: u32,
        pointer_extent: [u32; 3],
    ) -> Result<Self, BrickMapError> {
        let slots = [ATLAS_SLOTS_X, capacity_rows.max(1), ATLAS_SLOTS_Z];
        let usable = (slots[0] * slots[1] * slots[2] - 1) as usize;
        if usable > MAX_BRICKS {
            return Err(BrickMapError::TooManyBricks {
                actual: usable,
                maximum: MAX_BRICKS,
            });
        }
        let pointer_len = pointer_extent
            .into_iter()
            .try_fold(1usize, |total, axis| {
                total.checked_mul(axis as usize).filter(|_| axis > 0)
            })
            .ok_or(BrickMapError::PointerVolumeOverflow { pointer_extent })?;
        let mut pointers = Vec::new();
        pointers
            .try_reserve_exact(pointer_len)
            .map_err(|_| BrickMapError::AllocationFailed {
                entries: pointer_len,
            })?;
        pointers.resize(pointer_len, 0);
        let atlas_len = atlas_extent(slots)
            .into_iter()
            .try_fold(1usize, |total, axis| total.checked_mul(axis as usize))
            .expect("the fixed atlas bounds fit usize");
        Ok(Self {
            projection_revision,
            origin: [0; 3],
            pointer_extent,
            slots,
            pointers,
            atlas: vec![0; atlas_len],
            key_slots: BTreeMap::new(),
        })
    }

    /// How many bricks this map's atlas can hold.
    pub const fn capacity(&self) -> usize {
        (self.slots[0] * self.slots[1] * self.slots[2] - 1) as usize
    }

    /// Replaces the selection while retaining every kept brick's atlas
    /// slot, so a consumer republishing a retarget touches only the
    /// pointer volume and the loaded bricks' slots.
    ///
    /// Evicted slots recycle deterministically: the sorted free slots go
    /// to the sorted loaded keys in order. The pointer volume is rewritten
    /// wholesale at the new selection's origin; retained bricks' occupancy
    /// carries over from their previous pointer values, so their source is
    /// never refetched. Every refusal happens before the first write. An
    /// unchanged selection is a no-op; a changed one must advance the
    /// projection revision, per the working-set owner rule.
    pub fn retarget<'a>(
        &mut self,
        projection_revision: BrickProjectionRevision,
        keys: impl IntoIterator<Item = BrickKey>,
        mut brick: impl FnMut(BrickKey) -> Option<&'a [u8]>,
    ) -> Result<RetargetDelta, BrickMapError> {
        let keys: BTreeSet<_> = keys.into_iter().collect();
        if keys.len() > self.capacity() {
            return Err(BrickMapError::TooManyBricks {
                actual: keys.len(),
                maximum: self.capacity(),
            });
        }
        if keys.iter().eq(self.key_slots.keys()) {
            return Ok(RetargetDelta {
                loaded_slots: Vec::new(),
                evicted: 0,
                retained: keys.len(),
            });
        }
        if projection_revision.0 <= self.projection_revision.0 {
            return Err(BrickMapError::ProjectionNotAdvanced {
                current: self.projection_revision.0,
                offered: projection_revision.0,
            });
        }
        let sorted: Vec<_> = keys.iter().copied().collect();
        let (origin, extent) = bounds(&sorted);
        if (0..3).any(|axis| extent[axis] > self.pointer_extent[axis]) {
            return Err(BrickMapError::ExtentExceeded {
                extent,
                maximum: self.pointer_extent,
            });
        }
        let loaded: Vec<_> = keys
            .iter()
            .filter(|key| !self.key_slots.contains_key(*key))
            .copied()
            .collect();
        let mut payloads = Vec::with_capacity(loaded.len());
        for key in &loaded {
            let raw = brick(*key).ok_or(BrickMapError::MissingBrick { key: *key })?;
            if raw.len() != BRICK_EDGE.pow(3) as usize {
                return Err(BrickMapError::InvalidBrickLength {
                    key: *key,
                    actual: raw.len(),
                    expected: BRICK_EDGE.pow(3) as usize,
                });
            }
            payloads.push(raw);
        }

        // Retained keys keep their slots and their occupancy; everything
        // else about the previous selection is released.
        let mut occupancy: BTreeMap<BrickKey, bool> = BTreeMap::new();
        let mut retained_slots = BTreeSet::new();
        let previous: Vec<_> = self.key_slots.iter().map(|(k, s)| (*k, *s)).collect();
        let mut next_slots: BTreeMap<BrickKey, u32> = BTreeMap::new();
        let mut evicted = 0;
        for (key, slot) in previous {
            if keys.contains(&key) {
                occupancy.insert(key, self.pointers[self.pointer_index(key)] != 0);
                retained_slots.insert(slot);
                next_slots.insert(key, slot);
            } else {
                evicted += 1;
            }
        }
        let mut free = (1..=self.capacity() as u32).filter(|slot| !retained_slots.contains(slot));
        let mut loaded_slots = Vec::with_capacity(loaded.len());
        for (key, raw) in loaded.iter().zip(&payloads) {
            let slot = free.next().expect("capacity was checked before writes");
            next_slots.insert(*key, slot);
            loaded_slots.push((slot, *key, *raw));
            occupancy.insert(*key, raw.iter().any(|material| *material != 0));
        }

        self.origin = origin;
        self.key_slots = next_slots;
        self.projection_revision = projection_revision;
        self.pointers.fill(0);
        let entries: Vec<_> = self.key_slots.iter().map(|(k, s)| (*k, *s)).collect();
        for (key, slot) in entries {
            let index = self.pointer_index(key);
            self.pointers[index] = u32::from(occupancy[&key]) * slot;
        }
        for (slot, _, raw) in &loaded_slots {
            self.write_slot(*slot, raw);
        }
        Ok(RetargetDelta {
            loaded_slots: loaded_slots.into_iter().map(|(slot, ..)| slot).collect(),
            evicted,
            retained: keys.len() - payloads.len(),
        })
    }

    pub const fn projection_revision(&self) -> BrickProjectionRevision {
        self.projection_revision
    }

    pub const fn origin(&self) -> BrickKey {
        self.origin
    }

    pub const fn pointer_extent(&self) -> [u32; 3] {
        self.pointer_extent
    }

    pub fn atlas_extent(&self) -> [u32; 3] {
        atlas_extent(self.slots)
    }

    pub const fn slots(&self) -> [u32; 3] {
        self.slots
    }

    pub fn pointers(&self) -> &[u32] {
        &self.pointers
    }

    pub fn pointer_at(&self, coord: [u32; 3]) -> Option<u32> {
        let [x, y, z] = coord;
        let [width, height, depth] = self.pointer_extent;
        (x < width && y < height && z < depth)
            .then(|| self.pointers[(z * height * width + y * width + x) as usize])
    }

    pub fn atlas(&self) -> &[u8] {
        &self.atlas
    }

    pub fn pointer_coord(&self, slot: u32) -> Option<[u32; 3]> {
        self.key_slots
            .iter()
            .find_map(|(key, found)| (*found == slot).then(|| self.key_coord(*key)))
    }

    pub fn slot_texels(&self, slot: u32) -> Option<Vec<u8>> {
        let [base_x, base_y, base_z] = self.atlas_slot_origin(slot)?;
        let [width, height, _] = self.atlas_extent();
        let mut out = Vec::with_capacity(BRICK_EDGE.pow(3) as usize);
        for z in 0..BRICK_EDGE {
            for y in 0..BRICK_EDGE {
                for x in 0..BRICK_EDGE {
                    out.push(
                        self.atlas[((base_z + z) * height * width
                            + (base_y + y) * width
                            + base_x
                            + x) as usize],
                    );
                }
            }
        }
        Some(out)
    }

    pub fn material_at(&self, at: [i32; 3]) -> u8 {
        let key = at.map(|axis| axis.div_euclid(BRICK_EDGE as i32) as i16);
        let local = at.map(|axis| axis.rem_euclid(BRICK_EDGE as i32) as u32);
        let Some(&slot) = self.key_slots.get(&key) else {
            return 0;
        };
        if self.pointers[self.pointer_index(key)] == 0 {
            return 0;
        }
        let Some([base_x, base_y, base_z]) = self.atlas_slot_origin(slot) else {
            return 0;
        };
        let [width, height, _] = self.atlas_extent();
        self.atlas[((base_z + local[2]) * height * width
            + (base_y + local[1]) * width
            + base_x
            + local[0]) as usize]
    }

    pub fn atlas_slot_origin(&self, slot: u32) -> Option<[u32; 3]> {
        if slot == 0 || slot as usize > self.key_slots.len() {
            return None;
        }
        let index = slot - 1;
        let x = index % self.slots[0];
        let z = (index / self.slots[0]) % self.slots[2];
        let y = index / (self.slots[0] * self.slots[2]);
        Some([x * BRICK_EDGE, y * BRICK_EDGE, z * BRICK_EDGE])
    }

    fn key_coord(&self, key: BrickKey) -> [u32; 3] {
        [0, 1, 2].map(|axis| (i32::from(key[axis]) - i32::from(self.origin[axis])) as u32)
    }

    fn pointer_index(&self, key: BrickKey) -> usize {
        let [x, y, z] = self.key_coord(key);
        let [width, height, _] = self.pointer_extent;
        (z * height * width + y * width + x) as usize
    }

    fn write_slot(&mut self, slot: u32, raw: &[u8]) {
        let [base_x, base_y, base_z] = self.atlas_slot_origin(slot).expect("assigned slot");
        let [width, height, _] = self.atlas_extent();
        for z in 0..BRICK_EDGE {
            for y in 0..BRICK_EDGE {
                for x in 0..BRICK_EDGE {
                    let source = ((y * BRICK_EDGE + z) * BRICK_EDGE + x) as usize;
                    let destination =
                        ((base_z + z) * height * width + (base_y + y) * width + base_x + x)
                            as usize;
                    self.atlas[destination] = raw[source];
                }
            }
        }
    }
}

/// What one [`BrickMap::retarget`] actually moved.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RetargetDelta {
    /// Atlas slots written this retarget, ascending. Retained bricks'
    /// slots are deliberately absent: their bytes did not move.
    pub loaded_slots: Vec<u32>,
    pub evicted: usize,
    pub retained: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrickMapError {
    TooManyBricks {
        actual: usize,
        maximum: usize,
    },
    PointerVolumeOverflow {
        pointer_extent: [u32; 3],
    },
    /// A retargeted selection's bounding box outgrew the fixed pointer
    /// volume.
    ExtentExceeded {
        extent: [u32; 3],
        maximum: [u32; 3],
    },
    /// A changed selection was offered without advancing the projection
    /// revision.
    ProjectionNotAdvanced {
        current: u64,
        offered: u64,
    },
    AllocationFailed {
        entries: usize,
    },
    UnknownKey {
        key: BrickKey,
    },
    MissingBrick {
        key: BrickKey,
    },
    InvalidBrickLength {
        key: BrickKey,
        actual: usize,
        expected: usize,
    },
}

impl fmt::Display for BrickMapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyBricks { actual, maximum } => {
                write!(
                    formatter,
                    "brick map has {actual} bricks; maximum is {maximum}"
                )
            }
            Self::PointerVolumeOverflow { pointer_extent } => {
                write!(
                    formatter,
                    "pointer volume overflows usize: {pointer_extent:?}"
                )
            }
            Self::ExtentExceeded { extent, maximum } => {
                write!(
                    formatter,
                    "selection bounds {extent:?} exceed the fixed pointer extent {maximum:?}"
                )
            }
            Self::ProjectionNotAdvanced { current, offered } => {
                write!(
                    formatter,
                    "a changed selection needs a projection revision past {current}; offered {offered}"
                )
            }
            Self::AllocationFailed { entries } => {
                write!(
                    formatter,
                    "pointer volume could not allocate {entries} entries"
                )
            }
            Self::UnknownKey { key } => write!(formatter, "brick key is not selected: {key:?}"),
            Self::MissingBrick { key } => write!(formatter, "selected brick is missing: {key:?}"),
            Self::InvalidBrickLength {
                key,
                actual,
                expected,
            } => write!(
                formatter,
                "brick {key:?} has {actual} bytes; expected {expected}"
            ),
        }
    }
}

impl Error for BrickMapError {}

fn bounds(keys: &[BrickKey]) -> (BrickKey, [u32; 3]) {
    let mut min = [0; 3];
    let mut max = [0; 3];
    if let Some(first) = keys.first() {
        min = *first;
        max = *first;
        for key in keys {
            for axis in 0..3 {
                min[axis] = min[axis].min(key[axis]);
                max[axis] = max[axis].max(key[axis]);
            }
        }
    }
    let extent = [0, 1, 2].map(|axis| (i32::from(max[axis]) - i32::from(min[axis]) + 1) as u32);
    (min, extent)
}

fn atlas_extent(slots: [u32; 3]) -> [u32; 3] {
    slots.map(|axis| axis * BRICK_EDGE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(material: u8) -> [u8; BRICK_EDGE.pow(3) as usize] {
        [material; BRICK_EDGE.pow(3) as usize]
    }

    #[test]
    fn map_order_and_atlas_layout_are_source_order_independent() {
        let low = solid(2);
        let high = solid(3);
        let source = |key: BrickKey| match key {
            [-1, 0, 0] => Some(low.as_slice()),
            [1, 0, 0] => Some(high.as_slice()),
            _ => None,
        };
        let first = BrickMap::from_keys(
            BrickProjectionRevision(7),
            [[1, 0, 0], [-1, 0, 0], [1, 0, 0]],
            source,
        )
        .unwrap();
        let second =
            BrickMap::from_keys(BrickProjectionRevision(7), [[-1, 0, 0], [1, 0, 0]], source)
                .unwrap();

        assert_eq!(first.origin(), [-1, 0, 0]);
        assert_eq!(first.pointer_extent(), [3, 1, 1]);
        assert_eq!(first.pointers(), second.pointers());
        assert_eq!(first.atlas(), second.atlas());
        assert_eq!(first.material_at([-8, 0, 0]), 2);
        assert_eq!(first.material_at([8, 0, 0]), 3);
        assert_eq!(first.material_at([0, 0, 0]), 0);
        assert_eq!(
            BrickTraceSpace::from_map(&first).world_min,
            [-8.0, 0.0, 0.0, 0.0]
        );
    }

    #[test]
    fn missing_and_malformed_bricks_are_refused() {
        assert!(matches!(
            BrickMap::from_keys(BrickProjectionRevision(0), [[0, 0, 0]], |_| None),
            Err(BrickMapError::MissingBrick { .. })
        ));
        let malformed = [1u8; 8];
        assert!(matches!(
            BrickMap::from_keys(BrickProjectionRevision(0), [[0, 0, 0]], |_| {
                Some(malformed.as_slice())
            }),
            Err(BrickMapError::InvalidBrickLength { .. })
        ));
    }

    #[test]
    fn refused_refresh_writes_nothing() {
        let first = solid(1);
        let replacement = solid(2);
        let mut map = BrickMap::from_keys(BrickProjectionRevision(0), [[0, 0, 0]], |_| {
            Some(first.as_slice())
        })
        .unwrap();
        let pointers = map.pointers.clone();
        let atlas = map.atlas.clone();

        assert!(matches!(
            map.refresh([[0, 0, 0], [1, 0, 0]], |_| Some(replacement.as_slice())),
            Err(BrickMapError::UnknownKey { .. })
        ));
        assert_eq!(map.pointers, pointers);
        assert_eq!(map.atlas, atlas);
    }

    #[test]
    fn a_retarget_retains_slots_and_recycles_evictions_deterministically() {
        let a = solid(2);
        let b = solid(3);
        let c = solid(4);
        let source = |key: BrickKey| match key {
            [0, 0, 0] => Some(a.as_slice()),
            [1, 0, 0] => Some(b.as_slice()),
            [2, 0, 0] => Some(c.as_slice()),
            _ => None,
        };
        let mut map = BrickMap::with_capacity(BrickProjectionRevision(0), 1, [4, 1, 4]).unwrap();
        assert_eq!(map.capacity(), 255);
        let first = map
            .retarget(BrickProjectionRevision(1), [[0, 0, 0], [1, 0, 0]], source)
            .unwrap();
        assert_eq!(first.loaded_slots, vec![1, 2]);
        let b_slot = map.key_slots[&[1, 0, 0]];

        let second = map
            .retarget(BrickProjectionRevision(2), [[1, 0, 0], [2, 0, 0]], source)
            .unwrap();
        assert_eq!(map.key_slots[&[1, 0, 0]], b_slot, "a kept brick keeps its slot");
        assert_eq!(second.evicted, 1);
        assert_eq!(second.retained, 1);
        assert_eq!(
            second.loaded_slots,
            vec![map.key_slots[&[2, 0, 0]]],
            "only the loaded brick's slot was written"
        );
        assert_eq!(
            map.key_slots[&[2, 0, 0]],
            1,
            "the evicted slot recycles to the loaded key"
        );
        assert_eq!(map.material_at([8, 0, 0]), 3);
        assert_eq!(map.material_at([16, 0, 0]), 4);
        assert_eq!(map.material_at([0, 0, 0]), 0, "the evicted brick is gone");
        assert_eq!(map.pointer_extent(), [4, 1, 4], "the extent never moves");

        // The same sequence from scratch lands byte-identically.
        let mut again = BrickMap::with_capacity(BrickProjectionRevision(0), 1, [4, 1, 4]).unwrap();
        again
            .retarget(BrickProjectionRevision(1), [[0, 0, 0], [1, 0, 0]], source)
            .unwrap();
        again
            .retarget(BrickProjectionRevision(2), [[1, 0, 0], [2, 0, 0]], source)
            .unwrap();
        assert_eq!(map.pointers(), again.pointers());
        assert_eq!(map.atlas(), again.atlas());
    }

    #[test]
    fn a_refused_retarget_writes_nothing() {
        let a = solid(2);
        let source = |_: BrickKey| Some(a.as_slice());
        let mut map = BrickMap::with_capacity(BrickProjectionRevision(0), 1, [2, 1, 2]).unwrap();
        map.retarget(BrickProjectionRevision(1), [[0, 0, 0]], source)
            .unwrap();
        let pointers = map.pointers.clone();
        let atlas = map.atlas.clone();
        let held_origin = map.origin();

        assert!(matches!(
            map.retarget(BrickProjectionRevision(2), [[0, 0, 0], [4, 0, 0]], source),
            Err(BrickMapError::ExtentExceeded { .. })
        ));
        assert!(matches!(
            map.retarget(BrickProjectionRevision(1), [[1, 0, 0]], source),
            Err(BrickMapError::ProjectionNotAdvanced { .. })
        ));
        assert!(matches!(
            map.retarget(BrickProjectionRevision(2), [[1, 0, 0]], |_| None),
            Err(BrickMapError::MissingBrick { .. })
        ));
        assert_eq!(map.pointers, pointers);
        assert_eq!(map.atlas, atlas);
        assert_eq!(map.origin(), held_origin);

        // An unchanged selection is a no-op that needs no revision advance.
        let unchanged = map
            .retarget(BrickProjectionRevision(1), [[0, 0, 0]], source)
            .unwrap();
        assert!(unchanged.loaded_slots.is_empty());
        assert_eq!(unchanged.retained, 1);
    }

    #[test]
    fn a_retargeted_map_reads_like_a_rebuilt_one() {
        let low = solid(2);
        let high = solid(3);
        let source = |key: BrickKey| match key {
            [-1, 0, 0] => Some(low.as_slice()),
            [1, 0, 1] => Some(high.as_slice()),
            _ => None,
        };
        let mut travelled =
            BrickMap::with_capacity(BrickProjectionRevision(0), 1, [4, 1, 4]).unwrap();
        travelled
            .retarget(BrickProjectionRevision(1), [[-1, 0, 0]], source)
            .unwrap();
        travelled
            .retarget(BrickProjectionRevision(2), [[-1, 0, 0], [1, 0, 1]], source)
            .unwrap();
        let rebuilt =
            BrickMap::from_keys(BrickProjectionRevision(2), [[-1, 0, 0], [1, 0, 1]], source)
                .unwrap();
        for at in [[-8, 0, 0], [-1, 7, 7], [8, 0, 8], [15, 7, 15], [0, 0, 0]] {
            assert_eq!(travelled.material_at(at), rebuilt.material_at(at), "{at:?}");
        }
        assert_eq!(travelled.origin(), rebuilt.origin());
    }

    #[test]
    fn shared_shader_stops_before_product_policy() {
        assert!(BRICK_DDA_WGSL.contains("fn brick_dda"));
        for forbidden in ["camera", "fog", "light", "material_colour", "critter"] {
            assert!(!BRICK_DDA_WGSL.contains(forbidden), "found {forbidden}");
        }
    }
}
