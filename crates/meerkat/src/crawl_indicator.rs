/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The chrome's crawl-progress view-model (relational-browse V2 controls).
//!
//! A small, host-neutral projection of the crawl actor's progress, rendered as a chip
//! in the toolbar next to the sync chip. The host folds [`crawl::CrawlSession`]'s
//! progress into it each frame the crawl drains; the chip is empty (hidden via CSS)
//! when no crawl has run.

/// A chrome-side snapshot of the crawl's progress, projected from
/// [`crawl::CrawlSession::progress`](crate-internal). Empty by default.
#[derive(Clone, Debug, Default)]
pub struct CrawlIndicator {
    /// Whether a crawl is currently running.
    pub running: bool,
    /// Pages fetched so far (or by the last crawl).
    pub fetched: usize,
}

impl CrawlIndicator {
    /// The chip text: empty before any crawl (so the chip is hidden), `crawling: N
    /// pages` while one runs, then `crawled: N pages` once it finishes (the result
    /// lingers until the next crawl starts).
    pub fn summary(&self) -> String {
        if !self.running && self.fetched == 0 {
            return String::new();
        }
        let verb = if self.running { "crawling" } else { "crawled" };
        format!("{verb}: {} pages", self.fetched)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_reflects_the_crawl_state() {
        assert_eq!(
            CrawlIndicator::default().summary(),
            "",
            "idle, never run -> hidden"
        );
        assert_eq!(
            CrawlIndicator {
                running: true,
                fetched: 3
            }
            .summary(),
            "crawling: 3 pages",
        );
        assert_eq!(
            CrawlIndicator {
                running: false,
                fetched: 12
            }
            .summary(),
            "crawled: 12 pages",
            "the finished result lingers",
        );
    }
}
