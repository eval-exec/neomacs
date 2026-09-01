//! Typed Lisp work discovered at a layout boundary.
//!
//! The closed enum makes every callback site explicit. Scroll hooks execute at
//! their GNU leaf-local position; synchronous display queries made by those
//! hooks are routed through the redisplay runtime's disjoint query engine.

use neovm_core::window::WindowId;

use crate::buffer_source::window_source::ResolvedWindowStart;

/// Exact GNU callback site acknowledged by one logical redisplay.
///
/// A visibility retry can choose another start for the same window and must
/// still run its hook. Keying the ledger by this typed pair prevents a retry
/// from either replaying one site or suppressing a distinct one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct WindowScrollHookSite {
    window_id: WindowId,
    window_start: ResolvedWindowStart,
}

impl WindowScrollHookSite {
    pub(crate) const fn new(window_id: WindowId, window_start: ResolvedWindowStart) -> Self {
        Self {
            window_id,
            window_start,
        }
    }
}

/// Work that must cross from layout back into the Lisp evaluator.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LayoutEffect {
    RunWindowScrollFunctions(WindowScrollEffect),
}

impl LayoutEffect {
    pub(crate) const fn scroll_hook_site(&self) -> WindowScrollHookSite {
        match self {
            Self::RunWindowScrollFunctions(effect) => effect.site,
        }
    }

    /// Execute an effect at its GNU leaf-local position.
    ///
    /// The production redisplay runtime gives synchronous display queries a
    /// disjoint query engine, so a scroll hook can enter `window-end` without
    /// recursively borrowing the presentation engine. Keeping this operation
    /// on the typed effect prevents an arbitrary callback from being smuggled
    /// into the row producer while preserving GNU's per-leaf Lisp order.
    pub(crate) fn execute_inline(self, evaluator: &mut neovm_core::emacs_core::Context) {
        match self {
            Self::RunWindowScrollFunctions(effect) => effect.execute(evaluator),
        }
    }
}

/// GNU `run_window_scroll_functions` after redisplay commits a new start.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct WindowScrollEffect {
    site: WindowScrollHookSite,
}

impl WindowScrollEffect {
    pub(crate) const fn new(site: WindowScrollHookSite) -> Self {
        Self { site }
    }

    fn execute(self, evaluator: &mut neovm_core::emacs_core::Context) {
        evaluator.run_window_scroll_functions_for_committed_start(self.site.window_id);
    }
}
