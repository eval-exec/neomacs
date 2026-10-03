//! The native write policy and its existing-mutator observation envelope.
//!
//! Configuration is immutable process state. The test override is a scalar,
//! not Lisp state. Observed addresses remain in the existing mutator journal
//! State: certificates are !Send and survive Context switches on that mutator.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompiledJournalMode {
    Off,
    Observed,
    Eager,
}

#[inline]
pub(crate) fn compiled_journal_mode() -> CompiledJournalMode {
    #[cfg(test)]
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

#[cfg(test)]
thread_local! {
    /// Scalar policy only; every mutator retains its own existing journal.
    static TEST_MODE: std::cell::Cell<Option<CompiledJournalMode>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub(crate) fn force_compiled_journal_for_test(mode: Option<CompiledJournalMode>) {
    TEST_MODE.with(|setting| setting.set(mode));
    super::publish_compiled_observation_window();
}

/// A non-observing query. A non-cons address must still name retained storage;
/// Cons bits are keys into metadata, and never dereferenced by the query.
#[inline]
pub(crate) fn is_observed(bits: usize) -> bool {
    crate::tagged::gc::collection_observed(bits)
}
