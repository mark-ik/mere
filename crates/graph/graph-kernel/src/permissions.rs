// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Settings/permissions scope hierarchy and the narrowing rule.
//!
//! Adopted from the donor `graphshell` `settings_and_permissions_spine_spec.md`
//! (per the [adoption roadmap](../../../../../design_docs/mere_docs/implementation_strategy/2026-05-27_adoption_roadmap.md)
//! R0). This is the host-agnostic *policy core*: the scope vocabulary plus the
//! resolution rule. Storage of per-scope values and the host wiring that gates
//! surfaces/mods consume this; they are not here.
//!
//! ## The five scopes
//!
//! [`SettingScope`] is a hierarchy from broadest to narrowest, grounded in
//! Mere's own structure (a persona owns sessions; a session/window hosts graphs;
//! a graph hosts surfaces/tiles):
//!
//! 1. `App` — installation-wide defaults.
//! 2. `Persona` — one human's persona (work / research / throwaway).
//! 3. `Session` — a session, i.e. a graph-shaped window.
//! 4. `Graph` — a specific graph (orrery / moot) within a session.
//! 5. `Surface` — a tile / pane / surface within a graph.
//!
//! The same hierarchy serves *settings* (where the narrower scope simply
//! overrides) and *permissions* (where the narrower scope may only narrow).
//! This module implements the **permission** resolution; a settings-override
//! resolver is a future sibling that reuses [`SettingScope`].
//!
//! ## The narrowing rule (the R0 invariant)
//!
//! **A narrower scope can only narrow a permission, never broaden it.** No
//! persona, session, graph, or surface can grant itself more than a broader
//! scope allows. This is defense in depth: tighten centrally and trust that
//! nothing downstream loosens it.
//!
//! The rule is enforced *structurally*, not by convention: [`resolve_permission`]
//! returns the **most restrictive** opinion across the chain
//! (`Allow` < `Prompt` < `Deny`). Because resolution is a maximum over
//! restrictiveness, a narrower scope's `Allow` can never override a broader
//! scope's `Deny` — there is no code path that broadens.
//!
//! This supersedes the earlier capability-gate-catalogue "first-match-wins"
//! framing for permissions (first-match can let an earlier, broader opinion win
//! over a stricter later one — wrong direction for security). `RequireConsent`
//! from that brief is preserved as the [`Permission::Prompt`] middle state.

use serde::{Deserialize, Serialize};

/// The scope a setting or permission is declared at, broadest to narrowest.
/// The discriminant order *is* the breadth order (`App` broadest).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SettingScope {
    App,
    Persona,
    Session,
    Graph,
    Surface,
}

impl SettingScope {
    /// The scopes broadest-to-narrowest. Callers assemble a permission chain in
    /// this order (omitting scopes with no opinion, or marking them
    /// [`Permission::Inherit`]).
    pub const BROAD_TO_NARROW: [SettingScope; 5] = [
        Self::App,
        Self::Persona,
        Self::Session,
        Self::Graph,
        Self::Surface,
    ];
}

/// A permission opinion at one scope.
///
/// Restrictiveness orders `Allow` < `Prompt` < `Deny`; [`Inherit`](Self::Inherit)
/// is "no opinion, defer to the rest of the chain".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Permission {
    /// No opinion at this scope; defer to other scopes (and the default).
    Inherit,
    /// Permitted (unless a scope narrows it).
    Allow,
    /// Permitted only after explicit user consent (the capability-gate brief's
    /// `RequireConsent`). More restrictive than `Allow`, less than `Deny`.
    Prompt,
    /// Forbidden. The most restrictive opinion; no scope can broaden past it.
    Deny,
}

impl Permission {
    /// Restrictiveness rank for resolution. `None` for [`Self::Inherit`]
    /// (excluded from the maximum). Higher = more restrictive.
    fn restrictiveness(self) -> Option<u8> {
        match self {
            Self::Inherit => None,
            Self::Allow => Some(0),
            Self::Prompt => Some(1),
            Self::Deny => Some(2),
        }
    }
}

/// One scope's opinion in a resolution chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopedPermission {
    pub scope: SettingScope,
    pub permission: Permission,
}

impl ScopedPermission {
    pub fn new(scope: SettingScope, permission: Permission) -> Self {
        Self { scope, permission }
    }
}

/// The outcome of [`resolve_permission`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedPermission {
    /// The effective permission after applying the narrowing rule. Never
    /// [`Permission::Inherit`] (Inherit folds into the default).
    pub effective: Permission,
    /// The scope whose opinion decided the result, if any opinion was expressed
    /// (the narrowest scope among the most-restrictive opinions). `None` when
    /// every scope inherited and the default applied.
    pub decided_by: Option<SettingScope>,
}

/// Resolve a permission across a scope `chain` per the narrowing rule: the
/// effective permission is the **most restrictive** non-[`Permission::Inherit`]
/// opinion in the chain. `default` applies when every scope inherits.
///
/// `chain` should be ordered broadest-to-narrowest (see
/// [`SettingScope::BROAD_TO_NARROW`]); among equally-restrictive opinions the
/// narrowest (latest) is reported as `decided_by`, which is the most useful one
/// to surface to the user ("denied for this tile").
pub fn resolve_permission(chain: &[ScopedPermission], default: Permission) -> ResolvedPermission {
    let winner = chain
        .iter()
        .filter(|scoped| scoped.permission.restrictiveness().is_some())
        .max_by_key(|scoped| scoped.permission.restrictiveness().unwrap_or(0));

    match winner {
        Some(scoped) => ResolvedPermission {
            effective: scoped.permission,
            decided_by: Some(scoped.scope),
        },
        None => ResolvedPermission {
            effective: default,
            decided_by: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use SettingScope::*;

    fn chain(items: &[(SettingScope, Permission)]) -> Vec<ScopedPermission> {
        items
            .iter()
            .map(|&(scope, permission)| ScopedPermission::new(scope, permission))
            .collect()
    }

    #[test]
    fn narrower_deny_overrides_broader_allow() {
        let c = chain(&[(App, Permission::Allow), (Session, Permission::Deny)]);
        let r = resolve_permission(&c, Permission::Allow);
        assert_eq!(r.effective, Permission::Deny);
        assert_eq!(r.decided_by, Some(Session));
    }

    #[test]
    fn narrower_allow_cannot_broaden_a_broader_deny() {
        // The crux of the narrowing rule: a Surface saying Allow does NOT
        // override an App-level Deny.
        let c = chain(&[(App, Permission::Deny), (Surface, Permission::Allow)]);
        let r = resolve_permission(&c, Permission::Allow);
        assert_eq!(
            r.effective,
            Permission::Deny,
            "narrower scope broadened a Deny"
        );
        assert_eq!(r.decided_by, Some(App));
    }

    #[test]
    fn prompt_sits_between_allow_and_deny() {
        let c = chain(&[(App, Permission::Allow), (Persona, Permission::Prompt)]);
        assert_eq!(
            resolve_permission(&c, Permission::Allow).effective,
            Permission::Prompt
        );

        // ...but Deny still wins over Prompt.
        let c = chain(&[(Persona, Permission::Prompt), (Graph, Permission::Deny)]);
        assert_eq!(
            resolve_permission(&c, Permission::Allow).effective,
            Permission::Deny
        );
    }

    #[test]
    fn all_inherit_falls_back_to_default() {
        let c = chain(&[(App, Permission::Inherit), (Session, Permission::Inherit)]);
        let r = resolve_permission(&c, Permission::Prompt);
        assert_eq!(r.effective, Permission::Prompt);
        assert_eq!(r.decided_by, None);
    }

    #[test]
    fn decided_by_is_the_narrowest_among_equally_restrictive() {
        // Two Denies: the narrower (Surface) is the more useful explanation.
        let c = chain(&[(Session, Permission::Deny), (Surface, Permission::Deny)]);
        assert_eq!(
            resolve_permission(&c, Permission::Allow).decided_by,
            Some(Surface)
        );
    }

    #[test]
    fn scopes_are_ordered_broad_to_narrow() {
        assert!(App < Persona && Persona < Session && Session < Graph && Graph < Surface);
        assert_eq!(SettingScope::BROAD_TO_NARROW.len(), 5);
    }
}
