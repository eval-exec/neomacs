use crate::coords::lisp_char_pos_to_layout_i64;
use crate::display_row::builder::{DisplayRowGlyphCheckpoint, DisplayRowPosition};
use crate::display_row::face_state::DisplayRowExtendFace;
#[cfg(test)]
use crate::display_row::geometry::DisplayRowGeometryState;
use crate::display_row::geometry::{DisplayRowHitRange, DisplayRowMarker, DisplayRowStartMarker};
use crate::display_source::DisplaySourceTextPosition;
use crate::neovm_bridge::{LayoutBufferView, RustBufferAccess};
use crate::types::LineWrapMode;
use neomacs_display_protocol::types::Color;
use neovm_core::buffer::LispCharPos1;
use neovm_core::window::DisplayRowSnapshot;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct WordWrapBreakCandidate {
    /// Typed window-slice position.  Keeping the relative byte index paired
    /// with its absolute character position prevents a wrap checkpoint from
    /// accidentally storing a buffer-absolute byte offset.
    source_position: DisplaySourceTextPosition,
    display_point_count: usize,
    row_first_display_pos: Option<LispCharPos1>,
    row_last_display_pos: Option<LispCharPos1>,
    /// Glyph-row checkpoint captured when this candidate was recorded, i.e. the
    /// row's drawn-glyph counts *before* the candidate (word-start) char was
    /// pushed. The word-wrap break restores it so the partial word that already
    /// fit on the current row is rolled off and re-rendered on the next row.
    glyph_checkpoint: DisplayRowGlyphCheckpoint,
    row_position: DisplayRowPosition,
    /// Complete realized `:extend` state immediately before this boundary.
    /// GNU saves it with the iterator; restoring only glyph counts would let
    /// the overflowing item's face leak onto the preceding visual row.
    row_extend: Option<DisplayRowExtendFace>,
    available: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WordWrapRenderState {
    enabled: bool,
    may_wrap: bool,
    candidate: WordWrapBreakCandidate,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct HorizontalScrollSkipState {
    configured_columns: i32,
    truncation_target: HorizontalScrollTruncationTarget,
    phase: HorizontalScrollSkipPhase,
}

/// The already-laid-out glyph that GNU's left-truncation marker overwrites.
///
/// `display_line` normally replaces the first visible source glyph.  With
/// display line numbers, however, `maybe_produce_line_number` defers the
/// prefix until horizontal skipping reaches `first_visible_x`, and
/// `insert_left_trunc_glyphs` subsequently overwrites the prefix's first
/// glyph.  Keeping those targets distinct prevents the line-number case from
/// consuming one additional buffer character or one additional screen cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum HorizontalScrollTruncationTarget {
    #[default]
    FirstVisibleSourceGlyph,
    LineNumberPrefix,
}

/// One source display item encountered while walking to the horizontal-scroll
/// boundary.  The variant, rather than a width heuristic later in rendering,
/// decides how GNU exposes an item that straddles `first_visible_x`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HorizontalScrollDisplayItem {
    Glyph { ch: char, columns: u16 },
    Tab { columns: u16 },
}

impl HorizontalScrollDisplayItem {
    pub(crate) fn glyph(ch: char, columns: i32) -> Self {
        Self::Glyph {
            ch,
            columns: columns.clamp(1, i32::from(u16::MAX)) as u16,
        }
    }

    pub(crate) fn tab(columns: i32) -> Self {
        Self::Tab {
            columns: columns.clamp(1, i32::from(u16::MAX)) as u16,
        }
    }

    fn columns(self) -> i32 {
        i32::from(match self {
            Self::Glyph { columns, .. } | Self::Tab { columns } => columns,
        })
    }

    fn visible_remainder(
        self,
        visible_columns: i32,
        target: HorizontalScrollTruncationTarget,
    ) -> HorizontalScrollVisibleRemainder {
        match target {
            // The marker consumes the first visible cell of the boundary
            // item.  A TAB or wide glyph can leave blank cells after it.
            HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph => {
                HorizontalScrollVisibleRemainder::blank_columns(visible_columns - 1)
            }
            // GNU suppresses a negative row offset when line numbers are
            // present.  A wide glyph is therefore retained whole; a TAB uses
            // `stretch_adjust` and retains only its visible suffix.
            HorizontalScrollTruncationTarget::LineNumberPrefix => match self {
                Self::Glyph { ch, columns } if columns > 1 => {
                    HorizontalScrollVisibleRemainder::WholeWideGlyph { ch, columns }
                }
                Self::Tab { .. } => {
                    HorizontalScrollVisibleRemainder::blank_columns(visible_columns)
                }
                Self::Glyph { .. } => HorizontalScrollVisibleRemainder::None,
            },
        }
    }
}

/// The part of a boundary-crossing item that remains after the truncation
/// marker has been installed.  This is deliberately independent from the
/// marker target: matrix overwrite and source-item clipping are two different
/// GNU redisplay operations.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum HorizontalScrollVisibleRemainder {
    #[default]
    None,
    BlankColumns(u16),
    WholeWideGlyph {
        ch: char,
        columns: u16,
    },
}

impl HorizontalScrollVisibleRemainder {
    fn blank_columns(columns: i32) -> Self {
        if columns <= 0 {
            Self::None
        } else {
            Self::BlankColumns(columns.min(i32::from(u16::MAX)) as u16)
        }
    }

    pub(crate) fn columns(self) -> usize {
        usize::from(match self {
            Self::None => 0,
            Self::BlankColumns(columns) | Self::WholeWideGlyph { columns, .. } => columns,
        })
    }
}

/// What remains to be consumed at the left edge of a horizontally-scrolled
/// row.  GNU lays out the first visible glyph and then overwrites it with the
/// left-truncation mark.  `ReplaceNextGlyph` makes the exact-boundary case a
/// distinct state: reaching the boundary is not the same as having consumed
/// the glyph that the mark replaces.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum HorizontalScrollSkipPhase {
    #[default]
    Disabled,
    Skipping {
        remaining_columns: i32,
    },
    ReplaceNextGlyph,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HscrollConsumedTextDisposition {
    Hidden,
    InstallLeftTruncation {
        target: HorizontalScrollTruncationTarget,
        visible_remainder: HorizontalScrollVisibleRemainder,
    },
}

/// Which side of a consumed source item owns the cursor anchor.
///
/// Fully hidden items advance the best approximation to their end.  The item
/// that reaches or crosses the visible boundary is different: GNU
/// `set_cursor_from_row` places point at the first visible row position, so its
/// source start owns the anchor even when a truncation marker covers it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HscrollConsumedTextCursorAnchor {
    ConsumedItemEnd,
    VisibleBoundaryItemStart,
}

impl HscrollConsumedTextCursorAnchor {
    fn charpos(self, source_char: crate::display_source::DisplaySourceStepChar) -> i64 {
        match self {
            Self::ConsumedItemEnd => source_char.start_charpos() + 1,
            Self::VisibleBoundaryItemStart => source_char.start_charpos(),
        }
    }
}

impl HscrollConsumedTextDisposition {
    pub(crate) fn cursor_anchor_charpos(
        self,
        source_char: crate::display_source::DisplaySourceStepChar,
    ) -> i64 {
        match self {
            Self::Hidden => HscrollConsumedTextCursorAnchor::ConsumedItemEnd,
            Self::InstallLeftTruncation { .. } => {
                HscrollConsumedTextCursorAnchor::VisibleBoundaryItemStart
            }
        }
        .charpos(source_char)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LineNumberRenderState {
    current_line: i64,
    point_line: i64,
    phase: LineNumberRenderPhase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LineNumberRenderPhase {
    Disabled,
    Pending(LineNumberRowPrefix),
    Rendered,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LineNumberRowPrefix {
    Numbered,
    ReservedBlank(LineNumberBlankReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LineNumberBlankReason {
    Continuation,
    BeyondAccessibleEnd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LineNumberTextPrefixFace {
    Normal,
    CurrentLine,
    MajorTick,
}

impl LineNumberTextPrefixFace {
    /// Every role that can own a line-number row. Window geometry measures
    /// this closed set, so adding a new role requires the compiler-visible
    /// policy here rather than another untyped string at the call site.
    pub(crate) const ALL: [Self; 3] = [Self::Normal, Self::CurrentLine, Self::MajorTick];

    pub(crate) fn face_name(self) -> &'static str {
        match self {
            Self::Normal => "line-number",
            Self::CurrentLine => "line-number-current-line",
            Self::MajorTick => "line-number-major-tick",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LineNumberTextPrefixContent {
    Number(i64),
    Blank,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LineNumberTextPrefixColumns(usize);

impl LineNumberTextPrefixColumns {
    fn from_layout_columns(cols: i32) -> Self {
        Self(cols.max(1) as usize)
    }

    pub(crate) fn get(self) -> usize {
        self.0
    }
}

/// The authoritative pixel extent reserved for the line-number field.
///
/// Keeping this distinct from [`LineNumberTextPrefixColumns`] prevents the prefix
/// producer from accidentally comparing a shaped glyph advance with a logical
/// column count. Buffer-window geometry reserves the same measured extent
/// before ordinary source text is laid out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LineNumberTextPrefixExtentPx(f32);

impl LineNumberTextPrefixExtentPx {
    pub(crate) fn get(self) -> f32 {
        self.0
    }
}

/// One window's complete line-number field after semantic column planning and
/// concrete face measurement have met.
///
/// GNU records `lnum_width` (digits) separately from `lnum_pixel_width`
/// (advances produced with the window-resolved line-number face).  Keeping the
/// two domains together prevents geometry from multiplying line-number columns
/// by the unrelated frame-default cell width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LineNumberFieldLayout {
    columns: i32,
    cell_width_px: f32,
    extent_px: f32,
}

impl LineNumberFieldLayout {
    pub(crate) fn new(columns: i32, cell_width_px: f32) -> Self {
        let columns = columns.max(0);
        let cell_width_px = cell_width_px.max(1.0);
        Self {
            columns,
            cell_width_px,
            extent_px: columns as f32 * cell_width_px,
        }
    }

    pub(crate) fn columns(self) -> i32 {
        self.columns
    }

    pub(crate) fn cell_width_px(self) -> f32 {
        self.cell_width_px
    }

    pub(crate) fn extent(self) -> LineNumberTextPrefixExtentPx {
        LineNumberTextPrefixExtentPx(self.extent_px)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LineNumberTextPrefix {
    content: LineNumberTextPrefixContent,
    field: LineNumberFieldLayout,
    face: LineNumberTextPrefixFace,
}

impl LineNumberTextPrefix {
    pub(crate) fn blank_beyond_accessible_end(field: LineNumberFieldLayout) -> Self {
        Self {
            content: LineNumberTextPrefixContent::Blank,
            field,
            face: LineNumberTextPrefixFace::Normal,
        }
    }

    #[cfg(test)]
    pub(crate) fn text(self) -> String {
        match self.content {
            LineNumberTextPrefixContent::Number(number) => number.to_string(),
            LineNumberTextPrefixContent::Blank => String::new(),
        }
    }

    pub(crate) fn cols(self) -> i32 {
        self.field.columns()
    }

    pub(crate) fn cell_width_px(self) -> f32 {
        self.field.cell_width_px()
    }

    pub(crate) fn pixel_extent(self) -> LineNumberTextPrefixExtentPx {
        self.field.extent()
    }

    /// Produce GNU's complete `lnum_buf`: right-aligned number field plus
    /// trailing separator, or an all-space field for continuation rows.
    ///
    /// Keeping the entire field as one text run makes every cell use the
    /// selected line-number face.  Mixing default-grid stretches with shaped
    /// number glyphs makes the total pixel width depend on digit count whenever
    /// that face has different metrics from the buffer's default face.
    pub(crate) fn padded_text(self) -> String {
        let columns = LineNumberTextPrefixColumns::from_layout_columns(self.field.columns()).get();
        match self.content {
            LineNumberTextPrefixContent::Blank => " ".repeat(columns),
            LineNumberTextPrefixContent::Number(number) => {
                let number = number.to_string();
                let leading_spaces = columns.saturating_sub(number.chars().count() + 1);
                let mut text = String::with_capacity(columns.max(number.len() + 1));
                text.extend(std::iter::repeat_n(' ', leading_spaces));
                text.push_str(&number);
                text.push(' ');
                text
            }
        }
    }

    pub(crate) fn face(self) -> LineNumberTextPrefixFace {
        self.face
    }

    #[cfg(test)]
    pub(crate) fn blank(self) -> bool {
        self.content == LineNumberTextPrefixContent::Blank
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FaceScanCheckpoint {
    next_check: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum BoxFaceRowState {
    Inactive,
    Active { row: DisplayRowMarker, start_x: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TrailingWhitespaceRenderState {
    background: Option<Color>,
    start_marker: DisplayRowStartMarker,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HitRowRangeTracker {
    start_charpos: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InvisibleTextScanCheckpoint {
    invisible_next: i64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SpecialTextRowOverflowDecision {
    Fits,
    Truncate,
    Wrap,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DisplayRowTextOverflowDecision {
    Fits,
    Truncate,
    WordWrap {
        break_candidate: WordWrapBreakCandidate,
    },
    CharacterWrap,
}

/// Whether the row a transition opens starts a NEW buffer line or CONTINUES the
/// current one.
///
/// This is the only fact that decides whether the horizontal-scroll skip is
/// re-armed, so it is asked rather than restated: see
/// [`TextRowTransitionStatePolicy::resets_hscroll`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextRowTransitionPrefixAction {
    /// The next row begins a new buffer line (a line break, or the tail of a
    /// TRUNCATED line being discarded).
    Line,
    /// The next row continues the current buffer line (word/character wrap).
    Wrap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextRowTransitionWordWrapAction {
    Keep,
    Reset,
    Disallow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TextRowTransitionStatePolicy {
    advance_line_number: bool,
    word_wrap: TextRowTransitionWordWrapAction,
    reset_trailing_whitespace: bool,
    prefix: TextRowTransitionPrefixAction,
}

impl WordWrapBreakCandidate {
    #[cfg(test)]
    pub(crate) fn record(
        &mut self,
        byte_idx: usize,
        charpos: i64,
        display_point_count: usize,
        row_display_positions: (Option<LispCharPos1>, Option<LispCharPos1>),
        glyph_checkpoint: DisplayRowGlyphCheckpoint,
    ) {
        self.record_at(
            DisplaySourceTextPosition::new(byte_idx, charpos),
            display_point_count,
            row_display_positions,
            glyph_checkpoint,
            DisplayRowPosition::default(),
            None,
        );
    }

    pub(crate) fn record_at(
        &mut self,
        source_position: DisplaySourceTextPosition,
        display_point_count: usize,
        row_display_positions: (Option<LispCharPos1>, Option<LispCharPos1>),
        glyph_checkpoint: DisplayRowGlyphCheckpoint,
        row_position: DisplayRowPosition,
        row_extend: Option<DisplayRowExtendFace>,
    ) {
        self.source_position = source_position;
        self.display_point_count = display_point_count;
        self.row_first_display_pos = row_display_positions.0;
        self.row_last_display_pos = row_display_positions.1;
        self.glyph_checkpoint = glyph_checkpoint;
        self.row_position = row_position;
        self.row_extend = row_extend;
        self.available = true;
    }

    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn is_available(&self) -> bool {
        self.available
    }

    #[cfg(test)]
    pub(crate) fn byte_idx(&self) -> usize {
        self.source_position.byte_idx()
    }

    #[cfg(test)]
    pub(crate) fn charpos(&self) -> i64 {
        self.source_position.charpos()
    }

    pub(crate) fn source_position(self) -> DisplaySourceTextPosition {
        self.source_position
    }

    pub(crate) fn display_point_count(&self) -> usize {
        self.display_point_count
    }

    pub(crate) fn row_display_positions(&self) -> (Option<LispCharPos1>, Option<LispCharPos1>) {
        (self.row_first_display_pos, self.row_last_display_pos)
    }

    pub(crate) fn glyph_checkpoint(&self) -> DisplayRowGlyphCheckpoint {
        self.glyph_checkpoint
    }

    pub(crate) fn row_position(&self) -> DisplayRowPosition {
        self.row_position
    }

    pub(crate) fn row_extend(&self) -> Option<DisplayRowExtendFace> {
        self.row_extend
    }
}

impl SpecialTextRowOverflowDecision {
    pub(crate) fn for_width(
        x_px: f32,
        width_px: f32,
        right_edge_px: f32,
        wrap_mode: LineWrapMode,
    ) -> Self {
        if x_px + width_px <= right_edge_px {
            Self::Fits
        } else if wrap_mode == LineWrapMode::Truncate {
            Self::Truncate
        } else {
            Self::Wrap
        }
    }
}

impl DisplayRowTextOverflowDecision {
    pub(crate) fn for_char(
        ch: char,
        x_px: f32,
        advance_px: f32,
        right_edge_px: f32,
        wrap_mode: LineWrapMode,
        word_wrap: WordWrapRenderState,
    ) -> Self {
        if ch == '\t' || x_px + advance_px <= right_edge_px {
            Self::Fits
        } else if wrap_mode == LineWrapMode::Truncate {
            Self::Truncate
        } else if word_wrap.has_candidate() {
            Self::WordWrap {
                break_candidate: word_wrap.candidate(),
            }
        } else {
            Self::CharacterWrap
        }
    }
}

impl TextRowTransitionStatePolicy {
    fn new(
        advance_line_number: bool,
        word_wrap: TextRowTransitionWordWrapAction,
        reset_trailing_whitespace: bool,
        prefix: TextRowTransitionPrefixAction,
    ) -> Self {
        Self {
            advance_line_number,
            word_wrap,
            reset_trailing_whitespace,
            prefix,
        }
    }

    /// GNU keeps horizontal scrolling as a COORDINATE: `init_iterator` sets
    /// `it->first_visible_x` once from `w->hscroll`, and every row
    /// `display_line` produces drops glyphs while
    /// `it->current_x < it->first_visible_x` (src/xdisp.c).  Nothing is
    /// consumed, so nothing has to be reset.
    ///
    /// This port models the same thing as a consumable per-window budget
    /// ([`HorizontalScrollSkipState`]), which must therefore be re-armed at the
    /// start of every buffer line.  That is not an independent decision -- it is
    /// exactly [`TextRowTransitionPrefixAction::Line`] -- so it is DERIVED here
    /// rather than restated as a field each constructor has to remember.  When
    /// it was a field, `truncation()` and `special_truncation()` left it false,
    /// and the first truncated line in a hscrolled window spent the budget for
    /// every line below it (ledger 201).
    const fn resets_hscroll(self) -> bool {
        matches!(self.prefix, TextRowTransitionPrefixAction::Line)
    }

    pub(crate) fn hscroll_line_break() -> Self {
        Self::new(
            true,
            TextRowTransitionWordWrapAction::Keep,
            true,
            TextRowTransitionPrefixAction::Line,
        )
    }

    pub(crate) fn line_break() -> Self {
        Self::new(
            true,
            TextRowTransitionWordWrapAction::Reset,
            false,
            TextRowTransitionPrefixAction::Line,
        )
    }

    pub(crate) fn hidden_line_break() -> Self {
        Self::new(
            true,
            TextRowTransitionWordWrapAction::Reset,
            true,
            TextRowTransitionPrefixAction::Line,
        )
    }

    pub(crate) fn truncation() -> Self {
        Self::new(
            false,
            TextRowTransitionWordWrapAction::Reset,
            true,
            TextRowTransitionPrefixAction::Line,
        )
    }

    pub(crate) fn special_truncation() -> Self {
        Self::new(
            false,
            TextRowTransitionWordWrapAction::Disallow,
            true,
            TextRowTransitionPrefixAction::Line,
        )
    }

    pub(crate) fn visual_wrap() -> Self {
        Self::new(
            false,
            TextRowTransitionWordWrapAction::Reset,
            true,
            TextRowTransitionPrefixAction::Wrap,
        )
    }

    pub(crate) fn special_visual_wrap() -> Self {
        Self::new(
            false,
            TextRowTransitionWordWrapAction::Keep,
            true,
            TextRowTransitionPrefixAction::Wrap,
        )
    }

    pub(crate) fn character_wrap() -> Self {
        Self::new(
            false,
            TextRowTransitionWordWrapAction::Disallow,
            true,
            TextRowTransitionPrefixAction::Wrap,
        )
    }

    pub(crate) fn apply(
        self,
        line_numbers: &mut LineNumberRenderState,
        hscroll_skip: &mut HorizontalScrollSkipState,
        word_wrap: &mut WordWrapRenderState,
        trailing_whitespace: &mut TrailingWhitespaceRenderState,
    ) -> TextRowTransitionPrefixAction {
        if self.advance_line_number {
            line_numbers.advance_line();
        }
        if self.resets_hscroll() {
            hscroll_skip.reset_line();
        }
        match self.word_wrap {
            TextRowTransitionWordWrapAction::Keep => {}
            TextRowTransitionWordWrapAction::Reset => word_wrap.reset_after_row_transition(),
            TextRowTransitionWordWrapAction::Disallow => word_wrap.disallow_after_current_char(),
        }
        if self.reset_trailing_whitespace {
            trailing_whitespace.reset_after_row_transition();
        }
        self.prefix
    }
}

impl WordWrapRenderState {
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            may_wrap: false,
            candidate: WordWrapBreakCandidate::default(),
        }
    }

    pub(crate) fn is_enabled(self) -> bool {
        self.enabled
    }

    pub(crate) fn can_record_candidate(self, ch: char) -> bool {
        self.enabled && self.may_wrap && char_can_wrap_before_basic(ch)
    }

    #[cfg(test)]
    pub(crate) fn record_candidate(
        &mut self,
        ch: char,
        byte_idx: usize,
        charpos: i64,
        display_point_count: usize,
        row_display_positions: (Option<LispCharPos1>, Option<LispCharPos1>),
        glyph_checkpoint: DisplayRowGlyphCheckpoint,
    ) {
        self.record_candidate_at(
            ch,
            DisplaySourceTextPosition::new(byte_idx, charpos),
            display_point_count,
            row_display_positions,
            glyph_checkpoint,
            DisplayRowPosition::default(),
            None,
        );
    }

    pub(crate) fn record_candidate_at(
        &mut self,
        ch: char,
        source_position: DisplaySourceTextPosition,
        display_point_count: usize,
        row_display_positions: (Option<LispCharPos1>, Option<LispCharPos1>),
        glyph_checkpoint: DisplayRowGlyphCheckpoint,
        row_position: DisplayRowPosition,
        row_extend: Option<DisplayRowExtendFace>,
    ) {
        if self.can_record_candidate(ch) {
            self.candidate.record_at(
                source_position,
                display_point_count,
                row_display_positions,
                glyph_checkpoint,
                row_position,
                row_extend,
            );
        }
    }

    pub(crate) fn allow_after_current_char(&mut self, ch: char) {
        self.may_wrap = char_can_wrap_after_basic(ch);
    }

    pub(crate) fn disallow_after_current_char(&mut self) {
        self.may_wrap = false;
    }

    pub(crate) fn reset_after_row_transition(&mut self) {
        self.may_wrap = false;
        self.candidate.clear();
    }

    pub(crate) fn has_candidate(self) -> bool {
        self.enabled && self.candidate.is_available()
    }

    pub(crate) fn candidate(self) -> WordWrapBreakCandidate {
        self.candidate
    }
}

impl HorizontalScrollSkipState {
    pub(crate) fn new(
        wrap_mode: LineWrapMode,
        hscroll_columns: i32,
        truncation_target: HorizontalScrollTruncationTarget,
    ) -> Self {
        let configured_columns = if wrap_mode == LineWrapMode::Truncate {
            hscroll_columns.max(0)
        } else {
            0
        };
        Self {
            configured_columns,
            truncation_target,
            phase: if configured_columns > 0 {
                HorizontalScrollSkipPhase::Skipping {
                    remaining_columns: configured_columns,
                }
            } else {
                HorizontalScrollSkipPhase::Disabled
            },
        }
    }

    pub(crate) fn reset_line(&mut self) {
        self.phase = if self.configured_columns > 0 {
            HorizontalScrollSkipPhase::Skipping {
                remaining_columns: self.configured_columns,
            }
        } else {
            HorizontalScrollSkipPhase::Disabled
        };
    }

    pub(crate) fn should_skip(self) -> bool {
        matches!(
            self.phase,
            HorizontalScrollSkipPhase::Skipping { .. }
                | HorizontalScrollSkipPhase::ReplaceNextGlyph
        )
    }

    pub(crate) fn consumed_columns(self) -> i32 {
        match self.phase {
            HorizontalScrollSkipPhase::Disabled => 0,
            HorizontalScrollSkipPhase::Skipping { remaining_columns } => {
                self.configured_columns - remaining_columns
            }
            HorizontalScrollSkipPhase::ReplaceNextGlyph | HorizontalScrollSkipPhase::Complete => {
                self.configured_columns
            }
        }
    }

    pub(crate) fn consume_display_item(
        &mut self,
        item: HorizontalScrollDisplayItem,
    ) -> HscrollConsumedTextDisposition {
        let columns = item.columns();
        match self.phase {
            HorizontalScrollSkipPhase::Skipping { remaining_columns }
                if columns < remaining_columns =>
            {
                self.phase = HorizontalScrollSkipPhase::Skipping {
                    remaining_columns: remaining_columns - columns,
                };
                HscrollConsumedTextDisposition::Hidden
            }
            HorizontalScrollSkipPhase::Skipping { remaining_columns }
                if columns == remaining_columns =>
            {
                match self.truncation_target {
                    HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph => {
                        self.phase = HorizontalScrollSkipPhase::ReplaceNextGlyph;
                        HscrollConsumedTextDisposition::Hidden
                    }
                    HorizontalScrollTruncationTarget::LineNumberPrefix => {
                        self.phase = HorizontalScrollSkipPhase::Complete;
                        HscrollConsumedTextDisposition::InstallLeftTruncation {
                            target: HorizontalScrollTruncationTarget::LineNumberPrefix,
                            visible_remainder: HorizontalScrollVisibleRemainder::None,
                        }
                    }
                }
            }
            HorizontalScrollSkipPhase::Skipping { remaining_columns } => {
                self.phase = HorizontalScrollSkipPhase::Complete;
                HscrollConsumedTextDisposition::InstallLeftTruncation {
                    target: self.truncation_target,
                    visible_remainder: item
                        .visible_remainder(columns - remaining_columns, self.truncation_target),
                }
            }
            HorizontalScrollSkipPhase::ReplaceNextGlyph => {
                self.phase = HorizontalScrollSkipPhase::Complete;
                HscrollConsumedTextDisposition::InstallLeftTruncation {
                    target: self.truncation_target,
                    visible_remainder: item.visible_remainder(columns, self.truncation_target),
                }
            }
            HorizontalScrollSkipPhase::Disabled | HorizontalScrollSkipPhase::Complete => {
                debug_assert!(false, "hscroll columns consumed after skipping completed");
                HscrollConsumedTextDisposition::Hidden
            }
        }
    }
}

impl LineNumberRenderState {
    pub(crate) fn new(enabled: bool, current_line: i64, point_line: i64) -> Self {
        Self {
            current_line,
            point_line,
            phase: if enabled {
                LineNumberRenderPhase::Pending(LineNumberRowPrefix::Numbered)
            } else {
                LineNumberRenderPhase::Disabled
            },
        }
    }

    #[cfg(test)]
    pub(crate) fn should_render(self) -> bool {
        matches!(self.phase, LineNumberRenderPhase::Pending(_))
    }

    fn arm_prefix(&mut self, prefix: LineNumberRowPrefix) {
        if !matches!(self.phase, LineNumberRenderPhase::Disabled) {
            self.phase = LineNumberRenderPhase::Pending(prefix);
        }
    }

    pub(crate) fn advance_line(&mut self) {
        self.current_line += 1;
        self.arm_prefix(LineNumberRowPrefix::Numbered);
    }

    /// GNU `maybe_produce_line_number` renders a blank (width-reserved, no
    /// number) gutter on each wrapped continuation row. Re-arm the pending
    /// render and mark the next row as a continuation so the gutter is blank.
    pub(crate) fn mark_continuation_row(&mut self) {
        self.arm_prefix(LineNumberRowPrefix::ReservedBlank(
            LineNumberBlankReason::Continuation,
        ));
    }

    /// GNU `maybe_produce_line_number` reserves the line-number width but does
    /// not display another number on the synthetic empty row at or beyond ZV.
    /// Only a still-pending prefix can become the EOB prefix: a final non-newline
    /// text row has already rendered its number and must remain unchanged.
    pub(crate) fn mark_beyond_accessible_end(&mut self) {
        if matches!(self.phase, LineNumberRenderPhase::Pending(_)) {
            self.phase = LineNumberRenderPhase::Pending(LineNumberRowPrefix::ReservedBlank(
                LineNumberBlankReason::BeyondAccessibleEnd,
            ));
        }
    }

    pub(crate) fn advance_hidden_line(&mut self) {
        self.current_line += 1;
    }

    #[cfg(test)]
    pub(crate) fn current_line(self) -> i64 {
        self.current_line
    }

    #[cfg(test)]
    pub(crate) fn point_line(self) -> i64 {
        self.point_line
    }

    pub(crate) fn is_current_line(self) -> bool {
        self.current_line == self.point_line
    }

    pub(crate) fn display_number(
        self,
        mode: crate::types::DisplayLineNumbersMode,
        current_absolute: bool,
        offset: i64,
    ) -> i64 {
        use crate::types::DisplayLineNumbersMode;
        match mode {
            DisplayLineNumbersMode::Relative | DisplayLineNumbersMode::Visual => {
                if current_absolute && self.is_current_line() {
                    (self.current_line + offset).abs()
                } else {
                    (self.current_line - self.point_line).abs()
                }
            }
            DisplayLineNumbersMode::Off | DisplayLineNumbersMode::Absolute => {
                (self.current_line + offset).abs()
            }
        }
    }

    pub(crate) fn take_text_prefix(
        &mut self,
        mode: crate::types::DisplayLineNumbersMode,
        current_absolute: bool,
        offset: i64,
        major_tick: i32,
        field: LineNumberFieldLayout,
    ) -> Option<LineNumberTextPrefix> {
        let LineNumberRenderPhase::Pending(prefix) = self.phase else {
            return None;
        };

        let content = match prefix {
            LineNumberRowPrefix::Numbered => LineNumberTextPrefixContent::Number(
                self.display_number(mode, current_absolute, offset),
            ),
            LineNumberRowPrefix::ReservedBlank(_) => LineNumberTextPrefixContent::Blank,
        };
        let face = if matches!(
            prefix,
            LineNumberRowPrefix::ReservedBlank(LineNumberBlankReason::BeyondAccessibleEnd)
        ) {
            LineNumberTextPrefixFace::Normal
        } else if self.is_current_line() {
            LineNumberTextPrefixFace::CurrentLine
        } else if major_tick > 0 && self.current_line % i64::from(major_tick) == 0 {
            LineNumberTextPrefixFace::MajorTick
        } else {
            LineNumberTextPrefixFace::Normal
        };
        let request = LineNumberTextPrefix {
            content,
            field,
            face,
        };
        self.phase = LineNumberRenderPhase::Rendered;
        Some(request)
    }
}

impl FaceScanCheckpoint {
    pub(crate) fn initial() -> Self {
        Self { next_check: 0 }
    }

    pub(crate) fn should_resolve_at(self, charpos: usize) -> bool {
        charpos >= self.next_check
    }

    pub(crate) fn invalidate(&mut self) {
        self.next_check = 0;
    }

    pub(crate) fn next_check_mut(&mut self) -> &mut usize {
        &mut self.next_check
    }
}

impl BoxFaceRowState {
    pub(crate) fn inactive() -> Self {
        Self::Inactive
    }

    pub(crate) fn activate(&mut self, row: DisplayRowMarker, start_x: f32) {
        *self = Self::Active { row, start_x };
    }

    pub(crate) fn continue_on_row(&mut self, row: DisplayRowMarker, start_x: f32) {
        if self.is_active() {
            self.activate(row, start_x);
        }
    }

    pub(crate) fn clear(&mut self) {
        *self = Self::Inactive;
    }

    pub(crate) fn is_active(&self) -> bool {
        matches!(self, Self::Active { .. })
    }

    #[cfg(test)]
    pub(crate) fn start_x(&self) -> Option<f32> {
        match self {
            Self::Active { start_x, .. } => Some(*start_x),
            Self::Inactive => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn row(&self) -> DisplayRowMarker {
        match self {
            Self::Active { row, .. } => *row,
            Self::Inactive => DisplayRowMarker::Inactive,
        }
    }
}

impl TrailingWhitespaceRenderState {
    pub(crate) fn new(enabled: bool, background_pixel: u32) -> Self {
        Self {
            background: enabled.then(|| Color::from_pixel(background_pixel)),
            start_marker: DisplayRowStartMarker::Inactive,
        }
    }

    #[cfg(test)]
    pub(crate) fn background(self) -> Option<Color> {
        self.background
    }

    #[cfg(test)]
    pub(crate) fn start_marker(self) -> DisplayRowStartMarker {
        self.start_marker
    }

    pub(crate) fn reset_after_row_transition(&mut self) {
        self.start_marker = DisplayRowStartMarker::Inactive;
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.background.is_some()
    }

    pub(crate) fn track_rendered_char(&mut self, ch: char, start_marker: DisplayRowStartMarker) {
        if self.background.is_none() {
            return;
        }

        if ch == ' ' || ch == '\t' {
            if !self.start_marker.is_active() {
                self.start_marker = start_marker;
            }
        } else {
            self.reset_after_row_transition();
        }
    }

    #[cfg(test)]
    pub(crate) fn highlight_start_x(
        self,
        geometry: &DisplayRowGeometryState,
    ) -> Option<(Color, f32)> {
        Some((self.background?, self.start_marker.x_on(geometry)?))
    }
}

impl HitRowRangeTracker {
    pub(crate) fn new(start_charpos: i64) -> Self {
        Self { start_charpos }
    }

    pub(crate) fn start(self) -> i64 {
        self.start_charpos
    }

    pub(crate) fn range_to(self, end_charpos: i64) -> DisplayRowHitRange {
        DisplayRowHitRange {
            charpos_start: self.start_charpos,
            charpos_end: end_charpos,
        }
    }

    pub(crate) fn advance_to(&mut self, start_charpos: i64) {
        self.start_charpos = start_charpos;
    }

    pub(crate) fn should_finish_current_row(
        self,
        current_charpos: i64,
        has_pending_row_output: bool,
    ) -> bool {
        current_charpos > self.start_charpos || has_pending_row_output
    }
}

/// Sync the source text position to `synced_charpos` and advance the hit-row
/// range to follow it after a row transition. GNU's xdisp.c keeps this as one
/// operation; the line-break, hidden-line-break, and truncation-skip actions
/// all perform exactly this step.
pub(crate) fn sync_position_after_row_transition(
    synced_charpos: i64,
    position: &mut DisplaySourceTextPosition,
    hit_row_range: &mut HitRowRangeTracker,
) {
    *position = position.with_charpos(synced_charpos);
    hit_row_range.advance_to(position.charpos());
}

impl InvisibleTextScanCheckpoint {
    pub(crate) fn new(start_charpos: i64) -> Self {
        Self {
            invisible_next: start_charpos,
        }
    }

    pub(crate) fn should_check(self, charpos: i64) -> bool {
        charpos >= self.invisible_next
    }

    pub(crate) fn record_next_visible(&mut self, charpos: i64) {
        self.invisible_next = charpos;
    }
}

/// Window start after scrolling down by `rows_to_scroll` DISPLAY rows, given the
/// rows laid out from `current_start`.
///
/// This is GNU's `move_it_vertically (&it, amount_to_scroll)` from the old
/// window start (src/xdisp.c:19526) evaluated against real display rows, so it
/// is exact for wrapped lines, images and variable-height rows. Scrolling past
/// the rows we have is not: those rows were never laid out, so the caller falls
/// back to a buffer-line walk and the visibility retry converges on the rest.
///
/// `None` when the rows cannot produce a start further down than
/// `current_start` — the caller must not retry, or it would loop.
pub(crate) fn next_window_start_from_visible_rows(
    rows: &[DisplayRowSnapshot],
    current_start: i64,
    rows_to_scroll: i64,
) -> Option<i64> {
    if rows.is_empty() || rows_to_scroll <= 0 {
        return None;
    }

    // Row starts strictly below the current one, in display order. A row that
    // begins where the previous one did (a continuation carrying no new buffer
    // position) is not a distinct scroll step.
    let mut starts = rows
        .iter()
        .filter_map(row_next_window_start_charpos)
        .filter(|&pos| pos > current_start);
    let mut chosen = starts.next()?;
    for _ in 1..rows_to_scroll {
        match starts.next() {
            Some(pos) => chosen = pos,
            // Fewer laid-out rows than the scroll amount: return the furthest
            // start we can prove, and let the caller extend it.
            None => break,
        }
    }
    Some(chosen)
}

/// Rows the caller can still scroll through beyond `window_start_after`, i.e.
/// how much of a requested scroll [`next_window_start_from_visible_rows`] could
/// actually satisfy.
pub(crate) fn visible_rows_below(rows: &[DisplayRowSnapshot], current_start: i64) -> i64 {
    rows.iter()
        .filter_map(row_next_window_start_charpos)
        .filter(|&pos| pos > current_start)
        .count() as i64
}

#[inline]
fn row_start_charpos(row: &DisplayRowSnapshot) -> Option<i64> {
    row.start_buffer_pos.map(lisp_char_pos_to_layout_i64)
}

#[inline]
fn row_end_charpos(row: &DisplayRowSnapshot) -> Option<i64> {
    row.end_buffer_pos.map(lisp_char_pos_to_layout_i64)
}

#[inline]
pub(crate) fn row_next_window_start_charpos(row: &DisplayRowSnapshot) -> Option<i64> {
    row.end_buffer_pos
        .map(LispCharPos1::as_i64)
        .or_else(|| row_start_charpos(row))
}

pub(crate) fn next_window_start_for_partially_visible_point_row(
    rows: &[DisplayRowSnapshot],
    point: i64,
    text_area_top: i64,
    text_area_bottom: i64,
    current_start: i64,
) -> Option<i64> {
    let text_area_height = text_area_bottom.saturating_sub(text_area_top);
    let point_row_index = rows.iter().position(|row| {
        let start = row_start_charpos(row).unwrap_or(i64::MAX);
        let end = row_end_charpos(row).unwrap_or(i64::MIN);
        start <= point && point <= end
    })?;
    let point_row = &rows[point_row_index];
    if point_row.height > text_area_height {
        return None;
    }

    let row_top = point_row.y;
    let row_bottom = point_row.y.saturating_add(point_row.height);
    if row_top >= text_area_top && row_bottom <= text_area_bottom {
        return None;
    }

    if row_bottom > text_area_bottom {
        let overflow = row_bottom.saturating_sub(text_area_bottom);
        let mut lifted = 0i64;
        for row in rows.iter().take(point_row_index) {
            lifted = lifted.saturating_add(row.height.max(1));
            let candidate = row_next_window_start_charpos(row);
            if lifted >= overflow
                && let Some(pos) = candidate
                && pos > current_start
            {
                return Some(pos);
            }
        }
    }

    None
}

pub(crate) fn next_window_start_for_point_line_continuation<B: LayoutBufferView>(
    rows: &[DisplayRowSnapshot],
    point: i64,
    current_start: i64,
    buf_access: &RustBufferAccess<'_, B>,
    buffer_size: i64,
) -> Option<i64> {
    let point_row_index = rows.iter().position(|row| {
        let start = row_start_charpos(row).unwrap_or(i64::MAX);
        let end = row_end_charpos(row).unwrap_or(i64::MIN);
        start <= point && point <= end
    })?;
    let point_row = rows.get(point_row_index)?;
    let point_is_visible_row_start =
        row_start_charpos(point_row).is_some_and(|start| start == point);

    for row in rows.iter().skip(point_row_index) {
        let end_pos = row.end_buffer_pos?.as_i64();
        let end_byte = buf_access.lisp_charpos_to_bytepos(end_pos);
        if matches!(buf_access.byte_at(end_byte), Some(b'\n')) {
            return None;
        }
        let next_pos = end_pos.saturating_add(1);
        if next_pos > buffer_size {
            return None;
        }

        let next_byte = buf_access.lisp_charpos_to_bytepos(next_pos);
        match buf_access.byte_at(next_byte) {
            Some(b'\n') | None => return None,
            Some(_) if std::ptr::eq(row, rows.last()?) => {
                if point_is_visible_row_start {
                    return point
                        .checked_sub(1)
                        .filter(|&new_start| new_start > current_start);
                }
                break;
            }
            Some(_) => {}
        }
    }

    if point_row_index + 1 < rows.len() {
        return None;
    }

    rows.iter()
        .skip(1)
        .find_map(row_next_window_start_charpos)
        .filter(|&pos| pos > current_start)
}

#[inline]
fn is_word_wrap_whitespace(ch: char) -> bool {
    matches!(ch, ' ' | '\t')
}

#[inline]
fn char_can_wrap_before_basic(ch: char) -> bool {
    !matches!(ch, ' ' | '\t' | '\n' | '\r')
}

#[inline]
fn char_can_wrap_after_basic(ch: char) -> bool {
    is_word_wrap_whitespace(ch)
}
