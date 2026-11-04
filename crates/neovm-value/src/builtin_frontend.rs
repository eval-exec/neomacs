//! The process-wide U2.8 builtin-front-end knob (stage-3b T5 frontend
//! service).
//!
//! `NEOVM_BUILTIN_FRONTEND` gates every U2.8 fast path, for a same-binary
//! A/B: unset, `1`, `on` or `all` enables them; `0`, `off` or `none` restores
//! the general reads and the old per-call preparation. Read once per process,
//! on the first builtin that asks.
//!
//! Moved verbatim out of the evaluator: the knob is a plain process-global
//! (one relaxed byte load once read) named by the text layer's regex/syntax
//! engines and by core's builtin variable reads; core keeps its paths through
//! eval re-exports. No TLS is added: the test override is
//! `cfg(any(test, feature = "test-support"))` and absent from product
//! profiles.

use std::sync::atomic::{AtomicU8, Ordering};

const KNOB_UNREAD: u8 = 0;
const KNOB_OFF: u8 = 1;
const KNOB_ON: u8 = 2;

static BUILTIN_FRONTEND: AtomicU8 = AtomicU8::new(KNOB_UNREAD);

/// Whether a value of `NEOVM_BUILTIN_FRONTEND` enables the front-end diets.
#[doc(hidden)]
pub fn parse_builtin_frontend_knob(value: Option<&str>) -> bool {
    match value.map(str::trim) {
        None | Some("") => true,
        Some(word) => match word.to_ascii_lowercase().as_str() {
            "1" | "on" | "all" | "true" => true,
            "0" | "off" | "none" | "false" => false,
            other => {
                tracing::warn!(
                    value = other,
                    "NEOVM_BUILTIN_FRONTEND: unknown value, front-end diets stay on"
                );
                true
            }
        },
    }
}

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    static KNOB_TEST_OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Force the knob on this thread (tests only); `None` returns to the
/// environment.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn set_builtin_frontend_for_test(on: Option<bool>) {
    KNOB_TEST_OVERRIDE.with(|cell| cell.set(on));
}

/// Whether the U2.8 front-end fast paths are enabled. One relaxed byte load
/// once the knob has been read.
#[doc(hidden)]
#[inline(always)]
pub fn builtin_frontend_on() -> bool {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(on) = KNOB_TEST_OVERRIDE.with(|cell| cell.get()) {
        return on;
    }
    match BUILTIN_FRONTEND.load(Ordering::Relaxed) {
        KNOB_ON => true,
        KNOB_OFF => false,
        _ => read_builtin_frontend_knob(),
    }
}

#[cold]
#[inline(never)]
fn read_builtin_frontend_knob() -> bool {
    let on = parse_builtin_frontend_knob(std::env::var("NEOVM_BUILTIN_FRONTEND").ok().as_deref());
    BUILTIN_FRONTEND.store(if on { KNOB_ON } else { KNOB_OFF }, Ordering::Relaxed);
    tracing::debug!(on, "NEOVM_BUILTIN_FRONTEND read");
    on
}
