# Graphshell H1 Local Mere Host Receipt

**Date:** 2026-07-27  
**Scope:** H1 of the Graphshell reference-host plan

## Result

Graphshell now hosts local Mere graph truth in its `web` profile. The adapter
stays in `ports/graphshell`; the portable protocol, client, and endpoint crates
remain independent of Mere and identity.

The composed host includes:

- Mere graph, relation, tag, address, and unknown-forward facet truth;
- a selected public persona/profile reference injected at construction;
- Muniment JSON-slot persistence over an injected `Backend`;
- an in-process `ProjectionCatalog`, `ProjectionSource`,
  `PresentationSource`, and `IntentSink`;
- Mere canvas placement lowered into Scenograph scene and relation vocabulary;
- portable card resources and configurable handler offers;
- strict `graphshell.address.open/v1` payload validation;
- durable access records after an accepted open;
- one `ClientState` mounting the local Mere scene and a remote G1 scene.

Private keys, vault handles, signing authority, and launch commands are absent
from the portable host state. Native Personae and application-launch adapters
remain later composition work.

## Deterministic fixture

The fixture contains:

- HTTPS, I2P-style custom-protocol, and file addresses;
- durable tags and semantic, containment, arrangement, and provenance
  relations;
- a saved Scenograph score;
- access records from a laptop and phone;
- a mounted remote-projection reference and live remote client mount;
- `future.graphshell.transport-route/v7`, an unknown facet preserved exactly;
- synthetic public persona, two device, SSH key-reference, grant, and signing
  receipt projections.

The test mounts local and remote scenes, invokes the advertised typed open
intent, observes the third access record and a new projection revision,
persists, reopens, remounts, checks the unknown facet, and compares the
unchanged persisted boundary bytes.

## Persistence finding

Reconstructing a live Mere graph may normalize internal ordering. Regenerating
JSON immediately after reopen therefore need not reproduce the original bytes
even when graph and facet meaning is unchanged.

The host now retains the loaded boundary document while clean. A graph or facet
mutation marks it dirty and produces a fresh document; an unchanged reopen and
save writes the retained document exactly. This preserves Mere's normalized
live model and gives the host a byte-stable unchanged-save contract.

## Verification

All commands used the ignored `target-plan-graphshell` proof target.

```powershell
cargo test -p graphshell --no-default-features --features web
```

> **2026-09-01.** On windows-msvc this bare invocation no longer links
> (`LNK1120`, one unresolved `drop_glue<CartographyGeometry>` from an
> incremental `mere-canvas`); run it as `cargo test-web`, the alias in
> `.cargo/config.toml.example`, which adds
> `--config profile.dev.package.mere-canvas.incremental=false`. Cause, cost and
> retire condition are on the alias. 39 tests pass under it as of that date.

Result: 10 passed, 0 failed. This includes the H1 load, project, typed-intent
mutation, persistence, reopen, remote remount, unknown-facet, and byte-equivalent
unchanged-save proof.

```powershell
cargo test -p graphshell
cargo test -p graphshell --all-features
```

Result: 44 passed, 0 failed for the incumbent native Graphshell profile, and
45 passed, 0 failed for the combined native and web feature set.

```powershell
cargo check -p graphshell --all-features
cargo clippy -p graphshell --all-features --no-deps -- -D warnings
```

Result: both passed. Focused Graphshell code is warning-denying clean; existing
dependency warnings remain outside that claim.

```powershell
cargo check -p graphshell --target wasm32-unknown-unknown --no-default-features --features web
python scripts/check_port_boundaries.py
```

Result: both passed. The dependency checker reported:

```text
Mere port and Graphshell web dependency boundaries passed
```

This checkout uses the repository's ignored local Cargo patches, so the result
proves the live patched checkout. `Cargo.lock` is ignored here; this is not a
locked clean-checkout claim.

## Acceptance boundary

H1 proves the portable local host and deterministic data flow. It does not
prove a headed browser surface, OPFS, native installers, system application
launch, or live Personae vault interaction. H2 owns the first useful headed
host.
