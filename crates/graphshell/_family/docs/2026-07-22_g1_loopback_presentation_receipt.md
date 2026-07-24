# G1 loopback presentation receipt

**Status:** complete locally on 2026-07-22. Publication still waits for the
archived donor rename required by G0.

## What this proves

- One product-free `sceno::Scene` arrives through an in-memory endpoint.
- A Graphshell-owned sidecar binds scene instances to ordered presentation
  offers without adding resource references or bytes to Scenograph.
- Glyph and card payloads use small versioned codecs. Images remain ordinary
  content-addressed bytes.
- The client fetches resources independently, checks their BLAKE3 address and
  advertised byte size, and caches them per projection session.
- A rich client selects card and image. A compact client selects the glyph
  fallback and keeps an unsupported image useful as a labeled placeholder.
- Advertised actions survive both resolutions in the renderer-neutral
  accessibility tree and as ordinary keyboard-focusable buttons.

## Acceptance

- `cargo test --workspace`
- `cargo check --workspace --target wasm32-unknown-unknown`
- `cargo clippy --workspace --all-targets -- -D warnings`
- byte-for-byte comparison of fresh output with
  [`receipts/g1_loopback.html`](receipts/g1_loopback.html)
- headed browser inspection at 1440 × 1000 and 390 × 844: equal desktop
  columns, stacked narrow columns, four tabbable actions, decoded map image,
  zero horizontal overflow, and zero browser warnings or errors

## Deliberate limits

The buttons expose advertised intents but G1 does not invoke them. Scene diffs,
resume, and persistence policy are exercised separately by the
[G2 receipt](2026-07-22_g2_diff_resume_receipt.md). Authenticated carriers,
product adapters, rendered NetRender fragments, and live panes remain later
proofs. The receipt view is native semantic HTML; composing the application
through Genet and Cambium is also later host work.
