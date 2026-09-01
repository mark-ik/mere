# luggage

Self-update for apps packaged by
[`cargo-packager`](https://docs.rs/cargo-packager): check a feed for a signed
release manifest, download and verify it, install per platform. Packing itself
still uses upstream's `cargo-packager` CLI unchanged.

Forked 2026-07-24 from
[`cargo-packager-updater` 0.2.3](https://crates.io/crates/cargo-packager-updater)
(Tauri Programme within The Commons Conservancy / CrabNebula Ltd,
MIT OR Apache-2.0). Upstream copyright headers are preserved on derived files.

```rust
use luggage::{check_update, Config, Feed};

let config = Config {
    feeds: vec![Feed::parse("github:merely-made/hocket").unwrap()],
    pubkey: "<minisign public key>".into(),
    ..Default::default()
};
if let Some(update) = check_update("0.1.0".parse().unwrap(), config)? {
    update.download_and_install()?;
}
```

## Public API

Every module is private; the whole surface is re-exported at the crate root, so
a caller writes `luggage::Config`. The `Source` column names the source file;
`Feature` says whether the item survives `default-features = false` (see
[Feature split](#feature-split)).

| Item | Source | Feature | What it is |
| --- | --- | --- | --- |
| `check_update(Version, Config)` | `lib.rs` | `native` | One call: build an `Updater`, check the feeds, return `Option<Update>` |
| `Config` | `config` | `native` | `feeds`, `pubkey`, `windows`, `require_signed_manifest` (defaults to `true`) |
| `Feed` | `config` | `native` | `Http(Url)`, `Directory(PathBuf)`, `GitHub { owner, repo }`; built by `Feed::parse` |
| `WindowsConfig`, `WindowsUpdateInstallMode` | `config` | `native` | Extra installer args; `BasicUi` / `Quiet` / `Passive` (default), with `msiexec_args` and `nsis_args` |
| `UpdaterBuilder`, `Updater`, `target()` | `updater` | `native` | Builder form: `version_comparator`, `pub_key`, `target`, `feeds`, `executable_path`, `header`, `timeout`, `installer_args`; `Updater::check` |
| `Update` | `install` | `native` | `download`, `download_extended`, `install`, `download_and_install`, `download_and_install_extended`, plus `stage` and `install_staged` |
| `StagedUpdate` | `staging` | `native` | `load`, `take_verified`, `version`, `format`, `extract_path`, `discard` |
| `ReleaseRefV1` | `release` | **core** | A reference naming one signed release: `manifest_blake3` + `publisher_key_id`, and nothing else. Needs no dependency, so it reaches wasm32. Display and lookup only — never an authorization |
| `RemoteRelease`, `RemoteReleaseData`, `ReleaseManifestPlatform`, `UpdateFormat`, `MANIFEST_NAME` | `release` | **core** | Manifest types. `UpdateFormat` is `Nsis` / `Wix` / `AppImage` / `App`; `MANIFEST_NAME` is `"luggage.json"` |
| `Error`, `Result` | `error` | **core** | `thiserror` enum and the crate alias |

`semver` and `url` are re-exported in every build, and `http` and `reqwest`
under `native`, so a caller can name a `Url` or a `Version` without adding
those crates.

## Feature split

`native` is on by default and carries the whole update pipeline — everything
that touches the network, the filesystem, or the host process. Nothing changes
for a caller who does not opt out.

`default-features = false` leaves the **release-identity core**: the `release`
module's manifest types and the error enum, with no I/O. It compiles for
`wasm32-unknown-unknown`, which is what lets a browser build name a release
without carrying an updater it could never run — and without a second,
hand-maintained copy of the release types drifting against this one.

| | Core | Native (default) |
| --- | --- | --- |
| modules | `error`, `release` | + `config`, `signing`, `install`, `staging`, `updater` |
| direct deps | `semver`, `serde`, `serde_json`, `thiserror`, `time`, `url` | + 8 more |
| crates resolved (wasm32 / host) | 103 | 261 |
| `luggage-manifest` binary | no (`required-features`) | yes |

The error enum is `#[non_exhaustive]`, and the five variants carrying
native-only types (`Reqwest`, `Http`, `PersistError`, `Minisign`, `Base64`) are
gated with the feature, so their absence cannot break a downstream match.

Verification is **not** in the core. The minisign and BLAKE3 checks are
`pub(crate)` helpers the native pipeline drives, so they stayed with it; a core
consumer can name and compare a release but not verify one. Exporting a
verification entry point is a public-API decision, deliberately left open.

## Feeds

| Form | Parsed as | Behaviour |
| --- | --- | --- |
| `https://host/path` | `Feed::Http` | Upstream semantics: `{{target}}`, `{{arch}}`, `{{current_version}}` templating; 204 means no update, 200 carries the release JSON |
| `/srv/updates`, `C:/feed`, `file://...` | `Feed::Directory` | A local path holding `luggage.json`. Artifact URLs are absolute `file://` URLs |
| `github:owner/repo` | `Feed::GitHub` | Resolves to `https://github.com/owner/repo/releases/latest/download/luggage.json` |

Feeds are checked in order; the first that yields a release wins.

## Manifest

`luggage.json`, the shape upstream documents plus the optional per-platform
`blake3` digest, verified before the minisign signature.

```json
{
  "version": "0.2.0",
  "pub_date": "2026-07-24T12:00:00Z",
  "platforms": {
    "windows-x86_64": {
      "url": "https://github.com/you/app/releases/download/v0.2.0/app-setup.exe",
      "signature": "<contents of the .sig>",
      "blake3": "<hex digest of the artifact>",
      "format": "nsis"
    }
  }
}
```

## Signatures

Two minisign signatures are checked per release: the per-platform `signature`
over the artifact bytes, and a detached `luggage.json.sig` over the manifest.
`Config::require_signed_manifest` defaults to `true`, so a feed serving no
`luggage.json.sig` is refused. Sign the manifest after every host has added its
platform entry; any later edit invalidates it.

```sh
cargo packager signer sign <feed>/luggage.json
```

Both `.sig` conventions are accepted: the raw minisign text block `minisign -S`
writes, and the base64-wrapped form `cargo packager signer sign` writes.

## Staging

`Update::stage(dir, bytes)` writes a verified artifact plus a `staged.json`
record into `dir`, so a pending update survives the app closing.
`StagedUpdate::load(dir)` picks it up on the next launch and
`StagedUpdate::take_verified` re-hashes the file before it is applied. Staging
requires the manifest to have carried a `blake3` digest and errors otherwise.

## luggage-manifest

A binary that turns a `cargo packager` artifact plus its `.sig` into a manifest
entry, computing the BLAKE3 digest and merging into an existing `luggage.json`
so a multi-platform release is assembled one host at a time.

```text
luggage-manifest --artifact <path> --version <semver> --format nsis \
    [--target <os-arch>] [--signature <path>] [--url <url>] \
    [--out <luggage.json>] [--notes <text>]
```

Defaults: `--target` is the host triple, `--signature` is `<artifact>.sig`,
`--url` is a `file://` URL to the artifact, `--out` is `luggage.json` beside it.

## Dependencies

Core (always): `semver`, `serde` / `serde_json`, `thiserror`, `time`, `url`.

Behind `native`: `reqwest` with `rustls-tls` and no default features,
`minisign-verify`, `blake3`, `http`, `dirs`, `tempfile`, `percent-encoding`,
`base64`, `log`, and upstream's `cargo-packager-utils` for `current_exe`
resolution. macOS additionally pulls `flate2` and `tar` for `.app.tar.gz`.
Tests use `minisign` to sign in-process.

## License

MPL-2.0 (see [`LICENSE`](../../../LICENSE)).

A substantial derivative of
[`cargo-packager-updater`](https://crates.io/crates/cargo-packager-updater)
(Tauri Programme within The Commons Conservancy / CrabNebula Ltd,
MIT OR Apache-2.0), relicensed under the 2026-08-22 license posture ruling
with the upstream copyright notices retained verbatim on every derived file.
Both MIT and Apache-2.0 permit this provided the notice travels with the work.
Recorded in [`LICENSES.md`](../../../LICENSES.md).

Published version 0.1.0 carries MIT OR Apache-2.0 permanently; MPL-2.0 ships
at the next functional bump.
