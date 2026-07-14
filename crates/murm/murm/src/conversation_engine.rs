//! Native conversation runtime over [`crate::ConversationStore`].

use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::sync::{Arc, Mutex};

use identity::{Ed25519Keypair, Ed25519PublicKey, IdentityProvider};
use murm_replication::{DropImportReport, DropLimits, MunimentStore, ProcessOutcome};
use p2panda_core::Operation;
use tokio::sync::{Mutex as AsyncMutex, broadcast};

use crate::{
    CabalExt, CabalId, CabalKey, CabalKeyEpoch, CabalKeyring, ChannelName, ConversationBackend,
    ConversationStorage, ConversationStore, InfoEntry, MurmError, Post, PostId, PostKind,
    hash_cabal_id, hash_post, operation_to_post, post_to_operation, sign_post,
};

const EVENT_CHANNEL_CAPACITY: usize = 256;

/// Result of reconciling the materialized conversation view with its store.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConversationRefresh {
    /// Total materialized posts after reconciliation.
    pub materialized: u64,
    /// Posts newly added to the view.
    pub added: u64,
    /// Posts removed because the durable store no longer retains them.
    pub removed: u64,
}

/// Murm's native conversation runtime.
///
/// Signed-post grammar, storage, admission, materialized history, and
/// subscriptions are assembled here over the shared replication substrate.
pub struct ConversationEngine {
    identity: Arc<dyn IdentityProvider>,
    storage: ConversationStorage,
    conversations: Mutex<HashMap<[u8; 32], Arc<ConversationSession>>>,
}

struct ConversationSession {
    keypair: Ed25519Keypair,
    keyring: CabalKeyring,
    store: ConversationStore<ConversationBackend>,
    author_head: AsyncMutex<(u64, Option<PostId>)>,
    ingest: AsyncMutex<()>,
    posts: Mutex<BTreeMap<[u8; 32], Post>>,
    events: broadcast::Sender<Post>,
}

impl ConversationEngine {
    /// Create a runtime using `identity` for per-conversation signing keys.
    pub fn new(identity: Arc<dyn IdentityProvider>) -> Self {
        Self::with_storage(identity, ConversationStorage::Memory)
    }

    /// Create a runtime using host-selected conversation persistence.
    pub fn with_storage(identity: Arc<dyn IdentityProvider>, storage: ConversationStorage) -> Self {
        Self {
            identity,
            storage,
            conversations: Mutex::new(HashMap::new()),
        }
    }

    /// Open or rejoin a conversation by its shared secret key.
    pub async fn open(&self, conversation_key: [u8; 32]) -> Result<[u8; 32], MurmError> {
        let id = hash_cabal_id(&conversation_key);
        if self.conversations.lock().unwrap().contains_key(&id) {
            return Ok(id);
        }

        let keypair = self.identity.derive_keypair(&conversation_key)?;
        let backend = ConversationBackend::open(&self.storage, id)?;
        let session = Arc::new(ConversationSession {
            keypair,
            keyring: CabalKeyring::from_cabal_key(
                CabalId::new(id),
                &CabalKey::new(conversation_key),
            ),
            store: ConversationStore::new(id, backend),
            author_head: AsyncMutex::new((0, None)),
            ingest: AsyncMutex::new(()),
            posts: Mutex::new(BTreeMap::new()),
            events: broadcast::channel(EVENT_CHANNEL_CAPACITY).0,
        });
        Self::rebuild(&session, false).await?;

        self.conversations
            .lock()
            .unwrap()
            .entry(id)
            .or_insert(session);
        Ok(id)
    }

    /// Close an in-memory conversation session.
    pub fn close(&self, conversation_id: &[u8; 32]) -> bool {
        self.conversations
            .lock()
            .unwrap()
            .remove(conversation_id)
            .is_some()
    }

    /// Whether this runtime currently has the conversation open.
    pub fn contains(&self, conversation_id: &[u8; 32]) -> bool {
        self.conversations
            .lock()
            .unwrap()
            .contains_key(conversation_id)
    }

    fn session(&self, conversation_id: &[u8; 32]) -> Result<Arc<ConversationSession>, MurmError> {
        self.conversations
            .lock()
            .unwrap()
            .get(conversation_id)
            .cloned()
            .ok_or(MurmError::CabalNotFound)
    }

    /// The local signing identity inside one conversation.
    pub fn author_public_key(
        &self,
        conversation_id: &[u8; 32],
    ) -> Result<Ed25519PublicKey, MurmError> {
        Ok(self.session(conversation_id)?.keypair.public_key())
    }

    /// Clone the cabal's shared key-epoch history and drop protector.
    pub fn keyring(&self, conversation_id: &[u8; 32]) -> Result<CabalKeyring, MurmError> {
        Ok(self.session(conversation_id)?.keyring.clone())
    }

    /// Install a group-authorized encryption epoch without changing cabal id.
    pub fn install_key_epoch(
        &self,
        conversation_id: &[u8; 32],
        epoch: CabalKeyEpoch,
        key: [u8; 32],
    ) -> Result<(), MurmError> {
        self.session(conversation_id)?
            .keyring
            .install(epoch, key)
            .map_err(MurmError::from)
    }

    /// The shared LogSync/native-drop store for one open conversation.
    pub fn sync_store(
        &self,
        conversation_id: &[u8; 32],
    ) -> Result<MunimentStore<ConversationBackend, CabalExt>, MurmError> {
        Ok(self.session(conversation_id)?.store.sync_store())
    }

    /// Author a text message.
    pub async fn post_text(
        &self,
        conversation_id: &[u8; 32],
        channel: &str,
        text: &str,
        timestamp_ms: u64,
    ) -> Result<PostId, MurmError> {
        self.post_with_kind(
            conversation_id,
            PostKind::Text {
                channel: ChannelName::new(channel),
                text: text.to_owned(),
                timestamp_ms,
            },
        )
        .await
    }

    /// Author a channel topic.
    pub async fn post_topic(
        &self,
        conversation_id: &[u8; 32],
        channel: &str,
        topic: &str,
        timestamp_ms: u64,
    ) -> Result<PostId, MurmError> {
        self.post_with_kind(
            conversation_id,
            PostKind::Topic {
                channel: ChannelName::new(channel),
                topic: topic.to_owned(),
                timestamp_ms,
            },
        )
        .await
    }

    /// Author a channel join.
    pub async fn post_join(
        &self,
        conversation_id: &[u8; 32],
        channel: &str,
        timestamp_ms: u64,
    ) -> Result<PostId, MurmError> {
        self.post_with_kind(
            conversation_id,
            PostKind::Join {
                channel: ChannelName::new(channel),
                timestamp_ms,
            },
        )
        .await
    }

    /// Author a channel leave.
    pub async fn post_leave(
        &self,
        conversation_id: &[u8; 32],
        channel: &str,
        timestamp_ms: u64,
    ) -> Result<PostId, MurmError> {
        self.post_with_kind(
            conversation_id,
            PostKind::Leave {
                channel: ChannelName::new(channel),
                timestamp_ms,
            },
        )
        .await
    }

    /// Author profile information.
    pub async fn post_info(
        &self,
        conversation_id: &[u8; 32],
        entries: Vec<InfoEntry>,
        timestamp_ms: u64,
    ) -> Result<PostId, MurmError> {
        self.post_with_kind(
            conversation_id,
            PostKind::Info {
                entries,
                timestamp_ms,
            },
        )
        .await
    }

    /// Author a deletion request.
    pub async fn post_delete(
        &self,
        conversation_id: &[u8; 32],
        posts: Vec<PostId>,
        timestamp_ms: u64,
    ) -> Result<PostId, MurmError> {
        self.post_with_kind(
            conversation_id,
            PostKind::Delete {
                posts,
                timestamp_ms,
            },
        )
        .await
    }

    async fn post_with_kind(
        &self,
        conversation_id: &[u8; 32],
        kind: PostKind,
    ) -> Result<PostId, MurmError> {
        let session = self.session(conversation_id)?;
        let _ingest = session.ingest.lock().await;
        let mut head = session.author_head.lock().await;
        let (seq_num, backlink) = *head;
        let post = sign_post(
            &session.keypair,
            *conversation_id,
            seq_num,
            backlink,
            Vec::new(),
            kind,
        );
        let operation = post_to_operation(&post)?;
        let outcome = session.store.accept(&operation).await?;
        let post_id = PostId::new(*operation.hash.as_bytes());
        *head = (seq_num + 1, Some(post_id));
        drop(head);
        Self::materialize(&session, post, outcome);
        Ok(post_id)
    }

    fn materialize(session: &ConversationSession, post: Post, outcome: ProcessOutcome) -> bool {
        if !matches!(
            outcome,
            ProcessOutcome::Inserted { .. } | ProcessOutcome::HydratedPayload
        ) {
            return false;
        }
        let id = *hash_post(&post).as_bytes();
        let inserted = session
            .posts
            .lock()
            .unwrap()
            .insert(id, post.clone())
            .is_none();
        if inserted {
            let _ = session.events.send(post);
        }
        inserted
    }

    fn materializable(operation: &Operation<CabalExt>) -> bool {
        operation.header.payload_hash.is_none() || operation.body.is_some()
    }

    async fn rebuild(
        session: &ConversationSession,
        emit_added: bool,
    ) -> Result<ConversationRefresh, MurmError> {
        let mut operations = session.store.operations().await?;
        operations.sort_by(|left, right| {
            left.header
                .verifying_key
                .as_bytes()
                .cmp(right.header.verifying_key.as_bytes())
                .then_with(|| left.header.seq_num.cmp(&right.header.seq_num))
                .then_with(|| left.hash.as_bytes().cmp(right.hash.as_bytes()))
        });

        let local_author = session.keypair.public_key().to_bytes();
        let mut local_head = (0, None);
        let mut rebuilt = BTreeMap::new();
        for operation in operations {
            if *operation.header.verifying_key.as_bytes() == local_author
                && operation.header.seq_num >= local_head.0
            {
                local_head = (
                    operation.header.seq_num + 1,
                    Some(PostId::new(*operation.hash.as_bytes())),
                );
            }
            if Self::materializable(&operation) {
                let post = operation_to_post(&operation)?;
                rebuilt.insert(*operation.hash.as_bytes(), post);
            }
        }

        *session.author_head.lock().await = local_head;
        let (added_posts, removed) = {
            let mut current = session.posts.lock().unwrap();
            let added_posts: Vec<_> = rebuilt
                .iter()
                .filter(|(id, _)| !current.contains_key(*id))
                .map(|(_, post)| post.clone())
                .collect();
            let removed = current
                .keys()
                .filter(|id| !rebuilt.contains_key(*id))
                .count() as u64;
            *current = rebuilt;
            (added_posts, removed)
        };
        if emit_added {
            for post in &added_posts {
                let _ = session.events.send(post.clone());
            }
        }
        Ok(ConversationRefresh {
            materialized: session.posts.lock().unwrap().len() as u64,
            added: added_posts.len() as u64,
            removed,
        })
    }

    /// Reconcile history and subscriptions after out-of-band store mutation.
    pub async fn refresh(
        &self,
        conversation_id: &[u8; 32],
    ) -> Result<ConversationRefresh, MurmError> {
        let session = self.session(conversation_id)?;
        let _ingest = session.ingest.lock().await;
        Self::rebuild(&session, true).await
    }

    /// Import a native plaintext/public drop and immediately refresh views.
    pub async fn import_plain_drop<R: Read>(
        &self,
        conversation_id: &[u8; 32],
        reader: R,
        limits: DropLimits,
    ) -> Result<(DropImportReport, ConversationRefresh), MurmError> {
        let session = self.session(conversation_id)?;
        let _ingest = session.ingest.lock().await;
        let report = session.store.import_plain(reader, limits).await?;
        let refresh = Self::rebuild(&session, true).await?;
        Ok((report, refresh))
    }

    /// Import a cabal-protected native drop and immediately refresh views.
    pub async fn import_protected_drop<R: Read>(
        &self,
        conversation_id: &[u8; 32],
        reader: R,
        limits: DropLimits,
    ) -> Result<(DropImportReport, ConversationRefresh), MurmError> {
        let session = self.session(conversation_id)?;
        let _ingest = session.ingest.lock().await;
        let report = session
            .store
            .import_protected(reader, limits, &session.keyring)
            .await?;
        let refresh = Self::rebuild(&session, true).await?;
        Ok((report, refresh))
    }

    /// Ingest one signed operation from gossip, LogSync, or native drop.
    pub async fn ingest_operation(
        &self,
        conversation_id: &[u8; 32],
        operation: &Operation<CabalExt>,
    ) -> Result<PostId, MurmError> {
        let session = self.session(conversation_id)?;
        let _ingest = session.ingest.lock().await;
        let outcome = session.store.accept(operation).await?;
        let post_id = PostId::new(*operation.hash.as_bytes());

        if *operation.header.verifying_key.as_bytes() == session.keypair.public_key().to_bytes() {
            let mut head = session.author_head.lock().await;
            if operation.header.seq_num >= head.0 {
                *head = (operation.header.seq_num + 1, Some(post_id));
            }
        }
        if Self::materializable(operation) {
            let post = operation_to_post(operation)?;
            Self::materialize(&session, post, outcome);
        }
        Ok(post_id)
    }

    /// Ingest one logical post received from gossip.
    pub async fn ingest_post(
        &self,
        conversation_id: &[u8; 32],
        post: Post,
    ) -> Result<PostId, MurmError> {
        let operation = post_to_operation(&post)?;
        self.ingest_operation(conversation_id, &operation).await
    }

    /// One materialized post by id.
    pub fn get_post(&self, conversation_id: &[u8; 32], post_id: &PostId) -> Option<Post> {
        self.session(conversation_id)
            .ok()?
            .posts
            .lock()
            .unwrap()
            .get(post_id.as_bytes())
            .cloned()
    }

    /// Materialized posts in a channel, ordered by asserted time and post id.
    pub fn history(&self, conversation_id: &[u8; 32], channel: &str) -> Vec<Post> {
        let Ok(session) = self.session(conversation_id) else {
            return Vec::new();
        };
        let mut posts: Vec<_> = session
            .posts
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, post)| {
                post.kind
                    .channel()
                    .is_some_and(|name| name.as_str() == channel)
            })
            .map(|(id, post)| (*id, post.clone()))
            .collect();
        posts.sort_by(|(left_id, left), (right_id, right)| {
            left.kind
                .timestamp_ms()
                .cmp(&right.kind.timestamp_ms())
                .then_with(|| left_id.cmp(right_id))
        });
        posts.into_iter().map(|(_, post)| post).collect()
    }

    /// Subscribe to newly materialized posts.
    pub fn subscribe(
        &self,
        conversation_id: &[u8; 32],
    ) -> Result<broadcast::Receiver<Post>, MurmError> {
        Ok(self.session(conversation_id)?.events.subscribe())
    }
}
