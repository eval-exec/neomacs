//! Per-face order of the fontset fallback group.
//!
//! GNU keeps one realized fontset per ASCII face and moves the found
//! fallback object to index 0 (`src/fontset.c`, the `found:` label).
//! `set-fontset-font` frees every realized fontset, so a new
//! `fontset_generation` starts again in charset order. The layout
//! resolver and the worker share one slot. The fontconfig walker uses
//! `size: None` because it has no pixel size and must not reorder a
//! layout face.

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

struct FallbackGroup {
    entries: Vec<FontSpecEntry>,
    epoch: u64,
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

pub(crate) fn fallback_epoch(key: &FallbackOrderKey) -> u64 {
    lock().get(key).map(|group| group.epoch).unwrap_or(0)
}

/// Charset order, with fallbacks already found for this face moved forward.
pub(crate) fn order_fallback_group(
    key: &FallbackOrderKey,
    base: Vec<FontSpecEntry>,
) -> Vec<FontSpecEntry> {
    let mut orders = lock();
    let group = orders.entry(key.clone()).or_insert_with(|| FallbackGroup {
        entries: base.clone(),
        epoch: 0,
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

/// Move `found` to index 0. The epoch changes only when the front changes.
pub(crate) fn promote_fallback(key: &FallbackOrderKey, found: &FontSpecEntry) {
    let mut orders = lock();
    let group = orders.entry(key.clone()).or_default();
    match group.entries.iter().position(|entry| entry == found) {
        Some(0) => return,
        Some(index) => {
            let entry = group.entries.remove(index);
            group.entries.insert(0, entry);
        }
        None => group.entries.insert(0, found.clone()),
    }
    group.epoch = group.epoch.wrapping_add(1);
}

impl Default for FallbackGroup {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            epoch: 0,
        }
    }
}
