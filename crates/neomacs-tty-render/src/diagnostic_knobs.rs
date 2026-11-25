//! Process-level TTY diagnostics, independent of terminal or Lisp state.
//!
//! OnceLock publishes the first-use environment presence to all mutators.
//! Presence deliberately includes empty and non-Unicode values. This adds no
//! TLS, terminal field or per-edit maintenance; all dump sites remain intact.

use std::sync::OnceLock;

#[inline]
pub(super) fn dump_tty_glyphs_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        #[cfg(test)]
        ENV_READS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        std::env::var_os("NEOMACS_DUMP_TTY_GLYPHS").is_some()
    })
}

#[cfg(test)]
static ENV_READS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
#[path = "tests/diagnostic_knobs_test.rs"]
mod tests;
