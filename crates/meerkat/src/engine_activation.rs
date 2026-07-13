/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Which engines are active this session.
//!
//! An engine has three levels (engine-picker plan §3): **absent** (the build does
//! not carry it, so it never registers), **off** (present but deactivated — never
//! routed to, no actors spawned), and **active**. This type owns the off/active
//! line; presence is the host's `engine_present`, and the two combine in
//! `engine_available` (the closure the routing policy filters with).
//!
//! Two layers, the per-session override winning over the global default:
//! - **global** — disabled ids from the app-wide settings (`mere_root/settings.json`).
//! - **session** — explicit per-engine overrides for this session (the apparatus
//!   toggle, Phase 2). Held in memory today; per-session persistence rides the
//!   session manifest when that is wired.

use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(crate) struct EngineActivation {
    /// Globally disabled engine ids (from the app-wide settings). Empty = every
    /// present engine is active by default.
    global_disabled: HashSet<String>,
    /// Per-session overrides (engine id → active), winning over the global default.
    session_override: HashMap<String, bool>,
}

impl EngineActivation {
    /// Seed from the global disabled list (settings.json `disabled_engines`).
    pub(crate) fn new(global_disabled: impl IntoIterator<Item = String>) -> Self {
        Self {
            global_disabled: global_disabled.into_iter().collect(),
            session_override: HashMap::new(),
        }
    }

    /// Whether engine `id` is active this session: the per-session override if one
    /// is set, otherwise "not globally disabled".
    pub(crate) fn is_enabled(&self, id: &str) -> bool {
        match self.session_override.get(id) {
            Some(active) => *active,
            None => !self.global_disabled.contains(id),
        }
    }

    /// Set this session's override for `id` (the apparatus per-session toggle).
    #[allow(dead_code)] // consumed by the Phase 2 apparatus engine manager
    pub(crate) fn set_session(&mut self, id: &str, active: bool) {
        self.session_override.insert(id.to_string(), active);
    }

    /// Clear this session's override for `id`, falling back to the global default.
    #[allow(dead_code)] // consumed by the Phase 2 apparatus engine manager
    pub(crate) fn clear_session(&mut self, id: &str) {
        self.session_override.remove(id);
    }

    /// Set the global default for `id`. The caller persists via `persist_settings`.
    #[allow(dead_code)] // consumed by the Phase 2 apparatus engine manager
    pub(crate) fn set_global(&mut self, id: &str, active: bool) {
        if active {
            self.global_disabled.remove(id);
        } else {
            self.global_disabled.insert(id.to_string());
        }
    }

    /// The globally-disabled ids, for persistence into settings.json (sorted so the
    /// written file is stable across runs).
    pub(crate) fn global_disabled_vec(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.global_disabled.iter().cloned().collect();
        ids.sort();
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::EngineActivation;

    #[test]
    fn enabled_by_default() {
        let act = EngineActivation::default();
        assert!(act.is_enabled("scrying.web"));
        assert!(act.is_enabled("nematic.gemtext"));
    }

    #[test]
    fn global_disable_takes_effect() {
        let act = EngineActivation::new(["scrying.web".to_string()]);
        assert!(!act.is_enabled("scrying.web"));
        assert!(act.is_enabled("genet.web"));
    }

    #[test]
    fn session_override_wins_over_global() {
        let mut act = EngineActivation::new(["scrying.web".to_string()]);
        // Re-enable globally-disabled scrying for this session.
        act.set_session("scrying.web", true);
        assert!(act.is_enabled("scrying.web"));
        // Disable a globally-enabled engine for this session.
        act.set_session("nematic.gemtext", false);
        assert!(!act.is_enabled("nematic.gemtext"));
        // Clearing the override falls back to the global default.
        act.clear_session("scrying.web");
        assert!(!act.is_enabled("scrying.web"));
    }

    #[test]
    fn global_disabled_vec_is_sorted() {
        let mut act = EngineActivation::default();
        act.set_global("weld.web", false);
        act.set_global("scrying.web", false);
        assert_eq!(act.global_disabled_vec(), vec!["scrying.web", "weld.web"]);
    }
}
