// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Portable content-engine state types.
//!
//! Shared vocabulary the shell uses to describe content viewers,
//! independent of which engine (Servo / Wry / `iced_webview` / Blitz /
//! a future MiddleNet Direct Lane) is backing any given pane. The
//! types here are WASM-clean and have no dependency on a particular
//! content engine.
//!
//! Each provider is expected to convert between its native types and
//! these portable ones at its boundary — typically in a
//! `*_status_sync` module on the shell side, or inside the provider
//! crate if it's already graphshell-aware. Conversion is always
//! lossless for the states this module defines; if a provider grows a
//! state that doesn't map, we add a variant here and update the
//! providers in lockstep.

use serde::{Deserialize, Serialize};

// TODO(m4-followon / portable viewer identity): this module currently
// only carries `ContentLoadState` because that was the trivial 3-
// variant wrap. The companion problem — wrapping `servo::WebViewId`
// as a portable `ViewerInstanceId` so the shell-state types can move
// to a servo-free sub-crate — still needs a design pass. Brief
// sketch of the options considered during the 2026-04-22 work, so
// this doesn't have to start from zero:
//
// - **Enum sum type across providers** (preferred). Looks like:
//
//   ```rust
//   pub enum ViewerInstanceId {
//       Servo(u64),          // encoding of WebViewId(PainterId, BrowsingContextId)
//       Wry(u64),            // Wry native handle
//       IcedWebview(u64),    // iced_webview native id
//       MiddlenetDirect(u32),
//   }
//   ```
//
//   Pros: explicit about which engine produced the id; no generic
//   type parameter propagates through `GraphshellRuntime`, the
//   authority bundles, or the view-model; mixed-provider shells (some
//   panes Servo, others Wry or iced_webview) work naturally.
//
//   Cons: 16 bytes instead of 12; each provider needs its own
//   `From<NativeId>` boundary impl; if a native id carries more than
//   64 bits we'd bump the variant payload.
//
//   Note on encoding: `servo::WebViewId` is 12 bytes
//   (`PainterId(PipelineNamespaceId, PipelineIndex)` + `BrowsingContextId`).
//   The pack/unpack is 1:1 by laying both structs' `u32` fields into
//   a `u64`. Deterministic, lossless, no registry needed.
//
// - **Provider-opaque `[u8; 16]` with a ProviderTag enum**. One
//   storage shape, flexible internal layout. Close second-place;
//   slightly less explicit than the enum sum and forces every call
//   site that reads the bytes to also read the tag.
//
// - **Generic `<V: ViewerIdentity>` parameter on shell-state types**.
//   Zero encoding cost; maximum type noise. `GraphshellRuntime<V>`,
//   `ToolbarAuthorityMut<'a, V>`, `FrameViewModel<V>`, ... Rejected
//   on readability grounds unless a compelling reason emerges.
//
// Adopting the enum sum unlocks moving `GraphshellRuntime`, the
// authority bundles that reference webview ids
// (`PendingWebviewContextSurfaceRequest`, `EmbeddedContentTarget`,
// `FocusedContentStatus`), and the remaining parts of `gui_state.rs`
// and `frame_model.rs` into a servo-free sub-crate. The
// `thumbnail_capture_in_flight: HashSet<ViewerInstanceId>` on the
// runtime would become portable; the toolbar/palette/omnibar types
// we already extracted or can trivially extract would follow.
//
// When that slice happens, update this module to own `ViewerInstanceId`
// alongside `ContentLoadState`, add provider-specific boundary
// conversions (Servo first, Wry/iced/MiddleNet as they land), and
// revisit the sub-crate extraction plan.

/// Life-cycle state of content loading in a viewer.
///
/// Models the three progress checkpoints every mainstream content
/// engine exposes, mapped onto the WHATWG `Document.readyState`
/// vocabulary where applicable:
///
/// - `Started` — fetch has kicked off; no content yet. Not a `readyState`
///   value itself but the pre-"loading" transition every engine surfaces.
/// - `HeadParsed` — corresponds to `document.readyState == "interactive"`:
///   the document element is available in the DOM, subresources may still
///   be loading.
/// - `Complete` — corresponds to `document.readyState == "complete"`:
///   document and all subresources have finished loading.
///
/// [`default`]: Default::default
///
/// Conversion from provider types:
///
/// - `servo::LoadStatus` — 1:1 on the three variants. See
///   `shell/desktop/lifecycle/webview_status_sync.rs::content_load_state_from_servo`.
/// - `iced_webview` — the `PageLoadStatus` / equivalent surface maps
///   onto these three checkpoints; converter lives alongside the iced
///   host adapter when iced lands as a production host.
/// - `Wry` (via `WebView::is_ready`) — only exposes a binary ready/not-
///   ready signal; provider converts by mapping not-ready → `Started`
///   and ready → `Complete`. `HeadParsed` is never observed under Wry
///   today, which is acceptable — the state is advisory.
/// - MiddleNet Direct Lane (Gemini/Gopher/Markdown viewers) — emits
///   `Complete` synchronously on first frame; `Started` only during
///   the fetch window.
///
/// Defaulting to [`Self::Complete`] matches the pre-wrap behavior of
/// the toolbar view-model and avoids spurious "loading" chrome during
/// cold startup before any viewer has reported status.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContentLoadState {
    /// Load request has been issued; no content bytes parsed yet.
    Started,
    /// The document's `<head>` has been parsed. Subresources may still
    /// be loading. Equivalent to `document.readyState == "interactive"`.
    HeadParsed,
    /// Document and subresources complete. Equivalent to
    /// `document.readyState == "complete"`.
    #[default]
    Complete,
}

impl ContentLoadState {
    /// `true` when the viewer has loaded enough content that interaction
    /// (scrolling, clicking links) is meaningful. Used by toolbar chrome
    /// to gate "loading" affordances.
    pub fn is_interactive(self) -> bool {
        matches!(self, Self::HeadParsed | Self::Complete)
    }

    /// `true` when the viewer has fully completed loading. Used to
    /// gate one-shot post-load side effects (thumbnail capture, title
    /// refresh, link-preview prefetch).
    pub fn is_complete(self) -> bool {
        matches!(self, Self::Complete)
    }

    /// Serialize to a stable machine-readable name. Prefer this over
    /// `{:?}` / `Debug` when round-tripping through persistence or
    /// diagnostic channels, so renaming the Rust variant doesn't break
    /// on-disk or on-wire data.
    pub fn persisted_name(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::HeadParsed => "head-parsed",
            Self::Complete => "complete",
        }
    }

    /// Inverse of [`persisted_name`]. Returns `None` for unknown names so
    /// the caller can decide whether to fall back to [`Default`] or
    /// surface a diagnostic.
    ///
    /// [`persisted_name`]: Self::persisted_name
    /// [`Default`]: Default::default
    pub fn from_persisted_name(raw: &str) -> Option<Self> {
        match raw {
            "started" => Some(Self::Started),
            "head-parsed" => Some(Self::HeadParsed),
            "complete" => Some(Self::Complete),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_complete() {
        // Toolbar chrome uses `Complete` as the cold-startup default to
        // avoid flashing "loading" affordances before any viewer
        // reports. Pin that in a test so changing the derive doesn't
        // silently break that behavior.
        assert_eq!(ContentLoadState::default(), ContentLoadState::Complete);
    }

    #[test]
    fn is_interactive_matches_head_parsed_and_complete() {
        assert!(!ContentLoadState::Started.is_interactive());
        assert!(ContentLoadState::HeadParsed.is_interactive());
        assert!(ContentLoadState::Complete.is_interactive());
    }

    #[test]
    fn is_complete_only_matches_complete() {
        assert!(!ContentLoadState::Started.is_complete());
        assert!(!ContentLoadState::HeadParsed.is_complete());
        assert!(ContentLoadState::Complete.is_complete());
    }

    #[test]
    fn persisted_name_round_trips_all_variants() {
        for state in [
            ContentLoadState::Started,
            ContentLoadState::HeadParsed,
            ContentLoadState::Complete,
        ] {
            let name = state.persisted_name();
            assert_eq!(
                ContentLoadState::from_persisted_name(name),
                Some(state),
                "persisted_name round-trip failed for {state:?} (name={name})",
            );
        }
    }

    #[test]
    fn from_persisted_name_rejects_unknown_values() {
        assert_eq!(ContentLoadState::from_persisted_name(""), None);
        assert_eq!(ContentLoadState::from_persisted_name("Complete"), None); // case-sensitive
        assert_eq!(ContentLoadState::from_persisted_name("loading"), None); // not a canonical name
    }

    #[test]
    fn persisted_names_are_lowercase_kebab() {
        // Serialization-layer convention: persisted names are lowercase
        // kebab-case. Enforcing this here so future contributors don't
        // introduce a `"HeadParsed"`-style variant that breaks existing
        // on-disk data.
        for state in [
            ContentLoadState::Started,
            ContentLoadState::HeadParsed,
            ContentLoadState::Complete,
        ] {
            let name = state.persisted_name();
            assert_eq!(
                name,
                name.to_ascii_lowercase(),
                "persisted name {name} must be lowercase"
            );
            assert!(
                !name.contains('_'),
                "persisted name {name} must use '-' (kebab), not '_' (snake)"
            );
        }
    }

    #[test]
    fn serde_json_round_trips_all_variants() {
        for state in [
            ContentLoadState::Started,
            ContentLoadState::HeadParsed,
            ContentLoadState::Complete,
        ] {
            let encoded = serde_json::to_string(&state).expect("serialize");
            let decoded: ContentLoadState = serde_json::from_str(&encoded).expect("deserialize");
            assert_eq!(decoded, state);
        }
    }
}

// ---------------------------------------------------------------------------
// Portable viewer-instance identity.
// ---------------------------------------------------------------------------

/// Provider-opaque identity for a content-viewer instance.
///
/// Graphshell's shell state tracks "which viewer has pending X?" across
/// multiple content engines — Servo, Wry, `iced_webview`, MiddleNet's
/// Direct Lane — that are expected to coexist in one shell. The enum
/// keeps the provider explicit so mixed-provider scenarios (some panes
/// backed by Servo, some by Wry, some by MiddleNet) work without a
/// generic type parameter propagating through
/// `GraphshellRuntime`, the authority bundles, or the view-model.
///
/// Each variant's payload is provider-chosen; each provider's boundary
/// code (typically in `shell/desktop/lifecycle/webview_status_sync.rs`
/// for Servo, analogous modules for other providers) converts to/from
/// its native identity. The payload is **session-local** — not stable
/// across app restarts — matching the lifecycle of every underlying
/// native viewer id.
///
/// Current encoding choices:
///
/// - `Servo`: JSON-encoded `servo::WebViewId`. `WebViewId`'s fields
///   are private to the servo crate, so the portable encoding goes
///   through `serde_json` for a stable, deterministic round-trip. The
///   per-op cost is a small allocation — acceptable for the low-
///   frequency mutation sites (thumbnail in-flight, focus target,
///   pending context-surface requests). If it becomes a hotspot, a
///   host-side registry mapping `WebViewId` ↔ `u64` is a drop-in
///   replacement that preserves this public API.
/// - `Wry`, `IcedWebview`, `MiddlenetDirect`: placeholder variants
///   representing the shape providers will fill in when they land.
///   Kept explicit in the enum today so match arms that handle
///   "all viewer providers" fail closed when a new one arrives.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ViewerInstanceId {
    /// A `servo::WebViewId` encoded as its `serde_json` representation.
    /// The inner string is treated as opaque by the shell-state crate;
    /// only the servo boundary inspects it.
    Servo(String),
    /// Reserved for future Wry viewer integration. The `u64` payload
    /// carries Wry's native handle; encoding pattern matches Wry's
    /// existing API when the integration lands.
    #[doc(hidden)]
    Wry(u64),
    /// Reserved for future `iced_webview` integration.
    #[doc(hidden)]
    IcedWebview(u64),
    /// Reserved for MiddleNet Direct Lane viewers (Gemini / Gopher /
    /// Markdown / plain-text / RSS). These are in-process viewers with
    /// simpler identity needs — a single `u32` namespace-local id.
    #[doc(hidden)]
    MiddlenetDirect(u32),
}

impl ViewerInstanceId {
    /// Name of the content provider that produced this id. Useful for
    /// diagnostics and for host code that needs to route dispatch per
    /// provider.
    pub fn provider_name(&self) -> &'static str {
        match self {
            Self::Servo(_) => "servo",
            Self::Wry(_) => "wry",
            Self::IcedWebview(_) => "iced_webview",
            Self::MiddlenetDirect(_) => "middlenet_direct",
        }
    }
}

#[cfg(test)]
mod viewer_instance_id_tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn provider_name_covers_all_variants() {
        assert_eq!(
            ViewerInstanceId::Servo(String::new()).provider_name(),
            "servo"
        );
        assert_eq!(ViewerInstanceId::Wry(0).provider_name(), "wry");
        assert_eq!(
            ViewerInstanceId::IcedWebview(0).provider_name(),
            "iced_webview"
        );
        assert_eq!(
            ViewerInstanceId::MiddlenetDirect(0).provider_name(),
            "middlenet_direct"
        );
    }

    #[test]
    fn different_providers_with_same_payload_dont_collide() {
        // A u64 0 under Wry and a u64 0 under IcedWebview must NOT
        // compare equal or hash to the same bucket. Matters for shells
        // that run multiple providers — a thumbnail capture pending
        // for Wry viewer #0 must not be mistaken for one pending under
        // iced_webview viewer #0.
        let a = ViewerInstanceId::Wry(0);
        let b = ViewerInstanceId::IcedWebview(0);
        assert_ne!(a, b);

        let mut set = HashSet::new();
        set.insert(a);
        assert!(!set.contains(&b));
        set.insert(b);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn servo_equal_payloads_are_equal() {
        // Two portable ids encoded from the same underlying servo
        // WebViewId must compare equal. Pin the string-equality
        // contract so the JSON-encoding approach remains sound — if
        // we switch to a host-registry-assigned u64 later, this
        // semantic stays the same.
        let a = ViewerInstanceId::Servo("TEST-ENCODING-1".to_string());
        let b = ViewerInstanceId::Servo("TEST-ENCODING-1".to_string());
        assert_eq!(a, b);

        let c = ViewerInstanceId::Servo("TEST-ENCODING-2".to_string());
        assert_ne!(a, c);
    }

    #[test]
    fn hashset_membership_works_across_clone() {
        let id = ViewerInstanceId::Servo("MVP-123".to_string());
        let mut set = HashSet::new();
        set.insert(id.clone());
        assert!(set.contains(&id));
        assert!(set.contains(&ViewerInstanceId::Servo("MVP-123".to_string())));
        set.remove(&ViewerInstanceId::Servo("MVP-123".to_string()));
        assert!(set.is_empty());
    }

    #[test]
    fn serde_json_roundtrip_preserves_variant_and_payload() {
        let samples = [
            ViewerInstanceId::Servo("{encoded-servo-id}".to_string()),
            ViewerInstanceId::Wry(42),
            ViewerInstanceId::IcedWebview(99),
            ViewerInstanceId::MiddlenetDirect(7),
        ];
        for id in samples {
            let encoded = serde_json::to_string(&id).expect("serialize");
            let decoded: ViewerInstanceId = serde_json::from_str(&encoded).expect("deserialize");
            assert_eq!(decoded, id);
        }
    }
}
