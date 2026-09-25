//! Background compilation (P2.4, design `p2-4-background-compile`).
//!
//! A JIT compile splits into a FRONT that stays on the eval thread and a
//! BACKEND that needs nothing of the Lisp heap (`compile::shared::split`).
//! The front is today's pipeline up to the finished Cranelift function; it
//! reads the heap, the obarray and the eval thread's knobs, and it is
//! unchanged. The backend is Cranelift alone.
//!
//! `NEOVM_JIT_BG` ([`mode`], read once per process; tests override it per
//! thread) picks where the backend runs:
//!
//! | value | [`BgMode`] | what runs where |
//! |---|---|---|
//! | `legacy` (default) | `Legacy` | the persistent per-thread module defines each leaf in place (the B4 path, unchanged) |
//! | `sync` | `Sync` | the split, run in line: the eval thread's backend compiles each packaged function at once. Deterministic |
//!
//! Whatever the mode, the CLIF and the machine code are the same, and so is
//! everything Lisp can observe: the mode moves where code is produced, never
//! what it does.

use std::sync::OnceLock;

/// How JIT compiles run (`NEOVM_JIT_BG`). Exhaustive matches only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::IntoStaticStr)]
#[strum(serialize_all = "lowercase")]
pub(crate) enum BgMode {
    /// The persistent per-thread module defines in place (the B4 path).
    Legacy,
    /// The front/backend split, run in line on the eval thread.
    Sync,
}

/// The mode a `NEOVM_JIT_BG` value selects; anything unrecognised (or
/// unset) is [`BgMode::Legacy`].
pub(crate) fn parse_mode(value: Option<&str>) -> BgMode {
    match value.map(str::trim) {
        Some("sync") => BgMode::Sync,
        _ => BgMode::Legacy,
    }
}

/// This process's (or, under a test override, this thread's) mode.
pub(crate) fn mode() -> BgMode {
    #[cfg(test)]
    if let Some(mode) = MODE_TEST_OVERRIDE.with(std::cell::Cell::get) {
        return mode;
    }
    static MODE: OnceLock<BgMode> = OnceLock::new();
    *MODE.get_or_init(|| {
        let mode = parse_mode(std::env::var("NEOVM_JIT_BG").ok().as_deref());
        if mode != BgMode::Legacy {
            let name: &'static str = mode.into();
            tracing::info!(target: "neovm::jit::knobs", "NEOVM_JIT_BG={name} is on in this process");
        }
        mode
    })
}

/// Whether compiles take the front/backend split (every mode but
/// [`BgMode::Legacy`]). Read once per compile, never per call.
pub(crate) fn split_enabled() -> bool {
    match mode() {
        BgMode::Legacy => false,
        BgMode::Sync => true,
    }
}

#[cfg(test)]
thread_local! {
    static MODE_TEST_OVERRIDE: std::cell::Cell<Option<BgMode>> = const { std::cell::Cell::new(None) };
}

/// Force the mode for compiles on this thread (tests only); `None` returns
/// to the environment's.
#[cfg(test)]
pub(crate) fn force_mode_for_test(mode: Option<BgMode>) {
    MODE_TEST_OVERRIDE.with(|c| c.set(mode));
}

#[cfg(test)]
#[path = "bg/tests/split_test.rs"]
mod split_tests;
