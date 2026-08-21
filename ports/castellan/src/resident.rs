// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The process-wide owner of Castellan's sealed credential records.
//!
//! A desktop host keeps one resident alive and gives applications admitted
//! views or protocol adapters over it. Sandboxed applications may embed the
//! same type. Either shape retains one OS file lock and one external freshness
//! ledger for the lifetime of every clone.

use std::path::PathBuf;

#[cfg(feature = "secret-service")]
use std::collections::BTreeMap;
#[cfg(feature = "secret-service")]
use std::sync::{Arc, Mutex};

use personae::{IdentityError, PersonaId, SealedRecordStorage};

use crate::otp::OtpItemStore;

/// Exclusive authority over one Castellan credential-record directory.
#[derive(Clone)]
pub struct CastellanResident {
    records: SealedRecordStorage,
    #[cfg(feature = "secret-service")]
    secret_service_transactions: Arc<Mutex<BTreeMap<PersonaId, Arc<Mutex<()>>>>>,
}

impl CastellanResident {
    /// Claim the record directory and its separately rooted freshness ledger.
    ///
    /// `record_key` seals credential contents. `freshness_key` authenticates
    /// the external generation ledger and should be independently derived by
    /// the already-unlocked Personae authority. A second claim against the
    /// same record directory fails immediately.
    pub fn claim(
        records_root: impl Into<PathBuf>,
        record_key: [u8; 32],
        freshness_root: impl Into<PathBuf>,
        freshness_key: [u8; 32],
    ) -> Result<Self, IdentityError> {
        Ok(Self {
            records: SealedRecordStorage::claim_with_file_freshness(
                records_root,
                record_key,
                freshness_root,
                freshness_key,
            )?,
            #[cfg(feature = "secret-service")]
            secret_service_transactions: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }

    /// Open the persona-scoped OTP namespace under this authority.
    pub fn otp_items(&self, persona: PersonaId) -> OtpItemStore {
        OtpItemStore::new(self.records.clone(), persona)
    }

    /// Open the persona-scoped Freedesktop Secret Service store.
    #[cfg(feature = "secret-service")]
    pub fn secret_service(
        &self,
        persona: PersonaId,
        limits: crate::secret_service::SecretServiceLimits,
    ) -> crate::secret_service::SecretServiceStore {
        let transaction = self
            .secret_service_transactions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(persona)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        crate::secret_service::SecretServiceStore::new(
            self.records.clone(),
            persona,
            limits,
            transaction,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::otp::{OtpReleaseGate, OtpReleaseParticipantClaim};
    use tempfile::tempdir;

    const RFC4226_SECRET_BASE32: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    fn claim(root: &std::path::Path) -> CastellanResident {
        CastellanResident::claim(
            root.join("records"),
            [0x81; 32],
            root.join("freshness"),
            [0x82; 32],
        )
        .unwrap()
    }

    #[test]
    fn independent_gates_under_one_resident_cannot_repeat_an_hotp_counter() {
        let dir = tempdir().unwrap();
        let resident = claim(dir.path());
        let persona = PersonaId::new();
        let items = resident.otp_items(persona);
        let item = items
            .import_otpauth_uri(&format!(
                "otpauth://hotp/Steam:mark?secret={RFC4226_SECRET_BASE32}&issuer=Steam&counter=0"
            ))
            .unwrap();
        let left = OtpReleaseGate::new(resident.otp_items(persona));
        let right = OtpReleaseGate::new(resident.otp_items(persona));
        let participant =
            || OtpReleaseParticipantClaim::unverified("local:test", "resident:test").unwrap();

        let first = left.petition(item.id, participant()).unwrap();
        let first = left.approve(first.id).unwrap();
        let second = right.petition(item.id, participant()).unwrap();
        let second = right.approve(second.id).unwrap();

        assert_eq!(first.tile().code_at_unix_time(0), Some("755224"));
        assert_eq!(second.tile().code_at_unix_time(0), Some("287082"));
    }

    #[test]
    fn resident_rejects_restored_hotp_state_before_releasing_it_again() {
        let dir = tempdir().unwrap();
        let records = dir.path().join("records");
        let resident = claim(dir.path());
        let persona = PersonaId::new();
        let items = resident.otp_items(persona);
        let item = items
            .import_otpauth_uri(&format!(
                "otpauth://hotp/Steam:mark?secret={RFC4226_SECRET_BASE32}&issuer=Steam&counter=0"
            ))
            .unwrap();
        let record = records
            .join("castellan/otp/v1")
            .join(persona.as_uuid().to_string())
            .join(format!("{}.json", item.id));
        let counter_zero = std::fs::read(&record).unwrap();
        let gate = OtpReleaseGate::new(items);
        let participant =
            || OtpReleaseParticipantClaim::unverified("local:test", "rollback:test").unwrap();

        let first = gate.petition(item.id, participant()).unwrap();
        assert_eq!(
            gate.approve(first.id).unwrap().tile().code_at_unix_time(0),
            Some("755224")
        );
        let second = gate.petition(item.id, participant()).unwrap();
        std::fs::write(&record, counter_zero).unwrap();

        let error = gate.approve(second.id).unwrap_err();
        assert!(error.to_string().contains("rollback detected"));
    }

    #[cfg(feature = "secret-service")]
    #[test]
    fn independent_secret_service_views_share_one_composite_transaction() {
        use crate::secret_service::{NewSecretItem, SecretServiceLimits};

        let dir = tempdir().unwrap();
        let resident = claim(dir.path());
        let persona = PersonaId::new();
        let left = resident.secret_service(persona, SecretServiceLimits::default());
        let right = resident.secret_service(persona, SecretServiceLimits::default());
        let collection = left.ensure_default_collection("Castellan", 1).unwrap();
        let create = |store: crate::secret_service::SecretServiceStore, prefix: &'static str| {
            std::thread::spawn(move || {
                for index in 0..20 {
                    store
                        .create_item(NewSecretItem {
                            collection: collection.id,
                            label: format!("{prefix} {index}"),
                            attributes: std::collections::BTreeMap::from([(
                                "id".to_string(),
                                format!("{prefix}-{index}"),
                            )]),
                            secret: vec![index],
                            content_type: "application/octet-stream".into(),
                            replace: false,
                            unix_secs: index.into(),
                        })
                        .unwrap();
                }
            })
        };
        let left = create(left, "left");
        let right = create(right, "right");
        left.join().unwrap();
        right.join().unwrap();

        assert_eq!(
            resident
                .secret_service(persona, SecretServiceLimits::default())
                .items(collection.id)
                .unwrap()
                .len(),
            40
        );
    }
}
