//! Serializable DCGKA group sessions and addressed control payloads.
//!
//! These payloads are carrier-neutral but not authority-bearing. A domain such
//! as Gemot still authenticates and causally orders each control frame before
//! passing it here. Recipient ids are derived from the DCGKA identity key,
//! while a group-scoped Personae-derived key signs that recipient and its
//! pre-key bundle. A host therefore cannot relabel either half of the binding.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::convert::Infallible;
use std::fmt;

use identity::{DerivedKeyAttestation, Ed25519Signature, IdentityProvider};
use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_encryption::Rng;
use p2panda_encryption::crypto::x25519::SecretKey;
use p2panda_encryption::data_scheme::GroupSecretId;
use p2panda_encryption::data_scheme::dcgka::{
    ControlMessage, Dcgka, DcgkaState, DirectMessage, GroupSecretOutput, OperationOutput,
    ProcessInput,
};
use p2panda_encryption::key_bundle::{Lifetime, LongTermKeyBundle};
use p2panda_encryption::key_manager::KeyManager;
use p2panda_encryption::key_registry::KeyRegistry;
use p2panda_encryption::traits::{
    GroupMembership, IdentityHandle, KeyBundle, OperationId, PreKeyManager,
};
use serde::{Deserialize, Serialize};

use crate::{DataKeyring, GroupCiphertext, GroupCryptoError};

const GROUP_SESSION_VERSION: u16 = 1;
const GROUP_PREKEY_VERSION: u16 = 1;
const GROUP_CONTROL_VERSION: u16 = 1;
const GROUP_DIRECT_VERSION: u16 = 1;
const GROUP_PREKEY_IDENTITY_DOMAIN: &[u8] = b"stickleback/group-prekey-identity/v1";

/// Stable id for one encrypted group session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GroupSessionId(pub [u8; 32]);

impl fmt::Display for GroupSessionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&hex::encode(self.0))
    }
}

/// Self-authenticating address of one DCGKA recipient.
///
/// This is the X25519 identity public key carried by the recipient's signed
/// pre-key bundle. It is deliberately distinct from a Personae root id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GroupRecipientId(pub [u8; 32]);

impl fmt::Display for GroupRecipientId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&hex::encode(self.0))
    }
}

impl IdentityHandle for GroupRecipientId {}

/// Per-author control sequence used by authenticated causal delivery.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GroupControlId {
    pub author: GroupRecipientId,
    pub sequence: u64,
}

impl OperationId for GroupControlId {}

/// Inspectable group-control grammar carried inside a domain operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroupControlAction {
    Create {
        initial_members: Vec<GroupRecipientId>,
    },
    Update,
    Add {
        added: GroupRecipientId,
    },
    Remove {
        removed: GroupRecipientId,
    },
}

/// Broadcast half of one DCGKA operation.
///
/// The containing domain operation must authenticate the author and provide
/// causal ordering. This frame only carries the crypto transition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupControlFrame {
    version: u16,
    pub group: GroupSessionId,
    pub id: GroupControlId,
    pub action: GroupControlAction,
}

impl GroupControlFrame {
    pub fn to_bytes(&self) -> Result<Vec<u8>, GroupSessionError> {
        encode_cbor(self).map_err(|error| GroupSessionError::Encode(error.to_string()))
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, GroupSessionError> {
        let frame: Self =
            decode_cbor(bytes).map_err(|error| GroupSessionError::Decode(error.to_string()))?;
        frame.validate()?;
        Ok(frame)
    }

    fn validate(&self) -> Result<(), GroupSessionError> {
        if self.version != GROUP_CONTROL_VERSION {
            return Err(GroupSessionError::UnsupportedControlVersion(self.version));
        }
        Ok(())
    }
}

/// Recipient-bound half of one DCGKA operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupDirectFrame {
    version: u16,
    pub group: GroupSessionId,
    pub control: GroupControlId,
    pub recipient: GroupRecipientId,
    payload: Vec<u8>,
}

impl GroupDirectFrame {
    pub fn to_bytes(&self) -> Result<Vec<u8>, GroupSessionError> {
        encode_cbor(self).map_err(|error| GroupSessionError::Encode(error.to_string()))
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, GroupSessionError> {
        let frame: Self =
            decode_cbor(bytes).map_err(|error| GroupSessionError::Decode(error.to_string()))?;
        frame.validate()?;
        Ok(frame)
    }

    fn validate(&self) -> Result<(), GroupSessionError> {
        if self.version != GROUP_DIRECT_VERSION {
            return Err(GroupSessionError::UnsupportedDirectVersion(self.version));
        }
        Ok(())
    }
}

/// Broadcast control plus its separately routable recipient messages.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupSessionDispatch {
    pub control: GroupControlFrame,
    pub direct: Vec<GroupDirectFrame>,
}

impl GroupSessionDispatch {
    /// Return the sole direct message addressed to `recipient`, when present.
    pub fn direct_for(&self, recipient: GroupRecipientId) -> Option<&GroupDirectFrame> {
        self.direct
            .iter()
            .find(|message| message.recipient == recipient)
    }
}

/// Publishable long-term pre-key bundle for one group recipient.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupPrekeyBundle {
    version: u16,
    pub group: GroupSessionId,
    pub recipient: GroupRecipientId,
    payload: Vec<u8>,
    attestation: DerivedKeyAttestation,
    identity_signature: Vec<u8>,
}

impl GroupPrekeyBundle {
    /// Personae root that authenticated this group recipient and pre-key.
    pub fn personae_root(&self) -> Result<[u8; 32], GroupSessionError> {
        self.verify_identity()
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, GroupSessionError> {
        encode_cbor(self).map_err(|error| GroupSessionError::Encode(error.to_string()))
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, GroupSessionError> {
        let bundle: Self =
            decode_cbor(bytes).map_err(|error| GroupSessionError::Decode(error.to_string()))?;
        bundle.decode_bundle()?;
        Ok(bundle)
    }

    fn decode_bundle(&self) -> Result<(LongTermKeyBundle, [u8; 32]), GroupSessionError> {
        if self.version != GROUP_PREKEY_VERSION {
            return Err(GroupSessionError::UnsupportedPrekeyVersion(self.version));
        }
        let bundle: LongTermKeyBundle = decode_cbor(self.payload.as_slice())
            .map_err(|error| GroupSessionError::Decode(error.to_string()))?;
        let derived = GroupRecipientId(bundle.identity_key().to_bytes());
        if self.recipient != derived {
            return Err(GroupSessionError::PrekeyRecipientMismatch {
                claimed: self.recipient,
                derived,
            });
        }
        let root = self.verify_identity()?;
        Ok((bundle, root))
    }

    fn verify_identity(&self) -> Result<[u8; 32], GroupSessionError> {
        let salt = group_prekey_identity_salt(self.group);
        if !self.attestation.verify(&salt) {
            return Err(GroupSessionError::InvalidPersonaeAttestation);
        }
        let root = self
            .attestation
            .master_public_key()
            .map_err(|error| GroupSessionError::Identity(error.to_string()))?
            .to_bytes();
        let derived = self
            .attestation
            .derived_public_key()
            .map_err(|error| GroupSessionError::Identity(error.to_string()))?;
        let signature_bytes: [u8; 64] = self
            .identity_signature
            .as_slice()
            .try_into()
            .map_err(|_| GroupSessionError::InvalidPersonaeSignature)?;
        let signature = Ed25519Signature::from_bytes(&signature_bytes);
        if !derived.verify(&self.identity_signing_bytes(), &signature) {
            return Err(GroupSessionError::InvalidPersonaeSignature);
        }
        Ok(root)
    }

    fn identity_signing_bytes(&self) -> Vec<u8> {
        let mut message = Vec::with_capacity(
            GROUP_PREKEY_IDENTITY_DOMAIN.len() + 2 + 32 + 32 + 8 + self.payload.len(),
        );
        message.extend_from_slice(GROUP_PREKEY_IDENTITY_DOMAIN);
        message.extend_from_slice(&self.version.to_le_bytes());
        message.extend_from_slice(&self.group.0);
        message.extend_from_slice(&self.recipient.0);
        message.extend_from_slice(&(self.payload.len() as u64).to_le_bytes());
        message.extend_from_slice(&self.payload);
        message
    }

    fn digest(&self) -> Result<[u8; 32], GroupSessionError> {
        let bytes =
            encode_cbor(self).map_err(|error| GroupSessionError::Encode(error.to_string()))?;
        Ok(*blake3::hash(&bytes).as_bytes())
    }
}

fn group_prekey_identity_salt(group: GroupSessionId) -> Vec<u8> {
    let mut salt = Vec::with_capacity(GROUP_PREKEY_IDENTITY_DOMAIN.len() + 32);
    salt.extend_from_slice(GROUP_PREKEY_IDENTITY_DOMAIN);
    salt.extend_from_slice(&group.0);
    salt
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct OrderedMembershipState {
    my_id: GroupRecipientId,
    members: BTreeSet<GroupRecipientId>,
}

#[derive(Clone, Debug)]
struct OrderedMembership;

impl OrderedMembership {
    fn init(my_id: GroupRecipientId) -> OrderedMembershipState {
        OrderedMembershipState {
            my_id,
            members: BTreeSet::new(),
        }
    }
}

impl GroupMembership<GroupRecipientId, GroupControlId> for OrderedMembership {
    type State = OrderedMembershipState;
    type Error = Infallible;

    fn create(
        my_id: GroupRecipientId,
        initial_members: &[GroupRecipientId],
    ) -> Result<Self::State, Self::Error> {
        Ok(OrderedMembershipState {
            my_id,
            members: initial_members.iter().copied().collect(),
        })
    }

    fn from_welcome(
        my_id: GroupRecipientId,
        mut history: Self::State,
    ) -> Result<Self::State, Self::Error> {
        history.my_id = my_id;
        history.members.insert(my_id);
        Ok(history)
    }

    fn add(
        mut state: Self::State,
        _adder: GroupRecipientId,
        added: GroupRecipientId,
        _operation_id: GroupControlId,
    ) -> Result<Self::State, Self::Error> {
        state.members.insert(added);
        Ok(state)
    }

    fn remove(
        mut state: Self::State,
        _remover: GroupRecipientId,
        removed: &GroupRecipientId,
        _operation_id: GroupControlId,
    ) -> Result<Self::State, Self::Error> {
        state.members.remove(removed);
        Ok(state)
    }

    fn members(state: &Self::State) -> Result<HashSet<GroupRecipientId>, Self::Error> {
        Ok(state.members.iter().copied().collect())
    }
}

type SessionDcgka = DcgkaState<
    GroupRecipientId,
    GroupControlId,
    KeyRegistry<GroupRecipientId>,
    OrderedMembership,
    KeyManager,
>;
type SessionDirect = DirectMessage<GroupRecipientId, GroupControlId, OrderedMembership>;

/// Result of applying one authenticated, causally ordered control frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupSessionProcess {
    /// Epochs learned from the addressed direct message, oldest first.
    pub installed_epochs: Vec<GroupSecretId>,
}

/// Durable state for one DCGKA group member.
///
/// Persist [`GroupSession::to_bytes`] through Personae's sealed-record store.
/// The encoded state contains long-term private keys, ratchet state, and data
/// epochs and must never be written to plaintext storage.
#[derive(Debug, Serialize, Deserialize)]
pub struct GroupSession {
    version: u16,
    group: GroupSessionId,
    member: GroupRecipientId,
    personae_root: [u8; 32],
    dcgka: Option<SessionDcgka>,
    keyring: DataKeyring,
    next_local_sequence: u64,
    next_remote_sequence: BTreeMap<GroupRecipientId, u64>,
    prekey_digests: BTreeMap<GroupRecipientId, [u8; 32]>,
    prekey_roots: BTreeMap<GroupRecipientId, [u8; 32]>,
}

impl GroupSession {
    /// Prepare a local recipient and its publishable pre-key bundle.
    pub fn new(
        group: GroupSessionId,
        identity: &dyn IdentityProvider,
    ) -> Result<(Self, GroupPrekeyBundle), GroupSessionError> {
        let rng = Rng::default();
        let identity_secret = SecretKey::from_rng(&rng)
            .map_err(|error| GroupSessionError::Engine(error.to_string()))?;
        let keys = KeyManager::init(&identity_secret)
            .map_err(|error| GroupSessionError::Engine(error.to_string()))?;
        let keys = KeyManager::rotate_prekey(keys, Lifetime::default(), &rng)
            .map_err(|error| GroupSessionError::Engine(error.to_string()))?;
        let prekey = KeyManager::prekey_bundle(&keys)
            .map_err(|error| GroupSessionError::Engine(error.to_string()))?;
        let member = GroupRecipientId(prekey.identity_key().to_bytes());
        let payload =
            encode_cbor(&prekey).map_err(|error| GroupSessionError::Encode(error.to_string()))?;
        let salt = group_prekey_identity_salt(group);
        let attestation = identity
            .attest_derived_key(&salt)
            .map_err(|error| GroupSessionError::Identity(error.to_string()))?;
        let signer = identity
            .derive_keypair(&salt)
            .map_err(|error| GroupSessionError::Identity(error.to_string()))?;
        let mut public_bundle = GroupPrekeyBundle {
            version: GROUP_PREKEY_VERSION,
            group,
            recipient: member,
            payload,
            attestation,
            identity_signature: Vec::new(),
        };
        public_bundle.identity_signature = signer
            .sign(&public_bundle.identity_signing_bytes())
            .to_bytes()
            .to_vec();
        let personae_root = public_bundle.personae_root()?;
        let pki = KeyRegistry::add_longterm_bundle(KeyRegistry::init(), member, prekey)
            .map_err(|error| GroupSessionError::Engine(error.to_string()))?;
        let dcgka = Dcgka::init(member, keys, pki, OrderedMembership::init(member));
        let session = Self {
            version: GROUP_SESSION_VERSION,
            group,
            member,
            personae_root,
            dcgka: Some(dcgka),
            keyring: DataKeyring::new(),
            next_local_sequence: 0,
            next_remote_sequence: BTreeMap::new(),
            prekey_digests: BTreeMap::from([(member, public_bundle.digest()?)]),
            prekey_roots: BTreeMap::from([(member, personae_root)]),
        };
        Ok((session, public_bundle))
    }

    pub fn group(&self) -> GroupSessionId {
        self.group
    }

    pub fn member(&self) -> GroupRecipientId {
        self.member
    }

    pub fn personae_root(&self) -> [u8; 32] {
        self.personae_root
    }

    pub fn current_epoch(&self) -> Option<GroupSecretId> {
        self.keyring.current_epoch()
    }

    pub fn epoch_count(&self) -> usize {
        self.keyring.epoch_count()
    }

    /// Serialize the retained data epochs for a domain replica opened beside
    /// this group session.
    ///
    /// The returned bytes remain secret state and must stay within the same
    /// Personae-sealed storage boundary as [`Self::to_bytes`].
    pub fn data_keyring_state(&self) -> Result<Vec<u8>, GroupSessionError> {
        self.keyring.to_bytes().map_err(GroupSessionError::Crypto)
    }

    pub fn members(&self) -> Result<BTreeSet<GroupRecipientId>, GroupSessionError> {
        let state = self.state()?;
        let members =
            Dcgka::members(state).map_err(|error| GroupSessionError::Engine(error.to_string()))?;
        Ok(members.into_iter().collect())
    }

    /// Register a cryptographically bound pre-key only after the containing
    /// domain has established that its Personae root is eligible to join.
    pub fn register_prekey(&mut self, bundle: &GroupPrekeyBundle) -> Result<(), GroupSessionError> {
        if bundle.group != self.group {
            return Err(GroupSessionError::WrongGroup {
                expected: self.group,
                actual: bundle.group,
            });
        }
        let (decoded, personae_root) = bundle.decode_bundle()?;
        let digest = bundle.digest()?;
        if let Some(existing) = self.prekey_digests.get(&bundle.recipient) {
            return if existing == &digest {
                Ok(())
            } else {
                Err(GroupSessionError::ConflictingPrekey(bundle.recipient))
            };
        }

        let mut next = self.try_clone()?;
        let state = next.take_state()?;
        let pki = KeyRegistry::add_longterm_bundle(state.pki, bundle.recipient, decoded)
            .map_err(|error| GroupSessionError::Engine(error.to_string()))?;
        next.dcgka = Some(SessionDcgka { pki, ..state });
        next.prekey_digests.insert(bundle.recipient, digest);
        next.prekey_roots.insert(bundle.recipient, personae_root);
        next.validate()?;
        *self = next;
        Ok(())
    }

    /// Create the initial group and return its broadcast and addressed frames.
    pub fn create(
        &mut self,
        initial_members: &[GroupRecipientId],
    ) -> Result<GroupSessionDispatch, GroupSessionError> {
        if !self.members()?.is_empty() {
            return Err(GroupSessionError::GroupAlreadyCreated);
        }
        let members: BTreeSet<_> = initial_members
            .iter()
            .copied()
            .chain(std::iter::once(self.member))
            .collect();
        self.require_registered(members.iter().copied())?;

        let mut next = self.try_clone()?;
        let secret = next.keyring.rotate_random()?;
        let state = next.take_state()?;
        let (state, output) = Dcgka::create(
            state,
            members.into_iter().collect(),
            &secret,
            &Rng::default(),
        )
        .map_err(|error| GroupSessionError::Engine(error.to_string()))?;
        next.dcgka = Some(state);
        let dispatch = next.finish_local(output)?;
        *self = next;
        Ok(dispatch)
    }

    /// Rotate the group epoch for all current members.
    pub fn update(&mut self) -> Result<GroupSessionDispatch, GroupSessionError> {
        self.require_active_author()?;
        let mut next = self.try_clone()?;
        let secret = next.keyring.rotate_random()?;
        let state = next.take_state()?;
        let (state, output) = Dcgka::update(state, &secret, &Rng::default())
            .map_err(|error| GroupSessionError::Engine(error.to_string()))?;
        next.dcgka = Some(state);
        let dispatch = next.finish_local(output)?;
        *self = next;
        Ok(dispatch)
    }

    /// Welcome a registered recipient with the retained data-epoch bundle.
    pub fn add(
        &mut self,
        added: GroupRecipientId,
    ) -> Result<GroupSessionDispatch, GroupSessionError> {
        // One membership snapshot answers both the active-author check and the
        // already-present check; `members()` walks the DCGKA state and rebuilds
        // two sets on every call, so the checks share one view here rather than
        // each asking for their own.
        let members = self.members()?;
        if !members.contains(&self.member) {
            return Err(GroupSessionError::LocalMemberInactive);
        }
        self.require_registered(std::iter::once(added))?;
        if members.contains(&added) {
            return Err(GroupSessionError::MemberAlreadyPresent(added));
        }
        let mut next = self.try_clone()?;
        let state = next.take_state()?;
        let (state, output) =
            Dcgka::add(state, added, next.keyring.secret_bundle(), &Rng::default())
                .map_err(|error| GroupSessionError::Engine(error.to_string()))?;
        next.dcgka = Some(state);
        let dispatch = next.finish_local(output)?;
        *self = next;
        Ok(dispatch)
    }

    /// Remove a member and rotate the epoch away from that recipient.
    pub fn remove(
        &mut self,
        removed: GroupRecipientId,
    ) -> Result<GroupSessionDispatch, GroupSessionError> {
        // As in `add`: one membership snapshot serves both checks.
        let members = self.members()?;
        if !members.contains(&self.member) {
            return Err(GroupSessionError::LocalMemberInactive);
        }
        if removed == self.member {
            return Err(GroupSessionError::CannotRemoveSelf);
        }
        if !members.contains(&removed) {
            return Err(GroupSessionError::UnknownMember(removed));
        }
        let mut next = self.try_clone()?;
        let secret = next.keyring.rotate_random()?;
        let state = next.take_state()?;
        let (state, output) = Dcgka::remove(state, removed, &secret, &Rng::default())
            .map_err(|error| GroupSessionError::Engine(error.to_string()))?;
        next.dcgka = Some(state);
        let dispatch = next.finish_local(output)?;
        *self = next;
        Ok(dispatch)
    }

    /// Apply one frame after the domain has authenticated and causally ordered
    /// it. `authenticated_author_root` must come from that verified domain
    /// operation and must match the pre-key's Personae binding. State changes
    /// commit only after the full crypto transition succeeds.
    pub fn process(
        &mut self,
        authenticated_author_root: [u8; 32],
        control: &GroupControlFrame,
        direct: Option<&GroupDirectFrame>,
    ) -> Result<GroupSessionProcess, GroupSessionError> {
        let mut next = self.try_clone()?;
        let output = next.process_inner(authenticated_author_root, control, direct)?;
        next.validate()?;
        *self = next;
        Ok(output)
    }

    pub fn seal_random(&self, plaintext: &[u8]) -> Result<GroupCiphertext, GroupSessionError> {
        self.keyring
            .seal_random(plaintext)
            .map_err(GroupSessionError::Crypto)
    }

    pub fn open(&self, ciphertext: &GroupCiphertext) -> Result<Vec<u8>, GroupSessionError> {
        self.keyring
            .open(ciphertext)
            .map_err(GroupSessionError::Crypto)
    }

    /// Serialize secret session state for storage in a Personae-sealed record.
    pub fn to_bytes(&self) -> Result<Vec<u8>, GroupSessionError> {
        encode_cbor(self).map_err(|error| GroupSessionError::Encode(error.to_string()))
    }

    /// Restore and validate secret session state loaded from a sealed record.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, GroupSessionError> {
        let session: Self =
            decode_cbor(bytes).map_err(|error| GroupSessionError::Decode(error.to_string()))?;
        session.validate()?;
        Ok(session)
    }

    fn state(&self) -> Result<&SessionDcgka, GroupSessionError> {
        self.dcgka.as_ref().ok_or(GroupSessionError::MissingState)
    }

    fn take_state(&mut self) -> Result<SessionDcgka, GroupSessionError> {
        self.dcgka.take().ok_or(GroupSessionError::MissingState)
    }

    fn require_registered(
        &self,
        recipients: impl IntoIterator<Item = GroupRecipientId>,
    ) -> Result<(), GroupSessionError> {
        for recipient in recipients {
            if !self.prekey_digests.contains_key(&recipient) {
                return Err(GroupSessionError::MissingPrekey(recipient));
            }
        }
        Ok(())
    }

    fn require_active_author(&self) -> Result<(), GroupSessionError> {
        if !self.members()?.contains(&self.member) {
            return Err(GroupSessionError::LocalMemberInactive);
        }
        Ok(())
    }

    fn finish_local(
        &mut self,
        output: OperationOutput<GroupRecipientId, GroupControlId, OrderedMembership>,
    ) -> Result<GroupSessionDispatch, GroupSessionError> {
        let id = GroupControlId {
            author: self.member,
            sequence: self.next_local_sequence,
        };
        let control = GroupControlFrame {
            version: GROUP_CONTROL_VERSION,
            group: self.group,
            id,
            action: output.control_message.into(),
        };
        let mut direct = Vec::with_capacity(output.direct_messages.len());
        for message in output.direct_messages {
            let recipient = message.recipient;
            let payload = encode_cbor(&message)
                .map_err(|error| GroupSessionError::Encode(error.to_string()))?;
            direct.push(GroupDirectFrame {
                version: GROUP_DIRECT_VERSION,
                group: self.group,
                control: id,
                recipient,
                payload,
            });
        }
        let dispatch = GroupSessionDispatch { control, direct };
        self.process_inner(self.personae_root, &dispatch.control, None)?;
        Ok(dispatch)
    }

    fn process_inner(
        &mut self,
        authenticated_author_root: [u8; 32],
        control: &GroupControlFrame,
        direct: Option<&GroupDirectFrame>,
    ) -> Result<GroupSessionProcess, GroupSessionError> {
        control.validate()?;
        if control.group != self.group {
            return Err(GroupSessionError::WrongGroup {
                expected: self.group,
                actual: control.group,
            });
        }
        if control.id.author != self.member && !self.prekey_digests.contains_key(&control.id.author)
        {
            return Err(GroupSessionError::MissingPrekey(control.id.author));
        }
        let expected_root = self
            .prekey_roots
            .get(&control.id.author)
            .ok_or(GroupSessionError::MissingPrekey(control.id.author))?;
        if expected_root != &authenticated_author_root {
            return Err(GroupSessionError::ControlAuthorRootMismatch {
                author: control.id.author,
            });
        }

        // Only Update and Remove consult membership: Create and Add decide from
        // the frame alone, so the membership snapshot is built inside the arms
        // that read it instead of once for every frame processed.
        let needs_direct = match &control.action {
            GroupControlAction::Create { initial_members } => {
                control.id.author != self.member && initial_members.contains(&self.member)
            }
            GroupControlAction::Update => {
                control.id.author != self.member && self.members()?.contains(&self.member)
            }
            GroupControlAction::Add { added } => {
                control.id.author != self.member && *added == self.member
            }
            GroupControlAction::Remove { removed } => {
                control.id.author != self.member
                    && self.members()?.contains(&self.member)
                    && *removed != self.member
            }
        };
        match (needs_direct, direct) {
            (true, None) => return Err(GroupSessionError::MissingAddressedDirect(self.member)),
            (false, Some(_)) => return Err(GroupSessionError::UnexpectedDirect(self.member)),
            _ => {}
        }

        let expected = self
            .next_remote_sequence
            .get(&control.id.author)
            .copied()
            .unwrap_or(0);
        let joining_at_current_sequence =
            !self.next_remote_sequence.contains_key(&control.id.author)
                && matches!(
                    control.action,
                    GroupControlAction::Add { added } if added == self.member
                );
        if control.id.sequence != expected && !joining_at_current_sequence {
            return Err(GroupSessionError::UnexpectedSequence {
                author: control.id.author,
                expected,
                actual: control.id.sequence,
            });
        }

        let direct_message = match direct {
            Some(frame) => Some(self.decode_direct(control, frame)?),
            None => None,
        };
        let state = self.take_state()?;
        let input = ProcessInput {
            seq: control.id,
            sender: control.id.author,
            control_message: control.action.clone().into(),
            direct_message,
        };
        let (state, output) = Dcgka::process(state, input)
            .map_err(|error| GroupSessionError::Engine(error.to_string()))?;
        self.dcgka = Some(state);
        let installed_epochs = self.install_output(output);
        let next_sequence = control
            .id
            .sequence
            .checked_add(1)
            .ok_or(GroupSessionError::SequenceExhausted(control.id.author))?;
        self.next_remote_sequence
            .insert(control.id.author, next_sequence);
        if control.id.author == self.member {
            self.next_local_sequence = next_sequence;
        }
        Ok(GroupSessionProcess { installed_epochs })
    }

    fn decode_direct(
        &self,
        control: &GroupControlFrame,
        direct: &GroupDirectFrame,
    ) -> Result<SessionDirect, GroupSessionError> {
        direct.validate()?;
        if direct.group != self.group {
            return Err(GroupSessionError::WrongGroup {
                expected: self.group,
                actual: direct.group,
            });
        }
        if direct.control != control.id {
            return Err(GroupSessionError::DirectControlMismatch);
        }
        if direct.recipient != self.member {
            return Err(GroupSessionError::DirectRecipientMismatch {
                expected: self.member,
                actual: direct.recipient,
            });
        }
        let message: SessionDirect = decode_cbor(direct.payload.as_slice())
            .map_err(|error| GroupSessionError::Decode(error.to_string()))?;
        if message.recipient != direct.recipient {
            return Err(GroupSessionError::DirectRecipientMismatch {
                expected: direct.recipient,
                actual: message.recipient,
            });
        }
        Ok(message)
    }

    fn install_output(&mut self, output: GroupSecretOutput) -> Vec<GroupSecretId> {
        match output {
            GroupSecretOutput::None => Vec::new(),
            GroupSecretOutput::Secret(secret) => {
                let epoch = secret.id();
                self.keyring.install(secret);
                vec![epoch]
            }
            GroupSecretOutput::Bundle(bundle) => self.keyring.install_bundle(bundle),
        }
    }

    fn try_clone(&self) -> Result<Self, GroupSessionError> {
        Self::from_bytes(&self.to_bytes()?)
    }

    fn validate(&self) -> Result<(), GroupSessionError> {
        if self.version != GROUP_SESSION_VERSION {
            return Err(GroupSessionError::UnsupportedSessionVersion(self.version));
        }
        self.keyring.validate()?;
        let state = self.state()?;
        if state.my_id != self.member || state.dgm.my_id != self.member {
            return Err(GroupSessionError::StateRecipientMismatch);
        }
        if !self.prekey_digests.contains_key(&self.member) {
            return Err(GroupSessionError::MissingPrekey(self.member));
        }
        if self.prekey_roots.get(&self.member) != Some(&self.personae_root)
            || self.prekey_roots.len() != self.prekey_digests.len()
            || self
                .prekey_digests
                .keys()
                .any(|recipient| !self.prekey_roots.contains_key(recipient))
        {
            return Err(GroupSessionError::InvalidPrekeyBindings);
        }
        let expected_local = self
            .next_remote_sequence
            .get(&self.member)
            .copied()
            .unwrap_or(0);
        if self.next_local_sequence != expected_local {
            return Err(GroupSessionError::InvalidLocalSequence {
                expected: expected_local,
                actual: self.next_local_sequence,
            });
        }
        Ok(())
    }
}

impl From<ControlMessage<GroupRecipientId>> for GroupControlAction {
    fn from(value: ControlMessage<GroupRecipientId>) -> Self {
        match value {
            ControlMessage::Create { initial_members } => Self::Create { initial_members },
            ControlMessage::Update => Self::Update,
            ControlMessage::Add { added } => Self::Add { added },
            ControlMessage::Remove { removed } => Self::Remove { removed },
        }
    }
}

impl From<GroupControlAction> for ControlMessage<GroupRecipientId> {
    fn from(value: GroupControlAction) -> Self {
        match value {
            GroupControlAction::Create { initial_members } => Self::Create { initial_members },
            GroupControlAction::Update => Self::Update,
            GroupControlAction::Add { added } => Self::Add { added },
            GroupControlAction::Remove { removed } => Self::Remove { removed },
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GroupSessionError {
    #[error("group session has no DCGKA state")]
    MissingState,
    #[error("group session already has membership state")]
    GroupAlreadyCreated,
    #[error("local group recipient is not an active member")]
    LocalMemberInactive,
    #[error("cannot remove the local author with its own rotation")]
    CannotRemoveSelf,
    #[error("group recipient {0} is already present")]
    MemberAlreadyPresent(GroupRecipientId),
    #[error("group recipient {0} is not a current member")]
    UnknownMember(GroupRecipientId),
    #[error("missing authenticated pre-key for group recipient {0}")]
    MissingPrekey(GroupRecipientId),
    #[error("conflicting pre-key for group recipient {0}")]
    ConflictingPrekey(GroupRecipientId),
    #[error("pre-key recipient mismatch: claimed {claimed}, derived {derived}")]
    PrekeyRecipientMismatch {
        claimed: GroupRecipientId,
        derived: GroupRecipientId,
    },
    #[error("group pre-key has an invalid Personae derived-key attestation")]
    InvalidPersonaeAttestation,
    #[error("group pre-key has an invalid Personae binding signature")]
    InvalidPersonaeSignature,
    #[error("group session has inconsistent pre-key identity bindings")]
    InvalidPrekeyBindings,
    #[error("authenticated Personae root does not own control author {author}")]
    ControlAuthorRootMismatch { author: GroupRecipientId },
    #[error("Personae identity boundary: {0}")]
    Identity(String),
    #[error("wrong group session: expected {expected}, got {actual}")]
    WrongGroup {
        expected: GroupSessionId,
        actual: GroupSessionId,
    },
    #[error("addressed direct message for {0} is required")]
    MissingAddressedDirect(GroupRecipientId),
    #[error("group recipient {0} was given an unexpected direct message")]
    UnexpectedDirect(GroupRecipientId),
    #[error("direct message names a different control frame")]
    DirectControlMismatch,
    #[error("direct recipient mismatch: expected {expected}, got {actual}")]
    DirectRecipientMismatch {
        expected: GroupRecipientId,
        actual: GroupRecipientId,
    },
    #[error("unexpected control sequence for {author}: expected {expected}, got {actual}")]
    UnexpectedSequence {
        author: GroupRecipientId,
        expected: u64,
        actual: u64,
    },
    #[error("control sequence exhausted for {0}")]
    SequenceExhausted(GroupRecipientId),
    #[error("group session recipient state does not match its public recipient")]
    StateRecipientMismatch,
    #[error("local sequence state mismatch: expected {expected}, got {actual}")]
    InvalidLocalSequence { expected: u64, actual: u64 },
    #[error("unsupported group-session version {0}")]
    UnsupportedSessionVersion(u16),
    #[error("unsupported group pre-key version {0}")]
    UnsupportedPrekeyVersion(u16),
    #[error("unsupported group control version {0}")]
    UnsupportedControlVersion(u16),
    #[error("unsupported group direct-message version {0}")]
    UnsupportedDirectVersion(u16),
    #[error("group-session encryption engine: {0}")]
    Engine(String),
    #[error("encode group-session state: {0}")]
    Encode(String),
    #[error("decode group-session state: {0}")]
    Decode(String),
    #[error(transparent)]
    Crypto(#[from] GroupCryptoError),
}

#[cfg(test)]
mod tests {
    use identity::{InMemoryProvider, SealedRecordStorage};

    use super::*;

    fn register_all(sessions: &mut [&mut GroupSession], bundles: &[GroupPrekeyBundle]) {
        for session in sessions {
            for bundle in bundles {
                session.register_prekey(bundle).unwrap();
            }
        }
    }

    #[test]
    fn addressed_welcome_update_removal_and_sealed_restart() {
        let group = GroupSessionId([0x47; 32]);
        let alice_identity = InMemoryProvider::from_seed([0xa1; 32]);
        let bob_identity = InMemoryProvider::from_seed([0xb2; 32]);
        let charlie_identity = InMemoryProvider::from_seed([0xc3; 32]);
        let (mut alice, alice_prekey) = GroupSession::new(group, &alice_identity).unwrap();
        let (mut bob, bob_prekey) = GroupSession::new(group, &bob_identity).unwrap();
        let (mut charlie, charlie_prekey) = GroupSession::new(group, &charlie_identity).unwrap();
        let alice_prekey =
            GroupPrekeyBundle::from_bytes(&alice_prekey.to_bytes().unwrap()).unwrap();
        assert_eq!(alice_prekey.personae_root().unwrap(), alice.personae_root());
        assert_eq!(bob_prekey.personae_root().unwrap(), bob.personae_root());
        assert_eq!(
            charlie_prekey.personae_root().unwrap(),
            charlie.personae_root()
        );
        assert_ne!(alice.personae_root(), bob.personae_root());
        let mut forged_prekey = alice_prekey.clone();
        forged_prekey.identity_signature[0] ^= 0x01;
        assert!(matches!(
            GroupPrekeyBundle::from_bytes(&forged_prekey.to_bytes().unwrap()),
            Err(GroupSessionError::InvalidPersonaeSignature)
        ));
        register_all(
            &mut [&mut alice, &mut bob, &mut charlie],
            &[
                alice_prekey.clone(),
                bob_prekey.clone(),
                charlie_prekey.clone(),
            ],
        );

        let create = alice.create(&[bob.member(), charlie.member()]).unwrap();
        let encoded_control = create.control.to_bytes().unwrap();
        let create_control = GroupControlFrame::from_bytes(&encoded_control).unwrap();
        let bob_direct = create.direct_for(bob.member()).unwrap();
        let encoded_direct = bob_direct.to_bytes().unwrap();
        let bob_direct = GroupDirectFrame::from_bytes(&encoded_direct).unwrap();
        let charlie_direct = create.direct_for(charlie.member()).unwrap();

        let bob_before = bob.to_bytes().unwrap();
        let mut wrong_recipient = GroupSession::from_bytes(&bob_before).unwrap();
        assert!(matches!(
            wrong_recipient.process(alice.personae_root(), &create_control, Some(charlie_direct)),
            Err(GroupSessionError::DirectRecipientMismatch { .. })
        ));
        assert_eq!(wrong_recipient.current_epoch(), None);

        let mut wrong_root = GroupSession::from_bytes(&bob_before).unwrap();
        assert!(matches!(
            wrong_root.process(bob.personae_root(), &create_control, Some(&bob_direct)),
            Err(GroupSessionError::ControlAuthorRootMismatch { .. })
        ));
        assert_eq!(wrong_root.current_epoch(), None);

        let bob_result = bob
            .process(alice.personae_root(), &create_control, Some(&bob_direct))
            .unwrap();
        assert_eq!(bob_result.installed_epochs.len(), 1);
        let charlie_result = charlie
            .process(alice.personae_root(), &create_control, Some(charlie_direct))
            .unwrap();
        assert_eq!(charlie_result.installed_epochs.len(), 1);
        assert_eq!(bob.current_epoch(), alice.current_epoch());
        assert_eq!(charlie.current_epoch(), alice.current_epoch());

        let before_update = alice.seal_random(b"before update").unwrap();
        assert_eq!(bob.open(&before_update).unwrap(), b"before update");

        let update = alice.update().unwrap();
        bob.process(
            alice.personae_root(),
            &update.control,
            update.direct_for(bob.member()),
        )
        .unwrap();
        charlie
            .process(
                alice.personae_root(),
                &update.control,
                update.direct_for(charlie.member()),
            )
            .unwrap();
        let after_update = alice.seal_random(b"after update").unwrap();
        assert_eq!(bob.open(&after_update).unwrap(), b"after update");
        assert_eq!(charlie.open(&after_update).unwrap(), b"after update");

        let directory = tempfile::tempdir().unwrap();
        let storage = SealedRecordStorage::open_with_key(directory.path(), [0x53; 32]);
        storage
            .save_record("groups/main.session", &bob.to_bytes().unwrap())
            .unwrap();
        let sealed_bytes: Vec<u8> = storage
            .load_record("groups/main.session")
            .unwrap()
            .expect("sealed group session");
        let mut reopened_bob = GroupSession::from_bytes(&sealed_bytes).unwrap();
        assert_eq!(reopened_bob.member(), bob.member());
        assert_eq!(reopened_bob.members().unwrap(), bob.members().unwrap());
        assert_eq!(reopened_bob.current_epoch(), bob.current_epoch());
        let reopened_keys = DataKeyring::from_bytes(&reopened_bob.data_keyring_state().unwrap())
            .expect("group session exposes the same sealed data epochs");
        assert_eq!(
            reopened_keys.epochs_oldest_first(),
            bob.keyring.epochs_oldest_first()
        );
        assert_eq!(reopened_bob.open(&after_update).unwrap(), b"after update");

        let removal = alice.remove(bob.member()).unwrap();
        assert!(removal.direct_for(bob.member()).is_none());
        assert!(removal.direct_for(charlie.member()).is_some());
        let bob_removal = reopened_bob
            .process(
                alice.personae_root(),
                &removal.control,
                removal.direct_for(reopened_bob.member()),
            )
            .unwrap();
        assert!(bob_removal.installed_epochs.is_empty());
        charlie
            .process(
                alice.personae_root(),
                &removal.control,
                removal.direct_for(charlie.member()),
            )
            .unwrap();

        let after_removal = alice.seal_random(b"after removal").unwrap();
        assert!(matches!(
            reopened_bob.open(&after_removal),
            Err(GroupSessionError::Crypto(GroupCryptoError::UnknownEpoch(_)))
        ));
        assert_eq!(charlie.open(&after_removal).unwrap(), b"after removal");
        assert_eq!(
            alice.members().unwrap(),
            BTreeSet::from([alice.member(), charlie.member()])
        );
        assert_eq!(alice.current_epoch(), charlie.current_epoch());
        assert_ne!(alice.current_epoch(), reopened_bob.current_epoch());
        assert_eq!(reopened_bob.open(&before_update).unwrap(), b"before update");

        let dave_identity = InMemoryProvider::from_seed([0xd4; 32]);
        let (mut dave, dave_prekey) = GroupSession::new(group, &dave_identity).unwrap();
        alice.register_prekey(&dave_prekey).unwrap();
        charlie.register_prekey(&dave_prekey).unwrap();
        dave.register_prekey(&alice_prekey).unwrap();
        dave.register_prekey(&charlie_prekey).unwrap();

        let welcome = alice.add(dave.member()).unwrap();
        assert_eq!(welcome.direct.len(), 1);
        assert!(welcome.direct_for(dave.member()).is_some());
        charlie
            .process(alice.personae_root(), &welcome.control, None)
            .unwrap();
        let welcomed = dave
            .process(
                alice.personae_root(),
                &welcome.control,
                welcome.direct_for(dave.member()),
            )
            .unwrap();
        assert_eq!(welcomed.installed_epochs.len(), alice.epoch_count());
        assert_eq!(dave.current_epoch(), alice.current_epoch());
        assert_eq!(dave.members().unwrap(), alice.members().unwrap());
        assert_eq!(dave.open(&after_removal).unwrap(), b"after removal");
    }
}
