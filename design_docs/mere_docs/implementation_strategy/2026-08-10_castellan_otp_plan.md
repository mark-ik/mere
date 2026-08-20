# Castellan C1-C2: the OTP Slice

**Date:** 2026-08-10
**Status:** C1 and C2 complete 2026-08-20; platform adapters and issuer
compatibility remain follow-on slices
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
- [ ] Later platform adapter: Secret Service (`org.freedesktop.secrets`), the one OS surface a third
      party can *be* rather than read
- [ ] Later compatibility decision: Steam's nonstandard alphabet, with a real
      issuer corpus and user-facing contract before carrying it

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
  require a process-wide storage authority and durable rollback evidence in a
  separate slice. The admitted adapter does not upgrade caller text into proof;
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
