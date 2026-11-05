//! Which part of which window a frame-relative coordinate falls on.
//!
//! GNU asks this question ONCE, in `window_from_coordinates`
//! (src/window.c:1686-1750) via `coordinates_in_window` (src/window.c:1348-1489),
//! and every consumer of a mouse-shaped coordinate goes through it:
//! `make_lispy_position` (src/keyboard.c:5793) builds the posn for a real mouse
//! event and for `posn-at-x-y` alike, `note_mouse_highlight` asks it for
//! `help-echo`, and `Fcoordinates_in_window_p` exposes it to Lisp.
//!
//! The order matters and is the whole of ledger 205's residual 2:
//! `window_from_coordinates` runs BEFORE any buffer position is looked up, and
//! `make_lispy_position` branches on `ON_MODE_LINE` / `ON_HEADER_LINE` /
//! `ON_TAB_LINE` / margins / fringes first (src/keyboard.c:5862-5975). A port
//! that pins the window from its caller and goes straight to a row lookup has
//! no answer for a mode-line coordinate, and no way to notice that the
//! coordinate belongs to a different window than the one it was handed.
//!
//! This module is that step, and the types carry the order:
//!
//! * [`WindowPart`] is GNU's `enum window_part` minus `ON_NOTHING`, which is
//!   `None` here so "not in this window" cannot be mistaken for a part.
//! * [`WindowCoordinate`] is GNU's split inside `make_lispy_position`: the
//!   three chrome lines set `textpos = -1` and answer from `mode_line_string`,
//!   and EVERY other part leaves `textpos` at 0 and therefore falls into
//!   `if (!textpos)`, which runs `buffer_posn_from_coords`.
//! * [`TextAreaCoordinate`] is the witness that a classification happened. It
//!   has no public constructor, and the snapshot row lookup takes nothing
//!   else, so a mode-line or header-line coordinate cannot reach a buffer
//!   position at all.

pub use neovm_eval::emacs_core::eval::data::window_state::{
    PosnArea, TextAreaCoordinate, WindowChromeLine, WindowCoordinate, WindowPart,
    WindowPartGeometry,
};

/// The window part a presented region names, if it names one at all.
///
/// This is the *only* conversion from the display protocol's region vocabulary
/// into GNU's posn vocabulary. It exists because there were two: this file's
/// GNU-sourced `area_symbol` table, and a second hand-written match in
/// `keyboard.rs` that had
/// drifted — it reported a bottom divider as `horizontal-scroll-bar` and a
/// right divider as `vertical-line`. The first of those meant
/// `[bottom-divider down-mouse-1]`, which GNU binds to `mouse-drag-mode-line`
/// (lisp/mouse.el:3825), never matched, so dragging a bottom divider to resize
/// windows did nothing at all.
///
/// `None` for regions that are not part of a window: the menu bar, tool bar and
/// tab bar replace the window with the frame entirely in GNU's posn
/// (src/keyboard.c:5799-5852), so they have no `WindowPart` to name.
#[must_use]
pub use neovm_eval::emacs_core::eval::data::posn_object_extent::window_part_of_region;

#[cfg(test)]
#[path = "part/tests/part_test.rs"]
mod tests;
