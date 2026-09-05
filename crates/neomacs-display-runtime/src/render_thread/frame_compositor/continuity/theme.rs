//! Whether the frame's theme changed between two presentations.
//!
//! The producer reported this as a `ThemeTransition` hint. Both sides of the
//! comparison ship in the presentations themselves — `FrameGlyphBuffer` carries
//! the resolved `background` — so the compositor can see the change without
//! being told, and without the layout engine having to retain a copy of the
//! previous frame's background to diff against.

use crate::core::frame_glyphs::FrameGlyphBuffer;
use neomacs_display_protocol::types::Rect;

/// How far a background colour channel must move to count as a theme change.
///
/// Small drifts are not a theme: a face recomputation can nudge a channel
/// without the user having changed anything, and crossfading the whole frame
/// for that is worse than doing nothing.
const THEME_COLOR_EPSILON: f32 = 0.02;

/// The frame's theme changed, and this is the area to crossfade.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::render_thread) struct ThemeChange {
    pub(in crate::render_thread) bounds: Rect,
}

/// Height of the frame above the minibuffer, or the whole frame if it has none.
///
/// The minibuffer is excluded from the crossfade: it is drawn from the echo
/// area's own state and does not participate in a theme transition.
fn content_height_before_minibuffer(frame: &FrameGlyphBuffer) -> f32 {
    frame
        .window_infos
        .iter()
        .find(|info| info.is_minibuffer)
        .map_or(frame.height, |info| info.bounds.y)
}

/// Whether the theme changed from `previous` to `next`.
///
/// Compares only the colour channels. Alpha is deliberately excluded: frame
/// opacity is a window-manager property a user may animate on its own, and
/// treating it as a theme change would crossfade the frame every time it faded.
pub(in crate::render_thread) fn theme_change(
    previous: &FrameGlyphBuffer,
    next: &FrameGlyphBuffer,
) -> Option<ThemeChange> {
    let before = previous.background;
    let after = next.background;
    let changed = (after.r - before.r).abs() > THEME_COLOR_EPSILON
        || (after.g - before.g).abs() > THEME_COLOR_EPSILON
        || (after.b - before.b).abs() > THEME_COLOR_EPSILON;
    changed.then(|| ThemeChange {
        bounds: Rect::new(0.0, 0.0, next.width, content_height_before_minibuffer(next)),
    })
}

#[cfg(test)]
#[path = "theme_test.rs"]
mod tests;
