# Graphshell receipts

`g1_loopback.html` is generated from the real G1 loopback endpoint, client
state machine, capability resolver, and native HTML view:

```powershell
$env:CARGO_TARGET_DIR = 'target-proof'
cargo run -p graphshell --bin g1_receipt -- ports/graphshell/docs/receipts/g1_loopback.html
```

The workspace test suite compares fresh output byte-for-byte with the committed
receipt. Inspect it at desktop and narrow widths before updating the file.

`n4_owner_policy.html` is generated from Notochord's real owner-policy model,
the persona-scoped session-runtime store, and Graphshell's host-neutral
settings projection:

```powershell
$env:CARGO_TARGET_DIR = 'target-proof'
cargo run -p graphshell --bin n4_policy_receipt -- ports/graphshell/docs/receipts/n4_owner_policy.html
```

Its test regenerates the scenario and compares the result byte-for-byte. The
scenario disables Murm without changing Graphshell or transit, then disables
transit without changing either service rule, and finally reloads the policy
from disk.

`h3_browser_receipts.json` records the standalone local graph product workflow
at 1280×800 and 600×800. Its sibling screenshots show the wide and repaired
narrow product layouts after selected-subgraph export. The corresponding
[H3 receipt note](../2026-07-28_h3_local_graph_product_receipt.md) states the
portable file-facet and verification boundaries.

`h4_identity_surface.html` is generated from a resident `PersonaeHost` with a
real pending per-use SSH signing request. The paired
`h4_identity_receipt.json` records the typed approval intent, key generation,
direct native import, confirmed removal, an isolated real SSH wire exchange,
secret-exclusion assertions, and the deliberate decision to leave the standard
agent endpoint unchanged:

```powershell
$env:CARGO_TARGET_DIR = 'target-proof'
cargo run -p graphshell --bin h4_identity_receipt -- ports/graphshell/docs/receipts
```

The corresponding
[H4a authority note](../2026-07-28_h4a_personae_authority_receipt.md) and
[H4b key-management note](../2026-07-28_h4b_ssh_key_management_receipt.md)
cover the authority and SSH wire slices. The
[H4c admitted-endpoint note](../2026-07-28_h4c_admitted_identity_endpoint_receipt.md)
records the portable-client read and approval path. The
[H4d browser-carrier note](../2026-07-28_h4d_browser_native_carrier_receipt.md)
adds the real Chromium extension, native host, transcript-bound admission,
headed identity cards, a real headed per-use approval and signature, and clean
session close. The
[H4e native-import note](../2026-07-28_h4e_native_key_import_receipt.md)
adds the real Windows file picker and password dialog, encrypted-key import,
user-selected unlock policy, native-dialog re-entry guard, public-card refresh,
and secret-exclusion checks. The
[H4f resident-device-host note](../2026-07-28_h4f_resident_device_host_receipt.md)
adds the one-vault resident process, vault-free browser relay, isolated and
live standard-agent restart signatures, admitted browser relay proof, and
reversible Windows lifecycle installer. Remote login and real logon startup
remain open because the known laptop was offline. The interim task was restored.
The
[H4g carry-mutation and mixed-scene note](../2026-07-28_h4g_carry_mutation_mixed_scene_receipt.md)
adds confirmed delegated-device revocation through the live carry authority
and proves explicitly pinned public identity projections alongside access
history in one persisted and reopened Mere scene. The
[H4h live-cutover and Firefox note](../2026-07-28_h4h_live_cutover_firefox_receipt.md)
records the installed Windows task, same-fingerprint crash replacement,
intentional failed-update rollback, headed Firefox admission with ten public
cards, and clean carrier close. The retained Personae task is disabled but not
retired because logon/reboot and remote-login receipts remain open.

The
[H5a browser storage and capture-core note](../2026-07-28_h5a_browser_storage_capture_core_receipt.md)
records the Graphshell-local IndexedDB backend, consented and redacted history
pipeline, LocalOnly typed AccessRecord authority, restart dedupe, filtering,
forget behavior, durable extension delivery queue, and the real Chromium
permission/import/restart receipt. H5a left headed live intake, Firefox, and
the final controls open. The
[H5b cross-browser capture and controls note](../2026-07-28_h5b_cross_browser_capture_controls_receipt.md)
closes those walls with exact-package Chromium and Firefox receipts, portal
filtering and scoped forgetting, and the explicit Personae attribution seam.

The
[H6a G5f prerequisite note](../2026-07-29_h6a_g5f_prerequisite_receipt.md)
records a real p2panda/QUIC suspend, redial, contiguous-diff resume, accepted
intent, and intent-first revocation refusal across separate local processes.
The
[H6b physical G5f closure note](../2026-07-29_h6b_physical_g5f_closure_receipt.md)
repeats the combined run between Windows and Q-PC and closes G5. The
[H6c transfer-core note](../2026-07-29_h6c_transfer_core_receipt.md)
records the versioned selection engram, independently verified blobs,
replicate/copy identity rules, policy-carried access history, destination
AccessRecords, typed receipt, revoked-before-mutation guard, and resumable
two-store proof. The
[H6d physical transfer closure note](../2026-07-29_h6d_physical_transfer_closure_receipt.md)
records the real Windows-to-Q-PC manifest/blob transfer, fresh-admission
resume, preserved ids/tags/relation, destination AccessRecords, typed receipt,
and intent-first live revocation. H6 is complete. The
[H4i remote SSH login note](../2026-07-29_h4i_remote_ssh_login_receipt.md)
proves the same resident Graphshell authority authenticated a batch-mode
OpenSSH login to Q-PC through the standard agent endpoint.

The
[H7a personal-sync core note](../2026-07-29_h7a_personal_sync_core_receipt.md)
records the selected, secret-free Graphshell event grammar, shared
Stickleback intake boundary, two-peer LogSync convergence, retained
per-device access chronology, and explicit concurrent scalar conflict. H7
remains open for durable reopen, resident-host/browser wiring, a physical
two-device receipt, and blob availability.
