/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Background ticks attached to the `HostRoot` entity.
//!
//! - **apparatus auto-refresh** — slow tick to surface new tracing
//!   events without user interaction.
//! - **orrery inertia tick** — ~60 Hz pan-momentum decay + smooth
//!   zoom interpolation toward `orrery_zoom_target`.
//!
//! Both self-terminate when the entity drops, and both skip
//! `cx.notify()` when nothing changed so an idle window doesn't burn
//! frames.

use euclid::default::Vector2D;
use gpui::Context;

use crate::a11y::A11yAdapter;
use crate::HostRoot;

/// Spawn a background task that ticks every ~500ms and calls
/// `cx.notify()` on `HostRoot`, so the apparatus pane re-renders and
/// the live tracing event buffer surfaces new entries without manual
/// interaction.
pub fn install_apparatus_auto_refresh(cx: &mut Context<HostRoot>) {
    cx.spawn(async move |this, cx| {
        // The async closure receives `cx: &mut AsyncApp`. AsyncApp is
        // Clone, so we clone once into an owned local and reborrow
        // `&mut` from it each loop iteration.
        let mut cx = cx.clone();
        let executor = cx.background_executor().clone();
        loop {
            executor.timer(std::time::Duration::from_millis(500)).await;
            if this.update(&mut cx, |_, cx| cx.notify()).is_err() {
                break;
            }
        }
    })
    .detach();
}

/// Spawn a background task that owns the platform a11y adapter and
/// pushes uxtree updates to the OS. Runs at ~5 Hz — plenty for
/// screen-reader announcement freshness, and intentionally **out of
/// gpui's render path**: accesskit's `Adapter::new` and
/// `update_if_active` synchronously pump platform messages, which
/// re-enters the wndproc and panics on `RefCell::borrow_mut` if
/// gpui's `App` is borrowed at that moment.
///
/// The pattern: brief `entity.update(...)` calls fetch data (handle,
/// tree); the actual accesskit invocations happen between those
/// calls, when no `App` borrow is live.
pub fn install_a11y_push(cx: &mut Context<HostRoot>) {
    cx.spawn(async move |this, cx| {
        let mut cx = cx.clone();
        let executor = cx.background_executor().clone();
        let mut adapter: Option<A11yAdapter> = None;
        loop {
            executor
                .timer(std::time::Duration::from_millis(200))
                .await;

            if adapter.is_none() {
                // Try to lazy-construct. We need both the raw
                // handle (captured during render) and an initial
                // tree. Pull both out in a single brief update.
                let snapshot = this
                    .update(&mut cx, |this, _cx| {
                        this.a11y_handle
                            .map(|handle| (handle, this.app_tree.to_tree_update(None)))
                    })
                    .ok()
                    .flatten();
                if let Some((handle, tree)) = snapshot {
                    // Out-of-borrow construction — safe to re-enter
                    // the wndproc.
                    adapter = Some(A11yAdapter::from_raw(handle, tree));
                }
            } else {
                // Push the latest tree. Same pattern: pull, push.
                let tree_result =
                    this.update(&mut cx, |this, _cx| this.app_tree.to_tree_update(None));
                let stop = tree_result.is_err();
                if let Ok(tree) = tree_result {
                    if let Some(adapter) = adapter.as_mut() {
                        adapter.update(tree);
                    }
                }
                if stop {
                    break;
                }
            }
        }
    })
    .detach();
}

/// Spawn a background task that ticks ~60Hz to:
///  1. Decay the orrery camera's pan velocity (flick-pan momentum).
///  2. Lerp `camera.zoom` toward `orrery_zoom_target` (smooth scroll-zoom).
pub fn install_orrery_inertia_tick(cx: &mut Context<HostRoot>) {
    use graph_canvas::camera::{DEFAULT_PAN_DAMPING_PER_SECOND, PAN_VELOCITY_EPSILON};
    const TICK_HZ: f64 = 60.0;
    const ZOOM_LERP: f32 = 0.2;
    const ZOOM_SNAP_THRESHOLD: f32 = 0.001;
    let dt = (1.0 / TICK_HZ) as f32;
    let frame = std::time::Duration::from_secs_f64(1.0 / TICK_HZ);

    cx.spawn(async move |this, cx| {
        let mut cx = cx.clone();
        let executor = cx.background_executor().clone();
        loop {
            executor.timer(frame).await;
            let stop = this
                .update(&mut cx, |this, cx| {
                    // Tick every orrery pane in this window.
                    // Multi-orrery (Phase 1B Part 2): each gets
                    // its own inertia decay + zoom lerp.
                    let mut changed = false;
                    for state in this.panes.values_mut() {
                        let crate::pane_state::PaneState::Orrery(orrery) = state else {
                            continue;
                        };
                        let had_velocity =
                            orrery.camera.pan_velocity.length() >= PAN_VELOCITY_EPSILON;
                        if had_velocity {
                            let delta = orrery
                                .camera
                                .tick_inertia(dt, DEFAULT_PAN_DAMPING_PER_SECOND);
                            if delta != Vector2D::zero() {
                                changed = true;
                            }
                        }
                        let zoom_diff = orrery.zoom_target - orrery.camera.zoom;
                        if zoom_diff.abs() > ZOOM_SNAP_THRESHOLD {
                            orrery.camera.zoom += zoom_diff * ZOOM_LERP;
                            changed = true;
                        } else if (orrery.camera.zoom - orrery.zoom_target).abs() > 0.0 {
                            orrery.camera.zoom = orrery.zoom_target;
                            changed = true;
                        }
                    }
                    if changed {
                        cx.notify();
                    }
                    false
                })
                .is_err();
            if stop {
                break;
            }
        }
    })
    .detach();
}
