use super::*;
use neomacs_display_protocol::cursor::CursorBarWidth;
// --- WindowParams ---

#[test]
fn line_number_modes_declare_their_point_motion_body_dependency() {
    assert_eq!(
        DisplayLineNumbersMode::Off.point_motion_body_dependency(),
        PointMotionBodyDependency::Independent
    );
    assert_eq!(
        DisplayLineNumbersMode::Absolute.point_motion_body_dependency(),
        PointMotionBodyDependency::CurrentDisplayRow
    );
    for mode in [
        DisplayLineNumbersMode::Relative,
        DisplayLineNumbersMode::Visual,
    ] {
        assert_eq!(
            mode.point_motion_body_dependency(),
            PointMotionBodyDependency::EntireWindow
        );
    }
}

#[test]
fn window_params_construction() {
    let params = WindowParams {
        space_image_catalog: None,
        window_id: 12345,
        buffer_id: 67890,
        bounds: Rect::new(0.0, 0.0, 800.0, 600.0),
        text_bounds: Rect::new(10.0, 0.0, 780.0, 580.0),
        selected: true,
        cursor_role: WindowCursorRole::Active,
        mode_line_active: true,
        kind: WindowKind::Main,
        left_col: 0,
        top_line: 0,
        window_start: 1,
        force_start: false,
        previous_visible_end: None,
        point: 42,
        buffer_size: 10000,
        buffer_modiff: 0,
        buffer_begv: 1,
        display_line_numbers: DisplayLineNumbersMode::Off,
        hscroll: 0,
        vscroll: 0,
        wrap_mode: LineWrapMode::Wrap,
        word_wrap: true,
        tab_width: 8,
        scroll_conservatively: 0,
        scroll_step: 0,
        scroll_minibuffer_conservatively: true,
        scroll_margin: 0,
        tab_stop_list: vec![],
        default_fg: 0x00FFFFFF,
        default_bg: 0x00000000,
        char_width: 8.0,
        char_height: 16.0,
        window_system: true,
        font_pixel_size: 14.0,
        image_scale_environment: Default::default(),
        font_ascent: 12.0,
        mode_line_height: 20.0,
        header_line_height: 0.0,
        tab_line_height: 0.0,
        cursor_kind: neomacs_display_protocol::frame_glyphs::CursorKind::FilledBox,
        cursor_bar_width: CursorBarWidth::TWO,
        x_stretch_cursor: false,
        cursor_color: 0x00000000,
        cursor_foreground: 0x00ffffff,
        cursor_effects: None,
        visual_cursors: Vec::new(),
        left_fringe_width: 8.0,
        right_fringe_width: 8.0,
        fringes_outside_margins: false,
        indicate_empty_lines: 0,
        show_trailing_whitespace: false,
        trailing_ws_bg: 0,
        fill_column_indicator: 80,
        fill_column_indicator_char: '|',
        fill_column_indicator_fg: 0x00808080,
        extra_line_spacing: 0.0,
        selective_display: 0,
        escape_glyph_fg: 0x00FF0000,
        nobreak_char_display: NobreakDisplayMode::HighlightOriginal,
        nobreak_char_fg: 0x0000FF00,
        glyphless_char_fg: 0x00808080,
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
    };
    assert_eq!(params.window_id, 12345);
    assert_eq!(params.buffer_id, 67890);
    assert!(params.selected);
    assert!(!params.is_minibuffer());
    assert_eq!(params.point, 42);
    assert!(params.word_wrap);
    assert_eq!(params.wrap_mode, LineWrapMode::Wrap);
    assert_eq!(params.tab_width, 8);
    assert_eq!(params.fill_column_indicator, 80);
    assert_eq!(params.fill_column_indicator_char, '|');
}

#[test]
fn window_params_minibuffer() {
    let params = WindowParams {
        space_image_catalog: None,
        window_id: 1,
        buffer_id: 1,
        bounds: Rect::new(0.0, 580.0, 800.0, 20.0),
        text_bounds: Rect::new(0.0, 580.0, 800.0, 20.0),
        selected: true,
        cursor_role: WindowCursorRole::Active,
        mode_line_active: false,
        kind: WindowKind::Minibuffer,
        left_col: 0,
        top_line: 0,
        window_start: 1,
        force_start: false,
        previous_visible_end: None,
        point: 1,
        buffer_size: 0,
        buffer_modiff: 0,
        buffer_begv: 1,
        display_line_numbers: DisplayLineNumbersMode::Off,
        hscroll: 0,
        vscroll: 0,
        wrap_mode: LineWrapMode::Truncate,
        word_wrap: false,
        tab_width: 8,
        scroll_conservatively: 0,
        scroll_step: 0,
        scroll_minibuffer_conservatively: true,
        scroll_margin: 0,
        tab_stop_list: vec![],
        default_fg: 0x00FFFFFF,
        default_bg: 0x00000000,
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
        cursor_bar_width: CursorBarWidth::TWO,
        x_stretch_cursor: false,
        cursor_color: 0x00000000,
        cursor_foreground: 0x00ffffff,
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
        nobreak_char_display: NobreakDisplayMode::Literal,
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
    };
    assert!(params.is_minibuffer());
    assert_eq!(params.mode_line_height, 0.0);
}

#[test]
fn window_params_clone() {
    let params = WindowParams {
        space_image_catalog: None,
        window_id: 1,
        buffer_id: 1,
        bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
        text_bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
        selected: false,
        cursor_role: WindowCursorRole::Inactive,
        mode_line_active: false,
        kind: WindowKind::Main,
        left_col: 0,
        top_line: 0,
        window_start: 1,
        force_start: false,
        previous_visible_end: None,
        point: 1,
        buffer_size: 100,
        buffer_modiff: 0,
        buffer_begv: 1,
        display_line_numbers: DisplayLineNumbersMode::Off,
        hscroll: 5,
        vscroll: 0,
        wrap_mode: LineWrapMode::Truncate,
        word_wrap: false,
        tab_width: 4,
        scroll_conservatively: 0,
        scroll_step: 0,
        scroll_minibuffer_conservatively: true,
        scroll_margin: 0,
        tab_stop_list: vec![],
        default_fg: 0,
        default_bg: 0,
        char_width: 8.0,
        char_height: 16.0,
        window_system: true,
        font_pixel_size: 14.0,
        image_scale_environment: Default::default(),
        font_ascent: 12.0,
        mode_line_height: 20.0,
        header_line_height: 20.0,
        tab_line_height: 20.0,
        cursor_kind: neomacs_display_protocol::frame_glyphs::CursorKind::Bar,
        cursor_bar_width: CursorBarWidth::new(3),
        x_stretch_cursor: false,
        cursor_color: 0x00000000,
        cursor_foreground: 0x00ffffff,
        cursor_effects: None,
        visual_cursors: Vec::new(),
        left_fringe_width: 10.0,
        right_fringe_width: 10.0,
        fringes_outside_margins: false,
        indicate_empty_lines: 1,
        show_trailing_whitespace: true,
        trailing_ws_bg: 0x00FF0000,
        fill_column_indicator: -1,
        fill_column_indicator_char: '|',
        fill_column_indicator_fg: 0,
        extra_line_spacing: 2.0,
        selective_display: 3,
        escape_glyph_fg: 0,
        nobreak_char_display: NobreakDisplayMode::Escape,
        nobreak_char_fg: 0,
        glyphless_char_fg: 0,
        wrap_prefix: b"  ".to_vec(),
        line_prefix: b"> ".to_vec(),
        left_margin_width: 5.0,
        left_margin_columns: 1,
        right_margin_width: 5.0,
        right_margin_columns: 1,
        vertical_scroll_bar_side: None,
        horizontal_scroll_bar: false,
        scroll_bar_pixel_width: 0.0,
        scroll_bar_pixel_height: 0.0,
    };
    let cloned = params.clone();
    assert_eq!(cloned.window_id, params.window_id);
    assert_eq!(cloned.hscroll, 5);
    assert_eq!(cloned.tab_width, 4);
    assert_eq!(cloned.wrap_mode, LineWrapMode::Truncate);
    assert!(cloned.show_trailing_whitespace);
    assert_eq!(cloned.wrap_prefix, b"  ".to_vec());
    assert_eq!(cloned.line_prefix, b"> ".to_vec());
    assert_eq!(cloned.selective_display, 3);
    assert_eq!(cloned.extra_line_spacing, 2.0);
}

// --- FrameParams ---

#[test]
fn frame_params_construction() {
    let fp = FrameParams {
        width: 1920.0,
        height: 1080.0,
        menu_bar_height: 0.0,
        tool_bar_height: 0.0,
        compact_bar_height: 0.0,
        tab_bar_height: 0.0,
        char_width: 8.0,
        char_height: 16.0,
        font_pixel_size: 14.0,
        image_scale_environment: Default::default(),
        window_system: true,
        background: 0x00282828,
        vertical_border_fg: 0x00808080,
        zero_width_vertical_border_edge: neomacs_display_protocol::PresentedResizeEdge::Trailing,
        right_divider_width: 1,
        bottom_divider_width: 1,
        divider_fg: 0x00444444,
        divider_first_fg: 0x00555555,
        divider_last_fg: 0x00333333,
    };
    assert_eq!(fp.width, 1920.0);
    assert_eq!(fp.height, 1080.0);
    assert_eq!(fp.char_width, 8.0);
    assert_eq!(fp.char_height, 16.0);
    assert_eq!(fp.font_pixel_size, 14.0);
    assert_eq!(fp.background, 0x00282828);
    assert_eq!(fp.right_divider_width, 1);
    assert_eq!(fp.bottom_divider_width, 1);
}

#[test]
fn frame_params_no_dividers() {
    let fp = FrameParams {
        width: 800.0,
        height: 600.0,
        menu_bar_height: 0.0,
        tool_bar_height: 0.0,
        compact_bar_height: 0.0,
        tab_bar_height: 0.0,
        char_width: 7.0,
        char_height: 14.0,
        font_pixel_size: 12.0,
        image_scale_environment: Default::default(),
        window_system: false,
        background: 0x00FFFFFF,
        vertical_border_fg: 0x00000000,
        zero_width_vertical_border_edge: neomacs_display_protocol::PresentedResizeEdge::Trailing,
        right_divider_width: 0,
        bottom_divider_width: 0,
        divider_fg: 0,
        divider_first_fg: 0,
        divider_last_fg: 0,
    };
    assert_eq!(fp.right_divider_width, 0);
    assert_eq!(fp.bottom_divider_width, 0);
}

#[test]
fn frame_params_clone() {
    let fp = FrameParams {
        width: 1024.0,
        height: 768.0,
        menu_bar_height: 0.0,
        tool_bar_height: 0.0,
        compact_bar_height: 0.0,
        tab_bar_height: 0.0,
        char_width: 9.0,
        char_height: 18.0,
        font_pixel_size: 16.0,
        image_scale_environment: Default::default(),
        window_system: true,
        background: 0x001A1A1A,
        vertical_border_fg: 0x00AAAAAA,
        zero_width_vertical_border_edge: neomacs_display_protocol::PresentedResizeEdge::Trailing,
        right_divider_width: 2,
        bottom_divider_width: 3,
        divider_fg: 0x00BBBBBB,
        divider_first_fg: 0x00CCCCCC,
        divider_last_fg: 0x00DDDDDD,
    };
    let cloned = fp.clone();
    assert_eq!(cloned.width, fp.width);
    assert_eq!(cloned.background, fp.background);
    assert_eq!(cloned.right_divider_width, fp.right_divider_width);
    assert_eq!(cloned.divider_fg, fp.divider_fg);
}

#[test]
fn frame_params_debug() {
    let fp = FrameParams {
        width: 800.0,
        height: 600.0,
        menu_bar_height: 0.0,
        tool_bar_height: 0.0,
        compact_bar_height: 0.0,
        tab_bar_height: 0.0,
        char_width: 8.0,
        char_height: 16.0,
        font_pixel_size: 14.0,
        image_scale_environment: Default::default(),
        window_system: false,
        background: 0,
        vertical_border_fg: 0,
        zero_width_vertical_border_edge: neomacs_display_protocol::PresentedResizeEdge::Trailing,
        right_divider_width: 0,
        bottom_divider_width: 0,
        divider_fg: 0,
        divider_first_fg: 0,
        divider_last_fg: 0,
    };
    let debug_str = format!("{:?}", fp);
    assert!(debug_str.contains("FrameParams"));
    assert!(debug_str.contains("800"));
}
