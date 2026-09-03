// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Which persona: the vault's roster, the remembered choice, and opening it.
//!
//! [`crate::bootstrap`] opens a vault and loads a profile you name. Naming one
//! was the part every application did for itself, and every one of them named
//! `"default"` — so a vault holding a work persona beside a personal one had no
//! way to say which was in use, and nothing could switch between them. This
//! module is the missing half: list what the vault holds, remember which one
//! was picked, open that one.
//!
//! **The choice lives beside the vault, not in an application's data
//! directory.** That is the whole point of a shared vault: picking a persona in
//! one Merely application picks it in all of them, so a document sealed in one
//! opens in the next. An application that needs a different persona anyway sets
//! [`PROFILE_ENV`], which is also how a scenario run points at a scratch one.
//!
//! The choice is a profile id in a plain text file. Nothing secret is in it —
//! the names of the personas are already visible to anyone who can list the
//! vault directory, and the secrets stay sealed in the records themselves.

use std::path::{Path, PathBuf};

use crate::bootstrap::{self, Unlock};
use crate::vault::{IdentityStorage, IdentityVault, Profile, ProfileId};
use crate::{Ed25519Keypair, IdentityError};

/// Environment variable naming the profile to use, overriding the remembered
/// choice.
pub const PROFILE_ENV: &str = "PERSONAE_PROFILE";

/// The profile id used when a vault has nothing to go on.
pub const DEFAULT_PROFILE: &str = "default";

/// The file beside the vault holding the chosen profile id.
const CHOICE_FILENAME: &str = "chosen-profile";

/// One persona in the vault, as a picker needs to show it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RosterEntry {
    /// Stable id, and what [`remember_profile`] writes.
    pub id: ProfileId,
    /// The name to show.
    pub display_name: String,
    /// How many protocol slots this persona carries. Shown because it is the
    /// one honest signal of which persona is the used one when the display
    /// names are not telling.
    pub slot_count: usize,
    /// Whether this is the persona currently in use.
    pub chosen: bool,
}

/// Everything a persona picker needs from the vault.
#[derive(Clone, Debug)]
pub struct Roster {
    /// The vault's personas, sorted by id so the list does not reorder itself
    /// between runs.
    pub entries: Vec<RosterEntry>,
    /// The persona in use, whether or not it exists yet: a fresh vault resolves
    /// to a name that will be minted on first open.
    pub chosen: ProfileId,
    /// What protects the vault, from [`crate::bootstrap::OpenedStorage`].
    /// Shown, never guessed.
    pub description: String,
}

impl Roster {
    /// The entry for the persona in use, absent on a vault that has none yet.
    pub fn chosen_entry(&self) -> Option<&RosterEntry> {
        self.entries.iter().find(|entry| entry.chosen)
    }

    /// Whether the vault holds no personas at all, which is a first run.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// A vault opened on a particular persona.
pub struct OpenedVault {
    /// The vault, loaded on [`Self::profile`].
    pub vault: IdentityVault<Box<dyn IdentityStorage>>,
    /// What protects it.
    pub description: String,
    /// Which persona was opened.
    pub profile: ProfileId,
    /// Whether the persona was minted just now rather than loaded, so a caller
    /// can say so instead of silently creating an identity.
    pub created: bool,
}

/// Where the remembered choice is written, for a caller that wants to say so.
pub fn choice_path(vault_dir: &Path) -> PathBuf {
    vault_dir.join(CHOICE_FILENAME)
}

/// The explicitly chosen persona: [`PROFILE_ENV`] first, then the remembered
/// choice.
///
/// `None` means nobody has chosen — which is not the same as choosing
/// [`DEFAULT_PROFILE`], and is why this returns an option rather than falling
/// back. [`resolve_profile`] is the one that decides what an unchosen vault
/// opens.
pub fn chosen_profile(vault_dir: &Path) -> Option<ProfileId> {
    if let Some(name) = std::env::var_os(PROFILE_ENV) {
        let name = name.to_string_lossy().trim().to_string();
        if !name.is_empty() {
            return Some(ProfileId(name));
        }
    }
    let remembered = std::fs::read_to_string(choice_path(vault_dir)).ok()?;
    let remembered = remembered.trim();
    (!remembered.is_empty()).then(|| ProfileId(remembered.to_string()))
}

/// Which persona this vault opens on, applying the full ladder:
///
/// 1. An explicit choice ([`PROFILE_ENV`] or the remembered file).
/// 2. The vault's sole persona, when it holds exactly one. Falling through to
///    `"default"` here would mint a second identity beside the only one the
///    user has, which is the failure worth spending a storage read to avoid.
/// 3. [`DEFAULT_PROFILE`], minted on open if absent.
pub fn resolve_profile(
    storage: &dyn IdentityStorage,
    vault_dir: &Path,
) -> Result<ProfileId, IdentityError> {
    if let Some(chosen) = chosen_profile(vault_dir) {
        return Ok(chosen);
    }
    let summaries = storage.list_profiles()?;
    if let [only] = summaries.as_slice() {
        return Ok(only.id.clone());
    }
    Ok(ProfileId(DEFAULT_PROFILE.to_string()))
}

/// Write the chosen persona, so every application opens it next time.
pub fn remember_profile(vault_dir: &Path, id: &ProfileId) -> Result<(), IdentityError> {
    std::fs::create_dir_all(vault_dir)
        .and_then(|()| std::fs::write(choice_path(vault_dir), &id.0))
        .map_err(|err| IdentityError::Backend(format!("remember profile {:?}: {err}", id.0)))
}

/// Read the vault's roster.
///
/// `description` comes from the [`crate::bootstrap::OpenedStorage`] the caller
/// already has, rather than being re-derived here: what protects the vault is
/// something the backend reports, not something this module can work out.
pub fn read_roster(
    storage: &dyn IdentityStorage,
    vault_dir: &Path,
    description: impl Into<String>,
) -> Result<Roster, IdentityError> {
    let chosen = resolve_profile(storage, vault_dir)?;
    let mut summaries = storage.list_profiles()?;
    summaries.sort_by(|a, b| a.id.cmp(&b.id));
    let entries = summaries
        .into_iter()
        .map(|summary| RosterEntry {
            chosen: summary.id == chosen,
            id: summary.id,
            display_name: summary.display_name,
            slot_count: summary.slot_count,
        })
        .collect();
    Ok(Roster {
        entries,
        chosen,
        description: description.into(),
    })
}

/// Mint a persona with its own display name.
///
/// [`crate::bootstrap::load_or_create_profile`] names a new profile after its
/// id, which is right for `"default"` and wrong for one a person named. Fails
/// rather than overwriting if the id is taken: minting over an existing persona
/// would replace its master key, and every certificate rooted on it.
pub fn create_profile(
    storage: &dyn IdentityStorage,
    id: &ProfileId,
    display_name: impl Into<String>,
) -> Result<Profile, IdentityError> {
    import_profile(storage, id, display_name, Ed25519Keypair::generate())
}

/// Adopt an identity an application already holds, as a persona, keeping its
/// key.
///
/// The migration primitive for an application that minted its own identity
/// before the shared vault existed. **The master key is carried over
/// unchanged**, which is the whole point: an application's durable public key
/// is frequently already in the world — pasted to a peer as a contact token,
/// naming the signer of envelopes it sent — and minting a fresh one would
/// silently turn its user into a different person. Hocket is the first caller;
/// its own pre-vault rename took the same care for the same reason.
///
/// Refuses an id that is taken, exactly like [`create_profile`], and here the
/// refusal is load-bearing rather than tidy: the taken profile is somebody's
/// real persona, and overwriting it would destroy every certificate rooted on
/// it. Callers adopt into a free id or leave the vault alone.
pub fn import_profile(
    storage: &dyn IdentityStorage,
    id: &ProfileId,
    display_name: impl Into<String>,
    master: Ed25519Keypair,
) -> Result<Profile, IdentityError> {
    if storage.list_profiles()?.iter().any(|s| &s.id == id) {
        return Err(IdentityError::Backend(format!(
            "persona {:?} already exists",
            id.0
        )));
    }
    let profile = Profile::new(id.clone(), display_name, master);
    storage.save_profile(&profile)?;
    Ok(profile)
}

/// Open the vault at `dir` on a named persona, minting it when absent.
///
/// [`open_chosen`] is this with the convention ladder in front of it. An
/// application whose user has just picked a persona calls this one, so the
/// choice takes effect on its own rather than by way of the remembered file:
/// a vault directory that cannot be written to would otherwise silently open
/// somebody else.
pub fn open_profile(
    dir: &Path,
    unlock: Unlock,
    profile_id: &ProfileId,
) -> Result<OpenedVault, IdentityError> {
    vault_on(bootstrap::open_storage(dir, unlock)?, profile_id)
}

/// Open the vault at `dir` on whichever persona [`resolve_profile`] picks,
/// minting it when absent.
pub fn open_chosen(dir: &Path, unlock: Unlock) -> Result<OpenedVault, IdentityError> {
    let opened = bootstrap::open_storage(dir, unlock)?;
    let profile_id = resolve_profile(&*opened.storage, dir)?;
    vault_on(opened, &profile_id)
}

/// Load (or mint) `profile_id` in an already-open storage and hand back the
/// vault on it. Shared by both open paths so unlocking happens exactly once:
/// a passphrase vault pays for Argon2 on open, and paying twice to name a
/// profile would be felt.
fn vault_on(
    opened: bootstrap::OpenedStorage,
    profile_id: &ProfileId,
) -> Result<OpenedVault, IdentityError> {
    let (profile, created) = bootstrap::load_or_create_profile(&*opened.storage, profile_id)?;
    Ok(OpenedVault {
        vault: IdentityVault::with_profile(opened.storage, profile),
        description: opened.description,
        profile: profile_id.clone(),
        created,
    })
}

/// Open the family-shared vault on the chosen persona: the one call an
/// application makes to get the user's identity rather than its own.
pub fn open_shared(unlock: Unlock) -> Result<OpenedVault, IdentityError> {
    open_chosen(&bootstrap::default_vault_dir(), unlock)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IdentityProvider;
    use crate::vault::InMemoryStorage;
    use tempfile::tempdir;

    /// `PERSONAE_PROFILE` is process-wide, so the tests that touch it are
    /// serialized behind one lock rather than left to race each other.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn seeded(names: &[&str]) -> InMemoryStorage {
        let storage = InMemoryStorage::new();
        for name in names {
            create_profile(&storage, &ProfileId((*name).into()), *name).unwrap();
        }
        storage
    }

    #[test]
    fn an_unchosen_vault_with_one_persona_opens_that_one() {
        // The trap this rule exists for: falling through to "default" would
        // mint a second identity beside the only persona the user has.
        let _guard = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
        let dir = tempdir().unwrap();
        let storage = seeded(&["stage-name"]);
        assert_eq!(
            resolve_profile(&storage, dir.path()).unwrap(),
            ProfileId("stage-name".into())
        );
    }

    #[test]
    fn an_unchosen_vault_with_several_personas_falls_back_to_default() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
        let dir = tempdir().unwrap();
        let storage = seeded(&["work", "personal"]);
        assert_eq!(
            resolve_profile(&storage, dir.path()).unwrap(),
            ProfileId(DEFAULT_PROFILE.into())
        );
    }

    #[test]
    fn a_remembered_choice_survives_and_is_what_every_app_reads() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
        let dir = tempdir().unwrap();
        let storage = seeded(&["work", "personal"]);
        remember_profile(dir.path(), &ProfileId("personal".into())).unwrap();
        assert_eq!(
            resolve_profile(&storage, dir.path()).unwrap(),
            ProfileId("personal".into())
        );
    }

    #[test]
    fn the_env_override_beats_the_remembered_choice() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
        let dir = tempdir().unwrap();
        remember_profile(dir.path(), &ProfileId("personal".into())).unwrap();
        unsafe { std::env::set_var(PROFILE_ENV, "scratch") };
        let chosen = chosen_profile(dir.path());
        unsafe { std::env::remove_var(PROFILE_ENV) };
        assert_eq!(chosen, Some(ProfileId("scratch".into())));
    }

    #[test]
    fn nothing_chosen_reads_as_nothing_rather_than_as_default() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
        let dir = tempdir().unwrap();
        assert_eq!(chosen_profile(dir.path()), None);
    }

    #[test]
    fn the_roster_marks_the_chosen_persona_and_sorts_stably() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
        let dir = tempdir().unwrap();
        let storage = seeded(&["work", "alt", "personal"]);
        remember_profile(dir.path(), &ProfileId("personal".into())).unwrap();

        let roster = read_roster(&storage, dir.path(), "test storage").unwrap();
        let ids: Vec<&str> = roster.entries.iter().map(|e| e.id.0.as_str()).collect();
        assert_eq!(ids, ["alt", "personal", "work"]);
        assert_eq!(
            roster.chosen_entry().map(|e| e.id.0.as_str()),
            Some("personal")
        );
    }

    #[test]
    fn a_fresh_vault_has_an_empty_roster_but_still_names_what_it_would_open() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
        let dir = tempdir().unwrap();
        let roster = read_roster(&InMemoryStorage::new(), dir.path(), "test storage").unwrap();
        assert!(roster.is_empty());
        assert!(roster.chosen_entry().is_none());
        assert_eq!(roster.chosen, ProfileId(DEFAULT_PROFILE.into()));
    }

    #[test]
    fn an_adopted_identity_keeps_the_key_it_arrived_with() {
        // The property the migration rests on: an application's durable public
        // key is already in the world, so adoption must not mint a new one.
        let storage = InMemoryStorage::new();
        let existing = Ed25519Keypair::from_seed([9u8; 32]);
        let before = existing.public_key().to_bytes();

        import_profile(&storage, &ProfileId("mine".into()), "Mine", existing).unwrap();

        let loaded = storage.load_profile(&ProfileId("mine".into())).unwrap();
        assert_eq!(
            loaded.master.public_key().to_bytes(),
            before,
            "the fingerprint a peer already has still resolves"
        );
    }

    #[test]
    fn adopting_over_an_existing_persona_is_refused_and_changes_nothing() {
        // Here the refusal is load-bearing: the taken profile is somebody's
        // real persona, and overwriting it destroys every certificate rooted
        // on it.
        let storage = seeded(&["work"]);
        let before = storage
            .load_profile(&ProfileId("work".into()))
            .unwrap()
            .master
            .public_key()
            .to_bytes();

        assert!(
            import_profile(
                &storage,
                &ProfileId("work".into()),
                "Imported",
                Ed25519Keypair::from_seed([7u8; 32]),
            )
            .is_err()
        );
        assert_eq!(
            storage
                .load_profile(&ProfileId("work".into()))
                .unwrap()
                .master
                .public_key()
                .to_bytes(),
            before
        );
    }

    #[test]
    fn minting_over_an_existing_persona_is_refused() {
        // It would replace the master key, and with it every certificate
        // rooted on that persona.
        let storage = seeded(&["work"]);
        let before = storage
            .load_profile(&ProfileId("work".into()))
            .unwrap()
            .master
            .public_key()
            .to_bytes();
        assert!(create_profile(&storage, &ProfileId("work".into()), "Work again").is_err());
        let after = storage
            .load_profile(&ProfileId("work".into()))
            .unwrap()
            .master
            .public_key()
            .to_bytes();
        assert_eq!(before, after);
    }

    #[test]
    fn a_created_persona_keeps_the_name_it_was_given() {
        let storage = InMemoryStorage::new();
        create_profile(&storage, &ProfileId("alt".into()), "Late Night Alt").unwrap();
        let summaries = storage.list_profiles().unwrap();
        assert_eq!(summaries[0].display_name, "Late Night Alt");
    }

    #[test]
    fn switching_personas_switches_the_derived_key() {
        // The property the whole lane exists for: two personas seal to
        // different keys, so choosing is what decides whose documents open.
        let storage = seeded(&["work", "personal"]);
        let work = IdentityVault::open(&storage, &ProfileId("work".into())).unwrap();
        let personal = IdentityVault::open(&storage, &ProfileId("personal".into())).unwrap();
        assert_ne!(
            work.derive_keypair(b"woodshed.session").unwrap().to_seed(),
            personal
                .derive_keypair(b"woodshed.session")
                .unwrap()
                .to_seed()
        );
    }
}
