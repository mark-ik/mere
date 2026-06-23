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

use std::path::Path;

use document_host::{CapPermission, DocumentScript, Grant, Quota, TurnOutcome};
use kernel::permissions::{Permission, ResolvedPermission};
use layout_dom_api::{LayoutDom, LayoutDomMut, NodeKind};
use serval_layout::{
    inline_stylesheets, lay_out_content, linked_stylesheets_with_loader, ContentLayout, ImageLoader,
};
use serval_scripted_dom::{NodeId, ScriptedDom};
use serval_static_dom::StaticDocument;

use crate::card::HTML_SHEET;

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

/// Build a document-host [`Grant`] from the host-resolved permissions for the two
/// application capabilities the `document-core` world exposes (`log`, `document`).
/// The `kernel::permissions` -> `Grant` seam (§11.4): the grant *policy* lives in
/// document-host, the five-scope *resolution* is the host's input (the content actor
/// resolves the scope chain and passes the effective opinion in). A `Deny`/`Prompt`
/// on a capability the component requires makes instantiation fail — the boundary.
pub(crate) fn grant_from_resolved(log: ResolvedPermission, document: ResolvedPermission) -> Grant {
    Grant { log: cap_permission(log), document: cap_permission(document) }
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
    pub(crate) fn attach(
        component_path: &Path,
        body: &str,
        loader: &impl ImageLoader,
        w: u32,
        h: u32,
        grant: &Grant,
        quota: Quota,
    ) -> Result<Self, String> {
        let static_doc = StaticDocument::parse(body);
        let sheets = page_sheets(&static_doc, loader);
        let scripted = mirror_to_scripted_dom(&static_doc);
        let layout = lay_out_with(&scripted, &sheets, loader, w, h);
        let script = DocumentScript::attach(component_path, scripted, grant, quota)
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

        let g = grant_from_resolved(allow, allow);
        assert_eq!(g.log, CapPermission::Allow);
        assert_eq!(g.document, CapPermission::Allow);

        // A denied document capability maps to Deny -> the import is omitted, so a
        // component that requires it fails to instantiate (the enforced boundary).
        assert_eq!(grant_from_resolved(allow, deny).document, CapPermission::Deny);
        // Prompt is preserved (P2 omits it conservatively, like Deny, at link time).
        assert_eq!(grant_from_resolved(allow, prompt).document, CapPermission::Prompt);
    }
}
