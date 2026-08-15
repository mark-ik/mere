# Graphshell session stack

Mere's reusable remote-session machinery: a versioned session protocol, the
client state above it, the traits an authority implements, and one crate per
carrier.

| Crate | Contents |
|---|---|
| `chirograph` | Session messages (score, scene, presentation, resource, resume, status, intent); the `Carrier` trait; `CarrierError` |
| `graphshell-client` | `ClientState` (snapshots, diffs, resume, resource cache, cache policy), `RetainedEndpointSession`, `ActionDraft` |
| `graphshell-endpoint` | `ProjectionCatalog`, `ProjectionSource`, `PresentationSource`, `IntentSink`, `ResumableProjectionSource`, `ProjectionNoticeSource`, `LiveViewReferenceGate`, `dispatch_common` |
| `graphshell-stdio` | `StdioCarrier` plus `serve_basic`, `serve_resumable`, `serve_resumable_notifying`: NDJSON over a child process's standard streams |
| `graphshell-local` | `LocalCarrier`: an endpoint hosted in this process, still round-tripping the wire encoding |
| `graphshell-network` | `NetworkCarrier`, `CarrierRuntime`: NDJSON over any `AsyncRead + AsyncWrite` |

`Carrier::request` returns `Result<CarrierResponseBody, CarrierError>`.
`CarrierError` is `Refused` (the session is intact) or `Disconnected` (the
session is finished). `StdioCarrier`, `LocalCarrier`, and `NetworkCarrier` each
implement `Carrier`.

## Dependencies

| Crate | Depends on |
|---|---|
| `chirograph` | `sceno`, `scenotime`, `serde`, `serde_json`, `blake3`, `base64` |
| `graphshell-client` | `chirograph`, `sceno`, `scenotime`, `serde`, `serde_json` |
| `graphshell-endpoint` | `chirograph` |
| `graphshell-stdio` | `graphshell-endpoint`, `chirograph`, `serde_json`, `std::process` |
| `graphshell-local` | `graphshell-endpoint`, `chirograph`, `serde`, `serde_json` |
| `graphshell-network` | `chirograph`, `serde_json`, Tokio (`io-util`, `rt`, `rt-multi-thread`) |

`chirograph`, `-client`, and `-endpoint` build for
`wasm32-unknown-unknown`. `NetworkCarrier`'s `Carrier` methods block, so they
must run off a runtime worker thread.

## Not in these crates

Admission: which peers may open a session, under what grant, and over which
ALPN. That lives in [`ports/graphshell`](../../ports/graphshell), along with the
serve loops for admitted sessions, `ResidentEndpointCatalog`, and
`ResidentProjectionHost`. That port is the reference application: it composes
these crates, and they do not depend on it.
