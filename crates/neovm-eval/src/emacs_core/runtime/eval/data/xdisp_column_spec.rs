//! Carrier-parked latch for the `%c`/`%C` mode-line detector.
//!
//! Stage-4 E7: this thread-local moves WITH eval as one unit (one definition,
//! same type, PT_TLS identical); the display engine above eval reaches it
//! downward through the accessors here. The carrier walker in
//! `eval::green::carrier_tls` resets/fingerprints it in place.
//!
//! We record only WHETHER the spec was consumed, not its value. Refusing the
//! chrome skip outright whenever `%c`/`%C` is displayed is strictly more
//! conservative than GNU (which compares the value and still skips when the
//! column happens to be unchanged), and it costs nothing in the default
//! configuration because `column-number-mode` is off (lisp/simple.el:9387).
//! Comparing values is the later relaxation.
//!
//! Thread-local for the same reason as the layout engine's eval counter: the
//! mode line is expanded on the evaluator's thread.

thread_local! {
    static COLUMN_SPEC_CONSUMED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[doc(hidden)]
pub fn note_column_spec_consumed() {
    COLUMN_SPEC_CONSUMED.with(|consumed| consumed.set(true));
}

/// Arm the `%c`/`%C` detector for one mode-line expansion.
pub fn reset_column_spec_consumed() {
    COLUMN_SPEC_CONSUMED.with(|consumed| consumed.set(false));
}

/// Whether the expansion since [`reset_column_spec_consumed`] displayed a
/// column. See the `COLUMN_SPEC_CONSUMED` comment for why redisplay asks.
pub fn column_spec_consumed() -> bool {
    COLUMN_SPEC_CONSUMED.with(std::cell::Cell::get)
}

/// Carrier park: clear the column-spec consumption latch.
pub(crate) fn reset_carrier_tls() {
    COLUMN_SPEC_CONSUMED.with(|slot| slot.set(false));
}

/// Debug-only carrier TLS fingerprint for this module's classified records.
#[cfg(debug_assertions)]
pub(crate) fn carrier_tls_fingerprint(h: &mut u64) {
    neovm_value::tls_fingerprint::mix(h, COLUMN_SPEC_CONSUMED.with(|slot| slot.get() as u64));
}
