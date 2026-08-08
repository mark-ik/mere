//! Candidate Phase A codec for private Knot publishing.
//!
//! This serde/postcard envelope is deliberately a prototype. Its messages are
//! kept here, behind the Knot port, with a fixture corpus that Phase B can use
//! to compare a replacement grammar rather than treating this first shape as
//! a compatibility promise.

use serde::{Deserialize, Serialize};

use crate::{KnotPublishRead, KnotPublishedDocument, PublicationId};

/// Hard request ceiling, before an owner selects a lower runtime value.
pub const HARD_MAX_REQUEST_BYTES: u32 = 64 * 1024;
/// Hard response ceiling, before an owner selects a lower runtime value.
pub const HARD_MAX_RESPONSE_BYTES: u32 = 17 * 1024 * 1024;
/// Hard authored source-body ceiling for this candidate.
pub const HARD_MAX_DOCUMENT_BYTES: u32 = 16 * 1024 * 1024;
/// Hard catalog-size ceiling for this candidate.
pub const HARD_MAX_CATALOG_ENTRIES: u32 = 4096;

/// Owner-configurable candidate codec limits, clamped to hard ceilings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublishWireLimits {
    pub max_request_bytes: u32,
    pub max_response_bytes: u32,
    pub max_document_bytes: u32,
    pub max_catalog_entries: u32,
}

impl Default for PublishWireLimits {
    fn default() -> Self {
        Self {
            max_request_bytes: HARD_MAX_REQUEST_BYTES,
            max_response_bytes: HARD_MAX_RESPONSE_BYTES,
            max_document_bytes: HARD_MAX_DOCUMENT_BYTES,
            max_catalog_entries: HARD_MAX_CATALOG_ENTRIES,
        }
    }
}

impl PublishWireLimits {
    /// Apply the protocol's absolute allocation and disclosure caps.
    pub fn clamped(self) -> Self {
        Self {
            max_request_bytes: self.max_request_bytes.min(HARD_MAX_REQUEST_BYTES),
            max_response_bytes: self.max_response_bytes.min(HARD_MAX_RESPONSE_BYTES),
            max_document_bytes: self.max_document_bytes.min(HARD_MAX_DOCUMENT_BYTES),
            max_catalog_entries: self.max_catalog_entries.min(HARD_MAX_CATALOG_ENTRIES),
        }
    }
}

/// Candidate request vocabulary. These names are not a published protocol.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PublishRequest {
    List,
    GetCurrent {
        publication: PublicationId,
    },
    GetVersion {
        publication: PublicationId,
        operation: [u8; 32],
    },
}

/// Candidate response vocabulary. `NotAvailable` deliberately conflates every
/// unavailable source state so it cannot enumerate the holder's catalog.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PublishResponse {
    Catalog {
        publications: Vec<PublicationId>,
    },
    Document {
        publication: PublicationId,
        media_type: String,
        body: Vec<u8>,
        operation: [u8; 32],
        body_digest: [u8; 32],
    },
    NotAvailable,
}

impl From<KnotPublishRead> for PublishResponse {
    fn from(read: KnotPublishRead) -> Self {
        match read {
            KnotPublishRead::Document(document) => Self::Document {
                publication: document.publication,
                media_type: document.media_type,
                body: document.body,
                operation: document.operation,
                body_digest: document.body_digest,
            },
            KnotPublishRead::NotAvailable => Self::NotAvailable,
        }
    }
}

impl PublishResponse {
    /// Convert a decoded document response into the model's checked value.
    pub fn into_read(self) -> Result<KnotPublishRead, PublishWireError> {
        match self {
            Self::Document {
                publication,
                media_type,
                body,
                operation,
                body_digest,
            } => {
                if *blake3::hash(&body).as_bytes() != body_digest {
                    return Err(PublishWireError::InvalidDigest);
                }
                Ok(KnotPublishRead::Document(KnotPublishedDocument {
                    publication,
                    media_type,
                    body,
                    operation,
                    body_digest,
                }))
            }
            Self::NotAvailable => Ok(KnotPublishRead::NotAvailable),
            Self::Catalog { .. } => Err(PublishWireError::UnexpectedCatalog),
        }
    }
}

/// Candidate codec failure. The carrier maps this to a clean close rather than
/// an application-level source result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PublishWireError {
    #[error("candidate frame exceeds its configured bound")]
    TooLarge,
    #[error("candidate frame cannot be decoded")]
    Codec,
    #[error("candidate frame has trailing data")]
    TrailingData,
    #[error("candidate response exceeds its document or catalog ceiling")]
    ResponseLimit,
    #[error("candidate document body digest does not match")]
    InvalidDigest,
    #[error("expected a document or NotAvailable response, not a catalog")]
    UnexpectedCatalog,
}

/// Encode one candidate request after enforcing its bounded frame limit.
pub fn encode_request(
    request: &PublishRequest,
    limits: PublishWireLimits,
) -> Result<Vec<u8>, PublishWireError> {
    encode(request, limits.clamped().max_request_bytes)
}

/// Decode exactly one candidate request. Trailing bytes are refused so a peer
/// cannot smuggle a second request into a one-request Phase A stream.
pub fn decode_request(
    bytes: &[u8],
    limits: PublishWireLimits,
) -> Result<PublishRequest, PublishWireError> {
    decode(bytes, limits.clamped().max_request_bytes)
}

/// Encode a candidate response, refusing a body or catalog before allocating a
/// postcard output frame that would exceed the owner's bound.
pub fn encode_response(
    response: &PublishResponse,
    limits: PublishWireLimits,
) -> Result<Vec<u8>, PublishWireError> {
    let limits = limits.clamped();
    match response {
        PublishResponse::Catalog { publications }
            if publications.len() > limits.max_catalog_entries as usize =>
        {
            return Err(PublishWireError::ResponseLimit);
        }
        PublishResponse::Document { body, .. }
            if body.len() > limits.max_document_bytes as usize =>
        {
            return Err(PublishWireError::ResponseLimit);
        }
        _ => {}
    }
    encode(response, limits.max_response_bytes)
}

/// Decode exactly one candidate response and reject a frame whose advertised
/// digest does not match its authored bytes.
pub fn decode_response(
    bytes: &[u8],
    limits: PublishWireLimits,
) -> Result<PublishResponse, PublishWireError> {
    let response: PublishResponse = decode(bytes, limits.clamped().max_response_bytes)?;
    if let PublishResponse::Document {
        body, body_digest, ..
    } = &response
    {
        if body.len() > limits.clamped().max_document_bytes as usize
            || *blake3::hash(body).as_bytes() != *body_digest
        {
            return Err(PublishWireError::InvalidDigest);
        }
    }
    if let PublishResponse::Catalog { publications } = &response
        && publications.len() > limits.clamped().max_catalog_entries as usize
    {
        return Err(PublishWireError::ResponseLimit);
    }
    Ok(response)
}

fn encode<T: Serialize>(value: &T, max_bytes: u32) -> Result<Vec<u8>, PublishWireError> {
    let bytes = postcard::to_allocvec(value).map_err(|_| PublishWireError::Codec)?;
    if bytes.len() > max_bytes as usize {
        return Err(PublishWireError::TooLarge);
    }
    Ok(bytes)
}

fn decode<T: for<'de> Deserialize<'de>>(
    bytes: &[u8],
    max_bytes: u32,
) -> Result<T, PublishWireError> {
    if bytes.len() > max_bytes as usize {
        return Err(PublishWireError::TooLarge);
    }
    let (value, remaining) =
        postcard::take_from_bytes(bytes).map_err(|_| PublishWireError::Codec)?;
    if !remaining.is_empty() {
        return Err(PublishWireError::TrailingData);
    }
    Ok(value)
}

/// Outcome recorded for the first candidate corpus. Phase B uses these
/// semantic cases to compare a replacement codec without adopting postcard's
/// enum layout by accident.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CandidateFixtureOutcome {
    Response(PublishResponse),
    Refused(PublishWireError),
}

/// One deterministic candidate request/outcome pair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateFixture {
    pub request: Vec<u8>,
    pub outcome: CandidateFixtureOutcome,
}

/// The minimum Phase A candidate corpus: all semantic operations, absence,
/// malformed input, and a bounded response refusal.
pub fn candidate_fixture_corpus() -> Vec<CandidateFixture> {
    let limits = PublishWireLimits::default();
    let publication = PublicationId::from_uuid(uuid::Uuid::from_u128(1));
    let document = PublishResponse::Document {
        publication,
        media_type: "text/vnd.knot".into(),
        body: b"fixture".to_vec(),
        operation: [1; 32],
        body_digest: *blake3::hash(b"fixture").as_bytes(),
    };
    vec![
        CandidateFixture {
            request: encode_request(&PublishRequest::List, limits).expect("fixture request"),
            outcome: CandidateFixtureOutcome::Response(PublishResponse::Catalog {
                publications: vec![publication],
            }),
        },
        CandidateFixture {
            request: encode_request(&PublishRequest::GetCurrent { publication }, limits)
                .expect("fixture request"),
            outcome: CandidateFixtureOutcome::Response(document.clone()),
        },
        CandidateFixture {
            request: encode_request(
                &PublishRequest::GetVersion {
                    publication,
                    operation: [1; 32],
                },
                limits,
            )
            .expect("fixture request"),
            outcome: CandidateFixtureOutcome::Response(document),
        },
        CandidateFixture {
            request: encode_request(&PublishRequest::GetCurrent { publication }, limits)
                .expect("fixture request"),
            outcome: CandidateFixtureOutcome::Response(PublishResponse::NotAvailable),
        },
        CandidateFixture {
            request: vec![0xff],
            outcome: CandidateFixtureOutcome::Refused(PublishWireError::Codec),
        },
        CandidateFixture {
            request: vec![0; HARD_MAX_REQUEST_BYTES as usize + 1],
            outcome: CandidateFixtureOutcome::Refused(PublishWireError::TooLarge),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_corpus_covers_semantics_and_refusals() {
        let fixtures = candidate_fixture_corpus();
        assert_eq!(fixtures.len(), 6);
        assert_eq!(
            decode_request(&fixtures[0].request, PublishWireLimits::default()).unwrap(),
            PublishRequest::List
        );
        assert!(matches!(
            decode_request(&fixtures[1].request, PublishWireLimits::default()),
            Ok(PublishRequest::GetCurrent { .. })
        ));
        assert!(matches!(
            decode_request(&fixtures[2].request, PublishWireLimits::default()),
            Ok(PublishRequest::GetVersion { .. })
        ));
        assert!(matches!(
            &fixtures[3].outcome,
            CandidateFixtureOutcome::Response(PublishResponse::NotAvailable)
        ));
        assert_eq!(
            decode_request(&fixtures[4].request, PublishWireLimits::default()),
            Err(PublishWireError::Codec)
        );
        assert_eq!(
            decode_request(&fixtures[5].request, PublishWireLimits::default()),
            Err(PublishWireError::TooLarge)
        );
    }

    #[test]
    fn trailing_or_oversized_data_is_refused_before_use() {
        let limits = PublishWireLimits {
            max_request_bytes: 8,
            ..PublishWireLimits::default()
        };
        let mut encoded =
            encode_request(&PublishRequest::List, PublishWireLimits::default()).unwrap();
        encoded.push(0);
        assert_eq!(
            decode_request(&encoded, PublishWireLimits::default()),
            Err(PublishWireError::TrailingData)
        );
        assert_eq!(
            decode_request(&[0; 9], limits),
            Err(PublishWireError::TooLarge)
        );
    }

    #[test]
    fn response_limits_and_digests_fail_closed() {
        let publication = PublicationId::from_uuid(uuid::Uuid::from_u128(2));
        let response = PublishResponse::Document {
            publication,
            media_type: "text/vnd.knot".into(),
            body: vec![1; 5],
            operation: [2; 32],
            body_digest: [0; 32],
        };
        assert_eq!(
            encode_response(
                &response,
                PublishWireLimits {
                    max_document_bytes: 4,
                    ..PublishWireLimits::default()
                }
            ),
            Err(PublishWireError::ResponseLimit)
        );

        let bytes = postcard::to_allocvec(&response).unwrap();
        assert_eq!(
            decode_response(&bytes, PublishWireLimits::default()),
            Err(PublishWireError::InvalidDigest)
        );
    }
}
