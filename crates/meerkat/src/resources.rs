/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Subresource cache + demand loader for the content card.
//!
//! The card renders fetched HTML through genet, which pulls `<img>` and
//! `<link rel=stylesheet>` resources by calling [`ImageLoader::load`] for each
//! URL. Network fetches are async (see [`crate::fetch`]), so the loader cannot
//! block: on a cache miss it records the resolved absolute URL as *wanted* and
//! returns `None`. After the render the host fetches every wanted URL it has
//! not already requested; each completion fills the cache and re-renders, so
//! the resource appears on the next frame. This converges — once every
//! resource has arrived or failed, a render records only already-requested URLs
//! and spawns nothing, so no further wake fires. `data:` URIs never reach the
//! loader (genet decodes them inline).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use genet_layout::ImageLoader;

/// Fetched subresource bytes keyed by absolute URL, plus the set of URLs
/// already requested (in flight or done) so the demand loader never re-spawns a
/// fetch. Shared across pages: a stylesheet two pages link resolves once.
#[derive(Default)]
pub struct ResourceStore {
    bytes: HashMap<String, Vec<u8>>,
    requested: HashSet<String>,
}

impl ResourceStore {
    /// Record fetched `bytes` for absolute `url`.
    pub fn insert(&mut self, url: String, bytes: Vec<u8>) {
        self.bytes.insert(url, bytes);
    }

    /// Mark `url` requested, returning `true` if it is newly requested (the
    /// caller should then spawn its fetch) or `false` if already in flight/done.
    pub fn request(&mut self, url: String) -> bool {
        self.requested.insert(url)
    }
}

/// A demand loader over a [`ResourceStore`], resolving relative resource URLs
/// against `base` (the page URL). Cache misses are pushed to `wanted` for the
/// host to fetch after the render.
pub struct ResourceLoader<'a> {
    store: &'a RefCell<ResourceStore>,
    base: &'a str,
    wanted: &'a RefCell<Vec<String>>,
}

impl<'a> ResourceLoader<'a> {
    pub fn new(
        store: &'a RefCell<ResourceStore>,
        base: &'a str,
        wanted: &'a RefCell<Vec<String>>,
    ) -> Self {
        Self {
            store,
            base,
            wanted,
        }
    }

    /// Resolve `url` (possibly relative) against the page `base`.
    fn resolve(&self, url: &str) -> Option<String> {
        url::Url::parse(self.base)
            .ok()?
            .join(url)
            .ok()
            .map(|u| u.to_string())
    }
}

impl ImageLoader for ResourceLoader<'_> {
    fn load(&self, url: &str) -> Option<Vec<u8>> {
        let abs = self.resolve(url)?;
        if let Some(bytes) = self.store.borrow().bytes.get(&abs) {
            return Some(bytes.clone());
        }
        self.wanted.borrow_mut().push(abs);
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn miss_resolves_against_the_page_and_records_wanted() {
        let store = RefCell::new(ResourceStore::default());
        let wanted = RefCell::new(Vec::new());
        let loader = ResourceLoader::new(&store, "https://example.com/dir/page.html", &wanted);

        assert_eq!(loader.load("../style.css"), None, "a miss yields no bytes");
        assert_eq!(
            wanted.borrow().as_slice(),
            ["https://example.com/style.css".to_string()],
            "the relative URL resolves against the page and is recorded for fetching",
        );
    }

    #[test]
    fn hit_returns_cached_bytes_and_records_nothing() {
        let store = RefCell::new(ResourceStore::default());
        store
            .borrow_mut()
            .insert("https://example.com/a.css".into(), b"x{}".to_vec());
        let wanted = RefCell::new(Vec::new());
        let loader = ResourceLoader::new(&store, "https://example.com/", &wanted);

        assert_eq!(
            loader.load("a.css"),
            Some(b"x{}".to_vec()),
            "a cached resource loads"
        );
        assert!(wanted.borrow().is_empty(), "a hit records nothing to fetch");
    }

    #[test]
    fn request_dedups() {
        let mut store = ResourceStore::default();
        assert!(
            store.request("https://x/a".into()),
            "first request is newly spawned"
        );
        assert!(
            !store.request("https://x/a".into()),
            "a duplicate is suppressed"
        );
    }
}
