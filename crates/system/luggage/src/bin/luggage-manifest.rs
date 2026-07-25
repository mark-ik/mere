//! `luggage-manifest` — build the release manifest a luggage feed serves.
//!
//! `cargo packager` produces an installer and a `.sig` beside it; luggage
//! needs a `luggage.json` describing them. This turns the one into the
//! other, computing the BLAKE3 digest as it goes, and merges into an
//! existing manifest so a multi-platform release is built one host at a
//! time (Windows packs its NSIS entry, the Mac adds its `.app`, Linux its
//! AppImage).
//!
//! Rust, so it runs wherever the app builds.
//!
//! ```text
//! luggage-manifest --artifact <path> --version <semver> --format nsis \
//!     [--target <os-arch>] [--signature <path>] [--url <url>] \
//!     [--out <luggage.json>] [--notes <text>]
//! ```
//!
//! Defaults: `--target` is the host triple luggage would ask for,
//! `--signature` is `<artifact>.sig`, `--url` is a `file://` URL to the
//! artifact (what a directory feed wants), `--out` is `luggage.json` beside
//! the artifact.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::{Map, Value, json};

const USAGE: &str = "\
usage: luggage-manifest --artifact <path> --version <semver> --format <fmt> [options]

  --artifact <path>    the packaged installer/bundle (required)
  --version <semver>   the release version (required)
  --format <fmt>       nsis | wix | app | appimage (required)
  --target <os-arch>   defaults to this host (e.g. windows-x86_64)
  --signature <path>   defaults to <artifact>.sig
  --url <url>          defaults to a file:// url for the artifact
  --out <path>         defaults to luggage.json beside the artifact
  --notes <text>       release notes

Merges into an existing --out, so each host can add its own platform entry.";

struct Args {
    artifact: PathBuf,
    version: String,
    format: String,
    target: Option<String>,
    signature: Option<PathBuf>,
    url: Option<String>,
    out: Option<PathBuf>,
    notes: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut artifact = None;
    let mut version = None;
    let mut format = None;
    let mut target = None;
    let mut signature = None;
    let mut url = None;
    let mut out = None;
    let mut notes = None;

    let mut argv = std::env::args().skip(1);
    while let Some(arg) = argv.next() {
        let mut next = |what: &str| argv.next().ok_or(format!("{what} needs a value"));
        match arg.as_str() {
            "--artifact" => artifact = Some(PathBuf::from(next("--artifact")?)),
            "--version" => version = Some(next("--version")?),
            "--format" => format = Some(next("--format")?),
            "--target" => target = Some(next("--target")?),
            "--signature" => signature = Some(PathBuf::from(next("--signature")?)),
            "--url" => url = Some(next("--url")?),
            "--out" => out = Some(PathBuf::from(next("--out")?)),
            "--notes" => notes = Some(next("--notes")?),
            "--help" | "-h" => return Err(USAGE.to_string()),
            other => return Err(format!("unknown argument {other:?}\n\n{USAGE}")),
        }
    }

    Ok(Args {
        artifact: artifact.ok_or_else(|| format!("--artifact is required\n\n{USAGE}"))?,
        version: version.ok_or_else(|| format!("--version is required\n\n{USAGE}"))?,
        format: format.ok_or_else(|| format!("--format is required\n\n{USAGE}"))?,
        target,
        signature,
        url,
        out,
        notes,
    })
}

fn main() -> ExitCode {
    match run() {
        Ok(path) => {
            println!("wrote {}", path.display());
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("luggage-manifest: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<PathBuf, String> {
    let args = parse_args()?;

    if !args.artifact.is_file() {
        return Err(format!("no artifact at {}", args.artifact.display()));
    }
    let bytes = std::fs::read(&args.artifact)
        .map_err(|e| format!("read {}: {e}", args.artifact.display()))?;
    let digest = blake3::hash(&bytes).to_hex().to_string();

    let signature_path = args
        .signature
        .unwrap_or_else(|| with_extra_extension(&args.artifact, "sig"));
    let signature = std::fs::read_to_string(&signature_path)
        .map_err(|e| {
            format!(
                "read signature {}: {e} (pack with a signing key so cargo-packager emits it)",
                signature_path.display()
            )
        })?
        .trim()
        .to_string();
    if signature.is_empty() {
        return Err(format!("{} is empty", signature_path.display()));
    }

    let target = match args.target {
        Some(target) => target,
        None => luggage::target()
            .ok_or_else(|| "could not determine this host's target; pass --target".to_string())?,
    };

    let url = match args.url {
        Some(url) => url,
        None => url::Url::from_file_path(
            args.artifact
                .canonicalize()
                .map_err(|e| format!("canonicalize {}: {e}", args.artifact.display()))?,
        )
        .map_err(|_| format!("could not build a file url for {}", args.artifact.display()))?
        .to_string(),
    };

    let out = args.out.unwrap_or_else(|| {
        args.artifact
            .parent()
            .unwrap_or(Path::new("."))
            .join(luggage::MANIFEST_NAME)
    });

    // Merge into an existing manifest when the version matches, so a
    // release can be assembled one host at a time. A different version
    // replaces it wholesale: mixing artifacts across versions in one
    // manifest would ship a peer the wrong bytes.
    let mut platforms = Map::new();
    if let Ok(existing) = std::fs::read(&out) {
        if let Ok(Value::Object(manifest)) = serde_json::from_slice::<Value>(&existing) {
            let same_version = manifest
                .get("version")
                .and_then(Value::as_str)
                .map(|v| v.trim_start_matches('v') == args.version.trim_start_matches('v'))
                .unwrap_or(false);
            if same_version {
                if let Some(Value::Object(existing_platforms)) = manifest.get("platforms") {
                    platforms = existing_platforms.clone();
                }
            } else {
                println!(
                    "note: replacing manifest for a different version ({})",
                    manifest
                        .get("version")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                );
            }
        }
    }

    platforms.insert(
        target.clone(),
        json!({
            "url": url,
            "signature": signature,
            "blake3": digest,
            "format": args.format,
        }),
    );

    let mut manifest = Map::new();
    manifest.insert("version".into(), json!(args.version));
    if let Some(notes) = args.notes {
        manifest.insert("notes".into(), json!(notes));
    }
    manifest.insert("platforms".into(), Value::Object(platforms));

    let rendered = serde_json::to_string_pretty(&Value::Object(manifest))
        .map_err(|e| format!("render manifest: {e}"))?;
    std::fs::write(&out, rendered).map_err(|e| format!("write {}: {e}", out.display()))?;

    println!("  version:  {}", args.version);
    println!("  target:   {target}");
    println!("  blake3:   {digest}");

    // The manifest must be signed too, or a feed controller can rewrite the
    // version around a genuinely-signed artifact and roll clients backwards.
    // Updaters refuse an unsigned manifest by default, so say the next step
    // out loud rather than leaving it to be discovered as a failure.
    if !out.with_extension("json.sig").exists() {
        println!(
            "\nnow sign it, or the feed will be refused:\n  cargo packager signer sign {}",
            out.display()
        );
    }
    Ok(out)
}

/// `foo.exe` -> `foo.exe.sig` (append, not replace: cargo-packager writes
/// the signature beside the artifact with the extra extension).
fn with_extra_extension(path: &Path, extra: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(extra);
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_path_appends_rather_than_replaces() {
        let path = with_extra_extension(Path::new("/tmp/app_0.1.0_x64-setup.exe"), "sig");
        assert!(path.ends_with("app_0.1.0_x64-setup.exe.sig"));
    }
}
