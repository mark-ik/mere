//! The roster-driven passes: commissioning publish and revocation retraction.
//!
//! Two halves of one lifecycle, sharing the retraction index that links them:
//! publish writes it while the wrapping key still exists, retract consumes it
//! after revocation has deleted that key.
//!
//! The leases design ruled that "a peer that receives a revocation statement
//! destroys the certificate's leases immediately". Blinding forecloses that
//! mechanism as written: a statement names a `DelegationId`, a replica holds
//! only blinded slots, and mapping one to the other needs the device's
//! wrapping key, which a replica never has and which the wallet itself
//! deletes during revocation (`remove_remote_auth_wrapping_key`). No peer
//! can act on the statement directly, and nothing should be unblinded to
//! make it able to.
//!
//! What the ratified grammar already provides is stronger: **supersession
//! destroys**. The issuer publishes an empty record over the slot, and every
//! cooperative peer's admission prunes the superseded operation and erases
//! its payload in the same backend batch. The material is gone the moment
//! the retraction syncs, the empty shell purges at its own short expiry, and
//! a peer the retraction never reaches converges at the original lease's
//! expiry exactly as ruling 5 always guaranteed.
//!
//! The wallet must therefore remember which slots it published for a device
//! *before* revocation deletes the wrapping key. That is the retraction
//! index: a local muniment slot beside the carriage store, written at
//! publish time. Wallet-side only, which is fine, because the wallet is the
//! party that already knows every association the blinding hides from peers.

use muniment::JsonSlots;
use pandect::{DeviceId, PersonaId, WrappedEpochRecord, encode_epoch_record};
use personae::carry::persona_wallet_salt;
use personae::{IdentityProvider, InMemoryProvider};
use serde::{Deserialize, Serialize};

use super::{CarriageCeilings, CarriageHost, CarriageHostError, HeldLease, now_ms};

/// How long a retraction shell stays admissible. Short on purpose: its whole
/// job is done the moment the prune lands, and the shell itself carries
/// nothing. Long enough to survive clock skew between cooperative peers.
pub const RETRACTION_TTL_MS: u64 = 10 * 60 * 1000;

/// One published slot the wallet may later need to retract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetractionTarget {
    /// The blinded slot, exactly as published.
    pub slot: [u8; 32],
    /// The persona whose chain root signs the retraction.
    pub persona: PersonaId,
    /// The certificate the slot serves, for the empty record's binding.
    pub certificate: [u8; 32],
}

/// What one retraction pass did, and what it honestly could not.
#[derive(Debug, Default)]
pub struct CarriageRetractReport {
    /// Slots superseded with an empty record.
    pub retracted: Vec<[u8; 32]>,
    /// Indexed slots this host no longer holds a version of, so it cannot
    /// name an issue that supersedes. Expiry remains their backstop.
    pub skipped_not_held: Vec<[u8; 32]>,
}

/// What one commissioning pass published, and what it honestly could not.
#[derive(Debug, Default)]
pub struct CarriagePublishReport {
    /// Each (device, persona) whose slot went onto the lane.
    pub published: Vec<(pandect::DeviceId, pandect::PersonaId)>,
    /// Leased devices whose wrapping key the wallet never retained, so their
    /// slots cannot be addressed. Pairing retains one; direct issue does not.
    pub skipped_no_wrapping_key: Vec<pandect::DeviceId>,
    /// Certificates obliged to carry epoch material that have no record yet.
    pub skipped_no_record: usize,
    /// Certificates whose grant expiry has already passed.
    pub skipped_grant_expired: usize,
}

fn retraction_index_key(device: DeviceId) -> String {
    format!("carriage-retraction-index/{}", device.as_uuid())
}

impl CarriageHost {
    /// Publish every slot this wallet's roster says should ride this lane.
    ///
    /// The issue-path integration: after grants are issued or refreshed
    /// through `pandect`, the wallet host calls this to put each leased
    /// device's wrapped-epoch records on the carriage topic. The roster is
    /// the authority for *whether* (the ruled layering: wallet roster grants
    /// carriage, pairing list routes it); this reads `CarriagePolicy` off
    /// each `DeviceRecord` and publishes only for devices leased onto this
    /// host's graph.
    ///
    /// Skips are reported rather than erred, because each is a normal state:
    /// a device with no retained wrapping key cannot have its slot addressed
    /// (the direct issue path retains none; pairing does), a certificate with
    /// no epoch record has nothing to carry, and an already-expired grant has
    /// nothing left to lease.
    pub async fn publish_grant_carriage(
        &self,
        data_root: &std::path::Path,
    ) -> Result<CarriagePublishReport, CarriageHostError> {
        let seed = pandect::load_identity_seed(data_root)
            .map_err(|error| CarriageHostError::Transport(error.to_string()))?
            .ok_or_else(|| {
                CarriageHostError::Refused("wallet root missing identity seed".into())
            })?;
        let provider = personae::InMemoryProvider::from_seed(seed);
        let roster = pandect::load_device_roster(data_root)
            .map_err(|error| CarriageHostError::Transport(error.to_string()))?
            .unwrap_or_else(pandect::DeviceRoster::new);
        let bridge = pandect::load_remote_auth_wrapping_key_bridge(data_root)
            .map_err(|error| CarriageHostError::Transport(error.to_string()))?
            .unwrap_or_default();

        let mut report = CarriagePublishReport::default();
        for device in &roster.devices {
            if roster.revoked.contains(&device.device_id) {
                continue;
            }
            let pandect::CarriagePolicy::Leased { max_ttl_ms, graph } = device.carriage else {
                continue;
            };
            if graph != self.graph {
                continue;
            }
            let Some(wrapping_key) = bridge
                .keys
                .iter()
                .find(|key| key.device_id == device.device_id)
                .map(|key| key.wrapping_key)
            else {
                report.skipped_no_wrapping_key.push(device.device_id);
                continue;
            };
            let set = pandect::load_device_grant_set(data_root, device.device_id)
                .map_err(|error| CarriageHostError::Transport(error.to_string()))?;
            let mut retraction_targets = Vec::new();
            for (persona, certificate) in &set.personas {
                if !pandect::requires_epoch_material(certificate) {
                    continue;
                }
                let certificate_id = certificate.certificate.id();
                let Some(record) = pandect::load_wrapped_epoch_record(data_root, certificate_id)
                    .map_err(|error| CarriageHostError::Transport(error.to_string()))?
                else {
                    report.skipped_no_record += 1;
                    continue;
                };
                let bytes = pandect::encode_epoch_record(&record)
                    .map_err(|error| CarriageHostError::Refused(error.to_string()))?;

                let now = now_ms();
                let mut expires_at_ms = now.saturating_add(max_ttl_ms);
                if let Some(grant_expiry) = certificate.certificate.expires_at_ms {
                    expires_at_ms = expires_at_ms.min(grant_expiry);
                }
                if expires_at_ms <= now {
                    report.skipped_grant_expired += 1;
                    continue;
                }

                let slot = pandect::blinded_slot_id(certificate_id, wrapping_key);
                let issue = self
                    .held
                    .read()
                    .await
                    .get(&slot)
                    .map(|lease| lease.issue + 1)
                    .unwrap_or(1);
                // The persona's chain-root keypair: the same authority whose
                // public key verifiers hold as this persona's TrustedRoot.
                let issuer = provider
                    .derive_keypair(&personae::carry::persona_wallet_salt(*persona))
                    .map_err(CarriageHostError::Identity)?;
                self.publish_slot(
                    &issuer,
                    slot,
                    issue,
                    expires_at_ms,
                    bytes,
                    CarriageCeilings {
                        device_max_ttl_ms: Some(max_ttl_ms),
                        grant_expires_at_ms: certificate.certificate.expires_at_ms,
                    },
                )
                .await?;
                report.published.push((device.device_id, *persona));
                retraction_targets.push(RetractionTarget {
                    slot: slot.0,
                    persona: *persona,
                    certificate: certificate_id.0,
                });
            }
            // Written while the wrapping key still exists, which is the whole
            // point: revocation deletes that key, and this index is what lets
            // the fast path address the slots afterwards.
            self.index_retraction_targets(device.device_id, retraction_targets)
                .await?;
        }
        Ok(report)
    }

    fn retraction_index(&self) -> JsonSlots<muniment::RedbBackend> {
        JsonSlots::new(self.store.backend().clone())
    }

    /// Record what was just published for a device, so revocation can still
    /// address its slots after the wrapping key is gone. Union by slot, so
    /// repeated publish passes stay idempotent.
    pub(super) async fn index_retraction_targets(
        &self,
        device: DeviceId,
        targets: Vec<RetractionTarget>,
    ) -> Result<(), CarriageHostError> {
        if targets.is_empty() {
            return Ok(());
        }
        let index = self.retraction_index();
        let key = retraction_index_key(device);
        let mut merged: Vec<RetractionTarget> = index.load(&key).await?.unwrap_or_default();
        for target in targets {
            if !merged.iter().any(|existing| existing.slot == target.slot) {
                merged.push(target);
            }
        }
        index.save(&key, &merged).await?;
        Ok(())
    }

    /// Destroy a revoked device's carriage on every cooperative peer, now
    /// rather than at expiry.
    ///
    /// Call after `pandect::revoke_remote_auth_device`; the index makes the
    /// ordering safe, since it was written when the wrapping key still
    /// existed. Each indexed slot is superseded by an empty record under a
    /// short lease, which the grammar's own prune turns into destruction:
    /// the superseded payload is erased in the same batch that admits the
    /// shell. A peer this never reaches still converges at the original
    /// expiry, which is the dependency posture ruling 5 chose.
    pub async fn retract_device_carriage(
        &self,
        device: DeviceId,
        master_seed: [u8; 32],
    ) -> Result<CarriageRetractReport, CarriageHostError> {
        let index = self.retraction_index();
        let key = retraction_index_key(device);
        let targets: Vec<RetractionTarget> = index.load(&key).await?.unwrap_or_default();
        let provider = InMemoryProvider::from_seed(master_seed);

        let mut report = CarriageRetractReport::default();
        for target in &targets {
            let slot = pandect::BlindedSlotId(target.slot);
            let Some(issue) = self
                .held
                .read()
                .await
                .get(&slot)
                .map(|lease| lease.issue + 1)
            else {
                report.skipped_not_held.push(target.slot);
                continue;
            };
            let shell =
                WrappedEpochRecord::new(personae::delegation::DelegationId(target.certificate));
            let bytes = encode_epoch_record(&shell)
                .map_err(|error| CarriageHostError::Refused(error.to_string()))?;
            let issuer = provider
                .derive_keypair(&persona_wallet_salt(target.persona))
                .map_err(CarriageHostError::Identity)?;
            self.publish_slot(
                &issuer,
                slot,
                issue,
                now_ms() + RETRACTION_TTL_MS,
                bytes,
                CarriageCeilings::default(),
            )
            .await?;
            report.retracted.push(target.slot);
        }
        index.delete(&key).await?;
        Ok(report)
    }
}
