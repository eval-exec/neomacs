//! The Rust layout engine — Phase 1+2: Monospace layout with face resolution.
//!
//! Reads buffer text and display state from neovm-core, resolves faces per
//! character position, computes line breaks, positions glyphs on a fixed-width
//! grid, and publishes `FrameDisplayState` snapshots for render backends.

use neovm_core::emacs_core::image::ImageContextExt;
use neovm_core::emacs_core::neo::scroll::ScrollContextExt;
use neovm_core::emacs_core::xdisp::XdispContextExt;
use neovm_core::window::display::WindowDisplayContextExt;
mod diagnostic_knobs;
mod leaf_cursor_checkpoint;
mod window_input_ownership;
use neovm_core::window::DisplayFrameExt;
use neovm_core::window::DisplayFrameManagerExt;
use window_input_ownership::{
    resolve_window_display_source_params, resolve_window_display_source_params_legacy,
};

mod mini_eob_scroll;
mod mini_source_start;
mod mini_source_stop;
mod posn_object_extent;
mod prepared_viewports;

mod position_chrome;
mod query_cache;
mod retained_face_gather;
#[cfg(test)]
#[path = "engine/tests/retained_face_gather_support.rs"]
pub(crate) mod retained_face_gather_test_support;
mod rows_face_services;
use rows_face_services::{BoundFaceServices, FaceServiceFactory};
mod rows_fontification;
mod rows_lifetime;
mod rows_query_policy;
pub use rows_query_policy::{RowsQueryAdmission, RowsQueryPolicy};
mod scroll_coverage;
pub use scroll_coverage::ScrollCoverageProgress;
mod scroll_preview;
use prepared_viewports::PreparedViewports;

#[cfg(test)]
use super::display_status_line::eval_status_line_format;
use super::display_status_line::{
    BuiltTabBar, ChromeRowRenderServices, FrameTabBarDisplayRowRender,
    FrameTabBarDisplayRowRequest, ResizeMiniWindowsMode, ScratchGcRootScope,
    TabBarPresentedPointerPlan, build_tab_bar_display, gnu_tab_bar_pointer_appearance_style,
    max_mini_window_lines_from_value, tab_bar_effective_mouse_faces, tab_bar_image_relief_styles,
    tab_bar_pointer_slot_plan, tab_bar_presented_pointer_plan,
};
use super::gui_chrome::{
    collect_gui_menu_bar_items_for_frame, collect_gui_tool_bar_items_for_frame,
    layout_gui_compact_bar_content, layout_gui_menu_bar_content, layout_gui_tool_bar_content,
};
use super::types::*;
#[cfg(test)]
use super::window_output::RowMetricsSnapshot;
use crate::buffer_source::render_attempt::{QueryRowCoverage, WindowPositionPublication};
use crate::buffer_source::window_geometry::BufferWindowGeometryRequest;
use crate::buffer_source::window_render::{
    BufferSourceRenderAttemptContext, BufferSourceRenderAttemptOutcome, BufferWindowRenderRequest,
};
use crate::buffer_source::window_source::{BufferWindowSourceRequest, ResolvedWindowStart};
#[cfg(test)]
use crate::display_cursor::CapturedCursorVisualState;
#[cfg(test)]
use crate::display_cursor::CursorCaptureState;
#[cfg(test)]
use crate::display_cursor::CursorSlotWidthPolicy;
#[cfg(test)]
use crate::display_cursor::resolve_cursor_vertical_metrics;
#[cfg(test)]
use crate::display_cursor::{CapturedCursorInfo, CapturedCursorPlacement, CapturedCursorSlotWidth};
#[cfg(test)]
use crate::display_cursor::{CursorSlotWidthRequest, VisualCursorGeometryContext};
use crate::display_frame_output::{
    FrameContentTransitionHintRenderRequest, FrameOutputIdentity, FrameOutputOwner,
    FrameOutputStateRenderRequest, NavigationIntentObservation, WindowContentTransitionMode,
    WindowFrameDecorationsRenderRequest, WindowFrameGeometry, WindowFrameGeometryRequest,
    WindowFrameInfoEffectsRenderRequest, WindowFrameInfoRenderRequest, WindowFrameMetadata,
};
use crate::display_mock_frame::layout_mock_frame_content;
use crate::display_origin::DisplayOrigin;
use crate::display_rendered_row_output_install::frame_chrome_display_row;
use crate::display_row::face_state::DisplayRowFaceRealizer;
#[cfg(test)]
use crate::display_row::geometry::{DisplayRowMarker, DisplayRowStartMarker};
#[cfg(test)]
use crate::display_row::lisp_string::DisplayRowPrefixRequest;
#[cfg(test)]
use crate::display_row::lisp_string::DisplayRowPrefixValues;
use crate::display_row::metrics::DisplayRowFallbackMetrics;
#[cfg(test)]
use crate::display_row::overlay_string::OverlayStringRenderSource;
#[cfg(test)]
use crate::display_row::walk_state::FaceScanCheckpoint;
#[cfg(test)]
use crate::display_row::walk_state::WordWrapBreakCandidate;
#[cfg(test)]
use crate::display_row::walk_state::{
    BoxFaceRowState, DisplayRowSourceStart, HorizontalScrollDisplayItem, HorizontalScrollSkipState,
    HorizontalScrollTruncationTarget, HorizontalScrollVisibleRemainder,
    HscrollConsumedTextDisposition, InvisibleTextScanCheckpoint, LineNumberRenderState,
    TrailingWhitespaceRenderState, WordWrapRenderState,
};
use crate::display_status_line::{max_mini_window_lines, max_mini_window_lines_for_buffer};
use crate::font::frame_metrics::FrameFontDomain;
use crate::font::metrics::{FontMetricsService, FrameCellGeometry};
use crate::font::sizing::FontSizing;
use crate::frame_face_arena::{
    FaceEpoch, FrameFaceArena, FrameFaceAttempt, FrameFaceGeneration, FrameFaceReuseError,
};
use crate::frame_layout_transaction::{FrameLayoutCoordinator, FrameRelayoutRequest};
use crate::frame_visual_history::{FrameVisualHistories, FrameVisualHistory};
use crate::incremental_layout::{
    CursorOnlyReplay, EditDamage, LayoutClass, LayoutStats, MatrixValidity, RetainedWindowKey,
    RetainedWindowMatrix, ReusedMatrixRows, RowDamage, ScrollReplay,
};
use crate::layout_effect::{LayoutEffect, WindowScrollHookSite};
use crate::redisplay_fontification::VisibleFontificationCoverage;
use crate::viewport_resolution::{ForwardViewportMeasurement, ViewportDecision};
use crate::window_layout::{
    WindowChromeMetrics, WindowDividerLayout, WindowLayoutBox, WindowLayoutOutcome,
};
use neomacs_display_protocol::frame_chrome::{
    ChromeBandRequest, FrameChromeContent, FrameChromeKind as ProtocolFrameChromeKind,
    TerminalMenuBarStyle,
};
#[cfg(test)]
use neomacs_display_protocol::frame_glyphs::CursorStyle;
use neomacs_display_protocol::frame_glyphs::LineNumberFieldWidth;
use neomacs_display_protocol::frame_glyphs::PhysCursor;
use neomacs_display_protocol::frame_glyphs::WindowInfo;
use neomacs_display_protocol::types::Color;
use neomacs_display_protocol::types::DisplayWindowId;
#[cfg(test)]
use neomacs_display_protocol::types::Rect;
use neomacs_display_protocol::types::{FaceId, Px};
use neomacs_display_protocol::{MenuBarItem, ToolBarItem};
use neovm_core::emacs_core::Value;
use neovm_core::window::{
    FrameParam, WindowDisplaySnapshot, WindowPresentationSnapshot, WindowTreePath,
};

/// Bound redisplay convergence work when point begins outside the visible span.
const MAX_WINDOW_VISIBILITY_RETRIES: usize = 128;

/// Test-only probe counting nested `layout_window_rust` invocations.
///
/// The viewport retry budget must consume *iterations*, not Rust stack.
/// Production builds never reference this module.
#[cfg(test)]
#[path = "engine/tests/viewport_retry_depth_probe_test.rs"]
mod viewport_retry_depth_probe;

/// Bound intrinsic chrome convergence so oscillating status-line Lisp can
/// never publish mismatched geometry or spin forever.
const MAX_FRAME_LAYOUT_RETRIES: usize = 12;

/// Text properties that can change row structure across a localized edit.
///
/// This is a closed enum rather than a string slice so adding a new structural
/// property necessarily participates in the pre-interned symbol table below.
/// `VariantArray` and `EnumCount` keep that table exhaustive at compile time.
#[repr(usize)]
#[derive(Clone, Copy, Debug, strum::EnumCount, strum::IntoStaticStr, strum::VariantArray)]
#[strum(serialize_all = "kebab-case")]
enum EditReplayStructureProperty {
    Display,
    Invisible,
    Composition,
    LinePrefix,
    WrapPrefix,
}

/// A preencoded symbol identity used only for text-property comparisons.
///
/// Symbol IDs are append-only and never reused. This stores their bits without
/// owning a Lisp object or relying on canonicality to keep a symbol rooted.
#[derive(Clone, Copy, Debug)]
#[repr(transparent)]
struct SymbolComparisonKey(usize);

static_assertions::assert_impl_all!(SymbolComparisonKey: Send, Sync, Copy, std::fmt::Debug);
static_assertions::assert_eq_size!(SymbolComparisonKey, usize);
static_assertions::assert_eq_align!(SymbolComparisonKey, usize);
static_assertions::assert_eq_size!(Value, usize);
static_assertions::assert_eq_align!(Value, usize);

impl SymbolComparisonKey {
    fn new(id: neovm_core::emacs_core::intern::SymId) -> Self {
        Self(Value::from_sym_id(id).bits())
    }

    fn local_comparison_key(self) -> Value {
        // SAFETY: `new` stores only bits of a valid symbol ID, whose append-only
        // process identity is never reused. Value is transparently one usize
        // (pinned in neovm-core). This local copy is used by the two callers
        // below only for identity comparison, never for symbol-cell access or
        // as a GC root; no unrooted Lisp object crosses threads.
        unsafe { std::mem::transmute::<usize, Value>(self.0) }
    }
}

impl EditReplayStructureProperty {
    /// The property identities, converted to local comparison keys.
    ///
    /// The cache holds preencoded append-only SymId identity bits, even after
    /// uninterning. Both callers use these keys only for the bounded
    /// text-property scan: its presence index and plist lookup compare identity
    /// bits without reading a symbol cell. The cache owns no Lisp object or GC
    /// root; values in live interval plists supply their own roots.
    fn symbols() -> [Value; <Self as strum::EnumCount>::COUNT] {
        use neovm_core::emacs_core::intern::intern;
        use std::sync::OnceLock;
        use strum::VariantArray;

        const N: usize = <EditReplayStructureProperty as strum::EnumCount>::COUNT;
        static SYMBOLS: OnceLock<[SymbolComparisonKey; N]> = OnceLock::new();
        SYMBOLS
            .get_or_init(|| {
                std::array::from_fn(|index| {
                    SymbolComparisonKey::new(intern(
                        EditReplayStructureProperty::VARIANTS[index].into(),
                    ))
                })
            })
            .map(SymbolComparisonKey::local_comparison_key)
    }
}

/// Why layout is running.
///
/// This is deliberately a closed set: callers cannot accidentally combine a
/// logical query with renderer publication or input consumption.  A
/// synchronous query still uses the canonical window row producer, but targets
/// one window and retires its speculative output before the presentation
/// boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LayoutPurpose {
    Redisplay,
    Snapshot,
    MiniPreparation {
        window_id: neovm_core::window::WindowId,
    },
    SynchronousQuery {
        window_id: neovm_core::window::WindowId,
        scope: neovm_core::window::WindowLayoutQueryScope,
    },
}

impl LayoutPurpose {
    const fn query_window(self) -> Option<neovm_core::window::WindowId> {
        match self {
            Self::Redisplay | Self::Snapshot => None,
            Self::SynchronousQuery { window_id, .. } | Self::MiniPreparation { window_id } => {
                Some(window_id)
            }
        }
    }
}

/// Exhaustive result of one renderer-facing frame attempt.
// Built and consumed once per layout attempt on the frame hot path; boxing
// the large variant would add a heap allocation per attempt.
#[allow(clippy::large_enum_variant)]
#[must_use = "a frame attempt must be prepared or discarded"]
pub enum FrameLayoutAttempt {
    Prepared(neomacs_display_protocol::SealedFramePresentation),
    Aborted,
}

/// Accepted presentation inputs needed by a renderer-inert display query.
///
/// The fields stay private so query clients cannot forge or inspect retained
/// renderer state; only a presentation engine can produce this seed.
#[derive(Clone)]
pub struct WindowLayoutQuerySeed {
    retained_window_chrome_metrics: rustc_hash::FxHashMap<DisplayWindowId, WindowChromeMetrics>,
}

/// Canonical row producer for GNU stack-local display queries.
///
/// This type deliberately exposes no redisplay, snapshot, presentation, or
/// retained-matrix API. Rust therefore prevents a nested `window-end` call
/// from mutating or preparing the renderer transaction that invoked Lisp.
pub struct WindowLayoutQueryEngine {
    inner: LayoutEngine,
}

impl Default for WindowLayoutQueryEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowLayoutQueryEngine {
    pub fn new_without_font_metrics() -> Self {
        Self {
            inner: LayoutEngine::new_without_font_metrics(),
        }
    }

    pub fn new() -> Self {
        Self {
            inner: LayoutEngine::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test_catalog() -> Self {
        Self {
            inner: crate::font::metrics::test_catalog::layout_engine(),
        }
    }

    pub fn set_display_identity(
        &mut self,
        identity: neomacs_display_protocol::GraphicalDisplayIdentity,
    ) {
        self.inner.set_display_identity(identity);
    }

    pub fn enable_cosmic_metrics(&mut self) {
        self.inner.enable_cosmic_metrics();
    }

    pub fn disable_cosmic_metrics(&mut self) {
        self.inner.disable_cosmic_metrics();
    }

    pub fn set_font_sizing(&mut self, font_sizing: FontSizing) {
        self.inner.set_font_sizing(font_sizing);
    }

    pub fn synchronize(&mut self, seed: WindowLayoutQuerySeed) {
        if self.inner.retained_window_chrome_metrics != seed.retained_window_chrome_metrics {
            self.inner.query_cache.clear();
        }
        self.inner.retained_window_chrome_metrics = seed.retained_window_chrome_metrics;
    }

    pub fn query_window_layout(
        &mut self,
        evaluator: &mut neovm_core::emacs_core::Context,
        frame_id: neovm_core::window::FrameId,
        window_id: neovm_core::window::WindowId,
        scope: neovm_core::window::WindowLayoutQueryScope,
    ) -> Result<neovm_core::window::WindowLayoutQuery, neovm_core::window::WindowLayoutQueryFailure>
    {
        self.inner
            .query_window_layout(evaluator, frame_id, window_id, scope)
    }
}

fn resize_mini_windows_mode_for_buffer(
    evaluator: &neovm_core::emacs_core::Context,
    buffer_id: neovm_core::buffer::BufferId,
) -> ResizeMiniWindowsMode {
    // GNU `resize_mini_window` reads Vresize_mini_windows from the C global
    // with the mini window's buffer current (xdisp.c:13308-13407).
    let value = crate::neovm_bridge::visible_display_value(
        evaluator,
        buffer_id,
        crate::fixed_symbols::RESIZE_MINI_WINDOWS.id(),
    );
    ResizeMiniWindowsMode::from_lisp_value(value.as_ref())
}

fn uses_adhoc_minibuffer_resize_scroll(
    evaluator: &neovm_core::emacs_core::Context,
    buffer_id: neovm_core::buffer::BufferId,
) -> bool {
    crate::neovm_bridge::visible_display_value(
        evaluator,
        buffer_id,
        crate::fixed_symbols::REDISPLAY_ADHOC_SCROLL_IN_RESIZE_MINI_WINDOWS.id(),
    )
    .is_none_or(|value| !value.is_nil())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowLayoutWalkPurpose {
    Redisplay,
    /// GNU renderer-inert snapshot. This numeric purpose belongs to one
    /// exclusively borrowed engine/Context; independent mutators share no
    /// Lisp values or mutable cache through it.
    Snapshot,
    MiniPreparation,
    SynchronousQuery(neovm_core::window::WindowLayoutQueryScope),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowDisplaySource {
    LiveWindow,
    InactiveEchoArea,
}

struct ResolvedWindowDisplaySource {
    params: WindowParams,
    source: WindowDisplaySource,
}

fn window_position_publication(
    evaluator: &neovm_core::emacs_core::Context,
    params: &WindowParams,
    purpose: WindowLayoutWalkPurpose,
    source: WindowDisplaySource,
) -> WindowPositionPublication {
    if matches!(
        purpose,
        WindowLayoutWalkPurpose::SynchronousQuery(_) | WindowLayoutWalkPurpose::MiniPreparation
    ) {
        // `pos-visible-in-window-p`/`posn-at-point` answer from freshly measured
        // chrome (GNU `pos_visible_p`, src/xdisp.c:1732-1770); the other logical
        // queries preserve the accepted partition.
        return match purpose {
            WindowLayoutWalkPurpose::SynchronousQuery(
                neovm_core::window::WindowLayoutQueryScope::Position { .. },
            ) => WindowPositionPublication::SynchronousQueryPosition,
            WindowLayoutWalkPurpose::SynchronousQuery(
                neovm_core::window::WindowLayoutQueryScope::TextExtent { .. },
            ) => WindowPositionPublication::SynchronousQueryTextExtent,
            _ => WindowPositionPublication::SynchronousQueryEnd,
        };
    }
    if source == WindowDisplaySource::InactiveEchoArea {
        return WindowPositionPublication::InactiveEchoArea;
    }
    if purpose == WindowLayoutWalkPurpose::Snapshot {
        return WindowPositionPublication::Snapshot;
    }
    // A real GNU transaction already performed the pre-hook resize walk.
    // Its presentation now enters redisplay_window. Standalone engine calls
    // and the explicit legacy policy retain the original measurement route.
    if evaluator.gnu_redisplay_hooks_policy_enabled()
        && evaluator.gnu_redisplay_transaction_active()
    {
        return WindowPositionPublication::Redisplay;
    }
    if params.is_minibuffer() {
        let buffer_id = neovm_core::buffer::BufferId(params.buffer_id);
        if resize_mini_windows_mode_for_buffer(evaluator, buffer_id).should_grow()
            && uses_adhoc_minibuffer_resize_scroll(evaluator, buffer_id)
        {
            return WindowPositionPublication::RedisplayMinibufferMeasurement;
        }
    }
    WindowPositionPublication::Redisplay
}

/// The retained KEY with the window start the MATRIX was laid out from.
///
/// A key is taken from a window's inputs before its layout. When the layout
/// itself moves the start -- point left the window, GNU's `try_scrolling` or
/// recentering inside `redisplay_window` -- it resolves the new start within
/// the same attempt, so the key still names the old one while the matrix
/// shows the new. The next frame's inputs then carry the new start, the keys
/// differ, and that frame is laid out in full: after `M->` from a distant
/// point the first keystroke evaluated the mode line where GNU's optimization
/// 1 keeps it (GNU's `w->start` is the start it displayed), and a forced idle
/// redisplay that would have settled the key was skipped under
/// `NEOMACS_REDISPLAY_IDLE_SKIP` (the mode-line oracle's `typenofl` case).
///
/// The start is corrected only when the live window's start (LIVE_START) is
/// where the matrix's first text row begins; otherwise the key is kept, and
/// the next frame is laid out in full as before.
pub(crate) fn key_with_displayed_window_start(
    key: &RetainedWindowKey,
    matrix: &neomacs_display_protocol::glyph_matrix::GlyphMatrix,
    live_start: Option<i64>,
) -> RetainedWindowKey {
    let mut key = key.clone();
    let Some(live_start) = live_start else {
        return key;
    };
    if live_start == key.window_start {
        return key;
    }
    let first_text_row_start = matrix
        .rows
        .iter()
        .find(|row| {
            row.enabled && row.displays_text && !RetainedWindowMatrix::is_chrome_role(row.role)
        })
        .map(|row| row.start_charpos as i64);
    if first_text_row_start == Some(live_start) {
        key.window_start = live_start;
    }
    key
}

/// Select the semantic viewport start before the leaf can enter Lisp.
///
/// GNU decides its start, commits `w->start`, and runs
/// `window-scroll-functions` before `try_window` starts producing body or
/// chrome rows.  Neomacs used to hide the scrolling policy inside the source
/// read, which let rows begin hundreds of characters away from the live
/// marker and made any hook observe the wrong transaction.  Return a typed
/// resolved value so the render walk can replay this exact decision instead of
/// resolving it a second time.
fn resolve_leaf_window_start(
    evaluator: &neovm_core::emacs_core::Context,
    params: &WindowParams,
    frame_params: &FrameParams,
    layout_box: &WindowLayoutBox,
    position_publication: WindowPositionPublication,
    incremental_partial_walk: bool,
) -> ResolvedWindowStart {
    let request_max_rows = |buffer: &neovm_core::buffer::Buffer| {
        let frame_rows = frame_params.height / params.char_height.max(1.0);
        let max_mini_window_rows = if params.is_minibuffer() {
            max_mini_window_lines_for_buffer(evaluator, buffer, frame_rows)
        } else {
            max_mini_window_lines(evaluator, frame_rows)
        }
        .ceil()
        .max(1.0) as usize;
        BufferWindowGeometryRequest::new(params, layout_box, params.char_width, params.char_height)
            .with_max_mini_window_rows(max_mini_window_rows)
            .into_geometry(crate::display_row::walk_state::LineNumberFieldLayout::new(
                0,
                params.char_width,
            ))
            .max_rows
    };

    let request = evaluator
        .buffer_manager()
        .get(neovm_core::buffer::BufferId(params.buffer_id))
        .map(|buffer| {
            BufferWindowSourceRequest::from_window_params(params, request_max_rows(buffer))
        })
        .unwrap_or_else(|| BufferWindowSourceRequest::from_window_params(params, 1));

    if incremental_partial_walk
        || position_publication.uses_exact_window_start()
        || params.force_start
    {
        return request.resolve_exact();
    }

    let Some(buffer) = evaluator
        .buffer_manager()
        .get(neovm_core::buffer::BufferId(params.buffer_id))
    else {
        return request.resolve_exact();
    };
    request.resolve(&crate::neovm_bridge::RustBufferAccess::new(buffer))
}

#[cfg(test)]
#[inline]
fn cursor_point_columns(text: &[u8], byte_idx: usize, col: i32, params: &WindowParams) -> usize {
    CursorSlotWidthRequest::from_window_params(CursorStyle::FilledBox, text, byte_idx, col, params)
        .point_columns()
}

#[cfg(test)]
#[inline]
fn cursor_width_for_style(
    style: CursorStyle,
    text: &[u8],
    byte_idx: usize,
    col: i32,
    params: &WindowParams,
    face_char_w: f32,
) -> f32 {
    CursorSlotWidthRequest::from_window_params(style, text, byte_idx, col, params)
        .width_px(face_char_w)
}

/// What an inactive mini-window's layout must render.
///
/// GNU's `with_echo_area_buffer` (xdisp.c:12961, :13038) temporarily installs
/// the echo-area buffer in the mini-window **while a message is displayed**
/// and restores `w->contents` on unwind; with no message up, the window
/// renders its own buffer, which GNU keeps pointing at the
/// permanently-inactive minibuffer `" *Minibuf-0*"` (minibuf.c:1281-1284,
/// set at startup and re-installed by `minibuffer_unwind` after every
/// minibuffer exit).  Package code such as `minibuffer-line' (GNU ELPA)
/// relies on that idle state: it writes status text into `" *Minibuf-0*"`
/// and the mini-window must show it whenever no message is up.
pub(crate) enum InactiveMiniWindowSource {
    /// A message is up (`current_message` non-nil): render the transient
    /// echo buffer the message text is mirrored into.
    EchoMessage,
    /// No message: render the window's own buffer (`" *Minibuf-0*"`).
    OwnBuffer,
}

impl InactiveMiniWindowSource {
    fn for_context(evaluator: &neovm_core::emacs_core::Context) -> Self {
        if evaluator.has_current_message() {
            Self::EchoMessage
        } else {
            Self::OwnBuffer
        }
    }
}

fn window_source_has_fontification_callbacks(
    evaluator: &neovm_core::emacs_core::Context,
    buffer_id: u64,
) -> bool {
    // GNU reads Vfontification_functions with the window's buffer current
    // (xdisp.c:4681-4697).
    crate::neovm_bridge::visible_display_value(
        evaluator,
        neovm_core::buffer::BufferId(buffer_id),
        crate::fixed_symbols::FONTIFICATION_FUNCTIONS.id(),
    )
    .is_some_and(|value| !value.is_nil())
}

/// Canonical live inputs for one leaf at a Lisp-visible layout boundary.
///
/// Phase A is an optimization snapshot. Earlier siblings and the current
/// window's scroll hook may run arbitrary Lisp before this leaf is walked, so
/// the accepted layout must be rebuilt from live state at the boundary instead
/// of patching selected fields in an old `WindowParams` value.
struct LiveWindowLayoutInputs {
    frame: FrameParams,
    window: WindowParams,
    source: WindowDisplaySource,
    main_area_bottom: f32,
    divider_corner: super::neovm_bridge::DividerCornerPolicy,
}

fn live_window_frame_metadata(
    evaluator: &neovm_core::emacs_core::Context,
    buffer_id: neovm_core::buffer::BufferId,
) -> WindowFrameMetadata {
    let buffer = evaluator.buffer_manager().get(buffer_id);
    WindowFrameMetadata {
        buffer_name: buffer
            .map(|buffer| buffer.name_runtime_string_owned())
            .unwrap_or_default(),
        buffer_file_name: buffer
            .and_then(|buffer| buffer.file_name_runtime_string_owned())
            .unwrap_or_default(),
        modified: buffer.is_some_and(|buffer| buffer.is_modified()),
    }
}

struct FrameFontInputs {
    bootstrap: FrameParams,
    window_system: Option<String>,
}

struct PreparedFrameFonts {
    resolver: super::neovm_bridge::FaceResolver,
    default_metrics: Option<crate::font::metrics::FontMetrics>,
    face_epoch: crate::frame_face_arena::FaceEpoch,
}

#[allow(clippy::too_many_arguments)]
fn collect_live_window_layout_inputs(
    evaluator: &mut neovm_core::emacs_core::Context,
    frame_id: neovm_core::window::FrameId,
    window_id: neovm_core::window::WindowId,
    window_path: &WindowTreePath,
    default_font_ascent: Option<f32>,
    font_sizing: FontSizing,
    accepted_chrome: &rustc_hash::FxHashMap<DisplayWindowId, WindowChromeMetrics>,
    purpose: WindowLayoutWalkPurpose,
) -> Option<LiveWindowLayoutInputs> {
    // Recollect only the target leaf. A checked path costs O(tree depth), while
    // repeating a full-frame bridge walk would cost O(windows) per leaf. The
    // root window's canonical bounds supply the same main-area bottom as the
    // maximum of its partitioned leaves.
    let (frame, mut window, main_area_bottom, divider_corner) = {
        let (live_frame, live_window) = evaluator
            .frame_manager()
            .frame_and_window_at_path(frame_id, window_path)?;
        if live_window.id() != window_id {
            return None;
        }
        let buffer_id = match purpose {
            WindowLayoutWalkPurpose::SynchronousQuery(
                neovm_core::window::WindowLayoutQueryScope::TextExtent { buffer, .. },
            ) => buffer,
            _ => live_window.buffer_id()?,
        };
        let buffer = evaluator.buffer_manager().get(buffer_id)?;
        let frame_is_selected = evaluator
            .frame_manager()
            .selected_frame()
            .is_some_and(|selected| selected.id == frame_id);
        let is_selected = frame_is_selected && live_frame.selected_window == window_id;
        let cursor_role = match purpose {
            WindowLayoutWalkPurpose::Redisplay | WindowLayoutWalkPurpose::Snapshot => {
                super::neovm_bridge::redisplay_cursor_target(evaluator, frame_id)
                    .role_for(window_id)
            }
            WindowLayoutWalkPurpose::SynchronousQuery(_)
            | WindowLayoutWalkPurpose::MiniPreparation => {
                WindowCursorRole::from_active(is_selected)
            }
        };
        let mode_line_active = frame_is_selected
            && (is_selected || evaluator.minibuffer_selected_window_id() == Some(window_id));
        let is_minibuffer = live_frame.minibuffer_window == Some(window_id);
        let cursor_type = live_window
            .display()
            .map_or(Value::T, |display| display.cursor_type);
        let cursor_effect =
            super::neovm_bridge::window_parameter_by_name(live_window, "neomacs-cursor-effect")
                .unwrap_or(Value::NIL);
        let window = super::neovm_bridge::window_params_from_neovm_with_font_sizing(
            live_window,
            buffer,
            live_frame,
            evaluator.variables(),
            evaluator.face_table(),
            default_font_ascent,
            super::neovm_bridge::WindowDisplayRole {
                is_selected,
                cursor_role,
                mode_line_active,
                is_minibuffer,
            },
            cursor_type,
            cursor_effect,
            // x_set_cursor_color reads Vx_cursor_fore_pixel in the command
            // context (xfns.c:1482-1490); resolve the visible value there.
            super::neovm_bridge::visible_value(
                evaluator,
                crate::fixed_symbols::X_CURSOR_FORE_PIXEL.id(),
            ),
            font_sizing,
        )?;
        let root_bounds = live_frame.root_window().bounds();
        (
            super::neovm_bridge::frame_params_from_neovm(
                live_frame,
                evaluator.face_table(),
                evaluator.variables(),
            ),
            window,
            root_bounds.y + root_bounds.height,
            super::neovm_bridge::divider_corner_at_path(live_frame, window_path)?,
        )
    };
    if let Some(metrics) = accepted_chrome.get(&DisplayWindowId::new(window.window_id)) {
        metrics.seed_params(&mut window);
    }
    let source = match window_input_ownership::mode() {
        window_input_ownership::WindowInputOwnership::InPlace => {
            resolve_window_display_source_params(evaluator, &mut window, purpose)
        }
        window_input_ownership::WindowInputOwnership::LegacyCopies => {
            let resolved = resolve_window_display_source_params_legacy(evaluator, &window, purpose);
            window = resolved.params;
            resolved.source
        }
    };
    Some(LiveWindowLayoutInputs {
        frame,
        window,
        source,
        main_area_bottom,
        divider_corner,
    })
}

fn max_mini_window_lines_for_window(
    evaluator: &mut neovm_core::emacs_core::Context,
    params: &WindowParams,
    frame_rows: f32,
) -> f32 {
    let window_id = neovm_core::window::WindowId(params.window_id as u64);
    let buf_id = if params.is_minibuffer() && !evaluator.minibuffer_window_is_active(window_id) {
        evaluator.ensure_echo_area_buffers();
        evaluator
            .echo_area_display_buffer()
            .unwrap_or(neovm_core::buffer::BufferId(params.buffer_id))
    } else {
        neovm_core::buffer::BufferId(params.buffer_id)
    };
    let raw = crate::neovm_bridge::visible_display_value(
        evaluator,
        buf_id,
        crate::fixed_symbols::MAX_MINI_WINDOW_HEIGHT.id(),
    )
    .unwrap_or_else(|| Value::make_float(0.25));
    max_mini_window_lines_from_value(raw, frame_rows)
}

fn tab_bar_button_relief_geometry(evaluator: &neovm_core::emacs_core::Context) -> (f32, f32, f32) {
    // Global defcustoms (no make-local in GNU lisp); the C global read sees a
    // live `let`.
    let margin = crate::neovm_bridge::visible_value(
        evaluator,
        crate::fixed_symbols::TAB_BAR_BUTTON_MARGIN.id(),
    )
    .unwrap_or_else(|| Value::fixnum(1));
    let (horizontal_margin, vertical_margin) = if let Some(value) = margin.as_int() {
        let value = value.max(0) as f32;
        (value, value)
    } else if margin.is_cons() {
        (
            margin.cons_car().as_int().unwrap_or(1).max(0) as f32,
            margin.cons_cdr().as_int().unwrap_or(1).max(0) as f32,
        )
    } else {
        (1.0, 1.0)
    };
    let configured_thickness = crate::neovm_bridge::visible_value(
        evaluator,
        crate::fixed_symbols::TAB_BAR_BUTTON_RELIEF.id(),
    )
    .and_then(Value::as_int)
    .unwrap_or(1);
    let thickness = if configured_thickness < 0 {
        1.0
    } else {
        configured_thickness.min(1_000_000) as f32
    };
    (horizontal_margin, vertical_margin, thickness)
}

/// The main Rust layout engine.
///
/// Called on the Emacs thread during redisplay. Reads buffer/state from
/// neovm-core, resolves faces, computes layout, and publishes immutable
/// display snapshots for the render thread and TTY backend.
pub struct LayoutEngine {
    /// Reusable text buffer to avoid allocation per frame
    text_buf: Vec<u8>,
    /// Hit-test data being built for current frame
    /// Authoritative visible glyph geometry published back into core state.
    /// Presentation geometry paired with its live-publication domain.
    ///
    /// Keeping both in one enum prevents temporary sources such as GNU's
    /// inactive echo area from being detached from their cache policy while
    /// the frame converges.
    window_snapshots: Vec<WindowPresentationSnapshot>,
    query_restart_rows: Vec<(neovm_core::buffer::LispCharPos1, i64)>,
    query_row_coverage: QueryRowCoverage,
    query_target_prefix_policy: bool,
    /// One renderer-inert full mini measurement. Owned numeric attempt result;
    /// never shared with other Context mutators or retained presentations.
    mini_preparation_height: Option<f32>,
    /// Granted only by the canonical walk's semantic reuse barrier. Geometry
    /// queries with evaluated Lisp conditions must never skip that walk.
    query_body_cache_state: QueryBodyCacheState,
    query_cache: query_cache::QueryCache,
    /// Cosmic-text font metrics service.
    ///
    /// Populated by `enable_cosmic_metrics()` at GUI startup. Left
    /// `None` for TTY mode, where all measurements go through the
    /// character-cell grid. Replaces the previous
    /// `use_cosmic_metrics: bool` runtime flag — the decision is
    /// now made once at startup by the binary that constructs the
    /// layout engine.
    pub font_metrics: Option<FontMetricsService>,
    /// Connection binding survives disabling or lazily recreating font metrics.
    display_identity: Option<neomacs_display_protocol::GraphicalDisplayIdentity>,
    /// Converts Emacs face height units into layout pixels for this display.
    font_sizing: FontSizing,
    /// Last accepted visual history, isolated by logical frame like GNU's
    /// per-frame current/desired matrices.
    frame_visual_histories: FrameVisualHistories,
    /// Authoritative frame output owner for the current frame layout pass.
    frame_output: FrameOutputOwner,
    /// Source-addressed tab-bar pointer plan awaiting canonical glyph indices.
    pending_tab_bar_pointer: Option<TabBarPresentedPointerPlan>,
    /// The last completed `FrameDisplayState`, produced by `layout_frame_rust()`.
    /// Used by the TTY redisplay path to drive `TtyRif` on the evaluator thread.
    pub last_frame_display_state: Option<neomacs_display_protocol::SealedFramePresentation>,
    /// Certified semantic rows survive full-frame transfer and renewal-only
    /// publications, independently of the renderer's retained paint surface.
    scroll_preview_coverage: scroll_preview::ScrollPreviewCoverage,
    /// Last sealed face namespace for each logical frame.
    ///
    /// A speculative layout gets a fresh [`FrameFaceAttempt`] from this arena;
    /// retries discard that attempt, and only a sealed presentation replaces
    /// the committed arena.
    frame_face_arenas: rustc_hash::FxHashMap<neovm_core::window::FrameId, FrameFaceArena>,
    /// The previous frame layout's composed-source cache and the face table
    /// snapshot it was folded against.
    ///
    /// A [`super::neovm_bridge::FaceResolver`] lives for one redisplay
    /// transaction; without this pin, every transaction refolded every named
    /// face from the table and rebuilt its composed sources from scratch.
    /// GNU keeps a frame's face cache across redisplays and frees it only when
    /// a face definition changes (`face_change`, src/xfaces.c:349 and
    /// src/xfaces.c:3098-3105) — the pinned snapshot plays that role here.
    face_composed_sources: Option<super::neovm_bridge::ComposedSourcePin>,
    /// The previous frame layout's realized terminal faces, valid while the
    /// binding stamp and table snapshot recorded with them still hold. This is
    /// GNU's per-frame realized-face reuse (src/xfaces.c:3098-3105) carried
    /// across the per-transaction resolver boundary.
    face_tty_realization: Option<super::neovm_bridge::TtyRealizationPin>,
    /// Last accepted layout per window, used by cursor, scroll, and edit
    /// replay. Speculative attempts never replace this state.
    retained_window_matrices: rustc_hash::FxHashMap<DisplayWindowId, RetainedWindowMatrix>,
    /// Resolved automatic-composition widths for the current frame's windows,
    /// parked and loaded with `retained_window_matrices`.
    retained_composition_advances:
        rustc_hash::FxHashMap<DisplayWindowId, RetainedCompositionAdvances>,
    prepared_viewports: PreparedViewports,
    scroll_coverage: scroll_coverage::ScrollCoverage,
    /// Every OTHER frame's retained state, parked while this one is laid out.
    ///
    /// One `LayoutEngine` serves every visible frame -- `RedisplayRuntime` owns
    /// exactly one (`redisplay.rs:106`) and the drivers lay out the root frame
    /// and then each visible child through it. The two maps above are keyed by
    /// WINDOW alone, so replacing them wholesale at the accepted break used to
    /// discard every window of every other frame: with a corfu or
    /// vertico-posframe popup, a tooltip, lsp-ui-doc or a second top-level
    /// frame visible, every window lost its retained matrix on each cycle and
    /// all three fast paths died.
    ///
    /// The fix that reads as an invariant rather than as bookkeeping: the two
    /// maps above hold exactly ONE frame's windows -- the frame named by
    /// `retained_frame` -- and everything else waits here. Wholesale
    /// replacement is then correct by construction, and it still prunes that
    /// frame's own deleted windows, which is what it was always for.
    retained_by_frame: rustc_hash::FxHashMap<neovm_core::window::FrameId, RetainedFrameState>,
    /// Which frame [`Self::retained_window_matrices`] currently belongs to.
    retained_frame: Option<neovm_core::window::FrameId>,
    /// Accepted intrinsic chrome metrics, the Rust equivalent of GNU's
    /// current-matrix tab/header/mode-line heights.  They seed the next
    /// speculative layout and are replaced only by a sealed frame.
    retained_window_chrome_metrics: rustc_hash::FxHashMap<DisplayWindowId, WindowChromeMetrics>,
    /// Windows that took the Phase 1 cursor-only fast path this frame (their body
    /// rows were reused, not relaid). Populated as each window is laid out, read
    /// by the commit path to attribute rows to `reused_rows` and classify the
    /// window `CursorOnly`. Reset per frame.
    cursor_only_window_ids: rustc_hash::FxHashSet<DisplayWindowId>,
    prepared_window_ids: rustc_hash::FxHashSet<DisplayWindowId>,
    /// Windows that took the Phase 2 pure-scroll fast path this frame, mapped to
    /// `(exact_reused_rows, dvpos)`. Read by the commit path to attribute
    /// rows + classify `Scroll` + emit `RowDamage::ReusedShifted`.
    scroll_window_ids: rustc_hash::FxHashMap<DisplayWindowId, (ReusedMatrixRows, f32)>,
    /// Windows that took the Phase 3 localized-edit fast path this frame, mapped
    /// to the exact matrix rows reused verbatim on either side of the relaid
    /// span. Read by the commit path to attribute rows + classify `Edit`.
    edit_window_ids: rustc_hash::FxHashMap<DisplayWindowId, ReusedMatrixRows>,
    /// Per-buffer dirty span snapshotted BEFORE this frame's fontification
    /// pass (GNU: the this_line decision reads BEG/END_UNCHANGED before
    /// fontification fires). Phase A edit classification consumes this so the
    /// span is the keystroke's damage, not the jit-lock chunk the
    /// fontification pass is about to rewrite. Keyed by buffer id.
    pre_fontify_dirty_spans: rustc_hash::FxHashMap<u64, Option<(i64, i64)>>,
    /// GNU `BEG_UNCHANGED` per dirty buffer, snapshotted with
    /// [`Self::pre_fontify_dirty_spans`]: the same prefix, but with property
    /// changes counted one char early (textprop.c:87). Only the mode-line
    /// gate (`NEOMACS_MODE_LINE_GATE=gnu`) reads it.
    pre_fontify_gnu_beg_unchanged: rustc_hash::FxHashMap<u64, i64>,
    /// GNU `consider_all_windows_p` as `bset_redisplay` raises it across all
    /// frames: a buffer changed that a non-selected window shows, or that two
    /// windows show (xdisp.c:886-898). Computed before fontification in Phase A.
    frame_other_windows_changed: bool,
    /// Phase 3 below-reuse switch (default true). The localized edit fast path
    /// reuses the rows BELOW the dirty span too (charpos-shifted, same pixel_y),
    /// relaying ONLY the edited line — but ONLY for a simple insert that provably
    /// keeps the edited line one row (the ASCII gate in `build_edit_replay` + the
    /// width gate in `edit_replay`); a newline/tab/wide/wrapping insert or a
    /// delete falls back to above-only. Settable for tests.
    allow_below_reuse: bool,
    /// Instrumentation from the most recent `layout_frame_rust` pass: the
    /// relaid-row-count gate metric (spec §7). Reset per frame.
    layout_stats: LayoutStats,
}

/// Ownership of the numeric body observation's read journal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QueryBodyCacheState {
    Disabled,
    Produced,
    Reused,
}

fn log_invalid_face_references(evaluator: &mut neovm_core::emacs_core::Context, names: &[String]) {
    for name in names {
        evaluator.add_to_log(&format!("Invalid face reference: {name}"));
    }
}

/// One window's incremental-layout decision, gathered before the frame admits
/// any retained face IDs or allocates any fresh ones.
///
/// Keeping the alternatives in one type makes the face namespace a property of
/// the frame plan, rather than an ordering side effect of whichever window
/// happens to render first.
struct IncrementalWindowPlan {
    prepared_faces: Option<crate::frame_face_arena::PreparedFaceSnapshot>,
    cursor_only: Option<CursorOnlyReplay>,
    scroll: Option<ScrollReplay>,
    is_edit: bool,
    /// Sorted union of edit replay and continuation-row faces admitted in
    /// Phase A, captured for a bounded retry that may reclassify the edit into
    /// a scroll. Retry subset checks rely on this ordering for binary search.
    retry_admitted_faces: Option<Vec<FaceId>>,
    /// Every enabled retained row for plans without a replay or with an edit
    /// replay. Either can discover a new viewport while measuring and continue
    /// as a scroll. Phase A must admit these faces before the leaf allocates
    /// fresh IDs; the continuation cannot admit retained IDs afterward.
    scroll_continuation_faces: Option<(
        FrameFaceGeneration,
        Vec<neomacs_display_protocol::glyph_matrix::MatrixRow>,
    )>,
}

/// Lisp-derived GUI chrome semantics prepared before any window rows.
///
/// GNU's `prepare_menu_bars` updates menu, tab, then tool bars before
/// `redisplay_windows`. Keeping the evaluated semantics here lets physical
/// layout retries reuse one logical callback result while post-leaf code
/// remains pure positioning and emission.
struct PreparedGuiChromeSemantics {
    menu_items: Vec<MenuBarItem>,
    built_tab_bar: Option<BuiltTabBar>,
    tool_items: Vec<ToolBarItem>,
}

impl PreparedGuiChromeSemantics {
    fn collect(
        evaluator: &mut neovm_core::emacs_core::Context,
        frame_id: neovm_core::window::FrameId,
        gc_roots: &ScratchGcRootScope,
    ) -> Option<Self> {
        let is_root_frame = evaluator
            .frame_manager()
            .get(frame_id)
            .is_some_and(|frame| frame.parent_frame.as_frame_id().unwrap_or(0) == 0);
        if !is_root_frame {
            return None;
        }
        let needs_menu_items = evaluator
            .frame_manager()
            .get(frame_id)
            .is_some_and(|frame| {
                frame.menu_bar_height > 0
                    || (frame.compact_bar_height > 0
                        && frame
                            .known_frame_parameter_int(FrameParam::MenuBarLines)
                            .is_some_and(|lines| lines > 0))
            });
        let menu_items = if needs_menu_items {
            collect_gui_menu_bar_items_for_frame(evaluator, frame_id)
        } else {
            Default::default()
        };
        let needs_tab_bar = evaluator
            .frame_manager()
            .get(frame_id)
            .is_some_and(|frame| frame.tab_bar_height > 0);
        let built_tab_bar = needs_tab_bar
            .then(|| build_tab_bar_display(evaluator, frame_id.0, gc_roots))
            .flatten();
        // Tab-bar Lisp may change `tool-bar-lines`. GNU computes the tool-bar
        // update predicate only after `update_tab_bar` returns, so re-read the
        // live frame instead of using a gate captured before entering Lisp.
        let needs_tool_items = evaluator
            .frame_manager()
            .get(frame_id)
            .is_some_and(|frame| {
                frame.tool_bar_height > 0
                    || frame
                        .known_frame_parameter_int(FrameParam::ToolBarLines)
                        .is_some_and(|lines| lines > 0)
            });
        let tool_items = if needs_tool_items {
            collect_gui_tool_bar_items_for_frame(evaluator, frame_id)
        } else {
            Default::default()
        };
        Some(Self {
            menu_items,
            built_tab_bar,
            tool_items,
        })
    }
}

/// Result of one leaf attempt before the frame coordinator classifies it.
///
/// `Effect` is affine work: the row producer cannot proceed until the caller
/// executes it and recollects the leaf's complete live input projection.
// Built and consumed once per layout attempt on the frame hot path; boxing
// the large variant would add a heap allocation per attempt.
#[allow(clippy::large_enum_variant)]
/// Frame layout pass mode inside `'frame_layout`.
///
/// Ordinary attempts run in `Converging` mode with callbacks enabled. When a
/// `LayoutPurpose::Redisplay` frame exhausts its bounded retry budget without
/// a pending non-local `Flow` and while the frame is still live, the coordinator
/// switches to ONE callback-inhibited `ExhaustionSnapshot` pass so the frame
/// publishes coherently (mirroring GNU's fall-through to `update_frame` at
/// retry exhaustion, `src/xdisp.c:17926-17973, 18054-18091`) instead of leaving
/// windows permanently blank.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrameAttemptMode {
    Converging,
    ExhaustionSnapshot,
}

/// Typed outcome of one internal frame layout invocation.
///
/// Replaces re-entrant boolean flags on `LayoutEngine` so nested synchronous
/// queries cannot clobber a redisplay attempt's NoSafe settlement decision.
enum FrameLayoutInnerOutcome {
    Prepared(Option<neovm_core::window::WindowLayoutQuery>),
    Aborted { settle_no_safe: bool },
}

impl FrameLayoutInnerOutcome {
    fn into_query(self) -> Option<neovm_core::window::WindowLayoutQuery> {
        match self {
            Self::Prepared(query) => query,
            Self::Aborted { .. } => None,
        }
    }
}

/// Action returned by `decide_frame_relayout` when a speculative pass requests
/// a whole-frame retry.
enum RelayoutDecision {
    Retry,
    EnterExhaustionSnapshot,
    Abort { settle_no_safe: bool },
}

/// Bounded leaf-local recollection state after pre-walk Lisp (`Prepass`
/// fontification or pre-walk `display (when ...)`). Transitions strictly
/// forward at most once per leaf per pass so it can never loop.
enum PrepassRecollectState {
    /// Initial leaf pass: run Prepass fontification and pre-walk `display (when ...)`.
    Fresh,
    /// Already recollected once after Prepass fontification mutated inputs.
    /// Skip Prepass fontification; run pre-walk `display (when ...)` on fresh inputs.
    AfterFontification {
        outcome: neovm_core::emacs_core::xdisp::RedisplayFontificationOutcome,
        fontify_end: i64,
    },
    /// Already recollected once after pre-walk `display (when ...)` mutated inputs.
    /// Skip both Prepass fontification and pre-walk `display (when ...)`; walk immediately.
    AfterDisplayWhen {
        outcome: neovm_core::emacs_core::xdisp::RedisplayFontificationOutcome,
        fontify_end: i64,
        display_when: crate::display_when::DisplayWhenConditions,
    },
}

enum LeafLayoutAttempt {
    Completed {
        outcome: WindowLayoutOutcome,
        window_end_attempt: Option<neovm_core::window::WindowEndAttempt>,
        /// Attempt-owned geometry; retries cannot publish a candidate.
        snapshot: Option<WindowPresentationSnapshot>,
        produced_key: Option<RetainedWindowKey>,
    },
    Effect(LayoutEffect),
    /// The attempt unwound and asks the window loop to re-enter with these
    /// inputs instead of nesting another `layout_window_rust` frame.  The
    /// visibility retry budget must consume *iterations*, never Rust stack:
    /// each `layout_window_rust` frame is large (image parsing, font
    /// resolution, row walks), and a bounded retry chain of nested frames
    /// still overflows a profiling evaluator's stack well before the
    /// numeric budget is spent.
    Retry(Box<WindowLayoutRetry>),
    /// The walk consumed at or past the window-range overlay view's end. The
    /// attempt was answered from a subset and is discarded; the window loop
    /// retries once with the whole-buffer overlay view.
    OverlayViewExhausted,
    /// The row walk painted an automatic composition whose shaped-gstring
    /// width the visible-range resolution pass did not reach, so the row
    /// would have measured the raw text instead of GNU's gstring width. The
    /// attempt is discarded; the loop retries with the resolution range
    /// widened to `resolve_from..resolve_through`, then (if that still
    /// misses) to every span the snapshot captured.
    AutomaticCompositionUnresolved {
        resolve_from: usize,
        resolve_through: usize,
    },
    /// Pre-walk Lisp (`Prepass` fontification or pre-walk `display (when ...)`)
    /// mutated source inputs while window topology and face definitions stayed
    /// unchanged. Recollect live inputs for this leaf once and walk immediately
    /// without burning a frame retry.
    PrepassInputsChanged(Box<PrepassRecollectState>),
    LogicalInputsChanged,
    MatrixCapacityUnavailable,
    /// A `when` clause the walk took by the structural rule evaluated nil
    /// after the walk: relayout the frame once with the evaluated answers,
    /// which the ledger carries (`display_when::settle_display_when_visits`).
    DisplayWhenReplay,
}

/// Iterative continuation of `layout_window_rust` for one visibility retry.
struct WindowLayoutRetry {
    params: WindowParams,
    remaining_visibility_retries: usize,
    viewport_resolution: ViewportResolutionPhase,
}

/// Whether this leaf is choosing, measuring, or committing its viewport.
///
/// A measurement start drives the display iterator but is never Lisp-visible.
/// A committed start has already been selected from measured rows and bypasses
/// the preflight estimator on its final rendering pass.
#[derive(Clone, Debug)]
enum ViewportResolutionPhase {
    Resolve,
    Measure(ForwardViewportMeasurement),
    Commit(ResolvedWindowStart),
}

/// Affine live-window publications accumulated by one speculative frame walk.
///
/// Status-line Lisp must see each body's just-produced window end, but those
/// values become accepted viewport evidence only when the whole frame
/// converges. A later sibling or minibuffer retry therefore rejects every
/// staged leaf in reverse publication order.
#[derive(Default)]
#[must_use = "a frame's speculative window ends must be accepted or rejected"]
struct FrameWindowEndAttempts {
    pending: Vec<neovm_core::window::WindowEndAttempt>,
}

impl FrameWindowEndAttempts {
    fn stage(&mut self, attempt: Option<neovm_core::window::WindowEndAttempt>) {
        if let Some(attempt) = attempt {
            self.pending.push(attempt);
        }
    }

    fn reject_all(&mut self, evaluator: &mut neovm_core::emacs_core::Context) {
        for attempt in self.pending.drain(..).rev() {
            evaluator.reject_redisplay_window_end_attempt(attempt);
        }
    }

    fn accept_all(&mut self) {
        for attempt in self.pending.drain(..) {
            attempt.accept();
        }
    }
}

/// Owner and physical inputs of one completed body, retained only until this
/// frame attempt has crossed every sibling's Lisp callback boundary.
struct CompletedBodyIdentity {
    source_buffer: neovm_core::buffer::BufferId,
    inputs: neovm_core::window::WindowLayoutAttemptFreshness,
}

/// Callback acknowledgements and producer artifacts of one logical frame attempt.
/// Chrome/minibuffer retries must not replay its acknowledged Lisp callbacks.
#[derive(Default)]
struct RedisplayLispLedger {
    rows_fontification: rows_fontification::RowsFontificationDemand,
    acknowledged_scroll_hooks: rustc_hash::FxHashSet<WindowScrollHookSite>,
    exact_hook_resumes: rustc_hash::FxHashMap<neovm_core::window::WindowId, ResolvedWindowStart>,
    mini_eob_scroll: mini_eob_scroll::MiniEobScrollLedger,
    /// `when` answers evaluated after a walk, carried to its one replay.
    display_when: crate::display_when::DisplayWhenReplayLedger,
    /// Original authority selected with a historical/prepared row plan.
    replay_sources: rustc_hash::FxHashMap<
        DisplayWindowId,
        crate::cursor_source_dependencies::CursorSourceDependencies,
    >,
    /// Cleared on physical frame retry; completed siblings retain their owner
    /// identities until final whole-frame admission, independently of source freshness.
    completed_body_inputs: rustc_hash::FxHashMap<DisplayWindowId, CompletedBodyIdentity>,
    /// First window whose finished walk asked for one source-only frame retry.
    /// Counted once, after every sibling has been assembled, not once per leaf.
    source_only_retry: Option<DisplayWindowId>,
    /// Original source observations commit only with the accepted matrices.
    cursor_sources: rustc_hash::FxHashMap<
        DisplayWindowId,
        crate::cursor_source_dependencies::CursorSourceDependencies,
    >,
    /// Most recent non-flow chrome outputs evaluated for each `(window, kind)`
    /// during this frame's retry attempts. Values are rooted in the frame
    /// attempt's `ScratchGcRootScope` and reused by `ExhaustionSnapshot`.
    captured_chrome_outputs: crate::display_status_line::CapturedChromeOutputs,
    /// Number of post-prepass freshness rechecks executed during this frame.
    prepass_freshness_rechecks: usize,
}

/// A rejected body-source projection cannot retain its speculative end.
fn reject_changed_cursor_sources(
    evaluator: &mut neovm_core::emacs_core::Context,
    window_end_attempt: &mut Option<neovm_core::window::WindowEndAttempt>,
    changed: bool,
) -> bool {
    if changed && let Some(attempt) = window_end_attempt.take() {
        evaluator.reject_redisplay_window_end_attempt(attempt);
    }
    changed
}

impl RedisplayLispLedger {
    fn publication_for_site(
        &self,
        publication: WindowPositionPublication,
        site: WindowScrollHookSite,
    ) -> WindowPositionPublication {
        if matches!(publication, WindowPositionPublication::Redisplay)
            && self.acknowledged_scroll_hooks.contains(&site)
        {
            WindowPositionPublication::RedisplayResumedScrollHook
        } else {
            publication
        }
    }

    fn acknowledge_scroll_hook(&mut self, effect: &LayoutEffect) {
        let site = effect.scroll_hook_site();
        self.acknowledged_scroll_hooks.insert(site);
        self.mini_eob_scroll.acknowledge(site);
    }

    /// GNU resumes the same callback site from the start after Lisp has had a
    /// chance to rewrite `w->start`; that immediate continuation is not a new
    /// scroll decision. Record the reread start as part of the acknowledgement
    /// while leaving later visibility/recenter decisions distinct.
    fn acknowledge_hook_resume(
        &mut self,
        window_id: neovm_core::window::WindowId,
        window_start: ResolvedWindowStart,
    ) {
        self.acknowledged_scroll_hooks
            .insert(WindowScrollHookSite::new(window_id, window_start));
        self.exact_hook_resumes.insert(window_id, window_start);
    }

    /// Capture GNU's immediate post-hook `w->start` reread by live identity.
    ///
    /// This must happen before reacting to a topology mutation: the old tree
    /// path is already stale, but the hook target may still be live in the new
    /// tree and its explicitly chosen start remains authoritative for the
    /// resumed redisplay.
    fn acknowledge_live_hook_resume(
        &mut self,
        evaluator: &neovm_core::emacs_core::Context,
        frame_id: neovm_core::window::FrameId,
        window_id: neovm_core::window::WindowId,
    ) {
        let Some(window_start) = evaluator
            .frame_manager()
            .get(frame_id)
            .and_then(|frame| frame.find_window(window_id))
            .and_then(neovm_core::window::Window::window_start)
        else {
            return;
        };
        self.acknowledge_hook_resume(
            window_id,
            ResolvedWindowStart::from_layout_charpos(crate::coords::lisp_char_pos_to_layout_i64(
                window_start,
            )),
        );
        tracing::debug!(
            window = window_id.0,
            post_hook_start = window_start.as_i64(),
            "captured post-scroll-hook window start"
        );
    }

    fn exact_hook_resume(
        &mut self,
        window_id: neovm_core::window::WindowId,
        live_window_start: ResolvedWindowStart,
    ) -> Option<ResolvedWindowStart> {
        let resume = self.exact_hook_resumes.get(&window_id).copied()?;
        tracing::debug!(
            window = window_id.0,
            live_start = live_window_start.get(),
            resume_start = resume.get(),
            "checking exact scroll-hook continuation"
        );
        if resume == live_window_start {
            Some(resume)
        } else {
            // Lisp after the hook (fontification, body display properties, or
            // chrome) installed a newer canonical start. The continuation is
            // valid only for the exact start reread immediately after its
            // callback; it must never overwrite later Lisp state on a retry.
            self.exact_hook_resumes.remove(&window_id);
            None
        }
    }

    fn finish_hook_resume(&mut self, window_id: neovm_core::window::WindowId) {
        self.exact_hook_resumes.remove(&window_id);
    }
}

impl IncrementalWindowPlan {
    fn retained_face_generation(&self) -> Option<FrameFaceGeneration> {
        self.cursor_only
            .as_ref()
            .map(|replay| replay.face_generation)
            .or_else(|| self.scroll.as_ref().map(|replay| replay.face_generation))
            .or_else(|| {
                self.scroll_continuation_faces
                    .as_ref()
                    .map(|(generation, _)| *generation)
            })
    }

    #[inline]
    fn extend_retained_face_ids(&self, ids: &mut std::collections::BTreeSet<FaceId>) {
        if let Some(replay) = &self.cursor_only {
            replay.extend_retained_face_ids(ids);
        } else if let Some(replay) = &self.scroll {
            replay.extend_retained_face_ids(ids);
        } else if let Some((_, rows)) = &self.scroll_continuation_faces {
            crate::incremental_layout::extend_rows_face_ids(rows, ids);
        }
    }

    fn retained_face_ids(&self) -> Vec<FaceId> {
        self.cursor_only
            .as_ref()
            .map(CursorOnlyReplay::retained_face_ids)
            .or_else(|| self.scroll.as_ref().map(ScrollReplay::retained_face_ids))
            .or_else(|| {
                self.scroll_continuation_faces
                    .as_ref()
                    .map(|(_, rows)| crate::incremental_layout::candidate_face_ids(rows))
            })
            .unwrap_or_default()
    }

    fn disable_reuse(&mut self) {
        self.prepared_faces = None;
        self.cursor_only = None;
        self.scroll = None;
        self.is_edit = false;
        self.retry_admitted_faces = None;
        self.scroll_continuation_faces = None;
    }
}

/// The current-frame facts a chrome reuse decision compares against.
fn chrome_reuse_context(
    params: &WindowParams,
    evaluator: &neovm_core::emacs_core::Context,
) -> crate::incremental_layout::ChromeReuseContext {
    crate::incremental_layout::ChromeReuseContext {
        chrome_dirty: evaluator
            .chrome_dirty()
            .is_dirty(neovm_core::window::WindowId(params.window_id as u64)),
        buffer_modified: evaluator
            .buffer_manager()
            .get(neovm_core::buffer::BufferId(params.buffer_id))
            .is_some_and(|buffer| buffer.is_modified()),
    }
}

/// One frame's retained layout state, parked while another frame is laid out.
///
/// Matrices, chrome metrics, and certified semantic scroll rows travel together
/// because they belong to the same accepted frame. Parking all three preserves
/// renewal-only publications when redisplay switches from one frame to another.
#[derive(Default)]
struct RetainedFrameState {
    matrices: rustc_hash::FxHashMap<DisplayWindowId, RetainedWindowMatrix>,
    chrome_metrics: rustc_hash::FxHashMap<DisplayWindowId, WindowChromeMetrics>,
    scroll_preview_coverage: scroll_preview::ScrollPreviewCoverage,
    composition_advances: rustc_hash::FxHashMap<DisplayWindowId, RetainedCompositionAdvances>,
}

/// Shaped automatic-composition widths retained for one window.
///
/// GNU's display reuses a matrix row without re-shaping its compositions; the
/// equivalent here is to keep each resolved span's `composition_gstring_width`
/// while the inputs that decide it are unchanged.  The advances map is rebuilt
/// for the spans of one layout, so it stays bounded by the snapshot's captured
/// span list.
struct RetainedCompositionAdvances {
    buffer_id: neovm_core::buffer::BufferId,
    buffer_modiff: i64,
    overlay_modified_tick: i64,
    face_change_count: u64,
    display_var_change_count: u64,
    body_redisplay: neovm_core::window::BodyRedisplayRevision,
    fontset_generation: u64,
    window_parameters_generation: u64,
    char_width_bits: u32,
    advances: rustc_hash::FxHashMap<(usize, usize), f32>,
}

impl RetainedCompositionAdvances {
    /// Whether these advances still describe the same composition inputs.
    ///
    /// This is deliberately the logical-input subset of
    /// [`neovm_core::window::WindowDisplaySnapshotFreshness`], not the whole
    /// token: the cache mirrors GNU's matrix-row reuse, which legitimately
    /// survives point, vscroll and repaint-only (`redisplay_generation`)
    /// moves, so comparing the full token would defeat the cache on every
    /// cursor move.  The fields below are the ones that decide the shaped
    /// width:
    ///
    /// - text/property changes move `buffer_modiff`;
    /// - overlay changes move `overlay_modified_tick`;
    /// - face definitions and fontsets move their generations;
    /// - a per-window face parameter moves the window parameter generation,
    ///   and `char_width_bits` catches a window font-size change that only
    ///   the layout parameters record;
    /// - `display_var_change_count` moves when a display-affecting variable
    ///   changes without any of the above ticks — `face-remap-add-relative`
    ///   (hence `text-scale-adjust`) writes buffer-local
    ///   `face-remapping-alist`, and GNU re-derives the face (and therefore
    ///   the font and the composition width) on every compute
    ///   (src/xfaces.c:2141, 5188-5223);
    /// - `body_redisplay` moves on `force-window-update`, which must re-derive
    ///   the body even when no buffer/face/font tick moved
    ///   (src/window.c:4492).
    fn is_current_for(
        &self,
        buffer_id: neovm_core::buffer::BufferId,
        buffer_modiff: i64,
        overlay_modified_tick: i64,
        face_change_count: u64,
        display_var_change_count: u64,
        body_redisplay: neovm_core::window::BodyRedisplayRevision,
        fontset_generation: u64,
        window_parameters_generation: u64,
        char_width_bits: u32,
    ) -> bool {
        self.buffer_id == buffer_id
            && self.buffer_modiff == buffer_modiff
            && self.overlay_modified_tick == overlay_modified_tick
            && self.face_change_count == face_change_count
            && self.display_var_change_count == display_var_change_count
            && self.body_redisplay == body_redisplay
            && self.fontset_generation == fontset_generation
            && self.window_parameters_generation == window_parameters_generation
            && self.char_width_bits == char_width_bits
    }
}

/// The advances one layout publishes for its window.
///
/// Every span the current snapshot captured keeps this layout's resolved
/// width, or the validated previous layout's width for spans this pass did not
/// resolve (a replay frame's narrow band must not erase the widths a
/// full-screen pass resolved).  Resolutions for spans the snapshot no longer
/// has are dropped, so the map stays bounded by the snapshot.
fn retained_composition_advances_for_spans(
    spans: impl Iterator<Item = (usize, usize)>,
    resolved: &rustc_hash::FxHashMap<usize, f32>,
    seeded: &rustc_hash::FxHashMap<(usize, usize), f32>,
) -> rustc_hash::FxHashMap<(usize, usize), f32> {
    let mut retained = rustc_hash::FxHashMap::default();
    for (start, end) in spans {
        let width = resolved
            .get(&start)
            .copied()
            .or_else(|| seeded.get(&(start, end)).copied());
        if let Some(width) = width {
            retained.insert((start, end), width);
        }
    }
    retained
}

/// Drop parked advances for windows the accepted frame no longer has.
///
/// The per-window caches are inserted while a layout attempt runs, so an
/// attempt that retried, or a frame switch that removed a window, can leave an
/// entry behind.  The accepted break owns this cleanup.
fn prune_retained_composition_advances(
    caches: &mut rustc_hash::FxHashMap<DisplayWindowId, RetainedCompositionAdvances>,
    accepted: impl Fn(&DisplayWindowId) -> bool,
) {
    caches.retain(|window, _| accepted(window));
}

/// An upper bound on the characters a window can display, as
/// `(first_char, budget)`.
///
/// Deliberately generous. The automatic-composition scan may look at MORE
/// text than a window shows with no consequence beyond time, but looking at
/// less drops a composition, which is a wrong glyph. `window_start` is in
/// 0-based char coordinates (`WindowParams::window_start`), and the budget is
/// four screenfuls of cells so that soft-wrapped lines, a scroll that lands
/// past the start, and narrow glyphs all stay inside it -- while still being
/// proportional to the WINDOW rather than to the buffer, which is the entire
/// point.
fn visible_char_bound(params: &WindowParams) -> (usize, usize) {
    let cols = (params.text_bounds.width / params.char_width.max(1.0)).ceil() as usize;
    let rows = params.source_interpretation_rows();
    let screenful = rows.saturating_mul(cols).max(1);
    let first = params.window_start.max(0) as usize;
    (
        first.saturating_sub(screenful),
        screenful.saturating_mul(4).max(8192),
    )
}

/// The span-start range of automatic compositions the row walk painted but
/// the visible-range resolver did not cover, or `None` when the resolved
/// range is sufficient.
///
/// `starts` are the snapshot's captured automatic-composition span starts in
/// ascending order; the walk paints elements from `walk_start` up to (not
/// including) `walk_end`, so a start in that half-open interval could have
/// been painted and must lie in `resolve`.  Starts before `walk_start` are
/// never painted by this walk (the snapshot's scan is deliberately wider than
/// the window) and must not trigger a retry.  The returned range is the union
/// of the missed starts, used to widen the resolver on the retry
/// (`LeafLayoutAttempt::AutomaticCompositionUnresolved`).  This is the
/// coverage proof behind "never measure the raw text as a fallback for a
/// painted composition" (N-orphan A3).
pub(crate) fn uncovered_automatic_composition_span_range(
    starts: impl Iterator<Item = usize>,
    walk_start: usize,
    walk_end: usize,
    resolve: &std::ops::Range<usize>,
) -> Option<std::ops::Range<usize>> {
    let mut missed: Option<std::ops::Range<usize>> = None;
    for start in starts {
        if start < walk_start
            || start >= walk_end
            || (start >= resolve.start && start < resolve.end)
        {
            continue;
        }
        missed = Some(match missed {
            None => start..start.saturating_add(1),
            Some(range) => range.start.min(start)..range.end.max(start.saturating_add(1)),
        });
    }
    missed
}

fn admit_retained_frame_faces(
    plans: &mut [IncrementalWindowPlan],
    face_attempt: &mut FrameFaceAttempt,
    committed_arena: &FrameFaceArena,
) -> Result<(), FrameFaceReuseError> {
    let current_generation = committed_arena.generation();
    let gather_directly = retained_face_gather::enabled();
    let mut face_ids = std::collections::BTreeSet::new();
    for plan in plans.iter_mut() {
        if let Some(source) = &plan.prepared_faces {
            face_attempt.admit_prepared(
                plan.retained_face_ids()
                    .into_iter()
                    .filter(|id| *id != FaceId::new(0)),
                source,
                committed_arena,
            )?;
            continue;
        }
        let Some(retained_generation) = plan.retained_face_generation() else {
            continue;
        };
        if retained_generation != current_generation {
            return Err(FrameFaceReuseError::StaleGeneration {
                retained: retained_generation,
                current: current_generation,
            });
        }
        if plan.is_edit && plan.prepared_faces.is_none() {
            // Keep the sorted union of edit replay and continuation faces:
            // a retry may only reuse identities admitted in Phase A.
            let mut admitted = std::collections::BTreeSet::new();
            plan.extend_retained_face_ids(&mut admitted);
            if let Some((_, rows)) = &plan.scroll_continuation_faces {
                crate::incremental_layout::extend_rows_face_ids(rows, &mut admitted);
            }
            face_ids.extend(admitted.iter().copied());
            plan.retry_admitted_faces = Some(admitted.into_iter().collect());
        } else if gather_directly {
            plan.extend_retained_face_ids(&mut face_ids);
        } else {
            face_ids.extend(
                plan.retained_face_ids()
                    .into_iter()
                    .filter(|face_id| *face_id != FaceId::new(0)),
            );
        }
    }
    face_attempt.admit_retained(current_generation, face_ids, committed_arena)
}

impl Default for LayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutEngine {
    /// Attach the previous transaction's composed-source cache to this
    /// transaction's resolver when the face table snapshot still matches, and
    /// re-pin the result for the next transaction.
    fn bind_face_composed_sources(
        &mut self,
        face_resolver: &mut super::neovm_bridge::FaceResolver,
    ) {
        if let Some(pin) = self.face_composed_sources.take() {
            pin.apply_to(face_resolver);
        }
        self.face_composed_sources = Some(face_resolver.composed_source_pin());
    }

    /// Bind the resolver's terminal realization, adopting the previous
    /// transaction's realized faces when their stamp and snapshot still hold,
    /// and re-pinning the result for the next transaction.
    fn prepare_face_resolver_tty(
        &mut self,
        face_resolver: &mut super::neovm_bridge::FaceResolver,
        frame_id: neovm_core::window::FrameId,
        evaluator: &mut neovm_core::emacs_core::Context,
    ) {
        let previous = self.face_tty_realization.take();
        face_resolver.prepare_tty_realization(frame_id, evaluator, previous);
        self.face_tty_realization = face_resolver.tty_realization_pin();
    }

    /// Invalidate every retained value whose geometry or identity was derived
    /// from old font-selection inputs. This is deliberately one exhaustive
    /// owner rather than a list of clears spread across redisplay fast paths.
    fn invalidate_for_font_selection_change(&mut self) {
        self.query_cache.clear();
        self.frame_visual_histories = FrameVisualHistories::default();
        self.frame_face_arenas.clear();
        self.face_composed_sources = None;
        self.face_tty_realization = None;
        self.retained_window_matrices.clear();
        self.retained_composition_advances.clear();
        self.prepared_viewports = PreparedViewports::default();
        self.scroll_coverage.cancel();
        self.retained_window_chrome_metrics.clear();
        self.last_frame_display_state = None;
        self.scroll_preview_coverage.clear();
        for retained in self.retained_by_frame.values_mut() {
            retained.scroll_preview_coverage.clear();
        }
        self.reset_frame_attempt_state();
    }

    /// Discard every value owned by one speculative frame-layout attempt.
    ///
    /// A retry must not inherit output or fast-path classifications from the
    /// rejected attempt. Retained matrices and accepted chrome metrics live
    /// outside this boundary and are updated only after sealing.
    fn reset_frame_attempt_state(&mut self) {
        self.frame_output.reset();
        self.pending_tab_bar_pointer = None;
        self.window_snapshots.clear();
        self.query_restart_rows.clear();
        self.mini_preparation_height = None;
        self.cursor_only_window_ids.clear();
        self.prepared_window_ids.clear();
        self.scroll_window_ids.clear();
        self.edit_window_ids.clear();
    }

    fn decide_frame_relayout(
        &mut self,
        coordinator: &mut FrameLayoutCoordinator,
        evaluator: &mut neovm_core::emacs_core::Context,
        frame_id: neovm_core::window::FrameId,
        purpose: LayoutPurpose,
        attempt_mode: FrameAttemptMode,
        window_end_attempts: &mut FrameWindowEndAttempts,
        presentation_id: u64,
        request: FrameRelayoutRequest,
    ) -> RelayoutDecision {
        if evaluator.has_mode_line_display_flow() || evaluator.redisplay_hook_flow_pending() {
            window_end_attempts.reject_all(evaluator);
            evaluator.retire_interaction_presentation(presentation_id);
            self.last_frame_display_state = None;
            self.reset_frame_attempt_state();
            return RelayoutDecision::Abort {
                settle_no_safe: false,
            };
        }
        if evaluator.frame_manager().get(frame_id).is_none() {
            window_end_attempts.reject_all(evaluator);
            evaluator.retire_interaction_presentation(presentation_id);
            self.last_frame_display_state = None;
            self.reset_frame_attempt_state();
            return RelayoutDecision::Abort {
                settle_no_safe: purpose == LayoutPurpose::Redisplay,
            };
        }
        match attempt_mode {
            FrameAttemptMode::Converging => match coordinator.request_retry(request) {
                Ok(()) => RelayoutDecision::Retry,
                Err(error) => {
                    if purpose == LayoutPurpose::Redisplay {
                        tracing::warn!(
                            presentation = presentation_id,
                            ?error,
                            "frame layout retry budget exhausted; publishing callback-inhibited exhaustion snapshot"
                        );
                        RelayoutDecision::EnterExhaustionSnapshot
                    } else {
                        tracing::error!(
                            presentation = presentation_id,
                            ?error,
                            "rejecting frame whose layout failed to converge"
                        );
                        window_end_attempts.reject_all(evaluator);
                        evaluator.retire_interaction_presentation(presentation_id);
                        RelayoutDecision::Abort {
                            settle_no_safe: false,
                        }
                    }
                }
            },
            FrameAttemptMode::ExhaustionSnapshot => {
                tracing::error!(
                    presentation = presentation_id,
                    ?request,
                    "rejecting frame after callback-inhibited exhaustion snapshot could not complete"
                );
                window_end_attempts.reject_all(evaluator);
                evaluator.retire_interaction_presentation(presentation_id);
                self.last_frame_display_state = None;
                self.reset_frame_attempt_state();
                RelayoutDecision::Abort {
                    settle_no_safe: purpose == LayoutPurpose::Redisplay,
                }
            }
        }
    }

    fn latest_output_window_info(&self, window_id: i64) -> Option<WindowInfo> {
        self.frame_output.latest_window_info(window_id)
    }

    fn output_window_automatic_mini_height_px(
        &self,
        params: &WindowParams,
        fallback_row_height: Px,
    ) -> Option<Px> {
        let height = if params.window_system {
            // Preserve legacy GUI sizing; the automatic-tail correction is TTY-only.
            self.frame_output
                .window_content_height_px(params.window_id, fallback_row_height.get())
        } else {
            self.frame_output
                .mini_measurement_height_px(params.window_id, fallback_row_height.get())
        };
        height.map(Px)
    }

    /// Keep inactive echo-area geometry renderer-visible without admitting it
    /// to the live minibuffer's retained display cache.
    ///
    /// GNU temporarily swaps w->contents while displaying the echo area and
    /// restores the live minibuffer afterward. The frame still needs the
    /// temporary geometry, but window-end/posn/vertical-motion must never
    /// mistake those rows for evidence about the minibuffer's real buffer.
    fn mark_inactive_echo_snapshot_geometry_only(
        snapshot: &mut Option<WindowPresentationSnapshot>,
        publication: WindowPositionPublication,
    ) {
        if publication == WindowPositionPublication::InactiveEchoArea
            && let Some(snapshot) = snapshot
        {
            snapshot.retain_as_geometry_only();
        }
    }

    fn finish_frame_output(
        &mut self,
        frame_params: &FrameParams,
        table: crate::frame_face_arena::FrameFaceTableAttempt,
    ) -> Result<
        (
            neomacs_display_protocol::glyph_matrix::FrameDisplayState,
            crate::frame_face_arena::FrameFaceSeal,
        ),
        crate::display_frame_output::FrameOutputFinishError,
    > {
        let (mut state, seal) = self.frame_output.finish(frame_params, table)?;
        // Publish exact resolved font identities for every realized face so
        // the render thread rasterizes the same fonts layout measured with
        // (font realization / render boundary design, Phase 1).
        crate::font::metrics::realize_frame_fonts(&mut state, &mut self.font_metrics);
        Ok((state, seal))
    }

    #[allow(clippy::too_many_arguments)]
    fn render_window_output_decorations(
        &mut self,
        params: &WindowParams,
        frame_params: &FrameParams,
        window_geometry: crate::display_frame_output::WindowFrameGeometry,
        info: &WindowInfo,
        divider_paint: crate::window_layout::WindowDividerPaint,
        effective_default_face: Option<&crate::display_face_policy::EffectiveWindowDefaultFace>,
        face_resolver: &super::neovm_bridge::FaceResolver,
        face_attempt: &FrameFaceAttempt,
    ) {
        let mut decoration_face_ids = face_attempt.clone();
        let font_metrics = &mut self.font_metrics;
        self.frame_output.render_window_decorations(
            WindowFrameDecorationsRenderRequest::new(
                params,
                frame_params,
                window_geometry,
                info,
                divider_paint,
                effective_default_face,
            ),
            // Decorations emit native border/marker items. Their services
            // carry neither a display host nor a Lisp face realizer.
            ChromeRowRenderServices::new(
                font_metrics,
                face_resolver,
                &mut decoration_face_ids,
                crate::string_property_snapshot::StringSourceRootScope::CallbackFree,
            ),
        );
    }

    fn render_latest_window_output_info_effects(
        &mut self,
        previous: &FrameVisualHistory,
        curr_window_infos: &mut rustc_hash::FxHashMap<DisplayWindowId, WindowInfo>,
        transition_mode: WindowContentTransitionMode,
    ) -> NavigationIntentObservation {
        self.frame_output.render_latest_window_info_effects(
            WindowFrameInfoEffectsRenderRequest::new(previous.window_infos(), transition_mode),
            curr_window_infos,
        )
    }

    fn render_frame_output_hints(
        &mut self,
        previous: &FrameVisualHistory,
        curr_window_infos: &rustc_hash::FxHashMap<DisplayWindowId, WindowInfo>,
        frame_navigation: Option<neomacs_display_protocol::TransitionDirection>,
    ) -> NavigationIntentObservation {
        let prev_window_infos = previous.window_infos();
        self.frame_output.render_frame_content_transition_hint(
            FrameContentTransitionHintRenderRequest::new(
                prev_window_infos,
                curr_window_infos,
                frame_navigation,
            ),
        )
    }

    /// Create a new layout engine with cosmic-text font metrics.
    ///
    /// Initializes the `FontMetricsService` eagerly (~500ms font
    /// database scan). Used by GUI mode and tests that need pixel-
    /// accurate font measurement. TTY binaries should use
    /// `new_without_font_metrics()` to skip the scan.
    pub fn new() -> Self {
        Self {
            text_buf: Vec::with_capacity(64 * 1024), // 64KB initial
            window_snapshots: Vec::new(),
            query_restart_rows: Vec::new(),
            query_row_coverage: QueryRowCoverage::Complete,
            query_target_prefix_policy: false,
            mini_preparation_height: None,
            query_body_cache_state: QueryBodyCacheState::Disabled,
            query_cache: Default::default(),
            font_metrics: Some(FontMetricsService::new()),
            display_identity: None,
            font_sizing: FontSizing::native_gui(),
            frame_visual_histories: FrameVisualHistories::default(),
            frame_output: FrameOutputOwner::new(),
            pending_tab_bar_pointer: None,
            last_frame_display_state: None,
            scroll_preview_coverage: scroll_preview::ScrollPreviewCoverage::default(),
            frame_face_arenas: rustc_hash::FxHashMap::default(),
            face_composed_sources: None,
            face_tty_realization: None,
            retained_window_matrices: rustc_hash::FxHashMap::default(),
            retained_composition_advances: rustc_hash::FxHashMap::default(),
            prepared_viewports: PreparedViewports::default(),
            scroll_coverage: scroll_coverage::ScrollCoverage::default(),
            retained_by_frame: rustc_hash::FxHashMap::default(),
            retained_frame: None,
            retained_window_chrome_metrics: rustc_hash::FxHashMap::default(),
            cursor_only_window_ids: rustc_hash::FxHashSet::default(),
            prepared_window_ids: rustc_hash::FxHashSet::default(),
            scroll_window_ids: rustc_hash::FxHashMap::default(),
            edit_window_ids: rustc_hash::FxHashMap::default(),
            pre_fontify_dirty_spans: rustc_hash::FxHashMap::default(),
            pre_fontify_gnu_beg_unchanged: rustc_hash::FxHashMap::default(),
            frame_other_windows_changed: false,
            allow_below_reuse: true,
            layout_stats: LayoutStats::default(),
        }
    }

    /// Create a layout engine without font metrics (TTY mode).
    ///
    /// Skips the ~500ms cosmic-text font database scan. All
    /// measurements fall back to the character-cell grid (1x1 for
    /// TTY, matching GNU Emacs frame.c:1184-1185). GUI binaries
    /// should use `new()` instead.
    pub fn new_without_font_metrics() -> Self {
        Self {
            text_buf: Vec::with_capacity(64 * 1024),
            window_snapshots: Vec::new(),
            query_restart_rows: Vec::new(),
            query_row_coverage: QueryRowCoverage::Complete,
            query_target_prefix_policy: false,
            mini_preparation_height: None,
            query_body_cache_state: QueryBodyCacheState::Disabled,
            query_cache: Default::default(),
            font_metrics: None,
            display_identity: None,
            font_sizing: FontSizing::native_gui(),
            frame_visual_histories: FrameVisualHistories::default(),
            frame_output: FrameOutputOwner::new(),
            pending_tab_bar_pointer: None,
            last_frame_display_state: None,
            scroll_preview_coverage: scroll_preview::ScrollPreviewCoverage::default(),
            frame_face_arenas: rustc_hash::FxHashMap::default(),
            face_composed_sources: None,
            face_tty_realization: None,
            retained_window_matrices: rustc_hash::FxHashMap::default(),
            retained_composition_advances: rustc_hash::FxHashMap::default(),
            prepared_viewports: PreparedViewports::default(),
            scroll_coverage: scroll_coverage::ScrollCoverage::default(),
            retained_by_frame: rustc_hash::FxHashMap::default(),
            retained_frame: None,
            retained_window_chrome_metrics: rustc_hash::FxHashMap::default(),
            cursor_only_window_ids: rustc_hash::FxHashSet::default(),
            prepared_window_ids: rustc_hash::FxHashSet::default(),
            scroll_window_ids: rustc_hash::FxHashMap::default(),
            edit_window_ids: rustc_hash::FxHashMap::default(),
            pre_fontify_dirty_spans: rustc_hash::FxHashMap::default(),
            pre_fontify_gnu_beg_unchanged: rustc_hash::FxHashMap::default(),
            frame_other_windows_changed: false,
            allow_below_reuse: true,
            layout_stats: LayoutStats::default(),
        }
    }

    /// Disable cosmic-text font measurement (TTY mode).
    ///
    /// Drops the `FontMetricsService` so all measurements fall back
    /// to the character-cell grid. Called once at TTY startup from
    /// the binary that constructs the layout engine.
    pub fn disable_cosmic_metrics(&mut self) {
        self.query_cache.clear();
        self.font_metrics = None;
    }

    /// Enable cosmic-text font measurement for GUI rendering.
    ///
    /// Constructs the `FontMetricsService` if it hasn't already been
    /// constructed. Called once at GUI startup from the binary that
    /// sets up the layout engine. TTY mode skips this call and
    /// leaves `font_metrics` as `None`, so all measurements fall
    /// back to the character-cell grid (GNU Emacs frame.c:1184-1185:
    /// TTY frames have column_width=1 and line_height=1).
    ///
    /// This replaces the previous `use_cosmic_metrics: bool` runtime
    /// flag. The decision of which measurement strategy to use is
    /// now made once at startup by which binary constructs the
    /// engine, matching GNU's per-frame redisplay_interface vtable
    /// dispatch.
    pub fn set_display_identity(
        &mut self,
        identity: neomacs_display_protocol::GraphicalDisplayIdentity,
    ) {
        self.query_cache.clear();
        if let Some(service) = self.font_metrics.as_mut() {
            service.set_display_identity(identity.clone());
        }
        self.display_identity = Some(identity);
    }

    pub fn enable_cosmic_metrics(&mut self) {
        self.query_cache.clear();
        if self.font_metrics.is_none() {
            let mut service = FontMetricsService::new();
            if let Some(identity) = self.display_identity.as_ref() {
                service.set_display_identity(identity.clone());
            }
            self.font_metrics = Some(service);
        }
    }

    pub fn set_font_sizing(&mut self, font_sizing: FontSizing) {
        self.query_cache.clear();
        self.font_sizing = font_sizing;
    }

    /// Snapshot only the accepted geometry a stack-local query must inherit.
    pub fn window_layout_query_seed(&self) -> WindowLayoutQuerySeed {
        WindowLayoutQuerySeed {
            retained_window_chrome_metrics: self.retained_window_chrome_metrics.clone(),
        }
    }

    /// Instrumentation from the most recent `layout_frame_rust` pass.
    ///
    /// THE gate metric for the incremental-layout phases: a phase ships only
    /// when its bench cases prove the win on relaid-row-count, not wall-time
    /// alone (spec §7). Phase 0a always reports a full rebuild (every body row
    /// relaid, zero reused, all windows `Full`).
    pub fn last_layout_stats(&self) -> &LayoutStats {
        &self.layout_stats
    }

    /// Test-only convenience for frame fixtures.
    #[cfg(test)]
    pub fn layout_frame_rust(
        &mut self,
        evaluator: &mut neovm_core::emacs_core::Context,
        frame_id: neovm_core::window::FrameId,
    ) {
        match self.redisplay_frame_attempt(evaluator, frame_id) {
            FrameLayoutAttempt::Prepared(state) => self.last_frame_display_state = Some(state),
            FrameLayoutAttempt::Aborted => {}
        }
    }

    /// Run one renderer-facing redisplay attempt.
    pub fn redisplay_frame_attempt(
        &mut self,
        evaluator: &mut neovm_core::emacs_core::Context,
        frame_id: neovm_core::window::FrameId,
    ) -> FrameLayoutAttempt {
        self.frame_layout_attempt(evaluator, frame_id, LayoutPurpose::Redisplay)
    }

    /// The face id redisplay sealed for one buffer position, and the arena
    /// epoch that keys that face's fallback order.
    ///
    /// GNU `font_at` reads the frame face cache (`src/font.c` `font_at` and
    /// `FACE_FOR_CHAR` at a360712c9d). This lookup uses that same arena. It
    /// does not poll the font catalog and does not install a new fontset
    /// context: either of those can clear or re-key the order font-at is
    /// trying to see. A face this arena never sealed returns `None`, so the
    /// caller keeps the implicit fontset instead of inventing an id.
    pub fn realized_fontset_for_lisp_pos(
        &mut self,
        evaluator: &mut neovm_core::emacs_core::Context,
        frame_id: neovm_core::window::FrameId,
        window_id: neovm_core::window::WindowId,
        lisp_pos: i64,
    ) -> Option<(u32, u64)> {
        let arena_epoch = self.frame_face_arenas.get(&frame_id)?.face_epoch();
        let (bootstrap, window_system) = {
            let frame = evaluator.frame_manager().get(frame_id)?;
            if frame.find_window(window_id).is_none() {
                return None;
            }
            let bootstrap = crate::neovm_bridge::frame_params_from_neovm(
                frame,
                evaluator.face_table(),
                evaluator.variables(),
            );
            let window_system = frame
                .effective_window_system()
                .and_then(|value| value.as_bare_symbol_name().map(str::to_owned));
            (bootstrap, window_system)
        };
        let font_sizing = self.font_sizing;
        let mut resolver = crate::neovm_bridge::FaceResolver::new_with_font_sizing(
            evaluator.face_table(),
            0x00FFFFFF,
            bootstrap.background,
            bootstrap.font_pixel_size,
            window_system,
            font_sizing,
        );
        self.bind_face_composed_sources(&mut resolver);
        self.prepare_face_resolver_tty(&mut resolver, frame_id, evaluator);
        resolver.set_current_window_parameters(
            evaluator.frame_manager().window_parameters_pairs(window_id),
        );
        resolver.set_current_window_id(Some(window_id.0));
        let buffer_id = evaluator
            .frame_manager()
            .get(frame_id)?
            .find_window(window_id)?
            .buffer_id()?;
        let mut resolved = {
            let buffer = evaluator.buffer_manager().get(buffer_id)?;
            let beg = buffer.point_min_lisp_char_pos().as_i64();
            let end = buffer.point_max_lisp_char_pos().as_i64();
            if !(beg <= lisp_pos && lisp_pos < end) {
                return None;
            }
            let origin = crate::display_origin::DisplayOrigin::BufferText {
                charpos: neovm_core::buffer::LispCharPos1::new(lisp_pos).to_char_pos(),
            };
            let mut next_check = 0usize;
            resolver.default_base_face_for_origin(Some(buffer), &origin, &mut next_check)
        };
        resolver.realize_for_display(&mut resolved, evaluator);
        let face_id = self
            .frame_face_arenas
            .get(&frame_id)?
            .begin_attempt()
            .lookup_face_id_for_resolved(&resolved)?;
        Some((face_id.get(), arena_epoch.get()))
    }

    /// Run one renderer-facing snapshot attempt.
    pub fn snapshot_frame_attempt(
        &mut self,
        evaluator: &mut neovm_core::emacs_core::Context,
        frame_id: neovm_core::window::FrameId,
    ) -> FrameLayoutAttempt {
        self.frame_layout_attempt(evaluator, frame_id, LayoutPurpose::Snapshot)
    }

    fn frame_layout_attempt(
        &mut self,
        evaluator: &mut neovm_core::emacs_core::Context,
        frame_id: neovm_core::window::FrameId,
        purpose: LayoutPurpose,
    ) -> FrameLayoutAttempt {
        debug_assert!(purpose.query_window().is_none());
        self.retire_removed_frame_owners(evaluator);
        if evaluator.redisplay_hook_flow_pending() {
            return FrameLayoutAttempt::Aborted;
        }
        let outcome = self.layout_frame_rust_with_face_services(
            evaluator,
            frame_id,
            purpose,
            None,
            #[cfg(test)]
            None,
            &rows_face_services::NoopFaceServiceFactory,
            None,
        );
        self.retire_removed_frame_owners(evaluator);
        if evaluator.has_mode_line_display_flow() || evaluator.redisplay_hook_flow_pending() {
            self.last_frame_display_state = None;
            return FrameLayoutAttempt::Aborted;
        }
        self.report_image_failures(evaluator);
        match (outcome, self.last_frame_display_state.take()) {
            (FrameLayoutInnerOutcome::Prepared(_), Some(state)) => {
                FrameLayoutAttempt::Prepared(state)
            }
            (FrameLayoutInnerOutcome::Aborted { settle_no_safe }, _) => {
                if settle_no_safe && purpose == LayoutPurpose::Redisplay {
                    evaluator.note_no_safe_handled_frame(frame_id);
                }
                FrameLayoutAttempt::Aborted
            }
            (FrameLayoutInnerOutcome::Prepared(_), None) => FrameLayoutAttempt::Aborted,
        }
    }

    /// Report every image failure this pass observed.
    ///
    /// One point, after the pass that performed the lookups, because that is
    /// where GNU reports too: `lookup_image` runs from the display iterator and
    /// its `image_error` lands in the same redisplay. Putting it here rather
    /// than at each of this engine's exits is what makes it unmissable — a new
    /// early return cannot skip it.
    fn report_image_failures(&self, evaluator: &mut neovm_core::emacs_core::Context) {
        evaluator.log_pending_image_diagnostics();
    }

    fn prepare_frame_font_inputs(
        &mut self,
        evaluator: &mut neovm_core::emacs_core::Context,
        frame_id: neovm_core::window::FrameId,
    ) -> Option<FrameFontInputs> {
        let (bootstrap, window_system, device_scale) = {
            let Some(frame) = evaluator.frame_manager().get(frame_id) else {
                tracing::error!("layout_frame_rust: frame {:?} not found", frame_id);
                return None;
            };
            let bootstrap = super::neovm_bridge::frame_params_from_neovm(
                frame,
                evaluator.face_table(),
                evaluator.variables(),
            );
            let ws = frame
                .effective_window_system()
                .and_then(|v| v.as_bare_symbol_name().map(|s| s.to_string()));
            (
                bootstrap,
                ws,
                neomacs_display_protocol::geometry::DeviceScale::new(
                    frame.device_scale_factor as f32,
                )
                .unwrap_or(neomacs_display_protocol::geometry::DeviceScale::ONE),
            )
        };
        let font_selection_changed = if let Some(font_metrics) = self.font_metrics.as_mut() {
            font_metrics.set_device_scale(device_scale);
            let font_catalog_changed = font_metrics.synchronize_font_catalog().changed();
            let use_primary_font = crate::neovm_bridge::visible_value(
                evaluator,
                crate::fixed_symbols::USE_DEFAULT_FONT_FOR_SYMBOLS.id(),
            )
            .is_none_or(|value| !value.is_nil());
            let char_script_table = crate::neovm_bridge::visible_value(
                evaluator,
                crate::fixed_symbols::CHAR_SCRIPT_TABLE.id(),
            );
            let symbol_policy_changed = font_metrics
                .synchronize_symbol_font_policy(use_primary_font, char_script_table)
                .changed();
            font_catalog_changed || symbol_policy_changed
        } else {
            false
        };
        if font_selection_changed {
            self.invalidate_for_font_selection_change();
        }
        Some(FrameFontInputs {
            bootstrap,
            window_system,
        })
    }

    fn realize_frame_fonts(
        &mut self,
        evaluator: &mut neovm_core::emacs_core::Context,
        frame_id: neovm_core::window::FrameId,
        inputs: &FrameFontInputs,
    ) -> PreparedFrameFonts {
        // One read for this realization. Position queries and the layout loop
        // both enter here, and a later bump must not re-key this attempt.
        let face_epoch = crate::frame_face_arena::FaceEpoch::new(evaluator.face_change_count());
        if let Some(service) = self.font_metrics.as_mut() {
            service.set_realized_fontset_context(frame_id.0, face_epoch.get());
        }
        // Realize the default face before collecting window params so frame and
        // window geometry use the same default metrics GNU Emacs redisplay does.
        let mut face_resolver = super::neovm_bridge::FaceResolver::new_with_font_sizing(
            evaluator.face_table(),
            0x00FFFFFF,
            inputs.bootstrap.background,
            inputs.bootstrap.font_pixel_size,
            inputs.window_system.clone(),
            self.font_sizing,
        );
        self.bind_face_composed_sources(&mut face_resolver);
        self.prepare_face_resolver_tty(&mut face_resolver, frame_id, evaluator);
        let default_resolved = face_resolver.default_face();
        let frame_font_domain = FrameFontDomain::for_frame(
            inputs.window_system.is_some(),
            inputs.bootstrap.font_pixel_size,
        );
        let default_geometry = if inputs.window_system.is_some() {
            self.font_metrics.as_mut().map(|svc| {
                svc.frame_cell_geometry(
                    &default_resolved.font_family,
                    default_resolved.font_weight,
                    default_resolved.italic,
                    default_resolved.font_size,
                    frame_font_domain,
                )
            })
        } else {
            Some(FrameCellGeometry::TerminalCell)
        };
        let default_metrics = match default_geometry {
            Some(FrameCellGeometry::Graphic(geometry)) => Some(geometry.metrics),
            Some(FrameCellGeometry::TerminalCell) | None => None,
        };

        match default_geometry {
            Some(FrameCellGeometry::Graphic(geometry)) => {
                face_resolver.retain_opened_default_font_size(geometry.font_size);
                if let Some(frame) = evaluator.frame_manager_mut().get_mut(frame_id) {
                    frame.char_width = geometry.metrics.char_width;
                    frame.char_height = geometry.metrics.line_height;
                    frame.font_pixel_size = geometry.font_size.get();
                    // The same metrics object already decides the ascent every
                    // image row is laid out against (`default_metrics.ascent`
                    // below); publishing it on the frame keeps a non-rendering
                    // measurement (`window-text-pixel-size`) from having to
                    // guess the baseline split of the cell it reports.
                    frame.font_ascent = geometry.metrics.ascent;
                }
            }
            Some(FrameCellGeometry::TerminalCell) => {
                // GNU Emacs terminal frames are an explicit 1x1-cell domain
                // (frame.c:1182-1183), not a small graphic font. Preserve an
                // already configured logical-cell scale (used by alternate
                // terminal hosts and deterministic layout fixtures), while
                // guaranteeing that an uninitialized axis becomes one cell.
                if let Some(frame) = evaluator.frame_manager_mut().get_mut(frame_id) {
                    if frame.char_width < 1.0 {
                        frame.char_width = 1.0;
                    }
                    if frame.char_height < 1.0 {
                        frame.char_height = 1.0;
                    }
                    // No font object on a terminal frame: the whole cell sits
                    // above the baseline, which is the same choice
                    // `neovm_bridge::window_params_from_neovm` makes for
                    // terminal `WindowParams`.
                    frame.font_ascent = frame.char_height;
                }
            }
            // A graphic layout engine without font services retains the last
            // coherent frame geometry instead of publishing a partial update.
            None => {}
        }

        if matches!(default_geometry, Some(FrameCellGeometry::Graphic(_)))
            && let Some(service) = self.font_metrics.as_mut()
        {
            use neovm_core::face::BasicFace;
            use strum::IntoEnumIterator;

            let default_face = face_resolver.default_face();
            if service
                .resolved_font_for_face(
                    &default_face.font_family,
                    default_face.font_weight,
                    default_face.italic,
                    default_face.font_size,
                )
                .is_some()
            {
                // GNU xfaces.c:5950 realizes every basic GUI face after the
                // default font opens, including hidden chrome, before desired
                // matrices use the minimum. Bypass buffer face remapping.
                let table = evaluator.face_table();
                let default =
                    table.resolve_fixed_symbol(&neovm_core::emacs_core::intern::WellKnown::Default);
                for basic in BasicFace::iter().filter(|basic| !matches!(basic, BasicFace::Default))
                {
                    // Merge relative point heights before pixel conversion, as
                    // GNU merge_face_heights does; opened default pixels may
                    // have already rounded to a different size.
                    let attributes = default.merge(&table.resolve(basic.name()));
                    let face = face_resolver.realize_face(&attributes);
                    let _ = service.resolved_font_for_face(
                        &face.font_family,
                        face.font_weight,
                        face.italic,
                        face.font_size,
                    );
                }
            }
        }

        PreparedFrameFonts {
            resolver: face_resolver,
            default_metrics,
            face_epoch,
        }
    }

    fn layout_frame_rust_for_purpose_inner(
        &mut self,
        evaluator: &mut neovm_core::emacs_core::Context,
        frame_id: neovm_core::window::FrameId,
        purpose: LayoutPurpose,
        rows_lifetime: Option<&mut rows_lifetime::Producer>,
        #[cfg(test)] rows_fontification_sink: Option<
            &mut dyn FnMut(neovm_core::emacs_core::xdisp::RedisplayFontificationWitness),
        >,
    ) -> Option<neovm_core::window::WindowLayoutQuery> {
        self.layout_frame_rust_with_face_services(
            evaluator,
            frame_id,
            purpose,
            rows_lifetime,
            #[cfg(test)]
            rows_fontification_sink,
            &rows_face_services::NoopFaceServiceFactory,
            None,
        )
        .into_query()
    }

    fn layout_frame_rust_with_face_services<F: FaceServiceFactory>(
        &mut self,
        evaluator: &mut neovm_core::emacs_core::Context,
        frame_id: neovm_core::window::FrameId,
        purpose: LayoutPurpose,
        mut rows_lifetime: Option<&mut rows_lifetime::Producer>,
        #[cfg(test)] mut rows_fontification_sink: Option<
            &mut dyn FnMut(neovm_core::emacs_core::xdisp::RedisplayFontificationWitness),
        >,
        face_services: &F,
        position_chrome: Option<&position_chrome::MeasuredPositionChrome>,
    ) -> FrameLayoutInnerOutcome {
        // Anything recorded since the last pass that no pass has reported yet.
        // The drain below covers what this pass observes; this one covers
        // whatever a path outside layout observed -- a `:channel0` image spec
        // resolved from Lisp, say -- so a failure cannot sit unlogged just
        // because it happened between frames.
        self.report_image_failures(evaluator);
        let query_window = purpose.query_window();
        // Layout Lisp can enter a nested command reader. Capture completion
        // before gathering pixels, never from commands that finish mid-layout.
        let input_checkpoint = evaluator.input_progress.checkpoint();
        self.load_retained_frame(frame_id);
        let arrow_buffer = purpose
            .query_window()
            .and_then(|window_id| {
                evaluator
                    .frame_manager()
                    .get(frame_id)?
                    .find_window(window_id)
            })
            .and_then(|window| window.buffer_id());
        if !scroll_coverage::inactive_overlay_arrows(evaluator, arrow_buffer) {
            self.scroll_coverage.cancel();
            self.prepared_viewports = PreparedViewports::default();
        } else if query_window.is_none() {
            let _ = self.scroll_coverage.drain(&mut self.prepared_viewports);
        }
        // Incremental-layout instrumentation (Phase 0a): start each frame from
        // a clean slate; populated as the accepted frame is committed below.
        if query_window.is_none() {
            self.layout_stats = LayoutStats::default();
        }
        // The font service can exist on the engine even while laying out a
        // terminal frame in tests. Match GNU's redisplay split: window-system
        // frames use realized font pixels, terminal frames stay on cell
        // metrics.

        // Reset the per-redisplay mode-line eval counter. Each chrome row is
        // laid out (and thus its `*-format` evaluated) exactly once per window
        // per frame; the single-eval invariant test asserts this stays at 1.
        if query_window.is_none() {
            crate::display_status_line::reset_mode_line_eval_count();
            crate::display_status_line::reset_chrome_generation_record();
        }

        // GNU `prepare_menu_bars` runs menu, tab, then tool-bar Lisp before
        // `redisplay_windows` starts filling display lines. Evaluate all three
        // once at the logical redisplay boundary, before taking any
        // face/window/topology snapshot. Physical chrome/minibuffer retries
        // reuse the typed result below. Keep its Lisp values rooted until the
        // accepted presentation has consumed them.
        let gui_chrome_gc_roots = ScratchGcRootScope::new();
        let prepared_gui_chrome = query_window
            .is_none()
            .then(|| PreparedGuiChromeSemantics::collect(evaluator, frame_id, &gui_chrome_gc_roots))
            .flatten();

        // Queries normalize faces inside their dependency observation before
        // entering the row producer; redisplay normalizes after chrome Lisp.
        if query_window.is_none() {
            evaluator.sync_runtime_faces_for_frame(frame_id);
        }

        let Some(font_inputs) = self.prepare_frame_font_inputs(evaluator, frame_id) else {
            return FrameLayoutInnerOutcome::Aborted {
                settle_no_safe: purpose == LayoutPurpose::Redisplay,
            };
        };
        let window_system = font_inputs.window_system.clone();
        let presentation_id = evaluator.begin_interaction_presentation();
        let previous_visual_history = self.frame_visual_histories.snapshot(frame_id);
        let pending_frame_navigation = query_window
            .is_none()
            .then(|| {
                evaluator
                    .frame_manager()
                    .pending_frame_navigation_intent(frame_id)
            })
            .flatten();

        let PreparedFrameFonts {
            resolver: mut face_resolver,
            default_metrics: mut default_metrics,
            face_epoch,
        } = self.realize_frame_fonts(evaluator, frame_id, &font_inputs);
        let mut resolver_face_change_count = face_epoch.get();
        enum FaceResolverRefresh {
            Current,
            Required,
        }
        let mut resolver_refresh = FaceResolverRefresh::Current;

        // --- Frame layout convergence loop (GNU xdisp.c redisplay retries) ---
        //
        // After laying out all windows we check whether the minibuffer
        // used more (or fewer) display rows than its allocated height.
        // If so we call grow_mini_window / shrink_mini_window and
        // A typed coordinator gives frame chrome, leaf chrome, and minibuffer
        // allocation one bounded retry policy. Every retry discards the whole
        // speculative output. A source-only retry is the exception at
        // exhaustion: that completed attempt reaches sealing below.
        let mut layout_coordinator = FrameLayoutCoordinator::new(MAX_FRAME_LAYOUT_RETRIES);
        let mut attempt_mode = FrameAttemptMode::Converging;
        // The Lisp objects of the `when` sites a walk records and of the
        // answers carried to a replay stay rooted for the whole logical frame
        // attempt: window chrome and the replay's preparation run Lisp (and
        // may collect) between a walk and the evaluation or reuse of them.
        // Every leaf attempt that records runs inside this function.
        let _display_when_gc_roots = ScratchGcRootScope::new();
        let mut lisp_ledger = RedisplayLispLedger {
            rows_fontification: rows_fontification::RowsFontificationDemand::for_purpose(
                evaluator, frame_id, purpose,
            ),
            ..RedisplayLispLedger::default()
        };
        let mut window_chrome_metrics = self.retained_window_chrome_metrics.clone();
        if let Some(measured) = position_chrome {
            window_chrome_metrics.insert(measured.display_window(), measured.metrics());
        }
        // GNU retires every realized face when `face_change` is set
        // (`free_realized_faces`, src/xfaces.c:4889-4954, driven by
        // `face_change_count`, src/xfaces.c:3098-3105). Do the same at the
        // frame boundary: the arena keeps its owner lineage, but its faces,
        // identity memo and generation are replaced, so retained rows minted
        // under the old definitions fail admission with a typed
        // `StaleGeneration` instead of colliding on a recycled basic id.
        let mut frame_face_epoch = FaceEpoch::new(evaluator.face_change_count());
        if let Some(service) = self.font_metrics.as_mut() {
            // Bind the arena epoch once. Lisp inside the retry loop can bump
            // the live counter; re-reading it would key this arena's face ids
            // under the next epoch.
            service.set_realized_fontset_context(frame_id.0, frame_face_epoch.get());
        }
        let mut committed_face_arena = match self.frame_face_arenas.get(&frame_id) {
            Some(arena) if arena.face_epoch() == frame_face_epoch => arena.clone(),
            Some(arena) => {
                let invalidated = arena.invalidate_for_face_definition_change(frame_face_epoch);
                // GNU's `face_change` retires everything derived from the old
                // definitions, not only the realized face array: retained
                // matrices and their chrome/scroll side tables carry face ids
                // too (xfaces.c:3098-3105 -> redisplay_internal's full
                // redisplay). Clearing them here means no stale row can reach
                // a path that forgot an admission check; the arena lineage is
                // re-inserted with a fresh generation so a retained handle
                // that still exists elsewhere fails with `StaleGeneration`.
                self.invalidate_for_font_selection_change();
                self.frame_face_arenas.insert(frame_id, invalidated.clone());
                invalidated
            }
            None => FrameFaceArena::default(),
        };
        let mut frame_window_end_attempts = FrameWindowEndAttempts::default();
        let layout_walk_purpose = match purpose {
            LayoutPurpose::MiniPreparation { .. } => WindowLayoutWalkPurpose::MiniPreparation,
            LayoutPurpose::SynchronousQuery { scope, .. } => {
                WindowLayoutWalkPurpose::SynchronousQuery(scope)
            }
            LayoutPurpose::Snapshot if evaluator.gnu_redisplay_hooks_policy_enabled() => {
                WindowLayoutWalkPurpose::Snapshot
            }
            LayoutPurpose::Redisplay | LayoutPurpose::Snapshot => {
                WindowLayoutWalkPurpose::Redisplay
            }
        };
        let mut minibuffer_measurement_needs_begv = query_window.is_none()
            && layout_walk_purpose != WindowLayoutWalkPurpose::Snapshot
            && !(evaluator.gnu_redisplay_hooks_policy_enabled()
                && evaluator.gnu_redisplay_transaction_active());

        // Position normalization measured once before body observation. Every
        // physical retry consumes the same numeric partition without Lisp.
        let mut chrome_remeasured_next_round: std::collections::HashSet<DisplayWindowId> =
            std::collections::HashSet::new();
        macro_rules! request_frame_relayout {
            ($label:lifetime, $request:expr) => {
                match self.decide_frame_relayout(
                    &mut layout_coordinator,
                    evaluator,
                    frame_id,
                    purpose,
                    attempt_mode,
                    &mut frame_window_end_attempts,
                    presentation_id,
                    $request,
                ) {
                    RelayoutDecision::Retry => {
                        continue $label;
                    }
                    RelayoutDecision::EnterExhaustionSnapshot => {
                        attempt_mode = FrameAttemptMode::ExhaustionSnapshot;
                        continue $label;
                    }
                    RelayoutDecision::Abort { settle_no_safe } => {
                        return FrameLayoutInnerOutcome::Aborted { settle_no_safe };
                    }
                }
            };
        }
        let (
            frame_params,
            curr_window_infos,
            retained_keys,
            accepted_window_chrome_metrics,
            mut frame_display_state,
            face_seal,
            accepted_window_navigation_intents,
            accepted_frame_navigation_intent,
        ) = 'frame_layout: loop {
            lisp_ledger.completed_body_inputs.clear();
            lisp_ledger.source_only_retry = None;
            let chrome_remeasured = std::mem::take(&mut chrome_remeasured_next_round);
            // Re-entering this loop means the preceding physical frame walk
            // was rejected. Restore every leaf publication before collecting
            // the next attempt's canonical previous-viewport evidence.
            frame_window_end_attempts.reject_all(evaluator);
            if attempt_mode == FrameAttemptMode::ExhaustionSnapshot {
                // Converging callbacks (e.g. a face-mutating header-line) may
                // have updated the live face table and bumped face_change_count.
                // Refresh face resolver, metrics, epoch, and arena once before
                // the callback-inhibited pass so the published snapshot seals
                // against the latest live face definitions.
                evaluator.sync_runtime_faces_for_frame(frame_id);
                let live_face_epoch = FaceEpoch::new(evaluator.face_change_count());
                if live_face_epoch != frame_face_epoch
                    || resolver_face_change_count != live_face_epoch.get()
                    || matches!(resolver_refresh, FaceResolverRefresh::Required)
                {
                    let Some(fresh_font_inputs) =
                        self.prepare_frame_font_inputs(evaluator, frame_id)
                    else {
                        evaluator.retire_interaction_presentation(presentation_id);
                        self.last_frame_display_state = None;
                        self.reset_frame_attempt_state();
                        return FrameLayoutInnerOutcome::Aborted {
                            settle_no_safe: purpose == LayoutPurpose::Redisplay,
                        };
                    };
                    let PreparedFrameFonts {
                        resolver,
                        default_metrics: fresh_metrics,
                        face_epoch,
                    } = self.realize_frame_fonts(evaluator, frame_id, &fresh_font_inputs);
                    face_resolver = resolver;
                    default_metrics = fresh_metrics;
                    resolver_face_change_count = face_epoch.get();
                    resolver_refresh = FaceResolverRefresh::Current;
                    frame_face_epoch = FaceEpoch::new(evaluator.face_change_count());
                    if let Some(service) = self.font_metrics.as_mut() {
                        service.set_realized_fontset_context(frame_id.0, frame_face_epoch.get());
                    }
                    committed_face_arena = match self.frame_face_arenas.get(&frame_id) {
                        Some(arena) if arena.face_epoch() == frame_face_epoch => arena.clone(),
                        Some(arena) => {
                            let invalidated =
                                arena.invalidate_for_face_definition_change(frame_face_epoch);
                            self.invalidate_for_font_selection_change();
                            self.frame_face_arenas.insert(frame_id, invalidated.clone());
                            invalidated
                        }
                        None => FrameFaceArena::default(),
                    };
                }
            } else if face_resolver.has_tty_realization() {
                // A color callback may mutate face definitions while a prior
                // leaf is being rendered. Recollect the immutable resolver at
                // the transaction restart, before any cached row is admitted.
                let before_sync = evaluator.face_change_count();
                let refreshed = evaluator.sync_runtime_faces_for_frame(frame_id);
                let after_sync = evaluator.face_change_count();
                if matches!(resolver_refresh, FaceResolverRefresh::Required)
                    || refreshed
                    || resolver_face_change_count != after_sync
                {
                    let Some(frame) = evaluator.frame_manager().get(frame_id) else {
                        evaluator.retire_interaction_presentation(presentation_id);
                        return FrameLayoutInnerOutcome::Aborted {
                            settle_no_safe: purpose == LayoutPurpose::Redisplay,
                        };
                    };
                    let bootstrap = super::neovm_bridge::frame_params_from_neovm(
                        frame,
                        evaluator.face_table(),
                        evaluator.variables(),
                    );
                    face_resolver = super::neovm_bridge::FaceResolver::new_with_font_sizing(
                        evaluator.face_table(),
                        0x00FFFFFF,
                        bootstrap.background,
                        frame.font_pixel_size,
                        None,
                        self.font_sizing,
                    );
                    self.bind_face_composed_sources(&mut face_resolver);
                    self.prepare_face_resolver_tty(&mut face_resolver, frame_id, evaluator);
                    resolver_face_change_count = after_sync;
                    resolver_refresh = FaceResolverRefresh::Current;
                }
                if before_sync != after_sync {
                    let request = FrameRelayoutRequest::FaceRealizationInputsChanged {
                        before: before_sync,
                        after: after_sync,
                    };
                    request_frame_relayout!('frame_layout, request);
                }
            }
            // Layout retries are speculative. GNU logs invalid face references
            // observed by the accepted redisplay, not once per discarded
            // geometry attempt.
            face_resolver.clear_diagnostics();
            // P3.5 E2: the fingerprints committed with a window's chrome must
            // be the ones its ACCEPTED rows were rendered from, so the record
            // is per attempt (the generation record above is per frame). A
            // nested synchronous query renders no chrome and keeps it.
            if query_window.is_none() {
                crate::incremental_layout::chrome_memo::reset_fingerprint_record();
            }
            if query_window.is_none() {
                let pending = evaluator
                    .frame_manager()
                    .get(frame_id)
                    .is_some_and(|frame| {
                        // Only a graphical connection adjusts the pixel-division
                        // allocation; identity-less frames keep it dormant.
                        frame
                            .layout_inputs()
                            .smallest_opened_font_height
                            .is_some_and(|min_height| {
                                frame
                                    .live_window_ids_with_minibuffer()
                                    .into_iter()
                                    .any(|id| {
                                        frame
                                            .find_window(id)
                                            .and_then(|window| window.display())
                                            .is_some_and(|display| {
                                                display
                                                    .glyph_matrix_row_scale
                                                    .needs_adjustment(min_height)
                                            })
                                    })
                            })
                    });
                if pending {
                    neovm_core::emacs_core::window_cmds::adjust_frame_glyph_matrices(
                        evaluator, frame_id,
                    );
                }
            }
            let attempt_topology_generation =
                evaluator.frame_manager().window_topology_generation();
            let Some(attempt_smallest_font_height) = evaluator
                .frame_manager()
                .get(frame_id)
                .map(|frame| frame.layout_inputs().smallest_opened_font_height)
            else {
                frame_window_end_attempts.reject_all(evaluator);
                evaluator.retire_interaction_presentation(presentation_id);
                self.last_frame_display_state = None;
                self.reset_frame_attempt_state();
                return FrameLayoutInnerOutcome::Aborted {
                    settle_no_safe: purpose == LayoutPurpose::Redisplay,
                };
            };

            let window_paths: rustc_hash::FxHashMap<_, _> = evaluator
                .frame_manager()
                .leaf_window_paths(frame_id)
                .map(|paths| paths.into_iter().collect())
                .unwrap_or_default();

            // A synchronous GNU-style display query is a one-leaf walk. Do
            // not build every sibling's expensive Lisp/layout projection only
            // to discard it below. Normal redisplay snapshots the whole frame
            // once, then uses these paths for O(depth) leaf recollection after
            // Lisp boundaries.
            let (
                frame_params,
                mut window_params_list,
                query_main_area_bottom,
                query_divider_corner,
            ) = if let Some(target) = query_window {
                let target = neovm_core::window::WindowId(target.0);
                let Some(path) = window_paths.get(&target) else {
                    evaluator.retire_interaction_presentation(presentation_id);
                    self.reset_frame_attempt_state();
                    return FrameLayoutInnerOutcome::Aborted {
                        settle_no_safe: false,
                    };
                };
                let Some(inputs) = collect_live_window_layout_inputs(
                    evaluator,
                    frame_id,
                    target,
                    path,
                    default_metrics.map(|metrics| metrics.ascent),
                    self.font_sizing,
                    &window_chrome_metrics,
                    layout_walk_purpose,
                ) else {
                    evaluator.retire_interaction_presentation(presentation_id);
                    self.reset_frame_attempt_state();
                    return FrameLayoutInnerOutcome::Aborted {
                        settle_no_safe: false,
                    };
                };
                (
                    inputs.frame,
                    vec![inputs.window],
                    Some(inputs.main_area_bottom),
                    Some(inputs.divider_corner),
                )
            } else {
                match super::neovm_bridge::collect_layout_params_with_font_sizing(
                    evaluator,
                    frame_id,
                    default_metrics.map(|metrics| metrics.ascent),
                    self.font_sizing,
                ) {
                    Some((frame, windows)) => (frame, windows, None, None),
                    None => {
                        tracing::error!("layout_frame_rust: frame {:?} not found", frame_id);
                        evaluator.retire_interaction_presentation(presentation_id);
                        return FrameLayoutInnerOutcome::Aborted {
                            settle_no_safe: purpose == LayoutPurpose::Redisplay,
                        };
                    }
                }
            };

            // Capture sibling ownership once per frame attempt. Synchronous
            // queries already captured only their target through its checked path.
            let divider_corners = if let Some(corner) = query_divider_corner {
                let Some(params) = window_params_list.first() else {
                    return FrameLayoutInnerOutcome::Aborted {
                        settle_no_safe: false,
                    };
                };
                let mut corners = rustc_hash::FxHashMap::default();
                corners.insert(
                    neovm_core::window::WindowId(params.window_id as u64),
                    corner,
                );
                corners
            } else {
                let corners = evaluator
                    .frame_manager()
                    .get(frame_id)
                    .and_then(super::neovm_bridge::divider_corner_policies);
                let Some(corners) = corners else {
                    evaluator.retire_interaction_presentation(presentation_id);
                    self.reset_frame_attempt_state();
                    return FrameLayoutInnerOutcome::Aborted {
                        settle_no_safe: purpose == LayoutPurpose::Redisplay,
                    };
                };
                corners
            };

            // GNU seeds desired-window layout from the current matrix's
            // accepted chrome heights, falling back to a face estimate when a
            // row is new.  Apply the same single authority before cache
            // classification, body allocation, and chrome shaping.
            for params in &mut window_params_list {
                if let Some(metrics) =
                    window_chrome_metrics.get(&DisplayWindowId::new(params.window_id))
                {
                    metrics.seed_params(params);
                }
            }

            let window_sources: Vec<_> = match window_input_ownership::mode() {
                window_input_ownership::WindowInputOwnership::InPlace => window_params_list
                    .iter_mut()
                    .map(|params| {
                        resolve_window_display_source_params(evaluator, params, layout_walk_purpose)
                    })
                    .collect(),
                window_input_ownership::WindowInputOwnership::LegacyCopies => {
                    let resolved_window_sources: Vec<_> = window_params_list
                        .iter()
                        .map(|params| {
                            resolve_window_display_source_params_legacy(
                                evaluator,
                                params,
                                layout_walk_purpose,
                            )
                        })
                        .collect();
                    let sources = resolved_window_sources
                        .iter()
                        .map(|resolved| resolved.source)
                        .collect();
                    window_params_list = resolved_window_sources
                        .into_iter()
                        .map(|resolved| resolved.params)
                        .collect();
                    sources
                }
            };
            if minibuffer_measurement_needs_begv
                && let Some(mini_params) = window_params_list
                    .iter_mut()
                    .find(|params| params.is_minibuffer())
            {
                let buffer_id = neovm_core::buffer::BufferId(mini_params.buffer_id);
                if resize_mini_windows_mode_for_buffer(evaluator, buffer_id).should_grow()
                    && uses_adhoc_minibuffer_resize_scroll(evaluator, buffer_id)
                {
                    // GNU resize_mini_window starts its measurement at BEGV.
                    // If all content fits, this remains the accepted start; if
                    // it overflows, the visibility convergence below chooses
                    // the tail start once and subsequent frame retries keep it.
                    mini_params.window_start = mini_params.buffer_begv;
                    mini_params.previous_visible_end = None;
                    mini_params.force_start = false;
                }
            }
            let main_area_bottom = query_main_area_bottom.unwrap_or_else(|| {
                window_params_list
                    .iter()
                    .filter(|params| !params.is_minibuffer())
                    .map(|params| params.bounds.y + params.bounds.height)
                    .fold(0.0_f32, f32::max)
            });

            // --- Pre-fontification dirty-span snapshot ---
            // GNU's this_line/try_window_id decision reads BEG/END_UNCHANGED
            // BEFORE fontification runs (fontification fires inside
            // display_line via handle_fontified_prop, after the decision), so
            // the keystroke's damage is the edited line — not the jit-lock
            // chunk font-lock is about to unfontify+refontify. Snapshot each
            // buffer's accumulated span now; Phase A classification consumes
            // this snapshot. The fontification pass's own property damage is
            // acked at the accepted break exactly like GNU's
            // mark_window_display_accurate; a contextual face change beyond
            // the span repaints via jit-lock's deferred contextual pass
            // (jit-lock-context-timer), same as GNU.
            self.pre_fontify_dirty_spans.clear();
            self.pre_fontify_gnu_beg_unchanged.clear();
            for params in &window_params_list {
                let buf_id = neovm_core::buffer::BufferId(params.buffer_id);
                if let Some(buffer) = evaluator.buffer_manager().get(buf_id) {
                    self.pre_fontify_dirty_spans
                        .entry(params.buffer_id)
                        .or_insert_with(|| buffer.changed_char_range());
                    if let Some(beg) = buffer.gnu_beg_unchanged() {
                        self.pre_fontify_gnu_beg_unchanged
                            .insert(params.buffer_id, beg);
                    }
                }
            }
            // `bset_redisplay` (xdisp.c:886-898) turns any change to a buffer
            // shown in a non-selected window, or in two windows, into
            // `windows_or_buffers_changed`, which disables GNU's one-line
            // optimization for the whole redisplay. The mini-window is left
            // out: echo-area text reaches it through `echo_area_display`.
            self.frame_other_windows_changed = evaluator
                .frame_manager()
                .other_window_buffer_changed(evaluator.buffer_manager());

            self.reset_frame_attempt_state();
            // A relayout pass must not inherit `when` records settled by the
            // rejected pass. The replay budget in the same ledger is per
            // logical attempt, not per pass, so it stays.
            lisp_ledger.display_when.begin_frame_pass();
            let face_table = committed_face_arena.begin_table_attempt();
            let mut face_attempt = face_table.handle();
            self.frame_output.set_face_attempt(face_attempt.clone());
            self.frame_output.set_presentation_id(presentation_id);
            let mut curr_window_infos: rustc_hash::FxHashMap<DisplayWindowId, WindowInfo> =
                rustc_hash::FxHashMap::default();
            let mut observed_window_navigation_intents = Vec::new();
            let default_resolved = face_resolver.default_face();
            let child_frame_border = face_resolver
                .resolve_fixed_face(neovm_core::emacs_core::intern::WellKnown::ChildFrameBorder);

            // Set up frame dimensions in the builder
            let frame_identity = if let Some(frame) = evaluator.frame_manager().get(frame_id) {
                let parent_id = frame.parent_frame.as_frame_id().unwrap_or(0);
                let (parent_x, parent_y) = if parent_id == 0 {
                    (0.0, 0.0)
                } else {
                    (frame.left_pos as f32, frame.top_pos as f32)
                };
                Some(FrameOutputIdentity {
                    frame_id: frame.id.0,
                    parent_id,
                    parent_x,
                    parent_y,
                    z_order: frame.z_order,
                    undecorated: frame.undecorated,
                    border_width: frame.internal_border_width() as f32,
                    border_color: Color::from_pixel(child_frame_border.bg),
                    outer_border_width: frame.outer_border_width() as f32,
                    outer_border_color: frame
                        .known_parameter(FrameParam::BorderColor)
                        .and_then(|value| value.as_utf8_str())
                        .and_then(neovm_core::face::Color::parse)
                        .map(|color| Color::from_pixel(color.to_pixel()))
                        .unwrap_or(Color::BLACK),
                    background_alpha: frame.background_alpha,
                    frame_alpha: {
                        let mut alpha = frame.frame_alpha;
                        let limit = neovm_core::window::frame_alpha::lower_limit(
                            crate::neovm_bridge::visible_value(
                                evaluator,
                                crate::fixed_symbols::FRAME_ALPHA_LOWER_LIMIT.id(),
                            )
                            .unwrap_or(Value::fixnum(20)),
                        );
                        for value in &mut alpha {
                            if *value >= 0.0 && (0.0..=1.0).contains(&limit) {
                                *value = value.max(limit);
                            }
                        }
                        alpha
                    },
                    no_accept_focus: frame.no_accept_focus,
                })
            } else {
                None
            };
            self.frame_output
                .render_frame_state(FrameOutputStateRenderRequest::new(
                    frame_identity,
                    Color::from_pixel(frame_params.background),
                    frame_params.font_pixel_size,
                    default_resolved,
                    default_metrics,
                ));

            // Tab-bar Lisp was evaluated once by the logical GUI-chrome
            // preflight above. Clone only the rooted semantic payload here;
            // shaping remains physical so a measured-height retry can converge
            // without re-entering Lisp or changing GNU callback order.
            let tab_bar_height = frame_params.tab_bar_height;
            let built_tab_bar = prepared_gui_chrome
                .as_ref()
                .and_then(|chrome| chrome.built_tab_bar.clone());

            let Some(window_layout_inputs): Option<Vec<(WindowFrameGeometry, WindowLayoutBox)>> =
                window_params_list
                    .iter()
                    .map(|params| {
                        let corner = *divider_corners
                            .get(&neovm_core::window::WindowId(params.window_id as u64))?;
                        let geometry = WindowFrameGeometryRequest::new(
                            params,
                            &frame_params,
                            main_area_bottom,
                            corner,
                        )
                        .resolve();
                        let layout_box = WindowLayoutBox::resolve(
                            params,
                            WindowChromeMetrics::from_params(params),
                            WindowDividerLayout::resolve(params, &frame_params, geometry),
                        );
                        Some((geometry, layout_box))
                    })
                    .collect()
            else {
                evaluator.retire_interaction_presentation(presentation_id);
                self.reset_frame_attempt_state();
                return FrameLayoutInnerOutcome::Aborted {
                    settle_no_safe: purpose == LayoutPurpose::Redisplay,
                };
            };

            // Snapshot the exact accepted partition along with the other
            // incremental-layout inputs. A retry discards this vector, so only
            // signatures derived from a converged WindowLayoutBox are retained.
            let mut retained_keys: Vec<(DisplayWindowId, RetainedWindowKey)> = window_params_list
                .iter()
                .zip(&window_layout_inputs)
                .map(|(params, (_, layout_box))| {
                    (
                        DisplayWindowId::new(params.window_id),
                        RetainedWindowKey::from_params(params, *layout_box, evaluator),
                    )
                })
                .collect();

            // --- Phase A (single-threaded gather, spec §4.5) ---
            //
            // Classify every incremental fast path before ANY dynamic face is
            // allocated for this attempt. This is both the multi-window
            // same-buffer ordering guarantee and the ownership boundary for the
            // frame-wide face namespace below.
            lisp_ledger.replay_sources.clear();
            let mut window_plans: Vec<IncrementalWindowPlan> =
                Vec::with_capacity(window_params_list.len());
            for ((params, (_, layout_box)), (_, key)) in window_params_list
                .iter()
                .zip(&window_layout_inputs)
                .zip(&retained_keys)
            {
                if query_window.is_some()
                    || layout_walk_purpose == WindowLayoutWalkPurpose::Snapshot
                    || attempt_mode == FrameAttemptMode::ExhaustionSnapshot
                {
                    // Snapshot/query rows must start at the live marker;
                    // ExhaustionSnapshot rebuilds cleanly without fast-path reuse.
                    window_plans.push(IncrementalWindowPlan {
                        prepared_faces: None,
                        cursor_only: None,
                        scroll: None,
                        is_edit: false,
                        retry_admitted_faces: None,
                        scroll_continuation_faces: None,
                    });
                    continue;
                }
                // The mini-window is probe-excluded from retention: its
                // layout runs `resize_mini_window` measurement passes whose
                // results must not become a reusable matrix, and it is one
                // or two rows tall, so the scroll and edit paths buy
                // nothing.  It is re-walked every frame (the phase tests
                // assert exactly this by counting the content window's
                // classification) -- unless NEOMACS_LAYOUT_MINI_STILL lets
                // it stand still: GNU's `redisplay_window` leaves an echo
                // area it "already displayed" alone, and a retained key
                // equal in every field (the displayed echo or minibuffer
                // buffer and its ticks, point, cursor role, geometry
                // including the resize measurement's start) is exactly
                // that, so the cursor-only replay reuses every row.
                if params.is_minibuffer() {
                    let cursor_only = if crate::incremental_layout::mini_window_still_enabled() {
                        self.build_cursor_only_replay(params, *layout_box, evaluator)
                    } else {
                        None
                    };
                    window_plans.push(IncrementalWindowPlan {
                        prepared_faces: None,
                        cursor_only,
                        scroll: None,
                        is_edit: false,
                        retry_admitted_faces: None,
                        scroll_continuation_faces: None,
                    });
                    continue;
                }
                let mut cursor_only = self.build_cursor_only_replay(params, *layout_box, evaluator);
                let mut prepared_faces = None;
                if cursor_only.is_none() {
                    if let Some((replay, faces, sources)) = self.prepared_viewports.replay(
                        frame_id,
                        DisplayWindowId::new(params.window_id),
                        key,
                        params.force_start,
                    ) {
                        if let Some(sources) = sources {
                            lisp_ledger
                                .replay_sources
                                .insert(DisplayWindowId::new(params.window_id), sources);
                        }
                        cursor_only = Some(replay);
                        prepared_faces = Some(faces);
                    }
                }

                let mut is_edit = false;
                let mut scroll = if cursor_only.is_none() {
                    if let Some(scroll) = self.build_scroll_replay(params, *layout_box, evaluator) {
                        Some(scroll)
                    } else if let Some(edit) =
                        self.build_edit_replay(params, *layout_box, evaluator)
                    {
                        is_edit = true;
                        Some(edit)
                    } else {
                        None
                    }
                } else {
                    None
                };
                // With no partial walk, the leaf may resolve a different
                // semantic start before producing rows. History keys name
                // that displayed start. Preserve ordinary scroll/edit plans,
                // which consume the input start exactly, but prefer a full
                // historical viewport over shifting a subset of that page.
                if cursor_only.is_none() && scroll.is_none() {
                    let resolved_start = resolve_leaf_window_start(
                        evaluator,
                        params,
                        &frame_params,
                        layout_box,
                        WindowPositionPublication::Redisplay,
                        false,
                    );
                    if resolved_start.get() != key.window_start {
                        let mut prepared_key = key.clone();
                        prepared_key.window_start = resolved_start.get();
                        if let Some((replay, faces, sources)) = self.prepared_viewports.replay(
                            frame_id,
                            DisplayWindowId::new(params.window_id),
                            &prepared_key,
                            params.force_start,
                        ) {
                            if let Some(sources) = sources {
                                lisp_ledger
                                    .replay_sources
                                    .insert(DisplayWindowId::new(params.window_id), sources);
                            }
                            cursor_only = Some(replay);
                            prepared_faces = Some(faces);
                        }
                    }
                }
                // A cached page supplies content, not a new viewport
                // decision. Point motion alone must still run recentering.
                // Prefer an existing backward synchronization plan over
                // shifting an older page with less predictable overlap.
                if cursor_only.is_none()
                    && scroll.as_ref().is_none_or(|replay| replay.sync.is_none())
                    && params.selected
                    && self
                        .retained_window_matrices
                        .get(&DisplayWindowId::new(params.window_id))
                        .is_some_and(|previous| {
                            RetainedWindowKey::scroll_eligible(&previous.key, key)
                        })
                {
                    let reused = scroll.as_ref().map_or(0, |replay| replay.reused_rows.len());
                    if let Some((replay, faces, sources)) = self.prepared_viewports.scroll_replay(
                        frame_id,
                        DisplayWindowId::new(params.window_id),
                        key,
                        reused,
                    ) {
                        if let Some(sources) = sources {
                            lisp_ledger
                                .replay_sources
                                .insert(DisplayWindowId::new(params.window_id), sources);
                        }
                        scroll = Some(replay);
                        prepared_faces = Some(faces);
                        is_edit = false;
                    }
                }
                if cursor_only.is_none()
                    && !is_edit
                    && !params.is_minibuffer()
                    && self
                        .prepared_viewports
                        .has_computed(frame_id, DisplayWindowId::new(params.window_id))
                    && params.display_line_numbers == crate::types::DisplayLineNumbersMode::Off
                {
                    let mut geometry = BufferWindowGeometryRequest::new(
                        params,
                        layout_box,
                        params.char_width,
                        params.char_height,
                    )
                    .into_geometry(
                        crate::display_row::walk_state::LineNumberFieldLayout::new(
                            0,
                            params.char_width,
                        ),
                    );
                    if params.window_system
                        && params.measurement_rows.is_none()
                        && params.measurement_pixels.is_none()
                        && params.source_extent == crate::types::WindowSourceExtent::Viewport
                        && let Some(buffer) = evaluator
                            .buffer_manager()
                            .get(neovm_core::buffer::BufferId(params.buffer_id))
                        && let Some(metrics) = self.font_metrics.as_mut()
                    {
                        let start =
                            neovm_core::buffer::CharPos0::new(key.window_start.max(0) as usize);
                        let view =
                            crate::neovm_bridge::BorrowedLayoutBuffer::for_window_with_symbols(
                                buffer,
                                evaluator.variables(),
                                start,
                                1,
                                crate::display_property::DisplayPropertyTarget::Graphical,
                                evaluator.symbol_mode(),
                            );
                        if let Some(face_line) =
                            crate::buffer_source::window_render::shorter_face_line_at_window_start(
                                &view,
                                params.window_id as u64,
                                start,
                                default_resolved,
                                self.font_sizing,
                                metrics,
                            )
                        {
                            geometry.widen_body_for_shorter_face(face_line);
                        }
                    }
                    let projected = scroll
                        .is_none()
                        .then(|| {
                            self.retained_window_matrices
                                .get(&DisplayWindowId::new(params.window_id))
                                .and_then(|previous| {
                                    previous.prepared_projection_prefix(
                                        key,
                                        geometry.text_y - geometry.vscroll - params.bounds.y,
                                    )
                                })
                        })
                        .flatten();
                    let prefix_sources = lisp_ledger
                        .replay_sources
                        .get(&DisplayWindowId::new(params.window_id))
                        .cloned()
                        .or_else(|| {
                            self.frame_visual_histories
                                .get(frame_id)
                                .and_then(|history| {
                                    history
                                        .cursor_sources_for(DisplayWindowId::new(params.window_id))
                                })
                        });
                    if let Some(prefix) = scroll.as_ref().or(projected.as_ref())
                        && let Some(prefix_sources) = prefix_sources
                        && let Some((replay, faces, sources)) =
                            self.prepared_viewports.complete_forward_scroll(
                                frame_id,
                                DisplayWindowId::new(params.window_id),
                                key,
                                prefix,
                                geometry,
                                &committed_face_arena,
                                params.force_start,
                                prefix_sources,
                            )
                    {
                        if let Some(sources) = sources {
                            lisp_ledger
                                .replay_sources
                                .insert(DisplayWindowId::new(params.window_id), sources);
                        }
                        cursor_only = Some(replay);
                        prepared_faces = Some(faces);
                        scroll = None;
                    }
                }
                // A cursor-only decline or edit can discover a new viewport
                // while measuring. Admit all retained row faces in Phase A
                // so a later scroll continuation can reuse any of those rows
                // -- but only while a continuation could reuse them at all:
                // after a face/font/display change their IDs name stale
                // realizations of faces the fresh walk re-realizes.
                let scroll_continuation_faces = (cursor_only.is_none()
                    && (scroll.is_none() || is_edit))
                    .then(|| {
                        self.retained_window_matrices
                            .get(&DisplayWindowId::new(params.window_id))
                    })
                    .flatten()
                    .and_then(|previous| {
                        previous
                            .continuation_candidate_rows(key)
                            .map(|rows| (previous.face_generation, rows))
                    });
                window_plans.push(IncrementalWindowPlan {
                    prepared_faces,
                    cursor_only,
                    scroll,
                    is_edit,
                    retry_admitted_faces: None,
                    scroll_continuation_faces,
                });
            }

            // Admit all prior-frame face identities as one atomic batch before
            // the tab bar or any window can mint fresh IDs. A stale retained
            // generation invalidates every fast path for this attempt.
            if let Err(error) = admit_retained_frame_faces(
                &mut window_plans,
                &mut face_attempt,
                &committed_face_arena,
            ) {
                tracing::warn!(
                    ?error,
                    "discarding incremental frame plans with stale retained faces"
                );
                for plan in &mut window_plans {
                    plan.disable_reuse();
                }
            }

            if let Some(tab_bar) = built_tab_bar
                && let Some(actual_tab_bar_height) = self.render_frame_tab_bar_rust(
                    evaluator,
                    &face_resolver,
                    &frame_params,
                    tab_bar_height,
                    presentation_id,
                    tab_bar,
                    &face_attempt,
                )
                && actual_tab_bar_height != tab_bar_height
                && attempt_mode == FrameAttemptMode::Converging
            {
                let request = FrameRelayoutRequest::FrameTabBar {
                    assumed_height: tab_bar_height,
                    measured_height: actual_tab_bar_height,
                };
                match self.decide_frame_relayout(
                    &mut layout_coordinator,
                    evaluator,
                    frame_id,
                    purpose,
                    attempt_mode,
                    &mut frame_window_end_attempts,
                    presentation_id,
                    request,
                ) {
                    RelayoutDecision::Retry => {
                        if let Some(frame) = evaluator.frame_manager_mut().get_mut(frame_id) {
                            frame.tab_bar_height = actual_tab_bar_height.max(1.0) as u32;
                            frame.sync_window_area_bounds();
                        }
                        continue 'frame_layout;
                    }
                    RelayoutDecision::EnterExhaustionSnapshot => {
                        if let Some(frame) = evaluator.frame_manager_mut().get_mut(frame_id) {
                            frame.tab_bar_height = actual_tab_bar_height.max(1.0) as u32;
                            frame.sync_window_area_bounds();
                        }
                        attempt_mode = FrameAttemptMode::ExhaustionSnapshot;
                        continue 'frame_layout;
                    }
                    RelayoutDecision::Abort { settle_no_safe } => {
                        return FrameLayoutInnerOutcome::Aborted { settle_no_safe };
                    }
                }
            }

            tracing::debug!(
                "layout_frame_rust: {}x{} char={}x{} windows={}",
                frame_params.width,
                frame_params.height,
                frame_params.char_width,
                frame_params.char_height,
                window_params_list.len()
            );

            // --- Phase B (per-window layout; single-threaded today) ---
            for (window_index, (planned_params, mut plan)) in
                window_params_list.iter().zip(window_plans).enumerate()
            {
                let window_id = neovm_core::window::WindowId(planned_params.window_id as u64);
                let Some(window_path) = window_paths.get(&window_id) else {
                    let request = FrameRelayoutRequest::LogicalInputsChanged {
                        window_id: DisplayWindowId::new(planned_params.window_id),
                    };
                    request_frame_relayout!('frame_layout, request);
                };
                let mut live_inputs = LiveWindowLayoutInputs {
                    frame: frame_params.clone(),
                    window: planned_params.clone(),
                    source: window_sources[window_index],
                    main_area_bottom,
                    divider_corner: window_layout_inputs[window_index].0.divider_corner,
                };

                // Earlier leaves may have evaluated arbitrary mode/header-line
                // Lisp. Recollect before this leaf and retain the Phase-A plan
                // only if its complete typed key still matches. This target-
                // only projection is O(tree depth); a full-frame bridge walk
                // here would be O(windows) per leaf. Synchronous queries have
                // no earlier leaves and retain their exact-start projection.
                if query_window.is_none() {
                    let Some(current) = collect_live_window_layout_inputs(
                        evaluator,
                        frame_id,
                        window_id,
                        window_path,
                        default_metrics.map(|metrics| metrics.ascent),
                        self.font_sizing,
                        &window_chrome_metrics,
                        layout_walk_purpose,
                    ) else {
                        let request = FrameRelayoutRequest::LogicalInputsChanged {
                            window_id: DisplayWindowId::new(planned_params.window_id),
                        };
                        request_frame_relayout!('frame_layout, request);
                    };
                    live_inputs = current;
                    // GNU's preliminary resize-mini-window walk intentionally
                    // measures from BEGV. Reapply that typed projection only
                    // after classifying the freshly recollected leaf, so Lisp
                    // from an earlier sibling can turn measurement on or off.
                    if minibuffer_measurement_needs_begv
                        && matches!(
                            window_position_publication(
                                evaluator,
                                &live_inputs.window,
                                layout_walk_purpose,
                                live_inputs.source,
                            ),
                            WindowPositionPublication::RedisplayMinibufferMeasurement
                        )
                    {
                        live_inputs.window.window_start = live_inputs.window.buffer_begv;
                        live_inputs.window.previous_visible_end = None;
                        live_inputs.window.force_start = false;
                    }
                }
                let mut position_publication = window_position_publication(
                    evaluator,
                    &live_inputs.window,
                    layout_walk_purpose,
                    live_inputs.source,
                );
                let mut window_geometry = WindowFrameGeometryRequest::new(
                    &live_inputs.window,
                    &live_inputs.frame,
                    live_inputs.main_area_bottom,
                    live_inputs.divider_corner,
                )
                .resolve();
                let mut layout_box = WindowLayoutBox::resolve(
                    &live_inputs.window,
                    WindowChromeMetrics::from_params(&live_inputs.window),
                    WindowDividerLayout::resolve(
                        &live_inputs.window,
                        &live_inputs.frame,
                        window_geometry,
                    ),
                );
                if let Some(measured) = position_chrome
                    && measured
                        .validate_owner(frame_id, &live_inputs.window)
                        .is_err()
                {
                    evaluator.retire_interaction_presentation(presentation_id);
                    self.reset_frame_attempt_state();
                    return FrameLayoutInnerOutcome::Aborted {
                        settle_no_safe: false,
                    };
                }
                let live_key =
                    RetainedWindowKey::from_params(&live_inputs.window, layout_box, evaluator);
                if retained_keys[window_index].1 != live_key {
                    plan.disable_reuse();
                }
                let consumed_force_start = live_inputs.window.force_start;
                let params = &live_inputs.window;
                tracing::debug!(
                    "layout window: id={} buf={} bounds=({:.0},{:.0},{:.0},{:.0}) mini={} selected={} mode_line_h={:.0}",
                    params.window_id,
                    params.buffer_id,
                    params.bounds.x,
                    params.bounds.y,
                    params.bounds.width,
                    params.bounds.height,
                    params.is_minibuffer(),
                    params.selected,
                    params.mode_line_height,
                );

                if plan.prepared_faces.is_some() {
                    self.prepared_window_ids
                        .insert(DisplayWindowId::new(params.window_id));
                }
                // A point move or face-overlay edit can discover a new viewport
                // while measuring. A later scroll continuation may only reuse
                // faces already admitted by Phase A; fresh face IDs have been
                // allocated by now, so never admit more retained IDs here.
                let retry_retained_faces = plan.retry_admitted_faces.take().map(|faces| {
                    debug_assert!(faces.is_sorted());
                    (plan.retained_face_generation(), faces)
                });
                // Phase A already admitted these rows' faces. Only the generation
                // is needed to validate a continuation; release the rows now.
                let retry_continuation_generation = plan
                    .scroll_continuation_faces
                    .take()
                    .map(|(generation, _)| generation);
                let mut cursor_only_replay = plan.cursor_only.take();
                let mut scroll_replay = plan.scroll.take();
                let mut is_edit = plan.is_edit;
                let mut visibility_retry_budget = if query_window.is_some()
                    || layout_walk_purpose == WindowLayoutWalkPurpose::Snapshot
                {
                    0
                } else {
                    MAX_WINDOW_VISIBILITY_RETRIES
                };
                let mut viewport_retry_phase = ViewportResolutionPhase::Resolve;
                let mut overlay_view_full = false;
                // Automatic-composition resolution range widened by the
                // previous attempt's post-walk coverage check, if any
                // (`None` = the normal visible-range pass). `(0, usize::MAX)`
                // resolves every span the snapshot captured.
                let mut automatic_composition_resolve: Option<(usize, usize)> = None;
                let mut prepass_recollect = PrepassRecollectState::Fresh;
                let allow_prepass_recollect = purpose == LayoutPurpose::Redisplay
                    && attempt_mode == FrameAttemptMode::Converging;
                let capture_chrome_for_exhaustion = attempt_mode == FrameAttemptMode::Converging
                    && layout_coordinator.retry_count() > 0;
                let completed_row_key;
                let window_layout = loop {
                    // A leaf hook/visibility continuation can re-enter this
                    // loop without visiting the outer transaction restart.
                    // Reject before rendering any stale logical face sources.
                    let current_face_change_count = evaluator.face_change_count();
                    if attempt_mode == FrameAttemptMode::Converging
                        && face_resolver.has_tty_realization()
                        && current_face_change_count != resolver_face_change_count
                    {
                        let request = FrameRelayoutRequest::FaceRealizationInputsChanged {
                            before: resolver_face_change_count,
                            after: current_face_change_count,
                        };
                        request_frame_relayout!('frame_layout, request);
                    }
                    let bound_faces = face_services.bind(&face_resolver);
                    let output_checkpoint =
                        self.frame_output
                            .capture_leaf_attempt_checkpoint(DisplayWindowId::new(
                                live_inputs.window.window_id,
                            ));
                    let cursor_checkpoint = leaf_cursor_checkpoint::LeafCursorCheckpoint::capture(
                        evaluator, frame_id, window_id,
                    );
                    let attempt = self.layout_window_rust(
                        evaluator,
                        frame_id,
                        &live_inputs.window,
                        &live_inputs.frame,
                        &layout_box,
                        bound_faces.services(),
                        window_geometry.reserve_terminal_right_border_col,
                        visibility_retry_budget,
                        viewport_retry_phase.clone(),
                        overlay_view_full,
                        automatic_composition_resolve,
                        cursor_only_replay.take(),
                        scroll_replay.take(),
                        is_edit,
                        position_publication,
                        position_chrome.is_none()
                            && !chrome_remeasured
                                .contains(&DisplayWindowId::new(live_inputs.window.window_id)),
                        &mut lisp_ledger,
                        attempt_topology_generation,
                        &face_attempt,
                        attempt_mode,
                        allow_prepass_recollect,
                        capture_chrome_for_exhaustion,
                        &mut prepass_recollect,
                    );
                    // Every in-leaf continuation passes this boundary, including
                    // implicit loop re-entry after Retry/Effect. The closed row
                    // grid and paint/cursor artifacts remain speculative until
                    // the post-walk checks accept this attempt.
                    if !matches!(&attempt, LeafLayoutAttempt::Completed { .. }) {
                        self.frame_output
                            .restore_leaf_attempt_checkpoint(output_checkpoint);
                    }
                    let accepted = matches!(
                        &attempt,
                        LeafLayoutAttempt::Completed {
                            outcome: WindowLayoutOutcome::Stable { .. }
                                | WindowLayoutOutcome::Skipped,
                            ..
                        }
                    );
                    if !accepted && let Some(checkpoint) = cursor_checkpoint {
                        checkpoint.restore(evaluator, frame_id, window_id);
                    }
                    match attempt {
                        LeafLayoutAttempt::Completed {
                            outcome,
                            window_end_attempt,
                            snapshot,
                            produced_key,
                        } => {
                            completed_row_key = produced_key;
                            if accepted {
                                self.window_snapshots.extend(snapshot);
                            }
                            frame_window_end_attempts.stage(window_end_attempt);
                            break outcome;
                        }
                        LeafLayoutAttempt::Retry(retry) => {
                            // Same-inputs continuation: the attempt committed a
                            // new window start (or point) and asks for one more
                            // bounded attempt.  Do not re-collect live inputs —
                            // the retry carries the exact params the recursive
                            // call site used to build. Reclassify a committed
                            // scroll against those inputs only when its faces
                            // are a subset of the original admitted plan.
                            // Same-start failures still retry without reuse.
                            if matches!(
                                retry.viewport_resolution,
                                ViewportResolutionPhase::Commit(_)
                            ) && retry.params.window_start != live_inputs.window.window_start
                                && let Some(replay) =
                                    self.build_scroll_replay(&retry.params, layout_box, evaluator)
                            {
                                // Edit retries must stay within their Phase A
                                // admission union. Other continuations reuse the
                                // matrix whose enabled rows were all admitted,
                                // so matching its generation is sufficient.
                                let faces_admitted =
                                    match (&retry_retained_faces, &retry_continuation_generation) {
                                        (Some((generation, faces)), _) => {
                                            *generation == Some(replay.face_generation)
                                                && replay
                                                    .retained_face_ids()
                                                    .iter()
                                                    .filter(|id| **id != FaceId::new(0))
                                                    .all(|id| faces.binary_search(id).is_ok())
                                        }
                                        (None, Some(generation)) => {
                                            *generation == replay.face_generation
                                        }
                                        (None, None) => false,
                                    };
                                if faces_admitted {
                                    scroll_replay = Some(replay);
                                }
                            }
                            live_inputs.window = retry.params;
                            visibility_retry_budget = retry.remaining_visibility_retries;
                            viewport_retry_phase = retry.viewport_resolution;
                            is_edit = false;
                        }
                        LeafLayoutAttempt::OverlayViewExhausted => {
                            // The attempt read a subset of the buffer's
                            // overlays past the range view's end. It is
                            // discarded; one more attempt uses the
                            // whole-buffer view, which cannot exhaust.
                            debug_assert!(
                                !overlay_view_full,
                                "whole-buffer overlay view reported exhaustion"
                            );
                            overlay_view_full = true;
                            continue;
                        }
                        LeafLayoutAttempt::AutomaticCompositionUnresolved {
                            resolve_from,
                            resolve_through,
                        } => {
                            // The walk painted a composition the visible-range
                            // pass did not resolve. Discard the attempt and
                            // retry with the range widened to cover it; if that
                            // second pass still misses (a rule may change the
                            // walk's own extent), resolve every span the
                            // snapshot captured, which cannot miss.
                            automatic_composition_resolve =
                                Some(match automatic_composition_resolve {
                                    Some(_) => (0, usize::MAX),
                                    None => (resolve_from, resolve_through),
                                });
                            continue;
                        }
                        LeafLayoutAttempt::Effect(effect) => {
                            lisp_ledger.acknowledge_scroll_hook(&effect);
                            if let Err(flow) = effect.execute_inline(evaluator) {
                                evaluator.defer_redisplay_hook_flow(flow);
                                frame_window_end_attempts.reject_all(evaluator);
                                evaluator.retire_interaction_presentation(presentation_id);
                                self.last_frame_display_state = None;
                                self.reset_frame_attempt_state();
                                return FrameLayoutInnerOutcome::Aborted {
                                    settle_no_safe: false,
                                };
                            }
                            lisp_ledger
                                .acknowledge_live_hook_resume(evaluator, frame_id, window_id);
                            let current_topology_generation =
                                evaluator.frame_manager().window_topology_generation();
                            if current_topology_generation != attempt_topology_generation {
                                let request = FrameRelayoutRequest::WindowTopologyChanged {
                                    before: attempt_topology_generation,
                                    after: current_topology_generation,
                                };
                                request_frame_relayout!('frame_layout, request);
                            }
                            let Some(mut current) = collect_live_window_layout_inputs(
                                evaluator,
                                frame_id,
                                window_id,
                                window_path,
                                default_metrics.map(|metrics| metrics.ascent),
                                self.font_sizing,
                                &window_chrome_metrics,
                                layout_walk_purpose,
                            ) else {
                                let request = FrameRelayoutRequest::LogicalInputsChanged {
                                    window_id: DisplayWindowId::new(live_inputs.window.window_id),
                                };
                                request_frame_relayout!('frame_layout, request);
                            };
                            current.window.force_start |= consumed_force_start;
                            live_inputs = current;
                            window_geometry = WindowFrameGeometryRequest::new(
                                &live_inputs.window,
                                &live_inputs.frame,
                                live_inputs.main_area_bottom,
                                live_inputs.divider_corner,
                            )
                            .resolve();
                            layout_box = WindowLayoutBox::resolve(
                                &live_inputs.window,
                                WindowChromeMetrics::from_params(&live_inputs.window),
                                WindowDividerLayout::resolve(
                                    &live_inputs.window,
                                    &live_inputs.frame,
                                    window_geometry,
                                ),
                            );
                            position_publication = window_position_publication(
                                evaluator,
                                &live_inputs.window,
                                layout_walk_purpose,
                                live_inputs.source,
                            );
                            is_edit = false;
                            prepass_recollect = PrepassRecollectState::Fresh;
                            // A hook effect re-enters this leaf from its live
                            // inputs, which is the same fresh attempt the old
                            // recursion performed by unwinding to this loop.
                            visibility_retry_budget = if query_window.is_some()
                                || layout_walk_purpose == WindowLayoutWalkPurpose::Snapshot
                            {
                                0
                            } else {
                                MAX_WINDOW_VISIBILITY_RETRIES
                            };
                            viewport_retry_phase = ViewportResolutionPhase::Resolve;
                        }
                        LeafLayoutAttempt::MatrixCapacityUnavailable => {
                            frame_window_end_attempts.reject_all(evaluator);
                            evaluator.retire_interaction_presentation(presentation_id);
                            self.last_frame_display_state = None;
                            self.reset_frame_attempt_state();
                            return FrameLayoutInnerOutcome::Aborted {
                                settle_no_safe: purpose == LayoutPurpose::Redisplay,
                            };
                        }
                        LeafLayoutAttempt::PrepassInputsChanged(next_recollect) => {
                            let Some(mut current) = collect_live_window_layout_inputs(
                                evaluator,
                                frame_id,
                                window_id,
                                window_path,
                                default_metrics.map(|metrics| metrics.ascent),
                                self.font_sizing,
                                &window_chrome_metrics,
                                layout_walk_purpose,
                            ) else {
                                let request = FrameRelayoutRequest::LogicalInputsChanged {
                                    window_id: DisplayWindowId::new(live_inputs.window.window_id),
                                };
                                request_frame_relayout!('frame_layout, request);
                            };
                            if minibuffer_measurement_needs_begv
                                && matches!(
                                    window_position_publication(
                                        evaluator,
                                        &current.window,
                                        layout_walk_purpose,
                                        current.source,
                                    ),
                                    WindowPositionPublication::RedisplayMinibufferMeasurement
                                )
                            {
                                current.window.window_start = current.window.buffer_begv;
                                current.window.previous_visible_end = None;
                                current.window.force_start = false;
                            }
                            current.window.force_start |= consumed_force_start;
                            live_inputs = current;
                            window_geometry = WindowFrameGeometryRequest::new(
                                &live_inputs.window,
                                &live_inputs.frame,
                                live_inputs.main_area_bottom,
                                live_inputs.divider_corner,
                            )
                            .resolve();
                            layout_box = WindowLayoutBox::resolve(
                                &live_inputs.window,
                                WindowChromeMetrics::from_params(&live_inputs.window),
                                WindowDividerLayout::resolve(
                                    &live_inputs.window,
                                    &live_inputs.frame,
                                    window_geometry,
                                ),
                            );
                            position_publication = window_position_publication(
                                evaluator,
                                &live_inputs.window,
                                layout_walk_purpose,
                                live_inputs.source,
                            );
                            let live_key = RetainedWindowKey::from_params(
                                &live_inputs.window,
                                layout_box,
                                evaluator,
                            );
                            retained_keys[window_index] =
                                (DisplayWindowId::new(live_inputs.window.window_id), live_key);
                            self.prepared_window_ids
                                .remove(&DisplayWindowId::new(live_inputs.window.window_id));
                            cursor_only_replay = None;
                            scroll_replay = None;
                            is_edit = false;
                            prepass_recollect = *next_recollect;
                        }
                        LeafLayoutAttempt::LogicalInputsChanged => {
                            if evaluator.has_mode_line_display_flow() {
                                frame_window_end_attempts.reject_all(evaluator);
                                evaluator.retire_interaction_presentation(presentation_id);
                                return FrameLayoutInnerOutcome::Aborted {
                                    settle_no_safe: false,
                                };
                            }
                            let request = FrameRelayoutRequest::LogicalInputsChanged {
                                window_id: DisplayWindowId::new(live_inputs.window.window_id),
                            };
                            request_frame_relayout!('frame_layout, request);
                        }
                        LeafLayoutAttempt::DisplayWhenReplay => {
                            if evaluator.has_mode_line_display_flow() {
                                frame_window_end_attempts.reject_all(evaluator);
                                evaluator.retire_interaction_presentation(presentation_id);
                                return FrameLayoutInnerOutcome::Aborted {
                                    settle_no_safe: false,
                                };
                            }
                            let request = FrameRelayoutRequest::DisplayWhenAnswerChanged {
                                window_id: DisplayWindowId::new(live_inputs.window.window_id),
                            };
                            request_frame_relayout!('frame_layout, request);
                        }
                    }
                };
                lisp_ledger.finish_hook_resume(window_id);
                let params = &live_inputs.window;
                let (accepted_layout_box, effective_default_face) = match window_layout {
                    WindowLayoutOutcome::Stable {
                        layout_box,
                        effective_default_face,
                    } => {
                        window_chrome_metrics
                            .insert(DisplayWindowId::new(params.window_id), layout_box.chrome());
                        (layout_box, Some(effective_default_face))
                    }
                    WindowLayoutOutcome::Skipped => (layout_box, None),
                    WindowLayoutOutcome::NeedsRelayout { assumed, measured } => {
                        let request = FrameRelayoutRequest::WindowChrome {
                            window_id: DisplayWindowId::new(params.window_id),
                            assumed,
                            measured,
                        };
                        match self.decide_frame_relayout(
                            &mut layout_coordinator,
                            evaluator,
                            frame_id,
                            purpose,
                            attempt_mode,
                            &mut frame_window_end_attempts,
                            presentation_id,
                            request,
                        ) {
                            RelayoutDecision::Retry => {
                                window_chrome_metrics
                                    .insert(DisplayWindowId::new(params.window_id), measured);
                                chrome_remeasured_next_round
                                    .insert(DisplayWindowId::new(params.window_id));
                                continue 'frame_layout;
                            }
                            RelayoutDecision::EnterExhaustionSnapshot => {
                                window_chrome_metrics
                                    .insert(DisplayWindowId::new(params.window_id), measured);
                                attempt_mode = FrameAttemptMode::ExhaustionSnapshot;
                                continue 'frame_layout;
                            }
                            RelayoutDecision::Abort { settle_no_safe } => {
                                return FrameLayoutInnerOutcome::Aborted { settle_no_safe };
                            }
                        }
                    }
                };
                if params.is_minibuffer() {
                    minibuffer_measurement_needs_begv = false;
                }

                retained_keys[window_index] = (
                    DisplayWindowId::new(params.window_id),
                    completed_row_key.unwrap_or_else(|| {
                        RetainedWindowKey::from_params(params, accepted_layout_box, evaluator)
                    }),
                );

                if let Some(snapshot) = self
                    .window_snapshots
                    .iter()
                    .map(WindowPresentationSnapshot::display_snapshot)
                    .find(|snapshot| snapshot.window_id.0 as i64 == params.window_id)
                {
                    debug_assert_eq!(snapshot.cell_origin.column().get(), params.left_col);
                    debug_assert_eq!(snapshot.cell_origin.line().get(), params.top_line);
                    if snapshot.regions_materialized {
                        debug_assert_eq!(snapshot.regions, accepted_layout_box.regions());
                    }
                    self.frame_output.publish_window_geometry(
                        params.window_id,
                        params.left_col,
                        params.top_line,
                        &snapshot.regions,
                        snapshot.regions_materialized,
                    );
                }

                let window_id = neovm_core::window::WindowId(params.window_id as u64);
                let pending_window_navigation = evaluator
                    .frame_manager()
                    .pending_window_navigation_intent(window_id);
                let transition_mode = if pending_frame_navigation.is_some() {
                    WindowContentTransitionMode::SuppressedByFrameNavigation {
                        superseded_navigation: pending_window_navigation
                            .map(|intent| intent.direction()),
                    }
                } else {
                    WindowContentTransitionMode::PerWindow {
                        navigation: pending_window_navigation.map(|intent| intent.direction()),
                    }
                };
                let navigation_observation = self.render_latest_window_output_info_effects(
                    &previous_visual_history,
                    &mut curr_window_infos,
                    transition_mode,
                );
                if let Some(direction) = navigation_observation.direction_to_acknowledge()
                    && let Some(intent) = pending_window_navigation
                {
                    debug_assert_eq!(direction, intent.direction());
                    observed_window_navigation_intents.push((window_id, intent));
                }

                if let Some(info) = self.latest_output_window_info(params.window_id) {
                    self.render_window_output_decorations(
                        params,
                        &live_inputs.frame,
                        window_geometry,
                        &info,
                        accepted_layout_box.divider_paint(),
                        effective_default_face.as_ref(),
                        &face_resolver,
                        &face_attempt,
                    );
                }
            }

            if let Some(target) = query_window {
                // A synchronous query owns its answer exactly as GNU
                // `Fwindow_end` owns its stack-local display iterator: this
                // physical attempt computed both fields, but publishes neither
                // into retained redisplay state.  Reading the live window here
                // would therefore return the last presentation's stale end.
                let query_snapshot = self
                    .window_snapshots
                    .iter()
                    .map(WindowPresentationSnapshot::display_snapshot)
                    .find(|snapshot| snapshot.window_id == target);
                let end = query_snapshot
                    .and_then(|snapshot| snapshot.window_end_record)
                    .and_then(|record| {
                        let params = window_params_list
                            .iter()
                            .find(|params| params.window_id == target.0 as i64)?;
                        let buffer_z = crate::coords::layout_i64_char_pos_to_lisp_char_pos(
                            params.accessible_end_charpos().get(),
                        );
                        Some(record.charpos_from_z(buffer_z))
                    })
                    .unwrap_or_else(|| {
                        // A zero-area leaf has no matrix row to carry an end
                        // record. GNU's stack-local iterator still has a
                        // coherent answer: the exact live start it was asked
                        // to walk from.
                        window_params_list.first().map_or(
                            neovm_core::buffer::LispCharPos1::ONE,
                            |params| {
                                let start = params.window_start.clamp(
                                    params.accessible_start_charpos().get(),
                                    params.accessible_end_charpos().get(),
                                );
                                crate::coords::layout_i64_char_pos_to_lisp_char_pos(start)
                            },
                        )
                    });
                // GNU's `pos_visible_p` and `buffer_posn_from_coords` use the
                // same on-demand walk from `w->start`; return that walk's
                // geometry rather than inventing a second approximation.
                let mut geometry = query_snapshot.cloned();
                if let LayoutPurpose::SynchronousQuery {
                    scope: neovm_core::window::WindowLayoutQueryScope::Pixels { height, .. },
                    ..
                } = purpose
                    && let Some(snapshot) = &mut geometry
                    && let Some(first) = snapshot.rows.first()
                {
                    // An inserted string can stage the next row before the
                    // source loop observes its pixel stop. It is outside this
                    // observation and may not yet contain the complete row.
                    let bottom = first
                        .y
                        .saturating_add(i64::try_from(height.get()).unwrap_or(i64::MAX));
                    snapshot.rows.retain(|row| row.y < bottom);
                    let last = snapshot.rows.last().map(|row| row.row);
                    if let Some(points) = &mut snapshot.point_rows {
                        points.rows.retain(|row| Some(row.row()) <= last);
                    }
                    snapshot.points.retain(|point| Some(point.row) <= last);
                    snapshot
                        .body_rows
                        .retain(|row| Some(row.output_row) <= last);
                }
                let invalid_face_references = face_resolver.take_invalid_face_references();
                face_services.note_invalid_face_references(&invalid_face_references);
                log_invalid_face_references(evaluator, &invalid_face_references);
                frame_window_end_attempts.reject_all(evaluator);
                evaluator.retire_interaction_presentation(presentation_id);
                let mut query_restart_rows = std::mem::take(&mut self.query_restart_rows);
                query_restart_rows.retain(|(anchor, index)| {
                    geometry.as_ref().is_some_and(|snapshot| {
                        snapshot
                            .rows
                            .iter()
                            .any(|row| row.row == *index && row.start_buffer_pos == Some(*anchor))
                    })
                });
                let mini_height = if matches!(purpose, LayoutPurpose::MiniPreparation { .. }) {
                    self.frame_output.mini_measurement_height_px(
                        target.0 as i64,
                        window_params_list
                            .first()
                            .map_or(1.0, |params| params.char_height.max(1.0)),
                    )
                } else {
                    None
                };
                self.reset_frame_attempt_state();
                self.mini_preparation_height = mini_height;
                // The completed query owns these small certificates until
                // `query_window_layout` transfers them into its cache entry.
                self.query_restart_rows = query_restart_rows;
                let witness = lisp_ledger
                    .rows_fontification
                    .finish(evaluator, geometry.as_ref());
                if let Some(producer) = rows_lifetime.as_deref_mut() {
                    producer.note_fontification(witness.as_ref());
                }
                #[cfg(test)]
                if let (Some(witness), Some(sink)) =
                    (witness, rows_fontification_sink.as_deref_mut())
                {
                    sink(witness);
                }
                #[cfg(not(test))]
                let _ = witness;
                return FrameLayoutInnerOutcome::Prepared(Some(
                    neovm_core::window::WindowLayoutQuery::new(end, geometry),
                ));
            }

            // GNU `resize_mini_window` compares measured content pixels with
            // the live allocation. Plan an achievable change before rejecting
            // this speculative frame, then apply it and retry within the
            // shared convergence budget. Resize policy controls which planned
            // growth or shrink is permitted; the frame owns its geometry.
            if attempt_mode == FrameAttemptMode::Converging
                && !(evaluator.gnu_redisplay_hooks_policy_enabled()
                    && evaluator.gnu_redisplay_transaction_active())
                && let Some(mini_params) = window_params_list.last()
                && mini_params.is_minibuffer()
                && let Some(mini_content_height_px) = self.output_window_automatic_mini_height_px(
                    mini_params,
                    Px(frame_params.char_height.max(1.0)),
                )
            {
                let char_h = frame_params.char_height.max(1.0);
                let frame_rows = frame_params.height / char_h;
                let max_mini_lines =
                    max_mini_window_lines_for_window(evaluator, mini_params, frame_rows);
                // GNU `resize_mini_window` reads `Vresize_mini_windows`
                // after `set_buffer_internal (XBUFFER (w->contents))`
                // (xdisp.c:13296,13318), so a buffer-local binding in the
                // mini-window's buffer takes effect. Read buffer-local-
                // then-global from that buffer, not the raw global.
                let resize_mode = resize_mini_windows_mode_for_buffer(
                    evaluator,
                    neovm_core::buffer::BufferId(mini_params.buffer_id),
                );

                // GNU `resize_mini_window` measures the mini-window's
                // CONTENT height via `move_it_to (ZV)` (xdisp.c:13340) and
                // shrinks a grow-only window when `height < old_height &&
                // (exact_p || BEGV == ZV)` (xdisp.c:13395). Empty accessible
                // source permits shrinking; overlay strings still contribute
                // their actual measured display height.
                //
                // The source was resolved before incremental classification,
                // so this height and emptiness check refer to the same buffer
                // that produced the current glyph rows.  This makes emitted
                // pixel geometry (including images, overlays, font height and
                // line spacing) authoritative, like GNU's display iterator.
                let buf_id = neovm_core::buffer::BufferId(mini_params.buffer_id);
                let visible_region_empty = evaluator
                    .buffer_manager()
                    .get(buf_id)
                    .map(|b| b.accessible_emacs_byte_range().is_empty())
                    .unwrap_or(true);

                let resize = evaluator
                    .frame_manager()
                    .get(frame_id)
                    .and_then(|frame| {
                        frame.plan_mini_window_resize(mini_content_height_px.get(), max_mini_lines)
                    })
                    .filter(|resize| {
                        if resize.is_growth() {
                            resize_mode.should_grow()
                        } else {
                            resize_mode.should_shrink(
                                evaluator.echo_area_resize_exact_pending(),
                                visible_region_empty,
                            )
                        }
                    });
                if let Some(resize) = resize {
                    // The native frame plan knows the mini's requested size,
                    // but only window.el knows what its root tree can yield.
                    // Reuse GNU's staged resize/commit rather than shrinking
                    // every child proportionally below its own minimum.
                    // This late sizing call may enter arbitrary Lisp. Keep
                    // the existing complete per-window witnesses local to
                    // this call, including the no-op/refused-resize case.
                    let before_sizing: Vec<_> = window_params_list
                        .iter()
                        .map(|params| {
                            let window = neovm_core::window::WindowId(params.window_id as u64);
                            let buffer = neovm_core::buffer::BufferId(params.buffer_id);
                            (
                                window,
                                buffer,
                                evaluator.window_layout_attempt_freshness(frame_id, window, buffer),
                            )
                        })
                        .collect();
                    let resize_outcome = match evaluator.apply_prepared_minibuffer_resize(
                        frame_id,
                        neovm_core::window::WindowId(mini_params.window_id as u64),
                        neomacs_display_protocol::types::Px(resize.height_px()),
                    ) {
                        Ok(outcome) => outcome,
                        Err(flow) => {
                            evaluator.defer_redisplay_hook_flow(flow);
                            frame_window_end_attempts.reject_all(evaluator);
                            evaluator.retire_interaction_presentation(presentation_id);
                            self.last_frame_display_state = None;
                            self.reset_frame_attempt_state();
                            return FrameLayoutInnerOutcome::Aborted {
                                settle_no_safe: false,
                            };
                        }
                    };
                    match resize_outcome {
                        neovm_core::window::PreparedMiniResizeOutcome::Unchanged => {
                            let changed =
                                before_sizing.iter().find_map(|(window, buffer, before)| {
                                    let after = evaluator.window_layout_attempt_freshness(
                                        frame_id, *window, *buffer,
                                    );
                                    (before.is_none() || *before != after).then_some(*window)
                                });
                            if let Some(window) = changed {
                                let request = FrameRelayoutRequest::LogicalInputsChanged {
                                    window_id: DisplayWindowId::new(window.0 as i64),
                                };
                                request_frame_relayout!('frame_layout, request);
                            }
                        }
                        neovm_core::window::PreparedMiniResizeOutcome::Committed => {
                            let required_height_px = evaluator
                                .frame_manager()
                                .get(frame_id)
                                .and_then(|frame| {
                                    frame.find_window(neovm_core::window::WindowId(
                                        mini_params.window_id as u64,
                                    ))
                                })
                                .map_or(resize.height_px(), |window| window.bounds().height);
                            let request = FrameRelayoutRequest::Minibuffer {
                                window_id: DisplayWindowId::new(mini_params.window_id),
                                allocated_height_px: resize.previous_height_px(),
                                required_height_px,
                            };
                            request_frame_relayout!('frame_layout, request);
                        }
                    }
                }
            }

            let frame_navigation_observation = self.render_frame_output_hints(
                &previous_visual_history,
                &curr_window_infos,
                pending_frame_navigation.map(|intent| intent.direction()),
            );
            let observed_frame_navigation = frame_navigation_observation
                .direction_to_acknowledge()
                .and_then(|direction| {
                    pending_frame_navigation.filter(|intent| intent.direction() == direction)
                });

            // Color advice may signal after a row has already produced fallback
            // paint. Preserve the original pending flow and reject speculative
            // rows before they can seal matrices, faces, or unchanged regions.
            if evaluator.has_mode_line_display_flow() || evaluator.redisplay_hook_flow_pending() {
                frame_window_end_attempts.reject_all(evaluator);
                evaluator.retire_interaction_presentation(presentation_id);
                self.last_frame_display_state = None;
                self.reset_frame_attempt_state();
                return FrameLayoutInnerOutcome::Aborted {
                    settle_no_safe: false,
                };
            }

            if attempt_mode == FrameAttemptMode::Converging && face_resolver.has_tty_realization() {
                let refreshed = evaluator.sync_runtime_faces_for_frame(frame_id);
                let after = evaluator.face_change_count();
                if refreshed || after != resolver_face_change_count {
                    resolver_refresh = FaceResolverRefresh::Required;
                    let request = FrameRelayoutRequest::FaceRealizationInputsChanged {
                        before: resolver_face_change_count,
                        after,
                    };
                    request_frame_relayout!('frame_layout, request);
                }
            }

            let current_topology_generation =
                evaluator.frame_manager().window_topology_generation();
            if current_topology_generation != attempt_topology_generation {
                let request = FrameRelayoutRequest::WindowTopologyChanged {
                    before: attempt_topology_generation,
                    after: current_topology_generation,
                };
                request_frame_relayout!('frame_layout, request);
            }

            // Later sibling chrome may change an earlier window's buffer
            // without changing topology. Sources may become stale, but a
            // completed body cannot be published for a different owner or
            // physical allocation. Validate every original body identity now.
            if attempt_mode == FrameAttemptMode::Converging
                && let Some(window_id) = curr_window_infos.keys().copied().find(|window| {
                    lisp_ledger
                        .completed_body_inputs
                        .get(window)
                        .is_some_and(|body| {
                            evaluator
                                .window_layout_attempt_freshness(
                                    frame_id,
                                    neovm_core::window::WindowId(window.get() as u64),
                                    body.source_buffer,
                                )
                                .is_none_or(|after| {
                                    !body.inputs.remains_valid_across(
                                        &after,
                                        neovm_core::window::WindowLayoutLispBoundary::WindowChrome,
                                    )
                                })
                        })
                })
            {
                request_frame_relayout!('frame_layout, FrameRelayoutRequest::LogicalInputsChanged { window_id });
            }

            let Some(current_smallest_font_height) = evaluator
                .frame_manager()
                .get(frame_id)
                .map(|frame| frame.layout_inputs().smallest_opened_font_height)
            else {
                frame_window_end_attempts.reject_all(evaluator);
                evaluator.retire_interaction_presentation(presentation_id);
                self.last_frame_display_state = None;
                self.reset_frame_attempt_state();
                return FrameLayoutInnerOutcome::Aborted {
                    settle_no_safe: purpose == LayoutPurpose::Redisplay,
                };
            };
            // `None` (no graphical connection) never changes during an
            // attempt, so a difference here always carries real heights; the
            // sentinel 1 only reports a pre-open graphical minimum.
            if attempt_mode == FrameAttemptMode::Converging
                && current_smallest_font_height != attempt_smallest_font_height
            {
                request_frame_relayout!('frame_layout, FrameRelayoutRequest::FontCapacityInputsChanged {
                    before: attempt_smallest_font_height.unwrap_or(1),
                    after: current_smallest_font_height.unwrap_or(1),
                });
            }

            let accepted_window_chrome_metrics: rustc_hash::FxHashMap<
                DisplayWindowId,
                WindowChromeMetrics,
            > = window_params_list
                .iter()
                .filter_map(|params| {
                    let window = DisplayWindowId::new(params.window_id);
                    window_chrome_metrics
                        .get(&window)
                        .copied()
                        .map(|metrics| (window, metrics))
                })
                .collect();
            // Frame chrome (menu / tool / tab bar) is not a window, so no
            // `(:window …)` `:filtered` predicate may match here. Clear the
            // per-window parameters left over from the window loop above (GNU
            // evaluates chrome faces with no window ⇒ such filters fail).
            face_resolver.set_current_window_parameters(Vec::new());
            face_resolver.set_current_window_id(None);

            // Position the already-evaluated GUI chrome before publishing the
            // frame. FrameChrome is the single owner of band ordering and absolute
            // placement; this phase is deliberately Lisp-free.
            // Keep the logical Lisp result available if a successful native
            // font open below requires another physical frame attempt.
            if let Some(PreparedGuiChromeSemantics {
                menu_items,
                tool_items,
                ..
            }) = prepared_gui_chrome.as_ref()
            {
                let pixel_to_color = |pixel: u32| -> Color {
                    Color::rgb(
                        ((pixel >> 16) & 0xFF) as f32 / 255.0,
                        ((pixel >> 8) & 0xFF) as f32 / 255.0,
                        (pixel & 0xFF) as f32 / 255.0,
                    )
                };
                if frame_params.compact_bar_height > 0.0 {
                    let menu_face = face_resolver.resolve_named_face_without_inverse_video("menu");
                    let tool_face = face_resolver
                        .resolve_fixed_face(neovm_core::emacs_core::intern::WellKnown::ToolBar);
                    let mut metrics = match self.font_metrics.as_mut() {
                        Some(service) => crate::gui_chrome::MenuHeadingMetrics::Pixels {
                            service,
                            face: &menu_face,
                        },
                        None => {
                            crate::gui_chrome::MenuHeadingMetrics::Cells(frame_params.char_width)
                        }
                    };
                    let content = layout_gui_compact_bar_content(
                        menu_items.clone(),
                        tool_items.clone(),
                        frame_params.width,
                        frame_params.compact_bar_height,
                        &mut metrics,
                        pixel_to_color(menu_face.fg),
                        pixel_to_color(menu_face.bg),
                        pixel_to_color(tool_face.fg),
                        pixel_to_color(tool_face.bg),
                    );
                    self.frame_output
                        .add_frame_chrome_band(ChromeBandRequest::new(
                            ProtocolFrameChromeKind::CompactBar,
                            frame_params.compact_bar_height,
                            FrameChromeContent::CompactBar(content),
                        ));
                } else {
                    if frame_params.menu_bar_height > 0.0 {
                        let face = face_resolver.resolve_named_face_without_inverse_video("menu");
                        let terminal_face = face_resolver
                            .resolve_fixed_face(neovm_core::emacs_core::intern::WellKnown::Menu);
                        // GNU's non-toolkit `display_menu_bar` uses a SCHARS + 1
                        // field. Preserve that terminal cell policy. GUI headings
                        // follow GTK's font-measured labels instead; an ASCII cell
                        // width cannot determine either their text or hit rectangles.
                        let menu_h_padding = if window_system.is_some() {
                            crate::gui_chrome::GUI_CHROME_HORIZONTAL_PADDING
                        } else {
                            frame_params.char_width * 0.5
                        };
                        let mut metrics =
                            match (window_system.is_some(), self.font_metrics.as_mut()) {
                                (true, Some(service)) => {
                                    crate::gui_chrome::MenuHeadingMetrics::Pixels {
                                        service,
                                        face: &face,
                                    }
                                }
                                _ => crate::gui_chrome::MenuHeadingMetrics::Cells(
                                    frame_params.char_width,
                                ),
                            };
                        let content = layout_gui_menu_bar_content(
                            menu_items.clone(),
                            frame_params.width,
                            frame_params.menu_bar_height,
                            &mut metrics,
                            menu_h_padding,
                            pixel_to_color(face.fg),
                            pixel_to_color(face.bg),
                        )
                        .with_terminal_style(TerminalMenuBarStyle {
                            fg: (!terminal_face.use_default_foreground)
                                .then_some(terminal_face.terminal_fg)
                                .flatten(),
                            bg: (!terminal_face.use_default_background)
                                .then_some(terminal_face.terminal_bg)
                                .flatten(),
                            bold: terminal_face.font_weight >= 600,
                            inverse: terminal_face.terminal_inverse_video,
                        });
                        self.frame_output
                            .add_frame_chrome_band(ChromeBandRequest::new(
                                ProtocolFrameChromeKind::MenuBar,
                                frame_params.menu_bar_height,
                                FrameChromeContent::MenuBar(content),
                            ));
                    }
                    if frame_params.tool_bar_height > 0.0 {
                        let face = face_resolver
                            .resolve_fixed_face(neovm_core::emacs_core::intern::WellKnown::ToolBar);
                        let content = layout_gui_tool_bar_content(
                            tool_items.clone(),
                            frame_params.width,
                            frame_params.tool_bar_height,
                            pixel_to_color(face.fg),
                            pixel_to_color(face.bg),
                        );
                        self.frame_output
                            .add_frame_chrome_band(ChromeBandRequest::new(
                                ProtocolFrameChromeKind::ToolBar,
                                frame_params.tool_bar_height,
                                FrameChromeContent::ToolBar(content),
                            ));
                    }
                }
            }

            // Publication requires the exclusive table owner. The output
            // builder drops its own handle; release the last producer here.
            drop(face_attempt);
            let (frame_display_state, face_seal) =
                match self.finish_frame_output(&frame_params, face_table) {
                    Ok(output) => output,
                    Err(error) => {
                        tracing::error!(%error, "rejecting invalid frame chrome snapshot");
                        frame_window_end_attempts.reject_all(evaluator);
                        evaluator.retire_interaction_presentation(presentation_id);
                        return FrameLayoutInnerOutcome::Aborted {
                            settle_no_safe: purpose == LayoutPurpose::Redisplay,
                        };
                    }
                };
            let Some(current_smallest_font_height) = evaluator
                .frame_manager()
                .get(frame_id)
                .map(|frame| frame.layout_inputs().smallest_opened_font_height)
            else {
                frame_window_end_attempts.reject_all(evaluator);
                evaluator.retire_interaction_presentation(presentation_id);
                self.last_frame_display_state = None;
                self.reset_frame_attempt_state();
                return FrameLayoutInnerOutcome::Aborted {
                    settle_no_safe: purpose == LayoutPurpose::Redisplay,
                };
            };
            // `None` (no graphical connection) never changes during an
            // attempt, so a difference here always carries real heights; the
            // sentinel 1 only reports a pre-open graphical minimum.
            if attempt_mode == FrameAttemptMode::Converging
                && current_smallest_font_height != attempt_smallest_font_height
            {
                request_frame_relayout!('frame_layout, FrameRelayoutRequest::FontCapacityInputsChanged {
                    before: attempt_smallest_font_height.unwrap_or(1),
                    after: current_smallest_font_height.unwrap_or(1),
                });
            }

            // The body rows are already certified. A source-only fontifier
            // disagreement retries the whole frame while the budget remains.
            // Spending the budget must not retire this presentation: the
            // ordinary tail below publishes the attempt just assembled.
            if attempt_mode == FrameAttemptMode::Converging
                && let Some(window_id) = lisp_ledger.source_only_retry.take()
            {
                let request = FrameRelayoutRequest::SourceOnlyRetry { window_id };
                if layout_coordinator.request_retry(request).is_ok() {
                    continue 'frame_layout;
                }
            }

            if attempt_mode == FrameAttemptMode::ExhaustionSnapshot {
                // Runtime guard (N3): every known Lisp seam is skipped during
                // ExhaustionSnapshot, except TTY face realization advice under
                // `realization.rs:777`. If unexpected Lisp mutated face definitions
                // or window topology during the snapshot pass, refuse to publish
                // a half-mutated frame and settle NoSafe cleanly.
                let face_epoch_moved = evaluator.face_change_count() != frame_face_epoch.get();
                let topology_moved = evaluator.frame_manager().window_topology_generation()
                    != attempt_topology_generation;
                if face_epoch_moved
                    || topology_moved
                    || evaluator.has_mode_line_display_flow()
                    || evaluator.redisplay_hook_flow_pending()
                {
                    tracing::error!(
                        presentation = presentation_id,
                        face_epoch_moved,
                        topology_moved,
                        "rejecting exhaustion snapshot after unexpected state mutation"
                    );
                    frame_window_end_attempts.reject_all(evaluator);
                    evaluator.retire_interaction_presentation(presentation_id);
                    self.last_frame_display_state = None;
                    self.reset_frame_attempt_state();
                    let flow_pending = evaluator.has_mode_line_display_flow()
                        || evaluator.redisplay_hook_flow_pending();
                    return FrameLayoutInnerOutcome::Aborted {
                        settle_no_safe: !flow_pending && purpose == LayoutPurpose::Redisplay,
                    };
                }
            }

            break (
                frame_params,
                curr_window_infos,
                retained_keys,
                accepted_window_chrome_metrics,
                frame_display_state,
                face_seal,
                observed_window_navigation_intents,
                observed_frame_navigation,
            );
        };

        // xterm.c reads Vx_scroll_event_delta_factor from the C global in
        // the event path (xterm.c:22802); a live `let` is visible.
        frame_display_state.scroll_input_policy.x11_delta_factor =
            crate::neovm_bridge::visible_value(
                evaluator,
                crate::fixed_symbols::X_SCROLL_EVENT_DELTA_FACTOR.id(),
            )
            .and_then(|value| value.as_number_f64())
            .filter(|factor| factor.is_finite())
            .unwrap_or(1.0);
        let sealed_face_arena = match face_seal.seal(&frame_display_state.faces, frame_face_epoch) {
            Ok(arena) => arena,
            Err(error) => {
                tracing::error!(?error, "rejecting incoherent frame face namespace");
                frame_window_end_attempts.reject_all(evaluator);
                evaluator.retire_interaction_presentation(presentation_id);
                return FrameLayoutInnerOutcome::Aborted {
                    settle_no_safe: purpose == LayoutPurpose::Redisplay,
                };
            }
        };

        // Embed the user-defined fringe bitmaps once per frame so the renderer
        // can expand any `GlyphRow::left_fringe_bitmap` reference (magit section
        // heading fold arrows). GC-safe: copied out as plain `u16`/`u8` data.
        for (index, bitmap) in evaluator.fringe_bitmap_registry().iter_indexed() {
            if index > u32::from(u16::MAX) {
                continue;
            }
            frame_display_state.fringe_bitmaps.insert(
                index as u16,
                neomacs_display_protocol::frame_glyphs::FringeBitmapData {
                    bits: bitmap.bits.clone(),
                    width: bitmap.width,
                    height: bitmap.height,
                    period: bitmap.period,
                    align: bitmap.align.as_u8(),
                },
            );
        }

        if window_system.is_none()
            && evaluator
                .frame_manager()
                .posn_object_extent_mode(frame_id)
                .enabled()
        {
            posn_object_extent::capture_terminal_object_extents(
                &frame_display_state,
                &mut self.window_snapshots,
            );
        }

        // NOTE: GlyphMatrix vs FrameGlyphBuffer character count validation removed.
        // FrameGlyphBuffer no longer receives glyph output; the DisplayOutputBuilder
        // is now the sole output path.

        // --- Incremental-layout commit (Phase 0a) ---
        //
        // Populate the relaid-row-count gate metric and retain each accepted
        // window's matrix. We are past the accepted `break`, so this never runs
        // on a resize-retry `continue`. Phase 0a always full-rebuilds: every
        // enabled row is `relaid`, every window is classified `Full`, and the
        // retained matrices are written but NOT read (no fast path exists yet).
        let (mut next_layout_stats, next_retained_window_matrices, acked_buffer_ids) = {
            let key_map: rustc_hash::FxHashMap<DisplayWindowId, RetainedWindowKey> =
                retained_keys.into_iter().collect();
            let frame_state = &mut frame_display_state;
            let mut next_layout_stats = LayoutStats {
                exhaustion_snapshot_passes: usize::from(
                    attempt_mode == FrameAttemptMode::ExhaustionSnapshot,
                ),
                prepass_freshness_rechecks: lisp_ledger.prepass_freshness_rechecks,
                chrome_exhaustion_captures: lisp_ledger.captured_chrome_outputs.captures(),
                ..LayoutStats::default()
            };
            let mut retained: rustc_hash::FxHashMap<DisplayWindowId, RetainedWindowMatrix> =
                rustc_hash::FxHashMap::default();
            let presented_cursors: rustc_hash::FxHashMap<DisplayWindowId, PhysCursor> = frame_state
                .window_matrices
                .iter()
                .filter_map(|entry| {
                    frame_state
                        .presented_cursor_for_window(entry.window_id)
                        .map(|cursor| (entry.window_id, cursor))
                })
                .collect();
            // What each window's chrome generation established this layout.
            // Windows absent from this map SKIPPED their chrome, which is
            // exactly the distinction the dirty-flag acknowledgement below and
            // the `chrome_uses_column` carry-forward both turn on.
            let chrome_generation: rustc_hash::FxHashMap<i64, _> =
                crate::display_status_line::chrome_generation_record()
                    .into_iter()
                    .collect();
            let mini_windows_stand_still = crate::incremental_layout::mini_window_still_enabled();
            let mini_windows: rustc_hash::FxHashSet<DisplayWindowId> = frame_state
                .window_infos
                .iter()
                .filter(|info| info.is_minibuffer)
                .map(|info| info.window_id)
                .collect();
            for entry in &mut frame_state.window_matrices {
                let window_id = entry.window_id;
                let cursor_only = self.cursor_only_window_ids.contains(&window_id);
                let scroll_reused = self.scroll_window_ids.get(&window_id).cloned();
                let edit_reused = self.edit_window_ids.get(&window_id).cloned();
                // Fast paths classify body vs chrome by ROLE (they reuse the
                // buffer-text `Text` rows and re-walk all chrome roles); a full
                // rebuild counts by the `mode_line` flag (the Phase 0a baseline).
                let role_based = cursor_only || scroll_reused.is_some() || edit_reused.is_some();
                let mut enabled_body = 0usize;
                let mut enabled_chrome = 0usize;
                for row in &entry.matrix.rows {
                    if !row.enabled {
                        continue;
                    }
                    let is_chrome = if role_based {
                        RetainedWindowMatrix::is_chrome_role(row.role)
                    } else {
                        row.mode_line
                    };
                    if is_chrome {
                        enabled_chrome += 1;
                    } else {
                        enabled_body += 1;
                    }
                }
                next_layout_stats.relaid_chrome_rows += enabled_chrome;
                let is_mini_window = mini_windows.contains(&window_id);
                if is_mini_window && cursor_only {
                    // A mini-window standing still is its own class (the
                    // content windows' class counts keep meaning what they
                    // did); its row is reused like any other.
                    next_layout_stats.mini_window_still += 1;
                    next_layout_stats.reused_rows += enabled_body;
                } else if cursor_only {
                    // Body rows were reused verbatim (0 relaid); chrome re-walked.
                    next_layout_stats.reused_rows += enabled_body;
                    if self.prepared_window_ids.contains(&window_id) {
                        next_layout_stats.prepared_windows += 1;
                    }
                    next_layout_stats.record_window_class(window_id, LayoutClass::CursorOnly);
                } else if let Some((ref reused, _dvpos)) = scroll_reused {
                    // Overlapping rows reused shifted; the rest were newly exposed
                    // and walked.
                    let reused = reused.len().min(enabled_body);
                    next_layout_stats.reused_shifted_rows += reused;
                    next_layout_stats.relaid_body_rows += enabled_body - reused;
                    if self.prepared_window_ids.contains(&window_id) {
                        next_layout_stats.prepared_windows += 1;
                    }
                    next_layout_stats.record_window_class(window_id, LayoutClass::Scroll);
                } else if let Some(ref reused) = edit_reused {
                    // Rows outside the regenerated edit span reused verbatim;
                    // under the edit sync the rows below a span that changed
                    // height are reused shifted.
                    let shifted = reused.shifted_len();
                    let reused = reused.len().min(enabled_body);
                    next_layout_stats.reused_rows += reused.saturating_sub(shifted);
                    next_layout_stats.reused_shifted_rows += shifted.min(reused);
                    next_layout_stats.relaid_body_rows += enabled_body - reused;
                    next_layout_stats.record_window_class(window_id, LayoutClass::Edit);
                } else {
                    next_layout_stats.relaid_body_rows += enabled_body;
                    next_layout_stats.record_window_class(window_id, LayoutClass::Full);
                }

                // Phase 5 (#44) per-row provenance. The fast paths reuse the
                // exact reused matrix-row identities; chrome + disabled +
                // relaid body rows are `New`.
                {
                    for idx in 0..entry.matrix.rows.len() {
                        let row = &entry.matrix.rows[idx];
                        let is_chrome = if role_based {
                            RetainedWindowMatrix::is_chrome_role(row.role)
                        } else {
                            row.mode_line
                        };
                        // Prepared coverage may come from an older viewport,
                        // not the consumer's immediately preceding matrix.
                        // Its rows save layout work, but their old provenance
                        // cannot authorize skipping paint in the current scene.
                        if !row.enabled
                            || is_chrome
                            || self.prepared_window_ids.contains(&window_id)
                        {
                            entry.matrix.set_row_damage(idx, RowDamage::New);
                            continue;
                        }
                        let d = if cursor_only {
                            RowDamage::Reused
                        } else if let Some((ref reused, dvpos)) = scroll_reused {
                            if reused.contains(idx) {
                                // A backward scroll records its shift with
                                // the rows (P3.5 G3); a forward one plans it.
                                RowDamage::ReusedShifted {
                                    dvpos: Px(reused.shift_of(idx).unwrap_or(dvpos)),
                                }
                            } else {
                                RowDamage::New
                            }
                        } else if let Some(ref reused) = edit_reused {
                            if !reused.contains(idx) {
                                RowDamage::New
                            } else if let Some(dy) = reused.shift_of(idx) {
                                RowDamage::ReusedShifted { dvpos: Px(dy) }
                            } else {
                                RowDamage::Reused
                            }
                        } else {
                            RowDamage::New
                        };
                        entry.matrix.set_row_damage(idx, d);
                    }
                }

                // Do not retain degraded matrices from a callback-inhibited
                // ExhaustionSnapshot pass: the next real redisplay must rebuild
                // cleanly with callbacks enabled instead of reusing degraded rows
                // or chrome_memo fingerprints.
                if attempt_mode == FrameAttemptMode::ExhaustionSnapshot {
                    continue;
                }
                // Probe-pass exclusion: a window that laid out <=1 enabled row
                // is the scroll-off hazard (spec §4.1); never retain it as a
                // clean reusable matrix. The mini-window is one row by nature,
                // not by a probe (see Phase A).
                if enabled_body + enabled_chrome <= 1
                    && !(mini_windows_stand_still && is_mini_window)
                {
                    continue;
                }
                if let Some(key) = key_map.get(&window_id) {
                    // The window's display snapshot (point-independent body rows
                    // + per-span display points) is needed to replay this window
                    // on a later cursor-only pass. A retained window without one
                    // cannot be reused, so skip retention if it is missing.
                    let Some(display_snapshot) = self
                        .window_snapshots
                        .iter()
                        .map(WindowPresentationSnapshot::shared_display_snapshot)
                        .find(|snapshot| snapshot.window_id.0 as i64 == entry.window_id.get())
                        .cloned()
                    else {
                        continue;
                    };
                    let displayed_key = key_with_displayed_window_start(
                        key,
                        &entry.matrix,
                        evaluator
                            .frame_manager()
                            .get(frame_id)
                            .and_then(|frame| {
                                frame.find_window(neovm_core::window::WindowId(
                                    window_id.get() as u64
                                ))
                            })
                            .and_then(neovm_core::window::Window::window_start)
                            .map(crate::coords::lisp_char_pos_to_layout_i64),
                    );
                    retained.insert(
                        window_id,
                        RetainedWindowMatrix {
                            matrix: entry.matrix.clone(),
                            key: displayed_key,
                            // Filled from the same accepted post-callback stamp
                            // as the window's normal redisplay output below.
                            gnu_accuracy: None,
                            // Every enabled body row now carries real
                            // MATRIX_ROW_START/END_CHARPOS values (empty lines
                            // hold their line's position; the EOB placeholder
                            // holds ZV), so both the scroll and edit fast paths
                            // commit a canonical matrix that first_dirty/reuse
                            // reads without the old (0, 0)-sentinel confusion.
                            // A committed matrix is therefore always reusable.
                            validity: MatrixValidity::Valid,
                            display_snapshot,
                            presented_cursor: presented_cursors.get(&window_id).cloned(),
                            face_generation: sealed_face_arena.generation(),
                            // A window that SKIPPED its chrome this frame has
                            // no generation record, and its retained chrome is
                            // the same chrome as last frame — so carry the
                            // previous answer forward rather than defaulting to
                            // "no column", which would let a `%c` format start
                            // being reused after one skip.
                            chrome_uses_column: chrome_generation
                                .get(&window_id.get())
                                .map(|record| record.uses_column)
                                .unwrap_or_else(|| {
                                    self.retained_window_matrices
                                        .get(&window_id)
                                        .is_some_and(|prev| prev.chrome_uses_column)
                                }),
                            // GNU `w->last_had_star`. Read fresh rather than
                            // carried forward: a window that skipped chrome
                            // kept the flag its chrome was generated with, and
                            // that is exactly what this records, because the
                            // skip required the two to be equal.
                            chrome_modified_flag: evaluator
                                .buffer_manager()
                                .get(neovm_core::buffer::BufferId(key.buffer_id))
                                .is_some_and(|buffer| buffer.is_modified()),
                            // P3.5 E2: what this chrome was rendered from. A
                            // window that skipped its chrome keeps the rows,
                            // so it keeps their fingerprints too.
                            chrome_fingerprints: if chrome_generation
                                .contains_key(&window_id.get())
                            {
                                crate::incremental_layout::chrome_memo::chrome_memo_mode()
                                    .enabled()
                                    .then(|| {
                                        crate::incremental_layout::chrome_memo::recorded_fingerprints(
                                            window_id.get(),
                                        )
                                    })
                                    .flatten()
                            } else {
                                self.retained_window_matrices
                                    .get(&window_id)
                                    .and_then(|prev| prev.chrome_fingerprints.clone())
                            },
                        },
                    );
                }
            }
            // Phase 3 redisplay ACK: reset each laid-out buffer's unchanged-region
            // accumulator at the committed (accepted) break — NEVER on a
            // retry/`continue` (which would under-invalidate, spec §6). From here
            // the accumulated dirty span is the edits the NEXT frame must relay.
            let acked_buffer_ids: rustc_hash::FxHashSet<u64> =
                key_map.values().map(|key| key.buffer_id).collect();
            (next_layout_stats, retained, acked_buffer_ids)
        };

        // Read at the moment of sealing, so the stamp describes the
        // presentation actually being published: whether the user's hand is
        // currently placing this geometry is what decides whether the
        // compositor may animate toward it.
        frame_display_state.origin = evaluator.presentation_origin();
        frame_display_state.input_checkpoint = input_checkpoint;
        self.prepared_viewports.export(
            frame_id,
            &next_retained_window_matrices,
            &mut next_layout_stats,
            evaluator
                .frame_manager()
                .get(frame_id)
                .and_then(|frame| frame.active_presentation())
                .map(|id| neomacs_display_protocol::PresentationId::new(id.get())),
            &mut frame_display_state,
            &sealed_face_arena,
            &mut self.font_metrics,
            |window| {
                let window = neovm_core::window::WindowId(window.get() as u64);
                (
                    evaluator.compositor_scrolling_enabled(window),
                    evaluator.permits_compositor_pixel_scroll(window),
                )
            },
        );
        for coverage in &frame_display_state.scroll_coverage {
            let window = neovm_core::window::WindowId(coverage.content.window_id.get() as u64);
            tracing::debug!(target: "neomacs_layout_engine::scroll_coverage",
                predict_pixels = coverage.predict_pixels, window = window.0,
                epoch = coverage.epoch, anchor_row = coverage.anchor_row, origin = coverage.origin,
                source_first = ?coverage.content.matrix.rows.first().map(|row| row.start_charpos),
                source_last = ?coverage.content.matrix.rows.last().map(|row| row.end_charpos),
                text_bounds = ?coverage.content.text_clip_bounds,
                "exported compositor coverage");
        }
        let resolved = match crate::frame_presentation::ResolvedFrame::new(frame_display_state) {
            Ok(resolved) => resolved,
            Err(error) => {
                tracing::error!(?error, "rejecting incoherent resolved frame");
                frame_window_end_attempts.reject_all(evaluator);
                evaluator.retire_interaction_presentation(presentation_id);
                return FrameLayoutInnerOutcome::Aborted {
                    settle_no_safe: purpose == LayoutPurpose::Redisplay,
                };
            }
        };
        let presentation_inputs = crate::frame_presentation::PresentationInputs::new(
            &self.window_snapshots,
            frame_params.zero_width_vertical_border_edge,
        )
        .with_tab_bar_pointer(self.pending_tab_bar_pointer.take());
        let sealed = match crate::frame_presentation::PresentationComposer::compose(
            resolved,
            presentation_inputs,
        ) {
            Ok(sealed) => sealed,
            Err(error) => {
                tracing::error!(?error, "rejecting invalid frame presentation");
                frame_window_end_attempts.reject_all(evaluator);
                evaluator.retire_interaction_presentation(presentation_id);
                return FrameLayoutInnerOutcome::Aborted {
                    settle_no_safe: purpose == LayoutPurpose::Redisplay,
                };
            }
        };
        debug_assert_eq!(sealed.presentation(), sealed.state().presentation_id);

        // Commit retained state only after the visual, spatial, and revision
        // invariants have sealed. A rejected presentation cannot acknowledge
        // buffer edits or replace the GNU "current matrix" analogue.
        if self.retained_window_chrome_metrics != accepted_window_chrome_metrics {
            self.query_cache.clear();
        }
        self.retained_window_chrome_metrics = accepted_window_chrome_metrics;
        self.layout_stats = next_layout_stats;
        // Admitted work: what the frame SPENT to decide, not what it emitted.
        let (snapshots, composition_bytes, reused_chrome) =
            crate::neovm_bridge::take_snapshot_work();
        self.layout_stats.buffer_snapshots_built = snapshots;
        self.layout_stats.composition_bytes_scanned = composition_bytes;
        self.layout_stats.buffer_text_cow_copies =
            neovm_core::buffer::text_snapshot::take_buffer_text_cow_copies();
        self.layout_stats.reused_chrome_rows = reused_chrome;
        self.layout_stats.chrome_memo_hits =
            crate::incremental_layout::chrome_memo::take_chrome_memo_hits();
        // `relaid_chrome_rows` accumulated EVERY enabled chrome row above,
        // reused or not. Reused rows were never walked, so take them back out.
        self.layout_stats.relaid_chrome_rows = self
            .layout_stats
            .relaid_chrome_rows
            .saturating_sub(reused_chrome);
        // Per-frame incremental-layout observability: append one line per
        // accepted frame when NEOMACS_LAYOUT_STATS_FILE names a path. This is
        // the only consumer-facing view of LayoutStats; the TTY typing
        // harness uses it to verify which fast path actually engaged.
        if let Some(stats_path) = diagnostic_knobs::layout_stats_path() {
            use std::io::Write as _;
            let s = &self.layout_stats;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(stats_path)
            {
                let overlay_stats = neovm_core::buffer::overlay::take_overlay_layout_stats();
                let _ = writeln!(
                    f,
                    "full={} cursor_only={} scroll={} edit={} relaid_body={} relaid_chrome={} reused={} reused_shifted={} reused_chrome={} snapshots={} compose_bytes={} text_cow_copies={} mini_still={} chrome_memo={} prepared={} {}",
                    s.full_windows,
                    s.cursor_only_windows,
                    s.scroll_windows,
                    s.edit_windows,
                    s.relaid_body_rows,
                    s.relaid_chrome_rows,
                    s.reused_rows,
                    s.reused_shifted_rows,
                    s.reused_chrome_rows,
                    s.buffer_snapshots_built,
                    s.composition_bytes_scanned,
                    s.buffer_text_cow_copies,
                    s.mini_window_still,
                    s.chrome_memo_hits,
                    s.prepared_windows,
                    neovm_core::buffer::overlay::format_overlay_layout_stats(overlay_stats),
                );
            }
        }
        // Row-route coverage telemetry (NEOMACS_ROW_ROUTE_STATS_FILE): one
        // CUMULATIVE counters line per accepted frame; aggregation takes the
        // last line per pid.
        crate::buffer_source::row_route::route_stats_append_report();
        // Wholesale, and correct because these maps hold exactly this frame's
        // windows -- `load_retained_frame` saw to that. Windows this frame
        // deleted are pruned by the replacement, which is what it is for.
        self.prepared_viewports.accept(
            frame_id,
            std::mem::take(&mut self.retained_window_matrices),
            &next_retained_window_matrices,
            self.frame_face_arenas.get(&frame_id),
            self.frame_visual_histories.get(frame_id),
        );
        self.retained_window_matrices = next_retained_window_matrices;
        // Composition advances are inserted per window while the attempt
        // runs, including attempts that later retry and windows a frame
        // switch removed. The accepted matrices are exactly this frame's
        // windows, so prune to them here: a deleted window must not leave its
        // shaped widths (and its buffer's roots) behind.
        prune_retained_composition_advances(&mut self.retained_composition_advances, |window| {
            self.retained_window_matrices.contains_key(window)
        });
        self.frame_face_arenas.insert(frame_id, sealed_face_arena);
        for buffer_id in acked_buffer_ids {
            if let Some(buffer) = evaluator
                .buffer_manager()
                .get(neovm_core::buffer::BufferId(buffer_id))
            {
                buffer.reset_unchanged_region();
            }
        }
        neomacs_display_protocol::input_latency::sealed(frame_id.0, sealed.presentation(), || {
            evaluator.input_latency_viewport(frame_id.0)
        });
        self.scroll_preview_coverage.refresh(&sealed);
        self.last_frame_display_state = Some(sealed);
        // Acknowledge the chrome dirty flag for exactly the windows whose
        // chrome this layout GENERATED — GNU's `mark_window_display_accurate_1`.
        // A blanket clear (what P5.2(a) did, correct while nothing skipped)
        // would now eat two kinds of outstanding flag: a window that skipped
        // its chrome this frame, and every window on a frame this layout never
        // visited.
        for (window_id, _) in crate::display_status_line::chrome_generation_record() {
            evaluator.note_chrome_generated(neovm_core::window::WindowId(window_id as u64));
        }

        // The accepted `when` dependency record per window, committed with
        // the sealed presentation exactly like the retained matrices.  The
        // chrome-dirty generation is frame-wide: one value describes every
        // window the sealed pass laid out.
        let display_when_generation = evaluator.chrome_dirty().all_generation();
        let mut accepted_display_when: rustc_hash::FxHashMap<
            DisplayWindowId,
            crate::display_when::DisplayWhenRetention,
        > = lisp_ledger
            .display_when
            .settled_entries()
            .map(|(window, record)| {
                (
                    DisplayWindowId::new(window.0 as i64),
                    crate::display_when::DisplayWhenRetention {
                        record,
                        mode_line_generation: display_when_generation,
                    },
                )
            })
            .collect();
        // Exactly the windows this sealed presentation committed (the same
        // set the retained-matrix commit writes); a window that settled but
        // did not reach the presentation is not evidence.
        accepted_display_when.retain(|window, _| curr_window_infos.contains_key(window));
        let mut cursor_sources = std::mem::take(&mut lisp_ledger.cursor_sources);
        cursor_sources.retain(|window, _| curr_window_infos.contains_key(window));
        self.frame_visual_histories
            .retain_live(|frame| evaluator.frame_manager().get(frame).is_some());
        self.frame_visual_histories.commit(
            frame_id,
            FrameVisualHistory::from_accepted_presentation(
                curr_window_infos,
                accepted_display_when,
            )
            .with_cursor_sources(cursor_sources),
        );
        {
            let frame_manager = evaluator.frame_manager_mut();
            for (window_id, intent) in accepted_window_navigation_intents {
                frame_manager.acknowledge_window_navigation_intent(window_id, intent);
            }
            if let Some(intent) = accepted_frame_navigation_intent {
                frame_manager.acknowledge_frame_navigation_intent(frame_id, intent);
            }
        }

        if layout_walk_purpose == WindowLayoutWalkPurpose::Snapshot {
            // Frame preparation also commits live output and window ends.
            // A renderer-inert snapshot owns only its fresh geometry.
            for snapshot in &mut self.window_snapshots {
                snapshot.retain_as_geometry_only();
            }
        }

        // Fringe bitmaps are stamped onto matrix rows after the row walk has
        // pushed their snapshot rows, so pair the two up now that both are
        // final — this is what makes `fringe-bitmaps-at-pos` readable from the
        // evaluator at all.
        if let Some(sealed) = self.last_frame_display_state.as_ref() {
            crate::fringe_snapshot::publish_row_fringe_bitmaps(
                &sealed.window_matrices,
                &mut self.window_snapshots,
            );
        }

        // The sealed producer contains final New/Reused/Shifted row operations,
        // including the final chrome adjustments. Queries never reach this seam.
        let accepted_tty_posn_pool = if window_system.is_none()
            && evaluator
                .frame_manager()
                .posn_object_extent_mode(frame_id)
                .enabled()
        {
            let previous = evaluator
                .frame_manager()
                .get(frame_id)
                .and_then(|frame| frame.tty_posn_pool());
            let captured =
                neomacs_display_protocol::posn_frame_pool::PosnFramePool::from_terminal_frame(
                    self.last_frame_display_state
                        .as_ref()
                        .expect("accepted sealed state")
                        .state(),
                    previous.map(|pool| &**pool),
                    neovm_core::encoding::char_width,
                );
            Some(
                match previous.filter(|pool| captured.retains_same_observations(pool)) {
                    Some(pool) => std::sync::Arc::clone(pool),
                    None => std::sync::Arc::new(captured),
                },
            )
        } else {
            None
        };
        // GNU marks current matrices accurate after the last callback. This
        // does not recapture the source inputs retained with the produced rows.
        let accepted_accuracy: Vec<_> = self
            .window_snapshots
            .iter()
            .filter_map(|publication| {
                let WindowPresentationSnapshot::LiveWindow(snapshot) = publication else {
                    return None;
                };
                let window = snapshot.window_id;
                let buffer = lisp_ledger
                    .completed_body_inputs
                    .get(&DisplayWindowId::new(window.0 as i64))
                    .map(|body| body.source_buffer)
                    .or_else(|| {
                        evaluator
                            .frame_manager()
                            .get(frame_id)?
                            .find_window(window)?
                            .buffer_id()
                    })?;
                Some((
                    window,
                    evaluator.gnu_window_accuracy(frame_id, window, buffer)?,
                ))
            })
            .collect();
        let snapshots = std::mem::take(&mut self.window_snapshots);
        if let Some(frame) = evaluator.frame_manager_mut().get_mut(frame_id) {
            frame
                .prepare_display_presentation_with_tty_posn_pool(
                    neovm_core::window::geometry::PresentationId::new(presentation_id),
                    snapshots,
                    accepted_tty_posn_pool,
                )
                .expect("layout presentation identity is fresh");
            // Body dimensions are now authoritative before renderer activation.
            // Failed prepare and missing-frame paths cannot acknowledge output.
            // On ExhaustionSnapshot, retained_window_matrices is intentionally
            // empty (no cache poisoning), so acknowledge the laid-out windows
            // from curr_window_infos instead.
            if attempt_mode == FrameAttemptMode::ExhaustionSnapshot {
                let accepted_windows: Vec<_> = self
                    .last_frame_display_state
                    .as_ref()
                    .map(|sealed| {
                        sealed
                            .state()
                            .window_infos
                            .iter()
                            .map(|info| neovm_core::window::WindowId(info.window_id.get() as u64))
                            .collect()
                    })
                    .unwrap_or_default();
                evaluator.note_gnu_frame_display_accepted(frame_id, accepted_windows);
            } else {
                evaluator.note_gnu_frame_display_accepted(
                    frame_id,
                    self.retained_window_matrices
                        .keys()
                        .map(|window| neovm_core::window::WindowId(window.get() as u64)),
                );
            }
        }
        for (window, accuracy) in accepted_accuracy {
            let stamped = evaluator
                .frame_manager_mut()
                .get_mut(frame_id)
                .and_then(|frame| frame.find_window_mut(window))
                .is_some_and(|window| {
                    window.set_gnu_redisplay_accuracy(
                        neovm_core::window::geometry::PresentationId::new(presentation_id),
                        accuracy,
                    )
                });
            if stamped {
                if let Some(buffer) = evaluator.buffer_manager().get(accuracy.buffer_id) {
                    buffer.acknowledge_mode_line_clip();
                }
                if let Some(retained) = self
                    .retained_window_matrices
                    .get_mut(&DisplayWindowId::new(window.0 as i64))
                {
                    retained.gnu_accuracy = Some(accuracy);
                }
            }
        }
        frame_window_end_attempts.accept_all();
        log_invalid_face_references(evaluator, &face_resolver.take_invalid_face_references());
        FrameLayoutInnerOutcome::Prepared(None)
    }

    /// GNU resize_mini_window's full, renderer-inert BEGV-to-ZV walk. The
    /// exclusive presentation engine is idle before window-change hooks; this
    /// query retires speculative rows and never seals or paints a frame.
    pub fn prepare_minibuffer_geometry(
        &mut self,
        evaluator: &mut neovm_core::emacs_core::Context,
        request: neovm_core::emacs_core::eval::RedisplayMiniGeometryRequest,
    ) -> Result<neovm_core::emacs_core::value::Value, neovm_core::emacs_core::error::Flow> {
        use neovm_core::buffer::LispCharPos1;
        let Some(frame) = evaluator.frame_manager().get(request.frame) else {
            return Ok(Value::NIL);
        };
        let Some(_window) = frame.find_window(request.window) else {
            return Ok(Value::NIL);
        };
        if frame.minibuffer_window != Some(request.window) {
            return Ok(Value::NIL);
        }
        let mini_only = frame.root_window().id() == request.window;
        let char_height = frame.char_height.max(1.0);
        let old_height = _window.bounds().height;
        let inner_height = frame.root_window().bounds().height
            + if mini_only {
                0.0
            } else {
                _window.bounds().height
            };
        let frame_rows = inner_height / char_height;
        let begv = evaluator
            .buffer_manager()
            .get(request.buffer)
            .map(|buffer| {
                LispCharPos1::from_one_based_usize(
                    buffer.point_min_char_pos().get().saturating_add(1),
                )
            })
            .unwrap_or(LispCharPos1::ONE);
        let adhoc = uses_adhoc_minibuffer_resize_scroll(evaluator, request.buffer);
        // GNU resets the default start before the nil-policy early return.
        if adhoc {
            evaluator.gnu_set_prepared_minibuffer_start(request.frame, request.window, begv, false);
        }
        if mini_only {
            return evaluator.gnu_resize_prepared_mini_frame(request.frame);
        }
        let mode = resize_mini_windows_mode_for_buffer(evaluator, request.buffer);
        if !mode.should_grow() {
            return Ok(Value::NIL);
        }
        let raw_maximum = crate::neovm_bridge::visible_display_value(
            evaluator,
            request.buffer,
            crate::fixed_symbols::MAX_MINI_WINDOW_HEIGHT.id(),
        )
        .unwrap_or_else(|| Value::make_float(0.25));
        let max_lines = max_mini_window_lines_from_value(raw_maximum, frame_rows);
        self.mini_preparation_height = None;
        self.query_body_cache_state = QueryBodyCacheState::Disabled;
        evaluator.sync_runtime_faces_for_frame(request.frame);
        let query = self.layout_frame_rust_for_purpose_inner(
            evaluator,
            request.frame,
            LayoutPurpose::MiniPreparation {
                window_id: request.window,
            },
            None,
            #[cfg(test)]
            None,
        );
        self.report_image_failures(evaluator);
        let query = query.ok_or_else(|| {
            evaluator.failed_redisplay_mini_preparation(
                neovm_core::emacs_core::eval::RedisplayMiniPreparationFailure::DidNotConverge,
            )
        })?;
        let height = self
            .mini_preparation_height
            .take()
            .unwrap_or(char_height)
            .max(char_height);
        let empty = evaluator
            .buffer_manager()
            .get(request.buffer)
            .is_none_or(|buffer| buffer.accessible_emacs_byte_range().is_empty());
        // GNU grow-only's allowed shrink is shrink_mini_window(unit), even
        // when a nonempty shorter message requests more than one line.
        let desired_height = if mode == ResizeMiniWindowsMode::GrowOnly && height < old_height {
            if mode.should_shrink(request.exact, empty) {
                char_height
            } else {
                old_height
            }
        } else {
            height
        };
        let resize = evaluator
            .frame_manager()
            .get(request.frame)
            .and_then(|frame| frame.plan_mini_window_resize(desired_height, max_lines))
            .filter(|resize| {
                if resize.is_growth() {
                    mode.should_grow()
                } else {
                    mode.should_shrink(request.exact, empty)
                }
            });
        // The display-row producer supplies exact source anchors, including
        // overlay/display-string rows. Choose the last screenful from these
        // rows, rather than counting physical newlines or measuring a prefix.
        let max_height = (max_lines * char_height).max(char_height).min(inner_height);
        let start = if height > max_height && adhoc {
            query
                .geometry()
                .and_then(|snapshot| {
                    let target_y = snapshot
                        .rows
                        .last()?
                        .y
                        .saturating_sub((max_height - char_height).round() as i64);
                    if evaluator.gnu_redisplay_hooks_policy_enabled() {
                        let end = evaluator
                            .buffer_manager()
                            .get(request.buffer)
                            .map(|buffer| {
                                LispCharPos1::from_one_based_usize(
                                    buffer.point_max_char_pos().get().saturating_add(1),
                                )
                            })?;
                        mini_source_start::aligned_after_string_start(snapshot, target_y, end)
                    } else {
                        snapshot
                            .rows
                            .iter()
                            .rev()
                            .find(|row| row.y <= target_y)
                            .and_then(|row| row.start_buffer_pos)
                    }
                })
                .unwrap_or(begv)
        } else if height <= max_height {
            begv
        } else {
            evaluator
                .frame_manager()
                .get(request.frame)
                .and_then(|frame| frame.find_window(request.window))
                .and_then(|window| {
                    if let neovm_core::window::Window::Leaf { window_start, .. } = window {
                        Some(*window_start)
                    } else {
                        None
                    }
                })
                .unwrap_or(begv)
        };
        evaluator.gnu_set_prepared_minibuffer_start(
            request.frame,
            request.window,
            start,
            height > max_height && adhoc,
        );
        if let Some(resize) = resize {
            evaluator.gnu_apply_prepared_minibuffer_resize(
                request.frame,
                request.window,
                resize.height_px(),
            )?;
        }
        Ok(Value::NIL)
    }

    /// Answer a live window query from validated geometry or the canonical row
    /// producer, GNU's `start_display` + `move_it_to` from `w->start`.
    ///
    /// Answers both display questions the walk settles at once:
    /// GNU-compatible `window-end`, and the window's display geometry.
    pub fn query_window_layout(
        &mut self,
        evaluator: &mut neovm_core::emacs_core::Context,
        frame_id: neovm_core::window::FrameId,
        window_id: neovm_core::window::WindowId,
        scope: neovm_core::window::WindowLayoutQueryScope,
    ) -> Result<neovm_core::window::WindowLayoutQuery, neovm_core::window::WindowLayoutQueryFailure>
    {
        self.query_window_layout_observed(
            evaluator,
            frame_id,
            window_id,
            scope,
            #[cfg(test)]
            None,
        )
    }

    /// Select native observation/admission for this engine-local query only.
    /// The policy is configuration, never evidence that geometry is reusable.
    pub fn query_window_layout_with_policy(
        &mut self,
        evaluator: &mut neovm_core::emacs_core::Context,
        frame_id: neovm_core::window::FrameId,
        window_id: neovm_core::window::WindowId,
        scope: neovm_core::window::WindowLayoutQueryScope,
        policy: &RowsQueryPolicy,
    ) -> Result<neovm_core::window::WindowLayoutQuery, neovm_core::window::WindowLayoutQueryFailure>
    {
        self.query_window_layout_observed_with_policy(
            evaluator,
            frame_id,
            window_id,
            scope,
            policy,
            #[cfg(test)]
            None,
        )
    }

    // The test sink runs after the proof/source lease have been consumed and
    // dropped, before the original publication candidate is revalidated.
    fn query_window_layout_observed(
        &mut self,
        evaluator: &mut neovm_core::emacs_core::Context,
        frame_id: neovm_core::window::FrameId,
        window_id: neovm_core::window::WindowId,
        scope: neovm_core::window::WindowLayoutQueryScope,
        #[cfg(test)] mut report_sink: Option<
            &mut dyn FnMut(&mut neovm_core::emacs_core::Context, rows_lifetime::Report),
        >,
    ) -> Result<neovm_core::window::WindowLayoutQuery, neovm_core::window::WindowLayoutQueryFailure>
    {
        self.query_window_layout_observed_with_policy(
            evaluator,
            frame_id,
            window_id,
            scope,
            &RowsQueryPolicy::disabled(),
            #[cfg(test)]
            report_sink.take(),
        )
    }

    fn query_window_layout_observed_with_policy(
        &mut self,
        evaluator: &mut neovm_core::emacs_core::Context,
        frame_id: neovm_core::window::FrameId,
        window_id: neovm_core::window::WindowId,
        scope: neovm_core::window::WindowLayoutQueryScope,
        policy: &RowsQueryPolicy,
        #[cfg(test)] mut report_sink: Option<
            &mut dyn FnMut(&mut neovm_core::emacs_core::Context, rows_lifetime::Report),
        >,
    ) -> Result<neovm_core::window::WindowLayoutQuery, neovm_core::window::WindowLayoutQueryFailure>
    {
        let text_extent = matches!(
            scope,
            neovm_core::window::WindowLayoutQueryScope::TextExtent { .. }
        );
        // Position lookup occurs only after canonical chrome measurement in
        // the frame coordinator. Other scopes retain their accepted partition.
        let position_query = matches!(
            scope,
            neovm_core::window::WindowLayoutQueryScope::Position { .. }
        );
        let certified_rows = matches!(
            (policy.environment(), policy.admission(), scope),
            (
                rows_query_policy::RowsQueryEnvironment::Terminal(_),
                RowsQueryAdmission::ExactProven,
                neovm_core::window::WindowLayoutQueryScope::Rows { .. }
            )
        );
        if certified_rows {
            if self.font_metrics.is_none()
                && let Some(hit) = self
                    .query_cache
                    .get_certified(evaluator, frame_id, window_id, scope)
            {
                // This success-only native move is the sole certified return.
                // No trace, sink or callback follows its final validation.
                return Ok(hit.into_query());
            }
            // An unproved selected Rows request must run the canonical walk;
            // neither a legacy observation nor a prepared viewport can answer.
        } else {
            // Position queries first measure their current chrome in the
            // normalized route below. Only its validated partition may admit
            // body reuse; other scopes keep their accepted partition.
            if !text_extent
                && !position_query
                && let Some(query) = self.query_cache.get(evaluator, frame_id, window_id, scope)
            {
                return Ok(query);
            }
            if !position_query
                && let Some(query) = self
                    .prepared_viewports
                    .measure_rows(evaluator, frame_id, window_id, scope)
            {
                return Ok(query);
            }
        }
        self.query_body_cache_state = QueryBodyCacheState::Produced;
        self.query_restart_rows.clear();
        self.query_row_coverage = QueryRowCoverage::Complete;
        self.query_target_prefix_policy = false;
        self.mini_preparation_height = None;
        // Snapshot only actual canonical Rows misses; no ordinary frame,
        // prepared-viewport or existing cache-hit maintenance is added.
        let mut lifetime = matches!(
            scope,
            neovm_core::window::WindowLayoutQueryScope::Rows { .. }
        )
        .then(|| {
            (match policy.environment() {
                rows_query_policy::RowsQueryEnvironment::Graphical => {
                    rows_lifetime::Producer::begin(self, evaluator, frame_id, window_id, scope)
                }
                rows_query_policy::RowsQueryEnvironment::Terminal(ref host) => {
                    rows_lifetime::Producer::begin_tty(
                        self,
                        evaluator,
                        frame_id,
                        window_id,
                        scope,
                        Some(host),
                    )
                }
            })
            .and_then(|producer| {
                if certified_rows {
                    producer.for_admission(evaluator)
                } else {
                    Ok(producer)
                }
            })
        });
        let (query, mut collections) =
            if let neovm_core::window::WindowLayoutQueryScope::Position { target } = scope {
                let request = position_chrome::Request::new(frame_id, window_id, target);
                let mut run = position_chrome::QueryRun::new(self, evaluator, request);
                neovm_core::tagged::collection_reads::capture_normalized(
                    &mut run,
                    |run| run.normalize(),
                    |run| run.read(),
                )
            } else if matches!(
                scope,
                neovm_core::window::WindowLayoutQueryScope::Rows { .. }
            ) {
                let face_services = rows_face_services::RowsFaceServiceFactory::new();
                let captured = neovm_core::tagged::collection_reads::capture_normalized(
                    evaluator,
                    |evaluator| {
                        evaluator.sync_runtime_faces_for_frame(frame_id);
                    },
                    |evaluator| {
                        self.layout_frame_rust_with_face_services(
                            evaluator,
                            frame_id,
                            LayoutPurpose::SynchronousQuery { window_id, scope },
                            lifetime.as_mut().and_then(|pending| pending.as_mut().ok()),
                            #[cfg(test)]
                            None,
                            &face_services,
                            None,
                        )
                        .into_query()
                    },
                );
                if let Some(producer) = lifetime.as_mut().and_then(|pending| pending.as_mut().ok())
                {
                    producer.note_face_entry(Some(face_services.cold_entry()));
                    producer.note_diagnostic_effects(face_services.diagnostic_effects());
                }
                captured
            } else {
                neovm_core::tagged::collection_reads::capture_normalized(
                    evaluator,
                    |evaluator| {
                        evaluator.sync_runtime_faces_for_frame(frame_id);
                    },
                    |evaluator| {
                        self.layout_frame_rust_for_purpose_inner(
                            evaluator,
                            frame_id,
                            LayoutPurpose::SynchronousQuery { window_id, scope },
                            lifetime.as_mut().and_then(|pending| pending.as_mut().ok()),
                            #[cfg(test)]
                            None,
                        )
                    },
                )
            };
        self.report_image_failures(evaluator);
        let mut query =
            query.ok_or(neovm_core::window::WindowLayoutQueryFailure::DidNotConverge)?;
        let mut candidate = None;
        if let Some(pending) = lifetime {
            let report = if certified_rows {
                let completed = rows_lifetime::finish_for_admission(
                    pending,
                    self,
                    evaluator,
                    query,
                    collections.take(),
                );
                query = completed.canonical_query;
                candidate = completed.candidate;
                rows_lifetime::trace_report(completed.report);
                completed.report
            } else {
                rows_lifetime::finish_and_drop(
                    pending,
                    self,
                    evaluator,
                    &query,
                    collections.as_ref(),
                )
            };
            #[cfg(test)]
            if let Some(sink) = report_sink.as_deref_mut() {
                sink(evaluator, report);
            }
            #[cfg(not(test))]
            let _ = report;
        }
        let query_restart_rows = std::mem::take(&mut self.query_restart_rows);
        if certified_rows {
            if let Some(candidate) = candidate {
                // All arbitrary trace/sink boundaries precede this original
                // epoch/source/read-set revalidation and native insertion.
                self.query_cache.remember_certified(evaluator, candidate);
            }
        } else if !text_extent
            && self.query_body_cache_state == QueryBodyCacheState::Produced
            && let Some(collections) = collections
        {
            self.query_cache.remember(
                evaluator,
                frame_id,
                window_id,
                scope,
                &query,
                collections,
                query_restart_rows,
                self.query_row_coverage,
                self.query_target_prefix_policy,
            );
        }
        Ok(query)
    }

    /// Simplified window layout using neovm-core data.
    ///
    /// Renders buffer text as a monospace grid with face resolution.
    /// Queries FontMetricsService for per-face character metrics when available.
    /// Note: fontification (jit-lock / font-lock) is triggered by
    /// `layout_frame_rust()` before this function is called, so text
    /// properties are already up-to-date when we read them here.
    /// Phase 1: if this window's previous-frame matrix can be reused with only
    /// the cursor re-decorated (point moved, every other layout input and the
    /// neovm-core invalidation ticks unchanged, cursor row structurally simple),
    /// return the replay bundle; else `None` (→ full rebuild). Reads the retained
    /// matrix from the *previous* frame (committed at the prior frame's accepted
    /// break; never overwritten until this frame's commit).
    /// Make [`Self::retained_window_matrices`] belong to `frame_id`.
    ///
    /// Parks whichever frame's state is loaded and loads this one. Parking
    /// rather than dropping is what lets an aborted layout keep its retained
    /// state: nothing is lost, it is merely somewhere else.
    fn load_retained_frame(&mut self, frame_id: neovm_core::window::FrameId) {
        if self.retained_frame == Some(frame_id) {
            return;
        }
        self.query_cache.clear();
        if let Some(parked) = self.retained_frame.take() {
            self.retained_by_frame.insert(
                parked,
                RetainedFrameState {
                    matrices: std::mem::take(&mut self.retained_window_matrices),
                    chrome_metrics: std::mem::take(&mut self.retained_window_chrome_metrics),
                    scroll_preview_coverage: std::mem::take(&mut self.scroll_preview_coverage),
                    composition_advances: std::mem::take(&mut self.retained_composition_advances),
                },
            );
        }
        let loaded = self.retained_by_frame.remove(&frame_id).unwrap_or_default();
        self.retained_window_matrices = loaded.matrices;
        self.retained_window_chrome_metrics = loaded.chrome_metrics;
        self.scroll_preview_coverage = loaded.scroll_preview_coverage;
        self.retained_composition_advances = loaded.composition_advances;
        self.retained_frame = Some(frame_id);
    }

    fn build_cursor_only_replay(
        &self,
        params: &WindowParams,
        layout_box: WindowLayoutBox,
        evaluator: &neovm_core::emacs_core::Context,
    ) -> Option<CursorOnlyReplay> {
        // The cursor-only fast path applies to ANY window. The render cursor branch
        // handles both styles (the resolved cursor is hollow for a non-selected
        // window). Phase A admits every replaying window's faces into one
        // frame-wide namespace before rendering, so a non-selected window
        // co-resident with a re-laid window cannot corrupt face resolution. The
        // dominant multi-window win: a non-selected window that did not change
        // reuses its body verbatim instead of full-rebuilding when another window
        // is edited.
        let window_id = DisplayWindowId::new(params.window_id);
        let Some(prev) = self.retained_window_matrices.get(&window_id) else {
            tracing::debug!(
                window = params.window_id,
                "cursor-only declined: no retained matrix for this window yet"
            );
            return None;
        };
        // The row key cannot see interval mutations on an existing string.
        // A missing/expired producer certificate must never retain selection.
        if !self
            .retained_frame
            .and_then(|frame| self.frame_visual_histories.get(frame))
            .and_then(|history| history.cursor_sources_for(window_id))
            .is_some_and(|sources| sources.can_replay() && sources.unchanged())
        {
            return None;
        }
        let curr_key = RetainedWindowKey::from_params(params, layout_box, evaluator);
        let mut replay = match prev.cursor_only_replay(&curr_key) {
            Ok(replay) => replay,
            Err(reason) => {
                // The edit path reports why it declined; without the same
                // report here a window that silently full-rebuilds every frame
                // is invisible, which is exactly how the LSP fixture hid two of
                // its three windows.
                tracing::debug!(
                    window = params.window_id,
                    validity = ?prev.validity,
                    ?reason,
                    differing = ?prev.key.differing_fields(&curr_key),
                    "cursor-only declined by the retained matrix"
                );
                return None;
            }
        };
        if prev.chrome_reusable_after_cursor_move(&replay, chrome_reuse_context(params, evaluator))
            && let Some(chrome) = prev.retained_chrome()
        {
            crate::neovm_bridge::CHROME_ROWS_REUSED
                .fetch_add(chrome.rows.len(), std::sync::atomic::Ordering::Relaxed);
            replay.chrome = Some(chrome);
        } else if crate::incremental_layout::chrome_memo::chrome_memo_mode().enabled() {
            replay.chrome_memo = prev.chrome_memo();
        }
        Some(replay)
    }

    /// Phase 2: if this window's previous-frame matrix can be reused after a
    /// whole-row scroll (overlapping rows shifted, only newly-exposed rows
    /// walked), return the scroll replay; else `None`. Selected window only, for
    /// the same reason as [`Self::build_cursor_only_replay`].
    fn build_scroll_replay(
        &self,
        params: &WindowParams,
        layout_box: WindowLayoutBox,
        evaluator: &neovm_core::emacs_core::Context,
    ) -> Option<ScrollReplay> {
        if !params.selected {
            return None;
        }
        let window_id = DisplayWindowId::new(params.window_id);
        let prev = self.retained_window_matrices.get(&window_id)?;
        let curr_key = RetainedWindowKey::from_params(params, layout_box, evaluator);
        if curr_key.window_start < prev.key.window_start
            && (!crate::incremental_layout::edit_sync::scroll_back_enabled()
                || !Self::small_backward_scroll(prev, &curr_key, evaluator))
        {
            return None;
        }

        // A genuine scroll keeps walking chrome, and structurally rather than by
        // trusting a trigger: `%p` is computed from window-start/window-end, so
        // chrome whose visible region moved is stale by definition. Leaving
        // `chrome` at `None` is that decision. (GNU agrees from the other side —
        // its scroll commands all call `wset_update_mode_line`, window.c:6279,
        // 6418, 6603, 6864.)
        let mut replay = prev.scroll_replay(&curr_key).or_else(|| {
            use crate::incremental_layout::edit_sync::{self, EditSyncMode};
            if !self.allow_below_reuse
                || edit_sync::edit_sync_mode() != EditSyncMode::Sync
                || !edit_sync::sync_layout_conditions(&curr_key, params.scroll_margin)
            {
                return None;
            }
            let buffer = evaluator
                .buffer_manager()
                .get(neovm_core::buffer::BufferId(params.buffer_id))?;
            if !crate::incremental_layout::face_overlay_aliases_supported(
                crate::neovm_bridge::visible_display_value(
                    evaluator,
                    neovm_core::buffer::BufferId(params.buffer_id),
                    crate::neovm_bridge::LayoutVar::CharPropertyAliasAlist.sym_id(),
                ),
            ) {
                return None;
            }
            prev.face_overlay_scroll_replay(&curr_key)
        })?;
        // P3.5 E2: the scrolled chrome is evaluated; it may still render the
        // string it showed (the `%p` of a long buffer rarely moves).
        if crate::incremental_layout::chrome_memo::chrome_memo_mode().enabled() {
            replay.chrome_memo = prev.chrome_memo();
        }
        Some(replay)
    }

    /// Choose reuse only when most rows can survive. Count logical lines
    /// through the text index, with a bounded scan fallback; this estimate
    /// controls optimization only. The canonical walk still measures every
    /// exposed row and proves the synchronization point before installing it.
    fn small_backward_scroll(
        previous: &RetainedWindowMatrix,
        current: &RetainedWindowKey,
        evaluator: &neovm_core::emacs_core::Context,
    ) -> bool {
        use neovm_core::buffer::{BufferId, CharPos0, EmacsByteRange, LineIndexBuild};
        let Some(buffer) = evaluator.buffer_manager().get(BufferId(current.buffer_id)) else {
            return false;
        };
        let from = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(
            current.window_start.max(0) as usize,
        ));
        let to = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(
            previous.key.window_start.max(0) as usize,
        ));
        if to.get().saturating_sub(from.get()) > 65_536 {
            return false;
        }
        let range = EmacsByteRange::new(from, to);
        let limit = previous
            .matrix
            .rows
            .iter()
            .filter(|row| row.enabled && !RetainedWindowMatrix::is_chrome_role(row.role))
            .count()
            / 2;
        // An estimate that only picks an optimization: an existing index may
        // answer it, but it must not build one (the bounded scan below is the
        // documented fallback).
        if let Some(lines) = buffer.indexed_newline_count(range, LineIndexBuild::ExistingOnly) {
            return lines <= limit;
        }
        let mut lines = 0;
        buffer
            .try_for_each_emacs_byte_range_chunk(range, |chunk| {
                lines += chunk.iter().filter(|byte| **byte == b'\n').count();
                if lines > limit { Err(()) } else { Ok(()) }
            })
            .is_ok()
    }

    /// Source newlines in a dirty span, the mirror of GNU `find_newline` over
    /// a changed region (backed by the buffer's newline cache; the line delta
    /// `try_window_id` scrolls its matrix by, src/xdisp.c). An existing text
    /// index answers without touching bytes; an uncovered range falls back to
    /// a chunked `\n` count. This replaces a per-character
    /// `char_at_emacs_byte_pos` walk that cost ~77 Ir per inserted character
    /// on the first redisplay after a bulk insert — the dominant linear term
    /// of a cold redisplay over a newline-less tail.
    fn span_source_newline_count(
        buffer: &neovm_core::buffer::Buffer,
        dirty_start: i64,
        dirty_end: i64,
    ) -> usize {
        use neovm_core::buffer::{CharPos0, EmacsByteRange, LineIndexBuild};
        let start = CharPos0::new(dirty_start.max(0) as usize);
        let end = CharPos0::new(dirty_end.max(0) as usize);
        let from = buffer.char_pos_to_emacs_byte_pos_clamped(start);
        let to = buffer.char_pos_to_emacs_byte_pos_clamped(end.max(start));
        let range = EmacsByteRange::new(from, to);
        // This is a scan of the CHANGED region, so its cost is the edit's,
        // never the buffer's: an existing index may answer it, but it must
        // not build one. GNU counts the same newlines with `find_newline`
        // over the changed area (src/search.c:675), whose newline cache only
        // covers regions something scanned (src/search.c:595).
        if let Some(count) = buffer.indexed_newline_count(range, LineIndexBuild::ExistingOnly) {
            return count;
        }
        let mut count = 0;
        let _ = buffer.try_for_each_emacs_byte_range_chunk(range, |chunk| {
            count += memchr::memchr_iter(b'\n', chunk).count();
            Ok::<(), std::convert::Infallible>(())
        });
        count
    }

    /// Phase 3: if this window's previous-frame matrix can be reused after a
    /// localized (plain) edit — reuse the rows above the dirty span verbatim and
    /// re-walk only the dirty line + below — return the replay (a [`ScrollReplay`]
    /// with `dvpos = 0`); else `None`. Reads the accumulated dirty char span from
    /// the buffer. Selected window only.
    fn build_edit_replay(
        &self,
        params: &WindowParams,
        layout_box: WindowLayoutBox,
        evaluator: &neovm_core::emacs_core::Context,
    ) -> Option<ScrollReplay> {
        use crate::incremental_layout::edit_sync::{self, BelowReuse, EditSyncMode};
        // Deliberately NOT restricted to the selected window. GNU's
        // `try_window_id` (xdisp.c:22560-22960) has no such condition -- it
        // runs for whichever window `redisplay_window` is laying out -- and the
        // restriction here dated only from the fast paths' first landing
        // (ce214316a), where it was starting scope rather than a fix for
        // anything. A non-selected window that took an edit is exactly the
        // multi-window case worth catching, as `build_cursor_only_replay` says
        // in its own comment. Selection is part of `RetainedWindowKey`
        // (`selected` and `cursor_role`), so a window that GAINS or LOSES it
        // still escalates to a full relayout rather than carrying a stale
        // cursor in from the retained matrix.
        let window_id = DisplayWindowId::new(params.window_id);
        let prev = self.retained_window_matrices.get(&window_id)?;
        let curr_key = RetainedWindowKey::from_params(params, layout_box, evaluator);
        let buffer = evaluator
            .buffer_manager()
            .get(neovm_core::buffer::BufferId(params.buffer_id))?;
        if prev.key.overlay_digest != curr_key.overlay_digest {
            // A face-only overlay change moves no text, so the row walker
            // re-measures every changed face and the sync frontier's y-check
            // refuses a reused row whose height moved. That is why this path
            // accepts `line-spacing` and automatic paragraph direction, which
            // an edit replay must refuse; wrapping, numbered gutters,
            // trailing whitespace, horizontal scroll and a scroll margin still
            // decline in `sync_layout_conditions`.
            //
            // Fontification callbacks are deliberately NOT a refusal here:
            // jit-lock installs buffer-local `fontification-functions` in every
            // font-locked buffer, which is exactly where hl-line, show-paren
            // and region move. A face installed this frame bumps the property
            // tick, so the leaf's post-fontification freshness revalidation
            // rejects the attempt and the retry rebuilds from the new inputs
            // rather than publishing rows this plan measured without them.
            if !self.allow_below_reuse
                // The process-wide sync switch also governs this replay.
                || edit_sync::edit_sync_mode() != EditSyncMode::Sync
                || !edit_sync::sync_layout_conditions(&curr_key, params.scroll_margin)
                // An alias can give an ordinary face overlay a structural
                // meaning (for example, a display-property fallback).
                || !crate::incremental_layout::face_overlay_aliases_supported(
                    crate::neovm_bridge::visible_display_value(
                        evaluator,
                        neovm_core::buffer::BufferId(params.buffer_id),
                        crate::neovm_bridge::LayoutVar::CharPropertyAliasAlist.sym_id(),
                    ),
                )
            {
                return None;
            }
            let mut replay = prev.face_overlay_replay(&curr_key)?;
            // Keep evaluating mode-line Lisp. Reuse its glyphs only when the
            // existing chrome memo proves that its freshly evaluated output
            // and rendering inputs are identical.
            if crate::incremental_layout::chrome_memo::chrome_memo_mode().enabled() {
                replay.chrome_memo = prev.chrome_memo();
            }
            return Some(replay);
        }
        // The PRE-fontification span (GNU decision order): the keystroke's own
        // damage. The fontification pass has already run by now and widened
        // the live accumulator to the whole jit-lock chunk; using the live
        // range would relay every chunk row for identical faces.
        let Some((dirty_start, dirty_end)) = *self
            .pre_fontify_dirty_spans
            .get(&params.buffer_id)
            .unwrap_or(&None)
        else {
            tracing::debug!(
                window = params.window_id,
                differing = ?prev.key.differing_fields(&curr_key),
                "edit replay declined: no keystroke damage span this frame"
            );
            return None;
        };
        let delta = curr_key.buffer_size - prev.key.buffer_size;
        use crate::incremental_layout::{EditReplayPositions, EditReplaySourceProof};
        let positions = EditReplayPositions::new(dirty_start, dirty_end, delta);
        let span_newlines = Self::span_source_newline_count(buffer, dirty_start, dirty_end);
        // The one-line-one-row ASCII proof is this engine's extra safety for
        // below-reuse; GNU gets the same guarantee by re-syncing its
        // regenerated region against the old matrix after the walk and never
        // scans the changed text semantically. Bound the proof by the WINDOW,
        // not the buffer: a dirty span longer than the whole narrowed region
        // the window can display (`get_narrowed_len` = width × height,
        // src/xdisp.c:3732) is never proven. That is the O(window) cap that
        // keeps a cold first redisplay after a bulk insert from walking every
        // inserted character; the window then re-lays, the bounded cost GNU
        // pays in the same situation.
        //
        // The window-start condition is narrower than GNU `try_window_id`'s
        // GIVE_UP (15), which declines whenever the start lies anywhere in the
        // changed area (`CHARPOS (start) >= first_changed_charpos && CHARPOS
        // (start) <= last_changed_charpos`, src/xdisp.c:22816-22825) and
        // re-lays with `try_window` (:21052, :21266). This replay supports the
        // bounded case GNU's give-up rejects: edit eligibility already pins
        // the window start position (`RetainedWindowKey::edit_eligible`,
        // `!window_start_moved`; the start marker stays before text inserted
        // at it, `START_INSERTION_TYPE`), so when the span merely BEGINS at
        // the window start the retained first row is dirty, nothing above it
        // is reused, and the walk starts from that same charpos with the NEW
        // text — exactly the rows `try_window` would re-lay — while the rows
        // below reuse through the usual post-walk validation. A span that
        // CROSSES a length-changing edit over the window start is the case
        // where the position stops denoting the same source text; decline it
        // like GIVE_UP (15). Properties-only damage (delta == 0) leaves every
        // character in place, so the start stays valid even inside the span.
        // The newline count above is kept: it feeds the Sync walk's damage
        // record the way GNU feeds `find_newline` counts to its matrix
        // scroll, and costs bytes-per-span rather than chars-per-span.
        let dirty_end_old = dirty_end.saturating_sub(delta);
        let window_start_inside_changed_text = delta != 0
            && dirty_start < curr_key.window_start
            && curr_key.window_start < dirty_end_old;
        let proof_scan_allowed = self.allow_below_reuse
            && !window_start_inside_changed_text
            && (dirty_end.saturating_sub(dirty_start).max(0) as usize)
                <= params
                    .source_interpretation_rows()
                    .saturating_mul(params.narrowed_width_chars());
        let (replay, span_newlines, span_structure_safe) = if edit_sync::lazy_proof_enabled()
            && self.allow_below_reuse
            && edit_sync::edit_sync_mode() == EditSyncMode::Sync
            && !edit_sync::prove_first_enabled()
            && edit_sync::sync_allowed(&curr_key, params.scroll_margin, buffer)
        {
            #[cfg(test)]
            crate::incremental_layout::lazy_proof_test_support::note_lazy_entry();
            let mut observed_newlines = None;
            let mut observed_safe = None;
            let replay = prev.edit_replay_sync_lazy(&curr_key, positions, || {
                #[cfg(test)]
                crate::incremental_layout::lazy_proof_test_support::note_source_proof();
                let simple = proof_scan_allowed
                    && (dirty_start..dirty_end).all(|cp| {
                        #[cfg(test)]
                        crate::incremental_layout::lazy_proof_test_support::note_char_query();
                        let byte = buffer.char_pos_to_emacs_byte_pos_clamped(
                            neovm_core::buffer::CharPos0::new(cp as usize),
                        );
                        match buffer.char_at_emacs_byte_pos(byte) {
                            Some('\n') => true,
                            Some(c) => c.is_ascii_graphic() || c == ' ',
                            None => false,
                        }
                    });
                let structure_range = neovm_core::buffer::CharRange::new(
                    neovm_core::buffer::CharPos0::new(dirty_start.max(0) as usize),
                    neovm_core::buffer::CharPos0::new(dirty_end.max(0) as usize),
                );
                let safe = simple && {
                    #[cfg(test)]
                    crate::incremental_layout::lazy_proof_test_support::note_property_query();
                    !buffer.has_any_non_nil_property_in_char_range(
                        structure_range,
                        &EditReplayStructureProperty::symbols(),
                    )
                };
                observed_newlines = Some(span_newlines);
                observed_safe = Some(safe);
                if safe {
                    EditReplaySourceProof::Simple {
                        newlines: span_newlines,
                    }
                } else {
                    EditReplaySourceProof::Rejected
                }
            });
            (replay, observed_newlines, observed_safe)
        } else {
            // Below-reuse SAFETY GATE, part 1: every char in the dirty span is
            // printable ASCII (graphic or space) or a newline — combined with
            // edit_replay's monospace + width check this proves each span line
            // still occupies exactly one row (no wrap), which is what makes the
            // rows-below reuse (shift charpos, keep pixel_y) sound. Newlines are
            // allowed because the span relays WHOLE rows: an existing newline is
            // a row boundary inside the span (jit-lock's line region includes the
            // trailing newline), and an INSERTED newline — which does change the
            // row structure — makes the bounded walk miss its expected_walk
            // end-charpos contract and bail, the same runtime backstop deletes
            // rely on. A tab or wide char escalates to above-only (the
            // cols-times-char_width fit arithmetic would lie about them). With
            // property changes feeding the accumulator, the span covers the
            // jit-lock line region, so this also vets the line's existing content.
            // A pure delete has an empty NEW span (its old extent is the deleted
            // range) — vacuously ASCII-safe; edit_replay + the post-walk
            // validation own the delete-specific safety.
            //
            // The scan itself is window-bounded through `proof_scan_allowed`
            // above (get_narrowed_len): a span longer than the window can
            // display skips the proof and pays one bounded window re-lay, and
            // a length-changing edit crossing the window start declines like
            // GNU try_window_id's GIVE_UP (15). Newlines come from
            // `span_source_newline_count`, not this scan.
            #[cfg(test)]
            if self.allow_below_reuse {
                crate::incremental_layout::lazy_proof_test_support::note_source_proof();
            }
            let simple_span = proof_scan_allowed
                && (dirty_start..dirty_end).all(|cp| {
                    #[cfg(test)]
                    crate::incremental_layout::lazy_proof_test_support::note_char_query();
                    let byte = buffer.char_pos_to_emacs_byte_pos_clamped(
                        neovm_core::buffer::CharPos0::new(cp as usize),
                    );
                    match buffer.char_at_emacs_byte_pos(byte) {
                        Some('\n') => true,
                        Some(c) => c.is_ascii_graphic() || c == ' ',
                        None => false,
                    }
                });
            // Part 2: no structure-affecting text property may cover the span.
            // Face-class props (`face`, `font-lock-face`, `fontified`) only recolor
            // glyphs; these change what the chars BECOME (replacement, hiding,
            // prefixes, composition), which invalidates the one-line-one-row proof.
            // GNU's try_window_id needs no such scan because its regenerated region
            // re-syncs against the old matrix post-walk; here the post-walk
            // `expected_walk` validation is the backstop and this scan keeps the
            // replay from being built (and bailed) pointlessly.
            let structure_range = neovm_core::buffer::CharRange::new(
                neovm_core::buffer::CharPos0::new(dirty_start.max(0) as usize),
                neovm_core::buffer::CharPos0::new(dirty_end.max(0) as usize),
            );
            let span_structure_safe = simple_span && {
                #[cfg(test)]
                crate::incremental_layout::lazy_proof_test_support::note_property_query();
                !buffer.has_any_non_nil_property_in_char_range(
                    structure_range,
                    &EditReplayStructureProperty::symbols(),
                )
            };
            let damage = EditDamage::new(dirty_start, dirty_end, delta, span_newlines);
            // P3.5 G2: under NEOMACS_LAYOUT_EDIT_SYNC=sync the rows below the edit
            // are reused by synchronizing the walk with them (GNU try_window_id),
            // which needs none of the span proofs above; they remain the fallback
            // when no row below can be synchronized with.
            let below = if self.allow_below_reuse
                && edit_sync::edit_sync_mode() == EditSyncMode::Sync
                && edit_sync::sync_allowed(&curr_key, params.scroll_margin, buffer)
            {
                BelowReuse::Sync {
                    prove_fallback: span_structure_safe,
                }
            } else if span_structure_safe {
                BelowReuse::Prove
            } else {
                BelowReuse::Off
            };
            // GNU sync remains the semantic policy. Its general producer need
            // not replace the already certified single-row producer: both are
            // sealed by the existing post-walk contracts. A prove call may also
            // return an ABOVE-ONLY replay; that is a rejected proof, never a win.
            let replay = if matches!(
                below,
                BelowReuse::Sync {
                    prove_fallback: true
                }
            ) && edit_sync::prove_first_enabled()
            {
                match prev.edit_replay_with(&curr_key, damage, BelowReuse::Prove) {
                    Some(proved)
                        if proved.bound_walk
                            && proved.expected_walk.is_some()
                            && prev.prove_span_matches_sync(damage, &proved) =>
                    {
                        #[cfg(test)]
                        edit_sync::note_prove_first_for_test(true);
                        Some(proved)
                    }
                    _ => {
                        #[cfg(test)]
                        edit_sync::note_prove_first_for_test(false);
                        prev.edit_replay_with(&curr_key, damage, below)
                    }
                }
            } else {
                prev.edit_replay_with(&curr_key, damage, below)
            };
            (replay, Some(span_newlines), Some(span_structure_safe))
        };
        let Some(mut replay) = replay else {
            if tracing::enabled!(tracing::Level::DEBUG) {
                let body: Vec<&neomacs_display_protocol::glyph_matrix::GlyphRow> = prev
                    .matrix
                    .rows
                    .iter()
                    .filter(|row| row.enabled && !RetainedWindowMatrix::is_chrome_role(row.role))
                    .map(|row| &**row)
                    .collect();
                tracing::debug!(
                    window = params.window_id,
                    differing = ?prev.key.differing_fields(&curr_key),
                    validity = ?prev.validity,
                    vscroll = curr_key.vscroll,
                    dirty_start,
                    dirty_end,
                    delta,
                    span_newlines = ?span_newlines,
                    span_structure_safe = ?span_structure_safe,
                    body_rows = body.len(),
                    margin_rows = body
                        .iter()
                        .filter(|row| {
                            !row.glyphs
                                [neomacs_display_protocol::glyph_matrix::GlyphArea::LeftMargin.index()]
                            .is_empty()
                        })
                        .count(),
                    continued_rows = body.iter().filter(|row| row.continued).count(),
                    truncated_rows = body.iter().filter(|row| row.truncated_left).count(),
                    text_fringe_rows = body
                        .iter()
                        .filter(|row| {
                            (row.left_fringe_bitmap.is_some() || row.right_fringe_bitmap.is_some())
                                && row.displays_text
                        })
                        .count(),
                    first_dirty = body
                        .iter()
                        .position(|row| row.end_charpos as i64 >= dirty_start),
                    "edit replay declined by the retained matrix"
                );
            }
            return None;
        };
        let keep_chrome = match crate::incremental_layout::mode_line_gate::mode_line_gate() {
            crate::incremental_layout::mode_line_gate::ModeLineGate::Legacy => prev
                .chrome_reusable_after_edit_positions(
                    &replay,
                    positions,
                    chrome_reuse_context(params, evaluator),
                    |from, to| {
                        (from.max(0)..to.max(0)).any(|cp| {
                            let byte = buffer.char_pos_to_emacs_byte_pos_clamped(
                                neovm_core::buffer::CharPos0::new(cp as usize),
                            );
                            buffer.char_at_emacs_byte_pos(byte) == Some('\n')
                        })
                    },
                ),
            crate::incremental_layout::mode_line_gate::ModeLineGate::Gnu => {
                self.gnu_edit_chrome_decision(prev, &curr_key, &mut replay, params, evaluator)
            }
        };
        if keep_chrome && let Some(chrome) = prev.retained_chrome() {
            crate::neovm_bridge::CHROME_ROWS_REUSED
                .fetch_add(chrome.rows.len(), std::sync::atomic::Ordering::Relaxed);
            replay.chrome = Some(chrome);
        } else {
            replay.one_line_contract = None;
        }
        // P3.5 E2: a frame that evaluates the chrome -- including one whose
        // one-line contract breaks after the walk -- may render it from the
        // memo.
        if (replay.chrome.is_none() || replay.one_line_contract.is_some())
            && crate::incremental_layout::chrome_memo::chrome_memo_mode().enabled()
        {
            replay.chrome_memo = prev.chrome_memo();
        }
        Some(replay)
    }

    /// `NEOMACS_MODE_LINE_GATE=gnu`: GNU's optimization-1 decision for an
    /// edit replay's chrome. On a keep, the replay carries the post-walk
    /// contract the render must verify before the retained chrome stands.
    #[cold]
    #[inline(never)]
    fn gnu_edit_chrome_decision(
        &self,
        prev: &RetainedWindowMatrix,
        curr_key: &RetainedWindowKey,
        replay: &mut ScrollReplay,
        params: &WindowParams,
        evaluator: &neovm_core::emacs_core::Context,
    ) -> bool {
        use crate::incremental_layout::mode_line_gate::{
            EditFrameFacts, ModeLineDecision, ModeLineEvaluateReason, decide_edit_chrome,
            line_numbers_require_mode_line,
        };
        // GNU cannot keep optimization 1 with nonvisual line numbers. This
        // refusal is independent of every other clause, so do not gather
        // buffer facts or search the retained cursor row to rediscover it.
        // Only the GNU arm calls this function; the explicit legacy path is
        // unchanged. The diagnostic names this sufficient refusal clause.
        if line_numbers_require_mode_line(curr_key.display_line_numbers) {
            let decision = ModeLineDecision::Evaluate(ModeLineEvaluateReason::LineNumbersDisplayed);
            tracing::debug!(window = params.window_id, ?decision, "mode-line gate (gnu)");
            return false;
        }
        let Some(buffer) = evaluator
            .buffer_manager()
            .get(neovm_core::buffer::BufferId(params.buffer_id))
        else {
            return false;
        };
        let Some((dirty_start, dirty_end)) = self
            .pre_fontify_dirty_spans
            .get(&params.buffer_id)
            .copied()
            .flatten()
        else {
            return false;
        };
        let beg_unchanged = self
            .pre_fontify_gnu_beg_unchanged
            .get(&params.buffer_id)
            .copied()
            .unwrap_or(dirty_start);
        let frame = EditFrameFacts {
            chrome: chrome_reuse_context(params, evaluator),
            selected: params.selected,
            shows_current_buffer: evaluator.buffer_manager().current_buffer_id()
                == Some(neovm_core::buffer::BufferId(params.buffer_id)),
            other_windows_changed: self.frame_other_windows_changed,
            beg_unchanged,
            dirty_end,
        };
        let decision = decide_edit_chrome(prev, curr_key, replay, frame, buffer);
        tracing::debug!(window = params.window_id, ?decision, "mode-line gate (gnu)");
        match decision {
            ModeLineDecision::KeepRetained(contract) => {
                replay.one_line_contract = Some(contract);
                true
            }
            ModeLineDecision::Evaluate(_) => false,
        }
    }

    #[allow(clippy::too_many_arguments)]
    /// The overlay-view character span for one window attempt.
    ///
    /// The view must cover what the walk consumes. The accepted previous
    /// window end is the measured consumed span; a full visible budget of
    /// margin covers ordinary growth. `None` (whole-buffer view) on the first
    /// frame or when no previous span is recorded. Invisible overlays can make
    /// the walk consume far past this span; that attempt is rejected and
    /// retried with the whole-buffer view, so a range view never commits a
    /// subset answer.
    fn overlay_view_char_span_for(
        &self,
        params: &WindowParams,
        buffer: &neovm_core::buffer::Buffer,
    ) -> Option<(usize, usize)> {
        // `visible_char_bound` returns an unbounded budget when the source
        // extent is AccessibleEnd; the overlay view must stay window-sized in
        // that mode too, so derive the viewport rows from the text-area
        // geometry instead of the source-interpretation row count.
        let cols = (params.text_bounds.width / params.char_width.max(1.0)).ceil() as usize;
        let rows = (params.text_bounds.height / params.char_height.max(1.0))
            .ceil()
            .max(1.0) as usize;
        let screenful = rows.saturating_mul(cols).max(1);
        let budget = screenful.saturating_mul(4).max(8192);
        let first = (params.window_start.max(0) as usize).saturating_sub(screenful);
        let minimum_end = first.saturating_add(budget);
        let previous_span = self
            .retained_window_matrices
            .get(&DisplayWindowId::new(params.window_id))
            .and_then(|matrix| matrix.display_snapshot.visible_buffer_span())?;
        let previous_start = neovm_core::buffer::CharPos0::from_lisp(previous_span.start()).get();
        let previous_end = neovm_core::buffer::CharPos0::from_lisp(previous_span.end())
            .get()
            .saturating_add(1);
        // Carry forward `previous_end` only when the new viewport overlaps or
        // adjoins the previous one (e.g. scrolling/editing across an invisible
        // span). A large jump backward (such as `G` -> `gg` in a long log
        // buffer) must not stretch the range view from the new top all the way
        // to the old bottom and snapshot the entire buffer's overlays.
        let end_anchor = if previous_start <= minimum_end && first <= previous_end {
            previous_end.max(minimum_end)
        } else {
            minimum_end
        };
        let end = end_anchor
            .saturating_add(budget)
            .min(buffer.total_char_end_pos().get());
        Some((first.min(end), end))
    }

    fn layout_window_rust<FS: crate::neovm_bridge::FaceServices>(
        &mut self,
        evaluator: &mut neovm_core::emacs_core::Context,
        frame_id: neovm_core::window::FrameId,
        params: &WindowParams,
        frame_params: &FrameParams,
        layout_box: &WindowLayoutBox,
        face_services: &FS,
        reserve_right_border_col: bool,
        remaining_visibility_retries: usize,
        viewport_resolution: ViewportResolutionPhase,
        // A walk that consumed at or past a window-range overlay view must be
        // retried once with the whole-buffer view. `true` selects it.
        overlay_view_full: bool,
        // A previous attempt's row walk painted an automatic composition
        // outside the visible-range resolution pass (`None` = no widening).
        // `Some(from, through)` resolves at least that range; `(0,
        // usize::MAX)` resolves every span the snapshot captured.
        automatic_composition_resolve: Option<(usize, usize)>,
        // Phase A (gather) classified this window's incremental fast path against
        // the *original* params (before any echo-buffer swap below), reading the
        // same retained key the predicate was snapshotted from. Phase B (here)
        // consumes the plan inside the render path in place of the body walk.
        // `is_edit` only steers the commit-path stats classification.
        cursor_only_replay: Option<CursorOnlyReplay>,
        scroll_replay: Option<ScrollReplay>,
        is_edit: bool,
        position_publication: WindowPositionPublication,
        // Whether this attempt still needs to evaluate/measure chrome. The
        // retry after `WindowChrome` uses the already measured metrics.
        remeasure_chrome: bool,
        lisp_ledger: &mut RedisplayLispLedger,
        topology_generation: u64,
        face_attempt: &FrameFaceAttempt,
        attempt_mode: FrameAttemptMode,
        allow_prepass_recollect: bool,
        capture_chrome_for_exhaustion: bool,
        prepass_recollect: &mut PrepassRecollectState,
    ) -> LeafLayoutAttempt {
        // A row walk may finish before coverage, freshness or viewport checks
        // reject it. Keep its sole snapshot local until Completed transfers it
        // to the frame coordinator; all other returns drop the candidate.
        let mut window_snapshot = None;
        let mut source_capture = crate::cursor_source_dependencies::Capture::begin(evaluator);
        let source_window = DisplayWindowId::new(params.window_id);
        lisp_ledger.cursor_sources.remove(&source_window);
        let previous_sources = lisp_ledger
            .replay_sources
            .get(&source_window)
            .cloned()
            .or_else(|| {
                self.frame_visual_histories
                    .get(frame_id)
                    .and_then(|history| history.cursor_sources_for(source_window))
            });
        let sources_current = previous_sources
            .as_ref()
            .is_some_and(|sources| sources.can_replay() && sources.unchanged());
        let cursor_only_replay = if sources_current {
            cursor_only_replay
        } else {
            None
        };
        let scroll_replay = if sources_current { scroll_replay } else { None };
        // How much of the window this layout will actually (re)produce.  A
        // scroll replay regenerates only the newly exposed rows; the rest
        // are reused with their advances already baked in.  Record the first
        // regenerated position and the newly exposed row count so the
        // composition resolver can cover just that band.  A cursor-only
        // replay leaves `scroll_replay` None and keeps the full range.
        let replay_composition_walk = scroll_replay.as_ref().map(|replay| {
            (
                usize::try_from(replay.walk_start.get().max(0)).unwrap_or(0),
                replay.exposed_row_count,
            )
        });
        // When a reused row starts BELOW the regenerated start (an upward
        // scroll or an edit above the viewport), the regenerated band ends at
        // that row's source position; the rows below it are reused and need
        // no resolution.  `None` means the walk regenerates to the window end
        // (a downward scroll).
        let replay_first_reused_below_start =
            replay_composition_walk.and_then(|(walk_start, _)| {
                scroll_replay.as_ref().and_then(|replay| {
                    replay
                        .reused_row_snapshots
                        .iter()
                        .filter_map(|row| row.start_buffer_pos)
                        .map(|pos| neovm_core::buffer::CharPos0::from_lisp(pos).get())
                        .filter(|start| *start > walk_start)
                        .min()
                })
            });
        tracing::debug!(
            target: "neomacs_layout_engine::composition_resolver",
            replay = scroll_replay.is_some(),
            cursor_only = cursor_only_replay.is_some(),
            ?replay_composition_walk,
            ?replay_first_reused_below_start,
            "composition walk scope"
        );
        let face_resolver = face_services.resolver();
        #[cfg(test)]
        let _viewport_retry_depth = viewport_retry_depth_probe::Guard::enter();
        tracing::debug!(
            "layout_window_rust: enter win={} start={} point={} remaining={} phase={:?}",
            params.window_id,
            params.window_start,
            params.point,
            remaining_visibility_retries,
            viewport_resolution,
        );
        let window_id = neovm_core::window::WindowId(params.window_id as u64);
        let live_window_start = ResolvedWindowStart::from_layout_charpos(params.window_start);
        let resolved_window_start = match &viewport_resolution {
            ViewportResolutionPhase::Resolve => lisp_ledger
                .exact_hook_resume(window_id, live_window_start)
                .unwrap_or_else(|| {
                    resolve_leaf_window_start(
                        evaluator,
                        params,
                        frame_params,
                        layout_box,
                        position_publication,
                        scroll_replay.is_some(),
                    )
                }),
            ViewportResolutionPhase::Measure(measurement) => {
                ResolvedWindowStart::from_layout_charpos(measurement.probe_window_start().get())
            }
            ViewportResolutionPhase::Commit(window_start) => *window_start,
        };
        let resolved_params;
        let params = if resolved_window_start.get() == params.window_start {
            params
        } else {
            resolved_params = {
                let mut resolved = params.clone();
                resolved.window_start = resolved_window_start.get();
                resolved
            };
            &resolved_params
        };
        let scroll_hook_site = WindowScrollHookSite::new(window_id, resolved_window_start);
        let site_publication = if attempt_mode == FrameAttemptMode::ExhaustionSnapshot
            && matches!(position_publication, WindowPositionPublication::Redisplay)
        {
            // Commit the resolved window start without invoking
            // `window-scroll-functions` Lisp during ExhaustionSnapshot.
            WindowPositionPublication::RedisplayResumedScrollHook
        } else if params.is_minibuffer()
            && matches!(position_publication, WindowPositionPublication::Redisplay)
            && evaluator.gnu_redisplay_hooks_policy_enabled()
            && evaluator.gnu_redisplay_transaction_active()
            && lisp_ledger
                .mini_eob_scroll
                .completed(window_id, neovm_core::buffer::BufferId(params.buffer_id))
        {
            WindowPositionPublication::RedisplayMiniEobFallback
        } else {
            lisp_ledger.publication_for_site(position_publication, scroll_hook_site)
        };
        if !matches!(&viewport_resolution, ViewportResolutionPhase::Measure(_))
            && let Some(effect) = site_publication.publish_window_start(
                evaluator,
                frame_id,
                window_id,
                resolved_window_start,
            )
        {
            return LeafLayoutAttempt::Effect(effect);
        }
        let buf_id = neovm_core::buffer::BufferId(params.buffer_id);

        // GNU reaches `handle_fontified_prop` from this leaf's display
        // iterator, after `run_window_scroll_functions` and before body/chrome
        // production. Keep fontification leaf-local so sibling windows cannot
        // run Lisp ahead of this window's scroll hook.
        evaluator.setup_thread_locals();
        let accessible_start = params.accessible_start_charpos().get();
        let accessible_end = params.accessible_end_charpos().get();
        let window_start = params.window_start_charpos().get().max(accessible_start);
        let max_rows = i64::try_from(params.source_interpretation_rows()).unwrap_or(i64::MAX);
        // GNU reads `w->window_end_valid` once, at the start of
        // `redisplay_window`, and it is one input of
        // `current_matrix_up_to_date_p` (src/xdisp.c:20557-20586). The walk
        // below REPUBLISHES it, so the probe gate must read this entry value,
        // not the one the completed walk leaves behind.
        let window_end_valid_at_entry = evaluator
            .frame_manager()
            .get(frame_id)
            .and_then(|frame| frame.find_window(window_id))
            .and_then(neovm_core::window::Window::window_end_valid)
            == Some(true);
        // Speculative pre-walk span.  It lets a fontifying hook (jit-lock
        // style) mark text before the walk, and it also bounds the
        // display-condition (`when`) scan.  A hook that DECLINES ends the
        // pre-walk request at its first declined stop; the coverage pass below
        // then owns GNU's visible-stop schedule.
        let mut fontify_end = window_start
            .saturating_add(max_rows.saturating_mul(200))
            .min(accessible_end);
        let Some(freshness_before_fontification) =
            evaluator.window_layout_attempt_freshness(frame_id, window_id, buf_id)
        else {
            return LeafLayoutAttempt::LogicalInputsChanged;
        };
        let display_target = crate::display_property::DisplayPropertyTarget::for_window_system(
            face_resolver.is_window_system(),
        );
        let freshness_mode = crate::attempt_freshness::mode();
        let (
            fontification_outcome,
            display_when,
            display_when_reuse,
            cursor_only_replay,
            scroll_replay,
            is_edit,
            freshness_after_fontification,
        ) = if attempt_mode == FrameAttemptMode::ExhaustionSnapshot {
            // Callback-inhibited exhaustion snapshot: skip prepass fontification,
            // pre-walk `display (when ...)` Lisp, and incremental fast-path reuse.
            (
                neovm_core::emacs_core::xdisp::RedisplayFontificationOutcome::Unchanged,
                crate::display_when::DisplayWhenConditions::Structural,
                false,
                None,
                None,
                false,
                std::borrow::Cow::Owned(freshness_before_fontification),
            )
        } else {
            let face_change_before_prepass = evaluator.face_change_count();
            let incoming_recollect =
                std::mem::replace(prepass_recollect, PrepassRecollectState::Fresh);
            let (
                fontification_outcome,
                run_prewalk_display_when,
                can_recollect_display_when,
                carried_display_when,
            ) = match incoming_recollect {
                PrepassRecollectState::Fresh => {
                    lisp_ledger.rows_fontification.begin_leaf(window_id, buf_id);
                    let outcome = lisp_ledger.rows_fontification.ensure(
                        evaluator,
                        buf_id,
                        window_start,
                        fontify_end,
                        params.point_charpos().get() as i64 + 1,
                        neovm_core::emacs_core::xdisp::RedisplayFontificationRequest::Prepass,
                        neovm_core::emacs_core::xdisp::ProbeRow::None,
                    );
                    let preserve_fast_path_replan =
                        (is_edit || scroll_replay.is_some() || cursor_only_replay.is_some())
                            && matches!(
                            outcome,
                            neovm_core::emacs_core::xdisp::RedisplayFontificationOutcome::Fontified
                        );
                    if allow_prepass_recollect
                        && outcome.entered_callbacks()
                        && !preserve_fast_path_replan
                        && !evaluator.has_mode_line_display_flow()
                        && !evaluator.redisplay_hook_flow_pending()
                        && evaluator.frame_manager().window_topology_generation()
                            == topology_generation
                        && evaluator.face_change_count() == face_change_before_prepass
                    {
                        lisp_ledger.prepass_freshness_rechecks += 1;
                        let after_prepass =
                            evaluator.window_layout_attempt_freshness(frame_id, window_id, buf_id);
                        if let Some(after_prepass) = after_prepass
                            && after_prepass != freshness_before_fontification
                        {
                            return LeafLayoutAttempt::PrepassInputsChanged(Box::new(
                                PrepassRecollectState::AfterFontification {
                                    outcome,
                                    fontify_end,
                                },
                            ));
                        }
                    }
                    (
                        outcome,
                        true,
                        allow_prepass_recollect && !preserve_fast_path_replan,
                        None,
                    )
                }
                PrepassRecollectState::AfterFontification {
                    outcome,
                    fontify_end: carried_end,
                } => {
                    fontify_end = carried_end;
                    (outcome, true, false, None)
                }
                PrepassRecollectState::AfterDisplayWhen {
                    outcome,
                    fontify_end: carried_end,
                    display_when,
                } => {
                    fontify_end = carried_end;
                    (outcome, false, false, Some(display_when))
                }
            };
            let accepted_display_when =
                self.frame_visual_histories
                    .get(frame_id)
                    .and_then(|history| {
                        history.display_when_for(DisplayWindowId::new(params.window_id))
                    });
            let display_when_reuse = run_prewalk_display_when
                && cursor_only_replay.is_some()
                && accepted_display_when.is_some_and(|retention| {
                    retention.record == crate::display_when::DisplayWhenRecord::Known
                        && retention.mode_line_generation
                            == evaluator.chrome_dirty().all_generation()
                })
                && !self
                    .prepared_window_ids
                    .contains(&DisplayWindowId::new(params.window_id))
                && !position_publication.is_synchronous_query()
                && !matches!(&viewport_resolution, ViewportResolutionPhase::Measure(_))
                && evaluator
                    .frame_manager()
                    .get(frame_id)
                    .and_then(|frame| frame.find_window(window_id))
                    .and_then(neovm_core::window::Window::window_end_valid)
                    == Some(true)
                && !evaluator.gnu_redisplay_refuses_cursor_movement()
                && !self.frame_other_windows_changed
                && !params.is_minibuffer()
                && !params.show_trailing_whitespace
                && !params.display_line_numbers.enabled()
                && scroll_coverage::inactive_overlay_arrows(evaluator, Some(buf_id));
            let display_when = if let Some(carried) = carried_display_when {
                carried
            } else if display_when_reuse {
                crate::display_when::DisplayWhenConditions::reused_without_evaluation(
                    neovm_core::emacs_core::value::Value::make_buffer(buf_id),
                )
            } else if params.source_extent == WindowSourceExtent::AccessibleEnd
                && evaluator.gnu_redisplay_hooks_policy_enabled()
            {
                let scan_end = window_start
                    .saturating_add(max_rows.saturating_mul(200))
                    .min(accessible_end);
                let condition_end =
                    mini_source_stop::condition_end_boundary(evaluator, params, scan_end);
                crate::display_when::evaluate_window_display_when_forms_at_end(
                    evaluator,
                    buf_id,
                    Some(window_id.0),
                    neovm_core::buffer::CharPos0::new(window_start.max(0) as usize),
                    neovm_core::buffer::CharPos0::new(scan_end.max(0) as usize),
                    display_target,
                    condition_end,
                )
            } else {
                let scan_end = window_start
                    .saturating_add(max_rows.saturating_mul(200))
                    .min(accessible_end);
                crate::display_when::evaluate_window_display_when_forms(
                    evaluator,
                    buf_id,
                    Some(window_id.0),
                    neovm_core::buffer::CharPos0::new(window_start.max(0) as usize),
                    neovm_core::buffer::CharPos0::new(scan_end.max(0) as usize),
                    display_target,
                )
            };
            let display_when = display_when
                .with_visit_recording()
                .with_carried_answers(lisp_ledger.display_when.carried(window_id));
            if !display_when.allows_body_reuse() {
                self.query_body_cache_state = QueryBodyCacheState::Disabled;
            }
            let retained_rows_unverified = self
                .frame_visual_histories
                .get(frame_id)
                .is_some_and(|history| history.any_display_when_unverified())
                && accepted_display_when.is_some_and(|retention| {
                    retention.record == crate::display_when::DisplayWhenRecord::Unknown
                });
            let (cursor_only_replay, scroll_replay, is_edit) = if display_when_reuse {
                (cursor_only_replay, None, false)
            } else if display_when.allows_body_reuse() && !retained_rows_unverified {
                (cursor_only_replay, scroll_replay, is_edit)
            } else {
                (None, None, false)
            };
            if evaluator.frame_manager().window_topology_generation() != topology_generation {
                return LeafLayoutAttempt::LogicalInputsChanged;
            }
            let freshness_after_fontification = if !run_prewalk_display_when {
                std::borrow::Cow::Owned(freshness_before_fontification)
            } else {
                let admission = crate::attempt_freshness::preparation_admission(
                    fontification_outcome,
                    &display_when,
                );
                let Some(after) = crate::attempt_freshness::after_span(
                    evaluator,
                    &freshness_before_fontification,
                    admission,
                    freshness_mode,
                ) else {
                    return LeafLayoutAttempt::LogicalInputsChanged;
                };
                if after.as_ref() != &freshness_before_fontification {
                    if can_recollect_display_when
                        && !evaluator.has_mode_line_display_flow()
                        && !evaluator.redisplay_hook_flow_pending()
                        && evaluator.face_change_count() == face_change_before_prepass
                    {
                        return LeafLayoutAttempt::PrepassInputsChanged(Box::new(
                            PrepassRecollectState::AfterDisplayWhen {
                                outcome: fontification_outcome,
                                fontify_end,
                                display_when,
                            },
                        ));
                    }
                    return LeafLayoutAttempt::LogicalInputsChanged;
                }
                after
            };
            (
                fontification_outcome,
                display_when,
                display_when_reuse,
                cursor_only_replay,
                scroll_replay,
                is_edit,
                freshness_after_fontification,
            )
        };

        // Conditional display can install fontification callbacks during
        // preparation. Check the final callback state before bounding rows.
        let callback_params;
        let params = if params.query_target.is_some()
            && window_source_has_fontification_callbacks(evaluator, params.buffer_id)
        {
            callback_params = {
                let mut resolved = params.clone();
                resolved.query_target = None;
                resolved
            };
            &callback_params
        } else {
            params
        };

        let scroll_dvpos = scroll_replay
            .as_ref()
            .map(|replay| replay.dvpos)
            .unwrap_or(0.0);
        // `params` already names the semantic display source chosen before
        // fontification and incremental classification.
        let mut layout_buffer = match evaluator.buffer_manager().get(buf_id) {
            Some(buffer) => {
                let overlay_span = if overlay_view_full {
                    None
                } else {
                    self.overlay_view_char_span_for(params, buffer)
                };
                super::neovm_bridge::LayoutBufferSnapshot::from_buffer_for_window_with_symbols(
                    buffer,
                    evaluator.variables(),
                    Some(visible_char_bound(params)),
                    overlay_span,
                    display_target,
                    evaluator.symbol_mode(),
                    &evaluator.owned_root_retainer(),
                )
                .with_accessible_end(neovm_core::buffer::CharPos0::new(
                    params.accessible_end_charpos().get().max(0) as usize,
                ))
                .with_display_when(display_when)
            }
            None => {
                tracing::debug!("layout_window_rust: buffer {} not found", params.buffer_id);
                self.frame_output
                    .render_window_info(WindowFrameInfoRenderRequest::new(
                        params,
                        live_window_frame_metadata(evaluator, buf_id),
                        // No body walk ran, so no line-number glyphs were
                        // produced and there is no field on screen to report.
                        None,
                    ));
                window_snapshot = Some(WindowPresentationSnapshot::live(WindowDisplaySnapshot {
                    window_id,
                    cell_origin: neovm_core::window::geometry::CellOrigin::new(
                        params.left_col,
                        params.top_line,
                    ),
                    regions: layout_box.regions(),
                    regions_materialized: false,
                    ..Default::default()
                }));
                return LeafLayoutAttempt::Completed {
                    outcome: WindowLayoutOutcome::Skipped,
                    window_end_attempt: None,
                    snapshot: window_snapshot,
                    produced_key: None,
                };
            }
        };
        // GNU resolves an automatic composition's rule lazily while the
        // display iterator reaches it; the snapshot is immutable across the
        // row walk (and may feed a worker), so resolve the visible spans'
        // gstring widths once, here, with the evaluator in scope.
        //
        // A cache miss can enter Lisp (the matched rule's FUNC). This call
        // sits after `freshness_after_fontification` and before the buffer
        // render's own capture, so a rule that moved any logical input fails
        // the BufferBody revalidation below and retries instead of publishing
        // rows built from pre-call inputs; because the shaped gstring is then
        // cached, the retry does not enter Lisp again (GNU's `autocmp_chars`
        // skips the rule for a gstring with a non-nil ID).
        //
        // The first pass bounds the rows this layout will actually produce:
        // one screenful plus one line on a first display, or just the
        // replay's regenerated band on a scroll.  That character count is a
        // heuristic (proportional fonts fit more characters per row than
        // `text_width / char_width`), so the post-walk coverage check below
        // verifies every painted automatic span was resolved and
        // widens/retries instead of letting the row measure the raw text,
        // which is exactly the pre-fix wrong advance.
        let mut resolved_composition_range: Option<std::ops::Range<usize>> = None;
        if matches!(
            display_target,
            crate::display_property::DisplayPropertyTarget::Graphical
        ) {
            let rows = params.source_interpretation_rows();
            let cols = (params.text_bounds.width / params.char_width.max(1.0)).ceil() as usize;
            let screenful = rows.saturating_mul(cols).max(1);
            let visible_start = params.window_start.max(0) as usize;
            let mut resolve_start = visible_start.saturating_sub(cols);
            let mut resolve_end = visible_start.saturating_add(screenful).saturating_add(cols);
            // A scroll replay regenerates only the newly exposed rows; the
            // reused rows are not re-measured and keep their advances.  Bound
            // both ends to that regenerated band so a 4 px scroll resolves at
            // most the newly exposed line's compositions, exactly like GNU
            // shaping a composition when its iterator first reaches it.  A
            // frame that exposes no whole new row resolves only the two-line
            // band around the edge row.
            if let Some((replay_walk_start, exposed_rows)) = replay_composition_walk {
                // Keep one line of margin on each side: the iterator may
                // reseat onto a composition beginning just above the
                // regenerated start, and a wrapped/narrow line can hold more
                // characters per row than `cols`.  Zero newly exposed rows
                // still keeps two lines: the replay can re-lay the partially
                // visible edge row even when no whole row entered.
                resolve_start = resolve_start.max(replay_walk_start.saturating_sub(cols));
                let band = exposed_rows.saturating_add(2).saturating_mul(cols);
                resolve_end = resolve_end.min(replay_walk_start.saturating_add(band));
            }
            if let Some((from, through)) = automatic_composition_resolve {
                resolve_start = resolve_start.min(from);
                resolve_end = resolve_end.max(through);
            }
            // Reuse the previous layout's shaped widths when the inputs that
            // decide them did not move.  This is the GNU matrix-row reuse for
            // compositions: a scroll that keeps faces, overlays and text
            // unchanged does not re-enter `font-at` or the rule search.
            let (composition_buffer_modiff, composition_overlay_modified_tick) = evaluator
                .buffer_manager()
                .get(buf_id)
                .map(|buffer| (buffer.modified_tick(), buffer.overlay_modified_tick()))
                .unwrap_or((i64::MIN, i64::MIN));
            let composition_face_change_count = evaluator.face_change_count();
            let composition_display_var_change_count = evaluator.display_var_change_count();
            let composition_body_redisplay = evaluator.body_redisplay_revision(window_id, buf_id);
            let composition_fontset_generation =
                neovm_core::emacs_core::fontset::fontset_generation();
            let composition_window_parameters_generation = evaluator
                .frame_manager()
                .get(frame_id)
                .and_then(|frame| frame.window_layout_inputs(window_id))
                .map(|inputs| inputs.window_parameters_generation)
                .unwrap_or(0);
            let composition_char_width_bits = params.char_width.to_bits();
            let composition_window = DisplayWindowId::new(params.window_id);
            // The rejection dump costs a cache lookup even when nobody is
            // listening; only build it when this target is enabled.
            if tracing::enabled!(
                target: "neomacs_layout_engine::composition_resolver",
                tracing::Level::DEBUG
            ) && let Some(previous) = self.retained_composition_advances.get(&composition_window)
            {
                tracing::debug!(
                    target: "neomacs_layout_engine::composition_resolver",
                    previous = ?(
                        previous.buffer_modiff,
                        previous.overlay_modified_tick,
                        previous.face_change_count,
                        previous.display_var_change_count,
                        previous.fontset_generation,
                        previous.window_parameters_generation,
                        previous.char_width_bits,
                    ),
                    current = ?(
                        composition_buffer_modiff,
                        composition_overlay_modified_tick,
                        composition_face_change_count,
                        composition_display_var_change_count,
                        composition_fontset_generation,
                        composition_window_parameters_generation,
                        composition_char_width_bits,
                    ),
                    body_unchanged = previous.body_redisplay == composition_body_redisplay,
                    "composition cache inputs"
                );
            }
            let seeded_composition_advances = self
                .retained_composition_advances
                .get(&composition_window)
                .filter(|cache| {
                    cache.is_current_for(
                        buf_id,
                        composition_buffer_modiff,
                        composition_overlay_modified_tick,
                        composition_face_change_count,
                        composition_display_var_change_count,
                        composition_body_redisplay,
                        composition_fontset_generation,
                        composition_window_parameters_generation,
                        composition_char_width_bits,
                    )
                })
                .map(|cache| cache.advances.clone())
                .unwrap_or_default();
            let composition_resolve_started = std::time::Instant::now();
            layout_buffer.resolve_automatic_composition_advances(
                evaluator,
                buf_id,
                resolve_start..resolve_end,
                &seeded_composition_advances,
            );
            tracing::debug!(
                target: "neomacs_layout_engine::composition_resolver",
                micros = composition_resolve_started.elapsed().as_micros() as u64,
                seeded = seeded_composition_advances.len(),
                resolve_start,
                resolve_end,
                "composition resolve timing"
            );
            let resolved_advances = layout_buffer.resolved_automatic_composition_advances();
            let retained_advances = retained_composition_advances_for_spans(
                layout_buffer.automatic_composition_span_ranges(),
                &resolved_advances,
                &seeded_composition_advances,
            );
            self.retained_composition_advances.insert(
                composition_window,
                RetainedCompositionAdvances {
                    buffer_id: buf_id,
                    buffer_modiff: composition_buffer_modiff,
                    overlay_modified_tick: composition_overlay_modified_tick,
                    face_change_count: composition_face_change_count,
                    display_var_change_count: composition_display_var_change_count,
                    body_redisplay: composition_body_redisplay,
                    fontset_generation: composition_fontset_generation,
                    window_parameters_generation: composition_window_parameters_generation,
                    char_width_bits: composition_char_width_bits,
                    advances: retained_advances,
                },
            );
            resolved_composition_range = Some(resolve_start..resolve_end);
        }
        let buffer = &layout_buffer;

        // Point the face resolver at this window's parameters so a
        // `(:window PARAMETER VALUE)` `:filtered` face remap — e.g. indent-bars'
        // per-window stipple-rotation remap keyed on the `indent-bars-whr`
        // window parameter — can match. GNU threads the window into
        // `evaluate_face_filter`; the frame-shared resolver reads it back via an
        // interior-mutable slot set at each window boundary. Cleared for frame
        // chrome below so a `:window` filter can never match there.
        face_resolver.set_current_window_parameters(
            evaluator.frame_manager().window_parameters_pairs(window_id),
        );
        // Honor overlay `window` properties (hl-line non-sticky) for this window.
        face_resolver.set_current_window_id(Some(params.window_id as u64));

        // Capture buffer name as owned String for use in mode-line fallback.
        // This avoids holding a borrow on `evaluator` through eval calls.
        let buffer_name = buffer.name().to_owned();
        let mut window_end_attempt =
            evaluator.begin_redisplay_window_end_attempt(frame_id, window_id, buf_id);
        // Pure attempt-local admission captured after conditional replay refusal.
        // No plan, snapshot or Lisp owner is retained by this numeric decision.
        let admitted_edit_sync_fontify_attempt = is_edit
            && scroll_replay
                .as_ref()
                .is_some_and(|replay| replay.edit && replay.sync.is_some());
        let render_request = BufferWindowRenderRequest::new(
            frame_id,
            window_id,
            params,
            frame_params,
            layout_box,
            buf_id,
            buffer,
            &buffer_name,
            reserve_right_border_col,
            resolved_window_start,
        )
        .with_position_publication(site_publication)
        .with_remeasure_chrome(remeasure_chrome)
        .with_forward_viewport_measurement(match &viewport_resolution {
            ViewportResolutionPhase::Measure(measurement) => Some(measurement.clone()),
            ViewportResolutionPhase::Resolve | ViewportResolutionPhase::Commit(_) => None,
        });
        // Conditional display/face callbacks above may have changed a source
        // after Phase A admission. Validate again at consumption, not just lookup.
        let sources_current = previous_sources
            .as_ref()
            .is_some_and(|sources| sources.can_replay() && sources.unchanged());
        let cursor_only_replay = if sources_current {
            cursor_only_replay.map(|mut replay| {
                replay.ordinary_cursor_source = previous_sources
                    .as_ref()
                    .and_then(|sources| sources.ordinary_cursor_source_proof());
                replay
            })
        } else {
            None
        };
        let scroll_replay = if sources_current { scroll_replay } else { None };
        let canonical_source_walk = cursor_only_replay.is_none() && scroll_replay.is_none();
        let unchanged_body_replay = cursor_only_replay.is_some();
        let produced_freshness = match evaluator
            .window_display_snapshot_freshness(frame_id, window_id, buf_id)
        {
            Some(token) => {
                crate::buffer_source::render_attempt::ProducedSnapshotIdentity::Live(token)
            }
            None if position_publication == WindowPositionPublication::InactiveEchoArea => {
                crate::buffer_source::render_attempt::ProducedSnapshotIdentity::InactiveEchoArea
            }
            None if position_publication
                == WindowPositionPublication::SynchronousQueryTextExtent =>
            {
                // Only the explicit TextExtent scope admits a projected
                // query source without live-window snapshot identity.
                crate::buffer_source::render_attempt::ProducedSnapshotIdentity::ProjectedTextExtent
            }
            None => {
                if let Some(attempt) = window_end_attempt.take() {
                    evaluator.reject_redisplay_window_end_attempt(attempt);
                }
                return LeafLayoutAttempt::LogicalInputsChanged;
            }
        };
        let produced_key = RetainedWindowKey::from_params(params, *layout_box, evaluator);
        let produced_modiff = evaluator
            .buffer_manager()
            .get(buf_id)
            .map(neovm_core::buffer::Buffer::modified_tick)
            .unwrap_or(0);
        let source_walk = if canonical_source_walk {
            crate::buffer_source::render_attempt::BodySourceWalk::Canonical
        } else if let Some(previous) = previous_sources.clone() {
            if unchanged_body_replay {
                crate::buffer_source::render_attempt::BodySourceWalk::Unchanged(previous)
            } else {
                crate::buffer_source::render_attempt::BodySourceWalk::Partial(previous)
            }
        } else {
            if let Some(attempt) = window_end_attempt.take() {
                evaluator.reject_redisplay_window_end_attempt(attempt);
            }
            return LeafLayoutAttempt::LogicalInputsChanged;
        };
        let body_completion = crate::buffer_source::render_attempt::BodyCompletionRequest::new(
            &mut source_capture,
            freshness_after_fontification.as_ref().clone(),
            produced_key,
            produced_freshness,
            produced_modiff,
            source_walk,
        );
        let chrome_eval_mode = match attempt_mode {
            FrameAttemptMode::Converging => {
                crate::display_status_line::WindowChromeEvalMode::Converging {
                    captured: (capture_chrome_for_exhaustion
                        && !position_publication.is_synchronous_query())
                    .then_some(&mut lisp_ledger.captured_chrome_outputs),
                }
            }
            FrameAttemptMode::ExhaustionSnapshot => {
                crate::display_status_line::WindowChromeEvalMode::ExhaustionSnapshot {
                    captured: &lisp_ledger.captured_chrome_outputs,
                }
            }
        };
        let render_outcome = render_request.render_into(
            BufferSourceRenderAttemptContext::from_frame_output_owner(
                &mut self.frame_output,
                evaluator,
                &mut self.font_metrics,
                face_services,
                face_attempt.clone(),
                &mut window_snapshot,
                body_completion,
                chrome_eval_mode,
            ),
            &mut self.text_buf,
            remaining_visibility_retries,
            cursor_only_replay,
            scroll_replay,
        );

        // GNU's composition width comes from the shaped gstring, never from
        // measuring the raw text. The visible-range resolution pass above is
        // a character-count heuristic, so check the walk it produced: every
        // automatic-composition span the walk could have painted must lie in
        // the range the resolver covered. Otherwise discard the attempt and
        // ask the loop for a wider pass (`usize::MAX` on the second miss
        // covers every captured span, so this terminates).
        if let (
            BufferSourceRenderAttemptOutcome::Finished {
                redisplay_positions,
                cursor_only,
                reused_matrix_rows,
                ..
            },
            Some(resolved),
        ) = (&render_outcome, resolved_composition_range.clone())
            && !*cursor_only
        {
            let semantic_walk_start =
                usize::try_from(resolved_window_start.get().max(0)).unwrap_or(0);
            // Only an attempt that really reused matrix rows walked from the
            // replay's `walk_start`; a refused replay is re-laid without it
            // and then covers the semantic range, where this check exposes any
            // span the narrow range missed.
            let walk_start = if reused_matrix_rows.is_some() {
                replay_composition_walk
                    .map(|(walk_start, _)| walk_start)
                    .unwrap_or(semantic_walk_start)
            } else {
                semantic_walk_start
            };
            let walk_end =
                neovm_core::buffer::CharPos0::from_lisp(redisplay_positions.window_end_lisp())
                    .get();
            // With reused rows the walk paints only the regenerated band:
            // it starts at `walk_start` and either runs to the window end
            // (downward scroll) or stops where the reused rows resume
            // (upward scroll / edit).  Spans in the reused part already carry
            // their advances.  A frame that exposes no whole new row keeps
            // the two-line band the resolver used: the replay may re-lay the
            // partially visible edge row.
            let coverage_end = if reused_matrix_rows.is_some() {
                match (replay_first_reused_below_start, replay_composition_walk) {
                    (Some(sync), _) => sync.min(walk_end),
                    (None, Some((walk_start, 0))) => {
                        let cols =
                            (params.text_bounds.width / params.char_width.max(1.0)).ceil() as usize;
                        walk_start
                            .saturating_add(cols.saturating_mul(2))
                            .min(walk_end)
                    }
                    (None, _) => walk_end,
                }
            } else {
                walk_end
            };
            if let Some(missed) = uncovered_automatic_composition_span_range(
                layout_buffer.automatic_composition_span_starts(),
                walk_start,
                coverage_end,
                &resolved,
            ) {
                tracing::debug!(
                    target: "neomacs_layout_engine::composition_resolver",
                    missed_start = missed.start,
                    missed_end = missed.end,
                    walk_start,
                    coverage_end,
                    resolve_start = resolved.start,
                    resolve_end = resolved.end,
                    "automatic composition coverage retry"
                );
                if let Some(attempt) = window_end_attempt.take() {
                    evaluator.reject_redisplay_window_end_attempt(attempt);
                }
                let cols = (params.text_bounds.width / params.char_width.max(1.0)).ceil() as usize;
                let resolve_from = resolved.start.min(missed.start);
                let resolve_through = resolved
                    .end
                    .max(missed.end)
                    .max(walk_end.saturating_add(cols));
                return LeafLayoutAttempt::AutomaticCompositionUnresolved {
                    resolve_from,
                    resolve_through,
                };
            }
        }

        // Body evidence was sealed before chrome. Late source mutations do
        // not discard those completed rows; ownership, physical geometry and
        // realized resources must still survive the callback boundary.
        let freshness_after_leaf = match &render_outcome {
            BufferSourceRenderAttemptOutcome::Finished {
                freshness_before_chrome,
                chrome_lisp_admission,
                ..
            }
            | BufferSourceRenderAttemptOutcome::EmptyBodyChromeMeasured {
                freshness_before_chrome,
                chrome_lisp_admission,
                ..
            } => crate::attempt_freshness::after_span(
                evaluator,
                freshness_before_chrome,
                *chrome_lisp_admission,
                freshness_mode,
            ),
            BufferSourceRenderAttemptOutcome::Skipped
            | BufferSourceRenderAttemptOutcome::LogicalInputsChanged
            | BufferSourceRenderAttemptOutcome::OverlayViewExhausted
            | BufferSourceRenderAttemptOutcome::GlyphMatrixCapacityExhausted
            | BufferSourceRenderAttemptOutcome::GnuMiniEobCursorUnavailable { .. }
            | BufferSourceRenderAttemptOutcome::Retry { .. }
            | BufferSourceRenderAttemptOutcome::ResolveViewport { .. }
            | BufferSourceRenderAttemptOutcome::RetryPointIntoWindow { .. }
            | BufferSourceRenderAttemptOutcome::ReplayMispredicted
            | BufferSourceRenderAttemptOutcome::SyncSourceHorizonExhausted { .. }
            | BufferSourceRenderAttemptOutcome::QuerySourceHorizonExhausted => evaluator
                .window_layout_attempt_freshness(frame_id, window_id, buf_id)
                .map(std::borrow::Cow::Owned),
        };
        let lisp_boundaries_remain_valid = match (&render_outcome, &freshness_after_leaf) {
            (
                BufferSourceRenderAttemptOutcome::Finished {
                    freshness_before_chrome,
                    ..
                }
                | BufferSourceRenderAttemptOutcome::EmptyBodyChromeMeasured {
                    freshness_before_chrome,
                    ..
                },
                Some(freshness_after_leaf),
            ) => freshness_before_chrome.remains_valid_across(
                freshness_after_leaf,
                neovm_core::window::WindowLayoutLispBoundary::WindowChrome,
            ),
            (_, Some(freshness_after_leaf)) => {
                freshness_after_leaf.as_ref() == freshness_after_fontification.as_ref()
            }
            (_, None) => false,
        };
        if evaluator.frame_manager().window_topology_generation() != topology_generation
            || !lisp_boundaries_remain_valid
        {
            if let Some(attempt) = window_end_attempt.take() {
                evaluator.reject_redisplay_window_end_attempt(attempt);
            }
            return LeafLayoutAttempt::LogicalInputsChanged;
        }

        // Two fontification requests precede this comment's pass.  The
        // speculative PREPASS above covers the budget span for hooks that MARK
        // text; a hook that declines ends that request at its first declined
        // stop.  The VISIBLE pass below starts from the completed row walk's
        // own positions, so it follows the stops GNU's iterator actually
        // displays (handle_fontified_prop, src/xdisp.c:4702-4786), including
        // GNU's one-row probe past the window end when the walk qualifies.
        // Sparse spans are fontified before retrying from a fresh snapshot.
        //
        // Convergence with a mixed hook (marks one stop, declines a later
        // one): the prepass reports Fontified, which forces a retry; that
        // retry's prepass starts on the now-fontified text and its first
        // remaining unfontified stop is declined, yielding
        // CallbacksUnchanged; the visible pass then restarts from the window
        // start and covers the declined stops.
        //
        // GNU's reuse decision, not Neo's: `partial_line_height` is skipped
        // only when `current_matrix_up_to_date_p` held at entry AND the walk
        // actually took a reuse path.  Neo's own reuse of rows under a stale
        // window end is an optimization and must not hide the probe-row hook
        // calls GNU makes.  The three per-buffer/per-window inputs of the
        // predicate are read from the retained matrix of the last accepted
        // display, the state GNU keeps in `w->current_matrix` plus
        // `w->last_modified`/`last_overlay_modified`.
        let retained_matrix = self
            .retained_window_matrices
            .get(&DisplayWindowId::new(params.window_id));
        let buffer_state = evaluator.buffer_manager().get(buf_id);
        // GNU `window_outdated` compares `w->last_modified` with `BUF_MODIFF`
        // and `w->last_overlay_modified` with `BUF_OVERLAY_MODIFF`
        // (src/window.c:231-236).  `modified_tick` is MODIFF, which also moves
        // for `restore-buffer-modified-p` (src/buffer.c:1630-1631) and for
        // text-property changes, so it is the faithful half; the retained
        // matrix records it at acceptance, after redisplay's own fontification.
        let window_outdated = match (retained_matrix, buffer_state) {
            (Some(retained), Some(buffer)) => {
                retained.gnu_accuracy.as_ref().is_none_or(|accuracy| {
                    buffer.modified_tick() != accuracy.modified_tick
                        || buffer.overlay_modified_tick() != accuracy.overlay_modified_tick
                })
            }
            _ => false,
        };
        // GNU `prevent_redisplay_optimizations_p` has its own per-buffer
        // witness (raised by set-buffer-redisplay and force-mode-line-update).
        // A moved body revision -- GNU `force-window-update` -- refuses too.
        let prevent_redisplay_optimizations = retained_matrix.is_some_and(|retained| {
            retained.key.body_redisplay != evaluator.body_redisplay_revision(window_id, buf_id)
                || retained.gnu_accuracy.as_ref().is_none_or(|accuracy| {
                    accuracy.redisplay_optimizations
                        != evaluator.prevent_redisplay_optimizations_revision(buf_id)
                })
        });
        // GNU reads `clip_changed` only after `reconsider_clip_changes (w)`
        // (src/xdisp.c:20486), which clears it when the window's current
        // matrix already matches the buffer and its restriction
        // (src/xdisp.c:17271-17277).  The retained key carries exactly those
        // matrix facts: buffer identity, BEGV and ZV.
        let clip_changed = match (retained_matrix, buffer_state) {
            (Some(retained), Some(buffer)) => {
                // The key stores BEGV and ZV as `CharPos0` counts
                // (neovm_bridge.rs:2029-2031), so compare the same form.
                let matrix_matches = window_end_valid_at_entry
                    && retained.gnu_accuracy.as_ref().is_some_and(|accuracy| {
                        accuracy.buffer_id == buf_id
                            && accuracy.accessible_chars.start() == buffer.point_min_char_pos()
                            && accuracy.accessible_chars.end() == buffer.point_max_char_pos()
                    });
                buffer.mode_line_clip_changed() && !matrix_matches
            }
            (Some(_), None) => false,
            (None, Some(buffer)) => buffer.mode_line_clip_changed(),
            (None, None) => false,
        };
        let gnu_matrix_reuse = GnuMatrixReuse::at_entry(
            window_end_valid_at_entry,
            evaluator.gnu_redisplay_refuses_cursor_movement(),
            self.frame_other_windows_changed,
            clip_changed,
            prevent_redisplay_optimizations,
            window_outdated,
        );
        let finished_walk = match &render_outcome {
            BufferSourceRenderAttemptOutcome::Finished {
                redisplay_positions,
                cursor_only,
                reused_matrix_rows,
                ..
            } => Some((
                redisplay_positions.window_end_lisp(),
                gnu_matrix_reuse.skips_probe(*cursor_only || reused_matrix_rows.is_some()),
            )),
            _ => None,
        };
        // Set when this walk's own visible fontification is a source-only
        // retry. Display-when then compares against that post-hook token, so
        // the fontifier's admitted source edits are not a second cause.
        let mut fontification_source_baseline: Option<
            neovm_core::window::WindowLayoutAttemptFreshness,
        > = None;
        if attempt_mode == FrameAttemptMode::Converging
            && let Some((window_end, skip_probe)) = finished_walk
        {
            // GNU `partial_line_height` (`src/xdisp.c:11630-11661`): when
            // `try_window` ran with TRY_WINDOW_CHECK_MARGINS, it re-walks to
            // the window bottom and then scans the FIRST screen row after the
            // window end, entering fontification hooks at that row's stops
            // (the sole call site is `try_window`, src/xdisp.c:21718).  That
            // row starts at the walk's published window end.
            let probe = if skip_probe {
                neovm_core::emacs_core::xdisp::ProbeRow::None
            } else {
                fontification_probe_row(evaluator, params, frame_id, window_id, buf_id, window_end)
            };
            let coverage = window_snapshot
                .iter()
                .rev()
                .map(WindowPresentationSnapshot::display_snapshot)
                .find(|snapshot| snapshot.window_id == window_id)
                .map(|snapshot| {
                    // A declining pre-walk hook did not fontify the budget
                    // span, so the walk's visible positions must be
                    // reconsidered by the coverage pass.  A marking or
                    // no-callback prepass keeps the old boundary.
                    let prepass_end = if matches!(
                        fontification_outcome,
                        neovm_core::emacs_core::xdisp::RedisplayFontificationOutcome::CallbacksUnchanged
                    ) {
                        neovm_core::buffer::CharPos0::new(window_start.max(0) as usize)
                    } else {
                        neovm_core::buffer::CharPos0::new(fontify_end.max(0) as usize)
                    };
                    if admitted_edit_sync_fontify_attempt {
                        VisibleFontificationCoverage::inspect_for_edit_sync(
                            buffer,
                            snapshot,
                            prepass_end,
                            true,
                        )
                    } else {
                        // Finished plus the body/chrome freshness and topology
                        // checks above certify this attempt's actual rows.
                        // Numeric refusal retains the exact global iterator.
                        VisibleFontificationCoverage::inspect_completed(
                            buffer,
                            snapshot,
                            prepass_end,
                        )
                    }
                })
                .unwrap_or(VisibleFontificationCoverage::Complete);

            if let VisibleFontificationCoverage::Requires(plan) = coverage {
                // Every span here comes from the same certified walk. Fontify
                // all of them before replaying: a successful fontification
                // necessarily changes the buffer's property ticks, so testing
                // freshness inside the loop would replay after the first span
                // and leave the rest of the plan unfontified. A folded Org
                // buffer yields dozens of sparse visible runs -- more than the
                // retry budget -- so that shape could never converge.
                let mut requires_replay = false;
                let spans: Vec<_> = plan.spans().collect();
                let last_span = spans.len().checked_sub(1);
                for (index, span) in spans.iter().enumerate() {
                    let span_probe = if Some(index) == last_span {
                        probe
                    } else {
                        neovm_core::emacs_core::xdisp::ProbeRow::None
                    };
                    let outcome = lisp_ledger.rows_fontification.ensure(
                        evaluator,
                        buf_id,
                        span.start(),
                        span.end(),
                        params.point_charpos().get() as i64 + 1,
                        neovm_core::emacs_core::xdisp::RedisplayFontificationRequest::VisibleWalk,
                        span_probe,
                    );
                    requires_replay |= outcome.requires_layout_retry();
                    if evaluator.frame_manager().window_topology_generation() != topology_generation
                    {
                        if let Some(attempt) = window_end_attempt.take() {
                            evaluator.reject_redisplay_window_end_attempt(attempt);
                        }
                        return LeafLayoutAttempt::LogicalInputsChanged;
                    }
                }
                // Check freshness once, after the whole plan ran. The property
                // ticks it changed were produced by the fontification this
                // attempt itself requested.
                let freshness_after_visible_fontification =
                    evaluator.window_layout_attempt_freshness(frame_id, window_id, buf_id);
                if requires_replay
                    || freshness_after_visible_fontification.as_ref()
                        != freshness_after_leaf.as_deref()
                {
                    let seals_completed_body = matches!(
                        position_publication,
                        WindowPositionPublication::Redisplay
                            | WindowPositionPublication::RedisplayResumedScrollHook
                            | WindowPositionPublication::RedisplayMiniEobFallback
                    );
                    let source_only = match (
                        freshness_after_leaf.as_ref(),
                        freshness_after_visible_fontification.as_ref(),
                    ) {
                        (Some(before), Some(after)) => before.remains_valid_across(
                            after,
                            neovm_core::window::WindowLayoutLispBoundary::WindowChrome,
                        ),
                        _ => false,
                    };
                    // A finished body may keep its rows across a source edit
                    // this fontification itself performed. Finish the siblings
                    // and seal; the frame loop retries, and publishes this
                    // attempt when the budget is spent. Owner, physical, and
                    // topology changes stay on the discard path above.
                    if seals_completed_body && source_only {
                        fontification_source_baseline = freshness_after_visible_fontification;
                        if lisp_ledger.source_only_retry.is_none() {
                            lisp_ledger.source_only_retry =
                                Some(DisplayWindowId::new(params.window_id));
                        }
                    } else {
                        if let Some(attempt) = window_end_attempt.take() {
                            evaluator.reject_redisplay_window_end_attempt(attempt);
                        }
                        return LeafLayoutAttempt::LogicalInputsChanged;
                    }
                }
            }
        }

        // The `when` clauses the walk took by the structural rule: evaluate
        // them now with GNU's bindings, and replay the frame once if an
        // answer disagrees with the rule the rows were drawn by.
        if matches!(
            &render_outcome,
            BufferSourceRenderAttemptOutcome::Finished { .. }
        ) {
            if attempt_mode == FrameAttemptMode::ExhaustionSnapshot {
                lisp_ledger
                    .display_when
                    .settle_window(window_id, crate::display_when::DisplayWhenRecord::Unknown);
            } else {
                let conditions = layout_buffer.display_when_conditions();
                let evaluates_lisp = conditions.has_recorded_visits();
                let settlement = crate::display_when::settle_display_when_visits(
                    evaluator,
                    buf_id,
                    window_id,
                    conditions,
                    display_target,
                    &mut lisp_ledger.display_when,
                );
                let display_when_baseline = fontification_source_baseline
                    .as_ref()
                    .or(freshness_after_leaf.as_deref());
                if evaluates_lisp
                    && (evaluator.frame_manager().window_topology_generation()
                        != topology_generation
                        || evaluator
                            .window_layout_attempt_freshness(frame_id, window_id, buf_id)
                            .as_ref()
                            != display_when_baseline)
                {
                    if let Some(attempt) = window_end_attempt.take() {
                        evaluator.reject_redisplay_window_end_attempt(attempt);
                    }
                    return LeafLayoutAttempt::LogicalInputsChanged;
                }
                match settlement {
                    crate::display_when::DisplayWhenSettlement::Replay => {
                        if let Some(attempt) = window_end_attempt.take() {
                            evaluator.reject_redisplay_window_end_attempt(attempt);
                        }
                        return LeafLayoutAttempt::DisplayWhenReplay;
                    }
                    crate::display_when::DisplayWhenSettlement::Settled(record) => {
                        // A reuse without evaluation keeps the rows, and so the
                        // record, it reused.
                        let record = if display_when_reuse
                            && record == crate::display_when::DisplayWhenRecord::NoSites
                        {
                            crate::display_when::DisplayWhenRecord::Known
                        } else {
                            record
                        };
                        tracing::trace!(target: "neomacs_layout_engine::display_when",
                            window = window_id.0, ?record, display_when_reuse,
                            "walk's `when' record settled");
                        // The record lives with the frame attempt's display-when
                        // ledger (a stack local), not in LayoutEngine: it must not
                        // add bytes to the thread-local redisplay runtime.
                        lisp_ledger.display_when.settle_window(window_id, record);
                    }
                }
            }
        }

        drop(freshness_after_leaf);
        let completed_body_key;
        let (redisplay_positions, effective_default_face, line_number_field) = match render_outcome
        {
            BufferSourceRenderAttemptOutcome::GlyphMatrixCapacityExhausted => {
                if let Some(attempt) = window_end_attempt.take() {
                    evaluator.reject_redisplay_window_end_attempt(attempt);
                }
                // The producer certifies a real GUI viewport row crossing the
                // semantic desired-matrix bound while text remains visible.
                // Query, measurement and growing-source grids cannot request it.
                if !params.window_system
                    || params.source_extent != WindowSourceExtent::Viewport
                    || position_publication.is_synchronous_query()
                    || params.measurement_pixels.is_some()
                    || params.measurement_rows.is_some()
                {
                    return LeafLayoutAttempt::MatrixCapacityUnavailable;
                }
                if let Some(display) = evaluator
                    .frame_manager_mut()
                    .get_mut(frame_id)
                    .and_then(|frame| frame.find_window_mut(window_id))
                    .and_then(|window| window.display_mut())
                {
                    let scale = &display.glyph_matrix_row_scale;
                    if scale.grow_if(params.glyph_matrix_row_scale).is_none()
                        && scale.current() == params.glyph_matrix_row_scale
                    {
                        // Same-value refusal means dormant GUI state or checked
                        // overflow. Neither can converge by retrying unchanged.
                        return LeafLayoutAttempt::MatrixCapacityUnavailable;
                    }
                }
                // Recollect numeric inputs after growth or stale CAS. This
                // does not move start/point or spend a visibility retry.
                return LeafLayoutAttempt::LogicalInputsChanged;
            }
            BufferSourceRenderAttemptOutcome::LogicalInputsChanged => {
                reject_changed_cursor_sources(evaluator, &mut window_end_attempt, true);
                return LeafLayoutAttempt::LogicalInputsChanged;
            }
            BufferSourceRenderAttemptOutcome::OverlayViewExhausted => {
                if let Some(attempt) = window_end_attempt.take() {
                    evaluator.reject_redisplay_window_end_attempt(attempt);
                }
                return LeafLayoutAttempt::OverlayViewExhausted;
            }
            BufferSourceRenderAttemptOutcome::GnuMiniEobCursorUnavailable { boundary } => {
                if let Some(attempt) = window_end_attempt.take() {
                    evaluator.reject_redisplay_window_end_attempt(attempt);
                }
                let Ok(site) = lisp_ledger.mini_eob_scroll.next_site(
                    evaluator,
                    frame_id,
                    window_id,
                    buf_id,
                    resolved_window_start,
                    params,
                    layout_box.body().height,
                    topology_generation,
                    boundary,
                ) else {
                    return LeafLayoutAttempt::LogicalInputsChanged;
                };
                // Commit the independently resolved decision marker, even
                // when the producer proved the prepared source line is stable.
                let _ = evaluator.publish_redisplay_window_start(
                    frame_id,
                    window_id,
                    crate::coords::layout_i64_char_pos_to_lisp_char_pos(site.window_start().get()),
                );
                let effect = LayoutEffect::RunWindowScrollFunctions(
                    crate::layout_effect::WindowScrollEffect::new(site),
                );
                if attempt_mode == FrameAttemptMode::Converging
                    && evaluator.window_scroll_functions_may_run(window_id)
                {
                    return LeafLayoutAttempt::Effect(effect);
                }
                // With no Lisp hook, acknowledge the same semantic decision
                // and re-enter the real producer instead of manufacturing a
                // second callback or treating a missing hook as convergence.
                lisp_ledger.acknowledge_scroll_hook(&effect);
                return LeafLayoutAttempt::Retry(Box::new(WindowLayoutRetry {
                    params: params.clone(),
                    remaining_visibility_retries,
                    viewport_resolution: ViewportResolutionPhase::Commit(site.window_start()),
                }));
            }
            BufferSourceRenderAttemptOutcome::EmptyBodyChromeMeasured { measured, .. }
                if attempt_mode == FrameAttemptMode::Converging
                    && layout_box.chrome() != measured =>
            {
                if let Some(attempt) = window_end_attempt.take() {
                    evaluator.reject_redisplay_window_end_attempt(attempt);
                }
                return LeafLayoutAttempt::Completed {
                    outcome: WindowLayoutOutcome::NeedsRelayout {
                        assumed: layout_box.chrome(),
                        measured,
                    },
                    window_end_attempt: None,
                    snapshot: window_snapshot,
                    produced_key: None,
                };
            }
            BufferSourceRenderAttemptOutcome::Skipped
            | BufferSourceRenderAttemptOutcome::EmptyBodyChromeMeasured { .. } => {
                self.frame_output
                    .render_window_info(WindowFrameInfoRenderRequest::new(
                        params,
                        live_window_frame_metadata(evaluator, buf_id),
                        // No body walk ran, so no line-number glyphs were
                        // produced and there is no field on screen to report.
                        None,
                    ));
                self.publish_window_fringe_backgrounds(buffer, face_resolver, layout_box);
                window_snapshot = Some(WindowPresentationSnapshot::live(WindowDisplaySnapshot {
                    window_id,
                    cell_origin: neovm_core::window::geometry::CellOrigin::new(
                        params.left_col,
                        params.top_line,
                    ),
                    regions: layout_box.regions(),
                    regions_materialized: false,
                    ..WindowDisplaySnapshot::default()
                }));
                Self::mark_inactive_echo_snapshot_geometry_only(
                    &mut window_snapshot,
                    position_publication,
                );
                return LeafLayoutAttempt::Completed {
                    outcome: WindowLayoutOutcome::Skipped,
                    window_end_attempt,
                    snapshot: window_snapshot,
                    produced_key: None,
                };
            }
            BufferSourceRenderAttemptOutcome::ReplayMispredicted
            | BufferSourceRenderAttemptOutcome::SyncSourceHorizonExhausted { .. }
            | BufferSourceRenderAttemptOutcome::QuerySourceHorizonExhausted => {
                if let Some(attempt) = window_end_attempt.take() {
                    evaluator.reject_redisplay_window_end_attempt(attempt);
                }
                // The bounded edit-replay walk failed post-walk validation
                // (span re-wrapped / re-measured / lost position continuity).
                // Re-lay this window from scratch with no fast-path plan —
                // replay-free layout cannot mispredict, so this terminates.
                return LeafLayoutAttempt::Retry(Box::new(WindowLayoutRetry {
                    params: params.clone(),
                    remaining_visibility_retries,
                    viewport_resolution: viewport_resolution.clone(),
                }));
            }
            BufferSourceRenderAttemptOutcome::ResolveViewport {
                decision: ViewportDecision::NeedMoreMeasurement(measurement),
            } => {
                if let Some(attempt) = window_end_attempt.take() {
                    evaluator.reject_redisplay_window_end_attempt(attempt);
                }
                let mut measurement_params = params.clone();
                measurement_params.window_start = measurement.probe_window_start().get();
                measurement_params.previous_visible_end = None;
                return LeafLayoutAttempt::Retry(Box::new(WindowLayoutRetry {
                    params: measurement_params,
                    remaining_visibility_retries: remaining_visibility_retries.saturating_sub(1),
                    viewport_resolution: ViewportResolutionPhase::Measure(measurement),
                }));
            }
            BufferSourceRenderAttemptOutcome::ResolveViewport {
                decision:
                    ViewportDecision::PlaceRelativeToPoint {
                        lines_above_point,
                        fallback_window_start,
                    },
            } => {
                if let Some(attempt) = window_end_attempt.take() {
                    evaluator.reject_redisplay_window_end_attempt(attempt);
                }
                let freshness_before_motion =
                    evaluator.window_layout_attempt_freshness(frame_id, window_id, buf_id);
                let point = neovm_core::buffer::CharPos0::new(params.point.max(0) as usize);
                let resolved_start = if attempt_mode == FrameAttemptMode::ExhaustionSnapshot {
                    fallback_window_start
                } else {
                    evaluator
                        .redisplay_start_before_point_by_display_rows(
                            buf_id,
                            window_id,
                            point,
                            lines_above_point,
                        )
                        .map_or(fallback_window_start, |start| {
                            ResolvedWindowStart::from_layout_charpos(start.get() as i64)
                        })
                };
                tracing::debug!(
                    "layout_window_rust: point-relative placement win={} point={} lines_above={} fallback={} resolved={} remaining={}",
                    params.window_id,
                    point.get(),
                    lines_above_point,
                    fallback_window_start.get(),
                    resolved_start.get(),
                    remaining_visibility_retries,
                );
                // A display-motion error, or a motion engine that cannot
                // represent a better start for this display span, preserves
                // the semantic viewport. Do not spend the remaining budget
                // rediscovering the same no-op placement.
                //
                // A motion result equal to the start this attempt already
                // laid out from is the same story: layout from those inputs
                // is deterministic (the freshness check above pins them), so
                // re-committing it cannot change the producer's decision and
                // would only cycle.  Stop the chain there; the numeric
                // retry budget is a bound, not a convergence plan.  Measure
                // phases are exempt: their start is a probe, and a commit of
                // the same position re-runs the producer *without* the probe
                // continuation, which can legitimately converge differently.
                //
                // A zero budget is the producer's signal to finish at the
                // committed start (`ViewportDecision::resolve_with_budget`,
                // GNU `redisplay_window`'s `done:`), so the retry below is
                // the last attempt for this leaf.
                let reattempting_current_start =
                    !matches!(&viewport_resolution, ViewportResolutionPhase::Measure(_))
                        && resolved_start.get() == params.window_start;
                let next_visibility_retries =
                    if resolved_start == fallback_window_start || reattempting_current_start {
                        0
                    } else {
                        remaining_visibility_retries.saturating_sub(1)
                    };
                if evaluator.frame_manager().window_topology_generation() != topology_generation
                    || evaluator.window_layout_attempt_freshness(frame_id, window_id, buf_id)
                        != freshness_before_motion
                {
                    return LeafLayoutAttempt::LogicalInputsChanged;
                }
                lisp_ledger.finish_hook_resume(window_id);
                let mut retry_params = params.clone();
                retry_params.window_start = resolved_start.get();
                retry_params.previous_visible_end = None;
                return LeafLayoutAttempt::Retry(Box::new(WindowLayoutRetry {
                    params: retry_params,
                    remaining_visibility_retries: next_visibility_retries,
                    viewport_resolution: ViewportResolutionPhase::Commit(resolved_start),
                }));
            }
            BufferSourceRenderAttemptOutcome::ResolveViewport {
                decision: ViewportDecision::Keep | ViewportDecision::Commit { .. },
            } => unreachable!("only unresolved viewport decisions leave the row producer"),
            BufferSourceRenderAttemptOutcome::Retry { window_start } => {
                if let Some(attempt) = window_end_attempt.take() {
                    evaluator.reject_redisplay_window_end_attempt(attempt);
                }
                // This is a new visibility/recenter decision, not the exact
                // continuation from the hook-reread start.
                lisp_ledger.finish_hook_resume(window_id);
                let mut retry_params = params.clone();
                retry_params.window_start = window_start;
                // The retry re-lays the window from a different start, so the
                // previous layout's visible end describes nothing about it.
                retry_params.previous_visible_end = None;
                // A visibility retry re-flows the window from a new window_start,
                // so the Phase A fast-path plan (snapshotted against the original
                // window_start) no longer applies — re-lay from scratch.
                return LeafLayoutAttempt::Retry(Box::new(WindowLayoutRetry {
                    params: retry_params,
                    remaining_visibility_retries: remaining_visibility_retries.saturating_sub(1),
                    viewport_resolution: ViewportResolutionPhase::Commit(
                        ResolvedWindowStart::from_layout_charpos(window_start),
                    ),
                }));
            }
            BufferSourceRenderAttemptOutcome::RetryPointIntoWindow { point_charpos } => {
                if let Some(attempt) = window_end_attempt.take() {
                    evaluator.reject_redisplay_window_end_attempt(attempt);
                }
                // GNU redisplay_window force_start branch: the explicitly set
                // window start stays, and POINT moves into the window (we use
                // the last fully-visible position of the attempt just laid
                // out). Update the real buffer point + window point marker so
                // the Lisp-visible state matches what the retry lays out.
                let point_lisp = neovm_core::buffer::LispCharPos1::new(point_charpos + 1);
                let window_id = neovm_core::window::WindowId(params.window_id as u64);
                let buffer_id = neovm_core::buffer::BufferId(params.buffer_id);
                let window_selected = evaluator
                    .frame_manager()
                    .get(frame_id)
                    .is_some_and(|frame| frame.selected_window == window_id);
                if window_selected {
                    let byte_pos = evaluator
                        .buffer_manager()
                        .get(buffer_id)
                        .map(|buffer| buffer.lisp_pos_to_emacs_byte_pos(point_lisp));
                    if let Some(byte_pos) = byte_pos {
                        let _ = evaluator
                            .buffer_manager_mut()
                            .goto_buffer_emacs_byte_pos(buffer_id, byte_pos);
                    }
                }
                evaluator.set_window_point_for_redisplay(frame_id, window_id, point_lisp);
                let mut retry_params = params.clone();
                retry_params.point = point_charpos;
                return LeafLayoutAttempt::Retry(Box::new(WindowLayoutRetry {
                    params: retry_params,
                    remaining_visibility_retries: remaining_visibility_retries.saturating_sub(1),
                    viewport_resolution: ViewportResolutionPhase::Commit(resolved_window_start),
                }));
            }
            BufferSourceRenderAttemptOutcome::Finished {
                redisplay_positions,
                query_restart_rows,
                query_row_coverage,
                query_target_prefix_policy,
                window_end_record,
                freshness_before_chrome,
                chrome_lisp_admission: _,
                effective_default_face,
                cursor_only,
                reused_matrix_rows,
                line_number_field_width,
            } => {
                let body = freshness_before_chrome;
                if let Some(snapshot) = window_snapshot.as_mut() {
                    let snapshot = snapshot.display_snapshot_mut();
                    snapshot.layout_freshness = body.freshness.into_freshness();
                    snapshot.buffer_modiff = Some(body.buffer_modiff);
                }
                lisp_ledger.completed_body_inputs.insert(
                    source_window,
                    CompletedBodyIdentity {
                        source_buffer: buf_id,
                        inputs: body.attempt,
                    },
                );
                completed_body_key = Some(body.key);
                lisp_ledger
                    .cursor_sources
                    .insert(source_window, body.sources);
                if position_publication.is_synchronous_query() {
                    self.query_row_coverage = query_row_coverage;
                    self.query_target_prefix_policy = query_target_prefix_policy;
                }
                if params.measurement_pixels.is_some() {
                    self.query_restart_rows = query_restart_rows;
                }
                if let Some(snapshot) = window_snapshot
                    .iter_mut()
                    .rev()
                    .map(|snapshot| snapshot.display_snapshot_mut())
                    .find(|snapshot| snapshot.window_id == window_id)
                {
                    snapshot.window_end_record = Some(window_end_record);
                }
                if cursor_only {
                    self.cursor_only_window_ids
                        .insert(DisplayWindowId::new(params.window_id));
                }
                if let Some(reused) = reused_matrix_rows {
                    let window_id = DisplayWindowId::new(params.window_id);
                    if is_edit {
                        self.edit_window_ids.insert(window_id, reused);
                    } else {
                        self.scroll_window_ids
                            .insert(window_id, (reused, scroll_dvpos));
                    }
                }
                (
                    redisplay_positions,
                    effective_default_face,
                    LineNumberFieldWidth::measured(line_number_field_width),
                )
            }
        };

        // Window metadata is an accepted per-leaf artifact. Visibility and
        // edit-replay retries recurse through this function, so publish it
        // only after the row walk has reached its final start. Publishing it
        // before a retry would retain speculative geometry and install the
        // same output identity twice.
        self.frame_output
            .render_window_info(WindowFrameInfoRenderRequest::new(
                params,
                live_window_frame_metadata(evaluator, buf_id),
                line_number_field,
            ));
        self.publish_window_fringe_backgrounds(buffer, face_resolver, layout_box);

        tracing::debug!(
            "  layout_window_rust: window_start={} window_end={}",
            redisplay_positions.window_start().as_i64(),
            redisplay_positions.window_end_lisp().as_i64()
        );

        let assumed = WindowChromeMetrics::from_params(params);
        let measured = window_snapshot
            .iter()
            .rev()
            .map(WindowPresentationSnapshot::display_snapshot)
            .find(|snapshot| snapshot.window_id == window_id)
            .map(WindowChromeMetrics::from_snapshot)
            .unwrap_or(assumed);
        let outcome = if attempt_mode == FrameAttemptMode::ExhaustionSnapshot {
            WindowLayoutOutcome::Stable {
                layout_box: *layout_box,
                effective_default_face,
            }
        } else {
            WindowLayoutOutcome::from_measurement(*layout_box, measured, effective_default_face)
        };
        if let WindowLayoutOutcome::NeedsRelayout { assumed, measured } = outcome {
            if let Some(attempt) = window_end_attempt.take() {
                evaluator.reject_redisplay_window_end_attempt(attempt);
            }
            tracing::debug!(
                window = params.window_id,
                ?assumed,
                ?measured,
                "window chrome metrics changed; rejecting speculative layout"
            );
        } else {
            Self::mark_inactive_echo_snapshot_geometry_only(
                &mut window_snapshot,
                position_publication,
            );
        }
        LeafLayoutAttempt::Completed {
            outcome,
            window_end_attempt,
            snapshot: window_snapshot,
            produced_key: completed_body_key,
        }
    }

    /// Paint the window's left and right fringe columns with the window's
    /// resolved `fringe` face background (honoring buffer-local
    /// `face-remapping-alist`, matching GNU `draw_window_fringes` in
    /// `src/fringe.c`). Runs for accepted and empty/minibuffer windows; TTY
    /// frames have no fringe regions and emit nothing.
    fn publish_window_fringe_backgrounds(
        &mut self,
        buffer: &impl super::neovm_bridge::LayoutBufferView,
        face_resolver: &super::neovm_bridge::FaceResolver,
        layout_box: &crate::window_layout::WindowLayoutBox,
    ) {
        let regions = layout_box.regions();
        if regions.left_fringe.is_none() && regions.right_fringe.is_none() {
            return;
        }
        let fringe_face = crate::display_row::face_environment::FrameFaces::new(face_resolver)
            .for_window(buffer)
            .resolve_fixed_face(neovm_core::emacs_core::intern::WellKnown::Fringe);
        if fringe_face.use_default_background {
            return;
        }
        let color = Color::from_pixel(fringe_face.bg);
        for rect in [regions.left_fringe, regions.right_fringe]
            .into_iter()
            .flatten()
        {
            if rect.width > 0.0 && rect.height > 0.0 {
                self.frame_output.add_fringe_background(rect, color);
            }
        }
    }

    /// Trigger fontification for a buffer region via the Rust Context.
    ///
    /// Delegates to the neovm-core redisplay helper modeled after GNU
    /// `handle_fontified_prop`: walk the visible Lisp character region and
    /// invoke `fontification-functions` at each unfontified position.
    fn ensure_fontified_rust(
        evaluator: &mut neovm_core::emacs_core::Context,
        buf_id: neovm_core::buffer::BufferId,
        from: i64,
        to: i64,
        request: neovm_core::emacs_core::xdisp::RedisplayFontificationRequest,
        probe: neovm_core::emacs_core::xdisp::ProbeRow,
        window_point: i64,
    ) -> neovm_core::emacs_core::xdisp::RedisplayFontificationOutcome {
        match neovm_core::emacs_core::xdisp::ensure_fontified_for_redisplay_for_window_request(
            evaluator,
            buf_id,
            from,
            to,
            request,
            probe,
            window_point,
        ) {
            Ok(outcome) => outcome,
            Err(e) => {
                tracing::debug!("ensure_fontified_rust: fontification error: {:?}", e);
                neovm_core::emacs_core::xdisp::RedisplayFontificationOutcome::CallbacksUnchanged
            }
        }
    }
}

/// GNU's matrix-reuse eligibility for one window redisplay
/// (`current_matrix_up_to_date_p`, src/xdisp.c:20557-20586).
///
/// `redisplay_window` reads `w->window_end_valid` at ENTRY and only consults
/// `try_cursor_movement` (`:20923`) and
/// `try_window_reusing_current_matrix` (`:21041`) when the predicate holds.
/// The same-start branch otherwise falls into
/// `try_window (..., TRY_WINDOW_CHECK_MARGINS)` (`:21052`), so this is also
/// the gate for `partial_line_height`'s probe row.
///
/// Known follow-ups (N1-F2..F5) — each is a GNU comparison this predicate's
/// first implementation does not perform yet; none changes the six inputs
/// above:
///
/// * N1-F2 — GNU's strict boundary in `window_outdated` (src/window.c:231-236)
///   is `w->last_modified < BUF_MODIFF` and
///   `w->last_overlay_modified < BUF_OVERLAY_MODIFF`.  Neo records both ticks
///   at acceptance and compares with `!=`, so a tick that moves backwards
///   refuses reuse where GNU would accept it.
/// * N1-F3 — hscroll/vscroll buffer scope.  GNU raises the *buffer's*
///   `prevent_redisplay_optimizations_p` when `set-window-hscroll` changes
///   `w->hscroll` (src/window.c:1303), `set-window-vscroll` changes
///   `w->vscroll` (src/window.c:8971), or redisplay auto-hscrolls
///   (src/xdisp.c:16879), so every window showing that buffer refuses reuse.
///   Neo records hscroll/vscroll in the changed window's retained key but
///   does not raise the buffer's prevent revision, so a sibling window
///   showing the same buffer can still reuse its matrix.
/// * N1-F4 — jit-lock steady state.  GNU records `w->last_modified` /
///   `w->last_overlay_modified` only when the matrix is marked accurate
///   (`mark_window_display_accurate_1`, src/xdisp.c:18282-18290); Neo records
///   them on every accepted retained matrix.  Verify that repeated
///   property-only fontification settles to one rebuild and then reuses.
/// * N1-F5 — recenter/scroll `TRY_WINDOW_CHECK_MARGINS`.  GNU passes the
///   flag on the same-start (`src/xdisp.c:21052`) and recenter (`:21403`)
///   walks, where `partial_line_height` can move the window start; Neo has no
///   carried check-margins bit and refuses every non-presentation walk, so
///   that arm is UNATTESTED.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GnuMatrixReuse {
    /// The predicate held at entry: GNU may display this window from the
    /// current matrix and `partial_line_height` does not run.
    Eligible,
    /// GNU re-walks with `try_window (..., TRY_WINDOW_CHECK_MARGINS)`.
    NotEligible,
}

impl GnuMatrixReuse {
    /// `window_end_valid` must be the value from the START of this window's
    /// redisplay: the walk republishes it.
    ///
    /// `windows_or_buffers_changed` clears `w->window_end_valid` and forces
    /// the predicate false (src/xdisp.c:20576-20586); Neo represents that
    /// pending scope with `gnu_redisplay_refuses_cursor_movement`, and the
    /// sibling-window change flag is its frame-local companion.
    ///
    /// The predicate has three more per-buffer/per-window inputs
    /// (src/xdisp.c:20557-20565):
    ///
    /// * `current_buffer->clip_changed` is not read raw: `redisplay_window`
    ///   calls `reconsider_clip_changes (w)` first (src/xdisp.c:20486,
    ///   :17271-17277), which clears it when the window's current matrix still
    ///   matches the buffer and its BEGV/ZV.  Neo's caller performs the same
    ///   reconsideration against the retained matrix's buffer/BEGV/ZV before
    ///   passing `clip_changed`; the durable witness itself is never cleared
    ///   here (the mode-line path owns that).
    /// * `current_buffer->prevent_redisplay_optimizations_p` has its own
    ///   per-buffer revision, raised by `set-buffer-redisplay`
    ///   (src/xdisp.c:929) and both `force-mode-line-update` branches
    ///   (src/buffer.c:1538-1545) and recorded on the accepted matrix outside
    ///   its reuse key.  `force-window-update` additionally moves the body
    ///   revision; either move is a refusal.  Chrome-only raises never move
    ///   the body revision.
    /// * `window_outdated (w)` -- `w->last_modified < BUF_MODIFF` or
    ///   `w->last_overlay_modified < BUF_OVERLAY_MODIFF` (src/window.c:231-236).
    ///   Neo records both at acceptance (`modified_tick`, the overlay tick).
    ///   Its own reuse keys the overlay SET on a digest, so the raw overlay
    ///   tick must be compared here: a touch that leaves the set identical is
    ///   still "outdated" to GNU and still disables its optimizations.
    ///
    /// A window that is stale for any of these is `NotEligible` even if the
    /// walk later reuses rows internally.
    fn at_entry(
        window_end_valid: bool,
        refuses_cursor_movement: bool,
        other_windows_changed: bool,
        clip_changed: bool,
        prevent_redisplay_optimizations: bool,
        window_outdated: bool,
    ) -> Self {
        if window_end_valid
            && !refuses_cursor_movement
            && !other_windows_changed
            && !clip_changed
            && !prevent_redisplay_optimizations
            && !window_outdated
        {
            Self::Eligible
        } else {
            Self::NotEligible
        }
    }

    /// GNU runs `partial_line_height` unless the predicate held AND the walk
    /// reused the current matrix.  When the predicate held but Neo rebuilt
    /// the rows, GNU's own fallback would have called `try_window` with the
    /// check-margins flag, so the probe still runs.
    fn skips_probe(self, walk_reused_rows: bool) -> bool {
        self == Self::Eligible && walk_reused_rows
    }
}

/// GNU `partial_line_height`'s probe row (`src/xdisp.c:11630-11661`).
///
/// `try_window` runs it only under `TRY_WINDOW_CHECK_MARGINS` with
/// `w->vscroll == 0` in a non-minibuffer window (`src/xdisp.c:21696-21722`);
/// the flag is passed on the same-window-start path (`:21052`) and recenter
/// (`:21402`) but not on a forced start (`:20767`, `:20901`) or a plain
/// `try_window (..., 0)`.  Neo has no carried check-margins bit: this helper
/// represents that gate by refusing every non-presentation walk (query
/// target, measurement rows/pixels, accessible-end extent) and every
/// `force_start` walk; the matrix-reuse half of the gate is
/// [`GnuMatrixReuse`], applied by the caller.  `partial_line_height` walks to
/// the window bottom
/// and, unless that walk hits EOB, scans the FIRST screen row after the
/// window end with `move_it_by_lines (&it, 1)`.  The returned typed probe is
/// that row's exclusive Lisp end.  A long-line truncated buffer returns early
/// (`:11636-11638`).
///
/// The row starts at the completed walk's PUBLISHED window end -- the
/// exclusive first position after the last displayed row, the same value the
/// engine records as `window-end` -- and its boundary is measured by the
/// display line-motion engine (`MotionEngine::ComputeMotion`), not from a
/// character count, so wrapping and truncation decide it.
fn fontification_probe_row(
    evaluator: &mut neovm_core::emacs_core::Context,
    params: &crate::types::WindowParams,
    frame_id: neovm_core::window::FrameId,
    window_id: neovm_core::window::WindowId,
    buf_id: neovm_core::buffer::BufferId,
    window_end: neovm_core::buffer::LispCharPos1,
) -> neovm_core::emacs_core::xdisp::ProbeRow {
    use neovm_core::emacs_core::xdisp::ProbeRow;
    // `force_start` bypasses TRY_WINDOW_CHECK_MARGINS (src/xdisp.c:20767,
    // :20901): one pass, no probe row.
    if params.force_start {
        return ProbeRow::None;
    }
    // Neo does not carry GNU's TRY_WINDOW_CHECK_MARGINS bit; these params
    // identify the walks that are not a margin-checked presentation walk
    // (`try_window (..., 0)` and synchronous measurement/query walks), and
    // they get no probe.
    if params.query_target.is_some()
        || params.measurement_rows.is_some()
        || params.measurement_pixels.is_some()
        || params.source_extent == crate::types::WindowSourceExtent::AccessibleEnd
    {
        return ProbeRow::None;
    }
    // `try_window`'s own gates (src/xdisp.c:21696-21722).
    if params.is_minibuffer() {
        return ProbeRow::None;
    }
    let Some(window) = evaluator
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.find_window(window_id))
    else {
        return ProbeRow::None;
    };
    if let neovm_core::window::Window::Leaf { vscroll, .. } = window
        && *vscroll != 0
    {
        return ProbeRow::None;
    }
    let Some(buffer) = evaluator.edit.buffers.get(buf_id) else {
        return ProbeRow::None;
    };
    // `partial_line_height` returns early for a long-line truncated buffer
    // (src/xdisp.c:11636-11638).
    if buffer.long_line_optimizations_p()
        && params.wrap_mode == crate::types::LineWrapMode::Truncate
    {
        return ProbeRow::None;
    }
    // GNU's `move_it_to (..., it.last_visible_y, ...)` re-walks the window to
    // its bottom and `move_it_by_lines (&it, 1)` then scans the FIRST screen
    // row after the window end (`src/xdisp.c:11630-11661`). That row starts at
    // the position Neo publishes as `window-end`: the exclusive first
    // position after the last displayed row, passed in from the same
    // `TextWindowRedisplayPositions` record the engine turns into the
    // `WindowEndRecord` it publishes (and that `window-end` returns).
    //
    // Do not derive it from `visible_buffer_span().end()`: that is the last
    // drawn glyph (an inclusive position), and a right-truncated row reports
    // its skipped line terminator there, so a probe started there begins
    // inside a row the window already displayed and its one-line advance
    // does not reach the following row. The published end also sits after a
    // closing glyph that covers a source range (`display` property or
    // composition), where last-glyph arithmetic has no exact successor in the
    // buffer.
    let probe_start = window_end;
    // The re-walk reached EOB: GNU takes the `IT_EOB` branch and never scans
    // a following row (`src/xdisp.c:11644-11649`).
    if probe_start.as_i64() >= buffer.point_max_lisp_char_pos().as_i64() {
        return ProbeRow::None;
    }
    let start_byte = buffer.lisp_pos_to_emacs_byte_pos(probe_start);
    // Pass the laid-out window, not the selected one: GNU's
    // `move_it_by_lines` uses THIS window's width and wrap decision.
    let window_value = Value::make_window(window_id.0);
    let Ok((end_byte, moved)) = neovm_core::emacs_core::indent::vmotion_target(
        evaluator,
        buf_id,
        start_byte,
        Some(window_value),
        1,
    ) else {
        return ProbeRow::None;
    };
    if moved == 0 || end_byte <= start_byte {
        return ProbeRow::None;
    }
    let Some(buffer) = evaluator.edit.buffers.get(buf_id) else {
        return ProbeRow::None;
    };
    let end_lisp = buffer.emacs_byte_pos_to_lisp_char_pos(end_byte).as_i64();
    if end_lisp <= probe_start.as_i64() {
        ProbeRow::None
    } else {
        ProbeRow::Through(end_lisp)
    }
}

impl LayoutEngine {
    /// Render the frame-level tab-bar from GNU Lisp keymap output on the Rust path.
    ///
    /// Build the frame-level tab-bar row and attach it to the published
    /// `FrameDisplayState` as frame chrome, not as a leaf-window row.
    ///
    /// GNU handles the tab bar outside ordinary leaf-window text rows:
    /// - GUI uses `frame->tab_bar_window`
    /// - TTY writes tab-bar rows directly into the frame matrix
    ///
    /// Neomacs keeps immutable snapshots, so this method records typed
    /// frame-chrome content that every renderer can consume directly.
    #[allow(clippy::too_many_arguments)]
    fn render_frame_tab_bar_rust(
        &mut self,
        evaluator: &mut neovm_core::emacs_core::Context,
        face_resolver: &super::neovm_bridge::FaceResolver,
        frame_params: &FrameParams,
        tab_bar_height: f32,
        presentation_id: u64,
        tab_bar: BuiltTabBar,
        face_attempt: &FrameFaceAttempt,
    ) -> Option<f32> {
        let width = frame_params.width;
        let mut tab_bar_face =
            face_resolver.default_base_face_for_origin_without_buffer(&DisplayOrigin::TabBar);
        face_resolver.realize_for_display(&mut tab_bar_face, evaluator);
        let tab_bar_ascent = frame_params.char_height * 0.8;
        let fallback_metrics =
            DisplayRowFallbackMetrics::from_frame_defaults(frame_params, tab_bar_ascent);
        let metrics = DisplayRowFaceRealizer::new(&mut self.font_metrics)
            .row_metrics_for_face(&tab_bar_face, fallback_metrics);
        let mut face_ids = face_attempt.clone();
        let automatic_composition =
            crate::neovm_bridge::current_string_composition_rules(evaluator);
        let root_scope = crate::string_property_snapshot::StringSourceRootScope::Rooted(
            evaluator.owned_root_retainer(),
        );
        let rendered_tab_bar = {
            let (display_host, evaluator) = if face_resolver.has_tty_realization() {
                (None, Some(&mut *evaluator))
            } else {
                (evaluator.display_host.as_deref(), None)
            };
            let evaluator = std::cell::RefCell::new(evaluator);
            let realize = |face: &mut super::neovm_bridge::ResolvedFace| {
                if let Some(evaluator) = evaluator.borrow_mut().as_deref_mut() {
                    face_resolver.realize_for_display(face, evaluator);
                }
            };
            let realizer = face_resolver
                .has_tty_realization()
                .then_some(&realize as &dyn Fn(&mut super::neovm_bridge::ResolvedFace));
            self.frame_output.render_frame_tab_bar_row(
                FrameTabBarDisplayRowRequest {
                    row_index: 0,
                    y: 0.0,
                    width,
                    height: tab_bar_height,
                    metrics,
                    base_face: &tab_bar_face,
                    text: tab_bar.text,
                    image_scale_environment: frame_params.image_scale_environment,
                },
                ChromeRowRenderServices::new(
                    &mut self.font_metrics,
                    face_resolver,
                    &mut face_ids,
                    root_scope,
                )
                .with_face_realizer(realizer)
                .with_automatic_composition(automatic_composition),
                display_host,
            )
        }?;
        let FrameTabBarDisplayRowRender::Measured(measured) = rendered_tab_bar else {
            return None;
        };
        let pointer_slots = tab_bar_pointer_slot_plan(
            evaluator,
            measured.rendered(),
            tab_bar.text,
            &tab_bar.source_items,
        );
        let effective_mouse_faces = tab_bar_effective_mouse_faces(&pointer_slots);
        let mut realized_mouse_faces = Vec::new();
        for value in effective_mouse_faces {
            let Some(mut resolved) = face_resolver.resolve_face_value_over(&tab_bar_face, &value)
            else {
                continue;
            };
            resolved.lisp_name = value.as_bare_symbol_name().map(std::sync::Arc::from);
            let face_id = crate::display_row::face_state::stable_face_id_for_resolved(
                &mut face_ids,
                &resolved,
            );
            resolved.face_id = face_id.get();
            let realized = DisplayRowFaceRealizer::new(&mut self.font_metrics).realize_face(
                face_id,
                &resolved,
                metrics.char_width(),
                metrics.ascent(),
                metrics.row_height(),
            );
            let registered = face_ids
                .import_face(realized.render_face())
                .expect("pointer face must preserve its realized identity");
            self.frame_output.install_pointer_face(&registered);
            realized_mouse_faces.push((value, face_id));
        }
        let actual_tab_bar_height = measured.bounds().height;
        let (horizontal_margin, vertical_margin, thickness) =
            tab_bar_button_relief_geometry(evaluator);
        let pointer_style = gnu_tab_bar_pointer_appearance_style(
            tab_bar_face.bg,
            frame_params.background,
            horizontal_margin,
            vertical_margin,
            thickness,
        );
        let image_styles = tab_bar_image_relief_styles(
            measured.rendered(),
            tab_bar_face.bg,
            frame_params.background,
            horizontal_margin,
            vertical_margin,
            thickness,
        );
        let pointer_plan = tab_bar_presented_pointer_plan(
            evaluator,
            presentation_id,
            &pointer_slots,
            &tab_bar.source_items,
            actual_tab_bar_height,
            pointer_style,
            &image_styles,
            &realized_mouse_faces,
        );
        let hit_regions = pointer_plan.hit_regions().to_vec();
        self.frame_output.add_frame_chrome_band(
            ChromeBandRequest::new(
                ProtocolFrameChromeKind::TabBar,
                actual_tab_bar_height,
                FrameChromeContent::DisplayRow(frame_chrome_display_row(&measured)),
            )
            .with_hit_regions(hit_regions),
        );
        self.pending_tab_bar_pointer = Some(pointer_plan);
        Some(actual_tab_bar_height)
    }

    /// Layout a MockFrameContent into FrameDisplayState snapshots.
    ///
    /// This is the mock-display entry point.  The real neomacs GUI pipeline
    /// goes through `layout_frame_rust()` which takes a live Lisp evaluator.
    pub fn layout_mock_frame(
        &mut self,
        content: &super::mock_frame::MockFrameContent,
        char_w: f32,
        char_h: f32,
    ) -> Vec<neomacs_display_protocol::glyph_matrix::FrameDisplayState> {
        layout_mock_frame_content(content, char_w, char_h, &mut self.font_metrics)
    }
}

#[cfg(test)]
#[path = "engine/tests/engine_test.rs"]
mod tests;

#[cfg(test)]
#[path = "engine/tests/mode_line_flow_test.rs"]
mod mode_line_flow_test;
