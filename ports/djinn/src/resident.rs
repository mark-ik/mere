// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Product-neutral ownership assembled by the desktop resident.
//!
//! This module owns process-lifetime resources without importing a product UI.
//! Graphshell supplies admitted local sessions; Knot supplies document and sync
//! semantics; Castellan supplies credential records; Djinn keeps their
//! lifetimes and local storage topology coherent.

use std::path::{Path, PathBuf};

use castellan::resident::CastellanResident;
use graphshell::native::endpoint_catalog::{
    ResidentEndpointCatalog, ResidentEndpointCatalogError, ResidentEndpointRoute,
};
use mere_resident::{CloseAction, CloseFuture, close_all};
use personae::bootstrap::Unlock;
use personae::{IdentityProvider, ProfileId};
use zeroize::Zeroize;

use crate::resident_blobs::ResidentBlobCustody;
use crate::resident_distillery::ResidentDistillery;
use crate::resident_knot::ResidentKnot;
use crate::settings::OwnerSettings;

const CASTELLAN_RECORD_SALT: &[u8] = b"mere.djinn/castellan/records/v1";
const CASTELLAN_FRESHNESS_SALT: &[u8] = b"mere.djinn/castellan/freshness/v1";

/// Process-wide services retained by one Djinn run.
pub struct DjinnResident {
    credentials: CastellanResident,
    blobs: ResidentBlobCustody,
    knot: Option<ResidentKnot>,
    distillery: Option<ResidentDistillery>,
}

impl DjinnResident {
    /// Open the credential, content, and optional Knot and Distillery
    /// authorities selected for one already-unlocked Personae profile.
    ///
    /// `vault_dir` and `unlock` are threaded through for the Distillery lane
    /// alone. It opens the shared Personae vault a second time, on the same
    /// directory, because its identity is a *profile-derived* mesh author
    /// rather than the process's `IdentityProvider` — and Personae's storage
    /// opens are not exclusive, so the two coexist. Passing an already-opened
    /// vault instead would mean either handing this constructor a Personae
    /// internal or moving the derivation out of Distillery, and both cost more
    /// than one extra open.
    pub async fn open<P: IdentityProvider + ?Sized>(
        identity: &P,
        data_root: &Path,
        profile: &ProfileId,
        owner: OwnerSettings,
        vault_dir: &Path,
        unlock: Unlock,
    ) -> Result<Self, String> {
        let credentials = claim_credentials(identity, data_root, profile)?;
        let blobs = ResidentBlobCustody::open(data_root, &owner.content).await?;
        let knot = match owner.knot {
            Some(settings) => match ResidentKnot::open(data_root, settings, blobs.clone()).await {
                Ok(knot) => Some(knot),
                Err(open_error) => {
                    let close_error = blobs.shutdown().await.err();
                    return Err(match close_error {
                        Some(close_error) => {
                            format!(
                                "open resident Knot: {open_error}; close blob custody: {close_error}"
                            )
                        }
                        None => open_error,
                    });
                }
            },
            None => None,
        };
        let distillery = match owner.distillery {
            Some(lane) => {
                match ResidentDistillery::open(data_root, profile, vault_dir, unlock, lane).await {
                    Ok(distillery) => Some(distillery),
                    Err(open_error) => {
                        // Unwind in reverse construction order: the Knot lane
                        // was built after blob custody and borrows it, so it
                        // has to let go first.
                        let knot_error = match knot {
                            Some(knot) => knot.close().await.err(),
                            None => None,
                        };
                        let close_error = blobs.shutdown().await.err();
                        let mut message = format!("open resident Distillery: {open_error}");
                        if let Some(knot_error) = knot_error {
                            message.push_str(&format!("; close resident Knot: {knot_error}"));
                        }
                        if let Some(close_error) = close_error {
                            message.push_str(&format!("; close blob custody: {close_error}"));
                        }
                        return Err(message);
                    }
                }
            }
            None => None,
        };
        Ok(Self {
            credentials,
            blobs,
            knot,
            distillery,
        })
    }

    /// The single record authority behind every Castellan view or adapter.
    pub fn credentials(&self) -> &CastellanResident {
        &self.credentials
    }

    /// Clone the shared physical blob custody for a composed product lane.
    pub fn blobs(&self) -> ResidentBlobCustody {
        self.blobs.clone()
    }

    /// Register the optional Knot source under its stable first-party route.
    pub fn register_knot_route(
        &self,
        catalog: &mut ResidentEndpointCatalog,
    ) -> Result<Option<ResidentEndpointRoute>, ResidentEndpointCatalogError> {
        let Some(knot) = self.knot.as_ref() else {
            return Ok(None);
        };
        knot.register(catalog)?;
        Ok(Some(ResidentKnot::route()))
    }

    /// Refresh mutable Knot pairing and evidence-reader authority.
    pub async fn refresh(&mut self) -> Result<bool, String> {
        match self.knot.as_mut() {
            Some(knot) => knot.refresh_settings().await,
            None => Ok(false),
        }
    }

    /// Whether this run has a configured resident Knot source.
    pub fn knot_enabled(&self) -> bool {
        self.knot.is_some()
    }

    /// Loggable Knot transport facts without exposing a source or signing key.
    pub fn knot_network_facts(&self) -> Option<([u8; 32], [u8; 32])> {
        let knot = self.knot.as_ref()?;
        Some((knot.node_id()?, knot.space_id()?))
    }

    /// Whether this run composes the resident Distillery works.
    pub fn distillery_enabled(&self) -> bool {
        self.distillery.is_some()
    }

    /// The composed works, for read-only projection and job posting.
    pub fn distillery(&self) -> Option<&ResidentDistillery> {
        self.distillery.as_ref()
    }

    /// Lift the works out so a caller can drive
    /// [`ResidentDistillery::run_until`] for the length of its run loop.
    ///
    /// This exists because that future needs `&mut` on the lane for the whole
    /// loop while the Knot arm of the same `select!` still needs `&mut` on this
    /// resident. Lifting the lane out splits the borrow that Rust cannot split
    /// on its own; [`restore_distillery`](Self::restore_distillery) hands it
    /// back before shutdown so the ordered close still owns it.
    pub fn take_distillery(&mut self) -> Option<ResidentDistillery> {
        self.distillery.take()
    }

    /// Return a lane taken by [`take_distillery`](Self::take_distillery).
    pub fn restore_distillery(&mut self, distillery: Option<ResidentDistillery>) {
        self.distillery = distillery;
    }

    /// Stop mesh work, then Knot networking, release credential authority, and
    /// finally flush the shared physical store. Call only after every broker
    /// and endpoint view borrowing this resident has stopped.
    pub async fn shutdown(self) -> Result<(), String> {
        let Self {
            credentials,
            blobs,
            knot,
            distillery,
        } = self;
        let report = close_all(vec![
            (
                // First: the works borrow nothing from blob custody or
                // credentials, and stopping work the ring believes is running
                // is the one close that other devices can observe. Everything
                // else here is local teardown.
                "Distillery",
                Box::new(move || {
                    Box::pin(async move {
                        match distillery {
                            Some(distillery) => distillery.close().await,
                            None => Ok(()),
                        }
                    }) as CloseFuture<'static>
                }) as CloseAction<'static>,
            ),
            (
                "Knot",
                Box::new(move || {
                    Box::pin(async move {
                        match knot {
                            Some(knot) => knot.close().await,
                            None => Ok(()),
                        }
                    }) as CloseFuture<'static>
                }) as CloseAction<'static>,
            ),
            (
                "Castellan",
                Box::new(move || {
                    Box::pin(async move {
                        drop(credentials);
                        Ok(())
                    }) as CloseFuture<'static>
                }) as CloseAction<'static>,
            ),
            (
                "blob custody",
                Box::new(move || {
                    Box::pin(async move { blobs.shutdown().await }) as CloseFuture<'static>
                }) as CloseAction<'static>,
            ),
        ])
        .await;
        if report.is_clean() {
            Ok(())
        } else {
            let failures = report
                .into_failures()
                .into_iter()
                .map(|failure| format!("{}: {}", failure.resource, failure.error))
                .collect::<Vec<_>>()
                .join("; ");
            Err(format!("Djinn resident shutdown failed ({failures})"))
        }
    }
}

fn claim_credentials<P: IdentityProvider + ?Sized>(
    identity: &P,
    data_root: &Path,
    profile: &ProfileId,
) -> Result<CastellanResident, String> {
    let profile_root = credential_profile_root(data_root, profile);
    let mut record_key = identity
        .derive_keypair(CASTELLAN_RECORD_SALT)
        .map_err(|error| format!("derive Castellan record key: {error}"))?
        .to_seed();
    let mut freshness_key = identity
        .derive_keypair(CASTELLAN_FRESHNESS_SALT)
        .map_err(|error| format!("derive Castellan freshness key: {error}"))?
        .to_seed();
    let claimed = CastellanResident::claim(
        profile_root.join("records"),
        record_key,
        profile_root.join("freshness"),
        freshness_key,
    );
    record_key.zeroize();
    freshness_key.zeroize();
    claimed.map_err(|error| format!("claim Castellan resident: {error}"))
}

fn credential_profile_root(data_root: &Path, profile: &ProfileId) -> PathBuf {
    let segment: String = profile
        .0
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect();
    data_root
        .join("castellan")
        .join("profiles")
        .join(if segment.is_empty() {
            "default"
        } else {
            &segment
        })
}

#[cfg(test)]
mod tests {
    use personae::{IdentityProvider, InMemoryProvider, ProfileId};

    use super::credential_profile_root;

    #[test]
    fn credential_roots_stay_profile_scoped_and_below_the_data_root() {
        let root = std::path::Path::new("resident-data");
        assert_eq!(
            credential_profile_root(root, &ProfileId("work/../burner".into())),
            root.join("castellan/profiles/work____burner")
        );
    }

    #[test]
    fn credential_derivation_is_distinct_from_the_personal_sync_identity() {
        let identity = InMemoryProvider::from_seed([0x91; 32]);
        assert_ne!(
            identity
                .derive_keypair(super::CASTELLAN_RECORD_SALT)
                .unwrap()
                .to_seed(),
            identity
                .derive_keypair(super::CASTELLAN_FRESHNESS_SALT)
                .unwrap()
                .to_seed()
        );
    }
}
