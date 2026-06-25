/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Minimal robots.txt honoring for the crawl frontier.
//!
//! A from-scratch implementation of the de-facto robots.txt subset crawlers honor:
//! `User-agent` groups, `Allow` / `Disallow` path rules, longest-match wins (an
//! `Allow` breaking a tie). Wildcards (`*`, `$`) are a follow-on; today rules match
//! as path **prefixes**, which covers the overwhelming majority of real robots.txt
//! (`Disallow: /admin/`). A missing / unparseable / empty robots.txt allows
//! everything, per spec. Technique borrowed from Google's robots.txt spec; kept
//! dependency-free to hold the crawler's dep graph minimal.

/// The crawler's user-agent token, matched against `User-agent:` groups. The same
/// `merebot` token rides the descriptive UA sent on crawl fetches
/// ([`fetch::CRAWLER_USER_AGENT`](crate::fetch)).
pub const CRAWLER_UA: &str = "merebot";

/// Parsed robots.txt rules for one host: the `Allow` / `Disallow` path rules from the
/// most specific matching `User-agent` group (our UA, else `*`). Empty = allow all.
pub struct RobotsRules {
    /// `(is_allow, path_prefix)` in file order; matched longest-prefix-wins.
    rules: Vec<(bool, String)>,
}

impl RobotsRules {
    /// Parse robots.txt `text`, selecting the rules for [`CRAWLER_UA`] and falling
    /// back to the `*` group when our UA declares none. Comments (`#…`), blank lines,
    /// and unknown directives are ignored.
    pub fn parse(text: &str) -> Self {
        let mut star: Vec<(bool, String)> = Vec::new();
        let mut ours: Vec<(bool, String)> = Vec::new();
        let mut group_uas: Vec<String> = Vec::new();
        let mut seen_rule = false;
        for raw in text.lines() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim();
            match key.as_str() {
                "user-agent" => {
                    // A User-agent after a rule starts a fresh group; consecutive
                    // User-agent lines (no rule between) accumulate into one group.
                    if seen_rule {
                        group_uas.clear();
                        seen_rule = false;
                    }
                    group_uas.push(value.to_ascii_lowercase());
                },
                "disallow" | "allow" => {
                    seen_rule = true;
                    if value.is_empty() {
                        continue; // an empty value imposes no restriction
                    }
                    let rule = (key == "allow", value.to_string());
                    if group_uas.iter().any(|u| u == "*") {
                        star.push(rule.clone());
                    }
                    if group_uas.iter().any(|u| u == CRAWLER_UA) {
                        ours.push(rule);
                    }
                },
                _ => {},
            }
        }
        let rules = if ours.is_empty() { star } else { ours };
        Self { rules }
    }

    /// Whether `path` (a URL path like `/foo/bar`) may be fetched. The longest matching
    /// rule wins; an `Allow` beats a `Disallow` of equal length; no match = allowed.
    pub fn allows(&self, path: &str) -> bool {
        let mut best: Option<(usize, bool)> = None; // (matched prefix len, is_allow)
        for (is_allow, prefix) in &self.rules {
            if path.starts_with(prefix.as_str()) {
                let len = prefix.len();
                let better = match best {
                    None => true,
                    Some((blen, ballow)) => len > blen || (len == blen && *is_allow && !ballow),
                };
                if better {
                    best = Some((len, *is_allow));
                }
            }
        }
        best.map(|(_, allow)| allow).unwrap_or(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_or_comment_only_allows_everything() {
        assert!(RobotsRules::parse("").allows("/anything"));
        assert!(RobotsRules::parse("# just a comment\n").allows("/x"));
    }

    #[test]
    fn disallow_blocks_its_prefix() {
        let r = RobotsRules::parse("User-agent: *\nDisallow: /admin/");
        assert!(!r.allows("/admin/users"));
        assert!(r.allows("/public"));
    }

    #[test]
    fn empty_disallow_is_allow_all() {
        let r = RobotsRules::parse("User-agent: *\nDisallow:");
        assert!(r.allows("/anything"));
    }

    #[test]
    fn disallow_root_blocks_all() {
        let r = RobotsRules::parse("User-agent: *\nDisallow: /");
        assert!(!r.allows("/anything"));
        assert!(!r.allows("/"));
    }

    #[test]
    fn allow_overrides_disallow_on_a_longer_match() {
        let r = RobotsRules::parse("User-agent: *\nDisallow: /a\nAllow: /a/b");
        assert!(!r.allows("/a/x"), "the shorter Disallow blocks");
        assert!(r.allows("/a/b/c"), "the longer Allow wins");
    }

    #[test]
    fn our_ua_group_overrides_the_star_group() {
        // `*` disallows everything; our group narrows it to just /secret.
        let txt = "User-agent: *\nDisallow: /\n\nUser-agent: merebot\nDisallow: /secret";
        let r = RobotsRules::parse(txt);
        assert!(r.allows("/public"), "our UA ignores the * group's Disallow: /");
        assert!(!r.allows("/secret/x"));
    }

    #[test]
    fn comments_and_casing_are_tolerated() {
        let r = RobotsRules::parse("USER-AGENT: *  # any bot\nDISALLOW: /tmp  # temp");
        assert!(!r.allows("/tmp/file"));
        assert!(r.allows("/keep"));
    }
}
