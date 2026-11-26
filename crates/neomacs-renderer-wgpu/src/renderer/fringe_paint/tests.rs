use super::*;

const ROW_HEIGHT: i32 = 17;
const FRINGE_WIDTH: i32 = 8;

/// A non-periodic, centered bitmap one [`FringeBitmapData::bits`] row per
/// pixel row, default-aligned.
fn bitmap(rows: &[u16], width: u8) -> FringeBitmapData {
    FringeBitmapData {
        bits: rows.to_vec(),
        width,
        height: rows.len() as u8,
        period: 0,
        align: 0,
    }
}

/// A bitmap `rows` high whose every row is `row`.
fn solid(row: u16, width: u8, rows: usize) -> FringeBitmapData {
    bitmap(&vec![row; rows], width)
}

/// An 8-px left fringe beside the row `ROW_HEIGHT` high at the frame's top
/// left, fully on screen.
fn place(bitmap: &FringeBitmapData, kind: FringeBitmapKind) -> FringeBitmapPlace<'_> {
    FringeBitmapPlace {
        column_x: 0,
        column_width: FRINGE_WIDTH,
        row_y: 0,
        row_height: ROW_HEIGHT,
        visible: None,
        bitmap,
        side: FringeSide::Left,
        kind,
    }
}

fn background(x: i32, y: i32, width: i32, height: i32) -> FringeRect {
    FringeRect {
        x: x as f32,
        y: y as f32,
        width: width as f32,
        height: height as f32,
        ink: FringeInk::Background,
    }
}

fn foreground(x: i32, y: i32, width: i32) -> FringeRect {
    FringeRect {
        ink: FringeInk::Foreground,
        ..background(x, y, width, 1)
    }
}

#[test]
fn blank_bitmap_clears_the_row_fringe_with_the_face_background() {
    // Issue #834: the reported bitmap (rows of 128, width 4) has no set bit
    // inside its four columns. GNU still gives the row its face background
    // across the whole fringe -- a solid blue bar, no foreground pixel.
    let blank = solid(0x0000, 4, ROW_HEIGHT as usize);
    assert_eq!(
        fringe_bitmap_rects(place(&blank, FringeBitmapKind::RowBitmap)),
        vec![background(0, 0, FRINGE_WIDTH, ROW_HEIGHT)]
    );
}

#[test]
fn row_bitmap_paints_set_bits_in_the_foreground_over_the_background() {
    // A solid four-pixel bar: centered in the eight-pixel fringe, it leaves
    // two background columns on each side (GNU: `p.x = x - p.wd - (wd - p.wd) / 2`).
    let bar = solid(0xF000, 4, ROW_HEIGHT as usize);
    let mut expected = vec![background(0, 0, FRINGE_WIDTH, ROW_HEIGHT)];
    expected.extend((0..ROW_HEIGHT).map(|y| foreground(2, y, 4)));
    assert_eq!(
        fringe_bitmap_rects(place(&bar, FringeBitmapKind::RowBitmap)),
        expected
    );
}

#[test]
fn overlay_paints_set_bits_only() {
    // The overlay arrow is masked over the row's fringe; it has no background
    // (`x_draw_fringe_bitmap` clears nothing and `x_cr_draw_image` skips the
    // background fill for an overlay).
    let arrow = bitmap(&[0x8000, 0xC000, 0xE000, 0xC000, 0x8000], 4);
    assert_eq!(
        fringe_bitmap_rects(place(&arrow, FringeBitmapKind::Overlay)),
        vec![
            foreground(2, 6, 1),
            foreground(2, 7, 2),
            foreground(2, 8, 3),
            foreground(2, 9, 2),
            foreground(2, 10, 1),
        ]
    );
}

#[test]
fn diff_hl_column_and_caps_cover_the_fringe() {
    // diff-hl's `diff-hl-bmp-single' for an eight-pixel fringe: full-width
    // caps on the first and last row, a one-pixel column on the left edge of
    // every row (bit `1- w` is the leftmost of the low `w` columns).
    let mut rows = vec![0x8000u16; ROW_HEIGHT as usize];
    rows[0] = 0xFF00;
    rows[ROW_HEIGHT as usize - 1] = 0xFF00;
    let single = bitmap(&rows, 8);
    let mut expected = vec![background(0, 0, FRINGE_WIDTH, ROW_HEIGHT)];
    expected.push(foreground(0, 0, 8));
    expected.extend((1..ROW_HEIGHT - 1).map(|y| foreground(0, y, 1)));
    expected.push(foreground(0, ROW_HEIGHT - 1, 8));
    assert_eq!(
        fringe_bitmap_rects(place(&single, FringeBitmapKind::RowBitmap)),
        expected
    );
}

#[test]
fn bitmap_wider_than_the_fringe_shows_its_leftmost_columns() {
    let wide = solid(0xFFFF, 16, ROW_HEIGHT as usize);
    let mut expected = vec![background(0, 0, FRINGE_WIDTH, ROW_HEIGHT)];
    expected.extend((0..ROW_HEIGHT).map(|y| foreground(0, y, 8)));
    assert_eq!(
        fringe_bitmap_rects(place(&wide, FringeBitmapKind::RowBitmap)),
        expected
    );
}

#[test]
fn narrow_bitmap_is_centered_with_truncating_division() {
    // GNU `draw_fringe_bitmap_1` (src/fringe.c): for an odd slack (9 - 4 = 5,
    // half = 2), the left fringe places the bar at `9 - 4 - 2 = 3` (toward the
    // text area) and the right fringe places it at `0 + 2 = 2` (also toward
    // the text area).
    let bar = solid(0xF000, 4, 1);
    let left_place = FringeBitmapPlace {
        column_width: 9,
        side: FringeSide::Left,
        ..place(&bar, FringeBitmapKind::RowBitmap)
    };
    assert_eq!(
        fringe_bitmap_rects(left_place),
        vec![background(0, 0, 9, ROW_HEIGHT), foreground(3, 8, 4)]
    );
    let right_place = FringeBitmapPlace {
        column_width: 9,
        side: FringeSide::Right,
        ..place(&bar, FringeBitmapKind::RowBitmap)
    };
    assert_eq!(
        fringe_bitmap_rects(right_place),
        vec![background(0, 0, 9, ROW_HEIGHT), foreground(2, 8, 4)]
    );
}

#[test]
fn alignment_places_a_short_bitmap_top_center_or_bottom() {
    let three = solid(0xF000, 4, 3);
    for (align, top) in [(1u8, 0), (0, 7), (2, 14)] {
        let bitmap = FringeBitmapData {
            align,
            ..three.clone()
        };
        let paint = fringe_bitmap_rects(place(&bitmap, FringeBitmapKind::RowBitmap));
        assert_eq!(
            paint,
            vec![
                background(0, 0, FRINGE_WIDTH, ROW_HEIGHT),
                foreground(2, top, 4),
                foreground(2, top + 1, 4),
                foreground(2, top + 2, 4),
            ],
            "align {align}"
        );
    }
}

#[test]
fn bitmap_taller_than_the_row_is_clipped_to_the_row() {
    // (17 - 20) / 2 truncates to -1, so the bitmap starts one pixel above the
    // row and is clipped to it on both sides.
    let tall = solid(0xF000, 4, 20);
    let paint = fringe_bitmap_rects(place(&tall, FringeBitmapKind::RowBitmap));
    assert_eq!(paint.len(), 1 + ROW_HEIGHT as usize);
    assert_eq!(paint[0], background(0, 0, FRINGE_WIDTH, ROW_HEIGHT));
    assert_eq!(paint[1], foreground(2, 0, 4));
    assert_eq!(paint[ROW_HEIGHT as usize], foreground(2, 16, 4));
}

#[test]
fn partly_visible_row_paints_only_its_visible_slice() {
    // The row occupies y 0..17 of the frame but the text area starts at y 10:
    // seven rows show. GNU clears `visible_height` rows below
    // `max (header_line_height, row->y)` and bottom-aligns against the visible
    // bottom.
    let three = FringeBitmapData {
        align: 2,
        ..solid(0xF000, 4, 3)
    };
    let place = FringeBitmapPlace {
        visible: Some(Rect::new(0.0, 10.0, 640.0, 200.0)),
        ..place(&three, FringeBitmapKind::RowBitmap)
    };
    assert_eq!(
        fringe_bitmap_rects(place),
        vec![
            background(0, 10, FRINGE_WIDTH, 7),
            foreground(2, 14, 4),
            foreground(2, 15, 4),
            foreground(2, 16, 4),
        ]
    );
}

#[test]
fn periodic_bitmap_keeps_one_grid_across_rows() {
    // GNU's `empty-line'-style bitmap: a period of 3 whose motif is one dash
    // row and two blank rows, stored as height 255. It starts `row y mod
    // period` rows into the motif, so the next row continues the grid.
    let mut bits = Vec::new();
    while bits.len() < 255 {
        bits.extend([0xFF00u16, 0x0000, 0x0000]);
    }
    bits.truncate(255);
    let periodic = FringeBitmapData {
        bits,
        width: 8,
        height: 255,
        period: 3,
        align: 1,
    };
    let dashes = |row_y: i32| {
        fringe_bitmap_rects(FringeBitmapPlace {
            row_y,
            visible: Some(Rect::new(0.0, 0.0, 640.0, 100.0)),
            ..place(&periodic, FringeBitmapKind::RowBitmap)
        })
        .into_iter()
        .filter(|rect| rect.ink == FringeInk::Foreground)
        .map(|rect| rect.y as i32)
        .collect::<Vec<_>>()
    };
    // Rows at y = 0 and y = 17: 17 mod 3 = 2, so the second row starts at the
    // end of a blank pair and puts its dashes at frame y = 18, 21, ..., 33,
    // continuing the 0, 3, 6, 9, 12, 15 grid unbroken across rows.
    assert_eq!(dashes(0), vec![0, 3, 6, 9, 12, 15]);
    assert_eq!(dashes(17), vec![18, 21, 24, 27, 30, 33]);
}

#[test]
fn a_face_without_a_background_paints_none() {
    // A face that sets no background shows the window background, which is
    // what an unspecified GNU face background inherits from the default face.
    let transparent = Color::new(0.0, 0.0, 0.0, 0.0);
    assert_eq!(
        fringe_ink_color(FringeInk::Background, Color::BLACK, transparent),
        None
    );
    assert_eq!(
        fringe_ink_color(FringeInk::Foreground, transparent, Color::WHITE),
        Some(transparent)
    );
    assert_eq!(
        fringe_ink_color(FringeInk::Background, Color::BLACK, Color::WHITE),
        Some(Color::WHITE)
    );
}
