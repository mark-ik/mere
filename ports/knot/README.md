# Knot

Knot is Mere's files-in-place authoring port. It serves a Graphshell
projection over a real directory or a sealed personal vault, so a host can
mount documents without owning the source files or the vault keys. File bytes
stay on disk; containers carry `file:` references, titles, media types, and
facets.

## Modules

`src/lib.rs` re-exports everything below.

| Module | Contents |
|---|---|
| `content_classes` | `KnotContentClasses` (`registry`, `validator`), `FILE_CLASS` (`knot.file`), `NOTE_CLASS` (`knot.note`), `FILE_DOCUMENT_FACET`, `NOTE_DOCUMENT_FACET`. Facet schemas are built with `eidetic::MereNativeSchemaBuilder` and registered in a `session_runtime::SchemaFacetValidator`. |
| `directory` | `DirectorySource`, `DiskDocument`, `IgnorePolicy`. Discovery keyed by filesystem identity (volume + file index on Windows, device + inode on Unix, path as fallback), so a container and its facets survive rename. |
| `endpoint` | `KnotEndpoint`, `KnotWriteGrant`, `KnotEffectAuthority`, `KnotEffectFetcher`, `KnotEffectMode`, `KnotEffectPolicy`. Constructors: `open`, `open_writable`, `open_with_identity`, `open_writable_with_identity`, `fixture`, `from_vault`, `from_synced_vault`, `from_communal_vault`. Grants are revocable at runtime: `revoke_watcher`/`grant_watcher`, `revoke_writes`/`grant_writes`, `revoke_effects`/`grant_effects`, `lock_vault`/`unlock_vault`. |
| `watcher` | `DirectoryWatcher`. A `notify` recursive watch whose queued events collapse into one attributed Servitor journal transition under a revocable grant on the `watch` scope. |
| `writer` | `AuthoredFile`, `DocumentFormat` (`Knot`, `Markdown`, `Djot`, `Json`), `SaveOutcome`. Fixed-point writing: untouched files are not rewritten. |
| `editor` | `KnotEditor`, `EditOutcome`. Cambium's `TextInput` owns the sole source buffer; highlights, outline, folds, and preview are derived by `knot_editor_host::KnotReadout`. |
| `vault` | `KnotVault`, `VaultDocument`. Sealed document store on `personae::SealedRecordStorage`, with the Sibylla search index sealed in the same store. |
| `search` | `KnotSearch`, `SearchConfig`, `SearchHit`, `SearchLane` (`Disk`, `Vault`). Sibylla lexical index over both lanes under separate Servitor caps, `knot/search/disk` and `knot/search/vault`. |
| `sync` | `KnotSyncStore<B>` and its `KnotSyncFileStore` redb alias, `KnotSyncEvent`, `KnotSyncExt`, `KnotSyncCipher`, `KnotSyncError`, `KnotDocumentProjection`, `KnotDocumentVersion`, `KnotDocumentConflict`, `KnotAutomaticTextMerge`, `KnotEncryptionProfile` (`PersonalVaultV1`, `CommonsDataV1`), `KNOT_COMMONS_ENCRYPTION_PROFILE`, `KnotCheckpointSnapshot`, `KnotProjectionCheckpoint`, `KnotEpochExecutionReceipt`, `KnotOfflineMemberEpochHold`, `KnotOfflineMemberRecovery`, `KnotTailReceipt`. Signed causal events over Stickleback and p2panda; the projection reports same-document conflicts and, where two concurrent versions share a base, a clean three-way text merge. |
| `resident` | `KnotSyncHost`, `KnotSyncHostConfig`, `KnotSyncHostError`. Keeps a persona's space joined over `transport::P2pandaTransport` without serving a projection. |
| `settings` | `KnotSettings`, `KnotSyncSettings`, `KnotSettingsError`, `knot_settings_path`, `hex32`, `parse_hex32`. Per-persona `knot-sync.json` holding paired writer keys, relay urls, and peer hints. |
| `startup` | `StartupUnlockedPersonalVault`, `local_device_root`, `persona_vault_root`. Recovers the persona epoch through session-runtime's wallet and derives the vault key, space id, and device-distinct writer key. |

## Binaries and example

| Target | Invocation |
|---|---|
| `knot_endpoint` | Serves over stdio with `graphshell_stdio::serve_resumable_notifying`. Modes: no argument (the deterministic K0 fixture), `[directory]`, `directory <root>`, `directory-write <root> <max-source-bytes>`, `directory-write-effects <root> <max-source-bytes> <resolve-mode> <run-mode> <schemes> <languages> <max-depth> <max-ops>`, `persona-vault <data-root> <persona-id> <max-source-bytes>`, `persona-vault-effects ...`, `communal-fixture-effects ...`. Effect modes are `auto`, `ask`, `never`. |
| `knot_sync_host` | `knot_sync_host <data-root> <persona-uuid> [--label <name>] [--log-file <path>]`. Management verbs exit after reporting: `--pair-writer <64-hex>`, `--unpair-writer <64-hex>`, `--pairing-facts`. |
| `examples/k2_peer.rs` | Two-machine rehearsal for a place-held document. `cargo run -p knot --example k2_peer -- hold --root <vault-dir>` on the holder; `visit --peer <ticket>` or `visit --discover` on the visitor. Env: `K2_OWNER`, `K2_SEED`, `K2_NETWORK`, and `K2_PEER` for `--discover`. It is an example rather than a bin because it uses the `graphshell` dev-dependency. |

Integration tests: `tests/place_projection.rs`, `tests/revision_bell.rs`,
`tests/send_probe.rs`.

## Dependencies

- Disclosure: `graphshell-endpoint`, `chirograph`, `graphshell-stdio`,
  `sceno`, `scenotime`.
- Graph and schema: `chartulary`, `eidetic` (`json-schema`), `session-runtime`,
  `servitor`, `proofs`.
- Documents: `inker`, `nematic`, `illume`, `cambium`, `knot-editor-host`,
  `similar`.
- Storage and identity: `muniment` (`redb`), `personae`, `zeroize`.
- Sync: `stickleback`, `transport`, `p2panda-core`, `p2panda-net`,
  `p2panda-store` (all 0.7).
- Search: `esp::embed`.
- Effects: `fetch`, `script-rhai`, `url`.
- Filesystem and platform: `notify` 8, and `windows-sys` on Windows for file
  identity.
- Dev-dependencies: `graphshell` (path `../graphshell`), `notochord`,
  `tempfile`.

## Plans

- [Knot port plan](../../design_docs/mere_docs/implementation_strategy/2026-07-25_knot_port_plan.md)
- [Knot authoring consumer plan](../../design_docs/mere_docs/implementation_strategy/2026-07-27_knot_authoring_consumer_plan.md)
- [Knot in Graphshell plan](../../design_docs/mere_docs/implementation_strategy/2026-08-02_knot_in_graphshell_plan.md)
