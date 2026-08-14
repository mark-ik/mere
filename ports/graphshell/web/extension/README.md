# Graphshell browser extension

The Chromium and Firefox extension that carries Graphshell's Wasm portal, plus
the `org.mere.graphshell` native-messaging registration that connects it to the
resident device host.

## Files

| File | Contents |
|---|---|
| `manifest.chromium.json` | MV3 manifest: `background.js` service worker, fixed public `key` |
| `manifest.firefox.json` | MV3 manifest: `capture-model.js` + `background.js` background scripts, add-on id |
| `background.js` | Browser action opens `graph.html`; history and permission listeners; the bounded `storage.local` capture queue |
| `capture-model.js` | `GraphshellCaptureModel`: default policy, visit sanitization, queue bounds |
| `action-form.js` | `GraphshellActionForm`: builds an intent payload from an advertised `input_form` |
| `bridge.html`, `bridge.css`, `bridge.js`, `resource-chunk.js` | The native surface: connects `org.mere.graphshell`, pulls bounded resource chunks, renders identity cards and advertised actions |
| `prepare-extension.ps1`, `prepare-extension.sh` | Assemble one unpacked extension directory |
| `install-native-host.ps1`, `install-native-host.sh` | Register the native host for the current user |
| `native-host.chromium.json.in`, `native-host.firefox.json.in` | Native-host manifest templates; `__GRAPHSHELL_NATIVE_HOST__` is replaced with the binary path |
| `smoke-action-form.mjs`, `smoke-capture-model.mjs`, `smoke-capture-background.mjs`, `smoke-native-host.mjs`, `smoke-transfer-accept.mjs`, `smoke-resource-chunk.mjs` | Node smoke tests |

`prepare-extension` also copies `index.html` (as `graph.html`), `styles.css`,
`loader.js`, `extension-profile.js`, `GraphshellSans.ttf`, and `pkg/` from the
parent `web/` directory into the destination. `dist/` is gitignored, so it is
the usual destination.

## Identities

| Browser | Extension id |
|---|---|
| Chromium | `oajkkocppbpbmfblepgbiidagliniofd`, derived from the manifest `key` |
| Firefox | `graphshell@mere.systems` |

The same values are `graphshell::browser_carrier::CHROMIUM_EXTENSION_ID` and
`FIREFOX_EXTENSION_ID`. Set `GRAPHSHELL_EXTENSION_IDS` to a comma-separated
list to add locally built ids; the native host matches each id exactly. Store
release ids can replace the defaults through the installer.

## Permissions

`nativeMessaging` and `storage` are installed permissions. `history`, `tabs`,
and `webNavigation` are optional; the portal's **Enable capture** control
requests `history`. The default policy in `capture-model.js` has `enabled:
false`. Visits are sanitized against the query-string and origin-exclusion
settings before they enter the `storage.local` queue, and the Wasm host
acknowledges them after one Muniment batch persists the graph document, the
`LocalOnly` AccessRecords, and the Eidetic browsing traces.

Capture attribution comes from the `SelectedPersonaRef` and device reference
injected by the composing host, not from the selected vault profile or a
device-roster entry.

## Admission

The extension and `graphshell_native_host` hold presentation and connection
state. The selected Personae profile, vault, SSH keys, delegation issuer, and
`SessionHello` signer stay in `graphshell_device_host`.

Installing the native-host manifest pairs one exact extension id with the relay
binary. The relay connects to the per-user device broker, and each connection:

1. receives a fresh native-host challenge;
2. answers with that challenge and a fresh browser nonce;
3. derives a link id from the browser launcher id and both nonces;
4. runs signed `SessionHello` admission over that link;
5. releases the Graphshell request stream after admission accepts.

Native messaging frames are capped at `MAX_NATIVE_MESSAGE_BYTES` (1 MiB). On
Windows the device broker compares the connecting client's process token SID
with its own before reading the broker hello.

SSH-key import is a native-only interaction after admission. The extension
sends the projection session id and selected unlock policy; the picker path,
key bytes, and passphrase stay in the native host, and the extension receives a
public import receipt or a bounded failure reason.

`bridge.html` is the admitted native surface and is linked from the graph
portal as **Open Identity**. An endpoint may advertise a bounded `input_form`;
the bridge renders its labels and choices and serializes the declared schema
with the selected values.

## Building

Build both hosts. `graphshell_device_host` requires `personal-sync`, which
includes `native`:

```text
cargo build -p graphshell --features personal-sync \
  --bin graphshell_device_host --bin graphshell_native_host
```

Start `graphshell_device_host` under the platform's resident lifecycle before
opening the extension. On Windows,
[`../../install-device-host-windows.ps1`](../../install-device-host-windows.ps1)
performs the reversible standard-agent cutover; it requires the expected live
fingerprint and preserves the previous Personae task.

Prepare an unpacked extension directory:

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
