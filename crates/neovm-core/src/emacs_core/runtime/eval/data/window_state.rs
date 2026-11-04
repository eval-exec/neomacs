//! Window state records owned by the evaluator layer (stage-4 W1 E2).

use neovm_edit::buffer::{BufferId, LispCharPos1, MarkerRef};
use neovm_edit::emacs_core::display_metrics::{Rect, WindowId, WindowMargins};
use neovm_value::display_vocab::FrameId;
use neovm_value::intern::SymId;
use neovm_value::value::Value;
use rustc_hash::FxHashMap as HashMap;

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
