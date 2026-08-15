# H0 WASM Product-Cone Receipt

**Date:** 2026-07-27  
**Plan:** [Graphshell reference host H0](../../../design_docs/mere_docs/implementation_strategy/2026-07-27_graphshell_reference_host_plan.md#h0-seal-the-wasm-product-cone)  
**Result:** complete in the live checkout

## Boundary landed

- Mere's facade is split into `graph`, `linked-data`, `canvas`, and
  `workbench` capabilities. Its default enables all four, preserving the
  incumbent Turnstone-facing facade.
- Graphshell's default `native` capability owns Notochord, Personae,
  `session-runtime`, transport, Tokio, stdio, and the existing receipt binaries.
- Graphshell's opt-in `web` capability selects Mere with only `graph` and
  `canvas`.
- Native Graphshell modules are compiled only for `native` on non-WASM targets.
- Cargo automatic binary discovery is disabled. All four Graphshell receipt
  binaries require `native`.
- The `mere-canvas` library remains portable. Its winit presenter and native
  dependencies require `native-present`.
- `scripts/check_port_boundaries.py` now walks the actual Graphshell web Cargo
  tree and rejects Turnstone, Servo application/runtime packages,
  `genet-winit-host`, and the graft/scry/weld embedding families.

## WASM receipts

Run with `CARGO_TARGET_DIR=target-plan-graphshell`, which is ignored by
`/target-*/`:

```powershell
cargo check -p chirograph -p graphshell-client --target wasm32-unknown-unknown
cargo check -p mere-canvas --target wasm32-unknown-unknown
cargo check -p graphshell --target wasm32-unknown-unknown --no-default-features --features web
python scripts/check_port_boundaries.py
```

All passed.

The dependency walk contains small Servo-derived utilities used by Genet's
portable layout stack: `servo-base`, `servo-config`, `servo-config-macro`,
`servo-malloc-size-of`, `servo-pixels`, and `servo-url`. The reverse path is
`genet-layout -> mere-canvas -> mere -> graphshell`; it does not include the
Servo application/runtime. The boundary checker permits those measured utility
crates and rejects the runtime packages explicitly.

The web cone contains none of:

- Turnstone;
- Servo's application, constellation, script, layout-thread, compositing,
  embedder, or WebDriver runtime packages;
- `genet-winit-host`;
- `grafting`, `scrying`, `welding`, `graft-engine`, `scrying-engine`, or
  `weld-engine`.

## Native compatibility receipts

```powershell
cargo check -p mere --all-features
cargo check -p mere-canvas --features native-present --bin canvas
cargo test -p graphshell
cargo test -p mere --all-features
cargo test -p mere-canvas --lib
```

All passed. Graphshell reported 44 passed, Mere reported 3 passed, and
`mere-canvas` reported 148 passed, all with 0 failed. The four explicitly
declared Graphshell receipt binaries and doc tests also built and ran their
empty test harnesses successfully.

## Focused hygiene receipts

```powershell
cargo clippy -p graphshell --target wasm32-unknown-unknown --no-default-features --features web --no-deps -- -D warnings
cargo clippy -p graphshell --no-deps -- -D warnings
cargo clippy -p mere --all-features --no-deps -- -D warnings
cargo fmt --check -p mere -p graphshell
git diff --check
```

All passed. The native Clippy pass exposed one collapsible conditional in the
G5 peer receipt; it was rewritten without changing the revocation condition.
Existing warning output in dependency crates and `mere-canvas` is outside these
focused `--no-deps` claims.

## Evidence boundary

This is a compile, dependency, unit-test, and native-integration receipt. It is
not a headed-browser receipt; H2 owns that proof.

The checkout uses its existing ignored `.cargo/config.toml` path patches for
local Genet, NetRender, and related working trees. This receipt proves the live
patched checkout. It does not prove patch-free clean-checkout resolution.
`Cargo.lock` is intentionally ignored in this library workspace, so the Cargo
commands above follow repository policy and make no `--locked` claim.
