// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The certificate half of `personae-vault`: the authority, minting, and
//! the one-command enrollment that replaces per-pair `authorized_keys`
//! setup.
//!
//! Split from `main.rs` to stay under the 600-line ceiling.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use personae::delegation::{DelegationRevocation, SignedDelegationRevocation};
use personae::enroll::{self, device_id_for_host};
use personae::ssh_ca::{SshCertAuthority, UserCertRequest};
use personae::ssh_face::{self, FacePolicy};
use personae::ssh_krl;
use personae::vault::Profile;
use personae::{IdentityProvider, InMemoryProvider, ssh_slot};
use ssh_key::public::PublicKey;

use crate::format_key;

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

pub(crate) fn cmd_ca(profile: &Profile, rest: &[String]) -> Result<(), String> {
    let ca = authority(profile)?;
    let patterns = flag(rest, "--patterns").unwrap_or_else(|| "*".to_string());
    println!("fingerprint: {}", ca.fingerprint());
    println!("\n# TrustedUserCAKeys / cert-authority key");
    println!("{}", ca.trusted_user_ca_line().map_err(str_err)?.trim_end());
    println!("\n# ~/.ssh/known_hosts line for hosts serving a host certificate");
    println!(
        "{}",
        enroll::known_hosts_line(&ca, &patterns).map_err(str_err)?
    );
    Ok(())
}

pub(crate) fn cmd_mint(profile: &Profile, rest: &[String]) -> Result<(), String> {
    let slot_key = rest
        .first()
        .ok_or("mint needs a slot, e.g. `mint ssh:SHA256:d3tQ --host q-pc.local`")?;
    // The device is this machine unless told otherwise: a grant is held by
    // the machine carrying the credential, which is what makes revoking a
    // lost machine meaningful. See ssh_ca::self_grant.
    let device_name = flag(rest, "--device").unwrap_or_else(enroll::local_host_name);
    let principal = flag(rest, "--principal").unwrap_or_else(default_principal);
    let hours = flag(rest, "--hours")
        .map(|value| value.parse::<u64>().map_err(|_| "--hours wants a number"))
        .transpose()?
        .unwrap_or(12);

    let private = resolve_ssh_key(profile, slot_key)?;

    let policy = ssh_face::effective_policy(profile).map_err(str_err)?;
    if !policy.principals.contains(&principal) {
        return Err(format!(
            "face {:?} may not name principal {principal:?} (it carries: {})",
            profile.id.0,
            policy.principals.join(", ")
        ));
    }
    let ledger = ssh_krl::load_ledger(profile).map_err(str_err)?;
    let provider = InMemoryProvider::from_seed(profile.master.to_seed());
    let grant = personae::ssh_ca::self_grant(
        &provider,
        device_id_for_host(&device_name),
        &policy.action_refs(),
        hours * 3_600_000,
        now_ms(),
    )
    .map_err(|err| format!("issue the ssh grant: {err}"))?;
    let ca = authority(profile)?;
    let cert = ca
        .mint_user_cert(
            &UserCertRequest {
                grant: &grant,
                subject: &PublicKey::from(&private),
                principals: vec![principal.clone()],
                // The face's own limits outrank the command line: a flag may
                // narrow a face further, never widen it.
                force_command: flag(rest, "--force-command").or(policy.force_command.clone()),
                source_address: flag(rest, "--source-address").or(policy.source_address.clone()),
                ledger: &ledger,
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
        "minted for {principal}, held by {device_name}, valid {hours}h, grant {}",
        &personae::ssh_ca::key_id_for(&grant.certificate.id())[..16]
    );
    Ok(())
}

pub(crate) fn cmd_enroll_host(profile: &Profile, rest: &[String]) -> Result<(), String> {
    let target = rest
        .first()
        .ok_or("enroll-host needs a target, e.g. `enroll-host markik@q-pc.local`")?;
    let (user, host) = enroll::split_target(target);
    let policy = ssh_face::effective_policy(profile).map_err(str_err)?;
    let principal = flag(rest, "--principal")
        .or_else(|| user.map(str::to_string))
        .unwrap_or_else(|| {
            policy
                .principals
                .first()
                .cloned()
                .unwrap_or_else(default_principal)
        });
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
    println!(
        "  host-key prompts still apply; `enroll-host {target} --system` prints the root half"
    );
    Ok(())
}

/// Resolve a slot name among this profile's SSH *keys* only.
///
/// The generic resolver matches any mod_id by prefix, and `ssh` is a prefix
/// of `ssh-face`, so `mint ssh` became ambiguous the moment face policies
/// existed. Minting needs a key, and a policy is not one.
fn resolve_ssh_key(profile: &Profile, typed: &str) -> Result<ssh_key::PrivateKey, String> {
    let mut matches: Vec<_> = ssh_slot::ssh_slots(profile)
        .into_iter()
        .filter(|slot| format_key(&slot.key).starts_with(typed))
        .collect();
    match matches.len() {
        1 => Ok(matches.remove(0).private),
        0 => Err(format!(
            "no ssh key matches {typed:?} (`personae-vault list` shows what is there)"
        )),
        _ => {
            let mut names: Vec<String> = matches.iter().map(|slot| format_key(&slot.key)).collect();
            names.sort();
            Err(format!(
                "{typed:?} is ambiguous; it matches:\n  {}",
                names.join("\n  ")
            ))
        }
    }
}

/// Revoke a device: this vault stops certifying it, immediately.
pub(crate) fn cmd_revoke(
    storage: &dyn personae::vault::IdentityStorage,
    id: &personae::vault::ProfileId,
    rest: &[String],
) -> Result<(), String> {
    let name = rest
        .first()
        .filter(|arg| !arg.starts_with("--"))
        .cloned()
        .ok_or("revoke needs a device, e.g. `revoke thinkpad.local`")?;
    let mut profile = storage
        .load_profile(id)
        .map_err(|err| format!("load profile: {err}"))?;
    let device = device_id_for_host(&name);
    let provider = InMemoryProvider::from_seed(profile.master.to_seed());
    let policy = ssh_face::effective_policy(&profile).map_err(str_err)?;
    let now = now_ms();

    // Revocation names a grant, and the grant it names is one issued for
    // this device: what closes the device is the serial they share.
    let grant = personae::ssh_ca::self_grant(&provider, device, &policy.action_refs(), 1, now)
        .map_err(|err| format!("name the device's grant: {err}"))?;
    let revocation = SignedDelegationRevocation::issue(
        &provider,
        DelegationRevocation::new(
            grant.certificate.id(),
            provider.master_public_key().to_bytes(),
            grant.certificate.scope.clone(),
            now,
            *blake3::hash(&now.to_le_bytes()).as_bytes(),
        ),
    )
    .map_err(|err| format!("sign the revocation: {err}"))?;

    let mut ledger = ssh_krl::load_ledger(&profile).map_err(str_err)?;
    if !ledger.fold(&revocation, &name) {
        return Err("the revocation did not verify".to_string());
    }
    ssh_krl::store_ledger(&mut profile, &ledger).map_err(str_err)?;
    storage
        .save_profile(&profile)
        .map_err(|err| format!("save profile: {err}"))?;

    let serial = personae::ssh_ca::serial_for_device(device);
    println!("revoked {name} (certificate serial {serial})");
    println!("  this vault will not certify it again, so its access ends when its");
    println!("  last certificate expires (at most 12h)");
    println!("  to close it on a host now: personae-vault krl --out <file>, then");
    println!("  RevokedKeys in that host's sshd_config");
    Ok(())
}

/// Render the revocation list, and compile it when ssh-keygen is present.
pub(crate) fn cmd_krl(profile: &Profile, rest: &[String]) -> Result<(), String> {
    let ledger = ssh_krl::load_ledger(profile).map_err(str_err)?;
    if ledger.is_empty() {
        println!("nothing is revoked");
        return Ok(());
    }
    let spec = ledger.krl_spec();
    let Some(out) = flag(rest, "--out") else {
        print!("{spec}");
        println!("# compile with: ssh-keygen -k -s <ca.pub> -f <krl> <this file>");
        return Ok(());
    };

    // ssh-keygen needs the CA key on disk to bind serial and id records to
    // the authority they revoke under, so it goes to a sibling file the
    // caller can keep: it is public material, and a host deploying the KRL
    // wants it anyway.
    let ca = authority(profile)?;
    let spec_path = format!("{out}.spec");
    let ca_path = format!("{out}.ca.pub");
    std::fs::write(&spec_path, &spec).map_err(|err| format!("write {spec_path}: {err}"))?;
    std::fs::write(&ca_path, ca.trusted_user_ca_line().map_err(str_err)?)
        .map_err(|err| format!("write {ca_path}: {err}"))?;

    let status = Command::new("ssh-keygen")
        .args(["-k", "-s", &ca_path, "-f", &out, &spec_path])
        .status()
        .map_err(|err| format!("run ssh-keygen: {err}"))?;
    if !status.success() {
        return Err(format!(
            "ssh-keygen could not compile the KRL (spec left at {spec_path})"
        ));
    }
    println!(
        "wrote {out} ({} device(s) revoked)",
        ledger.devices().count()
    );
    println!("  deploy: copy to the host and name it in sshd_config's RevokedKeys");
    Ok(())
}

/// Show or set this face's SSH policy.
pub(crate) fn cmd_face(
    storage: &dyn personae::vault::IdentityStorage,
    id: &personae::vault::ProfileId,
    rest: &[String],
) -> Result<(), String> {
    let mut profile = storage
        .load_profile(id)
        .map_err(|err| format!("load profile: {err}"))?;

    if let Some(shape) = rest.first().filter(|arg| !arg.starts_with("--")) {
        let principal = flag(rest, "--principal").unwrap_or_else(|| id.0.clone());
        let policy = match shape.as_str() {
            "work" => FacePolicy::work(principal),
            "research" => FacePolicy::research(principal),
            "burner" => FacePolicy::burner(
                principal,
                flag(rest, "--command").unwrap_or_else(|| "true".into()),
            ),
            other => {
                return Err(format!(
                    "unknown face shape {other:?} (work, research, burner)"
                ));
            }
        };
        ssh_face::store_policy(&mut profile, &policy).map_err(str_err)?;
        storage
            .save_profile(&profile)
            .map_err(|err| format!("save profile: {err}"))?;
        println!("face {:?} is now a {shape} face", id.0);
    }

    let policy = ssh_face::effective_policy(&profile).map_err(str_err)?;
    let stored = ssh_face::load_policy(&profile).map_err(str_err)?.is_some();
    println!(
        "face: {:?}{}",
        id.0,
        if stored {
            ""
        } else {
            " (no stored policy; showing the default)"
        }
    );
    println!("  principals: {}", policy.principals.join(", "));
    println!("  actions:    {}", policy.action_refs().join(", "));
    if let Some(command) = &policy.force_command {
        println!("  forced:     {command}");
    }
    if let Some(addresses) = &policy.source_address {
        println!("  from:       {addresses}");
    }
    Ok(())
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
