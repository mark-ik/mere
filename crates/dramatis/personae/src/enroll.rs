//! Teaching a machine to trust the authority, once.
//!
//! Enrollment is the whole cost of the certificate model: after it, any
//! machine holding an unlocked vault reaches the enrolled one with no
//! per-pair setup at all. It runs over ordinary key-or-password SSH, which
//! is the one permanent job plain SSH keeps.
//!
//! Two paths, and the difference is who has to be root:
//!
//! - **Per-user** ([`user_trust_line`]) writes one `cert-authority` line to
//!   the target's `~/.ssh/authorized_keys`. `sshd` accepts it for that
//!   account with no privilege, no daemon restart, and nothing outside the
//!   user's own home. This is the default, and for a personal fleet it is
//!   usually the whole story.
//! - **System-wide** ([`system_sshd_snippet`]) is `TrustedUserCAKeys` plus
//!   `HostCertificate` in `sshd_config`. It covers every account and is the
//!   only way to retire host-key TOFU, because a host certificate must be
//!   offered by the daemon. It needs root, so this module *prints* it and
//!   never runs it: a tool that silently edits `sshd_config` over a network
//!   is a tool nobody should run.
//!
//! Feature `ssh`.

use crate::ssh_ca::{CertMintError, SshCertAuthority};

/// Comment marking the lines this module owns, so re-enrolling replaces
/// rather than appends. Versioned: a future format change can find and
/// retire its predecessor.
pub const ENROLLMENT_MARKER: &str = "personae-ca-v1";

/// The `cert-authority` line granting one account's trust to the CA.
///
/// `principals` restricts which certificate principals this line accepts,
/// which is what keeps a burner face's certificate from opening an account
/// it was never meant to reach. An empty list omits the restriction and
/// accepts any principal the certificate names.
pub fn user_trust_line(
    ca: &SshCertAuthority,
    principals: &[String],
) -> Result<String, CertMintError> {
    let key = ca.trusted_user_ca_line()?;
    let key = key.trim_end();
    // The key's own trailing comment is replaced by the marker, so the line
    // is findable by exactly one string.
    let mut fields = key.split_whitespace();
    let algorithm = fields.next().unwrap_or_default();
    let material = fields.next().unwrap_or_default();
    let options = if principals.is_empty() {
        "cert-authority".to_string()
    } else {
        format!("cert-authority,principals=\"{}\"", principals.join(","))
    };
    Ok(format!(
        "{options} {algorithm} {material} {ENROLLMENT_MARKER}"
    ))
}

/// Idempotent shell that installs `line` into `~/.ssh/authorized_keys`.
///
/// Written to be safe to run twice: it filters any previous marker line out
/// before appending, and rewrites the file through `cat` so an existing
/// file keeps its inode and mode.
pub fn user_install_script(line: &str) -> String {
    format!(
        "set -eu\n\
         umask 077\n\
         mkdir -p \"$HOME/.ssh\"\n\
         touch \"$HOME/.ssh/authorized_keys\"\n\
         tmp=$(mktemp \"$HOME/.ssh/.personae.XXXXXX\")\n\
         grep -v '{ENROLLMENT_MARKER}' \"$HOME/.ssh/authorized_keys\" > \"$tmp\" || true\n\
         printf '%s\\n' '{line}' >> \"$tmp\"\n\
         cat \"$tmp\" > \"$HOME/.ssh/authorized_keys\"\n\
         rm -f \"$tmp\"\n\
         chmod 700 \"$HOME/.ssh\"\n\
         chmod 600 \"$HOME/.ssh/authorized_keys\"\n\
         echo enrolled\n"
    )
}

/// The root-only half, as text for a human to review and apply.
///
/// Returned rather than executed on purpose. `sshd_config` is the file that
/// decides who may log in; a remote tool that edits it unattended is one
/// bug away from locking its owner out of the machine.
pub fn system_sshd_snippet(
    ca: &SshCertAuthority,
    ca_path: &str,
    host_cert_path: Option<&str>,
) -> Result<String, CertMintError> {
    let mut out = String::new();
    out.push_str("# Run as root on the target, then: sshd -t && systemctl reload sshd\n");
    out.push_str("#   (macOS: sudo launchctl kickstart -k system/com.openssh.sshd)\n");
    out.push_str(&format!(
        "cat > {ca_path} <<'EOF'\n{}\nEOF\nchmod 644 {ca_path}\n\n",
        ca.trusted_user_ca_line()?.trim_end()
    ));
    out.push_str("# in /etc/ssh/sshd_config:\n");
    out.push_str(&format!("TrustedUserCAKeys {ca_path}\n"));
    if let Some(path) = host_cert_path {
        out.push_str(&format!("HostCertificate {path}\n"));
        out.push_str("# ...and clients need only the @cert-authority line below in known_hosts,\n");
        out.push_str("# which is what retires first-contact host-key prompts entirely.\n");
    }
    Ok(out)
}

/// The client-side `known_hosts` line, for hosts that serve a host cert.
pub fn known_hosts_line(ca: &SshCertAuthority, patterns: &str) -> Result<String, CertMintError> {
    ca.known_hosts_ca_line(patterns)
}

/// Domain separator for host-derived device ids.
const DEVICE_ID_DOMAIN: &[u8] = b"personae/ssh-device/v1";

/// The stable device id naming one target machine.
///
/// A grant addresses a device, so an SSH grant addresses the machine being
/// logged into: that is what makes access revocable one machine at a time
/// rather than all at once. Deriving the id from the hostname keeps it
/// stable without a roster round trip, and the hostname is lowercased so
/// `Q-PC.local` and `q-pc.local` are one device rather than two.
pub fn device_id_for_host(host: &str) -> crate::carry::DeviceId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(DEVICE_ID_DOMAIN);
    hasher.update(host.to_ascii_lowercase().as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest.as_bytes()[..16]);
    crate::carry::DeviceId::from_uuid(uuid::Uuid::from_bytes(bytes))
}

/// This machine's hostname, lowercased.
pub fn local_host_name() -> String {
    gethostname::gethostname()
        .to_string_lossy()
        .to_ascii_lowercase()
}

/// The device id of the machine this is running on.
///
/// The default subject of a self-grant: authority is held by a device, and
/// revoking it should retire *this* machine's reach (see
/// [`crate::ssh_ca::self_grant`]).
pub fn local_device_id() -> crate::carry::DeviceId {
    device_id_for_host(&local_host_name())
}

/// Split `[user@]host` into its parts.
pub fn split_target(target: &str) -> (Option<&str>, &str) {
    match target.split_once('@') {
        Some((user, host)) => (Some(user), host),
        None => (None, target),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryProvider;

    fn ca() -> SshCertAuthority {
        SshCertAuthority::derive(&InMemoryProvider::from_seed([1; 32])).unwrap()
    }

    #[test]
    fn the_trust_line_is_a_cert_authority_line_with_our_marker() {
        let line = user_trust_line(&ca(), &["markik".to_string()]).unwrap();
        assert!(line.starts_with("cert-authority,principals=\"markik\" ssh-ed25519 "));
        assert!(line.ends_with(ENROLLMENT_MARKER));
        // One line, or it would corrupt authorized_keys.
        assert_eq!(line.lines().count(), 1);
    }

    #[test]
    fn no_principals_means_no_principals_restriction() {
        let line = user_trust_line(&ca(), &[]).unwrap();
        assert!(line.starts_with("cert-authority ssh-ed25519 "));
        assert!(!line.contains("principals="));
    }

    #[test]
    fn several_principals_are_comma_joined_inside_one_quoted_field() {
        let line = user_trust_line(&ca(), &["markik".into(), "research".into()]).unwrap();
        assert!(line.contains("principals=\"markik,research\""));
    }

    /// Re-enrolling must not stack duplicate lines, so the script filters by
    /// the marker before it appends.
    #[test]
    fn the_install_script_filters_before_it_appends() {
        let script = user_install_script("cert-authority ssh-ed25519 AAAA test");
        let filter = script.find("grep -v").expect("filters old lines");
        let append = script.find(">> \"$tmp\"").expect("appends the new line");
        assert!(filter < append, "the filter has to run before the append");
        assert!(script.contains("umask 077"));
        assert!(script.contains("chmod 600 \"$HOME/.ssh/authorized_keys\""));
    }

    /// Per-machine revocation only works if the id is per-machine and
    /// stable, and case is not a machine.
    #[test]
    fn device_ids_are_stable_per_host_and_case_insensitive() {
        assert_eq!(
            device_id_for_host("q-pc.local"),
            device_id_for_host("Q-PC.local")
        );
        assert_ne!(
            device_id_for_host("q-pc.local"),
            device_id_for_host("thinkpad.local")
        );
    }

    #[test]
    fn targets_split_into_user_and_host() {
        assert_eq!(
            split_target("markik@q-pc.local"),
            (Some("markik"), "q-pc.local")
        );
        assert_eq!(split_target("q-pc.local"), (None, "q-pc.local"));
    }

    #[test]
    fn the_system_snippet_is_instructions_and_not_commands_we_run() {
        let ca = ca();
        let snippet = system_sshd_snippet(
            &ca,
            "/etc/ssh/personae_ca.pub",
            Some("/etc/ssh/host-cert.pub"),
        )
        .unwrap();
        assert!(snippet.contains("TrustedUserCAKeys /etc/ssh/personae_ca.pub"));
        assert!(snippet.contains("HostCertificate /etc/ssh/host-cert.pub"));
        assert!(snippet.starts_with("# Run as root on the target"));

        let without = system_sshd_snippet(&ca, "/etc/ssh/personae_ca.pub", None).unwrap();
        assert!(!without.contains("HostCertificate"));
    }
}
