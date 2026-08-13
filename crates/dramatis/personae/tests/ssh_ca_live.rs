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

use personae::carry::{ACTION_SSH_LOGIN, ACTION_SSH_PTY, DeviceId, device_capability_scope};
use personae::delegation::{DelegationCertificate, DelegationParent, SignedDelegationCertificate};
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
    fs::write(
        path,
        key.to_openssh(LineEnding::LF)
            .expect("encode key")
            .as_bytes(),
    )
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
    fs::write(
        &cert_path,
        user_cert.to_openssh().expect("encode user cert"),
    )
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

/// The enrollment ceremony's claim, checked against `sshd` itself: one
/// `cert-authority` line in a user's own `authorized_keys` — no root, no
/// `sshd_config`, no daemon restart — is enough for every certificate that
/// authority ever signs.
///
/// The fixture redirects `HOME`, so the script runs verbatim against a
/// throwaway home rather than the tester's.
#[test]
#[ignore = "needs a local sshd; run explicitly with --ignored"]
fn enrollment_teaches_a_host_to_accept_the_authority() {
    let user = std::env::var("USER").expect("USER is set");
    let dir = std::env::temp_dir().join(format!("personae-enroll-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join(".ssh")).expect("create fixture home");
    set_mode(&dir, 0o700);
    set_mode(&dir.join(".ssh"), 0o700);
    let mut fixture = Fixture {
        dir: dir.clone(),
        sshd: None,
        port: free_port(),
    };

    let provider = InMemoryProvider::from_seed([2; 32]);
    let ca = SshCertAuthority::derive(&provider).expect("derive the CA");

    // Run the real enrollment script against the fixture home. This is the
    // same string `personae-vault enroll-host` pipes into `ssh`.
    let line = personae::enroll::user_trust_line(&ca, &[user.clone()]).expect("trust line");
    let script = personae::enroll::user_install_script(&line);
    let enrolled = run_script(&script, &dir);
    assert!(
        enrolled.contains("enrolled"),
        "the enrollment script did not confirm: {enrolled}"
    );

    let authorized = dir.join(".ssh/authorized_keys");
    let contents = fs::read_to_string(&authorized).expect("read authorized_keys");
    assert_eq!(
        contents.lines().count(),
        1,
        "enrollment writes exactly one line, got:\n{contents}"
    );
    assert!(contents.starts_with("cert-authority,principals="));

    // Re-running must replace rather than stack: enrollment is idempotent.
    run_script(&script, &dir);
    let contents = fs::read_to_string(&authorized).expect("read authorized_keys");
    assert_eq!(
        contents.lines().count(),
        1,
        "re-enrolling stacked a duplicate line:\n{contents}"
    );

    let host_key = PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).unwrap();
    write_private(&dir.join("host_key"), &host_key);
    let config = dir.join("sshd_config");
    fs::write(
        &config,
        format!(
            "Port {port}\n\
             ListenAddress 127.0.0.1\n\
             HostKey {dir}/host_key\n\
             AuthorizedKeysFile {dir}/.ssh/authorized_keys\n\
             PidFile {dir}/sshd.pid\n\
             StrictModes no\n\
             UsePAM no\n\
             PasswordAuthentication no\n\
             KbdInteractiveAuthentication no\n\
             LogLevel VERBOSE\n",
            port = fixture.port,
            dir = dir.display(),
        ),
    )
    .expect("write sshd_config");
    // Note what is *not* in that config: no TrustedUserCAKeys. The trust
    // lives entirely in the user's own authorized_keys.

    let log = fs::File::create(dir.join("sshd.log")).expect("create log");
    fixture.sshd = Some(
        Command::new("/usr/sbin/sshd")
            .args(["-D", "-e", "-f"])
            .arg(&config)
            .stderr(Stdio::from(log))
            .stdout(Stdio::null())
            .spawn()
            .expect("spawn sshd"),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while TcpListener::bind(("127.0.0.1", fixture.port)).is_ok() {
        assert!(Instant::now() < deadline, "sshd never bound its port");
        std::thread::sleep(Duration::from_millis(50));
    }

    // A key the host has never seen, carrying a certificate from the
    // authority it now trusts.
    let user_key = PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).unwrap();
    let user_key_path = dir.join("id_ed25519");
    write_private(&user_key_path, &user_key);
    let grant = ssh_grant(&provider);
    let cert = ca
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
        .expect("mint");
    let cert_path = dir.join("id_ed25519-cert.pub");
    fs::write(&cert_path, cert.to_openssh().expect("encode")).expect("write cert");

    let out = Command::new("ssh")
        .args([
            "-i",
            user_key_path.to_str().unwrap(),
            "-o",
            &format!("CertificateFile={}", cert_path.display()),
            "-o",
            &format!("UserKnownHostsFile={}", dir.join("known_hosts").display()),
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "IdentitiesOnly=yes",
            "-o",
            "BatchMode=yes",
            "-p",
            &fixture.port.to_string(),
            &format!("{user}@127.0.0.1"),
            "echo enrolled-login-ok",
        ])
        .output()
        .expect("run ssh");

    let sshd_log = fs::read_to_string(dir.join("sshd.log")).unwrap_or_default();
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("enrolled-login-ok"),
        "login failed after enrollment\n--- ssh stderr ---\n{}\n--- sshd log ---\n{sshd_log}",
        String::from_utf8_lossy(&out.stderr)
    );

    // The bare key alone must still be refused: what was enrolled is the
    // authority, not this key.
    let bare = Command::new("ssh")
        .args([
            "-i",
            user_key_path.to_str().unwrap(),
            "-o",
            "CertificateFile=/dev/null",
            "-o",
            &format!("UserKnownHostsFile={}", dir.join("known_hosts").display()),
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "IdentitiesOnly=yes",
            "-o",
            "BatchMode=yes",
            "-p",
            &fixture.port.to_string(),
            &format!("{user}@127.0.0.1"),
            "echo should-not-happen",
        ])
        .output()
        .expect("run ssh");
    assert!(
        !String::from_utf8_lossy(&bare.stdout).contains("should-not-happen"),
        "the uncertified key logged in, so the host trusts more than the authority"
    );
    println!("one cert-authority line, no root: certified login accepted, bare key refused");
}

/// Faces, checked where it counts: the same host, the same authority, two
/// personae with different reach. The burner is refused by `sshd` on the
/// principal restriction alone, before its (empty) extensions matter, and a
/// forced command replaces whatever the client asks for.
#[test]
#[ignore = "needs a local sshd; run explicitly with --ignored"]
fn a_face_reaches_exactly_as_far_as_its_policy() {
    let user = std::env::var("USER").expect("USER is set");
    let dir = std::env::temp_dir().join(format!("personae-face-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join(".ssh")).expect("create fixture home");
    set_mode(&dir, 0o700);
    set_mode(&dir.join(".ssh"), 0o700);
    let mut fixture = Fixture {
        dir: dir.clone(),
        sshd: None,
        port: free_port(),
    };

    let provider = InMemoryProvider::from_seed([3; 32]);
    let ca = SshCertAuthority::derive(&provider).expect("derive the CA");

    // The host is enrolled for the work face's principal only.
    let line = personae::enroll::user_trust_line(&ca, &[user.clone()]).expect("trust line");
    run_script(&personae::enroll::user_install_script(&line), &dir);

    let host_key = PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).unwrap();
    write_private(&dir.join("host_key"), &host_key);
    let config = dir.join("sshd_config");
    fs::write(
        &config,
        format!(
            "Port {port}\nListenAddress 127.0.0.1\nHostKey {dir}/host_key\n\
             AuthorizedKeysFile {dir}/.ssh/authorized_keys\nPidFile {dir}/sshd.pid\n\
             StrictModes no\nUsePAM no\nPasswordAuthentication no\n\
             KbdInteractiveAuthentication no\nLogLevel VERBOSE\n",
            port = fixture.port,
            dir = dir.display(),
        ),
    )
    .expect("write sshd_config");
    let log = fs::File::create(dir.join("sshd.log")).expect("create log");
    fixture.sshd = Some(
        Command::new("/usr/sbin/sshd")
            .args(["-D", "-e", "-f"])
            .arg(&config)
            .stderr(Stdio::from(log))
            .stdout(Stdio::null())
            .spawn()
            .expect("spawn sshd"),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while TcpListener::bind(("127.0.0.1", fixture.port)).is_ok() {
        assert!(Instant::now() < deadline, "sshd never bound its port");
        std::thread::sleep(Duration::from_millis(50));
    }

    let key = PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).unwrap();
    let key_path = dir.join("id_ed25519");
    write_private(&key_path, &key);
    let grant = ssh_grant(&provider);

    let mint = |principals: Vec<String>, force: Option<String>, name: &str| {
        let cert = ca
            .mint_user_cert(
                &UserCertRequest {
                    grant: &grant,
                    subject: &PublicKey::from(&key),
                    principals,
                    force_command: force,
                    source_address: None,
                },
                now_ms(),
            )
            .expect("mint");
        let path = dir.join(format!("{name}-cert.pub"));
        fs::write(&path, cert.to_openssh().expect("encode")).expect("write cert");
        path
    };
    let login = |cert: &Path, command: &str| -> String {
        let out = Command::new("ssh")
            .args([
                "-i",
                key_path.to_str().unwrap(),
                "-o",
                &format!("CertificateFile={}", cert.display()),
                "-o",
                &format!("UserKnownHostsFile={}", dir.join("known_hosts").display()),
                "-o",
                "StrictHostKeyChecking=no",
                "-o",
                "IdentitiesOnly=yes",
                "-o",
                "BatchMode=yes",
                "-p",
                &fixture.port.to_string(),
                &format!("{user}@127.0.0.1"),
                command,
            ])
            .output()
            .expect("run ssh");
        String::from_utf8_lossy(&out.stdout).into_owned()
    };

    // The work face: its principal is the one the host was enrolled for.
    let work = mint(vec![user.clone()], None, "work");
    assert!(
        login(&work, "echo work-face-ok").contains("work-face-ok"),
        "the work face should reach this host"
    );

    // The burner: same authority, same key, same host — a principal the
    // enrollment line does not list.
    let burner = mint(vec!["burner".into()], None, "burner");
    assert!(
        !login(&burner, "echo burner-got-in").contains("burner-got-in"),
        "the burner's principal is not enrolled here; sshd must refuse it"
    );

    // A forced command outranks whatever the client asks for.
    let forced = mint(
        vec![user.clone()],
        Some("echo forced-instead".into()),
        "forced",
    );
    let out = login(&forced, "echo client-asked-for-this");
    assert!(
        out.contains("forced-instead"),
        "force-command did not run: {out:?}"
    );
    assert!(
        !out.contains("client-asked-for-this"),
        "the client's own command ran despite force-command: {out:?}"
    );
    println!("work face in, burner refused on the same host, forced command in effect");
}

/// Run an enrollment script the way `ssh` would, but against `home`.
fn run_script(script: &str, home: &Path) -> String {
    let mut child = Command::new("sh")
        .arg("-s")
        .env("HOME", home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn sh");
    use std::io::Write;
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(script.as_bytes())
        .expect("write script");
    let out = child.wait_with_output().expect("wait for sh");
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}
