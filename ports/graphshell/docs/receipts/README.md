# Graphshell receipts

`g1_loopback.html` is generated from the real G1 loopback endpoint, client
state machine, capability resolver, and native HTML view:

```powershell
$env:CARGO_TARGET_DIR = 'target-proof'
cargo run -p graphshell --bin g1_receipt -- docs/receipts/g1_loopback.html
```

The workspace test suite compares fresh output byte-for-byte with the committed
receipt. Inspect it at desktop and narrow widths before updating the file.
