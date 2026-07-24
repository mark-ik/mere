use std::path::PathBuf;

fn main() {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .expect("usage: g1_receipt <output.html>");
    let html = graphshell::view::render_g1_receipt().expect("G1 loopback canary resolves");
    std::fs::write(&path, html).expect("receipt writes");
    println!("wrote {}", path.display());
}
