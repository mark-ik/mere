/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The misfin server's durable mailbox, redb-backed.
//!
//! [`MailboxStore`] is where received mail lands. One redb file holds every
//! mailbox this server serves; messages are appended under a monotonic sequence
//! number and indexed per recipient mailbox so the inbox-read path
//! ([`MailboxStore::list`]) is a single multimap lookup. A separate sender table
//! records the first time each sender certificate fingerprint is seen — the seed
//! of trust-on-first-use (the spec says a sender's identity *is* its fingerprint).
//!
//! ## Schema
//!
//! - `messages`: `u64` (sequence) → `&[u8]` (JSON [`ReceivedMessage`])
//! - `mailbox_index`: multimap `&str` (recipient addr-spec) → `u64` (sequence)
//! - `meta`: `&str` (`"next_seq"`) → `u64` (the next sequence to allot)
//! - `senders`: `&str` (sender fingerprint) → `u64` (first-seen unix secs)

use std::path::Path;
use std::sync::Arc;

use redb::{Database, MultimapTableDefinition, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};

use crate::{MisfinAddress, MisfinSender, MisfinServerError};

const MESSAGES: TableDefinition<u64, &[u8]> = TableDefinition::new("messages");
const MAILBOX_INDEX: MultimapTableDefinition<&str, u64> =
    MultimapTableDefinition::new("mailbox_index");
const META: TableDefinition<&str, u64> = TableDefinition::new("meta");
const SENDERS: TableDefinition<&str, u64> = TableDefinition::new("senders");

const NEXT_SEQ_KEY: &str = "next_seq";

/// One delivered misfin message, as held in a mailbox and surfaced to the inbox.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceivedMessage {
    /// Monotonic per-store sequence number (also the delivery order).
    pub seq: u64,
    /// The recipient mailbox addr-spec (`mailbox@host`) this was addressed to.
    pub recipient: String,
    /// SHA-256 hex fingerprint of the sender's client certificate — the sender's
    /// misfin identity. Always present (the cert is required to deliver).
    pub sender_fingerprint: String,
    /// The sender's claimed addr-spec, if the server extracted it from the cert
    /// DN. v1 leaves this `None` (sender is tracked by fingerprint); a later pass
    /// parses the certificate's USER_ID + SAN to populate it.
    pub sender_address: Option<String>,
    /// The message body (gemtext), verbatim.
    pub body: String,
    /// Delivery time, unix seconds.
    pub received_at: u64,
}

/// Whether a sender fingerprint had been seen before this delivery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SenderSeen {
    /// First contact — the fingerprint was recorded just now.
    First,
    /// A fingerprint the store had already recorded.
    Known,
}

/// A redb-backed mailbox store for a misfin server. `Clone` shares the same
/// underlying database (cheap `Arc` clone), so a clone handed to a connection
/// task reads and writes the same file.
#[derive(Clone)]
pub struct MailboxStore {
    db: Arc<Database>,
}

fn storage_err(error: impl std::fmt::Display) -> MisfinServerError {
    MisfinServerError::Storage(error.to_string())
}

/// Touch every table so reads succeed before the first delivery.
fn init_tables(db: &Database) -> Result<(), MisfinServerError> {
    let txn = db.begin_write().map_err(storage_err)?;
    {
        txn.open_table(MESSAGES).map_err(storage_err)?;
        txn.open_multimap_table(MAILBOX_INDEX).map_err(storage_err)?;
        txn.open_table(META).map_err(storage_err)?;
        txn.open_table(SENDERS).map_err(storage_err)?;
    }
    txn.commit().map_err(storage_err)?;
    Ok(())
}

impl MailboxStore {
    /// Open or create the mailbox database at `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, MisfinServerError> {
        let db = Database::create(path).map_err(storage_err)?;
        init_tables(&db)?;
        Ok(Self { db: Arc::new(db) })
    }

    /// Open an in-memory mailbox (redb's in-memory backend). Nothing persists;
    /// used by tests and ephemeral servers.
    pub fn in_memory() -> Result<Self, MisfinServerError> {
        let db = Database::builder()
            .create_with_backend(redb::backends::InMemoryBackend::new())
            .map_err(storage_err)?;
        init_tables(&db)?;
        Ok(Self { db: Arc::new(db) })
    }

    /// Append a delivered message to `recipient`'s mailbox. Returns its sequence.
    pub fn store(
        &self,
        recipient: &MisfinAddress,
        sender_fingerprint: &str,
        sender: Option<&MisfinSender>,
        body: &str,
        received_at: u64,
    ) -> Result<u64, MisfinServerError> {
        let recipient_spec = recipient.as_addr_spec();
        let txn = self.db.begin_write().map_err(storage_err)?;
        let seq;
        {
            let mut meta = txn.open_table(META).map_err(storage_err)?;
            seq = meta
                .get(NEXT_SEQ_KEY)
                .map_err(storage_err)?
                .map(|value| value.value())
                .unwrap_or(0);
            meta.insert(NEXT_SEQ_KEY, &(seq + 1)).map_err(storage_err)?;

            let message = ReceivedMessage {
                seq,
                recipient: recipient_spec.clone(),
                sender_fingerprint: sender_fingerprint.to_string(),
                sender_address: sender.map(|sender| sender.address.as_addr_spec()),
                body: body.to_string(),
                received_at,
            };
            let bytes = serde_json::to_vec(&message).map_err(storage_err)?;

            let mut messages = txn.open_table(MESSAGES).map_err(storage_err)?;
            messages.insert(seq, bytes.as_slice()).map_err(storage_err)?;

            let mut index = txn.open_multimap_table(MAILBOX_INDEX).map_err(storage_err)?;
            index
                .insert(recipient_spec.as_str(), seq)
                .map_err(storage_err)?;
        }
        txn.commit().map_err(storage_err)?;
        Ok(seq)
    }

    /// Record that `fingerprint` sent mail at `now`, returning whether it was
    /// seen for the first time. The first-seen timestamp is never overwritten.
    pub fn record_sender(
        &self,
        fingerprint: &str,
        now: u64,
    ) -> Result<SenderSeen, MisfinServerError> {
        let txn = self.db.begin_write().map_err(storage_err)?;
        let seen;
        {
            let mut senders = txn.open_table(SENDERS).map_err(storage_err)?;
            if senders.get(fingerprint).map_err(storage_err)?.is_some() {
                seen = SenderSeen::Known;
            } else {
                senders.insert(fingerprint, &now).map_err(storage_err)?;
                seen = SenderSeen::First;
            }
        }
        txn.commit().map_err(storage_err)?;
        Ok(seen)
    }

    /// Every message addressed to `mailbox` (a `mailbox@host` addr-spec), in
    /// delivery order.
    pub fn list(&self, mailbox: &str) -> Result<Vec<ReceivedMessage>, MisfinServerError> {
        let txn = self.db.begin_read().map_err(storage_err)?;
        let index = txn.open_multimap_table(MAILBOX_INDEX).map_err(storage_err)?;
        let messages = txn.open_table(MESSAGES).map_err(storage_err)?;

        let mut out = Vec::new();
        for entry in index.get(mailbox).map_err(storage_err)? {
            let seq = entry.map_err(storage_err)?.value();
            if let Some(bytes) = messages.get(seq).map_err(storage_err)? {
                let message: ReceivedMessage =
                    serde_json::from_slice(bytes.value()).map_err(storage_err)?;
                out.push(message);
            }
        }
        out.sort_by_key(|message| message.seq);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(spec: &str) -> MisfinAddress {
        MisfinAddress::parse(spec).unwrap()
    }

    #[test]
    fn store_then_list_returns_messages_in_order() {
        let store = MailboxStore::in_memory().unwrap();
        let mark = addr("mark@example.test");
        store.store(&mark, "fp-a", None, "first", 100).unwrap();
        store.store(&mark, "fp-b", None, "second", 200).unwrap();

        let inbox = store.list("mark@example.test").unwrap();
        assert_eq!(inbox.len(), 2);
        assert_eq!(inbox[0].body, "first");
        assert_eq!(inbox[0].seq, 0);
        assert_eq!(inbox[0].sender_fingerprint, "fp-a");
        assert_eq!(inbox[1].body, "second");
        assert_eq!(inbox[1].seq, 1);
        assert_eq!(inbox[1].received_at, 200);
    }

    #[test]
    fn list_is_scoped_to_the_recipient_mailbox() {
        let store = MailboxStore::in_memory().unwrap();
        store
            .store(&addr("mark@example.test"), "fp", None, "for mark", 1)
            .unwrap();
        store
            .store(&addr("ana@example.test"), "fp", None, "for ana", 2)
            .unwrap();

        assert_eq!(store.list("mark@example.test").unwrap().len(), 1);
        assert_eq!(store.list("ana@example.test").unwrap()[0].body, "for ana");
        assert!(store.list("nobody@example.test").unwrap().is_empty());
    }

    #[test]
    fn record_sender_reports_first_then_known() {
        let store = MailboxStore::in_memory().unwrap();
        assert_eq!(store.record_sender("fp-1", 10).unwrap(), SenderSeen::First);
        assert_eq!(store.record_sender("fp-1", 20).unwrap(), SenderSeen::Known);
        assert_eq!(store.record_sender("fp-2", 30).unwrap(), SenderSeen::First);
    }
}
