use super::*;
use crate::BasicFaceId;
use crate::TransitionDirection;
use crate::WebViewId;

// -----------------------------------------------------------------------
// Helper: assert a Color matches expected RGBA (with tolerance for floats)
// -----------------------------------------------------------------------
fn assert_color_eq(actual: &Color, expected: &Color) {
    assert!(
        (actual.r - expected.r).abs() < 1e-5
            && (actual.g - expected.g).abs() < 1e-5
            && (actual.b - expected.b).abs() < 1e-5
            && (actual.a - expected.a).abs() < 1e-5,
        "Colors differ: actual {:?} vs expected {:?}",
        actual,
        expected,
    );
}

#[test]
fn frame_default_resolved_font_is_one_coherent_binding_lookup() {
    use crate::font::{
        FontFileAsset, FontOutlineAsset, FontReplay, FontResolutionSource, FontSlantKind,
        ResolvedFont, ResolvedFontAdvance, ResolvedFontId, ResolvedFontIdentity,
    };

    let mut frame = FrameGlyphBuffer::with_size(120.0, 80.0);
    let font_id = ResolvedFontId(42);
    let mut default_face = Face::new(BasicFaceId::Default.into());
    default_face.default_resolved_font_id = Some(font_id);
    frame.faces.insert(default_face.id, default_face);
    frame.fonts.insert(
        font_id,
        ResolvedFont {
            id: font_id,
            identity: ResolvedFontIdentity::from_file("/test-fixtures/default-font.ttf", 0, None),
            replay: FontReplay::Swash {
                asset: FontOutlineAsset::File(
                    FontFileAsset::new("/test-fixtures/default-font.ttf", 0)
                        .expect("valid test font asset"),
                ),
            },
            family: "Default Font".to_string(),
            full_name: None,
            postscript_name: None,
            weight: 400,
            slant: FontSlantKind::Normal,
            width: 5,
            pixel_size: 18.0,
            ascent_px: 14.0,
            descent_px: 4.0,
            space_advance_px: 10.0,
            glyph_advance: ResolvedFontAdvance::fixed_cell(10.0),
            source: FontResolutionSource::FacePrimary,
        },
    );

    let resolved = frame
        .default_resolved_font()
        .expect("default face and font table should resolve together");
    assert_eq!(resolved.id, font_id);
    assert_eq!(resolved.family, "Default Font");

    frame.fonts.remove(&font_id);
    assert!(
        frame.default_resolved_font().is_none(),
        "a stale face id must not escape as a partial binding"
    );
}

fn make_window_info(window_id: i64, buffer_id: u64, window_start: i64, bounds: Rect) -> WindowInfo {
    let mode_line_height = 20.0;
    WindowInfo {
        window_id: DisplayWindowId::new(window_id),
        buffer_id,
        buffer_name: String::new(),
        window_start,
        window_end: window_start + 200,
        buffer_size: 10_000,
        buffer_modiff: crate::presentation_origin::BufferModiff::default(),
        bounds,
        geometry: PresentedWindowGeometry::Complete {
            cell_origin: PresentedCellOrigin::default(),
            regions: PresentedWindowRegions {
                outer: bounds,
                text_body: Rect::new(
                    bounds.x,
                    bounds.y,
                    bounds.width,
                    bounds.height - mode_line_height,
                ),
                mode_line: Some(Rect::new(
                    bounds.x,
                    bounds.y + bounds.height - mode_line_height,
                    bounds.width,
                    mode_line_height,
                )),
                ..PresentedWindowRegions::default()
            },
        },
        mode_line_height,
        header_line_height: 0.0,
        tab_line_height: 0.0,
        selected: false,
        is_minibuffer: false,
        char_height: 16.0,
        buffer_file_name: String::new(),
        modified: false,
    }
}

// =======================================================================
// new() - initial state
// =======================================================================

#[test]
fn new_creates_empty_buffer() {
    let buf = FrameGlyphBuffer::new();
    assert!(buf.glyphs.is_empty());
    assert!(buf.window_infos.is_empty());
    assert!(buf.faces.is_empty());
    assert!(buf.active_cursor().is_none());
}

#[test]
fn new_has_correct_defaults() {
    let buf = FrameGlyphBuffer::new();
    assert_eq!(buf.width, 0.0);
    assert_eq!(buf.height, 0.0);
    assert_eq!(buf.char_width, 8.0);
    assert_eq!(buf.char_height, 16.0);
    assert_eq!(buf.font_pixel_size, 14.0);
    assert_color_eq(&buf.background, &Color::BLACK);
    assert_eq!(buf.frame_placement.frame().get(), 0);
    assert_eq!(buf.frame_placement.parent(), None);
    assert_eq!(buf.background_alpha, 1.0);
    assert!(!buf.no_accept_focus);
}

#[test]
fn new_is_empty_and_len_zero() {
    let buf = FrameGlyphBuffer::new();
    assert!(buf.is_empty());
    assert_eq!(buf.len(), 0);
}

// =======================================================================
// with_size()
// =======================================================================

#[test]
fn with_size_sets_dimensions() {
    let buf = FrameGlyphBuffer::with_size(1920.0, 1080.0);
    assert_eq!(buf.width, 1920.0);
    assert_eq!(buf.height, 1080.0);
    // Everything else should match new()
    assert!(buf.glyphs.is_empty());
    assert_eq!(buf.char_width, 8.0);
}

// =======================================================================
// clear_all()
// =======================================================================

#[test]
fn clear_all_resets_glyphs_and_metadata() {
    let mut buf = FrameGlyphBuffer::new();

    // Populate some data
    buf.add_char('A', 0.0, 0.0, 8.0, 16.0, 12.0, false);
    buf.add_stretch(0.0, 0.0, 100.0, 16.0, Color::RED, FaceId::new(0), false);
    buf.add_cursor(
        DisplayWindowId::new(1),
        10.0,
        20.0,
        2.0,
        16.0,
        CursorStyle::Bar(2.0),
        Color::WHITE,
    );
    buf.add_window_info(
        DisplayWindowId::new(1),
        100,
        0,
        500,
        1000,
        crate::presentation_origin::BufferModiff::default(),
        0.0,
        0.0,
        800.0,
        600.0,
        20.0,
        0.0,
        0.0,
        true,
        false,
        16.0,
        String::new(),
        "test.rs".to_string(),
        false,
    );
    buf.set_phys_cursor(PhysCursor {
        window_id: DisplayWindowId::new(1),
        charpos: 10,
        row: 1,
        col: 2,
        slot_id: DisplaySlotId::from_pixels(
            DisplayWindowId::new(1),
            Px(10.0),
            Px(20.0),
            Px(buf.char_width),
            Px(buf.char_height),
        ),
        x: 10.0,
        y: 20.0,
        width: 8.0,
        height: 16.0,
        ascent: 12.0,
        style: CursorStyle::FilledBox,
        color: Color::WHITE,
        cursor_fg: Color::BLACK,
    });
    assert!(!buf.glyphs.is_empty());
    assert!(!buf.window_infos.is_empty());

    buf.clear_all();

    assert!(buf.glyphs.is_empty());
    assert!(buf.window_infos.is_empty());
    assert!(buf.transition_hints.is_empty());
    assert!(buf.effect_hints.is_empty());
    assert!(buf.active_cursor().is_none());
    assert!(buf.window_cursors.is_empty());
    assert!(buf.faces.is_empty());
}

#[test]
fn clear_all_preserves_frame_dimensions() {
    let mut buf = FrameGlyphBuffer::with_size(1920.0, 1080.0);
    buf.background = Color::BLUE;
    buf.add_char('X', 0.0, 0.0, 8.0, 16.0, 12.0, false);

    buf.clear_all();

    // Dimensions and background should survive clear_all
    assert_eq!(buf.width, 1920.0);
    assert_eq!(buf.height, 1080.0);
    assert_color_eq(&buf.background, &Color::BLUE);
}

#[test]
fn take_runtime_hints_drains_transition_and_effect_hints() {
    let mut buf = FrameGlyphBuffer::new();
    let region = PresentedWindowRegions {
        text_body: Rect::new(0.0, 0.0, 100.0, 100.0),
        ..PresentedWindowRegions::default()
    }
    .buffer_viewport()
    .unwrap();
    buf.add_transition_hint(ContentTransitionHint::BufferReplaced {
        target: BufferTransitionTarget::Window {
            window_id: DisplayWindowId::new(1),
            region,
        },
        intent: ContentTransitionIntent::Replace,
    });
    buf.add_effect_hint(WindowEffectHint::TextFadeIn {
        window_id: DisplayWindowId::new(1),
        bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
    });

    let (transition_hints, effect_hints) = buf.take_runtime_hints();
    assert_eq!(transition_hints.len(), 1);
    assert_eq!(effect_hints.len(), 1);
    assert!(buf.transition_hints.is_empty());
    assert!(buf.effect_hints.is_empty());
}

#[test]
fn set_face_with_font_registers_baseline_render_face() {
    let mut buf = FrameGlyphBuffer::new();
    let fg = Color::rgb(0.8, 0.7, 0.6);
    let bg = Color::rgb(0.1, 0.2, 0.3);
    let ul = Color::rgb(0.9, 0.1, 0.2);

    buf.set_face_with_font(
        FaceId::new(42),
        fg,
        Some(bg),
        "DejaVu Sans",
        700,
        true,
        18.0,
        3,
        Some(ul),
        1,
        None,
        0,
        None,
        false,
    );

    let face = buf
        .faces
        .get(&FaceId::new(42))
        .expect("face entry should exist");
    assert_eq!(face.id, FaceId::new(42));
    assert_eq!(face.font_family, "DejaVu Sans");
    assert_eq!(face.font_size, 18.0);
    assert_eq!(face.font_weight, 700);
    assert!(face.attributes.contains(FaceAttributes::BOLD));
    assert!(face.attributes.contains(FaceAttributes::ITALIC));
    assert!(face.attributes.contains(FaceAttributes::UNDERLINE));
    assert!(face.attributes.contains(FaceAttributes::STRIKE_THROUGH));
    assert_eq!(face.underline_style, UnderlineStyle::Wave);
    assert_eq!(face.underline_color, Some(ul));
    assert_color_eq(&face.foreground, &fg);
    assert_color_eq(&face.background, &bg);
}

#[test]
fn render_face_treats_face_ids_as_frame_local() {
    let mut root = FrameGlyphBuffer::new();
    let mut child = FrameGlyphBuffer::new();

    let mut root_face = Face::new(FaceId::new(7));
    root_face.font_family = "Root Mono".to_string();
    root_face.font_size = 11.0;
    root.faces.insert(FaceId::new(7), root_face);

    let mut child_face = Face::new(FaceId::new(7));
    child_face.font_family = "Child Mono".to_string();
    child_face.font_size = 23.0;
    child.faces.insert(FaceId::new(7), child_face);

    let root_render_face = root.render_face(FaceId::new(7)).expect("root face");
    let child_render_face = child.render_face(FaceId::new(7)).expect("child face");

    assert_eq!(root_render_face.font_family, "Root Mono");
    assert_eq!(root_render_face.font_size, 11.0);
    assert_eq!(child_render_face.font_family, "Child Mono");
    assert_eq!(child_render_face.font_size, 23.0);
}

#[test]
fn set_face_uses_current_font_context_for_face_entry() {
    let mut buf = FrameGlyphBuffer::new();
    let fg = Color::rgb(0.4, 0.5, 0.6);

    buf.set_face_with_font(
        FaceId::new(1),
        fg,
        None,
        "Iosevka",
        400,
        false,
        15.0,
        0,
        None,
        0,
        None,
        0,
        None,
        false,
    );
    buf.set_face(
        FaceId::new(2),
        fg,
        None,
        600,
        true,
        0,
        None,
        0,
        None,
        1,
        None,
    );

    let face = buf
        .faces
        .get(&FaceId::new(2))
        .expect("face entry should exist");
    assert_eq!(face.font_family, "Iosevka");
    assert_eq!(face.font_size, 15.0);
    assert_eq!(face.font_weight, 600);
    assert!(face.attributes.contains(FaceAttributes::ITALIC));
    assert!(face.attributes.contains(FaceAttributes::OVERLINE));
}

// =======================================================================
// add_char()
// =======================================================================

#[test]
fn add_char_appends_char_glyph() {
    let mut buf = FrameGlyphBuffer::new();
    buf.add_char('H', 10.0, 20.0, 8.0, 16.0, 12.0, false);

    assert_eq!(buf.len(), 1);
    match &buf.glyphs[0] {
        FrameGlyph::Char {
            char: ch,
            x,
            y,
            width,
            height,
            ascent,
            composed,
            ..
        } => {
            assert_eq!(*ch, 'H');
            assert_eq!(*x, 10.0);
            assert_eq!(*y, 20.0);
            assert_eq!(*width, 8.0);
            assert_eq!(*height, 16.0);
            assert_eq!(*ascent, 12.0);
            assert!(!buf.glyphs[0].is_overlay());
            assert!(composed.is_none());
        }
        other => panic!("Expected Char glyph, got {:?}", other),
    }
}

#[test]
fn cell_rect_returns_glyph_cell_and_none_for_non_cells() {
    let mut buf = FrameGlyphBuffer::new();
    buf.set_draw_context(DisplayWindowId::new(1), GlyphRowRole::Text, None);
    buf.add_char('A', 30.0, 40.0, 18.0, 33.0, 26.0, false);
    assert_eq!(buf.glyphs[0].cell_rect(), Some((30.0, 40.0, 18.0, 33.0)));
    assert_eq!(buf.glyphs[0].cell_x(), Some(30.0));

    // A border occupies no cursor cell.
    buf.add_border(0.0, 0.0, 100.0, 100.0, Color::BLACK);
    let border = buf.glyphs.last().expect("border glyph");
    assert_eq!(border.cell_rect(), None);
    assert_eq!(border.cell_x(), None);
}

#[test]
fn cursor_cell_rect_resolves_slot_glyph_else_fallback() {
    let mut buf = FrameGlyphBuffer::new();
    buf.set_draw_context(DisplayWindowId::new(1), GlyphRowRole::Text, None);
    buf.add_char('A', 30.0, 40.0, 18.0, 33.0, 26.0, false);
    let slot = buf.glyphs[0].slot_id().expect("char glyph has a slot");

    // Slot occupied -> the glyph's actual cell, never the grid fallback.
    assert_eq!(
        buf.cursor_cell_rect(slot, (999.0, 999.0, 1.0, 1.0)),
        (30.0, 40.0, 18.0, 33.0)
    );

    // No glyph on the slot -> the layout-supplied fallback rect.
    let empty_slot = DisplaySlotId {
        window_id: DisplayWindowId::new(7),
        row: 9,
        col: 9,
    };
    assert_eq!(
        buf.cursor_cell_rect(empty_slot, (5.0, 6.0, 7.0, 8.0)),
        (5.0, 6.0, 7.0, 8.0)
    );
}

#[test]
fn cursor_draw_rect_box_adopts_cell_while_bar_keeps_width() {
    let mut buf = FrameGlyphBuffer::new();
    buf.set_draw_context(DisplayWindowId::new(1), GlyphRowRole::Text, None);
    buf.add_char('A', 30.0, 40.0, 18.0, 33.0, 26.0, false);
    let slot = buf.glyphs[0].slot_id().expect("char glyph has a slot");

    // A box cursor adopts the glyph's actual cell x and full width, and derives
    // its top y from the glyph baseline (40 + 26 = 66) minus the cursor ascent
    // (26) = 40 -- so it lands on the glyph, not the grid-approximate fallback y.
    // height still comes from the fallback (the layout's computed cursor height).
    assert_eq!(
        buf.cursor_draw_rect(slot, CursorStyle::FilledBox, 26.0, (5.0, 41.0, 2.0, 33.0)),
        (30.0, 40.0, 18.0, 33.0)
    );

    // A bar cursor adopts the same derived cell x/y but keeps its thin fallback width.
    assert_eq!(
        buf.cursor_draw_rect(slot, CursorStyle::Bar(2.0), 26.0, (5.0, 41.0, 2.0, 33.0)),
        (30.0, 40.0, 2.0, 33.0)
    );
}

#[test]
fn cursor_draw_rect_media_spans_whole_glyph_else_fallback() {
    let mut buf = FrameGlyphBuffer::new();
    buf.add_image(ImageId::new(9), 24.0, 48.0, 128.0, 96.0);
    let slot = buf.glyphs[0].slot_id().expect("image glyph has a slot");

    // A cursor over an image covers the whole image rect, ignoring the
    // grid-derived fallback geometry entirely.
    assert_eq!(
        buf.cursor_draw_rect(slot, CursorStyle::Hollow, 0.0, (1.0, 2.0, 3.0, 4.0)),
        (24.0, 48.0, 128.0, 96.0)
    );

    // An unoccupied slot falls back to the layout's grid geometry unchanged.
    let empty = DisplaySlotId {
        window_id: DisplayWindowId::new(7),
        row: 9,
        col: 9,
    };
    assert_eq!(
        buf.cursor_draw_rect(empty, CursorStyle::FilledBox, 0.0, (5.0, 6.0, 7.0, 8.0)),
        (5.0, 6.0, 7.0, 8.0)
    );
}

#[test]
fn frame_reports_the_distinct_image_assets_its_presentation_keeps_alive() {
    let mut frame = FrameGlyphBuffer::new();
    frame.add_image(ImageId::new(9), 0.0, 0.0, 16.0, 16.0);
    frame.add_image(ImageId::new(7), 16.0, 0.0, 16.0, 16.0);
    frame.add_image(ImageId::new(9), 32.0, 0.0, 16.0, 16.0);

    let retained = frame
        .referenced_images()
        .collect::<crate::RetainedImageSet>();
    let mut images = retained.iter().collect::<Vec<_>>();
    images.sort_unstable();

    assert_eq!(images, [ImageId::new(7), ImageId::new(9)]);
}

#[test]
fn cursor_draw_rect_image_uses_margin_inclusive_slot() {
    let mut buf = FrameGlyphBuffer::new();
    buf.add_image(ImageId::new(9), 24.0, 48.0, 128.0, 96.0);
    let slot = buf.glyphs[0].slot_id().expect("image glyph has a slot");
    let FrameGlyph::Image {
        slot_rect,
        x,
        y,
        width,
        height,
        ..
    } = &mut buf.glyphs[0]
    else {
        unreachable!("add_image emits an image")
    };
    *slot_rect = Rect::new(20.0, 44.0, 136.0, 104.0);
    *x = 24.0;
    *y = 48.0;
    *width = 128.0;
    *height = 96.0;

    assert_eq!(
        buf.cursor_draw_rect(slot, CursorStyle::FilledBox, 0.0, (1.0, 2.0, 3.0, 4.0)),
        (20.0, 44.0, 136.0, 104.0)
    );
}

#[test]
fn cursor_draw_rect_rtl_bar_shifts_to_right_edge() {
    let mut buf = FrameGlyphBuffer::new();
    buf.set_draw_context(DisplayWindowId::new(1), GlyphRowRole::Text, None);
    buf.add_char('\u{0627}', 30.0, 40.0, 18.0, 33.0, 26.0, false);
    // Mark the glyph right-to-left (odd bidi level); add_char defaults to 0.
    if let FrameGlyph::Char { bidi_level, .. } = &mut buf.glyphs[0] {
        *bidi_level = 1;
    }
    let slot = buf.glyphs[0].slot_id().expect("char glyph has a slot");

    // A 2px bar on an 18px RTL cell sits at the cell's right edge:
    // 30 + (18 - 2) = 46, so the caret leads the character as it should in RTL.
    let (x, _y, w, _h) =
        buf.cursor_draw_rect(slot, CursorStyle::Bar(2.0), 26.0, (30.0, 40.0, 2.0, 33.0));
    assert_eq!((x, w), (46.0, 2.0));
}

#[test]
fn cursor_draw_rect_empty_slot_snaps_to_preceding_glyph_right_edge() {
    let mut buf = FrameGlyphBuffer::new();
    buf.set_draw_context(DisplayWindowId::new(1), GlyphRowRole::Text, None);
    // A line-number gutter glyph; the cursor targets the empty text cell that
    // begins where this glyph ends (a blank line has no glyph of its own there).
    buf.add_char('2', 40.0, 10.0, 18.0, 33.0, 26.0, false); // right edge x = 58
    let gutter = buf.glyphs[0].slot_id().expect("char slot");
    let empty = DisplaySlotId {
        window_id: gutter.window_id,
        row: gutter.row,
        col: gutter.col + 1,
    };

    // With no glyph on the slot, the cursor snaps to the gutter glyph's right
    // edge (58) -- flush with the text column -- not the grid-approximate
    // fallback x (5), which would land it back inside the line-number gutter.
    let (x, _y, _w, _h) =
        buf.cursor_draw_rect(empty, CursorStyle::FilledBox, 0.0, (5.0, 11.0, 9.0, 33.0));
    assert_eq!(x, 58.0);

    // A slot with nothing before it (no gutter) keeps the layout fallback x.
    let lonely = DisplaySlotId {
        window_id: DisplayWindowId::new(9),
        row: 9,
        col: 3,
    };
    let (lx, _, _, _) =
        buf.cursor_draw_rect(lonely, CursorStyle::FilledBox, 0.0, (5.0, 6.0, 7.0, 8.0));
    assert_eq!(lx, 5.0);
}

#[test]
fn add_char_uses_current_face_attributes() {
    let mut buf = FrameGlyphBuffer::new();
    let fg = Color::rgb(1.0, 0.0, 0.0);
    let bg = Color::rgb(0.0, 0.0, 1.0);
    buf.set_face(
        FaceId::new(42),
        fg,
        Some(bg),
        700,
        true,
        1,
        Some(Color::GREEN), // underline
        1,
        Some(Color::RED), // strike-through
        1,
        Some(Color::BLUE), // overline
    );
    buf.set_draw_context(DisplayWindowId::new(1), GlyphRowRole::ModeLine, None);
    buf.add_char('X', 0.0, 0.0, 8.0, 16.0, 12.0, true);

    match &buf.glyphs[0] {
        FrameGlyph::Char { face_id, .. } => {
            assert_eq!(*face_id, FaceId::new(42));
            // The face-derived attributes are resolved from the face table.
            let rf = buf.resolved_face(*face_id);
            assert_color_eq(&rf.fg, &fg);
            assert_eq!(rf.bg, bg);
            assert_eq!(rf.font_weight, 700);
            assert!(rf.italic);
            assert_eq!(rf.underline, UnderlineStyle::Line);
            assert_eq!(rf.underline_color, Some(Color::GREEN));
            assert!(rf.strike_through);
            assert_eq!(rf.strike_through_color, Some(Color::RED));
            assert!(rf.overline);
            assert_eq!(rf.overline_color, Some(Color::BLUE));
            assert!(buf.glyphs[0].is_overlay());
        }
        other => panic!("Expected Char glyph, got {:?}", other),
    }
}

#[test]
fn add_char_multiple_appends_in_order() {
    let mut buf = FrameGlyphBuffer::new();
    buf.add_char('A', 0.0, 0.0, 8.0, 16.0, 12.0, false);
    buf.add_char('B', 8.0, 0.0, 8.0, 16.0, 12.0, false);
    buf.add_char('C', 16.0, 0.0, 8.0, 16.0, 12.0, false);

    assert_eq!(buf.len(), 3);
    let chars: Vec<char> = buf
        .glyphs
        .iter()
        .map(|g| match g {
            FrameGlyph::Char { char: ch, .. } => *ch,
            _ => panic!("Expected Char"),
        })
        .collect();
    assert_eq!(chars, vec!['A', 'B', 'C']);
}

#[test]
fn add_char_overlay_flag() {
    let mut buf = FrameGlyphBuffer::new();
    buf.set_draw_context(DisplayWindowId::new(1), GlyphRowRole::ModeLine, None);
    buf.add_char('M', 0.0, 0.0, 8.0, 16.0, 12.0, true);
    assert!(buf.glyphs[0].is_overlay());

    buf.set_draw_context(DisplayWindowId::new(1), GlyphRowRole::Text, None);
    buf.add_char('N', 0.0, 0.0, 8.0, 16.0, 12.0, false);
    assert!(!buf.glyphs[1].is_overlay());
}

// =======================================================================
// add_composed_char()
// =======================================================================

#[test]
fn add_composed_char_stores_text_and_base() {
    let mut buf = FrameGlyphBuffer::new();
    // Emoji ZWJ sequence: family emoji
    let composed_text = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
    buf.add_composed_char(
        composed_text,
        '\u{1F468}',
        0.0,
        0.0,
        24.0,
        16.0,
        12.0,
        false,
    );

    assert_eq!(buf.len(), 1);
    match &buf.glyphs[0] {
        FrameGlyph::Char {
            char: ch,
            composed,
            width,
            ..
        } => {
            assert_eq!(*ch, '\u{1F468}');
            assert!(composed.is_some());
            assert_eq!(&**composed.as_ref().unwrap(), composed_text);
            assert_eq!(*width, 24.0);
        }
        other => panic!("Expected Char glyph, got {:?}", other),
    }
}

#[test]
fn add_composed_char_uses_current_face() {
    let mut buf = FrameGlyphBuffer::new();
    let fg = Color::rgb(0.5, 0.5, 0.5);
    buf.set_face(
        FaceId::new(10),
        fg,
        None,
        400,
        false,
        0,
        None,
        0,
        None,
        0,
        None,
    );
    buf.add_composed_char("e\u{0301}", 'e', 0.0, 0.0, 8.0, 16.0, 12.0, false);

    match &buf.glyphs[0] {
        FrameGlyph::Char { face_id, .. } => {
            assert_eq!(*face_id, FaceId::new(10));
            // Colors resolve from the face; a `None` background synthesizes a
            // transparent face background.
            let rf = buf.resolved_face(*face_id);
            assert_color_eq(&rf.fg, &fg);
            assert_eq!(rf.bg, Color::TRANSPARENT);
        }
        other => panic!("Expected Char glyph, got {:?}", other),
    }
}

// =======================================================================
// add_cursor()
// =======================================================================

#[test]
fn add_cursor_appends_window_cursor_visual() {
    let mut buf = FrameGlyphBuffer::new();
    let cursor_color = Color::rgb(0.0, 1.0, 0.0);
    buf.add_cursor(
        DisplayWindowId::new(42),
        100.0,
        200.0,
        2.0,
        16.0,
        CursorStyle::Bar(2.0),
        cursor_color,
    );

    assert!(buf.glyphs.is_empty());
    assert_eq!(buf.window_cursors.len(), 1);
    let cursor = &buf.window_cursors[0];
    assert_eq!(cursor.window_id.get(), 42);
    assert_eq!(
        cursor.slot_id,
        DisplaySlotId::from_pixels(
            DisplayWindowId::new(42),
            Px(100.0),
            Px(200.0),
            Px(8.0),
            Px(16.0)
        )
    );
    assert_eq!(cursor.x, 100.0);
    assert_eq!(cursor.y, 200.0);
    assert_eq!(cursor.width, 2.0);
    assert_eq!(cursor.height, 16.0);
    assert_eq!(cursor.style, CursorStyle::Bar(2.0));
    assert_color_eq(&cursor.color, &cursor_color);
}

#[test]
fn add_cursor_all_styles() {
    let mut buf = FrameGlyphBuffer::new();
    let c = Color::WHITE;
    buf.add_cursor(
        DisplayWindowId::new(1),
        0.0,
        0.0,
        8.0,
        16.0,
        CursorStyle::FilledBox,
        c,
    );
    buf.add_cursor(
        DisplayWindowId::new(1),
        0.0,
        0.0,
        8.0,
        16.0,
        CursorStyle::Bar(2.0),
        c,
    );
    buf.add_cursor(
        DisplayWindowId::new(1),
        0.0,
        0.0,
        8.0,
        16.0,
        CursorStyle::Hbar(2.0),
        c,
    );
    buf.add_cursor(
        DisplayWindowId::new(1),
        0.0,
        0.0,
        8.0,
        16.0,
        CursorStyle::Hollow,
        c,
    );

    assert!(buf.glyphs.is_empty());
    assert_eq!(buf.window_cursors.len(), 4);
    let expected = [
        CursorStyle::FilledBox,
        CursorStyle::Bar(2.0),
        CursorStyle::Hbar(2.0),
        CursorStyle::Hollow,
    ];
    for (i, expected_style) in expected.iter().enumerate() {
        assert_eq!(
            buf.window_cursors[i].style, *expected_style,
            "Cursor {} has wrong style",
            i
        );
    }
}

#[test]
fn cursor_visual_is_not_counted_as_overlay_glyph() {
    let mut buf = FrameGlyphBuffer::new();
    buf.add_cursor(
        DisplayWindowId::new(1),
        0.0,
        0.0,
        8.0,
        16.0,
        CursorStyle::FilledBox,
        Color::WHITE,
    );
    assert!(buf.glyphs.is_empty());
    assert_eq!(buf.window_cursors.len(), 1);
}

// =======================================================================
// add_stretch()
// =======================================================================

#[test]
fn add_stretch_appends_stretch_glyph() {
    let mut buf = FrameGlyphBuffer::new();
    let bg = Color::rgb(0.2, 0.2, 0.2);
    buf.add_stretch(0.0, 100.0, 800.0, 16.0, bg, FaceId::new(5), false);

    assert_eq!(buf.len(), 1);
    match &buf.glyphs[0] {
        FrameGlyph::Stretch {
            x,
            y,
            width,
            height,
            bg: stretch_bg,
            face_id,
            ..
        } => {
            assert_eq!(*x, 0.0);
            assert_eq!(*y, 100.0);
            assert_eq!(*width, 800.0);
            assert_eq!(*height, 16.0);
            assert_color_eq(stretch_bg, &bg);
            assert_eq!(*face_id, FaceId::new(5));
            assert!(!buf.glyphs[0].is_overlay());
        }
        other => panic!("Expected Stretch glyph, got {:?}", other),
    }
}

#[test]
fn add_stretch_overlay() {
    let mut buf = FrameGlyphBuffer::new();
    buf.set_draw_context(DisplayWindowId::new(1), GlyphRowRole::ModeLine, None);
    buf.add_stretch(0.0, 0.0, 800.0, 20.0, Color::BLUE, FaceId::new(0), true);
    assert!(buf.glyphs[0].is_overlay());
}

#[test]
fn slot_glyph_returns_matching_stretch() {
    let mut buf = FrameGlyphBuffer::new();
    buf.set_draw_context(DisplayWindowId::new(3), GlyphRowRole::Text, None);
    buf.add_stretch(8.0, 16.0, 24.0, 16.0, Color::BLACK, FaceId::new(7), false);

    let slot_id = buf.glyphs[0].slot_id().expect("stretch slot id");
    let glyph = buf.slot_glyph(slot_id).expect("slot glyph");

    match glyph {
        FrameGlyph::Stretch {
            bidi_level,
            width,
            face_id,
            ..
        } => {
            assert_eq!(*bidi_level, 0);
            assert_eq!(*width, 24.0);
            assert_eq!(*face_id, FaceId::new(7));
            assert_eq!(glyph.bidi_level(), Some(0));
        }
        other => panic!("Expected Stretch glyph, got {:?}", other),
    }
}

// =======================================================================
// add_window_info()
// =======================================================================

#[test]
fn add_window_info_appends_metadata() {
    let mut buf = FrameGlyphBuffer::new();
    buf.add_window_info(
        DisplayWindowId::new(0x1234),
        0xABCD,
        1,
        500,
        1000,
        crate::presentation_origin::BufferModiff::default(),
        10.0,
        20.0,
        780.0,
        560.0,
        22.0,
        0.0,
        0.0,
        true,
        false,
        16.0,
        String::new(),
        "main.rs".to_string(),
        true,
    );

    assert_eq!(buf.window_infos.len(), 1);
    let info = &buf.window_infos[0];
    assert_eq!(info.window_id.get(), 0x1234);
    assert_eq!(info.buffer_id, 0xABCD);
    assert_eq!(info.window_start, 1);
    assert_eq!(info.window_end, 500);
    assert_eq!(info.buffer_size, 1000);
    assert_eq!(info.bounds, Rect::new(10.0, 20.0, 780.0, 560.0));
    assert_eq!(info.mode_line_height, 22.0);
    assert!(info.selected);
    assert!(!info.is_minibuffer);
    assert_eq!(info.char_height, 16.0);
    assert_eq!(info.buffer_file_name, "main.rs");
    assert!(info.modified);
}

#[test]
fn add_window_info_multiple_windows() {
    let mut buf = FrameGlyphBuffer::new();

    // Two side-by-side windows
    buf.add_window_info(
        DisplayWindowId::new(1),
        100,
        0,
        200,
        500,
        crate::presentation_origin::BufferModiff::default(),
        0.0,
        0.0,
        400.0,
        600.0,
        20.0,
        0.0,
        0.0,
        true,
        false,
        16.0,
        String::new(),
        "left.rs".to_string(),
        false,
    );
    buf.add_window_info(
        DisplayWindowId::new(2),
        200,
        0,
        300,
        800,
        crate::presentation_origin::BufferModiff::default(),
        400.0,
        0.0,
        400.0,
        600.0,
        20.0,
        0.0,
        0.0,
        false,
        false,
        16.0,
        String::new(),
        "right.rs".to_string(),
        true,
    );

    assert_eq!(buf.window_infos.len(), 2);
    assert_eq!(buf.window_infos[0].window_id.get(), 1);
    assert!(buf.window_infos[0].selected);
    assert_eq!(buf.window_infos[1].window_id.get(), 2);
    assert!(!buf.window_infos[1].selected);
}

#[test]
fn add_window_info_minibuffer() {
    let mut buf = FrameGlyphBuffer::new();
    buf.add_window_info(
        DisplayWindowId::new(99),
        50,
        0,
        0,
        0,
        crate::presentation_origin::BufferModiff::default(),
        0.0,
        580.0,
        800.0,
        20.0,
        0.0,
        0.0,
        0.0,
        false,
        true,
        16.0,
        String::new(),
        String::new(),
        false,
    );

    let info = &buf.window_infos[0];
    assert!(info.is_minibuffer);
    assert!(!info.selected);
    assert_eq!(info.buffer_file_name, "");
}

#[test]
fn derive_transition_hint_reports_buffer_content_replacement() {
    let prev = make_window_info(1, 100, 10, Rect::new(0.0, 0.0, 800.0, 600.0));
    let curr = make_window_info(1, 200, 10, Rect::new(0.0, 0.0, 800.0, 600.0));

    let hint = derive_buffer_replacement_hint(&prev, &curr).unwrap();
    assert_eq!(
        hint,
        ContentTransitionHint::BufferReplaced {
            target: BufferTransitionTarget::Window {
                window_id: DisplayWindowId::new(1),
                region: curr.geometry.buffer_viewport().unwrap(),
            },
            intent: ContentTransitionIntent::Replace,
        }
    );
}

#[test]
fn derive_transition_hint_requires_complete_previous_viewport_geometry() {
    let mut prev = make_window_info(1, 100, 10, Rect::new(0.0, 0.0, 800.0, 600.0));
    let curr = make_window_info(1, 200, 10, Rect::new(0.0, 0.0, 800.0, 600.0));
    prev.geometry = PresentedWindowGeometry::Skipped {
        cell_origin: PresentedCellOrigin::default(),
        outer: prev.bounds,
    };

    assert_eq!(derive_buffer_replacement_hint(&prev, &curr), None);
}

#[test]
fn derive_transition_hint_rejects_changed_viewport_inside_stable_outer_bounds() {
    let prev = make_window_info(1, 100, 10, Rect::new(0.0, 0.0, 800.0, 600.0));
    let mut curr = make_window_info(1, 200, 10, Rect::new(0.0, 0.0, 800.0, 600.0));
    let PresentedWindowGeometry::Complete { regions, .. } = &mut curr.geometry else {
        panic!("test window has complete geometry");
    };
    regions.text_body.y += 18.0;
    regions.text_body.height -= 18.0;
    regions.tab_line = Some(Rect::new(0.0, 0.0, 800.0, 18.0));

    assert_eq!(derive_buffer_replacement_hint(&prev, &curr), None);
}

#[test]
fn presented_window_regions_expose_only_the_buffer_viewport() {
    let regions = PresentedWindowRegions {
        outer: Rect::new(10.0, 20.0, 240.0, 180.0),
        text_body: Rect::new(42.0, 50.0, 160.0, 110.0),
        left_margin: Some(Rect::new(26.0, 50.0, 8.0, 110.0)),
        left_fringe: Some(Rect::new(34.0, 50.0, 8.0, 110.0)),
        right_fringe: Some(Rect::new(202.0, 50.0, 8.0, 110.0)),
        right_margin: Some(Rect::new(210.0, 50.0, 8.0, 110.0)),
        left_scroll_bar: Some(Rect::new(10.0, 50.0, 16.0, 110.0)),
        right_scroll_bar: Some(Rect::new(218.0, 50.0, 16.0, 110.0)),
        tab_line: Some(Rect::new(10.0, 20.0, 240.0, 16.0)),
        header_line: Some(Rect::new(10.0, 36.0, 240.0, 14.0)),
        mode_line: Some(Rect::new(10.0, 160.0, 240.0, 20.0)),
        horizontal_scroll_bar: Some(Rect::new(10.0, 180.0, 240.0, 10.0)),
        right_divider: Some(Rect::new(234.0, 20.0, 6.0, 170.0)),
        bottom_divider: Some(Rect::new(10.0, 190.0, 240.0, 10.0)),
        ..PresentedWindowRegions::default()
    };

    let viewport = regions
        .buffer_viewport()
        .expect("complete text geometry has a buffer viewport");

    assert_eq!(viewport.bounds(), Rect::new(26.0, 50.0, 192.0, 110.0));
}

#[test]
fn derive_transition_hint_skips_window_geometry_change() {
    let prev = make_window_info(1, 100, 10, Rect::new(0.0, 0.0, 664.0, 646.0));
    let curr = make_window_info(1, 100, 10, Rect::new(0.0, 0.0, 1100.0, 760.0));

    assert_eq!(derive_buffer_replacement_hint(&prev, &curr), None);
}

#[test]
fn a_viewport_scroll_is_no_longer_a_producer_hint() {
    // Scrolling within the same buffer produces no hint at all now: how far a
    // viewport moved is measured by the compositor from anchor rows, not
    // declared here from a character count. Only a change of content identity
    // is still the producer's to report.
    let prev = make_window_info(1, 100, 10, Rect::new(0.0, 0.0, 800.0, 600.0));
    let curr = make_window_info(1, 100, 42, Rect::new(0.0, 0.0, 800.0, 600.0));

    assert_eq!(derive_buffer_replacement_hint(&prev, &curr), None);
}

#[test]
fn derive_transition_hint_skips_minibuffer() {
    let prev = make_window_info(1, 100, 10, Rect::new(0.0, 0.0, 800.0, 600.0));
    let mut curr = make_window_info(1, 100, 20, Rect::new(0.0, 0.0, 800.0, 600.0));
    curr.is_minibuffer = true;

    assert!(derive_buffer_replacement_hint(&prev, &curr).is_none());
}

// =======================================================================
// set_face() / set_face_with_font()
// =======================================================================

#[test]
fn set_face_affects_subsequent_chars() {
    let mut buf = FrameGlyphBuffer::new();

    // Default face
    buf.add_char('A', 0.0, 0.0, 8.0, 16.0, 12.0, false);

    // Change face
    let red = Color::rgb(1.0, 0.0, 0.0);
    buf.set_face(
        FaceId::new(5),
        red,
        None,
        700,
        true,
        0,
        None,
        0,
        None,
        0,
        None,
    );
    buf.add_char('B', 8.0, 0.0, 8.0, 16.0, 12.0, false);

    // First char uses default face
    match &buf.glyphs[0] {
        FrameGlyph::Char { face_id, .. } => {
            assert_eq!(*face_id, FaceId::new(0));
            let rf = buf.resolved_face(*face_id);
            assert_eq!(rf.font_weight, 400);
            assert!(!rf.italic);
        }
        _ => panic!("Expected Char"),
    }

    // Second char uses newly set face
    match &buf.glyphs[1] {
        FrameGlyph::Char { face_id, .. } => {
            assert_eq!(*face_id, FaceId::new(5));
            let rf = buf.resolved_face(*face_id);
            assert_eq!(rf.font_weight, 700);
            assert!(rf.italic);
            assert_color_eq(&rf.fg, &red);
        }
        _ => panic!("Expected Char"),
    }
}

#[test]
fn set_face_with_font_stores_font_family() {
    let mut buf = FrameGlyphBuffer::new();
    let fg = Color::WHITE;
    buf.set_face_with_font(
        FaceId::new(7),
        fg,
        None,
        "Fira Code",
        400,
        false,
        14.0,
        0,
        None,
        0,
        None,
        0,
        None,
        false,
    );

    // current_font_family is set by set_face_with_font
    assert_eq!(buf.get_current_font_family(), "Fira Code");

    // set_face_with_font now keeps the face table coherent as well.
    assert_eq!(buf.get_face_font(FaceId::new(7)), "Fira Code");
}

#[test]
fn set_face_with_font_updates_font_size() {
    let mut buf = FrameGlyphBuffer::new();
    buf.set_face_with_font(
        FaceId::new(1),
        Color::WHITE,
        None,
        "monospace",
        400,
        false,
        24.0,
        0,
        None,
        0,
        None,
        0,
        None,
        false,
    );
    buf.add_char('A', 0.0, 0.0, 12.0, 24.0, 18.0, false);

    match &buf.glyphs[0] {
        FrameGlyph::Char { face_id, .. } => {
            assert_eq!(buf.resolved_face(*face_id).font_size, 24.0);
        }
        _ => panic!("Expected Char"),
    }
}

#[test]
fn get_face_font_reads_from_faces_map() {
    let mut buf = FrameGlyphBuffer::new();

    // No face inserted yet — falls back to "monospace"
    assert_eq!(buf.get_face_font(FaceId::new(1)), "monospace");

    // Insert faces (as layout engine's apply_face would)
    let mut face1 = Face::new(FaceId::new(1));
    face1.font_family = "JetBrains Mono".to_string();
    buf.faces.insert(FaceId::new(1), face1);

    assert_eq!(buf.get_face_font(FaceId::new(1)), "JetBrains Mono");
    assert_eq!(buf.get_face_font(FaceId::new(2)), "monospace"); // not inserted
}

#[test]
fn set_face_with_font_decoration_attributes() {
    let mut buf = FrameGlyphBuffer::new();
    let ul_color = Color::rgb(1.0, 1.0, 0.0);
    let st_color = Color::rgb(1.0, 0.0, 1.0);
    let ol_color = Color::rgb(0.0, 1.0, 1.0);
    buf.set_face_with_font(
        FaceId::new(3),
        Color::WHITE,
        None,
        "monospace",
        400,
        false,
        14.0,
        3,
        Some(ul_color), // wave underline
        1,
        Some(st_color), // strike-through
        1,
        Some(ol_color), // overline
        false,
    );
    buf.add_char('D', 0.0, 0.0, 8.0, 16.0, 12.0, false);

    match &buf.glyphs[0] {
        FrameGlyph::Char { face_id, .. } => {
            let rf = buf.resolved_face(*face_id);
            assert_eq!(rf.underline, UnderlineStyle::Wave);
            assert_eq!(rf.underline_color, Some(ul_color));
            assert!(rf.strike_through);
            assert_eq!(rf.strike_through_color, Some(st_color));
            assert!(rf.overline);
            assert_eq!(rf.overline_color, Some(ol_color));
        }
        _ => panic!("Expected Char"),
    }
}

#[test]
fn get_current_bg_returns_current_face_bg() {
    let mut buf = FrameGlyphBuffer::new();
    assert_eq!(buf.get_current_bg(), None);

    let bg = Color::rgb(0.1, 0.2, 0.3);
    buf.set_face(
        FaceId::new(1),
        Color::WHITE,
        Some(bg),
        400,
        false,
        0,
        None,
        0,
        None,
        0,
        None,
    );
    assert_eq!(buf.get_current_bg(), Some(bg));
}

// =======================================================================
// set_frame_identity()
// =======================================================================

#[test]
fn set_frame_identity_stores_all_fields() {
    let mut buf = FrameGlyphBuffer::new();
    let border_color = Color::rgb(0.5, 0.5, 0.5);
    buf.set_frame_identity(
        DisplayFrameId::new(0x100),
        DisplayFrameId::new(0x200),
        50.0,
        75.0,
        5,
        true,
        2.0,
        border_color,
        true,
        0.85,
    );

    assert_eq!(buf.frame_placement.frame().get(), 0x100);
    assert_eq!(buf.frame_placement.parent().unwrap().get(), 0x200);
    assert_eq!(buf.frame_placement.outer_in_parent().x(), 50.0);
    assert_eq!(buf.frame_placement.outer_in_parent().y(), 75.0);
    assert_eq!(buf.frame_placement.z_order(), 5);
    assert!(buf.undecorated);
    assert_eq!(buf.border_width, 2.0);
    assert_color_eq(&buf.border_color, &border_color);
    assert!(buf.no_accept_focus);
    assert_eq!(buf.background_alpha, 0.85);
}

#[test]
fn set_frame_identity_root_frame() {
    let mut buf = FrameGlyphBuffer::new();
    buf.set_frame_identity(
        DisplayFrameId::new(0x100),
        DisplayFrameId::new(0), // parent_id 0 = root frame
        0.0,
        0.0,
        0,
        false,
        0.0,
        Color::BLACK,
        false,
        1.0,
    );

    assert_eq!(buf.frame_placement.frame().get(), 0x100);
    assert_eq!(buf.frame_placement.parent(), None);
    assert!(!buf.undecorated);
    assert!(!buf.no_accept_focus);
    assert_eq!(buf.background_alpha, 1.0);
}

// =======================================================================
// set_phys_cursor()
// =======================================================================

#[test]
fn set_phys_cursor_normalizes_text_slot_geometry() {
    let mut buf = FrameGlyphBuffer::new();
    buf.set_draw_context(DisplayWindowId::new(1), GlyphRowRole::Text, None);
    buf.add_char('2', 8.0, 70.0, 8.108109, 18.0, 14.0, false);
    buf.add_stretch(
        16.108109,
        70.0,
        8.108109,
        18.0,
        Color::WHITE,
        FaceId::new(0),
        false,
    );
    buf.add_char('d', 24.216217, 70.0, 8.0, 18.0, 14.0, false);
    let text_slot = buf.glyphs[2].slot_id().expect("text slot");

    buf.set_phys_cursor(PhysCursor {
        window_id: DisplayWindowId::new(1),
        charpos: 5,
        row: text_slot.row as usize,
        col: text_slot.col,
        slot_id: text_slot,
        x: 16.86,
        y: 70.0,
        width: 8.0,
        height: 18.0,
        ascent: 14.0,
        style: CursorStyle::FilledBox,
        color: Color::BLACK,
        cursor_fg: Color::WHITE,
    });

    let stored = buf.active_cursor().expect("active cursor");
    assert_eq!(stored.slot_id, text_slot);
    assert_eq!(stored.x, 24.216217);
    assert_eq!(stored.width, 8.0);
}

#[test]
fn set_phys_cursor_keeps_empty_row_text_origin_over_extend_stretch() {
    let mut buf = FrameGlyphBuffer::new();
    buf.set_draw_context(DisplayWindowId::new(1), GlyphRowRole::Text, None);
    buf.add_stretch(0.0, 52.0, 624.0, 558.0, Color::WHITE, FaceId::new(1), false);
    let empty_row_slot = buf.glyphs[0]
        .slot_id()
        .expect("the extend stretch occupies the empty row's display slot");

    buf.set_phys_cursor(PhysCursor {
        window_id: DisplayWindowId::new(1),
        charpos: 3,
        row: empty_row_slot.row as usize,
        col: empty_row_slot.col,
        slot_id: empty_row_slot,
        x: 8.0,
        y: 106.0,
        width: 7.8,
        height: 18.0,
        ascent: 14.0,
        style: CursorStyle::FilledBox,
        color: Color::BLACK,
        cursor_fg: Color::WHITE,
    });

    let cursor = buf.active_cursor().expect("active cursor");
    assert_eq!(cursor.x, 8.0);
}

#[test]
fn set_phys_cursor_stores_info() {
    let mut buf = FrameGlyphBuffer::new();
    let cursor_fg = Color::rgb(0.0, 0.0, 0.0);
    let cursor = PhysCursor {
        window_id: DisplayWindowId::new(2),
        charpos: 99,
        row: 3,
        col: 4,
        slot_id: DisplaySlotId::from_pixels(
            DisplayWindowId::new(2),
            Px(50.0),
            Px(100.0),
            Px(buf.char_width),
            Px(buf.char_height),
        ),
        x: 50.0,
        y: 100.0,
        width: 8.0,
        height: 16.0,
        ascent: 12.0,
        style: CursorStyle::FilledBox,
        color: Color::rgb(0.9, 0.9, 0.0),
        cursor_fg,
    };
    buf.set_phys_cursor(cursor.clone());

    let stored = buf.active_cursor().unwrap();
    assert_eq!(stored.window_id, cursor.window_id);
    assert_eq!(stored.slot_id, cursor.slot_id);
    assert_eq!(stored.x, cursor.x);
    assert_eq!(stored.y, cursor.y);
    assert_eq!(stored.width, cursor.width);
    assert_eq!(stored.height, cursor.height);
    assert_eq!(stored.ascent, cursor.ascent);
    assert_eq!(stored.style, cursor.style);
    assert!(stored.active);
    assert_color_eq(&stored.color, &cursor.color);
    assert_color_eq(&stored.cursor_fg, &cursor.cursor_fg);
}

#[test]
fn effective_phys_cursor_effects_prefers_the_selected_window_profile() {
    let mut buf = FrameGlyphBuffer::new();
    let window_id = DisplayWindowId::new(2);
    buf.set_phys_cursor(PhysCursor {
        window_id,
        charpos: 0,
        row: 0,
        col: 0,
        slot_id: DisplaySlotId::ZERO,
        x: 0.0,
        y: 0.0,
        width: 8.0,
        height: 16.0,
        ascent: 12.0,
        style: CursorStyle::FilledBox,
        color: Color::WHITE,
        cursor_fg: Color::BLACK,
    });

    let mut global = EffectsConfig::default();
    global.cursor_color_cycle.fps = crate::FrameRate::new(24).unwrap();
    assert_eq!(
        buf.effective_phys_cursor_effects(&global)
            .cursor_color_cycle
            .fps
            .get(),
        24
    );

    let mut local = EffectsConfig::cursor_profile_baseline();
    local.cursor_color_cycle.enabled = true;
    local.cursor_color_cycle.fps = crate::FrameRate::new(12).unwrap();
    buf.set_window_cursor_effects(window_id, local);
    assert_eq!(
        buf.effective_window_cursor_effects(window_id, &global)
            .cursor_color_cycle
            .fps
            .get(),
        12
    );
    assert_eq!(
        buf.effective_window_cursor_effects(DisplayWindowId::new(99), &global)
            .cursor_color_cycle
            .fps
            .get(),
        24
    );
    let effective = &buf
        .effective_phys_cursor_effects(&global)
        .cursor_color_cycle;
    assert!(effective.enabled);
    assert_eq!(effective.fps.get(), 12);
}

// =======================================================================
// font_size() / set_font_size()
// =======================================================================

#[test]
fn font_size_accessors() {
    let mut buf = FrameGlyphBuffer::new();
    assert_eq!(buf.font_size(), 14.0); // default

    buf.set_font_size(20.0);
    assert_eq!(buf.font_size(), 20.0);

    // The current font size flows into the face synthesized by set_face, and
    // a char added afterwards resolves its size from that face.
    buf.set_face(
        FaceId::new(1),
        Color::WHITE,
        None,
        400,
        false,
        0,
        None,
        0,
        None,
        0,
        None,
    );
    buf.add_char('X', 0.0, 0.0, 10.0, 20.0, 15.0, false);
    match &buf.glyphs[0] {
        FrameGlyph::Char { face_id, .. } => {
            assert_eq!(buf.resolved_face(*face_id).font_size, 20.0)
        }
        _ => panic!("Expected Char"),
    }
}

// =======================================================================
// add_background()
// =======================================================================

#[test]
fn add_background_adds_glyph() {
    let mut buf = FrameGlyphBuffer::new();
    let bg = Color::rgb(0.15, 0.15, 0.15);
    buf.add_background(10.0, 20.0, 780.0, 560.0, bg);

    assert_eq!(buf.len(), 1);

    match &buf.glyphs[0] {
        FrameGlyph::Background { bounds, color } => {
            assert_eq!(*bounds, Rect::new(10.0, 20.0, 780.0, 560.0));
            assert_color_eq(color, &bg);
        }
        other => panic!("Expected Background glyph, got {:?}", other),
    }
}

// =======================================================================
// add_border()
// =======================================================================

#[test]
fn add_border_appends_border_glyph() {
    let mut buf = FrameGlyphBuffer::new();
    let border_color = Color::rgb(0.3, 0.3, 0.3);
    buf.add_border(400.0, 0.0, 1.0, 600.0, border_color);

    assert_eq!(buf.len(), 1);
    match &buf.glyphs[0] {
        FrameGlyph::Border {
            x,
            y,
            width,
            height,
            color,
            ..
        } => {
            assert_eq!(*x, 400.0);
            assert_eq!(*y, 0.0);
            assert_eq!(*width, 1.0);
            assert_eq!(*height, 600.0);
            assert_color_eq(color, &border_color);
        }
        other => panic!("Expected Border glyph, got {:?}", other),
    }
}

#[test]
fn border_glyph_is_not_overlay() {
    let mut buf = FrameGlyphBuffer::new();
    buf.add_border(0.0, 0.0, 1.0, 100.0, Color::WHITE);
    assert!(!buf.glyphs[0].is_overlay());
}

// =======================================================================
// add_image() / add_video() / add_xwidget()
// =======================================================================

#[test]
fn add_image_appends_image_glyph() {
    let mut buf = FrameGlyphBuffer::new();
    buf.add_image(ImageId::new(42), 100.0, 200.0, 320.0, 240.0);

    assert_eq!(buf.len(), 1);
    match &buf.glyphs[0] {
        FrameGlyph::Image {
            slot_id,
            image_id,
            x,
            y,
            width,
            height,
            ..
        } => {
            assert_eq!(
                *slot_id,
                Some(DisplaySlotId::from_pixels(
                    DisplayWindowId::new(0),
                    Px(100.0),
                    Px(200.0),
                    Px(buf.char_width),
                    Px(buf.char_height)
                ))
            );
            assert_eq!(image_id.get(), 42);
            assert_eq!(*x, 100.0);
            assert_eq!(*y, 200.0);
            assert_eq!(*width, 320.0);
            assert_eq!(*height, 240.0);
        }
        other => panic!("Expected Image glyph, got {:?}", other),
    }
}

#[test]
fn add_video_appends_video_glyph() {
    let mut buf = FrameGlyphBuffer::new();
    buf.add_video(VideoId::new(7), 0.0, 0.0, 640.0, 480.0);

    match &buf.glyphs[0] {
        FrameGlyph::Video { video_id, .. } => assert_eq!(video_id.get(), 7),
        other => panic!("Expected Video glyph, got {:?}", other),
    }
}

#[test]
fn add_xwidget_appends_xwidget_glyph() {
    let mut buf = FrameGlyphBuffer::new();
    let content = crate::XwidgetContentExtent::new(800.0, 600.0).expect("content extent");
    let presentation = crate::XwidgetPresentationGeometry::new(
        crate::GeometryPoint::<crate::FrameSpace, crate::LogicalPixels>::from_px(0.0, 0.0)
            .expect("frame-local origin"),
        content,
        crate::XwidgetLayoutAdvance::new(crate::Px(800.0)).expect("layout advance"),
        None,
    );
    buf.add_xwidget(XwidgetId::new(99), WebViewId::new(700), presentation);

    match &buf.glyphs[0] {
        FrameGlyph::Xwidget {
            xwidget_id,
            webview_id,
            presentation,
            ..
        } => {
            assert_eq!(xwidget_id.get(), 99);
            assert_eq!(webview_id.get(), 700);
            assert_eq!(presentation.content_extent(), content);
        }
        other => panic!("Expected Xwidget glyph, got {:?}", other),
    }
}

#[test]
fn slot_glyph_matches_media_slots() {
    let mut buf = FrameGlyphBuffer::new();
    buf.add_image(ImageId::new(42), 16.0, 32.0, 320.0, 240.0);

    let slot_id = buf.glyphs[0].slot_id().expect("media slot id");
    let slot = buf.slot_glyph(slot_id).expect("slot glyph");
    assert!(matches!(slot, FrameGlyph::Image { image_id, .. } if image_id.get() == 42));
}

#[test]
fn set_phys_cursor_normalizes_media_slots_to_hollow() {
    let mut buf = FrameGlyphBuffer::new();
    buf.add_image(ImageId::new(9), 24.0, 48.0, 128.0, 96.0);
    let slot_id = buf.glyphs[0].slot_id().expect("image slot id");

    buf.set_phys_cursor(PhysCursor {
        window_id: DisplayWindowId::new(0),
        charpos: 0,
        row: slot_id.row as usize,
        col: slot_id.col,
        slot_id,
        x: 24.0,
        y: 48.0,
        width: 8.0,
        height: 16.0,
        ascent: 12.0,
        style: CursorStyle::FilledBox,
        color: Color::WHITE,
        cursor_fg: Color::BLACK,
    });

    let stored = buf.active_cursor().expect("active cursor");
    assert_eq!(stored.style, CursorStyle::Hollow);
    assert_eq!(stored.x, 24.0);
    assert_eq!(stored.y, 48.0);
    assert_eq!(stored.width, 128.0);
    assert_eq!(stored.height, 96.0);
}

// =======================================================================
// add_scroll_bar()
// =======================================================================

#[test]
fn add_scroll_bar_appends_scrollbar_glyph() {
    let mut buf = FrameGlyphBuffer::new();
    let track = Color::rgb(0.1, 0.1, 0.1);
    let thumb = Color::rgb(0.5, 0.5, 0.5);
    buf.add_scroll_bar(
        false, 790.0, 0.0, 10.0, 600.0, 5, 20, 100, 50.0, 100.0, track, thumb,
    );

    assert_eq!(buf.len(), 1);
    match &buf.glyphs[0] {
        FrameGlyph::ScrollBar {
            window_id,
            row_role,
            clip_rect,
            horizontal,
            x,
            y,
            width,
            height,
            position,
            portion,
            whole,
            thumb_start,
            thumb_size,
            track_color,
            thumb_color,
        } => {
            assert_eq!(window_id.get(), 0);
            assert_eq!(*row_role, GlyphRowRole::Text);
            assert_eq!(*clip_rect, None);
            assert!(!*horizontal);
            assert_eq!(*x, 790.0);
            assert_eq!(*y, 0.0);
            assert_eq!(*width, 10.0);
            assert_eq!(*height, 600.0);
            assert_eq!(*position, 5);
            assert_eq!(*portion, 20);
            assert_eq!(*whole, 100);
            assert_eq!(*thumb_start, 50.0);
            assert_eq!(*thumb_size, 100.0);
            assert_color_eq(track_color, &track);
            assert_color_eq(thumb_color, &thumb);
        }
        other => panic!("Expected ScrollBar glyph, got {:?}", other),
    }
}

// =======================================================================
// is_overlay() dispatch
// =======================================================================

#[test]
fn is_overlay_returns_false_for_non_char_stretch_types() {
    let mut buf = FrameGlyphBuffer::new();
    buf.add_border(0.0, 0.0, 1.0, 100.0, Color::WHITE);
    buf.add_cursor(
        DisplayWindowId::new(1),
        0.0,
        0.0,
        8.0,
        16.0,
        CursorStyle::FilledBox,
        Color::WHITE,
    );
    buf.add_image(ImageId::new(1), 0.0, 0.0, 100.0, 100.0);

    for glyph in &buf.glyphs {
        assert!(!glyph.is_overlay());
    }
}

// =======================================================================
// Full frame simulation: realistic multi-window frame
// =======================================================================

#[test]
fn full_frame_simulation() {
    let frame_bg = Color::rgb(0.12, 0.12, 0.12);
    let mut buf = FrameGlyphBuffer::with_size(1920.0, 1080.0);
    buf.background = frame_bg;
    buf.set_frame_identity(
        DisplayFrameId::new(0x1),
        DisplayFrameId::new(0),
        0.0,
        0.0,
        0,
        false,
        0.0,
        Color::BLACK,
        false,
        1.0,
    );

    // Window 1: left pane background
    let win_bg = Color::rgb(0.13, 0.13, 0.13);
    buf.add_background(0.0, 0.0, 960.0, 1060.0, win_bg);

    // Window 1: some text
    let text_fg = Color::rgb(0.87, 0.87, 0.87);
    buf.set_face_with_font(
        FaceId::new(0),
        text_fg,
        None,
        "Iosevka",
        400,
        false,
        14.0,
        0,
        None,
        0,
        None,
        0,
        None,
        false,
    );
    for (i, ch) in "Hello, Neomacs!".chars().enumerate() {
        buf.add_char(ch, i as f32 * 8.0, 0.0, 8.0, 16.0, 12.0, false);
    }

    // Window 1: cursor
    buf.add_cursor(
        DisplayWindowId::new(1),
        15.0 * 8.0,
        0.0,
        2.0,
        16.0,
        CursorStyle::Bar(2.0),
        Color::WHITE,
    );
    buf.set_phys_cursor(PhysCursor {
        window_id: DisplayWindowId::new(1),
        charpos: 15,
        row: 0,
        col: 15,
        slot_id: DisplaySlotId::from_pixels(
            DisplayWindowId::new(1),
            Px(15.0 * 8.0),
            Px(0.0),
            Px(buf.char_width),
            Px(buf.char_height),
        ),
        x: 15.0 * 8.0,
        y: 0.0,
        width: 2.0,
        height: 16.0,
        ascent: 12.0,
        style: CursorStyle::Bar(2.0),
        color: Color::WHITE,
        cursor_fg: Color::BLACK,
    });

    // Vertical border
    buf.add_border(960.0, 0.0, 1.0, 1060.0, Color::rgb(0.3, 0.3, 0.3));

    // Window 2: right pane background
    buf.add_background(961.0, 0.0, 959.0, 1060.0, win_bg);

    // Mode-line (overlay)
    let ml_bg = Color::rgb(0.2, 0.2, 0.3);
    buf.set_face(
        FaceId::new(10),
        Color::WHITE,
        Some(ml_bg),
        700,
        false,
        0,
        None,
        0,
        None,
        0,
        None,
    );
    buf.set_draw_context(DisplayWindowId::new(1), GlyphRowRole::ModeLine, None);
    buf.add_stretch(0.0, 1060.0, 1920.0, 20.0, ml_bg, FaceId::new(10), true);

    // Window infos
    buf.add_window_info(
        DisplayWindowId::new(1),
        100,
        0,
        500,
        1000,
        crate::presentation_origin::BufferModiff::default(),
        0.0,
        0.0,
        960.0,
        1060.0,
        20.0,
        0.0,
        0.0,
        true,
        false,
        16.0,
        String::new(),
        "left.rs".to_string(),
        false,
    );
    buf.add_window_info(
        DisplayWindowId::new(2),
        200,
        0,
        300,
        800,
        crate::presentation_origin::BufferModiff::default(),
        961.0,
        0.0,
        959.0,
        1060.0,
        20.0,
        0.0,
        0.0,
        false,
        false,
        16.0,
        String::new(),
        "right.rs".to_string(),
        true,
    );

    // Verify totals
    // 15 chars + 2 backgrounds + 1 border + 1 mode-line stretch = 19 glyphs
    assert_eq!(buf.len(), 19);
    // One decorative cursor from add_cursor plus the active cursor from
    // set_phys_cursor (both window 1) now live in the same unified list.
    assert_eq!(buf.window_cursors.len(), 2);
    assert_eq!(buf.window_infos.len(), 2);
    assert!(buf.active_cursor().is_some());
    assert_eq!(buf.frame_placement.frame().get(), 0x1);
    assert_eq!(buf.width, 1920.0);
    assert_eq!(buf.height, 1080.0);

    // Verify overlay count
    let overlay_count = buf.glyphs.iter().filter(|g| g.is_overlay()).count();
    assert_eq!(overlay_count, 1); // just the mode-line stretch
}

// --- StipplePattern: built-in bitmaps + XBM parsing --------------------

#[test]
fn stipple_builtin_named_bitmaps_match_x11() {
    // Values verified against X11 /usr/include/X11/bitmaps and GNU src/bitmaps.
    assert_eq!(
        StipplePattern::builtin("gray3"),
        Some(StipplePattern {
            width: 4,
            height: 4,
            bits: vec![0x01, 0x00, 0x04, 0x00]
        })
    );
    assert_eq!(
        StipplePattern::builtin("gray1"),
        Some(StipplePattern {
            width: 2,
            height: 2,
            bits: vec![0x01, 0x02]
        })
    );
    assert_eq!(
        StipplePattern::builtin("light_gray").map(|p| (p.width, p.height)),
        Some((4, 2))
    );
    assert_eq!(StipplePattern::builtin("not-a-bitmap"), None);
}

#[test]
fn stipple_from_xbm_parses_hex_and_char_token_forms() {
    // Standard X11 form (hex tokens).
    let hex = "#define g_width 4\n#define g_height 4\n\
               static char g_bits[] = {\n   0x01, 0x00, 0x04, 0x00};\n";
    assert_eq!(
        StipplePattern::from_xbm_source(hex),
        Some(StipplePattern {
            width: 4,
            height: 4,
            bits: vec![0x01, 0x00, 0x04, 0x00]
        })
    );
    // GNU src form (C char escapes) must parse identically.
    let chars = "#define g_width 4\n#define g_height 4\n\
                 static char g_bits[] = {\n  '\\x01','\\x00','\\x04','\\x00'};\n";
    assert_eq!(
        StipplePattern::from_xbm_source(chars),
        StipplePattern::from_xbm_source(hex)
    );
    // Too few bytes for the declared size => rejected.
    let short = "#define g_width 8\n#define g_height 4\nstatic char g_bits[] = { 0x01 };\n";
    assert_eq!(StipplePattern::from_xbm_source(short), None);
}
