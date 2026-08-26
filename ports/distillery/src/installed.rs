// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Installed-product authority: a persisted Personae choice plus private paths.
//!
//! This module deliberately owns neither scheduler nor device policy. A caller
//! supplies the mesh store, its retention policy, [`mesh_host::HostConfig`],
//! and [`crate::ResidentSettings`] when it binds a resident. Those are mesh
//! and device facts, not preferences Distillery may quietly invent.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mesh::{MESH_AUTHOR_SALT, MeshStore, SyncedMesh};
use mesh_host::{HostConfig, MeshHost};
use muniment::Backend;
use personae::bootstrap::{self, Unlock};
use personae::vault::{IdentityStorage, IdentityVault, ProfileId};
use personae::{DerivedKeyAttestation, Ed25519Keypair, IdentityError, IdentityProvider};
use serde::{Deserialize, Serialize};
use transport::{P2pandaTransport, TransportError};

use crate::{ResidentAuthority, ResidentError, ResidentSettings, ResidentStorage};

const DISTILLERY_DIR: &str = "distillery";
const SETTINGS_FILENAME: &str = "settings.json";
const MESHES_DIR: &str = "meshes";
const MESH_STORE_FILENAME: &str = "mesh.redb";
const BLOB_STORE_DIR: &str = "blobs";

/// Product-owned settings path: `<data_root>/distillery/settings.json`.
pub fn distillery_settings_path(data_root: &Path) -> PathBuf {
    data_root.join(DISTILLERY_DIR).join(SETTINGS_FILENAME)
}

/// The one durable choice Distillery makes before a mesh is opened.
///
/// The profile is an explicit Personae profile id, not an environment seed or
/// an application-local replacement identity. Its secret stays in the shared
/// Personae vault; this file carries only the public-facing profile name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstalledSettings {
    /// scope=application; movement=local-only; mutability=restart-required;
    /// security=ordinary. The Personae profile authorized to speak for this
    /// installed Distillery instance.
    pub profile: String,
}

impl InstalledSettings {
    /// Record an explicitly selected profile.
    pub fn new(profile: ProfileId) -> Result<Self, InstalledSettingsError> {
        let settings = Self { profile: profile.0 };
        settings.validate()?;
        Ok(settings)
    }

    /// Recover the typed Personae profile id.
    pub fn profile_id(&self) -> ProfileId {
        ProfileId(self.profile.clone())
    }

    /// Reject a non-choice instead of treating it as a hidden default profile.
    pub fn validate(&self) -> Result<(), InstalledSettingsError> {
        if self.profile.trim().is_empty() {
            return Err(InstalledSettingsError::InvalidProfile);
        }
        Ok(())
    }

    /// Load settings. An absent file means the product has not been configured.
    pub fn load(data_root: &Path) -> Result<Option<Self>, InstalledSettingsError> {
        let path = distillery_settings_path(data_root);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(InstalledSettingsError::file(&path, error)),
        };
        let settings: Self = serde_json::from_str(&text)
            .map_err(|error| InstalledSettingsError::file(&path, error))?;
        settings.validate()?;
        Ok(Some(settings))
    }

    /// Save through a sibling temporary file. Windows does not replace an
    /// existing destination on rename, so its replace path follows Knot's
    /// established remove-then-rename fallback.
    pub fn save(&self, data_root: &Path) -> Result<(), InstalledSettingsError> {
        self.validate()?;
        let path = distillery_settings_path(data_root);
        let parent = path
            .parent()
            .expect("settings path has a product directory");
        fs::create_dir_all(parent).map_err(|error| InstalledSettingsError::file(&path, error))?;
        let mut json = serde_json::to_string_pretty(self)
            .map_err(|error| InstalledSettingsError::file(&path, error))?;
        json.push('\n');
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, json).map_err(|error| InstalledSettingsError::file(&path, error))?;
        if path.exists() {
            fs::remove_file(&path).map_err(|error| InstalledSettingsError::file(&path, error))?;
        }
        fs::rename(&temporary, &path).map_err(|error| InstalledSettingsError::file(&path, error))
    }
}

/// A persisted settings failure.
#[derive(Debug, thiserror::Error)]
pub enum InstalledSettingsError {
    /// A file could not be read, parsed, or replaced.
    #[error("Distillery settings at {path}: {message}")]
    File {
        /// The failing file.
        path: String,
        /// The operating-system or serialization error.
        message: String,
    },
    /// The profile field did not name a profile.
    #[error("Distillery settings name an empty Personae profile")]
    InvalidProfile,
}

impl InstalledSettingsError {
    fn file(path: &Path, error: impl std::fmt::Display) -> Self {
        Self::File {
            path: path.display().to_string(),
            message: error.to_string(),
        }
    }
}

/// Private durable paths for one configured mesh.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DistilleryPaths {
    root: PathBuf,
    mesh_id: [u8; 32],
}

impl DistilleryPaths {
    /// Build paths under the installed product root.
    pub fn for_mesh(data_root: &Path, mesh_id: [u8; 32]) -> Self {
        Self {
            root: data_root
                .join(DISTILLERY_DIR)
                .join(MESHES_DIR)
                .join(hex(mesh_id)),
            mesh_id,
        }
    }

    /// The mesh this path set belongs to.
    pub fn mesh_id(&self) -> [u8; 32] {
        self.mesh_id
    }

    /// Parent directory for this mesh's private state.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The redb database path a mesh-policy owner may open.
    pub fn mesh_store_path(&self) -> PathBuf {
        self.root.join(MESH_STORE_FILENAME)
    }

    /// The persistent blob-store root mounted by the resident.
    pub fn blob_store_root(&self) -> PathBuf {
        self.root.join(BLOB_STORE_DIR)
    }
}

/// A configured, unlocked installed identity.
///
/// It is intentionally useful before a resident exists: callers can expose
/// the selected profile and its protection, derive the mesh author, or supply
/// the transport identity while their mesh and device owners assemble their
/// respective facts.
pub struct InstalledAuthority {
    data_root: PathBuf,
    settings: InstalledSettings,
    vault: IdentityVault<Box<dyn IdentityStorage>>,
    protection: String,
}

impl InstalledAuthority {
    /// Persist the explicit profile selection for this installation.
    pub fn configure(
        data_root: &Path,
        profile: ProfileId,
    ) -> Result<InstalledSettings, InstalledError> {
        let settings = InstalledSettings::new(profile)?;
        settings.save(data_root)?;
        Ok(settings)
    }

    /// Open the selected profile from the ordinary shared Personae vault.
    pub fn open(data_root: &Path) -> Result<Self, InstalledError> {
        Self::open_with(
            data_root,
            &bootstrap::default_vault_dir(),
            Unlock::from_env(),
        )
    }

    /// Open against a named vault and unlock method.
    ///
    /// This refuses an absent profile rather than silently minting a new one:
    /// configuring an installed port must bind the owner's existing face, not
    /// surprise them with another identity.
    pub fn open_with(
        data_root: &Path,
        vault_dir: &Path,
        unlock: Unlock,
    ) -> Result<Self, InstalledError> {
        let settings = InstalledSettings::load(data_root)?.ok_or(InstalledError::Unconfigured)?;
        let opened = bootstrap::open_storage(vault_dir, unlock)?;
        let vault = IdentityVault::open(opened.storage, &settings.profile_id())?;
        Ok(Self {
            data_root: data_root.to_path_buf(),
            settings,
            vault,
            protection: opened.description,
        })
    }

    /// The private application root supplied by the installed host.
    pub fn data_root(&self) -> &Path {
        &self.data_root
    }

    /// The persisted settings that selected this identity.
    pub fn settings(&self) -> &InstalledSettings {
        &self.settings
    }

    /// The Personae profile this installed port speaks as.
    pub fn profile(&self) -> ProfileId {
        self.settings.profile_id()
    }

    /// Personae's account of how the selected profile is protected at rest.
    pub fn protection(&self) -> &str {
        &self.protection
    }

    /// The master transport identity belonging to the selected Personae profile.
    pub fn transport_identity(&self) -> &Ed25519Keypair {
        &self.vault.current_profile().master
    }

    /// The mesh author derived under the selected profile.
    pub fn mesh_author(&self) -> Result<Ed25519Keypair, InstalledError> {
        Ok(self.vault.derive_keypair(MESH_AUTHOR_SALT)?)
    }

    /// The evidence peers need to connect the mesh author to this profile's
    /// transport identity.
    pub fn mesh_author_attestation(&self) -> Result<DerivedKeyAttestation, InstalledError> {
        Ok(self.vault.attest_derived_key(MESH_AUTHOR_SALT)?)
    }

    /// Product-owned locations for one mesh. The caller still supplies its
    /// mesh store and retention policy before anything is opened.
    pub fn paths(&self, mesh_id: [u8; 32]) -> DistilleryPaths {
        DistilleryPaths::for_mesh(&self.data_root, mesh_id)
    }

    /// Bind an installed resident from facts owned by the mesh and device hosts.
    ///
    /// `store` carries mesh retention/admission truth. `host_config` is formed
    /// after receiving this resident's blob space, so the device owner supplies
    /// its own scheduler, resources, conditions, and policy without Distillery
    /// fabricating any of them.
    pub async fn bind_resident<B, F>(
        self,
        mesh_id: [u8; 32],
        store: MeshStore<B>,
        settings: ResidentSettings,
        host_config: F,
    ) -> Result<ResidentAuthority<B>, InstalledError>
    where
        B: Backend + Clone + Send + Sync + 'static,
        F: FnOnce(Arc<mesh_host::TransportBlobSpace>) -> HostConfig,
    {
        let paths = self.paths(mesh_id);
        let storage =
            ResidentStorage::open(paths.blob_store_root(), mesh_id, settings.blob_gc_every).await?;
        let blobs = storage.blobs();
        let transport = Arc::new(
            P2pandaTransport::builder(self.transport_identity())
                .gossip()
                .blobs(&blobs)
                .bind()
                .await?,
        );
        let (endpoint, gossip) = transport
            .sync_parts()
            .ok_or(InstalledError::MissingGossip)?;
        let synced = SyncedMesh::join(endpoint, gossip, store, mesh_id).await?;
        let host = MeshHost::new(synced, self.mesh_author()?, host_config(storage.space()));
        Ok(ResidentAuthority::new(host, transport, storage, settings)?)
    }
}

/// Failure while opening or binding an installed Distillery authority.
#[derive(Debug, thiserror::Error)]
pub enum InstalledError {
    /// The installed product has not selected a Personae profile yet.
    #[error("Distillery is not configured: select and persist a Personae profile first")]
    Unconfigured,
    /// Product-owned settings failed to load or save.
    #[error(transparent)]
    Settings(#[from] InstalledSettingsError),
    /// Personae could not unlock or load the selected profile.
    #[error(transparent)]
    Personae(#[from] IdentityError),
    /// The p2p transport could not bind.
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// The mesh store or sync lane rejected the join.
    #[error(transparent)]
    Mesh(#[from] mesh::MeshSyncError),
    /// The resident could not open or compose its durable resources.
    #[error(transparent)]
    Resident(#[from] ResidentError),
    /// A resident binding requires a gossip-enabled transport.
    #[error("Distillery installed authority bound without a gossip transport")]
    MissingGossip,
}

fn hex(bytes: [u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use mesh::{
        AvailabilityPolicy, ErasurePolicy, KeepBound, LeasePolicy, MeshRetentionPolicy,
        PolicyRevision,
    };
    use personae::bootstrap::load_or_create_profile;

    use super::*;
    use crate::RetentionSettings;

    const MESH: [u8; 32] = [0xD1; 32];

    fn unlock() -> Unlock {
        Unlock::passphrase(b"distillery-installed-test-passphrase")
    }

    fn provision_profile(vault_dir: &Path, profile: &ProfileId) {
        let opened = bootstrap::open_storage(vault_dir, unlock()).unwrap();
        let (_, created) = load_or_create_profile(&*opened.storage, profile).unwrap();
        assert!(created, "test profile starts absent");
    }

    #[test]
    fn settings_bind_only_an_explicit_profile_and_reject_unknown_policy() {
        let directory = tempfile::tempdir().unwrap();
        let settings =
            InstalledAuthority::configure(directory.path(), ProfileId("research".into())).unwrap();

        assert_eq!(settings.profile, "research");
        assert_eq!(
            distillery_settings_path(directory.path()),
            directory.path().join("distillery/settings.json")
        );
        assert_eq!(
            InstalledSettings::load(directory.path()).unwrap(),
            Some(settings)
        );
        InstalledAuthority::configure(directory.path(), ProfileId("work".into())).unwrap();
        assert_eq!(
            InstalledSettings::load(directory.path()).unwrap(),
            Some(InstalledSettings::new(ProfileId("work".into())).unwrap()),
            "reconfiguration replaces the prior selection on every platform"
        );

        let path = distillery_settings_path(directory.path());
        let json = fs::read_to_string(&path).unwrap();
        assert!(json.contains("research"));
        assert!(!json.contains("seed"));
        fs::write(&path, r#"{"profile":"research","tick_every_ms":5}"#).unwrap();
        assert!(InstalledSettings::load(directory.path()).is_err());
    }

    #[test]
    fn configured_profile_reopens_stably_and_names_private_mesh_paths() {
        let directory = tempfile::tempdir().unwrap();
        let vault_dir = directory.path().join("vault");
        let profile = ProfileId("research".into());
        provision_profile(&vault_dir, &profile);
        InstalledAuthority::configure(directory.path(), profile.clone()).unwrap();

        let first = InstalledAuthority::open_with(directory.path(), &vault_dir, unlock()).unwrap();
        let first_author = first.mesh_author().unwrap().public_key().to_bytes();
        let first_transport = first.transport_identity().public_key().to_bytes();
        assert_eq!(first.profile(), profile);
        assert!(first.protection().contains("passphrase-encrypted"));
        assert_ne!(
            first_author, first_transport,
            "mesh author is profile-derived"
        );
        let paths = first.paths(MESH);
        assert_eq!(paths.mesh_store_path(), paths.root().join("mesh.redb"));
        assert_eq!(paths.blob_store_root(), paths.root().join("blobs"));
        drop(first);

        let reopened =
            InstalledAuthority::open_with(directory.path(), &vault_dir, unlock()).unwrap();
        assert_eq!(
            reopened.mesh_author().unwrap().public_key().to_bytes(),
            first_author,
            "the persisted profile, not an environment seed, owns the mesh author"
        );
    }

    #[tokio::test]
    async fn binding_requires_mesh_and_device_facts_from_the_caller() {
        let directory = tempfile::tempdir().unwrap();
        let vault_dir = directory.path().join("vault");
        let profile = ProfileId("research".into());
        provision_profile(&vault_dir, &profile);
        InstalledAuthority::configure(directory.path(), profile).unwrap();
        let authority =
            InstalledAuthority::open_with(directory.path(), &vault_dir, unlock()).unwrap();
        let author = authority.mesh_author().unwrap();
        let policy = MeshRetentionPolicy {
            revision: PolicyRevision([0x41; 32]),
            checkpoint_authority: author.public_key().to_bytes(),
            availability: AvailabilityPolicy {
                promised_floor: KeepBound::Forever,
            },
            erasure: ErasurePolicy {
                privacy_ceiling: KeepBound::UntilCheckpoint,
                terminal_job_payload: mesh::PayloadRule::EraseTerminalAtCheckpoint,
            },
            lease: LeasePolicy { max_skew_ms: 0 },
        };
        let store =
            MeshStore::at_path_with_retention(authority.paths(MESH).mesh_store_path(), policy)
                .unwrap();
        let settings = ResidentSettings {
            tick_every: Duration::from_secs(1),
            maintenance_every: None,
            blob_gc_every: Duration::from_secs(1),
            retention: RetentionSettings::default(),
        };

        let resident = authority
            .bind_resident(MESH, store, settings, |space| HostConfig::supervised(space))
            .await
            .unwrap();
        assert_eq!(
            resident.authority().host().me(),
            author.public_key().to_bytes()
        );
        resident.shutdown().await.unwrap();
    }
}
