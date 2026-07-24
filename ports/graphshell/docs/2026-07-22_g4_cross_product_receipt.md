# G4 cross-product session receipt

Date: 2026-07-22

## Claim

One product-neutral Graphshell process discovers and mounts Merecat and
Isometry projections through the same protocol and client state machine.
Neither product appears in Graphshell's dependency graph.

## Executed proof

The three binaries were built independently:

```powershell
# repos/graphshell
cargo build -p graphshell --bin g4_sessions

# repos/merecat
cargo build --bin graphshell_endpoint

# repos/isometry
cargo build -p isometry-graphshell --bin isometry_endpoint
```

The host was then run against both endpoint processes:

```powershell
g4_sessions.exe docs/receipts/g4_session_switch.html `
  graphshell_endpoint.exe `
  isometry_endpoint.exe
```

The process reported:

```text
mounted 3 sessions from 2 endpoints into docs\receipts\g4_session_switch.html
```

The committed receipt contains:

- Merecat: one browsing-graph session with three items and two routed
  relations;
- Isometry: one player-overmap session with three places and two routed
  relations;
- Isometry: one tile-board session with four placed tiles;
- accepted local curation actions from both products;
- rejected Merecat and Isometry product actions with endpoint-owned reasons.

Graphshell's workspace tests, native warning-denying Clippy gate, and Wasm
check pass. Isometry's two endpoint tests and warning-denying `--no-deps`
Clippy gate pass. Merecat's three focused endpoint tests and endpoint-binary
build pass; its wider dependency graph retains existing warnings.

## Dependency direction

`cargo tree -p graphshell` contains neither Merecat nor Isometry. Product
adapters depend on Graphshell's protocol, endpoint, and local-carrier crates in
their own repositories.

## Remaining boundary

This proves local process discovery, mounting, resources, intents, and session
switching. The carrier has no authenticated handshake or cross-device
transport. Those remain G5 work. A headed interaction check also remains to be
run because this execution environment exposed no controllable browser.
