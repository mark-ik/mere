// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Sitemap seed sourcing for the crawl frontier.
//!
//! A site's `sitemap.xml` is the canonical, polite list of its pages — the way to
//! crawl a site comprehensively rather than only what the seed page happens to link.
//! This is a dependency-free `<loc>` scan (not a full XML parse): both `<urlset>`
//! (page lists) and `<sitemapindex>` (child-sitemap lists) carry their entries in
//! `<loc>` elements, so pulling every `<loc>` payload covers both. Recursing a
//! `<sitemapindex>` into its child sitemaps is a follow-on; today a child-sitemap URL
//! is simply enqueued like any page (it returns no `<a>` links, so it contributes
//! nothing and does not expand the crawl).

/// Extract the http(s) URLs from a sitemap.xml's `<loc>` elements, in document order.
/// Trims whitespace; skips non-http(s) and malformed entries.
pub fn parse_sitemap(xml: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let mut rest = xml;
    while let Some(open) = rest.find("<loc>") {
        let after = &rest[open + "<loc>".len()..];
        let Some(close) = after.find("</loc>") else {
            break;
        };
        let loc = after[..close].trim();
        if loc.starts_with("http://") || loc.starts_with("https://") {
            urls.push(loc.to_string());
        }
        rest = &after[close + "</loc>".len()..];
    }
    urls
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_loc_urls_from_a_urlset() {
        let xml = r#"<?xml version="1.0"?>
            <urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
              <url><loc>https://s.test/a</loc><lastmod>2026-01-01</lastmod></url>
              <url><loc> https://s.test/b </loc></url>
            </urlset>"#;
        assert_eq!(
            parse_sitemap(xml),
            vec!["https://s.test/a", "https://s.test/b"]
        );
    }

    #[test]
    fn handles_a_sitemapindex_the_same_way() {
        // A sitemapindex's <loc>s are child sitemaps; we still pull them (recursion is
        // a follow-on).
        let xml = "<sitemapindex><sitemap><loc>https://s.test/sitemap-1.xml</loc></sitemap></sitemapindex>";
        assert_eq!(parse_sitemap(xml), vec!["https://s.test/sitemap-1.xml"]);
    }

    #[test]
    fn skips_non_http_and_empty() {
        assert!(parse_sitemap("").is_empty());
        assert!(parse_sitemap("<urlset></urlset>").is_empty());
        assert!(parse_sitemap("<url><loc>ftp://s.test/x</loc></url>").is_empty());
        assert!(parse_sitemap("<url><loc></loc></url>").is_empty());
    }
}
