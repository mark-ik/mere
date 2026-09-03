// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! What one face may do over SSH.
//!
//! A person has personae, plural, and they are not interchangeable: the
//! work face reaches everything, the research face reads, the burner gets
//! in and does one thing. This module is where that difference becomes
//! mechanical — a face's policy fixes the principals its certificates may
//! name and the actions its grants may carry, so *which persona was
//! unlocked* decides reach, rather than which machine happened to run the
//! command.
//!
//! Two independent enforcement points, which is what makes the burner
//! safe rather than merely narrow:
//!
//! 1. **The certificate.** Actions the policy omits are extensions the
//!    certificate never carries, so the host permits no pty, no
//!    forwarding, nothing.
//! 2. **The host's enrollment line.** `principals="markik"` in a
//!    `cert-authority` line means a certificate naming any other principal
//!    is refused *by sshd*, before the certificate's own contents matter.
//!
//! A policy lives in the profile as an ordinary slot, so it travels with
//! the face and needs no change to the profile wire format.
//!
//! Feature `ssh`.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::IdentityError;
use crate::carry::{
    ACTION_SSH_AGENT_FORWARD, ACTION_SSH_LOGIN, ACTION_SSH_PORT_FORWARD, ACTION_SSH_PTY,
};
use crate::vault::{
    CredentialLineage, IdentitySlot, Profile, ProtocolKey, SecretBytes, UnlockTier,
};

/// The `mod_id` a face's SSH policy is stored under.
pub const SSH_FACE_MOD_ID: &str = "ssh-face";

/// One face's SSH reach.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FacePolicy {
    /// Unix accounts certificates for this face may name.
    pub principals: Vec<String>,
    /// `ssh.*` actions its grants carry, which become certificate extensions.
    pub actions: BTreeSet<String>,
    /// A command to force in place of whatever the client asks for.
    pub force_command: Option<String>,
    /// Addresses its certificates work from, in OpenSSH's CIDR list form.
    pub source_address: Option<String>,
}

impl FacePolicy {
    /// The full-reach face: login, terminal, and both forwardings.
    pub fn work(principal: impl Into<String>) -> Self {
        Self {
            principals: vec![principal.into()],
            actions: [
                ACTION_SSH_LOGIN,
                ACTION_SSH_PTY,
                ACTION_SSH_AGENT_FORWARD,
                ACTION_SSH_PORT_FORWARD,
            ]
            .iter()
            .map(|action| (*action).to_string())
            .collect(),
            force_command: None,
            source_address: None,
        }
    }

    /// A terminal and nothing to carry with it: no agent, no ports.
    ///
    /// The reason to have this rather than reuse [`Self::work`]: agent
    /// forwarding hands the far end the ability to sign as you, which is
    /// exactly the authority a research face should not be lending out.
    pub fn research(principal: impl Into<String>) -> Self {
        Self {
            principals: vec![principal.into()],
            actions: [ACTION_SSH_LOGIN, ACTION_SSH_PTY]
                .iter()
                .map(|action| (*action).to_string())
                .collect(),
            force_command: None,
            source_address: None,
        }
    }

    /// One principal, one command, no extensions at all.
    pub fn burner(principal: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            principals: vec![principal.into()],
            actions: [ACTION_SSH_LOGIN.to_string()].into_iter().collect(),
            force_command: Some(command.into()),
            source_address: None,
        }
    }

    /// Whether this policy is a narrowing of `parent`.
    ///
    /// The delegation grammar's own attenuation rule, applied to the face
    /// layer: a face may drop principals and actions, and may add a forced
    /// command, but may never widen either set.
    pub fn attenuates(&self, parent: &Self) -> bool {
        self.actions.is_subset(&parent.actions)
            && self
                .principals
                .iter()
                .all(|principal| parent.principals.contains(principal))
    }

    /// Actions as the borrowed strings the grant builder wants.
    pub fn action_refs(&self) -> Vec<&str> {
        self.actions.iter().map(String::as_str).collect()
    }
}

/// The protocol key a face policy stores under.
pub fn policy_key() -> ProtocolKey {
    ProtocolKey::new(SSH_FACE_MOD_ID, None)
}

/// Read a profile's face policy.
///
/// Returns `None` when the face has never been given one; callers should
/// fall back to [`FacePolicy::work`] under the profile's own name rather
/// than inventing a wider default.
pub fn load_policy(profile: &Profile) -> Result<Option<FacePolicy>, IdentityError> {
    let Some(IdentitySlot::Direct { payload, .. }) = profile.slots.get(&policy_key()) else {
        return Ok(None);
    };
    serde_json::from_slice(payload.as_slice())
        .map(Some)
        .map_err(|err| IdentityError::Backend(format!("decode ssh face policy: {err}")))
}

/// Write a profile's face policy, replacing any previous one.
pub fn store_policy(profile: &mut Profile, policy: &FacePolicy) -> Result<(), IdentityError> {
    let encoded = serde_json::to_vec(policy)
        .map_err(|err| IdentityError::Backend(format!("encode ssh face policy: {err}")))?;
    profile.slots.insert(
        policy_key(),
        IdentitySlot::Direct {
            kind: SSH_FACE_MOD_ID.to_string(),
            payload: SecretBytes::new(encoded),
            // The policy is derived from a decision, not from key material:
            // losing it costs a re-declaration, not a re-registration.
            lineage: CredentialLineage::LocallyDerived,
            unlock_tier: UnlockTier::Session,
        },
    );
    Ok(())
}

/// The policy in force for a profile: its stored one, or a work face named
/// for the profile itself.
pub fn effective_policy(profile: &Profile) -> Result<FacePolicy, IdentityError> {
    Ok(load_policy(profile)?.unwrap_or_else(|| FacePolicy::work(profile.id.0.clone())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Ed25519Keypair;
    use crate::vault::ProfileId;

    fn profile(id: &str) -> Profile {
        Profile::new(ProfileId(id.into()), id, Ed25519Keypair::from_seed([5; 32]))
    }

    #[test]
    fn a_policy_round_trips_through_a_slot() {
        let mut profile = profile("work");
        let policy = FacePolicy::research("markik");
        store_policy(&mut profile, &policy).unwrap();
        assert_eq!(load_policy(&profile).unwrap(), Some(policy));
    }

    #[test]
    fn storing_twice_replaces_rather_than_accumulates() {
        let mut profile = profile("work");
        store_policy(&mut profile, &FacePolicy::work("markik")).unwrap();
        store_policy(&mut profile, &FacePolicy::burner("burner", "uptime")).unwrap();
        assert_eq!(profile.slots.len(), 1);
        assert_eq!(
            load_policy(&profile).unwrap().unwrap().principals,
            vec!["burner".to_string()]
        );
    }

    /// An absent policy must not silently mean "full reach for any name".
    #[test]
    fn the_default_face_is_named_for_its_profile() {
        let policy = effective_policy(&profile("research")).unwrap();
        assert_eq!(policy.principals, vec!["research".to_string()]);
    }

    #[test]
    fn the_faces_narrow_in_the_order_they_are_named() {
        let work = FacePolicy::work("markik");
        let research = FacePolicy::research("markik");
        let burner = FacePolicy::burner("markik", "uptime");

        assert!(research.attenuates(&work));
        assert!(burner.attenuates(&research));
        assert!(!work.attenuates(&research), "a face may not widen");

        // The burner carries exactly one action, so its certificates carry
        // no extensions at all.
        assert_eq!(burner.actions.len(), 1);
        assert!(burner.actions.contains(ACTION_SSH_LOGIN));
        assert!(burner.force_command.is_some());

        // Research keeps a terminal but lends no authority onward.
        assert!(!research.actions.contains(ACTION_SSH_AGENT_FORWARD));
        assert!(research.actions.contains(ACTION_SSH_PTY));
    }

    #[test]
    fn a_face_may_not_borrow_a_principal_it_was_not_given() {
        let work = FacePolicy::work("markik");
        let mut other = FacePolicy::research("markik");
        other.principals.push("root".into());
        assert!(!other.attenuates(&work));
    }
}
