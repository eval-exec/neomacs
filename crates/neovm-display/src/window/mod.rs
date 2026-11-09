//! Window and frame management for the editor.
//!
//! Implements the Emacs window tree model:
//! - A **frame** contains a root window (which may be split).
//! - A **window** is either a *leaf* (displays a buffer) or an *internal*
//!   node with children (horizontal or vertical split).
//! - The **selected window** is the one receiving input.
//! - The **minibuffer window** is a special single-line window at the bottom.

use self::pixel_input::SpaceInputExt;
use self::string_property_input::{StringContentInputExt, StringDisplayInputsExt};
use neomacs_display_protocol::TransitionDirection;
use neovm_edit::buffer::{BufferId, BufferManager, LispCharPos1};
use neovm_value::value::Value;
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};

pub(crate) mod body;
mod chrome;
pub use chrome::WindowChromePresence;
pub mod display;
pub(crate) use display::frame_line_height;
pub use neovm_eval::emacs_core::eval::data::window_state::GUI_SCROLL_BAR_DEFAULT_WIDTH_PX;
pub use neovm_eval::emacs_core::eval::data::window_state::{
    current_char_table_layout_revision, resolve_window_scroll_bar_geometry,
};
pub(crate) use neovm_eval::emacs_core::eval::data::window_state::{
    frame_has_horizontal_scroll_bar, frame_vertical_scroll_bar_side,
    note_char_table_layout_mutation,
};
pub(crate) use neovm_eval::emacs_core::eval::data::window_state::{
    swap_world_state as swap_display_world_state,
    world_state_placeholder as display_world_state_placeholder,
};
mod display_table_input;
pub(crate) use display::WindowScrollUpdate;
pub use display_table_input::LayoutDisplayTableInput;
mod face_input;
mod frame_params;
pub(crate) use frame_params::FrameQueryParam;
pub mod geometry;
mod history;
mod invisibility_input;
pub use invisibility_input::LayoutInvisibilityInput;
mod parameters;
pub mod part;
mod pixel_input;
mod point_rows;
mod posn_object_extent;
mod tty_posn_current;
pub use point_rows::{
    DisplayPointRow, DisplayPointRowIter, DisplayPointRows, DisplayPointRowsIter,
    DisplayPointRowsMode, PointCell, display_point_rows_mode,
};
#[cfg(test)]
pub(crate) use posn_object_extent::force_posn_object_extent_for_test;
pub use posn_object_extent::{PosnObjectExtentMode, posn_object_extent_mode, retained_posn_extent};
mod scroll_bar;
mod scroll_margin;
pub use scroll_margin::ScrollMargin;
mod sibling_layout;
mod size;
pub(crate) use size::{
    HorizontalScroll, SplitSizeError, SplitSizes, WindowPixelOperation, WindowPixels,
    WindowSizeError,
};
pub use size::{NewTotalUpdate, WindowPixelStage, WindowTotal};
pub mod split;
mod string_property_input;
mod word_wrap;
pub use neovm_edit::emacs_core::word_wrap::{
    WordWrapCategories, WordWrapDirection, WordWrapMode, WordWrapOpportunity, WordWrapPolicy,
};
pub use word_wrap::word_wrap_policy_for_buffer;
pub mod window_markers;

pub use part::{
    PosnArea, TextAreaCoordinate, WindowChromeLine, WindowCoordinate, WindowPart,
    WindowPartGeometry,
};
use sibling_layout::{ChildExtent, sibling_bounds};
pub use split::{CombinationLimit, DeleteOutcome, DeleteResize, ParentSeal, SplitAttachment};

mod display_ext;
#[cfg(test)]
pub(crate) use display_ext::DisplayFrameTestExt;
#[doc(hidden)]
pub use display_ext::resize_pixelwise_with_buffer_constraints;
pub use display_ext::{
    DisplayFrameExt, DisplayFrameManagerExt, DisplayWindowExt, DisplayWindowTreeExt,
};
#[cfg(any(test, feature = "test-support"))]
pub(crate) use display_ext::{
    commit_redisplay_cache_for_test, prepare_and_activate_display_presentation_for_test,
    replace_redisplay_cache_for_test,
};

pub use display::{WindowBufferDisplayDefaults, WindowFringeDefaults, WindowScrollBarDefaults};
pub(crate) use display::{frame_config_scroll_bar_height, frame_config_scroll_bar_width};
pub use frame_params::{
    CursorTypeSymbol, FrameFullscreen, FrameParam, FrameParamKey, FrameToolBarPosition,
    FrameZGroup, GNU_FRAME_PARAM_COUNT, GNU_FRAME_PARAMS,
};
pub(crate) use neovm_eval::emacs_core::eval::data::window_layout_query::WindowLayoutQueryAdapter;
pub use neovm_eval::emacs_core::eval::data::window_state::WindowScrollBarGeometry;
pub use scroll_bar::{
    HorizontalScrollBarType, VerticalScrollBarType, is_valid_horizontal_scroll_bar_value,
    is_valid_vertical_scroll_bar_value,
};

// ---------------------------------------------------------------------------
// IDs
// ---------------------------------------------------------------------------

pub use neovm_edit::emacs_core::display_metrics::WindowId;

pub(crate) use neovm_eval::emacs_core::eval::data::window_state::{
    FIRST_WINDOW_ID, WindowSequenceNumber,
};

pub use neovm_eval::emacs_core::eval::data::window_state::{
    DeletedWindowRecord, FrameDeletion, FrameDeletionSelectionPolicy, NavigationIntentToken,
    SelectedFrameAfterDeletion, WindowPresentationSnapshot,
};
pub(crate) use neovm_eval::emacs_core::eval::data::window_state::{
    FramePresentationState, PendingContentTransitionIntents, PendingIntentLedger,
    PreparedDisplayPresentation, TtyFrameNameOrdinal, WindowSequenceOrigin,
};

static_assertions::assert_eq_size!(WindowSequenceOrigin, WindowId);

pub use neovm_value::display_vocab::FrameId;

pub(crate) use neovm_eval::emacs_core::eval::data::window_state::GuiResizeDelivery;
pub use neovm_eval::emacs_core::eval::data::window_state::{GuiBarPresentation, PendingGuiResize};

pub mod frame_alpha;

/// GNU-compatible lifecycle state for a live frame.
///
/// GNU stores `visible` and `iconified` as separate bits, but only these three
/// public states are valid.  Keeping the sum type here prevents callers from
/// manufacturing the impossible `visible && iconified` combination and makes
/// every projection choose its intended semantics explicitly.
pub use neovm_value::display_vocab::FrameVisibility;

pub use neovm_eval::emacs_core::eval::data::window_state::{
    ModeLineBase, WindowCellDimensions, WindowRedisplayState, WindowStartCommit, WindowTreePath,
};
pub(crate) use neovm_eval::emacs_core::eval::data::window_state::{
    WindowLayoutSelectionState, WindowLayoutValueIdentity, WindowLayoutVariableState,
    WindowParameters, WindowTreeRoute,
};

/// Root-relative frame placement used by redisplay backends.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderFrameNode {
    pub frame_id: FrameId,
    pub parent_id: Option<FrameId>,
    pub origin_in_root_x: f32,
    pub origin_in_root_y: f32,
    pub z_order: i32,
}

/// Bottom-to-top render order for one frame tree.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderFrameTree {
    pub root_id: FrameId,
    pub frames_bottom_to_top: Vec<RenderFrameNode>,
}

/// Which frame trees a redisplay backend intends to render.
///
/// Keeping this typed prevents a GUI redisplay from accidentally treating the
/// selected frame tree as the complete set of native windows.  TTY redisplay
/// renders one composited tree; GUI redisplay renders every top-level native
/// window tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderFrameScope {
    TreeContaining(FrameId),
    AllNativeWindowTrees,
}

/// Whether hidden frames participate in a frame-tree query.
pub use neovm_value::display_vocab::RenderFrameVisibility;

pub use neovm_value::display_vocab::FRAME_ID_BASE;

/// GNU `DEFAULT_TOOL_BAR_LABEL_SIZE` in `src/dispextern.h`.
pub const DEFAULT_TOOL_BAR_LABEL_SIZE: f32 = 14.0;
/// GNU `DEFAULT_TOOL_BAR_BUTTON_MARGIN` in `src/dispextern.h`.
pub const DEFAULT_TOOL_BAR_BUTTON_MARGIN: f32 = 4.0;
/// GNU `DEFAULT_TOOL_BAR_BUTTON_RELIEF` in `src/dispextern.h`.
pub const DEFAULT_TOOL_BAR_BUTTON_RELIEF: f32 = 1.0;
/// GNU `DEFAULT_TOOL_BAR_IMAGE_HEIGHT` in `src/dispextern.h`.
pub const DEFAULT_TOOL_BAR_IMAGE_HEIGHT: f32 = 24.0;

pub fn default_gui_tool_bar_line_height(font_pixel_size: f32) -> u32 {
    let scale = if font_pixel_size.is_finite() && font_pixel_size > 0.0 {
        (font_pixel_size / DEFAULT_TOOL_BAR_LABEL_SIZE).max(1.0)
    } else {
        1.0
    };
    let image_height = (DEFAULT_TOOL_BAR_IMAGE_HEIGHT * scale).round().max(1.0);
    let margin = (DEFAULT_TOOL_BAR_BUTTON_MARGIN * scale).round().max(0.0);
    let relief = (DEFAULT_TOOL_BAR_BUTTON_RELIEF * scale).round().max(1.0);

    (image_height + 2.0 * margin + 2.0 * relief)
        .round()
        .max(1.0) as u32
}

// ---------------------------------------------------------------------------
// Window geometry
// ---------------------------------------------------------------------------

pub use neovm_edit::emacs_core::display_metrics::Rect;

// ---------------------------------------------------------------------------
// Split direction
// ---------------------------------------------------------------------------

pub use neovm_value::display_vocab::SplitDirection;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitPlacement {
    BeforeTarget,
    AfterTarget,
}

impl SplitPlacement {
    fn is_before_target(self) -> bool {
        matches!(self, Self::BeforeTarget)
    }
}

// ---------------------------------------------------------------------------
// Window display state
// ---------------------------------------------------------------------------

pub use neovm_eval::emacs_core::eval::data::window_state::{
    LogicalCursorDimensions, WindowCursorKind, WindowCursorPos, WindowCursorSnapshot,
    WindowDisplayState, WindowRedisplayOutput, WindowVisibleBufferSpan,
};

/// Explicit live redisplay/update session for one window.
///
/// This mirrors GNU's per-window output/update ownership model: explicit
/// output-cursor moves, cursor installation, and final redisplay commit all
/// flow through one update object over the live `WindowDisplayState`.
/// Snapshot replay remains a narrow compatibility path for replay/bootstrap
/// cases and is not used by the normal Rust layout pipeline.
pub struct WindowOutputUpdate<'a> {
    display: &'a mut WindowDisplayState,
}

impl<'a> WindowOutputUpdate<'a> {
    fn new(display: &'a mut WindowDisplayState) -> Self {
        Self { display }
    }

    pub fn begin_update(&mut self) {
        self.display.begin_window_output_update();
    }

    pub fn output_cursor_to(&mut self, pos: WindowCursorPos) {
        self.display.output_cursor_to(pos);
    }

    pub fn output_cursor_to_coords(&mut self, row: i64, col: i64, y: i64, x: i64) {
        self.output_cursor_to(WindowCursorPos { x, y, row, col });
    }

    fn replay_output_rows(&mut self, rows: &[DisplayRowSnapshot]) {
        if rows.is_empty() {
            self.display.clear_output_cursor_state();
            return;
        }
        for row in rows {
            self.output_cursor_to_coords(row.row, row.start_col, row.y, row.start_x);
            self.output_cursor_to_coords(row.row, row.end_col, row.y, row.end_x);
        }
    }

    pub fn install_logical_cursor(&mut self, cursor: Option<WindowCursorPos>) {
        self.display.install_logical_cursor(cursor);
    }

    pub fn apply_physical_cursor_snapshot(&mut self, cursor: Option<WindowCursorSnapshot>) {
        self.display.apply_physical_cursor_snapshot(cursor);
    }

    fn fallback_output_cursor_from_snapshot(&mut self, snapshot: &WindowDisplaySnapshot) {
        if self.display.output_cursor.is_none() {
            self.replay_output_rows(&snapshot.rows);
        }
    }

    pub fn finalize_live_update(
        &mut self,
        logical_cursor: Option<WindowCursorPos>,
        phys_cursor: Option<WindowCursorSnapshot>,
    ) {
        self.install_logical_cursor(logical_cursor);
        self.apply_physical_cursor_snapshot(phys_cursor);
        self.commit();
    }

    pub fn finalize_with_output_fallback(
        &mut self,
        logical_cursor: Option<WindowCursorPos>,
        phys_cursor: Option<WindowCursorSnapshot>,
        output_fallback: &WindowDisplaySnapshot,
    ) {
        self.install_logical_cursor(logical_cursor);
        self.apply_physical_cursor_snapshot(phys_cursor);
        self.fallback_output_cursor_from_snapshot(output_fallback);
        self.commit();
    }

    pub fn replay_snapshot(&mut self, snapshot: &WindowDisplaySnapshot) {
        self.begin_update();
        self.install_logical_cursor(snapshot.logical_cursor_pos());
        self.replay_output_rows(&snapshot.rows);
        self.apply_physical_cursor_snapshot(snapshot.phys_cursor.clone());
        self.commit();
    }

    pub fn commit(&mut self) {
        self.display.commit_completed_redisplay();
    }
}

pub use neovm_value::display_vocab::WindowHistoryState;

pub use neovm_edit::emacs_core::display_metrics::WindowMargins;

pub use neovm_eval::emacs_core::eval::data::window_state::{
    CharTableLayoutRevision, FrameLayoutInputState, WindowLayoutInputState,
};

pub use neovm_eval::emacs_core::eval::data::layout_input::{
    BufferLayoutInputState, LayoutPrefixInputs, WindowDisplaySnapshotFreshness,
};
pub(crate) use neovm_eval::emacs_core::eval::data::layout_input::{
    FontificationHookIdentity, LayoutPrefixInput, WindowBufferLayoutInputState,
};

/// Stage-4 W5/E13 upper extension trait over the eval-owned type.
#[doc(hidden)]
pub trait LayoutPrefixInputExt {
    fn capture(value: Option<Value>) -> Self;
}

impl LayoutPrefixInputExt for LayoutPrefixInput {
    fn capture(value: Option<Value>) -> Self {
        let Some(value) = value else {
            return Self::Missing;
        };
        if let Some(space) = pixel_input::SpaceInput::capture(value) {
            return Self::Space(space);
        }
        match value.as_lisp_string() {
            Some(string) => Self::String {
                content: string_property_input::StringContentInput::capture(value.bits(), string),
                display_properties: string_property_input::StringDisplayInputs::capture(
                    string.intervals(),
                ),
            },
            None => Self::Other(value.bits()),
        }
    }
}

// The default bodies above are the implementation; this binds them to the
// upper-layer type that owns the state.

pub use neovm_value::display_vocab::WindowLayoutVariable;

pub use neovm_eval::emacs_core::eval::data::window_layout_query::{
    WindowLayoutQuery, WindowLayoutQueryFailure, WindowLayoutQueryOutcome, WindowLayoutQueryScope,
};

pub use neovm_eval::emacs_core::eval::data::window_state::{
    MatrixRow0, WindowEndRecord, WindowEndState,
};

/// Affine rollback token for a speculative leaf's live `window_end`.
///
/// GNU lets status-line evaluation observe the end produced by the body walk,
/// but a Neomacs convergence retry must not retain that end as accepted
/// viewport evidence. The layout engine must either accept this token or hand
/// it back to `Context` for restoration.
#[must_use = "a speculative window-end attempt must be accepted or rejected"]
pub struct WindowEndAttempt {
    frame_id: FrameId,
    window_id: WindowId,
    buffer_id: BufferId,
    previous: WindowEndState,
}

impl WindowEndAttempt {
    /// Consume the rollback capability after the leaf is accepted.
    pub fn accept(self) {}
}

/// Build one accepted redisplay output record from a display snapshot.
///
/// The snapshot is display-owned, so this constructor stays in the window
/// engine; the record it builds is eval data (stage-4 W2/E5 FM-1b).
fn window_redisplay_output_from_snapshot(
    generation: geometry::PresentationId,
    snapshot: &WindowDisplaySnapshot,
    window_end: WindowEndRecord,
) -> WindowRedisplayOutput {
    WindowRedisplayOutput {
        generation,
        window_end,
        visible_span: snapshot.visible_buffer_span(),
        logical_cursor: snapshot.logical_cursor_pos(),
        phys_cursor: snapshot.phys_cursor.clone(),
    }
}

// ---------------------------------------------------------------------------
// Window
// ---------------------------------------------------------------------------

pub use neovm_eval::emacs_core::eval::data::window_state::{
    AttachedWindowPositionMarkers, WindowOldPointMarker, WindowPointMarker,
    WindowPositionMarkerState, WindowStartMarker,
};

/// GNU's window-change epoch counter (`f->change_stamp`, `w->change_stamp`).
///
/// GNU reserves 0 for "never recorded" and keeps that invariant with a runtime
/// wrap guard:
///
/// ```c
///   f->change_stamp += 1;
///   if (f->change_stamp == 0) f->change_stamp = 1;
/// ```
///
/// `NonZeroU32` makes the reserved value unrepresentable instead, so
/// `Option<ChangeStamp>` IS GNU's "0 or a stamp" in one word and the guard
/// cannot be dropped by accident.
pub use neovm_value::display_vocab::ChangeStamp;

/// GNU's three possible answers for `window-old-buffer`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowOldBuffer {
    /// `NILP (w->old_buffer)` -- the window was created since the last record.
    NeverRecorded,
    /// The record is from an older epoch: a window restored from a
    /// configuration, or one that has been deleted.
    StaleEpoch,
    /// The buffer the window showed when the epoch was recorded.
    Recorded(BufferId),
}

pub use neovm_eval::emacs_core::eval::data::window_state::{Window, WindowTree};

fn collect_leaf_window_paths(
    tree: &WindowTree,
    window: WindowId,
    generation: u64,
    route: &mut Vec<usize>,
    paths: &mut Vec<(WindowId, WindowTreePath)>,
) {
    let children = tree.child_ids(window);
    if tree.find(window).is_some_and(Window::is_leaf) {
        paths.push((
            window,
            WindowTreePath {
                generation,
                target: window,
                route: WindowTreeRoute::Root(route.clone()),
            },
        ));
        return;
    }
    for (index, child) in children.iter().copied().enumerate() {
        route.push(index);
        collect_leaf_window_paths(tree, child, generation, route, paths);
        route.pop();
    }
}

// ---------------------------------------------------------------------------
// Last Display Snapshot
// ---------------------------------------------------------------------------

/// What a published display point IS: a glyph the row drew, or a screen column
/// a special glyph covered.
///
/// A continuation or truncation marker owns NO buffer position in GNU.
/// `insert_left_trunc_glyphs` stamps `CHARPOS (truncate_it.position) =
/// BYTEPOS (truncate_it.position) = -1` and `truncate_it.object = Qnil`
/// (src/xdisp.c:23858-23860) and then OVERWRITES the row's leading glyphs;
/// `produce_special_glyphs`, which makes the right-edge `$` and the
/// continuation `\`, does `temp_it.object = Qnil` and zeroes `temp_it.current`
/// (src/xdisp.c:32989-32991) and likewise overwrites the last glyph the row
/// produced (src/xdisp.c:26611-26632). The marker stands ON a column; it does
/// not consume a position.
///
/// What answers for that column is therefore not the glyph but the WALK, and
/// GNU never has to choose: every position-answering path re-runs the iterator.
/// `Fvertical_motion` goes through `start_display` and `move_it_by_lines`
/// (src/indent.c:2317, :2466-2472); `buffer_posn_from_coords` through
/// `move_it_to` and `move_it_in_display_line` after `to_x +=
/// it.first_visible_x` (src/dispnew.c:6273-6281, :6300-6302, :6327);
/// `pos_visible_in_window_p` likewise. A port that answers from ROWS has to
/// name the case instead, which is what this type is for.
///
/// MEASURED, GNU Emacs 31.0.90, 80x24 pty (`scripts/l212-marker-column-probe.el`):
/// a truncating row hscrolled by 5 whose line starts at 202 answers **207** for
/// a click on the `$` in column 0 -- the character the marker overlays, not the
/// one after it -- and a row truncated at the right edge answers **80** for a
/// click on the `$` in column 79. But the same measurement says a marker slot
/// must NOT win a POSITION lookup outright: with `truncate-lines` nil, position
/// 80 stands under the `\` in row 0 column 79 AND is drawn at row 1 column 0,
/// and `posn-at-point` answers `(0 . 1)` -- the glyph. So the marker slot
/// answers a COORDINATE, and stands in for a POSITION only when nothing drew
/// one.
pub use neomacs_display_protocol::posn_object_extent::PosnPointRole as DisplayPointRole;

pub use neovm_eval::emacs_core::eval::data::window_state::{
    CompletedEobOverlayMotionExtent, DisplayPointSnapshot, DisplayRowEndSource, DisplayRowSnapshot,
    EobOverlayMotionCoverage, FringeBitmapIndex, PresentedBodyRowSnapshot,
    PresentedWindowChromeString, PresentedWindowChromeStrings, RowFringeBitmaps,
    RowOverlayArrowBitmap,
};

pub use neovm_eval::emacs_core::eval::data::window_state::WindowSnapshotRowAtY;

/// Last authoritative redisplay geometry for a live leaf window.
pub use neomacs_display_protocol::frame_glyphs::PresentedWindowRegions;

/// Window chrome whose displayed Lisp-string object participates in mouse
/// position reporting.
pub use neomacs_display_protocol::PresentedWindowChromeArea;

static_assertions::assert_impl_all!(PresentedWindowChromeString: Send, Sync, Clone, std::fmt::Debug);

static_assertions::assert_impl_all!(PresentedWindowChromeStrings: Send, Sync, Clone, std::fmt::Debug, Eq);
static_assertions::assert_not_impl_any!(PresentedWindowChromeStrings: Copy);

pub use neovm_eval::emacs_core::eval::data::window_state::WindowDisplaySnapshot;
static_assertions::assert_impl_all!(WindowDisplaySnapshot: Send, Sync, Clone, std::fmt::Debug);
static_assertions::assert_not_impl_any!(WindowDisplaySnapshot: Copy);

pub use neovm_eval::emacs_core::eval::data::window_state::{
    BodyRedisplayRevision, ForcedBodyRedisplay,
};

/// Canonical logical input identity for one speculative layout leaf.
///
/// Inactive minibuffer layout can read the echo-area buffer while the live
/// window remains bound to the minibuffer buffer. Keeping both identities
/// prevents absence or a semantic source substitution from masquerading as an
/// unchanged `Option` freshness token across Lisp fontification.
///
/// This is intentionally distinct from [`WindowDisplaySnapshotFreshness`].
/// Retained rows must be invalidated when *any* display-affecting mutation
/// occurs, so that token contains monotonic mutation epochs. An in-flight
/// attempt instead compares canonical state before and after Lisp callbacks:
/// a scoped binding that restores its original value did not stale the rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowLayoutAttemptFreshness {
    fontset_generation: u64,
    context_instance_id: u64,
    window_topology_generation: u64,
    frame: FrameLayoutInputState,
    window: WindowLayoutInputState,
    live_buffer: WindowBufferLayoutInputState,
    source_buffer: WindowBufferLayoutInputState,
    source_layout_variables: WindowLayoutVariableState,
    selection: WindowLayoutSelectionState,
    face_change_count: u64,
    media_generation: u64,
    /// Body-only invalidation for this window and its displayed buffer; see
    /// [`BodyRedisplayRevision`]. A `force-window-update` issued while body
    /// Lisp is running must not let that attempt publish rows produced from
    /// the pre-invalidation inputs, and unlike `media_generation` it does not
    /// depend on `image-flush` having actually changed a catalog entry.
    body_redisplay: BodyRedisplayRevision,
    function_epoch: u64,
}

impl WindowLayoutAttemptFreshness {
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(any(test, feature = "test-support"), visibility::make(pub))]
    fn live_buffer(&self) -> &WindowBufferLayoutInputState {
        &self.live_buffer
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(any(test, feature = "test-support"), visibility::make(pub))]
    fn source_buffer(&self) -> &WindowBufferLayoutInputState {
        &self.source_buffer
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(any(test, feature = "test-support"), visibility::make(pub))]
    fn function_epoch(&self) -> u64 {
        self.function_epoch
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(any(test, feature = "test-support"), visibility::make(pub))]
    fn body_redisplay(&self) -> &BodyRedisplayRevision {
        &self.body_redisplay
    }

    /// Write route for the moved attempt test: the source-buffer projection is
    /// replaced by the live one after a re-capture. Private in production; the
    /// cross-crate route is the AUTHORITY fence in test_support::authority.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn set_source_buffer_from_live(&mut self) {
        self.source_buffer = self.live_buffer.clone();
    }
}

/// Lisp-bearing boundary crossed by one speculative window layout.
///
/// The variants encode GNU redisplay's ordering contract.  Buffer-body Lisp
/// runs while rows are still being produced, so every logical input must stay
/// unchanged.  Window chrome runs after GNU has completed the body
/// (`redisplay_window` -> `display_mode_lines`, xdisp.c): status-line helpers
/// may populate window-local caches or lazily load functions without making
/// the body GNU would publish invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowLayoutLispBoundary {
    BufferBody,
    WindowChrome,
}

/// Actual callback admission at an attempt-local input boundary.
///
/// NativeOnly is restricted to audited layout preparation/publication that
/// changes no logical input, restores temporary buffer/point selection, and
/// evaluates no Lisp. Arbitrary native edits require MayHaveEntered too.
/// A possible callback includes unsuccessful/unmarking
/// callbacks and errors. The complete token is freshly captured in that case.
/// This value belongs to one exclusively borrowed evaluator attempt; it is
/// never retained or used as an invalidation key by another mutator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowLayoutLispAdmission {
    NativeOnly,
    MayHaveEntered,
}

impl WindowLayoutAttemptFreshness {
    /// Whether `after` may still publish rows produced from `self` across the
    /// given GNU redisplay boundary.
    ///
    /// Keep this projection closed over the complete freshness value: newly
    /// added fields participate in the final equality automatically.  The two
    /// assignments below are the only explicitly permitted late-chrome
    /// mutations.
    pub fn remains_valid_across(&self, after: &Self, boundary: WindowLayoutLispBoundary) -> bool {
        match boundary {
            WindowLayoutLispBoundary::BufferBody => self == after,
            WindowLayoutLispBoundary::WindowChrome => {
                let mut expected_after = self.clone();
                // GNU tab-line.el writes window-local caches such as
                // `tab-line-buffers` while `display_mode_lines` is formatting
                // chrome.  Face filters observe the new identity on the next
                // redisplay; GNU does not rewind the body already produced.
                expected_after.window.window_parameters_generation =
                    after.window.window_parameters_generation;
                // Chrome evaluation can autoload its formatter.  The emitted
                // chrome already used the loaded definition, while the body
                // precedes this phase in GNU and is not replayed.
                expected_after.function_epoch = after.function_epoch;
                &expected_after == after
            }
        }
    }
}

pub use neovm_eval::emacs_core::eval::data::window_state::ChromeLineHit;

pub use neovm_eval::emacs_core::eval::data::window_state::{
    FrameWindowHookRecord, WindowHookSnapshot,
};

pub use neovm_edit::emacs_core::display_metrics::GuiFrameGeometryHints;

// ---------------------------------------------------------------------------
// Frame
// ---------------------------------------------------------------------------

mod display_identity;
pub use display_identity::FrameDisplayIdentity;

/// A frame divider whose requested parameter may become effective geometry.
///
/// Keeping this narrower than [`FrameParam`] prevents geometry callers from
/// accidentally asking for an unrelated frame parameter.  GNU stores divider
/// parameters on every frame, but only a window-system backend realizes them
/// into `struct frame`'s effective divider widths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameDivider {
    Right,
    Bottom,
}

impl FrameDivider {
    const fn parameter(self) -> FrameParam {
        match self {
            Self::Right => FrameParam::RightDividerWidth,
            Self::Bottom => FrameParam::BottomDividerWidth,
        }
    }
}

pub use neovm_eval::emacs_core::eval::data::window_state::{Frame, FrameManager};

pub use neovm_eval::emacs_core::eval::data::window_state::FrameCoordinateHit;

pub use neovm_eval::emacs_core::eval::data::window_state::PreparedMiniResizeOutcome;

// ---------------------------------------------------------------------------
// FrameManager
// ---------------------------------------------------------------------------

fn is_generated_tty_frame_name(value: Value) -> bool {
    value
        .as_lisp_string()
        .and_then(|name| name.as_utf8_str())
        .is_some_and(|name| {
            name.strip_prefix('F').is_some_and(|digits| {
                !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
            })
        })
}

// ---------------------------------------------------------------------------
// Tree manipulation helpers
// ---------------------------------------------------------------------------

/// Build the live leaf a split introduces beside `reference`.
///
/// Mirrors GNU `make_window` plus the property inheritance in
/// `Fsplit_window_internal`: the new window gets a fresh identity, no
/// parameters and no history, and inherits buffer-relative position state from
/// the reference window only when it will display the same buffer.
///
/// # Why this is shared
///
/// The two split paths — interposing a new parent, and splicing into an
/// existing combination — each used to clone-and-patch the reference leaf with
/// their own hand-written field list, and the lists drifted. The splice path's
/// list omitted `history`, so a window born there inherited the reference's
/// `use_time` and reported itself to `get-lru-window` as recently used, which
/// made `display-buffer`'s fallback pick a different window than GNU. Any field
/// added to `Window::Leaf` now has exactly one place to be considered.
fn make_split_sibling(
    reference: &Window,
    new_id: WindowId,
    new_buffer_id: BufferId,
    bounds: Rect,
) -> Window {
    // A split always introduces a live LEAF (GNU `make_window`), so an internal
    // reference seeds a fresh one rather than being cloned.
    let Window::Leaf {
        buffer_id: reference_buffer,
        ..
    } = reference
    else {
        let mut leaf = Window::new_leaf(new_id, new_buffer_id, bounds);
        // `Window::new_leaf` starts the character edges at zero and there is no
        // leaf to clone them from, so seed them from the reference.  They are
        // NOT derived from `bounds` here: GNU updates character edges lazily via
        // `window--pixel-to-total`, and eagerly re-deriving the whole frame
        // makes windows report an origin GNU has not moved yet (see
        // `FrameManager::split_window`).
        leaf.set_left_col(reference.left_col());
        leaf.set_top_line(reference.top_line());
        return leaf;
    };
    let same_buffer = new_buffer_id == *reference_buffer;

    let mut sibling = reference.clone();
    if let Window::Leaf {
        id,
        buffer_id,
        bounds: sibling_bounds,
        mode_line_base,
        parameters,
        parameters_generation,
        dedicated,
        history,
        window_start,
        position_markers,
        window_end,
        point,
        old_point,
        vscroll,
        preserve_vscroll_p,
        old_buffer,
        change_stamp,
        ..
    } = &mut sibling
    {
        *id = new_id;
        *mode_line_base = None;
        *buffer_id = new_buffer_id;
        *sibling_bounds = bounds;
        parameters.clear();
        *parameters_generation = 0;
        // GNU `make_window` leaves `w->dedicated` nil, and
        // `Fsplit_window_internal` copies only decorations (margins, fringes,
        // scroll bars) from the reference -- never the dedication.  Inheriting
        // it makes the new window invisible to `get-lru-window', which skips
        // dedicated windows unless asked otherwise, so `display-buffer' picks a
        // different window than GNU.
        *dedicated = Value::NIL;
        // `use_time` lives here: a brand-new window has never been selected.
        *history = WindowHistoryState::default();
        *position_markers = WindowPositionMarkerState::Detached;
        *window_end = WindowEndState::Unrecorded;
        if neovm_eval::emacs_core::eval::gnu_redisplay_hooks_enabled() {
            // GNU make_window starts old_buffer NIL and change_stamp zero.
            // A cloned decoration is not a recorded redisplay epoch: the new
            // leaf must run its first buffer-change callback even when it
            // shows the reference's buffer. Each frame's exclusive mutator
            // owns these numeric IDs/stamps; no shared Lisp state is added.
            *old_buffer = None;
            *change_stamp = None;
        }
        if !same_buffer {
            *window_start = LispCharPos1::ONE;
            *point = LispCharPos1::ONE;
            *old_point = LispCharPos1::ONE;
            *vscroll = 0;
            *preserve_vscroll_p = false;
        }
    }
    sibling
}

/// The combination direction of `target`'s parent within `tree`.
///
/// Returns `Some(None)` when `target` *is* `tree`'s root (GNU's
/// `NILP (o->parent)`), `Some(Some(dir))` when it is a child of a combination
/// running along `dir`, and `None` when `target` is not in this tree at all.
///
/// The doubled `Option` is deliberate: "absent" and "has no parent" are
/// different answers, and collapsing them would silently turn a lookup miss
/// into a root split.
fn parent_combination_of(tree: &WindowTree, target: WindowId) -> Option<split::ParentCombination> {
    if !tree.contains(target) {
        return None;
    }
    let Some(parent) = tree.parent_of(target) else {
        return Some(None);
    };
    match tree.find(parent) {
        Some(Window::Internal { direction, .. }) => Some(Some(*direction)),
        _ => Some(None),
    }
}

/// Attach a new window next to `target`, per `attachment`.
///
/// `attachment` carries GNU's already-made `combination_limit` decision (see
/// [`crate::window::split`]); this function only *executes* it, and must not
/// re-derive it from the tree.
///
/// `size` semantics (lines for vertical, columns for horizontal — 1 unit = 1.0
/// pixel in the abstract coordinate system):
/// - `None` / `Some(0)`: 50/50 split.
/// - `Some(n)` (n > 0): the new window gets `n` units.
/// - `Some(n)` (n < 0): the target window keeps `|n|` units.
#[allow(clippy::too_many_arguments)] // the split carries the explicit IDs and requested geometry
fn split_window_in_tree(
    tree: &mut WindowTree,
    target: WindowId,
    direction: SplitDirection,
    internal_id: WindowId,
    new_id: WindowId,
    new_buffer_id: BufferId,
    size: Option<i64>,
    placement: SplitPlacement,
    attachment: SplitAttachment,
) -> Option<()> {
    /// The two halves `old` splits into along `direction`, in layout order.
    fn split_rects(
        old: Rect,
        direction: SplitDirection,
        first_size: f32,
        second_size: f32,
    ) -> (Rect, Rect) {
        match direction {
            SplitDirection::Horizontal => (
                Rect::new(old.x, old.y, first_size, old.height),
                Rect::new(old.x + first_size, old.y, second_size, old.height),
            ),
            SplitDirection::Vertical => (
                Rect::new(old.x, old.y, old.width, first_size),
                Rect::new(old.x, old.y + first_size, old.width, second_size),
            ),
        }
    }

    /// A window's extent along `direction`.
    fn extent(bounds: Rect, direction: SplitDirection) -> f32 {
        match direction {
            SplitDirection::Horizontal => bounds.width,
            SplitDirection::Vertical => bounds.height,
        }
    }

    /// Give `window` the normal sizes GNU leaves on a window that a split
    /// creates or interposes a parent above: 1.0 on both axes.
    ///
    /// `make_window` initializes a fresh window that way, and `replace_window`
    /// (with SETFLAG) moves the old window's normal sizes to the new parent
    /// and resets the old window's to 1.0 (`src/window.c:4597-4612`).  Neither
    /// derives a share from pixel extents: the split-direction share is the
    /// `new_normal` that `split-window` staged in Lisp, which
    /// `window_resize_apply` commits afterwards
    /// ([`FrameManager::apply_staged_split_sizes`]).
    fn reset_to_unit_normals(window: &mut Window) {
        window.set_normal_cols(Value::make_float(1.0));
        window.set_normal_lines(Value::make_float(1.0));
    }

    let old = tree.find(target)?;
    let old_bounds = *old.bounds();
    // The new internal (parent) node takes the old window's slot, so it
    // inherits the old window's character-line position (GNU `split_window`);
    // the subsequent resize pass refines both children.
    let old_top_line = old.top_line();
    let old_left_col = old.left_col();
    let inherited_normal_lines = old.normal_lines();
    let inherited_normal_cols = old.normal_cols();

    let new_before_target = placement.is_before_target();
    let sizes = match SplitSizes::new(extent(old_bounds, direction), size) {
        Ok(sizes) => sizes,
        Err(SplitSizeError::NewTooSmall) => return None,
        Err(SplitSizeError::OldTooSmall) => return None,
    };
    let (old_size_px, new_size_px) = (sizes.old(), sizes.new_size());
    let (first_bounds, second_bounds) = if new_before_target {
        split_rects(old_bounds, direction, new_size_px, old_size_px)
    } else {
        split_rects(old_bounds, direction, old_size_px, new_size_px)
    };
    let (old_slot_bounds, new_leaf_bounds) = if new_before_target {
        (second_bounds, first_bounds)
    } else {
        (first_bounds, second_bounds)
    };

    // The target need NOT be a leaf in either case: GNU splices a sibling next
    // to an internal node just the same, which is how a side window is
    // attached beside the frame's main-window group, and interposes a parent
    // above one just the same.  Its whole subtree follows its new rectangle.
    resize_window_subtree(tree, target, old_slot_bounds);
    // A split always introduces a live LEAF (GNU `make_window`), so an internal
    // target seeds a fresh one rather than being cloned.
    let mut new_leaf =
        make_split_sibling(tree.find(target)?, new_id, new_buffer_id, new_leaf_bounds);

    // GNU `make_window`: the new window starts at normal size 1.0 on both
    // axes until its staged `new_normal` is committed.
    reset_to_unit_normals(&mut new_leaf);

    if attachment.reuses_parent() {
        // The target's own parent combination absorbs the new window as a
        // plain sibling (GNU `p = XWINDOW (o->parent)` -- no
        // `make_parent_window`).  The siblings' normal sizes are NOT restated
        // from their pixel extents: `Fsplit_window_internal` never writes them,
        // and `window_resize_apply` copies only the numeric `new_normal`s that
        // `split-window` staged (the old window's, or every sibling's under
        // `window-combination-resize`).  A sibling `window--resize-reset` left
        // at nil keeps its normal size, which later `delete-window` and
        // `balance-windows` arithmetic reads (window-oracle-nightly
        // 2026-10-29: restating from pixels gave 27/80 instead of GNU's 1/3).
        let parent = tree.parent_of(target)?;
        tree.insert(new_leaf);
        let mut siblings = tree.child_ids(parent).to_vec();
        let idx = siblings.iter().position(|child| *child == target)?;
        siblings.insert(if new_before_target { idx } else { idx + 1 }, new_id);
        tree.set_children(parent, siblings);
        return Some(());
    }

    // A fresh internal node takes the target's slot and holds both windows.
    // GNU `replace_window (old, parent, true)` hands the old window's normal
    // sizes to the parent (`inherited_normal_*` below) and resets the old
    // window's own to 1.0; its split-direction share is the staged
    // `new_normal`.
    if let Some(node) = tree.find_mut(target) {
        reset_to_unit_normals(node);
    }

    // Asked before the new parent is inserted, which would itself name the
    // target as a child.
    let grandparent = tree.parent_of(target);
    tree.insert(new_leaf);
    tree.insert(Window::Internal {
        id: internal_id,
        direction,
        children: if new_before_target {
            vec![new_id, target]
        } else {
            vec![target, new_id]
        },
        bounds: old_bounds,
        parameters: Vec::new(),
        parameters_generation: 0,
        combination_limit: attachment.new_parent_seal().as_stored_slot(),
        new_pixel: None,
        new_total: None,
        cell_dimensions: WindowCellDimensions::default(),
        // GNU stages the new parent's `new_normal` from the OLD window's
        // pre-split fraction, captured before `make_parent_window` corrupts it
        // (`src/window.c:5543,5570`):
        //
        //     Lisp_Object new_normal = horflag ? o->normal_cols : o->normal_lines;
        //     ...
        //     wset_new_normal (p, new_normal);
        //
        // This is not optional once `new_normal` starts life as a NUMBER rather
        // than nil: `window_resize_apply` copies it into
        // `normal_lines`/`normal_cols` guarded by `NUMBERP`
        // (`src/window.c:4829,4838`), so leaving it 0 here makes the apply
        // overwrite the inherited fraction with 0.
        new_normal: match direction {
            SplitDirection::Horizontal => inherited_normal_cols,
            SplitDirection::Vertical => inherited_normal_lines,
        },
        // The new internal node takes the slot that the old window used to
        // fill, so it inherits that window's pre-split proportional fractions.
        normal_lines: inherited_normal_lines,
        normal_cols: inherited_normal_cols,
        top_line: old_top_line,
        left_col: old_left_col,
    });

    match grandparent {
        Some(grandparent) => {
            let mut slots = tree.child_ids(grandparent).to_vec();
            let idx = slots.iter().position(|child| *child == target)?;
            slots[idx] = internal_id;
            tree.set_children(grandparent, slots);
        }
        None => tree.set_root(internal_id),
    }
    Some(())
}

/// Delete a window from the tree. Returns how the tree absorbed the removal.
/// Every node that actually leaves the tree reaches `on_removed` with its
/// metadata intact, including discarded subtrees and collapsed internal nodes.
fn delete_window_in_tree(
    tree: &mut WindowTree,
    target: WindowId,
    resize: DeleteResize,
    cells: body::WindowBodyCellSize,
    mut on_removed: impl FnMut(Window),
) -> DeleteOutcome {
    let Some(parent) = tree.parent_of(target) else {
        return DeleteOutcome::NotFound;
    };
    let Some(Window::Internal {
        bounds, direction, ..
    }) = tree.find(parent)
    else {
        return DeleteOutcome::NotFound;
    };
    let parent_bounds = *bounds;
    let horflag = matches!(*direction, SplitDirection::Horizontal);

    // Unlink the target, keeping the parent's geometry and axis, and drop the
    // nodes it took with it.  Owning children inline made that automatic --
    // removing the child dropped its whole subtree -- and it has to stay true:
    // a window that answers a lookup is a window `window-live-p` reports live.
    let discarded = tree.subtree_ids(target);
    let mut siblings = tree.child_ids(parent).to_vec();
    siblings.retain(|child| *child != target);
    let remaining = siblings.len();
    tree.set_children(parent, siblings);
    for id in discarded {
        if let Some(window) = tree.remove(id) {
            on_removed(window);
        }
    }

    // GNU `Fdelete_window_internal` commits the staged sizes for the whole
    // parent combination FIRST -- `window_resize_apply (p, horflag)` runs
    // before the matryoshka `replace_window` (`src/window.c`), so it applies
    // whether or not one sibling is about to be promoted.  Doing it only in
    // the multi-child case left a promoted subtree laid out by proportional
    // redistribution instead of by the plan `window.el` computed.
    if let DeleteResize::ApplyStaged = resize {
        let axis = if horflag {
            SplitDirection::Horizontal
        } else {
            SplitDirection::Vertical
        };
        resize_apply_pixels(tree, parent, axis, cells);
    }

    if remaining == 1 {
        // GNU's matryoshka case: the sole surviving sibling replaces the
        // parent and inherits its geometry (`replace_window`).
        let promoted = tree.child_ids(parent)[0];
        let (parent_normal_cols, parent_normal_lines) = tree
            .find(parent)
            .map(|node| (node.normal_cols(), node.normal_lines()))
            .expect("the parent is still held");
        if let Some(node) = tree.find_mut(promoted) {
            // GNU `Fdelete_window_internal` (`src/window.c:5793-5796`):
            //
            //     wset_normal_cols (s, p->normal_cols);
            //     wset_normal_lines (s, p->normal_lines);
            //
            // The promoted window takes over the parent's SHARE of the
            // grandparent, not the share it had of the parent it replaced.
            // Leaving it at its own (typically 1.0, being an only child) makes
            // every later proportional resize of the grandparent misweight it.
            node.set_normal_cols(parent_normal_cols);
            node.set_normal_lines(parent_normal_lines);
        }
        match resize {
            // The sizes were just committed above, and the sole child was
            // re-packed to fill its parent; re-laying it out here would
            // discard exactly that.  Only its own rect needs to take the
            // parent's slot.
            DeleteResize::ApplyStaged => {
                if let Some(node) = tree.find_mut(promoted) {
                    node.set_bounds(parent_bounds);
                }
            }
            // Nothing was staged, so the promoted child keeps its own
            // subtree at the old, smaller geometry unless it is re-laid-out
            // -- visible as windows that fail to reclaim a deleted
            // sibling's space.
            DeleteResize::Redistribute => {
                resize_window_subtree(tree, promoted, parent_bounds);
            }
        }

        // The promoted window takes the parent's slot.  GNU: "Have SIBLING
        // inherit the following three slot values from PARENT (the
        // combination_limit slot is not inherited)" (`src/window.c:5793-5796`)
        // -- so the promoted node keeps its own seal, which is what decides
        // recombination at the level above.
        let grandparent = tree.parent_of(parent);
        match grandparent {
            Some(grandparent) => {
                let mut slots = tree.child_ids(grandparent).to_vec();
                let Some(idx) = slots.iter().position(|child| *child == parent) else {
                    return DeleteOutcome::Removed;
                };
                slots[idx] = promoted;
                tree.set_children(grandparent, slots);
                if let Some(window) = tree.remove(parent) {
                    on_removed(window);
                }
                // GNU calls `recombine_windows` on the promoted sibling, and
                // only there (`src/window.c:5801`).
                if let Some(window) = recombine_child_into_parent(tree, grandparent, idx) {
                    on_removed(window);
                }
            }
            None => {
                tree.set_root(promoted);
                if let Some(window) = tree.remove(parent) {
                    on_removed(window);
                }
            }
        }
        return DeleteOutcome::Removed;
    }

    if let DeleteResize::Redistribute = resize {
        // No staged sizes to honor: spread the freed space over the
        // remaining children, then push each child's new rect down through
        // its own subtree.
        tree.redistribute_bounds(parent, parent_bounds);
        for child in tree.child_ids(parent).to_vec() {
            let Some(child_bounds) = tree.find(child).map(|node| *node.bounds()) else {
                continue;
            };
            resize_window_subtree(tree, child, child_bounds);
        }
    }
    DeleteOutcome::Removed
}

/// Merge `parent`'s `idx`th child into `parent` when the two are iso-combined
/// and the child is unsealed.
///
/// Mirrors GNU `recombine_windows` (`src/window.c:2606-2650`). The child's own
/// children are spliced into `parent`'s child list at the child's position, and
/// each gets a fresh normal size measured against its new parent:
///
/// ```c
/// wset_normal_cols (c, make_float ((double) c->pixel_width
///                                  / (double) p->pixel_width));
/// ```
///
/// A child whose stored `combination_limit` slot is set is left alone — that is
/// exactly what `set-window-combination-limit` is for, and what
/// `window--make-major-side-window` relies on to keep the main-window group
/// from dissolving into the root (Bug#80665).
/// Return the removed wrapper so its owner can retain the deleted object state.
fn recombine_child_into_parent(
    tree: &mut WindowTree,
    parent: WindowId,
    idx: usize,
) -> Option<Window> {
    let Some(Window::Internal {
        direction: parent_direction,
        bounds: parent_bounds,
        children,
        ..
    }) = tree.find(parent)
    else {
        return None;
    };
    let parent_direction = *parent_direction;
    let parent_bounds = *parent_bounds;
    let mut siblings = children.clone();
    let Some(&merged) = siblings.get(idx) else {
        return None;
    };

    // Only an unsealed combination along the same axis merges upward.
    match tree.find(merged) {
        Some(Window::Internal {
            direction,
            combination_limit,
            ..
        }) if !*combination_limit && *direction == parent_direction => {}
        _ => return None,
    }

    let inner = tree.child_ids(merged).to_vec();
    siblings.remove(idx);
    // The merged node is gone from the tree, not merely unlinked: its children
    // take its place in the parent's list.
    let removed = tree.remove(merged);

    let parent_size = match parent_direction {
        SplitDirection::Horizontal => parent_bounds.width,
        SplitDirection::Vertical => parent_bounds.height,
    };
    for (offset, child) in inner.into_iter().enumerate() {
        if parent_size > 0.0
            && let Some(node) = tree.find_mut(child)
        {
            let child_bounds = *node.bounds();
            // GNU divides in double precision; an f32 quotient widened
            // afterwards stores 0.33750000596... for 27/80, not 0.3375.
            let child_size = match parent_direction {
                SplitDirection::Horizontal => child_bounds.width,
                SplitDirection::Vertical => child_bounds.height,
            };
            let fraction = f64::from(child_size) / f64::from(parent_size);
            match parent_direction {
                SplitDirection::Horizontal => {
                    node.set_normal_cols(Value::make_float(fraction));
                }
                SplitDirection::Vertical => {
                    node.set_normal_lines(Value::make_float(fraction));
                }
            }
        }
        siblings.insert(idx + offset, child);
    }
    tree.set_children(parent, siblings);
    removed
}

fn find_sibling_in_tree(tree: &WindowTree, target: WindowId, next: bool) -> Option<WindowId> {
    let parent = tree.parent_of(target)?;
    let siblings = tree.child_ids(parent);
    let index = siblings.iter().position(|child| *child == target)?;
    if next {
        siblings.get(index + 1).copied()
    } else {
        index
            .checked_sub(1)
            .and_then(|index| siblings.get(index))
            .copied()
    }
}

fn find_first_child_in_tree(
    tree: &WindowTree,
    target: WindowId,
    direction: SplitDirection,
) -> Option<WindowId> {
    let Some(Window::Internal {
        direction: node_direction,
        children,
        ..
    }) = tree.find(target)
    else {
        return None;
    };
    (*node_direction == direction)
        .then(|| children.first().copied())
        .flatten()
}

/// Return the parent of WINDOW-ID inside FRAME, if any.
pub fn window_parent_id(frame: &Frame, window_id: WindowId) -> Option<WindowId> {
    if frame.minibuffer_window == Some(window_id) {
        return None;
    }
    frame.tree().parent_of(window_id)
}

/// Return the first child of WINDOW-ID when it is combined in DIRECTION.
pub fn window_first_child_id(
    frame: &Frame,
    window_id: WindowId,
    direction: SplitDirection,
) -> Option<WindowId> {
    if frame.minibuffer_window == Some(window_id) {
        return None;
    }
    find_first_child_in_tree(frame.tree(), window_id, direction)
}

/// Return the next sibling of WINDOW-ID, if any.
pub fn window_next_sibling_id(frame: &Frame, window_id: WindowId) -> Option<WindowId> {
    if frame.minibuffer_window == Some(window_id) {
        return None;
    }
    if frame.root_window().id() == window_id && frame.minibuffer_leaf.is_some() {
        return frame.minibuffer_window;
    }
    find_sibling_in_tree(frame.tree(), window_id, true)
}

/// Return the previous sibling of WINDOW-ID, if any.
pub fn window_prev_sibling_id(frame: &Frame, window_id: WindowId) -> Option<WindowId> {
    if frame.minibuffer_window == Some(window_id) {
        return frame
            .minibuffer_leaf
            .as_ref()
            .map(|_| frame.root_window().id());
    }
    find_sibling_in_tree(frame.tree(), window_id, false)
}

/// Apply pixel-based resize values to a window tree.
///
/// Mirrors GNU Emacs `window_resize_apply()` in window.c:
/// - Reads `new_pixel` for each window from the provided map
/// - Sets window bounds accordingly
/// - Recursively processes children, tracking edge positions
/// - For vertical combinations: accumulates vertical edge
/// - For horizontal combinations: accumulates horizontal edge
///
/// `horflag`: true = applying horizontal sizes, false = applying vertical sizes.
///
/// The pending sizes are read from each window's own `new_pixel`
/// slot (set previously via `set-window-new-pixel`), mirroring the
/// way GNU `window_resize_apply` walks `w->new_pixel` on every
/// node it visits. After audit Structural 1 in
/// `drafts/window-system-audit.md`, the slot lives on
/// `Window::Leaf` / `Window::Internal` directly so the resize
/// function no longer needs a side-table HashMap.
pub fn window_resize_apply(
    tree: &mut WindowTree,
    id: WindowId,
    horflag: bool,
    char_width: f32,
    char_height: f32,
) {
    let axis = if horflag {
        SplitDirection::Horizontal
    } else {
        SplitDirection::Vertical
    };
    resize_apply_pixels(
        tree,
        id,
        axis,
        body::WindowBodyCellSize::new(char_width, char_height),
    );
}

fn resize_body_axis(axis: SplitDirection) -> body::WindowBodyAxis {
    match axis {
        SplitDirection::Horizontal => body::WindowBodyAxis::Width,
        SplitDirection::Vertical => body::WindowBodyAxis::Height,
    }
}

fn resize_apply_pixels(
    tree: &mut WindowTree,
    id: WindowId,
    axis: SplitDirection,
    cells: body::WindowBodyCellSize,
) {
    let Some(window) = tree.find_mut(id) else {
        return;
    };
    let bounds = *window.bounds();
    if let Some(px) = window.new_pixel() {
        let px = px.max(0) as f32;
        let bounds = match axis {
            SplitDirection::Horizontal => Rect::new(bounds.x, bounds.y, px, bounds.height),
            SplitDirection::Vertical => Rect::new(bounds.x, bounds.y, bounds.width, px),
        };
        window.set_bounds(bounds);
        window.set_new_pixel(None);
    }

    // GNU window_resize_apply commits floored cell totals before returning to
    // Lisp's window--pixel-to-total reconciliation, including unchanged sizes.
    let bounds = *window.bounds();
    let extent = match axis {
        SplitDirection::Horizontal => bounds.width,
        SplitDirection::Vertical => bounds.height,
    };
    let total = body::WindowBodyUnit::CanonicalChars.measure(
        resize_body_axis(axis),
        extent as i64,
        cells,
        None,
    );
    window.commit_cell_total(axis == SplitDirection::Horizontal, total);

    // A planner may leave skip/stuck/ignore in new_normal. Only a numeric
    // value becomes persistent, and the pending slot remains readable by Lisp.
    let pending_normal = window.new_normal();
    if pending_normal.is_number() {
        match axis {
            SplitDirection::Horizontal => window.set_normal_cols(pending_normal),
            SplitDirection::Vertical => window.set_normal_lines(pending_normal),
        }
    }

    let Window::Internal {
        direction,
        children,
        ..
    } = window
    else {
        return;
    };
    let direction = *direction;
    let children = children.clone();
    let mut edge = match axis {
        SplitDirection::Horizontal => bounds.x,
        SplitDirection::Vertical => bounds.y,
    };
    for child in children {
        let Some(node) = tree.find_mut(child) else {
            continue;
        };
        let child_bounds = *node.bounds();
        let cell_edge = body::WindowBodyUnit::CanonicalChars.measure(
            resize_body_axis(axis),
            edge as i64,
            cells,
            None,
        );
        match axis {
            SplitDirection::Horizontal => {
                node.set_bounds(Rect::new(
                    edge,
                    child_bounds.y,
                    child_bounds.width,
                    child_bounds.height,
                ));
                node.set_left_col(cell_edge);
            }
            SplitDirection::Vertical => {
                node.set_bounds(Rect::new(
                    child_bounds.x,
                    edge,
                    child_bounds.width,
                    child_bounds.height,
                ));
                node.set_top_line(cell_edge);
            }
        }
        resize_apply_pixels(tree, child, axis, cells);
        if direction == axis {
            if let Some(node) = tree.find(child) {
                edge += match axis {
                    SplitDirection::Horizontal => node.bounds().width,
                    SplitDirection::Vertical => node.bounds().height,
                };
            }
        }
    }
}

/// Validate a completely staged pixel plan, as GNU window_resize_check does:
/// iso children sum to their parent, ortho children equal their parent, and
/// leaves retain at least two columns or one line. Missing staging is invalid.
#[doc(hidden)]
pub fn window_resize_check(
    tree: &WindowTree,
    id: WindowId,
    axis: SplitDirection,
    cells: body::WindowBodyCellSize,
) -> bool {
    let Some(window) = tree.find(id) else {
        return false;
    };
    let Some(staged) = window.new_pixel() else {
        return false;
    };
    if staged < 0 {
        return false;
    }
    match window {
        Window::Leaf { .. } => {
            let minimum = match axis {
                SplitDirection::Horizontal => 2,
                SplitDirection::Vertical => 1,
            };
            body::WindowBodyUnit::CanonicalChars.measure(
                resize_body_axis(axis),
                staged,
                cells,
                None,
            ) >= minimum
        }
        Window::Internal {
            direction,
            children,
            ..
        } => {
            if children.is_empty() {
                return false;
            }
            let mut remaining = staged;
            for child in children {
                let Some(child_window) = tree.find(*child) else {
                    return false;
                };
                let Some(child_staged) = child_window.new_pixel() else {
                    return false;
                };
                if !window_resize_check(tree, *child, axis, cells) {
                    return false;
                }
                if *direction == axis {
                    let Some(rest) = remaining.checked_sub(child_staged) else {
                        return false;
                    };
                    if rest < 0 {
                        return false;
                    }
                    remaining = rest;
                } else if child_staged != staged {
                    return false;
                }
            }
            *direction != axis || remaining == 0
        }
    }
}

/// Apply only character-grid sizes and edges, like GNU window_resize_apply_total.
/// Pixel geometry was already committed by window_resize_apply and must not
/// be reconstructed from rounded cell counts.
pub fn window_resize_apply_total(
    tree: &mut WindowTree,
    id: WindowId,
    horflag: bool,
    char_width: f32,
    char_height: f32,
) {
    let Some(window) = tree.find_mut(id) else {
        return;
    };
    if let Some(total) = window.new_total() {
        window.commit_cell_total(horflag, total.max(0));
    }
    let mut edge = if horflag {
        window.left_col()
    } else {
        window.top_line()
    };
    let Window::Internal {
        direction,
        children,
        ..
    } = window
    else {
        return;
    };
    let combines = (*direction == SplitDirection::Horizontal) == horflag;
    let children = children.clone();
    for child in children {
        let Some(node) = tree.find_mut(child) else {
            continue;
        };
        if horflag {
            node.set_left_col(edge);
        } else {
            node.set_top_line(edge);
        }
        window_resize_apply_total(tree, child, horflag, char_width, char_height);
        if combines && let Some(node) = tree.find(child) {
            edge += if horflag {
                node.total_columns(char_width)
            } else {
                node.total_lines(char_height)
            };
        }
    }
}

fn resize_window_subtree(tree: &mut WindowTree, window: WindowId, bounds: Rect) {
    let Some(node) = tree.find_mut(window) else {
        return;
    };
    node.set_bounds(bounds);
    let children = node.children().to_vec();
    if children.is_empty() {
        return;
    }
    for (child, child_bounds) in children
        .into_iter()
        .zip(tree.sibling_bounds_of(window, bounds))
    {
        resize_window_subtree(tree, child, child_bounds);
    }
}

fn sync_window_character_edges_from_bounds(
    tree: &mut WindowTree,
    window: WindowId,
    char_width: f32,
    char_height: f32,
) {
    let Some(node) = tree.find(window) else {
        return;
    };
    let (left_col, top_line) = (node.left_col(), node.top_line());
    sync_window_character_edges_from_bounds_at(
        tree,
        window,
        left_col,
        top_line,
        char_width,
        char_height,
    );
}

fn sync_window_character_edges_from_bounds_at(
    tree: &mut WindowTree,
    window: WindowId,
    left_col: i64,
    top_line: i64,
    char_width: f32,
    char_height: f32,
) {
    let Some(node) = tree.find_mut(window) else {
        return;
    };
    match node {
        Window::Leaf {
            cell_dimensions, ..
        }
        | Window::Internal {
            cell_dimensions, ..
        } => {
            *cell_dimensions = WindowCellDimensions::default();
        }
    }
    node.set_left_col(left_col);
    node.set_top_line(top_line);

    let parent_bounds = *node.bounds();
    let children = node.children().to_vec();
    let char_width = char_width.max(1.0);
    let char_height = char_height.max(1.0);

    for child in children {
        let Some(child_bounds) = tree.find(child).map(|node| *node.bounds()) else {
            continue;
        };
        let child_left_col =
            left_col + ((child_bounds.x - parent_bounds.x) / char_width).round() as i64;
        let child_top_line =
            top_line + ((child_bounds.y - parent_bounds.y) / char_height).round() as i64;
        sync_window_character_edges_from_bounds_at(
            tree,
            child,
            child_left_col,
            child_top_line,
            char_width,
            char_height,
        );
    }
}

// ===========================================================================
// GcTrace
// ===========================================================================

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
#[path = "tests/window_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/eob_overlay_motion_test.rs"]
mod eob_overlay_motion_tests;

#[cfg(test)]
#[path = "tests/resize_commit_test.rs"]
mod resize_commit_tests;

pub use neovm_eval::emacs_core::eval::data::window_state::MiniWindowResize;
pub(crate) use neovm_eval::emacs_core::eval::data::window_state::mini_window_rows;

pub use neovm_eval::emacs_core::eval::data::window_state::FrameFocusTracking;
