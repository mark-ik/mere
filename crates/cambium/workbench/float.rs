// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The float layer: tiles presented outside the tiled tree, and the
//! [`Workspace`] that holds both stations under one rule — every tile is in
//! exactly one place. Ported from Turnstone's `SpaceBlueprint` (its A5 float
//! layer) so the shared tree carries what its hosts had to keep beside it.
//! Geometry stays proportional here; a host resolves a float against its own
//! area with [`FloatingTile::resolve`].

use crate::{
    DropTarget, Edge, Tile, TileEvent, TileId, TileTree, WorkbenchEffect, WorkbenchOutcome,
};

/// A float's rectangle as fractions of the host area, so a layout survives a
/// window resize with its intent intact.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RelativeRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Pixel bounds applied after a float's proportional rectangle is resolved,
/// so a useful pane never shrinks below its controls or grows past its area.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FloatSizeConstraints {
    pub min_width: f32,
    pub min_height: f32,
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
}

impl Default for FloatSizeConstraints {
    fn default() -> Self {
        Self {
            min_width: 0.0,
            min_height: 0.0,
            max_width: None,
            max_height: None,
        }
    }
}

/// A tile at its floating station. The tile itself lives here while it
/// floats, since it has left the tree.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FloatingTile {
    pub tile: Tile,
    pub rect: RelativeRect,
    #[cfg_attr(feature = "serde", serde(default))]
    pub constraints: FloatSizeConstraints,
    /// Stacking order; higher is nearer. Normalized to `1..=n` after a raise.
    pub z: u32,
    /// Stays present when the host hides the float layer.
    pub pinned: bool,
    pub visible: bool,
}

impl FloatingTile {
    /// The float's pixel rectangle inside `area` (`x, y, w, h`): the
    /// proportional rect scaled and clamped by the constraints, then kept
    /// inside the area. Non-finite inputs read as zero.
    pub fn resolve(&self, area: (f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
        let (ax, ay, aw, ah) = area;
        let c = &self.constraints;
        let width = constrained_extent(aw, self.rect.width, c.min_width, c.max_width);
        let height = constrained_extent(ah, self.rect.height, c.min_height, c.max_height);
        let x = (ax + finite(self.rect.x) * aw).clamp(ax, (ax + aw - width).max(ax));
        let y = (ay + finite(self.rect.y) * ah).clamp(ay, (ay + ah - height).max(ay));
        (x, y, width, height)
    }
}

fn constrained_extent(available: f32, proportion: f32, min: f32, max: Option<f32>) -> f32 {
    let available = available.max(0.0);
    let min = finite(min).max(0.0).min(available);
    let max = max.map(finite).unwrap_or(available).max(min).min(available);
    (available * finite(proportion).max(0.0)).clamp(min, max)
}

fn finite(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

/// Where a float docks back into the tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FloatDockTarget {
    /// Become the tree. Refused when the tree already has tiles, because
    /// replacing it would silently orphan them.
    TiledRoot,
    /// Split out beside `target` on `edge`.
    Beside { target: TileId, edge: Edge },
    /// Join `target`'s stack after it, and become the active tab.
    Tab { target: TileId },
}

/// A command against the float layer.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FloatEvent {
    /// Move a tiled tile to the float layer at `rect`, on top.
    Float {
        tile: TileId,
        rect: RelativeRect,
    },
    Dock {
        tile: TileId,
        target: FloatDockTarget,
    },
    Raise(TileId),
    SetRect {
        tile: TileId,
        rect: RelativeRect,
    },
    SetConstraints {
        tile: TileId,
        constraints: FloatSizeConstraints,
    },
    SetPinned {
        tile: TileId,
        pinned: bool,
    },
    SetVisible {
        tile: TileId,
        visible: bool,
    },
}

/// A command against a [`Workspace`]: either station's vocabulary.
#[derive(Clone, Debug, PartialEq)]
pub enum WorkspaceEvent {
    Tile(TileEvent),
    Float(FloatEvent),
}

impl From<TileEvent> for WorkspaceEvent {
    fn from(event: TileEvent) -> Self {
        Self::Tile(event)
    }
}

impl From<FloatEvent> for WorkspaceEvent {
    fn from(event: FloatEvent) -> Self {
        Self::Float(event)
    }
}

/// The tiled tree plus the float layer, with each tile in exactly one place.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Workspace {
    tiled: TileTree,
    floating: Vec<FloatingTile>,
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new(TileTree::empty())
    }
}

impl Workspace {
    pub fn new(tiled: TileTree) -> Self {
        Self {
            tiled,
            floating: Vec::new(),
        }
    }

    pub fn tiled(&self) -> &TileTree {
        &self.tiled
    }

    /// Mutable tree access for host-owned restoration; the station rule is
    /// the caller's to keep while it holds this.
    pub fn tiled_mut(&mut self) -> &mut TileTree {
        &mut self.tiled
    }

    pub fn floating(&self) -> &[FloatingTile] {
        &self.floating
    }

    /// Floats to present, nearest last: the visible ones, or only the pinned
    /// ones while the host hides the float layer.
    pub fn visible_floating(&self, layer_visible: bool) -> Vec<&FloatingTile> {
        let mut floats: Vec<_> = self
            .floating
            .iter()
            .filter(|f| f.visible && (layer_visible || f.pinned))
            .collect();
        floats.sort_by_key(|f| (f.z, f.tile.id.0));
        floats
    }

    pub fn contains(&self, id: TileId) -> bool {
        self.tiled.find(id).is_some() || self.floating.iter().any(|f| f.tile.id == id)
    }

    pub fn find(&self, id: TileId) -> Option<&Tile> {
        self.tiled.find(id).or_else(|| {
            self.floating
                .iter()
                .find(|f| f.tile.id == id)
                .map(|f| &f.tile)
        })
    }

    /// Every tile at either station.
    pub fn tiles(&self) -> Vec<&Tile> {
        let mut out = self.tiled.tiles();
        out.extend(self.floating.iter().map(|f| &f.tile));
        out
    }

    /// Apply one command. Tile events reach the tree, with two float-aware
    /// readings: closing a floating tile removes it, and activating one raises
    /// it. An outside drop of any tile is the host's tear-out effect.
    pub fn apply(&mut self, event: &WorkspaceEvent) -> WorkbenchOutcome {
        let changed = match event {
            WorkspaceEvent::Tile(TileEvent::Dragged {
                tile,
                to: DropTarget::Outside,
            }) => {
                return if self.contains(*tile) {
                    WorkbenchOutcome::Effect(WorkbenchEffect::TearOut { tile: *tile })
                } else {
                    WorkbenchOutcome::Unchanged
                };
            }
            WorkspaceEvent::Tile(TileEvent::Closed(id)) if self.is_floating(*id) => {
                self.floating.retain(|f| f.tile.id != *id);
                true
            }
            WorkspaceEvent::Tile(TileEvent::Activated(id)) if self.is_floating(*id) => {
                self.raise(*id)
            }
            WorkspaceEvent::Tile(event) => self.tiled.apply(event),
            WorkspaceEvent::Float(event) => self.apply_float(*event),
        };
        if changed {
            WorkbenchOutcome::Applied
        } else {
            WorkbenchOutcome::Unchanged
        }
    }

    fn is_floating(&self, id: TileId) -> bool {
        self.floating.iter().any(|f| f.tile.id == id)
    }

    fn apply_float(&mut self, event: FloatEvent) -> bool {
        match event {
            FloatEvent::Float { tile, rect } => self.float(tile, rect),
            FloatEvent::Dock { tile, target } => self.dock(tile, target),
            FloatEvent::Raise(tile) => self.raise(tile),
            FloatEvent::SetRect { tile, rect } => self.with_float(tile, |f| f.rect = rect),
            FloatEvent::SetConstraints { tile, constraints } => {
                self.with_float(tile, |f| f.constraints = constraints)
            }
            FloatEvent::SetPinned { tile, pinned } => self.with_float(tile, |f| f.pinned = pinned),
            FloatEvent::SetVisible { tile, visible } => {
                self.with_float(tile, |f| f.visible = visible)
            }
        }
    }

    fn with_float(&mut self, id: TileId, edit: impl FnOnce(&mut FloatingTile)) -> bool {
        match self.floating.iter_mut().find(|f| f.tile.id == id) {
            Some(float) => {
                edit(float);
                true
            }
            None => false,
        }
    }

    /// Move `id` from the tree (or re-place it if already floating) to the
    /// float layer at `rect`, on top, keeping its constraints and pin.
    fn float(&mut self, id: TileId, rect: RelativeRect) -> bool {
        let (tile, prior) = if let Some(i) = self.floating.iter().position(|f| f.tile.id == id) {
            let prior = self.floating.remove(i);
            (prior.tile.clone(), Some(prior))
        } else if let Some(tile) = self.tiled.take_tile(id) {
            (tile, None)
        } else {
            return false;
        };
        let z = self.top_z() + 1;
        self.floating.push(FloatingTile {
            tile,
            rect,
            constraints: prior.as_ref().map(|p| p.constraints).unwrap_or_default(),
            z,
            pinned: prior.as_ref().is_some_and(|p| p.pinned),
            visible: prior.as_ref().is_none_or(|p| p.visible),
        });
        true
    }

    fn dock(&mut self, id: TileId, target: FloatDockTarget) -> bool {
        let Some(i) = self.floating.iter().position(|f| f.tile.id == id) else {
            return false;
        };
        let tile = self.floating[i].tile.clone();
        let docked = match target {
            FloatDockTarget::TiledRoot => {
                if !self.tiled.is_empty() {
                    return false;
                }
                self.tiled = TileTree::single(tile);
                true
            }
            FloatDockTarget::Beside { target, edge } => self.tiled.split_beside(target, edge, tile),
            FloatDockTarget::Tab { target } => self.tiled.insert_tab_after(target, tile),
        };
        if docked {
            self.floating.remove(i);
        }
        docked
    }

    fn raise(&mut self, id: TileId) -> bool {
        if !self.is_floating(id) {
            return false;
        }
        self.normalize_z();
        let top = self.floating.len() as u32 + 1;
        self.with_float(id, |f| f.z = top);
        self.normalize_z();
        true
    }

    fn top_z(&self) -> u32 {
        self.floating.iter().map(|f| f.z).max().unwrap_or(0)
    }

    fn normalize_z(&mut self) {
        self.floating.sort_by_key(|f| (f.z, f.tile.id.0));
        for (index, float) in self.floating.iter_mut().enumerate() {
            float.z = index as u32 + 1;
        }
    }

    /// Remove a floating tile with its station, for a move to another
    /// workspace (a tear-out to a window). `None` when it is not floating.
    pub fn take_floating(&mut self, id: TileId) -> Option<FloatingTile> {
        let i = self.floating.iter().position(|f| f.tile.id == id)?;
        Some(self.floating.remove(i))
    }

    /// Remove a tile from whichever station holds it.
    pub fn take_tile(&mut self, id: TileId) -> Option<Tile> {
        if let Some(float) = self.take_floating(id) {
            return Some(float.tile);
        }
        self.tiled.take_tile(id)
    }

    /// Place a tile that arrived from elsewhere at a floating station, keeping
    /// its rect, constraints and pin, on top. Refused if the id is present.
    pub fn insert_floating(&mut self, mut float: FloatingTile) -> bool {
        if self.contains(float.tile.id) {
            return false;
        }
        float.z = self.top_z() + 1;
        self.floating.push(float);
        true
    }

    /// Float a tile that arrived from elsewhere at `rect`. Refused if the id
    /// is present.
    pub fn insert_tile_floating(&mut self, tile: Tile, rect: RelativeRect) -> bool {
        self.insert_floating(FloatingTile {
            tile,
            rect,
            constraints: FloatSizeConstraints::default(),
            z: 0,
            pinned: false,
            visible: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ContentSource, SplitAxis, TabStack, TileBranch};

    fn pane(id: u64, kind: &str) -> Tile {
        Tile {
            id: TileId(id),
            title: kind.to_string(),
            content: ContentSource::Open {
                kind: format!("turnstone.{kind}"),
                id: String::new(),
            },
            accent: None,
        }
    }

    fn rect() -> RelativeRect {
        RelativeRect {
            x: 0.25,
            y: 0.25,
            width: 0.5,
            height: 0.5,
        }
    }

    fn two_up() -> Workspace {
        Workspace::new(TileTree::split(
            SplitAxis::Row,
            vec![
                TileBranch::new(0.5, TileTree::single(pane(1, "graph"))),
                TileBranch::new(0.5, TileTree::single(pane(2, "roster"))),
            ],
        ))
    }

    fn stations(ws: &Workspace, id: TileId) -> (bool, bool) {
        (ws.tiled().find(id).is_some(), ws.is_floating(id))
    }

    /// One tile id walks tiled → float → nested split → float → another
    /// workspace → back → tab, and is at exactly one station throughout.
    #[test]
    fn a_tile_moves_through_every_station_and_is_at_one_station_throughout() {
        let mut primary = two_up();
        let roster = TileId(2);
        assert_eq!(stations(&primary, roster), (true, false));

        assert!(
            primary
                .apply(
                    &FloatEvent::Float {
                        tile: roster,
                        rect: rect()
                    }
                    .into()
                )
                .changed()
        );
        assert_eq!(stations(&primary, roster), (false, true));
        assert_eq!(
            primary.tiled().tiles().len(),
            1,
            "the split collapsed to the graph"
        );

        let beside = FloatDockTarget::Beside {
            target: TileId(1),
            edge: Edge::Bottom,
        };
        assert!(
            primary
                .apply(
                    &FloatEvent::Dock {
                        tile: roster,
                        target: beside
                    }
                    .into()
                )
                .changed()
        );
        assert_eq!(stations(&primary, roster), (true, false));
        assert!(matches!(
            primary.tiled(),
            TileTree::Split {
                axis: SplitAxis::Column,
                ..
            }
        ));

        assert!(
            primary
                .apply(
                    &FloatEvent::Float {
                        tile: roster,
                        rect: rect()
                    }
                    .into()
                )
                .changed()
        );
        let outside = TileEvent::Dragged {
            tile: roster,
            to: DropTarget::Outside,
        };
        assert_eq!(
            primary.apply(&outside.into()),
            WorkbenchOutcome::Effect(WorkbenchEffect::TearOut { tile: roster }),
            "an outside drop is the host's effect, and the tile stays put"
        );
        assert_eq!(stations(&primary, roster), (false, true));

        let float = primary.take_floating(roster).unwrap();
        assert!(!primary.contains(roster));
        let mut lens = Workspace::default();
        assert!(lens.insert_floating(float.clone()));
        assert!(
            !lens.insert_floating(float),
            "the station rule refuses a second copy"
        );
        assert_eq!(
            lens.find(roster),
            Some(&pane(2, "roster")),
            "identity travelled"
        );
        assert_eq!(lens.floating()[0].rect, rect(), "so did its rect");

        let back = lens.take_floating(roster).unwrap();
        assert!(primary.insert_floating(back));
        let tab = FloatDockTarget::Tab { target: TileId(1) };
        assert!(
            primary
                .apply(
                    &FloatEvent::Dock {
                        tile: roster,
                        target: tab
                    }
                    .into()
                )
                .changed()
        );
        assert_eq!(stations(&primary, roster), (true, false));
        match primary.tiled() {
            TileTree::Stack(TabStack { tabs, active }) => {
                assert_eq!(tabs.iter().map(|t| t.id.0).collect::<Vec<_>>(), vec![1, 2]);
                assert_eq!(*active, 1, "the docked tab is the active one");
            }
            other => panic!("expected a two-tab stack, got {other:?}"),
        }
    }

    #[test]
    fn docking_to_the_root_needs_an_empty_tree() {
        let mut ws = two_up();
        ws.apply(
            &FloatEvent::Float {
                tile: TileId(2),
                rect: rect(),
            }
            .into(),
        );
        let root = FloatDockTarget::TiledRoot;
        assert!(
            !ws.apply(
                &FloatEvent::Dock {
                    tile: TileId(2),
                    target: root
                }
                .into()
            )
            .changed()
        );
        ws.apply(
            &FloatEvent::Float {
                tile: TileId(1),
                rect: rect(),
            }
            .into(),
        );
        assert!(ws.tiled().is_empty());
        assert!(
            ws.apply(
                &FloatEvent::Dock {
                    tile: TileId(2),
                    target: root
                }
                .into()
            )
            .changed()
        );
        assert_eq!(ws.tiled().tiles().len(), 1);
    }

    #[test]
    fn raise_reorders_z_and_pinned_floats_outlive_a_hidden_layer() {
        let mut ws = two_up();
        ws.apply(
            &FloatEvent::Float {
                tile: TileId(1),
                rect: rect(),
            }
            .into(),
        );
        ws.apply(
            &FloatEvent::Float {
                tile: TileId(2),
                rect: rect(),
            }
            .into(),
        );
        let order = |ws: &Workspace| {
            ws.visible_floating(true)
                .iter()
                .map(|f| f.tile.id.0)
                .collect::<Vec<_>>()
        };
        assert_eq!(order(&ws), vec![1, 2]);
        assert!(
            ws.apply(&TileEvent::Activated(TileId(1)).into()).changed(),
            "activating a float raises it"
        );
        assert_eq!(order(&ws), vec![2, 1]);
        assert_eq!(
            ws.floating().iter().map(|f| f.z).max(),
            Some(2),
            "z stays normalized"
        );

        ws.apply(
            &FloatEvent::SetPinned {
                tile: TileId(2),
                pinned: true,
            }
            .into(),
        );
        assert_eq!(
            ws.visible_floating(false)
                .iter()
                .map(|f| f.tile.id.0)
                .collect::<Vec<_>>(),
            vec![2]
        );
        assert!(ws.apply(&TileEvent::Closed(TileId(2)).into()).changed());
        assert!(!ws.contains(TileId(2)));
    }

    #[test]
    fn a_float_resolves_inside_its_area_within_its_constraints() {
        let mut ws = two_up();
        ws.apply(
            &FloatEvent::Float {
                tile: TileId(2),
                rect: rect(),
            }
            .into(),
        );
        ws.apply(
            &FloatEvent::SetConstraints {
                tile: TileId(2),
                constraints: FloatSizeConstraints {
                    min_width: 300.0,
                    min_height: 0.0,
                    max_width: None,
                    max_height: Some(100.0),
                },
            }
            .into(),
        );
        let float = &ws.floating()[0];
        let (x, y, w, h) = float.resolve((0.0, 0.0, 400.0, 400.0));
        assert_eq!((w, h), (300.0, 100.0), "min width and max height applied");
        assert_eq!(
            (x, y),
            (100.0, 100.0),
            "x clamped so the float stays inside"
        );
        let nan = FloatingTile {
            rect: RelativeRect {
                x: f32::NAN,
                y: 0.0,
                width: f32::INFINITY,
                height: 0.5,
            },
            ..float.clone()
        };
        let (x, _, w, _) = nan.resolve((0.0, 0.0, 400.0, 400.0));
        assert_eq!(
            (x, w),
            (0.0, 300.0),
            "non-finite reads as zero, then the minimum holds"
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn a_workspace_round_trips_through_serde_with_every_station_intact() {
        let mut ws = two_up();
        ws.apply(
            &FloatEvent::Float {
                tile: TileId(2),
                rect: rect(),
            }
            .into(),
        );
        ws.apply(
            &FloatEvent::SetPinned {
                tile: TileId(2),
                pinned: true,
            }
            .into(),
        );
        let json = serde_json::to_string(&ws).unwrap();
        let restored: Workspace = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, ws);
    }
}
