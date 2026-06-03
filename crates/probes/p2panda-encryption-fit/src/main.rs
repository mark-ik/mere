//! Probe: does p2panda-encryption's `traits` seam take Mere's types?
//!
//! Tests the "take the encryption engine, not the bundle" decision (see
//! design_docs/.../2026-06-01_p2panda_substrate_spike_plan.md). We adopt only
//! the published `p2panda-encryption` crate (the crypto), NOT the git-only
//! `p2panda-spaces` bundle (which would add `p2panda-auth`'s authorization model
//! that competes with our cluster-path capabilities).
//!
//! What it proves:
//! 1. **Our operation rides as the payload.** The thing we encrypt is a real
//!    p2panda-core `Operation` shaped exactly like a murm cabal post
//!    (`Header<CabalExt>`, BLAKE3, Ed25519-signed). After the E2EE round trip it
//!    still decodes and verifies.
//! 2. **Membership comes from our side.** The `GroupMembership` (DGM) trait is
//!    implemented here as `CapDgm`, fed by a stub `cap_members` standing in for
//!    Mere's cluster-path capabilities + tessera admission. p2panda-encryption
//!    has no built-in access-control model; the member set is ours to decide.
//! 3. **One data-scheme round trip.** Alice and Bob run the DCGKA key agreement
//!    to a shared `GroupSecret`, then a sealed cabal operation travels Alice ->
//!    Bob and decrypts.
//! 4. **Quarantine boundary.** The rest of Mere would talk to the `EncryptedSpace`
//!    trait, never to p2panda-encryption directly, so upstream churn stays put.

use std::collections::HashSet;
use std::convert::Infallible;
use std::marker::PhantomData;

use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::{Body, Header, Signature, SigningKey, Timestamp, VerifyingKey};
use p2panda_encryption::Rng;
use p2panda_encryption::crypto::x25519::SecretKey;
use p2panda_encryption::crypto::xchacha20::XAeadNonce;
use p2panda_encryption::data_scheme::dcgka::{Dcgka, DcgkaState, GroupSecretOutput, ProcessInput};
use p2panda_encryption::data_scheme::group_secret::{SecretBundle, SecretBundleState};
use p2panda_encryption::data_scheme::{decrypt_data, encrypt_data};
use p2panda_encryption::key_bundle::Lifetime;
use p2panda_encryption::key_manager::KeyManager;
use p2panda_encryption::key_registry::KeyRegistry;
use p2panda_encryption::traits::{GroupMembership, IdentityHandle, OperationId, PreKeyManager};
use serde::{Deserialize, Serialize};

const NONCE_LEN: usize = 24; // XAeadNonce = [u8; 24]

// ── Mere's event, unchanged ──────────────────────────────────────────────────
// CabalExt + the operation mirror exactly what `murmuring` now produces on the
// wire: a signed p2panda-core `Header<CabalExt>`. This is the payload we encrypt.

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct CabalExt {
    cabal_id: [u8; 32],
    channel: String,
    post_type: u64,
    timestamp_ms: u64,
    parents: Vec<[u8; 32]>,
}

/// Build + sign a cabal operation (a murm post on the wire), CBOR-encoded.
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

// ── Mere identities plugged into the encryption traits ───────────────────────

/// A group member handle = a member's cabal-identity public key bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct MereMember([u8; 32]);
impl IdentityHandle for MereMember {}

/// An operation id (author + per-author sequence). Stands in for a murm event id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct MereOp {
    author: MereMember,
    seq: u64,
}
impl OperationId for MereOp {}

// ── Authorization is ours: a cluster-path-capability DGM ─────────────────────
// Same shape as the crate's reference DGM, but framed as the seam where Mere's
// cluster-path capabilities (Probe 2) + tessera admission decide membership.

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct CapDgm<ID, OP> {
    _marker: PhantomData<(ID, OP)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct CapDgmState<ID: IdentityHandle, OP> {
    my_id: ID,
    members: HashSet<ID>,
    _marker: PhantomData<OP>,
}

impl<ID: IdentityHandle, OP> CapDgm<ID, OP> {
    fn init(my_id: ID) -> CapDgmState<ID, OP> {
        CapDgmState {
            my_id,
            members: HashSet::new(),
            _marker: PhantomData,
        }
    }
}

impl<ID, OP> GroupMembership<ID, OP> for CapDgm<ID, OP>
where
    ID: IdentityHandle + Serialize + for<'a> Deserialize<'a>,
    OP: OperationId + Serialize + for<'a> Deserialize<'a>,
{
    type State = CapDgmState<ID, OP>;
    type Error = Infallible;

    fn create(my_id: ID, initial_members: &[ID]) -> Result<Self::State, Self::Error> {
        Ok(CapDgmState {
            my_id,
            members: HashSet::from_iter(initial_members.iter().cloned()),
            _marker: PhantomData,
        })
    }

    fn from_welcome(my_id: ID, y: Self::State) -> Result<Self::State, Self::Error> {
        Ok(CapDgmState {
            my_id,
            members: y.members,
            _marker: PhantomData,
        })
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

    fn members(y: &Self::State) -> Result<HashSet<ID>, Self::Error> {
        Ok(y.members.clone())
    }
}

// ── Authorization source: signed cluster-path capabilities (from Probe 2) ────
// Membership is decided by Mere's cluster-path caps (Meadowcap-shaped, BLAKE3),
// not by p2panda. A space is a (moot, cluster-path); a member holds a valid cap
// granting Read over that path. This is the real shape from the willow-cluster-cap
// probe, ported onto p2panda-core's ed25519 so the probe uses one key family.

type ClusterPath = Vec<u32>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Read,
    Write,
}

/// A granted area: which subspace (author) and which cluster-path prefix.
#[derive(Clone)]
struct Area {
    subspace: Option<VerifyingKey>,
    path_prefix: ClusterPath,
}

impl Area {
    /// Path-prefix containment: does this area cover `path`?
    fn includes_path(&self, path: &ClusterPath) -> bool {
        self.path_prefix.len() <= path.len()
            && path[..self.path_prefix.len()] == self.path_prefix[..]
    }
}

struct Delegation {
    area: Area,
    receiver: VerifyingKey,
    signature: Signature,
}

/// A signed capability chain: the moot owner roots it, delegations narrow it.
struct Capability {
    namespace: [u8; 32], // moot id
    mode: Mode,
    initial_area: Area,
    initial_receiver: VerifyingKey,
    root_signature: Signature,
    delegations: Vec<Delegation>,
}

/// Canonical signed bytes at each step: namespace || mode || area || receiver.
fn grant_msg(namespace: &[u8; 32], mode: Mode, area: &Area, receiver: &VerifyingKey) -> Vec<u8> {
    let mut m = Vec::new();
    m.extend_from_slice(namespace);
    m.push(if matches!(mode, Mode::Write) { 1 } else { 0 });
    match &area.subspace {
        Some(pk) => {
            m.push(1);
            m.extend_from_slice(pk.as_bytes());
        }
        None => m.push(0),
    }
    for c in &area.path_prefix {
        m.extend_from_slice(&c.to_le_bytes());
    }
    m.push(0xff); // path terminator
    m.extend_from_slice(receiver.as_bytes());
    m
}

impl Capability {
    fn root(
        namespace: [u8; 32],
        mode: Mode,
        owner: &SigningKey,
        initial_area: Area,
        initial_receiver: VerifyingKey,
    ) -> Self {
        let msg = grant_msg(&namespace, mode, &initial_area, &initial_receiver);
        Self {
            namespace,
            mode,
            initial_area,
            initial_receiver,
            root_signature: owner.sign(&msg),
            delegations: Vec::new(),
        }
    }

    fn current_holder(&self) -> VerifyingKey {
        self.delegations
            .last()
            .map(|d| d.receiver)
            .unwrap_or(self.initial_receiver)
    }

    /// Verify the full signed chain against the moot `owner`.
    fn is_valid(&self, owner: &VerifyingKey) -> bool {
        let root_msg = grant_msg(&self.namespace, self.mode, &self.initial_area, &self.initial_receiver);
        if !owner.verify(&root_msg, &self.root_signature) {
            return false;
        }
        let mut prior_holder = self.initial_receiver;
        for d in &self.delegations {
            let msg = grant_msg(&self.namespace, self.mode, &d.area, &d.receiver);
            if !prior_holder.verify(&msg, &d.signature) {
                return false;
            }
            prior_holder = d.receiver;
        }
        true
    }

    fn grants_read_at(&self, owner: &VerifyingKey, path: &ClusterPath) -> bool {
        matches!(self.mode, Mode::Read)
            && self.is_valid(owner)
            && self
                .delegations
                .last()
                .map(|d| &d.area)
                .unwrap_or(&self.initial_area)
                .includes_path(path)
    }
}

/// Mere's admission layer: the space's members are the holders of valid Read caps
/// (rooted by the moot owner) covering the space's cluster-path. In production,
/// tessera gates who the owner roots caps for in the first place; p2panda decides
/// nothing here.
fn cap_members(owner: &VerifyingKey, space_path: &ClusterPath, caps: &[Capability]) -> Vec<MereMember> {
    caps.iter()
        .filter(|c| c.grants_read_at(owner, space_path))
        .map(|c| MereMember(*c.current_holder().as_bytes()))
        .collect()
}

// ── The Mere-side seam (quarantine boundary) ─────────────────────────────────

/// The rest of Mere talks to this, never to p2panda-encryption directly.
trait EncryptedSpace {
    /// Seal application bytes (e.g. a murm operation) for the space's members.
    fn seal(&self, plaintext: &[u8], rng: &Rng) -> Vec<u8>;
    /// Open bytes sealed for this space, or `None` if we lack the key.
    fn open(&self, envelope: &[u8]) -> Option<Vec<u8>>;
}

/// Data-scheme realization: encrypts under the group's latest `GroupSecret`.
/// The nonce travels as a prefix on the envelope.
struct DataSchemeSpace {
    bundle: SecretBundleState,
}

impl EncryptedSpace for DataSchemeSpace {
    fn seal(&self, plaintext: &[u8], rng: &Rng) -> Vec<u8> {
        let nonce: XAeadNonce = rng.random_array().expect("nonce");
        let secret = self.bundle.latest().expect("a group secret");
        let ciphertext = encrypt_data(plaintext, secret, nonce).expect("encrypt");
        let mut envelope = nonce.to_vec();
        envelope.extend_from_slice(&ciphertext);
        envelope
    }

    fn open(&self, envelope: &[u8]) -> Option<Vec<u8>> {
        if envelope.len() < NONCE_LEN {
            return None;
        }
        let (nonce_bytes, ciphertext) = envelope.split_at(NONCE_LEN);
        let nonce: XAeadNonce = nonce_bytes.try_into().ok()?;
        // The probe uses the latest secret; production would index by the
        // GroupSecretId carried alongside the ciphertext.
        decrypt_data(ciphertext, self.bundle.latest()?, nonce).ok()
    }
}

// ── DCGKA setup helpers ──────────────────────────────────────────────────────

type MereDcgka =
    DcgkaState<MereMember, MereOp, KeyRegistry<MereMember>, CapDgm<MereMember, MereOp>, KeyManager>;

fn main() {
    // `Rng::from_seed` is test-only; the public constructor is getrandom-seeded.
    let rng = Rng::default();

    // Alice and Bob each have a cabal signing identity (Ed25519, for operations)
    // and an encryption identity (X25519, for key agreement). In Mere both come
    // from the identity vault; here we make them from seeds.
    let alice_sign = SigningKey::from_bytes(&[0xa1; 32]);
    let bob_sign = SigningKey::from_bytes(&[0xb0; 32]);
    let alice = MereMember(*alice_sign.verifying_key().as_bytes());
    let bob = MereMember(*bob_sign.verifying_key().as_bytes());

    let alice_id_secret = SecretKey::from_rng(&rng).unwrap();
    let bob_id_secret = SecretKey::from_rng(&rng).unwrap();

    // Public setup path (the one-shot `init_and_generate_prekey` is test-only):
    // init the manager, then rotate in a first pre-key, then publish the bundle.
    let alice_keys = KeyManager::init(&alice_id_secret).unwrap();
    let alice_keys = KeyManager::rotate_prekey(alice_keys, Lifetime::default(), &rng).unwrap();
    let bob_keys = KeyManager::init(&bob_id_secret).unwrap();
    let bob_keys = KeyManager::rotate_prekey(bob_keys, Lifetime::default(), &rng).unwrap();
    let alice_prekeys = KeyManager::prekey_bundle(&alice_keys).unwrap();
    let bob_prekeys = KeyManager::prekey_bundle(&bob_keys).unwrap();

    // Each peer's key registry holds both members' published pre-key bundles.
    let alice_pki = KeyRegistry::init();
    let alice_pki = KeyRegistry::add_longterm_bundle(alice_pki, alice, alice_prekeys.clone()).unwrap();
    let alice_pki = KeyRegistry::add_longterm_bundle(alice_pki, bob, bob_prekeys.clone()).unwrap();
    let bob_pki = KeyRegistry::init();
    let bob_pki = KeyRegistry::add_longterm_bundle(bob_pki, alice, alice_prekeys.clone()).unwrap();
    let bob_pki = KeyRegistry::add_longterm_bundle(bob_pki, bob, bob_prekeys.clone()).unwrap();

    // The space is a cluster-path within a moot; membership comes from signed
    // cluster-path caps (Probe 2), not a hardcoded set.
    let moot_owner = SigningKey::from_bytes(&[0x11; 32]);
    let moot_id = *blake3::hash(b"a-mere-moot").as_bytes();
    let space_path: ClusterPath = vec![3, 7]; // cluster 3 / sub-cluster 7
    let space_id = {
        let mut h = blake3::Hasher::new();
        h.update(&moot_id);
        for c in &space_path {
            h.update(&c.to_le_bytes());
        }
        *h.finalize().as_bytes()
    };

    // The moot owner roots a Read cap for each member over the space's path.
    // (Bob could equally hold a cap delegated from Alice; the DGM only sees the
    // verified holder set.)
    let alice_cap = Capability::root(
        moot_id,
        Mode::Read,
        &moot_owner,
        Area { subspace: None, path_prefix: space_path.clone() },
        alice_sign.verifying_key(),
    );
    let bob_cap = Capability::root(
        moot_id,
        Mode::Read,
        &moot_owner,
        Area { subspace: None, path_prefix: space_path.clone() },
        bob_sign.verifying_key(),
    );
    let caps = vec![alice_cap, bob_cap];
    let members = cap_members(&moot_owner.verifying_key(), &space_path, &caps);
    assert_eq!(members.len(), 2, "both cap holders are admitted as members");
    assert!(members.contains(&alice) && members.contains(&bob));

    // An outsider holding no cap is not admitted: the cap gates membership.
    let mallory_sign = SigningKey::from_bytes(&[0x6d; 32]);
    let mallory = MereMember(*mallory_sign.verifying_key().as_bytes());
    assert!(!members.contains(&mallory), "no cap over the path, no membership");

    let alice_dcgka: MereDcgka = Dcgka::init(alice, alice_keys, alice_pki, CapDgm::init(alice));
    let bob_dcgka: MereDcgka = Dcgka::init(bob, bob_keys, bob_pki, CapDgm::init(bob));

    // Alice creates the group around the cap-defined member set.
    let alice_bundle = SecretBundle::init();
    let secret0 = SecretBundle::generate(&alice_bundle, &rng).unwrap();
    let (alice_dcgka, out) = Dcgka::create(alice_dcgka, members.clone(), &secret0, &rng).unwrap();
    let control = out.control_message;
    let direct = out.direct_messages;

    let (_alice_dcgka, _) = Dcgka::process(
        alice_dcgka,
        ProcessInput {
            seq: MereOp { author: alice, seq: 0 },
            sender: alice,
            control_message: control.clone(),
            direct_message: None,
        },
    )
    .unwrap();
    let alice_bundle = SecretBundle::insert(alice_bundle, secret0.clone());

    // Bob processes Alice's "create" (with his direct welcome message) and
    // derives the same group secret.
    let dm_for_bob = direct
        .into_iter()
        .find(|dm| dm.recipient == bob)
        .expect("welcome direct message for bob");
    let bob_bundle = SecretBundle::init();
    let (_bob_dcgka, bob_out) = Dcgka::process(
        bob_dcgka,
        ProcessInput {
            seq: MereOp { author: alice, seq: 0 },
            sender: alice,
            control_message: control,
            direct_message: Some(dm_for_bob),
        },
    )
    .unwrap();
    let GroupSecretOutput::Secret(bob_secret0) = bob_out else {
        panic!("bob expected a group secret from the create message");
    };
    let bob_bundle = SecretBundle::insert(bob_bundle, bob_secret0.clone());

    assert_eq!(secret0, bob_secret0, "alice and bob share one group secret");

    // ── The actual round trip: a Mere cabal operation, end-to-end encrypted ──
    let alice_space = DataSchemeSpace { bundle: alice_bundle };
    let bob_space = DataSchemeSpace { bundle: bob_bundle };

    let op_bytes = build_cabal_operation(space_id, "session", "ahoy from alice", &alice_sign);

    // Alice seals the operation; only members can open it.
    let envelope = alice_space.seal(&op_bytes, &rng);
    assert_ne!(envelope, op_bytes, "payload is actually encrypted");

    // Bob, a member, opens it.
    let opened = bob_space.open(&envelope).expect("bob can open as a member");
    assert_eq!(opened, op_bytes, "operation survived the E2EE round trip intact");

    // And the recovered operation still verifies + belongs to the space.
    let header: Header<CabalExt> = decode_cbor(&opened[..]).expect("decode operation");
    assert!(header.verify(), "decrypted operation still verifies");
    assert_eq!(header.extensions.cabal_id, space_id, "operation is addressed to the space");
    assert_eq!(header.verifying_key.as_bytes(), &alice.0, "author is alice");

    // An outsider with no group secret cannot open the envelope.
    let outsider = DataSchemeSpace { bundle: SecretBundle::init() };
    assert!(outsider.open(&envelope).is_none(), "non-member cannot decrypt");

    println!("space_id (cabal/moot)   = {}", hex(&space_id));
    println!("members (from signed caps) = {} holders over cluster-path {:?}", members.len(), space_path);
    println!("sealed envelope         = {} bytes (24-byte nonce + ciphertext)", envelope.len());
    println!("opened == original op   = {}", opened == op_bytes);
    println!("recovered op verifies   = {}", header.verify());

    println!("\n--- p2panda-encryption-fit VERDICT (data scheme) ---");
    println!("p2panda-encryption is adoptable as the crypto engine WITHOUT the spaces bundle:");
    println!("- authorization is pluggable: our `CapDgm` (cluster-path caps + tessera) decided membership");
    println!("- our murm operation rode as the encrypted payload and verified after the round trip");
    println!("- the DCGKA key agreement gave alice + bob one shared GroupSecret; a non-member can't open");
    println!("- everything sits behind the Mere-side `EncryptedSpace` trait (upstream churn quarantined)");

    println!("\nData scheme vs message scheme (integration friction, found while probing):");
    println!("- DATA scheme is low-friction: `encrypt_data`/`decrypt_data` take a `GroupSecret` you");
    println!("  hold directly, so a thin DGM (membership) is the only trait you must supply.");
    println!("- MESSAGE scheme (forward secrecy) has NO external shortcut: its ratchet seed type");
    println!("  (`Secret`) is constructible only inside the crate, so you must run the full");
    println!("  `MessageGroup` with a `ForwardSecureOrdering` state machine + `AckedGroupMembership`.");
    println!("  That ordering layer is the heavy part; deferred. Data scheme is the path for now.");
    println!("- Also note: `Rng::from_seed`, `KeyManager::init_and_generate_prekey`, and most");
    println!("  `from_bytes` ctors are test-only; the public setup path is `init` + `rotate_prekey`.");
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
