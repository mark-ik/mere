use std::path::PathBuf;
use std::time::Duration;

fn main() {
    let mut endpoint = match std::env::args_os().nth(1) {
        Some(root) => knot::KnotEndpoint::open(PathBuf::from(root))
            .expect("Knot could not open the requested directory"),
        None => knot::KnotEndpoint::fixture(),
    };
    graphshell_stdio::serve_resumable_notifying(
        &mut endpoint,
        std::io::stdin(),
        std::io::stdout().lock(),
        Duration::from_millis(100),
    )
    .expect("Knot Graphshell endpoint failed");
}
