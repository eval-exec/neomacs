//! Window state records owned by the evaluator layer (stage-4 W1 E2).

use super::layout_input::WindowDisplaySnapshotFreshness;
use neomacs_display_protocol::PresentedWindowChromeArea;
use neomacs_display_protocol::TransitionDirection;
use neomacs_display_protocol::frame_glyphs::PresentedWindowRegions;
use neomacs_display_protocol::posn_frame_pool::PosnFramePool;
use neomacs_display_protocol::posn_object_extent::PosnPointRole as DisplayPointRole;
use neovm_edit::buffer::{
    BufferId, BufferManager, CharLen, CharPos0, EmacsByteLen, EmacsBytePos, LispCharPos1,
    MarkerRef, TextPositionAnchor,
};
use neovm_edit::emacs_core::display_metrics::{Rect, WindowId, WindowMargins};
use neovm_value::display_vocab::{
    ChangeStamp, FRAME_ID_BASE, FrameId, FrameVisibility, SplitDirection, WindowHistoryState,
    WindowLayoutVariable,
};
use neovm_value::gc_trace::GcTrace;
use neovm_value::intern::SymId;
use neovm_value::value::Value;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use rustc_hash::FxHashMap as HashMap;
use std::marker::PhantomData;
use std::num::NonZeroU64;
use std::sync::{Arc, OnceLock};

/// Redisplay-owned runtime state used to decide which GNU window hooks fire.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowHookSnapshot {
    /// Buffer currently shown in the window.
    pub buffer_id: BufferId,
    /// Last known live bounds of the window.
    pub bounds: Rect,
}

/// Per-frame redisplay record for GNU window change hook ownership.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameWindowHookRecord {
    /// Last known live windows on the frame.
    pub windows: HashMap<WindowId, WindowHookSnapshot>,
    /// Selected window the last time window change hooks were recorded.
    pub selected_window: Option<WindowId>,
    /// Whether this frame was the selected frame at last record time.
    pub was_selected_frame: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowBufferHistoryChange {
    pub outgoing_buffer_id: BufferId,
    pub incoming_buffer_id: BufferId,
    pub outgoing_window_start: LispCharPos1,
    pub outgoing_window_point: LispCharPos1,
}

/// Transient VM-owned goal, retained across consecutive scroll commands.
/// Not a Lisp object or persisted window configuration.
#[derive(Clone, Copy, Debug)]
pub struct ScrollGoal {
    pub frame: FrameId,
    pub window: WindowId,
    pub buffer: BufferId,
    pub x: i64,
    pub y: i64,
}

/// The set of windows whose chrome must be re-generated.
#[derive(Debug, Default, Clone)]
pub struct ChromeDirty {
    /// GNU `update_mode_lines`, as a generation: every window is dirty until
    /// it acknowledges this value. Bumping it is the "all windows" mark.
    all_generation: u64,
    /// GNU `w->update_mode_line`: these windows specifically.
    windows: std::collections::HashSet<WindowId>,
    /// The `all_generation` each window last generated chrome at. A window
    /// with no entry has never generated chrome and is therefore dirty.
    acknowledged: std::collections::HashMap<WindowId, u64>,
}

impl ChromeDirty {
    /// GNU `bset_update_mode_line` (`xdisp.c:901-907`): a buffer-scoped event
    /// raises the global flag, because the buffer may be shown in windows
    /// this call cannot enumerate.
    pub fn mark_all(&mut self) {
        self.all_generation = self.all_generation.wrapping_add(1);
    }

    /// GNU `wset_update_mode_line` (`xdisp.c:909-920`): a window-scoped event.
    pub fn mark_window(&mut self, window: WindowId) {
        self.windows.insert(window);
    }

    /// The current `update_mode_lines` generation.  A consumer that recorded
    /// it when it produced something can tell whether GNU's global flag was
    /// raised since (`try_cursor_movement` refuses while it is set).
    pub fn all_generation(&self) -> u64 {
        self.all_generation
    }

    /// Whether WINDOW carries its own mark (GNU `w->update_mode_line`).
    pub fn is_window_marked(&self, window: WindowId) -> bool {
        self.windows.contains(&window)
    }

    /// Whether WINDOW must re-generate its chrome this redisplay.
    pub fn is_dirty(&self, window: WindowId) -> bool {
        self.windows.contains(&window)
            || self.acknowledged.get(&window) != Some(&self.all_generation)
    }

    /// Whether any window known to this set is still outstanding.
    ///
    /// "Known" is the honest qualifier: a window that has never generated
    /// chrome has no acknowledgement entry and cannot be enumerated here, so
    /// this answers the question for windows redisplay has already visited.
    /// [`Self::is_dirty`] is the per-window authority and does treat an
    /// unknown window as dirty.
    pub fn is_any_dirty(&self) -> bool {
        !self.windows.is_empty()
            || self
                .acknowledged
                .values()
                .any(|generation| *generation != self.all_generation)
    }

    /// Called by redisplay for each window whose chrome it actually
    /// generated. GNU's analogue is `mark_window_display_accurate_1` clearing
    /// `w->update_mode_line`, plus `update_mode_lines` being reset once
    /// `redisplay_internal` has made a pass.
    ///
    /// A window that SKIPPED its chrome must not be passed here — its flag has
    /// not been honored and has to survive into the next redisplay.
    pub fn note_chrome_generated(&mut self, window: WindowId) {
        self.windows.remove(&window);
        self.acknowledged.insert(window, self.all_generation);
    }

    /// Drop a deleted window's acknowledgement so the table does not grow
    /// without bound across a session's window churn.
    pub fn forget_window(&mut self, window: WindowId) {
        self.windows.remove(&window);
        self.acknowledged.remove(&window);
    }
}

/// GNU's three window-argument domains, as a closed set.
///
/// Every subr that takes a WINDOW names one of GNU's decoders, and the decoder
/// fixes BOTH which windows are admitted and which predicate a rejection
/// reports.  Keeping them together is the point: the predicate used to be a
/// `&str` passed alongside a hard-coded lookup, so
/// `validate_optional_window_designator_in_state(.., WindowDomain::Live)` reported
/// `window-live-p` while still admitting internal windows -- a decoder that
/// lied about its own contract.  Here the predicate is derived from the variant
/// and the lookup is a method on it, so the two cannot drift, and adding a
/// domain is a compile error at every site rather than a silent default.
#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::IntoStaticStr)]
pub enum WindowDomain {
    /// `decode_live_window` / `CHECK_LIVE_WINDOW`: a live leaf window only.
    #[strum(serialize = "window-live-p")]
    Live,
    /// `decode_valid_window` / `CHECK_VALID_WINDOW`: live OR internal.
    #[strum(serialize = "window-valid-p")]
    Valid,
    /// `decode_any_window` / `CHECK_WINDOW`: any window object, deleted included.
    #[strum(serialize = "windowp")]
    Any,
}

impl WindowDomain {
    /// The symbol a rejection reports, derived from the variant rather than
    /// carried beside it.
    pub fn predicate(self) -> &'static str {
        self.into()
    }
}

/// Marker handles have distinct roles in GNU's live-window invariant. Keeping
/// those roles as separate types prevents accidentally moving (for example)
/// `w->start` when a caller intended to move `w->pointm`.
///
/// Each handle owns both the internal chain ID and the Lisp marker value that
/// roots its `MarkerObj` for precise GC. A numeric ID alone is not ownership:
/// the buffer's intrusive marker chain is deliberately weak and GC unlinks
/// unmarked entries before sweeping them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowStartMarker {
    marker: MarkerRef,
    gc_root: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowPointMarker {
    marker: MarkerRef,
    gc_root: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowOldPointMarker {
    marker: MarkerRef,
    gc_root: Value,
}

/// The complete marker set owned by one live leaf window.
///
/// The fields are intentionally private: construction is atomic through the
/// window-marker lifecycle module, so a window cannot represent a partially
/// attached `(start, point, old-point)` marker set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttachedWindowPositionMarkers {
    start: WindowStartMarker,
    point: WindowPointMarker,
    old_point: WindowOldPointMarker,
}

/// Marker lifecycle for a leaf window.
///
/// Fresh structural nodes may be detached while their buffer is being chosen;
/// every live frame/window factory must transition them to `Attached` before
/// publishing the window.  One enum replaces three independent `Option`s and
/// makes all partial attachment states unrepresentable.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowPositionMarkerState {
    #[default]
    Detached,
    Attached(AttachedWindowPositionMarkers),
}

impl WindowPositionMarkerState {
    pub const fn is_attached(self) -> bool {
        matches!(self, Self::Attached(_))
    }

    pub fn attached(self) -> Option<AttachedWindowPositionMarkers> {
        match self {
            Self::Detached => None,
            Self::Attached(markers) => Some(markers),
        }
    }

    pub fn detach(&mut self) -> Option<AttachedWindowPositionMarkers> {
        std::mem::take(self).attached()
    }

    pub fn trace_roots(self, roots: &mut Vec<Value>) {
        if let Self::Attached(markers) = self {
            markers.trace_roots(roots);
        }
    }
}

impl AttachedWindowPositionMarkers {
    pub fn new(
        start: (MarkerRef, Value),
        point: (MarkerRef, Value),
        old_point: (MarkerRef, Value),
    ) -> Self {
        Self {
            start: WindowStartMarker {
                marker: start.0,
                gc_root: start.1,
            },
            point: WindowPointMarker {
                marker: point.0,
                gc_root: point.1,
            },
            old_point: WindowOldPointMarker {
                marker: old_point.0,
                gc_root: old_point.1,
            },
        }
    }

    pub fn trace_roots(self, roots: &mut Vec<Value>) {
        roots.push(self.start.gc_root);
        roots.push(self.point.gc_root);
        roots.push(self.old_point.gc_root);
    }
}

impl WindowStartMarker {
    pub const fn raw(self) -> MarkerRef {
        self.marker
    }

    /// The rooted Lisp marker, whose `MarkerObj` carries the position.
    pub const fn root(self) -> Value {
        self.gc_root
    }
}

impl WindowPointMarker {
    pub const fn raw(self) -> MarkerRef {
        self.marker
    }

    pub const fn root(self) -> Value {
        self.gc_root
    }
}

impl WindowOldPointMarker {
    pub const fn raw(self) -> MarkerRef {
        self.marker
    }

    pub const fn root(self) -> Value {
        self.gc_root
    }
}

impl AttachedWindowPositionMarkers {
    pub const fn start(self) -> WindowStartMarker {
        self.start
    }

    pub const fn point(self) -> WindowPointMarker {
        self.point
    }

    pub const fn old_point(self) -> WindowOldPointMarker {
        self.old_point
    }
}

/// Frame inputs that can change display-row geometry.
///
/// Redisplay skipping and retained-row freshness both consume this projection,
/// so a new frame layout input cannot be added to one cache policy while being
/// forgotten by the other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameLayoutInputState {
    pub id: FrameId,
    pub geometry: (u32, u32, u32, u32, u32, u32),
    pub device_scale_bits: u64,
    pub visible: bool,
    pub displays_chrome: bool,
    pub has_window_system: bool,
    pub window_system_symbol: Option<SymId>,
}

/// Existing evaluator-thread revision for direct character-table writes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CharTableLayoutRevision(u64);

impl CharTableLayoutRevision {
    /// Wrap one evaluator-thread char-table mutation counter value.
    pub const fn from_counter(counter: u64) -> Self {
        Self(counter)
    }
}

/// Window inputs that can change display-row geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowLayoutInputState {
    pub id: WindowId,
    pub buffer_id: BufferId,
    pub bounds: (u32, u32, u32, u32),
    pub window_start: LispCharPos1,
    pub point: LispCharPos1,
    pub hscroll: usize,
    pub vscroll: i32,
    pub preserve_vscroll_p: bool,
    pub margins: WindowMargins,
    pub display_table_identity: usize,
    pub char_table_mutation_epoch: CharTableLayoutRevision,
    pub window_parameters_generation: u64,
    pub left_fringe_width: i32,
    pub right_fringe_width: i32,
    pub fringes_outside_margins: bool,
    pub left_scroll_bar_width: i64,
    pub right_scroll_bar_width: i64,
    pub horizontal_scroll_bar_height: i64,
}

/// Zero-based glyph-matrix row coordinate.
///
/// This deliberately does not accept or expose buffer positions, display
/// columns, or pixel coordinates. Those domains have separate types.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct MatrixRow0(usize);

impl MatrixRow0 {
    pub const ZERO: Self = Self(0);

    pub const fn new(row: usize) -> Self {
        Self(row)
    }

    pub const fn get(self) -> usize {
        self.0
    }
}

/// One atomically published GNU-compatible window-end record.
///
/// GNU stores both distances from buffer Z plus the matrix row that produced
/// them. Keeping the tuple together prevents partial char/byte/row updates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WindowEndRecord {
    char_offset_from_z: CharLen,
    byte_offset_from_z: EmacsByteLen,
    matrix_row: MatrixRow0,
}

impl WindowEndRecord {
    pub fn from_anchors(
        buffer_z: TextPositionAnchor,
        end: TextPositionAnchor,
        matrix_row: MatrixRow0,
    ) -> Self {
        Self {
            char_offset_from_z: buffer_z
                .char_pos()
                .saturating_offset_from(end.char_pos().min(buffer_z.char_pos())),
            byte_offset_from_z: buffer_z
                .emacs_byte_pos()
                .saturating_offset_from(end.emacs_byte_pos().min(buffer_z.emacs_byte_pos())),
            matrix_row,
        }
    }

    pub fn from_positions(
        buffer_z_char: LispCharPos1,
        buffer_z_byte: EmacsBytePos,
        end_charpos: LispCharPos1,
        end_bytepos: EmacsBytePos,
        matrix_row: MatrixRow0,
    ) -> Self {
        Self::from_anchors(
            TextPositionAnchor::new(CharPos0::from_lisp(buffer_z_char), buffer_z_byte),
            TextPositionAnchor::new(CharPos0::from_lisp(end_charpos), end_bytepos),
            matrix_row,
        )
    }

    pub const fn char_offset_from_z(self) -> CharLen {
        self.char_offset_from_z
    }

    pub const fn byte_offset_from_z(self) -> EmacsByteLen {
        self.byte_offset_from_z
    }

    pub const fn matrix_row(self) -> MatrixRow0 {
        self.matrix_row
    }

    /// Reindex a complete measured suffix while preserving its exact source end.
    /// This creates query geometry; it does not publish a retained window end.
    pub const fn with_matrix_row(self, matrix_row: MatrixRow0) -> Self {
        Self { matrix_row, ..self }
    }

    /// Recover GNU's Lisp-visible end position from the current buffer Z.
    pub fn charpos_from_z(self, buffer_z: LispCharPos1) -> LispCharPos1 {
        let buffer_z = buffer_z.to_one_based_usize();
        LispCharPos1::from_one_based_usize(
            buffer_z
                .saturating_sub(self.char_offset_from_z.get())
                .max(1),
        )
    }

    /// Recover the byte companion of [`Self::charpos_from_z`].
    pub const fn bytepos_from_z(self, buffer_z_byte: EmacsBytePos) -> EmacsBytePos {
        buffer_z_byte.saturating_sub_len(self.byte_offset_from_z)
    }
}

/// Presentation lifecycle of a leaf window's last complete end record.
///
/// `Stale` retains GNU's `UPDATE=nil` behavior while making it impossible to
/// mark a partially populated record current.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WindowEndState {
    #[default]
    Unrecorded,
    Stale(WindowEndRecord),
    Current(WindowEndRecord),
}

impl WindowEndState {
    pub const fn record(self) -> Option<WindowEndRecord> {
        match self {
            Self::Unrecorded => None,
            Self::Stale(record) | Self::Current(record) => Some(record),
        }
    }

    pub const fn is_current(self) -> bool {
        matches!(self, Self::Current(_))
    }

    /// Recover the last recorded end, treating an unrecorded offset as GNU's
    /// zero-initialized distance from Z.
    pub fn charpos_from_z(self, buffer_z: LispCharPos1) -> LispCharPos1 {
        self.record()
            .map_or(buffer_z, |record| record.charpos_from_z(buffer_z))
    }

    /// Recover the byte companion of [`Self::charpos_from_z`].
    pub fn bytepos_from_z(self, buffer_z_byte: EmacsBytePos) -> EmacsBytePos {
        self.record()
            .map_or(buffer_z_byte, |record| record.bytepos_from_z(buffer_z_byte))
    }

    pub fn invalidate(&mut self) {
        if let Self::Current(record) = *self {
            *self = Self::Stale(record);
        }
    }
}

// --- E5-FM data records (stage-4 W2/E5) ---

/// Stable route to one leaf within a particular window-tree generation.
///
/// Redisplay builds these routes once per frame attempt and then resolves each
/// leaf in O(tree depth). The target ID is checked at lookup, so a stale route
/// cannot alias a different window after a structural edit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowTreePath {
    pub(crate) generation: u64,
    pub(crate) target: WindowId,
    pub(crate) route: WindowTreeRoute,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WindowTreeRoute {
    Root(Vec<usize>),
    Minibuffer,
}

pub(crate) type WindowParameters = Vec<(Value, Value)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowRedisplayState {
    pub id: WindowId,
    pub buffer_id: BufferId,
    pub bounds: (u32, u32, u32, u32),
    pub window_start: LispCharPos1,
    pub window_end: WindowEndState,
    pub point: LispCharPos1,
    pub old_point: LispCharPos1,
    pub hscroll: usize,
    pub vscroll: i32,
    pub preserve_vscroll_p: bool,
}

/// Canonical effective values of the closed window-layout variable set.
///
/// Unlike redisplay mutation counters, this projection is unchanged when Lisp
/// temporarily binds a display variable and restores it before returning.
/// Persistent changes remain visible as exact tagged-value identities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct WindowLayoutValueIdentity(usize);

impl WindowLayoutValueIdentity {
    pub(crate) fn of(value: Value) -> Self {
        Self(value.bits())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WindowLayoutVariableState {
    pub(crate) values:
        [Option<WindowLayoutValueIdentity>; <WindowLayoutVariable as strum::EnumCount>::COUNT],
}

/// Selection inputs that affect active/inactive chrome and cursor layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WindowLayoutSelectionState {
    pub(crate) selected_frame: Option<FrameId>,
    pub(crate) frame_selected_window: WindowId,
    pub(crate) minibuffer_selected_window: Option<WindowId>,
    pub(crate) active_minibuffer_window: Option<WindowId>,
}

/// How a redisplay pass arrived at the window start it published.
///
/// GNU `redisplay_window` calls `run_window_scroll_functions`
/// (src/xdisp.c:19222) exactly at the sites where it commits a start rather
/// than reusing the one it was handed: the `w->force_start` branch
/// (src/xdisp.c:20728), the `try_scrolling` result (src/xdisp.c:19645), and
/// the recenter fallback (src/xdisp.c:21227). Everything else — the
/// optimizations and a `try_window` that simply worked from the existing start
/// — reaches none of them. Making that an enum keeps the hook from becoming an
/// opt-in each publication site has to remember.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum WindowStartCommit {
    /// Redisplay reused the start it was handed; GNU runs no scroll hook.
    Inherited,
    /// Redisplay committed a start (forced, scrolled, or recentered);
    /// GNU runs `window-scroll-functions` for it.
    Committed,
}

impl WindowStartCommit {
    /// `forced` is GNU `w->force_start` — its branch runs the hook even when
    /// the forced start equals the previous one. `moved` covers the
    /// `try_scrolling` and recenter sites, which are only reached because the
    /// start changed.
    pub const fn of(forced: bool, moved: bool) -> Self {
        if forced || moved {
            Self::Committed
        } else {
            Self::Inherited
        }
    }

    pub const fn runs_window_scroll_functions(self) -> bool {
        matches!(self, Self::Committed)
    }
}

/// Character-grid dimensions assigned by GNU's pixel-to-total pass.
/// Pixel mutations invalidate only the affected axis; until the next grid
/// assignment, callers derive its initial extent from the frame's metrics.
#[derive(Debug, Clone, Default)]
pub struct WindowCellDimensions {
    pub(crate) columns: Option<i64>,
    pub(crate) lines: Option<i64>,
}

/// GNU's positive numeric line base. This is live semantic window state:
/// nested format-mode-line calls update the same value. No Lisp roots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct ModeLineBase {
    position: CharPos0,
    line: std::num::NonZeroUsize,
}

static_assertions::assert_impl_all!(ModeLineBase: Send, Sync, Copy, std::fmt::Debug);
static_assertions::assert_eq_size!(ModeLineBase, Option<ModeLineBase>);

impl ModeLineBase {
    pub(crate) const fn first(position: CharPos0) -> Self {
        Self {
            position,
            line: std::num::NonZeroUsize::MIN,
        }
    }

    /// A nonpositive GNU numeric base is unusable on the next decode.
    pub(crate) fn new(position: CharPos0, line: usize) -> Option<Self> {
        std::num::NonZeroUsize::new(line).map(|line| Self { position, line })
    }

    pub(crate) const fn position(self) -> CharPos0 {
        self.position
    }

    pub(crate) const fn line_number(self) -> usize {
        self.line.get()
    }
}

// --- E5-FM data records, part b (stage-4 W2/E5) ---

/// Evaluator-owned identity of one immutable displayed geometry publication.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PresentationId(NonZeroU64);

impl PresentationId {
    pub const fn new(value: u64) -> Self {
        match Self::try_new(value) {
            Some(id) => id,
            None => panic!("presentation identity must be nonzero"),
        }
    }

    pub const fn try_new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Last authoritative physical cursor kind for a window.
///
/// Mirrors GNU `enum text_cursor_kinds` for resolved physical cursor states.
/// GNU's `DEFAULT_CURSOR = -2` is intentionally absent here because
/// `phys_cursor_type` stores the cursor kind after redisplay resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(i8)]
pub enum WindowCursorKind {
    NoCursor = -1,
    FilledBox = 0,
    HollowBox = 1,
    Bar = 2,
    Hbar = 3,
}

impl WindowCursorKind {
    pub fn from_gnu_code(code: i8) -> Option<Self> {
        Self::try_from(code).ok()
    }

    pub fn gnu_code(self) -> i8 {
        self.into()
    }
}

/// Cursor position within a window's text area.
///
/// Mirrors GNU's lightweight `struct cursor_pos`; physical cursor size and
/// style live separately on `WindowCursorSnapshot`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowCursorPos {
    /// X relative to the text area's left edge, in pixels.
    pub x: i64,
    /// Y relative to the text area's top edge, in pixels.
    pub y: i64,
    /// Visual row within the window's text area.
    pub row: i64,
    /// Visual column within that row.
    pub col: i64,
}

impl WindowCursorPos {
    pub fn from_snapshot(snapshot: &WindowCursorSnapshot) -> Self {
        Self {
            x: snapshot.x,
            y: snapshot.y,
            row: snapshot.row,
            col: snapshot.col,
        }
    }
}

/// Measured extent of the selected logical glyph, independent of cursor paint.
/// Zero-width glyphs remain representable; absence means no explicit extent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LogicalCursorDimensions {
    pub width: u32,
    pub height: u32,
}

/// Last authoritative physical cursor geometry for a window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowCursorSnapshot {
    /// Physical cursor kind that redisplay emitted for this window.
    pub kind: WindowCursorKind,
    /// X relative to the text area's left edge, in pixels.
    pub x: i64,
    /// Y relative to the text area's top edge, in pixels.
    pub y: i64,
    /// Cursor width in pixels.
    pub width: i64,
    /// Cursor height in pixels.
    pub height: i64,
    /// Pixels above the baseline.
    pub ascent: i64,
    /// Visual row within the window's text area.
    pub row: i64,
    /// Visual column within that row.
    pub col: i64,
}

/// One accepted window redisplay's mutually consistent output facts.
///
/// These values all come from the same immutable snapshot and presentation
/// generation. Consumers never have to combine a new end record with an old
/// visible span or cursor.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowRedisplayOutput {
    pub(crate) generation: PresentationId,
    pub(crate) window_end: WindowEndRecord,
    pub(crate) visible_span: Option<WindowVisibleBufferSpan>,
    pub(crate) logical_cursor: Option<WindowCursorPos>,
    pub(crate) phys_cursor: Option<WindowCursorSnapshot>,
}

impl WindowRedisplayOutput {
    pub const fn generation(&self) -> PresentationId {
        self.generation
    }

    pub const fn window_end(&self) -> WindowEndRecord {
        self.window_end
    }

    pub const fn visible_span(&self) -> Option<WindowVisibleBufferSpan> {
        self.visible_span
    }

    pub const fn logical_cursor(&self) -> Option<WindowCursorPos> {
        self.logical_cursor
    }

    pub fn phys_cursor(&self) -> Option<&WindowCursorSnapshot> {
        self.phys_cursor.as_ref()
    }
}

/// Per-window display settings that GNU Emacs stores on `struct window`.
///
/// # Cursor audit follow-through
///
/// neomacs now stores GNU-like cursor state directly on the live window:
///
/// - `cursor`: intended cursor position in the latest redisplay result
/// - `output_cursor`: the nominal output position last committed by redisplay
/// - `phys_cursor`: the last physical cursor geometry emitted on screen
///
/// This mirrors GNU's `struct window` ownership model closely enough for
/// `window-cursor-info` and related stateful cursor queries. The Rust
/// redisplay path now drives this state through an explicit per-window output
/// pass before frame snapshots are published. Rust layout/status-line emission
/// advances `output_cursor` through explicit output-cursor moves, while row
/// snapshots remain published artifacts for renderer handoff.
#[derive(Clone, Debug)]
pub struct WindowDisplayState {
    /// Window-local display table; nil means inherit from the buffer/frame.
    pub display_table: Value,
    /// Window-local cursor type; t means use the buffer-local value.
    pub cursor_type: Value,
    /// Intended cursor position in the latest redisplay result.
    pub cursor: Option<WindowCursorPos>,
    /// Last physical cursor geometry produced by redisplay for this window.
    pub phys_cursor: Option<WindowCursorSnapshot>,
    /// Last nominal output position actually committed by redisplay.
    pub output_cursor: Option<WindowCursorPos>,
    /// Last physical cursor type emitted by redisplay.
    pub phys_cursor_type: WindowCursorKind,
    /// Last accepted redisplay's related query/output facts, committed together.
    pub(crate) redisplay_output: Option<WindowRedisplayOutput>,
    /// Whether the window currently owns a live physical cursor.
    pub phys_cursor_on_p: bool,
    /// Whether the cursor is hidden without invalidating the geometry.
    pub cursor_off_p: bool,
    /// Cursor visibility state committed by the last completed redisplay.
    pub last_cursor_off_p: bool,
    /// Last visual row where redisplay placed the cursor.
    pub last_cursor_vpos: i64,
    /// Raw fringe widths; `-1` means use the frame default.
    pub left_fringe_width: i32,
    pub right_fringe_width: i32,
    pub fringes_outside_margins: bool,
    pub fringes_persistent: bool,
    /// Raw scroll bar sizes; `-1` means use the frame default.
    pub scroll_bar_width: i32,
    pub vertical_scroll_bar_type: Value,
    pub scroll_bar_height: i32,
    pub horizontal_scroll_bar_type: Value,
    pub scroll_bars_persistent: bool,
}

impl Default for WindowDisplayState {
    fn default() -> Self {
        Self {
            display_table: Value::NIL,
            cursor_type: Value::T,
            cursor: None,
            phys_cursor: None,
            output_cursor: None,
            phys_cursor_type: WindowCursorKind::NoCursor,
            redisplay_output: None,
            phys_cursor_on_p: false,
            cursor_off_p: false,
            last_cursor_off_p: false,
            last_cursor_vpos: 0,
            left_fringe_width: -1,
            right_fringe_width: -1,
            fringes_outside_margins: false,
            fringes_persistent: false,
            scroll_bar_width: -1,
            vertical_scroll_bar_type: Value::T,
            scroll_bar_height: -1,
            horizontal_scroll_bar_type: Value::T,
            scroll_bars_persistent: false,
        }
    }
}

impl WindowDisplayState {
    pub const fn redisplay_output(&self) -> Option<&WindowRedisplayOutput> {
        self.redisplay_output.as_ref()
    }

    pub fn clear_cursor_state(&mut self) {
        self.cursor = None;
        self.clear_output_cursor_state();
        self.clear_physical_cursor_state();
    }

    /// Start a new output pass for this window.
    ///
    /// The last committed output cursor remains authoritative until redisplay
    /// emits a new cursor position for this window.
    pub(crate) fn begin_output_pass(&mut self) {
        self.cursor = None;
        self.clear_physical_cursor_state();
    }

    /// Start a new output update for a window that will actively emit rows in
    /// the current redisplay pass.
    pub(crate) fn begin_window_output_update(&mut self) {
        self.begin_output_pass();
        self.clear_output_cursor_state();
    }

    pub(crate) fn clear_output_cursor_state(&mut self) {
        self.output_cursor = None;
    }

    pub(crate) fn clear_physical_cursor_state(&mut self) {
        self.phys_cursor = None;
        self.phys_cursor_type = WindowCursorKind::NoCursor;
        self.phys_cursor_on_p = false;
    }

    pub(crate) fn install_logical_cursor(&mut self, cursor: Option<WindowCursorPos>) {
        self.cursor = cursor;
    }

    /// Move the live output cursor to a new nominal output position.
    ///
    /// This mirrors GNU's `output_cursor_to` style of update more closely
    /// than the older row-start/row-finish helpers: Rust redisplay advances
    /// output by explicit output positions, while row boundaries remain local
    /// to snapshot recording in the layout/output emitter.
    pub(crate) fn output_cursor_to(&mut self, pos: WindowCursorPos) {
        self.output_cursor = Some(pos);
    }

    pub(crate) fn apply_physical_cursor_snapshot(&mut self, cursor: Option<WindowCursorSnapshot>) {
        self.phys_cursor = cursor.clone();
        self.phys_cursor_type = cursor
            .as_ref()
            .map(|c| c.kind)
            .unwrap_or(WindowCursorKind::NoCursor);
        self.phys_cursor_on_p = cursor.is_some();
    }

    pub(crate) fn commit_completed_redisplay(&mut self) {
        self.last_cursor_off_p = self.cursor_off_p;
        if let Some(cursor) = self.phys_cursor.as_ref() {
            self.last_cursor_vpos = cursor.row;
        } else if let Some(cursor) = self.cursor.as_ref() {
            self.last_cursor_vpos = cursor.row;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowVisibleBufferSpan {
    start: LispCharPos1,
    end: LispCharPos1,
}

impl WindowVisibleBufferSpan {
    pub const fn new(start: LispCharPos1, end: LispCharPos1) -> Self {
        Self { start, end }
    }

    pub const fn start(self) -> LispCharPos1 {
        self.start
    }

    pub const fn end(self) -> LispCharPos1 {
        self.end
    }

    /// Convert the inclusive redisplay span into GNU's Lisp-visible
    /// `window-end` position.
    ///
    /// Published row ends normally identify the last displayed character, so
    /// `window-end` is the following insertion position. Redisplay also
    /// publishes Z itself for a visible EOB insertion slot; that boundary is
    /// already a Lisp position and must not be advanced past `point-max`.
    pub fn lisp_window_end(self, buffer_z: LispCharPos1) -> LispCharPos1 {
        if self.end >= buffer_z {
            buffer_z
        } else {
            LispCharPos1::new(self.end.as_i64().saturating_add(1))
        }
    }
}

// --- E5-FM Window + WindowTree (stage-4 W2/E5) ---

/// A window in the window tree.
#[derive(Clone, Debug)]
// Window nodes own their complete leaf/split state and are cloned as snapshots;
// boxing the larger leaf would add indirection throughout the hot layout path.
#[allow(clippy::large_enum_variant)]
pub enum Window {
    /// Leaf window displaying a buffer.
    Leaf {
        id: WindowId,
        buffer_id: BufferId,
        /// Cleared at GNU freshness boundaries, never restored from a saved
        /// configuration or shared with a newly cloned sibling.
        mode_line_base: Option<ModeLineBase>,
        /// Pixel bounds within the frame.
        bounds: Rect,
        /// Cached byte position of the first visible character.
        ///
        /// GNU Emacs stores this as a marker (`w->start`) so that buffer
        /// edits before the start position auto-shift it.  neomacs now
        /// maintains a real marker in `position_markers` alongside this
        /// cached Lisp-visible position.  The cache is refreshed from the marker
        /// by `sync_window_positions_from_markers` after every text edit,
        /// and on every explicit position write.  All read sites continue
        /// to use this typed cache for zero-cost reads.
        window_start: LispCharPos1,
        /// Atomic ownership state for GNU's `start`, `pointm`, and
        /// `old_pointm` markers.
        position_markers: WindowPositionMarkerState,
        /// Last atomically published window-end record and its freshness.
        window_end: WindowEndState,
        /// Cached cursor (point) position in this window.
        ///
        /// GNU Emacs stores this as a marker (`w->pointm`) so that buffer
        /// insertions before the position auto-shift it.  neomacs now
        /// maintains a real marker in `position_markers` alongside this
        /// cached Lisp-visible position.  The cache is refreshed from the marker
        /// after every text edit.  For the selected window, reads should
        /// prefer `buffer.pt` directly (GNU fast path); this cached value
        /// is authoritative for non-selected windows.
        point: LispCharPos1,
        /// Cached previous point value mirrored from GNU `w->old_pointm`.
        old_point: LispCharPos1,
        /// GNU `w->old_buffer`: the buffer this window showed when its epoch was
        /// recorded.  `None` is GNU's nil.
        old_buffer: Option<BufferId>,
        /// GNU `w->change_stamp`: the window-change epoch this window was last
        /// recorded in.  `None` is GNU's 0 -- "created since the last record",
        /// which is why a brand-new window reports no old buffer.
        change_stamp: Option<ChangeStamp>,
        /// Mirror GNU `w->dedicated`: the dedication flag value.
        /// nil = not dedicated, t = strongly dedicated,
        /// side = side-window dedication (blocks display-buffer reuse but
        /// allows switch-to-buffer / set-window-buffer).
        dedicated: Value,
        /// Lisp-visible per-window parameter alist, newest entries first.
        parameters: WindowParameters,
        /// Monotonic identity for changes to `parameters`.
        parameters_generation: u64,
        /// Live-window history state mirrored from GNU `struct window`.
        history: WindowHistoryState,
        /// Desired height in lines (for fixed windows, 0 = flexible).
        fixed_height: usize,
        /// Desired width in columns (for fixed windows, 0 = flexible).
        fixed_width: usize,
        /// Horizontal scroll offset (columns).
        hscroll: usize,
        /// Lower bound for automatic horizontal scrolling (columns).
        ///
        /// Mirrors GNU `w->min_hscroll` (`src/window.h`). Set equal to the
        /// current `hscroll` by `scroll-left`/`scroll-right` when their
        /// SET-MINIMUM argument is non-nil; the auto-hscroll pass
        /// (`hscroll_window_tree`, STEP 7) never scrolls below it. Reset to
        /// 0 on buffer switch.
        min_hscroll: usize,
        /// Whether automatic horizontal scrolling is currently suspended for
        /// this window.
        ///
        /// Mirrors GNU `w->suspend_auto_hscroll` (`src/window.h`). Set true by
        /// `set-window-hscroll` and `scroll-left`/`scroll-right` so a manual
        /// hscroll sticks instead of being recomputed every redisplay; cleared
        /// by `hscroll_window_tree` STEP 4 once window point explicitly moves,
        /// and reset to false on buffer switch.
        suspend_auto_hscroll: bool,
        /// Raw GNU `w->vscroll` value in pixels: zero or negative.
        ///
        /// Lisp-visible `window-vscroll` reports `-vscroll`, either in pixels
        /// or in canonical line units depending on the call site.
        vscroll: i32,
        /// Mirrors GNU `w->preserve_vscroll_p`.
        preserve_vscroll_p: bool,
        /// Mirrors GNU `w->force_start`: the window start was set explicitly
        /// (window_scroll / set-window-start), so the next redisplay must
        /// honor it and move POINT into the window if it ended up outside,
        /// instead of recomputing the start around point. One-shot: cleared
        /// when redisplay publishes the window's positions.
        force_start: bool,
        /// Window margins in columns.
        margins: WindowMargins,
        /// Window-local display settings mirrored from GNU `struct window`.
        display: WindowDisplayState,
        /// Pending pixel size queued by `set-window-new-pixel`. GNU
        /// stores this as `w->new_pixel`
        /// (`src/window.h:283`). Cleared by `window-resize-apply`
        /// once committed. Window audit Structural 1 in
        /// `drafts/window-system-audit.md` moved this off a
        /// thread-local HashMap onto the window struct so
        /// window-configuration save/restore round-trips it
        /// automatically.
        new_pixel: Option<i64>,
        /// Pending total (line-cell) size queued by
        /// `set-window-new-total`. GNU `w->new_total`
        /// (`src/window.h:284`).
        new_total: Option<i64>,
        cell_dimensions: WindowCellDimensions,
        /// Pending normal-size fraction queued by
        /// `set-window-new-normal`. GNU `w->new_normal`
        /// (`src/window.h:285`). Stored as a `Value` to mirror
        /// GNU's Lisp_Object slot — `Value::NIL` means "unset".
        new_normal: Value,
        /// Authoritative proportional vertical size
        /// (height fraction of parent). GNU `w->normal_lines`
        /// (`src/window.h:128`). Initialized to 1.0 on the root
        /// and updated by `window-resize-apply` from
        /// `new_normal`. `(window-normal-size w nil)` returns
        /// this value. Window audit Critical 7 in
        /// `drafts/window-system-audit.md`.
        normal_lines: Value,
        /// Authoritative proportional horizontal size
        /// (width fraction of parent). GNU `w->normal_cols`
        /// (`src/window.h:129`).
        normal_cols: Value,
        /// Character-line top edge, stored separately from `bounds` (pixels).
        /// Mirrors GNU `w->top_line` (`src/window.h`). GNU maintains this in
        /// parallel with `pixel_top`; in batch a menu bar occupies 1 *line* but
        /// 0 *pixels*, so `top_line` can be nonzero while `bounds.y` is 0.
        /// Set by the resize passes (root gets `FRAME_TOP_MARGIN`).
        top_line: i64,
        /// Character-column left edge; GNU `w->left_col`. See `top_line`.
        left_col: i64,
    },

    /// Internal node: contains children split in a direction.
    Internal {
        id: WindowId,
        direction: SplitDirection,
        children: Vec<WindowId>,
        bounds: Rect,
        /// Character-line top edge; GNU `w->top_line`. See `Leaf::top_line`.
        top_line: i64,
        /// Character-column left edge; GNU `w->left_col`. See `Leaf::top_line`.
        left_col: i64,
        /// Lisp-visible per-window parameter alist, newest entries first.
        parameters: WindowParameters,
        /// Monotonic identity for changes to `parameters`.
        parameters_generation: u64,
        /// Combination limit — prevents recombination when non-nil.
        /// Mirrors GNU Emacs `w->combination_limit`.
        combination_limit: bool,
        /// Pending pixel size — see `Leaf::new_pixel`. GNU keeps
        /// the same `new_pixel` slot on every `struct window`,
        /// regardless of leaf/internal split state.
        new_pixel: Option<i64>,
        /// Pending total size — see `Leaf::new_total`.
        new_total: Option<i64>,
        cell_dimensions: WindowCellDimensions,
        /// Pending normal-size fraction — see `Leaf::new_normal`.
        new_normal: Value,
        /// Persistent normal-size fraction — see
        /// `Leaf::normal_lines`.
        normal_lines: Value,
        /// Persistent normal-size fraction — see
        /// `Leaf::normal_cols`.
        normal_cols: Value,
    },
}

/// A window tree: its nodes, and which of them is the root.
///
/// This is the ONE place that knows how a tree is stored, and the only handle
/// that can hand out a node.  Nodes live in an arena keyed by `WindowId` and
/// name their children by id, which is GNU's shape: windows there are heap
/// structs in a pointer graph (`w->parent`, `w->next`, `w->contents`), so
/// reaching one is a dereference rather than a walk from the root.
///
/// Two consequences shape every walk over a tree.  A node cannot reach its own
/// children -- only the tree can -- so a walk is written as "gather ids, then
/// visit one node at a time" ([`WindowTree::for_each_leaf_mut`],
/// [`WindowTree::for_each_node_mut`]), never as recursion through
/// `&mut Window`.  And moving a window is repointing one slot: the promotion
/// in `delete_window_in_tree` and the interposition in `split_window_in_tree`
/// each edit a child list instead of copying a subtree over another.
///
/// `Frame` is not the only owner of one.  `WindowConfigurationSnapshot` holds
/// a tree taken by `current-window-configuration` and restored later, which is
/// why the arena lives here and not on `Frame`: a detached tree has to carry
/// its own nodes.
#[derive(Debug, Clone)]
pub struct WindowTree {
    /// Every node of this tree, live or detached, keyed by its own id.
    ///
    /// GNU's windows are heap structs in a pointer graph (`w->parent`,
    /// `w->next`, `w->contents`), so reaching one is a dereference and a
    /// window that has been deleted still exists -- it just has `contents`
    /// nil.  The map is that graph: a lookup is a probe rather than a walk
    /// from the root, and a node removed from its parent's child list is
    /// still here to answer `windowp`.
    pub(crate) nodes: HashMap<WindowId, Window>,
    /// The id of the node every walk starts from.
    pub(crate) root: WindowId,
}

// --- E5-FM-3a frame records (stage-4 W2/E5) ---

use neomacs_display_protocol::GraphicalDisplayIdentity;

/// Non-graphical frames have no graphical connection. GUI frames share the
/// validated identity used to initialize their terminal.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum FrameDisplayIdentity {
    #[default]
    None,
    Graphical(GraphicalDisplayIdentity),
}

impl FrameDisplayIdentity {
    #[cfg(test)]
    pub fn wayland(display: impl Into<String>) -> Self {
        Self::Graphical(
            GraphicalDisplayIdentity::named(
                neomacs_display_protocol::GraphicalBackend::Wayland,
                display,
            )
            .expect("valid Wayland display name"),
        )
    }

    pub fn native_display(&self) -> Option<&str> {
        match self {
            Self::None => None,
            Self::Graphical(identity) => Some(identity.terminal_name()),
        }
    }

    pub fn x_display(&self) -> Option<&str> {
        match self {
            Self::None => None,
            Self::Graphical(identity) => identity.x_display(),
        }
    }
}

/// Presentation of the frame's requested menu/tool bars. Compact presentation
/// replaces their pixel bands without changing their Lisp frame parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuiBarPresentation {
    Separate,
    Compact,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingGuiResize {
    pub width_cols: i64,
    pub total_lines: i64,
    pub(crate) delivery: GuiResizeDelivery,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GuiResizeDelivery {
    Queued,
    Requested { width: u32, height: u32, wait: bool },
}

// --- E5-FM-3a snapshot/row records (stage-4 W2/E5) ---

/// Authoritative display geometry for a single visible buffer position.
///
/// These records are published by redisplay after layout so editor-side
/// queries like `posn-at-point` can answer from the actual rendered result.
/// A position can describe either a source glyph or a visible insertion
/// boundary such as end-of-buffer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayPointSnapshot {
    /// 1-based visible buffer position.
    pub buffer_pos: LispCharPos1,
    /// Whether this point is a drawn glyph or a marker column standing in for
    /// the position the walk reached there. See [`DisplayPointRole`].
    pub role: DisplayPointRole,
    /// X relative to the text area's left edge, in pixels.
    pub x: i64,
    /// Y relative to the window's top edge, in pixels.
    pub y: i64,
    /// Rendered glyph or insertion-slot width in pixels.
    pub width: i64,
    /// Rendered glyph or insertion-slot height in pixels.
    pub height: i64,
    /// Visual row number in the window (0-based).
    pub row: i64,
    /// Visual column start for this position.
    pub col: i64,
}

/// Per-row metrics from the last redisplay of a window.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DisplayRowSnapshot {
    /// Visual row number in the window (0-based).
    pub row: i64,
    /// Y relative to the window's top edge, in pixels.
    pub y: i64,
    /// Vertical row advance in pixels, including `extra_line_spacing`.
    pub height: i64,
    /// Extra spacing after this row, included in `height`. GNU recentering
    /// excludes the last requested row's trailing spacing when fitting bottom.
    pub extra_line_spacing: i64,
    /// X position where redisplay started emitting this row, relative to the
    /// text area's left edge.
    pub start_x: i64,
    /// Visual column where redisplay started emitting this row.
    pub start_col: i64,
    /// X position where redisplay finished emitting this row, relative to the
    /// text area's left edge.
    pub end_x: i64,
    /// Visual column where redisplay finished emitting this row.
    pub end_col: i64,
    /// First buffer position represented on this row, if any.
    pub start_buffer_pos: Option<LispCharPos1>,
    /// Last visible/source position associated with this row, if any.
    pub end_buffer_pos: Option<LispCharPos1>,
    /// Last source position skipped by right truncation, including the line
    /// terminator but never the following line. None means no truncated tail.
    /// A bounded query's final row must not implicitly own the rest of a buffer.
    pub truncated_end_buffer_pos: Option<LispCharPos1>,
    /// Whether the row ended in buffer text or in pushed display text. A
    /// newline or wrap in a display string does not consume its buffer anchor.
    pub end_source: DisplayRowEndSource,
    /// Fringe bitmaps redisplay laid out on this row.
    pub fringe: RowFringeBitmaps,
}

/// Body-local row facts emitted directly by redisplay for semantic queries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PresentedBodyRowSnapshot {
    pub output_row: i64,
    pub body_row: i64,
    pub body_y: i64,
}

/// An EOB source position does not prove that its virtual overlay tail ended.
/// Only the producer that drains all strings can publish `Complete`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EobOverlayMotionCoverage {
    #[default]
    NotObserved,
    Incomplete {
        anchor: LispCharPos1,
    },
    Complete(CompletedEobOverlayMotionExtent),
}

/// Physical distance to the end of a completely measured EOB overlay stream.
///
/// The distance is relative to the row where the first string at this anchor
/// started. It survives query-row translation and includes an empty final row
/// that the renderer need not publish. Source anchors and display-row distance
/// therefore remain separate facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompletedEobOverlayMotionExtent {
    pub(crate) anchor: LispCharPos1,
    pub(crate) following_rows: usize,
}

/// Source consumed at a row end, independent of its visible buffer anchor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DisplayRowEndSource {
    #[default]
    Buffer,
    DisplayStringNewline,
    DisplayStringWrap,
    OverlayBeforeString,
    OverlayAfterString,
}

/// The fringe bitmaps one display row carried out of the last redisplay.
///
/// Mirrors exactly the three `struct glyph_row` slots that
/// `fringe-bitmaps-at-pos` reports: `left_fringe_bitmap`,
/// `right_fringe_bitmap` and `overlay_arrow_bitmap`. The arrow is a slot of
/// its own because GNU draws it *in addition to* the left bitmap.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RowFringeBitmaps {
    /// Bitmap in the row's left fringe.
    pub left: Option<FringeBitmapIndex>,
    /// Bitmap in the row's right fringe.
    pub right: Option<FringeBitmapIndex>,
    /// Overlay arrow, drawn in the left fringe beside `left`.
    pub overlay_arrow: RowOverlayArrowBitmap,
}

/// Registry index of a fringe bitmap, as handed out by `define-fringe-bitmap`
/// and stored in a bitmap symbol's `fringe` property.
///
/// GNU uses a bare `int` with `0` meaning `NO_FRINGE_BITMAP`; the absent case
/// is `Option`/`RowOverlayArrowBitmap::Absent` here, so every value of this
/// type names a bitmap that really is registered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FringeBitmapIndex(pub u16);

/// The overlay-arrow fringe slot of a display row.
///
/// GNU packs three states into the signed `struct glyph_row::overlay_arrow_bitmap`
/// and `Ffringe_bitmaps_at_pos` decodes them as `0 => nil`, `< 0 => t`,
/// `> 0 => the bitmap's symbol` (src/fringe.c). Naming the three states makes
/// that decode an exhaustive match instead of a sign test.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RowOverlayArrowBitmap {
    /// No overlay arrow on this row (GNU `NO_FRINGE_BITMAP`).
    #[default]
    Absent,
    /// An overlay arrow is present but its bitmap was never resolved to a
    /// registry index (GNU's negative encoding, reported as `t`).
    Unresolved,
    /// An overlay arrow drawn with this bitmap.
    Bitmap(FringeBitmapIndex),
}

/// Evaluator-owned half of GNU's `(glyph->object, glyph->charpos)` pair.
///
/// The renderer-safe presentation carries the string identity and character
/// index. The string object stays in the window snapshot so input can join the
/// two halves without re-evaluating a mode/tab/header-line format. Snapshots
/// are shared with render and input threads and retained by the layout engine
/// across frames, so the object travels as a [`SharedRoot`]: its own lease
/// keeps it alive in every copy, and only the evaluator materializes it.
///
/// [`SharedRoot`]: neovm_value::tagged::transport::SharedRoot
#[derive(Clone, Debug)]
pub struct PresentedWindowChromeString {
    pub(crate) area: PresentedWindowChromeArea,
    pub(crate) string_id: neomacs_display_protocol::glyph_matrix::GlyphStringId,
    pub(crate) object: neovm_value::tagged::transport::SharedRoot,
}

/// Immutable rooted chrome sources shared across redisplay snapshots.
///
/// Cloning shares one allocation and its existing heap-identified root leases.
/// The empty collection has no allocation, and no mutable slice is exposed.
/// Formatting builds an owned vector; publication freezes it after root
/// coalescing, while unchanged windows retain this collection directly.
#[repr(transparent)]
#[derive(Clone, Debug, Default)]
pub struct PresentedWindowChromeStrings(pub(crate) Option<Arc<[PresentedWindowChromeString]>>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayPointRowsMode {
    Off,
    On,
    Verify,
}

/// Process-wide numeric policy published by OnceLock; readers share no cursor or Lisp state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PointRowIterMode {
    Off,
    On,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PointCellRole {
    Glyph,
    OverlaidMarker,
    InsertionBoundary,
    SyntheticBoundary,
}

/// The usual glyph/insertion slot: exactly sixteen bytes, with checked bounds.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointCell {
    pub(crate) position_delta: u32,
    pub(crate) x: i32,
    pub(crate) col: u16,
    pub(crate) width: u16,
    pub(crate) height: u16,
    pub(crate) y_offset: i8,
    pub(crate) role: PointCellRole,
}

#[derive(Clone)]
pub(crate) enum Cells {
    Compact(Arc<[PointCell]>),
    Wide(Arc<[DisplayPointSnapshot]>),
}

#[derive(Clone)]
pub(crate) enum PointOrder {
    /// Emission order already satisfies the exact stable index key. This proof
    /// belongs to immutable row data and needs no allocated permutation.
    Identity {
        len: usize,
    },
    Compact(Arc<[u32]>),
    Wide(Arc<[usize]>),
}

pub(crate) struct PointRowData {
    pub(crate) cells: Cells,
    pub(crate) source_order: PointOrder,
    pub(crate) x_order: PointOrder,
    pub(crate) original_y: i64,
    pub(crate) original_base: i64,
    pub(crate) source_min: i64,
    pub(crate) source_max: i64,
    pub(crate) y_min: i64,
    pub(crate) y_max: i64,
    pub(crate) max_height: i64,
    pub(crate) min_x: Option<i64>,
    pub(crate) max_x: Option<i64>,
}

/// Placement changes clone this descriptor and share its immutable cell arrays.
#[derive(Clone)]
pub struct DisplayPointRow {
    pub(crate) row: i64,
    pub(crate) y: i64,
    pub(crate) buffer_base: i64,
    pub(crate) data: Arc<PointRowData>,
}

pub struct DisplayPointRowIter<'a> {
    pub(crate) row: &'a DisplayPointRow,
    pub(crate) order: &'a PointOrder,
    pub(crate) front: usize,
    pub(crate) back: usize,
}

/// Per-window row index. Clone copies only descriptors, never the point arrays.
#[derive(Clone, Default)]
pub struct DisplayPointRows {
    /// Sorted by visual row, with at most one descriptor per row.
    pub rows: Vec<DisplayPointRow>,
}

// --- E5-FM-3a presentation geometry (stage-4 W2/E5) ---

/// A logical-pixel coordinate or extent.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct LogicalPx(pub(crate) f32);

/// A stored character-column coordinate, distinct from pixels.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Column(pub(crate) i64);

/// A stored character-line coordinate, distinct from pixels.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Line(pub(crate) i64);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FrameLogicalSpace;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WindowLocalSpace;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WindowBodySpace;

/// A logical-pixel point whose coordinate space is part of its type.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PixelPoint<Space> {
    pub(crate) x: LogicalPx,
    pub(crate) y: LogicalPx,
    pub(crate) space: PhantomData<Space>,
}

/// A finite, nonnegative-extent logical-pixel rectangle.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PixelRect<Space> {
    pub(crate) origin: PixelPoint<Space>,
    pub(crate) width: LogicalPx,
    pub(crate) height: LogicalPx,
}

/// Independent stored cell-grid origin for one window.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CellOrigin {
    pub(crate) column: Column,
    pub(crate) line: Line,
}

/// One immutable, presentation-owned publication of all evaluator window geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct PresentationGeometry {
    pub(crate) presentation: PresentationId,
    pub(crate) frame: PresentationFrame,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PresentationFrame {
    pub(crate) id: FrameId,
    pub(crate) placement: neomacs_display_protocol::PresentedFramePlacement,
    pub(crate) windows: HashMap<WindowId, PresentationWindow>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PresentationWindow {
    pub(crate) id: WindowId,
    pub(crate) cell_origin: CellOrigin,
    pub(crate) outer: PixelRect<FrameLogicalSpace>,
    pub(crate) regions: Option<WindowRegions>,
    pub(crate) positions: PresentationPositions,
    pub(crate) cursor: Option<PresentationCursor>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowRegions {
    pub(crate) outer: PixelRect<FrameLogicalSpace>,
    pub(crate) text_body: PixelRect<FrameLogicalSpace>,
    pub(crate) left_margin_columns: i64,
    pub(crate) right_margin_columns: i64,
    pub(crate) left_margin: Option<PixelRect<FrameLogicalSpace>>,
    pub(crate) right_margin: Option<PixelRect<FrameLogicalSpace>>,
    pub(crate) left_fringe: Option<PixelRect<FrameLogicalSpace>>,
    pub(crate) right_fringe: Option<PixelRect<FrameLogicalSpace>>,
    pub(crate) left_scroll_bar: Option<PixelRect<FrameLogicalSpace>>,
    pub(crate) right_scroll_bar: Option<PixelRect<FrameLogicalSpace>>,
    pub(crate) horizontal_scroll_bar: Option<PixelRect<FrameLogicalSpace>>,
    pub(crate) tab_line: Option<PixelRect<FrameLogicalSpace>>,
    pub(crate) header_line: Option<PixelRect<FrameLogicalSpace>>,
    pub(crate) mode_line: Option<PixelRect<FrameLogicalSpace>>,
    pub(crate) right_divider: Option<PixelRect<FrameLogicalSpace>>,
    pub(crate) bottom_divider: Option<PixelRect<FrameLogicalSpace>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PresentationPosition {
    pub(crate) buffer_pos: LispCharPos1,
    pub(crate) x: i64,
    pub(crate) body_y: i64,
    pub(crate) width: i64,
    pub(crate) height: i64,
    /// The line owns coordinate hits even below a shorter glyph. Keep this
    /// separate from glyph height, which position/cursor queries still need.
    pub(crate) row_height: i64,
    pub(crate) body_row: i64,
    pub(crate) col: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PresentationCursor {
    pub(crate) x: i64,
    pub(crate) body_y: i64,
    pub(crate) width: i64,
    pub(crate) height: i64,
}

/// Result of resolving a buffer position against one immutable presentation.
///
/// GNU redisplay can advance past text hidden by invisibility or a replacing
/// `display` property.  Keep that compatibility case distinct from an exact
/// glyph match so the row-boundary rule cannot be lost in a generic fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PresentedBufferPositionMatch<'a> {
    Exact(&'a PresentationPosition),
    HiddenOnRow { next: &'a PresentationPosition },
    NotVisible,
}

/// Owned numeric positions or a shared immutable source. Clones may be read
/// concurrently; deferred clones share a OnceLock-published complete vector.
/// Neither representation stores Lisp values, evaluator state, or live windows.
#[derive(Clone)]
pub(crate) enum PresentationPositions {
    Eager(Vec<PresentationPosition>),
    Deferred(Arc<DeferredPositions>),
}

/// Immutable numeric descriptors and maps with one thread-safe vector cache.
/// Concurrent readers publish once; no Lisp state or snapshot object is retained.
pub(crate) struct DeferredPositions {
    pub(crate) rows: DisplayPointRows,
    pub(crate) body_rows: HashMap<i64, PresentedBodyRowSnapshot>,
    pub(crate) row_heights: HashMap<i64, i64>,
    pub(crate) positions: OnceLock<Vec<PresentationPosition>>,
}

// --- E5-FM-3a part geometry records (stage-4 W2/E5) ---

/// One of GNU's `enum window_part` values (src/dispextern.h:216-232).
///
/// `ON_NOTHING` is deliberately absent. GNU uses it as the "no" answer of
/// `coordinates_in_window`, and every caller compares against it before using
/// the value; here that answer is `None`, so every value of this type names a
/// region that really exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WindowPart {
    Text,
    ModeLine,
    VerticalBorder,
    HeaderLine,
    TabLine,
    LeftFringe,
    RightFringe,
    LeftMargin,
    RightMargin,
    VerticalScrollBar,
    HorizontalScrollBar,
    RightDivider,
    BottomDivider,
}

/// The three window-chrome lines, which GNU answers through `mode_line_string`
/// (src/dispnew.c:6444-6519) instead of `buffer_posn_from_coords`.
///
/// They are a type of their own because they are the only parts for which GNU
/// sets `textpos = -1` (src/keyboard.c:5900), which is what makes a chrome
/// posn's `posn-point` nil.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WindowChromeLine {
    TabLine,
    HeaderLine,
    ModeLine,
}

/// Everything a posn's AREA slot can name.
///
/// GNU fills it from two different places: `make_lispy_position`'s window
/// branch, which is [`WindowPart`], and its frame-bar special cases, which
/// replace the window with the frame entirely (src/keyboard.c:5799-5843 for
/// the GUI tab/tool bar windows, :5844-5852 for the terminal tab bar). Naming
/// both in one type is what keeps the area strings in a single table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PosnArea {
    Window(WindowPart),
    MenuBar,
    ToolBar,
    TabBar,
}

/// A coordinate a part classification has placed in a window's text area.
///
/// There is no public constructor: [`WindowPartGeometry::resolve`] is the only
/// thing that makes one, and it makes one for exactly the parts GNU lets reach
/// `buffer_posn_from_coords`. The row lookup that answers a buffer position
/// accepts nothing else, so "look the position up first and classify later" --
/// the shape ledger 205 left behind -- is not expressible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextAreaCoordinate {
    pub(crate) text_area_x: i64,
    pub(crate) window_y: i64,
    pub(crate) text_area_y: i64,
}

/// What `make_lispy_position` does with a coordinate once it is classified.
///
/// The two arms are GNU's own and they are exclusive there: the chrome branch
/// sets `textpos = -1` (src/keyboard.c:5900) so the `if (!textpos)` block that
/// runs `buffer_posn_from_coords` (src/keyboard.c:5975) is skipped, and every
/// other part leaves `textpos` at 0 so it runs. Only [`Self::Buffer`] carries
/// a [`TextAreaCoordinate`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowCoordinate {
    /// A tab, header or mode line. `window_x`/`window_y` are GNU's `wx`/`wy`,
    /// which for this branch are both the posn's reported coordinates and the
    /// input to `mode_line_string` (src/keyboard.c:5890-5905).
    ChromeLine {
        line: WindowChromeLine,
        window_x: i64,
        window_y: i64,
    },
    /// Everything else. The part still names the AREA (nil for `Text`), and the
    /// buffer position is looked up at `at`.
    Buffer {
        part: WindowPart,
        window_x: i64,
        window_y: i64,
        at: TextAreaCoordinate,
    },
}

/// The window geometry `coordinates_in_window` reads (src/window.c:1348-1489),
/// in the frame's pixel units and relative to the frame's origin.
///
/// GNU asks two separate questions about each chrome line and so does this:
/// whether the window HAS one at all (`window_wants_mode_line`,
/// src/window.c:6021-6060 -- a question about the window, its parameters and
/// its buffer's `mode-line-format`) and how TALL it is
/// (`CURRENT_MODE_LINE_HEIGHT`, src/dispextern.h:1563-1570 -- the last
/// redisplay's answer, or an estimate when there is none). This port publishes
/// one number per line, so the two collapse into "height > 0"; ledger 205's
/// residual 4 is that collapse seen from the other side, and it is why a
/// window whose `mode-line-format` was set to nil without a redisplay still
/// reports a mode line here.
///
/// Scroll bars and window dividers are not among the fields. GNU reads their
/// widths off the window (`WINDOW_SCROLL_BAR_AREA_WIDTH`,
/// `WINDOW_RIGHT_DIVIDER_WIDTH`), and on a terminal frame -- the frames this
/// classifier serves -- they are all zero. A window-system frame answers a
/// coordinate from its published presentation regions instead, which carry the
/// scroll bar and divider rectangles directly; [`WindowPart`] still names them
/// so both routes share one table of AREA symbols.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowPartGeometry {
    /// `WINDOW_LEFT_EDGE_X` / `WINDOW_RIGHT_EDGE_X`.
    pub left_x: i64,
    pub right_x: i64,
    /// `WINDOW_TOP_EDGE_Y` / `WINDOW_BOTTOM_EDGE_Y`.
    pub top_y: i64,
    pub bottom_y: i64,
    /// `CURRENT_TAB_LINE_HEIGHT` and friends, zero when the line is absent.
    pub tab_line_height: i64,
    pub header_line_height: i64,
    pub mode_line_height: i64,
    /// `window_box_left_offset (w, TEXT_AREA)`: fringes, margins and any
    /// left-hand scroll bar between the window's left edge and its text.
    pub text_area_left_offset: i64,
    /// `window_box_width (w, TEXT_AREA)`.
    pub text_area_width: i64,
    /// `window_box_width (w, LEFT_MARGIN_AREA)` / `RIGHT_MARGIN_AREA`.
    pub left_margin_width: i64,
    pub right_margin_width: i64,
    /// `FRAME_COLUMN_WIDTH`, the width of the draggable strip GNU calls
    /// `grabbable_width`.
    pub column_width: i64,
    /// `WINDOW_RIGHTMOST_P`: a window that is not rightmost owns a vertical
    /// border in its last column on a terminal frame.
    pub rightmost: bool,
}

/// Immutable process policy, safely published by OnceLock. Independent
/// mutators share only this numeric enum; there is no Lisp state or TLS cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PosnObjectExtentMode {
    Off,
    On,
}

// --- E5-FM-3a tty posn records (stage-4 W2/E5) ---

/// Which existing numeric producer GNU's current rows presently admit.
/// Copy-only state may be read concurrently through immutable Frame publication.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TtyCurrentMatrixAuthority {
    AcceptedLocal,
    FramePoolPartition,
    Undrawn,
}

/// GNU desired-matrix allocation facts, not source/display-body freshness.
/// Numeric values are initialized by the exclusive Frame mutator and immutable
/// to readers; no query advances this allocation observation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TtyMatrixAllocation {
    pub(crate) window: WindowId,
    pub(crate) x: i64,
    pub(crate) y: i64,
    pub(crate) width: i64,
    pub(crate) height: i64,
    pub(crate) left_margin_glyphs: i64,
    pub(crate) right_margin_glyphs: i64,
}

/// Same inline Option<Arc<_>> representation as the prior pool-only owner.
/// COW copies small numeric allocation/authority records and the pool Arc,
/// never the retained pool's row/cell vectors. All writes require &mut Frame.
#[derive(Clone, Debug)]
pub(crate) struct TtyPosnCurrentOwner {
    pub(crate) pool: Option<Arc<PosnFramePool>>,
    pub(crate) allocation: Vec<TtyMatrixAllocation>,
    pub(crate) dimensions: (i64, i64),
    pub(crate) authority: HashMap<WindowId, TtyCurrentMatrixAuthority>,
}

// --- E5-FM-5a Window data accessors (stage-4 W2/E5) ---

impl Window {
    /// Create a new leaf window.
    pub fn new_leaf(id: WindowId, buffer_id: BufferId, bounds: Rect) -> Self {
        Window::Leaf {
            id,
            buffer_id,
            mode_line_base: None,
            // GNU gives a new window change_stamp 0 and a nil old_buffer, so the
            // next record can tell it did not exist before.
            old_buffer: None,
            change_stamp: None,
            bounds,
            window_start: LispCharPos1::ONE,
            position_markers: WindowPositionMarkerState::Detached,
            window_end: WindowEndState::Unrecorded,
            point: LispCharPos1::ONE,
            old_point: LispCharPos1::ONE,
            dedicated: Value::NIL,
            parameters: Vec::new(),
            parameters_generation: 0,
            history: WindowHistoryState::default(),
            fixed_height: 0,
            fixed_width: 0,
            hscroll: 0,
            min_hscroll: 0,
            suspend_auto_hscroll: false,
            vscroll: 0,
            preserve_vscroll_p: false,
            force_start: false,
            margins: WindowMargins::ZERO,
            display: WindowDisplayState::default(),
            // GNU `make_window` initializes all five of these together
            // (`src/window.c:4607-4611`) -- `normal_lines`/`normal_cols` to
            // 1.0 and the three pending-size slots to 0, explicitly because
            // `allocate_window` has already nil'd everything and these are the
            // slots "which should not be nil".  `new_pixel`/`new_total` are
            // `Option<i64>` here and read as 0 via `unwrap_or(0)`, but
            // `new_normal` is a raw `Value` and leaked that nil to
            // `window-new-normal`.
            new_pixel: None,
            new_total: None,
            cell_dimensions: WindowCellDimensions::default(),
            new_normal: Value::fixnum(0),
            normal_lines: Value::make_float(1.0),
            normal_cols: Value::make_float(1.0),
            // GNU `make_window` leaves top_line/left_col zero; the resize passes
            // assign them (root window gets `FRAME_TOP_MARGIN`).
            top_line: 0,
            left_col: 0,
        }
    }

    /// The ids of this window's children, in layout order; empty for a leaf.
    ///
    /// A window NAMES its children rather than owning them, so this answers
    /// only who they are.  To reach one, ask the [`WindowTree`] that holds
    /// them -- `tree.find(id)` -- which is also the only handle that can hand
    /// out a node at all.
    pub fn children(&self) -> &[WindowId] {
        match self {
            Window::Leaf { .. } => &[],
            Window::Internal { children, .. } => children,
        }
    }

    /// Mutable counterpart of [`Window::children`], for the structural edits
    /// -- push, remove, reorder -- that [`WindowTree`] performs on behalf of
    /// split and delete.  `None` for a leaf, which has no child list to edit.
    pub fn children_mut(&mut self) -> Option<&mut Vec<WindowId>> {
        match self {
            Window::Leaf { .. } => None,
            Window::Internal { children, .. } => Some(children),
        }
    }

    /// Character-line top edge. GNU `w->top_line` (`WINDOW_TOP_EDGE_LINE`).
    pub fn top_line(&self) -> i64 {
        match self {
            Window::Leaf { top_line, .. } | Window::Internal { top_line, .. } => *top_line,
        }
    }

    /// Set the character-line top edge. GNU assigns `w->top_line` in the resize
    /// passes / config restore.
    pub fn set_top_line(&mut self, value: i64) {
        match self {
            Window::Leaf { top_line, .. } | Window::Internal { top_line, .. } => *top_line = value,
        }
    }

    /// Character-column left edge. GNU `w->left_col` (`WINDOW_LEFT_EDGE_COL`).
    pub fn left_col(&self) -> i64 {
        match self {
            Window::Leaf { left_col, .. } | Window::Internal { left_col, .. } => *left_col,
        }
    }

    /// Set the character-column left edge. GNU `w->left_col`.
    pub fn set_left_col(&mut self, value: i64) {
        match self {
            Window::Leaf { left_col, .. } | Window::Internal { left_col, .. } => *left_col = value,
        }
    }

    /// Read the pending `new_pixel` slot. GNU `w->new_pixel`.
    pub fn new_pixel(&self) -> Option<i64> {
        match self {
            Window::Leaf { new_pixel, .. } | Window::Internal { new_pixel, .. } => *new_pixel,
        }
    }

    /// Write the pending `new_pixel` slot. GNU `wset_new_pixel`.
    pub fn set_new_pixel(&mut self, value: Option<i64>) {
        match self {
            Window::Leaf { new_pixel, .. } | Window::Internal { new_pixel, .. } => {
                *new_pixel = value;
            }
        }
    }

    /// Read the pending `new_total` slot. GNU `w->new_total`.
    pub fn new_total(&self) -> Option<i64> {
        match self {
            Window::Leaf { new_total, .. } | Window::Internal { new_total, .. } => *new_total,
        }
    }

    pub fn total_lines(&self, char_height: f32) -> i64 {
        match self {
            Self::Leaf {
                cell_dimensions,
                bounds,
                ..
            }
            | Self::Internal {
                cell_dimensions,
                bounds,
                ..
            } => cell_dimensions
                .lines
                .unwrap_or((bounds.height / char_height.max(1.0)) as i64),
        }
    }

    pub fn total_columns(&self, char_width: f32) -> i64 {
        match self {
            Self::Leaf {
                cell_dimensions,
                bounds,
                ..
            }
            | Self::Internal {
                cell_dimensions,
                bounds,
                ..
            } => cell_dimensions
                .columns
                .unwrap_or((bounds.width / char_width.max(1.0)) as i64),
        }
    }

    pub(crate) fn commit_cell_total(&mut self, horizontal: bool, total: i64) {
        match self {
            Self::Leaf {
                cell_dimensions, ..
            }
            | Self::Internal {
                cell_dimensions, ..
            } => {
                if horizontal {
                    cell_dimensions.columns = Some(total);
                } else {
                    cell_dimensions.lines = Some(total);
                }
            }
        }
    }

    /// Write the pending `new_total` slot. GNU `wset_new_total`.
    pub fn set_new_total(&mut self, value: Option<i64>) {
        match self {
            Window::Leaf { new_total, .. } | Window::Internal { new_total, .. } => {
                *new_total = value;
            }
        }
    }

    /// Read the pending `new_normal` Lisp slot. GNU `w->new_normal`.
    pub fn new_normal(&self) -> Value {
        match self {
            Window::Leaf { new_normal, .. } | Window::Internal { new_normal, .. } => *new_normal,
        }
    }

    /// Write the pending `new_normal` Lisp slot.
    pub fn set_new_normal(&mut self, value: Value) {
        match self {
            Window::Leaf { new_normal, .. } | Window::Internal { new_normal, .. } => {
                *new_normal = value;
            }
        }
    }

    /// Read the persistent `normal_lines` Lisp slot. GNU
    /// `w->normal_lines`.
    pub fn normal_lines(&self) -> Value {
        match self {
            Window::Leaf { normal_lines, .. } | Window::Internal { normal_lines, .. } => {
                *normal_lines
            }
        }
    }

    /// Write the persistent `normal_lines` Lisp slot. GNU
    /// `wset_normal_lines`.
    pub fn set_normal_lines(&mut self, value: Value) {
        match self {
            Window::Leaf { normal_lines, .. } | Window::Internal { normal_lines, .. } => {
                *normal_lines = value;
            }
        }
    }

    /// Read the persistent `normal_cols` Lisp slot. GNU
    /// `w->normal_cols`.
    pub fn normal_cols(&self) -> Value {
        match self {
            Window::Leaf { normal_cols, .. } | Window::Internal { normal_cols, .. } => *normal_cols,
        }
    }

    /// Write the persistent `normal_cols` Lisp slot. GNU
    /// `wset_normal_cols`.
    pub fn set_normal_cols(&mut self, value: Value) {
        match self {
            Window::Leaf { normal_cols, .. } | Window::Internal { normal_cols, .. } => {
                *normal_cols = value;
            }
        }
    }

    /// Record this leaf as width-fixed at COLS columns.  A value of 0 clears
    /// the fixed-width constraint.
    pub fn set_fixed_width_cols(&mut self, cols: usize) {
        if let Window::Leaf { fixed_width, .. } = self {
            *fixed_width = cols;
        }
    }

    /// Record this leaf as height-fixed at LINES rows.  A value of 0 clears
    /// the fixed-height constraint.
    pub fn set_fixed_height_lines(&mut self, lines: usize) {
        if let Window::Leaf { fixed_height, .. } = self {
            *fixed_height = lines;
        }
    }

    pub fn fixed_width_cols(&self) -> usize {
        match self {
            Window::Leaf { fixed_width, .. } => *fixed_width,
            Window::Internal { .. } => 0,
        }
    }

    pub fn fixed_height_lines(&self) -> usize {
        match self {
            Window::Leaf { fixed_height, .. } => *fixed_height,
            Window::Internal { .. } => 0,
        }
    }

    /// Set the window's point from a buffer position.
    /// GNU Emacs xdisp.c:20616 syncs w->pointm from buffer PT before redisplay.
    pub fn set_point(&mut self, pos: LispCharPos1) {
        if let Window::Leaf { point, .. } = self {
            *point = pos.max(LispCharPos1::ONE);
        }
    }

    /// Horizontal scroll offset in columns (`w->hscroll`). 0 for internal
    /// nodes.
    pub fn hscroll(&self) -> usize {
        match self {
            Window::Leaf { hscroll, .. } => *hscroll,
            Window::Internal { .. } => 0,
        }
    }

    /// Lower bound for automatic horizontal scrolling (`w->min_hscroll`).
    pub fn min_hscroll(&self) -> usize {
        match self {
            Window::Leaf { min_hscroll, .. } => *min_hscroll,
            Window::Internal { .. } => 0,
        }
    }

    /// Set the lower bound for automatic horizontal scrolling
    /// (`w->min_hscroll`). No-op for internal nodes.
    pub fn set_min_hscroll(&mut self, value: usize) {
        if let Window::Leaf { min_hscroll, .. } = self {
            *min_hscroll = value;
        }
    }

    /// Whether automatic horizontal scrolling is currently suspended
    /// (`w->suspend_auto_hscroll`).
    pub fn suspend_auto_hscroll(&self) -> bool {
        match self {
            Window::Leaf {
                suspend_auto_hscroll,
                ..
            } => *suspend_auto_hscroll,
            Window::Internal { .. } => false,
        }
    }

    /// Set the auto-hscroll suspend flag (`w->suspend_auto_hscroll`). No-op
    /// for internal nodes.
    pub fn set_suspend_auto_hscroll(&mut self, value: bool) {
        if let Window::Leaf {
            suspend_auto_hscroll,
            ..
        } = self
        {
            *suspend_auto_hscroll = value;
        }
    }

    pub fn redisplay_state(&self) -> Option<WindowRedisplayState> {
        match self {
            Window::Leaf {
                id,
                buffer_id,
                bounds,
                window_start,
                window_end,
                point,
                old_point,
                hscroll,
                vscroll,
                preserve_vscroll_p,
                ..
            } => Some(WindowRedisplayState {
                id: *id,
                buffer_id: *buffer_id,
                bounds: (
                    redisplay_f32_bits(bounds.x),
                    redisplay_f32_bits(bounds.y),
                    redisplay_f32_bits(bounds.width),
                    redisplay_f32_bits(bounds.height),
                ),
                window_start: *window_start,
                window_end: *window_end,
                point: *point,
                old_point: *old_point,
                hscroll: *hscroll,
                vscroll: *vscroll,
                preserve_vscroll_p: *preserve_vscroll_p,
            }),
            Window::Internal { .. } => None,
        }
    }

    /// Window ID.
    pub fn id(&self) -> WindowId {
        match self {
            Window::Leaf { id, .. } | Window::Internal { id, .. } => *id,
        }
    }

    /// Pixel bounds.
    pub fn bounds(&self) -> &Rect {
        match self {
            Window::Leaf { bounds, .. } | Window::Internal { bounds, .. } => bounds,
        }
    }

    /// Mutable reference to bounds.
    pub fn bounds_mut(&mut self) -> &mut Rect {
        match self {
            Window::Leaf {
                bounds,
                cell_dimensions,
                ..
            }
            | Window::Internal {
                bounds,
                cell_dimensions,
                ..
            } => {
                *cell_dimensions = WindowCellDimensions::default();
                bounds
            }
        }
    }

    /// Set bounds.
    pub fn set_bounds(&mut self, new_bounds: Rect) {
        match self {
            Window::Leaf {
                bounds,
                cell_dimensions,
                ..
            }
            | Window::Internal {
                bounds,
                cell_dimensions,
                ..
            } => {
                if bounds.width != new_bounds.width {
                    cell_dimensions.columns = None;
                }
                if bounds.height != new_bounds.height {
                    cell_dimensions.lines = None;
                }
                *bounds = new_bounds;
            }
        }
    }

    /// Whether this is a leaf window.
    pub fn is_leaf(&self) -> bool {
        matches!(self, Window::Leaf { .. })
    }

    /// Return this leaf window's display state.
    pub fn display(&self) -> Option<&WindowDisplayState> {
        match self {
            Window::Leaf { display, .. } => Some(display),
            Window::Internal { .. } => None,
        }
    }

    pub fn redisplay_output(&self) -> Option<&WindowRedisplayOutput> {
        self.display()?.redisplay_output()
    }

    /// Accept every query-visible fact produced by one redisplay generation.
    ///
    /// Synchronous layout queries deliberately do not use this path: they may
    /// refresh `window-end`, but cannot masquerade as a presented redisplay.
    pub(crate) fn accept_redisplay_output(&mut self, output: WindowRedisplayOutput) {
        if let Self::Leaf {
            window_end,
            display,
            ..
        } = self
        {
            *window_end = WindowEndState::Current(output.window_end());
            display.redisplay_output = Some(output);
        }
    }

    /// Return a mutable reference to this leaf window's display state.
    pub fn display_mut(&mut self) -> Option<&mut WindowDisplayState> {
        match self {
            Window::Leaf { display, .. } => Some(display),
            Window::Internal { .. } => None,
        }
    }

    /// Return this window's Lisp-visible parameter alist.
    pub fn parameters(&self) -> &WindowParameters {
        match self {
            Window::Leaf { parameters, .. } | Window::Internal { parameters, .. } => parameters,
        }
    }

    pub const fn parameters_generation(&self) -> u64 {
        match self {
            Window::Leaf {
                parameters_generation,
                ..
            }
            | Window::Internal {
                parameters_generation,
                ..
            } => *parameters_generation,
        }
    }

    /// Return this live window's history state.
    pub fn history(&self) -> Option<&WindowHistoryState> {
        match self {
            Window::Leaf { history, .. } => Some(history),
            Window::Internal { .. } => None,
        }
    }

    /// Return a mutable reference to this live window's history state.
    pub fn history_mut(&mut self) -> Option<&mut WindowHistoryState> {
        match self {
            Window::Leaf { history, .. } => Some(history),
            Window::Internal { .. } => None,
        }
    }

    /// Get the combination limit for an internal window.
    pub fn combination_limit(&self) -> Option<bool> {
        match self {
            Window::Internal {
                combination_limit, ..
            } => Some(*combination_limit),
            Window::Leaf { .. } => None,
        }
    }

    /// Set the combination limit for an internal window.
    pub fn set_combination_limit(&mut self, limit: bool) {
        if let Window::Internal {
            combination_limit, ..
        } = self
        {
            *combination_limit = limit;
        }
    }

    /// Start position of this window (leaf only).
    pub fn window_start(&self) -> Option<LispCharPos1> {
        match self {
            Window::Leaf { window_start, .. } => Some(*window_start),
            Window::Internal { .. } => None,
        }
    }

    /// Buffer displayed in this window (leaf only).
    /// This window's window-change epoch, or `None` when it has never been
    /// recorded -- GNU's `w->change_stamp == 0`, which marks a window created
    /// since the last run of the window change functions.
    ///
    /// Internal windows are never stamped: GNU's
    /// `window_change_record_windows` recurses past them and records only
    /// live leaves (`src/window.c`).
    pub fn old_buffer(&self) -> Option<BufferId> {
        match self {
            Window::Leaf { old_buffer, .. } => *old_buffer,
            Window::Internal { .. } => None,
        }
    }

    /// Record the buffer and epoch, as `window_change_record_windows` does.
    pub fn record_change_epoch(&mut self, stamp: ChangeStamp) {
        if let Window::Leaf {
            buffer_id,
            old_buffer,
            change_stamp,
            ..
        } = self
        {
            *old_buffer = Some(*buffer_id);
            *change_stamp = Some(stamp);
        }
    }

    pub fn change_stamp(&self) -> Option<ChangeStamp> {
        match self {
            Window::Leaf { change_stamp, .. } => *change_stamp,
            Window::Internal { .. } => None,
        }
    }

    /// Stamp this window with the epoch that has just been recorded.
    pub fn set_change_stamp(&mut self, stamp: ChangeStamp) {
        if let Window::Leaf { change_stamp, .. } = self {
            *change_stamp = Some(stamp);
        }
    }

    pub fn buffer_id(&self) -> Option<BufferId> {
        match self {
            Window::Leaf { buffer_id, .. } => Some(*buffer_id),
            Window::Internal { .. } => None,
        }
    }

    pub(crate) fn mode_line_base(&self) -> Option<ModeLineBase> {
        match self {
            Self::Leaf { mode_line_base, .. } => *mode_line_base,
            Self::Internal { .. } => None,
        }
    }

    pub(crate) fn set_mode_line_base(&mut self, value: Option<ModeLineBase>) {
        if let Self::Leaf { mode_line_base, .. } = self {
            *mode_line_base = value;
        }
    }

    /// Invalidate GNU's numeric base when replacing a displayed buffer without
    /// resetting the other window fields (temporary echo/startup transitions).
    pub fn reset_mode_line_base(&mut self) {
        self.set_mode_line_base(None);
    }

    /// Set the buffer displayed in this window (leaf only).
    pub fn set_buffer(&mut self, new_id: BufferId) {
        if let Window::Leaf {
            buffer_id,
            window_start,
            position_markers,
            window_end,
            point,
            min_hscroll,
            suspend_auto_hscroll,
            mode_line_base,
            ..
        } = self
        {
            // GNU wset_buffer invalidates base_line_pos even for a numeric
            // identity round trip; do not preserve pre-switch history.
            *mode_line_base = None;
            *buffer_id = new_id;
            *window_start = LispCharPos1::ONE;
            *position_markers = WindowPositionMarkerState::Detached;
            *window_end = WindowEndState::Unrecorded;
            *point = LispCharPos1::ONE;
            // GNU resets the auto-hscroll lower bound and the suspend flag on
            // every buffer switch (`unshow_buffer`, src/window.c:4368). The
            // `hscroll` reset itself is handled by the caller's normal
            // window-start/point reinitialization path.
            *min_hscroll = 0;
            *suspend_auto_hscroll = false;
        }
    }

    /// Stored Lisp-visible `window-end` for this leaf window.
    pub fn window_end_charpos(&self, buffer_z: LispCharPos1) -> Option<LispCharPos1> {
        match self {
            Window::Leaf { window_end, .. } => Some(window_end.charpos_from_z(buffer_z)),
            Window::Internal { .. } => None,
        }
    }

    /// Stored byte-position `window-end` for this leaf window.
    pub fn window_end_bytepos(&self, buffer_z_byte: EmacsBytePos) -> Option<EmacsBytePos> {
        match self {
            Window::Leaf { window_end, .. } => Some(window_end.bytepos_from_z(buffer_z_byte)),
            Window::Internal { .. } => None,
        }
    }

    /// Typed presentation lifecycle for this leaf window.
    pub fn window_end_state(&self) -> Option<WindowEndState> {
        match self {
            Window::Leaf { window_end, .. } => Some(*window_end),
            Window::Internal { .. } => None,
        }
    }

    /// Whether the stored window-end came from a completed redisplay.
    pub fn window_end_valid(&self) -> Option<bool> {
        self.window_end_state().map(WindowEndState::is_current)
    }

    /// Mark the last complete window-end record stale without discarding it.
    ///
    /// GNU keeps the offsets available for `window-end` with `UPDATE=nil`
    /// after invalidation, while `UPDATE=t` must recompute.
    pub fn invalidate_window_end(&mut self) {
        if let Window::Leaf { window_end, .. } = self {
            window_end.invalidate();
        }
    }

    /// Publish the last redisplay's window-end state for this leaf window.
    pub fn set_window_end_from_positions(
        &mut self,
        buffer_z_char: LispCharPos1,
        buffer_z_byte: EmacsBytePos,
        end_charpos: LispCharPos1,
        end_bytepos: EmacsBytePos,
        vpos: usize,
    ) {
        self.set_window_end_record(WindowEndRecord::from_positions(
            buffer_z_char,
            buffer_z_byte,
            end_charpos,
            end_bytepos,
            MatrixRow0::new(vpos),
        ));
    }

    /// Publish a complete char/byte/row record without reopening its tuple.
    pub fn set_window_end_record(&mut self, record: WindowEndRecord) {
        if let Window::Leaf { window_end, .. } = self {
            *window_end = WindowEndState::Current(record);
        }
    }

    pub(crate) fn restore_window_end_state(&mut self, state: WindowEndState) {
        if let Window::Leaf { window_end, .. } = self {
            *window_end = state;
        }
    }

    /// Replace a displayed buffer id in all leaf windows under this node.
    ///
    /// This is used when a buffer is killed; any window still attached to the
    /// dead buffer is moved back to a replacement buffer (typically `*scratch*`).
    pub fn replace_buffer_id(&mut self, old_id: BufferId, new_id: BufferId) {
        if let Window::Leaf { buffer_id, .. } = self
            && *buffer_id == old_id
        {
            self.set_buffer(new_id);
        }
    }

    /// Invalidate redisplay-derived window-end state for this window.
    pub fn invalidate_display_state(&mut self) {
        let Window::Leaf {
            window_end,
            display,
            ..
        } = self
        else {
            return;
        };
        window_end.invalidate();
        display.clear_physical_cursor_state();
    }

    /// Drop only horizontal scroll derived from the previous geometry.
    ///
    /// GNU distinguishes auto-hscroll state from an explicit
    /// `set-window-hscroll` with `suspend_auto_hscroll`. Geometry changes
    /// invalidate the former, while the latter remains user-owned.
    pub(crate) fn invalidate_automatic_hscroll_for_geometry_change(&mut self) {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum HorizontalScrollOwnership {
            Automatic,
            Explicit,
        }

        match self {
            Window::Leaf {
                hscroll,
                min_hscroll,
                suspend_auto_hscroll,
                ..
            } => {
                let ownership = if *suspend_auto_hscroll {
                    HorizontalScrollOwnership::Explicit
                } else {
                    HorizontalScrollOwnership::Automatic
                };
                match ownership {
                    HorizontalScrollOwnership::Automatic => *hscroll = *min_hscroll,
                    HorizontalScrollOwnership::Explicit => {}
                }
            }
            Window::Internal { .. } => {}
        }
    }
}

pub(crate) fn redisplay_f32_bits(value: f32) -> u32 {
    if value == 0.0 {
        0.0f32.to_bits()
    } else {
        value.to_bits()
    }
}

// --- E5-FM-5b WindowTree accessors (stage-4 W2/E5) ---

impl WindowTree {
    /// A tree whose only node is `root`.
    ///
    /// A childless node is the only thing a tree can be seeded with: under ids
    /// a detached `Window` carries no subtree, so a tree is grown through
    /// [`WindowTree::insert`] and [`WindowTree::set_children`] rather than
    /// handed one already built.
    pub fn new(root: Window) -> Self {
        debug_assert!(
            root.children().is_empty(),
            "a tree is seeded with a childless root; its children's nodes would have nowhere to live"
        );
        let root_id = root.id();
        let mut nodes = HashMap::default();
        nodes.insert(root_id, root);
        Self {
            nodes,
            root: root_id,
        }
    }

    /// The id of this tree's root.
    pub fn root_id(&self) -> WindowId {
        self.root
    }

    /// The root window of this tree.
    pub fn root(&self) -> &Window {
        self.find(self.root)
            .expect("a tree always holds the node its root id names")
    }

    pub fn root_mut(&mut self) -> &mut Window {
        let root = self.root;
        self.find_mut(root)
            .expect("a tree always holds the node its root id names")
    }

    /// Make `id` this tree's root.  The node must already be in the tree.
    pub fn set_root(&mut self, id: WindowId) {
        debug_assert!(self.nodes.contains_key(&id), "root must name a held node");
        self.root = id;
    }

    /// Find a window anywhere in this tree, by a single probe.
    pub fn find(&self, id: WindowId) -> Option<&Window> {
        self.nodes.get(&id)
    }

    pub fn find_mut(&mut self, id: WindowId) -> Option<&mut Window> {
        self.nodes.get_mut(&id)
    }

    /// Take ownership of `window`, which may then be named as someone's child.
    pub fn insert(&mut self, window: Window) {
        self.nodes.insert(window.id(), window);
    }

    /// Drop `id`'s node from the tree entirely.
    ///
    /// Unlinking a window from its parent is NOT this: GNU keeps a deleted
    /// window's struct so `windowp` still answers for it.  This is for a node
    /// that is genuinely going away, such as one absorbed into another tree.
    pub fn remove(&mut self, id: WindowId) -> Option<Window> {
        self.nodes.remove(&id)
    }

    /// Whether this tree holds a node called `id` at all, linked or not.
    pub fn contains(&self, id: WindowId) -> bool {
        self.nodes.contains_key(&id)
    }

    /// Every node this tree holds, in no particular order, including any that
    /// no longer hang off the root.
    pub fn nodes(&self) -> impl Iterator<Item = &Window> {
        self.nodes.values()
    }

    pub fn nodes_mut(&mut self) -> impl Iterator<Item = &mut Window> {
        self.nodes.values_mut()
    }

    /// `id`'s children, in layout order.
    pub fn child_ids(&self, id: WindowId) -> &[WindowId] {
        self.find(id).map_or(&[], Window::children)
    }

    /// Replace `id`'s children wholesale.
    pub fn set_children(&mut self, id: WindowId, children: Vec<WindowId>) {
        if let Some(Window::Internal { children: slot, .. }) = self.find_mut(id) {
            *slot = children;
        }
    }

    /// The parent of `id`, or `None` for the root and for any node not linked
    /// into it.
    pub fn parent_of(&self, id: WindowId) -> Option<WindowId> {
        self.parent_of_in(self.root, id)
    }

    fn parent_of_in(&self, node: WindowId, target: WindowId) -> Option<WindowId> {
        let children = self.child_ids(node);
        if children.contains(&target) {
            return Some(node);
        }
        children
            .iter()
            .find_map(|child| self.parent_of_in(*child, target))
    }

    /// The ids of the subtree rooted at `id`, parents before children.
    pub fn subtree_ids(&self, id: WindowId) -> Vec<WindowId> {
        let mut out = Vec::new();
        self.collect_subtree(id, &mut out);
        out
    }

    fn collect_subtree(&self, id: WindowId, out: &mut Vec<WindowId>) {
        if !self.nodes.contains_key(&id) {
            return;
        }
        out.push(id);
        for child in self.child_ids(id) {
            self.collect_subtree(*child, out);
        }
    }

    /// The leaf windows of this tree.
    pub fn leaf_ids(&self) -> Vec<WindowId> {
        self.leaf_ids_of(self.root)
    }

    /// The leaf windows of the subtree rooted at `id`.
    pub fn leaf_ids_of(&self, id: WindowId) -> Vec<WindowId> {
        let mut out = self.subtree_ids(id);
        out.retain(|candidate| self.find(*candidate).is_some_and(Window::is_leaf));
        out
    }

    pub fn leaf_count(&self) -> usize {
        self.leaf_ids().len()
    }

    /// Apply `visit` to every node linked under the root, parents first.
    ///
    /// The counterpart of [`WindowTree::for_each_leaf_mut`] for the walks that
    /// touch internal windows too, such as window parameters.
    pub fn for_each_node_mut(&mut self, mut visit: impl FnMut(&mut Window)) {
        for id in self.subtree_ids(self.root) {
            if let Some(node) = self.find_mut(id) {
                visit(node);
            }
        }
    }

    /// The leaf window whose rectangle covers `(px, py)`.
    ///
    /// Descends only into internal windows whose own rectangle contains the
    /// point, as the inline walk did: a child never sticks out of its parent,
    /// so a parent that misses rules out its whole subtree.
    pub fn window_at(&self, px: f32, py: f32) -> Option<WindowId> {
        self.window_at_in(self.root, px, py)
    }

    fn window_at_in(&self, id: WindowId, px: f32, py: f32) -> Option<WindowId> {
        let window = self.find(id)?;
        if !window.bounds().contains(px, py) {
            return None;
        }
        if window.is_leaf() {
            return Some(id);
        }
        window
            .children()
            .iter()
            .find_map(|child| self.window_at_in(*child, px, py))
    }

    /// How many leaves of this tree show text sharing `root_buffer`.
    pub fn buffer_window_count(&self, buffers: &BufferManager, root_buffer: BufferId) -> usize {
        self.leaf_ids()
            .into_iter()
            .filter(|id| {
                self.find(*id)
                    .and_then(Window::buffer_id)
                    .and_then(|buffer| buffers.shared_text_root_id(buffer))
                    == Some(root_buffer)
            })
            .count()
    }

    /// Point every leaf showing `old_id` at `new_id` instead.
    pub fn replace_buffer_id(&mut self, old_id: BufferId, new_id: BufferId) {
        self.for_each_leaf_mut(|leaf| leaf.replace_buffer_id(old_id, new_id));
    }

    /// Invalidate redisplay-derived window-end state across this tree.
    pub fn invalidate_display_state(&mut self) {
        self.for_each_leaf_mut(Window::invalidate_display_state);
    }

    /// Drop hscroll derived from the previous geometry across this tree.
    pub fn invalidate_automatic_hscroll_for_geometry_change(&mut self) {
        self.for_each_leaf_mut(Window::invalidate_automatic_hscroll_for_geometry_change);
    }

    /// Apply `visit` to every leaf of this tree, one leaf at a time.
    ///
    /// The leaves are enumerated before any of them is handed out, so no two
    /// nodes are borrowed at once.  That is what lets a walk over the tree
    /// stay flat rather than recursing through `&mut Window`, and it is the
    /// shape that survives children becoming ids: a node is then reached by a
    /// lookup, and a lookup cannot be taken while a sibling is still borrowed.
    pub fn for_each_leaf_mut(&mut self, mut visit: impl FnMut(&mut Window)) {
        // Walk the tree once. `leaf_ids` built a vector of every id in the
        // subtree, looked each one up to drop the internal ones, and then this
        // looked every survivor up again -- three passes and an allocation for
        // what is usually one window, on a path every buffer edit runs
        // (`sync_window_positions_from_markers`).
        let mut pending: smallvec::SmallVec<[WindowId; 8]> = smallvec::SmallVec::new();
        pending.push(self.root);
        while let Some(id) = pending.pop() {
            let Some(node) = self.nodes.get_mut(&id) else {
                continue;
            };
            match node {
                Window::Leaf { .. } => visit(node),
                // Reversed, so popping yields the children left to right and
                // the leaves arrive in the same preorder `leaf_ids` produced
                // -- the order in which a clone attaches its markers.
                Window::Internal { children, .. } => pending.extend(children.iter().rev().copied()),
            }
        }
    }
}

// --- E5-FM-3a body redisplay revision (stage-4 W2/E5) ---

/// Monotonic body-redisplay revisions consulted when a window may reuse
/// retained body rows.
///
/// This is deliberately separate from `Context::redisplay_generation`: that
/// counter also moves for presentation-only work (mode-line/chrome/menu
/// updates) which must NOT relayout body text. Only an explicit body
/// invalidation — today GNU `force-window-update` — moves these counters, and
/// the three scopes below keep a targeted request from escalating unrelated
/// windows:
///
/// - `all`    — every window (nil OBJECT),
/// - `window` — one live window (and, through `buffer`, its displayed buffer),
/// - `buffer` — every window displaying that buffer.
///
/// Equality is the reuse predicate: any move for this window/buffer pair makes
/// the retained key differ, which escalates to a full rebuild.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BodyRedisplayRevision {
    all: u64,
    window: u64,
    buffer: u64,
}

impl BodyRedisplayRevision {
    pub(crate) const fn new(all: u64, window: u64, buffer: u64) -> Self {
        Self {
            all,
            window,
            buffer,
        }
    }
}

// --- E5-FM-3a window display snapshot (stage-4 W2/E5) ---

/// Last authoritative redisplay geometry for a live leaf window.
#[derive(Clone, Debug)]
pub struct WindowDisplaySnapshot {
    /// Window identifier this snapshot belongs to.
    pub window_id: WindowId,
    /// Character-grid origin stored independently from pixel geometry.
    pub cell_origin: CellOrigin,
    /// Immutable regions produced by the same completed redisplay.
    pub regions: PresentedWindowRegions,
    /// Whether redisplay materialized the window body and chrome in this snapshot.
    pub regions_materialized: bool,
    /// Body-local row mapping published by the producer, independent of chrome
    /// row counts retained for compatibility.
    pub body_rows: Vec<PresentedBodyRowSnapshot>,
    /// Text-area offset from the window's left edge, in pixels.
    pub text_area_left_offset: i64,
    /// Last redisplay mode-line height in pixels.
    pub mode_line_height: i64,
    /// Last redisplay header-line height in pixels.
    pub header_line_height: i64,
    /// Last redisplay tab-line height in pixels.
    pub tab_line_height: i64,
    /// Rooted displayed string objects used by window-chrome glyph rows.
    pub chrome_strings: PresentedWindowChromeStrings,
    /// Intended cursor position in the redisplay result, even when no physical
    /// cursor was emitted.
    pub logical_cursor: Option<WindowCursorPos>,
    /// Extent from the same selected glyph as the logical position. Legacy
    /// snapshots may omit it and retain canonical source-point lookup.
    pub logical_cursor_dimensions: Option<LogicalCursorDimensions>,
    /// Last redisplay physical cursor geometry for this window, if the cursor
    /// was shown.
    pub phys_cursor: Option<WindowCursorSnapshot>,
    /// Visible source-position geometry, sorted by `buffer_pos`.
    pub points: Vec<DisplayPointSnapshot>,
    /// Shared immutable per-row storage. When present this is authoritative;
    /// The flat vector is empty under `on` and `verify`; `verify` checks each
    /// frozen row's encoding roundtrip before publication.
    pub point_rows: Option<DisplayPointRows>,
    /// Visible row metrics, sorted by `row`.
    pub rows: Vec<DisplayRowSnapshot>,
    /// Completion evidence for the virtual display tail at accessible EOB.
    pub eob_overlay_motion: EobOverlayMotionCoverage,
    /// The displayed buffer's `modified_tick` when this snapshot was produced.
    /// The snapshot is a redisplay cache; a display primitive that reads it
    /// (e.g. `vertical-motion` with a column target) must treat it as valid
    /// only while the buffer is unchanged since that redisplay — GNU always
    /// recomputes from current buffer state. `None` disables the staleness
    /// gate (used by test fixtures that install a synthetic snapshot).
    pub buffer_modiff: Option<i64>,
    /// Complete identity of the layout inputs that produced this snapshot.
    ///
    /// Production redisplay records this token. Display primitives compare it
    /// with the evaluator's current token before consuming rows, so overlay
    /// edits, narrowing, face changes, display variables and window geometry
    /// cannot leave query-visible layout stale. `None` is reserved for
    /// synthetic test fixtures that intentionally install trusted rows.
    pub layout_freshness: Option<WindowDisplaySnapshotFreshness>,
    /// Numeric matrix facts produced with this snapshot. Only the retained
    /// accepted output is current-matrix state; a query snapshot has no
    /// publication authority. Immutable Arc data is safe for concurrent readers.
    pub posn_matrix:
        Option<std::sync::Arc<neomacs_display_protocol::posn_object_extent::PosnMatrixSnapshot>>,
    /// Test-only numeric policy captured by the snapshot's exclusive producer.
    /// Published snapshots retain immutable copies; independent mutators share
    /// no selector, Lisp value, or TLS state. Absent from shipping layouts.
    #[cfg(any(test, feature = "redisplay-test-policy"))]
    #[doc(hidden)]
    pub test_posn_object_extent_mode: Option<PosnObjectExtentMode>,
    /// Exact end record produced by the same row walk as this snapshot.
    pub window_end_record: Option<WindowEndRecord>,
}

// --- E5-FM-3b frame manager support records (stage-4 W2/E5) ---

/// Publication domain for one snapshot in a renderer presentation.
///
/// Most snapshots describe the live buffer owned by their window and may
/// therefore update GNU-shaped window output and retained redisplay caches.
/// Inactive echo-area redisplay is different: GNU temporarily displays an
/// echo buffer through the minibuffer window, then restores the live
/// minibuffer state. Its geometry must remain available to rendering and hit
/// testing, but it must never become evidence about the live minibuffer.
///
/// The snapshot is shared (`Arc`): a frame's publication, its presentation
/// geometry, the frame's redisplay cache and the layout engine's retained
/// matrix all hold the same one, where each used to copy every visible
/// position and row of every window on every frame.
#[derive(Clone, Debug, PartialEq)]
pub enum WindowPresentationSnapshot {
    LiveWindow(std::sync::Arc<WindowDisplaySnapshot>),
    GeometryOnly(std::sync::Arc<WindowDisplaySnapshot>),
}

impl WindowPresentationSnapshot {
    /// A snapshot of a window's live buffer.
    pub fn live(snapshot: WindowDisplaySnapshot) -> Self {
        Self::LiveWindow(std::sync::Arc::new(snapshot))
    }

    /// A snapshot kept only as renderer and interaction geometry.
    pub fn geometry_only(snapshot: WindowDisplaySnapshot) -> Self {
        Self::GeometryOnly(std::sync::Arc::new(snapshot))
    }

    pub fn window_id(&self) -> WindowId {
        self.display_snapshot().window_id
    }

    pub fn display_snapshot(&self) -> &WindowDisplaySnapshot {
        self.shared_display_snapshot()
    }

    /// The snapshot itself, to share rather than copy.
    pub fn shared_display_snapshot(&self) -> &std::sync::Arc<WindowDisplaySnapshot> {
        match self {
            Self::LiveWindow(snapshot) | Self::GeometryOnly(snapshot) => snapshot,
        }
    }

    /// Mutable access; copies the snapshot first if another holder shares it.
    pub fn display_snapshot_mut(&mut self) -> &mut WindowDisplaySnapshot {
        match self {
            Self::LiveWindow(snapshot) | Self::GeometryOnly(snapshot) => {
                std::sync::Arc::make_mut(snapshot)
            }
        }
    }

    /// Prevent this snapshot from becoming live redisplay evidence while
    /// retaining it as renderer and interaction geometry.
    pub fn retain_as_geometry_only(&mut self) {
        if let Self::LiveWindow(snapshot) = self {
            *self = Self::GeometryOnly(std::mem::take(snapshot));
        }
    }

    /// Snapshot evidence that is valid for the window's live buffer.
    /// Geometry-only publications deliberately cannot produce this value.
    pub fn live_window_snapshot(&self) -> Option<&WindowDisplaySnapshot> {
        self.shared_live_window_snapshot()
            .map(|snapshot| &**snapshot)
    }

    /// [`Self::live_window_snapshot`], to share rather than copy.
    pub fn shared_live_window_snapshot(&self) -> Option<&std::sync::Arc<WindowDisplaySnapshot>> {
        match self {
            Self::LiveWindow(snapshot) => Some(snapshot),
            Self::GeometryOnly(_) => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PreparedDisplayPresentation {
    pub(crate) geometry: Arc<PresentationGeometry>,
    pub(crate) publications: Vec<WindowPresentationSnapshot>,
}

#[derive(Default)]
pub(crate) struct FramePresentationState {
    pub(crate) prepared: HashMap<PresentationId, PreparedDisplayPresentation>,
    pub(crate) active: Option<PreparedDisplayPresentation>,
    /// Accepted Lisp redisplay geometry outlives an unsubmitted renderer ticket.
    /// Share its allocation with the pending/active presentation, retaining only
    /// the latest completed geometry when those renderer owners release it.
    pub(crate) completed_geometry: Option<Arc<PresentationGeometry>>,
    /// Allocation high-water mark also rejects identities of discarded tickets.
    pub(crate) last_identity: Option<PresentationId>,
}

/// Native IDs stay monotone across source preload; only this printable origin
/// advances at the closed, no-live-frames construction boundary.
#[derive(Clone, Copy, Debug)]
pub(crate) struct WindowSequenceOrigin(pub(crate) WindowId);

impl WindowSequenceOrigin {
    pub(crate) fn sequence_number(self, window: WindowId, next: WindowId) -> WindowSequenceNumber {
        if window.0 < FIRST_WINDOW_ID.0 || window.0 >= next.0 {
            return WindowSequenceNumber::Unminted(window);
        }
        match window.0.checked_sub(self.0.0) {
            None => WindowSequenceNumber::RetiredPreload,
            Some(offset) => offset
                .checked_add(1)
                .and_then(std::num::NonZeroU64::new)
                .map_or(
                    WindowSequenceNumber::Unminted(window),
                    WindowSequenceNumber::Runtime,
                ),
        }
    }
}

/// Sequence position for GNU's generated terminal-frame names (`F1`, `F2`,
/// ...).  This is intentionally independent of `FrameId`: GNU keeps a
/// dedicated `tty_frame_count`, and pdump/internal object allocation must not
/// leak into the user-visible frame name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TtyFrameNameOrdinal(pub(crate) std::num::NonZeroU64);

/// Manages all frames and tracks the selected frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NavigationIntentToken {
    pub(crate) generation: std::num::NonZeroU64,
    pub(crate) direction: TransitionDirection,
}

impl NavigationIntentToken {
    pub const fn direction(self) -> TransitionDirection {
        self.direction
    }
}

#[derive(Debug)]
pub(crate) struct PendingIntentLedger<Scope> {
    pub(crate) by_scope: HashMap<Scope, NavigationIntentToken>,
}

#[derive(Debug, Default)]
pub(crate) struct PendingContentTransitionIntents {
    pub(crate) windows: PendingIntentLedger<WindowId>,
    pub(crate) frames: PendingIntentLedger<FrameId>,
}

/// What a deleted window remembers on the window object GNU keeps alive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeletedWindowRecord {
    /// The window's owning frame, retained even after window deletion.
    /// `None` means ownership was not recorded; it does not name another frame.
    pub owner: Option<FrameId>,
    /// The buffer the window was showing when it was deleted.
    pub old_buffer: Option<BufferId>,
    /// The window's epoch at deletion, or `None` if it was never recorded --
    /// i.e. it was created after the last time window change functions ran.
    pub change_stamp: Option<ChangeStamp>,
}

/// GNU's policy for choosing the frame selected after deleting the currently
/// selected frame (`src/frame.c:2750-2821`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameDeletionSelectionPolicy {
    MostRecentlyUsed,
    FrameListOrder,
}

/// Process-wide selection change caused by removing a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectedFrameAfterDeletion {
    Unchanged,
    Replaced(FrameId),
    Cleared,
}

/// Result of a low-level frame removal.  Callers that also own buffer state
/// must handle `selected` and realign the current buffer with the surviving
/// frame's selected window, as GNU's `do_switch_frame` does before freeing the
/// deleted frame.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameDeletion {
    NotFound,
    Deleted {
        selected: SelectedFrameAfterDeletion,
    },
}

impl FrameDeletion {
    pub const fn was_deleted(self) -> bool {
        matches!(self, Self::Deleted { .. })
    }
}

// --- E5-FM-3b window sequence identity (stage-4 W2/E5) ---

pub(crate) const FIRST_WINDOW_ID: WindowId = WindowId(1);

/// GNU's observable `window.sequence_number`, separate from native identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowSequenceNumber {
    RetiredPreload,
    Runtime(std::num::NonZeroU64),
    /// Contextual diagnostics can be asked to print an unminted test handle.
    /// It has no owner-issued sequence; preserve its native diagnostic ID.
    Unminted(WindowId),
}

impl std::fmt::Display for WindowSequenceNumber {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RetiredPreload => 0_u64.fmt(formatter),
            Self::Runtime(sequence) => sequence.fmt(formatter),
            Self::Unminted(window) => window.0.fmt(formatter),
        }
    }
}

// --- E5-FM-3b Frame + FrameManager (stage-4 W2/E5) ---

/// A frame (top-level window/screen).
pub struct Frame {
    /// GNU `f->change_stamp`: the epoch bumped each time the window change
    /// functions run.  Starts at `FIRST` rather than "unset" because GNU's is
    /// already non-zero by the time any Lisp can observe it, which is what
    /// makes a window created afterwards (stamp 0) compare unequal.
    pub change_stamp: ChangeStamp,
    pub id: FrameId,
    /// GNU `struct frame.name`: a Lisp string used for resources and default
    /// title fallback.
    pub name: Value,
    /// GNU `struct frame.explicit_name`: whether the frame name came from an
    /// explicit Lisp-side parameter rather than an auto-generated `F<num>`
    /// fallback.
    pub explicit_name: bool,
    /// GNU `struct frame.icon_name`: explicit icon name, or nil.
    pub icon_name: Value,
    /// GNU `struct frame.focus_frame`: frame receiving this frame's keystrokes,
    /// or nil when focus is not redirected.
    pub focus_frame: Value,
    /// GNU `struct frame.parent_frame`: parent frame for child frames, or nil.
    pub parent_frame: Value,
    /// Terminal owner id for GNU `frame-terminal` / terminal lifecycle.
    pub terminal_id: u64,
    /// GNU `FRAME_INITIAL_P`: bootstrap placeholder frame that exists before
    /// a real terminal or window-system frame is installed.
    pub initial: bool,
    /// This frame's window tree.
    ///
    /// Private: reach the root through [`Frame::root_window`] and windows
    /// through [`Frame::find_window`], so that how the tree is stored stays
    /// inside [`WindowTree`].
    pub(crate) tree: WindowTree,
    /// The selected (active) window.
    pub selected_window: WindowId,
    /// The previously-selected window. GNU stores this as
    /// `frame->old_selected_window` and returns it from
    /// `frame-old-selected-window` (`src/frame.c`). Window audit
    /// Critical 8 in `drafts/window-system-audit.md` flagged the
    /// builtin as a stub returning nil because this field did not
    /// exist; the builtin now reads it.
    ///
    /// Initialized to `None` (nil) on a fresh frame to match GNU
    /// `make_frame_without_minibuffer`, then set to whichever
    /// window was previously selected on every `select-window`,
    /// `set-frame-selected-window`, and `set-window-configuration`
    /// transition.
    pub old_selected_window: Option<WindowId>,
    /// Minibuffer window (always a leaf).
    pub minibuffer_window: Option<WindowId>,
    /// Storage for the minibuffer leaf, which is not part of the split tree.
    pub minibuffer_leaf: Option<Window>,
    /// Frame pixel dimensions.
    pub width: u32,
    pub height: u32,
    /// Physical device pixels per logical Emacs pixel for this native frame.
    /// TTY and X11 frames use 1.0; Wayland can publish fractional values.
    pub device_scale_factor: f64,
    /// Pixel/cell position of the frame on its display, or relative to its
    /// parent for child frames.
    pub left_pos: i64,
    pub top_pos: i64,
    /// Internal window-system kind, mirroring GNU Emacs frame state rather
    /// than the mutable Lisp-visible frame parameter alist.
    pub window_system: Option<Value>,
    /// The Lisp-visible `window-system` parameter, cached beside the map entry.
    ///
    /// `effective_window_system` falls back to this parameter when no internal
    /// kind is installed, and geometry/layout code calls it dozens of times
    /// per command.  Reading the entry out of `parameters` on every call cost
    /// 171 instructions (hash + symbol comparison) in the C-n profile; this
    /// copy makes the accessor two field reads.  The parameter map is private
    /// to this module and only [`Frame::set_parameter`] and
    /// [`Frame::remove_parameter`] write it, so those two methods are the
    /// complete set of writers that keep this copy current.
    pub(crate) window_system_parameter: Option<Value>,
    /// Native display connection plus the X display inherited by child
    /// processes. These differ for a Wayland frame running alongside
    /// Xwayland, just as GNU's PGTK backend distinguishes its GDK display from
    /// the `DISPLAY` exported to subprocesses.
    pub(crate) display_identity: FrameDisplayIdentity,
    /// Frame parameters.
    ///
    /// Window audit Medium 12 in
    /// `drafts/window-system-audit.md`: GNU's
    /// `Fset_frame_parameter` calls into the per-toolkit
    /// backend (`x_set_*`, `pgtk_set_*`, etc.) for each parameter
    /// class (position, size, fonts, fullscreen, scroll bars).
    /// neomacs writes to this HashMap unconditionally, so a
    /// `(modify-frame-parameters f '((width . 100)))` call
    /// updates the parameter alist but does not always reach the
    /// active display backend. Wiring the dispatch is tracked as
    /// audit Phase 6.
    pub parameters: HashMap<Value, Value>,
    /// Accepted GUI opacity, independent of the raw Lisp parameter alist.
    /// GNU stores a new raw value before its handler can signal an error.
    pub background_alpha: f32,
    pub frame_alpha: [f32; 2],
    /// Whether the frame is visible, iconified, or invisible.
    pub visibility: FrameVisibility,
    /// Whether the menu / tab / tool bars are actually displayed and therefore
    /// occupy rows of the window text area (mirrors GNU realizing
    /// `FRAME_MENU_BAR_LINES` into `FRAME_TOP_MARGIN` only on a shown frame).
    /// Set on interactively displayed frames; left false for non-displayed
    /// frames (e.g. `--batch`), so window-edge coordinates match GNU there.
    pub displays_chrome: bool,
    /// GNU `struct frame.title`: explicit title override, or nil.
    pub title: Value,
    /// Retained projection policy, including while this frame is unselected.
    pub(crate) gui_bar_presentation: GuiBarPresentation,
    /// Menu bar height in pixels.
    pub menu_bar_height: u32,
    /// Tool bar height in pixels.
    pub tool_bar_height: u32,
    /// Compact bar height in pixels.
    pub compact_bar_height: u32,
    /// Tab bar height in pixels.
    pub tab_bar_height: u32,
    /// Default font size in pixels.
    pub font_pixel_size: f32,
    /// Default character width.
    pub char_width: f32,
    /// Default character height.
    pub char_height: f32,
    /// The default font's ascent in pixels: how far the text baseline sits
    /// below the top of a character cell.
    ///
    /// GNU's `FONT_BASE` for the frame's default face font (`FRAME_BASELINE` is
    /// `char_height`-relative, not a pixel count).  A row's height is
    /// `max (ascent) + max (descent)` over the elements on it
    /// (`move_it_in_display_line_to`, src/xdisp.c:11203-11207; the image branch
    /// is `produce_image_glyph`, src/xdisp.c:32447-32463), so a display element
    /// that rises above the baseline -- an image with `:ascent 100` -- needs
    /// this number to be measurable at all.  Without it the frame carried only
    /// the cell's *total* height and every consumer had to guess the split.
    ///
    /// Terminal frames keep the cell convention `descent = 0` that
    /// `neovm_bridge` uses for them (`font_ascent = char_height`); they display
    /// no images, so the split is unobservable there.
    pub font_ascent: f32,
    /// One-shot guard used when a live default-font change updates the frame's
    /// character metrics before GNU would commit the follow-up width/height
    /// window-system resize.
    pub defer_next_gui_parameter_resize: bool,
    /// Logical GUI resize requested via frame parameters but not yet committed
    /// to the live host window.
    pub pending_gui_resize: Option<PendingGuiResize>,
    /// Authoritative last-redisplay geometry keyed by live leaf window.
    ///
    /// Window audit Medium 10 / Medium 11 in
    /// `drafts/window-system-audit.md`: GNU keeps `change_stamp`,
    /// `use_time`, `sequence_number`, `old_pixel_width`,
    /// `old_pixel_height`, `old_body_pixel_width`,
    /// `old_body_pixel_height`, and `old_buffer` directly on
    /// `struct window`. neomacs centralizes the redisplay-time
    /// geometry inside `presentation_state` and the change-detection
    /// state inside `window_hook_record`. The fields below are the
    /// neomacs-side equivalents — adding the GNU names verbatim is
    /// tracked as future work in the audit's Phase 4 plan.
    pub(crate) presentation_state: FramePresentationState,
    /// Latest completed layout output used for incremental redisplay and GNU
    /// output bookkeeping. This cache is not renderer-active geometry.
    pub(crate) redisplay_cache: HashMap<WindowId, std::sync::Arc<WindowDisplaySnapshot>>,
    /// Exact accepted TTY frame-pool numeric cells, independent of a leaf's
    /// surviving redisplay cache. One frame mutator replaces a fully initialized
    /// Arc only at validated presentation prepare; concurrent readers retain
    /// immutable numeric views through the existing publication transport.
    /// Query-only layouts never publish new pool observations. Typed allocation
    /// adjustments update only TEXT current-row admission, preserving physical cells.
    pub(crate) tty_posn_pool: Option<Arc<TtyPosnCurrentOwner>>,
    /// GNU preserves pool-backed topology only when the accepted tree has no margins.
    pub(crate) tty_posn_pool_can_repartition: bool,
    /// Test-only numeric policy owned by this Frame's exclusive mutator.
    /// Independent contexts and frames never share mutable selectors; readers
    /// borrow immutable state. No Lisp values, TLS, or shipping fields exist.
    #[cfg(any(test, feature = "redisplay-test-policy"))]
    pub(crate) test_posn_object_extent_mode: Option<PosnObjectExtentMode>,
    /// Last recorded redisplay state for GNU window change hooks.
    pub(crate) window_hook_record: FrameWindowHookRecord,
    /// GNU `frame-window-state-change` flag.
    pub(crate) window_state_change: bool,
    /// Real frame-local Lisp face hash table, mirroring GNU `frame->face_hash_table`.
    pub face_hash_table: Value,
    /// Derived realization state, owned by this frame's exclusive mutator.
    pub(crate) face_realization: super::face::FrameFaceRealizationCache,
    /// Per-frame realized Lisp faces, mirroring GNU's `frame->face_hash_table`
    /// runtime surface for renderer-facing consumers.
    /// GNU `struct frame.z_order`: stacking order among sibling child frames.
    pub z_order: i32,
    /// Whether a child frame suppresses the TTY decoration border.
    pub undecorated: bool,
    /// Non-focusable frame hint.  The TTY path stores it for Lisp-visible
    /// parameter parity; input focus policy is handled elsewhere.
    pub no_accept_focus: bool,
    /// Unsplittable frame hint.
    pub no_split: bool,
    /// GNU `struct frame.buffer_list`: buffers most-recently shown in this
    /// frame, in most-recently-shown-first order.  Updated by
    /// `bury-buffer-internal` and cleaned up on buffer kill.
    pub buffer_list: Vec<BufferId>,
    /// GNU `struct frame.buried_buffer_list`: buffers buried in this frame,
    /// in most-recently-buried-first order.  Updated by
    /// `bury-buffer-internal` and cleaned up on buffer kill.
    pub buried_buffer_list: Vec<BufferId>,
}

pub struct FrameManager {
    pub(crate) frames: HashMap<FrameId, Frame>,
    /// Called on each newly built Frame before it is inserted. The Lisp
    /// layer injects xfaces::init_frame_lisp_faces here (GNU calls
    /// init_frame_faces from the frame.c creation paths, not make_frame);
    /// display-side FrameManagers leave it unset.
    pub(crate) frame_init_hook: Option<fn(&mut Frame)>,
    pub(crate) selected: Option<FrameId>,
    /// GNU's `tty_display_info::top_frame`, indexed by terminal identity.
    /// A process has one globally selected frame, but every text terminal must
    /// remember which root frame owns its display while another terminal is
    /// selected.
    pub(crate) terminal_top_frames: HashMap<u64, FrameId>,
    pub(crate) next_frame_id: u64,
    pub(crate) last_tty_frame_name: Option<TtyFrameNameOrdinal>,
    pub(crate) next_window_id: u64,
    pub(crate) window_sequence_origin: WindowSequenceOrigin,
    pub(crate) old_selected_window: Option<WindowId>,
    /// Windows that were live and have been deleted, with the buffer each was
    /// showing when it went.
    ///
    /// GNU keeps the deleted window object itself and sets `w->old_buffer` from
    /// `w->contents` in `Fdelete_window_internal` (`src/window.c`), which is
    /// what lets `window-old-buffer` still answer for it.  neomacs drops the
    /// window from the tree, so the buffer has to be recorded here instead --
    /// a set of ids alone cannot answer the question.
    pub(crate) deleted_windows: HashMap<WindowId, DeletedWindowRecord>,
    pub(crate) deleted_window_parameters: HashMap<WindowId, WindowParameters>,
    pub(crate) window_select_count: i64,
    pub(crate) next_navigation_intent_generation: std::num::NonZeroU64,
    pub(crate) pending_content_transition_intents: PendingContentTransitionIntents,
    /// Monotonic identity of the live window-tree topology. Structural edits
    /// are rare and pay the increment; redisplay can then reject a stale leaf
    /// plan in O(1) after arbitrary Lisp callbacks.
    pub(crate) window_topology_generation: u64,
}

// --- E5-FM-6b snapshot/record impls (stage-4 W2/E5) ---

impl DisplayPointSnapshot {
    /// The column a coordinate query reports for a click that resolved here.
    ///
    /// GNU keeps counting columns once the click is past the display element
    /// its walk came to rest on. `buffer_posn_from_coords` closes with
    ///
    /// ```c
    ///   x1 = max (0, it.current_x + it.pixel_width);
    ///   if (to_x > x1)
    ///     it.hpos += (to_x - x1) / WINDOW_FRAME_COLUMN_WIDTH (w);
    ///   *x = it.hpos;
    /// ```
    ///
    /// (src/dispnew.c:6428-6432), and every click in the empty area under a
    /// short buffer is past the end of a line. Measured, GNU Emacs 31.0.90,
    /// 80x24 pty, `"abcdef\nghijkl\n"`: column 40 of row 0 reports column 40
    /// and column 79 reports 79, both for buffer position 7.
    ///
    /// Inside a wide element the same answer arrives by the other route rather
    /// than by this one: GNU's per-glyph loop does `++it->hpos` for each column
    /// a TAB or a double-width character occupies (src/xdisp.c:10635-10637), so
    /// `it.hpos` is already the clicked column and `x1` is past it. Both routes
    /// land on the click, which is why this one formula reproduces both --
    /// measured, `"ab\tcd\t\n"` column 5, inside the TAB that starts at column
    /// 2: GNU answers `(3 (5 . 0) (5 . 0))`.
    ///
    /// `it.pixel_width` is taken as zero. That is GNU's value wherever the walk
    /// rests on a line terminator, because a newline sets `it->pixel_width =
    /// it->nglyphs = 0` (src/term.c:1673-1674), and on a row that drew nothing
    /// at all. It is not GNU's value on the last line of a buffer with no final
    /// newline, where the walk stopped because `get_next_display_element`
    /// failed and the field still holds the last glyph's width; that case is
    /// ledger 205's named residual.
    pub fn column_for_click(&self, click_x: i64, column_width: i64) -> i64 {
        let past_end = click_x.saturating_sub(self.x).max(0);
        self.col.saturating_add(past_end / column_width.max(1))
    }
}

impl DisplayRowEndSource {
    pub const fn is_display_string(self) -> bool {
        matches!(self, Self::DisplayStringNewline | Self::DisplayStringWrap)
    }

    pub const fn is_overlay_string(self) -> bool {
        matches!(self, Self::OverlayBeforeString | Self::OverlayAfterString)
    }
}

impl CompletedEobOverlayMotionExtent {
    /// Refuse a completion before the stream's first physical row rather than
    /// turning an invalid interval into a large unsigned distance.
    pub const fn from_physical_rows(
        anchor: LispCharPos1,
        first_row: usize,
        completion_row: usize,
    ) -> Option<Self> {
        match completion_row.checked_sub(first_row) {
            Some(following_rows) => Some(Self {
                anchor,
                following_rows,
            }),
            None => None,
        }
    }

    pub const fn anchor(self) -> LispCharPos1 {
        self.anchor
    }

    pub const fn following_rows(self) -> usize {
        self.following_rows
    }
}

impl PresentedWindowChromeString {
    pub fn new(
        area: PresentedWindowChromeArea,
        string_id: neomacs_display_protocol::glyph_matrix::GlyphStringId,
        object: neovm_value::tagged::transport::SharedRoot,
    ) -> Self {
        Self {
            area,
            string_id,
            object,
        }
    }

    pub const fn area(&self) -> PresentedWindowChromeArea {
        self.area
    }

    pub const fn string_id(&self) -> neomacs_display_protocol::glyph_matrix::GlyphStringId {
        self.string_id
    }

    /// The rooted string object; the evaluator materializes it.
    pub fn object(&self) -> &neovm_value::tagged::transport::SharedRoot {
        &self.object
    }

    /// Share one private vector lease across all areas of a window snapshot.
    /// Existing handles keep every child alive during allocation, and are
    /// replaced only after successful admission. An already shared lease is
    /// reused, including whole-window chrome reused from a previous frame.
    ///
    /// # Errors
    /// A missing installed heap, or a source belonging to another evaluator.
    pub fn coalesce_roots(
        sources: &mut [Self],
        evaluator: &crate::emacs_core::Context,
    ) -> Result<(), neovm_value::tagged::transport::SharedRootError> {
        for source in sources.iter() {
            let _local = evaluator.materialize(&source.object)?;
        }
        let roots: Vec<_> = sources.iter().map(|source| &source.object).collect();
        let replacement =
            neovm_value::tagged::transport::SharedRoot::coalesce_on_current_mutator(&roots)?;
        if let Some(replacement) = replacement {
            for (source, root) in sources.iter_mut().zip(replacement) {
                source.object = root;
            }
        }
        Ok(())
    }
}

/// Snapshot equality keeps GNU `equal` on the string objects, as when the
/// field held the raw value: an evaluator comparing two snapshots treats a
/// re-formatted, equal mode line as unchanged. Off the evaluator only object
/// identity is observable.
impl PartialEq for PresentedWindowChromeString {
    fn eq(&self, other: &Self) -> bool {
        self.area == other.area
            && self.string_id == other.string_id
            && (self.object.is_same_object(&other.object)
                || self
                    .object
                    .value_on_current_mutator()
                    .zip(other.object.value_on_current_mutator())
                    .is_some_and(|(left, right)| left == right))
    }
}

impl Eq for PresentedWindowChromeString {}

impl PresentedWindowChromeStrings {
    /// Borrow the immutable sources without copying their rooted handles.
    #[inline]
    pub fn as_slice(&self) -> &[PresentedWindowChromeString] {
        self.0.as_deref().unwrap_or(&[])
    }
}

impl From<Vec<PresentedWindowChromeString>> for PresentedWindowChromeStrings {
    fn from(sources: Vec<PresentedWindowChromeString>) -> Self {
        if sources.is_empty() {
            Self::default()
        } else {
            Self(Some(sources.into()))
        }
    }
}

impl AsRef<[PresentedWindowChromeString]> for PresentedWindowChromeStrings {
    fn as_ref(&self) -> &[PresentedWindowChromeString] {
        self.as_slice()
    }
}

impl std::ops::Deref for PresentedWindowChromeStrings {
    type Target = [PresentedWindowChromeString];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl PartialEq for PresentedWindowChromeStrings {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (None, None) => true,
            (Some(left), Some(right)) => Arc::ptr_eq(left, right) || left == right,
            (None, Some(_)) | (Some(_), None) => false,
        }
    }
}

impl Eq for PresentedWindowChromeStrings {}

impl WindowDisplaySnapshotFreshness {
    /// Exact pixel-placement change for a query over the same source rows.
    /// Unlike scroll-surface compatibility, this preserves point, start and
    /// redisplay revisions. The consumer must separately prove row coverage.
    pub fn query_vscroll_delta(&self, current: &Self) -> Option<i64> {
        let mut placed = current.clone();
        placed.window.vscroll = self.window.vscroll;
        placed.window.preserve_vscroll_p = self.window.preserve_vscroll_p;
        (self == &placed)
            .then(|| i64::from(current.window.vscroll) - i64::from(self.window.vscroll))
    }

    /// Row measurements may change placement, but not the point-dependent
    /// source context. The producer must also validate its captured reads.
    pub fn same_query_row_content(&self, current: &Self) -> bool {
        self.window.point == current.window.point && self.same_scroll_content(current)
    }

    /// Row geometry for a position query when only point and repaint requests
    /// changed.
    ///
    /// A command moves point and then asks `posn-at-point` /
    /// `pos-visible-in-window-p` before redisplay runs. Overlay revisions must
    /// still match: even face-only overlays can change font metrics and wrapping.
    /// Every actual row input -- text, properties, accessibility, prefixes,
    /// display table, faces, fonts, geometry -- must still match, and BOTH
    /// tokens must certify that the overlays present could only change faces
    /// (an overlay with `display`/`invisible`/`category` never qualifies). The
    /// caller must separately rule out point-dependent chrome and line-number
    /// metrics: unchanged format/face definitions can still yield new geometry.
    pub fn same_position_rows_ignoring_point(&self, current: &Self) -> bool {
        if !(self.overlays_face_only && current.overlays_face_only) {
            return false;
        }
        let mut expected = current.clone();
        // Point-dependent geometry is checked by the position-query caller.
        expected.window.point = self.window.point;
        // A request to repaint is not a body-content mutation.
        expected.redisplay_generation = self.redisplay_generation;
        self == &expected
    }

    /// A committed scroll may move start/point and hide part of the first row.
    /// Every source, font, geometry, topology and mutation revision must still
    /// match before an existing row surface can preview that destination.
    pub fn same_scroll_content(&self, current: &Self) -> bool {
        let mut placed = current.clone();
        placed.window.window_start = self.window.window_start;
        placed.window.point = self.window.point;
        placed.window.vscroll = self.window.vscroll;
        placed.window.preserve_vscroll_p = self.window.preserve_vscroll_p;
        // A request to repaint echo/chrome is not a body-content mutation.
        // The source, face, display-variable, media and function revisions
        // below remain authoritative, as do all window and frame inputs.
        placed.redisplay_generation = self.redisplay_generation;
        self == &placed
    }
}

impl WindowDisplaySnapshot {
    /// Iterate canonical source order without materializing shared row cells.
    pub fn iter_points(&self) -> impl ExactSizeIterator<Item = DisplayPointSnapshot> + '_ {
        super::point_rows::WindowDisplayPointIter::new(&self.points, self.point_rows.as_ref())
    }

    pub fn point_count(&self) -> usize {
        self.point_rows
            .as_ref()
            .map_or_else(|| self.points.len(), DisplayPointRows::point_count)
    }

    pub fn has_points(&self) -> bool {
        self.point_rows.as_ref().map_or_else(
            || !self.points.is_empty(),
            |rows| rows.rows.iter().any(|row| row.point_count() != 0),
        )
    }

    /// Source-order points for one row, without scanning other compact rows.
    pub fn iter_row_points(&self, row: i64) -> impl Iterator<Item = DisplayPointSnapshot> + '_ {
        let mut compact = self
            .point_rows
            .as_ref()
            .and_then(|rows| rows.row(row))
            .map(DisplayPointRow::points);
        let shared = self.point_rows.is_some();
        let mut flat = self.points.iter();
        std::iter::from_fn(move || {
            if let Some(points) = compact.as_mut() {
                return points.next();
            }
            if shared {
                return None;
            }
            flat.find(|point| point.row == row).cloned()
        })
    }

    /// Cold scratch mutation boundary: flatten once and discard row storage.
    pub fn materialize_points_mut(&mut self) -> &mut Vec<DisplayPointSnapshot> {
        if let Some(rows) = self.point_rows.take() {
            self.points = rows.iter_points().collect();
        }
        &mut self.points
    }

    pub fn clear_point_rows(&mut self) {
        self.point_rows = None;
    }

    /// Replace the flat points and invalidate any previous shared representation.
    pub fn set_points(&mut self, points: Vec<DisplayPointSnapshot>) {
        self.point_rows = None;
        self.points = points;
    }

    /// Height of header/tab chrome above the window's text body.
    pub fn top_chrome_height(&self) -> i64 {
        self.header_line_height
            .max(0)
            .saturating_add(self.tab_line_height.max(0))
    }

    /// Number of rendered header/tab rows above the window's text body.
    pub fn top_chrome_rows(&self) -> i64 {
        i64::from(self.header_line_height > 0) + i64::from(self.tab_line_height > 0)
    }

    /// Convert a redisplay Y relative to the window top into GNU's text-area
    /// coordinate space used by text `posn` values.
    pub fn text_area_relative_y(&self, window_relative_y: i64) -> i64 {
        window_relative_y.saturating_sub(self.top_chrome_height())
    }

    /// Convert a redisplay row relative to the window top into GNU's text-area
    /// row used by text `posn` values.
    pub fn text_area_relative_row(&self, window_relative_row: i64) -> i64 {
        window_relative_row.saturating_sub(self.top_chrome_rows())
    }

    /// Resolve the published body-row entry for an output row.
    ///
    /// `body_rows` is sorted by `output_row` and deduplicated at
    /// construction (window_output.rs seals every snapshot through the same
    /// sort+dedup), so a binary search is exact. Hit-test compilation calls
    /// this once per glyph point — a linear scan here was O(points x rows)
    /// per window per frame.
    pub fn body_row_for_output_row(&self, output_row: i64) -> Option<&PresentedBodyRowSnapshot> {
        self.body_rows
            .binary_search_by_key(&output_row, |row| row.output_row)
            .ok()
            .map(|idx| &self.body_rows[idx])
    }

    /// Resolve the body-local coordinates published for an output row.
    ///
    /// New redisplay producers publish this mapping directly.  The chrome
    /// subtraction is retained only for snapshots from producers that do not
    /// yet materialize body rows (notably the synchronous TTY path).
    pub fn text_body_position(&self, output_row: i64, window_y: i64) -> (i64, i64) {
        self.body_row_for_output_row(output_row).map_or_else(
            || {
                (
                    self.text_area_relative_row(output_row),
                    self.text_area_relative_y(window_y),
                )
            },
            |row| (row.body_row, row.body_y),
        )
    }

    pub fn logical_cursor_pos(&self) -> Option<WindowCursorPos> {
        self.logical_cursor.or_else(|| {
            self.phys_cursor
                .as_ref()
                .map(WindowCursorPos::from_snapshot)
        })
    }

    pub fn visible_buffer_span(&self) -> Option<WindowVisibleBufferSpan> {
        let start = self
            .rows
            .iter()
            .find_map(|row| row.start_buffer_pos)
            .or_else(|| self.iter_points().next().map(|point| point.buffer_pos))?;
        // A right-truncated last row owns the tail it skipped (GNU's
        // `row->end` lies past it, src/xdisp.c:26695,26736), so the span the
        // window can answer for extends through that tail.
        let end = self
            .rows
            .iter()
            .rev()
            .find_map(|row| row.truncated_end_buffer_pos.or(row.end_buffer_pos))
            .or_else(|| self.iter_points().last().map(|point| point.buffer_pos))?;
        Some(WindowVisibleBufferSpan::new(start, end))
    }

    /// The row whose source span holds POS: through its last drawn glyph, or
    /// through the tail a right-truncated row skipped (the line terminator, or
    /// ZV when the truncated line ends the buffer), which GNU's
    /// `pos_visible_p` still lands on that row (src/xdisp.c:11122-11146).
    fn row_for_buffer_pos(&self, pos: LispCharPos1) -> Option<&DisplayRowSnapshot> {
        self.rows.iter().find(|row| {
            let Some(start) = row.start_buffer_pos else {
                return false;
            };
            let Some(end) = row.truncated_end_buffer_pos.or(row.end_buffer_pos) else {
                return false;
            };
            start <= pos && pos <= end
        })
    }

    /// Whether POS is on one of this snapshot's rows but left of the first
    /// glyph that row drew: hidden by the window's hscroll.  GNU `pos_visible_p`
    /// still reports such a position, at its x left of the text area
    /// (src/xdisp.c:1772-1830 with the iterator's line-relative x), so a
    /// snapshot that has no point for it has not answered; the caller re-walks
    /// as GNU does on every call.  A row's `start_buffer_pos` is its LINE
    /// start even when hscrolled (`note_row_walk_start`), which is what
    /// bounds the hidden prefix on the left; on the right it is the first
    /// glyph the row DREW -- the left-truncation `$` is an overlaid marker
    /// standing for the glyph that straddles the boundary, and GNU reports
    /// that glyph's own position at its hidden x too (a tab at columns 2-7
    /// under hscroll 3: (-1 1), `posn-point' nil; GNU 31.1).
    pub fn position_hidden_by_hscroll(&self, pos: LispCharPos1) -> bool {
        self.rows.iter().any(|row| {
            row.start_buffer_pos.is_some_and(|start| start <= pos)
                && self
                    .iter_row_points(row.row)
                    .find(|point| point.role.is_position())
                    .is_some_and(|first_drawn| pos < first_drawn.buffer_pos)
        })
    }

    /// Whether POS lies in the tail ROW skipped by right truncation: after its
    /// last drawn glyph and no further than the terminator (or ZV).
    fn pos_in_truncated_tail(row: &DisplayRowSnapshot, pos: LispCharPos1) -> bool {
        row.end_buffer_pos.is_some_and(|end| pos > end)
            && row.truncated_end_buffer_pos.is_some_and(|tail| pos <= tail)
    }

    /// The display point GNU reports for a position in ROW's truncated tail.
    ///
    /// `pos_visible_p` (src/xdisp.c:1772-1830) runs `move_it_to` to the
    /// position; on a truncated line `move_it_in_display_line_to` stops at the
    /// truncation column with MOVE_LINE_TRUNCATED, `move_it_to` reseats past
    /// the line and, now at or past the position, returns with the iterator's
    /// x still at that column and its y still on the row (xdisp.c:11122-11146,
    /// `reached = 9` before the row advance).  So the position is visible at
    /// the truncation marker's column of its row -- the `$` slot the walk
    /// published as an overlaid marker point, whose own position (the first
    /// character the row did not draw) is what `posn-at-point` then reports
    /// as `posn-point` (GNU 31.1: 102938 for every point of a tail starting
    /// there) -- and on a frame that shows no marker glyph, at the column the
    /// row stopped drawing.
    fn truncated_tail_point(&self, row: &DisplayRowSnapshot) -> Option<DisplayPointSnapshot> {
        let marker = self
            .iter_row_points(row.row)
            .filter(|point| point.role == DisplayPointRole::OverlaidMarker)
            .last();
        marker.or_else(|| {
            let first_hidden = row.end_buffer_pos?.as_i64().checked_add(1)?;
            self.iter_row_points(row.row)
                .last()
                .map(|last| DisplayPointSnapshot {
                    buffer_pos: LispCharPos1::new(first_hidden),
                    role: DisplayPointRole::OverlaidMarker,
                    x: row.end_x,
                    y: last.y,
                    width: last.width,
                    height: last.height,
                    row: row.row,
                    col: row.end_col,
                })
        })
    }

    /// Fringe bitmaps of the display row containing POS.
    ///
    /// `None` means POS is not on any row of this snapshot, which is GNU's
    /// "no row containing pos" case — `fringe-bitmaps-at-pos` returns nil.
    /// A row that simply carries no bitmaps yields a default (all-absent)
    /// record, which GNU reports as `(nil nil nil)`.
    pub fn fringe_bitmaps_for_buffer_pos(&self, pos: LispCharPos1) -> Option<RowFringeBitmaps> {
        self.row_for_buffer_pos(pos).map(|row| row.fringe)
    }

    /// Return the visible point for POS, or the nearest visible neighbor when
    /// POS itself is hidden by redisplay within the visible span.
    ///
    /// Off-window positions return `None`, matching GNU Emacs `posn-at-point`
    /// and `pos-visible-in-window-p` semantics.
    pub fn point_for_buffer_pos(&self, pos: LispCharPos1) -> Option<DisplayPointSnapshot> {
        if !self.has_points() {
            return None;
        }
        let visible_span = self.visible_buffer_span()?;
        if pos < visible_span.start() || pos > visible_span.end() {
            return None;
        }
        // Drawn glyphs take precedence over marker columns across all rows.
        // Source ties retain walk order, including bidi and replacing strings.
        let exact = if let Some(rows) = &self.point_rows {
            rows.point_for_buffer_pos(pos)
        } else {
            let idx = self.points.partition_point(|point| point.buffer_pos < pos);
            let run = &self.points[idx..];
            let run = &run[..run.partition_point(|point| point.buffer_pos == pos)];
            run.iter()
                .find(|point| point.role.is_position())
                .or_else(|| run.first())
                .cloned()
        };
        if exact.is_some() {
            return exact;
        }
        let row = self.row_for_buffer_pos(pos)?;
        if Self::pos_in_truncated_tail(row, pos) {
            return self.truncated_tail_point(row);
        }
        let next = self
            .iter_row_points(row.row)
            .find(|point| point.buffer_pos > pos);
        let previous = self
            .iter_row_points(row.row)
            .filter(|point| point.buffer_pos < pos)
            .last();
        match (previous, next) {
            (Some(_), Some(next)) => Some(next),
            _ => None,
        }
    }

    /// Which published row a window-relative Y belongs to.
    ///
    /// GNU never puts this question to a glyph matrix. `buffer_posn_from_coords`
    /// runs an ITERATOR from the window's own start
    /// (`start_display` + `move_it_to (&it, -1, 0, *y, -1, MOVE_TO_X | MOVE_TO_Y)`,
    /// src/dispnew.c:6278-6285), so a Y below every line the buffer can produce
    /// does not fall off the end of a list — it makes the walk run out of
    /// BUFFER: `move_it_in_display_line_to` breaks with `MOVE_POS_MATCH_OR_ZV`
    /// the moment `get_next_display_element` fails ("Stop when ZV reached",
    /// src/xdisp.c:10251-10258) and `move_it_to` leaves through its ZV exits
    /// (`reached = 5/7/8`, src/xdisp.c:10984/11030/11107). `*pos = it.current`
    /// is then point-max (src/dispnew.c:6353), and `it.vpos` is the row the
    /// iterator stopped on rather than the row that was asked about.
    ///
    /// A port that answers from ROWS has to name that case, because "no row
    /// contains this Y" is where its lookup ends and GNU's walk does not.
    /// Making it a variant rather than a `None` is what forces
    /// [`Self::point_at_coords`] to decide, instead of reporting GNU's ZV
    /// answer as no answer at all.
    pub fn row_at_y(&self, y: i64) -> WindowSnapshotRowAtY<'_> {
        if let Some(row) = self
            .rows
            .iter()
            .find(|row| y >= row.y && y < row.y.saturating_add(row.height.max(1)))
        {
            return WindowSnapshotRowAtY::Within(row);
        }
        // Only rows that carry buffer text can answer: the mode line, header
        // line and tab line are published here too and own no position, which
        // is also why GNU asks `window_from_coordinates` for the window PART
        // before it asks `buffer_posn_from_coords` anything
        // (src/keyboard.c:5793 and 5862-5975).
        match self
            .rows
            .iter()
            .filter(|row| {
                row.end_buffer_pos.is_some() && y >= row.y.saturating_add(row.height.max(1))
            })
            .max_by_key(|row| (row.y, row.row))
        {
            Some(row) => WindowSnapshotRowAtY::BelowLastTextRow(row),
            None => WindowSnapshotRowAtY::NoTextRow,
        }
    }

    /// The position a row ends at, with the geometry redisplay published for
    /// it — GNU's `it.current` when its walk comes to rest at ZV.
    ///
    /// Ledger 204 made every row publish a slot for its own terminator, so the
    /// end of a newline-terminated row, of the empty end-of-buffer row and of a
    /// last line with no terminator at all are all present in `points`; the
    /// constructed fallback is for a row whose end was recorded without one.
    fn row_end_point(&self, row: &DisplayRowSnapshot) -> Option<DisplayPointSnapshot> {
        let end = row.end_buffer_pos?;
        if let Some(point) = self
            .iter_row_points(row.row)
            .filter(|point| point.buffer_pos == end)
            .last()
        {
            return Some(point);
        }
        Some(DisplayPointSnapshot {
            role: if self.posn_object_extent_mode().enabled() {
                DisplayPointRole::SyntheticBoundary
            } else {
                DisplayPointRole::Glyph
            },
            buffer_pos: end,
            x: row.end_x,
            y: row.y,
            width: 0,
            height: row.height.max(1),
            row: row.row,
            col: row.end_col,
        })
    }

    /// Return the visible point nearest to a classified text-area coordinate.
    ///
    /// This is GNU's `buffer_posn_from_coords` (src/dispnew.c:6261-6437) and it
    /// is reachable only from a coordinate a window-part classification has
    /// placed in the text area -- GNU asks `window_from_coordinates` for the
    /// part FIRST and only the parts that are not chrome lines reach this
    /// (src/keyboard.c:5793 and 5975-6000). [`TextAreaCoordinate`] has no other
    /// public constructor, so the mode line, header line and tab line cannot
    /// arrive here at all.
    pub fn point_at_coords(&self, at: TextAreaCoordinate) -> Option<DisplayPointSnapshot> {
        let x = at.text_area_x();
        let y = at.window_y();
        let row = match self.row_at_y(y) {
            WindowSnapshotRowAtY::Within(row) => row,
            // GNU's ZV exit: the walk ran out of buffer above this Y, so the
            // answer is the last text row's own end, which is point-max.
            WindowSnapshotRowAtY::BelowLastTextRow(row) => return self.row_end_point(row),
            WindowSnapshotRowAtY::NoTextRow => return None,
        };
        let compact = self.point_rows.as_ref().and_then(|rows| rows.row(row.row));
        let mut flat_points = if self.point_rows.is_none() {
            self.iter_row_points(row.row).collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        flat_points.sort_by_key(|point| (point.x, point.col, point.buffer_pos));
        let mut compact_points = compact.map(DisplayPointRow::points_x_order);
        let mut flat_points = flat_points.into_iter();
        let mut row_points = std::iter::from_fn(move || match &mut compact_points {
            Some(points) => points.next(),
            None => flat_points.next(),
        });
        let Some(mut last) = row_points.next() else {
            return row.start_buffer_pos.map(|buffer_pos| DisplayPointSnapshot {
                role: if self.posn_object_extent_mode().enabled() {
                    DisplayPointRole::SyntheticBoundary
                } else {
                    DisplayPointRole::Glyph
                },
                buffer_pos,
                x: row.start_x,
                y: row.y,
                width: 0,
                height: row.height.max(1),
                row: row.row,
                col: row.start_col,
            });
        };
        if x <= last.x {
            return Some(last);
        }
        for point in row_points {
            let right = last.x.saturating_add(last.width.max(1));
            if x < right {
                return Some(last);
            }
            if x < point.x {
                return Some(last);
            }
            last = point;
        }
        Some(last)
    }

    /// Row metrics for visual row ROW.
    pub fn row_metrics(&self, row: i64) -> Option<&DisplayRowSnapshot> {
        self.rows.iter().find(|metrics| metrics.row == row)
    }

    /// The row the last redisplay published for one of this window's chrome
    /// lines, if it drew one.
    ///
    /// GNU reaches the same row through `MATRIX_MODE_LINE_ROW` /
    /// `MATRIX_TAB_LINE_ROW` / `MATRIX_HEADER_LINE_ROW`
    /// (src/dispextern.h:1159-1175) and then tests `row->mode_line_p &&
    /// row->enabled_p` (src/dispnew.c:6462). A window whose matrix has been
    /// allocated but never filled fails that test and answers column 0 with
    /// zero width and height (src/dispnew.c:6497-6502); that is `None` here,
    /// and it is why an un-redisplayed window's mode line reports column 0 for
    /// every X in GNU as well.
    pub fn chrome_row(
        &self,
        line: WindowChromeLine,
        window_height: i64,
    ) -> Option<&DisplayRowSnapshot> {
        let (top, bottom) = match line {
            WindowChromeLine::TabLine => (0, self.tab_line_height.max(0)),
            WindowChromeLine::HeaderLine => (
                self.tab_line_height.max(0),
                self.tab_line_height.max(0) + self.header_line_height.max(0),
            ),
            WindowChromeLine::ModeLine => {
                (window_height - self.mode_line_height.max(0), window_height)
            }
        };
        if top >= bottom {
            return None;
        }
        self.rows
            .iter()
            .find(|row| row.y >= top && row.y < bottom && row.end_buffer_pos.is_none())
    }

    /// GNU `mode_line_string` (src/dispnew.c:6444-6519): what a click on one of
    /// this window's chrome lines reports.
    ///
    /// Unlike the text area, GNU never re-runs a walk for this -- it reads the
    /// window's CURRENT MATRIX and nothing else, which is why a window that has
    /// not been redisplayed answers column 0 and row 0 for its header line and
    /// column 0 for its mode line no matter where the click was. This port's
    /// retained snapshot is that matrix, so the same asymmetry holds: the
    /// caller must pass the RETAINED snapshot here even where it would run a
    /// fresh layout for a text coordinate.
    pub fn chrome_line_hit(
        &self,
        line: WindowChromeLine,
        window_x: i64,
        window_y: i64,
        window_height: i64,
        line_height: i64,
        column_width: i64,
    ) -> ChromeLineHit {
        let line_height = line_height.max(1);
        let column_width = column_width.max(1);
        // `*y = row - MATRIX_FIRST_TEXT_ROW (w->current_matrix)`
        // (src/dispnew.c:6460): the chrome row's own index in the matrix, minus
        // the number of rows the matrix reserves above the text. Both come from
        // the matrix, so a window that has never been redisplayed reserves
        // none -- which is why GNU answers row 0 for a header line cold and
        // row -1 for the same click warm.
        let row_index = match line {
            WindowChromeLine::TabLine => 0,
            WindowChromeLine::HeaderLine => i64::from(self.tab_line_height > 0),
            WindowChromeLine::ModeLine => (window_height / line_height) - 1,
        };
        let row = row_index - self.top_chrome_rows();
        let Some(published) = self.chrome_row(line, window_height) else {
            return ChromeLineHit {
                col: 0,
                row,
                dx: 0,
                dy: window_y,
                width: 0,
                height: 0,
            };
        };
        // "Find the glyph under X" (src/dispnew.c:6465-6470), then "Add extra
        // (default width) columns if clicked after EOL" (src/dispnew.c:6496).
        // Every glyph a terminal chrome row draws is one column wide, so the
        // walk over the row's glyphs and the division by the column width
        // agree; a chrome glyph wider than one column would separate them, and
        // this port's chrome rows publish their extent rather than their
        // individual glyphs.
        let (col, dx, width) = if window_x < published.end_x {
            let offset = (window_x - published.start_x).max(0);
            (
                published.start_col + offset / column_width,
                offset % column_width,
                column_width,
            )
        } else {
            let offset = window_x - published.end_x;
            (published.end_col + offset / column_width, offset, 0)
        };
        ChromeLineHit {
            col,
            row,
            dx,
            dy: window_y - published.y,
            width,
            height: published.height.max(1),
        }
    }
}

impl PartialEq for WindowDisplaySnapshot {
    fn eq(&self, other: &Self) -> bool {
        #[cfg(any(test, feature = "redisplay-test-policy"))]
        if self.posn_object_extent_mode() != other.posn_object_extent_mode() {
            return false;
        }
        self.window_id == other.window_id
            && self.cell_origin == other.cell_origin
            && self.regions == other.regions
            && self.regions_materialized == other.regions_materialized
            && self.body_rows == other.body_rows
            && self.text_area_left_offset == other.text_area_left_offset
            && self.mode_line_height == other.mode_line_height
            && self.header_line_height == other.header_line_height
            && self.tab_line_height == other.tab_line_height
            && self.chrome_strings == other.chrome_strings
            && self.logical_cursor == other.logical_cursor
            && self.logical_cursor_dimensions == other.logical_cursor_dimensions
            && self.phys_cursor == other.phys_cursor
            && self.rows == other.rows
            && self.eob_overlay_motion == other.eob_overlay_motion
            && self.buffer_modiff == other.buffer_modiff
            && self.layout_freshness == other.layout_freshness
            && self.window_end_record == other.window_end_record
            && self.posn_matrix == other.posn_matrix
            && self.iter_points().eq(other.iter_points())
    }
}

impl Default for WindowDisplaySnapshot {
    fn default() -> Self {
        Self {
            window_id: WindowId(0),
            cell_origin: CellOrigin::default(),
            regions: PresentedWindowRegions::default(),
            regions_materialized: false,
            body_rows: Vec::new(),
            text_area_left_offset: 0,
            mode_line_height: 0,
            header_line_height: 0,
            tab_line_height: 0,
            chrome_strings: PresentedWindowChromeStrings::default(),
            logical_cursor: None,
            logical_cursor_dimensions: None,
            phys_cursor: None,
            points: Vec::new(),
            point_rows: None,
            rows: Vec::new(),
            eob_overlay_motion: EobOverlayMotionCoverage::NotObserved,
            buffer_modiff: None,
            layout_freshness: None,
            posn_matrix: None,
            #[cfg(any(test, feature = "redisplay-test-policy"))]
            test_posn_object_extent_mode: None,
            window_end_record: None,
        }
    }
}

impl PendingGuiResize {
    pub fn is_queued(&self) -> bool {
        matches!(self.delivery, GuiResizeDelivery::Queued)
    }

    /// Geometry queries may wait once for a newly sent request. An observation
    /// consumes that wait without implying which native request produced it.
    pub fn take_native_wait(&mut self) -> bool {
        match &mut self.delivery {
            GuiResizeDelivery::Queued => false,
            GuiResizeDelivery::Requested { wait, .. } => std::mem::take(wait),
        }
    }

    pub fn after_native_observation(mut self, width: u32, height: u32) -> Option<Self> {
        if let GuiResizeDelivery::Requested {
            width: target_width,
            height: target_height,
            wait,
        } = &mut self.delivery
        {
            if (width, height) == (*target_width, *target_height) {
                return None;
            }
            *wait = false;
        }
        // Actual allocation is always installed independently. Until the latest
        // target is attained, a later font change retains the requested grid.
        Some(self)
    }
}

impl TtyFrameNameOrdinal {
    pub(crate) const INITIAL: Self = Self(std::num::NonZeroU64::MIN);

    pub(crate) fn next(self) -> Self {
        let next = self
            .0
            .get()
            .checked_add(1)
            .expect("terminal frame name ordinal overflow");
        Self(std::num::NonZeroU64::new(next).expect("increment stays nonzero"))
    }

    pub(crate) fn value(self) -> Value {
        Value::string(format!("F{}", self.0))
    }
}

impl Default for FrameManager {
    fn default() -> Self {
        Self {
            frames: HashMap::default(),
            frame_init_hook: None,
            selected: None,
            terminal_top_frames: HashMap::default(),
            next_frame_id: FRAME_ID_BASE,
            last_tty_frame_name: None,
            next_window_id: FIRST_WINDOW_ID.0,
            window_sequence_origin: WindowSequenceOrigin(FIRST_WINDOW_ID),
            old_selected_window: None,
            deleted_windows: HashMap::default(),
            deleted_window_parameters: HashMap::default(),
            window_select_count: 0,
            next_navigation_intent_generation: std::num::NonZeroU64::MIN,
            pending_content_transition_intents: PendingContentTransitionIntents::default(),
            window_topology_generation: 1,
        }
    }
}

impl GcTrace for FrameManager {
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        // Deleted window parameter maps
        for params in self.deleted_window_parameters.values() {
            for (k, v) in params {
                roots.push(*k);
                roots.push(*v);
            }
        }
        // Frame and window tree parameters
        for frame in self.frames.values() {
            roots.push(frame.name);
            roots.push(frame.icon_name);
            roots.push(frame.focus_frame);
            roots.push(frame.parent_frame);
            roots.push(frame.title);
            if let Some(window_system) = frame.window_system {
                roots.push(window_system);
            }
            roots.extend(frame.parameters.keys().copied());
            for v in frame.parameters.values() {
                roots.push(*v);
            }
            roots.push(frame.face_hash_table);
            frame.face_realization.trace_roots(roots);
            // Chrome string objects in published snapshots are rooted by
            // their own `SharedRoot` leases.
            frame.tree.trace_roots(roots);
            if let Some(mb) = &frame.minibuffer_leaf {
                mb.trace_roots(roots);
            }
        }
    }
}

impl GcTrace for Window {
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        // Proportional sizes are HEAP values (`Value::make_float` ->
        // `alloc_float`), and every window node -- internal ones included --
        // owns some.  Leaving them untraced let a collection free a live
        // window's `normal_cols`/`normal_lines`, after which a proportional
        // resize read a dangling slot and produced a different layout.  That
        // showed up as ~10% run-to-run NONDETERMINISM in the window tree for
        // identical input: reading the sizes from Lisp shifted allocation
        // enough to move the GC and hide it.  GNU's `mark_window` marks these
        // slots for the same reason.
        roots.push(self.normal_cols());
        roots.push(self.normal_lines());
        roots.push(self.new_normal());
        match self {
            Window::Leaf {
                position_markers,
                display,
                dedicated,
                ..
            } => {
                roots.push(*dedicated);
                // GNU's `mark_window` traces w->start, w->pointm, and
                // w->old_pointm. The buffer marker chain is weak, so the live
                // window itself must keep the corresponding MarkerObjs alive.
                position_markers.trace_roots(roots);
                for (key, value) in self.parameters() {
                    roots.push(*key);
                    roots.push(*value);
                }
                if let Some(history) = self.history() {
                    roots.push(history.prev_buffers);
                    roots.push(history.next_buffers);
                }
                roots.push(display.display_table);
                roots.push(display.cursor_type);
                roots.push(display.vertical_scroll_bar_type);
                roots.push(display.horizontal_scroll_bar_type);
            }
            Window::Internal { .. } => {
                for (key, value) in self.parameters() {
                    roots.push(*key);
                    roots.push(*value);
                }
            }
        }
    }
}

impl GcTrace for WindowTree {
    /// Trace EVERY node the tree holds, not just those reachable from the
    /// root.
    ///
    /// A node that has been unlinked -- mid-surgery, or on its way out of the
    /// tree -- still owns Lisp values in its window parameters and markers.
    /// Walking from the root would stop rooting them the instant they were
    /// spliced out, which is precisely when they are most likely to be read
    /// again.
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        for node in self.nodes() {
            node.trace_roots(roots);
        }
    }
}

// --- E5-FM-6b WindowSnapshotRowAtY (stage-4 W2/E5) ---

/// Where a window-relative Y falls among the rows a redisplay published.
///
/// GNU has no "nothing here" answer for a Y inside a window's text area, and
/// the reason is that it does not look in a matrix at all: `posn-at-x-y` reaches
/// `buffer_posn_from_coords`, which runs an iterator from the window's start and
/// stops it wherever the walk ends (src/dispnew.c:6278-6285). Below the last
/// line of a short buffer the walk ends by running out of BUFFER — the ZV break
/// in `move_it_in_display_line_to` (src/xdisp.c:10251-10258) — so the answer is
/// point-max, not nil.
///
/// Measured, GNU Emacs 31.0.90, 80x24 pty, buffer `"abcdef\nghijkl\n"`
/// (`scripts/below-content-audit.el`): every column of every screen row from 3
/// down answers buffer position 15, and `posn-actual-col-row` answers row **2**
/// — the row the ITERATOR stopped on, not the row that was clicked. That
/// distinguishes this shape from the other one available to a row-based port,
/// which is to emit filler rows below the buffer and let the click land on one
/// of them; those would report row 3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowSnapshotRowAtY<'a> {
    /// Y is inside this row's own vertical band.
    Within(&'a DisplayRowSnapshot),
    /// No published row reaches Y, and this is the last row above it that
    /// carries buffer text. GNU's iterator comes to rest at ZV here, so the
    /// answer is this row's own end.
    BelowLastTextRow(&'a DisplayRowSnapshot),
    /// No published row reaches Y and none carrying buffer text lies above it:
    /// there is nothing to answer from. GNU reaches the same place through
    /// `pos_visible_p`'s `FRAME_INITIAL_P` early return (src/xdisp.c:1702-1703).
    NoTextRow,
}

// --- E5-FM-6b ChromeLineHit (stage-4 W2/E5) ---

/// What GNU's `mode_line_string` (src/dispnew.c:6444-6519) answers for a click
/// on a window's tab, header or mode line.
///
/// There is no buffer position among these: GNU sets `textpos = -1` for the
/// chrome branch (src/keyboard.c:5900) and the posn's `posn-point` is nil.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChromeLineHit {
    /// The index of the glyph under X, extended past the row's last glyph by
    /// whole columns.
    pub col: i64,
    /// The chrome row's own index relative to the matrix's first TEXT row --
    /// the number of text rows for a mode line, -1 for a header line.
    pub row: i64,
    pub dx: i64,
    pub dy: i64,
    pub width: i64,
    pub height: i64,
}
