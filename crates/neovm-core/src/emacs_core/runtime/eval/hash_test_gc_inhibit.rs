//! Lazy allocation-countdown accounting for GNU user hash-test callbacks.

use super::*;
use crate::emacs_core::builtins::HashTestGcInhibitAccounting;
use std::ptr::NonNull;

impl Context {
    /// Save live entry operands without refreshing the collector projection.
    /// Activation resolves these Context-local ids after construction or
    /// pdump reconstruction; only an unactivated Context takes the cold path.
    #[inline]
    pub(crate) fn capture_user_test_gc_accounting(&mut self) -> HashTestGcInhibitAccounting {
        let syms = match self.gc_runtime_settings_cache.syms {
            Some(syms) => syms,
            None => self.resolve_user_test_gc_setting_syms(),
        };
        let mut accounting = HashTestGcInhibitAccounting {
            bytes_at_start: self.tagged_heap.bytes_since_gc_exact(),
            charged_bytes_at_start: self.tagged_heap.bytes_since_gc(),
            collector_threshold_at_start: self.tagged_heap.gc_threshold(),
            threshold_at_start: None,
            threshold_setting: self.obarray.symbol_value_id_or_nil(syms.threshold),
            percentage_setting: self.obarray.symbol_value_id_or_nil(syms.percentage),
            threshold_overridden: self.tagged_heap.gc_threshold_is_overridden(),
            memory_full: !self
                .obarray
                .symbol_value_id_or_nil(syms.memory_full)
                .is_nil(),
            startup_ceiling: !self
                .obarray
                .symbol_value_id_or_nil(syms.startup_ceiling)
                .is_nil(),
        };
        // Specbind can change a live setting without refreshing the cached
        // collector projection. A stale high budget could otherwise delay an
        // automatic collection after this callback. Matching settings take
        // this inexpensive comparison; changed settings synchronize cold.
        if self.user_test_gc_settings_changed(&accounting)
            || (accounting.startup_ceiling
                && accounting.collector_threshold_at_start > GC_STARTUP_THRESHOLD_CEILING_BYTES)
            || (!accounting.startup_ceiling
                && accounting.collector_threshold_at_start == GC_STARTUP_THRESHOLD_CEILING_BYTES)
        {
            self.sync_gc_threshold_from_runtime_settings();
            accounting.collector_threshold_at_start = self.tagged_heap.gc_threshold();
        }
        accounting
    }

    #[inline]
    fn user_test_gc_settings_changed(&self, accounting: &HashTestGcInhibitAccounting) -> bool {
        let cached_threshold = self.gc_runtime_settings_cache.gc_cons_threshold_bytes;
        let threshold_matches = match accounting.threshold_setting.as_fixnum() {
            Some(number) if number >= 0 => number as usize == cached_threshold,
            _ => Self::user_test_gc_unusual_threshold_matches(
                accounting.threshold_setting,
                cached_threshold,
            ),
        };
        if !threshold_matches
            || accounting.memory_full != self.gc_runtime_settings_cache.memory_full
        {
            return true;
        }
        let percentage = accounting.percentage_setting.as_number_f64();
        match (
            percentage,
            self.gc_runtime_settings_cache.gc_cons_percentage_scaled,
        ) {
            (Some(float), Some(cached)) if float.is_finite() && float > 0.0 => {
                let scaled = float * GC_PERCENT_SCALE as f64;
                let cached = cached.get();
                // Both endpoints are exact in this range. Comparing the
                // ceil interval avoids doing a conversion on every callback.
                if cached <= (1_u64 << 53) {
                    scaled > cached as f64 || scaled <= (cached - 1) as f64
                } else {
                    !Self::user_test_gc_large_percentage_matches(scaled, cached)
                }
            }
            (None, None) => false,
            (Some(float), None) => float.is_finite() && float > 0.0,
            _ => true,
        }
    }

    #[cold]
    #[inline(never)]
    fn user_test_gc_unusual_threshold_matches(setting: Value, cached: usize) -> bool {
        setting
            .as_fixnum()
            .or_else(|| super::super::hashtab::gc_threshold_integer_fallback(setting))
            .and_then(|number| usize::try_from(number).ok())
            .unwrap_or(GC_DEFAULT_THRESHOLD_BYTES)
            == cached
    }

    #[cold]
    #[inline(never)]
    fn user_test_gc_large_percentage_matches(scaled: f64, cached: u64) -> bool {
        (scaled.ceil() as u64).clamp(1, u64::MAX) == cached
    }

    #[cold]
    #[inline(never)]
    fn resolve_user_test_gc_setting_syms(&mut self) -> GcSettingSyms {
        let syms = GcSettingSyms::resolve();
        self.gc_runtime_settings_cache.syms = Some(syms);
        syms
    }

    #[inline]
    pub(crate) fn replace_user_test_gc_accounting(
        &mut self,
        pointer: Option<NonNull<HashTestGcInhibitAccounting>>,
    ) -> Option<NonNull<HashTestGcInhibitAccounting>> {
        std::mem::replace(
            &mut self.gc_runtime_settings_cache.hash_test_accounting,
            pointer,
        )
    }

    #[inline]
    pub(crate) fn user_test_gc_accounting_pointer(
        &self,
    ) -> Option<NonNull<HashTestGcInhibitAccounting>> {
        self.gc_runtime_settings_cache.hash_test_accounting
    }

    /// The collector's live estimate is constant while GC is inhibited. The
    /// allocation estimate and Lisp settings must come from entry: changes
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
            .threshold_setting
            .as_fixnum()
            .or_else(|| {
                super::super::hashtab::gc_threshold_integer_fallback(accounting.threshold_setting)
            })
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(GC_DEFAULT_THRESHOLD_BYTES)
            .max(GC_THRESHOLD_FLOOR_BYTES);
        if let Some(float) = accounting
            .percentage_setting
            .as_number_f64()
            .filter(|float| float.is_finite() && *float > 0.0)
        {
            let scaled = ((float * GC_PERCENT_SCALE as f64).ceil() as u64).clamp(1, u64::MAX);
            let live_estimate = self
                .tagged_heap
                .live_bytes()
                .saturating_add(accounting.charged_bytes_at_start / 2);
            let percentage_threshold = ((live_estimate as u128)
                .saturating_mul(scaled as u128)
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
