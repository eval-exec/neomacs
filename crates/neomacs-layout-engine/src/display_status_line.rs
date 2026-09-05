//! Display-walker chrome row rendering.
//!
//! Mode-line, header-line, tab-line, tab-bar, and minibuffer echo rows share
//! the face realization helpers defined here. The shared row renderer and
//! property harvester live in `display_row`; this module retains the status-line
//! filename because it grew from the older mode-line-only path.
//!
//! History: this module started as a divergent
//! parallel implementation of display-line rendering that did not
//! process display properties and dropped doom-modeline's
//! (space :align-to ...) forms. Steps 3.3' through 3.6 of the
//! display-engine unification plan merged it into the backend
//! trait and renamed the file to reflect its new role.

use super::neovm_bridge::{FaceResolver, LayoutBufferView, ResolvedFace, buffer_local_value};
use super::window_output::{
    ChromeRowOutput, ChromeRowProgress, DisplayProgressSink, TextWindowOutputTarget,
    WindowOutputEmitter,
};
use crate::display_origin::DisplayOrigin;
use crate::display_rendered_row_output_install::install_measured_frame_chrome_display_row;
use crate::display_row::builder::{DisplayTabPolicy, display_row_text_is_empty};
pub(crate) use crate::display_row::face_state::DisplayRowFaceRealizer;
use crate::display_row::face_state::DisplayRowMeasurementMode;
use crate::display_row::measured_state::{
    DisplayRowBoundsPolicy, DisplayRowOwner, FrameChromeKind, MeasuredDisplayRow, WindowChromeKind,
};
use crate::display_row::metrics::DisplayRowFallbackMetrics;
pub(crate) use crate::display_row::render_state::DisplayRowOutputProgress;
use crate::display_row::render_state::{DisplayRowRenderIntoRowResult, RenderedDisplayRow};
use crate::display_row::source_state::DisplayRowSourceState;
use crate::display_row::{
    DisplayRowGeometry, DisplayRowLispStringSourceRenderRequest, DisplayRowLispStringSourceRequest,
    DisplayRowRenderExecutor, DisplayRowSourceRenderRequest,
};
use crate::display_source::DisplayItemSource;
use crate::display_source_resolver::DisplaySourceFaceScope;
use crate::font::metrics::FontMetricsService;
use crate::frame_face_arena::FrameFaceAttempt;
use crate::neovm_bridge::LayoutVar;
use crate::output::builder::DisplayOutputBuilder;
use crate::presentation::presented_pointer_map::{
    PointerAppearanceRangeId, PresentedPointerMapBuildError,
};
use crate::types::WindowParams;
use crate::window_layout::{WindowChromeMetrics, WindowLayoutBox};
#[cfg(test)]
use neomacs_display_protocol::FrameGlyphBuffer;
#[cfg(test)]
use neomacs_display_protocol::face::BoxType;
use neomacs_display_protocol::frame_chrome::{
    BandRect, ChromeAction, ChromeHitRegion, InteractionId,
};
use neomacs_display_protocol::glyph_matrix::{
    Glyph, GlyphArea, GlyphRow, GlyphStringId, GlyphType,
};
use neomacs_display_protocol::types::{Color, FaceId, Rect};
use neomacs_display_protocol::{
    FrameRect, PointerAppearanceId, PointerDrawMode, PointerImageRelief, PresentedPrimitiveKind,
};
use neovm_core::buffer::BufferId;
use neovm_core::emacs_core::Context;
use neovm_core::emacs_core::Value;
use neovm_core::emacs_core::eval::DisplayHost;
use neovm_core::emacs_core::image_catalog::ImageScaleEnvironment;
use neovm_core::emacs_core::keymap::{KeymapMarker, MenuItemProperty, is_list_keymap};
use neovm_core::emacs_core::value::list_to_vec;
use neovm_core::emacs_core::xdisp::{ModeLineDisplayOutput, ModeLineDisplaySourceSpan};
use neovm_core::keyboard::{PresentedMouseArea, PresentedMouseTarget};
use neovm_core::window::{FrameId, WindowId};
use neovm_core::window::{PresentedWindowChromeArea, PresentedWindowChromeString};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use strum::{EnumString, IntoStaticStr};

// Instrumentation: count how many times the mode line is *evaluated* (the
// expensive `format-mode-line` elisp run, ~4.3ms in a Doom config) per
// redisplay. GNU `display_mode_line` lays the mode line out exactly once per
// redisplay, after the window body is laid out (so `%p`/`%l` reflect the final
// window-start); neomacs matches that — the mode line is evaluated once, in the
// chrome render after the body, and its *measured* height (not the face-only
// estimate) is reported as `window-mode-line-height`. This counter proves the
// single-eval invariant: a measure-then-render two-pass approach increments it
// to 2, the single layout keeps it at 1. The counter is thread-local because
// layout runs on the evaluator's thread; `layout_frame_rust` resets it per
// frame.
thread_local! {
    static MODE_LINE_EVAL_COUNT: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    static HEADER_LINE_EVAL_COUNT: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    static TAB_LINE_EVAL_COUNT: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Reset the per-redisplay mode-line eval counter. Called once at the start of
/// `layout_frame_rust`.
pub fn reset_mode_line_eval_count() {
    MODE_LINE_EVAL_COUNT.with(|count| count.set(0));
    HEADER_LINE_EVAL_COUNT.with(|count| count.set(0));
    TAB_LINE_EVAL_COUNT.with(|count| count.set(0));
}

/// How many times `mode-line-format` has been evaluated for display since the
/// last `reset_mode_line_eval_count`. Used by the single-eval invariant test.
pub fn mode_line_eval_count() -> u32 {
    MODE_LINE_EVAL_COUNT.with(std::cell::Cell::get)
}

/// As [`mode_line_eval_count`], for `header-line-format`. GNU regenerates all
/// three chrome formats together (`display_mode_lines`, xdisp.c:28027+), so a
/// chrome skip must move these counters together with the mode line's.
pub fn header_line_eval_count() -> u32 {
    HEADER_LINE_EVAL_COUNT.with(std::cell::Cell::get)
}

/// As [`mode_line_eval_count`], for `tab-line-format`.
pub fn tab_line_eval_count() -> u32 {
    TAB_LINE_EVAL_COUNT.with(std::cell::Cell::get)
}

/// What one window's chrome generation established this layout.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ChromeGenerationRecord {
    /// The chrome displayed `%c`/`%C`. GNU's `w->column_number_displayed`
    /// (consulted by `mode_line_update_needed`, xdisp.c:13831-13837), kept as
    /// a bare "was it displayed" because P5.2(b) refuses the skip rather than
    /// comparing the value.
    pub(crate) uses_column: bool,
}

// Which windows actually GENERATED chrome this layout, and what that
// generation established. Two consumers at commit time: acknowledging the
// chrome dirty flag per window (a window that skipped must NOT be
// acknowledged, or its flag is eaten), and carrying `uses_column` into the
// retained matrix so the NEXT frame's skip decision can consult it.
thread_local! {
    static CHROME_GENERATION_RECORD: std::cell::RefCell<HashMap<i64, ChromeGenerationRecord>> =
        std::cell::RefCell::new(HashMap::new());
}

/// Reset the per-redisplay chrome generation record. Called with
/// [`reset_mode_line_eval_count`] at the start of `layout_frame_rust`.
pub(crate) fn reset_chrome_generation_record() {
    CHROME_GENERATION_RECORD.with(|record| record.borrow_mut().clear());
}

fn record_chrome_generated(window_id: i64, uses_column: bool) {
    CHROME_GENERATION_RECORD.with(|record| {
        let mut record = record.borrow_mut();
        let entry = record.entry(window_id).or_default();
        // All three chrome formats generate together, so OR the flag: a column
        // anywhere in a window's chrome pins that window.
        entry.uses_column |= uses_column;
    });
}

/// The windows whose chrome was generated this layout, with what it
/// established. Read once at layout commit.
pub(crate) fn chrome_generation_record() -> Vec<(i64, ChromeGenerationRecord)> {
    CHROME_GENERATION_RECORD.with(|record| record.borrow().iter().map(|(k, v)| (*k, *v)).collect())
}

fn record_mode_line_eval(format_symbol: &str) {
    if format_symbol == "mode-line-format" {
        MODE_LINE_EVAL_COUNT.with(|count| count.set(count.get() + 1));
    }
    if format_symbol == "header-line-format" {
        HEADER_LINE_EVAL_COUNT.with(|count| count.set(count.get() + 1));
    }
    if format_symbol == "tab-line-format" {
        TAB_LINE_EVAL_COUNT.with(|count| count.set(count.get() + 1));
    }
}

// Large payload variant; boxing is a perf hint deferred out of the lint gate.
#[allow(clippy::large_enum_variant)]
pub(crate) enum FrameTabBarDisplayRowRender {
    Empty,
    Measured(MeasuredDisplayRow),
}

pub(crate) struct FrameChromeOutputTarget<'a> {
    output_builder: &'a mut DisplayOutputBuilder,
}

impl<'a> FrameChromeOutputTarget<'a> {
    pub(crate) fn from_builder(output_builder: &'a mut DisplayOutputBuilder) -> Self {
        Self { output_builder }
    }

    fn builder(&mut self) -> &mut DisplayOutputBuilder {
        self.output_builder
    }

    fn install_measured_frame_chrome_display_row(&mut self, measured: &MeasuredDisplayRow) {
        install_measured_frame_chrome_display_row(self.builder(), measured);
    }
}

pub(crate) struct ChromeRowRenderServices<'emit, 'face> {
    font_metrics: &'emit mut Option<FontMetricsService>,
    measurement_mode: DisplayRowMeasurementMode,
    face_resolver: &'face FaceResolver,
    face_ids: &'emit mut FrameFaceAttempt,
}

impl<'emit, 'face> ChromeRowRenderServices<'emit, 'face> {
    pub(crate) fn new(
        font_metrics: &'emit mut Option<FontMetricsService>,
        face_resolver: &'face FaceResolver,
        face_ids: &'emit mut FrameFaceAttempt,
    ) -> Self {
        Self {
            font_metrics,
            measurement_mode: DisplayRowMeasurementMode::from_frame_window_system(
                face_resolver.is_window_system(),
            ),
            face_resolver,
            face_ids,
        }
    }

    pub(crate) fn reborrow(&mut self) -> ChromeRowRenderServices<'_, 'face> {
        ChromeRowRenderServices {
            font_metrics: self.font_metrics,
            measurement_mode: self.measurement_mode,
            face_resolver: self.face_resolver,
            face_ids: self.face_ids,
        }
    }

    pub(crate) fn resolve_frame_named_face(&self, name: &str) -> ResolvedFace {
        self.face_resolver.resolve_named_face(name)
    }

    pub(crate) fn face_ids(&mut self) -> &mut FrameFaceAttempt {
        self.face_ids
    }

    fn intrinsic_metrics_for_face(
        &mut self,
        face: &ResolvedFace,
        fallback: DisplayRowFallbackMetrics,
    ) -> DisplayRowFallbackMetrics {
        if !self.measurement_mode.uses_concrete_font_geometry() {
            return fallback;
        }
        DisplayRowFaceRealizer::new(&mut *self.font_metrics).row_metrics_for_face(face, fallback)
    }

    fn render_lisp_string_source_request(
        &mut self,
        request: DisplayRowLispStringSourceRenderRequest<'_>,
        display_host: Option<&dyn DisplayHost>,
    ) -> Option<RenderedDisplayRow> {
        let mut render_executor = DisplayRowRenderExecutor::new(
            &mut *self.font_metrics,
            self.measurement_mode,
            self.face_resolver,
            display_host,
            &mut *self.face_ids,
        );
        render_executor.render_lisp_string_source_request(request)
    }

    pub(crate) fn render_item_source_fragment_into_row(
        &mut self,
        request: DisplayRowSourceRenderRequest<'_>,
        row: &mut GlyphRow,
        source: &mut impl DisplayItemSource,
        source_state: &mut DisplayRowSourceState,
    ) -> Option<DisplayRowRenderIntoRowResult> {
        let mut render_executor = DisplayRowRenderExecutor::new(
            &mut *self.font_metrics,
            self.measurement_mode,
            self.face_resolver,
            None,
            &mut *self.face_ids,
        );
        render_executor.render_item_source_fragment_into_row(request, row, source, source_state)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_item_source_fragment_from_glyph_row_columns(
        &mut self,
        row: &mut GlyphRow,
        source: &mut impl DisplayItemSource,
        source_state: &mut DisplayRowSourceState,
        matrix_cols: usize,
        char_width: f32,
        face_id: FaceId,
        base_face: &ResolvedFace,
        start_col: usize,
        max_col: usize,
        area: GlyphArea,
    ) -> Option<DisplayRowRenderIntoRowResult> {
        // An in-place fragment extends the semantic row it was given.  Derive
        // the role here instead of accepting an independent value so callers
        // cannot accidentally turn mode/header/tab rows into text rows while
        // installing a decoration such as a terminal right border.
        let role = row.role;
        let request = crate::display_row::DisplayRowSourceFragmentFrame::from_glyph_row_columns(
            row,
            matrix_cols,
            char_width,
            role,
            face_id,
            base_face,
        )
        .render_request_from_column_for_area(start_col, max_col, area);
        self.render_item_source_fragment_into_row(request, row, source, source_state)
    }
}

pub(crate) struct FrameTabBarDisplayRowRequest<'face> {
    pub(crate) row_index: u32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) metrics: DisplayRowFallbackMetrics,
    pub(crate) base_face: &'face ResolvedFace,
    pub(crate) text: Value,
    pub(crate) image_scale_environment: ImageScaleEnvironment,
}

impl<'face> FrameTabBarDisplayRowRequest<'face> {
    fn lisp_string_row_request(&self) -> DisplayRowLispStringSourceRequest<'face> {
        DisplayRowLispStringSourceRequest::new(
            DisplayRowGeometry::new(
                self.y,
                self.width,
                self.metrics.row_height(),
                self.metrics.char_width(),
                self.metrics.ascent(),
                DisplayTabPolicy::every(8),
            ),
            DisplayOrigin::TabBar,
            self.base_face,
            self.text,
            DisplaySourceFaceScope::FrameLocal,
        )
        .with_image_scale_environment(self.image_scale_environment)
    }

    fn lisp_string_source_request(
        &self,
        face_ids: &mut FrameFaceAttempt,
    ) -> DisplayRowLispStringSourceRenderRequest<'face> {
        self.lisp_string_row_request().into_render_request(face_ids)
    }

    fn bounds(&self) -> Rect {
        Rect::new(0.0, self.y, self.width, self.height)
    }

    pub(crate) fn render(
        self,
        state: &mut FrameTabBarDisplayRowRenderState<'_, '_, 'face>,
    ) -> Option<FrameTabBarDisplayRowRender> {
        let rendered = self
            .into_chrome_render_request(state.render_services.face_ids())
            .render_row(&mut state.render_services, state.display_host)?;
        if rendered.text_is_empty() {
            return Some(FrameTabBarDisplayRowRender::Empty);
        }
        let measured = rendered.measure();
        state
            .output
            .install_measured_frame_chrome_display_row(&measured);
        Some(FrameTabBarDisplayRowRender::Measured(measured))
    }

    fn into_chrome_render_request(
        self,
        face_ids: &mut FrameFaceAttempt,
    ) -> ChromeDisplayRowRenderRequest<'face> {
        let render_request = self.lisp_string_source_request(face_ids);
        ChromeDisplayRowRenderRequest {
            plan: ChromeDisplayRowPlan::FrameTabBar {
                right_edge: ChromeRowRightEdge::base_face(
                    render_request.base_face_id(),
                    self.metrics.char_width(),
                ),
            },
            display_row_index: self.row_index,
            bounds: self.bounds(),
            render_request,
        }
    }
}

pub(crate) struct FrameTabBarDisplayRowRenderState<'emit, 'output, 'face> {
    output: FrameChromeOutputTarget<'emit>,
    render_services: ChromeRowRenderServices<'emit, 'face>,
    display_host: Option<&'emit dyn DisplayHost>,
    _output: std::marker::PhantomData<&'output mut ()>,
}

impl<'emit, 'output, 'face> FrameTabBarDisplayRowRenderState<'emit, 'output, 'face> {
    pub(crate) fn new(
        output: FrameChromeOutputTarget<'emit>,
        render_services: ChromeRowRenderServices<'emit, 'face>,
        display_host: Option<&'emit dyn DisplayHost>,
    ) -> Self {
        Self {
            output,
            render_services,
            display_host,
            _output: std::marker::PhantomData,
        }
    }
}

pub(crate) struct WindowChromeDisplayRowRequest<'face> {
    pub(crate) window_id: u64,
    pub(crate) kind: WindowChromeKind,
    pub(crate) selected: bool,
    pub(crate) display_row_index: usize,
    pub(crate) output: ChromeRowOutput,
    pub(crate) bounds: Rect,
    pub(crate) text_area_left_px: f32,
    pub(crate) metrics: DisplayRowFallbackMetrics,
    pub(crate) tab_policy: DisplayTabPolicy,
    pub(crate) base_face: &'face ResolvedFace,
    pub(crate) symbol_values: std::collections::HashMap<String, Value>,
    pub(crate) formatted: ModeLineDisplayOutput,
    pub(crate) face_scope: DisplaySourceFaceScope,
    pub(crate) image_scale_environment: ImageScaleEnvironment,
    pub(crate) tty_glyphless_char_display: crate::neovm_bridge::TtyGlyphlessCharDisplay,
}

pub(crate) struct WindowChromeRowsRenderRequest<'face, 'params> {
    pub(crate) params: &'params WindowParams,
    pub(crate) layout_box: WindowLayoutBox,
    pub(crate) tab_line_face: Option<&'face ResolvedFace>,
    pub(crate) header_line_face: Option<&'face ResolvedFace>,
    pub(crate) mode_line_face: Option<&'face ResolvedFace>,
    pub(crate) tab_line_height: f32,
    pub(crate) header_line_height: f32,
    pub(crate) mode_line_height: f32,
    pub(crate) mode_line_display_row: usize,
    pub(crate) reserve_right_border_col: bool,
    pub(crate) metrics: DisplayRowFallbackMetrics,
    pub(crate) buffer_name: &'params str,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WindowChromeTargetColumns {
    width: f32,
    char_width: f32,
    reserve_right_border_col: bool,
}

impl WindowChromeTargetColumns {
    pub(crate) fn new(width: f32, char_width: f32, reserve_right_border_col: bool) -> Self {
        Self {
            width,
            char_width,
            reserve_right_border_col,
        }
    }

    pub(crate) fn columns(self) -> usize {
        ((self.width / self.char_width.max(1.0)).round().max(1.0) as usize)
            .saturating_sub(usize::from(self.reserve_right_border_col))
            .max(1)
    }
}

pub(crate) struct WindowChromeRowsPlan {
    tab_line_face: Option<ResolvedFace>,
    header_line_face: Option<ResolvedFace>,
    mode_line_face: Option<ResolvedFace>,
    tab_line_height: f32,
    header_line_height: f32,
    mode_line_height: f32,
}

impl WindowChromeRowsPlan {
    pub(crate) fn new<B: LayoutBufferView>(
        params: &WindowParams,
        buffer: &B,
        face_resolver: &FaceResolver,
    ) -> Self {
        let mut next_check = buffer.layout_point_max_char_pos().get();
        let mode_line_face = (params.mode_line_height > 0.0).then(|| {
            face_resolver.default_base_face_for_origin(
                Some(buffer),
                &DisplayOrigin::ModeLine {
                    selected: params.mode_line_active,
                },
                &mut next_check,
            )
        });
        let header_line_face = (params.header_line_height > 0.0).then(|| {
            face_resolver.default_base_face_for_origin(
                Some(buffer),
                &DisplayOrigin::HeaderLine {
                    selected: params.mode_line_active,
                },
                &mut next_check,
            )
        });
        let tab_line_face = (params.tab_line_height > 0.0).then(|| {
            face_resolver.default_base_face_for_origin(
                Some(buffer),
                &DisplayOrigin::TabLine,
                &mut next_check,
            )
        });

        // `WindowParams` carries the frame attempt's canonical assumed
        // metrics: retained measured heights when known, otherwise the
        // bridge's face estimate.  Do not independently re-estimate here;
        // body allocation and chrome shaping must begin from one authority.
        let mode_line_height = mode_line_face
            .as_ref()
            .map_or(0.0, |_| params.mode_line_height.max(0.0));
        let header_line_height = header_line_face
            .as_ref()
            .map_or(0.0, |_| params.header_line_height.max(0.0));
        let tab_line_height = tab_line_face
            .as_ref()
            .map_or(0.0, |_| params.tab_line_height.max(0.0));

        Self {
            tab_line_face,
            header_line_face,
            mode_line_face,
            tab_line_height,
            header_line_height,
            mode_line_height,
        }
    }

    pub(crate) fn render_request<'face, 'params>(
        &'face self,
        params: &'params WindowParams,
        layout_box: WindowLayoutBox,
        mode_line_display_row: usize,
        reserve_right_border_col: bool,
        metrics: DisplayRowFallbackMetrics,
        buffer_name: &'params str,
    ) -> WindowChromeRowsRenderRequest<'face, 'params> {
        WindowChromeRowsRenderRequest {
            params,
            layout_box,
            tab_line_face: self.tab_line_face.as_ref(),
            header_line_face: self.header_line_face.as_ref(),
            mode_line_face: self.mode_line_face.as_ref(),
            tab_line_height: self.tab_line_height,
            header_line_height: self.header_line_height,
            mode_line_height: self.mode_line_height,
            mode_line_display_row,
            reserve_right_border_col,
            metrics,
            buffer_name,
        }
    }
}

impl<'face, 'params> WindowChromeRowsRenderRequest<'face, 'params> {
    fn target_columns(&self) -> WindowChromeTargetColumns {
        let regions = self.layout_box.regions();
        let chrome_width = regions
            .mode_line
            .or(regions.header_line)
            .or(regions.tab_line)
            .map_or(regions.outer.width, |bounds| bounds.width);
        WindowChromeTargetColumns::new(
            chrome_width,
            self.metrics.char_width(),
            self.reserve_right_border_col,
        )
    }

    fn target_cols(&self) -> usize {
        self.target_columns().columns()
    }

    /// Evaluate each chrome row's `*-format` ONCE (after the body, so `%p`/`%l`
    /// reflect the final window-start) and install the built rows. Returns the
    /// *measured* row heights — GNU `display_mode_line` returns the laid-out
    /// row's `glyph_row->height`, which becomes `w->mode_line_height`. The
    /// caller reports these as `window-mode-line-height` etc. and uses them to
    /// place the rows below. A tall `display` element (doom-modeline's bar)
    /// therefore grows the reported height past the face-only estimate, instead
    /// of being clamped to it. The text-area geometry was already reserved from
    /// the face estimate (GNU `estimate_mode_line_height`); the mode line is
    /// pinned to the window bottom, so the measured height is taken from there.
    pub(crate) fn render(
        self,
        state: &mut WindowChromeRowsRenderState<'_, '_, 'face>,
    ) -> WindowChromeRowsRenderOutcome {
        let params = self.params;
        let source = WindowChromeSourceIdentity::new(params.window_id, params.buffer_id);
        let regions = self.layout_box.regions();
        let chrome_width = regions
            .mode_line
            .or(regions.header_line)
            .or(regions.tab_line)
            .map_or(regions.outer.width, |bounds| bounds.width);
        let mut status_line_symbol_values = std::collections::HashMap::new();
        let tty_glyphless_char_display = state
            .evaluator
            .buffer_manager()
            .get(BufferId(params.buffer_id))
            .map(crate::neovm_bridge::TtyGlyphlessCharDisplay::capture)
            .unwrap_or_default();
        if let Some(value) = state
            .evaluator
            .buffer_manager()
            .get(BufferId(params.buffer_id))
            .and_then(|buffer| buffer.buffer_local_value("header-line-indent-width"))
        {
            status_line_symbol_values.insert("header-line-indent-width".to_string(), value);
        }
        let chrome_tab_policy = DisplayTabPolicy::from_tab_width_and_stops(
            0.0,
            params.tab_width,
            &params.tab_stop_list,
        );
        let text_area_left_px = (params.text_bounds.x - params.bounds.x).max(0.0);
        let target_cols = self.target_cols();
        let mut measured = WindowChromeMetrics {
            tab_line_height: self.tab_line_height,
            header_line_height: self.header_line_height,
            mode_line_height: self.mode_line_height,
        };

        if params.tab_line_height > 0.0 {
            let tab_line_y = regions.tab_line.map_or(regions.outer.y, |bounds| bounds.y);
            let tab_line_text = eval_status_line_format_output(
                state.evaluator,
                "tab-line-format",
                params.window_id,
                params.buffer_id,
                target_cols,
            )
            .unwrap_or_else(|| ModeLineDisplayOutput::from_root_string(Value::string("")));
            let Some(face_scope) = state.face_scope_for_source(source) else {
                return WindowChromeRowsRenderOutcome::SourceInvalidated;
            };
            if let Some(height) = state.render_display_row(
                WindowChromeDisplayRowRequest {
                    window_id: params.window_id as u64,
                    kind: WindowChromeKind::TabLine,
                    selected: params.selected,
                    display_row_index: 0,
                    output: ChromeRowOutput::new(0, tab_line_y),
                    bounds: Rect::new(
                        regions.outer.x,
                        tab_line_y,
                        chrome_width,
                        self.tab_line_height,
                    ),
                    text_area_left_px,
                    metrics: self.metrics,
                    tab_policy: chrome_tab_policy.clone(),
                    base_face: self
                        .tab_line_face
                        .expect("tab-line face should exist when tab-line height is positive"),
                    symbol_values: status_line_symbol_values.clone(),
                    formatted: tab_line_text,
                    face_scope,
                    image_scale_environment: params.image_scale_environment,
                    tty_glyphless_char_display,
                },
                ChromeRowVerticalAnchor::Top,
            ) {
                measured.tab_line_height = height;
            }
        }

        if params.header_line_height > 0.0 {
            // The header line sits directly below the (measured) tab line.
            let header_line_y = regions.outer.y + measured.tab_line_height;
            let header_line_text = eval_status_line_format_output(
                state.evaluator,
                "header-line-format",
                params.window_id,
                params.buffer_id,
                target_cols,
            )
            .unwrap_or_else(|| ModeLineDisplayOutput::from_root_string(Value::string("")));
            let Some(face_scope) = state.face_scope_for_source(source) else {
                return WindowChromeRowsRenderOutcome::SourceInvalidated;
            };
            if let Some(height) = state.render_display_row(
                WindowChromeDisplayRowRequest {
                    window_id: params.window_id as u64,
                    kind: WindowChromeKind::HeaderLine,
                    selected: params.mode_line_active,
                    display_row_index: usize::from(self.tab_line_height > 0.0),
                    output: ChromeRowOutput::new(
                        i64::from(self.tab_line_height > 0.0),
                        header_line_y,
                    ),
                    bounds: Rect::new(
                        regions.outer.x,
                        header_line_y,
                        chrome_width,
                        self.header_line_height,
                    ),
                    text_area_left_px,
                    metrics: self.metrics,
                    tab_policy: chrome_tab_policy.clone(),
                    base_face: self.header_line_face.expect(
                        "header-line face should exist when header-line height is positive",
                    ),
                    symbol_values: status_line_symbol_values.clone(),
                    formatted: header_line_text,
                    face_scope,
                    image_scale_environment: params.image_scale_environment,
                    tty_glyphless_char_display,
                },
                ChromeRowVerticalAnchor::Top,
            ) {
                measured.header_line_height = height;
            }
        }

        if params.mode_line_height > 0.0 {
            let window_bottom = regions
                .bottom_divider
                .map_or(regions.outer.y + regions.outer.height, |bounds| bounds.y);
            // Provisional Y from the face estimate; `render_and_apply` re-pins
            // the row to `window_bottom − measured_height` once it knows the
            // real height.
            let mode_line_y = window_bottom - self.mode_line_height;
            let mode_line_text = {
                let result = eval_status_line_format_output(
                    state.evaluator,
                    "mode-line-format",
                    params.window_id,
                    params.buffer_id,
                    target_cols,
                )
                .unwrap_or_else(|| {
                    ModeLineDisplayOutput::from_root_string(Value::string(format!(
                        " {} ",
                        self.buffer_name
                    )))
                });
                tracing::debug!(
                    "mode-line eval result: {:?} (len={})",
                    result
                        .value()
                        .as_utf8_str()
                        .map(|s| &s[..s.len().min(120)])
                        .unwrap_or(""),
                    result.value().as_utf8_str().map(str::len).unwrap_or(0)
                );
                result
            };
            let Some(face_scope) = state.face_scope_for_source(source) else {
                return WindowChromeRowsRenderOutcome::SourceInvalidated;
            };
            if let Some(height) = state.render_display_row(
                WindowChromeDisplayRowRequest {
                    window_id: params.window_id as u64,
                    kind: WindowChromeKind::ModeLine,
                    selected: params.mode_line_active,
                    display_row_index: self.mode_line_display_row,
                    output: ChromeRowOutput::new(self.mode_line_display_row as i64, mode_line_y),
                    bounds: Rect::new(
                        regions.outer.x,
                        mode_line_y,
                        chrome_width,
                        self.mode_line_height,
                    ),
                    text_area_left_px,
                    metrics: self.metrics,
                    tab_policy: chrome_tab_policy,
                    base_face: self
                        .mode_line_face
                        .expect("mode-line face should exist when mode-line height is positive"),
                    symbol_values: status_line_symbol_values,
                    formatted: mode_line_text,
                    face_scope,
                    image_scale_environment: params.image_scale_environment,
                    tty_glyphless_char_display,
                },
                ChromeRowVerticalAnchor::Bottom(window_bottom),
            ) {
                measured.mode_line_height = height;
            }
        }

        WindowChromeRowsRenderOutcome::Rendered(measured)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WindowChromeSourceIdentity {
    window_id: WindowId,
    buffer_id: BufferId,
}

impl WindowChromeSourceIdentity {
    fn new(window_id: i64, buffer_id: u64) -> Self {
        Self {
            window_id: WindowId(window_id as u64),
            buffer_id: BufferId(buffer_id),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum WindowChromeRowsRenderOutcome {
    Rendered(WindowChromeMetrics),
    /// A chrome `:eval` deleted the window, killed its buffer, or retargeted
    /// the window. The containing frame transaction must discard every row
    /// produced from the stale source projection and recollect live inputs.
    SourceInvalidated,
}

struct ChromeDisplayRowRenderRequest<'face> {
    plan: ChromeDisplayRowPlan,
    display_row_index: u32,
    bounds: Rect,
    render_request: DisplayRowLispStringSourceRenderRequest<'face>,
}

struct ChromeDisplayRowRenderedRequest {
    plan: ChromeDisplayRowPlan,
    display_row_index: u32,
    bounds: Rect,
    rendered: RenderedDisplayRow,
}

/// Couples a chrome row's semantic owner to the measurement and right-edge
/// completion rules that belong to that owner.
///
/// GNU completes both window chrome (`display_mode_line`) and frame tab bars
/// (`display_tab_bar` / `display_tab_bar_line`) with an owner-selected face.
/// Keeping those facts in one exhaustive plan prevents a new chrome owner
/// from silently selecting an intrinsic-width row or the wrong owner's face.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ChromeDisplayRowPlan {
    WindowChrome {
        window_id: u64,
        kind: WindowChromeKind,
        right_edge: ChromeRowRightEdge,
    },
    FrameTabBar {
        right_edge: ChromeRowRightEdge,
    },
}

impl ChromeDisplayRowPlan {
    fn owner(self) -> DisplayRowOwner {
        match self {
            Self::WindowChrome {
                window_id, kind, ..
            } => DisplayRowOwner::WindowChrome { window_id, kind },
            Self::FrameTabBar { .. } => DisplayRowOwner::FrameChrome {
                kind: FrameChromeKind::TabBar,
            },
        }
    }

    fn bounds_policy(self) -> DisplayRowBoundsPolicy {
        match self {
            Self::WindowChrome { .. } => DisplayRowBoundsPolicy::MeasureIntrinsic,
            Self::FrameTabBar { .. } => DisplayRowBoundsPolicy::MeasureContent,
        }
    }

    fn complete(self, measured: &mut MeasuredDisplayRow) {
        let right_edge = match self {
            Self::WindowChrome { right_edge, .. } | Self::FrameTabBar { right_edge } => right_edge,
        };
        measured.fill_trailing_background(right_edge.face_id, right_edge.char_width);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ChromeRowRightEdge {
    face_id: FaceId,
    char_width: f32,
}

impl ChromeRowRightEdge {
    fn base_face(face_id: FaceId, char_width: f32) -> Self {
        Self {
            face_id,
            char_width,
        }
    }
}

impl<'face> ChromeDisplayRowRenderRequest<'face> {
    fn render_row(
        self,
        render_services: &mut ChromeRowRenderServices<'_, 'face>,
        display_host: Option<&dyn DisplayHost>,
    ) -> Option<ChromeDisplayRowRenderedRequest> {
        let rendered =
            render_services.render_lisp_string_source_request(self.render_request, display_host)?;
        Some(ChromeDisplayRowRenderedRequest {
            plan: self.plan,
            display_row_index: self.display_row_index,
            bounds: self.bounds,
            rendered,
        })
    }
}

impl ChromeDisplayRowRenderedRequest {
    fn text_is_empty(&self) -> bool {
        display_row_text_is_empty(self.rendered.row())
    }

    fn measure(self) -> MeasuredDisplayRow {
        let mut measured = MeasuredDisplayRow::new(
            self.plan.owner(),
            self.display_row_index,
            self.bounds,
            self.rendered,
            self.plan.bounds_policy(),
        );
        self.plan.complete(&mut measured);
        measured
    }
}

struct WindowChromeDisplayRowRenderRequest<'face> {
    output: ChromeRowOutput,
    row: ChromeDisplayRowRenderRequest<'face>,
    chrome_strings: WindowChromeStringSources,
}

#[derive(Clone, Copy, Debug)]
struct WindowChromeSourceSpan {
    output_start: usize,
    output_end: usize,
    source_start: usize,
    string_id: GlyphStringId,
}

impl WindowChromeSourceSpan {
    fn from_mode_line_span(span: ModeLineDisplaySourceSpan, string_id: GlyphStringId) -> Self {
        Self {
            output_start: span.output_start(),
            output_end: span.output_end(),
            source_start: span.source_start(),
            string_id,
        }
    }

    fn source_position(self, output_position: usize) -> Option<(GlyphStringId, usize)> {
        if output_position < self.output_start || output_position >= self.output_end {
            return None;
        }
        Some((
            self.string_id,
            self.source_start
                .saturating_add(output_position - self.output_start),
        ))
    }
}

/// Joins formatter-owned Lisp sources to protocol-safe string ids for one
/// chrome row. The root formatted string remains available for synthetic `%`
/// expansions; direct leaf-string glyphs are remapped to their original
/// objects, matching GNU's `glyph->object` contract.
struct WindowChromeStringSources {
    area: PresentedWindowChromeArea,
    presented: Vec<PresentedWindowChromeString>,
    spans: Vec<WindowChromeSourceSpan>,
}

impl WindowChromeStringSources {
    fn new(area: PresentedWindowChromeArea, formatted: &ModeLineDisplayOutput) -> Self {
        let root_id = crate::display_row::root_lisp_string_id();
        let root_value = formatted.value();
        let mut presented = vec![PresentedWindowChromeString::new(area, root_id, root_value)];
        let mut source_ids: Vec<(Value, GlyphStringId)> = Vec::new();
        let mut spans = Vec::with_capacity(formatted.source_spans().len());

        for span in formatted.source_spans().iter().copied() {
            let source = span.source();
            let string_id = if source == root_value {
                root_id
            } else if let Some((_, id)) = source_ids.iter().find(|(value, _)| *value == source) {
                *id
            } else {
                let id = GlyphStringId::new(source_ids.len().saturating_add(2) as u64);
                source_ids.push((source, id));
                presented.push(PresentedWindowChromeString::new(area, id, source));
                id
            };
            spans.push(WindowChromeSourceSpan::from_mode_line_span(span, string_id));
        }

        Self {
            area,
            presented,
            spans,
        }
    }

    fn source_position(&self, output_position: usize) -> Option<(GlyphStringId, usize)> {
        self.spans
            .iter()
            .find_map(|span| span.source_position(output_position))
    }
}

/// How a chrome row is anchored vertically once its real (measured) height is
/// known. Top-anchored rows (tab line, header line) keep the Y they were laid
/// out at; the bottom-anchored mode line is pinned to the window bottom, so its
/// top moves up when its measured height exceeds the face-only estimate. This
/// is the only place the laid-out row's Y is adjusted for the measured height —
/// GNU pins `display_mode_line`'s row to the bottom of the window the same way.
#[derive(Clone, Copy)]
enum ChromeRowVerticalAnchor {
    Top,
    /// The window-bottom Y; the row's top is `bottom − measured_height`.
    Bottom(f32),
}

impl<'face> WindowChromeDisplayRowRenderRequest<'face> {
    #[cfg(test)]
    fn render_measured(
        self,
        render_services: &mut ChromeRowRenderServices<'_, 'face>,
        display_host: Option<&dyn DisplayHost>,
    ) -> Option<WindowChromeDisplayRowRender> {
        let measured = self
            .row
            .render_row(render_services, display_host)?
            .measure();
        Some(WindowChromeDisplayRowRender {
            output: self.output,
            measured,
        })
    }

    /// Evaluate-and-build the row ONCE, measure its real height, re-anchor it
    /// (mode line → window bottom), then install + emit. Returns the measured
    /// row height so the caller can report it as `window-mode-line-height` and
    /// position any rows below it. The single `format-mode-line` eval already
    /// happened in `eval_status_line_format_output`; nothing is re-evaluated.
    fn render_and_apply(
        self,
        state: &mut WindowChromeRowsRenderState<'_, '_, 'face>,
        anchor: ChromeRowVerticalAnchor,
    ) -> Option<f32> {
        let output = self.output;
        let chrome_strings = self.chrome_strings;
        let mut rendered = self.row.render_row(
            &mut state.render_services,
            state.evaluator.display_host.as_deref(),
        )?;
        rendered.rendered.remap_root_string_provenance(
            crate::display_row::root_lisp_string_id(),
            |output_position| chrome_strings.source_position(output_position),
        );
        state
            .output_emitter
            .replace_chrome_area_strings(chrome_strings.area, chrome_strings.presented);
        // Measure before owner-level width completion.  The fill is paint, not
        // content: letting its synthetic ascent/descent participate here can
        // combine maxima from different glyphs and spuriously grow a 16px row
        // to 17px, forcing a second format-mode-line evaluation.
        let mut measured = rendered.measure();
        let measured_height = measured.row_height();
        let final_y = match anchor {
            ChromeRowVerticalAnchor::Top => measured.bounds().y,
            ChromeRowVerticalAnchor::Bottom(window_bottom) => window_bottom - measured_height,
        };
        measured.reanchor_y(final_y);
        let progress = measured.output_progress();
        state.output.install_measured_window_display_row(&measured);
        state.output_emitter.emit_chrome_progress(
            state.evaluator,
            ChromeRowProgress::new(output.with_y(final_y), progress),
        );
        Some(measured.row_height())
    }
}

#[cfg(test)]
struct WindowChromeDisplayRowRender {
    output: ChromeRowOutput,
    measured: MeasuredDisplayRow,
}

impl<'face> WindowChromeDisplayRowRequest<'face> {
    fn lisp_string_row_request(&self) -> DisplayRowLispStringSourceRequest<'face> {
        DisplayRowLispStringSourceRequest::new(
            DisplayRowGeometry::new(
                self.bounds.y,
                self.bounds.width,
                // Intrinsic display properties such as `(height 2.0)` are
                // relative to the row's face/font metrics, not to the retained
                // allocation being validated.  Feeding `bounds.height` back into
                // shaping makes every retry multiply the previous measurement
                // (3 -> 6 -> 12 -> ...), so intrinsic measurement could never
                // converge.
                self.metrics.row_height(),
                self.metrics.char_width(),
                self.metrics.ascent(),
                self.tab_policy.clone(),
            ),
            window_chrome_display_origin(self.kind, self.selected),
            self.base_face,
            self.formatted.value(),
            self.face_scope,
        )
        .with_image_scale_environment(self.image_scale_environment)
        .with_tty_glyphless_char_display(self.tty_glyphless_char_display)
    }

    fn into_render_request(
        self,
        face_ids: &mut FrameFaceAttempt,
    ) -> WindowChromeDisplayRowRenderRequest<'face> {
        let chrome_strings = WindowChromeStringSources::new(
            presented_window_chrome_area(self.kind),
            &self.formatted,
        );
        let render_request = self
            .lisp_string_row_request()
            .with_symbol_values(self.symbol_values)
            .into_render_request(face_ids)
            .with_chrome_text_area_left_px(self.text_area_left_px);
        let row = ChromeDisplayRowRenderRequest {
            plan: ChromeDisplayRowPlan::WindowChrome {
                window_id: self.window_id,
                kind: self.kind,
                right_edge: ChromeRowRightEdge::base_face(
                    render_request.base_face_id(),
                    self.metrics.char_width(),
                ),
            },
            display_row_index: self.display_row_index.min(u32::MAX as usize) as u32,
            bounds: self.bounds,
            render_request,
        };
        WindowChromeDisplayRowRenderRequest {
            output: self.output,
            row,
            chrome_strings,
        }
    }
}

pub(crate) struct WindowChromeRowsRenderState<'state, 'services, 'face> {
    output: TextWindowOutputTarget<'state>,
    output_emitter: &'state mut WindowOutputEmitter,
    evaluator: &'state mut Context,
    render_services: ChromeRowRenderServices<'services, 'face>,
}

impl<'state, 'services, 'face> WindowChromeRowsRenderState<'state, 'services, 'face> {
    pub(crate) fn new(
        output: TextWindowOutputTarget<'state>,
        output_emitter: &'state mut WindowOutputEmitter,
        evaluator: &'state mut Context,
        render_services: ChromeRowRenderServices<'services, 'face>,
    ) -> Self {
        Self {
            output,
            output_emitter,
            evaluator,
            render_services,
        }
    }

    fn face_scope_for_source(
        &self,
        source: WindowChromeSourceIdentity,
    ) -> Option<DisplaySourceFaceScope> {
        let live_buffer_id = self
            .evaluator
            .frame_manager()
            .lookup_window(source.window_id)
            .and_then(neovm_core::window::Window::buffer_id)?;
        if live_buffer_id != source.buffer_id {
            return None;
        }
        self.evaluator
            .buffer_manager()
            .get(source.buffer_id)
            .map(DisplaySourceFaceScope::for_buffer)
    }

    fn render_display_row(
        &mut self,
        mut request: WindowChromeDisplayRowRequest<'face>,
        anchor: ChromeRowVerticalAnchor,
    ) -> Option<f32> {
        // `:eval` chrome elements can return fresh strings held only by the
        // formatter sidecar. Keep both the flattened text and every original
        // source alive across display-property evaluation during row render.
        let saved_roots = neovm_core::emacs_core::eval::save_scratch_gc_roots();
        neovm_core::emacs_core::eval::push_scratch_gc_root(request.formatted.value());
        for span in request.formatted.source_spans() {
            neovm_core::emacs_core::eval::push_scratch_gc_root(span.source());
        }
        request.metrics = self
            .render_services
            .intrinsic_metrics_for_face(request.base_face, request.metrics);
        let rendered = request
            .into_render_request(self.render_services.face_ids())
            .render_and_apply(self, anchor);
        neovm_core::emacs_core::eval::restore_scratch_gc_roots(saved_roots);
        rendered
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
pub(crate) enum ResizeMiniWindowsMode {
    #[strum(to_string = "nil")]
    Disabled,
    #[strum(to_string = "grow-only")]
    GrowOnly,
    #[strum(to_string = "t")]
    Exact,
}

impl ResizeMiniWindowsMode {
    pub(crate) fn from_lisp_value(value: Option<&Value>) -> Self {
        let Some(value) = value else {
            return Self::Exact;
        };
        if value.is_nil() {
            return Self::Disabled;
        }
        value
            .as_symbol_name()
            .and_then(|name| name.parse().ok())
            .unwrap_or(Self::Exact)
    }

    pub(crate) fn should_grow(self) -> bool {
        !matches!(self, Self::Disabled)
    }

    /// Mirror GNU `resize_mini_window` (src/xdisp.c:13395-13406).
    ///
    /// With `resize-mini-windows` = `grow-only`, the mini-window shrinks back
    /// only when `height < old_height && (exact_p || BEGV == ZV)`
    /// (xdisp.c:13401): i.e. when its buffer is empty, OR when an exact resize
    /// was requested. The exact case is GNU's `resize_echo_area_exactly`
    /// (xdisp.c:13228-13245), which passes `exact_p = (minibuf_level == 0)` and
    /// is run after every command from `command_loop_1` (keyboard.c:1344) — so
    /// a finished command with no active minibuffer shrinks even a NON-EMPTY
    /// shorter message to fit. With `resize-mini-windows` = `t`, always shrink.
    pub(crate) fn should_shrink(self, exact: bool, visible_region_empty: bool) -> bool {
        match self {
            Self::Disabled => false,
            Self::GrowOnly => exact || visible_region_empty,
            Self::Exact => true,
        }
    }
}

#[cfg(test)]
pub(crate) fn eval_status_line_format(
    evaluator: &mut Context,
    format_symbol: &str,
    window_id: i64,
    buffer_id: u64,
    target_cols: usize,
) -> Option<String> {
    eval_status_line_format_value(evaluator, format_symbol, window_id, buffer_id, target_cols)
        .and_then(|val| val.as_runtime_string_owned())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
pub(crate) fn eval_status_line_format_value(
    evaluator: &mut Context,
    format_symbol: &str,
    window_id: i64,
    buffer_id: u64,
    target_cols: usize,
) -> Option<Value> {
    eval_status_line_format_output(evaluator, format_symbol, window_id, buffer_id, target_cols)
        .map(ModeLineDisplayOutput::into_value)
}

fn eval_status_line_format_output(
    evaluator: &mut Context,
    format_symbol: &str,
    window_id: i64,
    buffer_id: u64,
    target_cols: usize,
) -> Option<ModeLineDisplayOutput> {
    evaluator.setup_thread_locals();
    // GNU Emacs (xdisp.c:28187): format-mode-line reads the format
    // variable from the TARGET buffer, not the caller's current
    // buffer. We must read the buffer-local value of mode-line-format
    // from the specified buffer BEFORE calling the walker.
    let window_format_value = evaluator
        .frame_manager()
        .window_parameter(WindowId(window_id as u64), &Value::symbol(format_symbol));
    let format_value = window_format_value
        .filter(|value| !value.is_nil())
        .unwrap_or_else(|| {
            evaluator
                .buffer_manager()
                .get(BufferId(buffer_id))
                .and_then(|buf| buf.buffer_local_value(format_symbol))
                .unwrap_or_else(|| {
                    // Fall back to the global default
                    evaluator
                        .obarray()
                        .symbol_value(format_symbol)
                        .copied()
                        .unwrap_or(Value::NIL)
                })
        });
    // GNU `display_mode_line` (xdisp.c:27911) runs the mode-line
    // walker in `MODE_LINE_DISPLAY` mode, which makes `%-` expand to
    // dashes filling the remaining row width. Our layout engine is the
    // equivalent redisplay path, so we call
    // `format_mode_line_for_display` directly rather than going
    // through the Lisp-facing `format-mode-line` builtin (which uses
    // `MODE_LINE_STRING` and returns `"--"` for `%-`).
    //
    // `target_cols` is the window's width in character cells, which
    // the DISPLAY walker uses to size the dash fill for `%-`.
    //
    // Count this eval for the single-eval invariant: each redisplay runs the
    // mode-line format exactly once (in the chrome render, after the body, so
    // `%p`/`%l` reflect the final window-start). The reserved/reported height
    // is the *measured* row height, not a second eval.
    record_mode_line_eval(format_symbol);
    // Arm the `%c`/`%C` detector for exactly this expansion. P5.2(b)'s chrome
    // skip refuses whenever a column was displayed, because a column is the one
    // point-dependent construct its same-screen-row precondition does not pin.
    neovm_core::emacs_core::xdisp::reset_column_spec_consumed();
    let rendered = neovm_core::emacs_core::xdisp::format_mode_line_for_display_with_sources(
        evaluator,
        format_value,
        Value::make_window(window_id as u64),
        Value::make_buffer(BufferId(buffer_id)),
        target_cols,
    );
    record_chrome_generated(
        window_id,
        neovm_core::emacs_core::xdisp::column_spec_consumed(),
    );
    if rendered
        .value()
        .as_runtime_string_owned()
        .is_some_and(|s| !s.is_empty())
    {
        Some(rendered)
    } else {
        None
    }
}

fn tab_bar_menu_item(entry: Value) -> Option<(Value, Value, Value, Value)> {
    if let Some(items) = list_to_vec(&entry)
        && items
            .get(1)
            .is_some_and(|value| KeymapMarker::MenuItem.is_value(*value))
    {
        let caption = *items.get(2)?;
        let binding = *items.get(3)?;
        return caption.is_string().then(|| {
            (
                *items.first().expect("checked item key"),
                caption,
                binding,
                Value::list(items[4..].to_vec()),
            )
        });
    }

    if !entry.is_cons() {
        return None;
    }
    let pair_cdr = entry.cons_cdr();
    let items = list_to_vec(&pair_cdr)?;
    if !items
        .first()
        .is_some_and(|value| KeymapMarker::MenuItem.is_value(*value))
    {
        return None;
    }
    let caption = *items.get(1)?;
    let binding = *items.get(2)?;
    caption.is_string().then(|| {
        (
            entry.cons_car(),
            caption,
            binding,
            Value::list(items[3..].to_vec()),
        )
    })
}

fn tab_bar_item_enabled(evaluator: &mut Context, plist: Value) -> bool {
    let Some(items) = list_to_vec(&plist) else {
        return true;
    };
    items
        .chunks_exact(2)
        .find(|pair| MenuItemProperty::Enable.is_value(pair[0]))
        .map(|pair| {
            evaluator
                .eval_form(pair[1])
                .is_ok_and(|value| !value.is_nil())
        })
        .unwrap_or(true)
}

#[derive(Clone)]
pub(crate) struct TabBarSourceItem {
    pub(crate) caption: Value,
    pub(crate) key: Value,
    pub(crate) binding: Value,
    pub(crate) char_range: Range<usize>,
    pub(crate) enabled: bool,
}

#[derive(Clone)]
pub(crate) struct BuiltTabBar {
    pub(crate) text: Value,
    pub(crate) source_items: Vec<TabBarSourceItem>,
}

struct TabBarDisplayBuildRequest {
    frame_id: u64,
}

impl TabBarDisplayBuildRequest {
    fn new(frame_id: u64) -> Self {
        Self { frame_id }
    }

    fn build(self, evaluator: &mut Context, gc_roots: &ScratchGcRootScope) -> Option<BuiltTabBar> {
        evaluator.setup_thread_locals();
        if !evaluator.obarray().fboundp("tab-bar-make-keymap-1") {
            return None;
        }

        let result = evaluator
            .with_frame_display_context(FrameId(self.frame_id), |evaluator| {
                Self::make_keymap(evaluator)
                    .inspect(|keymap| gc_roots.root(*keymap))
                    .and_then(|keymap| TabBarDisplaySource::from_keymap(evaluator, keymap))
                    .and_then(|source| source.into_built_tab_bar(evaluator))
            })
            .flatten();
        if let Some(tab_bar) = &result {
            gc_roots.root(tab_bar.text);
            for item in &tab_bar.source_items {
                gc_roots.root(item.caption);
                gc_roots.root(item.key);
                gc_roots.root(item.binding);
            }
        }
        result
    }

    fn make_keymap(evaluator: &mut Context) -> Option<Value> {
        evaluator
            .eval_form(Value::list(vec![Value::symbol("tab-bar-make-keymap-1")]))
            .ok()
    }
}

struct TabBarDisplaySource {
    entries: Vec<TabBarDisplayEntry>,
}

struct TabBarDisplayEntry {
    caption: Value,
    key: Value,
    binding: Value,
    enabled: bool,
}

impl TabBarDisplaySource {
    fn from_keymap(evaluator: &mut Context, keymap: Value) -> Option<Self> {
        let keymap_entries = list_to_vec(&keymap)?;
        let mut display_entries = Vec::new();
        for (index, entry) in keymap_entries.iter().enumerate() {
            if index == 0 && KeymapMarker::Keymap.is_value(*entry) {
                continue;
            }

            if is_list_keymap(entry) {
                break;
            }

            if let Some((key, caption, binding, plist)) = tab_bar_menu_item(*entry) {
                display_entries.push(TabBarDisplayEntry {
                    caption,
                    key,
                    binding,
                    enabled: tab_bar_item_enabled(evaluator, plist),
                });
            }
        }
        (!display_entries.is_empty()).then_some(Self {
            entries: display_entries,
        })
    }

    fn into_built_tab_bar(self, evaluator: &mut Context) -> Option<BuiltTabBar> {
        let mut char_start = 0usize;
        let item_char_ranges: Vec<_> = self
            .entries
            .iter()
            .map(|entry| {
                let char_len = entry
                    .caption
                    .as_runtime_string_owned()
                    .map_or(0, |caption| caption.chars().count());
                let range = char_start..char_start + char_len;
                char_start += char_len;
                range
            })
            .collect();
        let source_items = self
            .entries
            .iter()
            .zip(item_char_ranges.iter().cloned())
            .map(|(entry, char_range)| TabBarSourceItem {
                caption: entry.caption,
                key: entry.key,
                binding: entry.binding,
                char_range,
                enabled: entry.enabled,
            })
            .collect();
        let mut concat_form = Vec::with_capacity(self.entries.len() + 1);
        concat_form.push(Value::symbol("concat"));
        concat_form.extend(self.entries.iter().map(|entry| entry.caption));
        let text = evaluator.eval_form(Value::list(concat_form)).ok()?;
        text.as_runtime_string_owned()
            .is_some_and(|text| !text.is_empty())
            .then_some(BuiltTabBar { text, source_items })
    }
}

fn quoted(value: Value) -> Value {
    Value::list(vec![Value::symbol("quote"), value])
}

fn tab_bar_close_at(evaluator: &mut Context, text: Value, char_index: usize) -> bool {
    evaluator
        .eval_form(Value::list(vec![
            Value::symbol("get-text-property"),
            Value::fixnum(char_index as i64),
            quoted(Value::symbol("close-tab")),
            quoted(text),
        ]))
        .is_ok_and(|value| !value.is_nil())
}

fn tab_bar_mouse_face_at(evaluator: &mut Context, text: Value, char_index: usize) -> Value {
    evaluator
        .eval_form(Value::list(vec![
            Value::symbol("get-text-property"),
            Value::fixnum(char_index as i64),
            quoted(Value::symbol("mouse-face")),
            quoted(text),
        ]))
        .unwrap_or(Value::NIL)
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TabBarPointerSlotPlan {
    col: u16,
    source_char_index: usize,
    x: f32,
    width: f32,
    item_index: usize,
    enabled: bool,
    close: bool,
    mouse_face: Value,
}

pub(crate) fn tab_bar_pointer_slot_plan(
    evaluator: &mut Context,
    rendered: &RenderedDisplayRow,
    text: Value,
    items: &[TabBarSourceItem],
) -> Vec<TabBarPointerSlotPlan> {
    let mut slots = Vec::new();
    for slot in rendered.source_slots() {
        let Some(char_index) = slot.source().lisp_string_char_index() else {
            continue;
        };
        let item_index = items.partition_point(|item| item.char_range.end <= char_index);
        let Some(item) = items
            .get(item_index)
            .filter(|item| item.char_range.contains(&char_index))
        else {
            continue;
        };
        slots.push(TabBarPointerSlotPlan {
            col: u16::try_from(slot.col()).unwrap_or(u16::MAX),
            source_char_index: char_index,
            x: slot.x_px(),
            width: slot.width_px(),
            item_index,
            enabled: item.enabled,
            close: tab_bar_close_at(evaluator, text, char_index),
            mouse_face: tab_bar_mouse_face_at(evaluator, text, char_index),
        });
    }
    slots
}

pub(crate) fn tab_bar_effective_mouse_faces(slots: &[TabBarPointerSlotPlan]) -> Vec<Value> {
    let mut faces = Vec::new();
    for slot in slots {
        let face = slot.mouse_face;
        if !face.is_nil() && !faces.contains(&face) {
            faces.push(face);
        }
    }
    faces
}

fn tab_bar_posn_string(
    evaluator: &mut Context,
    item: &TabBarSourceItem,
    close: bool,
) -> Option<Value> {
    let menu_item = Value::list(vec![
        item.key,
        item.binding,
        if close { Value::T } else { Value::NIL },
    ]);
    let caption = evaluator
        .eval_form(Value::list(vec![
            Value::symbol("propertize"),
            Value::list(vec![Value::symbol("copy-sequence"), quoted(item.caption)]),
            quoted(Value::symbol("menu-item")),
            quoted(menu_item),
        ]))
        .ok()?;
    Some(Value::cons(caption, Value::fixnum(0)))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TabBarPointerAppearanceStyle {
    raised: PointerImageRelief,
    sunken: PointerImageRelief,
}

impl TabBarPointerAppearanceStyle {
    pub(crate) const fn new(raised: PointerImageRelief, sunken: PointerImageRelief) -> Self {
        Self { raised, sunken }
    }
}

fn gnu_relief_color(background: u32, factor: f64, delta: u16) -> Color {
    const DARK_BOOST_LIMIT: f64 = 48_000.0;
    let channels = [
        ((background >> 16) & 0xff) as u16 * 257,
        ((background >> 8) & 0xff) as u16 * 257,
        (background & 0xff) as u16 * 257,
    ];
    let brightness =
        (2 * u32::from(channels[0]) + 3 * u32::from(channels[1]) + u32::from(channels[2])) as f64
            / 6.0;
    let dimness = if brightness < DARK_BOOST_LIMIT {
        1.0 - brightness / DARK_BOOST_LIMIT
    } else {
        0.0
    };
    let adjustment = f64::from(delta) * dimness * factor / 2.0;
    let mut output = channels.map(|channel| {
        let scaled = f64::from(channel) * factor;
        if factor < 1.0 {
            (scaled - adjustment).clamp(0.0, f64::from(u16::MAX)) as u16
        } else {
            (scaled + adjustment).clamp(0.0, f64::from(u16::MAX)) as u16
        }
    });
    if output == channels {
        output = channels.map(|channel| channel.saturating_add(delta));
    }
    let pixel = (u32::from(output[0] >> 8) << 16)
        | (u32::from(output[1] >> 8) << 8)
        | u32::from(output[2] >> 8);
    Color::from_pixel(pixel)
}

pub(crate) fn gnu_tab_bar_pointer_appearance_style(
    relief_background: u32,
    frame_background: u32,
    horizontal_margin: f32,
    vertical_margin: f32,
    thickness: f32,
) -> TabBarPointerAppearanceStyle {
    let light = gnu_relief_color(relief_background, 1.2, 0x8000);
    let dark = gnu_relief_color(relief_background, 0.6, 0x4000);
    let margins = neomacs_display_protocol::PointerReliefMargins::new(
        horizontal_margin,
        vertical_margin,
        horizontal_margin,
        vertical_margin,
    );
    let edges = neomacs_display_protocol::PointerReliefEdges::new(true, true, true, true);
    let corner_erase = neomacs_display_protocol::PointerReliefCornerErase::new(
        Color::from_pixel(frame_background),
        6.0,
        1.0,
    );
    TabBarPointerAppearanceStyle::new(
        PointerImageRelief::new(light, dark, thickness, margins, edges, corner_erase),
        PointerImageRelief::new(dark, light, thickness, margins, edges, corner_erase),
    )
}

fn protocol_color_pixel(color: Color) -> u32 {
    let color = color.linear_to_srgb();
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u32;
    (channel(color.r) << 16) | (channel(color.g) << 8) | channel(color.b)
}

pub(crate) fn gnu_image_relief_background(
    box_shadow: Option<Color>,
    opaque_image_background: Option<u32>,
    glyph_face_background: Color,
) -> u32 {
    box_shadow
        .map(protocol_color_pixel)
        .or(opaque_image_background)
        .unwrap_or_else(|| protocol_color_pixel(glyph_face_background))
}

fn glyph_at_visual_column(row: &GlyphRow, target_col: u16) -> Option<&Glyph> {
    let mut col = 0u16;
    for glyph in &row.glyphs[GlyphArea::Text.index()] {
        if glyph.padding {
            continue;
        }
        if col == target_col {
            return Some(glyph);
        }
        col = col.saturating_add(glyph.materialized_slot_span());
    }
    None
}

pub(crate) fn tab_bar_image_relief_styles(
    rendered: &RenderedDisplayRow,
    fallback_background: u32,
    frame_background: u32,
    horizontal_margin: f32,
    vertical_margin: f32,
    thickness: f32,
) -> Vec<(u16, TabBarPointerAppearanceStyle)> {
    let mut col = 0u16;
    rendered.row().glyphs[GlyphArea::Text.index()]
        .iter()
        .filter(|glyph| !glyph.padding)
        .filter_map(|glyph| {
            let glyph_col = col;
            col = col.saturating_add(glyph.materialized_slot_span());
            let GlyphType::Image {
                opaque_background, ..
            } = glyph.glyph_type
            else {
                return None;
            };
            let face = glyph_at_visual_column(rendered.row(), glyph_col).and_then(|glyph| {
                rendered
                    .faces()
                    .iter()
                    .find(|face| face.id == glyph.face_id)
            });
            let background = gnu_image_relief_background(
                face.and_then(|face| face.box_color),
                opaque_background.get(),
                face.map(|face| face.background)
                    .unwrap_or_else(|| Color::from_pixel(fallback_background)),
            );
            Some((
                glyph_col,
                gnu_tab_bar_pointer_appearance_style(
                    background,
                    frame_background,
                    horizontal_margin,
                    vertical_margin,
                    thickness,
                ),
            ))
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum TabBarPointerPaintKind {
    Face,
    Image,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TabBarPointerAppearancePlan {
    identity: PointerAppearanceRangeId,
    kind: TabBarPointerPaintKind,
    hover: PointerDrawMode,
    pressed: PointerDrawMode,
}

#[derive(Clone, Debug, PartialEq)]
struct TabBarPointerRunPlan {
    local_bounds: FrameRect,
    col: u16,
    primitive_len: u32,
    interaction: InteractionId,
    appearance: Option<TabBarPointerAppearancePlan>,
}

fn push_coalesced_tab_pointer_run(
    runs: &mut Vec<TabBarPointerRunPlan>,
    next: TabBarPointerRunPlan,
) {
    if let Some(previous) = runs.last_mut()
        && previous.interaction == next.interaction
        && previous.appearance == next.appearance
        && previous.local_bounds.y() == next.local_bounds.y()
        && previous.local_bounds.height() == next.local_bounds.height()
        && (previous.local_bounds.x() + previous.local_bounds.width() - next.local_bounds.x()).abs()
            <= f32::EPSILON
    {
        previous.local_bounds = FrameRect::new(
            previous.local_bounds.x(),
            previous.local_bounds.y(),
            previous.local_bounds.width() + next.local_bounds.width(),
            previous.local_bounds.height(),
        )
        .expect("union of adjacent tab pointer runs is valid");
        previous.primitive_len = previous
            .primitive_len
            .checked_add(next.primitive_len)
            .expect("tab pointer run length fits u32");
    } else {
        runs.push(next);
    }
}

fn push_coalesced_tab_hit_region(hit_regions: &mut Vec<ChromeHitRegion>, next: ChromeHitRegion) {
    if let Some(previous) = hit_regions.last()
        && previous.action() == next.action()
    {
        let previous_bounds = previous.local_bounds().raw();
        let next_bounds = next.local_bounds().raw();
        if previous_bounds.y == next_bounds.y
            && previous_bounds.height == next_bounds.height
            && (previous_bounds.x + previous_bounds.width - next_bounds.x).abs() <= f32::EPSILON
        {
            let action = previous.action().clone();
            *hit_regions.last_mut().expect("last region still exists") = ChromeHitRegion::new(
                BandRect::new(
                    previous_bounds.x,
                    previous_bounds.y,
                    previous_bounds.width + next_bounds.width,
                    previous_bounds.height,
                )
                .expect("union of adjacent tab hit regions is valid"),
                action,
            );
            return;
        }
    }
    hit_regions.push(next);
}

pub(crate) struct TabBarPresentedPointerPlan {
    runs: Vec<TabBarPointerRunPlan>,
    hit_regions: Vec<ChromeHitRegion>,
}

impl TabBarPresentedPointerPlan {
    pub(crate) fn hit_regions(&self) -> &[ChromeHitRegion] {
        &self.hit_regions
    }

    pub(crate) fn into_source_map(
        self,
        band: FrameRect,
        canonical_row: u32,
    ) -> Result<neomacs_display_protocol::PresentedPointerSourceMap, PresentedPointerMapBuildError>
    {
        let mut appearance_bounds: HashMap<PointerAppearanceRangeId, FrameRect> = HashMap::new();
        for run in &self.runs {
            let Some(appearance) = run.appearance else {
                continue;
            };
            let bounds = FrameRect::new(
                band.x() + run.local_bounds.x(),
                band.y() + run.local_bounds.y(),
                run.local_bounds.width(),
                run.local_bounds.height(),
            )
            .expect("tab-bar band placement keeps finite positive geometry");
            appearance_bounds
                .entry(appearance.identity)
                .and_modify(|aggregate| {
                    let left = aggregate.x().min(bounds.x());
                    let top = aggregate.y().min(bounds.y());
                    let right =
                        (aggregate.x() + aggregate.width()).max(bounds.x() + bounds.width());
                    let bottom =
                        (aggregate.y() + aggregate.height()).max(bounds.y() + bounds.height());
                    *aggregate = FrameRect::new(left, top, right - left, bottom - top)
                        .expect("union of valid tab-bar regions is valid");
                })
                .or_insert(bounds);
        }
        let mut appearance_positions = HashMap::new();
        let mut appearances: Vec<(
            Vec<neomacs_display_protocol::PresentedSourcePaintSpan>,
            PointerDrawMode,
            PointerDrawMode,
        )> = Vec::new();
        let mut published_spans = HashSet::new();
        for run in &self.runs {
            let Some(appearance) = run.appearance else {
                continue;
            };
            let index = *appearance_positions
                .entry(appearance.identity)
                .or_insert_with(|| {
                    appearances.push((Vec::new(), appearance.hover, appearance.pressed));
                    appearances.len() - 1
                });
            if published_spans.insert((appearance.identity, appearance.kind, run.col)) {
                appearances[index].0.push(
                    neomacs_display_protocol::PresentedSourcePaintSpan::new_run(
                        match appearance.kind {
                            TabBarPointerPaintKind::Face => PresentedPrimitiveKind::Glyph,
                            TabBarPointerPaintKind::Image => PresentedPrimitiveKind::Image,
                        },
                        neomacs_display_protocol::GlyphRowRole::TabBar,
                        neomacs_display_protocol::DisplaySlotId {
                            window_id: neomacs_display_protocol::DisplayWindowId::new(0),
                            row: canonical_row,
                            col: run.col,
                        },
                        run.primitive_len,
                        appearance_bounds[&appearance.identity],
                    ),
                );
            }
        }
        let appearances = appearances
            .into_iter()
            .map(|(spans, hover, pressed)| {
                neomacs_display_protocol::PresentedPointerSourceAppearance::new(
                    spans, hover, pressed,
                )
            })
            .collect::<Vec<_>>();
        let mut regions = Vec::with_capacity(self.runs.len());
        for run in self.runs {
            let hit_bounds = FrameRect::new(
                band.x() + run.local_bounds.x(),
                band.y() + run.local_bounds.y(),
                run.local_bounds.width(),
                run.local_bounds.height(),
            )
            .expect("tab-bar band placement keeps finite positive geometry");
            let appearance = run
                .appearance
                .map(|appearance| {
                    PointerAppearanceId::try_from(appearance_positions[&appearance.identity])
                        .map_err(|_| PresentedPointerMapBuildError::TooManyAppearances)
                })
                .transpose()?;
            regions.push(neomacs_display_protocol::PresentedPointerRegion::new_owned(
                neomacs_display_protocol::PresentedRegionId::new(
                    None,
                    neomacs_display_protocol::PresentedRegionKind::TabBar,
                ),
                hit_bounds,
                Some(run.interaction),
                appearance,
            ));
        }
        Ok(neomacs_display_protocol::PresentedPointerSourceMap::new(
            regions,
            appearances,
        ))
    }

    #[cfg(test)]
    pub(crate) fn install_into(
        self,
        frame: &mut FrameGlyphBuffer,
        band: FrameRect,
    ) -> Result<(), PresentedPointerMapBuildError> {
        let source = self.into_source_map(band, 0)?;
        frame
            .install_presented_pointer_source_map(&source)
            .map_err(Into::into)
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn tab_bar_presented_pointer_plan(
    evaluator: &mut Context,
    presentation: u64,
    slots: &[TabBarPointerSlotPlan],
    items: &[TabBarSourceItem],
    height: f32,
    style: TabBarPointerAppearanceStyle,
    image_styles: &[(u16, TabBarPointerAppearanceStyle)],
    mouse_faces: &[(Value, FaceId)],
) -> TabBarPresentedPointerPlan {
    let mut targets: HashMap<(usize, bool), u32> = HashMap::new();
    let mut runs = Vec::new();
    let mut hit_regions = Vec::new();
    let mut next_appearance = 0_u64;
    let mut last_mouse_face: Option<(usize, usize, Value, PointerAppearanceRangeId)> = None;
    let mut relief_by_item: HashMap<usize, PointerAppearanceRangeId> = HashMap::new();

    for slot in slots {
        let item_index = slot.item_index;
        let item = &items[item_index];
        if !slot.enabled {
            last_mouse_face = None;
            continue;
        }
        let close = slot.close;
        let interaction = if let Some(interaction) = targets.get(&(item_index, close)).copied() {
            interaction
        } else {
            let Some(posn_string) = tab_bar_posn_string(evaluator, item, close) else {
                continue;
            };
            let interaction = evaluator.register_presented_mouse_target(
                presentation,
                PresentedMouseTarget {
                    area: PresentedMouseArea::TabBar,
                    posn_string,
                },
            );
            targets.insert((item_index, close), interaction);
            interaction
        };
        let local_bounds = FrameRect::new(slot.x, 0.0, slot.width, height)
            .expect("rendered tab source slots have valid geometry");
        let mouse_face = slot.mouse_face;
        let appearance = if !mouse_face.is_nil() {
            let Some((_, mouse_face_id)) =
                mouse_faces.iter().find(|(value, _)| *value == mouse_face)
            else {
                last_mouse_face = None;
                let interaction = InteractionId::new(interaction);
                push_coalesced_tab_pointer_run(
                    &mut runs,
                    TabBarPointerRunPlan {
                        local_bounds,
                        col: slot.col,
                        primitive_len: 1,
                        interaction,
                        appearance: None,
                    },
                );
                push_coalesced_tab_hit_region(
                    &mut hit_regions,
                    ChromeHitRegion::new(
                        BandRect::new(slot.x, 0.0, slot.width, height)
                            .expect("rendered tab source slots have valid geometry"),
                        ChromeAction::Presented { interaction },
                    ),
                );
                continue;
            };
            let identity = if let Some((previous_item, previous_char, previous_face, identity)) =
                last_mouse_face
                && previous_item == item_index
                && previous_char + 1 == slot.source_char_index
                && previous_face == mouse_face
            {
                identity
            } else {
                let identity = PointerAppearanceRangeId::new(next_appearance);
                next_appearance += 1;
                identity
            };
            last_mouse_face = Some((item_index, slot.source_char_index, mouse_face, identity));
            Some(TabBarPointerAppearancePlan {
                identity,
                kind: TabBarPointerPaintKind::Face,
                hover: PointerDrawMode::Face(*mouse_face_id),
                pressed: PointerDrawMode::Face(*mouse_face_id),
            })
        } else if item.key.as_symbol_name() == Some("add-tab") {
            last_mouse_face = None;
            let identity = *relief_by_item.entry(item_index).or_insert_with(|| {
                let identity = PointerAppearanceRangeId::new(next_appearance);
                next_appearance += 1;
                identity
            });
            let style = image_styles
                .iter()
                .find_map(|(col, style)| (*col == slot.col).then_some(*style))
                .unwrap_or(style);
            Some(TabBarPointerAppearancePlan {
                identity,
                kind: TabBarPointerPaintKind::Image,
                hover: PointerDrawMode::ImageRelief(style.raised),
                pressed: PointerDrawMode::ImageRelief(style.sunken),
            })
        } else {
            last_mouse_face = None;
            None
        };
        let interaction = InteractionId::new(interaction);
        push_coalesced_tab_pointer_run(
            &mut runs,
            TabBarPointerRunPlan {
                local_bounds,
                col: slot.col,
                primitive_len: 1,
                interaction,
                appearance,
            },
        );
        push_coalesced_tab_hit_region(
            &mut hit_regions,
            ChromeHitRegion::new(
                BandRect::new(slot.x, 0.0, slot.width, height)
                    .expect("rendered tab source slots have valid geometry"),
                ChromeAction::Presented { interaction },
            ),
        );
    }

    TabBarPresentedPointerPlan { runs, hit_regions }
}

pub(crate) struct ScratchGcRootScope {
    saved_len: usize,
}

impl ScratchGcRootScope {
    pub(crate) fn new() -> Self {
        Self {
            saved_len: neovm_core::emacs_core::eval::save_scratch_gc_roots(),
        }
    }

    fn root(&self, value: Value) {
        neovm_core::emacs_core::eval::push_scratch_gc_root(value);
    }
}

impl Drop for ScratchGcRootScope {
    fn drop(&mut self) {
        neovm_core::emacs_core::eval::restore_scratch_gc_roots(self.saved_len);
    }
}

pub(crate) fn build_tab_bar_display(
    evaluator: &mut Context,
    frame_id: u64,
    gc_roots: &ScratchGcRootScope,
) -> Option<BuiltTabBar> {
    TabBarDisplayBuildRequest::new(frame_id).build(evaluator, gc_roots)
}

pub(crate) fn max_mini_window_lines(evaluator: &Context, frame_rows: f32) -> f32 {
    let raw = evaluator
        .obarray()
        .symbol_value("max-mini-window-height")
        .copied()
        .unwrap_or_else(|| Value::make_float(0.25));
    max_mini_window_lines_from_value(raw, frame_rows)
}

pub(crate) fn max_mini_window_lines_for_buffer<B: LayoutBufferView>(
    evaluator: &Context,
    buffer: &B,
    frame_rows: f32,
) -> f32 {
    let raw = buffer_local_value(buffer, LayoutVar::MaxMiniWindowHeight)
        .or_else(|| {
            evaluator
                .obarray()
                .symbol_value("max-mini-window-height")
                .copied()
        })
        .unwrap_or_else(|| Value::make_float(0.25));
    max_mini_window_lines_from_value(raw, frame_rows)
}

pub(crate) fn max_mini_window_lines_from_value(raw: Value, frame_rows: f32) -> f32 {
    match raw.kind() {
        neovm_core::emacs_core::value::ValueKind::Float => {
            (frame_rows * raw.as_float().unwrap_or(0.25) as f32).max(1.0)
        }
        neovm_core::emacs_core::value::ValueKind::Fixnum(_) => raw.as_int().unwrap_or(1) as f32,
        _ => 1.0,
    }
}

fn window_chrome_display_origin(kind: WindowChromeKind, selected: bool) -> DisplayOrigin {
    match kind {
        WindowChromeKind::TabLine => DisplayOrigin::TabLine,
        WindowChromeKind::HeaderLine => DisplayOrigin::HeaderLine { selected },
        WindowChromeKind::ModeLine => DisplayOrigin::ModeLine { selected },
    }
}

fn presented_window_chrome_area(kind: WindowChromeKind) -> PresentedWindowChromeArea {
    match kind {
        WindowChromeKind::TabLine => PresentedWindowChromeArea::TabLine,
        WindowChromeKind::HeaderLine => PresentedWindowChromeArea::HeaderLine,
        WindowChromeKind::ModeLine => PresentedWindowChromeArea::ModeLine,
    }
}

#[cfg(test)]
pub(crate) fn window_chrome_row_height_for_face(
    font_metrics: &mut Option<FontMetricsService>,
    face: &ResolvedFace,
    fallback_metrics: DisplayRowFallbackMetrics,
) -> f32 {
    window_chrome_row_height_for_face_at_scale(
        font_metrics,
        face,
        fallback_metrics,
        neomacs_display_protocol::DeviceScale::ONE,
    )
}

#[cfg(test)]
pub(crate) fn window_chrome_row_height_for_face_at_scale(
    font_metrics: &mut Option<FontMetricsService>,
    face: &ResolvedFace,
    fallback_metrics: DisplayRowFallbackMetrics,
    device_scale: neomacs_display_protocol::DeviceScale,
) -> f32 {
    DisplayRowFaceRealizer::new_with_device_scale(font_metrics, device_scale).row_height_for_face(
        face,
        fallback_metrics.char_width(),
        fallback_metrics.ascent(),
        fallback_metrics.row_height(),
    )
}

#[cfg(test)]
#[path = "display_status_line_test.rs"]
mod tests;
