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
                max_source_bytes,
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
                resolve,
                run,
                schemes,
                languages,
                max_depth,
                max_ops,
                max_source_bytes,
                None,
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
            effect_authority(
                resolve,
                run,
                schemes,
                languages,
                max_depth,
                max_ops,
                max_source_bytes,
                None,
            ),
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
    max_fetch_bytes: &OsStr,
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
    assert!(
        !policy
            .allowed_schemes
            .iter()
            .any(|scheme| scheme == "titan"),
        "Titan is an upload protocol and cannot be admitted as a Knot read effect",
    );
    let has_file = policy.allowed_schemes.iter().any(|scheme| scheme == "file");
    let has_network = policy
        .allowed_schemes
        .iter()
        .any(|scheme| is_read_network_scheme(scheme));
    let has_rhai = policy
        .allowed_languages
        .iter()
        .any(|language| language == "rhai");
    let mut authority = knot::KnotEffectAuthority::new(policy);
    if has_file || has_network {
        let file = has_file.then(|| {
            RootedFileFetcher::new(
                file_root.expect("file effects require a directory-root endpoint"),
            )
            .expect("could not admit effect file root")
        });
        let network = has_network.then(|| {
            NetworkEffectFetcher::new(
                parse_u64(max_fetch_bytes, "effect fetch byte limit")
                    .try_into()
                    .expect("effect fetch byte limit must fit in usize"),
            )
            .expect("could not start Knot network effect provider")
        });
        authority = authority.with_fetcher(RoutedEffectFetcher { file, network });
    }
    if has_rhai {
        authority = authority.register_evaluator(script_rhai::RhaiEvaluator::new());
    }
    authority
}

fn is_read_network_scheme(scheme: &str) -> bool {
    matches!(
        scheme,
        "http" | "https" | "gemini" | "gopher" | "finger" | "spartan" | "nex" | "guppy"
    )
}

struct RoutedEffectFetcher {
    file: Option<RootedFileFetcher>,
    network: Option<NetworkEffectFetcher>,
}

impl knot::KnotEffectFetcher for RoutedEffectFetcher {
    fn fetch(&mut self, address: &str) -> Result<inker::Fetched, String> {
        let scheme = url::Url::parse(address)
            .map_err(|error| format!("could not parse effect address {address}: {error}"))?
            .scheme()
            .to_ascii_lowercase();
        match scheme.as_str() {
            "file" => self
                .file
                .as_mut()
                .ok_or_else(|| "Knot has no file effect provider".to_string())?
                .fetch(address),
            scheme if is_read_network_scheme(scheme) => self
                .network
                .as_mut()
                .ok_or_else(|| format!("Knot has no {scheme} effect provider"))?
                .fetch(address),
            _ => Err(format!("Knot has no fetch provider for {address}")),
        }
    }

    fn cache_version(&self) -> String {
        let network = self
            .network
            .as_ref()
            .map(knot::KnotEffectFetcher::cache_version)
            .unwrap_or_else(|| "none".into());
        format!(
            "knot.routed-effect-fetcher/v1;file={};network={network}",
            self.file.is_some()
        )
    }
}

struct NetworkEffectFetcher {
    runtime: tokio::runtime::Runtime,
    max_bytes: usize,
}

impl NetworkEffectFetcher {
    fn new(max_bytes: usize) -> Result<Self, String> {
        fetch::install_in_memory_smolweb_tofu();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("could not build effect fetch runtime: {error}"))?;
        Ok(Self { runtime, max_bytes })
    }
}

impl knot::KnotEffectFetcher for NetworkEffectFetcher {
    fn fetch(&mut self, address: &str) -> Result<inker::Fetched, String> {
        let fetched = self
            .runtime
            .block_on(fetch::fetch_page_anonymous_capped(address, self.max_bytes))?;
        Ok(inker::Fetched {
            content_type: fetched.content_type,
            body: fetched.body,
        })
    }

    fn cache_version(&self) -> String {
        format!(
            "knot.anonymous-network-effect-fetcher/v1;max-bytes={}",
            self.max_bytes
        )
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn serve_http_cookie_probe(body: &'static str) -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind HTTP fixture");
        let address = listener.local_addr().expect("read HTTP fixture address");
        let handle = thread::spawn(move || {
            let mut requests = Vec::new();
            for seed_session in [true, false] {
                let (mut stream, _) = listener.accept().expect("accept HTTP fixture");
                let mut request = [0_u8; 4096];
                let read = stream.read(&mut request).expect("read HTTP request");
                let set_cookie = if seed_session {
                    "Set-Cookie: knot-browser-authority=secret; Path=/\r\n"
                } else {
                    ""
                };
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/markdown\r\n{set_cookie}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body,
                )
                .expect("write HTTP response");
                requests.push(String::from_utf8_lossy(&request[..read]).into_owned());
            }
            requests
        });
        (format!("http://{address}/note"), handle)
    }

    fn serve_gopher_once(body: &'static str) -> (String, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind Gopher fixture");
        let address = listener.local_addr().expect("read Gopher fixture address");
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept Gopher fixture");
            let mut request = [0_u8; 256];
            let read = stream.read(&mut request).expect("read Gopher request");
            stream
                .write_all(body.as_bytes())
                .expect("write Gopher response");
            String::from_utf8_lossy(&request[..read]).into_owned()
        });
        (format!("gopher://{address}/0note"), handle)
    }

    #[test]
    fn network_effect_fetcher_reads_anonymous_http() {
        let (url, server) = serve_http_cookie_probe("# from HTTP\n");
        let mut fetcher = NetworkEffectFetcher::new(1024).expect("start fetcher");
        fetcher
            .runtime
            .block_on(fetch::fetch_page_capped(&url, 1024))
            .expect("seed browser session cookie");
        let fetched =
            knot::KnotEffectFetcher::fetch(&mut fetcher, &url).expect("fetch HTTP effect");

        assert_eq!(fetched.content_type.as_deref(), Some("text/markdown"));
        assert_eq!(fetched.body, "# from HTTP\n");
        let requests = server.join().expect("join HTTP fixture");
        assert_eq!(requests.len(), 2);
        assert!(requests[1].starts_with("GET /note HTTP/"));
        assert!(
            !requests[1].to_ascii_lowercase().contains("\r\ncookie:"),
            "effect fetch must not borrow browser cookies: {}",
            requests[1],
        );
    }

    #[test]
    fn network_effect_fetcher_reads_gopher_as_plain_text() {
        let (url, server) = serve_gopher_once("from Gopher\r\n");
        let mut fetcher = NetworkEffectFetcher::new(1024).expect("start fetcher");
        let fetched =
            knot::KnotEffectFetcher::fetch(&mut fetcher, &url).expect("fetch Gopher effect");

        assert_eq!(fetched.content_type.as_deref(), Some("text/plain"));
        assert_eq!(fetched.body, "from Gopher\r\n");
        assert_eq!(server.join().expect("join Gopher fixture"), "note\r\n");
    }

    #[test]
    fn effect_cache_identity_binds_the_fetch_byte_cap() {
        let small = RoutedEffectFetcher {
            file: None,
            network: Some(NetworkEffectFetcher::new(1024).expect("start small fetcher")),
        };
        let large = RoutedEffectFetcher {
            file: None,
            network: Some(NetworkEffectFetcher::new(2048).expect("start large fetcher")),
        };

        assert_ne!(
            knot::KnotEffectFetcher::cache_version(&small),
            knot::KnotEffectFetcher::cache_version(&large)
        );
    }

    #[test]
    #[should_panic(expected = "Titan is an upload protocol")]
    fn effect_authority_rejects_titan_read_grants() {
        effect_authority(
            OsStr::new("ask"),
            OsStr::new("never"),
            OsStr::new("titan"),
            OsStr::new(""),
            OsStr::new("1"),
            OsStr::new("1"),
            OsStr::new("1024"),
            None,
        );
    }
}
