// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Browser-launched relay for the resident Graphshell device host.
//!
//! stdout is protocol-only. This process parses the browser launch, checks the
//! exact extension allow-list, and relays native-messaging frames. It never
//! opens Personae or holds a signing identity.

use graphshell::browser_carrier::{AllowedExtensions, BrowserLauncher};
use graphshell::native::device_broker::{
    configured_device_endpoint, relay_browser_native_messages,
};

const EXTRA_EXTENSIONS_ENV: &str = "GRAPHSHELL_EXTENSION_IDS";

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("graphshell native host: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let launcher = BrowserLauncher::parse(&args)?;
    AllowedExtensions::default()
        .with_additional(&std::env::var(EXTRA_EXTENSIONS_ENV).unwrap_or_default())
        .admit(&launcher)?;
    relay_browser_native_messages(&configured_device_endpoint(), launcher).await?;
    Ok(())
}
