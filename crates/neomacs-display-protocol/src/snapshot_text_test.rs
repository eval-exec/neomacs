use crate::cursor::CursorStyle;
use crate::face::Face;
use crate::frame_glyphs::{DisplaySlotId, GlyphRowRole, PhysCursor, WindowInfo};
use crate::glyph_matrix::{
    FrameDisplayState, Glyph, GlyphArea, GlyphMatrix, GlyphType, WindowMatrixEntry,
};
use crate::types::FaceId;
use crate::types::{Color, DisplayFrameId, DisplayWindowId, Rect};

/// Two-row state exercising every glyph kind: chars, a stretch, an image,
/// a wide char with its padding cell, plus a mode-line row, a WindowInfo,
/// a named face, and a physical cursor.
fn golden_state() -> FrameDisplayState {
    let mut state = FrameDisplayState::new(16, 2, 8.0, 16.0);
    state.frame_placement = crate::PresentedFramePlacement::new(
        DisplayFrameId::new(1),
        state.presentation_id,
        None,
        crate::ParentFrameRect::new(0.0, 0.0, state.frame_pixel_width, state.frame_pixel_height)
            .unwrap(),
        0,
    );

    let mut default_face = Face::new(FaceId::new(0));
    default_face.lisp_name = Some("default".to_string());
    default_face.foreground = Color::WHITE;
    default_face.background = Color::BLACK;
    state.faces.insert(FaceId::new(0), default_face);

    let mut matrix = GlyphMatrix::new(2, 16);
    let text_area = GlyphArea::Text as usize;

    let row0 = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]);
    row0.enabled = true;
    row0.glyphs[text_area].push(Glyph::char('h', FaceId::new(0), 1));
    row0.glyphs[text_area].push(Glyph::char('i', FaceId::new(0), 2));
    row0.glyphs[text_area].push(Glyph::stretch(2, FaceId::new(0)));
    let image_margins = row0
        .intern_image_margins(crate::ImageMargins::default())
        .expect("image-margin token");
    row0.glyphs[text_area].push(Glyph {
        glyph_type: GlyphType::Image {
            source_rect: crate::ImageSourceRect::FULL,
            image_id: 7,
            width_cols: 1,
            margins: image_margins,
            opaque_background: crate::ImageOpaqueBackground::default(),
        },
        ..Glyph::char('x', FaceId::new(0), 3)
    });
    let mut wide = Glyph::char('你', FaceId::new(0), 4);
    wide.wide = true;
    row0.glyphs[text_area].push(wide);
    let mut padding = Glyph::char('你', FaceId::new(0), 4);
    padding.padding = true;
    row0.glyphs[text_area].push(padding);

    let row1 = crate::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[1]);
    row1.enabled = true;
    row1.role = GlyphRowRole::ModeLine;
    row1.mode_line = true;
    for (i, ch) in "-U:--".chars().enumerate() {
        row1.glyphs[text_area].push(Glyph::char(ch, FaceId::new(0), i));
    }

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 128.0, 32.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 128.0, 16.0),
        text_clip_bounds: None,
        selected: true,
    });

    state.window_infos.push(WindowInfo {
        window_id: DisplayWindowId::new(1),
        buffer_id: 9,
        buffer_name: "*scratch*".to_string(),
        window_start: 1,
        window_end: 12,
        buffer_size: 100,
        buffer_modiff: crate::presentation_origin::BufferModiff::default(),
        bounds: Rect::new(0.0, 0.0, 128.0, 32.0),
        geometry: Default::default(),
        mode_line_height: 16.0,
        header_line_height: 0.0,
        tab_line_height: 0.0,
        selected: true,
        is_minibuffer: false,
        char_height: 16.0,
        buffer_file_name: String::new(),
        modified: false,
    });

    state.phys_cursor = Some(PhysCursor {
        window_id: DisplayWindowId::new(1),
        charpos: 2,
        row: 0,
        col: 1,
        slot_id: DisplaySlotId::ZERO,
        x: 8.0,
        y: 0.0,
        width: 8.0,
        height: 16.0,
        ascent: 12.0,
        style: CursorStyle::FilledBox,
        color: Color::WHITE,
        cursor_fg: Color::BLACK,
    });

    state
}

#[test]
fn render_text_golden() {
    let state = golden_state();
    let expected = "\
=== frame 1: 16x2 cols 128x32 px ===
-- window 1 \"*scratch*\" bounds=(0,0 128x32)px text=(0,0 128x16)px start=1 end=12 selected --
   0|hi  [img:7]你
[mode-line]|-U:--
[cursor] window=1 row=0 col=1 charpos=2 style=FilledBox
";
    assert_eq!(state.render_text(), expected);
}

#[test]
fn render_text_faces_lists_runs_with_hex_colors() {
    let state = golden_state();
    let out = state.render_text_faces();
    // hi(2) + stretch(2) + [img:7](7) + 你(1, padding skipped) = 12 chars.
    assert!(
        out.contains("    : run 0-12 default fg=#FFFFFF bg=#000000"),
        "face run line missing:\n{out}"
    );
}

#[test]
fn render_text_skips_disabled_rows_and_falls_back_without_window_info() {
    let mut state = golden_state();
    state.window_infos.clear();
    crate::glyph_matrix::MatrixRow::make_mut(&mut state.window_matrices[0].matrix.rows[1])
        .enabled = false;
    let out = state.render_text();
    assert!(
        out.contains("-- window 1 selected --"),
        "fallback header:\n{out}"
    );
    assert!(
        !out.contains("[mode-line]"),
        "disabled row must not render:\n{out}"
    );
}

#[test]
fn face_run_name_falls_back_to_basic_face_and_raw_id() {
    let mut state = golden_state();
    // Unnamed face with a basic id resolves to the canonical basic name.
    let mut unnamed_basic = Face::new(FaceId::new(1)); // BasicFaceId::ModeLineActive
    unnamed_basic.lisp_name = None;
    state.faces.insert(FaceId::new(1), unnamed_basic);
    // Unnamed dynamic face falls back to face:<id>.
    state
        .faces
        .insert(FaceId::new(99), Face::new(FaceId::new(99)));
    let text_area = GlyphArea::Text as usize;
    let row =
        crate::glyph_matrix::MatrixRow::make_mut(&mut state.window_matrices[0].matrix.rows[1]);
    row.glyphs[text_area].push(Glyph::char('m', FaceId::new(1), 10));
    row.glyphs[text_area].push(Glyph::char('d', FaceId::new(99), 11));
    let out = state.render_text_faces();
    assert!(
        out.contains("mode-line-active fg=#"),
        "basic fallback:\n{out}"
    );
    assert!(out.contains("face:99 fg=#"), "raw-id fallback:\n{out}");
}
