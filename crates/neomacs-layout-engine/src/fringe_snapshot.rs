//! Publish each laid-out row's fringe bitmaps into the per-window display
//! snapshot the evaluator reads.
//!
//! GNU answers `fringe-bitmaps-at-pos` straight out of the window's current
//! matrix (`src/fringe.c` `Ffringe_bitmaps_at_pos` → `row_containing_pos`).
//! Our matrices live on this side of the crate boundary and neovm-core cannot
//! see them, so the row's three fringe slots travel out with the snapshot that
//! already carries the row's buffer-position span. Re-deriving the bitmaps on
//! the Lisp side from text properties is not an option: it would see none of
//! the truncation, continuation, empty-line or overlay-arrow indicators that
//! redisplay itself puts in the fringe.

use neomacs_display_protocol::glyph_matrix::{GlyphRow, WindowMatrixEntry};
use neovm_core::window::{
    FringeBitmapIndex, RowFringeBitmaps, RowOverlayArrowBitmap, WindowPresentationSnapshot,
};

/// Copy the fringe slots of every matrix row onto the snapshot row that shares
/// its output row index.
pub(crate) fn publish_row_fringe_bitmaps(
    entries: &[WindowMatrixEntry],
    snapshots: &mut [WindowPresentationSnapshot],
) {
    for publication in snapshots.iter_mut() {
        let snapshot = publication.display_snapshot_mut();
        let Some(entry) = entries
            .iter()
            .find(|entry| entry.window_id.get() == snapshot.window_id.0 as i64)
        else {
            continue;
        };
        for row in &mut snapshot.rows {
            let fringe = usize::try_from(row.row)
                .ok()
                .and_then(|index| entry.matrix.rows.get(index))
                .filter(|matrix_row| matrix_row.enabled)
                .map(|matrix_row| row_fringe_bitmaps(matrix_row))
                .unwrap_or_default();
            row.fringe = fringe;
        }
    }
}

fn row_fringe_bitmaps(row: &GlyphRow) -> RowFringeBitmaps {
    RowFringeBitmaps {
        left: row
            .left_fringe_bitmap
            .map(|info| FringeBitmapIndex(info.bitmap_index)),
        right: row
            .right_fringe_bitmap
            .map(|info| FringeBitmapIndex(info.bitmap_index)),
        // The layout always resolves an arrow to a concrete registry index
        // before stamping it, so GNU's negative "unresolved" encoding
        // (`RowOverlayArrowBitmap::Unresolved`) is never produced here.
        overlay_arrow: match row.overlay_arrow_bitmap {
            Some(info) => RowOverlayArrowBitmap::Bitmap(FringeBitmapIndex(info.bitmap_index)),
            None => RowOverlayArrowBitmap::Absent,
        },
    }
}
