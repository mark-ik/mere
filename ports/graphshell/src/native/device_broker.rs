//! Private local broker between browser-launched native-messaging relays and
//! the resident Graphshell device host.
//!
//! The browser executable does not open Personae itself. It forwards opaque
//! native-messaging frames after identifying the browser launch; the resident
//! process owns the one vault, approval broker, and SSH agent endpoint.

#[cfg(not(windows))]
use std::path::{Path, PathBuf};
use std::sync::Arc;

use personae::IdentityStorage;
#[cfg(not(windows))]
use personae::bootstrap;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::RwLock;

use crate::browser_carrier::{
    AllowedExtensions, BrowserCarrierError, BrowserLauncher, read_native_message_async,
    write_native_message_async,
};
use crate::identity_endpoint::{SupplementalCard, TransferDecisions};
use crate::native::browser_host::{BrowserHostError, serve_identity_native_messages_with_cards};
use crate::native::identity_ui::NativeIdentityUi;
use crate::native::personae_host::PersonaeHost;
use graphshell_protocol::ContentHash;

pub const DEVICE_ENDPOINT_ENV: &str = "GRAPHSHELL_DEVICE_ENDPOINT";
const BROKER_HELLO_SCHEMA: &str = "mere.graphshell/device-broker-hello/v1";

/// What the resident host offers an admitted browser beyond the identity
/// projection.
///
/// One structure rather than a handle per kind: both parts are refreshed by
/// the same host and read at the same moment, and threading a second
/// `Arc<RwLock<_>>` through the broker for each new kind is how plumbing
/// multiplies.
#[derive(Clone, Default)]
pub struct DeviceSurface {
    /// Public, read-only cards composed beside the Personae surface.
    pub cards: Vec<SupplementalCard>,
    /// Blobs an accepted transfer released to this device's browser. Empty
    /// unless a transfer is waiting to be applied.
    pub released_blobs: Vec<(ContentHash, Vec<u8>)>,
    /// Where a person's accept lands. Shared rather than snapshotted: cloning
    /// the surface for a session must hand that session the same queue the
    /// resident host drains, not a copy of it.
    pub decisions: TransferDecisions,
    /// Reads a blob from the resident host's replicating store, for content a
    /// card refers to but no transfer staged — a receipt's captures, above
    /// all. `None` on a surface composed without sync, which then serves only
    /// what it holds.
    pub blob_reader: Option<Arc<BlobReader>>,
}

/// The store read behind [`DeviceSurface::blob_reader`].
pub type BlobReader = dyn Fn(&ContentHash) -> Option<Vec<u8>> + Send + Sync;

// Hand-written because a closure has no `Debug`. Reports whether a reader is
// present rather than trying to describe it: whether this surface can reach
// the store is the fact a log line wants.
impl std::fmt::Debug for DeviceSurface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceSurface")
            .field("cards", &self.cards.len())
            .field("released_blobs", &self.released_blobs.len())
            .field("decisions", &self.decisions)
            .field("blob_reader", &self.blob_reader.is_some())
            .finish()
    }
}

pub type DeviceSurfaceHandle = Arc<RwLock<DeviceSurface>>;

#[cfg(windows)]
const DEFAULT_WINDOWS_DEVICE_ENDPOINT: &str = r"\\.\pipe\graphshell-device-browser";

#[derive(Debug, thiserror::Error)]
pub enum DeviceBrokerError {
    #[error(transparent)]
    Carrier(#[from] BrowserCarrierError),
    #[error(transparent)]
    BrowserHost(#[from] BrowserHostError),
    #[error("device broker rejected its private hello")]
    WrongSchema,
    #[error("device broker stream ended before its private hello")]
    MissingHello,
    #[error("device broker transport failed: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct DeviceBrokerHello {
    schema: String,
    launcher: BrowserLauncher,
}

impl DeviceBrokerHello {
    fn new(launcher: BrowserLauncher) -> Self {
        Self {
            schema: BROKER_HELLO_SCHEMA.to_string(),
            launcher,
        }
    }

    fn accept(self) -> Result<BrowserLauncher, DeviceBrokerError> {
        if self.schema != BROKER_HELLO_SCHEMA {
            return Err(DeviceBrokerError::WrongSchema);
        }
        Ok(self.launcher)
    }
}

/// The per-user endpoint used by the resident device host and browser relay.
pub fn configured_device_endpoint() -> String {
    std::env::var(DEVICE_ENDPOINT_ENV)
        .ok()
        .filter(|endpoint| !endpoint.trim().is_empty())
        .unwrap_or_else(default_device_endpoint)
}

fn default_device_endpoint() -> String {
    #[cfg(windows)]
    {
        DEFAULT_WINDOWS_DEVICE_ENDPOINT.to_string()
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(bootstrap::default_vault_dir)
            .join("graphshell-device.sock")
            .display()
            .to_string()
    }
}

/// Relay one browser native-messaging connection to the resident host.
///
/// This process retains only browser-launch context. Every application
/// request still enters through the resident host's `SessionHello` admission.
pub async fn relay_browser_native_messages(
    endpoint: &str,
    launcher: BrowserLauncher,
) -> Result<(), DeviceBrokerError> {
    let mut stream = connect(endpoint).await?;
    write_native_message_async(&mut stream, &DeviceBrokerHello::new(launcher)).await?;
    let (mut device_reader, mut device_writer) = tokio::io::split(stream);
    let mut browser_reader = tokio::io::stdin();
    let mut browser_writer = tokio::io::stdout();

    let browser_to_device = async {
        tokio::io::copy(&mut browser_reader, &mut device_writer).await?;
        tokio::io::AsyncWriteExt::shutdown(&mut device_writer).await
    };
    let device_to_browser = async {
        tokio::io::copy(&mut device_reader, &mut browser_writer).await?;
        tokio::io::AsyncWriteExt::flush(&mut browser_writer).await
    };
    tokio::try_join!(browser_to_device, device_to_browser)?;
    Ok(())
}

/// Serve browser relays from the resident authority.
pub async fn serve_browser_broker<S, U>(
    endpoint: &str,
    personae: Arc<PersonaeHost<S>>,
    native_ui: Arc<U>,
    allowlist: AllowedExtensions,
    session_duration_ms: u64,
) -> Result<(), DeviceBrokerError>
where
    S: IdentityStorage + 'static,
    U: NativeIdentityUi + 'static,
{
    serve(
        endpoint,
        personae,
        native_ui,
        allowlist,
        session_duration_ms,
        None,
    )
    .await
}

/// Serve browser relays with public cards owned by another resident authority.
pub async fn serve_browser_broker_with_cards<S, U>(
    endpoint: &str,
    personae: Arc<PersonaeHost<S>>,
    native_ui: Arc<U>,
    allowlist: AllowedExtensions,
    session_duration_ms: u64,
    surface: DeviceSurfaceHandle,
) -> Result<(), DeviceBrokerError>
where
    S: IdentityStorage + 'static,
    U: NativeIdentityUi + 'static,
{
    serve(
        endpoint,
        personae,
        native_ui,
        allowlist,
        session_duration_ms,
        Some(surface),
    )
    .await
}

async fn serve_connection<S, U, R, W>(
    reader: &mut R,
    writer: &mut W,
    personae: Arc<PersonaeHost<S>>,
    native_ui: Arc<U>,
    allowlist: &AllowedExtensions,
    session_duration_ms: u64,
    surface: Option<DeviceSurfaceHandle>,
) -> Result<(), DeviceBrokerError>
where
    S: IdentityStorage + 'static,
    U: NativeIdentityUi,
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let hello = read_native_message_async::<_, DeviceBrokerHello>(reader)
        .await?
        .ok_or(DeviceBrokerError::MissingHello)?;
    let launcher = hello.accept()?;
    allowlist.admit(&launcher)?;
    // Read once, at session start. A transfer accepted while a browser is
    // already connected becomes pullable on its next session rather than
    // mid-stream, which is the same timing the cards have always had.
    let surface = match surface {
        Some(surface) => surface.read().await.clone(),
        None => DeviceSurface::default(),
    };
    let summary = serve_identity_native_messages_with_cards(
        personae.as_ref(),
        Arc::clone(&personae),
        native_ui.as_ref(),
        launcher,
        reader,
        writer,
        session_duration_ms,
        surface,
    )
    .await?;
    if let Some(summary) = summary {
        tracing::info!(
            answered = summary.answered,
            end = ?summary.end,
            "browser device session ended"
        );
    }
    Ok(())
}

#[cfg(windows)]
async fn connect(
    endpoint: &str,
) -> Result<tokio::net::windows::named_pipe::NamedPipeClient, std::io::Error> {
    tokio::net::windows::named_pipe::ClientOptions::new().open(endpoint)
}

#[cfg(windows)]
async fn serve<S, U>(
    endpoint: &str,
    personae: Arc<PersonaeHost<S>>,
    native_ui: Arc<U>,
    allowlist: AllowedExtensions,
    session_duration_ms: u64,
    surface: Option<DeviceSurfaceHandle>,
) -> Result<(), DeviceBrokerError>
where
    S: IdentityStorage + 'static,
    U: NativeIdentityUi + 'static,
{
    use tokio::net::windows::named_pipe::ServerOptions;

    let mut server = ServerOptions::new()
        .first_pipe_instance(true)
        .create(endpoint)?;
    tracing::info!(endpoint, "browser device broker listening");
    loop {
        server.connect().await?;
        let connection = server;
        server = ServerOptions::new().create(endpoint)?;
        let personae = Arc::clone(&personae);
        let native_ui = Arc::clone(&native_ui);
        let allowlist = allowlist.clone();
        let surface = surface.clone();
        tokio::spawn(async move {
            if let Err(error) = verify_same_user(&connection) {
                tracing::warn!(%error, "browser device broker rejected a different user");
                return;
            }
            let (mut reader, mut writer) = tokio::io::split(connection);
            if let Err(error) = serve_connection(
                &mut reader,
                &mut writer,
                personae,
                native_ui,
                &allowlist,
                session_duration_ms,
                surface,
            )
            .await
            {
                tracing::warn!(%error, "browser device session failed");
            }
        });
    }
}

#[cfg(windows)]
fn verify_same_user(
    connection: &tokio::net::windows::named_pipe::NamedPipeServer,
) -> Result<(), std::io::Error> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Security::{EqualSid, TOKEN_USER};
    use windows_sys::Win32::System::Pipes::GetNamedPipeClientProcessId;
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let mut client_pid = 0u32;
    if unsafe { GetNamedPipeClientProcessId(connection.as_raw_handle() as HANDLE, &mut client_pid) }
        == 0
    {
        return Err(std::io::Error::last_os_error());
    }
    let client_process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, client_pid) };
    if client_process.is_null() {
        return Err(std::io::Error::last_os_error());
    }
    let client_process = OwnedHandle(client_process);
    let current_user = token_user(unsafe { GetCurrentProcess() })?;
    let client_user = token_user(client_process.0)?;
    let current = unsafe { &*(current_user.as_ptr().cast::<TOKEN_USER>()) };
    let client = unsafe { &*(client_user.as_ptr().cast::<TOKEN_USER>()) };
    if unsafe { EqualSid(current.User.Sid, client.User.Sid) } == 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "browser broker client belongs to another Windows user",
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn token_user(
    process: windows_sys::Win32::Foundation::HANDLE,
) -> Result<Vec<usize>, std::io::Error> {
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TokenUser};
    use windows_sys::Win32::System::Threading::OpenProcessToken;

    let mut token: HANDLE = std::ptr::null_mut();
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let token = OwnedHandle(token);
    let mut byte_len = 0u32;
    unsafe {
        GetTokenInformation(token.0, TokenUser, std::ptr::null_mut(), 0, &mut byte_len);
    }
    if byte_len == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let word_len = (byte_len as usize).div_ceil(std::mem::size_of::<usize>());
    let mut buffer = vec![0usize; word_len];
    if unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            byte_len,
            &mut byte_len,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error());
    }
    Ok(buffer)
}

#[cfg(windows)]
struct OwnedHandle(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(self.0);
            }
        }
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;
    use tokio::net::windows::named_pipe::{ClientOptions, ServerOptions};

    #[tokio::test]
    async fn browser_broker_accepts_the_current_windows_user() {
        let endpoint = format!(
            r"\\.\pipe\graphshell-device-user-check-{}",
            uuid::Uuid::new_v4()
        );
        let server = ServerOptions::new()
            .first_pipe_instance(true)
            .create(&endpoint)
            .unwrap();
        let _client = ClientOptions::new().open(&endpoint).unwrap();
        server.connect().await.unwrap();
        verify_same_user(&server).unwrap();
    }
}

#[cfg(not(windows))]
async fn connect(endpoint: &str) -> Result<tokio::net::UnixStream, std::io::Error> {
    tokio::net::UnixStream::connect(endpoint).await
}

#[cfg(not(windows))]
async fn serve<S, U>(
    endpoint: &str,
    personae: Arc<PersonaeHost<S>>,
    native_ui: Arc<U>,
    allowlist: AllowedExtensions,
    session_duration_ms: u64,
    surface: Option<DeviceSurfaceHandle>,
) -> Result<(), DeviceBrokerError>
where
    S: IdentityStorage + 'static,
    U: NativeIdentityUi + 'static,
{
    prepare_unix_endpoint(Path::new(endpoint)).await?;
    let listener = tokio::net::UnixListener::bind(endpoint)?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(endpoint, std::fs::Permissions::from_mode(0o600))?;
    tracing::info!(endpoint, "browser device broker listening");
    loop {
        let (connection, _) = listener.accept().await?;
        let personae = Arc::clone(&personae);
        let native_ui = Arc::clone(&native_ui);
        let allowlist = allowlist.clone();
        let surface = surface.clone();
        tokio::spawn(async move {
            let (mut reader, mut writer) = tokio::io::split(connection);
            if let Err(error) = serve_connection(
                &mut reader,
                &mut writer,
                personae,
                native_ui,
                &allowlist,
                session_duration_ms,
                surface,
            )
            .await
            {
                tracing::warn!(%error, "browser device session failed");
            }
        });
    }
}

#[cfg(not(windows))]
async fn prepare_unix_endpoint(endpoint: &Path) -> Result<(), std::io::Error> {
    match tokio::net::UnixStream::connect(endpoint).await {
        Ok(_) => Err(std::io::Error::new(
            std::io::ErrorKind::AddrInUse,
            "another Graphshell device host owns the browser socket",
        )),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
            ) =>
        {
            match std::fs::remove_file(endpoint) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}
