//! The Rust layout engine — Phase 1+2: Monospace layout with face resolution.
//!
//! Reads buffer text and display state from neovm-core, resolves faces per
//! character position, computes line breaks, positions glyphs on a fixed-width
//! grid, and publishes `FrameDisplayState` snapshots for render backends.

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
use crate::buffer_source::render_attempt::WindowPositionPublication;
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
    FrameContentTransitionHintRenderRequest, FrameLineAnimationHintsRenderRequest,
    FrameOutputIdentity, FrameOutputOwner, FrameOutputStateRenderRequest,
    FrameThemeTransitionHintRenderRequest, FrameWindowSwitchHintRenderRequest,
    NavigationIntentObservation, WindowContentTransitionMode, WindowFrameDecorationsRenderRequest,
    WindowFrameGeometry, WindowFrameGeometryRequest, WindowFrameInfoEffectsRenderRequest,
    WindowFrameInfoRenderRequest, WindowFrameMetadata,
};
use crate::display_mock_frame::layout_mock_frame_content;
use crate::display_origin::DisplayOrigin;
use crate::display_rendered_row_output_install::frame_chrome_display_row;
use crate::display_row::face_state::DisplayRowFaceRealizer;
#[cfg(test)]
use crate::display_row::geometry::{DisplayRowHitRange, DisplayRowMarker, DisplayRowStartMarker};
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
    BoxFaceRowState, HitRowRangeTracker, HorizontalScrollDisplayItem, HorizontalScrollSkipState,
    HorizontalScrollTruncationTarget, HorizontalScrollVisibleRemainder,
    HscrollConsumedTextDisposition, InvisibleTextScanCheckpoint, LineNumberRenderState,
    TrailingWhitespaceRenderState, WordWrapRenderState,
};
use crate::display_status_line::{max_mini_window_lines, max_mini_window_lines_for_buffer};
use crate::font::frame_metrics::FrameFontDomain;
use crate::font::metrics::{FontMetricsService, FrameCellGeometry};
use crate::font::sizing::FontSizing;
use crate::frame_face_arena::{
    FrameFaceArena, FrameFaceAttempt, FrameFaceGeneration, FrameFaceReuseError,
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
mod viewport_retry_depth_probe {
    use std::cell::Cell;

    thread_local! {
        static DEPTH: Cell<usize> = const { Cell::new(0) };
        static MAX_DEPTH: Cell<usize> = const { Cell::new(0) };
    }

    pub(super) struct Guard;

    impl Guard {
        pub(super) fn enter() -> Self {
            DEPTH.with(|depth| {
                let next = depth.get() + 1;
                depth.set(next);
                MAX_DEPTH.with(|max| {
                    if next > max.get() {
                        max.set(next);
                    }
                });
            });
            Guard
        }
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            DEPTH.with(|depth| depth.set(depth.get() - 1));
        }
    }

    pub(super) fn reset() {
        MAX_DEPTH.with(|max| max.set(0));
    }

    pub(super) fn max_depth() -> usize {
        MAX_DEPTH.with(|max| max.get())
    }
}
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

impl EditReplayStructureProperty {
    fn symbols() -> &'static [Value; <Self as strum::EnumCount>::COUNT] {
        use std::sync::OnceLock;
        use strum::VariantArray;

        const N: usize = <EditReplayStructureProperty as strum::EnumCount>::COUNT;
        static SYMBOLS: OnceLock<[Value; N]> = OnceLock::new();
        SYMBOLS.get_or_init(|| {
            std::array::from_fn(|index| {
                Value::symbol(neovm_core::emacs_core::intern::intern(
                    EditReplayStructureProperty::VARIANTS[index].into(),
                ))
            })
        })
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
    SynchronousQuery {
        window_id: neovm_core::window::WindowId,
    },
}

impl LayoutPurpose {
    const fn query_window(self) -> Option<neovm_core::window::WindowId> {
        match self {
            Self::Redisplay | Self::Snapshot => None,
            Self::SynchronousQuery { window_id } => Some(window_id),
        }
    }
}

/// Exhaustive result of one renderer-facing frame attempt.
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
        self.inner.retained_window_chrome_metrics = seed.retained_window_chrome_metrics;
    }

    pub fn query_window_layout(
        &mut self,
        evaluator: &mut neovm_core::emacs_core::Context,
        frame_id: neovm_core::window::FrameId,
        window_id: neovm_core::window::WindowId,
    ) -> Result<neovm_core::window::WindowLayoutQuery, neovm_core::window::WindowLayoutQueryFailure>
    {
        self.inner
            .query_window_layout(evaluator, frame_id, window_id)
    }
}

fn resize_mini_windows_mode_for_buffer(
    evaluator: &neovm_core::emacs_core::Context,
    buffer_id: neovm_core::buffer::BufferId,
) -> ResizeMiniWindowsMode {
    let value = evaluator
        .buffer_manager()
        .get(buffer_id)
        .and_then(|buffer| buffer.buffer_local_value("resize-mini-windows"))
        .or_else(|| {
            evaluator
                .obarray()
                .symbol_value("resize-mini-windows")
                .copied()
        });
    ResizeMiniWindowsMode::from_lisp_value(value.as_ref())
}

fn uses_adhoc_minibuffer_resize_scroll(
    evaluator: &neovm_core::emacs_core::Context,
    buffer_id: neovm_core::buffer::BufferId,
) -> bool {
    evaluator
        .buffer_manager()
        .get(buffer_id)
        .and_then(|buffer| {
            buffer.buffer_local_value("redisplay-adhoc-scroll-in-resize-mini-windows")
        })
        .or_else(|| {
            evaluator
                .obarray()
                .symbol_value("redisplay-adhoc-scroll-in-resize-mini-windows")
                .copied()
        })
        .is_none_or(|value| !value.is_nil())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowLayoutWalkPurpose {
    Redisplay,
    SynchronousQuery,
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
    if purpose == WindowLayoutWalkPurpose::SynchronousQuery {
        return WindowPositionPublication::SynchronousQueryEnd;
    }
    if source == WindowDisplaySource::InactiveEchoArea {
        return WindowPositionPublication::InactiveEchoArea;
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

/// Resolve the buffer and range that this window actually displays.
///
/// GNU's `with_echo_area_buffer` temporarily installs the echo-area buffer in an
/// inactive mini-window before redisplay measures or walks it.  Resolve that
/// semantic source once, before fontification and incremental-key creation,
/// so every phase observes the same buffer identity, ticks, range, and point.
fn resolve_window_display_source_params(
    evaluator: &mut neovm_core::emacs_core::Context,
    params: &WindowParams,
    purpose: WindowLayoutWalkPurpose,
) -> ResolvedWindowDisplaySource {
    let window_id = neovm_core::window::WindowId(params.window_id as u64);
    // Hand layout a shared handle to the image catalog: `(space :align-to …)`
    // may embed an `(image …)` operand whose intrinsic size decides the result
    // (GNU resolves it inline with `lookup_image`). This is the single point
    // every window's params pass through that also holds the evaluator.
    let mut params = params.clone();
    params.space_image_catalog = evaluator
        .display_host
        .as_ref()
        .and_then(|host| host.image_catalog_shared())
        .map(crate::types::SharedImageCatalog);
    let params = &params;

    if purpose == WindowLayoutWalkPurpose::SynchronousQuery
        || !params.is_minibuffer()
        || evaluator.minibuffer_window_is_active(window_id)
    {
        return ResolvedWindowDisplaySource {
            params: params.clone(),
            source: WindowDisplaySource::LiveWindow,
        };
    }

    evaluator.ensure_echo_area_buffers();
    let Some(buf_id) = evaluator.echo_area_display_buffer() else {
        return ResolvedWindowDisplaySource {
            params: params.clone(),
            source: WindowDisplaySource::LiveWindow,
        };
    };
    let Some(buffer_size) = evaluator
        .buffer_manager()
        .get(buf_id)
        .map(|buffer| buffer.point_max_char_pos().get() as i64)
    else {
        return ResolvedWindowDisplaySource {
            params: params.clone(),
            source: WindowDisplaySource::LiveWindow,
        };
    };

    let mut resolved = params.clone();
    resolved.buffer_id = buf_id.0;
    resolved.window_start = 0;
    resolved.previous_visible_end = None;
    resolved.point = if resolved.cursor_role.is_active() {
        // GNU's echo-area cursor is the insertion position immediately after
        // the displayed message, not point in the minibuffer's live buffer.
        buffer_size
    } else {
        0
    };
    resolved.buffer_begv = 0;
    resolved.buffer_size = buffer_size;
    ResolvedWindowDisplaySource {
        params: resolved,
        source: WindowDisplaySource::InactiveEchoArea,
    }
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
    let (frame, mut window, main_area_bottom) = {
        let (live_frame, live_window) = evaluator
            .frame_manager()
            .frame_and_window_at_path(frame_id, window_path)?;
        if live_window.id() != window_id {
            return None;
        }
        let buffer_id = live_window.buffer_id()?;
        let buffer = evaluator.buffer_manager().get(buffer_id)?;
        let frame_is_selected = evaluator
            .frame_manager()
            .selected_frame()
            .is_some_and(|selected| selected.id == frame_id);
        let is_selected = frame_is_selected && live_frame.selected_window == window_id;
        let cursor_role = match purpose {
            WindowLayoutWalkPurpose::Redisplay => {
                super::neovm_bridge::redisplay_cursor_target(evaluator, frame_id)
                    .role_for(window_id)
            }
            WindowLayoutWalkPurpose::SynchronousQuery => WindowCursorRole::from_active(is_selected),
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
            evaluator.obarray(),
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
            font_sizing,
        )?;
        let root_bounds = live_frame.root_window.bounds();
        (
            super::neovm_bridge::frame_params_from_neovm(
                live_frame,
                evaluator.face_table(),
                evaluator.obarray(),
            ),
            window,
            root_bounds.y + root_bounds.height,
        )
    };
    if let Some(metrics) = accepted_chrome.get(&DisplayWindowId::new(window.window_id)) {
        metrics.seed_params(&mut window);
    }
    let resolved = resolve_window_display_source_params(evaluator, &window, purpose);
    Some(LiveWindowLayoutInputs {
        frame,
        window: resolved.params,
        source: resolved.source,
        main_area_bottom,
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
    let raw = evaluator
        .buffer_manager()
        .get(buf_id)
        .and_then(|buffer| buffer.buffer_local_value("max-mini-window-height"))
        .or_else(|| {
            evaluator
                .obarray()
                .symbol_value("max-mini-window-height")
                .copied()
        })
        .unwrap_or_else(|| Value::make_float(0.25));
    max_mini_window_lines_from_value(raw, frame_rows)
}

fn minibuffer_growth_target(
    used_rows: usize,
    allocated_rows: usize,
    max_lines: f32,
) -> Option<usize> {
    let achievable_rows = used_rows.min(max_lines.floor().max(1.0) as usize);
    (achievable_rows > allocated_rows).then_some(achievable_rows)
}

fn tab_bar_button_relief_geometry(evaluator: &neovm_core::emacs_core::Context) -> (f32, f32, f32) {
    let margin = evaluator
        .obarray()
        .symbol_value("tab-bar-button-margin")
        .copied()
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
    let configured_thickness = evaluator
        .obarray()
        .symbol_value("tab-bar-button-relief")
        .copied()
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
    /// Cosmic-text font metrics service.
    ///
    /// Populated by `enable_cosmic_metrics()` at GUI startup. Left
    /// `None` for TTY mode, where all measurements go through the
    /// character-cell grid. Replaces the previous
    /// `use_cosmic_metrics: bool` runtime flag — the decision is
    /// now made once at startup by the binary that constructs the
    /// layout engine.
    pub font_metrics: Option<FontMetricsService>,
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
    /// Last sealed face namespace for each logical frame.
    ///
    /// A speculative layout gets a fresh [`FrameFaceAttempt`] from this arena;
    /// retries discard that attempt, and only a sealed presentation replaces
    /// the committed arena.
    frame_face_arenas: rustc_hash::FxHashMap<neovm_core::window::FrameId, FrameFaceArena>,
    /// Per-window retained layout, owned across cycles (incremental-layout
    /// Phase 0a). Committed at the accepted `break` only; NOT read yet — the
    /// engine still rebuilds every window every cycle. The container a later
    /// phase reuses rows out of.
    retained_window_matrices: rustc_hash::FxHashMap<DisplayWindowId, RetainedWindowMatrix>,
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

/// One window's incremental-layout decision, gathered before the frame admits
/// any retained face IDs or allocates any fresh ones.
///
/// Keeping the alternatives in one type makes the face namespace a property of
/// the frame plan, rather than an ordering side effect of whichever window
/// happens to render first.
struct IncrementalWindowPlan {
    cursor_only: Option<CursorOnlyReplay>,
    scroll: Option<ScrollReplay>,
    is_edit: bool,
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
            .is_some_and(|frame| frame.compact_bar_height > 0 || frame.menu_bar_height > 0);
        let menu_items = needs_menu_items
            .then(|| collect_gui_menu_bar_items_for_frame(evaluator, frame_id))
            .unwrap_or_default();
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
                frame.compact_bar_height > 0
                    || frame
                        .frame_parameter_int("compact-bar-lines")
                        .is_some_and(|lines| lines > 0)
                    || frame.tool_bar_height > 0
                    || frame
                        .known_frame_parameter_int(FrameParam::ToolBarLines)
                        .is_some_and(|lines| lines > 0)
            });
        let tool_items = needs_tool_items
            .then(|| collect_gui_tool_bar_items_for_frame(evaluator, frame_id))
            .unwrap_or_default();
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
enum LeafLayoutAttempt {
    Completed {
        outcome: WindowLayoutOutcome,
        window_end_attempt: Option<neovm_core::window::WindowEndAttempt>,
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
    LogicalInputsChanged,
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

/// GNU-visible callback acknowledgements owned by one logical frame attempt.
///
/// Chrome/minibuffer convergence may perform several physical row walks. Those
/// retries must not replay Lisp that the logical redisplay already ran.
#[derive(Default)]
struct RedisplayLispLedger {
    acknowledged_scroll_hooks: rustc_hash::FxHashSet<WindowScrollHookSite>,
    exact_hook_resumes: rustc_hash::FxHashMap<neovm_core::window::WindowId, ResolvedWindowStart>,
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
        self.acknowledged_scroll_hooks
            .insert(effect.scroll_hook_site());
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
    }

    fn retained_face_ids(&self) -> Vec<FaceId> {
        self.cursor_only
            .as_ref()
            .map(CursorOnlyReplay::retained_face_ids)
            .or_else(|| self.scroll.as_ref().map(ScrollReplay::retained_face_ids))
            .unwrap_or_default()
    }

    fn disable_reuse(&mut self) {
        self.cursor_only = None;
        self.scroll = None;
        self.is_edit = false;
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
/// The matrix and its chrome metrics travel together because they are keyed
/// the same way and committed at the same instant. Keeping them in one type is
/// the point: they previously sat in two sibling maps next to a third keyed by
/// `FrameId`, and only the frame-keyed one was right.
#[derive(Default)]
struct RetainedFrameState {
    matrices: rustc_hash::FxHashMap<DisplayWindowId, RetainedWindowMatrix>,
    chrome_metrics: rustc_hash::FxHashMap<DisplayWindowId, WindowChromeMetrics>,
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
    let rows = (params.text_bounds.height / params.char_height.max(1.0)).ceil() as usize;
    let screenful = rows.saturating_mul(cols).max(1);
    let first = params.window_start.max(0) as usize;
    (
        first.saturating_sub(screenful),
        screenful.saturating_mul(4).max(8192),
    )
}

fn admit_retained_frame_faces(
    plans: &[IncrementalWindowPlan],
    face_attempt: &mut FrameFaceAttempt,
    committed_arena: &FrameFaceArena,
) -> Result<(), FrameFaceReuseError> {
    let current_generation = committed_arena.generation();
    let mut face_ids = std::collections::BTreeSet::new();
    for plan in plans {
        let Some(retained_generation) = plan.retained_face_generation() else {
            continue;
        };
        if retained_generation != current_generation {
            return Err(FrameFaceReuseError::StaleGeneration {
                retained: retained_generation,
                current: current_generation,
            });
        }
        face_ids.extend(
            plan.retained_face_ids()
                .into_iter()
                .filter(|face_id| *face_id != FaceId::new(0)),
        );
    }
    face_attempt.admit_retained(current_generation, face_ids, committed_arena)
}

impl Default for LayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutEngine {
    /// Invalidate every retained value whose geometry or identity was derived
    /// from old font-selection inputs. This is deliberately one exhaustive
    /// owner rather than a list of clears spread across redisplay fast paths.
    fn invalidate_for_font_selection_change(&mut self) {
        self.frame_visual_histories = FrameVisualHistories::default();
        self.frame_face_arenas.clear();
        self.retained_window_matrices.clear();
        self.retained_window_chrome_metrics.clear();
        self.last_frame_display_state = None;
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
        self.cursor_only_window_ids.clear();
        self.scroll_window_ids.clear();
        self.edit_window_ids.clear();
    }

    fn accept_frame_relayout_request(
        coordinator: &mut FrameLayoutCoordinator,
        evaluator: &mut neovm_core::emacs_core::Context,
        window_end_attempts: &mut FrameWindowEndAttempts,
        presentation_id: u64,
        request: FrameRelayoutRequest,
    ) -> bool {
        match coordinator.request_retry(request) {
            Ok(()) => true,
            Err(error) => {
                tracing::error!(
                    presentation = presentation_id,
                    ?error,
                    "rejecting frame whose layout failed to converge"
                );
                window_end_attempts.reject_all(evaluator);
                evaluator.retire_interaction_presentation(presentation_id);
                false
            }
        }
    }

    fn latest_output_window_info(&self, window_id: i64) -> Option<WindowInfo> {
        self.frame_output.latest_window_info(window_id)
    }

    fn output_window_content_height_px(
        &self,
        window_id: i64,
        fallback_row_height: f32,
    ) -> Option<f32> {
        self.frame_output
            .window_content_height_px(window_id, fallback_row_height)
    }

    /// Keep inactive echo-area geometry renderer-visible without admitting it
    /// to the live minibuffer's retained display cache.
    ///
    /// GNU temporarily swaps w->contents while displaying the echo area and
    /// restores the live minibuffer afterward. The frame still needs the
    /// temporary geometry, but window-end/posn/vertical-motion must never
    /// mistake those rows for evidence about the minibuffer's real buffer.
    fn mark_inactive_echo_snapshot_geometry_only(
        &mut self,
        window_id: neovm_core::window::WindowId,
        publication: WindowPositionPublication,
    ) {
        if publication != WindowPositionPublication::InactiveEchoArea {
            return;
        }
        if let Some(snapshot) = self
            .window_snapshots
            .iter_mut()
            .rev()
            .find(|snapshot| snapshot.display_snapshot().window_id == window_id)
        {
            snapshot.retain_as_geometry_only();
        }
    }

    fn finish_frame_output(
        &mut self,
        frame_params: &FrameParams,
    ) -> Result<
        neomacs_display_protocol::glyph_matrix::FrameDisplayState,
        neomacs_display_protocol::frame_chrome::ChromeLayoutError,
    > {
        let mut state = self.frame_output.finish(frame_params)?;
        // Publish exact resolved font identities for every realized face so
        // the render thread rasterizes the same fonts layout measured with
        // (font realization / render boundary design, Phase 1).
        crate::font::metrics::realize_frame_fonts(&mut state, &mut self.font_metrics);
        Ok(state)
    }

    fn render_window_output_decorations(
        &mut self,
        params: &WindowParams,
        frame_params: &FrameParams,
        window_geometry: crate::display_frame_output::WindowFrameGeometry,
        info: &WindowInfo,
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
                effective_default_face,
            ),
            ChromeRowRenderServices::new(font_metrics, face_resolver, &mut decoration_face_ids),
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
        frame_params: &FrameParams,
        frame_navigation: Option<neomacs_display_protocol::TransitionDirection>,
    ) -> NavigationIntentObservation {
        let prev_window_infos = previous.window_infos();
        self.frame_output
            .render_line_animation_hints(FrameLineAnimationHintsRenderRequest::new(
                prev_window_infos,
                curr_window_infos,
            ));
        self.frame_output
            .render_window_switch_hint(FrameWindowSwitchHintRenderRequest::new(
                previous.selected_text_window(),
            ));
        self.frame_output
            .render_theme_transition_hint(FrameThemeTransitionHintRenderRequest::new(
                previous.background(),
                frame_params.width,
                frame_params.height,
            ));
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
            font_metrics: Some(FontMetricsService::new()),
            font_sizing: FontSizing::native_gui(),
            frame_visual_histories: FrameVisualHistories::default(),
            frame_output: FrameOutputOwner::new(),
            pending_tab_bar_pointer: None,
            last_frame_display_state: None,
            frame_face_arenas: rustc_hash::FxHashMap::default(),
            retained_window_matrices: rustc_hash::FxHashMap::default(),
            retained_by_frame: rustc_hash::FxHashMap::default(),
            retained_frame: None,
            retained_window_chrome_metrics: rustc_hash::FxHashMap::default(),
            cursor_only_window_ids: rustc_hash::FxHashSet::default(),
            scroll_window_ids: rustc_hash::FxHashMap::default(),
            edit_window_ids: rustc_hash::FxHashMap::default(),
            pre_fontify_dirty_spans: rustc_hash::FxHashMap::default(),
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
            font_metrics: None,
            font_sizing: FontSizing::native_gui(),
            frame_visual_histories: FrameVisualHistories::default(),
            frame_output: FrameOutputOwner::new(),
            pending_tab_bar_pointer: None,
            last_frame_display_state: None,
            frame_face_arenas: rustc_hash::FxHashMap::default(),
            retained_window_matrices: rustc_hash::FxHashMap::default(),
            retained_by_frame: rustc_hash::FxHashMap::default(),
            retained_frame: None,
            retained_window_chrome_metrics: rustc_hash::FxHashMap::default(),
            cursor_only_window_ids: rustc_hash::FxHashSet::default(),
            scroll_window_ids: rustc_hash::FxHashMap::default(),
            edit_window_ids: rustc_hash::FxHashMap::default(),
            pre_fontify_dirty_spans: rustc_hash::FxHashMap::default(),
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
    pub fn enable_cosmic_metrics(&mut self) {
        if self.font_metrics.is_none() {
            self.font_metrics = Some(FontMetricsService::new());
        }
    }

    pub fn set_font_sizing(&mut self, font_sizing: FontSizing) {
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
        self.layout_frame_rust_for_purpose_inner(evaluator, frame_id, purpose);
        self.last_frame_display_state
            .take()
            .map_or(FrameLayoutAttempt::Aborted, FrameLayoutAttempt::Prepared)
    }

    fn layout_frame_rust_for_purpose_inner(
        &mut self,
        evaluator: &mut neovm_core::emacs_core::Context,
        frame_id: neovm_core::window::FrameId,
        purpose: LayoutPurpose,
    ) -> Option<neovm_core::window::WindowLayoutQuery> {
        let query_window = purpose.query_window();
        self.load_retained_frame(frame_id);
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

        evaluator.sync_runtime_faces_for_frame(frame_id);

        let (bootstrap_bg, bootstrap_font_size, window_system, device_scale) = {
            let Some(frame) = evaluator.frame_manager().get(frame_id) else {
                tracing::error!("layout_frame_rust: frame {:?} not found", frame_id);
                return None;
            };
            let bootstrap = super::neovm_bridge::frame_params_from_neovm(
                frame,
                evaluator.face_table(),
                evaluator.obarray(),
            );
            let ws = frame
                .effective_window_system()
                .and_then(|v| v.as_symbol_name().map(|s| s.to_string()));
            (
                bootstrap.background,
                frame.font_pixel_size,
                ws,
                neomacs_display_protocol::geometry::DeviceScale::new(
                    frame.device_scale_factor as f32,
                )
                .unwrap_or_else(|_| {
                    neomacs_display_protocol::geometry::DeviceScale::new(1.0)
                        .expect("one is a valid device scale")
                }),
            )
        };
        let font_selection_changed = if let Some(font_metrics) = self.font_metrics.as_mut() {
            font_metrics.set_device_scale(device_scale);
            let font_catalog_changed = font_metrics.synchronize_font_catalog().changed();
            let use_primary_font = evaluator
                .obarray()
                .symbol_value("use-default-font-for-symbols")
                .is_none_or(|value| !value.is_nil());
            let char_script_table = evaluator
                .obarray()
                .symbol_value("char-script-table")
                .copied();
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

        // Realize the default face before collecting window params so frame and
        // window geometry use the same default metrics GNU Emacs redisplay does.
        let mut face_resolver = super::neovm_bridge::FaceResolver::new_with_font_sizing(
            evaluator.face_table(),
            0x00FFFFFF,
            bootstrap_bg,
            bootstrap_font_size,
            window_system.clone(),
            self.font_sizing,
        );
        let default_resolved = face_resolver.default_face();
        let frame_font_domain =
            FrameFontDomain::for_frame(window_system.is_some(), bootstrap_font_size);
        let default_geometry = if window_system.is_some() {
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
                }
            }
            // A graphic layout engine without font services retains the last
            // coherent frame geometry instead of publishing a partial update.
            None => {}
        }

        // --- Frame layout convergence loop (GNU xdisp.c redisplay retries) ---
        //
        // After laying out all windows we check whether the minibuffer
        // used more (or fewer) display rows than its allocated height.
        // If so we call grow_mini_window / shrink_mini_window and
        // A typed coordinator gives frame chrome, leaf chrome, and minibuffer
        // allocation one bounded retry policy. Every retry discards the whole
        // speculative output; only an iteration that requests no relayout can
        // reach presentation sealing below.
        let mut layout_coordinator = FrameLayoutCoordinator::new(MAX_FRAME_LAYOUT_RETRIES);
        let mut lisp_ledger = RedisplayLispLedger::default();
        let mut window_chrome_metrics = self.retained_window_chrome_metrics.clone();
        let committed_face_arena = self
            .frame_face_arenas
            .get(&frame_id)
            .cloned()
            .unwrap_or_default();
        let mut minibuffer_measurement_needs_begv = query_window.is_none();
        let mut frame_window_end_attempts = FrameWindowEndAttempts::default();
        let layout_walk_purpose = if query_window.is_some() {
            WindowLayoutWalkPurpose::SynchronousQuery
        } else {
            WindowLayoutWalkPurpose::Redisplay
        };

        let (
            frame_params,
            curr_window_infos,
            retained_keys,
            accepted_window_chrome_metrics,
            accepted_face_attempt,
            accepted_window_navigation_intents,
            accepted_frame_navigation_intent,
        ) = 'frame_layout: loop {
            // Re-entering this loop means the preceding physical frame walk
            // was rejected. Restore every leaf publication before collecting
            // the next attempt's canonical previous-viewport evidence.
            frame_window_end_attempts.reject_all(evaluator);
            // Layout retries are speculative. GNU logs invalid face references
            // observed by the accepted redisplay, not once per discarded
            // geometry attempt.
            face_resolver.clear_diagnostics();
            let attempt_topology_generation =
                evaluator.frame_manager().window_topology_generation();

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
            let (frame_params, mut window_params_list, query_main_area_bottom) =
                if let Some(target) = query_window {
                    let target = neovm_core::window::WindowId(target.0 as u64);
                    let Some(path) = window_paths.get(&target) else {
                        evaluator.retire_interaction_presentation(presentation_id);
                        self.reset_frame_attempt_state();
                        return None;
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
                        return None;
                    };
                    (
                        inputs.frame,
                        vec![inputs.window],
                        Some(inputs.main_area_bottom),
                    )
                } else {
                    match super::neovm_bridge::collect_layout_params_with_font_sizing(
                        evaluator,
                        frame_id,
                        default_metrics.map(|metrics| metrics.ascent),
                        self.font_sizing,
                    ) {
                        Some((frame, windows)) => (frame, windows, None),
                        None => {
                            tracing::error!("layout_frame_rust: frame {:?} not found", frame_id);
                            evaluator.retire_interaction_presentation(presentation_id);
                            return None;
                        }
                    }
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

            let resolved_window_sources: Vec<_> = window_params_list
                .iter()
                .map(|params| {
                    resolve_window_display_source_params(evaluator, params, layout_walk_purpose)
                })
                .collect();
            let window_sources: Vec<_> = resolved_window_sources
                .iter()
                .map(|resolved| resolved.source)
                .collect();
            window_params_list = resolved_window_sources
                .into_iter()
                .map(|resolved| resolved.params)
                .collect();
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
            for params in &window_params_list {
                let buf_id = neovm_core::buffer::BufferId(params.buffer_id);
                if let Some(buffer) = evaluator.buffer_manager().get(buf_id) {
                    self.pre_fontify_dirty_spans
                        .entry(params.buffer_id)
                        .or_insert_with(|| buffer.changed_char_range());
                }
            }

            self.reset_frame_attempt_state();
            let mut face_attempt = committed_face_arena.begin_attempt();
            self.frame_output.set_face_attempt(face_attempt.clone());
            self.frame_output.set_presentation_id(presentation_id);
            let mut curr_window_infos: rustc_hash::FxHashMap<DisplayWindowId, WindowInfo> =
                rustc_hash::FxHashMap::default();
            let mut observed_window_navigation_intents = Vec::new();
            let default_resolved = face_resolver.default_face();
            let child_frame_border = face_resolver.resolve_named_face("child-frame-border");

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
                    background_alpha: 1.0,
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

            let window_layout_inputs: Vec<(WindowFrameGeometry, WindowLayoutBox)> =
                window_params_list
                    .iter()
                    .map(|params| {
                        let geometry = WindowFrameGeometryRequest::new(
                            params,
                            &frame_params,
                            main_area_bottom,
                        )
                        .resolve();
                        let layout_box = WindowLayoutBox::resolve(
                            params,
                            WindowChromeMetrics::from_params(params),
                            WindowDividerLayout::resolve(params, &frame_params, geometry),
                        );
                        (geometry, layout_box)
                    })
                    .collect();

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
            let mut window_plans: Vec<IncrementalWindowPlan> = window_params_list
                .iter()
                .zip(&window_layout_inputs)
                .map(|(params, (_, layout_box))| {
                    if query_window.is_some() {
                        return IncrementalWindowPlan {
                            cursor_only: None,
                            scroll: None,
                            is_edit: false,
                        };
                    }
                    let cursor_only = self.build_cursor_only_replay(params, *layout_box, evaluator);
                    let mut is_edit = false;
                    let scroll = if cursor_only.is_none() {
                        if let Some(scroll) =
                            self.build_scroll_replay(params, *layout_box, evaluator)
                        {
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
                    IncrementalWindowPlan {
                        cursor_only,
                        scroll,
                        is_edit,
                    }
                })
                .collect();

            // Admit all prior-frame face identities as one atomic batch before
            // the tab bar or any window can mint fresh IDs. A stale retained
            // generation invalidates every fast path for this attempt.
            if let Err(error) =
                admit_retained_frame_faces(&window_plans, &mut face_attempt, &committed_face_arena)
            {
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
            {
                let request = FrameRelayoutRequest::FrameTabBar {
                    assumed_height: tab_bar_height,
                    measured_height: actual_tab_bar_height,
                };
                if !Self::accept_frame_relayout_request(
                    &mut layout_coordinator,
                    evaluator,
                    &mut frame_window_end_attempts,
                    presentation_id,
                    request,
                ) {
                    return None;
                }
                if let Some(frame) = evaluator.frame_manager_mut().get_mut(frame_id) {
                    frame.tab_bar_height = actual_tab_bar_height.max(1.0) as u32;
                    frame.sync_window_area_bounds();
                }
                continue;
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
                    if !Self::accept_frame_relayout_request(
                        &mut layout_coordinator,
                        evaluator,
                        &mut frame_window_end_attempts,
                        presentation_id,
                        request,
                    ) {
                        return None;
                    }
                    continue 'frame_layout;
                };
                let mut live_inputs = LiveWindowLayoutInputs {
                    frame: frame_params.clone(),
                    window: planned_params.clone(),
                    source: window_sources[window_index],
                    main_area_bottom,
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
                        if !Self::accept_frame_relayout_request(
                            &mut layout_coordinator,
                            evaluator,
                            &mut frame_window_end_attempts,
                            presentation_id,
                            request,
                        ) {
                            return None;
                        }
                        continue 'frame_layout;
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

                let mut cursor_only_replay = plan.cursor_only.take();
                let mut scroll_replay = plan.scroll.take();
                let mut is_edit = plan.is_edit;
                let mut visibility_retry_budget = if query_window.is_some() {
                    0
                } else {
                    MAX_WINDOW_VISIBILITY_RETRIES
                };
                let mut viewport_retry_phase = ViewportResolutionPhase::Resolve;
                let window_layout = loop {
                    match self.layout_window_rust(
                        evaluator,
                        frame_id,
                        &live_inputs.window,
                        &live_inputs.frame,
                        &layout_box,
                        &face_resolver,
                        window_geometry.reserve_terminal_right_border_col,
                        visibility_retry_budget,
                        viewport_retry_phase.clone(),
                        cursor_only_replay.take(),
                        scroll_replay.take(),
                        is_edit,
                        position_publication,
                        &mut lisp_ledger,
                        attempt_topology_generation,
                        &face_attempt,
                    ) {
                        LeafLayoutAttempt::Completed {
                            outcome,
                            window_end_attempt,
                        } => {
                            frame_window_end_attempts.stage(window_end_attempt);
                            break outcome;
                        }
                        LeafLayoutAttempt::Retry(retry) => {
                            // Same-inputs continuation: the attempt committed a
                            // new window start (or point) and asks for one more
                            // bounded attempt.  Do not re-collect live inputs —
                            // the retry carries the exact params the recursive
                            // call site used to build.  Replays were already
                            // consumed; position publication keeps the value
                            // the attempt derived it from.
                            live_inputs.window = retry.params;
                            visibility_retry_budget = retry.remaining_visibility_retries;
                            viewport_retry_phase = retry.viewport_resolution;
                            is_edit = false;
                        }
                        LeafLayoutAttempt::Effect(effect) => {
                            lisp_ledger.acknowledge_scroll_hook(&effect);
                            effect.execute_inline(evaluator);
                            lisp_ledger
                                .acknowledge_live_hook_resume(evaluator, frame_id, window_id);
                            let current_topology_generation =
                                evaluator.frame_manager().window_topology_generation();
                            if current_topology_generation != attempt_topology_generation {
                                let request = FrameRelayoutRequest::WindowTopologyChanged {
                                    before: attempt_topology_generation,
                                    after: current_topology_generation,
                                };
                                if !Self::accept_frame_relayout_request(
                                    &mut layout_coordinator,
                                    evaluator,
                                    &mut frame_window_end_attempts,
                                    presentation_id,
                                    request,
                                ) {
                                    return None;
                                }
                                continue 'frame_layout;
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
                                if !Self::accept_frame_relayout_request(
                                    &mut layout_coordinator,
                                    evaluator,
                                    &mut frame_window_end_attempts,
                                    presentation_id,
                                    request,
                                ) {
                                    return None;
                                }
                                continue 'frame_layout;
                            };
                            current.window.force_start |= consumed_force_start;
                            live_inputs = current;
                            window_geometry = WindowFrameGeometryRequest::new(
                                &live_inputs.window,
                                &live_inputs.frame,
                                live_inputs.main_area_bottom,
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
                            // A hook effect re-enters this leaf from its live
                            // inputs, which is the same fresh attempt the old
                            // recursion performed by unwinding to this loop.
                            visibility_retry_budget = if query_window.is_some() {
                                0
                            } else {
                                MAX_WINDOW_VISIBILITY_RETRIES
                            };
                            viewport_retry_phase = ViewportResolutionPhase::Resolve;
                        }
                        LeafLayoutAttempt::LogicalInputsChanged => {
                            let request = FrameRelayoutRequest::LogicalInputsChanged {
                                window_id: DisplayWindowId::new(live_inputs.window.window_id),
                            };
                            if !Self::accept_frame_relayout_request(
                                &mut layout_coordinator,
                                evaluator,
                                &mut frame_window_end_attempts,
                                presentation_id,
                                request,
                            ) {
                                return None;
                            }
                            continue 'frame_layout;
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
                        if !Self::accept_frame_relayout_request(
                            &mut layout_coordinator,
                            evaluator,
                            &mut frame_window_end_attempts,
                            presentation_id,
                            request,
                        ) {
                            return None;
                        }
                        window_chrome_metrics
                            .insert(DisplayWindowId::new(params.window_id), measured);
                        continue 'frame_layout;
                    }
                };
                if params.is_minibuffer() {
                    minibuffer_measurement_needs_begv = false;
                }

                retained_keys[window_index] = (
                    DisplayWindowId::new(params.window_id),
                    RetainedWindowKey::from_params(params, accepted_layout_box, evaluator),
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
                        let buffer_id = evaluator
                            .frame_manager()
                            .get(frame_id)?
                            .find_window(target)?
                            .buffer_id()?;
                        let buffer = evaluator.buffer_manager().get(buffer_id)?;
                        let buffer_z = neovm_core::buffer::LispCharPos1::from_one_based_usize(
                            buffer.point_max_char_pos().get().saturating_add(1),
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
                let geometry = query_snapshot.cloned();
                for face_name in face_resolver.take_invalid_face_references() {
                    evaluator.add_to_log(&format!("Invalid face reference: {face_name}"));
                }
                frame_window_end_attempts.reject_all(evaluator);
                evaluator.retire_interaction_presentation(presentation_id);
                self.reset_frame_attempt_state();
                return Some(neovm_core::window::WindowLayoutQuery::new(end, geometry));
            }

            // --- Minibuffer auto-resize check (GNU xdisp.c:13161-13301) ---
            //
            // After laying out all windows, check if the minibuffer used
            // more display rows than its allocated height. If so, grow
            // the minibuffer and re-layout the entire frame (one retry).
            // Also shrink back when the minibuffer content fits in fewer
            // rows than currently allocated.
            if let Some(mini_params) = window_params_list.last()
                && mini_params.is_minibuffer()
                && let Some(mini_content_height_px) = self.output_window_content_height_px(
                    mini_params.window_id,
                    frame_params.char_height.max(1.0),
                )
            {
                let char_h = frame_params.char_height.max(1.0);
                let mini_rows_used = (mini_content_height_px / char_h).ceil().max(1.0) as usize;
                let allocated_rows = (mini_params.bounds.height / char_h).floor().max(1.0) as usize;
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
                // (exact_p || BEGV == ZV)` (xdisp.c:13395). An empty
                // mini/echo buffer is exactly one line.
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

                if let Some(required_rows) =
                    minibuffer_growth_target(mini_rows_used, allocated_rows, max_mini_lines)
                {
                    // --- Grow ---
                    let delta = (required_rows as i32) - (allocated_rows as i32);

                    if resize_mode.should_grow() {
                        tracing::debug!(
                            "minibuffer auto-resize: grow by {} rows \
                                         (used={}, required={}, allocated={})",
                            delta,
                            mini_rows_used,
                            required_rows,
                            allocated_rows,
                        );
                        let request = FrameRelayoutRequest::Minibuffer {
                            window_id: DisplayWindowId::new(mini_params.window_id),
                            allocated_rows,
                            required_rows,
                        };
                        if !Self::accept_frame_relayout_request(
                            &mut layout_coordinator,
                            evaluator,
                            &mut frame_window_end_attempts,
                            presentation_id,
                            request,
                        ) {
                            return None;
                        }
                        if let Some(frame) = evaluator.frame_manager_mut().get_mut(frame_id) {
                            frame.grow_mini_window_with_max_lines(delta, max_mini_lines);
                        }
                        continue; // restart the layout loop
                    }
                } else if mini_rows_used < allocated_rows && allocated_rows > 1 {
                    // --- Shrink ---
                    // `exact_p` is GNU's post-command exact resize
                    // (`resize_echo_area_exactly`, run with
                    // `minibuf_level == 0`); `visible_region_empty`
                    // (computed above) is the `BEGV == ZV` case.
                    let exact = evaluator.echo_area_resize_exact_pending();
                    let should_shrink = resize_mode.should_shrink(exact, visible_region_empty);

                    if should_shrink {
                        tracing::debug!(
                            "minibuffer auto-resize: shrink \
                                         (used={}, allocated={})",
                            mini_rows_used,
                            allocated_rows,
                        );
                        let request = FrameRelayoutRequest::Minibuffer {
                            window_id: DisplayWindowId::new(mini_params.window_id),
                            allocated_rows,
                            required_rows: mini_rows_used,
                        };
                        if !Self::accept_frame_relayout_request(
                            &mut layout_coordinator,
                            evaluator,
                            &mut frame_window_end_attempts,
                            presentation_id,
                            request,
                        ) {
                            return None;
                        }
                        if let Some(frame) = evaluator.frame_manager_mut().get_mut(frame_id) {
                            frame.shrink_mini_window();
                        }
                        continue; // restart the layout loop
                    }
                }
            }

            let frame_navigation_observation = self.render_frame_output_hints(
                &previous_visual_history,
                &curr_window_infos,
                &frame_params,
                pending_frame_navigation.map(|intent| intent.direction()),
            );
            let observed_frame_navigation = frame_navigation_observation
                .direction_to_acknowledge()
                .and_then(|direction| {
                    pending_frame_navigation.filter(|intent| intent.direction() == direction)
                });

            let current_topology_generation =
                evaluator.frame_manager().window_topology_generation();
            if current_topology_generation != attempt_topology_generation {
                let request = FrameRelayoutRequest::WindowTopologyChanged {
                    before: attempt_topology_generation,
                    after: current_topology_generation,
                };
                if !Self::accept_frame_relayout_request(
                    &mut layout_coordinator,
                    evaluator,
                    &mut frame_window_end_attempts,
                    presentation_id,
                    request,
                ) {
                    return None;
                }
                continue 'frame_layout;
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
            break (
                frame_params,
                curr_window_infos,
                retained_keys,
                accepted_window_chrome_metrics,
                face_attempt,
                observed_window_navigation_intents,
                observed_frame_navigation,
            );
        };

        // Frame chrome (menu / tool / tab bar) is not a window, so no
        // `(:window …)` `:filtered` predicate may match here. Clear the
        // per-window parameters left over from the window loop above (GNU
        // evaluates chrome faces with no window ⇒ such filters fail).
        face_resolver.set_current_window_parameters(Vec::new());
        face_resolver.set_current_window_id(None);

        // Position the already-evaluated GUI chrome before publishing the
        // frame. FrameChrome is the single owner of band ordering and absolute
        // placement; this phase is deliberately Lisp-free.
        if let Some(PreparedGuiChromeSemantics {
            menu_items,
            tool_items,
            ..
        }) = prepared_gui_chrome
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
                let tool_face = face_resolver.resolve_named_face("tool-bar");
                let content = layout_gui_compact_bar_content(
                    menu_items,
                    tool_items,
                    frame_params.width,
                    frame_params.compact_bar_height,
                    frame_params.char_width,
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
                    let terminal_face = face_resolver.resolve_named_face("menu");
                    // GNU `display_menu_bar` lays each item out with a field width
                    // of SCHARS + 1 measured in the frame's character metric:
                    // pixels on a window-system frame, cells on a terminal frame.
                    // A fixed pixel gutter is ~1 char in the GUI but many cells in
                    // the TTY (char_width == 1), which over-inflates items and
                    // drops the trailing menus. Use the pixel gutter only for
                    // window-system frames; on a terminal frame use half a cell per
                    // side, giving GNU's one-character (SCHARS + 1) separation.
                    let menu_h_padding = if window_system.is_some() {
                        crate::gui_chrome::GUI_CHROME_HORIZONTAL_PADDING
                    } else {
                        frame_params.char_width * 0.5
                    };
                    let content = layout_gui_menu_bar_content(
                        menu_items,
                        frame_params.width,
                        frame_params.menu_bar_height,
                        frame_params.char_width,
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
                    let face = face_resolver.resolve_named_face("tool-bar");
                    let content = layout_gui_tool_bar_content(
                        tool_items,
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

        let mut frame_display_state = match self.finish_frame_output(&frame_params) {
            Ok(state) => state,
            Err(error) => {
                tracing::error!(?error, "rejecting invalid frame chrome snapshot");
                frame_window_end_attempts.reject_all(evaluator);
                evaluator.retire_interaction_presentation(presentation_id);
                return None;
            }
        };
        let sealed_face_arena = match accepted_face_attempt.seal(frame_display_state.faces.clone())
        {
            Ok(arena) => arena,
            Err(error) => {
                tracing::error!(?error, "rejecting incoherent frame face namespace");
                frame_window_end_attempts.reject_all(evaluator);
                evaluator.retire_interaction_presentation(presentation_id);
                return None;
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
        let (next_layout_stats, next_retained_window_matrices, acked_buffer_ids) = {
            let key_map: rustc_hash::FxHashMap<DisplayWindowId, RetainedWindowKey> =
                retained_keys.into_iter().collect();
            let frame_state = &mut frame_display_state;
            let mut next_layout_stats = LayoutStats::default();
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
                if cursor_only {
                    // Body rows were reused verbatim (0 relaid); chrome re-walked.
                    next_layout_stats.reused_rows += enabled_body;
                    next_layout_stats.record_window_class(LayoutClass::CursorOnly);
                } else if let Some((ref reused, _dvpos)) = scroll_reused {
                    // Overlapping rows reused shifted; the rest were newly exposed
                    // and walked.
                    let reused = reused.len().min(enabled_body);
                    next_layout_stats.reused_shifted_rows += reused;
                    next_layout_stats.relaid_body_rows += enabled_body - reused;
                    next_layout_stats.record_window_class(LayoutClass::Scroll);
                } else if let Some(ref reused) = edit_reused {
                    // Rows outside the regenerated edit span reused verbatim.
                    let reused = reused.len().min(enabled_body);
                    next_layout_stats.reused_rows += reused;
                    next_layout_stats.relaid_body_rows += enabled_body - reused;
                    next_layout_stats.record_window_class(LayoutClass::Edit);
                } else {
                    next_layout_stats.relaid_body_rows += enabled_body;
                    next_layout_stats.record_window_class(LayoutClass::Full);
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
                        if !row.enabled || is_chrome {
                            entry.matrix.set_row_damage(idx, RowDamage::New);
                            continue;
                        }
                        let d = if cursor_only {
                            RowDamage::Reused
                        } else if let Some((ref reused, dvpos)) = scroll_reused {
                            if reused.contains(idx) {
                                RowDamage::ReusedShifted { dvpos: Px(dvpos) }
                            } else {
                                RowDamage::New
                            }
                        } else if let Some(ref reused) = edit_reused {
                            if reused.contains(idx) {
                                RowDamage::Reused
                            } else {
                                RowDamage::New
                            }
                        } else {
                            RowDamage::New
                        };
                        entry.matrix.set_row_damage(idx, d);
                    }
                }

                // Probe-pass exclusion: a window that laid out <=1 enabled row
                // is the scroll-off hazard (spec §4.1); never retain it as a
                // clean reusable matrix.
                if enabled_body + enabled_chrome <= 1 {
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
                        .map(WindowPresentationSnapshot::display_snapshot)
                        .find(|snapshot| snapshot.window_id.0 as i64 == entry.window_id.get())
                        .cloned()
                    else {
                        continue;
                    };
                    retained.insert(
                        window_id,
                        RetainedWindowMatrix {
                            matrix: entry.matrix.clone(),
                            key: key.clone(),
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

        let resolved = match crate::frame_presentation::ResolvedFrame::new(frame_display_state) {
            Ok(resolved) => resolved,
            Err(error) => {
                tracing::error!(?error, "rejecting incoherent resolved frame");
                frame_window_end_attempts.reject_all(evaluator);
                evaluator.retire_interaction_presentation(presentation_id);
                return None;
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
                return None;
            }
        };
        debug_assert_eq!(sealed.presentation(), sealed.state().presentation_id);

        // Commit retained state only after the visual, spatial, and revision
        // invariants have sealed. A rejected presentation cannot acknowledge
        // buffer edits or replace the GNU "current matrix" analogue.
        self.retained_window_chrome_metrics = accepted_window_chrome_metrics;
        self.layout_stats = next_layout_stats;
        // Admitted work: what the frame SPENT to decide, not what it emitted.
        let (snapshots, composition_bytes, reused_chrome) =
            crate::neovm_bridge::take_snapshot_work();
        self.layout_stats.buffer_snapshots_built = snapshots;
        self.layout_stats.composition_bytes_scanned = composition_bytes;
        self.layout_stats.reused_chrome_rows = reused_chrome;
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
        if let Ok(stats_path) = std::env::var("NEOMACS_LAYOUT_STATS_FILE")
            && !stats_path.is_empty()
        {
            use std::io::Write as _;
            let s = &self.layout_stats;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&stats_path)
            {
                let _ = writeln!(
                    f,
                    "full={} cursor_only={} scroll={} edit={} relaid_body={} relaid_chrome={} reused={} reused_shifted={} reused_chrome={} snapshots={} compose_bytes={}",
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
        self.retained_window_matrices = next_retained_window_matrices;
        self.frame_face_arenas.insert(frame_id, sealed_face_arena);
        for buffer_id in acked_buffer_ids {
            if let Some(buffer) = evaluator
                .buffer_manager()
                .get(neovm_core::buffer::BufferId(buffer_id))
            {
                buffer.reset_unchanged_region();
            }
        }
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

        self.frame_visual_histories.commit(
            frame_id,
            FrameVisualHistory::from_accepted_presentation(
                curr_window_infos,
                Color::from_pixel(frame_params.background),
            ),
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

        let snapshots = std::mem::take(&mut self.window_snapshots);
        if let Some(frame) = evaluator.frame_manager_mut().get_mut(frame_id) {
            frame
                .prepare_display_presentation(
                    neovm_core::window::geometry::PresentationId::new(presentation_id),
                    snapshots,
                )
                .expect("layout presentation identity is fresh");
        }
        frame_window_end_attempts.accept_all();
        for face_name in face_resolver.take_invalid_face_references() {
            evaluator.add_to_log(&format!("Invalid face reference: {face_name}"));
        }
        None
    }

    /// Recompute one live window through the canonical row producer, GNU's
    /// `start_display` + `move_it_to` from `w->start`.
    ///
    /// Answers both display questions the walk settles at once:
    /// GNU-compatible `window-end`, and the window's display geometry.
    pub fn query_window_layout(
        &mut self,
        evaluator: &mut neovm_core::emacs_core::Context,
        frame_id: neovm_core::window::FrameId,
        window_id: neovm_core::window::WindowId,
    ) -> Result<neovm_core::window::WindowLayoutQuery, neovm_core::window::WindowLayoutQueryFailure>
    {
        self.layout_frame_rust_for_purpose_inner(
            evaluator,
            frame_id,
            LayoutPurpose::SynchronousQuery { window_id },
        )
        .ok_or(neovm_core::window::WindowLayoutQueryFailure::DidNotConverge)
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
        if let Some(parked) = self.retained_frame.take() {
            self.retained_by_frame.insert(
                parked,
                RetainedFrameState {
                    matrices: std::mem::take(&mut self.retained_window_matrices),
                    chrome_metrics: std::mem::take(&mut self.retained_window_chrome_metrics),
                },
            );
        }
        let loaded = self.retained_by_frame.remove(&frame_id).unwrap_or_default();
        self.retained_window_matrices = loaded.matrices;
        self.retained_window_chrome_metrics = loaded.chrome_metrics;
        self.retained_frame = Some(frame_id);
    }

    fn build_cursor_only_replay(
        &self,
        params: &WindowParams,
        layout_box: WindowLayoutBox,
        evaluator: &neovm_core::emacs_core::Context,
    ) -> Option<CursorOnlyReplay> {
        // The cursor-only fast path applies to ANY window. The render cursor branch
        // handles both styles (replay.cursor_style is hollow for a non-selected
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
        {
            if let Some(chrome) = prev.retained_chrome() {
                crate::neovm_bridge::CHROME_ROWS_REUSED
                    .fetch_add(chrome.rows.len(), std::sync::atomic::Ordering::Relaxed);
                replay.chrome = Some(chrome);
            }
        }
        Some(replay)
    }

    /// Smooth scroll (Phase 1): the laid-out body rows of `window_id` from the
    /// retained matrix, as `(start_charpos, height_px)` metrics in top-to-bottom
    /// order, for resolving a pixel scroll via
    /// [`crate::pixel_scroll::resolve_pixel_scroll`]. `None` if the window has no
    /// retained matrix yet or no body (non-chrome, text-displaying) rows.
    pub fn current_body_row_metrics(
        &self,
        window_id: DisplayWindowId,
    ) -> Option<Vec<crate::pixel_scroll::ScrollRowMetric>> {
        let retained = self.retained_window_matrices.get(&window_id)?;
        let rows: Vec<crate::pixel_scroll::ScrollRowMetric> = retained
            .matrix
            .rows
            .iter()
            .filter(|row| {
                row.enabled && row.displays_text && !RetainedWindowMatrix::is_chrome_role(row.role)
            })
            .map(|row| crate::pixel_scroll::ScrollRowMetric {
                start_charpos: row.start_charpos as i64,
                height_px: row.height_px.round() as i32,
            })
            .collect();
        if rows.is_empty() { None } else { Some(rows) }
    }

    /// Smooth scroll (Phase 1, T3b): resolve a vertical pixel scroll of `delta_px`
    /// (positive = down / content up, negative = up) for `window_id` using the
    /// retained matrix's real row heights, and commit it (marker-backed
    /// window-start + vscroll) onto `evaluator`. Returns `Some(())` if applied, or
    /// `None` if there is no retained matrix yet or the current window-start is not
    /// a laid-out body row (the caller then falls back to a normal relayout).
    pub fn pixel_scroll_window(
        &self,
        evaluator: &mut neovm_core::emacs_core::Context,
        window_id: neovm_core::window::WindowId,
        delta_px: i32,
    ) -> Option<()> {
        let metrics = self.current_body_row_metrics(DisplayWindowId::new(window_id.0 as i64))?;

        // Read the current window-start (1-based) and vscroll (stored <= 0).
        let frame_id = evaluator.frame_manager().find_window_frame_id(window_id)?;
        let (cur_start_1based, cur_vscroll_raw) = match evaluator
            .frame_manager()
            .get(frame_id)?
            .find_window(window_id)?
        {
            neovm_core::window::Window::Leaf {
                window_start,
                vscroll,
                ..
            } => (window_start.as_i64(), *vscroll),
            _ => return None,
        };

        // Row metrics carry 0-based charpos; window-start is 1-based. The residual
        // hidden above the top edge is `-vscroll` (vscroll is stored zero-or-negative).
        let cur_start_0based = cur_start_1based - 1;
        let top_idx = metrics
            .iter()
            .position(|m| m.start_charpos == cur_start_0based)?;
        let cur_residual = (-cur_vscroll_raw).max(0);

        let (new_top_idx, new_residual) =
            crate::pixel_scroll::resolve_pixel_scroll(&metrics, top_idx, cur_residual, delta_px);
        let new_start =
            neovm_core::buffer::LispCharPos1::new(metrics[new_top_idx].start_charpos + 1);

        evaluator
            .apply_pixel_scroll(window_id, new_start, new_residual)
            .then_some(())
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
        // A genuine scroll keeps walking chrome, and structurally rather than by
        // trusting a trigger: `%p` is computed from window-start/window-end, so
        // chrome whose visible region moved is stale by definition. Leaving
        // `chrome` at `None` is that decision. (GNU agrees from the other side —
        // its scroll commands all call `wset_update_mode_line`, window.c:6279,
        // 6418, 6603, 6864.)
        prev.scroll_replay(&curr_key)
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
        let mut span_newlines = 0usize;
        let simple_span = self.allow_below_reuse
            && (dirty_start..dirty_end).all(|cp| {
                let byte = buffer.char_pos_to_emacs_byte_pos_clamped(
                    neovm_core::buffer::CharPos0::new(cp as usize),
                );
                match buffer.char_at_emacs_byte_pos(byte) {
                    Some('\n') => {
                        span_newlines += 1;
                        true
                    }
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
        let span_structure_safe = simple_span
            && !buffer.has_any_non_nil_property_in_char_range(
                structure_range,
                EditReplayStructureProperty::symbols(),
            );
        let damage = EditDamage::new(dirty_start, dirty_end, delta, span_newlines);
        let Some(mut replay) = prev.edit_replay(&curr_key, damage, span_structure_safe) else {
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
                    span_newlines,
                    span_structure_safe,
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
        if prev.chrome_reusable_after_edit(
            &replay,
            damage,
            chrome_reuse_context(params, evaluator),
            |from, to| {
                (from.max(0)..to.max(0)).any(|cp| {
                    let byte = buffer.char_pos_to_emacs_byte_pos_clamped(
                        neovm_core::buffer::CharPos0::new(cp as usize),
                    );
                    buffer.char_at_emacs_byte_pos(byte) == Some('\n')
                })
            },
        ) {
            if let Some(chrome) = prev.retained_chrome() {
                crate::neovm_bridge::CHROME_ROWS_REUSED
                    .fetch_add(chrome.rows.len(), std::sync::atomic::Ordering::Relaxed);
                replay.chrome = Some(chrome);
            }
        }
        Some(replay)
    }

    fn layout_window_rust(
        &mut self,
        evaluator: &mut neovm_core::emacs_core::Context,
        frame_id: neovm_core::window::FrameId,
        params: &WindowParams,
        frame_params: &FrameParams,
        layout_box: &WindowLayoutBox,
        face_resolver: &super::neovm_bridge::FaceResolver,
        reserve_right_border_col: bool,
        remaining_visibility_retries: usize,
        viewport_resolution: ViewportResolutionPhase,
        // Phase A (gather) classified this window's incremental fast path against
        // the *original* params (before any echo-buffer swap below), reading the
        // same retained key the predicate was snapshotted from. Phase B (here)
        // consumes the plan inside the render path in place of the body walk.
        // `is_edit` only steers the commit-path stats classification.
        cursor_only_replay: Option<CursorOnlyReplay>,
        scroll_replay: Option<ScrollReplay>,
        is_edit: bool,
        position_publication: WindowPositionPublication,
        lisp_ledger: &mut RedisplayLispLedger,
        topology_generation: u64,
        face_attempt: &FrameFaceAttempt,
    ) -> LeafLayoutAttempt {
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
        let site_publication =
            lisp_ledger.publication_for_site(position_publication, scroll_hook_site);
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
        let text_height = params.bounds.height - params.mode_line_height;
        let max_rows = if params.char_height > 0.0 {
            (text_height / params.char_height).ceil() as i64
        } else {
            50
        };
        let fontify_end = (window_start + max_rows * 200).min(accessible_end);
        let Some(freshness_before_fontification) =
            evaluator.window_layout_attempt_freshness(frame_id, window_id, buf_id)
        else {
            return LeafLayoutAttempt::LogicalInputsChanged;
        };
        let _ = Self::ensure_fontified_rust(evaluator, buf_id, window_start, fontify_end);
        if evaluator.frame_manager().window_topology_generation() != topology_generation {
            return LeafLayoutAttempt::LogicalInputsChanged;
        }
        let Some(freshness_after_fontification) =
            evaluator.window_layout_attempt_freshness(frame_id, window_id, buf_id)
        else {
            return LeafLayoutAttempt::LogicalInputsChanged;
        };
        if freshness_after_fontification != freshness_before_fontification {
            return LeafLayoutAttempt::LogicalInputsChanged;
        }

        let scroll_dvpos = scroll_replay
            .as_ref()
            .map(|replay| replay.dvpos)
            .unwrap_or(0.0);
        // `(when FORM . SPEC)` display specs need Lisp, which the walk cannot
        // run while it holds the buffer: evaluate the forms of the span the
        // walk can reach (the same one fontification just covered) now and
        // let the snapshot carry the results.
        let display_when = crate::display_when::evaluate_window_display_when_forms(
            evaluator,
            buf_id,
            neovm_core::buffer::CharPos0::new(window_start.max(0) as usize),
            neovm_core::buffer::CharPos0::new(fontify_end.max(0) as usize),
        );
        // `params` already names the semantic display source chosen before
        // fontification and incremental classification.
        let layout_buffer = match evaluator.buffer_manager().get(buf_id) {
            Some(buffer) => super::neovm_bridge::LayoutBufferSnapshot::from_buffer_for_window(
                buffer,
                evaluator.obarray(),
                Some(visible_char_bound(params)),
            )
            .with_display_when(display_when),
            None => {
                tracing::debug!("layout_window_rust: buffer {} not found", params.buffer_id);
                self.frame_output
                    .render_window_info(WindowFrameInfoRenderRequest::new(
                        params,
                        live_window_frame_metadata(evaluator, buf_id),
                    ));
                self.window_snapshots
                    .push(WindowPresentationSnapshot::LiveWindow(
                        WindowDisplaySnapshot {
                            window_id,
                            cell_origin: neovm_core::window::geometry::CellOrigin::new(
                                params.left_col,
                                params.top_line,
                            ),
                            regions: layout_box.regions(),
                            regions_materialized: false,
                            ..Default::default()
                        },
                    ));
                return LeafLayoutAttempt::Completed {
                    outcome: WindowLayoutOutcome::Skipped,
                    window_end_attempt: None,
                };
            }
        };
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
        let render_outcome = BufferWindowRenderRequest::new(
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
        .with_forward_viewport_measurement(match &viewport_resolution {
            ViewportResolutionPhase::Measure(measurement) => Some(measurement.clone()),
            ViewportResolutionPhase::Resolve | ViewportResolutionPhase::Commit(_) => None,
        })
        .render_into(
            BufferSourceRenderAttemptContext::from_frame_output_owner(
                &mut self.frame_output,
                evaluator,
                &mut self.font_metrics,
                face_resolver,
                face_attempt.clone(),
                &mut self.window_snapshots,
            ),
            &mut self.text_buf,
            remaining_visibility_retries,
            cursor_only_replay,
            scroll_replay,
        );

        // Body and chrome can evaluate arbitrary Lisp after fontification.
        // Rows are valid only if the complete typed source projection still
        // equals the one they were produced from. Reject before attaching the
        // post-callback token to those rows; otherwise stale geometry could be
        // certified as fresh.
        let freshness_after_leaf =
            evaluator.window_layout_attempt_freshness(frame_id, window_id, buf_id);
        let lisp_boundaries_remain_valid = match (&render_outcome, freshness_after_leaf) {
            (
                BufferSourceRenderAttemptOutcome::Finished {
                    freshness_before_chrome,
                    ..
                },
                Some(freshness_after_leaf),
            ) => {
                freshness_after_fontification.remains_valid_across(
                    *freshness_before_chrome,
                    neovm_core::window::WindowLayoutLispBoundary::BufferBody,
                ) && freshness_before_chrome.remains_valid_across(
                    freshness_after_leaf,
                    neovm_core::window::WindowLayoutLispBoundary::WindowChrome,
                )
            }
            (_, Some(freshness_after_leaf)) => {
                freshness_after_leaf == freshness_after_fontification
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

        // The contiguous pre-pass above is an optimization, not a semantic
        // visibility boundary.  A provisional row walk can jump over an
        // arbitrarily large invisible/folded span and reach visible positions
        // beyond that estimate.  GNU handles `fontified` at exactly those
        // iterator stops.  Our immutable walk records the equivalent visible
        // positions; fontify any uncovered sparse spans and retry from a fresh
        // snapshot before accepting their provisional glyphs.
        if matches!(
            &render_outcome,
            BufferSourceRenderAttemptOutcome::Finished { .. }
        ) {
            let coverage = self
                .window_snapshots
                .iter()
                .rev()
                .map(WindowPresentationSnapshot::display_snapshot)
                .find(|snapshot| snapshot.window_id == window_id)
                .map(|snapshot| {
                    VisibleFontificationCoverage::inspect(
                        buffer,
                        snapshot,
                        neovm_core::buffer::CharPos0::new(fontify_end.max(0) as usize),
                    )
                })
                .unwrap_or(VisibleFontificationCoverage::Complete);

            if let VisibleFontificationCoverage::Requires(plan) = coverage {
                for span in plan.spans() {
                    let outcome =
                        Self::ensure_fontified_rust(evaluator, buf_id, span.start(), span.end());
                    let freshness_after_visible_fontification =
                        evaluator.window_layout_attempt_freshness(frame_id, window_id, buf_id);
                    if evaluator.frame_manager().window_topology_generation() != topology_generation
                        || freshness_after_visible_fontification != freshness_after_leaf
                        || outcome.requires_layout_retry()
                    {
                        if let Some(attempt) = window_end_attempt.take() {
                            evaluator.reject_redisplay_window_end_attempt(attempt);
                        }
                        return LeafLayoutAttempt::LogicalInputsChanged;
                    }
                }
            }
        }

        let (redisplay_positions, effective_default_face) = match render_outcome {
            BufferSourceRenderAttemptOutcome::LogicalInputsChanged => {
                if let Some(attempt) = window_end_attempt.take() {
                    evaluator.reject_redisplay_window_end_attempt(attempt);
                }
                return LeafLayoutAttempt::LogicalInputsChanged;
            }
            BufferSourceRenderAttemptOutcome::Skipped => {
                self.frame_output
                    .render_window_info(WindowFrameInfoRenderRequest::new(
                        params,
                        live_window_frame_metadata(evaluator, buf_id),
                    ));
                self.window_snapshots
                    .push(WindowPresentationSnapshot::LiveWindow(
                        WindowDisplaySnapshot {
                            window_id,
                            cell_origin: neovm_core::window::geometry::CellOrigin::new(
                                params.left_col,
                                params.top_line,
                            ),
                            regions: layout_box.regions(),
                            regions_materialized: false,
                            ..WindowDisplaySnapshot::default()
                        },
                    ));
                self.mark_inactive_echo_snapshot_geometry_only(window_id, position_publication);
                return LeafLayoutAttempt::Completed {
                    outcome: WindowLayoutOutcome::Skipped,
                    window_end_attempt,
                };
            }
            BufferSourceRenderAttemptOutcome::ReplayMispredicted => {
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
                let resolved_start = evaluator
                    .redisplay_start_before_point_by_display_rows(
                        buf_id,
                        window_id,
                        point,
                        lines_above_point,
                    )
                    .map_or(fallback_window_start, |start| {
                        ResolvedWindowStart::from_layout_charpos(start.get() as i64)
                    });
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
                window_end_record,
                freshness_before_chrome: _,
                effective_default_face,
                cursor_only,
                reused_matrix_rows,
            } => {
                if let Some(snapshot) = self
                    .window_snapshots
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
                (redisplay_positions, effective_default_face)
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
            ));

        tracing::debug!(
            "  layout_window_rust: window_start={} window_end={}",
            redisplay_positions.window_start().as_i64(),
            redisplay_positions.window_end_lisp().as_i64()
        );

        let assumed = WindowChromeMetrics::from_params(params);
        let measured = self
            .window_snapshots
            .iter()
            .rev()
            .map(WindowPresentationSnapshot::display_snapshot)
            .find(|snapshot| snapshot.window_id == window_id)
            .map(WindowChromeMetrics::from_snapshot)
            .unwrap_or(assumed);
        let outcome =
            WindowLayoutOutcome::from_measurement(*layout_box, measured, effective_default_face);
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
            self.mark_inactive_echo_snapshot_geometry_only(window_id, position_publication);
        }
        LeafLayoutAttempt::Completed {
            outcome,
            window_end_attempt,
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
    ) -> neovm_core::emacs_core::xdisp::RedisplayFontificationOutcome {
        match neovm_core::emacs_core::xdisp::ensure_fontified_for_redisplay(
            evaluator, buf_id, from, to,
        ) {
            Ok(outcome) => outcome,
            Err(e) => {
                tracing::debug!("ensure_fontified_rust: fontification error: {:?}", e);
                neovm_core::emacs_core::xdisp::RedisplayFontificationOutcome::Unchanged
            }
        }
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
        let tab_bar_face =
            face_resolver.default_base_face_for_origin_without_buffer(&DisplayOrigin::TabBar);
        let tab_bar_ascent = frame_params.char_height * 0.8;
        let fallback_metrics =
            DisplayRowFallbackMetrics::from_frame_defaults(frame_params, tab_bar_ascent);
        let metrics = DisplayRowFaceRealizer::new(&mut self.font_metrics)
            .row_metrics_for_face(&tab_bar_face, fallback_metrics);
        let mut face_ids = face_attempt.clone();
        let rendered_tab_bar = self.frame_output.render_frame_tab_bar_row(
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
            ChromeRowRenderServices::new(&mut self.font_metrics, face_resolver, &mut face_ids),
            evaluator.display_host.as_deref(),
        )?;
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
            resolved.lisp_name = value.as_symbol_name().map(str::to_owned);
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
            self.frame_output
                .install_pointer_face(face_id, realized.render_face());
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
#[path = "engine_test.rs"]
mod tests;
