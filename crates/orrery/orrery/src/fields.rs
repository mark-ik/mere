/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Field-region interaction for [`Orrery`](crate::Orrery): which field the cursor
//! is over (hover → box-on-interaction), hiding / showing fields (the roster's
//! visibility toggle, mirroring `hidden_edges`), and centering the camera on a
//! field. Factored out of `lib.rs` to stay under the workspace 600-LOC ceiling.

use std::collections::HashSet;

use euclid::default::Point2D;
use kernel::graph::{FieldExtent, FieldId};

use super::Orrery;

impl Orrery {
    /// The active (hovered) field whose extent box should draw, if any. Read by the
    /// field paint pass for box-on-interaction.
    pub(crate) fn active_field(&self) -> Option<FieldId> {
        self.active_field
    }

    /// The set of hidden field ids — the field paint pass skips these.
    pub(crate) fn hidden_field_ids(&self) -> &HashSet<FieldId> {
        &self.hidden_fields
    }

    /// The active field whose `Region` extent contains world point `world` — the
    /// topmost (last-placed) when they overlap. Hidden / retired fields are not
    /// pickable.
    fn field_at_world(&self, world: Point2D<f32>) -> Option<FieldId> {
        let mut hit = None;
        for field in self.graph.fields() {
            if !field.is_active() || self.hidden_fields.contains(&field.id) {
                continue;
            }
            let FieldExtent::Region { min_x, min_y, max_x, max_y } = field.extent else {
                continue;
            };
            if world.x >= min_x && world.x <= max_x && world.y >= min_y && world.y <= max_y {
                hit = Some(field.id); // last wins → the topmost (last-painted) field
            }
        }
        hit
    }

    /// Update the hovered field from a screen-space cursor point — the box-on-
    /// interaction driver (a field's dashed extent box shows only while hovered,
    /// the soft disk well always). Returns whether the active field changed (so the
    /// host redraws).
    pub(crate) fn update_active_field(&mut self, screen_xy: (f32, f32)) -> bool {
        let world = self.screen_to_world(screen_xy);
        let next = self.field_at_world(world);
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
        let FieldExtent::Region { min_x, min_y, max_x, max_y } = field.extent else {
            return false;
        };
        let (cx, cy) = ((min_x + max_x) / 2.0, (min_y + max_y) / 2.0);
        // screen = world * zoom + offset; put the field's center at the viewport center.
        self.camera.offset.0 = self.view_w as f32 / 2.0 - cx * self.camera.zoom;
        self.camera.offset.1 = self.view_h as f32 / 2.0 - cy * self.camera.zoom;
        self.hidden_fields.remove(&id);
        true
    }
}
