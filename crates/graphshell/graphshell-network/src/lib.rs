//! The stream carrier: an endpoint reached across a connection.
//!
//! The third carrier, beside `graphshell-stdio` and `graphshell-local`, and
//! the one that makes remote projection real rather than architectural. It
//! carries the protocol over any byte stream and knows nothing about how that
//! stream was obtained: who dialled, which ALPN they asked for, and whether
//! anyone was admitted are all decided before a [`NetworkCarrier`] exists.
//!
//! That ignorance is the point. Admission vocabulary belongs to the service
//! being reached, so it lives in the port that owns the service, and the
//! carrier stays a carrier. `ports/graphshell`'s `network_carrier` module
//! supplies the projection service's half: the ALPN, the initiator binding,
//! and the dial that produces an admitted stream to hand to [`NetworkCarrier::over`].
//!
//! ## The blocking surface is deliberate
//!
//! [`Carrier`] blocks, and this carrier drives an async stream behind it on a
//! runtime it holds. That was settled when the in-process carrier showed
//! blocking to be a stdio assumption rather than a protocol truth: an async
//! trait would colour every session holder and hub caller above it, to serve
//! one carrier that can perfectly well own its runtime.
//!
//! So the blocking calls below must run on a thread that is not a runtime
//! worker, which is the shape every current caller already has. Calling them
//! from inside an async context panics, which is tokio telling the truth
//! rather than a limitation of this crate.
//!
//! The case that reopens the decision is a browser reaching a *remote*
//! endpoint, where no thread may block. The answer there is an async sibling
//! trait, not a conversion.
//!
//! ## Why it reads [`CarrierOutput`] rather than [`CarrierResponse`]
//!
//! An admitted server loop writes bare responses and stdio writes an envelope,
//! but [`CarrierOutput`] is untagged and its two variants are structurally
//! disjoint, so one decode reads both. That is what lets a notice interleave
//! with responses on a stream that never framed them separately.

use std::collections::VecDeque;

use chirograph::{
    Carrier, CarrierError, CarrierNotice, CarrierOutput, CarrierRequest, CarrierRequestBody,
    CarrierResponse, CarrierResponseBody,
};
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, Lines,
    ReadHalf, WriteHalf,
};
use tokio::runtime::{Handle, Runtime};

/// The runtime a blocking carrier drives its stream on.
///
/// Borrowing is the ordinary case and the safer one: a stream produced by a
/// transport is driven by that transport's runtime, and handing the carrier
/// the same handle keeps one runtime responsible for one connection. An owned
/// runtime is for a caller that has no runtime of its own to lend.
pub enum CarrierRuntime {
    Owned(Runtime),
    Borrowed(Handle),
}

impl CarrierRuntime {
    /// A runtime of this carrier's own, for a caller that has none to lend.
    pub fn owned() -> std::io::Result<Self> {
        Ok(Self::Owned(Runtime::new()?))
    }

    /// The runtime the transport already runs on.
    pub fn borrowed(handle: Handle) -> Self {
        Self::Borrowed(handle)
    }

    fn block_on<F: std::future::Future>(&self, future: F) -> F::Output {
        match self {
            CarrierRuntime::Owned(runtime) => runtime.block_on(future),
            CarrierRuntime::Borrowed(handle) => handle.block_on(future),
        }
    }
}

/// A synchronous client for one endpoint at the far end of a stream.
///
/// Constructed over a stream that has already cleared whatever admission its
/// service requires, so this type never decides who may connect.
pub struct NetworkCarrier<S> {
    runtime: CarrierRuntime,
    writer: WriteHalf<S>,
    lines: Lines<BufReader<ReadHalf<S>>>,
    notices: VecDeque<CarrierNotice>,
    next_id: u64,
    closed: bool,
}

impl<S: AsyncRead + AsyncWrite + Unpin> NetworkCarrier<S> {
    /// Speak the protocol over an admitted stream.
    pub fn over(stream: S, runtime: CarrierRuntime) -> Self {
        let (reader, writer) = tokio::io::split(stream);
        Self {
            runtime,
            writer,
            lines: BufReader::new(reader).lines(),
            notices: VecDeque::new(),
            next_id: 1,
            closed: false,
        }
    }
}

async fn write_request<W>(writer: &mut W, request: &CarrierRequest) -> Result<(), CarrierError>
where
    W: AsyncWrite + Unpin,
{
    let mut line = serde_json::to_vec(request).map_err(|error| {
        CarrierError::Disconnected(format!("could not encode carrier request: {error}"))
    })?;
    line.push(b'\n');
    writer.write_all(&line).await.map_err(|error| {
        CarrierError::Disconnected(format!("could not send carrier request: {error}"))
    })?;
    writer.flush().await.map_err(|error| {
        CarrierError::Disconnected(format!("could not flush carrier request: {error}"))
    })
}

async fn read_output<R>(lines: &mut Lines<R>) -> Result<CarrierOutput, CarrierError>
where
    R: AsyncBufRead + Unpin,
{
    loop {
        let line = lines.next_line().await.map_err(|error| {
            CarrierError::Disconnected(format!("could not read carrier output: {error}"))
        })?;
        let Some(line) = line else {
            return Err(CarrierError::Disconnected(
                "endpoint closed without an output frame".into(),
            ));
        };
        if line.trim().is_empty() {
            continue;
        }
        return serde_json::from_str(&line).map_err(|error| {
            CarrierError::Disconnected(format!("invalid carrier output: {error}"))
        });
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> Carrier for NetworkCarrier<S> {
    fn request(&mut self, body: CarrierRequestBody) -> Result<CarrierResponseBody, CarrierError> {
        if self.closed {
            return Err(CarrierError::Disconnected(
                "projection carrier is closed".into(),
            ));
        }
        let id = self.next_id;
        self.next_id += 1;
        // Disjoint field borrows: the runtime is read while the stream halves
        // and the notice queue are written.
        let Self {
            runtime,
            writer,
            lines,
            notices,
            ..
        } = self;
        runtime.block_on(async move {
            write_request(writer, &CarrierRequest { id, body }).await?;
            loop {
                match read_output(lines).await? {
                    // A revision bell that arrived while this request was in
                    // flight. Queue it and keep waiting for the answer.
                    CarrierOutput::Notice(notice) => notices.push_back(notice),
                    CarrierOutput::Response(response) => {
                        return match_response(response, id);
                    }
                }
            }
        })
    }

    fn take_notice(&mut self) -> Option<CarrierNotice> {
        self.notices.pop_front()
    }

    /// Block until the endpoint rings.
    ///
    /// Unlike the in-process carrier, there is something real to wait for: a
    /// remote endpoint produces notices on its own schedule, and this is the
    /// call that lets a host wait on one rather than poll for it.
    fn wait_for_notice(&mut self) -> Result<CarrierNotice, CarrierError> {
        if let Some(notice) = self.take_notice() {
            return Ok(notice);
        }
        if self.closed {
            return Err(CarrierError::Disconnected(
                "projection carrier is closed".into(),
            ));
        }
        let Self { runtime, lines, .. } = self;
        runtime.block_on(async move {
            match read_output(lines).await? {
                CarrierOutput::Notice(notice) => Ok(notice),
                CarrierOutput::Response(response) => Err(CarrierError::Disconnected(format!(
                    "endpoint sent unexpected response {} while Graphshell waited for a notice",
                    response.id
                ))),
            }
        })
    }

    /// Close the stream.
    ///
    /// Saying goodbye at the protocol level is the client's own move, through
    /// `CarrierRequestBody::Close`, exactly as it is over stdio: a carrier
    /// that injected a verb of its own would answer for a client that may have
    /// already sent one. A second call is a no-op rather than an error.
    fn shutdown(&mut self) -> Result<(), CarrierError> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        let Self {
            runtime, writer, ..
        } = self;
        runtime
            .block_on(async move { writer.shutdown().await })
            .map_err(|error| {
                CarrierError::Disconnected(format!(
                    "projection carrier did not close cleanly: {error}"
                ))
            })
    }
}

/// A response is this request's answer only if it says so.
fn match_response(response: CarrierResponse, id: u64) -> Result<CarrierResponseBody, CarrierError> {
    if response.id != id {
        // The stream has lost its place, which no further request can recover.
        return Err(CarrierError::Disconnected(format!(
            "carrier response id {} did not match request {id}",
            response.id
        )));
    }
    // The one place an endpoint's own answer arrives: the session is fine.
    response
        .body
        .map_err(|failure| CarrierError::Refused(failure.message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chirograph::{ProjectionSession, Revision, SceneEpoch};
    use tokio::io::AsyncWriteExt;

    fn notice() -> CarrierNotice {
        CarrierNotice {
            session: ProjectionSession("fixture:scene".into()),
            epoch: SceneEpoch(3),
            revision: Revision(2),
        }
    }

    fn response(id: u64, body: CarrierResponseBody) -> CarrierOutput {
        CarrierOutput::Response(CarrierResponse { id, body: Ok(body) })
    }

    /// A carrier whose far end is a task the test wrote. These cases are about
    /// framing; the port's round-trip receipt is what proves the carrier
    /// against a real served loop.
    fn scripted(
        runtime: &Runtime,
        script: Vec<CarrierOutput>,
    ) -> NetworkCarrier<tokio::io::DuplexStream> {
        let (client, mut server) = tokio::io::duplex(64 * 1024);
        runtime.spawn(async move {
            for frame in script {
                let mut line = serde_json::to_vec(&frame).unwrap();
                line.push(b'\n');
                server.write_all(&line).await.unwrap();
                server.flush().await.unwrap();
            }
            // Held open so the carrier's reads block rather than hit EOF.
            std::future::pending::<()>().await;
        });
        NetworkCarrier::over(client, CarrierRuntime::borrowed(runtime.handle().clone()))
    }

    #[test]
    fn a_request_is_answered_by_the_response_that_carries_its_id() {
        let runtime = Runtime::new().unwrap();
        let mut carrier = scripted(&runtime, vec![response(1, CarrierResponseBody::Closed)]);
        assert!(matches!(
            carrier.request(CarrierRequestBody::Close).unwrap(),
            CarrierResponseBody::Closed
        ));
    }

    #[test]
    fn a_notice_in_flight_is_queued_rather_than_mistaken_for_an_answer() {
        // The case the untagged envelope exists for: a revision bell arrives
        // between a request and its response. It must not be read as the
        // answer, and it must not be dropped either.
        let runtime = Runtime::new().unwrap();
        let mut carrier = scripted(
            &runtime,
            vec![
                CarrierOutput::Notice(notice()),
                response(1, CarrierResponseBody::Closed),
            ],
        );
        assert!(matches!(
            carrier.request(CarrierRequestBody::Close).unwrap(),
            CarrierResponseBody::Closed
        ));
        assert_eq!(carrier.take_notice(), Some(notice()));
        assert_eq!(carrier.take_notice(), None, "a notice is delivered once");
    }

    #[test]
    fn waiting_for_a_notice_blocks_until_the_endpoint_rings() {
        // The behaviour the in-process carrier cannot have and a remote one
        // must: something real to wait for.
        let runtime = Runtime::new().unwrap();
        let (client, mut server) = tokio::io::duplex(64 * 1024);
        runtime.spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
            let mut line = serde_json::to_vec(&CarrierOutput::Notice(notice())).unwrap();
            line.push(b'\n');
            server.write_all(&line).await.unwrap();
            server.flush().await.unwrap();
            std::future::pending::<()>().await;
        });
        let mut carrier =
            NetworkCarrier::over(client, CarrierRuntime::borrowed(runtime.handle().clone()));
        assert_eq!(carrier.wait_for_notice().unwrap(), notice());
    }

    #[test]
    fn a_response_for_another_request_is_an_error_rather_than_a_wrong_answer() {
        let runtime = Runtime::new().unwrap();
        let mut carrier = scripted(&runtime, vec![response(99, CarrierResponseBody::Closed)]);
        let error = carrier.request(CarrierRequestBody::Close).unwrap_err();
        assert!(
            error.message().contains("did not match request 1"),
            "{error}"
        );
        assert!(
            error.is_disconnected(),
            "a stream that lost its place cannot be recovered by asking again"
        );
    }

    #[test]
    fn a_closed_carrier_refuses_rather_than_writing_to_a_dead_stream() {
        let runtime = Runtime::new().unwrap();
        let mut carrier = scripted(&runtime, Vec::new());
        carrier.shutdown().unwrap();
        carrier.shutdown().expect("a second shutdown is a no-op");
        let error = carrier.request(CarrierRequestBody::Close).unwrap_err();
        assert!(error.message().contains("closed"), "{error}");
        assert!(error.is_disconnected());
    }
}
