use std::path::PathBuf;

fn main() {
    let mut endpoint = match std::env::args_os().nth(1) {
        Some(root) => knot::KnotEndpoint::open(PathBuf::from(root))
            .expect("Knot could not open the requested directory"),
        None => knot::KnotEndpoint::fixture(),
    };
    graphshell_stdio::serve_resumable(
        &mut endpoint,
        std::io::stdin().lock(),
        std::io::stdout().lock(),
    )
    .expect("Knot Graphshell endpoint failed");
}
