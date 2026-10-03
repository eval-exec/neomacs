//! Lazy allocation-countdown accounting for GNU user hash-test callbacks.

use super::*;
use crate::emacs_core::builtins::HashTestGcInhibitAccounting;
use std::ptr::NonNull;

impl Context {
    /// Snapshot the already-normalized runtime projection at callback entry.
    /// Ordinary assignment publishes GC settings, and the parity-only slow
    /// binding hook also publishes dynamic binding and restoration. Only an
    /// unactivated Context resolves and refreshes settings here. The startup
    /// ceiling remains a live scalar read, since it has no cached projection.
    #[inline]
    pub(crate) fn capture_user_test_gc_accounting(&mut self) -> HashTestGcInhibitAccounting {
        if self
            .gc_runtime_settings_cache
            .hash_test_accounting
            .needs_live_settings()
        {
            self.sync_user_test_gc_binding();
        }
        let syms = match self.gc_runtime_settings_cache.syms {
            Some(syms) => syms,
            None => self.resolve_user_test_gc_setting_syms(),
        };
        let mut accounting = HashTestGcInhibitAccounting {
            bytes_at_start: self.tagged_heap.bytes_since_gc_exact(),
            charged_bytes_at_start: self.tagged_heap.bytes_since_gc(),
            collector_threshold_at_start: self.tagged_heap.gc_threshold(),
            threshold_at_start: None,
            entry_threshold_bytes: self.gc_runtime_settings_cache.gc_cons_threshold_bytes,
            entry_percentage_scaled: self.gc_runtime_settings_cache.gc_cons_percentage_scaled,
            threshold_overridden: self.tagged_heap.gc_threshold_is_overridden(),
            memory_full: self.gc_runtime_settings_cache.memory_full.is_full(),
            startup_ceiling: !self
                .obarray
                .symbol_value_id_or_nil(syms.startup_ceiling)
                .is_nil(),
        };
        if (accounting.startup_ceiling
            && accounting.collector_threshold_at_start > GC_STARTUP_THRESHOLD_CEILING_BYTES)
            || (!accounting.startup_ceiling
                && accounting.collector_threshold_at_start == GC_STARTUP_THRESHOLD_CEILING_BYTES)
        {
            self.sync_gc_threshold_from_runtime_settings();
            accounting.collector_threshold_at_start = self.tagged_heap.gc_threshold();
        }
        accounting
    }

    #[cold]
    #[inline(never)]
    fn resolve_user_test_gc_setting_syms(&mut self) -> GcSettingSyms {
        self.sync_gc_threshold_from_runtime_settings();
        self.gc_runtime_settings_cache
            .syms
            .expect("settings refresh resolves its symbols")
    }

    /// Dynamic GC variables already refuse the cached binding tiers. Publish
    /// their completed slow bind or restoration before Lisp can run again;
    /// the normal hash guard then needs no symbol lookup or normalization.
    /// Independent mutators publish only their own Context's projection.
    #[inline]
    pub(super) fn sync_user_test_gc_binding_by_id(&mut self, sym_id: SymId) {
        if self.is_gc_runtime_setting_symbol(sym_id)
            && super::super::hashtab::hash_test_parity_enabled()
        {
            self.sync_user_test_gc_binding();
        }
    }

    #[cold]
    #[inline(never)]
    fn sync_user_test_gc_binding(&mut self) {
        self.sync_gc_threshold_from_runtime_settings();
    }

    /// A rare symbol redirect change makes cached operands insufficient:
    /// future base-target writes or buffer swaps can bypass their canonical
    /// setters. Sticky fallback remains safe across nested guards and unwind.
    #[inline]
    pub(crate) fn mark_user_test_gc_settings_volatile(&mut self) {
        self.gc_runtime_settings_cache
            .hash_test_accounting
            .mark_live_settings();
    }

    #[inline]
    pub(crate) fn mark_user_test_gc_settings_volatile_if_gc_symbol(&mut self, sym_id: SymId) {
        if self.is_gc_runtime_setting_symbol(sym_id) {
            self.mark_user_test_gc_settings_volatile();
        }
    }

    #[inline]
    pub(crate) fn replace_user_test_gc_accounting(
        &mut self,
        pointer: Option<NonNull<HashTestGcInhibitAccounting>>,
    ) -> Option<NonNull<HashTestGcInhibitAccounting>> {
        self.gc_runtime_settings_cache
            .hash_test_accounting
            .replace_accounting(pointer)
    }

    #[inline]
    pub(crate) fn user_test_gc_accounting_pointer(
        &self,
    ) -> Option<NonNull<HashTestGcInhibitAccounting>> {
        self.gc_runtime_settings_cache
            .hash_test_accounting
            .accounting()
    }

    /// The collector's live estimate is constant while GC is inhibited. The
    /// allocation estimate and normalized settings must come from entry: changes
    /// within the callback adjust GNU's countdown and threshold equally.
    /// This repeats the existing pacing formula on the rare GC-maybe path so
    /// ordinary GC pacing and its inlining boundaries remain unchanged.
    #[cold]
    #[inline(never)]
    pub(crate) fn user_test_gc_entry_threshold(
        &self,
        accounting: &HashTestGcInhibitAccounting,
    ) -> usize {
        if accounting.threshold_overridden || accounting.memory_full {
            return accounting.collector_threshold_at_start;
        }
        let mut threshold = accounting
            .entry_threshold_bytes
            .max(GC_THRESHOLD_FLOOR_BYTES);
        if let Some(scaled) = accounting.entry_percentage_scaled {
            let live_estimate = self
                .tagged_heap
                .live_bytes()
                .saturating_add(accounting.charged_bytes_at_start / 2);
            let percentage_threshold = ((live_estimate as u128)
                .saturating_mul(scaled.get() as u128)
                .saturating_add((GC_PERCENT_SCALE - 1) as u128)
                / GC_PERCENT_SCALE as u128)
                .min(GC_HI_THRESHOLD_BYTES as u128) as usize;
            threshold = threshold.max(percentage_threshold);
        }
        let live_growth = ((self.tagged_heap.live_bytes() as u128)
            .saturating_mul(super::gc_live_growth_percent())
            / 100)
            .min(GC_HI_THRESHOLD_BYTES as u128) as usize;
        threshold = threshold.max(live_growth).clamp(1, GC_HI_THRESHOLD_BYTES);
        if accounting.startup_ceiling {
            threshold = threshold.min(GC_STARTUP_THRESHOLD_CEILING_BYTES);
        }
        gc_threshold_cap_from_env().map_or(threshold, |cap| threshold.min(cap))
    }
}
