/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! P2.5c: the scripted-page DOM path (the hybrid half of the unified document
//! model). A fetched page is parsed to a `StaticDocument` (full-document fidelity +
//! the stylesheet extraction the unscripted fast path already does). For a page
//! with an attached DocumentScript, its tree is **mirrored** into a mutable
//! `ScriptedDom` so the script can mutate the live page; the same extracted
//! stylesheets lay it out and the existing generic `scene_from_content_band`
//! renders it. Unscripted pages keep the `StaticDocument` path untouched.
//!
//! No serval change is needed: `ScriptedDom::set_inner_html` is fragment-only (it
//! drops `<head>`/`<style>`/`<link>`), but `build_html_layout` already extracts the
//! sheets separately and `lay_out_content`/`ContentLayout` are generic over
//! `LayoutDom`, so the mirror + the same sheets reproduce the static render.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use document_host::{
    CapPermission, DocumentScript, Grant, NetFetcher, NetResponse, Quota, TurnOutcome,
};
use kernel::permissions::{
    Permission, ResolvedPermission, ScopedPermission, SettingScope, resolve_permission,
};
use layout_dom_api::{LayoutDom, LayoutDomMut, NodeKind};
use serval_layout::{
    ContentLayout, ImageLoader, inline_stylesheets, lay_out_content, linked_stylesheets_with_loader,
};
use serval_scripted_dom::{NodeId, ScriptedDom};
use serval_static_dom::StaticDocument;
use session_runtime::settings_store::ScriptPermissionPrefs;

use crate::card::HTML_SHEET;

/// Overall ceiling on one `net.fetch`, covering connect + headers + body across
/// both transports. Bounds how long a single fetch can park the content actor
/// thread (the http path has no inner timeout of its own; smolweb has its own
/// per-hop one). A slow/black-hole server therefore frees the tile after this.
const NET_FETCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Body-size cap for a script `net.fetch` (§A5): tighter than a page load since the
/// response is copied into the guest's mem-quota'd linear memory. Enforced while
/// streaming, so an oversized response is refused, not buffered.
const SCRIPT_FETCH_BODY_CAP: usize = 8 * 1024 * 1024;

/// The content actor's real `net.fetch` backend (§11.7-7). Holds a current-thread
/// tokio runtime and `block_on`s the shared [`crate::fetch::fetch_page`] routing
/// (http/https via netfetcher, smolweb via errand) — a *blocking* fetch on the actor
/// thread while the wasm fiber is parked. Built lazily per content actor on first
/// script attach (so unscripted tiles pay nothing). Egress is hardened at this seam:
/// only real network schemes ([`crate::fetch::is_fetchable`]), never a loopback /
/// private host ([`is_disallowed_fetch_host`]), and bounded by [`NET_FETCH_TIMEOUT`].
/// `status` is 200 on success (the `Fetched` shape carries only the 2xx body; exact
/// status is a follow-on). The credential surface (ambient cookies, origin scoping)
/// is the larger hardening tracked in the net-hardening plan — net stays prototype.
pub(crate) struct ContentNetFetcher {
    rt: tokio::runtime::Runtime,
}

impl ContentNetFetcher {
    pub(crate) fn new() -> std::io::Result<Self> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        Ok(Self { rt })
    }
}

impl NetFetcher for ContentNetFetcher {
    fn fetch(&self, url: &str) -> Result<NetResponse, String> {
        // Scheme floor: only genuine network egress (http/https + smolweb), never
        // mere:// / about: / file: — fail fast, matching every other load path.
        if !crate::fetch::is_fetchable(url) {
            return Err(format!("net.fetch: non-network scheme: {url}"));
        }
        // SSRF floor: a granted script must not reach the user's loopback / intranet.
        if is_disallowed_fetch_host(url) {
            return Err(format!("net.fetch: blocked host (loopback/private): {url}"));
        }
        // Bounded so one fetch cannot wedge the tile actor forever (covers http,
        // which has no inner deadline, as well as smolweb).
        let result = self.rt.block_on(async {
            tokio::time::timeout(
                NET_FETCH_TIMEOUT,
                crate::fetch::fetch_page_capped(url, SCRIPT_FETCH_BODY_CAP),
            )
            .await
        });
        let fetched = match result {
            Ok(inner) => inner?,
            Err(_elapsed) => {
                return Err(format!(
                    "net.fetch: timed out after {}s",
                    NET_FETCH_TIMEOUT.as_secs()
                ));
            }
        };
        Ok(NetResponse {
            status: 200,
            content_type: fetched.content_type,
            body: fetched.body,
        })
    }
}

/// Whether `url`'s host is one a granted script must not reach: loopback, private
/// (RFC1918), link-local, CGNAT, the unspecified address, or the literal
/// `localhost`. An SSRF floor so a net-granted script cannot probe the user's local
/// services / intranet. Literal-IP only — a hostname that resolves to a private IP
/// (DNS rebinding) is a follow-on (resolve + pin); a non-IP, non-`localhost` host is
/// allowed through.
fn is_disallowed_fetch_host(url: &str) -> bool {
    let host = host_of(url);
    if host == "localhost" {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(v4)) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                // CGNAT 100.64.0.0/10
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 0x40)
        }
        Ok(std::net::IpAddr::V6(v6)) => {
            v6.is_loopback()
                || v6.is_unspecified()
                // unique-local fc00::/7
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                // link-local fe80::/10
                || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
        // Not an IP literal: a DNS name we cannot classify without resolving.
        Err(_) => false,
    }
}

/// Map a kernel-resolved permission to a document-host capability permission. The
/// kernel's narrowing rule (`resolve_permission`) runs host-side; this only
/// translates the effective opinion. `Inherit` cannot reach `effective` (resolution
/// folds it into the default); it maps to `Deny` defensively (fail-closed).
fn cap_permission(resolved: ResolvedPermission) -> CapPermission {
    match resolved.effective {
        Permission::Allow => CapPermission::Allow,
        Permission::Prompt => CapPermission::Prompt,
        Permission::Deny | Permission::Inherit => CapPermission::Deny,
    }
}

/// App-scope defaults. `log` / `document` default **Allow** (a user-typed
/// `>attach-script` is permitted by default; `document` in particular must default
/// Allow or the document-core guest — which requires it — could never instantiate).
/// `net` (network egress) defaults **Deny**: it is powerful, so a narrower scope
/// (session opinion or an origin binding) must *explicitly* grant it.
const APP_DEFAULT: Permission = Permission::Allow;
const NET_APP_DEFAULT: Permission = Permission::Deny;

/// A per-capability **Session-scope** override for script capabilities (the
/// `settings.json` `script_permissions` entry, §11.4). `None` on a capability = no
/// opinion at this scope (`Inherit`), so the App default stands.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ScriptCapPolicy {
    pub log: Option<Permission>,
    pub document: Option<Permission>,
    pub net: Option<Permission>,
}

/// Resolve a script's per-capability permissions through the scope chain (App
/// default + optional Session override) per the kernel narrowing rule, returning the
/// effective `(log, document, net)` to carry in `AttachScript`. The host calls this;
/// [`grant_from_resolved`] then maps the result to the link grant. The host-side half
/// of the §11.4 permissions seam (document-host stays kernel-free).
pub(crate) fn resolve_attach_permissions(
    session: ScriptCapPolicy,
) -> (ResolvedPermission, ResolvedPermission, ResolvedPermission) {
    (
        resolve_cap(session.log, APP_DEFAULT),
        resolve_cap(session.document, APP_DEFAULT),
        resolve_cap(session.net, NET_APP_DEFAULT),
    )
}

/// Resolve one capability. `default` is the **silent baseline** (the App-wide value
/// when no scope opines), passed as `resolve_permission`'s default rather than an
/// explicit App-scope opinion — this is what lets a default-Deny capability (`net`)
/// still be *granted* by a narrower scope. If the App level were an explicit `Deny`,
/// the narrowing rule (max-restrictiveness) would make narrower `Allow`s powerless and
/// `net` could never be granted. So: silent baseline = `default`; a Session-scope
/// opinion (and later Graph / Surface) narrows or, against a Deny baseline, grants.
fn resolve_cap(session: Option<Permission>, default: Permission) -> ResolvedPermission {
    let chain: Vec<ScopedPermission> = match session {
        Some(p) => vec![ScopedPermission::new(SettingScope::Session, p)],
        None => Vec::new(),
    };
    resolve_permission(&chain, default)
}

/// Build a document-host [`Grant`] from the host-resolved permissions for the three
/// application capabilities (`log`, `document`, `net`). The `kernel::permissions` ->
/// `Grant` seam (§11.4): the grant *policy* lives in document-host, the five-scope
/// *resolution* is the host's input. A `Deny`/`Prompt` on a capability the component
/// *requires* makes instantiation fail — the boundary. (So a `net`-importing script
/// attaches only where `net` resolved to Allow.)
pub(crate) fn grant_from_resolved(
    log: ResolvedPermission,
    document: ResolvedPermission,
    net: ResolvedPermission,
) -> Grant {
    Grant {
        log: cap_permission(log),
        document: cap_permission(document),
        net: cap_permission(net),
    }
}

/// A resolved auto-attach binding (follow-on #2): an origin pattern + the component
/// to attach + the resolved capability permissions. The host resolves these once
/// from `script-bindings.json` + the session permission policy and pushes them to
/// the constellation; on a fresh navigation matching `origin`, the script
/// auto-attaches via the same path the omnibar verb uses.
#[derive(Clone, Debug)]
pub(crate) struct ResolvedScriptBinding {
    pub origin: String,
    pub component_path: PathBuf,
    pub log: ResolvedPermission,
    pub document: ResolvedPermission,
    pub net: ResolvedPermission,
}

/// The normalized host of a URL: the authority between `://` and the next
/// `/` `?` `#`, with userinfo (`user[:pass]@`) and `:port` stripped, IPv6 brackets
/// removed, lowercased. So `https://u@Example.COM:8443/p` -> `example.com`. Used for
/// both binding matching and the SSRF host check, so a non-default port or a cased
/// host no longer silently misses a binding (it used to keep userinfo + port).
/// `pub(crate)` so the content actor derives a script's same-origin `net` allowlist
/// (§E1) from its page URL.
pub(crate) fn host_of(url: &str) -> String {
    let after_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let authority = after_scheme.split(['/', '?', '#']).next().unwrap_or("");
    // Drop userinfo: everything up to and including the last '@'.
    let host_port = authority
        .rsplit_once('@')
        .map(|(_, h)| h)
        .unwrap_or(authority);
    // Drop the port. For an IPv6 literal ([..]:port) take the bracketed host; the
    // ':' inside the address must not be read as a port separator.
    let host = if let Some(rest) = host_port.strip_prefix('[') {
        rest.split_once(']')
            .map(|(inner, _)| inner)
            .unwrap_or(host_port)
    } else {
        host_port.split(':').next().unwrap_or(host_port)
    };
    host.to_ascii_lowercase()
}

/// Whether `url`'s host matches `pattern` — an exact host (`example.com`) or a
/// `*.`-prefixed suffix glob (`*.example.com`, matching the apex and any subdomain).
/// Both sides are compared on the normalized, lowercased host (see [`host_of`]).
pub(crate) fn origin_matches(url: &str, pattern: &str) -> bool {
    let host = host_of(url);
    let pattern = pattern.to_ascii_lowercase();
    match pattern.strip_prefix("*.") {
        Some(suffix) => host == suffix || host.ends_with(&format!(".{suffix}")),
        None => host == pattern,
    }
}

/// The first binding whose origin matches `url` (the auto-attach lookup the
/// constellation runs on a fresh navigation).
pub(crate) fn binding_for<'a>(
    url: &str,
    bindings: &'a [ResolvedScriptBinding],
) -> Option<&'a ResolvedScriptBinding> {
    bindings.iter().find(|b| origin_matches(url, &b.origin))
}

/// Load the installed origin bindings from `<mere_root>/script-bindings.json` and
/// resolve each against the session script-permission `prefs` (App-default Allow,
/// narrowed by the session opinion), ready to push to the constellation via
/// `set_script_bindings`. Absent file = no bindings.
pub(crate) fn load_resolved_bindings(
    mere_root: &Path,
    prefs: &ScriptPermissionPrefs,
) -> Vec<ResolvedScriptBinding> {
    let bindings = session_runtime::script_bindings_store::load_script_bindings(mere_root)
        .ok()
        .flatten()
        .unwrap_or_default();
    let policy = ScriptCapPolicy {
        log: prefs.log,
        document: prefs.document,
        net: prefs.net,
    };
    let (log, document, net) = resolve_attach_permissions(policy);
    bindings
        .into_iter()
        .map(|b| ResolvedScriptBinding {
            origin: b.origin,
            component_path: PathBuf::from(b.component_path),
            log,
            document,
            net,
        })
        .collect()
}

/// Discover "installed extension" DocumentScript mods under `<mere_root>/mods/` and
/// resolve each into auto-attach bindings (the mod-manifest form of follow-on #2,
/// alongside the user `script-bindings.json` form above). A wasm mod whose sidecar
/// manifest declares `document_script_origins` binds its own `.wasm` (the component)
/// to each listed origin glob. `log` / `document` resolve through the session `prefs`
/// like user bindings; **`net` is additionally intersected with the manifest**: a mod
/// only gets network egress if it *declares* `ModCapability::Network` AND the session
/// prefs allow it (per-mod least-privilege — the manifest is the ceiling). Discovery
/// is sorted by module path so selection is deterministic across machines, and an
/// origin already claimed by an earlier mod is dropped with a warning. Absent `mods/`
/// dir = no mods. The attach reuses the ordinary `AttachScript` path, not the
/// WasmModRuntime bridge (page scripts mirror the live page DOM, not a seeded one).
///
/// Note: dropped-in mods still auto-attach without a per-mod install/approval step —
/// that gating + signing is tracked in the net-hardening plan (§B1/E3).
pub(crate) fn load_mod_bindings(
    mere_root: &Path,
    prefs: &ScriptPermissionPrefs,
) -> Vec<ResolvedScriptBinding> {
    let policy = ScriptCapPolicy {
        log: prefs.log,
        document: prefs.document,
        net: prefs.net,
    };
    let (log, document, net) = resolve_attach_permissions(policy);
    let net_deny = ResolvedPermission {
        effective: Permission::Deny,
        decided_by: None,
    };

    let mut mods = register_mod_loader::discover_wasm_mods_in_dir(&mere_root.join("mods"));
    mods.sort_by(|a, b| a.1.module_path.cmp(&b.1.module_path));

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut bindings = Vec::new();
    for (manifest, source) in mods {
        // net only if the manifest declared it (the ceiling) and the session allows it.
        let mod_net = if manifest
            .capabilities
            .contains(&register_mod_loader::ModCapability::Network)
        {
            net
        } else {
            net_deny
        };
        for origin in manifest.document_script_origins {
            if !seen.insert(origin.clone()) {
                tracing::warn!(
                    %origin,
                    mod_id = %manifest.mod_id,
                    "mod-binding origin already claimed by an earlier mod; ignoring the duplicate",
                );
                continue;
            }
            bindings.push(ResolvedScriptBinding {
                origin,
                component_path: source.module_path.clone(),
                log,
                document,
                net: mod_net,
            });
        }
    }
    bindings
}

/// Copy `src`'s whole document tree into a fresh mutable [`ScriptedDom`]: elements
/// with their attributes, and text nodes. Comments / doctype / processing
/// instructions are skipped (they do not paint; quirks mode rides the sheets path,
/// not the mirror). The result is the script-mutable twin of a parsed
/// `StaticDocument` — the cascade sees the same element tree, so under the same
/// stylesheets it lays out identically.
pub(crate) fn mirror_to_scripted_dom<D: LayoutDom>(src: &D) -> ScriptedDom {
    let mut dst = ScriptedDom::new();
    let dst_root = dst.document();
    let src_root = src.document();
    for child in src.dom_children(src_root) {
        mirror_node(src, child, &mut dst, dst_root);
    }
    dst
}

fn mirror_node<D: LayoutDom>(
    src: &D,
    src_id: D::NodeId,
    dst: &mut ScriptedDom,
    dst_parent: NodeId,
) {
    match src.kind(src_id) {
        NodeKind::Element => {
            let Some(name) = src.element_name(src_id) else {
                return;
            };
            let el = dst.create_element(name.clone());
            for attr in src.attributes(src_id) {
                dst.set_attribute(el, attr.name.clone(), attr.value);
            }
            dst.append_child(dst_parent, el);
            for child in src.dom_children(src_id) {
                mirror_node(src, child, dst, el);
            }
        }
        NodeKind::Text => {
            let t = dst.create_text(src.text(src_id).unwrap_or(""));
            dst.append_child(dst_parent, t);
        }
        // Comment / Doctype / PI / Document / DocumentFragment: not painted; skip.
        _ => {}
    }
}

/// The page's author stylesheets (inline `<style>` + linked `<link rel=stylesheet>`,
/// the latter fetched through `loader`), owned so the [`ScriptInstance`] can re-lay
/// out the mutated DOM without re-parsing. `HTML_SHEET` (the UA-ish base) is
/// prepended at layout time, not stored.
fn page_sheets(doc: &StaticDocument, loader: &impl ImageLoader) -> Vec<String> {
    let mut sheets = inline_stylesheets(doc);
    sheets.extend(linked_stylesheets_with_loader(doc, loader));
    sheets
}

/// Lay `dom` out with `HTML_SHEET` + the page's stored author sheets.
fn lay_out_with(
    dom: &ScriptedDom,
    sheets: &[String],
    loader: &impl ImageLoader,
    w: u32,
    h: u32,
) -> ContentLayout<NodeId> {
    let mut all: Vec<&str> = HTML_SHEET.to_vec();
    all.extend(sheets.iter().map(String::as_str));
    lay_out_content(dom, &all, loader, w, h)
}

/// A DocumentScript attached to a content tile's page: the wasm instance owns the
/// live mirrored `ScriptedDom`; this holds its laid-out [`ContentLayout`] and the
/// page sheets so a script mutation re-lays-out without re-parsing. The content
/// actor renders from [`dom`](Self::dom) + [`layout`](Self::layout) through the same
/// generic `scene_from_content_band` the unscripted lane uses. Errors are
/// stringified so the content actor never names a wasmtime type.
pub(crate) struct ScriptInstance {
    script: DocumentScript,
    layout: ContentLayout<NodeId>,
    sheets: Vec<String>,
    viewport: (u32, u32),
}

impl ScriptInstance {
    /// Mirror `body` into a `ScriptedDom`, lay it out, and attach the component at
    /// `component_path` over it under `grant` + `quota` (runs `activate`). The page
    /// becomes script-mutable; unscripted tiles never reach this path.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn attach(
        component_path: &Path,
        body: &str,
        loader: &impl ImageLoader,
        w: u32,
        h: u32,
        grant: &Grant,
        quota: Quota,
        fetcher: Option<Arc<dyn NetFetcher>>,
        net_origins: Vec<String>,
    ) -> Result<Self, String> {
        let static_doc = StaticDocument::parse(body);
        let sheets = page_sheets(&static_doc, loader);
        let scripted = mirror_to_scripted_dom(&static_doc);
        let layout = lay_out_with(&scripted, &sheets, loader, w, h);
        let script =
            DocumentScript::attach(component_path, scripted, grant, quota, fetcher, net_origins)
                .map_err(|e| e.to_string())?;
        Ok(Self {
            script,
            layout,
            sheets,
            viewport: (w, h),
        })
    }

    /// Deliver one event to the script. If the script changed the DOM (its revision
    /// advanced), re-lay-out so the next render reflects the mutation.
    pub(crate) fn deliver(
        &mut self,
        kind: &str,
        payload: &str,
        loader: &impl ImageLoader,
    ) -> Result<TurnOutcome, String> {
        let before = self.script.revision();
        let outcome = self
            .script
            .deliver_event(kind, payload)
            .map_err(|e| e.to_string())?;
        if self.script.revision() != before {
            let (w, h) = self.viewport;
            self.layout = lay_out_with(self.script.dom(), &self.sheets, loader, w, h);
        }
        Ok(outcome)
    }

    /// Re-lay-out the current (script-mutated) DOM at `(w, h)` with the page's
    /// sheets — a resize, or a re-decode after a newly-arrived subresource. The
    /// loader records any newly-wanted subresources for the caller to ship.
    /// (Follow-on #3.)
    pub(crate) fn relayout(&mut self, loader: &impl ImageLoader, w: u32, h: u32) {
        self.viewport = (w, h);
        self.layout = lay_out_with(self.script.dom(), &self.sheets, loader, w, h);
    }

    /// The live (script-mutated) page DOM, for rendering.
    pub(crate) fn dom(&self) -> &ScriptedDom {
        self.script.dom()
    }

    /// The current laid-out layout, kept in sync with `dom` across mutations.
    pub(crate) fn layout(&self) -> &ContentLayout<NodeId> {
        &self.layout
    }

    /// Run the script's `deactivate` and drop it, returning the final DOM.
    pub(crate) fn detach(self) -> Result<ScriptedDom, String> {
        self.script.detach().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests;
