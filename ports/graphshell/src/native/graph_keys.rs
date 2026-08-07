//! This device's membership in one personal graph's key group.
//!
//! The lane carries key agreement; this holds the secret state that reads it.
//! Session state contains long-term private keys and ratchet state, so it is
//! written only through Personae's sealed record store, never to plaintext
//! storage.
//!
//! Creating the group is an explicit act, not something a device does because
//! it noticed it could. Two devices creating independently would produce two
//! groups on one graph, each unable to read the other, and nothing on the lane
//! would say which was meant. So one device turns encryption on and adds the
//! others as their pre-keys arrive; every other device publishes a pre-key and
//! waits.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use personae::{IdentityProvider, SealedRecordStorage};
use stickleback::{
    DataKeyring, GroupPrekeyBundle, GroupRecipientId, GroupSession, GroupSessionDispatch,
    GroupSessionId,
};

use crate::personal_sync::{KeyAgreementEvent, KeyAgreementStep, PersonalGraphEvent};

/// Domain for the key that protects session state at rest.
///
/// Distinct from the group's own keys: this one only stops the file being
/// readable off the disk, and never leaves this device.
const SESSION_STORAGE_CONTEXT: &str = "graphshell.personal-graph.group-session.storage.v1";

/// Domain for the per-graph derivation that names this device's session.
const SESSION_IDENTITY_CONTEXT: &[u8] = b"graphshell.personal-graph.group-session/v1/";

#[derive(Debug, thiserror::Error)]
pub enum GraphKeyError {
    #[error(transparent)]
    Identity(#[from] personae::IdentityError),
    #[error("group session failed: {0}")]
    Session(String),
    #[error("this device is not yet a member of the graph's key group")]
    NotAMember,
    #[error("the graph's key group already exists on this device")]
    AlreadyCreated,
}

impl From<stickleback::GroupSessionError> for GraphKeyError {
    fn from(error: stickleback::GroupSessionError) -> Self {
        Self::Session(error.to_string())
    }
}

/// What opening produced, and what the caller must publish because of it.
pub struct OpenedKeyGroup {
    pub group: GraphKeyGroup,
    /// Present when this device's session was created just now. It must reach
    /// the lane before any member can add this device, so a caller that drops
    /// it leaves this device permanently unkeyable.
    pub publish: Option<GroupPrekeyBundle>,
}

/// One device's seat in one graph's key group.
pub struct GraphKeyGroup {
    session: GroupSession,
    storage: SealedRecordStorage,
    record: PathBuf,
}

impl GraphKeyGroup {
    /// Open this device's session for `graph`, creating it on first use.
    ///
    /// Creating a *session* is not creating the *group*: a fresh session is a
    /// device holding its own keys and no membership. It can read nothing
    /// until somebody adds it, which is the correct starting state.
    pub fn open(
        identity: &dyn IdentityProvider,
        graph: [u8; 32],
        root: &Path,
    ) -> Result<OpenedKeyGroup, GraphKeyError> {
        let storage = SealedRecordStorage::open_with_key(root, storage_key(identity, graph)?);
        let record =
            PathBuf::from("graphshell/group-sessions").join(format!("{}.session", hex(&graph)));
        if let Some(bytes) = storage.load_record::<Vec<u8>>(&record)? {
            return Ok(OpenedKeyGroup {
                group: Self {
                    session: GroupSession::from_bytes(&bytes)?,
                    storage,
                    record,
                },
                publish: None,
            });
        }
        let (session, bundle) = GroupSession::new(GroupSessionId(graph), identity)?;
        let group = Self {
            session,
            storage,
            record,
        };
        group.persist()?;
        Ok(OpenedKeyGroup {
            group,
            publish: Some(bundle),
        })
    }

    /// This device's recipient id, which is what another member adds.
    pub fn member(&self) -> GroupRecipientId {
        self.session.member()
    }

    /// Whether this device can read sealed operations yet.
    pub fn is_keyed(&self) -> bool {
        self.session.current_epoch().is_some()
    }

    /// The keyring to hand the replica, or `None` while unkeyed.
    ///
    /// Absent rather than empty on purpose: an empty keyring would seal
    /// operations nobody could open, including this device.
    pub fn keyring(&self) -> Result<Option<Arc<DataKeyring>>, GraphKeyError> {
        if !self.is_keyed() {
            return Ok(None);
        }
        let state = self.session.data_keyring_state()?;
        Ok(Some(Arc::new(DataKeyring::from_bytes(&state).map_err(
            |error| GraphKeyError::Session(error.to_string()),
        )?)))
    }

    /// Turn encryption on for this graph, with this device as first member.
    pub fn create(&mut self) -> Result<PersonalGraphEvent, GraphKeyError> {
        if !self.session.members()?.is_empty() {
            return Err(GraphKeyError::AlreadyCreated);
        }
        let dispatch = self.session.create(&[self.session.member()])?;
        self.persist()?;
        dispatch_event(&dispatch)
    }

    /// Admit a device whose pre-key this one has already registered.
    pub fn add(&mut self, member: GroupRecipientId) -> Result<PersonalGraphEvent, GraphKeyError> {
        if !self.is_keyed() {
            return Err(GraphKeyError::NotAMember);
        }
        let dispatch = self.session.add(member)?;
        self.persist()?;
        dispatch_event(&dispatch)
    }

    /// Remove a device and turn the epoch in one gesture.
    ///
    /// Two events, because removal alone leaves the departed device able to
    /// read everything written afterwards. It keeps what it could already
    /// read; no scheme can take that back, and implying otherwise would be
    /// worse than saying so.
    pub fn remove_and_rotate(
        &mut self,
        member: GroupRecipientId,
    ) -> Result<Vec<PersonalGraphEvent>, GraphKeyError> {
        if !self.is_keyed() {
            return Err(GraphKeyError::NotAMember);
        }
        let removed = self.session.remove(member)?;
        let rotated = self.session.update()?;
        self.persist()?;
        Ok(vec![dispatch_event(&removed)?, dispatch_event(&rotated)?])
    }

    /// Take everything the lane has said about keys, in the order it said it.
    ///
    /// Steps that do not concern this device, or that it has already seen, are
    /// skipped rather than treated as errors: replication means seeing the
    /// same step again is ordinary, and a step addressed elsewhere is most of
    /// the traffic.
    pub fn absorb(&mut self, steps: &[KeyAgreementStep]) -> Result<AbsorbReport, GraphKeyError> {
        let mut report = AbsorbReport::default();
        for step in steps {
            match &step.step {
                KeyAgreementEvent::Prekey(bundle) => {
                    let Ok(bundle) = GroupPrekeyBundle::from_bytes(bundle) else {
                        report.unreadable += 1;
                        continue;
                    };
                    if bundle.recipient == self.session.member() {
                        continue;
                    }
                    match self.session.register_prekey(&bundle) {
                        Ok(()) => report.registered.push(bundle.recipient),
                        Err(_) => report.unreadable += 1,
                    }
                }
                KeyAgreementEvent::Dispatch(bytes) => {
                    let Ok(dispatch) = decode_dispatch(bytes) else {
                        report.unreadable += 1;
                        continue;
                    };
                    let direct = dispatch.direct_for(self.session.member());
                    match self
                        .session
                        .process(step.author_root, &dispatch.control, direct)
                    {
                        Ok(processed) => report.installed += processed.installed_epochs.len(),
                        Err(_) => report.skipped += 1,
                    }
                }
            }
        }
        self.persist()?;
        Ok(report)
    }

    fn persist(&self) -> Result<(), GraphKeyError> {
        let bytes = self.session.to_bytes()?;
        self.storage.save_record(&self.record, &bytes)?;
        Ok(())
    }
}

/// What one pass over the lane's key traffic changed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AbsorbReport {
    /// Devices whose pre-keys this one can now add.
    pub registered: Vec<GroupRecipientId>,
    /// Epochs this device learned, which is how it becomes able to read.
    pub installed: usize,
    /// Steps that did not apply here: already seen, or addressed elsewhere.
    pub skipped: usize,
    /// Steps that would not decode. Counted rather than fatal, so one bad
    /// frame from one device cannot stop this one being keyed.
    pub unreadable: usize,
}

fn dispatch_event(dispatch: &GroupSessionDispatch) -> Result<PersonalGraphEvent, GraphKeyError> {
    let bytes = p2panda_core::cbor::encode_cbor(dispatch)
        .map_err(|error| GraphKeyError::Session(error.to_string()))?;
    Ok(PersonalGraphEvent::GroupDispatch { dispatch: bytes })
}

fn decode_dispatch(bytes: &[u8]) -> Result<GroupSessionDispatch, GraphKeyError> {
    p2panda_core::cbor::decode_cbor(bytes)
        .map_err(|error| GraphKeyError::Session(error.to_string()))
}

/// A key that protects session state on this disk and nowhere else.
fn storage_key(
    identity: &dyn IdentityProvider,
    graph: [u8; 32],
) -> Result<[u8; 32], GraphKeyError> {
    let mut salt = Vec::with_capacity(SESSION_IDENTITY_CONTEXT.len() + 32);
    salt.extend_from_slice(SESSION_IDENTITY_CONTEXT);
    salt.extend_from_slice(&graph);
    let keypair = identity.derive_keypair(&salt)?;
    Ok(blake3::derive_key(
        SESSION_STORAGE_CONTEXT,
        &keypair.to_seed(),
    ))
}

fn hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use personae::InMemoryProvider;

    const GRAPH: [u8; 32] = [0x9c; 32];

    fn open_device(seed: u8, dir: &Path) -> (InMemoryProvider, OpenedKeyGroup) {
        let identity = InMemoryProvider::from_seed([seed; 32]);
        let opened = GraphKeyGroup::open(&identity, GRAPH, dir).unwrap();
        (identity, opened)
    }

    /// One device turns encryption on and keys another, using only what the
    /// lane can carry. Nothing here is handed between devices directly: every
    /// step is an event, exactly as it would replicate.
    #[test]
    fn a_second_device_becomes_readable_through_lane_events_alone() {
        let owner_dir = tempfile::tempdir().unwrap();
        let sibling_dir = tempfile::tempdir().unwrap();
        let (_owner_identity, owner_opened) = open_device(0x9d, owner_dir.path());
        let (_sibling_identity, sibling_opened) = open_device(0x9e, sibling_dir.path());
        let mut owner = owner_opened.group;
        let mut sibling = sibling_opened.group;

        // A fresh session holds keys and no membership, so it can read
        // nothing. That is the correct starting state, not a failure.
        assert!(!owner.is_keyed());
        assert!(!sibling.is_keyed());
        assert!(owner.keyring().unwrap().is_none());

        let sibling_prekey = sibling_opened
            .publish
            .expect("a new session must publish a pre-key or it can never be added");
        let owner_prekey = owner_opened
            .publish
            .expect("the creator publishes too: a member cannot process control frames from a                      device whose pre-key it has never registered");

        // The owner turns encryption on.
        let created = owner.create().unwrap();
        assert!(
            owner.is_keyed(),
            "the creator can read from the moment it creates"
        );
        assert!(owner.keyring().unwrap().is_some());

        // The sibling's pre-key reaches the owner as a lane event.
        let published = PersonalGraphEvent::PublishPrekey {
            bundle: sibling_prekey.to_bytes().unwrap(),
        };
        let report = owner
            .absorb(&[step(&_sibling_identity, &published)])
            .unwrap();
        assert_eq!(report.registered, vec![sibling.member()]);

        // Now it can be added.
        let added = owner.add(sibling.member()).unwrap();

        // The sibling reads the group's traffic off the lane. It was not keyed
        // when `created` was authored, which is the whole point: key agreement
        // travels in the clear so a device can catch up. It registers the
        // owner's pre-key from the same lane, in the same pass.
        let owner_published = PersonalGraphEvent::PublishPrekey {
            bundle: owner_prekey.to_bytes().unwrap(),
        };
        let learned = sibling
            .absorb(&[
                step(&_owner_identity, &owner_published),
                step(&_owner_identity, &created),
                step(&_owner_identity, &added),
            ])
            .unwrap();
        assert!(learned.installed > 0, "the sibling learned an epoch");
        assert!(sibling.is_keyed(), "and can now read sealed operations");
        assert!(sibling.keyring().unwrap().is_some());
    }

    /// Removal alone leaves a departed device reading new writes, so unpair
    /// turns the epoch in the same gesture.
    #[test]
    fn removing_a_device_also_turns_the_epoch() {
        let dir = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let (_owner_identity, owner_opened) = open_device(0x9f, dir.path());
        let (_sibling_identity, sibling_opened) = open_device(0xa0, other.path());
        let mut owner = owner_opened.group;
        let sibling = sibling_opened.group;

        owner.create().unwrap();
        let published = PersonalGraphEvent::PublishPrekey {
            bundle: sibling_opened.publish.unwrap().to_bytes().unwrap(),
        };
        owner
            .absorb(&[step(&_sibling_identity, &published)])
            .unwrap();
        owner.add(sibling.member()).unwrap();
        let before = owner.session.current_epoch();

        let events = owner.remove_and_rotate(sibling.member()).unwrap();
        assert_eq!(
            events.len(),
            2,
            "a removal and a rotation, not just a removal"
        );
        assert_ne!(
            owner.session.current_epoch(),
            before,
            "the epoch must turn, or the departed device still reads new writes"
        );
    }

    /// Session state is secret. It must survive a restart and must not be
    /// readable without the device's own derived key.
    #[test]
    fn session_state_reopens_and_is_not_stored_in_the_clear() {
        let dir = tempfile::tempdir().unwrap();
        let identity = InMemoryProvider::from_seed([0xa1; 32]);
        let opened = GraphKeyGroup::open(&identity, GRAPH, dir.path()).unwrap();
        let mut owner = opened.group;
        assert!(opened.publish.is_some());
        owner.create().unwrap();
        let member = owner.member();

        let reopened = GraphKeyGroup::open(&identity, GRAPH, dir.path()).unwrap();
        assert!(
            reopened.publish.is_none(),
            "reopening must not mint a second session, which would orphan the first"
        );
        assert_eq!(reopened.group.member(), member);
        assert!(reopened.group.is_keyed(), "membership survived the restart");

        // Another persona's derivation must not open it.
        let stranger = InMemoryProvider::from_seed([0xa2; 32]);
        assert!(
            GraphKeyGroup::open(&stranger, GRAPH, dir.path())
                .map(|opened| opened.publish.is_some())
                .unwrap_or(true),
            "a different identity must not read this device's session state"
        );
    }

    /// A device that is not keyed cannot add or remove anyone.
    #[test]
    fn an_unkeyed_device_cannot_change_membership() {
        let dir = tempfile::tempdir().unwrap();
        let (_identity, opened) = open_device(0xa3, dir.path());
        let mut group = opened.group;
        let stranger = GroupRecipientId([0x11; 32]);
        assert!(matches!(
            group.add(stranger),
            Err(GraphKeyError::NotAMember)
        ));
        assert!(matches!(
            group.remove_and_rotate(stranger),
            Err(GraphKeyError::NotAMember)
        ));
    }

    /// One unreadable frame from one device must not stop this one being keyed.
    #[test]
    fn a_frame_that_will_not_decode_is_counted_rather_than_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let (identity, opened) = open_device(0xa4, dir.path());
        let mut group = opened.group;
        let junk = PersonalGraphEvent::GroupDispatch {
            dispatch: b"not a dispatch".to_vec(),
        };
        let report = group.absorb(&[step(&identity, &junk)]).unwrap();
        assert_eq!(report.unreadable, 1);
        assert_eq!(report.installed, 0);
    }

    fn step(identity: &InMemoryProvider, event: &PersonalGraphEvent) -> KeyAgreementStep {
        use personae::IdentityProvider;
        let inner = match event {
            PersonalGraphEvent::PublishPrekey { bundle } => {
                KeyAgreementEvent::Prekey(bundle.clone())
            }
            PersonalGraphEvent::GroupDispatch { dispatch } => {
                KeyAgreementEvent::Dispatch(dispatch.clone())
            }
            _ => panic!("only key agreement travels this way"),
        };
        KeyAgreementStep {
            operation: [0; 32],
            author_root: identity.master_public_key().to_bytes(),
            step: inner,
        }
    }
}
