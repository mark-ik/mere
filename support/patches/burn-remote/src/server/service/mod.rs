//! The session-layer service trait, plus the init-handshake helper shared by the session pump.
//!
//! The pump ([`drive_session`](crate::server::pump::drive_session)) is written against
//! [`SessionService`] rather than a concrete type, so the submit-loop teardown logic stays testable
//! against a fake service with no backend and no live socket.

mod base;

pub(crate) use base::parse_init_handshake;

use std::{future::Future, sync::Arc};

use burn_std::DeviceSettings;
use tokio::sync::{mpsc, watch};

use crate::shared::{SessionId, Task, TaskResponse};

/// The pump-owned halves of one newly admitted session.
pub(crate) struct SessionBinding {
    pub task_sender: mpsc::Sender<Task>,
    pub responses: mpsc::Receiver<TaskResponse>,
    pub close: watch::Receiver<bool>,
}

/// What the session pump needs from the session layer: bind a session to its worker channel, claim
/// its response receiver, read the device metadata for the handshake, and tear a session down.
///
/// Async methods return `impl Future + Send` so a session future built on them stays `Send` and can
/// be spawned by the server. The production implementation is
/// [`SessionManager`](super::session::SessionManager).
pub(crate) trait SessionService: Send + Sync + 'static {
    /// Reserve one session before application authorization begins.
    ///
    /// A duplicate id is rejected. Reserving first makes server-requested
    /// closure race-free: the host can see and close a session throughout the
    /// interval between authorization and worker binding.
    fn reserve_session(
        &self,
        session_id: SessionId,
        device_index: u32,
        authorization: Arc<[u8]>,
    ) -> impl Future<Output = Result<(), String>> + Send;

    /// Bind one reserved and admitted session and spawn its worker.
    fn bind_session(
        &self,
        session_id: SessionId,
    ) -> impl Future<Output = Result<SessionBinding, String>> + Send;

    /// The default settings of the device at `device_index`, returned on the handshake so the
    /// client can resolve op dtypes without an extra round-trip.
    fn device_settings(&self, device_index: u32) -> DeviceSettings;

    /// The number of devices this server hosts, returned on the handshake so the client can
    /// enumerate every device behind the address.
    fn device_count(&self) -> u32;

    /// Remove a finished session and acknowledge closure after its worker has released backend
    /// state.
    fn finish_session(
        &self,
        session_id: SessionId,
    ) -> impl Future<Output = Result<(), String>> + Send;
}
