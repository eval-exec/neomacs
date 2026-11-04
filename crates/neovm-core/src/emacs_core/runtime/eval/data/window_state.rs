//! Window state records owned by the evaluator layer (stage-4 W1 E2).

use neomacs_display_protocol::PresentedWindowChromeArea;
use neomacs_display_protocol::posn_frame_pool::PosnFramePool;
use neomacs_display_protocol::posn_object_extent::PosnPointRole as DisplayPointRole;
use neovm_edit::buffer::{
    BufferId, BufferManager, CharLen, CharPos0, EmacsByteLen, EmacsBytePos, LispCharPos1,
    MarkerRef, TextPositionAnchor,
};
use neovm_edit::emacs_core::display_metrics::{Rect, WindowId, WindowMargins};
use neovm_value::display_vocab::{
    ChangeStamp, FrameId, SplitDirection, WindowHistoryState, WindowLayoutVariable,
};
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
pub(crate) enum TtyCurrentMatrixAuthority {
    AcceptedLocal,
    FramePoolPartition,
    Undrawn,
}

/// GNU desired-matrix allocation facts, not source/display-body freshness.
/// Numeric values are initialized by the exclusive Frame mutator and immutable
/// to readers; no query advances this allocation observation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TtyMatrixAllocation {
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
