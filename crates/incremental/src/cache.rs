use std::collections::HashMap;

/// Resource bounds for a session-local fragment cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheLimits {
    pub max_entries: usize,
    pub max_bytes: usize,
}

impl Default for CacheLimits {
    fn default() -> Self {
        Self {
            max_entries: 256,
            max_bytes: 4 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone)]
struct CacheEntry<T> {
    stable_id: String,
    value: T,
    bytes: usize,
    last_used: u64,
}

/// A bounded approximate-LRU cache keyed by content-addressed fragment keys.
#[derive(Debug, Clone)]
pub struct BoundedCache<T> {
    entries: HashMap<String, CacheEntry<T>>,
    limits: CacheLimits,
    bytes: usize,
    clock: u64,
}

impl<T> BoundedCache<T> {
    pub fn new(limits: CacheLimits) -> Self {
        Self {
            entries: HashMap::new(),
            limits,
            bytes: 0,
            clock: 0,
        }
    }

    pub fn get(&mut self, key: &str) -> Option<&T> {
        self.clock = self.clock.wrapping_add(1);
        let entry = self.entries.get_mut(key)?;
        entry.last_used = self.clock;
        Some(&entry.value)
    }

    /// Insert a value and return the number of entries evicted to enforce bounds.
    pub fn insert(&mut self, key: String, stable_id: String, value: T, bytes: usize) -> u64 {
        self.clock = self.clock.wrapping_add(1);
        if let Some(previous) = self.entries.remove(&key) {
            self.bytes = self.bytes.saturating_sub(previous.bytes);
        }
        if bytes > self.limits.max_bytes || self.limits.max_entries == 0 {
            return 0;
        }
        self.bytes = self.bytes.saturating_add(bytes);
        self.entries.insert(
            key,
            CacheEntry {
                stable_id,
                value,
                bytes,
                last_used: self.clock,
            },
        );
        self.evict_to_limits()
    }

    pub fn remove_stable_id(&mut self, stable_id: &str) {
        self.entries.retain(|_, entry| {
            if entry.stable_id == stable_id {
                self.bytes = self.bytes.saturating_sub(entry.bytes);
                false
            } else {
                true
            }
        });
    }

    pub fn retain_stable_ids(&mut self, valid: impl Fn(&str) -> bool) {
        self.entries.retain(|_, entry| {
            if valid(&entry.stable_id) {
                true
            } else {
                self.bytes = self.bytes.saturating_sub(entry.bytes);
                false
            }
        });
    }

    pub fn bytes(&self) -> usize {
        self.bytes
    }

    fn evict_to_limits(&mut self) -> u64 {
        let mut evictions = 0;
        while self.entries.len() > self.limits.max_entries || self.bytes > self.limits.max_bytes {
            let Some(key) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(entry) = self.entries.remove(&key) {
                self.bytes = self.bytes.saturating_sub(entry.bytes);
                evictions += 1;
            }
        }
        evictions
    }
}
