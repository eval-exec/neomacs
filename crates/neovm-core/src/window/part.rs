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

use super::Rect;

pub use crate::emacs_core::eval::data::window_state::{
    PosnArea, TextAreaCoordinate, WindowChromeLine, WindowCoordinate, WindowPart,
    WindowPartGeometry,
};

impl WindowChromeLine {
    pub const fn part(self) -> WindowPart {
        match self {
            Self::TabLine => WindowPart::TabLine,
            Self::HeaderLine => WindowPart::HeaderLine,
            Self::ModeLine => WindowPart::ModeLine,
        }
    }
}

impl WindowPart {
    /// The chrome line this part is, if it is one.
    pub const fn chrome_line(self) -> Option<WindowChromeLine> {
        match self {
            Self::TabLine => Some(WindowChromeLine::TabLine),
            Self::HeaderLine => Some(WindowChromeLine::HeaderLine),
            Self::ModeLine => Some(WindowChromeLine::ModeLine),
            _ => None,
        }
    }

    /// The text-area coordinate of a window-relative click on this part, for a
    /// caller whose region was resolved by something other than
    /// [`WindowPartGeometry::classify`].
    ///
    /// `None` for every part but [`Self::Text`], which is what keeps the
    /// guarantee: the presented-geometry hit test and test fixtures can make a
    /// witness, but only by holding a classification that says "text area",
    /// so no route reaches the buffer-position lookup with a mode-line or
    /// header-line coordinate.
    pub const fn text_area_coordinate(
        self,
        text_area_x: i64,
        window_y: i64,
        top_chrome_height: i64,
    ) -> Option<TextAreaCoordinate> {
        match self {
            Self::Text => Some(TextAreaCoordinate::new(
                text_area_x,
                window_y,
                top_chrome_height,
            )),
            _ => None,
        }
    }

    /// The symbol `make_lispy_position` puts in the posn's AREA slot for this
    /// part (src/keyboard.c:5862-5975).
    ///
    /// `None` for the text area, where GNU puts the buffer POSITION instead --
    /// `posn = make_fixnum (textpos)` at src/keyboard.c:6024 runs only while
    /// `posn` is still nil.
    pub const fn area_symbol(self) -> Option<&'static str> {
        match self {
            Self::Text => None,
            Self::ModeLine => Some("mode-line"),
            Self::VerticalBorder => Some("vertical-line"),
            Self::HeaderLine => Some("header-line"),
            Self::TabLine => Some("tab-line"),
            Self::LeftFringe => Some("left-fringe"),
            Self::RightFringe => Some("right-fringe"),
            Self::LeftMargin => Some("left-margin"),
            Self::RightMargin => Some("right-margin"),
            Self::VerticalScrollBar => Some("vertical-scroll-bar"),
            Self::HorizontalScrollBar => Some("horizontal-scroll-bar"),
            Self::RightDivider => Some("right-divider"),
            Self::BottomDivider => Some("bottom-divider"),
        }
    }
}

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
pub fn window_part_of_region(
    kind: neomacs_display_protocol::PresentedRegionKind,
) -> Option<WindowPart> {
    use neomacs_display_protocol::PresentedRegionKind as Kind;
    match kind {
        Kind::TextBody => Some(WindowPart::Text),
        Kind::LeftMargin => Some(WindowPart::LeftMargin),
        Kind::RightMargin => Some(WindowPart::RightMargin),
        Kind::LeftFringe => Some(WindowPart::LeftFringe),
        Kind::RightFringe => Some(WindowPart::RightFringe),
        // GNU has one vertical scroll bar part; which side it is drawn on is a
        // frame parameter, not a different posn.
        Kind::LeftScrollBar | Kind::RightScrollBar => Some(WindowPart::VerticalScrollBar),
        Kind::HorizontalScrollBar => Some(WindowPart::HorizontalScrollBar),
        Kind::TabLine => Some(WindowPart::TabLine),
        Kind::HeaderLine => Some(WindowPart::HeaderLine),
        Kind::ModeLine => Some(WindowPart::ModeLine),
        Kind::RightDivider => Some(WindowPart::RightDivider),
        Kind::BottomDivider => Some(WindowPart::BottomDivider),
        Kind::MenuBar | Kind::ToolBar | Kind::CompactBar | Kind::TabBar => None,
    }
}

impl PosnArea {
    pub const fn symbol_name(self) -> Option<&'static str> {
        match self {
            Self::Window(part) => part.area_symbol(),
            Self::MenuBar => Some("menu-bar"),
            Self::ToolBar => Some("tool-bar"),
            Self::TabBar => Some("tab-bar"),
        }
    }
}

impl TextAreaCoordinate {
    /// X relative to the text area's left edge -- GNU's `xret = mx -
    /// window_box_left (w, TEXT_AREA)` (src/keyboard.c:5882), which is also the
    /// `x2` it hands to `buffer_posn_from_coords` (src/keyboard.c:5985-5991).
    pub const fn text_area_x(self) -> i64 {
        self.text_area_x
    }

    /// Y relative to the WINDOW's top edge, tab and header lines included.
    /// GNU passes `y2 = wy` (src/keyboard.c:5992), not the text-area-relative
    /// value it reports in the posn.
    pub const fn window_y(self) -> i64 {
        self.window_y
    }

    /// Y relative to the top of the text area -- GNU's `yret = wy -
    /// WINDOW_TAB_LINE_HEIGHT (w) - WINDOW_HEADER_LINE_HEIGHT (w)`
    /// (src/keyboard.c:5883), which is the value that reaches the posn and
    /// that `posn-col-row` divides the character cell out of.
    pub const fn text_area_y(self) -> i64 {
        self.text_area_y
    }

    const fn new(text_area_x: i64, window_y: i64, top_chrome_height: i64) -> Self {
        Self {
            text_area_x,
            window_y,
            text_area_y: window_y - top_chrome_height,
        }
    }
}

impl WindowCoordinate {
    pub const fn part(self) -> WindowPart {
        match self {
            Self::ChromeLine { line, .. } => line.part(),
            Self::Buffer { part, .. } => part,
        }
    }
}

impl WindowPartGeometry {
    /// Build the classifier's inputs from a window's frame-relative bounds and
    /// the chrome geometry the last redisplay published for it.
    ///
    /// `text_area_left_offset` and `text_area_width` describe the horizontal
    /// box; passing the window's whole width for the latter is the terminal
    /// case, where a window has no fringes and by default no margins.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        bounds: Rect,
        tab_line_height: i64,
        header_line_height: i64,
        mode_line_height: i64,
        text_area_left_offset: i64,
        text_area_width: i64,
        left_margin_width: i64,
        right_margin_width: i64,
        column_width: i64,
        rightmost: bool,
    ) -> Self {
        let left_x = bounds.x.round() as i64;
        let top_y = bounds.y.round() as i64;
        Self {
            left_x,
            right_x: left_x + bounds.width.round() as i64,
            top_y,
            bottom_y: top_y + bounds.height.round() as i64,
            tab_line_height: tab_line_height.max(0),
            header_line_height: header_line_height.max(0),
            mode_line_height: mode_line_height.max(0),
            text_area_left_offset: text_area_left_offset.max(0),
            text_area_width: text_area_width.max(0),
            left_margin_width: left_margin_width.max(0),
            right_margin_width: right_margin_width.max(0),
            column_width: column_width.max(1),
            rightmost,
        }
    }

    /// Height of the tab and header lines together -- the offset between a
    /// window-relative Y and a text-area-relative one.
    pub const fn top_chrome_height(self) -> i64 {
        self.tab_line_height + self.header_line_height
    }

    /// GNU `coordinates_in_window` (src/window.c:1348-1489). X and Y are
    /// frame-relative pixels; `None` is `ON_NOTHING`.
    ///
    /// The branch order is GNU's, and it is load-bearing: the mode line is
    /// tested before the text area, so the bottom row of a window is never a
    /// text coordinate, and the tab and header lines are tested before it too,
    /// so Y = 0 in a window with a header line IS the header line. GNU's own
    /// `posn-at-x-y` doc string says as much -- "the text area includes the
    /// header-line and the tab-line of the window".
    pub fn classify(self, x: i64, y: i64) -> Option<WindowPart> {
        // "Outside any interesting row or column?" (src/window.c:1360-1362)
        if y < self.top_y || y >= self.bottom_y || x < self.left_x || x >= self.right_x {
            return None;
        }

        // The mode/tab/header line test, src/window.c:1389-1421. GNU's own
        // `||` chain: whichever matches first wins, and the mode line is first.
        if self.mode_line_height > 0 && y >= self.bottom_y - self.mode_line_height {
            return Some(WindowPart::ModeLine);
        }
        if self.tab_line_height > 0 && y < self.top_y + self.tab_line_height {
            return Some(WindowPart::TabLine);
        }
        if self.header_line_height > 0 && y < self.top_y + self.top_chrome_height() {
            return Some(WindowPart::HeaderLine);
        }

        let box_left = self.left_x;
        let box_right = self.right_x - 1;
        if x < box_left || x > box_right {
            return Some(WindowPart::VerticalScrollBar);
        }

        // "Need to say x > right_x rather than >=, since on character
        // terminals, the vertical line's x coordinate is right_x."
        // (src/window.c:1455-1462)
        if !self.rightmost && x > box_right - self.column_width {
            return Some(WindowPart::VerticalBorder);
        }

        let text_left = self.left_x + self.text_area_left_offset;
        let text_right = text_left + self.text_area_width;
        if x < text_left {
            if self.left_margin_width > 0 && x < box_left + self.left_margin_width {
                return Some(WindowPart::LeftMargin);
            }
            return Some(WindowPart::LeftFringe);
        }
        if x >= text_right {
            if self.right_margin_width > 0 && x >= box_right - self.right_margin_width {
                return Some(WindowPart::RightMargin);
            }
            return Some(WindowPart::RightFringe);
        }

        // "Everything special ruled out - must be on text area"
        Some(WindowPart::Text)
    }

    /// Classify a frame-relative coordinate and carry it into the branch
    /// `make_lispy_position` takes for that part (src/keyboard.c:5862-6000).
    pub fn resolve(self, x: i64, y: i64) -> Option<WindowCoordinate> {
        let part = self.classify(x, y)?;
        let window_x = x - self.left_x;
        let window_y = y - self.top_y;
        if let Some(line) = part.chrome_line() {
            return Some(WindowCoordinate::ChromeLine {
                line,
                window_x,
                window_y,
            });
        }
        // GNU's `x2`: the text-area-relative X for the text area and for the
        // parts that lie to the RIGHT of it, and zero for everything on the
        // left, where a click carries no column of its own
        // (src/keyboard.c:5985-5991). `y2` is `wy` in every case.
        let text_area_x = match part {
            WindowPart::Text
            | WindowPart::RightFringe
            | WindowPart::RightMargin
            | WindowPart::VerticalScrollBar => window_x - self.text_area_left_offset,
            _ => 0,
        };
        Some(WindowCoordinate::Buffer {
            part,
            window_x,
            window_y,
            at: TextAreaCoordinate::new(text_area_x, window_y, self.top_chrome_height()),
        })
    }
}

#[cfg(test)]
#[path = "part/tests/part_test.rs"]
mod tests;
