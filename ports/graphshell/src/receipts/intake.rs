// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The resident host's side: pending receipts become one signed turn each.
//!
//! Ingest can happen anywhere — a CLI, a test, a future importer — but
//! **authoring belongs to the resident host alone**, because it holds the
//! signing identity and the log. So ingest deposits an events file in an
//! inbox and this picks it up. The file is the hand-off: readable JSON, so
//! an owner can see what is about to be authored on their behalf, and it
//! names the source directory because staging the capture bytes into the
//! replicating store is the host's job, not ingest's.
//!
//! Everything here is ordinary library code with tests rather than logic
//! reachable only by running the host, which is the same reason
//! `device_sync`'s settings and pairing work sits beside it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::personal_sync::PersonalGraphEvent;

/// Where the resident host looks for receipts waiting to be authored.
pub fn inbox_dir(data_root: &Path) -> PathBuf {
    data_root.join("receipts").join("inbox")
}

/// Where applied files are moved. Kept rather than deleted: it is the local
/// record of what this device authored, and it is what a person reads when
/// asking why a receipt did or did not arrive.
fn applied_dir(inbox: &Path) -> PathBuf {
    inbox.join("applied")
}

/// What a deposited receipt hands the host.
///
/// Carries the source directory as well as the events, because the bytes are
/// the host's job: ingest can compute a capture's hash anywhere, but only the
/// host can put it in the store that replicates. A receipt whose availability
/// facts named blobs the host never held would advertise captures no peer
/// could ever fetch.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InboxEntry {
    /// The receipt directory the artifacts can be read from.
    pub source: PathBuf,
    /// The events to author, in the order ingest produced them.
    pub events: Vec<PersonalGraphEvent>,
}

/// One receipt waiting to be authored.
#[derive(Clone, Debug)]
pub struct PendingReceipt {
    /// The file it came from.
    pub path: PathBuf,
    /// Where its artifact bytes live.
    pub source: PathBuf,
    /// The events to author, in the order ingest produced them.
    pub events: Vec<PersonalGraphEvent>,
}

/// The captures an entry's events refer to, as `(file name, blake3)`.
///
/// Read back out of the artifacts facet rather than passed alongside it, so
/// there is one statement of what a receipt's captures are and the host
/// stages exactly what the graph will claim.
pub fn captures_in(events: &[PersonalGraphEvent]) -> Vec<(String, [u8; 32])> {
    let mut found = Vec::new();
    for event in events {
        let PersonalGraphEvent::SetFacet { facet, value, .. } = event else {
            continue;
        };
        if facet != crate::receipts::FACET_ARTIFACTS {
            continue;
        }
        let Some(items) = value.as_array() else {
            continue;
        };
        for item in items {
            let (Some(name), Some(hex)) = (
                item.get("name").and_then(|v| v.as_str()),
                item.get("blake3").and_then(|v| v.as_str()),
            ) else {
                continue;
            };
            if let Some(hash) = parse_hex32(hex) {
                found.push((name.to_string(), hash));
            }
        }
    }
    found
}

fn parse_hex32(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (index, pair) in hex.as_bytes().chunks_exact(2).enumerate() {
        out[index] = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(out)
}

/// Deposit a receipt's events for the resident host to author.
///
/// Named by node id, so re-ingesting the same receipt overwrites its pending
/// file rather than queueing a second copy of the same facts.
pub fn write_to_inbox(
    inbox: &Path,
    node: uuid::Uuid,
    source: &Path,
    events: &[PersonalGraphEvent],
) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(inbox)?;
    let path = inbox.join(format!("{node}.json"));
    let entry = InboxEntry {
        source: source.to_path_buf(),
        events: events.to_vec(),
    };
    let json = serde_json::to_string_pretty(&entry).map_err(std::io::Error::other)?;
    std::fs::write(&path, json)?;
    Ok(path)
}

/// Every receipt waiting in `inbox`, oldest name first.
///
/// A file that does not parse is **skipped, not fatal**: one malformed
/// hand-off must not wedge the intake loop for every later receipt. The
/// caller logs it; the file stays put so it can be looked at.
pub fn pending(inbox: &Path) -> std::io::Result<Vec<PendingReceipt>> {
    if !inbox.exists() {
        return Ok(Vec::new());
    }
    let mut found = Vec::new();
    for entry in std::fs::read_dir(inbox)? {
        let path = entry?.path();
        // `applied/` is a directory and is skipped by the extension check.
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let text = std::fs::read_to_string(&path)?;
        match serde_json::from_str::<InboxEntry>(&text) {
            Ok(entry) => found.push(PendingReceipt {
                path,
                source: entry.source,
                events: entry.events,
            }),
            Err(_) => continue,
        }
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(found)
}

/// Move an authored file into `applied/`.
///
/// Re-authoring the same events would be harmless — the fold is
/// order-independent and every id in a receipt derives from content — but it
/// would append a fresh operation to the log every poll, so this is about
/// keeping the log honest rather than keeping the graph correct.
pub fn mark_applied(path: &Path) -> std::io::Result<()> {
    let Some(inbox) = path.parent() else {
        return Ok(());
    };
    let applied = applied_dir(inbox);
    std::fs::create_dir_all(&applied)?;
    let Some(name) = path.file_name() else {
        return Ok(());
    };
    // `rename` fails across devices and when the target exists on Windows;
    // a copy-then-remove is the portable form and this is a handful of KiB.
    let target = applied.join(name);
    std::fs::copy(path, &target)?;
    std::fs::remove_file(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn events() -> Vec<PersonalGraphEvent> {
        vec![PersonalGraphEvent::AddNode {
            id: Uuid::from_u128(1),
            address: "receipt:woodshed/thinkpad/x/2026".into(),
            title: "woodshed · frame on thinkpad · ok".into(),
        }]
    }

    #[test]
    fn an_absent_inbox_is_empty_rather_than_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let inbox = inbox_dir(dir.path());
        assert!(pending(&inbox).unwrap().is_empty());
    }

    #[test]
    fn a_deposited_receipt_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let inbox = inbox_dir(dir.path());
        let node = Uuid::from_u128(1);
        write_to_inbox(&inbox, node, dir.path(), &events()).unwrap();

        let found = pending(&inbox).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].events.len(), 1);
        assert!(found[0].path.ends_with(format!("{node}.json")));
    }

    #[test]
    fn re_depositing_the_same_receipt_does_not_queue_it_twice() {
        let dir = tempfile::tempdir().unwrap();
        let inbox = inbox_dir(dir.path());
        let node = Uuid::from_u128(1);
        write_to_inbox(&inbox, node, dir.path(), &events()).unwrap();
        write_to_inbox(&inbox, node, dir.path(), &events()).unwrap();
        assert_eq!(pending(&inbox).unwrap().len(), 1);
    }

    #[test]
    fn applying_clears_the_pending_file_and_keeps_the_record() {
        let dir = tempfile::tempdir().unwrap();
        let inbox = inbox_dir(dir.path());
        let path = write_to_inbox(&inbox, Uuid::from_u128(1), dir.path(), &events()).unwrap();

        mark_applied(&path).unwrap();

        assert!(pending(&inbox).unwrap().is_empty(), "no longer pending");
        assert!(!path.exists());
        assert!(
            applied_dir(&inbox).join(path.file_name().unwrap()).exists(),
            "the local record of what this device authored survives",
        );
    }

    /// One bad hand-off must not wedge every later receipt.
    #[test]
    fn a_malformed_file_is_skipped_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let inbox = inbox_dir(dir.path());
        write_to_inbox(&inbox, Uuid::from_u128(1), dir.path(), &events()).unwrap();
        std::fs::write(inbox.join("garbage.json"), "{not json").unwrap();

        let found = pending(&inbox).unwrap();
        assert_eq!(found.len(), 1, "the good one still comes through");
        assert!(
            inbox.join("garbage.json").exists(),
            "and the bad one stays put to be looked at",
        );
    }

    /// The `applied/` directory lives inside the inbox, so the scan must not
    /// mistake it for a receipt.
    #[test]
    fn the_applied_directory_is_not_scanned_as_a_receipt() {
        let dir = tempfile::tempdir().unwrap();
        let inbox = inbox_dir(dir.path());
        let path = write_to_inbox(&inbox, Uuid::from_u128(1), dir.path(), &events()).unwrap();
        mark_applied(&path).unwrap();
        write_to_inbox(&inbox, Uuid::from_u128(2), dir.path(), &events()).unwrap();

        assert_eq!(
            pending(&inbox).unwrap().len(),
            1,
            "only the new one is pending",
        );
    }
}
