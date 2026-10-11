//! Displayed text extents through the canonical offscreen row producer.
use super::*;
use crate::window::{WindowDisplaySnapshot, WindowLayoutQueryOutcome, WindowLayoutQueryScope};

/// Named measurement inputs prevent mixing source bounds and fallback units.
#[derive(Clone, Copy)]
pub(super) struct TextMeasurement {
    pub frame: FrameId,
    pub window: crate::window::WindowId,
    pub buffer: BufferId,
    pub range: MeasuredRange,
    pub trim: bool,
    pub cell: TextCellPixels,
    pub columns: CharColumnWidth,
    pub edge: Option<RowEdge>,
    pub y_limit: Option<f32>,
}

impl TextMeasurement {
    fn fallback(self, eval: &super::super::eval::Context) -> RegionTextMetrics {
        region_text_metrics_with_display(
            eval,
            self.frame,
            self.buffer,
            self.range,
            self.trim,
            self.cell,
            self.columns,
            self.edge,
            self.y_limit,
        )
    }

    /// Only an absent frontend permits the startup/batch cell approximation.
    pub(super) fn measure(
        self,
        eval: &mut super::super::eval::Context,
    ) -> Result<RegionTextMetrics, Flow> {
        let Self {
            frame,
            window,
            buffer,
            range,
            trim,
            edge,
            y_limit,
            ..
        } = self;
        if y_limit == Some(0.0) {
            return Ok(RegionTextMetrics::EMPTY);
        }
        let source = eval
            .buffers
            .get(buffer)
            .expect("resolved measurement buffer");
        let end = if trim {
            let mut bytes = Vec::new();
            source.copy_emacs_byte_range_to(EmacsByteRange::new(range.from, range.to), &mut bytes);
            EmacsBytePos::new(
                range.from.get() + trim_window_text_to_non_empty_line_end(&bytes).len(),
            )
        } else {
            range.to
        };
        let start = source.emacs_byte_pos_to_lisp_char_pos(range.line_start);
        let from = source.emacs_byte_pos_to_lisp_char_pos(range.from);
        let end = source.emacs_byte_pos_to_lisp_char_pos(end);
        let width = edge.map(|edge| edge.x.ceil().max(0.0) as usize);
        // The vertical budget begins at FROM's display row, after its wrapped
        // prefix. Include its first element so a wrap boundary is not mistaken
        // for the preceding row's artificial end-of-source insertion slot.
        let prefix_height = if y_limit.is_some() && from > start {
            let prefix = eval.query_window_layout_scope(
                frame,
                window,
                WindowLayoutQueryScope::TextExtent {
                    buffer,
                    start,
                    end: crate::buffer::LispCharPos1::new(from.as_i64().saturating_add(1)).min(end),
                    width,
                    height: None,
                },
            );
            let Some(snapshot) = query_geometry(prefix)? else {
                return Ok(self.fallback(eval));
            };
            snapshot
                .point_for_buffer_pos(from)
                .and_then(|origin| Some(origin.y.saturating_sub(snapshot.rows.first()?.y).max(0)))
                .unwrap_or(0)
        } else {
            0
        };
        let query = eval.query_window_layout_scope(
            frame,
            window,
            WindowLayoutQueryScope::TextExtent {
                buffer,
                start,
                end,
                width,
                height: y_limit.and_then(|limit| {
                    std::num::NonZeroUsize::new(
                        (limit.ceil().max(1.0) as usize).saturating_add(prefix_height as usize),
                    )
                }),
            },
        );
        let Some(snapshot) = query_geometry(query)? else {
            return Ok(self.fallback(eval));
        };
        let origin = snapshot.point_for_buffer_pos(from);
        let origin_row = origin.as_ref().map_or(0, |point| point.row);
        let origin_x = origin.as_ref().map_or(0, |point| point.x);
        let source = eval.buffers.get(buffer).ok_or_else(|| {
            signal(
                "error",
                vec![Value::string("Measurement buffer was deleted")],
            )
        })?;
        let origin_is_newline = source
            .char_code_after_emacs_byte_pos(source.lisp_pos_to_emacs_byte_pos(from))
            == Some('\n' as u32);
        let mut rows = 0;
        let mut width = 0i64;
        let mut height = 0i64;
        for row in snapshot.rows.iter().filter(|row| row.row >= origin_row) {
            // A trailing newline does not contribute the empty EOB row.
            if row.start_buffer_pos == Some(end)
                && end > from
                && source.char_before_emacs_byte_pos(source.lisp_pos_to_emacs_byte_pos(end))
                    == Some('\n')
            {
                break;
            }
            rows += 1;
            height += row.height;
            // Source bounds constrain shaping before production. The row's
            // advance includes overlay strings, without counting insertion slots.
            // GNU resets current_x at a newline FROM, while preserving the
            // row's vertical extent and any display strings at that anchor.
            let row_width = if origin_is_newline && row.row == origin_row {
                row.end_x.saturating_sub(origin_x)
            } else {
                row.end_x
            };
            width = width.max(row_width);
        }
        if rows <= 1 && !(origin_is_newline && end > from) {
            width = width.saturating_sub(origin_x);
        }
        if let Some(edge) = edge {
            width = width.min(edge.x.ceil() as i64);
        }
        let height = y_limit.map_or(height as f32, |limit| (height as f32).min(limit));
        Ok(RegionTextMetrics {
            lines: rows,
            max_width: width as f32,
            height,
        })
    }
}

/// Adapter absence is distinct from a failed or reentrant canonical query.
fn query_geometry(
    outcome: WindowLayoutQueryOutcome,
) -> Result<Option<WindowDisplaySnapshot>, Flow> {
    match outcome {
        WindowLayoutQueryOutcome::Unavailable => Ok(None),
        WindowLayoutQueryOutcome::LayoutBusy => Err(signal(
            "error",
            vec![Value::string(
                "Text measurement reentered an active layout query",
            )],
        )),
        WindowLayoutQueryOutcome::Failed(error) => {
            Err(signal("error", vec![Value::string(error.message())]))
        }
        WindowLayoutQueryOutcome::Ready(query) => {
            query.into_geometry().map(Some).ok_or_else(|| {
                signal(
                    "error",
                    vec![Value::string("Text measurement produced no geometry")],
                )
            })
        }
    }
}
