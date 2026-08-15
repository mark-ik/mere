//! The owner-only local transport both resident doors are served over.
//!
//! Graphshell serves two kinds of local caller — a browser relay and a
//! first-party application — on two endpoints. What reaches either one is the
//! same fact: a process running as this user. That check is transport work,
//! not door work, so it lives here once. A second door written against its own
//! copy of the listener would be a second copy of a security check, and copies
//! drift.
//!
//! **What the transport proves.** On Unix the socket is created in the runtime
//! directory at `0600`, so the filesystem does the proving. Windows named
//! pipes carry no such default, so the peer's process token is compared
//! against this process's before a byte is read. Neither says *which*
//! application is speaking; that is the door's business, above this layer.

use std::future::Future;

use tokio::io::{AsyncRead, AsyncWrite};

/// A local connection, whatever the platform calls it.
///
/// Boxed rather than threaded through as a type parameter: the two platforms
/// have unrelated stream types, and a session is a human-paced conversation,
/// so one virtual call per read costs nothing worth naming.
pub trait LocalStream: AsyncRead + AsyncWrite + Unpin + Send + 'static {}

impl<T> LocalStream for T where T: AsyncRead + AsyncWrite + Unpin + Send + 'static {}

/// Open a connection to a local endpoint.
pub async fn connect_local(endpoint: &str) -> Result<Box<dyn LocalStream>, std::io::Error> {
    #[cfg(windows)]
    {
        Ok(Box::new(
            tokio::net::windows::named_pipe::ClientOptions::new().open(endpoint)?,
        ))
    }
    #[cfg(not(windows))]
    {
        Ok(Box::new(tokio::net::UnixStream::connect(endpoint).await?))
    }
}

/// Accept owner-only connections forever, handing each to `handle`.
///
/// `what` names the door in the log line, so an operator reading a startup log
/// can tell which endpoint came up.
pub async fn serve_local<H, F>(
    endpoint: &str,
    what: &'static str,
    handle: H,
) -> Result<(), std::io::Error>
where
    H: Fn(Box<dyn LocalStream>) -> F + Clone + Send + 'static,
    F: Future<Output = ()> + Send + 'static,
{
    serve_platform(endpoint, what, handle).await
}

#[cfg(windows)]
async fn serve_platform<H, F>(
    endpoint: &str,
    what: &'static str,
    handle: H,
) -> Result<(), std::io::Error>
where
    H: Fn(Box<dyn LocalStream>) -> F + Clone + Send + 'static,
    F: Future<Output = ()> + Send + 'static,
{
    use tokio::net::windows::named_pipe::ServerOptions;

    let mut server = ServerOptions::new()
        .first_pipe_instance(true)
        .create(endpoint)?;
    tracing::info!(endpoint, door = what, "resident endpoint listening");
    loop {
        server.connect().await?;
        let connection = server;
        server = ServerOptions::new().create(endpoint)?;
        let handle = handle.clone();
        tokio::spawn(async move {
            if let Err(error) = verify_same_user(&connection) {
                tracing::warn!(%error, door = what, "rejected a client of another user");
                return;
            }
            handle(Box::new(connection)).await;
        });
    }
}

#[cfg(not(windows))]
async fn serve_platform<H, F>(
    endpoint: &str,
    what: &'static str,
    handle: H,
) -> Result<(), std::io::Error>
where
    H: Fn(Box<dyn LocalStream>) -> F + Clone + Send + 'static,
    F: Future<Output = ()> + Send + 'static,
{
    use std::os::unix::fs::PermissionsExt;

    prepare_unix_endpoint(std::path::Path::new(endpoint)).await?;
    let listener = tokio::net::UnixListener::bind(endpoint)?;
    std::fs::set_permissions(endpoint, std::fs::Permissions::from_mode(0o600))?;
    tracing::info!(endpoint, door = what, "resident endpoint listening");
    loop {
        let (connection, _) = listener.accept().await?;
        let handle = handle.clone();
        tokio::spawn(async move {
            handle(Box::new(connection)).await;
        });
    }
}

/// Clear a stale socket, refusing to displace a live one.
///
/// A second resident host must fail loudly rather than unlink the socket the
/// first one is serving: the first would keep running, answering nobody.
#[cfg(not(windows))]
async fn prepare_unix_endpoint(endpoint: &std::path::Path) -> Result<(), std::io::Error> {
    match tokio::net::UnixStream::connect(endpoint).await {
        Ok(_) => Err(std::io::Error::new(
            std::io::ErrorKind::AddrInUse,
            "another Graphshell resident host owns this endpoint",
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

/// Compare the peer's user against this process's.
///
/// Unix gets this from the socket's mode. Windows named pipes are reachable by
/// other users on the machine unless the server checks, so the server checks.
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
            "the client belongs to another Windows user",
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
    async fn a_local_endpoint_accepts_the_current_windows_user() {
        let endpoint = format!(r"\\.\pipe\graphshell-user-check-{}", uuid::Uuid::new_v4());
        let server = ServerOptions::new()
            .first_pipe_instance(true)
            .create(&endpoint)
            .unwrap();
        let _client = ClientOptions::new().open(&endpoint).unwrap();
        server.connect().await.unwrap();
        verify_same_user(&server).unwrap();
    }
}
