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
