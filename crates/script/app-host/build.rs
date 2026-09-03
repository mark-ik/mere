// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Builds the `app-core` guest component the integration tests load.
//!
//! Same shape as document-host's build script: the guest is a standalone
//! workspace compiled to `wasm32-wasip2`, and the tests resolve the artifact
//! by a path relative to the package root (overridable via
//! `APP_HOST_GUEST_WASM`). A missing `wasm32-wasip2` target warns and moves on
//! rather than failing the build — the library itself needs no wasm, and the
//! tests fail loudly naming the command to run, so a missing guest can never
//! become a silently-skipped test.

use std::path::Path;
use std::process::Command;

const GUEST: (&str, &str) = ("guest", "app_core_guest.wasm");

fn main() {
    let root = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let root = Path::new(&root);
    let (dir, artifact) = GUEST;
    let guest = root.join(dir);
    if !guest.join("Cargo.toml").exists() {
        println!("cargo::warning={dir}: no Cargo.toml; skipping guest build");
        return;
    }

    println!("cargo::rerun-if-changed={}/src", guest.display());
    println!("cargo::rerun-if-changed={}/Cargo.toml", guest.display());
    println!("cargo::rerun-if-changed=../wit/world.wit");

    // The guest is a separate workspace with its own target dir and toolchain
    // pin. Strip the parent cargo's environment so the child resolves its own.
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
            return;
        }
        Err(err) => {
            println!("cargo::warning={dir}: could not run cargo ({err})");
            return;
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
