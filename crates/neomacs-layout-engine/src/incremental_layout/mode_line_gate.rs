//! P3.5 E1: whether an incremental replay may keep the retained chrome
//! instead of evaluating the mode line (`NEOMACS_MODE_LINE_GATE`).
//!
//! The decision is Lisp-observable -- `(:eval ...)` forms in the mode line run
//! or do not -- so it must be GNU's decision, clause for clause, and not a
//! structural approximation of it.
//!
//! * `legacy` (the default) keeps the P5.2 rule: an edit replay keeps its
//!   chrome only when the walk regenerates exactly the cursor's row. That rule
//!   is stricter than GNU in two common cases: the box-topology lookbehind
//!   pulls the row ABOVE the edit into the walk whenever the damage starts at
//!   a line start (typing on an empty line, at the end of the buffer, or any
//!   line jit-lock marks from its start), and it never looks at WHY the rest
//!   of the line is unchanged. Measured with a counting `:eval` over 20
//!   typing cycles at the end of a buffer: GNU 20, neomacs 40.
//! * `gnu` ports GNU's optimization-1 guard (`redisplay_internal`,
//!   xdisp.c:17596-17720) as one [`ModeLineDecision`] per window: the dirty
//!   flags, the modified star, `consider_all_windows_p` from `bset_redisplay`,
//!   the recorded cursor line, `text_outside_line_unchanged_p` with GNU's
//!   one-char property shift, and -- after the walk -- the `display_line`
//!   result checks (the line is not continued, ends at the same distance from
//!   Z, and keeps its height).
//!
//! GNU positions are 1-based and the retained rows are 0-based; every
//! conversion is spelled out where it happens.

use neomacs_display_protocol::glyph_matrix::GlyphRow;

/// `NEOMACS_MODE_LINE_GATE`: which rule decides a replay's chrome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModeLineGate {
    /// The P5.2 structural rule (default).
    Legacy,
    /// GNU's optimization-1 guard, clause by clause.
    Gnu,
}

#[cfg(test)]
thread_local! {
    static GATE_OVERRIDE: std::cell::Cell<Option<ModeLineGate>> =
        const { std::cell::Cell::new(None) };
}

/// Force the gate on this thread (tests only); `None` restores the knob.
#[cfg(test)]
pub(crate) fn set_mode_line_gate_for_test(gate: Option<ModeLineGate>) {
    GATE_OVERRIDE.with(|cell| cell.set(gate));
}

/// The gate in effect. Read once per process; default `legacy`.
pub(crate) fn mode_line_gate() -> ModeLineGate {
    #[cfg(test)]
    if let Some(gate) = GATE_OVERRIDE.with(std::cell::Cell::get) {
        return gate;
    }
    static GATE: std::sync::OnceLock<ModeLineGate> = std::sync::OnceLock::new();
    *GATE.get_or_init(|| {
        match std::env::var("NEOMACS_MODE_LINE_GATE")
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref()
        {
            Some("gnu" | "on" | "1") => ModeLineGate::Gnu,
            _ => ModeLineGate::Legacy,
        }
    })
}

/// Why GNU would evaluate the mode line on this frame. One variant per GNU
/// clause, so a declined skip names the clause that declined it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModeLineEvaluateReason {
    /// `w->update_mode_line || update_mode_lines` (also `clip_changed` and
    /// `prevent_redisplay_optimizations_p`, which neomacs folds into the same
    /// dirty flags).
    ChromeDirty,
    /// `(SAVE_MODIFF < MODIFF) != w->last_had_star` (xdisp.c:17510).
    ModifiedStarFlipped,
    /// `%c`/`%C` displayed (`mode_line_update_needed`, xdisp.c:13844). GNU
    /// compares the value; neomacs still refuses on presence.
    ColumnDisplayed,
    /// `consider_all_windows_p`: a buffer changed that another window shows
    /// (`bset_redisplay` -> `redisplay_other_windows`, xdisp.c:886-898).
    OtherWindowsChanged,
    /// Optimization 1 is only ever tried for the selected window.
    NotSelectedWindow,
    /// `match_p`: the selected window does not show the current buffer.
    NotCurrentBuffer,
    /// `this_line_start_pos == 0`: the previous frame recorded no cursor line
    /// (no cursor row, or it was continued, a continuation, or scrolled).
    NoRecordedLine,
    /// `PT` outside `[tlbufpos, Z - tlendpos]`.
    PointLeftLine,
    /// `display-line-numbers` non-nil and not `visual`.
    LineNumbersDisplayed,
    /// `text_outside_line_unchanged_p`: a change starts before the line
    /// (property changes count one char early) or ends after it.
    TextOutsideLineChanged,
    /// `overlay_touches_p` at a line boundary the change reaches exactly.
    OverlayAtLineEdge,
    /// Bidi reordering with an automatic paragraph direction.
    BidiAutoParagraph,
    /// `selective-display` and a change at the line start.
    SelectiveDisplay,
    /// "Former continuation line has disappeared by becoming empty."
    ContinuationLineVanished,
    /// The visible region moved (`%p`); no one-line redisplay applies.
    WindowStartMoved,
    /// After the walk: the line wrapped, ends elsewhere, changed height, or
    /// lost the cursor (`display_line`'s result checks).
    LineChanged,
}

/// GNU's answer for one window's mode line on one frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ModeLineDecision {
    /// Optimization 1: the cursor line is redisplayed in place and
    /// `display_mode_lines` is never reached. Holds only if the walk then
    /// satisfies the carried [`OneLineContract`].
    KeepRetained(OneLineContract),
    /// GNU evaluates the mode line, for the named reason.
    Evaluate(ModeLineEvaluateReason),
}

/// What `display_line` must produce for GNU to keep optimization 1
/// (xdisp.c:17678-17690), in retained-row terms: the row at the recorded
/// vpos holds point, is not continued, keeps its start, ends at the same
/// distance from Z, and keeps its height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OneLineContract {
    /// Matrix row index of the recorded cursor line (`this_line_vpos`).
    pub(crate) row_index: usize,
    /// The line's start, unchanged by construction.
    pub(crate) start_charpos: usize,
    /// The line's end after the edit: the old end shifted by the size delta.
    pub(crate) end_charpos: usize,
    /// The line's old height.
    pub(crate) height_px: f32,
    /// Point after the edit (0-based).
    pub(crate) point: usize,
}

impl OneLineContract {
    /// Whether the freshly walked `row` still satisfies optimization 1.
    pub(crate) fn holds_for(&self, row: Option<&GlyphRow>) -> bool {
        let Some(row) = row else {
            return false;
        };
        row.enabled
            && !row.continued
            && !row.truncated_left
            && row.start_charpos == self.start_charpos
            && row.end_charpos == self.end_charpos
            && (row.height_px - self.height_px).abs() < 0.5
            && self.point >= row.start_charpos
            && self.point <= row.end_charpos
    }
}

/// Everything GNU's optimization-1 guard reads, gathered by the engine from
/// the retained matrix, the two keys, the buffer and the frame. Positions are
/// 0-based chars unless named otherwise.
#[derive(Clone, Copy, Debug)]
pub(crate) struct OneLineFacts {
    pub(crate) chrome_dirty: bool,
    pub(crate) modified_star_flipped: bool,
    pub(crate) column_displayed: bool,
    pub(crate) other_windows_changed: bool,
    pub(crate) selected_window: bool,
    pub(crate) shows_current_buffer: bool,
    pub(crate) window_start_moved: bool,
    pub(crate) line_numbers_displayed: bool,
    pub(crate) bidi_auto_paragraph: bool,
    pub(crate) selective_display: bool,
    /// Old and new buffer sizes (Z - BEG, i.e. `point-max` - 1).
    pub(crate) old_z: i64,
    pub(crate) new_z: i64,
    /// Point after the edit.
    pub(crate) point: i64,
    /// GNU `BEG_UNCHANGED` (property changes shifted one char early).
    pub(crate) beg_unchanged: i64,
    /// GNU `END_UNCHANGED`: chars at the end unchanged, in NEW coordinates.
    pub(crate) end_unchanged: i64,
}

/// The recorded cursor line (`this_line_*`) in retained-row terms.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RecordedLine<'a> {
    pub(crate) row_index: usize,
    pub(crate) row: &'a GlyphRow,
    /// The body row above it, if any: its `continued` flag is what makes
    /// the recorded row a continuation line.
    pub(crate) previous: Option<&'a GlyphRow>,
}

impl RecordedLine<'_> {
    /// GNU `MATRIX_ROW_END_CHARPOS` (exclusive, past the newline) for a row
    /// that ends a line. Retained rows record the end of their last glyph's
    /// span, which for a newline-terminated row is the newline itself.
    fn end_exclusive(&self) -> i64 {
        let end = self.row.end_charpos as i64;
        if self.row.ends_at_zv { end } else { end + 1 }
    }
}

/// Buffer queries the guard needs, answered by the engine (tests fake them).
pub(crate) trait OneLineBufferProbe {
    /// GNU `overlay_touches_p`: an overlay starts or ends exactly at `pos`.
    fn overlay_touches(&self, pos: i64) -> bool;
    /// The character at `pos`, if any.
    fn char_at(&self, pos: i64) -> Option<char>;
    /// `BEGV` and `ZV`, 0-based.
    fn accessible(&self) -> (i64, i64);
}

/// GNU's optimization-1 guard for an EDIT frame (xdisp.c:17596-17636, with
/// `text_outside_line_unchanged_p` at xdisp.c:16984-17038).
pub(crate) fn decide_after_edit(
    facts: OneLineFacts,
    line: Option<RecordedLine<'_>>,
    buffer: &dyn OneLineBufferProbe,
) -> ModeLineDecision {
    use ModeLineEvaluateReason as Why;
    let evaluate = ModeLineDecision::Evaluate;
    // The window-level clauses, in GNU's order.
    if facts.other_windows_changed {
        return evaluate(Why::OtherWindowsChanged);
    }
    if !facts.selected_window {
        return evaluate(Why::NotSelectedWindow);
    }
    if facts.chrome_dirty {
        return evaluate(Why::ChromeDirty);
    }
    if facts.modified_star_flipped {
        return evaluate(Why::ModifiedStarFlipped);
    }
    if facts.column_displayed {
        return evaluate(Why::ColumnDisplayed);
    }
    if !facts.shows_current_buffer {
        return evaluate(Why::NotCurrentBuffer);
    }
    if facts.window_start_moved {
        return evaluate(Why::WindowStartMoved);
    }
    // `CHARPOS (tlbufpos) > 0`: set_cursor_from_row recorded the line only
    // for a row that is neither continued nor a continuation line and starts
    // at x = 0 (xdisp.c:19206-19229).
    let Some(line) = line else {
        return evaluate(Why::NoRecordedLine);
    };
    if line.row.continued
        || line.row.truncated_left
        || line.previous.is_some_and(|previous| previous.continued)
    {
        return evaluate(Why::NoRecordedLine);
    }
    let delta = facts.new_z - facts.old_z;
    let line_start = line.row.start_charpos as i64;
    // tlendpos = Z - MATRIX_ROW_END_CHARPOS, recorded against the OLD Z.
    let tlendpos = facts.old_z - line.end_exclusive();
    // `PT >= CHARPOS (tlbufpos) && PT <= Z - CHARPOS (tlendpos)`, 1-based on
    // both sides: PT1 = point + 1, Z1 = new_z + 1, tlbufpos1 = line_start + 1.
    if facts.point < line_start || facts.point > facts.new_z - tlendpos {
        return evaluate(Why::PointLeftLine);
    }
    if facts.line_numbers_displayed {
        return evaluate(Why::LineNumbersDisplayed);
    }
    // text_outside_line_unchanged_p. `start` and `end` are GNU's arguments:
    // start = tlbufpos (1-based), end = tlendpos. BEG_UNCHANGED is a count,
    // so `BEG_UNCHANGED < start - 1` is `beg_unchanged < line_start`.
    if facts.beg_unchanged < line_start || facts.end_unchanged < tlendpos {
        return evaluate(Why::TextOutsideLineChanged);
    }
    if facts.selective_display && facts.beg_unchanged <= line_start {
        return evaluate(Why::SelectiveDisplay);
    }
    // `BEG + BEG_UNCHANGED == start && overlay_touches_p (start)`; `start` is
    // the 1-based line start, i.e. the 0-based position `line_start`.
    if facts.beg_unchanged == line_start && buffer.overlay_touches(line_start) {
        return evaluate(Why::OverlayAtLineEdge);
    }
    // `END_UNCHANGED == end && overlay_touches_p (Z - end)`: 1-based
    // `Z - end` is the 0-based position `new_z - tlendpos`.
    if facts.end_unchanged == tlendpos && buffer.overlay_touches(facts.new_z - tlendpos) {
        return evaluate(Why::OverlayAtLineEdge);
    }
    if facts.bidi_auto_paragraph {
        return evaluate(Why::BidiAutoParagraph);
    }
    // "Former continuation line has disappeared by becoming empty"
    // (xdisp.c:17631-17636).
    let (begv, zv) = buffer.accessible();
    if line_start > begv
        && buffer.char_at(line_start - 1) != Some('\n')
        && (line_start == zv || buffer.char_at(line_start) == Some('\n'))
    {
        return evaluate(Why::ContinuationLineVanished);
    }
    ModeLineDecision::KeepRetained(OneLineContract {
        row_index: line.row_index,
        start_charpos: line.row.start_charpos,
        end_charpos: (line.row.end_charpos as i64 + delta).max(0) as usize,
        height_px: line.row.height_px,
        point: facts.point.max(0) as usize,
    })
}

/// The engine-side facts an edit frame's decision needs that neither the
/// retained matrix nor the two keys carry.
#[derive(Clone, Copy, Debug)]
pub(crate) struct EditFrameFacts {
    pub(crate) chrome: super::ChromeReuseContext,
    pub(crate) selected: bool,
    pub(crate) shows_current_buffer: bool,
    pub(crate) other_windows_changed: bool,
    /// GNU `BEG_UNCHANGED` snapshotted before fontification.
    pub(crate) beg_unchanged: i64,
    /// End of the pre-fontification dirty span, in NEW coordinates.
    pub(crate) dirty_end: i64,
}

impl OneLineBufferProbe for neovm_core::buffer::Buffer {
    fn overlay_touches(&self, pos: i64) -> bool {
        if self.overlays().is_empty() || pos < 0 {
            return false;
        }
        let byte = |char_pos: i64| {
            self.char_pos_to_emacs_byte_pos_clamped(neovm_core::buffer::CharPos0::new(
                char_pos.max(0) as usize,
            ))
        };
        let at = byte(pos);
        // GNU searches `[pos - 1, pos + 1]` so that overlays ENDING at pos
        // and empty ones at pos are both found.
        let range = neovm_core::buffer::EmacsByteRange::ordered(byte(pos - 1), byte(pos + 1));
        self.overlays()
            .overlays_in_emacs_byte_range(range)
            .into_iter()
            .any(|overlay| {
                self.overlays().overlay_start_emacs_byte_pos(overlay) == Some(at)
                    || self.overlays().overlay_end_emacs_byte_pos(overlay) == Some(at)
            })
    }

    fn char_at(&self, pos: i64) -> Option<char> {
        if pos < 0 {
            return None;
        }
        let byte = self
            .char_pos_to_emacs_byte_pos_clamped(neovm_core::buffer::CharPos0::new(pos as usize));
        self.char_at_emacs_byte_pos(byte)
    }

    fn accessible(&self) -> (i64, i64) {
        (
            self.point_min_char_pos().get() as i64,
            self.point_max_char_pos().get() as i64,
        )
    }
}

/// Whether a buffer runs bidi reordering with an automatic paragraph
/// direction: `bidi-display-reordering` non-nil and `bidi-paragraph-direction`
/// nil (xdisp.c:17030-17035).
fn bidi_auto_paragraph(buffer: &neovm_core::buffer::Buffer) -> bool {
    let non_nil = |name: &str| buffer.buffer_local_value(name).is_some_and(|v| !v.is_nil());
    non_nil("bidi-display-reordering") && !non_nil("bidi-paragraph-direction")
}

/// GNU's decision for an EDIT replay's chrome. Gathers [`OneLineFacts`] from
/// the retained matrix, the keys, the replay, the frame and the buffer, then
/// runs [`decide_after_edit`].
pub(crate) fn decide_edit_chrome(
    prev: &super::RetainedWindowMatrix,
    curr: &super::RetainedWindowKey,
    replay: &super::ScrollReplay,
    frame: EditFrameFacts,
    buffer: &neovm_core::buffer::Buffer,
) -> ModeLineDecision {
    let new_z = buffer.total_char_len().get() as i64;
    let delta = curr.buffer_size - prev.key.buffer_size;
    let facts = OneLineFacts {
        chrome_dirty: frame.chrome.chrome_dirty,
        modified_star_flipped: prev.chrome_modified_flag != frame.chrome.buffer_modified,
        column_displayed: prev.chrome_uses_column,
        other_windows_changed: frame.other_windows_changed,
        selected_window: frame.selected,
        shows_current_buffer: frame.shows_current_buffer,
        window_start_moved: replay.dvpos != 0.0 || replay.new_window_start != prev.key.window_start,
        line_numbers_displayed: !matches!(
            curr.display_line_numbers,
            crate::types::DisplayLineNumbersMode::Off
                | crate::types::DisplayLineNumbersMode::Visual
        ),
        bidi_auto_paragraph: bidi_auto_paragraph(buffer),
        selective_display: curr.selective_display > 0,
        old_z: new_z - delta,
        new_z,
        point: curr.point,
        beg_unchanged: frame.beg_unchanged,
        end_unchanged: (new_z - frame.dirty_end).max(0),
    };
    let mut previous_body: Option<&GlyphRow> = None;
    let mut line = None;
    for (row_index, row) in prev.matrix.rows.iter().enumerate() {
        if !row.enabled || super::RetainedWindowMatrix::is_chrome_role(row.role) {
            continue;
        }
        if row.cursor_type.is_some() {
            line = Some(RecordedLine {
                row_index,
                row,
                previous: previous_body,
            });
            break;
        }
        previous_body = Some(row);
    }
    decide_after_edit(facts, line, buffer)
}

#[cfg(test)]
#[path = "tests/mode_line_gate_test.rs"]
mod tests;
