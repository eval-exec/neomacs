//! Core publication of the evaluator's cold display-policy queries (E9).
//!
//! The two selectors are display-engine policy; the evaluator only reads them
//! at redisplay boundaries through the PC2 table in
//! `emacs_core::eval::display_query_hooks`.

use crate::emacs_core::eval::display_query_hooks::{self, DisplayQueryHooks, DisplayWindowHooks};

/// GNU's frame-domain window lookup, as the display table publishes it.
fn window_domain_frame_of(
    domain: crate::emacs_core::eval::data::window_state::WindowDomain,
    frames: &crate::emacs_core::eval::data::window_state::FrameManager,
    window: crate::emacs_core::display_metrics::WindowId,
) -> Option<neovm_value::display_vocab::FrameId> {
    use crate::emacs_core::window_cmds::WindowDomainFrameOf;
    domain.frame_of(frames, window)
}

static CANONICAL_WINDOW_HOOKS: DisplayWindowHooks = DisplayWindowHooks::new(
    crate::emacs_core::window_cmds::ensure_selected_frame_id_in_state,
    crate::emacs_core::window_cmds::remember_selected_window_point_in_state,
    crate::emacs_core::window_cmds::hook_window_body_dimensions,
    crate::emacs_core::window_cmds::builtin_resize_mini_window_internal,
    crate::emacs_core::hscroll::update_auto_hscroll_before_redisplay,
    crate::emacs_core::fontset::fontset_generation,
    window_domain_frame_of,
);

static CANONICAL_DISPLAY_QUERY_HOOKS: DisplayQueryHooks = DisplayQueryHooks::new(
    super::redisplay_idle_skip_enabled,
    super::mode_line_flow_enabled,
    crate::face::realization::enabled,
    crate::face::realization::frame_is_tty_target,
    CANONICAL_WINDOW_HOOKS,
);

/// Publish the canonical display-policy table at the two construction seams.
pub(crate) fn install_eval_display_query_hooks() {
    let _status = display_query_hooks::install_display_query_hooks(&CANONICAL_DISPLAY_QUERY_HOOKS)
        .expect("display-query hooks must publish once");
}
