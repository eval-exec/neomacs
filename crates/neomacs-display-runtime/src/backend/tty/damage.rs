//! P3.5 stage B: terminal output that costs what changed.
//!
//! - **B1, silent frames** (`NEOMACS_TTY_SILENT=off|on`). A frame whose plan
//!   has no operation and whose cursor did not move writes nothing at all;
//!   GNU writes 0 bytes on an idle redisplay. A frame where only the cursor
//!   moved writes only the cursor motion, and the cursor shape is re-sent
//!   only when it changed (re-sending it resets the blink in several
//!   terminals).
//! - **B2, painter keys** (`NEOMACS_TTY_DAMAGE=off|verify|on`). Every grid
//!   row remembers, as [`PainterKey`]s, what painted it last frame: the face
//!   fills over it and the window-matrix rows written to it, each by
//!   identity (a `MatrixRow`'s `Arc` pointer, which the renderer keeps alive
//!   so it cannot be reused) plus placement. The frame-wide inputs (grid
//!   size, cell metrics, default colours, face-map generation, terminal
//!   capabilities) form a [`FrameKey`]. When the frame key is unchanged, a
//!   row whose key list is unchanged rasterizes to exactly the cells the
//!   screen model already holds, so a *damage frame* clears, paints, and
//!   plans only the other rows, through the same per-row planner the full
//!   path uses. Frame chrome bands and the rows under child frames carry no
//!   identity and are repainted every frame. Child frames, a layout scroll
//!   hint (`ReusedShifted` rows), a forced repaint, or damage over a quarter
//!   of the rows take the full path, which also runs scroll detection; there
//!   the old path's carry of `RowDamage::Reused` rows is limited to rows
//!   whose painters are unchanged. (The old path carries every reused row,
//!   which leaves a closed child frame on screen when the rows under it were
//!   reused; `verify`'s full side keeps that behaviour, so such a frame shows
//!   up as a screen difference, not a false negative.)
//!
//!   Under `on` and `verify`, a full frame's scroll detection also reuses
//!   the screen model's row signatures (**B3**, [`RowSignatures`]): a row's
//!   hash and default-blank test are computed once when the row is planned
//!   and kept until an operation or a damage frame rewrites it, instead of
//!   rehashing every row of both grids each frame. Hashes only route scroll
//!   candidates (`verify_delta` compares the cells), so a stale signature
//!   could only cost a missed scroll, never a wrong screen.
//!
//!   `verify` runs the full path (whose output reaches the terminal) and
//!   the damage path on a copy of the renderer, and compares the screen
//!   models they leave: a row the damage path skipped whose content (cells
//!   other than their erased/written materialization) differs from the full
//!   path's is a false negative, a stale screen. It also counts rows the full
//!   path rewrote without changing their content (its materialization churn:
//!   a colored blank tail written as spaces is re-marked erasable every frame
//!   and so rewritten every frame; GNU leaves an unchanged row alone, as the
//!   damage path does), materialization-only differences, and frames whose
//!   bytes differ; see [`TtyDamageVerifyTotals`]. With
//!   `NEOMACS_TTY_DAMAGE_REPORT_FILE` set, every verified frame appends one
//!   line there.
//!
//! Every knob is read once per process, when the first [`TtyRif`] is built,
//! and defaults to the old behaviour until measured (same-binary A/B).

use super::*;
use std::sync::OnceLock;

/// The value `NEOMACS_TTY_SILENT` selects.
pub fn parse_tty_silent_knob(value: Option<&str>) -> bool {
    matches!(
        value
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref(),
        Some("on" | "1" | "true" | "yes")
    )
}

pub(super) fn knob_silent_frames() -> bool {
    static SILENT: OnceLock<bool> = OnceLock::new();
    *SILENT
        .get_or_init(|| parse_tty_silent_knob(std::env::var("NEOMACS_TTY_SILENT").ok().as_deref()))
}

/// How the renderer decides which rows to repaint (`NEOMACS_TTY_DAMAGE`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TtyDamageMode {
    /// Rasterize and plan every row every frame (the old path).
    #[default]
    Off,
    /// Write the old path's output, run the damage path beside it on a copy
    /// of the renderer, and count every disagreement.
    Verify,
    /// Rasterize and plan only the rows whose painters changed.
    On,
}

/// The mode a value of `NEOMACS_TTY_DAMAGE` selects.
pub fn parse_tty_damage_knob(value: Option<&str>) -> TtyDamageMode {
    match value
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("on" | "1" | "true" | "yes") => TtyDamageMode::On,
        Some("verify") => TtyDamageMode::Verify,
        None | Some("" | "off" | "0" | "false" | "no") => TtyDamageMode::Off,
        Some(other) => {
            tracing::warn!(value = other, "NEOMACS_TTY_DAMAGE: unknown mode, using off");
            TtyDamageMode::Off
        }
    }
}

pub(super) fn knob_damage_mode() -> TtyDamageMode {
    static MODE: OnceLock<TtyDamageMode> = OnceLock::new();
    *MODE.get_or_init(|| parse_tty_damage_knob(std::env::var("NEOMACS_TTY_DAMAGE").ok().as_deref()))
}

/// Where the terminal's cursor is known to be after the last frame written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EmittedCursor {
    /// Nothing is known (no frame yet, or a forced full repaint is due).
    Unknown,
    Hidden,
    Shown {
        row: u16,
        col: u16,
    },
}

/// What one painter contributes to one grid row.
///
/// Equal key lists (in paint order) for a row, under an equal [`FrameKey`],
/// rasterize that row to equal cells. Every input of the painters must be
/// here or in the frame key; a missing one is a stale screen, which
/// `NEOMACS_TTY_DAMAGE=verify` exists to catch.
#[derive(Clone, Debug)]
pub(super) enum PainterKey {
    /// A face fill over the row. Its window and role also select the
    /// background of a window row's implicit blanks (`blank_cell_attrs`).
    FaceFill {
        window: DisplayWindowId,
        role: GlyphRowRole,
        rect: TtyCellRect,
        attrs: CellAttrs,
    },
    /// A window-matrix row written to this grid row.
    WindowRow {
        window: DisplayWindowId,
        /// The `MatrixRow`'s `Arc` allocation: the row's content identity.
        /// The renderer holds last frame's rows ([`DamageState::retained`]),
        /// so an address cannot be reused while it is being compared.
        row: usize,
        col: i64,
        area: GlyphRowAreaLayout,
    },
    /// A painter with no content identity (frame chrome bands): never equal
    /// to anything, so its rows are repainted every frame.
    Volatile,
}

impl PartialEq for PainterKey {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::FaceFill {
                    window,
                    role,
                    rect,
                    attrs,
                },
                Self::FaceFill {
                    window: other_window,
                    role: other_role,
                    rect: other_rect,
                    attrs: other_attrs,
                },
            ) => {
                window == other_window
                    && role == other_role
                    && rect == other_rect
                    && attrs == other_attrs
            }
            (
                Self::WindowRow {
                    window,
                    row,
                    col,
                    area,
                },
                Self::WindowRow {
                    window: other_window,
                    row: other_row,
                    col: other_col,
                    area: other_area,
                },
            ) => {
                window == other_window && row == other_row && col == other_col && area == other_area
            }
            _ => false,
        }
    }
}

/// The frame-wide inputs of every painter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct FrameKey {
    width: usize,
    height: usize,
    char_width: u32,
    char_height: u32,
    default_bg: Option<TerminalColor>,
    default_fg: Option<TerminalColor>,
    faces_generation: u64,
    caps: TermCaps,
}

/// Why a frame under `on` took the full path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TtyFullFrameReason {
    /// `verify`'s own side, which is always the full path.
    VerifyFullSide,
    /// A forced repaint (first frame, resize, `force_redraw`).
    Forced,
    /// Child frames are visible.
    ChildFrames,
    /// A window reported a scroll (`ReusedShifted` rows).
    LayoutScroll,
    /// The screen model's painters are unknown (after a failure or a mode
    /// change).
    UnknownPainters,
    /// A frame-wide input changed (grid, metrics, default colours,
    /// capabilities).
    FrameGeometry,
    /// A face the rows may use changed or went away.
    FaceChange,
    /// A quarter of the rows or more changed painters.
    WideDamage,
}

/// How the frame being rendered was rasterized.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum FrameKind {
    /// Every row was rasterized; the planner diffs the whole grid.
    #[default]
    Full,
    /// Only [`DamageState::touched`] rows were rasterized; the others hold
    /// stale cells and must be neither planned nor committed.
    Damage,
}

/// Counts of `NEOMACS_TTY_DAMAGE=verify` since the renderer was built.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TtyDamageVerifyTotals {
    /// Frames verified.
    pub frames: u64,
    /// Of those, frames the damage path rendered as damage frames.
    pub damage_frames: u64,
    /// Rows the damage path skipped whose content the full path changed:
    /// stale rows. The premise of the damage path is that this is 0.
    pub false_negatives: u64,
    /// Rows whose content (all but materialization) differs after the frame,
    /// skipped or not.
    pub screen_diff_rows: u64,
    /// Rows equal in content whose erased/written materialization differs.
    pub materialization_diff_rows: u64,
    /// Rows the damage path skipped on which the full path planned an
    /// operation that left their content unchanged.
    pub redundant_rewrite_rows: u64,
    /// Frames whose output bytes differ.
    pub byte_diff_frames: u64,
}

/// B3: what scroll detection needs of one row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RowSignature {
    hash: u64,
    default_blank: bool,
}

impl RowSignature {
    fn of(row: &[TtyCell]) -> Self {
        Self {
            hash: row_hash(row),
            default_blank: row.iter().all(|cell| cell == &TtyCell::default()),
        }
    }

    /// The value `detect_scroll` votes with for row ROW.
    fn scroll_hash(self, row: usize) -> u64 {
        if self.default_blank {
            DEFAULT_BLANK_SENTINEL | row as u64
        } else {
            self.hash
        }
    }
}

/// B3: row signatures of the screen model, and of the desired rows this
/// frame's scroll detection computed (promoted when the frame commits).
#[derive(Clone, Debug, Default)]
pub(crate) struct RowSignatures {
    current: Vec<Option<RowSignature>>,
    desired: Vec<Option<RowSignature>>,
    /// Screen-model signatures reused rather than recomputed this frame.
    pub(super) reused: u32,
}

impl RowSignatures {
    /// `detect_scroll`'s per-row hashes of both grids, reusing the screen
    /// model's signatures where known.
    pub(super) fn scroll_hashes(
        &mut self,
        current: &TtyGrid,
        desired: &TtyGrid,
    ) -> (Vec<u64>, Vec<u64>) {
        let height = desired.height;
        self.reused = 0;
        self.current.resize(height, None);
        self.desired.clear();
        self.desired.resize(height, None);
        let mut old = Vec::with_capacity(height);
        let mut new = Vec::with_capacity(height);
        for row in 0..height {
            if desired.row_provably_unchanged(row) {
                old.push(CARRIED_SENTINEL | row as u64);
                new.push(CARRIED_SENTINEL | row as u64);
                continue;
            }
            let before = match self.current[row] {
                Some(known) => {
                    self.reused += 1;
                    known
                }
                None => *self.current[row].insert(RowSignature::of(current.row(row))),
            };
            old.push(before.scroll_hash(row));
            let now = RowSignature::of(desired.row(row));
            self.desired[row] = Some(now);
            new.push(now.scroll_hash(row));
        }
        (old, new)
    }

    fn clear(&mut self) {
        self.current.clear();
        self.desired.clear();
    }
}

/// Per-[`TtyRif`] state of the stage-B knobs.
#[derive(Clone)]
pub(super) struct DamageState {
    /// B1 is on for this renderer.
    pub(super) silent: bool,
    /// The cursor the terminal shows after the last frame written (B1).
    pub(super) cursor: EmittedCursor,
    /// The cursor shape last written, if known (B1).
    pub(super) shape: Option<TerminalCursorShape>,
    /// B2 mode for this renderer.
    pub(super) mode: TtyDamageMode,
    /// Why the last rasterized frame was full, under `on`/`verify`.
    pub(super) full_reason: Option<TtyFullFrameReason>,
    /// How the frame awaiting `diff_and_render`/`paint` was rasterized.
    pub(super) frame: FrameKind,
    /// The painters of every row of the screen model (`current`), or `None`
    /// when unknown.
    rows: Option<Vec<Vec<PainterKey>>>,
    frame_key: Option<FrameKey>,
    /// The painters of the frame awaiting commit.
    next_rows: Vec<Vec<PainterKey>>,
    next_frame_key: Option<FrameKey>,
    /// The matrix rows `rows` names by address, kept alive (see
    /// [`PainterKey::WindowRow`]).
    retained: Vec<MatrixRow>,
    next_retained: Vec<MatrixRow>,
    /// Rows whose painters changed since the screen model (all of them when
    /// that is unknown): the rows a damage frame repaints, and the rows a
    /// full frame must not carry.
    pub(super) touched: Vec<bool>,
    /// The full path carries every `RowDamage::Reused` row, as under `off`
    /// (and on `verify`'s full side); otherwise only untouched rows.
    pub(super) legacy_carry: bool,
    /// Bumped whenever the installed face map changes content.
    pub(super) faces_generation: u64,
    /// The damage path's renderer beside this one, under `verify`.
    shadow: Option<Box<TtyRif>>,
    /// Rows this frame's plan writes to (`on` and `verify`).
    op_rows: Vec<bool>,
    op_rows_valid: bool,
    /// B3 row signatures.
    pub(super) signatures: RowSignatures,
    verify: TtyDamageVerifyTotals,
}

impl DamageState {
    pub(super) fn from_knobs() -> Self {
        Self {
            silent: knob_silent_frames(),
            cursor: EmittedCursor::Unknown,
            shape: None,
            mode: knob_damage_mode(),
            full_reason: None,
            frame: FrameKind::Full,
            rows: None,
            frame_key: None,
            next_rows: Vec::new(),
            next_frame_key: None,
            retained: Vec::new(),
            next_retained: Vec::new(),
            touched: Vec::new(),
            legacy_carry: true,
            faces_generation: 0,
            shadow: None,
            op_rows: Vec::new(),
            op_rows_valid: false,
            signatures: RowSignatures::default(),
            verify: TtyDamageVerifyTotals::default(),
        }
    }

    /// Forget what the terminal shows: the next frame writes it all.
    pub(super) fn forget_terminal(&mut self) {
        self.cursor = EmittedCursor::Unknown;
        self.shape = None;
    }

    /// Forget what painted the screen model: the next frame is a full one.
    pub(super) fn forget_painters(&mut self) {
        self.rows = None;
        self.frame_key = None;
        self.retained.clear();
        self.signatures.clear();
    }
}

/// Whether two faces rasterize alike on a terminal: every field
/// `TtyRif::resolve_attrs` and the default colours read (whole `attributes`
/// and `font_weight`, which is conservative).
pub(super) fn same_on_a_terminal(
    left: &neomacs_display_protocol::face::Face,
    right: &neomacs_display_protocol::face::Face,
) -> bool {
    left.use_default_foreground == right.use_default_foreground
        && left.terminal_foreground == right.terminal_foreground
        && left.use_default_background == right.use_default_background
        && left.terminal_background == right.terminal_background
        && left.attributes == right.attributes
        && left.font_weight == right.font_weight
        && left.underline_style == right.underline_style
        && left.terminal_underline_color == right.terminal_underline_color
}

/// Whether two rows show the same thing: every cell field but the
/// erased/written materialization.
fn cells_same_content(left: &[TtyCell], right: &[TtyCell]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.ch == right.ch
                && left.attrs == right.attrs
                && left.blank_erase == right.blank_erase
                && left.padding == right.padding
                && left.extenders == right.extenders
                && left.terminal_advance == right.terminal_advance
        })
}

impl TtyGrid {
    /// [`Self::clear`] for one row.
    fn clear_row(&mut self, row: usize, bg: Option<TerminalColor>) {
        if row >= self.height {
            return;
        }
        let blank = TtyCell {
            attrs: CellAttrs {
                bg,
                ..CellAttrs::default()
            },
            ..TtyCell::default()
        };
        self.cells[row * self.width..(row + 1) * self.width].fill(blank);
        self.row_written[row] = false;
        self.row_carried[row] = false;
    }

    fn row(&self, row: usize) -> &[TtyCell] {
        &self.cells[row * self.width..(row + 1) * self.width]
    }
}

impl TtyRif {
    /// Turn B1 silent frames on or off for this renderer, overriding
    /// `NEOMACS_TTY_SILENT` (tests and embedders).
    pub fn set_silent_frames(&mut self, silent: bool) {
        self.damage.silent = silent;
        self.damage.forget_terminal();
    }

    /// Choose the B2 mode for this renderer, overriding `NEOMACS_TTY_DAMAGE`
    /// (tests and embedders).
    pub fn set_damage_mode(&mut self, mode: TtyDamageMode) {
        self.damage.mode = mode;
        self.damage.forget_painters();
        self.damage.shadow = None;
        self.damage.legacy_carry = true;
    }

    /// What `NEOMACS_TTY_DAMAGE=verify` counted so far.
    pub fn damage_verify_totals(&self) -> TtyDamageVerifyTotals {
        self.damage.verify
    }

    // -----------------------------------------------------------------------
    // B1
    // -----------------------------------------------------------------------

    /// The cursor this frame leaves on the terminal.
    fn frame_cursor(&self) -> EmittedCursor {
        if self.cursor_visible {
            EmittedCursor::Shown {
                row: self.cursor_row,
                col: self.cursor_col,
            }
        } else {
            EmittedCursor::Hidden
        }
    }

    /// B1: write a frame whose plan is empty. Returns false when the frame
    /// needs the ordinary framing (B1 off, or a forced repaint).
    ///
    /// Nothing is written when the cursor is where the terminal already shows
    /// it; otherwise only the cursor motion is written: no synchronized-update
    /// bracket and no hide/show pair around a frame that writes no cell.
    pub(super) fn write_quiet_frame(&mut self, ops: &[TermOp]) -> bool {
        if !self.damage.silent || !ops.is_empty() || self.force_full_render {
            return false;
        }
        let now = self.frame_cursor();
        let before = self.damage.cursor;
        match now {
            EmittedCursor::Shown { row, col } => {
                if before != now {
                    write_cursor_goto(&mut self.output, row + 1, col + 1);
                }
                if self.damage.shape != Some(self.cursor_shape) {
                    write_cursor_shape(&mut self.output, self.cursor_shape);
                    self.damage.shape = Some(self.cursor_shape);
                }
                if !matches!(before, EmittedCursor::Shown { .. }) {
                    self.output.extend_from_slice(b"\x1b[?25h");
                }
            }
            EmittedCursor::Hidden => {
                if before != EmittedCursor::Hidden {
                    self.output.extend_from_slice(b"\x1b[?25l");
                }
            }
            EmittedCursor::Unknown => unreachable!("a frame always knows its cursor"),
        }
        self.damage.cursor = now;
        true
    }

    /// B1 for a frame that writes cells: the cursor shape is re-sent only when
    /// it changed. With B1 off it is sent every frame, as before.
    pub(super) fn cursor_shape_due(&self) -> bool {
        !self.damage.silent || self.damage.shape != Some(self.cursor_shape)
    }

    /// Record the cursor an ordinarily framed frame left on the terminal.
    pub(super) fn note_framed_cursor(&mut self, shape_written: bool) {
        self.damage.cursor = self.frame_cursor();
        if shape_written {
            self.damage.shape = Some(self.cursor_shape);
        }
    }

    // -----------------------------------------------------------------------
    // B2: rasterize
    // -----------------------------------------------------------------------

    /// Install STATE's face map; outside `off`, only when its content changed.
    ///
    /// The generation the frame key carries moves only when a face the old
    /// map had changed what a terminal shows ([`same_on_a_terminal`]) or went
    /// away: a row keyed by identity references only faces that existed when
    /// it was laid out, so a face added since (the `region` face on the first
    /// `set-mark`) cannot change how it looks, and neither can a GUI-only
    /// metric (the default face's `font_ascent` alternates between frames).
    pub(super) fn install_face_map(&mut self, state: &FrameDisplayState) {
        if self.damage.mode == TtyDamageMode::Off {
            self.faces = state.faces.clone();
        } else if self.faces != state.faces {
            let invalidates = self.faces.iter().any(|(id, face)| {
                !state
                    .faces
                    .get(id)
                    .is_some_and(|new| same_on_a_terminal(face, new))
            });
            self.faces = state.faces.clone();
            if invalidates {
                self.damage.faces_generation = self.damage.faces_generation.wrapping_add(1);
            }
        }
    }

    /// The rasterizer under `on` and `verify`.
    pub(super) fn rasterize_frame_tree_damaged(
        &mut self,
        root: &FrameDisplayState,
        children: &[&FrameDisplayState],
    ) {
        if self.damage.mode == TtyDamageMode::Verify {
            let mut shadow = self.clone_for_verify();
            shadow.damage.mode = TtyDamageMode::On;
            shadow.rasterize_frame(root, children, true);
            self.rasterize_frame(root, children, false);
            self.damage.shadow = Some(Box::new(shadow));
            return;
        }
        self.rasterize_frame(root, children, true);
    }

    fn clone_for_verify(&self) -> TtyRif {
        let mut copy = self.clone();
        copy.damage.shadow = None;
        copy.output.clear();
        copy
    }

    /// Rasterize one frame, as a damage frame when ALLOW_DAMAGE and the
    /// painters permit, else in full; either way record the painter keys the
    /// commit makes current.
    fn rasterize_frame(
        &mut self,
        root: &FrameDisplayState,
        children: &[&FrameDisplayState],
        allow_damage: bool,
    ) {
        self.install_state_faces(root);
        let has_shift = self.build_painter_keys(root, children);
        let small = self.mark_touched_rows();
        let reason = if !allow_damage {
            Some(TtyFullFrameReason::VerifyFullSide)
        } else if self.force_full_render {
            Some(TtyFullFrameReason::Forced)
        } else if !children.is_empty() {
            Some(TtyFullFrameReason::ChildFrames)
        } else if has_shift {
            Some(TtyFullFrameReason::LayoutScroll)
        } else if self.damage.rows.is_none() {
            Some(TtyFullFrameReason::UnknownPainters)
        } else if let (Some(before), Some(now)) =
            (self.damage.frame_key, self.damage.next_frame_key)
            && before != now
        {
            Some(if before.faces_generation != now.faces_generation {
                TtyFullFrameReason::FaceChange
            } else {
                TtyFullFrameReason::FrameGeometry
            })
        } else if !small {
            Some(TtyFullFrameReason::WideDamage)
        } else {
            None
        };
        let damage = reason.is_none();
        self.damage.full_reason = reason;
        self.damage.legacy_carry = !allow_damage;
        self.cursor_visible = false;
        self.cursor_shape = TerminalCursorShape::Block;
        if damage {
            self.damage.frame = FrameKind::Damage;
            let touched = std::mem::take(&mut self.damage.touched);
            self.rasterize_root_damaged(root, &touched);
            self.damage.touched = touched;
        } else {
            self.damage.frame = FrameKind::Full;
            self.desired.clear(self.default_bg);
            self.rasterize_state_at(root, 0, 0, false);
            for child in children {
                let outer = child.frame_placement.outer_in_parent();
                let origin_col = outer.x().round() as i64;
                let origin_row = outer.y().round() as i64;
                self.draw_child_border(child, origin_col, origin_row);
                self.rasterize_state_at(child, origin_col, origin_row, true);
            }
        }
        tracing::trace!(
            target: "neomacs::tty_damage",
            kind = ?self.damage.frame,
            touched = self.damage.touched.iter().filter(|touched| **touched).count(),
            "tty frame rasterized"
        );
    }

    /// Record every row's painters for the frame being rasterized. Returns
    /// whether a window reports a scroll (`ReusedShifted` rows), which only
    /// the full path's scroll detection handles.
    fn build_painter_keys(
        &mut self,
        state: &FrameDisplayState,
        children: &[&FrameDisplayState],
    ) -> bool {
        let (width, height) = (self.desired.width, self.desired.height);
        let mut rows = std::mem::take(&mut self.damage.next_rows);
        rows.resize_with(height, Vec::new);
        rows.iter_mut().for_each(Vec::clear);
        let mut retained = std::mem::take(&mut self.damage.next_retained);
        retained.clear();
        let char_w = state.char_width.max(1.0);
        let char_h = state.char_height.max(1.0);

        // Face fills, in paint order (`rasterize_face_fill`).
        for fill in &state.face_fills {
            let Some(rect) = projected_face_fill_rect(fill, char_w, char_h) else {
                continue;
            };
            if visible_cell_range(rect.left, rect.width(), width).is_empty() {
                continue;
            }
            let attrs = self.resolve_attrs(fill.face_id);
            for row in visible_cell_range(rect.top, rect.height(), height) {
                rows[row].push(PainterKey::FaceFill {
                    window: fill.window_id,
                    role: fill.row_role,
                    rect,
                    attrs,
                });
            }
        }

        // Frame chrome bands carry no identity: every row a band can reach
        // is repainted (`rasterize_frame_chrome_bands`).
        for band in state.frame_chrome.bands() {
            let bounds = band.bounds().raw();
            let band_row = (bounds.y / char_h).round() as i64;
            let painted_lines = match band.content() {
                FrameChromeContent::DisplayRow(_) => 1,
                FrameChromeContent::MenuBar(_) => {
                    (band.bounds().height() / char_h).round().max(1.0) as usize
                }
                FrameChromeContent::ToolBar(_) | FrameChromeContent::CompactBar(_) => 0,
            };
            let lines = painted_lines.max((bounds.height / char_h).ceil().max(0.0) as usize);
            for row in visible_cell_range(band_row, lines, height) {
                rows[row].push(PainterKey::Volatile);
            }
        }

        // Child frames (and their borders) paint over the root with no
        // identity of their own; the full path paints them.
        for child in children {
            let top = child.frame_placement.outer_in_parent().y().round() as i64;
            for row in visible_cell_range(top.saturating_sub(1), child.frame_rows + 2, height) {
                rows[row].push(PainterKey::Volatile);
            }
        }

        // Window-matrix rows (`rasterize_state_at`), by identity and placement.
        let mut has_shift = false;
        for entry in &state.window_matrices {
            for (row_idx, glyph_row) in entry.matrix.rows.iter().enumerate() {
                if let RowDamage::ReusedShifted { dvpos } = entry.matrix.row_damage(row_idx)
                    && char_h == 1.0
                    && dvpos.get().round() as isize != 0
                {
                    has_shift = true;
                }
                if !glyph_row.enabled {
                    // `rasterize_glyph_row` writes nothing for it.
                    continue;
                }
                let placement =
                    WindowRowPlacement::new(state, entry, glyph_row.role, row_idx, 0, 0);
                let Some(grid_row) = visible_cell(placement.grid_row, height) else {
                    continue;
                };
                rows[grid_row].push(PainterKey::WindowRow {
                    window: entry.window_id,
                    row: std::ptr::from_ref::<GlyphRow>(glyph_row) as usize,
                    col: placement.row_col,
                    area: placement.area_layout,
                });
                retained.push(glyph_row.clone());
            }
        }

        self.damage.next_rows = rows;
        self.damage.next_retained = retained;
        self.damage.next_frame_key = Some(FrameKey {
            width,
            height,
            char_width: state.char_width.to_bits(),
            char_height: state.char_height.to_bits(),
            default_bg: self.default_bg,
            default_fg: self.default_fg,
            faces_generation: self.damage.faces_generation,
            caps: self.caps,
        });
        has_shift
    }

    /// Mark the rows whose painters changed; every row when the screen
    /// model's painters or frame key are unknown or different. Returns
    /// whether few enough rows changed for a damage frame: fewer than a
    /// quarter (more may be a scroll, which only the full path detects).
    fn mark_touched_rows(&mut self) -> bool {
        let height = self.desired.height;
        let mut touched = std::mem::take(&mut self.damage.touched);
        touched.clear();
        let comparable = self.damage.frame_key.is_some()
            && self.damage.frame_key == self.damage.next_frame_key
            && self.damage.next_rows.len() == height;
        match self.damage.rows.as_ref() {
            Some(previous) if comparable && previous.len() == height => touched.extend(
                previous
                    .iter()
                    .zip(&self.damage.next_rows)
                    .map(|(before, now)| before != now),
            ),
            _ => touched.resize(height, true),
        }
        let count = touched.iter().filter(|touched| **touched).count();
        self.damage.touched = touched;
        comparable && count * 4 < height
    }

    /// Whether the full path may carry GRID_ROW from the screen model instead
    /// of rasterizing it (see [`DamageState::legacy_carry`]).
    #[inline]
    pub(super) fn carry_permitted(&self, grid_row: usize) -> bool {
        self.damage.legacy_carry || !self.damage.touched.get(grid_row).copied().unwrap_or(true)
    }

    /// Paint the touched rows of the root frame: clear them, then run every
    /// painter over them, in the full path's order.
    fn rasterize_root_damaged(&mut self, state: &FrameDisplayState, touched: &[bool]) {
        for (row, _) in touched.iter().enumerate().filter(|(_, touched)| **touched) {
            self.desired.clear_row(row, self.default_bg);
        }
        if std::env::var_os("NEOMACS_DUMP_TTY_GLYPHS").is_some() {
            self.dump_frame_display_state_to_log(state, 0, 0);
        }
        self.install_state_cursor(state, 0, 0);
        for fill in &state.face_fills {
            self.rasterize_face_fill(0, 0, state, fill, Some(touched));
        }
        // Band rows are volatile, hence always touched.
        self.rasterize_frame_chrome_bands(state, 0, 0);
        let char_w = state.char_width.max(1.0);
        let height = self.desired.height;
        for entry in &state.window_matrices {
            for (row_idx, glyph_row) in entry.matrix.rows.iter().enumerate() {
                let placement =
                    WindowRowPlacement::new(state, entry, glyph_row.role, row_idx, 0, 0);
                if !visible_cell(placement.grid_row, height).is_some_and(|row| touched[row]) {
                    continue;
                }
                self.rasterize_glyph_row(
                    state,
                    TtyGlyphRowOwner::Window(entry.window_id),
                    0,
                    0,
                    placement.row_col,
                    placement.grid_row,
                    glyph_row,
                    placement.area_layout,
                    char_w,
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // B2: plan and commit
    // -----------------------------------------------------------------------

    /// [`Self::plan_frame`] for a damage frame: the touched rows only, with
    /// no scroll detection.
    pub(super) fn plan_damage_frame(&mut self) -> Vec<TermOp> {
        let mut ops = Vec::new();
        self.scroll_seed = None;
        if self.desired.width == 0 || self.desired.height == 0 {
            return ops;
        }
        let touched = std::mem::take(&mut self.damage.touched);
        let width = self.desired.width;
        if !matches!(self.caps.blank_tail, BlankTailMethod::WriteSpaces) {
            for (row, _) in touched.iter().enumerate().filter(|(_, touched)| **touched) {
                normalize_row_blank_tail(
                    &mut self.desired.cells[row * width..(row + 1) * width],
                    self.caps.blank_tail,
                );
            }
        }
        for (row, _) in touched.iter().enumerate().filter(|(_, touched)| **touched) {
            self.plan_row(row, &[], &mut ops);
        }
        self.damage.touched = touched;
        ops
    }

    /// Make the planned frame the screen model: swap the grids after a full
    /// frame, copy the touched rows after a damage frame. The frame's
    /// painter keys become those of the screen model.
    pub(super) fn commit_frame(&mut self) {
        self.promote_row_signatures();
        match self.damage.frame {
            FrameKind::Full => std::mem::swap(&mut self.current, &mut self.desired),
            FrameKind::Damage => {
                let width = self.desired.width;
                for (row, _) in self
                    .damage
                    .touched
                    .iter()
                    .enumerate()
                    .filter(|(_, touched)| **touched)
                {
                    let range = row * width..(row + 1) * width;
                    self.current.cells[range.clone()].clone_from_slice(&self.desired.cells[range]);
                }
            }
        }
        self.frame_stats.damage_frame = self.damage.frame == FrameKind::Damage;
        self.frame_stats.full_reason = self.damage.full_reason.take();
        self.frame_stats.rows_repainted = match self.damage.frame {
            FrameKind::Full => self.desired.height as u32,
            FrameKind::Damage => self.damage.touched.iter().filter(|t| **t).count() as u32,
        };
        // A diff without a new rasterize (tests fill `desired` by hand) is a
        // full-grid diff whose painters nobody recorded.
        self.damage.frame = FrameKind::Full;
        match self.damage.next_frame_key.take() {
            Some(key) if self.damage.mode != TtyDamageMode::Off => {
                self.damage.frame_key = Some(key);
                let rows = std::mem::take(&mut self.damage.next_rows);
                // Reuse the old rows' allocations for the next frame.
                self.damage.next_rows = self.damage.rows.replace(rows).unwrap_or_default();
                std::mem::swap(&mut self.damage.retained, &mut self.damage.next_retained);
            }
            _ => self.damage.forget_painters(),
        }
    }

    /// B3: after the frame, a row's signature stays known unless an
    /// operation or a damage frame rewrites it; an unplanned frame (the
    /// painter path) forgets them all.
    fn promote_row_signatures(&mut self) {
        let op_rows_valid = std::mem::take(&mut self.damage.op_rows_valid);
        self.frame_stats.row_signatures_reused = std::mem::take(&mut self.damage.signatures.reused);
        if self.damage.mode == TtyDamageMode::Off {
            return;
        }
        let height = self.desired.height;
        let damage = &mut self.damage;
        damage.signatures.current.resize(height, None);
        match damage.frame {
            FrameKind::Full => {
                for row in 0..height {
                    let written =
                        !op_rows_valid || damage.op_rows.get(row).copied().unwrap_or(true);
                    damage.signatures.current[row] = if written {
                        None
                    } else {
                        damage.signatures.desired.get(row).copied().flatten()
                    };
                }
            }
            FrameKind::Damage => {
                for (row, touched) in damage.touched.iter().enumerate() {
                    if *touched && row < height {
                        damage.signatures.current[row] = None;
                    }
                }
            }
        }
        damage.signatures.desired.clear();
    }

    // -----------------------------------------------------------------------
    // B2: verify
    // -----------------------------------------------------------------------

    /// Record which rows this frame's plan writes (`on` and `verify`).
    pub(super) fn note_op_rows(&mut self, ops: &[TermOp]) {
        self.damage.op_rows_valid = true;
        let height = self.desired.height;
        let rows = &mut self.damage.op_rows;
        rows.clear();
        rows.resize(height, false);
        for op in ops {
            let span = match *op {
                TermOp::ScrollRows { top, bottom, .. } => top as usize..=bottom as usize,
                TermOp::WriteRun { row, .. }
                | TermOp::EraseToEol { row, .. }
                | TermOp::InsertCells { row, .. }
                | TermOp::DeleteCells { row, .. } => row as usize..=row as usize,
            };
            for row in span.filter(|row| *row < height) {
                rows[row] = true;
            }
        }
    }

    /// Render the damage path on the shadow after the full path rendered
    /// this frame, and count where they disagree.
    pub(super) fn finish_verified_frame(&mut self) {
        let Some(mut shadow) = self.damage.shadow.take() else {
            return;
        };
        let damage_frame = shadow.damage.frame == FrameKind::Damage;
        let touched = shadow.damage.touched.clone();
        shadow.diff_and_render();
        let mut frame = TtyDamageVerifyTotals {
            frames: 1,
            damage_frames: u64::from(damage_frame),
            ..TtyDamageVerifyTotals::default()
        };
        let comparable = self.current.width == shadow.current.width
            && self.current.height == shadow.current.height;
        for row in 0..self.current.height.min(shadow.current.height) {
            let skipped = damage_frame && !touched.get(row).copied().unwrap_or(false);
            let (full, damage) = (self.current.row(row), shadow.current.row(row));
            let same_content = comparable && cells_same_content(full, damage);
            if !same_content {
                frame.screen_diff_rows += 1;
                frame.false_negatives += u64::from(skipped);
            } else if full != damage {
                frame.materialization_diff_rows += 1;
            }
            if skipped && same_content && self.damage.op_rows.get(row).copied().unwrap_or(false) {
                frame.redundant_rewrite_rows += 1;
            }
        }
        frame.byte_diff_frames = u64::from(self.output != shadow.output);
        let totals = &mut self.damage.verify;
        totals.frames += frame.frames;
        totals.damage_frames += frame.damage_frames;
        totals.false_negatives += frame.false_negatives;
        totals.screen_diff_rows += frame.screen_diff_rows;
        totals.materialization_diff_rows += frame.materialization_diff_rows;
        totals.redundant_rewrite_rows += frame.redundant_rewrite_rows;
        totals.byte_diff_frames += frame.byte_diff_frames;
        self.frame_stats.verify = frame;
        if frame.false_negatives + frame.screen_diff_rows > 0 {
            tracing::error!(
                target: "neomacs::tty_damage",
                frame = totals.frames,
                damage_frame,
                false_negatives = frame.false_negatives,
                screen_diff_rows = frame.screen_diff_rows,
                "NEOMACS_TTY_DAMAGE=verify: the damage path left a different screen"
            );
        } else if frame.byte_diff_frames > 0 {
            tracing::debug!(
                target: "neomacs::tty_damage",
                frame = totals.frames,
                redundant_rewrite_rows = frame.redundant_rewrite_rows,
                materialization_diff_rows = frame.materialization_diff_rows,
                full_bytes = self.output.len(),
                damage_bytes = shadow.output.len(),
                "NEOMACS_TTY_DAMAGE=verify: same screen, different bytes"
            );
        }
        if let Some(path) = std::env::var_os("NEOMACS_TTY_DAMAGE_REPORT_FILE")
            && !path.is_empty()
        {
            use std::io::Write as _;
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                let _ = writeln!(
                    file,
                    "frame={} kind={} touched={} false_negatives={} screen_diff_rows={} materialization_diff_rows={} redundant_rewrite_rows={} byte_diff={} full_bytes={} damage_bytes={}",
                    totals.frames,
                    if damage_frame { "damage" } else { "full" },
                    touched.iter().filter(|touched| **touched).count(),
                    frame.false_negatives,
                    frame.screen_diff_rows,
                    frame.materialization_diff_rows,
                    frame.redundant_rewrite_rows,
                    frame.byte_diff_frames,
                    self.output.len(),
                    shadow.output.len(),
                );
            }
        }
    }

    /// The painter path renders no shadow: drop it (and the knob's frame kind
    /// is honoured by `paint` itself).
    pub(super) fn drop_verify_shadow(&mut self) {
        self.damage.shadow = None;
    }

    /// Whether ROW is planned this frame (`paint`).
    pub(super) fn row_planned(&self, row: usize) -> bool {
        self.damage.frame == FrameKind::Full
            || self.damage.touched.get(row).copied().unwrap_or(true)
    }

    /// Mark the planned rows' cells written (`paint`).
    pub(super) fn mark_planned_rows_written(&mut self) {
        let width = self.desired.width;
        for row in 0..self.desired.height {
            if self.row_planned(row) {
                for cell in &mut self.desired.cells[row * width..(row + 1) * width] {
                    cell.materialization = CellMaterialization::Written;
                }
            }
        }
    }

    /// A failed write leaves the terminal unknown.
    pub(super) fn forget_painted_state(&mut self) {
        self.damage.forget_painters();
        self.damage.frame = FrameKind::Full;
    }
}

/// How two renderers' screen models compare ([`TtyRif::compare_screen`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenMatch {
    /// Every cell equal, erased/written materialization included.
    Identical,
    /// Every cell shows the same thing; some materialization differs.
    SameContent,
    /// Some cell shows something else (or the grids differ in size).
    Different,
}

impl TtyRif {
    /// Compare this renderer's screen model with OTHER's.
    pub fn compare_screen(&self, other: &TtyRif) -> ScreenMatch {
        if self.current.width != other.current.width || self.current.height != other.current.height
        {
            return ScreenMatch::Different;
        }
        if self.current.cells == other.current.cells {
            return ScreenMatch::Identical;
        }
        if cells_same_content(&self.current.cells, &other.current.cells) {
            ScreenMatch::SameContent
        } else {
            ScreenMatch::Different
        }
    }
}

#[cfg(test)]
#[path = "damage/tests/damage_test.rs"]
mod tests;
