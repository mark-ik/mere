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

use crate::personal_sync::{KeyAgreementEvent, KeyAgreementStep, PersonalGraphEvent, SyncRoster};

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
    /// This device's own pre-key bundle, kept because the session can only
    /// mint one at creation and never again.
    ///
    /// A device that cannot author has no way to put this on the lane itself,
    /// so somebody else must carry it. Holding on to it is what makes that
    /// possible at any later moment rather than only in the instant the
    /// session was first opened.
    bundle: Vec<u8>,
}

impl GraphKeyGroup {
    /// Open this device's session for `graph`, creating it on first use.
    ///
    /// Creating a *session* is not creating the *group*: a fresh session is a
    /// device holding its own keys and no membership. It can read nothing
    /// until somebody adds it, which is the correct starting state.
    pub fn open<P: IdentityProvider + ?Sized>(
        identity: &P,
        graph: [u8; 32],
        root: &Path,
    ) -> Result<OpenedKeyGroup, GraphKeyError> {
        let storage = SealedRecordStorage::open_with_key(root, storage_key(identity, graph)?);
        let record =
            PathBuf::from("graphshell/group-sessions").join(format!("{}.session", hex(&graph)));
        let bundle_record = record.with_extension("prekey");
        if let Some(bytes) = storage.load_record::<Vec<u8>>(&record)? {
            let bundle = storage
                .load_record::<Vec<u8>>(&bundle_record)?
                .unwrap_or_default();
            return Ok(OpenedKeyGroup {
                group: Self {
                    session: GroupSession::from_bytes(&bytes)?,
                    storage,
                    record,
                    bundle,
                },
                publish: None,
            });
        }
        let (session, bundle) =
            GroupSession::new(GroupSessionId(graph), &BorrowedProvider(identity))?;
        let bundle_bytes = bundle.to_bytes()?;
        let group = Self {
            session,
            storage,
            record,
            bundle: bundle_bytes.clone(),
        };
        group.persist()?;
        group.storage.save_record(&bundle_record, &bundle_bytes)?;
        Ok(OpenedKeyGroup {
            group,
            publish: Some(bundle),
        })
    }

    /// This device's pre-key bundle, for somebody else to carry.
    ///
    /// Public by construction and self-authenticating: it attests back to this
    /// device's Personae root, so relaying it proves nothing about the relay
    /// and everything about its subject. That is what lets a device which
    /// cannot author still be keyed.
    ///
    /// Empty for a session created before this was persisted, which is
    /// reported rather than papered over.
    pub fn published_bundle(&self) -> &[u8] {
        &self.bundle
    }

    /// This device's recipient id, which is what another member adds.
    pub fn member(&self) -> GroupRecipientId {
        self.session.member()
    }

    /// Whether `member` currently holds a seat in this device's group view.
    pub(crate) fn has_member(&self, member: GroupRecipientId) -> Result<bool, GraphKeyError> {
        Ok(self.session.members()?.contains(&member))
    }

    /// Capture the durable session before preparing a lane-visible change.
    ///
    /// The host restores this checkpoint when authoring the corresponding lane
    /// event fails. Otherwise a locally persisted add, remove, or epoch turn
    /// could become impossible to retry because the lane never learned it.
    pub(crate) fn checkpoint(&self) -> Result<Vec<u8>, GraphKeyError> {
        Ok(self.session.to_bytes()?)
    }

    /// Restore a failed lane-visible change in memory and sealed storage.
    pub(crate) fn restore(&mut self, checkpoint: &[u8]) -> Result<(), GraphKeyError> {
        self.session = GroupSession::from_bytes(checkpoint)?;
        self.persist()
    }

    /// Who this device believes is in the group.
    pub fn members(&self) -> Result<Vec<GroupRecipientId>, GraphKeyError> {
        Ok(self.session.members()?.into_iter().collect())
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
        self.absorb_where(steps, |_| true)
    }

    /// Absorb key traffic while registering only roots the live roster admits.
    ///
    /// Dispatches remain readable regardless of current roster membership:
    /// they were accepted into the retained lane under the authority in force
    /// at the time. Pre-keys are different. Registering one is preparation for
    /// a future add, so a historical device removed before encryption was
    /// enabled must not regain a seat merely because its old pre-key remains.
    pub fn absorb_admitted(
        &mut self,
        steps: &[KeyAgreementStep],
        roster: &SyncRoster,
    ) -> Result<AbsorbReport, GraphKeyError> {
        self.absorb_where(steps, |root| roster.admits(root))
    }

    fn absorb_where(
        &mut self,
        steps: &[KeyAgreementStep],
        admits: impl Fn(&[u8; 32]) -> bool,
    ) -> Result<AbsorbReport, GraphKeyError> {
        let mut report = AbsorbReport::default();
        for step in steps {
            match &step.step {
                KeyAgreementEvent::Prekey(bundle) => {
                    let Ok(bundle) = GroupPrekeyBundle::from_bytes(bundle) else {
                        report.unreadable += 1;
                        continue;
                    };
                    let Ok(root) = bundle.personae_root() else {
                        report.unreadable += 1;
                        continue;
                    };
                    if !admits(&root) {
                        report.ineligible += 1;
                        continue;
                    }
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

/// Which group member belongs to a Personae root, read off the lane.
///
/// Unpairing knows a device by its transport node id and its Personae root;
/// the key group knows it by a recipient id derived from its pre-key. Nothing
/// persists that mapping, and nothing needs to: pre-keys are plaintext and
/// retained, so the lane can always be asked again. A device that re-publishes
/// simply resolves again.
///
/// A root with no pre-key on the lane is `None`, which is the ordinary state
/// of a device that was paired but never joined the key group.
pub fn recipient_for_root(steps: &[KeyAgreementStep], root: [u8; 32]) -> Option<GroupRecipientId> {
    steps.iter().rev().find_map(|step| {
        let KeyAgreementEvent::Prekey(bundle) = &step.step else {
            return None;
        };
        let bundle = GroupPrekeyBundle::from_bytes(bundle).ok()?;
        (bundle.personae_root().ok()? == root).then_some(bundle.recipient)
    })
}

/// Which Personae root a seated member belongs to, read off the lane.
///
/// The inverse of [`recipient_for_root`], and needed for the same reason: the
/// key group knows seats, the graph knows roots, and reconciling one against
/// the other means being able to cross between them in both directions.
pub fn root_for_recipient(
    steps: &[KeyAgreementStep],
    recipient: GroupRecipientId,
) -> Option<[u8; 32]> {
    steps.iter().rev().find_map(|step| {
        let KeyAgreementEvent::Prekey(bundle) = &step.step else {
            return None;
        };
        let bundle = GroupPrekeyBundle::from_bytes(bundle).ok()?;
        (bundle.recipient == recipient).then(|| bundle.personae_root().ok())?
    })
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
    /// Valid pre-keys whose Personae roots are outside the live roster.
    pub ineligible: usize,
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

/// Lets a `&P` where `P` may be unsized reach an API that wants `&dyn`.
///
/// The resident host is generic over `?Sized` providers, and a `&P` cannot be
/// coerced to a trait object without knowing `P: Sized`. Wrapping is cheaper
/// than narrowing that bound across the host, which nothing else needs.
struct BorrowedProvider<'a, P: ?Sized>(&'a P);

impl<P: IdentityProvider + ?Sized> IdentityProvider for BorrowedProvider<'_, P> {
    fn master_public_key(&self) -> personae::Ed25519PublicKey {
        self.0.master_public_key()
    }

    fn derive_keypair(
        &self,
        salt: &[u8],
    ) -> Result<personae::Ed25519Keypair, personae::IdentityError> {
        self.0.derive_keypair(salt)
    }

    fn attest_derived_key(
        &self,
        salt: &[u8],
    ) -> Result<personae::DerivedKeyAttestation, personae::IdentityError> {
        self.0.attest_derived_key(salt)
    }
}

/// A key that protects session state on this disk and nowhere else.
fn storage_key<P: IdentityProvider + ?Sized>(
    identity: &P,
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
    use personae::{IdentityProvider, InMemoryProvider};

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

    /// A retained pre-key is history, not current admission. Removing a root
    /// before encryption starts must keep that old device out of the group.
    #[test]
    fn only_live_roster_roots_are_registered_for_a_future_add() {
        let owner_dir = tempfile::tempdir().unwrap();
        let current_dir = tempfile::tempdir().unwrap();
        let departed_dir = tempfile::tempdir().unwrap();
        let (owner_identity, owner_opened) = open_device(0xb1, owner_dir.path());
        let (current_identity, current_opened) = open_device(0xb2, current_dir.path());
        let (departed_identity, departed_opened) = open_device(0xb3, departed_dir.path());
        let current_member = current_opened.group.member();
        let mut owner = owner_opened.group;
        let roster = SyncRoster::new([
            owner_identity.master_public_key().to_bytes(),
            current_identity.master_public_key().to_bytes(),
        ]);
        let steps = [
            (&current_identity, current_opened.publish.unwrap()),
            (&departed_identity, departed_opened.publish.unwrap()),
        ]
        .into_iter()
        .map(|(identity, bundle)| {
            step(
                identity,
                &PersonalGraphEvent::PublishPrekey {
                    bundle: bundle.to_bytes().unwrap(),
                },
            )
        })
        .collect::<Vec<_>>();

        let report = owner.absorb_admitted(&steps, &roster).unwrap();
        assert_eq!(report.registered, vec![current_member]);
        assert_eq!(report.ineligible, 1);
    }

    /// A lane write failure restores both memory and sealed storage so the
    /// exact group transition can be attempted again.
    #[test]
    fn a_failed_lane_change_can_restore_its_durable_checkpoint() {
        let dir = tempfile::tempdir().unwrap();
        let (identity, opened) = open_device(0xb4, dir.path());
        let mut group = opened.group;
        let checkpoint = group.checkpoint().unwrap();

        group.create().unwrap();
        assert!(group.is_keyed());
        group.restore(&checkpoint).unwrap();
        assert!(!group.is_keyed());

        let reopened = GraphKeyGroup::open(&identity, GRAPH, dir.path()).unwrap();
        assert!(
            !reopened.group.is_keyed(),
            "rollback must reach sealed storage, not only the live value"
        );
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

    /// Unpair knows a root; the key group knows a recipient. The lane is what
    /// joins them, and it can always be asked again.
    #[test]
    fn a_root_resolves_to_the_recipient_that_published_for_it() {
        let dir = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let (owner_identity, owner) = open_device(0xa5, dir.path());
        let (sibling_identity, sibling) = open_device(0xa6, other.path());

        let published: Vec<KeyAgreementStep> = [
            (&owner_identity, owner.publish.unwrap()),
            (&sibling_identity, sibling.publish.unwrap()),
        ]
        .into_iter()
        .map(|(identity, bundle)| {
            step(
                identity,
                &PersonalGraphEvent::PublishPrekey {
                    bundle: bundle.to_bytes().unwrap(),
                },
            )
        })
        .collect();

        assert_eq!(
            recipient_for_root(&published, sibling_identity.master_public_key().to_bytes()),
            Some(sibling.group.member())
        );
        assert_eq!(
            recipient_for_root(&published, owner_identity.master_public_key().to_bytes()),
            Some(owner.group.member())
        );
        // A device paired but never joined has nothing to revoke, which is not
        // an error: it is the ordinary state of most paired devices.
        assert_eq!(recipient_for_root(&published, [0xff; 32]), None);
    }
}

#[cfg(test)]
mod retention_probe {
    use super::*;
    use stickleback::{
        EpochProposalBlocker, EpochRetentionFacts, GroupEncryptionProfile, propose_epoch_pruning,
    };

    /// What retiring epochs would actually cost and permit here, measured
    /// before building machinery for it.
    ///
    /// Two questions. Does the keyring grow in a way worth managing, and can
    /// this domain safely prune at all?
    #[test]
    fn epoch_growth_is_small_and_pruning_is_blocked_for_a_reason() {
        let mut keyring = DataKeyring::new();
        keyring.rotate_random().unwrap();
        let one = keyring.to_bytes().unwrap().len();
        for _ in 0..49 {
            keyring.rotate_random().unwrap();
        }
        let fifty = keyring.to_bytes().unwrap().len();
        assert_eq!(keyring.epoch_count(), 50);

        // One rotation per revocation. If each costs little, unbounded growth
        // is a shape worth naming and not a pressure worth acting on.
        // Measured: 103 bytes per epoch, one epoch per revocation. A mesh that
        // retired a device every week for a decade would spend 54 KB, in a
        // record rewritten on every membership change anyway.
        let per_epoch = (fifty - one) / 49;
        assert!(
            per_epoch < 512,
            "an epoch costs {per_epoch} bytes; if that were large, retention would be urgent"
        );

        // Graphshell retains every operation and has no checkpoint to rebuild
        // a projection from, so it can supply none. Stickleback refuses to
        // propose anything destructive without one, which is the correct
        // answer rather than a limitation to work around: forgetting an epoch
        // makes every operation sealed under it permanently unreadable.
        let proposal = propose_epoch_pruning(
            GroupEncryptionProfile::durable_data(4),
            &keyring,
            &EpochRetentionFacts::default(),
        );
        assert!(!proposal.is_executable());
        assert!(proposal.forget.is_empty(), "nothing may be forgotten");
        assert!(
            proposal
                .blockers
                .contains(&EpochProposalBlocker::MissingCheckpoint),
            "the blocker is the missing checkpoint, not something incidental: {:?}",
            proposal.blockers.len()
        );
    }

    /// A receive-only device has no roster root, so every operation it authors
    /// is refused, including the one that would announce it. Its bundle has to
    /// be carried, and carrying it must not let the carrier speak for it.
    #[test]
    fn a_relayed_bundle_still_authenticates_its_own_subject() {
        let owner_dir = tempfile::tempdir().unwrap();
        let quiet_dir = tempfile::tempdir().unwrap();
        let (_owner_identity, owner_opened) = open_device(0xb1, owner_dir.path());
        let (quiet_identity, quiet_opened) = open_device(0xb2, quiet_dir.path());
        let mut owner = owner_opened.group;
        let quiet = quiet_opened.group;

        // What the quiet device discloses with its pairing facts. It comes
        // from the persisted session, not a fresh mint, because a second
        // session would orphan the seat the lane will hear about.
        let disclosed = quiet.published_bundle().to_vec();
        assert!(!disclosed.is_empty(), "a session must keep a bundle to disclose");
        assert_eq!(
            disclosed,
            quiet_opened.publish.unwrap().to_bytes().unwrap(),
            "the persisted bundle is the one the session minted, not another"
        );

        // The owner relays it. The relay authors the event, and the bundle
        // still names the quiet device: carrying it asserts nothing on that
        // device's behalf beyond what it already signed.
        let relayed = PersonalGraphEvent::PublishPrekey {
            bundle: disclosed.clone(),
        };
        owner.create().unwrap();
        let report = owner
            .absorb_admitted(
                &[step(&_owner_identity, &relayed)],
                &SyncRoster::new([quiet_identity.master_public_key().to_bytes()]),
            )
            .unwrap();
        assert_eq!(
            report.registered,
            vec![quiet.member()],
            "the relayed bundle registers its subject, not its carrier"
        );

        // And the root it resolves to is the quiet device's, so reconciliation
        // seats the right device when that root is admitted as a reader.
        let steps = [step(&_owner_identity, &relayed)];
        use personae::IdentityProvider;
        assert_eq!(
            recipient_for_root(&steps, quiet_identity.master_public_key().to_bytes()),
            Some(quiet.member())
        );
        assert_eq!(
            root_for_recipient(&steps, quiet.member()),
            Some(quiet_identity.master_public_key().to_bytes()),
            "a relayed bundle resolves to its subject in both directions"
        );
    }

    /// Reopening must hand back the same bundle, or a device that discloses
    /// its facts twice would offer two seats and be keyed into neither.
    #[test]
    fn the_disclosed_bundle_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let identity = personae::InMemoryProvider::from_seed([0xb3; 32]);
        let first = GraphKeyGroup::open(&identity, GRAPH, dir.path()).unwrap();
        let disclosed = first.group.published_bundle().to_vec();
        drop(first);

        let reopened = GraphKeyGroup::open(&identity, GRAPH, dir.path()).unwrap();
        assert!(reopened.publish.is_none(), "reopening mints nothing");
        assert_eq!(
            reopened.group.published_bundle(),
            disclosed.as_slice(),
            "the same bundle, so pairing facts read the same twice"
        );
    }
}
