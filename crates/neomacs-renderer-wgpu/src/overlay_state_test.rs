use super::*;
const FONT_SIZE: f32 = 14.0;
const LINE_HEIGHT: f32 = 18.0;
const CHAR_WIDTH: f32 = FONT_SIZE * 0.6;

// 6. TooltipState
// -----------------------------------------------------------------------

#[test]
fn tooltip_basic_positioning() {
    let tt = TooltipState::new(
        100.0,
        100.0,
        "Hello",
        (1.0, 1.0, 1.0),
        (0.0, 0.0, 0.0),
        1920.0,
        1080.0,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    // Should be offset +10, +20 from cursor
    assert!((tt.x - 110.0).abs() < 0.01);
    assert!((tt.y - 120.0).abs() < 0.01);
}

#[test]
fn tooltip_bounds_match_position() {
    let tt = TooltipState::new(
        100.0,
        100.0,
        "Hello",
        (1.0, 1.0, 1.0),
        (0.0, 0.0, 0.0),
        1920.0,
        1080.0,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    assert!((tt.bounds.0 - tt.x).abs() < 0.01);
    assert!((tt.bounds.1 - tt.y).abs() < 0.01);
}

#[test]
fn tooltip_width_from_text() {
    let text = "Hello World"; // 11 chars
    let tt = TooltipState::new(
        0.0,
        0.0,
        text,
        (1.0, 1.0, 1.0),
        (0.0, 0.0, 0.0),
        1920.0,
        1080.0,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    let padding = 6.0_f32;
    let char_width = FONT_SIZE * 0.6;
    let expected_w = (11.0 * char_width + padding * 2.0).max(40.0);
    assert!((tt.bounds.2 - expected_w).abs() < 0.01);
}

#[test]
fn tooltip_minimum_width() {
    let tt = TooltipState::new(
        0.0,
        0.0,
        "X",
        (1.0, 1.0, 1.0),
        (0.0, 0.0, 0.0),
        1920.0,
        1080.0,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    assert!(tt.bounds.2 >= 40.0);
}

#[test]
fn tooltip_multiline_height() {
    let text = "Line1\nLine2\nLine3";
    let tt = TooltipState::new(
        0.0,
        0.0,
        text,
        (1.0, 1.0, 1.0),
        (0.0, 0.0, 0.0),
        1920.0,
        1080.0,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    let padding = 6.0_f32;
    let expected_h = 3.0 * LINE_HEIGHT + padding * 2.0;
    assert!((tt.bounds.3 - expected_h).abs() < 0.01);
    assert_eq!(tt.lines.len(), 3);
}

#[test]
fn tooltip_clamps_right_edge() {
    // Cursor near right edge of screen
    let screen_w = 500.0;
    let tt = TooltipState::new(
        490.0,
        100.0,
        "A long tooltip text",
        (1.0, 1.0, 1.0),
        (0.0, 0.0, 0.0),
        screen_w,
        1080.0,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    // x + width should not exceed screen_w
    assert!(
        tt.x + tt.bounds.2 <= screen_w,
        "tooltip right edge {} exceeds screen width {}",
        tt.x + tt.bounds.2,
        screen_w
    );
}

#[test]
fn tooltip_flips_above_when_near_bottom() {
    let screen_h = 200.0;
    let cursor_y = 190.0;
    let tt = TooltipState::new(
        100.0,
        cursor_y,
        "Tooltip text",
        (1.0, 1.0, 1.0),
        (0.0, 0.0, 0.0),
        1920.0,
        screen_h,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    // When tooltip doesn't fit below, it flips above: ty = y - h - 5.0
    assert!(
        tt.y < cursor_y,
        "tooltip y ({}) should be above cursor y ({})",
        tt.y,
        cursor_y
    );
}

#[test]
fn tooltip_clamps_negative_x() {
    let tt = TooltipState::new(
        -50.0,
        100.0,
        "A long text for width",
        (1.0, 1.0, 1.0),
        (0.0, 0.0, 0.0),
        1920.0,
        1080.0,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    // After the right-edge clamping, if tx is still negative, it's clamped to 0.
    // -50 + 10 = -40, which might be further adjusted. Should be >= 0.
    assert!(tt.x >= 0.0, "tooltip x should be >= 0 but was {}", tt.x);
}

#[test]
fn tooltip_clamps_negative_y() {
    // Cursor at top of tiny screen so flipping above goes negative.
    let tt = TooltipState::new(
        100.0,
        5.0,
        "Some text\nMore text\nEven more",
        (1.0, 1.0, 1.0),
        (0.0, 0.0, 0.0),
        1920.0,
        30.0,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    assert!(tt.y >= 0.0, "tooltip y should be >= 0 but was {}", tt.y);
}

#[test]
fn tooltip_preserves_colors() {
    let fg = (0.1, 0.2, 0.3);
    let bg = (0.4, 0.5, 0.6);
    let tt = TooltipState::new(
        0.0,
        0.0,
        "test",
        fg,
        bg,
        1920.0,
        1080.0,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    assert_eq!(tt.fg, fg);
    assert_eq!(tt.bg, bg);
}

#[test]
fn tooltip_empty_text() {
    // Empty string produces no lines from .lines(), so max_line_len falls back to 1.
    let tt = TooltipState::new(
        0.0,
        0.0,
        "",
        (1.0, 1.0, 1.0),
        (0.0, 0.0, 0.0),
        1920.0,
        1080.0,
        FONT_SIZE,
        LINE_HEIGHT,
        CHAR_WIDTH,
    );
    assert!(tt.lines.is_empty());
    // Height = 0 lines * line_height + 2*padding = 12.0
    let padding = 6.0_f32;
    assert!((tt.bounds.3 - 2.0 * padding).abs() < 0.01);
    // Width should be at least min (40.0)
    assert!(tt.bounds.2 >= 40.0);
}

// -----------------------------------------------------------------------
