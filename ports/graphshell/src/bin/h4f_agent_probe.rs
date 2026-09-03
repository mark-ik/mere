// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Isolated H4f SSH-agent wire probe.
//!
//! This receipt helper connects to an explicitly named nonstandard endpoint,
//! lists one public identity, requests a real signature, and verifies it.

use signature::Verifier;
use ssh_agent_lib::agent::Session;
use ssh_agent_lib::client::Client;
use ssh_agent_lib::proto::SignRequest;
use ssh_key::{HashAlg, PublicKey};

const PAYLOAD: &[u8] = b"graphshell-h4f-resident-agent-restart";

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("graphshell H4f agent probe: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = std::env::args()
        .nth(1)
        .ok_or("usage: h4f_agent_probe <isolated-endpoint> [expected-fingerprint]")?;
    let expected = std::env::args().nth(2);
    let stream = connect(&endpoint).await?;
    let mut client = Client::new(stream);
    let identities = client.request_identities().await?;
    if identities.len() != 1 {
        return Err(format!("expected one identity, found {}", identities.len()).into());
    }
    let identity = &identities[0];
    let public = PublicKey::new(
        identity.credential.key_data().clone(),
        identity.comment.clone(),
    );
    let fingerprint = public.fingerprint(HashAlg::Sha256).to_string();
    if let Some(expected) = expected
        && fingerprint != expected
    {
        return Err(format!("expected {expected}, found {fingerprint}").into());
    }
    let signature = client
        .sign(SignRequest {
            credential: identity.credential.clone(),
            data: PAYLOAD.to_vec(),
            flags: 0,
        })
        .await?;
    identity.credential.key_data().verify(PAYLOAD, &signature)?;
    println!("{fingerprint}\t{}", identity.comment);
    Ok(())
}

#[cfg(windows)]
async fn connect(
    endpoint: &str,
) -> Result<tokio::net::windows::named_pipe::NamedPipeClient, std::io::Error> {
    tokio::net::windows::named_pipe::ClientOptions::new().open(endpoint)
}

#[cfg(not(windows))]
async fn connect(endpoint: &str) -> Result<tokio::net::UnixStream, std::io::Error> {
    tokio::net::UnixStream::connect(endpoint).await
}
