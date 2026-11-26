//! One realized fontset per ASCII face per frame.
//!
//! GNU keeps that fontset on the face and frees it in `free_realized_face`
//! (`src/xfaces.c`) when the face is an ASCII face. `face_change` drops the
//! realized faces (`src/xdisp.c`), so a new face epoch starts again in
//! charset order. `set-fontset-font` still does that through
//! `fontset_generation`. The layout resolver and the worker share this map:
//! the worker's service is a second instance and must see the same move.
//!
//! A cached fallback answer stays valid only while its winning entry is
//! still that fontset's front. Each fontset has its own generation, bumped
//! under the map lock when its front changes. A hit whose generation still
//! matches takes no lock. One face's move does not invalidate another.

use crate::font_backend::FontSelectionSize;
use neovm_core::emacs_core::fontset::FontSpecEntry;
use rustc_hash::FxHashMap as HashMap;
use rustc_hash::FxHashSet as HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// The realized fontset GNU attaches to one ASCII face on one frame.
///
/// `face` is the frame-local face id. `face_epoch` is the
/// `face_change_count` at realization. A later face change is a different
/// fontset, the same way a new `fontset_generation` is.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub(crate) struct FaceFontsetId {
    pub(crate) frame: u64,
    pub(crate) face: u32,
    pub(crate) face_epoch: u64,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct FallbackOrderKey {
    pub(crate) fontset: FaceFontsetId,
    pub(crate) family: String,
    pub(crate) weight: u16,
    pub(crate) slant: u16,
    pub(crate) width: u16,
    /// `None` is the fontconfig character walker.
    pub(crate) size: Option<FontSelectionSize>,
    pub(crate) fontset_generation: u64,
}

/// Generation of one fontset's fallback group. The slot is that group's
/// counter. A cleared group bumps its slot and drops the map entry, so a
/// stamp still holding the old slot misses even if a new group reuses the key.
#[derive(Clone, Debug)]
pub(crate) struct OrderGeneration {
    slot: Arc<AtomicU64>,
    observed: u64,
}

impl OrderGeneration {
    fn observe(slot: &Arc<AtomicU64>) -> Self {
        Self {
            observed: slot.load(Ordering::Acquire),
            slot: Arc::clone(slot),
        }
    }

    fn still_current(&self) -> bool {
        self.slot.load(Ordering::Acquire) == self.observed
    }
}

#[derive(Clone, Debug)]
pub(crate) struct FallbackAnswerStamp {
    pub(crate) order_key: FallbackOrderKey,
    pub(crate) winner: FontSpecEntry,
    /// Observed under the order lock while `winner` was the front.
    pub(crate) generation: OrderGeneration,
}

struct FallbackGroup {
    entries: Vec<FontSpecEntry>,
    generation: Arc<AtomicU64>,
}

impl Default for FallbackGroup {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            generation: Arc::new(AtomicU64::new(1)),
        }
    }
}

fn bump(group: &FallbackGroup) {
    group.generation.fetch_add(1, Ordering::Release);
}

static FALLBACK_ORDERS: OnceLock<Mutex<HashMap<FallbackOrderKey, FallbackGroup>>> = OnceLock::new();
/// Highest face epoch any service has installed. A stale context must not
/// prune groups that belong to a newer epoch.
static FONTSET_EPOCH_HIGH_WATER: AtomicU64 = AtomicU64::new(0);
/// Frames that currently own a fallback group. Frame-deletion checks this
/// set instead of walking every cache entry on each redisplay.
static ORDER_FRAMES: OnceLock<Mutex<HashSet<u64>>> = OnceLock::new();

#[cfg(test)]
static ORDER_LOCKS: AtomicU64 = AtomicU64::new(0);

fn orders() -> &'static Mutex<HashMap<FallbackOrderKey, FallbackGroup>> {
    FALLBACK_ORDERS.get_or_init(|| Mutex::new(HashMap::default()))
}

fn lock() -> std::sync::MutexGuard<'static, HashMap<FallbackOrderKey, FallbackGroup>> {
    #[cfg(test)]
    ORDER_LOCKS.fetch_add(1, Ordering::Relaxed);
    orders()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
pub(crate) fn fallback_order_lock_count() -> u64 {
    ORDER_LOCKS.load(Ordering::Relaxed)
}

pub(crate) fn fontset_epoch_high_water() -> u64 {
    FONTSET_EPOCH_HIGH_WATER.load(Ordering::Acquire)
}

/// Record `epoch` when it moves the process-wide high-water mark forward.
/// A stale or repeated epoch returns false and must not drop newer groups.
pub(crate) fn note_fontset_epoch(epoch: u64) -> bool {
    let mut current = FONTSET_EPOCH_HIGH_WATER.load(Ordering::Acquire);
    loop {
        if epoch <= current {
            return false;
        }
        match FONTSET_EPOCH_HIGH_WATER.compare_exchange(
            current,
            epoch,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return true,
            Err(observed) => current = observed,
        }
    }
}

fn order_frames() -> &'static Mutex<HashSet<u64>> {
    ORDER_FRAMES.get_or_init(|| Mutex::new(HashSet::default()))
}

fn remember_order_frame(frame: u64) {
    if frame == 0 {
        return;
    }
    order_frames()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(frame);
}

fn replace_order_frames(frames: impl IntoIterator<Item = u64>) {
    let mut recorded = order_frames()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    recorded.clear();
    recorded.extend(frames);
}

/// Frames that own a fallback group. Empty when no group has been created.
pub(crate) fn fallback_order_frames() -> Vec<u64> {
    let Some(frames) = ORDER_FRAMES.get() else {
        return Vec::new();
    };
    frames
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .copied()
        .collect()
}

pub(crate) fn clear_fallback_orders() {
    if let Some(orders) = FALLBACK_ORDERS.get() {
        let mut orders = orders
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for group in orders.values() {
            bump(group);
        }
        orders.clear();
    }
    if let Some(frames) = ORDER_FRAMES.get() {
        frames
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }
}

/// Drop groups whose fontset matches `drop_fontset`. Each dropped group is
/// bumped first, the same way [`clear_fallback_orders`] retires stamps.
pub(crate) fn prune_fallback_orders(drop_fontset: &impl Fn(&FaceFontsetId) -> bool) {
    if FALLBACK_ORDERS.get().is_none() {
        return;
    }
    let mut orders = lock();
    orders.retain(|key, group| {
        if drop_fontset(&key.fontset) {
            bump(group);
            false
        } else {
            true
        }
    });
    let frames = orders
        .keys()
        .filter(|key| key.fontset.frame != 0)
        .map(|key| key.fontset.frame);
    replace_order_frames(frames);
}

#[cfg(test)]
fn fallback_orders_matching(keep: impl Fn(&FaceFontsetId) -> bool) -> usize {
    let Some(orders) = FALLBACK_ORDERS.get() else {
        return 0;
    };
    let orders = orders
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    orders.keys().filter(|key| keep(&key.fontset)).count()
}

#[cfg(test)]
pub(crate) fn fallback_order_group_count() -> usize {
    fallback_orders_matching(|_| true)
}

#[cfg(test)]
pub(crate) fn fallback_order_groups_for_frame(frame: u64) -> usize {
    fallback_orders_matching(|fontset| fontset.frame == frame)
}

/// Fast path when this fontset's generation is still current. A mismatch
/// takes the order lock, accepts the answer only if `winner` is still the
/// front, and rebinds the stamp to that live group's generation.
pub(crate) fn stamp_still_front(stamp: &mut FallbackAnswerStamp) -> bool {
    if stamp.generation.still_current() {
        return true;
    }
    let orders = lock();
    let Some(group) = orders.get(&stamp.order_key) else {
        return false;
    };
    if group.entries.first() != Some(&stamp.winner) {
        return false;
    }
    // The winner is the front of the group that exists now. A clear bumps
    // the old slot and drops it, so this is the recreated group's slot.
    // Store that generation only because the front check just succeeded.
    stamp.generation = OrderGeneration::observe(&group.generation);
    true
}

/// Charset order, with fallbacks already found for this face moved forward.
pub(crate) fn order_fallback_group(
    key: &FallbackOrderKey,
    base: Vec<FontSpecEntry>,
) -> Vec<FontSpecEntry> {
    let mut orders = lock();
    let previous_front = orders
        .get(key)
        .and_then(|group| group.entries.first().cloned());
    let group = orders.entry(key.clone()).or_default();
    remember_order_frame(key.fontset.frame);
    let mut ordered = Vec::with_capacity(base.len());
    for entry in &group.entries {
        if base.contains(entry) && !ordered.contains(entry) {
            ordered.push(entry.clone());
        }
    }
    for entry in &base {
        if !ordered.contains(entry) {
            ordered.push(entry.clone());
        }
    }
    group.entries.clone_from(&ordered);
    // A new group has no stamps. Clear already bumped every group it dropped.
    if previous_front.is_some() && group.entries.first() != previous_front.as_ref() {
        bump(group);
    }
    ordered
}

/// Move `found` to index 0. Already-front is a no-op, matching GNU's
/// `found_index > 0` guard. The returned generation was loaded under the
/// same lock, after the move.
pub(crate) fn promote_fallback(key: &FallbackOrderKey, found: &FontSpecEntry) -> OrderGeneration {
    let mut orders = lock();
    let group = orders.entry(key.clone()).or_default();
    remember_order_frame(key.fontset.frame);
    let changed = match group.entries.iter().position(|entry| entry == found) {
        Some(0) => false,
        Some(index) => {
            let entry = group.entries.remove(index);
            group.entries.insert(0, entry);
            true
        }
        None => {
            group.entries.insert(0, found.clone());
            true
        }
    };
    if changed {
        bump(group);
    }
    OrderGeneration::observe(&group.generation)
}
