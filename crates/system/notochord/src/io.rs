//! Async framing for the handshake, behind the `tokio` feature.
//!
//! The decision logic in [`crate::handshake`] is sans-io and stays that way;
//! this is the small amount of socket work every caller would otherwise
//! duplicate. One `u32` big-endian length prefix, bounded before a single
//! payload byte is read, then the sans-io path.
//!
//! Both halves are deliberately shaped so the application stream is only
//! reachable after a decision: [`accept_session`] hands back the decision, and
//! the caller keeps or drops the stream accordingly.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::chain::RevocationLedger;
use crate::facts::SessionFacts;
use crate::handshake::{
    AdmittedSession, HandshakeError, SessionHello, SessionReply, admit, respond,
};
use crate::policy::LocalNetworkPolicy;
use crate::types::{DenyReason, HandshakeLimits, SessionDecision};

/// Failure while exchanging a handshake frame.
#[derive(Debug, thiserror::Error)]
pub enum IoHandshakeError {
    /// The peer hung up, or the socket failed.
    #[error("handshake transport failed: {0}")]
    Transport(#[from] std::io::Error),
    /// The frame was refused by the bounded codec.
    #[error(transparent)]
    Frame(#[from] HandshakeError),
}

async fn write_frame<S>(stream: &mut S, payload: &[u8]) -> Result<(), IoHandshakeError>
where
    S: AsyncWrite + Unpin,
{
    stream
        .write_all(&(payload.len() as u32).to_be_bytes())
        .await?;
    stream.write_all(payload).await?;
    stream.flush().await?;
    Ok(())
}

/// Read one length-prefixed frame, refusing an oversized one before reading
/// its body. A hostile peer cannot make this allocate past `max_bytes`.
async fn read_frame<S>(stream: &mut S, max_bytes: u32) -> Result<Vec<u8>, IoHandshakeError>
where
    S: AsyncRead + Unpin,
{
    let mut length = [0u8; 4];
    stream.read_exact(&mut length).await?;
    let length = u32::from_be_bytes(length);
    if length > max_bytes {
        return Err(HandshakeError::TooLarge.into());
    }
    let mut payload = vec![0u8; length as usize];
    stream.read_exact(&mut payload).await?;
    Ok(payload)
}

/// Responder half: read the hello, decide, write the reply.
///
/// Returns the decision. The caller hands the stream to the application only
/// when it accepts; on a refusal the reply has already been written and the
/// stream should be dropped without the application ever seeing it.
pub async fn accept_session<S>(
    stream: &mut S,
    policy: &LocalNetworkPolicy,
    ledger: &RevocationLedger,
    facts: &SessionFacts,
    now_ms: u64,
    active_sessions: u32,
) -> Result<SessionDecision, IoHandshakeError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let limits = policy.limits.clamped();
    let hello = read_frame(stream, limits.max_hello_bytes).await?;
    let (reply, decision) = respond(policy, ledger, &hello, facts, now_ms, active_sessions);
    write_frame(stream, &reply).await?;
    if !decision.is_accept() {
        finish_refusal(stream).await;
    }
    Ok(decision)
}

/// Finish a refused stream rather than leaving it to be dropped.
///
/// Both carriers happen to deliver a flushed-then-dropped refusal today: a
/// retinue relay reads its duplex to EOF before closing the link, and quinn's
/// `SendStream::drop` calls `finish`. Relying on that is relying on two
/// unrelated `Drop` implementations staying correct, and on nobody dropping
/// the endpoint underneath them, which is precisely the failure this lane
/// already hit once. `poll_shutdown` is implemented on both stream types and,
/// before this, had no callers anywhere in the tree. A refusal is finished
/// now, not merely flushed.
///
/// Best-effort on purpose: the decision is already made and the reply already
/// written, so a peer that has hung up cannot turn a refusal into an error.
async fn finish_refusal<S>(stream: &mut S)
where
    S: AsyncWrite + Unpin,
{
    let _ = stream.shutdown().await;
}

/// Responder half that yields the admitted session.
///
/// The shape a service carrier wants, and the one N2's two halves share: it
/// takes the stream, runs the bounded handshake, and hands back either an
/// [`AdmittedSession`] carrying the principal and the carrier's facts, or the
/// reason it was refused. A refusal consumes the stream and finishes it, so
/// there is no way to accidentally pass a refused stream to an application.
pub async fn admit_session<S>(
    mut stream: S,
    policy: &LocalNetworkPolicy,
    ledger: &RevocationLedger,
    facts: &SessionFacts,
    now_ms: u64,
    active_sessions: u32,
) -> Result<Result<AdmittedSession<S>, DenyReason>, IoHandshakeError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let limits = policy.limits.clamped();
    let hello = read_frame(&mut stream, limits.max_hello_bytes).await?;
    let (reply, outcome) = admit(policy, ledger, &hello, facts, now_ms, active_sessions);
    write_frame(&mut stream, &reply).await?;
    match outcome {
        Ok(principal) => {
            // `admit` accepted this exact bounded frame, so this second decode
            // cannot turn unverified bytes into retained claims. Keeping the
            // public sans-I/O return shape stable avoids making every caller
            // carry claims it does not need.
            let claims = SessionHello::decode(&hello, &limits)?.claims();
            Ok(Ok(AdmittedSession {
                stream,
                principal,
                claims,
                facts: facts.clone(),
                limits,
            }))
        }
        Err(reason) => {
            finish_refusal(&mut stream).await;
            Ok(Err(reason))
        }
    }
}

/// Initiator half: write the hello, read the reply.
pub async fn initiate_session<S>(
    stream: &mut S,
    hello: &SessionHello,
    limits: &HandshakeLimits,
) -> Result<SessionReply, IoHandshakeError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let limits = limits.clamped();
    let bytes = hello.encode(&limits)?;
    write_frame(stream, &bytes).await?;
    let reply = read_frame(stream, limits.max_reply_bytes).await?;
    Ok(SessionReply::decode(&reply, &limits)?)
}
