/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Shared settings control builders and their small view-model structs.

use crate::list_pane::PaneItem;

/// One theme option in the Appearance page: its id (the hit-test key), display
/// name, and whether it is the active theme.
pub(crate) struct ThemeOption {
    pub id: String,
    pub name: String,
    pub active: bool,
}

/// One engine in the Engines page: its id (carried in the toggle key), display
/// name, and whether it is active (not deactivated) this session.
pub(crate) struct EngineRow {
    pub id: String,
    pub name: String,
    pub active: bool,
}

/// The Appearance page's theme controls: one clickable button per option, its
/// theme id the activation key the host drains to switch the theme.
pub(crate) fn theme_section_items(themes: &[ThemeOption]) -> Vec<PaneItem> {
    themes
        .iter()
        .map(|theme| PaneItem::radio(theme.active, theme.name.clone(), theme.id.clone()))
        .collect()
}

/// The Engines page's controls: one toggle per present engine, the id riding
/// the key as `engine:toggle:<id>`.
pub(crate) fn engine_section_items(engines: &[EngineRow]) -> Vec<PaneItem> {
    engines
        .iter()
        .map(|engine| {
            let label = format!(
                "{}  —  {}",
                engine.name,
                if engine.active { "active" } else { "off" }
            );
            PaneItem::switch(engine.active, label, format!("engine:toggle:{}", engine.id))
        })
        .collect()
}

/// The Physics page's controls: the node-damping readout plus the − / + step
/// buttons (`phys:damping:down` / `:up`).
pub(crate) fn physics_section_items(physics_damping: f32) -> Vec<PaneItem> {
    vec![
        PaneItem::text(
            "app-row",
            format!("Node damping (inertia): {physics_damping:.1}"),
        ),
        PaneItem::button(
            "app-btn",
            "− less damping (more drift)".to_string(),
            "phys:damping:down".to_string(),
        ),
        PaneItem::button(
            "app-btn",
            "+ more damping (settle sooner)".to_string(),
            "phys:damping:up".to_string(),
        ),
    ]
}
