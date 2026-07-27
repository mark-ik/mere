use std::path::PathBuf;
use std::time::Duration;

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
        _ => panic!(
            "usage: knot_endpoint [directory] | directory <root> | \
             directory-write <root> <max-source-bytes> | \
             persona-vault <data-root> <persona-id> <max-source-bytes>"
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
