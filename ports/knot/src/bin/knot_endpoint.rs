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
        _ => panic!(
            "usage: knot_endpoint [directory] | directory <root> | \
             directory-write <root> <max-source-bytes>"
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
