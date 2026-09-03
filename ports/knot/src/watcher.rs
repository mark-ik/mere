// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Debounced OS watching under a revocable Servitor grant.

use std::path::Path;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use chartulary::{Container, EditSpec, GraphLog, Relation};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use personae::IdentityProvider;
use servitor::{AuthorityProvider, Cap, Gate, Grant, Mode, ScopePath, Subject};

const WATCH_SCOPE: &str = "watch";

struct RevocableAuthority {
    grant: Grant,
    enabled: bool,
}

impl AuthorityProvider for RevocableAuthority {
    fn covers(&self, subject: Subject, needed: &Cap, mode: Mode) -> bool {
        self.enabled && self.grant.covers(subject, needed, mode)
    }
}

/// An OS watcher whose event batches are journalled through Servitor before
/// the endpoint refreshes directory state.
pub struct DirectoryWatcher {
    _watcher: RecommendedWatcher,
    events: Receiver<notify::Result<Event>>,
    subject: Subject,
    scope: ScopePath,
    authority: RevocableAuthority,
    gate: Gate,
    audit: GraphLog<Container, Relation>,
    next_batch: u64,
}

impl DirectoryWatcher {
    /// Watch `root` recursively under a key derived from the host identity.
    pub fn new(root: &Path, identity: &impl IdentityProvider) -> Result<Self, String> {
        let salt = format!("knot/directory-watcher/{}", root.to_string_lossy());
        let key = identity
            .derive_keypair(salt.as_bytes())
            .map_err(|error| format!("could not derive Knot watcher identity: {error:?}"))?;
        let subject = Subject::new(key.public_key().to_bytes());
        let scope = ScopePath::parse(WATCH_SCOPE)
            .map_err(|error| format!("invalid Knot watcher scope: {error:?}"))?;
        let grant = Grant::new(subject, Cap::Scope(scope.clone()), Mode::Write);
        let authority = RevocableAuthority {
            grant: grant.clone(),
            enabled: true,
        };
        let gate = Gate::new();
        let mut audit = GraphLog::new();
        gate.project_grant(&mut audit, &grant)
            .map_err(|error| format!("could not project Knot watcher grant: {error:?}"))?;

        let (sender, events) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            let _ = sender.send(event);
        })
        .map_err(|error| format!("could not create Knot directory watcher: {error}"))?;
        watcher
            .watch(root, RecursiveMode::Recursive)
            .map_err(|error| format!("could not watch {}: {error}", root.display()))?;

        Ok(Self {
            _watcher: watcher,
            events,
            subject,
            scope,
            authority,
            gate,
            audit,
            next_batch: 0,
        })
    }

    /// The keyholder identity attributed to watcher journal transitions.
    pub fn subject(&self) -> Subject {
        self.subject
    }

    /// Whether the watcher currently holds its observation grant.
    pub fn is_enabled(&self) -> bool {
        self.authority.enabled
    }

    /// Revoke observation without stopping the endpoint or OS watcher.
    pub fn revoke(&mut self) {
        self.authority.enabled = false;
    }

    /// Restore the watcher grant.
    pub fn grant(&mut self) {
        self.authority.enabled = true;
    }

    /// Attributed watcher transitions, including the gate-authored grant
    /// projection at revision one.
    pub fn audit(&self) -> &GraphLog<Container, Relation> {
        &self.audit
    }

    /// Drain every queued OS event into at most one attributed journal batch.
    ///
    /// The returned count is the number of raw events collapsed. Errors from
    /// the watcher are surfaced before any transition is committed.
    pub fn drain(&mut self) -> Result<usize, String> {
        let mut count = 0usize;
        loop {
            match self.events.try_recv() {
                Ok(Ok(_event)) => count += 1,
                Ok(Err(error)) => {
                    return Err(format!("Knot directory watch failed: {error}"));
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        if count == 0 || !self.authority.enabled {
            return Ok(0);
        }
        self.record_batch(count)?;
        Ok(count)
    }

    fn record_batch(&mut self, count: usize) -> Result<(), String> {
        let id = format!("{WATCH_SCOPE}/events/{}", self.next_batch);
        self.next_batch = self.next_batch.saturating_add(1);
        let node = Container::new(id)
            .with_title(format!("{count} filesystem events"))
            .with_tag("knot.watcher")
            .with_tag(format!("event-count:{count}"));
        let expected = self.audit.revision();
        self.gate
            .petition(
                &self.authority,
                &mut self.audit,
                self.subject,
                &self.scope,
                expected,
                vec![EditSpec::InsertNode(node)],
            )
            .map_err(|error| format!("Knot watcher petition failed: {error:?}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::thread;
    use std::time::{Duration, Instant};

    use personae::InMemoryProvider;
    use tempfile::tempdir;

    use super::*;

    fn wait_for_transition(watcher: &mut DirectoryWatcher, after: u64) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            watcher.drain().unwrap();
            if watcher.audit().revision() > after {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        panic!("filesystem event did not reach the Knot watcher");
    }

    #[test]
    fn os_events_commit_once_under_the_watcher_subject() {
        let temp = tempdir().unwrap();
        let identity = InMemoryProvider::from_seed([0x51; 32]);
        let mut watcher = DirectoryWatcher::new(temp.path(), &identity).unwrap();
        let before = watcher.audit().revision();

        fs::write(temp.path().join("field.knot"), "one").unwrap();
        wait_for_transition(&mut watcher, before);

        assert_eq!(watcher.audit().revision(), before + 1);
        let batch = watcher.audit().log().entries().last().unwrap();
        assert_eq!(batch.author, watcher.subject().to_author());
    }

    #[test]
    fn revocation_discards_events_without_stopping_the_watcher() {
        let temp = tempdir().unwrap();
        let identity = InMemoryProvider::from_seed([0x52; 32]);
        let mut watcher = DirectoryWatcher::new(temp.path(), &identity).unwrap();
        let before = watcher.audit().revision();
        watcher.revoke();

        fs::write(temp.path().join("paused.knot"), "paused").unwrap();
        thread::sleep(Duration::from_millis(100));
        assert_eq!(watcher.drain().unwrap(), 0);
        assert_eq!(watcher.audit().revision(), before);

        watcher.grant();
        fs::write(temp.path().join("resumed.knot"), "resumed").unwrap();
        wait_for_transition(&mut watcher, before);
        assert_eq!(watcher.audit().revision(), before + 1);
    }
}
