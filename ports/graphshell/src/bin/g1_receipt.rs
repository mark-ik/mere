// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

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
