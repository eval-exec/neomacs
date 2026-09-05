use super::*;
use crate::neovm_bridge::RustBufferAccess;
use crate::scroll_policy::ScrollPolicy;
use crate::types::{DisplayLineNumbersMode, LineWrapMode, WindowKind, WindowParams};
use neomacs_display_protocol::types::Rect;
use neovm_core::buffer::{EmacsBytePos, EmacsByteRange};
use neovm_core::emacs_core::{Context, Value};

fn window_params() -> WindowParams {
    WindowParams {
        space_image_catalog: None,
        window_id: 1,
        buffer_id: 1,
        bounds: Rect::new(0.0, 8.0, 240.0, 120.0),
        text_bounds: Rect::new(16.0, 32.0, 160.0, 80.0),
        selected: true,
        cursor_role: crate::types::WindowCursorRole::Active,
        mode_line_active: true,
        kind: WindowKind::Main,
        left_col: 0,
        top_line: 0,
        window_start: 17,
        force_start: false,
        previous_visible_end: None,
        point: 21,
        buffer_size: 80,
        buffer_modiff: 0,
        buffer_begv: 3,
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

fn request(
    requested_window_start: i64,
    point_charpos: i64,
    accessible_end: i64,
    max_rows: usize,
    kind: WindowKind,
    scroll_policy: ScrollPolicy,
) -> BufferWindowSourceRequest {
    BufferWindowSourceRequest::new(
        requested_window_start,
        None,
        point_charpos,
        0,
        accessible_end,
        max_rows,
        kind,
        scroll_policy,
        0,
    )
}

fn byte_at_charpos(text: &'static [u8]) -> impl Fn(i64) -> Option<u8> {
    move |charpos| text.get(charpos as usize).copied()
}

#[test]
fn source_request_from_window_params_carries_source_bounds() {
    let params = window_params();
    let request = BufferWindowSourceRequest::from_window_params(&params, 6);

    assert_eq!(request.requested_window_start, 17);
    assert_eq!(request.point_charpos, 21);
    assert_eq!(request.accessible_start, 3);
    assert_eq!(request.accessible_end, 80);
    assert_eq!(request.max_rows, 6);
    assert!(!request.kind.is_minibuffer());
}

#[test]
fn source_request_from_window_params_resolves_the_scroll_policy() {
    let mut params = window_params();
    params.scroll_conservatively = 0;
    params.scroll_step = 3;

    let request = BufferWindowSourceRequest::from_window_params(&params, 6);

    assert_eq!(request.scroll_policy, ScrollPolicy::Step { lines: 3 });
}

#[test]
fn source_request_defers_folded_row_visibility_to_the_display_walk() {
    let mut eval = Context::new();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let hidden = (0..80)
        .map(|index| format!("hidden state {index}\n"))
        .collect::<String>();
    let text = format!("{}\n{hidden}after\n", "x".repeat(30));
    let hidden_start = 31;
    let point = text.find("after").expect("point after fold");
    {
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("current buffer");
        buffer.insert(&text);
        buffer.set_buffer_local(
            "buffer-invisibility-spec",
            Value::list(vec![Value::cons(Value::symbol("folded"), Value::T)]),
        );
        assert!(buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(EmacsBytePos::new(hidden_start), EmacsBytePos::new(point)),
            Value::symbol("invisible"),
            Value::symbol("folded"),
        ));
    }

    let request = BufferWindowSourceRequest::new(
        0,
        None,
        point as i64,
        0,
        text.len() as i64,
        4,
        WindowKind::Main,
        ScrollPolicy::Recenter,
        0,
    );
    let buffer = eval
        .buffer_manager()
        .get(buffer_id)
        .expect("current buffer");

    assert_eq!(
        request.resolve(&RustBufferAccess::new(buffer)).get(),
        0,
        "folded source lines must be measured by the canonical display walk before scrolling"
    );
}

#[test]
fn source_request_defers_replacing_display_spans_to_the_display_walk() {
    let mut eval = Context::new();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let replaced = (0..80)
        .map(|index| format!("replaced source line {index}\n"))
        .collect::<String>();
    let text = format!("{}\n{replaced}after\n", "x".repeat(30));
    let replaced_start = 31;
    let point = text.find("after").expect("point after replacement");
    {
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("current buffer");
        buffer.insert(&text);
        assert!(buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(EmacsBytePos::new(replaced_start), EmacsBytePos::new(point)),
            Value::symbol("display"),
            Value::string("replacement"),
        ));
    }

    let request = BufferWindowSourceRequest::new(
        0,
        None,
        point as i64,
        0,
        text.len() as i64,
        4,
        WindowKind::Main,
        ScrollPolicy::Recenter,
        0,
    );
    let buffer = eval
        .buffer_manager()
        .get(buffer_id)
        .expect("current buffer");

    assert_eq!(
        request.resolve(&RustBufferAccess::new(buffer)).get(),
        0,
        "replacing display spans must use their rendered rows before scrolling"
    );
}

// 6 single-letter lines; line N is the letter at charpos 2*(N-1).
const LINES6: &[u8] = b"a\nb\nc\nd\ne\nf\n";

#[test]
fn source_request_scrolls_back_when_start_is_past_remaining_content() {
    let resolved = request(
        10,
        10,
        LINES6.len() as i64,
        4,
        WindowKind::Main,
        ScrollPolicy::Recenter,
    )
    .resolve_window_start(byte_at_charpos(LINES6));

    // Start of line 4 ("d"), leaving max_rows/2 lines above point.
    assert_eq!(resolved, 6);
}

#[test]
fn source_request_scrolls_back_when_point_is_above_window_start() {
    let resolved = request(
        8,
        3,
        LINES6.len() as i64,
        4,
        WindowKind::Main,
        ScrollPolicy::Recenter,
    )
    .resolve_window_start(byte_at_charpos(LINES6));

    assert_eq!(resolved, 0);
}

// 16 single-letter lines: line N is the letter at charpos 2*(N-1), the newline
// at 2*(N-1)+1. Point at charpos 26 = line 14 ("n").
const LINES16: &[u8] = b"a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl\nm\nn\no\np\n";

#[test]
fn source_request_scrolls_one_line_when_point_steps_off_the_bottom() {
    // The #195 case: an 8-row window showing lines 1-8, point moved to line 9.
    // GNU `try_scrolling` scrolls by exactly dy (1 line), so the window start
    // moves to line 2 -- it does NOT jump a whole windowful.
    let resolved = request(
        0,
        16,
        LINES16.len() as i64,
        8,
        WindowKind::Main,
        ScrollPolicy::Unlimited,
    )
    .resolve_window_start(byte_at_charpos(LINES16));

    assert_eq!(resolved, 2, "one line down, not a page");
}

#[test]
fn source_request_window_start_lands_on_a_line_beginning() {
    // Every resolved start must be the first character of a line. A start on
    // the *newline* that ends the previous line renders as an empty leading row
    // and silently costs one line of text.
    for policy in [
        ScrollPolicy::Recenter,
        ScrollPolicy::Unlimited,
        ScrollPolicy::Conservative { max_lines: 20 },
        ScrollPolicy::Step { lines: 3 },
    ] {
        for point in 0..LINES16.len() as i64 {
            let resolved = request(0, point, LINES16.len() as i64, 8, WindowKind::Main, policy)
                .resolve_window_start(byte_at_charpos(LINES16));
            assert!(
                resolved == 0 || LINES16[resolved as usize - 1] == b'\n',
                "{policy:?} at point {point} resolved to {resolved}, mid-line"
            );
        }
    }
}

#[test]
fn source_request_recenters_far_forward_jump() {
    // GNU `try_scrolling` SCROLLING_FAILED -> `recenter:`: with the GNU default
    // scroll-conservatively=0 try_scrolling never runs at all, so any
    // off-screen point recenters -- window start goes max_rows/2 lines above
    // point, i.e. the start of line 10 for point on line 14.
    let resolved = request(
        0,
        26,
        LINES16.len() as i64,
        8,
        WindowKind::Main,
        ScrollPolicy::Recenter,
    )
    .resolve_window_start(byte_at_charpos(LINES16));

    assert_eq!(resolved, 18, "far jump centers point (max_rows/2 above)");
}

#[test]
fn source_request_near_forward_jump_does_not_recenter() {
    // A jump within `scroll-conservatively` scrolls minimally: GNU moves the
    // window start down by exactly dy (6 lines here), leaving point on the last
    // fully-visible row. Distinct from the recentered result (18) above.
    let resolved = request(
        0,
        26,
        LINES16.len() as i64,
        8,
        WindowKind::Main,
        ScrollPolicy::Conservative { max_lines: 20 },
    )
    .resolve_window_start(byte_at_charpos(LINES16));

    assert_eq!(resolved, 12);
}

#[test]
fn source_request_high_scroll_conservatively_never_recenters() {
    // scroll-conservatively above GNU's SCROLL_LIMIT (100) disables recentering;
    // even a far jump keeps the minimal forward-scroll (same result as the near
    // jump, not the recentered 18).
    let resolved = request(
        0,
        26,
        LINES16.len() as i64,
        8,
        WindowKind::Main,
        ScrollPolicy::Unlimited,
    )
    .resolve_window_start(byte_at_charpos(LINES16));

    assert_eq!(resolved, 12);
}

#[test]
fn source_request_does_not_forward_scroll_minibuffer() {
    let text = b"a\nb\nc\nd\ne\nf\ng\nh\n";
    let resolved = request(
        0,
        12,
        text.len() as i64,
        4,
        WindowKind::Minibuffer,
        ScrollPolicy::Recenter,
    )
    .resolve_window_start(byte_at_charpos(text));

    assert_eq!(resolved, 0);
}

#[test]
fn source_request_does_not_forward_scroll_degenerate_one_row_window() {
    // Regression: when a non-minibuffer window is transiently laid out at a
    // degenerate (<= 1 row) height — e.g. an intermediate/probe pass while a
    // posframe/child-frame or frame resize is in flight — point appears "far
    // below" the 1-row viewport, so the forward-scroll heuristic would scroll
    // window_start to point. That scrolled start then PERSISTS and corrupts the
    // real (tall) window: the Doom dashboard banner gets scrolled off-screen
    // when `SPC SPC` opens the project find-file posframe. A 1-row layout is not
    // a real scroll decision, so it must leave window_start unchanged.
    let text = b"a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl\nm\nn\no\np\nq\nr\ns\nt\nu\nv\nw\nx\ny\nz\n";
    let resolved = request(
        0,
        50,
        text.len() as i64,
        1,
        WindowKind::Main,
        ScrollPolicy::Recenter,
    )
    .resolve_window_start(byte_at_charpos(text));

    assert_eq!(
        resolved, 0,
        "a 1-row (degenerate) window must not forward-scroll past its start"
    );
}
