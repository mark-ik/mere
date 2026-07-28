//! Headed H4e receipt host for native encrypted-key import.
//!
//! The fixture is encrypted and temporary. The browser sees only the
//! advertised import action and public result; the real desktop picker and
//! password dialog stay inside this native process.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use graphshell::browser_carrier::{AllowedExtensions, BrowserLauncher};
use graphshell::identity::VaultProtectionView;
use graphshell::native::browser_host::serve_identity_native_messages;
use graphshell::native::identity_ui::SystemNativeIdentityUi;
use graphshell::native::personae_host::PersonaeHost;
use personae::{
    Ed25519Keypair, IdentityVault, InMemoryProvider, InMemoryStorage, Profile, ProfileId,
};
use ssh_key::{Algorithm, LineEnding, PrivateKey};

const RECEIPT_PASSPHRASE: &str = "graphshell H4e receipt";
const FIXTURE_PATH_ENV: &str = "GRAPHSHELL_H4E_FIXTURE_PATH";

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("graphshell H4e receipt host: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let launcher = BrowserLauncher::parse(&args)?;
    AllowedExtensions::default().admit(&launcher)?;

    let fixture_path = fixture_path();
    let (fixture, fingerprint) = EncryptedFixture::create(fixture_path)?;
    let profile = Profile::new(
        ProfileId("receipt".to_string()),
        "H4e receipt",
        Ed25519Keypair::from_seed([0x84; 32]),
    );
    let host = Arc::new(PersonaeHost::new(
        IdentityVault::with_profile(InMemoryStorage::new(), profile),
        None,
        VaultProtectionView::Ephemeral,
    ));
    let identity = InMemoryProvider::from_seed([0x85; 32]);
    let ui = SystemNativeIdentityUi::with_picker_start(fixture.path.clone());
    let mut reader = tokio::io::stdin();
    let mut writer = tokio::io::stdout();
    let summary = serve_identity_native_messages(
        &identity,
        Arc::clone(&host),
        &ui,
        launcher,
        &mut reader,
        &mut writer,
        10 * 60 * 1_000,
    )
    .await?
    .ok_or("browser closed before connecting")?;

    let snapshot = host.snapshot()?;
    let imported = snapshot
        .ssh_keys
        .iter()
        .find(|key| key.fingerprint == fingerprint)
        .ok_or("the headed browser did not import the receipt key")?;
    if imported.unlock_policy != "every use" {
        return Err("the headed browser did not retain the selected per-use policy".into());
    }
    eprintln!(
        "graphshell H4e receipt host: imported {}; served {} request(s); ended {:?}",
        imported.fingerprint, summary.answered, summary.end
    );
    drop(fixture);
    Ok(())
}

fn fixture_path() -> PathBuf {
    std::env::var_os(FIXTURE_PATH_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!(
                "graphshell-h4e-import-fixture-{}",
                std::process::id()
            ))
        })
}

struct EncryptedFixture {
    path: PathBuf,
}

impl EncryptedFixture {
    fn create(path: PathBuf) -> Result<(Self, String), Box<dyn std::error::Error>> {
        let mut private = PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519)?;
        private.set_comment("H4e native picker receipt");
        let fingerprint = private.fingerprint(ssh_key::HashAlg::Sha256).to_string();
        let private = private.encrypt(&mut rand_core::OsRng, RECEIPT_PASSPHRASE)?;
        let encoded = private.to_openssh(LineEnding::LF)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        file.write_all(encoded.as_bytes())?;
        file.flush()?;
        Ok((Self { path }, fingerprint))
    }
}

impl Drop for EncryptedFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
