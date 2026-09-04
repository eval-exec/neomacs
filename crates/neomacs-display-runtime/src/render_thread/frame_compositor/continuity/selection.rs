//! Which window the frame's selection moved to.
//!
//! The producer used to report this as a `WindowSwitchFade` hint, naming an
//! effect. It is a plain fact about two presentations: each carries a
//! `selected` flag on every window it presents, so the compositor can see the
//! selection move without being told.

use neomacs_display_protocol::frame_glyphs::WindowInfo;
use neomacs_display_protocol::types::{DisplayWindowId, Rect};

/// The frame's selection moved to this window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::render_thread) struct SelectionObservation {
    pub(in crate::render_thread) window: DisplayWindowId,
    /// Where the newly selected window sits, for an effect to draw within.
    pub(in crate::render_thread) bounds: Rect,
}

/// The frame's selected text window, if it has one.
///
/// The minibuffer is excluded: selecting it is what happens on every `M-x`, and
/// treating that as a window switch would fire on nearly every command.
fn selected_text_window(windows: &[WindowInfo]) -> Option<&WindowInfo> {
    windows
        .iter()
        .find(|info| info.selected && !info.is_minibuffer)
}

/// Whether the selection moved between two presentations.
///
/// Returns `None` when either side has no selected text window — which covers
/// the minibuffer being selected on one side, and the first install, where
/// there is no previous presentation to have moved from.
pub(in crate::render_thread) fn observe_selection(
    previous: &[WindowInfo],
    next: &[WindowInfo],
) -> Option<SelectionObservation> {
    let previous = selected_text_window(previous)?;
    let next = selected_text_window(next)?;
    (previous.window_id != next.window_id).then_some(SelectionObservation {
        window: next.window_id,
        bounds: next.bounds,
    })
}

#[cfg(test)]
#[path = "selection_test.rs"]
mod tests;
