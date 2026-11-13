//! GNU's 25-slot `standard_bitmaps[]` table, transcribed verbatim from
//! `src/fringe.c` (`standard_bitmaps[]`, the `*_bits[]` row arrays, and the
//! `MAX_STANDARD_FRINGE_BITMAPS` / index ordering).
//!
//! GNU keeps the built-in fringe bitmaps in a fixed table; slot 0 is
//! `NO_FRINGE_BITMAP` and the 24 named bitmaps occupy indices 1..=24. The
//! integer index is stashed on each bitmap symbol's `'fringe` plist property.
//! `lisp/fringe.el` re-`put`s the same indices (its bitmap list, with `bn`
//! starting at 1, is in the SAME order as this C table), so the C and Lisp
//! numbering agree. `max_used_fringe_bitmap` starts at
//! `MAX_STANDARD_FRINGE_BITMAPS == 25`, so the first user bitmap defined via
//! `define-fringe-bitmap` gets index 25 (`FIRST_USER_FRINGE_BITMAP_INDEX`).
//!
//! ROWS are stored here exactly as in fringe.c — the visible pixels live in the
//! low `width` bits (e.g. `0x3c` for an 8-wide bitmap). The registration step
//! (`FringeBitmapRegistry::pre_register_standard_bitmaps`) runs each row through
//! `parse_bits_rows`, which shifts the visible bits up to be MSB-aligned (the
//! convention the renderer reads: column `b` = `(bits >> (15 - b)) & 1`).
//!
//! `empty-line` is PERIODIC: GNU stores it with `period == 3` and the full
//! 72-row pre-expanded pattern (a 3-row tile repeated 24 times). We keep the
//! same: `period = 3`, all 72 rows verbatim. The single-tile vs full-expansion
//! distinction only matters to the (later) periodic-rendering stage; storing the
//! GNU rows verbatim keeps the data faithful for spot-checking against fringe.c.

use super::fringe_bitmap::FringeBitmapAlign;

/// A standard built-in fringe bitmap, transcribed from fringe.c. `rows` are the
/// raw GNU values (visible pixels in the low `width` bits); the registration
/// step MSB-aligns them.
pub(crate) struct StandardFringeBitmap {
    /// Bitmap symbol name (the physical-bitmap symbol the `'fringe` index hangs
    /// on, e.g. `"right-arrow"`).
    pub name: &'static str,
    /// GNU index = position in `standard_bitmaps[]` (1..=24; slot 0 is
    /// `NO_FRINGE_BITMAP`). MUST match fringe.c exactly.
    pub index: u32,
    /// Pixel width (GNU's `width` field; always 8 for the standard bitmaps).
    pub width: u8,
    /// Row count (GNU's `height` field == number of entries in the `*_bits[]`
    /// array).
    pub height: u8,
    /// Repeat period (GNU's `period`; 0 = not periodic, 3 for `empty-line`).
    pub period: u8,
    /// Vertical alignment within the row.
    pub align: FringeBitmapAlign,
    /// Raw GNU bitmap rows (low-bits), one u32 per row.
    pub rows: &'static [u32],
}

use FringeBitmapAlign::{Bottom, Center, Top};

// ---------------------------------------------------------------------------
// Row arrays — transcribed verbatim from src/fringe.c.
// ---------------------------------------------------------------------------

/// question_mark_bits (fringe.c) — height 10.
const QUESTION_MARK: &[u32] = &[0x3c, 0x7e, 0xc3, 0xc3, 0x0c, 0x18, 0x18, 0x00, 0x18, 0x18];

/// exclamation_mark_bits — height 10.
const EXCLAMATION_MARK: &[u32] = &[0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00, 0x18, 0x18];

/// left_arrow_bits — height 8.
const LEFT_ARROW: &[u32] = &[0x18, 0x30, 0x60, 0xfc, 0xfc, 0x60, 0x30, 0x18];

/// right_arrow_bits — height 8.
const RIGHT_ARROW: &[u32] = &[0x18, 0x0c, 0x06, 0x3f, 0x3f, 0x06, 0x0c, 0x18];

/// up_arrow_bits — height 8.
const UP_ARROW: &[u32] = &[0x18, 0x3c, 0x7e, 0xff, 0x18, 0x18, 0x18, 0x18];

/// down_arrow_bits — height 8.
const DOWN_ARROW: &[u32] = &[0x18, 0x18, 0x18, 0x18, 0xff, 0x7e, 0x3c, 0x18];

/// left_curly_arrow_bits — height 8.
const LEFT_CURLY_ARROW: &[u32] = &[0x3c, 0x7c, 0xc0, 0xe4, 0xfc, 0x7c, 0x3c, 0x7c];

/// right_curly_arrow_bits — height 8.
const RIGHT_CURLY_ARROW: &[u32] = &[0x3c, 0x3e, 0x03, 0x27, 0x3f, 0x3e, 0x3c, 0x3e];

/// large_circle_bits — height 8.
const LARGE_CIRCLE: &[u32] = &[0x3c, 0x7e, 0xff, 0xff, 0xff, 0xff, 0x7e, 0x3c];

/// left_triangle_bits — height 8.
const LEFT_TRIANGLE: &[u32] = &[0x03, 0x0f, 0x1f, 0x3f, 0x3f, 0x1f, 0x0f, 0x03];

/// right_triangle_bits — height 8.
const RIGHT_TRIANGLE: &[u32] = &[0xc0, 0xf0, 0xf8, 0xfc, 0xfc, 0xf8, 0xf0, 0xc0];

/// top_left_angle_bits — height 8.
const TOP_LEFT_ANGLE: &[u32] = &[0xfc, 0xfc, 0xc0, 0xc0, 0xc0, 0xc0, 0xc0, 0x00];

/// top_right_angle_bits — height 8.
const TOP_RIGHT_ANGLE: &[u32] = &[0x3f, 0x3f, 0x03, 0x03, 0x03, 0x03, 0x03, 0x00];

/// bottom_left_angle_bits — height 8.
const BOTTOM_LEFT_ANGLE: &[u32] = &[0x00, 0xc0, 0xc0, 0xc0, 0xc0, 0xc0, 0xfc, 0xfc];

/// bottom_right_angle_bits — height 8.
const BOTTOM_RIGHT_ANGLE: &[u32] = &[0x00, 0x03, 0x03, 0x03, 0x03, 0x03, 0x3f, 0x3f];

/// left_bracket_bits — height 10.
const LEFT_BRACKET: &[u32] = &[0xfc, 0xfc, 0xc0, 0xc0, 0xc0, 0xc0, 0xc0, 0xc0, 0xfc, 0xfc];

/// right_bracket_bits — height 10.
const RIGHT_BRACKET: &[u32] = &[0x3f, 0x3f, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x3f, 0x3f];

/// filled_rectangle_bits — height 13.
const FILLED_RECTANGLE: &[u32] = &[
    0xfe, 0xfe, 0xfe, 0xfe, 0xfe, 0xfe, 0xfe, 0xfe, 0xfe, 0xfe, 0xfe, 0xfe, 0xfe,
];

/// hollow_rectangle_bits — height 13.
const HOLLOW_RECTANGLE: &[u32] = &[
    0xfe, 0x82, 0x82, 0x82, 0x82, 0x82, 0x82, 0x82, 0x82, 0x82, 0x82, 0x82, 0xfe,
];

/// filled_square_bits — height 6.
const FILLED_SQUARE: &[u32] = &[0x7e, 0x7e, 0x7e, 0x7e, 0x7e, 0x7e];

/// hollow_square_bits — height 6.
const HOLLOW_SQUARE: &[u32] = &[0x7e, 0x42, 0x42, 0x42, 0x42, 0x7e];

/// vertical_bar_bits — height 13.
const VERTICAL_BAR: &[u32] = &[
    0xc0, 0xc0, 0xc0, 0xc0, 0xc0, 0xc0, 0xc0, 0xc0, 0xc0, 0xc0, 0xc0, 0xc0, 0xc0,
];

/// horizontal_bar_bits — height 2.
const HORIZONTAL_BAR: &[u32] = &[0xfe, 0xfe];

/// empty_line_bits — height 72, PERIODIC (period 3). The 3-row tile
/// `{0x00, 0x3c, 0x00}` repeated 24 times, transcribed verbatim from fringe.c.
const EMPTY_LINE: &[u32] = &[
    0x00, 0x3c, 0x00, 0x00, 0x3c, 0x00, 0x00, 0x3c, 0x00, //
    0x00, 0x3c, 0x00, 0x00, 0x3c, 0x00, 0x00, 0x3c, 0x00, //
    0x00, 0x3c, 0x00, 0x00, 0x3c, 0x00, 0x00, 0x3c, 0x00, //
    0x00, 0x3c, 0x00, 0x00, 0x3c, 0x00, 0x00, 0x3c, 0x00, //
    0x00, 0x3c, 0x00, 0x00, 0x3c, 0x00, 0x00, 0x3c, 0x00, //
    0x00, 0x3c, 0x00, 0x00, 0x3c, 0x00, 0x00, 0x3c, 0x00, //
    0x00, 0x3c, 0x00, 0x00, 0x3c, 0x00, 0x00, 0x3c, 0x00, //
    0x00, 0x3c, 0x00, 0x00, 0x3c, 0x00, 0x00, 0x3c, 0x00,
];

/// The 24 standard built-in fringe bitmaps, in GNU's `standard_bitmaps[]` order
/// (index 1..=24). Slot 0 (`NO_FRINGE_BITMAP`) is omitted — it has no symbol.
///
/// NOTE: the order here MUST match fringe.c's `standard_bitmaps[]` AND
/// fringe.el's bitmap list (which `put`s `'fringe` 1..24 in the same order),
/// because downstream code (`fringe-indicator-alist`, oracle parity tests)
/// references the symbols and expects `index_of(sym)` to agree with GNU.
pub(crate) const STANDARD_FRINGE_BITMAPS: &[StandardFringeBitmap] = &[
    StandardFringeBitmap {
        name: "question-mark",
        index: 1,
        width: 8,
        height: 10,
        period: 0,
        align: Center,
        rows: QUESTION_MARK,
    },
    StandardFringeBitmap {
        name: "exclamation-mark",
        index: 2,
        width: 8,
        height: 10,
        period: 0,
        align: Center,
        rows: EXCLAMATION_MARK,
    },
    StandardFringeBitmap {
        name: "left-arrow",
        index: 3,
        width: 8,
        height: 8,
        period: 0,
        align: Center,
        rows: LEFT_ARROW,
    },
    StandardFringeBitmap {
        name: "right-arrow",
        index: 4,
        width: 8,
        height: 8,
        period: 0,
        align: Center,
        rows: RIGHT_ARROW,
    },
    StandardFringeBitmap {
        name: "up-arrow",
        index: 5,
        width: 8,
        height: 8,
        period: 0,
        align: Top,
        rows: UP_ARROW,
    },
    StandardFringeBitmap {
        name: "down-arrow",
        index: 6,
        width: 8,
        height: 8,
        period: 0,
        align: Bottom,
        rows: DOWN_ARROW,
    },
    StandardFringeBitmap {
        name: "left-curly-arrow",
        index: 7,
        width: 8,
        height: 8,
        period: 0,
        align: Center,
        rows: LEFT_CURLY_ARROW,
    },
    StandardFringeBitmap {
        name: "right-curly-arrow",
        index: 8,
        width: 8,
        height: 8,
        period: 0,
        align: Center,
        rows: RIGHT_CURLY_ARROW,
    },
    StandardFringeBitmap {
        name: "large-circle",
        index: 9,
        width: 8,
        height: 8,
        period: 0,
        align: Center,
        rows: LARGE_CIRCLE,
    },
    StandardFringeBitmap {
        name: "left-triangle",
        index: 10,
        width: 8,
        height: 8,
        period: 0,
        align: Center,
        rows: LEFT_TRIANGLE,
    },
    StandardFringeBitmap {
        name: "right-triangle",
        index: 11,
        width: 8,
        height: 8,
        period: 0,
        align: Center,
        rows: RIGHT_TRIANGLE,
    },
    StandardFringeBitmap {
        name: "top-left-angle",
        index: 12,
        width: 8,
        height: 8,
        period: 0,
        align: Top,
        rows: TOP_LEFT_ANGLE,
    },
    StandardFringeBitmap {
        name: "top-right-angle",
        index: 13,
        width: 8,
        height: 8,
        period: 0,
        align: Top,
        rows: TOP_RIGHT_ANGLE,
    },
    StandardFringeBitmap {
        name: "bottom-left-angle",
        index: 14,
        width: 8,
        height: 8,
        period: 0,
        align: Bottom,
        rows: BOTTOM_LEFT_ANGLE,
    },
    StandardFringeBitmap {
        name: "bottom-right-angle",
        index: 15,
        width: 8,
        height: 8,
        period: 0,
        align: Bottom,
        rows: BOTTOM_RIGHT_ANGLE,
    },
    StandardFringeBitmap {
        name: "left-bracket",
        index: 16,
        width: 8,
        height: 10,
        period: 0,
        align: Center,
        rows: LEFT_BRACKET,
    },
    StandardFringeBitmap {
        name: "right-bracket",
        index: 17,
        width: 8,
        height: 10,
        period: 0,
        align: Center,
        rows: RIGHT_BRACKET,
    },
    StandardFringeBitmap {
        name: "filled-rectangle",
        index: 18,
        width: 8,
        height: 13,
        period: 0,
        align: Center,
        rows: FILLED_RECTANGLE,
    },
    StandardFringeBitmap {
        name: "hollow-rectangle",
        index: 19,
        width: 8,
        height: 13,
        period: 0,
        align: Center,
        rows: HOLLOW_RECTANGLE,
    },
    StandardFringeBitmap {
        name: "filled-square",
        index: 20,
        width: 8,
        height: 6,
        period: 0,
        align: Center,
        rows: FILLED_SQUARE,
    },
    StandardFringeBitmap {
        name: "hollow-square",
        index: 21,
        width: 8,
        height: 6,
        period: 0,
        align: Center,
        rows: HOLLOW_SQUARE,
    },
    StandardFringeBitmap {
        name: "vertical-bar",
        index: 22,
        width: 8,
        height: 13,
        period: 0,
        align: Center,
        rows: VERTICAL_BAR,
    },
    StandardFringeBitmap {
        name: "horizontal-bar",
        index: 23,
        width: 8,
        height: 2,
        period: 0,
        align: Bottom,
        rows: HORIZONTAL_BAR,
    },
    StandardFringeBitmap {
        name: "empty-line",
        index: 24,
        width: 8,
        height: 72,
        period: 3,
        align: Top,
        rows: EMPTY_LINE,
    },
];

/// GNU's `MAX_STANDARD_FRINGE_BITMAPS` = `ARRAYELTS(standard_bitmaps)` = 25
/// (slot 0 + the 24 named bitmaps). The first user bitmap index is this value.
pub(crate) const MAX_STANDARD_FRINGE_BITMAPS: u32 = 25;
