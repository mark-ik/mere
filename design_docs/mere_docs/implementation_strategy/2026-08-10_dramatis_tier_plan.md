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
| `crates/dramatis/gazetteer` | `mere-gazetteer` | handle resolution (finding them) |
| `crates/dramatis/gaz` | `gaz` | stored contacts (keeping them) |

Package names and lib names are untouched; only the tier directory and the
workspace paths change. Consumers that take `personae` by git dep (hocket) key
on package name and are unaffected.

## Done conditions

### D1: tier rename

- [ ] `crates/persona/` renamed `crates/dramatis/`
- [ ] Root `Cargo.toml`: 3 member paths + 4 workspace.dependencies paths updated
- [ ] README.md tier table row updated
- [ ] Dated plan docs deliberately NOT swept (receipts, per DOC_POLICY)

### D2: gaz relocation

- [ ] `repos/gaz` subtree-merged at `crates/dramatis/gaz` (history preserved,
      personae-consolidation precedent)
- [ ] Workspace member + `gaz = { path = ... }` dependency entry added
- [ ] gaz `Cargo.toml` repository field points at merely-made/mere; its bare
      `[workspace]` stub table removed so it joins mere's workspace
- [ ] Open with Mark: disposition of the emptied `repos/gaz` checkout and the
      merely-made/gaz GitHub repo (pointer README vs archive vs delete).
      Next gaz publish (0.1.0 per its founding plan) happens from mere.

### D3: the claim

- [ ] `dramatis` 0.0.1 reservation published (seven-file stub anatomy per the
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

The port is unnamed. The theatrical register has candidates (a tiring-house is
where actors don costumes) but the name needs its own challenge round.

## Progress

- 2026-08-10: plan written; D1-D3 executed this session (see Progress updates
  below); D4 deferred.
