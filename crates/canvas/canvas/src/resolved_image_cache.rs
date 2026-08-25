// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Byte-bounded LRU for decoded, content-addressed node imagery.

use std::collections::HashMap;

/// Default decoded-image working set: 64 MiB.
///
/// Hosts can replace this through
/// [`Canvas::set_resolved_image_cache_limit_bytes`](crate::Canvas::set_resolved_image_cache_limit_bytes).
pub const DEFAULT_RESOLVED_IMAGE_CACHE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug)]
struct Entry {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    touched: u64,
}

/// The decoded working set. Encoded blobs remain durable in the host store;
/// eviction only means the host will be asked to resolve the digest again if
/// it becomes visible.
#[derive(Debug)]
pub(crate) struct ResolvedImageCache {
    entries: HashMap<[u8; 32], Entry>,
    used_bytes: usize,
    limit_bytes: usize,
    clock: u64,
}

impl Default for ResolvedImageCache {
    fn default() -> Self {
        Self::new(DEFAULT_RESOLVED_IMAGE_CACHE_BYTES)
    }
}

impl ResolvedImageCache {
    pub(crate) fn new(limit_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            used_bytes: 0,
            limit_bytes,
            clock: 0,
        }
    }

    pub(crate) fn contains(&self, digest: &[u8; 32]) -> bool {
        self.entries.contains_key(digest)
    }

    /// Bump the LRU clock for one entry and report whether it was resident.
    ///
    /// Reading is split from touching so callers can decide (dimensions, empty
    /// pixels) *before* paying for a copy: a combined `get` needs `&mut self`
    /// for the clock, which forces it to hand back owned pixels — a full
    /// buffer clone per image per frame, even for entries about to be
    /// discarded. Pair this with [`peek`](Self::peek).
    pub(crate) fn touch(&mut self, digest: &[u8; 32]) -> bool {
        self.clock = self.clock.wrapping_add(1);
        match self.entries.get_mut(digest) {
            Some(entry) => {
                entry.touched = self.clock;
                true
            }
            None => false,
        }
    }

    /// Borrow one entry's pixels without disturbing the LRU order.
    pub(crate) fn peek(&self, digest: &[u8; 32]) -> Option<(&[u8], u32, u32)> {
        let entry = self.entries.get(digest)?;
        Some((entry.rgba.as_slice(), entry.width, entry.height))
    }

    /// Insert decoded pixels and return every digest evicted to honor the
    /// bound. An image larger than the entire bound is not retained.
    pub(crate) fn insert(
        &mut self,
        digest: [u8; 32],
        rgba: Vec<u8>,
        width: u32,
        height: u32,
    ) -> Vec<[u8; 32]> {
        let mut evicted = Vec::new();
        if let Some(prior) = self.entries.remove(&digest) {
            self.used_bytes = self.used_bytes.saturating_sub(prior.rgba.len());
        }
        if rgba.len() > self.limit_bytes {
            return evicted;
        }

        self.clock = self.clock.wrapping_add(1);
        self.used_bytes = self.used_bytes.saturating_add(rgba.len());
        self.entries.insert(
            digest,
            Entry {
                rgba,
                width,
                height,
                touched: self.clock,
            },
        );
        self.evict_to_limit(&mut evicted);
        evicted
    }

    pub(crate) fn set_limit(&mut self, limit_bytes: usize) -> Vec<[u8; 32]> {
        self.limit_bytes = limit_bytes;
        let mut evicted = Vec::new();
        self.evict_to_limit(&mut evicted);
        evicted
    }

    pub(crate) fn limit_bytes(&self) -> usize {
        self.limit_bytes
    }

    pub(crate) fn used_bytes(&self) -> usize {
        self.used_bytes
    }

    fn evict_to_limit(&mut self, evicted: &mut Vec<[u8; 32]>) {
        while self.used_bytes > self.limit_bytes {
            let Some(digest) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.touched)
                .map(|(digest, _)| *digest)
            else {
                break;
            };
            if let Some(entry) = self.entries.remove(&digest) {
                self.used_bytes = self.used_bytes.saturating_sub(entry.rgba.len());
                evicted.push(digest);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_the_least_recently_used_entry_under_the_byte_bound() {
        let mut cache = ResolvedImageCache::new(8);
        let a = [1; 32];
        let b = [2; 32];
        let c = [3; 32];
        assert!(cache.insert(a, vec![1; 4], 1, 1).is_empty());
        assert!(cache.insert(b, vec![2; 4], 1, 1).is_empty());
        assert!(cache.touch(&a), "a becomes most recent");

        assert_eq!(cache.insert(c, vec![3; 4], 1, 1), vec![b]);
        assert!(cache.contains(&a));
        assert!(!cache.contains(&b));
        assert!(cache.contains(&c));
        assert_eq!(cache.used_bytes(), 8);
    }

    #[test]
    fn shrinking_the_limit_evicts_and_an_oversize_entry_is_not_retained() {
        let mut cache = ResolvedImageCache::new(8);
        let a = [1; 32];
        let b = [2; 32];
        cache.insert(a, vec![1; 4], 1, 1);
        cache.insert(b, vec![2; 4], 1, 1);
        assert_eq!(cache.set_limit(4), vec![a]);
        assert_eq!(cache.used_bytes(), 4);

        let huge = [9; 32];
        assert!(cache.insert(huge, vec![9; 5], 1, 1).is_empty());
        assert!(!cache.contains(&huge));
        assert_eq!(cache.used_bytes(), 4);
    }
}
