//! Error types for `murm`.

use thiserror::Error;

/// Errors raised by murm operations.
#[derive(Debug, Error)]
pub enum MurmError {
    /// An identity-layer error.
    #[error("identity error: {0}")]
    Identity(#[from] mere_identity::IdentityError),

    /// A transport-layer error.
    #[error("transport error: {0}")]
    Transport(#[from] mere_transport::TransportError),

    /// A protocol-core error from `murmuring`.
    #[error("protocol error: {0}")]
    Protocol(#[from] murmuring::MurmuringError),

    /// The requested cabal is not known to this Murm instance.
    #[error("cabal not found")]
    CabalNotFound,

    /// Backend-specific error.
    #[error("backend error: {0}")]
    Backend(String),
}
