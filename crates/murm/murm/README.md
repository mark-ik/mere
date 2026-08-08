# murm

Direct peer-to-peer conversation for the [mere](https://crates.io/crates/mere)
browser. A *cabal* is an invitation-scoped conversation between known peers,
addressed by a 32-byte shared secret. Murm owns the post grammar, the signed
per-author log, admission of received operations, per-cabal storage, and the
sync lanes.

Product surfaces call a conversation a *murmur*; `Cabal` is the protocol and
code noun. `mere/cable/v1` names the inherited cabal/channel/post grammar and
the ALPN it claims. It does not assert wire compatibility with cabal-club
Cable. Many-to-many federation lives in `gemot`, not here.

## Public surface

Every module is private; `src/lib.rs` re-exports the whole surface, so a caller
writes `murm::CabalHandle`. The `Source` column names the source file.

| Source | Contents |
| --- | --- |
| `cabal` | `CabalKey`, `CabalId`, `CabalHandle`, `CabalMembership` |
| `conversation_engine` | `ConversationEngine`, `ConversationRefresh`; the runtime behind every handle |
| `conversation_store` | `ConversationStore<B>`, `ConversationStoreError`; the per-cabal store over stickleback |
| `conversation_backend` | `ConversationStorage`, `ConversationBackend` (memory or redb) |
| `drop_export` | `ConversationDropSelector`, `ConversationDropProfile`, `ConversationDropPrivacy`, `ConversationDropPriorities`, `ConversationFrontier` |
| `key_epoch` | `CabalKeyring`, `CabalKeyEpoch`, `CabalKeyringError`, `CABAL_DROP_SUITE` |
| `post` | `Post`, `PostId`, `PostKind`, `ChannelName`, `InfoEntry` |
| `post_sign` | `sign_post`, `verify_post` |
| `post_hash` | `hash_cabal_id` (`cabal_id = BLAKE3-256(cabal_key)`) |
| `post_wire` | `CabalExt`, `encode_post`, `decode_post`, `operation_id`, `post_to_operation`, `operation_to_post` |
| `gossip_sync` | `SyncedCabal`, and `subscribe_cabal` on `Murm<P2pandaTransport>` |
| `session_lane` (feature) | `serve_session`, `serve_accepted_session`, `push_posts`, `lane_binding`, `Admission`, `SessionOutcome`, `MAX_POST_FRAME` |
| `error` | `MurmError` |

`lib.rs` itself defines `Murm<T>`, `hash_post`, and the `VERSION` / `STAGE`
consts, and re-exports `Ed25519PublicKey` and `IdentityProvider` from identity
and `Alpn`, `PeerID`, `Transport` from transport.

## Key entry points

| Item | Signature / role |
| --- | --- |
| `Murm::new(Arc<dyn IdentityProvider>, T)` | In-memory storage. |
| `Murm::with_storage(identity, transport, ConversationStorage)` | `ConversationStorage::Memory` or `ConversationStorage::redb(dir)`, one redb file per cabal. |
| `Murm::open_cabal(&CabalKey) -> Result<CabalHandle, MurmError>` | Opens or rejoins. A durable open rebuilds history and the local author head before returning. Idempotent per key. |
| `Murm::derive_cabal_keypair(&CabalKey) -> Result<identity::Ed25519Keypair, MurmError>` | `child = BLAKE3(master_seed \|\| cabal_key)`; the master secret stays in the identity provider. |
| `Murm::local_peer_id() -> PeerID` | Derived from the identity provider's master public key. |
| `Murm::conversation_engine() -> &Arc<ConversationEngine>` | The runtime, for operations `CabalHandle` does not expose. |
| `Murm::<P2pandaTransport>::subscribe_cabal(&CabalKey) -> Result<SyncedCabal, MurmError>` | Joins the cabal's gossip topic and its LogSync overlay topic. |

`CabalHandle` authoring: `send_text`, `send_topic`, `send_join`, `send_leave`,
`send_info`, `send_delete`, each with an `_at` variant taking an explicit
timestamp. Reads: `id`, `author_public_key`, `get_post`, `history(channel)`,
`membership(channel) -> CabalMembership`, `subscribe() ->
broadcast::Receiver<Post>`. Ingress: `ingest_post`, `import_plain_drop`,
`import_protected_drop`, `refresh`. Keys: `keyring`, `install_key_epoch`.

`SyncedCabal` wraps a `CabalHandle` plus the two lanes over a
`P2pandaTransport`. Authored posts broadcast on gossip; peers that were offline
catch up over LogSync (RBSR) against the same muniment store. Both lanes ingest
idempotently, so a post arriving on both lands once. It exposes `send_text`,
`history`, `subscribe`, `handle`, `sync_status`, and `resync`.

`CabalKeyring` protects native drops with epoch keys under `CABAL_DROP_SUITE`
(XChaCha20). `install` adds an epoch and keeps the `CabalId` stable;
`forget_before` drops older epochs.

## Features

| Feature | Effect |
| --- | --- |
| `session-lane` (off) | Enables `session_lane`: an owner-gated point-to-point lane carrying posts over a stream a carrier already opened. Pulls in `notochord` and `transport/notochord`. |

## Dependencies

| Crate | Why |
| --- | --- |
| `identity` (`personae`) | `IdentityProvider::derive_keypair` for the per-cabal keypair; `Ed25519PublicKey`. |
| `transport` (`mere-transport`) | The `Transport` trait `Murm<T>` is generic over; `P2pandaTransport` and `GossipHandle` for the sync lanes. |
| `stickleback` | `MunimentStore`, `OperationProcessor`, `JoinedSpace`, and the native-drop codec and export/import path. |
| `muniment` (`redb`) | `MemoryBackend` / `RedbBackend` under `ConversationStore`. |
| `p2panda-core` | `Operation`, `Topic`, header hashing; the stored form of a post. |
| `p2panda-encryption` | XChaCha20 AEAD behind `CabalKeyring`. |
| `notochord` (optional) | Session admission for `session-lane`. |
| `blake3` | `hash_cabal_id`. |
| `tokio`, `tokio-stream` | Sync tasks, the `subscribe` broadcast channel, gossip streams. |

LogSync assembly is not done here; `stickleback::JoinedSpace` owns that
ceremony, so `p2panda-net` and `p2panda-sync` are not direct dependencies.

## Status

Pre-1.0 (`STAGE = "pre-alpha"`). Cabal lifecycle, keypair derivation, the
`CabalHandle` send / history / subscribe surface, durable redb storage with
reopen reconstruction, and native drop import with materialization refresh are
in place. Two peers on the same cabal key converge over a `P2pandaTransport`
(tested). The host wires a networked cabal into mere's Comms pane end to end
(join ticket, connect-by-ticket, live post drain).

Ahead: host-led co-op sessions (`host_coop` / `join_coop`), cross-author causal
links, retention-aware pruning, and group key distribution. Native drops have
epoch-aware protection today; authorization, epoch distribution, and persisted
key history still belong to personae or a p2panda group-state adapter.

## License

MIT OR Apache-2.0.
