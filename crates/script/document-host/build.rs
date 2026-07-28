//! Builds the two WASM guest components the integration tests load.
//!
//! The guests (`guest/`, `guest-bomb/`) are standalone workspaces compiled to
//! `wasm32-wasip2`. Before this script existed they had to be built by hand, so a
//! fresh checkout or a CI runner failed four tests until someone read the panic
//! message and ran two commands. Nothing about that was machine-specific, it was
//! just manual.
//!
//! The tests resolve the artifacts by a path relative to the package root (cargo
//! runs tests with the CWD set there), overridable via `DOC_HOST_GUEST_WASM` /
//! `DOC_HOST_BOMB_WASM`. This script only has to make sure the artifacts exist at
//! those paths, so it needs no cooperation from the tests.
//!
//! If the `wasm32-wasip2` target is missing, this warns and moves on rather than
//! failing the build: the library itself does not need wasm, and the tests already
//! fail loudly, naming the exact command to run. A missing guest must never become
//! a silently-skipped test.

use std::path::Path;
use std::process::Command;

const GUESTS: [(&str, &str); 2] = [
    ("guest", "document_core_guest.wasm"),
    ("guest-bomb", "document_core_guest_bomb.wasm"),
];

fn main() {
    let root = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let root = Path::new(&root);

    for (dir, artifact) in GUESTS {
        let guest = root.join(dir);
        if !guest.join("Cargo.toml").exists() {
            println!("cargo::warning={dir}: no Cargo.toml; skipping guest build");
            continue;
        }

        // Rebuild when the guest's own sources move, not on every touch of the host.
        println!("cargo::rerun-if-changed={}/src", guest.display());
        println!("cargo::rerun-if-changed={}/Cargo.toml", guest.display());

        // The guest is a separate workspace with its own target dir and toolchain
        // pin. Strip the parent cargo's environment so the child resolves its own:
        // inherited RUSTFLAGS / CARGO_TARGET_DIR / toolchain vars would otherwise
        // leak the host's native build into a wasm one.
        let mut cmd = Command::new("cargo");
        cmd.current_dir(&guest)
            .args(["build", "--target", "wasm32-wasip2", "--release"]);
        for (key, _) in std::env::vars() {
            if key.starts_with("CARGO")
                || key.starts_with("RUSTC")
                || key.starts_with("RUSTUP")
                || key == "RUSTFLAGS"
                || key == "RUSTDOCFLAGS"
                || key == "TARGET"
            {
                cmd.env_remove(key);
            }
        }

        match cmd.status() {
            Ok(status) if status.success() => {}
            Ok(status) => {
                println!(
                    "cargo::warning={dir}: guest build failed ({status}). The tests that load \
                     it will fail and name the command to run by hand."
                );
                continue;
            }
            Err(err) => {
                println!("cargo::warning={dir}: could not run cargo ({err})");
                continue;
            }
        }

        let built = guest.join("target/wasm32-wasip2/release").join(artifact);
        if !built.exists() {
            println!(
                "cargo::warning={dir}: build reported success but {} is missing",
                built.display()
            );
        }
    }
}
