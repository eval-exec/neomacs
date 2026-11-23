//! Per-face order of the fontset fallback group.
//!
//! GNU keeps one realized fontset per ASCII face and moves the found
//! fallback object to index 0 (`src/fontset.c`, the `found:` label).
//! `set-fontset-font` frees every realized fontset, so a new
//! `fontset_generation` starts again in charset order. The layout
//! resolver and the worker share one slot. A cached fallback answer
//! stays valid only while its winning entry is still the front, so a
//! clear cannot revive it by restarting a counter. The fontconfig
//! walker uses `size: None` because it has no pixel size and must not
//! reorder a layout face.

use crate::font_backend::FontSelectionSize;
use neovm_core::emacs_core::fontset::FontSpecEntry;
use rustc_hash::FxHashMap as HashMap;
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
        orders
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
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

/// Charset order, with fallbacks already found for this face moved forward.
pub(crate) fn order_fallback_group(
    key: &FallbackOrderKey,
    base: Vec<FontSpecEntry>,
) -> Vec<FontSpecEntry> {
    let mut orders = lock();
    let group = orders.entry(key.clone()).or_insert_with(|| FallbackGroup {
        entries: base.clone(),
    });
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
    ordered
}

/// Move `found` to index 0. Already-front is a no-op, matching GNU's
/// `found_index > 0` guard.
pub(crate) fn promote_fallback(key: &FallbackOrderKey, found: &FontSpecEntry) {
    let mut orders = lock();
    let group = orders.entry(key.clone()).or_default();
    match group.entries.iter().position(|entry| entry == found) {
        Some(0) => {}
        Some(index) => {
            let entry = group.entries.remove(index);
            group.entries.insert(0, entry);
        }
        None => group.entries.insert(0, found.clone()),
    }
}
