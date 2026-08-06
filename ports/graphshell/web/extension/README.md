# Graphshell browser carrier

This package carries the Graphshell Wasm portal and the H4 browser-to-device
identity bridge. H5 adds consented browser-history delivery, durable local
authority, filtering, forgetting, and headed Chromium and Firefox receipts.

## Boundary

The browser extension and `graphshell_native_host` relay hold presentation and
connection state only. The selected Personae profile, vault, SSH keys,
delegation issuer, and `SessionHello` signer stay in the resident
`graphshell_device_host`.

Installing the native host manifest pairs one exact extension id with the
relay binary. The relay connects to the per-user device broker. Every
connection then:

1. receives a fresh native-host challenge;
2. answers with that challenge and a fresh browser nonce;
3. derives a private link id from the browser launcher id and both nonces;
4. runs the ordinary signed `SessionHello` admission over that link;
5. releases the Graphshell request stream only after admission accepts.

The browser-supplied launcher argument is useful browser context, not an
OS-authenticated process identity. The current local threat boundary trusts
same-user processes. The Windows device broker verifies the connected client
process token SID against the resident host before it reads the broker hello.
The signed `SessionHello` remains the application admission mechanism.

SSH-key import is a separate native-only interaction after admission. The
extension sends the projection session id and selected unlock policy. The
system picker path, selected bytes, and encrypted-key passphrase stay inside
the native host; the extension receives only a public import receipt or a
bounded failure reason. Import controls remain disabled until that interaction
finishes.

The browser action opens `graph.html`. Its history capture begins off.
`nativeMessaging` and local `storage` are installed capabilities; `history`,
`tabs`, and `webNavigation` remain optional. The current surface requests only
`history`, from the user's **Enable capture** action. Visits are sanitized
against the visible query-string and origin-exclusion settings before entering
the bounded `storage.local` queue. The Wasm host acknowledges them only after
one atomic Muniment batch persists the graph document, LocalOnly AccessRecords,
and Eidetic browsing traces.

Capture attribution comes from the `SelectedPersonaRef` and device reference
injected by the composing host. The reference package displays and records its
stable reference pair. It does not infer an active browsing identity from the
selected Personae vault profile or a device-roster entry.

`bridge.html` remains the admitted native surface and is linked from the graph
portal. Identity signing and key import retain their native-only controls.
An endpoint may also advertise a bounded `input_form`: the bridge renders its
endpoint-supplied labels and advertised choices, then serializes only the
matching declared schema and selected values. It does not infer defaults,
invent values, or interpret a product's reading data.

## Development package

Build the native host:

```text
cargo build -p graphshell --bin graphshell_device_host --bin graphshell_native_host
```

Start `graphshell_device_host` under the platform's resident lifecycle before
opening the extension. On Windows,
`../../install-device-host-windows.ps1` owns the reversible standard-agent
cutover; it requires the expected live fingerprint and preserves the previous
Personae task until the login and logon receipts are explicit.

Prepare one unpacked extension directory:

```text
wasm-bindgen /path/to/graphshell_web.wasm --target web \
  --out-dir ../pkg --out-name graphshell_web
./prepare-extension.ps1 -Browser chromium -Destination ./dist/chromium
./prepare-extension.ps1 -Browser firefox -Destination ./dist/firefox
```

Register the built native host for the current user:

```text
./install-native-host.ps1 -BinaryPath /absolute/path/to/graphshell_native_host.exe
```

The Chromium development manifest has a fixed public key and therefore the
stable id `oajkkocppbpbmfblepgbiidagliniofd`. Firefox uses
`graphshell@mere.systems`. Store release ids can replace these through the
installer without widening the native host's allow-list.

Set `GRAPHSHELL_EXTENSION_IDS` to a comma-separated list only for additional
locally built extension ids. The native host still checks each id exactly.
