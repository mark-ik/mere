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
