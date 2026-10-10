use super::*;
use crate::buffer_source::window_geometry::BufferWindowGeometryRequest;
use crate::buffer_source::window_source::BufferWindowSourceRequest;
use crate::display_row::geometry::{DisplayRowFlagKind, DisplayRowLimit};
use crate::display_row::walk_state::LineNumberFieldLayout;
use crate::types::{DisplayLineNumbersMode, WindowKind, WindowSourceExtent};
use crate::window_layout::{WindowChromeMetrics, WindowDividerLayout, WindowLayoutBox};
use neomacs_display_protocol::types::Rect;
use neovm_core::buffer::{Buffer, BufferId};
use neovm_core::emacs_core::{Context, Value};
use neovm_core::face::FaceTable;

fn window_params() -> WindowParams {
    WindowParams {
        space_image_catalog: None,
        window_id: 1,
        buffer_id: 1,
        bounds: Rect::new(0.0, 8.0, 20.0, 1.0),
        text_bounds: Rect::new(0.0, 8.0, 20.0, 1.0),
        selected: true,
        cursor_role: crate::types::WindowCursorRole::Active,
        mode_line_active: true,
        kind: WindowKind::Minibuffer,
        left_col: 0,
        top_line: 0,
        window_start: 0,
        measurement_rows: None,
        measurement_width: None,
        source_extent: crate::types::WindowSourceExtent::Viewport,
        measurement_pixels: None,
        query_target: None,
        force_start: false,
        previous_visible_end: None,
        point: 0,
        buffer_size: 0,
        buffer_modiff: 0,
        buffer_begv: 0,
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
        default_fg: 0x00ff_ffff,
        frame_foreground: 0x00ff_ffff,
        default_bg: 0,
        char_width: 1.0,
        char_height: 1.0,
        window_system: false,
        font_pixel_size: 14.0,
        image_scale_environment: Default::default(),
        font_ascent: 1.0,
        mode_line_height: 0.0,
        header_line_height: 0.0,
        tab_line_height: 0.0,
        cursor_kind: neomacs_display_protocol::frame_glyphs::CursorKind::FilledBox,
        cursor_bar_width: neomacs_display_protocol::cursor::CursorBarWidth::TWO,
        x_stretch_cursor: false,
        cursor_color: 0x00ff_ffff,
        cursor_foreground: 0,
        cursor_effects: None,
        visual_cursors: Vec::new(),
        left_fringe_width: 0.0,
        right_fringe_width: 0.0,
        fringes_outside_margins: false,
        indicate_empty_lines: 2,
        show_trailing_whitespace: false,
        trailing_ws_bg: 0,
        fill_column_indicator: 3,
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

fn mini_setup(measurement: WindowSourceExtent) -> (BufferWindowGeometry, BufferSourceWalkSetup) {
    let _runtime = Context::new();
    let mut params = window_params();
    params.source_extent = measurement;
    let layout_box = WindowLayoutBox::resolve(
        &params,
        WindowChromeMetrics {
            tab_line_height: 0.0,
            header_line_height: 0.0,
            mode_line_height: 0.0,
        },
        WindowDividerLayout::without_dividers(&params),
    );
    let geometry = BufferWindowGeometryRequest::new(&params, &layout_box, 1.0, 1.0)
        .into_geometry(LineNumberFieldLayout::new(0, 1.0));
    let buffer = Buffer::new_standalone(BufferId(42), Value::string("*mini-row-storage*"));
    let source = BufferWindowSourceRequest::from_window_params(&params, geometry.max_rows)
        .read_exact_into(&RustBufferAccess::new(&buffer), &mut Vec::new());
    let policy = BufferWindowLocalDisplayPolicy::from_window(&buffer, &params);
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x00ff_ffff, 0, 14.0, None);
    let face = BufferSourceDefaultFacePlan::new(
        &resolver,
        &buffer,
        &mut None,
        DisplayRowMeasurementMode::LogicalCells,
        DisplayRowFallbackMetrics::from_default_face_extents(1.0, 1.0, 1.0),
    );
    let setup = BufferSourceWalkSetupRequest::from_window_geometry(
        source, &params, &geometry, &policy, &face, false, false,
    )
    .into_setup();
    (geometry, setup)
}

#[test]
fn mini_to_end_setup_grows_row_flags_without_allocating_the_logical_limit() {
    // This is the production mini-geometry -> source -> walk-setup seam. Its
    // logical row limit is unbounded even though the current viewport is one
    // row. The old setup panics before returning, allocating usize::MAX flags.
    let (geometry, mut setup) = mini_setup(WindowSourceExtent::AccessibleEnd);
    assert_eq!(geometry.max_rows, usize::MAX);
    assert_eq!(geometry.display_text_rows, 1);
    assert_eq!(setup.row_flags.len(), 0);
    assert_eq!(setup.row_y_positions.recorded(), &[geometry.row_origin_y()]);

    let kinds = [
        DisplayRowFlagKind::Continued,
        DisplayRowFlagKind::Truncated,
        DisplayRowFlagKind::Continuation,
        DisplayRowFlagKind::ContinuedMidElement,
        DisplayRowFlagKind::WideCut,
    ];
    let limit = DisplayRowLimit {
        max_rows: geometry.max_rows,
    };
    for row in 0..9 {
        setup.row_geometry = DisplayRowGeometryState::for_measurement_mode(
            row,
            geometry.row_origin_y() + row as f32,
            0.0,
            1.0,
            1.0,
            DisplayRowMeasurementMode::LogicalCells,
        );
        setup.row_geometry.mark_current_row_flag_kind(
            &mut setup.row_flags,
            kinds[row % kinds.len()],
            limit,
        );
        assert_eq!(setup.row_flags.len(), row + 1);
        assert!(setup.row_flags.is_set(row, kinds[row % kinds.len()]));
        if row > 0 {
            setup.row_y_positions.push(setup.row_geometry.y());
        }
    }
    for row in 0..9 {
        for (index, kind) in kinds.iter().copied().enumerate() {
            assert_eq!(
                setup.row_flags.is_set(row, kind),
                index == row % kinds.len()
            );
        }
    }
    assert!(!setup.row_flags.is_set(9, DisplayRowFlagKind::Truncated));
    assert_eq!(setup.row_y_positions.recorded().len(), 9);
    assert_eq!(
        setup.row_y_positions.recorded()[8],
        geometry.row_origin_y() + 8.0
    );

    // Presentation keeps its existing fixed viewport storage and out-of-range
    // mark behavior. This also rules out interpreting a row limit as the mode.
    let (normal_geometry, mut normal) = mini_setup(WindowSourceExtent::Viewport);
    assert_eq!(normal_geometry.max_rows, 1);
    assert_eq!(normal.row_flags.len(), 1);
    normal.row_flags.mark(0, DisplayRowFlagKind::WideCut);
    normal.row_flags.mark(1, DisplayRowFlagKind::Truncated);
    assert!(normal.row_flags.is_set(0, DisplayRowFlagKind::WideCut));
    assert!(!normal.row_flags.is_set(1, DisplayRowFlagKind::Truncated));
    assert_eq!(normal.row_flags.len(), 1);
    assert_eq!(
        normal.row_geometry,
        normal.row_geometry_defaults.initial_state()
    );
    assert_eq!(
        normal.row_y_positions.recorded(),
        &[normal_geometry.row_origin_y()]
    );
}
