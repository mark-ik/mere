//! An `AsyncRead + AsyncWrite` bridge over a Reticulum link.
//!
//! Reticulum v0.1.0 exposes no stream abstraction (its `buffer` module is
//! low-level byte helpers, not a `BufferedReader`/`Writer`), so the stream is
//! built directly on link data packets: a tokio [`DuplexStream`] whose far end
//! is driven by two relay tasks that move bytes between the duplex and the link.
//!
//! Reticulum data packets are unsequenced and best-effort at the link layer
//! (there is no retransmission or reordering above the interface). Over a
//! reliable interface (TCP loopback) that is invisible; over a lossy medium
//! (LoRa) a real reliability layer would be needed. This is a bilateral-lane
//! probe, so that limitation is accepted and documented, not solved here.

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, DuplexStream, ReadBuf};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use reticulum::destination::link::{LinkEvent, LinkEventData, LinkId};
use reticulum::hash::AddressHash;
use reticulum::transport::Transport as ReticulumStack;

/// In-memory buffer size for the bridging [`DuplexStream`].
const DUPLEX_BUF: usize = 64 * 1024;

/// Max plaintext bytes per Reticulum data packet. `PACKET_MDU` is 2048; Fernet
/// framing adds IV + HMAC overhead, so 1024 leaves comfortable headroom.
const LINK_CHUNK_SIZE: usize = 1024;

/// Which side of a link a stream drives, and the destination address its
/// outbound data packets are routed to.
#[derive(Clone, Copy)]
pub(super) enum LinkSide {
    /// Outbound link created by `connect`; send via out-links to the peer's
    /// destination address.
    Out(AddressHash),
    /// Inbound link accepted from a peer; send via in-links on our own
    /// destination address.
    In(AddressHash),
}

/// Bidirectional byte stream over a Reticulum link.
///
/// Delegates [`AsyncRead`]/[`AsyncWrite`] to an internal [`DuplexStream`]; the
/// far end is serviced by relay tasks that are aborted when the stream drops.
pub struct ReticulumStream {
    inner: DuplexStream,
    tasks: Vec<JoinHandle<()>>,
}

impl std::fmt::Debug for ReticulumStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReticulumStream").finish_non_exhaustive()
    }
}

impl Drop for ReticulumStream {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

/// Bridge a Reticulum link to a byte stream.
///
/// `events` must already be positioned past the link's `Activated` event (the
/// caller waits for activation first). `link_id` demultiplexes this link's data
/// from other links sharing the same broadcast channel.
pub(super) fn bridge_link(
    inner: Arc<ReticulumStack>,
    events: broadcast::Receiver<LinkEventData>,
    side: LinkSide,
    link_id: LinkId,
) -> ReticulumStream {
    let (mine, theirs) = tokio::io::duplex(DUPLEX_BUF);
    let (mut read_half, mut write_half) = tokio::io::split(theirs);

    // Outbound: bytes the caller writes become Reticulum data packets.
    let send_inner = Arc::clone(&inner);
    let writer = tokio::spawn(async move {
        let mut buf = [0u8; LINK_CHUNK_SIZE];
        loop {
            match read_half.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => match side {
                    LinkSide::Out(addr) => send_inner.send_to_out_links(&addr, &buf[..n]).await,
                    LinkSide::In(addr) => send_inner.send_to_in_links(&addr, &buf[..n]).await,
                },
            }
        }
    });

    // Inbound: this link's data events become bytes the caller can read.
    let mut events = events;
    let reader = tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(ev) => {
                    if ev.id != link_id {
                        continue;
                    }
                    match ev.event {
                        LinkEvent::Data(payload) => {
                            if write_half.write_all(payload.as_slice()).await.is_err() {
                                break;
                            }
                        }
                        LinkEvent::Closed => break,
                        LinkEvent::Activated => {}
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    ReticulumStream {
        inner: mine,
        tasks: vec![writer, reader],
    }
}

impl AsyncRead for ReticulumStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for ReticulumStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
