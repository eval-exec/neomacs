use super::*;
use neomacs_display_protocol::CursorStyle;
use neomacs_display_protocol::types::DisplayWindowId;
use neomacs_display_protocol::types::Px;
use neomacs_display_protocol::{DisplaySlotId, PhysCursor};

#[test]
fn test_push_rect_single() {
    let mut vertices = Vec::new();
    let color = Color::new(1.0, 0.5, 0.25, 0.8);

    push_rect(&mut vertices, 10.0, 20.0, 30.0, 40.0, &color);

    // Should push exactly 6 vertices (2 triangles)
    assert_eq!(vertices.len(), 6);

    // Check positions form correct rectangle
    assert_eq!(vertices[0].position, [10.0, 20.0]); // top-left
    assert_eq!(vertices[1].position, [40.0, 20.0]); // top-right
    assert_eq!(vertices[2].position, [10.0, 60.0]); // bottom-left
    assert_eq!(vertices[3].position, [40.0, 20.0]); // top-right (triangle 2)
    assert_eq!(vertices[4].position, [40.0, 60.0]); // bottom-right
    assert_eq!(vertices[5].position, [10.0, 60.0]); // bottom-left (triangle 2)

    // Check color propagated to all vertices
    let expected_color = [1.0, 0.5, 0.25, 0.8];
    for vertex in &vertices {
        assert_eq!(vertex.color, expected_color);
    }
}

#[test]
fn test_push_rect_multiple() {
    let mut vertices = Vec::new();
    let color1 = Color::new(1.0, 0.0, 0.0, 1.0);
    let color2 = Color::new(0.0, 1.0, 0.0, 1.0);

    push_rect(&mut vertices, 0.0, 0.0, 10.0, 10.0, &color1);
    push_rect(&mut vertices, 20.0, 20.0, 15.0, 15.0, &color2);

    // Should have 12 vertices (6 per rect)
    assert_eq!(vertices.len(), 12);

    // Check first rect has red color
    for i in 0..6 {
        assert_eq!(vertices[i].color, [1.0, 0.0, 0.0, 1.0]);
    }

    // Check second rect has green color
    for i in 6..12 {
        assert_eq!(vertices[i].color, [0.0, 1.0, 0.0, 1.0]);
    }
}

#[test]
fn test_push_rect_zero_size() {
    let mut vertices = Vec::new();
    let color = Color::new(0.5, 0.5, 0.5, 1.0);

    push_rect(&mut vertices, 100.0, 100.0, 0.0, 0.0, &color);

    // Even zero-size rect pushes 6 vertices
    assert_eq!(vertices.len(), 6);

    // All vertices should be at the same position
    for vertex in &vertices {
        assert_eq!(vertex.position[0], 100.0);
        assert_eq!(vertex.position[1], 100.0);
    }
}

#[test]
fn test_push_rect_negative_coords() {
    let mut vertices = Vec::new();
    let color = Color::new(0.0, 0.0, 1.0, 0.5);

    push_rect(&mut vertices, -50.0, -30.0, 20.0, 15.0, &color);

    assert_eq!(vertices.len(), 6);
    assert_eq!(vertices[0].position, [-50.0, -30.0]); // top-left
    assert_eq!(vertices[1].position, [-30.0, -30.0]); // top-right
    assert_eq!(vertices[2].position, [-50.0, -15.0]); // bottom-left
    assert_eq!(vertices[4].position, [-30.0, -15.0]); // bottom-right
}

#[test]
fn test_find_cursor_pos_animated() {
    let animated = Some(AnimatedCursor {
        window_id: neomacs_display_protocol::types::DisplayWindowId::new(1),
        x: 100.0,
        y: 200.0,
        width: 10.0,
        height: 20.0,
        corners: None,
        frame_id: neomacs_display_protocol::types::DisplayFrameId::new(0),
    });
    let frame_glyphs = FrameGlyphBuffer::new();

    let result = find_cursor_pos(&animated, &frame_glyphs);

    assert_eq!(result, Some((100.0, 200.0, 10.0, 20.0)));
}

#[test]
fn test_find_cursor_pos_from_phys_cursor() {
    let animated = None;
    let mut frame_glyphs = FrameGlyphBuffer::new();

    frame_glyphs.set_phys_cursor(PhysCursor {
        window_id: neomacs_display_protocol::types::DisplayWindowId::new(1),
        charpos: 12,
        row: 3,
        col: 5,
        slot_id: DisplaySlotId::from_pixels(
            DisplayWindowId::new(1),
            Px(50.0),
            Px(60.0),
            Px(8.0),
            Px(16.0),
        ),
        x: 50.0,
        y: 60.0,
        width: 8.0,
        height: 16.0,
        ascent: 12.0,
        style: CursorStyle::FilledBox,
        color: Color::new(1.0, 1.0, 1.0, 1.0),
        cursor_fg: Color::new(0.0, 0.0, 0.0, 1.0),
    });

    let result = find_cursor_pos(&animated, &frame_glyphs);

    assert_eq!(result, Some((50.0, 60.0, 8.0, 16.0)));
}

#[test]
fn test_find_cursor_pos_prefers_phys_cursor() {
    let animated = None;
    let mut frame_glyphs = FrameGlyphBuffer::new();

    frame_glyphs.set_phys_cursor(PhysCursor {
        window_id: neomacs_display_protocol::types::DisplayWindowId::new(2),
        charpos: 20,
        row: 4,
        col: 7,
        slot_id: DisplaySlotId::from_pixels(
            DisplayWindowId::new(2),
            Px(30.0),
            Px(40.0),
            Px(2.0),
            Px(16.0),
        ),
        x: 30.0,
        y: 40.0,
        width: 2.0,
        height: 16.0,
        ascent: 12.0,
        style: CursorStyle::Bar(2.0),
        color: Color::new(1.0, 1.0, 1.0, 1.0),
        cursor_fg: Color::new(0.0, 0.0, 0.0, 1.0),
    });

    let result = find_cursor_pos(&animated, &frame_glyphs);

    assert_eq!(result, Some((30.0, 40.0, 2.0, 16.0)));
}

#[test]
fn test_find_cursor_pos_ignores_window_cursor_visuals_without_phys_cursor() {
    let animated = None;
    let mut frame_glyphs = FrameGlyphBuffer::new();

    frame_glyphs.add_cursor(
        DisplayWindowId::new(1),
        100.0,
        200.0,
        8.0,
        16.0,
        CursorStyle::FilledBox,
        Color::new(1.0, 1.0, 1.0, 1.0),
    );
    frame_glyphs.add_cursor(
        DisplayWindowId::new(2),
        300.0,
        400.0,
        8.0,
        16.0,
        CursorStyle::Hbar(2.0),
        Color::new(1.0, 1.0, 1.0, 1.0),
    );

    let result = find_cursor_pos(&animated, &frame_glyphs);

    assert_eq!(result, None);
}

#[test]
fn test_find_cursor_pos_none_found() {
    let animated = None;
    let mut frame_glyphs = FrameGlyphBuffer::new();

    frame_glyphs.add_cursor(
        DisplayWindowId::new(1),
        10.0,
        20.0,
        8.0,
        16.0,
        CursorStyle::Hollow,
        Color::new(1.0, 1.0, 1.0, 1.0),
    );

    let result = find_cursor_pos(&animated, &frame_glyphs);

    assert_eq!(result, None);
}

#[test]
fn test_find_cursor_pos_empty() {
    let animated = None;
    let frame_glyphs = FrameGlyphBuffer::new();

    let result = find_cursor_pos(&animated, &frame_glyphs);

    assert_eq!(result, None);
}
