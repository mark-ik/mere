/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Pure persistence-boundary reducer intents.
//!
//! This module does not call concrete stores. It expresses workspace,
//! preference, and private local-memory requests as typed effects for service
//! glue to execute through `WorkspaceRepository`, `SettingsStore`, and
//! `MnemStore` implementations.

use serde::{Deserialize, Serialize};

use super::{GraphWorkspace, WorkspaceEffect, WorkspaceId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PersistenceIntent {
    RequestWorkspaceSave { workspace_id: WorkspaceId },
    MarkWorkspaceSaved,
    RequestPreferencesSave,
    RequestMnemBlobLoad { key: String },
    RequestMnemBlobSave { key: String, value: Vec<u8> },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PersistenceOutcome {
    pub state_changed: bool,
    pub effects_emitted: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MnemRequest {
    LoadBlob { key: String },
    SaveBlob { key: String, value: Vec<u8> },
}

pub fn reduce_persistence_intent(
    workspace: &mut GraphWorkspace,
    intent: PersistenceIntent,
) -> PersistenceOutcome {
    match intent {
        PersistenceIntent::RequestWorkspaceSave { workspace_id } => {
            workspace.push_effect(WorkspaceEffect::PersistWorkspace { workspace_id });
            PersistenceOutcome {
                state_changed: false,
                effects_emitted: 1,
            }
        }
        PersistenceIntent::MarkWorkspaceSaved => {
            let changed = workspace.workbench.has_unsaved_changes;
            workspace.workbench.has_unsaved_changes = false;
            PersistenceOutcome {
                state_changed: changed,
                effects_emitted: 0,
            }
        }
        PersistenceIntent::RequestPreferencesSave => {
            workspace.push_effect(WorkspaceEffect::PersistPreferences {
                preferences: workspace.chrome.preferences.clone(),
            });
            PersistenceOutcome {
                state_changed: false,
                effects_emitted: 1,
            }
        }
        PersistenceIntent::RequestMnemBlobLoad { key } => {
            workspace.push_effect(WorkspaceEffect::RequestMnem(MnemRequest::LoadBlob { key }));
            PersistenceOutcome {
                state_changed: false,
                effects_emitted: 1,
            }
        }
        PersistenceIntent::RequestMnemBlobSave { key, value } => {
            workspace.push_effect(WorkspaceEffect::RequestMnem(MnemRequest::SaveBlob {
                key,
                value,
            }));
            PersistenceOutcome {
                state_changed: false,
                effects_emitted: 1,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::{NavigatorSidebarPreference, ThemeModePreference, WorkspacePreferences};

    #[test]
    fn workspace_save_request_emits_effect_only() {
        let mut workspace = GraphWorkspace::new();
        let workspace_id = WorkspaceId::new("default");

        let outcome = reduce_persistence_intent(
            &mut workspace,
            PersistenceIntent::RequestWorkspaceSave {
                workspace_id: workspace_id.clone(),
            },
        );

        assert!(!outcome.state_changed);
        assert_eq!(outcome.effects_emitted, 1);
        assert_eq!(
            workspace.drain_effects(),
            vec![WorkspaceEffect::PersistWorkspace { workspace_id }]
        );
    }

    #[test]
    fn mark_workspace_saved_clears_dirty_state_without_effects() {
        let mut workspace = GraphWorkspace::new();
        workspace.workbench.has_unsaved_changes = true;

        let outcome =
            reduce_persistence_intent(&mut workspace, PersistenceIntent::MarkWorkspaceSaved);

        assert!(outcome.state_changed);
        assert!(!workspace.workbench.has_unsaved_changes);
        assert!(workspace.pending_effects.effects.is_empty());
    }

    #[test]
    fn preferences_save_emits_current_preferences_snapshot() {
        let mut workspace = GraphWorkspace::new();
        workspace.chrome.preferences = WorkspacePreferences {
            theme_mode: ThemeModePreference::Dark,
            navigator_sidebar: NavigatorSidebarPreference::Hidden,
        };

        reduce_persistence_intent(&mut workspace, PersistenceIntent::RequestPreferencesSave);

        assert_eq!(
            workspace.drain_effects(),
            vec![WorkspaceEffect::PersistPreferences {
                preferences: WorkspacePreferences {
                    theme_mode: ThemeModePreference::Dark,
                    navigator_sidebar: NavigatorSidebarPreference::Hidden,
                }
            }]
        );
    }

    #[test]
    fn mnem_blob_requests_emit_private_memory_effects() {
        let mut workspace = GraphWorkspace::new();

        reduce_persistence_intent(
            &mut workspace,
            PersistenceIntent::RequestMnemBlobSave {
                key: "clip-cache/example".to_string(),
                value: vec![1, 2, 3],
            },
        );
        reduce_persistence_intent(
            &mut workspace,
            PersistenceIntent::RequestMnemBlobLoad {
                key: "clip-cache/example".to_string(),
            },
        );

        assert_eq!(
            workspace.drain_effects(),
            vec![
                WorkspaceEffect::RequestMnem(MnemRequest::SaveBlob {
                    key: "clip-cache/example".to_string(),
                    value: vec![1, 2, 3],
                }),
                WorkspaceEffect::RequestMnem(MnemRequest::LoadBlob {
                    key: "clip-cache/example".to_string(),
                }),
            ]
        );
    }
}
