/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Theme switching + user-theme editing. `set_theme` re-resolves the active
//! theme and re-themes the chrome / orrery / document lanes live; the rest is the
//! user-theme CRUD the settings-lane appearance editor drives — fork / remove /
//! mode toggle / per-seed HSL channel / accent harmony — over the shared
//! `apply_edited_theme` (re-derive + validate + persist + re-apply) path. Split
//! from `frame_ops.rs` to keep files under the 600-LOC ceiling.

use super::WindowCtx;

impl WindowCtx<'_> {
    /// Switch the active theme: re-resolve from the registry, rebuild the chrome
    /// CSS + tokens, drop the host-drawn caches so they re-rasterize with the new
    /// palette, persist the choice, and redraw. (Theme switcher; the orrery's own
    /// palette is themed in A2.)
    pub(super) fn set_theme(&mut self, theme_id: &str) {
        let resolution = self.shared.presentation.theme.set_active_theme(theme_id);
        self.shared.presentation.active_theme_id = resolution.resolved_id;
        // Switching THEME re-seeds the mode from the theme's own def (what the
        // def encodes as authored), so the legacy built-ins — Dark, Light,
        // High Contrast — keep their meaning; switching MODE keeps the theme.
        // (Theme-modes plan.)
        if let Some(def) = self
            .shared
            .presentation
            .theme
            .theme_def(&self.shared.presentation.active_theme_id)
        {
            self.shared.presentation.mode = register_theme::seed::default_mode_for_def(def);
        }
        self.apply_resolved_tokens(resolution.tokens);
        self.shared
            .observability
            .record_theme_activated(&self.shared.presentation.active_theme_id);
    }

    /// Switch the active MODE — the derivation profile over the current theme's
    /// seeds (theme-modes T2). Within the current contrast level the baked
    /// scheme pair means the chrome sheet's strings do not change, so the pane
    /// sessions ride `set_prefers_color_scheme` (media re-evaluation, session
    /// survives) instead of rebuilding; a contrast-level change re-bakes the
    /// pair and takes the sheet-swap path. Non-sheet theming (orrery palette,
    /// document palette, host-drawn caches) re-keys off the resolved mode
    /// tokens on the same flip.
    pub(super) fn set_mode(&mut self, mode: register_theme::theme::Mode) {
        use register_theme::theme::Mode;
        if self.shared.presentation.mode == mode {
            return;
        }
        let active = self.shared.presentation.active_theme_id.clone();
        let tokens = match &mode {
            // A custom mode (T5): the non-sheet lanes derive canonically with
            // the mode's declared (dark, hc) flags; the shell palette comes
            // from the declarative calculator over the theme's seeds. A bad
            // pick (unknown id / failed evaluation) is a logged no-op — the
            // prior mode stays.
            Mode::Custom(id) => {
                let Some(custom) = self.shared.presentation.custom_mode(id).cloned() else {
                    tracing::warn!(mode = %id, "unknown custom mode; keeping the current mode");
                    return;
                };
                let canonical = Mode::from_flags(custom.dark, custom.high_contrast);
                let Some(mut tokens) = self
                    .shared
                    .presentation
                    .theme
                    .mode_tokens(&active, &canonical)
                else {
                    return;
                };
                let Some(def) = self.shared.presentation.theme.theme_def(&active) else {
                    return;
                };
                let seeds = register_theme::seed::harmonized_seeds(def);
                match register_theme::mode_calc::chrome_from_custom_mode(&custom, &seeds) {
                    Ok(chrome) => tokens.chrome = chrome,
                    Err(err) => {
                        tracing::warn!(mode = %id, %err, "custom mode failed; keeping the current mode");
                        return;
                    }
                }
                tokens
            }
            _ => {
                let Some(tokens) = self.shared.presentation.theme.mode_tokens(&active, &mode)
                else {
                    return;
                };
                tokens
            }
        };
        self.shared.presentation.mode = mode;
        self.apply_resolved_tokens(tokens);
        self.shared.observability.record_theme_activated(&active);
    }

    /// Re-theme every lane from a freshly resolved token set — the shared tail
    /// of a theme switch and a mode switch.
    fn apply_resolved_tokens(&mut self, tokens: register_theme::theme::ThemeTokenSet) {
        self.shared.presentation.chrome_theme = tokens.chrome;
        // Rebuild at the current UI scale (and re-add syntax rules); shared with the
        // zoom / DPI rebuild path. On a scheme-only mode flip this produces the
        // identical pair-baked strings, which is what keeps the sessions on the
        // cheap path. (UI scale; theme-modes T2.)
        self.shared.presentation.rebuild_chrome_sheet();
        // Re-theme the orrery's backdrop + edges to match. (A2.)
        let (backdrop, edge) = crate::orrery_palette(&tokens);
        self.orrery_mut().set_palette(backdrop, edge);
        // Re-theme the document lane: rebuild the content-card palette and
        // broadcast the composed sheet (new colours + the user's typography) to the
        // live content actors so already-open cards re-lay + re-bake their glyph
        // colors (and re-rasterize on the new background). (P3; typography D2.)
        self.shared.presentation.document_palette = crate::document_palette(&tokens);
        let sheet = self.shared.presentation.document_sheet_composed();
        self.shared.content.constellation.retheme(sheet);
        // Focus-card snapshots are cached data-URIs rendered through the old
        // palette (no live actor to re-bake them); drop them so the next focus
        // rebuilds each through `render_content_scene` with the new theme. (P3.)
        self.view.snapshot_data_uris.clear();
        self.view.window_controls_tex = None;
        self.view.divider_tex = None;
        self.persist_settings();
        self.view.request_redraw();
    }

    /// The registered themes as settings options (id + display name + active),
    /// listed from the registry — built-ins first, then user / mod themes.
    pub(super) fn theme_options(&self) -> Vec<crate::settings_lane::ThemeOption> {
        let active = self
            .shared
            .presentation
            .active_theme_id
            .to_ascii_lowercase();
        self.shared
            .presentation
            .theme
            .list()
            .iter()
            .map(|def| crate::settings_lane::ThemeOption {
                active: def.id.to_ascii_lowercase() == active,
                id: def.id.clone(),
                name: def.name.clone(),
            })
            .collect()
    }

    /// Fork the active theme into a new editable **user** theme, persist its
    /// file, and activate it. The non-destructive "make this mine" action.
    /// (Seed-palette themes T5.)
    pub(super) fn fork_active_theme(&mut self) {
        let active = self.shared.presentation.active_theme_id.clone();
        let base = self
            .shared
            .presentation
            .theme
            .theme_def(&active)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| "Custom".to_string());
        let new_id = format!("user:{}", uuid::Uuid::new_v4());
        let new_name = format!("{base} (custom)");
        if let Some(def) = self
            .shared
            .presentation
            .theme
            .fork(&active, &new_id, &new_name)
        {
            if let Err(err) =
                crate::theme_store::save_user_theme(&self.shared.session.mere_root, &def)
            {
                tracing::warn!(%err, "failed to save forked theme");
            }
            self.set_theme(&new_id);
        }
    }

    /// Remove the active theme if it is a user theme, delete its file, and fall
    /// back to the default. Built-ins can't be removed (no-op). (T5.)
    pub(super) fn remove_active_user_theme(&mut self) {
        let active = self.shared.presentation.active_theme_id.clone();
        if self.shared.presentation.theme.remove_user_theme(&active) {
            let _ = crate::theme_store::delete_user_theme(&self.shared.session.mere_root, &active);
            self.set_theme(register_theme::theme::THEME_ID_DEFAULT);
        }
    }

    /// Toggle the active user theme's light/dark mode, re-derive, persist, and
    /// re-apply live. Built-ins are read-only (no-op). (T5.)
    pub(super) fn toggle_active_theme_mode(&mut self) {
        let active = self.shared.presentation.active_theme_id.clone();
        let Some(mut def) = self.shared.presentation.theme.theme_def(&active).cloned() else {
            return;
        };
        if !register_theme::theme::toggle_user_theme_mode(&mut def) {
            return;
        }
        self.apply_edited_theme(&active, def);
    }

    /// Set one HSL channel of one seed of the active user theme to `fraction`
    /// (0..1) of its range (`'h'` → 0..360°, `'s'` / `'l'` → 0..1), re-derive,
    /// persist, and re-apply live. The seed sliders drain here. Built-ins are
    /// read-only. (T5.)
    pub(super) fn set_active_seed_channel(&mut self, seed: &str, channel: char, fraction: f64) {
        let active = self.shared.presentation.active_theme_id.clone();
        let Some(mut def) = self.shared.presentation.theme.theme_def(&active).cloned() else {
            return;
        };
        if !register_theme::theme::set_user_theme_seed_channel(&mut def, seed, channel, fraction) {
            return;
        }
        self.apply_edited_theme(&active, def);
    }

    /// Set the active user theme's accent harmony, re-derive, persist, and
    /// re-apply live. `"custom"` unlinks the accents (each its own seed); `"lock"`
    /// captures the triad's present hue gaps so editing the base rotates them; the
    /// named presets set known offset pairs. Offsets are measured + applied in
    /// OKLCH (matching the derivation), so "lock" is non-destructive. The editor's
    /// harmony buttons drain here. Built-ins are read-only. (Seed-palette harmony.)
    pub(super) fn set_active_harmony(&mut self, key: &str) {
        let active = self.shared.presentation.active_theme_id.clone();
        let Some(mut def) = self.shared.presentation.theme.theme_def(&active).cloned() else {
            return;
        };
        if !register_theme::theme::set_user_theme_harmony(&mut def, key) {
            return;
        }
        self.apply_edited_theme(&active, def);
    }

    /// Register an edited user-theme def (re-deriving + validating), persist its
    /// file, and re-apply it live. If the edit fails validation (e.g. text
    /// contrast), the prior valid theme is kept and the edit is dropped. (T5.)
    fn apply_edited_theme(&mut self, id: &str, def: register_theme::theme::ThemeDef) {
        if let Err(err) = self.shared.presentation.theme.add_user_theme(def.clone()) {
            tracing::warn!(%err, "edited theme failed validation; keeping the prior theme");
            return;
        }
        if let Err(err) = crate::theme_store::save_user_theme(&self.shared.session.mere_root, &def)
        {
            tracing::warn!(%err, "failed to persist edited theme");
        }
        self.set_theme(id);
    }
}
