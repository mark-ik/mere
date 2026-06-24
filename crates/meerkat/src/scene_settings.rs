/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The orrery **Scene** settings page (`pelt/scene`): load a physics backdrop scene (a drop-bowl,
//! pyramid, dominoes, Galton board, funnel, drift, chain), a whirlpool or fountain effect, or a
//! liquid pool; clear it; and set whether the graph is tangible to the scene. The page's controls
//! (the shared section builder) and the host-side drain of their activation keys (`scene:*`) live
//! here, the same builder + host-drain split the other `pelt/*` pages use (so the page is one source
//! of truth). The orrery owns the physics; this routes each key to the matching `Orrery` method.
//! Split into its own file so neither `input.rs` nor `settings_lane.rs` grows past the 600-LOC
//! ceiling. (Physics scenes — scene settings.)

use crate::list_pane::PaneItem;

use super::WindowCtx;

/// The backdrop scenes shown as load buttons, each id riding `scene:load:<id>`. These map to the
/// re-exported `SceneSpec` catalog constructors via [`WindowCtx::apply_scene_key`]; the whirlpool
/// and fountain (which also bind a force-field / emitter) are separate buttons below. (Physics
/// scenes — scene settings.)
const SCENE_CATALOG: &[(&str, &str)] = &[
    ("dropbowl", "Drop bowl"),
    ("pyramid", "Pyramid"),
    ("domino", "Dominoes"),
    ("galton", "Galton board"),
    ("funnel", "Funnel"),
    ("drift", "Drift (perpetual)"),
    ("chain", "Chain"),
    ("cradle", "Newton's cradle"),
    ("bridge", "Plank bridge"),
    ("ballchain", "Wrecking ball"),
    ("mixer", "Mixer (motor)"),
];

/// The `pelt/scene` page controls: a backdrop-scene picker, the whirlpool / fountain / liquid
/// effects, the graph-tangibility lever, and clear. The scene is transient (not persisted), so the
/// actions are stateless and the builder needs no host state — a press loads or clears outright.
/// Tangibility is two explicit buttons rather than one reflecting toggle because the physics runs
/// off-thread (its live state is the actor's, not synchronously readable here); a reflecting toggle
/// is a refinement once a getter is plumbed. (Physics scenes — scene settings.)
pub(crate) fn scene_section_items() -> Vec<PaneItem> {
    let mut items = vec![PaneItem::text("app-title", "Scenes")];
    for &(id, label) in SCENE_CATALOG {
        items.push(PaneItem::button("app-btn", label, format!("scene:load:{id}")));
    }

    items.push(PaneItem::text("app-title", "Effects"));
    items.push(PaneItem::button("app-btn", "Whirlpool (vortex)", "scene:load:whirlpool"));
    items.push(PaneItem::button("app-btn", "Fountain (emitter)", "scene:load:fountain"));
    items.push(PaneItem::button("app-btn", "Liquid pool", "scene:fluid:load"));
    items.push(PaneItem::button("app-btn", "Clear liquid", "scene:fluid:clear"));

    items.push(PaneItem::text("app-title", "Graph tangibility"));
    items.push(PaneItem::button("app-btn", "Collide with the scene", "scene:tangible:on"));
    items.push(PaneItem::button("app-btn", "Pass through (default)", "scene:tangible:off"));

    // Ambient backdrops are non-rapier sims painted behind the graph (independent of the scene
    // above), so they get their own section. (Physics scenes P5.)
    items.push(PaneItem::text("app-title", "Ambient backdrop"));
    items.push(PaneItem::button("app-btn", "Game of Life", "scene:ambient:gol"));
    items.push(PaneItem::button("app-btn", "Clear ambient", "scene:ambient:clear"));

    items.push(PaneItem::text("app-title", "Clear"));
    items.push(PaneItem::button("app-btn", "Clear the scene", "scene:clear"));
    items
}

impl WindowCtx<'_> {
    /// Drain a `scene:*` settings activation key (the prefix already stripped) from the Scene page:
    /// load / clear a backdrop scene, a whirlpool / fountain / liquid effect, or set the graph's
    /// tangibility. Routes to the matching `Orrery` method (which forwards to the physics actor) and
    /// requests a redraw. An unknown key is a no-op. (Physics scenes — scene settings.)
    pub(super) fn apply_scene_key(&mut self, key: &str) {
        match key {
            "load:dropbowl" => self.orrery_mut().load_demo_scene(),
            "load:pyramid" => self.orrery_mut().load_scene(orrery::pyramid_scene()),
            "load:domino" => self.orrery_mut().load_scene(orrery::domino_scene()),
            "load:galton" => self.orrery_mut().load_scene(orrery::galton_scene()),
            "load:funnel" => self.orrery_mut().load_scene(orrery::funnel_scene()),
            "load:drift" => self.orrery_mut().load_scene(orrery::drift_scene()),
            "load:chain" => self.orrery_mut().load_scene(orrery::chain_scene()),
            "load:cradle" => self.orrery_mut().load_scene(orrery::cradle_scene()),
            "load:bridge" => self.orrery_mut().load_scene(orrery::bridge_scene()),
            "load:ballchain" => self.orrery_mut().load_scene(orrery::ball_and_chain_scene()),
            "load:mixer" => self.orrery_mut().load_scene(orrery::mixer_scene()),
            "load:whirlpool" => self.orrery_mut().load_whirlpool(),
            "load:fountain" => self.orrery_mut().load_fountain(),
            "fluid:load" => self.orrery_mut().load_demo_fluid(),
            "fluid:clear" => self.orrery_mut().clear_fluid(),
            "tangible:on" => self.orrery_mut().set_nodes_tangible(true),
            "tangible:off" => self.orrery_mut().set_nodes_tangible(false),
            "ambient:gol" => self.orrery_mut().load_game_of_life(),
            "ambient:clear" => self.orrery_mut().clear_ambient(),
            "clear" => self.orrery_mut().clear_scene(),
            _ => return,
        }
        self.view.request_redraw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_page_is_listed_and_its_controls_carry_scene_keys() {
        // The Scene page joins the `pelt` settings index, so it shows in the lane.
        let pelt = crate::settings_lane::settings_index("pelt");
        assert!(pelt.iter().any(|p| p.id == "scene"), "the pelt index lists the Scene page");

        // Every actionable control is a button keyed under `scene:` (the prefix `input.rs` strips
        // before handing the suffix to `apply_scene_key`), and the catalog + effects + levers + clear
        // are all present — a guard against a key typo drifting the page from the drain.
        let items = scene_section_items();
        let keys: Vec<&str> = items.iter().filter_map(|i| i.key.as_deref()).collect();
        assert!(!keys.is_empty(), "the page has actionable buttons");
        assert!(keys.iter().all(|k| k.starts_with("scene:")), "every button key is scene-prefixed");
        for &(id, _) in SCENE_CATALOG {
            assert!(keys.contains(&format!("scene:load:{id}").as_str()), "catalog button for {id}");
        }
        for expected in [
            "scene:load:whirlpool",
            "scene:load:fountain",
            "scene:fluid:load",
            "scene:fluid:clear",
            "scene:tangible:on",
            "scene:tangible:off",
            "scene:ambient:gol",
            "scene:ambient:clear",
            "scene:clear",
        ] {
            assert!(keys.contains(&expected), "the page exposes {expected}");
        }
    }
}
