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
    DisplayLineNumbersMode, LineWrapMode, NobreakDisplayMode, PartialBodyWalkStart,
    PointMotionBodyDependency, WindowParams,
};
use crate::window_layout::{WindowLayoutBox, WindowPartitionSignature};
use neomacs_display_protocol::frame_glyphs::{DisplaySlotId, PhysCursor};
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

pub(crate) mod edit_sync;
pub(crate) mod mode_line_gate;

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
/// What actually moved between two layout keys.
///
/// The fast-path predicates were written as "clone the previous key, align the
/// fields this path tolerates, compare". The trick is a good one -- a field
/// ADDED to the key escalates by default, which is the safe direction -- but
/// it left every guard reading an anonymous pile of booleans, with no name for
/// the case that matters most. Four bugs in one day came from a guard
/// justified by MOTION being evaluated when nothing had moved: two scroll
/// bails, a cursor-row identity, and a displayed-column refusal.
///
/// Naming the delta makes "nothing moved" something a new guard has to think
/// about rather than something it falls through. `other` keeps the
/// align-and-compare trick, so a new key field still escalates until someone
/// deliberately accounts for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowDelta {
    /// Point moved. NOT on its own a reason to relay anything.
    pub point_moved: bool,
    /// Buffer text changed -- the chars tick, or the size that follows it.
    pub text_changed: bool,
    /// Text properties changed (font-lock refontifying the edited region).
    pub properties_changed: bool,
    /// The overlay SET changed. Keyed on the content digest, not the tick, so
    /// tooling that rebuilds an identical set does not count as a change --
    /// see `RetainedWindowKey::overlay_digest`.
    pub overlays_changed: bool,
    /// The visible region moved.
    pub window_start_moved: bool,
    /// Anything else in the key: geometry, faces, display variables, the
    /// wrap and tab settings, selection. The catch-all is what makes a newly
    /// added field safe by default.
    pub other_changed: bool,
}

impl WindowDelta {
    /// Decompose the difference between two keys.
    pub fn between(prev: &RetainedWindowKey, curr: &RetainedWindowKey) -> Self {
        let mut aligned = prev.clone();
        aligned.point = curr.point;
        aligned.chars_modified_tick = curr.chars_modified_tick;
        aligned.buffer_size = curr.buffer_size;
        aligned.props_modified_tick = curr.props_modified_tick;
        // The TICK is aligned away everywhere; `overlay_digest` is the field
        // that decides, so an identical rebuild is not a change.
        aligned.overlay_modified_tick = curr.overlay_modified_tick;
        aligned.overlay_digest = curr.overlay_digest;
        aligned.window_start = curr.window_start;
        Self {
            point_moved: prev.point != curr.point,
            text_changed: prev.chars_modified_tick != curr.chars_modified_tick
                || prev.buffer_size != curr.buffer_size,
            properties_changed: prev.props_modified_tick != curr.props_modified_tick,
            overlays_changed: prev.overlay_digest != curr.overlay_digest,
            window_start_moved: prev.window_start != curr.window_start,
            other_changed: aligned != *curr,
        }
    }

    /// Nothing about this window changed at all -- not even point.
    pub fn is_still(self) -> bool {
        !self.point_moved && self.only_point_moved_at_most()
    }

    /// Nothing changed except possibly point: the retained body is verbatim
    /// reusable.
    pub fn only_point_moved_at_most(self) -> bool {
        !self.text_changed
            && !self.properties_changed
            && !self.overlays_changed
            && !self.window_start_moved
            && !self.other_changed
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RetainedWindowKey {
    pub prefixes: neovm_core::window::LayoutPrefixInputs,
    pub invisibility: neovm_core::window::LayoutInvisibilityInput,
    pub char_table_revision: neovm_core::window::CharTableLayoutRevision,
    pub display_table: neovm_core::window::LayoutDisplayTableInput,
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
    /// Whether this is the frame's Lisp-selected window.
    pub selected: bool,
    /// Physical cursor ownership and active-vs-inactive cursor presentation.
    /// This can differ from `selected` while the cursor is in the echo area.
    pub cursor_role: crate::types::WindowCursorRole,
    /// `show-trailing-whitespace` + the resolved background color.
    pub show_trailing_whitespace: bool,
    pub trailing_ws_bg: u32,
    /// Effective `nobreak-char-display` + `nobreak-char-ascii-display` policy;
    /// changes special-character rendering.
    pub nobreak_char_display: NobreakDisplayMode,
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
    /// Content digest of the buffer's overlays (span + whole property list).
    /// The tick above says an overlay was TOUCHED; this says whether the set
    /// actually differs. Tooling rebuilds identical overlays constantly -- an
    /// LSP client re-creates its diagnostics on every change -- and without
    /// this every such rebuild forces a full relayout of the window.
    pub overlay_digest: u64,
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
            overlay_digest,
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
                    buffer.overlay_content_digest(),
                )
            })
            .unwrap_or((0, 0, 0, false, p.buffer_size, 0));
        Self {
            media_generation: evaluator.media_generation(),
            char_table_revision: neovm_core::window::CharTableLayoutRevision::current(),
            display_table: evaluator
                .layout_display_table_input(neovm_core::buffer::BufferId(p.buffer_id))
                .unwrap_or_default(),
            prefixes: evaluator
                .layout_prefix_inputs(neovm_core::buffer::BufferId(p.buffer_id))
                .unwrap_or_default(),
            invisibility: evaluator
                .layout_invisibility_input(neovm_core::buffer::BufferId(p.buffer_id))
                .unwrap_or_default(),
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
            cursor_role: p.cursor_role,
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
            overlay_digest,
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
        // Nothing moved except possibly point.
        //
        // Note what this now tolerates that it did not before: an overlay set
        // rebuilt identically. This predicate compared the raw
        // `overlay_modified_tick`, while `edit_eligible` compared the digest,
        // so the same identical rebuild was a change to one path and a
        // non-change to the other. That inconsistency was invisible while the
        // two predicates were written as separate field-alignment recipes;
        // saying "the overlay SET changed" once makes it impossible.
        WindowDelta::between(prev, curr).only_point_moved_at_most()
    }

    /// Whether a window may take the pure-scroll fast path: every layout input is
    /// identical to the retained (`prev`) key EXCEPT `window_start` (which moved)
    /// and `point` (which may follow the scroll). Any tick/geometry/face move
    /// escalates to a full rebuild. Whether the scroll is by WHOLE rows is decided
    /// separately against the retained matrix ([`RetainedWindowMatrix::scroll_replay`]).
    pub fn scroll_eligible(prev: &Self, curr: &Self) -> bool {
        let delta = WindowDelta::between(prev, curr);
        // The visible region moved and nothing else did.
        delta.window_start_moved
            && !delta.text_changed
            && !delta.properties_changed
            && !delta.overlays_changed
            && !delta.other_changed
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
        let delta = WindowDelta::between(prev, curr);
        // Something about the TEXT changed -- characters or properties -- and
        // nothing else did. An unchanged buffer is the cursor-only path's
        // business, not this one's.
        //
        // The buffer SIZE follows a char edit, so it counts as text rather
        // than as an escalation; `buffer_begv` does not, so a narrowing change
        // still lands in `other_changed`. The overlay TICK may move freely
        // because `overlays_changed` is keyed on the content digest: GNU has
        // to give up on any overlay modification (xdisp.c:22598 GIVE_UP (200))
        // because `OVERLAY_MODIFF` is its only signal and it has no per-window
        // retained key to hang a digest on.
        (delta.text_changed || delta.properties_changed)
            && !delta.overlays_changed
            && !delta.window_start_moved
            && !delta.other_changed
    }

    /// The names of every field that differs between two keys — the
    /// diagnostic behind a declined fast path (`RUST_LOG=neomacs_layout_engine=debug`).
    pub(crate) fn differing_fields(&self, other: &Self) -> Vec<&'static str> {
        macro_rules! diff {
            ($($field:ident),* $(,)?) => {{
                let mut out = Vec::new();
                $( if self.$field != other.$field { out.push(stringify!($field)); } )*
                out
            }};
        }
        diff!(
            prefixes,
            invisibility,
            char_table_revision,
            display_table,
            media_generation,
            buffer_id,
            window_start,
            point,
            display_line_numbers,
            buffer_begv,
            buffer_size,
            partition,
            hscroll,
            vscroll,
            wrap_mode,
            word_wrap,
            tab_width,
            char_width,
            char_height,
            font_pixel_size,
            tab_stop_list,
            extra_line_spacing,
            selective_display,
            selected,
            show_trailing_whitespace,
            trailing_ws_bg,
            nobreak_char_display,
            glyphless_char_fg,
            indicate_empty_lines,
            line_prefix,
            wrap_prefix,
            is_multibyte,
            chars_modified_tick,
            props_modified_tick,
            overlay_modified_tick,
            overlay_digest,
            face_change_count,
            display_var_change_count,
        )
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

/// Why a window could not take the cursor-only fast path.
///
/// A bare `None` was not enough to work with: a window that silently
/// full-rebuilds every frame costs as much as an edited one and leaves no
/// trace of which condition rejected it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorOnlyDecline {
    /// The retained matrix is not usable as a base.
    MatrixNotValid,
    /// Some layout input other than `point` moved.
    KeyMoved,
    /// Relative/visual line numbers derive every gutter value from point.
    LineNumbersCoverWholeWindow,
    /// Nothing non-chrome to reuse.
    NoBodyRows,
    /// No retained body row spans the current point.
    PointOutsideEveryRetainedRow,
    /// Absolute line numbers bake the current-line face into the margin, and
    /// point moved to a different display row.
    LineNumberedCursorRowChanged,
    /// The cursor's row cannot carry a re-decorated cursor.
    CursorRowNotReDecoratable,
    /// Point moved onto the last visible row with buffer left below, which a
    /// full layout might answer by scrolling.
    PointMoveMayScrollDown,
    /// Point moved onto the first visible row while scrolled off the buffer
    /// start, which a full layout might answer by scrolling.
    PointMoveMayScrollUp,
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
    /// `NEOMACS_MODE_LINE_GATE=gnu`: what the walked cursor line must look
    /// like for the retained `chrome` to stand (GNU's `display_line` result
    /// checks for optimization 1). The render drops the chrome and evaluates
    /// the mode line when the walk breaks it. `None` = no post-walk check.
    pub(crate) one_line_contract: Option<mode_line_gate::OneLineContract>,
    /// `NEOMACS_LAYOUT_EDIT_SYNC=sync`: the rows below the edit the walk may
    /// synchronize with (GNU `try_window_id`). The walk runs unbounded and
    /// stops where the next row would begin at the plan's `stop_charpos`;
    /// only then are these rows installed, moved by what the walk produced.
    pub(crate) sync: Option<edit_sync::EditSyncPlan>,
    /// An edit replay (window-start kept), as opposed to a scroll.
    pub(crate) edit: bool,
    /// Sealed frame-face generation that owns every ID in `reused_rows`.
    pub(crate) face_generation: FrameFaceGeneration,
}

/// Exact matrix-row identities reused by one accepted incremental replay.
///
/// Edit replay can reuse rows both above and below the regenerated span, so
/// the set is deliberately not representable as a prefix length.  Keeping the
/// identities in this type prevents commit and renderer provenance from
/// reconstructing a different set of rows from a lossy count.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ReusedMatrixRows {
    rows: std::collections::BTreeSet<usize>,
    /// The subset that moved vertically, with the shift's `f32` bits: an
    /// edit replay that synchronized below a span which changed height
    /// (`NEOMACS_LAYOUT_EDIT_SYNC`).
    shifted: Option<(std::collections::BTreeSet<usize>, u32)>,
}

impl ReusedMatrixRows {
    pub(crate) fn from_indices(indices: impl IntoIterator<Item = usize>) -> Self {
        Self {
            rows: indices.into_iter().collect(),
            shifted: None,
        }
    }

    pub(crate) fn from_replay_rows(rows: &[(usize, MatrixRow)]) -> Self {
        Self::from_indices(rows.iter().map(|(index, _)| *index))
    }

    /// Record that the rows at `indices` (already among the reused rows)
    /// moved down by `dy` pixels.
    pub(crate) fn with_shift(mut self, indices: impl IntoIterator<Item = usize>, dy: f32) -> Self {
        let indices: std::collections::BTreeSet<usize> = indices.into_iter().collect();
        if !indices.is_empty() && dy != 0.0 {
            self.shifted = Some((indices, dy.to_bits()));
        }
        self
    }

    pub(crate) fn contains(&self, index: usize) -> bool {
        self.rows.contains(&index)
    }

    pub(crate) fn len(&self) -> usize {
        self.rows.len()
    }

    /// The vertical shift the row at `index` took, if it moved.
    pub(crate) fn shift_of(&self, index: usize) -> Option<f32> {
        self.shifted
            .as_ref()
            .filter(|(rows, _)| rows.contains(&index))
            .map(|(_, dy)| f32::from_bits(*dy))
    }

    /// How many of the reused rows moved vertically.
    pub(crate) fn shifted_len(&self) -> usize {
        self.shifted.as_ref().map_or(0, |(rows, _)| rows.len())
    }
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
        // The rows an edit sync may install are retained rows too: their
        // faces must be admitted with the rest, whether or not the walk ends
        // up synchronizing.
        let sync_rows = self
            .sync
            .iter()
            .flat_map(|plan| plan.rows.iter().map(|(_, row)| row.as_ref()));
        referenced_face_ids(
            self.reused_rows
                .iter()
                .map(|(_, row)| row.as_ref())
                .chain(sync_rows)
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

        damage.start() >= start
            && damage.end_old() <= cursor_row.end_charpos as i64 + 1
            && replay.new_point >= start
            && replay.new_point <= end + 1
            && !region_contains_newline(start, replay.new_point)
    }

    /// Build a [`CursorOnlyReplay`] for this window if it can be reused this
    /// frame with only the cursor re-decorated, else `None` (→ full rebuild).
    ///
    /// Bails when the matrix is invalid, the reuse predicate fails
    /// ([`RetainedWindowKey::cursor_only_eligible`]), the new point lands outside
    /// the retained body rows, or the new cursor row is structurally unsafe
    /// (continuation / left-truncation / overlay-arrow fringe — the column
    /// resolve cannot place the cursor on those correctly).
    pub fn cursor_only_replay(
        &self,
        curr: &RetainedWindowKey,
    ) -> Result<CursorOnlyReplay, CursorOnlyDecline> {
        if self.validity != MatrixValidity::Valid {
            return Err(CursorOnlyDecline::MatrixNotValid);
        }
        if !RetainedWindowKey::cursor_only_eligible(&self.key, curr) {
            return Err(CursorOnlyDecline::KeyMoved);
        }
        let point_moved = self.key.point != curr.point;
        let point_dependency = curr.display_line_numbers.point_motion_body_dependency();
        if point_moved && point_dependency == PointMotionBodyDependency::EntireWindow {
            // GNU xdisp.c refuses cursor-only redisplay for relative and visual
            // line numbers: every gutter value is derived from point, so the
            // retained body as a whole is stale.
            return Err(CursorOnlyDecline::LineNumbersCoverWholeWindow);
        }
        let new_point = curr.point;
        let mut body_rows: Vec<(usize, MatrixRow)> = Vec::new();
        let mut body_indices: rustc_hash::FxHashSet<usize> = rustc_hash::FxHashSet::default();
        let mut retained_cursor_row_index: Option<usize> = None;
        let mut new_cursor: Option<(usize, &GlyphRow)> = None;
        for (idx, row) in self.matrix.rows.iter().enumerate() {
            if !row.enabled || Self::is_chrome_role(row.role) {
                continue;
            }
            if row.cursor_type.is_some() {
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
            return Err(CursorOnlyDecline::NoBodyRows);
        }
        let Some((new_cursor_row_index, cursor_row)) = new_cursor else {
            return Err(CursorOnlyDecline::PointOutsideEveryRetainedRow);
        };
        if point_moved
            && point_dependency == PointMotionBodyDependency::CurrentDisplayRow
            && retained_cursor_row_index != Some(new_cursor_row_index)
        {
            // Absolute line numbers bake the current-line face into the left
            // margin. Moving to another display row changes body decoration
            // even though the underlying number stays absolute.
            return Err(CursorOnlyDecline::LineNumberedCursorRowChanged);
        }
        if cursor_row.continued
            || cursor_row.truncated_left
            || cursor_row.left_fringe_bitmap.is_some()
        {
            return Err(CursorOnlyDecline::CursorRowNotReDecoratable);
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
        // Both bails are about a point MOVE onto an edge row, which is why they
        // are gated on `point_moved`. With point unchanged there is no motion
        // to keep visible: the retained matrix IS the layout a full pass
        // already accepted for exactly these inputs, scroll decision included.
        // Ungated, this rejected every frame of a window nothing had touched --
        // 200 consecutive frames of the rust-lsp-typing fixture rebuilt a
        // 7-row window in full with `differing=[]`.
        if point_moved {
            if Some(new_cursor_row_index) == last_body_index && !cursor_row.ends_at_zv {
                return Err(CursorOnlyDecline::PointMoveMayScrollDown);
            }
            if Some(new_cursor_row_index) == first_body_index && !window_at_buffer_top {
                return Err(CursorOnlyDecline::PointMoveMayScrollUp);
            }
        }
        let body_row_snapshots = self
            .display_snapshot
            .rows
            .iter()
            .filter(|snapshot| body_indices.contains(&(snapshot.row as usize)))
            .cloned()
            .collect();
        Ok(CursorOnlyReplay {
            body_rows,
            body_row_snapshots,
            points: self.display_snapshot.points.clone(),
            new_point,
            new_cursor_row_index,
            retained_cursor_row_index,
            // Chrome is decided separately, by the engine, because the decision
            // needs the chrome dirty flags off the evaluator. `None` = walk.
            chrome: None,
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
        // P3.5 G3: the start moved BACK. Walk the newly exposed rows from the
        // new start and synchronize with the old first row below them.
        if curr.window_start < body[0].1.start_charpos as i64 && edit_sync::scroll_back_enabled() {
            let plan = edit_sync::backward_scroll_plan(self, &body)?;
            let (first_index, first_row) = body[0];
            return Some(ScrollReplay {
                // The real shift is what the walk produces; the install
                // records it with the reused rows.
                dvpos: 0.0,
                reused_rows: Vec::new(),
                reused_row_snapshots: Vec::new(),
                reused_points: Vec::new(),
                walk_start: PartialBodyWalkStart::new(curr.window_start),
                exposed_row_base: first_index,
                exposed_row_count: body.len(),
                exposed_text_y: first_row.pixel_y,
                new_window_start: curr.window_start,
                new_point: curr.point,
                bound_walk: false,
                expected_walk: None,
                chrome: None,
                one_line_contract: None,
                sync: Some(plan),
                edit: false,
                face_generation: self.face_generation,
            });
        }
        let s = body.iter().position(|(_, row)| {
            row.displays_text && row.start_charpos as i64 == curr.window_start
        })?;
        if s == 0 {
            return None;
        }
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
            bound_walk: false,
            expected_walk: None,
            chrome: None,
            one_line_contract: None,
            sync: None,
            edit: false,
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
        self.edit_replay_with(
            curr,
            damage,
            if allow_below_reuse {
                edit_sync::BelowReuse::Prove
            } else {
                edit_sync::BelowReuse::Off
            },
        )
    }

    /// [`Self::edit_replay`] with the below-reuse strategy spelled out:
    /// [`edit_sync::BelowReuse::Sync`] synchronizes the walk with the first
    /// unchanged row the way GNU's `try_window_id` does instead of proving
    /// ahead that every changed line stays one row.
    pub(crate) fn edit_replay_with(
        &self,
        curr: &RetainedWindowKey,
        damage: EditDamage,
        below: edit_sync::BelowReuse,
    ) -> Option<ScrollReplay> {
        let allow_below_reuse = below.prove_allowed();
        let sync = matches!(below, edit_sync::BelowReuse::Sync { .. });
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
        // A row's `end_charpos` is where its last GLYPH came from, which is
        // short of where the row really ends when it closes with text a
        // `display` string replaces (or invisible text): the string's glyphs
        // carry its start. GNU's MATRIX_ROW_END_CHARPOS is the iterator
        // position instead. The next row's start is that position, so a row
        // reaches at least up to it: an edit just past a line-final display
        // string otherwise left the string's row "clean", reused it verbatim
        // and walked the next row from the wrong matrix index.
        let row_extent_end = |index: usize| -> i64 {
            let end = body[index].1.end_charpos as i64;
            match body.get(index + 1) {
                Some((_, next)) if next.start_charpos > body[index].1.start_charpos => {
                    end.max(next.start_charpos as i64 - 1)
                }
                _ => end,
            }
        };
        let damage_first_by_charpos =
            (0..body.len()).position(|index| row_extent_end(index) >= dirty_start)?;
        // First dirty row = first body row whose OLD extent reaches the widened
        // source dependency. Rows above it have unchanged positions and box
        // terminals.
        // An edit on the window's FIRST row leaves nothing above to reuse, but
        // the rows below still reuse shifted (GNU `try_window_id` regenerates
        // from the first row and syncs up with the rest the same way); only
        // the above-only fallback at the end has nothing to offer then.
        let mut first_dirty_by_charpos =
            (0..body.len()).position(|index| row_extent_end(index) >= topology_dirty_start)?;
        // The lookbehind only matters for a row that carries a box face: its
        // final glyph's box-run ownership is the one thing that depends on
        // the next character. Under `sync` a plain predecessor is not pulled
        // into the walk (the prove path keeps its historical widening).
        if sync
            && first_dirty_by_charpos < damage_first_by_charpos
            && !edit_sync::row_has_boxed_glyph(body[first_dirty_by_charpos].1)
        {
            first_dirty_by_charpos = damage_first_by_charpos;
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
        let pointer_shrunk_prefix = first_dirty < first_dirty_by_charpos;
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
        if sync && let Some(plan) = edit_sync::plan(self, &body, first_dirty, damage) {
            return Some(ScrollReplay {
                dvpos: 0.0,
                reused_rows,
                reused_row_snapshots,
                reused_points,
                walk_start: PartialBodyWalkStart::new(dirty_row.start_charpos as i64),
                exposed_row_base: body[first_dirty].0,
                // The walk is not bounded by a row count: it runs until it
                // synchronizes with `plan` or reaches the window bottom.
                exposed_row_count: body.len() - first_dirty,
                exposed_text_y: dirty_row.pixel_y,
                new_window_start: curr.window_start,
                new_point: curr.point,
                bound_walk: false,
                expected_walk: None,
                chrome: None,
                one_line_contract: None,
                sync: Some(plan),
                edit: true,
                face_generation: self.face_generation,
            });
        }

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
            let old_span_newlines = (damage_first_by_charpos..=damage_span_last)
                .filter(|&index| row_extent_end(index) < dirty_end_old)
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
                    bound_walk: true,
                    expected_walk: Some(ExpectedBoundWalk {
                        last_row_end_charpos: (span_end_row.end_charpos as i64 + delta) as usize,
                        total_height_px: walk_rows().map(|row| row.height_px).sum(),
                        row_count: span_count,
                    }),
                    chrome: None,
                    one_line_contract: None,
                    sync: None,
                    edit: true,
                    face_generation: self.face_generation,
                });
            }
        }

        // Above-only reuse with no row above the edit is a full walk in
        // disguise; let the plain rebuild own that frame.
        if reused_rows.is_empty() {
            return None;
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
            bound_walk: false,
            expected_walk: None,
            chrome: None,
            one_line_contract: None,
            sync: None,
            edit: true,
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
    /// Chrome rows installed from the retained matrix instead of walked.
    ///
    /// Without this, `relaid_chrome_rows` counted every enabled chrome row
    /// whether or not it was re-walked, so the stat COULD NOT show chrome
    /// reuse and a change that altered chrome cost left the numbers
    /// untouched.
    pub reused_chrome_rows: usize,
    /// ADMITTED WORK, not emitted rows. Everything above counts what the
    /// frame produced; these count what it spent to decide, which is where
    /// two regressions hid in one day: a chrome reuse that deep-cloned a
    /// snapshot per frame, and a composition scan that swept the whole
    /// buffer per window per frame. Both left every row counter identical.
    /// A statistic that cannot move when the cost moves is worse than none.
    pub buffer_snapshots_built: usize,
    /// Bytes of buffer text handed to the automatic-composition scan this
    /// frame. Proportional to BUFFER size today; it should be proportional to
    /// what is visible. Bytes rather than characters so the instrument stays
    /// O(1).
    pub composition_bytes_scanned: usize,
    /// Copy-on-write copies of buffer text since the previous accepted frame
    /// (`neovm_core::buffer::text_snapshot`). A snapshot that outlived its
    /// layout makes the next edit copy the whole buffer; steady-state typing
    /// under `NEOMACS_TEXT_SNAPSHOT=share` must keep this at 0.
    pub buffer_text_cow_copies: usize,
    /// Mini-windows that stood still this frame: every row reused through the
    /// cursor-only replay (`NEOMACS_LAYOUT_MINI_STILL`).
    pub mini_window_still: usize,
}

#[cfg(test)]
thread_local! {
    static MINI_STILL_OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Force `NEOMACS_LAYOUT_MINI_STILL` on this thread (tests only).
#[cfg(test)]
pub(crate) fn set_mini_window_still_for_test(enabled: Option<bool>) {
    MINI_STILL_OVERRIDE.with(|cell| cell.set(enabled));
}

/// `NEOMACS_LAYOUT_MINI_STILL=on` (P3.5 D): the mini-window is retained and
/// may take the cursor-only replay when nothing it displays changed. Read
/// once per process; default off.
pub(crate) fn mini_window_still_enabled() -> bool {
    #[cfg(test)]
    if let Some(enabled) = MINI_STILL_OVERRIDE.with(|cell| cell.get()) {
        return enabled;
    }
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        matches!(
            std::env::var("NEOMACS_LAYOUT_MINI_STILL")
                .ok()
                .map(|value| value.trim().to_ascii_lowercase())
                .as_deref(),
            Some("on" | "1" | "true" | "yes")
        )
    })
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
#[path = "incremental_layout/tests/scroll_classifier_test.rs"]
mod scroll_classifier_tests;
