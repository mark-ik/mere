# Djinn Family Resident Services Plan

**Status:** planned, gated on the `djinn 0.0.2` release
**Date:** 2026-08-22

**Related:**

- [auto-update brief](../../2026-07-22_auto-update_brief.md)
- [browser WebRTC carrier](2026-08-25_browser_webrtc_carrier_plan.md)
- [browser/native carrier research](../research/2026-08-25_browser_native_webrtc_carrier_probe.md)

## 1. Decision

Djinn is the product-neutral desktop resident for the Merely family. It owns
the lifetime and local projection of device services that must work while no
application is running. Applications render those services and send intents;
they do not each carry background polling, discovery, notification, or update
loops.

The resident composes domain crates rather than absorbing their semantics:

| Concern | Owner |
|---|---|
| Process lifetime, scheduling, installed-app inventory, local projections | Djinn |
| Update policy grammar, signed feeds, verified staging, platform apply | Luggage |
| Distillation, cleanup, and facet proposals | Athanor |
| Notification records and delivery rules | Redshank |
| Authenticated streams and DNS-SD mechanics | Murm |
| Local caller identity and durable secret authority | Personae and Castellan |
| Firmware compatibility and flashing | Linkboy |

Djinn contains the scheduler. Athanor is one scheduled service and remains a
proposal emitter; it does not become the generic scheduler or directly mutate
graph truth.

## 2. Entry gate: publish the resident baseline

This work starts after `djinn 0.0.2` and its public dependency chain are
published and pinned by a clean consumer. The current source version is not
that receipt: packaging still stops at workspace dependencies without registry
version requirements, beginning with `cambium` through `knot-editor`.

Done conditions:

1. `cargo package -p djinn --locked` succeeds from a clean checkout.
2. `djinn 0.0.2` is published with the resident positioning already landed.
3. A clean external probe resolves `djinn = "0.0.2"` without path or Git
   substitutions and starts the resident fixture.
4. The release commit and package contents are recorded before any work below
   changes the public API.

This plan targets the next source version. It does not fold new services into
the `0.0.2` release.

## 3. Invariants

1. An application never needs to be running for checks, downloads, or honest
   update status to advance.
2. Update policy is per installed application and configurable: off, notify,
   download then ask, or automatic; channel, cadence, and metered-network
   behavior remain explicit.
3. A store, distro, or enterprise-managed installation is reported as
   externally managed and is never overwritten by Djinn or Luggage.
4. Release keys are offline product keys, separate from Personae identity.
5. Manifest and artifact verification both happen before staging. Apply
   revalidates the complete persisted offer, not only the artifact digest.
6. A staged offer is bound to an application id, target, version, artifact
   format, canonical installation root, digest, and signed manifest.
7. Djinn never crosses an installation privilege boundary. Elevated or
   system-owned installs remain externally managed.
8. `Applied` means the newly launched binary reported its actual version. An
   installer spawn or process exit is not an apply receipt.
9. DNS-SD advertises only enabled service records. It does not disclose a
   persona, vault, place, document, or release key.
10. Firmware may use the signed artifact envelope, but Djinn never flashes a
    device. Linkboy owns compatibility checks, consent, transfer, and apply.
11. A release reference is a signed-manifest hash plus publisher-key identity.
    It carries neither a feed nor authority to trust that publisher.
12. Advertising, resolving, or joining a release never changes an installed
    application's trusted publisher, feed, channel, or update policy.

## 4. Phase A: make Luggage a durable resident boundary

### A1. Extract the proven policy grammar

Hocket currently owns the first real implementation of `UpdatePolicy`,
`UpdateChannel`, `UpdateStatus`, `decide`, and the update worker. Move the
product-neutral types and pure decisions into Luggage rather than copying them
into Djinn.

Add the missing policy axis from the auto-update brief:

- metered-network behavior;
- an explicit externally-managed state;
- durable timestamps for last attempt, last success, and next eligible check;
- failure phase and reason without a generic working state.

Hocket-specific settings projection and rendering stay in Hocket until the
resident migration in Phase C.

Done conditions:

- Hocket and a Luggage-only test consumer use the same public policy and status
  types;
- the four policies have table tests covering check, download, consent, and
  apply decisions on metered and unmetered networks;
- unknown persisted policy values fail visibly rather than silently changing
  behavior;
- no UI, Genet, Graphshell, or Djinn dependency enters Luggage.

### A2. Persist a self-sufficient verified offer

The current `Update::install_staged(&self, &StagedUpdate)` still requires the
ephemeral `Update` returned by the original feed check. A fresh application
holding only `StagedUpdate` cannot apply it. The current stage record also
stores mutable format and extraction-path fields while rechecking only the
artifact BLAKE3 digest.

Replace that handoff with a self-sufficient staged offer. The exact API may be
refined during implementation, but it must support this operation:

```rust
StagedUpdate::apply(expected_app, current_executable)
```

The stage persists the signed manifest bytes and signature, the artifact
signature, and the fields required to reconstruct the verified release. Apply
rechecks the manifest signature, artifact signature and digest, application
identity, target, version ordering, format, and permitted install root.

Applying returns an explicit disposition such as `RelaunchStarted`,
`AppliedExitRequired`, or `Deferred`; callers do not infer process behavior
from `Ok(())`.

Done conditions:

- a fresh process applies a stage without checking the network or retaining an
  `Update` object;
- tampering with any persisted offer field is refused;
- a valid artifact staged for one application or target cannot be applied to
  another;
- interrupted record/artifact writes recover as absent or resumable, never as
  a valid partial stage;
- Windows, macOS, and Linux tests cover the process disposition expected from
  each installer format.

### A3. Preserve rollback and channel history

Luggage persists the highest accepted release per application and channel.
Older signed releases remain authentic but are refused as automatic updates.
An explicit rollback, if later wanted, requires a separate user-authorized
operation and receipt.

### A4. Name a release independently of its feed and target

The browser/native session is the second consumer that forces Luggage's signed
manifest to become a portable release identity. Add a Wasm-clean release
envelope, owned and re-exported by Luggage:

```rust
pub struct ReleaseRefV1 {
    pub manifest_blake3: [u8; 32],
    pub publisher_key_id: [u8; 32],
}
```

`manifest_blake3` hashes the exact manifest bytes covered by the detached
signature. `publisher_key_id` is a domain-separated BLAKE3 digest of the
canonical decoded minisign public-key bytes. It is a lookup and display
identity, not a trust grant. A resolver accepts manifest bytes and their
detached signature from any carrier, then verifies them against a separately
trusted publisher key.

Version the signed manifest and add the cross-platform facts the current
updater-only shape lacks:

- stable application id and release version;
- source repository and revision;
- supported invitation and application-protocol versions;
- artifacts keyed by kind and target, including native installer, browser
  bundle, and later firmware;
- BLAKE3 digest, minisign signature, and format per artifact.

One manifest names one release family. Its Windows, Linux, macOS, and browser
artifacts have different hashes. The source revision is a signed publisher
claim, not reproducible-build proof.

Keep byte locations outside that identity. `ReleaseOfferV1` pairs a
`ReleaseRefV1` with disposable per-artifact locators supplied by a feed, native
host, HTTPS mirror, or peer. Changing mirrors must not change the release
reference. The offer is never trusted: only bytes matching an artifact in the
verified manifest may advance. The current v1 `luggage.json` embeds artifact
URLs, so v2 needs a compatibility reader and a split writer rather than
silently reinterpreting old manifests.

Keep the envelope verifier free of filesystem, HTTP, installer, Djinn,
Graphshell, and Genet dependencies so the stable `mer3ly.net` bootstrap loader
can verify a browser bundle before executing it. The native Luggage crate keeps
feed polling, staging, and platform apply around that core.

Done conditions:

- native and `wasm32-unknown-unknown` consumers derive the same
  `ReleaseRefV1` from frozen signed-manifest vectors;
- changing application id, source revision, compatibility, or artifact digest
  invalidates the manifest signature and reference;
- changing only `ReleaseOfferV1` locators leaves the reference unchanged and
  cannot make mismatching bytes verify;
- a reference carrying an unknown publisher id remains resolvable but is never
  described as trusted;
- the same verified artifact bytes are accepted from HTTP, a directory, and an
  injected in-memory source without changing the verifier;
- selecting a browser artifact cannot select a native installer or firmware
  entry with the same target spelling.

## 5. Phase B: add the family update resident to Djinn

### B1. Installed-app inventory

Add a versioned, device-local registry of installed family applications. An
installer writes or updates its own record; arbitrary application processes do
not choose another application's executable or release key.

Each record carries at least:

- stable application id and display label;
- install owner: self-managed or externally managed;
- canonical executable and allowed installation root;
- platform target and artifact formats;
- stable release-key identity;
- feed per configured channel;
- per-app update policy and stage directory.

An offered `ReleaseRefV1` may use an existing record only when its verified
application id and publisher key match. Adopting a fork is an explicit change
to both the trusted publisher key and configured feed; accepting its live
session is not that change.

Paths are canonicalized and constrained to the registering installation. A
record cannot contain an arbitrary command line for Djinn to execute.

Done conditions:

- registration, replacement, removal, malformed records, path escapes, and
  externally managed installs have focused tests;
- installer registration is atomic and idempotent;
- uninstall removes the registration and stage without touching another app;
- a copied development binary is not guessed to be an installed app.

### B2. Resident polling and staging

Add `FamilyUpdateResident` to Djinn. It loads the inventory, evaluates each
application's policy, schedules eligible checks, invokes Luggage, and persists
status transitions. Cadence, concurrency, retry, jitter, channel, and metered
behavior are policy inputs rather than hardcoded product defaults.

The resident must distinguish:

- disabled;
- externally managed;
- waiting for the next eligible check;
- checking;
- available;
- awaiting download consent;
- downloading;
- staged;
- awaiting install consent;
- apply on next launch;
- applied and confirmed;
- failed with phase, reason, and retry eligibility.

Djinn attempts every service shutdown and reports every failure through the
existing `mere-resident` close report.

Done conditions:

- two fake installed apps on different policies advance independently under a
  deterministic clock;
- closing every app does not stop checks or downloads;
- metered refusal and later unmetered resumption are chronological tests;
- a resident restart restores the exact durable status and never repeats an
  apply already acknowledged by the new binary;
- one broken feed does not block another application's schedule;
- shutdown cancels network work, preserves valid stages, and leaks no task.

### B3. Local status projection and intents

Project the durable application statuses through Djinn's admitted local
surface. Hosts render the projection and submit typed intents for check now,
download, approve install, defer, discard, and policy changes.

The projection does not expose feed credentials, release keys, installation
paths, or staged bytes. Caller admission precedes every mutation.

## 6. Phase C: migrate Hocket and prove the second consumer

Hocket is the first application migration because its update policy, worker,
Luggage transport, CLI, settings provider, and three-host receipts already
exist.

1. Move its current device-local update settings into the Djinn app record
   through an idempotent migration.
2. Remove Hocket's automatic check/download worker and in-memory Luggage
   transport.
3. Keep only the status renderer, explicit user intents, and the early startup
   hook that asks Luggage to apply an approved stage before Genet, winit, or
   the audio engine starts.
4. If Djinn is unavailable, report that the resident updater is unavailable;
   do not silently resurrect an application-owned polling loop.
5. Retain an explicit release-test command that exercises the same resident
   service rather than a second implementation.

Done conditions:

- a signed Hocket update is discovered and staged while Hocket is closed;
- notify does not download, download-then-ask does not apply without approval,
  and automatic applies at the next natural launch;
- Hocket reports its launched version to Djinn, which alone advances the state
  to applied;
- the existing Windows, macOS, and Linux installed-update receipts pass through
  the resident path;
- Hocket contains no periodic feed poll or background update worker afterward.

## 7. Phase D: Djinn updates itself

Djinn cannot rely on an always-running process to replace itself. The
platform launcher owns the final handoff:

- Windows Task Scheduler;
- a launchd user agent on macOS;
- a systemd user unit, or the selected equivalent, on Linux.

Djinn stages and records intent, then exits cleanly. The launcher invokes the
verified Luggage apply path before starting the resident again. Store locks and
network endpoints are released before apply. The new resident reports its
version and the launcher retains a recoverable previous installation until the
startup receipt succeeds.

Done conditions:

- the resident updates itself from `N` to `N+1` without a second live owner;
- failure before replacement restarts `N`; failure after replacement restores
  or explicitly offers the working previous version;
- the scheduled-task identity and profile selection survive the update;
- no general elevated update service is introduced.

## 8. Phase E: extract the resident scheduler through real jobs

Extract a small scheduler only after the update resident is working. Its first
two consumers are family updates and one Athanor maintenance pass.

The common contract is limited to:

- due-time calculation under an injected clock;
- cancellation and orderly shutdown;
- configurable jitter, retry, and missed-run behavior;
- durable last-run and next-eligible facts;
- structured outcome reporting.

Athanor still emits proposals. The authority that owns the affected store
reviews or applies them under its existing policy.

## 9. Phase F: notifications, agent door, and DNS-SD

### F1. Redshank notifications

Update availability, consent requests, staged updates, and failures provide
Redshank's first concrete resident consumer. Redshank owns notification
records, deduplication, acknowledgement, expiry, and delivery rules. Djinn owns
its lifetime and local projection. Hosts choose presentation.

### F2. Authenticated agent door

Djinn may own one authenticated local socket for agent clients. Personae and
Castellan decide caller and secret authority. MCP is an optional adapter over
that local service, not Djinn's native domain model. Remote listeners are out
of scope until a concrete consumer supplies an admission and disclosure model.

### F3. DNS-SD advertisement

Murm implements DNS-SD browsing and advertisement. Djinn supplies the registry
of enabled resident services and their disclosure-safe records. LAN discovery
is configurable and preferred for local reachability; it does not create
authorization and does not replace owner-selected relays outside the LAN.

The peer-discovery substrate this advertises beside is recorded as R0 of the
[reachability rungs plan](2026-08-03_reachability_rungs_and_privacy_lanes_plan.md)
(2026-09-01); `P2pandaHostPolicy` in `crates/murm/transport/src/p2panda_host.rs`
is where an advertisement toggle plugs in.

Done conditions for Phase F:

- an update notification appears once, survives a host restart, and can be
  acknowledged from two different hosts without duplicate delivery;
- an admitted local agent can enumerate only its allowed tools and receives no
  durable secret material;
- two LAN devices discover an enabled service without a relay, while an
  unconfigured service and all persona identifiers remain absent from DNS-SD;
- disabling discovery withdraws the advertisement and leaves underlying
  authorization unchanged.

## 10. Phase G: firmware artifacts without resident flashing

Extend Luggage's signed manifest envelope with an explicit artifact kind and
firmware compatibility facts: board, hardware revision, region, minimum
bootloader, and image digest. These are signed fields.

Djinn may check, download, and stage a compatible-looking firmware offer for a
registered device family. Linkboy independently validates the device and offer,
obtains consent, transfers bytes, flashes, and reports the resulting firmware
version. A desktop automatic policy never implies automatic firmware flashing.

## 11. Release and review gates

Each phase lands as an independently buildable slice. Before a new public
release:

1. run Luggage and Djinn focused suites with a deterministic clock and fake
   feeds;
2. run Hocket's installed update cycle on Windows, macOS, and Linux;
3. review canonical-path handling, signed-offer reconstruction, anti-rollback,
   release-reference resolution, publisher-key changes, local caller admission,
   stage permissions, and launcher privilege;
4. inspect packaged crate contents and verify every public dependency resolves
   from the registry;
5. retain app-authored receipts for applied version and resident-authored
   receipts for schedule and status chronology.

## 12. Non-goals

- a universal daemon framework shared by unrelated products;
- a public MCP, Knot, Misfin, Gemot, or relay service without its own forcing
  consumer and admission model;
- silent firmware flashing;
- replacing distro, store, or enterprise package managers;
- treating DNS-SD reachability as authorization;
- making Athanor a graph-truth authority;
- staged percentage rollout infrastructure before a release operator needs it.
