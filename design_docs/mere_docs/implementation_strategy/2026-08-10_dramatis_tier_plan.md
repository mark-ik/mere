# Dramatis Tier Plan

**Date:** 2026-08-10
**Status:** ratified by Mark 2026-08-10; D1-D3 executing this session
**Authority for gaz internals:** `crates/dramatis/gaz/design_docs/2026-08-08_gaz_founding_plan.md` (travels with the crate)

## What was ratified

The identity/contact tier gets a name that is not a collision with the
in-product term *persona*: `crates/persona/` becomes **`crates/dramatis/`**
(dramatis personae, the cast list: your faces and the other players). gaz moves
in from its standalone repo. The bare `dramatis` crates.io name is claimed as a
reservation for a possible future facade.

Considered and declined: gaz as the umbrella over the tier. gaz was defined
2026-08-08 as the contact layer specifically, sibling to gazetteer, and the
"them" side owning the "me" side inverts the model.

## Tier contents after this plan

| Crate | Package | Role |
|---|---|---|
| `crates/dramatis/personae` | `personae` | identity + carry spine (me) |
| `crates/dramatis/persona-picker` | `mere-persona-picker` | Cambium view over the roster |
| `crates/dramatis/gazette` | `gazette` | handle resolution (finding them) |
| `crates/dramatis/gaz` | `gaz` | stored contacts (keeping them) |

Package names and lib names are untouched; only the tier directory and the
workspace paths change. Consumers that take `personae` by git dep (hocket) key
on package name and are unaffected.

## Done conditions

### D1: tier rename

- [x] `crates/persona/` renamed `crates/dramatis/`
- [x] Root `Cargo.toml`: 3 member paths + 4 workspace.dependencies paths updated
- [x] README.md tier table row updated
- [x] Dated plan docs deliberately NOT swept (receipts, per DOC_POLICY)

### D2: gaz relocation

- [x] `repos/gaz` subtree-merged at `crates/dramatis/gaz` (history preserved,
      personae-consolidation precedent)
- [x] Workspace member + `gaz = { path = ... }` dependency entry added
- [x] gaz `Cargo.toml` repository field points at merely-made/mere; its bare
      `[workspace]` stub table removed so it joins mere's workspace
- [x] Disposition ruled by Mark 2026-08-10: DELETE. repos/gaz +
      merely-made/gaz and repos/dramatis + merely-made/dramatis all removed
      (gaz history lives in mere via the subtree merge; the dramatis stub
      moved to crates/dramatis/dramatis and 0.0.2 published so the registry
      points at mere). Next gaz publish (0.1.0 per its founding plan) happens
      from mere; its 0.0.1 registry page carries a dead repo link until then.

### D3: the claim

- [x] `dramatis` 0.0.1 reservation published (seven-file stub anatomy per the
      tulpa pattern, MIT/Apache, ed2024), local repo `repos/dramatis`,
      GitHub merely-made/dramatis

### D4: deferred, explicitly not this session

- [ ] Wallet fold-in: `session-runtime::{wallet_store, wallet_grant}` +
      `WalletEpochSealer` into personae per the 2026-07-08 ruling. Touches the
      live seal seam; own plan when taken up.
- [ ] The credential-manager port (see direction below).
- [ ] Any facade content in the `dramatis` crate. The reservation stays empty
      until something imports it.

## Direction: the credential surface is a port

Ruled direction from the same conversation, recorded here so D4 has an anchor.
The password/2FA/passkey/token/wallet capability is not a new app; it is a
**port** in `mere/ports/`, sibling to graphshell and knot: a capability other
apps using mere pattern or embed. Prior ruling already places the agent's
resident home in mere/Graphshell (2026-07-22 vault/agent plan).

Shape to hold: split the port into an embeddable half (vault browse, status,
TOTP tiles; any host can compose it) and an authority half (credential release,
signing approvals) that lives with the resident and flows through the
participant gate as petitions. Hosts embed views; they never hold secrets. This
mirrors how the ssh-agent already behaves (apps talk to the pipe, never see the
key).

Standards runway for the port, in rough order of cost: TOTP/HOTP (RFC 6238/4226,
published test vectors), CXF/CXP credential import (FIDO Alliance exchange
format), OS credential surfaces (Secret Service, Keychain, Credential Manager),
KDBX import, WebAuthn/CTAP2 passkey provider (the heavyweight item). Prior art
to read for technique, not adopt wholesale: IOTA Stronghold, keyring-rs.

The port is **castellan** (ratified 2026-08-10; 0.0.1 claimed from
ports/castellan, where it will found). Tiring-house is retired: its meaning is
personae's, not the port's. The round also promoted **chatelaine** (the
secrets, exercised never shown) and **emblem** (the graded proofs, made to be
shown; what lands in someone else's gaz) to tier vocabulary; both claimed
same day (0.0.1 reservations under crates/dramatis, Mark authorized).

Both this direction and the gazette's feed direction are fleshed out in the
[credential port + gazette brief](../research/2026-08-10_credential_port_gazette_brief.md),
which owns the standards inventory and the open questions until dated plans
supersede it.

## Progress

- 2026-08-10: plan written; D1-D3 executed and verified same session. Two
  relative personae paths (servitor, commons-spine) escaped the workspace-table
  sweep and were fixed in a follow-up commit; the lesson is that consumers path
  tier crates relatively, so a tier rename greps for `persona/`, not just
  `crates/persona`. gaz: 41 tests + doctest green from the workspace. dramatis
  0.0.1 published; merely-made/dramatis created. D4 remains deferred. Open with
  Mark: repos/gaz + merely-made/gaz disposition.
- 2026-08-10, follow-up: Mark noticed bare `gazetteer` is taken on crates.io
  while `gazette` is free, and ruled: take gazette. The crate renamed back to
  its pre-2026-07-08 name (`mere-gazetteer` -> `gazette`, dir + package + lib;
  zero consumers made it free), and `gazette` 0.0.1 published from mere. The
  2026-07-08 "an index, not a broadcast" rationale is superseded; the recovered
  sense is the official gazette, where appointments are *gazetted*: officially
  announced and thereby resolvable, which is what a resolver does.
