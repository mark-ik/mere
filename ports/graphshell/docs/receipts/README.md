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
`h4_identity_receipt.json` records the typed approval intent, 64-byte signature,
secret-exclusion assertions, and the deliberate decision to leave the standard
agent endpoint unchanged:

```powershell
$env:CARGO_TARGET_DIR = 'target-proof'
cargo run -p graphshell --bin h4_identity_receipt -- ports/graphshell/docs/receipts
```

The corresponding
[H4a receipt note](../2026-07-28_h4a_personae_authority_receipt.md) lists the
remaining endpoint, restart, browser-admission, and lifecycle gates.
