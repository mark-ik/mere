//! Native-only identity interactions.
//!
//! The browser may request one of these operations after admission, but it
//! never supplies or receives a local path, private-key byte, or passphrase.
//! A desktop UI implementation selects and unlocks the key inside the native
//! host process, then returns only a public mutation receipt.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use light_file_dialog::dialog::{Dialog, DialogBackend, InputBox, OpenFileDialog};
use personae::IdentityStorage;
use ssh_key::PrivateKey;
use zeroize::Zeroizing;

use crate::browser_carrier::{NativeIdentityAction, NativeIdentityFailure, NativeIdentityResult};
use crate::identity_projection::ImportSshKeyNativeIntentV1;
use crate::native::personae_host::PersonaeHost;

const MAX_SSH_PRIVATE_KEY_BYTES: u64 = 1024 * 1024;
const PICKER_START_ENV: &str = "GRAPHSHELL_NATIVE_PICKER_START";

/// Secret-bearing desktop interactions required by native identity actions.
///
/// Implementations own the selected path and passphrase. Neither appears in
/// the browser request or response types.
pub trait NativeIdentityUi: Send + Sync {
    fn pick_ssh_private_key(&self) -> Result<Option<PathBuf>, NativeIdentityFailure>;

    fn prompt_ssh_private_key_passphrase(
        &self,
    ) -> Result<Option<Zeroizing<String>>, NativeIdentityFailure>;
}

/// Cross-platform system-dialog implementation used by the installed host.
///
/// `light-file-dialog` uses Win32 dialogs on Windows, AppleScript on macOS,
/// and an available graphical dialog provider on Linux. Graphshell refuses
/// its console fallback because stdout belongs exclusively to native
/// messaging.
#[derive(Clone, Debug, Default)]
pub struct SystemNativeIdentityUi {
    picker_start: Option<PathBuf>,
}

impl SystemNativeIdentityUi {
    /// Override the initial picker location for an embedding host or receipt.
    pub fn with_picker_start(path: PathBuf) -> Self {
        Self {
            picker_start: Some(path),
        }
    }

    fn require_graphical_backend(&self) -> Result<(), NativeIdentityFailure> {
        light_file_dialog::set_verbose(0);
        light_file_dialog::set_silent(1);
        light_file_dialog::set_force_console(0);
        DialogBackend::query()
            .graphic
            .then_some(())
            .ok_or(NativeIdentityFailure::UiUnavailable)
    }
}

impl NativeIdentityUi for SystemNativeIdentityUi {
    fn pick_ssh_private_key(&self) -> Result<Option<PathBuf>, NativeIdentityFailure> {
        self.require_graphical_backend()?;
        let mut dialog = OpenFileDialog::new("Import SSH private key")
            .filter_description("OpenSSH private keys");
        let configured_start = self
            .picker_start
            .as_deref()
            .map(|path| path.as_os_str().to_owned())
            .or_else(|| std::env::var_os(PICKER_START_ENV));
        if let Some(start) = configured_start {
            dialog = dialog.default_path(start.to_string_lossy());
        }
        Ok(dialog.show().map(PathBuf::from))
    }

    fn prompt_ssh_private_key_passphrase(
        &self,
    ) -> Result<Option<Zeroizing<String>>, NativeIdentityFailure> {
        self.require_graphical_backend()?;
        Ok(InputBox::new(
            "Graphshell",
            "Enter the passphrase for the selected SSH private key.",
        )
        .password()
        .show()
        .map(Zeroizing::new))
    }
}

/// Refusal implementation for hosts that deliberately expose no desktop UI.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableNativeIdentityUi;

impl NativeIdentityUi for UnavailableNativeIdentityUi {
    fn pick_ssh_private_key(&self) -> Result<Option<PathBuf>, NativeIdentityFailure> {
        Err(NativeIdentityFailure::UiUnavailable)
    }

    fn prompt_ssh_private_key_passphrase(
        &self,
    ) -> Result<Option<Zeroizing<String>>, NativeIdentityFailure> {
        Err(NativeIdentityFailure::UiUnavailable)
    }
}

/// Apply one fixed native interaction against the resident Personae host.
pub fn apply_native_identity_action<S, U>(
    host: &Arc<PersonaeHost<S>>,
    ui: &U,
    action: NativeIdentityAction,
) -> NativeIdentityResult
where
    S: IdentityStorage + 'static,
    U: NativeIdentityUi,
{
    match action {
        NativeIdentityAction::ImportSshPrivate { unlock_policy } => {
            import_ssh_private(host, ui, ImportSshKeyNativeIntentV1 { unlock_policy })
        }
    }
}

fn import_ssh_private<S, U>(
    host: &Arc<PersonaeHost<S>>,
    ui: &U,
    options: ImportSshKeyNativeIntentV1,
) -> NativeIdentityResult
where
    S: IdentityStorage + 'static,
    U: NativeIdentityUi,
{
    let path = match ui.pick_ssh_private_key() {
        Ok(Some(path)) => path,
        Ok(None) => return NativeIdentityResult::Cancelled,
        Err(reason) => return NativeIdentityResult::Rejected { reason },
    };
    let metadata = match fs::metadata(&path) {
        Ok(metadata) if metadata.is_file() => metadata,
        _ => {
            return NativeIdentityResult::Rejected {
                reason: NativeIdentityFailure::SelectedFileUnreadable,
            };
        }
    };
    if metadata.len() > MAX_SSH_PRIVATE_KEY_BYTES {
        return NativeIdentityResult::Rejected {
            reason: NativeIdentityFailure::SelectedFileTooLarge,
        };
    }
    let bytes = match fs::read(&path) {
        Ok(bytes) => Zeroizing::new(bytes),
        Err(_) => {
            return NativeIdentityResult::Rejected {
                reason: NativeIdentityFailure::SelectedFileUnreadable,
            };
        }
    };
    if bytes.len() as u64 > MAX_SSH_PRIVATE_KEY_BYTES {
        return NativeIdentityResult::Rejected {
            reason: NativeIdentityFailure::SelectedFileTooLarge,
        };
    }
    let mut private = match PrivateKey::from_openssh(bytes.as_slice()) {
        Ok(private) => private,
        Err(_) => {
            return NativeIdentityResult::Rejected {
                reason: NativeIdentityFailure::InvalidPrivateKey,
            };
        }
    };
    if private.is_encrypted() {
        let passphrase = match ui.prompt_ssh_private_key_passphrase() {
            Ok(Some(passphrase)) => passphrase,
            Ok(None) => return NativeIdentityResult::Cancelled,
            Err(reason) => return NativeIdentityResult::Rejected { reason },
        };
        private = match private.decrypt(passphrase.as_bytes()) {
            Ok(private) => private,
            Err(_) => {
                return NativeIdentityResult::Rejected {
                    reason: NativeIdentityFailure::IncorrectPassphrase,
                };
            }
        };
    }

    match host.import_ssh_private(private, options) {
        Ok(receipt) => NativeIdentityResult::ImportedSshPrivate {
            fingerprint: receipt.fingerprint,
            comment: receipt.comment,
            unlock_policy: receipt.unlock_policy,
            replaced_existing: receipt.replaced_existing,
        },
        Err(_) => NativeIdentityResult::Rejected {
            reason: NativeIdentityFailure::ImportRejected,
        },
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use personae::{Ed25519Keypair, IdentityVault, InMemoryStorage, Profile, ProfileId};
    use ssh_key::{Algorithm, LineEnding};

    use super::*;
    use crate::browser_carrier::NativeIdentityAction;
    use crate::identity::VaultProtectionView;
    use crate::identity_projection::SshUnlockPolicyIntentV1;

    struct ScriptedUi {
        path: PathBuf,
        passphrases: Mutex<Vec<String>>,
    }

    impl NativeIdentityUi for ScriptedUi {
        fn pick_ssh_private_key(&self) -> Result<Option<PathBuf>, NativeIdentityFailure> {
            Ok(Some(self.path.clone()))
        }

        fn prompt_ssh_private_key_passphrase(
            &self,
        ) -> Result<Option<Zeroizing<String>>, NativeIdentityFailure> {
            Ok(self.passphrases.lock().unwrap().pop().map(Zeroizing::new))
        }
    }

    fn host() -> Arc<PersonaeHost<InMemoryStorage>> {
        let profile = Profile::new(
            ProfileId("native-ui".to_string()),
            "Native UI",
            Ed25519Keypair::from_seed([0x81; 32]),
        );
        Arc::new(PersonaeHost::new(
            IdentityVault::with_profile(InMemoryStorage::new(), profile),
            None,
            VaultProtectionView::Ephemeral,
        ))
    }

    fn scratch_key(tag: &str, passphrase: &str) -> (PathBuf, String) {
        let mut private = PrivateKey::random(&mut rand_core::OsRng, Algorithm::Ed25519).unwrap();
        private.set_comment("native picker receipt");
        let fingerprint = private.fingerprint(ssh_key::HashAlg::Sha256).to_string();
        let private = private.encrypt(&mut rand_core::OsRng, passphrase).unwrap();
        let encoded = private.to_openssh(LineEnding::LF).unwrap();
        let path = std::env::temp_dir().join(format!(
            "graphshell-native-import-{tag}-{}",
            std::process::id()
        ));
        fs::write(&path, encoded.as_bytes()).unwrap();
        (path, fingerprint)
    }

    #[test]
    fn encrypted_key_is_unlocked_and_imported_without_a_serialized_secret() {
        let (path, fingerprint) = scratch_key("ok", "correct horse");
        let ui = ScriptedUi {
            path: path.clone(),
            passphrases: Mutex::new(vec!["correct horse".to_string()]),
        };
        let host = host();
        let result = apply_native_identity_action(
            &host,
            &ui,
            NativeIdentityAction::ImportSshPrivate {
                unlock_policy: SshUnlockPolicyIntentV1::PerUse,
            },
        );
        assert!(matches!(
            &result,
            NativeIdentityResult::ImportedSshPrivate {
                fingerprint: imported,
                ..
            } if imported == &fingerprint
        ));
        assert_eq!(host.snapshot().unwrap().ssh_keys.len(), 1);

        let serialized = serde_json::to_string(&result).unwrap();
        assert!(!serialized.contains("correct horse"));
        assert!(!serialized.contains("BEGIN OPENSSH PRIVATE KEY"));
        assert!(!serialized.contains(path.to_string_lossy().as_ref()));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn wrong_passphrase_imports_nothing_and_returns_a_bounded_reason() {
        let (path, _) = scratch_key("wrong", "correct horse");
        let ui = ScriptedUi {
            path: path.clone(),
            passphrases: Mutex::new(vec!["wrong horse".to_string()]),
        };
        let host = host();
        let result = apply_native_identity_action(
            &host,
            &ui,
            NativeIdentityAction::ImportSshPrivate {
                unlock_policy: SshUnlockPolicyIntentV1::ShortTtl { idle_seconds: 90 },
            },
        );
        assert_eq!(
            result,
            NativeIdentityResult::Rejected {
                reason: NativeIdentityFailure::IncorrectPassphrase
            }
        );
        assert!(host.snapshot().unwrap().ssh_keys.is_empty());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn unavailable_ui_cannot_downgrade_to_browser_secret_input() {
        let result = apply_native_identity_action(
            &host(),
            &UnavailableNativeIdentityUi,
            NativeIdentityAction::ImportSshPrivate {
                unlock_policy: SshUnlockPolicyIntentV1::Session,
            },
        );
        assert_eq!(
            result,
            NativeIdentityResult::Rejected {
                reason: NativeIdentityFailure::UiUnavailable
            }
        );
    }
}
