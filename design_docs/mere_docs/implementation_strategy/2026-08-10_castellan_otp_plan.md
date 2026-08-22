# Castellan C1-C2: the OTP Slice

**Date:** 2026-08-10
**Status:** C1 and C2 complete 2026-08-20, hardened and extended 2026-08-21
(resident lock and freshness ledger, Linux Secret Service, Steam Guard).
Library-complete. Djinn now claims per-profile `CastellanResident` record and
freshness custody; code presentation, admitted approval, Secret Service
policy, credential replication between persona devices, and CXF import remain
follow-on slices.
**Anchors:** [credential port + gazette brief](../research/2026-08-10_credential_port_gazette_brief.md)
(Part I), [dramatis tier plan](2026-08-10_dramatis_tier_plan.md) D4,
[wallet carry fold-in plan](2026-08-10_wallet_carry_foldin_plan.md) (the
prerequisite, complete).

## Why this first

The brief's runway starts with TOTP for three reasons that still hold: the
RFCs ship test vectors so correctness is checkable rather than asserted, the
work is self-contained with no platform entanglement, and it has daily value
on its own. Secret Service, CXF import, and the WebAuthn provider all sit
behind platform or format work; this does not.

The carry fold-in is done, so `personae::carry` and `SealedRecordStorage`
(load/save/delete of serde records under a 32-byte key) are available as the
storage substrate when C2 needs them.

## Scope

**C1 is the algorithm and its import format only.** No storage, no UI, no
agent surface, no platform integration. Those are C2 and later, and each
wants its own slice.

Concretely: given a secret and a clock, produce the six digits; given an
`otpauth://` URI, produce the configured generator. Both directions verified
against published vectors.

## Where it lives

`ports/castellan/src/otp/`, as `castellan::otp`. Not in `chatelaine`: that
crate is an empty reservation, and per the module/crate/publish rule a crate
needs a wall, a subset, a consumer, or an audience. This has one consumer
(castellan) and no wall yet. If a second consumer appears, it promotes.

The division the vocabulary implies holds: a TOTP *secret* is a chatelaine
item (stored, never shown), and computing a code is the *castellan*
exercising it. C1 builds the exercising; C2 builds the item.

## Done conditions

### C1: the OTP core

- [x] HOTP per [RFC 4226](https://www.rfc-editor.org/rfc/rfc4226): HMAC-SHA1
      over the 8-byte big-endian counter, dynamic truncation, modulo digits
- [x] TOTP per [RFC 6238](https://www.rfc-editor.org/rfc/rfc6238): counter =
      (now - T0) / step, with SHA1, SHA256, and SHA512 variants
- [x] RFC 4226 Appendix D: all 10 published HOTP values
- [x] RFC 6238 Appendix B: all 18 published TOTP values across the three
      hash functions. Note the erratum: the appendix's table shares one ASCII
      seed visually, but each algorithm uses a seed of its own hash length
      (20/32/64 bytes), which is the classic way to fail this vector set.
- [x] Base32 secret decoding (RFC 4648, unpadded, case-insensitive) verified
      against RFC 4648 §10 vectors. Hand-rolled rather than a dependency: it
      is a 30-line alphabet mapping and the vectors make it checkable.
- [x] `otpauth://totp/` and `otpauth://hotp/` URI parsing (the de-facto
      Key Uri Format): label, issuer, secret, algorithm, digits, period,
      counter; percent-decoding; issuer-prefix vs `issuer=` reconciliation
- [x] Constant-time code matching, and a skew window so a
      code from the adjacent step still matches. This is deliberately named as
      a matching primitive: replay-safe verifier state is a separate authority.
- [x] RFC 4226's 128-bit minimum seed length, a bounded comparison window,
      six/eight-digit Key URI validation, nonempty labels, duplicate-parameter
      rejection, and issuer-prefix agreement
- [x] Every file under the 600-line ceiling

### C2 core and later integrations

- [x] C2a: the chatelaine item: a stored, sealed OTP secret over
      `SealedRecordStorage`, persona-scoped. `OtpItemStore` is deliberately
      local and in-process: it exposes secret-free metadata, has no seed
      accessor, and its code-bearing operation is crate-private to the gate.
- [x] C2b: the embeddable half: `OtpCodeTile` carries code, metadata, and
      an absolute expiry from which a renderer derives remaining seconds,
      without fixing component geometry or a carrier wire
- [x] C2c: the local authority half: `OtpReleaseGate` owns petition and release
      time, bounds and expires its resident-approval queue, and is the only
      public code path for sealed items. HOTP load/exercise/replace is one
      in-process transaction shared by clones of the opened sealed store; the
      replacement file is flushed before return.
- [x] First host/carrier consumer: `OtpAdmittedSession` consumes a Notochord
      admission for one exact persona/item path, derives participant and session identity
      from its signed transcript, and rechecks retained expiry/revocation before
      petition, approval, and delivery. Approval stays opaque until a final guard
      pairs the tile with that session's original carrier. The guard holds the
      caller's revocation snapshot through its application-protocol write rather
      than founding a second wire. Direct local claims remain explicitly
      `unverified` and cannot approve an admission-derived petition.
- [x] Linux platform adapter: Secret Service (`org.freedesktop.secrets`), the one OS surface a third
      party can *be* rather than read. It uses the Freedesktop 0.2 object tree,
      caller-bound plain sessions, host policy over bus credentials and
      executable identity, bounded stores, and a real `secret-tool` receipt.
- [x] Steam Guard compatibility: a separate five-character code style and
      base64 `shared_secret` import. The checked shared-secret/time/code corpus
      comes from the independently maintained `steamguard` 0.18.4 library,
      whose account-link flow verifies generated codes with Valve. Valve does
      not publish a normative algorithm or corpus, so this is named
      compatibility and never inferred from an `otpauth://` extension.

## Deliberate non-goals

- **No `otpauth-migration://`** (Google Authenticator's protobuf export). It
  is undocumented and reverse-engineered; CXF is the standard import path and
  is the brief's actual runway item.
- **No secret generation.** Provisioning is the relying party's job; the
  keeper stores and exercises what it is given.

## Progress

- 2026-08-10: C1 landed. 26 tests, every published vector green on the first
  run, clippy clean, largest file 344 lines.

  Three things worth carrying into C2:

  - The RFC 6238 three-seed trap was real and worth writing down: the appendix
    table reads as though one ASCII seed covers all three algorithms, but each
    uses a seed of its own hash length (20/32/64). The test module names this
    at the constants so the next reader does not lose an afternoon.
  - One generic `hmac_with<D>` did not survive contact with the RustCrypto
    bounds (`CoreProxy::Core: HashMarker + FixedOutputCore + Default + Clone`).
    Three concrete expansions through a local macro are correct and far easier
    to read. Not worth revisiting.
  - `Otp` holds the secret and hands it back through no accessor, and its
    `Debug` redacts. That is the chatelaine rule (exercised, never shown)
    enforced at the type rather than by convention, and a test asserts it.

- 2026-08-20: C2a landed. `OtpItemStore` writes one sealed record under
  `castellan/otp/v1/<persona>/<item>.json`; Personae binds that path into the
  authenticated ciphertext as well as keeping it below the persona namespace.
  The store round-trips RFC 6238 material after reopening, keeps account and
  issuer out of the on-disk plaintext, and proves that the same item handle is
  absent for another persona.

- 2026-08-20: C2b and C2c landed. `OtpCodeTile` gives a host the code,
  secret-free metadata, and an absolute expiry without prescribing component
  geometry or a carrier message. `OtpReleaseGate` is the only public code path
  for sealed items: the resident approves or denies a bounded, expiring
  petition, and denial does not exercise HOTP. Approval advances and seals
  HOTP's next counter before returning the code.

- 2026-08-20: security and standards review hardened the core. Ten-digit core
  generation now uses a non-overflowing modulus while Key URI imports accept
  only six or eight digits. Imports require RFC-minimum secrets, nonempty
  labels, unambiguous known parameters, and matching issuer identities. Matching
  windows are bounded and are not described as replay-safe verification.
  Release time comes from the gate, pending requests have configurable lifetime
  and capacity, session bindings are redacted from diagnostics, and TOTP tiles
  carry absolute expiry. Sealed-record clones serialize HOTP updates below the
  gate and flush the replacement before rename.

  The limits are stated rather than hidden: this local transaction does not
  coordinate independent store openings or processes, and AEAD does not provide
  rollback resistance if an older valid backing directory is restored. Those
  gaps are closed by the 2026-08-21 resident slice below. The admitted adapter
  does not upgrade caller text into proof;
  it constructs the authenticated participant form only from Notochord's local
  admission conclusion.

- 2026-08-20: the admitted-session consumer landed. The owner policy requires a
  Personae delegation and transport-authenticated identity for
  `mere.castellan` / `/services/castellan/otp/{persona}/{item}` / `release`. A signed
  Notochord admission produces the only authenticated participant form. Each
  session can petition only for its admitted item; approval rechecks the live
  chain, and the resulting code is opaque until same-session delivery rechecks
  it again. Revocation, expiry, cross-session substitution, direct-gate bypass,
  and unresolved-session cleanup have executable receipts. The adapter defines
  no carrier bytes; a successful write on the paired carrier remains the host's
  transport-liveness receipt.

- 2026-08-21: the Castellan resident now claims one exclusive OS file lock for
  the credential-record directory and retains it through every clone. A child
  process receipt proves a second authority is refused. Independent Secret
  Service views for one persona share the same composite transaction lock.

  Sealed-record envelopes now carry authenticated generations and authenticated
  tombstones. A keyed freshness ledger lives under a separately required root
  and uses prepare/write/commit evidence so interrupted replacement can be
  reconciled. Restoring an older valid HOTP record is rejected during approval,
  before the old counter can be released again. The boundary stays explicit:
  rolling back both roots together is outside a file ledger's power, and
  version-one records establish their first freshness baseline on authoritative
  access.

- 2026-08-21: feature `secret-service` implements the Freedesktop Secret
  Service 0.2 service, collection, item, and session interfaces on Linux. It
  requests the standard name without replacement, supports the specification's
  recommended plain transfer session, binds each session to its unique D-Bus
  caller, and asks a host policy about every catalog, metadata, mutation, and
  secret release operation using bus-supplied uid, pid, security label, and
  resolved executable facts. An isolated-session-bus test on the ThinkPad ran
  `secret-tool store`, `lookup`, and `clear` successfully.

- 2026-08-21: Steam Guard compatibility is carried as `OtpCodeStyle::SteamGuard`
  and `import_steam_guard`, not as a new RFC OTP mode or an `otpauth://`
  parameter. The implementation matches the `steamguard` 0.18.4 corpus
  (`zvI...44s=`, Unix time `1616374841`, code `2F9J5`), uses a fixed 20-byte
  secret, SHA-1, 30-second steps, and Valve's 26-character alphabet. Stored
  release still passes through the existing participant gate and expiring tile.

- 2026-08-21: status repaired; the slice is library-complete, not
  product-complete. `OtpAdmittedSession` is a tested consumer, but nothing
  outside Castellan's own tests hosts `CastellanResident`, renders
  `OtpCodeTile`, approves through Cambium, or starts `secret_service::serve`;
  graphshell enables only `keeper`. No replication or carry path exists for
  the OTP or Secret Service record namespaces, so records are durable on one
  device only. Follow-on slices, in order: product hosting (device-host
  composition per the [device resident consolidation plan](2026-08-20_device_resident_consolidation_plan.md),
  Cambium tile and approval, Linux user-service lifecycle); credential
  replication designed together with per-device freshness evidence; CXF/CXP
  import. Windows Credential Manager and macOS Keychain import, KDBX, and a
  WebAuthn/CTAP2 provider stay separate projects. Steam enrollment,
  recovery codes, and Valve API integration are out of scope; current Steam
  support imports a `shared_secret` and generates codes. The crate is still
  0.0.2; no release followed C2.

- 2026-08-22: Djinn now opens and closes `CastellanResident` beside the
  selected Personae profile. It derives separate record and freshness keys
  from that unlocked identity, stores both under a profile-constrained Djinn
  root, and keeps that custody alive with the other resident resources. It
  does **not** start `secret_service::serve`: Castellan's service needs a
  concrete `PersonaId` selection and executable/caller policy, neither of
  which may be guessed from a profile string. Likewise Djinn does not yet
  render `OtpCodeTile` or host an admitted approval surface. Those are the
  forcing consumers for the next product slice, not optional daemon defaults.
