//! The WebRTC door: how a browser holding an invitation becomes an admitted
//! Personae subject, and every way that does not happen.
//!
//! [`browser_carrier`](crate::browser_carrier) is the precedent. It brokers a
//! challenge-bound [`notochord::SessionHello`] for a local extension over a
//! trusted OS pipe, and the trust it leans on is the pipe: the browser had to
//! launch this process to speak to it at all. WebRTC has no such pipe. Its
//! signaling path is a relay nobody authenticated, so the door here adds the
//! two things that channel never needed:
//!
//! - **host authentication.** The host signs the link-challenge transcript,
//!   and the browser verifies that signature against the public key its
//!   invitation named *before* it trusts the channel with anything.
//! - **a redemption proof.** The invitation's capability is a secret seed the
//!   browser proves possession of, bound to that same transcript and to the
//!   ephemeral public subject it just generated. The host keeps only a
//!   verifier and a use count.
//!
//! Everything after that is the machinery that already exists. The redemption
//! mints an ordinary [`SignedDelegationCertificate`], the browser issues an
//! ordinary hello through [`crate::admission::open_session`], and the door
//! runs the ordinary [`crate::admission::admit_session`] over facts built by
//! the one audited N1 adapter. WebRTC needs no new proof grammar — only a link
//! identifier to put in the existing one, which is what
//! [`webrtc_carrier::LinkChallenge`] derives.
//!
//! ## What the host never holds
//!
//! Two secrets never exist on the host side of this module, and the signatures
//! say so rather than the prose:
//!
//! - the browser's **private subject key**. Every host-side entry point here
//!   takes a subject as `[u8; 32]` — a public key — so there is no parameter
//!   through which a private key could arrive, mistakenly or otherwise.
//! - the invitation's **redemption seed**. [`issue_invite`] generates it,
//!   hands it out inside the descriptor that becomes the URL fragment, and
//!   retains only [`RedemptionState`] — an Ed25519 *public* verifier plus a
//!   use count and an expiry. A host store read by an attacker therefore
//!   yields no power to mint a redemption for an invitation it holds.
//!
//! ## Signing discipline
//!
//! Nothing here signs with a bare master key. Both host signatures — the
//! invite descriptor and the challenge transcript — use one salted derived
//! keypair ([`WEBRTC_HOST_SIGNING_SALT`]) and travel with the
//! [`DerivedKeyAttestation`] that binds it to the master key, verified exactly
//! the way [`SignedDelegationCertificate`] verifies its own: the attestation
//! checks against the salt, the attested master equals the key the verifier
//! expected, and the derived public key verifies the signature. Two signatures
//! share the key because the *messages* are domain-separated in
//! `webrtc-carrier`, not because the key is.
//!
//! ## Sans-I/O
//!
//! Nothing in this module reads or writes. The host functions take bytes and
//! return bytes; the client functions are pure enough for a `wasm32` browser
//! build to call the same code the native tests do. The transport that carries
//! those bytes is [`webrtc_carrier::native`] on this side and
//! `webrtc_carrier::browser` on the other, and neither is named here.

use notochord::{
    AdmittedPrincipal, DenyReason, HandshakeError, LocalNetworkPolicy, NetworkId, ProfileRef,
    RequestedAction, RevocationLedger, SessionFacts, SessionHello, TrafficClass,
};
use personae::delegation::{
    CapabilityScope, DelegationCertificate, DelegationError, DelegationParent,
    SignedDelegationCertificate,
};
use personae::{DerivedKeyAttestation, Ed25519Keypair, Ed25519PublicKey, Ed25519Signature};
use personae::{IdentityError, IdentityProvider};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use transport::{AcceptedSession, Alpn, IngressContext, initiator_link_binding};
use webrtc_carrier::{
    InviteError, InviteId, InviteV1, LinkChallenge, ReleaseRefV1, challenge_signature_bytes,
    redemption_signing_bytes,
};

use crate::admission::{PROJECTION_PROTOCOL, connect_action, open_session};

/// The one derivation salt behind every host signature this door produces.
///
/// One key, two signatures: the invite descriptor and the challenge
/// transcript. They cannot be confused for one another because
/// `webrtc-carrier` domain-separates the *messages*
/// ([`webrtc_carrier::INVITE_DESCRIPTOR_DOMAIN`] versus
/// [`webrtc_carrier::HOST_CHALLENGE_SIGNATURE_DOMAIN`]), which is where that
/// separation belongs — a second derived key would say the same thing twice
/// and give a host two keys to lose.
pub const WEBRTC_HOST_SIGNING_SALT: &[u8] = b"mere.graphshell/webrtc-host-signing/v1";

/// What the host is offering, minus everything [`issue_invite`] generates.
///
/// The rendezvous id and the redemption seed are deliberately absent: an
/// invitation whose secret its caller chose is an invitation whose secret its
/// caller can reuse, so the door mints both from [`OsRng`] and never accepts
/// them as arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InviteTerms {
    /// The network the redeemed delegation is scoped to.
    pub network: NetworkId,
    /// The profile the browser must claim.
    pub profile: ProfileRef,
    /// The single action this invitation permits.
    pub action: RequestedAction,
    /// Last millisecond at which the invitation may be redeemed.
    pub expires_at_ms: u64,
    /// How many sessions one invitation may open. One is the interesting case.
    pub max_uses: u32,
    /// The Luggage release the browser is being pointed at. A claim, bound by
    /// the host signature so signaling cannot substitute another — never a
    /// grant of executable trust, which stays a separate loader decision.
    pub release: ReleaseRefV1,
}

impl InviteTerms {
    /// Terms for Graphshell's projection connect action.
    ///
    /// The action is [`crate::admission::connect_action`] rather than a
    /// parameter, because that is the only action this port's door serves; a
    /// host offering something else builds [`InviteTerms`] directly and takes
    /// responsibility for the policy rule that matches it.
    pub fn projection(
        network: NetworkId,
        profile: ProfileRef,
        expires_at_ms: u64,
        max_uses: u32,
        release: ReleaseRefV1,
    ) -> Self {
        Self {
            network,
            profile,
            action: connect_action(),
            expires_at_ms,
            max_uses,
            release,
        }
    }
}

/// A host-signed invitation descriptor.
///
/// The signature covers [`InviteV1::signing_bytes`], which binds every field
/// of the invitation including its [`ReleaseRefV1`]. A signaling relay that
/// rewrites the release reference, the permitted action, the expiry, or the
/// use ceiling produces a descriptor that no longer verifies.
///
/// `Debug` only, and that is [`InviteV1`]'s decision rather than this type's:
/// it holds a redemption seed, so it derives neither `Clone` (which would
/// multiply copies of a secret) nor `Serialize` (which would let one reach a
/// log through any structured logger). Carrying a descriptor on a wire means
/// [`InviteV1::to_fragment`] beside [`SignedInviteDescriptor::signature`] and
/// the attestation, assembled deliberately by whoever owns that wire.
#[derive(Debug)]
pub struct SignedInviteDescriptor {
    /// The invitation committed by the signature.
    pub invite: InviteV1,
    /// Master-signed proof of the derived key that signed it.
    pub signer: DerivedKeyAttestation,
    signature: Vec<u8>,
}

impl SignedInviteDescriptor {
    /// Reassemble a descriptor a browser received over its own wire.
    pub fn from_parts(invite: InviteV1, signer: DerivedKeyAttestation, signature: Vec<u8>) -> Self {
        Self {
            invite,
            signer,
            signature,
        }
    }

    /// Verify the attestation, the host binding, and the signature.
    ///
    /// `expected_host_key` is the key the *reader* independently expects. It
    /// must equal both the key the invitation declares and the master key the
    /// attestation carries, so a descriptor whose own `expected_host_key`
    /// field was rewritten to name the attacker's key fails here rather than
    /// verifying happily against itself.
    pub fn verify(&self, expected_host_key: &[u8; 32]) -> bool {
        if self.invite.expected_host_key() != expected_host_key {
            return false;
        }
        verify_derived(
            &self.signer,
            expected_host_key,
            &self.signature,
            &self.invite.signing_bytes(),
        )
    }

    /// The signature bytes, for a caller carrying the descriptor on its own
    /// wire.
    pub fn signature(&self) -> &[u8] {
        &self.signature
    }
}

/// The secret-bearing half of what the host keeps, with the secret removed.
///
/// The redemption seed leaves inside the descriptor and is dropped here; what
/// remains is its Ed25519 *public* verifier, a use count, and an expiry.
/// Stealing this store buys an attacker the ability to check a redemption,
/// never to produce one.
///
/// Precisely, because the difference matters at rest: an invitation's *public*
/// terms — network, profile, triple, expiry, use ceiling — are not secrets and
/// a host keeps them however it keeps any other record. What must never be
/// persisted in the clear is the seed, and [`InviteV1`] carries the seed
/// bundled with those public terms, so a host that persists a whole
/// [`InviteV1`] is persisting a secret. That is a storage decision this door
/// does not make for it; what the door does is hand the seed back exactly once
/// and keep no copy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RedemptionState {
    verifier: Ed25519PublicKey,
    remaining_uses: u32,
    expires_at_ms: u64,
}

impl RedemptionState {
    /// Rebuild retained state, for a host reloading it from its own store.
    pub fn new(verifier: Ed25519PublicKey, remaining_uses: u32, expires_at_ms: u64) -> Self {
        Self {
            verifier,
            remaining_uses,
            expires_at_ms,
        }
    }

    /// The public key a redemption proof must verify against.
    pub fn verifier(&self) -> Ed25519PublicKey {
        self.verifier
    }

    /// Sessions this invitation may still open.
    pub fn remaining_uses(&self) -> u32 {
        self.remaining_uses
    }

    /// Last millisecond at which a redemption is accepted.
    pub fn expires_at_ms(&self) -> u64 {
        self.expires_at_ms
    }
}

/// One issued invitation: the half that leaves, and the half that stays.
#[derive(Debug)]
pub struct InviteIssue {
    /// The signed descriptor. This is the URL fragment's payload — it carries
    /// the redemption seed — and the host does not keep it.
    pub descriptor: SignedInviteDescriptor,
    /// The only part the host retains. See [`RedemptionState`].
    pub redemption: RedemptionState,
}

/// A host-signed link-challenge transcript.
///
/// The browser checks this with [`verify_host_challenge`] before it trusts the
/// data channel with a redemption proof. Signing the transcript rather than a
/// bare nonce is what makes the signature specific to *this* connection: the
/// transcript binds both DTLS fingerprints, so a relay that terminated the
/// channel itself cannot replay a host signature onto it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostChallengeSignature {
    /// Master-signed proof of the derived key that signed the transcript.
    pub signer: DerivedKeyAttestation,
    signature: Vec<u8>,
}

impl HostChallengeSignature {
    /// The signature bytes.
    pub fn signature(&self) -> &[u8] {
        &self.signature
    }
}

/// Why a redemption was refused.
///
/// Deliberately small and deliberately public. Each variant is a state the
/// host can state without disclosing anything: whether an invitation exists,
/// when it was minted, or what its verifier is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RedemptionRefusal {
    /// The invitation's expiry has passed.
    #[error("the invitation expired")]
    Expired,
    /// The invitation's use ceiling is spent.
    #[error("the invitation has no remaining uses")]
    Exhausted,
    /// The proof did not verify against the retained verifier, this
    /// transcript, and this subject.
    #[error("the redemption proof did not verify")]
    BadProof,
}

/// Failures on the host side of the door, before any admission decision.
#[derive(Debug, thiserror::Error)]
pub enum DoorError {
    /// The invitation payload is not constructible.
    #[error("invitation rejected: {0}")]
    Invite(#[from] InviteError),
    /// A delegation statement could not be issued.
    #[error("delegation rejected: {0}")]
    Delegation(#[from] DelegationError),
    /// The identity provider could not derive or attest its signing key.
    #[error("identity provider could not produce the host signing key: {0}")]
    Identity(#[from] IdentityError),
    /// The invitation names another host.
    #[error("the invitation names another host key")]
    WrongHost,
    /// The invitation's own expiry has already passed, so no delegation minted
    /// under it could be usable.
    #[error("the invitation expired before the delegation was minted")]
    InviteExpired,
}

// ---------------------------------------------------------------------------
// Host side
// ---------------------------------------------------------------------------

/// Mint one invitation: a fresh rendezvous id, a fresh redemption secret, and
/// the host's signature over the whole descriptor.
///
/// The seed is generated here and immediately split in two directions. Its
/// public half becomes [`RedemptionState::verifier`], which is what the host
/// keeps; the seed itself travels inside the returned descriptor, which the
/// caller is expected to render into a URL fragment and then drop. Nothing in
/// this module writes it anywhere else, and the plan's requirement that logs,
/// referrers and signaling records never contain the fragment is a property of
/// what the caller does with `descriptor`, not something this function can
/// enforce for it.
pub fn issue_invite<P: IdentityProvider>(
    provider: &P,
    terms: &InviteTerms,
) -> Result<InviteIssue, DoorError> {
    let mut rendezvous = [0u8; webrtc_carrier::INVITE_ID_BYTES];
    OsRng.fill_bytes(&mut rendezvous);
    let mut seed = [0u8; 32];
    OsRng.fill_bytes(&mut seed);

    let host_key = provider.master_public_key().to_bytes();
    let verifier = Ed25519Keypair::from_seed(seed).public_key();

    let invite = InviteV1::new(
        InviteId::from_bytes(rendezvous),
        seed,
        host_key,
        terms.network.0,
        terms.profile.id.clone(),
        // Notochord counts profile revisions in `u32`; the invite wire field
        // is `u64`. Widening is lossless and stated here rather than left to
        // an inference surprise at the call site.
        u64::from(terms.profile.revision),
        terms.action.domain.clone(),
        terms.action.path.clone(),
        terms.action.action.clone(),
        terms.expires_at_ms,
        terms.max_uses,
        terms.release,
    )?;

    let signer = provider.attest_derived_key(WEBRTC_HOST_SIGNING_SALT)?;
    let signature = provider
        .derive_keypair(WEBRTC_HOST_SIGNING_SALT)?
        .sign(&invite.signing_bytes())
        .to_bytes()
        .to_vec();

    Ok(InviteIssue {
        descriptor: SignedInviteDescriptor {
            invite,
            signer,
            signature,
        },
        redemption: RedemptionState {
            verifier,
            remaining_uses: terms.max_uses,
            expires_at_ms: terms.expires_at_ms,
        },
    })
}

/// Sign the link-challenge transcript this connection derived.
///
/// The host writes the result to the browser as the first thing on the data
/// channel. Until the browser has checked it with [`verify_host_challenge`],
/// the channel is an anonymous relay's word for who is on the other end.
pub fn sign_challenge<P: IdentityProvider>(
    provider: &P,
    challenge: &LinkChallenge,
) -> Result<HostChallengeSignature, DoorError> {
    let signer = provider.attest_derived_key(WEBRTC_HOST_SIGNING_SALT)?;
    let signature = provider
        .derive_keypair(WEBRTC_HOST_SIGNING_SALT)?
        .sign(&challenge_signature_bytes(challenge))
        .to_bytes()
        .to_vec();
    Ok(HostChallengeSignature { signer, signature })
}

/// Spend one use of an invitation against this transcript and this subject.
///
/// Fails closed on every axis and in one direction only: the use count is
/// decremented after the proof verifies, never before, so a refused redemption
/// costs the invitation nothing and a wrong proof cannot exhaust someone
/// else's invitation.
pub fn redeem(
    state: &mut RedemptionState,
    challenge: &LinkChallenge,
    subject: &[u8; 32],
    proof: &[u8; 64],
    now_ms: u64,
) -> Result<(), RedemptionRefusal> {
    if now_ms > state.expires_at_ms {
        return Err(RedemptionRefusal::Expired);
    }
    if state.remaining_uses == 0 {
        return Err(RedemptionRefusal::Exhausted);
    }
    let message = redemption_signing_bytes(challenge, subject);
    if !state
        .verifier
        .verify(&message, &Ed25519Signature::from_bytes(proof))
    {
        return Err(RedemptionRefusal::BadProof);
    }
    state.remaining_uses -= 1;
    Ok(())
}

/// Mint the leaf delegation a redeemed invitation buys.
///
/// The scope is the invitation's exact triple and nothing wider: the domain,
/// path and action the descriptor was signed with, over the invitation's own
/// network as the resource. `remaining_delegation_depth` is zero, so the
/// browser cannot pass the grant on.
///
/// `root_authority` is the host's local trust anchor — the same value its
/// [`crate::carrier::projection_policy`] names in a
/// [`notochord::TrustedRoot`]. It is not in the invitation, and deliberately
/// so: an invitation that carried its own root would be an invitation that
/// chose which authority admits it.
///
/// `invite` is the host's own copy, not the browser's. Taking the invitation
/// the far end presented would let a client choose the scope it is granted,
/// which is the whole thing the redemption proof exists to prevent — the proof
/// says *which* invitation is being spent, and the host's own record of that
/// invitation says what it is worth.
#[allow(clippy::too_many_arguments)]
pub fn mint_delegation<P: IdentityProvider>(
    provider: &P,
    root_authority: [u8; 32],
    subject: [u8; 32],
    invite: &InviteV1,
    now_ms: u64,
    ttl_ms: u64,
    nonce: [u8; 32],
) -> Result<SignedDelegationCertificate, DoorError> {
    let issuer = provider.master_public_key().to_bytes();
    if &issuer != invite.expected_host_key() {
        return Err(DoorError::WrongHost);
    }
    if now_ms > invite.expires_at_ms() {
        return Err(DoorError::InviteExpired);
    }
    // A session grant never outlives the invitation that bought it. The ttl is
    // the host's own bound on one session; the invitation's expiry is the
    // owner's bound on the whole offer, and the tighter of the two wins.
    let expires_at_ms = now_ms.saturating_add(ttl_ms).min(invite.expires_at_ms());

    Ok(SignedDelegationCertificate::issue(
        provider,
        DelegationCertificate::new(
            DelegationParent::Root(root_authority),
            issuer,
            subject,
            CapabilityScope {
                domain: invite.domain().to_string(),
                resource: invite.network().to_vec(),
                path_prefix: invite.path().to_string(),
                actions: [invite.action().to_string()].into_iter().collect(),
            },
            now_ms,
            now_ms,
            Some(expires_at_ms),
            0,
            nonce,
        ),
    )?)
}

/// The facts a WebRTC channel can honestly report, from the audited adapter.
///
/// Built through [`AcceptedSession::session_facts`] rather than by hand, which
/// is the whole point: this port has one place where carrier facts come from,
/// and a door that assembled its own [`SessionFacts`] would be a second.
/// `peer` is `None` because the carrier authenticated nobody, and the link is
/// the challenge's derived value.
pub fn webrtc_session_facts(shared_link: [u8; 16]) -> SessionFacts {
    AcceptedSession::new(
        (),
        Alpn::from_bytes(PROJECTION_PROTOCOL),
        None,
        IngressContext::webrtc(shared_link),
    )
    .session_facts()
}

/// Run Graphshell admission over one hello that arrived on this channel.
///
/// Sans-I/O, exactly as [`crate::browser_carrier`] is: the hello arrives as
/// bytes and the reply leaves as bytes, so the caller writes the refusal on a
/// refused session and closes, and no application byte can precede that. The
/// returned reply is well-formed in both arms.
pub fn admit_webrtc_session(
    policy: &LocalNetworkPolicy,
    ledger: &RevocationLedger,
    hello_bytes: &[u8],
    shared_link: [u8; 16],
    now_ms: u64,
    active_sessions: u32,
) -> (Vec<u8>, Result<AdmittedPrincipal, DenyReason>) {
    crate::admission::admit_session(
        policy,
        ledger,
        hello_bytes,
        &webrtc_session_facts(shared_link),
        now_ms,
        active_sessions,
    )
}

// ---------------------------------------------------------------------------
// Client side
// ---------------------------------------------------------------------------
//
// Pure functions with no runtime beneath them. A wasm browser build calls
// exactly these; the tests below call them too, which is what keeps the two
// from drifting.

/// Check the host's signature over the link-challenge transcript.
///
/// The browser runs this **before** trusting the channel. `expected_host_key`
/// is the key its invitation named, so the check answers "is the peer that
/// completed this DTLS handshake the host my invitation was minted by", which
/// is the question a relay-mediated signaling path cannot answer on its own.
pub fn verify_host_challenge(
    expected_host_key: &[u8; 32],
    challenge: &LinkChallenge,
    signature: &HostChallengeSignature,
) -> bool {
    verify_derived(
        &signature.signer,
        expected_host_key,
        &signature.signature,
        &challenge_signature_bytes(challenge),
    )
}

/// Prove possession of the redemption seed for this transcript and subject.
///
/// The seed is an ordinary Ed25519 seed rather than a Personae derived key,
/// because it is not an identity: it is a one-shot bearer secret whose only
/// job is to be unforgeable and then be spent. Binding the signature to both
/// the transcript and the ephemeral subject is what stops a captured proof
/// from redeeming a different subject onto a different connection.
pub fn build_redemption_proof(
    seed: &[u8; 32],
    challenge: &LinkChallenge,
    subject: &[u8; 32],
) -> [u8; 64] {
    Ed25519Keypair::from_seed(*seed)
        .sign(&redemption_signing_bytes(challenge, subject))
        .to_bytes()
}

/// Issue the browser's hello, bound to the link both ends derived.
///
/// [`crate::admission::open_session`] with the binding a carrier that
/// authenticates nobody is entitled to: no peer, one link.
/// [`transport::initiator_binding`] would be a lie here.
pub fn open_webrtc_session<P: IdentityProvider>(
    ephemeral: &P,
    network: NetworkId,
    profile: ProfileRef,
    nonce: [u8; 32],
    shared_link: [u8; 16],
    delegations: Vec<SignedDelegationCertificate>,
) -> Result<SessionHello, HandshakeError> {
    open_session(
        ephemeral,
        network,
        profile,
        TrafficClass::Interactive,
        nonce,
        &initiator_link_binding(&Alpn::from_bytes(PROJECTION_PROTOCOL), shared_link),
        delegations,
    )
}

/// The three-part derived-key check `personae` uses everywhere.
///
/// Attestation valid for this salt, attested master equal to the key the
/// caller expected, and the derived public key verifying the signature. All
/// three, or false — this is the shape [`SignedDelegationCertificate::verify`]
/// establishes and the door does not get to simplify it.
fn verify_derived(
    signer: &DerivedKeyAttestation,
    expected_master: &[u8; 32],
    signature: &[u8],
    message: &[u8],
) -> bool {
    if !signer.verify(WEBRTC_HOST_SIGNING_SALT) {
        return false;
    }
    let Ok(master) = signer.master_public_key() else {
        return false;
    };
    if &master.to_bytes() != expected_master {
        return false;
    }
    let Ok(derived) = signer.derived_public_key() else {
        return false;
    };
    let Ok(signature) = <[u8; 64]>::try_from(signature) else {
        return false;
    };
    derived.verify(message, &Ed25519Signature::from_bytes(&signature))
}

#[cfg(test)]
mod tests {
    //! The fail-closed matrix.
    //!
    //! One row per way a browser join can be wrong, each asserting the *exact*
    //! refusal rather than `is_err()`. A door whose refusals are all spelled
    //! the same way is a door nobody can debug and nobody can trust: which
    //! check fired is the observable behaviour, so it is what the tests pin.
    //!
    //! The positive control is first, on purpose. Every negative row below is
    //! that same join with exactly one thing changed, so a refusal only means
    //! something if the unchanged join is known to be admitted.

    use super::*;
    use crate::admission::{GRAPHSHELL_DOMAIN, PROJECTION_SERVICE};
    use crate::carrier::projection_policy;
    use notochord::{ChainFault, SessionReply, TrustedRoot};
    use personae::InMemoryProvider;
    use personae::delegation::{DelegationRevocation, SignedDelegationRevocation};
    use webrtc_carrier::{
        DTLS_FINGERPRINT_BYTES, DtlsFingerprint, FingerprintRole, INVITE_FRAGMENT_PREFIX,
        MAX_INVITE_BYTES,
    };

    const NETWORK: NetworkId = NetworkId([3; 32]);
    const ROOT_AUTHORITY: [u8; 32] = [7; 32];
    const NOW_MS: u64 = 50;
    const TTL_MS: u64 = 30;
    const INVITE_EXPIRY_MS: u64 = 10_000;
    const HELLO_NONCE: [u8; 32] = [5; 32];
    const DELEGATION_NONCE: [u8; 32] = [4; 32];

    fn profile() -> ProfileRef {
        ProfileRef {
            id: "mere.base".into(),
            revision: 1,
        }
    }

    fn host() -> InMemoryProvider {
        InMemoryProvider::from_seed([1; 32])
    }

    fn policy() -> LocalNetworkPolicy {
        projection_policy(
            NETWORK,
            vec![TrustedRoot {
                authority: ROOT_AUTHORITY,
                issuer: host().master_public_key().to_bytes(),
            }],
            vec![profile()],
            None,
        )
    }

    fn release() -> ReleaseRefV1 {
        ReleaseRefV1 {
            manifest_blake3: [0x5a; 32],
            publisher_key_id: [0x6b; 32],
        }
    }

    fn terms(expires_at_ms: u64, max_uses: u32) -> InviteTerms {
        InviteTerms::projection(NETWORK, profile(), expires_at_ms, max_uses, release())
    }

    /// A transcript with fabricated fingerprints.
    ///
    /// Fabricated is correct *here* and nowhere else: these are pure tests
    /// with no DTLS handshake behind them, so there is no certificate to read
    /// a real digest off. The native side takes them from
    /// `Carrier::fingerprints()` — the certificate actually presented — and a
    /// browser takes them from the SDP; the loopback integration test in
    /// `tests/webrtc_door_loopback.rs` is the one that uses real ones.
    fn challenge(client_digest: u8, server_nonce: u8) -> LinkChallenge {
        LinkChallenge::new(
            PROJECTION_PROTOCOL,
            "mere-graphshell",
            InviteId::from_bytes([9; 16]),
            [0x11; 32],
            [server_nonce; 32],
            DtlsFingerprint::new(
                FingerprintRole::Client,
                [client_digest; DTLS_FINGERPRINT_BYTES],
            ),
            DtlsFingerprint::new(FingerprintRole::Server, [0xbb; DTLS_FINGERPRINT_BYTES]),
        )
        .expect("a well-formed link challenge")
    }

    /// One complete browser join, up to but not including the hello.
    ///
    /// Every negative row below constructs this and changes exactly one
    /// thing, which is what makes the refusals comparable to each other and
    /// to the positive control.
    struct Join {
        host: InMemoryProvider,
        /// The browser's locally generated ephemeral subject.
        ///
        /// `InMemoryProvider::random()` in real use; a fixed seed here so a
        /// failure is reproducible. What matters is where it lives: this
        /// provider is only ever touched on the client side of the module
        /// boundary. Every host-side entry point — [`redeem`],
        /// [`mint_delegation`] — takes a subject as `[u8; 32]`, a public key,
        /// so there is no parameter through which its private half could
        /// reach the host even by mistake.
        browser: InMemoryProvider,
        subject: [u8; 32],
        descriptor: SignedInviteDescriptor,
        redemption: RedemptionState,
        challenge: LinkChallenge,
    }

    impl Join {
        fn new(max_uses: u32, invite_expiry_ms: u64) -> Self {
            let host = host();
            let issue =
                issue_invite(&host, &terms(invite_expiry_ms, max_uses)).expect("issue invite");
            let browser = InMemoryProvider::from_seed([2; 32]);
            let subject = browser.master_public_key().to_bytes();
            Self {
                host,
                browser,
                subject,
                descriptor: issue.descriptor,
                redemption: issue.redemption,
                challenge: challenge(0xaa, 0x22),
            }
        }

        fn host_key(&self) -> [u8; 32] {
            self.host.master_public_key().to_bytes()
        }

        /// What the browser computes from the seed it read out of the
        /// fragment.
        fn proof(&self) -> [u8; 64] {
            build_redemption_proof(
                self.descriptor.invite.redemption_seed(),
                &self.challenge,
                &self.subject,
            )
        }

        fn redeem_at(&mut self, now_ms: u64) -> Result<(), RedemptionRefusal> {
            let proof = self.proof();
            redeem(
                &mut self.redemption,
                &self.challenge,
                &self.subject,
                &proof,
                now_ms,
            )
        }

        fn mint(&self, now_ms: u64, ttl_ms: u64) -> Result<SignedDelegationCertificate, DoorError> {
            mint_delegation(
                &self.host,
                ROOT_AUTHORITY,
                self.subject,
                &self.descriptor.invite,
                now_ms,
                ttl_ms,
                DELEGATION_NONCE,
            )
        }

        /// A grant from this host to this subject over an arbitrary action.
        ///
        /// Not [`mint_delegation`]: the door will only ever mint the
        /// invitation's own action, so an altered-action row has to build the
        /// wrong certificate the long way, exactly as a compromised or
        /// mistaken host would.
        fn grant_for(&self, action: &str, expires_at_ms: u64) -> SignedDelegationCertificate {
            SignedDelegationCertificate::issue(
                &self.host,
                DelegationCertificate::new(
                    DelegationParent::Root(ROOT_AUTHORITY),
                    self.host_key(),
                    self.subject,
                    CapabilityScope {
                        domain: GRAPHSHELL_DOMAIN.into(),
                        resource: NETWORK.0.to_vec(),
                        path_prefix: PROJECTION_SERVICE.into(),
                        actions: [action.to_string()].into_iter().collect(),
                    },
                    NOW_MS,
                    NOW_MS,
                    Some(expires_at_ms),
                    0,
                    DELEGATION_NONCE,
                ),
            )
            .expect("issue the grant")
        }

        /// The hello the browser signs, bound to `link`.
        fn hello(&self, delegations: Vec<SignedDelegationCertificate>, link: [u8; 16]) -> Vec<u8> {
            open_webrtc_session(
                &self.browser,
                NETWORK,
                profile(),
                HELLO_NONCE,
                link,
                delegations,
            )
            .expect("issue hello")
            .encode(&policy().limits.clamped())
            .expect("encode hello")
        }
    }

    /// The whole happy path, in the order the two ends actually run it.
    fn admitted_join() -> (Join, SignedDelegationCertificate) {
        let mut join = Join::new(1, INVITE_EXPIRY_MS);

        // 1. The host signs the transcript; the browser checks it before it
        //    trusts the channel with a redemption proof.
        let signed = sign_challenge(&join.host, &join.challenge).expect("host signs");
        assert!(verify_host_challenge(
            &join.host_key(),
            &join.challenge,
            &signed
        ));
        // 2. The browser proves the seed; the host spends a use.
        join.redeem_at(NOW_MS).expect("the first redemption");
        // 3. The host mints one narrow, short-lived leaf.
        let minted = join.mint(NOW_MS, TTL_MS).expect("mint");
        (join, minted)
    }

    // -- 1. positive control ------------------------------------------------

    #[test]
    fn a_valid_invite_admits_the_browser_subject_for_exactly_connect() {
        let (join, minted) = admitted_join();
        assert!(
            join.descriptor.verify(&join.host_key()),
            "the descriptor the browser read must verify against the key it named"
        );

        let hello = join.hello(vec![minted], join.challenge.shared_link());
        let (reply, outcome) = admit_webrtc_session(
            &policy(),
            &RevocationLedger::new(),
            &hello,
            join.challenge.shared_link(),
            NOW_MS,
            0,
        );

        let principal = outcome.expect("a valid invite opens a projection session");
        assert_eq!(
            principal.subject, join.subject,
            "the admitted principal is the subject the browser generated locally"
        );
        assert_eq!(principal.action, connect_action());
        assert!(
            SessionReply::decode(&reply, &policy().limits.clamped())
                .expect("a well-formed reply")
                .is_accept()
        );
    }

    #[test]
    fn the_host_retains_a_verifier_rather_than_the_redemption_seed() {
        // Structural, not behavioural: what the host retains is a public
        // verifier, a count, and a deadline. The seed left in the descriptor.
        let join = Join::new(1, INVITE_EXPIRY_MS);
        assert_eq!(
            join.redemption.verifier(),
            Ed25519Keypair::from_seed(*join.descriptor.invite.redemption_seed()).public_key(),
            "the retained verifier is the seed's public half and nothing more"
        );
        assert_eq!(join.redemption.remaining_uses(), 1);
        assert_eq!(join.redemption.expires_at_ms(), INVITE_EXPIRY_MS);
    }

    #[test]
    fn the_three_signing_domains_do_not_overlap() {
        let join = Join::new(1, INVITE_EXPIRY_MS);
        let challenge_bytes = challenge_signature_bytes(&join.challenge);
        let proof_bytes = redemption_signing_bytes(&join.challenge, &join.subject);
        assert_ne!(challenge_bytes, proof_bytes);
        assert_ne!(challenge_bytes, join.descriptor.invite.signing_bytes());
        assert_ne!(
            challenge_bytes,
            join.challenge.encode(),
            "a host signature must not double as a signature over the bare transcript"
        );
    }

    // -- 2. a one-use invite cannot mint a second delegation ----------------

    #[test]
    fn a_one_use_invite_refuses_its_second_redemption() {
        let mut join = Join::new(1, INVITE_EXPIRY_MS);
        join.redeem_at(NOW_MS).expect("the first redemption");
        assert_eq!(join.redemption.remaining_uses(), 0);

        assert_eq!(
            join.redeem_at(NOW_MS),
            Err(RedemptionRefusal::Exhausted),
            "the ceiling is a ceiling, not a rate"
        );

        // And there is no second delegation to present, so the only hello a
        // second connection could build is an empty-chained one, which a
        // MemberOnly service refuses outright.
        let second = challenge(0xaa, 0x33);
        let hello = join.hello(Vec::new(), second.shared_link());
        let (_, outcome) = admit_webrtc_session(
            &policy(),
            &RevocationLedger::new(),
            &hello,
            second.shared_link(),
            NOW_MS,
            0,
        );
        assert_eq!(
            outcome.unwrap_err(),
            DenyReason::Delegation(ChainFault::Empty)
        );
    }

    #[test]
    fn a_refused_redemption_costs_the_invitation_nothing() {
        let mut join = Join::new(1, INVITE_EXPIRY_MS);
        let wrong = [0u8; 64];
        assert_eq!(
            redeem(
                &mut join.redemption,
                &join.challenge,
                &join.subject,
                &wrong,
                NOW_MS
            ),
            Err(RedemptionRefusal::BadProof)
        );
        assert_eq!(
            join.redemption.remaining_uses(),
            1,
            "a wrong proof must not spend someone else's invitation"
        );
        join.redeem_at(NOW_MS)
            .expect("the real holder still redeems");
    }

    #[test]
    fn a_proof_bound_to_another_subject_does_not_redeem() {
        let mut join = Join::new(1, INVITE_EXPIRY_MS);
        let other = InMemoryProvider::from_seed([12; 32])
            .master_public_key()
            .to_bytes();
        let proof = build_redemption_proof(
            join.descriptor.invite.redemption_seed(),
            &join.challenge,
            &other,
        );
        assert_eq!(
            redeem(
                &mut join.redemption,
                &join.challenge,
                &join.subject,
                &proof,
                NOW_MS
            ),
            Err(RedemptionRefusal::BadProof)
        );
    }

    #[test]
    fn a_proof_from_another_connection_does_not_redeem() {
        let mut join = Join::new(1, INVITE_EXPIRY_MS);
        let elsewhere = challenge(0xaa, 0x55);
        let proof = build_redemption_proof(
            join.descriptor.invite.redemption_seed(),
            &elsewhere,
            &join.subject,
        );
        assert_eq!(
            redeem(
                &mut join.redemption,
                &join.challenge,
                &join.subject,
                &proof,
                NOW_MS
            ),
            Err(RedemptionRefusal::BadProof)
        );
    }

    // -- 3. an altered action -----------------------------------------------

    #[test]
    fn a_scope_that_does_not_cover_the_requested_action_is_not_covered() {
        // The delegation is genuinely the host's, genuinely for this subject,
        // and genuinely rooted — it simply does not name `connect`. The chain
        // validates; the coverage check is what refuses.
        let (join, _) = admitted_join();
        let hello = join.hello(
            vec![join.grant_for("administer", NOW_MS + TTL_MS)],
            join.challenge.shared_link(),
        );
        let (_, outcome) = admit_webrtc_session(
            &policy(),
            &RevocationLedger::new(),
            &hello,
            join.challenge.shared_link(),
            NOW_MS,
            0,
        );
        assert_eq!(outcome.unwrap_err(), DenyReason::ActionNotCovered);
    }

    #[test]
    fn an_action_this_door_does_not_serve_is_not_offered() {
        // The other half of the altered-action row, and a different refusal:
        // here the owner's policy *does* allow the action and the chain *does*
        // cover it, so Notochord admits. Graphshell's own service vocabulary
        // is what refuses, which is why the reason is `ActionNotOffered` and
        // not `ActionNotCovered`.
        //
        // The hello is hand-built rather than going through `open_session`,
        // for `carrier.rs`'s reason: that helper fixes the action to
        // `connect`, and editing a signed hello afterwards would invalidate
        // it.
        let (join, _) = admitted_join();
        let mut permissive = policy();
        permissive
            .services
            .get_mut(PROJECTION_SERVICE)
            .expect("the projection rule")
            .actions
            .insert("administer".to_string());

        let link = join.challenge.shared_link();
        let hello = SessionHello::issue(
            &join.browser,
            NETWORK,
            profile(),
            RequestedAction {
                domain: GRAPHSHELL_DOMAIN.into(),
                path: PROJECTION_SERVICE.into(),
                action: "administer".into(),
            },
            TrafficClass::Interactive,
            HELLO_NONCE,
            &initiator_link_binding(&Alpn::from_bytes(PROJECTION_PROTOCOL), link),
            vec![join.grant_for("administer", NOW_MS + TTL_MS)],
        )
        .expect("issue an administer hello")
        .encode(&permissive.limits.clamped())
        .expect("encode hello");

        let (_, outcome) = admit_webrtc_session(
            &permissive,
            &RevocationLedger::new(),
            &hello,
            link,
            NOW_MS,
            0,
        );
        assert_eq!(outcome.unwrap_err(), DenyReason::ActionNotOffered);
    }

    #[test]
    fn the_minted_scope_is_the_invitation_and_nothing_wider() {
        let (join, minted) = admitted_join();
        let scope = &minted.certificate.scope;
        assert_eq!(scope.domain, join.descriptor.invite.domain());
        assert_eq!(scope.path_prefix, join.descriptor.invite.path());
        assert_eq!(
            scope.actions,
            [join.descriptor.invite.action().to_string()]
                .into_iter()
                .collect()
        );
        assert_eq!(scope.resource, join.descriptor.invite.network().to_vec());
        assert_eq!(
            minted.certificate.remaining_delegation_depth, 0,
            "a redeemed browser cannot pass its grant on"
        );
        assert_eq!(minted.certificate.subject, join.subject);
        assert_eq!(minted.certificate.issuer, join.host_key());
    }

    // -- 4. expiry ----------------------------------------------------------

    #[test]
    fn a_delegation_whose_ttl_has_passed_is_refused_as_expired() {
        let (join, minted) = admitted_join();
        let hello = join.hello(vec![minted], join.challenge.shared_link());
        let (_, outcome) = admit_webrtc_session(
            &policy(),
            &RevocationLedger::new(),
            &hello,
            join.challenge.shared_link(),
            NOW_MS + TTL_MS + 1,
            0,
        );
        assert_eq!(
            outcome.unwrap_err(),
            DenyReason::Delegation(ChainFault::Expired)
        );
    }

    #[test]
    fn an_expired_invitation_refuses_at_redemption() {
        let mut join = Join::new(1, INVITE_EXPIRY_MS);
        assert_eq!(
            join.redeem_at(INVITE_EXPIRY_MS + 1),
            Err(RedemptionRefusal::Expired)
        );
        assert_eq!(
            join.redemption.remaining_uses(),
            1,
            "an expired invitation is refused, not consumed"
        );
        // And nothing downstream will mint one either.
        assert!(matches!(
            join.mint(INVITE_EXPIRY_MS + 1, TTL_MS),
            Err(DoorError::InviteExpired)
        ));
    }

    #[test]
    fn a_session_grant_never_outlives_the_invitation_that_bought_it() {
        let (join, _) = admitted_join();
        let generous = join
            .mint(NOW_MS, INVITE_EXPIRY_MS * 4)
            .expect("mint with a ttl past the invitation");
        assert_eq!(
            generous.certificate.expires_at_ms,
            Some(INVITE_EXPIRY_MS),
            "the tighter of the host's ttl and the owner's invite expiry wins"
        );
    }

    // -- 5. revocation ------------------------------------------------------

    #[test]
    fn a_revoked_delegation_is_refused() {
        let (join, minted) = admitted_join();
        let revocation = SignedDelegationRevocation::issue(
            &join.host,
            DelegationRevocation::new(
                minted.certificate.id(),
                join.host_key(),
                minted.certificate.scope.clone(),
                NOW_MS,
                [0x1d; 32],
            ),
        )
        .expect("issue the revocation");

        let mut ledger = RevocationLedger::new();
        assert!(ledger.fold(&revocation), "the statement must verify");

        let hello = join.hello(vec![minted], join.challenge.shared_link());
        let (_, outcome) = admit_webrtc_session(
            &policy(),
            &ledger,
            &hello,
            join.challenge.shared_link(),
            NOW_MS,
            0,
        );
        assert_eq!(
            outcome.unwrap_err(),
            DenyReason::Delegation(ChainFault::Revoked)
        );
    }

    // -- 6. the wrong host --------------------------------------------------

    #[test]
    fn a_challenge_signature_does_not_verify_under_another_host_key() {
        let join = Join::new(1, INVITE_EXPIRY_MS);
        let signed = sign_challenge(&join.host, &join.challenge).expect("host signs");
        assert!(verify_host_challenge(
            &join.host_key(),
            &join.challenge,
            &signed
        ));

        let impostor = InMemoryProvider::from_seed([99; 32]);
        assert!(
            !verify_host_challenge(
                &impostor.master_public_key().to_bytes(),
                &join.challenge,
                &signed
            ),
            "a browser that expected another host must not accept this channel"
        );

        // A relay that terminated the channel itself signs its own transcript
        // honestly — and still fails, because the key is not the one the
        // invitation named.
        let relay_signed = sign_challenge(&impostor, &join.challenge).expect("impostor signs");
        assert!(!verify_host_challenge(
            &join.host_key(),
            &join.challenge,
            &relay_signed
        ));
    }

    #[test]
    fn a_tampered_challenge_signature_does_not_verify() {
        let join = Join::new(1, INVITE_EXPIRY_MS);
        let mut signed = sign_challenge(&join.host, &join.challenge).expect("host signs");
        signed.signature[0] ^= 1;
        assert!(!verify_host_challenge(
            &join.host_key(),
            &join.challenge,
            &signed
        ));

        // A signature over another transcript is equally useless here: the
        // signed bytes are the transcript, not a bare nonce.
        let elsewhere = challenge(0xaa, 0x44);
        let for_elsewhere = sign_challenge(&join.host, &elsewhere).expect("host signs");
        assert!(!verify_host_challenge(
            &join.host_key(),
            &join.challenge,
            &for_elsewhere
        ));
    }

    #[test]
    fn a_tampered_derived_key_attestation_does_not_verify() {
        // The attestation is not decoration. Swapping in another identity's
        // attestation, so the signature's own derived key looks vouched for,
        // fails on the master binding.
        let join = Join::new(1, INVITE_EXPIRY_MS);
        let impostor = InMemoryProvider::from_seed([99; 32]);
        let mut signed = sign_challenge(&join.host, &join.challenge).expect("host signs");
        signed.signer = impostor
            .attest_derived_key(WEBRTC_HOST_SIGNING_SALT)
            .expect("attest");
        assert!(!verify_host_challenge(
            &join.host_key(),
            &join.challenge,
            &signed
        ));
        assert!(!verify_host_challenge(
            &impostor.master_public_key().to_bytes(),
            &join.challenge,
            &signed
        ));
    }

    #[test]
    fn an_invite_descriptor_does_not_verify_under_another_host_key() {
        let join = Join::new(1, INVITE_EXPIRY_MS);
        assert!(join.descriptor.verify(&join.host_key()));
        assert!(
            !join.descriptor.verify(
                &InMemoryProvider::from_seed([99; 32])
                    .master_public_key()
                    .to_bytes()
            )
        );
    }

    #[test]
    fn a_minted_delegation_must_come_from_the_host_the_invite_names() {
        let join = Join::new(1, INVITE_EXPIRY_MS);
        let impostor = InMemoryProvider::from_seed([99; 32]);
        assert!(matches!(
            mint_delegation(
                &impostor,
                ROOT_AUTHORITY,
                join.subject,
                &join.descriptor.invite,
                NOW_MS,
                TTL_MS,
                DELEGATION_NONCE,
            ),
            Err(DoorError::WrongHost)
        ));
    }

    // -- 7. a substituted SDP fingerprint -----------------------------------

    #[test]
    fn a_substituted_fingerprint_invalidates_the_session_proof() {
        // Two transcripts differing in one fingerprint and nothing else. This
        // is the man-in-the-middle case stated concretely: a relay that
        // terminated DTLS itself presents a different certificate, so the link
        // the two ends derive no longer agrees.
        let first = challenge(0xaa, 0x22);
        let second = challenge(0xac, 0x22);
        assert_ne!(
            first.shared_link(),
            second.shared_link(),
            "one substituted fingerprint must move the link, or nothing below tests anything"
        );

        let (join, minted) = admitted_join();
        let hello = join.hello(vec![minted], first.shared_link());
        let (_, outcome) = admit_webrtc_session(
            &policy(),
            &RevocationLedger::new(),
            &hello,
            second.shared_link(),
            NOW_MS,
            0,
        );
        assert_eq!(outcome.unwrap_err(), DenyReason::SessionProofInvalid);
    }

    // -- 8. an oversized invite ---------------------------------------------

    #[test]
    fn an_oversized_invite_is_refused_before_it_is_parsed() {
        let too_long = vec![0u8; MAX_INVITE_BYTES + 1];
        // `matches!` rather than `assert_eq!`: `InviteV1` deliberately does
        // not implement `PartialEq`, so a `Result` carrying one cannot be
        // compared. The refusal is still pinned exactly, fields included.
        assert!(matches!(
            InviteV1::decode(&too_long),
            Err(InviteError::Oversize {
                got,
                max,
            }) if got == MAX_INVITE_BYTES + 1 && max == MAX_INVITE_BYTES
        ));

        // And through the fragment door, on the text length, before base64
        // decoding allocates anything.
        let body = "A".repeat(MAX_INVITE_BYTES * 4);
        let fragment = format!("{INVITE_FRAGMENT_PREFIX}{body}");
        assert!(matches!(
            InviteV1::parse_fragment(&fragment),
            Err(InviteError::Oversize { .. })
        ));
    }

    // -- 9. a captured hello on a second connection -------------------------

    #[test]
    fn a_captured_hello_cannot_cross_webrtc_connections() {
        // Mirrors `browser_carrier`'s
        // `a_captured_brokered_hello_cannot_cross_browser_challenges`. The
        // second connection is a genuinely fresh one — new nonces, so a new
        // transcript — and the captured hello was signed against the first.
        let (join, minted) = admitted_join();
        let reconnect = challenge(0xaa, 0x77);
        assert_ne!(join.challenge.shared_link(), reconnect.shared_link());

        let hello = join.hello(vec![minted], join.challenge.shared_link());
        let (_, first) = admit_webrtc_session(
            &policy(),
            &RevocationLedger::new(),
            &hello,
            join.challenge.shared_link(),
            NOW_MS,
            0,
        );
        assert!(first.is_ok(), "the hello is valid on its own connection");

        let (_, replayed) = admit_webrtc_session(
            &policy(),
            &RevocationLedger::new(),
            &hello,
            reconnect.shared_link(),
            NOW_MS,
            0,
        );
        assert_eq!(replayed.unwrap_err(), DenyReason::SessionProofInvalid);
    }

    // -- 10. a refused session cannot proceed -------------------------------

    #[test]
    fn a_refused_session_yields_a_refusal_reply_and_no_principal() {
        // A refusal is a finished reply, not a dropped connection: the browser
        // learns it was refused rather than timing out. What it must not be is
        // a principal, and this door's signature makes "refused but usable"
        // unspellable — the caller receives a `Result`, and there is no
        // admitted principal on the error arm to hand an application.
        let stranger = InMemoryProvider::from_seed([13; 32]);
        let link = challenge(0xaa, 0x22).shared_link();
        let hello =
            open_webrtc_session(&stranger, NETWORK, profile(), HELLO_NONCE, link, Vec::new())
                .expect("issue hello")
                .encode(&policy().limits.clamped())
                .expect("encode hello");

        let (reply, outcome) =
            admit_webrtc_session(&policy(), &RevocationLedger::new(), &hello, link, NOW_MS, 0);
        assert_eq!(
            outcome.unwrap_err(),
            DenyReason::Delegation(ChainFault::Empty)
        );

        assert!(!reply.is_empty(), "a refusal is still written");
        let decoded = SessionReply::decode(&reply, &policy().limits.clamped())
            .expect("a well-formed refusal");
        assert!(!decoded.is_accept());
        assert!(matches!(
            decoded,
            SessionReply::Reject {
                reason: DenyReason::Delegation(ChainFault::Empty)
            }
        ));
    }

    // -- 11. an altered release reference -----------------------------------

    #[test]
    fn an_altered_release_reference_breaks_the_invite_signature() {
        // The descriptor binds the release claim, so signaling cannot swap in
        // a different one. Rebuilt rather than mutated, because `InviteV1`
        // keeps its fields private: the same invitation in every respect but
        // the release, carrying the signature the host made over the original.
        let join = Join::new(1, INVITE_EXPIRY_MS);
        let original = &join.descriptor.invite;
        let altered = InviteV1::new(
            original.rendezvous(),
            *original.redemption_seed(),
            *original.expected_host_key(),
            *original.network(),
            original.profile_id(),
            original.profile_revision(),
            original.domain(),
            original.path(),
            original.action(),
            original.expires_at_ms(),
            original.max_uses(),
            ReleaseRefV1 {
                manifest_blake3: [0xff; 32],
                publisher_key_id: release().publisher_key_id,
            },
        )
        .expect("the altered invite is still well-formed");
        assert_ne!(altered.signing_bytes(), original.signing_bytes());

        let forged = SignedInviteDescriptor::from_parts(
            altered,
            join.descriptor.signer.clone(),
            join.descriptor.signature().to_vec(),
        );
        assert!(
            !forged.verify(&join.host_key()),
            "a rewritten release reference must not verify"
        );
    }

    #[test]
    fn a_rewritten_expected_host_key_does_not_verify() {
        // The other half of the same property. An attacker who rewrites the
        // key the invitation names, hoping the descriptor will then verify
        // against itself, is refused because `verify` takes the key the reader
        // expected as an argument rather than reading it out of the payload.
        let join = Join::new(1, INVITE_EXPIRY_MS);
        let impostor = InMemoryProvider::from_seed([99; 32]);
        let original = &join.descriptor.invite;
        let altered = InviteV1::new(
            original.rendezvous(),
            *original.redemption_seed(),
            impostor.master_public_key().to_bytes(),
            *original.network(),
            original.profile_id(),
            original.profile_revision(),
            original.domain(),
            original.path(),
            original.action(),
            original.expires_at_ms(),
            original.max_uses(),
            original.release(),
        )
        .expect("well-formed");

        let forged = SignedInviteDescriptor::from_parts(
            altered,
            join.descriptor.signer.clone(),
            join.descriptor.signature().to_vec(),
        );
        assert!(!forged.verify(&impostor.master_public_key().to_bytes()));
        assert!(!forged.verify(&join.host_key()));
    }

    // -- the facts the door reports -----------------------------------------

    #[test]
    fn webrtc_facts_authenticate_nobody_and_carry_the_link() {
        let link = challenge(0xaa, 0x22).shared_link();
        let facts = webrtc_session_facts(link);
        assert_eq!(facts.authenticated_initiator, None);
        assert_eq!(facts.ingress.shared_link, Some(link));
        assert_eq!(facts.ingress.local_interface, None);
        assert_eq!(facts.transport, notochord::CarrierKind::Other);
        assert_eq!(
            facts.proof_binding(),
            initiator_link_binding(&Alpn::from_bytes(PROJECTION_PROTOCOL), link),
            "the responder's binding must be the one the browser signed against"
        );
    }
}
