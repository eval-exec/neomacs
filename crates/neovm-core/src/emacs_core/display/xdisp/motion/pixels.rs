//! Pixel extents preceding a source anchor, measured by redisplay's row walk.

use super::*;
use std::num::NonZeroUsize;

pub(crate) struct BackwardPixelExtent {
    pub start: LispCharPos1,
    pub width: i64,
    pub height: i64,
}

/// The precision-scroll caller excludes the row at `origin`. A negative pixel
/// offset selects the row covering that displacement, then measures all whole
/// rows up to the anchor. Neither the selected row nor its height can be
/// inferred from the frame's default font, even for a one-pixel displacement.
pub(crate) fn backward_extent(
    eval: &mut Context,
    frame: FrameId,
    window: WindowId,
    buffer: BufferId,
    origin: LispCharPos1,
    offset: i64,
) -> Result<Option<BackwardPixelExtent>, Flow> {
    debug_assert!(offset < 0);
    let Some(source) = eval.buffers.get(buffer) else {
        return Ok(None);
    };
    let mut start = measurement::backtrack(source, origin, 1);
    // Start near the requested anchor. Wrapped lines and larger pixel
    // displacements expand through the same canonical row query below.
    let mut count = 4usize;
    loop {
        let Some(snapshot) = measurement::query(
            eval,
            frame,
            window,
            buffer,
            start,
            NonZeroUsize::new(count).unwrap(),
        )?
        else {
            return Ok(None);
        };
        let accessible = eval
            .buffers
            .get(buffer)
            .ok_or_else(|| {
                signal(
                    LispCondition::Error,
                    vec![Value::string("Pixel measurement buffer was deleted")],
                )
            })?
            .accessible_char_region();
        let rows = snapshot_text_rows(&snapshot);
        // A continuation boundary can also be the preceding row's end. The
        // row starting at the anchor owns its y, not the preceding row.
        let anchor = rows
            .iter()
            .position(|row| row.start_buffer_pos == Some(origin))
            .or_else(|| snapshot_row_index_for_pos(&rows, origin));
        if let Some(anchor) = anchor {
            let goal = rows[anchor].y.saturating_add(offset);
            if goal >= rows[0].y || start == accessible.start_lisp() {
                let target = rows[..=anchor]
                    .iter()
                    .rposition(|row| row.y <= goal)
                    .unwrap_or(0);
                return Ok(Some(BackwardPixelExtent {
                    start: rows[target].start_buffer_pos.expect("source row"),
                    height: rows[anchor].y.saturating_sub(rows[target].y),
                    width: rows[target..anchor]
                        .iter()
                        .map(|row| row.end_x.saturating_sub(row.start_x).max(0))
                        .max()
                        .unwrap_or(0),
                }));
            }
            start = measurement::backtrack(
                eval.buffers.get(buffer).expect("validated buffer"),
                start,
                count,
            );
        } else if rows.is_empty()
            || rows.last().and_then(|row| row.end_buffer_pos) == Some(accessible.end_lisp())
        {
            return Err(signal(
                LispCondition::Error,
                vec![Value::string(
                    "Pixel measurement origin is outside measured coverage",
                )],
            ));
        }
        count = count.checked_mul(2).ok_or_else(|| {
            signal(
                LispCondition::Error,
                vec![Value::string(
                    "Pixel measurement exceeds the row address space",
                )],
            )
        })?;
    }
}
