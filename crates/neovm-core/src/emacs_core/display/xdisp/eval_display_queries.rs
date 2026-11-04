//! Core publication of the evaluator's cold display-policy queries (E9).
//!
//! The two selectors are display-engine policy; the evaluator only reads them
//! at redisplay boundaries through the PC2 table in
//! `emacs_core::eval::display_query_hooks`.

use crate::emacs_core::eval::display_query_hooks::{self, DisplayQueryHooks};

static CANONICAL_DISPLAY_QUERY_HOOKS: DisplayQueryHooks = DisplayQueryHooks::new(
    super::redisplay_idle_skip_enabled,
    super::mode_line_flow_enabled,
    crate::face::realization::enabled,
    crate::face::realization::frame_is_tty_target,
);

/// Publish the canonical display-policy table at the two construction seams.
pub(crate) fn install_eval_display_query_hooks() {
    let _status = display_query_hooks::install_display_query_hooks(&CANONICAL_DISPLAY_QUERY_HOOKS)
        .expect("display-query hooks must publish once");
}
