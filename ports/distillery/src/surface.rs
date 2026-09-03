// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Read-only Cambium projection of installed Distillery authority.
//!
//! The surface retains a projection supplied by the installed bootstrap and
//! resident observer. It never opens a vault, runs a tick, changes retention,
//! or reconstructs a board. Its role is exactly the same as a contributed
//! product session: render product-owned facts through Cambium while the host
//! owns placement, input routing, accessibility, and lifecycle.

use std::path::PathBuf;

use cambium::{
    AnyView, DomHandle, GenetAppRunner, GenetCtx, GenetElement, RunnerSurfaceSession, div, el, span,
};
use mere_surface_api::{
    ProviderId, SourceKindId, SurfaceAvailability, SurfaceDescriptor, SurfaceId, SurfaceSourceShape,
};
use muniment::Backend;

use crate::{InstalledAuthority, ResidentAuthority, ResidentReceipt, ResidentSettings};

/// Styling required by the installed authority surface.
pub const DISTILLERY_INSTALLED_CSS: &str = ".distillery-installed { display: flex; flex-direction: column; gap: 8px; } .distillery-installed-status { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; } .distillery-installed-item { white-space: pre-wrap; overflow-wrap: anywhere; }";

const SURFACE_ID: &str = "distillery.installed.v1";

/// Snapshot of resident facts that already have one owner-selected value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DistilleryResidentSnapshotV1 {
    /// The exact settings projected into the resident process.
    pub settings: ResidentSettings,
}

/// Read-only facts selected by the installed bootstrap for one mesh surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DistilleryInstalledSnapshotV1 {
    /// Personae profile selected in Distillery's persisted product settings.
    pub profile: String,
    /// Personae's own description of the profile's at-rest protection.
    pub protection: String,
    /// The mesh this surface projects.
    pub mesh_id: [u8; 32],
    /// The product-private root that contains this mesh's durable state.
    pub mesh_root: PathBuf,
    /// The mesh-policy owner's redb store location.
    pub mesh_store_path: PathBuf,
    /// The resident blob-store location.
    pub blob_store_root: PathBuf,
    /// Settings from a bound resident, absent before the resident is composed.
    pub resident: Option<DistilleryResidentSnapshotV1>,
}

impl DistilleryInstalledSnapshotV1 {
    /// Project the exact profile and private paths selected by installed
    /// authority. This does not unlock anything beyond the authority already
    /// opened by the caller.
    pub fn from_installed(authority: &InstalledAuthority, mesh_id: [u8; 32]) -> Self {
        let paths = authority.paths(mesh_id);
        Self {
            profile: authority.profile().0,
            protection: authority.protection().to_owned(),
            mesh_id,
            mesh_root: paths.root().to_path_buf(),
            mesh_store_path: paths.mesh_store_path(),
            blob_store_root: paths.blob_store_root(),
            resident: None,
        }
    }
}

/// Retained product state for the read-only installed authority surface.
pub struct DistilleryInstalledSurfaceState {
    snapshot: DistilleryInstalledSnapshotV1,
    latest_receipt: Option<ResidentReceipt>,
}

impl DistilleryInstalledSurfaceState {
    /// Start with the installed bootstrap facts for one selected mesh.
    pub fn new(snapshot: DistilleryInstalledSnapshotV1) -> Self {
        Self {
            snapshot,
            latest_receipt: None,
        }
    }

    /// The current product-owned projection.
    pub fn snapshot(&self) -> &DistilleryInstalledSnapshotV1 {
        &self.snapshot
    }

    /// The latest exact resident receipt supplied by the resident observer.
    pub fn latest_receipt(&self) -> Option<&ResidentReceipt> {
        self.latest_receipt.as_ref()
    }

    /// Copy the settings from the actual resident bound for this mesh.
    ///
    /// The type parameter stays entirely within Distillery. Cambium sees only
    /// the copied settings and never the mesh backend or authority handle.
    pub fn observe_resident<B>(&mut self, resident: &ResidentAuthority<B>)
    where
        B: Backend + Clone + Send + Sync + 'static,
    {
        self.snapshot.resident = Some(DistilleryResidentSnapshotV1 {
            settings: resident.settings(),
        });
    }

    /// Record an exact receipt delivered by [`ResidentAuthority::run_until`].
    ///
    /// This is a projection update, not another resident log or scheduler. A
    /// caller passes the same receipt it was already given in its run observer.
    pub fn observe_receipt(&mut self, receipt: ResidentReceipt) {
        self.latest_receipt = Some(receipt);
    }
}

/// The concrete state/view pair rendered by the retained session.
pub type DistilleryInstalledView =
    Box<dyn AnyView<DistilleryInstalledSurfaceState, (), GenetCtx, GenetElement>>;

/// Render the installed profile, storage boundary, resident settings, and last
/// observed receipt. There are deliberately no buttons or mutable controls.
pub fn distillery_installed_view(
    state: &DistilleryInstalledSurfaceState,
) -> DistilleryInstalledView {
    let snapshot = state.snapshot();
    let resident = snapshot.resident.map_or_else(
        || "Resident: not bound".to_owned(),
        |resident| {
            let settings = resident.settings;
            format!(
                "Resident: tick {} ms; maintenance {}; blob collection {} ms; release settled custody {}",
                settings.tick_every.as_millis(),
                settings
                    .maintenance_every
                    .map(|every| format!("{} ms", every.as_millis()))
                    .unwrap_or_else(|| "explicit only".to_owned()),
                settings.blob_gc_every.as_millis(),
                if settings.retention.collect_after_checkpoint { "enabled" } else { "disabled" },
            )
        },
    );
    let receipt = state
        .latest_receipt()
        .map(receipt_label)
        .unwrap_or_else(|| "Resident receipt: none observed".to_owned());
    Box::new(
        el(
            "section",
            div((
                span(format!("Profile: {}", snapshot.profile)),
                span(format!("Protection: {}", snapshot.protection)),
                span(format!("Mesh: {}", hex(snapshot.mesh_id))),
                span(format!("Mesh root: {}", snapshot.mesh_root.display())),
                span(format!(
                    "Mesh store: {}",
                    snapshot.mesh_store_path.display()
                )),
                span(format!(
                    "Blob store: {}",
                    snapshot.blob_store_root.display()
                )),
                span(resident),
                span(receipt),
            ))
            .attr("class", "distillery-installed-status"),
        )
        .attr("class", "distillery-installed")
        .attr("data-surface", SURFACE_ID)
        .attr("role", "status")
        .attr("aria-label", "Distillery installed authority"),
    )
}

/// Stable data-only descriptor for the installed Distillery surface.
pub fn distillery_installed_descriptor() -> SurfaceDescriptor {
    SurfaceDescriptor {
        provider_id: ProviderId::from("distillery"),
        surface_id: SurfaceId::from(SURFACE_ID),
        label: "Distillery".to_owned(),
        accepted_source: SurfaceSourceShape::One(SourceKindId::from(SURFACE_ID)),
    }
}

/// Erase the concrete read-only state behind Cambium's shared retained-session
/// contract, ready for a contributed-port host.
pub fn distillery_installed_surface(
    dom: DomHandle,
    state: DistilleryInstalledSurfaceState,
) -> Box<dyn cambium::RetainedSurfaceSession> {
    let runner = GenetAppRunner::new(dom, distillery_installed_view, state);
    Box::new(RunnerSurfaceSession::new(
        distillery_installed_descriptor(),
        runner,
        |_state: &DistilleryInstalledSurfaceState| SurfaceAvailability::Available,
        |_state, _viewport| {},
        |_action: ()| Vec::new(),
    ))
}

fn receipt_label(receipt: &ResidentReceipt) -> String {
    match receipt {
        ResidentReceipt::Tick { steps } => {
            format!("Resident receipt: tick ({} steps)", steps.len())
        }
        ResidentReceipt::MaintenanceCompleted(report) => format!(
            "Resident receipt: maintenance completed ({} candidates, {} custody tags released)",
            report.candidates, report.collected
        ),
        ResidentReceipt::MaintenanceIdle => "Resident receipt: maintenance idle".to_owned(),
        ResidentReceipt::MaintenanceFailed { error } => {
            format!("Resident receipt: maintenance failed ({error})")
        }
        ResidentReceipt::SupervisorFailed { error } => {
            format!("Resident receipt: supervisor failed ({error})")
        }
        ResidentReceipt::StopRequested => "Resident receipt: stop requested".to_owned(),
    }
}

fn hex(bytes: [u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use genet_scripted_dom::ScriptedDom;
    use layout_dom_api::LayoutDom;
    use personae::ProfileId;
    use personae::bootstrap::{self, Unlock, load_or_create_profile};

    use super::*;

    const MESH: [u8; 32] = [0xD5; 32];

    fn unlock() -> Unlock {
        Unlock::passphrase(b"distillery-surface-test-passphrase")
    }

    fn authority(root: &std::path::Path) -> InstalledAuthority {
        let vault_dir = root.join("vault");
        let profile = ProfileId("research".into());
        let opened = bootstrap::open_storage(&vault_dir, unlock()).unwrap();
        load_or_create_profile(&*opened.storage, &profile).unwrap();
        InstalledAuthority::configure(root, profile).unwrap();
        InstalledAuthority::open_with(root, &vault_dir, unlock()).unwrap()
    }

    fn text_present(dom: &ScriptedDom, needle: &str) -> bool {
        fn contains(dom: &ScriptedDom, node: genet_scripted_dom::NodeId, needle: &str) -> bool {
            dom.text(node).is_some_and(|text| text.contains(needle))
                || dom
                    .dom_children(node)
                    .any(|child| contains(dom, child, needle))
        }
        contains(dom, dom.document(), needle)
    }

    #[test]
    fn snapshot_and_erased_surface_project_the_configured_profile_and_paths() {
        let directory = tempfile::tempdir().unwrap();
        let authority = authority(directory.path());
        let snapshot = DistilleryInstalledSnapshotV1::from_installed(&authority, MESH);
        assert_eq!(snapshot.profile, "research");
        assert_eq!(snapshot.mesh_id, MESH);
        assert!(snapshot.resident.is_none());
        assert!(snapshot.mesh_store_path.ends_with("mesh.redb"));

        let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
        let session = distillery_installed_surface(
            dom.clone(),
            DistilleryInstalledSurfaceState::new(snapshot),
        );
        assert_eq!(session.descriptor(), &distillery_installed_descriptor());
        assert!(session.availability().is_available());
        let rendered = dom.borrow();
        assert!(text_present(&rendered, "Profile: research"));
        assert!(text_present(&rendered, "Resident: not bound"));
        assert!(text_present(&rendered, "Blob store:"));
    }

    #[test]
    fn exact_resident_receipts_render_without_a_parallel_lifecycle_model() {
        let directory = tempfile::tempdir().unwrap();
        let authority = authority(directory.path());
        let snapshot = DistilleryInstalledSnapshotV1::from_installed(&authority, MESH);
        let mut state = DistilleryInstalledSurfaceState::new(snapshot);
        state.observe_receipt(ResidentReceipt::MaintenanceFailed {
            error: "live lease".into(),
        });
        assert!(matches!(
            state.latest_receipt(),
            Some(ResidentReceipt::MaintenanceFailed { error }) if error == "live lease"
        ));
        let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
        let _runner = GenetAppRunner::new(dom.clone(), distillery_installed_view, state);
        assert!(text_present(
            &dom.borrow(),
            "Resident receipt: maintenance failed (live lease)"
        ));
    }
}
