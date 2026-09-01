use super::*;
use crate::display_current_row_output::DisplayRowCurrentRowOutput;
use crate::display_item::{DisplayItem, DisplayItemKind, DisplayMediaReplacement, SourceSpan};
use crate::display_rendered_row_output_install::{
    frame_chrome_display_row, install_measured_window_display_row,
};
use crate::display_row::builder::{DisplayGlyphMeasurer, DisplayRowItemMeasurement};
use crate::display_row::face_state::DisplayRowMeasurementMode;
use crate::display_row::source_render::TextRowSourceMeasureState;
use crate::display_text_output_install::install_display_row;
use crate::font::metrics::FontMetrics;
use crate::glyph_advance::GlyphAdvanceQuantization;
use crate::neovm_bridge::{FaceResolver, LayoutBufferSnapshot, LayoutBufferView};
use neomacs_display_protocol::Rect;
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
use neomacs_display_protocol::glyph_matrix::{Glyph, GlyphArea, GlyphRow, GlyphType};
use neomacs_display_protocol::types::Color;
use neomacs_display_protocol::types::FaceId;
use neovm_core::buffer::{CharPos0, EmacsBytePos, EmacsByteRange};
use neovm_core::emacs_core::eval::{
    DisplayHost, GuiFrameHostRequest, ResolvedVideo, ResolvedWebKit, VideoResolveRequest,
    WebKitResolveRequest,
};
use neovm_core::emacs_core::image_catalog::{
    ImageCatalog, ImageLookup, ImageResolveRequest, PendingImage, ReadyImage, ResolvedImageMetadata,
};
use neovm_core::emacs_core::value::StringTextPropertyRun;
use neovm_core::emacs_core::{Context, Value};
use neovm_core::face::FaceTable;
use std::sync::Mutex;

fn test_image_load(id: u32) -> neomacs_display_protocol::ImageLoadToken {
    neomacs_display_protocol::ImageLoadToken::new(
        neomacs_display_protocol::ImageId::new(id),
        neomacs_display_protocol::ImageLoadAttempt::new(1).expect("nonzero test attempt"),
    )
}

fn base_face() -> crate::neovm_bridge::ResolvedFace {
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    resolver.default_face().clone()
}

#[test]
fn realize_face_realizes_inline_stipple_spec() {
    // `(8 2 "AB")` — a GNU-style `(WIDTH HEIGHT DATA)` inline bitmap, exactly
    // what `indent-bars` emits. `realize_face` must turn it into the XBM
    // `StipplePattern` the renderer tiles, with the bytes preserved verbatim.
    // A live `Context` sets up the thread-local value heap the `Value`s need.
    let _ctx = Context::new();
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let mut face = neovm_core::face::Face::new("stip");
    face.stipple = Some(Value::list(vec![
        Value::fixnum(8),
        Value::fixnum(2),
        Value::string("AB"),
    ]));
    let pat = resolver
        .realize_face(&face)
        .stipple
        .expect("stipple realized to a pattern");
    assert_eq!(pat.width, 8);
    assert_eq!(pat.height, 2);
    assert_eq!(pat.bits, b"AB".to_vec());

    // A face without `:stipple` realizes to no pattern.
    let plain = neovm_core::face::Face::new("plain");
    assert!(resolver.realize_face(&plain).stipple.is_none());
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

fn display_row_request_from_base_face<'a>(
    geometry: DisplayRowGeometry,
    face_ids: &mut FrameFaceAttempt,
    base_face: &'a crate::neovm_bridge::ResolvedFace,
    role: GlyphRowRole,
    symbol_values: std::collections::HashMap<String, Value>,
) -> DisplayRowSourceRenderRequest<'a> {
    DisplayRowSourceRenderRequest::from_display_row_geometry(
        geometry,
        face_ids,
        base_face,
        role,
        symbol_values,
    )
}

fn display_row_request_for_face<'a>(
    geometry: DisplayRowGeometry,
    base_face_id: FaceId,
    base_face: &'a crate::neovm_bridge::ResolvedFace,
    role: GlyphRowRole,
) -> DisplayRowSourceRenderRequest<'a> {
    DisplayRowSourceRenderRequest::from_display_row_geometry_for_base_face_id(
        geometry,
        base_face_id,
        base_face,
        role,
    )
}

fn render_lisp_string_row_with_context(
    renderer: &mut DisplayRowRenderer<'_>,
    request: DisplayRowSourceRenderRequest<'_>,
    value: Value,
    context: &mut DisplayRowRenderContext<'_, '_>,
) -> Option<RenderedDisplayRow> {
    let request = DisplayRowLispStringSourceRenderRequest::from_value(
        request,
        value,
        crate::display_source_resolver::DisplaySourceFaceScope::FrameLocal,
    );
    let (plan, session_request) = request.into_render_parts();
    renderer.render_lisp_string_plan_with_context(plan, session_request, context)
}

fn render_lisp_string_row(
    renderer: &mut DisplayRowRenderer<'_>,
    request: DisplayRowSourceRenderRequest<'_>,
    value: Value,
    face_resolver: &FaceResolver,
    face_ids: &mut FrameFaceAttempt,
) -> Option<RenderedDisplayRow> {
    let mut context = DisplayRowRenderContext::new(face_resolver, None, face_ids);
    render_lisp_string_row_with_context(renderer, request, value, &mut context)
}

fn render_lisp_string_row_with_display_host(
    renderer: &mut DisplayRowRenderer<'_>,
    request: DisplayRowSourceRenderRequest<'_>,
    value: Value,
    face_resolver: &FaceResolver,
    display_host: Option<&dyn DisplayHost>,
    face_ids: &mut FrameFaceAttempt,
) -> Option<RenderedDisplayRow> {
    let mut context = DisplayRowRenderContext::new(face_resolver, display_host, face_ids);
    render_lisp_string_row_with_context(renderer, request, value, &mut context)
}

struct RecordingDisplayRowMediaHost {
    image_requests: Mutex<Vec<ImageResolveRequest>>,
    video_requests: Mutex<Vec<VideoResolveRequest>>,
    webkit_requests: Mutex<Vec<WebKitResolveRequest>>,
    image_width: u32,
    image_height: u32,
    image_metadata: Option<ResolvedImageMetadata>,
}

impl Default for RecordingDisplayRowMediaHost {
    fn default() -> Self {
        Self {
            image_requests: Mutex::default(),
            video_requests: Mutex::default(),
            webkit_requests: Mutex::default(),
            image_width: 64,
            image_height: 32,
            image_metadata: None,
        }
    }
}

impl RecordingDisplayRowMediaHost {
    fn with_image_size(width: u32, height: u32) -> Self {
        Self {
            image_width: width,
            image_height: height,
            ..Self::default()
        }
    }
}

impl DisplayHost for RecordingDisplayRowMediaHost {
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
        panic!("display row rendering must not use synchronous image resolution");
    }

    fn image_catalog(&self) -> Option<&dyn ImageCatalog> {
        Some(self)
    }

    fn request_video(&self, request: VideoResolveRequest) -> Result<Option<ResolvedVideo>, String> {
        self.video_requests
            .lock()
            .expect("video requests lock")
            .push(request);
        Ok(Some(ResolvedVideo { video_id: 84 }))
    }

    fn request_webkit(
        &self,
        request: WebKitResolveRequest,
    ) -> Result<Option<ResolvedWebKit>, String> {
        self.webkit_requests
            .lock()
            .expect("webkit requests lock")
            .push(request);
        Ok(Some(ResolvedWebKit {
            webview_id: neomacs_display_protocol::WebViewId::new(99),
        }))
    }
}

impl ImageCatalog for RecordingDisplayRowMediaHost {
    fn lookup(&self, request: ImageResolveRequest) -> ImageLookup {
        self.image_requests
            .lock()
            .expect("image requests lock")
            .push(request);
        match &self.image_metadata {
            Some(metadata) => ImageLookup::Ready(ReadyImage {
                load: test_image_load(42),
                metadata: metadata.clone(),
            }),
            None => ImageLookup::Pending(PendingImage::new(
                test_image_load(42),
                neomacs_display_protocol::ImageLayoutExtent::new(
                    self.image_width,
                    self.image_height,
                ),
            )),
        }
    }
}

#[test]
fn display_row_face_realizer_realizes_face_without_layout_engine() {
    let mut font_metrics = None;
    let mut realizer = DisplayRowFaceRealizer::new(&mut font_metrics);
    let mut face = base_face();
    face.set_measured_char_width_px(0.0);
    face.font_ascent = 0.0;
    face.font_line_height = 0.0;

    let rendered = realizer.realize_face(FaceId::new(7), &face, 8.0, 12.0, 16.0);

    assert_eq!(rendered.face_id, FaceId::new(7));
    assert_eq!(rendered.metrics.char_width_px(8.0), 8.0);
    assert_eq!(rendered.metrics.ascent_px(), 12.0);
    assert_eq!(rendered.metrics.descent_px(), 4);
}

#[test]
fn display_row_render_item_preserves_media_as_one_row_item() {
    let media = DisplayMediaReplacement::xwidget(crate::display_item::DisplayXwidgetItem {
        xwidget_id: neomacs_display_protocol::XwidgetId::new(17),
        webview_id: neomacs_display_protocol::WebViewId::new(170),
        width: 42.0,
        height: 11.0,
    });
    let source = DisplayItem::new(
        SourceSpan::synthetic(1, 0, 1),
        RenderFaceRef::FaceId(FaceId::new(7)),
        DisplayItemKind::MediaReplacement(media),
    );
    let render_item = DisplayRowRenderItem::from_source_item(source.clone());

    assert_eq!(render_item.source_item(), &source);
    assert_eq!(
        render_item.row_face(),
        RenderFaceRef::FaceId(FaceId::new(7))
    );
    let DisplayItemKind::MediaReplacement(rendered) = &render_item.row_item().kind else {
        panic!("media replacement should remain a typed row item");
    };
    assert_eq!(*rendered, media);
}

#[test]
fn current_display_row_cluster_tail_reports_live_text_row_tail() {
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 5, Rect::new(0.0, 0.0, 40.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let mut eval = Context::new();
    let mut font_metrics = None;

    assert_eq!(
        text_row_source_measure_state(&mut builder, &mut eval, &mut font_metrics, &resolver)
            .current_cluster_tail(),
        None
    );

    builder
        .edit_current_row_for_test(|row| {
            crate::glyph_row_writer::push_wide_char_to_row(
                row,
                '\u{1F1EF}',
                FaceId::new(3),
                100,
                0.0,
            );
        })
        .expect("current row");
    assert_eq!(
        text_row_source_measure_state(&mut builder, &mut eval, &mut font_metrics, &resolver)
            .current_cluster_tail(),
        Some(('\u{1F1EF}', true))
    );

    builder
        .edit_current_row_for_test(|row| {
            crate::glyph_row_writer::push_cluster_continuation_to_row(
                row,
                '\u{1F1F5}',
                FaceId::new(3),
                101,
            );
        })
        .expect("current row");
    assert_eq!(
        text_row_source_measure_state(&mut builder, &mut eval, &mut font_metrics, &resolver)
            .current_cluster_tail(),
        Some(('\u{1F1F5}', false))
    );
}

#[test]
fn insert_resolved_display_row_face_applies_metric_overrides() {
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    let face = base_face();

    builder.install_output_resolved_display_row_face(
        FaceId::new(9),
        &face,
        Some(FontMetrics {
            ascent: 10.0,
            descent: 3.0,
            line_height: 13.0,
            char_width: 7.0,
            space_width: 7.0,
        }),
    );

    let rendered = builder.output_face(FaceId::new(9)).expect("inserted face");
    assert_eq!(rendered.id, FaceId::new(9));
    assert_eq!(rendered.font_ascent, 10);
    assert_eq!(rendered.font_descent, 3);
}

#[test]
fn insert_resolved_display_row_face_preserves_descent_line_underline_position() {
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    let mut face = base_face();
    face.underline_position = neovm_core::face::UnderlinePosition::DescentLine { pixels_above: 2 };

    builder.install_output_resolved_display_row_face(FaceId::new(9), &face, None);

    let rendered = builder.output_face(FaceId::new(9)).expect("inserted face");
    assert_eq!(
        rendered.underline_placement,
        neomacs_display_protocol::face::UnderlinePosition::DescentLine { pixels_above: 2 }
    );
}

#[test]
fn display_row_source_geometry_allocates_dynamic_base_face_id_through_allocator() {
    let mut face = base_face();
    face.face_id = 0;
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(42);

    let request = display_row_request_from_base_face(
        DisplayRowGeometry {
            y: 0.0,
            width: 80.0,
            height: 16.0,
            char_width: 8.0,
            ascent: 12.0,
            tab_policy: DisplayTabPolicy::every(8),
        },
        &mut face_ids,
        &face,
        GlyphRowRole::Text,
        std::collections::HashMap::new(),
    );

    assert_eq!(request.base_face_id(), FaceId::new(42));
    assert_eq!(face_ids.next_face_id_for_test(), 43);
}

#[test]
fn display_row_source_geometry_builds_whole_row_request() {
    let face = base_face();
    let geometry = DisplayRowGeometry {
        y: 4.0,
        width: 96.0,
        height: 18.0,
        char_width: 9.0,
        ascent: 13.0,
        tab_policy: DisplayTabPolicy::every(4),
    };

    let request = display_row_request_for_face(
        geometry.clone(),
        FaceId::new(17),
        &face,
        GlyphRowRole::Minibuffer,
    );

    assert_eq!(request.geometry(), &geometry);
    assert_eq!(
        request.render_bounds(),
        DisplayRowRenderBounds::whole_row(96.0)
    );
    assert_eq!(request.base_face_id(), FaceId::new(17));
    assert!(std::ptr::eq(request.base_face(), &face));
    assert_eq!(request.role(), GlyphRowRole::Minibuffer);
    assert!(request.symbol_values().is_empty());
}

#[test]
fn display_row_source_geometry_allocates_base_face_id() {
    let mut face = base_face();
    face.face_id = 0;
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(24);
    let geometry = DisplayRowGeometry {
        y: 5.0,
        width: 120.0,
        height: 20.0,
        char_width: 10.0,
        ascent: 14.0,
        tab_policy: DisplayTabPolicy::every(8),
    };
    let mut symbol_values = std::collections::HashMap::new();
    symbol_values.insert("header-line-indent-width".to_string(), Value::fixnum(3));

    let request = display_row_request_from_base_face(
        geometry.clone(),
        &mut face_ids,
        &face,
        GlyphRowRole::HeaderLine,
        symbol_values.clone(),
    );

    assert_eq!(request.geometry(), &geometry);
    assert_eq!(
        request.render_bounds(),
        DisplayRowRenderBounds::whole_row(120.0)
    );
    assert_eq!(request.base_face_id(), FaceId::new(24));
    assert_eq!(face_ids.next_face_id_for_test(), 25);
    assert_eq!(request.role(), GlyphRowRole::HeaderLine);
    assert_eq!(request.symbol_values(), &symbol_values);
}

#[test]
fn display_row_source_request_policy_builds_chrome_request() {
    let mut face = base_face();
    face.face_id = 0;
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(31);
    let mut symbol_values = std::collections::HashMap::new();
    symbol_values.insert("tab-bar-tab-hscroll".to_string(), Value::fixnum(2));

    let request = DisplayRowSourceRenderRequest::from_origin(
        6.0,
        144.0,
        22.0,
        11.0,
        16.0,
        DisplayTabPolicy::every(8),
        crate::display_origin::DisplayOrigin::TabBar,
        &mut face_ids,
        &face,
        symbol_values.clone(),
    );

    assert_eq!(
        request.geometry(),
        &DisplayRowGeometry {
            y: 6.0,
            width: 144.0,
            height: 22.0,
            char_width: 11.0,
            ascent: 16.0,
            tab_policy: DisplayTabPolicy::every(8),
        }
    );
    assert_eq!(
        request.render_bounds(),
        DisplayRowRenderBounds::whole_row(144.0)
    );
    assert_eq!(request.base_face_id(), FaceId::new(31));
    assert_eq!(face_ids.next_face_id_for_test(), 32);
    assert_eq!(request.role(), GlyphRowRole::TabBar);
    assert_eq!(request.symbol_values(), &symbol_values);
}

#[test]
fn display_row_source_geometry_request_overrides_render_bounds() {
    let face = base_face();
    let geometry = DisplayRowGeometry {
        y: 0.0,
        width: 80.0,
        height: 16.0,
        char_width: 8.0,
        ascent: 12.0,
        tab_policy: DisplayTabPolicy::every(8),
    };
    let bounds = DisplayRowRenderBounds::new(
        DisplayRowPosition::new(16.0, 2),
        DisplayRowMaxX::Bounded(40.0),
    );

    let request = display_row_request_for_face(geometry, FaceId::new(7), &face, GlyphRowRole::Text)
        .with_render_bounds(bounds);

    assert_eq!(request.render_bounds(), bounds);
    assert_eq!(request.base_face_id(), FaceId::new(7));
    assert_eq!(request.role(), GlyphRowRole::Text);
}

#[test]
fn display_row_source_fragment_frame_builds_column_bounds_from_glyph_row() {
    let face = base_face();
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.pixel_y = 6.0;
    row.height_px = 18.0;
    row.ascent_px = 13.0;

    let request = DisplayRowSourceFragmentFrame::from_glyph_row_columns(
        &row,
        12,
        7.5,
        GlyphRowRole::Text,
        FaceId::new(9),
        &face,
    )
    .render_request_from_column_for_area(3, 12, GlyphArea::RightMargin);

    assert_eq!(
        request.geometry(),
        &DisplayRowGeometry {
            y: 6.0,
            width: 90.0,
            height: 18.0,
            char_width: 7.5,
            ascent: 13.0,
            tab_policy: DisplayTabPolicy::every(8),
        }
    );
    assert_eq!(
        request.render_bounds(),
        DisplayRowRenderBounds::new(
            DisplayRowPosition::new(22.5, 3),
            DisplayRowMaxX::Bounded(90.0),
        )
    );
    assert_eq!(request.glyph_area(), GlyphArea::RightMargin);
}

#[test]
fn display_row_source_fragment_frame_builds_column_bounds_from_row_geometry() {
    let face = base_face();
    let row_geometry = DisplayRowGeometryState::new(4, 11.0, 24.0, 20.0, 15.0);

    let request = DisplayRowSourceFragmentFrame::from_row_geometry_columns(
        &row_geometry,
        5,
        9.0,
        GlyphRowRole::Text,
        FaceId::new(17),
        &face,
    )
    .render_request_from_column_for_area(0, 5, GlyphArea::LeftMargin);

    assert_eq!(
        request.geometry(),
        &DisplayRowGeometry {
            y: 11.0,
            width: 45.0,
            height: 20.0,
            char_width: 9.0,
            ascent: 15.0,
            tab_policy: DisplayTabPolicy::every(8),
        }
    );
    assert_eq!(
        request.render_bounds(),
        DisplayRowRenderBounds::new(
            DisplayRowPosition::new(0.0, 0),
            DisplayRowMaxX::Bounded(45.0),
        )
    );
    assert_eq!(request.glyph_area(), GlyphArea::LeftMargin);
}

#[test]
fn display_row_render_context_builds_source_resolve_params() {
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let base_face = face_resolver.default_face();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let context = DisplayRowRenderContext::new(&face_resolver, None, &mut face_ids);
    let fallback =
        crate::display_row::metrics::DisplayRowFallbackMetrics::from_default_face_extents(
            8.0, 16.0, 12.0,
        );

    let params = context.source_resolve_params(
        FaceId::new(7),
        base_face,
        fallback,
        neovm_core::emacs_core::image_catalog::ImageScaleEnvironment::default(),
    );

    assert_eq!(params.face_basis().base_face_id(), FaceId::new(7));
    assert_eq!(params.face_basis().fallback_metrics(), fallback);
    assert!(std::ptr::eq(params.face_basis().base_face(), base_face));
    assert!(std::ptr::eq(
        params.face_basis().canonical_face(),
        base_face
    ));
}

#[test]
fn display_row_resolved_measured_face_installs_render_and_measurement_identity() {
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    let mut font_metrics = None;
    let policy = DisplayRowMeasurementPolicy::for_mode(DisplayRowMeasurementMode::ConcreteFont);
    let face = base_face();

    let realized = policy.resolved_measured_face(
        FaceId::new(12),
        face,
        Some(FontMetrics {
            ascent: 11.0,
            descent: 4.0,
            line_height: 15.0,
            char_width: 7.5,
            space_width: 4.0,
        }),
        7.0,
        DisplayRowFallbackMetrics {
            char_width: 7.0,
            row_height: 14.0,
            ascent: 10.0,
        },
        &mut font_metrics,
    );

    builder.install_output_resolved_display_row_face(
        realized.face_id(),
        realized.resolved_face(),
        realized.font_metrics(),
    );

    let rendered = builder
        .output_face(FaceId::new(12))
        .expect("installed face");
    assert_eq!(realized.face_id(), FaceId::new(12));
    assert_eq!(rendered.id, FaceId::new(12));
    assert_eq!(rendered.font_ascent, 11);
    assert_eq!(rendered.font_descent, 4);
}

#[test]
fn display_row_resolved_measured_face_builds_active_face_state_directly() {
    let mut font_metrics = None;
    let policy = DisplayRowMeasurementPolicy::for_mode(DisplayRowMeasurementMode::ConcreteFont);
    let face = base_face();

    let active = policy
        .resolved_measured_face(
            FaceId::new(12),
            face.clone(),
            Some(FontMetrics {
                ascent: 11.0,
                descent: 4.0,
                line_height: 15.0,
                char_width: 7.5,
                space_width: 4.0,
            }),
            7.0,
            DisplayRowFallbackMetrics {
                char_width: 7.0,
                row_height: 14.0,
                ascent: 10.0,
            },
            &mut font_metrics,
        )
        .into_active_face_state();

    assert_eq!(active.face_id(), FaceId::new(12));
    assert_eq!(active.resolved_face().fg, face.fg);
    assert_eq!(active.metrics().row_height(), 15.0);
}

#[test]
fn display_row_active_face_groups_resolved_measurement_metrics_and_colors() {
    let mut font_metrics = None;
    let policy = DisplayRowMeasurementPolicy::for_mode(DisplayRowMeasurementMode::LogicalCells);
    let mut face = base_face();
    face.fg = 0x00112233;
    face.bg = 0x00445566;

    let active = policy
        .resolved_measured_face(
            FaceId::new(14),
            face.clone(),
            None,
            7.0,
            DisplayRowFallbackMetrics {
                char_width: 7.0,
                row_height: 15.0,
                ascent: 10.0,
            },
            &mut font_metrics,
        )
        .into_active_face_state();

    assert_eq!(active.face_id(), FaceId::new(14));
    assert_eq!(active.metrics().char_width(), 7.0);
    assert_eq!(active.metrics().row_height(), 15.0);
    assert_eq!(active.metrics().ascent(), 10.0);
    assert_eq!(active.metrics().space_width(), 7.0);
    assert_eq!(active.background(), Color::from_pixel(face.bg));
    assert_eq!(active.resolved_face().fg, face.fg);
}

#[test]
fn display_row_active_face_state_exposes_render_and_measurement_accessors() {
    let mut font_metrics = None;
    let policy = DisplayRowMeasurementPolicy::for_mode(DisplayRowMeasurementMode::LogicalCells);
    let mut face = base_face();
    face.fg = 0x00112233;
    face.bg = 0x00445566;

    let active = policy
        .resolved_measured_face(
            FaceId::new(14),
            face.clone(),
            None,
            7.0,
            DisplayRowFallbackMetrics {
                char_width: 7.0,
                row_height: 15.0,
                ascent: 10.0,
            },
            &mut font_metrics,
        )
        .into_active_face_state();

    assert_eq!(active.face_id(), FaceId::new(14));
    assert_eq!(active.background(), Color::from_pixel(face.bg));
    assert_eq!(active.resolved_face().fg, face.fg);
    assert_eq!(active.metrics().char_width(), 7.0);
}

#[test]
fn display_row_active_face_state_constructs_from_resolved_and_measured_face() {
    let mut font_metrics = None;
    let policy = DisplayRowMeasurementPolicy::for_mode(DisplayRowMeasurementMode::LogicalCells);
    let mut face = base_face();
    face.fg = 0x00112233;
    face.bg = 0x00445566;
    let measured = policy.measured_face(
        FaceId::new(14),
        &face,
        None,
        7.0,
        DisplayRowFallbackMetrics {
            char_width: 7.0,
            row_height: 15.0,
            ascent: 10.0,
        },
        &mut font_metrics,
    );

    let active = DisplayRowActiveFaceState::new(face.clone(), measured);

    assert_eq!(active.face_id(), FaceId::new(14));
    assert_eq!(active.background(), Color::from_pixel(face.bg));
    assert_eq!(active.resolved_face().fg, face.fg);
    assert_eq!(active.metrics().char_width(), 7.0);
}

#[test]
fn display_row_renderer_renders_lisp_string_without_layout_engine() {
    let _eval = Context::new();
    let mut font_metrics = None;
    let mut renderer =
        DisplayRowRenderer::new(&mut font_metrics, DisplayRowMeasurementMode::LogicalCells);
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(1);
    let request = display_row_request_from_base_face(
        DisplayRowGeometry {
            y: 0.0,
            width: 240.0,
            height: 16.0,
            char_width: 8.0,
            ascent: 12.0,
            tab_policy: crate::display_row::builder::DisplayTabPolicy::every(8),
        },
        &mut face_ids,
        resolver.default_face(),
        GlyphRowRole::TabLine,
        std::collections::HashMap::new(),
    );

    let rendered = render_lisp_string_row(
        &mut renderer,
        request,
        Value::string("A中"),
        &resolver,
        &mut face_ids,
    )
    .expect("display source row");

    assert_eq!(row_text_expanding_stretches(rendered.row()), "A中");
    assert_eq!(rendered.row().role, GlyphRowRole::TabLine);
    assert_eq!(rendered.progress().end_col(), 3);
}

/// The HELLO file separates a script name from its greeting with a literal
/// TAB (tab-width 42). On a TTY the rendered column where the TAB stops must
/// match the buffer-level `current-column` model: GNU advances `current_x` by
/// the composed cluster's `char-width` sum, so combining marks contribute 0
/// columns and the TAB after `Arabic (العربيّة)` (string-width 16; the shadda
/// U+0651 is a zero-width combining mark) fills to the tab stop at column 42.
///
/// Regression for the composed/complex-run cell over-count: the TTY render
/// walk gave every complex-run member its own column (including the zero-width
/// shadda absorbed into the Arabic shaping run), so the running column ran
/// past the buffer model and the TAB over-filled, pushing the greeting right.
#[test]
fn tty_complex_run_then_tab_lands_on_buffer_tab_stop() {
    let _eval = Context::new();
    // font_metrics = None mirrors the TTY frame's fallback measurement path.
    let mut font_metrics = None;
    let mut renderer =
        DisplayRowRenderer::new(&mut font_metrics, DisplayRowMeasurementMode::LogicalCells);
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let mut test_base_face = resolver.default_face().clone();
    test_base_face.set_measured_char_width_px(8.0);
    test_base_face.font_ascent = 12.0;
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(1);
    let request = display_row_request_from_base_face(
        DisplayRowGeometry {
            y: 0.0,
            // 160 cols * 8px so nothing clips.
            width: 1280.0,
            height: 16.0,
            char_width: 8.0,
            ascent: 12.0,
            tab_policy: crate::display_row::builder::DisplayTabPolicy::every(42),
        },
        &mut face_ids,
        &test_base_face,
        GlyphRowRole::Text,
        std::collections::HashMap::new(),
    );

    let rendered = render_lisp_string_row(
        &mut renderer,
        request,
        Value::string("Arabic (العربيّة)\tx"),
        &resolver,
        &mut face_ids,
    )
    .expect("display source row");

    // The greeting (here `x`) must land at the tab stop, just past column 42.
    assert_eq!(
        rendered.progress().end_col(),
        43,
        "complex name + TAB must reach the buffer tab stop (col 42 + 1 for `x`); got {}",
        rendered.progress().end_col()
    );
}

#[test]
fn display_row_source_state_reuses_face_cache_across_items() {
    let _eval = Context::new();
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let base_face = face_resolver.default_face();
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
    let mut source = crate::display_source::LispStringSourceCursor::new(
        1,
        value,
        RenderFaceRef::FaceId(FaceId::new(0)),
        crate::display_source::LispStringSourceOrigin::Normal,
    )
    .expect("string source");
    let mut state = DisplayRowSourceState::frame_local();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let (first, second, third) = {
        let mut next_item = || {
            state.next_resolved_item(
                &mut source,
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
                &mut face_ids,
            )
        };
        (next_item(), next_item(), next_item())
    };

    let (first_item, first_pending_faces) = first.into_parts();
    let (second_item, second_pending_faces) = second.into_parts();
    let (third_item, third_pending_faces) = third.into_parts();
    assert_eq!(
        first_item.expect("first source item").face,
        RenderFaceRef::FaceId(FaceId::new(20))
    );
    assert_eq!(first_pending_faces.len(), 1);
    assert_eq!(
        second_item.expect("second source item").face,
        RenderFaceRef::FaceId(FaceId::new(0))
    );
    assert!(second_pending_faces.is_empty());
    assert_eq!(
        third_item.expect("third source item").face,
        RenderFaceRef::FaceId(FaceId::new(20))
    );
    assert!(third_pending_faces.is_empty());
    assert_eq!(face_ids.next_face_id_for_test(), 21);
}

#[test]
fn display_row_renderer_clips_lisp_string_rows_to_geometry_width() {
    let _eval = Context::new();
    let mut font_metrics = None;
    let mut renderer =
        DisplayRowRenderer::new(&mut font_metrics, DisplayRowMeasurementMode::LogicalCells);
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let mut test_base_face = resolver.default_face().clone();
    test_base_face.set_measured_char_width_px(8.0);
    test_base_face.font_ascent = 12.0;
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(1);
    let spec = display_row_request_from_base_face(
        DisplayRowGeometry {
            y: 0.0,
            width: 16.0,
            height: 16.0,
            char_width: 8.0,
            ascent: 12.0,
            tab_policy: crate::display_row::builder::DisplayTabPolicy::every(8),
        },
        &mut face_ids,
        &test_base_face,
        GlyphRowRole::ModeLine,
        std::collections::HashMap::new(),
    );

    let rendered = render_lisp_string_row(
        &mut renderer,
        spec,
        Value::string("ABC"),
        &resolver,
        &mut face_ids,
    )
    .expect("display source row");

    assert_eq!(row_text_expanding_stretches(rendered.row()), "AB");
    assert_eq!(rendered.progress().end_x(), 16.0);
    assert_eq!(rendered.progress().end_col(), 2);
}

#[test]
fn display_row_renderer_clips_from_render_bounds_start() {
    let _eval = Context::new();
    let mut font_metrics = None;
    let mut renderer =
        DisplayRowRenderer::new(&mut font_metrics, DisplayRowMeasurementMode::LogicalCells);
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let mut test_base_face = resolver.default_face().clone();
    test_base_face.set_measured_char_width_px(8.0);
    test_base_face.font_ascent = 12.0;
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(1);
    let request = display_row_request_from_base_face(
        DisplayRowGeometry {
            y: 0.0,
            width: 240.0,
            height: 16.0,
            char_width: 8.0,
            ascent: 12.0,
            tab_policy: crate::display_row::builder::DisplayTabPolicy::every(8),
        },
        &mut face_ids,
        &test_base_face,
        GlyphRowRole::ModeLine,
        std::collections::HashMap::new(),
    )
    .with_render_bounds(DisplayRowRenderBounds::new(
        DisplayRowPosition::new(16.0, 2),
        DisplayRowMaxX::Bounded(32.0),
    ));

    let rendered = render_lisp_string_row(
        &mut renderer,
        request,
        Value::string("ABC"),
        &resolver,
        &mut face_ids,
    )
    .expect("display source row");

    assert_eq!(row_text_expanding_stretches(rendered.row()), "AB");
    assert_eq!(rendered.progress().end_x(), 32.0);
    assert_eq!(rendered.progress().end_col(), 4);
    assert_eq!(rendered.source_slots()[0].x_px(), 16.0);
    assert_eq!(rendered.source_slots()[0].col(), 2);
}

#[test]
fn display_row_renderer_uses_render_bounds_start_for_tab_advance() {
    let _eval = Context::new();
    let mut font_metrics = None;
    let mut renderer =
        DisplayRowRenderer::new(&mut font_metrics, DisplayRowMeasurementMode::LogicalCells);
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let mut test_base_face = resolver.default_face().clone();
    test_base_face.set_measured_char_width_px(8.0);
    test_base_face.font_ascent = 12.0;
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(1);
    let request = display_row_request_from_base_face(
        DisplayRowGeometry {
            y: 0.0,
            width: 240.0,
            height: 16.0,
            char_width: 8.0,
            ascent: 12.0,
            tab_policy: crate::display_row::builder::DisplayTabPolicy::every(4),
        },
        &mut face_ids,
        &test_base_face,
        GlyphRowRole::ModeLine,
        std::collections::HashMap::new(),
    )
    .with_render_bounds(DisplayRowRenderBounds::new(
        DisplayRowPosition::new(16.0, 2),
        DisplayRowMaxX::Bounded(240.0),
    ));

    let rendered = render_lisp_string_row(
        &mut renderer,
        request,
        Value::string("\tX"),
        &resolver,
        &mut face_ids,
    )
    .expect("display source row");

    let glyphs = &rendered.row().glyphs[1];
    assert_eq!(glyphs[0].glyph_type, GlyphType::Stretch { width_cols: 2 });
    assert_eq!(glyphs[0].pixel_width, 16.0);
    assert_eq!(rendered.progress().end_x(), 40.0);
    assert_eq!(rendered.progress().end_col(), 5);
    assert_eq!(rendered.source_slots()[0].x_px(), 16.0);
    assert_eq!(rendered.source_slots()[0].width_px(), 16.0);
}

#[test]
fn display_row_renderer_continues_source_mapped_text_after_clip() {
    struct OnceSource {
        item: Option<crate::display_item::DisplayItem>,
    }

    impl crate::display_source::DisplayItemSource for OnceSource {
        fn next_item(
            &mut self,
            _context: &mut crate::display_source::DisplaySourceContext<'_>,
        ) -> Option<crate::display_item::DisplayItem> {
            self.item.take()
        }
    }

    let mut font_metrics = None;
    let mut renderer =
        DisplayRowRenderer::new(&mut font_metrics, DisplayRowMeasurementMode::LogicalCells);
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let mut test_base_face = resolver.default_face().clone();
    test_base_face.set_measured_char_width_px(8.0);
    test_base_face.font_ascent = 12.0;
    let base_face_id = FaceId::new(1);
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(2);
    let mut source = OnceSource {
        item: Some(crate::display_item::DisplayItem::new(
            crate::display_item::SourceSpan::synthetic(9, 0, 1),
            crate::display_item::RenderFaceRef::FaceId(base_face_id),
            crate::display_item::DisplayItemKind::SourceMappedText(
                crate::display_item::DisplaySourceMappedText::new("ABC"),
            ),
        )),
    };
    let mut state = DisplayRowSourceState::frame_local();
    let mut context = DisplayRowRenderContext::new(&resolver, None, &mut face_ids);

    let first = display_row_request_for_face(
        DisplayRowGeometry {
            y: 0.0,
            width: 16.0,
            height: 16.0,
            char_width: 8.0,
            ascent: 12.0,
            tab_policy: crate::display_row::builder::DisplayTabPolicy::every(8),
        },
        base_face_id,
        &test_base_face,
        GlyphRowRole::Text,
    )
    .render_step_with_context(&mut renderer, &mut source, &mut state, &mut context)
    .expect("first row");
    let second = display_row_request_for_face(
        DisplayRowGeometry {
            y: 16.0,
            width: 16.0,
            height: 16.0,
            char_width: 8.0,
            ascent: 12.0,
            tab_policy: crate::display_row::builder::DisplayTabPolicy::every(8),
        },
        base_face_id,
        &test_base_face,
        GlyphRowRole::Text,
    )
    .render_step_with_context(&mut renderer, &mut source, &mut state, &mut context)
    .expect("second row");

    assert_eq!(row_text_expanding_stretches(first.row()), "AB");
    assert_eq!(row_text_expanding_stretches(second.row()), "C");
}

#[test]
fn display_row_renderer_accepts_direct_text_run_measurement_policy() {
    struct OnceSource {
        item: Option<crate::display_item::DisplayItem>,
    }

    impl crate::display_source::DisplayItemSource for OnceSource {
        fn next_item(
            &mut self,
            _context: &mut crate::display_source::DisplaySourceContext<'_>,
        ) -> Option<crate::display_item::DisplayItem> {
            self.item.take()
        }
    }

    struct DirectTextRunPolicy;

    impl DisplayRowRenderPolicy for DirectTextRunPolicy {
        fn measurement_for(
            &mut self,
            _item: &crate::display_item::DisplayItem,
            _face_id: FaceId,
            _font_metrics: &mut Option<FontMetricsService>,
        ) -> DisplayRowItemMeasurement {
            DisplayRowItemMeasurement::TextRun(
                crate::display_text_run_measurement::DisplayTextRunMeasurementPlan::uniform_for_text(
                    "ABC", 5.0,
                ),
            )
        }
    }

    let mut font_metrics = None;
    let mut renderer =
        DisplayRowRenderer::new(&mut font_metrics, DisplayRowMeasurementMode::LogicalCells);
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let base_face = resolver.default_face();
    let base_face_id = FaceId::new(1);
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(2);
    let mut source = OnceSource {
        item: Some(crate::display_item::DisplayItem::new(
            crate::display_item::SourceSpan::synthetic(10, 0, 3),
            crate::display_item::RenderFaceRef::FaceId(base_face_id),
            crate::display_item::DisplayItemKind::TextRun(
                crate::display_item::DisplayTextRun::new("ABC"),
            ),
        )),
    };
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let mut state = DisplayRowSourceState::frame_local();
    let mut policy = DirectTextRunPolicy;
    let mut context = DisplayRowRenderContext::new(&resolver, None, &mut face_ids);

    let result = display_row_request_for_face(
        DisplayRowGeometry {
            y: 0.0,
            width: 240.0,
            height: 16.0,
            char_width: 8.0,
            ascent: 12.0,
            tab_policy: crate::display_row::builder::DisplayTabPolicy::every(8),
        },
        base_face_id,
        base_face,
        GlyphRowRole::Text,
    )
    .render_fragment_step_into_row_with_policy(
        &mut renderer,
        &mut row,
        &mut source,
        &mut state,
        &mut context,
        &mut policy,
    )
    .expect("rendered row");

    assert_eq!(result.progress().end_x(), 15.0);
    assert_eq!(result.progress().end_col(), 3);
    assert_eq!(row.glyphs[GlyphArea::Text.index()][0].pixel_width, 5.0);
}

fn row_text_expanding_stretches(row: &GlyphRow) -> String {
    row.glyphs[1]
        .iter()
        .filter(|glyph| !glyph.padding)
        .flat_map(|glyph| match &glyph.glyph_type {
            GlyphType::Char { ch } => std::iter::repeat_n(*ch, 1).collect::<Vec<_>>(),
            GlyphType::Composite { text } => text.chars().collect::<Vec<_>>(),
            GlyphType::Stretch { width_cols } => {
                std::iter::repeat_n(' ', usize::from(*width_cols)).collect::<Vec<_>>()
            }
            _ => Vec::new(),
        })
        .collect()
}

fn row_text_glyph_types(row: &GlyphRow) -> Vec<GlyphType> {
    row.glyphs[1]
        .iter()
        .filter(|glyph| !glyph.padding)
        .map(|glyph| glyph.glyph_type.clone())
        .collect()
}

fn row_text_face_ids(row: &GlyphRow) -> Vec<FaceId> {
    row.glyphs[1]
        .iter()
        .filter(|glyph| !glyph.padding)
        .map(|glyph| glyph.face_id)
        .collect()
}

#[test]
fn display_row_geometry_builds_row_layout() {
    let tab_policy =
        crate::display_row::builder::DisplayTabPolicy::from_tab_width_and_stops(8.0, 4, &[6]);
    let geometry = DisplayRowGeometry {
        y: 20.0,
        width: 120.0,
        height: 16.0,
        char_width: 8.0,
        ascent: 11.0,
        tab_policy: tab_policy.clone(),
    };

    let layout = geometry.to_layout(
        GlyphRowRole::Text,
        9.0,
        12.0,
        RenderFaceRef::FaceId(FaceId::new(42)),
        crate::display_pixel_calc::PixelCalcContext::for_chrome_row(
            120.0,
            9.0,
            16.0,
            std::collections::HashMap::new(),
        ),
        None,
    );

    assert_eq!(layout.role, GlyphRowRole::Text);
    assert_eq!(layout.y_px, 20.0);
    assert_eq!(layout.pixel_calc.text_area_width, 120.0);
    assert_eq!(layout.height_px, 16.0);
    assert_eq!(layout.ascent_px, 12.0);
    assert_eq!(layout.char_width_px, 9.0);
    assert_eq!(layout.tab_policy, tab_policy);
    assert_eq!(layout.base_face, RenderFaceRef::FaceId(FaceId::new(42)));
}

fn render_lisp_display_row(rendered: Value, role: GlyphRowRole) -> GlyphRow {
    render_lisp_display_row_with_symbols(rendered, role, std::collections::HashMap::new())
}

fn render_lisp_display_row_with_symbols(
    rendered: Value,
    role: GlyphRowRole,
    symbol_values: std::collections::HashMap<String, Value>,
) -> GlyphRow {
    render_lisp_display_row_output_with_symbols(rendered, role, symbol_values).into_row()
}

fn render_lisp_display_row_output(rendered: Value, role: GlyphRowRole) -> RenderedDisplayRow {
    render_lisp_display_row_output_with_symbols(rendered, role, std::collections::HashMap::new())
}

fn render_lisp_display_row_output_with_symbols(
    rendered: Value,
    role: GlyphRowRole,
    symbol_values: std::collections::HashMap<String, Value>,
) -> RenderedDisplayRow {
    render_lisp_display_row_output_with_symbols_and_chrome_text_area_left(
        rendered,
        role,
        symbol_values,
        0.0,
    )
}

fn render_lisp_display_row_output_with_symbols_and_chrome_text_area_left(
    rendered: Value,
    role: GlyphRowRole,
    symbol_values: std::collections::HashMap<String, Value>,
    chrome_text_area_left_px: f32,
) -> RenderedDisplayRow {
    let mut font_metrics = None;
    let mut renderer =
        DisplayRowRenderer::new(&mut font_metrics, DisplayRowMeasurementMode::LogicalCells);
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(1);
    let request = display_row_request_from_base_face(
        DisplayRowGeometry {
            y: 0.0,
            width: 240.0,
            height: 16.0,
            char_width: 8.0,
            ascent: 12.0,
            tab_policy: crate::display_row::builder::DisplayTabPolicy::every(8),
        },
        &mut face_ids,
        resolver.default_face(),
        role,
        symbol_values,
    )
    .with_chrome_text_area_left_px(chrome_text_area_left_px);
    render_lisp_string_row(&mut renderer, request, rendered, &resolver, &mut face_ids)
        .expect("display source row")
}

fn render_buffer_display_row(text: &str, role: GlyphRowRole) -> GlyphRow {
    render_buffer_display_row_with_properties(text, Vec::new(), role)
}

fn render_buffer_display_row_with_property(
    text: &str,
    property_start: usize,
    property_end: usize,
    property_name: Value,
    property_value: Value,
    role: GlyphRowRole,
) -> GlyphRow {
    render_buffer_display_row_with_properties(
        text,
        vec![(property_start, property_end, property_name, property_value)],
        role,
    )
}

fn render_buffer_display_row_with_properties(
    text: &str,
    properties: Vec<(usize, usize, Value, Value)>,
    role: GlyphRowRole,
) -> GlyphRow {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buffer.insert(text);
        for (property_start, property_end, property_name, property_value) in properties {
            let start = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(property_start));
            let end = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(property_end));
            buffer.text_props_put_property_in_emacs_byte_range(
                EmacsByteRange::new(start, end),
                property_name,
                property_value,
            );
        }
    }

    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let snapshot = LayoutBufferSnapshot::from_buffer(buffer);
    let mut font_metrics = None;
    let mut renderer =
        DisplayRowRenderer::new(&mut font_metrics, DisplayRowMeasurementMode::LogicalCells);
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(1);
    let request = display_row_request_from_base_face(
        DisplayRowGeometry {
            y: 0.0,
            width: 240.0,
            height: 16.0,
            char_width: 8.0,
            ascent: 12.0,
            tab_policy: crate::display_row::builder::DisplayTabPolicy::every(8),
        },
        &mut face_ids,
        resolver.default_face(),
        role,
        std::collections::HashMap::new(),
    );
    let mut source = crate::buffer_source::text_source::BufferTextSourceCursor::new(
        buf_id,
        &snapshot,
        CharPos0::ZERO,
        snapshot.layout_point_max_char_pos(),
        request.base_face_ref(),
    );

    request
        .render(&mut renderer, &mut source, &resolver, &mut face_ids)
        .expect("buffer display source row")
        .into_row()
}

#[test]
fn render_display_item_source_row_accepts_buffer_text_source() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("A中👨‍👩");
    }
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let mut font_metrics = None;
    let mut renderer =
        DisplayRowRenderer::new(&mut font_metrics, DisplayRowMeasurementMode::LogicalCells);
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(1);
    let mut source = crate::buffer_source::text_source::BufferTextSourceCursor::new(
        buf_id,
        buffer,
        neovm_core::buffer::CharPos0::new(0),
        buffer.layout_point_max_char_pos(),
        RenderFaceRef::FaceId(FaceId::new(1)),
    );

    let rendered = display_row_request_for_face(
        DisplayRowGeometry {
            y: 0.0,
            width: 240.0,
            height: 16.0,
            char_width: 8.0,
            ascent: 12.0,
            tab_policy: crate::display_row::builder::DisplayTabPolicy::every(8),
        },
        FaceId::new(1),
        resolver.default_face(),
        GlyphRowRole::TabLine,
    )
    .render(&mut renderer, &mut source, &resolver, &mut face_ids)
    .expect("display source row");

    assert_eq!(rendered.source_slots().len(), 5);
    assert_eq!(
        rendered.source_slots()[0].source(),
        crate::display_item::DisplaySourcePosition::buffer(
            buf_id,
            CharPos0::new(0),
            EmacsBytePos::new(0)
        )
    );
    assert_eq!(
        rendered.source_slots()[1].source(),
        crate::display_item::DisplaySourcePosition::buffer(
            buf_id,
            CharPos0::new(1),
            EmacsBytePos::new(1)
        )
    );
    assert_eq!(rendered.source_slots()[0].width_cols(), 1);
    assert_eq!(rendered.source_slots()[1].width_cols(), 2);

    let row = rendered.into_row();
    let glyphs = &row.glyphs[1];
    let cjk = glyphs
        .iter()
        .find(|glyph| matches!(glyph.glyph_type, GlyphType::Char { ch: '中' }))
        .expect("CJK glyph");

    assert_eq!(row.role, GlyphRowRole::TabLine);
    assert_eq!(row_text_expanding_stretches(&row), "A中👨‍👩");
    assert!(cjk.wide);
    assert!(glyphs.iter().any(|glyph| glyph.padding));
    assert!(
        glyphs.iter().any(
            |glyph| matches!(&glyph.glyph_type, GlyphType::Composite { text } if text.contains('\u{200d}'))
        )
    );
}

#[test]
fn render_lisp_string_row_records_xwidget_media_fragments() {
    let eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let xwidget = Value::make_xwidget(
        Value::symbol("webkit"),
        Value::string("Title"),
        Value::make_buffer(buf_id),
        96,
        54,
        1234,
        neomacs_display_protocol::WebViewId::new(5678),
    );
    let rendered_text = Value::string_with_text_properties(
        "AXB",
        vec![neovm_core::emacs_core::value::StringTextPropertyRun {
            start: 1,
            end: 2,
            plist: Value::list(vec![
                Value::symbol("display"),
                Value::list(vec![
                    Value::symbol("xwidget"),
                    Value::keyword("xwidget"),
                    xwidget,
                ]),
            ]),
        }],
    );
    let mut font_metrics = None;
    let mut renderer =
        DisplayRowRenderer::new(&mut font_metrics, DisplayRowMeasurementMode::LogicalCells);
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let mut base_face = resolver.default_face().clone();
    base_face.set_measured_char_width_px(8.0);
    base_face.font_ascent = 12.0;
    base_face.font_line_height = 16.0;
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(1);
    let spec = display_row_request_from_base_face(
        DisplayRowGeometry {
            y: 4.0,
            width: 240.0,
            height: 16.0,
            char_width: 8.0,
            ascent: 12.0,
            tab_policy: crate::display_row::builder::DisplayTabPolicy::every(8),
        },
        &mut face_ids,
        &base_face,
        GlyphRowRole::TabLine,
        std::collections::HashMap::new(),
    );

    let rendered =
        render_lisp_string_row(&mut renderer, spec, rendered_text, &resolver, &mut face_ids)
            .expect("display source row");

    let glyphs = &rendered.row().glyphs[1];
    assert!(matches!(
        glyphs[1].glyph_type,
        GlyphType::Xwidget {
            xwidget_id,
            webview_id,
            ..
        } if xwidget_id.get() == 1234 && webview_id.get() == 5678
    ));
    assert_eq!(glyphs[1].pixel_width, 96.0);
    assert_eq!(glyphs[1].pixel_height, 54.0);
}

fn render_tab_line_with_media_host(
    rendered_text: Value,
    default_fg: u32,
    default_bg: u32,
) -> (RenderedDisplayRow, RecordingDisplayRowMediaHost) {
    render_tab_line_with_sized_media_host(rendered_text, default_fg, default_bg, 64, 32, 16.0, 12.0)
}

fn render_tab_line_with_sized_media_host(
    rendered_text: Value,
    default_fg: u32,
    default_bg: u32,
    image_width: u32,
    image_height: u32,
    row_height: f32,
    row_ascent: f32,
) -> (RenderedDisplayRow, RecordingDisplayRowMediaHost) {
    render_tab_line_with_sized_media_metadata_host(
        rendered_text,
        default_fg,
        default_bg,
        image_width,
        image_height,
        row_height,
        row_ascent,
        None,
        neovm_core::emacs_core::image_catalog::ImageScaleEnvironment::default(),
    )
}

#[allow(clippy::too_many_arguments)]
fn render_tab_line_with_sized_media_metadata_host(
    rendered_text: Value,
    default_fg: u32,
    default_bg: u32,
    image_width: u32,
    image_height: u32,
    row_height: f32,
    row_ascent: f32,
    image_metadata: Option<ResolvedImageMetadata>,
    image_scale_environment: neovm_core::emacs_core::image_catalog::ImageScaleEnvironment,
) -> (RenderedDisplayRow, RecordingDisplayRowMediaHost) {
    let mut host = RecordingDisplayRowMediaHost::with_image_size(image_width, image_height);
    host.image_metadata = image_metadata;
    let mut font_metrics = None;
    let mut renderer =
        DisplayRowRenderer::new(&mut font_metrics, DisplayRowMeasurementMode::LogicalCells);
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, default_fg, default_bg, 14.0, None);
    let mut base_face = resolver.default_face().clone();
    base_face.set_measured_char_width_px(8.0);
    base_face.font_ascent = row_ascent;
    base_face.font_line_height = row_height;
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(1);
    let spec = display_row_request_from_base_face(
        DisplayRowGeometry {
            y: 4.0,
            width: 240.0,
            height: row_height,
            char_width: 8.0,
            ascent: row_ascent,
            tab_policy: crate::display_row::builder::DisplayTabPolicy::every(8),
        },
        &mut face_ids,
        &base_face,
        GlyphRowRole::TabLine,
        std::collections::HashMap::new(),
    )
    .with_image_scale_environment(image_scale_environment);
    let rendered = render_lisp_string_row_with_display_host(
        &mut renderer,
        spec,
        rendered_text,
        &resolver,
        Some(&host),
        &mut face_ids,
    )
    .expect("display source row");
    (rendered, host)
}

#[test]
fn render_lisp_string_row_centers_image_on_text_centerline() {
    let _eval = Context::new();
    let rendered_text = Value::string_with_text_properties(
        "AXB",
        vec![neovm_core::emacs_core::value::StringTextPropertyRun {
            start: 1,
            end: 2,
            plist: Value::list(vec![
                Value::symbol("display"),
                Value::list(vec![
                    Value::symbol("image"),
                    Value::keyword("type"),
                    Value::symbol("svg"),
                    Value::keyword("file"),
                    Value::string("/tmp/cross_16.svg"),
                    Value::keyword("ascent"),
                    Value::symbol("center"),
                ]),
            ]),
        }],
    );

    let (rendered, _) =
        render_tab_line_with_sized_media_host(rendered_text, 0, 0, 16, 16, 18.0, 14.0);
    let image = &rendered.row().glyphs[GlyphArea::Text.index()][1];

    assert!(matches!(
        image.glyph_type,
        GlyphType::Image { image_id: 42, .. }
    ));
    assert_eq!(image.pixel_height, 16.0);
    assert_eq!(image.pixel_ascent, 13.0);
    assert_eq!(
        rendered.source_slots()[1].source().lisp_string_char_index(),
        Some(1)
    );
    assert_eq!(rendered.source_slots()[1].width_px(), 16.0);
    assert_eq!(rendered.row().pixel_y + rendered.row().ascent_px, 18.0);
}

#[test]
fn render_lisp_string_row_resolves_image_display_property_through_display_host() {
    let _eval = Context::new();
    let rendered_text = Value::string_with_text_properties(
        "AXB",
        vec![neovm_core::emacs_core::value::StringTextPropertyRun {
            start: 1,
            end: 2,
            plist: Value::list(vec![
                Value::symbol("display"),
                Value::list(vec![
                    Value::symbol("image"),
                    Value::keyword("type"),
                    Value::symbol("png"),
                    Value::keyword("file"),
                    Value::string("/tmp/chrome.png"),
                ]),
            ]),
        }],
    );
    let (rendered, host) = render_tab_line_with_media_host(rendered_text, 0x00112233, 0x00445566);

    let image = &rendered.row().glyphs[GlyphArea::Text.index()][1];
    assert!(matches!(
        image.glyph_type,
        GlyphType::Image { image_id: 42, .. }
    ));
    assert_eq!((image.pixel_width, image.pixel_height), (64.0, 32.0));
    let requests = host.image_requests.lock().expect("image requests lock");
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].colors.foreground().rgb24(), 0x00112233);
    assert_eq!(requests[0].colors.background().rgb24(), 0x00445566);
}

#[test]
fn chrome_image_request_carries_the_rows_fractional_frame_realization() {
    let _eval = Context::new();
    let rendered_text = Value::string_with_text_properties(
        "X",
        vec![neovm_core::emacs_core::value::StringTextPropertyRun {
            start: 0,
            end: 1,
            plist: Value::list(vec![
                Value::symbol("display"),
                Value::list(vec![
                    Value::symbol("image"),
                    Value::keyword("type"),
                    Value::symbol("svg"),
                    Value::keyword("file"),
                    Value::string("/tmp/chrome.svg"),
                    Value::keyword("height"),
                    Value::fixnum(24),
                    Value::keyword("scale"),
                    Value::symbol("default"),
                ]),
            ]),
        }],
    );
    let environment = neovm_core::emacs_core::image_catalog::ImageScaleEnvironment::new(
        7.2,
        1.75,
        neovm_core::emacs_core::image_catalog::ImageDefaultScale::Auto,
    );

    let (_, host) = render_tab_line_with_sized_media_metadata_host(
        rendered_text,
        0,
        0,
        18,
        18,
        17.0,
        13.0,
        None,
        environment,
    );

    let requests = host.image_requests.lock().expect("image requests lock");
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].realization.layout_dimension(24), 18);
    assert_eq!(requests[0].realization.raster_dimension(18), 32);
}

#[test]
fn image_replacement_uses_only_ready_decoded_opaque_background_metadata() {
    let mut eval = Context::new();
    eval.setup_thread_locals();
    let image_text = |explicit_background: bool| {
        let mut spec = vec![
            Value::symbol("image"),
            Value::keyword("type"),
            Value::symbol("png"),
            Value::keyword("file"),
            Value::string("/tmp/chrome.png"),
        ];
        if explicit_background {
            spec.extend([Value::keyword("background"), Value::string("#aabbcc")]);
        }
        Value::string_with_text_properties(
            "X",
            vec![StringTextPropertyRun {
                start: 0,
                end: 1,
                plist: Value::list(vec![Value::symbol("display"), Value::list(spec)]),
            }],
        )
    };
    let render = |text, metadata| {
        render_tab_line_with_sized_media_metadata_host(
            text,
            0,
            0,
            16,
            16,
            18.0,
            14.0,
            metadata,
            neovm_core::emacs_core::image_catalog::ImageScaleEnvironment::default(),
        )
        .0
    };
    let opaque = render(
        image_text(false),
        Some(ResolvedImageMetadata::layout_is_image_pixels(
            16,
            16,
            0x12_34_56,
            false,
            neomacs_display_protocol::ImageMaskKind::None,
        )),
    );
    let transparent = render(
        image_text(true),
        Some(ResolvedImageMetadata::layout_is_image_pixels(
            16,
            16,
            0,
            true,
            neomacs_display_protocol::ImageMaskKind::Clipping,
        )),
    );
    let not_ready = render(image_text(true), None);
    let background = |rendered: &RenderedDisplayRow| match rendered.row().glyphs
        [GlyphArea::Text.index()][0]
        .glyph_type
    {
        GlyphType::Image {
            opaque_background, ..
        } => opaque_background.get(),
        _ => panic!("expected image replacement"),
    };

    assert_eq!(background(&opaque), Some(0x12_34_56));
    assert_eq!(background(&transparent), None);
    assert_eq!(background(&not_ready), None);
}

#[test]
fn render_lisp_string_row_resolves_video_display_property_through_display_host() {
    let _eval = Context::new();
    let rendered_text = Value::string_with_text_properties(
        "AVB",
        vec![neovm_core::emacs_core::value::StringTextPropertyRun {
            start: 1,
            end: 2,
            plist: Value::list(vec![
                Value::symbol("display"),
                Value::list(vec![
                    Value::symbol("video"),
                    Value::keyword("file"),
                    Value::string("/tmp/chrome.mp4"),
                    Value::keyword("width"),
                    Value::fixnum(120),
                    Value::keyword("height"),
                    Value::fixnum(45),
                    Value::keyword("loop"),
                    Value::symbol("t"),
                    Value::keyword("autoplay"),
                    Value::symbol("t"),
                ]),
            ]),
        }],
    );
    let (rendered, host) = render_tab_line_with_media_host(rendered_text, 0x00FFFFFF, 0x00000000);

    let video = &rendered.row().glyphs[GlyphArea::Text.index()][1];
    assert!(matches!(
        video.glyph_type,
        GlyphType::Video {
            video_id: 84,
            loop_count: -1,
            autoplay: true,
            ..
        }
    ));
    assert_eq!((video.pixel_width, video.pixel_height), (120.0, 45.0));
    assert_eq!(
        host.video_requests
            .lock()
            .expect("video requests lock")
            .len(),
        1
    );
}

#[test]
fn render_lisp_string_row_resolves_webkit_display_property_through_display_host() {
    let _eval = Context::new();
    let rendered_text = Value::string_with_text_properties(
        "AWB",
        vec![neovm_core::emacs_core::value::StringTextPropertyRun {
            start: 1,
            end: 2,
            plist: Value::list(vec![
                Value::symbol("display"),
                Value::list(vec![
                    Value::symbol("webkit"),
                    Value::keyword("uri"),
                    Value::string("https://example.invalid/"),
                    Value::keyword("width"),
                    Value::fixnum(80),
                    Value::keyword("height"),
                    Value::fixnum(50),
                ]),
            ]),
        }],
    );
    let (rendered, host) = render_tab_line_with_media_host(rendered_text, 0x00FFFFFF, 0x00000000);

    let xwidget = &rendered.row().glyphs[GlyphArea::Text.index()][1];
    assert!(matches!(
        xwidget.glyph_type,
        GlyphType::Xwidget { xwidget_id, .. } if xwidget_id.get() == 99
    ));
    assert_eq!((xwidget.pixel_width, xwidget.pixel_height), (80.0, 50.0));
    assert_eq!(
        host.webkit_requests
            .lock()
            .expect("webkit requests lock")
            .len(),
        1
    );
}

#[test]
fn display_row_buffer_and_lisp_sources_share_display_space_semantics() {
    let _eval = Context::new();
    let display_space = Value::list(vec![
        Value::symbol("space"),
        Value::keyword("align-to"),
        Value::fixnum(4),
    ]);
    let lisp_row = render_lisp_display_row(
        Value::string_with_text_properties(
            "A B",
            vec![neovm_core::emacs_core::value::StringTextPropertyRun {
                start: 1,
                end: 2,
                plist: Value::list(vec![Value::symbol("display"), display_space.clone()]),
            }],
        ),
        GlyphRowRole::HeaderLine,
    );
    let buffer_row = render_buffer_display_row_with_property(
        "A B",
        1,
        2,
        Value::symbol("display"),
        display_space,
        GlyphRowRole::HeaderLine,
    );

    assert_eq!(row_text_expanding_stretches(&buffer_row), "A   B");
    assert_eq!(
        row_text_expanding_stretches(&buffer_row),
        row_text_expanding_stretches(&lisp_row)
    );
    assert_eq!(
        buffer_row.glyphs[1]
            .iter()
            .filter(|glyph| matches!(glyph.glyph_type, GlyphType::Stretch { .. }))
            .count(),
        lisp_row.glyphs[1]
            .iter()
            .filter(|glyph| matches!(glyph.glyph_type, GlyphType::Stretch { .. }))
            .count()
    );
}

#[test]
fn display_row_buffer_and_lisp_sources_share_display_replacement_string_semantics() {
    let _eval = Context::new();
    let replacement = Value::string("YZ");
    let lisp_row = render_lisp_display_row(
        Value::string_with_text_properties(
            "axb",
            vec![neovm_core::emacs_core::value::StringTextPropertyRun {
                start: 1,
                end: 2,
                plist: Value::list(vec![Value::symbol("display"), replacement]),
            }],
        ),
        GlyphRowRole::TabLine,
    );
    let buffer_row = render_buffer_display_row_with_property(
        "axb",
        1,
        2,
        Value::symbol("display"),
        replacement,
        GlyphRowRole::TabLine,
    );

    assert_eq!(row_text_expanding_stretches(&buffer_row), "aYZb");
    assert_eq!(
        row_text_expanding_stretches(&buffer_row),
        row_text_expanding_stretches(&lisp_row)
    );
    assert_eq!(
        row_text_glyph_types(&buffer_row),
        row_text_glyph_types(&lisp_row)
    );
}

#[test]
fn display_row_buffer_and_lisp_sources_share_face_property_semantics() {
    let _eval = Context::new();
    let face = Value::list(vec![Value::keyword("foreground"), Value::string("#ff0000")]);
    let lisp_row = render_lisp_display_row(
        Value::string_with_text_properties(
            "AB",
            vec![neovm_core::emacs_core::value::StringTextPropertyRun {
                start: 1,
                end: 2,
                plist: Value::list(vec![Value::symbol("face"), face.clone()]),
            }],
        ),
        GlyphRowRole::ModeLine,
    );
    let buffer_row = render_buffer_display_row_with_property(
        "AB",
        1,
        2,
        Value::symbol("face"),
        face,
        GlyphRowRole::ModeLine,
    );

    assert_eq!(row_text_expanding_stretches(&buffer_row), "AB");
    assert_eq!(row_text_face_ids(&buffer_row), row_text_face_ids(&lisp_row));
    let face_ids = row_text_face_ids(&buffer_row);
    assert_ne!(
        face_ids[0], face_ids[1],
        "buffer face property should split the row face like Lisp-string chrome"
    );
}

#[test]
fn display_row_buffer_and_lisp_sources_share_raise_property_semantics() {
    let _eval = Context::new();
    let raise = Value::list(vec![Value::symbol("raise"), Value::make_float(0.25)]);
    let lisp_row = render_lisp_display_row(
        Value::string_with_text_properties(
            "AB",
            vec![neovm_core::emacs_core::value::StringTextPropertyRun {
                start: 1,
                end: 2,
                plist: Value::list(vec![Value::symbol("display"), raise.clone()]),
            }],
        ),
        GlyphRowRole::ModeLine,
    );
    let buffer_row = render_buffer_display_row_with_property(
        "AB",
        1,
        2,
        Value::symbol("display"),
        raise,
        GlyphRowRole::ModeLine,
    );

    assert_eq!(row_text_expanding_stretches(&buffer_row), "AB");
    assert_eq!(
        row_text_expanding_stretches(&buffer_row),
        row_text_expanding_stretches(&lisp_row)
    );
    assert_eq!(
        buffer_row.glyphs[1]
            .iter()
            .map(|glyph| glyph.vertical_offset_px)
            .collect::<Vec<_>>(),
        lisp_row.glyphs[1]
            .iter()
            .map(|glyph| glyph.vertical_offset_px)
            .collect::<Vec<_>>()
    );
    assert_eq!(buffer_row.glyphs[1][0].vertical_offset_px, 0.0);
    assert_eq!(buffer_row.glyphs[1][1].vertical_offset_px, -4.0);
}

#[test]
fn display_row_buffer_and_lisp_sources_share_height_property_semantics() {
    let _eval = Context::new();
    let height = Value::list(vec![Value::symbol("height"), Value::make_float(2.0)]);
    let lisp_row = render_lisp_display_row(
        Value::string_with_text_properties(
            "AB",
            vec![neovm_core::emacs_core::value::StringTextPropertyRun {
                start: 1,
                end: 2,
                plist: Value::list(vec![Value::symbol("display"), height.clone()]),
            }],
        ),
        GlyphRowRole::ModeLine,
    );
    let buffer_row = render_buffer_display_row_with_property(
        "AB",
        1,
        2,
        Value::symbol("display"),
        height,
        GlyphRowRole::ModeLine,
    );

    assert_eq!(row_text_expanding_stretches(&buffer_row), "AB");
    assert_eq!(
        row_text_expanding_stretches(&buffer_row),
        row_text_expanding_stretches(&lisp_row)
    );
    assert_eq!(row_text_face_ids(&buffer_row), row_text_face_ids(&lisp_row));
    assert_ne!(
        buffer_row.glyphs[1][0].face_id,
        buffer_row.glyphs[1][1].face_id
    );
    assert_eq!(buffer_row.height_px, lisp_row.height_px);
    assert_eq!(buffer_row.ascent_px, lisp_row.ascent_px);
    assert_eq!(buffer_row.height_px, 32.0);
    assert_eq!(buffer_row.ascent_px, 24.0);
}

#[test]
fn display_row_buffer_and_lisp_sources_share_control_and_glyphless_semantics() {
    let _eval = Context::new();
    // U+FFFC stays glyphless; U+FFF0 would instead escape to `\`+octal.
    let text = "a\u{0001}\u{fffc}b";
    let lisp_row = render_lisp_display_row(Value::string(text), GlyphRowRole::HeaderLine);
    let buffer_row = render_buffer_display_row(text, GlyphRowRole::HeaderLine);

    assert_eq!(
        row_text_glyph_types(&buffer_row),
        row_text_glyph_types(&lisp_row)
    );
    assert!(
        row_text_glyph_types(&buffer_row)
            .iter()
            .any(|kind| matches!(kind, GlyphType::Glyphless { ch: '\u{fffc}' })),
        "glyphless buffer source chars should reach the same row builder path as Lisp strings"
    );
}

#[test]
fn render_display_item_source_row_uses_spec_tab_policy() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.insert("\tX");
    }
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let mut font_metrics = None;
    let mut renderer =
        DisplayRowRenderer::new(&mut font_metrics, DisplayRowMeasurementMode::LogicalCells);
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(1);
    let mut source = crate::buffer_source::text_source::BufferTextSourceCursor::new(
        buf_id,
        buffer,
        neovm_core::buffer::CharPos0::new(0),
        buffer.layout_point_max_char_pos(),
        RenderFaceRef::FaceId(FaceId::new(1)),
    );

    let rendered = display_row_request_for_face(
        DisplayRowGeometry {
            y: 0.0,
            width: 240.0,
            height: 16.0,
            char_width: 8.0,
            ascent: 12.0,
            tab_policy: crate::display_row::builder::DisplayTabPolicy::from_tab_width_and_stops(
                0.0,
                4,
                &[2],
            ),
        },
        FaceId::new(1),
        resolver.default_face(),
        GlyphRowRole::TabLine,
    )
    .render(&mut renderer, &mut source, &resolver, &mut face_ids)
    .expect("display source row");

    let glyphs = &rendered.row().glyphs[1];
    assert_eq!(glyphs[0].glyph_type, GlyphType::Stretch { width_cols: 2 });
    let emitted_width: f32 = glyphs.iter().map(|glyph| glyph.pixel_width).sum();
    assert!(
        (rendered.progress().end_x() - emitted_width).abs() <= 0.01,
        "row progress should include the emitted tab stretch and following character"
    );
}

#[test]
fn render_lisp_string_row_uses_explicit_tab_policy() {
    let _eval = Context::new();
    let mut engine = crate::engine::LayoutEngine::new();
    let mut renderer = DisplayRowRenderer::new(
        &mut engine.font_metrics,
        DisplayRowMeasurementMode::LogicalCells,
    );
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(1);
    let spec = display_row_request_from_base_face(
        DisplayRowGeometry {
            y: 0.0,
            width: 240.0,
            height: 16.0,
            char_width: 8.0,
            ascent: 12.0,
            tab_policy: crate::display_row::builder::DisplayTabPolicy::from_tab_width_and_stops(
                0.0,
                4,
                &[2],
            ),
        },
        &mut face_ids,
        resolver.default_face(),
        GlyphRowRole::TabLine,
        std::collections::HashMap::new(),
    );

    let rendered = render_lisp_string_row(
        &mut renderer,
        spec,
        Value::string("\tX"),
        &resolver,
        &mut face_ids,
    )
    .expect("display source row");

    let glyphs = &rendered.row().glyphs[1];
    assert_eq!(glyphs[0].glyph_type, GlyphType::Stretch { width_cols: 2 });
}

#[test]
fn display_row_glyph_measurer_uses_face_specific_widths() {
    let mut base = base_face();
    base.set_measured_char_width_px(5.0);
    let mut wide = base.clone();
    wide.set_measured_char_width_px(9.0);
    let faces = vec![
        DisplayRowFace::from_resolved(FaceId::new(1), &base),
        DisplayRowFace::from_resolved(FaceId::new(2), &wide),
    ];
    let mut measurer = DisplayRowGlyphMeasurer::new(&faces, None, 5.0);

    assert_eq!(
        measurer.glyph_advance_px('a', FaceId::new(1), 1, 5.0),
        Some(5.0)
    );
    assert_eq!(
        measurer.glyph_advance_px('中', FaceId::new(2), 2, 10.0),
        Some(18.0)
    );
}

#[test]
fn display_row_glyph_measurer_preserves_fractional_gui_advances() {
    let mut base = base_face();
    base.set_measured_char_width_px(7.2);
    let faces = vec![DisplayRowFace::from_resolved(FaceId::new(1), &base)];
    let mut measurer = DisplayRowGlyphMeasurer::new(&faces, None, 7.2);

    assert_eq!(
        measurer.glyph_advance_px('x', FaceId::new(1), 1, 7.2),
        Some(7.2)
    );
}

#[test]
fn display_row_glyph_measurer_can_snap_terminal_advances() {
    let mut base = base_face();
    base.set_measured_char_width_px(7.2);
    let faces = vec![DisplayRowFace::from_resolved(FaceId::new(1), &base)];
    let mut measurer = DisplayRowGlyphMeasurer::with_quantization(
        &faces,
        None,
        7.2,
        GlyphAdvanceQuantization::SnapToIntegerPixels,
    );

    assert_eq!(
        measurer.glyph_advance_px('x', FaceId::new(1), 1, 7.2),
        Some(7.0)
    );
}

#[test]
fn display_row_glyph_measurement_face_measures_single_char_columns() {
    let mut base = base_face();
    base.set_measured_char_width_px(7.2);
    let face = DisplayRowFace::from_resolved(FaceId::new(8), &base);
    let measurement_face = DisplayRowGlyphMeasurementFace::with_mode(
        face,
        DisplayRowMeasurementMode::LogicalCells,
        7.2,
        GlyphAdvanceQuantization::SnapToIntegerPixels,
    );
    let mut font_metrics = None;

    assert_eq!(
        measurement_face.advance_for_char(&mut font_metrics, '.', 7.2),
        7.0
    );
    assert_eq!(
        measurement_face.advance_for_char(&mut font_metrics, '中', 14.4),
        14.0
    );
}

#[test]
fn display_row_glyph_measurement_face_constructs_from_resolved_face_policy() {
    let mut base = base_face();
    base.set_measured_char_width_px(7.2);
    let measurement_face =
        DisplayRowMeasurementPolicy::for_mode(DisplayRowMeasurementMode::LogicalCells)
            .measurement_face(FaceId::new(8), &base, None, 7.2);
    let mut font_metrics = None;

    assert_eq!(
        measurement_face.advance_for_char(&mut font_metrics, '.', 7.2),
        7.0
    );
}

#[test]
fn display_row_measurement_policy_builds_faces_from_frame_mode() {
    let mut base = base_face();
    base.set_measured_char_width_px(7.2);
    let tty_policy = DisplayRowMeasurementPolicy::for_mode(DisplayRowMeasurementMode::LogicalCells);
    let gui_policy = DisplayRowMeasurementPolicy::for_mode(DisplayRowMeasurementMode::ConcreteFont);
    let mut font_metrics = None;

    let tty_face = tty_policy.measurement_face(FaceId::new(8), &base, None, 7.2);
    let gui_face = gui_policy.measurement_face(FaceId::new(8), &base, None, 7.2);

    assert_eq!(tty_face.advance_for_char(&mut font_metrics, '.', 7.2), 7.0);
    assert_eq!(gui_face.advance_for_char(&mut font_metrics, '.', 7.2), 7.2);
}

#[test]
fn display_row_gui_measurement_preserves_narrow_proportional_glyph_advance() {
    let mut base = base_face();
    base.font_family = "Noto Sans".to_string();
    base.font_size = 9.12871;
    base.font_weight = 400;
    base.set_measured_char_width_px(7.2);
    let gui_face = DisplayRowMeasurementPolicy::for_mode(DisplayRowMeasurementMode::ConcreteFont)
        .measurement_face(FaceId::new(8), &base, None, 7.2);
    let mut font_metrics = Some(FontMetricsService::new());

    let width = gui_face.advance_for_char(&mut font_metrics, '.', 7.2);

    assert!(
        width > 0.0 && width < 7.2,
        "GNU GUI display uses the realized font's per-glyph advance for proportional faces; got {width}"
    );
}

#[test]
fn display_row_gui_renderer_preserves_narrow_proportional_glyph_advance() {
    let _eval = Context::new();
    let mut base = base_face();
    base.font_family = "Noto Sans".to_string();
    base.font_size = 9.12871;
    base.font_weight = 400;
    base.set_measured_char_width_px(7.2);
    base.font_ascent = 10.0;
    let mut font_metrics = Some(FontMetricsService::new());
    let mut renderer =
        DisplayRowRenderer::new(&mut font_metrics, DisplayRowMeasurementMode::ConcreteFont);
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, Some("neo".into()));
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(1);
    let request = display_row_request_from_base_face(
        DisplayRowGeometry {
            y: 0.0,
            width: 240.0,
            height: 17.0,
            char_width: 7.2,
            ascent: 10.0,
            tab_policy: crate::display_row::builder::DisplayTabPolicy::every(8),
        },
        &mut face_ids,
        &base,
        GlyphRowRole::Text,
        std::collections::HashMap::new(),
    );

    let rendered = render_lisp_string_row(
        &mut renderer,
        request,
        Value::string(".agent-sh"),
        &resolver,
        &mut face_ids,
    )
    .expect("display source row");
    let first_width = rendered.row().glyphs[GlyphArea::Text.index()][0].pixel_width;

    assert!(
        first_width > 0.0 && first_width < 7.2,
        "GUI row rendering must not floor proportional glyph advances to the frame cell; got {first_width}"
    );
}

#[test]
fn display_row_gui_measurement_preserves_narrow_proportional_text_run_advances() {
    let mut base = base_face();
    base.font_family = "Noto Sans".to_string();
    base.font_size = 9.12871;
    base.font_weight = 400;
    base.set_measured_char_width_px(7.2);
    let gui_face = DisplayRowMeasurementPolicy::for_mode(DisplayRowMeasurementMode::ConcreteFont)
        .measurement_face(FaceId::new(8), &base, None, 7.2);
    let mut font_metrics = Some(FontMetricsService::new());

    let measurement = gui_face.text_run_measurement(&mut font_metrics, ".agent-sh");

    let crate::display_text_run_measurement::DisplayTextRunMeasurement::Measured(advances) =
        measurement
    else {
        panic!("GUI font-backed proportional text should produce measured text-run advances");
    };
    let first = advances.first().expect("first glyph advance").advance_px;
    assert!(
        first > 0.0 && first < 7.2,
        "GNU GUI display keeps the realized proportional glyph advance in text runs; got {first}"
    );
}

#[test]
fn display_row_fallback_metrics_builds_from_default_face_extents() {
    let fallback = DisplayRowFallbackMetrics::from_default_face_extents(7.5, 18.0, 13.0);

    assert_eq!(
        fallback,
        DisplayRowFallbackMetrics {
            char_width: 7.5,
            row_height: 18.0,
            ascent: 13.0,
        }
    );
}

#[test]
fn display_row_measurement_policy_builds_measured_face_with_space_width() {
    let mut base = base_face();
    base.set_measured_char_width_px(7.2);
    let policy = DisplayRowMeasurementPolicy::for_mode(DisplayRowMeasurementMode::LogicalCells);
    let mut font_metrics = None;

    let active = DisplayRowActiveFaceState::new(
        base.clone(),
        policy.measured_face(
            FaceId::new(8),
            &base,
            None,
            7.2,
            DisplayRowFallbackMetrics {
                char_width: 7.2,
                row_height: 16.0,
                ascent: 11.0,
            },
            &mut font_metrics,
        ),
    );

    assert_eq!(active.metrics().space_width(), 7.0);
    assert_eq!(active.advance_for_char(&mut font_metrics, 'x', 7.2), 7.0);
    assert_eq!(active.advance_for_columns(&mut font_metrics, 'x', 2), 14.0);

    let text_run_measurement = active.text_run_measurement(&mut font_metrics, "a中");
    let crate::display_text_run_measurement::DisplayTextRunMeasurement::Measured(advances) =
        text_run_measurement
    else {
        panic!("active face should produce text-run measurement plans");
    };
    assert_eq!(
        advances
            .iter()
            .map(|advance| (advance.char_offset, advance.byte_offset, advance.advance_px))
            .collect::<Vec<_>>(),
        vec![(0, 0, 7.0), (1, 1, 14.0)]
    );
}

#[test]
fn display_row_measurement_policy_builds_measured_face_with_line_metrics() {
    let mut base = base_face();
    base.set_measured_char_width_px(7.2);
    let metrics = crate::font::metrics::FontMetrics {
        ascent: 13.0,
        descent: 5.0,
        line_height: 18.0,
        char_width: 9.0,
        space_width: 6.0,
    };
    let policy = DisplayRowMeasurementPolicy::for_mode(DisplayRowMeasurementMode::ConcreteFont);
    let mut font_metrics = None;

    let measured = policy.measured_face(
        FaceId::new(8),
        &base,
        Some(metrics),
        7.2,
        DisplayRowFallbackMetrics {
            char_width: 7.2,
            row_height: 16.0,
            ascent: 11.0,
        },
        &mut font_metrics,
    );

    let measured_metrics = measured.metrics();
    assert_eq!(measured_metrics.char_width(), 9.0);
    assert_eq!(measured_metrics.row_height(), 18.0);
    assert_eq!(measured_metrics.ascent(), 13.0);
}

#[test]
fn display_row_measured_face_exposes_face_identity() {
    let base = base_face();
    let policy = DisplayRowMeasurementPolicy::for_mode(DisplayRowMeasurementMode::LogicalCells);
    let mut font_metrics = None;

    let measured = policy.measured_face(
        FaceId::new(42),
        &base,
        None,
        7.2,
        DisplayRowFallbackMetrics {
            char_width: 7.2,
            row_height: 16.0,
            ascent: 11.0,
        },
        &mut font_metrics,
    );

    let active = DisplayRowActiveFaceState::new(base, measured);
    assert_eq!(active.face_id(), FaceId::new(42));
}

#[test]
fn display_row_measured_face_exposes_metrics_as_single_value() {
    let base = base_face();
    let policy = DisplayRowMeasurementPolicy::for_mode(DisplayRowMeasurementMode::LogicalCells);
    let mut font_metrics = None;

    let measured = policy.measured_face(
        FaceId::new(42),
        &base,
        None,
        7.2,
        DisplayRowFallbackMetrics {
            char_width: 7.2,
            row_height: 16.0,
            ascent: 11.0,
        },
        &mut font_metrics,
    );

    let metrics = measured.metrics();

    assert_eq!(metrics.char_width(), 7.2);
    assert_eq!(metrics.row_height(), 16.0);
    assert_eq!(metrics.ascent(), 11.0);
    assert_eq!(metrics.space_width(), 7.0);
}

#[test]
fn display_row_glyph_measurement_face_shapes_text_runs_as_measurement_plans() {
    let mut base = base_face();
    base.font_family = "monospace".to_string();
    base.font_size = 14.0;
    base.set_measured_char_width_px(8.0);
    let measurement_face =
        DisplayRowMeasurementPolicy::for_mode(DisplayRowMeasurementMode::ConcreteFont)
            .measurement_face(FaceId::new(8), &base, None, 8.0);
    let mut font_metrics = Some(FontMetricsService::new());

    let measurement = measurement_face.text_run_measurement(&mut font_metrics, "سلام");

    let crate::display_text_run_measurement::DisplayTextRunMeasurement::Measured(advances) =
        measurement
    else {
        panic!("complex script run should produce a measured text-run plan");
    };
    assert!(
        !advances.is_empty(),
        "complex script run should produce cluster advances"
    );
    assert!(
        advances.iter().all(|advance| advance.advance_px >= 0.0),
        "cluster advances should never be negative: {advances:?}"
    );
}

#[test]
fn display_text_run_measurement_plan_builds_from_shaped_glyphs() {
    fn shaped(cluster_start: usize, x_advance: f32) -> crate::font::metrics::ShapedGlyph {
        crate::font::metrics::ShapedGlyph {
            font_id: fontdb::ID::dummy(),
            glyph_id: 1,
            x: 0.0,
            y: 0.0,
            x_advance,
            cluster_start,
            cluster_end: cluster_start + 1,
        }
    }

    let measurement =
        crate::display_text_run_measurement::DisplayTextRunMeasurementPlan::from_shaped_glyphs(
            "aéb",
            [shaped(0, 2.0), shaped(1, 7.0), shaped(3, 8.5)],
            6.0,
            4.0,
            GlyphAdvanceQuantization::PreserveLogicalPixels,
            true,
        );

    let crate::display_text_run_measurement::DisplayTextRunMeasurement::Measured(advances) =
        measurement
    else {
        panic!("shaped glyphs should produce measured text-run advances");
    };
    assert_eq!(
        advances
            .iter()
            .map(|advance| (advance.char_offset, advance.byte_offset, advance.advance_px))
            .collect::<Vec<_>>(),
        vec![(0, 0, 6.0), (1, 1, 7.0), (2, 3, 8.5)]
    );
}

#[test]
fn display_row_glyph_measurer_builds_measured_complex_text_run_plan() {
    let mut base = base_face();
    base.font_family = "monospace".to_string();
    base.font_size = 14.0;
    base.set_measured_char_width_px(8.0);
    let faces = vec![DisplayRowFace::from_resolved(FaceId::new(8), &base)];
    let mut font_metrics = FontMetricsService::new();
    let mut measurer = DisplayRowGlyphMeasurer::new(&faces, Some(&mut font_metrics), 8.0);

    let measurement = measurer.text_run_advances_px("سلام", FaceId::new(8), 8.0);

    let crate::display_text_run_measurement::DisplayTextRunMeasurement::Measured(advances) =
        measurement
    else {
        panic!("font-backed measurer should produce a measured text-run plan");
    };
    assert_eq!(
        advances
            .iter()
            .map(|advance| (advance.char_offset, advance.byte_offset))
            .collect::<Vec<_>>(),
        vec![(0, 0), (1, 2), (2, 4), (3, 6)]
    );
    assert!(
        advances.iter().all(|advance| advance.advance_px > 0.0),
        "contextually shaped advances should remain positive: {advances:?}"
    );
}

#[test]
fn display_row_glyph_measurer_measures_every_plain_latin_character_independently() {
    let mut base = base_face();
    base.font_family = "Noto Sans".to_string();
    base.font_size = 14.0;
    base.set_measured_char_width_px(8.0);
    let faces = vec![DisplayRowFace::from_resolved(FaceId::new(8), &base)];
    let mut font_metrics = FontMetricsService::new();
    let mut measurer = DisplayRowGlyphMeasurer::new(&faces, Some(&mut font_metrics), 8.0);

    let measurement = measurer.text_run_advances_px("flake.nix", FaceId::new(8), 8.0);
    let crate::display_text_run_measurement::DisplayTextRunMeasurement::Measured(advances) =
        measurement
    else {
        panic!("font-backed ordinary text should retain exact per-character lookahead");
    };
    assert_eq!(
        advances
            .iter()
            .map(|advance| (advance.char_offset, advance.byte_offset))
            .collect::<Vec<_>>(),
        (0..9).map(|offset| (offset, offset)).collect::<Vec<_>>(),
        "ordinary Latin text must be measured the same way it is emitted; shaping `fl` as one ligature cluster drops the independent `l` advance",
    );
}

#[test]
fn display_row_glyph_measurement_face_builds_text_run_measurement_plan() {
    let mut base = base_face();
    base.font_family = "monospace".to_string();
    base.font_size = 14.0;
    base.set_measured_char_width_px(8.0);
    let measurement_face =
        DisplayRowMeasurementPolicy::for_mode(DisplayRowMeasurementMode::ConcreteFont)
            .measurement_face(FaceId::new(8), &base, None, 8.0);
    let mut font_metrics = Some(FontMetricsService::new());

    let measurement = measurement_face.text_run_measurement(&mut font_metrics, "abc");

    let crate::display_text_run_measurement::DisplayTextRunMeasurement::Measured(advances) =
        measurement
    else {
        panic!("font-backed measurement face should produce a measured text-run plan");
    };
    assert_eq!(
        advances
            .iter()
            .map(|advance| (advance.char_offset, advance.byte_offset))
            .collect::<Vec<_>>(),
        vec![(0, 0), (1, 1), (2, 2)]
    );
}

#[test]
fn display_row_glyph_measurement_face_builds_fallback_text_run_measurement_plan() {
    let mut base = base_face();
    base.set_measured_char_width_px(7.2);
    let measurement_face =
        DisplayRowMeasurementPolicy::for_mode(DisplayRowMeasurementMode::LogicalCells)
            .measurement_face(FaceId::new(8), &base, None, 7.2);
    let mut font_metrics = None;

    let measurement = measurement_face.text_run_measurement(&mut font_metrics, "a中");

    let crate::display_text_run_measurement::DisplayTextRunMeasurement::Measured(advances) =
        measurement
    else {
        panic!("measurement face should fall back to char-advance text-run plans");
    };
    assert_eq!(
        advances
            .iter()
            .map(|advance| (advance.char_offset, advance.byte_offset, advance.advance_px))
            .collect::<Vec<_>>(),
        vec![(0, 0, 7.0), (1, 1, 14.0)]
    );
}

#[test]
fn display_text_run_measurement_plan_builds_resolved_source_advance() {
    let measurement =
        crate::display_text_run_measurement::DisplayTextRunMeasurementPlan::from_resolved_source_advance(
            "\u{301}",
            0.0,
        );

    let crate::display_text_run_measurement::DisplayTextRunMeasurement::Measured(advances) =
        measurement
    else {
        panic!("resolved source advances should produce measured text-run plans");
    };
    assert_eq!(
        advances
            .iter()
            .map(|advance| (advance.char_offset, advance.byte_offset, advance.advance_px))
            .collect::<Vec<_>>(),
        vec![(0, 0, 0.0)]
    );

    let wide_measurement =
        crate::display_text_run_measurement::DisplayTextRunMeasurementPlan::from_resolved_source_advance(
            "中", 14.0,
        );
    let crate::display_text_run_measurement::DisplayTextRunMeasurement::Measured(wide_advances) =
        wide_measurement
    else {
        panic!("resolved wide source advance should produce a measured text-run plan");
    };
    assert_eq!(
        wide_advances
            .iter()
            .map(|advance| (advance.char_offset, advance.byte_offset, advance.advance_px))
            .collect::<Vec<_>>(),
        vec![(0, 0, 14.0)]
    );
}

#[test]
fn display_row_baseline_tab_bar_preserves_lisp_string_face_properties() {
    let _eval = Context::new();
    let rendered = Value::string_with_text_properties(
        "AB",
        vec![neovm_core::emacs_core::value::StringTextPropertyRun {
            start: 1,
            end: 2,
            plist: Value::list(vec![
                Value::symbol("face"),
                Value::list(vec![Value::keyword("foreground"), Value::string("#ff0000")]),
            ]),
        }],
    );

    let row = render_lisp_display_row(rendered, GlyphRowRole::TabBar);
    let glyphs = &row.glyphs[1];

    assert_eq!(row.role, GlyphRowRole::TabBar);
    assert_eq!(row_text_expanding_stretches(&row), "AB");
    assert_eq!(glyphs.len(), 2);
    assert_ne!(
        glyphs[0].face_id, glyphs[1].face_id,
        "propertized tab-bar chars should keep separate face ids"
    );
}

#[test]
fn display_row_baseline_tab_bar_preserves_lisp_string_raise_property() {
    let _eval = Context::new();
    let rendered = Value::string_with_text_properties(
        "AB",
        vec![neovm_core::emacs_core::value::StringTextPropertyRun {
            start: 1,
            end: 2,
            plist: Value::list(vec![
                Value::symbol("display"),
                Value::list(vec![Value::symbol("raise"), Value::make_float(0.25)]),
            ]),
        }],
    );

    let row = render_lisp_display_row(rendered, GlyphRowRole::TabBar);
    let glyphs = &row.glyphs[1];

    assert_eq!(row.role, GlyphRowRole::TabBar);
    assert_eq!(row_text_expanding_stretches(&row), "AB");
    assert_eq!(glyphs.len(), 2);
    assert_eq!(glyphs[0].vertical_offset_px, 0.0);
    assert_eq!(glyphs[1].vertical_offset_px, -4.0);
}

#[test]
fn display_row_baseline_tab_bar_preserves_lisp_string_height_property() {
    let _eval = Context::new();
    let rendered = Value::string_with_text_properties(
        "AB",
        vec![neovm_core::emacs_core::value::StringTextPropertyRun {
            start: 1,
            end: 2,
            plist: Value::list(vec![
                Value::symbol("display"),
                Value::list(vec![Value::symbol("height"), Value::make_float(2.0)]),
            ]),
        }],
    );

    let rendered = render_lisp_display_row_output(rendered, GlyphRowRole::TabBar);
    let row = rendered.row();
    let glyphs = &row.glyphs[1];

    assert_eq!(row.role, GlyphRowRole::TabBar);
    assert_eq!(row_text_expanding_stretches(row), "AB");
    assert_eq!(glyphs.len(), 2);
    assert_ne!(
        glyphs[0].face_id, glyphs[1].face_id,
        "height display property should realize a separate face like GNU face_with_height"
    );
    let raised_face = rendered
        .faces()
        .iter()
        .find(|face| face.id == glyphs[1].face_id)
        .expect("height-adjusted face");
    assert_eq!(raised_face.font_size, 28.0);
    assert_eq!(raised_face.font_ascent, 24);
    assert_eq!(row.height_px, 32.0);
    assert_eq!(row.ascent_px, 24.0);
    assert_eq!(rendered.progress().height(), 32.0);
}

#[test]
fn display_row_baseline_mode_line_display_space_align_expands_to_spaces() {
    let _eval = Context::new();
    let rendered = Value::string_with_text_properties(
        "A B",
        vec![neovm_core::emacs_core::value::StringTextPropertyRun {
            start: 1,
            end: 2,
            plist: Value::list(vec![
                Value::symbol("display"),
                Value::list(vec![
                    Value::symbol("space"),
                    Value::keyword("align-to"),
                    Value::fixnum(4),
                ]),
            ]),
        }],
    );

    let row = render_lisp_display_row(rendered, GlyphRowRole::ModeLine);

    assert_eq!(row.role, GlyphRowRole::ModeLine);
    assert_eq!(row_text_expanding_stretches(&row), "A   B");
}

/// GNU `produce_stretch_glyph` permits a valid `:align-to` expression to
/// resolve to zero width.  Magit relies on that for the header prefix
/// `(propertize " " 'display '(space :align-to 0))`: at column zero the
/// source character is replaced, but no glyph is emitted.
#[test]
fn header_line_align_to_current_column_emits_no_glyph() {
    let _eval = Context::new();
    let rendered = Value::string_with_text_properties(
        " C",
        vec![neovm_core::emacs_core::value::StringTextPropertyRun {
            start: 0,
            end: 1,
            plist: Value::list(vec![
                Value::symbol("display"),
                Value::list(vec![
                    Value::symbol("space"),
                    Value::keyword("align-to"),
                    Value::fixnum(0),
                ]),
            ]),
        }],
    );

    let row = render_lisp_display_row(rendered, GlyphRowRole::HeaderLine);

    assert_eq!(row_text_expanding_stretches(&row), "C");
    assert!(
        row.glyphs[1]
            .iter()
            .all(|glyph| !matches!(glyph.glyph_type, GlyphType::Stretch { .. })),
        "a zero-width align-to replacement must not materialize a terminal cell: {:?}",
        row.glyphs[1]
    );
}

#[test]
fn display_row_baseline_header_line_display_space_relative_width_expands_to_stretch() {
    let _eval = Context::new();
    let rendered = Value::string_with_text_properties(
        "C R",
        vec![neovm_core::emacs_core::value::StringTextPropertyRun {
            start: 1,
            end: 2,
            plist: Value::list(vec![
                Value::symbol("display"),
                Value::list(vec![
                    Value::symbol("space"),
                    Value::keyword("relative-width"),
                    Value::fixnum(2),
                ]),
            ]),
        }],
    );

    let row = render_lisp_display_row(rendered, GlyphRowRole::HeaderLine);

    assert_eq!(row.role, GlyphRowRole::HeaderLine);
    assert_eq!(row_text_expanding_stretches(&row), "C  R");
    assert!(
        row.glyphs[1]
            .iter()
            .any(|glyph| matches!(glyph.glyph_type, GlyphType::Stretch { width_cols: 2 })),
        "relative-width display space should become a stretch glyph: {:?}",
        row.glyphs[1]
    );
}

#[test]
fn display_row_baseline_header_line_align_to_symbol_values() {
    let _eval = Context::new();
    let rendered = Value::string_with_text_properties(
        "C ",
        vec![neovm_core::emacs_core::value::StringTextPropertyRun {
            start: 1,
            end: 2,
            plist: Value::list(vec![
                Value::symbol("display"),
                Value::list(vec![
                    Value::symbol("space"),
                    Value::keyword("align-to"),
                    Value::list(vec![
                        Value::symbol("+"),
                        Value::symbol("header-line-indent-width"),
                        Value::fixnum(1),
                    ]),
                ]),
            ]),
        }],
    );
    let mut symbol_values = std::collections::HashMap::new();
    symbol_values.insert("header-line-indent-width".to_string(), Value::fixnum(0));

    let row =
        render_lisp_display_row_with_symbols(rendered, GlyphRowRole::HeaderLine, symbol_values);

    assert_eq!(row.role, GlyphRowRole::HeaderLine);
    assert_eq!(row_text_expanding_stretches(&row), "C");
}

/// Chrome `(space :align-to right)` must push the following stretch to the
/// window's right region — the capability the now-retired `length_expr_pixels`
/// could not provide for window-box symbols. The chrome row is 240px wide with
/// an 8px cell (30 cols), so a stretch starting right after the leading "L"
/// (1 col, 8px) must span the remaining 29 cols / 232px to reach the window's
/// right edge at x=240.
///
/// `right` is resolved by the single GNU-faithful evaluator
/// (`calc_pixel_width_or_height`, xdisp.c:30435) against the chrome row's
/// `PixelCalcContext`, exactly as the buffer text path resolves it. Under the
/// old chrome evaluator a region symbol could only reach `width_px` crudely;
/// the unified path now resolves it through the same authority.
#[test]
fn mode_line_align_to_right_region_reaches_window_right_edge() {
    let _eval = Context::new();
    let rendered = Value::string_with_text_properties(
        "L ",
        vec![neovm_core::emacs_core::value::StringTextPropertyRun {
            start: 1,
            end: 2,
            plist: Value::list(vec![
                Value::symbol("display"),
                Value::list(vec![
                    Value::symbol("space"),
                    Value::keyword("align-to"),
                    Value::symbol("right"),
                ]),
            ]),
        }],
    );

    let row = render_lisp_display_row(rendered, GlyphRowRole::ModeLine);

    assert_eq!(row.role, GlyphRowRole::ModeLine);
    // The stretch fills from x=8 (after "L") to the text-area-right region at
    // x=240: (240 - 8) / 8 = 29 cols / 232px. Reaching the right edge is the
    // capability the unified evaluator unlocks for chrome rows.
    let stretch = row.glyphs[1]
        .iter()
        .find(|glyph| matches!(glyph.glyph_type, GlyphType::Stretch { .. }))
        .expect("align-to right should emit a stretch glyph");
    assert_eq!(
        stretch.glyph_type,
        GlyphType::Stretch { width_cols: 29 },
        "align-to right must reach the window-right region: {:?}",
        row.glyphs[1]
    );
    assert_eq!(
        stretch.pixel_width, 232.0,
        "align-to right stretch must end at the window right edge (x=240)"
    );
}

/// A window-region symbol the OLD `length_expr_pixels` evaluator zeroed out
/// (`right-fringe` returned 0.0) must now resolve to a real window position
/// through the unified evaluator. This pins the actual capability gain:
///
/// - OLD behavior: `right-fringe` → 0.0, so the stretch width is
///   `max(0 - 8, 0) == 0` (a 0-col stretch); the `width_cols: 29` /
///   `pixel_width: 232.0` assertions below would FAIL.
/// - NEW behavior: the GNU-faithful resolver maps `right-fringe` to the chrome
///   row's right edge (240px), producing a 29-col / 232px stretch.
#[test]
fn mode_line_align_to_fringe_region_now_resolves_to_window_edge() {
    let _eval = Context::new();
    let rendered = Value::string_with_text_properties(
        "L ",
        vec![neovm_core::emacs_core::value::StringTextPropertyRun {
            start: 1,
            end: 2,
            plist: Value::list(vec![
                Value::symbol("display"),
                Value::list(vec![
                    Value::symbol("space"),
                    Value::keyword("align-to"),
                    Value::symbol("right-fringe"),
                ]),
            ]),
        }],
    );

    let row = render_lisp_display_row(rendered, GlyphRowRole::ModeLine);

    assert_eq!(row.role, GlyphRowRole::ModeLine);
    let stretch = row.glyphs[1]
        .iter()
        .find(|glyph| matches!(glyph.glyph_type, GlyphType::Stretch { .. }))
        .expect("align-to right-fringe should emit a stretch glyph");
    // Would be a 0-col / 0px stretch under the retired `length_expr_pixels`
    // (which returned 0.0 for right-fringe): this is the must-fail assertion.
    assert_eq!(
        stretch.glyph_type,
        GlyphType::Stretch { width_cols: 29 },
        "align-to right-fringe must reach the window edge, not stay at 0: {:?}",
        row.glyphs[1]
    );
    assert_eq!(
        stretch.pixel_width, 232.0,
        "align-to right-fringe stretch must end at the window right edge (x=240)"
    );
}

/// Doom-modeline computes TTY right alignment as a one-element pixel list,
/// e.g. `(space :align-to (96))`, after subtracting window margins in Lisp.
/// GNU's mode-line renderer then adds `window_box_left_offset(TEXT_AREA)` for
/// this raw numeric target. With a 40px left text-area offset, the target is
/// 136px instead of 96px, so the stretch after "L" spans 128px / 16 cols.
#[test]
fn mode_line_pixel_align_to_adds_window_text_area_left_offset() {
    let _eval = Context::new();
    let rendered = Value::string_with_text_properties(
        "L R",
        vec![neovm_core::emacs_core::value::StringTextPropertyRun {
            start: 1,
            end: 2,
            plist: Value::list(vec![
                Value::symbol("display"),
                Value::list(vec![
                    Value::symbol("space"),
                    Value::keyword("align-to"),
                    Value::list(vec![Value::fixnum(96)]),
                ]),
            ]),
        }],
    );

    let row = render_lisp_display_row_output_with_symbols_and_chrome_text_area_left(
        rendered,
        GlyphRowRole::ModeLine,
        std::collections::HashMap::new(),
        40.0,
    )
    .into_row();

    assert_eq!(row.role, GlyphRowRole::ModeLine);
    assert_eq!(
        row_text_expanding_stretches(&row),
        format!("L{}R", " ".repeat(16))
    );
    let stretch = row.glyphs[1]
        .iter()
        .find(|glyph| matches!(glyph.glyph_type, GlyphType::Stretch { .. }))
        .expect("numeric align-to should emit a stretch glyph");
    assert_eq!(
        stretch.glyph_type,
        GlyphType::Stretch { width_cols: 16 },
        "mode-line numeric align-to must include the text-area left offset: {:?}",
        row.glyphs[1]
    );
    assert_eq!(
        stretch.pixel_width, 128.0,
        "target should be 96px + 40px text-area offset, minus current x=8px"
    );
}

/// Buffer-path `(space :align-to N)` must be unchanged by the unification: the
/// buffer text path already used `calc_pixel_width_or_height`, so this pins
/// that path's output stays byte-identical. An `:align-to 4` over a single
/// character produces a stretch filling columns 1..4 after the initial "X".
#[test]
fn buffer_align_to_number_unchanged_by_unification() {
    let _eval = Context::new();
    let row = render_buffer_display_row_with_property(
        "X Y",
        1,
        2,
        Value::symbol("display"),
        Value::list(vec![
            Value::symbol("space"),
            Value::keyword("align-to"),
            Value::fixnum(4),
        ]),
        GlyphRowRole::Text,
    );

    assert_eq!(row_text_expanding_stretches(&row), "X   Y");
    assert!(
        row.glyphs[1]
            .iter()
            .any(|glyph| matches!(glyph.glyph_type, GlyphType::Stretch { width_cols: 3 })),
        "buffer align-to 4 should still emit a 3-col stretch: {:?}",
        row.glyphs[1]
    );
}

#[test]
fn display_row_baseline_header_line_align_to_skips_multi_char_interval() {
    let _eval = Context::new();
    let rendered = Value::string_with_text_properties(
        "X   Y",
        vec![neovm_core::emacs_core::value::StringTextPropertyRun {
            start: 1,
            end: 4,
            plist: Value::list(vec![
                Value::symbol("display"),
                Value::list(vec![
                    Value::symbol("space"),
                    Value::keyword("align-to"),
                    Value::fixnum(4),
                ]),
            ]),
        }],
    );

    let row = render_lisp_display_row(rendered, GlyphRowRole::HeaderLine);

    assert_eq!(row.role, GlyphRowRole::HeaderLine);
    assert_eq!(row_text_expanding_stretches(&row), "X   Y");
    assert!(
        row.glyphs[1]
            .iter()
            .any(|glyph| matches!(glyph.glyph_type, GlyphType::Stretch { width_cols: 3 })),
        "multi-char display interval should become one stretch glyph: {:?}",
        row.glyphs[1]
    );
}

#[test]
fn display_row_baseline_header_line_align_to_after_multibyte_prefix_uses_character_offsets() {
    let _eval = Context::new();
    let rendered = Value::string_with_text_properties(
        "λC R",
        vec![neovm_core::emacs_core::value::StringTextPropertyRun {
            start: 2,
            end: 3,
            plist: Value::list(vec![
                Value::symbol("display"),
                Value::list(vec![
                    Value::symbol("space"),
                    Value::keyword("align-to"),
                    Value::fixnum(4),
                ]),
            ]),
        }],
    );

    let row = render_lisp_display_row(rendered, GlyphRowRole::HeaderLine);

    assert_eq!(row.role, GlyphRowRole::HeaderLine);
    assert_eq!(row_text_expanding_stretches(&row), "λC  R");
}

#[test]
fn render_lisp_string_row_uses_face_specific_glyph_widths() {
    let _eval = Context::new();
    let mut engine = crate::engine::LayoutEngine::new();
    let mut renderer = DisplayRowRenderer::new(
        &mut engine.font_metrics,
        DisplayRowMeasurementMode::LogicalCells,
    );
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let mut base_face = resolver.default_face().clone();
    base_face.set_measured_char_width_px(8.0);
    base_face.font_ascent = 12.0;
    let rendered = Value::string_with_text_properties(
        "AB",
        vec![neovm_core::emacs_core::value::StringTextPropertyRun {
            start: 1,
            end: 2,
            plist: Value::list(vec![
                Value::symbol("face"),
                Value::list(vec![
                    Value::keyword("family"),
                    Value::string("JetBrains Mono"),
                    Value::keyword("height"),
                    Value::make_float(2.0),
                ]),
            ]),
        }],
    );
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(1);
    let spec = display_row_request_from_base_face(
        DisplayRowGeometry {
            y: 0.0,
            width: 240.0,
            height: 32.0,
            char_width: 8.0,
            ascent: 12.0,
            tab_policy: crate::display_row::builder::DisplayTabPolicy::every(8),
        },
        &mut face_ids,
        &base_face,
        GlyphRowRole::TabLine,
        std::collections::HashMap::new(),
    );

    let row = render_lisp_string_row(&mut renderer, spec, rendered, &resolver, &mut face_ids)
        .expect("display source row")
        .into_row();
    let glyphs = &row.glyphs[1];

    assert_eq!(glyphs.len(), 2);
    assert!(
        glyphs[1].pixel_width > glyphs[0].pixel_width,
        "face-height run should be measured wider than base run: {glyphs:?}"
    );
}

#[test]
fn display_row_lisp_string_source_request_uses_render_context() {
    let _eval = Context::new();
    let mut font_metrics = None;
    let mut renderer =
        DisplayRowRenderer::new(&mut font_metrics, DisplayRowMeasurementMode::LogicalCells);
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let base_face = resolver.default_face().clone();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(1);
    let request = display_row_request_from_base_face(
        DisplayRowGeometry {
            y: 0.0,
            width: 240.0,
            height: 16.0,
            char_width: 8.0,
            ascent: 12.0,
            tab_policy: crate::display_row::builder::DisplayTabPolicy::every(8),
        },
        &mut face_ids,
        &base_face,
        GlyphRowRole::TabBar,
        std::collections::HashMap::new(),
    );
    let mut context = DisplayRowRenderContext::new(&resolver, None, &mut face_ids);

    let rendered = render_lisp_string_row_with_context(
        &mut renderer,
        request,
        Value::string("ctx"),
        &mut context,
    )
    .expect("rendered context row");

    assert_eq!(rendered.row().role, GlyphRowRole::TabBar);
    assert_eq!(row_text_expanding_stretches(rendered.row()), "ctx");
}

#[test]
fn display_row_render_executor_renders_lisp_string_source_request() {
    let _eval = Context::new();
    let mut font_metrics = None;
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let base_face = resolver.default_face().clone();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(1);
    let request = display_row_request_from_base_face(
        DisplayRowGeometry {
            y: 0.0,
            width: 240.0,
            height: 16.0,
            char_width: 8.0,
            ascent: 12.0,
            tab_policy: crate::display_row::builder::DisplayTabPolicy::every(8),
        },
        &mut face_ids,
        &base_face,
        GlyphRowRole::TabBar,
        std::collections::HashMap::new(),
    );
    let mut executor = DisplayRowRenderExecutor::new(
        &mut font_metrics,
        DisplayRowMeasurementMode::LogicalCells,
        &resolver,
        None,
        &mut face_ids,
    );

    let rendered = executor
        .render_lisp_string_source_request(DisplayRowLispStringSourceRenderRequest::from_value(
            request,
            Value::string("exec"),
            crate::display_source_resolver::DisplaySourceFaceScope::FrameLocal,
        ))
        .expect("executor rendered lisp string row");

    assert_eq!(rendered.row().role, GlyphRowRole::TabBar);
    assert_eq!(row_text_expanding_stretches(rendered.row()), "exec");
}

#[test]
fn display_row_tab_line_wide_char_uses_shared_wide_glyph() {
    let _eval = Context::new();
    let row = render_lisp_display_row(Value::string("A中B"), GlyphRowRole::TabLine);
    let glyphs = &row.glyphs[1];
    let cjk = glyphs
        .iter()
        .find(|glyph| matches!(glyph.glyph_type, GlyphType::Char { ch: '中' }))
        .expect("CJK glyph");

    assert_eq!(row.role, GlyphRowRole::TabLine);
    assert_eq!(row_text_expanding_stretches(&row), "A中B");
    assert!(
        cjk.wide,
        "tab-line CJK should use the shared wide glyph path: {glyphs:?}"
    );
    assert!(
        glyphs.iter().any(|glyph| glyph.padding),
        "tab-line CJK should retain a padding cell like main buffer text: {glyphs:?}"
    );
}

#[test]
fn display_row_tab_line_zwj_emoji_sequence_uses_shared_cluster() {
    let _eval = Context::new();
    let row = render_lisp_display_row(Value::string("👨‍👩"), GlyphRowRole::TabLine);
    let glyphs = &row.glyphs[1];

    assert_eq!(row.role, GlyphRowRole::TabLine);
    assert_eq!(row_text_expanding_stretches(&row), "👨‍👩");
    assert!(
        glyphs
            .iter()
            .any(|glyph| matches!(&glyph.glyph_type, GlyphType::Composite { text } if text.contains('\u{200d}'))),
        "tab-line ZWJ emoji should use the shared cluster path: {glyphs:?}"
    );
}

#[test]
fn display_row_lisp_chrome_roles_share_wide_and_cluster_builder() {
    let _eval = Context::new();

    for role in [
        GlyphRowRole::ModeLine,
        GlyphRowRole::HeaderLine,
        GlyphRowRole::TabLine,
        GlyphRowRole::TabBar,
    ] {
        let row = render_lisp_display_row(Value::string("A中👨‍👩"), role);
        let glyphs = &row.glyphs[1];
        let cjk = glyphs
            .iter()
            .find(|glyph| matches!(glyph.glyph_type, GlyphType::Char { ch: '中' }))
            .expect("CJK glyph");

        assert_eq!(row.role, role);
        assert_eq!(row_text_expanding_stretches(&row), "A中👨‍👩");
        assert!(
            cjk.wide,
            "Lisp-string chrome role {role:?} should use the shared wide-glyph path: {glyphs:?}"
        );
        assert!(
            glyphs.iter().any(|glyph| glyph.padding),
            "Lisp-string chrome role {role:?} should retain CJK padding cells: {glyphs:?}"
        );
        assert!(
            glyphs
                .iter()
                .any(|glyph| matches!(&glyph.glyph_type, GlyphType::Composite { text } if text.contains('\u{200d}'))),
            "Lisp-string chrome role {role:?} should use the shared cluster path: {glyphs:?}"
        );
    }
}

#[test]
fn display_row_tab_line_rtl_text_is_logical_order_at_render() {
    // Bidi reordering now happens exactly once, at row install — NOT during
    // render. A freshly rendered typed (chrome) row is therefore still in
    // LOGICAL order; `reversed_p` is not yet set and the glyphs read "אב".
    // The reorder to visual "בא" is asserted at install by
    // `frame_chrome_rtl_row_reorders_to_visual_order_at_install` and the
    // `builder_reorders_status_line_rtl_row` chrome-install test.
    let _eval = Context::new();
    let row = render_lisp_display_row(Value::string("אב"), GlyphRowRole::TabLine);

    assert_eq!(row.role, GlyphRowRole::TabLine);
    assert!(!row.reversed_p);
    assert_eq!(row_text_expanding_stretches(&row), "אב");
}

#[test]
fn display_row_fragment_keeps_bidi_unfinalized_for_current_row_append() {
    let _eval = Context::new();
    let mut font_metrics = None;
    let mut renderer =
        DisplayRowRenderer::new(&mut font_metrics, DisplayRowMeasurementMode::LogicalCells);
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(1);
    let request = display_row_request_from_base_face(
        DisplayRowGeometry {
            y: 0.0,
            width: 240.0,
            height: 16.0,
            char_width: 8.0,
            ascent: 12.0,
            tab_policy: crate::display_row::builder::DisplayTabPolicy::every(8),
        },
        &mut face_ids,
        resolver.default_face(),
        GlyphRowRole::TabLine,
        std::collections::HashMap::new(),
    );
    let base_face = request.base_face_ref();
    let mut source = crate::display_source::LispStringSourceCursor::new(
        1,
        Value::string("אב"),
        base_face,
        crate::display_source::LispStringSourceOrigin::Normal,
    )
    .expect("lisp string source");
    let mut state = DisplayRowSourceState::frame_local();

    let fragment = request
        .render_fragment_step_with_display_host(
            &mut renderer,
            &mut source,
            &mut state,
            &resolver,
            None,
            &mut face_ids,
        )
        .expect("unfinalized row fragment")
        .into_row();

    assert!(!fragment.reversed_p);
    assert_eq!(row_text_expanding_stretches(&fragment), "אב");

    // A complete source-row render now keeps the row in LOGICAL order too: the
    // single bidi reorder is deferred to install (see
    // `frame_chrome_rtl_row_reorders_to_visual_order_at_install`). Both the
    // fragment path and the complete-row path therefore read logical "אב" here.
    let step_path = render_lisp_display_row(Value::string("אב"), GlyphRowRole::TabLine);
    assert!(!step_path.reversed_p);
    assert_eq!(row_text_expanding_stretches(&step_path), "אב");
}

#[test]
fn display_row_renderer_can_render_source_fragment_into_existing_row() {
    let _eval = Context::new();
    let mut font_metrics = None;
    let mut renderer =
        DisplayRowRenderer::new(&mut font_metrics, DisplayRowMeasurementMode::LogicalCells);
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(1);
    let request = display_row_request_from_base_face(
        DisplayRowGeometry {
            y: 0.0,
            width: 240.0,
            height: 16.0,
            char_width: 8.0,
            ascent: 12.0,
            tab_policy: crate::display_row::builder::DisplayTabPolicy::every(8),
        },
        &mut face_ids,
        resolver.default_face(),
        GlyphRowRole::Text,
        std::collections::HashMap::new(),
    )
    .with_render_bounds(DisplayRowRenderBounds::new(
        DisplayRowPosition::new(8.0, 1),
        DisplayRowMaxX::Bounded(240.0),
    ));
    let base_face_id = request.base_face_id();
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    crate::glyph_row_writer::push_char_to_row(&mut row, 'e', base_face_id, 0, 8.0);
    let mut source = crate::display_source::LispStringSourceCursor::new(
        1,
        Value::string("\u{301}"),
        RenderFaceRef::FaceId(base_face_id),
        crate::display_source::LispStringSourceOrigin::Normal,
    )
    .expect("lisp string source");
    let mut state = DisplayRowSourceState::frame_local();

    let result = request
        .render_fragment_step_into_row_with_display_host(
            &mut renderer,
            &mut row,
            &mut source,
            &mut state,
            &resolver,
            None,
            &mut face_ids,
        )
        .expect("row render fragment");

    assert_eq!(result.stop(), DisplayRowRenderStop::SourceExhausted);
    assert_eq!(
        result.progress(),
        DisplayRowOutputProgress::new(8.0, 1, 0.0, 16.0)
    );
    let text = &row.glyphs[GlyphArea::Text.index()];
    assert_eq!(text.len(), 1);
    assert!(matches!(
        &text[0].glyph_type,
        GlyphType::Composite { text } if text.as_ref() == "e\u{301}"
    ));
}

#[test]
fn mock_current_row_output_install_preserves_row_metadata() {
    let mut row = GlyphRow::new(GlyphRowRole::ModeLine);
    row.enabled = true;
    row.pixel_y = 40.0;
    row.height_px = 18.0;
    row.ascent_px = 13.0;
    row.start_charpos = 7;
    row.end_charpos = 8;
    row.glyphs[GlyphArea::Text.index()].push(Glyph::char('M', FaceId::new(3), 7));

    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    builder.begin_window(1, 2, 10, Rect::new(0.0, 16.0, 80.0, 40.0), true);
    install_display_row(&mut builder, 1, &row);
    builder.end_window();

    let state = builder.finish(10, 2, 8.0, 16.0);
    let installed = &state.window_matrices[0].matrix.rows[1];
    assert_eq!(installed.role, GlyphRowRole::ModeLine);
    assert_eq!(installed.pixel_y, 24.0);
    assert_eq!(installed.height_px, 18.0);
    assert_eq!(installed.ascent_px, 13.0);
    assert_eq!(installed.start_charpos, 7);
    assert_eq!(installed.end_charpos, 8);
    assert!(matches!(
        installed.glyphs[GlyphArea::Text.index()][0].glyph_type,
        GlyphType::Char { ch: 'M' }
    ));
}

#[test]
fn rendered_display_row_materializes_output_row_with_geometry_and_finalization() {
    let mut row = GlyphRow::new(GlyphRowRole::TabLine);
    row.enabled = true;
    row.glyphs[GlyphArea::Text.index()].push(Glyph::char('T', FaceId::new(0), 0));

    let rendered = RenderedDisplayRow::new(
        row,
        DisplayRowOutputProgress::new(8.0, 1, 0.0, 16.0),
        Vec::new(),
        Vec::new(),
    );

    let output = rendered.materialize_output_row(21.0, 18.0, 13.0);

    assert_eq!(output.pixel_y, 21.0);
    assert_eq!(output.height_px, 18.0);
    assert_eq!(output.ascent_px, 13.0);
    assert!(output.displays_text);
    assert!(matches!(
        output.glyphs[GlyphArea::Text.index()][0].glyph_type,
        GlyphType::Char { ch: 'T' }
    ));
}

#[test]
fn measured_display_row_materializes_frame_chrome_local_and_window_relative_rows() {
    let mut row = GlyphRow::new(GlyphRowRole::TabBar);
    row.enabled = true;
    row.height_px = 18.0;
    row.ascent_px = 13.0;
    let measured = MeasuredDisplayRow::new(
        DisplayRowOwner::FrameChrome {
            kind: FrameChromeKind::TabBar,
        },
        3,
        Rect::new(10.0, 40.0, 120.0, 18.0),
        RenderedDisplayRow::new(
            row,
            DisplayRowOutputProgress::new(0.0, 0, 40.0, 18.0),
            Vec::new(),
            Vec::new(),
        ),
        DisplayRowBoundsPolicy::PreserveAllocatedMinimum,
    );

    let frame_chrome = measured.frame_chrome_output_row();
    let relative = measured.window_relative_output_row(Rect::new(0.0, 16.0, 120.0, 80.0));

    assert_eq!(frame_chrome.pixel_y, 0.0);
    assert_eq!(relative.pixel_y, 24.0);
    assert_eq!(frame_chrome.height_px, 18.0);
    assert_eq!(relative.height_px, 18.0);
    assert_eq!(frame_chrome.ascent_px, 13.0);
    assert_eq!(relative.ascent_px, 13.0);
}

/// Regression guard: an RTL (Hebrew) FRAME-CHROME row (e.g. an RTL tab-bar
/// string) must reorder to correct VISUAL order when converted to typed chrome.
///
/// Frame chrome rows do NOT pass through the window-row `Complete` lifecycle —
/// they become a `ChromeDisplayRow` before `FrameChrome` placement. That
/// conversion is their SOLE reorder timing. The rendered row is built in LOGICAL
/// order (only `normalize_external_row` ran; no pre-pass reorder), so if the
/// install-time finalizer ever stops reordering, this Hebrew "אב" would render
/// in logical (wrong) order and the assertions below would fail.
#[test]
fn frame_chrome_rtl_row_reorders_to_visual_order_at_install() {
    // Logical-order Hebrew "אב". This is exactly what a freshly rendered
    // (normalize-only) chrome row carries before install.
    let mut row = GlyphRow::new(GlyphRowRole::TabBar);
    row.enabled = true;
    row.height_px = 18.0;
    row.ascent_px = 13.0;
    row.glyphs[GlyphArea::Text.index()].push(Glyph::char('א', FaceId::new(5), 0));
    row.glyphs[GlyphArea::Text.index()].push(Glyph::char('ב', FaceId::new(5), 1));
    crate::glyph_row_writer::normalize_external_row(&mut row);
    // Sanity: the rendered row is still in LOGICAL order pre-install.
    assert!(!row.reversed_p);
    assert_eq!(
        row.glyphs[GlyphArea::Text.index()][0].glyph_type,
        GlyphType::Char { ch: 'א' }
    );

    let measured = MeasuredDisplayRow::new(
        DisplayRowOwner::FrameChrome {
            kind: FrameChromeKind::TabBar,
        },
        0,
        Rect::new(0.0, 0.0, 160.0, 18.0),
        RenderedDisplayRow::new(
            row,
            DisplayRowOutputProgress::new(0.0, 0, 0.0, 18.0),
            Vec::new(),
            Vec::new(),
        ),
        DisplayRowBoundsPolicy::PreserveAllocatedMinimum,
    );

    let installed_content = frame_chrome_display_row(&measured);

    let installed = installed_content.row();
    let glyphs = &installed.glyphs[GlyphArea::Text.index()];
    // Reordered to visual "בא": the second logical char now comes first.
    assert_eq!(glyphs.len(), 2);
    assert_eq!(glyphs[0].glyph_type, GlyphType::Char { ch: 'ב' });
    assert_eq!(glyphs[1].glyph_type, GlyphType::Char { ch: 'א' });
    assert_eq!(glyphs[0].bidi_level, 1);
    assert_eq!(glyphs[1].bidi_level, 1);
    // RTL paragraph base direction is recorded on the installed row.
    assert!(installed.reversed_p);
}

#[test]
fn install_measured_display_row_clips_window_chrome_media_to_measured_row() {
    let mut row = GlyphRow::new(GlyphRowRole::TabLine);
    row.enabled = true;
    row.pixel_y = 4.0;
    row.height_px = 54.0;
    row.ascent_px = 42.0;
    let mut xwidget = Glyph::stretch(12, FaceId::new(1)).with_pixel_geometry(96.0, 54.0, 42.0);
    xwidget.glyph_type = GlyphType::Xwidget {
        xwidget_id: neomacs_display_protocol::XwidgetId::new(1234),
        webview_id: neomacs_display_protocol::WebViewId::new(5678),
        width_cols: 12,
    };
    row.glyphs[GlyphArea::Text.index()].push(xwidget);
    let rendered = RenderedDisplayRow::new(
        row,
        DisplayRowOutputProgress::new(0.0, 0, 4.0, 54.0),
        Vec::new(),
        Vec::new(),
    );
    let mut builder = crate::output::builder::DisplayOutputBuilder::new();
    let window_bounds = Rect::new(0.0, 0.0, 200.0, 80.0);
    let row_bounds = Rect::new(0.0, 4.0, 200.0, 54.0);
    builder.begin_window_with_text_bounds(
        77,
        1,
        10,
        window_bounds,
        Rect::new(10.0, 20.0, 160.0, 64.0),
        true,
    );

    let measured = MeasuredDisplayRow::new(
        DisplayRowOwner::WindowChrome {
            window_id: 77,
            kind: WindowChromeKind::TabLine,
        },
        0,
        row_bounds,
        rendered,
        DisplayRowBoundsPolicy::PreserveAllocatedMinimum,
    );
    install_measured_window_display_row(&mut builder, &measured);
    builder.end_window();

    let state = builder.finish(10, 1, 8.0, 16.0);
    let materialized = state.materialize();
    let xwidget = materialized
        .glyphs
        .iter()
        .find_map(|glyph| match glyph {
            neomacs_display_protocol::frame_glyphs::FrameGlyph::Xwidget {
                window_id,
                row_role,
                clip_rect,
                slot_id,
                xwidget_id,
                x,
                y,
                width,
                height,
                ..
            } => Some((
                *window_id,
                *row_role,
                *clip_rect,
                *slot_id,
                *xwidget_id,
                *x,
                *y,
                *width,
                *height,
            )),
            _ => None,
        })
        .expect("xwidget materialized from row glyph");
    assert_eq!(xwidget.0.get(), 77);
    assert_eq!(xwidget.1, GlyphRowRole::TabLine);
    assert_eq!(xwidget.2, Some(row_bounds));
    assert_eq!(
        xwidget.3,
        Some(neomacs_display_protocol::frame_glyphs::DisplaySlotId {
            window_id: neomacs_display_protocol::types::DisplayWindowId::new(77),
            row: 0,
            col: 0,
        })
    );
    assert_eq!(xwidget.4.get(), 1234);
    assert_eq!(xwidget.5, 0.0);
    assert_eq!(xwidget.6, 4.0);
    assert_eq!(xwidget.7, 96.0);
    assert_eq!(xwidget.8, 54.0);
}

#[test]
fn measured_display_row_promotes_bounds_from_rendered_row_metrics() {
    let mut row = GlyphRow::new(GlyphRowRole::TabLine);
    row.enabled = true;
    row.height_px = 24.0;
    row.ascent_px = 20.0;
    let measured = MeasuredDisplayRow::new(
        DisplayRowOwner::WindowChrome {
            window_id: 77,
            kind: WindowChromeKind::TabLine,
        },
        0,
        Rect::new(10.0, 6.0, 120.0, 17.0),
        RenderedDisplayRow::new(
            row,
            DisplayRowOutputProgress::new(24.0, 3, 6.0, 24.0),
            Vec::new(),
            Vec::new(),
        ),
        DisplayRowBoundsPolicy::PreserveAllocatedMinimum,
    );

    assert_eq!(measured.bounds().height, 24.0);
    assert_eq!(measured.row_height(), 24.0);
    assert_eq!(measured.row_ascent(), 20.0);
}

#[test]
fn measured_display_row_content_policy_ignores_allocated_row_height() {
    let mut row = GlyphRow::new(GlyphRowRole::TabBar);
    row.enabled = true;
    row.height_px = 120.0;
    row.ascent_px = 13.0;
    let mut face = neomacs_display_protocol::face::Face::default();
    face.id = FaceId::new(8);
    face.font_ascent = 13;
    face.font_descent = 4;
    let image_margins = row
        .intern_image_margins(neomacs_display_protocol::ImageMargins::default())
        .expect("image-margin token");
    let mut image = Glyph::stretch(4, face.id).with_pixel_geometry(32.0, 24.0, 24.0);
    image.glyph_type = GlyphType::Image {
        source_rect: neomacs_display_protocol::ImageSourceRect::FULL,
        image_id: 77,
        width_cols: 4,
        margins: image_margins,
        opaque_background: neomacs_display_protocol::ImageOpaqueBackground::default(),
    };
    row.glyphs[GlyphArea::Text.index()].push(image);
    let measured = MeasuredDisplayRow::new(
        DisplayRowOwner::FrameChrome {
            kind: FrameChromeKind::TabBar,
        },
        0,
        Rect::new(0.0, 0.0, 640.0, 120.0),
        RenderedDisplayRow::new(
            row,
            DisplayRowOutputProgress::new(24.0, 1, 0.0, 120.0),
            Vec::new(),
            vec![face],
        ),
        DisplayRowBoundsPolicy::MeasureContent,
    );

    assert_eq!(measured.bounds().height, 24.0);
    assert_eq!(measured.row_height(), 24.0);
}

/// Complex-script (composed) runs must take the shaped advance as-is: GNU
/// measures compositions by the shaped gstring width, and joined Arabic
/// forms are legitimately narrower than a monospace cell. Clamping them up
/// to the cell re-inflated a Composite cluster to its isolated-forms sum
/// (e.g. السلام 54px measured vs 34px shaped/drawn).
#[test]
fn shaped_complex_script_advances_are_not_cell_clamped() {
    fn shaped(cluster_start: usize, x_advance: f32) -> crate::font::metrics::ShapedGlyph {
        crate::font::metrics::ShapedGlyph {
            font_id: fontdb::ID::dummy(),
            glyph_id: 1,
            x: 0.0,
            y: 0.0,
            x_advance,
            cluster_start,
            cluster_end: cluster_start + 2,
        }
    }

    // "سلا" — three Arabic letters, 2 bytes each; joined forms narrower
    // than the 8.4px cell.
    let measurement =
        crate::display_text_run_measurement::DisplayTextRunMeasurementPlan::from_shaped_glyphs(
            "سلا",
            [shaped(0, 5.0), shaped(2, 3.5), shaped(4, 6.0)],
            8.4,
            8.4,
            GlyphAdvanceQuantization::PreserveLogicalPixels,
            true,
        );

    let crate::display_text_run_measurement::DisplayTextRunMeasurement::Measured(advances) =
        measurement
    else {
        panic!("shaped glyphs should produce measured text-run advances");
    };
    assert_eq!(
        advances
            .iter()
            .map(|advance| advance.advance_px)
            .collect::<Vec<_>>(),
        vec![5.0, 3.5, 6.0],
        "joined complex-script advances must pass through unclamped"
    );
}
