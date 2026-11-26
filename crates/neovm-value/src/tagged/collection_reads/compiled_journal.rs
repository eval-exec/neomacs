//! The native write policy and its existing-mutator observation envelope.
//!
//! Configuration is immutable process state. The test override is a scalar,
//! not Lisp state. Observed addresses remain in the existing mutator journal
//! State: certificates are !Send and survive Context switches on that mutator.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum CompiledJournalMode {
    Off,
    Observed,
    Eager,
}

#[inline]
#[doc(hidden)]
pub fn compiled_journal_mode() -> CompiledJournalMode {
    // Same rule as the override's own gate (and its setter): a re-homed test
    // sets this through test-support, so the read must exist there too.
    #[cfg(any(test, feature = "test-support"))]
    if let Some(mode) = TEST_MODE.with(std::cell::Cell::get) {
        return mode;
    }
    #[cfg(not(feature = "jit"))]
    return CompiledJournalMode::Off;
    #[cfg(feature = "jit")]
    {
        static MODE: std::sync::OnceLock<CompiledJournalMode> = std::sync::OnceLock::new();
        *MODE.get_or_init(|| {
            match std::env::var("NEOVM_JIT_GEN0_COLLECTION_JOURNAL")
                .ok()
                .as_deref()
            {
                Some("0" | "off" | "false" | "no") => CompiledJournalMode::Off,
                Some("eager") => CompiledJournalMode::Eager,
                _ => CompiledJournalMode::Observed,
            }
        })
    }
}

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    /// Scalar policy only; every mutator retains its own existing journal.
    static TEST_MODE: std::cell::Cell<Option<CompiledJournalMode>> = const { std::cell::Cell::new(None) };
}

#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn force_compiled_journal_for_test(mode: Option<CompiledJournalMode>) {
    TEST_MODE.with(|setting| setting.set(mode));
    // An Off capture may populate RECENT without publishing a sticky mark.
    // Scalar test-policy changes cannot retain those hits in Observed mode.
    super::clear_recent_reads();
    super::publish_compiled_observation_window();
}

/// A non-observing query. Cons bits are keys into metadata and never
/// dereferenced; non-cons heap tags read the object's `GcHeader`.
///
/// # Safety
/// If `bits` carries `TAG_STRING`, `TAG_FLOAT`, or `TAG_VECLIKE` with a
/// non-zero address, `bits & !TAG_MASK` must point to a live, properly
/// aligned heap object whose leading `GcHeader` remains readable for the
/// duration of this call. `TAG_CONS` and immediate tags do not dereference
/// `bits`.
#[inline]
#[doc(hidden)]
pub unsafe fn is_observed(bits: usize) -> bool {
    // SAFETY: caller upholds the live-header contract of `collection_observed`.
    unsafe { crate::tagged::gc::collection_observed(bits) }
}
