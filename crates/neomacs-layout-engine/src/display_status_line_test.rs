use super::*;

fn window_chrome_test_face(face_resolver: &FaceResolver, origin: &DisplayOrigin) -> ResolvedFace {
    let buffer = neovm_core::buffer::Buffer::new_standalone(
        neovm_core::buffer::BufferId(999),
        Value::string("*chrome-face-test*"),
    );
    let mut next_check = buffer.point_max_char_pos().get();
    face_resolver.default_base_face_for_origin(Some(&buffer), origin, &mut next_check)
}
use crate::display_item::DisplaySourcePosition;
use crate::display_row::DisplayRowFace;
use crate::display_row::builder::DisplayRowGlyphSlot;
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::display_row::render_state::{DisplayRowOutputProgress, RenderedDisplayRow};
use neomacs_display_protocol::frame_chrome::ChromeAction;
use neomacs_display_protocol::frame_glyphs::{DisplaySlotId, FrameGlyph, GlyphRowRole};
use neomacs_display_protocol::glyph_matrix::{Glyph, GlyphArea, GlyphRow, GlyphType};
use neomacs_display_protocol::{
    Color, FaceId, FrameGlyphBuffer, FrameRect, ImageId, PointerDrawMode, PointerImageRelief,
    PointerReliefCornerErase, PointerReliefEdges, PointerReliefMargins,
};
use neovm_core::face::FaceTable;

#[test]
fn display_row_height_for_face_uses_realized_line_height_and_box() {
    let mut font_metrics = None;
    let mut face = ResolvedFace::default();
    face.font_family = "monospace".to_string();
    face.font_size = 14.0;
    face.font_ascent = 9.0;
    face.font_line_height = 12.0;
    face.box_type = 1;
    face.box_line_width = 1.into();

    assert_eq!(
        window_chrome_row_height_for_face(
            &mut font_metrics,
            &face,
            DisplayRowFallbackMetrics::from_default_face_extents(8.0, 20.0, 12.0),
        ),
        14.0
    );
}

#[test]
fn positive_box_width_expands_chrome_in_device_pixels_at_two_x_scale() {
    let mut font_metrics = None;
    let mut face = ResolvedFace::default();
    face.font_family = "monospace".to_string();
    face.font_size = 14.0;
    face.font_ascent = 9.0;
    face.font_line_height = 12.0;
    face.box_type = 1;
    face.box_line_width = 1.into();

    assert_eq!(
        window_chrome_row_height_for_face_at_scale(
            &mut font_metrics,
            &face,
            DisplayRowFallbackMetrics::from_default_face_extents(8.0, 20.0, 12.0),
            neomacs_display_protocol::DeviceScale::new(2.0).unwrap(),
        ),
        13.0,
        "one device pixel above and below is one logical pixel total at 2x"
    );
}

#[test]
fn negative_box_line_width_draws_inside_without_increasing_chrome_row_height() {
    let mut font_metrics = None;
    let mut face = ResolvedFace::default();
    face.font_family = "monospace".to_string();
    face.font_size = 14.0;
    face.font_ascent = 9.0;
    face.font_line_height = 12.0;
    face.box_type = 1;
    face.box_line_width = (-2).into();

    assert_eq!(
        window_chrome_row_height_for_face(
            &mut font_metrics,
            &face,
            DisplayRowFallbackMetrics::from_default_face_extents(8.0, 20.0, 12.0),
        ),
        12.0,
        "GNU :box line-width < 0 paints inside the glyph row"
    );
}

#[test]
fn display_row_metrics_for_smaller_gui_face_do_not_inherit_default_descent() {
    let mut font_metrics = Some(FontMetricsService::new());
    let mut face = ResolvedFace::default();
    face.font_family = "default".to_string();
    face.font_size = 11.5;
    let fallback = DisplayRowFallbackMetrics::from_default_face_extents(13.0, 32.0, 25.0);
    let metrics =
        DisplayRowFaceRealizer::new(&mut font_metrics).row_metrics_for_face(&face, fallback);

    assert!(
        metrics.row_height() < fallback.row_height(),
        "smaller face metrics must not be padded with default descent: {metrics:?}"
    );
}

#[test]
fn chrome_lisp_string_row_request_preserves_policy_inputs() {
    let _eval = Context::new();
    let base_face = ResolvedFace::default();
    let mut symbol_values = std::collections::HashMap::new();
    let align_value = Value::make_int(12);
    symbol_values.insert("align-to".to_string(), align_value);

    let request = DisplayRowLispStringSourceRequest::new(
        DisplayRowGeometry::new(3.0, 80.0, 16.0, 8.0, 12.0, DisplayTabPolicy::every(4)),
        DisplayOrigin::ModeLine { selected: true },
        &base_face,
        Value::string("mode"),
        DisplaySourceFaceScope::FrameLocal,
    )
    .with_symbol_values(symbol_values);
    let geometry = request.geometry();

    assert_eq!(
        request.origin().glyph_row_role(),
        Some(GlyphRowRole::ModeLine)
    );
    assert_eq!(geometry.y(), 3.0);
    assert_eq!(geometry.width(), 80.0);
    assert_eq!(geometry.height(), 16.0);
    assert_eq!(geometry.char_width(), 8.0);
    assert_eq!(geometry.ascent(), 12.0);
    assert_eq!(
        request.symbol_values().get("align-to").copied(),
        Some(align_value)
    );
}

#[test]
fn window_chrome_display_row_request_renders_measured_lifecycle_row() {
    let _eval = Context::new();
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base_face =
        window_chrome_test_face(&face_resolver, &DisplayOrigin::ModeLine { selected: true });
    let mut font_metrics = None;
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(1);
    let mut render_services =
        ChromeRowRenderServices::new(&mut font_metrics, &face_resolver, &mut face_ids);
    let mut symbol_values = std::collections::HashMap::new();
    symbol_values.insert("align-to".to_string(), Value::make_int(12));

    let render = WindowChromeDisplayRowRequest {
        window_id: 42,
        kind: WindowChromeKind::ModeLine,
        selected: true,
        display_row_index: 3,
        output: ChromeRowOutput::new(3, 24.0),
        bounds: neomacs_display_protocol::types::Rect::new(0.0, 24.0, 96.0, 16.0),
        text_area_left_px: 0.0,
        metrics: crate::display_row::metrics::DisplayRowFallbackMetrics::from_default_face_extents(
            8.0, 16.0, 12.0,
        ),
        tab_policy: DisplayTabPolicy::every(4),
        base_face: &base_face,
        symbol_values,
        formatted: ModeLineDisplayOutput::from_root_string(Value::string("mode")),
        face_scope: DisplaySourceFaceScope::FrameLocal,
        image_scale_environment:
            neovm_core::emacs_core::image_catalog::ImageScaleEnvironment::default(),
        tty_glyphless_char_display: Default::default(),
    }
    .into_render_request(render_services.face_ids())
    .render_measured(&mut render_services, None)
    .expect("chrome row should render");

    assert_eq!(render.output, ChromeRowOutput::new(3, 24.0));
    assert_eq!(
        render.measured.owner(),
        DisplayRowOwner::WindowChrome {
            window_id: 42,
            kind: WindowChromeKind::ModeLine,
        }
    );
    assert_eq!(render.measured.row_index(), 3);
    assert_eq!(render.measured.bounds().y, 24.0);
    assert_eq!(render.measured.output_progress().y(), 24.0);
}

#[test]
fn header_line_fills_the_complete_window_width_with_its_base_face() {
    let _eval = Context::new();
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base_face = window_chrome_test_face(
        &face_resolver,
        &DisplayOrigin::HeaderLine { selected: true },
    );
    let mut font_metrics = None;
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(1);
    let mut render_services =
        ChromeRowRenderServices::new(&mut font_metrics, &face_resolver, &mut face_ids);

    let render = WindowChromeDisplayRowRequest {
        window_id: 42,
        kind: WindowChromeKind::HeaderLine,
        selected: true,
        display_row_index: 0,
        output: ChromeRowOutput::new(0, 0.0),
        bounds: neomacs_display_protocol::types::Rect::new(0.0, 0.0, 96.0, 16.0),
        text_area_left_px: 0.0,
        metrics: DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        tab_policy: DisplayTabPolicy::every(4),
        base_face: &base_face,
        symbol_values: std::collections::HashMap::new(),
        formatted: ModeLineDisplayOutput::from_root_string(Value::string("header")),
        face_scope: DisplaySourceFaceScope::FrameLocal,
        image_scale_environment:
            neovm_core::emacs_core::image_catalog::ImageScaleEnvironment::default(),
        tty_glyphless_char_display: Default::default(),
    }
    .into_render_request(render_services.face_ids())
    .render_measured(&mut render_services, None)
    .expect("header line should render");

    let text = &render.measured.rendered().row().glyphs[GlyphArea::Text.index()];
    assert_eq!(
        text.iter().map(|glyph| glyph.pixel_width).sum::<f32>(),
        96.0,
        "GNU display_mode_line fills every header-line column"
    );
    let fill = text.last().expect("header-line trailing fill");
    assert!(matches!(fill.glyph_type, GlyphType::Stretch { .. }));
    assert_eq!(
        fill.face_id,
        text.first().expect("header text glyph").face_id,
        "the trailing fill must use the realized buffer-remapped header-line face"
    );
}

fn proportional_chrome_test_face(
    face_resolver: &FaceResolver,
    origin: &DisplayOrigin,
) -> ResolvedFace {
    let mut face = window_chrome_test_face(face_resolver, origin);
    face.font_family = "Noto Sans".to_string();
    face.font_size = 9.12871;
    face.font_weight = 400;
    face.set_measured_char_width_px(7.2);
    face.font_ascent = 10.0;
    face.font_line_height = 17.0;
    face
}

fn assert_matches_proportional_dot_width(actual: f32, label: &str) {
    let mut metrics = FontMetricsService::new();
    let expected = metrics.char_width('.', "Noto Sans", 400, false, 9.12871);
    assert!(
        expected > 0.0 && expected < 7.2,
        "test requires Noto Sans dot to be narrower than the fallback cell, got {expected}"
    );
    assert!(
        (actual - expected).abs() < 0.25,
        "{label} should use the GUI font-backed glyph advance for '.', got {actual}, expected {expected}"
    );
}

#[test]
fn window_chrome_gui_tab_and_mode_lines_use_font_backed_glyph_advances() {
    let _eval = Context::new();
    let table = FaceTable::new();
    let face_resolver =
        FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, Some("neo".to_string()));

    for (kind, origin, selected, label) in [
        (
            WindowChromeKind::TabLine,
            DisplayOrigin::TabLine,
            true,
            "tab-line",
        ),
        (
            WindowChromeKind::ModeLine,
            DisplayOrigin::ModeLine { selected: true },
            true,
            "mode-line",
        ),
    ] {
        let base_face = proportional_chrome_test_face(&face_resolver, &origin);
        let mut font_metrics = Some(FontMetricsService::new());
        let mut face_ids = FrameFaceAttempt::for_test_with_next_id(1);
        let mut render_services =
            ChromeRowRenderServices::new(&mut font_metrics, &face_resolver, &mut face_ids);

        let render = WindowChromeDisplayRowRequest {
            window_id: 42,
            kind,
            selected,
            display_row_index: 0,
            output: ChromeRowOutput::new(0, 0.0),
            bounds: neomacs_display_protocol::types::Rect::new(0.0, 0.0, 240.0, 17.0),
            text_area_left_px: 0.0,
            metrics: DisplayRowFallbackMetrics::from_default_face_extents(7.2, 17.0, 10.0),
            tab_policy: DisplayTabPolicy::every(8),
            base_face: &base_face,
            symbol_values: std::collections::HashMap::new(),
            formatted: ModeLineDisplayOutput::from_root_string(Value::string(".agent-sh")),
            face_scope: DisplaySourceFaceScope::FrameLocal,
            image_scale_environment:
                neovm_core::emacs_core::image_catalog::ImageScaleEnvironment::default(),
            tty_glyphless_char_display: Default::default(),
        }
        .into_render_request(render_services.face_ids())
        .render_measured(&mut render_services, None)
        .expect("window chrome row should render");
        let first_width =
            render.measured.rendered().row().glyphs[GlyphArea::Text.index()][0].pixel_width;

        assert_matches_proportional_dot_width(first_width, label);
    }
}

#[test]
fn frame_tab_bar_gui_uses_font_backed_glyph_advances() {
    let _eval = Context::new();
    let table = FaceTable::new();
    let face_resolver =
        FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, Some("neo".to_string()));
    let base_face = proportional_chrome_test_face(&face_resolver, &DisplayOrigin::TabBar);
    let mut font_metrics = Some(FontMetricsService::new());
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(1);
    let mut render_services =
        ChromeRowRenderServices::new(&mut font_metrics, &face_resolver, &mut face_ids);

    let measured = FrameTabBarDisplayRowRequest {
        row_index: 0,
        y: 0.0,
        width: 240.0,
        height: 17.0,
        metrics: DisplayRowFallbackMetrics::from_default_face_extents(7.2, 17.0, 10.0),
        base_face: &base_face,
        text: Value::string(".agent-sh"),
        image_scale_environment:
            neovm_core::emacs_core::image_catalog::ImageScaleEnvironment::default(),
    }
    .into_chrome_render_request(render_services.face_ids())
    .render_row(&mut render_services, None)
    .expect("frame tab-bar row should render")
    .measure();
    let first_width = measured.rendered().row().glyphs[GlyphArea::Text.index()][0].pixel_width;

    assert_matches_proportional_dot_width(first_width, "tab-bar");
}

#[test]
fn frame_tab_bar_fills_the_complete_frame_width_with_its_base_face() {
    let _eval = Context::new();
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base_face = window_chrome_test_face(&face_resolver, &DisplayOrigin::TabBar);
    let mut font_metrics = None;
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(1);
    let mut render_services =
        ChromeRowRenderServices::new(&mut font_metrics, &face_resolver, &mut face_ids);

    let measured = FrameTabBarDisplayRowRequest {
        row_index: 0,
        y: 0.0,
        width: 96.0,
        height: 16.0,
        metrics: DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        base_face: &base_face,
        text: Value::string("tab"),
        image_scale_environment:
            neovm_core::emacs_core::image_catalog::ImageScaleEnvironment::default(),
    }
    .into_chrome_render_request(render_services.face_ids())
    .render_row(&mut render_services, None)
    .expect("frame tab-bar row should render")
    .measure();

    let text = &measured.rendered().row().glyphs[GlyphArea::Text.index()];
    assert_eq!(
        text.iter().map(|glyph| glyph.pixel_width).sum::<f32>(),
        96.0,
        "GNU display_tab_bar extends the tab-bar face through the frame edge"
    );
    let fill = text.last().expect("frame tab-bar trailing fill");
    assert!(matches!(fill.glyph_type, GlyphType::Stretch { .. }));
    assert_eq!(
        fill.face_id,
        text.first().expect("tab-bar text glyph").face_id,
        "the trailing fill must carry the realized tab-bar base face"
    );
}

#[test]
fn tab_bar_hit_regions_preserve_body_close_and_add_item_meaning() {
    let mut eval = Context::new();
    eval.setup_thread_locals();
    let presentation = eval.begin_interaction_presentation();
    let slots = vec![
        DisplayRowGlyphSlot::new(DisplaySourcePosition::lisp_string(1, 0, 0), 0.0, 0, 4.0, 1),
        DisplayRowGlyphSlot::new(DisplaySourcePosition::lisp_string(1, 1, 1), 4.0, 1, 6.0, 1),
        DisplayRowGlyphSlot::new(DisplaySourcePosition::lisp_string(1, 2, 2), 10.0, 2, 8.0, 1),
        DisplayRowGlyphSlot::new(DisplaySourcePosition::lisp_string(1, 3, 3), 18.0, 3, 5.0, 1),
    ];
    let rendered = RenderedDisplayRow::new(
        GlyphRow::new(GlyphRowRole::TabBar),
        DisplayRowOutputProgress::new(23.0, 4, 0.0, 18.0),
        slots,
        Vec::new(),
    );
    let text = Value::string_with_text_properties(
        "abcd",
        vec![neovm_core::emacs_core::value::StringTextPropertyRun {
            start: 1,
            end: 2,
            plist: Value::list(vec![Value::symbol("close-tab"), Value::T]),
        }],
    );
    let items = vec![
        TabBarSourceItem {
            caption: Value::string("ab"),
            key: Value::symbol("tab-1"),
            binding: Value::symbol("tab-bar-select-tab"),
            enabled: true,
            char_range: 0..2,
        },
        TabBarSourceItem {
            caption: Value::string("cd"),
            key: Value::symbol("add-tab"),
            binding: Value::symbol("tab-bar-new-tab"),
            enabled: true,
            char_range: 2..4,
        },
    ];

    let slots = tab_bar_pointer_slot_plan(&mut eval, &rendered, text, &items);
    let plan = tab_bar_presented_pointer_plan(
        &mut eval,
        presentation,
        &slots,
        &items,
        18.0,
        TabBarPointerAppearanceStyle::new(
            test_tab_pointer_relief(false),
            test_tab_pointer_relief(true),
        ),
        &[],
        &[],
    );
    let regions = plan.hit_regions();

    assert_eq!(regions[0].local_bounds().raw().x, 0.0);
    assert_eq!(regions[0].local_bounds().raw().width, 4.0);
    assert_eq!(regions[1].local_bounds().raw().x, 4.0);
    assert_eq!(regions[1].local_bounds().raw().width, 6.0);
    assert_eq!(regions[2].local_bounds().raw().x, 10.0);
    assert_eq!(regions[2].local_bounds().raw().width, 13.0);

    let resolve = |region: &ChromeHitRegion| {
        let ChromeAction::Presented { interaction } = region.action() else {
            panic!("tab hit must be an opaque presented target")
        };
        eval.resolve_presented_mouse_target(presentation, interaction.get())
            .expect("registered target")
    };
    let body = resolve(&regions[0]);
    let close = resolve(&regions[1]);
    let add = resolve(&regions[2]);
    assert_eq!(
        presented_menu_item(&mut eval, body.posn_string),
        "(tab-1 tab-bar-select-tab nil)"
    );
    assert_eq!(
        presented_menu_item(&mut eval, close.posn_string),
        "(tab-1 tab-bar-select-tab t)"
    );
    assert_eq!(
        presented_menu_item(&mut eval, add.posn_string),
        "(add-tab tab-bar-new-tab nil)"
    );
}

fn test_tab_pointer_relief(raised: bool) -> PointerImageRelief {
    let light = Color::new(0.8, 0.8, 0.8, 1.0);
    let dark = Color::new(0.2, 0.2, 0.2, 1.0);
    PointerImageRelief::new(
        if raised { light } else { dark },
        if raised { dark } else { light },
        1.0,
        PointerReliefMargins::new(1.0, 1.0, 1.0, 1.0),
        PointerReliefEdges::new(true, true, true, true),
        PointerReliefCornerErase::new(Color::BLACK, 6.0, 1.0),
    )
}

#[test]
fn tab_bar_pointer_appearance_resolves_gnu_pgtk_relief_parameters() {
    let style = gnu_tab_bar_pointer_appearance_style(0x00808080, 0x00112233, 3.0, 2.0, 4.0);
    let raised = style.raised;
    let sunken = style.sunken;

    assert_eq!(raised.thickness(), 4.0);
    assert_eq!(
        raised.margins(),
        PointerReliefMargins::new(3.0, 2.0, 3.0, 2.0)
    );
    assert_eq!(
        raised.edges(),
        PointerReliefEdges::new(true, true, true, true)
    );
    assert_eq!(raised.corner_erase().radius(), 6.0);
    assert_eq!(raised.corner_erase().margin(), 1.0);
    assert_eq!(raised.corner_erase().color(), Color::from_pixel(0x00112233));
    assert_eq!(raised.top_left_color(), sunken.bottom_right_color());
    assert_eq!(raised.bottom_right_color(), sunken.top_left_color());
    assert_ne!(raised.top_left_color(), raised.bottom_right_color());
}

#[test]
fn image_relief_background_uses_gnu_box_image_face_precedence() {
    assert_eq!(
        gnu_image_relief_background(Some(Color::RED), Some(0x00_22_33_44), Color::BLUE),
        0x00_ff_00_00,
        "a face box supplies GNU's shadow color source first",
    );
    assert_eq!(
        gnu_image_relief_background(None, Some(0x00_22_33_44), Color::BLUE),
        0x00_22_33_44,
        "an opaque image background wins when the face has no box shadow source",
    );
    assert_eq!(
        gnu_image_relief_background(None, None, Color::BLUE),
        0x00_00_00_ff,
        "transparent images fall back to the glyph face background",
    );
}

#[test]
fn tab_bar_image_relief_styles_resolve_color_source_per_image_slot() {
    let face_background = FaceId::new(40);
    let face_box = FaceId::new(41);
    let mut row = GlyphRow::new(GlyphRowRole::TabBar);
    let image_margins = row
        .intern_image_margins(neomacs_display_protocol::ImageMargins::default())
        .expect("image-margin token");
    let image = |image_id, face_id, opaque_background| {
        let mut glyph = Glyph::stretch(1, face_id).with_pixel_geometry(8.0, 8.0, 8.0);
        glyph.glyph_type = GlyphType::Image {
            source_rect: neomacs_display_protocol::ImageSourceRect::FULL,
            image_id,
            width_cols: 1,
            margins: image_margins,
            opaque_background: neomacs_display_protocol::ImageOpaqueBackground::new(
                opaque_background,
            ),
        };
        glyph
    };
    row.glyphs[GlyphArea::Text.index()] = vec![
        image(0, face_background, Some(0x12_34_56)),
        image(1, face_box, Some(0x22_33_44)),
        image(2, face_background, None),
    ];
    let mut background = neomacs_display_protocol::face::Face::default();
    background.id = face_background;
    background.background = Color::BLUE;
    let mut boxed = background.clone();
    boxed.id = face_box;
    boxed.box_color = Some(Color::RED);
    let rendered = RenderedDisplayRow::new(
        row,
        DisplayRowOutputProgress::new(24.0, 3, 0.0, 18.0),
        Vec::new(),
        vec![background, boxed],
    );

    let styles = tab_bar_image_relief_styles(&rendered, 0, 0xaa_bb_cc, 2.0, 3.0, 1.0);
    assert_eq!(
        styles[0].1,
        gnu_tab_bar_pointer_appearance_style(0x12_34_56, 0xaa_bb_cc, 2.0, 3.0, 1.0)
    );
    assert_eq!(
        styles[1].1,
        gnu_tab_bar_pointer_appearance_style(0xff_00_00, 0xaa_bb_cc, 2.0, 3.0, 1.0)
    );
    assert_eq!(
        styles[2].1,
        gnu_tab_bar_pointer_appearance_style(0x00_00_ff, 0xaa_bb_cc, 2.0, 3.0, 1.0)
    );
}

#[test]
fn tab_bar_image_relief_uses_glyph_at_visual_column_after_wide_stretch() {
    let fallback_face = FaceId::new(50);
    let image_face = FaceId::new(51);
    let mut row = GlyphRow::new(GlyphRowRole::TabBar);
    let image_margins = row
        .intern_image_margins(neomacs_display_protocol::ImageMargins::default())
        .expect("image-margin token");
    let mut image_glyph = Glyph::stretch(1, image_face).with_pixel_geometry(8.0, 8.0, 8.0);
    image_glyph.glyph_type = GlyphType::Image {
        source_rect: neomacs_display_protocol::ImageSourceRect::FULL,
        image_id: 1,
        width_cols: 1,
        margins: image_margins,
        opaque_background: neomacs_display_protocol::ImageOpaqueBackground::default(),
    };
    row.glyphs[GlyphArea::Text.index()] = vec![Glyph::stretch(3, fallback_face), image_glyph];

    let mut fallback = neomacs_display_protocol::face::Face::default();
    fallback.id = fallback_face;
    fallback.background = Color::BLUE;
    let mut image = fallback.clone();
    image.id = image_face;
    image.box_color = Some(Color::RED);

    let rendered = RenderedDisplayRow::new(
        row,
        DisplayRowOutputProgress::new(32.0, 4, 0.0, 18.0),
        Vec::new(),
        vec![fallback, image],
    );

    let styles = tab_bar_image_relief_styles(&rendered, 0, 0xaa_bb_cc, 1.0, 1.0, 1.0);

    assert_eq!(styles.len(), 1);
    assert_eq!(
        styles[0].1,
        gnu_tab_bar_pointer_appearance_style(0xff_00_00, 0xaa_bb_cc, 1.0, 1.0, 1.0)
    );
}

#[test]
fn tab_bar_pointer_appearance_preserves_configured_zero_relief() {
    let style = gnu_tab_bar_pointer_appearance_style(0x00808080, 0x00112233, 1.0, 1.0, 0.0);

    assert_eq!(style.raised.thickness(), 0.0);
    assert_eq!(style.sunken.thickness(), 0.0);
}

fn tab_pointer_test_frame(highlight_face: FaceId) -> FrameGlyphBuffer {
    let mut frame = FrameGlyphBuffer::with_size(80.0, 18.0);
    frame.set_face(
        highlight_face,
        Color::BLACK,
        Some(Color::WHITE),
        400,
        false,
        0,
        None,
        0,
        None,
        0,
        None,
    );
    frame.set_draw_context(
        neomacs_display_protocol::DisplayWindowId::new(0),
        GlyphRowRole::TabBar,
        Some(neomacs_display_protocol::Rect::new(0.0, 0.0, 80.0, 18.0)),
    );
    frame.add_char('a', 0.0, 0.0, 8.0, 18.0, 14.0, false);
    frame.add_char(' ', 8.0, 0.0, 8.0, 18.0, 14.0, false);
    frame.glyphs.push(FrameGlyph::Image {
        source_rect: neomacs_display_protocol::ImageSourceRect::FULL,
        window_id: neomacs_display_protocol::DisplayWindowId::new(0),
        row_role: GlyphRowRole::TabBar,
        clip_rect: Some(neomacs_display_protocol::Rect::new(0.0, 0.0, 80.0, 18.0)),
        slot_id: Some(DisplaySlotId {
            window_id: neomacs_display_protocol::DisplayWindowId::new(0),
            row: 0,
            col: 1,
        }),
        image_id: ImageId::new(7),
        slot_rect: neomacs_display_protocol::Rect::new(8.0, 0.0, 8.0, 18.0),
        box_rect: neomacs_display_protocol::Rect::new(8.0, 0.0, 8.0, 18.0),
        x: 8.0,
        y: 1.0,
        width: 8.0,
        height: 16.0,
        face_id: FaceId::new(0),
        box_vertical_edges: Default::default(),
    });
    frame
}

#[test]
fn tab_bar_pointer_appearance_body_and_close_share_whole_tab_mouse_face() {
    let mut eval = Context::new();
    eval.setup_thread_locals();
    let presentation = eval.begin_interaction_presentation();
    let rendered = RenderedDisplayRow::new(
        GlyphRow::new(GlyphRowRole::TabBar),
        DisplayRowOutputProgress::new(16.0, 2, 0.0, 18.0),
        vec![
            DisplayRowGlyphSlot::new(DisplaySourcePosition::lisp_string(1, 0, 0), 0.0, 0, 8.0, 1),
            DisplayRowGlyphSlot::new(DisplaySourcePosition::lisp_string(1, 1, 1), 8.0, 1, 8.0, 1),
        ],
        Vec::new(),
    );
    let caption = Value::string_with_text_properties(
        "a ",
        vec![neovm_core::emacs_core::value::StringTextPropertyRun {
            start: 0,
            end: 2,
            plist: Value::list(vec![
                Value::symbol("mouse-face"),
                Value::symbol("tab-bar-tab-highlight"),
            ]),
        }],
    );
    eval.eval_form(Value::list(vec![
        Value::symbol("put-text-property"),
        Value::fixnum(1),
        Value::fixnum(2),
        quoted(Value::symbol("close-tab")),
        Value::T,
        quoted(caption),
    ]))
    .unwrap();
    let text = eval
        .eval_form(Value::list(vec![
            Value::symbol("copy-sequence"),
            quoted(caption),
        ]))
        .unwrap();
    let items = vec![TabBarSourceItem {
        caption,
        key: Value::symbol("tab-1"),
        binding: Value::symbol("tab-bar-select-tab"),
        enabled: true,
        char_range: 0..2,
    }];
    let highlight = FaceId::new(33);
    let slots = tab_bar_pointer_slot_plan(&mut eval, &rendered, text, &items);
    let plan = tab_bar_presented_pointer_plan(
        &mut eval,
        presentation,
        &slots,
        &items,
        18.0,
        TabBarPointerAppearanceStyle::new(
            test_tab_pointer_relief(true),
            test_tab_pointer_relief(false),
        ),
        &[],
        &[(Value::symbol("tab-bar-tab-highlight"), highlight)],
    );
    let mut frame = tab_pointer_test_frame(highlight);
    plan.install_into(&mut frame, FrameRect::new(0.0, 0.0, 80.0, 18.0).unwrap())
        .unwrap();

    let [body, close] = frame.presented_pointer().regions() else {
        panic!("the plan publishes one region for the tab body and one for its close box");
    };
    assert_ne!(body.interaction(), close.interaction());
    assert_eq!(body.appearance(), close.appearance());
    let appearance = frame
        .presented_pointer()
        .appearance(body.appearance().unwrap())
        .unwrap();
    assert_eq!(appearance.paint_spans().len(), 1);
    assert_eq!(appearance.paint_spans()[0].len(), 2);
    assert!(
        appearance
            .paint_spans()
            .iter()
            .all(|span| span.kind() == PresentedPrimitiveKind::Glyph),
        "GNU DRAW_MOUSE_FACE applies the whole-tab face to the replacement image's backing glyph; the close image pixels remain the normal image",
    );
    assert!(
        frame
            .glyphs
            .iter()
            .any(|glyph| matches!(glyph, FrameGlyph::Image { .. }))
    );
    assert_eq!(appearance.hover(), PointerDrawMode::Face(highlight));
    assert_eq!(appearance.pressed(), PointerDrawMode::Face(highlight));
}

#[test]
fn tab_bar_pointer_appearance_add_image_uses_resolved_raised_and_sunken_relief() {
    let mut eval = Context::new();
    eval.setup_thread_locals();
    let presentation = eval.begin_interaction_presentation();
    let rendered = RenderedDisplayRow::new(
        GlyphRow::new(GlyphRowRole::TabBar),
        DisplayRowOutputProgress::new(16.0, 2, 0.0, 18.0),
        vec![DisplayRowGlyphSlot::new(
            DisplaySourcePosition::lisp_string(1, 0, 0),
            8.0,
            1,
            8.0,
            1,
        )],
        Vec::new(),
    );
    let caption = Value::string(" ");
    let items = vec![TabBarSourceItem {
        caption,
        key: Value::symbol("add-tab"),
        binding: Value::symbol("tab-bar-new-tab"),
        enabled: true,
        char_range: 0..1,
    }];
    let raised = test_tab_pointer_relief(true);
    let sunken = test_tab_pointer_relief(false);
    let highlight = FaceId::new(33);
    let slots = tab_bar_pointer_slot_plan(&mut eval, &rendered, caption, &items);
    let plan = tab_bar_presented_pointer_plan(
        &mut eval,
        presentation,
        &slots,
        &items,
        18.0,
        TabBarPointerAppearanceStyle::new(raised, sunken),
        &[],
        &[],
    );
    let mut frame = tab_pointer_test_frame(highlight);
    frame.height = 54.0;
    for glyph in &mut frame.glyphs {
        match glyph {
            FrameGlyph::Char { slot_id, .. } | FrameGlyph::Stretch { slot_id, .. } => {
                slot_id.row = 2;
            }
            FrameGlyph::Image {
                slot_id: Some(slot_id),
                ..
            } => slot_id.row = 2,
            _ => {}
        }
    }
    let source = plan
        .into_source_map(FrameRect::new(0.0, 36.0, 80.0, 18.0).unwrap(), 2)
        .unwrap();
    frame.install_presented_pointer_source_map(&source).unwrap();

    let [hit] = frame.presented_pointer().regions() else {
        panic!("one tab item publishes one pointer region");
    };
    let appearance = frame
        .presented_pointer()
        .appearance(hit.appearance().unwrap())
        .unwrap();
    assert_eq!(appearance.paint_spans().len(), 1);
    assert_eq!(appearance.hover(), PointerDrawMode::ImageRelief(raised));
    assert_eq!(appearance.pressed(), PointerDrawMode::ImageRelief(sunken));
    let posn_string = eval
        .resolve_presented_mouse_target(presentation, hit.interaction().unwrap().get())
        .unwrap()
        .posn_string;
    assert_eq!(
        presented_menu_item(&mut eval, posn_string),
        "(add-tab tab-bar-new-tab nil)"
    );
}

fn presented_menu_item(eval: &mut Context, posn_string: Value) -> String {
    eval.eval_form(Value::list(vec![
        Value::symbol("prin1-to-string"),
        Value::list(vec![
            Value::symbol("get-text-property"),
            Value::fixnum(0),
            Value::list(vec![Value::symbol("quote"), Value::symbol("menu-item")]),
            Value::list(vec![
                Value::symbol("car"),
                Value::list(vec![Value::symbol("quote"), posn_string]),
            ]),
        ]),
    ]))
    .expect("inspect menu-item")
    .as_runtime_string_owned()
    .expect("printed menu-item")
}

#[test]
fn built_tab_bar_preserves_concatenated_caption_ranges() {
    let mut eval = Context::new();
    eval.setup_thread_locals();
    let source = TabBarDisplaySource {
        entries: vec![
            TabBarDisplayEntry {
                caption: Value::string("ab"),
                key: Value::symbol("tab-1"),
                binding: Value::symbol("ignore"),
                enabled: true,
            },
            TabBarDisplayEntry {
                caption: Value::string("cde"),
                key: Value::symbol("add-tab"),
                binding: Value::symbol("tab-bar-new-tab"),
                enabled: true,
            },
        ],
    };

    let built = source.into_built_tab_bar(&mut eval).expect("built tab bar");

    assert_eq!(built.source_items[0].char_range, 0..2);
    assert_eq!(built.source_items[1].char_range, 2..5);
}

/// A mode-line whose text carries a tall `display` element (here a glyph with
/// `(display (height 2.0))`, the same shape doom-modeline's bar uses) must
/// produce a measured row height taller than the bare font/char height — GNU's
/// `display_mode_line` returns the row's max ascent+descent. The single-layout
/// fix reserves this measured height for the mode line instead of the fixed
/// face height, and reuses the same built row at render time.
#[test]
fn window_chrome_mode_line_row_grows_for_tall_display_element() {
    let _eval = Context::new();
    let table = FaceTable::new();
    let face_resolver = FaceResolver::new(&table, 0x00ffffff, 0x000000, 14.0, None);
    let base_face =
        window_chrome_test_face(&face_resolver, &DisplayOrigin::ModeLine { selected: true });
    let mut font_metrics = None;
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(1);
    let mut render_services =
        ChromeRowRenderServices::new(&mut font_metrics, &face_resolver, &mut face_ids);

    // Allocated bounds use a 16px char height (the face estimate).
    let allocated_height = 16.0_f32;

    // Plain mode line: measured height stays at the allocated/char height.
    let plain = WindowChromeDisplayRowRequest {
        window_id: 7,
        kind: WindowChromeKind::ModeLine,
        selected: true,
        display_row_index: 1,
        output: ChromeRowOutput::new(1, 0.0),
        bounds: neomacs_display_protocol::types::Rect::new(0.0, 0.0, 240.0, allocated_height),
        text_area_left_px: 0.0,
        metrics: DisplayRowFallbackMetrics::from_default_face_extents(8.0, allocated_height, 12.0),
        tab_policy: DisplayTabPolicy::every(8),
        base_face: &base_face,
        symbol_values: std::collections::HashMap::new(),
        formatted: ModeLineDisplayOutput::from_root_string(Value::string("AB")),
        face_scope: DisplaySourceFaceScope::FrameLocal,
        image_scale_environment:
            neovm_core::emacs_core::image_catalog::ImageScaleEnvironment::default(),
        tty_glyphless_char_display: Default::default(),
    }
    .into_render_request(render_services.face_ids())
    .render_measured(&mut render_services, None)
    .expect("plain mode-line row should render");
    assert_eq!(
        plain.measured.row_height(),
        allocated_height,
        "plain mode line stays at the face/char height"
    );

    // Mode line with a tall display element on the 'B' glyph.
    let tall_text = Value::string_with_text_properties(
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
    let tall = WindowChromeDisplayRowRequest {
        window_id: 7,
        kind: WindowChromeKind::ModeLine,
        selected: true,
        display_row_index: 1,
        output: ChromeRowOutput::new(1, 0.0),
        bounds: neomacs_display_protocol::types::Rect::new(0.0, 0.0, 240.0, allocated_height),
        text_area_left_px: 0.0,
        metrics: DisplayRowFallbackMetrics::from_default_face_extents(8.0, allocated_height, 12.0),
        tab_policy: DisplayTabPolicy::every(8),
        base_face: &base_face,
        symbol_values: std::collections::HashMap::new(),
        formatted: ModeLineDisplayOutput::from_root_string(tall_text),
        face_scope: DisplaySourceFaceScope::FrameLocal,
        image_scale_environment:
            neovm_core::emacs_core::image_catalog::ImageScaleEnvironment::default(),
        tty_glyphless_char_display: Default::default(),
    }
    .into_render_request(render_services.face_ids())
    .render_measured(&mut render_services, None)
    .expect("tall mode-line row should render");

    assert!(
        tall.measured.row_height() > allocated_height,
        "tall display element must grow the mode-line row beyond the face/char height \
         (got {} <= {})",
        tall.measured.row_height(),
        allocated_height
    );
}

#[test]
fn tab_bar_display_source_extracts_menu_items_until_nested_keymap() {
    let mut eval = Context::new();
    let keymap = Value::list(vec![
        KeymapMarker::Keymap.symbol_value(),
        Value::list(vec![
            Value::symbol("current-tab"),
            KeymapMarker::MenuItem.symbol_value(),
            Value::string("One"),
            Value::symbol("select-one"),
        ]),
        Value::cons(
            Value::symbol("next-tab"),
            Value::list(vec![
                KeymapMarker::MenuItem.symbol_value(),
                Value::string("Two"),
                Value::symbol("select-two"),
                Value::keyword(":enable"),
                Value::NIL,
            ]),
        ),
        Value::list(vec![KeymapMarker::Keymap.symbol_value()]),
        Value::list(vec![
            Value::symbol("after-nested-map"),
            KeymapMarker::MenuItem.symbol_value(),
            Value::string("After"),
        ]),
    ]);

    let source = TabBarDisplaySource::from_keymap(&mut eval, keymap).expect("tab-bar source");

    assert_eq!(
        source
            .entries
            .iter()
            .map(|entry| entry.caption.as_runtime_string_owned().unwrap())
            .collect::<Vec<_>>(),
        vec!["One".to_string(), "Two".to_string()]
    );
    assert_eq!(
        source
            .entries
            .iter()
            .map(|entry| entry.key)
            .collect::<Vec<_>>(),
        vec![Value::symbol("current-tab"), Value::symbol("next-tab")]
    );
    assert_eq!(
        source
            .entries
            .iter()
            .map(|entry| entry.binding)
            .collect::<Vec<_>>(),
        vec![Value::symbol("select-one"), Value::symbol("select-two")]
    );
    assert_eq!(
        source
            .entries
            .iter()
            .map(|entry| entry.enabled)
            .collect::<Vec<_>>(),
        vec![true, false]
    );
}

#[test]
fn tab_bar_pointer_appearance_uses_each_effective_mouse_face_and_skips_invalid_face() {
    let mut eval = Context::new();
    eval.setup_thread_locals();
    let presentation = eval.begin_interaction_presentation();
    let rendered = RenderedDisplayRow::new(
        GlyphRow::new(GlyphRowRole::TabBar),
        DisplayRowOutputProgress::new(24.0, 3, 0.0, 18.0),
        vec![
            DisplayRowGlyphSlot::new(DisplaySourcePosition::lisp_string(1, 0, 0), 0.0, 0, 8.0, 1),
            DisplayRowGlyphSlot::new(DisplaySourcePosition::lisp_string(1, 1, 1), 8.0, 1, 8.0, 1),
            DisplayRowGlyphSlot::new(DisplaySourcePosition::lisp_string(1, 2, 2), 16.0, 2, 8.0, 1),
        ],
        Vec::new(),
    );
    let text = Value::string_with_text_properties(
        "abc",
        vec![
            neovm_core::emacs_core::value::StringTextPropertyRun {
                start: 0,
                end: 1,
                plist: Value::list(vec![Value::symbol("mouse-face"), Value::symbol("custom-a")]),
            },
            neovm_core::emacs_core::value::StringTextPropertyRun {
                start: 1,
                end: 2,
                plist: Value::list(vec![Value::symbol("mouse-face"), Value::symbol("custom-b")]),
            },
            neovm_core::emacs_core::value::StringTextPropertyRun {
                start: 2,
                end: 3,
                plist: Value::list(vec![Value::symbol("mouse-face"), Value::fixnum(99)]),
            },
        ],
    );
    let items = [TabBarSourceItem {
        caption: text,
        key: Value::symbol("tab-1"),
        binding: Value::symbol("ignore"),
        char_range: 0..3,
        enabled: true,
    }];
    let face_a = FaceId::new(40);
    let face_b = FaceId::new(41);
    let slots = tab_bar_pointer_slot_plan(&mut eval, &rendered, text, &items);
    let plan = tab_bar_presented_pointer_plan(
        &mut eval,
        presentation,
        &slots,
        &items,
        18.0,
        TabBarPointerAppearanceStyle::new(
            test_tab_pointer_relief(true),
            test_tab_pointer_relief(false),
        ),
        &[],
        &[
            (Value::symbol("custom-a"), face_a),
            (Value::symbol("custom-b"), face_b),
        ],
    );
    let source = plan
        .into_source_map(FrameRect::new(0.0, 0.0, 80.0, 18.0).unwrap(), 0)
        .unwrap();

    assert_eq!(source.appearances().len(), 2);
    assert_eq!(
        source.appearances()[0].hover(),
        PointerDrawMode::Face(face_a)
    );
    assert_eq!(
        source.appearances()[1].hover(),
        PointerDrawMode::Face(face_b)
    );
    assert!(source.regions()[2].appearance().is_none());
    assert!(source.regions()[2].interaction().is_some());
}

#[test]
fn tab_bar_mouse_face_coalesces_adjacent_wide_source_characters_not_display_columns() {
    let mut eval = Context::new();
    eval.setup_thread_locals();
    let text = Value::string_with_text_properties(
        "中👨",
        vec![neovm_core::emacs_core::value::StringTextPropertyRun {
            start: 0,
            end: 2,
            plist: Value::list(vec![
                Value::symbol("mouse-face"),
                Value::symbol("wide-hover"),
            ]),
        }],
    );
    let rendered = RenderedDisplayRow::new(
        GlyphRow::new(GlyphRowRole::TabBar),
        DisplayRowOutputProgress::new(32.0, 4, 0.0, 18.0),
        vec![
            DisplayRowGlyphSlot::new(DisplaySourcePosition::lisp_string(1, 0, 0), 0.0, 0, 16.0, 2),
            DisplayRowGlyphSlot::new(
                DisplaySourcePosition::lisp_string(1, 1, 3),
                16.0,
                2,
                16.0,
                2,
            ),
        ],
        Vec::new(),
    );
    let items = [TabBarSourceItem {
        caption: text,
        key: Value::symbol("tab-1"),
        binding: Value::symbol("ignore"),
        char_range: 0..2,
        enabled: true,
    }];
    let slots = tab_bar_pointer_slot_plan(&mut eval, &rendered, text, &items);
    let presentation = eval.begin_interaction_presentation();
    let plan = tab_bar_presented_pointer_plan(
        &mut eval,
        presentation,
        &slots,
        &items,
        18.0,
        TabBarPointerAppearanceStyle::new(
            test_tab_pointer_relief(true),
            test_tab_pointer_relief(false),
        ),
        &[],
        &[(Value::symbol("wide-hover"), FaceId::new(42))],
    );
    assert_eq!(plan.hit_regions().len(), 1);
    assert_eq!(plan.hit_regions()[0].local_bounds().raw().width, 32.0);
    let source = plan
        .into_source_map(FrameRect::new(0.0, 0.0, 80.0, 18.0).unwrap(), 0)
        .unwrap();

    assert_eq!(source.regions().len(), 1);
    assert_eq!(source.appearances().len(), 1);
    assert_eq!(source.appearances()[0].paint_spans().len(), 1);
    assert_eq!(source.appearances()[0].paint_spans()[0].len(), 2);
}

#[test]
fn disabled_tab_bar_item_publishes_no_pointer_behavior() {
    let mut eval = Context::new();
    eval.setup_thread_locals();
    let rendered = RenderedDisplayRow::new(
        GlyphRow::new(GlyphRowRole::TabBar),
        DisplayRowOutputProgress::new(8.0, 1, 0.0, 18.0),
        vec![DisplayRowGlyphSlot::new(
            DisplaySourcePosition::lisp_string(1, 0, 0),
            0.0,
            0,
            8.0,
            1,
        )],
        Vec::new(),
    );
    let text = Value::string("x");
    let items = [TabBarSourceItem {
        caption: text,
        key: Value::symbol("add-tab"),
        binding: Value::symbol("tab-bar-new-tab"),
        char_range: 0..1,
        enabled: false,
    }];
    let presentation = eval.begin_interaction_presentation();
    let slots = tab_bar_pointer_slot_plan(&mut eval, &rendered, text, &items);
    let plan = tab_bar_presented_pointer_plan(
        &mut eval,
        presentation,
        &slots,
        &items,
        18.0,
        TabBarPointerAppearanceStyle::new(
            test_tab_pointer_relief(true),
            test_tab_pointer_relief(false),
        ),
        &[],
        &[],
    );
    assert!(plan.hit_regions().is_empty());
    assert!(
        plan.into_source_map(FrameRect::new(0.0, 0.0, 80.0, 18.0).unwrap(), 0)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn tab_bar_display_source_builds_concat_text_and_preserves_items() {
    let mut eval = Context::new();
    let source = TabBarDisplaySource {
        entries: vec![
            TabBarDisplayEntry {
                caption: Value::string("One"),
                key: Value::symbol("tab-1"),
                binding: Value::symbol("ignore"),
                enabled: true,
            },
            TabBarDisplayEntry {
                caption: Value::string("Two"),
                key: Value::symbol("tab-2"),
                binding: Value::symbol("ignore"),
                enabled: true,
            },
        ],
    };

    let built = source.into_built_tab_bar(&mut eval).expect("built tab bar");

    assert_eq!(
        built.text.as_runtime_string_owned().as_deref(),
        Some("OneTwo")
    );
    assert_eq!(built.source_items.len(), 2);
}

#[test]
fn window_chrome_target_cols_reserves_right_border_column() {
    assert_eq!(
        WindowChromeTargetColumns::new(80.0, 8.0, false).columns(),
        10
    );
    assert_eq!(WindowChromeTargetColumns::new(80.0, 8.0, true).columns(), 9);
    assert_eq!(WindowChromeTargetColumns::new(3.0, 8.0, true).columns(), 1);
    assert_eq!(
        WindowChromeTargetColumns::new(80.0, 0.0, false).columns(),
        80
    );
}

#[test]
fn display_row_face_preserves_gnu_box_type_codes() {
    let mut resolved = ResolvedFace::default();
    let boxes = [
        (0, BoxType::None),
        (1, BoxType::Line),
        (2, BoxType::Raised3D),
        (3, BoxType::Sunken3D),
    ];

    for (code, box_type) in boxes {
        resolved.box_type = code;
        let row_face = DisplayRowFace::from_resolved(FaceId::new(1), &resolved);
        assert_eq!(row_face.box_type, box_type);
        assert_eq!(row_face.render_face().box_type, box_type);
    }
}

#[test]
fn display_row_face_preserves_explicit_black_box_color() {
    let mut resolved = ResolvedFace::default();
    resolved.box_type = 1;
    resolved.box_color = 0x00000000;
    resolved.fg = 0x00ff_ffff;

    let row_face = DisplayRowFace::from_resolved(FaceId::new(1), &resolved);

    assert_eq!(row_face.box_type, BoxType::Line);
    assert_eq!(
        row_face.box_color,
        Some(Color::BLACK),
        "pixel value zero is a valid explicit black color, not an absence sentinel",
    );
}
