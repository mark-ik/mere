# castellan

**Castellan**, the credential-keeper port of the Mere platform.

A castellan holds a keep in trust for its lord: custody without ownership, and
the office of the gate. This port is that keeper for your credentials. It
splits in two: an embeddable half any host app composes (vault browse, status,
code tiles; views that render *about* secrets and never contain them), and an
authority half that lives with the resident (release, signing, presentation),
answering participant-gate petitions over an agent-style channel the way the
personae ssh-agent already works. Apps talk to a pipe; apps never see the key.

The vocabulary it keeps, per the dramatis tier model:

- **chatelaine**: the secrets. Passwords, 2FA seeds, tokens, foreign key
  material. Never presented, only exercised.
- **insigne**: the proofs. Graded presentations of identity a persona hands
  out, from a bare handle to signed cross-attestations. Made to be shown; what
  lands in someone else's gaz.

The boundaries are the point: not [personae](https://crates.io/crates/personae)
(the faces and vault substrate castellan serves), and not
[gaz](https://crates.io/crates/gaz) or gazette (which keep and find the other
players; castellan guards and presents you).

Lives in the [mere](https://github.com/merely-made/mere) workspace at
`ports/castellan`.

## State (2026-08-21)

Implemented:

- `otp` — RFC 6238/4226 codes and `otpauth://` URIs, plus persona-scoped OTP
  items sealed through Personae's record store. `OtpReleaseGate` returns a
  redacted-debug `OtpCodeTile` only after a participant-bound petition receives
  an explicit approval; its time facts leave ring geometry to the host.
  `OtpAdmittedSession` consumes Notochord admission for one exact item, derives
  the participant from the signed transcript, rechecks expiry and revocation at
  approval and delivery, and exposes the tile only beside the original carrier.
  It leaves byte encoding to the composing host's existing protocol. Steam
  Guard is a separate, explicit code style with Valve's five-character
  alphabet and base64 `shared_secret` import. It does not reinterpret an
  `otpauth://` extension as Steam.
- `resident` — one process-wide owner for Castellan's sealed records. The
  resident retains an exclusive OS file lock, shares composite Secret Service
  transactions across independent views, and checks a separately rooted keyed
  freshness ledger before releasing restored HOTP state.
- feature `secret-service` — the Freedesktop Secret Service 0.2 object tree on
  Linux. The resident owns `org.freedesktop.secrets` without replacement,
  implements the recommended `plain` transfer session, binds sessions to D-Bus
  callers, and delegates every operation to a host policy over bus credentials
  and `/proc` executable identity. A `secret-tool` store/lookup/clear receipt
  runs under a disposable session bus.
- `reticulum` — the first device-identity issue seam: a radio credential
  derived from a Persona provider, no device-local account file.
- feature `keeper` — the two halves made real, moved home from graphshell
  where they first grew: `view` (the secret-free read model), `projection`
  (the cards and typed intents: signing decisions, SSH generate/import/
  remove, device revoke, persona switch, persona create), and `authority`
  (`PersonaeHost`, the resident keeper that holds the vault, serves the SSH
  agent, and brokers approvals).

Graphshell composes all three and re-exports them at its pre-founding paths,
so it is the first host rather than the owner. The intent wire strings keep
their `castellan.*` values for now; renaming the wire vocabulary is
a separate decision. CXF import remains follow-on work. The file freshness
ledger detects rollback of the credential-record root only when its separate
root was not restored with it; stronger platform monotonic storage remains a
host deployment choice. See the keeper founding plan and the credential port
brief in mere's `design_docs`.

## License

MIT OR Apache-2.0
