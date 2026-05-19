//! Error types for `mere-transport`.

use thiserror::Error;

/// Errors raised by transport operations.
#[derive(Debug, Error)]
pub enum TransportError {
    /// The requested ALPN is not registered with this transport.
    #[error("ALPN not registered")]
    AlpnNotRegistered,

    /// The peer refused the connection (e.g. unknown ALPN, peer not paired).
    #[error("connection refused by peer")]
    ConnectionRefused,

    /// The transport has been closed and no longer accepts new operations.
    #[error("transport closed")]
    Closed,

    /// An underlying I/O error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// A backend-specific error (iroh-side, in-memory adapter, etc.).
    #[error("backend error: {0}")]
    Backend(String),
}
