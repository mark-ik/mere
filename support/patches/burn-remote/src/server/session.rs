use burn_backend::tensor::Device;
use burn_ir::BackendIr;
use burn_router::{CustomOpRegistry, TensorInterpreter};
use std::{
    collections::HashMap,
    sync::{Arc, Once},
};
use tokio::sync::{Mutex, mpsc, watch};

use crate::metrics::{MetricSide, logger_task};
use crate::server::local_comm::LocalCommService;
use crate::server::service::{SessionBinding, SessionService};
use crate::server::spawn::spawn_detached;
use crate::server::transfer::TensorTransfer;
use crate::server::worker::SessionHandler;
use crate::shared::{SessionId, Task};
use crate::telemetry::{TelemetryEvent, TelemetryProbe};

/// Capacity for the per-session response queue.
///
/// Sized larger than the typical in-flight read/sync count so that request processing
/// doesn't block on backpressure during a burst, but small enough that a stuck response
/// writer surfaces as a backpressure stall rather than memory growth.
const RESPONSE_CHANNEL_CAPACITY: usize = 64;

/// Coordinates per-session state.
///
/// Each [`Session`] owns a dedicated [`SessionHandler`] that holds the session's
/// [`TensorInterpreter`] with its own [`HandleContainer`](burn_ir::HandleContainer) — different
/// sessions never share tensor handles, so concurrent sessions can't race on each other's backend
/// state. Cross-session tensor transfers go through `external_comm` (cross-server) or `local_comm`
/// (same-host), each of which has its own rendezvous.
///
/// Tasks run on the handler's worker threads, not the submit handler: the latter only decodes the
/// incoming batch and forwards each [`Task`] to the session over a bounded channel. Inside the
/// session a dispatcher routes each task to a per-stream worker thread, so per-stream ordering is
/// preserved while independent streams — and other sessions — keep making progress even when one
/// stream is parked on a blocking op (a same-host transfer rendezvous or an all-reduce barrier).
pub struct SessionManager<B, T>
where
    B: BackendIr,
    T: TensorTransfer<B>,
{
    /// All devices this server hosts, indexed by the device index the client selects at
    /// session init. `devices[0]` is the default device (`DeviceIndex::Default`).
    devices: Vec<Device<B>>,
    pub(crate) transfer: Arc<T>,
    /// Rendezvous registry for same-host tensor transfers between this server's sessions.
    pub(crate) local_comm: Arc<LocalCommService<B>>,
    /// Custom-op handlers shared (read-only) with every session's interpreter.
    custom_ops: CustomOpRegistry<B>,
    sessions: Mutex<HashMap<SessionId, Session>>,
    probe: TelemetryProbe,
    /// Spawns the telemetry logger once, on the first session.
    logger: Once,
}

struct Session {
    /// Inbound channel to the session's dispatcher thread; cloned once per submit connection.
    _task_sender: Option<mpsc::Sender<Task>>,
    device_index: u32,
    authorization: Arc<[u8]>,
    close: watch::Sender<bool>,
    done: watch::Sender<bool>,
}

/// One session currently served by an Iroh remote protocol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServedSession {
    /// Client-chosen process-local session id.
    pub id: SessionId,
    /// Hosted compute device selected at admission.
    pub device_index: u32,
    /// Opaque application credential accepted for this session.
    pub credential: Arc<[u8]>,
}

impl<B, T> SessionManager<B, T>
where
    B: BackendIr,
    T: TensorTransfer<B>,
{
    pub fn new(devices: Vec<Device<B>>, transfer: Arc<T>) -> Self {
        assert!(
            !devices.is_empty(),
            "A remote server must host at least one device"
        );
        Self {
            devices,
            transfer,
            local_comm: Arc::new(LocalCommService::new()),
            custom_ops: CustomOpRegistry::default(),
            sessions: Mutex::new(HashMap::new()),
            probe: TelemetryProbe::disabled(),
            logger: Once::new(),
        }
    }

    /// Register custom-op handlers, shared read-only with every session's interpreter.
    pub fn with_custom_ops(mut self, custom_ops: CustomOpRegistry<B>) -> Self {
        self.custom_ops = custom_ops;
        self
    }

    /// Emit telemetry into `probe`.
    pub fn with_telemetry(mut self, probe: TelemetryProbe) -> Self {
        self.probe = probe;
        self
    }

    fn ensure_logger(&self) {
        self.logger.call_once(|| {
            if let Some(task) = logger_task(&self.probe, MetricSide::Server) {
                spawn_detached(task);
            }
        });
    }

    /// Resolve the device at `device_index`.
    ///
    /// The index is validated against the server's device count on the client init handshake, so
    /// an out-of-range index here is a protocol/configuration error (e.g. a client enumerating
    /// more devices than this server hosts). Fail loudly rather than silently collapsing onto
    /// device 0 — for a collective that would reduce a device against itself and silently corrupt
    /// the result instead of producing a clear failure.
    pub(crate) fn device(&self, device_index: u32) -> Device<B> {
        self.devices
            .get(device_index as usize)
            .cloned()
            .unwrap_or_else(|| {
                panic!(
                    "Requested device index {device_index} but server hosts only {} device(s)",
                    self.devices.len()
                )
            })
    }

    /// Snapshot active sessions in deterministic id order.
    pub async fn sessions(&self) -> Vec<ServedSession> {
        let sessions = self.sessions.lock().await;
        let mut served: Vec<_> = sessions
            .iter()
            .map(|(id, session)| ServedSession {
                id: *id,
                device_index: session.device_index,
                credential: session.authorization.clone(),
            })
            .collect();
        served.sort_by_key(|session| session.id);
        served
    }

    /// Ask one active pump to close, and wait until its worker and writer have
    /// reached the ordinary session teardown boundary.
    pub async fn close_session(&self, session_id: SessionId) -> bool {
        let (close, mut done) = {
            let sessions = self.sessions.lock().await;
            let Some(session) = sessions.get(&session_id) else {
                return false;
            };
            (session.close.clone(), session.done.subscribe())
        };

        close.send_replace(true);
        while !*done.borrow() {
            if done.changed().await.is_err() {
                break;
            }
        }
        true
    }
}

impl<B, T> SessionService for SessionManager<B, T>
where
    B: BackendIr,
    T: TensorTransfer<B>,
{
    async fn reserve_session(
        &self,
        session_id: SessionId,
        device_index: u32,
        authorization: Arc<[u8]>,
    ) -> Result<(), String> {
        self.ensure_logger();
        let mut sessions = self.sessions.lock().await;
        if sessions.contains_key(&session_id) {
            return Err(format!("Session {session_id} is already active"));
        }

        let (close, _) = watch::channel(false);
        let (done, _) = watch::channel(false);
        sessions.insert(
            session_id,
            Session {
                _task_sender: None,
                device_index,
                authorization,
                close,
                done,
            },
        );
        Ok(())
    }

    async fn bind_session(&self, session_id: SessionId) -> Result<SessionBinding, String> {
        let mut sessions = self.sessions.lock().await;
        let session = sessions
            .get_mut(&session_id)
            .ok_or_else(|| format!("Session {session_id} was not reserved"))?;
        if *session.close.borrow() {
            return Err(format!("Session {session_id} was closed during admission"));
        }
        if session._task_sender.is_some() {
            return Err(format!("Session {session_id} is already active"));
        }

        let device_index = session.device_index;
        let (response_sender, responses) = mpsc::channel(RESPONSE_CHANNEL_CAPACITY);
        let runner =
            TensorInterpreter::with_custom_ops(self.device(device_index), self.custom_ops.clone());
        let task_sender = SessionHandler::spawn(
            session_id,
            runner,
            response_sender,
            self.transfer.clone(),
            self.local_comm.clone(),
            self.probe.clone(),
        );
        session._task_sender = Some(task_sender.clone());
        let close_receiver = session.close.subscribe();
        self.probe.emit(|| TelemetryEvent::SessionOpened {
            session: session_id,
            device: device_index,
        });
        Ok(SessionBinding {
            task_sender,
            responses,
            close: close_receiver,
        })
    }

    /// The device settings for `device_index`, used by the handshake before any session-specific
    /// runner is needed.
    fn device_settings(&self, device_index: u32) -> burn_std::DeviceSettings {
        use burn_backend::backend::DeviceOps;
        self.device(device_index).defaults()
    }

    /// The total number of devices this server hosts. Sent to the client on the init handshake so
    /// it can enumerate every device behind the address (see [`RemoteDevice::enumerate`]).
    fn device_count(&self) -> u32 {
        self.devices.len() as u32
    }

    async fn finish_session(&self, session_id: SessionId) {
        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.remove(&session_id) {
            let was_bound = session._task_sender.is_some();
            session.done.send_replace(true);
            if was_bound {
                self.probe.emit(|| TelemetryEvent::SessionClosed {
                    session: session_id,
                });
            }
        }
    }
}
