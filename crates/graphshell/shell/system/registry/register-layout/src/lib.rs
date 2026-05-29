/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Layout-domain registry — canvas, workbench-surface, and
//! profile-registry primitives, plus the conformance vocabulary
//! (`ConformanceLevel`, `CapabilityDeclaration`,
//! `SurfaceSubsystemCapabilities`) that surface profiles use to
//! declare per-subsystem guarantees.
//!
//! Extracted from `registries/domain/layout/` per Slice 55. The
//! `viewer_surface.rs` sub-module of the original directory stayed
//! in tree because it depends on `crate::registries::atomic::viewer`
//! (the viewer registry hasn't been extracted yet — `util::VersoAddress`
//! promotion is the prerequisite). Once viewer extracts, viewer_surface
//! folds in here.
//!
//! The aggregator `LayoutDomainRegistry` (which composed canvas +
//! workbench_surface + viewer_surface registries together) also stayed
//! in tree for the same reason — it can't compose what isn't here yet.
//! This crate exposes the registry primitives; the aggregator is a
//! shell-side composition concern until viewer lands.

pub mod canvas;
pub mod profile_registry;
pub mod workbench_surface;

/// Conformance level for a surface capability declaration.
///
/// Used by `CapabilityDeclaration` to declare whether a surface or profile
/// fully, partially, or does not implement a cross-cutting guarantee. Partial
/// conformance must carry a `reason`.
///
/// Populated at registry registration time; read by subsystem diagnostics and
/// validation to drive degraded-path warnings and conformance audit trails.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ConformanceLevel {
    /// Guarantee fully satisfied by this surface/profile.
    Full,
    /// Guarantee partially satisfied; `reason` must describe the gap.
    Partial,
    /// Guarantee not provided by this surface/profile.
    None,
}

/// Conformance declaration for a surface or viewer subsystem.
///
/// Registered alongside the owning profile to allow diagnostics to audit
/// conformance without reaching into runtime rendering code.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CapabilityDeclaration {
    pub level: ConformanceLevel,
    /// Required when `level` is `Partial` or `None`; describes the gap or
    /// degraded path.
    pub reason: Option<String>,
}

impl CapabilityDeclaration {
    pub fn full() -> Self {
        Self {
            level: ConformanceLevel::Full,
            reason: None,
        }
    }

    pub fn partial(reason: impl Into<String>) -> Self {
        Self {
            level: ConformanceLevel::Partial,
            reason: Some(reason.into()),
        }
    }

    pub fn none(reason: impl Into<String>) -> Self {
        Self {
            level: ConformanceLevel::None,
            reason: Some(reason.into()),
        }
    }
}

/// Folded subsystem conformance declarations carried by surface descriptors.
///
/// This keeps subsystem declarations typed and colocated with the owning
/// surface profile while allowing runtime/diagnostics code to inspect one field
/// instead of ad hoc per-subsystem plumbing.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SurfaceSubsystemCapabilities {
    pub accessibility: CapabilityDeclaration,
    pub security: CapabilityDeclaration,
    pub storage: CapabilityDeclaration,
    pub history: CapabilityDeclaration,
}

impl SurfaceSubsystemCapabilities {
    pub fn full() -> Self {
        Self {
            accessibility: CapabilityDeclaration::full(),
            security: CapabilityDeclaration::full(),
            storage: CapabilityDeclaration::full(),
            history: CapabilityDeclaration::full(),
        }
    }
}
