/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Frame-layout persistence.
//!
//! `FrameLayout` is per-window state but the split ratios are worth
//! restoring across launches so the user's spatial arrangement
//! survives. Serialized as pretty JSON next to the rest of the host's
//! local data.

use mere_frame::FrameLayout;

/// On-disk path for the saved frame layout. Returns `None` when the
/// platform's data-local dir can't be resolved (rare; e.g., headless
/// environments without a HOME).
pub fn frame_layout_path() -> Option<std::path::PathBuf> {
    dirs::data_local_dir().map(|d| d.join("mere").join("mere-host").join("frame_layout.json"))
}

/// Root of the per-session directory tree managed by
/// `mere_host_runtime::ManifestStore` + `session_graph_store`.
/// Resolves to `<data_local_dir>/mere/mere-host/sessions/` so it
/// sits alongside `frame_layout.json` + `keymap.json`.
pub fn sessions_dir() -> Option<std::path::PathBuf> {
    dirs::data_local_dir().map(|d| d.join("mere").join("mere-host").join("sessions"))
}

pub fn load_frame_layout() -> Option<FrameLayout> {
    let path = frame_layout_path()?;
    let bytes = std::fs::read(&path).ok()?;
    match serde_json::from_slice::<FrameLayout>(&bytes) {
        Ok(layout) => {
            tracing::debug!(path = %path.display(), "loaded saved frame layout");
            Some(layout)
        }
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                error = %err,
                "failed to parse saved frame layout — falling back to fixture"
            );
            None
        }
    }
}

pub fn save_frame_layout(layout: &FrameLayout) {
    let Some(path) = frame_layout_path() else {
        tracing::warn!("no data_local_dir; skipping frame layout save");
        return;
    };
    if let Some(parent) = path.parent() {
        if let Err(err) = std::fs::create_dir_all(parent) {
            tracing::warn!(
                parent = %parent.display(),
                error = %err,
                "failed to create config dir"
            );
            return;
        }
    }
    match serde_json::to_vec_pretty(layout) {
        Ok(bytes) => {
            if let Err(err) = std::fs::write(&path, bytes) {
                tracing::warn!(
                    path = %path.display(),
                    error = %err,
                    "failed to write frame layout"
                );
            } else {
                tracing::debug!(path = %path.display(), "saved frame layout");
            }
        }
        Err(err) => {
            tracing::warn!(error = %err, "failed to serialize frame layout");
        }
    }
}
