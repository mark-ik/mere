/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The **content root** — a document authority distinct from the chrome.
//!
//! The chrome root is a Xilem view tree diffed into one [`ScriptedDom`] by the
//! runner. The content root is a *separate* [`ScriptedDom`] built here from the
//! navigated location and rendered into its own region of the window; neither
//! root sees the other's tree (the separate-roots discipline from meerkat's
//! first commit). The shell rebuilds this DOM whenever
//! [`content_location`](crate::Chrome::content_location) changes.
//!
//! Today the content is *synthesized*: `mere://welcome` is a built-in page, and
//! any other location renders a small page echoing the URL. The real content
//! engine (fetch + parse + a live script/layout pipeline) is the next slice;
//! this module is the seam it slots into — the shell already composites a second
//! authority below the chrome, so wiring an engine is swapping what fills it.

use layout_dom_api::{LayoutDom, LayoutDomMut};
use serval_scripted_dom::ScriptedDom;

/// Build the content-root [`ScriptedDom`] for `location`.
///
/// Parses synthesized HTML for the location into a fresh document via
/// [`ScriptedDom::set_inner_html`](layout_dom_api::LayoutDomMut::set_inner_html),
/// so the content root is a normal serval DOM the render path treats exactly
/// like the chrome DOM.
pub fn build_content_dom(location: &str) -> ScriptedDom {
    let mut dom = ScriptedDom::new();
    let root = dom.document();
    dom.set_inner_html(root, &content_html(location));
    dom
}

/// The synthesized HTML for `location`: the built-in welcome page, or a page
/// echoing any other navigated URL.
fn content_html(location: &str) -> String {
    if location == "mere://welcome" {
        WELCOME_HTML.to_string()
    } else {
        format!(
            "<div class=\"page\">\
               <h1 class=\"loc\">{loc}</h1>\
               <p class=\"note\">This is the content root showing the navigated \
                location. A live page engine (fetch and parse) is not wired yet; \
                the chrome above is fully interactive.</p>\
             </div>",
            loc = escape_html(location)
        )
    }
}

/// The built-in `mere://welcome` page.
const WELCOME_HTML: &str = "<div class=\"page\">\
       <h1>Mere</h1>\
       <p class=\"lead\">A graph-shaped browser, hosted on serval.</p>\
       <p>Type a URL or a search above and press Enter. Use Back and Forward to \
        move through history.</p>\
     </div>";

/// Escape the five characters that are unsafe in HTML element/text content, so a
/// navigated URL (which can carry `&`, `?`, quotes) renders as literal text.
fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serval_scripted_dom::NodeId;

    /// Collect the text of every text node in the subtree at `id`.
    fn all_text(dom: &ScriptedDom, id: NodeId) -> String {
        let mut acc = dom.text(id).unwrap_or("").to_string();
        for c in dom.dom_children(id) {
            acc.push_str(&all_text(dom, c));
        }
        acc
    }

    /// Does an element with local tag `tag` exist in the subtree at `id`?
    fn has_tag(dom: &ScriptedDom, id: NodeId, tag: &str) -> bool {
        dom.element_name(id).is_some_and(|q| q.local.as_ref() == tag)
            || dom.dom_children(id).any(|c| has_tag(dom, c, tag))
    }

    #[test]
    fn welcome_page_has_a_heading() {
        let dom = build_content_dom("mere://welcome");
        let root = dom.document();
        assert!(has_tag(&dom, root, "h1"), "welcome page renders an <h1>");
        assert!(all_text(&dom, root).contains("Mere"));
    }

    #[test]
    fn other_location_echoes_the_url() {
        let dom = build_content_dom("https://example.com/path");
        let root = dom.document();
        assert!(
            all_text(&dom, root).contains("https://example.com/path"),
            "content root echoes the navigated URL"
        );
    }

    #[test]
    fn url_with_ampersand_is_escaped_but_renders_literally() {
        // The raw HTML must escape `&`, yet the parsed text node must read back
        // as the literal URL (html5ever decodes the entity on parse).
        let raw = content_html("https://e.com/?a=1&b=2");
        assert!(raw.contains("&amp;"), "raw HTML escapes the ampersand");
        let dom = build_content_dom("https://e.com/?a=1&b=2");
        let root = dom.document();
        assert!(all_text(&dom, root).contains("https://e.com/?a=1&b=2"));
    }
}
