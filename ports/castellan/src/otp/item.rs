// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Persona-scoped, sealed one-time-password items.
//!
//! An [`OtpItemStore`] is the narrow bridge between Personae's sealed-record
//! substrate and the OTP core. It accepts an `otpauth://` URI, stores the
//! configuration and seed in one sealed record, and can exercise that record
//! to produce a code. Its read model contains only display metadata; the seed
//! is neither a field nor an accessor on any public type here.
//!
//! This is the storage seam below [`super::OtpReleaseGate`], which is the only
//! public path that can turn a sealed record into a code-bearing tile.

use std::fmt;

use personae::{IdentityError, PersonaId, SealedRecordChange, SealedRecordStorage};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use super::steam_guard::decode_shared_secret;
use super::uri::OtpUri;
use super::{
    Otp, OtpAlgorithm, OtpCodeStyle, OtpCodeTile, OtpError, OtpKind, OtpUriError, SteamGuard,
    SteamGuardError, parse_otpauth_uri,
};

const LEGACY_RECORD_FORMAT_VERSION: u8 = 1;
const RECORD_FORMAT_VERSION: u8 = 2;
const RECORD_DIRECTORY: &str = "castellan/otp/v1";

/// Stable handle for one sealed OTP item.
///
/// The handle is not a secret. It is unique only within the persona-scoped
/// store that minted it; the record path also includes the owning persona.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OtpItemId(uuid::Uuid);

impl OtpItemId {
    fn mint() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    /// Reconstitute an item handle saved by a caller alongside its own view state.
    pub fn from_uuid(uuid: uuid::Uuid) -> Self {
        Self(uuid)
    }

    /// Return the UUID form suitable for a caller's own durable reference.
    pub fn as_uuid(self) -> uuid::Uuid {
        self.0
    }
}

impl fmt::Display for OtpItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Secret-free metadata for one stored OTP item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OtpItem {
    /// Stable handle used to exercise or delete this item.
    pub id: OtpItemId,
    /// Account name supplied by the provisioning URI.
    pub account: String,
    /// Issuing service supplied by the provisioning URI, when present.
    pub issuer: Option<String>,
    /// HMAC hash behind the generated code.
    pub algorithm: OtpAlgorithm,
    /// Character vocabulary and width used to present each code.
    pub code_style: OtpCodeStyle,
    /// Whether this item is time- or counter-based.
    pub kind: OtpKind,
}

/// Failure while importing, loading, or exercising an OTP item.
#[derive(Debug)]
pub enum OtpItemError {
    /// Personae could not read, write, or remove the sealed record.
    Storage(IdentityError),
    /// The supplied provisioning URI was malformed or unsupported.
    Import(OtpUriError),
    /// Steam Guard compatibility material was malformed.
    SteamGuard(SteamGuardError),
    /// A stored generator could not produce a code.
    Generation(OtpError),
    /// The caller asked for an item absent from this persona's store.
    NotFound(OtpItemId),
    /// The sealed record was written by an unsupported item format.
    UnsupportedRecordVersion(u8),
    /// The final HOTP value cannot be released because it could not be
    /// advanced before the replacement record could be persisted.
    HotpCounterExhausted,
}

impl fmt::Display for OtpItemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OtpItemError::Storage(error) => write!(f, "sealed OTP storage: {error}"),
            OtpItemError::Import(error) => write!(f, "OTP import: {error}"),
            OtpItemError::SteamGuard(error) => write!(f, "Steam Guard import: {error}"),
            OtpItemError::Generation(error) => write!(f, "OTP generation: {error}"),
            OtpItemError::NotFound(id) => write!(f, "no OTP item {id} for this persona"),
            OtpItemError::UnsupportedRecordVersion(version) => {
                write!(f, "unsupported sealed OTP item version {version}")
            }
            OtpItemError::HotpCounterExhausted => {
                f.write_str("the HOTP counter has no next durable value")
            }
        }
    }
}

impl std::error::Error for OtpItemError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            OtpItemError::Storage(error) => Some(error),
            OtpItemError::Import(error) => Some(error),
            OtpItemError::SteamGuard(error) => Some(error),
            OtpItemError::Generation(error) => Some(error),
            OtpItemError::NotFound(_)
            | OtpItemError::UnsupportedRecordVersion(_)
            | OtpItemError::HotpCounterExhausted => None,
        }
    }
}

impl From<IdentityError> for OtpItemError {
    fn from(error: IdentityError) -> Self {
        Self::Storage(error)
    }
}

impl From<SteamGuardError> for OtpItemError {
    fn from(error: SteamGuardError) -> Self {
        Self::SteamGuard(error)
    }
}

/// Sealed OTP items held for one persona.
///
/// The storage key is supplied by the already-unlocked Personae layer. This
/// type binds the key's record namespace to `persona`, rather than deriving a
/// new key or treating a display name as an identity boundary.
#[derive(Clone)]
pub struct OtpItemStore {
    storage: SealedRecordStorage,
    persona: PersonaId,
}

impl OtpItemStore {
    /// Open the OTP item namespace for one persona over an unlocked record store.
    pub fn new(storage: SealedRecordStorage, persona: PersonaId) -> Self {
        Self { storage, persona }
    }

    /// The persona whose namespace this store serves.
    pub fn persona(&self) -> PersonaId {
        self.persona
    }

    /// Parse and seal a provisioning URI, returning secret-free item metadata.
    pub fn import_otpauth_uri(&self, uri: &str) -> Result<OtpItem, OtpItemError> {
        let (otp, imported) = parse_otpauth_uri(uri).map_err(OtpItemError::Import)?;
        let id = OtpItemId::mint();
        let record = StoredOtpItem::from_import(otp, imported)?;
        let item = record.item(id);
        self.storage.save_record(self.record_path(id), &record)?;
        Ok(item)
    }

    /// Seal one Valve Steam Guard mobile authenticator `shared_secret`.
    ///
    /// The base64 value comes from a Steam authenticator file. It is not an
    /// `otpauth://` extension and is always presented as a five-character
    /// Steam Guard code under the fixed `Steam` issuer.
    pub fn import_steam_guard(
        &self,
        account: &str,
        shared_secret: &str,
    ) -> Result<OtpItem, OtpItemError> {
        if account.is_empty()
            || account != account.trim()
            || account.len() > 256
            || account.chars().any(char::is_control)
        {
            return Err(SteamGuardError::InvalidAccount.into());
        }
        let id = OtpItemId::mint();
        let record = StoredOtpItem::from_steam_guard(account, shared_secret)?;
        let item = record.item(id);
        self.storage.save_record(self.record_path(id), &record)?;
        Ok(item)
    }

    /// Read one item's secret-free metadata, or `None` when it is absent.
    pub fn get(&self, id: OtpItemId) -> Result<Option<OtpItem>, OtpItemError> {
        Ok(self.load(id)?.map(|record| record.item(id)))
    }

    /// Exercise one item after a participant-gated approval.
    ///
    /// TOTP records are unchanged. HOTP advances its counter before returning
    /// a code. The sealed-store update serializes load, exercise, and replace
    /// across every clone of the opened store, including independent gates.
    pub(crate) fn release_tile_at_unix_time(
        &self,
        id: OtpItemId,
        unix_secs: u64,
    ) -> Result<OtpCodeTile, OtpItemError> {
        self.storage
            .update_record(self.record_path(id), |record: Option<StoredOtpItem>| {
                let mut record = record.ok_or(OtpItemError::NotFound(id))?.checked()?;
                if matches!(record.kind, StoredOtpKind::Hotp { counter: u64::MAX }) {
                    return Err(OtpItemError::HotpCounterExhausted);
                }
                let item = record.item(id);
                let code = record.code_at_unix_time(unix_secs)?;
                let change = if let StoredOtpKind::Hotp { counter } = &mut record.kind {
                    *counter = counter
                        .checked_add(1)
                        .expect("HOTP counter was checked before the code was produced");
                    SealedRecordChange::Replace(record)
                } else {
                    SealedRecordChange::Keep
                };
                Ok((OtpCodeTile::new(item, code, unix_secs), change))
            })
    }

    /// Return seconds before a time-based item rolls over, or `None` for HOTP.
    pub fn seconds_remaining_at(
        &self,
        id: OtpItemId,
        unix_secs: u64,
    ) -> Result<Option<u64>, OtpItemError> {
        self.require(id)?.seconds_remaining_at(unix_secs)
    }

    /// Remove one item from this persona's sealed namespace.
    ///
    /// Deleting an absent item succeeds, matching Personae's record-store
    /// deletion semantics.
    pub fn delete(&self, id: OtpItemId) -> Result<(), OtpItemError> {
        self.storage.delete_record(self.record_path(id))?;
        Ok(())
    }

    fn require(&self, id: OtpItemId) -> Result<StoredOtpItem, OtpItemError> {
        self.load(id)?.ok_or(OtpItemError::NotFound(id))
    }

    fn load(&self, id: OtpItemId) -> Result<Option<StoredOtpItem>, OtpItemError> {
        let record = self
            .storage
            .load_record(self.record_path(id))?
            .map(StoredOtpItem::checked)
            .transpose()?;
        Ok(record)
    }

    fn record_path(&self, id: OtpItemId) -> String {
        format!("{RECORD_DIRECTORY}/{}/{}.json", self.persona.as_uuid(), id)
    }
}

#[derive(Serialize, Deserialize)]
struct StoredOtpItem {
    version: u8,
    account: String,
    issuer: Option<String>,
    secret: Vec<u8>,
    algorithm: StoredOtpAlgorithm,
    digits: u32,
    #[serde(default)]
    code_style: StoredOtpCodeStyle,
    kind: StoredOtpKind,
}

impl Drop for StoredOtpItem {
    fn drop(&mut self) {
        self.secret.zeroize();
    }
}

impl StoredOtpItem {
    fn from_import(otp: Otp, imported: OtpUri) -> Result<Self, OtpItemError> {
        Ok(Self {
            version: RECORD_FORMAT_VERSION,
            account: imported.account,
            issuer: imported.issuer,
            secret: otp.secret.to_vec(),
            algorithm: otp.algorithm.into(),
            digits: otp.digits,
            code_style: StoredOtpCodeStyle::Decimal,
            kind: otp.kind.into(),
        })
    }

    fn from_steam_guard(account: &str, shared_secret: &str) -> Result<Self, OtpItemError> {
        let secret = decode_shared_secret(shared_secret)?;
        Ok(Self {
            version: RECORD_FORMAT_VERSION,
            account: account.to_string(),
            issuer: Some("Steam".to_string()),
            secret: secret.as_slice().to_vec(),
            algorithm: StoredOtpAlgorithm::Sha1,
            digits: 5,
            code_style: StoredOtpCodeStyle::SteamGuard,
            kind: StoredOtpKind::Totp { period: 30 },
        })
    }

    fn checked(self) -> Result<Self, OtpItemError> {
        if matches!(
            self.version,
            LEGACY_RECORD_FORMAT_VERSION | RECORD_FORMAT_VERSION
        ) {
            Ok(self)
        } else {
            Err(OtpItemError::UnsupportedRecordVersion(self.version))
        }
    }

    fn item(&self, id: OtpItemId) -> OtpItem {
        OtpItem {
            id,
            account: self.account.clone(),
            issuer: self.issuer.clone(),
            algorithm: self.algorithm.into(),
            code_style: match self.code_style {
                StoredOtpCodeStyle::Decimal => OtpCodeStyle::Decimal {
                    digits: self.digits,
                },
                StoredOtpCodeStyle::SteamGuard => OtpCodeStyle::SteamGuard,
            },
            kind: self.kind.into(),
        }
    }

    fn code_at_unix_time(&self, unix_secs: u64) -> Result<String, OtpItemError> {
        match self.code_style {
            StoredOtpCodeStyle::Decimal => self
                .otp()?
                .code_at_unix_time(unix_secs)
                .map_err(OtpItemError::Generation),
            StoredOtpCodeStyle::SteamGuard => {
                Ok(SteamGuard::from_secret_bytes(&self.secret)?.code_at_unix_time(unix_secs))
            }
        }
    }

    fn seconds_remaining_at(&self, unix_secs: u64) -> Result<Option<u64>, OtpItemError> {
        match self.code_style {
            StoredOtpCodeStyle::Decimal => Ok(self.otp()?.seconds_remaining_at(unix_secs)),
            StoredOtpCodeStyle::SteamGuard => Ok(Some(30 - (unix_secs % 30))),
        }
    }

    fn otp(&self) -> Result<Otp, OtpItemError> {
        let otp = match self.kind {
            StoredOtpKind::Totp { period } => Otp::totp(self.secret.clone())
                .map_err(OtpItemError::Generation)?
                .with_period(period)
                .map_err(OtpItemError::Generation)?,
            StoredOtpKind::Hotp { counter } => {
                Otp::hotp(self.secret.clone(), counter).map_err(OtpItemError::Generation)?
            }
        };
        otp.with_digits(self.digits)
            .map_err(OtpItemError::Generation)
            .map(|otp| otp.with_algorithm(self.algorithm.into()))
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
enum StoredOtpAlgorithm {
    Sha1,
    Sha256,
    Sha512,
}

#[derive(Clone, Copy, Default, Serialize, Deserialize)]
enum StoredOtpCodeStyle {
    #[default]
    Decimal,
    SteamGuard,
}

impl From<OtpAlgorithm> for StoredOtpAlgorithm {
    fn from(algorithm: OtpAlgorithm) -> Self {
        match algorithm {
            OtpAlgorithm::Sha1 => Self::Sha1,
            OtpAlgorithm::Sha256 => Self::Sha256,
            OtpAlgorithm::Sha512 => Self::Sha512,
        }
    }
}

impl From<StoredOtpAlgorithm> for OtpAlgorithm {
    fn from(algorithm: StoredOtpAlgorithm) -> Self {
        match algorithm {
            StoredOtpAlgorithm::Sha1 => Self::Sha1,
            StoredOtpAlgorithm::Sha256 => Self::Sha256,
            StoredOtpAlgorithm::Sha512 => Self::Sha512,
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
enum StoredOtpKind {
    Totp { period: u64 },
    Hotp { counter: u64 },
}

impl From<OtpKind> for StoredOtpKind {
    fn from(kind: OtpKind) -> Self {
        match kind {
            OtpKind::Totp { period, .. } => Self::Totp { period },
            OtpKind::Hotp { counter } => Self::Hotp { counter },
        }
    }
}

impl From<StoredOtpKind> for OtpKind {
    fn from(kind: StoredOtpKind) -> Self {
        match kind {
            StoredOtpKind::Totp { period } => Self::Totp { period, t0: 0 },
            StoredOtpKind::Hotp { counter } => Self::Hotp { counter },
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    const RFC6238_SHA1_SECRET_BASE32: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
    const STEAM_SHARED_SECRET: &str = "zvIayp3JPvtvX/QGHqsqKBk/44s=";

    fn store(root: &std::path::Path, persona: PersonaId) -> OtpItemStore {
        OtpItemStore::new(
            SealedRecordStorage::open_with_key(root, [0x71; 32]),
            persona,
        )
    }

    fn provision_uri() -> String {
        format!(
            "otpauth://totp/Merely:mark?secret={RFC6238_SHA1_SECRET_BASE32}&issuer=Merely&digits=8"
        )
    }

    #[test]
    fn imported_item_reopens_with_its_secret_free_metadata() {
        let dir = tempdir().unwrap();
        let persona = PersonaId::new();
        let items = store(dir.path(), persona);
        let item = items.import_otpauth_uri(&provision_uri()).unwrap();

        assert_eq!(item.account, "mark");
        assert_eq!(item.issuer.as_deref(), Some("Merely"));
        assert_eq!(item.code_style, OtpCodeStyle::Decimal { digits: 8 });

        let reopened = store(dir.path(), persona);
        assert_eq!(reopened.get(item.id).unwrap(), Some(item.clone()));
        assert_eq!(reopened.seconds_remaining_at(item.id, 59).unwrap(), Some(1));
    }

    #[test]
    fn stored_record_contains_neither_seed_nor_display_metadata_in_plaintext() {
        let dir = tempdir().unwrap();
        let items = store(dir.path(), PersonaId::new());
        let item = items.import_otpauth_uri(&provision_uri()).unwrap();

        let bytes = std::fs::read(dir.path().join(items.record_path(item.id))).unwrap();
        let disk = String::from_utf8(bytes).unwrap();
        assert!(!disk.contains(RFC6238_SHA1_SECRET_BASE32));
        assert!(!disk.contains("Merely"));
        assert!(!disk.contains("\"account\":\"mark\""));
    }

    #[test]
    fn an_item_handle_has_no_meaning_in_another_persona_namespace() {
        let dir = tempdir().unwrap();
        let owner = store(dir.path(), PersonaId::new());
        let item = owner.import_otpauth_uri(&provision_uri()).unwrap();
        let other = store(dir.path(), PersonaId::new());

        assert_eq!(other.get(item.id).unwrap(), None);
        assert!(matches!(
            other.seconds_remaining_at(item.id, 59),
            Err(OtpItemError::NotFound(id)) if id == item.id
        ));
    }

    #[test]
    fn deletion_closes_the_item_without_affecting_the_persona_namespace() {
        let dir = tempdir().unwrap();
        let items = store(dir.path(), PersonaId::new());
        let item = items.import_otpauth_uri(&provision_uri()).unwrap();

        items.delete(item.id).unwrap();

        assert_eq!(items.get(item.id).unwrap(), None);
    }

    #[test]
    fn steam_guard_is_an_explicit_stored_style_and_uses_the_release_tile() {
        let dir = tempdir().unwrap();
        let items = store(dir.path(), PersonaId::new());
        let item = items
            .import_steam_guard("mark", STEAM_SHARED_SECRET)
            .unwrap();

        assert_eq!(item.issuer.as_deref(), Some("Steam"));
        assert_eq!(item.code_style, OtpCodeStyle::SteamGuard);
        assert_eq!(item.code_style.character_count(), 5);
        assert_eq!(item.kind, OtpKind::Totp { period: 30, t0: 0 });
        let tile = items
            .release_tile_at_unix_time(item.id, 1_616_374_841)
            .unwrap();
        assert_eq!(tile.code_at_unix_time(1_616_374_841), Some("2F9J5"));
        assert_eq!(tile.code_at_unix_time(1_616_374_860), None);
    }

    #[test]
    fn steam_guard_import_does_not_reclassify_otpauth_extensions() {
        let dir = tempdir().unwrap();
        let items = store(dir.path(), PersonaId::new());
        let item = items
            .import_otpauth_uri(&format!(
                "otpauth://totp/Steam:mark?secret={RFC6238_SHA1_SECRET_BASE32}&issuer=Steam&encoder=steam"
            ))
            .unwrap();

        assert_eq!(item.code_style, OtpCodeStyle::Decimal { digits: 6 });
    }
}
