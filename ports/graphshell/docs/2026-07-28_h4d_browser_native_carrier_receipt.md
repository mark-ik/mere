# H4d browser native-messaging carrier receipt

**Date:** 2026-07-28  
**Plan gate:** H4, make Personae visible and usable  
**Result:** browser carrier and headed Chromium identity projection passed; H4 remains in progress

## Product cut

Graphshell now has a browser-extension carrier backed by a native host:

- `graphshell_native_host` loads the selected Personae profile and resident
  identity authority;
- Chromium and Firefox extension manifests request only `nativeMessaging`;
- current-user installers write browser-specific native-host manifests with
  exact extension ids;
- the shared extension page opens the admitted identity projection, fetches
  content-addressed portable cards, and renders disclosed signing decisions;
- release packaging remains separate from the H5 history-capture work.

**Current topology note:** H4f subsequently moved vault and SSH ownership into
the resident `graphshell_device_host`. The installed `graphshell_native_host`
is now a vault-free browser relay into that authority. The admission and headed
receipt in this note remain the same wire path. See the
[H4f resident-device-host receipt](2026-07-28_h4f_resident_device_host_receipt.md).

Chromium's development manifest carries a fixed public key, giving it the
stable id `oajkkocppbpbmfblepgbiidagliniofd`. Firefox uses the explicit add-on
id `graphshell@mere.systems`.

## Admission boundary

Browser native messaging is JSON framed with a native-endian 32-bit length.
Graphshell applies a 1 MiB bound in both directions.

The browser process first sends a fresh host challenge. The extension echoes
it with its own fresh nonce. The native host derives a private link id from:

- the exact launcher id supplied by the browser;
- the host nonce;
- the extension nonce.

The host then runs the existing signed `SessionHello` over a private duplex
bound to that link. The application stream is not available until Notochord
accepts it. A captured hello fails against another challenge.

The carrier reports `CarrierKind::Other` and does not claim a
transport-authenticated initiator. Browser launcher arguments are browser
context, not an OS peer credential. The local threat model currently trusts
same-user processes. An OS peer credential can strengthen these facts later
without changing Graphshell's application protocol.

The Personae profile, session signer, delegation issuer, vault, and private
keys remain native. The extension receives the transcript-derived projection
session id, public subject, snapshots, resources, and advertised intents.

## Evidence

- Focused carrier suite: 6 passed. It covers native framing, exact launcher
  admission, challenge mismatch, transcript replay refusal, and successful
  `SessionHello` admission. The follow-on H4e schema test also proves that the
  serialized native-import request has no secret-bearing field.
- Carrier plus Personae integration: 1 passed. A browser carrier mounted every
  public identity card, reconstructed an approve-once payload from the public
  request id, released the waiting real SSH adapter, and verified the
  signature.
- Native host binary check passed against the isolated live-source Graphshell
  harness.
- A spawned native host process admitted a simulated Chromium launcher,
  opened the identity projection, transferred and decoded a portable card,
  found no private material, and closed cleanly.
- Headed Chromium loaded the real unpacked extension through the browser's
  native-messaging registry. A purpose-built receipt host presented a real
  waiting `PerUse` SSH request. The page reached `Admitted · 4 public cards`,
  clicked `Approve once`, replaced the pending card with one completed signing
  history card, and closed the device session cleanly. The receipt host
  verified the resulting signature and exited. The browser reported no
  console or page errors.
- Chromium and Firefox package manifests parse, both bridge scripts pass
  JavaScript syntax checks, and both unpacked package directories prepare
  successfully.

The headed before/after screenshots and generated test profiles remain under
`C:\t` and are not Git inputs. The temporary native-messaging registry entries
were verified against the test manifest paths and removed after the run.

## Verification boundary

The headed Chromium run proves the real browser extension, native host
registration, native-messaging framing, admission, application session,
identity-card presentation, approval intent, real SSH signature, refreshed
history projection, and clean close together.

Firefox's manifest and package were prepared and statically checked; a headed
Firefox native-messaging run remains open.

## H4 gates still open

- bind the standard SSH-agent endpoint and prove a real login after restart;
- own launch-at-login and crash recovery, then retire the standalone task;
- add carry mutation intents and mixed-scene reopen.

The native picker and encrypted-key passphrase interaction are proved by the
[H4e native import receipt](2026-07-28_h4e_native_key_import_receipt.md).
The standard endpoint and live scheduled task remain unchanged.
