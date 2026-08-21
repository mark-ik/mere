// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

#![cfg(all(target_os = "linux", feature = "secret-service"))]

use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;

use castellan::resident::CastellanResident;
use castellan::secret_service::{
    SecretServiceAccessPolicy, SecretServiceCaller, SecretServiceLimits, SecretServiceOperation,
    serve,
};
use personae::PersonaId;
use tempfile::tempdir;

fn executable_on_path(name: &str) -> PathBuf {
    let path = std::env::var_os("PATH").unwrap_or_else(|| OsString::from(""));
    std::env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
        .and_then(|candidate| std::fs::canonicalize(candidate).ok())
        .unwrap_or_else(|| panic!("{name} must be installed for this interoperability receipt"))
}

fn secret_tool(arguments: &[&str], secret_input: Option<&str>) -> std::process::Output {
    let mut child = Command::new("secret-tool")
        .args(arguments)
        .stdin(if secret_input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch secret-tool");
    if let Some(secret) = secret_input {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(secret.as_bytes())
            .expect("write secret-tool input");
    }
    let output = child.wait_with_output().expect("wait for secret-tool");
    assert!(
        output.status.success(),
        "secret-tool {:?} failed: {}",
        arguments,
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

/// A real libsecret client exercises create, search, release, and deletion.
///
/// Run inside a disposable session bus so the receipt cannot replace the
/// desktop keyring:
/// `dbus-run-session -- cargo test -p castellan --features secret-service \
///   --test secret_service_linux -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires Linux, dbus-run-session, and secret-tool"]
async fn secret_tool_store_lookup_and_clear() {
    let dir = tempdir().unwrap();
    let resident = CastellanResident::claim(
        dir.path().join("records"),
        [0x91; 32],
        dir.path().join("freshness"),
        [0x92; 32],
    )
    .unwrap();
    let store = resident.secret_service(PersonaId::new(), SecretServiceLimits::default());

    let admitted_executable = executable_on_path("secret-tool");
    let policy: Arc<dyn SecretServiceAccessPolicy> = Arc::new(
        move |caller: &SecretServiceCaller, _operation: &SecretServiceOperation| {
            caller.executable.as_deref() == Some(admitted_executable.as_path())
        },
    );
    let _server = serve(store, policy, "Castellan test collection")
        .await
        .unwrap();

    secret_tool(
        &[
            "store",
            "--label=Castellan interoperability receipt",
            "application",
            "turnstone",
            "account",
            "mark",
        ],
        Some("swordfish"),
    );
    let lookup = secret_tool(
        &["lookup", "application", "turnstone", "account", "mark"],
        None,
    );
    assert_eq!(String::from_utf8(lookup.stdout).unwrap().trim(), "swordfish");

    secret_tool(
        &["clear", "application", "turnstone", "account", "mark"],
        None,
    );
    let lookup = Command::new("secret-tool")
        .args(["lookup", "application", "turnstone", "account", "mark"])
        .output()
        .expect("launch final secret-tool lookup");
    assert!(lookup.status.success());
    assert!(lookup.stdout.is_empty());
}
