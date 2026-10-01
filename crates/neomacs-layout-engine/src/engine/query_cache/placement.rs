//! Re-place a synchronous observation only while its complete row set stays
//! unchanged. This neither publishes a frame nor updates live window markers.
use neovm_core::{
    buffer::LispCharPos1,
    window::{WindowDisplaySnapshotFreshness, WindowLayoutQuery},
};

pub(super) fn reposition(
    query: &WindowLayoutQuery,
    current: &WindowDisplaySnapshotFreshness,
    source_point: LispCharPos1,
) -> Option<WindowLayoutQuery> {
    reposition_complete_rows(query, current, source_point, None)
}

/// A complete target prefix can retain its rows while unrelated rows enter or
/// leave the viewport below it. Start, point and source inputs still match.
pub(super) fn reposition_position(
    query: &WindowLayoutQuery,
    current: &WindowDisplaySnapshotFreshness,
    source_point: LispCharPos1,
    target: LispCharPos1,
) -> Option<WindowLayoutQuery> {
    reposition_complete_rows(query, current, source_point, Some(target))
}

fn reposition_complete_rows(
    query: &WindowLayoutQuery,
    current: &WindowDisplaySnapshotFreshness,
    source_point: LispCharPos1,
    target: Option<LispCharPos1>,
) -> Option<WindowLayoutQuery> {
    let snapshot = query.geometry()?;
    let dy = snapshot
        .layout_freshness
        .as_ref()?
        .query_vscroll_delta(current)?;
    snapshot.window_end_record?;
    if !snapshot.chrome_strings.is_empty() || !snapshot.regions_materialized {
        return None;
    }
    let body = snapshot.regions.text_body;
    let top_px = body.y - snapshot.regions.outer.y;
    if top_px.fract() != 0.0 || body.height.fract() != 0.0 || body.height <= 0.0 {
        return None;
    }
    let top = top_px as i64;
    let bottom = top.checked_add(body.height as i64)?;
    let first = snapshot.rows.first()?;
    let last = snapshot.rows.last()?;
    if let Some(target) = target
        && snapshot.point_for_buffer_pos(target)?.row != last.row
    {
        // Missing targets and long strings that keep emitting rows at the
        // same source anchor require the canonical source walker.
        return None;
    }
    // Keep the first row at the top edge and the final row intersecting the
    // viewport. Full viewport observations must also cover the bottom edge;
    // a target prefix already completed its own final row at the source stop.
    for offset in [0, dy] {
        if first.y.checked_add(offset)? > top
            || first.y.checked_add(first.height)?.checked_add(offset)? <= top
            || last.y.checked_add(offset)? >= bottom
            || last.y.checked_add(last.height)?.checked_add(offset)? <= top
            || (target.is_none() && last.y.checked_add(last.height)?.checked_add(offset)? < bottom)
        {
            return None;
        }
    }
    if snapshot.rows.iter().any(|row| {
        row.height <= 0 || row.start_buffer_pos.is_none() || row.end_buffer_pos.is_none()
    }) || snapshot
        .rows
        .windows(2)
        .any(|rows| rows[0].y.checked_add(rows[0].height) != Some(rows[1].y))
    {
        return None;
    }
    // An unchanged row set keeps an absent cursor absent. Visible cursors
    // still need the clipping checks before their coordinates can translate.
    if snapshot.logical_cursor.is_some() || snapshot.phys_cursor.is_some() {
        // Cursor clipping is not a translation. Only re-place a cursor whose
        // source row is fully visible before and after the move.
        let point = snapshot.point_for_buffer_pos(source_point)?;
        let point_row = snapshot.rows.iter().find(|row| row.row == point.row)?;
        for offset in [0, dy] {
            if point_row.y.checked_add(offset)? < top
                || point_row
                    .y
                    .checked_add(point_row.height)?
                    .checked_add(offset)?
                    > bottom
            {
                return None;
            }
            if let Some(cursor) = &snapshot.phys_cursor
                && (cursor.y.checked_add(offset)? < 0
                    || cursor.y.checked_add(cursor.height)?.checked_add(offset)? > bottom - top)
            {
                return None;
            }
        }
    }
    let mut placed = snapshot.clone();
    for row in &mut placed.rows {
        row.y = row.y.checked_add(dy)?;
    }
    for point in &mut placed.points {
        point.y = point.y.checked_add(dy)?;
    }
    for row in &mut placed.body_rows {
        row.body_y = row.body_y.checked_add(dy)?;
    }
    if let Some(cursor) = &mut placed.logical_cursor {
        cursor.y = cursor.y.checked_add(dy)?;
    }
    if let Some(cursor) = &mut placed.phys_cursor {
        cursor.y = cursor.y.checked_add(dy)?;
    }
    placed.layout_freshness = Some(current.clone());
    Some(WindowLayoutQuery::new(query.end(), Some(placed)))
}

/// An explicit row/pixel query already chooses its own start and zero vscroll.
/// Moving the live viewport cannot move these pixels; point and every source
/// dependency still have to match. The owner rejects fontification callbacks.
pub(super) fn absolute_rows(
    query: &WindowLayoutQuery,
    current: &WindowDisplaySnapshotFreshness,
) -> Option<WindowLayoutQuery> {
    let snapshot = query.geometry()?;
    if !snapshot
        .layout_freshness
        .as_ref()?
        .same_query_row_content(current)
        || !snapshot.chrome_strings.is_empty()
    {
        return None;
    }
    let mut snapshot = snapshot.clone();
    snapshot.layout_freshness = Some(current.clone());
    Some(WindowLayoutQuery::new(query.end(), Some(snapshot)))
}
