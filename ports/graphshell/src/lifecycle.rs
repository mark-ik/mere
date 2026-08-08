//! G5e: what happens to a projection session after it is admitted.
//!
//! Admission is a conclusion drawn once, at connect. Authority is not: a
//! certificate expires, an owner revokes a grant, a viewer reconnects. This
//! module is the join between the two, and it introduces no new vocabulary to
//! do it. `graphshell-protocol` already models every state involved
//! ([`SessionStatus`], [`CachePolicy`], [`IntentResult::Stale`]); what was
//! missing is anything that *drives* them from the authority the carrier
//! proved.
//!
//! The dependency wall holds: `graphshell-client` and `graphshell-protocol`
//! still declare no Personae and no `notochord`. The authority lives
//! here, in the application, and reaches the client only through the mutators
//! the client already exposed.
//!
//! ## A reconnect is a new admission
//!
//! [`projection_session`] derives the session id from
//! `AdmittedPrincipal::session_id`, which is transcript-bound. A client
//! therefore cannot name the session it mounts under: reconnecting produces a
//! different id, so cached scenes from a previous admission are unreachable by
//! construction rather than by a rule someone has to enforce. Resume within
//! one admission stays exactly as G2 built it.
//!
//! ## Why the chain is retained
//!
//! [`AdmittedPrincipal`] is deliberately a conclusion and carries no claims,
//! so it cannot answer "is this still true?" later. [`AdmittedSession`]
//! retains the verified claims beside that conclusion, letting
//! [`SessionAuthority::retain_admitted`] preserve the chain for expiry and
//! revocation checks without decoding application bytes a second time.

use graphshell_client::ClientState;
use graphshell_protocol::{
    CachePolicy, IntentInvocation, IntentResult, ProjectionSession, Revision, SceneEpoch,
    SessionStatus,
};
use notochord::{
    AdmittedPrincipal, AdmittedSession, AuthorityLapse, RetainedAuthority, RevocationLedger,
};
use personae::delegation::SignedDelegationCertificate;

use crate::admission::{CONNECT_ACTION, PROJECTION_SERVICE};

/// The projection session id for an admitted principal.
///
/// Derived from the transcript-bound `session_id` rather than chosen by the
/// client, so a cached scene cannot be re-mounted under an authority that did
/// not earn it.
///
/// Distinctness is the *client's* to maintain, and the responder does not
/// enforce it. `session_id` is a digest of the transcript, the transcript
/// carries the client's nonce, and nothing checks that a nonce is fresh. Two
/// admissions of the same subject reusing one nonce therefore land on the same
/// projection session. That is not an authority hole, because the transcript
/// binds the subject and a peer can only collide with itself, but a client
/// mounting two projections in one `ClientState` must mint per-session
/// randomness or the second silently replaces the first.
pub fn projection_session(principal: &AdmittedPrincipal) -> ProjectionSession {
    use base64::Engine;
    ProjectionSession(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(principal.session_id))
}

/// The service path a named score is disclosed under.
///
/// Scores hang below the service path so that Personae's scope grammar can
/// express "this viewer may see one score". The `/` boundary in
/// `path_covers` is what makes that safe: a grant for `spiral` does not reach
/// `spiral-admin`.
pub fn score_path(score: &str) -> String {
    format!("{PROJECTION_SERVICE}/{score}")
}

/// Graphshell's name for Notochord's retained-authority lapse.
pub type Lapse = AuthorityLapse;

/// The status a client may render for a retained-authority lapse.
fn lapse_status(lapse: Lapse) -> SessionStatus {
    match lapse {
        AuthorityLapse::Expired { .. } => SessionStatus::Expired,
        AuthorityLapse::Revoked => SessionStatus::Revoked,
    }
}

/// Why a score was not disclosed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScoreDenial {
    /// The session's authority has lapsed; nothing is disclosed under it.
    Lapsed(Lapse),
    /// The authority is live but does not reach this score.
    NotCovered { score: String },
}

/// A live session's retained authority.
///
/// Holds the chain the session was admitted on so the session can re-ask the
/// question later. Nothing here is serializable, for the same reason
/// `AdmittedPrincipal` is not: it is a local conclusion about a connection,
/// and a conclusion that travels is a claim.
#[derive(Clone, Debug)]
pub struct SessionAuthority {
    session: ProjectionSession,
    retained: RetainedAuthority,
}

/// The narrow identity and projection-session handoff an admitted endpoint
/// receives from its composing host.
///
/// This is deliberately not an admission proof or a bearer credential. It
/// carries neither delegation certificates nor a transport. The enclosing
/// [`SessionAuthority`] and session loop remain responsible for admission,
/// expiry, and revocation; a product endpoint uses this only to name the
/// already-admitted projection and attribute its own scoped work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdmittedEndpointContext {
    session: ProjectionSession,
    subject: [u8; 32],
}

impl AdmittedEndpointContext {
    /// Construct one in-process composition handoff from already-verified
    /// facts. This constructor performs no authentication.
    pub fn new(session: ProjectionSession, subject: [u8; 32]) -> Self {
        Self { session, subject }
    }

    /// The transcript-derived projection session the endpoint must serve.
    pub fn session(&self) -> &ProjectionSession {
        &self.session
    }

    /// The already-admitted public-key subject for product-scoped work.
    pub fn subject(&self) -> [u8; 32] {
        self.subject
    }
}

/// Let a product endpoint bind itself to a session that Graphshell already
/// admitted.
///
/// Implementors must not treat [`AdmittedEndpointContext`] as a replacement
/// for carrier admission. It is a typed handoff inside a host that continues
/// to serve through [`SessionAuthority`].
pub trait BindAdmittedSession: Sized {
    fn bind_admitted_session(self, context: &AdmittedEndpointContext) -> Self;
}

impl SessionAuthority {
    /// Retain the authority carried by an admitted service session.
    ///
    /// This is the carrier path. The delegation chain comes from the verified
    /// claims retained by `notochord::admit_session`, so a long-lived
    /// session can notice a revocation after admission.
    pub fn retain_admitted<S>(session: &AdmittedSession<S>) -> Self {
        Self {
            session: projection_session(&session.principal),
            retained: RetainedAuthority::from_admitted(session),
        }
    }

    /// Retain the authority a session was admitted on.
    ///
    /// This remains useful to the sans-I/O path, where the caller already
    /// holds the verified hello. Carrier code should use
    /// [`Self::retain_admitted`] so it cannot accidentally discard the chain.
    pub fn retain(principal: AdmittedPrincipal, chain: Vec<SignedDelegationCertificate>) -> Self {
        Self {
            session: projection_session(&principal),
            retained: RetainedAuthority::new(principal, chain),
        }
    }

    /// The session id this authority admits, derived from the transcript.
    pub fn session(&self) -> &ProjectionSession {
        &self.session
    }

    /// Who was admitted, and for what.
    pub fn principal(&self) -> &AdmittedPrincipal {
        self.retained.principal()
    }

    /// Create the bounded product handoff for this already-admitted session.
    pub fn endpoint_context(&self) -> AdmittedEndpointContext {
        AdmittedEndpointContext::new(self.session.clone(), self.principal().subject)
    }

    /// When this session's authority runs out, if it does.
    ///
    /// The earliest expiry in the chain: a session lives no longer than the
    /// shortest-lived link in the authority that opened it.
    pub fn deadline_ms(&self) -> Option<u64> {
        self.retained.deadline_ms()
    }

    /// Whether the authority still holds, and if not, why.
    ///
    /// Revocation is checked before expiry: both can be true at once, and
    /// "the owner withdrew this" is the more useful thing to report.
    pub fn lapse(&self, ledger: &RevocationLedger, now_ms: u64) -> Option<Lapse> {
        self.retained.lapse(ledger, now_ms)
    }

    /// The status a client may render without inferring authority itself.
    pub fn status(&self, ledger: &RevocationLedger, now_ms: u64) -> SessionStatus {
        self.lapse(ledger, now_ms)
            .map_or(SessionStatus::Live, lapse_status)
    }

    /// Whether this authority reaches one named score.
    ///
    /// A grant scoped to the whole service discloses every score; a grant
    /// scoped to one score discloses only that one. Both are the same check,
    /// because the scope grammar already draws the line.
    pub fn authorize_score(
        &self,
        ledger: &RevocationLedger,
        now_ms: u64,
        score: &str,
    ) -> Result<(), ScoreDenial> {
        if let Some(lapse) = self.lapse(ledger, now_ms) {
            return Err(ScoreDenial::Lapsed(lapse));
        }
        let path = score_path(score);
        let covered = self.retained.covers(&path, CONNECT_ACTION, now_ms);
        if covered {
            Ok(())
        } else {
            Err(ScoreDenial::NotCovered {
                score: score.to_string(),
            })
        }
    }
}

/// Adjudicate one intent against both the authority and the scene.
///
/// Two ways to be stale, and they are answered in this order on purpose. An
/// intent from a lapsed session is refused outright: replying `Stale` with the
/// current revision would tell a peer whose authority has ended what the
/// current revision *is*, which is a disclosure the session no longer earns.
pub fn adjudicate_intent(
    authority: &SessionAuthority,
    ledger: &RevocationLedger,
    now_ms: u64,
    invocation: &IntentInvocation,
    current_epoch: SceneEpoch,
    current_revision: Revision,
) -> IntentResult {
    if invocation.session != *authority.session() {
        return IntentResult::Rejected {
            reason: "intent names a session this authority does not admit".to_string(),
        };
    }
    if let Some(lapse) = authority.lapse(ledger, now_ms) {
        return IntentResult::Rejected {
            reason: match lapse {
                Lapse::Expired { at_ms } => format!("session authority expired at {at_ms}"),
                Lapse::Revoked => "session authority was revoked".to_string(),
            },
        };
    }
    if invocation.observed_epoch != current_epoch
        || invocation.observed_revision != current_revision
    {
        return IntentResult::Stale {
            current_epoch,
            current_revision,
        };
    }
    IntentResult::Accepted
}

/// Apply a lapse to the client's mounted state.
///
/// Honours the mounted scene's own [`CachePolicy`]: `purge_on_revocation`
/// forgets the session outright, which drops both the scene and every resource
/// cached under it. Otherwise the scene stays mounted under a status that says
/// plainly it is no longer live, which is what an `EncryptedPersistent`
/// retention is for.
///
/// Returns whether the cache was purged.
pub fn apply_lapse(client: &mut ClientState, session: &ProjectionSession, lapse: Lapse) -> bool {
    let purge = client
        .mounted(session)
        .is_some_and(|mounted| purges_on(&mounted.cache_policy));
    if purge {
        client.forget_session(session);
    } else {
        match lapse {
            Lapse::Expired { .. } => client.mark_expired(session),
            Lapse::Revoked => client.mark_revoked(session),
        }
    }
    purge
}

/// Whether a cache policy purges when authority lapses.
///
/// `purge_on_revocation` is the owner's instruction about withdrawn authority.
/// An expiry is authority withdrawing on schedule rather than by hand, so it
/// purges under the same flag rather than a second one nobody would remember
/// to set.
fn purges_on(policy: &CachePolicy) -> bool {
    policy.purge_on_revocation
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admission::GRAPHSHELL_DOMAIN;
    use graphshell_protocol::{
        PresentationManifest, ProjectionSnapshot, ProtocolVersion, SceneSnapshot,
    };
    use notochord::{
        CarrierKind, HandshakeLimits, NetworkId, ProfileRef, RequestedAction, SessionClaims,
        SessionFacts, TrafficClass,
    };
    use personae::IdentityProvider;
    use personae::InMemoryProvider;
    use personae::delegation::{
        CapabilityScope, DelegationCertificate, DelegationParent, SignedDelegationRevocation,
    };
    use sceno::{InstanceId, Scene};

    const ROOT_AUTHORITY: [u8; 32] = [7; 32];
    const NETWORK: [u8; 32] = [3; 32];
    const NOW_MS: u64 = 50;
    const EXPIRY_MS: u64 = 100;

    fn owner() -> InMemoryProvider {
        InMemoryProvider::from_seed([1; 32])
    }

    fn viewer() -> InMemoryProvider {
        InMemoryProvider::from_seed([4; 32])
    }

    /// A grant to `subject` at `path_prefix`, expiring at [`EXPIRY_MS`].
    fn grant(path_prefix: &str) -> SignedDelegationCertificate {
        SignedDelegationCertificate::issue(
            &owner(),
            DelegationCertificate::new(
                DelegationParent::Root(ROOT_AUTHORITY),
                owner().master_public_key().to_bytes(),
                viewer().master_public_key().to_bytes(),
                CapabilityScope {
                    domain: GRAPHSHELL_DOMAIN.into(),
                    resource: NETWORK.to_vec(),
                    path_prefix: path_prefix.into(),
                    actions: [CONNECT_ACTION.to_string()].into_iter().collect(),
                },
                5,
                10,
                Some(EXPIRY_MS),
                1,
                [1; 32],
            ),
        )
        .expect("issue certificate")
    }

    fn principal(session_id: [u8; 32]) -> AdmittedPrincipal {
        AdmittedPrincipal {
            subject: viewer().master_public_key().to_bytes(),
            class: TrafficClass::Interactive,
            session_id,
            action: RequestedAction {
                domain: GRAPHSHELL_DOMAIN.to_string(),
                path: PROJECTION_SERVICE.to_string(),
                action: CONNECT_ACTION.to_string(),
            },
        }
    }

    fn authority_at(path_prefix: &str, session_id: [u8; 32]) -> SessionAuthority {
        SessionAuthority::retain(principal(session_id), vec![grant(path_prefix)])
    }

    fn authority() -> SessionAuthority {
        authority_at(PROJECTION_SERVICE, [21; 32])
    }

    fn admitted_authority() -> AdmittedSession<()> {
        let delegation = grant(PROJECTION_SERVICE);
        AdmittedSession {
            stream: (),
            principal: principal([21; 32]),
            claims: SessionClaims {
                wire_version: 1,
                network: NetworkId(NETWORK),
                profile: ProfileRef {
                    id: "mere.base".to_string(),
                    revision: 1,
                },
                action: RequestedAction {
                    domain: GRAPHSHELL_DOMAIN.to_string(),
                    path: PROJECTION_SERVICE.to_string(),
                    action: CONNECT_ACTION.to_string(),
                },
                class: TrafficClass::Interactive,
                subject: viewer().master_public_key().to_bytes(),
                delegations: vec![delegation],
            },
            facts: SessionFacts::authenticated(
                b"mere/graphshell/v1",
                CarrierKind::Memory,
                viewer().master_public_key().to_bytes(),
            ),
            limits: HandshakeLimits::default(),
        }
    }

    fn revoked_ledger(chain: &[SignedDelegationCertificate]) -> RevocationLedger {
        let mut ledger = RevocationLedger::new();
        let statement = SignedDelegationRevocation::issue(
            &owner(),
            personae::delegation::DelegationRevocation::new(
                chain[0].certificate.id(),
                owner().master_public_key().to_bytes(),
                chain[0].certificate.scope.clone(),
                NOW_MS,
                [2; 32],
            ),
        )
        .expect("issue revocation");
        assert!(ledger.fold(&statement), "revocation must verify");
        ledger
    }

    /// Mount a scene so cache-purge behaviour has something to act on.
    fn mount(client: &mut ClientState, session: &ProjectionSession, policy: CachePolicy) {
        let mut snapshot = ProjectionSnapshot {
            version: ProtocolVersion { major: 1, minor: 0 },
            session: session.clone(),
            scene: SceneSnapshot::from_dense(SceneEpoch(1), Revision(1), Scene::new())
                .expect("scene"),
            presentation: PresentationManifest::default(),
            cache_policy: CachePolicy::default(),
        };
        snapshot.cache_policy = policy;
        client.apply_snapshot(snapshot).expect("mount");
    }

    // --- reconnect ------------------------------------------------------

    #[test]
    fn a_reconnect_is_a_new_session_the_client_cannot_name() {
        let first = authority_at(PROJECTION_SERVICE, [21; 32]);
        let second = authority_at(PROJECTION_SERVICE, [22; 32]);
        assert_ne!(
            first.session(),
            second.session(),
            "a second admission must not land on the first session's id"
        );

        // The point of deriving the id: state cached under the first session
        // is unreachable from the second, without anyone enforcing a rule.
        let mut client = ClientState::default();
        mount(&mut client, first.session(), CachePolicy::default());
        assert!(client.mounted(first.session()).is_some());
        assert!(client.mounted(second.session()).is_none());
    }

    #[test]
    fn endpoint_context_carries_only_the_admitted_session_and_subject() {
        let authority = authority();
        let context = authority.endpoint_context();

        assert_eq!(context.session(), authority.session());
        assert_eq!(context.subject(), authority.principal().subject);
        assert_eq!(
            context,
            AdmittedEndpointContext::new(
                authority.session().clone(),
                authority.principal().subject,
            )
        );
    }

    // --- expiry ---------------------------------------------------------

    #[test]
    fn authority_holds_until_its_deadline_and_then_reports_when_it_ended() {
        let authority = authority();
        let ledger = RevocationLedger::default();
        assert_eq!(authority.deadline_ms(), Some(EXPIRY_MS));
        assert_eq!(authority.lapse(&ledger, EXPIRY_MS), None);
        assert_eq!(
            authority.lapse(&ledger, EXPIRY_MS + 1),
            // The deadline, not the observation time.
            Some(Lapse::Expired { at_ms: EXPIRY_MS })
        );
        assert_eq!(
            authority.status(&ledger, EXPIRY_MS + 1),
            SessionStatus::Expired
        );
    }

    // --- revocation -----------------------------------------------------

    #[test]
    fn a_revoked_grant_lapses_before_its_deadline_and_reports_revocation() {
        let chain = vec![grant(PROJECTION_SERVICE)];
        let authority = SessionAuthority::retain(principal([21; 32]), chain.clone());
        let ledger = revoked_ledger(&chain);
        assert_eq!(authority.lapse(&ledger, NOW_MS), Some(Lapse::Revoked));
        assert_eq!(authority.status(&ledger, NOW_MS), SessionStatus::Revoked);
    }

    #[test]
    fn a_carrier_session_retains_enough_authority_to_notice_revocation() {
        let admitted = admitted_authority();
        let authority = SessionAuthority::retain_admitted(&admitted);
        let ledger = revoked_ledger(&admitted.claims.delegations);

        assert_eq!(authority.lapse(&ledger, NOW_MS), Some(Lapse::Revoked));
        assert_eq!(authority.principal(), &admitted.principal);
    }

    #[test]
    fn revocation_outranks_expiry_when_both_are_true() {
        let chain = vec![grant(PROJECTION_SERVICE)];
        let authority = SessionAuthority::retain(principal([21; 32]), chain.clone());
        let ledger = revoked_ledger(&chain);
        assert_eq!(
            authority.lapse(&ledger, EXPIRY_MS + 1),
            Some(Lapse::Revoked),
            "an owner's withdrawal is the more useful thing to report"
        );
    }

    // --- cache purge ----------------------------------------------------

    #[test]
    fn a_lapse_purges_the_cache_when_the_policy_says_so() {
        let authority = authority();
        let mut client = ClientState::default();
        mount(&mut client, authority.session(), CachePolicy::default());

        let purged = apply_lapse(&mut client, authority.session(), Lapse::Revoked);
        assert!(purged);
        assert!(
            client.mounted(authority.session()).is_none(),
            "the scene must be gone, not merely relabelled"
        );
    }

    #[test]
    fn a_retaining_policy_keeps_the_scene_under_an_honest_status() {
        let authority = authority();
        let mut client = ClientState::default();
        mount(
            &mut client,
            authority.session(),
            CachePolicy {
                purge_on_revocation: false,
                ..CachePolicy::default()
            },
        );

        let purged = apply_lapse(&mut client, authority.session(), Lapse::Revoked);
        assert!(!purged);
        assert_eq!(
            client.mounted(authority.session()).map(|m| m.status),
            Some(SessionStatus::Revoked),
            "a retained scene must not still read as Live"
        );
    }

    // --- denied score ---------------------------------------------------

    #[test]
    fn a_service_wide_grant_discloses_any_score() {
        let authority = authority();
        let ledger = RevocationLedger::default();
        assert_eq!(authority.authorize_score(&ledger, NOW_MS, "spiral"), Ok(()));
    }

    #[test]
    fn a_single_score_grant_does_not_reach_a_neighbour_that_shares_its_prefix() {
        let authority = authority_at(&score_path("spiral"), [21; 32]);
        let ledger = RevocationLedger::default();
        assert_eq!(authority.authorize_score(&ledger, NOW_MS, "spiral"), Ok(()));
        assert_eq!(
            authority.authorize_score(&ledger, NOW_MS, "spiral-admin"),
            Err(ScoreDenial::NotCovered {
                score: "spiral-admin".to_string()
            }),
            "the scope grammar's `/` boundary is what makes per-score grants safe"
        );
    }

    #[test]
    fn a_lapsed_session_discloses_no_score_at_all() {
        let chain = vec![grant(PROJECTION_SERVICE)];
        let authority = SessionAuthority::retain(principal([21; 32]), chain.clone());
        let ledger = revoked_ledger(&chain);
        assert_eq!(
            authority.authorize_score(&ledger, NOW_MS, "spiral"),
            Err(ScoreDenial::Lapsed(Lapse::Revoked))
        );
    }

    // --- stale intent ---------------------------------------------------

    fn invocation(
        session: &ProjectionSession,
        epoch: SceneEpoch,
        revision: Revision,
    ) -> IntentInvocation {
        IntentInvocation {
            session: session.clone(),
            target: InstanceId(1),
            observed_epoch: epoch,
            observed_revision: revision,
            intent: "open".to_string(),
            payload: Vec::new(),
        }
    }

    #[test]
    fn an_intent_on_the_current_revision_is_accepted() {
        let authority = authority();
        let result = adjudicate_intent(
            &authority,
            &RevocationLedger::default(),
            NOW_MS,
            &invocation(authority.session(), SceneEpoch(1), Revision(4)),
            SceneEpoch(1),
            Revision(4),
        );
        assert_eq!(result, IntentResult::Accepted);
    }

    #[test]
    fn an_intent_against_a_superseded_revision_is_stale() {
        let authority = authority();
        let result = adjudicate_intent(
            &authority,
            &RevocationLedger::default(),
            NOW_MS,
            &invocation(authority.session(), SceneEpoch(1), Revision(3)),
            SceneEpoch(1),
            Revision(4),
        );
        assert_eq!(
            result,
            IntentResult::Stale {
                current_epoch: SceneEpoch(1),
                current_revision: Revision(4),
            }
        );
    }

    /// The ordering that matters: a lapsed session is refused outright rather
    /// than told what the current revision is.
    #[test]
    fn a_lapsed_session_is_refused_without_being_told_the_current_revision() {
        let chain = vec![grant(PROJECTION_SERVICE)];
        let authority = SessionAuthority::retain(principal([21; 32]), chain.clone());
        let ledger = revoked_ledger(&chain);
        let result = adjudicate_intent(
            &authority,
            &ledger,
            NOW_MS,
            &invocation(authority.session(), SceneEpoch(1), Revision(3)),
            SceneEpoch(1),
            Revision(4),
        );
        match result {
            IntentResult::Rejected { reason } => {
                assert!(reason.contains("revoked"), "reason was: {reason}");
            }
            other => panic!("a lapsed session must not receive Stale: {other:?}"),
        }
    }

    #[test]
    fn an_intent_naming_another_session_is_rejected() {
        let authority = authority();
        let other = authority_at(PROJECTION_SERVICE, [99; 32]);
        let result = adjudicate_intent(
            &authority,
            &RevocationLedger::default(),
            NOW_MS,
            &invocation(other.session(), SceneEpoch(1), Revision(4)),
            SceneEpoch(1),
            Revision(4),
        );
        assert!(matches!(result, IntentResult::Rejected { .. }));
    }
}
