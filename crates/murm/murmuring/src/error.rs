//! Error types for `murmuring`.

use thiserror::Error;

/// Errors raised by murmuring operations.
#[derive(Debug, Error)]
pub enum MurmuringError {
    /// A post's wire encoding is malformed.
    #[error("malformed post wire encoding")]
    MalformedPost,

    /// A post's signature did not verify.
    #[error("invalid post signature")]
    InvalidSignature,

    /// A required field is missing from a post (e.g. text body on `post/text`).
    #[error("missing required field: {0}")]
    MissingField(&'static str),

    /// An identity-layer error (key derivation, signing, etc.).
    #[error("identity error: {0}")]
    Identity(#[from] identity::IdentityError),

    /// A backend error (storage, transport-adapter, etc.).
    #[error("backend error: {0}")]
    Backend(String),
}
