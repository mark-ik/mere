/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Theme switching + user-theme editing. `set_theme` re-resolves the active
//! theme and re-themes the chrome / orrery / document lanes live; the rest is the
//! user-theme CRUD the settings-lane appearance editor drives — fork / remove /
//! mode toggle / per-seed HSL channel / accent harmony — over the shared
//! `apply_edited_theme` (re-derive + validate + persist + re-apply) path. Split
//! from `frame_ops.rs` to keep files under the 600-LOC ceiling.

use super::{WindowCtx, apparatus};

impl WindowCtx<'_> {
    /// Switch the active theme: re-resolve from the registry, rebuild the chrome
    /// CSS + tokens, drop the host-drawn caches so they re-rasterize with the new
    /// palette, persist the choice, and redraw. (Theme switcher; the orrery's own
    /// palette is themed in A2.)
    pub(super) fn set_theme(&mut self, theme_id: &str) {
        let resolution = self.shared.presentation.theme.set_active_theme(theme_id);
        self.shared.presentation.active_theme_id = resolution.resolved_id;
        self.shared.presentation.chrome_theme = resolution.tokens.chrome;
        self.shared.presentation.chrome_sheet = crate::chrome_sheet(&self.shared.presentation.chrome_theme);
        // Re-theme the orrery's backdrop + edges to match. (A2.)
        let (backdrop, edge) = crate::orrery_palette(&resolution.tokens);
        self.orrery_mut().set_palette(backdrop, edge);
        // Re-theme the document lane: rebuild the content-card palette and
        // broadcast it to the live content actors so already-open cards re-bake
        // their glyph colors (and re-rasterize on the new background). (P3.)
        let doc_palette = crate::document_palette(&resolution.tokens);
        self.shared.presentation.document_palette = doc_palette;
        self.shared.content.constellation.retheme(doc_palette);
        // Focus-card snapshots are cached data-URIs rendered through the old
        // palette (no live actor to re-bake them); drop them so the next focus
        // rebuilds each through `render_content_scene` with the new theme. (P3.)
        self.view.snapshot_data_uris.clear();
        self.view.window_controls_tex = None;
        self.view.divider_tex = None;
        self.persist_settings();
        self.shared.observability
            .record_theme_activated(&self.shared.presentation.active_theme_id);
        self.view.request_redraw();
    }

    /// The registered themes as apparatus options (id + display name + active),
    /// listed from the registry — built-ins first, then user / mod themes.
    pub(super) fn theme_options(&self) -> Vec<apparatus::ThemeOption> {
        let active = self.shared.presentation.active_theme_id.to_ascii_lowercase();
        self.shared
            .presentation
            .theme
            .list()
            .iter()
            .map(|def| apparatus::ThemeOption {
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
            if let Err(err) = crate::theme_store::save_user_theme(&self.shared.session.mere_root, &def) {
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
        if def.source != register_theme::theme::ThemeSource::User {
            return;
        }
        def.seeds.dark = !def.seeds.dark;
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
        if def.source != register_theme::theme::ThemeSource::User {
            return;
        }
        let target = match seed {
            "primary" => &mut def.seeds.primary,
            "secondary" => &mut def.seeds.secondary,
            "tertiary" => &mut def.seeds.tertiary,
            "neutral" => &mut def.seeds.neutral,
            _ => return,
        };
        let (mut h, mut s, mut l) = tincture::color_to_hsl(*target);
        let f = fraction.clamp(0.0, 1.0);
        match channel {
            'h' => h = (f * 360.0).rem_euclid(360.0),
            's' => s = f,
            'l' => l = f,
            _ => return,
        }
        *target = tincture::color_from_hsl(h, s, l);
        self.apply_edited_theme(&active, def);
    }

    /// Set the active user theme's accent harmony, re-derive, persist, and
    /// re-apply live. `"custom"` unlinks the accents (each its own seed); `"lock"`
    /// captures the triad's present hue gaps so editing the base rotates them; the
    /// named presets set known offset pairs. Offsets are measured + applied in
    /// OKLCH (matching the derivation), so "lock" is non-destructive. The editor's
    /// harmony buttons drain here. Built-ins are read-only. (Seed-palette harmony.)
    pub(super) fn set_active_harmony(&mut self, key: &str) {
        use register_theme::theme::Harmony;
        use tincture::oklch::Oklch;
        let active = self.shared.presentation.active_theme_id.clone();
        let Some(mut def) = self.shared.presentation.theme.theme_def(&active).cloned() else {
            return;
        };
        if def.source != register_theme::theme::ThemeSource::User {
            return;
        }
        let base_h = Oklch::from_srgb(def.seeds.primary).h;
        let gap = |c| (Oklch::from_srgb(c).h - base_h).to_degrees().rem_euclid(360.0) as f32;
        def.harmony = match key {
            "custom" => Harmony::Custom,
            "lock" => Harmony::Locked {
                secondary_deg: gap(def.seeds.secondary),
                tertiary_deg: gap(def.seeds.tertiary),
            },
            "triadic" => Harmony::Locked { secondary_deg: 120.0, tertiary_deg: 240.0 },
            "analogous" => Harmony::Locked { secondary_deg: 30.0, tertiary_deg: -30.0 },
            "complementary" => Harmony::Locked { secondary_deg: 180.0, tertiary_deg: 150.0 },
            "mono" => Harmony::Locked { secondary_deg: 0.0, tertiary_deg: 0.0 },
            _ => return,
        };
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
        if let Err(err) = crate::theme_store::save_user_theme(&self.shared.session.mere_root, &def) {
            tracing::warn!(%err, "failed to persist edited theme");
        }
        self.set_theme(id);
    }
}
