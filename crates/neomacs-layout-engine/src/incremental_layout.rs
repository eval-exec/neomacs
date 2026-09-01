//! Incremental-layout scaffolding (Phase 0a).
//!
//! This module holds the retained-matrix data types and the per-frame layout
//! instrumentation that the incremental-redisplay work is built on. In Phase
//! 0a NONE of the retained state is *read*: the engine still rebuilds every
//! window every cycle. What ships here is (a) the containers a later phase
//! reuses rows out of, written at the accepted `break` only, and (b) the
//! relaid-row-count metric that gates every phase — without it, a phase whose
//! golden matrices pass but which silently relays every row (i.e. regresses to
//! today's full-rebuild) is invisible.
//!
//! Spec: docs/superpowers/specs/2026-06-26-neomacs-incremental-layout-design.md
//! (§4.1 retained structure, §4.6 RowDamage, §5 Phase 0a, §7 go-criteria).

use crate::display_cursor::ResolvedCursorCoordinatePair;
use crate::frame_face_arena::FrameFaceGeneration;
use crate::types::{
    DisplayLineNumbersMode, LineWrapMode, PartialBodyWalkStart, PointMotionBodyDependency,
    WindowParams,
};
use crate::window_layout::{WindowLayoutBox, WindowPartitionSignature};
use neomacs_display_protocol::frame_glyphs::{CursorStyle, DisplaySlotId, PhysCursor};
pub use neomacs_display_protocol::glyph_matrix::RowDamage;
use neomacs_display_protocol::glyph_matrix::{
    GlyphArea, GlyphMatrix, GlyphPointerOccurrenceIdentity, GlyphPointerSourceKind, GlyphRow,
    MatrixRow,
};
use neomacs_display_protocol::types::FaceId;
#[cfg(test)]
use neomacs_display_protocol::types::Rect;
use neovm_core::buffer::position::LispCharPos1;
use neovm_core::window::{DisplayPointSnapshot, DisplayRowSnapshot};

/// How a window's layout was produced this cycle.
///
/// Phase 0a only ever produces [`LayoutClass::Full`]; the classifier that
/// yields the other variants arrives in Phases 1-3. Tracked per window so the
/// bench can assert, e.g., that an hl-line-on cursor move did NOT silently fall
/// back to `Full` (spec §5 Phase 1 gate).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LayoutClass {
    /// Full rebuild — every row relaid (today's only behavior).
    #[default]
    Full,
    /// Cursor moved; body reused verbatim, only cursor rows re-decorated (Phase 1).
    CursorOnly,
    /// Window scrolled by whole rows; rows shifted, newly-exposed laid (Phase 2).
    Scroll,
    /// Localized edit; only intersecting rows relaid (Phase 3).
    Edit,
}

/// Gate bit on a retained matrix.
///
/// A retained matrix may be reused only after a clean, fully-fontified,
/// non-probe pass set it [`MatrixValidity::Valid`]. Phase 0a sets it but never
/// reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MatrixValidity {
    /// Not safe to reuse (default, and after any escalation).
    #[default]
    Invalid,
    /// Produced by a clean, fully-fontified, accepted (non-probe) pass.
    Valid,
}

/// Snapshot of the layout inputs that, if changed, force a window's retained
/// matrix to be dropped (spec §4.2 window-level escalation list).
///
/// Phase 0a captures the geometry/window inputs that already exist on
/// [`WindowParams`]. Phase 0b/1 adds the neovm-core source-of-truth signals
/// (`chars`/`props`/`overlay` modified ticks + `face_change_count`). Any move
/// of these — or of geometry/window_start — escalates a window out of the
/// cursor-only fast path to a full rebuild ([`Self::cursor_only_eligible`]).
///
/// `PartialEq` is the reuse predicate: two keys are equal iff EVERY layout input
/// is identical. f32 fields compare bitwise-exactly — for an unchanged frame the
/// values are recomputed from identical inputs, so they are bit-identical; any
/// real geometry/metric change makes them differ and forces a full rebuild.
#[derive(Clone, Debug, PartialEq)]
pub struct RetainedWindowKey {
    /// Generation of asynchronously decoded media (see
    /// `Context::invalidate_media`). An image that finishes decoding changes
    /// none of the buffer ticks or geometry below, so without this term the
    /// window kept reusing the matrix that captured the image's 1x1 `Pending`
    /// placeholder and every async-decoded buffer image stayed one pixel.
    pub media_generation: u64,
    pub buffer_id: u64,
    pub window_start: i64,
    pub point: i64,
    /// Effective line-number semantics baked into the retained left margin.
    /// Unlike a generic display-variable generation, this typed value also
    /// declares how bare point motion invalidates body glyphs.
    pub display_line_numbers: DisplayLineNumbersMode,
    pub buffer_begv: i64,
    pub buffer_size: i64,
    pub(crate) partition: WindowPartitionSignature,
    pub hscroll: i32,
    pub vscroll: i32,
    pub wrap_mode: LineWrapMode,
    pub word_wrap: bool,
    pub tab_width: i32,
    pub char_width: f32,
    pub char_height: f32,
    pub font_pixel_size: f32,
    // --- Additional layout-affecting inputs (adversarial-review fixes). These
    // are read fresh from buffer-locals / window state each frame but do NOT
    // bump any of the four ticks below; a change must still force a full rebuild,
    // so they live in the key and the derived PartialEq compares them.
    /// Per-line tab stops (`tab-stop-list`); changes tab glyph widths/columns.
    pub tab_stop_list: Vec<i32>,
    /// `line-spacing` extra pixels per row; changes row height + window_end.
    pub extra_line_spacing: f32,
    /// `selective-display`; hides/shows lines past a column.
    pub selective_display: i32,
    /// Whether this is the frame's selected window; drives the solid-vs-hollow
    /// cursor, so a selection change must re-decorate (full rebuild).
    pub selected: bool,
    /// `show-trailing-whitespace` + the resolved background color.
    pub show_trailing_whitespace: bool,
    pub trailing_ws_bg: u32,
    /// `nobreak-char-display` + `glyphless-char-display` foreground; special-char
    /// rendering.
    pub nobreak_char_display: i32,
    pub glyphless_char_fg: u32,
    /// `indicate-empty-lines` controls empty-line fringe glyphs baked into
    /// reused rows. Physical fringe allocation lives in `partition`.
    pub indicate_empty_lines: i32,
    /// `line-prefix` / `wrap-prefix` (prepended glyphs on each line / wrap).
    pub line_prefix: Vec<u8>,
    pub wrap_prefix: Vec<u8>,
    /// Buffer multibyteness; char→byte mapping (window_end_byte) + char widths
    /// flip on `set-buffer-multibyte`.
    pub is_multibyte: bool,
    /// neovm-core per-buffer character-modification tick (spec §4.2 signal,
    /// chars). Any move means the buffer text changed → not cursor-only.
    pub chars_modified_tick: i64,
    /// Per-buffer text-property tick (face/display/invisible/composition; spec
    /// §4.2 (A)). Catches `put-text-property`, fontification, prettify-symbols.
    pub props_modified_tick: i64,
    /// Per-buffer overlay tick (spec §4.2 (B)). Catches hl-line, show-paren,
    /// region, flymake/lsp, iedit — any of which co-moves with the cursor.
    pub overlay_modified_tick: i64,
    /// Global face-subsystem change counter (spec §4.2 (C)). Catches
    /// `set-face-attribute` / theme load / face-remap that mutate pixels with
    /// no buffer tick.
    pub face_change_count: u64,
    /// Global display-variable change counter (adversarial-review fix). Bumped
    /// by `mark_redisplay_dirty_if_display_var` for the whole DISPLAY_AFFECTING
    /// set (truncate-lines, bidi-*, ctl-arrow, buffer-display-table /
    /// -invisibility-spec, fill-column-indicator, overlay-arrow,
    /// display-line-numbers, …) — none of which move a buffer/face tick.
    pub display_var_change_count: u64,
}

impl RetainedWindowKey {
    /// Snapshot the layout inputs from the resolved window params for this pass,
    /// reading the per-buffer invalidation ticks + global face counter from the
    /// evaluator. A missing buffer falls back to zero ticks (it will not match a
    /// real retained key, so it harmlessly forces a full rebuild).
    pub(crate) fn from_params(
        p: &WindowParams,
        layout_box: WindowLayoutBox,
        evaluator: &neovm_core::emacs_core::Context,
    ) -> Self {
        // Read buffer_size FRESH (same accessor as WindowParams, neovm_bridge.rs)
        // alongside the ticks, so the key is internally consistent: if Lisp edits
        // the buffer mid-render (after window params were captured), the fresh
        // chars tick + fresh buffer_size both reflect it and the edit/full path
        // sees a correct delta (adversarial-review Phase A staleness fix).
        let (
            chars_modified_tick,
            props_modified_tick,
            overlay_modified_tick,
            is_multibyte,
            buffer_size,
        ) = evaluator
            .buffer_manager()
            .get(neovm_core::buffer::BufferId(p.buffer_id))
            .map(|buffer| {
                (
                    buffer.chars_modified_tick(),
                    buffer.props_modified_tick(),
                    buffer.overlay_modified_tick(),
                    buffer.get_multibyte(),
                    buffer.point_max_char_pos().get() as i64,
                )
            })
            .unwrap_or((0, 0, 0, false, p.buffer_size));
        Self {
            media_generation: evaluator.media_generation(),
            buffer_id: p.buffer_id,
            window_start: p.window_start,
            point: p.point,
            display_line_numbers: p.display_line_numbers,
            buffer_begv: p.buffer_begv,
            buffer_size,
            partition: WindowPartitionSignature::from_layout_box(layout_box),
            hscroll: p.hscroll,
            vscroll: p.vscroll,
            wrap_mode: p.wrap_mode,
            word_wrap: p.word_wrap,
            tab_width: p.tab_width,
            char_width: p.char_width,
            char_height: p.char_height,
            font_pixel_size: p.font_pixel_size,
            tab_stop_list: p.tab_stop_list.clone(),
            extra_line_spacing: p.extra_line_spacing,
            selective_display: p.selective_display,
            selected: p.selected,
            show_trailing_whitespace: p.show_trailing_whitespace,
            trailing_ws_bg: p.trailing_ws_bg,
            nobreak_char_display: p.nobreak_char_display,
            glyphless_char_fg: p.glyphless_char_fg,
            indicate_empty_lines: p.indicate_empty_lines,
            line_prefix: p.line_prefix.clone(),
            wrap_prefix: p.wrap_prefix.clone(),
            is_multibyte,
            chars_modified_tick,
            props_modified_tick,
            overlay_modified_tick,
            face_change_count: evaluator.face_change_count,
            display_var_change_count: evaluator.display_var_change_count,
        }
    }

    /// Whether a window may take the cursor-only fast path this frame: every
    /// layout input is identical to the retained (`prev`) key EXCEPT possibly
    /// `point`. This covers BOTH a cursor move (point differs, re-decorate the
    /// cursor) AND a no-change re-layout (point equal — e.g. a non-edited window
    /// in a multi-window frame, which must reuse its body verbatim instead of
    /// full-rebuilding; re-decorating the cursor at the same point is a no-op).
    /// Any tick/geometry/window_start move escalates to full (the buffer text,
    /// properties, overlays, faces, or the viewport changed, so the retained body
    /// rows are no longer trustworthy).
    pub fn cursor_only_eligible(prev: &Self, curr: &Self) -> bool {
        let mut aligned = prev.clone();
        aligned.point = curr.point;
        aligned == *curr
    }

    /// Whether a window may take the pure-scroll fast path: every layout input is
    /// identical to the retained (`prev`) key EXCEPT `window_start` (which moved)
    /// and `point` (which may follow the scroll). Any tick/geometry/face move
    /// escalates to a full rebuild. Whether the scroll is by WHOLE rows is decided
    /// separately against the retained matrix ([`RetainedWindowMatrix::scroll_replay`]).
    pub fn scroll_eligible(prev: &Self, curr: &Self) -> bool {
        if prev.window_start == curr.window_start {
            return false;
        }
        let mut aligned = prev.clone();
        aligned.window_start = curr.window_start;
        aligned.point = curr.point;
        aligned == *curr
    }

    /// Whether a window may take the localized-edit fast path: the CHARS tick
    /// (a plain text edit) and/or the PROPS tick (a text-property write —
    /// font-lock re-fontifying the edited region) moved, while overlay/face
    /// ticks, window_start, and geometry are all unchanged. Property changes
    /// are covered because they feed the same unchanged-region accumulator as
    /// char edits (GNU BUF_COMPUTE_UNCHANGED parity, textprop.c), so the
    /// dirty span bounds BOTH kinds of damage; GNU's try_window_id likewise
    /// proceeds through property changes and hard-bails only on overlay
    /// modiff (xdisp.c GIVE_UP 200). An overlay/face move still escalates to
    /// a full rebuild. `point` may also move with the edit.
    pub fn edit_eligible(prev: &Self, curr: &Self) -> bool {
        if prev.chars_modified_tick == curr.chars_modified_tick
            && prev.props_modified_tick == curr.props_modified_tick
        {
            return false;
        }
        let mut aligned = prev.clone();
        aligned.chars_modified_tick = curr.chars_modified_tick;
        aligned.props_modified_tick = curr.props_modified_tick;
        aligned.point = curr.point;
        // A char edit necessarily changes the buffer size; that is the expected
        // consequence, not an escalation. `buffer_begv` is NOT aligned, so a
        // narrowing change (or an edit before BEGV) still escalates to Full.
        aligned.buffer_size = curr.buffer_size;
        aligned == *curr
    }
}

/// One window's retained layout, owned across cycles by `LayoutEngine`.
///
/// Committed at the accepted `break` only — never on a retry-loop `continue`
/// (mini/tab-bar resize) or a ≤1-row probe pass (the scroll-off hazard). Phase
/// 0a writes it; nothing reads it yet.
#[derive(Clone, Debug)]
pub struct RetainedWindowMatrix {
    /// The last clean pass's per-window matrix (the GNU "current matrix" analog).
    pub matrix: GlyphMatrix,
    /// Snapshot of every layout input for the reuse predicate.
    pub key: RetainedWindowKey,
    /// Reuse gate; only `Valid` matrices may be reused (Phases 1+).
    pub validity: MatrixValidity,
    /// The clean pass's window display snapshot (point-independent body row
    /// snapshots + per-span display points). The cursor-only fast path (Phase 1)
    /// replays its body half verbatim, re-decorating only the cursor; the
    /// position fields are unchanged because the visible region did not move.
    pub display_snapshot: neovm_core::window::WindowDisplaySnapshot,
    /// Exact renderer-facing cursor produced by the accepted full display walk.
    /// Kept separately from the integer window snapshot so unchanged cursor-only
    /// replay preserves subpixel geometry and explicit display-string placement.
    pub presented_cursor: Option<PhysCursor>,
    /// Sealed frame-face generation that owns every ID referenced by `matrix`.
    pub(crate) face_generation: FrameFaceGeneration,
    /// Whether the chrome this matrix carries displayed `%c` / `%C`. GNU's
    /// `w->column_number_displayed`; a column is the one point-dependent
    /// construct the same-row precondition does not pin, so chrome that shows
    /// one is never reused.
    pub(crate) chrome_uses_column: bool,
    /// The buffer's modified flag when this chrome was generated — GNU's
    /// `w->last_had_star`. GNU compares it in `redisplay_internal`
    /// (xdisp.c:17487-17488, `if ((SAVE_MODIFF < MODIFF) != w->last_had_star)
    /// w->update_mode_line = true;`) and the flip DISQUALIFIES the one-line
    /// optimization, which is how `%*` / `%+` stay honest across the first edit
    /// to a clean buffer. It is a comparison rather than a trigger because
    /// nothing at edit time knows the flag changed value.
    pub(crate) chrome_modified_flag: bool,
}

/// Everything the cursor-only fast path (Phase 1) needs to replay a window
/// without re-walking the buffer: the retained body rows to install verbatim,
/// the point-independent emitter body state to seed, and the old/new point so
/// the cursor can be cleared and re-decorated.
///
/// Built in the engine window loop from the previous frame's
/// [`RetainedWindowMatrix`] when [`RetainedWindowKey::cursor_only_eligible`]
/// holds and the cursor neighbourhood is structurally safe; consumed inside the
/// window render path in place of the body walk.
#[derive(Clone, Debug)]
pub struct CursorOnlyReplay {
    /// `(matrix_row_index, finalized GlyphRow)` for each retained NON-chrome
    /// row, installed verbatim (cursor decoration stripped, re-applied for the
    /// new point).
    pub body_rows: Vec<(usize, MatrixRow)>,
    /// Retained body `DisplayRowSnapshot`s (point-independent) to seed the
    /// emitter so `finish_and_install` rebuilds an identical window snapshot.
    pub body_row_snapshots: Vec<DisplayRowSnapshot>,
    /// Retained per-span display points (point-independent).
    pub points: Vec<DisplayPointSnapshot>,
    /// 0-based buffer position the cursor moved to this frame.
    pub new_point: i64,
    /// Matrix row index that now contains the new point (where the cursor is
    /// re-decorated).
    pub new_cursor_row_index: usize,
    /// Matrix row index that carried the cursor in the RETAINED pass — GNU's
    /// `this_line_vpos`. The chrome skip requires this to equal
    /// `new_cursor_row_index`; see [`RetainedWindowMatrix::retained_chrome`].
    pub retained_cursor_row_index: Option<usize>,
    /// The retained chrome to install verbatim instead of re-walking it, or
    /// `None` when the chrome must be regenerated. Filled by the engine, which
    /// owns the dirty flags; the builder always produces `None`.
    pub chrome: Option<RetainedChrome>,
    /// Cursor style carried over from the retained pass.
    pub cursor_style: CursorStyle,
    /// The authoritative cursor identities from the retained display, when
    /// point is unchanged. The renderer-facing placement and GNU live-window
    /// output coordinate travel as one value so replay cannot collapse them.
    /// This also preserves explicit display-string `cursor` placement;
    /// reconstructing from buffer point would lose that semantic override.
    pub(crate) retained_cursor: Option<RetainedTextWindowCursor>,
    /// Sealed frame-face generation that owns every ID in `body_rows`.
    pub(crate) face_generation: FrameFaceGeneration,
}

/// A retained cursor's renderer placement paired with its live-window output
/// coordinate. Neither half is meaningful as a cursor replay without the
/// other, so the incremental boundary never exposes two independent options.
#[derive(Clone, Debug)]
pub(crate) struct RetainedTextWindowCursor {
    presented: PhysCursor,
    coordinates: ResolvedCursorCoordinatePair,
    output_grid_x: i64,
}

impl RetainedTextWindowCursor {
    fn new(
        presented: PhysCursor,
        output_slot_id: DisplaySlotId,
        output_grid_x: i64,
    ) -> Option<Self> {
        let coordinates =
            ResolvedCursorCoordinatePair::from_slots(output_slot_id, presented.slot_id)?;
        Some(Self {
            presented,
            coordinates,
            output_grid_x,
        })
    }

    pub(crate) fn presented(&self) -> &PhysCursor {
        &self.presented
    }

    pub(crate) const fn coordinates(&self) -> ResolvedCursorCoordinatePair {
        self.coordinates
    }

    pub(crate) const fn output_grid_x(&self) -> i64 {
        self.output_grid_x
    }
}

/// Reuse plan for the pure-scroll fast path (Phase 2): the overlapping retained
/// body rows shifted to their new positions, plus the span of newly-exposed rows
/// that still must be laid out. Built when the window scrolled by whole rows with
/// no text/appearance change.
#[derive(Clone, Debug)]
pub struct ScrollReplay {
    /// Uniform vertical shift applied to every reused row's `pixel_y` (negative =
    /// scrolled up / content moved up). The removed top rows' total height.
    pub dvpos: f32,
    /// `(new_matrix_row_index, shifted GlyphRow)` for each reused body row, with
    /// `pixel_y` already shifted by `dvpos` and cursor decoration stripped.
    pub reused_rows: Vec<(usize, MatrixRow)>,
    /// The reused rows' display-snapshot rows, re-indexed (`row -= s`) and
    /// y-shifted (`y += round(dvpos)`), to seed the emitter so `finish` rebuilds
    /// an identical window snapshot.
    pub reused_row_snapshots: Vec<DisplayRowSnapshot>,
    /// The reused rows' display points, re-indexed + y-shifted likewise.
    pub reused_points: Vec<DisplayPointSnapshot>,
    /// First buffer position regenerated by the partial body walk.  This is
    /// intentionally distinct from semantic point and window-start.
    pub walk_start: PartialBodyWalkStart,
    /// Matrix row index of the first newly-exposed row.
    pub exposed_row_base: usize,
    /// Number of newly-exposed rows to lay out (= the whole-row scroll distance).
    pub exposed_row_count: usize,
    /// Window-relative y where the first newly-exposed row begins.
    pub exposed_text_y: f32,
    /// The real new window_start (where the reused region begins) — the partial
    /// walk reads from [`Self::walk_start`], but the published redisplay
    /// positions + mode-line must use this.
    pub new_window_start: i64,
    /// 0-based point for this frame (the cursor is re-decorated as in Phase 1,
    /// since a scroll usually accompanies a point move).
    pub new_point: i64,
    /// Cursor style carried over from the retained pass.
    pub cursor_style: CursorStyle,
    /// Phase 3 below-reuse: when true, the partial walk is BOUNDED to
    /// `exposed_row_count` rows (the edited line only) — the rows below the edit
    /// are reused (charpos-shifted, same pixel_y) and are already included in
    /// `reused_rows`. When false (scroll, above-only edit) the walk runs to the
    /// window bottom as usual.
    pub bound_walk: bool,
    /// Post-walk validation contract for `bound_walk` plans (GNU try_window_id
    /// analog: the regenerated region must sync back up with the reused rows).
    /// `None` for unbounded walks. The render compares the walked span against
    /// these NEW-coordinate expectations and bails the replay (relaying the
    /// window without it) on any mismatch — the runtime backstop for whatever
    /// the prove-ahead gates could not see (a property change re-wrapping or
    /// re-measuring a span line).
    pub expected_walk: Option<ExpectedBoundWalk>,
    /// The retained chrome to install verbatim instead of re-walking it, or
    /// `None` when the chrome must be regenerated. Filled by the engine, which
    /// owns the dirty flags; the builders always produce `None`.
    pub chrome: Option<RetainedChrome>,
    /// Sealed frame-face generation that owns every ID in `reused_rows`.
    pub(crate) face_generation: FrameFaceGeneration,
}

/// The current-frame facts a chrome reuse decision compares the retained
/// chrome against. Bundled so a caller cannot supply one and forget the other.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ChromeReuseContext {
    /// GNU `w->update_mode_line || update_mode_lines` for this window.
    pub(crate) chrome_dirty: bool,
    /// The buffer's modified flag NOW, compared against the flag the retained
    /// chrome was generated with (GNU's `w->last_had_star`).
    pub(crate) buffer_modified: bool,
}

/// One window's retained chrome (mode / header / tab line), ready to install
/// verbatim in place of a chrome walk.
///
/// This is the payload of GNU's one-line optimization as it applies to chrome:
/// GNU does not *reuse* a mode line, it simply never regenerates one, because
/// `redisplay_window` — and so `display_mode_lines` — is never entered
/// (xdisp.c:17572-17726). Neomacs re-emits every row into a fresh frame each
/// redisplay, so "never regenerated" has to be spelled as "re-installed from
/// the retained matrix", which is the same output by construction: these are
/// the exact rows the previous accepted frame published.
///
/// All three pieces are needed because the chrome walk produces all three, and
/// a skip that dropped any of them would be a silent divergence rather than an
/// optimization:
///   * `rows` — the glyphs;
///   * `row_snapshots` — what the emitter would have pushed, which is what
///     populates `WindowDisplaySnapshot.rows`;
///   * `metrics` — the MEASURED heights, which is what `window-mode-line-height`
///     reports (never the face-only estimate the text area was reserved from).
#[derive(Clone, Debug)]
pub struct RetainedChrome {
    pub rows: Vec<(usize, MatrixRow)>,
    pub row_snapshots: Vec<DisplayRowSnapshot>,
    pub chrome_strings: Vec<neovm_core::window::PresentedWindowChromeString>,
    pub(crate) metrics: crate::window_layout::WindowChromeMetrics,
}

/// One window's accumulated edit damage for a frame: the dirty char span in
/// POST-EDIT (current buffer) coordinates plus the net size delta since the
/// retained frame was committed.
///
/// The retained matrix's rows carry PRE-EDIT positions, so replay building
/// constantly needs both coordinate systems. Every conversion lives here as a
/// method — callers never do the `end - delta` arithmetic ad hoc (the exact
/// arithmetic a raw `(i64, i64)` span made easy to get wrong).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditDamage {
    /// First damaged char position. The unchanged PREFIX is shared by both
    /// coordinate systems, so this needs no conversion.
    span_start: i64,
    /// One past the last damaged char, in POST-EDIT coordinates.
    span_end_new: i64,
    /// Net buffer-size change (chars) since the retained frame.
    delta: i64,
    /// Newlines inside the post-edit span (the line-structure invariant's
    /// NEW-side count).
    span_newlines: usize,
}

impl EditDamage {
    pub fn new(span_start: i64, span_end_new: i64, delta: i64, span_newlines: usize) -> Self {
        Self {
            span_start,
            span_end_new,
            delta,
            span_newlines,
        }
    }

    /// First damaged position (valid in both coordinate systems).
    pub fn start(&self) -> i64 {
        self.span_start
    }

    /// Span end in POST-EDIT coordinates.
    pub fn end_new(&self) -> i64 {
        self.span_end_new
    }

    /// Span end in PRE-EDIT (retained matrix) coordinates: the unchanged
    /// SUFFIX is what the accumulator preserves, so the old end sits `delta`
    /// before the new one.
    pub fn end_old(&self) -> i64 {
        self.span_end_new - self.delta
    }

    pub fn delta(&self) -> i64 {
        self.delta
    }

    pub fn span_newlines(&self) -> usize {
        self.span_newlines
    }
}

/// What a bounded edit-replay walk must produce for the reused-below rows to
/// remain valid. All values are in post-edit (NEW) coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExpectedBoundWalk {
    /// The last walked row must end exactly here (old span-end charpos shifted
    /// by the edit delta) — position continuity with the first reused-below row.
    pub last_row_end_charpos: usize,
    /// Total pixel height the walked span must occupy (the retained span rows'
    /// height sum) — the reused-below rows keep their `pixel_y` only if the
    /// span's height is unchanged.
    pub total_height_px: f32,
    /// No walked row may be continued (a wrap changes the row structure).
    pub row_count: usize,
}

fn referenced_face_ids<'a>(rows: impl IntoIterator<Item = &'a GlyphRow>) -> Vec<FaceId> {
    rows.into_iter()
        .flat_map(GlyphRow::referenced_face_ids)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// The face IDs a reused chrome plan references, so Phase A admits them with
/// the body's. Re-installing a chrome row moves its GLYPHS but not the face
/// publication the chrome walk would have done
/// (`install_measured_window_display_row` -> `install_faces`), so a skip that
/// forgot this would leave the mode line's face IDs dangling in the frame's
/// face table — glyphs correct, colors arbitrary.
fn chrome_face_ids(chrome: Option<&RetainedChrome>) -> impl Iterator<Item = &GlyphRow> {
    chrome
        .into_iter()
        .flat_map(|chrome| chrome.rows.iter().map(|(_, row)| row.as_ref()))
}

impl CursorOnlyReplay {
    pub(crate) fn retained_face_ids(&self) -> Vec<FaceId> {
        referenced_face_ids(
            self.body_rows
                .iter()
                .map(|(_, row)| row.as_ref())
                .chain(chrome_face_ids(self.chrome.as_ref())),
        )
    }
}

impl ScrollReplay {
    pub(crate) fn retained_face_ids(&self) -> Vec<FaceId> {
        referenced_face_ids(
            self.reused_rows
                .iter()
                .map(|(_, row)| row.as_ref())
                .chain(chrome_face_ids(self.chrome.as_ref())),
        )
    }
}

impl RetainedWindowMatrix {
    /// True for chrome rows (mode/header/tab line, tab bar) — re-walked every
    /// frame, never reused. NOTE the discriminator is the row ROLE, not the
    /// `GlyphRow::mode_line` flag (which is set only for `ModeLine`, so header /
    /// tab lines have `mode_line == false`).
    pub fn is_chrome_role(role: neomacs_display_protocol::frame_glyphs::GlyphRowRole) -> bool {
        use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
        matches!(
            role,
            GlyphRowRole::ModeLine
                | GlyphRowRole::HeaderLine
                | GlyphRowRole::TabLine
                | GlyphRowRole::TabBar
        )
    }

    /// Harvest this window's retained chrome for verbatim re-install, or
    /// `None` when there is none to reuse.
    ///
    /// Callers must have already established that the chrome is ALLOWED to be
    /// reused — this only gathers it. The permission half lives in
    /// [`Self::chrome_reusable_after_cursor_move`] and
    /// [`Self::chrome_reusable_after_edit`].
    pub(crate) fn retained_chrome(&self) -> Option<RetainedChrome> {
        let mut rows: Vec<(usize, MatrixRow)> = Vec::new();
        let mut indices: rustc_hash::FxHashSet<usize> = rustc_hash::FxHashSet::default();
        for (idx, row) in self.matrix.rows.iter().enumerate() {
            if row.enabled && Self::is_chrome_role(row.role) {
                indices.insert(idx);
                // Copy-on-write reuse: a refcount bump, not a deep copy.
                rows.push((idx, MatrixRow::clone(row)));
            }
        }
        if rows.is_empty() {
            return None;
        }
        let row_snapshots = self
            .display_snapshot
            .rows
            .iter()
            .filter(|snapshot| indices.contains(&(snapshot.row as usize)))
            .cloned()
            .collect();
        Some(RetainedChrome {
            rows,
            row_snapshots,
            chrome_strings: self.display_snapshot.chrome_strings.clone(),
            metrics: crate::window_layout::WindowChromeMetrics::from_snapshot(
                &self.display_snapshot,
            ),
        })
    }

    /// Whether a CURSOR-ONLY replay of this window may skip its chrome walk.
    ///
    /// This is GNU's one-line optimization guard (`xdisp.c:17572-17610`)
    /// restated in this engine's terms. The pieces GNU spells and we inherit
    /// from fast-path eligibility (same buffer, no face change, no property or
    /// overlay movement, unchanged geometry) are already in
    /// [`RetainedWindowKey`]; what is left is the part GNU gets structurally
    /// and we must state:
    ///
    /// * **The dirty flags.** `!w->update_mode_line && !update_mode_lines`,
    ///   xdisp.c:17577 — the caller supplies this from [`ChromeDirty`].
    /// * **Point stays on the recorded line.** GNU enters the optimization only
    ///   while `PT >= CHARPOS (tlbufpos) && PT <= Z - CHARPOS (tlendpos)`
    ///   (xdisp.c:17591-17593); once point leaves that line control falls to
    ///   `cancel:`, whose comment is "Text changed drastically or point moved
    ///   off of line" (xdisp.c:17813), and a full `redisplay_window` follows.
    ///   THAT restriction is the entire reason `%l` cannot go stale while GNU
    ///   skips, and our cursor-only eligibility does NOT imply it — the key
    ///   compares every field except `point`, so point may move to any retained
    ///   row. The screen row is the exact analogue of `this_line_vpos`, and
    ///   requiring it to be unchanged is both free here and stricter than "same
    ///   buffer line" (a continued line spans several rows).
    /// * **No column displayed.** See `chrome_uses_column`.
    pub(crate) fn chrome_reusable_after_cursor_move(
        &self,
        replay: &CursorOnlyReplay,
        ctx: ChromeReuseContext,
    ) -> bool {
        self.chrome_reuse_baseline(ctx)
            && replay.retained_cursor_row_index == Some(replay.new_cursor_row_index)
    }

    /// The clauses every chrome reuse needs, whatever moved: the dirty flags,
    /// the displayed-column refusal, and GNU's modified-star comparison.
    fn chrome_reuse_baseline(&self, ctx: ChromeReuseContext) -> bool {
        !ctx.chrome_dirty
            && !self.chrome_uses_column
            && self.chrome_modified_flag == ctx.buffer_modified
    }

    /// Whether an EDIT replay of this window may skip its chrome walk.
    ///
    /// Same guard as [`Self::chrome_reusable_after_cursor_move`], with GNU's
    /// `text_outside_line_unchanged_p` (xdisp.c:17604) supplying the extra
    /// clause an edit needs. Expressed here as: the walk regenerates exactly
    /// one row, that row is the one that carried the cursor, the edit adds or
    /// removes no newline, and point lands back inside it. Those together mean
    /// no line boundary above point moved, so `%l` is unchanged — which is what
    /// GNU's one-line restriction buys structurally.
    ///
    /// A GENUINE SCROLL IS EXCLUDED HERE, and deliberately not by trusting a
    /// trigger to have fired: `%p` is computed from window-start and window-end
    /// (`xdisp.rs`, GNU xdisp.c:29406), so a replay whose visible region moved
    /// must regenerate chrome no matter what the flags say. Requiring
    /// `dvpos == 0` and an unchanged window-start states that in terms of what
    /// `%p` actually reads, rather than relying on the internal scroll path
    /// going through the `set-window-start` builtin that (a) wired.
    pub(crate) fn chrome_reusable_after_edit(
        &self,
        replay: &ScrollReplay,
        damage: EditDamage,
        ctx: ChromeReuseContext,
        region_contains_newline: impl Fn(i64, i64) -> bool,
    ) -> bool {
        if !self.chrome_reuse_baseline(ctx) {
            return false;
        }
        // The visible region must not have moved, because `%p` is computed
        // from window-start and window-end.
        //
        // DEFENSIVE, and deliberately kept as such: every edit replay already
        // satisfies this (`edit_replay` builds with `dvpos: 0.0` and the
        // eligibility check pins window-start), so a mutation of this clause
        // alone reds no pin. The scroll path's refusal is structural — it never
        // asks for chrome reuse at all — and THAT is what
        // `p52_scroll_re_evaluates_the_mode_line` pins, verified by mutating
        // `build_scroll_replay` to attach chrome. This clause states the
        // invariant at the point that depends on it, so a future replay shape
        // with a nonzero dvpos cannot silently inherit chrome reuse.
        if replay.dvpos != 0.0 || replay.new_window_start != self.key.window_start {
            return false;
        }
        // (The line-structure test is below, once the cursor row is known.)
        // That row must be the one holding the cursor, and point must still be
        // inside it after the edit. Row bounds are PRE-edit, so the end moves
        // by the edit's delta; `+ 1` admits point resting just past the last
        // character, which is where an append leaves it.
        let Some((_, cursor_row)) = self
            .matrix
            .rows
            .iter()
            .enumerate()
            .find(|(idx, row)| *idx == replay.exposed_row_base && row.cursor_type.is_some())
        else {
            return false;
        };
        let start = cursor_row.start_charpos as i64;
        let end = cursor_row.end_charpos as i64 + damage.delta();
        // Point must still be on the line the retained chrome described, which
        // is what keeps `%l` honest. Rows ABOVE the cursor row are reused
        // verbatim and the damage starts at or after this row's start, so the
        // newline count before this row cannot have moved; all that remains is
        // whether a newline now sits between the row's start and point.
        //
        // This is a bounded scan (a row is at most a window's width), and it
        // is the test that `span_newlines` could not be. That counter reports
        // newlines PRESENT in the damaged span, and jit-lock's span covers the
        // whole line including its terminator, so it is >= 1 on every ordinary
        // keystroke — using it refused 199 of 199 real edits while reading as
        // a faithful port of `text_outside_line_unchanged_p`.
        let ok = damage.start() >= start
            && damage.end_old() <= cursor_row.end_charpos as i64 + 1
            && replay.new_point >= start
            && replay.new_point <= end + 1
            && !region_contains_newline(start, replay.new_point);
        ok
    }

    /// Build a [`CursorOnlyReplay`] for this window if it can be reused this
    /// frame with only the cursor re-decorated, else `None` (→ full rebuild).
    ///
    /// Bails when the matrix is invalid, the reuse predicate fails
    /// ([`RetainedWindowKey::cursor_only_eligible`]), the new point lands outside
    /// the retained body rows, or the new cursor row is structurally unsafe
    /// (continuation / left-truncation / overlay-arrow fringe — the column
    /// resolve cannot place the cursor on those correctly).
    pub fn cursor_only_replay(&self, curr: &RetainedWindowKey) -> Option<CursorOnlyReplay> {
        if self.validity != MatrixValidity::Valid {
            return None;
        }
        if !RetainedWindowKey::cursor_only_eligible(&self.key, curr) {
            return None;
        }
        let point_moved = self.key.point != curr.point;
        let point_dependency = curr.display_line_numbers.point_motion_body_dependency();
        if point_moved && point_dependency == PointMotionBodyDependency::EntireWindow {
            // GNU xdisp.c refuses cursor-only redisplay for relative and visual
            // line numbers: every gutter value is derived from point, so the
            // retained body as a whole is stale.
            return None;
        }
        let new_point = curr.point;
        let mut body_rows: Vec<(usize, MatrixRow)> = Vec::new();
        let mut body_indices: rustc_hash::FxHashSet<usize> = rustc_hash::FxHashSet::default();
        let mut cursor_style: Option<CursorStyle> = None;
        let mut retained_cursor_row_index: Option<usize> = None;
        let mut new_cursor: Option<(usize, &GlyphRow)> = None;
        for (idx, row) in self.matrix.rows.iter().enumerate() {
            if !row.enabled || Self::is_chrome_role(row.role) {
                continue;
            }
            if row.cursor_type.is_some() {
                cursor_style = row.cursor_type;
                retained_cursor_row_index = Some(idx);
            }
            let start = row.start_charpos as i64;
            let end = row.end_charpos as i64;
            if new_cursor.is_none() && start <= new_point && new_point <= end {
                new_cursor = Some((idx, row.as_ref()));
            }
            body_indices.insert(idx);
            // Copy-on-write reuse: a refcount bump, not a per-glyph deep copy.
            body_rows.push((idx, MatrixRow::clone(row)));
        }
        if body_rows.is_empty() {
            return None;
        }
        let (new_cursor_row_index, cursor_row) = new_cursor?;
        if point_moved
            && point_dependency == PointMotionBodyDependency::CurrentDisplayRow
            && retained_cursor_row_index != Some(new_cursor_row_index)
        {
            // Absolute line numbers bake the current-line face into the left
            // margin. Moving to another display row changes body decoration
            // even though the underlying number stays absolute.
            return None;
        }
        if cursor_row.continued
            || cursor_row.truncated_left
            || cursor_row.left_fringe_bitmap.is_some()
        {
            return None;
        }
        // Scroll-safety (GNU `try_cursor_movement` / `make_cursor_line_fully_visible`):
        // a point move onto the top or bottom visible row can trigger a window
        // scroll to keep point visible, which the cursor-only path does NOT
        // perform (it never re-derives window_start). Bail on a boundary row
        // unless the window is already pinned to that buffer edge:
        //   * bottom row with more buffer below (`!ends_at_zv`) → may scroll down,
        //   * top row while the window is scrolled off the buffer start → may scroll up.
        let first_body_index = body_rows.first().map(|(idx, _)| *idx);
        let last_body_index = body_rows.last().map(|(idx, _)| *idx);
        let window_at_buffer_top = curr.window_start <= curr.buffer_begv + 1;
        if Some(new_cursor_row_index) == last_body_index && !cursor_row.ends_at_zv {
            return None;
        }
        if Some(new_cursor_row_index) == first_body_index && !window_at_buffer_top {
            return None;
        }
        let body_row_snapshots = self
            .display_snapshot
            .rows
            .iter()
            .filter(|snapshot| body_indices.contains(&(snapshot.row as usize)))
            .cloned()
            .collect();
        Some(CursorOnlyReplay {
            body_rows,
            body_row_snapshots,
            points: self.display_snapshot.points.clone(),
            new_point,
            new_cursor_row_index,
            retained_cursor_row_index,
            // Chrome is decided separately, by the engine, because the decision
            // needs the chrome dirty flags off the evaluator. `None` = walk.
            chrome: None,
            cursor_style: cursor_style.unwrap_or(CursorStyle::FilledBox),
            retained_cursor: (self.key.point == curr.point)
                .then(|| {
                    self.presented_cursor
                        .clone()
                        .zip(self.display_snapshot.phys_cursor.clone())
                        .and_then(|(presented, output)| {
                            let output_slot_id = DisplaySlotId {
                                window_id: presented.window_id,
                                row: u32::try_from(output.row).ok()?,
                                col: u16::try_from(output.col).ok()?,
                            };
                            RetainedTextWindowCursor::new(presented, output_slot_id, output.x)
                        })
                })
                .flatten(),
            face_generation: self.face_generation,
        })
    }

    /// Build a [`ScrollReplay`] if this window scrolled by WHOLE rows with no
    /// text/appearance change, else `None` (→ full rebuild). Handles forward
    /// (downward) scroll — the new `window_start` lands on a retained row
    /// boundary `s` rows down; rows `[s..]` are reused (shifted up by `dvpos`)
    /// and `s` newly-exposed rows remain to be laid at the bottom. Scroll-up
    /// (new start above the retained top), partial-row scroll, vscroll, line
    /// numbers, and continuation/truncation rows all bail (conservative).
    pub fn scroll_replay(&self, curr: &RetainedWindowKey) -> Option<ScrollReplay> {
        if self.validity != MatrixValidity::Valid {
            return None;
        }
        if !RetainedWindowKey::scroll_eligible(&self.key, curr) || curr.vscroll != 0 {
            return None;
        }
        // Collect body rows in matrix order; bail on anything that the uniform
        // shift cannot reproduce (line numbers renumber; continuation/truncation
        // /fringe rows have position-dependent decoration).
        let mut body: Vec<(usize, &MatrixRow)> = Vec::new();
        for (idx, row) in self.matrix.rows.iter().enumerate() {
            if !row.enabled || Self::is_chrome_role(row.role) {
                continue;
            }
            if !row.glyphs[GlyphArea::LeftMargin.index()].is_empty()
                || row.continued
                || row.truncated_left
            {
                return None;
            }
            // Fringe bitmaps on TEXT rows are buffer-dependent decorations the
            // replay cannot re-derive. The indicate-empty-lines fillers past
            // EOB (!displays_text, left OR right side) are position-independent
            // and carry real ZV bounds, so they reuse like the placeholder.
            if (row.left_fringe_bitmap.is_some() || row.right_fringe_bitmap.is_some())
                && row.displays_text
            {
                return None;
            }
            body.push((idx, row));
        }
        if body.len() < 2 {
            return None;
        }
        // Whole-row scroll distance: the body row whose start matches the new
        // window_start. `None` (no match) → partial-row scroll or scroll-up → bail.
        // Match only rows that DISPLAY buffer text: an empty line or the EOB
        // placeholder now also carries a real buffer position, but window_start
        // is always the position of a line that shows text, so gating on
        // `displays_text` keeps a positionless-but-real empty row from being
        // taken as the new top (GNU only starts a window at a text row).
        let s = body.iter().position(|(_, row)| {
            row.displays_text && row.start_charpos as i64 == curr.window_start
        })?;
        if s == 0 {
            return None;
        }
        let cursor_style = body
            .iter()
            .find_map(|(_, row)| row.cursor_type)
            .unwrap_or(CursorStyle::FilledBox);
        let last = body.len() - 1;
        let dvpos = body[0].1.pixel_y - body[s].1.pixel_y;
        let dvpos_i64 = dvpos.round() as i64;
        // old matrix row index → new matrix row index for each reused row.
        let mut remap: rustc_hash::FxHashMap<i64, i64> = rustc_hash::FxHashMap::default();
        let mut reused_rows = Vec::with_capacity(last - s + 1);
        for p in s..=last {
            // A shift mutates placement (pixel_y), so this reuse is a real
            // copy; verbatim reuse elsewhere is a refcount bump.
            let mut shifted = GlyphRow::clone(body[p].1);
            shifted.pixel_y += dvpos;
            shifted.cursor_col = None;
            shifted.cursor_type = None;
            remap.insert(body[p].0 as i64, body[p - s].0 as i64);
            reused_rows.push((body[p - s].0, MatrixRow::new(shifted)));
        }
        let reused_row_snapshots = self
            .display_snapshot
            .rows
            .iter()
            .filter_map(|row| {
                remap.get(&row.row).map(|&new_row| {
                    let mut row = row.clone();
                    row.row = new_row;
                    row.y += dvpos_i64;
                    row
                })
            })
            .collect();
        let reused_points = self
            .display_snapshot
            .points
            .iter()
            .filter_map(|point| {
                remap.get(&point.row).map(|&new_row| {
                    let mut point = point.clone();
                    point.row = new_row;
                    point.y += dvpos_i64;
                    point
                })
            })
            .collect();
        let last_row = body[last].1;
        Some(ScrollReplay {
            dvpos,
            reused_rows,
            reused_row_snapshots,
            reused_points,
            walk_start: PartialBodyWalkStart::new(last_row.end_charpos as i64 + 1),
            exposed_row_base: body[last - s + 1].0,
            exposed_row_count: s,
            exposed_text_y: last_row.pixel_y + dvpos + last_row.height_px,
            new_window_start: curr.window_start,
            new_point: curr.point,
            cursor_style,
            bound_walk: false,
            expected_walk: None,
            chrome: None,
            face_generation: self.face_generation,
        })
    }

    /// Build an edit replay (Phase 3 `try_window_id` analog) reusing the rows
    /// ABOVE the dirty span verbatim and partial-walking the dirty line + every
    /// row below it. `dirty_start` is the buffer's accumulated `changed_char_range`
    /// start. Reuses the [`ScrollReplay`] shape with `dvpos = 0` (no scroll, so
    /// the reused rows keep their position). Returns `None` (→ full rebuild) when
    /// the edit touches the very top visible row (nothing above to reuse) or the
    /// reuse predicate fails. The reused rows' charpos is valid because the edit
    /// is strictly below them.
    pub fn edit_replay(
        &self,
        curr: &RetainedWindowKey,
        damage: EditDamage,
        allow_below_reuse: bool,
    ) -> Option<ScrollReplay> {
        let dirty_start = damage.start();
        let dirty_end_old = damage.end_old();
        let span_newlines = damage.span_newlines();
        if self.validity != MatrixValidity::Valid {
            return None;
        }
        if !RetainedWindowKey::edit_eligible(&self.key, curr) || curr.vscroll != 0 {
            return None;
        }
        let mut body: Vec<(usize, &MatrixRow)> = Vec::new();
        for (idx, row) in self.matrix.rows.iter().enumerate() {
            if !row.enabled || Self::is_chrome_role(row.role) {
                continue;
            }
            if !row.glyphs[GlyphArea::LeftMargin.index()].is_empty()
                || row.continued
                || row.truncated_left
            {
                return None;
            }
            // Fringe bitmaps on TEXT rows are buffer-dependent decorations the
            // replay cannot re-derive. The indicate-empty-lines fillers past
            // EOB (!displays_text, left OR right side) are position-independent
            // and carry real ZV bounds, so they reuse like the placeholder.
            if (row.left_fringe_bitmap.is_some() || row.right_fringe_bitmap.is_some())
                && row.displays_text
            {
                return None;
            }
            body.push((idx, row));
        }
        if body.is_empty() {
            return None;
        }
        // Box-run end ownership on a row's final glyph depends on the first
        // following source face (GNU `end_of_box_run_p`).  Therefore damage at
        // N has a one-character backwards dependency even when N begins the
        // next visual row.  Widening here keeps the replay proof local and
        // prevents a verbatim predecessor row from retaining stale Right/open
        // ownership after a face-property edit.
        let topology_dirty_start = dirty_start.saturating_sub(1);
        let damage_first_by_charpos = body
            .iter()
            .position(|(_, row)| row.end_charpos as i64 >= dirty_start)?;
        // First dirty row = first body row whose OLD extent reaches the widened
        // source dependency. Rows above it have unchanged positions and box
        // terminals.
        let first_dirty_by_charpos = body
            .iter()
            .position(|(_, row)| row.end_charpos as i64 >= topology_dirty_start)?;
        if first_dirty_by_charpos == 0 {
            return None;
        }
        // A row's CHARPOS is unchanged above the edit, but its pointer
        // identities (mouse-face source ranges, display-replacement anchors)
        // carry the RANGE's positions, and a range reaching the edit point is
        // restructured by the insert itself — the end shifts, or the interval
        // splits around non-inheriting inserted text — which verbatim reuse
        // cannot reproduce. Reuse only the leading rows whose identities
        // provably cannot change (every buffer range strictly before the
        // edit), relaying the rest.
        let row_pointers_stable = |row: &GlyphRow| {
            row.pointer_appearances().iter().all(|appearance| {
                let identity = appearance.source;
                let range_ok = identity.kind != GlyphPointerSourceKind::Buffer
                    || (identity.range_end as i64) < dirty_start;
                let anchor_ok = match identity.occurrence {
                    GlyphPointerOccurrenceIdentity::BufferDisplayReplacement { anchor, .. } => {
                        (anchor as i64) < dirty_start
                    }
                    _ => true,
                };
                range_ok && anchor_ok
            })
        };
        let stable_prefix = body[..first_dirty_by_charpos]
            .iter()
            .take_while(|(_, row)| row_pointers_stable(row))
            .count();
        let first_dirty = first_dirty_by_charpos.min(stable_prefix);
        if first_dirty == 0 {
            return None;
        }
        let pointer_shrunk_prefix = first_dirty < first_dirty_by_charpos;
        let cursor_style = body
            .iter()
            .find_map(|(_, row)| row.cursor_type)
            .unwrap_or(CursorStyle::FilledBox);
        let mut reused_rows = Vec::with_capacity(first_dirty);
        let mut above_indices: rustc_hash::FxHashSet<i64> = rustc_hash::FxHashSet::default();
        for &(idx, row) in body.iter().take(first_dirty) {
            // Verbatim reuse is a refcount bump; only a row still carrying
            // cursor decoration pays a copy to strip it.
            let row = if row.cursor_col.is_some() || row.cursor_type.is_some() {
                let mut stripped = GlyphRow::clone(row);
                stripped.cursor_col = None;
                stripped.cursor_type = None;
                MatrixRow::new(stripped)
            } else {
                MatrixRow::clone(row)
            };
            above_indices.insert(idx as i64);
            reused_rows.push((idx, row));
        }
        let mut reused_row_snapshots: Vec<DisplayRowSnapshot> = self
            .display_snapshot
            .rows
            .iter()
            .filter(|row| above_indices.contains(&row.row))
            .cloned()
            .collect();
        let mut reused_points: Vec<DisplayPointSnapshot> = self
            .display_snapshot
            .points
            .iter()
            .filter(|point| above_indices.contains(&point.row))
            .cloned()
            .collect();
        let dirty_row = body[first_dirty].1;
        // The dirty SPAN: every body row whose OLD extent intersects
        // `[dirty_start, dirty_end_old)` must be relaid — with property changes
        // feeding the accumulator, the span routinely covers the whole
        // refontified region, not just the edited line. Rows strictly below the
        // span are untouched content whose positions shift by the edit delta.
        // A pure insert has an empty old span (`dirty_end_old == dirty_start`),
        // which keeps the span at exactly the edited row.
        let damage_span_last = (damage_first_by_charpos..body.len())
            .take_while(|&i| {
                i == damage_first_by_charpos || (body[i].1.start_charpos as i64) < dirty_end_old
            })
            .last()
            .unwrap_or(damage_first_by_charpos);
        // The physical retry also includes any predecessor row pulled in by
        // box-topology lookbehind. Keep that extra row OUT of the edit's
        // line-structure proof: its contents did not change, only its final
        // edge ownership depends on the damaged source position.
        let span_last = damage_span_last.max(first_dirty);
        let span_count = span_last - first_dirty + 1;

        // BELOW-REUSE (full try_window_id, post-walk-validated). When this is a
        // simple insert into a monospace span that has rows below it, build
        // the plan OPTIMISTICALLY: reuse the rows below the span too (content
        // unchanged, charpos shifted by the inserted count, same pixel_y) and
        // BOUND the walk to the span rows. The render validates post-walk
        // against `expected_walk` (row count, no continuation, end-charpos and
        // height continuity with the first reused-below row); on failure it
        // bails to a replay-free relayout.
        // `allow_below_reuse` is the kill switch for this path, not a staging
        // gate: it defaults to TRUE at every `LayoutEngine` construction site,
        // so below-reuse is the production path. Tests flip it off to isolate
        // above-only reuse.
        if allow_below_reuse {
            let delta = damage.delta();
            debug_assert_eq!(
                delta,
                curr.buffer_size - self.key.buffer_size,
                "EditDamage delta must equal the retained-key size delta"
            );
            // Every span row must be plain monospace text, and every span line
            // must PROVABLY still fit in one row after the insert (no wrap →
            // the rows below keep their pixel_y). `allow_below_reuse` already
            // guarantees the span chars are simple/char_width (the caller's
            // ASCII + structure-props check), so `(cols + delta) * char_width`
            // is exact. Applying `delta` to every span row is over-conservative
            // (the insert lands in exactly one of them) but always safe.
            let walk_rows = || body[first_dirty..=span_last].iter().map(|(_, row)| *row);
            let damage_rows = || {
                body[damage_first_by_charpos..=damage_span_last]
                    .iter()
                    .map(|(_, row)| *row)
            };
            let monospace = damage_rows().all(|row| {
                let text_glyphs = &row.glyphs[GlyphArea::Text.index()];
                !text_glyphs.is_empty()
                    && text_glyphs
                        .iter()
                        .all(|g| (g.pixel_width - curr.char_width).abs() < 0.5)
            });
            let stays_one_row = damage_rows().all(|row| {
                let cols = row.glyphs[GlyphArea::Text.index()].len();
                (cols as f32 + delta.max(0) as f32) * curr.char_width
                    <= curr.partition.text_body().width
            });
            // Pointer identities on the rows below shift with the insert only
            // when their buffer positions lie entirely at/after it. A range
            // that BEGINS above the edit can be restructured by the insert
            // itself (a text-property interval splits around non-inheriting
            // inserted text), which no position shift reproduces — such rows
            // fall back to the above-only reuse below, which relays them.
            let below_pointers_shiftable = body.iter().skip(span_last + 1).all(|(_, row)| {
                row.pointer_appearances().iter().all(|appearance| {
                    let identity = appearance.source;
                    let range_ok = identity.kind != GlyphPointerSourceKind::Buffer
                        || identity.range_start as i64 >= dirty_start;
                    let anchor_ok = match identity.occurrence {
                        GlyphPointerOccurrenceIdentity::BufferDisplayReplacement {
                            anchor, ..
                        } => anchor as i64 >= dirty_start,
                        _ => true,
                    };
                    range_ok && anchor_ok
                })
            });
            // `delta == 0` is the props-only refontification frame: content and
            // positions below the span are bitwise unchanged, so below-reuse
            // needs no shift and no fit proof beyond the span rows themselves.
            // `delta < 0` is a delete: the span rows only shrink (never wrap),
            // and the deleted chars cannot be inspected — a deleted NEWLINE
            // changes the row structure, which the prove-ahead gates cannot
            // see, so deletes lean entirely on the post-walk `expected_walk`
            // validation (the merged line misses the end-charpos contract and
            // the replay bails). Pointer-appearance shifting is add-only, so
            // deletes additionally require pointer-free rows below.
            let below_pointer_free = || {
                body.iter()
                    .skip(span_last + 1)
                    .all(|(_, row)| row.pointer_appearances().is_empty())
            };
            // Line-structure invariant: below-reuse is sound only when the
            // edit did not add or remove a newline — the span's line count
            // (and with it every below row's pixel_y) is preserved exactly
            // when the NEW span's newline count equals the OLD span's. The
            // old span's content is gone, but its newline count survives in
            // the retained rows: each span row whose line end lies strictly
            // inside the old span contributed exactly one newline. Typing
            // keeps 1/1 (the jit-lock line region includes the trailing
            // newline both sides); Enter makes 1 new against 0 old (or 2
            // against 1 under font-lock); a newline-join delete makes 0
            // against 1 — all structure changes fall to above-only here, with
            // the post-walk validation as the backstop for anything subtler.
            let old_span_newlines = damage_rows()
                .filter(|row| (row.end_charpos as i64) < dirty_end_old)
                .count();
            let line_structure_preserved = span_newlines == old_span_newlines;
            if !pointer_shrunk_prefix
                && span_last + 1 < body.len()
                && line_structure_preserved
                && monospace
                && stays_one_row
                && below_pointers_shiftable
                && (delta >= 0 || below_pointer_free())
            {
                let shift = |p: LispCharPos1| {
                    LispCharPos1::from_one_based_usize(
                        (p.to_one_based_usize() as i64 + delta) as usize,
                    )
                };
                let mut below_indices: rustc_hash::FxHashSet<i64> =
                    rustc_hash::FxHashSet::default();
                for &(idx, row) in body.iter().skip(span_last + 1) {
                    // A props-only frame (delta 0) shifts nothing: verbatim
                    // refcount reuse unless a stale cursor must be stripped.
                    if delta == 0 {
                        let row = if row.cursor_col.is_some() || row.cursor_type.is_some() {
                            let mut stripped = GlyphRow::clone(row);
                            stripped.cursor_col = None;
                            stripped.cursor_type = None;
                            MatrixRow::new(stripped)
                        } else {
                            MatrixRow::clone(row)
                        };
                        below_indices.insert(idx as i64);
                        reused_rows.push((idx, row));
                        continue;
                    }
                    let mut row = GlyphRow::clone(row);
                    row.cursor_col = None;
                    row.cursor_type = None;
                    // Every enabled body row below the edit sits at a real buffer
                    // position past the insert point, so all of them move by the
                    // inserted count — including empty lines (which carry their
                    // line's charpos) and the trailing EOB placeholder (which
                    // carries ZV, and ZV moves by `delta`). A full rebuild of the
                    // post-edit state reproduces exactly these shifted positions.
                    // Glyphs that map to no buffer position (the empty-row face
                    // anchor and `:extend` fill, charpos `NO_BUFFER_POSITION`)
                    // keep their sentinel.
                    row.start_charpos = (row.start_charpos as i64 + delta) as usize;
                    row.end_charpos = (row.end_charpos as i64 + delta) as usize;
                    for area in row.glyphs.iter_mut() {
                        for g in area.iter_mut() {
                            g.provenance = g
                                .provenance
                                .shifted_buffer_positions(dirty_start.max(0) as usize, delta);
                        }
                    }
                    // String indices are row-local string coordinates and do
                    // not move. Replacement coverage is occurrence-wide, so
                    // shift each row side-table entry exactly once.
                    row.shift_string_source_buffer_positions(dirty_start.max(0) as usize, delta);
                    // Pointer identities carry buffer positions too (mouse-face
                    // source ranges, display-replacement anchors); a range that
                    // crosses the relaid row into these reused rows must key
                    // identically to the fresh appearance the relaid row got,
                    // or hover paints the pieces separately.
                    // Add-only API; deletes are gated to pointer-free rows
                    // above, so a negative delta never reaches this call with
                    // anything to shift.
                    if delta > 0 {
                        row.shift_pointer_appearance_buffer_positions(
                            dirty_start.max(0) as u64,
                            delta as u64,
                        );
                    }
                    below_indices.insert(idx as i64);
                    reused_rows.push((idx, MatrixRow::new(row)));
                }
                for snap in self
                    .display_snapshot
                    .rows
                    .iter()
                    .filter(|r| below_indices.contains(&r.row))
                {
                    let mut snap = snap.clone();
                    snap.start_buffer_pos = snap.start_buffer_pos.map(shift);
                    snap.end_buffer_pos = snap.end_buffer_pos.map(shift);
                    reused_row_snapshots.push(snap);
                }
                for point in self
                    .display_snapshot
                    .points
                    .iter()
                    .filter(|p| below_indices.contains(&p.row))
                {
                    let mut point = point.clone();
                    point.buffer_pos = shift(point.buffer_pos);
                    reused_points.push(point);
                }
                let span_end_row = body[span_last].1;
                return Some(ScrollReplay {
                    dvpos: 0.0,
                    reused_rows,
                    reused_row_snapshots,
                    reused_points,
                    walk_start: PartialBodyWalkStart::new(dirty_row.start_charpos as i64),
                    exposed_row_base: body[first_dirty].0,
                    // Bound the walk to the dirty span's rows (no body row is
                    // continued, so every span line occupies exactly one row).
                    exposed_row_count: span_count,
                    exposed_text_y: dirty_row.pixel_y,
                    new_window_start: curr.window_start,
                    new_point: curr.point,
                    cursor_style,
                    bound_walk: true,
                    expected_walk: Some(ExpectedBoundWalk {
                        last_row_end_charpos: (span_end_row.end_charpos as i64 + delta) as usize,
                        total_height_px: walk_rows().map(|row| row.height_px).sum(),
                        row_count: span_count,
                    }),
                    chrome: None,
                    face_generation: self.face_generation,
                });
            }
        }

        Some(ScrollReplay {
            dvpos: 0.0,
            reused_rows,
            reused_row_snapshots,
            reused_points,
            walk_start: PartialBodyWalkStart::new(dirty_row.start_charpos as i64),
            exposed_row_base: body[first_dirty].0,
            exposed_row_count: body.len() - first_dirty,
            exposed_text_y: dirty_row.pixel_y,
            new_window_start: curr.window_start,
            new_point: curr.point,
            cursor_style,
            bound_walk: false,
            expected_walk: None,
            chrome: None,
            face_generation: self.face_generation,
        })
    }
}

/// Per-frame layout instrumentation — THE gate metric for every phase.
///
/// Reset at the top of each `layout_frame_rust`, populated as the frame is
/// committed, and read by the bench harness. Each phase ships ONLY when its
/// bench cases prove the win on relaid-row-count, not wall-time alone (spec §7).
#[derive(Clone, Debug, Default)]
pub struct LayoutStats {
    /// Buffer-text ("body") rows laid out from scratch this frame. THE
    /// rank-1 gate metric: Phase 1 cursor-only must drive this to 0.
    pub relaid_body_rows: usize,
    /// Chrome rows (mode/header/tab line) laid out this frame. Chrome is always
    /// re-walked (spec §4.2), so this stays nonzero even on the fast paths.
    pub relaid_chrome_rows: usize,
    /// Rows reused verbatim from the retained matrix. Phase 0a: 0.
    pub reused_rows: usize,
    /// Rows reused with a uniform vertical shift. Phase 0a: 0.
    pub reused_shifted_rows: usize,
    /// Windows classified `Full` this frame. Phase 0a: all of them.
    pub full_windows: usize,
    /// Windows that took the cursor-only fast path (Phase 1).
    pub cursor_only_windows: usize,
    /// Windows that took the pure-scroll fast path (Phase 2).
    pub scroll_windows: usize,
    /// Windows that took the localized-edit fast path (Phase 3).
    pub edit_windows: usize,
    /// Wall-time spent evaluating the reuse predicate (Phase 0a: ~0). Tracked
    /// because the predicate could approach relayout cost for screenfuls of
    /// short rows when the dirty set is small (spec §6).
    pub reuse_predicate_cpu: std::time::Duration,
}

impl LayoutStats {
    /// Total rows laid out from scratch this frame (body + chrome).
    pub fn relaid_rows(&self) -> usize {
        self.relaid_body_rows + self.relaid_chrome_rows
    }

    /// Total windows laid out this frame, across all classifications.
    pub fn total_windows(&self) -> usize {
        self.full_windows + self.cursor_only_windows + self.scroll_windows + self.edit_windows
    }

    /// Bump the per-class window counter for one laid-out window.
    pub fn record_window_class(&mut self, class: LayoutClass) {
        match class {
            LayoutClass::Full => self.full_windows += 1,
            LayoutClass::CursorOnly => self.cursor_only_windows += 1,
            LayoutClass::Scroll => self.scroll_windows += 1,
            LayoutClass::Edit => self.edit_windows += 1,
        }
    }
}

#[cfg(test)]
mod scroll_classifier_tests {
    use super::*;
    use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
    use neovm_core::window::{WindowDisplaySnapshot, WindowId};

    fn synthetic_key(window_start: i64, point: i64) -> RetainedWindowKey {
        RetainedWindowKey {
            media_generation: 0,
            buffer_id: 1,
            window_start,
            point,
            display_line_numbers: DisplayLineNumbersMode::Off,
            buffer_begv: 0,
            buffer_size: 1000,
            partition: WindowPartitionSignature::from_regions(
                neovm_core::window::PresentedWindowRegions {
                    outer: Rect::new(0.0, 0.0, 800.0, 616.0),
                    text_body: Rect::new(0.0, 0.0, 800.0, 600.0),
                    mode_line: Some(Rect::new(0.0, 600.0, 800.0, 16.0)),
                    ..Default::default()
                },
            ),
            hscroll: 0,
            vscroll: 0,
            wrap_mode: LineWrapMode::Truncate,
            word_wrap: false,
            tab_width: 8,
            char_width: 8.0,
            char_height: 16.0,
            font_pixel_size: 16.0,
            tab_stop_list: Vec::new(),
            extra_line_spacing: 0.0,
            selective_display: 0,
            selected: true,
            show_trailing_whitespace: false,
            trailing_ws_bg: 0,
            nobreak_char_display: 0,
            glyphless_char_fg: 0,
            indicate_empty_lines: 0,
            line_prefix: Vec::new(),
            wrap_prefix: Vec::new(),
            is_multibyte: true,
            chars_modified_tick: 5,
            props_modified_tick: 5,
            overlay_modified_tick: 5,
            face_change_count: 5,
            display_var_change_count: 5,
        }
    }

    /// A retained matrix with `n_body` 16px Text rows (10 chars each, the first
    /// starting at `base`) plus a mode-line row.
    fn synthetic_matrix(base: i64, n_body: usize) -> RetainedWindowMatrix {
        let mut matrix = GlyphMatrix::new(n_body + 1, 100);
        for i in 0..n_body {
            let row = MatrixRow::make_mut(&mut matrix.rows[i]);
            row.enabled = true;
            row.role = GlyphRowRole::Text;
            row.displays_text = true;
            row.start_charpos = (base + (i as i64) * 10) as usize;
            row.end_charpos = (base + (i as i64) * 10 + 9) as usize;
            row.pixel_y = (i as f32) * 16.0;
            row.height_px = 16.0;
            row.ascent_px = 16.0;
        }
        let ml = MatrixRow::make_mut(&mut matrix.rows[n_body]);
        ml.enabled = true;
        ml.role = GlyphRowRole::ModeLine;
        ml.mode_line = true;
        ml.pixel_y = (n_body as f32) * 16.0;
        ml.height_px = 16.0;
        RetainedWindowMatrix {
            matrix,
            key: synthetic_key(base, 0),
            validity: MatrixValidity::Valid,
            display_snapshot: WindowDisplaySnapshot {
                window_id: WindowId(1),
                text_area_left_offset: 0,
                mode_line_height: 16,
                header_line_height: 0,
                tab_line_height: 0,
                logical_cursor: None,
                phys_cursor: None,
                points: Vec::new(),
                rows: Vec::new(),
                ..WindowDisplaySnapshot::default()
            },
            presented_cursor: None,
            face_generation: FrameFaceGeneration::default(),
            chrome_uses_column: false,
            chrome_modified_flag: false,
        }
    }

    #[test]
    fn scroll_replay_detects_whole_row_scroll_down() {
        let m = synthetic_matrix(0, 5); // rows start at 0,10,20,30,40
        let curr = synthetic_key(20, 25); // scrolled to row 2, point followed
        let r = m
            .scroll_replay(&curr)
            .expect("whole-row scroll-down is eligible");
        assert_eq!(r.dvpos, -32.0, "removed two 16px rows");
        // Rows [2,3,4] reused into matrix indices [0,1,2] with shifted pixel_y.
        assert_eq!(
            r.reused_rows.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert_eq!(r.reused_rows[0].1.pixel_y, 0.0);
        assert_eq!(r.reused_rows[2].1.pixel_y, 32.0);
        assert_eq!(
            r.exposed_row_count, 2,
            "two newly-exposed rows at the bottom"
        );
        assert_eq!(r.exposed_row_base, 3);
        assert_eq!(r.walk_start.get(), 50); // after row 4 (chars 40..49)
        assert_eq!(r.exposed_text_y, 48.0); // 3rd visual row top
        assert_eq!(r.new_point, 25);
    }

    #[test]
    fn replay_face_references_come_from_the_rows_being_reused() {
        use neomacs_display_protocol::glyph_matrix::Glyph;

        let mut retained = synthetic_matrix(0, 3);
        MatrixRow::make_mut(&mut retained.matrix.rows[0]).glyphs[GlyphArea::Text.index()]
            .push(Glyph::char('a', FaceId::new(27), 0));
        MatrixRow::make_mut(&mut retained.matrix.rows[1]).glyphs[GlyphArea::Text.index()]
            .push(Glyph::char('b', FaceId::new(31), 10));

        let replay = retained
            .cursor_only_replay(&synthetic_key(0, 15))
            .expect("pure point movement reuses retained body rows");
        assert_eq!(
            replay.retained_face_ids(),
            vec![FaceId::new(27), FaceId::new(31)]
        );
    }

    #[test]
    fn scroll_replay_bails_on_partial_row_scroll() {
        let m = synthetic_matrix(0, 5);
        let curr = synthetic_key(15, 15); // not a row boundary
        assert!(m.scroll_replay(&curr).is_none());
    }

    #[test]
    fn scroll_replay_bails_on_scroll_up() {
        let m = synthetic_matrix(20, 5); // rows start at 20,30,40,50,60
        let curr = synthetic_key(0, 5); // above the retained top
        assert!(m.scroll_replay(&curr).is_none());
    }

    #[test]
    fn scroll_replay_bails_on_tick_change() {
        let m = synthetic_matrix(0, 5);
        let mut curr = synthetic_key(20, 25);
        curr.props_modified_tick += 1; // a text-property write co-occurred
        assert!(m.scroll_replay(&curr).is_none());
    }

    #[test]
    fn scroll_replay_bails_on_vscroll() {
        let m = synthetic_matrix(0, 5);
        let mut curr = synthetic_key(20, 25);
        curr.vscroll = 1; // pixel-level scroll offset
        assert!(m.scroll_replay(&curr).is_none());
    }

    #[test]
    fn scroll_replay_bails_when_unchanged() {
        let m = synthetic_matrix(0, 5);
        let curr = synthetic_key(0, 0); // window_start did not move
        assert!(m.scroll_replay(&curr).is_none());
    }

    /// Adversarial-review fix: each layout-affecting input that does NOT bump a
    /// modification tick (tab-stop-list, line-spacing, selective-display, window
    /// selection, trailing-whitespace, special-char display, fringes, line/wrap
    /// prefixes, multibyteness) must, when changed alone, force a FULL rebuild —
    /// otherwise the fast paths would reuse rows shaped under the old setting.
    #[test]
    fn fast_paths_bail_when_a_non_tick_layout_param_changes() {
        let prev = synthetic_key(0, 0);
        // Baseline: a pure point move IS cursor-only.
        assert!(RetainedWindowKey::cursor_only_eligible(
            &prev,
            &synthetic_key(0, 7)
        ));

        let mutations: &[(&str, fn(&mut RetainedWindowKey))] = &[
            ("display_line_numbers", |k| {
                k.display_line_numbers = DisplayLineNumbersMode::Relative
            }),
            ("tab_stop_list", |k| k.tab_stop_list = vec![4, 12]),
            ("extra_line_spacing", |k| k.extra_line_spacing = 2.0),
            ("selective_display", |k| k.selective_display = 4),
            ("selected", |k| k.selected = false),
            ("show_trailing_whitespace", |k| {
                k.show_trailing_whitespace = true
            }),
            ("trailing_ws_bg", |k| k.trailing_ws_bg = 0x00ff_00ff),
            ("nobreak_char_display", |k| k.nobreak_char_display = 1),
            ("glyphless_char_fg", |k| k.glyphless_char_fg = 0x00ff_ffff),
            ("indicate_empty_lines", |k| k.indicate_empty_lines = 1),
            ("left_fringe", |k| {
                k.partition.regions_mut().left_fringe = Some(Rect::new(0.0, 0.0, 8.0, 600.0))
            }),
            ("right_fringe", |k| {
                k.partition.regions_mut().right_fringe = Some(Rect::new(792.0, 0.0, 8.0, 600.0))
            }),
            ("left_margin", |k| {
                k.partition.regions_mut().left_margin = Some(Rect::new(0.0, 0.0, 16.0, 600.0))
            }),
            ("horizontal_scroll_bar", |k| {
                k.partition.regions_mut().horizontal_scroll_bar =
                    Some(Rect::new(0.0, 592.0, 800.0, 8.0))
            }),
            ("text_body_origin", |k| {
                k.partition.regions_mut().text_body.x += 8.0
            }),
            ("line_prefix", |k| k.line_prefix = vec![b'>', b' ']),
            ("wrap_prefix", |k| k.wrap_prefix = vec![b' ', b' ']),
            ("is_multibyte", |k| k.is_multibyte = false),
        ];
        for (name, mutate) in mutations {
            // Cursor-only (point also moved): must bail.
            let mut curr = synthetic_key(0, 7);
            mutate(&mut curr);
            assert!(
                !RetainedWindowKey::cursor_only_eligible(&prev, &curr),
                "{name} change must block the cursor-only fast path"
            );
            // Scroll (window_start also moved): must bail.
            let mut scrolled = synthetic_key(20, 7);
            mutate(&mut scrolled);
            assert!(
                !RetainedWindowKey::scroll_eligible(&prev, &scrolled),
                "{name} change must block the scroll fast path"
            );
            // Edit (chars tick + buffer_size also moved): must bail.
            let mut edited = synthetic_key(0, 7);
            edited.chars_modified_tick = prev.chars_modified_tick + 1;
            edited.buffer_size = prev.buffer_size + 1;
            mutate(&mut edited);
            assert!(
                !RetainedWindowKey::edit_eligible(&prev, &edited),
                "{name} change must block the edit fast path"
            );
        }
    }

    /// Phase 3 below-reuse classifier: a simple insert into a monospace edited
    /// row reuses the rows BELOW the edit (charpos shifted by the inserted count,
    /// pixel_y unchanged) and bounds the walk to the edited line. Gated on
    /// `allow_below_reuse`; with it off the plan is the above-only edit replay.
    #[test]
    fn edit_replay_below_reuse_shifts_rows_below_by_inserted_count() {
        use neomacs_display_protocol::glyph_matrix::{
            Glyph, GlyphProvenance, GlyphStringBufferRange, GlyphStringId, GlyphStringSource,
        };
        let mut m = synthetic_matrix(0, 5); // rows start at 0,10,20,30,40
        // Give the edited row (row 2, chars [20,29]) 10 monospace (8px) glyphs.
        for c in 0..10 {
            let mut g = Glyph::char('a', FaceId::new(0), (20 + c) as usize);
            g.pixel_width = 8.0;
            MatrixRow::make_mut(&mut m.matrix.rows[2]).glyphs[GlyphArea::Text.index()].push(g);
        }
        let row3 = MatrixRow::make_mut(&mut m.matrix.rows[3]);
        let string_source = row3
            .push_string_source(GlyphStringSource::replacement(
                GlyphStringId::new(7),
                GlyphStringBufferRange::new(30, 35),
            ))
            .expect("row-local string source");
        row3.glyphs[GlyphArea::Text.index()].push(
            Glyph::char('S', FaceId::new(0), 30)
                .with_provenance(GlyphProvenance::string(string_source, 4)),
        );
        // A 1-char insert at charpos 25 (inside row 2): chars tick moved, point +
        // buffer_size grew by 1, everything else equal.
        let mut curr = synthetic_key(0, 25);
        curr.chars_modified_tick = 6;
        curr.buffer_size = 1001;

        // allow_below_reuse = true → reuse above (0,1) AND below (3,4); below rows
        // are charpos-shifted by +1; the walk is bounded to the one edited row.
        let r = m
            .edit_replay(&curr, EditDamage::new(25, 26, 1, 0), true)
            .expect("below-reuse is eligible");
        assert!(r.bound_walk, "walk bounded to the edited line");
        assert_eq!(r.exposed_row_count, 1, "only the edited line is walked");
        assert_eq!(r.exposed_row_base, 2);
        assert_eq!(
            r.reused_rows.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
            vec![0, 1, 3, 4],
            "above (0,1) reused verbatim + below (3,4) reused shifted"
        );
        let below3 = r.reused_rows.iter().find(|(i, _)| *i == 3).unwrap();
        assert_eq!(below3.1.start_charpos, 31, "row 3 start 30 -> 31");
        assert_eq!(below3.1.end_charpos, 40, "row 3 end 39 -> 40");
        assert_eq!(
            below3.1.glyphs[GlyphArea::Text.index()][0].provenance,
            GlyphProvenance::string(string_source, 4),
            "reuse shifts the covered buffer range but never the string index"
        );
        assert_eq!(
            below3
                .1
                .string_source(string_source)
                .and_then(|source| source.covered_buffer_range()),
            Some(GlyphStringBufferRange::new(31, 36)),
            "reuse shifts the occurrence-wide range exactly once"
        );
        let below4 = r.reused_rows.iter().find(|(i, _)| *i == 4).unwrap();
        assert_eq!(below4.1.start_charpos, 41, "row 4 start 40 -> 41");

        // allow_below_reuse = false → the above-only edit replay (no below reuse,
        // walk runs to the bottom).
        let above_only = m
            .edit_replay(&curr, EditDamage::new(25, 26, 1, 0), false)
            .expect("above-only edit replay");
        assert!(!above_only.bound_walk);
        assert_eq!(
            above_only
                .reused_rows
                .iter()
                .map(|(i, _)| *i)
                .collect::<Vec<_>>(),
            vec![0, 1],
            "only the rows above the edit are reused"
        );
        assert_eq!(
            above_only.exposed_row_count, 3,
            "edited line + 2 rows below"
        );
    }

    /// A props-only refontification frame (font-lock after-the-fact pass, or
    /// the props part of a keystroke): chars tick unchanged, props tick moved,
    /// dirty span covering two rows. The span rows are relaid; the rows below
    /// the span are reused UNSHIFTED (delta = 0); expected_walk carries the
    /// span's continuity contract.
    #[test]
    fn edit_replay_props_only_span_relays_span_rows_and_reuses_below_unshifted() {
        use neomacs_display_protocol::glyph_matrix::Glyph;
        let mut m = synthetic_matrix(0, 5); // rows start at 0,10,20,30,40
        for row_idx in [2usize, 3] {
            let base = row_idx * 10;
            for c in 0..10 {
                let mut g = Glyph::char('a', FaceId::new(0), base + c);
                g.pixel_width = 8.0;
                MatrixRow::make_mut(&mut m.matrix.rows[row_idx]).glyphs[GlyphArea::Text.index()]
                    .push(g);
            }
        }
        // Font-lock rewrote faces over chars [22, 35): props tick moved, size
        // unchanged.
        let mut curr = synthetic_key(0, 25);
        curr.props_modified_tick = 6;

        let r = m
            .edit_replay(&curr, EditDamage::new(22, 35, 0, 1), true)
            .expect("props-only span replay is eligible");
        assert!(r.bound_walk);
        assert_eq!(r.exposed_row_base, 2, "span starts at row 2 (chars 20..)");
        assert_eq!(r.exposed_row_count, 2, "rows 2 and 3 intersect [22,35)");
        assert_eq!(
            r.reused_rows.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
            vec![0, 1, 4],
            "above (0,1) verbatim + below-span (4) reused"
        );
        let below = r.reused_rows.iter().find(|(i, _)| *i == 4).unwrap();
        assert_eq!(below.1.start_charpos, 40, "delta 0: no shift");
        let expected = r.expected_walk.expect("bound walk carries the contract");
        assert_eq!(expected.row_count, 2);
        assert_eq!(
            expected.last_row_end_charpos, 39,
            "old row-3 end 39 + delta 0"
        );
        assert!((expected.total_height_px - 32.0).abs() < 0.01);
    }

    #[test]
    fn edit_replay_face_change_at_row_start_invalidates_predecessor_box_terminal() {
        use neomacs_display_protocol::glyph_matrix::Glyph;

        let mut m = synthetic_matrix(0, 5); // old row ends: 9, 19, 29, 39, 49
        // Populate the two replayed span rows with fixed-width text so the
        // classifier can prove that unchanged rows below remain aligned.  The
        // assertion is then specifically about widening the property damage
        // to row 1 for row 2's source-face lookahead, not about the unrelated
        // conservative fallback for empty synthetic span rows.
        for row_idx in [1usize, 2] {
            let base = row_idx * 10;
            for charpos in base..base + 10 {
                let mut glyph = Glyph::char('a', FaceId::new(0), charpos);
                glyph.pixel_width = 8.0;
                MatrixRow::make_mut(&mut m.matrix.rows[row_idx]).glyphs[GlyphArea::Text.index()]
                    .push(glyph);
            }
        }
        let mut curr = synthetic_key(0, 20);
        curr.props_modified_tick += 1;

        let replay = m
            .edit_replay(&curr, EditDamage::new(20, 21, 0, 0), true)
            .expect("property-only replay is eligible");

        assert_eq!(
            replay.exposed_row_base, 1,
            "row 1 owns the box terminal whose lookahead is source position 20"
        );
        assert_eq!(
            replay
                .reused_rows
                .iter()
                .map(|(index, _)| *index)
                .collect::<Vec<_>>(),
            vec![0, 3, 4],
            "only rows strictly before the predecessor dependency and below the damage reuse"
        );
    }

    /// An insert whose accumulated dirty span (edit + refontification) covers
    /// two rows: both span rows are relaid, rows below the span shift by the
    /// insert delta.
    #[test]
    fn edit_replay_insert_with_multi_row_span_shifts_only_below_span() {
        use neomacs_display_protocol::glyph_matrix::Glyph;
        let mut m = synthetic_matrix(0, 5);
        for row_idx in [2usize, 3] {
            let base = row_idx * 10;
            for c in 0..10 {
                let mut g = Glyph::char('a', FaceId::new(0), base + c);
                g.pixel_width = 8.0;
                MatrixRow::make_mut(&mut m.matrix.rows[row_idx]).glyphs[GlyphArea::Text.index()]
                    .push(g);
            }
        }
        // Insert 1 char at 25, font-lock refontified [20, 36) (NEW coords).
        // Old-coordinate span end = 36 - 1 = 35.
        let mut curr = synthetic_key(0, 26);
        curr.chars_modified_tick = 6;
        curr.props_modified_tick = 7;
        curr.buffer_size = 1001;

        let r = m
            .edit_replay(&curr, EditDamage::new(20, 36, 1, 1), true)
            .expect("multi-row span insert replay is eligible");
        assert!(r.bound_walk);
        assert_eq!(
            r.exposed_row_base, 1,
            "row 1 owns the source lookahead into the edited span"
        );
        assert_eq!(
            r.exposed_row_count, 3,
            "topology predecessor plus rows 2 and 3 intersecting [20,35)"
        );
        assert_eq!(
            r.reused_rows.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
            vec![0, 4]
        );
        let below = r.reused_rows.iter().find(|(i, _)| *i == 4).unwrap();
        assert_eq!(below.1.start_charpos, 41, "row 4 start 40 -> 41");
        let expected = r.expected_walk.unwrap();
        assert_eq!(expected.row_count, 3);
        assert_eq!(
            expected.last_row_end_charpos, 40,
            "old row-3 end 39 + delta 1"
        );
    }

    /// Props tick movement alone now qualifies for the edit path (GNU
    /// try_window_id proceeds through property changes); overlay/face moves
    /// still escalate.
    #[test]
    fn edit_eligible_accepts_props_tick_movement_but_not_overlay_or_face() {
        let prev = synthetic_key(0, 10);
        let mut props_only = synthetic_key(0, 12);
        props_only.props_modified_tick = 9;
        assert!(RetainedWindowKey::edit_eligible(&prev, &props_only));

        let mut overlay_moved = props_only.clone();
        overlay_moved.overlay_modified_tick = 6;
        assert!(!RetainedWindowKey::edit_eligible(&prev, &overlay_moved));

        let mut face_moved = props_only.clone();
        face_moved.face_change_count = 6;
        assert!(!RetainedWindowKey::edit_eligible(&prev, &face_moved));
    }

    /// A simple in-line delete (delta < 0) also reuses the rows below the
    /// span, shifted DOWN by the deleted count; the deleted-newline hazard is
    /// owned by the post-walk expected_walk validation, so the plan builds
    /// optimistically.
    #[test]
    fn edit_replay_delete_reuses_below_rows_with_negative_shift() {
        use neomacs_display_protocol::glyph_matrix::Glyph;
        let mut m = synthetic_matrix(0, 5); // rows start at 0,10,20,30,40
        for c in 0..10 {
            let mut g = Glyph::char('a', FaceId::new(0), 20 + c);
            g.pixel_width = 8.0;
            MatrixRow::make_mut(&mut m.matrix.rows[2]).glyphs[GlyphArea::Text.index()].push(g);
        }
        // 1 char deleted at 25: old span [25, 26), new span empty; size -1.
        let mut curr = synthetic_key(0, 25);
        curr.chars_modified_tick = 6;
        curr.buffer_size = 999;

        let r = m
            .edit_replay(&curr, EditDamage::new(25, 25, -1, 0), true)
            .expect("delete below-reuse is eligible");
        assert!(r.bound_walk);
        assert_eq!(r.exposed_row_base, 2);
        assert_eq!(r.exposed_row_count, 1);
        assert_eq!(
            r.reused_rows.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
            vec![0, 1, 3, 4]
        );
        let below3 = r.reused_rows.iter().find(|(i, _)| *i == 3).unwrap();
        assert_eq!(below3.1.start_charpos, 29, "row 3 start 30 -> 29");
        let expected = r.expected_walk.unwrap();
        assert_eq!(expected.row_count, 1);
        assert_eq!(
            expected.last_row_end_charpos, 28,
            "old row-2 end 29 + delta -1"
        );
    }
}
