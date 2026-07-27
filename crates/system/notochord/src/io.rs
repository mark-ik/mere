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

/// Failure while exchanging one length-prefixed frame.
///
/// Separate from [`IoHandshakeError`] because the frame primitives below are
/// public and serve application traffic too, where "the handshake failed" is
/// the wrong word for an oversized post.
#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    /// The peer hung up mid-frame, or the socket failed.
    #[error("frame transport failed: {0}")]
    Transport(#[from] std::io::Error),
    /// The frame is larger than the bound this call was given. Carries both
    /// numbers because "too large" without them is unactionable in a log.
    #[error("frame of {len} bytes exceeds the {max} byte bound")]
    TooLarge { len: u64, max: u32 },
}

impl From<FrameError> for IoHandshakeError {
    fn from(error: FrameError) -> Self {
        match error {
            FrameError::Transport(io) => IoHandshakeError::Transport(io),
            FrameError::TooLarge { .. } => IoHandshakeError::Frame(HandshakeError::TooLarge),
        }
    }
}

/// Write one length-prefixed frame, refusing to send past `max_bytes`.
///
/// The bound is checked on the way out, not only on the way in. The earlier
/// private version cast `payload.len() as u32`, which silently truncates the
/// prefix for a payload over 4 GiB and would frame the stream wrong rather
/// than fail: every subsequent frame boundary on that connection is then
/// garbage. Unreachable in practice today, and still not something to leave
/// as a silent cast in a primitive other services are about to call with
/// their own payloads.
///
/// `max_bytes` is the caller's: the handshake passes its own limits, a
/// service passes the bound it holds its application frames to. This
/// deliberately does not read a bound out of [`HandshakeLimits`], which
/// describes handshake attack surface rather than what any service's payload
/// may weigh.
pub async fn write_frame<S>(
    stream: &mut S,
    payload: &[u8],
    max_bytes: u32,
) -> Result<(), FrameError>
where
    S: AsyncWrite + Unpin,
{
    let len = u32::try_from(payload.len()).map_err(|_| FrameError::TooLarge {
        len: payload.len() as u64,
        max: max_bytes,
    })?;
    if len > max_bytes {
        return Err(FrameError::TooLarge {
            len: u64::from(len),
            max: max_bytes,
        });
    }
    stream.write_all(&len.to_be_bytes()).await?;
    stream.write_all(payload).await?;
    stream.flush().await?;
    Ok(())
}

/// Read one length-prefixed frame, refusing an oversized one before reading
/// its body. A hostile peer cannot make this allocate past `max_bytes`.
///
/// End of stream is a failure here: a handshake that stops mid-exchange did
/// not happen. A service reading application frames wants
/// [`read_frame_or_eof`] instead, where the peer having nothing more to say
/// is how a session ends normally.
pub async fn read_frame<S>(stream: &mut S, max_bytes: u32) -> Result<Vec<u8>, FrameError>
where
    S: AsyncRead + Unpin,
{
    let mut length = [0u8; 4];
    stream.read_exact(&mut length).await?;
    read_body(stream, u32::from_be_bytes(length), max_bytes).await
}

/// Read one length-prefixed frame, or `None` at a clean end of stream.
///
/// "Clean" means the peer closed *between* frames. A stream that ends partway
/// through a length prefix or a body is still an error: that is a truncated
/// frame, not a finished session, and treating the two alike would let a
/// severed connection read as an orderly goodbye.
pub async fn read_frame_or_eof<S>(
    stream: &mut S,
    max_bytes: u32,
) -> Result<Option<Vec<u8>>, FrameError>
where
    S: AsyncRead + Unpin,
{
    let mut length = [0u8; 4];
    match stream.read_exact(&mut length).await {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    read_body(stream, u32::from_be_bytes(length), max_bytes)
        .await
        .map(Some)
}

async fn read_body<S>(stream: &mut S, length: u32, max_bytes: u32) -> Result<Vec<u8>, FrameError>
where
    S: AsyncRead + Unpin,
{
    if length > max_bytes {
        return Err(FrameError::TooLarge {
            len: u64::from(length),
            max: max_bytes,
        });
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
    write_frame(stream, &reply, limits.max_reply_bytes).await?;
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
    write_frame(&mut stream, &reply, limits.max_reply_bytes).await?;
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
    write_frame(stream, &bytes, limits.max_hello_bytes).await?;
    let reply = read_frame(stream, limits.max_reply_bytes).await?;
    Ok(SessionReply::decode(&reply, &limits)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three behaviours that differed between this module's private
    /// framing and the copy Murm kept. Each one is the reason the two could
    /// not simply be merged.
    #[tokio::test]
    async fn a_write_past_the_bound_is_refused_rather_than_truncated() {
        // The old private writer took no maximum and cast `len as u32`. It
        // would have framed this stream with a wrong prefix instead of
        // failing, corrupting every later boundary on the connection.
        let mut sink = Vec::new();
        let error = write_frame(&mut sink, &[0u8; 64], 32)
            .await
            .expect_err("a frame over the bound must not go out");
        assert!(matches!(error, FrameError::TooLarge { len: 64, max: 32 }));
        assert!(sink.is_empty(), "nothing may be written before the refusal");
    }

    #[tokio::test]
    async fn a_clean_end_of_stream_is_completion_for_a_service_and_failure_for_a_handshake() {
        let mut framed = Vec::new();
        write_frame(&mut framed, b"one", 1024).await.unwrap();

        // A service reading application frames: one frame, then a clean end.
        let mut cursor = std::io::Cursor::new(framed.clone());
        assert_eq!(
            read_frame_or_eof(&mut cursor, 1024)
                .await
                .unwrap()
                .as_deref(),
            Some(&b"one"[..])
        );
        assert_eq!(
            read_frame_or_eof(&mut cursor, 1024).await.unwrap(),
            None,
            "the peer having nothing more to say ends a session normally"
        );

        // The handshake reading the same end of stream: it did not happen.
        let mut empty = std::io::Cursor::new(Vec::new());
        assert!(
            read_frame(&mut empty, 1024).await.is_err(),
            "a handshake that stops mid-exchange is a failure, not a goodbye"
        );
    }

    #[tokio::test]
    async fn a_truncated_frame_is_an_error_not_a_clean_end() {
        // The distinction a severed connection would otherwise blur: this
        // stream ends *inside* a frame, so it must not read as an orderly
        // close.
        let mut framed = Vec::new();
        write_frame(&mut framed, b"payload", 1024).await.unwrap();
        framed.truncate(framed.len() - 3);

        let mut cursor = std::io::Cursor::new(framed);
        assert!(
            read_frame_or_eof(&mut cursor, 1024).await.is_err(),
            "a body cut short is a truncated frame, not a finished session"
        );
    }

    #[tokio::test]
    async fn an_oversized_frame_is_refused_before_its_body_is_allocated() {
        // The bound the reader always had, kept: the length prefix claims far
        // more than the caller allows, and no allocation of that size happens.
        let mut framed = Vec::new();
        framed.extend_from_slice(&u32::to_be_bytes(1_000_000));
        let mut cursor = std::io::Cursor::new(framed);
        assert!(matches!(
            read_frame(&mut cursor, 64).await,
            Err(FrameError::TooLarge {
                len: 1_000_000,
                max: 64
            })
        ));
    }
}
