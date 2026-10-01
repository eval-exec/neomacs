//! A producer-certified source-line restart can reuse a complete pixel-query
//! suffix. These measurements own neither live markers nor presentation rights.

use std::num::NonZeroUsize;

use neovm_core::{
    buffer::LispCharPos1,
    window::{MatrixRow0, WindowDisplaySnapshotFreshness, WindowLayoutQuery},
};

pub(super) fn pixels_suffix(
    query: &WindowLayoutQuery,
    current: &WindowDisplaySnapshotFreshness,
    source_point: LispCharPos1,
    start: LispCharPos1,
    height: NonZeroUsize,
    restart_rows: &[(LispCharPos1, i64)],
) -> Option<WindowLayoutQuery> {
    let snapshot = query.geometry()?;
    if !snapshot
        .layout_freshness
        .as_ref()?
        .same_query_row_content(current)
        || !snapshot.regions_materialized
        || !snapshot.chrome_strings.is_empty()
    {
        return None;
    }
    // A retained row boundary is not itself a source restart certificate:
    // wraps and pushed strings can share its anchor. Only the producer knows
    // that it consumed an ordinary newline with no deferred source state.
    let first = snapshot.rows.first()?;
    let offset = snapshot.rows.iter().position(|row| {
        row.start_buffer_pos == Some(start)
            && restart_rows
                .iter()
                .any(|(anchor, index)| *anchor == start && *index == row.row)
    })?;
    if offset == 0 {
        return None;
    }
    let suffix = &snapshot.rows[offset..];
    let selected = suffix.first()?;
    let last = suffix.last()?;
    let row_delta = selected.row.checked_sub(first.row)?;
    let y_delta = selected.y.checked_sub(first.y)?;
    if row_delta <= 0
        || y_delta <= 0
        || suffix.iter().any(|row| {
            row.height <= 0 || row.start_buffer_pos.is_none() || row.end_buffer_pos.is_none()
        })
        || suffix.windows(2).any(|rows| {
            rows[0].y.checked_add(rows[0].height) != Some(rows[1].y)
                || rows[0].row.checked_add(1) != Some(rows[1].row)
        })
        || last.y.checked_add(last.height)?.checked_sub(selected.y)?
            < i64::try_from(height.get()).ok()?
    {
        return None;
    }
    let record = snapshot.window_end_record?;
    // Pixel walks can stage an incomplete extra string row before observing
    // their stop. Its metadata must not masquerade as the complete suffix end.
    if i64::try_from(record.matrix_row().get()).ok()? != last.row {
        return None;
    }
    // Cursor clipping depends on the viewport, rather than on translation
    // alone. The initial restart path admits only point outside the complete
    // retained source extent; such a canonical walk emits neither cursor.
    if (source_point >= start && source_point <= query.end())
        || snapshot
            .logical_cursor
            .is_some_and(|cursor| cursor.row >= selected.row)
        || snapshot
            .phys_cursor
            .as_ref()
            .is_some_and(|cursor| cursor.row >= selected.row)
        || snapshot
            .points
            .iter()
            .any(|point| point.row >= selected.row && point.buffer_pos < start)
    {
        return None;
    }
    let mut placed = snapshot.clone();
    placed.rows.drain(..offset);
    for row in &mut placed.rows {
        row.row = row.row.checked_sub(row_delta)?;
        row.y = row.y.checked_sub(y_delta)?;
    }
    placed.points.retain(|point| point.row >= selected.row);
    for point in &mut placed.points {
        point.row = point.row.checked_sub(row_delta)?;
        point.y = point.y.checked_sub(y_delta)?;
    }
    placed
        .body_rows
        .retain(|row| row.output_row >= selected.row);
    for row in &mut placed.body_rows {
        row.output_row = row.output_row.checked_sub(row_delta)?;
        row.body_row = row.body_row.checked_sub(row_delta)?;
        row.body_y = row.body_y.checked_sub(y_delta)?;
    }
    placed.logical_cursor = None;
    placed.phys_cursor = None;
    placed.window_end_record = Some(
        record.with_matrix_row(MatrixRow0::new(
            record
                .matrix_row()
                .get()
                .checked_sub(usize::try_from(row_delta).ok()?)?,
        )),
    );
    placed.layout_freshness = Some(current.clone());
    tracing::trace!(target: "neomacs_layout_engine::query_cache", ?start,
        dropped_rows = offset, retained_rows = placed.rows.len(),
        "reused certified pixel-query suffix");
    // Pixels may answer with a larger certified observation. Keeping the
    // complete suffix preserves the exact final char/byte offsets from the
    // original walk, instead of inferring a new end from visible positions.
    Some(WindowLayoutQuery::new(query.end(), Some(placed)))
}
