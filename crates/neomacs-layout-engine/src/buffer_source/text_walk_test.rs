use super::*;
use crate::buffer_source::body_render::BufferSourceWalkSetupRequest;
use crate::buffer_source::loop_context::BufferSourceLoopRequestContext;
use crate::buffer_source::render_plan::BufferSourceOutputSetup;
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::types::WindowKind;
use crate::window_layout::{WindowChromeMetrics, WindowDividerLayout, WindowLayoutBox};
use neomacs_display_protocol::types::{Color, Rect};
use neovm_core::window::{FrameId, WindowId};

fn window_params() -> WindowParams {
    WindowParams {
        space_image_catalog: None,
        window_id: 1,
        buffer_id: 1,
        bounds: Rect::new(0.0, 8.0, 240.0, 120.0),
        text_bounds: Rect::new(16.0, 32.0, 160.0, 80.0),
        selected: true,
        mode_line_active: true,
        kind: WindowKind::Main,
        left_col: 0,
        top_line: 0,
        window_start: 17,
        force_start: false,
        previous_visible_end: None,
        point: 17,
        buffer_size: 80,
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
        default_fg: 0x00ff_ffff,
        default_bg: 0,
        char_width: 8.0,
        char_height: 16.0,
        window_system: true,
        font_pixel_size: 14.0,
        image_scale_environment: Default::default(),
        font_ascent: 11.0,
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
        left_fringe_width: 8.0,
        right_fringe_width: 8.0,
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
        nobreak_char_display: 0,
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

fn setup_request() -> BufferSourceWalkSetupRequest<'static> {
    BufferSourceWalkSetupRequest::new(
        17,
        24.0,
        16.0,
        160.0,
        32.0,
        8.0,
        12.0,
        5,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 11.0),
        LineWrapMode::Truncate,
        3,
        true,
        true,
        true,
        true,
        false,
        4,
        &[4, 12],
        true,
        0x00ff00,
    )
}

fn geometry_request(
    params: &WindowParams,
    char_width: f32,
    char_height: f32,
    mode_line_height: f32,
    header_line_height: f32,
    tab_line_height: f32,
) -> BufferWindowGeometryRequest {
    let layout_box = WindowLayoutBox::resolve(
        params,
        WindowChromeMetrics {
            tab_line_height,
            header_line_height,
            mode_line_height,
        },
        WindowDividerLayout::without_dividers(params),
    );
    BufferWindowGeometryRequest::new(params, &layout_box, char_width, char_height)
}

#[test]
fn geometry_request_derives_text_area_and_matrix_rows() {
    let params = window_params();
    let request = geometry_request(&params, 8.0, 16.0, 12.0, 10.0, 6.0);

    assert_eq!(request.line_number_row_capacity(), 5);

    let geometry = request.into_geometry(
        crate::display_row::walk_state::LineNumberFieldLayout::new(3, 8.0),
    );

    assert_eq!(geometry.text_x, 16.0);
    assert_eq!(geometry.text_y, 24.0);
    assert_eq!(geometry.text_width, 160.0);
    assert_eq!(geometry.text_height, 92.0);
    assert_eq!(geometry.char_width, 8.0);
    assert_eq!(geometry.char_height, 16.0);
    assert_eq!(geometry.max_rows, 5);
    assert_eq!(geometry.display_text_row_base, 2);
    assert_eq!(geometry.display_text_rows, 5);
    assert_eq!(geometry.bottom_chrome_rows, 1);
    assert_eq!(geometry.mode_line_display_row, 7);
    assert_eq!(geometry.line_number_pixel_width, 24.0);
    assert_eq!(geometry.content_x, 40.0);
    assert_eq!(geometry.matrix_columns.get(), 20);
}

#[test]
fn geometry_reserves_the_measured_line_number_face_extent() {
    let params = window_params();
    let request = geometry_request(&params, 8.0, 16.0, 12.0, 10.0, 6.0);
    let field = crate::display_row::walk_state::LineNumberFieldLayout::new(3, 21.0);

    let geometry = request.into_geometry(field);

    assert_eq!(geometry.line_number_pixel_width, 63.0);
    assert_eq!(geometry.content_x, geometry.text_x + 63.0);
}

#[test]
fn geometry_request_only_forces_fractional_row_for_minibuffer() {
    let mut params = window_params();
    params.bounds.height = 15.0;
    params.text_bounds.height = 15.0;

    let ordinary = geometry_request(&params, 8.0, 16.0, 0.0, 0.0, 0.0).into_geometry(
        crate::display_row::walk_state::LineNumberFieldLayout::new(0, 8.0),
    );
    assert_eq!(ordinary.max_rows, 0);

    params.kind = WindowKind::Minibuffer;
    let minibuffer = geometry_request(&params, 8.0, 16.0, 0.0, 0.0, 0.0).into_geometry(
        crate::display_row::walk_state::LineNumberFieldLayout::new(0, 8.0),
    );
    assert_eq!(minibuffer.max_rows, 1);
}

#[test]
fn geometry_request_measures_minibuffer_up_to_max_mini_window_rows() {
    // GNU `resize_mini_window` measures the mini-window content unclamped and
    // clips only to `max-mini-window-height`. `with_max_mini_window_rows` lets
    // the walk emit up to that ceiling even when the window is one row tall.
    let mut params = window_params();
    params.kind = WindowKind::Minibuffer;
    // One physical row tall (16px), but a ceiling of 3 rows.
    params.bounds.height = 16.0;
    params.text_bounds.height = 16.0;
    let request = geometry_request(&params, 8.0, 16.0, 0.0, 0.0, 0.0).with_max_mini_window_rows(3);

    let geometry = request.into_geometry(
        crate::display_row::walk_state::LineNumberFieldLayout::new(0, 8.0),
    );

    assert_eq!(geometry.max_rows, 3);
    assert_eq!(geometry.display_text_rows, 3);
    assert_eq!(geometry.mode_line_display_row, 3);
    // The visibility bottom is lifted so the walk can emit all three rows even
    // though the window is physically one row tall.
    assert_eq!(geometry.visibility_bottom_y, geometry.text_y + 3.0 * 16.0);
}

#[test]
fn geometry_request_does_not_apply_max_mini_window_rows_to_ordinary_windows() {
    let mut params = window_params();
    params.bounds.height = 16.0;
    params.text_bounds.height = 16.0;
    let request = geometry_request(&params, 8.0, 16.0, 0.0, 0.0, 0.0).with_max_mini_window_rows(5);

    let geometry = request.into_geometry(
        crate::display_row::walk_state::LineNumberFieldLayout::new(0, 8.0),
    );

    // Ordinary windows ignore the minibuffer ceiling and keep the physical
    // row count and physical visibility bottom.
    assert_eq!(geometry.max_rows, 1);
    assert_eq!(geometry.visibility_bottom_y, geometry.text_y + 16.0);
}

#[test]
fn ordinary_window_vscroll_shifts_row_origin_and_keeps_full_height() {
    // GNU `w->vscroll` scrolls an ordinary window's contents UP by `vscroll`
    // pixels: the text area keeps its full height, the row-walk origin moves up
    // to `text_y - vscroll` (top-clipped first row), and one extra partially
    // visible row is exposed at the bottom.  `w->vscroll` is stored negative.
    let mut params = window_params();
    // vscroll of 8px (half a 16px row).
    params.vscroll = -8;
    // mode-line only (no header/tab) => text_height is a clean multiple of the
    // row height: 120 - 8 = 112 = 7 rows.
    let request = geometry_request(&params, 8.0, 16.0, 8.0, 0.0, 0.0);
    let geometry = request.into_geometry(
        crate::display_row::walk_state::LineNumberFieldLayout::new(0, 8.0),
    );

    // Full height is retained (NOT shrunk by vscroll) for an ordinary window.
    assert_eq!(geometry.text_y, 8.0);
    assert_eq!(geometry.text_height, 112.0);
    // The applied content-up shift and the shifted walk origin.
    assert_eq!(geometry.vscroll, 8.0);
    assert_eq!(geometry.row_origin_y(), 0.0);
    // base rows (7) + one exposed bottom row = 8.
    assert_eq!(geometry.max_rows, 8);
    assert_eq!(geometry.display_text_rows, 8);
    // Walk bottom lifted to the shifted last-row edge so the extra bottom row is
    // emitted; the visible/clip band still ends at text_y + text_height = 144.
    assert_eq!(geometry.visibility_bottom_y, 128.0);
    assert_eq!(
        geometry.row_origin_y() + geometry.max_rows as f32 * 16.0,
        128.0
    );
}

#[test]
fn minibuffer_vscroll_preserves_shrink_and_unshifted_origin() {
    // A minibuffer repurposes vscroll to HIDE content by shrinking the visible
    // area (vertico-posframe): the historical shrink is preserved and the origin
    // is NOT shifted (row_origin_y == text_y).
    let mut params = window_params();
    params.kind = WindowKind::Minibuffer;
    params.vscroll = -8;
    let request = geometry_request(&params, 8.0, 16.0, 8.0, 0.0, 0.0);
    let geometry = request.into_geometry(
        crate::display_row::walk_state::LineNumberFieldLayout::new(0, 8.0),
    );

    // Height IS shrunk by vscroll for a minibuffer: 112 - 8 = 104.
    assert_eq!(geometry.text_y, 8.0);
    assert_eq!(geometry.text_height, 104.0);
    // No content-up shift applied to the origin.
    assert_eq!(geometry.vscroll, 0.0);
    assert_eq!(geometry.row_origin_y(), 8.0);
    // floor(104 / 16) = 6 physical rows.
    assert_eq!(geometry.max_rows, 6);
}

#[test]
fn tty_window_vscroll_keeps_historical_shrink_not_shift() {
    // The GNU content-up shift is a graphical concept; a TTY (non-window-system)
    // frame is a char-cell grid and must keep the historical behavior (vscroll
    // shrinks text_height, origin unshifted) -- byte-identical to pre-fix, so
    // "do NOT change TTY".
    let mut params = window_params();
    params.window_system = false;
    params.vscroll = -8;
    let geometry = geometry_request(&params, 8.0, 16.0, 8.0, 0.0, 0.0).into_geometry(
        crate::display_row::walk_state::LineNumberFieldLayout::new(0, 8.0),
    );

    // Shrunk like before (112 - 8 = 104); NO origin shift; physical row count.
    assert_eq!(geometry.text_height, 104.0);
    assert_eq!(geometry.vscroll, 0.0);
    assert_eq!(geometry.row_origin_y(), geometry.text_y);
    assert_eq!(geometry.max_rows, 6);
    assert_eq!(geometry.visibility_bottom_y, geometry.text_y + 104.0);
}

#[test]
fn ordinary_window_zero_vscroll_is_unchanged() {
    // vscroll == 0 must be byte-identical to the pre-fix behavior: no shift, no
    // extra row, visibility bottom at the physical text-area bottom.
    let params = window_params();
    assert_eq!(params.vscroll, 0);
    let geometry = geometry_request(&params, 8.0, 16.0, 8.0, 0.0, 0.0).into_geometry(
        crate::display_row::walk_state::LineNumberFieldLayout::new(0, 8.0),
    );

    assert_eq!(geometry.vscroll, 0.0);
    assert_eq!(geometry.text_height, 112.0);
    assert_eq!(geometry.row_origin_y(), geometry.text_y);
    assert_eq!(geometry.max_rows, 7);
    assert_eq!(
        geometry.visibility_bottom_y,
        geometry.text_y + geometry.text_height
    );
}

#[test]
fn walk_setup_initializes_source_position_and_geometry_state() {
    let setup = setup_request().into_setup();

    assert_eq!(setup.byte_idx, 0);
    assert_eq!(setup.charpos, 17);
    assert_eq!(setup.x, 24.0);
    assert_eq!(setup.col, 0);
    assert_eq!(setup.text_area_left, 16.0);
    assert_eq!(setup.window_top, 8.0);
    assert_eq!(setup.row_flags.len(), 5);
    assert_eq!(setup.row_geometry.row(), 0);
    assert_eq!(setup.row_geometry.y(), 32.0);
    assert_eq!(setup.row_geometry.height(), 16.0);
    assert_eq!(setup.row_geometry.ascent(), 11.0);
    assert_eq!(setup.hit_row_range.start(), 17);
}

#[test]
fn walk_setup_applies_hscroll_prefix_and_reserved_surface_policy() {
    let setup = setup_request().into_setup();

    assert!(setup.hscroll_skip.should_skip());
    assert_eq!(setup.hscroll_skip.consumed_columns(), 0);
    assert!(setup.prefix_request.is_requested());
    assert_eq!(setup.text_append_surface.content_x(), 24.0);
    assert_eq!(setup.text_append_surface.right_edge(), 164.0);
    assert!(setup.trailing_whitespace.background().is_some());
}

#[test]
fn output_setup_derives_begin_request_and_row_limits_from_walk_setup() {
    let walk_setup = setup_request().into_setup();
    let output_setup = BufferSourceOutputSetup::new(
        FrameId(3),
        WindowId(9),
        99,
        2,
        6,
        1,
        0,
        Rect::new(0.0, 8.0, 240.0, 120.0),
        Rect::new(16.0, 32.0, 160.0, 80.0),
        Rect::new(16.0, 32.0, 160.0, 48.0),
        true,
        32.0,
        48.0,
        80.0,
        5,
        &walk_setup,
    );

    assert_eq!(output_setup.row_visibility_limit().max_rows, 5);
    assert_eq!(output_setup.row_visibility_limit().bottom_y, 80.0);
    assert_eq!(output_setup.row_limit().max_rows, 5);
    assert_eq!(output_setup.body_install_context().output_cols(), 1);
    assert_eq!(output_setup.retry_bounds().text_area_top(), 24);
    assert_eq!(output_setup.retry_bounds().text_area_bottom(), 72);
}

#[test]
fn loop_request_context_carries_buffer_and_window_policy() {
    let params = window_params();
    let walk_setup = setup_request().into_setup();
    let output_setup = BufferSourceOutputSetup::new(
        FrameId(3),
        WindowId(9),
        99,
        2,
        6,
        1,
        20,
        params.bounds,
        params.text_bounds,
        Rect::new(16.0, 32.0, 160.0, 48.0),
        params.selected,
        32.0,
        48.0,
        80.0,
        5,
        &walk_setup,
    );
    let context = BufferSourceLoopRequestContext::new(
        neovm_core::buffer::BufferId(42),
        11,
        80,
        17,
        &params,
        24.0,
        true,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 11.0),
        output_setup.row_visibility_limit(),
        walk_setup.row_geometry_defaults,
        2,
        5,
        output_setup.row_limit(),
        Color::from_pixel(0x00FFFFFF),
    );

    assert_eq!(context.buffer_id(), neovm_core::buffer::BufferId(42));
    assert_eq!(context.text_start_byte(), 11);
    assert_eq!(context.accessible_end(), 80);
    assert_eq!(context.selective_display(), params.selective_display);
    assert_eq!(context.tab_width(), params.tab_width);
    assert_eq!(context.row_limit(), output_setup.row_limit());
}

#[test]
fn row_prelude_request_context_carries_margin_and_prefix_policy() {
    let prefix_values =
        crate::display_row::lisp_string::DisplayRowPrefixValues::default_values(None, None);
    let context = BufferSourceRowPreludeRequestContext::new(
        DisplayLineNumbersMode::Relative,
        true,
        3,
        4,
        crate::display_row::walk_state::LineNumberFieldLayout::new(5, 8.0),
        prefix_values,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
    );

    assert_eq!(context.line_number_mode(), DisplayLineNumbersMode::Relative);
    assert_eq!(context.prefix_values(), prefix_values);
    assert_eq!(context.char_width(), 8.0);
}

#[test]
fn local_display_policy_builds_row_prelude_context() {
    let prefix_values =
        crate::display_row::lisp_string::DisplayRowPrefixValues::default_values(None, None);
    let policy = BufferWindowLocalDisplayPolicy::from_parts(
        DisplayLineNumbersMode::Relative,
        false,
        3,
        prefix_values,
    );
    let context = policy.row_prelude_context(
        crate::display_row::walk_state::LineNumberFieldLayout::new(6, 8.0),
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
    );

    assert!(!policy.has_prefix());
    assert!(!policy.has_line_default_prefix());
    assert_eq!(context.line_number_mode(), DisplayLineNumbersMode::Relative);
    assert_eq!(context.prefix_values(), prefix_values);
    assert_eq!(context.char_width(), 8.0);
}
