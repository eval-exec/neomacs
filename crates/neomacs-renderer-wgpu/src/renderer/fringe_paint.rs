//! The opaque rectangles GNU paints for one fringe bitmap.
//!
//! `draw_fringe_bitmap_1` (src/fringe.c) places the bitmap in the row's fringe
//! column and, for a row's own bitmap, clears the row's fringe with the face
//! background whenever the bitmap does not fill it; `x_draw_fringe_bitmap`
//! (src/xterm.c) then fills the bitmap's own rectangle with that background
//! and masks the set bits in the foreground. An overlay, the overlay arrow, is
//! masked without either background. Everything here is whole-pixel integer
//! geometry, as in GNU: the divisions truncate like C's.

use neomacs_display_protocol::frame_glyphs::{FringeBitmapData, FringeBitmapKind, FringeSide};
use neomacs_display_protocol::types::{Color, Rect};

/// Which of the fringe face's two colours a rectangle is painted with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FringeInk {
    Foreground,
    Background,
}

/// One opaque rectangle, in paint order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct FringeRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub ink: FringeInk,
}

/// One fringe bitmap draw: the fringe column it goes into and the row it
/// decorates, in frame pixels.
#[derive(Clone, Copy, Debug)]
pub(super) struct FringeBitmapPlace<'a> {
    /// Left edge and width of the fringe column.
    pub column_x: i32,
    pub column_width: i32,
    /// Top and full height of the row the bitmap decorates.
    pub row_y: i32,
    pub row_height: i32,
    /// The band of the frame the row may paint in. A row partly scrolled
    /// under a header line shows only a slice of itself, which is GNU's
    /// `row->visible_height` below `max (header_line_height, row->y)`.
    pub visible: Option<Rect>,
    pub bitmap: &'a FringeBitmapData,
    pub side: FringeSide,
    pub kind: FringeBitmapKind,
}

impl FringeBitmapPlace<'_> {
    /// The top and height of the row that is on screen.
    fn visible_band(self) -> (i32, i32) {
        let Some(visible) = self.visible else {
            return (self.row_y, self.row_height);
        };
        let top = self.row_y.max(visible.y.round() as i32);
        let bottom = (self.row_y + self.row_height).min(visible.bottom().round() as i32);
        (top, (bottom - top).max(0))
    }
}

/// The draw geometry of one fringe glyph: its column `(x, y, width, height)`
/// and the band it clips to, both moved by the paint offset of the layer that
/// draws it.
#[allow(clippy::too_many_arguments)]
pub(super) fn fringe_place(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    clip_rect: Option<Rect>,
    offset: (f32, f32),
    bitmap: &FringeBitmapData,
    side: FringeSide,
    kind: FringeBitmapKind,
) -> FringeBitmapPlace<'_> {
    FringeBitmapPlace {
        column_x: (x + offset.0).round() as i32,
        column_width: width.round() as i32,
        row_y: (y + offset.1).round() as i32,
        row_height: height.round() as i32,
        visible: clip_rect.map(|rect| {
            Rect::new(
                rect.x + offset.0,
                rect.y + offset.1,
                rect.width,
                rect.height,
            )
        }),
        bitmap,
        side,
        kind,
    }
}

/// The rectangles that paint `place`, in the order GNU paints them.
pub(super) fn fringe_bitmap_rects(place: FringeBitmapPlace<'_>) -> Vec<FringeRect> {
    let (band_y, band_h) = place.visible_band();
    if band_h <= 0 || place.column_width <= 0 {
        return Vec::new();
    }
    let mut rects = Vec::new();
    if place.kind == FringeBitmapKind::RowBitmap {
        // The strip clear and the bitmap's own background fill cover the same
        // pixels: the bitmap never leaves its column, and both are clipped to
        // the visible band.
        rects.push(FringeRect {
            x: place.column_x as f32,
            y: band_y as f32,
            width: place.column_width as f32,
            height: band_h as f32,
            ink: FringeInk::Background,
        });
    }

    let bitmap = place.bitmap;
    let period = i32::from(bitmap.period);
    // A bitmap wider than the fringe shows its leftmost columns. GNU's
    // `draw_fringe_bitmap_1` (src/fringe.c) centers toward the text area:
    // left fringe uses `right_edge - wd - (col_wd - wd) / 2`, right fringe
    // uses `left_edge + (col_wd - wd) / 2`.
    let columns = i32::from(bitmap.width).min(place.column_width);
    let x = match place.side {
        FringeSide::Left => {
            place.column_x + place.column_width - columns - (place.column_width - columns) / 2
        }
        FringeSide::Right => place.column_x + (place.column_width - columns) / 2,
    };
    let y = place.row_y;
    // A periodic bitmap continues one global grid across rows: it starts
    // `row y mod period` rows into its motif.
    let phase = if period > 0 { y.rem_euclid(period) } else { 0 };
    let rows = i32::from(bitmap.height) - phase;
    let y = match bitmap.align {
        2 => (place.row_y + place.row_height).min(band_y + band_h) - rows,
        1 => y,
        _ => y + (place.row_height - rows) / 2,
    };

    for row in 0..rows {
        let py = y + row;
        if py < band_y {
            continue;
        }
        if py >= band_y + band_h {
            break;
        }
        let motif_row = if period > 0 {
            (phase + row) % period
        } else {
            row
        };
        let Some(bits) = bitmap.bits.get(motif_row as usize) else {
            break;
        };
        let mut column = 0;
        while column < columns {
            if (bits >> (15 - column)) & 1 == 0 {
                column += 1;
                continue;
            }
            let start = column;
            column += 1;
            while column < columns && (bits >> (15 - column)) & 1 != 0 {
                column += 1;
            }
            rects.push(FringeRect {
                x: (x + start) as f32,
                y: py as f32,
                width: (column - start) as f32,
                height: 1.0,
                ink: FringeInk::Foreground,
            });
        }
    }
    rects
}

/// The colour a rectangle paints with, or `None` when it paints nothing: a
/// face that sets no background shows the window background, which is what an
/// unspecified GNU face background inherits from the default face.
pub(super) fn fringe_ink_color(
    ink: FringeInk,
    foreground: Color,
    background: Color,
) -> Option<Color> {
    match ink {
        FringeInk::Foreground => Some(foreground),
        FringeInk::Background => (background.a > 0.0).then_some(background),
    }
}

#[cfg(test)]
#[path = "fringe_paint/tests.rs"]
mod tests;
