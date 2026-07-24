//! Native local-process carrier for Graphshell endpoints.
//!
//! This crate supplies only newline-delimited JSON framing over a child
//! process's standard streams. Authentication, discovery policy, and remote
//! transport remain later carrier concerns.

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::ffi::OsStr;
    use std::fmt::Display;
    use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
    use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

    use graphshell_endpoint::{
        IntentSink, PresentationSource, ProjectionCatalog, ProjectionSource,
        ResumableProjectionSource,
    };
    use graphshell_protocol::{
        CarrierFailure, CarrierRequest, CarrierRequestBody, CarrierResponse, CarrierResponseBody,
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
            let request: CarrierRequest = match serde_json::from_str(&line) {
                Ok(request) => request,
                Err(error) => {
                    write_response(
                        &mut writer,
                        &CarrierResponse {
                            id: 0,
                            body: Err(CarrierFailure {
                                message: format!("invalid carrier request: {error}"),
                            }),
                        },
                    )?;
                    continue;
                }
            };
            let id = request.id;
            let body = match request.body {
                CarrierRequestBody::Discover => {
                    Ok(CarrierResponseBody::Descriptor(endpoint.describe()))
                }
                CarrierRequestBody::Snapshot(request) => endpoint
                    .snapshot(request)
                    .map(|snapshot| CarrierResponseBody::Snapshot(Box::new(snapshot)))
                    .map_err(|error| error.to_string()),
                CarrierRequestBody::Resource(request) => endpoint
                    .resource(request)
                    .map(CarrierResponseBody::Resource)
                    .map_err(|error| error.to_string()),
                CarrierRequestBody::Resume(request) => {
                    resume(endpoint, request).map(CarrierResponseBody::Resume)
                }
                CarrierRequestBody::Intent(intent) => endpoint
                    .invoke(intent)
                    .map(CarrierResponseBody::Intent)
                    .map_err(|error| error.to_string()),
            };
            write_response(
                &mut writer,
                &CarrierResponse {
                    id,
                    body: body.map_err(|message| CarrierFailure { message }),
                },
            )?;
        }
        Ok(())
    }

    fn write_response(writer: &mut impl Write, response: &CarrierResponse) -> io::Result<()> {
        serde_json::to_writer(&mut *writer, response).map_err(io::Error::other)?;
        writer.write_all(b"\n")?;
        writer.flush()
    }

    /// A synchronous client for one local endpoint child process.
    pub struct StdioCarrier {
        child: Child,
        input: Option<BufWriter<ChildStdin>>,
        output: BufReader<ChildStdout>,
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
            let mut line = String::new();
            self.output
                .read_line(&mut line)
                .map_err(|error| format!("could not read carrier response: {error}"))?;
            if line.is_empty() {
                return Err("endpoint closed without a response".into());
            }
            let response: CarrierResponse = serde_json::from_str(&line)
                .map_err(|error| format!("invalid carrier response: {error}"))?;
            if response.id != id {
                return Err(format!(
                    "carrier response id {} did not match request {id}",
                    response.id
                ));
            }
            response.body.map_err(|failure| failure.message)
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
        use std::io::Cursor;

        use graphshell_endpoint::{
            IntentSink, PresentationSource, ProjectionCatalog, ProjectionSource,
        };
        use graphshell_protocol::{
            CachePolicy, CarrierRequest, CarrierRequestBody, CarrierResponse, CarrierResponseBody,
            EndpointDescriptor, IntentInvocation, IntentResult, ProjectionOffer, ProjectionRequest,
            ProjectionSession, ProjectionSnapshot, ProtocolVersion, ResourceRequest,
            ResourceResponse,
        };
        use sceno::{Arrangement, Scene, Score, Spiral};
        use scenotime::{Revision, SceneEpoch, SceneSnapshot};

        use super::serve_basic;

        struct Fixture {
            session: ProjectionSession,
            resources: BTreeMap<graphshell_protocol::ContentHash, Vec<u8>>,
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

        #[test]
        fn basic_server_discovers_and_serves_a_snapshot() {
            let session = ProjectionSession("fixture:scene".into());
            let mut fixture = Fixture {
                session: session.clone(),
                resources: BTreeMap::new(),
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
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;
