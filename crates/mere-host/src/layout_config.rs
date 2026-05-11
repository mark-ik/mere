/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Persisted chrome layout — which edge the shellbar and the
//! workbench tile strip live on. Both are independent, both cycle
//! through `Top → Right → Bottom → Left → Top` so a single action
//! covers every position.

use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum EdgePosition {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}

impl EdgePosition {
    /// Cycle through the four edges in a stable clockwise order.
    /// Top → Right → Bottom → Left → Top.
    pub fn cycle(self) -> Self {
        match self {
            Self::Top => Self::Right,
            Self::Right => Self::Bottom,
            Self::Bottom => Self::Left,
            Self::Left => Self::Top,
        }
    }

    /// True when this edge is along the window's vertical axis
    /// (left or right) — the rendering layer uses this to decide
    /// whether the chrome strip is row-shaped or column-shaped.
    pub fn is_vertical(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

/// The whole chrome-position config persisted to disk. Kept separate
/// from `FrameLayout` so the user can reset one without losing the
/// other.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ChromeLayoutConfig {
    pub shellbar: EdgePosition,
    pub workbench_strip: WorkbenchStripDefault,
}

/// Workbench strip default — separate type so we can give it a
/// different `Default` (left-side feels more useful than top).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkbenchStripDefault(pub EdgePosition);

impl Default for WorkbenchStripDefault {
    fn default() -> Self {
        Self(EdgePosition::Left)
    }
}

/// On-disk path for the chrome layout config. Sits next to
/// `frame_layout.json` under the same per-platform local data dir.
pub fn chrome_layout_path() -> Option<std::path::PathBuf> {
    dirs::data_local_dir().map(|d| {
        d.join("mere")
            .join("mere-host")
            .join("chrome_layout.json")
    })
}

pub fn load_chrome_layout() -> ChromeLayoutConfig {
    let Some(path) = chrome_layout_path() else {
        return ChromeLayoutConfig::default();
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return ChromeLayoutConfig::default();
    };
    match serde_json::from_slice::<ChromeLayoutConfig>(&bytes) {
        Ok(cfg) => {
            tracing::debug!(path = %path.display(), "loaded chrome layout config");
            cfg
        }
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                error = %err,
                "failed to parse chrome layout config — falling back to default"
            );
            ChromeLayoutConfig::default()
        }
    }
}

pub fn save_chrome_layout(cfg: &ChromeLayoutConfig) {
    let Some(path) = chrome_layout_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_vec_pretty(cfg) {
        Ok(bytes) => {
            if let Err(err) = std::fs::write(&path, bytes) {
                tracing::warn!(
                    path = %path.display(),
                    error = %err,
                    "failed to write chrome layout config"
                );
            }
        }
        Err(err) => tracing::warn!(error = %err, "failed to serialize chrome layout config"),
    }
}
