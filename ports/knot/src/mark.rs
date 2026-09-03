// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A bounded, read-only Mark projection beside Knot's private publish lane.
//!
//! The two protocols deliberately remain separate. Native publishing is an
//! admitted Personae session over the `mere/knot-publish/v1` carrier, with
//! causal operation identifiers. Mark is a public QUIC/TLS protocol with
//! numeric, append-only versions and CommonMark bodies. This module therefore
//! snapshots an owner-selected Knot publication into a separate Mark history;
//! it never relabels raw Knot or Djot bytes as CommonMark and never exposes a
//! Personae delegation as a Mark token.

use std::{
    collections::BTreeMap,
    net::SocketAddr,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use muniment::Backend;
use quinn::crypto::rustls::QuicServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;

use crate::{
    DocumentFormat, KnotPublishCatalog, KnotPublishError, KnotPublishedDocument, KnotSyncStore,
    KnotVault, PublicationId,
};

/// The ALPN registered by the Mark Protocol working draft.
pub const MARK_ALPN: &[u8] = b"mark";
/// Mark's assigned UDP port. Callers may bind another port explicitly.
pub const MARK_DEFAULT_PORT: u16 = 6309;
/// Mark's maximum request-line size.
pub const MARK_MAX_REQUEST_BYTES: usize = 4096;
/// Mark's maximum YAML metadata block size.
pub const MARK_MAX_METADATA_BYTES: usize = 64 * 1024;
/// Mark's recommended document bound, made a default here rather than a hard
/// global policy for native Knot publishing.
pub const MARK_MAX_DOCUMENT_BYTES: usize = 1024 * 1024;

/// A configured bound for the Mark projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarkReadAdapterLimits {
    /// Maximum CommonMark bytes an owner may snapshot into one Mark version.
    pub max_document_bytes: usize,
}

impl Default for MarkReadAdapterLimits {
    fn default() -> Self {
        Self {
            max_document_bytes: MARK_MAX_DOCUMENT_BYTES,
        }
    }
}

impl MarkReadAdapterLimits {
    fn clamped(self) -> Self {
        Self {
            max_document_bytes: self.max_document_bytes.min(MARK_MAX_DOCUMENT_BYTES),
        }
    }
}

/// An RFC 3339 UTC timestamp held with a Mark snapshot.
///
/// Knot causal operations have no authored wall-clock time. The timestamp is
/// consequently the owner's projection time, not a claim about the causal
/// operation's time of authorship.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MarkTimestamp(String);

impl MarkTimestamp {
    /// Parse the UTC, second-precision form emitted by this adapter.
    pub fn parse(value: impl Into<String>) -> Result<Self, MarkAdapterError> {
        let value = value.into();
        if !is_utc_rfc3339_seconds(&value) {
            return Err(MarkAdapterError::InvalidTimestamp);
        }
        Ok(Self(value))
    }

    /// Capture the local projection time in Mark's required UTC form.
    pub fn now() -> Result<Self, MarkAdapterError> {
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| MarkAdapterError::Clock)?
            .as_secs();
        let days = (seconds / 86_400) as i64;
        let seconds_of_day = seconds % 86_400;
        let (year, month, day) = civil_from_days(days);
        let hour = seconds_of_day / 3_600;
        let minute = (seconds_of_day % 3_600) / 60;
        let second = seconds_of_day % 60;
        Self::parse(format!(
            "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"
        ))
    }

    /// The serialized response value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One-based numeric Mark version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MarkVersionId(u64);

impl MarkVersionId {
    /// Numeric representation used in Mark paths and metadata.
    pub fn get(self) -> u64 {
        self.0
    }
}

/// A snapshot outcome. Identical CommonMark bodies deliberately remain the
/// current Mark version, matching Mark's no-op publish rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkSnapshotOutcome {
    Created(MarkVersionId),
    Unchanged(MarkVersionId),
}

/// A Mark read access rule. The token form stores only a SHA-256 digest and is
/// a separately-issued adapter credential, never a serialized Personae grant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkReadAccess {
    Public,
    TokenHash([u8; 32]),
}

impl MarkReadAccess {
    /// Construct an adapter-local protected-read rule from a token the owner
    /// distributes out of band. The raw token is not retained.
    pub fn protected(token: impl AsRef<[u8]>) -> Self {
        Self::TokenHash(sha256(token.as_ref()))
    }

    fn allows(&self, candidate: Option<&str>) -> bool {
        match self {
            Self::Public => true,
            Self::TokenHash(expected) => candidate
                .is_some_and(|candidate| constant_time_eq(expected, &sha256(candidate.as_bytes()))),
        }
    }
}

/// One served Mark snapshot. The stored bytes are retained privately so the
/// adapter can compute the specification's ETag and `previous-hash` chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkVersion {
    pub id: MarkVersionId,
    pub modified: MarkTimestamp,
    pub source_operation: [u8; 32],
    pub source_media_type: String,
    pub body: Vec<u8>,
    stored: Vec<u8>,
    etag: [u8; 32],
    content_hash: [u8; 32],
}

impl MarkVersion {
    /// SHA-256 of the complete Mark store representation, formatted for ETag.
    pub fn etag(&self) -> String {
        hex(&self.etag)
    }

    /// SHA-256 of the served CommonMark body, formatted for `content-hash`.
    pub fn content_hash(&self) -> String {
        format!("sha256-{}", hex(&self.content_hash))
    }

    #[cfg(test)]
    fn stored(&self) -> &[u8] {
        &self.stored
    }
}

#[derive(Clone, Debug)]
struct MarkDocument {
    access: MarkReadAccess,
    publication: PublicationId,
    versions: Vec<MarkVersion>,
}

/// Explicit Mark export selections and their independent immutable snapshots.
///
/// `configure_export` and `snapshot_*` are owner actions. A native source
/// changing does not silently mutate a Mark document, and an unresolved causal
/// history cannot be invented as a numeric Mark history.
#[derive(Clone, Debug)]
pub struct MarkReadAdapter {
    limits: MarkReadAdapterLimits,
    documents: BTreeMap<String, MarkDocument>,
    current_content: BTreeMap<[u8; 32], String>,
}

impl MarkReadAdapter {
    pub fn new(limits: MarkReadAdapterLimits) -> Self {
        Self {
            limits: limits.clamped(),
            documents: BTreeMap::new(),
            current_content: BTreeMap::new(),
        }
    }

    /// Create or update an owner-selected Mark path. A path keeps its existing
    /// immutable history when its access rule changes, but cannot be rebound to
    /// another native publication.
    pub fn configure_export(
        &mut self,
        path: impl Into<String>,
        publication: PublicationId,
        access: MarkReadAccess,
    ) -> Result<(), MarkAdapterError> {
        let path = validate_document_path(path.into())?;
        if let Some(document) = self.documents.get_mut(&path) {
            if document.publication != publication {
                return Err(MarkAdapterError::RebindPath);
            }
            document.access = access;
            return Ok(());
        }
        self.documents.insert(
            path,
            MarkDocument {
                access,
                publication,
                versions: Vec::new(),
            },
        );
        Ok(())
    }

    /// Withdraw a Mark export and all future reads through this adapter.
    pub fn withdraw(&mut self, path: &str) -> bool {
        let removed = self.documents.remove(path).is_some();
        if removed {
            self.rebuild_content_index();
        }
        removed
    }

    /// Convert and append one explicit native source snapshot.
    pub fn snapshot(
        &mut self,
        path: &str,
        source: &KnotPublishedDocument,
        modified: MarkTimestamp,
    ) -> Result<MarkSnapshotOutcome, MarkAdapterError> {
        let document = self
            .documents
            .get_mut(path)
            .ok_or(MarkAdapterError::UnknownExport)?;
        if document.publication != source.publication {
            return Err(MarkAdapterError::PublicationMismatch);
        }
        if !source.body_digest_matches() {
            return Err(MarkAdapterError::InvalidSourceDigest);
        }
        let format = DocumentFormat::from_media_type(&source.media_type)
            .filter(|format| matches!(*format, DocumentFormat::Knot | DocumentFormat::Djot))
            .ok_or_else(|| MarkAdapterError::UnsupportedSource(source.media_type.clone()))?;
        let body = format
            .to_commonmark(
                &format!("knot-publication:{}", source.publication.as_uuid()),
                &source.body,
            )
            .map_err(MarkAdapterError::CommonMarkConversion)?;
        if body.len() > self.limits.max_document_bytes {
            return Err(MarkAdapterError::DocumentTooLarge);
        }
        if let Some(current) = document.versions.last()
            && current.body == body
        {
            return Ok(MarkSnapshotOutcome::Unchanged(current.id));
        }

        let next = document
            .versions
            .len()
            .checked_add(1)
            .and_then(|value| u64::try_from(value).ok())
            .map(MarkVersionId)
            .ok_or(MarkAdapterError::VersionOverflow)?;
        let previous_hash = document.versions.last().map(|version| version.etag);
        let stored = mark_stored_version(next, previous_hash, source, &body);
        let version = MarkVersion {
            id: next,
            modified,
            source_operation: source.operation,
            source_media_type: source.media_type.clone(),
            etag: sha256(&stored),
            content_hash: sha256(&body),
            body,
            stored,
        };
        document.versions.push(version);
        self.rebuild_content_index();
        Ok(MarkSnapshotOutcome::Created(next))
    }

    /// Materialize the holder's *currently eligible* native publication and
    /// append it only when the owner explicitly invokes this method.
    pub async fn snapshot_current<B>(
        &mut self,
        path: &str,
        catalog: &KnotPublishCatalog,
        store: &KnotSyncStore<B>,
        vault: &KnotVault,
        publication: PublicationId,
        modified: MarkTimestamp,
    ) -> Result<MarkSnapshotOutcome, MarkAdapterError>
    where
        B: Backend + Clone,
    {
        let source = catalog
            .current_for_mark_export(store, vault, publication)
            .await
            .map_err(MarkAdapterError::NativeSource)?
            .ok_or(MarkAdapterError::NativeNotAvailable)?;
        self.snapshot(path, &source, modified)
    }

    /// Handle one complete Mark request. Absence and a protected export denied
    /// by this adapter both use `not-found`, preserving the native lane's
    /// catalog non-disclosure discipline.
    pub fn respond(&self, request: &[u8]) -> MarkResponse {
        match decode_mark_request(request) {
            Ok(request) => self.respond_to(request),
            Err(_) => MarkResponse::bad_request(),
        }
    }

    fn respond_to(&self, request: MarkRequest) -> MarkResponse {
        match request {
            MarkRequest::Fetch {
                path,
                auth,
                if_none_match,
                if_modified_since,
            } => self.fetch(
                &path,
                auth.as_deref(),
                if_none_match.as_deref(),
                if_modified_since,
            ),
            MarkRequest::Versions { path, auth } => self.versions(&path, auth.as_deref()),
            MarkRequest::Other { .. } => MarkResponse::not_permitted(),
        }
    }

    fn fetch(
        &self,
        path: &str,
        auth: Option<&str>,
        if_none_match: Option<&str>,
        if_modified_since: Option<MarkTimestamp>,
    ) -> MarkResponse {
        if path == "/health" {
            return MarkResponse::ok(
                [(
                    "content-hash",
                    format!("sha256-{}", hex(&sha256(HEALTH_BODY))),
                )],
                HEALTH_BODY.to_vec(),
            );
        }
        let (path, requested_version) = match resolve_mark_path(path) {
            Ok(value) => value,
            Err(_) => return MarkResponse::not_found(),
        };
        let path = if let Some(content_hash) = parse_content_hash(&path) {
            let Some(path) = self.current_content.get(&content_hash) else {
                return MarkResponse::not_found();
            };
            path.as_str()
        } else {
            path.as_str()
        };
        let Some(document) = self.documents.get(path) else {
            return MarkResponse::not_found();
        };
        if !document.access.allows(auth) {
            return MarkResponse::not_found();
        }
        let version = match requested_version {
            Some(id) => document.versions.get(id.0.saturating_sub(1) as usize),
            None => document.versions.last(),
        };
        let Some(version) = version else {
            return MarkResponse::not_found();
        };
        if if_none_match.is_some_and(|candidate| candidate == version.etag())
            || if_modified_since.is_some_and(|since| version.modified <= since)
        {
            return MarkResponse::not_modified();
        }
        let mut metadata = vec![
            ("modified", version.modified.as_str().to_string()),
            ("etag", version.etag()),
            ("version", version.id.get().to_string()),
            ("content-hash", version.content_hash()),
        ];
        if requested_version.is_some() {
            metadata.push(("current-version", document.versions.len().to_string()));
        }
        MarkResponse::ok(metadata, version.body.clone())
    }

    fn versions(&self, path: &str, auth: Option<&str>) -> MarkResponse {
        let Ok((path, requested_version)) = resolve_mark_path(path) else {
            return MarkResponse::not_found();
        };
        if requested_version.is_some() {
            return MarkResponse::not_found();
        }
        let Some(document) = self.documents.get(&path) else {
            return MarkResponse::not_found();
        };
        if !document.access.allows(auth) || document.versions.is_empty() {
            return MarkResponse::not_found();
        }
        let mut body = format!("# Version History: {path}\n");
        for version in document.versions.iter().rev() {
            body.push_str(&format!(
                "- [v{}]({path}/v{}) - {}\n",
                version.id.get(),
                version.id.get(),
                version.modified.as_str()
            ));
        }
        let chain_valid = mark_chain_is_valid(&document.versions);
        let mut metadata = vec![
            ("total", document.versions.len().to_string()),
            ("current", document.versions.len().to_string()),
            ("chain-valid", chain_valid.to_string()),
        ];
        if !chain_valid {
            metadata.push(("chain-error", "stored version hash chain is broken".into()));
        }
        MarkResponse::ok(metadata, body.into_bytes())
    }

    fn rebuild_content_index(&mut self) {
        self.current_content.clear();
        for (path, document) in &self.documents {
            if let Some(version) = document.versions.last() {
                self.current_content
                    .entry(version.content_hash)
                    .or_insert_with(|| path.clone());
            }
        }
    }
}

/// A parsed bounded Mark request. The read adapter recognizes FETCH and
/// VERSIONS; the remaining specified verbs parse but are refused as writes or
/// unsupported discovery rather than leaking a native catalog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkRequest {
    Fetch {
        path: String,
        auth: Option<String>,
        if_none_match: Option<String>,
        if_modified_since: Option<MarkTimestamp>,
    },
    Versions {
        path: String,
        auth: Option<String>,
    },
    Other {
        verb: String,
        path: String,
    },
}

/// A textual Mark response. `to_wire` always emits the mandatory YAML
/// frontmatter and preserves a body exactly as supplied by the projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkResponse {
    status: &'static str,
    metadata: Vec<(&'static str, String)>,
    body: Vec<u8>,
}

impl MarkResponse {
    fn ok(metadata: impl IntoIterator<Item = (&'static str, String)>, body: Vec<u8>) -> Self {
        Self {
            status: "ok",
            metadata: metadata.into_iter().collect(),
            body,
        }
    }

    fn not_modified() -> Self {
        Self {
            status: "not-modified",
            metadata: Vec::new(),
            body: Vec::new(),
        }
    }

    fn not_found() -> Self {
        Self {
            status: "not-found",
            metadata: Vec::new(),
            body: b"# Not found\n\nThis document is not available.\n".to_vec(),
        }
    }

    fn bad_request() -> Self {
        Self {
            status: "bad-request",
            metadata: Vec::new(),
            body: b"# Bad request\n\nThe Mark request is malformed.\n".to_vec(),
        }
    }

    fn not_permitted() -> Self {
        Self {
            status: "not-permitted",
            metadata: Vec::new(),
            body: b"# Not permitted\n\nThis Mark adapter is read-only.\n".to_vec(),
        }
    }

    /// The Mark status string, useful to an embedding host before writing.
    pub fn status(&self) -> &str {
        self.status
    }

    /// Serialize the response's mandatory YAML frontmatter and body.
    pub fn to_wire(&self) -> Vec<u8> {
        let mut output = format!("---\nstatus: {}\n", self.status).into_bytes();
        for (key, value) in &self.metadata {
            output.extend_from_slice(format!("{key}: {value}\n").as_bytes());
        }
        output.extend_from_slice(b"---\n");
        output.extend_from_slice(&self.body);
        output
    }
}

/// Parse one complete bounded read-adapter request.
pub fn decode_mark_request(bytes: &[u8]) -> Result<MarkRequest, MarkAdapterError> {
    if bytes.len() > MARK_MAX_REQUEST_BYTES + MARK_MAX_METADATA_BYTES {
        return Err(MarkAdapterError::RequestTooLarge);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| MarkAdapterError::MalformedRequest)?;
    let Some(line_end) = text.find('\n') else {
        return Err(MarkAdapterError::MalformedRequest);
    };
    if line_end > MARK_MAX_REQUEST_BYTES || text[..line_end].contains('\r') {
        return Err(MarkAdapterError::MalformedRequest);
    }
    let line = &text[..line_end];
    let Some((verb, path)) = line.split_once(' ') else {
        return Err(MarkAdapterError::MalformedRequest);
    };
    if verb.is_empty() || path.is_empty() || path.contains(' ') {
        return Err(MarkAdapterError::MalformedRequest);
    }
    let path = validate_request_path(path)?;
    let (metadata, body) = parse_frontmatter(&text[line_end + 1..])?;
    if !body.is_empty() {
        return Err(MarkAdapterError::MalformedRequest);
    }
    let auth = metadata.get("auth").cloned();
    match verb {
        "FETCH" => Ok(MarkRequest::Fetch {
            path,
            auth,
            if_none_match: metadata.get("if-none-match").cloned(),
            if_modified_since: metadata
                .get("if-modified-since")
                .cloned()
                .map(MarkTimestamp::parse)
                .transpose()?,
        }),
        "VERSIONS" => Ok(MarkRequest::Versions { path, auth }),
        "LIST" | "PUBLISH" | "ARCHIVE" | "APPEND" | "LOOKUP" => Ok(MarkRequest::Other {
            verb: verb.into(),
            path,
        }),
        _ => Err(MarkAdapterError::MalformedRequest),
    }
}

/// A standard QUIC/TLS Mark listener. It is intentionally not layered over
/// the private p2panda carrier: external Mark clients connect with ALPN `mark`.
pub struct MarkQuicHost {
    endpoint: quinn::Endpoint,
    adapter: Arc<RwLock<MarkReadAdapter>>,
}

impl MarkQuicHost {
    /// Bind an independently configured direct QUIC listener.
    pub fn bind(
        address: SocketAddr,
        server_config: quinn::ServerConfig,
        adapter: Arc<RwLock<MarkReadAdapter>>,
    ) -> Result<Self, MarkServerError> {
        let endpoint = quinn::Endpoint::server(server_config, address)
            .map_err(|error| MarkServerError::Bind(error.to_string()))?;
        Ok(Self { endpoint, adapter })
    }

    /// The actual socket address, including an OS-selected port when requested.
    pub fn local_addr(&self) -> Result<SocketAddr, MarkServerError> {
        self.endpoint
            .local_addr()
            .map_err(|error| MarkServerError::Bind(error.to_string()))
    }

    /// Accept one Mark connection and serve its bidirectional request streams
    /// until the client closes it. Embedders call this repeatedly or place it
    /// in their own task supervision for subsequent connections.
    pub async fn serve_once(&self) -> Result<(), MarkServerError> {
        let incoming = self
            .endpoint
            .accept()
            .await
            .ok_or(MarkServerError::Closed)?;
        let connection = incoming
            .await
            .map_err(|error| MarkServerError::Connection(error.to_string()))?;
        while let Ok((mut send, mut receive)) = connection.accept_bi().await {
            let request = receive
                .read_to_end(MARK_MAX_REQUEST_BYTES + MARK_MAX_METADATA_BYTES)
                .await
                .map_err(|error| MarkServerError::Stream(error.to_string()))?;
            let response = self.adapter.read().await.respond(&request).to_wire();
            send.write_all(&response)
                .await
                .map_err(|error| MarkServerError::Stream(error.to_string()))?;
            send.finish()
                .map_err(|error| MarkServerError::Stream(error.to_string()))?;
        }
        Ok(())
    }
}

/// Build the direct QUIC/TLS configuration required by a Mark listener. The
/// caller owns certificate issuance and renewal; no self-signed certificate is
/// silently generated for a resident service.
pub fn mark_server_config(
    certificates: Vec<CertificateDer<'static>>,
    private_key: PrivateKeyDer<'static>,
) -> Result<quinn::ServerConfig, MarkServerError> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut tls = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certificates, private_key)
        .map_err(|error| MarkServerError::Config(error.to_string()))?;
    tls.alpn_protocols = vec![MARK_ALPN.to_vec()];
    let crypto = QuicServerConfig::try_from(tls)
        .map_err(|error| MarkServerError::Config(error.to_string()))?;
    Ok(quinn::ServerConfig::with_crypto(Arc::new(crypto)))
}

/// Source conversion, request parsing, or snapshot failure.
#[derive(Debug, thiserror::Error)]
pub enum MarkAdapterError {
    #[error("Mark export paths must be absolute .md paths without traversal")]
    InvalidPath,
    #[error("the Mark request is malformed")]
    MalformedRequest,
    #[error("the Mark request exceeds its fixed limit")]
    RequestTooLarge,
    #[error("the Mark timestamp must be RFC 3339 UTC at second precision")]
    InvalidTimestamp,
    #[error("the system clock is before the Unix epoch")]
    Clock,
    #[error("the Mark export does not exist")]
    UnknownExport,
    #[error("a Mark path cannot be rebound to a different native publication")]
    RebindPath,
    #[error("the native source does not match this export selection")]
    PublicationMismatch,
    #[error("the native source digest does not match its bytes")]
    InvalidSourceDigest,
    #[error("the native media type cannot be converted to CommonMark: {0}")]
    UnsupportedSource(String),
    #[error("the Knot-to-CommonMark projection failed: {0}")]
    CommonMarkConversion(String),
    #[error("the projected CommonMark document exceeds the configured Mark limit")]
    DocumentTooLarge,
    #[error("the Mark numeric version sequence is exhausted")]
    VersionOverflow,
    #[error("the selected native publication is not available for export")]
    NativeNotAvailable,
    #[error(transparent)]
    NativeSource(KnotPublishError),
}

/// Listener setup or stream-service failure.
#[derive(Debug, thiserror::Error)]
pub enum MarkServerError {
    #[error("could not configure Mark TLS: {0}")]
    Config(String),
    #[error("could not bind Mark QUIC listener: {0}")]
    Bind(String),
    #[error("Mark listener closed")]
    Closed,
    #[error("Mark QUIC connection failed: {0}")]
    Connection(String),
    #[error("Mark QUIC stream failed: {0}")]
    Stream(String),
}

const HEALTH_BODY: &[u8] = b"# Knot Mark read adapter\n\nReady.\n";

fn mark_stored_version(
    id: MarkVersionId,
    previous_hash: Option<[u8; 32]>,
    source: &KnotPublishedDocument,
    body: &[u8],
) -> Vec<u8> {
    let mut output = format!("---\nversion: {}\narchived: false\n", id.get());
    if let Some(previous_hash) = previous_hash {
        output.push_str(&format!("previous-hash: sha256-{}\n", hex(&previous_hash)));
    }
    output.push_str(&format!(
        "meta.mere-source-operation: {}\nmeta.mere-source-body-blake3: {}\nmeta.mere-source-media-type: {}\nmeta.mere-projection: canonical-commonmark\n",
        hex(&source.operation),
        hex(&source.body_digest),
        source.media_type,
    ));
    output.push_str("---\n");
    let mut stored = output.into_bytes();
    stored.extend_from_slice(body);
    stored
}

fn mark_chain_is_valid(versions: &[MarkVersion]) -> bool {
    versions.windows(2).all(|pair| {
        let previous = &pair[0];
        let current = &pair[1];
        current.etag == sha256(&current.stored)
            && std::str::from_utf8(&current.stored).is_ok_and(|stored| {
                stored.contains(&format!("previous-hash: sha256-{}\n", previous.etag()))
            })
    })
}

fn parse_frontmatter(tail: &str) -> Result<(BTreeMap<String, String>, &str), MarkAdapterError> {
    if !tail.starts_with("---\n") {
        return Ok((BTreeMap::new(), tail));
    }
    let remaining = &tail[4..];
    let Some(close) = remaining.find("\n---\n") else {
        return Err(MarkAdapterError::MalformedRequest);
    };
    let metadata_text = &remaining[..close];
    if metadata_text.len() > MARK_MAX_METADATA_BYTES {
        return Err(MarkAdapterError::RequestTooLarge);
    }
    let mut metadata = BTreeMap::new();
    for line in metadata_text.lines() {
        let Some((key, value)) = line.split_once(": ") else {
            return Err(MarkAdapterError::MalformedRequest);
        };
        if key.is_empty()
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            || value.contains(['\r', '\n'])
            || metadata.insert(key.into(), value.into()).is_some()
        {
            return Err(MarkAdapterError::MalformedRequest);
        }
    }
    Ok((metadata, &remaining[close + 5..]))
}

fn validate_request_path(path: &str) -> Result<String, MarkAdapterError> {
    if !path.starts_with('/')
        || path
            .bytes()
            .any(|byte| byte == 0 || byte < 32 || byte == 127)
        || path.contains(['?', '#'])
        || path.split('/').any(|segment| matches!(segment, "." | ".."))
    {
        return Err(MarkAdapterError::InvalidPath);
    }
    Ok(path.into())
}

fn validate_document_path(path: String) -> Result<String, MarkAdapterError> {
    let path = validate_request_path(&path)?;
    if !path.ends_with(".md") || path == "/.md" {
        return Err(MarkAdapterError::InvalidPath);
    }
    Ok(path)
}

fn resolve_mark_path(path: &str) -> Result<(String, Option<MarkVersionId>), MarkAdapterError> {
    let path = validate_request_path(path)?;
    let Some((base, tail)) = path.rsplit_once('/') else {
        return Ok((path, None));
    };
    let Some(number) = tail.strip_prefix('v') else {
        return Ok((path, None));
    };
    if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return Ok((path, None));
    }
    let version = number
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .map(MarkVersionId)
        .ok_or(MarkAdapterError::InvalidPath)?;
    Ok((base.into(), Some(version)))
}

fn parse_content_hash(path: &str) -> Option<[u8; 32]> {
    let value = path.strip_prefix("/sha256-")?;
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut output = [0u8; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(output)
}

fn is_utc_rfc3339_seconds(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'Z'
        || [0..4, 5..7, 8..10, 11..13, 14..16, 17..19]
            .into_iter()
            .flatten()
            .any(|index| !bytes[index].is_ascii_digit())
    {
        return false;
    }
    let number = |start: usize, end: usize| {
        bytes[start..end]
            .iter()
            .fold(0u8, |value, byte| value * 10 + (byte - b'0'))
    };
    (1..=12).contains(&number(5, 7))
        && (1..=31).contains(&number(8, 10))
        && number(11, 13) < 24
        && number(14, 16) < 60
        && number(17, 19) < 60
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    (year + i64::from(month <= 2), month as u32, day as u32)
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    output
}

fn constant_time_eq(left: &[u8; 32], right: &[u8; 32]) -> bool {
    left.iter()
        .zip(right)
        .fold(0u8, |difference, (left, right)| difference | (left ^ right))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use quinn::crypto::rustls::QuicClientConfig;
    use rustls::pki_types::{PrivatePkcs8KeyDer, ServerName, UnixTime};

    fn source(operation: u8, body: &[u8]) -> KnotPublishedDocument {
        KnotPublishedDocument {
            publication: PublicationId::from_uuid(uuid::Uuid::from_u128(5)),
            media_type: "text/vnd.knot".into(),
            body: body.to_vec(),
            operation: [operation; 32],
            body_digest: *blake3::hash(body).as_bytes(),
        }
    }

    fn configured_adapter() -> MarkReadAdapter {
        let publication = PublicationId::from_uuid(uuid::Uuid::from_u128(5));
        let mut adapter = MarkReadAdapter::new(MarkReadAdapterLimits::default());
        adapter
            .configure_export(
                "/shares/field-notes.md",
                publication,
                MarkReadAccess::protected("reader-token"),
            )
            .unwrap();
        adapter
    }

    fn request(path: &str, metadata: &str) -> Vec<u8> {
        format!("FETCH {path}\n---\n{metadata}---\n").into_bytes()
    }

    #[test]
    fn snapshots_are_commonmark_versions_with_a_sha256_chain() {
        let mut adapter = configured_adapter();
        let timestamp = MarkTimestamp::parse("2026-08-08T02:00:00Z").unwrap();
        assert_eq!(
            adapter
                .snapshot(
                    "/shares/field-notes.md",
                    &source(1, b"# Field notes\n\nFirst pass.\n"),
                    timestamp.clone(),
                )
                .unwrap(),
            MarkSnapshotOutcome::Created(MarkVersionId(1))
        );
        assert_eq!(
            adapter
                .snapshot(
                    "/shares/field-notes.md",
                    &source(2, b"# Field notes\n\nSecond pass.\n"),
                    MarkTimestamp::parse("2026-08-08T02:01:00Z").unwrap(),
                )
                .unwrap(),
            MarkSnapshotOutcome::Created(MarkVersionId(2))
        );
        let document = adapter.documents.get("/shares/field-notes.md").unwrap();
        let first = &document.versions[0];
        let second = &document.versions[1];
        assert!(
            std::str::from_utf8(&second.body)
                .unwrap()
                .contains("Second pass.")
        );
        assert!(
            std::str::from_utf8(second.stored())
                .unwrap()
                .contains(&format!("previous-hash: sha256-{}", first.etag()))
        );

        let response = adapter.respond(&request("/shares/field-notes.md", "auth: reader-token\n"));
        let wire = String::from_utf8(response.to_wire()).unwrap();
        assert!(wire.starts_with("---\nstatus: ok\n"));
        assert!(wire.contains("version: 2\n"));
        assert!(wire.contains("content-hash: sha256-"));
        assert!(wire.contains("Second pass."));
    }

    #[test]
    fn conditional_and_denied_reads_do_not_reveal_an_export() {
        let mut adapter = configured_adapter();
        adapter
            .snapshot(
                "/shares/field-notes.md",
                &source(1, b"# Field notes\n\nOne.\n"),
                MarkTimestamp::parse("2026-08-08T02:00:00Z").unwrap(),
            )
            .unwrap();
        let current = adapter.documents["/shares/field-notes.md"]
            .versions
            .last()
            .unwrap();
        let conditional_request = request(
            "/shares/field-notes.md",
            &format!("auth: reader-token\nif-none-match: {}\n", current.etag()),
        );
        assert_eq!(
            adapter.respond(&conditional_request).status(),
            "not-modified"
        );
        assert_eq!(
            adapter
                .respond(&request("/shares/field-notes.md", "auth: wrong\n"))
                .status(),
            "not-found"
        );
        assert_eq!(
            adapter
                .respond(&request("/missing.md", "auth: wrong\n"))
                .status(),
            "not-found"
        );
    }

    #[test]
    fn request_parser_rejects_bodies_and_path_traversal() {
        assert!(decode_mark_request(b"FETCH /safe.md\n").is_ok());
        assert!(decode_mark_request(b"FETCH /../safe.md\n").is_err());
        assert!(decode_mark_request(b"FETCH /safe.md\nbody").is_err());
        assert!(decode_mark_request(b"FETCH /safe.md\r\n").is_err());
    }

    #[derive(Debug)]
    struct NoVerify;

    impl rustls::client::danger::ServerCertVerifier for NoVerify {
        fn verify_server_cert(
            &self,
            _: &CertificateDer,
            _: &[CertificateDer],
            _: &ServerName,
            _: &[u8],
            _: UnixTime,
        ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            _: &[u8],
            _: &CertificateDer,
            _: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
        }

        fn verify_tls13_signature(
            &self,
            _: &[u8],
            _: &CertificateDer,
            _: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
        }

        fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
            rustls::crypto::ring::default_provider()
                .signature_verification_algorithms
                .supported_schemes()
        }
    }

    fn insecure_mark_client() -> quinn::ClientConfig {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut tls = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerify))
            .with_no_client_auth();
        tls.alpn_protocols = vec![MARK_ALPN.to_vec()];
        quinn::ClientConfig::new(Arc::new(QuicClientConfig::try_from(tls).unwrap()))
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn standard_quic_mark_alpn_serves_a_snapshot() {
        let certificate = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let certificate_der = CertificateDer::from(certificate.cert.der().to_vec());
        let private_key = PrivatePkcs8KeyDer::from(certificate.signing_key.serialize_der());
        let config = mark_server_config(vec![certificate_der], private_key.into()).unwrap();
        let mut adapter = configured_adapter();
        adapter
            .snapshot(
                "/shares/field-notes.md",
                &source(1, b"# Field notes\n\nOver QUIC.\n"),
                MarkTimestamp::parse("2026-08-08T02:00:00Z").unwrap(),
            )
            .unwrap();
        let expected_body = String::from_utf8(
            adapter.documents["/shares/field-notes.md"].versions[0]
                .body
                .clone(),
        )
        .unwrap();
        let host = Arc::new(
            MarkQuicHost::bind(
                "127.0.0.1:0".parse().unwrap(),
                config,
                Arc::new(RwLock::new(adapter)),
            )
            .unwrap(),
        );
        let address = host.local_addr().unwrap();
        let serving_host = Arc::clone(&host);
        let serving = tokio::spawn(async move { serving_host.serve_once().await });

        let mut client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        client.set_default_client_config(insecure_mark_client());
        let connection = client.connect(address, "localhost").unwrap().await.unwrap();
        let (mut send, mut receive) = connection.open_bi().await.unwrap();
        send.write_all(&request("/shares/field-notes.md", "auth: reader-token\n"))
            .await
            .unwrap();
        send.finish().unwrap();
        let response = receive
            .read_to_end(MARK_MAX_DOCUMENT_BYTES + 1024)
            .await
            .unwrap();
        drop(send);
        drop(receive);
        drop(connection);
        serving.await.unwrap().unwrap();
        drop(host);
        let response = String::from_utf8(response).unwrap();
        assert!(response.starts_with("---\nstatus: ok\n"));
        assert!(response.ends_with(&expected_body));
        assert!(response.contains("Over QUIC."));
    }
}
