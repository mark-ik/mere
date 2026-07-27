//! Native local-process carrier for Graphshell endpoints.
//!
//! This crate supplies only newline-delimited JSON framing over a child
//! process's standard streams. Authentication, discovery policy, and remote
//! transport remain later carrier concerns.

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::collections::VecDeque;
    use std::ffi::OsStr;
    use std::fmt::Display;
    use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
    use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
    use std::sync::mpsc;
    use std::time::Duration;

    use graphshell_endpoint::{
        IntentSink, PresentationSource, ProjectionCatalog, ProjectionNoticeSource,
        ProjectionSource, ResumableProjectionSource,
    };
    use graphshell_protocol::{
        CarrierFailure, CarrierNotice, CarrierOutput, CarrierRequest, CarrierRequestBody,
        CarrierResponse, CarrierResponseBody,
    };

    /// Serve discovery, snapshots, resources, and intents until the input
    /// stream closes. Resume requests are reported as unsupported.
    pub fn serve_basic<E, R, W>(endpoint: &mut E, reader: R, writer: W) -> io::Result<()>
    where
        E: ProjectionCatalog + ProjectionSource + PresentationSource + IntentSink,
        <E as ProjectionSource>::Error: Display,
        <E as PresentationSource>::Error: Display,
        <E as IntentSink>::Error: Display,
        R: Read,
        W: Write,
    {
        serve_with(endpoint, reader, writer, |_, _| {
            Err("this endpoint does not support resume".into())
        })
    }

    /// Serve the complete local protocol, including resume.
    pub fn serve_resumable<E, R, W>(endpoint: &mut E, reader: R, writer: W) -> io::Result<()>
    where
        E: ProjectionCatalog
            + ProjectionSource
            + PresentationSource
            + IntentSink
            + ResumableProjectionSource,
        <E as ProjectionSource>::Error: Display,
        <E as PresentationSource>::Error: Display,
        <E as IntentSink>::Error: Display,
        <E as ResumableProjectionSource>::Error: Display,
        R: Read,
        W: Write,
    {
        serve_with(endpoint, reader, writer, |endpoint, request| {
            endpoint.resume(request).map_err(|error| error.to_string())
        })
    }

    /// Serve resume plus endpoint-initiated revision notices.
    ///
    /// Input framing is read on a helper thread so a quiet client cannot
    /// prevent the endpoint from polling its native watcher. Only this
    /// function writes stdout, preserving line-atomic response/notice frames.
    pub fn serve_resumable_notifying<E, R, W>(
        endpoint: &mut E,
        reader: R,
        writer: W,
        poll_interval: Duration,
    ) -> io::Result<()>
    where
        E: ProjectionCatalog
            + ProjectionSource
            + PresentationSource
            + IntentSink
            + ResumableProjectionSource
            + ProjectionNoticeSource,
        <E as ProjectionSource>::Error: Display,
        <E as PresentationSource>::Error: Display,
        <E as IntentSink>::Error: Display,
        <E as ResumableProjectionSource>::Error: Display,
        <E as ProjectionNoticeSource>::Error: Display,
        R: Read + Send + 'static,
        W: Write,
    {
        let (send, receive) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            for line in BufReader::new(reader).lines() {
                if send.send(line).is_err() {
                    return;
                }
            }
        });
        let mut writer = BufWriter::new(writer);
        let mut resume =
            |endpoint: &mut E, request| endpoint.resume(request).map_err(|error| error.to_string());
        loop {
            match receive.recv_timeout(poll_interval) {
                Ok(Ok(line)) => {
                    if line.trim().is_empty() {
                        continue;
                    }
                    let request = match parse_request(&line) {
                        Ok(request) => request,
                        Err(response) => {
                            write_response(&mut writer, &response)?;
                            continue;
                        }
                    };
                    let (response, close) = dispatch(endpoint, request, &mut resume);
                    write_response(&mut writer, &response)?;
                    if close {
                        return Ok(());
                    }
                    if let Some(notice) = endpoint
                        .poll_notice()
                        .map_err(|error| io::Error::other(error.to_string()))?
                    {
                        write_notice(&mut writer, &notice)?;
                    }
                }
                Ok(Err(error)) => return Err(error),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if let Some(notice) = endpoint
                        .poll_notice()
                        .map_err(|error| io::Error::other(error.to_string()))?
                    {
                        write_notice(&mut writer, &notice)?;
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
            }
        }
    }

    fn serve_with<E, R, W, F>(
        endpoint: &mut E,
        reader: R,
        writer: W,
        mut resume: F,
    ) -> io::Result<()>
    where
        E: ProjectionCatalog + ProjectionSource + PresentationSource + IntentSink,
        <E as ProjectionSource>::Error: Display,
        <E as PresentationSource>::Error: Display,
        <E as IntentSink>::Error: Display,
        R: Read,
        W: Write,
        F: FnMut(
            &mut E,
            graphshell_protocol::ResumeRequest,
        ) -> Result<graphshell_protocol::ResumeReply, String>,
    {
        let reader = BufReader::new(reader);
        let mut writer = BufWriter::new(writer);
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let request = match parse_request(&line) {
                Ok(request) => request,
                Err(response) => {
                    write_response(&mut writer, &response)?;
                    continue;
                }
            };
            let (response, close) = dispatch(endpoint, request, &mut resume);
            write_response(&mut writer, &response)?;
            if close {
                return Ok(());
            }
        }
        Ok(())
    }

    fn parse_request(line: &str) -> Result<CarrierRequest, CarrierResponse> {
        serde_json::from_str(line).map_err(|error| CarrierResponse {
            id: 0,
            body: Err(CarrierFailure {
                message: format!("invalid carrier request: {error}"),
            }),
        })
    }

    /// Stdio's dispatch: the carrier-agnostic verbs come from
    /// `graphshell_endpoint::dispatch_common`, and only the session-plane
    /// answers are stdio's own.
    ///
    /// Those two refusals are the whole difference between this carrier and an
    /// authenticated one, which is why they live here rather than in the
    /// shared dispatcher: an inherited pipe performs no key exchange, and no
    /// session survives the process.
    fn dispatch<E, F>(
        endpoint: &mut E,
        request: CarrierRequest,
        resume: &mut F,
    ) -> (CarrierResponse, bool)
    where
        E: ProjectionCatalog + ProjectionSource + PresentationSource + IntentSink,
        <E as ProjectionSource>::Error: Display,
        <E as PresentationSource>::Error: Display,
        <E as IntentSink>::Error: Display,
        F: FnMut(
            &mut E,
            graphshell_protocol::ResumeRequest,
        ) -> Result<graphshell_protocol::ResumeReply, String>,
    {
        let session_plane = match graphshell_endpoint::dispatch_common(endpoint, request, resume) {
            Ok(response) => return (response, false),
            Err(session_plane) => session_plane,
        };
        let id = session_plane.id;
        let mut close = false;
        let body = match session_plane.verb {
            // Inherited pipes perform no key exchange, so stdio cannot make
            // an authenticated session claim.
            graphshell_endpoint::SessionPlaneVerb::Open(_) => Err(
                "the stdio carrier does not authenticate; `open` requires a carrier that proves its peer"
                    .to_string(),
            ),
            // Closing this process-scoped session is honest and terminal.
            graphshell_endpoint::SessionPlaneVerb::Close => {
                close = true;
                Ok(CarrierResponseBody::Closed)
            }
            // No stdio session survives the endpoint process.
            graphshell_endpoint::SessionPlaneVerb::Suspend => Err(
                "the stdio carrier has no session that outlives its process; `suspend` requires a carrier that can reconnect"
                    .to_string(),
            ),
        };
        (
            CarrierResponse {
                id,
                body: body.map_err(|message| CarrierFailure { message }),
            },
            close,
        )
    }

    fn write_response(writer: &mut impl Write, response: &CarrierResponse) -> io::Result<()> {
        serde_json::to_writer(&mut *writer, &CarrierOutput::Response(response.clone()))
            .map_err(io::Error::other)?;
        writer.write_all(b"\n")?;
        writer.flush()
    }

    fn write_notice(writer: &mut impl Write, notice: &CarrierNotice) -> io::Result<()> {
        serde_json::to_writer(&mut *writer, &CarrierOutput::Notice(notice.clone()))
            .map_err(io::Error::other)?;
        writer.write_all(b"\n")?;
        writer.flush()
    }

    /// A synchronous client for one local endpoint child process.
    pub struct StdioCarrier {
        child: Child,
        input: Option<BufWriter<ChildStdin>>,
        output: BufReader<ChildStdout>,
        notices: VecDeque<CarrierNotice>,
        next_id: u64,
    }

    impl StdioCarrier {
        pub fn spawn(
            program: impl AsRef<OsStr>,
            args: impl IntoIterator<Item = impl AsRef<OsStr>>,
        ) -> io::Result<Self> {
            let mut child = Command::new(program)
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()?;
            let input = child
                .stdin
                .take()
                .ok_or_else(|| io::Error::other("endpoint stdin was not piped"))?;
            let output = child
                .stdout
                .take()
                .ok_or_else(|| io::Error::other("endpoint stdout was not piped"))?;
            Ok(Self {
                child,
                input: Some(BufWriter::new(input)),
                output: BufReader::new(output),
                notices: VecDeque::new(),
                next_id: 1,
            })
        }

        pub fn request(&mut self, body: CarrierRequestBody) -> Result<CarrierResponseBody, String> {
            let id = self.next_id;
            self.next_id += 1;
            let request = CarrierRequest { id, body };
            let input = self
                .input
                .as_mut()
                .ok_or_else(|| "endpoint input is closed".to_string())?;
            serde_json::to_writer(&mut *input, &request)
                .map_err(|error| format!("could not encode carrier request: {error}"))?;
            input
                .write_all(b"\n")
                .and_then(|()| input.flush())
                .map_err(|error| format!("could not send carrier request: {error}"))?;
            loop {
                match self.read_output()? {
                    CarrierOutput::Notice(notice) => self.notices.push_back(notice),
                    CarrierOutput::Response(response) => {
                        if response.id != id {
                            return Err(format!(
                                "carrier response id {} did not match request {id}",
                                response.id
                            ));
                        }
                        return response.body.map_err(|failure| failure.message);
                    }
                }
            }
        }

        /// Return a notice already encountered while waiting for a response.
        pub fn take_notice(&mut self) -> Option<CarrierNotice> {
            self.notices.pop_front()
        }

        /// Wait for the endpoint's next revision bell.
        pub fn wait_for_notice(&mut self) -> Result<CarrierNotice, String> {
            if let Some(notice) = self.take_notice() {
                return Ok(notice);
            }
            match self.read_output()? {
                CarrierOutput::Notice(notice) => Ok(notice),
                CarrierOutput::Response(response) => Err(format!(
                    "endpoint sent unexpected response {} while Graphshell waited for a notice",
                    response.id
                )),
            }
        }

        fn read_output(&mut self) -> Result<CarrierOutput, String> {
            let mut line = String::new();
            self.output
                .read_line(&mut line)
                .map_err(|error| format!("could not read carrier output: {error}"))?;
            if line.is_empty() {
                return Err("endpoint closed without an output frame".into());
            }
            serde_json::from_str(&line).map_err(|error| format!("invalid carrier output: {error}"))
        }

        pub fn shutdown(mut self) -> io::Result<()> {
            self.input.take();
            let status = self.child.wait()?;
            if status.success() {
                Ok(())
            } else {
                Err(io::Error::other(format!(
                    "endpoint exited with status {status}"
                )))
            }
        }
    }

    impl Drop for StdioCarrier {
        fn drop(&mut self) {
            self.input.take();
            let _ = self.child.wait();
        }
    }

    #[cfg(test)]
    mod tests {
        use std::collections::BTreeMap;
        use std::io::{Cursor, Read};
        use std::time::Duration;

        use graphshell_endpoint::{
            IntentSink, PresentationSource, ProjectionCatalog, ProjectionNoticeSource,
            ProjectionSource, ResumableProjectionSource,
        };
        use graphshell_protocol::{
            CachePolicy, CapabilityProfile, CarrierNotice, CarrierOutput, CarrierRequest,
            CarrierRequestBody, CarrierResponse, CarrierResponseBody, EndpointDescriptor,
            IntentInvocation, IntentResult, ProjectionAck, ProjectionOffer, ProjectionRequest,
            ProjectionSession, ProjectionSnapshot, ProtocolVersion, ResourceRequest,
            ResourceResponse, ResumeReply, ResumeRequest, SessionOpen,
        };
        use sceno::{Arrangement, Scene, Score, Spiral};
        use scenotime::{Revision, SceneEpoch, SceneSnapshot};

        use super::{serve_basic, serve_resumable_notifying};

        struct Fixture {
            session: ProjectionSession,
            resources: BTreeMap<graphshell_protocol::ContentHash, Vec<u8>>,
            notice: Option<CarrierNotice>,
        }

        impl ProjectionCatalog for Fixture {
            fn describe(&self) -> EndpointDescriptor {
                EndpointDescriptor {
                    label: "Fixture".into(),
                    projections: vec![ProjectionOffer {
                        label: "Scene".into(),
                        request: ProjectionRequest {
                            version: ProtocolVersion::V1,
                            session: self.session.clone(),
                            score: Score::new(Arrangement::Spiral(Spiral::default())),
                        },
                    }],
                }
            }
        }

        impl ProjectionSource for Fixture {
            type Error = String;

            fn snapshot(
                &mut self,
                request: ProjectionRequest,
            ) -> Result<ProjectionSnapshot, Self::Error> {
                Ok(ProjectionSnapshot {
                    version: ProtocolVersion::V1,
                    session: request.session,
                    scene: SceneSnapshot::from_dense(SceneEpoch(1), Revision(1), Scene::new())
                        .unwrap(),
                    presentation: Default::default(),
                    cache_policy: CachePolicy::default(),
                })
            }
        }

        impl PresentationSource for Fixture {
            type Error = String;

            fn resource(
                &mut self,
                request: ResourceRequest,
            ) -> Result<ResourceResponse, Self::Error> {
                Ok(ResourceResponse {
                    session: request.session,
                    resource: request.resource,
                    bytes: self
                        .resources
                        .get(&request.resource)
                        .cloned()
                        .unwrap_or_default(),
                })
            }
        }

        impl IntentSink for Fixture {
            type Error = String;

            fn invoke(&mut self, _: IntentInvocation) -> Result<IntentResult, Self::Error> {
                Ok(IntentResult::Accepted)
            }
        }

        impl ResumableProjectionSource for Fixture {
            type Error = String;

            fn resume(&mut self, request: ResumeRequest) -> Result<ResumeReply, Self::Error> {
                Ok(ResumeReply::Current(ProjectionAck {
                    session: request.session,
                    epoch: request.epoch,
                    revision: request.revision,
                }))
            }
        }

        impl ProjectionNoticeSource for Fixture {
            type Error = String;

            fn poll_notice(&mut self) -> Result<Option<CarrierNotice>, Self::Error> {
                Ok(self.notice.take())
            }
        }

        /// Drive `serve_basic` over a list of requests and return the replies.
        fn exchange(requests: &[CarrierRequest]) -> Vec<CarrierResponse> {
            let mut fixture = Fixture {
                session: ProjectionSession("fixture:scene".into()),
                resources: BTreeMap::new(),
                notice: None,
            };
            let input: String = requests
                .iter()
                .map(|request| {
                    format!(
                        "{}
",
                        serde_json::to_string(request).unwrap()
                    )
                })
                .collect();
            let mut output = Vec::new();
            serve_basic(&mut fixture, Cursor::new(input), &mut output).unwrap();
            String::from_utf8(output)
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect()
        }

        #[test]
        fn stdio_refuses_to_open_because_it_cannot_authenticate() {
            // The far end is a subprocess of the same user over inherited
            // pipes: there is no key exchange anywhere in this profile. A
            // carrier that cannot verify a principal must not answer `Opened`,
            // or every client above it inherits a false belief.
            let responses = exchange(&[CarrierRequest {
                id: 1,
                body: CarrierRequestBody::Open(Box::new(SessionOpen {
                    version: ProtocolVersion::V1,
                    capabilities: CapabilityProfile::default(),
                })),
            }]);
            match &responses[0].body {
                Err(failure) => assert!(
                    failure.message.contains("does not authenticate"),
                    "the refusal says why: {}",
                    failure.message
                ),
                Ok(body) => panic!("stdio must not answer an open: {body:?}"),
            }
        }

        #[test]
        fn stdio_refuses_to_suspend_because_the_session_is_the_process() {
            // Suspend promises a session that outlives its transport. Over
            // stdio the session IS the process and its pipes.
            let responses = exchange(&[CarrierRequest {
                id: 1,
                body: CarrierRequestBody::Suspend,
            }]);
            match &responses[0].body {
                Err(failure) => assert!(
                    failure.message.contains("outlives its process"),
                    "the refusal says why: {}",
                    failure.message
                ),
                Ok(body) => panic!("stdio must not answer a suspend: {body:?}"),
            }
        }

        #[test]
        fn close_is_answered_and_ends_the_session() {
            // Close is the one verb stdio can honour, and honouring it means
            // the loop stops: a request after it is never served.
            let responses = exchange(&[
                CarrierRequest {
                    id: 1,
                    body: CarrierRequestBody::Close,
                },
                CarrierRequest {
                    id: 2,
                    body: CarrierRequestBody::Discover,
                },
            ]);
            assert_eq!(responses.len(), 1, "nothing is served after a close");
            assert_eq!(responses[0].id, 1);
            assert!(matches!(responses[0].body, Ok(CarrierResponseBody::Closed)));
        }

        #[test]
        fn basic_server_discovers_and_serves_a_snapshot() {
            let session = ProjectionSession("fixture:scene".into());
            let mut fixture = Fixture {
                session: session.clone(),
                resources: BTreeMap::new(),
                notice: None,
            };
            let discover = CarrierRequest {
                id: 1,
                body: CarrierRequestBody::Discover,
            };
            let snapshot = CarrierRequest {
                id: 2,
                body: CarrierRequestBody::Snapshot(ProjectionRequest {
                    version: ProtocolVersion::V1,
                    session,
                    score: Score::new(Arrangement::Spiral(Spiral::default())),
                }),
            };
            let input = format!(
                "{}\n{}\n",
                serde_json::to_string(&discover).unwrap(),
                serde_json::to_string(&snapshot).unwrap()
            );
            let mut output = Vec::new();
            serve_basic(&mut fixture, Cursor::new(input), &mut output).unwrap();
            let responses: Vec<CarrierResponse> = String::from_utf8(output)
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            assert!(matches!(
                responses[0].body,
                Ok(CarrierResponseBody::Descriptor(_))
            ));
            assert!(matches!(
                responses[1].body,
                Ok(CarrierResponseBody::Snapshot(_))
            ));
        }

        #[test]
        fn notifying_server_emits_a_payload_free_frame_while_input_is_quiet() {
            struct DelayedEof;

            impl Read for DelayedEof {
                fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                    std::thread::sleep(Duration::from_millis(30));
                    Ok(0)
                }
            }

            let notice = CarrierNotice {
                session: ProjectionSession("fixture:scene".into()),
                epoch: SceneEpoch(1),
                revision: Revision(2),
            };
            let mut fixture = Fixture {
                session: notice.session.clone(),
                resources: BTreeMap::new(),
                notice: Some(notice.clone()),
            };
            let mut output = Vec::new();
            serve_resumable_notifying(
                &mut fixture,
                DelayedEof,
                &mut output,
                Duration::from_millis(5),
            )
            .unwrap();

            let frames: Vec<CarrierOutput> = String::from_utf8(output)
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            assert_eq!(frames, vec![CarrierOutput::Notice(notice)]);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;
