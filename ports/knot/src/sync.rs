//! Causal personal and Commons document replication over Stickleback.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use muniment::{Backend, MemoryBackend, RedbBackend, StoreError};
use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::{Body, Hash, Header, Operation, SigningKey, Topic, VerifyingKey};
use p2panda_net::{Endpoint, Gossip};
use p2panda_store::logs::LogStore;
use p2panda_store::topics::TopicStore;
use serde::{Deserialize, Serialize};
use stickleback::{
    Admission, CausalEntry, CausalError, CausalLimits, DataKeyring, GroupCiphertext,
    GroupCryptoError, JoinError, JoinedSpace, MunimentStore, OperationPolicy, OperationProcessor,
    PendingCausalOperation, ProcessError, Reject, StoreTarget, author_head, causal_projection,
    happens_before, observed_frontier, validate_causal_metadata,
};
use zeroize::{Zeroize, Zeroizing};

use crate::{KnotVault, VaultDocument};

const LOG_ID: u64 = 0;
const SYNC_AAD: &[u8] = b"mere.knot.sync-operation.v1";
const KNOT_CAUSAL_LIMITS: CausalLimits = CausalLimits {
    max_parents: 64,
    max_payload_bytes: 16 * 1024 * 1024,
};

/// The signed encryption contract for one Knot space.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum KnotEncryptionProfile {
    /// Personal device sync derives a key from the local vault root.
    #[default]
    PersonalVaultV1,
    /// Commons documents use the group's retained data-encryption epochs.
    CommonsDataV1,
}

/// Signed addressing extension for one Knot vault space.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnotSyncExt {
    pub space_id: [u8; 32],
    #[serde(default)]
    pub encryption: KnotEncryptionProfile,
    /// Exact per-author frontier observed before this event was authored.
    #[serde(default)]
    pub parents: Vec<[u8; 32]>,
}

/// Plaintext event sealed inside the p2panda operation body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Zeroize)]
pub enum KnotSyncEvent {
    Put(VaultDocument),
    Delete {
        id: String,
    },
    /// Replace the named causal document versions with one chosen value.
    Resolve {
        id: String,
        supersedes: Vec<[u8; 32]>,
        document: Option<VaultDocument>,
    },
}

/// Encryption material used by a Knot replica.
#[derive(Clone, Copy)]
pub enum KnotSyncCipher<'a> {
    Personal(&'a KnotVault),
    CommonsData(&'a DataKeyring),
}

impl KnotSyncCipher<'_> {
    fn profile(self) -> KnotEncryptionProfile {
        match self {
            Self::Personal(_) => KnotEncryptionProfile::PersonalVaultV1,
            Self::CommonsData(_) => KnotEncryptionProfile::CommonsDataV1,
        }
    }
}

/// Knot sync failures.
#[derive(Debug, thiserror::Error)]
pub enum KnotSyncError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Process(#[from] ProcessError),
    #[error(transparent)]
    Causal(#[from] CausalError),
    #[error(transparent)]
    GroupCrypto(#[from] GroupCryptoError),
    #[error("sync payload: {0}")]
    Payload(String),
    #[error("sync cipher does not match the space's signed encryption profile")]
    WrongEncryptionProfile,
    #[error("invalid conflict resolution: {0}")]
    InvalidResolution(String),
    #[error("Knot sync has no durable projection checkpoint")]
    MissingCheckpoint,
    #[error("document {0} has operations from more than one writer")]
    ConcurrentWriter(String),
}

#[derive(Clone)]
struct KnotSyncPolicy {
    space_id: [u8; 32],
    writers: BTreeSet<[u8; 32]>,
    encryption: KnotEncryptionProfile,
}

impl OperationPolicy<KnotSyncExt> for KnotSyncPolicy {
    type LogId = u64;

    fn admit(&self, operation: &Operation<KnotSyncExt>) -> Result<Admission<u64>, Reject> {
        if operation.header.extensions.space_id != self.space_id {
            return Err(Reject::new(
                "wrong-knot-space",
                "operation addresses a different Knot vault",
            ));
        }
        if operation.header.extensions.encryption != self.encryption {
            return Err(Reject::new(
                "wrong-knot-encryption-profile",
                "operation uses a different Knot encryption profile",
            ));
        }
        if !self
            .writers
            .contains(operation.header.verifying_key.as_bytes())
        {
            return Err(Reject::new(
                "unrecognized-knot-writer",
                "operation author is not admitted to this Knot vault",
            ));
        }
        let body = operation.body.as_ref().ok_or_else(|| {
            Reject::new(
                "missing-knot-event",
                "Knot sync operations require a sealed body",
            )
        })?;
        if self.encryption == KnotEncryptionProfile::CommonsDataV1 {
            decode_cbor::<GroupCiphertext, _>(body.to_bytes().as_slice()).map_err(|error| {
                Reject::new(
                    "invalid-knot-group-ciphertext",
                    format!("Commons Knot body is not a data-envelope: {error}"),
                )
            })?;
        }
        validate_causal_metadata(
            operation,
            &operation.header.extensions.parents,
            KNOT_CAUSAL_LIMITS,
        )
        .map_err(|error| Reject::new("invalid-knot-causality", error.to_string()))?;
        Ok(Admission::keep(StoreTarget::new(
            Topic::from(self.space_id),
            LOG_ID,
        )))
    }
}

/// One writer's current contribution to a conflicted document id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotDocumentVersion {
    pub writer: [u8; 32],
    pub operation: [u8; 32],
    /// `None` is that writer's current deletion.
    pub document: Option<VaultDocument>,
}

/// A document id touched by more than one writer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotDocumentConflict {
    pub id: String,
    pub versions: Vec<KnotDocumentVersion>,
}

/// Knot's current causally closed document view.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnotDocumentProjection {
    pub documents: Vec<VaultDocument>,
    pub conflicts: Vec<KnotDocumentConflict>,
    pub pending: Vec<PendingCausalOperation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnotAuthorHead {
    pub author: [u8; 32],
    pub log_id: u64,
    pub seq_num: u32,
    pub operation: [u8; 32],
}

/// Durable projection boundary required before domain-authorized pruning.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnotProjectionCheckpoint {
    pub version: u16,
    pub space_id: [u8; 32],
    pub heads: Vec<KnotAuthorHead>,
    pub document_digests: Vec<(String, [u8; 32])>,
    pub conflict_ids: Vec<String>,
    pub pending: Vec<([u8; 32], Vec<[u8; 32]>)>,
}

/// Exact retained tail after a durable checkpoint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnotTailReceipt {
    pub checkpoint: [u8; 32],
    pub operations: Vec<[u8; 32]>,
}

#[derive(Clone)]
struct StoredKnotOperation {
    operation: Operation<KnotSyncExt>,
    log_id: u64,
}

/// Replicated encrypted event store for one personal Knot vault.
#[derive(Clone)]
pub struct KnotSyncStore<B> {
    store: MunimentStore<B, KnotSyncExt>,
    policy: KnotSyncPolicy,
}

pub type KnotSyncFileStore = KnotSyncStore<RedbBackend>;

impl KnotSyncStore<MemoryBackend> {
    pub fn in_memory(space_id: [u8; 32], writers: impl IntoIterator<Item = [u8; 32]>) -> Self {
        Self::in_memory_with_profile(space_id, writers, KnotEncryptionProfile::PersonalVaultV1)
    }

    pub fn in_memory_commons(
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
    ) -> Self {
        Self::in_memory_with_profile(space_id, writers, KnotEncryptionProfile::CommonsDataV1)
    }

    pub fn in_memory_with_profile(
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
        encryption: KnotEncryptionProfile,
    ) -> Self {
        Self {
            store: MunimentStore::new(MemoryBackend::new()),
            policy: KnotSyncPolicy {
                space_id,
                writers: writers.into_iter().collect(),
                encryption,
            },
        }
    }
}

impl KnotSyncStore<RedbBackend> {
    pub fn open(
        path: impl AsRef<Path>,
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
    ) -> Result<Self, KnotSyncError> {
        Self::open_with_profile(
            path,
            space_id,
            writers,
            KnotEncryptionProfile::PersonalVaultV1,
        )
    }

    pub fn open_commons(
        path: impl AsRef<Path>,
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
    ) -> Result<Self, KnotSyncError> {
        Self::open_with_profile(
            path,
            space_id,
            writers,
            KnotEncryptionProfile::CommonsDataV1,
        )
    }

    pub fn open_with_profile(
        path: impl AsRef<Path>,
        space_id: [u8; 32],
        writers: impl IntoIterator<Item = [u8; 32]>,
        encryption: KnotEncryptionProfile,
    ) -> Result<Self, KnotSyncError> {
        Ok(Self {
            store: MunimentStore::new(RedbBackend::open(path)?),
            policy: KnotSyncPolicy {
                space_id,
                writers: writers.into_iter().collect(),
                encryption,
            },
        })
    }
}

impl<B> KnotSyncStore<B>
where
    B: Backend + Clone,
{
    pub fn space_id(&self) -> [u8; 32] {
        self.policy.space_id
    }

    pub fn encryption_profile(&self) -> KnotEncryptionProfile {
        self.policy.encryption
    }

    /// Seal, sign, admit, and store the next event in this device's log.
    pub async fn author(
        &self,
        signing_seed: [u8; 32],
        vault: &KnotVault,
        event: &KnotSyncEvent,
    ) -> Result<Operation<KnotSyncExt>, KnotSyncError> {
        self.author_with_cipher(signing_seed, KnotSyncCipher::Personal(vault), event)
            .await
    }

    pub async fn author_communal(
        &self,
        signing_seed: [u8; 32],
        keys: &DataKeyring,
        event: &KnotSyncEvent,
    ) -> Result<Operation<KnotSyncExt>, KnotSyncError> {
        self.author_with_cipher(signing_seed, KnotSyncCipher::CommonsData(keys), event)
            .await
    }

    pub async fn author_with_cipher(
        &self,
        signing_seed: [u8; 32],
        cipher: KnotSyncCipher<'_>,
        event: &KnotSyncEvent,
    ) -> Result<Operation<KnotSyncExt>, KnotSyncError> {
        self.require_cipher(cipher)?;
        let signing_key = SigningKey::from_bytes(&signing_seed);
        let author = signing_key.verifying_key();
        let records = self.load_operations().await?;
        let entries = causal_entries(&records);
        let parents = observed_frontier(&entries)?;
        let (seq_num, backlink) = author_head(&entries, *author.as_bytes(), &LOG_ID)?;
        let plaintext = Zeroizing::new(
            serde_json::to_vec(event).map_err(|error| KnotSyncError::Payload(error.to_string()))?,
        );
        let aad = operation_aad(self.policy.space_id, author.as_bytes(), seq_num);
        let ciphertext = seal_event(cipher, &aad, plaintext.as_slice())?;
        let body = Body::new(&ciphertext);
        let mut header = Header {
            version: 1,
            verifying_key: author,
            signature: None,
            payload_size: body.size(),
            payload_hash: Some(body.hash()),
            seq_num,
            backlink: backlink.map(Hash::from),
            extensions: KnotSyncExt {
                space_id: self.policy.space_id,
                encryption: self.policy.encryption,
                parents,
            },
        };
        header.sign(&signing_key);
        let operation = Operation {
            hash: header.hash(),
            header,
            body: Some(body),
        };
        self.accept(&operation).await?;
        Ok(operation)
    }

    pub async fn resolve_conflict(
        &self,
        signing_seed: [u8; 32],
        vault: &KnotVault,
        conflict: &KnotDocumentConflict,
        document: Option<VaultDocument>,
    ) -> Result<Operation<KnotSyncExt>, KnotSyncError> {
        self.resolve_conflict_with_cipher(
            signing_seed,
            KnotSyncCipher::Personal(vault),
            conflict,
            document,
        )
        .await
    }

    pub async fn resolve_communal_conflict(
        &self,
        signing_seed: [u8; 32],
        keys: &DataKeyring,
        conflict: &KnotDocumentConflict,
        document: Option<VaultDocument>,
    ) -> Result<Operation<KnotSyncExt>, KnotSyncError> {
        self.resolve_conflict_with_cipher(
            signing_seed,
            KnotSyncCipher::CommonsData(keys),
            conflict,
            document,
        )
        .await
    }

    pub async fn resolve_conflict_with_cipher(
        &self,
        signing_seed: [u8; 32],
        cipher: KnotSyncCipher<'_>,
        conflict: &KnotDocumentConflict,
        document: Option<VaultDocument>,
    ) -> Result<Operation<KnotSyncExt>, KnotSyncError> {
        if document
            .as_ref()
            .is_some_and(|document| document.id != conflict.id)
        {
            return Err(KnotSyncError::InvalidResolution(
                "chosen document id does not match the conflict".into(),
            ));
        }
        let mut supersedes: Vec<_> = conflict
            .versions
            .iter()
            .map(|version| version.operation)
            .collect();
        supersedes.sort();
        supersedes.dedup();
        self.author_with_cipher(
            signing_seed,
            cipher,
            &KnotSyncEvent::Resolve {
                id: conflict.id.clone(),
                supersedes,
                document,
            },
        )
        .await
    }

    fn require_cipher(&self, cipher: KnotSyncCipher<'_>) -> Result<(), KnotSyncError> {
        if cipher.profile() != self.policy.encryption {
            return Err(KnotSyncError::WrongEncryptionProfile);
        }
        Ok(())
    }

    /// The Knot-specific `accept` closure target used by Stickleback.
    pub async fn accept(&self, operation: &Operation<KnotSyncExt>) -> Result<bool, KnotSyncError> {
        let processor = OperationProcessor::new(self.store.clone(), self.policy.clone());
        Ok(processor.process(operation).await?.inserted())
    }

    async fn load_operations(&self) -> Result<Vec<StoredKnotOperation>, KnotSyncError> {
        let logs: BTreeMap<VerifyingKey, Vec<u64>> = self
            .store
            .resolve(&Topic::from(self.policy.space_id))
            .await?;
        let mut records = Vec::new();
        for (author, mut log_ids) in logs {
            log_ids.sort_unstable();
            log_ids.dedup();
            for log_id in log_ids {
                let Some(entries) = self
                    .store
                    .get_log_entries(&author, &log_id, None, None)
                    .await?
                else {
                    continue;
                };
                for (operation, _) in entries {
                    records.push(StoredKnotOperation { operation, log_id });
                }
            }
        }
        Ok(records)
    }

    /// Fold the causally closed subset into documents while preserving
    /// document conflicts and missing-history diagnostics.
    pub async fn projection(
        &self,
        vault: &KnotVault,
    ) -> Result<KnotDocumentProjection, KnotSyncError> {
        self.projection_with_cipher(KnotSyncCipher::Personal(vault))
            .await
    }

    pub async fn communal_projection(
        &self,
        keys: &DataKeyring,
    ) -> Result<KnotDocumentProjection, KnotSyncError> {
        self.projection_with_cipher(KnotSyncCipher::CommonsData(keys))
            .await
    }

    pub async fn projection_with_cipher(
        &self,
        cipher: KnotSyncCipher<'_>,
    ) -> Result<KnotDocumentProjection, KnotSyncError> {
        self.require_cipher(cipher)?;
        let records = self.load_operations().await?;
        let entries = causal_entries(&records);
        let projection = causal_projection(&entries)?;
        let mut current = BTreeMap::<String, BTreeMap<[u8; 32], KnotDocumentVersion>>::new();
        let mut event_documents = BTreeMap::<[u8; 32], String>::new();

        for index in projection.order {
            let operation = &records[index].operation;
            let writer = *operation.header.verifying_key.as_bytes();
            let operation_id = *operation.hash.as_bytes();
            let event = decode_event(cipher, operation)?;
            let (id, document) = match event {
                KnotSyncEvent::Put(document) => (document.id.clone(), Some(document)),
                KnotSyncEvent::Delete { id } => (id, None),
                KnotSyncEvent::Resolve {
                    id,
                    supersedes,
                    document,
                } => {
                    let targets = validate_resolution(
                        &entries,
                        &event_documents,
                        operation_id,
                        &id,
                        &supersedes,
                        document.as_ref(),
                    )?;
                    if let Some(versions) = current.get_mut(&id) {
                        versions.retain(|_, version| !targets.contains(&version.operation));
                    }
                    (id, document)
                }
            };
            event_documents.insert(operation_id, id.clone());
            current.entry(id).or_default().insert(
                writer,
                KnotDocumentVersion {
                    writer,
                    operation: operation_id,
                    document,
                },
            );
        }

        let mut documents = Vec::new();
        let mut conflicts = Vec::new();
        for (id, versions) in current {
            if versions.len() == 1 {
                if let Some(document) = versions
                    .into_values()
                    .next()
                    .and_then(|version| version.document)
                {
                    documents.push(document);
                }
            } else {
                conflicts.push(KnotDocumentConflict {
                    id,
                    versions: versions.into_values().collect(),
                });
            }
        }
        Ok(KnotDocumentProjection {
            documents,
            conflicts,
            pending: projection.pending,
        })
    }

    /// Compatibility view for existing callers. New consumers should use
    /// [`Self::projection`] so unrelated documents remain available beside an
    /// explicit conflict.
    pub async fn documents(&self, vault: &KnotVault) -> Result<Vec<VaultDocument>, KnotSyncError> {
        self.documents_with_cipher(KnotSyncCipher::Personal(vault))
            .await
    }

    pub async fn communal_documents(
        &self,
        keys: &DataKeyring,
    ) -> Result<Vec<VaultDocument>, KnotSyncError> {
        self.documents_with_cipher(KnotSyncCipher::CommonsData(keys))
            .await
    }

    pub async fn documents_with_cipher(
        &self,
        cipher: KnotSyncCipher<'_>,
    ) -> Result<Vec<VaultDocument>, KnotSyncError> {
        let projection = self.projection_with_cipher(cipher).await?;
        if let Some(conflict) = projection.conflicts.first() {
            return Err(KnotSyncError::ConcurrentWriter(conflict.id.clone()));
        }
        Ok(projection.documents)
    }

    /// Persist the current projection frontier. This is a prerequisite receipt,
    /// not permission to prune.
    pub async fn save_checkpoint(
        &self,
        vault: &KnotVault,
    ) -> Result<KnotProjectionCheckpoint, KnotSyncError> {
        self.save_checkpoint_with_cipher(KnotSyncCipher::Personal(vault))
            .await
    }

    pub async fn save_communal_checkpoint(
        &self,
        keys: &DataKeyring,
    ) -> Result<KnotProjectionCheckpoint, KnotSyncError> {
        self.save_checkpoint_with_cipher(KnotSyncCipher::CommonsData(keys))
            .await
    }

    pub async fn save_checkpoint_with_cipher(
        &self,
        cipher: KnotSyncCipher<'_>,
    ) -> Result<KnotProjectionCheckpoint, KnotSyncError> {
        let projection = self.projection_with_cipher(cipher).await?;
        let records = self.load_operations().await?;
        let mut heads = BTreeMap::<([u8; 32], u64), KnotAuthorHead>::new();
        for record in records {
            let operation = &record.operation;
            let key = (*operation.header.verifying_key.as_bytes(), record.log_id);
            let candidate = KnotAuthorHead {
                author: key.0,
                log_id: key.1,
                seq_num: operation.header.seq_num,
                operation: *operation.hash.as_bytes(),
            };
            if heads
                .get(&key)
                .is_none_or(|current| candidate.seq_num > current.seq_num)
            {
                heads.insert(key, candidate);
            }
        }
        let mut document_digests = Vec::new();
        for document in &projection.documents {
            let bytes = serde_json::to_vec(document)
                .map_err(|error| KnotSyncError::Payload(error.to_string()))?;
            document_digests.push((document.id.clone(), *blake3::hash(&bytes).as_bytes()));
        }
        let checkpoint = KnotProjectionCheckpoint {
            version: 1,
            space_id: self.policy.space_id,
            heads: heads.into_values().collect(),
            document_digests,
            conflict_ids: projection
                .conflicts
                .into_iter()
                .map(|conflict| conflict.id)
                .collect(),
            pending: projection
                .pending
                .into_iter()
                .map(|pending| (pending.operation, pending.missing))
                .collect(),
        };
        let bytes = serde_json::to_vec(&checkpoint)
            .map_err(|error| KnotSyncError::Payload(error.to_string()))?;
        self.store
            .backend()
            .put(&checkpoint_key(self.policy.space_id), &bytes)
            .await?;
        Ok(checkpoint)
    }

    pub async fn load_checkpoint(&self) -> Result<Option<KnotProjectionCheckpoint>, KnotSyncError> {
        let Some(bytes) = self
            .store
            .backend()
            .get(&checkpoint_key(self.policy.space_id))
            .await?
        else {
            return Ok(None);
        };
        let checkpoint: KnotProjectionCheckpoint = serde_json::from_slice(&bytes)
            .map_err(|error| KnotSyncError::Payload(error.to_string()))?;
        if checkpoint.version != 1 || checkpoint.space_id != self.policy.space_id {
            return Err(KnotSyncError::Payload(
                "checkpoint version or space does not match this store".into(),
            ));
        }
        Ok(Some(checkpoint))
    }

    /// Name the exact operations newer than the last durable checkpoint.
    pub async fn tail_receipt(&self) -> Result<KnotTailReceipt, KnotSyncError> {
        let checkpoint = self
            .load_checkpoint()
            .await?
            .ok_or(KnotSyncError::MissingCheckpoint)?;
        let bytes = serde_json::to_vec(&checkpoint)
            .map_err(|error| KnotSyncError::Payload(error.to_string()))?;
        let checkpoint_id = *blake3::hash(&bytes).as_bytes();
        let heads: BTreeMap<_, _> = checkpoint
            .heads
            .iter()
            .map(|head| ((head.author, head.log_id), head.seq_num))
            .collect();
        let mut tail = Vec::new();
        for record in self.load_operations().await? {
            let operation = &record.operation;
            let key = (*operation.header.verifying_key.as_bytes(), record.log_id);
            if heads
                .get(&key)
                .is_none_or(|seq_num| operation.header.seq_num > *seq_num)
            {
                tail.push((
                    key.0,
                    key.1,
                    operation.header.seq_num,
                    *operation.hash.as_bytes(),
                ));
            }
        }
        tail.sort();
        Ok(KnotTailReceipt {
            checkpoint: checkpoint_id,
            operations: tail
                .into_iter()
                .map(|(_, _, _, operation)| operation)
                .collect(),
        })
    }

    pub fn sync_store(&self) -> MunimentStore<B, KnotSyncExt> {
        self.store.clone()
    }
}

impl<B> KnotSyncStore<B>
where
    B: Backend + Clone + Send + Sync + 'static,
{
    /// Join the real p2panda LogSync lane with Knot's admission closure.
    pub async fn join(
        &self,
        endpoint: Endpoint,
        gossip: Gossip,
    ) -> Result<JoinedSpace<KnotSyncExt>, JoinError> {
        let accept_store = self.clone();
        JoinedSpace::join::<_, u64, _, _>(
            self.sync_store(),
            endpoint,
            gossip,
            self.policy.space_id,
            move |operation: Operation<KnotSyncExt>| {
                let store = accept_store.clone();
                async move { matches!(store.accept(&operation).await, Ok(true)) }
            },
        )
        .await
    }
}

fn causal_entries(records: &[StoredKnotOperation]) -> Vec<CausalEntry<u64>> {
    records
        .iter()
        .map(|record| {
            CausalEntry::from_operation(
                &record.operation,
                record.log_id,
                record.operation.header.extensions.parents.clone(),
            )
        })
        .collect()
}

fn checkpoint_key(space_id: [u8; 32]) -> String {
    let hex: String = space_id.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("knot-sync/checkpoint/{hex}")
}

fn operation_aad(space_id: [u8; 32], author: &[u8; 32], seq_num: u32) -> Vec<u8> {
    let mut aad = Vec::with_capacity(SYNC_AAD.len() + 68);
    aad.extend_from_slice(SYNC_AAD);
    aad.extend_from_slice(&space_id);
    aad.extend_from_slice(author);
    aad.extend_from_slice(&seq_num.to_le_bytes());
    aad
}

fn seal_event(
    cipher: KnotSyncCipher<'_>,
    aad: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, KnotSyncError> {
    match cipher {
        KnotSyncCipher::Personal(vault) => vault
            .seal_sync_payload(aad, plaintext)
            .map_err(KnotSyncError::Payload),
        KnotSyncCipher::CommonsData(keys) => {
            let envelope = keys.seal_random(plaintext)?;
            encode_cbor(&envelope).map_err(|error| KnotSyncError::Payload(error.to_string()))
        }
    }
}

fn decode_event(
    cipher: KnotSyncCipher<'_>,
    operation: &Operation<KnotSyncExt>,
) -> Result<KnotSyncEvent, KnotSyncError> {
    let body = operation
        .body
        .as_ref()
        .ok_or_else(|| KnotSyncError::Payload("operation body is absent".into()))?;
    let aad = operation_aad(
        operation.header.extensions.space_id,
        operation.header.verifying_key.as_bytes(),
        operation.header.seq_num,
    );
    let plaintext = Zeroizing::new(match cipher {
        KnotSyncCipher::Personal(vault) => vault
            .unseal_sync_payload(&aad, &body.to_bytes())
            .map_err(KnotSyncError::Payload)?,
        KnotSyncCipher::CommonsData(keys) => {
            let envelope: GroupCiphertext = decode_cbor(body.to_bytes().as_slice())
                .map_err(|error| KnotSyncError::Payload(error.to_string()))?;
            keys.open(&envelope)?
        }
    });
    serde_json::from_slice(plaintext.as_slice())
        .map_err(|error| KnotSyncError::Payload(error.to_string()))
}

fn validate_resolution(
    entries: &[CausalEntry<u64>],
    event_documents: &BTreeMap<[u8; 32], String>,
    resolution: [u8; 32],
    id: &str,
    supersedes: &[[u8; 32]],
    document: Option<&VaultDocument>,
) -> Result<BTreeSet<[u8; 32]>, KnotSyncError> {
    if supersedes.is_empty() {
        return Err(KnotSyncError::InvalidResolution(
            "resolution names no document versions".into(),
        ));
    }
    if supersedes.len() > KNOT_CAUSAL_LIMITS.max_parents {
        return Err(KnotSyncError::InvalidResolution(format!(
            "resolution names {} versions; maximum is {}",
            supersedes.len(),
            KNOT_CAUSAL_LIMITS.max_parents
        )));
    }
    if document.is_some_and(|document| document.id != id) {
        return Err(KnotSyncError::InvalidResolution(
            "chosen document id does not match the resolution".into(),
        ));
    }
    let targets: BTreeSet<_> = supersedes.iter().copied().collect();
    if targets.len() != supersedes.len() {
        return Err(KnotSyncError::InvalidResolution(
            "resolution repeats a document version".into(),
        ));
    }
    for target in &targets {
        let Some(target_id) = event_documents.get(target) else {
            return Err(KnotSyncError::InvalidResolution(
                "resolution names an unavailable document version".into(),
            ));
        };
        if target_id != id {
            return Err(KnotSyncError::InvalidResolution(
                "resolution names a version of another document".into(),
            ));
        }
        if !happens_before(entries, *target, resolution) {
            return Err(KnotSyncError::InvalidResolution(
                "resolution names a version outside its causal history".into(),
            ));
        }
    }
    Ok(targets)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use personae::{IdentityProvider, InMemoryProvider};
    use tempfile::tempdir;
    use transport::{P2pandaTransport, PeerID, sync_overlay_topic};

    use super::*;

    const SPACE: [u8; 32] = [0x81; 32];
    const VAULT_KEY: [u8; 32] = [0x82; 32];

    fn doc(id: &str, body: &str) -> VaultDocument {
        VaultDocument {
            id: id.into(),
            title: id.into(),
            body: body.as_bytes().to_vec(),
            media_type: "text/vnd.knot".into(),
        }
    }

    fn identities() -> (InMemoryProvider, InMemoryProvider) {
        (
            InMemoryProvider::from_seed([0x83; 32]),
            InMemoryProvider::from_seed([0x84; 32]),
        )
    }

    fn paired_group_keys() -> (DataKeyring, DataKeyring) {
        let mut alice = DataKeyring::new();
        let secret = alice.rotate_random().unwrap();
        let mut bob = DataKeyring::new();
        bob.install(secret);
        (alice, bob)
    }

    #[tokio::test]
    async fn two_memory_stores_converge_through_the_accept_seam() {
        let roots = tempdir().unwrap();
        let (alice, bob) = identities();
        let alice_seed = alice.master_keypair().to_seed();
        let bob_seed = bob.master_keypair().to_seed();
        let writers = [
            alice.master_public_key().to_bytes(),
            bob.master_public_key().to_bytes(),
        ];
        let a = KnotSyncStore::in_memory(SPACE, writers);
        let b = KnotSyncStore::in_memory(SPACE, writers);
        let alice_vault = KnotVault::open(roots.path().join("alice"), VAULT_KEY).unwrap();
        let bob_vault = KnotVault::open(roots.path().join("bob"), VAULT_KEY).unwrap();

        let a_op = a
            .author(
                alice_seed,
                &alice_vault,
                &KnotSyncEvent::Put(doc("alice-note", "amber")),
            )
            .await
            .unwrap();
        let b_op = b
            .author(
                bob_seed,
                &bob_vault,
                &KnotSyncEvent::Put(doc("bob-note", "blue")),
            )
            .await
            .unwrap();
        assert!(a.accept(&b_op).await.unwrap());
        assert!(b.accept(&a_op).await.unwrap());

        assert_eq!(
            a.documents(&alice_vault).await.unwrap(),
            b.documents(&bob_vault).await.unwrap()
        );
    }

    #[tokio::test]
    async fn commons_documents_use_group_epochs_instead_of_personal_vault_keys() {
        let roots = tempdir().unwrap();
        let (alice, bob) = identities();
        let writers = [
            alice.master_public_key().to_bytes(),
            bob.master_public_key().to_bytes(),
        ];
        let (mut alice_keys, bob_keys) = paired_group_keys();
        let a = KnotSyncStore::in_memory_commons(SPACE, writers);
        let b = KnotSyncStore::in_memory_commons(SPACE, writers);
        let alice_vault = KnotVault::open(roots.path().join("alice"), [0x91; 32]).unwrap();
        let bob_vault = KnotVault::open(roots.path().join("bob"), [0x92; 32]).unwrap();

        let old = a
            .author_communal(
                alice.master_keypair().to_seed(),
                &alice_keys,
                &KnotSyncEvent::Put(doc("shared", "before removal")),
            )
            .await
            .unwrap();
        b.accept(&old).await.unwrap();
        assert_eq!(
            a.communal_documents(&alice_keys).await.unwrap(),
            b.communal_documents(&bob_keys).await.unwrap()
        );
        assert!(matches!(
            a.projection(&alice_vault).await,
            Err(KnotSyncError::WrongEncryptionProfile)
        ));
        assert!(matches!(
            b.projection(&bob_vault).await,
            Err(KnotSyncError::WrongEncryptionProfile)
        ));

        alice_keys.rotate_random().unwrap();
        let after_removal = a
            .author_communal(
                alice.master_keypair().to_seed(),
                &alice_keys,
                &KnotSyncEvent::Put(doc("new", "after removal")),
            )
            .await
            .unwrap();
        assert!(b.accept(&after_removal).await.unwrap());
        assert!(matches!(
            b.communal_projection(&bob_keys).await,
            Err(KnotSyncError::GroupCrypto(GroupCryptoError::UnknownEpoch(
                _
            )))
        ));
        assert_eq!(a.communal_documents(&alice_keys).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn a_signed_encryption_profile_cannot_replay_into_another_knot_profile() {
        let roots = tempdir().unwrap();
        let alice = InMemoryProvider::from_seed([0x83; 32]);
        let writer = alice.master_public_key().to_bytes();
        let vault = KnotVault::open(roots.path(), VAULT_KEY).unwrap();
        let personal = KnotSyncStore::in_memory(SPACE, [writer]);
        let communal = KnotSyncStore::in_memory_commons(SPACE, [writer]);
        let operation = personal
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("note", "personal")),
            )
            .await
            .unwrap();

        assert!(communal.accept(&operation).await.is_err());
    }

    #[tokio::test]
    async fn concurrent_writers_for_one_document_are_refused_at_projection() {
        let roots = tempdir().unwrap();
        let (alice, bob) = identities();
        let writers = [
            alice.master_public_key().to_bytes(),
            bob.master_public_key().to_bytes(),
        ];
        let store = KnotSyncStore::in_memory(SPACE, writers);
        let vault = KnotVault::open(roots.path(), VAULT_KEY).unwrap();
        store
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("shared", "alice")),
            )
            .await
            .unwrap();
        store
            .author(
                bob.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("shared", "bob")),
            )
            .await
            .unwrap();
        store
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("solo", "still visible")),
            )
            .await
            .unwrap();
        let projection = store.projection(&vault).await.unwrap();
        assert_eq!(projection.documents, vec![doc("solo", "still visible")]);
        assert_eq!(projection.conflicts.len(), 1);
        assert_eq!(projection.conflicts[0].id, "shared");
        assert_eq!(projection.conflicts[0].versions.len(), 2);
        assert!(projection.pending.is_empty());
        assert!(matches!(
            store.documents(&vault).await,
            Err(KnotSyncError::ConcurrentWriter(id)) if id == "shared"
        ));
    }

    #[tokio::test]
    async fn an_explicit_resolution_replaces_exact_conflicting_versions() {
        let roots = tempdir().unwrap();
        let (alice, bob) = identities();
        let writers = [
            alice.master_public_key().to_bytes(),
            bob.master_public_key().to_bytes(),
        ];
        let vault = KnotVault::open(roots.path(), VAULT_KEY).unwrap();
        let a = KnotSyncStore::in_memory(SPACE, writers);
        let b = KnotSyncStore::in_memory(SPACE, writers);
        let alice_op = a
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("shared", "alice")),
            )
            .await
            .unwrap();
        let bob_op = b
            .author(
                bob.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("shared", "bob")),
            )
            .await
            .unwrap();
        a.accept(&bob_op).await.unwrap();
        b.accept(&alice_op).await.unwrap();
        let conflict = a.projection(&vault).await.unwrap().conflicts.remove(0);
        let resolution = a
            .resolve_conflict(
                alice.master_keypair().to_seed(),
                &vault,
                &conflict,
                Some(doc("shared", "chosen")),
            )
            .await
            .unwrap();
        b.accept(&resolution).await.unwrap();

        assert_eq!(
            a.documents(&vault).await.unwrap(),
            vec![doc("shared", "chosen")]
        );
        assert_eq!(
            a.documents(&vault).await.unwrap(),
            b.documents(&vault).await.unwrap()
        );
    }

    #[tokio::test]
    async fn a_resolution_does_not_erase_an_unseen_concurrent_version() {
        let roots = tempdir().unwrap();
        let (alice, bob) = identities();
        let writers = [
            alice.master_public_key().to_bytes(),
            bob.master_public_key().to_bytes(),
        ];
        let vault = KnotVault::open(roots.path(), VAULT_KEY).unwrap();
        let a = KnotSyncStore::in_memory(SPACE, writers);
        let b = KnotSyncStore::in_memory(SPACE, writers);
        let alice_op = a
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("shared", "alice")),
            )
            .await
            .unwrap();
        let local = KnotDocumentConflict {
            id: "shared".into(),
            versions: vec![KnotDocumentVersion {
                writer: alice.master_public_key().to_bytes(),
                operation: *alice_op.hash.as_bytes(),
                document: Some(doc("shared", "alice")),
            }],
        };
        a.resolve_conflict(
            alice.master_keypair().to_seed(),
            &vault,
            &local,
            Some(doc("shared", "alice resolved")),
        )
        .await
        .unwrap();
        let bob_op = b
            .author(
                bob.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("shared", "bob unseen")),
            )
            .await
            .unwrap();
        a.accept(&bob_op).await.unwrap();

        let projection = a.projection(&vault).await.unwrap();
        assert_eq!(projection.conflicts.len(), 1);
        assert_eq!(projection.conflicts[0].versions.len(), 2);
    }

    #[tokio::test]
    async fn a_resolution_cannot_name_a_version_outside_its_causal_history() {
        let roots = tempdir().unwrap();
        let (alice, bob) = identities();
        let writers = [
            alice.master_public_key().to_bytes(),
            bob.master_public_key().to_bytes(),
        ];
        let vault = KnotVault::open(roots.path(), VAULT_KEY).unwrap();
        let a = KnotSyncStore::in_memory(SPACE, writers);
        let b = KnotSyncStore::in_memory(SPACE, writers);
        let alice_op = a
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("shared", "alice")),
            )
            .await
            .unwrap();
        let forged_conflict = KnotDocumentConflict {
            id: "shared".into(),
            versions: vec![KnotDocumentVersion {
                writer: alice.master_public_key().to_bytes(),
                operation: *alice_op.hash.as_bytes(),
                document: Some(doc("shared", "alice")),
            }],
        };
        let forged_resolution = b
            .resolve_conflict(
                bob.master_keypair().to_seed(),
                &vault,
                &forged_conflict,
                Some(doc("shared", "forged")),
            )
            .await
            .unwrap();
        a.accept(&forged_resolution).await.unwrap();

        assert!(matches!(
            a.projection(&vault).await,
            Err(KnotSyncError::InvalidResolution(_))
        ));
    }

    #[tokio::test]
    async fn missing_history_blocks_only_its_document_branch() {
        let roots = tempdir().unwrap();
        let alice = InMemoryProvider::from_seed([0x83; 32]);
        let bob = InMemoryProvider::from_seed([0x84; 32]);
        let carol = InMemoryProvider::from_seed([0x85; 32]);
        let writers = [
            alice.master_public_key().to_bytes(),
            bob.master_public_key().to_bytes(),
            carol.master_public_key().to_bytes(),
        ];
        let vault = KnotVault::open(roots.path(), VAULT_KEY).unwrap();
        let parent_store = KnotSyncStore::in_memory(SPACE, writers);
        let child_store = KnotSyncStore::in_memory(SPACE, writers);
        let unrelated_store = KnotSyncStore::in_memory(SPACE, writers);
        let receiver = KnotSyncStore::in_memory(SPACE, writers);

        let parent = parent_store
            .author(
                alice.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("shared", "parent")),
            )
            .await
            .unwrap();
        child_store.accept(&parent).await.unwrap();
        let child = child_store
            .author(
                bob.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("shared", "child")),
            )
            .await
            .unwrap();
        let unrelated = unrelated_store
            .author(
                carol.master_keypair().to_seed(),
                &vault,
                &KnotSyncEvent::Put(doc("solo", "visible")),
            )
            .await
            .unwrap();

        receiver.accept(&child).await.unwrap();
        receiver.accept(&unrelated).await.unwrap();
        let partial = receiver.projection(&vault).await.unwrap();
        assert_eq!(partial.documents, vec![doc("solo", "visible")]);
        assert_eq!(partial.pending.len(), 1);
        assert_eq!(partial.pending[0].operation, *child.hash.as_bytes());
        assert_eq!(partial.pending[0].missing, vec![*parent.hash.as_bytes()]);

        receiver.accept(&parent).await.unwrap();
        let complete = receiver.projection(&vault).await.unwrap();
        assert!(complete.pending.is_empty());
        assert_eq!(complete.conflicts.len(), 1);
        assert_eq!(complete.conflicts[0].id, "shared");
        assert_eq!(complete.documents, vec![doc("solo", "visible")]);
    }

    #[tokio::test]
    async fn redb_reopen_restores_author_head_and_observed_frontier() {
        let roots = tempdir().unwrap();
        let database = roots.path().join("knot-sync.redb");
        let vault = KnotVault::open(roots.path().join("vault"), VAULT_KEY).unwrap();
        let alice = InMemoryProvider::from_seed([0x83; 32]);
        let writer = alice.master_public_key().to_bytes();
        let seed = alice.master_keypair().to_seed();

        let second = {
            let store = KnotSyncFileStore::open(&database, SPACE, [writer]).unwrap();
            store
                .author(seed, &vault, &KnotSyncEvent::Put(doc("one", "first")))
                .await
                .unwrap();
            let second = store
                .author(seed, &vault, &KnotSyncEvent::Put(doc("two", "second")))
                .await
                .unwrap();
            let checkpoint = store.save_checkpoint(&vault).await.unwrap();
            assert_eq!(checkpoint.heads.len(), 1);
            assert_eq!(checkpoint.heads[0].operation, *second.hash.as_bytes());
            second
        };

        let reopened = KnotSyncFileStore::open(&database, SPACE, [writer]).unwrap();
        assert_eq!(
            reopened.load_checkpoint().await.unwrap().unwrap().heads[0].operation,
            *second.hash.as_bytes()
        );
        let third = reopened
            .author(seed, &vault, &KnotSyncEvent::Put(doc("three", "third")))
            .await
            .unwrap();
        assert_eq!(third.header.seq_num, second.header.seq_num + 1);
        assert_eq!(
            third.header.backlink.as_ref().map(|hash| *hash.as_bytes()),
            Some(*second.hash.as_bytes())
        );
        assert_eq!(
            third.header.extensions.parents,
            vec![*second.hash.as_bytes()]
        );
        assert_eq!(
            reopened.tail_receipt().await.unwrap().operations,
            vec![*third.hash.as_bytes()]
        );
        assert_eq!(
            reopened.projection(&vault).await.unwrap().documents.len(),
            3
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn two_instances_converge_over_real_p2panda_logsync() {
        let roots = tempdir().unwrap();
        let (alice, bob) = identities();
        let alice_id = PeerID::from_public_key(alice.master_public_key());
        let bob_id = PeerID::from_public_key(bob.master_public_key());
        let writers = [
            alice.master_public_key().to_bytes(),
            bob.master_public_key().to_bytes(),
        ];
        let alice_transport = P2pandaTransport::builder(alice.master_keypair())
            .gossip()
            .bind()
            .await
            .unwrap();
        let bob_transport = P2pandaTransport::builder(bob.master_keypair())
            .gossip()
            .bind()
            .await
            .unwrap();
        let overlay = sync_overlay_topic(SPACE);
        alice_transport
            .add_peer(bob_transport.endpoint_addr().await.unwrap())
            .await
            .unwrap();
        alice_transport
            .set_topics(bob_id, &[overlay])
            .await
            .unwrap();
        bob_transport
            .add_peer(alice_transport.endpoint_addr().await.unwrap())
            .await
            .unwrap();
        bob_transport
            .set_topics(alice_id, &[overlay])
            .await
            .unwrap();

        let alice_store = KnotSyncStore::in_memory(SPACE, writers);
        let bob_store = KnotSyncStore::in_memory(SPACE, writers);
        let alice_vault = Arc::new(KnotVault::open(roots.path().join("alice"), VAULT_KEY).unwrap());
        let bob_vault = Arc::new(KnotVault::open(roots.path().join("bob"), VAULT_KEY).unwrap());
        alice_store
            .author(
                alice.master_keypair().to_seed(),
                &alice_vault,
                &KnotSyncEvent::Put(doc("alice-note", "amber")),
            )
            .await
            .unwrap();
        bob_store
            .author(
                bob.master_keypair().to_seed(),
                &bob_vault,
                &KnotSyncEvent::Put(doc("bob-note", "blue")),
            )
            .await
            .unwrap();

        let (a_endpoint, a_gossip) = alice_transport.sync_parts().unwrap();
        let (b_endpoint, b_gossip) = bob_transport.sync_parts().unwrap();
        let alice_joined = alice_store.join(a_endpoint, a_gossip).await.unwrap();
        let bob_joined = bob_store.join(b_endpoint, b_gossip).await.unwrap();

        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if alice_store.documents(&alice_vault).await.unwrap().len() == 2
                    && bob_store.documents(&bob_vault).await.unwrap().len() == 2
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("Knot peers did not converge");

        assert_eq!(
            alice_store.documents(&alice_vault).await.unwrap(),
            bob_store.documents(&bob_vault).await.unwrap()
        );
        assert!(alice_joined.ops_received() >= 1);
        assert!(bob_joined.ops_received() >= 1);
    }
}
