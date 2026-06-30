/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Camera / viewport / orbit control and scope (curated-subset) lensing.

use super::*;

impl Orrery {
    /// The session graph, for the host to persist (`to_snapshot` → `graph.json`).
    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    /// The current camera (pan + zoom), for the host to persist as view-intent.
    pub fn camera(&self) -> CameraView {
        CameraView {
            offset: self.camera.offset,
            zoom: self.camera.zoom,
        }
    }

    /// Restore the camera from persisted view-intent. A non-finite or
    /// non-positive zoom falls back to `1.0`; the zoom is clamped to the orrery's
    /// range. The host suppresses its own first-frame recenter when it restores a
    /// camera, so this value is not immediately overwritten.
    pub fn set_camera(&mut self, view: CameraView) {
        self.camera.zoom = if view.zoom.is_finite() && view.zoom > 0.0 {
            view.zoom.clamp(MIN_ZOOM, MAX_ZOOM)
        } else {
            1.0
        };
        self.camera.offset = view.offset;
    }

    /// The full per-pane [`Viewport`] (camera + pan inertia), for the host to stash on
    /// the owning (window, graph) pane and reinstall before the next render / input
    /// pass via [`set_viewport`](Self::set_viewport). Carries yaw / tilt too (unlike
    /// [`camera`](Self::camera)), so an isometric pane round-trips losslessly. This is
    /// the seam that moves the camera off the shared authority onto the view: the
    /// orrery's own `camera` / `pan_velocity` become per-pass working state the host
    /// drives, so two windows on one graph hold distinct viewports, not a mirror.
    pub fn viewport(&self) -> Viewport {
        Viewport {
            offset: self.camera.offset,
            zoom: self.camera.zoom,
            yaw: self.camera.yaw,
            tilt: self.camera.tilt,
            pan_velocity: self.pan_velocity,
        }
    }

    /// Install a per-pane [`Viewport`] before rendering or handling input for the pane
    /// that owns it. Clamps as [`set_camera`](Self::set_camera) (zoom) and
    /// [`set_tilt`](Self::set_tilt) (tilt) do, so a host-stashed value is sanitized on
    /// the way back in.
    pub fn set_viewport(&mut self, v: Viewport) {
        self.camera.offset = v.offset;
        self.camera.zoom = if v.zoom.is_finite() && v.zoom > 0.0 {
            v.zoom.clamp(MIN_ZOOM, MAX_ZOOM)
        } else {
            1.0
        };
        self.camera.yaw = v.yaw;
        self.camera.tilt = v.tilt.clamp(0.05, 1.0);
        self.pan_velocity = v.pan_velocity;
    }

    /// Toggle the isometric (foreshortened-ground) projection. `on` reclines the
    /// ground by foreshortening the vertical, so the graph reads as a tilted floor
    /// while the node cards stay upright billboards; `off` restores the plain
    /// top-down view. The free-cam yaw orbit + persistence are P2. (Isometric camera P1.)
    pub fn set_isometric(&mut self, on: bool) {
        /// Vertical foreshorten for the isometric preset (a dimetric squash); becomes
        /// a setting once the projection picker lands (P2).
        const ISO_TILT: f32 = 0.55;
        self.camera.tilt = if on { ISO_TILT } else { 1.0 };
    }

    /// Whether the isometric projection is active (the ground is foreshortened).
    pub fn is_isometric(&self) -> bool {
        self.camera.tilt < 1.0
    }

    /// Orbit the view by `d_radians` about the vertical (the free-cam yaw). The
    /// ground rotates while the node cards stay upright billboards; pair with the
    /// isometric `tilt` for the 2.5D orbit (at `tilt == 1` it spins the flat layout).
    /// The host's orbit gesture / projection picker drives this. (Isometric camera P2.)
    pub fn orbit_by(&mut self, d_radians: f32) {
        self.camera.yaw += d_radians;
    }

    /// Set the orbit yaw (radians) directly. (Isometric camera P2.)
    pub fn set_yaw(&mut self, yaw: f32) {
        self.camera.yaw = yaw;
    }

    /// Set the vertical foreshorten directly, clamped to a sane `(0, 1]`. `1.0` is
    /// top-down; lower reclines the ground further. (Isometric camera P2.)
    pub fn set_tilt(&mut self, tilt: f32) {
        self.camera.tilt = tilt.clamp(0.05, 1.0);
    }

    /// The current orbit yaw (radians), for a host control to display / persist.
    pub fn yaw(&self) -> f32 {
        self.camera.yaw
    }

    /// The current vertical foreshorten (`tilt`), for a host control to display / persist.
    pub fn tilt(&self) -> f32 {
        self.camera.tilt
    }

    /// Whether a scope lens is active (the orrery is showing a curated subset, not the
    /// whole graph). The host offers "Show all" when this is true. (Curated orrery.)
    pub fn is_scoped(&self) -> bool {
        self.scope.is_some()
    }

    /// Focus the orrery on the current selection: scope it to the selected nodes plus
    /// their immediate (undirected) neighbors, so the selection shows as its own
    /// neighborhood projected through a curated arrangement. A no-op with no
    /// selection. (Curated orrery.)
    pub fn isolate_selection(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        let mut scope: HashSet<NodeKey> = self.selected.clone();
        for &key in &self.selected {
            scope.extend(self.graph.neighbors_undirected(key));
        }
        self.scope = Some(scope.into_iter().collect());
    }

    /// Scope the orrery to a host-supplied member set (by UUID), e.g. the workbench's
    /// open tiles, so the *same* arrangement renders as both a tiled workbench and a
    /// spatial map (the spine's "two projections of one arrangement"). Members absent
    /// from the graph are skipped; an empty set clears the lens (shows the whole
    /// graph). (Curated orrery — workbench mirror.)
    pub fn scope_to_members(&mut self, members: impl IntoIterator<Item = uuid::Uuid>) {
        let keys: Vec<NodeKey> = members
            .into_iter()
            .filter_map(|id| self.graph.get_node_by_id(id).map(|(key, _)| key))
            .collect();
        self.scope = (!keys.is_empty()).then_some(keys);
    }

    /// Drop the scope lens — show the whole graph again. (Curated orrery.)
    pub fn clear_scope(&mut self) {
        self.scope = None;
    }

    /// The current scope lens as member uuids, or `None` when unscoped. The inverse of
    /// [`scope_to_members`](Self::scope_to_members) — a host saves this before a transient
    /// per-window scope override (a branch window scoping to its graphlet) and restores it
    /// after. (Per-window branch scope.)
    pub fn scope_members(&self) -> Option<Vec<uuid::Uuid>> {
        self.scope.as_ref().map(|keys| {
            keys.iter()
                .filter_map(|&k| self.graph.get_node(k).map(|n| n.id))
                .collect()
        })
    }

    /// Whether `key` is within the active scope lens (always true when unscoped). The
    /// host's node-card builder filters on this so a scoped orrery (a branch window)
    /// shows only its scoped members, matching the `frame()` scene's own scope filter.
    /// (Curated orrery — host card path.)
    pub fn node_in_scope(&self, key: NodeKey) -> bool {
        self.scope.as_ref().is_none_or(|s| s.contains(&key))
    }

    /// Zoom by `factor`, keeping the world point under `anchor` (screen px) fixed.
    pub(crate) fn zoom_at(&mut self, anchor: (f32, f32), factor: f32) {
        // Keep the world point currently under `anchor` fixed across the zoom: read it
        // before, then shift `offset` so it lands back under `anchor` after. Correct for
        // any projection (top-down or isometric), and identical to the old
        // `world*zoom+offset` formula at the default camera. (Isometric camera P0.)
        let world_under = self.camera.to_world(anchor);
        self.camera.zoom = (self.camera.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let landed = self.camera.to_screen(world_under);
        self.camera.offset.0 += anchor.0 - landed.0;
        self.camera.offset.1 += anchor.1 - landed.1;
    }

    /// Map a screen-px point back to world space through the camera projector
    /// (the inverse of `Camera::to_screen`; at the default camera this is
    /// `world = (screen - offset) / zoom`).
    pub(crate) fn screen_to_world(&self, screen: (f32, f32)) -> Point2D<f32> {
        let w = self.camera.to_world(screen);
        Point2D::new(w.x, w.y)
    }

    /// The screen viewport mapped to world space — the region gyre culls against
    /// to decide which nodes are on screen.
    pub(crate) fn world_viewport(&self) -> Box2D<f32> {
        // Bound all four screen corners in world space: under an isometric yaw the
        // screen rectangle maps to a rotated world quad, so two corners under-cover.
        // At the default top-down camera this is the same box as before. (Isometric P0.)
        let (w, h) = (self.view_w as f32, self.view_h as f32);
        let corners = [
            self.screen_to_world((0.0, 0.0)),
            self.screen_to_world((w, 0.0)),
            self.screen_to_world((0.0, h)),
            self.screen_to_world((w, h)),
        ];
        let mut min = corners[0];
        let mut max = corners[0];
        for p in &corners[1..] {
            min.x = min.x.min(p.x);
            min.y = min.y.min(p.y);
            max.x = max.x.max(p.x);
            max.y = max.y.max(p.y);
        }
        Box2D::new(min, max)
    }
}
