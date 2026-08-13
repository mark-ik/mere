//! personae-vault — inspect and manage the personae vault from a terminal.
//!
//! V3 of the identity-vault-ssh-agent plan. Opens the same vault the
//! agent serves (same directory, same unlock ladder via
//! [`personae::bootstrap`]), and shows what is in it — including, per plan
//! §3.4, what losing this device would mean for each slot.
//!
//! **Secret bytes are never printed.** Slots report their size, kind,
//! lineage and unlock tier; the payload stays in the vault. Public
//! material (SSH public keys, fingerprints) prints on request.
//!
//! ```text
//! personae-vault [--dir <d>] [--profile <p>] <command>
//!
//!   profiles                    list profiles in this vault
//!   new-profile <id>            create a profile
//!   list                        list the profile's slots
//!   show <mod[:instance]>       inspect one slot
//!   add-ssh <file> [--per-use]  import an OpenSSH private key
//!   pub <mod[:instance]>        print an ssh slot's public key
//!   remove <mod[:instance]>     remove a slot
//!   ca                          print the profile's SSH certificate authority
//!   mint <slot> --host <h>      mint a login certificate for a machine
//!   enroll-host [user@]host     teach a machine to accept this authority
//!   face [work|research|burner] show or set this face's SSH reach
//! ```
//!
//! Slot keys accept a unique prefix, so
//! `personae-vault show ssh:SHA256:d3tQ` resolves when only one slot
//! matches.

use std::path::PathBuf;

use personae::bootstrap::{self, Unlock};
use personae::ssh_slot;
use personae::vault::{IdentitySlot, IdentityStorage, Profile, ProfileId, ProtocolKey, UnlockTier};
use ssh_key::HashAlg;
use ssh_key::private::PrivateKey;
use ssh_key::public::PublicKey;

mod certs;
use certs::{cmd_ca, cmd_enroll_host, cmd_face, cmd_mint};

const USAGE: &str = "\
usage: personae-vault [--dir <vault-dir>] [--profile <name>] <command>

commands:
  profiles                    list profiles in this vault
  new-profile <id>            create a profile
  list                        list the profile's slots
  show <mod[:instance]>       inspect one slot (never prints secrets)
  add-ssh <file> [--per-use]  import an OpenSSH private key
  pub <mod[:instance]>        print an ssh slot's public key
  remove <mod[:instance]>     remove a slot
  ca [--patterns <pat>]       print this profile's SSH certificate authority
  mint <slot> --host <h>      mint a login certificate for a machine
        [--principal <p>] [--hours <n>] [--out <f>]
        [--force-command <c>] [--source-address <cidr>]
  enroll-host [user@]host     teach a machine to accept this authority
        [--principal <p>] [--system]
  face [work|research|burner] show or set this face's SSH reach
        [--principal <p>] [--command <c>]

slot keys accept a unique prefix, e.g. `show ssh:SHA256:d3tQ`.
set PERSONAE_PASSPHRASE to use the portable passphrase vault instead of
the OS-protected one.";

fn main() {
    if let Err(message) = run() {
        eprintln!("personae-vault: {message}");
        std::process::exit(1);
    }
}

struct Cli {
    dir: PathBuf,
    profile: ProfileId,
    command: String,
    rest: Vec<String>,
}

fn parse_cli() -> Result<Cli, String> {
    let mut dir = bootstrap::default_vault_dir();
    let mut profile = "default".to_string();
    let mut command = None;
    let mut rest = Vec::new();

    let mut argv = std::env::args().skip(1);
    while let Some(arg) = argv.next() {
        match arg.as_str() {
            "--dir" => dir = PathBuf::from(argv.next().ok_or("--dir needs a value")?),
            "--profile" => profile = argv.next().ok_or("--profile needs a value")?,
            "--help" | "-h" => return Err(USAGE.to_string()),
            _ if command.is_none() => command = Some(arg),
            _ => rest.push(arg),
        }
    }
    Ok(Cli {
        dir,
        profile: ProfileId(profile),
        command: command.ok_or_else(|| USAGE.to_string())?,
        rest,
    })
}

fn run() -> Result<(), String> {
    let cli = parse_cli()?;
    let opened = bootstrap::open_storage(&cli.dir, Unlock::from_env())
        .map_err(|err| format!("open vault: {err}"))?;

    match cli.command.as_str() {
        "profiles" => cmd_profiles(&*opened.storage, &opened.description),
        "new-profile" => cmd_new_profile(&*opened.storage, &cli.rest),
        "list" => cmd_list(&*opened.storage, &cli.profile, &opened.description),
        "show" => cmd_show(&*opened.storage, &cli.profile, &cli.rest),
        "add-ssh" => cmd_add_ssh(&*opened.storage, &cli.profile, &cli.rest),
        "pub" => cmd_pub(&*opened.storage, &cli.profile, &cli.rest),
        "remove" => cmd_remove(&*opened.storage, &cli.profile, &cli.rest),
        "ca" => cmd_ca(&load(&*opened.storage, &cli.profile)?, &cli.rest),
        "mint" => cmd_mint(&load(&*opened.storage, &cli.profile)?, &cli.rest),
        "enroll-host" => cmd_enroll_host(&load(&*opened.storage, &cli.profile)?, &cli.rest),
        "face" => cmd_face(&*opened.storage, &cli.profile, &cli.rest),
        other => Err(format!("unknown command {other:?}\n\n{USAGE}")),
    }
}

fn load(storage: &dyn IdentityStorage, id: &ProfileId) -> Result<Profile, String> {
    storage.load_profile(id).map_err(|err| {
        let known = storage.list_profiles().unwrap_or_default();
        if known.iter().any(|summary| &summary.id == id) {
            return format!("load profile {:?}: {err}", id.0);
        }
        let names: Vec<&str> = known.iter().map(|summary| summary.id.0.as_str()).collect();
        let existing = if names.is_empty() {
            "this vault has no profiles yet".to_string()
        } else {
            format!("this vault has: {}", names.join(", "))
        };
        format!(
            "no profile {:?} ({existing})\ncreate it with: personae-vault new-profile {}",
            id.0, id.0
        )
    })
}

// ─── commands ─────────────────────────────────────────────────────────────

fn cmd_profiles(storage: &dyn IdentityStorage, description: &str) -> Result<(), String> {
    println!("storage: {description}");
    let mut summaries = storage
        .list_profiles()
        .map_err(|err| format!("list profiles: {err}"))?;
    if summaries.is_empty() {
        println!("\nno profiles yet (`personae-vault new-profile default` creates one)");
        return Ok(());
    }
    summaries.sort_by(|a, b| a.id.cmp(&b.id));
    println!();
    for summary in summaries {
        let plural = if summary.slot_count == 1 { "" } else { "s" };
        println!(
            "{}  \"{}\", {} slot{plural}",
            summary.id.0, summary.display_name, summary.slot_count
        );
    }
    Ok(())
}

fn cmd_new_profile(storage: &dyn IdentityStorage, rest: &[String]) -> Result<(), String> {
    let id = ProfileId(rest.first().ok_or("new-profile needs an id")?.clone());
    let (profile, created) = bootstrap::load_or_create_profile(storage, &id)
        .map_err(|err| format!("create profile: {err}"))?;
    if created {
        println!("created profile {:?}", profile.id.0);
    } else {
        println!("profile {:?} already exists", profile.id.0);
    }
    Ok(())
}

fn cmd_list(
    storage: &dyn IdentityStorage,
    id: &ProfileId,
    description: &str,
) -> Result<(), String> {
    let profile = load(storage, id)?;
    let plural = if profile.slots.len() == 1 { "" } else { "s" };
    println!(
        "profile {:?} ({}), {} slot{plural}",
        profile.id.0,
        profile.display_name,
        profile.slots.len()
    );
    println!("storage: {description}");
    if profile.slots.is_empty() {
        println!("\nno slots yet (`personae-vault add-ssh <file>` imports an SSH key)");
        return Ok(());
    }

    let mut keys: Vec<&ProtocolKey> = profile.slots.keys().collect();
    keys.sort();
    println!();
    for key in keys {
        let slot = &profile.slots[key];
        println!("{}", format_key(key));
        println!(
            "  {}, {}, {} unlock",
            slot.kind(),
            category(slot),
            tier_label(slot.unlock_tier())
        );
        if let Some(line) = ssh_summary(slot) {
            println!("  {line}");
        }
    }
    Ok(())
}

fn cmd_show(storage: &dyn IdentityStorage, id: &ProfileId, rest: &[String]) -> Result<(), String> {
    let profile = load(storage, id)?;
    let key = resolve_key(&profile, rest.first().ok_or("show needs a slot key")?)?;
    let slot = &profile.slots[&key];

    println!("{}", format_key(&key));
    println!("  kind:      {}", slot.kind());
    println!("  category:  {}", category(slot));
    println!("  unlock:    {}", tier_label(slot.unlock_tier()));
    println!("  payload:   {} bytes (not shown)", payload_len(slot));
    if let IdentitySlot::Bootstrap { state_dir, .. } = slot {
        println!("  state dir: {}", state_dir.display());
    }
    if let Some(line) = ssh_summary(slot) {
        println!("  ssh:       {line}");
    }
    println!("  lineage:   {:?}", slot.lineage());
    println!(
        "  losing this device: {}",
        slot.lineage().device_loss_note()
    );
    Ok(())
}

fn cmd_add_ssh(
    storage: &dyn IdentityStorage,
    id: &ProfileId,
    rest: &[String],
) -> Result<(), String> {
    let mut path = None;
    let mut tier = UnlockTier::Session;
    for arg in rest {
        match arg.as_str() {
            "--per-use" => tier = UnlockTier::PerUse,
            _ if path.is_none() => path = Some(PathBuf::from(arg)),
            other => return Err(format!("unexpected argument {other:?}")),
        }
    }
    let path = path.ok_or("add-ssh needs a private-key file")?;
    let bytes = std::fs::read(&path).map_err(|err| format!("read {}: {err}", path.display()))?;
    let private = PrivateKey::from_openssh(&bytes)
        .map_err(|err| format!("parse {} as an OpenSSH private key: {err}", path.display()))?;

    let key = ssh_slot::protocol_key_for(&private);
    let slot = ssh_slot::slot_for(&private, tier).map_err(|err| err.to_string())?;
    let profile = load(storage, id)?;
    let already = profile.slots.contains_key(&key);

    let mut vault = personae::IdentityVault::with_profile(storage, profile);
    vault
        .add_slot(key.clone(), slot)
        .map_err(|err| format!("store slot: {err}"))?;

    println!(
        "{} {}",
        if already { "replaced" } else { "imported" },
        format_key(&key)
    );
    println!("  {}", describe_ssh(&private));
    println!("  unlock: {}", tier_label(tier));
    if tier == UnlockTier::PerUse {
        println!(
            "  note: the agent refuses to sign with per-use slots until a confirmation UI exists"
        );
    }
    println!(
        "\nthe vault now holds this key. Deleting the original file leaves the vault as its \
         only holder: {}",
        path.display()
    );
    Ok(())
}

fn cmd_pub(storage: &dyn IdentityStorage, id: &ProfileId, rest: &[String]) -> Result<(), String> {
    let profile = load(storage, id)?;
    let key = resolve_key(&profile, rest.first().ok_or("pub needs a slot key")?)?;
    let private = ssh_slot::private_key_from_slot(&profile.slots[&key])
        .map_err(|err| format!("decode ssh slot: {err}"))?;
    let public = PublicKey::from(&private);
    println!(
        "{}",
        public
            .to_openssh()
            .map_err(|err| format!("encode public key: {err}"))?
    );
    Ok(())
}

fn cmd_remove(
    storage: &dyn IdentityStorage,
    id: &ProfileId,
    rest: &[String],
) -> Result<(), String> {
    let profile = load(storage, id)?;
    let key = resolve_key(&profile, rest.first().ok_or("remove needs a slot key")?)?;
    let slot = &profile.slots[&key];
    let note = slot.lineage().device_loss_note().to_string();
    let is_ssh = slot.kind() == ssh_slot::SSH_MOD_ID;

    let mut vault = personae::IdentityVault::with_profile(storage, profile);
    let removed = vault
        .remove_slot(&key)
        .map_err(|err| format!("remove slot: {err}"))?;
    if !removed {
        return Err(format!("slot {} was not present", format_key(&key)));
    }
    println!("removed {}", format_key(&key));
    println!("  {note}");
    if is_ssh {
        println!("  the vault no longer holds this key; re-import from a file to restore it");
    }
    Ok(())
}

// ─── formatting helpers ───────────────────────────────────────────────────

pub(crate) fn format_key(key: &ProtocolKey) -> String {
    match &key.instance {
        Some(instance) => format!("{}:{instance}", key.mod_id),
        None => key.mod_id.clone(),
    }
}

fn category(slot: &IdentitySlot) -> &'static str {
    match slot {
        IdentitySlot::Direct { .. } => "direct",
        IdentitySlot::Bootstrap { .. } => "bootstrap",
    }
}

fn payload_len(slot: &IdentitySlot) -> usize {
    match slot {
        IdentitySlot::Direct { payload, .. } => payload.len(),
        IdentitySlot::Bootstrap { bootstrap, .. } => bootstrap.len(),
    }
}

fn tier_label(tier: UnlockTier) -> String {
    match tier {
        UnlockTier::Session => "session".to_string(),
        UnlockTier::ShortTtl { idle_seconds } => {
            format!("short-ttl ({idle_seconds}s idle; not enforced yet)")
        }
        UnlockTier::PerUse => "per-use (agent refuses to sign; no confirmation UI yet)".to_string(),
    }
}

fn describe_ssh(private: &PrivateKey) -> String {
    let public = PublicKey::from(private);
    let comment = private.comment();
    let comment = if comment.is_empty() {
        String::new()
    } else {
        format!(", comment {comment:?}")
    };
    format!(
        "{}, {}{comment}",
        private.algorithm(),
        public.fingerprint(HashAlg::Sha256)
    )
}

fn ssh_summary(slot: &IdentitySlot) -> Option<String> {
    if slot.kind() != ssh_slot::SSH_MOD_ID {
        return None;
    }
    match ssh_slot::private_key_from_slot(slot) {
        Ok(private) => Some(describe_ssh(&private)),
        Err(err) => Some(format!("unreadable ssh payload: {err}")),
    }
}

/// Resolve a user-typed slot key against the profile, accepting a unique
/// prefix so full fingerprints need not be typed.
fn resolve_key(profile: &Profile, typed: &str) -> Result<ProtocolKey, String> {
    let exact = match typed.split_once(':') {
        Some((mod_id, instance)) => ProtocolKey::new(mod_id, Some(instance.to_string())),
        None => ProtocolKey::new(typed, None),
    };
    if profile.slots.contains_key(&exact) {
        return Ok(exact);
    }

    let mut matches: Vec<&ProtocolKey> = profile
        .slots
        .keys()
        .filter(|key| format_key(key).starts_with(typed))
        .collect();
    match matches.len() {
        1 => Ok(matches.remove(0).clone()),
        0 => Err(format!(
            "no slot matches {typed:?} (`personae-vault list` shows what is there)"
        )),
        _ => {
            matches.sort();
            let candidates: Vec<String> = matches.iter().map(|key| format_key(key)).collect();
            Err(format!(
                "{typed:?} is ambiguous; it matches:\n  {}",
                candidates.join("\n  ")
            ))
        }
    }
}

// The unit-testable core here is `resolve_key`; everything else is I/O
// formatting exercised by the end-to-end CLI run recorded in the plan.
#[cfg(test)]
mod tests {
    use super::*;
    use personae::Ed25519Keypair;
    use personae::vault::{CredentialLineage, SecretBytes};

    fn profile_with(keys: &[ProtocolKey]) -> Profile {
        let mut profile = Profile::new(
            ProfileId("t".into()),
            "t",
            Ed25519Keypair::from_seed([1; 32]),
        );
        for key in keys {
            profile.slots.insert(
                key.clone(),
                IdentitySlot::Direct {
                    kind: key.mod_id.clone(),
                    payload: SecretBytes::new(vec![0; 4]),
                    lineage: CredentialLineage::LocallyDerived,
                    unlock_tier: UnlockTier::Session,
                },
            );
        }
        profile
    }

    #[test]
    fn exact_key_resolves_including_colons_in_the_instance() {
        let key = ProtocolKey::new("ssh", Some("SHA256:abcdef".into()));
        let profile = profile_with(std::slice::from_ref(&key));
        assert_eq!(resolve_key(&profile, "ssh:SHA256:abcdef").unwrap(), key);
    }

    #[test]
    fn unique_prefix_resolves() {
        let key = ProtocolKey::new("ssh", Some("SHA256:abcdef".into()));
        let profile = profile_with(std::slice::from_ref(&key));
        assert_eq!(resolve_key(&profile, "ssh:SHA256:abc").unwrap(), key);
        assert_eq!(resolve_key(&profile, "ssh").unwrap(), key);
    }

    #[test]
    fn ambiguous_prefix_lists_candidates() {
        let profile = profile_with(&[
            ProtocolKey::new("ssh", Some("SHA256:aaa".into())),
            ProtocolKey::new("ssh", Some("SHA256:aab".into())),
        ]);
        let err = resolve_key(&profile, "ssh:SHA256:aa").unwrap_err();
        assert!(err.contains("ambiguous"), "got: {err}");
        assert!(
            err.contains("SHA256:aaa") && err.contains("SHA256:aab"),
            "got: {err}"
        );
    }

    #[test]
    fn unknown_key_is_an_error() {
        let profile = profile_with(&[ProtocolKey::new("ssh", Some("SHA256:aaa".into()))]);
        assert!(resolve_key(&profile, "nostr").is_err());
    }

    #[test]
    fn instanceless_key_resolves_exactly() {
        let key = ProtocolKey::new("nostr", None);
        let profile = profile_with(std::slice::from_ref(&key));
        assert_eq!(resolve_key(&profile, "nostr").unwrap(), key);
    }
}
