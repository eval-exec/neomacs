use neomacs_display_protocol::font::{FontMemoryAsset, ResolvedFontIdentity};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, MutexGuard, Weak};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct NativeAssetKey {
    stable_key: String,
    face_index: u32,
}

#[derive(Debug)]
struct CachedNativeBytes {
    bytes: Weak<Vec<u8>>,
    last_used: u64,
}

#[derive(Debug, Default)]
struct NativeFontAssetCacheState {
    entries: HashMap<NativeAssetKey, CachedNativeBytes>,
    clock: u64,
}

impl NativeFontAssetCacheState {
    fn tick(&mut self) -> u64 {
        self.clock = self.clock.wrapping_add(1);
        if self.clock == 0 {
            // A wrapped clock cannot preserve LRU ordering. This happens only
            // after 2^64 accesses; clearing bounded weak metadata is cheaper
            // and safer than allowing an old entry to look newest forever.
            self.entries.clear();
            self.clock = 1;
        }
        self.clock
    }
}

/// Bounded weak interner for immutable bytes copied from a native font API.
///
/// The backend owns one cache per font-catalog generation. Entries never keep
/// font bytes alive by themselves: layout, frame snapshots, and rendering own
/// the strong references. The bound limits stale key metadata even when many
/// process-local fonts appear and disappear.
#[derive(Debug)]
pub(super) struct NativeFontAssetCache {
    capacity: NonZeroUsize,
    state: Mutex<NativeFontAssetCacheState>,
}

impl Default for NativeFontAssetCache {
    fn default() -> Self {
        Self::new(NonZeroUsize::new(64).expect("native font cache capacity is non-zero"))
    }
}

impl NativeFontAssetCache {
    pub(super) fn new(capacity: NonZeroUsize) -> Self {
        Self {
            capacity,
            state: Mutex::new(NativeFontAssetCacheState::default()),
        }
    }

    pub(super) fn get_or_materialize(
        &self,
        identity: &ResolvedFontIdentity,
        materialize: impl FnOnce() -> Option<Vec<u8>>,
    ) -> Option<FontMemoryAsset> {
        let stable_key = identity.stable_key.as_str();
        let face_index = identity.file_face_index();
        if identity.file_path.is_some() {
            return None;
        }
        if stable_key.is_empty() {
            return None;
        }
        let key = NativeAssetKey {
            stable_key: stable_key.to_owned(),
            face_index,
        };
        if let Some(bytes) = self.cached_bytes(&key) {
            return FontMemoryAsset::new(stable_key, bytes, face_index);
        }

        let bytes = Arc::new(materialize()?);
        let asset = FontMemoryAsset::new(stable_key, Arc::clone(&bytes), face_index)?;
        let mut state = self.lock_state();
        let now = state.tick();

        // Another resolver call may have materialized this face while the
        // platform copy ran outside the mutex. Prefer that already-published
        // allocation so callers converge on one Arc without serializing slow
        // CoreText/DirectWrite calls behind the cache lock.
        if let Some(existing) = state.entries.get_mut(&key)
            && let Some(existing_bytes) = existing.bytes.upgrade()
        {
            existing.last_used = now;
            return FontMemoryAsset::new(stable_key, existing_bytes, face_index);
        }

        state.entries.insert(
            key,
            CachedNativeBytes {
                bytes: Arc::downgrade(&bytes),
                last_used: now,
            },
        );
        while state.entries.len() > self.capacity.get() {
            let Some(oldest) = state
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            state.entries.remove(&oldest);
        }
        Some(asset)
    }

    pub(super) fn clear(&self) {
        let mut state = self.lock_state();
        state.entries.clear();
        state.clock = 0;
    }

    fn cached_bytes(&self, key: &NativeAssetKey) -> Option<Arc<Vec<u8>>> {
        let mut state = self.lock_state();
        let now = state.tick();
        let entry = state.entries.get_mut(key)?;
        let bytes = entry.bytes.upgrade();
        if bytes.is_some() {
            entry.last_used = now;
        } else {
            state.entries.remove(key);
        }
        bytes
    }

    fn lock_state(&self) -> MutexGuard<'_, NativeFontAssetCacheState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
#[path = "native_asset_cache_test.rs"]
mod tests;
