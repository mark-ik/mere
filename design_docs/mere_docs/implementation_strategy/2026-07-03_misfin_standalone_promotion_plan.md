# Misfin Standalone Promotion Plan

**Date**: 2026-07-03
**Status**: Executed same-session; a short tail remains (below).
**Scope**: Promote `crates/murm/misfin` out of the workspace into a standalone
public repo, completed to specification prototype B, and point the workspace
at it. The first workspace-crate analogue of the
[meerkat promotion pass](2026-07-02_meerkat_promotion_pass_plan.md), executed
at Mark's direction after a name-stewardship conversation: the bare `misfin`
crates.io name should only be held by something reference-quality, and the
protocol's author (lem) gets the name on request.

## What was done

1. **Repo**: <https://github.com/mark-ik/misfin> (local `repos/misfin/`),
   **MIT** (relicensed same-day at Mark's direction to match how lem's repo
   licenses the reference implementation; the protocol itself is declared
   public domain there, spec docs CC-BY-SA 4.0), crates.io-only deps (the
   wgpu-sibling standalone bar). README
   carries the stewardship language: not the reference implementation, name
   transfers to the protocol author on request.
2. **Spec completion** (audited against the spec + best-practices docs at
   `github.com/JCLemme/misfin`, prototype B, 2023-05-11):
   - **Client** (new): public async `send` — one TLS transaction, client cert,
     SNI, 2048-byte enforcement (typed `MessageTooLong`), CR rejection,
     fingerprint pinning (`expected_fingerprint`), close-notify, per-step
     timeouts, `connect_addr` override. The old crate had no send path at all
     (production sending rode errand).
   - **Status codes** (new): full typed `MisfinStatus` for all 19 spec codes +
     categories + `parse_response_line`. The old server knew 5 codes.
   - **Server**: sender-identity extraction from client certs
     (USER_ID + SAN via `x509-parser`) so `sender_address` is populated;
     per-identity TOFU pins in a new `sender_identities` redb table with
     **63** on changed fingerprints (pin never silently overwritten;
     `forget_sender_identity` is the operator release); **62** on
     out-of-validity certs; **59** on over-long (was: silent truncation) and
     non-UTF-8 requests. `MisfinServerConfig` gained
     `require_sender_identity` / `reject_changed_sender` (constructor
     defaults: false / true).
   - **Gemmail**: composition (`to_gemtext`) and the §4.2 reply-set helper
     (`reply_recipients`: sender-first, deduped, never-self) beside the
     existing parse.
   - **Identity**: explicit-root storage API (`*_with_root` now public);
     dropped the `dirs`-based implicit root that hardcoded a `graphshell`
     config path (no mere consumer used it; production uses
     `deterministic_identity`). New `identity_material_with_root` for TLS
     material.
   - **CLI** (new, `cli` feature): `misfin id / send / serve / inbox`, so
     `cargo install misfin` yields a working mailbox tool.
3. **Verification**: 35 unit/integration tests green (`--all-features`),
   including a full TLS round-trip through the public client and a 59-on-3000-
   bytes wire test; plus a headed CLI smoke (serve + send + inbox on
   localhost) confirming sender identity arrives from the certificate.
4. **Workspace swap**: root `Cargo.toml` now takes `misfin` as a git dep
   (branch-tracked, per the owned-repos convention); `crates/murm/misfin`
   deleted; `meerkat::comms_host` moved to `MisfinServerConfig::new`.
   `cargo check -p meerkat` green; `cargo test -p comms
   --features misfin-adapter,murm-adapter` 22/22 green.

## Remaining tail

- ~~crates.io publish of 0.0.2~~ **done 2026-07-04** (Mark published; the
  registry now carries the MIT relicense, the send client, and the CLI).
- **Send-path unification**: mere still sends misfin mail via
  `errand::misfin_send`; the crate now owns a spec-complete client, so errand's
  bespoke sender can eventually delegate or retire. Not urgent; errand is
  serval-side.
- **Out of scope, recorded**: multi-domain hosting + CA-signed mailbox certs
  (spec §3.1 advanced), behaviors behind codes 42/43/44/64, and the
  community's post-B spec discussions (misfin "C") — revisit if the upstream
  spec moves.
- **Name transfer**: standing offer; if lem asks, transfer crates.io ownership
  and rename the repo's crate to a qualified name (`misfin-client` fits the
  ecosystem convention).

## Progress

- **2026-07-04**: misfin 0.0.2 published to crates.io by Mark. The remaining
  tail is errand send-path unification (errand's `misfin_send` could delegate
  to `misfin::client::send`, the same move errand's guppy made to
  `guppy-protocol` on 2026-07-04), the spec-C watch, and the standing
  name-transfer offer.
- **2026-07-03**: Spec + best-practices read; per-file audit of the workspace
  crate (solid: identity minting, gemmail parse, receive server, mailbox
  store; gaps: no client, 5/19 codes, no cert-identity read, silent
  truncation, `graphshell` path). Repo created, gaps closed, tests + smoke
  green, GitHub pushed, workspace swapped and verified. Publish left for
  Mark.
