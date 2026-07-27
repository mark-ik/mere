//! Single-writer personal-vault replication over Stickleback.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use muniment::{Backend, MemoryBackend, RedbBackend, StoreError};
use p2panda_core::{Body, Hash, Header, Operation, SigningKey, Topic, VerifyingKey};
use p2panda_net::{Endpoint, Gossip};
use p2panda_store::logs::LogStore;
use p2panda_store::topics::TopicStore;
use serde::{Deserialize, Serialize};
use stickleback::{
    Admission, JoinError, JoinedSpace, MunimentStore, OperationPolicy, OperationProcessor,
    ProcessError, Reject, StoreTarget,
};
use zeroize::{Zeroize, Zeroizing};

use crate::{KnotVault, VaultDocument};

const LOG_ID: u64 = 0;
const SYNC_AAD: &[u8] = b"mere.knot.sync-operation.v1";

/// Signed addressing extension for one Knot vault space.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnotSyncExt {
    pub space_id: [u8; 32],
}

/// Plaintext event sealed inside the p2panda operation body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Zeroize)]
pub enum KnotSyncEvent {
    Put(VaultDocument),
    Delete { id: String },
}

/// Knot sync failures, including the deliberately unresolved multi-writer case.
#[derive(Debug, thiserror::Error)]
pub enum KnotSyncError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Process(#[from] ProcessError),
    #[error("sync payload: {0}")]
    Payload(String),
    #[error("document {0} has operations from more than one writer")]
    ConcurrentWriter(String),
}

#[derive(Clone)]
struct KnotSyncPolicy {
    space_id: [u8; 32],
    writers: BTreeSet<[u8; 32]>,
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
        if !self
            .writers
            .contains(operation.header.verifying_key.as_bytes())
        {
            return Err(Reject::new(
                "unrecognized-knot-writer",
                "operation author is not admitted to this Knot vault",
            ));
        }
        if operation.body.is_none() {
            return Err(Reject::new(
                "missing-knot-event",
                "Knot sync operations require a sealed body",
            ));
        }
        Ok(Admission::keep(StoreTarget::new(
            Topic::from(self.space_id),
            LOG_ID,
        )))
    }
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
        Self {
            store: MunimentStore::new(MemoryBackend::new()),
            policy: KnotSyncPolicy {
                space_id,
                writers: writers.into_iter().collect(),
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
        Ok(Self {
            store: MunimentStore::new(RedbBackend::open(path)?),
            policy: KnotSyncPolicy {
                space_id,
                writers: writers.into_iter().collect(),
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

    /// Seal, sign, admit, and store the next event in this device's log.
    pub async fn author(
        &self,
        signing_seed: [u8; 32],
        vault: &KnotVault,
        event: &KnotSyncEvent,
    ) -> Result<Operation<KnotSyncExt>, KnotSyncError> {
        let signing_key = SigningKey::from_bytes(&signing_seed);
        let author = signing_key.verifying_key();
        let previous = self.store.get_latest_entry(&author, &LOG_ID).await?;
        let (seq_num, backlink) = match previous {
            Some(operation) => (
                operation.header.seq_num + 1,
                Some(*operation.hash.as_bytes()),
            ),
            None => (0, None),
        };
        let plaintext = Zeroizing::new(
            serde_json::to_vec(event).map_err(|error| KnotSyncError::Payload(error.to_string()))?,
        );
        let aad = operation_aad(self.policy.space_id, author.as_bytes(), seq_num);
        let ciphertext = vault
            .seal_sync_payload(&aad, plaintext.as_slice())
            .map_err(KnotSyncError::Payload)?;
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

    /// The Knot-specific `accept` closure target used by Stickleback.
    pub async fn accept(&self, operation: &Operation<KnotSyncExt>) -> Result<bool, KnotSyncError> {
        let processor = OperationProcessor::new(self.store.clone(), self.policy.clone());
        Ok(processor.process(operation).await?.inserted())
    }

    /// Fold the sealed logs into documents. A second writer for one id is a
    /// hard boundary until Knot has a convergence rule.
    pub async fn documents(&self, vault: &KnotVault) -> Result<Vec<VaultDocument>, KnotSyncError> {
        let logs: BTreeMap<VerifyingKey, Vec<u64>> = self
            .store
            .resolve(&Topic::from(self.policy.space_id))
            .await?;
        let mut owners = BTreeMap::<String, [u8; 32]>::new();
        let mut documents = BTreeMap::<String, VaultDocument>::new();

        for (author, log_ids) in logs {
            let author_bytes = *author.as_bytes();
            for log_id in log_ids {
                let Some(entries) = self
                    .store
                    .get_log_entries(&author, &log_id, None, None)
                    .await?
                else {
                    continue;
                };
                for (operation, _) in entries {
                    let event = decode_event(vault, &operation)?;
                    let id = match &event {
                        KnotSyncEvent::Put(document) => &document.id,
                        KnotSyncEvent::Delete { id } => id,
                    };
                    if let Some(owner) = owners.get(id) {
                        if owner != &author_bytes {
                            return Err(KnotSyncError::ConcurrentWriter(id.clone()));
                        }
                    } else {
                        owners.insert(id.clone(), author_bytes);
                    }
                    match event {
                        KnotSyncEvent::Put(document) => {
                            documents.insert(document.id.clone(), document);
                        }
                        KnotSyncEvent::Delete { id } => {
                            documents.remove(&id);
                        }
                    }
                }
            }
        }
        Ok(documents.into_values().collect())
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

fn operation_aad(space_id: [u8; 32], author: &[u8; 32], seq_num: u32) -> Vec<u8> {
    let mut aad = Vec::with_capacity(SYNC_AAD.len() + 68);
    aad.extend_from_slice(SYNC_AAD);
    aad.extend_from_slice(&space_id);
    aad.extend_from_slice(author);
    aad.extend_from_slice(&seq_num.to_le_bytes());
    aad
}

fn decode_event(
    vault: &KnotVault,
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
    let plaintext = Zeroizing::new(
        vault
            .unseal_sync_payload(&aad, &body.to_bytes())
            .map_err(KnotSyncError::Payload)?,
    );
    serde_json::from_slice(plaintext.as_slice())
        .map_err(|error| KnotSyncError::Payload(error.to_string()))
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
        assert!(matches!(
            store.documents(&vault).await,
            Err(KnotSyncError::ConcurrentWriter(id)) if id == "shared"
        ));
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
