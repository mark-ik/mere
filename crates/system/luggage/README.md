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
a caller writes `luggage::Config`. The `Source` column names the source file.

| Item | Source | What it is |
| --- | --- | --- |
| `check_update(Version, Config)` | `lib.rs` | One call: build an `Updater`, check the feeds, return `Option<Update>` |
| `Config` | `config` | `feeds`, `pubkey`, `windows`, `require_signed_manifest` (defaults to `true`) |
| `Feed` | `config` | `Http(Url)`, `Directory(PathBuf)`, `GitHub { owner, repo }`; built by `Feed::parse` |
| `WindowsConfig`, `WindowsUpdateInstallMode` | `config` | Extra installer args; `BasicUi` / `Quiet` / `Passive` (default), with `msiexec_args` and `nsis_args` |
| `UpdaterBuilder`, `Updater`, `target()` | `updater` | Builder form: `version_comparator`, `pub_key`, `target`, `feeds`, `executable_path`, `header`, `timeout`, `installer_args`; `Updater::check` |
| `Update` | `install` | `download`, `download_extended`, `install`, `download_and_install`, `download_and_install_extended`, plus `stage` and `install_staged` |
| `StagedUpdate` | `staging` | `load`, `take_verified`, `version`, `format`, `extract_path`, `discard` |
| `RemoteRelease`, `RemoteReleaseData`, `ReleaseManifestPlatform`, `UpdateFormat`, `MANIFEST_NAME` | `release` | Manifest types. `UpdateFormat` is `Nsis` / `Wix` / `AppImage` / `App`; `MANIFEST_NAME` is `"luggage.json"` |
| `Error`, `Result` | `error` | `thiserror` enum and the crate alias |

`http`, `reqwest`, `semver` and `url` are re-exported too, so a caller can name
a `Url` or a `Version` without adding those crates.

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

`reqwest` with `rustls-tls` and no default features, `minisign-verify`,
`blake3`, `semver`, `serde` / `serde_json`, `url`, `http`, `time`, `dirs`,
`tempfile`, `percent-encoding`, `base64`, `log`, `thiserror`, and upstream's
`cargo-packager-utils` for `current_exe` resolution. macOS additionally pulls
`flate2` and `tar` for `.app.tar.gz`. Tests use `minisign` to sign in-process.
