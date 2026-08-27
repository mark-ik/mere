// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The restricted job namespace — the only door a resource has to data.
//!
//! A [`JobSpec`] names blobs; it does not grant them. The host builds a
//! [`JobNamespaceView`] *after* checking the job and its own authority, and the
//! view exposes exactly two capabilities: read an input the job named, and
//! write the single output slot the job was granted. It hands out no
//! `muniment::Backend`, no filesystem handle, no network client, and no ambient
//! blob resolver, so an adapter cannot widen its own reach.
//!
//! Reads verify the content address inside the view, so unverified bytes never
//! reach an adapter at all.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

use muniment::{Backend, BlobStore, MemoryBackend};
use proofs::BlobRef;

use crate::spec::JobSpec;

/// A boxed future, spelled out so the blob seams stay object-safe without
/// pulling in an async-trait macro.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Where granted input bytes come from. Host-owned: the mesh never resolves a
/// blob on an adapter's behalf.
pub trait BlobSource: Send + Sync {
    fn fetch<'a>(
        &'a self,
        blob: &'a BlobRef,
    ) -> BoxFuture<'a, Result<Option<Vec<u8>>, NamespaceError>>;
}

/// Where a granted output is committed.
pub trait BlobSink: Send + Sync {
    fn commit<'a>(&'a self, bytes: &'a [u8]) -> BoxFuture<'a, Result<BlobRef, NamespaceError>>;
}

/// What was committed for the granted slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputCommit {
    pub name: String,
    pub blob: BlobRef,
}

/// Why a namespace access was refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NamespaceError {
    #[error("job does not grant input {0:?}")]
    UngrantedInput(String),
    #[error("job does not grant output {0:?}")]
    UngrantedOutput(String),
    #[error("granted input {0:?} is not held by this device")]
    MissingBlob(String),
    #[error("granted input {0:?} does not match its content address")]
    DigestMismatch(String),
    #[error("output is {bytes} bytes against a {max_bytes}-byte grant")]
    OverGrant { bytes: u64, max_bytes: u64 },
    #[error("blob store: {0}")]
    Backend(String),
}

/// A content-addressed blob space over any muniment backend: the ordinary way
/// a host realizes [`BlobSource`] + [`BlobSink`].
pub struct MunimentBlobSpace<B> {
    store: BlobStore<B>,
}

/// The in-process space tests and rehearsals use.
pub type MemoryBlobSpace = MunimentBlobSpace<MemoryBackend>;

impl MemoryBlobSpace {
    pub fn in_memory() -> Self {
        Self::new(MemoryBackend::new())
    }
}

impl<B: Backend> MunimentBlobSpace<B> {
    pub fn new(backend: B) -> Self {
        Self {
            store: BlobStore::new(backend),
        }
    }

    /// Store bytes and return their content address — the host's own put path,
    /// used to stage a job's inputs before posting it.
    pub async fn put(&self, bytes: &[u8]) -> Result<BlobRef, NamespaceError> {
        self.store
            .put(bytes)
            .await
            .map_err(|err| NamespaceError::Backend(err.to_string()))?;
        Ok(BlobRef::blake3(bytes))
    }

    /// Whether this device holds the referenced bytes.
    pub async fn has(&self, blob: &BlobRef) -> Result<bool, NamespaceError> {
        let Ok(bytes) = blob.digest.as_32() else {
            return Ok(false);
        };
        self.store
            .has(&muniment::Hash::from_bytes(bytes))
            .await
            .map_err(|err| NamespaceError::Backend(err.to_string()))
    }

    /// Ambient read of the device's own space. The *host* has this; a resource
    /// never does — it sees only [`JobNamespaceView`].
    pub async fn get(&self, blob: &BlobRef) -> Result<Option<Vec<u8>>, NamespaceError> {
        let Ok(bytes) = blob.digest.as_32() else {
            return Ok(None);
        };
        self.store
            .get(&muniment::Hash::from_bytes(bytes))
            .await
            .map_err(|err| NamespaceError::Backend(err.to_string()))
    }
}

impl<B: Backend + Send + Sync> BlobSource for MunimentBlobSpace<B> {
    fn fetch<'a>(
        &'a self,
        blob: &'a BlobRef,
    ) -> BoxFuture<'a, Result<Option<Vec<u8>>, NamespaceError>> {
        Box::pin(self.get(blob))
    }
}

impl<B: Backend + Send + Sync> BlobSink for MunimentBlobSpace<B> {
    fn commit<'a>(&'a self, bytes: &'a [u8]) -> BoxFuture<'a, Result<BlobRef, NamespaceError>> {
        Box::pin(async move {
            self.store
                .put(bytes)
                .await
                .map_err(|err| NamespaceError::Backend(err.to_string()))?;
            Ok(BlobRef::blake3(bytes))
        })
    }
}

/// One job's whole world.
pub struct JobNamespaceView<'a> {
    inputs: BTreeMap<&'a str, &'a BlobRef>,
    output_name: &'a str,
    max_output_bytes: u64,
    source: &'a dyn BlobSource,
    sink: &'a dyn BlobSink,
}

impl<'a> JobNamespaceView<'a> {
    /// Construct the view for `spec`. **Host-only**: calling this is the act of
    /// granting access, so a caller must already have resolved authorization.
    /// The spec is assumed validated (the store refuses malformed specs before
    /// they are stored).
    pub fn grant(spec: &'a JobSpec, source: &'a dyn BlobSource, sink: &'a dyn BlobSink) -> Self {
        Self {
            inputs: spec
                .inputs
                .iter()
                .map(|input| (input.name.as_str(), &input.blob))
                .collect(),
            output_name: spec.output.name.as_str(),
            max_output_bytes: spec.output.max_bytes,
            source,
            sink,
        }
    }

    /// The input slots this job granted, in name order.
    pub fn input_names(&self) -> impl Iterator<Item = &str> {
        self.inputs.keys().copied()
    }

    /// The single output slot this job granted.
    pub fn output_name(&self) -> &str {
        self.output_name
    }

    pub fn max_output_bytes(&self) -> u64 {
        self.max_output_bytes
    }

    /// Read a granted input. Fails for any name the job did not grant, and
    /// fails on a content-address mismatch *before* returning bytes.
    pub async fn read(&self, name: &str) -> Result<Vec<u8>, NamespaceError> {
        let blob = *self
            .inputs
            .get(name)
            .ok_or_else(|| NamespaceError::UngrantedInput(name.to_string()))?;
        let bytes = self
            .source
            .fetch(blob)
            .await?
            .ok_or_else(|| NamespaceError::MissingBlob(name.to_string()))?;
        if !blob.verifies(&bytes) {
            return Err(NamespaceError::DigestMismatch(name.to_string()));
        }
        Ok(bytes)
    }

    /// Commit the granted output. Refuses another slot name, and refuses an
    /// oversized write before anything reaches the sink — so a rejected write
    /// leaves no partial output behind.
    pub async fn commit(&self, name: &str, bytes: &[u8]) -> Result<OutputCommit, NamespaceError> {
        if name != self.output_name {
            return Err(NamespaceError::UngrantedOutput(name.to_string()));
        }
        let len = bytes.len() as u64;
        if len > self.max_output_bytes {
            return Err(NamespaceError::OverGrant {
                bytes: len,
                max_bytes: self.max_output_bytes,
            });
        }
        Ok(OutputCommit {
            name: name.to_string(),
            blob: self.sink.commit(bytes).await?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ident::ResourceId;
    use crate::spec::{DeterminismClass, JobInput, JobSpec};
    use std::sync::Mutex;

    /// A sink that records every write, so "nothing was committed" is testable.
    #[derive(Default)]
    struct RecordingSink {
        writes: Mutex<Vec<Vec<u8>>>,
    }

    impl BlobSink for RecordingSink {
        fn commit<'a>(&'a self, bytes: &'a [u8]) -> BoxFuture<'a, Result<BlobRef, NamespaceError>> {
            Box::pin(async move {
                self.writes.lock().unwrap().push(bytes.to_vec());
                Ok(BlobRef::blake3(bytes))
            })
        }
    }

    /// A source that lies: it returns bytes that do not match the address.
    struct LyingSource;

    impl BlobSource for LyingSource {
        fn fetch<'a>(
            &'a self,
            _blob: &'a BlobRef,
        ) -> BoxFuture<'a, Result<Option<Vec<u8>>, NamespaceError>> {
            Box::pin(async move { Ok(Some(b"not what was asked for".to_vec())) })
        }
    }

    fn spec_for(granted: &BlobRef) -> JobSpec {
        JobSpec::simple(
            ResourceId::parse("mesh.echo/v1").unwrap(),
            "payload",
            granted.clone(),
            "result",
            16,
            DeterminismClass::Exact,
        )
    }

    #[tokio::test]
    async fn a_granted_input_reads_and_an_ungranted_one_does_not() {
        let space = MemoryBlobSpace::in_memory();
        let granted = space.put(b"granted bytes").await.unwrap();
        let secret = space.put(b"the other job's private input").await.unwrap();
        assert!(
            space.has(&secret).await.unwrap(),
            "the blob is held locally"
        );

        let spec = spec_for(&granted);
        let view = JobNamespaceView::grant(&spec, &space, &space);
        assert_eq!(view.read("payload").await.unwrap(), b"granted bytes");
        assert_eq!(view.input_names().collect::<Vec<_>>(), ["payload"]);
        assert_eq!(
            view.read("secret").await,
            Err(NamespaceError::UngrantedInput("secret".to_string())),
            "holding the bytes locally does not make them reachable"
        );
    }

    #[tokio::test]
    async fn an_input_the_device_does_not_hold_is_missing_not_empty() {
        let space = MemoryBlobSpace::in_memory();
        let spec = spec_for(&BlobRef::blake3(b"never stored"));
        let view = JobNamespaceView::grant(&spec, &space, &space);
        assert_eq!(
            view.read("payload").await,
            Err(NamespaceError::MissingBlob("payload".to_string()))
        );
    }

    #[tokio::test]
    async fn a_digest_mismatch_fails_before_the_adapter_sees_bytes() {
        let sink = RecordingSink::default();
        let spec = spec_for(&BlobRef::blake3(b"the real bytes"));
        let view = JobNamespaceView::grant(&spec, &LyingSource, &sink);
        assert_eq!(
            view.read("payload").await,
            Err(NamespaceError::DigestMismatch("payload".to_string()))
        );
    }

    #[tokio::test]
    async fn only_the_granted_output_slot_is_writable() {
        let space = MemoryBlobSpace::in_memory();
        let sink = RecordingSink::default();
        let granted = space.put(b"in").await.unwrap();
        let spec = spec_for(&granted);
        let view = JobNamespaceView::grant(&spec, &space, &sink);

        let commit = view.commit("result", b"out").await.unwrap();
        assert_eq!(commit.blob, BlobRef::blake3(b"out"));
        assert_eq!(
            view.commit("elsewhere", b"out").await,
            Err(NamespaceError::UngrantedOutput("elsewhere".to_string()))
        );
        assert_eq!(
            sink.writes.lock().unwrap().len(),
            1,
            "the refused write never reached the sink"
        );
    }

    #[tokio::test]
    async fn an_oversized_output_fails_without_committing_anything() {
        let space = MemoryBlobSpace::in_memory();
        let sink = RecordingSink::default();
        let granted = space.put(b"in").await.unwrap();
        let spec = spec_for(&granted); // grants 16 bytes
        let view = JobNamespaceView::grant(&spec, &space, &sink);

        let too_big = vec![b'x'; 17];
        assert_eq!(
            view.commit("result", &too_big).await,
            Err(NamespaceError::OverGrant {
                bytes: 17,
                max_bytes: 16
            })
        );
        assert!(
            sink.writes.lock().unwrap().is_empty(),
            "no partial output was committed"
        );
    }

    #[tokio::test]
    async fn many_named_inputs_each_resolve_to_their_own_blob() {
        let space = MemoryBlobSpace::in_memory();
        let texts = space.put(b"a batch of texts").await.unwrap();
        let weights = space.put(b"model weights").await.unwrap();
        let spec = JobSpec {
            inputs: vec![
                JobInput {
                    name: "texts".to_string(),
                    blob: texts,
                },
                JobInput {
                    name: "weights".to_string(),
                    blob: weights,
                },
            ],
            ..spec_for(&BlobRef::blake3(b"unused"))
        };
        let view = JobNamespaceView::grant(&spec, &space, &space);
        assert_eq!(view.read("texts").await.unwrap(), b"a batch of texts");
        assert_eq!(view.read("weights").await.unwrap(), b"model weights");
        assert_eq!(view.input_names().collect::<Vec<_>>(), ["texts", "weights"]);
    }
}
