// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Persona-scoped persistence for Notochord owner policy.
//!
//! Path: `<data_root>/personas/<persona_id>/settings/notochord.json`.
//! The document contains owner rules and verified revocations. Live carrier
//! facts, principals, admitted streams, and session counters have no field in
//! [`OwnerPolicySet`] and therefore cannot survive a restart accidentally.

use std::path::{Path, PathBuf};
use std::{fs, io};

use notochord::OwnerPolicySet;

use crate::manifest::PersonaId;
use crate::persona_settings_store::persona_settings_dir;

/// Persona-scoped Notochord policy filename.
pub const NOTOCHORD_POLICY_FILENAME: &str = "notochord.json";

/// Path to one persona's persisted owner policy.
pub fn notochord_policy_path(data_root: &Path, persona: PersonaId) -> PathBuf {
    persona_settings_dir(data_root, persona).join(NOTOCHORD_POLICY_FILENAME)
}

/// Load the owner's Notochord policy, or `None` for a fresh persona.
///
/// Malformed policy is an error. Silently replacing it with a closed default
/// would hide both owner intent and revocation state.
pub fn load_notochord_policy(
    data_root: &Path,
    persona: PersonaId,
) -> io::Result<Option<OwnerPolicySet>> {
    let path = notochord_policy_path(data_root, persona);
    match fs::read_to_string(path) {
        Ok(json) => serde_json::from_str(&json)
            .map(Some)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Persist owner policy beside the persona's other settings.
///
/// The temporary file is fully written before it replaces the previous
/// document. On Windows an existing destination cannot be renamed over, so a
/// short-lived backup keeps the prior policy recoverable until the new one is
/// in place.
pub fn save_notochord_policy(
    data_root: &Path,
    persona: PersonaId,
    policies: &OwnerPolicySet,
) -> io::Result<()> {
    policies
        .validate()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let target = notochord_policy_path(data_root, persona);
    let directory = target
        .parent()
        .expect("a settings filename always has a parent");
    fs::create_dir_all(directory)?;

    let json = serde_json::to_string_pretty(policies)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let temporary = target.with_extension("json.tmp");
    let backup = target.with_extension("json.previous");
    fs::write(&temporary, json)?;

    if !target.exists() {
        return fs::rename(temporary, target);
    }
    if backup.exists() {
        fs::remove_file(&backup)?;
    }
    fs::rename(&target, &backup)?;
    match fs::rename(&temporary, &target) {
        Ok(()) => {
            fs::remove_file(backup)?;
            Ok(())
        }
        Err(error) => {
            let _ = fs::rename(&backup, &target);
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use notochord::{NetworkId, OwnerNetworkPolicy, OwnerPolicyEdit, ServiceAccess, ServiceRule};

    use super::*;

    fn scratch(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "mere-notochord-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn settings() -> OwnerPolicySet {
        let mut network = OwnerNetworkPolicy::closed(NetworkId([7; 32]));
        network.apply(OwnerPolicyEdit::Service {
            path: "/services/murm".to_string(),
            rule: ServiceRule::new(
                ServiceAccess::MemberOnly,
                "mere.network",
                ["connect"],
                true,
                Some(3),
            ),
        });
        network.apply(OwnerPolicyEdit::Transit(true));
        let mut policies = OwnerPolicySet::new();
        policies.upsert(network);
        policies
    }

    #[test]
    fn owner_policy_survives_restart_without_live_session_state() {
        let root = scratch("restart");
        let persona = PersonaId::default_persona();
        let policies = settings();

        save_notochord_policy(&root, persona, &policies).expect("first save");
        let restored = load_notochord_policy(&root, persona)
            .expect("load")
            .expect("policy exists");
        assert_eq!(restored, policies);

        let json = fs::read_to_string(notochord_policy_path(&root, persona)).expect("read policy");
        assert!(!json.contains("authenticated_initiator"));
        assert!(!json.contains("session_id"));
        assert!(!json.contains("AdmittedPrincipal"));
        assert!(!json.contains("SessionFacts"));

        fs::remove_dir_all(root).expect("remove scratch");
    }

    #[test]
    fn a_second_save_replaces_the_policy_document() {
        let root = scratch("replace");
        let persona = PersonaId::default_persona();
        let mut policies = settings();
        save_notochord_policy(&root, persona, &policies).expect("first save");

        policies
            .network_mut(NetworkId([7; 32]))
            .expect("network")
            .apply(OwnerPolicyEdit::Transit(false));
        save_notochord_policy(&root, persona, &policies).expect("replacement save");

        let restored = load_notochord_policy(&root, persona)
            .expect("load")
            .expect("policy exists");
        assert_eq!(restored, policies);
        assert!(
            !notochord_policy_path(&root, persona)
                .with_extension("json.previous")
                .exists()
        );

        fs::remove_dir_all(root).expect("remove scratch");
    }

    #[test]
    fn an_unsupported_document_is_neither_loaded_nor_saved() {
        let root = scratch("version");
        let persona = PersonaId::default_persona();
        let mut policies = settings();
        policies.version += 1;
        let error = save_notochord_policy(&root, persona, &policies).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);

        let path = notochord_policy_path(&root, persona);
        fs::create_dir_all(path.parent().unwrap()).expect("settings directory");
        fs::write(&path, serde_json::to_string(&policies).unwrap()).expect("future policy");
        let error = load_notochord_policy(&root, persona).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);

        fs::remove_dir_all(root).expect("remove scratch");
    }
}
