use super::*;

#[cfg(feature = "neo-term")]
use crate::terminal::content::{RenderCell, RenderCursor, TerminalContent};
#[cfg(feature = "neo-term")]
use neomacs_display_protocol::font::{
    FontFileAsset, FontOutlineAsset, FontReplay, FontSlantKind, ResolvedFont, ResolvedFontAdvance,
    ResolvedFontId, ResolvedFontIdentity,
};
#[cfg(feature = "neo-term")]
use rio_vt::crosswords::style::StyleFlags as CellFlags;

#[cfg(feature = "video")]
#[test]
fn video_service_keeps_targets_independent_across_presenters() {
    let now = std::time::Instant::now();
    let fast = neomacs_display_protocol::types::VideoId::new(1);
    let slow = neomacs_display_protocol::types::VideoId::new(2);

    let request = video_service_request(
        now,
        [
            (fast, std::time::Duration::from_millis(16)),
            (slow, std::time::Duration::from_millis(16)),
            (fast, std::time::Duration::from_millis(8)),
        ],
    );

    assert_eq!(
        request.timing_for(fast).target_presentation_time(),
        now + std::time::Duration::from_millis(8)
    );
    assert_eq!(
        request.timing_for(slow).target_presentation_time(),
        now + std::time::Duration::from_millis(16)
    );
}

#[cfg(feature = "neo-term")]
fn terminal_face_for_flags_and_font(
    flags: CellFlags,
    font_weight: u16,
    font_slant: FontSlantKind,
) -> Face {
    let mut frame = FrameGlyphBuffer::with_size(120.0, 80.0);
    frame.char_width = 10.0;
    frame.char_height = 20.0;
    frame.font_pixel_size = 18.0;
    let font_id = ResolvedFontId(42);
    let font_path = "/test-fixtures/terminal-font.ttf";
    let font = ResolvedFont {
        id: font_id,
        identity: ResolvedFontIdentity::from_file(font_path, 0, Some("TerminalFont".to_string())),
        replay: FontReplay::Swash {
            asset: FontOutlineAsset::File(
                FontFileAsset::new(font_path, 0).expect("valid terminal font asset"),
            ),
        },
        family: "Terminal Font".to_string(),
        full_name: Some("Terminal Font Regular".to_string()),
        postscript_name: Some("TerminalFont".to_string()),
        weight: font_weight,
        slant: font_slant,
        width: 5,
        pixel_size: 18.0,
        ascent_px: 14.0,
        descent_px: 4.0,
        space_advance_px: 10.0,
        glyph_advance: ResolvedFontAdvance::fixed_cell(10.0),
    };
    let mut default_face = Face::new(FaceId::new(0));
    default_face.default_resolved_font_id = Some(font_id);
    frame.faces.insert(default_face.id, default_face);
    frame.fonts.insert(font_id, font);
    frame.glyphs.push(FrameGlyph::Terminal {
        terminal_id: 7,
        x: 30.0,
        y: 40.0,
        width: 50.0,
        height: 20.0,
    });
    let contents = HashMap::from([(
        crate::terminal::TerminalId::new(7).expect("nonzero terminal id"),
        TerminalContent {
            cells: vec![RenderCell {
                col: 0,
                row: 0,
                c: 'x',
                fg: Color::WHITE,
                bg: Color::BLACK,
                flags,
            }],
            cols: 1,
            rows: 1,
            cursor: RenderCursor {
                col: 0,
                row: 0,
                visible: false,
            },
            default_bg: Color::BLACK,
            default_fg: Color::WHITE,
        },
    )]);

    let (glyphs, faces) = RenderApp::expanded_terminal_glyphs_for_frame(&frame, &contents);
    let face_id = glyphs
        .iter()
        .find_map(|glyph| match glyph {
            FrameGlyph::Char { face_id, .. } => Some(*face_id),
            _ => None,
        })
        .expect("terminal char face");
    faces
        .get(&face_id)
        .expect("synthesized terminal face")
        .clone()
}

#[cfg(feature = "neo-term")]
fn terminal_face_for_flags(flags: CellFlags) -> Face {
    terminal_face_for_flags_and_font(flags, 400, FontSlantKind::Normal)
}

#[cfg(feature = "neo-term")]
#[test]
fn bold_terminal_cell_does_not_replay_the_regular_font_as_exact() {
    let face = terminal_face_for_flags(CellFlags::BOLD);

    assert_eq!(face.default_resolved_font_id, None);
    assert_eq!(face.font_weight, 700);
    assert_eq!(face.font_family, "Terminal Font");
    assert_eq!(
        face.font_file_path.as_deref(),
        Some("/test-fixtures/terminal-font.ttf")
    );
    assert_eq!((face.font_ascent, face.font_descent), (14, 4));
}

#[cfg(feature = "neo-term")]
#[test]
fn italic_terminal_cell_does_not_replay_the_upright_font_as_exact() {
    let face = terminal_face_for_flags(CellFlags::ITALIC);

    assert_eq!(face.default_resolved_font_id, None);
    assert!(face.attributes.contains(FaceAttributes::ITALIC));
    assert_eq!(face.font_family, "Terminal Font");
    assert_eq!(
        face.font_file_path.as_deref(),
        Some("/test-fixtures/terminal-font.ttf")
    );
}

#[cfg(feature = "neo-term")]
#[test]
fn terminal_cell_replays_an_exact_font_that_already_satisfies_its_style() {
    let face = terminal_face_for_flags_and_font(
        CellFlags::BOLD | CellFlags::ITALIC,
        700,
        FontSlantKind::Italic,
    );

    assert_eq!(face.default_resolved_font_id, Some(ResolvedFontId(42)));
    assert_eq!(face.font_weight, 700);
    assert!(face.attributes.contains(FaceAttributes::ITALIC));
}

#[cfg(feature = "neo-term")]
#[test]
fn terminal_glyph_expansion_inherits_frame_font_identity() {
    let face = terminal_face_for_flags(CellFlags::empty());

    assert_eq!(face.default_resolved_font_id, Some(ResolvedFontId(42)));
    assert_eq!(face.font_family, "Terminal Font");
    assert_eq!(
        face.font_file_path.as_deref(),
        Some("/test-fixtures/terminal-font.ttf")
    );
    assert_eq!((face.font_ascent, face.font_descent), (14, 4));
}

#[cfg(feature = "neo-term")]
#[test]
fn terminal_face_interning_keeps_distinct_opacity_faces_distinct() {
    let content = TerminalContent {
        cells: vec![RenderCell {
            col: 0,
            row: 0,
            c: 'x',
            fg: Color::WHITE,
            bg: Color::BLACK,
            flags: CellFlags::empty(),
        }],
        cols: 1,
        rows: 1,
        cursor: RenderCursor {
            col: 0,
            row: 0,
            visible: false,
        },
        default_bg: Color::BLACK,
        default_fg: Color::WHITE,
    };
    let mut glyphs = Vec::new();
    let mut faces = HashMap::new();

    RenderApp::expand_terminal_cells(
        &content,
        0.0,
        0.0,
        10.0,
        20.0,
        14.0,
        18.0,
        None,
        TerminalPaintTarget::DETACHED_TEXT,
        1.0,
        &mut glyphs,
        &mut faces,
    );
    RenderApp::expand_terminal_cells(
        &content,
        20.0,
        0.0,
        10.0,
        20.0,
        14.0,
        18.0,
        None,
        TerminalPaintTarget::DETACHED_TEXT,
        0.5,
        &mut glyphs,
        &mut faces,
    );

    let face_ids: Vec<_> = glyphs
        .iter()
        .filter_map(FrameGlyph::face_id)
        .filter(|face_id| face_id.get() >= TERMINAL_FACE_ID_BASE)
        .collect();
    assert_eq!(face_ids.len(), 2);
    assert_ne!(face_ids[0], face_ids[1]);
    assert_eq!(faces[&face_ids[0]].foreground.a, 1.0);
    assert_eq!(faces[&face_ids[1]].foreground.a, 0.5);
}
