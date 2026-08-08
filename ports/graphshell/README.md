# graphshell (port)

The reference application over [`crates/graphshell`](../../crates/graphshell).
The stack crates carry sessions; this port decides who gets one, runs the serve
loops, hosts resident endpoints, and renders projections.

Package name `graphshell`. It re-exports the stack as `graphshell::protocol`,
`graphshell::client`, and `graphshell::endpoint`.

## Modules

Always compiled:

| Module | Contents |
|---|---|
| `view` | `ProjectionReceiptView`, `ProjectionLayoutView`, `ScenePlacementView`, `SceneRelationView`, `IntentReceiptView`, `render_projection_receipt`, `render_g1_receipt`, `render_canary_html` |
| `canary` | `FixtureEndpoint`, `CanaryRun`, `CanaryError`, `run_loopback_canary` |
| `resume` | `ResumeFixtureEndpoint`, `run_resume_canary` |
| `sessions` | `SessionProjectionView`, `render_session_switch_receipt`; under `native`, `spawn_endpoint_session` and `mount_endpoint_processes` |

Under `native` (non-wasm):

| Module | Contents |
|---|---|
| `admission` | `GRAPHSHELL_DOMAIN`, `PROJECTION_SERVICE`, `CONNECT_ACTION`, `PROJECTION_PROTOCOL`, `open_session`, `admit_session`, `serves_action` |
| `carrier` | `projection_alpn`, `projection_policy`, `accept_projection_session`, `ProjectionRefusal`, `ProjectionAcceptError` |
| `network_carrier` | `projection_binding`, `dial_projection_session`, `DialError`; re-exports `NetworkCarrier` and `CarrierRuntime` |
| `session_loop` | `serve_admitted_session`, `SessionEnd`, `SessionSummary`, `SessionLoopError` |
| `session_notices` | `serve_admitted_session_notifying` |
| `lifecycle` | `SessionAuthority`, `AdmittedEndpointContext`, `BindAdmittedSession`, `Lapse`, `ScoreDenial`, `adjudicate_intent`, `apply_lapse` |
| `browser_carrier` | WebExtensions native messaging: `NATIVE_HOST_NAME`, `CHROMIUM_EXTENSION_ID`, `FIREFOX_EXTENSION_ID`, `AllowedExtensions`, `BrowserLauncher`, `BrowserChallenge` |
| `identity` | Secret-free read model: `VaultView`, `ProfileView`, `SshKeyView`, `DeviceView`, `DeviceGrantView` |
| `identity_endpoint` | `SupplementalCard`, `TransferAcceptIntentV1`, `TransferDecision`, `IdentityEndpointError` |
| `identity_projection` | Identity cards plus the signing, SSH generate, and SSH import intents |
| `policy_projection` | `PolicySettingsView`, `run_n4_policy_scenario`, `render_n4_policy_receipt` |
| `profile` | `PROFILE_ENV` (`GRAPHSHELL_PROFILE`), `selected_profile`, `default_vault_dir`, `GraphshellIdentity` |
| `native::endpoint_catalog` | `ResidentEndpointCatalog`, `ResidentEndpoint`, `ResidentEndpointRoute`, `ResidentEndpointSession` |
| `native::projection_host` | `ResidentProjectionHost`, `ServedProjection` |
| `native::personae_host` | Resident Personae authority; SSH key mutation receipts |
| `native::browser_host` | `serve_identity_native_messages`, `serve_catalog_native_messages` |
| `native::device_broker` | `DeviceSurface`, `DEVICE_ENDPOINT_ENV` (`GRAPHSHELL_DEVICE_ENDPOINT`), `configured_device_endpoint` |
| `native::identity_ui` | `NativeIdentityUi`, `SystemNativeIdentityUi`, `apply_native_identity_action` |
| `native::owner_settings` | App directory, data root, per-profile settings file |

Under `web` (portable, builds for `wasm32-unknown-unknown`):

| Module | Contents |
|---|---|
| `app` | `GraphshellApp`, `AppError`: local and remote projections through one `ClientState` |
| `mere_host` | `MereHost`, `SelectedPersonaRef`: Mere graph truth as a Graphshell endpoint |
| `product` | Graph-product operations and facets: `TransferScope`, `RelationFamilyFilter`, `EditableRelation` |
| `handlers` | `HandlerRegistry`, `HandlerOffer`, `OpenAddressV1` |
| `access` | `AccessRecord`, `AccessObservation`, `AccessTransition`, `save_access_record` |
| `capture` | `HistoryCapturePolicy`, `BrowserVisit`, `NormalizedVisit`, `CaptureBackend` |
| `browser_storage` | `StoragePersistence`, `decide`, `status_line` |
| `transfer` | `TransferRequest`, `TransferRouteV1`, `TransferAuthorization`, manifest and content facets |
| `transfer_endpoint` | `TransferSourceEndpoint`, `TransferBeginV1` |

Under `personal-sync`:

| Module | Contents |
|---|---|
| `personal_sync` | `PersonalGraphEvent`, `PersonalGraphRecord`, `PersonalGraphExt`, `PersonalEncryption` |
| `transfer_offer` | `TransferOfferV1`, `transfer_offer_rule`, `offer_address` |
| `native::personal_sync_host` | `PersonalSyncHost`, `PersonalSyncHostConfig` |
| `native::device_sync` | `personal_graph_id`, `SeedNote`, `BlobAction`, `resolve_data_root` |
| `native::pairing` | `pair_device`, `unpair_device`, `PairingFacts` |
| `native::graph_keys` | `GraphKeyGroup`, `OpenedKeyGroup`, `AbsorbReport` |
| `native::transfer_staging` | `offer_transfer`, `receive_transfer`, `released_blobs_for` |

## Features

| Feature | Scope | Key dependencies |
|---|---|---|
| `native` (default) | Admitted sessions, native transports, Personae composition, the binaries | `graphshell-network`, `graphshell-stdio`, `notochord`, `transport`, `personae`, `session-runtime`, `ssh-agent-lib`, `ssh-key`, `light-file-dialog`, Tokio |
| `web` | The portable graph and canvas cone | `mere` (`graph`, `canvas`), `chartulary`, `eidetic`, `muniment`, `url`, `sha2` |
| `personal-sync` | `native` + `web` + device synchronization | `muniment/redb`, `p2panda-core`, `p2panda-store`, `stickleback` |

Crates under `crates/` may not depend on this package.
`scripts/check_port_boundaries.py` enforces that, checks that `graphshell` has
exactly one manifest, and checks that the `web` cone's dependency tree excludes
Turnstone, Genet host packages, and Servo runtime packages.

## Binaries

| Binary | Required features |
|---|---|
| `g1_receipt` | `native` |
| `g4_sessions` | `native` |
| `g5_peer` | `native` |
| `n4_policy_receipt` | `native` |
| `h4_identity_receipt` | `native` |
| `h4d_browser_receipt_host` | `native` |
| `h4e_browser_import_receipt_host` | `native` |
| `h4f_agent_probe` | `native` |
| `graphshell_native_host` | `native` |
| `h6_transfer_peer` | `native`, `web` |
| `h7_sync_peer` | `personal-sync` |
| `graphshell_device_host` | `personal-sync` |

## Browser surfaces

`web/Cargo.toml` is a separate package, `graphshell-web`, whose `[lib] path` is
`../src/web.rs`. It is a `cdylib` composing this port's `web` cone with Mere
Canvas, Cambium over Genet's DOM and layout seam, and NetRender over a WebGPU
canvas. Its browser dependencies stay out of the `graphshell` package.

[`web/extension`](web/extension) holds the Chromium and Firefox extension and
the `org.mere.graphshell` native-messaging registration.

## Where to look next

[`docs/`](docs) holds the dated receipt notes for each landed slice.
[`docs/receipts`](docs/receipts) holds the committed receipt artifacts and the
commands that regenerate them.
