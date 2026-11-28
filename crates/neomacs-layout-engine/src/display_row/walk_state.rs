use crate::coords::lisp_char_pos_to_layout_i64;
use crate::display_row::builder::{DisplayRowGlyphCheckpoint, DisplayRowPosition};
use crate::display_row::face_state::DisplayRowExtendFace;
#[cfg(test)]
use crate::display_row::geometry::DisplayRowGeometryState;
use crate::display_row::geometry::{DisplayRowMarker, DisplayRowStartMarker};
use crate::display_source::{DisplaySourceResumeCursor, DisplaySourceTextPosition};
use crate::neovm_bridge::{LayoutBufferView, RustBufferAccess};
use crate::scroll_policy::{SCROLL_CONSERVATIVELY_LIMIT, ScrollPolicy};
use crate::types::LineWrapMode;
use neomacs_display_protocol::types::Color;
use neovm_core::buffer::LispCharPos1;
use neovm_core::window::{DisplayRowSnapshot, ScrollMargin};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct WordWrapBreakCandidate {
    /// Typed window-slice position and insertion phase. Glyph state may retain
    /// an overlay string even when its buffer anchor is the resume position.
    source_cursor: DisplaySourceResumeCursor,
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

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WordWrapRenderState {
    enabled: bool,
    policy: neovm_core::window::WordWrapPolicy,
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
    last_numbered_line: Option<i64>,
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

/// The source charpos at which the current display row's text starts.
///
/// GNU's `display_line` keeps the same fact in `it->current.pos` at row entry
/// and consults it to tell an empty row from one that consumed source
/// (xdisp.c `row->used[TEXT_AREA] == 0` guards). Both users here need that
/// origin: deciding whether the pending row may be finished at all, and
/// stamping the charpos the NEXT row starts at across a row break.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DisplayRowSourceStart {
    start_charpos: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InvisibleTextScanCheckpoint {
    invisible_next: i64,
    /// Character offset into the ellipsis display vector (GNU `dpvec_index`,
    /// src/xdisp.c:4115-4121) for an already-consumed hidden span whose
    /// ellipsis clipped on the previous wrapping row. `None` when settled.
    deferred_ellipsis: Option<usize>,
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
    line_number: TextRowLineNumberTransition,
    word_wrap: TextRowTransitionWordWrapAction,
    reset_trailing_whitespace: bool,
    prefix: TextRowTransitionPrefixAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextRowLineNumberTransition {
    BufferNewline,
    DisplayStringNewline,
    Keep,
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
            DisplaySourceResumeCursor::AfterOverlayStrings(DisplaySourceTextPosition::new(
                byte_idx, charpos,
            )),
            display_point_count,
            row_display_positions,
            glyph_checkpoint,
            DisplayRowPosition::default(),
            None,
        );
    }

    pub(crate) fn record_at(
        &mut self,
        source_cursor: DisplaySourceResumeCursor,
        display_point_count: usize,
        row_display_positions: (Option<LispCharPos1>, Option<LispCharPos1>),
        glyph_checkpoint: DisplayRowGlyphCheckpoint,
        row_position: DisplayRowPosition,
        row_extend: Option<DisplayRowExtendFace>,
    ) {
        self.source_cursor = source_cursor;
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
        self.source_cursor.position().byte_idx()
    }

    #[cfg(test)]
    pub(crate) fn charpos(&self) -> i64 {
        self.source_cursor.position().charpos()
    }

    pub(crate) fn source_position(self) -> DisplaySourceTextPosition {
        self.source_cursor.position()
    }

    pub(crate) fn source_cursor(self) -> DisplaySourceResumeCursor {
        self.source_cursor
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
        word_wrap: Option<WordWrapBreakCandidate>,
    ) -> Self {
        if ch == '\t' || x_px + advance_px <= right_edge_px {
            Self::Fits
        } else if wrap_mode == LineWrapMode::Truncate {
            // Wide-glyph cut case (issue #446 wide-name): a wide character
            // whose first cell fits is still refused here, but the truncation
            // arm records the cut as the row's WideCut flag (overflow.rs),
            // and the marker installer then overwrites the padding cells —
            // the cut glyph's cells — with the truncation glyph, matching
            // GNU's visible result (append + overwrite,
            // xdisp.c:26611-26641) without touching the walk/append
            // lifecycle.
            Self::Truncate
        } else if let Some(break_candidate) = word_wrap {
            Self::WordWrap { break_candidate }
        } else {
            Self::CharacterWrap
        }
    }
}

impl TextRowTransitionStatePolicy {
    fn new(
        line_number: TextRowLineNumberTransition,
        word_wrap: TextRowTransitionWordWrapAction,
        reset_trailing_whitespace: bool,
        prefix: TextRowTransitionPrefixAction,
    ) -> Self {
        Self {
            line_number,
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
            TextRowLineNumberTransition::BufferNewline,
            TextRowTransitionWordWrapAction::Keep,
            true,
            TextRowTransitionPrefixAction::Line,
        )
    }

    pub(crate) fn line_break() -> Self {
        Self::new(
            TextRowLineNumberTransition::BufferNewline,
            TextRowTransitionWordWrapAction::Reset,
            false,
            TextRowTransitionPrefixAction::Line,
        )
    }

    pub(crate) fn display_string_line_break() -> Self {
        Self {
            line_number: TextRowLineNumberTransition::DisplayStringNewline,
            ..Self::line_break()
        }
    }

    pub(crate) fn hidden_line_break() -> Self {
        Self::new(
            TextRowLineNumberTransition::BufferNewline,
            TextRowTransitionWordWrapAction::Reset,
            true,
            TextRowTransitionPrefixAction::Line,
        )
    }

    pub(crate) fn truncation() -> Self {
        Self::new(
            TextRowLineNumberTransition::Keep,
            TextRowTransitionWordWrapAction::Reset,
            true,
            TextRowTransitionPrefixAction::Line,
        )
    }

    pub(crate) fn special_truncation() -> Self {
        Self::new(
            TextRowLineNumberTransition::Keep,
            TextRowTransitionWordWrapAction::Disallow,
            true,
            TextRowTransitionPrefixAction::Line,
        )
    }

    pub(crate) fn visual_wrap() -> Self {
        Self::new(
            TextRowLineNumberTransition::Keep,
            TextRowTransitionWordWrapAction::Reset,
            true,
            TextRowTransitionPrefixAction::Wrap,
        )
    }

    pub(crate) fn special_visual_wrap() -> Self {
        Self::new(
            TextRowLineNumberTransition::Keep,
            TextRowTransitionWordWrapAction::Keep,
            true,
            TextRowTransitionPrefixAction::Wrap,
        )
    }

    pub(crate) fn character_wrap() -> Self {
        Self::new(
            TextRowLineNumberTransition::Keep,
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
        match self.line_number {
            TextRowLineNumberTransition::BufferNewline => line_numbers.advance_line(),
            TextRowLineNumberTransition::DisplayStringNewline => {
                line_numbers.mark_display_string_break()
            }
            TextRowLineNumberTransition::Keep => {}
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
            policy: Default::default(),
            may_wrap: false,
            candidate: WordWrapBreakCandidate::default(),
        }
    }

    pub(crate) fn with_policy(mut self, policy: neovm_core::window::WordWrapPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn can_record_candidate(&self, ch: char) -> bool {
        self.enabled
            && self.may_wrap
            && !matches!(ch, '\n' | '\r')
            && self
                .policy
                .opportunity(ch as u32, neovm_core::window::WordWrapDirection::Forward)
                .before()
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
            DisplaySourceResumeCursor::AfterOverlayStrings(DisplaySourceTextPosition::new(
                byte_idx, charpos,
            )),
            display_point_count,
            row_display_positions,
            glyph_checkpoint,
            DisplayRowPosition::default(),
            None,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record_candidate_at(
        &mut self,
        ch: char,
        source_cursor: DisplaySourceResumeCursor,
        display_point_count: usize,
        row_display_positions: (Option<LispCharPos1>, Option<LispCharPos1>),
        glyph_checkpoint: DisplayRowGlyphCheckpoint,
        row_position: DisplayRowPosition,
        row_extend: Option<DisplayRowExtendFace>,
    ) {
        if self.can_record_candidate(ch) {
            self.candidate.record_at(
                source_cursor,
                display_point_count,
                row_display_positions,
                glyph_checkpoint,
                row_position,
                row_extend,
            );
        }
    }

    pub(crate) fn allow_after_current_char(&mut self, ch: char) {
        self.may_wrap = self
            .policy
            .opportunity(ch as u32, neovm_core::window::WordWrapDirection::Forward)
            .after();
    }

    pub(crate) fn disallow_after_current_char(&mut self) {
        self.may_wrap = false;
    }

    pub(crate) fn reset_after_row_transition(&mut self) {
        self.may_wrap = false;
        self.candidate.clear();
    }

    pub(crate) fn has_candidate(&self) -> bool {
        self.enabled && self.candidate.is_available()
    }

    pub(crate) fn checkpoint(&self) -> Option<WordWrapBreakCandidate> {
        self.has_candidate().then_some(self.candidate)
    }

    pub(crate) fn candidate(&self) -> WordWrapBreakCandidate {
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

    /// The window's hscroll in columns: GNU's `first_visible_x` in cells.
    pub(crate) fn configured_columns(self) -> i32 {
        self.configured_columns
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

    /// GNU's long-line hscroll jump (`fast_move_it_horizontally`,
    /// src/xdisp.c:25896-25920) stopped exactly at the first visible
    /// boundary: the next source item is the glyph the left-truncation
    /// marker replaces, or, with line numbers, the marker is complete.
    pub(crate) fn complete_at_visible_boundary(&mut self) {
        self.phase = match self.truncation_target {
            HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph => {
                HorizontalScrollSkipPhase::ReplaceNextGlyph
            }
            HorizontalScrollTruncationTarget::LineNumberPrefix => {
                HorizontalScrollSkipPhase::Complete
            }
        };
    }

    /// The line ended before the hscroll boundary, so the jump clamped at the
    /// newline and no source cell remains to consume on this row.
    pub(crate) fn complete_hidden_line(&mut self) {
        self.phase = HorizontalScrollSkipPhase::Complete;
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
    pub(crate) fn is_enabled(self) -> bool {
        !matches!(self.phase, LineNumberRenderPhase::Disabled)
    }

    pub(crate) fn new(enabled: bool, current_line: i64, point_line: i64) -> Self {
        Self {
            current_line,
            point_line,
            last_numbered_line: None,
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

    /// Row break triggered by a synthetic newline (pushed `display` string or
    /// display-table vector `?\n`) without crossing a buffer `'\n'` at the
    /// break itself. Prints the current line number only if `current_line` has
    /// advanced since the last numbered row (e.g. a previous display vector for
    /// `'\n'` finished mid-row); otherwise reserves a blank continuation gutter.
    pub(crate) fn mark_display_string_break(&mut self) {
        if self.last_numbered_line != Some(self.current_line) {
            self.arm_prefix(LineNumberRowPrefix::Numbered);
        } else {
            self.mark_continuation_row();
        }
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

    /// A replacement consumes BUFFER text only when its pushed iterator ends.
    /// Its string newlines never contribute to this logical line count.
    pub(crate) fn resume_after_replacement(&mut self, buffer_newlines: i64) {
        self.current_line += buffer_newlines;
        if buffer_newlines > 0 && matches!(self.phase, LineNumberRenderPhase::Pending(_)) {
            self.arm_prefix(LineNumberRowPrefix::Numbered);
        }
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
            LineNumberRowPrefix::Numbered => {
                self.last_numbered_line = Some(self.current_line);
                LineNumberTextPrefixContent::Number(self.display_number(
                    mode,
                    current_absolute,
                    offset,
                ))
            }
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

impl DisplayRowSourceStart {
    pub(crate) fn new(start_charpos: i64) -> Self {
        Self { start_charpos }
    }

    /// Only the walk-setup test reads the seed back; production consults the
    /// origin through [`Self::should_finish_current_row`] instead.
    #[cfg(test)]
    pub(crate) fn start(self) -> i64 {
        self.start_charpos
    }

    pub(crate) fn advance_to(&mut self, start_charpos: i64) {
        self.start_charpos = start_charpos;
    }

    /// Whether `charpos` sits at or after the row's source start — used by
    /// the hscroll skip to tell that a point hidden in the hscrolled-off
    /// prefix belongs to THIS line, so its cursor clamps to the truncation
    /// marker's cell.
    pub(crate) fn covers(self, charpos: i64) -> bool {
        charpos >= self.start_charpos
    }

    pub(crate) fn should_finish_current_row(
        self,
        current_charpos: i64,
        has_pending_row_output: bool,
    ) -> bool {
        current_charpos > self.start_charpos || has_pending_row_output
    }
}

/// Sync the source text position to `synced_charpos` and advance the display
/// row's source start to follow it after a row transition. GNU's xdisp.c keeps
/// this as one operation; the line-break, hidden-line-break, and
/// truncation-skip actions all perform exactly this step.
pub(crate) fn sync_position_after_row_transition(
    synced_charpos: i64,
    position: &mut DisplaySourceTextPosition,
    row_source_start: &mut DisplayRowSourceStart,
) {
    *position = position.with_charpos(synced_charpos);
    row_source_start.advance_to(position.charpos());
}

impl InvisibleTextScanCheckpoint {
    pub(crate) fn new(start_charpos: i64) -> Self {
        Self {
            invisible_next: start_charpos,
            deferred_ellipsis: None,
        }
    }

    pub(crate) fn should_check(self, charpos: i64) -> bool {
        charpos >= self.invisible_next
    }

    pub(crate) fn record_next_visible(&mut self, charpos: i64) {
        self.invisible_next = charpos;
    }

    pub(crate) fn defer_ellipsis_from(&mut self, char_offset: usize) {
        self.deferred_ellipsis = Some(char_offset);
    }

    /// Takes the pending ellipsis resume offset (GNU `dpvec_index`).
    pub(crate) fn take_deferred_ellipsis(&mut self) -> Option<usize> {
        self.deferred_ellipsis.take()
    }

    pub(crate) fn rewind_to(&mut self, position: crate::display_source::DisplaySourceTextPosition) {
        self.invisible_next = position.charpos();
        self.deferred_ellipsis = None;
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

/// Addressable buffer starts the caller can scroll through beyond `current_start`, i.e.
/// how much of a requested scroll [`next_window_start_from_visible_rows`] could
/// actually satisfy.
///
/// This is NOT the number of visible display rows: a string-only row can
/// occupy screen space without offering a new buffer position to resume at.
pub(crate) fn scrollable_row_count(rows: &[DisplayRowSnapshot], current_start: i64) -> i64 {
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
    use neovm_core::window::DisplayRowEndSource;
    match row.end_source {
        DisplayRowEndSource::Buffer => row
            .end_buffer_pos
            .map(LispCharPos1::as_i64)
            .or_else(|| row_start_charpos(row)),
        // The display string's row break consumed no buffer character. Resume
        // at its source anchor, not at an invented anchor + 1 inside the
        // replacement. Visible row positions stay valid one-based positions.
        DisplayRowEndSource::DisplayStringNewline
        | DisplayRowEndSource::DisplayStringWrap
        | DisplayRowEndSource::OverlayBeforeString
        | DisplayRowEndSource::OverlayAfterString => row_end_charpos(row),
    }
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
    if point_row.height >= text_area_height {
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

/// Window-level inputs GNU `cursor_row_fully_visible_p` and `try_scrolling`
/// consult when point lands on a partially visible bottom row at least as tall
/// as the window's text area (GNU `src/xdisp.c:19310-19704, 20098-20126,
/// 21052-21245`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TallPointRowScrollContext {
    pub(crate) default_line_height: i64,
    pub(crate) scroll_step: i64,
    pub(crate) make_cursor_line_fully_visible: bool,
    pub(crate) is_minibuffer: bool,
    pub(crate) has_vscroll: bool,
    pub(crate) force_start: bool,
    pub(crate) temp_scroll_step: bool,
    pub(crate) already_scrolled_this_redisplay: bool,
    /// Initial `extra_scroll_margin_lines` / `last_line_misfit` (`src/xdisp.c:19390`).
    /// Starts at `0` on the initial pass (both cursor-only and `try_window`
    /// with `TRY_WINDOW_CHECK_MARGINS` returning `-1` on a bottom-overflowing
    /// tall row), and `1` only when continuing after an off-screen scroll-down
    /// pass that already ran `extra = 0`.
    pub(crate) initial_extra_scroll_margin_lines: i64,
    /// Maximum rendered glyph height on `point_row` strictly before `point`
    /// (`it->max_ascent + it->max_descent` in GNU `line_bottom_y`,
    /// `src/xdisp.c:1511-1543`). `None` at BOL so GNU falls back to
    /// `last_height` (previous row's height).
    pub(crate) point_row_prefix_height: Option<i64>,
    /// Whether `point_row` reaches `ZV` (`IT_EOB`) without a trailing newline
    /// and without truncation (`partial_line_height`, `src/xdisp.c:11647-11652`).
    pub(crate) point_row_ends_at_eob_without_newline: bool,
}

/// Target returned by [`next_window_start_for_tall_partially_visible_point_row`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TallPointRowScrollTarget {
    /// Set `window-start` to an exact layout charpos from the laid-out rows.
    WindowStart(i64),
    /// Move `window-start` backward by `lines` buffer lines above `current_start`
    /// (GNU `recenter:` when `move_it_vertically_backward` steps above the
    /// current viewport start, e.g. under a header-line where `w->cursor.vpos > 0`).
    LinesBeforeCurrentStart(i64),
}

/// Whether `point`'s row ends at `accessible_end` (`ZV`) without a trailing
/// newline and without truncation (GNU `partial_line_height`'s `IT_EOB` branch,
/// `src/xdisp.c:11647-11652`).
pub(crate) fn point_row_ends_at_eob_without_newline<B: LayoutBufferView>(
    rows: &[DisplayRowSnapshot],
    point: i64,
    accessible_end: i64,
    buf_access: &RustBufferAccess<'_, B>,
) -> bool {
    let Some(point_row) = rows.iter().find(|row| {
        let start = row_start_charpos(row).unwrap_or(i64::MAX);
        let end = row_end_charpos(row).unwrap_or(i64::MIN);
        start <= point && point <= end
    }) else {
        return false;
    };
    if point_row.truncated_end_buffer_pos.is_some() {
        return false;
    }
    let Some(end_lisp) = point_row.end_buffer_pos.map(LispCharPos1::as_i64) else {
        return false;
    };
    if end_lisp < accessible_end {
        return false;
    }
    let end_byte = buf_access.lisp_charpos_to_bytepos(end_lisp);
    !matches!(buf_access.byte_at(end_byte), Some(b'\n'))
}

/// Choose the next `window-start` when point sits on a partially visible bottom
/// row at least as tall as the text area (`point_row.height >= text_area_height`).
///
/// Mirrors GNU Emacs 31.1 (`src/xdisp.c`):
/// - `cursor_row_fully_visible_p` (`xdisp.c:19310-19336`) suppresses scrolling
///   when `make-cursor-line-fully-visible` is nil, for minibuffers, non-zero
///   `vscroll`, forced starts (`force_p == false`), window-absolute `vpos == 0`,
///   or after a scroll has already committed in this redisplay.
/// - `try_cursor_movement` (`xdisp.c:20110-20125`) checks strict
///   `row->height > window_box_height(w)` before setting `temp_scroll_step`;
///   an exact-height row (`row->height == text_area_height`) on cursor-only
///   motion passes `cursor_row_fully_visible_p(w, false, ...)` and does not scroll.
/// - `try_scrolling` (`xdisp.c:19380-19704`) runs when `scroll-conservatively`
///   or `scroll-step` is non-zero, or when cursor-only motion sets
///   `temp_scroll_step = true`. `scroll_max` uses
///   `max(scroll_step, scroll_conservatively, temp_scroll_step)` while the
///   conservative per-iteration clamp uses `scroll_conservatively`. `dy` and
///   `scroll_down_p` persist across `too_near_end:` when `to_y < 0` or
///   `to_y >= y_before`, `line_bottom_y` uses `point_row_prefix_height` before
///   falling back to `last_height` at BOL, `partial_line_height` returns `0` at
///   untruncated `IT_EOB` when `vis_height >= line_h`, and any overshoot past
///   `point_row_index` fails (`w->cursor.vpos < 0`) into `recenter:`.
/// - When `try_scrolling` is skipped or fails, `recenter:` (`xdisp.c:21169-21237`)
///   steps backward from `point_row` via the shared
///   [`neovm_core::window::geometry::move_it_vertically_backward_row_index`],
///   resets to `point_row` if `moved_px >= text_area_height` (`xdisp.c:21232`),
///   and steps backward above `current_start` when shortfall remains.
#[allow(clippy::too_many_arguments)]
pub(crate) fn next_window_start_for_tall_partially_visible_point_row(
    rows: &[DisplayRowSnapshot],
    point: i64,
    text_area_top: i64,
    text_area_bottom: i64,
    current_start: i64,
    scroll_policy: ScrollPolicy,
    scroll_margin: ScrollMargin,
    ctx: TallPointRowScrollContext,
) -> Option<TallPointRowScrollTarget> {
    let text_area_height = text_area_bottom.saturating_sub(text_area_top);
    if text_area_height <= 0 {
        return None;
    }
    let first_body_idx = rows
        .iter()
        .position(|row| row.start_buffer_pos.is_some())
        .unwrap_or(0);
    let point_row_index = rows.iter().position(|row| {
        let start = row_start_charpos(row).unwrap_or(i64::MAX);
        let end = row_end_charpos(row).unwrap_or(i64::MIN);
        start <= point && point <= end
    })?;
    let point_row = &rows[point_row_index];
    if point_row.height < text_area_height {
        return None;
    }

    let row_bottom = point_row.y.saturating_add(point_row.height);
    if row_bottom <= text_area_bottom {
        return None;
    }
    if !ctx.make_cursor_line_fully_visible
        || ctx.already_scrolled_this_redisplay
        || ctx.is_minibuffer
        || ctx.has_vscroll
        || ctx.force_start
        || point_row.row == 0
        || (point_row.height == text_area_height && ctx.temp_scroll_step)
    {
        return None;
    }

    let line_h = ctx.default_line_height.max(1);
    let viewport_rows = (text_area_height / line_h).max(1);
    let margin_lines = scroll_margin.lines(viewport_rows);
    let margin_px = margin_lines.saturating_mul(line_h);

    let row_candidate = |idx: usize| -> Option<i64> {
        if idx <= first_body_idx {
            return None;
        }
        row_start_charpos(&rows[idx])
            .or_else(|| row_next_window_start_charpos(&rows[idx - 1]))
            .filter(|&pos| pos > current_start)
    };

    let enters_try_scrolling =
        !matches!(scroll_policy, ScrollPolicy::Recenter) || ctx.temp_scroll_step;
    if enters_try_scrolling {
        let is_unlimited = matches!(scroll_policy, ScrollPolicy::Unlimited);
        let (is_conservative, conservative_cap_px, scroll_max_px) = match scroll_policy {
            ScrollPolicy::Unlimited => (
                true,
                SCROLL_CONSERVATIVELY_LIMIT
                    .saturating_add(1)
                    .saturating_mul(line_h),
                SCROLL_CONSERVATIVELY_LIMIT.saturating_mul(line_h),
            ),
            ScrollPolicy::Conservative { max_lines } => {
                let cap = max_lines.max(1).saturating_mul(line_h);
                let max_lines_eff = ctx
                    .scroll_step
                    .max(max_lines)
                    .max(i64::from(ctx.temp_scroll_step))
                    .clamp(0, 1_000_000);
                (true, cap, max_lines_eff.saturating_mul(line_h))
            }
            ScrollPolicy::Step { lines } => {
                let max_px = lines.clamp(1, 1_000_000).saturating_mul(line_h);
                (false, max_px, max_px)
            }
            ScrollPolicy::Recenter => (false, line_h, line_h),
        };
        let point_line_bottom_delta = ctx
            .point_row_prefix_height
            .unwrap_or_else(|| {
                if point_row_index > first_body_idx {
                    rows[point_row_index - 1].height
                } else {
                    line_h
                }
            })
            .max(1);
        let mut start_idx = first_body_idx;
        let mut extra: i64 = ctx.initial_extra_scroll_margin_lines.max(0);
        let mut dy: i64 = 0;
        let mut scroll_down_p = false;
        let mut succeeded = false;

        loop {
            let start_y = rows[start_idx].y;
            let y_before = point_row.y.saturating_sub(start_y);
            let vis_height = text_area_height.saturating_sub(y_before);
            let partial_height = if ctx.point_row_ends_at_eob_without_newline {
                if vis_height < line_h { vis_height } else { 0 }
            } else {
                vis_height.max(0)
            };
            let scroll_margin_y = text_area_height
                .saturating_sub(partial_height)
                .saturating_sub(margin_px)
                .saturating_sub(extra.saturating_mul(line_h));
            let to_y = scroll_margin_y.saturating_sub(1);

            // GNU `move_it_to(&it, PT, -1, scroll_margin_y - 1, -1, MOVE_TO_POS | MOVE_TO_Y)`:
            // negative `to_y` disables the Y stop (`src/xdisp.c:10996, 11010`), and
            // `to_y >= y_before` reaches `PT` first. In both cases `PT > CHARPOS(it)`
            // is false and `dy` / `scroll_down_p` keep their prior-iteration values.
            if to_y >= 0 && to_y < y_before {
                let mut y0 = y_before;
                for row in &rows[start_idx..point_row_index] {
                    let rel_bottom = row.y.saturating_add(row.height).saturating_sub(start_y);
                    if rel_bottom > to_y {
                        y0 = rel_bottom;
                        break;
                    }
                }
                dy = y_before
                    .saturating_add(point_line_bottom_delta)
                    .saturating_sub(y0);
                if dy > scroll_max_px {
                    break;
                }
                if dy > 0 {
                    scroll_down_p = true;
                }
            }

            if scroll_down_p {
                let amount_to_scroll = if is_conservative {
                    dy.max(line_h).min(conservative_cap_px)
                } else {
                    scroll_max_px
                };
                if amount_to_scroll <= 0 {
                    break;
                }
                let mut next_idx = start_idx;
                if is_unlimited {
                    // GNU `src/xdisp.c:19548-19570`: step one screen line at a
                    // time while `line_bottom_y(&it1) - start_y < amount_to_scroll`.
                    let start_row_h = rows[start_idx].height.max(1);
                    let mut y_acc = 0i64;
                    loop {
                        let row_h = rows[next_idx].height.max(1);
                        y_acc = y_acc.saturating_add(row_h);
                        next_idx += 1;
                        let line_bottom = y_acc.saturating_add(row_h);
                        if next_idx > point_row_index
                            || line_bottom.saturating_sub(start_row_h) >= amount_to_scroll
                        {
                            break;
                        }
                    }
                } else {
                    let mut lifted = 0i64;
                    while next_idx <= point_row_index && lifted < amount_to_scroll {
                        lifted = lifted.saturating_add(rows[next_idx].height.max(1));
                        next_idx += 1;
                    }
                    if next_idx == start_idx {
                        next_idx += 1;
                    }
                }
                if next_idx > point_row_index {
                    // Overshot past `PT`: `try_window` leaves `w->cursor.vpos = -1`
                    // -> `SCROLLING_FAILED` -> `recenter:`.
                    break;
                }
                start_idx = next_idx;
            }

            // GNU `cursor_row_fully_visible_p` (`src/xdisp.c:19332-19334`)
            // stops early only when `w->cursor.vpos == 0` (window-absolute,
            // so `rows[first_body_idx].row == 0`) or `extra > 1` (`!force_p`).
            // Under a header-line (`rows[first_body_idx].row > 0`), landing
            // `start_idx == point_row_index` at `extra <= 1` keeps iterating.
            let point_at_window_vpos_zero =
                start_idx == point_row_index && rows[first_body_idx].row == 0;
            if point_at_window_vpos_zero || extra > 1 {
                succeeded = true;
                break;
            }
            extra += 1;
        }

        if succeeded {
            return if start_idx == first_body_idx {
                None
            } else {
                row_candidate(start_idx).map(TallPointRowScrollTarget::WindowStart)
            };
        }
    }

    let last_line_misfit = i64::from(ctx.initial_extra_scroll_margin_lines > 0);
    let centering_position = if matches!(scroll_policy, ScrollPolicy::Unlimited) {
        text_area_height
            .saturating_sub(line_h.saturating_mul(1 + margin_lines + last_line_misfit))
            .max(margin_px)
    } else {
        text_area_height / 2
    };
    let body_rows = &rows[first_body_idx..];
    let rel_point_idx = point_row_index - first_body_idx;
    let rel_recenter_idx = neovm_core::window::geometry::move_it_vertically_backward_row_index(
        body_rows,
        rel_point_idx,
        centering_position,
        text_area_height,
        line_h,
    );
    let recenter_idx = first_body_idx + rel_recenter_idx;
    let moved_px = point_row.y.saturating_sub(rows[recenter_idx].y);
    // GNU `xdisp.c:21232-21237`: if stepping backward pushed `point_row` at or
    // below `text_area_height` (`it.current_y <= 0`), start at `point_row`.
    if moved_px >= text_area_height {
        return row_candidate(point_row_index).map(TallPointRowScrollTarget::WindowStart);
    }
    if recenter_idx == first_body_idx && current_start > 0 {
        let remaining = centering_position.saturating_sub(moved_px);
        let threshold = text_area_height.min(line_h.saturating_mul(2) / 3);
        if remaining > threshold {
            let mut extra_lines = remaining / line_h;
            if remaining % line_h > threshold {
                extra_lines = extra_lines.saturating_add(1);
            }
            return Some(TallPointRowScrollTarget::LinesBeforeCurrentStart(
                extra_lines.max(1),
            ));
        }
    }

    row_candidate(recenter_idx).map(TallPointRowScrollTarget::WindowStart)
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
