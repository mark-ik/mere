//! Minimal encrypted Commons chat domain.
//!
//! This is intentionally distinct from Murm's bilateral `Post` grammar. It is
//! the second consumer of Stickleback's causal projection seam after Knot.

use std::collections::BTreeMap;

use muniment::{Backend, MemoryBackend, StoreError};
use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::{Body, Hash, Header, Operation, SigningKey, Topic, VerifyingKey};
use p2panda_net::{Endpoint, Gossip};
use p2panda_store::logs::LogStore;
use p2panda_store::topics::TopicStore;
use serde::{Deserialize, Serialize};
use stickleback::{
    Admission, CausalEntry, CausalError, CausalLimits, DataKeyring, GroupCiphertext,
    GroupCryptoError, GroupEncryptionMode, GroupEncryptionProfile, JoinError, JoinedSpace,
    MunimentStore, OperationPolicy, OperationProcessor, PendingCausalOperation, ProcessError,
    Reject, StoreTarget, author_head, causal_projection, observed_frontier,
    validate_causal_metadata,
};

const CHAT_LOG: u64 = 0;
const CHAT_LIMITS: CausalLimits = CausalLimits {
    max_parents: 64,
    max_payload_bytes: 1024 * 1024,
};

/// The first chat fixture deliberately chooses durable data encryption. A
/// forward-secure profile is a different wire/runtime contract.
pub const COMMONS_CHAT_PROFILE: GroupEncryptionProfile = GroupEncryptionProfile::durable_data(8);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatClass {
    #[default]
    Channel,
    Message,
}

impl ChatClass {
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Channel => "commons.channel",
            Self::Message => "commons.message",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatExt {
    pub space_id: [u8; 32],
    pub class: ChatClass,
    #[serde(default)]
    pub parents: Vec<[u8; 32]>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Channel {
    pub id: String,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub channel: String,
    pub body: String,
    pub sent_at_ms: u64,
    pub reply_to: Option<[u8; 32]>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatEvent {
    Channel(Channel),
    Message(Message),
}

impl ChatEvent {
    fn class(&self) -> ChatClass {
        match self {
            Self::Channel(_) => ChatClass::Channel,
            Self::Message(_) => ChatClass::Message,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthoredMessage {
    pub operation: [u8; 32],
    pub author: [u8; 32],
    pub message: Message,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChatProjection {
    pub channels: Vec<Channel>,
    pub messages: Vec<AuthoredMessage>,
    pub pending: Vec<PendingCausalOperation>,
}

#[derive(Debug, thiserror::Error)]
pub enum ChatError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Process(#[from] ProcessError),
    #[error(transparent)]
    Causal(#[from] CausalError),
    #[error(transparent)]
    Crypto(#[from] GroupCryptoError),
    #[error("chat wire: {0}")]
    Wire(String),
}

#[derive(Clone)]
struct ChatPolicy {
    space_id: [u8; 32],
}

impl OperationPolicy<ChatExt> for ChatPolicy {
    type LogId = u64;

    fn admit(&self, operation: &Operation<ChatExt>) -> Result<Admission<u64>, Reject> {
        if operation.header.extensions.space_id != self.space_id {
            return Err(Reject::new(
                "wrong-chat-space",
                "operation addresses another Commons chat space",
            ));
        }
        validate_causal_metadata(operation, &operation.header.extensions.parents, CHAT_LIMITS)
            .map_err(|error| Reject::new("invalid-chat-causality", error.to_string()))?;
        let body = operation.body.as_ref().ok_or_else(|| {
            Reject::new(
                "missing-chat-ciphertext",
                "chat operation requires an encrypted body",
            )
        })?;
        decode_cbor::<GroupCiphertext, _>(body.to_bytes().as_slice())
            .map_err(|error| Reject::new("invalid-chat-ciphertext", error.to_string()))?;
        Ok(Admission::keep(StoreTarget::new(
            Topic::from(self.space_id),
            CHAT_LOG,
        )))
    }
}

#[derive(Clone)]
struct StoredChatOperation {
    operation: Operation<ChatExt>,
    log_id: u64,
}

/// One member's encrypted Commons chat replica.
pub struct ChatReplica<B: Backend + Clone> {
    store: MunimentStore<B, ChatExt>,
    space_id: [u8; 32],
    signing_seed: [u8; 32],
    keys: DataKeyring,
}

impl ChatReplica<MemoryBackend> {
    pub fn in_memory(space_id: [u8; 32], signing_seed: [u8; 32], keys: DataKeyring) -> Self {
        Self::new(MemoryBackend::new(), space_id, signing_seed, keys)
    }
}

impl<B: Backend + Clone> ChatReplica<B> {
    pub fn new(backend: B, space_id: [u8; 32], signing_seed: [u8; 32], keys: DataKeyring) -> Self {
        debug_assert_eq!(COMMONS_CHAT_PROFILE.mode, GroupEncryptionMode::Data);
        Self {
            store: MunimentStore::new(backend),
            space_id,
            signing_seed,
            keys,
        }
    }

    pub fn sync_store(&self) -> MunimentStore<B, ChatExt> {
        self.store.clone()
    }

    pub fn key_state(&self) -> Result<Vec<u8>, GroupCryptoError> {
        self.keys.to_bytes()
    }

    pub async fn author(&mut self, event: ChatEvent) -> Result<Operation<ChatExt>, ChatError> {
        let records = self.load_operations().await?;
        let entries = causal_entries(&records);
        let parents = observed_frontier(&entries)?;
        let signing_key = SigningKey::from_bytes(&self.signing_seed);
        let author = signing_key.verifying_key();
        let (seq_num, backlink) = author_head(&entries, *author.as_bytes(), &CHAT_LOG)?;
        let plaintext = encode_cbor(&event).map_err(|error| ChatError::Wire(error.to_string()))?;
        let envelope = self
            .keys
            .seal(&plaintext, &p2panda_encryption::Rng::default())?;
        let body_bytes =
            encode_cbor(&envelope).map_err(|error| ChatError::Wire(error.to_string()))?;
        let body = Body::new(&body_bytes);
        let mut header = Header {
            version: 1,
            verifying_key: author,
            signature: None,
            payload_size: body.size(),
            payload_hash: Some(body.hash()),
            seq_num,
            backlink: backlink.map(Hash::from),
            extensions: ChatExt {
                space_id: self.space_id,
                class: event.class(),
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

    pub async fn accept(&self, operation: &Operation<ChatExt>) -> Result<bool, ChatError> {
        let processor = OperationProcessor::new(
            self.store.clone(),
            ChatPolicy {
                space_id: self.space_id,
            },
        );
        Ok(processor.process(operation).await?.inserted())
    }

    pub async fn projection(&self) -> Result<ChatProjection, ChatError> {
        let records = self.load_operations().await?;
        let causal = causal_projection(&causal_entries(&records))?;
        let mut channels = BTreeMap::new();
        let mut messages = Vec::new();
        for index in causal.order {
            let operation = &records[index].operation;
            let event = decode_event(&self.keys, operation)?;
            if event.class() != operation.header.extensions.class {
                return Err(ChatError::Wire(
                    "signed content class does not match encrypted event".into(),
                ));
            }
            match event {
                ChatEvent::Channel(channel) => {
                    channels.insert(channel.id.clone(), channel);
                }
                ChatEvent::Message(message) => messages.push(AuthoredMessage {
                    operation: *operation.hash.as_bytes(),
                    author: *operation.header.verifying_key.as_bytes(),
                    message,
                }),
            }
        }
        Ok(ChatProjection {
            channels: channels.into_values().collect(),
            messages,
            pending: causal.pending,
        })
    }

    async fn load_operations(&self) -> Result<Vec<StoredChatOperation>, ChatError> {
        let logs: BTreeMap<VerifyingKey, Vec<u64>> =
            self.store.resolve(&Topic::from(self.space_id)).await?;
        let mut records = Vec::new();
        for (author, mut log_ids) in logs {
            log_ids.sort_unstable();
            log_ids.dedup();
            for log_id in log_ids {
                for (operation, _) in self
                    .store
                    .get_log_entries(&author, &log_id, None, None)
                    .await?
                    .unwrap_or_default()
                {
                    records.push(StoredChatOperation { operation, log_id });
                }
            }
        }
        Ok(records)
    }
}

impl<B: Backend + Clone + Send + Sync + 'static> ChatReplica<B> {
    pub async fn join(
        &self,
        endpoint: Endpoint,
        gossip: Gossip,
    ) -> Result<JoinedSpace<ChatExt>, JoinError> {
        let store = self.sync_store();
        let accept_store = self.store.clone();
        let space_id = self.space_id;
        JoinedSpace::join::<_, u64, _, _>(
            store,
            endpoint,
            gossip,
            space_id,
            move |operation: Operation<ChatExt>| {
                let processor = OperationProcessor::new(
                    accept_store.clone(),
                    ChatPolicy { space_id },
                );
                async move { matches!(processor.process(&operation).await, Ok(out) if out.inserted()) }
            },
        )
        .await
    }
}

fn causal_entries(records: &[StoredChatOperation]) -> Vec<CausalEntry<u64>> {
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

fn decode_event(
    keys: &DataKeyring,
    operation: &Operation<ChatExt>,
) -> Result<ChatEvent, ChatError> {
    let body = operation
        .body
        .as_ref()
        .ok_or_else(|| ChatError::Wire("operation body is absent".into()))?;
    let envelope: GroupCiphertext = decode_cbor(body.to_bytes().as_slice())
        .map_err(|error| ChatError::Wire(error.to_string()))?;
    let plaintext = keys.open(&envelope)?;
    decode_cbor(plaintext.as_slice()).map_err(|error| ChatError::Wire(error.to_string()))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::net::{SocketAddr, TcpListener};
    use std::sync::Arc;
    use std::time::Duration;

    use murm::{CabalId, CabalKey, CabalKeyring};
    use personae::{IdentityProvider, InMemoryProvider};
    use stickleback::{
        DropLimits, DropRecord, decode_operation_record, operation_record, read_protected_drop,
        write_protected_drop,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use transport::{
        Alpn, P2pandaTransport, PeerID, ReticulumInterface, ReticulumTransport, Transport,
        sync_overlay_topic,
    };

    use super::*;

    const SPACE: [u8; 32] = [0x51; 32];

    fn paired_keys() -> (DataKeyring, DataKeyring) {
        let rng = p2panda_encryption::Rng::default();
        let mut alice = DataKeyring::new();
        let secret = alice.rotate(&rng).unwrap();
        let mut bob = DataKeyring::new();
        bob.install(secret);
        (alice, bob)
    }

    fn free_addr() -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        addr
    }

    #[tokio::test]
    async fn two_partitioned_members_converge_through_memory_accept() {
        let (alice_keys, bob_keys) = paired_keys();
        let mut alice = ChatReplica::in_memory(SPACE, [0xa1; 32], alice_keys);
        let mut bob = ChatReplica::in_memory(SPACE, [0xb2; 32], bob_keys);

        let channel = alice
            .author(ChatEvent::Channel(Channel {
                id: "general".into(),
                title: "General".into(),
            }))
            .await
            .unwrap();
        bob.accept(&channel).await.unwrap();
        let a_message = alice
            .author(ChatEvent::Message(Message {
                channel: "general".into(),
                body: "amber".into(),
                sent_at_ms: 1,
                reply_to: None,
            }))
            .await
            .unwrap();
        let b_message = bob
            .author(ChatEvent::Message(Message {
                channel: "general".into(),
                body: "blue".into(),
                sent_at_ms: 2,
                reply_to: Some(*a_message.hash.as_bytes()),
            }))
            .await
            .unwrap();
        alice.accept(&b_message).await.unwrap();
        bob.accept(&a_message).await.unwrap();

        let a = alice.projection().await.unwrap();
        let b = bob.projection().await.unwrap();
        assert_eq!(a, b);
        assert_eq!(a.channels.len(), 1);
        assert_eq!(a.messages.len(), 2);
        assert!(a.pending.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn two_members_converge_over_real_logsync() {
        let alice_identity = Arc::new(InMemoryProvider::from_seed([0xa1; 32]));
        let bob_identity = Arc::new(InMemoryProvider::from_seed([0xb2; 32]));
        let alice_transport = P2pandaTransport::builder(alice_identity.master_keypair())
            .gossip()
            .bind()
            .await
            .unwrap();
        let bob_transport = P2pandaTransport::builder(bob_identity.master_keypair())
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
            .set_topics(
                PeerID::from_public_key(bob_identity.master_public_key()),
                &[overlay],
            )
            .await
            .unwrap();
        bob_transport
            .add_peer(alice_transport.endpoint_addr().await.unwrap())
            .await
            .unwrap();
        bob_transport
            .set_topics(
                PeerID::from_public_key(alice_identity.master_public_key()),
                &[overlay],
            )
            .await
            .unwrap();

        let (alice_keys, bob_keys) = paired_keys();
        let mut alice =
            ChatReplica::in_memory(SPACE, alice_identity.master_keypair().to_seed(), alice_keys);
        let mut bob =
            ChatReplica::in_memory(SPACE, bob_identity.master_keypair().to_seed(), bob_keys);
        alice
            .author(ChatEvent::Message(Message {
                channel: "general".into(),
                body: "over the lane".into(),
                sent_at_ms: 3,
                reply_to: None,
            }))
            .await
            .unwrap();
        bob.author(ChatEvent::Message(Message {
            channel: "general".into(),
            body: "from the other side".into(),
            sent_at_ms: 4,
            reply_to: None,
        }))
        .await
        .unwrap();

        let (a_endpoint, a_gossip) = alice_transport.sync_parts().unwrap();
        let (b_endpoint, b_gossip) = bob_transport.sync_parts().unwrap();
        let alice_joined = alice.join(a_endpoint, a_gossip).await.unwrap();
        let bob_joined = bob.join(b_endpoint, b_gossip).await.unwrap();
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if alice.projection().await.unwrap().messages.len() == 2
                    && bob.projection().await.unwrap().messages.len() == 2
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("Commons chat peers did not converge");
        assert_eq!(
            alice.projection().await.unwrap(),
            bob.projection().await.unwrap()
        );
        assert!(alice_joined.ops_received() >= 1);
        assert!(bob_joined.ops_received() >= 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn identical_signed_ciphertext_survives_native_drop_and_reticulum_tcp() {
        let (alice_keys, _) = paired_keys();
        let mut alice = ChatReplica::in_memory(SPACE, [0xa1; 32], alice_keys);
        let operation = alice
            .author(ChatEvent::Message(Message {
                channel: "general".into(),
                body: "same bytes on every carrier".into(),
                sent_at_ms: 5,
                reply_to: None,
            }))
            .await
            .unwrap();
        let canonical_record = operation_record(&operation, true);
        let canonical = encode_cbor(&canonical_record).unwrap();

        let protector =
            CabalKeyring::from_cabal_key(CabalId::new([0xd1; 32]), &CabalKey::new([0xd2; 32]));
        let mut drop_bytes = Vec::new();
        write_protected_drop(
            &mut drop_bytes,
            &[operation_record(&operation, true)],
            DropLimits::default(),
            &protector,
        )
        .unwrap();
        let (_, records) =
            read_protected_drop(Cursor::new(drop_bytes), DropLimits::default(), &protector)
                .unwrap();
        assert_eq!(records[0], canonical_record);
        let recovered = decode_operation_record::<ChatExt>(&records[0])
            .unwrap()
            .unwrap();
        assert_eq!(operation_record(&recovered, true), canonical_record);

        let server_identity = InMemoryProvider::from_seed([0xe1; 32]);
        let client_identity = InMemoryProvider::from_seed([0xe2; 32]);
        let server_peer = PeerID::from_public_key(server_identity.master_public_key());
        let server_keypair = server_identity.master_keypair();
        let client_keypair = client_identity.master_keypair();
        let addr = free_addr();
        let alpn = Alpn::new("mere/commons-operation/v1");
        let server = ReticulumTransport::builder(&server_keypair)
            .alpns(vec![alpn.clone()])
            .interfaces(vec![ReticulumInterface::TcpServer { bind: addr }])
            .announce_interval(Duration::from_millis(100))
            .bind()
            .await
            .unwrap();
        let client = ReticulumTransport::builder(&client_keypair)
            .alpns(vec![alpn.clone()])
            .interfaces(vec![ReticulumInterface::TcpClient { addr }])
            .announce_interval(Duration::from_millis(100))
            .connect_timeout(Duration::from_secs(10))
            .bind()
            .await
            .unwrap();
        let expected = canonical.clone();
        let accept = tokio::spawn(async move {
            let accepted = server.accept(alpn).await.unwrap();
            let mut stream = accepted.into_stream();
            let len = stream.read_u32_le().await.unwrap();
            let mut received = vec![0; len as usize];
            stream.read_exact(&mut received).await.unwrap();
            received
        });
        let mut stream = client
            .connect(server_peer, Alpn::new("mere/commons-operation/v1"))
            .await
            .unwrap();
        stream.write_u32_le(canonical.len() as u32).await.unwrap();
        stream.write_all(&canonical).await.unwrap();
        stream.flush().await.unwrap();
        let received = tokio::time::timeout(Duration::from_secs(15), accept)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(received, expected);
        let decoded_record: DropRecord = decode_cbor(received.as_slice()).unwrap();
        assert_eq!(decoded_record, canonical_record);
        let decoded = decode_operation_record::<ChatExt>(&decoded_record)
            .unwrap()
            .unwrap();
        assert!(decoded.header.verify());
    }
}
