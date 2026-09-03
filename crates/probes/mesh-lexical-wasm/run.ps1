# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

# Build the probe for wasm32 and run it under Node.
#   rustup target add wasm32-unknown-unknown   (once)
$ErrorActionPreference = "Stop"
Push-Location $PSScriptRoot
try {
    cargo build --release --target wasm32-unknown-unknown
    node run.mjs "target/wasm32-unknown-unknown/release/mesh_lexical_wasm.wasm"
} finally {
    Pop-Location
}
