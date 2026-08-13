//! The receipt for the SSH-CA projection: a real `sshd` accepts a minted
//! certificate, with nothing in `authorized_keys` and no host-key prompt.
//!
//! Ignored by default. It needs an OpenSSH `sshd` binary and a free local
//! port, neither of which a library test may assume, and it is a receipt
//! rather than a guard: what it proves is that OpenSSH agrees with
//! [`personae::ssh_ca`]'s understanding of its own format, which cannot be
//! established by any amount of in-crate assertion.
//!
//! ```sh
//! cargo test -p personae --features ssh --test ssh_ca_live -- --ignored --nocapture
//! ```
//!
//! Everything happens in a temp directory under an unprivileged `sshd` on a
//! high port: no system configuration is read or written, and the test
//! authenticates the user running it to itself.

#![cfg(feature = "ssh")]

use std::fs;
use std::net::TcpListener;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use personae::carry::{
    ACTION_SSH_LOGIN, ACTION_SSH_PTY, DeviceId, device_capability_scope,
};
use personae::delegation::{
    DelegationCertificate, DelegationParent, SignedDelegationCertificate,
};
use personae::ssh_ca::{SshCertAuthority, UserCertRequest};
use personae::{IdentityProvider, InMemoryProvider};
use ssh_key::private::PrivateKey;
use ssh_key::public::PublicKey;
use ssh_key::{Algorithm, LineEnding};

/// `sshd` refuses to start if any directory above its keys is group- or
/// world-writable, so the whole fixture lives in a 0700 directory.
struct Fixture {
    dir: std::path::PathBuf,
    sshd: Option<Child>,
    port: u16,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(mut sshd) = self.sshd.take() {
            let _ = sshd.kill();
            let _ = sshd.wait();
        }
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after 1970")
        .as_millis() as u64
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("a free loopback port")
        .local_addr()
        .expect("bound address")
        .port()
}

fn write_private(path: &Path, key: &PrivateKey) {
    fs::write(path, key.to_openssh(LineEnding::LF).expect("encode key").as_bytes())
        .expect("write private key");
    set_mode(path, 0o600);
}

fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("chmod");
}

/// A grant authorizing SSH login on this device, signed by the master.
fn ssh_grant(provider: &InMemoryProvider) -> SignedDelegationCertificate {
    let master = provider.master_public_key().to_bytes();
    let now = now_ms();
    SignedDelegationCertificate::issue(
        provider,
        DelegationCertificate::new(
            DelegationParent::Root([7; 32]),
            master,
            master,
            device_capability_scope(
                DeviceId::from_uuid(uuid::Uuid::from_u128(0x2026_0812)),
                [ACTION_SSH_LOGIN, ACTION_SSH_PTY],
            ),
            now - 2000,
            now - 1000,
            Some(now + 600_000),
            0,
            [4; 32],
        ),
    )
    .expect("issue the ssh grant")
}

#[test]
#[ignore = "needs a local sshd; run explicitly with --ignored"]
fn a_live_sshd_accepts_a_minted_certificate() {
    let user = std::env::var("USER").expect("USER is set");
    let dir = std::env::temp_dir().join(format!("personae-ssh-ca-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create fixture dir");
    set_mode(&dir, 0o700);
    let mut fixture = Fixture {
        dir: dir.clone(),
        sshd: None,
        port: free_port(),
    };

    // The authority, derived from a master identity like any other key.
    let provider = InMemoryProvider::from_seed([1; 32]);
    let ca = SshCertAuthority::derive(&provider).expect("derive the CA");
    let ca_pub = dir.join("ca.pub");
    fs::write(&ca_pub, ca.trusted_user_ca_line().expect("ca line")).expect("write ca.pub");

    // The host's own key, plus a CA-signed certificate for it: this is what
    // retires `known_hosts` TOFU.
    let host_key = PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).unwrap();
    write_private(&dir.join("host_key"), &host_key);
    let host_cert = ca
        .mint_host_cert(
            &personae::ssh_ca::HostCertRequest {
                subject: &PublicKey::from(&host_key),
                principals: vec!["localhost".into(), "127.0.0.1".into()],
                valid_for_ms: 3_600_000,
            },
            now_ms(),
        )
        .expect("mint the host cert");
    fs::write(
        dir.join("host_key-cert.pub"),
        host_cert.to_openssh().expect("encode host cert"),
    )
    .expect("write host cert");

    // The user's key and its certificate. Note what is *absent*: no
    // authorized_keys file is ever written.
    let user_key = PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).unwrap();
    let user_key_path = dir.join("id_ed25519");
    write_private(&user_key_path, &user_key);
    let grant = ssh_grant(&provider);
    let user_cert = ca
        .mint_user_cert(
            &UserCertRequest {
                grant: &grant,
                subject: &PublicKey::from(&user_key),
                principals: vec![user.clone()],
                force_command: None,
                source_address: None,
            },
            now_ms(),
        )
        .expect("mint the user cert");
    let cert_path = dir.join("id_ed25519-cert.pub");
    fs::write(&cert_path, user_cert.to_openssh().expect("encode user cert"))
        .expect("write user cert");

    let config = dir.join("sshd_config");
    fs::write(
        &config,
        format!(
            "Port {port}\n\
             ListenAddress 127.0.0.1\n\
             HostKey {dir}/host_key\n\
             HostCertificate {dir}/host_key-cert.pub\n\
             TrustedUserCAKeys {dir}/ca.pub\n\
             AuthorizedKeysFile /dev/null\n\
             PidFile {dir}/sshd.pid\n\
             StrictModes no\n\
             UsePAM no\n\
             PasswordAuthentication no\n\
             KbdInteractiveAuthentication no\n\
             PubkeyAuthentication yes\n\
             LogLevel VERBOSE\n",
            port = fixture.port,
            dir = dir.display(),
        ),
    )
    .expect("write sshd_config");

    let log = fs::File::create(dir.join("sshd.log")).expect("create log");
    fixture.sshd = Some(
        Command::new("/usr/sbin/sshd")
            .arg("-D")
            .arg("-e")
            .arg("-f")
            .arg(&config)
            .stderr(Stdio::from(log))
            .stdout(Stdio::null())
            .spawn()
            .expect("spawn sshd (is /usr/sbin/sshd present?)"),
    );

    // Wait for the listener rather than sleeping a guessed interval.
    let deadline = Instant::now() + Duration::from_secs(10);
    while TcpListener::bind(("127.0.0.1", fixture.port)).is_ok() {
        assert!(
            Instant::now() < deadline,
            "sshd never bound port {}; log:\n{}",
            fixture.port,
            fs::read_to_string(dir.join("sshd.log")).unwrap_or_default()
        );
        std::thread::sleep(Duration::from_millis(50));
    }

    let known_hosts = dir.join("known_hosts");
    fs::write(
        &known_hosts,
        ca.known_hosts_ca_line("localhost,127.0.0.1")
            .expect("known_hosts line"),
    )
    .expect("write known_hosts");

    let out = Command::new("ssh")
        .args([
            "-i",
            user_key_path.to_str().unwrap(),
            "-o",
            &format!("CertificateFile={}", cert_path.display()),
            "-o",
            &format!("UserKnownHostsFile={}", known_hosts.display()),
            // The point of the host certificate: the client is told to
            // refuse anything it has not already been given authority for,
            // so a TOFU prompt would be a failure rather than a question.
            "-o",
            "StrictHostKeyChecking=yes",
            "-o",
            "IdentitiesOnly=yes",
            "-o",
            "PreferredAuthentications=publickey",
            "-o",
            "BatchMode=yes",
            "-p",
            &fixture.port.to_string(),
            &format!("{user}@localhost"),
            "echo personae-ca-ok",
        ])
        .output()
        .expect("run ssh");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let sshd_log = fs::read_to_string(dir.join("sshd.log")).unwrap_or_default();
    assert!(
        stdout.contains("personae-ca-ok"),
        "certificate login failed\n--- ssh stderr ---\n{stderr}\n--- sshd log ---\n{sshd_log}"
    );

    // The key id is the audit trail the plan promised: sshd logs the grant
    // id on the accepted login, which is what a KRL will later revoke by.
    let key_id = personae::ssh_ca::key_id_for(&grant.certificate.id());
    assert!(
        sshd_log.contains(&key_id),
        "sshd should log the grant id {key_id} it accepted\n--- sshd log ---\n{sshd_log}"
    );
    println!("sshd accepted the certificate; grant id {key_id} appears in its log");
}
