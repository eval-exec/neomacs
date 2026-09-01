//! GNU-compatible cursor protocol types.
//!
//! These types model the semantic cursor state that GNU Emacs resolves in
//! `xdisp.c` before any backend-specific drawing happens.

use std::fmt;

use num_enum::{IntoPrimitive, TryFromPrimitive};

/// GNU Emacs `enum text_cursor_kinds` (`src/dispextern.h`).
///
/// The discriminants match GNU exactly so constants copied from display code
/// do not get silently re-numbered:
///
/// ```text
/// DEFAULT_CURSOR    = -2
/// NO_CURSOR         = -1
/// FILLED_BOX_CURSOR =  0
/// HOLLOW_BOX_CURSOR =  1
/// BAR_CURSOR        =  2
/// HBAR_CURSOR       =  3
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(i8)]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum CursorKind {
    Default = -2,
    NoCursor = -1,
    FilledBox = 0,
    HollowBox = 1,
    Bar = 2,
    Hbar = 3,
}

impl CursorKind {
    /// Decode from GNU's signed `enum text_cursor_kinds` integer
    /// representation. Returns `None` for any value outside the legal
    /// discriminants.
    pub fn from_gnu_code(code: i8) -> Option<Self> {
        Self::try_from(code).ok()
    }

    /// GNU enum integer code (matches `enum text_cursor_kinds`).
    pub fn gnu_code(self) -> i8 {
        self.into()
    }
}

/// Lisp-level bar/hbar cursor width.
///
/// GNU accepts `(bar . WIDTH)` and `(hbar . WIDTH)` when WIDTH is a fixnum in
/// the nonnegative C `int` range.  Width zero is therefore a valid Lisp
/// semantic value even though render backends clamp to one visible pixel when
/// drawing.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct CursorBarWidth(u32);

impl CursorBarWidth {
    pub const DEFAULT: Self = Self(1);
    pub const TWO: Self = Self(2);

    pub const fn new(width: u32) -> Self {
        Self(width)
    }

    pub fn from_lisp_fixnum(width: i64) -> Option<Self> {
        if (0..=i64::from(i32::MAX)).contains(&width) {
            Some(Self(width as u32))
        } else {
            None
        }
    }

    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Pixel dimension used by renderers for bar/hbar drawing.
    pub fn render_px(self) -> f32 {
        self.0.max(1) as f32
    }

    /// GNU narrows bar cursors in non-selected windows by one pixel when the
    /// alternate cursor is `t`.
    pub fn narrowed_for_non_selected_bar(self) -> Self {
        if self.0 > 1 { Self(self.0 - 1) } else { self }
    }
}

impl Default for CursorBarWidth {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl fmt::Display for CursorBarWidth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Resolved cursor kind plus semantic bar width.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CursorSpec {
    pub cursor_kind: CursorKind,
    pub bar_width: CursorBarWidth,
}

impl CursorSpec {
    pub const fn new(cursor_kind: CursorKind, bar_width: CursorBarWidth) -> Self {
        Self {
            cursor_kind,
            bar_width,
        }
    }

    pub const fn no_cursor() -> Self {
        Self::new(CursorKind::NoCursor, CursorBarWidth::DEFAULT)
    }

    pub const fn filled_box() -> Self {
        Self::new(CursorKind::FilledBox, CursorBarWidth::DEFAULT)
    }

    pub const fn hollow_box() -> Self {
        Self::new(CursorKind::HollowBox, CursorBarWidth::DEFAULT)
    }

    pub const fn bar(width: CursorBarWidth) -> Self {
        Self::new(CursorKind::Bar, width)
    }

    pub const fn hbar(width: CursorBarWidth) -> Self {
        Self::new(CursorKind::Hbar, width)
    }

    pub fn to_style(self) -> Option<CursorStyle> {
        CursorStyle::from_spec(self)
    }
}

/// Cursor visual style, carrying backend drawing dimensions.
///
/// Filled and hollow cursors use the owning slot rectangle as-is. Bar/Hbar
/// variants carry the thin dimension (width or height) for rendering within
/// that slot.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CursorStyle {
    /// Filled box cursor (covers entire character cell).
    FilledBox,
    /// Vertical bar cursor with specified render width in pixels.
    Bar(f32),
    /// Horizontal bar cursor with specified render height in pixels.
    Hbar(f32),
    /// Hollow box cursor.
    Hollow,
}

impl CursorStyle {
    /// Convert a resolved semantic cursor spec into a renderable style.
    pub fn from_spec(spec: CursorSpec) -> Option<Self> {
        Self::from_kind(spec.cursor_kind, spec.bar_width)
    }

    /// Convert a `CursorKind` plus bar width into a renderable `CursorStyle`.
    /// `Default` collapses to `FilledBox` and `NoCursor` returns `None`.
    pub fn from_kind(kind: CursorKind, bar_width: CursorBarWidth) -> Option<Self> {
        match kind {
            CursorKind::FilledBox | CursorKind::Default => Some(Self::FilledBox),
            CursorKind::HollowBox => Some(Self::Hollow),
            CursorKind::Bar => Some(Self::Bar(bar_width.render_px())),
            CursorKind::Hbar => Some(Self::Hbar(bar_width.render_px())),
            CursorKind::NoCursor => None,
        }
    }

    /// Legacy entry point that accepts the old raw cursor byte.
    #[deprecated(note = "use CursorStyle::from_kind with CursorKind for GNU-parity encoding")]
    pub fn from_type(cursor_type: u8, bar_width: i32) -> Option<Self> {
        let kind = CursorKind::from_gnu_code(cursor_type as i8)?;
        let width = CursorBarWidth::new(bar_width.max(0) as u32);
        Self::from_kind(kind, width)
    }

    /// Returns true if this is a hollow cursor.
    pub fn is_hollow(&self) -> bool {
        matches!(self, Self::Hollow)
    }

    /// Whether this cursor draws purely as a top layer over unchanged text.
    ///
    /// Bar, Hbar, and Hollow cursors are drawn on top of otherwise-normal
    /// glyphs, so they can be composited over a retained static scene. The
    /// filled box uses inverse video — the glyph under it is redrawn in the
    /// cursor foreground color during the text pass — so it is not separable
    /// from the static scene and must go through a full render.
    pub fn is_clean_top_layer(&self) -> bool {
        !matches!(self, Self::FilledBox)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_top_layer_covers_bar_hbar_hollow_only() {
        // The retained-static cursor fast path depends on this mapping: only
        // the filled box (inverse video) forces the full render.
        assert!(CursorStyle::Bar(2.0).is_clean_top_layer());
        assert!(CursorStyle::Hbar(2.0).is_clean_top_layer());
        assert!(CursorStyle::Hollow.is_clean_top_layer());
        assert!(!CursorStyle::FilledBox.is_clean_top_layer());
    }

    #[test]
    fn cursor_kind_matches_gnu_text_cursor_codes() {
        let cases = [
            (-2, CursorKind::Default),
            (-1, CursorKind::NoCursor),
            (0, CursorKind::FilledBox),
            (1, CursorKind::HollowBox),
            (2, CursorKind::Bar),
            (3, CursorKind::Hbar),
        ];

        for (code, kind) in cases {
            assert_eq!(CursorKind::from_gnu_code(code), Some(kind));
            assert_eq!(kind.gnu_code(), code);
        }

        assert_eq!(CursorKind::from_gnu_code(i8::MIN), None);
        assert_eq!(CursorKind::from_gnu_code(4), None);
    }

    #[test]
    fn lisp_bar_width_accepts_gnu_nonnegative_int_range() {
        assert_eq!(
            CursorBarWidth::from_lisp_fixnum(0),
            Some(CursorBarWidth::new(0))
        );
        assert_eq!(
            CursorBarWidth::from_lisp_fixnum(i64::from(i32::MAX)),
            Some(CursorBarWidth::new(i32::MAX as u32))
        );
        assert_eq!(CursorBarWidth::from_lisp_fixnum(-1), None);
        assert_eq!(
            CursorBarWidth::from_lisp_fixnum(i64::from(i32::MAX) + 1),
            None
        );
    }

    #[test]
    fn renderer_clamps_zero_width_without_changing_lisp_value() {
        let width = CursorBarWidth::new(0);

        assert_eq!(width.raw(), 0);
        assert_eq!(width.render_px(), 1.0);
    }

    #[test]
    fn non_selected_bar_narrowing_preserves_zero_and_one() {
        assert_eq!(
            CursorBarWidth::new(0).narrowed_for_non_selected_bar(),
            CursorBarWidth::new(0)
        );
        assert_eq!(
            CursorBarWidth::new(1).narrowed_for_non_selected_bar(),
            CursorBarWidth::new(1)
        );
        assert_eq!(
            CursorBarWidth::new(3).narrowed_for_non_selected_bar(),
            CursorBarWidth::new(2)
        );
    }
}
