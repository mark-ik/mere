// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Read-only source-time selection for a [`Canvas`](crate::Canvas).
//!
//! A source cursor chooses a separate historical canvas. The live canvas remains
//! owned and untouched underneath it, so returning to live restores the exact
//! current presentation instead of replaying or mutating graph truth. Hosts own
//! their source labels, scrubber controls, and subscriptions; this type only
//! binds the source snapshot to the Canvas presentation root.

use kernel::graph::{Graph, SourceTime};
use netrender::Scene;

use crate::Canvas;

/// Which source snapshot a [`SourceTimeCanvas`] currently presents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceTimeSelection<Cursor> {
    /// The host's current, mutable canvas.
    Live,
    /// A read-only historical source cursor.
    Historical(Cursor),
}

/// A Canvas with an independently preserved live presentation and an optional
/// historical source-time preview.
///
/// `Canvas` is deliberately not given mutable access while a historical cursor
/// is visible. The small render-facing API below is enough for a host to paint
/// and resize that preview; graph-edit commands stay routed to `live_canvas_mut`
/// after [`return_to_live`](Self::return_to_live). A historical graph is a
/// disposable snapshot, never a branch or a replacement for source truth.
pub struct SourceTimeCanvas<Cursor> {
    live: Canvas,
    historical: Option<(Cursor, Canvas)>,
}

impl<Cursor> SourceTimeCanvas<Cursor>
where
    Cursor: Clone + Eq,
{
    /// Bind an already-hosted live canvas. The host is responsible for keeping
    /// it current as its graph authority receives edits.
    pub fn new(live: Canvas) -> Self {
        Self {
            live,
            historical: None,
        }
    }

    /// Construct a live Canvas from a source's current immutable snapshot.
    pub fn from_source<Source>(source: &Source) -> Option<Self>
    where
        Source: SourceTime<Cursor = Cursor, Snapshot = Graph>,
    {
        let current = source.source_extent().current;
        source
            .source_snapshot(&current)
            .map(Canvas::with_graph)
            .map(Self::new)
    }

    /// The source selection currently painted by [`frame`](Self::frame).
    pub fn selection(&self) -> SourceTimeSelection<Cursor> {
        self.historical
            .as_ref()
            .map(|(cursor, _)| SourceTimeSelection::Historical(cursor.clone()))
            .unwrap_or(SourceTimeSelection::Live)
    }

    /// Whether a historical snapshot rather than the live canvas is visible.
    pub fn is_historical(&self) -> bool {
        self.historical.is_some()
    }

    /// The Canvas currently visible to read-only host presentation code.
    pub fn canvas(&self) -> &Canvas {
        self.historical
            .as_ref()
            .map(|(_, canvas)| canvas)
            .unwrap_or(&self.live)
    }

    /// The source-of-truth Canvas, even while a historical snapshot is visible.
    pub fn live_canvas(&self) -> &Canvas {
        &self.live
    }

    /// Mutable graph access is intentionally explicit and always targets live
    /// truth. Callers must return to live before dispatching graph-edit commands.
    pub fn live_canvas_mut(&mut self) -> &mut Canvas {
        &mut self.live
    }

    /// Select `cursor` from `source` for presentation.
    ///
    /// Selecting the source's current cursor returns to the retained live
    /// Canvas. Unknown cursors are refused without changing the selection.
    pub fn select<Source>(&mut self, source: &Source, cursor: Cursor) -> bool
    where
        Source: SourceTime<Cursor = Cursor, Snapshot = Graph>,
    {
        if cursor == source.source_extent().current {
            self.return_to_live();
            return true;
        }
        let Some(snapshot) = source.source_snapshot(&cursor) else {
            return false;
        };
        self.historical = Some((cursor, self.snapshot_canvas(snapshot)));
        true
    }

    /// Drop the disposable historical snapshot and reveal the retained live
    /// Canvas unchanged.
    pub fn return_to_live(&mut self) {
        self.historical = None;
    }

    /// Refresh the live Canvas from a source's current snapshot, retaining the
    /// view's camera, layout choice, member-keyed positions, and selection where
    /// the current graph still contains those members. This never writes back to
    /// `source`. A host calls it after its authority has accepted a live edit.
    pub fn refresh_live<Source>(&mut self, source: &Source) -> bool
    where
        Source: SourceTime<Cursor = Cursor, Snapshot = Graph>,
    {
        let current = source.source_extent().current;
        let Some(snapshot) = source.source_snapshot(&current) else {
            return false;
        };
        self.live = self.snapshot_canvas(snapshot);
        if self
            .historical
            .as_ref()
            .is_some_and(|(cursor, _)| *cursor == current)
        {
            self.return_to_live();
        }
        true
    }

    /// Advance one presentation frame for the selected source snapshot.
    pub fn frame(&mut self, width: u32, height: u32) -> (Scene, bool) {
        match self.historical.as_mut() {
            Some((_, canvas)) => canvas.frame(width, height),
            None => self.live.frame(width, height),
        }
    }

    /// Resize both retained presentations. The historical canvas is only a
    /// disposable read model, but the live canvas must also learn the new
    /// viewport while it is hidden so returning to live does not restore a
    /// stale camera extent.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.live.resize(width, height);
        if let Some((_, historical)) = self.historical.as_mut() {
            historical.resize(width, height);
        }
    }

    fn snapshot_canvas(&self, graph: Graph) -> Canvas {
        let geometry = self.live.cartography_geometry();
        let viewport = self.live.viewport();
        let selected = self.live.selected_members();
        let scope = self.live.scope_members();
        let strategy = self.live.layout_strategy().map(str::to_owned);
        let score = self.live.projection_score().cloned();
        let arrangement_pull = self.live.arrangement_pull();

        let mut snapshot = Canvas::with_graph(graph);
        snapshot.set_viewport(viewport);
        snapshot.seed_cartography(geometry.iter());
        snapshot.apply_cartography_importance_metric(geometry.importance_metric());
        snapshot.apply_cartography_sizing(
            geometry.size_iter(),
            geometry.size_by_degree(),
            geometry.size_by_importance(),
        );
        snapshot.apply_cartography_sprites(geometry.sprite_iter());
        snapshot.apply_cartography_sprite_hulls(geometry.sprite_hull_iter());
        snapshot.apply_cartography_materials(geometry.material_iter());
        snapshot.apply_cartography_faces(geometry.face_iter());
        snapshot.set_selected_members(&selected);
        if let Some(scope) = scope {
            snapshot.scope_to_members(scope);
        }

        if let Some(strategy) = strategy {
            let positions = geometry
                .iter()
                .filter_map(|(id, (x, y))| {
                    snapshot
                        .graph()
                        .get_node_key_by_id(id)
                        .map(|key| (key, kernel::geometry::PortablePoint::new(x, y)))
                })
                .collect::<Vec<_>>();
            snapshot.set_layout_strategy(Some(strategy));
            snapshot.apply_strategy_positions(&positions);
            snapshot.set_projection_score(score);
            snapshot.set_arrangement_pull(arrangement_pull);
        }
        snapshot
    }
}
