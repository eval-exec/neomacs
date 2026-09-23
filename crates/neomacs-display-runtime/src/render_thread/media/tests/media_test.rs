use super::*;
use crate::core::frame_glyphs::FrameGlyphBuffer;
use crate::core::types::Color;
use crate::terminal::content::{RenderCell, RenderCursor, TerminalContent};
use rio_vt::crosswords::style::StyleFlags as CellFlags;

#[test]
fn terminal_glyph_expansion_uses_frame_metrics() {
    let mut frame = FrameGlyphBuffer::with_size(120.0, 80.0);
    frame.char_width = 10.0;
    frame.char_height = 20.0;
    frame.font_pixel_size = 18.0;
    frame.glyphs.push(FrameGlyph::Terminal {
        terminal_id: 7,
        x: 30.0,
        y: 40.0,
        width: 50.0,
        height: 20.0,
    });
    let mut contents = HashMap::new();
    contents.insert(
        crate::terminal::TerminalId::new(7).expect("nonzero terminal id"),
        TerminalContent {
            cells: vec![RenderCell {
                col: 1,
                row: 0,
                c: 'x',
                fg: Color::WHITE,
                bg: Color::BLACK,
                flags: CellFlags::empty(),
            }],
            cols: 2,
            rows: 1,
            cursor: RenderCursor {
                col: 0,
                row: 0,
                visible: false,
            },
            default_bg: Color::BLACK,
            default_fg: Color::WHITE,
        },
    );

    let (glyphs, faces) = RenderApp::expanded_terminal_glyphs_for_frame(&frame, &contents);

    assert!(matches!(
        glyphs.first(),
        Some(FrameGlyph::Stretch {
            x: 30.0,
            y: 40.0,
            width: 50.0,
            height: 20.0,
            ..
        })
    ));
    // Geometry stays on the glyph; the font size now lives on the
    // synthesized face referenced by the glyph's face_id.
    let Some(FrameGlyph::Char {
        char: ch,
        x,
        y,
        width,
        height,
        face_id,
        ..
    }) = glyphs.get(1)
    else {
        panic!("expected a Char glyph at index 1");
    };
    assert_eq!(*ch, 'x');
    assert_eq!(*x, 40.0);
    assert_eq!(*y, 40.0);
    assert_eq!(*width, 10.0);
    assert_eq!(*height, 20.0);
    assert_eq!(faces.get(face_id).expect("terminal face").font_size, 18.0);
}

#[test]
fn terminal_glyph_expansion_ignores_missing_terminal_content() {
    let mut frame = FrameGlyphBuffer::with_size(120.0, 80.0);
    frame.glyphs.push(FrameGlyph::Terminal {
        terminal_id: 7,
        x: 30.0,
        y: 40.0,
        width: 50.0,
        height: 20.0,
    });
    let contents = HashMap::new();

    let (glyphs, faces) = RenderApp::expanded_terminal_glyphs_for_frame(&frame, &contents);

    assert!(glyphs.is_empty());
    assert!(faces.is_empty());
}

#[test]
fn window_terminal_is_clipped_to_windows_displaying_its_owner_buffer() {
    let mut frame = FrameGlyphBuffer::with_size(300.0, 200.0);
    frame.char_width = 10.0;
    frame.char_height = 20.0;
    frame.font_pixel_size = 18.0;
    frame.add_window_info(
        DisplayWindowId::new(31),
        9,
        1,
        1,
        1,
        neomacs_display_protocol::presentation_origin::BufferModiff::default(),
        20.0,
        30.0,
        100.0,
        80.0,
        20.0,
        0.0,
        0.0,
        true,
        false,
        20.0,
        "*neo-term-1*".to_owned(),
        String::new(),
        false,
    );
    frame.add_window_info(
        DisplayWindowId::new(32),
        10,
        1,
        1,
        1,
        neomacs_display_protocol::presentation_origin::BufferModiff::default(),
        130.0,
        30.0,
        100.0,
        80.0,
        20.0,
        0.0,
        0.0,
        false,
        false,
        20.0,
        "*scratch*".to_owned(),
        String::new(),
        false,
    );
    let id = crate::terminal::TerminalId::new(7).unwrap();
    let contents = HashMap::from([(
        id,
        TerminalContent {
            cells: Vec::new(),
            cols: 10,
            rows: 3,
            cursor: RenderCursor {
                col: 0,
                row: 0,
                visible: false,
            },
            default_bg: Color::BLACK,
            default_fg: Color::WHITE,
        },
    )]);
    let targets = HashMap::from([(
        id,
        crate::terminal::TerminalDisplayTarget::Window {
            buffer: neovm_core::buffer::BufferId(9),
        },
    )]);

    let (glyphs, _) =
        RenderApp::expanded_window_terminals_for_frame(&frame, &[id], &contents, &targets);

    assert_eq!(glyphs.len(), 1);
    assert!(matches!(
        glyphs[0],
        FrameGlyph::Stretch {
            window_id,
            clip_rect: Some(Rect {
                x: 20.0,
                y: 30.0,
                width: 100.0,
                height: 60.0,
            }),
            x: 20.0,
            y: 30.0,
            width: 100.0,
            height: 60.0,
            ..
        } if window_id == DisplayWindowId::new(31)
    ));
}
