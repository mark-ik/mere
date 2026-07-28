# H4e native encrypted-key import receipt

**Date:** 2026-07-28  
**Plan gate:** H4, make Personae visible and usable  
**Result:** native picker, encrypted-key unlock, and headed Chromium import passed; H4 remains in progress

## Product cut

The admitted browser identity surface can now ask the resident Personae
authority to import an SSH private key. The native-messaging request contains:

- the admitted projection session id;
- the fixed `import_ssh_private` action;
- a user-selected `PerUse`, bounded `ShortTtl`, or `Session` unlock policy.

It has no path, key bytes, passphrase, or general payload field. The native
host opens the system file picker and, only for an encrypted OpenSSH key, a
local password dialog. The browser receives a public fingerprint, comment,
unlock-policy label, and replacement flag or a bounded failure reason.

The extension disables the import controls while a native interaction is in
flight. Repeated activation therefore cannot queue several native dialogs.

## Secret and input boundary

The host:

- refuses a console fallback because stdout belongs exclusively to native
  messaging;
- accepts only a regular file no larger than 1 MiB;
- zeroizes the selected file buffer and passphrase buffer on drop;
- parses and decrypts inside the resident process;
- passes the parsed `ssh_key::PrivateKey` directly to `PersonaeHost`;
- maps picker, parser, passphrase, and vault errors to bounded public results.

The dialog provider is isolated behind `NativeIdentityUi`. The current
[`light-file-dialog` backend](https://docs.rs/crate/light-file-dialog/3.21.3)
offers Windows, macOS, and Linux implementations, but its own release notes say
the macOS and Linux ports were not yet tested. Treat it as provisional until
it receives dependency review and headed receipts on those platforms.

## Evidence

- Native identity UI suite: 3 passed. It imported an encrypted key with the
  correct local passphrase, returned `IncorrectPassphrase` without importing
  on a wrong passphrase, and refused an unavailable graphical UI.
- Browser carrier suite: 6 passed. The added schema test proves the serialized
  request has no path, passphrase, key-bytes, or payload field.
- Admitted carrier plus Personae integration: 1 passed.
- Native and web-only Graphshell checks passed offline.
- Both extension manifests prepared successfully and `bridge.js` passed the
  JavaScript syntax check.
- Headed Chromium loaded the unpacked extension and reached an admitted
  two-card identity scene. Two immediate import activations produced exactly
  one pending native request and disabled the import controls.
- The real Windows file picker selected a fresh encrypted Ed25519 receipt key.
  Its real local password dialog unlocked the key. The refreshed scene reached
  `Admitted · 3 public cards`, showed one `H4e native picker receipt` card with
  `Unlock: every use`, and reported no console or page errors.
- The browser-visible page contained neither the fixture path nor an OpenSSH
  private-key header. The session closed cleanly, the receipt host verified the
  expected public fingerprint, and its exact temporary key file was deleted.

The final headed screenshot is
`C:\t\graphshell-h4e-import-after.png`. Generated browser profiles and proof
outputs remain outside Git. Temporary native-host registrations and receipt
browser processes were removed after the run.

## Verification boundary

The headed receipt used an in-memory receipt profile and a disposable key. It
did not read or alter the user's vault, standard SSH-agent endpoint, or
standalone agent task.

Windows now has a real end-to-end import receipt. Headed Firefox and the other
desktop dialog providers remain open.

## H4 gates still open

- bind the standard SSH-agent endpoint and prove a real login after restart;
- own launch-at-login and crash recovery, then retire the standalone task;
- add device enrollment, grant, delegation, expiry, and revocation intents;
- add carry mutation intents and reopen identity and carry in one mixed scene;
- prove the resident endpoint and native dialogs on the other supported
  desktop platforms.

The standard endpoint and live scheduled task remain unchanged.
