//! LRU page cache for buffering Btrieve file pages
//!
//! The page cache reduces disk I/O by keeping frequently accessed pages in memory.

use lru::LruCache;
use parking_lot::RwLock;
use std::num::NonZeroUsize;
use std::sync::Arc;

use crate::storage::page::Page;

/// Cache key combining file path and page number
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    file_path: String,
    page_number: u32,
}

/// Cached page with metadata
#[derive(Debug, Clone)]
struct CachedPage {
    page: Page,
    dirty: bool,
    pin_count: u32,
}

/// Thread-safe LRU page cache
pub struct PageCache {
    cache: RwLock<LruCache<CacheKey, CachedPage>>,
    capacity: usize,
    stats: RwLock<CacheStats>,
}

/// Cache statistics
#[derive(Debug, Default, Clone)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub dirty_writes: u64,
}

impl PageCache {
    /// Create a new page cache with given capacity (number of pages)
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(16); // Minimum 16 pages
        PageCache {
            cache: RwLock::new(LruCache::new(
                NonZeroUsize::new(capacity).unwrap(),
            )),
            capacity,
            stats: RwLock::new(CacheStats::default()),
        }
    }

    /// Get a page from cache
    pub fn get(&self, file_path: &str, page_number: u32) -> Option<Page> {
        let key = CacheKey {
            file_path: file_path.to_string(),
            page_number,
        };

        let mut cache = self.cache.write();
        if let Some(cached) = cache.get(&key) {
            self.stats.write().hits += 1;
            Some(cached.page.clone())
        } else {
            self.stats.write().misses += 1;
            None
        }
    }

    /// Put a page into cache
    pub fn put(&self, file_path: &str, page: Page, dirty: bool) {
        let key = CacheKey {
            file_path: file_path.to_string(),
            page_number: page.page_number,
        };

        let cached = CachedPage {
            page,
            dirty,
            pin_count: 0,
        };

        let mut cache = self.cache.write();

        // Check if we're evicting a dirty page
        if cache.len() >= self.capacity {
            if let Some((_, evicted)) = cache.peek_lru() {
                if evicted.dirty {
                    self.stats.write().dirty_writes += 1;
                }
            }
            self.stats.write().evictions += 1;
        }

        cache.put(key, cached);
    }

    /// Mark a page as dirty
    pub fn mark_dirty(&self, file_path: &str, page_number: u32) {
        let key = CacheKey {
            file_path: file_path.to_string(),
            page_number,
        };

        let mut cache = self.cache.write();
        if let Some(cached) = cache.get_mut(&key) {
            cached.dirty = true;
        }
    }

    /// Get all dirty pages for a file
    pub fn get_dirty_pages(&self, file_path: &str) -> Vec<Page> {
        let cache = self.cache.read();
        cache
            .iter()
            .filter(|(k, v)| k.file_path == file_path && v.dirty)
            .map(|(_, v)| v.page.clone())
            .collect()
    }

    /// Clear dirty flag for a page
    pub fn clear_dirty(&self, file_path: &str, page_number: u32) {
        let key = CacheKey {
            file_path: file_path.to_string(),
            page_number,
        };

        let mut cache = self.cache.write();
        if let Some(cached) = cache.get_mut(&key) {
            cached.dirty = false;
        }
    }

    /// Remove all pages for a file from cache
    pub fn invalidate_file(&self, file_path: &str) -> Vec<Page> {
        let mut cache = self.cache.write();
        let mut dirty_pages = Vec::new();

        // Collect keys to remove
        let keys_to_remove: Vec<_> = cache
            .iter()
            .filter(|(k, _)| k.file_path == file_path)
            .map(|(k, _)| k.clone())
            .collect();

        // Remove and collect dirty pages
        for key in keys_to_remove {
            if let Some(cached) = cache.pop(&key) {
                if cached.dirty {
                    dirty_pages.push(cached.page);
                }
            }
        }

        dirty_pages
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        self.stats.read().clone()
    }

    /// Get current cache size
    pub fn len(&self) -> usize {
        self.cache.read().len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.read().is_empty()
    }

    /// Clear entire cache, returning dirty pages
    pub fn clear(&self) -> Vec<(String, Page)> {
        let mut cache = self.cache.write();
        let mut dirty = Vec::new();

        while let Some((key, cached)) = cache.pop_lru() {
            if cached.dirty {
                dirty.push((key.file_path, cached.page));
            }
        }

        dirty
    }
}

impl Default for PageCache {
    fn default() -> Self {
        // Default to 1000 pages (~4MB with 4K pages)
        Self::new(1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_basic() {
        let cache = PageCache::new(10);

        // Create and insert a page
        let page = Page::new(0, 4096);
        cache.put("test.dat", page.clone(), false);

        // Retrieve it
        let retrieved = cache.get("test.dat", 0);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().page_number, 0);

        // Check stats
        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 0);
    }

    #[test]
    fn test_cache_miss() {
        let cache = PageCache::new(10);

        let result = cache.get("nonexistent.dat", 0);
        assert!(result.is_none());

        let stats = cache.stats();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 1);
    }

    #[test]
    fn test_dirty_tracking() {
        let cache = PageCache::new(10);

        let page = Page::new(0, 4096);
        cache.put("test.dat", page, true);

        let dirty = cache.get_dirty_pages("test.dat");
        assert_eq!(dirty.len(), 1);

        cache.clear_dirty("test.dat", 0);
        let dirty = cache.get_dirty_pages("test.dat");
        assert_eq!(dirty.len(), 0);
    }

    #[test]
    fn test_invalidate_file() {
        let cache = PageCache::new(10);

        for i in 0..5 {
            let page = Page::new(i, 4096);
            cache.put("test.dat", page, i % 2 == 0);
        }

        let dirty = cache.invalidate_file("test.dat");
        // Pages 0, 2, 4 are dirty
        assert_eq!(dirty.len(), 3);
        assert!(cache.is_empty());
    }
}
