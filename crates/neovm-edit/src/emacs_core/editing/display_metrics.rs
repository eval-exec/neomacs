//! Display-owned layout vocabulary and pure metrics shared with the edit layer.
//!
//! Stage 3 places the edit layer below display. Everything here is plain data
//! or a pure function over value/buffer types — line-wrap modes, the motion
//! engine selector, the invisible-run context, packed display-glyph decoding,
//! and the line-number gutter width — so it lives with edit and the display
//! layer reads it downward. Live display state (frames, windows, retained
//! glyph rows) is reached only through the `EditHost` display queries.

use crate::buffer::{Buffer, BufferId, EmacsBytePos, LispCharPos1};
use neovm_value::emacs_char::EmacsChar;
use neovm_value::value::{Value, ValueKind};

// ---------------------------------------------------------------------------
// Window identity
// ---------------------------------------------------------------------------

/// Opaque window identifier.
///
/// Plain data: an id owns no display state. It lives here so the edit layer can
/// name a window designator without reaching into the display crate; the window
/// module re-exports it under its historical public path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WindowId(pub u64);

// ---------------------------------------------------------------------------
// Line layout vocabulary
// ---------------------------------------------------------------------------

/// How a display line ends in one window -- GNU `enum line_wrap_method`
/// (src/dispextern.h), the `it->line_wrap` that `init_iterator` resolves once
/// per window+buffer pair (src/xdisp.c:3413-3426):
///
/// ```c
///   if (TRUNCATE != 0)
///     it->line_wrap = TRUNCATE;
///   if (base_face_id == DEFAULT_FACE_ID
///       && !it->w->hscroll
///       && (WINDOW_FULL_WIDTH_P (it->w)
///           || NILP (Vtruncate_partial_width_windows)
///           || (FIXNUMP (Vtruncate_partial_width_windows)
///               && (XFIXNUM (Vtruncate_partial_width_windows)
///                   <= WINDOW_TOTAL_COLS (it->w))))
///       && NILP (BVAR (current_buffer, truncate_lines)))
///     it->line_wrap = NILP (BVAR (current_buffer, word_wrap))
///       ? WINDOW_WRAP : WORD_WRAP;
/// ```
///
/// Every screen-line question downstream -- `vertical-motion`,
/// `beginning-of-visual-line`, `end-of-visual-line`, `next-line` /
/// `previous-line` under `line-move-visual`, `move-to-window-line`,
/// `count-screen-lines` -- is answered against this one value.
///
/// It is an enum rather than a `truncate: bool` on purpose.  A boolean can
/// spell only two of GNU's three methods, so a scanner taking one had no way
/// to represent "break at the last word boundary" and silently answered
/// `WindowWrap` instead; and "not truncated" read as a single bit made it easy
/// for the truncation half of the decision to be computed from the GLOBAL
/// `truncate-partial-width-windows` while its sibling `truncate-lines` was
/// read buffer-locally.  With the three methods in the type, a new consumer
/// must say what it does for `WordWrap`, and the value can only be produced by
/// the one function that reads every input the way GNU does.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineWrap {
    /// GNU `TRUNCATE`: the display line is the whole logical line; text past
    /// the right edge is not shown and never starts a new screen line.
    Truncate,
    /// GNU `WINDOW_WRAP`: continuation at the right edge, mid-word.
    WindowWrap,
    /// GNU `WORD_WRAP`: continuation at the last position on the row where a
    /// wrap is allowed (`char_can_wrap_before` after `char_can_wrap_after`,
    /// src/xdisp.c:577-617), falling back to `WindowWrap` when the row offers
    /// no such position.
    WordWrap,
}

/// GNU's `skip_invisible` gives ellipsis-bearing invisibility different
/// semantics depending on its WINDOW argument: display motion elides the
/// source and realizes an ellipsis, while `current-column`/`move-to-column`
/// pass nil and count the underlying source text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvisibleRunContext {
    DisplayMotion,
    ColumnScan,
}

/// Which of GNU's TWO screen-line engines answers a motion question.
///
/// `Fvertical_motion` is not one algorithm with a display switch inside it:
/// its body is an `if` over `noninteractive` choosing between two
/// implementations that share no code (`src/indent.c:2280-2287`):
///
/// ```c
///   if (noninteractive)
///     {
///       struct position pos;
///       pos = *vmotion (PT, PT_BYTE, XFIXNUM (lines), w);
///       SET_PT_BOTH (pos.bufpos, pos.bytepos);
///       it.vpos = pos.vpos;
///     }
///   else
///     { ... start_display / move_it_by_lines / move_it_in_display_line ... }
/// ```
///
/// The batch arm is `vmotion` -> `compute_motion` (`src/indent.c:1963-1964`,
/// `:1253-1254`).  Two things follow from that being a different program rather
/// than a different setting:
///
/// * `compute_motion` has **no word-wrap concept at all**.  Its only line-end
///   decision is truncate-or-continue at `width` (`src/indent.c:1474-1527`);
///   the identifier `word_wrap` does not occur anywhere in `src/indent.c`.
///   So `LineWrap::WordWrap` is not reachable from the batch engine.
/// * The `(COLS . LINES)` goal column is never applied: the `lcols` walk lives
///   inside the `else` (`src/indent.c:2528-2558`), so a batch
///   `vertical-motion` answers a cons argument using only its cdr.
///
/// Measured under GNU Emacs 31.0.90 over one 201-character line carrying a
/// single space at column 100, in an 80-column terminal:
///
/// ```text
///   emacs --batch        word-wrap nil -> rows 1 80 159      count-screen-lines 3
///                        word-wrap t   -> rows 1 80 159      count-screen-lines 3
///   emacs -nw in a pty   word-wrap nil -> rows 1 80 159      count-screen-lines 3
///                        word-wrap t   -> rows 1 80 102 181  count-screen-lines 4
/// ```
///
/// and, for the goal column, `(vertical-motion '(40 . 0))` from the start of a
/// long line answers point 1 under `--batch` and point 41 in a terminal.
///
/// This is a type rather than a condition spelled at each site because the two
/// engines are not interchangeable and their difference is invisible in a
/// value: ledger 191 gated the goal column on `noninteractive` correctly and
/// missed that the very same branch also decides whether `word-wrap` exists at
/// all, which made every batch `count-screen-lines` over a wrapped word answer
/// one screen line too many.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MotionEngine {
    /// GNU `vmotion` -> `compute_motion` (`src/indent.c:1963-1964`): the engine
    /// `Fvertical_motion` uses under `noninteractive`.
    ComputeMotion,
    /// GNU's display iterator (`start_display` / `move_it_by_lines` /
    /// `move_it_in_display_line`): the engine `Fvertical_motion` uses when a
    /// terminal or window system is live.
    DisplayIterator,
}

impl MotionEngine {
    /// The wrap method a NON-truncating display line uses under this engine.
    ///
    /// This is the ONLY producer of [`LineWrap::WordWrap`] in the port.
    /// `init_iterator` reaches `WORD_WRAP` from the buffer's `word-wrap`
    /// (`src/xdisp.c:3425-3426`), and `init_iterator` runs only in the
    /// interactive arm; `compute_motion` continues at `width` whatever the
    /// buffer asks for.
    pub fn continuation_wrap(self, word_wrap: bool) -> LineWrap {
        match self {
            Self::ComputeMotion => LineWrap::WindowWrap,
            Self::DisplayIterator if word_wrap => LineWrap::WordWrap,
            Self::DisplayIterator => LineWrap::WindowWrap,
        }
    }

    /// Whether `(COLS . LINES)`'s COLS is applied at all
    /// (`src/indent.c:2528-2558`, inside the interactive arm).
    pub fn honors_goal_column(self) -> bool {
        matches!(self, Self::DisplayIterator)
    }

    /// Whether realized display rows may answer the question.  GNU's batch
    /// engine walks buffer text and never consults a glyph matrix, so a
    /// retained redisplay snapshot is not an input to it.
    pub fn uses_display_rows(self) -> bool {
        matches!(self, Self::DisplayIterator)
    }
}

/// A settled display-row motion result, in source coordinates.
#[derive(Clone, Copy, Debug)]
pub struct MeasuredMotion {
    pub target: LispCharPos1,
    pub moved: i64,
}

/// One window's text-area geometry, in columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowGeometry {
    pub body_width: i64,
    pub hscroll: i64,
}

// ---------------------------------------------------------------------------
// Packed display glyphs (GNU dispextern.h:389,1973,2040-2050)
// ---------------------------------------------------------------------------

/// GNU's widest character code (`MAX_CHAR` domain used by packed glyphs).
const MAX_CHAR_CODE: i64 = 0x3F_FFFF;

/// A validated GNU packed display glyph (dispextern.h:389,1973,2040-2050).
///
/// The low 22 bits contain a character and the next 20 bits a face ID. This
/// immutable scalar owns no Lisp state and may be copied between mutators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
struct PackedDisplayGlyph(i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
enum PackedDisplayGlyphError {
    #[error("packed display glyph exceeds the character and face domains")]
    OutOfRange,
}

impl PackedDisplayGlyph {
    const CHARACTER_BITS: u32 = 22;
    const CHARACTER_MASK: i64 = EmacsChar::MAX as i64;
    const FACE_ID_MAX: i64 = (1 << 20) - 1;
    const FACE_MASK: i64 = Self::FACE_ID_MAX << Self::CHARACTER_BITS;
    const MAX: i64 = Self::FACE_MASK | Self::CHARACTER_MASK;

    #[inline]
    fn character(self) -> Option<EmacsChar> {
        EmacsChar::from_code((self.0 & Self::CHARACTER_MASK) as u32)
    }
}

const _: () = {
    assert!(PackedDisplayGlyph::CHARACTER_MASK == (1 << PackedDisplayGlyph::CHARACTER_BITS) - 1);
    assert!(PackedDisplayGlyph::CHARACTER_MASK & PackedDisplayGlyph::FACE_MASK == 0);
    assert!(PackedDisplayGlyph::MAX <= Value::MOST_POSITIVE_FIXNUM);
    assert!(std::mem::size_of::<PackedDisplayGlyph>() == std::mem::size_of::<i64>());
};
static_assertions::assert_impl_all!(PackedDisplayGlyph: Send, Sync);

impl TryFrom<i64> for PackedDisplayGlyph {
    type Error = PackedDisplayGlyphError;

    #[inline]
    fn try_from(code: i64) -> Result<Self, Self::Error> {
        if (0..=Self::MAX).contains(&code) {
            Ok(Self(code))
        } else {
            Err(PackedDisplayGlyphError::OutOfRange)
        }
    }
}

/// Decode one valid GNU display glyph (dispextern.h:389,2040-2050).
/// Face IDs occupy 20 bits; packed characters occupy the low 22 bits.
/// Cons glyphs carry the same validated character and face domains separately.
#[inline]
pub fn display_glyph_character(value: Value) -> Option<EmacsChar> {
    match value.kind() {
        ValueKind::Fixnum(code) => PackedDisplayGlyph::try_from(code).ok()?.character(),
        ValueKind::Cons => {
            let code = value.cons_car().as_fixnum()?;
            let face = value.cons_cdr().as_fixnum()?;
            if !(0..=MAX_CHAR_CODE).contains(&code)
                || !(0..=PackedDisplayGlyph::FACE_ID_MAX).contains(&face)
            {
                return None;
            }
            EmacsChar::from_code(code as u32)
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Line-number display target
// ---------------------------------------------------------------------------

/// The selected window's line-number measurement, resolved by the display layer.
///
/// The edit layer owns the gutter metric but cannot inspect frames or window
/// geometry; a host query supplies the selected window's buffer and the frame
/// metrics the metric needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineNumberDisplayTarget {
    pub buffer: BufferId,
    pub char_width: i64,
    pub visible_lines: i64,
}

// ---------------------------------------------------------------------------
// Line-number gutter metrics
// ---------------------------------------------------------------------------

/// Number of digit columns a line-number gutter shows, before the two
/// padding columns (one leading, one trailing).  Mirrors GNU
/// `maybe_produce_line_number`'s `it->lnum_width` (`max(width, log10(max)+1)`,
/// src/xdisp.c).  `visible_lines` is the floor of the window's visible row
/// count; GNU widens to whichever is larger so the gutter never shrinks below
/// what the visible rows need.
pub fn line_number_digit_width(buffer: &Buffer, visible_lines: i64) -> i64 {
    let total_lines = display_line_number_total_lines(buffer)
        .max(visible_lines)
        .max(1);
    let digit_count = total_lines.to_string().len() as i64;
    let min_width = buffer
        .buffer_local_value("display-line-numbers-width")
        .and_then(|value| value.as_fixnum())
        .filter(|width| *width > 0)
        .unwrap_or(1);
    digit_count.max(min_width)
}

fn display_line_number_total_lines(buffer: &Buffer) -> i64 {
    let end = buffer.total_emacs_byte_end_pos();
    buffer.count_newlines_emacs_byte(EmacsBytePos::ZERO, end) as i64 + 1
}
