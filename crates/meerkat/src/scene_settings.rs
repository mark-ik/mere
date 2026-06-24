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

/// The backdrop scenes shown as load buttons, each id riding `scene:<id>`. The id is the flat scene
/// **name** [`WindowCtx::load_named_scene`] maps to a `SceneSpec` catalog constructor — the same
/// vocabulary the `>scene <name>` omnibar verb uses, so the page and the verb never drift. (Physics
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
        items.push(PaneItem::button("app-btn", label, format!("scene:{id}")));
    }

    items.push(PaneItem::text("app-title", "Effects"));
    items.push(PaneItem::button("app-btn", "Whirlpool (vortex)", "scene:whirlpool"));
    items.push(PaneItem::button("app-btn", "Fountain (emitter)", "scene:fountain"));
    items.push(PaneItem::button("app-btn", "Liquid pool", "scene:fluid"));
    items.push(PaneItem::button("app-btn", "Clear liquid", "scene:fluidclear"));

    items.push(PaneItem::text("app-title", "Graph tangibility"));
    items.push(PaneItem::button("app-btn", "Collide with the scene", "scene:tangible"));
    items.push(PaneItem::button("app-btn", "Pass through (default)", "scene:intangible"));

    // Ambient backdrops are non-rapier sims painted behind the graph (independent of the scene
    // above), so they get their own section. (Physics scenes P5.)
    items.push(PaneItem::text("app-title", "Ambient backdrop"));
    items.push(PaneItem::button("app-btn", "Game of Life", "scene:gol"));
    items.push(PaneItem::button("app-btn", "N-body drift", "scene:nbody"));
    items.push(PaneItem::button("app-btn", "Particle-life", "scene:particles"));
    items.push(PaneItem::button("app-btn", "Falling sand", "scene:sand"));
    items.push(PaneItem::button("app-btn", "Clear ambient", "scene:ambientclear"));

    items.push(PaneItem::text("app-title", "Clear"));
    items.push(PaneItem::button("app-btn", "Clear everything", "scene:clear"));
    items
}

impl WindowCtx<'_> {
    /// Load (or clear) a scene / effect / ambient backdrop by its flat **name** — the single
    /// vocabulary shared by the Scene page buttons (`scene:<name>`, drained in `input.rs`) and the
    /// `>scene <name>` omnibar verb (drained in `command_drain`). Routes to the matching `Orrery`
    /// method (which forwards to the physics actor), requests a redraw, and returns whether the name
    /// was known (so a caller can report an unknown name). A few friendly aliases are accepted.
    /// (Physics scenes — scene settings.)
    pub(super) fn load_named_scene(&mut self, name: &str) -> bool {
        let matched = match name.trim().to_ascii_lowercase().as_str() {
            "dropbowl" | "bowl" => {
                self.orrery_mut().load_demo_scene();
                true
            }
            "pyramid" => {
                self.orrery_mut().load_scene(orrery::pyramid_scene());
                true
            }
            "domino" | "dominoes" => {
                self.orrery_mut().load_scene(orrery::domino_scene());
                true
            }
            "galton" => {
                self.orrery_mut().load_scene(orrery::galton_scene());
                true
            }
            "funnel" => {
                self.orrery_mut().load_scene(orrery::funnel_scene());
                true
            }
            "drift" => {
                self.orrery_mut().load_scene(orrery::drift_scene());
                true
            }
            "chain" => {
                self.orrery_mut().load_scene(orrery::chain_scene());
                true
            }
            "cradle" => {
                self.orrery_mut().load_scene(orrery::cradle_scene());
                true
            }
            "bridge" => {
                self.orrery_mut().load_scene(orrery::bridge_scene());
                true
            }
            "ballchain" | "wreck" | "wrecking" => {
                self.orrery_mut().load_scene(orrery::ball_and_chain_scene());
                true
            }
            "mixer" => {
                self.orrery_mut().load_scene(orrery::mixer_scene());
                true
            }
            "whirlpool" => {
                self.orrery_mut().load_whirlpool();
                true
            }
            "fountain" => {
                self.orrery_mut().load_fountain();
                true
            }
            "fluid" | "pool" | "liquid" => {
                self.orrery_mut().load_demo_fluid();
                true
            }
            "fluidclear" | "nofluid" => {
                self.orrery_mut().clear_fluid();
                true
            }
            "gol" | "life" | "gameoflife" => {
                self.orrery_mut().load_game_of_life();
                true
            }
            "nbody" | "galaxy" => {
                self.orrery_mut().load_nbody();
                true
            }
            "particles" | "particlelife" | "plife" => {
                self.orrery_mut().load_particle_life();
                true
            }
            "sand" | "fallingsand" => {
                self.orrery_mut().load_sand();
                true
            }
            "ambientclear" | "noambient" => {
                self.orrery_mut().clear_ambient();
                true
            }
            "tangible" => {
                self.orrery_mut().set_nodes_tangible(true);
                true
            }
            "intangible" => {
                self.orrery_mut().set_nodes_tangible(false);
                true
            }
            "clear" | "none" => {
                self.orrery_mut().clear_scene();
                self.orrery_mut().clear_fluid();
                self.orrery_mut().clear_ambient();
                true
            }
            _ => false,
        };
        if matched {
            self.view.request_redraw();
        }
        matched
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

        // Every actionable control is a button keyed `scene:<name>` (the prefix `input.rs` strips
        // before handing the name to `load_named_scene`), and the catalog + effects + levers + clear
        // are all present — a guard against a key typo drifting the page from the drain.
        let items = scene_section_items();
        let keys: Vec<&str> = items.iter().filter_map(|i| i.key.as_deref()).collect();
        assert!(!keys.is_empty(), "the page has actionable buttons");
        assert!(keys.iter().all(|k| k.starts_with("scene:")), "every button key is scene-prefixed");
        for &(id, _) in SCENE_CATALOG {
            assert!(keys.contains(&format!("scene:{id}").as_str()), "catalog button for {id}");
        }
        for expected in [
            "scene:whirlpool",
            "scene:fountain",
            "scene:fluid",
            "scene:fluidclear",
            "scene:tangible",
            "scene:intangible",
            "scene:gol",
            "scene:nbody",
            "scene:particles",
            "scene:sand",
            "scene:ambientclear",
            "scene:clear",
        ] {
            assert!(keys.contains(&expected), "the page exposes {expected}");
        }
    }
}
