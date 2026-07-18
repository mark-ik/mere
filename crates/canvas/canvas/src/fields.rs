/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Field-region interaction for [`Canvas`](crate::Canvas): which field the cursor
//! is over (hover → box-on-interaction), hiding / showing fields (the roster's
//! visibility toggle, mirroring `hidden_edges`), and centering the camera on a
//! field. Factored out of `lib.rs` to stay under the workspace 600-LOC ceiling.

use std::collections::HashSet;

use euclid::default::Point2D;
use seiche::CouplingForce;
use kernel::graph::apply::{GraphDelta, GraphDeltaResult, apply_graph_delta};
use kernel::graph::{Falloff, Field, FieldDefinition, FieldExtent, FieldId, ScalarField};

use super::Canvas;

/// World-space reach of a field's corner resize handles. A press within this of a
/// `Region` corner resizes; the rest of the box moves. (Field regions — move/resize.)
const FIELD_HANDLE_REACH: f32 = 22.0;

/// Smallest a field box may be dragged to, so a resize can't collapse it. (px.)
const FIELD_MIN_SIZE: f32 = 28.0;

/// What a field drag grabbed: the body (move) or a `Region` corner (resize). For a
/// corner, `left`/`top` name which corner, so the opposite edges stay pinned.
#[derive(Clone, Copy)]
pub(crate) enum FieldHandle {
    Move,
    Resize { left: bool, top: bool },
}

/// An in-progress field move / resize drag. Captures the field's extent at press so
/// the drag is a pure function of the cursor delta (no accumulation drift).
#[derive(Clone, Copy)]
pub(crate) struct FieldDrag {
    id: FieldId,
    handle: FieldHandle,
    press_world: Point2D<f32>,
    /// The field's `[min_x, min_y, max_x, max_y]` at press.
    orig: [f32; 4],
}

impl Canvas {
    /// Re-resolve every field coupling against the current graph + node set and
    /// replace the live forces — the rebuild a field place / move / resize / new
    /// node triggers. A `CouplingForce` snapshots its targets + the field definition
    /// at build, so this is how a moved field's well re-aims and a node added after
    /// the field gets captured. Position-preserving (forces don't touch body state).
    /// (Field regions — rebuild-on-mutation.)
    pub(crate) fn rebuild_coupling_forces(&mut self) {
        let forces: Vec<CouplingForce> = self
            .graph
            .couplings()
            .filter_map(|c| {
                // A retired (deleted) field exerts no force, so its well drops when the
                // field is deleted — `from_coupling` resolves a field by id regardless of
                // lifecycle, so the active check is the canvas's job. (Field regions — delete.)
                if !self.graph.field(c.field).is_some_and(|f| f.is_active()) {
                    return None;
                }
                crate::build::coupling_force_from_graph(c, &self.graph)
            })
            .collect();
        self.physics.set_coupling_forces(forces);
    }

    /// The field whose `Region` extent contains the canvas-local screen point, if any — the
    /// host's right-click field hit-test for the "Delete field" menu. (Field regions — delete.)
    pub fn field_at_screen(&self, sx: f32, sy: f32) -> Option<FieldId> {
        self.field_at_world(self.screen_to_world((sx, sy)))
    }

    /// Delete a field: retire it (the kernel keeps the definition for history / federation)
    /// and rebuild the coupling forces so its well drops, clearing it from the hover + hidden
    /// sets and settling the nodes it no longer pulls. The host's "Delete field" context
    /// action. (Field regions — delete.)
    pub fn delete_field(&mut self, id: FieldId) {
        let retired = matches!(
            apply_graph_delta(&mut self.graph, GraphDelta::RetireField { id }),
            GraphDeltaResult::FieldChanged(true)
        );
        if retired {
            if self.active_field == Some(id) {
                self.active_field = None;
            }
            self.hidden_fields.remove(&id);
            self.rebuild_coupling_forces();
            self.physics.settle(crate::SETTLE_TICKS / 3);
        }
    }

    /// The active (hovered) field whose extent box should draw, if any. Read by the
    /// field paint pass for box-on-interaction.
    pub(crate) fn active_field(&self) -> Option<FieldId> {
        self.active_field
    }

    /// The set of hidden field ids — the field paint pass skips these.
    pub(crate) fn hidden_field_ids(&self) -> &HashSet<FieldId> {
        &self.hidden_fields
    }

    /// The active field whose `Region` extent contains world point `world`, when they
    /// overlap the one painted last (so hover and paint agree). `graph.fields()`
    /// iterates a `HashMap`, so "last" is the paint order, not placement order — a
    /// real z-order for fields is a follow-up; overlap is rare in practice. Hidden /
    /// retired fields are not pickable.
    fn field_at_world(&self, world: Point2D<f32>) -> Option<FieldId> {
        let mut hit = None;
        for field in self.graph.fields() {
            if !field.is_active() || self.hidden_fields.contains(&field.id) {
                continue;
            }
            let FieldExtent::Region {
                min_x,
                min_y,
                max_x,
                max_y,
            } = field.extent
            else {
                continue;
            };
            if world.x >= min_x && world.x <= max_x && world.y >= min_y && world.y <= max_y {
                hit = Some(field.id); // last wins → matches the paint pass's last-drawn
            }
        }
        hit
    }

    /// Update the hovered field from a screen-space cursor point — the box-on-
    /// interaction driver (a field's dashed extent box shows only while hovered,
    /// the soft disk well always). Returns whether the active field changed (so the
    /// host redraws).
    pub(crate) fn update_active_field(&mut self, screen_xy: (f32, f32)) -> bool {
        // Off the canvas viewport (the cursor moved onto an adjacent pane / the
        // toolbar) → no field is hovered, so the box doesn't stay stuck on.
        let off = screen_xy.0 < 0.0
            || screen_xy.0 > self.view_w as f32
            || screen_xy.1 < 0.0
            || screen_xy.1 > self.view_h as f32;
        let next = if off {
            None
        } else {
            self.field_at_world(self.screen_to_world(screen_xy))
        };
        let changed = next != self.active_field;
        self.active_field = next;
        changed
    }

    /// Whether field `id` is visible on the canvas (not hidden).
    pub fn field_visible(&self, id: FieldId) -> bool {
        !self.hidden_fields.contains(&id)
    }

    /// Toggle field `id`'s canvas visibility (display-only — the field and its
    /// coupling persist, mirroring `hidden_edges`). Returns the new visible state.
    /// The roster's hide/show control.
    pub fn toggle_field_visible(&mut self, id: FieldId) -> bool {
        if self.hidden_fields.remove(&id) {
            true
        } else {
            self.hidden_fields.insert(id);
            if self.active_field == Some(id) {
                self.active_field = None;
            }
            false
        }
    }

    /// Center the camera on field `id`'s extent at the current zoom (the roster's
    /// click-to-locate), un-hiding it so a located field is actually shown. Returns
    /// whether the field was found.
    pub fn center_on_field(&mut self, id: FieldId) -> bool {
        let Some(field) = self.graph.field(id) else {
            return false;
        };
        let FieldExtent::Region {
            min_x,
            min_y,
            max_x,
            max_y,
        } = field.extent
        else {
            return false;
        };
        let (cx, cy) = ((min_x + max_x) / 2.0, (min_y + max_y) / 2.0);
        // screen = world * zoom + offset; put the field's center at the viewport center.
        self.camera.offset.0 = self.view_w as f32 / 2.0 - cx * self.camera.zoom;
        self.camera.offset.1 = self.view_h as f32 / 2.0 - cy * self.camera.zoom;
        self.hidden_fields.remove(&id);
        true
    }

    /// Clear the hovered field (no field active) — the host calls this when the
    /// cursor leaves the canvas, so a hover box does not stay stuck on after the
    /// pointer moves onto another pane. Returns whether it changed. (Field regions.)
    pub fn clear_active_field(&mut self) -> bool {
        let changed = self.active_field.is_some();
        self.active_field = None;
        changed
    }

    /// The field + handle (body → move, corner → resize) under world point `world`,
    /// if any — the topmost (paint-order-consistent) non-hidden field. (Field regions.)
    fn field_handle_at(&self, world: Point2D<f32>) -> Option<(FieldId, FieldHandle)> {
        let mut hit = None;
        for field in self.graph.fields() {
            if !field.is_active() || self.hidden_fields.contains(&field.id) {
                continue;
            }
            let FieldExtent::Region {
                min_x,
                min_y,
                max_x,
                max_y,
            } = field.extent
            else {
                continue;
            };
            if world.x < min_x || world.x > max_x || world.y < min_y || world.y > max_y {
                continue;
            }
            let near_left = world.x - min_x <= FIELD_HANDLE_REACH;
            let near_right = max_x - world.x <= FIELD_HANDLE_REACH;
            let near_top = world.y - min_y <= FIELD_HANDLE_REACH;
            let near_bottom = max_y - world.y <= FIELD_HANDLE_REACH;
            // The box border is the grab handle (corner = resize, edge = move); the
            // deep interior passes through to nodes / marquee, so you can still select
            // the nodes a field sits over. The box is exactly the visible affordance.
            if !(near_left || near_right || near_top || near_bottom) {
                continue;
            }
            let handle = if (near_left || near_right) && (near_top || near_bottom) {
                FieldHandle::Resize {
                    left: near_left,
                    top: near_top,
                }
            } else {
                FieldHandle::Move
            };
            hit = Some((field.id, handle)); // last wins → paint-order-consistent topmost
        }
        hit
    }

    /// Begin a field move / resize drag if a field handle is under world point
    /// `world` (a left press on empty canvas that lands on a field). Returns whether
    /// a drag started. (Field regions — move/resize.)
    pub(crate) fn begin_field_drag(&mut self, world: Point2D<f32>) -> bool {
        let Some((id, handle)) = self.field_handle_at(world) else {
            return false;
        };
        let Some(field) = self.graph.field(id) else {
            return false;
        };
        let FieldExtent::Region {
            min_x,
            min_y,
            max_x,
            max_y,
        } = field.extent
        else {
            return false;
        };
        self.active_field = Some(id);
        self.field_drag = Some(FieldDrag {
            id,
            handle,
            press_world: world,
            orig: [min_x, min_y, max_x, max_y],
        });
        true
    }

    /// Whether a field move / resize drag is in progress (the host saves the session
    /// on its release, since the field's geometry is graph truth).
    pub fn dragging_field(&self) -> bool {
        self.field_drag.is_some()
    }

    /// Apply the in-progress field drag to the cursor at `screen_xy`: translate (move)
    /// or move the grabbed corner (resize), reshape the field, and re-aim its well.
    /// (Field regions — move/resize.)
    pub(crate) fn drag_field_to(&mut self, screen_xy: (f32, f32)) {
        let Some(fd) = self.field_drag else {
            return;
        };
        let world = self.screen_to_world(screen_xy);
        let dx = world.x - fd.press_world.x;
        let dy = world.y - fd.press_world.y;
        let [omin_x, omin_y, omax_x, omax_y] = fd.orig;
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (omin_x, omin_y, omax_x, omax_y);
        match fd.handle {
            FieldHandle::Move => {
                min_x += dx;
                max_x += dx;
                min_y += dy;
                max_y += dy;
            }
            FieldHandle::Resize { left, top } => {
                if left {
                    min_x = omin_x + dx;
                } else {
                    max_x = omax_x + dx;
                }
                if top {
                    min_y = omin_y + dy;
                } else {
                    max_y = omax_y + dy;
                }
                // A corner dragged past the opposite edge flips; floor the size so the
                // field can't collapse to nothing.
                if min_x > max_x {
                    std::mem::swap(&mut min_x, &mut max_x);
                }
                if min_y > max_y {
                    std::mem::swap(&mut min_y, &mut max_y);
                }
                if max_x - min_x < FIELD_MIN_SIZE {
                    max_x = min_x + FIELD_MIN_SIZE;
                }
                if max_y - min_y < FIELD_MIN_SIZE {
                    max_y = min_y + FIELD_MIN_SIZE;
                }
            }
        }
        self.reshape_field(fd.id, min_x, min_y, max_x, max_y);
    }

    /// End the field drag, if any. Returns whether one was in progress (the host then
    /// settles + persists). (Field regions — move/resize.)
    pub(crate) fn end_field_drag(&mut self) -> bool {
        self.field_drag.take().is_some()
    }

    /// Replace field `id` (same id) with a new `Region` box + a `Disk` re-centered and
    /// re-sized to fit, preserving the field's name + falloff, then re-aim its coupling
    /// well. (Field regions — move/resize.)
    fn reshape_field(&mut self, id: FieldId, min_x: f32, min_y: f32, max_x: f32, max_y: f32) {
        let Some(old) = self.graph.field(id) else {
            return;
        };
        let name = old.name.clone();
        let falloff = match &old.definition {
            FieldDefinition::Scalar(ScalarField::Disk { falloff, .. }) => *falloff,
            _ => Falloff::Smoothstep,
        };
        let (cx, cy) = ((min_x + max_x) / 2.0, (min_y + max_y) / 2.0);
        let radius = ((max_x - min_x).min(max_y - min_y) / 2.0).max(1.0);
        let definition = FieldDefinition::Scalar(ScalarField::disk_at(cx, cy, radius, falloff));
        let extent = FieldExtent::Region {
            min_x,
            min_y,
            max_x,
            max_y,
        };
        let mut field = Field::new(id, definition).with_extent(extent);
        if let Some(name) = name {
            field = field.with_name(name);
        }
        let _ = apply_graph_delta(&mut self.graph, GraphDelta::AddField { field });
        self.rebuild_coupling_forces();
    }
}
