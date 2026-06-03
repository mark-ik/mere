/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The host's tiled-workbench composition (S4).
//!
//! The orrery and the tiled workbench are two **projections of one
//! arrangement** (the composition spine): the orrery is the
//! [`ProjectionKind::Cartography`] projection (graph members positioned
//! spatially), the tiled workbench the [`ProjectionKind::Tree`] one (members
//! laid out as splits of tab-stacks). This module owns the host-side state for
//! that — which tiles are open, and the current projection mode — and turns it
//! into placed, content-resolved tiles for the render loop.
//!
//! The geometry is not ours: [`platen::project_tree`] turns the arrangement into
//! a geometry-free [`WorkbenchPlan`](platen::WorkbenchPlan), and
//! [`platen::layout_plan`] assigns each slot a concrete rect (morphorm-backed).
//! This module adds only the two host concerns platen leaves open: holding the
//! open-tile set, and resolving each tile's member id (a kernel node UUID) to a
//! node URL against the session graph, so the content lane has something to
//! render.
//!
//! v1 scope: tiles are a flat ordered set, each bound to one graph member, laid
//! side-by-side. Tab-stacks, groups, and re-ordering are forme/platen features
//! already (`StackedWith`, `Group`); surfacing them here waits on the UX that
//! needs them. The render integration (one content actor per laid-out tile) is
//! the actor-constellation plan's P4 and lives in the kernel, not here.

use forme::{Arrangement, GraphMemberId};
use kernel::geometry::{PortableRect, PortableSize};
use kernel::graph::Graph;
use platen::{layout_plan, project_tree, LaidOutSlot, LayoutConfig, ProjectionKind};

/// A placed workbench tile: the band-relative rect it occupies, the graph member
/// it shows, and that member's resolved URL (`None` if the node has since left
/// the graph, so the host can show a "gone" placeholder rather than guess).
#[derive(Clone, Debug, PartialEq)]
pub struct WorkbenchTile {
    pub rect: PortableRect,
    pub member: GraphMemberId,
    pub url: Option<String>,
}

/// The host's tiled-workbench composition: the open tiles and the projection
/// mode. Cartography (the orrery) is the default; the host flips to Tree for the
/// tiled view.
#[derive(Clone, Debug)]
pub struct Workbench {
    mode: ProjectionKind,
    /// Open tiles, in placement order; each bound to a graph member (node UUID).
    open: Vec<GraphMemberId>,
}

impl Default for Workbench {
    fn default() -> Self {
        Self::new()
    }
}

impl Workbench {
    /// A new workbench: no open tiles, in Cartography (orrery) mode.
    pub fn new() -> Self {
        Self { mode: ProjectionKind::Cartography, open: Vec::new() }
    }

    /// The current projection mode.
    pub fn mode(&self) -> ProjectionKind {
        self.mode
    }

    /// Whether the workbench is in the tiled (Tree) projection. In Cartography
    /// mode the host renders the orrery as usual and ignores the open-tile set.
    pub fn is_tiled(&self) -> bool {
        matches!(self.mode, ProjectionKind::Tree)
    }

    /// Switch between the orrery (Cartography) and the tiled workbench (Tree),
    /// returning the new mode.
    pub fn toggle_mode(&mut self) -> ProjectionKind {
        self.mode = match self.mode {
            ProjectionKind::Cartography => ProjectionKind::Tree,
            ProjectionKind::Tree => ProjectionKind::Cartography,
        };
        self.mode
    }

    /// Set the projection mode explicitly.
    pub fn set_mode(&mut self, mode: ProjectionKind) {
        self.mode = mode;
    }

    /// The open tiles' graph members, in placement order.
    pub fn open_tiles(&self) -> &[GraphMemberId] {
        &self.open
    }

    /// How many tiles are open.
    pub fn tile_count(&self) -> usize {
        self.open.len()
    }

    /// Whether `member` already has an open tile.
    pub fn has_tile(&self, member: GraphMemberId) -> bool {
        self.open.contains(&member)
    }

    /// Open a tile for `member` (appended last). Deduplicates: a member already
    /// open is left in place. Returns whether a new tile was added.
    pub fn open_tile(&mut self, member: GraphMemberId) -> bool {
        if self.open.contains(&member) {
            return false;
        }
        self.open.push(member);
        true
    }

    /// Close the tile showing `member`. Returns whether one was removed.
    pub fn close_tile(&mut self, member: GraphMemberId) -> bool {
        let before = self.open.len();
        self.open.retain(|m| *m != member);
        self.open.len() != before
    }

    /// The forme arrangement for the open tiles: one root-attached `TileIntent`
    /// per member, in order. This is the projection input; v1 has no stacking, so
    /// `project_tree` yields one split slot per tile.
    fn arrangement(&self) -> Arrangement {
        let mut arrangement = Arrangement::new();
        for member in &self.open {
            arrangement.add_tile_intent(Some(*member));
        }
        arrangement
    }

    /// Lay the open tiles out inside the content band `viewport` and resolve each
    /// to its node URL via `graph`. Band-relative rects (origin at the band's top
    /// left); the host offsets by the band origin when compositing. Empty when no
    /// tiles are open. A tile whose member has left the graph resolves to
    /// `url: None`.
    pub fn laid_out_tiles(
        &self,
        viewport: PortableSize,
        config: &LayoutConfig,
        graph: &Graph,
    ) -> Vec<WorkbenchTile> {
        let plan = project_tree(&self.arrangement());
        let laid = layout_plan(&plan, viewport, config);
        laid
            .slots
            .into_iter()
            .filter_map(|slot| match slot {
                LaidOutSlot::Tile { tile, rect } => {
                    // v1 tiles are always member-bound (see `arrangement`).
                    let member = tile.member?;
                    let url = graph.get_node_by_id(member).map(|(_, node)| node.url().to_string());
                    Some(WorkbenchTile { rect, member, url })
                },
                // No stacking in v1; a stack slot would need a tab strip + active
                // tab, which the render integration grows when the UX lands.
                LaidOutSlot::Tabs { .. } => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use kernel::geometry::PortablePoint;
    use uuid::Uuid;

    use super::*;

    fn band() -> PortableSize {
        PortableSize::new(1000.0, 600.0)
    }

    /// A graph with one node per `(uuid, url)`, positioned at the origin.
    fn graph_with(nodes: &[(Uuid, &str)]) -> Graph {
        let mut graph = Graph::new();
        for (id, url) in nodes {
            graph.add_node_with_id(*id, (*url).to_string(), PortablePoint::new(0.0, 0.0));
        }
        graph
    }

    #[test]
    fn new_workbench_is_cartography_and_empty() {
        let wb = Workbench::new();
        assert_eq!(wb.mode(), ProjectionKind::Cartography);
        assert!(!wb.is_tiled());
        assert_eq!(wb.tile_count(), 0);
    }

    #[test]
    fn toggle_mode_flips_between_orrery_and_tiled() {
        let mut wb = Workbench::new();
        assert_eq!(wb.toggle_mode(), ProjectionKind::Tree);
        assert!(wb.is_tiled());
        assert_eq!(wb.toggle_mode(), ProjectionKind::Cartography);
        assert!(!wb.is_tiled());
    }

    #[test]
    fn open_tile_appends_and_dedups() {
        let mut wb = Workbench::new();
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        assert!(wb.open_tile(a), "first open is new");
        assert!(wb.open_tile(b), "a distinct member is new");
        assert!(!wb.open_tile(a), "re-opening an open member is a no-op");
        assert_eq!(wb.open_tiles(), &[a, b], "placement order is insertion order");
        assert_eq!(wb.tile_count(), 2);
    }

    #[test]
    fn close_tile_removes_only_the_named_member() {
        let mut wb = Workbench::new();
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        wb.open_tile(a);
        wb.open_tile(b);
        assert!(wb.close_tile(a));
        assert!(!wb.close_tile(a), "closing an absent member reports false");
        assert_eq!(wb.open_tiles(), &[b]);
    }

    #[test]
    fn two_tiles_split_the_band_and_resolve_urls() {
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let graph = graph_with(&[(a, "https://a.example"), (b, "https://b.example")]);
        let mut wb = Workbench::new();
        wb.open_tile(a);
        wb.open_tile(b);

        let cfg = LayoutConfig { gap: 10.0, tab_strip_height: 28.0 };
        let tiles = wb.laid_out_tiles(PortableSize::new(810.0, 600.0), &cfg, &graph);

        assert_eq!(tiles.len(), 2, "one placed tile per open member");
        // (810 - 10 gap) / 2 = 400 each, full band height; second begins at 410.
        assert_eq!(tiles[0].member, a);
        assert_eq!(tiles[0].rect.origin.x, 0.0);
        assert_eq!(tiles[0].rect.size.width, 400.0);
        assert_eq!(tiles[0].rect.size.height, 600.0);
        assert_eq!(tiles[0].url.as_deref(), Some("https://a.example"));
        assert_eq!(tiles[1].member, b);
        assert_eq!(tiles[1].rect.origin.x, 410.0);
        assert_eq!(tiles[1].url.as_deref(), Some("https://b.example"));
    }

    #[test]
    fn an_unknown_member_resolves_to_no_url() {
        let a = Uuid::from_u128(1);
        let graph = graph_with(&[]); // member `a` is not in the graph
        let mut wb = Workbench::new();
        wb.open_tile(a);
        let tiles = wb.laid_out_tiles(band(), &LayoutConfig::default(), &graph);
        assert_eq!(tiles.len(), 1, "the tile is still placed");
        assert_eq!(tiles[0].member, a);
        assert_eq!(tiles[0].url, None, "a member gone from the graph has no URL");
    }

    #[test]
    fn no_open_tiles_lays_out_nothing() {
        let wb = Workbench::new();
        let graph = graph_with(&[]);
        assert!(wb.laid_out_tiles(band(), &LayoutConfig::default(), &graph).is_empty());
    }
}
