// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::path::PathBuf;

use pandect::PersonaId;

fn main() {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .expect("usage: n4_policy_receipt <output.html>");
    let state = std::env::temp_dir().join(format!(
        "graphshell-n4-receipt-state-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&state);

    let receipt =
        graphshell::policy_projection::run_n4_policy_scenario(&state, PersonaId::default_persona())
            .expect("N4 policy scenario");
    let html = graphshell::policy_projection::render_n4_policy_receipt(&receipt);
    std::fs::write(&output, html).expect("receipt writes");
    std::fs::remove_dir_all(state).expect("receipt state cleans up");
    println!("wrote {}", output.display());
}
