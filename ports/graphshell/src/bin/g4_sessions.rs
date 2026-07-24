#[cfg(not(target_arch = "wasm32"))]
fn main() {
    use std::path::PathBuf;

    let mut args = std::env::args_os().skip(1);
    let output = PathBuf::from(
        args.next()
            .expect("usage: g4_sessions <output.html> <endpoint> [endpoint...]"),
    );
    let endpoints: Vec<PathBuf> = args.map(PathBuf::from).collect();
    assert!(
        !endpoints.is_empty(),
        "usage: g4_sessions <output.html> <endpoint> [endpoint...]"
    );
    let sessions = graphshell::sessions::mount_endpoint_processes(&endpoints)
        .expect("Graphshell could not mount endpoint sessions");
    assert!(!sessions.is_empty(), "endpoints advertised no projections");
    let html = graphshell::sessions::render_session_switch_receipt(&sessions);
    std::fs::write(&output, html).expect("could not write G4 session receipt");
    println!(
        "mounted {} sessions from {} endpoints into {}",
        sessions.len(),
        endpoints.len(),
        output.display()
    );
}

#[cfg(target_arch = "wasm32")]
fn main() {}
