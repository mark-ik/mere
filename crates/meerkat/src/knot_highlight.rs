/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The bridge from illume's lexer spans to serval's styled field.
//!
//! Maps each [`illume::SyntaxKind`] onto a tinct [`SyntaxRole`], names the role's
//! CSS class, and produces the [`StyleRange`]s the styled field paints. The colours
//! for those classes are derived by tinct (`derive_syntax_palette`) and registered
//! as a host stylesheet; this module owns only the kind → role → class half, which
//! is the seam that keeps illume and tinct independent of each other.

use illume::{default_pack, entities, highlight, Span, SyntaxKind};
use tincture::{derive_syntax_palette, Seeds, Srgb, SyntaxRole};
use xilem_serval::StyleRange;

/// Map an illume lexer kind onto a tinct highlight role. `None` for kinds that
/// carry no colour of their own: a code/raw block region (its inner tokens are
/// coloured instead), bare identifiers, and the few structural kinds without a role.
fn syntax_role(kind: SyntaxKind) -> Option<SyntaxRole> {
    use SyntaxKind as K;
    use SyntaxRole as R;
    Some(match kind {
        K::Heading => R::Heading,
        K::Emphasis => R::Emphasis,
        K::Strong => R::Strong,
        K::Verbatim => R::Verbatim,
        K::Link | K::Image => R::Link,
        K::Blockquote => R::Quote,
        K::Keyword => R::Keyword,
        K::StringLit => R::String,
        K::Number => R::Number,
        K::Comment => R::Comment,
        K::Function => R::Function,
        K::Type => R::Type,
        K::Punctuation => R::Punctuation,
        K::Url | K::Email => R::Url,
        K::Mention => R::Mention,
        K::Tag => R::Tag,
        K::Strikethrough
        | K::Mark
        | K::Math
        | K::CodeBlock
        | K::RawBlock
        | K::Div
        | K::Identifier => return None,
    })
}

/// The CSS class a host stylesheet themes for `role` (`SyntaxRole::Keyword` →
/// `"syntax-keyword"`). The colours come from tinct's `derive_syntax_palette`.
pub fn role_class(role: SyntaxRole) -> &'static str {
    use SyntaxRole as R;
    match role {
        R::Heading => "syntax-heading",
        R::Emphasis => "syntax-emphasis",
        R::Strong => "syntax-strong",
        R::Link => "syntax-link",
        R::Quote => "syntax-quote",
        R::Verbatim => "syntax-verbatim",
        R::Keyword => "syntax-keyword",
        R::Type => "syntax-type",
        R::Function => "syntax-function",
        R::String => "syntax-string",
        R::Number => "syntax-number",
        R::Comment => "syntax-comment",
        R::Punctuation => "syntax-punctuation",
        R::Url => "syntax-url",
        R::Mention => "syntax-mention",
        R::Tag => "syntax-tag",
    }
}

/// The styled ranges for `text`: djot structure + polyglot injection (illume's
/// default pack) plus inline prose entities (urls, mentions, tags), each mapped to
/// its themed class. The styled field paints these; unmapped kinds are skipped.
pub fn knot_styles(text: &str) -> Vec<StyleRange> {
    let registry = default_pack();
    let mut styles = Vec::new();
    let mut push = |span: Span| {
        if let Some(role) = syntax_role(span.kind) {
            styles.push(StyleRange {
                range: span.range,
                class: role_class(role).to_string(),
            });
        }
    };
    for span in highlight(text, &registry) {
        push(span);
    }
    for span in entities(text) {
        push(span);
    }
    styles
}

/// The styled ranges for an omnibar or other single-line surface: only the inline
/// entities (urls, mentions, tags, emails) mapped to their classes. Unlike
/// [`knot_styles`] it runs no djot structure pass (the omnibar is not a note).
pub fn omnibar_styles(text: &str) -> Vec<StyleRange> {
    entities(text)
        .into_iter()
        .filter_map(|span| {
            syntax_role(span.kind).map(|role| StyleRange {
                range: span.range,
                class: role_class(role).to_string(),
            })
        })
        .collect()
}

/// The seeds the syntax palette falls back to when the active theme's seeds are
/// unavailable (a brand-coherent dark triad, matching the default chrome).
pub fn fallback_seeds() -> Seeds {
    Seeds {
        primary: Srgb::rgb(0x33, 0x66, 0xC8),
        secondary: Srgb::rgb(0x2E, 0x9D, 0xA6),
        tertiary: Srgb::rgb(0xE0, 0xA8, 0x46),
        neutral: Srgb::rgb(0x10, 0x14, 0x22),
        text_header: None,
        text_body: None,
        success: Srgb::rgb(0x4F, 0xB3, 0x6E),
        danger: Srgb::rgb(0xD5, 0x4E, 0x4E),
        dark: true,
    }
}

/// CSS rules colouring the `syntax-*` classes from tinct's derived syntax palette,
/// one rule per role, for the chrome stylesheet. Derived from the active theme's
/// `seeds` (perceptual, contrast-gated against the surface), so the syntax colours
/// track a theme switch; the styled field's spans carry the classes these rules theme.
pub fn syntax_css(seeds: &Seeds) -> Vec<String> {
    let palette = derive_syntax_palette(seeds);
    SyntaxRole::ALL
        .iter()
        .map(|&role| {
            let c = palette.role(role);
            format!(
                ".{} {{ color: rgb({}, {}, {}); }}",
                role_class(role),
                c.r,
                c.g,
                c.b
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_structure_and_entities() {
        let styles = knot_styles("# Title\n\nsee @ada and https://ex.com\n");
        let classes: Vec<&str> = styles.iter().map(|s| s.class.as_str()).collect();
        assert!(classes.contains(&"syntax-heading"), "{classes:?}");
        assert!(classes.contains(&"syntax-mention"), "{classes:?}");
        assert!(classes.contains(&"syntax-url"), "{classes:?}");
    }

    #[test]
    fn every_role_has_a_distinct_class() {
        let mut seen = std::collections::HashSet::new();
        for role in SyntaxRole::ALL {
            assert!(seen.insert(role_class(role)), "duplicate class for {role:?}");
        }
    }
}
