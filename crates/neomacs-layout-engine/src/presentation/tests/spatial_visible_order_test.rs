//! Visible lazy hits preserve the eager source order for ambiguous descriptors.

use super::*;
use neomacs_display_protocol::frame_glyphs::WindowInfo;
use neomacs_display_protocol::{PresentationId, PresentedHitQuery};
use neovm_core::buffer::LispCharPos1;
use neovm_core::window::{
    DisplayPointRole, DisplayPointRow, DisplayPointRows, DisplayPointSnapshot, DisplayRowSnapshot,
    PresentedBodyRowSnapshot, WindowId,
};

fn point(pos: i64, row: i64, column: i64, x: i64, height: i64) -> DisplayPointSnapshot {
    DisplayPointSnapshot {
        buffer_pos: LispCharPos1::new(pos),
        role: DisplayPointRole::Glyph,
        row,
        col: column,
        x,
        y: row * 10,
        width: 1,
        height,
    }
}

fn descriptor(point: DisplayPointSnapshot) -> DisplayPointRow {
    DisplayPointRows::from_points(vec![point]).rows.remove(0)
}

fn indices(
    descriptors: Vec<DisplayPointRow>,
    body_rows: Vec<PresentedBodyRowSnapshot>,
    rows: Vec<DisplayRowSnapshot>,
    body: neomacs_display_protocol::Rect,
) -> (PresentedHitIndex, PresentedHitIndex) {
    assert!(
        neovm_core::window::display_point_rows_mode().enabled(),
        "the test must exercise visible row-lazy lookup"
    );
    let window = DisplayWindowId::new(1);
    let point_rows = DisplayPointRows { rows: descriptors };
    let snapshot = WindowDisplaySnapshot {
        window_id: WindowId(1),
        points: point_rows.iter_points().collect(),
        point_rows: Some(point_rows),
        rows,
        body_rows,
        regions_materialized: true,
        regions: PresentedWindowRegions {
            outer: body,
            text_body: body,
            ..PresentedWindowRegions::default()
        },
        ..WindowDisplaySnapshot::default()
    };
    // Use the ambiguous snapshot's canonical iterator, not its legacy points
    // field: equal source endpoints retain descriptor-vector precedence.
    let expected = body_text_positions(window, &snapshot, body).unwrap();
    let mut state = FrameDisplayState::new(10, 20, 1.0, 1.0);
    state.presentation_id = PresentationId::new(7);
    state.window_infos.push(WindowInfo {
        window_id: window,
        buffer_id: 1,
        window_start: 1,
        window_end: 1000,
        buffer_size: 1000,
        buffer_modiff: neomacs_display_protocol::presentation_origin::BufferModiff::new(1),
        bounds: body,
        geometry: PresentedWindowGeometry::default(),
        line_number_field: None,
        mode_line_height: 0.0,
        header_line_height: 0.0,
        tab_line_height: 0.0,
        selected: true,
        is_minibuffer: false,
        char_height: 1.0,
        buffer_name: "visible-row-order".to_owned(),
        buffer_file_name: String::new(),
        modified: false,
    });
    PresentationSpatialPlan::compile(
        &state,
        &[WindowPresentationSnapshot::live(snapshot)],
        PresentedResizeEdge::Trailing,
    )
    .expect("compile the real visible spatial plan")
    .seal(&mut state)
    .expect("seal visible interaction geometry");
    let lazy = state.presented_hit_index;
    assert!(
        lazy.text_deferred(),
        "composition must select the lazy source"
    );
    let eager =
        PresentedHitIndex::from_parts(lazy.presentation(), lazy.regions().to_vec(), expected)
            .unwrap();
    (lazy, eager)
}

fn query(index: &PresentedHitIndex, x: f32, y: f32) -> Option<PresentedTextPosition> {
    let point = neomacs_display_protocol::InteractionProjection::settled(index.presentation())
        .map(
            neomacs_display_protocol::GeometryPoint::<
                neomacs_display_protocol::RootSurfaceSpace,
                neomacs_display_protocol::LogicalPixels,
            >::from_px(x, y)
            .unwrap(),
        )
        .unwrap();
    index
        .resolve(PresentedHitQuery::new(point))
        .unwrap()
        .and_then(|hit| hit.text_position())
}

fn body_row(row: i64) -> PresentedBodyRowSnapshot {
    PresentedBodyRowSnapshot {
        output_row: row,
        body_row: row,
        body_y: row * 10,
    }
}

#[test]
fn visible_duplicate_rows_preserve_eager_column_precedence() {
    let (lazy, eager) = indices(
        vec![
            descriptor(point(1, 0, 20, 0, 1)),
            descriptor(point(1, 0, 10, 0, 1)),
        ],
        vec![body_row(0)],
        Vec::new(),
        neomacs_display_protocol::Rect::new(0.0, 0.0, 10.0, 1.0),
    );
    let expected = query(&eager, 0.5, 0.5).unwrap();
    assert_eq!(expected.column(), 20, "first descriptor wins eagerly");
    assert_eq!(query(&lazy, 0.5, 0.5), Some(expected));
    assert_eq!(lazy.text_positions(), eager.text_positions());
}

#[test]
fn visible_unordered_rows_preserve_eager_overlap_precedence() {
    let (lazy, eager) = indices(
        vec![
            descriptor(point(1, 1, 1, 0, 1)),
            descriptor(point(1, 0, 1, 0, 20)),
        ],
        vec![body_row(0), body_row(1)],
        Vec::new(),
        neomacs_display_protocol::Rect::new(0.0, 0.0, 10.0, 20.0),
    );
    let expected = query(&eager, 0.5, 10.5).unwrap();
    assert_eq!(expected.row(), 1, "first descriptor wins eagerly");
    assert_eq!(query(&lazy, 0.5, 10.5), Some(expected));
    assert_eq!(lazy.text_positions(), eager.text_positions());
}

#[test]
fn visible_unordered_rows_preserve_eager_fallback_position() {
    let rows = [0, 1]
        .map(|row| DisplayRowSnapshot {
            row,
            y: row * 10,
            height: 1,
            start_buffer_pos: Some(LispCharPos1::new(100 * (row + 1))),
            end_buffer_pos: Some(LispCharPos1::new(100 * (row + 1) + 1)),
            ..DisplayRowSnapshot::default()
        })
        .to_vec();
    let (lazy, eager) = indices(
        vec![
            descriptor(point(20, 1, 20, 5, 1)),
            descriptor(point(1, 0, 1, 5, 1)),
        ],
        vec![body_row(0), body_row(1)],
        rows,
        neomacs_display_protocol::Rect::new(0.0, 0.0, 10.0, 11.0),
    );
    let expected = query(&eager, 8.5, 10.5).unwrap();
    assert_eq!(expected.buffer_position(), 20);
    assert_eq!(query(&lazy, 8.5, 10.5), Some(expected));
    assert_eq!(lazy.text_positions(), eager.text_positions());
}
