# Castellan C1: the OTP Slice

**Date:** 2026-08-10
**Status:** C1 done 2026-08-10; C2 open
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
- [x] Constant-time code comparison for verification, and a skew window so a
      code from the adjacent step still verifies
- [x] Every file under the 600-line ceiling

### C2 and beyond: not this slice

- [ ] The chatelaine item: a stored, sealed OTP secret over
      `SealedRecordStorage`, persona-scoped
- [ ] The embeddable half: code tiles with their remaining-seconds ring
- [ ] The authority half: release through the participant gate
- [ ] Secret Service (`org.freedesktop.secrets`), the one OS surface a third
      party can *be* rather than read
- [ ] Steam's nonstandard alphabet: decide explicitly whether to carry it

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
