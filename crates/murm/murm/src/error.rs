//! Error types for `murm`.

use thiserror::Error;

/// Errors raised by murm operations.
#[derive(Debug, Error)]
pub enum MurmError {
    /// An identity-layer error.
    #[error("identity error: {0}")]
    Identity(#[from] identity::IdentityError),

    /// A transport-layer error.
    #[error("transport error: {0}")]
    Transport(#[from] transport::TransportError),

    /// A post's wire representation is malformed.
    #[error("malformed post wire encoding")]
    MalformedPost,

    /// A required post field is missing or invalid.
    #[error("missing required field: {0}")]
    MissingField(&'static str),

    /// Shared conversation admission or storage failed.
    #[error(transparent)]
    ConversationStore(#[from] crate::ConversationStoreError),

    /// A key epoch was missing or installed out of order.
    #[error(transparent)]
    Keyring(#[from] crate::CabalKeyringError),

    /// The requested cabal is not known to this Murm instance.
    #[error("cabal not found")]
    CabalNotFound,

    /// Backend-specific error.
    #[error("backend error: {0}")]
    Backend(String),
}
