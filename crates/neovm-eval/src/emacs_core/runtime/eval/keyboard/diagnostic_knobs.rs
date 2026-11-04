//! Immutable process diagnostics for the keyboard runtime.
//!
//! These switches observe the process environment on first use. They cannot
//! change editor policy or Lisp-visible environment values. The shared
//! OnceLock publishes one immutable value to every mutator; no TLS or Context
//! state is added, and later environment changes do not reconfigure logging.

use std::sync::OnceLock;

/// Whether verbose opening-frame synchronization diagnostics were requested.
/// Only the exact Unicode string "1" enables the existing DEBUG messages.
#[inline]
pub(super) fn trace_host_sync_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        #[cfg(test)]
        HOST_SYNC_ENV_READS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        std::env::var("NEOMACS_TRACE_HOST_SYNC")
            .ok()
            .is_some_and(|value| value == "1")
    })
}

#[cfg(test)]
static HOST_SYNC_ENV_READS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
#[path = "tests/diagnostic_knobs_test.rs"]
mod tests;
