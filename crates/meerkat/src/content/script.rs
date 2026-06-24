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

use document_host::{CapPermission, DocumentScript, Grant, NetFetcher, NetResponse, Quota, TurnOutcome};
use session_runtime::settings_store::ScriptPermissionPrefs;
use kernel::permissions::{
    resolve_permission, Permission, ResolvedPermission, ScopedPermission, SettingScope,
};
use layout_dom_api::{LayoutDom, LayoutDomMut, NodeKind};
use serval_layout::{
    inline_stylesheets, lay_out_content, linked_stylesheets_with_loader, ContentLayout, ImageLoader,
};
use serval_scripted_dom::{NodeId, ScriptedDom};
use serval_static_dom::StaticDocument;

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
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
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
                return Err(format!("net.fetch: timed out after {}s", NET_FETCH_TIMEOUT.as_secs()))
            }
        };
        Ok(NetResponse { status: 200, content_type: fetched.content_type, body: fetched.body })
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
    let host_port = authority.rsplit_once('@').map(|(_, h)| h).unwrap_or(authority);
    // Drop the port. For an IPv6 literal ([..]:port) take the bracketed host; the
    // ':' inside the address must not be read as a port separator.
    let host = if let Some(rest) = host_port.strip_prefix('[') {
        rest.split_once(']').map(|(inner, _)| inner).unwrap_or(host_port)
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
    let policy = ScriptCapPolicy { log: prefs.log, document: prefs.document, net: prefs.net };
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
    let policy = ScriptCapPolicy { log: prefs.log, document: prefs.document, net: prefs.net };
    let (log, document, net) = resolve_attach_permissions(policy);
    let net_deny = ResolvedPermission { effective: Permission::Deny, decided_by: None };

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

fn mirror_node<D: LayoutDom>(src: &D, src_id: D::NodeId, dst: &mut ScriptedDom, dst_parent: NodeId) {
    match src.kind(src_id) {
        NodeKind::Element => {
            let Some(name) = src.element_name(src_id) else { return };
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
        Ok(Self { script, layout, sheets, viewport: (w, h) })
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
        let outcome = self.script.deliver_event(kind, payload).map_err(|e| e.to_string())?;
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
mod tests {
    use super::*;

    fn find_element(dom: &ScriptedDom, id: NodeId, tag: &str) -> Option<NodeId> {
        if dom.kind(id) == NodeKind::Element
            && dom.element_name(id).map(|q| q.local.to_string()).as_deref() == Some(tag)
        {
            return Some(id);
        }
        for child in dom.dom_children(id) {
            if let Some(found) = find_element(dom, child, tag) {
                return Some(found);
            }
        }
        None
    }

    #[test]
    fn mirror_preserves_elements_attributes_and_text() {
        let doc = StaticDocument::parse("<body><p class=\"intro\">Hello</p></body>");
        let dom = mirror_to_scripted_dom(&doc);

        let p = find_element(&dom, dom.document(), "p").expect("mirror keeps the <p>");
        assert_eq!(
            dom.attribute(p, &layout_dom_api::Namespace::from(""), &layout_dom_api::LocalName::from("class")),
            Some("intro"),
            "the class attribute is carried into the mirror",
        );
        let text = dom.dom_children(p).next().expect("the <p> has a text child");
        assert_eq!(dom.text(text), Some("Hello"), "the text node is carried into the mirror");
    }

    #[test]
    fn grant_maps_resolved_permissions() {
        use kernel::permissions::SettingScope;
        let allow = ResolvedPermission { effective: Permission::Allow, decided_by: None };
        let deny =
            ResolvedPermission { effective: Permission::Deny, decided_by: Some(SettingScope::Surface) };
        let prompt = ResolvedPermission { effective: Permission::Prompt, decided_by: None };

        let g = grant_from_resolved(allow, allow, allow);
        assert_eq!(g.log, CapPermission::Allow);
        assert_eq!(g.document, CapPermission::Allow);
        assert_eq!(g.net, CapPermission::Allow);

        // A denied document capability maps to Deny -> the import is omitted, so a
        // component that requires it fails to instantiate (the enforced boundary).
        assert_eq!(grant_from_resolved(allow, deny, allow).document, CapPermission::Deny);
        // Prompt is preserved (P2 omits it conservatively, like Deny, at link time).
        assert_eq!(grant_from_resolved(allow, prompt, allow).document, CapPermission::Prompt);
        // net maps independently (a denied net omits the import).
        assert_eq!(grant_from_resolved(allow, allow, deny).net, CapPermission::Deny);
    }

    #[test]
    fn attach_permissions_resolve_with_narrowing() {
        // No Session override: log/document default Allow; net defaults Deny (powerful).
        let (log, document, net) = resolve_attach_permissions(ScriptCapPolicy::default());
        assert_eq!(log.effective, Permission::Allow);
        assert_eq!(document.effective, Permission::Allow);
        assert_eq!(net.effective, Permission::Deny, "net is denied unless granted");

        // A Session-scope Deny on `document` narrows past the Allow baseline (the
        // narrowing rule); the grant then omits the document import — a session-wide
        // "no script may touch the page" switch.
        let policy = ScriptCapPolicy { document: Some(Permission::Deny), ..Default::default() };
        let (_log, document, _net) = resolve_attach_permissions(policy);
        assert_eq!(document.effective, Permission::Deny);
        assert_eq!(document.decided_by, Some(SettingScope::Session));

        // A Session-scope Allow on `net` GRANTS it over the Deny baseline — the
        // loop-closing case: a user opts a script into network egress. (This is why
        // the baseline is the resolve default, not an explicit App Deny — an App Deny
        // could never be granted back by a narrower scope.)
        let policy = ScriptCapPolicy { net: Some(Permission::Allow), ..Default::default() };
        let (log, _doc, net) = resolve_attach_permissions(policy);
        assert_eq!(net.effective, Permission::Allow, "a session grant flips net on");
        assert_eq!(grant_from_resolved(log, _doc, net).net, CapPermission::Allow);
    }

    #[test]
    fn origin_matching_handles_exact_host_and_suffix_glob() {
        assert!(origin_matches("https://example.com/path", "example.com"));
        assert!(!origin_matches("https://evil.com/", "example.com"));
        // `*.` matches the apex and any subdomain, not an unrelated host.
        assert!(origin_matches("https://en.wikipedia.org/wiki/X", "*.wikipedia.org"));
        assert!(origin_matches("https://wikipedia.org/", "*.wikipedia.org"));
        assert!(!origin_matches("https://notwikipedia.org/", "*.wikipedia.org"));
        // Host extraction drops scheme / path / query / fragment (any scheme).
        assert!(origin_matches("gemini://example.com/cap?q#f", "example.com"));
        // Userinfo + port + case are normalized away, so a non-default port or a
        // cased host still matches (and userinfo cannot smuggle a different host).
        assert!(origin_matches("https://example.com:8443/", "example.com"));
        assert!(origin_matches("https://EXAMPLE.com/", "example.com"));
        assert!(origin_matches("https://x.wikipedia.org:8443/", "*.wikipedia.org"));
        assert!(!origin_matches("https://example.com@evil.com/", "example.com"));
    }

    #[test]
    fn ssrf_floor_blocks_loopback_and_private_hosts() {
        assert!(is_disallowed_fetch_host("http://localhost/"));
        assert!(is_disallowed_fetch_host("http://127.0.0.1/x"));
        assert!(is_disallowed_fetch_host("http://10.0.0.5/"));
        assert!(is_disallowed_fetch_host("http://192.168.1.1/"));
        assert!(is_disallowed_fetch_host("http://169.254.1.1/"));
        assert!(is_disallowed_fetch_host("http://[::1]/"));
        assert!(is_disallowed_fetch_host("http://100.64.0.1/"));
        // Public hosts (name or IP) are allowed through.
        assert!(!is_disallowed_fetch_host("https://example.com/"));
        assert!(!is_disallowed_fetch_host("https://1.1.1.1/"));
    }

    #[test]
    fn binding_for_finds_the_matching_origin() {
        let allow = ResolvedPermission { effective: Permission::Allow, decided_by: None };
        let bindings = vec![
            ResolvedScriptBinding {
                origin: "example.com".into(),
                component_path: PathBuf::from("a.wasm"),
                log: allow,
                document: allow,
                net: allow,
            },
            ResolvedScriptBinding {
                origin: "*.wikipedia.org".into(),
                component_path: PathBuf::from("w.wasm"),
                log: allow,
                document: allow,
                net: allow,
            },
        ];
        assert_eq!(
            binding_for("https://en.wikipedia.org/x", &bindings).map(|b| b.component_path.clone()),
            Some(PathBuf::from("w.wasm")),
        );
        assert!(binding_for("https://other.net/", &bindings).is_none());
    }

    #[test]
    fn load_mod_bindings_derives_bindings_from_installed_mods() {
        let mere_root = tempfile::tempdir().expect("temp mere_root");
        let mods_dir = mere_root.path().join("mods");
        std::fs::create_dir_all(&mods_dir).expect("mods dir");
        // A valid component preamble (\0asm + version/layer), required by the
        // tightened validate_wasm_binary; a bare core-module header is rejected.
        let wasm = [0x00, 0x61, 0x73, 0x6d, 0x0d, 0x00, 0x01, 0x00];
        // A net-declaring mod (two origins), a document-only mod (no Network), and a
        // plain mod with no document-script declaration (contributes nothing).
        std::fs::write(mods_dir.join("weather.wasm"), wasm).unwrap();
        std::fs::write(
            mods_dir.join("weather.wasm.toml"),
            "mod_id = \"mod:weather\"\ncapabilities = [\"network\"]\ndocument_script_origins = [\"example.com\", \"*.weather.test\"]\n",
        )
        .unwrap();
        std::fs::write(mods_dir.join("reader.wasm"), wasm).unwrap();
        std::fs::write(
            mods_dir.join("reader.wasm.toml"),
            "mod_id = \"mod:reader\"\ndocument_script_origins = [\"reader.example\"]\n",
        )
        .unwrap();
        std::fs::write(mods_dir.join("plain.wasm"), wasm).unwrap();
        std::fs::write(mods_dir.join("plain.wasm.toml"), "mod_id = \"mod:plain\"\n").unwrap();

        // net allowed this session -> only a mod that *declares* Network may fetch.
        let prefs = ScriptPermissionPrefs { log: None, document: None, net: Some(Permission::Allow) };
        let bindings = load_mod_bindings(mere_root.path(), &prefs);

        assert_eq!(bindings.len(), 3, "weather's 2 origins + reader's 1; plain adds none");
        let example = bindings
            .iter()
            .find(|b| b.origin == "example.com")
            .expect("example.com bound");
        assert!(
            example.component_path.ends_with("weather.wasm"),
            "the binding points at the mod's own wasm component",
        );
        assert_eq!(
            example.net.effective,
            Permission::Allow,
            "weather declares Network + session allows -> net granted",
        );
        // The reader mod never declared Network, so net is denied even though the
        // session pref is Allow (per-mod least-privilege; the manifest is the ceiling).
        let reader = bindings
            .iter()
            .find(|b| b.origin == "reader.example")
            .expect("reader.example bound");
        assert_eq!(
            reader.net.effective,
            Permission::Deny,
            "a mod without a Network capability cannot fetch regardless of session prefs",
        );
        // A matching navigation routes to the mod's component via the shared lookup.
        assert_eq!(
            binding_for("https://sub.weather.test/x", &bindings).map(|b| b.component_path.clone()),
            Some(example.component_path.clone()),
        );
    }

    #[test]
    fn load_mod_bindings_empty_without_a_mods_dir() {
        let mere_root = tempfile::tempdir().expect("temp mere_root");
        let prefs = ScriptPermissionPrefs::default();
        assert!(
            load_mod_bindings(mere_root.path(), &prefs).is_empty(),
            "no mods/ dir = no mod bindings",
        );
    }
}
