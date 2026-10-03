//! GNU resize_mini_window's final move_it_by_lines(0) aligns an EOB overlay
//! continuation to its source screen line. This consumes immutable numeric row
//! provenance from one exclusive layout attempt; it caches no Lisp state.
use neovm_core::buffer::LispCharPos1;
use neovm_core::window::{DisplayRowEndSource, WindowDisplaySnapshot};

#[cold]
#[inline(never)]
pub(super) fn aligned_after_string_start(
    snapshot: &WindowDisplaySnapshot,
    target_y: i64,
    end: LispCharPos1,
) -> Option<LispCharPos1> {
    let (index, row) = snapshot
        .rows
        .iter()
        .enumerate()
        .rev()
        .find(|(_, row)| row.y <= target_y)?;
    let anchor = row.start_buffer_pos?;
    // A buffer source position cannot resume inside an after-string. GNU's
    // extra zero-line motion specifically repairs this EOB case (xdisp.c:13383
    // through 13396). A real buffer/wrapped row retains its exact start.
    if anchor == end
        && row.end_buffer_pos == Some(anchor)
        && row.end_source == DisplayRowEndSource::OverlayAfterString
    {
        if let Some(source_row_start) = snapshot.rows[..=index].iter().rev().find_map(|source| {
            let start = source.start_buffer_pos?;
            let finish = source.end_buffer_pos?;
            (start < anchor && anchor <= finish).then_some(start)
        }) {
            return Some(source_row_start);
        }
    }
    Some(anchor)
}

#[cfg(test)]
#[path = "tests/mini_source_start.rs"]
mod tests;
