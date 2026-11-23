//! Per-face order of the fontset fallback group.
//!
//! GNU keeps one realized fontset per ASCII face and moves the found
//! fallback object to index 0 (`src/fontset.c`, the `found:` label).
//! `set-fontset-font` frees every realized fontset, so a new
//! `fontset_generation` starts again in charset order. The layout
//! resolver and the worker share one slot. A cached fallback answer
//! stays valid only while its winning entry is still the front. A
//! monotonic generation, bumped under the order lock on every front
//! change and every clear, lets a hit skip that lock when nothing has
//! moved. The generation never restarts, so a clear cannot revive a
//! stored answer. The fontconfig walker uses `size: None` because it
//! has no pixel size and must not reorder a layout face.

use crate::font_backend::FontSelectionSize;
use neovm_core::emacs_core::fontset::FontSpecEntry;
use rustc_hash::FxHashMap as HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct FallbackOrderKey {
    pub(crate) family: String,
    pub(crate) weight: u16,
    pub(crate) slant: u16,
    pub(crate) width: u16,
    /// `None` is the fontconfig character walker.
    pub(crate) size: Option<FontSelectionSize>,
    pub(crate) fontset_generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FallbackAnswerStamp {
    pub(crate) order_key: FallbackOrderKey,
    pub(crate) winner: FontSpecEntry,
    /// `ORDER_GENERATION` observed under the order lock while `winner` was
    /// the front. A later load that still equals this value needs no lock.
    pub(crate) order_gen: u64,
}

/// Bumped only while `FALLBACK_ORDERS` is held, and never reset.
static ORDER_GENERATION: AtomicU64 = AtomicU64::new(1);

fn bump_order_generation() {
    ORDER_GENERATION.fetch_add(1, Ordering::Release);
}

fn order_generation() -> u64 {
    ORDER_GENERATION.load(Ordering::Acquire)
}

#[derive(Default)]
struct FallbackGroup {
    entries: Vec<FontSpecEntry>,
}

static FALLBACK_ORDERS: OnceLock<Mutex<HashMap<FallbackOrderKey, FallbackGroup>>> = OnceLock::new();

fn orders() -> &'static Mutex<HashMap<FallbackOrderKey, FallbackGroup>> {
    FALLBACK_ORDERS.get_or_init(|| Mutex::new(HashMap::default()))
}

fn lock() -> std::sync::MutexGuard<'static, HashMap<FallbackOrderKey, FallbackGroup>> {
    orders()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) fn clear_fallback_orders() {
    if let Some(orders) = FALLBACK_ORDERS.get() {
        let mut orders = orders
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        orders.clear();
        bump_order_generation();
    }
}

/// True when `winner` is still the front of this face's fallback group.
///
/// A missing group is not a match: `clear_fallback_orders` drops the history,
/// and a cached winner must not survive that just because a later history
/// would have reused an epoch number.
pub(crate) fn fallback_front_is(key: &FallbackOrderKey, winner: &FontSpecEntry) -> bool {
    lock().get(key).and_then(|group| group.entries.first()) == Some(winner)
}

/// Fast path when `stamp.order_gen` is still current. A mismatch takes the
/// order lock, accepts the answer only if `winner` is still the front, and
/// refreshes the stored generation from that same critical section.
pub(crate) fn stamp_still_front(stamp: &mut FallbackAnswerStamp) -> bool {
    if stamp.order_gen == order_generation() {
        return true;
    }
    let orders = lock();
    let front_ok = orders
        .get(&stamp.order_key)
        .and_then(|group| group.entries.first())
        == Some(&stamp.winner);
    if !front_ok {
        return false;
    }
    stamp.order_gen = order_generation();
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
    if group.entries.first() != previous_front.as_ref() {
        bump_order_generation();
    }
    ordered
}

/// Move `found` to index 0. Already-front is a no-op, matching GNU's
/// `found_index > 0` guard.
pub(crate) fn promote_fallback(key: &FallbackOrderKey, found: &FontSpecEntry) -> u64 {
    let mut orders = lock();
    let group = orders.entry(key.clone()).or_default();
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
        bump_order_generation();
    }
    order_generation()
}
