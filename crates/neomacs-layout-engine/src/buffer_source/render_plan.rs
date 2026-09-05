//! Buffer source render plan construction and completion.

use crate::buffer_source::body_render::BufferSourceWalkSetup;
use crate::buffer_source::end_of_buffer_rows::EndOfBufferRowsFillRequest;
use crate::buffer_source::face_resolution::*;
use crate::buffer_source::fringe_arrows::TruncationContinuationFringeRequest;
use crate::buffer_source::loop_context::BufferSourceLoopRequestContext;
use crate::buffer_source::render_attempt::{
    BufferSourceRedisplayPublishRequest, BufferSourceRenderAttemptContext,
    BufferSourceRenderAttemptOutcome, BufferSourceRetryPlan, WindowPositionPublication,
};
use crate::buffer_source::tail_render::{
    BufferSourceBodyInstallContext, BufferSourceRetryBounds, BufferSourceTailRequestContext,
};
use crate::buffer_source::window_geometry::{BufferWindowGeometry, BufferWindowLocalDisplayPolicy};
use crate::buffer_source::window_source::BufferWindowSource;
use crate::display_cursor::{
    CursorGlyphFaceColors, CursorVisualColumnResolutionRequest, ResolvedBoxCursorPaint,
    ResolvedCursorCoordinatePair,
};
use crate::display_face_policy::EffectiveWindowDefaultFace;
use crate::display_row::append_context::DisplayRowAppendSurface;
use crate::display_row::face_environment::FrameFaces;
use crate::display_row::face_state::{
    DisplayRowActiveFaceState, DisplayRowMeasurementMode, DisplayRowMeasurementPolicy,
};
use crate::display_row::geometry::{DisplayRowLimit, DisplayRowVisibilityLimit};
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::display_row::overlay_string::BufferOverlayStringTextRowRenderContext;
use crate::display_row::walk_state::{FaceScanCheckpoint, LineNumberFieldLayout};
use crate::display_status_line::{
    ChromeRowRenderServices, WindowChromeRowsRenderOutcome, WindowChromeRowsRenderRequest,
};
use crate::display_text_window_row_lifecycle::{TextWindowBeginRequest, TextWindowFinishState};
use crate::font::metrics::FontMetricsService;
use crate::frame_face_arena::FrameFaceAttempt;
use crate::incremental_layout::{
    CursorOnlyReplay, RetainedChrome, RetainedTextWindowCursor, ScrollReplay,
};
use crate::neovm_bridge::{FaceResolver, LayoutBufferView, ResolvedFace, RustBufferAccess};
use crate::types::WindowParams;
use crate::viewport_resolution::ForwardViewportMeasurement;
use crate::window_layout::{WindowChromeMetrics, WindowLayoutBox};
use crate::window_output::{
    TextWindowCursor, TextWindowCursorRole, TextWindowCursorSlots, TextWindowOutputTarget,
    TextWindowRedisplayPositions, WindowOutputEmitter, publish_text_window_cursor,
    record_text_window_display_range, render_window_chrome_rows,
};
use neomacs_display_protocol::frame_glyphs::{
    CursorStyle, DisplaySlotId, GlyphRowRole, PhysCursor,
};
use neomacs_display_protocol::glyph_matrix::{FaceFillItem, GlyphArea};
use neomacs_display_protocol::types::{Color, DisplayWindowId, Rect};
use neovm_core::buffer::BufferId;
use neovm_core::window::{FrameId, WindowId};

/// Selects the one authoritative source for a window's chrome rows.
///
/// The variants make the three legal redisplay states explicit: rebuild the
/// rows from live Lisp, reuse rows admitted by an incremental replay, or keep
/// the already-reserved geometry for a synchronous query that must not run
/// status-line Lisp. In particular, there is no representable state that both
/// retains and rebuilds chrome.
enum WindowChromeRowSource<'chrome> {
    Recompute,
    Retained(&'chrome RetainedChrome),
    PreserveReservedMetrics(WindowChromeMetrics),
}

impl<'chrome> From<Option<&'chrome RetainedChrome>> for WindowChromeRowSource<'chrome> {
    fn from(retained: Option<&'chrome RetainedChrome>) -> Self {
        match retained {
            Some(chrome) => Self::Retained(chrome),
            None => Self::Recompute,
        }
    }
}

/// Resolve chrome production and translate an invalidated Lisp source into the
/// containing buffer attempt's retry contract in exactly one place.
fn render_or_retain_window_chrome(
    output: TextWindowOutputTarget<'_>,
    output_emitter: &mut WindowOutputEmitter,
    evaluator: &mut neovm_core::emacs_core::Context,
    request: WindowChromeRowsRenderRequest<'_, '_>,
    render_services: ChromeRowRenderServices<'_, '_>,
    source: WindowChromeRowSource<'_>,
) -> Result<WindowChromeMetrics, BufferSourceRenderAttemptOutcome> {
    match source {
        WindowChromeRowSource::Recompute => match render_window_chrome_rows(
            output,
            output_emitter,
            evaluator,
            request,
            render_services,
        ) {
            WindowChromeRowsRenderOutcome::Rendered(metrics) => Ok(metrics),
            WindowChromeRowsRenderOutcome::SourceInvalidated => {
                Err(BufferSourceRenderAttemptOutcome::LogicalInputsChanged)
            }
        },
        WindowChromeRowSource::Retained(chrome) => Ok(
            crate::window_output::install_retained_window_chrome(output, output_emitter, chrome),
        ),
        WindowChromeRowSource::PreserveReservedMetrics(metrics) => Ok(metrics),
    }
}

fn freshness_before_window_chrome(
    evaluator: &neovm_core::emacs_core::Context,
    frame_id: FrameId,
    params: &WindowParams,
) -> Result<neovm_core::window::WindowLayoutAttemptFreshness, BufferSourceRenderAttemptOutcome> {
    evaluator
        .window_layout_attempt_freshness(
            frame_id,
            WindowId(params.window_id as u64),
            BufferId(params.buffer_id),
        )
        .ok_or(BufferSourceRenderAttemptOutcome::LogicalInputsChanged)
}

pub(crate) struct BufferSourceOutputSetup {
    begin_request: TextWindowBeginRequest,
    row_visibility_limit: DisplayRowVisibilityLimit,
    row_limit: DisplayRowLimit,
    body_install_context: BufferSourceBodyInstallContext,
    retry_bounds: BufferSourceRetryBounds,
    position_publication: WindowPositionPublication,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display_row::metrics::DisplayRowFallbackMetrics;
    use neovm_core::buffer::{Buffer, BufferId};
    use neovm_core::emacs_core::Value;
    use neovm_core::face::FaceTable;

    #[test]
    fn default_face_plan_uses_buffer_default_face_remap() {
        let _runtime = neovm_core::emacs_core::Context::new();
        let table = FaceTable::new();
        let resolver = FaceResolver::new(&table, 0x000000, 0xFFFFFF, 14.0, None);
        let mut buffer = Buffer::new_standalone(BufferId(42), Value::string("*default-remap*"));
        buffer.set_buffer_local(
            "face-remapping-alist",
            Value::list(vec![Value::list(vec![
                Value::symbol("default"),
                Value::list(vec![
                    Value::keyword("background"),
                    Value::string("#000000"),
                    Value::keyword("foreground"),
                    Value::string("#ffffff"),
                ]),
                Value::symbol("default"),
            ])]),
        );

        let plan = BufferSourceDefaultFacePlan::new(
            &resolver,
            &buffer,
            &mut None,
            DisplayRowMeasurementMode::LogicalCells,
            DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        );

        assert_eq!(plan.face().bg, 0x000000);
        assert_eq!(plan.face().fg, 0xFFFFFF);
    }
}

pub(crate) struct BufferSourceDefaultFacePlan {
    face: ResolvedFace,
    metrics: DisplayRowFallbackMetrics,
    measurement_policy: DisplayRowMeasurementPolicy,
}

/// Background ownership for one buffer window.
///
/// GNU's iterator always begins with the window's effective default face.  A
/// terminal-default background needs no explicit paint; every other effective
/// background must be published independently of whether the body is walked or
/// replayed.  Encoding that distinction here prevents an incremental path from
/// returning before it has restored the window-level paint contract.
#[derive(Clone, Debug)]
enum BufferWindowBackground {
    TerminalDefault,
    Resolved {
        face_id: neomacs_display_protocol::types::FaceId,
        face: ResolvedFace,
    },
}

impl BufferWindowBackground {
    fn from_default_face(default_face: &EffectiveWindowDefaultFace) -> Self {
        let face = default_face.face();
        if face.use_default_background {
            Self::TerminalDefault
        } else {
            Self::Resolved {
                face_id: default_face.face_id(),
                face: face.clone(),
            }
        }
    }

    fn publish(
        &self,
        output: &mut TextWindowOutputTarget<'_>,
        params: &WindowParams,
        geometry: &BufferWindowGeometry,
    ) {
        let Self::Resolved { face_id, face } = self else {
            return;
        };
        if geometry.text_height <= 0.0 {
            return;
        }
        output.install_resolved_face(*face_id, face, None);
        output.builder().add_output_face_fill(FaceFillItem {
            window_id: DisplayWindowId::new(params.window_id),
            row_role: GlyphRowRole::Text,
            clip_rect: Some(params.bounds),
            bounds: Rect::new(
                params.bounds.x,
                geometry.text_y,
                params.bounds.width,
                geometry.text_height,
            ),
            face_id: *face_id,
        });
    }
}

/// Publish a fast-path (cursor-only / scroll / edit) window's re-decorated cursor
/// through the SAME production machinery a full rebuild uses
/// ([`publish_text_window_cursor`] → [`TextWindowCursorPublication`]), so the
/// frame's single overwritable phys-cursor slot is stored ONLY for the active
/// window while an inactive window gets its per-window (hollow) cursor artifact
/// instead — never the reverse.
///
/// The frame phys_cursor is one slot per frame; installing it unconditionally for
/// every fast-path window let a non-selected window clobber the selected window's
/// caret (split-window `C-x 3` + `C-p`: the selected window's cursor vanished).
/// Routing through the shared publication keeps the fast paths from drifting
/// from full redisplay. Both roles first use one resolved slot; the role enum
/// chooses only the transport afterward.
///
/// The typed variants distinguish a point-derived cursor that must be resolved
/// against the installed row from an accepted presentation that already owns
/// both output and display coordinates.
enum FastPathCursorPlacement {
    /// A point-derived cursor whose output-space seed still needs mapping onto
    /// the retained row's materialized glyph slots.
    Reconstructed {
        output_cursor: PhysCursor,
        char_width: f32,
    },
    /// A semantic display-string cursor retained with both coordinate
    /// identities from the accepted presentation.
    Retained(RetainedTextWindowCursor),
}

impl FastPathCursorPlacement {
    fn resolve(
        self,
        output: &mut TextWindowOutputTarget<'_>,
        text_area_left: f32,
    ) -> ResolvedFastPathCursorPlacement {
        match self {
            Self::Reconstructed {
                mut output_cursor,
                char_width,
            } => {
                let coordinates = CursorVisualColumnResolutionRequest::from_cursor(&output_cursor)
                    .resolve_cursor_coordinates(output.builder().cursor_visual_column_context())
                    .unwrap_or_else(|| ResolvedCursorCoordinatePair::same(output_cursor.slot_id));
                coordinates.apply_display_to(&mut output_cursor);
                output_cursor.x =
                    text_area_left + f32::from(output_cursor.col) * char_width.max(1.0);
                ResolvedFastPathCursorPlacement {
                    presented: output_cursor,
                    coordinates,
                    output_grid_x: None,
                }
            }
            Self::Retained(retained) => ResolvedFastPathCursorPlacement {
                presented: retained.presented().clone(),
                coordinates: retained.coordinates(),
                output_grid_x: Some(retained.output_grid_x()),
            },
        }
    }
}

/// A fast-path cursor after its one coordinate-pair resolution step.
///
/// Keeping the presentation and its validated pair named until publication
/// prevents replay code from reassembling interchangeable raw slot IDs.
struct ResolvedFastPathCursorPlacement {
    presented: PhysCursor,
    coordinates: ResolvedCursorCoordinatePair,
    output_grid_x: Option<i64>,
}

fn publish_fast_path_cursor(
    output: &mut TextWindowOutputTarget<'_>,
    output_emitter: &mut WindowOutputEmitter,
    cursor: FastPathCursorPlacement,
    cursor_role: crate::types::WindowCursorRole,
    text_area_left: f32,
    window_top: f32,
) {
    let cursor = cursor.resolve(output, text_area_left);
    publish_text_window_cursor(
        output.reborrow(),
        output_emitter,
        TextWindowCursor {
            role: TextWindowCursorRole::from_window_role(cursor_role),
            window_id: cursor.presented.window_id.get(),
            charpos: cursor.presented.charpos,
            slots: TextWindowCursorSlots::resolved(cursor.coordinates),
            x: cursor.presented.x,
            y: cursor.presented.y,
            width: cursor.presented.width,
            height: cursor.presented.height,
            ascent: cursor.presented.ascent,
            style: cursor.presented.style,
            color: cursor.presented.color,
            cursor_fg: cursor.presented.cursor_fg,
            text_area_left,
            window_top,
            grid_x_override: cursor.output_grid_x,
        },
    );
}

fn replay_cursor_paint(
    output: &mut TextWindowOutputTarget<'_>,
    cursor_row: usize,
    point: usize,
    params: &WindowParams,
) -> ResolvedBoxCursorPaint {
    let fallback_face = CursorGlyphFaceColors::new(
        Color::from_pixel(params.default_fg),
        Color::from_pixel(params.default_bg),
    );
    let glyph_face_id = output
        .builder()
        .current_window_row(cursor_row)
        .and_then(|row| {
            row.glyphs[GlyphArea::Text.index()]
                .iter()
                .find(|glyph| row.glyph_covers_buffer_charpos(glyph, point))
                .map(|glyph| glyph.face_id)
        });
    let glyph_face = glyph_face_id
        .and_then(|face_id| output.builder().output_face(face_id))
        .map(|face| CursorGlyphFaceColors::new(face.foreground, face.background))
        .unwrap_or(fallback_face);

    ResolvedBoxCursorPaint::resolve_gnu(
        Color::from_pixel(params.cursor_color),
        glyph_face,
        Color::from_pixel(params.cursor_foreground),
    )
}

/// Re-decorate a window's cursor for the current point on an already-installed
/// grid row (Phase 2 scroll fast path). Reads the row geometry from the grid,
/// resolves the visual column, and writes the cursor onto the matrix row + the
/// window snapshot, then publishes the cursor via the shared role-directed
/// machinery (see [`publish_fast_path_cursor`]) so an inactive window's cursor
/// never clobbers the active window's frame phys-cursor. Mirrors the Phase 1
/// cursor-only branch but sources the row from the grid (the cursor may land in
/// a reused or a newly-exposed row).
#[allow(clippy::too_many_arguments)]
fn decorate_window_cursor(
    output: &mut TextWindowOutputTarget<'_>,
    output_emitter: &mut WindowOutputEmitter,
    window_id: u64,
    cursor_row: usize,
    point: usize,
    style: CursorStyle,
    params: &WindowParams,
    text_area_left: f32,
    window_top: f32,
    char_w: f32,
    default_height: f32,
    default_ascent: f32,
) {
    let (row_pixel_y, row_height, row_ascent, cursor_width) =
        match output.builder().current_window_row(cursor_row) {
            Some(row) => {
                let mut width = char_w;
                for glyph in &row.glyphs[GlyphArea::Text.index()] {
                    if row.glyph_covers_buffer_charpos(glyph, point) {
                        width = glyph.pixel_width;
                        break;
                    }
                }
                (row.pixel_y, row.height_px, row.ascent_px, width)
            }
            None => (0.0, default_height, default_ascent, char_w),
        };
    let window_id_i64 = window_id as i64;
    let paint = replay_cursor_paint(output, cursor_row, point, params);
    let cursor = PhysCursor {
        window_id: DisplayWindowId::new(window_id_i64),
        charpos: point,
        row: cursor_row,
        col: 0,
        slot_id: DisplaySlotId {
            window_id: DisplayWindowId::new(window_id_i64),
            row: cursor_row as u32,
            col: 0,
        },
        x: text_area_left,
        y: window_top + row_pixel_y,
        width: cursor_width,
        height: row_height,
        ascent: row_ascent,
        style,
        color: paint.background,
        cursor_fg: paint.glyph_foreground,
    };
    publish_fast_path_cursor(
        output,
        output_emitter,
        FastPathCursorPlacement::Reconstructed {
            output_cursor: cursor,
            char_width: char_w,
        },
        params.cursor_role,
        text_area_left,
        window_top,
    );
}

impl BufferSourceOutputSetup {
    pub(crate) fn from_window_geometry(
        frame_id: FrameId,
        window_id: WindowId,
        params: &WindowParams,
        geometry: &BufferWindowGeometry,
        layout_box: &WindowLayoutBox,
        max_rows: usize,
        walk_setup: &BufferSourceWalkSetup,
    ) -> Self {
        Self::new(
            frame_id,
            window_id,
            params.window_id as u64,
            geometry.display_text_row_base,
            geometry.display_text_rows,
            geometry.bottom_chrome_rows,
            geometry.matrix_columns.get(),
            params.bounds,
            params.text_bounds,
            layout_box.body(),
            params.selected,
            geometry.text_y,
            geometry.text_height,
            geometry.visibility_bottom_y,
            max_rows,
            walk_setup,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        frame_id: FrameId,
        window_id: WindowId,
        output_window_id: u64,
        display_text_row_base: usize,
        display_text_rows: usize,
        bottom_chrome_rows: usize,
        cols: usize,
        bounds: Rect,
        text_bounds: Rect,
        text_clip_bounds: Rect,
        selected: bool,
        text_y: f32,
        text_height: f32,
        visibility_bottom_y: f32,
        max_rows: usize,
        walk_setup: &BufferSourceWalkSetup,
    ) -> BufferSourceOutputSetup {
        let output_cols = cols.max(1);
        BufferSourceOutputSetup {
            begin_request: TextWindowBeginRequest::new(
                frame_id,
                window_id,
                display_text_row_base,
                walk_setup.text_area_left,
                walk_setup.window_top,
                output_window_id,
                display_text_row_base + display_text_rows + bottom_chrome_rows,
                output_cols,
                bounds,
                text_bounds,
                text_clip_bounds,
                selected,
                walk_setup.row_geometry.display_text_row_begin(
                    display_text_row_base,
                    walk_setup.col,
                    walk_setup.x,
                    crate::types::LayoutCharPos0::new(walk_setup.charpos),
                ),
            ),
            row_visibility_limit: DisplayRowVisibilityLimit {
                max_rows,
                // Lifted to span `max_rows` for a minibuffer so the unclamped
                // GNU `resize_mini_window` measurement can emit content rows
                // beyond the window's current physical height (see
                // `BufferWindowGeometry::visibility_bottom_y`).
                bottom_y: visibility_bottom_y,
            },
            row_limit: DisplayRowLimit { max_rows },
            body_install_context: BufferSourceBodyInstallContext::new(
                output_window_id,
                display_text_row_base,
                output_cols,
            ),
            retry_bounds: BufferSourceRetryBounds::new(
                (text_y - walk_setup.window_top).round() as i64,
                (text_y + text_height - walk_setup.window_top).round() as i64,
            ),
            position_publication: WindowPositionPublication::Redisplay,
        }
    }

    pub(crate) fn with_position_publication(
        mut self,
        publication: WindowPositionPublication,
    ) -> Self {
        self.position_publication = publication;
        self
    }
}

impl BufferSourceDefaultFacePlan {
    pub(crate) fn new(
        face_resolver: &FaceResolver,
        buffer: &impl LayoutBufferView,
        font_metrics: &mut Option<FontMetricsService>,
        measurement_mode: DisplayRowMeasurementMode,
        fallback_metrics: DisplayRowFallbackMetrics,
    ) -> Self {
        let face = face_resolver.resolve_buffer_default_face(buffer);
        let metrics = if measurement_mode.uses_concrete_font_geometry()
            && let Some(service) = font_metrics
        {
            let metrics = service.font_metrics(
                &face.font_family,
                face.font_weight,
                face.italic,
                face.font_size,
            );
            DisplayRowFallbackMetrics::from_font_metrics(metrics)
        } else {
            fallback_metrics
        };

        Self {
            face,
            metrics,
            measurement_policy: DisplayRowMeasurementPolicy::for_mode(measurement_mode),
        }
    }

    pub(crate) fn face(&self) -> &ResolvedFace {
        &self.face
    }

    /// Reserve the effective display identity of this buffer's default face.
    ///
    /// The basic `default` face owns the canonical frame ID only while buffer
    /// face remapping leaves its realized rendering unchanged. A remapped
    /// default is a distinct realized face and must receive a dynamic ID before
    /// any leading synthetic glyph can publish it.
    pub(crate) fn reserve_effective_face(
        &self,
        face_resolver: &FaceResolver,
        face_ids: &mut FrameFaceAttempt,
    ) -> EffectiveWindowDefaultFace {
        EffectiveWindowDefaultFace::resolve(face_resolver, &self.face, face_ids)
    }

    pub(crate) fn char_width(&self) -> f32 {
        self.metrics.char_width()
    }

    pub(crate) fn row_height(&self) -> f32 {
        self.metrics.row_height()
    }

    pub(crate) fn ascent(&self) -> f32 {
        self.metrics.ascent()
    }

    pub(crate) fn metrics(&self) -> DisplayRowFallbackMetrics {
        self.metrics
    }

    pub(crate) fn row_metrics_for_body_width(&self, char_width: f32) -> DisplayRowFallbackMetrics {
        self.metrics
            .with_extents(char_width, self.metrics.row_height())
    }

    pub(crate) fn measurement_policy(&self) -> DisplayRowMeasurementPolicy {
        self.measurement_policy
    }
}

impl BufferSourceOutputSetup {
    #[cfg(test)]
    pub(crate) fn row_visibility_limit(&self) -> DisplayRowVisibilityLimit {
        self.row_visibility_limit
    }

    #[cfg(test)]
    pub(crate) fn row_limit(&self) -> DisplayRowLimit {
        self.row_limit
    }

    #[cfg(test)]
    pub(crate) fn body_install_context(&self) -> BufferSourceBodyInstallContext {
        self.body_install_context
    }

    #[cfg(test)]
    pub(crate) fn retry_bounds(&self) -> BufferSourceRetryBounds {
        self.retry_bounds
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_body_attempt<'a, 'surface, 'buf, B>(
        self,
        walk_setup: &mut BufferSourceWalkSetup,
        state: BufferSourceRenderAttemptContext<'_, '_>,
        chrome_request: WindowChromeRowsRenderRequest<'_, '_>,
        remaining_visibility_retries: usize,
        forward_viewport_measurement: Option<ForwardViewportMeasurement>,
        local_display_policy: BufferWindowLocalDisplayPolicy,
        line_number_field: LineNumberFieldLayout,
        geometry: &BufferWindowGeometry,
        layout_box: &WindowLayoutBox,
        buffer: &'a B,
        buffer_id: BufferId,
        source: BufferWindowSource,
        params: &'a WindowParams,
        default_face: &'a BufferSourceDefaultFacePlan,
        window_metrics: DisplayRowFallbackMetrics,
        output_window_id: u64,
        append_surface: &'surface DisplayRowAppendSurface,
        reserve_right_special_col: bool,
        reserve_right_border_col: bool,
        text: &'a [u8],
        buf_access: &RustBufferAccess<'buf, B>,
        cursor_only: Option<CursorOnlyReplay>,
        scroll: Option<ScrollReplay>,
    ) -> BufferSourceRenderAttemptOutcome
    where
        B: LayoutBufferView,
    {
        let (mut output, font_metrics, face_resolver, mut face_ids, window_snapshots) =
            state.into_parts();
        let retry_checkpoint = output.capture_retry_checkpoint();
        let effective_default_face =
            default_face.reserve_effective_face(face_resolver, &mut face_ids);
        let default_face_id = effective_default_face.face_id();
        BufferWindowBackground::from_default_face(&effective_default_face).publish(
            &mut output.output_target(),
            params,
            geometry,
        );

        let has_overlays = !buffer.layout_overlays().is_empty();
        let face_resolution = BufferSourceFaceResolutionContext::new(
            buffer,
            face_resolver,
            default_face.measurement_policy(),
            default_face.face(),
            default_face_id,
            default_face.metrics(),
            window_metrics,
            params.image_scale_environment,
        );
        let row_fallback_metrics = default_face.row_metrics_for_body_width(geometry.char_width);
        let row_prelude_context =
            local_display_policy.row_prelude_context(line_number_field, row_fallback_metrics);
        let overlay_text_row = BufferOverlayStringTextRowRenderContext::new(
            has_overlays,
            output_window_id,
            append_surface,
            row_fallback_metrics,
            // Overlay-string rows share the body row grid, so they start from the
            // same vscroll-shifted origin (`text_y - vscroll`) as the buffer walk.
            geometry.row_origin_y(),
            self.body_install_context.display_text_row_base(),
            geometry.max_rows,
        )
        .with_continuation_row_prelude(row_prelude_context.continuation_row_prelude());
        let loop_context = BufferSourceLoopRequestContext::new(
            buffer_id,
            source.text_start_byte(),
            source.accessible_end(),
            source.point_charpos(),
            params,
            geometry.content_x,
            local_display_policy.has_prefix(),
            row_fallback_metrics,
            self.row_visibility_limit,
            walk_setup.row_geometry_defaults,
            self.body_install_context.display_text_row_base(),
            geometry.max_rows,
            self.row_limit,
            // Frame background = default-face background pixel. Used to gate the
            // trailing `:extend` fill (GNU extend_face_to_end_of_line): a fill
            // whose bg equals the frame bg is a visual no-op and is skipped.
            Color::from_pixel(default_face.face().bg),
        );
        let fallback_metrics = default_face.metrics();
        let tail_context = BufferSourceTailRequestContext::new(
            params,
            source.window_start(),
            source.accessible_start(),
            source.accessible_end(),
            source.text_start_byte(),
            self.body_install_context.display_text_row_base(),
            walk_setup.text_area_left,
            walk_setup.window_top,
            geometry.text_y,
            geometry.text_height,
            geometry.char_width,
            geometry.char_height,
            self.row_limit,
            self.retry_bounds,
            forward_viewport_measurement,
            self.body_install_context,
            reserve_right_special_col,
            reserve_right_border_col,
            default_face_id,
            default_face.face(),
            neovm_core::window::geometry::CellOrigin::new(params.left_col, params.top_line),
            layout_box.regions(),
        );
        let publish_request = BufferSourceRedisplayPublishRequest::new(
            self.begin_request.frame_id(),
            self.begin_request.window_id(),
            source.accessible_end_position(),
            self.position_publication,
        );

        // --- Phase 1 cursor-only fast path ---
        //
        // Point moved but every other layout input + the neovm-core invalidation
        // ticks are unchanged: install the retained body rows verbatim instead of
        // walking the buffer, re-decorate only the cursor, and fall through to the
        // SAME chrome + finish path. The output is byte-identical to a full
        // rebuild (honest layering, spec §4.6) — the win is skipping fontify /
        // shaping / measurement for the body.
        if let Some(mut replay) = cursor_only {
            // Taken before the replay's rows are moved into the grid below.
            let retained_chrome = replay.chrome.take();
            let mut output_emitter = output.begin_text_window_output(self.begin_request);

            // Capture the point glyph's metrics from the new cursor row before the
            // rows are moved into the grid (fall back to the window face metrics
            // when point is at EOL / on hidden text with no glyph of its own).
            // Cursor height/ascent are ROW metrics (`height_px`/`ascent_px`); only
            // the width comes from the point glyph's advance. Fall back to the
            // window face metrics when point is at EOL / on hidden text.
            let mut cursor_width = geometry.char_width;
            let mut cursor_height = geometry.char_height;
            let mut cursor_ascent = window_metrics.ascent();
            let mut cursor_row_pixel_y = 0.0_f32;
            // Body rows are in matrix (top-to-bottom) order; track the bottom
            // edge of the row above the cursor row.
            let mut prev_row_bottom = 0.0_f32;
            for (idx, row) in &replay.body_rows {
                if *idx == replay.new_cursor_row_index {
                    // The retained empty end-of-buffer placeholder row (point at
                    // the empty last line / ZV) can carry a stale `pixel_y` of 0
                    // because its geometry is not advanced from the running y
                    // during the layout walk. Trusting it verbatim placed the
                    // cursor at `window_top` — the whole "C-p then C-n at the last
                    // line jumps the cursor to line 1" bug. Clamp to the bottom of
                    // the row above so the cursor lands on its real line; a genuine
                    // top row (no row above) keeps `pixel_y` via `prev_row_bottom`
                    // starting at 0. The full-rebuild path is unaffected: it
                    // captures the cursor y during emission from `row_geometry`.
                    cursor_row_pixel_y = row.pixel_y.max(prev_row_bottom);
                    cursor_height = row.height_px;
                    cursor_ascent = row.ascent_px;
                    for glyph in &row.glyphs[GlyphArea::Text.index()] {
                        if row.glyph_covers_buffer_charpos(glyph, replay.new_point as usize) {
                            cursor_width = glyph.pixel_width;
                            break;
                        }
                    }
                }
                prev_row_bottom = row.pixel_y + row.height_px;
            }

            let (mut output, evaluator) = output.into_parts();
            // Phase A admitted every replaying window's retained faces and
            // reserved their complete frame-wide ID range before any fresh face
            // allocation. This window can now install rows without mutating the
            // namespace behind glyphs emitted by an earlier sibling.
            let mut render_services =
                ChromeRowRenderServices::new(font_metrics, face_resolver, &mut face_ids);

            // Install retained body rows verbatim (already bidi-finalized), with
            // the prior cursor decoration stripped — the new cursor is set below.
            for (idx, row) in replay.body_rows {
                // Verbatim install is a refcount bump; only a row still
                // carrying the prior cursor decoration pays a copy to strip
                // it (the new cursor is set below).
                let row = if row.cursor_col.is_some() || row.cursor_type.is_some() {
                    let mut stripped = neomacs_display_protocol::GlyphRow::clone(&row);
                    stripped.cursor_col = None;
                    stripped.cursor_type = None;
                    neomacs_display_protocol::glyph_matrix::MatrixRow::new(stripped)
                } else {
                    row
                };
                output.builder().install_finalized_output_row(idx, row);
            }

            // Seed the emitter's point-independent body half, then publish the
            // (unchanged) redisplay positions for the mode-line chrome.
            output_emitter.seed_cursor_only_body(replay.body_row_snapshots, replay.points);
            let mut redisplay_positions = TextWindowRedisplayPositions::from_output_rows(
                &output_emitter,
                tail_context.window_start,
                source.text_start_byte(),
                0,
            );
            // Cursor-only does not walk (byte_idx = 0 above), so from_output_rows
            // leaves window_end_byte at text_start_byte. Re-derive it from the
            // (correct) window_end char so the published byte companion matches a
            // full rebuild instead of defaulting to the buffer top.
            {
                let end_char0 = redisplay_positions
                    .window_end_lisp()
                    .to_one_based_usize()
                    .saturating_sub(1) as i64;
                redisplay_positions.replace_window_end_anchor(
                    neovm_core::buffer::TextPositionAnchor::new(
                        neovm_core::buffer::CharPos0::new(end_char0 as usize),
                        neovm_core::buffer::EmacsBytePos::new(
                            buf_access.charpos_to_bytepos(end_char0) as usize,
                        ),
                    ),
                );
            }
            record_text_window_display_range(
                output.reborrow(),
                redisplay_positions.display_range(output_window_id),
            );
            publish_request.publish_window_end(evaluator, redisplay_positions);

            // Re-decorate the cursor. When point is unchanged, preserve the
            // authoritative presented placement captured by the full walk; it
            // may come from a display-string `cursor` property and therefore
            // intentionally differ from the buffer-point column. When point
            // moved, resolve a fresh buffer-point placement on the retained row.
            let window_id_i64 = output_window_id as i64;
            let display_window_id = DisplayWindowId::new(window_id_i64);
            let cursor = if let Some(retained) = replay.retained_cursor.as_ref() {
                FastPathCursorPlacement::Retained(retained.clone())
            } else {
                let paint = replay_cursor_paint(
                    &mut output,
                    replay.new_cursor_row_index,
                    replay.new_point as usize,
                    params,
                );
                let cursor = PhysCursor {
                    window_id: display_window_id,
                    charpos: replay.new_point as usize,
                    row: replay.new_cursor_row_index,
                    col: 0,
                    slot_id: DisplaySlotId {
                        window_id: display_window_id,
                        row: replay.new_cursor_row_index as u32,
                        col: 0,
                    },
                    x: walk_setup.text_area_left,
                    y: walk_setup.window_top + cursor_row_pixel_y,
                    width: cursor_width,
                    height: cursor_height,
                    ascent: cursor_ascent,
                    style: replay.cursor_style,
                    color: paint.background,
                    cursor_fg: paint.glyph_foreground,
                };
                FastPathCursorPlacement::Reconstructed {
                    output_cursor: cursor,
                    char_width: geometry.char_width,
                }
            };
            // Publish through the shared role-directed machinery so a non-selected
            // window in a split never clobbers the selected window's frame
            // phys-cursor (see `publish_fast_path_cursor`).
            publish_fast_path_cursor(
                &mut output,
                &mut output_emitter,
                cursor,
                params.cursor_role,
                walk_setup.text_area_left,
                walk_setup.window_top,
            );

            // Chrome is re-walked unless the replay carries a retained chrome
            // plan — GNU's one-line optimization never reaches
            // `display_mode_lines` (xdisp.c:17572-17726). The permission is
            // decided in `RetainedWindowMatrix::chrome_reusable_after_cursor_move`,
            // where the dirty flags and the point-stayed-on-this-line
            // precondition live.
            let freshness_before_chrome =
                match freshness_before_window_chrome(evaluator, publish_request.frame_id(), params)
                {
                    Ok(freshness) => freshness,
                    Err(outcome) => return outcome,
                };
            let measured_chrome_heights = match render_or_retain_window_chrome(
                output.reborrow(),
                &mut output_emitter,
                evaluator,
                chrome_request,
                render_services.reborrow(),
                retained_chrome.as_ref().into(),
            ) {
                Ok(metrics) => metrics,
                Err(outcome) => return outcome,
            };
            tail_context.finish_and_install(
                TextWindowFinishState::new(output, output_emitter, evaluator),
                measured_chrome_heights,
                window_snapshots,
            );
            return BufferSourceRenderAttemptOutcome::Finished {
                redisplay_positions,
                window_end_record: publish_request.window_end_record(redisplay_positions),
                freshness_before_chrome,
                effective_default_face,
                cursor_only: true,
                reused_matrix_rows: None,
                line_number_field_width: line_number_field.extent().get(),
            };
        }

        let mut line_numbers = local_display_policy.initial_line_numbers(
            buf_access,
            tail_context.window_start,
            loop_context.point_charpos(),
        );
        let mut face_scan = FaceScanCheckpoint::initial();
        let default_measured_face = default_face.measurement_policy().measured_face(
            default_face_id,
            default_face.face(),
            None,
            default_face.char_width(),
            fallback_metrics,
            font_metrics,
        );
        let mut active_face_state =
            DisplayRowActiveFaceState::new(default_face.face().clone(), default_measured_face);
        // --- Phase 2 pure-scroll fast path ---
        //
        // render_into overrode the geometry so THIS walk lays ONLY the
        // newly-exposed rows (at matrix indices [exposed_row_base..]); the source
        // reads from the replay's typed partial-body walk start. We then install the reused rows
        // (shifted) above them, splice the snapshots, re-decorate the cursor, and
        // re-walk chrome. Byte-identical to a full rebuild of the scrolled window.
        if let Some(mut scroll) = scroll {
            let retained_chrome = scroll.chrome.take();
            // Phase A already admitted the frame-wide retained face namespace
            // before this partial walk can mint IDs.
            let (mut output_emitter, _post_loop) = walk_setup.begin_render_body_and_tail(
                self.begin_request,
                &mut output,
                font_metrics,
                face_resolver,
                &mut face_ids,
                &mut line_numbers,
                &mut face_scan,
                &mut active_face_state,
                row_prelude_context,
                loop_context,
                face_resolution,
                &tail_context,
                text,
                params,
                overlay_text_row,
                buffer,
                buf_access,
            );
            let (mut output, evaluator) = output.into_parts();

            // Post-walk validation (GNU try_window_id: the regenerated region
            // must sync back up with the reused rows). The bounded walk just
            // relaid the dirty span optimistically; if the span's structure
            // changed in any way the prove-ahead gates could not see — a row
            // wrapped (continued), the span re-flowed to a different height, or
            // its end charpos no longer meets the first reused-below row — the
            // reused rows below it are positioned wrong. Bail the whole replay:
            // the caller re-lays the window from scratch, exactly as if no
            // fast path had been planned.
            if let Some(expected) = scroll.expected_walk {
                let mut height = 0.0f32;
                let mut last_end = None;
                let mut broken = false;
                for i in 0..expected.row_count {
                    match output
                        .builder()
                        .current_window_row(scroll.exposed_row_base + i)
                    {
                        Some(row) if row.enabled && !row.continued => {
                            height += row.height_px;
                            last_end = Some(row.end_charpos);
                        }
                        _ => {
                            broken = true;
                            break;
                        }
                    }
                }
                if broken
                    || last_end != Some(expected.last_row_end_charpos)
                    || (height - expected.total_height_px).abs() >= 0.5
                {
                    crate::window_output::restore_text_window_retry_checkpoint(
                        output.reborrow(),
                        retry_checkpoint,
                    );
                    return BufferSourceRenderAttemptOutcome::ReplayMispredicted;
                }
            }

            let mut render_services =
                ChromeRowRenderServices::new(font_metrics, face_resolver, &mut face_ids);

            // Redisplay positions: window_end is the LAST visible row.
            //  - SCROLL: reused rows sit ABOVE the exposed (bottom) rows, so the
            //    last visible row is the last EXPOSED row — compute BEFORE the
            //    reused rows are spliced in out of visual order.
            //  - BELOW-REUSE (bound_walk): reused rows sit BELOW the exposed
            //    (edited) line, so the last visible row is the last reused-below
            //    row — compute AFTER they are spliced into the emitter.
            let scroll_positions = (!scroll.bound_walk).then(|| {
                TextWindowRedisplayPositions::from_output_rows(
                    &output_emitter,
                    scroll.new_window_start,
                    source.text_start_byte(),
                    walk_setup.byte_idx,
                )
            });

            // Install the reused (shifted, already-finalized) rows and splice
            // their snapshots/points into the emitter.
            let reused_matrix_rows =
                crate::incremental_layout::ReusedMatrixRows::from_replay_rows(&scroll.reused_rows);
            for (idx, row) in &scroll.reused_rows {
                output
                    .builder()
                    .install_finalized_output_row(*idx, row.clone());
            }
            output_emitter.push_reused_body(scroll.reused_row_snapshots, scroll.reused_points);
            output_emitter.normalize_body_start_cols();

            let mut redisplay_positions = scroll_positions.unwrap_or_else(|| {
                TextWindowRedisplayPositions::from_output_rows(
                    &output_emitter,
                    scroll.new_window_start,
                    source.text_start_byte(),
                    walk_setup.byte_idx,
                )
            });
            if scroll.bound_walk {
                // The bounded walk's `byte_idx` stops at the edited line, so the
                // from_output_rows window_end_byte points there, not at the last
                // reused-below row. Re-derive it from the (correct) window_end
                // char so the published byte companion matches a full rebuild.
                let end_char0 = redisplay_positions
                    .window_end_lisp()
                    .to_one_based_usize()
                    .saturating_sub(1) as i64;
                redisplay_positions.replace_window_end_anchor(
                    neovm_core::buffer::TextPositionAnchor::new(
                        neovm_core::buffer::CharPos0::new(end_char0 as usize),
                        neovm_core::buffer::EmacsBytePos::new(
                            buf_access.charpos_to_bytepos(end_char0) as usize,
                        ),
                    ),
                );
            }

            // Finalize the exposed rows (reused rows are already finalized), then
            // publish the corrected positions for the mode-line chrome.
            let _ = walk_setup.install_body(
                output.reborrow(),
                &mut output_emitter,
                render_services.reborrow(),
                &tail_context,
            );
            record_text_window_display_range(
                output.reborrow(),
                redisplay_positions.display_range(output_window_id),
            );
            publish_request.publish_window_end(evaluator, redisplay_positions);

            // The partial walk decorated a spurious cursor at its pinned point;
            // clear it, then re-decorate for the REAL moved point (which may sit
            // in a reused OR a newly-exposed row); skipped when point is off-screen.
            output.builder().clear_current_window_cursors();
            if let Some(cursor_row) = output
                .builder()
                .find_current_window_cursor_row(scroll.new_point as usize)
            {
                decorate_window_cursor(
                    &mut output,
                    &mut output_emitter,
                    output_window_id,
                    cursor_row,
                    scroll.new_point as usize,
                    scroll.cursor_style,
                    params,
                    walk_setup.text_area_left,
                    walk_setup.window_top,
                    geometry.char_width,
                    geometry.char_height,
                    window_metrics.ascent(),
                );
            }

            // As on the cursor-only path: an EDIT replay confined to the cursor's
            // own row may re-install the retained chrome, while a genuine scroll
            // never may (its `%p` moved). The discriminator is in
            // `RetainedWindowMatrix::chrome_reusable_after_edit`.
            let freshness_before_chrome =
                match freshness_before_window_chrome(evaluator, publish_request.frame_id(), params)
                {
                    Ok(freshness) => freshness,
                    Err(outcome) => return outcome,
                };
            let measured_chrome_heights = match render_or_retain_window_chrome(
                output.reborrow(),
                &mut output_emitter,
                evaluator,
                chrome_request,
                render_services.reborrow(),
                retained_chrome.as_ref().into(),
            ) {
                Ok(metrics) => metrics,
                Err(outcome) => return outcome,
            };

            tail_context.finish_and_install(
                TextWindowFinishState::new(output, output_emitter, evaluator),
                measured_chrome_heights,
                window_snapshots,
            );
            return BufferSourceRenderAttemptOutcome::Finished {
                redisplay_positions,
                window_end_record: publish_request.window_end_record(redisplay_positions),
                freshness_before_chrome,
                effective_default_face,
                cursor_only: false,
                reused_matrix_rows: Some(reused_matrix_rows),
                line_number_field_width: line_number_field.extent().get(),
            };
        }

        let (output_emitter, post_loop) = walk_setup.begin_render_body_and_tail(
            self.begin_request,
            &mut output,
            font_metrics,
            face_resolver,
            &mut face_ids,
            &mut line_numbers,
            &mut face_scan,
            &mut active_face_state,
            row_prelude_context,
            loop_context,
            face_resolution,
            &tail_context,
            text,
            params,
            overlay_text_row,
            buffer,
            buf_access,
        );

        let retry_plan = BufferSourceRetryPlan::from_post_loop(
            tail_context.params.window_id,
            tail_context.window_start,
            tail_context.params.point_charpos().get(),
            walk_setup.charpos,
            self.retry_bounds,
            post_loop,
        );
        retry_plan.log_visibility_adjustments();

        let source_exhausted = walk_setup.charpos >= source.accessible_end();
        let retry_budget = if self
            .position_publication
            .keeps_complete_minibuffer_measurement_start()
            && source_exhausted
        {
            0
        } else {
            remaining_visibility_retries
        };
        if !params.force_start {
            if let Some(decision) = retry_plan.viewport_resolution(retry_budget) {
                output.restore_retry_checkpoint(retry_checkpoint);
                return BufferSourceRenderAttemptOutcome::ResolveViewport { decision };
            }
        }
        if let Some(window_start) = retry_plan.should_retry(retry_budget) {
            // GNU `w->force_start` (redisplay_window force_start branch): an
            // explicitly scrolled/set start is kept, and POINT moves into the
            // window instead of the start moving back to point.
            if params.force_start {
                if let Some(point_charpos) = retry_plan
                    .forced_start_point_target()
                    .filter(|target| *target != tail_context.params.point_charpos().get())
                {
                    output.restore_retry_checkpoint(retry_checkpoint);
                    return BufferSourceRenderAttemptOutcome::RetryPointIntoWindow {
                        point_charpos,
                    };
                }
            } else {
                retry_plan.log_retry(window_start, remaining_visibility_retries);
                output.restore_retry_checkpoint(retry_checkpoint);
                return BufferSourceRenderAttemptOutcome::Retry { window_start };
            }
        }

        let (mut output, evaluator) = output.into_parts();
        let window_faces = FrameFaces::new(face_resolver).for_window(buffer);
        let mut render_services =
            ChromeRowRenderServices::new(font_metrics, face_resolver, &mut face_ids);
        let mut output_emitter = output_emitter;
        let redisplay_positions = walk_setup.install_body_and_publish_redisplay(
            output.reborrow(),
            &mut output_emitter,
            evaluator,
            render_services.reborrow(),
            &tail_context,
            publish_request,
        );
        // GNU's redisplay tail keeps producing rows below the last buffer line.
        // Compose the decorations for those rows here: a line-number-faced
        // TEXT_AREA prefix when line numbers are active, and an `empty-line`
        // fringe bitmap when requested. This runs after the body is installed
        // (so `walk_setup.row_geometry` is immediately below the last buffer
        // row) and before mode-line chrome, with row and pixel boundary guards.
        EndOfBufferRowsFillRequest::new(
            params,
            geometry.display_text_row_base,
            geometry.max_rows,
            geometry.text_y,
            geometry.text_height,
            geometry.char_height,
            window_metrics.ascent(),
            line_number_field,
            walk_setup.beyond_accessible_end_line_prefix.as_ref(),
        )
        .fill(
            buffer,
            output.reborrow(),
            evaluator,
            window_faces,
            render_services.face_ids(),
            &walk_setup.row_geometry,
        );
        // GNU's `overlay_arrow_at_row` draws the overlay arrow — a left-fringe
        // bitmap on a window-system frame with a left fringe, else the string
        // over the marked row's leading glyphs. Stamp it here for the same
        // reason the fringe bitmaps below are: the rows are installed and their
        // start/end charpos are final, which is exactly what GNU's row test
        // needs.
        let overlay_arrow_style = crate::display_overlay_arrow::OverlayArrowStyle::for_window(
            params.window_system,
            params.left_fringe_width,
            geometry.char_width,
        );
        crate::display_overlay_arrow::draw_overlay_arrows(
            output.reborrow(),
            evaluator,
            buffer,
            neovm_core::buffer::BufferId(params.buffer_id),
            window_faces,
            render_services.face_ids(),
            overlay_arrow_style,
        );
        // GNU `draw_window_fringes` (src/fringe.c): every truncated/continued
        // buffer-text row gets a left/right arrow bitmap in its fringe. neomacs's
        // body walk records the truncation/continuation state in `row_flags`
        // (+ `GlyphRow::truncated_left` for the hscroll left edge); resolve the
        // arrow bitmaps through `fringe-indicator-alist` and stamp them onto the
        // already-installed rows here, after the body + empty-line filler so an
        // explicit `(left-fringe …)` spec or the empty-line `~` keeps the slot.
        if let Some(arrows) = TruncationContinuationFringeRequest::new(
            buffer,
            evaluator,
            params,
            geometry.display_text_row_base,
            {
                let resolved = window_faces.resolve_named_face("fringe");
                let face_id = crate::display_row::face_state::stable_face_id_for_resolved(
                    render_services.face_ids(),
                    &resolved,
                );
                output.install_resolved_face(face_id, &resolved, None);
                face_id
            },
        ) {
            arrows.install(output.builder(), &walk_setup.row_flags);
        }
        // GNU `display_mode_line` returns the laid-out row's height into
        // `w->mode_line_height`; the chrome render likewise hands back the
        // *measured* chrome heights so the window snapshot reports the real
        // (possibly taller, e.g. doom-modeline's bar) height rather than the
        // face-only estimate the text-area geometry was reserved from.
        let chrome_source = if self.position_publication.is_synchronous_query() {
            // GNU `Fwindow_end` builds a stack-local display iterator for the
            // text area.  Reuse the already accepted partition instead of
            // evaluating status-line Lisp as a side effect of a geometry
            // query.  This result is discarded rather than presented, so no
            // chrome rows need to be emitted.
            WindowChromeRowSource::PreserveReservedMetrics(WindowChromeMetrics::from_params(params))
        } else {
            WindowChromeRowSource::Recompute
        };
        let freshness_before_chrome =
            match freshness_before_window_chrome(evaluator, publish_request.frame_id(), params) {
                Ok(freshness) => freshness,
                Err(outcome) => return outcome,
            };
        let measured_chrome_heights = match render_or_retain_window_chrome(
            output.reborrow(),
            &mut output_emitter,
            evaluator,
            chrome_request,
            render_services.reborrow(),
            chrome_source,
        ) {
            Ok(metrics) => metrics,
            Err(outcome) => return outcome,
        };
        tail_context.finish_and_install(
            TextWindowFinishState::new(output, output_emitter, evaluator),
            measured_chrome_heights,
            window_snapshots,
        );
        BufferSourceRenderAttemptOutcome::Finished {
            redisplay_positions,
            window_end_record: publish_request.window_end_record(redisplay_positions),
            freshness_before_chrome,
            effective_default_face,
            cursor_only: false,
            reused_matrix_rows: None,
            line_number_field_width: line_number_field.extent().get(),
        }
    }
}
