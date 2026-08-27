// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The family-shared root: one identity for every Merely application.
//!
//! A persona is the point of sharing. Knot, Graphshell, Turnstone, Woodshed and
//! the rest are separate products, but they are the same person's, and a
//! document sealed on one should open on another. That only works if they agree
//! on where the wallet and the personas live, which they did not: each app
//! passed its own private data root, so `ensure_local_device_identity` minted a
//! different device identity per app and personas lived under whichever product
//! happened to create them.
//!
//! So the split is: **personas and the identity wallet are family-shared, and
//! everything else stays app-private.** An app keeps its own root for sessions,
//! graphs, and settings, and asks [`shared_root`] for identity.

use std::io;
use std::path::{Path, PathBuf};

use crate::engine_profile_store::PERSONAS_DIR;
use crate::wallet_store::IDENTITY_DIR;

/// The environment override, for scenario runs and for pointing a test at a
/// scratch profile rather than the real one.
pub const SHARED_ROOT_ENV: &str = "MERE_ROOT";

/// The directory family-shared state lives in, under the platform data dir.
const SHARED_DIR: &str = "mere";

/// Where personas and the identity wallet live for every application on this
/// machine.
///
/// `MERE_ROOT` wins when set. Otherwise the platform data directory plus
/// `mere`, and the current directory if the platform exposes no data dir, which
/// matches the posture the apps already take: a machine without one still runs,
/// it just keeps its state somewhere less tidy.
pub fn shared_root() -> PathBuf {
    if let Some(root) = std::env::var_os(SHARED_ROOT_ENV) {
        return PathBuf::from(root);
    }
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(SHARED_DIR)
}

/// Whether `root` already holds family identity state.
pub fn has_identity(root: &Path) -> bool {
    root.join(IDENTITY_DIR).is_dir() || root.join(PERSONAS_DIR).is_dir()
}

/// Move an application's private identity into the shared root, once.
///
/// Called on access rather than as a migration step, so an app that has not run
/// since the split still finds its own persona the first time it looks.
///
/// **Moves rather than copies, deliberately.** Two copies of an identity is the
/// worse failure: they diverge, and nothing can say which is authoritative. One
/// location has to win, so the legacy one is emptied.
///
/// Returns whether anything moved. Adopting nothing is the ordinary case and is
/// not an error: a fresh install has no legacy root, and an already-migrated one
/// has nothing left to take.
pub fn adopt_legacy_identity(shared: &Path, legacy: &Path) -> io::Result<bool> {
    // Never overwrite identity that is already shared. If both exist, the shared
    // root is authoritative and the legacy copy is left alone for a human to
    // look at, rather than merged by a rule nobody chose.
    if has_identity(shared) || !has_identity(legacy) {
        return Ok(false);
    }
    std::fs::create_dir_all(shared)?;
    let mut moved = false;
    for dir in [IDENTITY_DIR, PERSONAS_DIR] {
        let from = legacy.join(dir);
        if !from.is_dir() {
            continue;
        }
        let to = shared.join(dir);
        match std::fs::rename(&from, &to) {
            Ok(()) => moved = true,
            // A rename across filesystems fails; copy and remove instead. The
            // shared root and an app's root can easily be on different volumes.
            Err(_) => {
                copy_tree(&from, &to)?;
                std::fs::remove_dir_all(&from)?;
                moved = true;
            }
        }
    }
    Ok(moved)
}

fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed_identity(root: &Path, marker: &str) {
        std::fs::create_dir_all(root.join(IDENTITY_DIR)).unwrap();
        std::fs::write(root.join(IDENTITY_DIR).join("local-device.json"), marker).unwrap();
        std::fs::create_dir_all(root.join(PERSONAS_DIR).join("a-persona")).unwrap();
    }

    #[test]
    fn the_override_wins_so_a_scenario_can_use_a_scratch_profile() {
        // Set and read in one test rather than asserting the default, because
        // the default depends on the platform's data dir.
        let scratch = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var(SHARED_ROOT_ENV, scratch.path()) };
        assert_eq!(shared_root(), scratch.path());
        unsafe { std::env::remove_var(SHARED_ROOT_ENV) };
    }

    #[test]
    fn an_apps_identity_moves_to_the_shared_root_on_first_access() {
        let legacy = tempfile::tempdir().unwrap();
        let shared = tempfile::tempdir().unwrap();
        seed_identity(legacy.path(), "device-a");

        assert!(adopt_legacy_identity(shared.path(), legacy.path()).unwrap());
        assert_eq!(
            std::fs::read_to_string(shared.path().join(IDENTITY_DIR).join("local-device.json"))
                .unwrap(),
            "device-a"
        );
        assert!(shared.path().join(PERSONAS_DIR).join("a-persona").is_dir());
        assert!(
            !legacy.path().join(IDENTITY_DIR).exists(),
            "one location has to be authoritative, so the legacy copy is emptied"
        );
    }

    #[test]
    fn adopting_twice_is_a_no_op_rather_than_an_error() {
        let legacy = tempfile::tempdir().unwrap();
        let shared = tempfile::tempdir().unwrap();
        seed_identity(legacy.path(), "device-a");

        assert!(adopt_legacy_identity(shared.path(), legacy.path()).unwrap());
        assert!(!adopt_legacy_identity(shared.path(), legacy.path()).unwrap());
    }

    #[test]
    fn shared_identity_is_never_overwritten_by_a_legacy_one() {
        // The case that would silently destroy the newer identity: both roots
        // hold one. The shared root wins and the legacy copy is left for a human.
        let legacy = tempfile::tempdir().unwrap();
        let shared = tempfile::tempdir().unwrap();
        seed_identity(legacy.path(), "old");
        seed_identity(shared.path(), "current");

        assert!(!adopt_legacy_identity(shared.path(), legacy.path()).unwrap());
        assert_eq!(
            std::fs::read_to_string(shared.path().join(IDENTITY_DIR).join("local-device.json"))
                .unwrap(),
            "current"
        );
        assert!(
            legacy.path().join(IDENTITY_DIR).exists(),
            "the legacy copy stays put rather than being merged by a rule nobody chose"
        );
    }

    #[test]
    fn a_fresh_install_adopts_nothing_and_says_so() {
        let legacy = tempfile::tempdir().unwrap();
        let shared = tempfile::tempdir().unwrap();
        assert!(!adopt_legacy_identity(shared.path(), legacy.path()).unwrap());
    }
}
