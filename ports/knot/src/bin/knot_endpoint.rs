use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use p2panda_core::SigningKey;
use stickleback::DataKeyring;

fn main() {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let mut endpoint = match args.as_slice() {
        [] => knot::KnotEndpoint::fixture(),
        [root] => knot::KnotEndpoint::open(PathBuf::from(root))
            .expect("Knot could not open the requested directory"),
        [mode, root] if mode == "directory" => knot::KnotEndpoint::open(PathBuf::from(root))
            .expect("Knot could not open the requested directory"),
        [mode, root, max_source_bytes] if mode == "directory-write" => {
            let max_source_bytes = max_source_bytes
                .to_string_lossy()
                .parse::<u64>()
                .expect("directory-write byte limit must be an integer");
            knot::KnotEndpoint::open_writable(
                PathBuf::from(root),
                knot::KnotWriteGrant::new(max_source_bytes),
            )
            .expect("Knot could not open the requested writable directory")
        }
        [
            mode,
            root,
            max_source_bytes,
            resolve,
            run,
            schemes,
            languages,
            max_depth,
            max_ops,
        ] if mode == "directory-write-effects" => {
            let root = PathBuf::from(root);
            let mut endpoint = knot::KnotEndpoint::open_writable(
                &root,
                knot::KnotWriteGrant::new(parse_u64(
                    max_source_bytes,
                    "directory-write byte limit",
                )),
            )
            .expect("Knot could not open the requested writable directory");
            endpoint.grant_effects(effect_authority(
                resolve,
                run,
                schemes,
                languages,
                max_depth,
                max_ops,
                Some(&root),
            ));
            endpoint
        }
        [mode, data_root, persona, max_source_bytes] if mode == "persona-vault" => {
            let persona = persona
                .to_string_lossy()
                .parse::<uuid::Uuid>()
                .map(personae::PersonaId::from_uuid)
                .expect("persona-vault persona must be a UUID");
            let max_source_bytes = max_source_bytes
                .to_string_lossy()
                .parse::<u64>()
                .expect("persona-vault byte limit must be an integer");
            knot::StartupUnlockedPersonalVault::open(PathBuf::from(data_root), persona)
                .and_then(|authority| {
                    authority.into_endpoint(knot::KnotWriteGrant::new(max_source_bytes))
                })
                .expect("Knot could not startup-unlock the requested persona vault")
        }
        [
            mode,
            data_root,
            persona,
            max_source_bytes,
            resolve,
            run,
            schemes,
            languages,
            max_depth,
            max_ops,
        ] if mode == "persona-vault-effects" => {
            let persona = persona
                .to_string_lossy()
                .parse::<uuid::Uuid>()
                .map(personae::PersonaId::from_uuid)
                .expect("persona-vault persona must be a UUID");
            let mut endpoint =
                knot::StartupUnlockedPersonalVault::open(PathBuf::from(data_root), persona)
                    .and_then(|authority| {
                        authority.into_endpoint(knot::KnotWriteGrant::new(parse_u64(
                            max_source_bytes,
                            "persona-vault byte limit",
                        )))
                    })
                    .expect("Knot could not startup-unlock the requested persona vault");
            endpoint.grant_effects(effect_authority(
                resolve, run, schemes, languages, max_depth, max_ops, None,
            ));
            endpoint
        }
        [
            mode,
            root,
            max_source_bytes,
            resolve,
            run,
            schemes,
            languages,
            max_depth,
            max_ops,
        ] if mode == "communal-fixture-effects" => communal_fixture_endpoint(
            PathBuf::from(root),
            parse_u64(max_source_bytes, "communal fixture byte limit"),
            effect_authority(resolve, run, schemes, languages, max_depth, max_ops, None),
        ),
        _ => panic!(
            "usage: knot_endpoint [directory] | directory <root> | \
             directory-write <root> <max-source-bytes> | \
             directory-write-effects <root> <max-source-bytes> \
             <resolve-mode> <run-mode> <schemes> <languages> <max-depth> <max-ops> | \
             persona-vault <data-root> <persona-id> <max-source-bytes> | \
             persona-vault-effects <data-root> <persona-id> <max-source-bytes> \
             <resolve-mode> <run-mode> <schemes> <languages> <max-depth> <max-ops> | \
             communal-fixture-effects <root> <max-source-bytes> \
             <resolve-mode> <run-mode> <schemes> <languages> <max-depth> <max-ops>"
        ),
    };
    graphshell_stdio::serve_resumable_notifying(
        &mut endpoint,
        std::io::stdin(),
        std::io::stdout().lock(),
        Duration::from_millis(100),
    )
    .expect("Knot Graphshell endpoint failed");
}

/// Process-only received-content fixture. Its keys are minted inside the
/// endpoint process and the root is caller-owned scratch storage, so this mode
/// does not turn group keys into CLI or environment authority.
fn communal_fixture_endpoint(
    root: PathBuf,
    max_source_bytes: u64,
    effects: knot::KnotEffectAuthority,
) -> knot::KnotEndpoint {
    const SPACE: [u8; 32] = [0xC1; 32];
    const RECEIVED_SEED: [u8; 32] = [0xC2; 32];
    const LOCAL_SEED: [u8; 32] = [0xC3; 32];
    const VAULT_KEY: [u8; 32] = [0xC4; 32];

    fs::create_dir_all(&root).expect("could not create communal fixture root");
    let received_writer = *SigningKey::from_bytes(&RECEIVED_SEED)
        .verifying_key()
        .as_bytes();
    let local_writer = *SigningKey::from_bytes(&LOCAL_SEED)
        .verifying_key()
        .as_bytes();
    let store = knot::KnotSyncFileStore::open_commons(
        root.join("commons.redb"),
        SPACE,
        [received_writer, local_writer],
    )
    .expect("could not open communal fixture sync store");
    let mut keys = DataKeyring::new();
    keys.rotate_random()
        .expect("could not mint communal fixture data epoch");
    let source = "\
# Received calculation

```rhai eval
40 + 2
```
";
    pollster::block_on(store.author_communal(
        RECEIVED_SEED,
        &keys,
        &knot::KnotSyncEvent::Put(knot::VaultDocument {
            id: "received".into(),
            title: "Received calculation".into(),
            body: source.as_bytes().to_vec(),
            media_type: "text/vnd.knot".into(),
        }),
    ))
    .expect("could not author received communal fixture document");
    let vault =
        knot::KnotVault::open(root.join("vault"), VAULT_KEY).expect("could not open fixture vault");
    let mut endpoint = knot::KnotEndpoint::from_communal_vault(
        vault,
        store,
        LOCAL_SEED,
        keys,
        knot::KnotWriteGrant::new(max_source_bytes),
    )
    .expect("could not open communal fixture endpoint");
    endpoint.grant_effects(effects);
    endpoint
}

fn parse_u64(value: &OsStr, label: &str) -> u64 {
    value
        .to_string_lossy()
        .parse()
        .unwrap_or_else(|_| panic!("{label} must be an integer"))
}

fn parse_effect_mode(value: &OsStr) -> knot::KnotEffectMode {
    match value.to_string_lossy().as_ref() {
        "auto" => knot::KnotEffectMode::Auto,
        "ask" => knot::KnotEffectMode::Ask,
        "never" => knot::KnotEffectMode::Never,
        other => panic!("effect mode must be auto, ask, or never; got {other}"),
    }
}

fn parse_csv(value: &OsStr) -> Vec<String> {
    value
        .to_string_lossy()
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn effect_authority(
    resolve: &OsStr,
    run: &OsStr,
    schemes: &OsStr,
    languages: &OsStr,
    max_depth: &OsStr,
    max_ops: &OsStr,
    file_root: Option<&Path>,
) -> knot::KnotEffectAuthority {
    let policy = knot::KnotEffectPolicy {
        resolve: parse_effect_mode(resolve),
        run: parse_effect_mode(run),
        allowed_schemes: parse_csv(schemes),
        allowed_languages: parse_csv(languages),
        max_depth: parse_u64(max_depth, "effect max depth")
            .try_into()
            .expect("effect max depth must fit in u8"),
        max_ops: parse_u64(max_ops, "effect operation limit"),
    };
    let has_file = policy.allowed_schemes.iter().any(|scheme| scheme == "file");
    let has_rhai = policy
        .allowed_languages
        .iter()
        .any(|language| language == "rhai");
    let mut authority = knot::KnotEffectAuthority::new(policy);
    if has_file {
        let root = file_root.expect("file effects require a directory-root endpoint");
        authority = authority
            .with_fetcher(RootedFileFetcher::new(root).expect("could not admit effect file root"));
    }
    if has_rhai {
        authority = authority.register_evaluator(script_rhai::RhaiEvaluator::new());
    }
    authority
}

struct RootedFileFetcher {
    root: PathBuf,
}

impl RootedFileFetcher {
    fn new(root: &Path) -> Result<Self, String> {
        Ok(Self {
            root: fs::canonicalize(root)
                .map_err(|error| format!("could not canonicalize file effect root: {error}"))?,
        })
    }
}

impl knot::KnotEffectFetcher for RootedFileFetcher {
    fn fetch(&mut self, address: &str) -> Result<inker::Fetched, String> {
        let url = url::Url::parse(address)
            .map_err(|error| format!("could not parse effect address {address}: {error}"))?;
        if url.scheme() != "file" {
            return Err(format!("Knot has no fetch provider for {address}"));
        }
        let path = url
            .to_file_path()
            .map_err(|_| format!("could not map {address} to a local file"))?;
        let path = fs::canonicalize(&path)
            .map_err(|error| format!("could not resolve {}: {error}", path.display()))?;
        if !path.starts_with(&self.root) {
            return Err(format!(
                "{} is outside the admitted Knot directory",
                path.display()
            ));
        }
        let body = fs::read_to_string(&path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        Ok(inker::Fetched {
            content_type: content_type(&path),
            body,
        })
    }
}

fn content_type(path: &Path) -> Option<String> {
    match path
        .extension()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("gmi" | "gemini") => Some("text/gemini".into()),
        Some("md" | "markdown") => Some("text/markdown".into()),
        Some("knot") => Some("text/x-knot".into()),
        Some("txt") => Some("text/plain".into()),
        _ => None,
    }
}
