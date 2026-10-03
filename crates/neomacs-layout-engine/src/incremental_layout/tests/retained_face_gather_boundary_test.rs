//! API controls for complete numeric dependencies. These are expected GREEN
//! with either direct collector; they are not literal-predecessor RED claims.

use super::*;
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
use neomacs_display_protocol::glyph_matrix::{
    FringeBitmapInfo, Glyph, GlyphPointerAppearance, GlyphPointerSourceIdentity,
};

fn row(role: GlyphRowRole, faces: &[u32]) -> GlyphRow {
    let mut row = GlyphRow::new(role);
    for (index, face) in faces.iter().enumerate() {
        row.glyphs[GlyphArea::Text.index()].push(Glyph::char('x', FaceId::new(*face), index));
    }
    row
}

#[test]
fn direct_gather_keeps_zero_row_boundary_pointer_and_fringe_dependencies() {
    let mut body = row(GlyphRowRole::Text, &[0, 7, 7, 0, 7, 8, 8, 7]);
    for (index, id) in [7, 9, 7].into_iter().enumerate() {
        body.intern_pointer_appearance(GlyphPointerAppearance {
            source: GlyphPointerSourceIdentity {
                kind: GlyphPointerSourceKind::Buffer,
                source_id: 1,
                range_start: index as u64,
                range_end: index as u64 + 1,
                property_owner: 1,
                occurrence: GlyphPointerOccurrenceIdentity::Source,
            },
            face_id: FaceId::new(id),
        })
        .unwrap();
    }
    body.left_fringe_bitmap = Some(FringeBitmapInfo {
        bitmap_index: 1,
        face_id: FaceId::new(10),
    });
    body.right_fringe_bitmap = Some(FringeBitmapInfo {
        bitmap_index: 2,
        face_id: FaceId::new(7),
    });
    body.overlay_arrow_bitmap = Some(FringeBitmapInfo {
        bitmap_index: 3,
        face_id: FaceId::new(11),
    });
    // The previous body's final dependency is 11. Reset row-local state before
    // this row, and retain its repeated starting dependency in the final union.
    let next = row(GlyphRowRole::Text, &[11, 11, 12, 0, 12]);
    let mut chrome = row(GlyphRowRole::ModeLine, &[]);
    chrome.left_fringe_bitmap = Some(FringeBitmapInfo {
        bitmap_index: 4,
        face_id: FaceId::new(13),
    });
    chrome.overlay_arrow_bitmap = Some(FringeBitmapInfo {
        bitmap_index: 5,
        face_id: FaceId::new(14),
    });
    let rows = [&body, &next, &chrome];
    let expected: std::collections::BTreeSet<_> = rows
        .into_iter()
        .flat_map(GlyphRow::referenced_face_ids)
        .filter(|id| *id != FaceId::new(0))
        .chain([FaceId::new(15)])
        .collect();
    let mut gathered = std::collections::BTreeSet::from([FaceId::new(15)]);
    extend_referenced_face_ids(rows, &mut gathered);
    assert_eq!(
        gathered, expected,
        "all source categories must keep complete dependencies"
    );
    assert_eq!(
        gathered.into_iter().map(FaceId::get).collect::<Vec<_>>(),
        vec![7, 8, 9, 10, 11, 12, 13, 14, 15]
    );
}
