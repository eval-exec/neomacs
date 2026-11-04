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

/// Presented regions for one window in an active publication.
fn window_regions_at(
    geometry: &crate::emacs_core::eval::data::window_state::PresentationGeometry,
    presentation: crate::emacs_core::eval::data::window_state::PresentationId,
    window: crate::emacs_core::display_metrics::WindowId,
) -> Option<crate::emacs_core::eval::data::window_state::WindowRegions> {
    use crate::window::geometry::{DisplayGeometryExt, WindowGeometryQuery};
    geometry
        .resolve(WindowGeometryQuery::new(presentation, window))
        .ok()
        .map(|view| view.regions())
}

static CANONICAL_WINDOW_HOOKS: DisplayWindowHooks = DisplayWindowHooks::new(
    crate::emacs_core::window_cmds::ensure_selected_frame_id_in_state,
    crate::emacs_core::window_cmds::remember_selected_window_point_in_state,
    crate::emacs_core::window_cmds::hook_window_body_dimensions,
    crate::emacs_core::window_cmds::builtin_resize_mini_window_internal,
    crate::emacs_core::hscroll::update_auto_hscroll_before_redisplay,
    crate::emacs_core::fontset::fontset_generation,
    window_domain_frame_of,
    window_regions_at,
    crate::emacs_core::xdisp::motion::resolve,
    crate::emacs_core::window_cmds::window_line_wrap,
    crate::emacs_core::window_cmds::builtin_window_body_width,
    crate::emacs_core::window_cmds::builtin_window_hscroll,
    crate::emacs_core::xdisp::invisible_status_for_value,
    crate::emacs_core::xdisp::invisible_source_run_end_byte,
);

static CANONICAL_DISPLAY_QUERY_HOOKS: DisplayQueryHooks = DisplayQueryHooks::new(
    super::redisplay_idle_skip_enabled,
    super::mode_line_flow_enabled,
    crate::face::realization::enabled,
    crate::face::realization::frame_is_tty_target,
    CANONICAL_WINDOW_HOOKS,
);

/// Publish the canonical display-policy table at the two construction seams.
/// The one canonical display-query descriptor.
pub(crate) fn canonical_display_query_hooks()
-> &'static crate::emacs_core::eval::display_query_hooks::DisplayQueryHooks {
    &CANONICAL_DISPLAY_QUERY_HOOKS
}

pub(crate) fn install_eval_display_query_hooks() {
    let _status = display_query_hooks::install_display_query_hooks(&CANONICAL_DISPLAY_QUERY_HOOKS)
        .expect("display-query hooks must publish once");
}
