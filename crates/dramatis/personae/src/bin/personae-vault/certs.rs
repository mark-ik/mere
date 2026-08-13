//! The certificate half of `personae-vault`: the authority, minting, and
//! the one-command enrollment that replaces per-pair `authorized_keys`
//! setup.
//!
//! Split from `main.rs` to stay under the 600-line ceiling.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use personae::carry::{
    ACTION_SSH_AGENT_FORWARD, ACTION_SSH_LOGIN, ACTION_SSH_PORT_FORWARD, ACTION_SSH_PTY,
    device_capability_scope,
};
use personae::delegation::{DelegationCertificate, DelegationParent, SignedDelegationCertificate};
use personae::enroll::{self, device_id_for_host};
use personae::ssh_ca::{SshCertAuthority, UserCertRequest};
use personae::vault::Profile;
use personae::{IdentityProvider, InMemoryProvider, ssh_slot};
use ssh_key::public::PublicKey;

use crate::resolve_key;

// ─── the certificate authority ────────────────────────────────────────────

/// The authority for this profile, derived from its master key.
pub(crate) fn authority(profile: &Profile) -> Result<SshCertAuthority, String> {
    let provider = InMemoryProvider::from_seed(profile.master.to_seed());
    SshCertAuthority::derive(&provider).map_err(|err| format!("derive the CA: {err}"))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or_default()
}

/// A self-grant: the persona authorizing itself to log in to one machine.
///
/// Scoped to the target device so revoking it closes that machine and no
/// other, and re-issued per mint rather than stored, which is the same
/// posture the device-grant migration chose — a grant that is re-issued on
/// unlock never needs a legacy decoder.
fn ssh_grant(
    profile: &Profile,
    host: &str,
    actions: &[&str],
    hours: u64,
) -> Result<SignedDelegationCertificate, String> {
    let provider = InMemoryProvider::from_seed(profile.master.to_seed());
    let master = provider.master_public_key().to_bytes();
    let now = now_ms();
    SignedDelegationCertificate::issue(
        &provider,
        DelegationCertificate::new(
            DelegationParent::Root(master),
            master,
            master,
            device_capability_scope(device_id_for_host(host), actions.iter().copied()),
            now,
            now,
            Some(now + hours * 3_600_000),
            0,
            grant_nonce(host, now),
        ),
    )
    .map_err(|err| format!("issue the ssh grant: {err}"))
}

fn grant_nonce(host: &str, now_ms: u64) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(host.as_bytes());
    hasher.update(&now_ms.to_le_bytes());
    *hasher.finalize().as_bytes()
}

pub(crate) fn cmd_ca(profile: &Profile, rest: &[String]) -> Result<(), String> {
    let ca = authority(profile)?;
    let patterns = flag(rest, "--patterns").unwrap_or_else(|| "*".to_string());
    println!("fingerprint: {}", ca.fingerprint());
    println!("\n# TrustedUserCAKeys / cert-authority key");
    println!("{}", ca.trusted_user_ca_line().map_err(str_err)?.trim_end());
    println!("\n# ~/.ssh/known_hosts line for hosts serving a host certificate");
    println!("{}", enroll::known_hosts_line(&ca, &patterns).map_err(str_err)?);
    Ok(())
}

pub(crate) fn cmd_mint(profile: &Profile, rest: &[String]) -> Result<(), String> {
    let slot_key = rest
        .first()
        .ok_or("mint needs a slot, e.g. `mint ssh:SHA256:d3tQ --host q-pc.local`")?;
    let host = flag(rest, "--host").ok_or("mint needs --host <hostname>")?;
    let principal = flag(rest, "--principal").unwrap_or_else(default_principal);
    let hours = flag(rest, "--hours")
        .map(|value| value.parse::<u64>().map_err(|_| "--hours wants a number"))
        .transpose()?
        .unwrap_or(12);

    let key = resolve_key(profile, slot_key)?;
    let slot = profile.slots.get(&key).ok_or("slot vanished")?;
    let private = ssh_slot::private_key_from_slot(slot).map_err(|err| format!("{err}"))?;

    let actions = face_actions(&principal);
    let grant = ssh_grant(profile, &host, &actions, hours)?;
    let ca = authority(profile)?;
    let cert = ca
        .mint_user_cert(
            &UserCertRequest {
                grant: &grant,
                subject: &PublicKey::from(&private),
                principals: vec![principal.clone()],
                force_command: flag(rest, "--force-command"),
                source_address: flag(rest, "--source-address"),
            },
            now_ms(),
        )
        .map_err(str_err)?;

    let encoded = cert.to_openssh().map_err(|err| format!("{err}"))?;
    match flag(rest, "--out") {
        Some(path) => {
            std::fs::write(&path, format!("{encoded}\n"))
                .map_err(|err| format!("write {path}: {err}"))?;
            println!("wrote {path}");
        }
        None => println!("{encoded}"),
    }
    eprintln!(
        "minted for {principal}@{host}, valid {hours}h, grant {}",
        &personae::ssh_ca::key_id_for(&grant.certificate.id())[..16]
    );
    Ok(())
}

pub(crate) fn cmd_enroll_host(profile: &Profile, rest: &[String]) -> Result<(), String> {
    let target = rest
        .first()
        .ok_or("enroll-host needs a target, e.g. `enroll-host markik@q-pc.local`")?;
    let (user, host) = enroll::split_target(target);
    let principal = flag(rest, "--principal")
        .or_else(|| user.map(str::to_string))
        .unwrap_or_else(default_principal);
    let ca = authority(profile)?;

    if rest.iter().any(|arg| arg == "--system") {
        println!(
            "{}",
            enroll::system_sshd_snippet(
                &ca,
                "/etc/ssh/personae_ca.pub",
                Some("/etc/ssh/ssh_host_ed25519_key-cert.pub"),
            )
            .map_err(str_err)?
        );
        return Ok(());
    }

    let line = enroll::user_trust_line(&ca, &[principal.clone()]).map_err(str_err)?;
    let script = enroll::user_install_script(&line);
    let output = Command::new("ssh")
        .args(["-o", "BatchMode=yes", target, "sh -s"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child
                .stdin
                .take()
                .expect("piped stdin")
                .write_all(script.as_bytes())?;
            child.wait_with_output()
        })
        .map_err(|err| format!("run ssh {target}: {err}"))?;

    if !String::from_utf8_lossy(&output.stdout).contains("enrolled") {
        return Err(format!(
            "enrollment did not confirm on {host}:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    println!("enrolled {host}: certificates for principal {principal:?} are now accepted");
    println!(
        "  no authorized_keys entry per client machine, and re-running this replaces \
         the line rather than stacking one"
    );
    println!("  host-key prompts still apply; `enroll-host {target} --system` prints the root half");
    Ok(())
}

/// Which capabilities a face carries.
///
/// The burner is the shape the plan asked for: it logs in and nothing else,
/// so its certificates carry no extensions at all.
fn face_actions(principal: &str) -> Vec<&'static str> {
    if principal.contains("burner") {
        vec![ACTION_SSH_LOGIN]
    } else {
        vec![
            ACTION_SSH_LOGIN,
            ACTION_SSH_PTY,
            ACTION_SSH_AGENT_FORWARD,
            ACTION_SSH_PORT_FORWARD,
        ]
    }
}

fn default_principal() -> String {
    std::env::var("USER").unwrap_or_else(|_| "root".into())
}

/// Read `--name value` out of the trailing arguments.
fn flag(rest: &[String], name: &str) -> Option<String> {
    rest.iter()
        .position(|arg| arg == name)
        .and_then(|at| rest.get(at + 1))
        .cloned()
}

fn str_err<E: std::fmt::Display>(err: E) -> String {
    err.to_string()
}
