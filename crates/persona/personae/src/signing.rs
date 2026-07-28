//! Approval and receipt boundary for native signing adapters.
//!
//! The broker carries only public request facts and a payload digest. Secret
//! key bytes and cleartext payloads remain inside the adapter that performs the
//! cryptographic operation.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::UnlockTier;

/// Facts a signing carrier can prove without disclosing the payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SigningRequest {
    /// Stable id used by the approval intent.
    pub request_id: Uuid,
    /// Selected Personae profile.
    pub profile: String,
    /// Public key fingerprint, never the private slot payload.
    pub public_key_fingerprint: String,
    /// Adapter-scoped operation name, such as `ssh.sign`.
    pub operation: String,
    /// BLAKE3 digest of the payload presented to the signer.
    pub payload_digest: String,
    /// Native adapter that produced the request.
    pub adapter: String,
    /// Wall-clock request time.
    pub requested_at_ms: u64,
    /// Authenticated requester identity, when the carrier proves one.
    pub authenticated_requester: Option<String>,
    /// Authenticated local process, when the carrier proves one.
    pub authenticated_process: Option<String>,
    /// Authenticated target, such as a verified SSH host key.
    pub authenticated_target: Option<String>,
    /// Authenticated session binding digest, when available.
    pub session_binding: Option<String>,
    /// Related public graph object or Graphshell session, when one exists.
    pub related_object: Option<String>,
}

impl SigningRequest {
    /// Build a secret-free request from an opaque payload.
    pub fn new(
        profile: impl Into<String>,
        public_key_fingerprint: impl Into<String>,
        operation: impl Into<String>,
        payload: &[u8],
        adapter: impl Into<String>,
    ) -> Self {
        Self {
            request_id: Uuid::new_v4(),
            profile: profile.into(),
            public_key_fingerprint: public_key_fingerprint.into(),
            operation: operation.into(),
            payload_digest: format!("blake3:{}", blake3::hash(payload).to_hex()),
            adapter: adapter.into(),
            requested_at_ms: now_ms(),
            authenticated_requester: None,
            authenticated_process: None,
            authenticated_target: None,
            session_binding: None,
            related_object: None,
        }
    }

    /// Attach a target proved by the carrier.
    pub fn with_authenticated_target(mut self, target: impl Into<String>) -> Self {
        self.authenticated_target = Some(target.into());
        self
    }

    /// Attach a session binding proved by the carrier.
    pub fn with_session_binding(mut self, binding: impl Into<String>) -> Self {
        self.session_binding = Some(binding.into());
        self
    }

    /// Attach a related public graph object or session.
    pub fn with_related_object(mut self, related: impl Into<String>) -> Self {
        self.related_object = Some(related.into());
        self
    }
}

/// Approval behavior selected for one key and adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SigningPolicy {
    /// The unlocked native session may sign without another prompt.
    Session,
    /// One approval remains valid until the configured idle window expires.
    ShortTtl {
        /// Idle window in seconds.
        idle_seconds: u32,
    },
    /// Every request waits for a visible decision.
    PerUse,
}

impl From<UnlockTier> for SigningPolicy {
    fn from(tier: UnlockTier) -> Self {
        match tier {
            UnlockTier::Session => Self::Session,
            UnlockTier::ShortTtl { idle_seconds } => Self::ShortTtl { idle_seconds },
            UnlockTier::PerUse => Self::PerUse,
        }
    }
}

/// Bounded scope requested by an approval decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RememberApproval {
    /// Approve only the pending request.
    Once,
    /// Use the key's configured short idle window.
    UntilIdle,
}

/// Visible user decision for a pending signing request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SigningDecision {
    /// Approve under the stated bounded remember scope.
    Approve {
        /// How long the approval may remain useful.
        remember: RememberApproval,
    },
    /// Refuse the request.
    Deny,
}

/// Why a signing request was authorized.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalSource {
    /// The key's session policy allowed the request.
    SessionPolicy,
    /// A still-live short-TTL approval allowed the request.
    CachedShortTtl,
    /// A visible user approved only this request.
    UserOnce,
    /// A visible user approved the configured short-TTL window.
    UserUntilIdle,
}

/// One successful authorization, completed by the signing adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SigningAuthorization {
    /// Secret-free request facts.
    pub request: SigningRequest,
    /// Policy enforced for this request.
    pub policy: SigningPolicy,
    /// Source of the approval.
    pub source: ApprovalSource,
}

/// Final result retained in the signing history.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SigningRecordResult {
    /// The native adapter produced a signature.
    Signed {
        /// Public reference to the result, normally a digest.
        signature_ref: String,
    },
    /// The user denied the request.
    Denied,
    /// The visible request expired without a decision.
    TimedOut,
    /// The native adapter failed after authorization.
    Failed {
        /// Bounded failure class. Free-form adapter errors are not retained.
        code: SigningFailureCode,
    },
}

/// Secret-safe failure classes retained by signing history.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SigningFailureCode {
    /// The requester disappeared while its decision was pending.
    ApprovalChannelClosed,
    /// The native adapter could not complete the authorized operation.
    AdapterFailure,
}

/// Append-only, secret-free decision and result record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SigningRecord {
    /// Request facts and payload digest.
    pub request: SigningRequest,
    /// Policy enforced for the request.
    pub policy: SigningPolicy,
    /// Approval source when authorization succeeded.
    pub approval_source: Option<ApprovalSource>,
    /// Final result.
    pub result: SigningRecordResult,
    /// Completion time.
    pub completed_at_ms: u64,
}

/// One request available to Graphshell's approval surface.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingSigningRequest {
    /// Secret-free request facts.
    pub request: SigningRequest,
    /// Policy enforced for the request.
    pub policy: SigningPolicy,
    /// Deadline after which the request becomes a timeout record.
    pub expires_at_ms: u64,
}

/// Authorization failure returned to the native adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorizationError {
    /// A visible user denied the request.
    Denied,
    /// No decision arrived before the configured deadline.
    TimedOut,
    /// The decision channel closed unexpectedly.
    BrokerClosed,
}

impl std::fmt::Display for AuthorizationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Denied => write!(formatter, "signing request denied"),
            Self::TimedOut => write!(formatter, "signing request timed out"),
            Self::BrokerClosed => write!(formatter, "signing approval broker closed"),
        }
    }
}

impl std::error::Error for AuthorizationError {}

/// Invalid or stale visible decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecisionError {
    /// The request is no longer pending.
    NotPending,
    /// The key policy does not permit the requested remember scope.
    RememberNotAllowed,
    /// The waiting adapter disappeared before receiving the decision.
    RequestClosed,
}

impl std::fmt::Display for DecisionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotPending => write!(formatter, "signing request is not pending"),
            Self::RememberNotAllowed => {
                write!(
                    formatter,
                    "the key policy does not allow that remember scope"
                )
            }
            Self::RequestClosed => write!(formatter, "signing requester is no longer waiting"),
        }
    }
}

impl std::error::Error for DecisionError {}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ApprovalCacheKey {
    profile: String,
    fingerprint: String,
    operation: String,
    adapter: String,
    requester: Option<String>,
    process: Option<String>,
    target: Option<String>,
    session: Option<String>,
}

impl From<&SigningRequest> for ApprovalCacheKey {
    fn from(request: &SigningRequest) -> Self {
        Self {
            profile: request.profile.clone(),
            fingerprint: request.public_key_fingerprint.clone(),
            operation: request.operation.clone(),
            adapter: request.adapter.clone(),
            requester: request.authenticated_requester.clone(),
            process: request.authenticated_process.clone(),
            target: request.authenticated_target.clone(),
            session: request.session_binding.clone(),
        }
    }
}

struct ResolvedDecision {
    decision: SigningDecision,
}

struct PendingEntry {
    request: SigningRequest,
    policy: SigningPolicy,
    expires_at_ms: u64,
    sender: oneshot::Sender<ResolvedDecision>,
}

#[derive(Default)]
struct BrokerState {
    pending: HashMap<Uuid, PendingEntry>,
    history: Vec<SigningRecord>,
    short_ttl: HashMap<ApprovalCacheKey, u64>,
}

/// Shared native approval broker used by Graphshell and signing adapters.
#[derive(Clone)]
pub struct ApprovalBroker {
    state: Arc<Mutex<BrokerState>>,
    decision_timeout: Duration,
}

impl ApprovalBroker {
    /// Build a broker with a bounded decision timeout.
    pub fn new(decision_timeout: Duration) -> Self {
        Self {
            state: Arc::new(Mutex::new(BrokerState::default())),
            decision_timeout,
        }
    }

    /// Authorize one request under its configured policy.
    pub async fn authorize(
        &self,
        request: SigningRequest,
        policy: SigningPolicy,
    ) -> Result<SigningAuthorization, AuthorizationError> {
        let now = now_ms();
        match policy {
            SigningPolicy::Session => {
                return Ok(SigningAuthorization {
                    request,
                    policy,
                    source: ApprovalSource::SessionPolicy,
                });
            }
            SigningPolicy::ShortTtl { idle_seconds } => {
                let key = ApprovalCacheKey::from(&request);
                let mut state = self.state.lock().unwrap();
                state.short_ttl.retain(|_, expires| *expires > now);
                if state
                    .short_ttl
                    .get(&key)
                    .is_some_and(|expires| *expires > now)
                {
                    state.short_ttl.insert(
                        key,
                        now.saturating_add(u64::from(idle_seconds).saturating_mul(1_000)),
                    );
                    return Ok(SigningAuthorization {
                        request,
                        policy,
                        source: ApprovalSource::CachedShortTtl,
                    });
                }
            }
            SigningPolicy::PerUse => {}
        }

        self.await_visible_decision(request, policy).await
    }

    async fn await_visible_decision(
        &self,
        request: SigningRequest,
        policy: SigningPolicy,
    ) -> Result<SigningAuthorization, AuthorizationError> {
        let (sender, receiver) = oneshot::channel();
        let expires_at_ms = now_ms().saturating_add(self.decision_timeout.as_millis() as u64);
        self.state.lock().unwrap().pending.insert(
            request.request_id,
            PendingEntry {
                request: request.clone(),
                policy,
                expires_at_ms,
                sender,
            },
        );

        match tokio::time::timeout(self.decision_timeout, receiver).await {
            Ok(Ok(resolved)) => match resolved.decision {
                SigningDecision::Approve { remember } => Ok(SigningAuthorization {
                    request,
                    policy,
                    source: match remember {
                        RememberApproval::Once => ApprovalSource::UserOnce,
                        RememberApproval::UntilIdle => ApprovalSource::UserUntilIdle,
                    },
                }),
                SigningDecision::Deny => {
                    self.push_terminal_record(request, policy, None, SigningRecordResult::Denied);
                    Err(AuthorizationError::Denied)
                }
            },
            Ok(Err(_)) => {
                self.state
                    .lock()
                    .unwrap()
                    .pending
                    .remove(&request.request_id);
                self.push_terminal_record(
                    request,
                    policy,
                    None,
                    SigningRecordResult::Failed {
                        code: SigningFailureCode::ApprovalChannelClosed,
                    },
                );
                Err(AuthorizationError::BrokerClosed)
            }
            Err(_) => {
                self.state
                    .lock()
                    .unwrap()
                    .pending
                    .remove(&request.request_id);
                self.push_terminal_record(request, policy, None, SigningRecordResult::TimedOut);
                Err(AuthorizationError::TimedOut)
            }
        }
    }

    /// Resolve one pending request from Graphshell's visible approval surface.
    pub fn decide(&self, request_id: Uuid, decision: SigningDecision) -> Result<(), DecisionError> {
        let mut state = self.state.lock().unwrap();
        let Some(entry) = state.pending.remove(&request_id) else {
            return Err(DecisionError::NotPending);
        };

        if matches!(
            (entry.policy, decision),
            (
                SigningPolicy::PerUse | SigningPolicy::Session,
                SigningDecision::Approve {
                    remember: RememberApproval::UntilIdle
                }
            )
        ) {
            state.pending.insert(request_id, entry);
            return Err(DecisionError::RememberNotAllowed);
        }

        let short_ttl_cache = if let (
            SigningPolicy::ShortTtl { idle_seconds },
            SigningDecision::Approve {
                remember: RememberApproval::UntilIdle,
            },
        ) = (entry.policy, decision)
        {
            Some((
                ApprovalCacheKey::from(&entry.request),
                now_ms().saturating_add(u64::from(idle_seconds).saturating_mul(1_000)),
            ))
        } else {
            None
        };

        entry
            .sender
            .send(ResolvedDecision { decision })
            .map_err(|_| DecisionError::RequestClosed)?;
        if let Some((key, expires_at_ms)) = short_ttl_cache {
            state.short_ttl.insert(key, expires_at_ms);
        }
        Ok(())
    }

    /// Append the final result for an authorized request.
    pub fn complete(&self, authorization: SigningAuthorization, result: SigningRecordResult) {
        self.push_terminal_record(
            authorization.request,
            authorization.policy,
            Some(authorization.source),
            result,
        );
    }

    /// Current visible requests, ordered by request time then id.
    pub fn pending(&self) -> Vec<PendingSigningRequest> {
        let mut pending: Vec<_> = self
            .state
            .lock()
            .unwrap()
            .pending
            .values()
            .map(|entry| PendingSigningRequest {
                request: entry.request.clone(),
                policy: entry.policy,
                expires_at_ms: entry.expires_at_ms,
            })
            .collect();
        pending.sort_by_key(|entry| (entry.request.requested_at_ms, entry.request.request_id));
        pending
    }

    /// Append-only signing decision/result history.
    pub fn history(&self) -> Vec<SigningRecord> {
        self.state.lock().unwrap().history.clone()
    }

    fn push_terminal_record(
        &self,
        request: SigningRequest,
        policy: SigningPolicy,
        approval_source: Option<ApprovalSource>,
        result: SigningRecordResult,
    ) {
        self.state.lock().unwrap().history.push(SigningRecord {
            request,
            policy,
            approval_source,
            result,
            completed_at_ms: now_ms(),
        });
    }
}

impl Default for ApprovalBroker {
    fn default() -> Self {
        Self::new(Duration::from_secs(120))
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(payload: &[u8]) -> SigningRequest {
        SigningRequest::new("research", "SHA256:test", "ssh.sign", payload, "ssh-agent")
    }

    async fn wait_for_pending(broker: &ApprovalBroker) -> PendingSigningRequest {
        for _ in 0..100 {
            if let Some(pending) = broker.pending().into_iter().next() {
                return pending;
            }
            tokio::task::yield_now().await;
        }
        panic!("request never became pending");
    }

    #[tokio::test]
    async fn per_use_waits_for_a_visible_decision_and_records_the_result() {
        let broker = ApprovalBroker::new(Duration::from_secs(1));
        let waiting = broker.clone();
        let task = tokio::spawn(async move {
            waiting
                .authorize(request(b"payload-secret"), SigningPolicy::PerUse)
                .await
        });
        let pending = wait_for_pending(&broker).await;
        assert_eq!(pending.request.payload_digest.len(), "blake3:".len() + 64);
        broker
            .decide(
                pending.request.request_id,
                SigningDecision::Approve {
                    remember: RememberApproval::Once,
                },
            )
            .unwrap();
        let authorized = task.await.unwrap().unwrap();
        broker.complete(
            authorized,
            SigningRecordResult::Signed {
                signature_ref: "blake3:signature".to_string(),
            },
        );

        let history = broker.history();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].approval_source, Some(ApprovalSource::UserOnce));
        let json = serde_json::to_string(&history).unwrap();
        assert!(!json.contains("payload-secret"));
    }

    #[tokio::test]
    async fn denial_returns_to_the_adapter_and_appends_one_record() {
        let broker = ApprovalBroker::new(Duration::from_secs(1));
        let waiting = broker.clone();
        let task = tokio::spawn(async move {
            waiting
                .authorize(request(b"deny-me"), SigningPolicy::PerUse)
                .await
        });
        let pending = wait_for_pending(&broker).await;
        broker
            .decide(pending.request.request_id, SigningDecision::Deny)
            .unwrap();
        assert_eq!(task.await.unwrap(), Err(AuthorizationError::Denied));
        assert_eq!(broker.history().len(), 1);
        assert_eq!(broker.history()[0].result, SigningRecordResult::Denied);
    }

    #[tokio::test]
    async fn short_ttl_reuses_then_expires_the_visible_approval() {
        let broker = ApprovalBroker::new(Duration::from_secs(1));
        let policy = SigningPolicy::ShortTtl { idle_seconds: 1 };
        let waiting = broker.clone();
        let first = tokio::spawn(async move { waiting.authorize(request(b"a"), policy).await });
        let pending = wait_for_pending(&broker).await;
        broker
            .decide(
                pending.request.request_id,
                SigningDecision::Approve {
                    remember: RememberApproval::UntilIdle,
                },
            )
            .unwrap();
        assert_eq!(
            first.await.unwrap().unwrap().source,
            ApprovalSource::UserUntilIdle
        );

        assert_eq!(
            broker
                .authorize(request(b"b"), policy)
                .await
                .unwrap()
                .source,
            ApprovalSource::CachedShortTtl
        );
        tokio::time::sleep(Duration::from_millis(1_100)).await;

        let waiting = broker.clone();
        let after_expiry =
            tokio::spawn(async move { waiting.authorize(request(b"c"), policy).await });
        let pending = wait_for_pending(&broker).await;
        broker
            .decide(pending.request.request_id, SigningDecision::Deny)
            .unwrap();
        assert_eq!(after_expiry.await.unwrap(), Err(AuthorizationError::Denied));
    }

    #[tokio::test]
    async fn per_use_cannot_be_widened_into_a_cached_approval() {
        let broker = ApprovalBroker::new(Duration::from_secs(1));
        let waiting = broker.clone();
        let task = tokio::spawn(async move {
            waiting
                .authorize(request(b"x"), SigningPolicy::PerUse)
                .await
        });
        let pending = wait_for_pending(&broker).await;
        assert_eq!(
            broker.decide(
                pending.request.request_id,
                SigningDecision::Approve {
                    remember: RememberApproval::UntilIdle,
                },
            ),
            Err(DecisionError::RememberNotAllowed)
        );
        broker
            .decide(pending.request.request_id, SigningDecision::Deny)
            .unwrap();
        assert_eq!(task.await.unwrap(), Err(AuthorizationError::Denied));
    }
}
