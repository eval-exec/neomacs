use super::*;
use neomacs_display_protocol::VideoId;

fn test_image_load(id: u32) -> neomacs_display_protocol::ImageLoadToken {
    neomacs_display_protocol::ImageLoadToken::new(
        neomacs_display_protocol::ImageId::new(id),
        neomacs_display_protocol::ImageLoadAttempt::new(1).expect("nonzero test attempt"),
    )
}
use crate::buffer_source::consumption::*;
use crate::buffer_source::display_property_render::{
    BufferDisplayPropertyTextReplacementRenderOutcome,
    BufferDisplayPropertyTextReplacementRenderRequest,
    BufferDisplayPropertyTextReplacementRenderState,
};
use crate::buffer_source::face_resolution::*;
use crate::buffer_source::item_append::*;
use crate::buffer_source::loop_context::*;
use crate::buffer_source::loop_state::{
    BufferSourceHitCaptureState, BufferSourceLoopMutableState, BufferSourceRowBuildState,
    BufferSourceRowCarryoverState, BufferSourceSurfaceContext,
};
use crate::buffer_source::overflow::*;
use crate::buffer_source::producer::frame::ReplacementCoveredSpan;
use crate::buffer_source::render::*;
use crate::buffer_source::row_lifecycle::*;
use crate::buffer_source::text_source::*;
use crate::buffer_source::walk::*;
use crate::buffer_source::window_render::*;
use crate::display_current_row_output::DisplayRowCurrentRowOutput;
use crate::display_current_row_output::append_rendered_display_row_fragment_to_text_row_and_emit;
use crate::display_cursor::CursorCaptureState;
use crate::display_face_policy::BaseFacePolicy;
use crate::display_face_ref::render_face_ref_id;
use crate::display_item::{
    BufferDisplayPropertyReplacementItem, BufferDisplayReplacementSource, DisplayImageItem,
    DisplayItemKind, DisplayItemLayout, DisplayLength, DisplayMediaReplacement,
    DisplayMediaReplacementKind, DisplayPropertyReplacementDescriptor, DisplaySourceMappedText,
    DisplaySourcePosition, DisplayStretch, DisplayStretchWidth, DisplayVideoItem,
    DisplayXwidgetItem, GlyphlessMethod, RenderFaceRef,
};
use crate::display_origin::{DisplayOrigin, DisplayPropertySource};
use crate::display_property::{
    DisplayMediaReplacementProperty, DisplayPropertyClassification, DisplayReplacementProperty,
    classify_display_property,
};
use crate::display_row::append_context::*;
use crate::display_row::builder::{
    DisplayRowAppendProgress, DisplayRowAppendStatus, DisplayRowGlyphCheckpoint,
    DisplayRowGlyphSlot, DisplayRowItemMeasurement, DisplayRowPosition, DisplayRowWriteMetrics,
    DisplayTabPolicy,
};
use crate::display_row::face_state::{DisplayRowExtendFace, DisplayRowMeasurementMode};
use crate::display_row::geometry::{
    DisplayRowBoundaryTarget, DisplayRowFlagKind, DisplayRowFlags, DisplayRowGeometryDefaults,
    DisplayRowGeometryState, DisplayRowHitRange, DisplayRowLimit, DisplayRowMaxX,
    DisplayRowScopedValue, DisplayRowStartMarker, DisplayRowVisibilityLimit, DisplayRowYPositions,
};
use crate::display_row::line_number_prefix::BufferLineNumberTextPrefixRenderRequest;
use crate::display_row::lisp_string::{
    BufferLinePrefixRenderRequest, DisplayRowPrefixRequest, DisplayRowPrefixValues,
    LispStringRowAppendContext, LispStringSourceAppendRequest,
    LispStringSourceAppendSessionRequest, LispStringSourceId, LispStringSourceRowAppendSession,
    LispStringSourceRowAppendSessionRequest, append_lisp_string_to_text_row,
    apply_pending_display_source_faces,
};
use crate::display_row::overlay_string::{
    BufferOverlayStringTextRowRenderContext, OverlayStringRenderPositions,
    OverlayStringRenderRowContext, OverlayStringRenderState, OverlayStringRowBreakRenderContext,
};
use crate::display_row::replacement::*;
use crate::display_row::source_render::{
    TextRowOutputRenderState, TextRowSourceMeasureState, TextRowSourceRenderState,
};
use crate::display_row::transition::*;
use crate::display_row::walk_state::{
    BoxFaceRowState, DisplayRowTextOverflowDecision, FaceScanCheckpoint, HitRowRangeTracker,
    HorizontalScrollDisplayItem, HorizontalScrollSkipState, HorizontalScrollTruncationTarget,
    HorizontalScrollVisibleRemainder, HscrollConsumedTextDisposition, InvisibleTextScanCheckpoint,
    LineNumberRenderState, TrailingWhitespaceRenderState, WordWrapBreakCandidate,
    WordWrapRenderState,
};
use crate::display_row::{
    CurrentTextRowRenderOutcome, DisplayRowActiveFaceState, DisplayRowFallbackMetrics,
    DisplayRowGeometry, DisplayRowMeasuredFaceMetrics, DisplayRowMeasurementPolicy,
    DisplayRowRenderBounds, DisplayRowRenderPolicy, DisplayRowRenderStop, DisplayRowRenderer,
    DisplayRowSourceFragmentFrame, DisplayRowSourceState,
};
use crate::display_source::*;
use crate::display_source::{
    DisplayPropertyReplacementCursorPolicy, DisplayPropertyReplacementSourceItem,
    DisplayReplacementMediaSourceItem, DisplayReplacementMediaSourceResolution,
    DisplayReplacementSourceMappedTextItem, DisplayReplacementStretchSourceItem,
    DisplayReplacementStringSourceItem, DisplaySourceRenderPlanRequest,
    DisplaySourceSpecialDisplay, DisplaySpaceAscentPolicy, DisplaySpaceHeightPolicy,
    DisplaySpaceWidthPolicy,
};
use crate::display_source_append_plan::{
    DisplaySourceAppendMeasurementKind, DisplaySourceAppendRenderPlan,
    NaturalDisplayRowAppendRenderPolicy,
};
use crate::display_source_item_append::*;
use crate::display_source_overflow::DisplaySourceTextCharOverflowAction;
use crate::display_source_progress::{DisplaySourceProgressState, DisplaySourceRowProgressState};
use crate::display_source_resolver::DisplayPropertyReplacementSourceResolveRequest;
use crate::display_text_run_measurement::{DisplayTextRunAdvance, DisplayTextRunMeasurement};
use crate::display_text_window_row_lifecycle::{
    TextWindowBeginRequest, TextWindowBodyInstallRenderContext, TextWindowBodyInstallRequest,
    TextWindowBodyInstallState, TextWindowCursorEffectsRequest, TextWindowCursorPublishStatus,
    TextWindowFinishRequest, TextWindowFinishState, TextWindowTailFinalizeContext,
    TextWindowTailFinalizeRequest, TextWindowTailFinalizeState,
    TextWindowTerminalRightBorderRequest, TextWindowVisibilityRetryRequest,
};
use crate::font::metrics::FontMetricsService;
use crate::frame_face_arena::FrameFaceAttempt;
use crate::neovm_bridge::{FaceResolver, LayoutBufferSnapshot, RustBufferAccess};
use crate::scroll_policy::ScrollPolicy;
use crate::types::DisplayLineNumbersMode;
use crate::types::LayoutCharPos0;
use crate::types::WindowKind;
use crate::window_output::DisplayTextRowTransition;
use crate::window_output::TextWindowOutputTarget;
use crate::{LineWrapMode, WindowParams};
use neomacs_display_protocol::effect_config::EffectsConfig;
use neomacs_display_protocol::face::{BasicFaceId, Face};
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
use neomacs_display_protocol::glyph_matrix::{GlyphArea, GlyphType};
use neomacs_display_protocol::types::DisplayWindowId;
use neomacs_display_protocol::types::FaceId;
use neomacs_display_protocol::types::{Color, Rect};
use neovm_core::buffer::{Buffer, BufferId, CharPos0, EmacsBytePos, EmacsByteRange, LispCharPos1};
use neovm_core::emacs_core::eval::{DisplayHost, GuiFrameHostRequest};
use neovm_core::emacs_core::image_catalog::{
    ImageCatalog, ImageLookup, ImageResolveRequest, PendingImage, ReadyImage,
};
use neovm_core::emacs_core::value::StringTextPropertyRun;
use neovm_core::emacs_core::{Context, Value};
use neovm_core::face::FaceTable;
use std::sync::{Arc, Mutex};

fn text_row_output_render_state<'a>(
    builder: &'a mut crate::output::builder::DisplayOutputBuilder,
    output_emitter: &'a mut crate::window_output::WindowOutputEmitter,
    evaluator: &'a mut Context,
) -> TextRowOutputRenderState<'a> {
    TextRowOutputRenderState::from_parts(
        TextWindowOutputTarget::from_builder(builder),
        output_emitter,
        evaluator,
    )
}

fn text_row_source_render_state<'a>(
    builder: &'a mut crate::output::builder::DisplayOutputBuilder,
    output_emitter: &'a mut crate::window_output::WindowOutputEmitter,
    evaluator: &'a mut Context,
    font_metrics: &'a mut Option<FontMetricsService>,
    face_resolver: &'a FaceResolver,
) -> TextRowSourceRenderState<'a> {
    TextRowSourceRenderState::from_output_render(
        text_row_output_render_state(builder, output_emitter, evaluator),
        font_metrics,
        DisplayRowMeasurementMode::LogicalCells,
        crate::display_row::face_environment::FrameFaces::new(face_resolver)
            .unremapped_window_for_test(),
    )
}

fn text_row_source_measure_state<'a>(
    builder: &'a mut crate::output::builder::DisplayOutputBuilder,
    evaluator: &'a mut Context,
    font_metrics: &'a mut Option<FontMetricsService>,
    face_resolver: &'a FaceResolver,
) -> TextRowSourceMeasureState<'a> {
    TextRowSourceMeasureState::from_current_row(
        DisplayRowCurrentRowOutput::from_output_builder(builder),
        evaluator,
        font_metrics,
        face_resolver,
    )
}

fn write_char_to_current_row_with_width(
    builder: &mut crate::output::builder::DisplayOutputBuilder,
    ch: char,
    face_id: FaceId,
    charpos: usize,
    pixel_width: f32,
) {
    builder
        .edit_current_row_for_test(|row| {
            crate::glyph_row_writer::push_char_to_row(row, ch, face_id, charpos, pixel_width);
        })
        .expect("current row");
}

fn buffer_display_item(
    buffer_id: BufferId,
    start: usize,
    end: usize,
    face: RenderFaceRef,
    kind: DisplayItemKind,
) -> crate::display_item::DisplayItem {
    crate::display_item::DisplayItem::new(
        crate::display_item::SourceSpan::new(
            DisplaySourcePosition::buffer(
                buffer_id,
                CharPos0::new(start),
                EmacsBytePos::new(start),
            ),
            DisplaySourcePosition::buffer(buffer_id, CharPos0::new(end), EmacsBytePos::new(end)),
        ),
        face,
        kind,
    )
}

fn buffer_source_mapped_display_item(
    buffer_id: BufferId,
    start: usize,
    end: usize,
    text: &str,
    face: RenderFaceRef,
) -> crate::display_item::DisplayItem {
    buffer_display_item(
        buffer_id,
        start,
        end,
        face,
        DisplayItemKind::SourceMappedText(DisplaySourceMappedText::new(text)),
    )
}

fn buffer_special_request_display_item(
    request: &DisplaySpecialSourceCharRequest,
) -> crate::display_item::DisplayItem {
    let source_item = request.source_item_request();
    let range = source_item.range();
    buffer_display_item(
        BufferId(7),
        range.start().get(),
        range.end().get(),
        RenderFaceRef::Inherit,
        source_item.into_display_item_kind(),
    )
}

fn emitted_row(
    row: i64,
    y: i64,
    height: i64,
    start_lisp: i64,
    end_lisp: i64,
) -> neovm_core::window::DisplayRowSnapshot {
    neovm_core::window::DisplayRowSnapshot {
        row,
        y,
        height,
        start_x: 0,
        start_col: 0,
        end_x: 0,
        end_col: 0,
        start_buffer_pos: Some(LispCharPos1::new(start_lisp)),
        end_buffer_pos: Some(LispCharPos1::new(end_lisp)),
        fringe: Default::default(),
    }
}

struct RecordingAppendImageHost {
    requests: Arc<Mutex<Vec<ImageResolveRequest>>>,
}

struct RowTransitionTestContext {
    eval: Context,
    output_emitter: crate::window_output::WindowOutputEmitter,
    builder: crate::output::builder::DisplayOutputBuilder,
    defaults: DisplayRowGeometryDefaults,
    geometry: DisplayRowGeometryState,
    row_y_positions: DisplayRowYPositions,
    hit_rows: Vec<crate::hit_test::HitRow>,
    row_flags: DisplayRowFlags,
    row_limit: DisplayRowLimit,
}

impl RowTransitionTestContext {
    fn new(frame_name: &str) -> Self {
        let mut eval = Context::new();
        let buf_id = eval
            .buffer_manager()
            .current_buffer()
            .expect("current buffer")
            .id();
        let frame_id = eval
            .frame_manager_mut()
            .create_frame(frame_name, 320, 120, buf_id);
        let window_id = eval
            .frame_manager()
            .get(frame_id)
            .expect("frame")
            .selected_window;
        let mut output_emitter =
            crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
        output_emitter.begin_update(&mut eval);
        output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);

        let mut builder = crate::output::builder::DisplayOutputBuilder::new();
        builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 48.0), true);
        builder.begin_row(0, GlyphRowRole::Text);
        let defaults = DisplayRowGeometryDefaults::new(
            0.0,
            16.0,
            12.0,
            DisplayRowMeasurementMode::ConcreteFont,
        );
        let geometry = defaults.initial_state();
        let max_rows = 4;

        Self {
            eval,
            output_emitter,
            builder,
            defaults,
            geometry,
            row_y_positions: DisplayRowYPositions::with_capacity_and_first_row(max_rows, 0.0),
            hit_rows: Vec::new(),
            row_flags: DisplayRowFlags::new(max_rows),
            row_limit: DisplayRowLimit { max_rows },
        }
    }
}

#[test]
fn buffer_line_number_text_prefix_renders_and_consumes_pending_request() {
    let mut context = RowTransitionTestContext::new("line-number-text-prefix-render-request");
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(7);
    let mut line_numbers = LineNumberRenderState::new(true, 12, 9);
    let mut face_scan = FaceScanCheckpoint::initial();
    let mut font_metrics = None;
    *face_scan.next_check_mut() = 99;

    {
        let mut source_render = text_row_source_render_state(
            &mut context.builder,
            &mut context.output_emitter,
            &mut context.eval,
            &mut font_metrics,
            &face_resolver,
        );
        assert!(
            BufferLineNumberTextPrefixRenderRequest::new(
                DisplayLineNumbersMode::Absolute,
                false,
                0,
                4,
                crate::display_row::walk_state::LineNumberFieldLayout::new(4, 21.0),
            )
            .render_pending_with_source_state(
                &mut line_numbers,
                &mut source_render,
                &mut face_ids,
                &mut context.geometry,
                &mut face_scan,
            )
        );
    }

    context.builder.end_row();
    context.builder.end_window();
    let state = context.builder.finish(20, 1, 8.0, 16.0);
    let prefix = &state.window_matrices[0].matrix.rows[0].glyphs[GlyphArea::Text as usize];

    assert_eq!(prefix.len(), 4);
    assert_eq!(prefix[0].glyph_type, GlyphType::Char { ch: ' ' });
    assert_eq!(prefix[1].glyph_type, GlyphType::Char { ch: '1' });
    assert_eq!(prefix[2].glyph_type, GlyphType::Char { ch: '2' });
    assert_eq!(prefix[3].glyph_type, GlyphType::Char { ch: ' ' });
    assert_eq!(
        prefix.iter().map(|glyph| glyph.pixel_width).sum::<f32>(),
        84.0,
        "the TEXT_AREA prefix must consume the measured face's four-column extent"
    );
    assert!(
        prefix
            .iter()
            .all(|glyph| glyph.face_id == FaceId::new(BasicFaceId::SENTINEL))
    );
    assert_eq!(face_scan, FaceScanCheckpoint::initial());
    assert!(!line_numbers.should_render());
}

#[test]
fn buffer_line_number_text_prefix_renders_blank_field_on_continuation_row() {
    // GNU `maybe_produce_line_number` reserves a blank, width-matched gutter on
    // each wrapped continuation row so its text aligns with the first row.
    let mut context = RowTransitionTestContext::new("line-number-prefix-continuation-row");
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(7);
    let mut line_numbers = LineNumberRenderState::new(true, 12, 9);
    let mut face_scan = FaceScanCheckpoint::initial();
    let mut font_metrics = None;

    // A continuation transition replaces the first-row request with a blank one.
    line_numbers.mark_continuation_row();
    assert!(line_numbers.should_render());

    {
        let mut source_render = text_row_source_render_state(
            &mut context.builder,
            &mut context.output_emitter,
            &mut context.eval,
            &mut font_metrics,
            &face_resolver,
        );
        assert!(
            BufferLineNumberTextPrefixRenderRequest::new(
                DisplayLineNumbersMode::Absolute,
                false,
                0,
                4,
                crate::display_row::walk_state::LineNumberFieldLayout::new(4, 8.0),
            )
            .render_pending_with_source_state(
                &mut line_numbers,
                &mut source_render,
                &mut face_ids,
                &mut context.geometry,
                &mut face_scan,
            )
        );
    }

    context.builder.end_row();
    context.builder.end_window();
    let state = context.builder.finish(20, 1, 8.0, 16.0);
    let prefix = &state.window_matrices[0].matrix.rows[0].glyphs[GlyphArea::Text as usize];

    // No number glyphs: GNU's complete four-character field becomes four
    // face-backed blanks whose total is the same reserved extent as the
    // numbered first row.
    assert_eq!(prefix.len(), 4);
    assert!(
        prefix
            .iter()
            .all(|glyph| glyph.glyph_type == GlyphType::Char { ch: ' ' })
    );
    assert_eq!(
        prefix.iter().map(|glyph| glyph.pixel_width).sum::<f32>(),
        32.0
    );
    assert!(
        prefix
            .iter()
            .all(|glyph| glyph.face_id == FaceId::new(BasicFaceId::SENTINEL))
    );
    assert!(!line_numbers.should_render());
}

impl DisplayHost for RecordingAppendImageHost {
    fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resolve_image_sync(
        &self,
        _request: ImageResolveRequest,
    ) -> Result<Option<ReadyImage>, String> {
        panic!("append display source rendering must not use synchronous image resolution");
    }

    fn image_catalog(&self) -> Option<&dyn ImageCatalog> {
        Some(self)
    }
}

impl ImageCatalog for RecordingAppendImageHost {
    fn lookup(&self, request: ImageResolveRequest) -> ImageLookup {
        self.requests
            .lock()
            .expect("image requests lock")
            .push(request);
        ImageLookup::Pending(PendingImage::new(
            test_image_load(42),
            neomacs_display_protocol::ImageLayoutExtent::new(64, 32),
        ))
    }
}

#[test]
fn display_row_append_metrics_builds_from_measured_face_metrics() {
    let metrics = DisplayRowAppendMetrics::from_measured_face_metrics(
        DisplayRowMeasuredFaceMetrics::new(7.5, 18.0, 13.0, 8.0),
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
    );

    assert_eq!(
        metrics,
        DisplayRowAppendMetrics::new(
            18.0,
            13.0,
            7.5,
            8.0,
            DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0)
        )
    );
}

#[test]
fn display_row_append_metrics_builds_from_active_face_state() {
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base = resolver.default_face().clone();
    let mut font_metrics = None;
    let measured = DisplayRowMeasurementPolicy::for_mode(DisplayRowMeasurementMode::LogicalCells)
        .measured_face(
            FaceId::new(7),
            &base,
            None,
            7.5,
            DisplayRowFallbackMetrics {
                char_width: 7.5,
                row_height: 18.0,
                ascent: 13.0,
            },
            &mut font_metrics,
        );
    let active_face = DisplayRowActiveFaceState::new(base, measured);

    let metrics = DisplayRowAppendMetrics::from_active_face_state(
        &active_face,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
    );

    assert_eq!(
        metrics,
        DisplayRowAppendMetrics::new(
            18.0,
            13.0,
            7.5,
            8.0,
            DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0)
        )
    );
}

#[test]
fn display_row_append_metrics_builds_display_box_from_active_face_state() {
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base = resolver.default_face().clone();
    let mut font_metrics = None;
    let measured = DisplayRowMeasurementPolicy::for_mode(DisplayRowMeasurementMode::LogicalCells)
        .measured_face(
            FaceId::new(7),
            &base,
            None,
            7.5,
            DisplayRowFallbackMetrics {
                char_width: 7.5,
                row_height: 18.0,
                ascent: 13.0,
            },
            &mut font_metrics,
        );
    let active_face = DisplayRowActiveFaceState::new(base, measured);

    let metrics = DisplayRowAppendMetrics::display_box_from_active_face_state(
        &active_face,
        42.0,
        31.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
    );

    assert_eq!(
        metrics,
        DisplayRowAppendMetrics::new(
            42.0,
            31.0,
            7.5,
            8.0,
            DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0)
        )
    );
}

#[test]
fn buffer_current_face_resolution_context_skips_before_checkpoint() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let buffer = current_buffer_snapshot(&eval, buf_id);
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let default_face = face_resolver.default_face().clone();
    let mut font_metrics = None;
    let measurement_policy =
        DisplayRowMeasurementPolicy::for_mode(DisplayRowMeasurementMode::LogicalCells);
    let measured = measurement_policy.measured_face(
        FaceId::new(7),
        &default_face,
        None,
        8.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        &mut font_metrics,
    );
    let mut active_face = DisplayRowActiveFaceState::new(default_face.clone(), measured);
    let mut face_scan = FaceScanCheckpoint::initial();
    *face_scan.next_check_mut() = 99;
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("face-resolution-not-due", 80, 40, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    let mut row_geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let mut row_extend = DisplayRowScopedValue::inactive();
    let mut box_face = BoxFaceRowState::inactive();
    let mut source_render = text_row_source_render_state(
        &mut builder,
        &mut output_emitter,
        &mut eval,
        &mut font_metrics,
        &face_resolver,
    );

    let resolved = BufferSourceFaceResolutionContext::new(
        &buffer,
        &face_resolver,
        measurement_policy,
        &default_face,
        BasicFaceId::Default.into(),
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        neovm_core::emacs_core::image_catalog::ImageScaleEnvironment::default(),
    )
    .resolve_at_checkpoint(
        &mut BufferSourceFaceResolutionState::new(
            &mut source_render,
            &mut face_scan,
            &mut face_ids,
            &mut active_face,
            &mut row_geometry,
            &mut row_extend,
            &mut box_face,
            0.0,
        ),
        1,
    );

    assert!(!resolved);
    assert_eq!(active_face.face_id(), FaceId::new(7));
    assert_eq!(face_ids.reserve_dynamic_face(), FaceId::new(20));
}

#[test]
fn buffer_current_face_resolution_context_resolves_due_face() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let buffer = current_buffer_snapshot(&eval, buf_id);
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let default_face = face_resolver.default_face().clone();
    let mut font_metrics = None;
    let measurement_policy =
        DisplayRowMeasurementPolicy::for_mode(DisplayRowMeasurementMode::LogicalCells);
    let measured = measurement_policy.measured_face(
        FaceId::new(7),
        &default_face,
        None,
        8.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        &mut font_metrics,
    );
    let mut active_face = DisplayRowActiveFaceState::new(default_face.clone(), measured);
    let mut face_scan = FaceScanCheckpoint::initial();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("face-resolution-due", 80, 40, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    let mut row_geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 8.0, 6.0);
    let mut row_extend = DisplayRowScopedValue::inactive();
    let mut box_face = BoxFaceRowState::inactive();
    let mut source_render = text_row_source_render_state(
        &mut builder,
        &mut output_emitter,
        &mut eval,
        &mut font_metrics,
        &face_resolver,
    );

    let resolved = BufferSourceFaceResolutionContext::new(
        &buffer,
        &face_resolver,
        measurement_policy,
        &default_face,
        BasicFaceId::Default.into(),
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        neovm_core::emacs_core::image_catalog::ImageScaleEnvironment::default(),
    )
    .resolve_at_checkpoint(
        &mut BufferSourceFaceResolutionState::new(
            &mut source_render,
            &mut face_scan,
            &mut face_ids,
            &mut active_face,
            &mut row_geometry,
            &mut row_extend,
            &mut box_face,
            4.0,
        ),
        0,
    );

    assert!(resolved);
    assert_eq!(active_face.face_id(), FaceId::new(20));
    assert_eq!(active_face.metrics().row_height(), 16.0);
    assert_eq!(row_geometry.height(), 16.0);
}

#[test]
fn display_row_boundary_transition_request_records_hit_and_emits_next_row() {
    let mut ctx = RowTransitionTestContext::new("boundary-transition-request");

    let transition = DisplayRowBoundaryTransitionRequest::new(
        DisplayRowBoundaryTarget::visual_wrap(
            DisplayRowHitRange {
                charpos_start: 3,
                charpos_end: 9,
            },
            ctx.defaults,
            0,
            6,
            48.0,
            ctx.row_y_positions.recording(),
        ),
        4,
    )
    .emit_with_output(
        &mut ctx.geometry,
        &mut ctx.hit_rows,
        text_row_output_render_state(&mut ctx.builder, &mut ctx.output_emitter, &mut ctx.eval),
    );

    assert_eq!(transition, DisplayTextRowTransition::BeganNextRow);
    assert_eq!(ctx.geometry.row(), 1);
    assert_eq!(ctx.hit_rows.len(), 1);
    assert_eq!(ctx.hit_rows[0].charpos_start, 3);
    assert_eq!(ctx.hit_rows[0].charpos_end, 9);
    assert_eq!(ctx.row_y_positions.recorded(), &[0.0, 16.0]);
}

#[test]
fn display_row_line_break_transition_request_records_hit_spacing_and_emits_next_row() {
    let mut ctx = RowTransitionTestContext::new("line-break-transition-request");

    let transition = DisplayRowLineBreakTransitionRequest::new(
        DisplayRowHitRange {
            charpos_start: 3,
            charpos_end: 9,
        },
        ctx.defaults,
        0,
        6,
        48.0,
        4.0,
        ctx.row_y_positions.recording(),
        4,
    )
    .emit_with_output(
        &mut ctx.geometry,
        &mut ctx.hit_rows,
        text_row_output_render_state(&mut ctx.builder, &mut ctx.output_emitter, &mut ctx.eval),
    );

    assert_eq!(transition, DisplayTextRowTransition::BeganNextRow);
    assert_eq!(ctx.geometry.row(), 1);
    assert_eq!(ctx.hit_rows.len(), 1);
    assert_eq!(ctx.hit_rows[0].charpos_start, 3);
    assert_eq!(ctx.hit_rows[0].charpos_end, 9);
    assert_eq!(ctx.row_y_positions.recorded(), &[0.0, 20.0]);
}

#[test]
fn display_row_transition_request_context_builds_line_break_and_overflow_requests() {
    let mut line_ctx = RowTransitionTestContext::new("transition-context-line-break");

    let transition = DisplayRowTransitionRequestContext::new(
        line_ctx.defaults,
        0,
        line_ctx.row_y_positions.recording(),
        4,
    )
    .line_break(
        DisplayRowLineBreakTransitionPlan::line_break(),
        DisplayRowHitRange {
            charpos_start: 3,
            charpos_end: 9,
        },
        DisplayRowPosition::new(48.0, 6),
        4.0,
    )
    .emit_with_output(
        &mut line_ctx.geometry,
        &mut line_ctx.hit_rows,
        text_row_output_render_state(
            &mut line_ctx.builder,
            &mut line_ctx.output_emitter,
            &mut line_ctx.eval,
        ),
    );

    assert_eq!(transition, DisplayTextRowTransition::BeganNextRow);
    assert_eq!(line_ctx.geometry.row(), 1);
    assert_eq!(line_ctx.hit_rows.len(), 1);
    assert_eq!(line_ctx.hit_rows[0].charpos_start, 3);
    assert_eq!(line_ctx.hit_rows[0].charpos_end, 9);
    assert_eq!(line_ctx.row_y_positions.recorded(), &[0.0, 20.0]);

    let mut wrap_ctx = RowTransitionTestContext::new("transition-context-overflow");
    let DisplaySourceTextCharOverflowAction::CharacterWrap { transition } =
        DisplaySourceTextCharOverflowAction::for_decision(
            DisplayRowTextOverflowDecision::CharacterWrap,
        )
    else {
        panic!("expected character wrap transition");
    };

    let transition = DisplayRowTransitionRequestContext::new(
        wrap_ctx.defaults,
        0,
        wrap_ctx.row_y_positions.recording(),
        4,
    )
    .overflow(
        transition,
        DisplayRowHitRange {
            charpos_start: 4,
            charpos_end: 10,
        },
        DisplayRowPosition::new(56.0, 7),
    )
    .emit_with_output(
        &mut wrap_ctx.geometry,
        &mut wrap_ctx.row_flags,
        wrap_ctx.row_limit,
        &mut wrap_ctx.hit_rows,
        text_row_output_render_state(
            &mut wrap_ctx.builder,
            &mut wrap_ctx.output_emitter,
            &mut wrap_ctx.eval,
        ),
    );

    assert_eq!(transition, DisplayTextRowTransition::BeganNextRow);
    assert_eq!(wrap_ctx.geometry.row(), 1);
    assert_eq!(wrap_ctx.hit_rows.len(), 1);
    assert_eq!(wrap_ctx.hit_rows[0].charpos_start, 4);
    assert_eq!(wrap_ctx.hit_rows[0].charpos_end, 10);
    assert!(wrap_ctx.row_flags.is_set(0, DisplayRowFlagKind::Continued));
    assert!(
        wrap_ctx
            .row_flags
            .is_set(1, DisplayRowFlagKind::Continuation)
    );
    assert_eq!(wrap_ctx.row_y_positions.recorded(), &[0.0, 16.0]);
}

#[test]
fn display_row_text_window_transition_context_emits_line_break_and_overflow() {
    let mut line_ctx = RowTransitionTestContext::new("text-window-transition-line-break");

    let row_limit = line_ctx.row_limit;
    let transition = DisplayRowTextWindowEmitContext::new(
        line_ctx.defaults,
        0,
        &mut line_ctx.row_y_positions,
        4,
        &mut line_ctx.geometry,
        &mut line_ctx.row_flags,
        row_limit,
        &mut line_ctx.hit_rows,
        text_row_output_render_state(
            &mut line_ctx.builder,
            &mut line_ctx.output_emitter,
            &mut line_ctx.eval,
        ),
    )
    .emit_line_break(
        DisplayRowLineBreakTransitionPlan::line_break(),
        DisplayRowHitRange {
            charpos_start: 1,
            charpos_end: 5,
        },
        DisplayRowPosition::new(32.0, 4),
        2.0,
    );

    assert_eq!(transition, DisplayTextRowTransition::BeganNextRow);
    assert_eq!(line_ctx.geometry.row(), 1);
    assert_eq!(line_ctx.hit_rows.len(), 1);
    assert_eq!(line_ctx.hit_rows[0].charpos_start, 1);
    assert_eq!(line_ctx.hit_rows[0].charpos_end, 5);
    assert_eq!(line_ctx.row_y_positions.recorded(), &[0.0, 18.0]);

    let mut overflow_ctx = RowTransitionTestContext::new("text-window-transition-overflow");
    let DisplaySourceTextCharOverflowAction::CharacterWrap { transition } =
        DisplaySourceTextCharOverflowAction::for_decision(
            DisplayRowTextOverflowDecision::CharacterWrap,
        )
    else {
        panic!("expected character wrap transition");
    };

    let row_limit = overflow_ctx.row_limit;
    let transition = DisplayRowTextWindowEmitContext::new(
        overflow_ctx.defaults,
        0,
        &mut overflow_ctx.row_y_positions,
        4,
        &mut overflow_ctx.geometry,
        &mut overflow_ctx.row_flags,
        row_limit,
        &mut overflow_ctx.hit_rows,
        text_row_output_render_state(
            &mut overflow_ctx.builder,
            &mut overflow_ctx.output_emitter,
            &mut overflow_ctx.eval,
        ),
    )
    .emit_overflow(
        transition,
        DisplayRowHitRange {
            charpos_start: 2,
            charpos_end: 8,
        },
        DisplayRowPosition::new(64.0, 8),
    );

    assert_eq!(transition, DisplayTextRowTransition::BeganNextRow);
    assert_eq!(overflow_ctx.geometry.row(), 1);
    assert_eq!(overflow_ctx.hit_rows.len(), 1);
    assert_eq!(overflow_ctx.hit_rows[0].charpos_start, 2);
    assert_eq!(overflow_ctx.hit_rows[0].charpos_end, 8);
    assert!(
        overflow_ctx
            .row_flags
            .is_set(0, DisplayRowFlagKind::Continued)
    );
    assert!(
        overflow_ctx
            .row_flags
            .is_set(1, DisplayRowFlagKind::Continuation)
    );
    assert_eq!(overflow_ctx.row_y_positions.recorded(), &[0.0, 16.0]);
}

#[test]
fn display_row_text_window_emit_context_applies_line_break_render_state_after_transition() {
    let mut ctx = RowTransitionTestContext::new("text-window-transition-line-state");
    let mut prefix_request = DisplayRowPrefixRequest::None;
    let mut line_numbers = LineNumberRenderState::new(true, 4, 9);
    let mut hscroll_skip = HorizontalScrollSkipState::new(
        LineWrapMode::Truncate,
        4,
        HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph,
    );
    hscroll_skip.consume_display_item(HorizontalScrollDisplayItem::tab(5));
    let mut word_wrap = WordWrapRenderState::new(true);
    word_wrap.allow_after_current_char(' ');
    let mut trailing_whitespace = TrailingWhitespaceRenderState::new(true, 0x00ff00);
    trailing_whitespace.track_rendered_char(' ', ctx.geometry.start_marker_at_x(8.0));
    let mut col = 6;

    let row_limit = ctx.row_limit;
    let transition = DisplayRowTextWindowEmitContext::new(
        ctx.defaults,
        0,
        &mut ctx.row_y_positions,
        4,
        &mut ctx.geometry,
        &mut ctx.row_flags,
        row_limit,
        &mut ctx.hit_rows,
        text_row_output_render_state(&mut ctx.builder, &mut ctx.output_emitter, &mut ctx.eval),
    )
    .emit_line_break_then_row_start(
        DisplayRowLineBreakTransitionPlan::hidden_line_break(),
        DisplayRowHitRange {
            charpos_start: 1,
            charpos_end: 5,
        },
        DisplayRowPosition::new(32.0, col),
        2.0,
        DisplayRowTransitionRenderState::new(
            &mut prefix_request,
            true,
            &mut line_numbers,
            &mut hscroll_skip,
            &mut word_wrap,
            &mut trailing_whitespace,
        ),
        &mut col,
    );

    assert_eq!(transition, DisplayTextRowTransition::BeganNextRow);
    assert_eq!(col, 0);
    assert_eq!(prefix_request, DisplayRowPrefixRequest::Line);
    assert_eq!(line_numbers.current_line(), 5);
    assert!(hscroll_skip.should_skip());
    assert_eq!(
        trailing_whitespace.start_marker(),
        DisplayRowStartMarker::Inactive
    );
}

#[test]
fn display_row_text_window_emit_context_applies_overflow_render_state_after_transition() {
    let mut ctx = RowTransitionTestContext::new("text-window-transition-overflow-state");
    let mut prefix_request = DisplayRowPrefixRequest::None;
    let mut line_numbers = LineNumberRenderState::new(true, 4, 9);
    let mut hscroll_skip = HorizontalScrollSkipState::new(
        LineWrapMode::Wrap,
        0,
        HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph,
    );
    let mut word_wrap = WordWrapRenderState::new(true);
    word_wrap.allow_after_current_char(' ');
    let mut trailing_whitespace = TrailingWhitespaceRenderState::new(true, 0x00ff00);
    trailing_whitespace.track_rendered_char(' ', ctx.geometry.start_marker_at_x(8.0));
    let mut col = 6;
    let DisplaySourceTextCharOverflowAction::CharacterWrap { transition } =
        DisplaySourceTextCharOverflowAction::for_decision(
            DisplayRowTextOverflowDecision::CharacterWrap,
        )
    else {
        panic!("expected character wrap transition");
    };

    let row_limit = ctx.row_limit;
    let row_transition = DisplayRowTextWindowEmitContext::new(
        ctx.defaults,
        0,
        &mut ctx.row_y_positions,
        4,
        &mut ctx.geometry,
        &mut ctx.row_flags,
        row_limit,
        &mut ctx.hit_rows,
        text_row_output_render_state(&mut ctx.builder, &mut ctx.output_emitter, &mut ctx.eval),
    )
    .emit_overflow_then_row_start(
        transition,
        DisplayRowHitRange {
            charpos_start: 2,
            charpos_end: 8,
        },
        DisplayRowPosition::new(64.0, col),
        DisplayRowTransitionRenderState::new(
            &mut prefix_request,
            true,
            &mut line_numbers,
            &mut hscroll_skip,
            &mut word_wrap,
            &mut trailing_whitespace,
        ),
        &mut col,
    );

    assert_eq!(row_transition, DisplayTextRowTransition::BeganNextRow);
    assert_eq!(col, 0);
    assert_eq!(prefix_request, DisplayRowPrefixRequest::Wrap);
    assert_eq!(line_numbers.current_line(), 4);
    assert!(!hscroll_skip.should_skip());
    assert!(!word_wrap.has_candidate());
    assert_eq!(
        trailing_whitespace.start_marker(),
        DisplayRowStartMarker::Inactive
    );
}

#[test]
fn display_row_transition_render_state_applies_row_start_line_break_policy() {
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let mut prefix_request = DisplayRowPrefixRequest::None;
    let mut line_numbers = LineNumberRenderState::new(true, 4, 9);
    let mut hscroll_skip = HorizontalScrollSkipState::new(
        LineWrapMode::Truncate,
        4,
        HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph,
    );
    hscroll_skip.consume_display_item(HorizontalScrollDisplayItem::tab(5));
    let mut word_wrap = WordWrapRenderState::new(true);
    word_wrap.allow_after_current_char(' ');
    word_wrap.record_candidate(
        'a',
        0,
        4,
        2,
        (Some(LispCharPos1::new(1)), Some(LispCharPos1::new(1))),
        DisplayRowGlyphCheckpoint::default(),
    );
    let mut trailing_whitespace = TrailingWhitespaceRenderState::new(true, 0x00ff00);
    trailing_whitespace.track_rendered_char(' ', geometry.start_marker_at_x(8.0));
    let mut col = 7;

    DisplayRowTransitionRenderState::new(
        &mut prefix_request,
        true,
        &mut line_numbers,
        &mut hscroll_skip,
        &mut word_wrap,
        &mut trailing_whitespace,
    )
    .apply_line_break_row_start(
        DisplayRowLineBreakTransitionPlan::hidden_line_break(),
        &mut col,
    );

    assert_eq!(col, 0);
    assert_eq!(prefix_request, DisplayRowPrefixRequest::Line);
    assert_eq!(line_numbers.current_line(), 5);
    assert!(hscroll_skip.should_skip());
    assert!(!word_wrap.has_candidate());
    assert_eq!(
        trailing_whitespace.start_marker(),
        DisplayRowStartMarker::Inactive
    );
}

#[test]
fn buffer_hscroll_skip_preserves_line_break_action() {
    let mut position = DisplaySourceTextPosition::new(0, 10);
    let mut hscroll_skip = HorizontalScrollSkipState::new(
        LineWrapMode::Truncate,
        4,
        HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph,
    );

    let action = consume_hscroll_skip_from_position(b"\nnext", &mut position, &mut hscroll_skip, 8)
        .expect("hscroll skip action");

    assert_eq!(
        action,
        BufferSourceHscrollSkipAction::LineBreak {
            source_char: DisplaySourceStepChar::new('\n', 0, 10),
        }
    );
    assert_eq!(position, DisplaySourceTextPosition::new(1, 11));
    assert!(hscroll_skip.should_skip());
}

#[test]
fn buffer_hscroll_skip_consumes_tab_to_next_stop() {
    let mut position = DisplaySourceTextPosition::new(0, 0);
    let mut hscroll_skip = HorizontalScrollSkipState::new(
        LineWrapMode::Truncate,
        4,
        HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph,
    );

    let action = consume_hscroll_skip_from_position(b"\tabc", &mut position, &mut hscroll_skip, 8)
        .expect("hscroll skip action");

    assert_eq!(
        action,
        BufferSourceHscrollSkipAction::Text {
            source_char: DisplaySourceStepChar::new('\t', 0, 0),
            disposition: HscrollConsumedTextDisposition::InstallLeftTruncation {
                target: HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph,
                visible_remainder: HorizontalScrollVisibleRemainder::BlankColumns(3),
            }
        }
    );
    assert_eq!(position, DisplaySourceTextPosition::new(1, 1));
    assert!(!hscroll_skip.should_skip());
}

#[test]
fn buffer_hscroll_skip_consumes_wide_char_columns() {
    let mut position = DisplaySourceTextPosition::new(0, 3);
    let mut hscroll_skip = HorizontalScrollSkipState::new(
        LineWrapMode::Truncate,
        2,
        HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph,
    );

    let action =
        consume_hscroll_skip_from_position("界x".as_bytes(), &mut position, &mut hscroll_skip, 8)
            .expect("hscroll skip action");

    assert_eq!(
        action,
        BufferSourceHscrollSkipAction::Text {
            source_char: DisplaySourceStepChar::new('界', 0, 3),
            disposition: HscrollConsumedTextDisposition::Hidden
        }
    );
    assert_eq!(position, DisplaySourceTextPosition::new("界".len(), 4));
    assert!(hscroll_skip.should_skip());

    let action =
        consume_hscroll_skip_from_position("界x".as_bytes(), &mut position, &mut hscroll_skip, 8)
            .expect("left truncation replacement action");

    assert_eq!(
        action,
        BufferSourceHscrollSkipAction::Text {
            source_char: DisplaySourceStepChar::new('x', "界".len(), 4),
            disposition: HscrollConsumedTextDisposition::InstallLeftTruncation {
                target: HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph,
                visible_remainder: HorizontalScrollVisibleRemainder::None,
            }
        }
    );
    assert_eq!(position, DisplaySourceTextPosition::new("界x".len(), 5));
    assert!(!hscroll_skip.should_skip());
}

#[test]
fn buffer_hscroll_skip_keeps_marker_pending_while_still_skipping() {
    let mut position = DisplaySourceTextPosition::new(0, 0);
    let mut hscroll_skip = HorizontalScrollSkipState::new(
        LineWrapMode::Truncate,
        3,
        HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph,
    );

    let action = consume_hscroll_skip_from_position(b"abc", &mut position, &mut hscroll_skip, 8)
        .expect("hscroll skip action");

    assert_eq!(
        action,
        BufferSourceHscrollSkipAction::Text {
            source_char: DisplaySourceStepChar::new('a', 0, 0),
            disposition: HscrollConsumedTextDisposition::Hidden
        }
    );
    assert_eq!(position, DisplaySourceTextPosition::new(1, 1));
    assert!(hscroll_skip.should_skip());
}

#[test]
fn buffer_hscroll_skip_render_request_appends_left_truncation_marker() {
    let mut context = RowTransitionTestContext::new("hscroll-render-request-marker");
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let mut byte_idx = 0;
    let mut charpos = 0;
    let mut hscroll_skip = HorizontalScrollSkipState::new(
        LineWrapMode::Truncate,
        4,
        HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph,
    );
    let mut row_extend = DisplayRowScopedValue::inactive();
    let mut x = 0.0;
    let mut col = 0;
    let mut prefix_request = DisplayRowPrefixRequest::None;
    let mut line_numbers = LineNumberRenderState::new(false, 0, 0);
    let mut word_wrap = WordWrapRenderState::new(false);
    let mut trailing_whitespace = TrailingWhitespaceRenderState::new(false, 0);
    let mut invisible_text_checkpoint = InvisibleTextScanCheckpoint::new(charpos);
    let mut box_face = BoxFaceRowState::inactive();
    let mut face_scan = FaceScanCheckpoint::initial();
    let mut hit_row_range = HitRowRangeTracker::new(0);
    let mut cursor_info = CursorCaptureState::new();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let overlay_context = BufferOverlayStringTextRowRenderContext::new(
        false,
        1,
        &surface,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        0.0,
        0,
        4,
    );
    let mut font_metrics = None;
    let row_limit = context.row_limit;
    let buf_id = context
        .eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let snapshot = current_buffer_snapshot(&context.eval, buf_id);
    let mut source_walk = BufferSourceWalk::new(buf_id, &snapshot, 0, 0);

    let continuation = BufferSourceHscrollSkipRenderContext::new(
        b"\tabc",
        8,
        0.0,
        &surface,
        &active_face,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        99,
        false,
        context.defaults,
        0,
        4,
        row_limit,
    )
    .render_next_and_apply(
        &mut source_walk,
        BufferSourceLoopMutableState::new(
            &mut invisible_text_checkpoint,
            DisplaySourceProgressState::new(&mut byte_idx, &mut charpos, &mut x, &mut col),
            text_row_source_render_state(
                &mut context.builder,
                &mut context.output_emitter,
                &mut context.eval,
                &mut font_metrics,
                &face_resolver,
            ),
            BufferSourceRowBuildState::new(
                &mut context.geometry,
                &mut context.row_flags,
                &mut row_extend,
                &mut box_face,
            ),
            BufferSourceHitCaptureState::new(&mut context.hit_rows, &mut hit_row_range),
            BufferSourceRowCarryoverState::new(
                &mut prefix_request,
                &mut line_numbers,
                &mut hscroll_skip,
                &mut word_wrap,
                &mut trailing_whitespace,
            ),
            &mut face_scan,
            &mut context.row_y_positions,
            &mut cursor_info,
            &mut face_ids,
            BufferSourceSurfaceContext::new(&surface, overlay_context),
        ),
    );

    assert_eq!(continuation, DisplayRowTransitionContinuation::Continue);
    assert_eq!(byte_idx, 1);
    assert_eq!(charpos, 1);
    assert!(!hscroll_skip.should_skip());
    assert_eq!(x, 32.0);
    assert_eq!(col, 4);
    context
        .builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[GlyphArea::Text.index()];
            assert_eq!(text.len(), 2);
            assert!(matches!(text[0].glyph_type, GlyphType::Char { ch: '$' }));
            assert!(matches!(
                text[1].glyph_type,
                GlyphType::Stretch { width_cols: 3 }
            ));
            assert!(row.truncated_left);
        })
        .expect("current row");
}

#[test]
fn buffer_hscroll_skip_action_applies_line_break_transition_state() {
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let mut context = RowTransitionTestContext::new("hscroll-line-break-state");
    let action = BufferSourceHscrollSkipAction::LineBreak {
        source_char: DisplaySourceStepChar::new('\n', 3, 11),
    };
    let mut row_extend = DisplayRowScopedValue::inactive();
    row_extend.activate(
        geometry.current_row_marker(),
        test_row_extend_face(0x112233, FaceId::new(17)),
    );
    let mut x = 80.0;

    action.apply_line_break_before_row_transition(
        &mut row_extend,
        &mut context.output_emitter,
        &mut x,
        4.0,
    );

    assert_eq!(x, 4.0);
    assert_eq!(row_extend.value_on(&geometry), None);

    let mut hit_row_range = HitRowRangeTracker::new(7);
    let hit_range = action
        .line_break_hit_range(&mut hit_row_range)
        .expect("line break hit range");

    assert_eq!(hit_range.charpos_start, 7);
    assert_eq!(hit_range.charpos_end, 12);
    assert_eq!(hit_row_range.start(), 12);
}

#[test]
fn buffer_hscroll_skip_action_captures_line_break_cursor() {
    let active_face = test_active_face_state(FaceId::new(9), 8.0);
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let action = BufferSourceHscrollSkipAction::LineBreak {
        source_char: DisplaySourceStepChar::new('\n', 3, 11),
    };
    let mut cursor = CursorCaptureState::new();

    action.capture_line_break_cursor_if_point(
        &mut cursor,
        &active_face,
        &geometry,
        12,
        32.0,
        4,
        16.0,
    );

    let captured = cursor.as_ref().expect("cursor captured");
    assert_eq!(captured.x, 32.0);
    assert_eq!(captured.byte_idx, 3);
    assert_eq!(captured.col, 4);
    assert_eq!(captured.slot_width, Some(8.0));
}

#[test]
fn buffer_hscroll_skip_action_applies_after_line_break_transition() {
    let active_face = test_active_face_state(FaceId::new(9), 8.0);
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let action = BufferSourceHscrollSkipAction::LineBreak {
        source_char: DisplaySourceStepChar::new('\n', 3, 11),
    };
    let mut cursor = CursorCaptureState::new();

    let continuation = action.apply_after_line_break_row_transition(
        DisplayTextRowTransition::BeganNextRow,
        &mut cursor,
        &active_face,
        &geometry,
        12,
        32.0,
        4,
        16.0,
    );

    assert_eq!(continuation, DisplayRowTransitionContinuation::Continue);
    assert!(cursor.as_ref().is_some());
}

#[test]
fn buffer_hscroll_skip_action_skips_after_state_when_transition_exhausted() {
    let active_face = test_active_face_state(FaceId::new(9), 8.0);
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let action = BufferSourceHscrollSkipAction::LineBreak {
        source_char: DisplaySourceStepChar::new('\n', 3, 11),
    };
    let mut cursor = CursorCaptureState::new();

    let continuation = action.apply_after_line_break_row_transition(
        DisplayTextRowTransition::ExhaustedRows,
        &mut cursor,
        &active_face,
        &geometry,
        12,
        32.0,
        4,
        16.0,
    );

    assert_eq!(continuation, DisplayRowTransitionContinuation::Exhausted);
    assert!(cursor.as_ref().is_none());
}

#[test]
fn buffer_hscroll_skip_action_captures_text_cursor() {
    let active_face = test_active_face_state(FaceId::new(9), 8.0);
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let action = BufferSourceHscrollSkipAction::Text {
        source_char: DisplaySourceStepChar::new('x', 5, 8),
        disposition: HscrollConsumedTextDisposition::Hidden,
    };
    let mut cursor = CursorCaptureState::new();

    action.capture_text_cursor_if_point(&mut cursor, &active_face, &geometry, 9, 24.0, 3);

    let captured = cursor.as_ref().expect("cursor captured");
    assert_eq!(captured.x, 24.0);
    assert_eq!(captured.byte_idx, 5);
    assert_eq!(captured.col, 3);
    assert_eq!(captured.slot_width, Some(8.0));
}

#[test]
fn buffer_hscroll_boundary_item_anchors_cursor_at_its_source_start() {
    let active_face = test_active_face_state(FaceId::new(9), 8.0);
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let action = BufferSourceHscrollSkipAction::Text {
        source_char: DisplaySourceStepChar::new('\t', 5, 8),
        disposition: HscrollConsumedTextDisposition::InstallLeftTruncation {
            target: HorizontalScrollTruncationTarget::LineNumberPrefix,
            visible_remainder: HorizontalScrollVisibleRemainder::BlankColumns(3),
        },
    };
    let mut cursor = CursorCaptureState::new();

    action.capture_text_cursor_if_point(&mut cursor, &active_face, &geometry, 8, 32.0, 4);

    let captured = cursor.as_ref().expect("boundary cursor captured");
    assert_eq!(captured.x, 32.0);
    assert_eq!(captured.byte_idx, 5);
    assert_eq!(captured.col, 4);
}

#[test]
fn buffer_hscroll_skip_action_appends_left_truncation_marker_and_marks_row() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("hscroll-left-truncation-marker", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);

    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let mut font_metrics = None;
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let mut x = 0.0;
    let mut col = 0;
    let mut source_render = text_row_source_render_state(
        &mut builder,
        &mut output_emitter,
        &mut eval,
        &mut font_metrics,
        &face_resolver,
    );
    let action = BufferSourceHscrollSkipAction::Text {
        source_char: DisplaySourceStepChar::new('x', 5, 8),
        disposition: HscrollConsumedTextDisposition::InstallLeftTruncation {
            target: HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph,
            visible_remainder: HorizontalScrollVisibleRemainder::None,
        },
    };

    action.append_left_truncation_marker_to_text_row_and_apply(
        BufferSyntheticTextRenderContext::new(
            &surface,
            &active_face,
            0.0,
            DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        ),
        &geometry,
        &mut source_render,
        DisplaySourceRowProgressState::new(&mut x, &mut col),
        0.0,
    );

    assert_eq!(x, 8.0);
    assert_eq!(col, 1);
    builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[GlyphArea::Text.index()];
            assert_eq!(text.len(), 1);
            assert!(matches!(text[0].glyph_type, GlyphType::Char { ch: '$' }));
            assert!(row.truncated_left);
        })
        .expect("current row");
}

#[test]
fn buffer_invisible_text_scan_context_skips_when_checkpoint_not_reached() {
    let buffer_text = b"visible";
    let mut checkpoints = InvisibleTextScanCheckpoint::new(5);
    let mut position = DisplaySourceTextPosition::new(2, 2);
    let eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let snapshot = current_buffer_snapshot(&eval, buf_id);

    let action = BufferSourceInvisibleTextScanContext::new(buffer_text, 7, 2, true)
        .consume_at_checkpoint(&snapshot, &mut checkpoints, &mut position);

    assert_eq!(action, BufferSourceInvisibleTextScanAction::Unchecked);
    assert_eq!(position, DisplaySourceTextPosition::new(2, 2));
}

#[test]
fn buffer_invisible_text_scan_context_records_visible_boundary() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.insert("visible hidden");
        let _ = eval
            .buffer_manager_mut()
            .put_buffer_text_property_in_emacs_byte_range(
                buf_id,
                EmacsByteRange::from_usize(8, 14),
                Value::symbol("invisible"),
                Value::T,
            );
    }
    let snapshot = current_buffer_snapshot(&eval, buf_id);
    let mut checkpoints = InvisibleTextScanCheckpoint::new(0);
    let mut position = DisplaySourceTextPosition::new(0, 0);

    let action =
        BufferSourceInvisibleTextScanContext::new("visible hidden".as_bytes(), 14, 0, true)
            .consume_at_checkpoint(&snapshot, &mut checkpoints, &mut position);

    assert_eq!(
        action,
        BufferSourceInvisibleTextScanAction::Visible { next_visible: 8 }
    );
    assert_eq!(position, DisplaySourceTextPosition::new(0, 0));
    assert!(!checkpoints.should_check(7));
    assert!(checkpoints.should_check(8));
}

#[test]
fn buffer_invisible_text_scan_context_skips_hidden_region_and_reports_point() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.insert("visible hidden visible");
        let _ = eval
            .buffer_manager_mut()
            .put_buffer_text_property_in_emacs_byte_range(
                buf_id,
                EmacsByteRange::from_usize(8, 14),
                Value::symbol("invisible"),
                Value::T,
            );
    }
    let snapshot = current_buffer_snapshot(&eval, buf_id);
    let mut checkpoints = InvisibleTextScanCheckpoint::new(8);
    let mut position = DisplaySourceTextPosition::new(8, 8);

    let action = BufferSourceInvisibleTextScanContext::new(
        "visible hidden visible".as_bytes(),
        22,
        10,
        true,
    )
    .consume_at_checkpoint(&snapshot, &mut checkpoints, &mut position);

    let BufferSourceInvisibleTextScanAction::Hidden(hidden) = action else {
        panic!("expected hidden region");
    };
    assert_eq!(hidden.start_byte_idx(), 8);
    assert_eq!(hidden.start_charpos(), 8);
    assert_eq!(hidden.skip_to(), 14);
    assert_eq!(hidden.next_visible(), 14);
    assert!(hidden.point_in_hidden_region());
    assert!(!hidden.ellipsis());
    assert_eq!(position, DisplaySourceTextPosition::new(14, 14));
    assert!(!checkpoints.should_check(13));
    assert!(checkpoints.should_check(14));
}

#[test]
fn buffer_invisible_text_scan_context_reports_ellipsis_policy() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.insert("folded rest");
        buffer.set_buffer_local(
            "buffer-invisibility-spec",
            Value::list(vec![Value::cons(Value::symbol("outline"), Value::T)]),
        );
        let _ = eval
            .buffer_manager_mut()
            .put_buffer_text_property_in_emacs_byte_range(
                buf_id,
                EmacsByteRange::from_usize(0, 6),
                Value::symbol("invisible"),
                Value::symbol("outline"),
            );
    }
    let snapshot = current_buffer_snapshot(&eval, buf_id);
    let mut checkpoints = InvisibleTextScanCheckpoint::new(0);
    let mut position = DisplaySourceTextPosition::new(0, 0);

    let action = BufferSourceInvisibleTextScanContext::new("folded rest".as_bytes(), 11, 9, true)
        .consume_at_checkpoint(&snapshot, &mut checkpoints, &mut position);

    let BufferSourceInvisibleTextScanAction::Hidden(hidden) = action else {
        panic!("expected hidden region");
    };
    assert_eq!(hidden.skip_to(), 6);
    assert!(!hidden.point_in_hidden_region());
    assert!(hidden.ellipsis());
    assert_eq!(position, DisplaySourceTextPosition::new(6, 6));
}

#[test]
fn buffer_invisible_text_scan_context_advances_multibyte_source_position() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.insert("a中b");
        let _ = eval
            .buffer_manager_mut()
            .put_buffer_text_property_in_emacs_byte_range(
                buf_id,
                EmacsByteRange::from_usize(1, "a中".len()),
                Value::symbol("invisible"),
                Value::T,
            );
    }
    let snapshot = current_buffer_snapshot(&eval, buf_id);
    let mut checkpoints = InvisibleTextScanCheckpoint::new(1);
    let mut position = DisplaySourceTextPosition::new(1, 1);

    let action = BufferSourceInvisibleTextScanContext::new("a中b".as_bytes(), 3, 1, true)
        .consume_at_checkpoint(&snapshot, &mut checkpoints, &mut position);

    let BufferSourceInvisibleTextScanAction::Hidden(hidden) = action else {
        panic!("expected hidden region");
    };
    assert_eq!(hidden.start_byte_idx(), 1);
    assert_eq!(hidden.start_charpos(), 1);
    assert_eq!(hidden.skip_to(), 2);
    assert_eq!(position, DisplaySourceTextPosition::new("a中".len(), 2));
    // No newline inside this fold.
    assert_eq!(hidden.hidden_newline_count(), 0);
}

#[test]
fn buffer_invisible_text_skip_counts_hidden_newlines() {
    // A fold that hides whole buffer lines must report the newlines it crossed,
    // so display-line-numbers advance past them (GNU counts every buffer newline,
    // visible or not — folding lines 2..4 makes the next visible row read 5).
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let text = "L1\nL2\nL3\nL4\nL5\n";
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.insert(text);
        // Hide "L2\nL3\nL4\n" (3 interior newlines) — the contiguous fold from
        // after "L1\n" up to the start of "L5".
        let start = text.find("L2").expect("L2");
        let end = text.find("L5").expect("L5");
        let _ = eval
            .buffer_manager_mut()
            .put_buffer_text_property_in_emacs_byte_range(
                buf_id,
                EmacsByteRange::from_usize(start, end),
                Value::symbol("invisible"),
                Value::T,
            );
    }
    let snapshot = current_buffer_snapshot(&eval, buf_id);
    let start = text.find("L2").expect("L2");
    let mut checkpoints = InvisibleTextScanCheckpoint::new(start as i64);
    let mut position = DisplaySourceTextPosition::new(start, start as i64);

    let action =
        BufferSourceInvisibleTextScanContext::new(text.as_bytes(), text.len() as i64, 0, true)
            .consume_at_checkpoint(&snapshot, &mut checkpoints, &mut position);

    let BufferSourceInvisibleTextScanAction::Hidden(hidden) = action else {
        panic!("expected hidden region");
    };
    // "L2\nL3\nL4\n" contains exactly three '\n'.
    assert_eq!(
        hidden.hidden_newline_count(),
        3,
        "fold over L2..L5 hides 3 buffer newlines"
    );
}

#[test]
fn invisible_text_skip_advances_line_numbers_by_hidden_newlines() {
    // The hidden-newline count advances display-line-numbers so the next visible
    // row shows its true buffer line (GNU counts folded lines).
    let mut line_numbers = crate::display_row::walk_state::LineNumberRenderState::new(true, 3, 5);
    // Only the trailing `hidden_newline_count` (4) matters here.
    let hidden = BufferSourceInvisibleTextSkip::new(0, 3, 30, 30, false, true, 4);
    hidden.apply_to_line_numbers(&mut line_numbers);
    assert_eq!(
        line_numbers.current_line(),
        7,
        "line 3 + 4 hidden newlines = 7"
    );
}

#[test]
fn buffer_invisible_text_skip_captures_cursor_at_hidden_span_start() {
    let active_face = test_active_face_state(FaceId::new(9), 8.0);
    let geometry = DisplayRowGeometryState::new(2, 24.0, 0.0, 16.0, 12.0);
    let hidden = BufferSourceInvisibleTextSkip::new(5, 8, 14, 14, true, false, 0);
    let mut cursor = CursorCaptureState::new();

    hidden.capture_cursor_if_point(&mut cursor, &active_face, &geometry, 40.0, 5);

    let captured = cursor.as_ref().expect("cursor captured");
    assert_eq!(captured.x, 40.0);
    assert_eq!(captured.y, 24.0);
    assert_eq!(captured.byte_idx, 5);
    assert_eq!(captured.col, 5);
    assert_eq!(captured.display_row_offset, 2);
    assert_eq!(captured.slot_width, Some(8.0));
}

#[test]
fn buffer_invisible_text_skip_keeps_cursor_missing_when_point_is_visible() {
    let active_face = test_active_face_state(FaceId::new(9), 8.0);
    let geometry = DisplayRowGeometryState::new(2, 24.0, 0.0, 16.0, 12.0);
    let hidden = BufferSourceInvisibleTextSkip::new(5, 8, 14, 14, false, false, 0);
    let mut cursor = CursorCaptureState::new();

    hidden.capture_cursor_if_point(&mut cursor, &active_face, &geometry, 40.0, 5);

    assert!(cursor.as_ref().is_none());
}

#[test]
fn buffer_invisible_text_skip_builds_active_ellipsis_request() {
    let hidden = BufferSourceInvisibleTextSkip::new(5, 8, 14, 14, false, true, 0);
    let position = DisplayRowPosition::new(16.0, 2);

    let request = hidden
        .ellipsis_append_request(position, None)
        .expect("ellipsis request");
    let (request_position, source, face) = request.into_parts();

    assert_eq!(request_position, position);
    assert_eq!(source.source_id(), SYNTHETIC_SOURCE_INVISIBLE_ELLIPSIS);
    assert_eq!(source.text(), "...");
    assert!(matches!(face, SyntheticTextAppendFace::ActiveFace));
}

#[test]
fn buffer_invisible_text_skip_omits_ellipsis_request_without_policy() {
    let hidden = BufferSourceInvisibleTextSkip::new(5, 8, 14, 14, false, false, 0);

    assert!(
        hidden
            .ellipsis_append_request(DisplayRowPosition::new(16.0, 2), None)
            .is_none()
    );
}

#[test]
fn buffer_invisible_text_render_request_appends_ellipsis_and_captures_cursor() {
    let mut context = RowTransitionTestContext::new("invisible-text-render-request");
    let buf_id = context
        .eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = context
            .eval
            .buffer_manager_mut()
            .get_mut(buf_id)
            .expect("buffer");
        buffer.insert("folded rest");
        buffer.set_buffer_local(
            "buffer-invisibility-spec",
            Value::list(vec![Value::cons(Value::symbol("outline"), Value::T)]),
        );
        let _ = context
            .eval
            .buffer_manager_mut()
            .put_buffer_text_property_in_emacs_byte_range(
                buf_id,
                EmacsByteRange::from_usize(0, 6),
                Value::symbol("invisible"),
                Value::symbol("outline"),
            );
    }
    let snapshot = current_buffer_snapshot(&context.eval, buf_id);
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let overlay_context = BufferOverlayStringTextRowRenderContext::new(
        false,
        1,
        &surface,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        0.0,
        0,
        4,
    );
    let mut checkpoints = InvisibleTextScanCheckpoint::new(0);
    let mut byte_idx = 0;
    let mut charpos = 0;
    let mut x = 0.0;
    let mut col = 0;
    let mut cursor_info = CursorCaptureState::new();
    let mut hit_row_range = HitRowRangeTracker::new(0);
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(7);
    let mut row_extend = DisplayRowScopedValue::inactive();
    let mut box_face = BoxFaceRowState::inactive();
    let mut line_numbers = LineNumberRenderState::new(false, 0, 0);
    let mut prefix_request = DisplayRowPrefixRequest::None;
    let mut hscroll_skip = HorizontalScrollSkipState::new(
        LineWrapMode::Wrap,
        0,
        HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph,
    );
    let mut word_wrap = WordWrapRenderState::new(false);
    let mut trailing_whitespace = TrailingWhitespaceRenderState::new(false, 0);
    let mut face_scan = FaceScanCheckpoint::initial();
    let mut font_metrics = None;
    let mut source_walk = BufferSourceWalk::new(buf_id, &snapshot, 0, 0);

    let outcome = BufferSourceInvisibleTextRenderContext::new(
        b"folded rest",
        11,
        2,
        &surface,
        &active_face,
        0.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
    )
    .render_at_checkpoint_and_apply(
        &mut source_walk,
        &snapshot,
        BufferSourceLoopMutableState::new(
            &mut checkpoints,
            DisplaySourceProgressState::new(&mut byte_idx, &mut charpos, &mut x, &mut col),
            text_row_source_render_state(
                &mut context.builder,
                &mut context.output_emitter,
                &mut context.eval,
                &mut font_metrics,
                &face_resolver,
            ),
            BufferSourceRowBuildState::new(
                &mut context.geometry,
                &mut context.row_flags,
                &mut row_extend,
                &mut box_face,
            ),
            BufferSourceHitCaptureState::new(&mut context.hit_rows, &mut hit_row_range),
            BufferSourceRowCarryoverState::new(
                &mut prefix_request,
                &mut line_numbers,
                &mut hscroll_skip,
                &mut word_wrap,
                &mut trailing_whitespace,
            ),
            &mut face_scan,
            &mut context.row_y_positions,
            &mut cursor_info,
            &mut face_ids,
            BufferSourceSurfaceContext::new(&surface, overlay_context),
        ),
    );

    assert_eq!(
        outcome,
        BufferSourceInvisibleTextRenderOutcome::HiddenSpanApplied
    );
    assert_eq!(byte_idx, 6);
    assert_eq!(charpos, 6);
    assert_eq!(x, 24.0);
    assert_eq!(col, 3);
    assert!(cursor_info.captured().is_some());
    context
        .builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[GlyphArea::Text.index()];
            assert_eq!(text.len(), 3);
            assert!(matches!(text[0].glyph_type, GlyphType::Char { ch: '.' }));
            assert!(matches!(text[1].glyph_type, GlyphType::Char { ch: '.' }));
            assert!(matches!(text[2].glyph_type, GlyphType::Char { ch: '.' }));
        })
        .expect("current row");
}

#[test]
fn buffer_selective_display_context_skips_carriage_return_tail_to_newline() {
    let text = b"a\rb\nc";
    let context = BufferSourceSelectiveDisplayContext::new(text, 1, 8);
    let mut position = DisplaySourceTextPosition::new(2, 1);

    assert!(context.hides_carriage_return_tail('\r'));
    let action = context.skip_rest_of_line_after_carriage_return(&mut position);

    assert_eq!(
        action,
        BufferSourceSelectiveDisplayLineTailAction::LineBreak { charpos: 4 }
    );
    assert!(action.is_line_break());
    assert_eq!(action.charpos(), Some(4));
    assert_eq!(position, DisplaySourceTextPosition::new(4, 4));
}

#[test]
fn buffer_selective_display_context_reports_carriage_return_tail_marker() {
    let context = BufferSourceSelectiveDisplayContext::new(b"a\rb", 1, 8);

    assert_eq!(
        context.carriage_return_tail_marker('\r'),
        Some(BufferSourceSelectiveDisplayLineTailMarker)
    );
    assert_eq!(context.carriage_return_tail_marker('x'), None);
}

#[test]
fn buffer_selective_display_line_tail_marker_builds_active_ellipsis_request() {
    let marker = BufferSourceSelectiveDisplayLineTailMarker;
    let position = DisplayRowPosition::new(24.0, 3);

    let request = marker.ellipsis_append_request(position, None);
    let (request_position, source, face) = request.into_parts();

    assert_eq!(request_position, position);
    assert_eq!(source.source_id(), SYNTHETIC_SOURCE_SELECTIVE_ELLIPSIS);
    assert_eq!(source.text(), "...");
    assert!(matches!(face, SyntheticTextAppendFace::ActiveFace));
}

#[test]
fn buffer_selective_display_context_reports_exhausted_carriage_return_tail() {
    let text = b"a\rhidden";
    let context = BufferSourceSelectiveDisplayContext::new(text, 1, 8);
    let mut position = DisplaySourceTextPosition::new(2, 1);

    let action = context.skip_rest_of_line_after_carriage_return(&mut position);

    assert_eq!(
        action,
        BufferSourceSelectiveDisplayLineTailAction::Exhausted
    );
    assert!(!action.is_line_break());
    assert_eq!(action.charpos(), None);
    assert_eq!(position, DisplaySourceTextPosition::new(text.len(), 8));
}

#[test]
fn buffer_selective_display_context_skips_hidden_indented_lines() {
    let text = b"  hidden\n\talso\n visible\n";
    let context = BufferSourceSelectiveDisplayContext::new(text, 1, 4);
    let mut position = DisplaySourceTextPosition::new(0, 0);
    let mut line_numbers = LineNumberRenderState::new(true, 7, 9);

    assert!(context.hides_indented_lines_after_line_break(position.byte_idx()));
    let hidden_lines = context.skip_hidden_indented_lines_after_line_break(&mut position);
    hidden_lines.apply_to_line_numbers(&mut line_numbers);

    assert_eq!(hidden_lines.hidden_line_count(), 2);
    assert_eq!(
        position,
        DisplaySourceTextPosition::new(
            b"  hidden\n\talso\n".len(),
            b"  hidden\n\talso\n".len() as i64
        )
    );
    assert_eq!(line_numbers.current_line(), 9);
}

#[test]
fn buffer_selective_display_context_applies_hidden_indented_lines_after_line_break() {
    let text = b"  hidden\n\talso\n visible\n";
    let context = BufferSourceSelectiveDisplayContext::new(text, 1, 4);
    let mut position = DisplaySourceTextPosition::new(0, 0);
    let mut line_numbers = LineNumberRenderState::new(true, 7, 9);

    let hidden_lines =
        context.apply_hidden_indented_lines_after_line_break(&mut position, &mut line_numbers);

    assert_eq!(hidden_lines.hidden_line_count(), 2);
    assert_eq!(
        position,
        DisplaySourceTextPosition::new(
            b"  hidden\n\talso\n".len(),
            b"  hidden\n\talso\n".len() as i64
        )
    );
    assert_eq!(line_numbers.current_line(), 9);
}

#[test]
fn buffer_selective_display_context_apply_hidden_indented_lines_noops_when_disabled() {
    let text = b"  visible\n";
    let context = BufferSourceSelectiveDisplayContext::new(text, 0, 4);
    let mut position = DisplaySourceTextPosition::new(0, 0);
    let mut line_numbers = LineNumberRenderState::new(true, 7, 9);

    let hidden_lines =
        context.apply_hidden_indented_lines_after_line_break(&mut position, &mut line_numbers);

    assert_eq!(hidden_lines.hidden_line_count(), 0);
    assert_eq!(line_numbers.current_line(), 7);
    assert_eq!(position, DisplaySourceTextPosition::new(0, 0));
}

#[test]
fn buffer_selective_display_context_keeps_visible_indented_line() {
    let text = b" visible\n";
    let context = BufferSourceSelectiveDisplayContext::new(text, 1, 4);
    let mut position = DisplaySourceTextPosition::new(0, 0);

    let hidden_lines = context.skip_hidden_indented_lines_after_line_break(&mut position);

    assert_eq!(hidden_lines.hidden_line_count(), 0);
    assert_eq!(position, DisplaySourceTextPosition::new(0, 0));
}

#[test]
fn buffer_text_source_consumption_state_preserves_single_char_source_item() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.insert("a");
    }
    let snapshot = current_buffer_snapshot(&eval, buf_id);
    let mut cursor = BufferTextSourceCursor::new(
        buf_id,
        &snapshot,
        CharPos0::new(0),
        CharPos0::new(1),
        RenderFaceRef::Inherit,
    );
    let item = cursor
        .next_item(&mut DisplaySourceContext::empty())
        .expect("source item");
    let text_start_byte = match &item.span.start {
        DisplaySourcePosition::Buffer { byte_pos, .. } => byte_pos.get(),
        other => panic!("expected buffer source, got {other:?}"),
    };
    let mut position = DisplaySourceTextPosition::new(0, 0);

    let step = BufferSourceConsumptionState::new(text_start_byte)
        .render_item_from_item(item, &mut position)
        .expect("source step");

    let source_char = step.source_step_char().expect("source step char");
    let (_, source_item) = step.into_render_parts().expect("render parts");
    assert_eq!(source_char.ch(), 'a');
    assert_eq!(position.byte_idx(), 1);
    assert_eq!(position.charpos(), 0);
    assert_eq!(
        source_item.span.end,
        DisplaySourcePosition::buffer(
            buf_id,
            CharPos0::new(1),
            neovm_core::buffer::EmacsBytePos::new(text_start_byte + 1),
        )
    );
    match &source_item.kind {
        DisplayItemKind::TextRun(run) => assert_eq!(&*run.text, "a"),
        other => panic!("expected single-char text run, got {other:?}"),
    }
}

#[test]
fn buffer_text_source_consumption_state_can_return_full_text_run_item() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.insert("abcdefghij");
    }
    let snapshot = current_buffer_snapshot(&eval, buf_id);
    let mut cursor = BufferTextSourceCursor::new(
        buf_id,
        &snapshot,
        CharPos0::new(0),
        CharPos0::new(3),
        RenderFaceRef::Inherit,
    );
    let mut source_context = DisplaySourceContext::empty();
    let mut source_consumption = BufferSourceConsumptionState::new(0);
    let position = DisplaySourceTextPosition::new(0, 0);

    let typed_item = source_consumption
        .next_item_from_source(&mut cursor, &mut source_context, &position)
        .expect("typed source item");

    assert_eq!(typed_item.start_byte_idx(), 0);
    assert_eq!(typed_item.start_charpos(), 0);
    assert_eq!(cursor.current_char_pos(), CharPos0::new(3));
    match &typed_item.item().kind {
        DisplayItemKind::TextRun(run) => assert_eq!(&*run.text, "abc"),
        other => panic!("expected full text run, got {other:?}"),
    }
}

#[test]
fn buffer_text_source_item_can_build_direct_single_char_step() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.insert("a");
    }
    let snapshot = current_buffer_snapshot(&eval, buf_id);
    let mut cursor = BufferTextSourceCursor::new(
        buf_id,
        &snapshot,
        CharPos0::new(0),
        CharPos0::new(1),
        RenderFaceRef::Inherit,
    );
    let mut source_context = DisplaySourceContext::empty();
    let mut source_consumption = BufferSourceConsumptionState::new(0);
    let mut position = DisplaySourceTextPosition::new(0, 0);
    let typed_item = source_consumption
        .next_item_from_source(&mut cursor, &mut source_context, &position)
        .expect("typed source item");

    let step = typed_item
        .consume_for_render(&mut position)
        .ok()
        .expect("direct source step");

    assert_eq!(position.byte_idx(), 1);
    assert_eq!(position.charpos(), 0);
    assert_eq!(step.source_step_char().expect("source step char").ch(), 'a');
    let (_, source_item) = step.into_render_parts().expect("render parts");
    match &source_item.kind {
        DisplayItemKind::TextRun(run) => assert_eq!(&*run.text, "a"),
        other => panic!("expected single-char text run, got {other:?}"),
    }
}

#[test]
fn buffer_text_source_item_can_build_direct_source_mapped_step() {
    let source_item = crate::display_item::DisplayItem::new(
        crate::display_item::SourceSpan::new(
            DisplaySourcePosition::buffer(
                BufferId(1),
                CharPos0::new(0),
                neovm_core::buffer::EmacsBytePos::new(0),
            ),
            DisplaySourcePosition::buffer(
                BufferId(1),
                CharPos0::new(1),
                neovm_core::buffer::EmacsBytePos::new(2),
            ),
        ),
        RenderFaceRef::Inherit,
        DisplayItemKind::SourceMappedText(DisplaySourceMappedText::new("\\ ")),
    );
    let typed_item = DisplaySourceItem::new_for_test(source_item, 0, 0, Some('\u{00a0}'));
    let mut position = DisplaySourceTextPosition::new(0, 0);

    let step = BufferSourceConsumptionState::new(0)
        .render_item_from_source_item(typed_item, &mut position)
        .expect("source-mapped item should retain direct source char");

    assert_eq!(position.byte_idx(), 2);
    assert_eq!(position.charpos(), 0);
    assert_eq!(
        step.source_step_char().expect("source step char").ch(),
        '\u{00a0}'
    );
    let (_, source_item) = step.into_render_parts().expect("render parts");
    match &source_item.kind {
        DisplayItemKind::SourceMappedText(text) => assert_eq!(&*text.text, "\\ "),
        other => panic!("expected source-mapped text, got {other:?}"),
    }
}

#[test]
fn buffer_text_source_item_direct_source_mapped_step_uses_item_span_end() {
    let source_item = crate::display_item::DisplayItem::new(
        crate::display_item::SourceSpan::new(
            DisplaySourcePosition::buffer(
                BufferId(1),
                CharPos0::new(1),
                neovm_core::buffer::EmacsBytePos::new(1),
            ),
            DisplaySourcePosition::buffer(
                BufferId(1),
                CharPos0::new(3),
                neovm_core::buffer::EmacsBytePos::new(4),
            ),
        ),
        RenderFaceRef::Inherit,
        DisplayItemKind::SourceMappedText(DisplaySourceMappedText::new("YZ")),
    );
    let typed_item = DisplaySourceItem::new_for_test(source_item, 1, 1, Some('b'));
    let mut position = DisplaySourceTextPosition::new(1, 1);

    let step = BufferSourceConsumptionState::new(0)
        .render_item_from_source_item(typed_item, &mut position)
        .expect("source-mapped item should advance over covered source span");

    assert_eq!(position.byte_idx(), 4);
    assert_eq!(position.charpos(), 1);
    assert_eq!(step.source_step_char().expect("source step char").ch(), 'b');
    assert_eq!(step.end_charpos(), 3);
    let (_, source_item) = step.into_render_parts().expect("render parts");
    match &source_item.kind {
        DisplayItemKind::SourceMappedText(text) => assert_eq!(&*text.text, "YZ"),
        other => panic!("expected source-mapped text, got {other:?}"),
    }
}

#[test]
fn buffer_text_source_item_without_source_char_rejects_source_mapped_without_source_char() {
    let source_item = crate::display_item::DisplayItem::new(
        crate::display_item::SourceSpan::new(
            DisplaySourcePosition::buffer(
                BufferId(1),
                CharPos0::new(0),
                neovm_core::buffer::EmacsBytePos::new(0),
            ),
            DisplaySourcePosition::buffer(
                BufferId(1),
                CharPos0::new(1),
                neovm_core::buffer::EmacsBytePos::new(1),
            ),
        ),
        RenderFaceRef::Inherit,
        DisplayItemKind::SourceMappedText(DisplaySourceMappedText::new("\\ ")),
    );
    let typed_item = DisplaySourceItem::new_for_test(source_item, 0, 0, None);
    let mut position = DisplaySourceTextPosition::new(0, 0);

    assert!(typed_item.direct_source_char().is_none());
    match &typed_item.item().kind {
        DisplayItemKind::SourceMappedText(text) => assert_eq!(&*text.text, "\\ "),
        other => panic!("expected source-mapped text, got {other:?}"),
    }

    let step = typed_item.consume_for_render(&mut position);

    assert_eq!(position, DisplaySourceTextPosition::new(0, 0));
    assert!(step.is_err());
}

#[test]
fn buffer_text_source_item_builds_direct_multi_char_runs() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.insert("ab");
    }
    let snapshot = current_buffer_snapshot(&eval, buf_id);
    let mut cursor = BufferTextSourceCursor::new(
        buf_id,
        &snapshot,
        CharPos0::new(0),
        CharPos0::new(2),
        RenderFaceRef::Inherit,
    );
    let mut source_context = DisplaySourceContext::empty();
    let mut source_consumption = BufferSourceConsumptionState::new(0);
    let mut position = DisplaySourceTextPosition::new(0, 0);
    let typed_item = source_consumption
        .next_item_from_source(&mut cursor, &mut source_context, &position)
        .expect("typed source item");

    assert_eq!(typed_item.direct_source_char(), Some('a'));
    match &typed_item.item().kind {
        DisplayItemKind::TextRun(run) => assert_eq!(&*run.text, "ab"),
        other => panic!("expected full text run, got {other:?}"),
    }
    let step = typed_item
        .consume_for_render(&mut position)
        .ok()
        .expect("multi-char text run remains a typed source item");
    assert_eq!(step.source_step_char().expect("source step char").ch(), 'a');
    assert_eq!(position, DisplaySourceTextPosition::new(2, 0));

    let direct = step;
    let (first, pending) = direct
        .split_text_run_items(0)
        .expect("direct multi-char text run splits for rendering");
    assert_eq!(first.into_render_parts().expect("render parts").0.ch(), 'a');
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0]
            .source_step_char()
            .expect("source step char")
            .ch(),
        'b'
    );
    let (_, pending_item) = pending[0]
        .clone()
        .into_render_parts()
        .expect("render parts");
    match &pending_item.kind {
        DisplayItemKind::TextRun(run) => assert_eq!(&*run.text, "b"),
        other => panic!("expected direct split text run, got {other:?}"),
    }

    let position = DisplaySourceTextPosition::new(0, 0);
    let typed_item = source_consumption
        .next_item_from_source(&mut cursor, &mut source_context, &position)
        .expect("typed source item after cursor reset");
    assert_eq!(position, DisplaySourceTextPosition::new(0, 0));
    match &typed_item.item().kind {
        DisplayItemKind::TextRun(run) => assert_eq!(&*run.text, "ab"),
        other => panic!("expected full text run, got {other:?}"),
    }
}

#[test]
fn buffer_text_source_consumption_state_keeps_display_item_layout() {
    let source_item = crate::display_item::DisplayItem::new(
        crate::display_item::SourceSpan::new(
            DisplaySourcePosition::buffer(
                BufferId(1),
                CharPos0::new(0),
                neovm_core::buffer::EmacsBytePos::new(0),
            ),
            DisplaySourcePosition::buffer(
                BufferId(1),
                CharPos0::new(1),
                neovm_core::buffer::EmacsBytePos::new(1),
            ),
        ),
        RenderFaceRef::Inherit,
        DisplayItemKind::TextRun(crate::display_item::DisplayTextRun::new("x")),
    )
    .with_layout(DisplayItemLayout {
        raise: Some(0.25),
        height: Some(1.5),
        space_width: None,
        break_after_row: false,
    });
    let mut position = DisplaySourceTextPosition::new(0, 0);

    let step = BufferSourceConsumptionState::new(0)
        .render_item_from_item(source_item, &mut position)
        .expect("source step");

    let (_, source_item) = step.into_render_parts().expect("render parts");
    assert_eq!(source_item.layout.raise, Some(0.25));
    assert_eq!(source_item.layout.height, Some(1.5));
}

#[test]
fn buffer_text_source_consumption_state_splits_persistent_text_run() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.insert("abc");
    }
    let snapshot = current_buffer_snapshot(&eval, buf_id);
    let mut cursor = BufferTextSourceCursor::new(
        buf_id,
        &snapshot,
        CharPos0::new(0),
        CharPos0::new(3),
        RenderFaceRef::Inherit,
    );
    let mut source_context = DisplaySourceContext::empty();
    let mut source_consumption = BufferSourceConsumptionState::new(0);
    let mut position = DisplaySourceTextPosition::new(0, 0);

    let item = source_consumption
        .next_display_item_from_source(&mut cursor, &mut source_context, &mut position)
        .expect("source item");
    assert_eq!(item.source_step_char().expect("source step char").ch(), 'a');
    assert_eq!(
        item.source_step_char()
            .expect("source step char")
            .start_byte_idx(),
        0
    );
    assert_eq!(position.byte_idx(), 3);
    assert_eq!(position.charpos(), 0);
    assert_eq!(cursor.current_char_pos(), CharPos0::new(3));
    let direct = item;
    let (first, pending) = direct
        .split_text_run_items(0)
        .expect("direct text run splits at render boundary");
    assert_eq!(first.into_render_parts().expect("render parts").0.ch(), 'a');
    assert_eq!(pending.len(), 2);
    assert_eq!(
        pending[0]
            .source_step_char()
            .expect("source step char")
            .ch(),
        'b'
    );
    assert_eq!(
        pending[1]
            .source_step_char()
            .expect("source step char")
            .ch(),
        'c'
    );
}

#[test]
fn buffer_text_source_consumption_state_rejects_non_buffer_items() {
    let item = crate::display_item::DisplayItem::new(
        crate::display_item::SourceSpan::synthetic(9, 0, 1),
        RenderFaceRef::Inherit,
        DisplayItemKind::TextRun(crate::display_item::DisplayTextRun::new("x")),
    );
    let mut position = DisplaySourceTextPosition::new(3, 7);

    let step = BufferSourceConsumptionState::new(0).render_item_from_item(item, &mut position);

    assert!(step.is_none());
    assert_eq!(position, DisplaySourceTextPosition::new(3, 7));
}

#[test]
fn buffer_text_source_consumption_state_rejects_replacement_items() {
    let item = crate::display_item::DisplayItem::new(
        crate::display_item::SourceSpan::new(
            DisplaySourcePosition::buffer(
                BufferId(1),
                CharPos0::new(0),
                neovm_core::buffer::EmacsBytePos::new(0),
            ),
            DisplaySourcePosition::buffer(
                BufferId(1),
                CharPos0::new(1),
                neovm_core::buffer::EmacsBytePos::new(1),
            ),
        ),
        RenderFaceRef::Inherit,
        DisplayItemKind::Stretch(crate::display_item::DisplayStretch {
            width: crate::display_item::DisplayStretchWidth::Length(
                crate::display_item::DisplayLength::Pixels(8.0),
            ),
            height: None,
            ascent: None,
        }),
    );
    let mut position = DisplaySourceTextPosition::new(0, 0);

    let step = BufferSourceConsumptionState::new(0).render_item_from_item(item, &mut position);

    assert!(step.is_none());
    assert_eq!(position, DisplaySourceTextPosition::new(0, 0));
}

#[test]
fn buffer_text_source_step_char_consumes_multibyte_text_cursor() {
    let mut position = DisplaySourceTextPosition::new("a".len(), 4);

    let source_char = position
        .consume_step_char("a界b".as_bytes())
        .expect("source char");

    assert_eq!(source_char.ch(), '界');
    assert_eq!(source_char.start_byte_idx(), "a".len());
    assert_eq!(source_char.start_charpos(), 4);
    assert_eq!(position.byte_idx(), "a界".len());
    assert_eq!(position.charpos(), 5);
}

#[test]
fn buffer_text_source_step_char_records_word_wrap_candidate() {
    let mut context = RowTransitionTestContext::new("source-step-char-word-wrap");
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let mut font_metrics = None;
    let source_char = DisplaySourceStepChar::new('a', 1, 6);
    let mut word_wrap = WordWrapRenderState::new(true);
    word_wrap.allow_after_current_char(' ');

    {
        let source_render = text_row_source_render_state(
            &mut context.builder,
            &mut context.output_emitter,
            &mut context.eval,
            &mut font_metrics,
            &face_resolver,
        );
        source_char.record_word_wrap_candidate(&mut word_wrap, &source_render);
    }

    let candidate = word_wrap.candidate();
    assert!(candidate.is_available());
    assert_eq!(candidate.byte_idx(), 1);
    assert_eq!(candidate.charpos(), 6);
    assert_eq!(candidate.display_point_count(), 0);
    // The current row is empty here, so the captured glyph checkpoint is the
    // zero-length default (nothing to roll back).
    assert_eq!(
        candidate.glyph_checkpoint(),
        crate::display_row::builder::DisplayRowGlyphCheckpoint::default()
    );
}

#[test]
fn buffer_text_source_step_char_builds_line_break_action() {
    let source_char = DisplaySourceStepChar::new('\n', 1, 1);

    let action = BufferSourceLineBreakSourceAction::for_source_step_newline(
        source_char,
        16.0,
        5.0,
        crate::display_item::DisplayLineSpacingPolicy::Inherit,
    );

    assert_eq!(source_char.ch(), '\n');
    assert!(action.point_matches(1));
    assert_eq!(action.next_charpos(), 2);
    assert_eq!(action.line_spacing(), 5.0);
}

#[test]
fn buffer_text_line_break_source_action_uses_extra_line_spacing() {
    let action = BufferSourceLineBreakSourceAction::for_newline(
        1,
        1,
        16.0,
        5.0,
        crate::display_item::DisplayLineSpacingPolicy::Inherit,
    );

    assert!(action.point_matches(1));
    assert!(!action.point_matches(2));
    assert_eq!(action.next_charpos(), 2);
    assert_eq!(action.line_spacing(), 5.0);
}

#[test]
fn buffer_text_line_break_source_action_uses_resolved_text_property_spacing() {
    let action = BufferSourceLineBreakSourceAction::for_newline(
        1,
        1,
        16.0,
        5.0,
        crate::display_item::DisplayLineSpacingPolicy::Pixels(7.0),
    );

    assert_eq!(action.next_charpos(), 2);
    assert_eq!(action.line_spacing(), 7.0);
}

#[test]
fn buffer_text_line_break_source_action_builds_row_end_cursor_info() {
    let active_face = test_active_face_state(FaceId::new(9), 8.0);
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);

    let action = BufferSourceLineBreakSourceAction::for_newline(
        4,
        12,
        16.0,
        0.0,
        crate::display_item::DisplayLineSpacingPolicy::Inherit,
    );
    let cursor = action.cursor_info(&active_face, &geometry, 32.0, 4);

    assert_eq!(cursor.x, 32.0);
    assert_eq!(cursor.byte_idx, 12);
    assert_eq!(cursor.col, 4);
    assert_eq!(cursor.slot_width, Some(8.0));
    assert!(!cursor.stretch_like);
}

#[test]
fn buffer_text_line_break_source_action_captures_cursor_when_point_matches() {
    let active_face = test_active_face_state(FaceId::new(9), 8.0);
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let action = BufferSourceLineBreakSourceAction::for_newline(
        4,
        12,
        16.0,
        0.0,
        crate::display_item::DisplayLineSpacingPolicy::Inherit,
    );
    let mut cursor = CursorCaptureState::new();

    action.capture_cursor_if_point(&mut cursor, &active_face, &geometry, 4, 32.0, 4);

    let captured = cursor.as_ref().expect("cursor captured");
    assert_eq!(captured.x, 32.0);
    assert_eq!(captured.byte_idx, 12);
    assert_eq!(captured.col, 4);
    assert_eq!(captured.slot_width, Some(8.0));
}

#[test]
fn buffer_text_line_break_source_action_keeps_cursor_missing_when_point_differs() {
    let active_face = test_active_face_state(FaceId::new(9), 8.0);
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let action = BufferSourceLineBreakSourceAction::for_newline(
        4,
        12,
        16.0,
        0.0,
        crate::display_item::DisplayLineSpacingPolicy::Inherit,
    );
    let mut cursor = CursorCaptureState::new();

    action.capture_cursor_if_point(&mut cursor, &active_face, &geometry, 5, 32.0, 4);

    assert!(cursor.as_ref().is_none());
}

#[test]
fn buffer_text_line_break_source_action_applies_row_transition_state() {
    let mut context = RowTransitionTestContext::new("line-break-source-state");
    let geometry = context.geometry;
    let action = BufferSourceLineBreakSourceAction::for_newline(
        4,
        12,
        16.0,
        0.0,
        crate::display_item::DisplayLineSpacingPolicy::Inherit,
    );
    let mut trailing_whitespace = TrailingWhitespaceRenderState::new(true, 0x00ff00);
    trailing_whitespace.track_rendered_char(' ', geometry.start_marker_at_x(24.0));
    let mut row_extend = DisplayRowScopedValue::inactive();
    row_extend.activate(
        geometry.current_row_marker(),
        test_row_extend_face(0x112233, FaceId::new(17)),
    );
    let mut box_face = BoxFaceRowState::inactive();
    box_face.activate(geometry.current_row_marker(), 8.0);
    let mut byte_idx = 12;
    let mut x = 40.0;
    let mut charpos = 4;
    let mut col = 6;
    let mut progress =
        DisplaySourceProgressState::new(&mut byte_idx, &mut charpos, &mut x, &mut col);

    action.apply_before_row_transition(
        &geometry,
        &mut trailing_whitespace,
        &mut row_extend,
        &mut box_face,
        &mut context.output_emitter,
        crate::buffer_source::row_lifecycle::DisplayRowEnd::BufferNewline {
            cell: crate::window_output::DisplayRowTerminatorCell::new(8.0, 16.0),
        },
        2.0,
        &mut progress,
    );

    assert_eq!(x, 2.0);
    assert_eq!(charpos, 5);
    assert_eq!(byte_idx, 12);
    assert_eq!(col, 6);
    assert_eq!(trailing_whitespace.highlight_start_x(&geometry), None);
    assert_eq!(row_extend.value_on(&geometry), None);
    assert_eq!(box_face.row(), geometry.current_row_marker());
    assert_eq!(box_face.start_x(), Some(2.0));
}

#[test]
fn sync_position_after_row_transition_advances_charpos_and_hit_range() {
    // Shared by the line-break, hidden-line-break, and truncation-skip actions.
    let mut position = DisplaySourceTextPosition::new(2, 9);
    let mut hit_row_range = HitRowRangeTracker::new(3);

    crate::display_row::walk_state::sync_position_after_row_transition(
        14,
        &mut position,
        &mut hit_row_range,
    );

    assert_eq!(position, DisplaySourceTextPosition::new(2, 14));
    assert_eq!(hit_row_range.start(), 14);
}

#[test]
fn buffer_text_line_break_source_action_applies_after_transition() {
    let geometry = DisplayRowGeometryState::new(1, 16.0, 0.0, 16.0, 12.0);
    let action = BufferSourceLineBreakSourceAction::for_newline(
        4,
        12,
        16.0,
        0.0,
        crate::display_item::DisplayLineSpacingPolicy::Inherit,
    );
    let active_face = test_active_face_state_with_extend(FaceId::new(23), 8.0, true);
    let mut row_extend = DisplayRowScopedValue::inactive();
    let mut box_face = BoxFaceRowState::inactive();
    box_face.activate(geometry.current_row_marker(), 8.0);
    let mut position = DisplaySourceTextPosition::new(2, 9);
    let mut hit_row_range = HitRowRangeTracker::new(3);

    let continuation = action.apply_after_line_break_row_transition(
        DisplayTextRowTransition::BeganNextRow,
        14,
        &mut position,
        &mut hit_row_range,
        &geometry,
        &mut row_extend,
        &active_face,
        &mut box_face,
        2.0,
    );

    assert_eq!(continuation, DisplayRowTransitionContinuation::Continue);
    assert_eq!(position, DisplaySourceTextPosition::new(2, 14));
    assert_eq!(hit_row_range.start(), 14);
    assert_eq!(
        row_extend.value_on(&geometry).copied(),
        active_face.row_extend_fill()
    );
    assert_eq!(box_face.row(), geometry.current_row_marker());
    assert_eq!(box_face.start_x(), Some(2.0));
}

#[test]
fn buffer_text_line_break_source_action_skips_after_state_when_transition_exhausted() {
    let geometry = DisplayRowGeometryState::new(1, 16.0, 0.0, 16.0, 12.0);
    let action = BufferSourceLineBreakSourceAction::for_newline(
        4,
        12,
        16.0,
        0.0,
        crate::display_item::DisplayLineSpacingPolicy::Inherit,
    );
    let active_face = test_active_face_state_with_extend(FaceId::new(23), 8.0, true);
    let mut row_extend = DisplayRowScopedValue::inactive();
    let mut box_face = BoxFaceRowState::inactive();
    let mut position = DisplaySourceTextPosition::new(2, 9);
    let mut hit_row_range = HitRowRangeTracker::new(3);

    let continuation = action.apply_after_line_break_row_transition(
        DisplayTextRowTransition::ExhaustedRows,
        14,
        &mut position,
        &mut hit_row_range,
        &geometry,
        &mut row_extend,
        &active_face,
        &mut box_face,
        2.0,
    );

    assert_eq!(continuation, DisplayRowTransitionContinuation::Exhausted);
    assert_eq!(position, DisplaySourceTextPosition::new(2, 9));
    assert_eq!(hit_row_range.start(), 3);
    assert_eq!(row_extend.value_on(&geometry), None);
    assert_eq!(box_face.start_x(), None);
}

#[test]
fn buffer_text_line_break_render_request_emits_row_transition_and_syncs_position() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.insert("\nnext");
    }
    let snapshot = current_buffer_snapshot(&eval, buf_id);
    let mut context = RowTransitionTestContext::new("line-break-render-request");
    let active_face = test_active_face_state(FaceId::new(9), 8.0);
    let text = b"\nnext";
    let mut byte_idx = 1;
    let source_char = DisplaySourceStepChar::new('\n', 0, 0);
    let mut charpos = 0;
    let mut cursor_info = CursorCaptureState::new();
    let mut trailing_whitespace = TrailingWhitespaceRenderState::new(true, 0x00ff00);
    trailing_whitespace.track_rendered_char(' ', context.geometry.start_marker_at_x(24.0));
    let mut row_extend = DisplayRowScopedValue::inactive();
    row_extend.activate(
        context.geometry.current_row_marker(),
        test_row_extend_face(0x112233, FaceId::new(17)),
    );
    let mut box_face = BoxFaceRowState::inactive();
    box_face.activate(context.geometry.current_row_marker(), 8.0);
    let mut x = 40.0;
    let mut col = 5;
    let mut prefix_request = DisplayRowPrefixRequest::None;
    let mut line_numbers = LineNumberRenderState::new(true, 1, 0);
    let mut hscroll_skip = HorizontalScrollSkipState::new(
        LineWrapMode::Wrap,
        0,
        HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph,
    );
    let mut word_wrap = WordWrapRenderState::new(false);
    let mut hit_row_range = HitRowRangeTracker::new(0);
    let mut invisible_text_checkpoint = InvisibleTextScanCheckpoint::new(charpos);
    let mut face_scan = FaceScanCheckpoint::initial();
    let row_limit = context.row_limit;
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let mut font_metrics = None;
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let overlay_context = BufferOverlayStringTextRowRenderContext::new(
        true,
        1,
        &surface,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        0.0,
        0,
        4,
    );
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let mut source_walk = BufferSourceWalk::new(buf_id, &snapshot, charpos, 0);

    let continuation = BufferSourceLineBreakRenderRequest::new(
        source_char,
        BufferSourceLineBreakRenderContext::new(
            text,
            0,
            0,
            8,
            &active_face,
            0,
            16.0,
            0.0,
            0.0,
            false,
            context.defaults,
            0,
            4,
            row_limit,
            &surface,
            Color::from_pixel(0x00FFFFFF),
            -1,
            '|',
        ),
    )
    .render_and_apply(
        &mut source_walk,
        &snapshot,
        BufferSourceLoopMutableState::new(
            &mut invisible_text_checkpoint,
            DisplaySourceProgressState::new(&mut byte_idx, &mut charpos, &mut x, &mut col),
            text_row_source_render_state(
                &mut context.builder,
                &mut context.output_emitter,
                &mut context.eval,
                &mut font_metrics,
                &face_resolver,
            ),
            BufferSourceRowBuildState::new(
                &mut context.geometry,
                &mut context.row_flags,
                &mut row_extend,
                &mut box_face,
            ),
            BufferSourceHitCaptureState::new(&mut context.hit_rows, &mut hit_row_range),
            BufferSourceRowCarryoverState::new(
                &mut prefix_request,
                &mut line_numbers,
                &mut hscroll_skip,
                &mut word_wrap,
                &mut trailing_whitespace,
            ),
            &mut face_scan,
            &mut context.row_y_positions,
            &mut cursor_info,
            &mut face_ids,
            BufferSourceSurfaceContext::new(&surface, overlay_context),
        ),
    );

    assert_eq!(continuation, DisplayRowTransitionContinuation::Continue);
    assert_eq!(byte_idx, 1);
    assert_eq!(charpos, 1);
    assert_eq!(x, 0.0);
    assert_eq!(col, 0);
    assert_eq!(hit_row_range.start(), 1);
    assert_eq!(context.row_y_positions.recorded(), &[0.0, 16.0]);
    assert_eq!(
        trailing_whitespace.highlight_start_x(&context.geometry),
        None
    );
    assert_eq!(row_extend.value_on(&context.geometry), None);
    assert_eq!(box_face.row(), context.geometry.current_row_marker());
    assert_eq!(box_face.start_x(), Some(0.0));
    assert!(cursor_info.as_ref().is_some());
}

#[test]
fn buffer_selective_display_line_tail_action_applies_after_hidden_line_break_transition() {
    let action = BufferSourceSelectiveDisplayLineTailAction::LineBreak { charpos: 12 };
    let mut position = DisplaySourceTextPosition::new(2, 9);
    let mut hit_row_range = HitRowRangeTracker::new(3);

    let continuation = action.apply_after_hidden_line_break_transition(
        DisplayTextRowTransition::BeganNextRow,
        14,
        &mut position,
        &mut hit_row_range,
    );

    assert_eq!(continuation, DisplayRowTransitionContinuation::Continue);
    assert_eq!(position, DisplaySourceTextPosition::new(2, 14));
    assert_eq!(hit_row_range.start(), 14);
}

#[test]
fn buffer_selective_display_line_tail_action_skips_after_state_when_transition_exhausted() {
    let action = BufferSourceSelectiveDisplayLineTailAction::LineBreak { charpos: 12 };
    let mut position = DisplaySourceTextPosition::new(2, 9);
    let mut hit_row_range = HitRowRangeTracker::new(3);

    let continuation = action.apply_after_hidden_line_break_transition(
        DisplayTextRowTransition::ExhaustedRows,
        14,
        &mut position,
        &mut hit_row_range,
    );

    assert_eq!(continuation, DisplayRowTransitionContinuation::Exhausted);
    assert_eq!(position, DisplaySourceTextPosition::new(2, 9));
    assert_eq!(hit_row_range.start(), 3);
}

#[test]
fn buffer_selective_display_tail_render_request_appends_marker_and_transitions_row() {
    let mut context = RowTransitionTestContext::new("selective-display-tail-request");
    let buf_id = context
        .eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = context
            .eval
            .buffer_manager_mut()
            .get_mut(buf_id)
            .expect("buffer");
        buffer.insert("a\rb\nc");
    }
    let snapshot = current_buffer_snapshot(&context.eval, buf_id);
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let text = b"a\rb\nc";
    let mut byte_idx = 2;
    let source_step_char = DisplaySourceStepChar::new('\r', 1, 1);
    let mut charpos = 1;
    let mut col = 0;
    let mut row_extend = DisplayRowScopedValue::inactive();
    let mut box_face = BoxFaceRowState::inactive();
    let mut x = 0.0;
    let mut line_numbers = LineNumberRenderState::new(false, 0, 0);
    let mut hit_row_range = HitRowRangeTracker::new(1);
    let mut prefix_request = DisplayRowPrefixRequest::None;
    let mut hscroll_skip = HorizontalScrollSkipState::new(
        LineWrapMode::Wrap,
        0,
        HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph,
    );
    let mut word_wrap = WordWrapRenderState::new(false);
    let mut trailing_whitespace = TrailingWhitespaceRenderState::new(false, 0);
    let mut invisible_text_checkpoint = InvisibleTextScanCheckpoint::new(charpos);
    let mut face_scan = FaceScanCheckpoint::initial();
    let mut cursor_info = CursorCaptureState::new();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let overlay_context = BufferOverlayStringTextRowRenderContext::new(
        false,
        1,
        &surface,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        0.0,
        0,
        4,
    );
    let mut font_metrics = None;
    let mut source_walk = BufferSourceWalk::new(buf_id, &snapshot, charpos, 0);

    let outcome = BufferSourceSelectiveDisplayTailRenderRequest::new(
        source_step_char,
        BufferSourceSelectiveDisplayTailRenderContext::new(
            text,
            0,
            1,
            8,
            &surface,
            &active_face,
            0.0,
            DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
            0.0,
            false,
            context.defaults,
            0,
            4,
            context.row_limit,
        ),
    )
    .render_if_needed_and_apply(
        &mut source_walk,
        &snapshot,
        BufferSourceLoopMutableState::new(
            &mut invisible_text_checkpoint,
            DisplaySourceProgressState::new(&mut byte_idx, &mut charpos, &mut x, &mut col),
            text_row_source_render_state(
                &mut context.builder,
                &mut context.output_emitter,
                &mut context.eval,
                &mut font_metrics,
                &face_resolver,
            ),
            BufferSourceRowBuildState::new(
                &mut context.geometry,
                &mut context.row_flags,
                &mut row_extend,
                &mut box_face,
            ),
            BufferSourceHitCaptureState::new(&mut context.hit_rows, &mut hit_row_range),
            BufferSourceRowCarryoverState::new(
                &mut prefix_request,
                &mut line_numbers,
                &mut hscroll_skip,
                &mut word_wrap,
                &mut trailing_whitespace,
            ),
            &mut face_scan,
            &mut context.row_y_positions,
            &mut cursor_info,
            &mut face_ids,
            BufferSourceSurfaceContext::new(&surface, overlay_context),
        ),
    );

    assert_eq!(
        outcome,
        BufferSourceSelectiveDisplayTailRenderOutcome::ContinueBufferWalk
    );
    assert_eq!(byte_idx, 4);
    assert_eq!(charpos, 4);
    assert_eq!(hit_row_range.start(), 4);
    assert_eq!(context.geometry.row(), 1);
    assert_eq!(x, 0.0);
    assert_eq!(col, 0);
    assert_eq!(context.hit_rows.len(), 1);
    assert_eq!(context.hit_rows[0].charpos_start, 1);
    assert_eq!(context.hit_rows[0].charpos_end, 4);
}

#[test]
fn buffer_text_truncation_skip_action_consumes_source_step_char_and_reaches_newline() {
    let text = b"abc\nnext";
    let mut position = DisplaySourceTextPosition::new(1, 1);

    let action = BufferSourceTruncationSkipAction::consume_source_step_char_and_rest_of_line(
        text,
        &mut position,
    );

    assert!(action.reached_line_break());
    assert_eq!(action.charpos(), 4);
    assert_eq!(position, DisplaySourceTextPosition::new(4, 4));
    assert_eq!(
        action.source_position(),
        DisplaySourceTextPosition::new(4, 4)
    );
}

#[test]
fn buffer_text_truncation_skip_action_consumes_to_text_end_without_newline() {
    let text = b"abc";
    let mut position = DisplaySourceTextPosition::new(1, 1);

    let action = BufferSourceTruncationSkipAction::consume_source_step_char_and_rest_of_line(
        text,
        &mut position,
    );

    assert!(!action.reached_line_break());
    assert_eq!(action.charpos(), 3);
    assert_eq!(position, DisplaySourceTextPosition::new(3, 3));
    assert_eq!(
        action.source_position(),
        DisplaySourceTextPosition::new(3, 3)
    );
}

#[test]
fn buffer_text_truncation_skip_action_counts_multibyte_chars() {
    let text = "a界b\n".as_bytes();
    let mut position = DisplaySourceTextPosition::new("a".len(), 1);

    let action = BufferSourceTruncationSkipAction::consume_source_step_char_and_rest_of_line(
        text,
        &mut position,
    );

    assert!(action.reached_line_break());
    assert_eq!(action.charpos(), 4);
    assert_eq!(position, DisplaySourceTextPosition::new(text.len(), 4));
    assert_eq!(
        action.source_position(),
        DisplaySourceTextPosition::new(text.len(), 4)
    );
}

#[test]
fn buffer_text_truncation_skip_action_applies_transition_state() {
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let text = b"abc\nnext";
    let mut position = DisplaySourceTextPosition::new(1, 0);
    let action = BufferSourceTruncationSkipAction::consume_source_step_char_and_rest_of_line(
        text,
        &mut position,
    );
    let mut line_numbers = LineNumberRenderState::new(true, 5, 8);
    let mut row_extend = DisplayRowScopedValue::inactive();
    row_extend.activate(
        geometry.current_row_marker(),
        test_row_extend_face(0x112233, FaceId::new(17)),
    );
    let mut x = 80.0;

    action.apply_before_row_transition(&mut line_numbers, &mut row_extend, &mut x, 3.0);

    assert_eq!(line_numbers.current_line(), 6);
    assert_eq!(x, 3.0);
    assert_eq!(row_extend.value_on(&geometry), None);
}

#[test]
fn buffer_text_truncation_skip_action_reports_transition_continuation() {
    let action = BufferSourceTruncationSkipAction {
        charpos: 12,
        reached_line_break: false,
        source_position: DisplaySourceTextPosition::new(0, 12),
    };

    assert_eq!(
        action.transition_continuation(DisplayTextRowTransition::BeganNextRow),
        DisplayRowTransitionContinuation::Continue
    );
    assert_eq!(
        action.transition_continuation(DisplayTextRowTransition::ExhaustedRows),
        DisplayRowTransitionContinuation::Exhausted
    );
}

#[test]
fn buffer_text_truncation_skip_action_syncs_after_visible_transition() {
    let action = BufferSourceTruncationSkipAction {
        charpos: 12,
        reached_line_break: true,
        source_position: DisplaySourceTextPosition::new(0, 12),
    };
    let mut position = DisplaySourceTextPosition::new(2, 9);
    let mut hit_row_range = HitRowRangeTracker::new(2);

    let continuation = action.sync_after_row_transition_if_visible(
        DisplayTextRowTransition::BeganNextRow,
        14,
        &mut position,
        &mut hit_row_range,
    );

    assert_eq!(continuation, DisplayRowTransitionContinuation::Continue);
    assert_eq!(position, DisplaySourceTextPosition::new(2, 14));
    assert_eq!(hit_row_range.start(), 14);
}

#[test]
fn buffer_text_truncation_skip_action_skips_sync_when_transition_exhausted() {
    let action = BufferSourceTruncationSkipAction {
        charpos: 12,
        reached_line_break: true,
        source_position: DisplaySourceTextPosition::new(0, 12),
    };
    let mut position = DisplaySourceTextPosition::new(2, 9);
    let mut hit_row_range = HitRowRangeTracker::new(2);

    let continuation = action.sync_after_row_transition_if_visible(
        DisplayTextRowTransition::ExhaustedRows,
        14,
        &mut position,
        &mut hit_row_range,
    );

    assert_eq!(continuation, DisplayRowTransitionContinuation::Exhausted);
    assert_eq!(position, DisplaySourceTextPosition::new(2, 9));
    assert_eq!(hit_row_range.start(), 2);
}

#[test]
fn buffer_text_word_wrap_source_action_rewinds_source_state() {
    let mut break_candidate = WordWrapBreakCandidate::default();
    break_candidate.record(
        7,
        12,
        3,
        (Some(LispCharPos1::new(10)), Some(LispCharPos1::new(12))),
        DisplayRowGlyphCheckpoint::default(),
    );
    let action = BufferSourceWordWrapAction::new(break_candidate);
    let mut position = DisplaySourceTextPosition::new(20, 30);
    let mut col = 9;

    action.rewind_source_state(&mut position, &mut col);

    assert_eq!(action.byte_idx(), 7);
    assert_eq!(action.charpos(), 12);
    assert_eq!(
        action.source_position(),
        DisplaySourceTextPosition::new(7, 12)
    );
    assert_eq!(position, DisplaySourceTextPosition::new(7, 12));
    assert_eq!(col, 0);
}

#[test]
fn buffer_text_word_wrap_restores_the_candidate_extend_face_and_pen() {
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let candidate_extend = test_row_extend_face(0x112233, FaceId::new(17));
    let overflow_extend = test_row_extend_face(0x445566, FaceId::new(21));
    let candidate_position = DisplayRowPosition::new(31.0, 4);
    let mut break_candidate = WordWrapBreakCandidate::default();
    break_candidate.record_at(
        DisplaySourceTextPosition::new(7, 12),
        3,
        (Some(LispCharPos1::new(10)), Some(LispCharPos1::new(12))),
        DisplayRowGlyphCheckpoint::default(),
        candidate_position,
        Some(candidate_extend),
    );
    let action = BufferSourceWordWrapAction::new(break_candidate);
    let mut row_extend = DisplayRowScopedValue::inactive();
    row_extend.activate(geometry.current_row_marker(), overflow_extend);

    action.restore_row_extend(&mut row_extend, &geometry);

    assert_eq!(action.row_position(), candidate_position);
    assert_eq!(
        row_extend.value_on(&geometry).copied(),
        Some(candidate_extend),
        "word-wrap rollback must restore the predecessor's complete realized extend state"
    );
}

#[test]
fn buffer_text_word_wrap_source_action_applies_transition_state() {
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let mut context = RowTransitionTestContext::new("word-wrap-source-state");
    let mut break_candidate = WordWrapBreakCandidate::default();
    break_candidate.record(
        7,
        12,
        3,
        (Some(LispCharPos1::new(10)), Some(LispCharPos1::new(12))),
        DisplayRowGlyphCheckpoint::default(),
    );
    let action = BufferSourceWordWrapAction::new(break_candidate);
    let mut position = DisplaySourceTextPosition::new(20, 30);
    let mut col = 9;
    let mut x = 88.0;
    let mut row_extend = DisplayRowScopedValue::inactive();
    row_extend.activate(
        geometry.current_row_marker(),
        test_row_extend_face(0x112233, FaceId::new(17)),
    );

    action.apply_before_row_transition(
        &mut context.output_emitter,
        &mut position,
        &mut col,
        &mut row_extend,
        &mut x,
        2.0,
    );

    assert_eq!(position, DisplaySourceTextPosition::new(7, 12));
    assert_eq!(col, 0);
    assert_eq!(x, 2.0);
    assert_eq!(row_extend.value_on(&geometry), None);

    let mut hit_row_range = HitRowRangeTracker::new(4);
    let mut face_scan = FaceScanCheckpoint::initial();
    *face_scan.next_check_mut() = 99;
    let mut final_position = DisplaySourceTextPosition::new(20, 30);
    let mut prefix_request = DisplayRowPrefixRequest::None;
    let mut line_numbers = LineNumberRenderState::new(true, 4, 9);
    let mut hscroll_skip = HorizontalScrollSkipState::new(
        LineWrapMode::Wrap,
        0,
        HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph,
    );
    let mut wrap_state = WordWrapRenderState::new(true);
    wrap_state.allow_after_current_char(' ');
    wrap_state.record_candidate(
        'a',
        0,
        4,
        2,
        (Some(LispCharPos1::new(1)), Some(LispCharPos1::new(1))),
        DisplayRowGlyphCheckpoint::default(),
    );
    let mut trailing_whitespace = TrailingWhitespaceRenderState::new(true, 0x00ff00);
    trailing_whitespace.track_rendered_char(' ', geometry.start_marker_at_x(8.0));
    let DisplaySourceTextCharOverflowAction::WordWrap { transition, .. } =
        DisplaySourceTextCharOverflowAction::for_decision(
            DisplayRowTextOverflowDecision::WordWrap { break_candidate },
        )
    else {
        panic!("expected word wrap transition");
    };

    let continuation = action.apply_after_row_transition_and_prefix(
        DisplayTextRowTransition::BeganNextRow,
        transition,
        &mut final_position,
        &mut hit_row_range,
        &mut face_scan,
        &geometry,
        DisplayRowVisibilityLimit {
            max_rows: 2,
            bottom_y: 64.0,
        },
        DisplayRowTransitionRenderState::new(
            &mut prefix_request,
            true,
            &mut line_numbers,
            &mut hscroll_skip,
            &mut wrap_state,
            &mut trailing_whitespace,
        ),
    );

    assert_eq!(continuation, DisplayRowTransitionContinuation::Continue);
    assert_eq!(final_position, DisplaySourceTextPosition::new(7, 12));
    assert_eq!(hit_row_range.start(), 12);
    assert!(face_scan.should_resolve_at(0));
    assert_eq!(prefix_request, DisplayRowPrefixRequest::Wrap);
    assert!(!wrap_state.has_candidate());
    assert_eq!(
        trailing_whitespace.start_marker(),
        DisplayRowStartMarker::Inactive
    );
}

#[test]
fn word_wrap_break_glyph_checkpoint_rolls_partial_word_off_first_row() {
    // GUI bug repro: with word-wrap on, the chars between the last word boundary
    // and the overflow point (e.g. the `wo` of `word10`) already fit and were
    // pushed to the current row's glyph buffer. The word-wrap break must roll
    // those partial-word glyphs back to the boundary so GNU's "keep whole words"
    // behavior holds. This exercises the capture (at the boundary) /restore (at
    // the break) round-trip through `source_render` on a real builder row.
    let mut context = RowTransitionTestContext::new("word-wrap-glyph-checkpoint");
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let mut font_metrics = None;

    // Draw the first full word and its trailing space onto the row.
    for (offset, ch) in "word09 ".chars().enumerate() {
        write_char_to_current_row_with_width(&mut context.builder, ch, FaceId::new(0), offset, 8.0);
    }

    // At the word boundary (start of the next word `word10`), word-wrap records
    // a candidate. Capture the glyph checkpoint at that exact moment, BEFORE the
    // candidate char is drawn.
    let mut word_wrap = WordWrapRenderState::new(true);
    word_wrap.allow_after_current_char(' ');
    let break_candidate = {
        let source_render = text_row_source_render_state(
            &mut context.builder,
            &mut context.output_emitter,
            &mut context.eval,
            &mut font_metrics,
            &face_resolver,
        );
        DisplaySourceStepChar::new('w', 7, 11)
            .record_word_wrap_candidate(&mut word_wrap, &source_render);
        word_wrap.candidate()
    };
    assert!(break_candidate.is_available());

    // The checkpoint must point at the boundary: 7 text glyphs (`word09 `) drawn.
    {
        let row = context.builder.current_row_for_test().expect("current row");
        assert_eq!(row.glyphs[GlyphArea::Text.index()].len(), 7);
    }

    // Now the partial next word (`wo` of `word10`) fits and gets drawn before the
    // overflow is detected.
    for (offset, ch) in "wo".chars().enumerate() {
        write_char_to_current_row_with_width(
            &mut context.builder,
            ch,
            FaceId::new(0),
            7 + offset,
            8.0,
        );
    }
    {
        let row = context.builder.current_row_for_test().expect("current row");
        assert_eq!(row.glyphs[GlyphArea::Text.index()].len(), 9);
    }

    // The word-wrap break restores the captured glyph checkpoint, rolling the
    // partial word off the first row.
    let action = BufferSourceWordWrapAction::new(break_candidate);
    {
        let mut source_render = text_row_source_render_state(
            &mut context.builder,
            &mut context.output_emitter,
            &mut context.eval,
            &mut font_metrics,
            &face_resolver,
        );
        source_render.restore_glyph_checkpoint(action.glyph_checkpoint());
    }

    // The first row's TEXT glyphs now end at the word boundary: exactly
    // `word09 ` with no partial word.
    let row = context.builder.current_row_for_test().expect("current row");
    let text = &row.glyphs[GlyphArea::Text.index()];
    assert_eq!(text.len(), 7);
    let drawn: String = text
        .iter()
        .filter_map(|glyph| match glyph.glyph_type {
            GlyphType::Char { ch } => Some(ch),
            _ => None,
        })
        .collect();
    assert_eq!(drawn, "word09 ");
    // The last drawn text glyph is the trailing space (the boundary), not part
    // of `word10`.
    assert!(matches!(
        text.last().expect("last text glyph").glyph_type,
        GlyphType::Char { ch: ' ' }
    ));
}

#[test]
fn buffer_text_special_wrap_source_action_applies_transition_state() {
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let action = BufferSourceSpecialWrapAction::new(21);
    let mut row_extend = DisplayRowScopedValue::inactive();
    row_extend.activate(
        geometry.current_row_marker(),
        test_row_extend_face(0x445566, FaceId::new(21)),
    );
    let mut x = 88.0;

    action.apply_before_row_transition(&mut row_extend, &mut x, 3.0);

    assert_eq!(action.charpos(), 21);
    assert_eq!(x, 3.0);
    assert_eq!(row_extend.value_on(&geometry), None);

    let mut hit_row_range = HitRowRangeTracker::new(6);
    let hit_range = action.hit_range_and_advance(&mut hit_row_range);

    assert_eq!(hit_range.charpos_start, 6);
    assert_eq!(hit_range.charpos_end, 21);
    assert_eq!(hit_row_range.start(), 21);
    assert_eq!(
        action.transition_continuation(
            DisplayTextRowTransition::BeganNextRow,
            &geometry,
            DisplayRowVisibilityLimit {
                max_rows: 2,
                bottom_y: 64.0,
            },
        ),
        DisplayRowTransitionContinuation::Continue
    );
}

#[test]
fn buffer_text_special_overflow_render_request_wraps_then_keeps_prepared_append() {
    let mut context = RowTransitionTestContext::new("special-overflow-wrap-request");
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.insert("a");
    }
    let snapshot = current_buffer_snapshot(&eval, buf_id);
    let prepared_append = DisplaySourceSpecialCharPreparedAppend::new(
        DisplaySourceSpecialDisplayKind::Control,
        DisplaySourceSpecialCharAppendPlan::new(
            DisplaySourceItemRequest::new(
                DisplaySourceTextRange::single_char(CharPos0::new(21)),
                DisplaySourceAppendItem::ControlChar { ch: '\n' },
            ),
            DisplayRowPosition::new(80.0, 10),
            buffer_display_item(
                buf_id,
                21,
                22,
                RenderFaceRef::Inherit,
                DisplayItemKind::ControlChar { ch: '\n' },
            ),
        ),
        Some(8.0),
    );
    let text = b"a";
    let mut byte_idx = 0;
    let mut charpos = 21;
    let mut invisible_text_checkpoint = InvisibleTextScanCheckpoint::new(charpos);
    let mut col = 10;
    let mut row_extend = DisplayRowScopedValue::inactive();
    row_extend.activate(
        context.geometry.current_row_marker(),
        test_row_extend_face(0x445566, FaceId::new(21)),
    );
    let mut x = 80.0;
    let mut line_numbers = LineNumberRenderState::new(false, 0, 0);
    let mut hit_row_range = HitRowRangeTracker::new(6);
    let mut prefix_request = DisplayRowPrefixRequest::None;
    let mut hscroll_skip = HorizontalScrollSkipState::new(
        LineWrapMode::Wrap,
        0,
        HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph,
    );
    let mut word_wrap = WordWrapRenderState::new(false);
    let mut trailing_whitespace = TrailingWhitespaceRenderState::new(false, 0);
    let mut face_scan = FaceScanCheckpoint::initial();
    let mut box_face = BoxFaceRowState::inactive();
    let row_limit = context.row_limit;
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let mut font_metrics = None;
    let mut cursor_info = CursorCaptureState::new();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(1);
    let surface = test_advance_resolution_surface();
    let overlay_context = BufferOverlayStringTextRowRenderContext::new(
        false,
        1,
        &surface,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        0.0,
        0,
        4,
    );
    let mut source_walk = BufferSourceWalk::new(buf_id, &snapshot, charpos, 0);

    let outcome = BufferSourceSpecialOverflowRenderRequest::new(
        &prepared_append,
        BufferSourceSpecialOverflowRenderContext::new(
            text,
            0,
            x,
            80.0,
            LineWrapMode::Wrap,
            DisplayRowVisibilityLimit {
                max_rows: 4,
                bottom_y: 64.0,
            },
            0.0,
            false,
            context.defaults,
            0,
            4,
            row_limit,
        ),
    )
    .render_if_needed_and_apply(
        &mut source_walk,
        &snapshot,
        BufferSourceLoopMutableState::new(
            &mut invisible_text_checkpoint,
            DisplaySourceProgressState::new(&mut byte_idx, &mut charpos, &mut x, &mut col),
            text_row_source_render_state(
                &mut context.builder,
                &mut context.output_emitter,
                &mut context.eval,
                &mut font_metrics,
                &face_resolver,
            ),
            BufferSourceRowBuildState::new(
                &mut context.geometry,
                &mut context.row_flags,
                &mut row_extend,
                &mut box_face,
            ),
            BufferSourceHitCaptureState::new(&mut context.hit_rows, &mut hit_row_range),
            BufferSourceRowCarryoverState::new(
                &mut prefix_request,
                &mut line_numbers,
                &mut hscroll_skip,
                &mut word_wrap,
                &mut trailing_whitespace,
            ),
            &mut face_scan,
            &mut context.row_y_positions,
            &mut cursor_info,
            &mut face_ids,
            BufferSourceSurfaceContext::new(&surface, overlay_context),
        ),
    );

    assert_eq!(
        outcome,
        BufferSourceSpecialOverflowRenderOutcome::AppendPrepared(
            DisplayRowTransitionContinuation::Continue
        )
    );
    assert_eq!(byte_idx, 0);
    assert_eq!(charpos, 21);
    assert_eq!(hit_row_range.start(), 21);
    assert_eq!(x, 0.0);
    assert_eq!(col, 0);
    assert_eq!(row_extend.value_on(&context.geometry), None);
}

#[test]
fn buffer_text_character_wrap_source_action_rewinds_to_current_char_start() {
    let action = BufferSourceCharacterWrapAction::new(13, 21);
    let mut position = DisplaySourceTextPosition::new(17, 22);

    action.rewind_source_state(&mut position);

    assert_eq!(
        action.source_position(),
        DisplaySourceTextPosition::new(13, 21)
    );
    assert_eq!(position, DisplaySourceTextPosition::new(13, 21));
}

#[test]
fn buffer_text_character_wrap_source_action_rewinds_source_step_char() {
    let source_char = DisplaySourceStepChar::new('界', "a".len(), 9);
    let action = BufferSourceCharacterWrapAction::from_source_step_char(source_char);
    let mut rewind_position = DisplaySourceTextPosition::new("a界".len(), 10);

    action.rewind_source_state(&mut rewind_position);

    assert_eq!(
        rewind_position,
        DisplaySourceTextPosition::new("a".len(), 9)
    );
}

#[test]
fn buffer_text_character_wrap_source_action_applies_transition_state() {
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let action = BufferSourceCharacterWrapAction::new(13, 21);
    let mut row_extend = DisplayRowScopedValue::inactive();
    row_extend.activate(
        geometry.current_row_marker(),
        test_row_extend_face(0x445566, FaceId::new(21)),
    );
    let mut x = 88.0;

    action.apply_before_row_transition(&mut row_extend, &mut x, 3.0);

    assert_eq!(x, 3.0);
    assert_eq!(row_extend.value_on(&geometry), None);

    let mut position = DisplaySourceTextPosition::new(17, 22);
    let mut hit_row_range = HitRowRangeTracker::new(6);
    let mut face_scan = FaceScanCheckpoint::initial();
    *face_scan.next_check_mut() = 99;

    let continuation = action.apply_after_visible_row_transition(
        DisplayTextRowTransition::BeganNextRow,
        &mut position,
        &mut hit_row_range,
        &mut face_scan,
        &geometry,
        DisplayRowVisibilityLimit {
            max_rows: 2,
            bottom_y: 64.0,
        },
    );

    assert_eq!(continuation, DisplayRowTransitionContinuation::Continue);
    assert_eq!(position, DisplaySourceTextPosition::new(13, 21));
    assert_eq!(hit_row_range.start(), 21);
    assert!(face_scan.should_resolve_at(0));
}

#[test]
fn buffer_text_character_wrap_source_action_skips_state_when_transition_exhausted() {
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let action = BufferSourceCharacterWrapAction::new(13, 21);
    let mut position = DisplaySourceTextPosition::new(17, 22);
    let mut hit_row_range = HitRowRangeTracker::new(6);
    let mut face_scan = FaceScanCheckpoint::initial();
    *face_scan.next_check_mut() = 99;

    let continuation = action.apply_after_visible_row_transition(
        DisplayTextRowTransition::ExhaustedRows,
        &mut position,
        &mut hit_row_range,
        &mut face_scan,
        &geometry,
        DisplayRowVisibilityLimit {
            max_rows: 2,
            bottom_y: 64.0,
        },
    );

    assert_eq!(continuation, DisplayRowTransitionContinuation::Exhausted);
    assert_eq!(position, DisplaySourceTextPosition::new(17, 22));
    assert_eq!(hit_row_range.start(), 6);
    assert!(!face_scan.should_resolve_at(0));
}

#[test]
fn buffer_text_character_wrap_source_action_reports_hidden_after_state_sync() {
    let geometry = DisplayRowGeometryState::new(0, 64.0, 0.0, 16.0, 12.0);
    let action = BufferSourceCharacterWrapAction::new(13, 21);
    let mut position = DisplaySourceTextPosition::new(17, 22);
    let mut hit_row_range = HitRowRangeTracker::new(6);
    let mut face_scan = FaceScanCheckpoint::initial();
    *face_scan.next_check_mut() = 99;

    let continuation = action.apply_after_visible_row_transition(
        DisplayTextRowTransition::BeganNextRow,
        &mut position,
        &mut hit_row_range,
        &mut face_scan,
        &geometry,
        DisplayRowVisibilityLimit {
            max_rows: 2,
            bottom_y: 64.0,
        },
    );

    assert_eq!(continuation, DisplayRowTransitionContinuation::Hidden);
    assert_eq!(position, DisplaySourceTextPosition::new(13, 21));
    assert_eq!(hit_row_range.start(), 21);
    assert!(face_scan.should_resolve_at(0));
}

#[test]
fn buffer_text_overflow_render_request_handles_character_wrap_transition() {
    let mut context = RowTransitionTestContext::new("text-overflow-character-wrap-request");
    let buf_id = context
        .eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let text = b"a";
    let mut byte_idx = 0;
    let source_step_char = DisplaySourceStepChar::new('a', 0, 21);
    let prepared_append =
        DisplaySourceTextCharPreparedAppend::new(DisplaySourceTextCharAppendPlan::new(
            DisplaySourceTextRequest::new(
                DisplaySourceTextRange::single_char(CharPos0::new(21)),
                'a',
                DisplaySourceAppendRenderPlan::resolved_advance(8.0),
            ),
            DisplayRowPosition::new(80.0, 10),
            buffer_source_mapped_display_item(buf_id, 21, 22, "a", RenderFaceRef::Inherit),
        ));
    let mut charpos = 21;
    let mut invisible_text_checkpoint = InvisibleTextScanCheckpoint::new(charpos);
    let mut col = 10;
    let mut row_extend = DisplayRowScopedValue::inactive();
    row_extend.activate(
        context.geometry.current_row_marker(),
        test_row_extend_face(0x445566, FaceId::new(21)),
    );
    let mut x = 80.0;
    let mut line_numbers = LineNumberRenderState::new(false, 0, 0);
    let mut hit_row_range = HitRowRangeTracker::new(6);
    let mut prefix_request = DisplayRowPrefixRequest::None;
    let mut hscroll_skip = HorizontalScrollSkipState::new(
        LineWrapMode::Wrap,
        0,
        HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph,
    );
    let mut word_wrap = WordWrapRenderState::new(false);
    let mut trailing_whitespace = TrailingWhitespaceRenderState::new(false, 0);
    let mut face_scan = FaceScanCheckpoint::initial();
    *face_scan.next_check_mut() = 99;
    let mut box_face = BoxFaceRowState::inactive();
    let row_limit = context.row_limit;
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let mut font_metrics = None;
    let mut cursor_info = CursorCaptureState::new();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(1);
    let surface = test_advance_resolution_surface();
    let overlay_context = BufferOverlayStringTextRowRenderContext::new(
        false,
        1,
        &surface,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        0.0,
        0,
        4,
    );
    let snapshot = current_buffer_snapshot(&context.eval, buf_id);
    let mut source_walk = BufferSourceWalk::new(buf_id, &snapshot, charpos, 0);

    let outcome = BufferSourceOverflowRenderRequest::new(
        &prepared_append,
        source_step_char,
        BufferSourceOverflowRenderContext::new(
            'a',
            80.0,
            crate::display_row::append_context::RightEdgeMarkerColumn::NotReserved,
            LineWrapMode::Wrap,
            word_wrap,
            DisplayRowVisibilityLimit {
                max_rows: 4,
                bottom_y: 64.0,
            },
            0.0,
            false,
            context.defaults,
            0,
            4,
            row_limit,
            DisplayRowMeasuredFaceMetrics::new(8.0, 16.0, 12.0, 8.0),
            Color::from_pixel(0x00FFFFFF),
        ),
    )
    .render_if_needed_and_apply(
        &mut source_walk,
        text,
        BufferSourceLoopMutableState::new(
            &mut invisible_text_checkpoint,
            DisplaySourceProgressState::new(&mut byte_idx, &mut charpos, &mut x, &mut col),
            text_row_source_render_state(
                &mut context.builder,
                &mut context.output_emitter,
                &mut context.eval,
                &mut font_metrics,
                &face_resolver,
            ),
            BufferSourceRowBuildState::new(
                &mut context.geometry,
                &mut context.row_flags,
                &mut row_extend,
                &mut box_face,
            ),
            BufferSourceHitCaptureState::new(&mut context.hit_rows, &mut hit_row_range),
            BufferSourceRowCarryoverState::new(
                &mut prefix_request,
                &mut line_numbers,
                &mut hscroll_skip,
                &mut word_wrap,
                &mut trailing_whitespace,
            ),
            &mut face_scan,
            &mut context.row_y_positions,
            &mut cursor_info,
            &mut face_ids,
            BufferSourceSurfaceContext::new(&surface, overlay_context),
        ),
    );

    assert_eq!(
        outcome,
        BufferSourceOverflowRenderOutcome::Transition(DisplayRowTransitionContinuation::Continue)
    );
    assert_eq!(byte_idx, 0);
    assert_eq!(charpos, 21);
    assert_eq!(hit_row_range.start(), 21);
    assert_eq!(x, 0.0);
    assert!(face_scan.should_resolve_at(0));
    assert_eq!(row_extend.value_on(&context.geometry), None);
}

#[test]
fn display_row_transition_render_state_applies_overflow_wrap_policy() {
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let mut prefix_request = DisplayRowPrefixRequest::None;
    let mut line_numbers = LineNumberRenderState::new(true, 4, 9);
    let mut hscroll_skip = HorizontalScrollSkipState::new(
        LineWrapMode::Wrap,
        0,
        HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph,
    );
    let mut word_wrap = WordWrapRenderState::new(true);
    word_wrap.allow_after_current_char(' ');
    word_wrap.record_candidate(
        'a',
        0,
        4,
        2,
        (Some(LispCharPos1::new(1)), Some(LispCharPos1::new(1))),
        DisplayRowGlyphCheckpoint::default(),
    );
    let break_candidate = word_wrap.candidate();
    let mut trailing_whitespace = TrailingWhitespaceRenderState::new(true, 0x00ff00);
    trailing_whitespace.track_rendered_char(' ', geometry.start_marker_at_x(8.0));
    let col = 7;
    let DisplaySourceTextCharOverflowAction::WordWrap { transition, .. } =
        DisplaySourceTextCharOverflowAction::for_decision(
            DisplayRowTextOverflowDecision::WordWrap { break_candidate },
        )
    else {
        panic!("expected word wrap transition");
    };

    DisplayRowTransitionRenderState::new(
        &mut prefix_request,
        true,
        &mut line_numbers,
        &mut hscroll_skip,
        &mut word_wrap,
        &mut trailing_whitespace,
    )
    .apply_overflow_prefix(transition);

    assert_eq!(col, 7);
    assert_eq!(prefix_request, DisplayRowPrefixRequest::Wrap);
    assert_eq!(line_numbers.current_line(), 4);
    assert!(!hscroll_skip.should_skip());
    assert!(!word_wrap.has_candidate());
    assert_eq!(
        trailing_whitespace.start_marker(),
        DisplayRowStartMarker::Inactive
    );
}

#[test]
fn display_row_overflow_transition_request_marks_truncated_row_and_emits_boundary() {
    let mut ctx = RowTransitionTestContext::new("overflow-truncation-request");

    let transition = DisplayRowOverflowTransitionRequest::truncation(
        DisplayRowHitRange {
            charpos_start: 3,
            charpos_end: 9,
        },
        ctx.defaults,
        0,
        6,
        48.0,
        ctx.row_y_positions.recording(),
        4,
    )
    .emit_with_output(
        &mut ctx.geometry,
        &mut ctx.row_flags,
        ctx.row_limit,
        &mut ctx.hit_rows,
        text_row_output_render_state(&mut ctx.builder, &mut ctx.output_emitter, &mut ctx.eval),
    );

    assert_eq!(transition, DisplayTextRowTransition::BeganNextRow);
    assert_eq!(ctx.geometry.row(), 1);
    assert_eq!(ctx.hit_rows.len(), 1);
    assert_eq!(ctx.hit_rows[0].charpos_start, 3);
    assert_eq!(ctx.hit_rows[0].charpos_end, 9);
    assert!(ctx.row_flags.is_set(0, DisplayRowFlagKind::Truncated));
    assert!(!ctx.row_flags.is_set(0, DisplayRowFlagKind::Continued));
    assert!(!ctx.row_flags.is_set(1, DisplayRowFlagKind::Continuation));
    assert_eq!(ctx.row_y_positions.recorded(), &[0.0, 16.0]);
}

#[test]
fn display_row_overflow_transition_request_marks_visual_wrap_rows_and_emits_boundary() {
    let mut ctx = RowTransitionTestContext::new("overflow-visual-wrap-request");

    let transition = DisplayRowOverflowTransitionRequest::visual_wrap(
        VisualWrapBreak::MidElement,
        DisplayRowHitRange {
            charpos_start: 3,
            charpos_end: 9,
        },
        ctx.defaults,
        0,
        6,
        48.0,
        ctx.row_y_positions.recording(),
        4,
    )
    .emit_with_output(
        &mut ctx.geometry,
        &mut ctx.row_flags,
        ctx.row_limit,
        &mut ctx.hit_rows,
        text_row_output_render_state(&mut ctx.builder, &mut ctx.output_emitter, &mut ctx.eval),
    );

    assert_eq!(transition, DisplayTextRowTransition::BeganNextRow);
    assert_eq!(ctx.geometry.row(), 1);
    assert_eq!(ctx.hit_rows.len(), 1);
    assert_eq!(ctx.hit_rows[0].charpos_start, 3);
    assert_eq!(ctx.hit_rows[0].charpos_end, 9);
    assert!(ctx.row_flags.is_set(0, DisplayRowFlagKind::Continued));
    assert!(
        ctx.row_flags
            .is_set(0, DisplayRowFlagKind::ContinuedMidElement)
    );
    assert!(ctx.row_flags.is_set(1, DisplayRowFlagKind::Continuation));
    assert!(!ctx.row_flags.is_set(0, DisplayRowFlagKind::Truncated));
    assert_eq!(ctx.row_y_positions.recorded(), &[0.0, 16.0]);
}

fn test_active_face_state(face_id: FaceId, char_width: f32) -> DisplayRowActiveFaceState {
    test_active_face_state_with_extend(face_id, char_width, false)
}

fn test_row_extend_face(background: u32, face_id: FaceId) -> DisplayRowExtendFace {
    DisplayRowExtendFace::new(
        Color::from_pixel(background),
        face_id,
        DisplayRowMeasuredFaceMetrics::new(8.0, 16.0, 12.0, 8.0),
    )
}

fn test_active_face_state_with_extend(
    face_id: FaceId,
    char_width: f32,
    extend: bool,
) -> DisplayRowActiveFaceState {
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let mut base = resolver.default_face().clone();
    base.set_measured_char_width_px(char_width);
    base.extend = extend;
    let mut font_metrics = None;
    let measured = DisplayRowMeasurementPolicy::for_mode(DisplayRowMeasurementMode::LogicalCells)
        .measured_face(
            face_id,
            &base,
            None,
            char_width,
            DisplayRowFallbackMetrics {
                char_width,
                row_height: 18.0,
                ascent: 13.0,
            },
            &mut font_metrics,
        );
    DisplayRowActiveFaceState::new(base, measured)
}

fn test_append_frame(
    char_width: f32,
    space_width: f32,
    tab_policy: DisplayTabPolicy,
) -> DisplayRowAppendFrame {
    test_append_frame_at(
        0,
        0.0,
        0.0,
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayRowAppendMetrics::new(
            16.0,
            12.0,
            char_width,
            space_width,
            DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        ),
        tab_policy,
    )
}

fn test_append_frame_at(
    row: usize,
    y: f32,
    glyph_y: f32,
    area: DisplayRowAppendArea,
    metrics: DisplayRowAppendMetrics,
    tab_policy: DisplayTabPolicy,
) -> DisplayRowAppendFrame {
    let surface = DisplayRowAppendSurface::new(area, tab_policy);
    let geometry = DisplayRowGeometryState::new(row, y, 0.0, metrics.height(), metrics.ascent());
    surface.frame_from_geometry_state(&geometry, glyph_y - y, metrics)
}

fn test_advance_resolution_surface() -> DisplayRowAppendSurface {
    DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayTabPolicy::every(8),
    )
}

#[test]
fn fallback_display_source_natural_measurement_uses_frame_tab_policy() {
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let frame = test_append_frame(8.0, 8.0, DisplayTabPolicy::every(4));
    let mut font_metrics = None;

    let advance = DisplayRowTextNaturalAdvanceKind::for_cluster_state(
        DisplaySourceClusterState::for_char('\t', None),
    )
    .resolve_to_text_row(
        &mut font_metrics,
        &active_face,
        &frame,
        DisplayRowPosition::new(8.0, 1),
        '\t',
    );

    assert_eq!(advance, 24.0);
}

#[test]
fn fallback_display_source_natural_measurement_zeroes_cluster_continuation() {
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let frame = test_append_frame(8.0, 8.0, DisplayTabPolicy::every(8));
    let mut font_metrics = None;

    let advance = DisplayRowTextNaturalAdvanceKind::for_cluster_state(
        DisplaySourceClusterState::for_char('\u{301}', Some(('e', false))),
    )
    .resolve_to_text_row(
        &mut font_metrics,
        &active_face,
        &frame,
        DisplayRowPosition::new(8.0, 1),
        '\u{301}',
    );

    assert_eq!(advance, 0.0);
}

#[test]
fn fallback_display_source_natural_measurement_uses_face_columns() {
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let frame = test_append_frame(8.0, 8.0, DisplayTabPolicy::every(8));
    let mut font_metrics = None;

    let advance = DisplayRowTextNaturalAdvanceKind::for_cluster_state(
        DisplaySourceClusterState::for_char('中', None),
    )
    .resolve_to_text_row(
        &mut font_metrics,
        &active_face,
        &frame,
        DisplayRowPosition::new(0.0, 0),
        '中',
    );

    assert_eq!(advance, 16.0);
}

#[test]
fn display_row_text_natural_advance_kind_names_width_policy() {
    assert_eq!(
        DisplayRowTextNaturalAdvanceKind::for_cluster_state(DisplaySourceClusterState::for_char(
            '\t', None
        ),),
        DisplayRowTextNaturalAdvanceKind::Tab
    );
    assert_eq!(
        DisplayRowTextNaturalAdvanceKind::for_cluster_state(DisplaySourceClusterState::for_char(
            '\u{301}',
            Some(('e', false))
        ),),
        DisplayRowTextNaturalAdvanceKind::ClusterContinuation
    );
    assert_eq!(
        DisplayRowTextNaturalAdvanceKind::for_cluster_state(DisplaySourceClusterState::for_char(
            'x', None
        ),),
        DisplayRowTextNaturalAdvanceKind::FaceColumns { columns: 1 }
    );
    assert_eq!(
        DisplayRowTextNaturalAdvanceKind::for_cluster_state(DisplaySourceClusterState::for_char(
            '中', None
        ),),
        DisplayRowTextNaturalAdvanceKind::FaceColumns { columns: 2 }
    );
}

#[test]
fn display_source_natural_measurement_request_names_source_and_fallback() {
    let request = DisplaySourceNaturalMeasurementRequest::for_range_and_cluster(
        DisplaySourceTextRange::new(CharPos0::new(2), CharPos0::new(3)),
        DisplaySourceClusterState::for_char('中', None),
    );

    assert_eq!(
        request.source_item(),
        DisplaySourceTextItemRequest::new(
            DisplaySourceTextRange::new(CharPos0::new(2), CharPos0::new(3)),
            '中'
        )
    );
    assert_eq!(
        request.fallback(),
        DisplayRowTextNaturalAdvanceKind::FaceColumns { columns: 2 }
    );
}

#[test]
fn display_source_append_measurement_kind_names_measurement_mode() {
    assert_eq!(
        DisplaySourceAppendMeasurementKind::for_char('x'),
        DisplaySourceAppendMeasurementKind::NaturalRenderedSource
    );
    assert_eq!(
        DisplaySourceAppendMeasurementKind::for_char('\t'),
        DisplaySourceAppendMeasurementKind::NaturalRenderedSource
    );
    assert_eq!(
        DisplaySourceAppendMeasurementKind::for_char('\u{301}'),
        DisplaySourceAppendMeasurementKind::NaturalRenderedSource
    );
    assert_eq!(
        DisplaySourceAppendMeasurementKind::for_char('中'),
        DisplaySourceAppendMeasurementKind::NaturalRenderedSource
    );
    assert_eq!(
        DisplaySourceAppendMeasurementKind::for_char('\u{0633}'),
        DisplaySourceAppendMeasurementKind::ResolvedComplexRun
    );
}

fn current_buffer_snapshot(eval: &Context, buf_id: BufferId) -> LayoutBufferSnapshot {
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    LayoutBufferSnapshot::from_buffer(buffer)
}

#[test]
fn buffer_text_source_range_append_requests_preserve_source_and_kind() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.insert("\tA");
    }
    let snapshot = current_buffer_snapshot(&eval, buf_id);

    let tab_request = buffer_source_text_item_append_request(
        DisplaySourceTextItemRequest::new(
            DisplaySourceTextRange::new(CharPos0::new(0), CharPos0::new(1)),
            '\t',
        ),
        buf_id,
        &snapshot,
        FaceId::new(7),
    )
    .expect("tab append request");
    assert_eq!(tab_request.append_kind(), DisplayRowAppendKind::Tab);
    let tab_item = tab_request.into_item();
    assert_eq!(tab_item.face, RenderFaceRef::FaceId(FaceId::new(7)));
    assert_eq!(
        tab_item.span.start,
        DisplaySourcePosition::buffer(buf_id, CharPos0::new(0), EmacsBytePos::new(0))
    );
    assert!(matches!(
        &tab_item.kind,
        DisplayItemKind::TextRun(run) if run.text.as_ref() == "\t"
    ));

    let mapped_request = buffer_source_item_append_request(
        DisplaySourceItemRequest::new(
            DisplaySourceTextRange::new(CharPos0::new(1), CharPos0::new(2)),
            DisplaySourceAppendItem::SourceMappedText { text: "x".into() },
        ),
        buf_id,
        &snapshot,
        FaceId::new(9),
    )
    .expect("mapped append request");
    assert_eq!(
        mapped_request.append_kind(),
        DisplayRowAppendKind::SourceMappedText
    );
    let mapped_item = mapped_request.into_item();
    assert_eq!(mapped_item.face, RenderFaceRef::FaceId(FaceId::new(9)));
    assert_eq!(
        mapped_item.span.start,
        DisplaySourcePosition::buffer(buf_id, CharPos0::new(1), EmacsBytePos::new(1))
    );
    assert!(matches!(
        &mapped_item.kind,
        DisplayItemKind::SourceMappedText(text) if text.text.as_ref() == "x"
    ));
}

#[test]
fn buffer_text_source_text_request_uses_source_step_char_payload() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.insert("a");
    }
    let snapshot = current_buffer_snapshot(&eval, buf_id);

    let request = DisplaySourceTextRequest::new(
        DisplaySourceTextRange::new(CharPos0::new(0), CharPos0::new(1)),
        'z',
        DisplaySourceAppendRenderPlan::natural(8.0),
    )
    .append_request(buf_id, &snapshot, FaceId::new(7))
    .expect("append request");

    assert_eq!(request.append_kind(), DisplayRowAppendKind::SourceText);
    let item = request.into_item();
    assert_eq!(
        item.span.start,
        DisplaySourcePosition::buffer(buf_id, CharPos0::new(0), EmacsBytePos::new(0))
    );
    assert!(matches!(
        &item.kind,
        DisplayItemKind::TextRun(run) if run.text.as_ref() == "z"
    ));
}

#[test]
fn buffer_text_source_append_context_resolves_natural_measurement_for_ascii() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let snapshot = current_buffer_snapshot(&eval, buf_id);
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let surface = test_advance_resolution_surface();
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let mut font_metrics = None;
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    let mut append_state = DisplaySourceRowAppendState::default();
    let append_context = BufferSourceRowAppendContext::new(
        &snapshot,
        buf_id,
        &surface,
        &active_face,
        0.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
    );
    let source_item =
        buffer_source_mapped_display_item(buf_id, 0, 1, "x", RenderFaceRef::FaceId(FaceId::new(7)));

    let resolved = append_context.resolve_source_render_plan_to_text_row(
        &geometry,
        &mut append_state,
        &mut text_row_source_measure_state(
            &mut builder,
            &mut eval,
            &mut font_metrics,
            &face_resolver,
        ),
        DisplaySourceRenderPlanRequest::new(
            b"x",
            0,
            DisplaySourceTextRange::new(CharPos0::new(0), CharPos0::new(1)),
            DisplaySourceClusterState::for_char('x', None),
        ),
        DisplayRowPosition::new(0.0, 0),
        &source_item,
    );

    assert_eq!(resolved.advance_px(), 8.0);
    assert_eq!(resolved, DisplaySourceAppendRenderPlan::natural(8.0));
}

#[test]
fn buffer_text_source_append_context_resolves_complex_text_measurement() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let snapshot = current_buffer_snapshot(&eval, buf_id);
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let surface = test_advance_resolution_surface();
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let mut font_metrics = None;
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    let mut append_state = DisplaySourceRowAppendState::default();
    let append_context = BufferSourceRowAppendContext::new(
        &snapshot,
        buf_id,
        &surface,
        &active_face,
        0.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
    );
    let source_item = buffer_source_mapped_display_item(
        buf_id,
        0,
        1,
        "\u{0633}",
        RenderFaceRef::FaceId(FaceId::new(7)),
    );

    let resolved = append_context.resolve_source_render_plan_to_text_row(
        &geometry,
        &mut append_state,
        &mut text_row_source_measure_state(
            &mut builder,
            &mut eval,
            &mut font_metrics,
            &face_resolver,
        ),
        DisplaySourceRenderPlanRequest::new(
            "\u{0633}".as_bytes(),
            0,
            DisplaySourceTextRange::new(CharPos0::new(0), CharPos0::new(1)),
            DisplaySourceClusterState::for_char('\u{0633}', None),
        ),
        DisplayRowPosition::new(0.0, 0),
        &source_item,
    );

    assert_eq!(resolved.advance_px(), 8.0);
    assert_eq!(
        resolved,
        DisplaySourceAppendRenderPlan::resolved_advance(8.0)
    );
}

#[test]
fn synthetic_display_text_item_builds_synthetic_text_run() {
    let mut source = crate::display_source::SyntheticTextItemSource::new(
        9,
        "...",
        RenderFaceRef::FaceId(FaceId::new(7)),
        0,
    );
    let item = source
        .next_item(&mut crate::display_source::DisplaySourceContext::empty())
        .expect("synthetic item");

    assert_eq!(item.face, RenderFaceRef::FaceId(FaceId::new(7)));
    assert_eq!(item.span.start, DisplaySourcePosition::synthetic(9, 0));
    assert_eq!(item.span.end, DisplaySourcePosition::synthetic(9, 3));
    match item.kind {
        DisplayItemKind::TextRun(run) => assert_eq!(&*run.text, "..."),
        other => panic!("expected text run, got {other:?}"),
    }
}

#[test]
fn display_row_append_frame_builds_from_geometry_state() {
    let geometry = DisplayRowGeometryState::new(2, 40.0, 0.0, 18.0, 13.0);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(10.0, 90.0, 120.0, 6.0),
        DisplayTabPolicy::every(4),
    );

    let frame = surface.frame_from_geometry_state(
        &geometry,
        3.0,
        DisplayRowAppendMetrics::new(
            18.0,
            13.0,
            7.0,
            8.0,
            DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        ),
    );

    assert_eq!(frame.row(), 2);
    assert_eq!(frame.glyph_y(), 43.0);
    assert_eq!(frame.geometry().y(), 40.0);
    assert_eq!(frame.geometry().width(), 90.0);
    assert_eq!(frame.geometry().height(), 18.0);
    assert_eq!(frame.default_row_height(), 16.0);
    assert_eq!(frame.content_x(), 10.0);
    assert_eq!(frame.text_width(), 120.0);
    assert_eq!(frame.line_number_width(), 6.0);
}

#[test]
fn synthetic_text_append_context_renders_fragment_and_emits_slots() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("append-synthetic-text", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);
    let table = neovm_core::face::FaceTable::new();
    let face_resolver =
        crate::neovm_bridge::FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let mut font_metrics = None;

    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let append_context = SyntheticTextRowAppendContext::new(
        &surface,
        &geometry,
        &active_face,
        0.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
    );
    let progress = append_context
        .append_request_to_text_row_and_emit(
            &mut text_row_source_render_state(
                &mut builder,
                &mut output_emitter,
                &mut eval,
                &mut font_metrics,
                &face_resolver,
            ),
            SyntheticTextAppendRequest::active_source(
                DisplayRowPosition::new(0.0, 0),
                SyntheticTextSource::new(99, "..."),
            ),
        )
        .expect("synthetic text progress");
    let end = progress.end();

    assert_eq!(end, DisplayRowPosition::new(24.0, 3));
    assert_eq!(progress.metrics().width_px(), 24.0);
    assert_eq!(progress.metrics().width_cols(), 3);
    assert_eq!(progress.slots().len(), 3);
    assert_eq!(
        progress.slots()[0].source(),
        DisplaySourcePosition::synthetic(99, 0)
    );
    builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[1];
            assert_eq!(text.len(), 3);
            assert!(text.iter().all(|glyph| glyph.face_id == FaceId::new(7)));
            assert!(
                text.iter()
                    .all(|glyph| matches!(glyph.glyph_type, GlyphType::Char { ch: '.' }))
            );
        })
        .expect("current row");
}

#[test]
fn synthetic_text_marker_names_source_ids_and_text() {
    assert_eq!(SyntheticTextMarker::InvisibleEllipsis.source_id(), 3);
    assert_eq!(SyntheticTextMarker::InvisibleEllipsis.text(), "...");
    assert_eq!(SyntheticTextMarker::HscrollTruncation.source_id(), 4);
    assert_eq!(SyntheticTextMarker::HscrollTruncation.text(), "$");
    assert_eq!(SyntheticTextMarker::SelectiveEllipsis.source_id(), 5);
    assert_eq!(SyntheticTextMarker::SelectiveEllipsis.text(), "...");
}

#[test]
fn buffer_synthetic_text_render_context_renders_active_marker() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("append-synthetic-active-marker", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);

    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let mut font_metrics = None;
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);

    let end = BufferSyntheticTextRenderContext::new(
        &surface,
        &active_face,
        0.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
    )
    .render_active_marker_to_text_row(
        &mut text_row_source_render_state(
            &mut builder,
            &mut output_emitter,
            &mut eval,
            &mut font_metrics,
            &face_resolver,
        ),
        &geometry,
        DisplayRowPosition::new(0.0, 0),
        SyntheticTextMarker::InvisibleEllipsis,
    )
    .expect("active marker end position");

    assert_eq!(end, DisplayRowPosition::new(24.0, 3));
    builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[GlyphArea::Text.index()];
            assert_eq!(text.len(), 3);
            assert!(text.iter().all(|glyph| glyph.face_id == FaceId::new(7)));
        })
        .expect("current row");
}

#[test]
fn buffer_synthetic_text_render_context_renders_hscroll_marker() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("append-synthetic-hscroll-marker", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);

    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let mut font_metrics = None;
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);

    let end = BufferSyntheticTextRenderContext::new(
        &surface,
        &active_face,
        0.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
    )
    .render_hscroll_truncation_marker_to_text_row(
        &mut text_row_source_render_state(
            &mut builder,
            &mut output_emitter,
            &mut eval,
            &mut font_metrics,
            &face_resolver,
        ),
        &geometry,
        0.0,
    )
    .expect("hscroll marker end position");

    assert_eq!(end, DisplayRowPosition::new(8.0, 1));
    builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[GlyphArea::Text.index()];
            assert_eq!(text.len(), 1);
            assert!(matches!(text[0].glyph_type, GlyphType::Char { ch: '$' }));
            assert_eq!(text[0].face_id, FaceId::new(0));
        })
        .expect("current row");
}

#[test]
fn display_row_prefix_source_builds_append_request_with_prefix_source_id() {
    let _eval = Context::new();
    let value = Value::string("=>");
    let source = DisplayRowPrefixRequest::Line
        .source_for_value(value, CharPos0::new(4))
        .expect("prefix source");

    let request = source
        .append_request(DisplayRowPosition::new(10.0, 2))
        .expect("string prefix append request");

    assert_eq!(request.value, value);
    assert_eq!(request.source_id, LispStringSourceId::PREFIX);
    assert_eq!(request.position, DisplayRowPosition::new(10.0, 2));
}

#[test]
fn buffer_line_prefix_render_context_renders_default_prefix_and_clears_request() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.insert("abc");
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("append-line-prefix-context", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);

    let snapshot = current_buffer_snapshot(&eval, buf_id);
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let mut font_metrics = None;
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let values = DisplayRowPrefixValues::default_values(Some(Value::string("=>")), None);
    let mut prefix_request = DisplayRowPrefixRequest::Line;
    let params = test_display_space_window_params();

    let end = BufferLinePrefixRenderRequest::new(
        values,
        &surface,
        &geometry,
        &active_face,
        0.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        DisplayRowPosition::new(0.0, 0),
        &params,
    )
    .render_requested_to_text_row_and_emit(
        &mut prefix_request,
        &mut text_row_source_render_state(
            &mut builder,
            &mut output_emitter,
            &mut eval,
            &mut font_metrics,
            &face_resolver,
        ),
        &snapshot,
        0,
        &mut face_ids,
    );

    assert_eq!(prefix_request, DisplayRowPrefixRequest::None);
    assert_eq!(end, DisplayRowPosition::new(16.0, 2));
    builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[GlyphArea::Text.index()];
            assert_eq!(text.len(), 2);
            assert!(matches!(text[0].glyph_type, GlyphType::Char { ch: '=' }));
            assert!(matches!(text[1].glyph_type, GlyphType::Char { ch: '>' }));
            assert_eq!(text[0].face_id, FaceId::new(0));
            assert_eq!(text[1].face_id, FaceId::new(0));
        })
        .expect("current row");
}

#[test]
fn buffer_line_prefix_render_context_appends_gnu_space_align_to_prefix() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.insert("ISSUE-170-CENTERED");
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("append-space-line-prefix", 248, 34, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);

    let snapshot = current_buffer_snapshot(&eval, buf_id);
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let active_face = test_active_face_state(FaceId::new(7), 1.0);
    let mut font_metrics = None;
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 248, Rect::new(0.0, 0.0, 248.0, 34.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 248.0, 248.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 1.0, 1.0);
    let prefix = Value::list(vec![
        Value::symbol("space"),
        Value::keyword(":align-to"),
        Value::list(vec![
            Value::symbol("-"),
            Value::symbol("center"),
            Value::fixnum(9),
        ]),
    ]);
    let values = DisplayRowPrefixValues::default_values(Some(prefix), None);
    let mut prefix_request = DisplayRowPrefixRequest::Line;
    let mut params = test_display_space_window_params();
    params.bounds = Rect::new(0.0, 0.0, 248.0, 34.0);
    params.text_bounds = params.bounds;
    params.char_width = 1.0;
    params.char_height = 1.0;
    params.font_ascent = 1.0;
    params.window_system = false;

    let end = BufferLinePrefixRenderRequest::new(
        values,
        &surface,
        &geometry,
        &active_face,
        0.0,
        DisplayRowFallbackMetrics::from_default_face_extents(1.0, 1.0, 1.0),
        DisplayRowPosition::new(0.0, 0),
        &params,
    )
    .render_requested_to_text_row_and_emit(
        &mut prefix_request,
        &mut text_row_source_render_state(
            &mut builder,
            &mut output_emitter,
            &mut eval,
            &mut font_metrics,
            &face_resolver,
        ),
        &snapshot,
        0,
        &mut face_ids,
    );

    assert_eq!(prefix_request, DisplayRowPrefixRequest::None);
    assert_eq!(end, DisplayRowPosition::new(115.0, 115));
    builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[GlyphArea::Text.index()];
            assert_eq!(text.len(), 1);
            assert_eq!(text[0].pixel_width, 115.0);
            assert!(matches!(
                text[0].glyph_type,
                GlyphType::Stretch { width_cols: 115 }
            ));
        })
        .expect("current row");
}

#[test]
fn buffer_line_prefix_render_request_applies_rendered_position() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.insert("abc");
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("append-line-prefix-request", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);

    let snapshot = current_buffer_snapshot(&eval, buf_id);
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let mut font_metrics = None;
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let values = DisplayRowPrefixValues::default_values(Some(Value::string("=>")), None);
    let mut prefix_request = DisplayRowPrefixRequest::Line;
    let mut x = 0.0;
    let mut col = 0;
    let params = test_display_space_window_params();

    {
        let mut source_render = text_row_source_render_state(
            &mut builder,
            &mut output_emitter,
            &mut eval,
            &mut font_metrics,
            &face_resolver,
        );
        BufferLinePrefixRenderRequest::new(
            values,
            &surface,
            &geometry,
            &active_face,
            0.0,
            DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
            DisplayRowPosition::new(x, col),
            &params,
        )
        .render_requested_with_source_state_and_apply(
            &mut prefix_request,
            &mut source_render,
            &snapshot,
            0,
            &mut face_ids,
            &mut x,
            &mut col,
        );
    }

    assert_eq!(prefix_request, DisplayRowPrefixRequest::None);
    assert_eq!(x, 16.0);
    assert_eq!(col, 2);
    builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[GlyphArea::Text.index()];
            assert_eq!(text.len(), 2);
            assert!(matches!(text[0].glyph_type, GlyphType::Char { ch: '=' }));
            assert!(matches!(text[1].glyph_type, GlyphType::Char { ch: '>' }));
        })
        .expect("current row");
}

#[test]
fn synthetic_text_append_context_composes_with_current_row_tail() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("append-synthetic-combining", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 1, 0.0, 8.0);
    let table = neovm_core::face::FaceTable::new();
    let face_resolver =
        crate::neovm_bridge::FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base_face = face_resolver.default_face();
    let mut font_metrics = None;

    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    write_char_to_current_row_with_width(&mut builder, 'e', FaceId::new(7), 0, 8.0);
    let frame = test_append_frame(8.0, 8.0, DisplayTabPolicy::every(8));

    let append_context = SyntheticTextAppendContext::new(FaceId::new(7), base_face, frame);
    let progress = append_context
        .append_to_text_row_and_emit(
            &mut text_row_source_render_state(
                &mut builder,
                &mut output_emitter,
                &mut eval,
                &mut font_metrics,
                &face_resolver,
            ),
            DisplayRowPosition::new(8.0, 1),
            SyntheticTextSource::new(100, "\u{301}"),
        )
        .expect("combining fragment progress");
    let end = progress.end();

    assert_eq!(end, DisplayRowPosition::new(8.0, 1));
    assert_eq!(progress.metrics().width_px(), 0.0);
    assert_eq!(progress.metrics().width_cols(), 0);
    builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[1];
            assert_eq!(text.len(), 1);
            assert!(matches!(
                &text[0].glyph_type,
                GlyphType::Composite { text } if text.as_ref() == "e\u{301}"
            ));
        })
        .expect("current row");
}

#[test]
fn buffer_overlay_string_render_context_disabled_keeps_render_state() {
    let mut ctx = RowTransitionTestContext::new("overlay-disabled-render-state");
    let buf_id = ctx
        .eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let buffer = current_buffer_snapshot(&ctx.eval, buf_id);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let render_context = BufferOverlayStringTextRowRenderContext::new(
        false,
        1,
        &surface,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        0.0,
        0,
        4,
    );
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let mut font_metrics = None;
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let mut x = 24.0;
    let mut col = 3;
    let mut cursor_info = CursorCaptureState::new();
    let mut hit_row_range = HitRowRangeTracker::new(2);

    {
        let source_render = text_row_source_render_state(
            &mut ctx.builder,
            &mut ctx.output_emitter,
            &mut ctx.eval,
            &mut font_metrics,
            &face_resolver,
        );
        let mut state = OverlayStringRenderState::from_source_render(
            source_render,
            &mut x,
            &mut col,
            &mut ctx.geometry,
            &mut cursor_info,
            &mut ctx.hit_rows,
            &mut hit_row_range,
            &mut ctx.row_y_positions,
            &mut face_ids,
        );
        let strings = crate::neovm_bridge::RustTextPropAccess::new(&buffer).overlay_strings_at(5);
        render_context.render_produced_strings(
            &buffer,
            OverlayStringRenderPositions::from_layout_i64(5, 5),
            &strings,
            crate::display_item::DisplayStringBoxBoundaries::known(false, false),
            &mut state,
        );
    }

    assert_eq!(x, 24.0);
    assert_eq!(col, 3);
    assert_eq!(ctx.geometry.row(), 0);
    assert!(cursor_info.captured().is_none());
    assert!(ctx.hit_rows.is_empty());
    assert_eq!(hit_row_range.start(), 2);
}

#[test]
fn overlay_string_row_break_context_finishes_current_row() {
    let mut ctx = RowTransitionTestContext::new("overlay-row-break-context");
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let row_context = OverlayStringRenderRowContext::new(
        &surface,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        0.0,
        0,
        4,
    );
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let mut font_metrics = None;
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let mut x = 24.0;
    let mut col = 3;
    let mut cursor_info = CursorCaptureState::new();
    let mut hit_row_range = HitRowRangeTracker::new(2);

    {
        let source_render = text_row_source_render_state(
            &mut ctx.builder,
            &mut ctx.output_emitter,
            &mut ctx.eval,
            &mut font_metrics,
            &face_resolver,
        );
        let mut state = OverlayStringRenderState::from_source_render(
            source_render,
            &mut x,
            &mut col,
            &mut ctx.geometry,
            &mut cursor_info,
            &mut ctx.hit_rows,
            &mut hit_row_range,
            &mut ctx.row_y_positions,
            &mut face_ids,
        );

        assert_eq!(
            OverlayStringRowBreakRenderContext::new(5, row_context).finish_row(&mut state),
            DisplayRowTransitionContinuation::Continue
        );
    }

    assert_eq!(x, 0.0);
    assert_eq!(col, 0);
    assert_eq!(ctx.geometry.row(), 1);
    assert_eq!(ctx.hit_rows.len(), 1);
    assert_eq!(ctx.hit_rows[0].charpos_start, 2);
    assert_eq!(ctx.hit_rows[0].charpos_end, 5);
    assert_eq!(hit_row_range.start(), 5);
}

#[test]
fn render_natural_display_item_source_into_current_text_row_and_emit_uses_current_row_tail() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("render-current-row-fragment", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 1, 0.0, 8.0);
    let table = neovm_core::face::FaceTable::new();
    let face_resolver =
        crate::neovm_bridge::FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let mut base_face = face_resolver.default_face().clone();
    base_face.set_measured_char_width_px(8.0);
    base_face.font_ascent = 12.0;
    let mut source = crate::display_source::LispStringSourceCursor::new(
        101,
        Value::string("\u{301}"),
        RenderFaceRef::FaceId(FaceId::new(7)),
        crate::display_source::LispStringSourceOrigin::Normal,
    )
    .expect("lisp string source");
    let mut source_state = DisplayRowSourceState::frame_local();
    let mut font_metrics = None;
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(8);

    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    write_char_to_current_row_with_width(&mut builder, 'e', FaceId::new(7), 0, 8.0);

    let frame = test_append_frame(8.0, 8.0, DisplayTabPolicy::every(8));
    let position = DisplayRowPosition::new(8.0, 1);
    let request = frame.source_append_render_request(
        position,
        FaceId::new(7),
        &base_face,
        DisplayRowAppendKind::SourceText,
    );

    let mut source_render = text_row_source_render_state(
        &mut builder,
        &mut output_emitter,
        &mut eval,
        &mut font_metrics,
        &face_resolver,
    );
    let mut render_policy = NaturalDisplayRowAppendRenderPolicy;
    let outcome = source_render
        .render_display_item_source_into_current_text_row_and_emit(
            &mut face_ids,
            &mut source,
            &mut source_state,
            request,
            &mut render_policy,
        )
        .expect("current-row fragment outcome");

    assert_eq!(outcome.end_position(), DisplayRowPosition::new(8.0, 1));
    builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[1];
            assert_eq!(text.len(), 1);
            assert!(matches!(
                &text[0].glyph_type,
                GlyphType::Composite { text } if text.as_ref() == "e\u{301}"
            ));
        })
        .expect("current row");
}

#[test]
fn render_natural_display_item_source_into_current_text_row_stamps_slots_at_current_row_tail() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("render-current-row-slot-tail", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 1, 0.0, 8.0);
    let table = neovm_core::face::FaceTable::new();
    let face_resolver =
        crate::neovm_bridge::FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let mut base_face = face_resolver.default_face().clone();
    base_face.set_measured_char_width_px(8.0);
    base_face.font_ascent = 12.0;
    let mut source = crate::display_source::LispStringSourceCursor::new(
        101,
        Value::string("x"),
        RenderFaceRef::FaceId(FaceId::new(7)),
        crate::display_source::LispStringSourceOrigin::Normal,
    )
    .expect("lisp string source");
    let mut source_state = DisplayRowSourceState::frame_local();
    let mut font_metrics = None;
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(8);

    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    write_char_to_current_row_with_width(&mut builder, 'a', FaceId::new(7), 0, 8.0);
    write_char_to_current_row_with_width(&mut builder, 'b', FaceId::new(7), 1, 8.0);

    let frame = test_append_frame(8.0, 8.0, DisplayTabPolicy::every(8));
    let position = DisplayRowPosition::new(0.0, 0);
    let request = frame.source_append_render_request(
        position,
        FaceId::new(7),
        &base_face,
        DisplayRowAppendKind::SourceText,
    );

    let mut source_render = text_row_source_render_state(
        &mut builder,
        &mut output_emitter,
        &mut eval,
        &mut font_metrics,
        &face_resolver,
    );
    let mut render_policy = NaturalDisplayRowAppendRenderPolicy;
    let outcome = source_render
        .render_display_item_source_into_current_text_row_and_emit(
            &mut face_ids,
            &mut source,
            &mut source_state,
            request,
            &mut render_policy,
        )
        .expect("current-row fragment outcome");

    assert_eq!(
        outcome.source_slots(),
        &[DisplayRowGlyphSlot::new(
            DisplaySourcePosition::lisp_string(101, 0, 0),
            16.0,
            2,
            8.0,
            1
        )]
    );
    assert_eq!(outcome.end_position(), DisplayRowPosition::new(24.0, 3));
}

#[test]
fn render_face_ref_id_uses_fallback_for_inherit() {
    assert_eq!(
        render_face_ref_id(RenderFaceRef::FaceId(FaceId::new(12)), FaceId::new(7)),
        FaceId::new(12)
    );
    assert_eq!(
        render_face_ref_id(RenderFaceRef::Inherit, FaceId::new(7)),
        FaceId::new(7)
    );
}

#[test]
fn current_text_row_render_outcome_builds_append_progress() {
    let outcome = CurrentTextRowRenderOutcome::new(
        DisplayRowRenderStop::Clipped,
        vec![DisplayRowGlyphSlot::new(
            DisplaySourcePosition::synthetic(9, 0),
            8.0,
            1,
            16.0,
            2,
        )],
        DisplayRowPosition::new(24.0, 3),
        18.0,
        13.0,
    );
    let start = DisplayRowPosition::new(8.0, 1);

    let progress = outcome.into_append_progress(start);
    let end = progress.end();

    assert_eq!(end, DisplayRowPosition::new(24.0, 3));
    assert_eq!(progress.start(), start);
    assert_eq!(progress.end(), end);
    assert_eq!(progress.metrics().width_px(), 16.0);
    assert_eq!(progress.metrics().width_cols(), 2);
    assert_eq!(progress.status(), DisplayRowAppendStatus::Clipped);
    assert_eq!(progress.slots().len(), 1);
    assert_eq!(
        progress.slots()[0].source(),
        DisplaySourcePosition::synthetic(9, 0)
    );
}

#[test]
fn append_rendered_display_row_fragment_to_text_row_and_emit_appends_glyphs_and_slots() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("AB");
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("append-rendered-fragment", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let table = neovm_core::face::FaceTable::new();
    let face_resolver =
        crate::neovm_bridge::FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let mut base_face = face_resolver.default_face().clone();
    base_face.set_measured_char_width_px(8.0);
    base_face.font_ascent = 12.0;
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(8);
    let mut font_metrics = None;
    let rendered = {
        let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
        let mut source = crate::buffer_source::text_source::BufferTextSourceCursor::new(
            buf_id,
            buffer,
            CharPos0::new(0),
            CharPos0::new(2),
            RenderFaceRef::FaceId(FaceId::new(7)),
        );
        let mut renderer =
            DisplayRowRenderer::new(&mut font_metrics, DisplayRowMeasurementMode::LogicalCells);
        let mut source_state = DisplayRowSourceState::frame_local();
        DisplayRowSourceFragmentFrame::new(
            DisplayRowGeometry::new(0.0, 160.0, 16.0, 8.0, 12.0, DisplayTabPolicy::every(8)),
            GlyphRowRole::Text,
            FaceId::new(7),
            &base_face,
        )
        .render_request(DisplayRowRenderBounds::new(
            DisplayRowPosition::new(16.0, 2),
            DisplayRowMaxX::Bounded(160.0),
        ))
        .render_fragment_step_with_display_host(
            &mut renderer,
            &mut source,
            &mut source_state,
            &face_resolver,
            None,
            &mut face_ids,
        )
        .expect("rendered source")
    };
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 2, 0.0, 16.0);

    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    write_char_to_current_row_with_width(&mut builder, 'X', FaceId::new(7), 0, 8.0);
    write_char_to_current_row_with_width(&mut builder, 'Y', FaceId::new(7), 0, 8.0);

    let end = append_rendered_display_row_fragment_to_text_row_and_emit(
        &mut builder,
        &mut output_emitter,
        &mut eval,
        &rendered,
        crate::display_row::text_output::TextRowOutput::new(0, 0.0, 0.0, 16.0),
    );

    assert_eq!(end, DisplayRowPosition::new(32.0, 4));
    builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[1];
            assert_eq!(text.len(), 4);
            assert!(matches!(text[0].glyph_type, GlyphType::Char { ch: 'X' }));
            assert!(matches!(text[1].glyph_type, GlyphType::Char { ch: 'Y' }));
            assert!(matches!(text[2].glyph_type, GlyphType::Char { ch: 'A' }));
            assert!(matches!(text[3].glyph_type, GlyphType::Char { ch: 'B' }));
            assert_eq!(row.start_charpos, 0);
            assert_eq!(row.end_charpos, 2);
        })
        .expect("current row");

    let first = output_emitter
        .point_for_lisp_buffer_pos(LispCharPos1::new(1))
        .expect("first buffer display point");
    assert_eq!(first.x, 16);
    assert_eq!(first.col, 2);
    let second = output_emitter
        .point_for_lisp_buffer_pos(LispCharPos1::new(2))
        .expect("second buffer display point");
    assert_eq!(second.x, 24);
    assert_eq!(second.col, 3);
}

#[test]
fn display_row_append_surface_builds_positioned_source_requests() {
    let tab_policy = DisplayTabPolicy::from_tab_width_and_stops(8.0, 4, &[6, 10]);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(8.0, 120.0, 150.0, 10.0),
        tab_policy.clone(),
    );

    let geometry = DisplayRowGeometryState::new(3, 20.0, 0.0, 16.0, 11.0);
    let frame = surface.frame_from_geometry_state(
        &geometry,
        2.0,
        DisplayRowAppendMetrics::new(
            16.0,
            11.0,
            9.0,
            7.0,
            DisplayRowFallbackMetrics::from_default_face_extents(9.0, 14.0, 11.0),
        ),
    );
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base_face = resolver.default_face();
    let position = DisplayRowPosition::new(18.0, 2);
    let request = frame.source_append_render_request(
        position,
        FaceId::new(42),
        base_face,
        DisplayRowAppendKind::SourceText,
    );
    let output = request.output();
    let request = request.row_request();

    assert_eq!(request.render_bounds().start(), position);
    assert_eq!(
        request.render_bounds().max_x(),
        DisplayRowMaxX::Bounded(128.0)
    );
    assert_eq!(request.line_end_right_edge_x(), 148.0);
    assert_eq!(request.role(), GlyphRowRole::Text);
    assert_eq!(
        request.base_face_ref(),
        RenderFaceRef::FaceId(FaceId::new(42))
    );
    assert_eq!(
        *request.geometry(),
        DisplayRowGeometry::new(20.0, 120.0, 16.0, 9.0, 11.0, tab_policy)
    );
    assert_eq!(output.row(), 3);
    assert_eq!(output.row_y(), 20.0);
    assert_eq!(output.glyph_y(), 22.0);
    assert_eq!(output.height(), 16.0);
}

#[test]
fn display_row_append_frame_derives_layout_output_and_bounds() {
    let tab_policy = DisplayTabPolicy::from_tab_width_and_stops(8.0, 4, &[6, 10]);
    let frame = test_append_frame_at(
        3,
        20.0,
        22.0,
        DisplayRowAppendArea::new(8.0, 120.0, 150.0, 10.0),
        DisplayRowAppendMetrics::new(
            16.0,
            11.0,
            9.0,
            7.0,
            DisplayRowFallbackMetrics::from_default_face_extents(9.0, 14.0, 11.0),
        ),
        tab_policy,
    );
    let position = DisplayRowPosition::new(8.0, 0);
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base_face = resolver.default_face();

    let ordinary = frame.source_append_render_request(
        position,
        FaceId::new(42),
        base_face,
        DisplayRowAppendKind::SourceText,
    );
    let ordinary_output = ordinary.output();
    let ordinary = ordinary.row_request();
    assert_eq!(
        ordinary.render_bounds().start(),
        DisplayRowPosition::new(8.0, 0)
    );
    assert_eq!(
        ordinary.render_bounds().max_x(),
        DisplayRowMaxX::Bounded(128.0)
    );
    assert_eq!(ordinary.geometry().char_width(), 9.0);
    assert_eq!(ordinary_output.row(), 3);
    assert_eq!(ordinary_output.row_y(), 20.0);
    assert_eq!(ordinary_output.glyph_y(), 22.0);
    assert_eq!(ordinary_output.height(), 16.0);

    let tab = frame.source_append_render_request(
        position,
        FaceId::new(42),
        base_face,
        DisplayRowAppendKind::Tab,
    );
    let tab_output = tab.output();
    let tab = tab.row_request();
    assert_eq!(tab.render_bounds().max_x(), DisplayRowMaxX::Unbounded);
    assert_eq!(tab.geometry().char_width(), 7.0);
    assert_eq!(tab_output.height(), 14.0);

    let control = frame.source_append_render_request(
        position,
        FaceId::new(42),
        base_face,
        DisplayRowAppendKind::ControlChar,
    );
    let control_output = control.output();
    let control = control.row_request();
    assert_eq!(
        control.render_bounds().max_x(),
        DisplayRowMaxX::Bounded(148.0)
    );
    assert_eq!(control.geometry().char_width(), 9.0);
    assert_eq!(control_output.height(), 14.0);

    let mapped = frame.source_append_render_request(
        position,
        FaceId::new(42),
        base_face,
        DisplayRowAppendKind::SourceMappedText,
    );
    let mapped_output = mapped.output();
    let mapped = mapped.row_request();
    assert_eq!(
        mapped.render_bounds().max_x(),
        DisplayRowMaxX::Bounded(128.0)
    );
    assert_eq!(mapped_output.height(), 14.0);

    let glyphless = frame.source_append_render_request(
        position,
        FaceId::new(42),
        base_face,
        DisplayRowAppendKind::Glyphless,
    );
    let glyphless_output = glyphless.output();
    let glyphless = glyphless.row_request();
    assert_eq!(
        glyphless.render_bounds().max_x(),
        DisplayRowMaxX::Bounded(128.0)
    );
    assert_eq!(glyphless_output.height(), 16.0);

    let replacement = frame.source_append_render_request(
        position,
        FaceId::new(42),
        base_face,
        DisplayRowAppendKind::DisplayReplacement,
    );
    let replacement_output = replacement.output();
    let replacement = replacement.row_request();
    assert_eq!(
        replacement.render_bounds().max_x(),
        DisplayRowMaxX::Bounded(128.0)
    );
    assert_eq!(replacement.geometry().char_width(), 9.0);
    assert_eq!(replacement_output.height(), 16.0);

    let replacement_string = frame.source_append_render_request(
        position,
        FaceId::new(42),
        base_face,
        DisplayRowAppendKind::DisplayReplacementString,
    );
    let replacement_string_output = replacement_string.output();
    let replacement_string = replacement_string.row_request();
    assert_eq!(
        replacement_string.render_bounds().max_x(),
        DisplayRowMaxX::Bounded(128.0)
    );
    assert_eq!(replacement_string.geometry().char_width(), 7.0);
    assert_eq!(replacement_string_output.height(), 16.0);
}

#[test]
fn display_row_append_kind_names_width_clip_and_output_policy() {
    let frame = test_append_frame_at(
        3,
        20.0,
        22.0,
        DisplayRowAppendArea::new(8.0, 120.0, 150.0, 10.0),
        DisplayRowAppendMetrics::new(
            16.0,
            11.0,
            9.0,
            7.0,
            DisplayRowFallbackMetrics::from_default_face_extents(9.0, 14.0, 11.0),
        ),
        DisplayTabPolicy::every(4),
    );

    assert_eq!(DisplayRowAppendKind::SourceText.char_width(&frame), 9.0);
    assert_eq!(DisplayRowAppendKind::Tab.char_width(&frame), 7.0);
    assert_eq!(
        DisplayRowAppendKind::DisplayReplacementString.char_width(&frame),
        7.0
    );
    assert_eq!(
        DisplayRowAppendKind::Tab.max_x(&frame),
        DisplayRowMaxX::Unbounded
    );
    assert_eq!(
        DisplayRowAppendKind::ControlChar.max_x(&frame),
        DisplayRowMaxX::Bounded(148.0)
    );
    assert_eq!(
        DisplayRowAppendKind::Glyphless.max_x(&frame),
        DisplayRowMaxX::Bounded(128.0)
    );
    assert_eq!(
        DisplayRowAppendKind::DisplayReplacement.output_height(&frame),
        16.0
    );
    assert_eq!(
        DisplayRowAppendKind::ControlChar.output_height(&frame),
        14.0
    );
}

#[test]
fn display_row_append_frame_builds_positioned_source_append_render_request() {
    let tab_policy = DisplayTabPolicy::every(4);
    let frame = test_append_frame_at(
        3,
        20.0,
        22.0,
        DisplayRowAppendArea::new(8.0, 120.0, 150.0, 10.0),
        DisplayRowAppendMetrics::new(
            16.0,
            11.0,
            9.0,
            7.0,
            DisplayRowFallbackMetrics::from_default_face_extents(9.0, 14.0, 11.0),
        ),
        tab_policy,
    );
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base_face = resolver.default_face();

    let position = DisplayRowPosition::new(18.0, 2);
    let request = frame.source_append_render_request(
        position,
        FaceId::new(42),
        base_face,
        DisplayRowAppendKind::SourceText,
    );
    let output = request.output();
    let request = request.row_request();

    assert_eq!(request.render_bounds().start(), position);
    assert_eq!(
        request.render_bounds().max_x(),
        DisplayRowMaxX::Bounded(128.0)
    );
    assert_eq!(
        request.base_face_ref(),
        RenderFaceRef::FaceId(FaceId::new(42))
    );
    assert_eq!(request.role(), GlyphRowRole::Text);
    assert_eq!(request.geometry().y(), 20.0);
    assert_eq!(request.geometry().char_width(), 9.0);
    assert_eq!(output.row(), 3);
    assert_eq!(output.height(), 16.0);
}

#[test]
fn display_row_append_frame_exposes_source_row_request_through_append_request() {
    let tab_policy = DisplayTabPolicy::every(4);
    let frame = test_append_frame_at(
        3,
        20.0,
        22.0,
        DisplayRowAppendArea::new(8.0, 120.0, 150.0, 10.0),
        DisplayRowAppendMetrics::new(
            16.0,
            11.0,
            9.0,
            7.0,
            DisplayRowFallbackMetrics::from_default_face_extents(9.0, 14.0, 11.0),
        ),
        tab_policy,
    );
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base_face = resolver.default_face();

    let request = frame
        .source_append_render_request(
            DisplayRowPosition::new(18.0, 2),
            FaceId::new(42),
            base_face,
            DisplayRowAppendKind::SourceText,
        )
        .row_request();

    assert_eq!(
        request.render_bounds().max_x(),
        DisplayRowMaxX::Bounded(128.0)
    );
    assert_eq!(
        request.base_face_ref(),
        RenderFaceRef::FaceId(FaceId::new(42))
    );
    assert_eq!(request.role(), GlyphRowRole::Text);
    assert_eq!(request.geometry().y(), 20.0);
    assert_eq!(request.geometry().char_width(), 9.0);
}

#[test]
fn display_row_append_frame_builds_source_measure_request() {
    let frame = test_append_frame_at(
        3,
        20.0,
        22.0,
        DisplayRowAppendArea::new(8.0, 120.0, 150.0, 10.0),
        DisplayRowAppendMetrics::new(
            16.0,
            11.0,
            9.0,
            7.0,
            DisplayRowFallbackMetrics::from_default_face_extents(9.0, 14.0, 11.0),
        ),
        DisplayTabPolicy::every(4),
    );
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base_face = resolver.default_face();
    let position = DisplayRowPosition::new(18.0, 2);

    let request = frame.source_append_measure_request(
        position,
        FaceId::new(42),
        base_face,
        DisplayRowAppendKind::SourceText,
    );

    assert_eq!(request.render_bounds().start(), position);
    assert_eq!(request.render_bounds().max_x(), DisplayRowMaxX::Unbounded);
    assert_eq!(
        request.base_face_ref(),
        RenderFaceRef::FaceId(FaceId::new(42))
    );
    assert_eq!(request.role(), GlyphRowRole::Text);
    assert_eq!(request.geometry().char_width(), 9.0);
}

#[test]
fn display_row_source_append_render_request_uses_frame_policy() {
    let tab_policy = DisplayTabPolicy::every(4);
    let frame = test_append_frame_at(
        3,
        20.0,
        22.0,
        DisplayRowAppendArea::new(8.0, 120.0, 150.0, 10.0),
        DisplayRowAppendMetrics::new(
            16.0,
            11.0,
            9.0,
            7.0,
            DisplayRowFallbackMetrics::from_default_face_extents(9.0, 14.0, 11.0),
        ),
        tab_policy,
    );
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base_face = resolver.default_face();

    let position = DisplayRowPosition::new(18.0, 2);
    let request = frame.source_append_render_request(
        position,
        FaceId::new(42),
        base_face,
        DisplayRowAppendKind::ControlChar,
    );
    let output = request.output();
    let request = request.row_request();

    assert_eq!(request.base_face_id(), FaceId::new(42));
    assert_eq!(request.render_bounds().start(), position);
    assert_eq!(
        request.render_bounds().max_x(),
        DisplayRowMaxX::Bounded(148.0)
    );
    assert_eq!(request.geometry().height(), 16.0);
    assert_eq!(output.height(), 14.0);
}

#[test]
fn display_row_append_frame_builds_control_char_source_append_render_request() {
    let frame = test_append_frame_at(
        3,
        20.0,
        22.0,
        DisplayRowAppendArea::new(8.0, 120.0, 150.0, 10.0),
        DisplayRowAppendMetrics::new(
            16.0,
            11.0,
            9.0,
            7.0,
            DisplayRowFallbackMetrics::from_default_face_extents(9.0, 14.0, 11.0),
        ),
        DisplayTabPolicy::every(4),
    );
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base_face = resolver.default_face();

    let position = DisplayRowPosition::new(18.0, 2);
    let request = frame.source_append_render_request(
        position,
        FaceId::new(42),
        base_face,
        DisplayRowAppendKind::ControlChar,
    );
    let output = request.output();
    let request = request.row_request();

    assert_eq!(request.base_face_id(), FaceId::new(42));
    assert_eq!(request.render_bounds().start(), position);
    assert_eq!(
        request.render_bounds().max_x(),
        DisplayRowMaxX::Bounded(148.0)
    );
    assert_eq!(request.base_face_id(), FaceId::new(42));
    assert_eq!(output.row(), 3);
    assert_eq!(output.height(), 14.0);
}

#[test]
fn display_row_append_surface_builds_frames_with_shared_area() {
    let tab_policy = DisplayTabPolicy::every(4);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(8.0, 120.0, 150.0, 10.0),
        tab_policy.clone(),
    );

    assert_eq!(surface.content_x(), 8.0);
    assert_eq!(surface.right_edge(), 128.0);

    let full_text_surface = surface.full_text_width_surface();
    assert_eq!(full_text_surface.content_x(), 8.0);
    assert_eq!(full_text_surface.right_edge(), 148.0);
    assert_eq!(surface.full_text_right_edge(), 148.0);

    let geometry = DisplayRowGeometryState::new(3, 20.0, 0.0, 16.0, 11.0);
    let frame = surface.frame_from_geometry_state(
        &geometry,
        2.0,
        DisplayRowAppendMetrics::new(
            16.0,
            11.0,
            9.0,
            7.0,
            DisplayRowFallbackMetrics::from_default_face_extents(9.0, 14.0, 11.0),
        ),
    );

    assert_eq!(frame.row(), 3);
    assert_eq!(frame.glyph_y(), 22.0);
    assert_eq!(
        *frame.geometry(),
        DisplayRowGeometry::new(20.0, 120.0, 16.0, 9.0, 11.0, tab_policy)
    );
    assert_eq!(frame.content_x(), 8.0);
    assert_eq!(frame.text_width(), 150.0);
    assert_eq!(frame.line_number_width(), 10.0);
}

#[test]
fn display_row_text_append_context_builds_text_frame_from_shared_surface() {
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(8.0, 120.0, 150.0, 10.0),
        DisplayTabPolicy::every(4),
    );
    let geometry = DisplayRowGeometryState::new(3, 20.0, 0.0, 16.0, 11.0);

    let frame = DisplayRowAppendMetrics::text_row(
        16.0,
        11.0,
        9.0,
        DisplayRowFallbackMetrics::from_default_face_extents(9.0, 14.0, 11.0),
    )
    .text_row_frame(&surface, &geometry, 2.0);

    assert_eq!(frame.row(), 3);
    assert_eq!(frame.glyph_y(), 22.0);
    assert_eq!(frame.geometry().height(), 16.0);
    assert_eq!(frame.geometry().ascent(), 11.0);
    assert_eq!(frame.geometry().char_width(), 9.0);
    assert_eq!(frame.face_space_width(), 9.0);
    assert_eq!(frame.default_row_height(), 14.0);
    assert_eq!(frame.content_x(), 8.0);
    assert_eq!(frame.text_width(), 150.0);
    assert_eq!(frame.line_number_width(), 10.0);
}

#[test]
fn display_row_append_surface_builds_frame_from_active_face_state() {
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base = resolver.default_face().clone();
    let mut font_metrics = None;
    let measured = DisplayRowMeasurementPolicy::for_mode(DisplayRowMeasurementMode::LogicalCells)
        .measured_face(
            FaceId::new(7),
            &base,
            None,
            7.5,
            DisplayRowFallbackMetrics {
                char_width: 7.5,
                row_height: 18.0,
                ascent: 13.0,
            },
            &mut font_metrics,
        );
    let active_face = DisplayRowActiveFaceState::new(base, measured);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(8.0, 120.0, 150.0, 10.0),
        DisplayTabPolicy::every(4),
    );

    let geometry = DisplayRowGeometryState::new(3, 20.0, 0.0, 16.0, 12.0);
    let frame = DisplayRowActiveFaceAppendContext::new(
        &surface,
        &geometry,
        &active_face,
        2.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
    )
    .active_face_frame();

    assert_eq!(frame.row(), 3);
    assert_eq!(frame.glyph_y(), 22.0);
    assert_eq!(frame.geometry().height(), 18.0);
    assert_eq!(frame.geometry().ascent(), 13.0);
    assert_eq!(frame.geometry().char_width(), 7.5);
    assert_eq!(frame.face_space_width(), 8.0);
    assert_eq!(frame.default_row_height(), 16.0);

    let full_text_frame = DisplayRowActiveFaceAppendContext::new(
        &surface,
        &geometry,
        &active_face,
        2.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
    )
    .full_text_width_active_face_frame();
    assert_eq!(full_text_frame.geometry().width(), 140.0);
}

#[test]
fn display_row_append_frame_preserves_geometry_and_area() {
    let tab_policy = DisplayTabPolicy::every(4);
    let frame = test_append_frame_at(
        3,
        20.0,
        22.0,
        DisplayRowAppendArea::new(8.0, 120.0, 150.0, 10.0),
        DisplayRowAppendMetrics::new(
            16.0,
            11.0,
            9.0,
            7.0,
            DisplayRowFallbackMetrics::from_default_face_extents(9.0, 14.0, 11.0),
        ),
        tab_policy.clone(),
    );

    assert_eq!(frame.row(), 3);
    assert_eq!(frame.glyph_y(), 22.0);
    assert_eq!(
        *frame.geometry(),
        DisplayRowGeometry::new(20.0, 120.0, 16.0, 9.0, 11.0, tab_policy)
    );
    assert_eq!(frame.default_row_height(), 14.0);
    assert_eq!(frame.content_x(), 8.0);
    assert_eq!(frame.text_width(), 150.0);
    assert_eq!(frame.line_number_width(), 10.0);
    assert_eq!(frame.face_space_width(), 7.0);
}

#[test]
fn layout_display_source_face_resolver_records_pending_faces_without_builder() {
    let _eval = Context::new();
    let table = neovm_core::face::FaceTable::new();
    let face_resolver =
        crate::neovm_bridge::FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base_face = face_resolver.default_face();
    let mut resolve_state = crate::display_source_resolver::DisplaySourceResolveState::default();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let mut pending_faces = Vec::new();
    let params = crate::display_source_resolver::DisplaySourceResolveParams::new(
        crate::display_source_resolver::DisplaySourceFaceBasis::new(
            &face_resolver,
            FaceId::new(0),
            base_face,
            crate::display_row::metrics::DisplayRowFallbackMetrics::from_default_face_extents(
                8.0, 16.0, 12.0,
            ),
        ),
        None,
        neovm_core::emacs_core::image_catalog::ImageScaleEnvironment::default(),
    );
    let mut resolver = crate::display_source_resolver::DisplaySourcePropertyResolver::frame_local(
        params,
        &mut resolve_state,
        &mut face_ids,
        &mut pending_faces,
    );
    let face_value = Value::list(vec![Value::keyword("foreground"), Value::string("#ff0000")]);

    let face = crate::display_source::DisplayItemFaceResolver::resolve_face_ref(
        &mut resolver,
        RenderFaceRef::FaceId(FaceId::new(0)),
        face_value,
    );

    assert_eq!(face, RenderFaceRef::FaceId(FaceId::new(20)));
    assert_eq!(face_ids.next_face_id_for_test(), 21);
    assert_eq!(pending_faces.len(), 1);
    assert_eq!(pending_faces[0].face_id(), FaceId::new(20));
    assert_eq!(pending_faces[0].resolved().fg, 0x00ff0000);
}

#[test]
fn display_source_resolve_params_are_built_from_typed_face_basis() {
    let table = neovm_core::face::FaceTable::new();
    let face_resolver =
        crate::neovm_bridge::FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base_face = face_resolver.default_face();
    let fallback =
        crate::display_row::metrics::DisplayRowFallbackMetrics::from_default_face_extents(
            8.0, 16.0, 12.0,
        );
    let basis = crate::display_source_resolver::DisplaySourceFaceBasis::new(
        &face_resolver,
        FaceId::new(7),
        base_face,
        fallback,
    );

    let params = crate::display_source_resolver::DisplaySourceResolveParams::new(
        basis,
        None,
        neovm_core::emacs_core::image_catalog::ImageScaleEnvironment::default(),
    );

    assert_eq!(params.face_basis().base_face_id(), FaceId::new(7));
    assert_eq!(params.face_basis().fallback_metrics(), fallback);
    assert!(std::ptr::eq(params.face_basis().base_face(), base_face));
    assert!(std::ptr::eq(
        params.face_basis().canonical_face(),
        face_resolver.default_face()
    ));
}

#[test]
fn resolve_next_display_source_item_returns_item_and_pending_faces() {
    let _eval = Context::new();
    let table = neovm_core::face::FaceTable::new();
    let face_resolver =
        crate::neovm_bridge::FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base_face = face_resolver.default_face();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let mut resolve_state = crate::display_source_resolver::DisplaySourceResolveState::default();
    let value = Value::string_with_text_properties(
        "a",
        vec![StringTextPropertyRun {
            start: 0,
            end: 1,
            plist: Value::list(vec![
                Value::symbol("face"),
                Value::list(vec![Value::keyword("foreground"), Value::string("#ff0000")]),
            ]),
        }],
    );
    let mut source = crate::display_source::LispStringSourceCursor::new(
        1,
        value,
        RenderFaceRef::FaceId(FaceId::new(0)),
        crate::display_source::LispStringSourceOrigin::Normal,
    )
    .expect("string source");

    let resolved = crate::display_source_resolver::resolve_next_display_source_item(
        &mut source,
        crate::display_source_resolver::DisplaySourceFaceScope::FrameLocal,
        crate::display_source_resolver::DisplaySourceResolveParams::new(
            crate::display_source_resolver::DisplaySourceFaceBasis::new(
                &face_resolver,
                FaceId::new(0),
                base_face,
                crate::display_row::metrics::DisplayRowFallbackMetrics::from_default_face_extents(
                    8.0, 16.0, 12.0,
                ),
            ),
            None,
            neovm_core::emacs_core::image_catalog::ImageScaleEnvironment::default(),
        ),
        &mut resolve_state,
        &mut face_ids,
    );

    let (item, pending_faces) = resolved.into_parts();
    let item = item.expect("source item");
    assert_eq!(item.face, RenderFaceRef::FaceId(FaceId::new(20)));
    assert_eq!(pending_faces.len(), 1);
    assert_eq!(pending_faces[0].face_id(), FaceId::new(20));
    assert_eq!(pending_faces[0].resolved().fg, 0x00ff0000);
}

#[test]
fn resolve_next_display_source_item_merges_glyphless_char_face() {
    let _eval = Context::new();
    let mut table = neovm_core::face::FaceTable::new();
    let mut glyphless = neovm_core::face::Face::new("glyphless-char");
    glyphless.foreground = Some(neovm_core::face::Color::rgb(0x12, 0x34, 0x56));
    table.define("glyphless-char", glyphless);
    let face_resolver =
        crate::neovm_bridge::FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base_face = face_resolver.default_face();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let mut resolve_state = crate::display_source_resolver::DisplaySourceResolveState::default();
    let acronym =
        crate::display_item::GlyphlessAcronym::from_ascii("v").expect("one-byte ASCII acronym");
    let mut source = crate::display_source::DisplayItemSegmentSource::new(DisplayItem::new(
        crate::display_item::SourceSpan::synthetic(1, 0, 1),
        RenderFaceRef::FaceId(FaceId::new(0)),
        DisplayItemKind::Glyphless(crate::display_item::DisplayGlyphless {
            ch: '▼',
            method: GlyphlessMethod::Acronym(acronym),
        }),
    ));

    let resolved = crate::display_source_resolver::resolve_next_display_source_item(
        &mut source,
        crate::display_source_resolver::DisplaySourceFaceScope::FrameLocal,
        crate::display_source_resolver::DisplaySourceResolveParams::new(
            crate::display_source_resolver::DisplaySourceFaceBasis::new(
                &face_resolver,
                FaceId::new(0),
                base_face,
                crate::display_row::metrics::DisplayRowFallbackMetrics::from_default_face_extents(
                    8.0, 16.0, 12.0,
                ),
            ),
            None,
            neovm_core::emacs_core::image_catalog::ImageScaleEnvironment::default(),
        ),
        &mut resolve_state,
        &mut face_ids,
    );

    let (item, pending_faces) = resolved.into_parts();
    let item = item.expect("glyphless source item");
    assert!(matches!(item.kind, DisplayItemKind::Glyphless(_)));
    assert_eq!(item.face, RenderFaceRef::FaceId(FaceId::new(20)));
    assert_eq!(pending_faces.len(), 1);
    assert_eq!(pending_faces[0].face_id(), FaceId::new(20));
    assert_eq!(pending_faces[0].resolved().fg, 0x00123456);
}

#[test]
fn resolve_next_display_source_item_resolves_height_modifier_to_pending_face() {
    let _eval = Context::new();
    let table = neovm_core::face::FaceTable::new();
    let face_resolver =
        crate::neovm_bridge::FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base_face = face_resolver.default_face();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let mut resolve_state = crate::display_source_resolver::DisplaySourceResolveState::default();
    let value = Value::string_with_text_properties(
        "a",
        vec![StringTextPropertyRun {
            start: 0,
            end: 1,
            plist: Value::list(vec![
                Value::symbol("display"),
                Value::list(vec![Value::symbol("height"), Value::make_float(2.0)]),
            ]),
        }],
    );
    let mut source = crate::display_source::LispStringSourceCursor::new(
        1,
        value,
        RenderFaceRef::FaceId(FaceId::new(0)),
        crate::display_source::LispStringSourceOrigin::Normal,
    )
    .expect("string source");

    let resolved = crate::display_source_resolver::resolve_next_display_source_item(
        &mut source,
        crate::display_source_resolver::DisplaySourceFaceScope::FrameLocal,
        crate::display_source_resolver::DisplaySourceResolveParams::new(
            crate::display_source_resolver::DisplaySourceFaceBasis::new(
                &face_resolver,
                FaceId::new(0),
                base_face,
                crate::display_row::metrics::DisplayRowFallbackMetrics::from_default_face_extents(
                    8.0, 16.0, 12.0,
                ),
            ),
            None,
            neovm_core::emacs_core::image_catalog::ImageScaleEnvironment::default(),
        ),
        &mut resolve_state,
        &mut face_ids,
    );

    let (item, pending_faces) = resolved.into_parts();
    let item = item.expect("source item");
    assert_eq!(item.face, RenderFaceRef::FaceId(FaceId::new(20)));
    assert_eq!(pending_faces.len(), 1);
    assert_eq!(pending_faces[0].face_id(), FaceId::new(20));
    assert_eq!(pending_faces[0].resolved().font_size, 28.0);
    assert_eq!(pending_faces[0].resolved().font_line_height, 32.0);
    assert_eq!(pending_faces[0].resolved().font_ascent, 24.0);
    assert_eq!(pending_faces[0].resolved().measured_char_width_px(), 16.0);
}

#[test]
fn display_row_source_walker_reuses_face_cache_across_items() {
    let _eval = Context::new();
    let table = neovm_core::face::FaceTable::new();
    let face_resolver =
        crate::neovm_bridge::FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base_face = face_resolver.default_face();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    let face_value = Value::list(vec![Value::keyword("foreground"), Value::string("#ff0000")]);
    let value = Value::string_with_text_properties(
        "aba",
        vec![
            StringTextPropertyRun {
                start: 0,
                end: 1,
                plist: Value::list(vec![Value::symbol("face"), face_value.clone()]),
            },
            StringTextPropertyRun {
                start: 2,
                end: 3,
                plist: Value::list(vec![Value::symbol("face"), face_value]),
            },
        ],
    );
    let source = crate::display_source::LispStringSourceCursor::new(
        1,
        value,
        RenderFaceRef::FaceId(FaceId::new(0)),
        crate::display_source::LispStringSourceOrigin::Normal,
    )
    .expect("string source");
    let mut source = DisplayRowSourceWalker::new(source);
    let (first, second, third) = {
        let mut next_item = |label: &str| {
            let step = source
                .next_step(
                    &face_resolver,
                    base_face,
                    FaceId::new(0),
                    &mut face_ids,
                    None,
                    8.0,
                    12.0,
                    16.0,
                )
                .unwrap_or_else(|| panic!("{label} source item"));
            let (item, mut pending_faces) = step.into_parts();
            apply_pending_display_source_faces(&mut builder, &mut pending_faces);
            item
        };
        (next_item("first"), next_item("second"), next_item("third"))
    };

    assert_eq!(first.face, RenderFaceRef::FaceId(FaceId::new(20)));
    assert_eq!(second.face, RenderFaceRef::FaceId(FaceId::new(0)));
    assert_eq!(third.face, RenderFaceRef::FaceId(FaceId::new(20)));
    assert_eq!(face_ids.next_face_id_for_test(), 21);
    assert_eq!(
        builder
            .output_face(FaceId::new(20))
            .map(|face| face.foreground),
        Some(Color::from_pixel(0x00ff0000))
    );
}

#[test]
fn append_lisp_string_to_text_row_appends_propertized_string_items() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("append-lisp-string", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);

    let table = neovm_core::face::FaceTable::new();
    let face_resolver =
        crate::neovm_bridge::FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base_face = face_resolver.default_face();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    let value = Value::string_with_text_properties(
        "ab",
        vec![StringTextPropertyRun {
            start: 1,
            end: 2,
            plist: Value::list(vec![
                Value::symbol("face"),
                Value::list(vec![Value::keyword("foreground"), Value::string("#ff0000")]),
            ]),
        }],
    );
    let frame = test_append_frame(8.0, 8.0, DisplayTabPolicy::every(8));
    let end = {
        let mut font_metrics = None;
        let mut source_render = text_row_source_render_state(
            &mut builder,
            &mut output_emitter,
            &mut eval,
            &mut font_metrics,
            &face_resolver,
        );
        append_lisp_string_to_text_row(
            &mut source_render,
            value,
            1,
            base_face,
            FaceId::new(0),
            &mut face_ids,
            frame,
            DisplayRowPosition::new(0.0, 0),
        )
    };

    assert_eq!(end, DisplayRowPosition::new(16.0, 2));
    assert_eq!(face_ids.next_face_id_for_test(), 21);
    assert_eq!(
        builder
            .output_face(FaceId::new(20))
            .map(|face| face.foreground),
        Some(Color::from_pixel(0x00ff0000))
    );
    builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[1];
            assert_eq!(text[0].face_id, FaceId::new(0));
            assert_eq!(text[1].face_id, FaceId::new(20));
        })
        .expect("current row");
}

#[test]
fn lisp_string_append_context_appends_fragment_items() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("append-lisp-fragment-context", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);

    let table = neovm_core::face::FaceTable::new();
    let face_resolver =
        crate::neovm_bridge::FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base_face = face_resolver.default_face();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let mut font_metrics = None;
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    let active_face = test_active_face_state(FaceId::new(0), 8.0);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let append_context = LispStringRowAppendContext::new(
        &surface,
        &geometry,
        &active_face,
        0.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
    );

    let request = LispStringSourceAppendRequest::new(
        DisplayRowPosition::new(0.0, 0),
        LispStringSourceId::PREFIX,
        Value::string("=>"),
    );
    let session_request =
        LispStringSourceAppendSessionRequest::frame_local(request, FaceId::new(0), base_face);
    let end = append_context.render_active_face_source_request_to_text_row_and_emit(
        &mut text_row_source_render_state(
            &mut builder,
            &mut output_emitter,
            &mut eval,
            &mut font_metrics,
            &face_resolver,
        ),
        &mut face_ids,
        session_request,
    );

    assert_eq!(end, DisplayRowPosition::new(16.0, 2));
    builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[1];
            assert_eq!(text.len(), 2);
            assert!(matches!(text[0].glyph_type, GlyphType::Char { ch: '=' }));
            assert!(matches!(text[1].glyph_type, GlyphType::Char { ch: '>' }));
        })
        .expect("current row");
}

#[test]
fn buffer_text_source_append_context_appends_source_char() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.insert("ab");
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("append-buffer-fragment", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);

    let snapshot = {
        let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
        LayoutBufferSnapshot::from_buffer(buffer)
    };
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let mut font_metrics = None;
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);

    let append_context = BufferSourceRowAppendContext::new(
        &snapshot,
        buf_id,
        &surface,
        &active_face,
        0.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
    );
    let source_char = DisplaySourceTextChar::new(
        'a',
        CharPos0::new(0),
        crate::types::NobreakDisplayMode::Escape,
    );
    let source_item =
        buffer_source_mapped_display_item(buf_id, 0, 1, "a", RenderFaceRef::FaceId(FaceId::new(7)));
    let mut append_state = DisplaySourceRowAppendState::default();
    let prepared_append = append_context
        .prepare_source_item_char_at(
            &geometry,
            &mut append_state,
            &mut text_row_source_measure_state(
                &mut builder,
                &mut eval,
                &mut font_metrics,
                &face_resolver,
            ),
            &source_char,
            b"a",
            0,
            DisplayRowPosition::new(0.0, 0),
            &source_item,
            None,
        )
        .into_text()
        .expect("ordinary buffer char should prepare text append");
    let cursor_info = prepared_append.cursor_info_for_main_char(
        &active_face,
        geometry.text_position(2.0, 0, 3),
        false,
    );
    assert_eq!(cursor_info.x, 2.0);
    assert_eq!(cursor_info.col, 3);
    assert_eq!(cursor_info.slot_width, Some(8.0));
    assert!(!cursor_info.stretch_like);
    let mut cursor_capture = CursorCaptureState::new();
    prepared_append.capture_cursor_info_for_main_char_if_point(
        &mut cursor_capture,
        &active_face,
        &geometry,
        2.0,
        0,
        3,
        false,
        4,
        5,
    );
    assert!(cursor_capture.is_missing());
    prepared_append.capture_cursor_info_for_main_char_if_point(
        &mut cursor_capture,
        &active_face,
        &geometry,
        2.0,
        0,
        3,
        false,
        5,
        5,
    );
    let captured_cursor = cursor_capture.as_ref().expect("captured cursor");
    assert_eq!(captured_cursor.x, 2.0);
    assert_eq!(captured_cursor.col, 3);
    assert_eq!(captured_cursor.slot_width, Some(8.0));
    assert!(!captured_cursor.stretch_like);
    assert_eq!(
        prepared_append.overflow_decision(
            'a',
            80.0,
            LineWrapMode::Wrap,
            WordWrapRenderState::new(false)
        ),
        DisplayRowTextOverflowDecision::Fits
    );
    assert!(matches!(
        prepared_append.overflow_action(
            'a',
            80.0,
            LineWrapMode::Wrap,
            WordWrapRenderState::new(false)
        ),
        DisplaySourceTextCharOverflowAction::Fits
    ));
    assert!(matches!(
        prepared_append.overflow_action(
            'a',
            4.0,
            LineWrapMode::Truncate,
            WordWrapRenderState::new(false)
        ),
        DisplaySourceTextCharOverflowAction::Truncate { .. }
    ));
    assert!(matches!(
        prepared_append.overflow_action(
            'a',
            4.0,
            LineWrapMode::Wrap,
            WordWrapRenderState::new(false)
        ),
        DisplaySourceTextCharOverflowAction::CharacterWrap { .. }
    ));
    let mut word_wrap = WordWrapRenderState::new(true);
    word_wrap.allow_after_current_char(' ');
    word_wrap.record_candidate(
        'a',
        0,
        0,
        2,
        (Some(LispCharPos1::new(1)), Some(LispCharPos1::new(1))),
        DisplayRowGlyphCheckpoint::default(),
    );
    assert!(matches!(
        prepared_append.overflow_action('a', 4.0, LineWrapMode::Wrap, word_wrap),
        DisplaySourceTextCharOverflowAction::WordWrap { break_candidate, .. }
            if break_candidate.byte_idx() == 0
                && break_candidate.charpos() == 0
                && break_candidate.display_point_count() == 2
    ));
    let mut trailing_whitespace = TrailingWhitespaceRenderState::new(true, 0x00ff00);
    let mut word_wrap = WordWrapRenderState::new(true);
    let mut charpos = 4;
    let mut byte_idx = 0;
    let mut end_x = 0.0;
    let mut end_col = 0;
    let mut source_render = text_row_source_render_state(
        &mut builder,
        &mut output_emitter,
        &mut eval,
        &mut font_metrics,
        &face_resolver,
    );
    let mut progress =
        DisplaySourceProgressState::new(&mut byte_idx, &mut charpos, &mut end_x, &mut end_col);
    let continuation = prepared_append.append_to_text_row_and_apply(
        &append_context,
        &geometry,
        ' ',
        &mut source_render,
        &mut trailing_whitespace,
        &mut word_wrap,
        &mut progress,
    );
    assert_eq!(continuation, DisplaySourceAppendContinuation::Rendered);
    assert_eq!(
        trailing_whitespace
            .highlight_start_x(&geometry)
            .map(|(_color, x)| x),
        Some(0.0)
    );
    assert_eq!(end_x, 8.0);
    assert_eq!(end_col, 1);
    assert_eq!(charpos, 5);
    builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[1];
            assert_eq!(text.len(), 1);
            assert_eq!(text[0].face_id, FaceId::new(7));
            assert!(matches!(
                text[0].glyph_type,
                neomacs_display_protocol::glyph_matrix::GlyphType::Char { ch: 'a' }
            ));
        })
        .expect("current row");
}

#[test]
fn buffer_text_source_render_request_appends_plain_text_run_with_cursor_inside() {
    let mut context = RowTransitionTestContext::new("source-char-render-request");
    let buf_id = context
        .eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = context
            .eval
            .buffer_manager_mut()
            .get_mut(buf_id)
            .expect("buffer");
        buffer.insert("éβ");
    }
    let snapshot = current_buffer_snapshot(&context.eval, buf_id);
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let default_face = face_resolver.default_face().clone();
    let measurement_policy =
        DisplayRowMeasurementPolicy::for_mode(DisplayRowMeasurementMode::LogicalCells);
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let overlay_context = BufferOverlayStringTextRowRenderContext::new(
        false,
        1,
        &surface,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        0.0,
        0,
        4,
    );
    let params = test_display_space_window_params();
    let text = "éβ".as_bytes();
    let mut byte_idx = 0;
    let mut invisible_text_checkpoint = InvisibleTextScanCheckpoint::new(0);
    let mut charpos = 0;
    let mut col = 0;
    let mut row_extend = DisplayRowScopedValue::inactive();
    let mut box_face = BoxFaceRowState::inactive();
    let mut x = 0.0;
    let mut line_numbers = LineNumberRenderState::new(false, 0, 0);
    let mut hit_row_range = HitRowRangeTracker::new(0);
    let mut prefix_request = DisplayRowPrefixRequest::None;
    let mut hscroll_skip = HorizontalScrollSkipState::new(
        LineWrapMode::Wrap,
        0,
        HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph,
    );
    let mut word_wrap = WordWrapRenderState::new(false);
    let mut trailing_whitespace = TrailingWhitespaceRenderState::new(false, 0);
    let mut face_scan = FaceScanCheckpoint::initial();
    let mut font_metrics = None;
    let mut cursor_info = CursorCaptureState::new();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(7);
    let mut source_walk = BufferSourceWalk::new(buf_id, &snapshot, charpos, 0);
    let face_resolution_context = BufferSourceFaceResolutionContext::new(
        &snapshot,
        &face_resolver,
        measurement_policy,
        &default_face,
        BasicFaceId::Default.into(),
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        neovm_core::emacs_core::image_catalog::ImageScaleEnvironment::default(),
    );
    let loop_context = BufferSourceLoopRequestContext::new(
        buf_id,
        0,
        2,
        1,
        &params,
        0.0,
        false,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        DisplayRowVisibilityLimit {
            max_rows: 4,
            bottom_y: 64.0,
        },
        context.defaults,
        0,
        4,
        context.row_limit,
        Color::from_pixel(0x00FFFFFF),
    );

    let continue_buffer_walk = BufferSourceRenderRequest::new(
        loop_context,
        text,
        &params,
        &active_face,
        BufferSourceLoopMutableState::new(
            &mut invisible_text_checkpoint,
            DisplaySourceProgressState::new(&mut byte_idx, &mut charpos, &mut x, &mut col),
            text_row_source_render_state(
                &mut context.builder,
                &mut context.output_emitter,
                &mut context.eval,
                &mut font_metrics,
                &face_resolver,
            ),
            BufferSourceRowBuildState::new(
                &mut context.geometry,
                &mut context.row_flags,
                &mut row_extend,
                &mut box_face,
            ),
            BufferSourceHitCaptureState::new(&mut context.hit_rows, &mut hit_row_range),
            BufferSourceRowCarryoverState::new(
                &mut prefix_request,
                &mut line_numbers,
                &mut hscroll_skip,
                &mut word_wrap,
                &mut trailing_whitespace,
            ),
            &mut face_scan,
            &mut context.row_y_positions,
            &mut cursor_info,
            &mut face_ids,
            BufferSourceSurfaceContext::new(&surface, overlay_context),
        ),
    )
    .render_next_and_apply(&mut source_walk, face_resolution_context, &snapshot);

    assert!(continue_buffer_walk);
    assert_eq!(byte_idx, 4);
    assert_eq!(charpos, 2);
    assert_eq!(x, 16.0);
    assert_eq!(col, 2);
    let cursor = cursor_info.as_ref().expect("cursor inside whole text run");
    assert_eq!(cursor.byte_idx, 2);
    assert_eq!(cursor.col, 1);
    assert_eq!(cursor.x, 8.0);
    assert_eq!(cursor.slot_width, Some(8.0));
    context
        .builder
        .edit_current_row_for_test(|row| {
            let text_glyphs = &row.glyphs[GlyphArea::Text as usize];
            assert_eq!(text_glyphs.len(), 2);
        })
        .expect("current row");
}

#[test]
fn buffer_text_source_render_request_keeps_space_run_whole_when_trailing_enabled() {
    let mut context = RowTransitionTestContext::new("source-run-trailing-whitespace-enabled");
    let buf_id = context
        .eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = context
            .eval
            .buffer_manager_mut()
            .get_mut(buf_id)
            .expect("buffer");
        buffer.insert("a ");
    }
    let snapshot = current_buffer_snapshot(&context.eval, buf_id);
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let default_face = face_resolver.default_face().clone();
    let measurement_policy =
        DisplayRowMeasurementPolicy::for_mode(DisplayRowMeasurementMode::LogicalCells);
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let overlay_context = BufferOverlayStringTextRowRenderContext::new(
        false,
        1,
        &surface,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        0.0,
        0,
        4,
    );
    let mut params = test_display_space_window_params();
    params.wrap_mode = LineWrapMode::Truncate;
    let text = b"a ";
    let mut byte_idx = 0;
    let mut invisible_text_checkpoint = InvisibleTextScanCheckpoint::new(0);
    let mut charpos = 0;
    let mut col = 0;
    let mut row_extend = DisplayRowScopedValue::inactive();
    let mut box_face = BoxFaceRowState::inactive();
    let mut x = 0.0;
    let mut line_numbers = LineNumberRenderState::new(false, 0, 0);
    let mut hit_row_range = HitRowRangeTracker::new(0);
    let mut prefix_request = DisplayRowPrefixRequest::None;
    let mut hscroll_skip = HorizontalScrollSkipState::new(
        LineWrapMode::Truncate,
        0,
        HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph,
    );
    let mut word_wrap = WordWrapRenderState::new(false);
    let mut trailing_whitespace = TrailingWhitespaceRenderState::new(true, 0x00ff00);
    trailing_whitespace.track_rendered_char(' ', context.geometry.start_marker_at_x(0.0));
    let mut face_scan = FaceScanCheckpoint::initial();
    let mut font_metrics = None;
    let mut cursor_info = CursorCaptureState::new();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(7);
    let mut source_walk = BufferSourceWalk::new(buf_id, &snapshot, charpos, 0);
    let face_resolution_context = BufferSourceFaceResolutionContext::new(
        &snapshot,
        &face_resolver,
        measurement_policy,
        &default_face,
        BasicFaceId::Default.into(),
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        neovm_core::emacs_core::image_catalog::ImageScaleEnvironment::default(),
    );
    let loop_context = BufferSourceLoopRequestContext::new(
        buf_id,
        0,
        2,
        99,
        &params,
        0.0,
        false,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        DisplayRowVisibilityLimit {
            max_rows: 4,
            bottom_y: 64.0,
        },
        context.defaults,
        0,
        4,
        context.row_limit,
        Color::from_pixel(0x00FFFFFF),
    );

    let continue_buffer_walk = BufferSourceRenderRequest::new(
        loop_context,
        text,
        &params,
        &active_face,
        BufferSourceLoopMutableState::new(
            &mut invisible_text_checkpoint,
            DisplaySourceProgressState::new(&mut byte_idx, &mut charpos, &mut x, &mut col),
            text_row_source_render_state(
                &mut context.builder,
                &mut context.output_emitter,
                &mut context.eval,
                &mut font_metrics,
                &face_resolver,
            ),
            BufferSourceRowBuildState::new(
                &mut context.geometry,
                &mut context.row_flags,
                &mut row_extend,
                &mut box_face,
            ),
            BufferSourceHitCaptureState::new(&mut context.hit_rows, &mut hit_row_range),
            BufferSourceRowCarryoverState::new(
                &mut prefix_request,
                &mut line_numbers,
                &mut hscroll_skip,
                &mut word_wrap,
                &mut trailing_whitespace,
            ),
            &mut face_scan,
            &mut context.row_y_positions,
            &mut cursor_info,
            &mut face_ids,
            BufferSourceSurfaceContext::new(&surface, overlay_context),
        ),
    )
    .render_next_and_apply(&mut source_walk, face_resolution_context, &snapshot);

    assert!(continue_buffer_walk);
    assert_eq!(byte_idx, 2);
    assert_eq!(charpos, 2);
    assert_eq!(x, 16.0);
    assert_eq!(col, 2);
    assert_eq!(
        trailing_whitespace
            .highlight_start_x(&context.geometry)
            .map(|(_color, x)| x),
        Some(8.0)
    );
    context
        .builder
        .edit_current_row_for_test(|row| {
            let text_glyphs = &row.glyphs[GlyphArea::Text as usize];
            assert_eq!(text_glyphs.len(), 2);
            assert!(matches!(
                text_glyphs[0].glyph_type,
                GlyphType::Char { ch: 'a' }
            ));
            assert!(matches!(
                text_glyphs[1].glyph_type,
                GlyphType::Char { ch: ' ' }
            ));
        })
        .expect("current row");
}

#[test]
fn buffer_text_source_render_request_keeps_space_run_whole_when_word_wrap_enabled() {
    let mut context = RowTransitionTestContext::new("source-run-word-wrap-enabled");
    let buf_id = context
        .eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = context
            .eval
            .buffer_manager_mut()
            .get_mut(buf_id)
            .expect("buffer");
        buffer.insert("a b");
    }
    let snapshot = current_buffer_snapshot(&context.eval, buf_id);
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let default_face = face_resolver.default_face().clone();
    let measurement_policy =
        DisplayRowMeasurementPolicy::for_mode(DisplayRowMeasurementMode::LogicalCells);
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let overlay_context = BufferOverlayStringTextRowRenderContext::new(
        false,
        1,
        &surface,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        0.0,
        0,
        4,
    );
    let params = test_display_space_window_params();
    let text = b"a b";
    let mut byte_idx = 0;
    let mut invisible_text_checkpoint = InvisibleTextScanCheckpoint::new(0);
    let mut charpos = 0;
    let mut col = 0;
    let mut row_extend = DisplayRowScopedValue::inactive();
    let mut box_face = BoxFaceRowState::inactive();
    let mut x = 0.0;
    let mut line_numbers = LineNumberRenderState::new(false, 0, 0);
    let mut hit_row_range = HitRowRangeTracker::new(0);
    let mut prefix_request = DisplayRowPrefixRequest::None;
    let mut hscroll_skip = HorizontalScrollSkipState::new(
        LineWrapMode::Wrap,
        0,
        HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph,
    );
    let mut word_wrap = WordWrapRenderState::new(true);
    let mut trailing_whitespace = TrailingWhitespaceRenderState::new(false, 0);
    let mut face_scan = FaceScanCheckpoint::initial();
    let mut font_metrics = None;
    let mut cursor_info = CursorCaptureState::new();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(7);
    let mut source_walk = BufferSourceWalk::new(buf_id, &snapshot, charpos, 0);
    let face_resolution_context = BufferSourceFaceResolutionContext::new(
        &snapshot,
        &face_resolver,
        measurement_policy,
        &default_face,
        BasicFaceId::Default.into(),
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        neovm_core::emacs_core::image_catalog::ImageScaleEnvironment::default(),
    );
    let loop_context = BufferSourceLoopRequestContext::new(
        buf_id,
        0,
        3,
        99,
        &params,
        0.0,
        false,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        DisplayRowVisibilityLimit {
            max_rows: 4,
            bottom_y: 64.0,
        },
        context.defaults,
        0,
        4,
        context.row_limit,
        Color::from_pixel(0x00FFFFFF),
    );

    let continue_buffer_walk = BufferSourceRenderRequest::new(
        loop_context,
        text,
        &params,
        &active_face,
        BufferSourceLoopMutableState::new(
            &mut invisible_text_checkpoint,
            DisplaySourceProgressState::new(&mut byte_idx, &mut charpos, &mut x, &mut col),
            text_row_source_render_state(
                &mut context.builder,
                &mut context.output_emitter,
                &mut context.eval,
                &mut font_metrics,
                &face_resolver,
            ),
            BufferSourceRowBuildState::new(
                &mut context.geometry,
                &mut context.row_flags,
                &mut row_extend,
                &mut box_face,
            ),
            BufferSourceHitCaptureState::new(&mut context.hit_rows, &mut hit_row_range),
            BufferSourceRowCarryoverState::new(
                &mut prefix_request,
                &mut line_numbers,
                &mut hscroll_skip,
                &mut word_wrap,
                &mut trailing_whitespace,
            ),
            &mut face_scan,
            &mut context.row_y_positions,
            &mut cursor_info,
            &mut face_ids,
            BufferSourceSurfaceContext::new(&surface, overlay_context),
        ),
    )
    .render_next_and_apply(&mut source_walk, face_resolution_context, &snapshot);

    assert!(continue_buffer_walk);
    assert_eq!(byte_idx, 3);
    assert_eq!(charpos, 3);
    assert_eq!(x, 24.0);
    assert_eq!(col, 3);
    assert!(word_wrap.has_candidate());
    assert_eq!(word_wrap.candidate().byte_idx(), 2);
    assert_eq!(word_wrap.candidate().charpos(), 2);
    assert_eq!(word_wrap.candidate().display_point_count(), 2);
    context
        .builder
        .edit_current_row_for_test(|row| {
            let text_glyphs = &row.glyphs[GlyphArea::Text as usize];
            assert_eq!(text_glyphs.len(), 3);
            assert!(matches!(
                text_glyphs[0].glyph_type,
                GlyphType::Char { ch: 'a' }
            ));
            assert!(matches!(
                text_glyphs[1].glyph_type,
                GlyphType::Char { ch: ' ' }
            ));
            assert!(matches!(
                text_glyphs[2].glyph_type,
                GlyphType::Char { ch: 'b' }
            ));
        })
        .expect("current row");
}

#[test]
fn buffer_text_source_render_request_renders_fit_prefix_before_overflow() {
    let mut context = RowTransitionTestContext::new("source-run-fit-prefix");
    let buf_id = context
        .eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = context
            .eval
            .buffer_manager_mut()
            .get_mut(buf_id)
            .expect("buffer");
        buffer.insert("abc");
    }
    let snapshot = current_buffer_snapshot(&context.eval, buf_id);
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let default_face = face_resolver.default_face().clone();
    let measurement_policy =
        DisplayRowMeasurementPolicy::for_mode(DisplayRowMeasurementMode::LogicalCells);
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 32.0, 32.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let overlay_context = BufferOverlayStringTextRowRenderContext::new(
        false,
        1,
        &surface,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        0.0,
        0,
        4,
    );
    let params = test_display_space_window_params();
    let text = b"abcdefghij";
    let mut byte_idx = 0;
    let mut invisible_text_checkpoint = InvisibleTextScanCheckpoint::new(0);
    let mut charpos = 0;
    let mut col = 0;
    let mut row_extend = DisplayRowScopedValue::inactive();
    let mut box_face = BoxFaceRowState::inactive();
    let mut x = 0.0;
    let mut line_numbers = LineNumberRenderState::new(false, 0, 0);
    let mut hit_row_range = HitRowRangeTracker::new(0);
    let mut prefix_request = DisplayRowPrefixRequest::None;
    let mut hscroll_skip = HorizontalScrollSkipState::new(
        LineWrapMode::Wrap,
        0,
        HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph,
    );
    let mut word_wrap = WordWrapRenderState::new(false);
    let mut trailing_whitespace = TrailingWhitespaceRenderState::new(false, 0);
    let mut face_scan = FaceScanCheckpoint::initial();
    let mut font_metrics = None;
    let mut cursor_info = CursorCaptureState::new();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(7);
    let mut source_walk = BufferSourceWalk::new(buf_id, &snapshot, charpos, 0);
    let face_resolution_context = BufferSourceFaceResolutionContext::new(
        &snapshot,
        &face_resolver,
        measurement_policy,
        &default_face,
        BasicFaceId::Default.into(),
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        neovm_core::emacs_core::image_catalog::ImageScaleEnvironment::default(),
    );
    let loop_context = BufferSourceLoopRequestContext::new(
        buf_id,
        0,
        10,
        99,
        &params,
        0.0,
        false,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        DisplayRowVisibilityLimit {
            max_rows: 4,
            bottom_y: 64.0,
        },
        context.defaults,
        0,
        4,
        context.row_limit,
        Color::from_pixel(0x00FFFFFF),
    );

    let continue_buffer_walk = BufferSourceRenderRequest::new(
        loop_context,
        text,
        &params,
        &active_face,
        BufferSourceLoopMutableState::new(
            &mut invisible_text_checkpoint,
            DisplaySourceProgressState::new(&mut byte_idx, &mut charpos, &mut x, &mut col),
            text_row_source_render_state(
                &mut context.builder,
                &mut context.output_emitter,
                &mut context.eval,
                &mut font_metrics,
                &face_resolver,
            ),
            BufferSourceRowBuildState::new(
                &mut context.geometry,
                &mut context.row_flags,
                &mut row_extend,
                &mut box_face,
            ),
            BufferSourceHitCaptureState::new(&mut context.hit_rows, &mut hit_row_range),
            BufferSourceRowCarryoverState::new(
                &mut prefix_request,
                &mut line_numbers,
                &mut hscroll_skip,
                &mut word_wrap,
                &mut trailing_whitespace,
            ),
            &mut face_scan,
            &mut context.row_y_positions,
            &mut cursor_info,
            &mut face_ids,
            BufferSourceSurfaceContext::new(&surface, overlay_context),
        ),
    )
    .render_next_and_apply(&mut source_walk, face_resolution_context, &snapshot);

    assert!(continue_buffer_walk);
    assert!(
        byte_idx > 1 && byte_idx < text.len(),
        "expected multi-char prefix before overflow, got byte_idx={byte_idx}"
    );
    assert_eq!(charpos, byte_idx as i64);
    assert_eq!(x, 8.0 * byte_idx as f32);
    assert_eq!(col, byte_idx);
    context
        .builder
        .edit_current_row_for_test(|row| {
            let text_glyphs = &row.glyphs[GlyphArea::Text as usize];
            assert_eq!(text_glyphs.len(), byte_idx);
            for (idx, glyph) in text_glyphs.iter().enumerate() {
                let expected = (b'a' + idx as u8) as char;
                assert!(matches!(glyph.glyph_type, GlyphType::Char { ch } if ch == expected));
            }
        })
        .expect("current row");
}

#[test]
fn buffer_text_source_append_context_prepares_current_text_row_source_char() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("buffer-text-prepares-source-char", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.insert("ab");
    }
    let snapshot = {
        let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
        LayoutBufferSnapshot::from_buffer(buffer)
    };
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let mut font_metrics = None;
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let append_context = BufferSourceRowAppendContext::new(
        &snapshot,
        buf_id,
        &surface,
        &active_face,
        0.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
    );
    let source_char = DisplaySourceTextChar::new(
        'a',
        CharPos0::new(0),
        crate::types::NobreakDisplayMode::Escape,
    );
    let source_item =
        buffer_source_mapped_display_item(buf_id, 0, 1, "a", RenderFaceRef::FaceId(FaceId::new(7)));
    let mut append_state = DisplaySourceRowAppendState::default();
    let mut source_render = text_row_source_render_state(
        &mut builder,
        &mut output_emitter,
        &mut eval,
        &mut font_metrics,
        &face_resolver,
    );

    let prepared_append = append_context
        .prepare_source_item_for_current_text_row(
            geometry,
            &mut append_state,
            &mut source_render,
            &source_char,
            b"a",
            0,
            DisplayRowPosition::new(0.0, 0),
            &source_item,
        )
        .into_text()
        .expect("ordinary buffer char should prepare text append");

    assert_eq!(
        prepared_append.overflow_decision(
            'a',
            80.0,
            LineWrapMode::Wrap,
            WordWrapRenderState::new(false)
        ),
        DisplayRowTextOverflowDecision::Fits
    );
}

#[test]
fn buffer_end_of_buffer_cursor_action_captures_visible_eob_cursor() {
    let active_face = test_active_face_state(FaceId::new(9), 8.0);
    let geometry = DisplayRowGeometryState::new(2, 32.0, 0.0, 16.0, 12.0);
    let action = BufferSourceEndOfBufferCursorAction::new(5, 9, 9, 9);
    let mut cursor = CursorCaptureState::new();

    action.capture_cursor_if_point(&mut cursor, &active_face, &geometry, 48.0, 6);

    let captured = cursor.as_ref().expect("cursor captured");
    assert_eq!(captured.x, 48.0);
    assert_eq!(captured.y, 32.0);
    assert_eq!(captured.byte_idx, 5);
    assert_eq!(captured.col, 6);
    assert_eq!(captured.display_row_offset, 2);
    assert_eq!(captured.slot_width, Some(8.0));
    assert!(!captured.stretch_like);
}

#[test]
fn buffer_end_of_buffer_cursor_action_keeps_cursor_missing_when_point_differs() {
    let active_face = test_active_face_state(FaceId::new(9), 8.0);
    let geometry = DisplayRowGeometryState::new(2, 32.0, 0.0, 16.0, 12.0);
    let action = BufferSourceEndOfBufferCursorAction::new(5, 9, 12, 10);
    let mut cursor = CursorCaptureState::new();

    action.capture_cursor_if_point(&mut cursor, &active_face, &geometry, 48.0, 6);

    assert!(cursor.as_ref().is_none());
}

#[test]
fn buffer_end_of_buffer_tail_action_reports_cursor_state() {
    let active_face = test_active_face_state(FaceId::new(9), 8.0);
    let geometry = DisplayRowGeometryState::new(2, 32.0, 0.0, 16.0, 12.0);
    let action = BufferSourceEndOfBufferTailAction::new(5, 9, 9, 9);
    let mut cursor = CursorCaptureState::new();

    assert!(action.point_is_visible_eob());
    action.capture_cursor_if_point(&mut cursor, &active_face, &geometry, 48.0, 6);

    let captured = cursor.as_ref().expect("cursor captured");
    assert_eq!(captured.x, 48.0);
    assert_eq!(captured.display_row_offset, 2);
}

#[test]
fn buffer_overlay_string_context_reports_render_gate() {
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let geometry = DisplayRowGeometryState::new(2, 32.0, 0.0, 16.0, 12.0);
    let past_limit = DisplayRowGeometryState::new(4, 64.0, 0.0, 16.0, 12.0);
    let enabled = BufferOverlayStringTextRowRenderContext::new(
        true,
        1,
        &surface,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        0.0,
        0,
        4,
    );
    let disabled = BufferOverlayStringTextRowRenderContext::new(
        false,
        1,
        &surface,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        0.0,
        0,
        4,
    );

    assert!(enabled.should_render(&geometry));
    assert!(!enabled.should_render(&past_limit));
    assert!(!disabled.should_render(&geometry));
}

#[test]
fn buffer_end_of_buffer_tail_render_request_captures_cursor_and_renders_overlay() {
    let mut context = RowTransitionTestContext::new("eob-tail-render-request");
    let buf_id = context
        .eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = context
            .eval
            .buffer_manager_mut()
            .get_mut(buf_id)
            .expect("buffer");
        buffer.insert("abc");
        let eob = buffer.point_max_emacs_byte_pos().get();
        let overlay = Value::make_overlay(neovm_core::heap_types::OverlayDataInit {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(buf_id),
            start: eob,
            end: eob,
            front_advance: false,
            rear_advance: false,
        });
        buffer.overlays_mut().insert_overlay(overlay);
        let _ = buffer.overlays_mut().overlay_put(
            overlay,
            Value::symbol("before-string"),
            Value::string("Z"),
        );
    }

    let snapshot = current_buffer_snapshot(&context.eval, buf_id);
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let overlay_context = BufferOverlayStringTextRowRenderContext::new(
        true,
        1,
        &surface,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        0.0,
        0,
        4,
    );
    let mut x = 24.0;
    let mut col = 3;
    let mut cursor_info = CursorCaptureState::new();
    let mut hit_row_range = HitRowRangeTracker::new(0);
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(7);
    let mut line_numbers = LineNumberRenderState::new(false, 1, 1);
    let mut face_scan = FaceScanCheckpoint::initial();
    let mut font_metrics = None;

    let outcome =
        BufferSourceEndOfBufferTailRenderContext::new(3, 3, 3, 3, overlay_context, &active_face)
            .render_and_apply(
                &snapshot,
                text_row_source_render_state(
                    &mut context.builder,
                    &mut context.output_emitter,
                    &mut context.eval,
                    &mut font_metrics,
                    &face_resolver,
                ),
                DisplaySourceRowProgressState::new(&mut x, &mut col),
                &mut context.geometry,
                &mut cursor_info,
                &mut context.hit_rows,
                &mut hit_row_range,
                &mut context.row_y_positions,
                &mut face_ids,
                &mut line_numbers,
                &mut face_scan,
            );

    assert!(outcome.point_is_visible_eob());
    let captured = cursor_info.captured().expect("EOB cursor captured");
    assert_eq!(captured.x, 24.0);
    assert_eq!(captured.col, 3);
    assert_eq!(x, 32.0);
    assert_eq!(col, 4);
    let eob = context
        .output_emitter
        .point_for_lisp_buffer_pos(LispCharPos1::new(4))
        .expect("visible EOB insertion boundary");
    assert_eq!((eob.x, eob.y, eob.width, eob.height), (24, 0, 8, 18));
    assert_eq!((eob.row, eob.col), (0, 3));
    context
        .builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[GlyphArea::Text.index()];
            assert_eq!(text.len(), 1);
            assert!(matches!(text[0].glyph_type, GlyphType::Char { ch: 'Z' }));
        })
        .expect("current row");
}

#[test]
fn buffer_text_window_tail_finalize_request_publishes_cursor_and_finishes_row() {
    let mut context = RowTransitionTestContext::new("tail-finalize-request");
    let mut params = test_display_space_window_params();
    params.window_id = 1;
    params.selected = true;
    params.cursor_color = 0x00ffffff;
    params.text_bounds = Rect::new(0.0, 0.0, 160.0, 48.0);
    params.visual_cursors = vec![crate::types::VisualCursorSpec {
        id: -42,
        charpos: 0,
        cursor_kind: neomacs_display_protocol::frame_glyphs::CursorKind::Bar,
        cursor_bar_width: neomacs_display_protocol::frame_glyphs::CursorBarWidth::new(3),
        color: 0x00112233,
        effects: None,
    }];
    context
        .output_emitter
        .push_display_point(LispCharPos1::ONE, 34.0, 20.0, 11.0, 16.0, 0, 4);

    let mut cursor_info = CursorCaptureState::new();
    cursor_info.capture_once(crate::display_cursor::CapturedCursorInfo {
        x: 0.0,
        y: 0.0,
        face_w: 8.0,
        face_h: 16.0,
        face_ascent: 12.0,
        fg: Color::WHITE,
        bg: Color::BLACK,
        byte_idx: 0,
        col: 0,
        display_row_offset: 0,
        slot_width: Some(8.0),
        stretch_like: false,
        slot_state: crate::display_cursor::CursorSlotResolutionState::Unresolved,
        display_replacement_anchor_charpos: None,
    });
    let mut hit_row_range = HitRowRangeTracker::new(0);

    let outcome = TextWindowTailFinalizeRequest::new(TextWindowTailFinalizeContext::new(
        &params,
        b"abc",
        0,
        0.0,
        0.0,
        0.0,
        48.0,
        8.0,
        16.0,
        0,
        0,
        3,
        false,
        context.row_limit,
    ))
    .finalize_and_apply(TextWindowTailFinalizeState::new(
        &mut cursor_info,
        &context.geometry,
        &context.row_y_positions,
        &mut hit_row_range,
        &mut context.hit_rows,
        text_row_output_render_state(
            &mut context.builder,
            &mut context.output_emitter,
            &mut context.eval,
        ),
    ));

    assert!(outcome.cursor_requested());
    assert_eq!(
        outcome.cursor_publish_status(),
        TextWindowCursorPublishStatus::Published
    );
    assert!(outcome.cursor_published());
    assert!(outcome.pending_row_finished());
    assert_eq!(outcome.visual_cursor_summary().requested, 1);
    assert_eq!(outcome.visual_cursor_summary().published, 1);
    assert_eq!(context.hit_rows.len(), 1);
    let cursor = context.builder.phys_cursor().expect("physical cursor");
    assert_eq!(cursor.window_id.get(), 1);
    assert_eq!(cursor.row, 0);
    assert_eq!(cursor.col, 0);
    assert_eq!(cursor.x, 0.0);
    assert_eq!(cursor.height, 16.0);
    let cursors = context.builder.cursors();
    assert_eq!(cursors.len(), 1);
    assert_eq!(cursors[0].window_id.get(), -42);
    assert_eq!(cursors[0].slot_id.row, 0);
    assert_eq!(cursors[0].slot_id.col, 4);
}

#[test]
fn buffer_text_window_tail_finalize_reports_missing_cursor_capture() {
    let mut context = RowTransitionTestContext::new("tail-finalize-missing-cursor");
    let mut params = test_display_space_window_params();
    params.window_id = 1;
    params.selected = true;
    params.cursor_color = 0x00ffffff;
    params.text_bounds = Rect::new(0.0, 0.0, 160.0, 48.0);

    let mut cursor_info = CursorCaptureState::new();
    let mut hit_row_range = HitRowRangeTracker::new(0);

    let outcome = TextWindowTailFinalizeRequest::new(TextWindowTailFinalizeContext::new(
        &params,
        b"abc",
        0,
        0.0,
        0.0,
        0.0,
        48.0,
        8.0,
        16.0,
        0,
        0,
        3,
        false,
        context.row_limit,
    ))
    .finalize_and_apply(TextWindowTailFinalizeState::new(
        &mut cursor_info,
        &context.geometry,
        &context.row_y_positions,
        &mut hit_row_range,
        &mut context.hit_rows,
        text_row_output_render_state(
            &mut context.builder,
            &mut context.output_emitter,
            &mut context.eval,
        ),
    ));

    assert!(outcome.cursor_requested());
    assert_eq!(
        outcome.cursor_publish_status(),
        TextWindowCursorPublishStatus::MissingCapture
    );
    assert!(!outcome.cursor_published());
    assert!(context.builder.phys_cursor().is_none());
}

#[test]
fn buffer_text_window_body_install_request_records_positions_and_edge_markers() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("body-install-request", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(41, 1, 5, Rect::new(0.0, 0.0, 40.0, 20.0), true);
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    crate::window_output::begin_text_window_row(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut output_emitter,
        &mut eval,
        crate::window_output::DisplayTextRowBegin {
            display_row_index: 0,
            row: 0,
            col: 0,
            y: 2.0,
            x: 0.0,
            start_charpos: LayoutCharPos0::new(0),
        },
    );
    output_emitter.note_display_buffer_pos(LispCharPos1::new(7));
    builder
        .edit_current_row_for_test(|row| {
            crate::glyph_row_writer::push_stretch_to_row(row, 3, FaceId::new(7), 24.0, 0.0, 0.0, 0);
        })
        .expect("current row");
    crate::window_output::finish_text_window_row(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut output_emitter,
        crate::window_output::DisplayTextRowMetrics {
            y: 2.0,
            height: 20.0,
            ascent: 15.0,
        },
    );

    let mut row_flags = DisplayRowFlags::new(1);
    row_flags.mark(0, DisplayRowFlagKind::Truncated);
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(10);
    let mut font_metrics = None;
    let positions = TextWindowBodyInstallRequest::new(TextWindowBodyInstallRenderContext::new(
        41,
        3,
        100,
        4,
        true,
        false,
        0,
        5,
        &row_flags,
        FaceId::new(9),
        face_resolver.default_face(),
        8.0,
    ))
    .install_and_apply(TextWindowBodyInstallState::new(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut output_emitter,
        crate::display_status_line::ChromeRowRenderServices::new(
            &mut font_metrics,
            &face_resolver,
            &mut face_ids,
        ),
    ));

    assert_eq!(positions.window_start(), LispCharPos1::new(4));
    assert_eq!(positions.window_end_lisp(), LispCharPos1::new(8));
    assert_eq!(
        positions.window_end_position().anchor().emacs_byte_pos(),
        EmacsBytePos::new(104)
    );
    assert_eq!(positions.window_end_position().matrix_row().get(), 0);

    builder.end_window();
    let state = builder.finish(5, 1, 8.0, 16.0);
    let row = &state.window_matrices[0].matrix.rows[0];
    assert_eq!(row.height_px, 20.0);
    assert_eq!(row.ascent_px, 15.0);
    let text = &row.glyphs[GlyphArea::Text.index()];
    assert_eq!(text.len(), 3, "one stretch glyph occupies three columns");
    assert_eq!(text[0].glyph_type, GlyphType::Stretch { width_cols: 3 });
    assert!(matches!(text[1].glyph_type, GlyphType::Char { ch: ' ' }));
    assert!(matches!(text[2].glyph_type, GlyphType::Char { ch: '$' }));
    assert_eq!(text[2].face_id, FaceId::new(9));
}

#[test]
fn buffer_text_window_begin_request_opens_window_and_first_text_row() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("begin-request", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    let mut output_emitter = TextWindowBeginRequest::new(
        frame_id,
        window_id,
        2,
        10.0,
        5.0,
        41,
        4,
        8,
        Rect::new(3.0, 5.0, 80.0, 64.0),
        Rect::new(10.0, 9.0, 64.0, 48.0),
        Rect::new(10.0, 9.0, 64.0, 48.0),
        true,
        crate::window_output::DisplayTextRowBegin {
            display_row_index: 2,
            row: 0,
            col: 1,
            y: 9.0,
            x: 18.0,
            start_charpos: LayoutCharPos0::new(0),
        },
    )
    .begin_and_apply(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut eval,
    );

    output_emitter.move_text_output_to(&mut eval, 0, 3, 9.0, 34.0);
    crate::window_output::finish_text_window_row(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut output_emitter,
        crate::window_output::DisplayTextRowMetrics {
            y: 9.0,
            height: 17.0,
            ascent: 12.0,
        },
    );
    crate::window_output::close_text_window_output(TextWindowOutputTarget::from_builder(
        &mut builder,
    ));

    let state = builder.finish(8, 4, 8.0, 16.0);
    assert_eq!(state.window_matrices.len(), 1);
    let window = &state.window_matrices[0];
    assert_eq!(window.window_id, DisplayWindowId::new(41));
    assert!(window.selected);
    assert_eq!(window.pixel_bounds, Rect::new(3.0, 5.0, 80.0, 64.0));
    assert_eq!(window.text_pixel_bounds, Rect::new(10.0, 9.0, 64.0, 48.0));
    assert_eq!(window.matrix.rows[2].role, GlyphRowRole::Text);
    assert_eq!(window.matrix.rows[2].pixel_y, 4.0);
    assert_eq!(window.matrix.rows[2].height_px, 17.0);
    assert_eq!(window.matrix.rows[2].ascent_px, 12.0);
}

#[test]
fn buffer_text_window_cursor_effects_request_installs_effect_profile() {
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    let effects = EffectsConfig::default();

    let installed = TextWindowCursorEffectsRequest::new(42, Some(effects.clone()))
        .install_and_apply(TextWindowOutputTarget::from_builder(&mut builder));

    assert!(installed);
    let state = builder.finish(1, 1, 8.0, 16.0);
    assert_eq!(
        state
            .cursor_effects_by_window
            .get(&neomacs_display_protocol::types::DisplayWindowId::new(42)),
        Some(&effects)
    );
}

#[test]
fn buffer_text_window_cursor_effects_request_ignores_missing_effect_profile() {
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();

    let installed = TextWindowCursorEffectsRequest::new(42, None)
        .install_and_apply(TextWindowOutputTarget::from_builder(&mut builder));

    assert!(!installed);
    let state = builder.finish(1, 1, 8.0, 16.0);
    assert!(
        !state
            .cursor_effects_by_window
            .contains_key(&neomacs_display_protocol::types::DisplayWindowId::new(42))
    );
}

#[test]
fn buffer_text_window_terminal_right_border_request_installs_face_and_border() {
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 5, Rect::new(0.0, 0.0, 40.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    for ch in "abcd".chars() {
        write_char_to_current_row_with_width(&mut builder, ch, FaceId::new(0), 0, 8.0);
    }
    builder.end_row();
    builder.end_window();

    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(10);
    let effective_default_face = crate::display_face_policy::EffectiveWindowDefaultFace::resolve(
        &face_resolver,
        face_resolver.default_face(),
        &mut face_ids,
    );
    let mut font_metrics = None;
    let face_id = TextWindowTerminalRightBorderRequest::new(8.0).install_and_apply(
        TextWindowOutputTarget::from_builder(&mut builder),
        crate::display_status_line::ChromeRowRenderServices::new(
            &mut font_metrics,
            &face_resolver,
            &mut face_ids,
        ),
        &effective_default_face,
    );

    let state = builder.finish(5, 1, 8.0, 16.0);
    assert!(state.faces.contains_key(&face_id));
    let row = &state.window_matrices[0].matrix.rows[0];
    let text = &row.glyphs[GlyphArea::Text.index()];
    let right = &row.glyphs[GlyphArea::RightMargin.index()];
    assert_eq!(text.len(), 4);
    assert_eq!(right.len(), 1);
    assert_eq!(right[0].glyph_type, GlyphType::Char { ch: '|' });
    assert_eq!(right[0].face_id, face_id);
}

#[test]
fn terminal_right_border_decoration_preserves_the_rows_semantic_role() {
    // GNU `build_frame_matrix_from_leaf_window` replaces only the reserved
    // LAST_AREA glyph with the vertical-border glyph.  It does not turn a
    // mode-line row into a text row; that role still selects window-wide
    // bounds and keeps the row outside the body-text clip.
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 5, Rect::new(0.0, 0.0, 40.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::ModeLine);
    write_char_to_current_row_with_width(&mut builder, '-', FaceId::new(1), 0, 8.0);
    builder.end_row();
    builder.end_window();

    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(10);
    let effective_default_face = crate::display_face_policy::EffectiveWindowDefaultFace::resolve(
        &face_resolver,
        face_resolver.default_face(),
        &mut face_ids,
    );
    let mut font_metrics = None;
    TextWindowTerminalRightBorderRequest::new(8.0).install_and_apply(
        TextWindowOutputTarget::from_builder(&mut builder),
        crate::display_status_line::ChromeRowRenderServices::new(
            &mut font_metrics,
            &face_resolver,
            &mut face_ids,
        ),
        &effective_default_face,
    );

    let state = builder.finish(5, 1, 8.0, 16.0);
    let row = &state.window_matrices[0].matrix.rows[0];
    assert_eq!(row.role, GlyphRowRole::ModeLine);
    assert_eq!(
        row.glyphs[GlyphArea::RightMargin.index()]
            .last()
            .map(|glyph| glyph.glyph_type.clone()),
        Some(GlyphType::Char { ch: '|' }),
        "GNU installs the vertical border in every enabled window row"
    );
}

#[test]
fn terminal_right_border_face_id_comes_from_the_shared_frame_allocator() {
    // Slice 10 B / GNU `face_cache->used` (xfaces.c `lookup_face`): the TTY right
    // border must draw its realized face id from the single per-frame allocator,
    // so it can never alias a window's dynamic content faces (which are keyed
    // from that same allocator). Before this fix the border used a separate
    // `FaceResolver` counter that ALSO started at SENTINEL, so a multi-window TTY
    // frame whose window had both a dynamic content face AND a right border could
    // collapse them onto one id — silent in single-window fixtures.
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 5, Rect::new(0.0, 0.0, 40.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    for ch in "abcd".chars() {
        write_char_to_current_row_with_width(&mut builder, ch, FaceId::new(0), 0, 8.0);
    }
    builder.end_row();
    builder.end_window();

    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let effective_default_face = crate::display_face_policy::EffectiveWindowDefaultFace::resolve(
        &face_resolver,
        face_resolver.default_face(),
        &mut face_ids,
    );
    // A dynamic (non-basic) content face takes the first id from the frame
    // allocator, exactly as a propertized buffer-text run would.
    let content_face_id = face_ids.reserve_dynamic_face();
    let mut font_metrics = None;
    let border_face_id = TextWindowTerminalRightBorderRequest::new(8.0).install_and_apply(
        TextWindowOutputTarget::from_builder(&mut builder),
        crate::display_status_line::ChromeRowRenderServices::new(
            &mut font_metrics,
            &face_resolver,
            &mut face_ids,
        ),
        &effective_default_face,
    );

    assert_ne!(
        border_face_id, content_face_id,
        "border face id must not collide with a window's dynamic content face id"
    );
    assert_eq!(
        border_face_id,
        FaceId::new(content_face_id.get() + 1),
        "border face id must be the next id from the shared frame allocator"
    );
}

#[test]
fn buffer_text_window_terminal_right_border_request_pads_blank_rows_and_preserves_marker() {
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 3, 5, Rect::new(0.0, 0.0, 40.0, 48.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    for ch in "ABCD$".chars() {
        write_char_to_current_row_with_width(&mut builder, ch, FaceId::new(0), 0, 8.0);
    }
    builder.end_row();
    builder.begin_row(2, GlyphRowRole::Text);
    write_char_to_current_row_with_width(&mut builder, 'Z', FaceId::new(0), 0, 8.0);
    builder.end_row();
    builder.end_window();

    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(10);
    let effective_default_face = crate::display_face_policy::EffectiveWindowDefaultFace::resolve(
        &face_resolver,
        face_resolver.default_face(),
        &mut face_ids,
    );
    let mut font_metrics = None;
    let face_id = TextWindowTerminalRightBorderRequest::new(8.0).install_and_apply(
        TextWindowOutputTarget::from_builder(&mut builder),
        crate::display_status_line::ChromeRowRenderServices::new(
            &mut font_metrics,
            &face_resolver,
            &mut face_ids,
        ),
        &effective_default_face,
    );

    let state = builder.finish(5, 3, 8.0, 16.0);
    let matrix = &state.window_matrices[0].matrix;
    let row_text = |row: usize| -> String {
        matrix.rows[row].glyphs[GlyphArea::Text.index()]
            .iter()
            .map(|glyph| match glyph.glyph_type {
                GlyphType::Char { ch } => ch,
                _ => '?',
            })
            .collect()
    };

    assert_eq!(row_text(0), "ABC$");
    assert_eq!(row_text(1), "    ");
    assert_eq!(row_text(2), "Z   ");
    assert_eq!(
        matrix.rows[0].glyphs[GlyphArea::Text.index()][3].face_id,
        FaceId::new(0)
    );
    assert!(
        matrix.rows[1].glyphs[GlyphArea::Text.index()]
            .iter()
            .all(|glyph| glyph.face_id == FaceId::new(0)),
        "right-border padding on blank text rows must keep the default face"
    );
    assert!(
        matrix.rows[2].glyphs[GlyphArea::Text.index()][1..]
            .iter()
            .all(|glyph| glyph.face_id == FaceId::new(0)),
        "right-border padding after text must keep the default face"
    );
    assert!(!matrix.rows[1].displays_text);
    for row in 0..3 {
        let right = &matrix.rows[row].glyphs[GlyphArea::RightMargin.index()];
        assert_eq!(right.len(), 1);
        assert_eq!(right[0].glyph_type, GlyphType::Char { ch: '|' });
        assert_eq!(right[0].face_id, face_id);
    }
}

#[test]
fn terminal_right_border_padding_uses_the_effective_window_default_face() {
    // GNU `extend_face_to_end_of_line` (xdisp.c) produces the blank TTY tail
    // with the window's already-remapped default face.  The later
    // `build_frame_matrix_from_leaf_window` border install replaces only the
    // reserved LAST_AREA cell; it must not turn the text-area tail back into
    // the frame-global default face.
    let _runtime = Context::new();
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x000000, 0xFFFFFF, 14.0, None);
    let mut buffer = Buffer::new_standalone(BufferId(42), Value::string("*border-remap*"));
    buffer.set_buffer_local(
        "face-remapping-alist",
        Value::list(vec![Value::list(vec![
            Value::symbol("default"),
            Value::list(vec![
                Value::keyword("foreground"),
                Value::string("#ffffff"),
                Value::keyword("background"),
                Value::string("#000000"),
            ]),
            Value::symbol("default"),
        ])]),
    );
    let padding_face = face_resolver.resolve_buffer_default_face(&buffer);
    let border_face = face_resolver.resolve_named_face("vertical-border");

    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 5, Rect::new(0.0, 0.0, 40.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    write_char_to_current_row_with_width(&mut builder, 'Z', FaceId::new(0), 0, 8.0);
    builder.end_row();
    builder.end_window();

    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(10);
    let effective_default_face = crate::display_face_policy::EffectiveWindowDefaultFace::resolve(
        &face_resolver,
        &padding_face,
        &mut face_ids,
    );
    let expected_padding_face_id = effective_default_face.face_id();
    let border_face_id =
        crate::display_row::face_state::stable_face_id_for_resolved(&mut face_ids, &border_face);
    let mut font_metrics = None;
    crate::display_row::special_glyphs::install_text_window_right_border_rows(
        &mut builder,
        crate::display_status_line::ChromeRowRenderServices::new(
            &mut font_metrics,
            &face_resolver,
            &mut face_ids,
        ),
        crate::display_row::special_glyphs::TextWindowRightBorder {
            ch: '|',
            face_id: border_face_id,
            char_width: 8.0,
        },
        &border_face,
        &effective_default_face,
    );

    let state = builder.finish(5, 1, 8.0, 16.0);
    assert!(
        state.window_matrices[0].matrix.rows[0].glyphs[GlyphArea::Text.index()][1..]
            .iter()
            .all(|glyph| glyph.face_id == expected_padding_face_id),
        "the terminal border's synthetic spaces must preserve the window-remapped default face"
    );
}

#[test]
fn buffer_text_window_finish_request_closes_window_and_returns_snapshot_artifacts() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("finish-request", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(41, 1, 5, Rect::new(0.0, 0.0, 40.0, 20.0), true);
    let output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 10.0, 5.0);
    output_emitter.begin_update(&mut eval);
    let hit_rows = vec![crate::hit_test::HitRow {
        y_start: 2.0,
        y_end: 18.0,
        charpos_start: 3,
        charpos_end: 9,
    }];

    let finished = TextWindowFinishRequest::new(
        neovm_core::window::geometry::CellOrigin::new(0, 0),
        neovm_core::window::PresentedWindowRegions {
            outer: Rect::new(0.0, 0.0, 40.0, 20.0),
            text_body: Rect::new(2.0, 0.0, 38.0, 20.0),
            ..Default::default()
        },
        11,
        7,
        5,
    )
    .finish_and_snapshot(TextWindowFinishState::new(
        TextWindowOutputTarget::from_builder(&mut builder),
        output_emitter,
        &mut eval,
        hit_rows,
    ));
    let snapshot = finished.into_snapshot();
    assert_eq!(snapshot.cell_origin.column().get(), 0);
    assert_eq!(snapshot.cell_origin.line().get(), 0);
    assert_eq!(snapshot.regions.outer, Rect::new(0.0, 0.0, 40.0, 20.0));
    assert_eq!(snapshot.regions.text_body, Rect::new(2.0, 0.0, 38.0, 20.0));
    assert!(snapshot.regions_materialized);
    assert_eq!(snapshot.text_area_left_offset, 2);
    assert_eq!(snapshot.mode_line_height, 11);
    assert_eq!(snapshot.header_line_height, 7);
    assert_eq!(snapshot.tab_line_height, 5);

    let state = builder.finish(5, 1, 8.0, 16.0);
    assert_eq!(state.window_matrices.len(), 1);
    assert_eq!(state.window_matrices[0].window_id, DisplayWindowId::new(41));
}

#[test]
fn buffer_text_window_visibility_retry_request_scrolls_down_from_visible_rows() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let buffer_size = {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.insert("abcdefghijklmnopqrstuvwxyz\n");
        buffer.point_max_char_pos().get() as i64
    };
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let access = RustBufferAccess::new(buffer);
    let rows = vec![
        emitted_row(0, 0, 16, 1, 8),
        emitted_row(1, 16, 16, 9, 16),
        emitted_row(2, 32, 16, 17, 24),
    ];

    let outcome = TextWindowVisibilityRetryRequest::new(
        &rows,
        1,
        0,
        buffer_size,
        30,
        24,
        false,
        0,
        48,
        ScrollPolicy::Unlimited,
        0,
        &access,
    )
    .decide();

    assert_eq!(outcome.visible_end_lisp(), Some(LispCharPos1::new(24)));
    assert!(outcome.point_beyond_visible_span());
    // Point is two display lines past the bottom row, so the window start moves
    // down two rows (to the start of row 2). Scrolling to the visible end (24)
    // would throw away the whole window -- GNU's `try_scrolling` never does
    // that, and doing so is what #195 saw as an arbitrary page break.
    assert_eq!(outcome.scroll_down_window_start(), Some(16));
    assert_eq!(outcome.retry_window_start(), Some(16));
}

#[test]
fn buffer_text_window_visibility_retry_request_detects_partially_visible_point_row() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let buffer_size = {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.insert("abcdefghijklmnopqrstuvwxyz\n");
        buffer.point_max_char_pos().get() as i64
    };
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let access = RustBufferAccess::new(buffer);
    let rows = vec![
        emitted_row(0, 0, 20, 1, 10),
        emitted_row(1, 20, 20, 11, 20),
        emitted_row(2, 40, 30, 21, 30),
    ];

    let outcome = TextWindowVisibilityRetryRequest::new(
        &rows,
        1,
        0,
        buffer_size,
        25,
        30,
        false,
        0,
        60,
        ScrollPolicy::Unlimited,
        0,
        &access,
    )
    .decide();

    assert!(!outcome.point_beyond_visible_span());
    assert_eq!(outcome.point_row_window_start(), Some(10));
    assert_eq!(outcome.retry_window_start(), Some(10));
}

#[test]
fn buffer_text_window_visibility_retry_request_detects_point_line_continuation() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let buffer_size = {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.insert("abcdefghijklmnopqrstuvwxyz\n");
        buffer.point_max_char_pos().get() as i64
    };
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let access = RustBufferAccess::new(buffer);
    let rows = vec![
        emitted_row(0, 0, 16, 1, 10),
        emitted_row(1, 16, 16, 11, 20),
        emitted_row(2, 32, 16, 21, 25),
    ];

    let outcome = TextWindowVisibilityRetryRequest::new(
        &rows,
        1,
        0,
        buffer_size,
        21,
        25,
        false,
        0,
        48,
        ScrollPolicy::Unlimited,
        0,
        &access,
    )
    .decide();

    assert_eq!(outcome.point_line_window_start(), Some(20));
    assert_eq!(outcome.retry_window_start(), Some(20));
}

#[test]
fn measure_buffer_text_source_range_append_uses_shared_renderer_without_mutating_row() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.insert("ab");
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("measure-buffer-fragment-append", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);

    let snapshot = {
        let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
        LayoutBufferSnapshot::from_buffer(buffer)
    };
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let surface = test_advance_resolution_surface();
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let mut font_metrics = None;
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    write_char_to_current_row_with_width(&mut builder, 'x', FaceId::new(7), 0, 8.0);
    let position = DisplayRowPosition::new(8.0, 1);
    let source_range = DisplaySourceTextRange::new(CharPos0::new(1), CharPos0::new(2));
    let source_item =
        buffer_source_mapped_display_item(buf_id, 1, 2, "b", RenderFaceRef::FaceId(FaceId::new(7)));

    let append_context = BufferSourceRowAppendContext::new(
        &snapshot,
        buf_id,
        &surface,
        &active_face,
        0.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
    );
    let mut append_state = DisplaySourceRowAppendState::default();
    let measured_width = append_context
        .resolve_source_render_plan_to_text_row(
            &geometry,
            &mut append_state,
            &mut text_row_source_measure_state(
                &mut builder,
                &mut eval,
                &mut font_metrics,
                &face_resolver,
            ),
            DisplaySourceRenderPlanRequest::new(
                b"b",
                0,
                source_range,
                DisplaySourceClusterState::for_char('b', None),
            ),
            position,
            &source_item,
        )
        .advance_px();

    builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[1];
            assert_eq!(text.len(), 1);
            assert!(matches!(text[0].glyph_type, GlyphType::Char { ch: 'x' }));
        })
        .expect("current row");

    let appended = append_context
        .append_source_text_request_to_text_row(
            &geometry,
            &mut text_row_source_render_state(
                &mut builder,
                &mut output_emitter,
                &mut eval,
                &mut font_metrics,
                &face_resolver,
            ),
            DisplaySourceTextRequest::new(
                source_range,
                'b',
                DisplaySourceAppendRenderPlan::natural(measured_width),
            ),
            position,
        )
        .expect("appended buffer fragment");
    let end = appended.end();

    assert_eq!(end.x_px() - position.x_px(), measured_width);
    assert_eq!(appended.metrics().width_px(), measured_width);
}

#[test]
fn buffer_text_source_append_context_uses_resolved_render_plan() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.insert("a");
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("append-buffer-resolved-fragment", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);

    let snapshot = {
        let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
        LayoutBufferSnapshot::from_buffer(buffer)
    };
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let surface = test_advance_resolution_surface();
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let mut font_metrics = None;
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);

    let append_context = BufferSourceRowAppendContext::new(
        &snapshot,
        buf_id,
        &surface,
        &active_face,
        0.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
    );
    let progress = append_context
        .append_source_text_request_to_text_row(
            &geometry,
            &mut text_row_source_render_state(
                &mut builder,
                &mut output_emitter,
                &mut eval,
                &mut font_metrics,
                &face_resolver,
            ),
            DisplaySourceTextRequest::new(
                DisplaySourceTextRange::new(CharPos0::new(0), CharPos0::new(1)),
                'a',
                DisplaySourceAppendRenderPlan::resolved_advance(13.0),
            ),
            DisplayRowPosition::new(0.0, 0),
        )
        .expect("appended resolved buffer fragment");
    let end = progress.end();

    assert_eq!(end, DisplayRowPosition::new(13.0, 1));
    assert_eq!(progress.metrics().width_px(), 13.0);
    builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[1];
            assert_eq!(text.len(), 1);
            assert_eq!(text[0].pixel_width, 13.0);
        })
        .expect("current row");
}

#[test]
fn buffer_text_source_append_context_uses_resolved_item_face_for_fragment_base() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.insert("a");
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("append-buffer-item-face-base", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);

    let snapshot = current_buffer_snapshot(&eval, buf_id);
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let active_face = test_active_face_state(FaceId::new(32), 8.0);
    let mut item_face = active_face.resolved_face().clone();
    item_face.fg = 0x0051afef;
    item_face.bg = 0x00282c34;
    item_face.use_default_background = false;
    let surface = test_advance_resolution_surface();
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let mut font_metrics = None;
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);

    let append_context = BufferSourceRowAppendContext::new(
        &snapshot,
        buf_id,
        &surface,
        &active_face,
        0.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
    )
    .with_resolved_item_face(FaceId::new(32), item_face);
    let item = buffer_display_item(
        buf_id,
        0,
        1,
        RenderFaceRef::FaceId(FaceId::new(32)),
        DisplayItemKind::TextRun(crate::display_item::DisplayTextRun::new("a")),
    );
    let mut render_policy = DisplaySourceAppendRenderPolicy::natural();

    append_context
        .append_source_display_item_to_text_row(
            &geometry,
            &mut text_row_source_render_state(
                &mut builder,
                &mut output_emitter,
                &mut eval,
                &mut font_metrics,
                &face_resolver,
            ),
            item,
            DisplayRowPosition::new(0.0, 0),
            DisplayRowAppendKind::SourceText,
            &mut render_policy,
        )
        .expect("appended source item");

    builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[GlyphArea::Text.index()];
            assert_eq!(text.len(), 1);
            assert_eq!(text[0].face_id, FaceId::new(32));
        })
        .expect("current row");
    let face = builder
        .output_face(FaceId::new(32))
        .expect("item face installed");
    assert_eq!(face.foreground, Color::from_pixel(0x0051afef));
    assert_eq!(face.background, Color::from_pixel(0x00282c34));
    assert!(!face.use_default_background);
}

#[test]
fn buffer_text_source_append_context_never_rebinds_an_unknown_item_face_id() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    eval.buffer_manager_mut()
        .get_mut(buf_id)
        .expect("buffer")
        .insert("a");
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("append-buffer-unknown-item-face", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);

    let snapshot = current_buffer_snapshot(&eval, buf_id);
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let surface = test_advance_resolution_surface();
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let mut font_metrics = None;

    let unknown_id = FaceId::new(32);
    let mut face_attempt = FrameFaceAttempt::for_test_with_next_id(33);
    let mut existing = Face::new(unknown_id);
    existing.foreground = Color::from_pixel(0x0051afef);
    face_attempt
        .publish(existing.clone())
        .expect("publish the existing immutable rendering");

    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.set_face_attempt(face_attempt.clone());
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);

    let append_context = BufferSourceRowAppendContext::from_active_face_row(
        &snapshot,
        buf_id,
        &surface,
        &active_face,
        0.0,
        16.0,
        face_attempt,
    );
    let item = buffer_display_item(
        buf_id,
        0,
        1,
        RenderFaceRef::FaceId(unknown_id),
        DisplayItemKind::TextRun(crate::display_item::DisplayTextRun::new("a")),
    );
    let mut render_policy = DisplaySourceAppendRenderPolicy::natural();

    append_context
        .append_source_display_item_to_text_row(
            &geometry,
            &mut text_row_source_render_state(
                &mut builder,
                &mut output_emitter,
                &mut eval,
                &mut font_metrics,
                &face_resolver,
            ),
            item,
            DisplayRowPosition::new(0.0, 0),
            DisplayRowAppendKind::SourceText,
            &mut render_policy,
        )
        .expect("unknown item face falls back as one complete active-face identity");

    builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[GlyphArea::Text.index()];
            assert_eq!(text.len(), 1);
            assert_eq!(text[0].face_id, active_face.face_id());
        })
        .expect("current row");
    assert_eq!(
        builder.output_face(unknown_id),
        Some(existing),
        "fallback must not overwrite the rendering already bound to the unknown id"
    );
}

#[test]
fn buffer_text_source_append_context_composes_with_current_row_tail() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.insert("e\u{301}");
    }
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("append-buffer-combining-char", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);

    let snapshot = {
        let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
        LayoutBufferSnapshot::from_buffer(buffer)
    };
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let surface = test_advance_resolution_surface();
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let mut font_metrics = None;
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    write_char_to_current_row_with_width(&mut builder, 'e', FaceId::new(7), 0, 8.0);

    let append_context = BufferSourceRowAppendContext::new(
        &snapshot,
        buf_id,
        &surface,
        &active_face,
        0.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
    );
    let progress = append_context
        .append_source_text_request_to_text_row(
            &geometry,
            &mut text_row_source_render_state(
                &mut builder,
                &mut output_emitter,
                &mut eval,
                &mut font_metrics,
                &face_resolver,
            ),
            DisplaySourceTextRequest::new(
                DisplaySourceTextRange::new(CharPos0::new(1), CharPos0::new(2)),
                '\u{301}',
                DisplaySourceAppendRenderPlan::natural(0.0),
            ),
            DisplayRowPosition::new(8.0, 1),
        )
        .expect("appended combining buffer char");
    let end = progress.end();

    assert_eq!(end, DisplayRowPosition::new(8.0, 1));
    assert_eq!(progress.metrics().width_px(), 0.0);
    assert_eq!(progress.metrics().width_cols(), 0);
    builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[1];
            assert_eq!(text.len(), 1);
            assert!(matches!(
                &text[0].glyph_type,
                GlyphType::Composite { text } if text.as_ref() == "e\u{301}"
            ));
        })
        .expect("current row");
}

#[test]
fn buffer_text_item_append_context_builds_control_char_item() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.insert("\u{0001}");
    }
    let snapshot = {
        let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
        LayoutBufferSnapshot::from_buffer(buffer)
    };
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("append-buffer-text-item-fragment", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);
    let table = neovm_core::face::FaceTable::new();
    let face_resolver =
        crate::neovm_bridge::FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base_face = face_resolver.default_face();
    let mut font_metrics = None;
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    let frame = test_append_frame(8.0, 8.0, DisplayTabPolicy::every(8));
    let item = DisplaySourceAppendItem::ControlChar { ch: '\u{0001}' };
    let source_item = DisplaySourceItemRequest::new(
        DisplaySourceTextRange::new(CharPos0::new(0), CharPos0::new(1)),
        item.clone(),
    );

    let append_context =
        BufferSourceRequestAppendContext::new(&snapshot, buf_id, FaceId::new(7), base_face, frame);
    let measured_width = append_context
        .try_measure_source_request_width_to_text_row(
            &mut text_row_source_measure_state(
                &mut builder,
                &mut eval,
                &mut font_metrics,
                &face_resolver,
            ),
            source_item.clone(),
            DisplayRowPosition::new(0.0, 0),
        )
        .expect("measured buffer text item fragment");
    builder
        .edit_current_row_for_test(|row| assert!(row.glyphs[1].is_empty()))
        .expect("current row");
    let fallback_width = append_context.measure_source_request_width_to_text_row(
        &mut text_row_source_measure_state(
            &mut builder,
            &mut eval,
            &mut font_metrics,
            &face_resolver,
        ),
        DisplaySourceItemRequest::new(
            DisplaySourceTextRange::new(CharPos0::new(0), CharPos0::new(0)),
            item.clone(),
        ),
        DisplayRowPosition::new(0.0, 0),
    );
    let edge_width = append_context.measure_source_request_width_to_text_row(
        &mut text_row_source_measure_state(
            &mut builder,
            &mut eval,
            &mut font_metrics,
            &face_resolver,
        ),
        source_item.clone(),
        DisplayRowPosition::new(80.0, 10),
    );

    let progress = append_context
        .append_source_request_to_text_row_and_emit(
            &mut text_row_source_render_state(
                &mut builder,
                &mut output_emitter,
                &mut eval,
                &mut font_metrics,
                &face_resolver,
            ),
            source_item,
            DisplayRowPosition::new(0.0, 0),
        )
        .expect("appended buffer text item fragment");
    let end = progress.end();

    assert_eq!(end, DisplayRowPosition::new(16.0, 2));
    assert_eq!(measured_width, 16.0);
    assert_eq!(fallback_width, 16.0);
    assert_eq!(edge_width, 16.0);
    assert_eq!(progress.metrics().width_px(), measured_width);
    builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[1];
            assert_eq!(text.len(), 2);
            assert!(matches!(
                text[0].glyph_type,
                neomacs_display_protocol::glyph_matrix::GlyphType::Char { ch: '^' }
            ));
            assert!(matches!(
                text[1].glyph_type,
                neomacs_display_protocol::glyph_matrix::GlyphType::Char { ch: 'A' }
            ));
        })
        .expect("current row");
}

#[test]
fn buffer_text_source_append_item_names_nobreak_display_policy() {
    assert_eq!(
        DisplaySourceAppendItem::nobreak_display(
            '\u{00A0}',
            crate::types::NobreakDisplayMode::HighlightOriginal,
        ),
        Some(DisplaySourceAppendItem::SourceMappedText {
            text: "\u{00A0}".into()
        })
    );
    assert_eq!(
        DisplaySourceAppendItem::nobreak_display(
            '\u{00A0}',
            crate::types::NobreakDisplayMode::HighlightAscii,
        ),
        Some(DisplaySourceAppendItem::SourceMappedText { text: " ".into() })
    );
    assert_eq!(
        DisplaySourceAppendItem::nobreak_display(
            '\u{00AD}',
            crate::types::NobreakDisplayMode::HighlightAscii,
        ),
        Some(DisplaySourceAppendItem::SourceMappedText { text: "-".into() })
    );
    assert_eq!(
        DisplaySourceAppendItem::nobreak_display(
            '\u{00A0}',
            crate::types::NobreakDisplayMode::Escape,
        ),
        Some(DisplaySourceAppendItem::SourceMappedText { text: "\\ ".into() })
    );
    assert_eq!(
        DisplaySourceAppendItem::nobreak_display(
            '\u{00AD}',
            crate::types::NobreakDisplayMode::Escape,
        ),
        Some(DisplaySourceAppendItem::SourceMappedText { text: "\\-".into() })
    );
    assert_eq!(
        DisplaySourceAppendItem::nobreak_display(
            '\u{00A0}',
            crate::types::NobreakDisplayMode::Literal,
        ),
        None
    );
    assert_eq!(
        DisplaySourceAppendItem::nobreak_display('x', crate::types::NobreakDisplayMode::Escape,),
        None
    );
}

#[test]
fn buffer_text_source_special_display_names_precluster_policy() {
    assert_eq!(
        DisplaySourceSpecialDisplay::for_precluster_char(
            '\u{0001}',
            crate::types::NobreakDisplayMode::Escape,
        ),
        Some(DisplaySourceSpecialDisplay::Control(
            DisplaySourceAppendItem::ControlChar { ch: '\u{0001}' }
        ))
    );
    assert_eq!(
        DisplaySourceSpecialDisplay::for_precluster_char(
            '\u{007F}',
            crate::types::NobreakDisplayMode::Escape,
        ),
        Some(DisplaySourceSpecialDisplay::Control(
            DisplaySourceAppendItem::ControlChar { ch: '\u{007F}' }
        ))
    );
    assert_eq!(
        DisplaySourceSpecialDisplay::for_precluster_char(
            '\u{00A0}',
            crate::types::NobreakDisplayMode::Escape,
        ),
        Some(DisplaySourceSpecialDisplay::Nobreak(
            DisplaySourceAppendItem::SourceMappedText { text: "\\ ".into() }
        ))
    );
    assert_eq!(
        DisplaySourceSpecialDisplay::for_precluster_char(
            '\n',
            crate::types::NobreakDisplayMode::Escape,
        ),
        None
    );
    assert_eq!(
        DisplaySourceSpecialDisplay::for_precluster_char(
            '\t',
            crate::types::NobreakDisplayMode::Escape,
        ),
        None
    );
    assert_eq!(
        DisplaySourceSpecialDisplay::for_precluster_char(
            'x',
            crate::types::NobreakDisplayMode::Escape,
        ),
        None
    );
}

#[test]
fn buffer_text_source_special_display_names_cluster_policy() {
    assert_eq!(
        DisplaySourceSpecialDisplay::for_cluster_state(DisplaySourceClusterState::for_char(
            '\u{200E}',
            Some(('a', false)),
        )),
        Some(DisplaySourceSpecialDisplay::Glyphless(
            DisplaySourceAppendItem::Glyphless {
                ch: '\u{200E}',
                method: GlyphlessMethod::ZeroWidth,
            }
        ))
    );
    assert_eq!(
        DisplaySourceSpecialDisplay::for_cluster_state(DisplaySourceClusterState::for_char(
            '\u{FE0F}',
            Some(('\u{2764}', false)),
        )),
        None
    );
    assert_eq!(
        DisplaySourceSpecialDisplay::for_cluster_state(DisplaySourceClusterState::for_char(
            'x', None,
        )),
        None
    );
}

#[test]
fn buffer_text_source_char_names_range_and_precluster_policy() {
    let source_char = DisplaySourceTextChar::new(
        '\u{00A0}',
        CharPos0::new(4),
        crate::types::NobreakDisplayMode::Escape,
    );
    let request = source_char
        .nobreak_special_request()
        .expect("nobreak source char should produce source request");

    assert_eq!(
        source_char.range(),
        DisplaySourceTextRange::new(CharPos0::new(4), CharPos0::new(5))
    );
    assert!(source_char.control_special_request().is_none());
    assert_eq!(
        source_char
            .special_request(None)
            .map(|request| request.kind()),
        Some(DisplaySourceSpecialDisplayKind::Nobreak)
    );
    let expected_request = DisplaySpecialSourceCharRequest::new(
        &source_char,
        DisplaySourceSpecialDisplay::Nobreak(DisplaySourceAppendItem::SourceMappedText {
            text: "\\ ".into(),
        }),
    );
    assert_eq!(
        request.append_plan_at(
            DisplayRowPosition::new(0.0, 0),
            buffer_special_request_display_item(&request),
        ),
        expected_request.append_plan_at(
            DisplayRowPosition::new(0.0, 0),
            buffer_special_request_display_item(&expected_request),
        )
    );
}

#[test]
fn buffer_text_source_char_names_cluster_policy() {
    let source_char = DisplaySourceTextChar::new(
        '\u{FE0F}',
        CharPos0::new(1),
        crate::types::NobreakDisplayMode::Escape,
    );
    let cluster_tail = Some(('\u{2764}', false));

    assert_eq!(
        source_char.cluster_state(cluster_tail),
        DisplaySourceClusterState::for_char('\u{FE0F}', cluster_tail)
    );
    assert_eq!(source_char.cluster_special_request(cluster_tail), None);

    let standalone_joiner = DisplaySourceTextChar::new(
        '\u{200D}',
        CharPos0::new(2),
        crate::types::NobreakDisplayMode::Escape,
    );
    assert_eq!(
        standalone_joiner
            .special_request(None)
            .map(|request| request.append_plan_at(
                DisplayRowPosition::new(0.0, 0),
                buffer_special_request_display_item(&request),
            )),
        DisplaySourceSpecialDisplay::for_cluster_state(DisplaySourceClusterState::for_char(
            '\u{200D}', None
        ))
        .map(|display| {
            let request = DisplaySpecialSourceCharRequest::new(&standalone_joiner, display);
            request.append_plan_at(
                DisplayRowPosition::new(0.0, 0),
                buffer_special_request_display_item(&request),
            )
        })
    );
}

#[test]
fn buffer_text_source_append_item_names_fallback_widths() {
    let empty_source_mapped = DisplaySourceAppendItem::SourceMappedText { text: "".into() };
    let glyphless = DisplaySourceAppendItem::Glyphless {
        ch: '\u{200E}',
        method: GlyphlessMethod::ZeroWidth,
    };
    let frame = test_append_frame(8.0, 8.0, DisplayTabPolicy::every(8));

    assert_eq!(
        DisplaySourceAppendItem::ControlChar { ch: '\u{0001}' }
            .fallback_width()
            .column_count(),
        2
    );
    assert_eq!(
        DisplaySourceAppendItem::SourceMappedText { text: "\\ ".into() }
            .fallback_width()
            .column_count(),
        2
    );
    assert_eq!(empty_source_mapped.fallback_width().column_count(), 1);
    assert_eq!(glyphless.fallback_width().column_count(), 1);
    assert_eq!(
        empty_source_mapped
            .fallback_width()
            .resolve_to_text_row(&frame),
        8.0
    );
    assert_eq!(glyphless.fallback_width().resolve_to_text_row(&frame), 8.0);
    assert_eq!(
        DisplaySourceItemRequest::new(
            DisplaySourceTextRange::new(CharPos0::new(0), CharPos0::new(0)),
            empty_source_mapped,
        )
        .fallback_width()
        .column_count(),
        1
    );
    assert_eq!(
        DisplaySourceItemRequest::new(
            DisplaySourceTextRange::new(CharPos0::new(0), CharPos0::new(1)),
            glyphless,
        )
        .fallback_width()
        .column_count(),
        1
    );
}

#[test]
fn buffer_text_source_append_item_names_glyphless_display_policy() {
    let variation_selector_state =
        DisplaySourceClusterState::for_char('\u{FE0F}', Some(('\u{2764}', false)));
    assert!(variation_selector_state.is_cluster_continuation());

    assert_eq!(
        DisplaySourceAppendItem::glyphless_display(DisplaySourceClusterState::for_char(
            '\u{fffc}', None,
        )),
        Some(DisplaySourceAppendItem::Glyphless {
            ch: '\u{fffc}',
            method: GlyphlessMethod::EmptyBox,
        })
    );
    assert_eq!(
        DisplaySourceAppendItem::glyphless_display(DisplaySourceClusterState::for_char(
            '\u{FE0F}', None,
        )),
        Some(DisplaySourceAppendItem::Glyphless {
            ch: '\u{FE0F}',
            method: GlyphlessMethod::ZeroWidth,
        })
    );
    assert_eq!(
        DisplaySourceAppendItem::glyphless_display(variation_selector_state),
        None
    );
    assert_eq!(
        DisplaySourceAppendItem::glyphless_display(DisplaySourceClusterState::for_char(
            '\u{200E}',
            Some(('a', false)),
        )),
        Some(DisplaySourceAppendItem::Glyphless {
            ch: '\u{200E}',
            method: GlyphlessMethod::ZeroWidth,
        })
    );
    assert_eq!(
        DisplaySourceAppendItem::glyphless_display(DisplaySourceClusterState::for_char('x', None,)),
        None
    );
}

#[test]
fn buffer_text_item_append_context_builds_mapped_item() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id = eval.frame_manager_mut().create_frame(
        "append-buffer-source-mapped-fragment",
        320,
        120,
        buf_id,
    );
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);

    let snapshot = current_buffer_snapshot(&eval, buf_id);
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let mut font_metrics = None;
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let append_context = BufferSourceRowAppendContext::new(
        &snapshot,
        buf_id,
        &surface,
        &active_face,
        0.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
    );
    let source_char = DisplaySourceTextChar::new(
        '\u{00A0}',
        CharPos0::new(0),
        crate::types::NobreakDisplayMode::Escape,
    );
    let source_request = source_char
        .special_request(None)
        .expect("nobreak source char should map to a display item");
    let source_item = buffer_source_item_append_request(
        source_request.source_item_request(),
        buf_id,
        &snapshot,
        active_face.face_id(),
    )
    .expect("nobreak source item")
    .into_item();
    let prepared_append = append_context.prepare_special_source_char_at(
        &geometry,
        &mut text_row_source_measure_state(
            &mut builder,
            &mut eval,
            &mut font_metrics,
            &face_resolver,
        ),
        source_request,
        DisplayRowPosition::new(0.0, 0),
        &source_item,
    );
    assert_eq!(
        prepared_append.kind(),
        DisplaySourceSpecialDisplayKind::Nobreak
    );
    assert_eq!(
        prepared_append.overflow_decision(0.0, 80.0, LineWrapMode::Wrap),
        None
    );
    assert_eq!(
        prepared_append.overflow_action(0.0, 80.0, LineWrapMode::Wrap),
        None
    );
    let mut params = test_display_space_window_params();
    params.nobreak_char_fg = 0x00ff00;
    // The special-char append no longer allocates a (discarded) policy face id.
    // The escape-glyph / nobreak face merge is realized earlier, during active-
    // face resolution (`resolve_source_item_layout_for_active_face`), so this
    // append leaves the face-id allocator untouched.
    let mut policy_face_ids = FrameFaceAttempt::for_test_with_next_id(30);
    let mut face_scan = FaceScanCheckpoint::initial();
    *face_scan.next_check_mut() = 99;
    let mut word_wrap = WordWrapRenderState::new(true);
    let mut charpos = 8;
    let mut byte_idx = 0;
    let mut end_x = 0.0;
    let mut end_col = 0;
    let mut source_render = text_row_source_render_state(
        &mut builder,
        &mut output_emitter,
        &mut eval,
        &mut font_metrics,
        &face_resolver,
    );
    let mut progress =
        DisplaySourceProgressState::new(&mut byte_idx, &mut charpos, &mut end_x, &mut end_col);
    let continuation = prepared_append.append_to_text_row_and_apply(
        &append_context,
        &geometry,
        &params,
        &mut policy_face_ids,
        &mut source_render,
        &mut face_scan,
        &mut word_wrap,
        &mut progress,
    );
    assert_eq!(continuation, DisplaySourceAppendContinuation::Rendered);
    assert!(face_scan.should_resolve_at(1));
    assert_eq!(policy_face_ids.next_face_id_for_test(), 30);

    assert_eq!(end_x, 16.0);
    assert_eq!(end_col, 2);
    assert_eq!(charpos, 9);
    builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[1];
            assert_eq!(text.len(), 2);
            assert!(matches!(
                text[0].glyph_type,
                neomacs_display_protocol::glyph_matrix::GlyphType::Char { ch: '\\' }
            ));
            assert!(matches!(
                text[1].glyph_type,
                neomacs_display_protocol::glyph_matrix::GlyphType::Char { ch: ' ' }
            ));
        })
        .expect("current row");
}

#[test]
fn buffer_text_special_source_append_preserves_direct_control_item() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.insert("\u{0007}");
    }
    let snapshot = current_buffer_snapshot(&eval, buf_id);
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let surface = test_advance_resolution_surface();
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let mut font_metrics = None;
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    let append_context = BufferSourceRowAppendContext::new(
        &snapshot,
        buf_id,
        &surface,
        &active_face,
        0.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
    );
    let source_char = DisplaySourceTextChar::new(
        '\u{0007}',
        CharPos0::new(0),
        crate::types::NobreakDisplayMode::Escape,
    );
    let source_request = source_char
        .special_request(None)
        .expect("control source char should map to a display item");
    let source_item = crate::display_item::DisplayItem::new(
        crate::display_item::SourceSpan::new(
            DisplaySourcePosition::buffer(
                buf_id,
                CharPos0::new(0),
                neovm_core::buffer::EmacsBytePos::new(0),
            ),
            DisplaySourcePosition::buffer(
                buf_id,
                CharPos0::new(1),
                neovm_core::buffer::EmacsBytePos::new(1),
            ),
        ),
        RenderFaceRef::Inherit,
        DisplayItemKind::ControlChar { ch: '\u{0007}' },
    );

    let prepared_append = append_context.prepare_special_source_char_at(
        &geometry,
        &mut text_row_source_measure_state(
            &mut builder,
            &mut eval,
            &mut font_metrics,
            &face_resolver,
        ),
        source_request,
        DisplayRowPosition::new(0.0, 0),
        &source_item,
    );

    assert_eq!(
        prepared_append.kind(),
        DisplaySourceSpecialDisplayKind::Control
    );
    let direct_item = prepared_append.display_item().clone();
    assert!(matches!(
        direct_item.kind,
        DisplayItemKind::ControlChar { ch: '\u{0007}' }
    ));
}

#[test]
fn buffer_text_item_append_context_builds_glyphless_item() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("append-buffer-glyphless-fragment", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);

    let snapshot = current_buffer_snapshot(&eval, buf_id);
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let mut font_metrics = None;
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let append_context = BufferSourceRowAppendContext::new(
        &snapshot,
        buf_id,
        &surface,
        &active_face,
        0.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
    );
    let source_char = DisplaySourceTextChar::new(
        '\u{fffc}',
        CharPos0::new(0),
        crate::types::NobreakDisplayMode::Escape,
    );
    let source_request = source_char
        .special_request(None)
        .expect("glyphless source char should map to a display item");
    let source_item = buffer_source_item_append_request(
        source_request.source_item_request(),
        buf_id,
        &snapshot,
        active_face.face_id(),
    )
    .expect("glyphless source item")
    .into_item();
    let prepared_append = append_context.prepare_special_source_char_at(
        &geometry,
        &mut text_row_source_measure_state(
            &mut builder,
            &mut eval,
            &mut font_metrics,
            &face_resolver,
        ),
        source_request,
        DisplayRowPosition::new(0.0, 0),
        &source_item,
    );
    assert_eq!(
        prepared_append.kind(),
        DisplaySourceSpecialDisplayKind::Glyphless
    );
    assert_eq!(
        prepared_append.overflow_decision(0.0, 80.0, LineWrapMode::Wrap),
        None
    );
    assert_eq!(
        prepared_append.overflow_action(0.0, 80.0, LineWrapMode::Wrap),
        None
    );
    let mut policy_face_ids = FrameFaceAttempt::for_test_with_next_id(30);
    let params = test_display_space_window_params();
    let append_outcome = prepared_append
        .append_to_text_row(
            &append_context,
            &geometry,
            &params,
            &mut policy_face_ids,
            &mut text_row_source_render_state(
                &mut builder,
                &mut output_emitter,
                &mut eval,
                &mut font_metrics,
                &face_resolver,
            ),
        )
        .expect("appended glyphless buffer text item fragment");
    let mut face_scan = FaceScanCheckpoint::initial();
    *face_scan.next_check_mut() = 99;
    let mut end_x = 0.0;
    let mut end_col = 0;
    append_outcome.apply_to_text_row_state(&mut face_scan, &mut end_x, &mut end_col);
    assert!(!face_scan.should_resolve_at(1));
    assert_eq!(policy_face_ids.next_face_id_for_test(), 30);

    // U+FFFC uses the EmptyBox method: one column wide (was HexCode = 6 cols).
    assert_eq!(end_x, 8.0);
    assert_eq!(end_col, 1);
    builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[1];
            assert_eq!(text.len(), 1);
            assert!(matches!(
                text[0].glyph_type,
                neomacs_display_protocol::glyph_matrix::GlyphType::Glyphless { ch: '\u{fffc}', .. }
            ));
        })
        .expect("current row");
}

#[test]
fn append_lisp_string_to_text_row_stops_at_row_break() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("append-display-item-source", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);

    let table = neovm_core::face::FaceTable::new();
    let face_resolver =
        crate::neovm_bridge::FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base_face = face_resolver.default_face();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    let frame = test_append_frame(8.0, 8.0, DisplayTabPolicy::every(8));
    let end = {
        let mut font_metrics = None;
        let mut source_render = text_row_source_render_state(
            &mut builder,
            &mut output_emitter,
            &mut eval,
            &mut font_metrics,
            &face_resolver,
        );
        append_lisp_string_to_text_row(
            &mut source_render,
            Value::string("a\nb"),
            1,
            base_face,
            FaceId::new(7),
            &mut face_ids,
            frame,
            DisplayRowPosition::new(0.0, 0),
        )
    };

    assert_eq!(end, DisplayRowPosition::new(8.0, 1));
    builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[1];
            // 'a' plus the GNU append_space_for_newline glyph that a
            // terminal row appends at a real line end (string-sourced
            // newlines included -- xdisp.c:26525-26530).
            assert_eq!(text.len(), 2);
            assert!(matches!(
                text[0].glyph_type,
                neomacs_display_protocol::glyph_matrix::GlyphType::Char { ch: 'a' }
            ));
            assert!(matches!(
                text[1].glyph_type,
                neomacs_display_protocol::glyph_matrix::GlyphType::Char { ch: ' ' }
            ));
        })
        .expect("current row");
}

#[test]
fn lisp_string_source_append_context_preserves_source_after_row_break() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("render-lisp-source-row-break", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);

    let table = neovm_core::face::FaceTable::new();
    let face_resolver =
        crate::neovm_bridge::FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base_face = face_resolver.default_face();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let mut font_metrics = None;
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let first_geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let second_geometry = DisplayRowGeometryState::new(1, 16.0, 0.0, 16.0, 12.0);

    let request = LispStringSourceAppendRequest::new(
        DisplayRowPosition::new(0.0, 0),
        LispStringSourceId::OVERLAY_STRING,
        Value::string("a\nb"),
    );
    let session_request =
        LispStringSourceAppendSessionRequest::frame_local(request, FaceId::new(7), base_face);
    let row_session_request = LispStringSourceRowAppendSessionRequest::new(
        session_request,
        &surface,
        0.0,
        16.0,
        12.0,
        8.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
    );
    let mut append_context = LispStringSourceRowAppendSession::new(row_session_request)
        .expect("lisp string source session");

    let first = append_context
        .render_to_text_row_and_emit(
            &mut text_row_source_render_state(
                &mut builder,
                &mut output_emitter,
                &mut eval,
                &mut font_metrics,
                &face_resolver,
            ),
            &mut face_ids,
            &first_geometry,
            DisplayRowPosition::new(0.0, 0),
        )
        .expect("first lisp source append");

    assert_eq!(first.end_position(), DisplayRowPosition::new(8.0, 1));
    assert!(matches!(
        first.stop(),
        crate::display_row::DisplayRowRenderStop::RowBreak(_)
    ));

    let second = append_context
        .render_to_text_row_and_emit(
            &mut text_row_source_render_state(
                &mut builder,
                &mut output_emitter,
                &mut eval,
                &mut font_metrics,
                &face_resolver,
            ),
            &mut face_ids,
            &second_geometry,
            DisplayRowPosition::new(0.0, 0),
        )
        .expect("second lisp source append");

    // One column wider than before the GNU newline-space append: the
    // harness renders both string segments into a single row, so the
    // EOL space occupies a cell between them.
    assert_eq!(second.end_position(), DisplayRowPosition::new(24.0, 3));
    assert_eq!(
        second.stop(),
        crate::display_row::DisplayRowRenderStop::SourceExhausted
    );
    builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[1];
            // 'a', the append_space_for_newline glyph for the string
            // newline (GNU xdisp.c:26525-26530), then 'b'.
            assert_eq!(text.len(), 3);
            assert!(matches!(
                text[0].glyph_type,
                neomacs_display_protocol::glyph_matrix::GlyphType::Char { ch: 'a' }
            ));
            assert!(matches!(
                text[1].glyph_type,
                neomacs_display_protocol::glyph_matrix::GlyphType::Char { ch: ' ' }
            ));
            assert!(matches!(
                text[2].glyph_type,
                neomacs_display_protocol::glyph_matrix::GlyphType::Char { ch: 'b' }
            ));
        })
        .expect("current row");
}

#[test]
fn append_lisp_string_to_text_row_resolves_image_display_property_through_display_host() {
    let mut eval = Context::new();
    let requests = Arc::new(Mutex::new(Vec::new()));
    eval.set_display_host(Box::new(RecordingAppendImageHost {
        requests: Arc::clone(&requests),
    }));
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("append-lisp-string-image", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 6.0);

    let table = neovm_core::face::FaceTable::new();
    let face_resolver =
        crate::neovm_bridge::FaceResolver::new(&table, 0x00112233, 0x00445566, 14.0, None);
    let base_face = face_resolver.default_face();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    let text_bounds = Rect::new(10.0, 20.0, 160.0, 64.0);
    builder.begin_window_with_text_bounds(
        77,
        1,
        24,
        Rect::new(0.0, 0.0, 200.0, 80.0),
        text_bounds,
        true,
    );
    builder.begin_row(0, GlyphRowRole::Text);
    let value = Value::string_with_text_properties(
        "A",
        vec![StringTextPropertyRun {
            start: 0,
            end: 1,
            plist: Value::list(vec![
                Value::symbol("display"),
                Value::list(vec![
                    Value::symbol("image"),
                    Value::keyword("type"),
                    Value::symbol("png"),
                    Value::keyword("file"),
                    Value::string("./tmp/append-lisp-string.png"),
                ]),
            ]),
        }],
    );
    let frame = test_append_frame_at(
        0,
        0.0,
        6.0,
        DisplayRowAppendArea::new(text_bounds.x, 160.0, 160.0, 0.0),
        DisplayRowAppendMetrics::new(
            16.0,
            12.0,
            8.0,
            8.0,
            DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        ),
        DisplayTabPolicy::from_tab_width_and_stops(text_bounds.x, 8, &[]),
    );
    let end = {
        let mut font_metrics = None;
        let mut source_render = text_row_source_render_state(
            &mut builder,
            &mut output_emitter,
            &mut eval,
            &mut font_metrics,
            &face_resolver,
        );
        append_lisp_string_to_text_row(
            &mut source_render,
            value,
            1,
            base_face,
            FaceId::new(7),
            &mut face_ids,
            frame,
            DisplayRowPosition::new(16.0, 2),
        )
    };

    assert_eq!(end, DisplayRowPosition::new(80.0, 10));
    builder.end_row();
    builder.end_window();
    let state = builder.finish(24, 1, 8.0, 16.0);
    let frame = state.materialize();
    let image = frame
        .glyphs
        .iter()
        .find_map(|glyph| match glyph {
            neomacs_display_protocol::frame_glyphs::FrameGlyph::Image {
                window_id,
                row_role,
                clip_rect,
                image_id,
                x,
                y,
                width,
                height,
                ..
            } => Some((
                *window_id, *row_role, *clip_rect, *image_id, *x, *y, *width, *height,
            )),
            _ => None,
        })
        .expect("image materialized from its row glyph");
    assert_eq!(image.0.get(), 77);
    assert_eq!(image.1, GlyphRowRole::Text);
    assert_eq!(image.2, Some(text_bounds));
    assert_eq!(image.3.get(), 42);
    assert_eq!(
        (image.4, image.5, image.6, image.7),
        (16.0, 20.0, 64.0, 32.0)
    );
    let requests = requests.lock().expect("image requests lock");
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].colors.foreground().rgb24(), 0x00112233);
    assert_eq!(requests[0].colors.background().rgb24(), 0x00445566);
}

struct SourceMappedTextWidthByFace;

impl SourceMappedTextWidthByFace {
    fn new() -> Self {
        Self
    }
}

impl DisplayRowRenderPolicy for SourceMappedTextWidthByFace {
    fn measurement_for(
        &mut self,
        item: &crate::display_item::DisplayItem,
        face_id: FaceId,
        _font_metrics: &mut Option<FontMetricsService>,
    ) -> DisplayRowItemMeasurement {
        let DisplayItemKind::SourceMappedText(text) = &item.kind else {
            return DisplayRowItemMeasurement::Default;
        };
        let advance_px = if face_id == FaceId::new(20) {
            13.0
        } else {
            11.0
        };
        let advances = text
            .text
            .char_indices()
            .enumerate()
            .map(|(char_offset, (byte_offset, _))| {
                DisplayTextRunAdvance::new(char_offset, byte_offset, advance_px)
            })
            .collect();
        DisplayRowItemMeasurement::TextRun(DisplayTextRunMeasurement::Measured(advances))
    }
}

#[test]
fn display_replacement_string_append_item_names_cursor_and_source_policy() {
    let _eval = Context::new();
    let value = Value::string("ab");
    let item = DisplayReplacementStringSourceItem::display_property_string(
        value,
        CharPos0::new(4),
        DisplayPropertySource::TextProperty,
        9,
        8.0,
    )
    .expect("display property string item");

    assert_eq!(item.cursor_slot_width_px(), 8.0);
    assert!(!item.is_empty());
    let snapshot = item.append_source_snapshot(DisplayRowPosition::new(2.0, 1));
    assert_eq!(
        snapshot.source_id(),
        LispStringSourceId::display_replacement(9)
    );
    assert_eq!(snapshot.value(), value);
    assert_eq!(snapshot.position(), DisplayRowPosition::new(2.0, 1));
    assert_eq!(
        snapshot.origin(),
        DisplayOrigin::DisplayPropertyString {
            anchor_charpos: CharPos0::new(4),
            source: DisplayPropertySource::TextProperty,
        }
    );
    assert_eq!(
        snapshot.base_face_policy(),
        BaseFacePolicy::DisplayPropertyUnderlyingFace
    );
    assert_eq!(snapshot.cursor_slot_width_px(), 8.0);
    assert!(!snapshot.is_empty());

    let empty = DisplayReplacementStringSourceItem::display_property_string(
        Value::string(""),
        CharPos0::new(4),
        DisplayPropertySource::TextProperty,
        10,
        9.0,
    )
    .expect("empty display property string item");
    assert!(empty.is_empty());
    assert_eq!(empty.cursor_slot_width_px(), 9.0);
}

#[test]
fn display_replacement_string_append_item_measures_source_text_from_active_face() {
    let _eval = Context::new();
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let item = DisplayReplacementStringSourceItem::display_property_string(
        Value::string("abc"),
        CharPos0::new(0),
        DisplayPropertySource::TextProperty,
        11,
        8.0,
    )
    .expect("display property string item");
    let mut font_metrics = Some(crate::font::metrics::FontMetricsService::new());
    let source_item = crate::display_item::DisplayItem::new(
        crate::display_item::SourceSpan::synthetic(11, 0, 3),
        RenderFaceRef::FaceId(FaceId::new(7)),
        DisplayItemKind::SourceMappedText(DisplaySourceMappedText::new("abc")),
    );

    let measurement = item.measurement_from_active_face(
        &active_face,
        &source_item,
        FaceId::new(7),
        &mut font_metrics,
    );

    let DisplayRowItemMeasurement::TextRun(measurement) = measurement else {
        panic!("replacement string text should use a direct text-run measurement");
    };
    let DisplayTextRunMeasurement::Measured(advances) = measurement else {
        panic!("replacement string run should be measured");
    };
    assert_eq!(
        advances
            .iter()
            .map(|advance| (advance.char_offset, advance.byte_offset))
            .collect::<Vec<_>>(),
        vec![(0, 0), (1, 1), (2, 2)]
    );
}

fn test_display_property_replacement_resolve_context<'a>(
    classification: &'a DisplayPropertyClassification,
    active_face: &'a DisplayRowActiveFaceState,
    font_metrics: &'a mut Option<FontMetricsService>,
    params: &'a WindowParams,
) -> DisplayPropertyReplacementSourceResolveRequest<'a, 'static> {
    DisplayPropertyReplacementSourceResolveRequest::from_typed_replacement(
        classification,
        CharPos0::new(4),
        b"x",
        active_face,
        font_metrics,
        0.0,
        0.0,
        params,
        None,
    )
}

#[test]
fn display_property_replacement_append_item_resolves_string_replacement() {
    let _eval = Context::new();
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let mut font_metrics = None;
    let value = Value::string("ab");
    let classification = classify_display_property(value);
    let params = test_display_space_window_params();

    let item = test_display_property_replacement_resolve_context(
        &classification,
        &active_face,
        &mut font_metrics,
        &params,
    )
    .resolve()
    .expect("string replacement append item");

    let DisplayPropertyReplacementSourceItem::String(item) = item else {
        panic!("expected string replacement append item");
    };
    assert_eq!(item.cursor_slot_width_px(), 8.0);
    assert!(!item.is_empty());
}

#[test]
fn display_property_replacement_append_item_resolves_stretch_replacement() {
    let _eval = Context::new();
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let mut font_metrics = None;
    let value = Value::list(vec![
        Value::symbol("space"),
        Value::keyword("relative-width"),
        Value::fixnum(2),
        Value::keyword("height"),
        Value::fixnum(3),
    ]);
    let classification = classify_display_property(value);
    let params = test_display_space_window_params();

    let item = test_display_property_replacement_resolve_context(
        &classification,
        &active_face,
        &mut font_metrics,
        &params,
    )
    .resolve()
    .expect("stretch replacement append item");

    let DisplayPropertyReplacementSourceItem::Stretch(item) = item else {
        panic!("expected stretch replacement append item");
    };
    assert_eq!(item.width_px(), 16.0);
    assert_eq!(item.height_px(), 48.0);
}

#[test]
fn display_property_replacement_append_item_resolves_media_replacement() {
    let _eval = Context::new();
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let mut font_metrics = None;
    let media = DisplayMediaReplacement::xwidget(DisplayXwidgetItem {
        xwidget_id: neomacs_display_protocol::XwidgetId::new(17),
        webview_id: neomacs_display_protocol::WebViewId::new(170),
        width: 42.0,
        height: 11.0,
    });
    let classification = DisplayPropertyClassification::new_for_test(
        Some(DisplayReplacementProperty::Media(
            DisplayMediaReplacementProperty::Xwidget(media),
        )),
        Value::NIL,
        Default::default(),
    );
    let params = test_display_space_window_params();

    let item = test_display_property_replacement_resolve_context(
        &classification,
        &active_face,
        &mut font_metrics,
        &params,
    )
    .resolve()
    .expect("media replacement append item");

    let DisplayPropertyReplacementSourceItem::Media(
        DisplayReplacementMediaSourceResolution::Media(item),
    ) = item
    else {
        panic!("expected media replacement append item");
    };
    assert_eq!(item.width_px(), 42.0);
    assert_eq!(item.display_height_px(), 11.0);
}

#[test]
fn display_property_replacement_append_item_names_cursor_policy() {
    let _eval = Context::new();
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let mut font_metrics = None;
    let value = Value::string("ab");
    let classification = classify_display_property(value);
    let params = test_display_space_window_params();
    let string = test_display_property_replacement_resolve_context(
        &classification,
        &active_face,
        &mut font_metrics,
        &params,
    )
    .resolve()
    .expect("string replacement append item");

    assert_eq!(
        string.cursor_policy(),
        DisplayPropertyReplacementCursorPolicy::TextSlot {
            width_px: 8.0,
            stretch_like: false,
        }
    );

    let stretch = DisplayPropertyReplacementSourceItem::Stretch(
        DisplayReplacementStretchSourceItem::from_space_extents(13.0, 16.0, 12.0, 8.0),
    );
    assert_eq!(
        stretch.cursor_policy(),
        DisplayPropertyReplacementCursorPolicy::TextSlot {
            width_px: 13.0,
            stretch_like: true,
        }
    );

    let media = DisplayPropertyReplacementSourceItem::Media(
        DisplayReplacementMediaSourceResolution::Media(DisplayReplacementMediaSourceItem::new(
            DisplayMediaReplacement::xwidget(DisplayXwidgetItem {
                xwidget_id: neomacs_display_protocol::XwidgetId::new(17),
                webview_id: neomacs_display_protocol::WebViewId::new(170),
                width: 42.0,
                height: 11.0,
            }),
            active_face.metrics().row_height(),
            active_face.metrics().ascent(),
            true,
        )),
    );
    assert_eq!(
        media.cursor_policy(),
        DisplayPropertyReplacementCursorPolicy::DisplayBox {
            width_px: 42.0,
            cursor_face_height_px: 18.0,
            cursor_face_ascent_px: 13.0,
        }
    );

    let placeholder = DisplayPropertyReplacementSourceItem::Media(
        DisplayReplacementMediaSourceResolution::Placeholder(
            DisplayReplacementSourceMappedTextItem::new("[img]"),
        ),
    );
    assert_eq!(
        placeholder.cursor_policy(),
        DisplayPropertyReplacementCursorPolicy::FaceChar
    );
}

#[test]
fn display_property_replacement_row_render_request_keeps_item_policy_and_start_position() {
    let item = DisplayPropertyReplacementSourceItem::Stretch(
        DisplayReplacementStretchSourceItem::from_space_extents(13.0, 16.0, 12.0, 8.0),
    );
    let request = DisplayPropertyReplacementRowRenderRequest::from_resolved_source_item(
        crate::display_item::BufferDisplayReplacementSource::new(
            BufferId(7),
            CharPos0::new(3),
            EmacsBytePos::new(12),
        ),
        item,
        -2.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 18.0, 12.0),
        DisplayRowPosition::new(24.0, 4),
    );

    assert_eq!(
        request.cursor_policy(),
        DisplayPropertyReplacementCursorPolicy::TextSlot {
            width_px: 13.0,
            stretch_like: true,
        }
    );
    assert_eq!(request.start_position(), DisplayRowPosition::new(24.0, 4));
    let DisplayPropertyReplacementSourceItem::Stretch(item) = request.into_item() else {
        panic!("expected stretch replacement item");
    };
    assert_eq!(item.height_px(), 16.0);
    assert_eq!(item.ascent_px(), 12.0);
}

#[test]
fn display_property_replacement_row_render_request_builds_append_plan() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let buffer = current_buffer_snapshot(&eval, buf_id);
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let mut font_metrics = None;
    let value = Value::string("ab");
    let classification = classify_display_property(value);
    let params = test_display_space_window_params();
    let descriptor = DisplayPropertyReplacementDescriptor::new(
        value,
        classification,
        BufferDisplayReplacementSource::spanning(
            buf_id,
            CharPos0::new(3),
            EmacsBytePos::new(12),
            CharPos0::new(4),
            EmacsBytePos::new(13),
        ),
        ReplacementCoveredSpan::for_single_property_run(CharPos0::new(3), CharPos0::new(4)),
    );
    let request = DisplayPropertyReplacementRowRenderRequest::from_typed_replacement_descriptor(
        &descriptor,
        b"x",
        &active_face,
        &mut font_metrics,
        24.0,
        8.0,
        &params,
        None,
        -2.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 18.0, 12.0),
        DisplayRowPosition::new(24.0, 4),
    )
    .expect("display replacement row render request");

    assert_eq!(
        request.cursor_policy(),
        DisplayPropertyReplacementCursorPolicy::TextSlot {
            width_px: 8.0,
            stretch_like: false,
        }
    );
    assert_eq!(request.start_position(), DisplayRowPosition::new(24.0, 4));
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("display-property-replacement-plan", 80, 40, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    let mut source_render = text_row_source_render_state(
        &mut builder,
        &mut output_emitter,
        &mut eval,
        &mut font_metrics,
        &face_resolver,
    );
    let snapshot = request
        .string_plan_snapshot(&buffer, &mut source_render, &active_face, &mut face_ids)
        .expect("string replacement lowers to string append request");
    assert_eq!(
        snapshot.origin(),
        DisplayOrigin::DisplayPropertyString {
            anchor_charpos: CharPos0::new(3),
            source: DisplayPropertySource::TextProperty,
        }
    );
    assert_eq!(
        snapshot.base_face_policy(),
        BaseFacePolicy::DisplayPropertyUnderlyingFace
    );
    assert!(snapshot.has_replacement_base_face());
}

#[test]
fn buffer_display_property_replacement_outcome_applies_walk_state_and_cursor() {
    let outcome = BufferDisplayPropertyTextReplacementOutcome {
        replacement: DisplayPropertyReplacementAppendOutcome::new(
            DisplayRowPosition::new(4.0, 1),
            DisplayRowPosition::new(12.0, 2),
            DisplayPropertyReplacementCursorPolicy::FaceChar,
        ),
        skip_to: 4,
    };
    let mut byte_idx = "a".len();
    let mut charpos = 1;
    let mut x = 4.0;
    let mut col = 1;
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let mut cursor_info = CursorCaptureState::new();
    {
        let mut progress =
            DisplaySourceProgressState::new(&mut byte_idx, &mut charpos, &mut x, &mut col);
        outcome.apply_to_progress_and_cursor(
            "a界b\n".as_bytes(),
            &mut progress,
            &mut cursor_info,
            &active_face,
            &geometry,
            2,
            1,
        );
    }

    assert_eq!(byte_idx, "a界b\n".len());
    assert_eq!(charpos, 4);
    assert_eq!(x, 12.0);
    assert_eq!(col, 2);
    assert_eq!(outcome.skip_to(), 4);
    let cursor = cursor_info.captured().expect("captured replacement cursor");
    assert_eq!(cursor.x, 4.0);
    assert_eq!(cursor.byte_idx, "a".len());
    assert_eq!(cursor.col, 1);
    assert_eq!(cursor.slot_width, Some(8.0));
}

#[test]
fn display_property_replacement_resolve_request_appends_and_reports_outcome() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let buffer = current_buffer_snapshot(&eval, buf_id);
    let frame_id = eval.frame_manager_mut().create_frame(
        "display-property-replacement-request",
        320,
        120,
        buf_id,
    );
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let mut font_metrics = None;

    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 32.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let mut geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let value = Value::list(vec![
        Value::symbol("space"),
        Value::keyword("relative-width"),
        Value::fixnum(2),
        Value::keyword("height"),
        Value::fixnum(3),
    ]);
    let classification = classify_display_property(value);
    let params = test_display_space_window_params();

    let descriptor = DisplayPropertyReplacementDescriptor::new(
        value,
        classification,
        BufferDisplayReplacementSource::spanning(
            buf_id,
            CharPos0::new(3),
            EmacsBytePos::new(12),
            CharPos0::new(4),
            EmacsBytePos::new(13),
        ),
        ReplacementCoveredSpan::for_single_property_run(CharPos0::new(3), CharPos0::new(4)),
    );
    let request = DisplayPropertyReplacementRowRenderRequest::from_typed_replacement_descriptor(
        &descriptor,
        b"x",
        &active_face,
        &mut font_metrics,
        24.0,
        8.0,
        &params,
        None,
        -2.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 18.0, 12.0),
        DisplayRowPosition::new(24.0, 4),
    )
    .expect("display replacement row render request");
    let outcome = request.begin_render_to_text_rows(
        &buffer,
        &mut text_row_source_render_state(
            &mut builder,
            &mut output_emitter,
            &mut eval,
            &mut font_metrics,
            &face_resolver,
        ),
        &mut face_ids,
        &surface,
        &mut geometry,
        &active_face,
    );
    let DisplayPropertyReplacementRowRender::Applied(outcome) = outcome else {
        panic!("stretch replacement must complete atomically")
    };

    assert_eq!(outcome.start_position(), DisplayRowPosition::new(24.0, 4));
    assert_eq!(outcome.end_position(), DisplayRowPosition::new(40.0, 6));
    let cursor = outcome.cursor_info(
        &active_face,
        geometry.text_position(
            outcome.start_position().x_px(),
            0,
            outcome.start_position().col(),
        ),
        None,
    );
    assert_eq!(cursor.x, 24.0);
    assert_eq!(cursor.slot_width, Some(16.0));
    let metrics = geometry.row_metrics_snapshot(0);
    assert!(metrics.height() > 16.0);
    assert!(metrics.ascent() > 12.0);
}

#[test]
fn buffer_display_property_replacement_render_outcome_updates_progress() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let buffer = current_buffer_snapshot(&eval, buf_id);
    let frame_id = eval.frame_manager_mut().create_frame(
        "display-property-replacement-apply",
        320,
        120,
        buf_id,
    );
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);

    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let mut font_metrics = None;
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 32.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let mut geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let value = Value::list(vec![
        Value::symbol("space"),
        Value::keyword("relative-width"),
        Value::fixnum(2),
        Value::keyword("height"),
        Value::fixnum(3),
    ]);
    let classification = classify_display_property(value);
    let params = test_display_space_window_params();
    let replacement = BufferDisplayPropertyReplacementItem::new(
        value,
        classification,
        BufferDisplayReplacementSource::spanning(
            buf_id,
            CharPos0::new(3),
            EmacsBytePos::new(12),
            CharPos0::new(4),
            EmacsBytePos::new(13),
        ),
        EmacsBytePos::new(12),
        EmacsBytePos::new(13),
        ReplacementCoveredSpan::for_single_property_run(CharPos0::new(3), CharPos0::new(4)),
    );
    let mut cursor_info = CursorCaptureState::new();
    let mut byte_idx = 0usize;
    let mut charpos = 3i64;
    let mut x = 24.0;
    let mut col = 4usize;

    let outcome = BufferDisplayPropertyTextReplacementRenderRequest::new(
        replacement,
        12,
        b"x",
        8.0,
        &params,
        -2.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 18.0, 12.0),
        &active_face,
    )
    .render(
        &buffer,
        BufferDisplayPropertyTextReplacementRenderState::new(
            text_row_source_render_state(
                &mut builder,
                &mut output_emitter,
                &mut eval,
                &mut font_metrics,
                &face_resolver,
            ),
            &mut face_ids,
            &surface,
            &mut geometry,
            &active_face,
        ),
        x,
        DisplayRowPosition::new(x, col),
    );
    let BufferDisplayPropertyTextReplacementRenderOutcome::Rendered(outcome) = outcome else {
        panic!("expected rendered display replacement");
    };
    {
        let mut progress =
            DisplaySourceProgressState::new(&mut byte_idx, &mut charpos, &mut x, &mut col);
        outcome.apply_to_progress_and_cursor(
            b"x",
            &mut progress,
            &mut cursor_info,
            &active_face,
            &geometry,
            3,
            3,
        );
    }

    assert_eq!(byte_idx, 1);
    assert_eq!(charpos, 4);
    assert_eq!(x, 40.0);
    assert_eq!(col, 6);
    let cursor = cursor_info
        .captured()
        .expect("replacement cursor should be captured");
    assert_eq!(cursor.x, 24.0);
    assert_eq!(cursor.byte_idx, 0);
    assert_eq!(cursor.col, 4);
    assert_eq!(cursor.slot_width, Some(16.0));
}

#[test]
fn display_replacement_stretch_append_item_names_cursor_and_extent_policy() {
    let item = DisplayReplacementStretchSourceItem::from_space_extents(13.0, 16.0, 12.0, 8.0);
    assert_eq!(item.width_px(), 13.0);
    assert_eq!(item.height_px(), 16.0);
    assert_eq!(item.ascent_px(), 12.0);
    assert_eq!(item.cursor_slot_width_px(), 13.0);

    let narrow = DisplayReplacementStretchSourceItem::from_space_extents(3.0, 10.0, 7.0, 8.0);
    assert_eq!(narrow.width_px(), 3.0);
    assert_eq!(narrow.cursor_slot_width_px(), 8.0);

    let clamped = DisplayReplacementStretchSourceItem::from_extents(-1.0, -2.0, -3.0);
    assert_eq!(clamped.width_px(), 0.0);
    assert_eq!(clamped.height_px(), 0.0);
    assert_eq!(clamped.ascent_px(), 0.0);
    assert_eq!(clamped.cursor_slot_width_px(), 0.0);
}

fn test_display_space_window_params() -> WindowParams {
    WindowParams {
        space_image_catalog: None,
        window_id: 1,
        buffer_id: 1,
        bounds: Rect::new(0.0, 0.0, 800.0, 600.0),
        text_bounds: Rect::new(0.0, 0.0, 800.0, 560.0),
        selected: true,
        cursor_role: crate::types::WindowCursorRole::Active,
        mode_line_active: true,
        kind: WindowKind::Main,
        left_col: 0,
        top_line: 0,
        window_start: 1,
        force_start: false,
        previous_visible_end: None,
        point: 1,
        buffer_size: 1,
        buffer_begv: 1,
        display_line_numbers: DisplayLineNumbersMode::Off,
        hscroll: 0,
        vscroll: 0,
        wrap_mode: LineWrapMode::Wrap,
        word_wrap: false,
        tab_width: 8,
        scroll_conservatively: 0,
        scroll_step: 0,
        scroll_minibuffer_conservatively: true,
        scroll_margin: 0,
        tab_stop_list: vec![],
        default_fg: 0xFFFFFF,
        default_bg: 0x000000,
        char_width: 8.0,
        char_height: 16.0,
        window_system: true,
        font_pixel_size: 14.0,
        image_scale_environment: Default::default(),
        font_ascent: 12.0,
        mode_line_height: 0.0,
        header_line_height: 0.0,
        tab_line_height: 0.0,
        cursor_kind: neomacs_display_protocol::frame_glyphs::CursorKind::FilledBox,
        cursor_bar_width: neomacs_display_protocol::frame_glyphs::CursorBarWidth::TWO,
        x_stretch_cursor: false,
        cursor_color: 0xFFFFFF,
        cursor_foreground: 0x000000,
        cursor_effects: None,
        visual_cursors: Vec::new(),
        left_fringe_width: 0.0,
        right_fringe_width: 0.0,
        fringes_outside_margins: false,
        indicate_empty_lines: 0,
        show_trailing_whitespace: false,
        trailing_ws_bg: 0,
        fill_column_indicator: -1,
        fill_column_indicator_char: '|',
        fill_column_indicator_fg: 0,
        extra_line_spacing: 0.0,
        selective_display: 0,
        escape_glyph_fg: 0,
        nobreak_char_display: crate::types::NobreakDisplayMode::Literal,
        nobreak_char_fg: 0,
        glyphless_char_fg: 0,
        wrap_prefix: vec![],
        line_prefix: vec![],
        left_margin_width: 0.0,
        left_margin_columns: 0,
        right_margin_width: 0.0,
        right_margin_columns: 0,
        vertical_scroll_bar_side: None,
        horizontal_scroll_bar: false,
        scroll_bar_pixel_width: 0.0,
        scroll_bar_pixel_height: 0.0,
    }
}

#[test]
fn display_replacement_space_width_policy_names_width_sources() {
    let _eval = Context::new();
    let explicit = Value::list(vec![
        Value::symbol("space"),
        Value::keyword("width"),
        Value::fixnum(4),
    ]);
    let relative = Value::list(vec![
        Value::symbol("space"),
        Value::keyword("relative-width"),
        Value::fixnum(2),
    ]);
    let align_to = Value::list(vec![
        Value::symbol("space"),
        Value::keyword("align-to"),
        Value::fixnum(12),
    ]);
    let default = Value::list(vec![Value::symbol("space")]);

    assert!(matches!(
        DisplaySpaceWidthPolicy::from_items(
            &neovm_core::emacs_core::value::list_to_vec(&explicit).expect("explicit list")
        ),
        DisplaySpaceWidthPolicy::Explicit(_)
    ));
    assert!(matches!(
        DisplaySpaceWidthPolicy::from_items(
            &neovm_core::emacs_core::value::list_to_vec(&relative).expect("relative list")
        ),
        DisplaySpaceWidthPolicy::Relative { factor } if factor == 2.0
    ));
    assert!(matches!(
        DisplaySpaceWidthPolicy::from_items(
            &neovm_core::emacs_core::value::list_to_vec(&align_to).expect("align list")
        ),
        DisplaySpaceWidthPolicy::AlignTo(_)
    ));
    assert!(matches!(
        DisplaySpaceWidthPolicy::from_items(
            &neovm_core::emacs_core::value::list_to_vec(&default).expect("default list")
        ),
        DisplaySpaceWidthPolicy::Default
    ));
}

#[test]
fn display_replacement_space_height_policy_names_height_sources() {
    let _eval = Context::new();
    let explicit = Value::list(vec![
        Value::symbol("space"),
        Value::keyword("height"),
        Value::fixnum(4),
    ]);
    let relative = Value::list(vec![
        Value::symbol("space"),
        Value::keyword("relative-height"),
        Value::fixnum(2),
    ]);
    let default = Value::list(vec![Value::symbol("space")]);

    assert!(matches!(
        DisplaySpaceHeightPolicy::from_items(
            &neovm_core::emacs_core::value::list_to_vec(&explicit).expect("explicit list")
        ),
        DisplaySpaceHeightPolicy::Explicit(_)
    ));
    assert!(matches!(
        DisplaySpaceHeightPolicy::from_items(
            &neovm_core::emacs_core::value::list_to_vec(&relative).expect("relative list")
        ),
        DisplaySpaceHeightPolicy::Relative { factor } if factor == 2.0
    ));
    assert!(matches!(
        DisplaySpaceHeightPolicy::from_items(
            &neovm_core::emacs_core::value::list_to_vec(&default).expect("default list")
        ),
        DisplaySpaceHeightPolicy::Default
    ));
}

#[test]
fn display_replacement_space_ascent_policy_names_ascent_sources() {
    let _eval = Context::new();
    let percent = Value::list(vec![
        Value::symbol("space"),
        Value::keyword("ascent"),
        Value::fixnum(40),
    ]);
    let pixel = Value::list(vec![
        Value::symbol("space"),
        Value::keyword("ascent"),
        Value::fixnum(140),
    ]);
    let default = Value::list(vec![Value::symbol("space")]);

    assert!(matches!(
        DisplaySpaceAscentPolicy::from_items(
            &neovm_core::emacs_core::value::list_to_vec(&percent).expect("percent list")
        ),
        DisplaySpaceAscentPolicy::Percent { percent } if percent == 40.0
    ));
    assert!(matches!(
        DisplaySpaceAscentPolicy::from_items(
            &neovm_core::emacs_core::value::list_to_vec(&pixel).expect("pixel list")
        ),
        DisplaySpaceAscentPolicy::Pixel(_)
    ));
    assert!(matches!(
        DisplaySpaceAscentPolicy::from_items(
            &neovm_core::emacs_core::value::list_to_vec(&default).expect("default list")
        ),
        DisplaySpaceAscentPolicy::Default
    ));
}

#[test]
fn display_replacement_stretch_append_item_resolves_display_space_property() {
    let _eval = Context::new();
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let mut font_metrics = None;
    let spec = Value::list(vec![
        Value::symbol("space"),
        Value::keyword("relative-width"),
        Value::fixnum(2),
        Value::keyword("height"),
        Value::list(vec![Value::fixnum(10)]),
        Value::keyword("ascent"),
        Value::fixnum(40),
    ]);

    let display_char_width = active_face.advance_for_char(&mut font_metrics, 'x', 8.0);
    let item = DisplayReplacementStretchSourceItem::from_display_space_spec(
        &spec,
        0.0,
        0.0,
        8.0,
        display_char_width,
        18.0,
        13.0,
        8.0,
        &test_display_space_window_params(),
    );

    assert_eq!(item.width_px(), 16.0);
    assert_eq!(item.height_px(), 10.0);
    assert_eq!(item.ascent_px(), 4.0);
    assert_eq!(item.cursor_slot_width_px(), 16.0);
}

#[test]
fn display_replacement_media_append_item_names_display_and_cursor_extents() {
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let media = DisplayMediaReplacement::image(DisplayImageItem {
        image_id: 42,
        source_rect: neomacs_display_protocol::ImageSourceRect::FULL,
        width: 64.0,
        height: 18.0,
        ascent: 14.0,
        horizontal_margin: 0.0,
        vertical_margin: 0.0,
        opaque_background: None,
    });

    let ordinary = DisplayReplacementMediaSourceItem::new(
        media,
        active_face.metrics().row_height(),
        active_face.metrics().ascent(),
        false,
    );
    assert_eq!(ordinary.width_px(), 64.0);
    assert_eq!(ordinary.display_height_px(), 18.0);
    assert_eq!(ordinary.display_ascent_px(), 14.0);
    assert_eq!(ordinary.cursor_face_height_px(), 18.0);
    assert_eq!(ordinary.cursor_face_ascent_px(), 14.0);

    let xwidget_media = DisplayMediaReplacement::image(DisplayImageItem {
        image_id: 43,
        source_rect: neomacs_display_protocol::ImageSourceRect::FULL,
        width: 64.0,
        height: 10.0,
        ascent: 10.0,
        horizontal_margin: 0.0,
        vertical_margin: 0.0,
        opaque_background: None,
    });
    let xwidget_cursor = DisplayReplacementMediaSourceItem::new(
        xwidget_media,
        active_face.metrics().row_height(),
        active_face.metrics().ascent(),
        true,
    );
    assert_eq!(xwidget_cursor.cursor_face_height_px(), 18.0);
    assert_eq!(xwidget_cursor.cursor_face_ascent_px(), 13.0);
}

#[test]
fn positive_image_box_width_expands_layout_and_insets_content() {
    let media = DisplayMediaReplacement::image(DisplayImageItem {
        image_id: 42,
        source_rect: neomacs_display_protocol::ImageSourceRect::FULL,
        width: 64.0,
        height: 18.0,
        ascent: 14.0,
        horizontal_margin: 0.0,
        vertical_margin: 0.0,
        opaque_background: None,
    })
    .with_positive_box_line_width(2.0)
    .apply_positive_box_expansion(neomacs_display_protocol::face::BoxVerticalEdges::Both);

    assert_eq!(media.width, 68.0);
    assert_eq!(media.height, 22.0);
    assert_eq!(media.ascent, 16.0);
    assert!(matches!(
        media.kind,
        DisplayMediaReplacementKind::Image {
            margin_left: 2.0,
            margin_right: 2.0,
            margin_top: 2.0,
            margin_bottom: 2.0,
            ..
        }
    ));
}

#[test]
fn positive_image_box_width_respects_slice_and_terminal_boundaries() {
    let media = DisplayMediaReplacement::image(DisplayImageItem {
        image_id: 42,
        source_rect: neomacs_display_protocol::ImageSourceRect::new(0.25, 0.0, 0.75, 0.5)
            .expect("valid slice"),
        width: 48.0,
        height: 9.0,
        ascent: 7.0,
        horizontal_margin: 0.0,
        vertical_margin: 0.0,
        opaque_background: None,
    })
    .with_positive_box_line_width(2.0)
    .apply_positive_box_expansion(neomacs_display_protocol::face::BoxVerticalEdges::Both);

    assert_eq!(media.width, 50.0, "only the touched right side expands");
    assert_eq!(media.height, 11.0, "only the touched top side expands");
    assert_eq!(media.ascent, 9.0);
    assert!(matches!(
        media.kind,
        DisplayMediaReplacementKind::Image {
            margin_left: 0.0,
            margin_right: 2.0,
            margin_top: 2.0,
            margin_bottom: 0.0,
            ..
        }
    ));
}

#[test]
fn display_replacement_media_append_item_resolves_direct_media_property() {
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let media = DisplayMediaReplacement::xwidget(DisplayXwidgetItem {
        xwidget_id: neomacs_display_protocol::XwidgetId::new(17),
        webview_id: neomacs_display_protocol::WebViewId::new(170),
        width: 42.0,
        height: 11.0,
    });
    let replacement = DisplayMediaReplacementProperty::Xwidget(media);

    let resolved = DisplayReplacementMediaSourceItem::resolve_display_property(
        Value::NIL,
        &replacement,
        None,
        &active_face,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        neovm_core::emacs_core::image_catalog::ImageScaleEnvironment::default(),
        None,
    )
    .expect("direct media replacement");

    match resolved {
        DisplayReplacementMediaSourceResolution::Media(item) => {
            assert_eq!(item.width_px(), 42.0);
            assert_eq!(item.display_height_px(), 11.0);
            assert_eq!(item.cursor_face_height_px(), 18.0);
        }
        DisplayReplacementMediaSourceResolution::Placeholder(_) => {
            panic!("expected direct media item")
        }
    }
}

#[test]
fn display_replacement_media_append_item_resolves_placeholder_item_without_host() {
    let active_face = test_active_face_state(FaceId::new(7), 8.0);

    let resolved = DisplayReplacementMediaSourceItem::resolve_display_property(
        Value::NIL,
        &DisplayMediaReplacementProperty::Image,
        None,
        &active_face,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        neovm_core::emacs_core::image_catalog::ImageScaleEnvironment::default(),
        None,
    )
    .expect("image placeholder");

    match resolved {
        DisplayReplacementMediaSourceResolution::Placeholder(item) => {
            assert_eq!(item, DisplayReplacementSourceMappedTextItem::new("[img]"));
        }
        DisplayReplacementMediaSourceResolution::Media(_) => panic!("expected placeholder item"),
    }
}

#[test]
fn display_replacement_media_append_item_names_row_extent_policy() {
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let item = DisplayReplacementMediaSourceItem::new(
        DisplayMediaReplacement::image(DisplayImageItem {
            image_id: 42,
            source_rect: neomacs_display_protocol::ImageSourceRect::FULL,
            width: 64.0,
            height: 10.0,
            ascent: 10.0,
            horizontal_margin: 0.0,
            vertical_margin: 0.0,
            opaque_background: None,
        }),
        active_face.metrics().row_height(),
        active_face.metrics().ascent(),
        false,
    );
    let progress = DisplayRowAppendProgress::from_positions(
        DisplayRowPosition::new(0.0, 0),
        DisplayRowPosition::new(64.0, 8),
        DisplayRowAppendStatus::Complete,
        Vec::new(),
    );

    assert_eq!(item.row_extents_after_append(&progress), Some((10.0, 10.0)));

    let clipped_progress = DisplayRowAppendProgress::from_positions(
        progress.start(),
        progress.end(),
        DisplayRowAppendStatus::Clipped,
        Vec::new(),
    );
    assert_eq!(item.row_extents_after_append(&clipped_progress), None);

    let zero_width_progress = DisplayRowAppendProgress::new(
        progress.start(),
        progress.end(),
        DisplayRowWriteMetrics::new(0.0, progress.metrics().width_cols()),
        DisplayRowAppendStatus::Complete,
        Vec::new(),
    );
    assert_eq!(item.row_extents_after_append(&zero_width_progress), None);
}

#[test]
fn display_replacement_append_context_walks_string_faces_and_measurements() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("append-replacement-string-source", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);

    let table = neovm_core::face::FaceTable::new();
    let face_resolver =
        crate::neovm_bridge::FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base_face = face_resolver.default_face();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    let value = Value::string_with_text_properties(
        "ab",
        vec![StringTextPropertyRun {
            start: 1,
            end: 2,
            plist: Value::list(vec![
                Value::symbol("face"),
                Value::list(vec![Value::keyword("foreground"), Value::string("#ff0000")]),
            ]),
        }],
    );
    let replacement_source = crate::display_item::BufferDisplayReplacementSource::new(
        buf_id,
        CharPos0::new(0),
        EmacsBytePos::new(0),
    );
    let frame = test_append_frame(8.0, 8.0, DisplayTabPolicy::every(8));
    let mut font_metrics = None;
    let mut measurer = SourceMappedTextWidthByFace::new();

    let append_context = DisplayReplacementAppendContext::new(FaceId::new(7), base_face, frame);
    let end = append_context.append_replacement_string_source_to_text_row_and_emit(
        &mut text_row_source_render_state(
            &mut builder,
            &mut output_emitter,
            &mut eval,
            &mut font_metrics,
            &face_resolver,
        ),
        &mut face_ids,
        replacement_source,
        LispStringSourceId::display_replacement(1),
        value,
        DisplayRowPosition::new(0.0, 0),
        &mut measurer,
    );

    assert_eq!(end, DisplayRowPosition::new(24.0, 2));
    assert_eq!(face_ids.next_face_id_for_test(), 21);
    assert_eq!(
        builder
            .output_face(FaceId::new(20))
            .map(|face| face.foreground),
        Some(Color::from_pixel(0x00ff0000))
    );
    builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[1];
            assert_eq!(text.len(), 2);
            assert_eq!(text[0].face_id, FaceId::new(7));
            assert_eq!(text[0].pixel_width, 11.0);
            assert_eq!(text[1].face_id, FaceId::new(20));
            assert_eq!(text[1].pixel_width, 13.0);
        })
        .expect("current row");
}

#[test]
fn display_replacement_append_context_uses_face_fallback() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("append-replacement-item-face", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);
    let table = neovm_core::face::FaceTable::new();
    let face_resolver =
        crate::neovm_bridge::FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base_face = face_resolver.default_face();
    let mut font_metrics = None;

    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    let frame = test_append_frame(8.0, 8.0, DisplayTabPolicy::every(8));
    let replacement_source = crate::display_item::BufferDisplayReplacementSource::new(
        buf_id,
        CharPos0::new(0),
        EmacsBytePos::new(0),
    );
    let append_context = DisplayReplacementAppendContext::new(FaceId::new(7), base_face, frame);

    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(8);
    let progress = append_context
        .append_replacement_item_kind_to_text_row_and_emit(
            &mut text_row_source_render_state(
                &mut builder,
                &mut output_emitter,
                &mut eval,
                &mut font_metrics,
                &face_resolver,
            ),
            &mut face_ids,
            replacement_source,
            DisplayItemKind::Stretch(DisplayStretch {
                width: DisplayStretchWidth::Length(DisplayLength::Pixels(13.0)),
                height: Some(DisplayLength::Pixels(16.0)),
                ascent: Some(DisplayLength::Pixels(12.0)),
            }),
            DisplayRowPosition::new(0.0, 0),
        )
        .expect("append progress");
    let end = progress.end();

    assert_eq!(progress.start(), DisplayRowPosition::new(0.0, 0));
    assert_eq!(end, DisplayRowPosition::new(13.0, 2));
    builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[1];
            assert_eq!(text.len(), 1);
            assert_eq!(text[0].face_id, FaceId::new(7));
            assert_eq!(text[0].pixel_width, 13.0);
            assert!(matches!(
                text[0].glyph_type,
                neomacs_display_protocol::glyph_matrix::GlyphType::Stretch { width_cols: 2 }
            ));
        })
        .expect("current row");
}

#[test]
fn display_replacement_append_context_advances_stretch_output() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("append-replacement-item", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);
    let table = neovm_core::face::FaceTable::new();
    let face_resolver =
        crate::neovm_bridge::FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let mut font_metrics = None;

    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let replacement_source = crate::display_item::BufferDisplayReplacementSource::new(
        buf_id,
        CharPos0::new(0),
        EmacsBytePos::new(0),
    );
    let active_face = test_active_face_state(FaceId::new(3), 8.0);

    let append_context = DisplayReplacementRowAppendContext::new(
        replacement_source,
        &surface,
        &geometry,
        &active_face,
        0.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
    );
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(4);
    let progress = append_context
        .append_stretch_source_item_to_text_row_and_emit(
            &mut text_row_source_render_state(
                &mut builder,
                &mut output_emitter,
                &mut eval,
                &mut font_metrics,
                &face_resolver,
            ),
            &mut face_ids,
            DisplayReplacementStretchSourceItem::from_extents(13.0, 16.0, 12.0),
            DisplayRowPosition::new(0.0, 0),
        )
        .expect("append progress");
    let end = progress.end();

    assert_eq!(end, DisplayRowPosition::new(13.0, 2));
    let display = eval
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.find_window(window_id))
        .and_then(|window| window.display())
        .expect("window display state");
    assert_eq!(
        display.output_cursor,
        Some(neovm_core::window::WindowCursorPos {
            x: 13,
            y: 0,
            row: 0,
            col: 2,
        })
    );
}

#[test]
fn display_replacement_append_context_advances_source_mapped_text_output() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("append-replacement-mapped-text", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);
    let table = neovm_core::face::FaceTable::new();
    let face_resolver =
        crate::neovm_bridge::FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let mut font_metrics = None;

    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 16.0, 12.0);
    let replacement_source = crate::display_item::BufferDisplayReplacementSource::new(
        buf_id,
        CharPos0::new(0),
        EmacsBytePos::new(0),
    );
    let active_face = test_active_face_state(FaceId::new(3), 8.0);

    let append_context = DisplayReplacementRowAppendContext::new(
        replacement_source,
        &surface,
        &geometry,
        &active_face,
        0.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
    );
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(4);
    let progress = append_context
        .append_source_mapped_text_item_to_text_row_and_emit(
            &mut text_row_source_render_state(
                &mut builder,
                &mut output_emitter,
                &mut eval,
                &mut font_metrics,
                &face_resolver,
            ),
            &mut face_ids,
            DisplayReplacementSourceMappedTextItem::new("??"),
            DisplayRowPosition::new(0.0, 0),
        )
        .expect("append progress");
    let end = progress.end();

    assert_eq!(end, DisplayRowPosition::new(16.0, 2));
    builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[1];
            assert_eq!(text.len(), 2);
            assert_eq!(text[0].face_id, FaceId::new(3));
            assert!(matches!(
                text[0].glyph_type,
                neomacs_display_protocol::glyph_matrix::GlyphType::Char { ch: '?' }
            ));
            assert!(matches!(
                text[1].glyph_type,
                neomacs_display_protocol::glyph_matrix::GlyphType::Char { ch: '?' }
            ));
        })
        .expect("current row");
}

#[test]
fn synthetic_text_append_context_uses_source_append_render_request() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("append-display-item", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);
    let table = neovm_core::face::FaceTable::new();
    let face_resolver =
        crate::neovm_bridge::FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base_face = face_resolver.default_face();
    let mut font_metrics = None;

    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 80.0, 80.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let active_face = test_active_face_state(FaceId::new(3), 8.0);
    let geometry = DisplayRowGeometryState::new(0, 0.0, 0.0, 18.0, 13.0);

    let append_context = SyntheticTextRowAppendContext::new(
        &surface,
        &geometry,
        &active_face,
        0.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 10.0, 8.0),
    );
    let progress = append_context
        .append_request_to_text_row_and_emit(
            &mut text_row_source_render_state(
                &mut builder,
                &mut output_emitter,
                &mut eval,
                &mut font_metrics,
                &face_resolver,
            ),
            SyntheticTextAppendRequest::text_row_metrics_source(
                DisplayRowPosition::new(0.0, 0),
                SyntheticTextSource::new(9, "x"),
                FaceId::new(7),
                base_face,
                DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
            ),
        )
        .expect("append progress");
    let end = progress.end();

    assert_eq!(end, DisplayRowPosition::new(8.0, 1));
    builder
        .edit_current_row_for_test(|row| {
            let text = &row.glyphs[1];
            assert_eq!(text.len(), 1);
            assert_eq!(text[0].face_id, FaceId::new(7));
        })
        .expect("current row");
}

#[test]
fn display_replacement_append_context_installs_xwidget_replacements() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("append-xwidget-item", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);
    let table = neovm_core::face::FaceTable::new();
    let face_resolver =
        crate::neovm_bridge::FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let mut font_metrics = None;

    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    let text_bounds = Rect::new(10.0, 20.0, 160.0, 64.0);
    builder.begin_window_with_text_bounds(
        77,
        1,
        24,
        Rect::new(0.0, 0.0, 200.0, 80.0),
        text_bounds,
        true,
    );
    builder.begin_row(0, GlyphRowRole::Text);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(text_bounds.x, 160.0, 160.0, 0.0),
        DisplayTabPolicy::from_tab_width_and_stops(text_bounds.x, 8, &[]),
    );
    let geometry = DisplayRowGeometryState::new(0, 4.0, 0.0, 16.0, 12.0);
    let replacement_source = crate::display_item::BufferDisplayReplacementSource::new(
        buf_id,
        CharPos0::new(0),
        EmacsBytePos::new(0),
    );

    let active_face = test_active_face_state(FaceId::new(3), 8.0);
    let media_item = DisplayReplacementMediaSourceItem::new(
        DisplayMediaReplacement::xwidget(DisplayXwidgetItem {
            xwidget_id: neomacs_display_protocol::XwidgetId::new(1234),
            webview_id: neomacs_display_protocol::WebViewId::new(5678),
            width: 96.0,
            height: 54.0,
        }),
        active_face.metrics().row_height(),
        active_face.metrics().ascent(),
        true,
    );
    let append_context = DisplayReplacementRowAppendContext::new(
        replacement_source,
        &surface,
        &geometry,
        &active_face,
        2.0,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
    );
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(4);
    let progress = append_context
        .append_media_source_item_to_text_row_and_emit(
            &mut text_row_source_render_state(
                &mut builder,
                &mut output_emitter,
                &mut eval,
                &mut font_metrics,
                &face_resolver,
            ),
            &mut face_ids,
            media_item,
            DisplayRowPosition::new(16.0, 2),
        )
        .expect("append progress");
    let end = progress.end();

    assert_eq!(progress.start(), DisplayRowPosition::new(16.0, 2));
    assert_eq!(progress.metrics().width_px(), 96.0);
    assert_eq!(end, DisplayRowPosition::new(112.0, 14));
    builder
        .edit_current_row_for_test(|row| {
            let glyph = &row.glyphs[1][0];
            assert_eq!(glyph.face_id, FaceId::new(3));
            assert_eq!(glyph.pixel_width, 96.0);
            assert_eq!(glyph.pixel_height, 54.0);
            assert_eq!(glyph.pixel_ascent, 54.0);
            assert!(matches!(
                glyph.glyph_type,
                neomacs_display_protocol::glyph_matrix::GlyphType::Xwidget {
                    xwidget_id,
                    width_cols: 12,
                    ..
                } if xwidget_id.get() == 1234
            ));
        })
        .expect("current row");

    builder.end_row();
    builder.end_window();
    let state = builder.finish(24, 1, 8.0, 16.0);
    let frame = state.materialize();
    let xwidget = frame
        .glyphs
        .iter()
        .find_map(|glyph| match glyph {
            neomacs_display_protocol::frame_glyphs::FrameGlyph::Xwidget {
                window_id,
                row_role,
                slot_id,
                xwidget_id,
                presentation,
                ..
            } => Some((*window_id, *row_role, *slot_id, *xwidget_id, *presentation)),
            _ => None,
        })
        .expect("xwidget materialized from its row glyph");
    assert_eq!(xwidget.0.get(), 77);
    assert_eq!(xwidget.1, GlyphRowRole::Text);
    assert_eq!(
        xwidget.2,
        Some(neomacs_display_protocol::frame_glyphs::DisplaySlotId {
            window_id: neomacs_display_protocol::types::DisplayWindowId::new(77),
            row: 0,
            col: 2,
        })
    );
    assert_eq!(xwidget.3.get(), 1234);
    let slot = xwidget.4.layout_slot_rect();
    assert_eq!(
        (slot.x(), slot.y(), slot.width(), slot.height()),
        (16.0, 24.0, 96.0, 54.0)
    );
    let clip = xwidget.4.clip_rect().expect("body-row text-area clip");
    assert_eq!(
        (clip.x(), clip.y(), clip.width(), clip.height()),
        (
            text_bounds.x,
            text_bounds.y,
            text_bounds.width,
            text_bounds.height
        )
    );
}

#[test]
fn display_replacement_append_context_installs_image_replacements() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("append-image-item", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);
    let table = neovm_core::face::FaceTable::new();
    let face_resolver =
        crate::neovm_bridge::FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base_face = face_resolver.default_face();
    let mut font_metrics = None;

    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    let text_bounds = Rect::new(10.0, 20.0, 160.0, 64.0);
    builder.begin_window_with_text_bounds(
        77,
        1,
        24,
        Rect::new(0.0, 0.0, 200.0, 80.0),
        text_bounds,
        true,
    );
    builder.begin_row(0, GlyphRowRole::Text);
    let frame = test_append_frame_at(
        0,
        4.0,
        6.0,
        DisplayRowAppendArea::new(text_bounds.x, 160.0, 160.0, 0.0),
        DisplayRowAppendMetrics::new(
            16.0,
            12.0,
            8.0,
            8.0,
            DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        ),
        DisplayTabPolicy::from_tab_width_and_stops(text_bounds.x, 8, &[]),
    );
    let replacement_source = crate::display_item::BufferDisplayReplacementSource::new(
        buf_id,
        CharPos0::new(0),
        EmacsBytePos::new(0),
    );

    let active_face = test_active_face_state(FaceId::new(3), 8.0);
    let media_item = DisplayReplacementMediaSourceItem::new(
        DisplayMediaReplacement::image(DisplayImageItem {
            image_id: 42,
            source_rect: neomacs_display_protocol::ImageSourceRect::FULL,
            width: 64.0,
            height: 32.0,
            ascent: 32.0,
            horizontal_margin: 0.0,
            vertical_margin: 0.0,
            opaque_background: None,
        }),
        active_face.metrics().row_height(),
        active_face.metrics().ascent(),
        false,
    );
    let append_context = DisplayReplacementAppendContext::new(FaceId::new(3), base_face, frame);
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(4);
    let progress = append_context
        .append_replacement_item_kind_to_text_row_and_emit(
            &mut text_row_source_render_state(
                &mut builder,
                &mut output_emitter,
                &mut eval,
                &mut font_metrics,
                &face_resolver,
            ),
            &mut face_ids,
            replacement_source,
            DisplayItemKind::MediaReplacement(media_item.media()),
            DisplayRowPosition::new(16.0, 2),
        )
        .expect("append progress");
    let end = progress.end();

    assert_eq!(progress.start(), DisplayRowPosition::new(16.0, 2));
    assert_eq!(progress.metrics().width_px(), 64.0);
    assert_eq!(end, DisplayRowPosition::new(80.0, 10));
    builder
        .edit_current_row_for_test(|row| {
            let glyph = &row.glyphs[1][0];
            assert_eq!(glyph.face_id, FaceId::new(3));
            assert_eq!(glyph.pixel_width, 64.0);
            assert_eq!(glyph.pixel_height, 32.0);
            assert_eq!(glyph.pixel_ascent, 32.0);
            assert!(matches!(
                glyph.glyph_type,
                neomacs_display_protocol::glyph_matrix::GlyphType::Image {
                    image_id: 42,
                    width_cols: 8,
                    ..
                }
            ));
        })
        .expect("current row");

    builder.end_row();
    builder.end_window();
    let state = builder.finish(24, 1, 8.0, 16.0);
    let frame = state.materialize();
    let image = frame
        .glyphs
        .iter()
        .find_map(|glyph| match glyph {
            neomacs_display_protocol::frame_glyphs::FrameGlyph::Image {
                window_id,
                row_role,
                clip_rect,
                slot_id,
                image_id,
                x,
                y,
                width,
                height,
                ..
            } => Some((
                *window_id, *row_role, *clip_rect, *slot_id, *image_id, *x, *y, *width, *height,
            )),
            _ => None,
        })
        .expect("image materialized from its row glyph");
    assert_eq!(image.0.get(), 77);
    assert_eq!(image.1, GlyphRowRole::Text);
    assert_eq!(image.2, Some(text_bounds));
    assert_eq!(
        image.3,
        Some(neomacs_display_protocol::frame_glyphs::DisplaySlotId {
            window_id: neomacs_display_protocol::types::DisplayWindowId::new(77),
            row: 0,
            col: 2,
        })
    );
    assert_eq!(image.4.get(), 42);
    assert_eq!(
        (image.5, image.6, image.7, image.8),
        (16.0, 24.0, 64.0, 32.0)
    );
}

#[test]
fn display_replacement_append_context_installs_video_replacements() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("append-video-item", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let mut output_emitter =
        crate::window_output::WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    output_emitter.begin_update(&mut eval);
    output_emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);
    let table = neovm_core::face::FaceTable::new();
    let face_resolver =
        crate::neovm_bridge::FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base_face = face_resolver.default_face();
    let mut font_metrics = None;

    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    let text_bounds = Rect::new(10.0, 20.0, 160.0, 64.0);
    builder.begin_window_with_text_bounds(
        77,
        1,
        24,
        Rect::new(0.0, 0.0, 200.0, 80.0),
        text_bounds,
        true,
    );
    builder.begin_row(0, GlyphRowRole::Text);
    let frame = test_append_frame_at(
        0,
        4.0,
        6.0,
        DisplayRowAppendArea::new(text_bounds.x, 160.0, 160.0, 0.0),
        DisplayRowAppendMetrics::new(
            16.0,
            12.0,
            8.0,
            8.0,
            DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        ),
        DisplayTabPolicy::from_tab_width_and_stops(text_bounds.x, 8, &[]),
    );
    let replacement_source = crate::display_item::BufferDisplayReplacementSource::new(
        buf_id,
        CharPos0::new(0),
        EmacsBytePos::new(0),
    );

    let active_face = test_active_face_state(FaceId::new(3), 8.0);
    let media_item = DisplayReplacementMediaSourceItem::new(
        DisplayMediaReplacement::video(DisplayVideoItem {
            video_id: VideoId::new(88),
            width: 80.0,
            height: 45.0,
            opacity: 0.5,
        }),
        active_face.metrics().row_height(),
        active_face.metrics().ascent(),
        false,
    );
    let append_context = DisplayReplacementAppendContext::new(FaceId::new(3), base_face, frame);
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(4);
    let progress = append_context
        .append_replacement_item_kind_to_text_row_and_emit(
            &mut text_row_source_render_state(
                &mut builder,
                &mut output_emitter,
                &mut eval,
                &mut font_metrics,
                &face_resolver,
            ),
            &mut face_ids,
            replacement_source,
            DisplayItemKind::MediaReplacement(media_item.media()),
            DisplayRowPosition::new(16.0, 2),
        )
        .expect("append progress");
    let end = progress.end();

    assert_eq!(progress.start(), DisplayRowPosition::new(16.0, 2));
    assert_eq!(progress.metrics().width_px(), 80.0);
    assert_eq!(end, DisplayRowPosition::new(96.0, 12));
    builder
        .edit_current_row_for_test(|row| {
            let glyph = &row.glyphs[1][0];
            assert_eq!(glyph.face_id, FaceId::new(3));
            assert_eq!(glyph.pixel_width, 80.0);
            assert_eq!(glyph.pixel_height, 45.0);
            assert_eq!(glyph.pixel_ascent, 45.0);
            assert!(matches!(
                glyph.glyph_type,
                neomacs_display_protocol::glyph_matrix::GlyphType::Video {
                    video_id,
                    width_cols: 10,
                    opacity: 0.5,
                } if video_id == VideoId::new(88)
            ));
        })
        .expect("current row");

    builder.end_row();
    builder.end_window();
    let state = builder.finish(24, 1, 8.0, 16.0);
    let frame = state.materialize();
    let video = frame
        .glyphs
        .iter()
        .find_map(|glyph| match glyph {
            neomacs_display_protocol::frame_glyphs::FrameGlyph::Video {
                window_id,
                row_role,
                clip_rect,
                slot_id,
                video_id,
                x,
                y,
                width,
                height,
                opacity,
                ..
            } => Some((
                *window_id, *row_role, *clip_rect, *slot_id, *video_id, *x, *y, *width, *height,
                *opacity,
            )),
            _ => None,
        })
        .expect("video materialized from its row glyph");
    assert_eq!(video.0.get(), 77);
    assert_eq!(video.1, GlyphRowRole::Text);
    assert_eq!(video.2, Some(text_bounds));
    assert_eq!(
        video.3,
        Some(neomacs_display_protocol::frame_glyphs::DisplaySlotId {
            window_id: neomacs_display_protocol::types::DisplayWindowId::new(77),
            row: 0,
            col: 2,
        })
    );
    assert_eq!(video.4.get(), 88);
    assert_eq!(
        (video.5, video.6, video.7, video.8),
        (16.0, 24.0, 80.0, 45.0)
    );
    assert_eq!(video.9, 0.5);
}

struct DisplayRowSourceStep {
    item: crate::display_item::DisplayItem,
    pending_faces: Vec<crate::display_source_resolver::PendingDisplaySourceFace>,
}

impl DisplayRowSourceStep {
    fn into_parts(
        self,
    ) -> (
        crate::display_item::DisplayItem,
        Vec<crate::display_source_resolver::PendingDisplaySourceFace>,
    ) {
        (self.item, self.pending_faces)
    }
}

struct DisplayRowSourceWalker<S> {
    source: S,
    state: DisplayRowSourceState,
}

impl<S> DisplayRowSourceWalker<S> {
    fn new(source: S) -> Self {
        Self {
            source,
            state: DisplayRowSourceState::frame_local(),
        }
    }
}

impl<S: crate::display_source::DisplayItemSource> DisplayRowSourceWalker<S> {
    fn next_step(
        &mut self,
        face_resolver: &FaceResolver,
        base_face: &crate::neovm_bridge::ResolvedFace,
        base_face_id: FaceId,
        face_ids: &mut FrameFaceAttempt,
        display_host: Option<&dyn DisplayHost>,
        fallback_char_width: f32,
        fallback_ascent: f32,
        fallback_row_height: f32,
    ) -> Option<DisplayRowSourceStep> {
        let face_basis = crate::display_source_resolver::DisplaySourceFaceBasis::new(
            face_resolver,
            base_face_id,
            base_face,
            DisplayRowFallbackMetrics::from_default_face_extents(
                fallback_char_width,
                fallback_row_height,
                fallback_ascent,
            ),
        );
        let resolved = self.state.next_resolved_item(
            &mut self.source,
            crate::display_source_resolver::DisplaySourceResolveParams::new(
                face_basis,
                display_host,
                neovm_core::emacs_core::image_catalog::ImageScaleEnvironment::default(),
            ),
            face_ids,
        );
        let (item, pending_faces) = resolved.into_parts();
        item.map(|item| DisplayRowSourceStep {
            item,
            pending_faces,
        })
    }
}

struct BufferSourceRequestAppendContext<'a, B: crate::neovm_bridge::LayoutBufferView + ?Sized> {
    buffer: &'a B,
    buffer_id: BufferId,
    item_context: crate::display_source_item_append::DisplaySourceItemAppendContext<'a>,
}

impl<'a, B: crate::neovm_bridge::LayoutBufferView + ?Sized>
    BufferSourceRequestAppendContext<'a, B>
{
    fn new(
        buffer: &'a B,
        buffer_id: BufferId,
        face_id: FaceId,
        base_face: &'a crate::neovm_bridge::ResolvedFace,
        frame: DisplayRowAppendFrame,
    ) -> Self {
        Self {
            buffer,
            buffer_id,
            item_context: crate::display_source_item_append::DisplaySourceItemAppendContext::new(
                face_id, base_face, frame,
            ),
        }
    }

    fn append_source_request_to_text_row_and_emit(
        &self,
        state: &mut TextRowSourceRenderState<'_>,
        source_item: DisplaySourceItemRequest,
        position: DisplayRowPosition,
    ) -> Option<DisplayRowAppendProgress> {
        let append_item = buffer_source_item_append_request(
            source_item,
            self.buffer_id,
            self.buffer,
            self.item_context.face_id(),
        )?;
        let kind = append_item.append_kind();
        let item = append_item.into_item();
        self.item_context
            .append_display_item_to_text_row_and_emit(state, item, position, kind)
    }

    fn try_measure_source_request_width_to_text_row(
        &self,
        state: &mut TextRowSourceMeasureState<'_>,
        source_item: DisplaySourceItemRequest,
        position: DisplayRowPosition,
    ) -> Option<f32> {
        let append_item = buffer_source_item_append_request(
            source_item,
            self.buffer_id,
            self.buffer,
            self.item_context.face_id(),
        )?;
        let kind = append_item.append_kind();
        let item = append_item.into_item();
        self.item_context
            .measure_display_item_width_naturally(state, &item, position, kind)
    }

    fn measure_source_request_width_to_text_row(
        &self,
        state: &mut TextRowSourceMeasureState<'_>,
        source_item: DisplaySourceItemRequest,
        position: DisplayRowPosition,
    ) -> f32 {
        let fallback_width = source_item.fallback_width();
        let Some(append_item) = buffer_source_item_append_request(
            source_item.clone(),
            self.buffer_id,
            self.buffer,
            self.item_context.face_id(),
        ) else {
            return fallback_width.resolve_to_text_row(self.item_context.frame());
        };
        let item = append_item.into_item();
        self.item_context
            .measure_source_display_item_width_to_text_row(state, &item, source_item, position)
    }
}

// ---------------------------------------------------------------------------
// Characterization tests: display-property replacements through the LIVE
// buffer render path (`BufferSourceRenderRequest::render_next_and_apply`).
//
// These pin the byte-exact matrix + cursor outcomes for every replacement kind
// (string / stretch / image-placeholder / mapped text) so the
// `TypedReplacementItem` vs `InlineSourceItems` feed unification can only land
// if it is provably behavior-preserving.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
struct DisplayPropertyLiveRenderGlyph {
    glyph_type: GlyphType,
    face_id: FaceId,
}

#[derive(Debug, PartialEq)]
struct DisplayPropertyLiveRenderOutcome {
    continued: Vec<bool>,
    byte_idx: usize,
    charpos: i64,
    x: f32,
    col: usize,
    text_glyphs: Vec<DisplayPropertyLiveRenderGlyph>,
    cursor: Option<(usize, usize, f32, Option<f32>, bool)>,
}

fn display_property_live_render_outcome(
    frame_name: &str,
    text: &str,
    display_value_for: impl FnOnce(&mut Context) -> Value,
    display_byte_range: (usize, usize),
    point_charpos: i64,
) -> DisplayPropertyLiveRenderOutcome {
    let mut context = RowTransitionTestContext::new(frame_name);
    let buf_id = context
        .eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let display_value = display_value_for(&mut context.eval);
    {
        let buffer = context
            .eval
            .buffer_manager_mut()
            .get_mut(buf_id)
            .expect("buffer");
        buffer.insert(text);
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(
                EmacsBytePos::new(display_byte_range.0),
                EmacsBytePos::new(display_byte_range.1),
            ),
            Value::symbol("display"),
            display_value,
        );
    }
    let snapshot = current_buffer_snapshot(&context.eval, buf_id);
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let default_face = face_resolver.default_face().clone();
    let measurement_policy =
        DisplayRowMeasurementPolicy::for_mode(DisplayRowMeasurementMode::LogicalCells);
    let active_face = test_active_face_state(FaceId::new(7), 8.0);
    let surface = DisplayRowAppendSurface::new(
        DisplayRowAppendArea::new(0.0, 800.0, 800.0, 0.0),
        DisplayTabPolicy::every(8),
    );
    let params = test_display_space_window_params();
    let text_bytes = text.as_bytes();
    let total_chars = text.chars().count() as i64;

    let mut byte_idx = 0;
    let mut invisible_text_checkpoint = InvisibleTextScanCheckpoint::new(0);
    let mut charpos = 0;
    let mut col = 0;
    let mut row_extend = DisplayRowScopedValue::inactive();
    let mut box_face = BoxFaceRowState::inactive();
    let mut x = 0.0;
    let mut line_numbers = LineNumberRenderState::new(false, 0, 0);
    let mut hit_row_range = HitRowRangeTracker::new(0);
    let mut prefix_request = DisplayRowPrefixRequest::None;
    let mut hscroll_skip = HorizontalScrollSkipState::new(
        LineWrapMode::Wrap,
        0,
        HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph,
    );
    let mut word_wrap = WordWrapRenderState::new(false);
    let mut trailing_whitespace = TrailingWhitespaceRenderState::new(false, 0);
    let mut face_scan = FaceScanCheckpoint::initial();
    let mut font_metrics = None;
    let mut cursor_info = CursorCaptureState::new();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(7);
    let mut source_walk = BufferSourceWalk::new(buf_id, &snapshot, charpos, 0);

    let mut continued = Vec::new();
    // Drive the live render loop until the buffer walk stops (or all chars
    // consumed). Each iteration mirrors one `render_next_step` call.
    for _ in 0..(total_chars as usize + 4) {
        let overlay_context = BufferOverlayStringTextRowRenderContext::new(
            false,
            1,
            &surface,
            DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
            0.0,
            0,
            4,
        );
        let face_resolution_context = BufferSourceFaceResolutionContext::new(
            &snapshot,
            &face_resolver,
            measurement_policy,
            &default_face,
            BasicFaceId::Default.into(),
            DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
            DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
            neovm_core::emacs_core::image_catalog::ImageScaleEnvironment::default(),
        );
        let loop_context = BufferSourceLoopRequestContext::new(
            buf_id,
            0,
            total_chars,
            point_charpos,
            &params,
            0.0,
            false,
            DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
            DisplayRowVisibilityLimit {
                max_rows: 4,
                bottom_y: 64.0,
            },
            context.defaults,
            0,
            4,
            context.row_limit,
            Color::from_pixel(0x00FFFFFF),
        );

        let cont = BufferSourceRenderRequest::new(
            loop_context,
            text_bytes,
            &params,
            &active_face,
            BufferSourceLoopMutableState::new(
                &mut invisible_text_checkpoint,
                DisplaySourceProgressState::new(&mut byte_idx, &mut charpos, &mut x, &mut col),
                text_row_source_render_state(
                    &mut context.builder,
                    &mut context.output_emitter,
                    &mut context.eval,
                    &mut font_metrics,
                    &face_resolver,
                ),
                BufferSourceRowBuildState::new(
                    &mut context.geometry,
                    &mut context.row_flags,
                    &mut row_extend,
                    &mut box_face,
                ),
                BufferSourceHitCaptureState::new(&mut context.hit_rows, &mut hit_row_range),
                BufferSourceRowCarryoverState::new(
                    &mut prefix_request,
                    &mut line_numbers,
                    &mut hscroll_skip,
                    &mut word_wrap,
                    &mut trailing_whitespace,
                ),
                &mut face_scan,
                &mut context.row_y_positions,
                &mut cursor_info,
                &mut face_ids,
                BufferSourceSurfaceContext::new(&surface, overlay_context),
            ),
        )
        .render_next_and_apply(&mut source_walk, face_resolution_context, &snapshot);

        continued.push(cont);
        if !cont || charpos >= total_chars {
            break;
        }
    }

    let mut text_glyphs = Vec::new();
    context
        .builder
        .edit_current_row_for_test(|row| {
            for glyph in &row.glyphs[GlyphArea::Text as usize] {
                text_glyphs.push(DisplayPropertyLiveRenderGlyph {
                    glyph_type: glyph.glyph_type.clone(),
                    face_id: glyph.face_id,
                });
            }
        })
        .expect("current row");

    let cursor = cursor_info.as_ref().map(|cursor| {
        (
            cursor.byte_idx,
            cursor.col,
            cursor.x,
            cursor.slot_width,
            cursor.stretch_like,
        )
    });

    DisplayPropertyLiveRenderOutcome {
        continued,
        byte_idx,
        charpos,
        x,
        col,
        text_glyphs,
        cursor,
    }
}

#[test]
fn live_buffer_display_property_string_replacement_matrix_and_cursor() {
    // "axb": charpos 1 ('x') carries display = "YZ"; point inside replacement.
    let outcome = display_property_live_render_outcome(
        "live-display-prop-string",
        "axb",
        |_| Value::string("YZ"),
        (1, 2),
        1,
    );

    // Glyphs: 'a', replacement 'Y', 'Z', then 'b'.
    let glyph_chars: Vec<GlyphType> = outcome
        .text_glyphs
        .iter()
        .map(|glyph| glyph.glyph_type.clone())
        .collect();
    assert_eq!(
        glyph_chars,
        vec![
            GlyphType::Char { ch: 'a' },
            GlyphType::Char { ch: 'Y' },
            GlyphType::Char { ch: 'Z' },
            GlyphType::Char { ch: 'b' },
        ],
        "string replacement live matrix",
    );
    assert_eq!(outcome.charpos, 3);
    assert_eq!(outcome.byte_idx, 3);
    assert!(
        outcome.cursor.is_some(),
        "cursor captured inside string replacement"
    );
    insta_like_snapshot_string_replacement(&outcome);
}

fn insta_like_snapshot_string_replacement(outcome: &DisplayPropertyLiveRenderOutcome) {
    // Pin the exact cursor capture and face ids so the feed unification cannot
    // silently move them.
    assert_eq!(
        outcome.cursor,
        Some((1, 1, 8.0, Some(8.0), false)),
        "string replacement cursor capture"
    );
    let faces: Vec<FaceId> = outcome.text_glyphs.iter().map(|g| g.face_id).collect();
    assert_eq!(faces.len(), 4, "string replacement glyph count");
}

#[test]
fn live_buffer_display_property_stretch_replacement_matrix_and_cursor() {
    // "axb": charpos 1 ('x') carries display = (space :width 2).
    let outcome = display_property_live_render_outcome(
        "live-display-prop-stretch",
        "axb",
        |_| {
            Value::list(vec![
                Value::symbol("space"),
                Value::keyword(":width"),
                Value::fixnum(2),
            ])
        },
        (1, 2),
        1,
    );

    let glyph_types: Vec<GlyphType> = outcome
        .text_glyphs
        .iter()
        .map(|glyph| glyph.glyph_type.clone())
        .collect();
    assert_eq!(
        glyph_types,
        vec![
            GlyphType::Char { ch: 'a' },
            GlyphType::Stretch { width_cols: 2 },
            GlyphType::Char { ch: 'b' },
        ],
        "stretch replacement live matrix",
    );
    assert_eq!(outcome.charpos, 3);
    assert_eq!(outcome.byte_idx, 3);
    assert_eq!(
        outcome.cursor,
        Some((1, 1, 8.0, Some(16.0), true)),
        "stretch replacement cursor capture (x-stretch policy)",
    );
}

#[test]
fn live_buffer_display_property_image_placeholder_replacement_matrix() {
    // "axb": charpos 1 ('x') carries an unresolvable image spec. Without a
    // display host the media replacement resolves to a placeholder mapped-text.
    let outcome = display_property_live_render_outcome(
        "live-display-prop-image",
        "axb",
        |_| {
            Value::list(vec![
                Value::symbol("image"),
                Value::keyword(":type"),
                Value::symbol("png"),
                Value::keyword(":file"),
                Value::string("/nonexistent.png"),
            ])
        },
        (1, 2),
        1,
    );

    // The leading 'a' and trailing 'b' always render; the middle is whatever the
    // image spec resolves to. Pin the full matrix so a feed switch cannot alter
    // it.
    let glyph_types: Vec<GlyphType> = outcome
        .text_glyphs
        .iter()
        .map(|glyph| glyph.glyph_type.clone())
        .collect();
    assert_eq!(
        glyph_types.first().cloned(),
        Some(GlyphType::Char { ch: 'a' }),
        "image replacement leading glyph",
    );
    assert_eq!(
        glyph_types.last().cloned(),
        Some(GlyphType::Char { ch: 'b' }),
        "image replacement trailing glyph",
    );
    assert_eq!(outcome.charpos, 3);
    assert_eq!(outcome.byte_idx, 3);
}

#[test]
fn live_buffer_display_property_mapped_text_replacement_matrix() {
    // A `display` property whose value is a vector of strings maps the covered
    // text onto the concatenated replacement text. "axb" with charpos 1 mapped
    // to "MN".
    let outcome = display_property_live_render_outcome(
        "live-display-prop-mapped-text",
        "axb",
        |_| {
            Value::string_with_text_properties(
                "MN",
                vec![StringTextPropertyRun {
                    start: 0,
                    end: 2,
                    plist: Value::NIL,
                }],
            )
        },
        (1, 2),
        1,
    );

    let glyph_types: Vec<GlyphType> = outcome
        .text_glyphs
        .iter()
        .map(|glyph| glyph.glyph_type.clone())
        .collect();
    assert_eq!(
        glyph_types,
        vec![
            GlyphType::Char { ch: 'a' },
            GlyphType::Char { ch: 'M' },
            GlyphType::Char { ch: 'N' },
            GlyphType::Char { ch: 'b' },
        ],
        "mapped-text replacement live matrix",
    );
    assert_eq!(outcome.charpos, 3);
    assert_eq!(outcome.byte_idx, 3);
}
