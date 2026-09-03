// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Small, recoverable replacement for product-owned settings documents.
//!
//! This module deliberately shares only write mechanics. Each product keeps
//! its path, schema, validation, and load policy. The backup is retained until
//! Windows has accepted the replacement, so a failed second rename can restore
//! the previous document instead of leaving no settings file at all.

use std::fs;
use std::io;
use std::path::Path;

/// Fully write `contents` to a sibling temporary file, then replace `target`.
///
/// On platforms where rename cannot overwrite an existing destination, the
/// previous document moves to `<extension>.previous` until the temporary file
/// is installed. A stale backup from an interrupted earlier write is removed
/// only after a new target has been installed successfully.
pub fn write_bytes_with_backup(target: &Path, contents: &[u8]) -> io::Result<()> {
    let directory = target.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "settings target must have a parent directory",
        )
    })?;
    fs::create_dir_all(directory)?;

    let temporary = target.with_extension(format!(
        "{}.tmp",
        target
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
    ));
    let backup = target.with_extension(format!(
        "{}.previous",
        target
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
    ));
    fs::write(&temporary, contents)?;

    replace_temporary_with_backup(target, &temporary, &backup, |from, to| fs::rename(from, to))
}

fn replace_temporary_with_backup(
    target: &Path,
    temporary: &Path,
    backup: &Path,
    mut rename: impl FnMut(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    if !target.exists() {
        rename(temporary, target)?;
        if backup.exists() {
            fs::remove_file(backup)?;
        }
        return Ok(());
    }

    if backup.exists() {
        fs::remove_file(&backup)?;
    }
    rename(target, backup)?;
    match rename(temporary, target) {
        Ok(()) => {
            fs::remove_file(backup)?;
            Ok(())
        }
        Err(replacement_error) => {
            if let Err(restore_error) = rename(backup, target) {
                return Err(io::Error::other(format!(
                    "could not replace {target:?}: {replacement_error}; could not restore backup: {restore_error}"
                )));
            }
            Err(replacement_error)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn scratch(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "mere-atomic-file-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn a_second_write_replaces_the_document_and_cleans_up() {
        let directory = scratch("replace");
        let target = directory.join("settings.json");

        write_bytes_with_backup(&target, b"research").expect("first write");
        write_bytes_with_backup(&target, b"work").expect("replacement write");

        assert_eq!(fs::read(&target).expect("read target"), b"work");
        assert!(!target.with_extension("json.tmp").exists());
        assert!(!target.with_extension("json.previous").exists());
        fs::remove_dir_all(directory).expect("remove scratch");
    }

    #[test]
    fn a_completed_write_cleans_a_backup_left_by_an_interrupted_earlier_write() {
        let directory = scratch("recovery");
        let target = directory.join("settings.json");
        let backup = target.with_extension("json.previous");

        write_bytes_with_backup(&target, b"research").expect("first write");
        fs::rename(&target, &backup).expect("simulate interrupted replacement");
        write_bytes_with_backup(&target, b"work").expect("recovery write");

        assert_eq!(fs::read(&target).expect("read target"), b"work");
        assert!(!backup.exists());
        fs::remove_dir_all(directory).expect("remove scratch");
    }

    #[test]
    fn a_failed_replacement_restores_the_previous_document() {
        let directory = scratch("restore");
        let target = directory.join("settings.json");
        let temporary = target.with_extension("json.tmp");
        let backup = target.with_extension("json.previous");
        fs::create_dir_all(&directory).expect("create scratch");
        fs::write(&target, b"research").expect("write target");
        fs::write(&temporary, b"work").expect("write temporary");
        let mut rename_count = 0;

        let error = replace_temporary_with_backup(&target, &temporary, &backup, |from, to| {
            rename_count += 1;
            if rename_count == 2 {
                return Err(io::Error::other("simulated replacement failure"));
            }
            fs::rename(from, to)
        })
        .expect_err("replacement fails");

        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(fs::read(&target).expect("restored target"), b"research");
        assert!(!backup.exists());
        assert_eq!(fs::read(&temporary).expect("retained temporary"), b"work");
        fs::remove_dir_all(directory).expect("remove scratch");
    }
}
