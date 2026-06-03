//! Follow-up B: the message (forward-secret) scheme, with a `ForwardSecureOrdering`
//! implemented over our event-DAG.
//!
//! Unlike the data scheme (which hands you a `GroupSecret` you can encrypt with
//! directly), the message scheme's forward-secret ratchet is only reachable
//! through the full `MessageGroup`, which demands three traits the integrator
//! must supply: `AckedGroupMembership`, `ForwardSecureOrdering`, and a message
//! type. Upstream only ships these as test-only, explicitly-"not production"
//! code (and the ordering helper it leans on is crate-private), so an external
//! adopter has to bring their own. This probe does exactly that, Mere-side:
//!
//! - `AckedDgm`: a minimal `AckedGroupMembership` (membership would come from
//!   cluster-path caps, as proven in the data-scheme probe).
//! - `DagOrderer`: a `ForwardSecureOrdering` driven by our event-DAG's causal
//!   links. A message is ready once its `previous` (parent) set has been
//!   processed. The probe delivers in causal order, so this stays a simple
//!   FIFO; the parent-set check is the same shape our real DAG already gives us.
//! - `DagMessage`: our broadcast envelope (a `ForwardSecureGroupMessage`).
//!
//! It then drives one forward-secret round trip of a real signed cabal operation
//! (Alice -> Bob) and confirms the decrypted payload still verifies.

use std::collections::{HashMap, HashSet, VecDeque};
use std::convert::Infallible;
use std::fmt::Debug;
use std::marker::PhantomData;

use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::{Body, Header, SigningKey, Timestamp};
use p2panda_encryption::Rng;
use p2panda_encryption::crypto::x25519::SecretKey;
use p2panda_encryption::key_bundle::Lifetime;
use p2panda_encryption::key_manager::KeyManager;
use p2panda_encryption::key_registry::KeyRegistry;
use p2panda_encryption::message_scheme::group::{GroupConfig, GroupEvent, GroupState, MessageGroup};
use p2panda_encryption::message_scheme::{ControlMessage, DirectMessage, Generation};
use p2panda_encryption::traits::{
    AckedGroupMembership, ForwardSecureGroupMessage, ForwardSecureMessageContent,
    ForwardSecureOrdering, IdentityHandle, OperationId, PreKeyManager,
};
use serde::{Deserialize, Serialize};

// ── Mere's event (unchanged from the data-scheme probe) ──────────────────────

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct CabalExt {
    cabal_id: [u8; 32],
    channel: String,
    post_type: u64,
    timestamp_ms: u64,
    parents: Vec<[u8; 32]>,
}

fn build_cabal_operation(cabal_id: [u8; 32], channel: &str, text: &str, key: &SigningKey) -> Vec<u8> {
    let body = Body::new(text.as_bytes());
    let mut header: Header<CabalExt> = Header {
        version: 1,
        verifying_key: key.verifying_key(),
        signature: None,
        payload_size: body.size(),
        payload_hash: Some(body.hash()),
        timestamp: Timestamp::now(),
        seq_num: 0,
        backlink: None,
        extensions: CabalExt {
            cabal_id,
            channel: channel.to_string(),
            post_type: 0,
            timestamp_ms: 0,
            parents: vec![],
        },
    };
    header.sign(key);
    encode_cbor(&header).expect("encode operation")
}

// ── Mere identity handles ────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct MereMember([u8; 32]);
impl IdentityHandle for MereMember {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct MereOp {
    sender: MereMember,
    seq: u64,
}
impl OperationId for MereOp {}

// ── Minimal AckedGroupMembership (membership would come from cluster-path caps) ─

#[derive(Clone, Debug, Serialize, Deserialize)]
struct AckedDgm<ID, OP> {
    _marker: PhantomData<(ID, OP)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct AckedDgmState<ID: IdentityHandle, OP> {
    my_id: ID,
    members: HashSet<ID>,
    _marker: PhantomData<OP>,
}

impl<ID: IdentityHandle, OP> AckedDgm<ID, OP> {
    fn init(my_id: ID) -> AckedDgmState<ID, OP> {
        AckedDgmState {
            my_id,
            members: HashSet::new(),
            _marker: PhantomData,
        }
    }
}

impl<ID, OP> AckedGroupMembership<ID, OP> for AckedDgm<ID, OP>
where
    ID: IdentityHandle + Serialize + for<'a> Deserialize<'a>,
    OP: OperationId + Serialize + for<'a> Deserialize<'a>,
{
    type State = AckedDgmState<ID, OP>;
    type Error = Infallible;

    fn create(my_id: ID, initial_members: &[ID]) -> Result<Self::State, Self::Error> {
        let mut members: HashSet<ID> = initial_members.iter().cloned().collect();
        members.insert(my_id);
        Ok(AckedDgmState {
            my_id,
            members,
            _marker: PhantomData,
        })
    }

    fn from_welcome(mut y: Self::State, y_welcome: Self::State) -> Result<Self::State, Self::Error> {
        y.members.extend(y_welcome.members);
        Ok(y)
    }

    fn add(mut y: Self::State, _adder: ID, added: ID, _op: OP) -> Result<Self::State, Self::Error> {
        y.members.insert(added);
        Ok(y)
    }

    fn remove(
        mut y: Self::State,
        _remover: ID,
        removed: &ID,
        _op: OP,
    ) -> Result<Self::State, Self::Error> {
        y.members.remove(removed);
        Ok(y)
    }

    fn ack(y: Self::State, _acker: ID, _op: OP) -> Result<Self::State, Self::Error> {
        Ok(y)
    }

    fn members_view(y: &Self::State, _viewer: &ID) -> Result<HashSet<ID>, Self::Error> {
        Ok(y.members.clone())
    }

    fn is_add(_y: &Self::State, _op: OP) -> bool {
        false
    }

    fn is_remove(_y: &Self::State, _op: OP) -> bool {
        false
    }
}

// ── The broadcast message (a ForwardSecureGroupMessage) ──────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
enum DagContent<DGM>
where
    DGM: Clone + AckedGroupMembership<MereMember, MereOp>,
{
    Application {
        ciphertext: Vec<u8>,
        generation: Generation,
    },
    Control {
        control: ControlMessage<MereMember, MereOp>,
        direct: Vec<DirectMessage<MereMember, MereOp, DGM>>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DagMessage<DGM>
where
    DGM: Clone + AckedGroupMembership<MereMember, MereOp>,
{
    seq: u64,
    sender: MereMember,
    /// Causal parents (our event-DAG links). The orderer gates readiness on them.
    previous: Vec<MereOp>,
    content: DagContent<DGM>,
}

impl<DGM> ForwardSecureGroupMessage<MereMember, MereOp, DGM> for DagMessage<DGM>
where
    DGM: Clone + AckedGroupMembership<MereMember, MereOp>,
{
    fn id(&self) -> MereOp {
        MereOp {
            sender: self.sender,
            seq: self.seq,
        }
    }

    fn sender(&self) -> MereMember {
        self.sender
    }

    fn content(&self) -> ForwardSecureMessageContent<MereMember, MereOp> {
        match &self.content {
            DagContent::Application {
                ciphertext,
                generation,
            } => ForwardSecureMessageContent::Application {
                ciphertext: ciphertext.clone(),
                generation: *generation,
            },
            DagContent::Control { control, .. } => {
                ForwardSecureMessageContent::Control(control.clone())
            }
        }
    }

    fn direct_messages(&self) -> Vec<DirectMessage<MereMember, MereOp, DGM>> {
        match &self.content {
            DagContent::Application { .. } => Vec::new(),
            DagContent::Control { direct, .. } => direct.clone(),
        }
    }
}

// ── ForwardSecureOrdering over our event-DAG ─────────────────────────────────

#[derive(Debug)]
struct DagOrderer<DGM> {
    _marker: PhantomData<DGM>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DagOrdererState<DGM>
where
    DGM: Clone + AckedGroupMembership<MereMember, MereOp>,
{
    my_id: MereMember,
    next_seq: u64,
    /// All messages we've authored or queued, by id.
    messages: HashMap<MereOp, DagMessage<DGM>>,
    /// Ids whose causal parents are all processed: ready to hand to the group.
    ready: VecDeque<MereOp>,
    /// Ids waiting on unmet parents.
    pending: Vec<MereOp>,
    /// Ids already processed (so we can resolve parent-readiness).
    processed: HashSet<MereOp>,
    welcome: Option<DagMessage<DGM>>,
}

impl<DGM> DagOrderer<DGM>
where
    DGM: Clone + AckedGroupMembership<MereMember, MereOp>,
{
    fn init(my_id: MereMember) -> DagOrdererState<DGM> {
        DagOrdererState {
            my_id,
            next_seq: 0,
            messages: HashMap::new(),
            ready: VecDeque::new(),
            pending: Vec::new(),
            processed: HashSet::new(),
            welcome: None,
        }
    }

    /// Promote any pending message whose parents are now all processed.
    fn promote_pending(y: &mut DagOrdererState<DGM>) {
        let mut made_progress = true;
        while made_progress {
            made_progress = false;
            let mut still_pending = Vec::new();
            for id in std::mem::take(&mut y.pending) {
                let deps_met = y
                    .messages
                    .get(&id)
                    .map(|m| m.previous.iter().all(|p| y.processed.contains(p)))
                    .unwrap_or(true);
                if deps_met {
                    y.ready.push_back(id);
                    made_progress = true;
                } else {
                    still_pending.push(id);
                }
            }
            y.pending = still_pending;
        }
    }
}

impl<DGM> ForwardSecureOrdering<MereMember, MereOp, DGM> for DagOrderer<DGM>
where
    DGM: Debug + Clone + AckedGroupMembership<MereMember, MereOp> + Serialize + for<'a> Deserialize<'a>,
{
    type State = DagOrdererState<DGM>;
    type Error = Infallible;
    type Message = DagMessage<DGM>;

    fn next_control_message(
        mut y: Self::State,
        control_message: &ControlMessage<MereMember, MereOp>,
        direct_messages: &[DirectMessage<MereMember, MereOp, DGM>],
    ) -> Result<(Self::State, Self::Message), Self::Error> {
        let message = DagMessage {
            seq: y.next_seq,
            sender: y.my_id,
            previous: y.processed.iter().cloned().collect(),
            content: DagContent::Control {
                control: control_message.clone(),
                direct: direct_messages.to_vec(),
            },
        };
        y.next_seq += 1;
        y.messages.insert(message.id(), message.clone());
        // Our own authored messages are immediately considered processed locally.
        y.processed.insert(message.id());
        Ok((y, message))
    }

    fn next_application_message(
        mut y: Self::State,
        generation: Generation,
        ciphertext: Vec<u8>,
    ) -> Result<(Self::State, Self::Message), Self::Error> {
        let message = DagMessage {
            seq: y.next_seq,
            sender: y.my_id,
            previous: y.processed.iter().cloned().collect(),
            content: DagContent::Application {
                ciphertext,
                generation,
            },
        };
        y.next_seq += 1;
        y.messages.insert(message.id(), message.clone());
        y.processed.insert(message.id());
        Ok((y, message))
    }

    fn queue(mut y: Self::State, message: &Self::Message) -> Result<Self::State, Self::Error> {
        let id = message.id();
        y.messages.insert(id, message.clone());
        // Ready iff every causal parent we don't already own has been processed.
        let deps_met = message
            .previous
            .iter()
            .all(|p| p.sender == y.my_id || y.processed.contains(p));
        if deps_met {
            y.ready.push_back(id);
        } else {
            y.pending.push(id);
        }
        Ok(y)
    }

    fn set_welcome(mut y: Self::State, message: &Self::Message) -> Result<Self::State, Self::Error> {
        // The welcome is processed directly by the group; record it as processed
        // so dependents become ready.
        y.welcome = Some(message.clone());
        y.processed.insert(message.id());
        Self::promote_pending(&mut y);
        Ok(y)
    }

    fn next_ready_message(
        mut y: Self::State,
    ) -> Result<(Self::State, Option<Self::Message>), Self::Error> {
        if y.welcome.is_none() {
            return Ok((y, None));
        }
        let next = y.ready.pop_front();
        match next {
            Some(id) => {
                y.processed.insert(id);
                Self::promote_pending(&mut y);
                let message = y.messages.get(&id).cloned();
                Ok((y, message))
            }
            None => Ok((y, None)),
        }
    }
}

// ── Drive: one forward-secret round trip of a cabal operation ────────────────

type MereGroup = GroupState<
    MereMember,
    MereOp,
    KeyRegistry<MereMember>,
    AckedDgm<MereMember, MereOp>,
    KeyManager,
    DagOrderer<AckedDgm<MereMember, MereOp>>,
>;

fn main() {
    let rng = Rng::default();

    let alice_sign = SigningKey::from_bytes(&[0xa1; 32]);
    let bob_sign = SigningKey::from_bytes(&[0xb0; 32]);
    let alice = MereMember(*alice_sign.verifying_key().as_bytes());
    let bob = MereMember(*bob_sign.verifying_key().as_bytes());

    // Encryption identities + ONE-TIME pre-key bundles (the message scheme needs
    // one-time prekeys for forward secrecy, unlike the data scheme's long-term).
    let alice_id = SecretKey::from_rng(&rng).unwrap();
    let bob_id = SecretKey::from_rng(&rng).unwrap();
    // init -> rotate in a long-term prekey -> generate the one-time bundle the
    // message scheme consumes for X3DH (the one-time bundle is certified by the
    // prekey, so the prekey must exist first).
    let alice_keys = KeyManager::init(&alice_id).unwrap();
    let alice_keys = KeyManager::rotate_prekey(alice_keys, Lifetime::default(), &rng).unwrap();
    let (alice_keys, alice_ot) = KeyManager::generate_onetime_bundle(alice_keys, &rng).unwrap();
    let bob_keys = KeyManager::init(&bob_id).unwrap();
    let bob_keys = KeyManager::rotate_prekey(bob_keys, Lifetime::default(), &rng).unwrap();
    let (bob_keys, bob_ot) = KeyManager::generate_onetime_bundle(bob_keys, &rng).unwrap();

    let alice_pki = KeyRegistry::init();
    let alice_pki = KeyRegistry::add_onetime_bundle(alice_pki, alice, alice_ot.clone()).unwrap();
    let alice_pki = KeyRegistry::add_onetime_bundle(alice_pki, bob, bob_ot.clone()).unwrap();
    let bob_pki = KeyRegistry::init();
    let bob_pki = KeyRegistry::add_onetime_bundle(bob_pki, alice, alice_ot).unwrap();
    let bob_pki = KeyRegistry::add_onetime_bundle(bob_pki, bob, bob_ot).unwrap();

    let cfg = GroupConfig::default();

    // Membership would come from cluster-path caps (proven in the data-scheme
    // probe); here both are initial members.
    let space_id = *blake3::hash(b"a-mere-moot-fs-space").as_bytes();

    let alice_group: MereGroup = MessageGroup::init(
        alice,
        alice_keys,
        alice_pki,
        AckedDgm::init(alice),
        DagOrderer::init(alice),
        cfg.clone(),
    );
    let bob_group: MereGroup = MessageGroup::init(
        bob,
        bob_keys,
        bob_pki,
        AckedDgm::init(bob),
        DagOrderer::init(bob),
        cfg,
    );

    // Alice creates the forward-secret group with Bob.
    let (alice_group, create_msg) =
        MessageGroup::create(alice_group, vec![alice, bob], &rng).expect("create");

    // Bob processes the create/welcome: establishes Alice's ratchet on his side.
    let (bob_group, _welcome_out) =
        MessageGroup::receive(bob_group, &create_msg, &rng).expect("bob receive create");

    // Alice sends a real signed cabal operation, forward-secret.
    let op_bytes = build_cabal_operation(space_id, "session", "ahoy, forward-secret", &alice_sign);
    let (_alice_group, app_msg) = MessageGroup::send(alice_group, &op_bytes).expect("alice send");

    // Bob receives + decrypts via the ratchet.
    let (_bob_group, output) =
        MessageGroup::receive(bob_group, &app_msg, &rng).expect("bob receive app");

    let mut decrypted: Option<Vec<u8>> = None;
    if let Some(output) = output {
        for event in output.events {
            if let GroupEvent::Application { plaintext, .. } = event {
                decrypted = Some(plaintext);
            }
        }
    }

    let decrypted = decrypted.expect("bob decrypted an application message");
    assert_eq!(decrypted, op_bytes, "operation survived the FS round trip intact");

    let header: Header<CabalExt> = decode_cbor(&decrypted[..]).expect("decode operation");
    assert!(header.verify(), "decrypted operation still verifies");
    assert_eq!(header.extensions.cabal_id, space_id);

    println!("forward-secret round trip:");
    println!("  alice created the group, bob processed the welcome");
    println!("  alice sent a signed cabal op under the message ratchet");
    println!("  bob decrypted {} bytes; op verifies = {}", decrypted.len(), header.verify());

    println!("\n--- message-scheme VERDICT ---");
    println!("The forward-secret scheme works with our types, but ONLY by supplying the full trait");
    println!("stack ourselves: AckedGroupMembership + ForwardSecureOrdering (over our event-DAG) + a");
    println!("message type. Upstream ships these as test-only / crate-private code, so this is real");
    println!("integration work, not a drop-in. The data scheme stays the low-friction path; this is");
    println!("the shape of the FS path when we need it (e.g. sensitive moot threads).");
}
