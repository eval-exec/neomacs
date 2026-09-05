use super::*;
use neomacs_display_protocol::face::{Face, FaceAttributes, UnderlineStyle};
use neomacs_display_protocol::frame_glyphs::{
    CursorStyle, DisplaySlotId, GlyphRowRole, PhysCursor, PresentedCellOrigin,
    PresentedWindowGeometry, PresentedWindowRegions, WindowInfo,
};
use neomacs_display_protocol::glyph_matrix::{
    FaceFillItem, FrameDisplayState, Glyph, GlyphArea, GlyphMatrix, GlyphProvenance, GlyphRow,
    GlyphlessPresentation, RowDamage, WindowMatrixEntry,
};
use neomacs_display_protocol::tty_capabilities::{
    TerminfoExpander, TerminfoParameters, TtyCapability, TtyColorCapabilities, TtyColorDepth,
    TtyColorSource, TtyDirectColorRoute, TtyNoColorVideo, TtyStyledUnderline,
};
use neomacs_display_protocol::types::Px;
use neomacs_display_protocol::types::{Color, DisplayFrameId, DisplayWindowId, Rect};
use std::collections::HashMap;

/// GNU `turn_on_face` against the capabilities registered for this terminal,
/// starting from the state GNU leaves the terminal in between runs: no face on.
/// That is exactly `write_face_transition` out of `None`, so these tests
/// measure the writer's real entry point and not a second copy of it.
fn write_sgr(buf: &mut Vec<u8>, attrs: &CellAttrs) {
    write_face_transition(buf, &mut None, attrs);
}

// ---------------------------------------------------------------------------
// TtyRif::new
// ---------------------------------------------------------------------------

#[test]
fn new_creates_correct_grid_dimensions() {
    let rif = TtyRif::new(80, 24);
    assert_eq!(rif.width(), 80);
    assert_eq!(rif.height(), 24);
    assert_eq!(rif.current.cells.len(), 80 * 24);
    assert_eq!(rif.desired.cells.len(), 80 * 24);
}

#[test]
fn new_grids_are_blank_spaces() {
    let rif = TtyRif::new(10, 5);
    for cell in &rif.current.cells {
        assert_eq!(cell.ch, ' ');
        assert!(!cell.padding);
    }
}

// ---------------------------------------------------------------------------
// resize
// ---------------------------------------------------------------------------

#[test]
fn resize_updates_dimensions() {
    let mut rif = TtyRif::new(80, 24);
    rif.resize(120, 40);
    assert_eq!(rif.width(), 120);
    assert_eq!(rif.height(), 40);
    assert_eq!(rif.current.cells.len(), 120 * 40);
    assert_eq!(rif.desired.cells.len(), 120 * 40);
}

#[test]
fn resize_clears_grids() {
    let mut rif = TtyRif::new(10, 5);
    // Dirty a cell in current.
    rif.current.set(0, 0, 'X', CellAttrs::default(), false);
    rif.resize(10, 5);
    // After resize, the cell should be blank again.
    assert_eq!(rif.current.cells[0].ch, ' ');
}

// ---------------------------------------------------------------------------
// Face resolution
// ---------------------------------------------------------------------------

#[test]
fn resolve_attrs_uses_face_table() {
    let mut rif = TtyRif::new(80, 24);
    let mut face = Face::new(FaceId::new(1));
    // What the writer emits is the REALIZED TERMINAL colour -- the index
    // `tty-color-desc` returned (GNU `map_tty_color`, src/xfaces.c:6640-6648) --
    // not the pixel the GUI paints. A face carrying only a pixel has no colour
    // to write, which is GNU's FACE_TTY_DEFAULT_COLOR.
    face.foreground = Color::rgb(1.0, 0.0, 0.0);
    face.background = Color::rgb(0.0, 1.0, 0.0);
    face.terminal_foreground = Some(TerminalColor::Indexed(9));
    face.terminal_background = Some(TerminalColor::Indexed(10));
    face.font_weight = 700;
    face.attributes |= FaceAttributes::ITALIC;
    face.underline_style = UnderlineStyle::Wave;
    face.attributes |= FaceAttributes::STRIKE_THROUGH;

    let mut faces = HashMap::new();
    faces.insert(FaceId::new(1), face);
    rif.set_faces(faces);

    let attrs = rif.resolve_attrs(FaceId::new(1));
    assert_eq!(attrs.fg, Some(TerminalColor::Indexed(9)));
    assert_eq!(attrs.bg, Some(TerminalColor::Indexed(10)));
    assert!(attrs.bold);
    assert!(attrs.italic);
    assert_eq!(attrs.underline, 3); // Wave
    assert!(attrs.strikethrough);
}

#[test]
fn resolve_attrs_falls_back_to_defaults_for_unknown_face() {
    let rif = TtyRif::new(80, 24);
    let attrs = rif.resolve_attrs(FaceId::new(999));
    // Should get default fg/bg.
    assert_eq!(attrs.fg, None);
    assert_eq!(attrs.bg, None);
    assert!(!attrs.bold);
    assert!(!attrs.italic);
}

#[test]
fn resolve_attrs_preserves_terminal_default_face_colors() {
    let mut rif = TtyRif::new(80, 24);
    let mut face = Face::new(FaceId::new(0));
    face.foreground = Color::rgb(0.0, 0.0, 0.0);
    face.background = Color::rgb(1.0, 1.0, 1.0);
    face.use_default_foreground = true;
    face.use_default_background = true;

    let mut faces = HashMap::new();
    faces.insert(FaceId::new(0), face);
    rif.set_faces(faces);

    let attrs = rif.resolve_attrs(FaceId::new(0));
    assert_eq!(attrs.fg, None);
    assert_eq!(attrs.bg, None);
}

// ---------------------------------------------------------------------------
// glyph_to_char
// ---------------------------------------------------------------------------

#[test]
fn glyph_to_char_returns_char_for_char_glyph() {
    let g = Glyph::char('Z', FaceId::new(0), 0);
    assert_eq!(glyph_to_char(&g), 'Z');
}

#[test]
fn glyph_to_char_returns_first_char_for_composite() {
    let g = Glyph {
        glyph_type: GlyphType::Composite { text: "ab".into() },
        face_id: FaceId::new(0),
        box_vertical_edges: Default::default(),
        provenance: GlyphProvenance::buffer(0),
        bidi_level: 0,
        wide: false,
        pixel_width: 0.0,
        pixel_height: 0.0,
        pixel_ascent: 0.0,
        vertical_offset_px: 0.0,
        padding: false,
        pointer_appearance: None,
    };
    assert_eq!(glyph_to_char(&g), 'a');
}

#[test]
fn glyph_to_char_returns_space_for_stretch() {
    let g = Glyph::stretch(4, FaceId::new(0));
    assert_eq!(glyph_to_char(&g), ' ');
}

#[test]
fn surface_tty_placeholder_labels_and_fills_the_reserved_width() {
    // Exactly the label width: no fill.
    assert_eq!(surface_tty_placeholder(8), "[shader]");
    // Wider: label centered in a light-shade fill, exactly width_cols wide.
    let p = surface_tty_placeholder(12);
    assert_eq!(p.chars().count(), 12);
    assert_eq!(p, "░░[shader]░░");
    // Odd remainder: extra fill goes on the right.
    assert_eq!(surface_tty_placeholder(11), "░[shader]░░");
    // Too narrow for the label: pure fill, still visible (never blank).
    let narrow = surface_tty_placeholder(3);
    assert_eq!(narrow.chars().count(), 3);
    assert!(narrow.chars().all(|c| c == '░'));
    assert_eq!(surface_tty_placeholder(1), "░");
}

#[test]
fn rasterize_shows_placeholder_for_surface_glyph() {
    let cols = 12;
    let mut state = FrameDisplayState::new(cols, 3, 8.0, 16.0);
    state.background = Color::rgb(0.0, 0.0, 0.0);

    let mut matrix = GlyphMatrix::new(3, cols);
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    // A shader-surface glyph spanning the full 12 columns (as push_media builds
    // it: a stretch glyph whose type is overwritten to Surface).
    let mut surface = Glyph::stretch(cols as u16, FaceId::new(0));
    surface.glyph_type = GlyphType::Surface {
        surface_id: 0x7000_0001u32 as i32,
        width_cols: cols as u16,
    };
    row.glyphs[GlyphArea::Text as usize].push(surface);
    matrix.rows[0] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(row);

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, cols as f32 * 8.0, 3.0 * 16.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, cols as f32 * 8.0, 3.0 * 16.0),
        text_clip_bounds: None,
        selected: true,
    });

    let mut rif = TtyRif::new(cols, 3);
    rif.rasterize(&state);

    // The reserved columns show the labeled placeholder, not blank space.
    let rendered: String = (0..cols).map(|c| desired_char(&rif, 0, c)).collect();
    assert_eq!(rendered, "░░[shader]░░");
}

// ---------------------------------------------------------------------------
// rasterize
// ---------------------------------------------------------------------------

/// Helper: build a simple FrameDisplayState with one window containing
/// the given text on a single row.
fn make_simple_state(text: &str) -> FrameDisplayState {
    let cols = text.len().max(10);
    let mut state = FrameDisplayState::new(cols, 5, 8.0, 16.0);
    state.background = Color::rgb(0.0, 0.0, 0.0);

    let mut matrix = GlyphMatrix::new(5, cols);
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    for (i, ch) in text.chars().enumerate() {
        row.glyphs[GlyphArea::Text as usize].push(Glyph::char(ch, FaceId::new(0), i));
    }
    matrix.rows[0] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(row);

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, cols as f32 * 8.0, 5.0 * 16.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, cols as f32 * 8.0, 5.0 * 16.0),
        text_clip_bounds: None,
        selected: true,
    });
    state
}

fn make_grid_state(
    frame_id: u64,
    parent_id: u64,
    parent_x: f32,
    parent_y: f32,
    cols: usize,
    rows: usize,
    text: &str,
) -> FrameDisplayState {
    let mut state = FrameDisplayState::new(cols, rows, 1.0, 1.0);
    state.frame_placement = neomacs_display_protocol::PresentedFramePlacement::new(
        DisplayFrameId::new(frame_id),
        state.presentation_id,
        (parent_id != 0).then(|| DisplayFrameId::new(parent_id)),
        neomacs_display_protocol::ParentFrameRect::new(
            parent_x,
            parent_y,
            state.frame_pixel_width,
            state.frame_pixel_height,
        )
        .unwrap(),
        0,
    );
    state.background = Color::rgb(0.0, 0.0, 0.0);

    let mut matrix = GlyphMatrix::new(rows, cols);
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    for (i, ch) in text.chars().take(cols).enumerate() {
        row.glyphs[GlyphArea::Text as usize].push(Glyph::char(ch, FaceId::new(0), i));
    }
    if rows > 0 {
        matrix.rows[0] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(row);
    }

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new((frame_id + 100) as i64),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, cols as f32, rows as f32),
        text_pixel_bounds: Rect::new(0.0, 0.0, cols as f32, rows as f32),
        text_clip_bounds: None,
        selected: true,
    });
    state
}

fn desired_char(rif: &TtyRif, row: usize, col: usize) -> char {
    rif.desired.cells[row * rif.width() + col].ch
}

#[test]
fn rasterize_simple_text() {
    let mut rif = TtyRif::new(10, 5);
    let state = make_simple_state("Hello");
    rif.rasterize(&state);

    // First row should have "Hello" followed by spaces.
    assert_eq!(rif.desired.cells[0].ch, 'H');
    assert_eq!(rif.desired.cells[1].ch, 'e');
    assert_eq!(rif.desired.cells[2].ch, 'l');
    assert_eq!(rif.desired.cells[3].ch, 'l');
    assert_eq!(rif.desired.cells[4].ch, 'o');
    assert_eq!(rif.desired.cells[5].ch, ' '); // cleared to space
}

#[test]
fn rasterize_places_each_right_margin_at_the_reserved_columns() {
    let window = DisplayWindowId::new(1);
    let outer = Rect::new(0.0, 0.0, 12.0, 2.0);
    let text_body = Rect::new(0.0, 0.0, 10.0, 2.0);
    let right_margin = Rect::new(10.0, 0.0, 2.0, 2.0);
    let mut state = FrameDisplayState::new(12, 2, 1.0, 1.0);
    let mut matrix = GlyphMatrix::new(2, 10);

    for (row_index, text) in ["x", "long"].into_iter().enumerate() {
        let mut row = GlyphRow::new(GlyphRowRole::Text);
        row.enabled = true;
        for (charpos, ch) in text.chars().enumerate() {
            row.glyphs[GlyphArea::Text.index()].push(Glyph::char(ch, FaceId::new(0), charpos));
        }
        row.glyphs[GlyphArea::RightMargin.index()].extend([
            Glyph::char('R', FaceId::new(0), 0),
            Glyph::char('M', FaceId::new(0), 0),
        ]);
        matrix.rows[row_index] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(row);
    }

    state.window_matrices.push(WindowMatrixEntry {
        window_id: window,
        matrix,
        pixel_bounds: outer,
        text_pixel_bounds: text_body,
        text_clip_bounds: Some(text_body),
        selected: true,
    });
    state.window_infos.push(WindowInfo {
        window_id: window,
        buffer_id: 1,
        window_start: 1,
        window_end: 1,
        buffer_size: 1,
        buffer_modiff: neomacs_display_protocol::presentation_origin::BufferModiff::default(),
        bounds: outer,
        geometry: PresentedWindowGeometry::Complete {
            cell_origin: PresentedCellOrigin::default(),
            regions: PresentedWindowRegions {
                outer,
                text_body,
                right_margin_columns: 2,
                right_margin: Some(right_margin),
                ..PresentedWindowRegions::default()
            },
        },
        line_number_field: None,
        mode_line_height: 0.0,
        header_line_height: 0.0,
        tab_line_height: 0.0,
        selected: true,
        is_minibuffer: false,
        char_height: 1.0,
        buffer_name: String::new(),
        buffer_file_name: String::new(),
        modified: false,
    });

    let mut rif = TtyRif::new(12, 2);
    rif.rasterize(&state);

    assert_eq!(desired_char(&rif, 0, 10), 'R');
    assert_eq!(desired_char(&rif, 0, 11), 'M');
    assert_eq!(desired_char(&rif, 1, 10), 'R');
    assert_eq!(desired_char(&rif, 1, 11), 'M');

    let _ = render_output(&mut rif);
    state.window_matrices[0]
        .matrix
        .set_row_damage(0, RowDamage::Reused);
    state.window_matrices[0]
        .matrix
        .set_row_damage(1, RowDamage::Reused);
    rif.rasterize(&state);

    assert_eq!(
        desired_char(&rif, 0, 10),
        'R',
        "a reused row must carry its structural right margin"
    );
    assert_eq!(desired_char(&rif, 0, 11), 'M');
    assert_eq!(desired_char(&rif, 1, 10), 'R');
    assert_eq!(desired_char(&rif, 1, 11), 'M');
    assert!(
        rif.plan_for_test().is_empty(),
        "an unchanged structural margin must not be repainted"
    );
}

#[test]
fn rasterize_anchors_a_synthetic_right_border_after_short_text() {
    let window = DisplayWindowId::new(1);
    let outer = Rect::new(0.0, 0.0, 5.0, 1.0);
    let mut state = FrameDisplayState::new(5, 1, 1.0, 1.0);
    let mut matrix = GlyphMatrix::new(1, 5);
    let row = neomacs_display_protocol::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]);
    row.enabled = true;
    row.glyphs[GlyphArea::Text.index()].push(Glyph::char('x', FaceId::new(0), 0));
    row.glyphs[GlyphArea::RightMargin.index()].push(Glyph::char('|', FaceId::new(0), 0));
    state.window_matrices.push(WindowMatrixEntry {
        window_id: window,
        matrix,
        pixel_bounds: outer,
        text_pixel_bounds: outer,
        text_clip_bounds: Some(outer),
        selected: true,
    });
    state.window_infos.push(WindowInfo {
        window_id: window,
        buffer_id: 1,
        window_start: 1,
        window_end: 1,
        buffer_size: 1,
        buffer_modiff: neomacs_display_protocol::presentation_origin::BufferModiff::default(),
        bounds: outer,
        geometry: PresentedWindowGeometry::Complete {
            cell_origin: PresentedCellOrigin::default(),
            regions: PresentedWindowRegions {
                outer,
                text_body: outer,
                ..PresentedWindowRegions::default()
            },
        },
        line_number_field: None,
        mode_line_height: 0.0,
        header_line_height: 0.0,
        tab_line_height: 0.0,
        selected: true,
        is_minibuffer: false,
        char_height: 1.0,
        buffer_name: String::new(),
        buffer_file_name: String::new(),
        modified: false,
    });

    let mut rif = TtyRif::new(5, 1);
    rif.rasterize(&state);

    assert_eq!(desired_char(&rif, 0, 0), 'x');
    assert_eq!(
        desired_char(&rif, 0, 4),
        '|',
        "GNU reserves the final window-matrix cell for the vertical border"
    );
}

#[test]
fn rasterize_places_a_mode_line_right_border_in_the_final_window_cell() {
    // GNU installs a vertical-border glyph in LAST_AREA on mode-line rows as
    // well as body-text rows.  The row stays window-wide, but the right-margin
    // glyph must use the structurally reserved final cell rather than flowing
    // one cell past the window edge.
    let mut state = FrameDisplayState::new(5, 1, 1.0, 1.0);
    let mut matrix = GlyphMatrix::new(1, 5);
    let row = neomacs_display_protocol::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]);
    row.enabled = true;
    row.role = GlyphRowRole::ModeLine;
    for _ in 0..4 {
        row.glyphs[GlyphArea::Text.index()].push(Glyph::char('-', FaceId::new(0), 0));
    }
    row.glyphs[GlyphArea::RightMargin.index()].push(Glyph::char('|', FaceId::new(0), 0));
    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 5.0, 1.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 5.0, 1.0),
        text_clip_bounds: None,
        selected: true,
    });

    let mut rif = TtyRif::new(5, 1);
    rif.rasterize(&state);

    assert_eq!(rif.dump_desired(), vec!["----|".to_owned()]);
}

#[test]
fn later_window_blank_rows_do_not_inherit_an_overlapped_windows_face() {
    // GNU `build_frame_matrix_from_window_tree` copies each leaf matrix into
    // the frame matrix from left to right/top to bottom.  A later leaf owns
    // every cell in its slice, including default-face blanks; those blanks do
    // not inherit the face of a glyph copied earlier at the same frame cell.
    //
    // This is the split-window + full-width-minibuffer shape from
    // `quit_describe_bindings_via_q`: a surplus row in the left matrix first
    // places its reserved right-border cell on the minibuffer row.  The
    // minibuffer's empty row must replace both the glyph and its face.
    let mut state = FrameDisplayState::new(5, 3, 1.0, 1.0);
    let mut overlapped_face = Face::new(FaceId::new(2));
    overlapped_face.use_default_foreground = false;
    overlapped_face.use_default_background = false;
    overlapped_face.terminal_foreground = Some(TerminalColor::Direct {
        r: 204,
        g: 204,
        b: 204,
    });
    overlapped_face.terminal_background = Some(TerminalColor::Direct {
        r: 77,
        g: 77,
        b: 77,
    });
    state.faces.insert(FaceId::new(2), overlapped_face);

    let mut left_matrix = GlyphMatrix::new(3, 5);
    let overflow =
        neomacs_display_protocol::glyph_matrix::MatrixRow::make_mut(&mut left_matrix.rows[2]);
    overflow.enabled = true;
    overflow.glyphs[GlyphArea::RightMargin.index()].push(Glyph::char('|', FaceId::new(2), 0));
    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix: left_matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 5.0, 2.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 5.0, 2.0),
        text_clip_bounds: Some(Rect::new(0.0, 0.0, 5.0, 1.0)),
        selected: true,
    });

    let mut minibuffer_matrix = GlyphMatrix::new(1, 5);
    let minibuffer_row =
        neomacs_display_protocol::glyph_matrix::MatrixRow::make_mut(&mut minibuffer_matrix.rows[0]);
    minibuffer_row.enabled = true;
    minibuffer_row.ends_at_zv = true;
    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(2),
        matrix: minibuffer_matrix,
        pixel_bounds: Rect::new(0.0, 2.0, 5.0, 1.0),
        text_pixel_bounds: Rect::new(0.0, 2.0, 5.0, 1.0),
        text_clip_bounds: Some(Rect::new(0.0, 2.0, 5.0, 1.0)),
        selected: false,
    });

    let mut rif = TtyRif::new(5, 3);
    rif.rasterize(&state);

    let cell = &rif.desired.cells[2 * rif.width() + 4];
    assert_eq!(cell.ch, ' ');
    assert_eq!(cell.attrs, CellAttrs::default());
    assert_eq!(cell.blank_erase, BlankErase::DefaultFace);
}

#[test]
fn rasterize_clips_text_glyphs_and_reserved_border_to_the_text_band() {
    // Window-system matrices may reserve surplus rows for partially visible
    // glyphs, while GNU's TTY matrix is exactly WINDOW_TOTAL_LINES.  The
    // shared display protocol therefore carries an explicit text clip: a TTY
    // projection must honor it for every glyph area, including the synthetic
    // right-border cell, so no surplus row can escape into another window.
    let mut state = FrameDisplayState::new(5, 3, 1.0, 1.0);
    let mut matrix = GlyphMatrix::new(3, 5);
    let overflow = neomacs_display_protocol::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[2]);
    overflow.enabled = true;
    overflow.glyphs[GlyphArea::Text.index()].push(Glyph::char('X', FaceId::new(0), 0));
    overflow.glyphs[GlyphArea::RightMargin.index()].push(Glyph::char('|', FaceId::new(0), 0));
    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 5.0, 2.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 5.0, 2.0),
        text_clip_bounds: Some(Rect::new(0.0, 0.0, 5.0, 1.0)),
        selected: true,
    });

    let mut rif = TtyRif::new(5, 3);
    rif.rasterize(&state);

    assert_eq!(desired_char(&rif, 2, 0), ' ');
    assert_eq!(desired_char(&rif, 2, 4), ' ');
}

#[test]
fn tty_line_end_filler_with_default_sgr_still_preserves_nondefault_face_identity() {
    let mut state = FrameDisplayState::new(5, 1, 1.0, 1.0);
    let mut comment = Face::new(FaceId::new(1));
    comment.foreground = Color::from_pixel(0x00ff7f24);
    // What makes this face terminal-distinct from the default is its REALIZED
    // terminal colour, not its pixel: the pixel is never written to a tty.
    comment.terminal_foreground = Some(TerminalColor::Indexed(208));
    comment.use_default_background = true;
    let mut default_face = Face::new(FaceId::new(0));
    default_face.use_default_foreground = true;
    default_face.use_default_background = true;
    state.faces.insert(FaceId::new(0), default_face);
    state.faces.insert(FaceId::new(1), comment);

    let mut matrix = GlyphMatrix::new(1, 5);
    let row = neomacs_display_protocol::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]);
    row.enabled = true;
    row.glyphs[GlyphArea::Text.index()].push(Glyph::char('#', FaceId::new(1), 0));
    row.glyphs[GlyphArea::Text.index()].push(Glyph::char(
        ' ',
        FaceId::new(1),
        neomacs_display_protocol::glyph_matrix::NO_BUFFER_POSITION_CHARPOS,
    ));
    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 5.0, 1.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 5.0, 1.0),
        text_clip_bounds: None,
        selected: true,
    });

    let mut caps = TermCaps::default();
    caps.synchronized_output = false;
    let mut rif = TtyRif::new_with_caps(5, 1, caps);
    rif.rasterize(&state);

    assert_eq!(
        rif.plan_for_test(),
        vec![TermOp::WriteRun {
            row: 0,
            start: 0,
            end: 5,
        }],
        "GNU's filtered TTY line-end face looks default but is not CHAR_GLYPH_SPACE_P",
    );
    assert_eq!(rif.desired.cells[2].attrs, CellAttrs::default());
}

#[test]
fn tty_line_end_filler_from_a_nonzero_default_style_face_remains_erasable() {
    let mut state = FrameDisplayState::new(5, 1, 1.0, 1.0);
    let mut default_face = Face::new(FaceId::new(0));
    default_face.use_default_foreground = true;
    default_face.use_default_background = true;
    let mut resolved_default = Face::new(FaceId::new(21));
    resolved_default.use_default_foreground = true;
    resolved_default.use_default_background = true;
    state.faces.insert(FaceId::new(0), default_face);
    state.faces.insert(FaceId::new(21), resolved_default);

    let mut matrix = GlyphMatrix::new(1, 5);
    let row = neomacs_display_protocol::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]);
    row.enabled = true;
    row.glyphs[GlyphArea::Text.index()].push(Glyph::char('x', FaceId::new(21), 0));
    row.glyphs[GlyphArea::Text.index()].push(Glyph::char(
        ' ',
        FaceId::new(21),
        neomacs_display_protocol::glyph_matrix::NO_BUFFER_POSITION_CHARPOS,
    ));
    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 5.0, 1.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 5.0, 1.0),
        text_clip_bounds: None,
        selected: true,
    });

    let mut caps = TermCaps::default();
    caps.synchronized_output = false;
    let mut rif = TtyRif::new_with_caps(5, 1, caps);
    rif.rasterize(&state);

    assert_eq!(
        rif.plan_for_test(),
        vec![
            TermOp::WriteRun {
                row: 0,
                start: 0,
                end: 1,
            },
            TermOp::EraseToEol {
                row: 0,
                from: 1,
                bg: None,
            },
        ],
        "layout may intern GNU's default newline face under a nonzero stable FaceId",
    );
}

#[test]
fn rasterize_respects_matrix_position() {
    let mut state = FrameDisplayState::new(20, 10, 8.0, 16.0);
    state.background = Color::rgb(0.0, 0.0, 0.0);

    let mut matrix = GlyphMatrix::new(3, 10);
    matrix.matrix_x = 5;
    matrix.matrix_y = 2;
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.glyphs[GlyphArea::Text as usize].push(Glyph::char('A', FaceId::new(0), 0));
    matrix.rows[0] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(row);

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(40.0, 32.0, 80.0, 48.0),
        text_pixel_bounds: Rect::new(40.0, 32.0, 80.0, 48.0),
        text_clip_bounds: None,
        selected: true,
    });

    let mut rif = TtyRif::new(20, 10);
    rif.rasterize(&state);

    // 'A' should be at row=2, col=5.
    let idx = 2 * 20 + 5;
    assert_eq!(rif.desired.cells[idx].ch, 'A');
    // row=0 col=0 should still be blank.
    assert_eq!(rif.desired.cells[0].ch, ' ');
}

#[test]
fn rasterize_face_fill_paints_blank_cells_before_glyphs() {
    let mut state = FrameDisplayState::new(8, 2, 1.0, 1.0);
    state.background = Color::from_pixel(0x000000);
    let mut default_face = Face::new(FaceId::new(0));
    default_face.background = Color::from_pixel(0x000000);
    default_face.terminal_background = Some(TerminalColor::Indexed(0));
    let mut fill_face = Face::new(FaceId::new(7));
    fill_face.background = Color::from_pixel(0x112233);
    fill_face.terminal_background = Some(TerminalColor::Indexed(235));
    let mut glyph_face = Face::new(FaceId::new(8));
    glyph_face.background = Color::from_pixel(0x445566);
    glyph_face.terminal_background = Some(TerminalColor::Indexed(240));
    state.faces.insert(FaceId::new(0), default_face);
    state.faces.insert(FaceId::new(7), fill_face);
    state.faces.insert(FaceId::new(8), glyph_face);
    state.face_fills.push(FaceFillItem {
        window_id: DisplayWindowId::new(1),
        row_role: GlyphRowRole::Text,
        clip_rect: Some(Rect::new(0.0, 0.0, 8.0, 2.0)),
        bounds: Rect::new(0.0, 0.0, 8.0, 1.0),
        face_id: FaceId::new(7),
    });

    let mut matrix = GlyphMatrix::new(1, 6);
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.glyphs[GlyphArea::Text as usize].push(Glyph::char('X', FaceId::new(8), 0));
    matrix.rows[0] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(row);
    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 8.0, 1.0),
        text_pixel_bounds: Rect::new(2.0, 0.0, 6.0, 1.0),
        text_clip_bounds: None,
        selected: true,
    });

    let mut rif = TtyRif::new(8, 2);
    rif.rasterize(&state);

    assert_eq!(rif.desired.cells[0].ch, ' ');
    assert_eq!(
        rif.desired.cells[0].attrs.bg,
        Some(TerminalColor::Indexed(235))
    );
    assert_eq!(
        rif.desired.cells[1].attrs.bg,
        Some(TerminalColor::Indexed(235))
    );
    assert_eq!(rif.desired.cells[2].ch, 'X');
    assert_eq!(
        rif.desired.cells[2].attrs.bg,
        Some(TerminalColor::Indexed(240))
    );
    assert_eq!(
        rif.desired.cells[7].attrs.bg,
        Some(TerminalColor::Indexed(235))
    );
    assert_eq!(
        rif.desired.cells[8].attrs.bg,
        Some(TerminalColor::Indexed(0))
    );
}

#[test]
fn rasterize_uses_grid_rows_not_pixel_row_metrics() {
    let mut state = FrameDisplayState::new(12, 5, 1.0, 1.0);
    state.background = Color::rgb(0.0, 0.0, 0.0);

    let mut matrix = GlyphMatrix::new(3, 12);
    for (row_idx, ch) in ['A', 'B', 'C'].into_iter().enumerate() {
        let mut row = GlyphRow::new(GlyphRowRole::Text);
        row.pixel_y = row_idx as f32 * 13.0;
        row.height_px = 13.0;
        row.ascent_px = 10.0;
        row.glyphs[GlyphArea::Text as usize].push(Glyph::char(ch, FaceId::new(0), row_idx));
        matrix.rows[row_idx] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(row);
    }

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 12.0, 5.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 12.0, 5.0),
        text_clip_bounds: None,
        selected: true,
    });

    let mut rif = TtyRif::new(12, 5);
    rif.rasterize(&state);

    assert_eq!(desired_char(&rif, 0, 0), 'A');
    assert_eq!(desired_char(&rif, 1, 0), 'B');
    assert_eq!(desired_char(&rif, 2, 0), 'C');
}

#[test]
fn rasterize_text_rows_use_text_pixel_bounds_but_chrome_rows_do_not() {
    let mut state = FrameDisplayState::new(12, 3, 8.0, 16.0);
    state.background = Color::rgb(0.0, 0.0, 0.0);

    let mut matrix = GlyphMatrix::new(2, 9);

    let mut text_row = GlyphRow::new(GlyphRowRole::Text);
    text_row.glyphs[GlyphArea::Text as usize].push(Glyph::char('T', FaceId::new(0), 0));
    matrix.rows[0] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(text_row);

    let mut mode_line_row = GlyphRow::new(GlyphRowRole::ModeLine);
    mode_line_row.glyphs[GlyphArea::Text as usize].push(Glyph::char('M', FaceId::new(0), 0));
    matrix.rows[1] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(mode_line_row);

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 96.0, 48.0),
        text_pixel_bounds: Rect::new(24.0, 0.0, 72.0, 32.0),
        text_clip_bounds: None,
        selected: true,
    });

    let mut rif = TtyRif::new(12, 3);
    rif.rasterize(&state);

    assert_eq!(desired_char(&rif, 0, 0), ' ');
    assert_eq!(desired_char(&rif, 0, 3), 'T');
    assert_eq!(desired_char(&rif, 1, 0), 'M');
}

#[test]
fn rasterize_frame_tree_draws_decorated_child_in_z_order() {
    let root = make_grid_state(1, 0, 0.0, 0.0, 12, 6, "root");
    let child = make_grid_state(2, 1, 4.0, 2.0, 3, 1, "M-x");

    let mut rif = TtyRif::new(12, 6);
    rif.rasterize_frame_tree(&root, &[child]);

    assert_eq!(desired_char(&rif, 1, 3), '+');
    assert_eq!(desired_char(&rif, 1, 4), '-');
    assert_eq!(desired_char(&rif, 1, 6), '-');
    assert_eq!(desired_char(&rif, 1, 7), '+');
    assert_eq!(desired_char(&rif, 2, 3), '|');
    assert_eq!(desired_char(&rif, 2, 4), 'M');
    assert_eq!(desired_char(&rif, 2, 5), '-');
    assert_eq!(desired_char(&rif, 2, 6), 'x');
    assert_eq!(desired_char(&rif, 2, 7), '|');
    assert_eq!(desired_char(&rif, 3, 3), '+');
    assert_eq!(desired_char(&rif, 3, 4), '-');
    assert_eq!(desired_char(&rif, 3, 7), '+');
}

#[test]
fn rasterize_frame_tree_skips_border_for_undecorated_child() {
    let root = make_grid_state(1, 0, 0.0, 0.0, 12, 6, "root");
    let mut child = make_grid_state(2, 1, 4.0, 2.0, 3, 1, "M-x");
    child.undecorated = true;

    let mut rif = TtyRif::new(12, 6);
    rif.rasterize_frame_tree(&root, &[child]);

    assert_eq!(desired_char(&rif, 1, 3), ' ');
    assert_eq!(desired_char(&rif, 1, 4), ' ');
    assert_eq!(desired_char(&rif, 1, 7), ' ');
    assert_eq!(desired_char(&rif, 2, 3), ' ');
    assert_eq!(desired_char(&rif, 2, 4), 'M');
    assert_eq!(desired_char(&rif, 2, 5), '-');
    assert_eq!(desired_char(&rif, 2, 6), 'x');
    assert_eq!(desired_char(&rif, 2, 7), ' ');
    assert_eq!(desired_char(&rif, 3, 3), ' ');
    assert_eq!(desired_char(&rif, 3, 4), ' ');
    assert_eq!(desired_char(&rif, 3, 7), ' ');
}

#[test]
fn rasterize_frame_tree_clips_negative_child_origin_without_shifting_its_content() {
    let root = make_grid_state(1, 0, 0.0, 0.0, 6, 3, "root");
    let mut child = make_grid_state(2, 1, -1.0, 0.0, 3, 1, "ABC");
    child.undecorated = true;

    let mut rif = TtyRif::new(6, 3);
    rif.rasterize_frame_tree(&root, &[child]);

    assert_eq!(desired_char(&rif, 0, 0), 'B');
    assert_eq!(desired_char(&rif, 0, 1), 'C');
    assert_eq!(desired_char(&rif, 0, 2), 'o');
}

#[test]
fn rasterize_frame_tree_clips_negative_child_rows_without_shifting_the_source_row() {
    let root = make_grid_state(1, 0, 0.0, 0.0, 6, 3, "root");
    let mut child = make_grid_state(2, 1, 0.0, -1.0, 2, 2, "AA");
    child.undecorated = true;
    let mut second_row = GlyphRow::new(GlyphRowRole::Text);
    second_row.glyphs[GlyphArea::Text as usize].push(Glyph::char('B', FaceId::new(0), 0));
    second_row.glyphs[GlyphArea::Text as usize].push(Glyph::char('B', FaceId::new(0), 1));
    child.window_matrices[0].matrix.rows[1] =
        neomacs_display_protocol::glyph_matrix::MatrixRow::new(second_row);

    let mut rif = TtyRif::new(6, 3);
    rif.rasterize_frame_tree(&root, &[child]);

    assert_eq!(desired_char(&rif, 0, 0), 'B');
    assert_eq!(desired_char(&rif, 0, 1), 'B');
    assert_eq!(desired_char(&rif, 1, 0), ' ');
}

#[test]
fn rasterize_frame_tree_clips_a_decorated_child_at_the_left_edge() {
    let root = make_grid_state(1, 0, 0.0, 0.0, 6, 4, "root");
    let child = make_grid_state(2, 1, -1.0, 1.0, 3, 1, "ABC");

    let mut rif = TtyRif::new(6, 4);
    rif.rasterize_frame_tree(&root, &[child]);

    assert_eq!(
        [
            desired_char(&rif, 0, 0),
            desired_char(&rif, 0, 1),
            desired_char(&rif, 0, 2),
        ],
        ['-', '-', '+']
    );
    assert_eq!(
        [
            desired_char(&rif, 1, 0),
            desired_char(&rif, 1, 1),
            desired_char(&rif, 1, 2),
        ],
        ['B', 'C', '|']
    );
    assert_eq!(
        [
            desired_char(&rif, 2, 0),
            desired_char(&rif, 2, 1),
            desired_char(&rif, 2, 2),
        ],
        ['-', '-', '+']
    );
}

#[test]
fn rasterize_fully_clipped_decorated_child_suppresses_edge_only_border_like_gnu() {
    let root = make_grid_state(1, 0, 0.0, 0.0, 6, 3, "root");
    let child = make_grid_state(2, 1, -3.0, 1.0, 3, 1, "ABC");

    let mut rif = TtyRif::new(6, 3);
    rif.rasterize_frame_tree(&root, &[child]);

    // GNU's copy_child_glyphs returns before drawing borders when the child
    // frame rectangle has no interior intersection with the root.
    assert_eq!(desired_char(&rif, 1, 0), ' ');
}

#[test]
fn rasterize_frame_tree_hides_a_child_cursor_clipped_off_the_left_edge() {
    let root = make_grid_state(1, 0, 0.0, 0.0, 6, 3, "root");
    let mut child = make_grid_state(2, 1, -2.0, 0.0, 3, 1, "ABC");
    child.undecorated = true;
    child.phys_cursor = Some(PhysCursor {
        window_id: DisplayWindowId::new(102),
        charpos: 0,
        row: 0,
        col: 0,
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
        ascent: 1.0,
        style: CursorStyle::FilledBox,
        color: Color::WHITE,
        slot_id: DisplaySlotId {
            window_id: DisplayWindowId::new(102),
            row: 0,
            col: 0,
        },
        cursor_fg: Color::BLACK,
    });

    let mut rif = TtyRif::new(6, 3);
    rif.rasterize_frame_tree(&root, &[child]);

    assert!(!rif.cursor_visible);
}

#[test]
fn clipping_a_wide_child_glyph_does_not_leave_an_unrenderable_padding_cell() {
    let root = make_grid_state(1, 0, 0.0, 0.0, 4, 2, "root");
    let mut child = make_grid_state(2, 1, -1.0, 0.0, 2, 1, "");
    child.undecorated = true;
    let row = neomacs_display_protocol::glyph_matrix::MatrixRow::make_mut(
        &mut child.window_matrices[0].matrix.rows[0],
    );
    let mut wide = Glyph::char('\u{4f60}', FaceId::new(0), 0);
    wide.wide = true;
    row.glyphs[GlyphArea::Text as usize].push(wide);
    row.glyphs[GlyphArea::Text as usize].push(Glyph::padding_for(FaceId::new(0), 0));

    let mut rif = TtyRif::new(4, 2);
    rif.rasterize_frame_tree(&root, &[child]);

    assert_eq!(desired_char(&rif, 0, 0), ' ');
    assert!(!rif.desired.cells[0].padding);
}

#[test]
fn rasterize_disabled_rows_are_skipped() {
    let mut state = FrameDisplayState::new(10, 5, 8.0, 16.0);
    state.background = Color::rgb(0.0, 0.0, 0.0);

    let mut matrix = GlyphMatrix::new(5, 10);
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.glyphs[GlyphArea::Text as usize].push(Glyph::char('X', FaceId::new(0), 0));
    row.enabled = false;
    matrix.rows[0] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(row);

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 80.0, 80.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 80.0, 80.0),
        text_clip_bounds: None,
        selected: true,
    });

    let mut rif = TtyRif::new(10, 5);
    rif.rasterize(&state);

    // Row 0 should be blank because the glyph row is disabled.
    assert_eq!(rif.desired.cells[0].ch, ' ');
}

// ---------------------------------------------------------------------------
// Wide character handling
// ---------------------------------------------------------------------------

#[test]
fn rasterize_wide_char_creates_padding() {
    let mut state = FrameDisplayState::new(10, 5, 8.0, 16.0);
    state.background = Color::BLACK;

    let mut matrix = GlyphMatrix::new(5, 10);
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    // CJK character, wide=true.
    let mut g = Glyph::char('\u{4e16}', FaceId::new(0), 0); // Unicode: "world" in Chinese
    g.wide = true;
    row.glyphs[GlyphArea::Text as usize].push(g);
    // Followed by a normal char.
    row.glyphs[GlyphArea::Text as usize].push(Glyph::char('!', FaceId::new(0), 1));
    matrix.rows[0] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(row);

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 80.0, 80.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 80.0, 80.0),
        text_clip_bounds: None,
        selected: true,
    });

    let mut rif = TtyRif::new(10, 5);
    rif.rasterize(&state);

    // Col 0: the wide char.
    assert_eq!(rif.desired.cells[0].ch, '\u{4e16}');
    assert!(!rif.desired.cells[0].padding);
    // Col 1: padding cell.
    assert!(rif.desired.cells[1].padding);
    // Col 2: '!'
    assert_eq!(rif.desired.cells[2].ch, '!');
    assert!(!rif.desired.cells[2].padding);
}

#[test]
fn rasterize_explicit_padding_glyph_is_not_duplicated() {
    let mut state = FrameDisplayState::new(10, 5, 8.0, 16.0);
    state.background = Color::BLACK;

    let mut matrix = GlyphMatrix::new(5, 10);
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let mut wide = Glyph::char('\u{4f60}', FaceId::new(0), 0);
    wide.wide = true;
    row.glyphs[GlyphArea::Text as usize].push(wide);
    row.glyphs[GlyphArea::Text as usize].push(Glyph::padding_for(FaceId::new(0), 0));
    row.glyphs[GlyphArea::Text as usize].push(Glyph::char('!', FaceId::new(0), 1));
    matrix.rows[0] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(row);

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 80.0, 80.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 80.0, 80.0),
        text_clip_bounds: None,
        selected: true,
    });

    let mut rif = TtyRif::new(10, 5);
    rif.rasterize(&state);

    assert_eq!(rif.desired.cells[0].ch, '\u{4f60}');
    assert!(rif.desired.cells[1].padding);
    assert_eq!(rif.desired.cells[2].ch, '!');
    assert!(!rif.desired.cells[2].padding);
}

#[test]
fn rasterize_stretch_glyph_uses_declared_width() {
    let mut state = FrameDisplayState::new(10, 5, 8.0, 16.0);
    state.background = Color::BLACK;

    let mut matrix = GlyphMatrix::new(5, 10);
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.glyphs[GlyphArea::Text as usize].push(Glyph::char('A', FaceId::new(0), 0));
    row.glyphs[GlyphArea::Text as usize].push(Glyph::stretch(4, FaceId::new(0)));
    row.glyphs[GlyphArea::Text as usize].push(Glyph::char('B', FaceId::new(0), 1));
    matrix.rows[0] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(row);

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 80.0, 80.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 80.0, 80.0),
        text_clip_bounds: None,
        selected: true,
    });

    let mut rif = TtyRif::new(10, 5);
    rif.rasterize(&state);

    assert_eq!(rif.desired.cells[0].ch, 'A');
    assert_eq!(rif.desired.cells[1].ch, ' ');
    assert_eq!(rif.desired.cells[2].ch, ' ');
    assert_eq!(rif.desired.cells[3].ch, ' ');
    assert_eq!(rif.desired.cells[4].ch, ' ');
    assert_eq!(rif.desired.cells[5].ch, 'B');
}

// ---------------------------------------------------------------------------
// Cursor tracking
// ---------------------------------------------------------------------------

#[test]
fn rasterize_tracks_phys_cursor_position() {
    let mut state = FrameDisplayState::new(10, 5, 8.0, 16.0);
    state.background = Color::BLACK;

    let mut matrix = GlyphMatrix::new(5, 10);
    matrix.matrix_x = 0;
    matrix.matrix_y = 0;
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.glyphs[GlyphArea::Text as usize].push(Glyph::char('a', FaceId::new(0), 0));
    row.glyphs[GlyphArea::Text as usize].push(Glyph::char('b', FaceId::new(0), 1));
    matrix.rows[0] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(row);

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 80.0, 80.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 80.0, 80.0),
        text_clip_bounds: None,
        selected: true,
    });
    state.phys_cursor = Some(PhysCursor {
        window_id: DisplayWindowId::new(1),
        charpos: 1,
        row: 0,
        col: 1,
        x: 8.0,
        y: 0.0,
        width: 8.0,
        height: 16.0,
        ascent: 12.0,
        style: CursorStyle::FilledBox,
        color: Color::WHITE,
        slot_id: DisplaySlotId {
            window_id: DisplayWindowId::new(1),
            row: 0,
            col: 1,
        },
        cursor_fg: Color::BLACK,
    });

    let mut rif = TtyRif::new(10, 5);
    rif.rasterize(&state);

    assert!(rif.cursor_visible);
    assert_eq!(rif.cursor_row, 0);
    assert_eq!(rif.cursor_col, 1);
    assert_eq!(rif.cursor_shape, TerminalCursorShape::Block);
}

#[test]
fn rasterize_prefers_phys_cursor_over_matrix_cursor_columns() {
    let mut state = FrameDisplayState::new(10, 5, 8.0, 16.0);
    state.background = Color::BLACK;

    let mut matrix = GlyphMatrix::new(5, 10);
    let mut row0 = GlyphRow::new(GlyphRowRole::Text);
    row0.glyphs[GlyphArea::Text as usize].push(Glyph::char('a', FaceId::new(0), 0));
    row0.cursor_col = Some(1);
    row0.cursor_type = Some(CursorStyle::FilledBox);
    matrix.rows[0] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(row0);

    let mut row1 = GlyphRow::new(GlyphRowRole::Text);
    row1.glyphs[GlyphArea::Text as usize].push(Glyph::char('b', FaceId::new(0), 1));
    matrix.rows[1] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(row1);

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 80.0, 80.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 80.0, 80.0),
        text_clip_bounds: None,
        selected: true,
    });
    state.phys_cursor = Some(PhysCursor {
        window_id: DisplayWindowId::new(1),
        charpos: 1,
        row: 1,
        col: 4,
        x: 32.0,
        y: 16.0,
        width: 8.0,
        height: 16.0,
        ascent: 12.0,
        style: CursorStyle::FilledBox,
        color: Color::WHITE,
        slot_id: DisplaySlotId {
            window_id: DisplayWindowId::new(1),
            row: 1,
            col: 4,
        },
        cursor_fg: Color::BLACK,
    });

    let mut rif = TtyRif::new(10, 5);
    rif.rasterize(&state);

    assert!(rif.cursor_visible);
    assert_eq!(rif.cursor_row, 1);
    assert_eq!(rif.cursor_col, 4);
}

#[test]
fn tty_frame_chrome_rasterizes_menu_and_tab_bands_in_order() {
    use neomacs_display_protocol::frame_chrome::{
        BandRect, ChromeAction, ChromeBandRequest, ChromeDisplayRow, FrameChrome,
        FrameChromeContent, FrameChromeKind, FrameSize, MenuBarContent, PositionedChromeItem,
    };
    use neomacs_display_protocol::ui_types::MenuBarItem;

    let mut state = FrameDisplayState::new(10, 5, 1.0, 1.0);
    state.background = Color::BLACK;

    let mut row = GlyphRow::new(GlyphRowRole::TabBar);
    row.enabled = true;
    row.mode_line = true;
    row.displays_text = true;
    row.height_px = 1.0;
    row.ascent_px = 1.0;
    row.glyphs[GlyphArea::Text as usize].push(Glyph::char('T', FaceId::new(0), 0));
    row.glyphs[GlyphArea::Text as usize].push(Glyph::char('B', FaceId::new(0), 1));

    let menu = MenuBarContent::new(
        vec![PositionedChromeItem::new(
            BandRect::new(0.0, 0.0, 5.0, 1.0).expect("menu item bounds"),
            MenuBarItem {
                index: 0,
                label: "File".into(),
                key: "file".into(),
            },
            ChromeAction::OpenMenu {
                index: 0,
                key: "file".into(),
            },
        )],
        Color::WHITE,
        Color::BLACK,
    );
    state.frame_chrome = FrameChrome::layout(
        FrameSize::new(10.0, 5.0).expect("frame size"),
        vec![
            ChromeBandRequest::new(
                FrameChromeKind::MenuBar,
                1.0,
                FrameChromeContent::MenuBar(menu),
            ),
            ChromeBandRequest::new(
                FrameChromeKind::TabBar,
                1.0,
                FrameChromeContent::DisplayRow(ChromeDisplayRow::new(row)),
            ),
        ],
    )
    .expect("frame chrome");

    let mut rif = TtyRif::new(10, 5);
    rif.rasterize(&state);

    assert_eq!(rif.desired.cells[0].ch, 'F');
    assert_eq!(rif.desired.cells[1].ch, 'i');
    assert_eq!(rif.desired.cells[10].ch, 'T');
    assert_eq!(rif.desired.cells[11].ch, 'B');
}

#[test]
fn rasterize_ignores_matrix_cursor_columns_without_phys_cursor() {
    let mut state = FrameDisplayState::new(10, 5, 8.0, 16.0);
    state.background = Color::BLACK;

    let mut matrix = GlyphMatrix::new(5, 10);
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.glyphs[GlyphArea::Text as usize].push(Glyph::char('a', FaceId::new(0), 0));
    row.cursor_col = Some(1);
    row.cursor_type = Some(CursorStyle::FilledBox);
    matrix.rows[0] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(row);

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 80.0, 80.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 80.0, 80.0),
        text_clip_bounds: None,
        selected: true,
    });

    let mut rif = TtyRif::new(10, 5);
    rif.rasterize(&state);

    assert!(!rif.cursor_visible);
}

#[test]
fn rasterize_keeps_phys_filled_box_cursor_out_of_cell_attrs() {
    let mut state = FrameDisplayState::new(10, 5, 8.0, 16.0);
    state.background = Color::BLACK;
    let mut default_face = Face::new(FaceId::new(0));
    default_face.use_default_foreground = true;
    default_face.use_default_background = true;
    state.faces.insert(FaceId::new(0), default_face);

    let mut matrix = GlyphMatrix::new(5, 10);
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.glyphs[GlyphArea::Text as usize].push(Glyph::char('a', FaceId::new(0), 0));
    row.glyphs[GlyphArea::Text as usize].push(Glyph::char('b', FaceId::new(0), 1));
    matrix.rows[0] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(row);

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 80.0, 80.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 80.0, 80.0),
        text_clip_bounds: None,
        selected: true,
    });
    state.phys_cursor = Some(PhysCursor {
        window_id: DisplayWindowId::new(1),
        charpos: 1,
        row: 0,
        col: 1,
        x: 8.0,
        y: 0.0,
        width: 8.0,
        height: 16.0,
        ascent: 12.0,
        style: CursorStyle::FilledBox,
        color: Color::RED,
        slot_id: DisplaySlotId {
            window_id: DisplayWindowId::new(1),
            row: 0,
            col: 1,
        },
        cursor_fg: Color::BLACK,
    });

    let mut rif = TtyRif::new(10, 5);
    rif.rasterize(&state);

    let cell = &rif.desired.cells[1];
    assert_eq!(cell.ch, 'b');
    assert_eq!(cell.attrs.bg, None);
    assert_eq!(cell.attrs.fg, None);
    assert!(rif.cursor_visible);
    assert_eq!(rif.cursor_row, 0);
    assert_eq!(rif.cursor_col, 1);
}

#[test]
fn rasterize_ignores_nonselected_hollow_cursor_visual_on_tty() {
    let mut state = FrameDisplayState::new(10, 5, 8.0, 16.0);
    state.background = Color::BLACK;

    let mut matrix = GlyphMatrix::new(2, 10);
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.glyphs[GlyphArea::Text as usize].push(Glyph::char('x', FaceId::new(0), 0));
    row.glyphs[GlyphArea::Text as usize].push(Glyph::char('y', FaceId::new(0), 1));
    row.cursor_col = Some(1);
    row.cursor_type = Some(CursorStyle::Hollow);
    matrix.rows[0] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(row);

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(9),
        matrix,
        pixel_bounds: Rect::new(0.0, 16.0, 80.0, 32.0),
        text_pixel_bounds: Rect::new(0.0, 16.0, 80.0, 32.0),
        text_clip_bounds: None,
        selected: false,
    });

    let mut rif = TtyRif::new(10, 5);
    rif.rasterize(&state);

    let row_start = rif.width();
    let cell = &rif.desired.cells[row_start + 1];
    assert_eq!(cell.ch, 'y');
    assert!(!cell.attrs.inverse);
    assert!(!rif.cursor_visible);
}

#[test]
fn rasterize_uses_hardware_bar_shape_for_phys_bar_cursor() {
    let mut state = FrameDisplayState::new(10, 5, 8.0, 16.0);
    state.background = Color::BLACK;

    let mut matrix = GlyphMatrix::new(5, 10);
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.glyphs[GlyphArea::Text as usize].push(Glyph::char('a', FaceId::new(0), 0));
    matrix.rows[0] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(row);

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 80.0, 80.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 80.0, 80.0),
        text_clip_bounds: None,
        selected: true,
    });
    state.phys_cursor = Some(PhysCursor {
        window_id: DisplayWindowId::new(1),
        charpos: 0,
        row: 0,
        col: 0,
        x: 0.0,
        y: 0.0,
        width: 2.0,
        height: 16.0,
        ascent: 12.0,
        style: CursorStyle::Bar(2.0),
        color: Color::WHITE,
        slot_id: DisplaySlotId {
            window_id: DisplayWindowId::new(1),
            row: 0,
            col: 0,
        },
        cursor_fg: Color::BLACK,
    });

    let mut rif = TtyRif::new(10, 5);
    rif.rasterize(&state);

    assert!(rif.cursor_visible);
    assert_eq!(rif.cursor_shape, TerminalCursorShape::Bar);
    assert!(!rif.desired.cells[0].attrs.inverse);
}

/// Regression test for a bug observed after `C-x 2` in an
/// interactive `neomacs -nw -Q` session: the physical terminal
/// cursor ended up inside the newly-created (non-selected)
/// bottom window because the TTY RIF iterated both windows'
/// glyph matrices and let the LAST `cursor_col` it saw win,
/// clobbering the selected window's cursor with the hollow
/// cursor hint drawn for the non-selected window.
///
/// GNU Emacs has a dedicated `tty_set_cursor` in
/// `src/dispnew.c:5670-5751` that explicitly uses
/// `FRAME_SELECTED_WINDOW (f)` and only calls `cursor_to` once,
/// with this comment:
///
///   /* We have only one cursor on terminal frames. Use it to
///      display the cursor of the selected window of the
///      frame.  */
///   struct window *w = XWINDOW (FRAME_SELECTED_WINDOW (f));
///   ...
///   cursor_to (f, y, x);
///
/// The `selected: bool` field on `WindowMatrixEntry` is the
/// per-frame-state equivalent of GNU's `FRAME_SELECTED_WINDOW`
/// check: only the selected window contributes the frame-level
/// `phys_cursor` used for the terminal cursor geometry/position.
/// Non-selected windows may still mark `cursor_col`, but on TTY
/// frames GNU has no per-window cursor painting path; only the
/// frame-level terminal cursor is moved.
#[test]
fn rasterize_terminal_cursor_comes_from_selected_window_only() {
    // Two vertically stacked 2-row windows at screen cols 0..10.
    // Top window (w1) is selected; its cursor is in row 0, col 3.
    // Bottom window (w2) is NOT selected but still has a
    // hollow cursor marker in its row 0, col 7. The terminal
    // cursor MUST come from w1.
    let mut state = FrameDisplayState::new(10, 5, 8.0, 16.0);
    state.background = Color::BLACK;

    let mut top_matrix = GlyphMatrix::new(2, 10);
    let mut top_row = GlyphRow::new(GlyphRowRole::Text);
    for (i, ch) in "TOP-BUFFER".chars().enumerate() {
        top_row.glyphs[GlyphArea::Text as usize].push(Glyph::char(ch, FaceId::new(0), i));
    }
    top_row.cursor_col = Some(3);
    top_row.cursor_type = Some(CursorStyle::FilledBox);
    top_matrix.rows[0] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(top_row);

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix: top_matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 80.0, 32.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 80.0, 32.0),
        text_clip_bounds: None,
        selected: true,
    });

    let mut bot_matrix = GlyphMatrix::new(2, 10);
    let mut bot_row = GlyphRow::new(GlyphRowRole::Text);
    for (i, ch) in "BOT-BUFFER".chars().enumerate() {
        bot_row.glyphs[GlyphArea::Text as usize].push(Glyph::char(ch, FaceId::new(0), i));
    }
    // Non-selected window still marks a hollow cursor column via
    // the same `cursor_col` slot, reflecting the `Hollow` style
    // chosen by `cursor_style_for_window` for windows where
    // `cursor-in-non-selected-windows` is non-nil.
    bot_row.cursor_col = Some(7);
    bot_row.cursor_type = Some(CursorStyle::Hollow);
    bot_matrix.rows[0] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(bot_row);

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(2),
        matrix: bot_matrix,
        // Bottom half of the screen.
        pixel_bounds: Rect::new(0.0, 32.0, 80.0, 32.0),
        text_pixel_bounds: Rect::new(0.0, 32.0, 80.0, 32.0),
        text_clip_bounds: None,
        selected: false,
    });
    state.phys_cursor = Some(PhysCursor {
        window_id: DisplayWindowId::new(1),
        charpos: 3,
        row: 0,
        col: 3,
        x: 24.0,
        y: 0.0,
        width: 8.0,
        height: 16.0,
        ascent: 12.0,
        style: CursorStyle::FilledBox,
        color: Color::WHITE,
        slot_id: DisplaySlotId {
            window_id: DisplayWindowId::new(1),
            row: 0,
            col: 3,
        },
        cursor_fg: Color::BLACK,
    });

    let mut rif = TtyRif::new(10, 5);
    rif.rasterize(&state);

    assert!(
        rif.cursor_visible,
        "TTY filled-box cursor should use the hardware cursor"
    );
    assert_eq!(
        rif.cursor_row, 0,
        "cursor row must come from selected (top) window"
    );
    assert_eq!(
        rif.cursor_col, 3,
        "cursor column must come from selected (top) window — \
         the non-selected bottom window's hollow cursor at col 7 \
         must NOT move the frame-level cursor geometry"
    );
}

/// Complementary test: when the frame layout lists the selected
/// window AFTER a non-selected window, the terminal cursor must
/// still come from the selected window. Without the
/// `entry.selected` guard this case happens to succeed by
/// accident (last-writer-wins lands on the selected window), so
/// we verify it explicitly to pin the intent rather than the
/// iteration order.
#[test]
fn rasterize_terminal_cursor_comes_from_selected_window_regardless_of_order() {
    let mut state = FrameDisplayState::new(10, 5, 8.0, 16.0);
    state.background = Color::BLACK;

    // First entry: non-selected window with a hollow cursor.
    let mut w1_matrix = GlyphMatrix::new(2, 10);
    let mut w1_row = GlyphRow::new(GlyphRowRole::Text);
    for (i, ch) in "FIRST-WIN".chars().enumerate() {
        w1_row.glyphs[GlyphArea::Text as usize].push(Glyph::char(ch, FaceId::new(0), i));
    }
    w1_row.cursor_col = Some(9);
    w1_row.cursor_type = Some(CursorStyle::Hollow);
    w1_matrix.rows[0] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(w1_row);

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix: w1_matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 80.0, 32.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 80.0, 32.0),
        text_clip_bounds: None,
        selected: false,
    });

    // Second entry: the selected window with its real cursor.
    let mut w2_matrix = GlyphMatrix::new(2, 10);
    let mut w2_row = GlyphRow::new(GlyphRowRole::Text);
    for (i, ch) in "SECND-WIN".chars().enumerate() {
        w2_row.glyphs[GlyphArea::Text as usize].push(Glyph::char(ch, FaceId::new(0), i));
    }
    w2_row.cursor_col = Some(2);
    w2_row.cursor_type = Some(CursorStyle::FilledBox);
    w2_matrix.rows[0] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(w2_row);

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(2),
        matrix: w2_matrix,
        pixel_bounds: Rect::new(0.0, 32.0, 80.0, 32.0),
        text_pixel_bounds: Rect::new(0.0, 32.0, 80.0, 32.0),
        text_clip_bounds: None,
        selected: true,
    });
    state.phys_cursor = Some(PhysCursor {
        window_id: DisplayWindowId::new(2),
        charpos: 2,
        row: 2,
        col: 2,
        x: 16.0,
        y: 32.0,
        width: 8.0,
        height: 16.0,
        ascent: 12.0,
        style: CursorStyle::FilledBox,
        color: Color::WHITE,
        slot_id: DisplaySlotId {
            window_id: DisplayWindowId::new(2),
            row: 2,
            col: 2,
        },
        cursor_fg: Color::BLACK,
    });

    let mut rif = TtyRif::new(10, 5);
    rif.rasterize(&state);

    assert!(rif.cursor_visible);
    assert_eq!(rif.cursor_row, 2, "selected window starts at screen row 2");
    assert_eq!(rif.cursor_col, 2, "cursor col from selected window only");
}

// ---------------------------------------------------------------------------
// diff_and_render
// ---------------------------------------------------------------------------

#[test]
fn first_diff_repaints_unknown_terminal() {
    let mut rif = TtyRif::new(10, 5);
    // A fresh terminal's real contents are unknown.  GNU marks new/resized
    // frames garbaged and repaints them before relying on matrix diffs.
    rif.diff_and_render();
    let output = rif.take_output();

    let s = String::from_utf8_lossy(&output);
    assert!(s.contains("\x1b[?25l")); // hide cursor
    // and no `\E[0m`: the first repaint writes default-face blanks, which GNU
    // turns neither on nor off (ledger 188).
    assert!(!s.contains("\x1b[0m"), "{s:?}");

    // The first render repaints every row with one CUP per contiguous row run.
    let cup_count = s.matches("H").count();
    assert!(
        cup_count == 5,
        "Expected 5 CUP moves for initial full repaint, got {}",
        cup_count
    );
}

#[test]
fn magic_wrap_normalizes_a_full_width_run_before_the_adjacent_row_like_gnu() {
    let mut caps = TermCaps::default();
    caps.synchronized_output = false;
    let mut rif = TtyRif::new_with_caps(4, 2, caps);
    for (column, character) in "ABCD".chars().enumerate() {
        rif.desired
            .set(0, column, character, CellAttrs::default(), false);
    }
    rif.desired.set(1, 0, 'E', CellAttrs::default(), false);

    rif.diff_and_render();
    let output = rif.take_output();

    // GNU `tty_write_glyphs_1' advances its tracked cursor with `cmplus', then
    // `cmcheckmagic' resolves the right-margin phantom position with CRLF
    // (term.c:768,816; cm.c:113-132).  The adjacent run therefore needs no
    // absolute cursor address.  Keeping this continuous cursor history also
    // preserves any real terminal wrap caused by a Unicode-width disagreement,
    // which affects copy/reflow semantics and is part of ExactDisplay.
    assert!(
        output.windows(7).any(|bytes| bytes == b"ABCD\r\nE"),
        "magic-wrap should normalize the right margin before the adjacent row: {:?}",
        String::from_utf8_lossy(&output),
    );
    assert!(
        !output
            .windows(b"\x1b[2;1H".len())
            .any(|bytes| bytes == b"\x1b[2;1H"),
        "an absolute move to the normalized adjacent row would erase GNU's cursor provenance",
    );
}

#[test]
fn no_autowrap_addresses_the_adjacent_row_explicitly() {
    let mut caps = TermCaps::default();
    caps.right_margin = RightMarginBehavior::NoAutoWrap;
    caps.synchronized_output = false;
    let mut rif = TtyRif::new_with_caps(4, 2, caps);
    for (column, character) in "ABCD".chars().enumerate() {
        rif.desired
            .set(0, column, character, CellAttrs::default(), false);
    }
    rif.desired.set(1, 0, 'E', CellAttrs::default(), false);

    rif.diff_and_render();
    let output = rif.take_output();

    assert!(
        output
            .windows(b"\x1b[2;1H".len())
            .any(|bytes| bytes == b"\x1b[2;1H"),
        "a terminal without autowrap must receive an explicit adjacent-row address: {:?}",
        String::from_utf8_lossy(&output),
    );
}

#[test]
fn first_diff_writes_content_then_erases_the_default_blank_tail() {
    let mut caps = TermCaps::default();
    caps.synchronized_output = false;
    let mut rif = TtyRif::new_with_caps(5, 1, caps);
    rif.desired.set(0, 0, 'A', CellAttrs::default(), false);

    rif.diff_and_render();
    let output = rif.take_output();

    assert!(
        output
            .windows(b"\x1b[K".len())
            .any(|bytes| bytes == b"\x1b[K"),
        "GNU trims the default blank tail and erases it on an unknown row: {:?}",
        String::from_utf8_lossy(&output),
    );
    assert!(
        !output.windows(5).any(|bytes| bytes == b"A    "),
        "screen-256color lacks termcap `in`, so GNU must not write the tail: {:?}",
        String::from_utf8_lossy(&output),
    );
}

#[test]
fn first_diff_erases_wholly_blank_rows_instead_of_writing_spaces() {
    // GNU never touches the rows below the last content row: after the
    // initial clear (ED) their current-matrix rows compare blank, and a
    // wholly blank desired line is finished with EL (dispnew.c write_row's
    // just_erase path), so the physical terminal cells stay UNWRITTEN.
    // Writing explicit spaces instead is observable terminal state — a
    // pty capture sees " " where GNU's screen has never-written cells
    // (the magit_log_buffer_file_margin_columns_match_gnu_full_screen
    // fixture compares exactly this below-content region).
    let mut caps = TermCaps::default();
    caps.synchronized_output = false;
    let mut rif = TtyRif::new_with_caps(5, 3, caps);
    rif.desired.set(0, 0, 'A', CellAttrs::default(), false);

    assert_eq!(
        rif.plan_for_test(),
        vec![
            TermOp::WriteRun {
                row: 0,
                start: 0,
                end: 1,
            },
            TermOp::EraseToEol {
                row: 0,
                from: 1,
                bg: None,
            },
            TermOp::EraseToEol {
                row: 1,
                from: 0,
                bg: None,
            },
            TermOp::EraseToEol {
                row: 2,
                from: 0,
                bg: None,
            },
        ],
        "blank rows below content must be erased (unwritten cells), never space-filled",
    );
}

#[test]
fn first_content_written_into_an_erased_row_keeps_its_erased_tail() {
    let mut caps = TermCaps::default();
    caps.synchronized_output = false;
    let mut rif = TtyRif::new_with_caps(8, 1, caps);

    // The initial blank repaint uses EL, leaving the physical cells
    // unwritten. GNU dispnew.c's `!olen` path writes only the meaningful
    // content when termcap `in` is absent, so that tail remains unwritten.
    rif.diff_and_render();
    let _ = rif.take_output();
    for (col, ch) in "NEW".chars().enumerate() {
        rif.desired.set(0, col, ch, CellAttrs::default(), false);
    }

    assert_eq!(
        rif.plan_for_test(),
        vec![TermOp::WriteRun {
            row: 0,
            start: 0,
            end: 3,
        }],
        "first content on an erased row must preserve GNU's erased tail",
    );
}

#[test]
fn erased_row_gaining_content_up_to_its_last_column_writes_its_interior_gap() {
    let mut caps = TermCaps::default();
    caps.synchronized_output = false;
    let mut rif = TtyRif::new_with_caps(20, 1, caps);

    // Blank repaint first: every cell of the row is physically erased.
    rif.diff_and_render();
    let _ = rif.take_output();

    // A Magit log row: text, then the text-area gap, then right-margin
    // glyphs that reach the last column.  GNU's `!olen` branch trims only
    // TRAILING default-face blanks (`dispnew.c:6019-6022`); with none to
    // trim, `nlen` stays the full row and `write_glyphs (f, nbody + nsp,
    // nlen - nsp)` (`dispnew.c:6062-6079`) writes the interior gap too.
    for (col, ch) in "ab".chars().enumerate() {
        rif.desired.set(0, col, ch, CellAttrs::default(), false);
    }
    for (offset, ch) in "xyz".chars().enumerate() {
        rif.desired
            .set(0, 17 + offset, ch, CellAttrs::default(), false);
    }

    assert_eq!(
        rif.plan_for_test(),
        vec![TermOp::WriteRun {
            row: 0,
            start: 0,
            end: 20,
        }],
        "a blank row's interior gap is written when margin glyphs follow it",
    );
}

#[test]
fn changed_row_writes_the_erased_cells_inside_its_changed_span() {
    let mut caps = TermCaps::default();
    caps.synchronized_output = false;
    let mut rif = TtyRif::new_with_caps(20, 1, caps);

    // Leftover content keeps the row out of GNU's `!olen` path.
    for (col, ch) in "two".chars().enumerate() {
        rif.desired.set(0, col, ch, CellAttrs::default(), false);
    }
    rif.diff_and_render();
    let _ = rif.take_output();

    // GNU's insert/delete path writes ONE span, `write_glyphs (f, nbody +
    // nsp + begmatch, nlen - tem)` (`dispnew.c:6180-6186`), and never
    // re-diffs its interior.  Skipping an unchanged interior cell is
    // invisible only when that cell is already written; here the gap is
    // erased, so GNU's span leaves spaces where a skip leaves unwritten
    // cells.
    rif.desired = TtyGrid::new(20, 1);
    for (col, ch) in "ab".chars().enumerate() {
        rif.desired.set(0, col, ch, CellAttrs::default(), false);
    }
    for (offset, ch) in "xyz".chars().enumerate() {
        rif.desired
            .set(0, 17 + offset, ch, CellAttrs::default(), false);
    }

    assert_eq!(
        rif.plan_for_test(),
        vec![TermOp::WriteRun {
            row: 0,
            start: 0,
            end: 20,
        }],
        "erased cells inside a changed span are content GNU writes",
    );
}

#[test]
fn written_default_blanks_inside_a_changed_row_are_not_treated_as_erased() {
    let mut caps = TermCaps::default();
    caps.synchronized_output = false;
    let mut rif = TtyRif::new_with_caps(8, 1, caps);

    rif.desired.set(0, 0, 'A', CellAttrs::default(), false);
    rif.diff_and_render();
    let _ = rif.take_output();

    rif.desired = TtyGrid::new(8, 1);
    rif.desired.set(0, 0, 'A', CellAttrs::default(), false);
    for col in 1..4 {
        rif.desired.set(0, col, ' ', CellAttrs::default(), false);
    }
    rif.desired.set(0, 4, '|', CellAttrs::default(), false);
    rif.desired.set(0, 5, 'B', CellAttrs::default(), false);

    assert_eq!(
        rif.plan_for_test(),
        vec![TermOp::WriteRun {
            row: 0,
            start: 1,
            end: 6,
        }],
        "GNU's filled window-matrix gap is physical content, while only the row tail is erased",
    );
}

#[test]
fn relocated_content_keeps_its_unwritten_tail_when_repainted() {
    let mut caps = TermCaps::default();
    caps.synchronized_output = false;
    let mut rif = TtyRif::new_with_caps(8, 3, caps);
    for (col, ch) in "MOVED".chars().enumerate() {
        rif.desired.set(2, col, ch, CellAttrs::default(), false);
    }
    rif.diff_and_render();
    let _ = rif.take_output();

    rif.desired = TtyGrid::new(8, 3);
    for (col, ch) in "MOVED".chars().enumerate() {
        rif.desired.set(0, col, ch, CellAttrs::default(), false);
    }

    assert_eq!(
        rif.plan_for_test(),
        vec![
            TermOp::WriteRun {
                row: 0,
                start: 0,
                end: 5,
            },
            TermOp::EraseToEol {
                row: 2,
                from: 0,
                bg: None,
            },
        ],
        "relocated rows must retain GNU's unwritten tail instead of writing spaces through it",
    );
}

#[test]
fn copied_content_uses_the_same_erased_tail_rule() {
    let mut caps = TermCaps::default();
    caps.synchronized_output = false;
    let mut rif = TtyRif::new_with_caps(8, 3, caps);
    for (col, ch) in "COPY".chars().enumerate() {
        rif.desired.set(2, col, ch, CellAttrs::default(), false);
    }
    rif.diff_and_render();
    let _ = rif.take_output();

    rif.desired = TtyGrid::new(8, 3);
    for row in [0, 2] {
        for (col, ch) in "COPY".chars().enumerate() {
            rif.desired.set(row, col, ch, CellAttrs::default(), false);
        }
    }

    assert_eq!(
        rif.plan_for_test(),
        vec![TermOp::WriteRun {
            row: 0,
            start: 0,
            end: 4,
        }],
        "content provenance does not change GNU's erased-tail rule",
    );
}

#[test]
fn copied_content_in_two_destinations_uses_the_same_erased_tail_rule() {
    let mut caps = TermCaps::default();
    caps.synchronized_output = false;
    let mut rif = TtyRif::new_with_caps(8, 3, caps);
    for (col, ch) in "COPY".chars().enumerate() {
        rif.desired.set(2, col, ch, CellAttrs::default(), false);
    }
    rif.diff_and_render();
    let _ = rif.take_output();

    rif.desired = TtyGrid::new(8, 3);
    for row in [0, 1] {
        for (col, ch) in "COPY".chars().enumerate() {
            rif.desired.set(row, col, ch, CellAttrs::default(), false);
        }
    }

    assert_eq!(
        rif.plan_for_test(),
        vec![
            TermOp::WriteRun {
                row: 0,
                start: 0,
                end: 4,
            },
            TermOp::WriteRun {
                row: 1,
                start: 0,
                end: 4,
            },
            TermOp::EraseToEol {
                row: 2,
                from: 0,
                bg: None,
            },
        ],
        "all erased destinations use GNU's same content-only write path",
    );
}

#[test]
fn equal_length_changed_row_preserves_its_default_blank_tail() {
    let mut caps = TermCaps::default();
    caps.synchronized_output = false;
    let mut rif = TtyRif::new_with_caps(8, 1, caps);
    for (col, ch) in "OLD".chars().enumerate() {
        rif.desired.set(0, col, ch, CellAttrs::default(), false);
    }
    rif.diff_and_render();
    let _ = rif.take_output();

    rif.desired = TtyGrid::new(8, 1);
    for (col, ch) in "NEW".chars().enumerate() {
        rif.desired.set(0, col, ch, CellAttrs::default(), false);
    }

    assert_eq!(
        rif.plan_for_test(),
        vec![TermOp::WriteRun {
            row: 0,
            start: 0,
            end: 3,
        }],
        "GNU preserves the tail when trimmed `olen` and `nlen` are both three",
    );
}

#[test]
fn erase_to_eol_preserves_a_colored_foreground_space_at_the_boundary() {
    let mut caps = TermCaps::default();
    caps.synchronized_output = false;
    let mut rif = TtyRif::new_with_caps(8, 1, caps);
    for (col, ch) in "ABCDEFGH".chars().enumerate() {
        rif.desired.set(0, col, ch, CellAttrs::default(), false);
    }
    rif.diff_and_render();
    let _ = rif.take_output();

    rif.desired = TtyGrid::new(8, 1);
    rif.desired.set(0, 0, 'A', CellAttrs::default(), false);
    rif.desired.set(
        0,
        1,
        ' ',
        CellAttrs {
            fg: Some(TerminalColor::Indexed(208)),
            ..CellAttrs::default()
        },
        false,
    );
    let ops = rif.plan_for_test();

    assert_eq!(
        ops,
        vec![
            TermOp::WriteRun {
                row: 0,
                start: 1,
                end: 2,
            },
            TermOp::EraseToEol {
                row: 0,
                from: 2,
                bg: None,
            },
        ],
        "the styled boundary space must be written before only the default tail is erased",
    );
}

#[test]
fn diff_and_render_emits_hardware_cursor_shape_for_bar_cursor() {
    let mut rif = TtyRif::new(10, 5);
    rif.desired.set(0, 0, 'A', CellAttrs::default(), false);
    rif.cursor_visible = true;
    rif.cursor_row = 0;
    rif.cursor_col = 0;
    rif.cursor_shape = TerminalCursorShape::Bar;

    rif.diff_and_render();
    let output = String::from_utf8(rif.take_output()).expect("utf8 output");

    assert!(output.contains("\x1b[1;1H"));
    assert!(output.contains("\x1b[6 q"));
    assert!(output.contains("\x1b[?25h"));
}

#[test]
fn diff_with_changes_produces_ansi_sequences() {
    let mut rif = TtyRif::new(10, 5);
    // Write something into the desired grid.
    rif.desired.set(
        0,
        0,
        'A',
        CellAttrs {
            fg: Some(TerminalColor::Direct { r: 255, g: 0, b: 0 }),
            ..CellAttrs::default()
        },
        false,
    );
    rif.diff_and_render();
    let output = rif.take_output();
    let s = String::from_utf8_lossy(&output);

    // Should contain CUP to row 1, col 1 (1-based).
    assert!(s.contains("\x1b[1;1H"), "Missing CUP: {}", s);
    // Should contain the character 'A'.
    assert!(s.contains('A'), "Missing character A: {}", s);
    // Should contain true-color foreground sequence for red.
    assert!(s.contains("\x1b[38;2;255;0;0m"), "Missing fg color: {}", s);
}

#[test]
fn diff_and_render_emits_wide_glyphs_as_one_row_run() {
    let mut rif = TtyRif::new(10, 5);
    let attrs = CellAttrs::default();

    rif.desired.set(0, 0, '你', attrs, false);
    rif.desired.set(0, 1, ' ', attrs, true);
    rif.desired.set(0, 2, '好', attrs, false);
    rif.desired.set(0, 3, ' ', attrs, true);
    rif.desired.set(0, 4, ',', attrs, false);

    rif.diff_and_render();
    let output = String::from_utf8(rif.take_output()).expect("utf8 output");

    assert!(output.contains("\x1b[1;1H"));
    assert!(output.contains("你好,"));
    assert!(!output.contains("\x1b[1;3H"));
    assert!(!output.contains("\x1b[1;5H"));
}

#[test]
fn diff_and_render_rewrites_changed_row_span_contiguously() {
    let mut rif = TtyRif::new(10, 5);
    let attrs = CellAttrs::default();

    for (col, ch) in ['A', 'B', 'C', 'D', 'E'].into_iter().enumerate() {
        rif.desired.set(0, col, ch, attrs, false);
    }
    rif.diff_and_render();
    let _ = rif.take_output();

    rif.desired = rif.current.clone();
    rif.desired.set(0, 0, 'X', attrs, false);
    rif.desired.set(0, 4, 'Y', attrs, false);
    rif.diff_and_render();
    let output = String::from_utf8(rif.take_output()).expect("utf8 output");

    assert!(output.contains("\x1b[1;1H"));
    assert!(output.contains("XBCDY"));
    assert_eq!(
        output.matches("\x1b[1;").count(),
        1,
        "the equal-length rewrite is one contiguous GNU-style span"
    );
    assert!(
        !output.contains("\x1b[K"),
        "GNU does not erase the tail when `olen == nlen`: {output:?}"
    );
}

#[test]
fn equal_logical_row_lengths_do_not_request_a_tail_erase() {
    let mut caps = TermCaps::default();
    caps.insert_delete_char = false;
    caps.synchronized_output = false;
    let mut rif = TtyRif::new_with_caps(16, 1, caps);
    let attrs = CellAttrs::default();

    for (column, character) in "ABCDEFGHIJK□".chars().enumerate() {
        rif.desired.set(0, column, character, attrs, false);
    }
    let _ = render_output(&mut rif);

    rif.desired = rif.current.clone();
    for (column, character) in "abcdefghijk\u{10d24}".chars().enumerate() {
        rif.desired.set(0, column, character, attrs, false);
    }

    let ops = rif.plan_for_test();
    assert!(
        !ops.iter().any(|op| matches!(op, TermOp::EraseToEol { .. })),
        "GNU preserves the physical tail when trimmed old/new row lengths are equal: {ops:?}",
    );
}

#[test]
fn equal_length_composite_rewrite_does_not_preclear_the_row() {
    let mut rif = TtyRif::new(10, 5);
    let attrs = CellAttrs::default();

    rif.desired
        .set_cluster(0, 0, 'A', "\u{0301}\u{0302}", attrs, false);
    rif.desired.set(0, 1, 'B', attrs, false);
    rif.diff_and_render();
    let _ = rif.take_output();

    rif.desired = rif.current.clone();
    rif.desired.set_cluster(0, 0, 'A', "\u{0301}", attrs, false);
    rif.diff_and_render();
    let output = String::from_utf8(rif.take_output()).expect("utf8 output");

    assert!(
        output.contains("\x1b[1;1HA\u{0301}"),
        "the changed composite cell should be repainted in place: {output:?}"
    );
    assert!(
        !output.contains("\x1b[K"),
        "GNU does not erase a tail when old and new logical lengths are equal: {output:?}"
    );
}

#[test]
fn equal_length_composite_repaint_does_not_touch_its_unchanged_tail() {
    let mut caps = TermCaps::default();
    caps.synchronized_output = false;
    let mut rif = TtyRif::new_with_caps(16, 2, caps);
    let attrs = CellAttrs::default();

    rif.desired.set_cluster(0, 9, ' ', "ꦮ", attrs, false);
    rif.desired.set(0, 10, ')', attrs, false);
    rif.desired.set(0, 11, ';', attrs, false);
    let _ = render_output(&mut rif);

    rif.desired = rif.current.clone();
    rif.desired.set_cluster(0, 9, ' ', "ꦶ", attrs, false);
    let output = String::from_utf8(render_output(&mut rif)).expect("UTF-8 terminal output");

    // GNU rewrites only the changed equal-length span and neither clears nor
    // retransmits the unchanged suffix (`write_row`, dispnew.c:6019-6238).
    assert!(
        output.contains("\x1b[1;10H ꦶ"),
        "the replacement composite should be painted at its logical cell: {output:?}",
    );
    assert!(
        !output.contains(");") && !output.contains("\x1b[K"),
        "an equal-length rewrite must preserve both the logical and physical tail: {output:?}",
    );
    assert!(
        !output.contains("\r\n"),
        "a short logical row must not enter the right-margin state: {output:?}",
    );
}

#[test]
fn equal_length_composite_plan_is_independent_of_el_capability() {
    let mut caps = TermCaps::default();
    caps.blank_tail = BlankTailMethod::WriteSpaces;
    caps.synchronized_output = false;
    let mut rif = TtyRif::new_with_caps(16, 2, caps);
    let attrs = CellAttrs::default();

    rif.desired.set_cluster(0, 9, ' ', "ꦮ", attrs, false);
    rif.desired.set(0, 10, ')', attrs, false);
    rif.desired.set(0, 11, ';', attrs, false);
    let _ = render_output(&mut rif);

    rif.desired = rif.current.clone();
    rif.desired.set_cluster(0, 9, ' ', "ꦶ", attrs, false);
    assert_eq!(
        rif.plan_for_test(),
        vec![TermOp::WriteRun {
            row: 0,
            start: 9,
            end: 10,
        }],
        "GNU does not consult EL when the logical row did not shrink",
    );
}

#[test]
fn width_sensitive_row_rewrite_is_one_logical_terminal_run() {
    let mut caps = TermCaps::default();
    caps.insert_delete_char = false;
    caps.synchronized_output = false;
    let mut rif = TtyRif::new_with_caps(32, 1, caps);
    let attrs = CellAttrs::default();

    rif.desired.set_cluster(0, 0, 'A', "\u{0301}", attrs, false);
    for column in 1..=20 {
        rif.desired.set(0, column, 'x', attrs, false);
    }
    let _ = render_output(&mut rif);

    rif.desired = rif.current.clone();
    rif.desired.set_cluster(0, 0, 'A', "\u{0302}", attrs, false);
    rif.desired.set(0, 20, 'y', attrs, false);

    assert_eq!(
        rif.plan_for_test(),
        vec![TermOp::WriteRun {
            row: 0,
            start: 0,
            end: 21,
        }],
        "GNU keeps width-sensitive glyphs and their changed suffix in one cursor-contiguous run",
    );
}

#[test]
fn diff_swaps_current_and_desired() {
    let mut rif = TtyRif::new(10, 5);
    rif.desired.set(0, 0, 'X', CellAttrs::default(), false);
    rif.diff_and_render();

    // After diff, current should have 'X' at (0,0).
    assert_eq!(rif.current.cells[0].ch, 'X');
}

#[test]
fn second_diff_with_same_content_is_minimal() {
    let mut rif = TtyRif::new(10, 5);
    rif.desired.set(0, 0, 'Q', CellAttrs::default(), false);
    rif.diff_and_render();

    // Set the desired to the same content again.
    rif.desired.set(0, 0, 'Q', CellAttrs::default(), false);
    rif.diff_and_render();
    let output = rif.take_output();
    let s = String::from_utf8_lossy(&output);

    // Since desired == current, no cell CUP moves.
    // Only hide cursor + reset + possibly show cursor.
    let cup_count = s.matches("H").count();
    assert!(
        cup_count == 0,
        "Expected 0 CUP for identical frames, got {}",
        cup_count
    );
}

// ---------------------------------------------------------------------------
// Cursor visibility in output
// ---------------------------------------------------------------------------

#[test]
fn cursor_visible_emits_show_cursor_sequence() {
    let mut rif = TtyRif::new(10, 5);
    rif.cursor_visible = true;
    rif.cursor_row = 3;
    rif.cursor_col = 7;
    rif.diff_and_render();
    let output = rif.take_output();
    let s = String::from_utf8_lossy(&output);

    // Should show cursor.
    assert!(s.contains("\x1b[?25h"), "Missing show cursor: {}", s);
    // Should position cursor at (4, 8) (1-based).
    assert!(s.contains("\x1b[4;8H"), "Missing cursor position: {}", s);
}

#[test]
fn cursor_not_visible_omits_show_cursor_sequence() {
    let mut rif = TtyRif::new(10, 5);
    rif.cursor_visible = false;
    rif.diff_and_render();
    let output = rif.take_output();
    let s = String::from_utf8_lossy(&output);

    assert!(
        !s.contains("\x1b[?25h"),
        "Show cursor should not appear: {}",
        s
    );
}

// ---------------------------------------------------------------------------
// SGR sequences
// ---------------------------------------------------------------------------

#[test]
fn write_sgr_bold_italic_underline() {
    let attrs = CellAttrs {
        fg: Some(TerminalColor::Indexed(0)),
        bg: Some(TerminalColor::Indexed(15)),
        bold: true,
        italic: true,
        underline: 1,
        underline_color: None,
        strikethrough: false,
        inverse: false,
    };
    let mut buf = Vec::new();
    write_sgr(&mut buf, &attrs);
    let s = String::from_utf8_lossy(&buf);

    // No reset: GNU's `turn_on_face` has no such step, and the one this port
    // used to write is now `turn_off_face`'s `me`, emitted AFTER the run
    // (ledger 188).
    assert!(!s.contains("\x1b[0m"), "turn_on_face resets nothing: {s:?}");
    assert!(s.contains("\x1b[1m"), "Missing bold");
    assert!(s.contains("\x1b[3m"), "Missing italic");
    assert!(s.contains("\x1b[4m"), "Missing underline");
}

#[test]
fn write_sgr_underline_styles_match_gnu_smulx_codes() {
    let styles = [
        (UnderlineStyle::Line, "\x1b[4m"),
        (UnderlineStyle::Double, "\x1b[4:2m"),
        (UnderlineStyle::Wave, "\x1b[4:3m"),
        (UnderlineStyle::Dotted, "\x1b[4:4m"),
        (UnderlineStyle::Dashed, "\x1b[4:5m"),
    ];

    for (style, escape) in styles {
        let attrs = CellAttrs {
            underline: style.gnu_code(),
            ..CellAttrs::default()
        };
        let mut buf = Vec::new();
        write_sgr(&mut buf, &attrs);
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains(escape), "{style:?} did not emit {escape:?}");
    }
}

#[test]
fn write_sgr_strikethrough_inverse() {
    let attrs = CellAttrs {
        fg: Some(TerminalColor::Indexed(0)),
        bg: Some(TerminalColor::Indexed(0)),
        bold: false,
        italic: false,
        underline: 0,
        underline_color: None,
        strikethrough: true,
        inverse: true,
    };
    let mut buf = Vec::new();
    write_sgr(&mut buf, &attrs);
    let s = String::from_utf8_lossy(&buf);

    assert!(s.contains("\x1b[9m"), "Missing strikethrough");
    assert!(s.contains("\x1b[7m"), "Missing inverse");
}

#[test]
fn write_sgr_terminal_default_colors() {
    let attrs = CellAttrs {
        fg: None,
        bg: None,
        bold: false,
        italic: false,
        underline: 0,
        underline_color: None,
        strikethrough: false,
        inverse: false,
    };
    let mut buf = Vec::new();
    write_sgr(&mut buf, &attrs);
    let s = String::from_utf8_lossy(&buf);

    // GNU spells a terminal-default colour by emitting NOTHING for it
    // (`face_tty_specified_color`, src/term.c:2099); the `\E[39m` / `\E[49m`
    // this port used to write is GNU's `op`, and `op` belongs to
    // `turn_off_face`.
    assert_eq!(s, "", "no colour is specified, so none is emitted: {s:?}");
}

// ---------------------------------------------------------------------------
// TtyGrid
// ---------------------------------------------------------------------------

#[test]
fn grid_clear_sets_background() {
    let mut grid = TtyGrid::new(5, 3);
    grid.clear(Some(TerminalColor::Indexed(94)));
    for cell in &grid.cells {
        assert_eq!(cell.ch, ' ');
        assert_eq!(cell.attrs.bg, Some(TerminalColor::Indexed(94)));
    }
}

#[test]
fn grid_set_out_of_bounds_is_noop() {
    let mut grid = TtyGrid::new(5, 3);
    // Should not panic.
    grid.set(100, 100, 'X', CellAttrs::default(), false);
    // All cells still blank.
    for cell in &grid.cells {
        assert_eq!(cell.ch, ' ');
    }
}

// ---------------------------------------------------------------------------
// take_output
// ---------------------------------------------------------------------------

#[test]
fn take_output_clears_buffer() {
    let mut rif = TtyRif::new(10, 5);
    rif.desired.set(0, 0, 'A', CellAttrs::default(), false);
    rif.diff_and_render();

    let first = rif.take_output();
    assert!(!first.is_empty());

    let second = rif.take_output();
    assert!(second.is_empty());
}

// ---------------------------------------------------------------------------
// Full round-trip: rasterize + diff_and_render
// ---------------------------------------------------------------------------

#[test]
fn full_round_trip_simple_text() {
    let mut rif = TtyRif::new(10, 5);
    let state = make_simple_state("Hi");
    rif.rasterize(&state);
    rif.diff_and_render();
    let output = rif.take_output();
    let s = String::from_utf8_lossy(&output);

    // Should contain 'H' and 'i' somewhere in the output.
    assert!(s.contains('H'), "Missing H in output");
    assert!(s.contains('i'), "Missing i in output");
}

// ---------------------------------------------------------------------------
// Complex-run (Arabic/Indic) TTY decomposition
// ---------------------------------------------------------------------------

/// Build a one-row state whose text area is `glyphs`.
fn state_with_text_glyphs(cols: usize, glyphs: Vec<Glyph>) -> FrameDisplayState {
    let mut state = FrameDisplayState::new(cols, 5, 8.0, 16.0);
    state.background = Color::rgb(0.0, 0.0, 0.0);
    let mut matrix = GlyphMatrix::new(5, cols);
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.glyphs[GlyphArea::Text as usize] = glyphs;
    matrix.rows[0] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(row);
    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, cols as f32 * 8.0, 5.0 * 16.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, cols as f32 * 8.0, 5.0 * 16.0),
        text_clip_bounds: None,
        selected: true,
    });
    state
}

fn run_composite(text: &str, bidi_level: u8) -> Glyph {
    Glyph {
        glyph_type: GlyphType::Composite { text: text.into() },
        face_id: FaceId::new(0),
        box_vertical_edges: Default::default(),
        provenance: GlyphProvenance::buffer(0),
        bidi_level,
        wide: false,
        pixel_width: 0.0,
        pixel_height: 0.0,
        pixel_ascent: 0.0,
        vertical_offset_px: 0.0,
        padding: false,
        pointer_appearance: None,
    }
}

fn automatic_composite(text: &str, width_cols: u16) -> Glyph {
    let terminal = neovm_core::emacs_core::composite::automatic_composition_for_terminal(text);
    assert_eq!(terminal.width_cols, width_cols);
    Glyph {
        glyph_type: GlyphType::AutomaticComposite {
            text: text.into(),
            terminal,
        },
        face_id: FaceId::new(0),
        box_vertical_edges: Default::default(),
        provenance: GlyphProvenance::buffer(0),
        bidi_level: 0,
        wide: false,
        pixel_width: f32::from(width_cols),
        pixel_height: 0.0,
        pixel_ascent: 0.0,
        vertical_offset_px: 0.0,
        padding: false,
        pointer_appearance: None,
    }
}

#[test]
fn automatic_composite_uses_gnu_terminal_gstring_cells() {
    let glyphs = vec![
        // Both Javanese glyphs have GNU char-width 0.  Since neither follows a
        // positive-width glyph in the gstring, each is displayed as a mark on
        // a prepended space.
        automatic_composite("ꦮꦶ", 2),
        // A zero-width Rejang mark after a positive-width base shares its cell.
        automatic_composite("ꤸꥉ", 1),
        // A trailing format control is retained when it shares the preceding
        // positive-width base's cluster; GNU only replaces an orphan Cf.
        automatic_composite("ᠨ\u{180E}", 1),
    ];
    let mut rif = TtyRif::new(10, 5);
    rif.rasterize(&state_with_text_glyphs(10, glyphs));

    assert_eq!(desired_char(&rif, 0, 0), ' ');
    assert_eq!(rif.desired.cells[0].extenders.as_deref(), Some("ꦮ"));
    assert_eq!(desired_char(&rif, 0, 1), ' ');
    assert_eq!(rif.desired.cells[1].extenders.as_deref(), Some("ꦶ"));
    assert_eq!(desired_char(&rif, 0, 2), 'ꤸ');
    assert_eq!(rif.desired.cells[2].extenders.as_deref(), Some("ꥉ"));
    assert_eq!(desired_char(&rif, 0, 3), 'ᠨ');
    assert_eq!(rif.desired.cells[3].extenders.as_deref(), Some("\u{180E}"));
}

#[test]
fn rtl_automatic_composite_emits_terminal_cells_in_visual_order() {
    // GNU composite.c walks an automatic composition from its final cluster
    // in an odd-level bidi run, and term.c emits each selected cluster in
    // ascending gstring order.  Hanifi U+10D24 has GNU char-width 1, so it is
    // an independent cell and must move with the other cells, not remain
    // attached to its logical predecessor.
    let mut glyph = automatic_composite("𐴓𐴠𐴑𐴤𐴝", 5);
    glyph.bidi_level = 1;
    let mut rif = TtyRif::new(10, 5);

    rif.rasterize(&state_with_text_glyphs(10, vec![glyph]));

    let visual = (0..5)
        .map(|col| desired_char(&rif, 0, col))
        .collect::<String>();
    assert_eq!(visual, "𐴝𐴤𐴑𐴠𐴓");
    assert!(rif.desired.cells[..5].iter().all(|cell| {
        cell.extenders.is_none() && cell.terminal_advance == TerminalAdvance::TerminalResolved
    }));
}

fn run_member_padding(ch: char, charpos: usize) -> Glyph {
    let mut g = Glyph::char(ch, FaceId::new(0), charpos);
    g.padding = true;
    g
}

#[test]
fn rtl_run_decomposes_into_reversed_per_letter_cells() {
    // Arabic "اب" (alef, beh) as the GUI emits it: one Composite holding the
    // whole run plus one per-letter grapheme padding, flagged right-to-left.
    let glyphs = vec![
        run_composite("\u{0627}\u{0628}", 1),
        run_member_padding('\u{0628}', 1),
    ];
    let mut rif = TtyRif::new(10, 5);
    rif.rasterize(&state_with_text_glyphs(10, glyphs));
    // Visual order is reversed: beh then alef.
    assert_eq!(desired_char(&rif, 0, 0), '\u{0628}'); // beh
    assert_eq!(desired_char(&rif, 0, 1), '\u{0627}'); // alef
}

#[test]
fn ltr_run_decomposes_in_logical_order() {
    // Same structure but left-to-right (e.g. an Indic run): not reversed.
    let glyphs = vec![
        run_composite("\u{0627}\u{0628}", 0),
        run_member_padding('\u{0628}', 1),
    ];
    let mut rif = TtyRif::new(10, 5);
    rif.rasterize(&state_with_text_glyphs(10, glyphs));
    assert_eq!(desired_char(&rif, 0, 0), '\u{0627}'); // alef
    assert_eq!(desired_char(&rif, 0, 1), '\u{0628}'); // beh
}

#[test]
fn rtl_run_keeps_combining_mark_on_its_letter() {
    // Run "سّل": seen+shadda forms one grapheme, then lam. The shadda rides on
    // seen's padding cell; reversed visual order is lam, then seen+shadda.
    let glyphs = vec![
        run_composite("\u{0633}\u{0651}\u{0644}", 1),
        run_member_padding_cluster("\u{0633}\u{0651}", 1),
        run_member_padding('\u{0644}', 2),
    ];
    let mut rif = TtyRif::new(10, 5);
    rif.rasterize(&state_with_text_glyphs(10, glyphs));
    assert_eq!(desired_char(&rif, 0, 0), '\u{0644}'); // lam (leftmost)
    let seen_cell = &rif.desired.cells[1];
    assert_eq!(seen_cell.ch, '\u{0633}'); // seen base
    assert_eq!(seen_cell.extenders.as_deref(), Some("\u{0651}")); // shadda rides along
}

fn run_member_padding_cluster(text: &str, charpos: usize) -> Glyph {
    let mut g = Glyph {
        glyph_type: GlyphType::Composite { text: text.into() },
        face_id: FaceId::new(0),
        box_vertical_edges: Default::default(),
        provenance: GlyphProvenance::buffer(charpos),
        bidi_level: 1,
        wide: false,
        pixel_width: 0.0,
        pixel_height: 0.0,
        pixel_ascent: 0.0,
        vertical_offset_px: 0.0,
        padding: false,
        pointer_appearance: None,
    };
    g.padding = true;
    g
}

#[test]
fn plain_cluster_composite_stays_one_cell_and_skips_joiners() {
    // A ZWJ emoji family with no grapheme paddings stays a single cell; the
    // zero-width joiners are not drawn as their own characters.
    let glyphs = vec![run_composite("\u{1F468}\u{200D}\u{1F469}", 0)];
    let mut rif = TtyRif::new(10, 5);
    rif.rasterize(&state_with_text_glyphs(10, glyphs));
    assert_eq!(rif.desired.cells[0].ch, '\u{1F468}');
    // The ZWJ is dropped; extenders hold only the second emoji.
    assert_eq!(rif.desired.cells[0].extenders.as_deref(), Some("\u{1F469}"));
}

#[test]
fn glyphless_thin_space_projects_to_a_terminal_blank_cell() {
    // GNU term.c `produce_glyphless_glyph` cannot represent a one-pixel
    // graphical thin space and therefore emits one canonical space. The
    // source format character must never leak onto the terminal wire.
    let glyph = Glyph {
        glyph_type: GlyphType::Glyphless {
            ch: '\u{200C}',
            presentation: GlyphlessPresentation::ThinSpace,
        },
        face_id: FaceId::new(0),
        box_vertical_edges: Default::default(),
        provenance: GlyphProvenance::buffer(0),
        bidi_level: 0,
        wide: false,
        pixel_width: 0.25,
        pixel_height: 0.0,
        pixel_ascent: 0.0,
        vertical_offset_px: 0.0,
        padding: false,
        pointer_appearance: None,
    };
    let mut rif = TtyRif::new(10, 5);

    rif.rasterize(&state_with_text_glyphs(10, vec![glyph]));

    assert_eq!(rif.desired.cells[0].ch, ' ');
    assert_eq!(rif.desired.cells[0].extenders, None);
}

// --- The realized terminal colour is what the writer writes ----------------

/// The writer emits the number the realized face carries, spelled the way this
/// terminal's `setaf` spells it, and derives nothing.
///
/// GNU passes `face->foreground` straight to `tparam (ts, NULL, 0, fg, 0, 0, 0)`
/// (src/term.c:2098-2104), where `ts` is terminfo `setaf`.  Every xterm-family
/// entry spells it with the same three-way conditional, e.g.
///
/// ```text
/// screen-256color setaf =
///   \E[%?%p1%{8}%<%t3%p1%d%e%p1%{16}%<%t9%p1%{8}%-%d%e38;5;%p1%d%;m
/// ```
///
/// so index < 8 is `ESC [ 3N m`, 8..15 is `ESC [ 9(N-8) m`, and 16 and above is
/// `ESC [ 38;5;N m`.  `rxvt-16color` reaches the same bytes by a different
/// formula -- `setaf=\E[%?%p1%{8}%<%t%p1%{30}%+%e%p1%'R'%+%;%dm`, i.e. N+82 for
/// N >= 8, which is 90..97 exactly over 8..15.
#[test]
fn write_sgr_spells_the_index_the_realized_face_carries() {
    let expected = [
        (TerminalColor::Indexed(0), "\x1b[30m", "\x1b[40m"),
        (TerminalColor::Indexed(1), "\x1b[31m", "\x1b[41m"),
        (TerminalColor::Indexed(7), "\x1b[37m", "\x1b[47m"),
        (TerminalColor::Indexed(8), "\x1b[90m", "\x1b[100m"),
        (TerminalColor::Indexed(12), "\x1b[94m", "\x1b[104m"),
        (TerminalColor::Indexed(15), "\x1b[97m", "\x1b[107m"),
        (TerminalColor::Indexed(16), "\x1b[38;5;16m", "\x1b[48;5;16m"),
        (
            TerminalColor::Indexed(145),
            "\x1b[38;5;145m",
            "\x1b[48;5;145m",
        ),
        (
            TerminalColor::Indexed(255),
            "\x1b[38;5;255m",
            "\x1b[48;5;255m",
        ),
    ];
    for (color, fg, bg) in expected {
        let mut buf = Vec::new();
        write_sgr(
            &mut buf,
            &CellAttrs {
                fg: Some(color),
                ..CellAttrs::default()
            },
        );
        let s = String::from_utf8_lossy(&buf).into_owned();
        assert!(s.contains(fg), "{color:?} foreground: {s:?}");

        let mut buf = Vec::new();
        write_sgr(
            &mut buf,
            &CellAttrs {
                bg: Some(color),
                ..CellAttrs::default()
            },
        );
        let s = String::from_utf8_lossy(&buf).into_owned();
        assert!(s.contains(bg), "{color:?} background: {s:?}");
    }
}

/// A 24-bit colour is the OTHER reading of the same slot, not a fallback.
///
/// On a terminal reporting 16777216 cells, `tty-color-desc` answers
/// `tty-color-24bit`'s packed pixel in the INDEX position
/// (lisp/term/tty-colors.el:829-838), and GNU's `TF_rgb_separate` `setaf`
/// splits it into three channels (`fg >> 16`, `(fg >> 8) & 0xFF`, `fg & 0xFF`,
/// src/term.c:2103).  Measured out of GNU with `COLORTERM=truecolor`:
/// `(tty-color-desc "red")` => `("red" 13434880 52685 0 0)`, and 13434880 is
/// 0xCD0000.
#[test]
fn write_sgr_spells_a_direct_colour_as_three_channels() {
    let mut buf = Vec::new();
    write_sgr(
        &mut buf,
        &CellAttrs {
            fg: Some(TerminalColor::Direct { r: 205, g: 0, b: 0 }),
            bg: Some(TerminalColor::Direct {
                r: 0x12,
                g: 0x34,
                b: 0x56,
            }),
            ..CellAttrs::default()
        },
    );
    let s = String::from_utf8_lossy(&buf).into_owned();
    assert!(s.contains("\x1b[38;2;205;0;0m"), "{s:?}");
    assert!(s.contains("\x1b[48;2;18;52;86m"), "{s:?}");
    assert!(
        !s.contains("38;5;"),
        "a direct colour has no palette index: {s:?}"
    );
}

/// No realized terminal colour means no colour, never an invented one.
///
/// GNU seeds the slot with `FACE_TTY_DEFAULT_*_COLOR` and only assigns a pixel
/// when `tty-color-desc` answered (`map_tty_color`, src/xfaces.c:6620-6694);
/// `face_tty_specified_color` (src/dispextern.h:1933-1936) then keeps
/// `turn_on_face` from emitting anything for it.  This is the case a writer-side
/// quantizer used to paper over by approximating the RGB it happened to carry.
#[test]
fn a_face_with_no_realized_terminal_colour_emits_no_colour() {
    let mut buf = Vec::new();
    write_sgr(&mut buf, &CellAttrs::default());
    let s = String::from_utf8_lossy(&buf).into_owned();
    // NOTHING, not a `\E[39m\E[49m` pair: `face_tty_specified_color` failing
    // means `turn_on_face` never reaches an `OUTPUT` for that ground at all,
    // and the terminal is already at its default pair because `turn_off_face`
    // put it there with `op` (ledger 188).
    assert_eq!(s, "", "an all-default face turns nothing on: {s:?}");
}

/// A terminal with no colours gets no colour SGR at all -- GNU guards the whole
/// colour block with `if (tty->TN_max_colors > 0)` (src/term.c:2092).
#[test]
fn a_terminal_without_colors_emits_no_color_sgr() {
    let mut caps = TtyAttributeCapabilities::full();
    caps.colors = TtyColorSource::NoDatabase { max_colors: 0 };
    let mut buf = Vec::new();
    write_turn_on_face(
        &mut buf,
        &CellAttrs {
            fg: Some(TerminalColor::Indexed(9)),
            bg: Some(TerminalColor::Indexed(0)),
            ..CellAttrs::default()
        },
        &caps,
    );
    let s = String::from_utf8_lossy(&buf).into_owned();
    assert_eq!(s, "", "the whole colour block is skipped: {s:?}");
}

// ---------------------------------------------------------------------------
// GNU `turn_off_face` (ledger 188)
// ---------------------------------------------------------------------------

/// A stand-in for GNU's `tparam`, deliberately NOT a terminfo interpreter: it
/// substitutes the three `%pN%d` markers and nothing else.
///
/// The real expansion is ncurses' `tparm` and is pinned where ncurses is
/// linked -- `neomacs-bin`'s `a_colour_is_the_entrys_own_setaf_expanded_by_tparm`
/// runs it over `foot`, `linux-16color`, `qansi`, `xterm-direct` and
/// `xterm-kitty`.  What these tests pin is the other half: that the WRITER
/// reaches for the record's string at all, instead of spelling one itself.
fn stub_tparm(sequence: &[u8], parameters: TerminfoParameters) -> Option<Vec<u8>> {
    let (p1, p2, p3) = match parameters {
        TerminfoParameters::One(value) => (value, 0, 0),
        TerminfoParameters::Rgb { r, g, b } => (u32::from(r), u32::from(g), u32::from(b)),
    };
    let text = String::from_utf8(sequence.to_vec()).ok()?;
    Some(
        text.replace("%p1%d", &p1.to_string())
            .replace("%p2%d", &p2.to_string())
            .replace("%p3%d", &p3.to_string())
            .into_bytes(),
    )
}

const STUB_EXPANDER: TerminfoExpander = TerminfoExpander::new(stub_tparm);

/// TERM=linux's colour and reset capabilities, as `tgetstr` answers them.
fn linux_console_capabilities() -> TtyAttributeCapabilities {
    TtyAttributeCapabilities {
        // `tgetstr ("me")` on TERM=linux is `\E[m\017`, not the `\E[0m` this
        // port used to write, and not the `\E(B\E[m` that `infocmp` prints for
        // some other entries' `sgr0`: ncurses' termcap layer normalises the
        // one GNU reads (ledger 188, `tmp/pw188/mesweep.c`).
        exit_attribute_mode: Some(b"\x1b[m\x0f".to_vec()),
        exit_underline_mode: Some(b"\x1b[24m".to_vec()),
        colors: TtyColorSource::Entry(TtyColorCapabilities::new(
            b"\x1b[39;49m".to_vec(),
            Some(b"\x1b[3%p1%dm".to_vec()),
            Some(b"\x1b[4%p1%dm".to_vec()),
            false,
            TtyColorDepth::Indexed(8),
            STUB_EXPANDER,
        )),
        ..TtyAttributeCapabilities::full()
    }
}

/// GNU turns a face OFF after its run, with the terminal's own `me` and `op`,
/// and turns nothing off for a face that had nothing on.
///
/// Captured from GNU Emacs 31.0.90 in a pty on TERM=linux
/// (`tmp/pw188/gnu-linux-nocolorterm.raw`, `tmp/pw188/ptycap.py`):
///
/// ```text
///   ESC[31m PW188RED     ESC[39;49m ESC[K        <- colour only: `op`, no `me`
///   ESC[1m ESC[31m PW188BOLDRED ESC[m ^O ESC[39;49m ESC[K
/// ```
///
/// against this port's own pre-fix release binary on the same TERM
/// (`tmp/pw188/neo-before-linux.raw`):
///
/// ```text
///   ESC[2;1H ESC[0m ESC[31m ESC[49m PW188RED
///   ESC[5;1H ESC[0m ESC[1m ESC[31m ESC[49m PW188BOLDRED
/// ```
///
/// -- the reset in the wrong PLACE, spelled with the wrong BYTES, and a
/// `\E[39m`/`\E[49m` pair where GNU emits `op`.  Of the 927 entries this port
/// will start on, 460 spell `me` exactly `\E[0m`, 305 spell it `\E[m`, 50 emit
/// nothing at all (30 pure padding, 20 with no `me`), and 112 spell it other
/// bytes -- `linux` and ten variants among them.
#[test]
fn a_run_is_turned_off_with_the_terminals_own_me_and_op() {
    let caps = linux_console_capabilities();

    // Colour only: GNU emits `op` and no `me`, because none of
    // `tty_bold_p || tty_italic_p || tty_reverse_p || underline ||
    // tty_strike_through_p` is set (src/term.c:2140-2144).
    let red = CellAttrs {
        fg: Some(TerminalColor::Indexed(1)),
        ..CellAttrs::default()
    };
    let mut buf = Vec::new();
    write_turn_on_face(&mut buf, &red, &caps);
    write_turn_off_face(&mut buf, &red, &caps);
    assert_eq!(
        String::from_utf8(buf).unwrap(),
        "\x1b[31m\x1b[39;49m",
        "colour only: setaf on, `op` off, no `me`"
    );

    // Bold and colour: `me` first, then `op`, in GNU's order.
    let bold_red = CellAttrs { bold: true, ..red };
    let mut buf = Vec::new();
    write_turn_on_face(&mut buf, &bold_red, &caps);
    write_turn_off_face(&mut buf, &bold_red, &caps);
    assert_eq!(
        String::from_utf8(buf).unwrap(),
        "\x1b[1m\x1b[31m\x1b[m\x0f\x1b[39;49m",
        "bold: `me` is the entry's own \\E[m^O and it comes before `op`"
    );

    // Nothing on: nothing off.  This is the case the old writer spent
    // `\E[0m\E[39m\E[49m` on, on every run.
    let mut buf = Vec::new();
    write_turn_on_face(&mut buf, &CellAttrs::default(), &caps);
    write_turn_off_face(&mut buf, &CellAttrs::default(), &caps);
    assert!(
        buf.is_empty(),
        "a default face is all of turn_on and turn_off: {:?}",
        String::from_utf8_lossy(&buf)
    );

    // An appearance with no colour: `me` and no `op`.
    let underlined = CellAttrs {
        underline: 1,
        ..CellAttrs::default()
    };
    let mut buf = Vec::new();
    write_turn_off_face(&mut buf, &underlined, &caps);
    assert_eq!(String::from_utf8(buf).unwrap(), "\x1b[m\x0f");
}

/// GNU's `else` branch: a terminal with no `me` can only undo the ONE
/// appearance that has its own exit sequence.
///
/// ```c
///   else
///     {
///       /* If we don't have "me" we can only have those appearances
///          that have exit sequences defined.  */
///       if (face->underline)
///         OUTPUT_IF (tty, tty->TS_exit_underline_mode);
///     }
/// ```
///
/// (src/term.c:2151-2157.)  Twenty of the 927 reachable entries have no `me` at
/// all -- `ansi-mini`, `ansi-mtabs`, `ansi77`, `vt100-nav`, `dg210`, `luna`,
/// `masscomp` and the rest -- and thirty more have a `me` that is pure padding,
/// for which `tputs` emits a DELAY and no bytes.
#[test]
fn a_terminal_without_me_undoes_only_the_underline_like_gnu() {
    let no_me = TtyAttributeCapabilities {
        exit_attribute_mode: None,
        exit_underline_mode: Some(b"\x1b[24m".to_vec()),
        colors: TtyColorSource::Absent,
        ..TtyAttributeCapabilities::full()
    };

    let mut buf = Vec::new();
    write_turn_off_face(
        &mut buf,
        &CellAttrs {
            underline: 1,
            ..CellAttrs::default()
        },
        &no_me,
    );
    assert_eq!(String::from_utf8(buf).unwrap(), "\x1b[24m");

    // Bold is an appearance GNU cannot undo without `me`, so it emits nothing
    // -- not a `\E[0m` invented for the occasion.
    let mut buf = Vec::new();
    write_turn_off_face(
        &mut buf,
        &CellAttrs {
            bold: true,
            ..CellAttrs::default()
        },
        &no_me,
    );
    assert!(buf.is_empty(), "{:?}", String::from_utf8_lossy(&buf));

    // And the two branches are exclusive: a terminal that HAS `me` never emits
    // `ue`, however underlined the face is.
    let with_me = TtyAttributeCapabilities {
        exit_attribute_mode: Some(b"\x1b[m\x0f".to_vec()),
        exit_underline_mode: Some(b"\x1b[24m".to_vec()),
        colors: TtyColorSource::Absent,
        ..TtyAttributeCapabilities::full()
    };
    let mut buf = Vec::new();
    write_turn_off_face(
        &mut buf,
        &CellAttrs {
            underline: 1,
            ..CellAttrs::default()
        },
        &with_me,
    );
    assert_eq!(String::from_utf8(buf).unwrap(), "\x1b[m\x0f");
}

/// The writer emits the terminal's own `setaf`/`setab` rather than its own
/// rule, and takes GNU's `TF_rgb_separate` branch when the entry has one.
///
/// The parameter substitution here is [`stub_tparm`]; the real ncurses
/// expansion of real entries is pinned in `neomacs-bin`.  What this asserts is
/// that the bytes came from the RECORD -- the five ANSI spellings this port
/// used to write appear nowhere.
#[test]
fn a_colour_is_written_with_the_records_own_setaf() {
    let caps = TtyAttributeCapabilities {
        colors: TtyColorSource::Entry(TtyColorCapabilities::new(
            b"\x1b[39;49m".to_vec(),
            // `foot`'s spelling, colon-separated, for the 256 range.
            Some(b"\x1b[38:5:%p1%dm".to_vec()),
            Some(b"\x1b[48:5:%p1%dm".to_vec()),
            false,
            TtyColorDepth::Indexed(256),
            STUB_EXPANDER,
        )),
        ..TtyAttributeCapabilities::full()
    };
    let mut buf = Vec::new();
    write_turn_on_face(
        &mut buf,
        &CellAttrs {
            fg: Some(TerminalColor::Indexed(100)),
            bg: Some(TerminalColor::Indexed(7)),
            ..CellAttrs::default()
        },
        &caps,
    );
    let out = String::from_utf8(buf).unwrap();
    assert_eq!(out, "\x1b[38:5:100m\x1b[48:5:7m", "{out:?}");
    assert!(
        !out.contains("\x1b[38;5;") && !out.contains("\x1b[37m"),
        "the rule this port used to apply must not appear: {out:?}"
    );

    // GNU's `TF_rgb_separate` branch: three parameters, not one
    // (src/term.c:2101).  `xterm-kitty`'s `setrgbf` is the reachable entry
    // that has it.
    let rgb_separate = TtyAttributeCapabilities {
        colors: TtyColorSource::Entry(TtyColorCapabilities::new(
            b"\x1b[39;49m".to_vec(),
            Some(b"\x1b[38:2:%p1%d:%p2%d:%p3%dm".to_vec()),
            Some(b"\x1b[48:2:%p1%d:%p2%d:%p3%dm".to_vec()),
            true,
            TtyColorDepth::Direct(TtyDirectColorRoute::Setrgbf),
            STUB_EXPANDER,
        )),
        ..TtyAttributeCapabilities::full()
    };
    let mut buf = Vec::new();
    write_turn_on_face(
        &mut buf,
        &CellAttrs {
            fg: Some(TerminalColor::Direct {
                r: 205,
                g: 0,
                b: 17,
            }),
            ..CellAttrs::default()
        },
        &rgb_separate,
    );
    let out = String::from_utf8(buf).unwrap();
    assert_eq!(out, "\x1b[38:2:205:0:17m", "{out:?}");
    assert!(
        !out.contains("38;2;"),
        "the semicolon literal this port used to write must not appear: {out:?}"
    );

    // And without `TF_rgb_separate` the SAME realized colour goes through as
    // ONE packed parameter -- which is what the 20 reachable `RGB` entries
    // (`xterm-direct`, `tmux-direct`, `alacritty-direct`, ...) do.
    let packed = TtyAttributeCapabilities {
        colors: TtyColorSource::Entry(TtyColorCapabilities::new(
            b"\x1b[39;49m".to_vec(),
            Some(b"\x1b[38;PACKED=%p1%dm".to_vec()),
            Some(b"\x1b[48;PACKED=%p1%dm".to_vec()),
            false,
            TtyColorDepth::Direct(TtyDirectColorRoute::RgbFlag),
            STUB_EXPANDER,
        )),
        ..TtyAttributeCapabilities::full()
    };
    let mut buf = Vec::new();
    write_turn_on_face(
        &mut buf,
        &CellAttrs {
            fg: Some(TerminalColor::Direct {
                r: 205,
                g: 0,
                b: 17,
            }),
            ..CellAttrs::default()
        },
        &packed,
    );
    assert_eq!(
        String::from_utf8(buf).unwrap(),
        format!("\x1b[38;PACKED={}m", (205 << 16) | 17)
    );
}

/// A terminal with a colour count but no `op` has NO colour in GNU: `init_tty`
/// reads `AF`, `AB` and `Co` inside `if (tty->TS_orig_pair)` and leaves
/// `TN_max_colors` at zero otherwise, "because we can't switch back to the
/// default foreground and background" (src/term.c:4602-4606).
///
/// Three reachable entries are in that state -- `amiga-vnc`, `djgpp204`,
/// `vwmterm` -- and the fallback rule must not paint them.
#[test]
fn an_entry_without_op_gets_no_colour_from_the_writer() {
    // `Absent` is now the whole answer, count included: ledger 193 made
    // `TN_max_colors` a question about `colors` rather than a field beside
    // it, so "a colourless entry still claiming cells" -- which is the state
    // `amiga-vnc` used to reach the writer in -- is no longer representable.
    // The discrimination the old `color_cells: 8` provided comes from the
    // NEGATIVE CONTROL below instead: `NoDatabase` carries the same count and
    // MUST paint, so an assertion that passed for every record would fail
    // there.
    let no_op = TtyAttributeCapabilities {
        colors: TtyColorSource::Absent,
        ..TtyAttributeCapabilities::full()
    };
    let mut buf = Vec::new();
    write_turn_on_face(
        &mut buf,
        &CellAttrs {
            fg: Some(TerminalColor::Indexed(1)),
            bg: Some(TerminalColor::Indexed(4)),
            ..CellAttrs::default()
        },
        &no_op,
    );
    assert!(buf.is_empty(), "{:?}", String::from_utf8_lossy(&buf));

    let mut buf = Vec::new();
    write_turn_off_face(
        &mut buf,
        &CellAttrs {
            fg: Some(TerminalColor::Indexed(1)),
            ..CellAttrs::default()
        },
        &no_op,
    );
    assert!(
        buf.is_empty(),
        "no `op` to emit either: {:?}",
        String::from_utf8_lossy(&buf)
    );
}

// ---------------------------------------------------------------------------
// Terminal attribute capabilities (GNU term.c `turn_on_face` / `tty_capable_p`)
// ---------------------------------------------------------------------------

#[test]
fn italic_falls_back_to_dim_when_the_terminal_has_no_sitm() {
    // GNU term.c turn_on_face:
    //     if (tty->TS_enter_italic_mode) OUTPUT1 (tty, TS_enter_italic_mode);
    //     else  /* Italics not supported, use dim instead. */
    //           OUTPUT1 (tty, tty->TS_enter_dim_mode);
    // TERM=screen has no `sitm', so GNU renders `:slant italic' as SGR 2 there --
    // which is exactly what a GNU-vs-neomacs tty diff showed (GNU `^[[2m' vs
    // neomacs `^[[3m').
    let attrs = CellAttrs {
        italic: true,
        ..CellAttrs::default()
    };

    let with_italics = TtyAttributeCapabilities {
        italic_sequence: Some(b"\x1b[3m".to_vec()),
        dim_sequence: Some(b"\x1b[2m".to_vec()),
        ..TtyAttributeCapabilities::full()
    };
    let mut buf = Vec::new();
    write_turn_on_face(&mut buf, &attrs, &with_italics);
    let s = String::from_utf8(buf).unwrap();
    assert!(s.contains("\x1b[3m"), "sitm present -> italic: {s:?}");
    assert!(
        !s.contains("\x1b[2m"),
        "must not dim when italic works: {s:?}"
    );

    let no_italics = TtyAttributeCapabilities {
        italic_sequence: None,
        dim_sequence: Some(b"\x1b[2m".to_vec()),
        ..TtyAttributeCapabilities::full()
    };
    let mut buf = Vec::new();
    write_turn_on_face(&mut buf, &attrs, &no_italics);
    let s = String::from_utf8(buf).unwrap();
    assert!(s.contains("\x1b[2m"), "no sitm -> dim fallback: {s:?}");
    assert!(!s.contains("\x1b[3m"), "no sitm -> no italic escape: {s:?}");

    // Neither capability: GNU emits nothing for the slant.
    let neither = TtyAttributeCapabilities {
        italic_sequence: None,
        dim_sequence: None,
        ..TtyAttributeCapabilities::full()
    };
    let mut buf = Vec::new();
    write_turn_on_face(&mut buf, &attrs, &neither);
    let s = String::from_utf8(buf).unwrap();
    assert!(!s.contains("\x1b[3m") && !s.contains("\x1b[2m"), "{s:?}");
}

#[test]
fn inverse_face_uses_the_terminals_physical_standout_rendition() {
    let attrs = CellAttrs {
        inverse: true,
        ..CellAttrs::default()
    };
    let screen_like = TtyAttributeCapabilities {
        standout_sequence: Some(b"\x1b[3m".to_vec()),
        ..TtyAttributeCapabilities::full()
    };
    let mut output = Vec::new();

    write_turn_on_face(&mut output, &attrs, &screen_like);

    let output = String::from_utf8(output).expect("UTF-8 terminal output");
    assert!(
        output.contains("\x1b[3m"),
        "screen standout is SGR 3: {output:?}"
    );
    assert!(
        !output.contains("\x1b[7m"),
        "must not replace screen's standout with hard-coded inverse: {output:?}",
    );
}

#[test]
fn inverse_face_emits_the_complete_standout_capability() {
    let attrs = CellAttrs {
        inverse: true,
        ..CellAttrs::default()
    };
    let caps = TtyAttributeCapabilities {
        standout_sequence: Some(b"\x1b[0;1;3m".to_vec()),
        ..TtyAttributeCapabilities::full()
    };
    let mut output = Vec::new();

    write_turn_on_face(&mut output, &attrs, &caps);

    assert!(
        output
            .windows(b"\x1b[0;1;3m".len())
            .any(|bytes| bytes == b"\x1b[0;1;3m"),
        "standout must be emitted byte-for-byte after terminfo padding is removed: {:?}",
        String::from_utf8_lossy(&output),
    );
}

#[test]
fn attributes_the_terminal_lacks_are_not_emitted() {
    // Every arm of GNU turn_on_face is gated on its capability string.
    let attrs = CellAttrs {
        bold: true,
        underline: UnderlineStyle::Line.gnu_code(),
        strikethrough: true,
        inverse: true,
        ..CellAttrs::default()
    };
    let none = TtyAttributeCapabilities::none();
    let mut buf = Vec::new();
    write_turn_on_face(&mut buf, &attrs, &none);
    let s = String::from_utf8(buf).unwrap();
    for escape in ["\x1b[1m", "\x1b[4m", "\x1b[9m", "\x1b[7m"] {
        assert!(
            !s.contains(escape),
            "{escape:?} emitted without support: {s:?}"
        );
    }

    let mut buf = Vec::new();
    write_turn_on_face(&mut buf, &attrs, &TtyAttributeCapabilities::full());
    let s = String::from_utf8(buf).unwrap();
    for escape in ["\x1b[1m", "\x1b[4m", "\x1b[9m", "\x1b[7m"] {
        assert!(
            s.contains(escape),
            "{escape:?} missing when supported: {s:?}"
        );
    }
}

/// GNU `turn_on_face` emits the capability's OWN string --
/// `OUTPUT1_IF (tty, tty->TS_enter_bold_mode)` and its five neighbours
/// (src/term.c:2061-2090) -- never an ANSI literal of its own.
///
/// This port spelled all six itself while asking the capability record only
/// whether the terminal HAD them, and the two answers disagree on the database
/// ncurses ships.  Measured with `infocmp -x` over all 1,862 unique entries
/// (ledger 186, `tmp/pw186/cap_audit.tsv`): 448 of the 1,303 entries that have
/// `us` spell it something other than `ESC [ 4 m`, 234 of 996 spell `md`
/// something other than `ESC [ 1 m`, and 281 of 616 spell `mh` something other
/// than `ESC [ 2 m`.  Restricted to the entries this port would even start on
/// -- `check_terminal_powerful_enough` refuses a TERM whose `cm` is not the
/// ANSI form -- 138 of 927 disagree about at least one, `xterm-bold`
/// (`smul=\E[1m`), `xterm-pcolor` (`smul=\E[4;42m`, `bold=\E[1;43m`) and
/// `putty-m1b`/`putty-m2` (`bold=\E[33m`) among them.
#[test]
fn the_writer_emits_the_terminals_own_rendition_strings() {
    let attrs = CellAttrs {
        bold: true,
        italic: true,
        underline: UnderlineStyle::Line.gnu_code(),
        strikethrough: true,
        inverse: true,
        ..CellAttrs::default()
    };
    // `xterm-pcolor`'s and `putty-m1b`'s real spellings, plus the Wyse ones
    // `wy350` uses, on one record.
    let own = TtyAttributeCapabilities {
        bold_sequence: Some(b"\x1b[1;43m".to_vec()),
        italic_sequence: Some(b"\x1b[3;44m".to_vec()),
        underline_sequence: Some(b"\x1bG8".to_vec()),
        strike_through_sequence: Some(b"\x1bG@".to_vec()),
        standout_sequence: Some(b"\x1b[7;31m".to_vec()),
        ..TtyAttributeCapabilities::full()
    };
    let mut buf = Vec::new();
    write_turn_on_face(&mut buf, &attrs, &own);
    let out = String::from_utf8(buf).unwrap();

    for sequence in ["\x1b[1;43m", "\x1b[3;44m", "\x1bG8", "\x1bG@", "\x1b[7;31m"] {
        assert!(
            out.contains(sequence),
            "the entry's own {sequence:?} must be emitted: {out:?}"
        );
    }
    // and not the ANSI spelling this port used to write instead
    for invented in ["\x1b[1m", "\x1b[3m", "\x1b[4m", "\x1b[9m", "\x1b[7m"] {
        assert!(
            !out.contains(invented),
            "{invented:?} is this port's invention, not the terminal's: {out:?}"
        );
    }
}

/// GNU's styled underline is `Smulx` run through `tparam`
/// (`tparam (tty->TF_set_underline_style, NULL, 0, face->underline, 0, 0, 0)`,
/// src/term.c:2083), so a terminal whose `Smulx` is not the kitty spelling
/// gets its own sequence.  This port emitted a fixed `ESC [ 4 : N m`.
///
/// Every `Smulx` ncurses ships IS the kitty spelling -- 25 unique entries, 0
/// of them different (ledger 186) -- so the only way to see the difference is
/// a terminfo entry built for the purpose; `tmp/pw186/ti/pw186.src` is that
/// entry and `tmp/pw186/smulx_probe.c` is ncurses answering
/// `tparm ("\E[4;%p1%dm", 3)` = `\E[4;3m` for it.  Here the same four
/// expansions are handed to the record directly, because the expander lives in
/// the crate that links ncurses.
#[test]
fn a_styled_underline_is_the_terminals_own_smulx_expansion() {
    let semicolon = TtyAttributeCapabilities {
        styled_underline: TtyStyledUnderline::expand_all(|style| {
            Some(format!("\x1b[4;{style}m").into_bytes())
        }),
        ..TtyAttributeCapabilities::full()
    };
    for (style, expected) in [
        (UnderlineStyle::Double, "\x1b[4;2m"),
        (UnderlineStyle::Wave, "\x1b[4;3m"),
        (UnderlineStyle::Dotted, "\x1b[4;4m"),
        (UnderlineStyle::Dashed, "\x1b[4;5m"),
    ] {
        let attrs = CellAttrs {
            underline: style.gnu_code(),
            ..CellAttrs::default()
        };
        let mut buf = Vec::new();
        write_turn_on_face(&mut buf, &attrs, &semicolon);
        let out = String::from_utf8(buf).unwrap();
        assert!(
            out.contains(expected),
            "{style:?} must use the entry's own Smulx: {out:?}"
        );
        assert!(
            !out.contains("\x1b[4:"),
            "the kitty spelling is not this terminal's: {out:?}"
        );
    }

    // `FACE_UNDERLINE_SINGLE` never reaches `Smulx` in GNU, however the entry
    // spells it: it takes the `smul` arm above (src/term.c:2076-2078).
    let attrs = CellAttrs {
        underline: UnderlineStyle::Line.gnu_code(),
        ..CellAttrs::default()
    };
    let mut buf = Vec::new();
    write_turn_on_face(&mut buf, &attrs, &semicolon);
    let out = String::from_utf8(buf).unwrap();
    assert!(
        out.contains("\x1b[4m"),
        "single underline is `smul': {out:?}"
    );
    assert!(!out.contains("\x1b[4;1m"), "not a parameter: {out:?}");
}

/// GNU checks `MAY_USE_WITH_COLORS_P (tty, NC_ITALIC)` ONCE, around the whole
/// slant arm, and then emits `TS_enter_dim_mode` with `OUTPUT1` -- no second
/// `ncv` test (src/term.c:2063-2072).  This port asked `supports(Dim)` for the
/// fallback, which carries the `NC_DIM` term, so a terminal whose `ncv`
/// forbids dim on a colour frame got NOTHING for an italic face where GNU
/// dims it.
///
/// TERM=linux is exactly that terminal: `ncv#18` is `NC_UNDERLINE|NC_DIM`,
/// there is no `sitm`, and `dim=\E[2m`.  Captured from GNU Emacs 31.0.90 in a
/// pty (`tmp/pw186/gnu-linux-italic.raw`, `tmp/pw186/italic_capture.sh`):
///
/// ```text
///   ^[[2mPW186ITALIC^[[m^O
/// ```
///
/// 58 of the 927 entries this port would start on are in that state, `linux`
/// and its ten variants, `cons25`, `teken`, `screen.gnome` and `wy370` among
/// them (ledger 186).
#[test]
fn the_dim_fallback_for_italic_ignores_the_ncv_dim_bit_like_gnu() {
    let attrs = CellAttrs {
        italic: true,
        ..CellAttrs::default()
    };
    let linux_console = TtyAttributeCapabilities {
        italic_sequence: None,
        dim_sequence: Some(b"\x1b[2m".to_vec()),
        colors: TtyColorSource::NoDatabase { max_colors: 8 },
        no_color_video: TtyNoColorVideo(18),
        ..TtyAttributeCapabilities::full()
    };

    assert_eq!(
        linux_console.italic_rendition(),
        TtyItalicRendition::Dim(b"\x1b[2m"),
        "GNU's fallback has no NC_DIM term"
    );
    let mut buf = Vec::new();
    write_turn_on_face(&mut buf, &attrs, &linux_console);
    let out = String::from_utf8(buf).unwrap();
    assert!(out.contains("\x1b[2m"), "GNU writes ESC[2m here: {out:?}");

    // The gate GNU DOES have is on the italic bit, and it silences the arm
    // whole -- fallback included.
    let ncv_italic = TtyAttributeCapabilities {
        no_color_video: TtyNoColorVideo::ITALIC,
        colors: TtyColorSource::NoDatabase { max_colors: 8 },
        italic_sequence: None,
        dim_sequence: Some(b"\x1b[2m".to_vec()),
        ..TtyAttributeCapabilities::full()
    };
    assert_eq!(ncv_italic.italic_rendition(), TtyItalicRendition::None);
    let mut buf = Vec::new();
    write_turn_on_face(&mut buf, &attrs, &ncv_italic);
    assert!(!String::from_utf8(buf).unwrap().contains("\x1b[2m"));

    // `tty_capable_p` is a different question and keeps its NC_DIM term:
    // GNU's `TTY_CAP_DIM` arm really does test `MAY_USE_WITH_COLORS_P`
    // (src/term.c:2194-2195).
    assert!(!linux_console.supports(TtyCapability::Dim));
}

#[test]
fn a_styled_underline_degrades_to_a_plain_one_without_smulx() {
    // GNU turn_on_face: the styled form is used only `if (tty->TF_set_underline_style)',
    // otherwise the plain `smul' sequence stands in.
    let attrs = CellAttrs {
        underline: UnderlineStyle::Wave.gnu_code(),
        ..CellAttrs::default()
    };
    let no_smulx = TtyAttributeCapabilities {
        styled_underline: None,
        ..TtyAttributeCapabilities::full()
    };
    let mut buf = Vec::new();
    write_turn_on_face(&mut buf, &attrs, &no_smulx);
    let s = String::from_utf8(buf).unwrap();
    assert!(s.contains("\x1b[4m"), "plain underline fallback: {s:?}");
    assert!(
        !s.contains("4:3"),
        "no styled underline without Smulx: {s:?}"
    );
}

#[test]
fn color_capable_terminals_honor_the_no_color_video_mask() {
    // GNU MAY_USE_WITH_COLORS_P (term.c): when the terminal has colors, an
    // attribute listed in terminfo `ncv' cannot be combined with them, so
    // turn_on_face skips it entirely.
    let attrs = CellAttrs {
        bold: true,
        underline: UnderlineStyle::Line.gnu_code(),
        ..CellAttrs::default()
    };
    let ncv_bold = TtyAttributeCapabilities {
        colors: TtyColorSource::NoDatabase { max_colors: 256 },
        no_color_video: TtyNoColorVideo::BOLD,
        ..TtyAttributeCapabilities::full()
    };
    let mut buf = Vec::new();
    write_turn_on_face(&mut buf, &attrs, &ncv_bold);
    let s = String::from_utf8(buf).unwrap();
    assert!(!s.contains("\x1b[1m"), "ncv bold must suppress bold: {s:?}");
    assert!(s.contains("\x1b[4m"), "underline is unaffected: {s:?}");

    // A monochrome terminal ignores ncv (GNU: `TN_max_colors > 0 ? … : 1').
    let mono = TtyAttributeCapabilities {
        colors: TtyColorSource::NoDatabase { max_colors: 0 },
        no_color_video: TtyNoColorVideo::BOLD,
        ..TtyAttributeCapabilities::full()
    };
    let mut buf = Vec::new();
    write_turn_on_face(&mut buf, &attrs, &mono);
    let s = String::from_utf8(buf).unwrap();
    assert!(s.contains("\x1b[1m"), "monochrome ignores ncv: {s:?}");
}

#[test]
fn tty_capable_p_matches_gnu_capability_and_ncv_logic() {
    // GNU tty_capable_p: every requested capability needs its terminfo string
    // AND (when the terminal has colors) an `ncv' bit that is clear.
    let full = TtyAttributeCapabilities::full();
    assert!(full.supports(TtyCapability::Bold));
    assert!(full.supports(TtyCapability::Italic));
    assert!(full.supports(TtyCapability::UnderlineStyled));

    let screen_like = TtyAttributeCapabilities {
        italic_sequence: None,
        ..TtyAttributeCapabilities::full()
    };
    assert!(!screen_like.supports(TtyCapability::Italic));
    assert!(screen_like.supports(TtyCapability::Underline));
    assert!(screen_like.supports(TtyCapability::Bold));

    let ncv_underline = TtyAttributeCapabilities {
        colors: TtyColorSource::NoDatabase { max_colors: 8 },
        no_color_video: TtyNoColorVideo::UNDERLINE,
        ..TtyAttributeCapabilities::full()
    };
    assert!(!ncv_underline.supports(TtyCapability::Underline));
    assert!(ncv_underline.supports(TtyCapability::Bold));
}

// ---------------------------------------------------------------------------
// Scroll detection + synchronized output (issue #206)
// ---------------------------------------------------------------------------

/// Write `text` into desired row `row`, one char per cell.
fn set_row(rif: &mut TtyRif, row: usize, text: &str) {
    for (col, ch) in text.chars().enumerate() {
        rif.desired.set(row, col, ch, CellAttrs::default(), false);
    }
}

fn render_output(rif: &mut TtyRif) -> Vec<u8> {
    rif.diff_and_render();
    rif.take_output()
}

#[test]
fn scrolled_rows_emit_region_scroll_not_row_rewrites() {
    let mut rif = TtyRif::new(20, 10);
    for r in 0..10 {
        set_row(&mut rif, r, &format!("line-number-{r:02}"));
    }
    let _ = render_output(&mut rif); // establish the screen

    // Scroll down by one: rows shift up, one new line appears at the bottom.
    for r in 0..9 {
        set_row(&mut rif, r, &format!("line-number-{:02}", r + 1));
    }
    set_row(&mut rif, 9, "line-number-10");
    let out = render_output(&mut rif);
    let text = String::from_utf8_lossy(&out);

    assert!(
        text.contains("\x1b[1S"),
        "one-line shift must emit a scroll-up, got: {text:?}"
    );
    assert!(
        text.contains(";10r") || text.contains("[1;10r"),
        "scroll must be bounded by a DECSTBM region, got: {text:?}"
    );
    // The shifted rows must NOT be retransmitted: their text moved, the
    // terminal moved it, so e.g. row content 'line-number-05' appears
    // nowhere in the output.
    assert!(
        !text.contains("line-number-05"),
        "shifted row content was retransmitted: {text:?}"
    );
    // The newly exposed bottom line IS transmitted.
    assert!(
        text.contains("line-number-10"),
        "exposed row must be drawn: {text:?}"
    );
}

#[test]
fn scroll_model_matches_terminal_after_region_scroll() {
    // After the scroll path runs, the internal `current` grid must agree
    // with what the terminal shows: a second render with unchanged desired
    // content emits no further row writes.
    let mut rif = TtyRif::new(20, 10);
    for r in 0..10 {
        set_row(&mut rif, r, &format!("stable-content-{r:02}"));
    }
    let _ = render_output(&mut rif);
    for r in 0..9 {
        set_row(&mut rif, r, &format!("stable-content-{:02}", r + 1));
    }
    set_row(&mut rif, 9, "stable-content-10");
    let _ = render_output(&mut rif);

    // Re-render the same content.
    for r in 0..9 {
        set_row(&mut rif, r, &format!("stable-content-{:02}", r + 1));
    }
    set_row(&mut rif, 9, "stable-content-10");
    let out = render_output(&mut rif);
    let text = String::from_utf8_lossy(&out);
    assert!(
        !text.contains("stable-content"),
        "steady state after a scroll must be a no-op, got: {text:?}"
    );
}

#[test]
fn small_changes_do_not_trigger_scroll() {
    let mut rif = TtyRif::new(20, 10);
    for r in 0..10 {
        set_row(&mut rif, r, &format!("plain-old-row-{r:02}"));
    }
    let _ = render_output(&mut rif);
    for r in 0..10 {
        set_row(&mut rif, r, &format!("plain-old-row-{r:02}"));
    }
    set_row(&mut rif, 4, "edited-this-row!");
    let out = render_output(&mut rif);
    let text = String::from_utf8_lossy(&out);
    assert!(
        !text.contains("\x1b[1S"),
        "an edit is not a scroll: {text:?}"
    );
    assert!(text.contains("edited-this-row!"));
}

#[test]
fn every_render_is_wrapped_in_synchronized_output() {
    let mut rif = TtyRif::new(8, 4);
    set_row(&mut rif, 0, "abc");
    let out = render_output(&mut rif);
    let text = String::from_utf8_lossy(&out);
    let begin = text.find("\x1b[?2026h").expect("begin sync");
    let end = text.find("\x1b[?2026l").expect("end sync");
    assert!(begin < end, "sync begin must precede end");
    assert!(begin == 0, "sync begin must be the first bytes of a frame");
}

#[test]
fn distant_edits_on_one_row_skip_the_untouched_middle() {
    let mut rif = TtyRif::new(60, 4);
    set_row(
        &mut rif,
        1,
        "left-edit-here MIDDLE-STAYS-IDENTICAL right-edit-here",
    );
    let _ = render_output(&mut rif);
    set_row(
        &mut rif,
        1,
        "LEFT-EDIT-HERE MIDDLE-STAYS-IDENTICAL RIGHT-EDIT-HERE",
    );
    let out = render_output(&mut rif);
    let text = String::from_utf8_lossy(&out);
    assert!(
        !text.contains("MIDDLE-STAYS-IDENTICAL"),
        "unchanged middle must not be retransmitted: {text:?}"
    );
    assert!(text.contains("LEFT-EDIT-HERE") && text.contains("RIGHT-EDIT-HERE"));
    assert!(
        text.matches("\x1b[2;").count() >= 2,
        "two spans need two cursor motions on row 2: {text:?}"
    );
}

#[test]
fn nearby_edits_coalesce_into_one_span() {
    // Two edits six unchanged cells apart: retransmitting the gap is
    // cheaper than a second text run, so one span covers both. GNU preserves
    // the tail because the old and new effective lengths are equal.
    let mut rif = TtyRif::new(40, 4);
    set_row(&mut rif, 1, "abcdefgh");
    let _ = render_output(&mut rif);
    set_row(&mut rif, 1, "AbcdefgH");
    let out = render_output(&mut rif);
    let text = String::from_utf8_lossy(&out);
    assert_eq!(
        text.matches("\x1b[2;").count(),
        1,
        "a six-cell gap coalesces into one text span: {text:?}"
    );
    assert!(text.contains("AbcdefgH"));
    assert!(!text.contains("\x1b[K"));
}

#[test]
fn scroll_plan_is_one_scroll_op_plus_exposed_row_runs() {
    // Structural assertion on the typed plan: a one-line shift is exactly
    // one ScrollRows op followed by write runs that touch ONLY the exposed
    // row - no retransmission ops for shifted content.
    let mut rif = TtyRif::new(20, 10);
    for r in 0..10 {
        set_row(&mut rif, r, &format!("plan-row-{r:02}"));
    }
    let _ = render_output(&mut rif);
    for r in 0..9 {
        set_row(&mut rif, r, &format!("plan-row-{:02}", r + 1));
    }
    set_row(&mut rif, 9, "plan-row-10");
    let ops = rif.plan_for_test();
    assert!(
        matches!(ops.first(), Some(TermOp::ScrollRows { .. })),
        "first op must be the scroll: {ops:?}"
    );
    for op in &ops[1..] {
        match op {
            TermOp::WriteRun { row, .. } | TermOp::EraseToEol { row, .. } => {
                assert_eq!(*row, 9, "only the exposed row may be rewritten: {ops:?}");
            }
            other => panic!("unexpected op after scroll: {other:?}"),
        }
    }
}

#[test]
fn caps_without_scroll_region_never_plan_scroll_ops() {
    let mut rif = TtyRif::new(20, 10);
    rif.set_caps(TermCaps {
        right_margin: RightMarginBehavior::NoAutoWrap,
        scroll_region: None,
        insert_delete_char: false,
        blank_tail: BlankTailMethod::WriteSpaces,
        synchronized_output: false,
    });
    for r in 0..10 {
        set_row(&mut rif, r, &format!("dumb-term-{r:02}"));
    }
    let _ = render_output(&mut rif);
    for r in 0..9 {
        set_row(&mut rif, r, &format!("dumb-term-{:02}", r + 1));
    }
    set_row(&mut rif, 9, "dumb-term-10");
    let out = render_output(&mut rif);
    let text = String::from_utf8_lossy(&out);
    assert!(!text.contains("\x1b[?2026"), "sync gated off: {text:?}");
    assert!(
        !text.contains("S\x1b[r"),
        "no region scroll on a dumb terminal"
    );
    // Content still arrives, just via ordinary row diffs (which transmit
    // only the cells that changed, so assert on a changed fragment).
    assert!(
        text.contains("10"),
        "changed content must be drawn: {text:?}"
    );
}

#[test]
fn default_blank_rows_do_not_make_a_region_scroll_worthwhile() {
    let mut rif = TtyRif::new(12, 10);
    set_row(&mut rif, 8, "old-footer");
    let _ = render_output(&mut rif);

    rif.desired = TtyGrid::new(12, 10);
    set_row(&mut rif, 1, "new-line-1");
    set_row(&mut rif, 2, "new-line-2");
    rif.set_scroll_seed_for_test(Some(-2));

    let ops = rif.plan_for_test();
    assert!(
        ops.iter()
            .all(|op| !matches!(op, TermOp::ScrollRows { .. })),
        "moving default blank rows saves no terminal output and must not masquerade as a content scroll: {ops:?}",
    );
}

// ---------------------------------------------------------------------------
// Erase-to-EOL (issue #206 phase 4)
// ---------------------------------------------------------------------------

#[test]
fn line_kill_erases_to_eol_instead_of_writing_spaces() {
    let mut rif = TtyRif::new(40, 4);
    set_row(&mut rif, 1, "a-full-line-of-content-to-be-killed!");
    let _ = render_output(&mut rif);
    // Kill the line: desired becomes all blanks (grid default cells).
    for col in 0..40 {
        rif.desired.set(1, col, ' ', CellAttrs::default(), false);
    }
    let out = render_output(&mut rif);
    let text = String::from_utf8_lossy(&out);
    assert!(text.contains("\x1b[K"), "line kill must use EL: {text:?}");
    assert!(
        !text.contains("        "),
        "no long space runs when EL is available: {text:?}"
    );
    assert_eq!(rif.frame_stats().erase_ops, 1);

    // Steady state: the model must agree with the erased terminal.
    for col in 0..40 {
        rif.desired.set(1, col, ' ', CellAttrs::default(), false);
    }
    let out = render_output(&mut rif);
    assert_eq!(rif.frame_stats().write_runs, 0, "no-op after erase");
    let _ = out;
}

#[test]
fn inverse_blank_tail_refuses_erase() {
    let mut rif = TtyRif::new(20, 4);
    set_row(&mut rif, 1, "abcdefgh");
    let _ = render_output(&mut rif);
    let inverse_blank = CellAttrs {
        inverse: true,
        ..CellAttrs::default()
    };
    // A standout bar: inverse blanks render as a solid block; ESC[K would
    // erase it to plain background.
    for col in 0..20 {
        rif.desired.set(1, col, ' ', inverse_blank, false);
    }
    let out = render_output(&mut rif);
    let text = String::from_utf8_lossy(&out);
    assert!(
        !text.contains("\x1b[K"),
        "inverse blanks must be written, not erased: {text:?}"
    );
    assert_eq!(rif.frame_stats().erase_ops, 0);
}

#[test]
fn colored_tail_without_bce_stays_on_the_write_path() {
    let mut rif = TtyRif::new(20, 4);
    rif.set_caps(TermCaps {
        blank_tail: BlankTailMethod::EraseToEol {
            back_color_erase: false,
        },
        ..TermCaps::default()
    });
    set_row(&mut rif, 1, "abcdefghijkl");
    let _ = render_output(&mut rif);
    let colored_blank = CellAttrs {
        bg: Some(TerminalColor::Direct {
            r: 40,
            g: 44,
            b: 52,
        }),
        ..CellAttrs::default()
    };
    for col in 0..20 {
        rif.desired.set(1, col, ' ', colored_blank, false);
    }
    let out = render_output(&mut rif);
    let text = String::from_utf8_lossy(&out);
    assert!(
        !text.contains("\x1b[K"),
        "no BCE means a colored tail cannot be erased: {text:?}"
    );
}

// ---------------------------------------------------------------------------
// Efficiency regression guards (issue #206 evidence)
// ---------------------------------------------------------------------------

#[test]
fn one_line_scroll_costs_a_small_fraction_of_a_full_repaint() {
    // The quantified issue-206 claim, pinned as a regression guard: a
    // one-line scroll of a full 24x80 frame must emit far fewer bytes than
    // repainting it.
    let full_repaint_bytes = {
        let mut rif = TtyRif::new(80, 24);
        for r in 0..24 {
            set_row(
                &mut rif,
                r,
                &format!("{:-<79}{}", format!("content line {r} "), "|"),
            );
        }
        let out = render_output(&mut rif);
        out.len()
    };

    let mut rif = TtyRif::new(80, 24);
    for r in 0..24 {
        set_row(
            &mut rif,
            r,
            &format!("{:-<79}{}", format!("content line {r} "), "|"),
        );
    }
    let _ = render_output(&mut rif);
    for r in 0..23 {
        set_row(
            &mut rif,
            r,
            &format!("{:-<79}{}", format!("content line {} ", r + 1), "|"),
        );
    }
    set_row(&mut rif, 23, &format!("{:-<79}{}", "content line 24 ", "|"));
    let scroll_bytes = render_output(&mut rif).len();

    assert_eq!(rif.frame_stats().scroll_ops, 1);
    assert!(
        scroll_bytes * 5 < full_repaint_bytes,
        "a one-line scroll ({scroll_bytes} bytes) must cost <20% of a full repaint ({full_repaint_bytes} bytes)"
    );
}

#[test]
fn lying_scroll_seed_falls_back_to_correct_inference() {
    // The semantic seed is only a hint: a wrong delta must fail cell
    // verification and the voting path must still find the true scroll.
    let mut rif = TtyRif::new(20, 12);
    for r in 0..12 {
        set_row(&mut rif, r, &format!("seeded-line-{r:02}"));
    }
    let _ = render_output(&mut rif);
    for r in 0..11 {
        set_row(&mut rif, r, &format!("seeded-line-{:02}", r + 1));
    }
    set_row(&mut rif, 11, "seeded-line-12");
    rif.set_scroll_seed_for_test(Some(-3)); // a lie: the real shift is +1
    let ops = rif.plan_for_test();
    match ops.first() {
        Some(TermOp::ScrollRows { dir, .. }) => {
            assert!(
                matches!(dir, ScrollDir::Up(n) if n.get() == 1),
                "voting must recover the true delta despite the lying seed: {ops:?}"
            );
        }
        other => panic!("scroll expected despite lying seed: {other:?}"),
    }
    let stats = rif.frame_stats();
    assert_eq!(stats.scroll_seed_rejected, 1, "the lie must be counted");
    assert_eq!(stats.scroll_seed_accepted, 0);
}

#[test]
fn truthful_scroll_seed_is_accepted_and_counted() {
    let mut rif = TtyRif::new(20, 12);
    for r in 0..12 {
        set_row(&mut rif, r, &format!("seeded-line-{r:02}"));
    }
    let _ = render_output(&mut rif);
    for r in 0..11 {
        set_row(&mut rif, r, &format!("seeded-line-{:02}", r + 1));
    }
    set_row(&mut rif, 11, "seeded-line-12");
    rif.set_scroll_seed_for_test(Some(1));
    let ops = rif.plan_for_test();
    assert!(
        matches!(
            ops.first(),
            Some(TermOp::ScrollRows { dir: ScrollDir::Up(n), .. }) if n.get() == 1
        ),
        "seeded scroll expected: {ops:?}"
    );
    let stats = rif.frame_stats();
    assert_eq!(stats.scroll_seed_accepted, 1);
    assert_eq!(stats.scroll_seed_rejected, 0);
}

// ---------------------------------------------------------------------------
// In-line horizontal shifts (issue #206 phase 4b)
// ---------------------------------------------------------------------------

#[test]
fn typing_one_char_mid_line_uses_insert_cells() {
    let mut rif = TtyRif::new(60, 4);
    set_row(&mut rif, 1, "fn main() { println!(\"hello, world\"); }");
    let _ = render_output(&mut rif);
    set_row(&mut rif, 1, "fn mXain() { println!(\"hello, world\"); }");
    let out = render_output(&mut rif);
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("\x1b[1@"),
        "one typed char is one ICH: {text:?}"
    );
    assert!(
        !text.contains("println"),
        "the shifted tail must not be retransmitted: {text:?}"
    );

    // Steady state: the model matches the terminal after the shift.
    set_row(&mut rif, 1, "fn mXain() { println!(\"hello, world\"); }");
    let out = render_output(&mut rif);
    let text = String::from_utf8_lossy(&out);
    assert!(
        !text.contains('X') && !text.contains("main"),
        "no-op after shift: {text:?}"
    );
}

#[test]
fn deleting_one_char_mid_line_uses_delete_cells() {
    let mut rif = TtyRif::new(60, 4);
    set_row(&mut rif, 1, "fn mXain() { println!(\"hello, world\"); }");
    let _ = render_output(&mut rif);
    set_row(&mut rif, 1, "fn main() { println!(\"hello, world\"); } ");
    let out = render_output(&mut rif);
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("\x1b[1P"),
        "one deleted char is one DCH: {text:?}"
    );
    assert!(
        !text.contains("println"),
        "tail not retransmitted: {text:?}"
    );
}

#[test]
fn a_row_going_wholly_blank_erases_instead_of_shifting() {
    // GNU's update_frame_line (dispnew.c) strips trailing spaces from both the
    // old and the new row before any insert/delete-char reasoning, so a row
    // that becomes wholly blank has nlen == 0, no begmatch/endmatch to save,
    // and reaches just_erase -> clear_end_of_line. Shifting blanks onto blanks
    // is a degenerate match that preserves no content, and it costs both more
    // bytes than EL and a physically different terminal state: DCH shifts
    // written blanks in where EL leaves the cells unwritten.
    let mut rif = TtyRif::new(60, 4);
    set_row(&mut rif, 1, "tup.el\".");
    let _ = render_output(&mut rif);

    set_row(&mut rif, 1, &" ".repeat(60));
    let out = render_output(&mut rif);
    let text = String::from_utf8_lossy(&out);
    assert!(
        !text.contains("\x1b[8P"),
        "a wholly blank desired row must not be produced by DCH: {text:?}"
    );
    assert!(
        text.contains("\x1b[K"),
        "a wholly blank desired row is erased to end of line: {text:?}"
    );
}

#[test]
fn rows_with_wide_chars_refuse_horizontal_shifts() {
    let mut rif = TtyRif::new(30, 4);
    // A wide char occupies a base cell plus a padding cell.
    set_row(&mut rif, 1, "abc ");
    rif.desired.set(1, 4, '日', CellAttrs::default(), false);
    rif.desired.set(1, 5, ' ', CellAttrs::default(), true);
    set_row_from(&mut rif, 1, 6, "-tail-content-here");
    let _ = render_output(&mut rif);
    rif.desired.set(1, 0, 'a', CellAttrs::default(), false);
    rif.desired.set(1, 1, 'X', CellAttrs::default(), false);
    set_row_from(&mut rif, 1, 2, "bc ");
    rif.desired.set(1, 5, '日', CellAttrs::default(), false);
    rif.desired.set(1, 6, ' ', CellAttrs::default(), true);
    set_row_from(&mut rif, 1, 7, "-tail-content-her");
    let out = render_output(&mut rif);
    let text = String::from_utf8_lossy(&out);
    assert!(
        !text.contains('@') || !text.contains("\x1b[1@"),
        "wide-char rows must not shift: {text:?}"
    );
}

/// Write `text` into desired row `row` starting at `col`.
fn set_row_from(rif: &mut TtyRif, row: usize, col: usize, text: &str) {
    for (i, ch) in text.chars().enumerate() {
        rif.desired
            .set(row, col + i, ch, CellAttrs::default(), false);
    }
}

#[test]
fn typing_echo_bytes_are_a_fraction_of_tail_rewrite() {
    // Efficiency guard for the typing-echo case.
    let line = "let value = compute_the_answer(question, context, options);";
    let mut rif = TtyRif::new(80, 4);
    set_row(&mut rif, 1, line);
    let _ = render_output(&mut rif);
    let edited = format!("{}X{}", &line[..4], &line[4..]);
    set_row(&mut rif, 1, &edited);
    let out = render_output(&mut rif);
    let text = String::from_utf8_lossy(&out);
    // Sync wrapper + cursor hide + goto + ICH + goto + SGR + the one glyph.
    assert!(
        out.len() < 64,
        "typing echo must be tens of bytes, not a tail rewrite: {} {text:?}",
        out.len()
    );
    assert!(
        !text.contains("compute_the_answer"),
        "tail must ride the shift, not a rewrite: {text:?}"
    );
}

#[test]
fn index_method_scrolls_with_ind_ri_and_never_su_sd() {
    // vt220 / Linux-console shape: DECSTBM attested, CSI S/T not. The
    // capability carries the method, so the same plan encodes as cursor to
    // the region edge plus IND/RI.
    let caps = TermCaps {
        scroll_region: Some(RegionScrollMethod::Index),
        ..TermCaps::default()
    };
    let mut rif = TtyRif::new_with_caps(20, 10, caps);
    // Production rasterization clears the desired grid every frame; this
    // incremental harness must overwrite full rows for the same effect.
    let full_row = |n: usize| format!("{:<20}", format!("line-number-{n:02}"));
    for r in 0..10 {
        set_row(&mut rif, r, &full_row(r));
    }
    let _ = render_output(&mut rif);

    // Scroll up by one.
    for r in 0..10 {
        set_row(&mut rif, r, &full_row(r + 1));
    }
    let out = render_output(&mut rif);
    let text = String::from_utf8_lossy(&out);
    assert!(
        !text.contains("\x1b[1S") && !text.contains("\x1b[1T"),
        "index method must never emit SU/SD: {text:?}"
    );
    assert!(
        text.contains("\x1b[1;10r") && text.contains("\x1bD"),
        "scroll must be DECSTBM + IND at the region bottom: {text:?}"
    );
    assert!(
        !text.contains("line-number-05"),
        "shifted row content was retransmitted: {text:?}"
    );

    // And the reverse direction uses RI at the region top.
    for r in 0..10 {
        set_row(&mut rif, r, &full_row(r));
    }
    let out = render_output(&mut rif);
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("\x1bM") && !text.contains("\x1b[1T"),
        "reverse scroll must be RI, not SD: {text:?}"
    );
}

// ---------------------------------------------------------------------------
// Wide-char neutralization (GNU dispnew.c neutralize_wide_char parity)
// ---------------------------------------------------------------------------

#[test]
fn overwriting_the_padding_half_of_a_wide_pair_blanks_the_base() {
    // A child-frame border landing on the right half of a CJK char: the
    // terminal blanks the orphaned base, so the model must too, or the
    // divergence is invisible to every later diff.
    let mut grid = TtyGrid::new(10, 2);
    grid.set(1, 4, '\u{65e5}', CellAttrs::default(), false);
    grid.set(1, 5, ' ', CellAttrs::default(), true);

    grid.set(1, 5, '|', CellAttrs::default(), false);

    let base = &grid.cells[10 + 4];
    assert_eq!(base.ch, ' ', "orphaned wide base must become a space");
    assert!(!base.padding);
    assert_eq!(grid.cells[10 + 5].ch, '|');
}

#[test]
fn overwriting_the_base_half_of_a_wide_pair_blanks_the_padding() {
    let mut grid = TtyGrid::new(10, 2);
    grid.set(0, 2, '\u{65e5}', CellAttrs::default(), false);
    grid.set(0, 3, ' ', CellAttrs::default(), true);

    grid.set(0, 2, 'X', CellAttrs::default(), false);

    let orphan = &grid.cells[3];
    assert!(
        !orphan.padding,
        "orphaned padding must become a plain space"
    );
    assert_eq!(orphan.ch, ' ');
    assert_eq!(grid.cells[2].ch, 'X');
}

#[test]
fn rewriting_a_wide_pair_in_place_keeps_it_intact() {
    // The rasterizer writes base then padding every frame; neutralization
    // must not eat the pair it is in the middle of rewriting.
    let mut grid = TtyGrid::new(10, 1);
    grid.set(0, 2, '\u{65e5}', CellAttrs::default(), false);
    grid.set(0, 3, ' ', CellAttrs::default(), true);

    grid.set(0, 2, '\u{672c}', CellAttrs::default(), false);
    grid.set(0, 3, ' ', CellAttrs::default(), true);

    assert_eq!(grid.cells[2].ch, '\u{672c}');
    assert!(
        grid.cells[3].padding,
        "rewritten pair keeps its padding half"
    );
}

#[test]
fn wide_base_in_the_final_column_rasterizes_as_a_space() {
    // No room for the padding half: the terminal would blank the base while
    // the model kept it. GNU never emits a partially visible multi-column
    // glyph.
    let mut state = FrameDisplayState::new(10, 5, 8.0, 16.0);
    state.background = Color::BLACK;

    let mut matrix = GlyphMatrix::new(5, 10);
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    for (i, ch) in "abcdefghi".chars().enumerate() {
        row.glyphs[GlyphArea::Text as usize].push(Glyph::char(ch, FaceId::new(0), i));
    }
    let mut wide = Glyph::char('\u{4e16}', FaceId::new(0), 9);
    wide.wide = true;
    row.glyphs[GlyphArea::Text as usize].push(wide);
    matrix.rows[0] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(row);

    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(1),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 80.0, 80.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 80.0, 80.0),
        text_clip_bounds: None,
        selected: true,
    });

    let mut rif = TtyRif::new(10, 5);
    rif.rasterize(&state);

    assert_eq!(rif.desired.cells[9].ch, ' ', "clipped wide base is a space");
    assert!(!rif.desired.cells[9].padding);
    assert!(
        rif.desired.cells[..10].iter().all(|cell| !cell.padding),
        "no padding cell may survive at the row edge"
    );
}

// ---------------------------------------------------------------------------
// Planner hardening + coverage gaps from the adversarial review
// ---------------------------------------------------------------------------

#[test]
fn zero_area_grids_plan_nothing_and_do_not_panic() {
    for (w, h) in [(0, 10), (10, 0), (0, 0)] {
        let mut rif = TtyRif::new(w, h);
        let ops = rif.plan_for_test();
        assert!(ops.is_empty(), "{w}x{h} must plan the empty frame");
        let _ = render_output(&mut rif);
    }
}

#[test]
fn scrolling_backward_emits_region_scroll_down() {
    // Down-direction sibling of scrolled_rows_emit_region_scroll: content
    // moves down (viewport scrolled up), encoded as SD inside the region.
    let mut rif = TtyRif::new(20, 10);
    let full_row = |n: i32| format!("{:<20}", format!("line-number-{n:02}"));
    for r in 0..10i32 {
        set_row(&mut rif, r as usize, &full_row(r + 1));
    }
    let _ = render_output(&mut rif);
    for r in 0..10i32 {
        set_row(&mut rif, r as usize, &full_row(r));
    }
    let out = render_output(&mut rif);
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("\x1b[1T"),
        "one-line backward shift must emit SD: {text:?}"
    );
    assert!(
        !text.contains("line-number-05"),
        "shifted rows must not be retransmitted: {text:?}"
    );
    assert!(
        text.contains("line-number-00"),
        "the exposed top row must be drawn: {text:?}"
    );

    // Model fidelity: an identical next frame is a no-op.
    for r in 0..10i32 {
        set_row(&mut rif, r as usize, &full_row(r));
    }
    let out = render_output(&mut rif);
    let text = String::from_utf8_lossy(&out);
    assert!(
        !text.contains("line-number"),
        "model must match the terminal after SD: {text:?}"
    );
}

#[test]
fn colored_tail_with_bce_erases_and_establishes_the_fill_color() {
    // The accepting BCE path: erase must set the tail's background BEFORE
    // ESC[K with no reset between, or the fill color is wrong.
    let mut rif = TtyRif::new(20, 4);
    set_row(&mut rif, 1, "abcdefghijklmnopqrst");
    let _ = render_output(&mut rif);
    let colored_blank = CellAttrs {
        bg: Some(TerminalColor::Direct {
            r: 40,
            g: 44,
            b: 52,
        }),
        ..CellAttrs::default()
    };
    for col in 0..20 {
        rif.desired.set(1, col, ' ', colored_blank, false);
    }
    let out = render_output(&mut rif);
    let text = String::from_utf8_lossy(&out);
    let erase_at = text.find("\x1b[K").expect("BCE tail must erase");
    let bg_at = text
        .find("48;2;40;44;52m")
        .expect("fill background must be established");
    assert!(bg_at < erase_at, "bg must precede the erase: {text:?}");
    let between = &text[bg_at..erase_at];
    assert!(
        !between.contains("\x1b[0m"),
        "no SGR reset may sit between bg and erase: {text:?}"
    );
}

#[test]
fn layout_reused_shifted_damage_seeds_the_region_scroll() {
    // Producer-path acceptance: the semantic seed travels from
    // RowDamage::ReusedShifted on the glyph rows through rasterize (1x1
    // TTY metrics make dvpos the line delta) into an accepted, verified
    // ScrollRows plan — not via the test-only seed setter.
    let cols = 20usize;
    let rows = 12usize;
    let build_state = |first_line: usize, shifted: bool| {
        let mut state = FrameDisplayState::new(cols, rows, 1.0, 1.0);
        state.background = Color::BLACK;
        let mut matrix = GlyphMatrix::new(rows, cols);
        for r in 0..rows {
            let mut row = GlyphRow::new(GlyphRowRole::Text);
            for (i, ch) in format!("{:<20}", format!("seed-line-{:02}", first_line + r))
                .chars()
                .enumerate()
            {
                row.glyphs[GlyphArea::Text as usize].push(Glyph::char(ch, FaceId::new(0), i));
            }
            matrix.rows[r] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(row);
            if shifted {
                matrix.set_row_damage(r, RowDamage::ReusedShifted { dvpos: Px(-1.0) });
            }
        }
        state.window_matrices.push(WindowMatrixEntry {
            window_id: DisplayWindowId::new(1),
            matrix,
            pixel_bounds: Rect::new(0.0, 0.0, cols as f32, rows as f32),
            text_pixel_bounds: Rect::new(0.0, 0.0, cols as f32, rows as f32),
            text_clip_bounds: None,
            selected: true,
        });
        state
    };

    let mut rif = TtyRif::new(cols, rows);
    rif.rasterize(&build_state(0, false));
    let _ = render_output(&mut rif);

    // One line scrolled: content moved up (dvpos = -1px at 1px line height).
    rif.rasterize(&build_state(1, true));
    let ops = rif.plan_for_test();
    assert!(
        matches!(
            ops.first(),
            Some(TermOp::ScrollRows { dir: ScrollDir::Up(n), .. }) if n.get() == 1
        ),
        "shifted-damage seed must plan a verified region scroll: {ops:?}"
    );
    assert_eq!(
        rif.frame_stats().scroll_seed_accepted,
        1,
        "the layout hint, not voting, must carry this scroll"
    );
}

#[test]
fn reused_damage_rows_carry_verbatim_and_plan_nothing() {
    // Damage-aware carry: rows the layout engine reused verbatim are copied
    // from the previous frame's grid (no re-rasterize) and the planner skips
    // them without a cell compare — the GNU update_window contract that rows
    // the desired matrix left untouched never reach the terminal.
    let cols = 20usize;
    let rows = 8usize;
    let build_state = |damage: Option<RowDamage>, edited_row: Option<usize>| {
        let mut state = FrameDisplayState::new(cols, rows, 1.0, 1.0);
        state.background = Color::BLACK;
        let mut matrix = GlyphMatrix::new(rows, cols);
        for r in 0..rows {
            let mut row = GlyphRow::new(GlyphRowRole::Text);
            let text = if edited_row == Some(r) {
                format!("{:<20}", format!("edited-line-{r:02}"))
            } else {
                format!("{:<20}", format!("stable-line-{r:02}"))
            };
            for (i, ch) in text.chars().enumerate() {
                row.glyphs[GlyphArea::Text as usize].push(Glyph::char(ch, FaceId::new(0), i));
            }
            matrix.rows[r] = neomacs_display_protocol::glyph_matrix::MatrixRow::new(row);
            if let Some(damage) = damage {
                if edited_row != Some(r) {
                    matrix.set_row_damage(r, damage);
                }
            }
        }
        state.window_matrices.push(WindowMatrixEntry {
            window_id: DisplayWindowId::new(1),
            matrix,
            pixel_bounds: Rect::new(0.0, 0.0, cols as f32, rows as f32),
            text_pixel_bounds: Rect::new(0.0, 0.0, cols as f32, rows as f32),
            text_clip_bounds: None,
            selected: true,
        });
        state
    };

    let mut rif = TtyRif::new(cols, rows);
    rif.rasterize(&build_state(None, None));
    let _ = render_output(&mut rif);

    // Frame 2: identical content, all rows marked Reused → empty plan.
    rif.rasterize(&build_state(Some(RowDamage::Reused), None));
    assert!(
        rif.plan_for_test().is_empty(),
        "an all-reused frame must plan no ops"
    );
    let _ = render_output(&mut rif);

    // Frame 3: one row relaid (damage New, changed content), rest Reused →
    // exactly that row is rewritten, and its bytes carry the new content.
    rif.rasterize(&build_state(Some(RowDamage::Reused), Some(3)));
    let ops = rif.plan_for_test();
    assert!(
        ops.iter().all(|op| match op {
            TermOp::WriteRun { row, .. } => *row == 3,
            _ => false,
        }) && !ops.is_empty(),
        "only the edited row may be written: {ops:?}"
    );
}

// ---------------------------------------------------------------------------
// Underline colour -- GNU `Setulc` / `TF_set_underline_color`
// ---------------------------------------------------------------------------

/// GNU emits the underline colour LAST, after the foreground and the
/// background, and it emits it as three channels.
///
/// `turn_on_face`'s colour block ends with
///
/// ```c
///  ts = tty->TF_set_underline_color;
///  if (ts && face->underline_color)
///    { p = tparam (ts, NULL, 0, face->underline_color, 0, 0, 0); OUTPUT (tty, p); }
/// ```
///
/// (src/term.c:2119-2126), and `TF_set_underline_color` is one fixed string
/// GNU installs itself (src/term.c:4708):
///
/// ```text
///   \e[58:2::%p1%{65536}%/%d:%p1%{256}%/%{255}%&%d:%p1%{255}%&%dm
/// ```
///
/// One parameter, split into three channels. Measured from GNU 31.0.90 on a
/// pty, TERM=tmux-256color, face `(:underline (:color "red" :style wave))`:
///
/// ```text
///   COLORTERM unset    \e[4:3m \e[58:2::0:0:1m WAVY
///   COLORTERM=truecolor \e[4:3m \e[58:2::205:0:0m WAVY
/// ```
///
/// so the same slot spells a palette subscript as `0:0:N` and a 24-bit pixel
/// as `R:G:B`, exactly as `Setulc`'s single parameter implies.
#[test]
fn underline_color_is_gnus_setulc_emitted_after_the_other_colors() {
    let attrs = CellAttrs {
        fg: Some(TerminalColor::Indexed(2)),
        underline: UnderlineStyle::Wave.gnu_code(),
        underline_color: Some(TerminalColor::Indexed(1)),
        ..CellAttrs::default()
    };
    let mut buf = Vec::new();
    write_turn_on_face(&mut buf, &attrs, &TtyAttributeCapabilities::full());
    let s = String::from_utf8(buf).unwrap();

    assert!(
        s.contains("\x1b[58:2::0:0:1m"),
        "palette subscript 1 must spell as GNU's Setulc parameter: {s:?}"
    );
    let underline_color = s.find("\x1b[58").expect("Setulc emitted");
    let foreground = s.find("\x1b[32m").expect("foreground emitted");
    assert!(
        foreground < underline_color,
        "GNU emits the underline colour after fg and bg: {s:?}"
    );

    let direct = CellAttrs {
        underline: UnderlineStyle::Wave.gnu_code(),
        underline_color: Some(TerminalColor::Direct { r: 205, g: 0, b: 0 }),
        ..CellAttrs::default()
    };
    let mut buf = Vec::new();
    write_turn_on_face(&mut buf, &direct, &TtyAttributeCapabilities::full());
    let s = String::from_utf8(buf).unwrap();
    assert!(
        s.contains("\x1b[58:2::205:0:0m"),
        "a 24-bit underline colour spells as three channels: {s:?}"
    );
}

/// `TF_set_underline_color` is installed only when `TF_set_underline_style`
/// is (src/term.c:4705-4708), so a terminal with no `Smulx` gets no underline
/// colour even though it still gets the plain underline. The whole block also
/// sits inside `if (tty->TN_max_colors > 0)` (src/term.c:2092).
#[test]
fn underline_color_needs_smulx_and_colors() {
    let attrs = CellAttrs {
        underline: UnderlineStyle::Line.gnu_code(),
        underline_color: Some(TerminalColor::Indexed(1)),
        ..CellAttrs::default()
    };

    let no_smulx = TtyAttributeCapabilities {
        styled_underline: None,
        ..TtyAttributeCapabilities::full()
    };
    let mut buf = Vec::new();
    write_turn_on_face(&mut buf, &attrs, &no_smulx);
    let s = String::from_utf8(buf).unwrap();
    assert!(s.contains("\x1b[4m"), "the plain underline still stands");
    assert!(!s.contains("\x1b[58"), "no Smulx, no Setulc: {s:?}");

    let no_colors = TtyAttributeCapabilities {
        colors: TtyColorSource::NoDatabase { max_colors: 0 },
        ..TtyAttributeCapabilities::full()
    };
    let mut buf = Vec::new();
    write_turn_on_face(&mut buf, &attrs, &no_colors);
    let s = String::from_utf8(buf).unwrap();
    assert!(!s.contains("\x1b[58"), "no colours, no Setulc: {s:?}");
}

/// GNU cannot tell "no underline colour" from "underline colour zero": the
/// realized slot is one `unsigned long` and `if (ts && face->underline_color)`
/// reads 0 as absent (src/term.c:2120). Measured from GNU on a pty,
/// TERM=tmux-256color, face `(:underline (:color "black" :style wave))`:
///
/// ```text
///   \e[4:3mCASEc          -- the wave, and no \e[58 at all
/// ```
///
/// both with and without COLORTERM=truecolor, because "black" realizes to
/// palette subscript 0 on one and to pixel 0x000000 on the other.
#[test]
fn an_underline_color_of_zero_is_gnus_absent_underline_color() {
    for zero in [
        TerminalColor::Indexed(0),
        TerminalColor::Direct { r: 0, g: 0, b: 0 },
    ] {
        let attrs = CellAttrs {
            underline: UnderlineStyle::Wave.gnu_code(),
            underline_color: Some(zero),
            ..CellAttrs::default()
        };
        let mut buf = Vec::new();
        write_turn_on_face(&mut buf, &attrs, &TtyAttributeCapabilities::full());
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("\x1b[4:3m"), "the wave still stands: {s:?}");
        assert!(
            !s.contains("\x1b[58"),
            "{zero:?} is GNU's absent underline colour: {s:?}"
        );
    }
}

/// An underline with no colour of its own emits no `Setulc`. GNU sets
/// `face->underline_color = 0` for `:underline t` (src/xfaces.c:6741), for
/// `(:style STYLE)` with no `:color` (:6756), and for
/// `(:color foreground-color)` (:6772-6773). Measured on a pty,
/// TERM=tmux-256color:
///
/// ```text
///   (:underline t)                              \e[4mPLAIN
///   (:underline (:style dots))                  \e[4:4mCASEf
///   (:underline (:color foreground-color ...))  \e[4:3mCASEe
/// ```
#[test]
fn an_underline_without_a_color_emits_no_setulc() {
    let attrs = CellAttrs {
        underline: UnderlineStyle::Dotted.gnu_code(),
        underline_color: None,
        ..CellAttrs::default()
    };
    let mut buf = Vec::new();
    write_turn_on_face(&mut buf, &attrs, &TtyAttributeCapabilities::full());
    let s = String::from_utf8(buf).unwrap();
    assert!(
        s.contains("\x1b[4:4m"),
        "the dotted underline stands: {s:?}"
    );
    assert!(!s.contains("\x1b[58"), "no colour, no Setulc: {s:?}");
}

/// The colour rides a face, not a guess: `resolve_attrs` must carry the
/// realized `terminal_underline_color` through unchanged, the way it already
/// carries the realized foreground and background.
#[test]
fn resolve_attrs_carries_the_realized_underline_color() {
    let mut rif = TtyRif::new(4, 1);
    let mut face = Face::new(FaceId::new(7));
    face.underline_style = UnderlineStyle::Wave;
    face.terminal_underline_color = Some(TerminalColor::Indexed(1));
    let mut faces = HashMap::new();
    faces.insert(FaceId::new(7), face);
    rif.set_faces(faces);

    let attrs = rif.resolve_attrs(FaceId::new(7));
    assert_eq!(attrs.underline_color, Some(TerminalColor::Indexed(1)));
    assert_eq!(attrs.underline, UnderlineStyle::Wave.gnu_code());
}

/// GNU's `Setulc` guard has no `MAY_USE_WITH_COLORS_P` term, unlike every
/// other arm of `turn_on_face`: it is `if (ts && face->underline_color)`
/// (src/term.c:2120).  So an `ncv` that forbids underline on a colour frame
/// suppresses the underline and NOT its colour.
///
/// Unmeasurable against a real terminal: no entry ncurses ships carries both
/// `Smulx` and an `ncv`, checked across tmux-256color, alacritty, kitty,
/// vte-256color and tmux.  Pinned so that the literal reading is the one that
/// stays, rather than drifting to the `supports` gate the arms above use.
#[test]
fn ncv_suppresses_the_underline_but_not_gnus_underline_color() {
    let attrs = CellAttrs {
        underline: UnderlineStyle::Wave.gnu_code(),
        underline_color: Some(TerminalColor::Indexed(1)),
        ..CellAttrs::default()
    };
    let ncv_underline = TtyAttributeCapabilities {
        no_color_video: TtyNoColorVideo::UNDERLINE,
        ..TtyAttributeCapabilities::full()
    };
    let mut buf = Vec::new();
    write_turn_on_face(&mut buf, &attrs, &ncv_underline);
    let s = String::from_utf8(buf).unwrap();

    for underline in ["\x1b[4m", "\x1b[4:3m"] {
        assert!(
            !s.contains(underline),
            "ncv forbids the underline itself: {s:?}"
        );
    }
    assert!(
        s.contains("\x1b[58:2::0:0:1m"),
        "GNU's Setulc guard does not consult ncv: {s:?}"
    );
}
